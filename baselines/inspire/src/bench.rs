#[path = "inspire.rs"] mod official;
#[path = "inspire_bench/paper_masks.rs"] mod paper_masks;
#[path = "inspire_bench/context_parser.rs"] mod context_parser;
#[path = "inspire_bench/fixture.rs"] mod fixture;
#[path = "inspire_bench/affinity.rs"] mod affinity;
#[path = "inspire_bench/wire.rs"] mod wire;
#[path = "inspire_bench/compact.rs"] mod compact;

use std::fs::File;
use std::io::Write;
use std::os::fd::FromRawFd;
use std::sync::{mpsc, Arc, Barrier};
use std::time::Instant;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use spiral_rs::client::Client;
use spiral_rs::gadget::get_bits_per;
use spiral_rs::params::Params;
use spiral_rs::poly::{PolyMatrix, PolyMatrixNTT, PolyMatrixRaw};
use inspire::client::{decrypt_ct_reg_measured, pack_query, YClient};
use inspire::measurement::Measurement;
use inspire::modulus_switch::ModulusSwitch;
use inspire::packing::{PackingKeys, PackingType};
use inspire::params::GetQPrime;
use inspire::scheme::{SEED_0, V_SEED, W_SEED};
use inspire::server::{OfflinePrecomputedValues, YServer};

#[repr(C)]
struct Timespec {
    sec: i64,
    nsec: i64,
}

extern "C" {
    fn clock_gettime(clock: i32, out: *mut Timespec) -> i32;
    fn malloc_trim(pad: usize) -> i32;
    fn dup(fd: i32) -> i32;
    fn dup2(from: i32, to: i32) -> i32;
}

fn ns() -> u64 {
    let mut t = Timespec { sec: 0, nsec: 0 };
    assert_eq!(unsafe { clock_gettime(1, &mut t) }, 0);
    t.sec as u64 * 1_000_000_000 + t.nsec as u64
}

fn generate_gsw<'a>(p: &'a Params, client: &Client<'a>, seeds: &[[u8; 32]; 6]) -> PolyMatrixNTT<'a> {
    let mut gsw = PolyMatrixNTT::zero(p, 1, 2 * p.t_gsw);
    let bits = get_bits_per(p, p.t_gsw);
    assert_eq!(bits, 19);
    for j in 0..p.t_gsw {
        let mut sigma = PolyMatrixRaw::zero(p, 1, 1);
        sigma.get_poly_mut(0, 0)[0] = 1u64 << (bits * j);
        let sigma_ntt = sigma.ntt();
        let ct = client.encrypt_matrix_reg(&sigma_ntt, &mut ChaCha20Rng::from_entropy(), &mut ChaCha20Rng::from_seed(seeds[2 * j + 1]));
        gsw.copy_into(&ct.submatrix(1, 0, 1, 1), 0, 2 * j + 1);
        let prod = &client.get_sk_reg().ntt() * &sigma_ntt;
        let ct = client.encrypt_matrix_reg(&prod, &mut ChaCha20Rng::from_entropy(), &mut ChaCha20Rng::from_seed(seeds[2 * j]));
        gsw.copy_into(&ct.submatrix(1, 0, 1, 1), 0, 2 * j);
    }
    gsw
}

fn decode<'a>(p: &'a Params, client: &Client<'a>, bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.len() != wire::response_len(p) {
        return Err("response length".into());
    }
    let mut out = Vec::with_capacity(fixture::RECORD_BYTES);
    for part in bytes.chunks_exact(wire::response_part_len(p)) {
        let ct = PolyMatrixRaw::recover_how_many(p, p.get_q_prime_1(), p.get_q_prime_2(), p.poly_len, part);
        let words = decrypt_ct_reg_measured(client, p, &ct.ntt(), p.poly_len);
        for &word in words.as_slice() {
            if word >= 65535 {
                return Err("noncanonical plaintext".into());
            }
            out.extend_from_slice(&(word as u16).to_le_bytes());
        }
    }
    Ok(out)
}

struct Answer {
    id: usize,
    bytes: Vec<u8>,
    end: u64,
}

fn serve<'a>(p: &'a Params, server: &YServer<'a, u16>, offline: &OfflinePrecomputedValues<'a>, id: usize, received: Vec<u8>, rows: usize) -> Answer {
    let bytes = {
        let request = {
            let native = compact::decode(p, &received, rows).unwrap();
            drop(received);
            native
        };
        wire::validate_request(p, &request, rows).unwrap();
        let (mut keys, q, gsw) = {
            let public_context = &server.packing_params_set[&p.poly_len];
            assert!(std::ptr::eq(public_context.params, p));
            assert_eq!(public_context.num_to_pack, p.poly_len);
            context_parser::deserialize_everything(p, request, public_context)
        };
        assert!(!keys.expanded && keys.y_all_condensed.is_none() && keys.y_bar_all_condensed.is_none());
        let mut m = Measurement::default();
        let parts = paper_masks::RGSWPIR::perform_online_computation_simplepir_and_rgsw(server, 1, q.as_slice(), gsw, offline, &mut keys, Some(&mut m));
        assert!(keys.expanded && keys.y_all_condensed.is_some() && keys.y_bar_all_condensed.is_some());
        assert_eq!(parts.len(), p.instances);
        assert!(parts.iter().all(|v| v.len() == wire::response_part_len(p)));
        parts.into_iter().flatten().collect::<Vec<u8>>()
    };
    let end = ns();
    Answer { id, bytes, end }
}

enum Job {
    Run { items: Vec<(usize, Vec<u8>)>, cpu: usize, ready: mpsc::Sender<usize>, completed: mpsc::Sender<Vec<Answer>>, go: Arc<Barrier> },
    Stop,
}

fn worker<'a>(p: &'a Params, server: &YServer<'a, u16>, offline: &OfflinePrecomputedValues<'a>, rows: usize, rx: mpsc::Receiver<Job>) {
    loop {
        match rx.recv().unwrap() {
            Job::Stop => return,
            Job::Run { items, cpu, ready, completed, go } => {
                affinity::pin(cpu);
                let mut answers = Vec::with_capacity(items.len());
                ready.send(cpu).unwrap();
                go.wait();
                for (i, request) in items {
                    answers.push(serve(p, server, offline, i, request, rows));
                }
                completed.send(answers).unwrap();
            }
        }
    }
}

fn call_pool(pool: &[mpsc::Sender<Job>], requests: &[Vec<u8>], cpus: &[usize]) -> (Vec<Answer>, u64) {
    let active = requests.len().min(cpus.len());
    let (ready_tx, ready_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let go = Arc::new(Barrier::new(active + 1));
    let mut groups: Vec<Vec<(usize, Vec<u8>)>> = (0..active).map(|_| Vec::new()).collect();
    for (i, q) in requests.iter().enumerate() {
        groups[i % active].push((i, q.clone()));
    }
    for (w, items) in groups.into_iter().enumerate() {
        pool[w].send(Job::Run { items, cpu: cpus[w], ready: ready_tx.clone(), completed: done_tx.clone(), go: go.clone() }).unwrap();
    }
    drop(ready_tx);
    drop(done_tx);
    for _ in 0..active {
        ready_rx.recv().unwrap();
    }
    let begin = ns();
    go.wait();
    let mut answers = Vec::with_capacity(requests.len());
    for _ in 0..active {
        answers.extend(done_rx.recv().unwrap());
    }
    answers.sort_by_key(|a| a.id);
    let end = answers.iter().map(|a| a.end).max().unwrap();
    (answers, end - begin)
}

fn batch<'a>(p: &'a Params, server: &YServer<'a, u16>, pool: &[mpsc::Sender<Job>], rows: usize, targets: &[usize], cpus: &[usize]) -> u64 {
    affinity::restrict(cpus);
    let mut clients: Vec<_> = (0..targets.len()).map(|_| Client::init(p)).collect();
    for client in &mut clients {
        client.generate_secret_keys();
    }
    let yclients: Vec<_> = clients.iter_mut().map(|client| YClient::new(client, p)).collect();
    let mut requests = Vec::with_capacity(targets.len());
    for (y, &index) in yclients.iter().zip(targets) {
        let mut keys = PackingKeys::init_full(&server.packing_params_set[&p.poly_len], y.client().get_sk_reg(), W_SEED, V_SEED);
        let gsw = generate_gsw(p, y.client(), &paper_masks::RGSW_SEEDS);
        let q = y.generate_query_over_prime(SEED_0, p.db_dim_1, PackingType::InspiRING, index);
        let packed = pack_query(p, &q);
        requests.push(official::serialize_everything(p, &mut keys, packed, gsw));
    }
    let compact: Vec<_> = requests.iter().map(|r| compact::encode(p, r, rows).unwrap()).collect();
    let wants: Vec<_> = targets.iter().map(|&i| fixture::record(i)).collect();
    let (answers, wall) = call_pool(pool, &compact, cpus);
    for a in &answers {
        assert_eq!(decode(p, yclients[a.id].client(), &a.bytes).unwrap(), wants[a.id], "wrong record");
    }
    drop(requests);
    wall
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = v.len();
    if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2.0 }
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let arg = |k: &str| a.iter().position(|x| x == k).map(|i| a[i + 1].clone());
    let usage = "usage: inspire-bench --rows N --cpus LIST --cells B:C[,...] [--trials T]";
    let rows: usize = arg("--rows").expect(usage).parse().expect(usage);
    let cpus = affinity::parse(&arg("--cpus").expect(usage));
    let trials: u64 = arg("--trials").map_or(5, |v| v.parse().expect(usage));
    let cells: Vec<(usize, usize)> = arg("--cells")
        .expect(usage)
        .split(',')
        .map(|c| {
            let (b, k) = c.split_once(':').expect(usage);
            (b.parse().unwrap(), k.parse().unwrap())
        })
        .collect();
    assert!(rows.is_power_of_two() && rows >= 2048, "--rows must be a power of two of at least 2048");
    assert!(cells.iter().all(|&(b, k)| b > 0 && k > 0 && k <= cpus.len()), "every cell needs 0 < C <= number of CPUs");
    std::io::stdout().flush().unwrap();
    let mut out = unsafe { File::from_raw_fd(dup(1)) };
    assert_eq!(unsafe { dup2(2, 1) }, 1);
    let threads = std::thread::available_parallelism().unwrap().get();
    rayon::ThreadPoolBuilder::new().num_threads(threads).build_global().unwrap();
    let (p, t, _) = official::params_rgswpir_given_input_size_and_dim0(rows, fixture::RECORD_BYTES * 8, rows);
    assert_eq!(t, 1);
    assert_eq!((p.poly_len, p.instances, p.crt_count, p.pt_modulus, p.t_exp_left, p.t_gsw), (2048, 8, 2, 65535, 3, 3));
    let start = Instant::now();
    let flat = fixture::database(rows);
    let mut column_major = vec![0u16; rows * 16384];
    std::thread::scope(|scope| {
        for (group, chunk) in column_major.chunks_mut(rows * 256).enumerate() {
            let flat = &flat;
            scope.spawn(move || {
                for (local, col) in chunk.chunks_exact_mut(rows).enumerate() {
                    let c = group * 256 + local;
                    for (r, dst) in col.iter_mut().enumerate() {
                        let off = r * fixture::RECORD_BYTES + 2 * c;
                        *dst = u16::from_le_bytes([flat[off], flat[off + 1]]);
                    }
                }
            });
        }
    });
    drop(flat);
    let server = <YServer<u16> as paper_masks::RGSWPIR>::new_rgswpir(&p, &column_major, 1);
    drop(column_major);
    let offline = server.perform_offline_precomputation_simplepir(p.poly_len, None, false);
    let setup_s = start.elapsed().as_secs_f64();
    affinity::move_memory(cpus[0]);
    unsafe { malloc_trim(0) };
    writeln!(out, "setup_s {setup_s:.1} request_bytes {} response_bytes {}", compact::request_len(&p, rows), wire::response_len(&p)).unwrap();
    writeln!(out, "{:>5} {:>5} {:>6} {:>12} {:>10}", "batch", "cores", "plan", "median_ms", "qps").unwrap();
    std::thread::scope(|scope| {
        let mut pool = Vec::new();
        let mut handles = Vec::new();
        for _ in 0..cpus.len() {
            let (tx, rx) = mpsc::channel();
            pool.push(tx);
            let (p, server, offline) = (&p, &server, &offline);
            handles.push(scope.spawn(move || worker(p, server, offline, rows, rx)));
        }
        for &(b, k) in &cells {
            let mut times = Vec::new();
            for trial in 0..=trials {
                let wall = batch(&p, &server, &pool, rows, &fixture::indices(trial, b, rows), &cpus[..k]);
                if trial > 0 {
                    times.push(wall as f64 / 1e6);
                }
            }
            let ms = median(&mut times);
            writeln!(out, "{b:>5} {k:>5} {:>6} {ms:>12.2} {:>10.3}", "-", b as f64 * 1e3 / ms).unwrap();
        }
        for tx in &pool {
            tx.send(Job::Stop).unwrap();
        }
        for h in handles {
            h.join().unwrap();
        }
    });
}
