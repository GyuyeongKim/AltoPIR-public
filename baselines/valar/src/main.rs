mod affinity;
mod fixture;
mod wire;

use inspiring::{QueryPackPreprocessed, RlweParams, TopKeyImages};
use ipir_sp::modulus_switch::recover_published_c1;
use ipir_sp::serialize::{deserialize_packing_keys, serialize_packing_keys};
use ipir_sp::server::{build_pack_preprocessed_blocks, published_c1_rows};
use ipir_sp::{ClientSeed, IPIRClient, IPIRServer, ProductionSimplePirParams, SimplePirProfile};
use rayon::{ThreadPool, ThreadPoolBuilder};
use std::sync::{Arc, Barrier};
use std::time::Instant;

fn ns() -> u64 {
    let mut t = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    assert_eq!(unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut t) }, 0);
    t.tv_sec as u64 * 1_000_000_000 + t.tv_nsec as u64
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Plan {
    cores: usize,
    workers: usize,
    threads: usize,
}

struct Pools {
    plan: Plan,
    cpus: Vec<usize>,
    pools: Vec<ThreadPool>,
}

impl Pools {
    fn create(plan: Plan, cpus: &[usize]) -> Self {
        let pools = (0..plan.workers)
            .map(|w| {
                let cpus = cpus.to_vec();
                ThreadPoolBuilder::new()
                    .num_threads(plan.threads)
                    .start_handler(move |i| affinity::pin(cpus[w * plan.threads + i]))
                    .build()
                    .unwrap()
            })
            .collect();
        let pools = Self { plan, cpus: cpus.to_vec(), pools };
        pools.preflight();
        pools
    }

    fn preflight(&self) {
        for (w, pool) in self.pools.iter().enumerate() {
            pool.broadcast(|ctx| affinity::pin(self.cpus[w * self.plan.threads + ctx.index()]));
        }
    }
}

struct Answer {
    id: usize,
    bytes: Vec<u8>,
    end: u64,
}

fn serve<'a>(p: &'a RlweParams, server: &IPIRServer<u16>, pre: &'a [QueryPackPreprocessed<'a>],
    top: &TopKeyImages<'a>, published: &[u8], rows: usize, id: usize, received: Vec<u8>) -> Answer {
    let bytes = {
        let (keybytes, querybytes) = wire::split_request(p, rows, &received).unwrap();
        let keys = deserialize_packing_keys(p, keybytes).unwrap();
        let (body, _) = server.perform_full_online_computation_simplepir_measured(p, querybytes, &keys, top, pre).unwrap();
        assert_eq!(body.len(), wire::BODY_LEN);
        let mut bytes = Vec::with_capacity(wire::RESPONSE_LEN);
        bytes.extend_from_slice(published);
        bytes.extend_from_slice(&body);
        bytes
    };
    drop(received);
    let end = ns();
    Answer { id, bytes, end }
}

fn run_server<'a>(p: &'a RlweParams, server: &IPIRServer<u16>, pre: &'a [QueryPackPreprocessed<'a>],
    top: &TopKeyImages<'a>, published: &[u8], rows: usize, pools: &Pools, requests: &[Vec<u8>]) -> (Vec<Answer>, u64) {
    let plan = pools.plan;
    pools.preflight();
    let ready = Arc::new(Barrier::new(plan.workers + 1));
    let go = Arc::new(Barrier::new(plan.workers + 1));
    let groups: Vec<Vec<(usize, Vec<u8>)>> = (0..plan.workers)
        .map(|w| requests.iter().enumerate().filter(|(i, _)| i % plan.workers == w).map(|(i, v)| (i, v.clone())).collect())
        .collect();
    let (mut answers, begin, end) = std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for (w, items) in groups.into_iter().enumerate() {
            let (ready, go, pool) = (ready.clone(), go.clone(), &pools.pools[w]);
            handles.push(scope.spawn(move || {
                let mut output = Vec::with_capacity(items.len());
                ready.wait();
                go.wait();
                pool.install(|| {
                    for (id, bytes) in items {
                        output.push(serve(p, server, pre, top, published, rows, id, bytes));
                    }
                });
                output
            }));
        }
        ready.wait();
        let begin = ns();
        go.wait();
        let mut answers = Vec::with_capacity(requests.len());
        for handle in handles {
            answers.extend(handle.join().unwrap());
        }
        let end = answers.iter().map(|a| a.end).max().unwrap();
        (answers, begin, end)
    });
    answers.sort_by_key(|a| a.id);
    (answers, end - begin)
}

struct Fresh {
    seed: ClientSeed,
    request: Vec<u8>,
    index: usize,
}

fn prepare(profile: &ProductionSimplePirParams, setup_bytes: &[u8], indices: &[usize]) -> Vec<Fresh> {
    indices
        .iter()
        .map(|&index| {
            let seed_public = wire::parse_setup(setup_bytes, profile.ypir().db_rows).unwrap();
            let client = IPIRClient::new(profile);
            let setup = client.generate_public_query_setup_simplepir_from_seed(seed_public);
            let (query, keys, seed) = client.generate_fresh_query_simplepir(&setup, index);
            let mut request = serialize_packing_keys(client.rlwe_params(), &keys).unwrap();
            request.extend_from_slice(&query.to_switched_bytes(client.rlwe_params().q, client.params().query_bits));
            Fresh { seed, request, index }
        })
        .collect()
}

fn decode(profile: &ProductionSimplePirParams, seed: &ClientSeed, bytes: &[u8]) -> Result<Vec<u8>, String> {
    let client = IPIRClient::new(profile);
    let p = client.rlwe_params();
    let (c1, body) = wire::split_response(p, bytes)?;
    let published = recover_published_c1(c1, p.d, 8, p.q);
    let words = client.decode_response_simplepir_raw(seed, &published, body);
    if words.len() != 16384 || words.iter().any(|&v| v >= 65536) {
        return Err("decoded record shape".into());
    }
    Ok(words.into_iter().flat_map(|v| (v as u16).to_le_bytes()).collect())
}

fn switch_pools(current: &mut Option<Pools>, plan: Plan, cpus: &[usize]) {
    if current.as_ref().map(|v| v.plan) != Some(plan) {
        *current = None;
        affinity::restrict(&cpus[..plan.cores]);
        *current = Some(Pools::create(plan, cpus));
    } else {
        affinity::restrict(&cpus[..plan.cores]);
    }
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = v.len();
    if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2.0 }
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let arg = |k: &str| a.iter().position(|x| x == k).map(|i| a[i + 1].clone());
    let usage = "usage: valar-bench --rows N --cpus LIST --cells B:C:WxT[,...] [--trials T]";
    let rows: usize = arg("--rows").expect(usage).parse().expect(usage);
    let cpus = affinity::parse(&arg("--cpus").expect(usage));
    let trials: u64 = arg("--trials").map_or(5, |v| v.parse().expect(usage));
    let cells: Vec<(usize, Plan)> = arg("--cells")
        .expect(usage)
        .split(',')
        .map(|c| {
            let f: Vec<&str> = c.split(':').collect();
            let (w, t) = f[2].split_once('x').expect(usage);
            (f[0].parse().unwrap(), Plan { cores: f[1].parse().unwrap(), workers: w.parse().unwrap(), threads: t.parse().unwrap() })
        })
        .collect();
    assert!(rows.is_power_of_two() && rows >= 2048, "--rows must be a power of two of at least 2048");
    for (b, plan) in &cells {
        assert!(plan.workers * plan.threads == plan.cores && plan.cores <= cpus.len() && plan.workers <= *b && b % plan.workers == 0,
            "cell {b}:{}:{}x{} needs W*T = C <= number of CPUs and W dividing B", plan.cores, plan.workers, plan.threads);
    }
    let threads = std::thread::available_parallelism().unwrap().get();
    ThreadPoolBuilder::new().num_threads(threads).build_global().unwrap();
    let profile = ProductionSimplePirParams::new(rows as u64, 262144, SimplePirProfile::P16Q49).unwrap();
    let anchor = IPIRClient::new(&profile);
    let p = anchor.rlwe_params();
    let setup_bytes = wire::setup(rows);
    let setup_begin = Instant::now();
    let database = fixture::database(rows);
    let server = IPIRServer::new_auto_kernel_from_profile(&profile, database.chunks_exact(2).map(|v| u16::from_le_bytes([v[0], v[1]])), false, false);
    drop(database);
    let public_setup = anchor.generate_public_query_setup_simplepir_from_seed(wire::parse_setup(&setup_bytes, rows).unwrap());
    let offline = server.perform_offline_precomputation_simplepir(p, public_setup.polys());
    let pre = build_pack_preprocessed_blocks(p, &offline.crs_blocks).unwrap();
    drop(offline);
    let published = published_c1_rows(&pre, p.q);
    assert_eq!(published.len(), wire::C1_LEN);
    let top = TopKeyImages::build(p);
    let setup_s = setup_begin.elapsed().as_secs_f64();
    affinity::move_memory(cpus[0]);
    unsafe { libc::malloc_trim(0) };
    println!("setup_s {setup_s:.1} request_bytes {} response_bytes {}", wire::request_len(p, rows), wire::RESPONSE_LEN);
    println!("{:>5} {:>5} {:>6} {:>12} {:>10}", "batch", "cores", "plan", "median_ms", "qps");
    let mut pools = None;
    for (batch, plan) in cells {
        switch_pools(&mut pools, plan, &cpus);
        let mut times = Vec::new();
        for trial in 0..=trials {
            let fresh = prepare(&profile, &setup_bytes, &fixture::indices(trial, batch, rows));
            let requests: Vec<_> = fresh.iter().map(|f| f.request.clone()).collect();
            let (answers, wall) = run_server(p, &server, &pre, &top, &published, rows, pools.as_ref().unwrap(), &requests);
            for (f, a) in fresh.iter().zip(&answers) {
                assert_eq!(decode(&profile, &f.seed, &a.bytes).unwrap(), fixture::record(f.index), "wrong record");
            }
            if trial > 0 {
                times.push(wall as f64 / 1e6);
            }
        }
        let ms = median(&mut times);
        println!("{batch:>5} {:>5} {:>6} {ms:>12.2} {:>10.3}", plan.cores, format!("{}x{}", plan.workers, plan.threads), batch as f64 * 1e3 / ms);
    }
}
