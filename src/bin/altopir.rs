use std::collections::{HashMap, VecDeque};
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::os::unix::io::FromRawFd;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use altopir::engine::{self, Method};
use altopir::packing::PackParams;
use altopir::service::{self, synthetic, Database, Prepared, SetupTimes};
use rand::{Rng, SeedableRng};
use serde_json::json;
use spiral_rs::client::Client;
use spiral_rs::params::Params;

extern "C" {
    fn sched_setaffinity(pid: i32, size: usize, mask: *const u64) -> i32;
    fn sched_getaffinity(pid: i32, size: usize, mask: *mut u64) -> i32;
    fn mallopt(param: i32, value: i32) -> i32;
    fn socket(domain: i32, ty: i32, proto: i32) -> i32;
    fn bind(fd: i32, addr: *const SockAddrIn, len: u32) -> i32;
    fn connect(fd: i32, addr: *const SockAddrIn, len: u32) -> i32;
    fn setsockopt(fd: i32, level: i32, name: i32, val: *const i32, len: u32) -> i32;
    fn close(fd: i32) -> i32;
    fn madvise(addr: *mut u8, len: usize, advice: i32) -> i32;
    fn syscall(num: i64, ...) -> i64;
}

const MADV_DONTNEED: i32 = 4;
const SYS_MIGRATE_PAGES: i64 = 256;

#[repr(C)]
struct SockAddrIn {
    family: u16,
    port: u16,
    addr: [u8; 4],
    zero: [u8; 8],
}

const USAGE: &str = "\
usage:
  altopir server [--rows N | --db FILE] [--record-bytes B] [--listen ADDR] [--cpus LIST] [--net-cpus LIST]
                 [--setup-cpus LIST] [--max-batch N] [--queue N] [--response-buffers N] [--method amx|vnni]
                 [--log FILE]
  altopir query  --server ADDR --rows N [--record-bytes B] --index I[,I...] [--out FILE] [--check]
  altopir load   --server ADDR --rows N [--record-bytes B] --rate QPS --seconds S [--conns N] [--cpus LIST]
                 [--seed S] [--verify] [--out FILE] [--local-ip IP --port-base P]
  altopir bench  --rows N [--record-bytes B] [--batches 1,8,128] [--methods amx,vnni] [--trials T] [--cpus LIST]
                 [--client-cpus LIST] [--setup-cpus LIST] [--seed S]
  altopir stop   ADDR";

const STATUS_BAD_LENGTH: i32 = 1; // the engine's status for a request of the wrong length
const STATUS_BUSY: i32 = 9;

fn now_ns() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64
}

type Mask = [u64; 16];

fn mask(cpus: &[usize]) -> Mask {
    let mut m = [0u64; 16];
    for &c in cpus {
        m[c / 64] |= 1 << (c % 64);
    }
    m
}

fn set_self(m: &Mask) {
    assert_eq!(unsafe { sched_setaffinity(0, 128, m.as_ptr()) }, 0, "sched_setaffinity");
}

fn tids() -> Vec<i32> {
    let mut v: Vec<i32> = std::fs::read_dir("/proc/self/task").unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().parse().unwrap())
        .collect();
    v.sort();
    v
}

fn set_all(m: &Mask) {
    for _ in 0..32 {
        let before = tids();
        for &t in &before {
            unsafe { sched_setaffinity(t, 128, m.as_ptr()) };
        }
        if tids() == before && before.iter().all(|&t| {
            let mut g = [0u64; 16];
            (unsafe { sched_getaffinity(t, 128, g.as_mut_ptr()) }) == 0 && &g == m
        }) {
            return;
        }
    }
    panic!("thread affinity did not settle");
}

fn allowed_cpus() -> Vec<usize> {
    let mut m = [0u64; 16];
    assert_eq!(unsafe { sched_getaffinity(0, 128, m.as_mut_ptr()) }, 0);
    (0..1024).filter(|&c| m[c / 64] >> (c % 64) & 1 == 1).collect()
}

fn parse_list(s: &str) -> Vec<usize> {
    let mut v = Vec::new();
    for part in s.trim().split(',').filter(|p| !p.is_empty()) {
        match part.split_once('-') {
            Some((a, b)) => v.extend(a.parse::<usize>().unwrap()..=b.parse::<usize>().unwrap()),
            None => v.push(part.parse().unwrap()),
        }
    }
    v
}

fn cpu_node(cpu: usize) -> Option<usize> {
    std::fs::read_dir(format!("/sys/devices/system/cpu/cpu{}", cpu)).ok()?
        .filter_map(|e| e.ok()?.file_name().to_str()?.strip_prefix("node")?.parse().ok())
        .next()
}

/// The AMX engine reads only the tiled copy of the database, so an AMX-only process gives the pages of the row-order
/// copy back to the kernel after setup.
fn release_row_db(row_db: &[u8]) {
    let (lo, hi) = (row_db.as_ptr() as usize, row_db.as_ptr() as usize + row_db.len());
    let (start, end) = ((lo + 4095) / 4096 * 4096, hi / 4096 * 4096);
    if end > start {
        assert_eq!(unsafe { madvise(start as *mut u8, end - start, MADV_DONTNEED) }, 0, "madvise");
    }
}

/// Moves the memory of the process to the NUMA node of the engine cores, when they all belong to one node, so that a
/// setup that spilled to other nodes still serves from local memory. Returns that node.
fn move_memory_to_engine_node(cpus: &[usize]) -> Option<usize> {
    let node = cpu_node(cpus[0])?;
    if cpus.iter().any(|&c| cpu_node(c) != Some(node)) {
        return None;
    }
    let nodes = parse_list(&std::fs::read_to_string("/sys/devices/system/node/online").ok()?);
    let max = nodes.iter().copied().max()?.max(node) + 1;
    let (mut from, mut to) = (vec![0u64; (max + 63) / 64], vec![0u64; (max + 63) / 64]);
    for &n in nodes.iter().filter(|&&n| n != node) {
        from[n / 64] |= 1 << (n % 64);
    }
    to[node / 64] |= 1 << (node % 64);
    if from.iter().any(|&w| w != 0) {
        let r = unsafe { syscall(SYS_MIGRATE_PAGES, 0i64, (max + 1) as u64, from.as_ptr(), to.as_ptr()) };
        assert!(r >= 0, "migrate_pages: {}", io::Error::last_os_error());
    }
    Some(node)
}

fn parse_cpus(s: &str) -> Vec<usize> {
    let v = parse_list(s);
    let allowed = allowed_cpus();
    assert!(!v.is_empty() && v.iter().all(|c| allowed.contains(c)), "CPU list {} is not allowed for this process", s);
    v
}

struct Opts(HashMap<String, String>);

impl Opts {
    fn parse(a: &[String], values: &[&str], flags: &[&str]) -> Opts {
        let mut o = HashMap::new();
        let mut i = 0;
        while i < a.len() {
            let k = a[i].strip_prefix("--").unwrap_or_else(|| usage(&format!("unexpected argument {}", a[i])));
            if flags.contains(&k) {
                o.insert(k.to_string(), String::new());
                i += 1;
            } else if values.contains(&k) && i + 1 < a.len() {
                o.insert(k.to_string(), a[i + 1].clone());
                i += 2;
            } else {
                usage(&format!("unknown or incomplete option --{}", k));
            }
        }
        Opts(o)
    }
    fn get<T: std::str::FromStr>(&self, k: &str) -> Option<T> {
        self.0.get(k).map(|v| v.parse().unwrap_or_else(|_| usage(&format!("bad value for --{}: {}", k, v))))
    }
    fn req<T: std::str::FromStr>(&self, k: &str) -> T {
        self.get(k).unwrap_or_else(|| usage(&format!("--{} is required", k)))
    }
    fn str(&self, k: &str) -> Option<&str> {
        self.0.get(k).map(String::as_str)
    }
    fn has(&self, k: &str) -> bool {
        self.0.contains_key(k)
    }
    fn cpus(&self, k: &str) -> Option<Vec<usize>> {
        self.str(k).map(parse_cpus)
    }
}

fn usage(msg: &str) -> ! {
    eprintln!("altopir: {}\n{}", msg, USAGE);
    std::process::exit(2)
}

fn prepare_process() {
    unsafe {
        assert_eq!(mallopt(-1, i32::MAX), 1);
        assert_eq!(mallopt(-3, 32 << 20), 1);
    }
    assert!(std::env::var_os("OMP_PROC_BIND").is_none() && std::env::var_os("OMP_PLACES").is_none(),
            "unset OMP_PROC_BIND and OMP_PLACES: the engine places its own threads");
}

fn check_method(method: Method) {
    match method {
        Method::Amx => assert!(engine::amx_available(), "this CPU has no AMX; use --method vnni"),
        Method::Vnni => assert!(engine::vnni_available(), "this CPU has no AVX-512 VNNI"),
    }
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn emit(v: serde_json::Value) {
    println!("{}", v);
    io::stdout().flush().unwrap();
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    match a.get(1).map(String::as_str) {
        Some("server") => server(&a[2..]),
        Some("query") => query(&a[2..]),
        Some("load") => load(&a[2..]),
        Some("bench") => bench(&a[2..]),
        Some("stop") if a.len() == 3 => {
            let mut s = TcpStream::connect(&a[2]).unwrap();
            let mut h = [0u8; 12];
            h[4..].copy_from_slice(&u64::MAX.to_le_bytes());
            s.write_all(&h).unwrap();
        }
        Some("-h") | Some("--help") | Some("help") => println!("{}", USAGE),
        _ => usage("missing or unknown command"),
    }
}

struct Req {
    conn: usize,
    id: u64,
    buf: Vec<u8>,
    t_recv: u64,
}

enum Msg {
    Resp { lease: Arc<Lease>, off: usize, len: usize, id: u64 },
    Status { id: u64, status: i32 },
}

struct Arena(std::cell::UnsafeCell<Vec<u8>>);
unsafe impl Sync for Arena {}
unsafe impl Send for Arena {}

/// A response buffer lent to one batch. When the engine and every writer have dropped their handles, the buffer goes
/// back to the free list, so the engine waits for a free buffer without polling.
struct Lease {
    arena: Arc<Arena>,
    index: usize,
    free: mpsc::Sender<usize>,
}

impl Drop for Lease {
    fn drop(&mut self) {
        let _ = self.free.send(self.index);
    }
}

struct Queue {
    q: Mutex<VecDeque<Req>>,
    cv: Condvar,
    stop: AtomicBool,
    busy: AtomicU64,
}

fn write_frame(s: &mut TcpStream, id: u64, status: i32, body: &[u8]) -> io::Result<()> {
    let mut h = [0u8; 16];
    h[..4].copy_from_slice(&(body.len() as u32).to_le_bytes());
    h[4..12].copy_from_slice(&id.to_le_bytes());
    h[12..].copy_from_slice(&status.to_le_bytes());
    s.write_all(&h)?;
    s.write_all(body)
}

fn send_request(s: &mut TcpStream, id: u64, body: &[u8]) -> io::Result<()> {
    let mut h = [0u8; 12];
    h[..4].copy_from_slice(&(body.len() as u32).to_le_bytes());
    h[4..].copy_from_slice(&id.to_le_bytes());
    s.write_all(&h)?;
    s.write_all(body)
}

fn read_reply(s: &mut TcpStream) -> io::Result<(u64, i32, Vec<u8>)> {
    let mut h = [0u8; 16];
    s.read_exact(&mut h)?;
    let len = u32::from_le_bytes(h[..4].try_into().unwrap()) as usize;
    let id = u64::from_le_bytes(h[4..12].try_into().unwrap());
    let status = i32::from_le_bytes(h[12..].try_into().unwrap());
    let mut body = vec![0u8; len];
    s.read_exact(&mut body)?;
    Ok((id, status, body))
}

fn server(a: &[String]) {
    let o = Opts::parse(a, &["rows", "db", "record-bytes", "listen", "cpus", "net-cpus", "setup-cpus", "max-batch",
                             "queue", "response-buffers", "method", "log"], &[]);
    prepare_process();
    let rb: usize = o.get("record-bytes").unwrap_or(service::DEFAULT_RECORD_BYTES);
    let cpus = o.cpus("cpus").unwrap_or_else(allowed_cpus);
    let ncpus = o.cpus("net-cpus").unwrap_or_else(|| cpus.clone());
    let scpus = o.cpus("setup-cpus").unwrap_or_else(|| cpus.clone());
    let bmax: usize = o.get("max-batch").unwrap_or(128);
    let qcap: usize = o.get("queue").unwrap_or(512);
    let nbuf: usize = o.get("response-buffers").unwrap_or(4);
    if bmax == 0 || nbuf == 0 {
        usage("--max-batch and --response-buffers must be at least 1");
    }
    let method = Method::parse(o.str("method").unwrap_or("amx")).unwrap_or_else(|| usage("--method is amx or vnni"));
    check_method(method);
    let listen = o.str("listen").unwrap_or("0.0.0.0:7000").to_string();
    let (m, nm) = (mask(&cpus), mask(&ncpus));
    set_self(&mask(&scpus));

    let t0 = Instant::now();
    let db = match (o.str("db"), o.get::<usize>("rows")) {
        (Some(path), None) => Database::from_file(path, rb).unwrap_or_else(|e| usage(&format!("{}: {}", path, e))),
        (None, Some(rows)) => Database::synthetic(rows, rb),
        _ => usage("give either --rows (synthetic records) or --db FILE"),
    };
    let load_ms = t0.elapsed().as_secs_f64() * 1e3;
    let rows = db.rows;
    let params = service::params(rows, rb);
    let mut times = SetupTimes::default();
    let srv = service::build_server(&params, &db, scpus.len(), &mut times);
    let records = db.records;
    drop(db);
    let prep = Prepared::new(&params, &srv, rows, scpus.len(), &mut times);
    let mut engine = prep.engine(&params, &srv, cpus.len());
    let (rl, ql) = (engine.response_len(), engine.request_len());
    let t = Instant::now();
    if method == Method::Amx {
        release_row_db(srv.db());
    }
    let node = move_memory_to_engine_node(&cpus);
    let settle_ms = t.elapsed().as_secs_f64() * 1e3;
    set_all(&m);
    {
        // One batch of the largest size sizes every engine buffer before the first request, then one single request.
        let pack = PackParams::new(&params, params.poly_len);
        let (_, req) = service::new_request(&params, &pack, rows, 0);
        let mut out = vec![0u8; bmax * rl];
        for b in [bmax, 1] {
            let reqs: Vec<&[u8]> = vec![&req[..]; b];
            let mut status = vec![-1i32; b];
            let mut outs: Vec<&mut [u8]> = out.chunks_exact_mut(rl).take(b).collect();
            engine.answer(&reqs, &mut outs, &mut status, method);
            assert!(status.iter().all(|&s| s == 0));
        }
    }

    let queue = Arc::new(Queue { q: Mutex::new(VecDeque::new()), cv: Condvar::new(), stop: AtomicBool::new(false),
                                 busy: AtomicU64::new(0) });
    let writers: Arc<Mutex<Vec<mpsc::Sender<Msg>>>> = Arc::new(Mutex::new(Vec::new()));
    let sent_log: Arc<Mutex<Vec<(usize, u64, u64)>>> = Arc::new(Mutex::new(Vec::new()));
    let listener = TcpListener::bind(&listen).unwrap_or_else(|e| usage(&format!("cannot listen on {}: {}", listen, e)));
    {
        let (queue, writers, sent_log) = (queue.clone(), writers.clone(), sent_log.clone());
        std::thread::spawn(move || {
            set_self(&nm);
            for (conn, s) in listener.incoming().enumerate() {
                let s = match s { Ok(s) => s, Err(_) => continue };
                s.set_nodelay(true).unwrap();
                let (tx, rx) = mpsc::channel::<Msg>();
                writers.lock().unwrap().push(tx.clone());
                let mut ws = s.try_clone().unwrap();
                let sent_log = sent_log.clone();
                std::thread::spawn(move || {
                    set_self(&nm);
                    let mut log = Vec::new();
                    for msg in rx {
                        let (id, r) = match msg {
                            Msg::Resp { lease, off, len, id } => {
                                let body: &[u8] = unsafe { &(&*lease.arena.0.get())[off..off + len] };
                                (id, write_frame(&mut ws, id, 0, body))
                            }
                            Msg::Status { id, status } => (id, write_frame(&mut ws, id, status, &[])),
                        };
                        if r.is_err() {
                            break;
                        }
                        log.push((conn, id, now_ns()));
                    }
                    sent_log.lock().unwrap().extend(log);
                });
                let queue = queue.clone();
                let mut rs = s;
                std::thread::spawn(move || {
                    set_self(&nm);
                    loop {
                        let mut h = [0u8; 12];
                        if rs.read_exact(&mut h).is_err() {
                            break;
                        }
                        let len = u32::from_le_bytes(h[..4].try_into().unwrap()) as usize;
                        let id = u64::from_le_bytes(h[4..].try_into().unwrap());
                        if len == 0 && id == u64::MAX {
                            queue.stop.store(true, Ordering::SeqCst);
                            queue.cv.notify_all();
                            break;
                        }
                        if len > ql {
                            // Longer than any valid request: discard it without buffering and reply as the engine would.
                            match io::copy(&mut (&mut rs).take(len as u64), &mut io::sink()) {
                                Ok(n) if n == len as u64 => {}
                                _ => break,
                            }
                            let _ = tx.send(Msg::Status { id, status: STATUS_BAD_LENGTH });
                            continue;
                        }
                        let mut buf = vec![0u8; len];
                        if rs.read_exact(&mut buf).is_err() {
                            break;
                        }
                        let t_recv = now_ns();
                        let mut q = queue.q.lock().unwrap();
                        if q.len() >= qcap {
                            drop(q);
                            queue.busy.fetch_add(1, Ordering::Relaxed);
                            let _ = tx.send(Msg::Status { id, status: STATUS_BUSY });
                        } else {
                            q.push_back(Req { conn, id, buf, t_recv });
                            drop(q);
                            queue.cv.notify_one();
                        }
                    }
                });
            }
        });
    }
    emit(json!({"event":"ready","listen":listen,"rows":rows,"records":records,"record_bytes":rb,"method":method.name(),
        "cpus":cpus,"net_cpus":ncpus,"max_batch":bmax,"queue":qcap,"response_buffers":nbuf,"request_bytes":ql,
        "response_bytes":rl,
        "row_db_released":method == Method::Amx,"memory_node":node,
        "setup_ms":{"load":load_ms,"server":times.server_ms,"offline":times.offline_ms,"tile":times.tile_ms,
                    "packing":times.packing_ms,"settle":settle_ms}}));

    let arenas: Vec<Arc<Arena>> = (0..nbuf).map(|_| Arc::new(Arena(std::cell::UnsafeCell::new(vec![0u8; bmax * rl])))).collect();
    let (free_tx, free_rx) = mpsc::channel::<usize>();
    for i in 0..nbuf {
        free_tx.send(i).unwrap();
    }
    let mut batches: Vec<serde_json::Value> = Vec::new();
    let mut reqlog: Vec<(usize, u64, u64, usize)> = Vec::new();
    loop {
        let batch: Vec<Req> = {
            let mut q = queue.q.lock().unwrap();
            while q.is_empty() && !queue.stop.load(Ordering::SeqCst) {
                q = queue.cv.wait(q).unwrap();
            }
            if q.is_empty() {
                break;
            }
            let n = q.len().min(bmax);
            q.drain(..n).collect()
        };
        let t_start = now_ns();
        let i = free_rx.recv().unwrap();
        let lease = Arc::new(Lease { arena: arenas[i].clone(), index: i, free: free_tx.clone() });
        let b = batch.len();
        let mut status = vec![-1i32; b];
        let stage_ms = {
            let buf = unsafe { &mut *lease.arena.0.get() };
            let mut outs: Vec<&mut [u8]> = buf.chunks_exact_mut(rl).take(b).collect();
            let reqs: Vec<&[u8]> = batch.iter().map(|r| r.buf.as_slice()).collect();
            engine.answer(&reqs, &mut outs, &mut status, method).1
        };
        let t_end = now_ns();
        let w = writers.lock().unwrap();
        for (i, r) in batch.iter().enumerate() {
            let msg = if status[i] == 0 {
                Msg::Resp { lease: lease.clone(), off: i * rl, len: rl, id: r.id }
            } else {
                Msg::Status { id: r.id, status: status[i] }
            };
            let _ = w[r.conn].send(msg);
            reqlog.push((r.conn, r.id, r.t_recv, batches.len()));
        }
        drop(w);
        batches.push(json!([t_start, t_end, b, stage_ms]));
    }

    writers.lock().unwrap().clear();
    std::thread::sleep(Duration::from_millis(500));
    if let Some(path) = o.str("log") {
        let sent: HashMap<(usize, u64), u64> = sent_log.lock().unwrap().iter().map(|&(c, i, t)| ((c, i), t)).collect();
        let mut f = io::BufWriter::new(std::fs::File::create(path).unwrap());
        for bt in &batches {
            writeln!(f, "{}", json!({"batch":bt})).unwrap();
        }
        for &(c, i, t, bi) in &reqlog {
            writeln!(f, "{}", json!({"request":[c, i, t, bi, sent.get(&(c, i)).copied().unwrap_or(0)]})).unwrap();
        }
        f.flush().unwrap();
    }
    emit(json!({"event":"stopped","batches":batches.len(),"requests":reqlog.len(),
        "busy":queue.busy.load(Ordering::Relaxed)}));
}

fn open_connection(server: &str, local: Option<(Ipv4Addr, u16)>) -> TcpStream {
    let s = match local {
        None => TcpStream::connect(server).unwrap_or_else(|e| usage(&format!("cannot connect to {}: {}", server, e))),
        Some((ip, port)) => {
            let (h, p) = server.split_once(':').unwrap_or_else(|| usage("--server is IP:PORT"));
            let (sip, sport): (Ipv4Addr, u16) = (h.parse().unwrap(), p.parse().unwrap());
            unsafe {
                let fd = socket(2, 1, 0);
                assert!(fd >= 0);
                let one = 1i32;
                assert_eq!(setsockopt(fd, 1, 2, &one, 4), 0);
                let la = SockAddrIn { family: 2, port: port.to_be(), addr: ip.octets(), zero: [0; 8] };
                assert_eq!(bind(fd, &la, 16), 0, "bind {}:{}: {}", ip, port, io::Error::last_os_error());
                let sa = SockAddrIn { family: 2, port: sport.to_be(), addr: sip.octets(), zero: [0; 8] };
                if connect(fd, &sa, 16) != 0 {
                    let e = io::Error::last_os_error();
                    close(fd);
                    panic!("connect {}: {}", server, e);
                }
                TcpStream::from_raw_fd(fd)
            }
        }
    };
    s.set_nodelay(true).unwrap();
    s
}

fn query(a: &[String]) {
    let o = Opts::parse(a, &["server", "rows", "record-bytes", "index", "out"], &["check"]);
    let rb: usize = o.get("record-bytes").unwrap_or(service::DEFAULT_RECORD_BYTES);
    let rows: usize = o.req("rows");
    let server: String = o.req("server");
    let indices: Vec<usize> = o.req::<String>("index").split(',')
        .map(|v| v.parse().unwrap_or_else(|_| usage("--index takes record numbers")))
        .collect();
    assert!(indices.iter().all(|&i| i < rows), "a record number is not below --rows");
    let params = service::params(rows, rb);
    let pack = PackParams::new(&params, params.poly_len);
    let mut s = open_connection(&server, None);
    let mut out = o.str("out").map(|p| io::BufWriter::new(std::fs::File::create(p).unwrap()));
    let mut failed = 0;
    for (id, &index) in indices.iter().enumerate() {
        let t = Instant::now();
        let (client, req) = service::new_request(&params, &pack, rows, index);
        let t_sent = Instant::now();
        send_request(&mut s, id as u64, &req).unwrap();
        let (rid, status, body) = read_reply(&mut s).unwrap();
        let t_recv = Instant::now();
        assert_eq!(rid, id as u64);
        if status != 0 {
            println!("record {}: rejected by the server (status {})", index, status);
            failed += 1;
            continue;
        }
        let record = service::decrypt_response(&params, &client, &body).unwrap();
        let t_done = Instant::now();
        let mut line = format!("record {}: {} bytes, request {:.1} ms, server round trip {:.1} ms, decryption {:.1} ms",
                               index, record.len(), (t_sent - t).as_secs_f64() * 1e3,
                               (t_recv - t_sent).as_secs_f64() * 1e3, (t_done - t_recv).as_secs_f64() * 1e3);
        if o.has("check") {
            let ok = record == synthetic::record(index, rb);
            line += if ok { ", matches the synthetic record" } else { ", DIFFERS from the synthetic record" };
            if !ok {
                failed += 1;
            }
        }
        println!("{}", line);
        if let Some(f) = out.as_mut() {
            f.write_all(&record).unwrap();
        }
    }
    if let Some(mut f) = out {
        f.flush().unwrap();
    }
    if failed > 0 {
        std::process::exit(1);
    }
}

fn load(a: &[String]) {
    let o = Opts::parse(a, &["server", "rows", "record-bytes", "rate", "seconds", "conns", "cpus", "seed", "out",
                             "local-ip", "port-base", "start-at", "drain-s"], &["verify"]);
    let rb: usize = o.get("record-bytes").unwrap_or(service::DEFAULT_RECORD_BYTES);
    let rows: usize = o.req("rows");
    let rate: f64 = o.req("rate");
    let secs: f64 = o.req("seconds");
    let conns: usize = o.get("conns").unwrap_or(16);
    let cpus = o.cpus("cpus").unwrap_or_else(allowed_cpus);
    let seed: u64 = o.get("seed").unwrap_or(1);
    let server: String = o.req("server");
    let drain = Duration::from_secs_f64(o.get("drain-s").unwrap_or(10.0));
    let local: Option<Ipv4Addr> = o.get("local-ip");
    let port_base: Option<u16> = o.get("port-base");
    assert_eq!(local.is_some(), port_base.is_some(), "--local-ip and --port-base go together");
    let params = service::params(rows, rb);
    let pack = PackParams::new(&params, params.poly_len);

    let mut rng = rand_chacha::ChaCha20Rng::seed_from_u64(seed);
    let mut sched = Vec::new();
    let mut t = 0f64;
    loop {
        t += -(1.0 - rng.gen::<f64>()).ln() / rate;
        if t >= secs {
            break;
        }
        sched.push((t * 1e9) as u64);
    }
    let n = sched.len();
    let idx: Vec<usize> = (0..n).map(|_| rng.gen_range(0..rows)).collect();
    let g = mask(&cpus);
    let t0 = Instant::now();
    let chunk = ((n + cpus.len() - 1) / cpus.len()).max(1);
    let gen: Vec<(Client, Vec<u8>)> = std::thread::scope(|s| {
        let hs: Vec<_> = idx.chunks(chunk).map(|ch| {
            let (params, pack, g) = (&params, &pack, &g);
            s.spawn(move || {
                set_self(g);
                ch.iter().map(|&i| service::new_request(params, pack, rows, i)).collect::<Vec<_>>()
            })
        }).collect();
        hs.into_iter().flat_map(|h| h.join().unwrap()).collect()
    });
    let gen_s = t0.elapsed().as_secs_f64();
    let (clients, reqs): (Vec<Client>, Vec<Vec<u8>>) = gen.into_iter().unzip();
    let start_at: u64 = o.get("start-at").unwrap_or_else(|| now_ns() + 2_000_000_000);
    assert!(now_ns() + 500_000_000 < start_at, "requests were generated after the start time");

    let streams: Vec<TcpStream> = (0..conns)
        .map(|c| open_connection(&server, local.map(|ip| (ip, port_base.unwrap() + c as u16))))
        .collect();
    let reqs = Arc::new(reqs);
    let sched = Arc::new(sched);
    let results: Arc<Mutex<Vec<(u64, i32, u64, Vec<u8>)>>> = Arc::new(Mutex::new(Vec::new()));
    let mut handles = Vec::new();
    for (c, s) in streams.into_iter().enumerate() {
        let mut rs = s.try_clone().unwrap();
        let results = results.clone();
        let mine = (0..n).filter(|i| i % conns == c).count();
        handles.push(std::thread::spawn(move || {
            set_self(&g);
            let mut got = Vec::with_capacity(mine);
            while got.len() < mine {
                match read_reply(&mut rs) {
                    Ok((id, status, body)) => got.push((id, status, now_ns(), body)),
                    Err(_) => break,
                }
            }
            results.lock().unwrap().extend(got);
        }));
        let (reqs, sched, mut ws) = (reqs.clone(), sched.clone(), s);
        handles.push(std::thread::spawn(move || {
            set_self(&g);
            for i in (c..sched.len()).step_by(conns) {
                let due = start_at + sched[i];
                loop {
                    let now = now_ns();
                    if now >= due {
                        break;
                    }
                    if due - now > 200_000 {
                        std::thread::sleep(Duration::from_nanos(due - now - 150_000));
                    } else {
                        std::hint::spin_loop();
                    }
                }
                if send_request(&mut ws, i as u64, &reqs[i]).is_err() {
                    break;
                }
            }
            std::thread::sleep(drain);
            let _ = ws.shutdown(std::net::Shutdown::Both);
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    let results = Arc::try_unwrap(results).ok().unwrap().into_inner().unwrap();

    let answered: Vec<&(u64, i32, u64, Vec<u8>)> = results.iter().filter(|r| r.1 == 0).collect();
    let busy = results.iter().filter(|r| r.1 == STATUS_BUSY).count();
    let other = results.len() - answered.len() - busy;
    let mut lat: Vec<f64> = answered.iter()
        .map(|r| (r.2.saturating_sub(start_at + sched[r.0 as usize])) as f64 / 1e6)
        .collect();
    let bad = AtomicUsize::new(0);
    if o.has("verify") {
        std::thread::scope(|s| {
            let ch = ((answered.len() + cpus.len() - 1) / cpus.len()).max(1);
            for part in answered.chunks(ch) {
                let (params, clients, idx, g, bad) = (&params, &clients, &idx, &g, &bad);
                s.spawn(move || {
                    set_self(g);
                    for r in part {
                        let i = r.0 as usize;
                        let ok = service::decrypt_response(params, &clients[i], &r.3)
                            .map(|v| v == synthetic::record(idx[i], rb))
                            .unwrap_or(false);
                        if !ok {
                            bad.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                });
            }
        });
    }
    let pct = |v: &[f64], p: f64| if v.is_empty() { 0.0 } else { v[((v.len() as f64 * p) as usize).min(v.len() - 1)] };
    lat.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let summary = json!({"event":"load","rows":rows,"record_bytes":rb,"rate":rate,"seconds":secs,"conns":conns,
        "requests":n,"answered":answered.len(),"busy":busy,"rejected":other,"missing":n - results.len(),
        "answered_qps":answered.len() as f64 / secs,
        "latency_ms":{"p50":pct(&lat, 0.5),"p90":pct(&lat, 0.9),"p99":pct(&lat, 0.99),"max":lat.last().copied().unwrap_or(0.0)},
        "verified":o.has("verify"),"bad_records":bad.load(Ordering::Relaxed),"request_generation_s":gen_s});
    if let Some(path) = o.str("out") {
        let mut f = io::BufWriter::new(std::fs::File::create(path).unwrap());
        writeln!(f, "{}", summary).unwrap();
        for r in &results {
            writeln!(f, "{}", json!([r.0, start_at + sched[r.0 as usize], r.2, r.1])).unwrap();
        }
        f.flush().unwrap();
    }
    emit(summary);
    if bad.load(Ordering::Relaxed) > 0 {
        std::process::exit(1);
    }
}

fn bench(a: &[String]) {
    let o = Opts::parse(a, &["rows", "record-bytes", "batches", "methods", "trials", "cpus", "client-cpus",
                             "setup-cpus", "seed"], &[]);
    prepare_process();
    let rb: usize = o.get("record-bytes").unwrap_or(service::DEFAULT_RECORD_BYTES);
    let rows: usize = o.req("rows");
    let batches: Vec<usize> = o.str("batches").unwrap_or("1,8,128").split(',').map(|v| v.parse().unwrap()).collect();
    let methods: Vec<Method> = o.str("methods").unwrap_or("amx").split(',')
        .map(|v| Method::parse(v).unwrap_or_else(|| usage("--methods takes amx and vnni")))
        .collect();
    methods.iter().for_each(|&m| check_method(m));
    let trials: usize = o.get("trials").unwrap_or(5);
    let cpus = o.cpus("cpus").unwrap_or_else(allowed_cpus);
    let ccpus = o.cpus("client-cpus").unwrap_or_else(|| cpus.clone());
    let scpus = o.cpus("setup-cpus").unwrap_or_else(|| cpus.clone());
    let seed: u64 = o.get("seed").unwrap_or(1);
    let (m, g) = (mask(&cpus), mask(&ccpus));
    set_self(&mask(&scpus));

    let db = Database::synthetic(rows, rb);
    let params = service::params(rows, rb);
    let mut times = SetupTimes::default();
    let srv = service::build_server(&params, &db, scpus.len(), &mut times);
    drop(db);
    let prep = Prepared::new(&params, &srv, rows, scpus.len(), &mut times);
    let mut engine = prep.engine(&params, &srv, cpus.len());
    let rl = engine.response_len();
    let pack = PackParams::new(&params, params.poly_len);
    let t = Instant::now();
    let amx_only = methods.iter().all(|&m| m == Method::Amx);
    if amx_only {
        release_row_db(srv.db());
    }
    let node = move_memory_to_engine_node(&cpus);
    let settle_ms = t.elapsed().as_secs_f64() * 1e3;
    set_all(&m);
    let maxb = *batches.iter().max().unwrap();
    let mut arena = vec![0u8; maxb * rl];
    emit(json!({"event":"setup","rows":rows,"record_bytes":rb,"db_bytes":rows * rb,"threads":cpus.len(),
        "row_db_released":amx_only,"memory_node":node,
        "setup_ms":{"server":times.server_ms,"offline":times.offline_ms,"tile":times.tile_ms,"packing":times.packing_ms,
                    "settle":settle_ms}}));

    let mut times_ms: HashMap<(usize, Method), Vec<f64>> = HashMap::new();
    let mut state = seed ^ synthetic::SEED;
    for trial in 0..=trials {
        for bi in 0..batches.len() {
            let b = batches[(bi + trial) % batches.len()];
            let ix: Vec<usize> = (0..b).map(|_| synthetic::mix(&mut state) as usize % rows).collect();
            let workers = ccpus.len();
            let chunk = ((b + workers - 1) / workers).max(1);
            let (clients, reqs): (Vec<Client>, Vec<Vec<u8>>) = std::thread::scope(|s| {
                let hs: Vec<_> = ix.chunks(chunk).map(|ch| {
                    let (params, pack, g) = (&params, &pack, &g);
                    s.spawn(move || {
                        set_self(g);
                        ch.iter().map(|&i| service::new_request(params, pack, rows, i)).collect::<Vec<_>>()
                    })
                }).collect();
                hs.into_iter().flat_map(|h| h.join().unwrap()).unzip()
            });
            let fr: Vec<&[u8]> = reqs.iter().map(|v| v.as_slice()).collect();
            for pos in 0..methods.len() {
                let method = methods[(pos + trial + bi) % methods.len()];
                let mut status = vec![-1i32; b];
                let mut outs: Vec<&mut [u8]> = arena.chunks_exact_mut(rl).take(b).collect();
                let t = Instant::now();
                let (answered, _) = engine.answer(&fr, &mut outs, &mut status, method);
                let ms = t.elapsed().as_secs_f64() * 1e3;
                assert!(answered == b && status.iter().all(|&s| s == 0));
                verify(&params, &clients, &arena[..b * rl], rl, &ix, rb, &g, workers);
                if trial > 0 {
                    times_ms.entry((b, method)).or_default().push(ms);
                }
            }
        }
    }
    println!("{:>6}  {:>6}  {:>10}  {:>10}", "batch", "method", "median ms", "QPS");
    for &b in &batches {
        for &method in &methods {
            let mut v = times_ms[&(b, method)].clone();
            let med = median(&mut v);
            println!("{:>6}  {:>6}  {:>10.2}  {:>10.1}", b, method.name(), med, b as f64 / (med / 1e3));
        }
    }
}

fn verify(params: &Params, clients: &[Client], responses: &[u8], rl: usize, ix: &[usize], rb: usize, g: &Mask,
          workers: usize) {
    let n = clients.len();
    let chunk = ((n + workers - 1) / workers).max(1);
    std::thread::scope(|s| {
        for lo in (0..n).step_by(chunk) {
            s.spawn(move || {
                set_self(g);
                for i in lo..(lo + chunk).min(n) {
                    let got = service::decrypt_response(params, &clients[i], &responses[i * rl..(i + 1) * rl]).unwrap();
                    assert!(got == synthetic::record(ix[i], rb), "response {} does not decrypt to record {}", i, ix[i]);
                }
            });
        }
    });
}
