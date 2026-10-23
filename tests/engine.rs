use std::sync::Mutex;

use altopir::engine::{self, Method};
use altopir::packing::{PackParams, PackingKeys};
use altopir::service::{self, synthetic, Database, Prepared, SetupTimes};
use spiral_rs::client::Client;

static SERIAL: Mutex<()> = Mutex::new(());

const ROWS: usize = 2048;
const RECORD_BYTES: usize = 4096;

fn threads() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(16)
}

fn methods() -> Vec<Method> {
    let mut m = Vec::new();
    if engine::amx_available() {
        m.push(Method::Amx);
    }
    if engine::vnni_available() {
        m.push(Method::Vnni);
    }
    assert!(!m.is_empty(), "the engine needs AMX or AVX-512 VNNI");
    m
}

fn mutate(valid: &[u8], rnd: &mut impl FnMut() -> u64, q: u64, count: usize) -> Vec<u8> {
    let mut v = valid.to_vec();
    let special = [0u32, 1, (q - 1) as u32, q as u32, (q + 1) as u32, u32::MAX, 1 << 31];
    match rnd() % 8 {
        0 => {
            let n = (rnd() as usize) % v.len();
            v.truncate(n);
        }
        1 => {
            for _ in 0..1 + rnd() % 64 {
                v.push(rnd() as u8);
            }
        }
        2 => {
            let i = (rnd() as usize) % 13;
            v[i] = rnd() as u8;
        }
        3 => {
            let i = (rnd() as usize) % 13;
            v[i] ^= 1 << (rnd() % 8);
        }
        4 => {
            let w = 9 + 4 * ((rnd() as usize) % (2 * count));
            let x = special[(rnd() as usize) % special.len()];
            v[w..w + 4].copy_from_slice(&x.to_le_bytes());
        }
        5 => {
            let nq = (v.len() - 9 - 8 * count) / 4;
            let w = 9 + 8 * count + 4 * ((rnd() as usize) % nq);
            let x = special[(rnd() as usize) % special.len()];
            v[w..w + 4].copy_from_slice(&x.to_le_bytes());
        }
        6 => {
            let i = (rnd() as usize) % v.len();
            v[i] ^= 1 << (rnd() % 8);
        }
        _ => {}
    }
    v
}

#[test]
fn responses_match_the_reference_server_and_decrypt_to_the_records() {
    let _guard = SERIAL.lock().unwrap();
    let params = service::params(ROWS, RECORD_BYTES);
    let db = Database::synthetic(ROWS, RECORD_BYTES);
    let mut times = SetupTimes::default();
    let server = service::build_server(&params, &db, threads(), &mut times);
    let prep = Prepared::new(&params, &server, ROWS, threads(), &mut times);
    let mut engine = prep.engine(&params, &server, threads());
    let rl = engine.response_len();
    assert_eq!(rl, service::response_len(&params));
    assert_eq!(engine.request_len(), service::request_len(&params, ROWS));

    let pack = PackParams::new(&params, params.poly_len);
    let indices = [0usize, 1, 977, ROWS / 2, ROWS - 1];
    let (clients, reqs): (Vec<Client>, Vec<Vec<u8>>) =
        indices.iter().map(|&i| service::new_request(&params, &pack, ROWS, i)).unzip();

    let parsed: Vec<service::Request> = reqs.iter().map(|r| service::decode_request(&params, r, ROWS).unwrap()).collect();
    let owned: Vec<Vec<u64>> = parsed.iter().map(|p| p.query.clone()).collect();
    let queries: Vec<&[u64]> = owned.iter().map(|q| q.as_slice()).collect();
    let mut keys: Vec<PackingKeys> = parsed.into_iter().map(|p| p.key).collect();
    let reference: Vec<Vec<u8>> = server
        .perform_online_computation_simplepir_word_cpu(&queries, &prep.offline, &mut keys, None)
        .into_iter()
        .map(|parts| parts.concat())
        .collect();

    for method in methods() {
        let fr: Vec<&[u8]> = reqs.iter().map(|v| v.as_slice()).collect();
        let mut arena = vec![0u8; fr.len() * rl];
        let mut status = vec![-1i32; fr.len()];
        let mut outs: Vec<&mut [u8]> = arena.chunks_exact_mut(rl).collect();
        let (answered, _) = engine.answer(&fr, &mut outs, &mut status, method);
        assert_eq!(answered, fr.len());
        assert!(status.iter().all(|&s| s == 0));
        for (i, &index) in indices.iter().enumerate() {
            let got = &arena[i * rl..(i + 1) * rl];
            assert!(got == reference[i].as_slice(), "{:?}: response {} differs from the reference server", method, i);
            let record = service::decrypt_response(&params, &clients[i], got).unwrap();
            assert!(record == synthetic::record(index, RECORD_BYTES), "{:?}: record {} differs", method, index);
        }
    }
}

#[test]
fn malformed_requests_are_rejected_like_the_decoder_rejects_them() {
    let _guard = SERIAL.lock().unwrap();
    let params = service::params(ROWS, RECORD_BYTES);
    let db = Database::synthetic(ROWS, RECORD_BYTES);
    let mut times = SetupTimes::default();
    let server = service::build_server(&params, &db, threads(), &mut times);
    let prep = Prepared::new(&params, &server, ROWS, threads(), &mut times);
    let mut engine = prep.engine(&params, &server, threads());
    let rl = engine.response_len();
    let pack = PackParams::new(&params, params.poly_len);
    let count = params.t_exp_left * params.poly_len;

    let (c0, r0) = service::new_request(&params, &pack, ROWS, 5);
    let (c1, r1) = service::new_request(&params, &pack, ROWS, 1500);
    let mut frames = vec![r0.clone(), r0[..r0.len() - 1].to_vec()];
    let mut v = r0.clone();
    v.push(0);
    frames.push(v);
    let mut v = r0.clone();
    v[0] = 1;
    frames.push(v);
    let mut v = r0.clone();
    v[1..5].copy_from_slice(&u32::MAX.to_le_bytes());
    frames.push(v);
    let mut v = r0.clone();
    v[9..13].copy_from_slice(&(params.modulus as u32).to_le_bytes());
    frames.push(v);
    let mut v = r0.clone();
    let off = 9 + 8 * count - 4;
    v[off..off + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    frames.push(v);
    frames.push(r1.clone());
    let want = [0, 1, 1, 2, 2, 3, 3, 0];
    for (f, &w) in frames.iter().zip(&want) {
        assert_eq!(service::decode_request(&params, f, ROWS).is_ok(), w == 0);
    }

    for method in methods() {
        let fr: Vec<&[u8]> = frames.iter().map(|v| v.as_slice()).collect();
        let mut arena = vec![0xa5u8; fr.len() * rl];
        let mut status = vec![-1i32; fr.len()];
        let answered = {
            let mut outs: Vec<&mut [u8]> = arena.chunks_exact_mut(rl).collect();
            engine.answer(&fr, &mut outs, &mut status, method).0
        };
        assert_eq!(answered, 2);
        assert_eq!(status, want);
        for c in 1..7 {
            assert!(arena[c * rl..(c + 1) * rl].iter().all(|&b| b == 0xa5), "a rejected buffer was written");
        }
        assert!(service::decrypt_response(&params, &c0, &arena[..rl]).unwrap() == synthetic::record(5, RECORD_BYTES));
        assert!(service::decrypt_response(&params, &c1, &arena[7 * rl..]).unwrap() == synthetic::record(1500, RECORD_BYTES));
    }

    let mut state = 0x5eed_f00du64;
    let mut rnd = || synthetic::mix(&mut state);
    let base: Vec<Vec<u8>> = (0..4).map(|i| service::new_request(&params, &pack, ROWS, 100 * i).1).collect();
    let mut accepted = 0;
    for _ in 0..8 {
        let frames: Vec<Vec<u8>> = (0..64)
            .map(|_| {
                let k = (rnd() % 4) as usize;
                mutate(&base[k], &mut rnd, params.modulus, count)
            })
            .collect();
        let fr: Vec<&[u8]> = frames.iter().map(|v| v.as_slice()).collect();
        let mut arena = vec![0u8; fr.len() * rl];
        let mut status = vec![-1i32; fr.len()];
        {
            let mut outs: Vec<&mut [u8]> = arena.chunks_exact_mut(rl).collect();
            engine.answer(&fr, &mut outs, &mut status, methods()[0]);
        }
        for (f, &s) in frames.iter().zip(&status) {
            assert_eq!(s == 0, service::decode_request(&params, f, ROWS).is_ok(), "engine and decoder disagree");
            accepted += (s == 0) as usize;
        }
    }
    assert!(accepted > 0 && accepted < 512);
}

const BLOCK_ROWS: usize = 8192;

#[test]
fn multi_block_and_chunked_batches_match_the_reference_server() {
    let _guard = SERIAL.lock().unwrap();
    let params = service::params(BLOCK_ROWS, RECORD_BYTES);
    let db = Database::synthetic(BLOCK_ROWS, RECORD_BYTES);
    let mut times = SetupTimes::default();
    let server = service::build_server(&params, &db, threads(), &mut times);
    let prep = Prepared::new(&params, &server, BLOCK_ROWS, threads(), &mut times);
    let mut engine = prep.engine(&params, &server, threads());
    let rl = engine.response_len();
    let pack = PackParams::new(&params, params.poly_len);

    // 1 and 2 clients use the k-vector kernel of VNNI. With a 2 MB L2 cache, 72 clients give blocks of 3,584 records,
    // so the last block of the scan ends early, and 136 clients form two client chunks with four blocks each.
    for b in [1usize, 2, 72, 136] {
        let indices: Vec<usize> = (0..b).map(|i| (i * 7919 + 13) % BLOCK_ROWS).collect();
        let (clients, reqs): (Vec<Client>, Vec<Vec<u8>>) =
            indices.iter().map(|&i| service::new_request(&params, &pack, BLOCK_ROWS, i)).unzip();
        let parsed: Vec<service::Request> =
            reqs.iter().map(|r| service::decode_request(&params, r, BLOCK_ROWS).unwrap()).collect();
        let owned: Vec<Vec<u64>> = parsed.iter().map(|p| p.query.clone()).collect();
        let queries: Vec<&[u64]> = owned.iter().map(|q| q.as_slice()).collect();
        let mut keys: Vec<PackingKeys> = parsed.into_iter().map(|p| p.key).collect();
        let reference: Vec<Vec<u8>> = server
            .perform_online_computation_simplepir_word_cpu(&queries, &prep.offline, &mut keys, None)
            .into_iter()
            .map(|parts| parts.concat())
            .collect();

        for method in methods() {
            let fr: Vec<&[u8]> = reqs.iter().map(|v| v.as_slice()).collect();
            let mut arena = vec![0u8; b * rl];
            let mut status = vec![-1i32; b];
            let mut outs: Vec<&mut [u8]> = arena.chunks_exact_mut(rl).collect();
            let (answered, _) = engine.answer(&fr, &mut outs, &mut status, method);
            assert_eq!(answered, b);
            assert!(status.iter().all(|&s| s == 0));
            for (i, &index) in indices.iter().enumerate() {
                let got = &arena[i * rl..(i + 1) * rl];
                assert!(got == reference[i].as_slice(), "{:?}, batch {}: response {} differs from the reference server",
                        method, b, i);
                let record = service::decrypt_response(&params, &clients[i], got).unwrap();
                assert!(record == synthetic::record(index, RECORD_BYTES), "{:?}, batch {}: record {} differs", method, b,
                        index);
            }
        }
    }
}
