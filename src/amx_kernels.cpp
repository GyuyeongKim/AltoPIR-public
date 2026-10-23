#include <immintrin.h>
#include <omp.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/mman.h>
#include <sys/syscall.h>
#include <algorithm>
#include <vector>

enum { SW_AMX = 0, SW_VNNI = 2 };

// Clients per chunk of the scan. A chunk of 128 clients already makes the scan bound by the accelerator rather than
// by memory on the evaluated CPU, so a larger chunk would only shrink the block length. 4 limb columns per client,
// 16 columns per query tile.
static const size_t SW_CHUNK_TILES = 4 * 128 / 16;
// Batches of up to two clients use the k-vector VNNI kernel, because the block kernel pads clients to groups of four
// and would spend at least half of its work on padding.
static const size_t SW_KVEC_MAX = 2;
// Packing splits the key length into this many parts, so that the four plane slices of U' for one group of 32
// columns (4 x 32 x K / SW_KH bytes, 512 KB for K = 8192) stay in the L2 cache next to the 256 KB block of sums.
static const size_t SW_KH = 2;
// Largest ring dimension, which sizes the stack buffer of the tail.
static const size_t SW_MAX_N = 4096;

namespace {
struct __attribute__((packed)) TileCfg { uint8_t palette_id, start_row, reserved[14]; uint16_t colsb[16]; uint8_t rows[16]; };
inline void tile_config_full() {
    alignas(64) TileCfg cfg; memset(&cfg, 0, sizeof cfg);
    cfg.palette_id = 1;
    for (int i = 0; i < 8; i++) { cfg.colsb[i] = 64; cfg.rows[i] = 16; }
    _tile_loadconfig(&cfg);
}
}

static inline size_t sw_ubt(size_t j, size_t ct, size_t kc, size_t KC, size_t CT) {
    return ((j * (CT / 2) + ct / 2) * KC + kc) * 2048 + (ct % 2) * 1024;
}
static inline size_t sw_dbt(size_t cb, size_t kc, size_t KC) { return ((cb / 2) * KC + kc) * 2048 + (cb % 2) * 1024; }

// Per-core L2 cache size in bytes, or 0 if the system does not report it.
static size_t sw_l2_bytes() {
    long v = sysconf(_SC_LEVEL2_CACHE_SIZE);
    if (v > 0) return (size_t)v;
    for (int i = 0; i < 16; i++) {
        char path[96]; int level = 0; unsigned long size = 0; char unit = 0;
        snprintf(path, sizeof path, "/sys/devices/system/cpu/cpu0/cache/index%d/level", i);
        FILE *f = fopen(path, "r"); if (!f) break;
        if (fscanf(f, "%d", &level) != 1) level = 0;
        fclose(f);
        if (level != 2) continue;
        snprintf(path, sizeof path, "/sys/devices/system/cpu/cpu0/cache/index%d/size", i);
        if (!(f = fopen(path, "r"))) return 0;
        if (fscanf(f, "%lu%c", &size, &unit) < 1) size = 0;
        fclose(f);
        return unit == 'M' ? size << 20 : unit == 'K' ? size << 10 : size;
    }
    return 0;
}
// Block length of the scan. The active block of query limbs (16 bytes per record and query tile) takes at most half of
// the L2 cache, and the two groups of database tiles in flight, the current one and the prefetched next one (32
// columns each, one byte per record and column), take at most an eighth.
static size_t sw_block_len(size_t l2, size_t nct) {
    const size_t kb = std::min(l2 / 2 / (16 * nct), l2 / 8 / 64) / 64 * 64;
    return kb < 64 ? 64 : kb;
}

template <int NR, int NG>
static inline void vnni_scan_block(uint32_t *acc, const uint8_t *db, size_t ld, const uint8_t *qp, size_t NQT, size_t Bp,
                                   size_t r, size_t nt, size_t k0, size_t k1, int tiled, size_t KC) {
    __m512i a4[NR][NG];
    for (int p = 0; p < NR; p++) for (int j = 0; j < NG; j++) a4[p][j] = _mm512_setzero_si512();
    for (size_t kq = k0 / 4; kq < k1 / 4; kq++) {
        size_t kc = kq >> 4, rq = kq & 15; __m512i w[NG];
        for (int j = 0; j < NG; j++) w[j] = _mm512_loadu_si512(qp + (kc * NQT + nt + j) * 1024 + rq * 64);
        for (int p = 0; p < NR; p++) {
            size_t col = r + p;
            const uint8_t *src = tiled ? db + sw_dbt(col / 16, kc, KC) + (col % 16) * 64 + rq * 4 : db + col * ld + kq * 4;
            __m512i a = _mm512_set1_epi32(*(const int32_t *)src);
            for (int j = 0; j < NG; j++) a4[p][j] = _mm512_dpbusd_epi32(a4[p][j], a, w[j]);
        }
    }
    for (int p = 0; p < NR; p++) for (int j = 0; j < NG; j++) {
        alignas(64) int32_t t[16]; _mm512_store_si512(t, a4[p][j]);
        for (int cc = 0; cc < 4; cc++) {
            size_t c = (nt + j) * 4 + cc;
            uint32_t s = (uint32_t)t[4 * cc] + ((uint32_t)t[4 * cc + 1] << 8) + ((uint32_t)t[4 * cc + 2] << 16) + ((uint32_t)t[4 * cc + 3] << 24);
            uint32_t *dst = acc + (r + p) * Bp + c;
            *dst = k0 == 0 ? s : *dst + s;
        }
    }
}

struct SwMapHdr { void *base; size_t len; };
static void *sw_map(size_t bytes, int advice) {
    const size_t al = 2u << 20, body = (bytes + al - 1) / al * al, len = body + 2 * al;
    void *base = mmap(nullptr, len, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS | MAP_NORESERVE, -1, 0);
    if (base == MAP_FAILED) return nullptr;
    uint8_t *p = (uint8_t *)(((uintptr_t)base + 2 * al - 1) / al * al);
    if ((uintptr_t)p - (uintptr_t)base < sizeof(SwMapHdr)) p += al;
    if (madvise(p, body, advice)) { munmap(base, len); return nullptr; }
    ((SwMapHdr *)p)[-1] = SwMapHdr{base, len};
    return p;
}
extern "C" void *sw_alloc_huge(size_t bytes) {
    void *p = sw_map(bytes, MADV_HUGEPAGE); if (!p) return nullptr;
    memset(p, 0, (bytes + (2u << 20) - 1) / (2u << 20) * (2u << 20)); return p;
}
extern "C" void *sw_alloc_huge_nz(size_t bytes) { return sw_map(bytes, MADV_HUGEPAGE); }
extern "C" void sw_free(void *p) { if (p) { SwMapHdr h = ((SwMapHdr *)p)[-1]; munmap(h.base, h.len); } }

static inline void sw_scan_part(size_t m, int id, int nth, size_t &lo, size_t &hi) { size_t u = m / 32; lo = u * id / nth * 32; hi = u * (id + 1) / nth * 32; }
static inline void sw_pack_part(size_t CT, int id, int nth, size_t &lo, size_t &hi) { size_t p = CT / 2; lo = p * id / nth * 2; hi = p * (id + 1) / nth * 2; }
enum { SW_SLOT_SCAN_ACC, SW_SLOT_PACK_YA, SW_SLOT_PACK_RS, SW_SLOT_INTER, SW_SLOT_BETA, SW_SLOT_N };
static void *sw_scratch(int slot, size_t bytes, int advice) {
    static void *p[SW_SLOT_N]; static size_t cap[SW_SLOT_N];
    if (bytes > cap[slot]) { sw_free(p[slot]); p[slot] = sw_map(bytes, advice); if (!p[slot]) abort(); cap[slot] = bytes; }
    return p[slot];
}

struct SwScanWorkspace {
    uint8_t *qp = nullptr;
    size_t capacity = 0;
    bool huge = false;
    ~SwScanWorkspace() { sw_free(qp); }
    uint8_t *reserve(size_t bytes, bool hp) {
        if (bytes > capacity || hp != huge) {
            size_t sz = (bytes + 4095) / 4096 * 4096;
            auto p = (uint8_t *)sw_map(sz, hp ? MADV_HUGEPAGE : MADV_NOHUGEPAGE);
            if (!p) abort();
            sw_free(qp); qp = p; capacity = sz; huge = hp;
        }
        return qp;
    }
};
extern "C" void sw_tile_db(uint8_t *dst, const uint8_t *db, size_t ld, size_t K, size_t m, int threads) {
    const size_t KC = K / 64;
    #pragma omp parallel num_threads(threads)
    {
        size_t lo, hi; sw_scan_part(m, omp_get_thread_num(), omp_get_num_threads(), lo, hi);
        for (size_t cb = lo / 16; cb < hi / 16; cb++)
            for (size_t r = 0; r < 16; r++) {
                const uint8_t *src = db + (cb * 16 + r) * ld;
                for (size_t kc = 0; kc < KC; kc++) memcpy(dst + sw_dbt(cb, kc, KC) + r * 64, src + kc * 64, 64);
            }
    }
}

extern "C" int sw_amx_init(void) { return (int)syscall(SYS_arch_prctl, 0x1023, 18); }

template <int NC, int NB>
static void vnni_kvec(uint32_t *acc, const uint8_t *db, size_t ld, const int8_t *ql, size_t K, size_t col0, size_t Bp, size_t c0) {
    __m512i a[NC][NB][4];
    for (int c = 0; c < NC; c++) for (int b = 0; b < NB; b++) for (int i = 0; i < 4; i++) a[c][b][i] = _mm512_setzero_si512();
    for (size_t k = 0; k < K; k += 64) {
        __m512i d[NC];
        for (int c = 0; c < NC; c++) d[c] = _mm512_loadu_si512(db + (col0 + c) * ld + k);
        for (int b = 0; b < NB; b++) for (int i = 0; i < 4; i++) {
            const __m512i w = _mm512_loadu_si512(ql + ((c0 + b) * 4 + i) * K + k);
            for (int c = 0; c < NC; c++) a[c][b][i] = _mm512_dpbusd_epi32(a[c][b][i], d[c], w);
        }
    }
    for (int c = 0; c < NC; c++) for (int b = 0; b < NB; b++) {
        uint32_t s = 0;
        for (int i = 0; i < 4; i++) s += (uint32_t)_mm512_reduce_add_epi32(a[c][b][i]) << (8 * i);
        acc[(col0 + c) * Bp + c0 + b] = s;
    }
}

static inline uint32_t sw_ld32(const void *p) { uint32_t v; memcpy(&v, p, 4); return v; }
// The scan reads the tile-order database (tiled = 1, which AMX needs) or the column-order database (tiled = 0, VNNI
// only). Queries are 32-bit values.
static void sw_scan_impl(uint64_t *out, const uint8_t *db, size_t ld, size_t K, size_t m, const void *const *q, size_t B,
                         uint64_t Q, int method, int threads, int tiled, SwScanWorkspace *ws, size_t l2) {
    const size_t Bp = (B + 7) / 8 * 8, N = 4 * Bp, NQT = N / 16, KC = K / 64;
    const bool vn = method == SW_VNNI;
    if (!B || !K || K % 64 || !m || m % 32 || threads <= 0 || (!vn && !tiled)) abort();
    const size_t NCT = vn || SW_CHUNK_TILES >= NQT ? NQT : SW_CHUNK_TILES;
    const size_t KB = sw_block_len(l2, NCT);
    const bool kvec = vn && !tiled && B <= SW_KVEC_MAX;
    const size_t KBC = KB / 64, KCP = vn ? KC : (KC + KBC - 1) / KBC * KBC;
    uint8_t *qp = kvec ? nullptr : ws->reserve(KCP * 64 * N, !vn);
    auto qoff = [&](size_t nt, size_t kc) -> size_t {
        if (vn) return (kc * NQT + nt) * 1024;
        return (((kc / KBC) * NQT + nt) * KBC + kc % KBC) * 1024;
    };
    const __m128i tr = _mm_setr_epi8(0, 4, 8, 12, 1, 5, 9, 13, 2, 6, 10, 14, 3, 7, 11, 15);
    const __m128i bias = _mm_set1_epi32((int)0x80808080u), flip = _mm_set1_epi8((char)0x80);
    if (!kvec)
    #pragma omp parallel num_threads(threads)
    {
        const int id = omp_get_thread_num(), nth = omp_get_num_threads();
        if (nth != threads) abort();
        const size_t items = NQT * KC;
        for (size_t it = items * id / nth; it < items * (id + 1) / nth; it++) {
            const size_t nt = it / KC, kc = it % KC;
            uint8_t *blk = qp + qoff(nt, kc);
            for (int cc = 0; cc < 4; cc++) {
                size_t c = nt * 4 + cc;
                for (int r = 0; r < 16; r++) {
                    __m128i v = _mm_setzero_si128();
                    if (c < B) v = _mm_loadu_si128((const __m128i *)((const uint32_t *)q[c] + kc * 64 + 4 * r));
                    if (vn) v = _mm_xor_si128(_mm_add_epi32(v, bias), flip);
                    _mm_storeu_si128((__m128i *)(blk + r * 64 + 16 * cc), _mm_shuffle_epi8(v, tr));
                }
            }
        }
    }
    uint32_t *acc = (uint32_t *)sw_scratch(SW_SLOT_SCAN_ACC, sizeof(uint32_t) * m * Bp, MADV_NOHUGEPAGE);
    if (kvec) {
        int8_t *ql = (int8_t *)aligned_alloc(64, B * 4 * K);
        #pragma omp parallel for num_threads(threads) schedule(static)
        for (size_t ck = 0; ck < B * (K / 64); ck++) {
            const size_t c = ck / (K / 64), k0 = ck % (K / 64) * 64;
            for (size_t k = k0; k < k0 + 64; k++) {
                const uint32_t v = sw_ld32((const uint32_t *)q[c] + k) + 0x80808080u;
                for (int i = 0; i < 4; i++) ql[(c * 4 + i) * K + k] = (int8_t)((int)((v >> (8 * i)) & 0xff) - 128);
            }
        }
        #pragma omp parallel num_threads(threads)
        {
            size_t lo, hi; sw_scan_part(m, omp_get_thread_num(), omp_get_num_threads(), lo, hi);
            for (size_t col = lo; col < hi; ) {
                size_t c0 = 0;
                for (; c0 + 4 <= B; c0 += 4) vnni_kvec<1, 4>(acc, db, ld, ql, K, col, Bp, c0);
                if (B - c0 == 3) vnni_kvec<1, 3>(acc, db, ld, ql, K, col, Bp, c0);
                else if (B - c0 == 2) { if (hi - col >= 2 && c0 == 0) { vnni_kvec<2, 2>(acc, db, ld, ql, K, col, Bp, 0); col += 2; continue; } vnni_kvec<1, 2>(acc, db, ld, ql, K, col, Bp, c0); }
                else if (B - c0 == 1) { if (hi - col >= 4 && c0 == 0) { vnni_kvec<4, 1>(acc, db, ld, ql, K, col, Bp, 0); col += 4; continue; } vnni_kvec<1, 1>(acc, db, ld, ql, K, col, Bp, c0); }
                col += 1;
            }
            for (size_t col = lo; col < hi; col++) for (size_t c = 0; c < B; c++)
                out[c * m + col] = ((uint64_t)acc[col * Bp + c] * Q + (1ull << 31)) >> 32;
        }
        free(ql);
        return;
    }
    #pragma omp parallel num_threads(threads)
    {
        int id = omp_get_thread_num(), nth = omp_get_num_threads();
        size_t lo, hi; sw_scan_part(m, id, nth, lo, hi);
        if (!vn) tile_config_full();
        alignas(64) int32_t cbuf[32 * 32];
        for (size_t nc0 = 0; nc0 < NQT; nc0 += NCT) {
        const size_t nc1 = nc0 + NCT < NQT ? nc0 + NCT : NQT;
        for (size_t k0 = 0; k0 < K; k0 += KB) {
            size_t k1 = k0 + KB < K ? k0 + KB : K;
            for (size_t r0 = lo; r0 < hi; r0 += 32) {
                if (vn) {
                    const size_t groups = (B + 3) / 4;
                    for (size_t rr = 0; rr < 32; rr += 4)
                        for (size_t nt = 0; nt < groups; nt += 4) {
                            size_t ng = groups - nt < 4 ? groups - nt : 4;
                            if (ng == 4) vnni_scan_block<4, 4>(acc, db, ld, qp, NQT, Bp, r0 + rr, nt, k0, k1, tiled, KC);
                            else if (ng == 3) vnni_scan_block<4, 3>(acc, db, ld, qp, NQT, Bp, r0 + rr, nt, k0, k1, tiled, KC);
                            else if (ng == 2) vnni_scan_block<4, 2>(acc, db, ld, qp, NQT, Bp, r0 + rr, nt, k0, k1, tiled, KC);
                            else vnni_scan_block<4, 1>(acc, db, ld, qp, NQT, Bp, r0 + rr, nt, k0, k1, tiled, KC);
                        }
                    continue;
                }
                // Prefetch the database tiles of the next group of 32 columns (or of the first group of the next block)
                // into the L2 cache, spread over the tile steps of this group.
                const char *pf_base = nullptr; size_t pf_n = 0, pf_per = 0;
                {
                    size_t nr = r0 + 32, nk0 = k0, nk1 = k1;
                    if (nr >= hi) { nr = lo; nk0 = k0 + KB; nk1 = nk0 + KB < K ? nk0 + KB : K; }
                    if (nk0 < K) {
                        pf_base = (const char *)db + sw_dbt(nr / 16, nk0 / 64, KC);
                        pf_n = 2 * (nk1 - nk0) / 64 * 16;
                        pf_per = (pf_n + ((nc1 - nc0) / 2) * (k1 / 64 - k0 / 64) - 1) / (((nc1 - nc0) / 2) * (k1 / 64 - k0 / 64));
                    }
                }
                for (size_t nt = nc0; nt < nc1; nt += 2) {
                    _tile_zero(0); _tile_zero(1); _tile_zero(2); _tile_zero(3);
                    for (size_t kc = k0 / 64; kc < k1 / 64; kc++) {
                        _tile_loadd(4, db + sw_dbt(r0 / 16, kc, KC), 64);
                        _tile_loadd(5, db + sw_dbt(r0 / 16, kc, KC) + 1024, 64);
                        _tile_loadd(6, qp + qoff(nt, kc), 64);
                        _tile_loadd(7, qp + qoff(nt + 1, kc), 64);
                        if (pf_n) {
                            size_t s0 = (((nt - nc0) / 2) * (k1 / 64 - k0 / 64) + kc - k0 / 64) * pf_per;
                            for (size_t l = s0; l < s0 + pf_per && l < pf_n; l++) _mm_prefetch(pf_base + l * 64, _MM_HINT_T1);
                        }
                        _tile_dpbuud(0, 4, 6); _tile_dpbuud(1, 4, 7); _tile_dpbuud(2, 5, 6); _tile_dpbuud(3, 5, 7);
                    }
                    _tile_stored(0, cbuf, 128); _tile_stored(1, cbuf + 16, 128); _tile_stored(2, cbuf + 512, 128); _tile_stored(3, cbuf + 528, 128);
                    const __m512i sh = _mm512_setr_epi32(1, 256, 65536, 16777216, 1, 256, 65536, 16777216, 1, 256, 65536, 16777216, 1, 256, 65536, 16777216);
                    for (int r = 0; r < 32; r++) {
                        __m512i y0 = _mm512_mullo_epi32(_mm512_load_si512(cbuf + r * 32), sh);
                        __m512i y1 = _mm512_mullo_epi32(_mm512_load_si512(cbuf + r * 32 + 16), sh);
                        y0 = _mm512_add_epi32(y0, _mm512_shuffle_epi32(y0, _MM_PERM_CDAB)); y0 = _mm512_add_epi32(y0, _mm512_shuffle_epi32(y0, _MM_PERM_BADC));
                        y1 = _mm512_add_epi32(y1, _mm512_shuffle_epi32(y1, _MM_PERM_CDAB)); y1 = _mm512_add_epi32(y1, _mm512_shuffle_epi32(y1, _MM_PERM_BADC));
                        __m128i s0 = _mm512_castsi512_si128(_mm512_maskz_compress_epi32(0x1111, y0));
                        __m128i s1 = _mm512_castsi512_si128(_mm512_maskz_compress_epi32(0x1111, y1));
                        __m256i sv = _mm256_inserti128_si256(_mm256_castsi128_si256(s0), s1, 1);
                        uint32_t *dst = acc + (r0 + r) * Bp + nt * 4;
                        if (k0 == 0) _mm256_storeu_si256((__m256i *)dst, sv);
                        else _mm256_storeu_si256((__m256i *)dst, _mm256_add_epi32(_mm256_loadu_si256((const __m256i *)dst), sv));
                    }
                }
            }
        }
        }
        if (!vn) _tile_release();
        for (size_t col = lo; col < hi; col++) for (size_t c = 0; c < B; c++)
            out[c * m + col] = ((uint64_t)acc[col * Bp + c] * Q + (1ull << 31)) >> 32;
    }
}

extern "C" void sw_pack_build(int8_t *ub, const uint64_t *const *bold_t, const uint64_t *const *bold_t_bar, const uint32_t *tables,
                              const uint32_t *gen_pows, size_t num_iter, size_t tg, size_t n, size_t rho, uint64_t Q,
                              int threads) {
    const size_t K = tg * n, KC = K / 64, CT = rho * n / 16;
    #pragma omp parallel num_threads(threads)
    {
        uint64_t *col = (uint64_t *)malloc(sizeof(uint64_t) * K);
        #pragma omp for schedule(dynamic, 16)
        for (size_t oz = 0; oz < rho * n; oz++) {
            size_t o = oz / n, z = oz % n;
            memset(col, 0, sizeof(uint64_t) * K);
            for (size_t i = 0; i < num_iter; i++) {
                uint32_t gp = gen_pows[i];
                uint32_t zp = tables[((gp - 1) / 2) * n + z], zb = tables[((2 * n - gp - 1) / 2) * n + z];
                for (size_t k = 0; k < tg; k++) {
                    size_t idx = (i * tg + k) * n + z;
                    col[k * n + zp] += bold_t[o][idx];
                    col[k * n + zb] += bold_t_bar[o][idx];
                }
            }
            size_t ct = oz / 16, nn = oz % 16;
            for (size_t kk = 0; kk < K; kk++) {
                uint32_t v = (uint32_t)(col[kk] % Q);
                size_t kc = kk / 64, r = (kk % 64) / 4, t = kk % 4;
                for (int j = 0; j < 4; j++)
                    ub[sw_ubt(j, ct, kc, KC, CT) + r * 64 + 4 * nn + t] = (int8_t)((int)((v >> (8 * j)) & 0xff) - 128);
            }
        }
        free(col);
    }
}

// Packing of the keys y (32-bit values) with U'. The key length K must be a multiple of 64 x SW_KH.
static int sw_pack_impl(uint64_t *beta, const int8_t *ub, const void *const *y, size_t B, size_t K, size_t ncols,
                        uint64_t Q, int method, int threads) {
    const size_t Bp = (B + 7) / 8 * 8, R = 4 * Bp, KC = K / 64, CT = ncols / 16;
    if (K % (64 * SW_KH)) abort();
    const bool vn = method == SW_VNNI;
    uint8_t *ya = (uint8_t *)sw_scratch(SW_SLOT_PACK_YA, R * K, vn ? MADV_NOHUGEPAGE : MADV_HUGEPAGE);
    int64_t *rs = (int64_t *)sw_scratch(SW_SLOT_PACK_RS, sizeof(int64_t) * R, MADV_NOHUGEPAGE);
    int ybad = 0;
    #pragma omp parallel num_threads(threads) reduction(| : ybad)
    {
        const int id = omp_get_thread_num(), nth = omp_get_num_threads();
        for (size_t row = R * id / nth; row < R * (id + 1) / nth; row++) {
            size_t c = row / 4; int i = (int)(row % 4); int64_t s = 0;
            for (size_t k = 0; k < K; k++) {
                uint8_t b = 0;
                if (c < B) { const uint64_t v = sw_ld32((const uint32_t *)y[c] + k); ybad |= v >= Q; b = (uint8_t)((v >> (8 * i)) & 0xff); }
                ya[vn ? row * K + k : sw_dbt(row / 16, k / 64, KC) + (row % 16) * 64 + k % 64] = b; s += b;
            }
            rs[row] = 128 * s;
        }
    }
    uint64_t sm[7]; for (int s = 0; s < 7; s++) sm[s] = (uint64_t)(((unsigned __int128)1 << (8 * s)) % Q);
    #pragma omp parallel num_threads(threads)
    {
        int id = omp_get_thread_num(), nth = omp_get_num_threads();
        size_t lo, hi; sw_pack_part(CT, id, nth, lo, hi);
        if (!vn) tile_config_full();
        int32_t *g = (int32_t *)aligned_alloc(64, sizeof(int32_t) * 4 * R * 32);
        for (size_t ct = lo; ct < hi; ct += 2) {
            if (!vn) {
                const size_t KP = KC / SW_KH;
                for (size_t kh = 0; kh < SW_KH; kh++) {
                    for (size_t r0 = 0; r0 < R; r0 += 32)
                        for (int j = 0; j < 4; j++) {
                            int32_t *gj = g + (size_t)j * R * 32;
                            int32_t *c0 = gj + r0 * 32, *c1 = gj + (r0 + 16) * 32;
                            const int8_t *pb = ub + sw_ubt(j, ct, kh * KP, KC, CT);
                            const uint8_t *pa = ya + sw_dbt(r0 / 16, kh * KP, KC);
                            if (kh == 0) { _tile_zero(0); _tile_zero(1); _tile_zero(2); _tile_zero(3); }
                            else { _tile_loadd(0, c0, 128); _tile_loadd(1, c0 + 16, 128); _tile_loadd(2, c1, 128); _tile_loadd(3, c1 + 16, 128); }
                            for (size_t kc = 0; kc < KP; kc++, pa += 2048, pb += 2048) {
                                _tile_loadd(4, pa, 64); _tile_loadd(5, pa + 1024, 64);
                                _tile_loadd(6, pb, 64); _tile_loadd(7, pb + 1024, 64);
                                _tile_dpbusd(0, 4, 6); _tile_dpbusd(1, 4, 7); _tile_dpbusd(2, 5, 6); _tile_dpbusd(3, 5, 7);
                            }
                            _tile_stored(0, c0, 128); _tile_stored(1, c0 + 16, 128); _tile_stored(2, c1, 128); _tile_stored(3, c1 + 16, 128);
                        }
                }
            } else
            for (int j = 0; j < 4; j++) {
                int32_t *gj = g + (size_t)j * R * 32;
                for (size_t r0 = 0; r0 < 4 * B; r0 += 4) {
                    __m512i a4[4][2];
                    for (int p = 0; p < 4; p++) for (int h = 0; h < 2; h++) a4[p][h] = _mm512_setzero_si512();
                    for (size_t kq = 0; kq < K / 4; kq++) {
                        size_t kc = kq >> 4, r = kq & 15;
                        __m512i w0 = _mm512_loadu_si512(ub + sw_ubt(j, ct, kc, KC, CT) + r * 64), w1 = _mm512_loadu_si512(ub + sw_ubt(j, ct + 1, kc, KC, CT) + r * 64);
                        for (int p = 0; p < 4; p++) {
                            __m512i a = _mm512_set1_epi32(*(const int32_t *)(ya + (r0 + p) * K + kq * 4));
                            a4[p][0] = _mm512_dpbusd_epi32(a4[p][0], a, w0); a4[p][1] = _mm512_dpbusd_epi32(a4[p][1], a, w1);
                        }
                    }
                    for (int p = 0; p < 4; p++) { _mm512_store_si512(gj + (r0 + p) * 32, a4[p][0]); _mm512_store_si512(gj + (r0 + p) * 32 + 16, a4[p][1]); }
                }
            }
            if (Q == 4294955009ull && K <= 8192) {
                const __m512i mlo = _mm512_set1_epi64(0xffffffffll), c12287 = _mm512_set1_epi64(12287), vq = _mm512_set1_epi64((long long)Q);
                const __m512i w4 = _mm512_set1_epi64((long long)sm[4]), w5 = _mm512_set1_epi64((long long)sm[5]), w6 = _mm512_set1_epi64((long long)sm[6]);
                for (size_t c = 0; c < B; c++)
                    for (int cb = 0; cb < 32; cb += 8) {
                        __m512i V[7];
                        for (int t = 0; t < 7; t++) V[t] = _mm512_setzero_si512();
                        for (int i = 0; i < 4; i++) {
                            const __m256i rsv = _mm256_set1_epi32((int)rs[4 * c + i]);
                            for (int j = 0; j < 4; j++) {
                                __m256i v = _mm256_add_epi32(_mm256_load_si256((const __m256i *)(g + ((size_t)j * R + 4 * c + i) * 32 + cb)), rsv);
                                V[i + j] = _mm512_add_epi64(V[i + j], _mm512_cvtepu32_epi64(v));
                            }
                        }
                        __m512i t = _mm512_add_epi64(_mm512_add_epi64(V[0], _mm512_slli_epi64(V[1], 8)), _mm512_add_epi64(_mm512_slli_epi64(V[2], 16), _mm512_slli_epi64(V[3], 24)));
                        t = _mm512_add_epi64(t, _mm512_add_epi64(_mm512_mul_epu32(V[4], w4), _mm512_add_epi64(_mm512_mul_epu32(V[5], w5), _mm512_mul_epu32(V[6], w6))));
                        t = _mm512_add_epi64(_mm512_mul_epu32(_mm512_srli_epi64(t, 32), c12287), _mm512_and_si512(t, mlo));
                        t = _mm512_add_epi64(_mm512_mul_epu32(_mm512_srli_epi64(t, 32), c12287), _mm512_and_si512(t, mlo));
                        t = _mm512_mask_sub_epi64(t, _mm512_cmpge_epu64_mask(t, vq), t, vq);
                        _mm512_storeu_si512(beta + c * ncols + ct * 16 + cb, t);
                    }
            } else
            for (size_t c = 0; c < B; c++) for (int cc = 0; cc < 32; cc++) {
                unsigned __int128 s = 0;
                for (int i = 0; i < 4; i++) for (int j = 0; j < 4; j++) {
                    int64_t v = (int64_t)g[((size_t)j * R + 4 * c + i) * 32 + cc] + rs[4 * c + i];
                    s += (unsigned __int128)(uint64_t)v * sm[i + j];
                }
                beta[c * ncols + ct * 16 + cc] = (uint64_t)(s % Q);
            }
        }
        free(g);
        if (!vn) _tile_release();
    }
    return ybad ? -1 : 0;
}

static inline __m512i mq_red64(__m512i p, __m512i qv) {
    const __m512i M = _mm512_set1_epi64(0xffffffffll), C = _mm512_set1_epi64(12287);
    p = _mm512_add_epi64(_mm512_mul_epu32(_mm512_srli_epi64(p, 32), C), _mm512_and_si512(p, M));
    p = _mm512_add_epi64(_mm512_mul_epu32(_mm512_srli_epi64(p, 32), C), _mm512_and_si512(p, M));
    return _mm512_mask_sub_epi64(p, _mm512_cmpge_epu64_mask(p, qv), p, qv);
}
static inline void mq_bfly(__m512i &x, __m512i &y, __m512i w, __m512i qv) {
    __m512i s = _mm512_add_epi64(x, y);
    s = _mm512_mask_sub_epi64(s, _mm512_cmpge_epu64_mask(s, qv), s, qv);
    s = _mm512_srli_epi64(_mm512_add_epi64(s, _mm512_maskz_mov_epi64(_mm512_test_epi64_mask(s, _mm512_set1_epi64(1)), qv)), 1);
    __m512i d = _mm512_add_epi64(_mm512_sub_epi64(x, y), qv);
    d = _mm512_mask_sub_epi64(d, _mm512_cmpge_epu64_mask(d, qv), d, qv);
    x = s; y = mq_red64(_mm512_mul_epu32(d, w), qv);
}
static int sw_intt_q(uint64_t *a, const uint64_t *table, size_t n) {
    const uint64_t Q = 4294955009ull;
    if (n < 16 || (n & (n - 1))) return -1;
    const __m512i qv = _mm512_set1_epi64((long long)Q);
    for (size_t i = 0; i < n; i += 8)
        _mm512_storeu_si512(a + i, mq_red64(_mm512_loadu_si512(a + i), qv));
    static const long long X1[8] = {0, 2, 4, 6, 8, 10, 12, 14}, X2[8] = {0, 1, 4, 5, 8, 9, 12, 13}, X4[8] = {0, 1, 2, 3, 8, 9, 10, 11};
    const long long *XS[3] = {X1, X2, X4};
    size_t logn = 0; while ((1ull << logn) < n) logn++;
    for (int st = 0; st < 3; st++) {
        const size_t t = 1ull << st, h = n / (2 * t);
        const __m512i ix = _mm512_loadu_si512(XS[st]), iy = _mm512_add_epi64(ix, _mm512_set1_epi64((long long)t));
        alignas(64) long long inv[16], wi[8];
        for (int k = 0; k < 8; k++) { inv[XS[st][k]] = k; inv[XS[st][k] + t] = 8 + k; wi[k] = XS[st][k] / (2 * t); }
        const __m512i i0 = _mm512_load_si512(inv), i1 = _mm512_load_si512(inv + 8), iw = _mm512_load_si512(wi);
        for (size_t base = 0; base < n; base += 16) {
            __m512i v0 = _mm512_loadu_si512(a + base), v1 = _mm512_loadu_si512(a + base + 8);
            __m512i x = _mm512_permutex2var_epi64(v0, ix, v1), y = _mm512_permutex2var_epi64(v0, iy, v1);
            __m512i w = _mm512_permutexvar_epi64(iw, _mm512_loadu_si512(table + h + base / (2 * t)));
            mq_bfly(x, y, w, qv);
            _mm512_storeu_si512(a + base, _mm512_permutex2var_epi64(x, i0, y));
            _mm512_storeu_si512(a + base + 8, _mm512_permutex2var_epi64(x, i1, y));
        }
    }
    for (size_t mm = logn - 3; mm-- > 0;) {
        const size_t h = 1ull << mm, t = n >> (mm + 1);
        for (size_t i = 0; i < h; i++) {
            const __m512i w = _mm512_set1_epi64((long long)table[h + i]);
            uint64_t *op = a + 2 * t * i;
            for (size_t j = 0; j < t; j += 8) {
                __m512i x = _mm512_loadu_si512(op + j), y = _mm512_loadu_si512(op + t + j);
                mq_bfly(x, y, w, qv);
                _mm512_storeu_si512(op + j, x); _mm512_storeu_si512(op + t + j, y);
            }
        }
    }
    return 0;
}

static int sw_tail_ntt(uint64_t *acc, const uint64_t *beta, const uint32_t *z, const uint64_t *T, size_t len,
                       const uint64_t *table, size_t n) {
    const __m512i qv = _mm512_set1_epi64(4294955009ll);
    for (size_t i = 0; i < n; i += 8) {
        __m512i s = mq_red64(_mm512_loadu_si512(beta + i), qv);
        for (size_t k = 0; k < len; k++) {
            __m512i a = mq_red64(_mm512_cvtepu32_epi64(_mm256_loadu_si256((const __m256i *)(z + k * n + i))), qv),
                    b = mq_red64(_mm512_loadu_si512(T + k * n + i), qv);
            s = _mm512_add_epi64(s, mq_red64(_mm512_mul_epu32(a, b), qv));
        }
        _mm512_storeu_si512(acc + i, s);
    }
    return sw_intt_q(acc, table, n);
}

static int sw_switch_row1(uint8_t *res, size_t res_len, size_t bit_off, const uint64_t *b, const uint64_t *acc, size_t n,
                          uint64_t Q, uint64_t q1, int q1_bits) {
    if (Q != 4294955009ull || q1 >= (1ull << 21) || n % 8 || (bit_off + n * q1_bits + 7) / 8 > res_len || q1_bits > 24) return -1;
    alignas(64) int64_t r[8];
    const __m512i qv = _mm512_set1_epi64((long long)Q), half = _mm512_set1_epi64((long long)(Q / 2)), om = _mm512_set1_epi64((long long)q1);
    const __m512d qd = _mm512_set1_pd((double)Q), q1d = _mm512_set1_pd((double)q1);
    for (size_t z = 0; z < n; z += 8) {
        __m512i v = _mm512_add_epi64(_mm512_loadu_si512(b + z), _mm512_loadu_si512(acc + z));
        v = mq_red64(v, qv);
        __mmask8 neg = _mm512_cmpge_epu64_mask(v, half);
        __m512i c = _mm512_mask_sub_epi64(v, neg, v, qv);
        __m512d x = _mm512_fmadd_pd(_mm512_cvtepi64_pd(c), q1d, _mm512_mask_sub_pd(_mm512_cvtepi64_pd(half), neg, _mm512_setzero_pd(), _mm512_cvtepi64_pd(half)));
        __m512i t = _mm512_cvttpd_epi64(_mm512_div_pd(x, qd));
        t = _mm512_sub_epi64(t, _mm512_mullo_epi64(_mm512_srai_epi64(t, 63), om));
        t = _mm512_mask_sub_epi64(t, _mm512_cmpge_epi64_mask(t, om), t, om);
        _mm512_store_si512(r, t);
        for (int l = 0; l < 8; l++) {
            size_t p = bit_off + (z + l) * q1_bits; uint32_t w = (uint32_t)r[l] << (p % 8);
            for (int k = 0; k < 4 && p / 8 + k < res_len; k++) res[p / 8 + k] |= (uint8_t)(w >> (8 * k));
        }
    }
    return 0;
}

struct AltoEngine {
    const uint8_t *db_col, *db_tiled; size_t ld, rows, cols;
    const int8_t *ub; size_t n, tg, outputs;
    std::vector<std::vector<uint8_t>> prefix; size_t part_len, row1_off; uint64_t q1; int q1_bits;
    std::vector<const uint64_t *> that; const uint64_t *inv_table;
    uint64_t Q; int threads; size_t l2; SwScanWorkspace ws;
};
extern "C" void *alto_engine_new(const uint8_t *db_col, const uint8_t *db_tiled, size_t ld, size_t rows, size_t cols,
                                 const int8_t *ub, size_t n, size_t tg, const uint8_t *const *prefix, size_t part_len,
                                 size_t row1_off, uint64_t q1, int q1_bits, const uint64_t *const *that,
                                 const uint64_t *inv_table, uint64_t Q, int threads) {
    const size_t l2 = sw_l2_bytes();
    if (!db_col || !db_tiled || !ub || !prefix || !that || !inv_table || Q != 4294955009ull || !n || n > SW_MAX_N || n % 16 ||
        cols % n || rows % 64 || cols % 32 || threads <= 0 || (tg * n) % (64 * SW_KH) || !l2) return nullptr;
    auto *e = new AltoEngine;
    e->db_col = db_col; e->db_tiled = db_tiled; e->ld = ld; e->rows = rows; e->cols = cols;
    e->ub = ub; e->n = n; e->tg = tg; e->outputs = cols / n;
    for (size_t o = 0; o < e->outputs; o++) { e->prefix.emplace_back(prefix[o], prefix[o] + part_len); e->that.push_back(that[o]); }
    e->part_len = part_len; e->row1_off = row1_off; e->q1 = q1; e->q1_bits = q1_bits;
    e->inv_table = inv_table; e->Q = Q; e->threads = threads; e->l2 = l2;
    return e;
}
extern "C" void alto_engine_free(void *p) { delete (AltoEngine *)p; }
extern "C" size_t alto_request_len(const void *p) { auto *e = (const AltoEngine *)p; return 9 + 4 * (2 * e->tg * e->n + e->rows); }
extern "C" size_t alto_response_len(const void *p) { auto *e = (const AltoEngine *)p; return e->outputs * e->part_len; }
extern "C" int alto_answer(void *p, const uint8_t *const *req, const size_t *len, size_t B, uint8_t *const *out,
                           int *status, int method, double *stage_ms) {
    auto *e = (AltoEngine *)p;
    if (!e || !req || !len || !out || !status || (method != SW_AMX && method != SW_VNNI)) return -1;
    const size_t count = e->tg * e->n, want = 9 + 4 * (2 * count + e->rows);
    const uint32_t Qv = (uint32_t)e->Q;
    double t0 = omp_get_wtime();
    #pragma omp parallel for num_threads(e->threads) schedule(dynamic, 1)
    for (size_t c = 0; c < B; c++) {
        const uint8_t *f = req[c];
        if (!f || len[c] != want) { status[c] = 1; continue; }
        if (f[0] != 0 || sw_ld32(f + 1) != count || sw_ld32(f + 5) != count) { status[c] = 2; continue; }
        const __m512i qq = _mm512_set1_epi32((int)Qv);
        __mmask16 bad = 0;
        for (size_t i = 0; i < 2 * count; i += 16)
            bad |= _mm512_cmpge_epu32_mask(_mm512_loadu_si512(f + 9 + 4 * i), qq);
        status[c] = bad ? 3 : 0;
    }
    std::vector<const void *> qs, ys; std::vector<const uint32_t *> zs; std::vector<uint8_t *> os;
    for (size_t c = 0; c < B; c++) if (!status[c]) {
        ys.push_back(req[c] + 9); zs.push_back((const uint32_t *)(req[c] + 9 + 4 * count)); qs.push_back(req[c] + 9 + 8 * count);
        os.push_back(out[c]);
    }
    const size_t Bv = qs.size();
    double t1 = omp_get_wtime(), t2 = t1, t3 = t1, t4 = t1;
    if (Bv) {
        uint64_t *inter = (uint64_t *)sw_scratch(SW_SLOT_INTER, sizeof(uint64_t) * Bv * e->cols, MADV_NOHUGEPAGE);
        uint64_t *beta = (uint64_t *)sw_scratch(SW_SLOT_BETA, sizeof(uint64_t) * Bv * e->cols, MADV_NOHUGEPAGE);
        if (method == SW_AMX) sw_scan_impl(inter, e->db_tiled, e->ld, e->rows, e->cols, qs.data(), Bv, e->Q, SW_AMX, e->threads, 1, &e->ws, e->l2);
        else sw_scan_impl(inter, e->db_col, e->ld, e->rows, e->cols, qs.data(), Bv, e->Q, SW_VNNI, e->threads, 0, &e->ws, e->l2);
        t2 = omp_get_wtime();
        if (sw_pack_impl(beta, e->ub, ys.data(), Bv, count, e->cols, e->Q, method, e->threads)) return -2;
        t3 = omp_get_wtime();
        int bad = 0;
        #pragma omp parallel num_threads(e->threads) reduction(| : bad)
        {
            alignas(64) uint64_t acc[SW_MAX_N];
            #pragma omp for schedule(static)
            for (size_t pi = 0; pi < Bv * e->outputs; pi++) {
                const size_t c = pi / e->outputs, o = pi % e->outputs, off = c * e->cols + o * e->n;
                uint8_t *res = os[c] + o * e->part_len;
                memcpy(res, e->prefix[o].data(), e->part_len);
                bad |= sw_tail_ntt(acc, beta + off, zs[c], e->that[o], e->tg, e->inv_table, e->n) != 0;
                bad |= sw_switch_row1(res, e->part_len, e->row1_off, inter + off, acc, e->n, e->Q, e->q1, e->q1_bits) != 0;
            }
        }
        if (bad) return -3;
        t4 = omp_get_wtime();
    }
    if (stage_ms) { stage_ms[0] = 1e3 * (t1 - t0); stage_ms[1] = 1e3 * (t2 - t1); stage_ms[2] = 1e3 * (t3 - t2); stage_ms[3] = 1e3 * (t4 - t3); }
    return (int)Bv;
}
