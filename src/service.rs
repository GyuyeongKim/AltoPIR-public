use std::time::Instant;

use spiral_rs::client::Client;
use spiral_rs::params::Params;
use spiral_rs::poly::{PolyMatrix, PolyMatrixNTT, PolyMatrixRaw};

use crate::client::{decrypt_ct_reg_measured, YClient};
use crate::engine::{self, Engine, HugeBytes, TiledDb};
use crate::modulus_switch::{switch_prefix_row0, ModulusSwitch};
use crate::packing::{uncondense_matrix, PackParams, PackingKeys, PackingType};
use crate::params::{params_for_sandwichpir, GetQPrime};
use crate::scheme::{generate_packing_keys, SEED_0};
use crate::server::{OfflinePrecomputedValues, YServer};

pub const MIN_RECORDS: usize = 2048;
pub const RECORD_UNIT: usize = 2048;
pub const DEFAULT_RECORD_BYTES: usize = 32768;

pub fn params(rows: usize, record_bytes: usize) -> Params {
    assert!(rows.is_power_of_two() && rows >= MIN_RECORDS,
            "the number of records must be a power of two and at least {}", MIN_RECORDS);
    assert!(record_bytes > 0 && record_bytes % RECORD_UNIT == 0,
            "the record size must be a positive multiple of {} bytes", RECORD_UNIT);
    let params = params_for_sandwichpir(rows, record_bytes * 8);
    assert_eq!(params.modulus, 4294955009);
    assert_eq!((params.poly_len, params.instances, params.t_exp_left, params.pt_modulus),
               (2048, record_bytes / 2048, 4, 256));
    params
}

pub fn record_bytes(params: &Params) -> usize {
    params.instances * params.poly_len
}

pub fn request_len(params: &Params, rows: usize) -> usize {
    9 + 4 * (2 * params.t_exp_left * params.poly_len + rows)
}

pub fn response_len(params: &Params) -> usize {
    params.instances * params.poly_len * 28 / 8
}

pub mod synthetic {
    pub const SEED: u64 = 0x416c746f504952;

    pub fn mix(state: &mut u64) -> u64 {
        *state = state.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = *state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }

    pub fn record(row: usize, bytes: usize) -> Vec<u8> {
        assert!(bytes % 2 == 0);
        let mut state = SEED ^ 0xd1b54a32d192ed03u64.wrapping_mul(row as u64 + 1);
        let mut out = Vec::with_capacity(bytes);
        for _ in 0..bytes / 2 {
            out.extend_from_slice(&((mix(&mut state) % 65535) as u16).to_le_bytes());
        }
        out
    }

    pub fn database(rows: usize, bytes: usize) -> Vec<u8> {
        let mut out = vec![0u8; rows * bytes];
        let chunk_rows = (rows + 63) / 64;
        std::thread::scope(|scope| {
            for (group, chunk) in out.chunks_mut(chunk_rows * bytes).enumerate() {
                scope.spawn(move || {
                    for (r, dst) in chunk.chunks_exact_mut(bytes).enumerate() {
                        dst.copy_from_slice(&record(group * chunk_rows + r, bytes));
                    }
                });
            }
        });
        out
    }
}

pub struct Database {
    pub rows: usize,
    pub records: usize,
    pub record_bytes: usize,
    pub flat: Vec<u8>,
}

impl Database {
    pub fn synthetic(rows: usize, record_bytes: usize) -> Self {
        Database { rows, records: rows, record_bytes, flat: synthetic::database(rows, record_bytes) }
    }

    pub fn from_file(path: &str, record_bytes: usize) -> std::io::Result<Self> {
        let data = std::fs::read(path)?;
        let records = ((data.len() + record_bytes - 1) / record_bytes).max(1);
        let rows = records.next_power_of_two().max(MIN_RECORDS);
        let mut flat = vec![0u8; rows * record_bytes];
        flat[..data.len()].copy_from_slice(&data);
        Ok(Database { rows, records, record_bytes, flat })
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SetupTimes {
    pub server_ms: f64,
    pub offline_ms: f64,
    pub tile_ms: f64,
    pub packing_ms: f64,
}

pub fn build_server<'a>(params: &'a Params, db: &Database, threads: usize, times: &mut SetupTimes) -> YServer<'a, u8> {
    assert_eq!(record_bytes(params), db.record_bytes);
    std::env::set_var("SW_OFFLINE_THREADS", threads.to_string());
    let t = Instant::now();
    let server = YServer::<u8>::new_from_flat_db(params, &db.flat, db.rows);
    times.server_ms = t.elapsed().as_secs_f64() * 1e3;
    server
}

pub struct Prepared<'s> {
    pub offline: OfflinePrecomputedValues<'s>,
    pub tiled: TiledDb,
    pub ub: HugeBytes,
    pub prefixes: Vec<Vec<u8>>,
}

impl<'s> Prepared<'s> {
    pub fn new<'a>(params: &'a Params, server: &'s YServer<'a, u8>, rows: usize, threads: usize,
                   times: &mut SetupTimes) -> Self {
        let rb = record_bytes(params);
        let t = Instant::now();
        let offline = server.perform_offline_precomputation_simplepir(None, PackingType::InspiRING);
        times.offline_ms = t.elapsed().as_secs_f64() * 1e3;
        let t = Instant::now();
        let tiled = TiledDb::new(server.db(), server.db_rows_padded(), rows, rb, threads);
        times.tile_ms = t.elapsed().as_secs_f64() * 1e3;
        if engine::amx_available() {
            engine::init_amx();
        }
        let t = Instant::now();
        let mut ub = HugeBytes::zeroed(4 * params.t_exp_left * params.poly_len * rb);
        {
            let pp = offline.packing_params.as_ref().unwrap();
            let pv = offline.precomp_inspir_vec.as_ref().unwrap();
            let tables: Vec<u32> = pp.tables.iter().flat_map(|t| t.iter().map(|&v| v as u32)).collect();
            let powers: Vec<u32> = pp.gen_pows.iter().map(|&v| v as u32).collect();
            let t: Vec<_> = pv.iter().map(|v| v.bold_t_condensed.as_slice()).collect();
            let tbar: Vec<_> = pv.iter().map(|v| v.bold_t_bar_condensed.as_slice()).collect();
            engine::build_packing_matrix(ub.as_i8_mut(), &t, &tbar, &tables, &powers, params.poly_len / 2 - 1,
                                         params.t_exp_left, params.poly_len, params.modulus, threads);
        }
        times.packing_ms = t.elapsed().as_secs_f64() * 1e3;
        let (q1, q2) = (params.get_q_prime_1(), params.get_q_prime_2());
        let prefixes = offline.precomp_inspir_vec.as_ref().unwrap().iter()
            .map(|p| switch_prefix_row0(params, p.a_hat.get_poly(0, 0), q1, q2, params.poly_len))
            .collect();
        Prepared { offline, tiled, ub, prefixes }
    }

    pub fn engine<'e, 'a>(&'e self, params: &'e Params, server: &'e YServer<'a, u8>, threads: usize) -> Engine<'e> {
        let db: &[u8] = unsafe { std::slice::from_raw_parts(server.db().as_ptr() as *const u8, server.db().len()) };
        let that: Vec<&[u64]> = self.offline.precomp_inspir_vec.as_ref().unwrap().iter()
            .map(|p| p.bold_t_hat_condensed.as_slice())
            .collect();
        let (q1, q2) = (params.get_q_prime_1(), params.get_q_prime_2());
        let q1_bits = (q1 as f64).log2().ceil() as usize;
        let row1_off = (q2 as f64).log2().ceil() as usize * params.poly_len;
        Engine::new(db, &self.tiled, server.db_rows_padded(), self.ub.as_i8(), params.poly_len, params.t_exp_left,
                    &self.prefixes, row1_off, q1, q1_bits, &that, params.get_ntt_inverse_table(0), params.modulus,
                    threads)
    }
}

pub struct Request<'a> {
    pub query: Vec<u64>,
    pub key: PackingKeys<'a>,
}

pub fn encode_request(params: &Params, key: &PackingKeys, query: &[u64], rows: usize) -> Vec<u8> {
    assert_eq!(params.crt_count, 1);
    assert_eq!(query.len(), rows);
    let y = key.y_body_condensed.as_ref().unwrap().as_slice();
    let z = key.z_body_condensed.as_ref().unwrap().as_slice();
    let count = params.t_exp_left * params.poly_len;
    assert!(y.len() == count && z.len() == count);
    let mut out = Vec::with_capacity(request_len(params, rows));
    out.push(0);
    out.extend_from_slice(&(count as u32).to_le_bytes());
    out.extend_from_slice(&(count as u32).to_le_bytes());
    for &v in y.iter().chain(z.iter()) {
        assert!(v < params.modulus);
        out.extend_from_slice(&(v as u32).to_le_bytes());
    }
    for &v in query {
        assert!(v <= u32::MAX as u64);
        out.extend_from_slice(&(v as u32).to_le_bytes());
    }
    out
}

pub fn decode_request<'a>(params: &'a Params, input: &[u8], rows: usize) -> Result<Request<'a>, String> {
    let count = params.t_exp_left * params.poly_len;
    if input.len() != request_len(params, rows) || input[0] != 0 {
        return Err("request flag or length".into());
    }
    let u32_at = |off: usize| u32::from_le_bytes(input[off..off + 4].try_into().unwrap());
    if u32_at(1) as usize != count || u32_at(5) as usize != count {
        return Err("packing key length".into());
    }
    let mut y = PolyMatrixNTT::zero(params, 1, params.t_exp_left);
    let mut z = PolyMatrixNTT::zero(params, 1, params.t_exp_left);
    for (i, out) in y.as_mut_slice().iter_mut().chain(z.as_mut_slice().iter_mut()).enumerate() {
        let v = u32_at(9 + 4 * i) as u64;
        if v >= params.modulus {
            return Err("packing key coefficient out of range".into());
        }
        *out = v;
    }
    let start = 9 + 8 * count;
    let query = (0..rows).map(|i| u32_at(start + 4 * i) as u64).collect();
    let y_body = uncondense_matrix(params, &y);
    let z_body = uncondense_matrix(params, &z);
    let key = PackingKeys::init_from_uploaded(params.clone(), params.poly_len, Some(y_body), Some(z_body), Some(y),
                                              Some(z));
    Ok(Request { query, key })
}

pub fn new_request<'a>(params: &'a Params, pack: &PackParams<'a>, rows: usize, index: usize) -> (Client<'a>, Vec<u8>) {
    let mut client = Client::init(params);
    client.generate_secret_keys();
    let (_, key) = generate_packing_keys(PackingType::InspiRING, params, &client.get_sk_reg(), Some(pack));
    let values = YClient::new(&mut client, params)
        .generate_query(SEED_0, params.db_dim_1, PackingType::InspiRING, index, None, None);
    let q = params.modulus as u128;
    let query: Vec<u64> = values.into_iter()
        .map(|v| (((v as u128 * (1u128 << 32) + q / 2) / q) as u64) & 0xffffffff)
        .collect();
    let req = encode_request(params, &key, &query, rows);
    (client, req)
}

pub fn decrypt_response<'a>(params: &'a Params, client: &Client<'a>, bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.len() != response_len(params) {
        return Err(format!("response of {} bytes, expected {}", bytes.len(), response_len(params)));
    }
    let ct_bytes = params.poly_len * 28 / 8;
    let mut out = Vec::with_capacity(record_bytes(params));
    for chunk in bytes.chunks_exact(ct_bytes) {
        let ct = PolyMatrixRaw::recover(params, params.get_q_prime_1(), params.get_q_prime_2(), chunk);
        for &v in decrypt_ct_reg_measured(client, params, &ct.ntt(), params.poly_len).as_slice() {
            if v > 255 {
                return Err("decrypted value out of range".into());
            }
            out.push(v as u8);
        }
    }
    Ok(out)
}
