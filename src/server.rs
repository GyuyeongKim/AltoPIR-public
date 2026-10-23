#[cfg(target_feature = "avx2")]
use std::arch::x86_64::*;
use std::{marker::PhantomData, ops::Range, time::Instant};

use log::debug;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;

use spiral_rs::aligned_memory::AlignedMemory64;
#[allow(unused_imports)]
use spiral_rs::{arith::*, client::*, number_theory::invert_uint_mod, params::*, poly::*};

use crate::convolution::naive_multiply_matrices;
use crate::measurement::Measurement;

#[allow(unused_imports)]
use crate::modulus_switch::ModulusSwitch;
use crate::{
    bits::*,
    client::*,
    convolution::{negacyclic_perm_u32, Convolution},
    kernel::*,
    lwe::*,
    matmul::matmul_vec_packed,
    packing::*,
    params::*,
    scheme::*,
    transpose::*,
    util::*,
};

pub fn generate_y_constants<'a>(
    params: &'a Params,
) -> (Vec<PolyMatrixNTT<'a>>, Vec<PolyMatrixNTT<'a>>) {
    let mut y_constants = Vec::new();
    let mut neg_y_constants = Vec::new();
    for num_cts_log2 in 1..params.poly_len_log2 + 1 {
        let num_cts = 1 << num_cts_log2;

        // Y = X^(poly_len / num_cts)
        let mut y_raw = PolyMatrixRaw::zero(params, 1, 1);
        y_raw.data[params.poly_len / num_cts] = 1;
        let y = y_raw.ntt();

        let mut neg_y_raw = PolyMatrixRaw::zero(params, 1, 1);
        neg_y_raw.data[params.poly_len / num_cts] = params.modulus - 1;
        let neg_y = neg_y_raw.ntt();

        y_constants.push(y);
        neg_y_constants.push(neg_y);
    }

    (y_constants, neg_y_constants)
}

/// Takes a matrix of u64s and returns a matrix of T's.
///
/// Input is row x cols u64's.
/// Output is out_rows x cols T's.
pub fn split_alloc(
    buf: &[u64],
    special_bit_offs: usize,
    rows: usize,
    cols: usize,
    out_rows: usize,
    inp_mod_bits: usize,
    pt_bits: usize,
) -> Vec<u16> {
    let mut out = vec![0u16; out_rows * cols];

    assert!(out_rows >= rows);
    assert!(inp_mod_bits >= pt_bits);

    for j in 0..cols {
        let mut bytes_tmp = vec![0u8; out_rows * inp_mod_bits / 8];

        // read this column
        let mut bit_offs = 0;
        for i in 0..rows {
            // even though hint was stored in u64, it only needed u32, then mod switch down to u28, so we grab the value and only store those 28 bits
            let inp = buf[i * cols + j];

            if i == rows - 1 {
                bit_offs = special_bit_offs;
            }

            write_bits(&mut bytes_tmp, inp, bit_offs, inp_mod_bits);
            bit_offs += inp_mod_bits;
        }

        // now, 'stretch' the column vertically
        let mut bit_offs = 0;
        for i in 0..out_rows {
            let out_val = read_bits(&bytes_tmp, bit_offs, pt_bits);
            out[i * cols + j] = out_val as u16;
            bit_offs += pt_bits;
            if bit_offs >= out_rows * inp_mod_bits {
                break;
            }
        }

        assert_eq!(
            out[(special_bit_offs / pt_bits) * cols + j] as u64,
            buf[(rows - 1) * cols + j] & ((1 << pt_bits) - 1)
        );
    }

    out
}

pub fn generate_fake_pack_pub_params<'a>(params: &'a Params) -> Vec<PolyMatrixNTT<'a>> {
    // sk is 0, since this is server pre-processing no client
    let pack_pub_params = raw_generate_expansion_params(
        &params,
        &PolyMatrixRaw::zero(&params, 1, 1),
        params.poly_len_log2,
        params.t_exp_left,
        &mut ChaCha20Rng::from_entropy(),
        &mut ChaCha20Rng::from_seed(STATIC_SEED_2),
    );
    pack_pub_params
}

pub type Precomp<'a> = Vec<(PolyMatrixNTT<'a>, Vec<PolyMatrixNTT<'a>>, Vec<Vec<usize>>)>;

#[derive(Clone)]
pub struct OfflinePrecomputedValues<'a> {
    pub hint_0: Vec<u64>,
    pub y_constants: (Vec<PolyMatrixNTT<'a>>, Vec<PolyMatrixNTT<'a>>),
    pub prepacked_lwe: Vec<Vec<PolyMatrixNTT<'a>>>,
    pub precomp: Precomp<'a>,
    // InspiRING packing fields
    pub packing_type: PackingType,
    pub packing_params: Option<PackParams<'a>>,
    pub precomp_inspir_vec: Option<Vec<PrecompInsPIR<'a>>>,
    pub offline_packing_keys: Option<OfflinePackingKeys<'a>>,
}

/// Threads of the offline computation: SW_OFFLINE_THREADS (set by service::build_server), or all available CPUs.
fn offline_threads() -> usize {
    std::env::var("SW_OFFLINE_THREADS").ok().and_then(|v| v.parse().ok())
        .unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)).max(1)
}

#[derive(Clone)]
pub struct YServer<'a, T> {
    pub(crate) params: &'a Params,
    pub(crate) smaller_params: Params,
    pub(crate) db_buf_aligned: AlignedMemory64, // db_buf: Vec<u8>, // stored transposed
    pub(crate) phantom: PhantomData<T>,
    pub(crate) pad_rows: bool,
    pub(crate) ypir_params: YPIRParams,
}

pub trait DbRowsPadded {
    fn db_rows_padded(&self) -> usize;
}

impl DbRowsPadded for Params {
    fn db_rows_padded(&self) -> usize {
        1 << (self.db_dim_1 + self.poly_len_log2)
    }
}

impl<'a, T> YServer<'a, T>
where
    T: Sized + Copy + ToU64 + Default + Sync,
    *const T: ToM512,
{
    pub fn new<'b, I>(
        params: &'a Params,
        mut db: I,
        is_simplepir: bool,
        inp_transposed: bool,
        pad_rows: bool,
    ) -> Self
    where
        I: Iterator<Item = T>,
    {
        let mut ypir_params = YPIRParams::default();
        ypir_params.is_simplepir = is_simplepir;
        let bytes_per_pt_el = std::mem::size_of::<T>(); //1; //((lwe_params.pt_modulus as f64).log2() / 8.).ceil() as usize;

        let db_rows = 1 << (params.db_dim_1 + params.poly_len_log2);
        let db_rows_padded = if pad_rows {
            params.db_rows_padded()
        } else {
            db_rows
        };
        let db_cols = if is_simplepir {
            params.instances * params.poly_len
        } else {
            1 << (params.db_dim_2 + params.poly_len_log2)
        };

        let sz_bytes = db_rows_padded * db_cols * bytes_per_pt_el;

        let mut db_buf_aligned = AlignedMemory64::new(sz_bytes / 8);
        let db_buf_mut = as_bytes_mut(&mut db_buf_aligned);
        let db_buf_ptr = db_buf_mut.as_mut_ptr() as *mut T;

        // Load database in column-major format
        for i in 0..db_rows {
            for j in 0..db_cols {
                let idx = if inp_transposed {
                    i * db_cols + j
                } else {
                    j * db_rows_padded + i
                };

                unsafe {
                    *db_buf_ptr.add(idx) = db.next().unwrap();
                }
            }
        }

        // Parameters for the second round (the "DoublePIR" round)
        let smaller_params = if is_simplepir {
            params.clone()
        } else {
            let lwe_params = LWEParams::default();
            let pt_bits = (params.pt_modulus as f64).log2().floor() as usize;
            let blowup_factor = lwe_params.q2_bits as f64 / pt_bits as f64;
            let mut smaller_params = params.clone();
            smaller_params.db_dim_1 = params.db_dim_2;
            smaller_params.db_dim_2 = ((blowup_factor * (lwe_params.n + 1) as f64)
                / params.poly_len as f64)
                .log2()
                .ceil() as usize;

            let out_rows = 1 << (smaller_params.db_dim_2 + params.poly_len_log2);
            assert_eq!(smaller_params.db_dim_1, params.db_dim_2);
            assert!(out_rows as f64 >= (blowup_factor * (lwe_params.n + 1) as f64));
            smaller_params
        };


        Self {
            params,
            smaller_params,
            db_buf_aligned,
            phantom: PhantomData,
            pad_rows,
            ypir_params,
        }
    }

    /// Fast constructor for SandwichPIR server from a flat row-major u8 database.
    /// Bulk-loads then transposes to column-major in cache-friendly tiles.
    /// `db` is row-major: db[row * db_cols + col]. Padded with zeros if shorter than db_rows * db_cols.
    pub fn new_from_flat_db(
        params: &'a Params,
        db: &[u8],
        num_real_rows: usize,
    ) -> Self
    where T: From<u8>
    {
        let mut ypir_params = YPIRParams::default();
        ypir_params.is_simplepir = true;

        let db_rows = 1 << (params.db_dim_1 + params.poly_len_log2);
        let db_rows_padded = params.db_rows_padded();
        let db_cols = params.instances * params.poly_len;
        let bytes_per_pt_el = std::mem::size_of::<T>();
        let sz_bytes = db_rows_padded * db_cols * bytes_per_pt_el;

        debug!("Allocating {} MiB for transposed DB", sz_bytes / (1 << 20));
        let mut db_buf_aligned = AlignedMemory64::new(sz_bytes / 8);
        let db_buf_mut = as_bytes_mut(&mut db_buf_aligned);
        let db_buf_ptr = db_buf_mut.as_mut_ptr() as *mut T;

        // Zero the buffer (for padding rows)
        unsafe { std::ptr::write_bytes(db_buf_ptr, 0, db_rows_padded * db_cols); }

        // Tiled transpose: row-major input → column-major output.
        //
        // Implementation notes for the hot loop:
        //
        //   (1) We pre-bound `db` to exactly `rows_to_copy * db_cols`
        //       bytes (panicking if the caller's buffer is too small).
        //       Combined with raw pointer arithmetic in the inner
        //       loop, this eliminates the per-byte bounds checks that
        //       would otherwise run 8 billion times on an 8 GB DB
        //       (~200 s at ~25 ns/byte due to branch + redundant
        //       `src_idx < db.len()` tests).
        //
        //   (2) The inner loop reads 8 bytes at a time as a u64 from
        //       the contiguous source row, then fans them out to 8
        //       different destination columns (which are at separate
        //       cache lines, stride = db_rows_padded). One unaligned
        //       u64 load + 8 byte stores per iteration = ~8× the
        //       per-byte throughput of a naive loop.
        //
        //   (3) Tile blocking (64×64 u8 tiles) keeps both the source
        //       tile band and the 64 destination columns hot in L1.
        //
        // Measured: drops from ~200 s to a few seconds for an 8 GB
        // DB on a modern x86 host.
        let tile = 64usize;
        let rows_to_copy = num_real_rows.min(db_rows);
        assert!(
            db.len() >= rows_to_copy * db_cols,
            "new_from_flat_db: db ({} B) < rows_to_copy ({}) * db_cols ({}) = {}",
            db.len(), rows_to_copy, db_cols, rows_to_copy * db_cols,
        );
        assert_eq!(
            std::mem::size_of::<T>(), 1,
            "new_from_flat_db's fast path assumes T = u8 (pt_modulus = 256)",
        );
        let src_base = db.as_ptr();
        let start = std::time::Instant::now();

        for i_outer in (0..rows_to_copy).step_by(tile) {
            let i_end = (i_outer + tile).min(rows_to_copy);
            for j_outer in (0..db_cols).step_by(tile) {
                let j_end = (j_outer + tile).min(db_cols);
                // Number of full 8-byte j-chunks inside this tile slice.
                let j_chunks = (j_end - j_outer) / 8 * 8;
                for i in i_outer..i_end {
                    // Raw pointer to this row's starting byte.
                    let src_row = unsafe { src_base.add(i * db_cols) };
                    // 8 bytes per iteration: one u64 load, 8 scattered stores.
                    let mut j = j_outer;
                    while j < j_outer + j_chunks {
                        unsafe {
                            let chunk = (src_row.add(j) as *const u64).read_unaligned();
                            let dst_j0 = db_buf_ptr.add(j * db_rows_padded + i);
                            *dst_j0                                   = T::from((chunk      ) as u8);
                            *dst_j0.add(db_rows_padded)               = T::from((chunk >>  8) as u8);
                            *dst_j0.add(2 * db_rows_padded)           = T::from((chunk >> 16) as u8);
                            *dst_j0.add(3 * db_rows_padded)           = T::from((chunk >> 24) as u8);
                            *dst_j0.add(4 * db_rows_padded)           = T::from((chunk >> 32) as u8);
                            *dst_j0.add(5 * db_rows_padded)           = T::from((chunk >> 40) as u8);
                            *dst_j0.add(6 * db_rows_padded)           = T::from((chunk >> 48) as u8);
                            *dst_j0.add(7 * db_rows_padded)           = T::from((chunk >> 56) as u8);
                        }
                        j += 8;
                    }
                    // Tail: 1..7 remaining bytes when j_end isn't a multiple of 8.
                    while j < j_end {
                        unsafe {
                            let val = *src_row.add(j);
                            *db_buf_ptr.add(j * db_rows_padded + i) = T::from(val);
                        }
                        j += 1;
                    }
                }
            }
        }
        debug!(
            "Transposed {} x {} DB in {:.2} s",
            rows_to_copy, db_cols,
            start.elapsed().as_secs_f64()
        );

        let smaller_params = params.clone();

        Self {
            params,
            smaller_params,
            db_buf_aligned,
            phantom: PhantomData,
            pad_rows: true,
            ypir_params,
        }
    }

    /// Stream-construct a YServer by reading the DB in row-batches from
    /// an `io::Read` source and tile-transposing each batch directly
    /// into the aligned buffer. Never holds the full source DB in
    /// host memory — peak host allocation is the 8 GB aligned buffer
    /// plus a small (~8 MB) rolling `row_batch` scratch.
    ///
    /// Designed for the wiki-server / pir-serve path where the DB is
    /// a regular file on disk (local SSD or Lustre). On Lustre,
    /// large-block `read(2)` syscalls hit network bandwidth limits
    /// (~600 MB/s) rather than per-page RPC limits (~70 µs × 2M =
    /// 140 s for 8 GB with mmap), making this ~10× faster than
    /// `new_from_flat_db(&mmap[..])` on network filesystems.
    pub fn new_from_reader<R: std::io::Read>(
        params: &'a Params,
        reader: &mut R,
        num_real_rows: usize,
    ) -> std::io::Result<Self>
    where T: From<u8>
    {
        let mut ypir_params = YPIRParams::default();
        ypir_params.is_simplepir = true;

        assert_eq!(
            std::mem::size_of::<T>(), 1,
            "new_from_reader's fast path assumes T = u8",
        );

        let db_rows = 1 << (params.db_dim_1 + params.poly_len_log2);
        let db_rows_padded = params.db_rows_padded();
        let db_cols = params.instances * params.poly_len;
        let sz_bytes = db_rows_padded * db_cols;

        debug!(
            "Allocating {} MiB for transposed DB (streaming reader path)",
            sz_bytes / (1 << 20)
        );
        let mut db_buf_aligned = AlignedMemory64::new(sz_bytes / 8);
        let buf = as_bytes_mut(&mut db_buf_aligned);
        // Zero the whole buffer (padding rows will stay zero).
        unsafe { std::ptr::write_bytes(buf.as_mut_ptr(), 0, buf.len()); }
        // Cast to *mut T for consistency with new_from_flat_db's inner
        // loop — T is constrained to u8 by the size assert above.
        let db_buf_ptr = buf.as_mut_ptr() as *mut T;

        let rows_to_copy = num_real_rows.min(db_rows);

        // Row-batch buffer: 64 rows × db_cols ≈ 8 MB at default params.
        // Fits easily in L2, amortises `read_exact` syscall overhead
        // over a meaningful chunk of the file.
        const ROW_BATCH: usize = 64;
        let mut temp = vec![0u8; ROW_BATCH * db_cols];

        // J-tile for transpose cache/TLB locality. Each dst column has
        // stride = db_rows_padded (~64 KB at 8 GB DB), meaning each
        // column lands on a different 4 KB page. Without j-tiling the
        // inner loop would touch ~65 K pages per row and thrash the
        // 64-entry L1 dTLB; with a 64-col tile we touch at most 64
        // pages within a tile, staying hot through all `this_batch`
        // rows in that tile.
        const J_TILE: usize = 64;
        let start = std::time::Instant::now();
        let mut i_outer = 0;
        while i_outer < rows_to_copy {
            let this_batch = ROW_BATCH.min(rows_to_copy - i_outer);
            let bytes_this = this_batch * db_cols;
            reader.read_exact(&mut temp[..bytes_this])?;

            let mut j_outer = 0;
            while j_outer < db_cols {
                let j_end = (j_outer + J_TILE).min(db_cols);
                // Largest multiple-of-8 stop within this tile.
                let j_stop = j_end & !7;
                for i_within in 0..this_batch {
                    let i = i_outer + i_within;
                    let src_row = unsafe { temp.as_ptr().add(i_within * db_cols) };
                    let mut j = j_outer;
                    while j < j_stop {
                        unsafe {
                            let chunk = (src_row.add(j) as *const u64).read_unaligned();
                            let dst_j0 = db_buf_ptr.add(j * db_rows_padded + i);
                            *dst_j0                         = T::from((chunk      ) as u8);
                            *dst_j0.add(db_rows_padded)     = T::from((chunk >>  8) as u8);
                            *dst_j0.add(2 * db_rows_padded) = T::from((chunk >> 16) as u8);
                            *dst_j0.add(3 * db_rows_padded) = T::from((chunk >> 24) as u8);
                            *dst_j0.add(4 * db_rows_padded) = T::from((chunk >> 32) as u8);
                            *dst_j0.add(5 * db_rows_padded) = T::from((chunk >> 40) as u8);
                            *dst_j0.add(6 * db_rows_padded) = T::from((chunk >> 48) as u8);
                            *dst_j0.add(7 * db_rows_padded) = T::from((chunk >> 56) as u8);
                        }
                        j += 8;
                    }
                    // Tail within this tile (0..7 bytes when J_TILE
                    // isn't a multiple of 8 — not our default but
                    // handled for safety).
                    while j < j_end {
                        unsafe {
                            *db_buf_ptr.add(j * db_rows_padded + i) =
                                T::from(*src_row.add(j));
                        }
                        j += 1;
                    }
                }
                j_outer += J_TILE;
            }
            i_outer += this_batch;
        }
        debug!(
            "Streaming-read transposed {} x {} DB in {:.2} s",
            rows_to_copy, db_cols,
            start.elapsed().as_secs_f64()
        );

        let smaller_params = params.clone();

        Ok(Self {
            params,
            smaller_params,
            db_buf_aligned,
            phantom: PhantomData,
            pad_rows: true,
            ypir_params,
        })
    }

    /// Bench-only: construct a YServer with its internal aligned DB
    /// buffer filled **directly** with random bytes drawn from a
    /// `ChaCha20Rng` seeded with a fixed constant. Avoids the
    /// intermediate `Vec<u8>` that `new_from_flat_db` requires,
    /// cutting the construction memory peak in half (one 8 GB buffer
    /// instead of two) and eliminating the ~15 s tile-transpose loop
    /// for large databases.
    ///
    /// The ChaCha20 stream is seeded from OS entropy (so each run
    /// gets a fresh database) and ChaCha20 is a cryptographically
    /// secure PRNG — the generated database has no statistical
    /// structure a GPU's memory compression or matmul hardware could
    /// exploit, giving honest benchmark throughput numbers.
    ///
    /// Only valid when `params.pt_modulus == 256` (the SandwichPIR
    /// default), since we skip the per-byte modulo and store raw
    /// random bytes. Padding rows (>= `num_real_rows`) are left zero.
    pub fn new_random_filled(params: &'a Params, num_real_rows: usize) -> Self
    where T: From<u8>
    {
        use rand::SeedableRng;
        use rand::RngCore;
        use rand_chacha::ChaCha20Rng;

        let mut ypir_params = YPIRParams::default();
        ypir_params.is_simplepir = true;

        assert_eq!(
            params.pt_modulus, 256,
            "YServer::new_random_filled assumes pt_modulus = 256 \
             (SandwichPIR default); use new_from_flat_db for other moduli."
        );
        assert_eq!(
            std::mem::size_of::<T>(), 1,
            "YServer::new_random_filled only supports T = u8"
        );

        let db_rows = 1 << (params.db_dim_1 + params.poly_len_log2);
        let db_rows_padded = params.db_rows_padded();
        let db_cols = params.instances * params.poly_len;
        let bytes_per_pt_el = std::mem::size_of::<T>();
        let sz_bytes = db_rows_padded * db_cols * bytes_per_pt_el;

        debug!("Allocating {} MiB for transposed DB (ChaCha20 random-fill)",
               sz_bytes / (1 << 20));
        let mut db_buf_aligned = AlignedMemory64::new(sz_bytes / 8);
        let buf = as_bytes_mut(&mut db_buf_aligned);

        // Zero the whole buffer first (padding rows stay zero).
        unsafe { std::ptr::write_bytes(buf.as_mut_ptr(), 0, buf.len()); }

        // Fill only the "real" rows of each column with ChaCha20 bytes.
        // Since pt_modulus = 256 every raw u8 is a valid plaintext
        // value, and since the data is uniformly random, the transposed
        // layout is layout-invariant: filling col-major is
        // observationally identical to row-major + transpose.
        let rows_to_fill = num_real_rows.min(db_rows);
        let start = std::time::Instant::now();

        // Seed from OS entropy — every run produces a fresh database.
        let mut rng = ChaCha20Rng::from_entropy();

        for col in 0..db_cols {
            let col_start = col * db_rows_padded;
            let col_end = col_start + rows_to_fill;
            rng.fill_bytes(&mut buf[col_start..col_end]);
        }
        debug!(
            "ChaCha20 random-filled {} x {} DB in {:.2} s",
            rows_to_fill, db_cols,
            start.elapsed().as_secs_f64()
        );

        let smaller_params = params.clone();

        Self {
            params,
            smaller_params,
            db_buf_aligned,
            phantom: PhantomData,
            pad_rows: true,
            ypir_params,
        }
    }

    pub fn db_rows_padded(&self) -> usize {
        if self.pad_rows {
            self.params.db_rows_padded()
        } else {
            1 << (self.params.db_dim_1 + self.params.poly_len_log2)
        }
    }

    pub fn db_cols(&self) -> usize {
        if self.ypir_params.is_simplepir {
            self.params.instances * self.params.poly_len
        } else {
            1 << (self.params.db_dim_2 + self.params.poly_len_log2)
        }
    }

    pub fn multiply_batched_with_db_packed<const K: usize>(
        &self,
        aligned_query_packed: &[u64],
        query_rows: usize,
    ) -> AlignedMemory64 {
        let db_rows_padded = self.db_rows_padded();
        let db_cols = self.db_cols();
        assert_eq!(aligned_query_packed.len(), K * query_rows * db_rows_padded);
        assert_eq!(K, 1);
        assert_eq!(query_rows, 1);

        let now = Instant::now();
        let mut result = AlignedMemory64::new(K * db_cols);
        fast_batched_dot_product_avx512::<K, _>(
            self.params,
            result.as_mut_slice(),
            aligned_query_packed,
            db_rows_padded,
            &self.db(),
            db_rows_padded,
            db_cols,
        );
        debug!("Fast dot product in {} us", now.elapsed().as_micros());

        result
    }

    pub fn lwe_multiply_batched_with_db_packed<const K: usize>(
        &self,
        aligned_query_packed: &[u32],
    ) -> Vec<u32> {
        let _db_rows = 1 << (self.params.db_dim_1 + self.params.poly_len_log2);
        let db_cols = self.db_cols();
        let db_rows_padded = self.db_rows_padded();
        assert_eq!(aligned_query_packed.len(), K * db_rows_padded);

        let mut result = vec![0u32; (db_cols + 8) * K];
        let now = Instant::now();
        let a_rows = db_cols;
        let a_true_cols = db_rows_padded;
        let a_cols = a_true_cols / 4; // order is inverted on purpose, because db is transposed
        let b_rows = a_true_cols;
        let b_cols = K;
        // this guy just calculates mat A x mat B (vec if K=1) in AVX form
        // if you swap dimensions, then a column major A rows x cols is equivalent to a row major transposed A cols x rows
        matmul_vec_packed(
            result.as_mut_slice(),
            self.db_u32(),
            aligned_query_packed,
            a_rows,
            a_cols,
            b_rows,
            b_cols,
        );
        let t = Instant::now();
        // this op is negligible compared to the matmul_vec_packed, a cost worth it to use SimplePIR's kernel and compute (A x DB) our hint on a column major DB
        let result = transpose_generic(&result, db_cols, K);
        debug!("Transpose in {} us", t.elapsed().as_micros());
        debug!("Fast dot product in {} us", now.elapsed().as_micros());

        result
    }

    pub fn multiply_with_db_ring(
        &self,
        preprocessed_query: &[PolyMatrixNTT],
        col_range: Range<usize>,
        seed_idx: u8,
    ) -> Vec<u64> {
        let db_rows_poly = 1 << (self.params.db_dim_1);
        let db_rows = 1 << (self.params.db_dim_1 + self.params.poly_len_log2);
        assert_eq!(preprocessed_query.len(), db_rows_poly);

        let mut result = Vec::new();
        let db = self.db();

        let mut prod = PolyMatrixNTT::zero(self.params, 1, 1);
        let mut db_elem_poly = PolyMatrixRaw::zero(self.params, 1, 1);
        let mut db_elem_ntt = PolyMatrixNTT::zero(self.params, 1, 1);

        for col in col_range.clone() {
            let mut sum = PolyMatrixNTT::zero(self.params, 1, 1);

            for row in 0..db_rows_poly {
                for z in 0..self.params.poly_len {
                    db_elem_poly.data[z] =
                        db[col * db_rows + row * self.params.poly_len + z].to_u64();
                }
                to_ntt(&mut db_elem_ntt, &db_elem_poly);

                multiply(&mut prod, &preprocessed_query[row], &db_elem_ntt); // CRT-based modulo multiply

                if row == db_rows_poly - 1 {
                    add_into(&mut sum, &prod); // can take modulo since NTT-friendly
                } else {
                    add_into_no_reduce(&mut sum, &prod);
                }
            }

            let sum_raw = sum.raw();

            // do negacyclic permutation (for first mul only)
            if seed_idx == SEED_0 && !self.ypir_params.is_simplepir {
                // this never happens (negacyclic rules)
                let sum_raw_transformed =
                    negacyclic_perm(sum_raw.get_poly(0, 0), 0, self.params.modulus);
                result.extend(&sum_raw_transformed);
            } else {
                result.extend(sum_raw.as_slice());
            }
        }

        let now = Instant::now();
        let res = transpose_generic(&result, col_range.len(), self.params.poly_len);
        debug!("transpose in {} us", now.elapsed().as_micros());
        res
    }

    pub fn generate_pseudorandom_query(&self, public_seed_idx: u8) -> Vec<PolyMatrixNTT<'a>> {
        let mut client = Client::init(&self.params);
        client.generate_secret_keys(); // short-secret LWE for automorphisms
        let y_client = YClient::new(&mut client, &self.params);

        // Generate RLWE query for GPU GEMM hint computation.
        let query = y_client.generate_query_impl(public_seed_idx, self.params.db_dim_1, PackingType::CDKS, 0, None, None);

        // this is basically just grabbing the random portion (A2)
        // correct, but not efficient
        let query_mapped = query
            .iter()
            .map(|x| x.submatrix(0, 0, 1, 1))
            .collect::<Vec<_>>();

        let mut preprocessed_query = Vec::new();
        for query_raw in query_mapped {
            let query_raw_transformed =
                negacyclic_perm(query_raw.get_poly(0, 0), 0, self.params.modulus);
            let mut query_transformed_pol = PolyMatrixRaw::zero(self.params, 1, 1);
            query_transformed_pol
                .as_mut_slice()
                .copy_from_slice(&query_raw_transformed);
            preprocessed_query.push(query_transformed_pol.ntt());
        }

        preprocessed_query
    }

    pub fn answer_hint_ring(&self, public_seed_idx: u8, cols: usize) -> Vec<u64> {
        let preprocessed_query = self.generate_pseudorandom_query(public_seed_idx);

        let nth: usize = offline_threads().min(cols.max(1));
        if nth == 1 {
            return self.multiply_with_db_ring(&preprocessed_query, 0..cols, public_seed_idx);
        }
        let per = (cols + nth - 1) / nth;
        let ranges: Vec<Range<usize>> = (0..cols).step_by(per).map(|lo| lo..(lo + per).min(cols)).collect();
        let parts: Vec<Vec<u64>> = std::thread::scope(|s| {
            let hs: Vec<_> = ranges
                .iter()
                .map(|r| {
                    let (pq, r) = (&preprocessed_query, r.clone());
                    s.spawn(move || self.multiply_with_db_ring(pq, r, public_seed_idx))
                })
                .collect();
            hs.into_iter().map(|h| h.join().unwrap()).collect()
        });
        let n = self.params.poly_len;
        let mut res = vec![0u64; n * cols];
        for (r, part) in ranges.iter().zip(parts.iter()) {
            let w = r.len();
            for z in 0..n {
                res[z * cols + r.start..z * cols + r.end].copy_from_slice(&part[z * w..(z + 1) * w]);
            }
        }
        res
    }

    pub fn generate_hint_0(&self) -> Vec<u64> {
        let _db_rows = 1 << (self.params.db_dim_1 + self.params.poly_len_log2);
        let db_cols = self.db_cols();

        let mut rng_pub = ChaCha20Rng::from_seed(get_seed(SEED_0));
        let lwe_params = LWEParams::default();

        // pseudorandom LWE query is n x db_rows
        let psuedorandom_query =
            generate_matrix_ring(&mut rng_pub, lwe_params.n, lwe_params.n, db_cols);

        // db is db_cols x db_rows (!!!)
        // hint_0 is n x db_cols
        let hint_0 = naive_multiply_matrices(
            &psuedorandom_query,
            lwe_params.n,
            db_cols,
            &self.db(),
            self.db_rows_padded(), // TODO: doesn't quite work
            db_cols,
            true,
        );
        hint_0.iter().map(|&x| x as u64).collect::<Vec<_>>()
    }


    pub fn generate_hint_0_ring(&self) -> Vec<u64> {
        let lwe_params = LWEParams::default();
        let conv = Convolution::new(lwe_params.n); // wrapper around NTT operations

        let db_rows = 1 << (self.params.db_dim_1 + self.params.poly_len_log2);
        let db_cols = self.db_cols();
        let n = lwe_params.n;

        let mut hint_0 = vec![0u64; n * db_cols];
        let convd_len = conv.params().crt_count * conv.params().poly_len;

        let mut rng_pub = ChaCha20Rng::from_seed(get_seed(SEED_0));
        let mut v_nega_perm_a = Vec::new();
        for _ in 0..db_rows / n {
            let mut a = vec![0u32; n];
            for idx in 0..n {
                a[idx] = rng_pub.sample::<u32, _>(rand::distributions::Standard);
            }
            let nega_perm_a = negacyclic_perm_u32(&a); // re-write a so that an LWE can be interpreted as an RLWE under the same key—yes, negacyclic_matrix_u32 is the Toeplitz analogue of negacyclic_perm_u32
            let nega_perm_a_ntt = conv.ntt(&nega_perm_a);
            v_nega_perm_a.push(nega_perm_a_ntt);
        }

        // this is where we handle the 4.1."Modulus Selection" section
        // q = 2^32 is not an NTT friendly modulus, so we instead work over a much larger group that doesn't overflow
        // in the Toeplitz regime:
        // to avoid overflow, we essentially sum products of Zq, ZN. so the max element is q*N*d (== lwe_params.modulus, lwe_params.pt_modulus, lwe_params.n, respectively)
        // we are working in the coefficient space, so we can just work with uin64_t >> log(q*N*d) and mod 2^32 whenever to avoid overflow on the column
        // recall, per polymut, we are bounded by the q*N*d overflow. but we want to sum m_1 of these (l1/d1) at a time per coefficient, so the potential overflow is
        // l1 * q (2^32) * 2^8. if l1 > 2^22, then we overflow, which is a super large DB (>512 GB if 1bit)
        // the real overflow we saw was with trying to use GEMM32, so we needed to do CRT, etc. etc.
        
        // anyways!!!
        // we are actually doing NTT, so we are solving a different problem here.
        // we are working over the Ring space with modulus ~=2^56 - forget CRT for now, it's a detail
        // if we have coefficients bounded by 2^32 and 2^8, the maximum coefficient for a polynomial multiply will be: (N)(2^8)(2^32)
        // this is denoted as log2_conv_output
        // the idea is that: we are in R_q with q=2^32 right now with d=1024. this is not NTT_friendly (the 2n-th root of unity does not exist in the multiplicative grp)
        // but we can just pretend we are working over the integers as long as we never mod, do the INTT, then mod after.

        // in order to pretend we work in the integers, we work in the larger group
        // Q/A: it's quite neat that we can work in the larger group that exactly works for the RLWE automorphorisms!?
        // so we work in Z_q2[x]/(X^d2) with q2_bits >> log2_conv_output, INTT when we get close to overflowing the group!
        // TADA

        let log2_conv_output =
            log2(lwe_params.modulus) + log2(lwe_params.n as u64) + log2(lwe_params.pt_modulus);
        let log2_modulus = log2(conv.params().modulus); // ~= 2^56
        let log2_max_adds = log2_modulus - log2_conv_output - 1; // -1 so we stay BELOW the 2^56
        let max_adds = 1 << log2_max_adds;

        for col in 0..db_cols {
            let mut tmp_col = vec![0u64; convd_len]; // for each column, we compute one polynomial, stored in CRT format
            for outer_row in 0..db_rows / n {
                let start_idx = col * self.db_rows_padded() + outer_row * n;
                let pt_col = &self.db()[start_idx..start_idx + n];
                let pt_col_u32 = pt_col
                    .iter()
                    .map(|&x| x.to_u64() as u32)
                    .collect::<Vec<_>>();

                let pt_ntt = conv.ntt(&pt_col_u32);
                let convolved_ntt = conv.pointwise_mul(&v_nega_perm_a[outer_row], &pt_ntt); // pointwise mul over CRT-based NTT representation

                for r in 0..convd_len {
                    tmp_col[r] += convolved_ntt[r] as u64;
                }

                // write to hint_0
                if outer_row % max_adds == max_adds - 1 || outer_row == db_rows / n - 1 {
                    let mut col_poly_u32 = vec![0u32; convd_len];

                    // re-mod by CRT moduli
                    for i in 0..conv.params().crt_count {
                        for j in 0..conv.params().poly_len {
                            let val = barrett_coeff_u64(
                                conv.params(),
                                tmp_col[i * conv.params().poly_len + j],
                                i,
                            );
                            col_poly_u32[i * conv.params().poly_len + j] = val as u32;
                        }
                    }
                    
                    let col_poly_raw = conv.raw(&col_poly_u32);

                    // writes one column of the row-major matrix
                    for i in 0..n {
                        hint_0[i * db_cols + col] += col_poly_raw[i] as u64;
                        hint_0[i * db_cols + col] %= 1u64 << 32; // mod to Zq
                    }
                    tmp_col.fill(0);
                }
            }
        }
        hint_0
    }

    pub fn answer_query(&self, aligned_query_packed: &[u64]) -> AlignedMemory64 {
        self.multiply_batched_with_db_packed::<1>(aligned_query_packed, 1)
    }

    pub fn answer_batched_queries<const K: usize>(
        &self,
        aligned_queries_packed: &[u64],
    ) -> AlignedMemory64 {
        self.multiply_batched_with_db_packed::<K>(aligned_queries_packed, 1)
    }

    /// Word-based matrix-vector product for a single query.
    /// Returns the intermediate result (db_cols entries) after matmul in Z_{2^64},
    /// mod-switched to Z_Q, and CRT-packed into u64s.
    pub fn answer_query_word(&self, query: &[u64]) -> Vec<u64> {
        let params = self.params;
        let q = params.modulus;
        let db_rows = 1 << (params.db_dim_1 + params.poly_len_log2);
        let db_rows_padded = self.db_rows_padded();
        let db_cols = self.db_cols();
        let db = self.db();

        assert_eq!(query.len(), db_rows);

        let mut intermediate = vec![0u64; db_cols];
        for col in 0..db_cols {
            if params.crt_count == 1 {
                // SandwichPIR: W=2^32
                let mut acc: u32 = 0;
                for row in 0..db_rows {
                    acc = acc.wrapping_add(
                        (query[row] as u32).wrapping_mul(db[col * db_rows_padded + row].to_u64() as u32),
                    );
                }
                intermediate[col] = Self::modswitch_w32_to_q(acc, q);
            } else {
                // WordPIR: W=2^64
                let mut acc: u64 = 0;
                for row in 0..db_rows {
                    acc = acc.wrapping_add(
                        query[row].wrapping_mul(db[col * db_rows_padded + row].to_u64()),
                    );
                }
                let val_q = Self::modswitch_word_to_q(acc, q);
                let crt0 = val_q % params.moduli[0];
                let crt1 = val_q % params.moduli[1];
                intermediate[col] = crt0 | (crt1 << 32);
            }
        }

        intermediate
    }

    pub fn perform_offline_precomputation_simplepir(
        &self,
        measurement: Option<&mut Measurement>,
        packing: PackingType,
    ) -> OfflinePrecomputedValues<'_> {
        // Set up some parameters
        let params = self.params;
        assert!(self.ypir_params.is_simplepir);
        let db_cols = params.instances * params.poly_len;
        let num_rlwe_outputs = db_cols / params.poly_len;
        
        // Begin offline precomputation
        let simplepir_prep_time_ms: u128;

        let hint_0: Vec<u64> = {
            let now = Instant::now();
            let result = self.answer_hint_ring(SEED_0, db_cols);
            simplepir_prep_time_ms = now.elapsed().as_millis();
            debug!("SandwichPIR hint via CPU NTT in {} ms", simplepir_prep_time_ms);
            result
        };

        // hint_0 is poly_len x db_cols
        if let Some(measurement) = measurement {
            measurement.offline.simplepir_prep_time_ms = simplepir_prep_time_ms as f64;
        }

        // Prepare LWE packing input
        let combined = [&hint_0[..], &vec![0u64; db_cols]].concat();
        assert_eq!(combined.len(), db_cols * (params.poly_len + 1));
        let prepacked_lwe = prep_pack_many_lwes(&params, &combined, num_rlwe_outputs);

        // InspiRING offline precomputation (CPU path)
        let gamma = params.poly_len;
        let packing_params = PackParams::new(&params, gamma);
        let offline_packing_keys = OfflinePackingKeys::init_full(&packing_params, crate::scheme::W_SEED, crate::scheme::V_SEED);
        let now_precomp = Instant::now();
        let off_threads: usize = offline_threads();
        let mut slots: Vec<Option<PrecompInsPIR>> = (0..num_rlwe_outputs).map(|_| None).collect();
        let per = (num_rlwe_outputs + off_threads - 1) / off_threads;
        std::thread::scope(|s| {
            for (t, chunk) in slots.chunks_mut(per.max(1)).enumerate() {
                let (packing_params, offline_packing_keys, prepacked_lwe) = (&packing_params, &offline_packing_keys, &prepacked_lwe);
                s.spawn(move || {
                    for (j, slot) in chunk.iter_mut().enumerate() {
                        let i = t * per + j;
                        let a_ct_tilde: Vec<_> = (0..gamma)
                            .filter(|&jj| jj < prepacked_lwe[i].len())
                            .map(|jj| prepacked_lwe[i][jj].submatrix(0, 0, 1, 1))
                            .collect();
                        *slot = Some(crate::packing::full_packing_with_preprocessing_offline(
                            packing_params,
                            offline_packing_keys.w_all.as_ref().unwrap(),
                            offline_packing_keys.w_bar_all.as_ref().unwrap(),
                            offline_packing_keys.v_mask.as_ref().unwrap(),
                            &a_ct_tilde,
                        ));
                    }
                });
            }
        });
        let precomp_vec: Vec<PrecompInsPIR> = slots.into_iter().map(|x| x.unwrap()).collect();
        debug!("InspiRING precomp in {} us", now_precomp.elapsed().as_micros());

        OfflinePrecomputedValues {
            hint_0,
            y_constants: (Vec::new(), Vec::new()),
            prepacked_lwe,
            precomp: Vec::new(),
            packing_type: packing,
            packing_params: Some(packing_params),
            precomp_inspir_vec: Some(precomp_vec),
            offline_packing_keys: None,
        }
    }

    /// Modswitch a single value from Z_{2^64} to Z_Q with rounding.
    fn modswitch_word_to_q(x: u64, q: u64) -> u64 {
        ((x as u128 * q as u128 + (1u128 << 63)) >> 64) as u64
    }

    /// Modswitch a single value from Z_{2^32} to Z_Q with rounding.
    fn modswitch_w32_to_q(x: u32, q: u64) -> u64 {
        ((x as u64 * q + (1u64 << 31)) >> 32) as u64
    }

    /// Compute hint_0 using plain word-based matmul, then modswitch to Z_Q.
    /// A is poly_len × db_rows, DB is column-major db_cols × db_rows_padded.
    /// Output: poly_len × db_cols values in Z_Q, stored row-major.
    pub fn compute_hint_0_word(&self) -> Vec<u64> {
        let poly_len = self.params.poly_len;
        let db_rows = 1 << (self.params.db_dim_1 + self.params.poly_len_log2);
        let db_rows_padded = self.db_rows_padded();
        let db_cols = self.db_cols();
        let q = self.params.modulus;

        debug!(
            "compute_hint_0_word: poly_len={}, db_rows={}, db_cols={}, total_muls={}",
            poly_len, db_rows, db_cols, poly_len as u64 * db_rows as u64 * db_cols as u64
        );

        let now = Instant::now();
        let use_w32 = self.params.crt_count == 1;
        let a = if use_w32 {
            generate_pseudorandom_matrix_w32(SEED_0, poly_len, db_rows)
        } else {
            generate_pseudorandom_matrix_word(SEED_0, poly_len, db_rows)
        };
        debug!("  A matrix (W={}) generated in {} ms", if use_w32 { "2^32" } else { "2^64" }, now.elapsed().as_millis());

        let now = Instant::now();
        let mut hint_0 = vec![0u64; poly_len * db_cols];
        let db = self.db();

        let num_threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        let chunk_size = (db_cols + num_threads - 1) / num_threads;

        let mut hint_0_col_major = vec![0u64; db_cols * poly_len];

        std::thread::scope(|s| {
            let chunks: Vec<&mut [u64]> = hint_0_col_major.chunks_mut(chunk_size * poly_len).collect();
            let mut handles = Vec::new();
            for (chunk_idx, chunk) in chunks.into_iter().enumerate() {
                let col_start = chunk_idx * chunk_size;
                let col_end = (col_start + chunk_size).min(db_cols);
                let a_ref = &a;
                let db_ref = db;
                handles.push(s.spawn(move || {
                    for col in col_start..col_end {
                        let local_col = col - col_start;
                        for i in 0..poly_len {
                            if use_w32 {
                                // SandwichPIR: W=2^32, wrapping u32
                                let mut acc: u32 = 0;
                                for j in 0..db_rows {
                                    acc = acc.wrapping_add(
                                        (a_ref[i * db_rows + j] as u32)
                                            .wrapping_mul(db_ref[col * db_rows_padded + j].to_u64() as u32),
                                    );
                                }
                                chunk[local_col * poly_len + i] =
                                    Self::modswitch_w32_to_q(acc, q);
                            } else {
                                // WordPIR: W=2^64, wrapping u64
                                let mut acc: u64 = 0;
                                for j in 0..db_rows {
                                    acc = acc.wrapping_add(
                                        a_ref[i * db_rows + j]
                                            .wrapping_mul(db_ref[col * db_rows_padded + j].to_u64()),
                                    );
                                }
                                chunk[local_col * poly_len + i] =
                                    Self::modswitch_word_to_q(acc, q);
                            }
                        }
                    }
                }));
            }
            for h in handles {
                h.join().unwrap();
            }
        });

        // Transpose col-major (db_cols × poly_len) to row-major (poly_len × db_cols)
        for col in 0..db_cols {
            for i in 0..poly_len {
                hint_0[i * db_cols + col] = hint_0_col_major[col * poly_len + i];
            }
        }

        debug!(
            "  Matmul ({} threads) + modswitch in {} ms",
            num_threads,
            now.elapsed().as_millis()
        );

        hint_0
    }

    /// Offline precomputation for word-based SimplePIR.
    /// Same structure as perform_offline_precomputation_simplepir but uses plain word matmul.
    pub fn perform_offline_precomputation_simplepir_word(
        &self,
        mut measurement: Option<&mut Measurement>,
        packing: PackingType,
        #[allow(unused_variables)] max_batch_size: usize,
    ) -> OfflinePrecomputedValues<'_> {
        let params = self.params;
        assert!(self.ypir_params.is_simplepir);
        let db_cols = params.instances * params.poly_len;
        let num_rlwe_outputs = db_cols / params.poly_len;
        let gamma = params.poly_len;

        // ── Phase 1: Compute hint_0 ──

        let simplepir_prep_time_ms: u128;

        let hint_0_packed: Vec<u64> = {
            let now = Instant::now();
            let mut hint_0 = self.compute_hint_0_word();
            if packing != PackingType::InspiRING {
                let inv_n = invert_uint_mod(params.poly_len as u64, params.modulus).unwrap();
                for val in hint_0.iter_mut() {
                    *val = multiply_uint_mod(*val, inv_n, params.modulus);
                }
            }
            simplepir_prep_time_ms = now.elapsed().as_millis();
            hint_0
        };

        if let Some(ref mut m) = measurement {
            m.offline.simplepir_prep_time_ms = simplepir_prep_time_ms as f64;
        }

        // ── Phase 2: Packing precomputation (CPU path for SandwichPIR) ──

        let (packing_params, inspir_precomp_vec, prepacked_lwe, precomp, y_constants) = {
            let pp = if packing == PackingType::InspiRING { Some(PackParams::new(&params, gamma)) } else { None };

            let combined = [&hint_0_packed[..], &vec![0u64; db_cols]].concat();
            let prepacked = prep_pack_many_lwes(&params, &combined, num_rlwe_outputs);

            let (pc, yc) = if packing == PackingType::CDKS {
                let fake_pp = generate_fake_pack_pub_params(&params);
                let yc = generate_y_constants(&params);
                let mut pc: Precomp = Vec::new();
                for i in 0..prepacked.len() {
                    pc.push(precompute_pack(params, params.poly_len_log2, &prepacked[i], &fake_pp, &yc));
                }
                (pc, generate_y_constants(&params))
            } else {
                (Vec::new(), (Vec::new(), Vec::new()))
            };

            let inspir_precomp = if packing == PackingType::InspiRING {
                let pp_ref = pp.as_ref().unwrap();
                let offline_keys = OfflinePackingKeys::init_full(pp_ref, crate::scheme::W_SEED, crate::scheme::V_SEED);
                let mut pv = Vec::with_capacity(num_rlwe_outputs);
                for i in 0..num_rlwe_outputs {
                    let a_ct_tilde: Vec<_> = (0..gamma)
                        .filter(|&j| j < prepacked[i].len())
                        .map(|j| prepacked[i][j].submatrix(0, 0, 1, 1))
                        .collect();
                    pv.push(crate::packing::full_packing_with_preprocessing_offline(
                        pp_ref,
                        offline_keys.w_all.as_ref().unwrap(),
                        offline_keys.w_bar_all.as_ref().unwrap(),
                        offline_keys.v_mask.as_ref().unwrap(),
                        &a_ct_tilde,
                    ));
                }
                Some(pv)
            } else { None };

            (pp, inspir_precomp, prepacked, pc, yc)
        };

        OfflinePrecomputedValues {
            hint_0: hint_0_packed,
            y_constants,
            prepacked_lwe, precomp,
            packing_type: packing,
            packing_params, precomp_inspir_vec: inspir_precomp_vec,
            offline_packing_keys: None,
        }
    }

    pub fn perform_offline_precomputation(
        &self,
        measurement: Option<&mut Measurement>,
        packing: PackingType,
    ) -> OfflinePrecomputedValues<'_> {
        // Set up some parameters

        let params = self.params;
        let lwe_params = LWEParams::default();
        assert!(!self.ypir_params.is_simplepir);

        let db_cols = 1 << (params.db_dim_2 + params.poly_len_log2);

        // LWE reduced moduli
        let lwe_q_prime_bits = lwe_params.q2_bits as usize;
        let lwe_q_prime = lwe_params.get_q_prime_2();

        // The number of bits represented by a plaintext RLWE coefficient
        let pt_bits = (params.pt_modulus as f64).log2().floor() as usize;

        // The factor by which ciphertext values are bigger than plaintext values
        let blowup_factor = lwe_q_prime_bits as f64 / pt_bits as f64;
        debug!("blowup_factor: {}", blowup_factor);

        // The starting index of the final value (the '1' in lwe_params.n + 1)
        // This is rounded to start on a pt_bits boundary
        let special_offs =
            ((lwe_params.n * lwe_q_prime_bits) as f64 / pt_bits as f64).ceil() as usize;
        let special_bit_offs = special_offs * pt_bits;

        // Parameters for the second round (the "DoublePIR" round)
        let mut smaller_params = params.clone();
        smaller_params.db_dim_1 = params.db_dim_2;
        smaller_params.db_dim_2 = ((blowup_factor * (lwe_params.n + 1) as f64)
            / params.poly_len as f64)
            .log2()
            .ceil() as usize;

        let out_rows = 1 << (smaller_params.db_dim_2 + params.poly_len_log2);
        let rho = 1 << smaller_params.db_dim_2;
        assert_eq!(smaller_params.db_dim_1, params.db_dim_2);
        assert!(out_rows as f64 >= (blowup_factor * (lwe_params.n + 1) as f64));

        debug!(
            "the first {} LWE output ciphertexts of the DoublePIR round (out of {} total) are query-indepednent",
            special_offs, out_rows
        );
        debug!(
            "the next {} LWE output ciphertexts are query-dependent",
            blowup_factor.ceil() as usize
        );
        debug!("the rest are zero");

        // Begin offline precomputation

        let simplepir_prep_time_ms: u128;
        let hint_0: Vec<u64> = {
            let now = Instant::now();
            let result = self.generate_hint_0_ring();
            simplepir_prep_time_ms = now.elapsed().as_millis();
            result
        };
        // hint_0 is n x db_cols
        if let Some(measurement) = measurement {
            measurement.offline.simplepir_prep_time_ms = simplepir_prep_time_ms as f64;
        }
        // The debug message for non-CUDA case is moved inside the cfg block.
        // For CUDA, the timing is already debugged inside the block.

        // compute (most of) the secondary hint
        let intermediate_cts = [&hint_0[..], &vec![0u64; db_cols]].concat(); // concat so we add space for the SimplePIR repsonse in Z_q^l2
        let intermediate_cts_rescaled = intermediate_cts
            .iter()
            .map(|x| rescale(*x, lwe_params.modulus, lwe_q_prime))
            .collect::<Vec<_>>();

        // split and do a second PIR over intermediate_cts
        // split into blowup_factor=q/p instances (so that all values are now mod p)
        // the second PIR is over a database of db_cols x (blowup_factor * (lwe_params.n + 1)) values mod p

        // inp: (lwe_params.n + 1, db_cols)
        // out: (out_rows >= (lwe_params.n + 1) * blowup_factor, db_cols)
        //      we are 'stretching' the columns (and padding)

        debug!("Splitting intermediate cts...");

        // smaller_db: [H1 | T]: Z_p^(~(k*(d1+1)) + ~DB_dim_2) (~ because poly padded)
        // have to expand over the row-space because the column space is fixed
        // write now T is all zeroes per the concat
        // u16 because the plaintext space for the RLWE is 2^15
        let smaller_db = split_alloc(
            &intermediate_cts_rescaled,
            special_bit_offs,
            lwe_params.n + 1, // n represents H1, 1 is for T
            db_cols,
            out_rows,
            lwe_q_prime_bits,
            pt_bits,
        );
        assert_eq!(smaller_db.len(), db_cols * out_rows);

        debug!("Done splitting intermediate cts.");

        // This is the 'intermediate' db after the first pass of PIR and expansion
        // INP TRANSPOSED == TRUE!!!
        // smaller_db is row-major matrix of out_rows x db_cols
        // its stored as row-major as well
        let smaller_server: YServer<u16> = YServer::<u16>::new(
            &self.smaller_params,
            smaller_db.into_iter(),
            false,
            true,
            false,
        );
        debug!("gen'd smaller server.");


        // we just want to calculate H2 = A2 * H1 (recall, we padded H1 num_rows to poly_len)
        // this is an alternate way of generating a hint
        // for hint_0, we called generate_hint_0_ring, which the whole NTT thing
        // in perform_offline_precomputation_simplepir, we use this method to generate hint_0, so they must be functionally equivalent 

        // there is an identity between the encryption of query_row and the computation of hint_0
            // we work in the LWE space
            // query_row was encrypted by sampling a polynomial in q1=2^32 modulus, then getting the negacyclic_matrix and encrypting d1 pt at a time
            // hint_0 was computed using NTTs by sampling the same polynomial, using negacyclic_perm
            // negacyclic isn't really necessary since we don't pack this (it is for YPIR-SP)
        // there is an identify between the encryption of query_col and the computation of hint_1
            // we work in RLWE space
            // query_col was encrypted using the polynomial in q2=2^56 modulus, encrypting d2 pt at a time
            // hint_1 is computed using the same same poylnomial
            // neither was negacyclic, so in order to pack and unpack that has to be done at some point (CDKS 3.2 /JeremyKun)
            // yes-confirmed the random portions for the preprocess are negacyclically transformed in prepack_many_lwes, and the random portion for on the SimplePIR response encryption is negacyclically transformed before packing!!

        // gets A2 in NTT form through obfuscated method
        // then does same NTT multiply as in generate_hint_0_ring, but NTT-friendly so no modulo concerns
        // PolyMatrixRaw/NTT are stored in u64, so there was no overflow concern on adds (mods at the end)
        
        
        // fascinatingly, they pass the rows as the cols, but the DB was stored as row-major, so the column major access will actually be a row-based access
        // we initialized smaller_server with the transpose, so it didn't change it to column major, we also set its params to be swapped just like we pass in here
        // we compute DB2 * A2, iterating each row by poly and multiplying by A2's poly, giving H2 stored row-major: out_rows x poly_len
        // at the end, they transpose, getting a row-major poly_len x out_rows
        let hint_1 = smaller_server.answer_hint_ring(
            SEED_1,
            1 << (smaller_server.params.db_dim_2 + smaller_server.params.poly_len_log2),
        );
        assert_eq!(hint_1.len(), params.poly_len * out_rows);
        // T was 0, so transp(T*A2) will also be 0
        assert_eq!(hint_1[special_offs], 0);
        assert_eq!(hint_1[special_offs + 1], 0);

        // A2 in NTT form (we already generated this in the creation of hint_1)
        let _pseudorandom_query_1 = smaller_server.generate_pseudorandom_query(SEED_1);

        // now we just add the last row to store the DoublePIR response
        let combined = [&hint_1[..], &vec![0u64; out_rows]].concat(); // stored in row major
        assert_eq!(combined.len(), out_rows * (params.poly_len + 1)); // full DoublePIR response, 0s everywhere besides H2

        // get the rho many RLWE squares, negacyclic perms them, NTT form
        let prepacked_lwe = prep_pack_many_lwes(&params, &combined, rho);
        assert_eq!(prepacked_lwe.len(), rho);
        assert_eq!(prepacked_lwe[0].len(), params.poly_len);

        let gamma = params.poly_len;
        let now = Instant::now();
        let mut y_constants = (Vec::new(), Vec::new());
        let mut precomp: Precomp = Vec::new();
        let mut packing_params_opt: Option<PackParams> = None;
        let mut precomp_inspir_vec_opt: Option<Vec<PrecompInsPIR>> = None;
        let mut offline_packing_keys_opt: Option<OfflinePackingKeys> = None;

        match packing {
            PackingType::InspiRING => {
                let packing_params = PackParams::new(&params, gamma);
                let offline_packing_keys = OfflinePackingKeys::init_full(&packing_params, crate::scheme::W_SEED, crate::scheme::V_SEED);

                let mut precomp_vec = Vec::with_capacity(rho);
                for i in 0..rho {
                    let mut a_ct_tilde = Vec::new();
                    for j in 0..gamma {
                        if j < prepacked_lwe[i].len() {
                            a_ct_tilde.push(prepacked_lwe[i][j].submatrix(0, 0, 1, 1));
                        }
                    }

                    let w_all = offline_packing_keys.w_all.as_ref().unwrap();
                    let w_bar_all = offline_packing_keys.w_bar_all.as_ref().unwrap();
                    let v_mask = offline_packing_keys.v_mask.as_ref().unwrap();

                    let precomp_i = crate::packing::full_packing_with_preprocessing_offline_without_rotations(
                        &packing_params, w_all, w_bar_all, v_mask, &a_ct_tilde,
                    );
                    precomp_vec.push(precomp_i);
                }
                debug!("InspiRING DoublePIR precomp in {} us", now.elapsed().as_micros());
                packing_params_opt = Some(packing_params);
                precomp_inspir_vec_opt = Some(precomp_vec);
                offline_packing_keys_opt = Some(offline_packing_keys);
            },
            _ => {
                y_constants = generate_y_constants(&params);
                let fake_pack_pub_params = generate_fake_pack_pub_params(&params);
                for i in 0..prepacked_lwe.len() {
                    let tup: (PolyMatrixNTT<'_>, Vec<PolyMatrixNTT<'_>>, Vec<Vec<usize>>) = precompute_pack(
                        params,
                        params.poly_len_log2,
                        &prepacked_lwe[i],
                        &fake_pack_pub_params,
                        &y_constants,
                    );
                    precomp.push(tup);
                }
                debug!("CDKS Precomp in {} us", now.elapsed().as_micros());
            },
        }

        OfflinePrecomputedValues {
            hint_0,
            y_constants,
            prepacked_lwe,
            precomp,
            packing_type: packing,
            packing_params: packing_params_opt,
            precomp_inspir_vec: precomp_inspir_vec_opt,
            offline_packing_keys: offline_packing_keys_opt,
        }
    }

    /// Perform SimplePIR-style YPIR (CPU path, supports batching and InspiRING/CDKS dispatch)
    pub fn perform_online_computation_simplepir(
        &self,
        first_dim_queries_packed: &[&[u64]],
        offline_vals: &OfflinePrecomputedValues<'a>,
        packing_keys: &mut [PackingKeys<'a>],
        mut measurement: Option<&mut Measurement>,
    ) -> Vec<Vec<Vec<u8>>> {
        assert!(self.ypir_params.is_simplepir);

        let params = self.params;
        let y_constants = &offline_vals.y_constants;
        let prepacked_lwe = &offline_vals.prepacked_lwe;
        let precomp = &offline_vals.precomp;

        let rlwe_q_prime_1 = params.get_q_prime_1();
        let rlwe_q_prime_2 = params.get_q_prime_2();

        let db_rows = 1 << (params.db_dim_1 + params.poly_len_log2);
        let db_cols = params.instances * params.poly_len;
        let num_rlwe_outputs = db_cols / params.poly_len;
        let gamma = params.poly_len;

        let batch_size = first_dim_queries_packed.len();
        assert_eq!(first_dim_queries_packed[0].len(), params.db_rows_padded());

        // Step 1: Matmul for all queries
        let first_pass = Instant::now();
        debug!("Performing mul ({} queries)...", batch_size);
        let mut all_intermediates = vec![vec![0u64; db_cols]; batch_size];
        for (batch, query) in first_dim_queries_packed.iter().enumerate() {
            let mut intermediate = AlignedMemory64::new(db_cols);
            fast_batched_dot_product_avx512::<1, T>(
                &params,
                intermediate.as_mut_slice(),
                query,
                db_rows,
                self.db(),
                db_rows,
                db_cols,
            );
            all_intermediates[batch] = intermediate.as_slice().to_vec();
        }
        debug!("Done w mul...");
        let first_pass_time_ms = first_pass.elapsed().as_millis();
        if let Some(ref mut m) = measurement {
            m.online.first_pass_time_ms = first_pass_time_ms as f64;
        }

        // Step 2: Packing dispatch per client
        let ring_packing = Instant::now();
        let mut all_responses = Vec::with_capacity(batch_size);
        for (batch, intermediate) in all_intermediates.iter().enumerate() {
            let pk = &mut packing_keys[batch];

            let packed_mod_switched = match offline_vals.packing_type {
                PackingType::InspiRING => {
                    let packing_params = offline_vals.packing_params.as_ref().unwrap();
                    let precomp_inspir_vec = offline_vals.precomp_inspir_vec.as_ref().unwrap();

                    pk.expand(packing_params);
                    let packed = pack_many_lwes_inspir(
                        packing_params,
                        precomp_inspir_vec,
                        intermediate,
                        pk,
                        gamma,
                    );

                    let mut switched = Vec::with_capacity(packed.len());
                    for p in packed.iter() {
                        switched.push(p.switch_and_keep(rlwe_q_prime_1, rlwe_q_prime_2, gamma));
                    }
                    switched
                },
                PackingType::CDKS => {
                    let packed = pack_many_lwes(
                        &params,
                        &prepacked_lwe,
                        &precomp,
                        intermediate,
                        num_rlwe_outputs,
                        &pk.pack_pub_params_row_1s,
                        &y_constants,
                    );

                    let mut switched = Vec::with_capacity(packed.len());
                    for ct in packed.iter() {
                        let res = ct.raw();
                        switched.push(res.switch(rlwe_q_prime_1, rlwe_q_prime_2));
                    }
                    switched
                },
                _ => panic!("Unsupported packing type for online computation"),
            };
            all_responses.push(packed_mod_switched);
        }
        if let Some(m) = measurement {
            m.online.ring_packing_time_ms = ring_packing.elapsed().as_millis() as f64;
        }

        all_responses
    }

    /// Online computation for word-based SimplePIR (CPU path).
    /// Takes raw u64 queries (NOT CRT-packed), does plain matmul in Z_{2^64},
    /// modswitches to Z_Q, and feeds into existing packing.
    /// Always available (used as fallback for SandwichPIR when cuda is enabled).
    pub fn perform_online_computation_simplepir_word_cpu(
        &self,
        word_queries: &[&[u64]],
        offline_vals: &OfflinePrecomputedValues<'a>,
        packing_keys: &mut [PackingKeys<'a>],
        mut measurement: Option<&mut Measurement>,
    ) -> Vec<Vec<Vec<u8>>> {
        assert!(self.ypir_params.is_simplepir);

        let params = self.params;
        let q = params.modulus;
        let y_constants = &offline_vals.y_constants;
        let prepacked_lwe = &offline_vals.prepacked_lwe;
        let precomp = &offline_vals.precomp;

        let rlwe_q_prime_1 = params.get_q_prime_1();
        let rlwe_q_prime_2 = params.get_q_prime_2();

        let db_rows = 1 << (params.db_dim_1 + params.poly_len_log2);
        let db_rows_padded = self.db_rows_padded();
        let db_cols = params.instances * params.poly_len;
        let num_rlwe_outputs = db_cols / params.poly_len;
        let gamma = params.poly_len;

        let batch_size = word_queries.len();
        assert_eq!(word_queries[0].len(), db_rows);

        let first_pass = Instant::now();
        debug!("Performing word matmul ({} queries)...", batch_size);

        let db = self.db();

        // Compute intermediate per batch item: matmul in Z_W, modswitch to Z_Q
        let use_w32 = params.crt_count == 1;
        let mut all_intermediates = vec![vec![0u64; db_cols]; batch_size];
        for (batch, query) in word_queries.iter().enumerate() {
            let intermediate = &mut all_intermediates[batch];
            for col in 0..db_cols {
                if use_w32 {
                    // SandwichPIR: W=2^32, query values are u32 (stored in u64)
                    let mut acc: u32 = 0;
                    for row in 0..db_rows {
                        acc = acc.wrapping_add(
                            (query[row] as u32).wrapping_mul(db[col * db_rows_padded + row].to_u64() as u32),
                        );
                    }
                    intermediate[col] = Self::modswitch_w32_to_q(acc, q);
                } else {
                    // WordPIR: W=2^64
                    let mut acc: u64 = 0;
                    for row in 0..db_rows {
                        acc = acc.wrapping_add(
                            query[row].wrapping_mul(db[col * db_rows_padded + row].to_u64()),
                        );
                    }
                    intermediate[col] = Self::modswitch_word_to_q(acc, q);
                }
            }
        }

        // Pre-multiply all intermediate values by inv_N mod Q.
        // CDKS packing multiplies b-values by N (lines 594-601 of packing.rs).
        // N * inv_N = 1 mod Q, so the query noise is NOT amplified by N.
        // This mirrors the ring path's pre-division by N in generate_query_impl.
        // InspiRING does NOT pre-divide by N (its normalization happens via mod_inv_poly
        // on the mask side during offline precomp), so skip this for InspiRING.
        if offline_vals.packing_type != PackingType::InspiRING {
            let inv_n = invert_uint_mod(params.poly_len as u64, q).unwrap();
            for intermediate in all_intermediates.iter_mut() {
                for val in intermediate.iter_mut() {
                    *val = multiply_uint_mod(*val, inv_n, q);
                }
            }
        }

        debug!("Done w word matmul...");
        let first_pass_time_ms = first_pass.elapsed().as_millis();
        if let Some(ref mut m) = measurement {
            m.online.first_pass_time_ms = first_pass_time_ms as f64;
        }

        let ring_packing = Instant::now();
        let mut all_responses = Vec::with_capacity(batch_size);
        for (batch, intermediate) in all_intermediates.iter().enumerate() {
            let pk = &mut packing_keys[batch];

            let packed_mod_switched = match offline_vals.packing_type {
                PackingType::InspiRING => {
                    let packing_params = offline_vals.packing_params.as_ref().unwrap();
                    let precomp_inspir_vec = offline_vals.precomp_inspir_vec.as_ref().unwrap();

                    pk.expand(packing_params);
                    let packed = pack_many_lwes_inspir(
                        packing_params,
                        precomp_inspir_vec,
                        intermediate,
                        pk,
                        gamma,
                    );

                    let mut switched = Vec::with_capacity(packed.len());
                    for p in packed.iter() {
                        switched.push(p.switch_and_keep(rlwe_q_prime_1, rlwe_q_prime_2, gamma));
                    }
                    switched
                },
                _ => {
                    let packed = pack_many_lwes(
                        &params,
                        &prepacked_lwe,
                        &precomp,
                        intermediate,
                        num_rlwe_outputs,
                        &pk.pack_pub_params_row_1s,
                        &y_constants,
                    );
                    debug!("Packed batch {}...", batch);

                    let mut switched = Vec::with_capacity(packed.len());
                    for ct in packed.iter() {
                        let res = ct.raw();
                        switched.push(res.switch(rlwe_q_prime_1, rlwe_q_prime_2));
                    }
                    switched
                },
            };
            all_responses.push(packed_mod_switched);
        }
        if let Some(m) = measurement {
            m.online.ring_packing_time_ms = ring_packing.elapsed().as_millis() as f64;
        }

        all_responses
    }

    /// Non-CUDA alias: `perform_online_computation_simplepir_word` → CPU version
    pub fn perform_online_computation_simplepir_word(
        &self,
        word_queries: &[&[u64]],
        offline_vals: &OfflinePrecomputedValues<'a>,
        packing_keys: &mut [PackingKeys<'a>],
        measurement: Option<&mut Measurement>,
    ) -> Vec<Vec<Vec<u8>>> {
        self.perform_online_computation_simplepir_word_cpu(word_queries, offline_vals, packing_keys, measurement)
    }

    /// DoublePIR online computation (not used in SandwichPIR).
    pub fn perform_online_computation<const K: usize>(
        &self,
        _offline_vals: &mut OfflinePrecomputedValues<'a>,
        _first_dim_queries_packed: &[u32],
        _second_dim_query_cols: &[&[u64]],
        _packing_keys: &mut [PackingKeys<'a>],
        _measurement: Option<&mut Measurement>,
    ) -> Vec<Vec<Vec<u8>>> {
        unimplemented!("DoublePIR online path is not used in SandwichPIR")
    }

    // generic function that returns a u8 or u16:
    pub fn db(&self) -> &[T] {
        unsafe {
            std::slice::from_raw_parts(
                self.db_buf_aligned.as_ptr() as *const T,
                self.db_buf_aligned.len() * 8 / std::mem::size_of::<T>(),
            )
        }
    }

    pub fn db_mut(&mut self) -> &mut [T] {
        unsafe {
            std::slice::from_raw_parts_mut(
                self.db_buf_aligned.as_ptr() as *mut T,
                self.db_buf_aligned.len() * 8 / std::mem::size_of::<T>(),
            )
        }
    }

    /// Extract DB entries as u8 (p=256). Copies low byte of each T entry.
    pub fn db_u8(&self) -> Vec<u8> {
        self.db().iter().map(|x| x.to_u64() as u8).collect()
    }

    pub fn db_u16(&self) -> &[u16] {
        unsafe {
            std::slice::from_raw_parts(
                self.db_buf_aligned.as_ptr() as *const u16,
                self.db_buf_aligned.len() * 8 / std::mem::size_of::<u16>(),
            )
        }
    }

    pub fn db_u32(&self) -> &[u32] {
        unsafe {
            std::slice::from_raw_parts(
                self.db_buf_aligned.as_ptr() as *const u32,
                self.db_buf_aligned.len() * 8 / std::mem::size_of::<u32>(),
            )
        }
    }

    pub fn get_elem(&self, row: usize, col: usize) -> T {
        self.db()[col * self.db_rows_padded() + row] // stored transposed
    }

    pub fn get_row(&self, row: usize) -> Vec<T> {
        let db_cols = self.db_cols();
        let mut res = Vec::with_capacity(db_cols);
        for col in 0..db_cols {
            res.push(self.get_elem(row, col));
        }
        res
    }
}

#[cfg(not(target_feature = "avx512f"))]
#[allow(non_camel_case_types)]
type __m512i = u64;

pub trait ToM512 {
    fn to_m512(self) -> __m512i;
}

#[cfg(target_feature = "avx512f")]
mod m512_impl {
    use super::*;

    impl ToM512 for *const u8 {
        #[inline(always)]
        fn to_m512(self) -> __m512i {
            unsafe { _mm512_cvtepu8_epi64(_mm_loadl_epi64(self as *const _)) }
        }
    }

    impl ToM512 for *const u16 {
        #[inline(always)]
        fn to_m512(self) -> __m512i {
            unsafe { _mm512_cvtepu16_epi64(_mm_load_si128(self as *const _)) }
        }
    }

    impl ToM512 for *const u32 {
        #[inline(always)]
        fn to_m512(self) -> __m512i {
            unsafe { _mm512_cvtepu32_epi64(_mm256_load_si256(self as *const _)) }
        }
    }
}

#[cfg(not(target_feature = "avx512f"))]
mod m512_impl {
    use super::*;

    impl ToM512 for *const u8 {
        #[inline(always)]
        fn to_m512(self) -> __m512i {
            self as __m512i
        }
    }

    impl ToM512 for *const u16 {
        #[inline(always)]
        fn to_m512(self) -> __m512i {
            self as __m512i
        }
    }

    impl ToM512 for *const u32 {
        #[inline(always)]
        fn to_m512(self) -> __m512i {
            self as __m512i
        }
    }
}

pub trait ToU64 {
    fn to_u64(self) -> u64;
}

impl ToU64 for u8 {
    fn to_u64(self) -> u64 {
        self as u64
    }
}

impl ToU64 for u16 {
    fn to_u64(self) -> u64 {
        self as u64
    }
}

impl ToU64 for u32 {
    fn to_u64(self) -> u64 {
        self as u64
    }
}

impl ToU64 for u64 {
    fn to_u64(self) -> u64 {
        self
    }
}
