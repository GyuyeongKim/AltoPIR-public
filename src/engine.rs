use std::ffi::c_void;
use std::marker::PhantomData;

extern "C" {
    fn sw_amx_init() -> i32;
    fn sw_alloc_huge(bytes: usize) -> *mut u8;
    fn sw_alloc_huge_nz(bytes: usize) -> *mut u8;
    fn sw_free(p: *mut u8);
    fn sw_tile_db(dst: *mut u8, db: *const u8, ld: usize, k: usize, m: usize, threads: i32);
    fn sw_pack_build(ub: *mut i8, bold_t: *const *const u64, bold_t_bar: *const *const u64, tables: *const u32,
                     gen_pows: *const u32, num_iter: usize, tg: usize, n: usize, rho: usize, qmod: u64, threads: i32);
    fn alto_engine_new(db_col: *const u8, db_tiled: *const u8, ld: usize, rows: usize, cols: usize, ub: *const i8,
                       n: usize, tg: usize, prefix: *const *const u8, part_len: usize, row1_off: usize, q1: u64,
                       q1_bits: i32, that: *const *const u64, inv_table: *const u64, q: u64, threads: i32) -> *mut c_void;
    fn alto_engine_free(p: *mut c_void);
    fn alto_request_len(p: *const c_void) -> usize;
    fn alto_response_len(p: *const c_void) -> usize;
    fn alto_answer(p: *mut c_void, req: *const *const u8, len: *const usize, b: usize, out: *const *mut u8,
                   status: *mut i32, method: i32, stage_ms: *mut f64) -> i32;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Method {
    Amx,
    Vnni,
}

impl Method {
    pub fn parse(s: &str) -> Option<Method> {
        match s {
            "amx" => Some(Method::Amx),
            "vnni" => Some(Method::Vnni),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Method::Amx => "amx",
            Method::Vnni => "vnni",
        }
    }

    fn code(self) -> i32 {
        match self {
            Method::Amx => 0,
            Method::Vnni => 2,
        }
    }
}

pub fn cpu_flag(flag: &str) -> bool {
    std::fs::read_to_string("/proc/cpuinfo")
        .map(|s| s.lines().filter(|l| l.starts_with("flags")).any(|l| l.split_whitespace().any(|f| f == flag)))
        .unwrap_or(false)
}

pub fn amx_available() -> bool {
    cpu_flag("amx_int8") && cpu_flag("amx_tile")
}

pub fn vnni_available() -> bool {
    cpu_flag("avx512_vnni")
}

pub fn init_amx() {
    let r = unsafe { sw_amx_init() };
    assert_eq!(r, 0, "the kernel did not grant AMX tile state (arch_prctl ARCH_REQ_XCOMP_PERM)");
}

pub struct HugeBytes {
    ptr: *mut u8,
    len: usize,
}

unsafe impl Send for HugeBytes {}
unsafe impl Sync for HugeBytes {}

impl HugeBytes {
    pub fn zeroed(len: usize) -> Self {
        assert!(len > 0);
        let ptr = unsafe { sw_alloc_huge(len) };
        assert!(!ptr.is_null(), "out of memory");
        HugeBytes { ptr, len }
    }

    pub fn as_i8(&self) -> &[i8] {
        unsafe { std::slice::from_raw_parts(self.ptr as *const i8, self.len) }
    }

    pub fn as_i8_mut(&mut self) -> &mut [i8] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr as *mut i8, self.len) }
    }
}

impl Drop for HugeBytes {
    fn drop(&mut self) {
        unsafe { sw_free(self.ptr) }
    }
}

pub struct TiledDb {
    ptr: *mut u8,
    pub rows: usize,
    pub cols: usize,
}

unsafe impl Send for TiledDb {}
unsafe impl Sync for TiledDb {}

impl TiledDb {
    pub fn new(db: &[u8], ld: usize, rows: usize, cols: usize, threads: usize) -> Self {
        assert!(rows % 64 == 0 && cols % 32 == 0 && db.len() >= (cols - 1) * ld + rows);
        let ptr = unsafe { sw_alloc_huge_nz(rows * cols) };
        assert!(!ptr.is_null(), "out of memory");
        unsafe { sw_tile_db(ptr, db.as_ptr(), ld, rows, cols, threads as i32) };
        TiledDb { ptr, rows, cols }
    }
}

impl Drop for TiledDb {
    fn drop(&mut self) {
        unsafe { sw_free(self.ptr) }
    }
}

pub fn build_packing_matrix(ub: &mut [i8], bold_t: &[&[u64]], bold_t_bar: &[&[u64]], tables: &[u32], gen_pows: &[u32],
                            num_iter: usize, tg: usize, n: usize, qmod: u64, threads: usize) {
    let rho = bold_t.len();
    assert!(ub.len() == 4 * tg * n * rho * n && bold_t_bar.len() == rho);
    assert!(tables.len() == n * n && gen_pows.len() >= num_iter);
    let p: Vec<*const u64> = bold_t.iter().map(|s| { assert!(s.len() >= num_iter * tg * n); s.as_ptr() }).collect();
    let pb: Vec<*const u64> = bold_t_bar.iter().map(|s| { assert!(s.len() >= num_iter * tg * n); s.as_ptr() }).collect();
    unsafe {
        sw_pack_build(ub.as_mut_ptr(), p.as_ptr(), pb.as_ptr(), tables.as_ptr(), gen_pows.as_ptr(), num_iter, tg, n,
                      rho, qmod, threads as i32)
    }
}

pub struct Engine<'a> {
    ptr: *mut c_void,
    _borrow: PhantomData<&'a ()>,
}

unsafe impl Send for Engine<'_> {}

impl<'a> Engine<'a> {
    pub fn new(db_col: &'a [u8], tiled: &'a TiledDb, ld: usize, ub: &'a [i8], n: usize, tg: usize, prefixes: &[Vec<u8>],
               row1_off: usize, q1: u64, q1_bits: usize, that: &[&'a [u64]], inv_table: &'a [u64], q: u64,
               threads: usize) -> Self {
        let (rows, cols) = (tiled.rows, tiled.cols);
        assert!(db_col.len() >= (cols - 1) * ld + rows && ub.len() == 4 * tg * n * cols && prefixes.len() == cols / n);
        assert!(that.len() == cols / n && that.iter().all(|t| t.len() >= tg * n) && inv_table.len() >= n);
        let part_len = prefixes[0].len();
        assert!(prefixes.iter().all(|p| p.len() == part_len));
        let pp: Vec<*const u8> = prefixes.iter().map(|p| p.as_ptr()).collect();
        let tp: Vec<*const u64> = that.iter().map(|t| t.as_ptr()).collect();
        let ptr = unsafe {
            alto_engine_new(db_col.as_ptr(), tiled.ptr, ld, rows, cols, ub.as_ptr(), n, tg, pp.as_ptr(), part_len,
                            row1_off, q1, q1_bits as i32, tp.as_ptr(), inv_table.as_ptr(), q, threads as i32)
        };
        assert!(!ptr.is_null(), "unsupported engine configuration");
        Engine { ptr, _borrow: PhantomData }
    }

    pub fn request_len(&self) -> usize {
        unsafe { alto_request_len(self.ptr) }
    }

    pub fn response_len(&self) -> usize {
        unsafe { alto_response_len(self.ptr) }
    }

    pub fn answer(&mut self, req: &[&[u8]], out: &mut [&mut [u8]], status: &mut [i32], method: Method) -> (usize, [f64; 4]) {
        let b = req.len();
        let rl = self.response_len();
        assert!(out.len() == b && status.len() == b && out.iter().all(|o| o.len() == rl));
        let rp: Vec<*const u8> = req.iter().map(|r| r.as_ptr()).collect();
        let lens: Vec<usize> = req.iter().map(|r| r.len()).collect();
        let op: Vec<*mut u8> = out.iter_mut().map(|o| o.as_mut_ptr()).collect();
        let mut stage_ms = [0f64; 4];
        let r = unsafe {
            alto_answer(self.ptr, rp.as_ptr(), lens.as_ptr(), b, op.as_ptr(), status.as_mut_ptr(), method.code(),
                        stage_ms.as_mut_ptr())
        };
        assert!(r >= 0, "engine error {}", r);
        (r as usize, stage_ms)
    }
}

impl Drop for Engine<'_> {
    fn drop(&mut self) {
        unsafe { alto_engine_free(self.ptr) }
    }
}
