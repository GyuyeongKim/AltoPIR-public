use log::debug;
use rand::distributions::Distribution;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;

use spiral_rs::aligned_memory::AlignedMemory64;
use spiral_rs::{
    arith::*, client::*, discrete_gaussian::*, gadget::*, number_theory::*, params::*, poly::*,
};

use crate::convolution::negacyclic_matrix_u32;
use crate::packing::PackingType;
use crate::{lwe::*, noise_analysis::measure_noise_width_squared, scheme::*, util::*};

pub fn rlwe_to_lwe<'a>(params: &'a Params, ct: &PolyMatrixRaw<'a>) -> Vec<u64> {
    let a = ct.get_poly(0, 0);
    let mut negacylic_a = negacyclic_matrix(&a, params.modulus);
    negacylic_a.extend(ct.get_poly(1, 0));

    negacylic_a
}

/// Returns only the last row of the LWE ciphertext (the 'b' scalar).
pub fn rlwe_to_lwe_last_row<'a>(_params: &'a Params, ct: &PolyMatrixRaw<'a>) -> Vec<u64> {
    ct.get_poly(1, 0).to_vec()
}

pub fn pack_query(params: &Params, query: &[u64]) -> AlignedMemory64 {
    let query_packed = query
        .iter()
        .enumerate()
        .map(|(_i, x)| {
            let crt0 = (*x) % params.moduli[0];
            let crt1 = (*x) % params.moduli[1];
            crt0 | (crt1 << 32)
        })
        .collect::<Vec<_>>();
    let mut aligned_query_packed = AlignedMemory64::new(query_packed.len());
    aligned_query_packed
        .as_mut_slice()
        .copy_from_slice(&query_packed);
    aligned_query_packed
}

pub fn get_reg_sample<'a>(
    params: &'a Params,
    sk_reg: &PolyMatrixRaw<'a>,
    rng: &mut ChaCha20Rng,
    rng_pub: &mut ChaCha20Rng,
) -> PolyMatrixNTT<'a> {
    let a = PolyMatrixRaw::random_rng(params, 1, 1, rng_pub);
    let e = PolyMatrixRaw::noise(
        params,
        1,
        1,
        &DiscreteGaussian::init(params.noise_width),
        rng,
    );
    let b_p = &sk_reg.ntt() * &a.ntt();
    let b = &e.ntt() + &b_p;
    let mut p = PolyMatrixNTT::zero(params, 2, 1);
    p.copy_into(&(-&a).ntt(), 0, 0);
    p.copy_into(&b, 1, 0);
    p
}

pub fn get_fresh_reg_public_key<'a>(
    params: &'a Params,
    sk_reg: &PolyMatrixRaw<'a>,
    m: usize,
    rng: &mut ChaCha20Rng,
    rng_pub: &mut ChaCha20Rng,
) -> PolyMatrixNTT<'a> {
    let mut p = PolyMatrixNTT::zero(params, 2, m);

    for i in 0..m {
        p.copy_into(&get_reg_sample(params, sk_reg, rng, rng_pub), 0, i);
    }
    p
}

pub fn raw_generate_expansion_params<'a>(
    params: &'a Params,
    sk_reg: &PolyMatrixRaw<'a>,
    num_exp: usize,
    m_exp: usize,
    rng: &mut ChaCha20Rng,
    rng_pub: &mut ChaCha20Rng,
) -> Vec<PolyMatrixNTT<'a>> {
    let g_exp = build_gadget(params, 1, m_exp);
    debug!("using gadget base {}", g_exp.get_poly(0, 1)[0]);
    let g_exp_ntt = g_exp.ntt();
    let mut res = Vec::new();

    for i in 0..num_exp { // generates log D automorphism keys
        let t = (params.poly_len / (1 << i)) + 1;
        let tau_sk_reg = automorph_alloc(&sk_reg, t);
        let prod = &tau_sk_reg.ntt() * &g_exp_ntt; // PolyMatrixNTT mult overloaded

        // let w_exp_i = client.encrypt_matrix_reg(&prod, rng, rng_pub);
        let sample = get_fresh_reg_public_key(params, &sk_reg, m_exp, rng, rng_pub);
        let w_exp_i = &sample + &prod.pad_top(1); //PolyMatrixNTT add overloaded
        res.push(w_exp_i);
    }

    res
}

pub fn decrypt_ct_reg_measured<'a>(
    client: &Client<'a>,
    params: &'a Params,
    ct: &PolyMatrixNTT<'a>,
    coeffs_to_measure: usize,
) -> PolyMatrixRaw<'a> {
    let dec_result = client.decrypt_matrix_reg(ct).raw();

    let mut dec_rescaled = PolyMatrixRaw::zero(&params, dec_result.rows, dec_result.cols);
    // rounding operation
    for z in 0..dec_rescaled.data.len() {
        dec_rescaled.data[z] = rescale(dec_result.data[z], params.modulus, params.pt_modulus);
    }

    // measure noise width
    let s_2 = measure_noise_width_squared(params, client, ct, &dec_rescaled, coeffs_to_measure);
    debug!("log2(measured noise): {}", s_2.log2());

    dec_rescaled
}

pub struct YClient<'p, 'c> {
    inner: &'c mut Client<'p>,
    params: &'p Params,
    lwe_client: LWEClient,
}

pub fn get_seed(public_seed_idx: u8) -> [u8; 32] {
    let mut seed = STATIC_PUBLIC_SEED;
    seed[0] = public_seed_idx;
    seed
}

pub fn generate_matrix_ring(
    rng_pub: &mut ChaCha20Rng,
    n: usize,
    rows: usize,
    cols: usize,
) -> Vec<u32> {
    assert_eq!(rows % n, 0);
    assert_eq!(cols % n, 0);
    let rows_outer = rows / n;
    let cols_outer = cols / n;

    let mut out = vec![0u32; rows * cols];
    for i in 0..rows_outer {
        for j in 0..cols_outer {
            let mut a = vec![0u32; n];
            for idx in 0..n {
                a[idx] = rng_pub.sample::<u32, _>(rand::distributions::Standard);
            }

            let mat = negacyclic_matrix_u32(&a);
            for k in 0..n {
                for l in 0..n {
                    let idx = (i * n + k) * cols + (j * n + l);
                    out[idx] = mat[k * n + l];
                }
            }
        }
    }

    out
}

/// Generate a poly_len × db_rows matrix of random u64 values via ChaCha20.
/// Stored row-major: A[i * db_rows + j] for row i, col j.
pub fn generate_pseudorandom_matrix_word(
    seed: u8,
    poly_len: usize,
    db_rows: usize,
) -> Vec<u64> {
    assert_eq!(db_rows % poly_len, 0);
    let num_blocks = db_rows / poly_len;
    let mut rng_pub = ChaCha20Rng::from_seed(get_seed(seed));

    // Generate one random polynomial per block, then apply negperm.
    // This matches the ring path: generate_pseudorandom_query applies negperm(a)
    // before the NTT multiply, so the hint uses negperm(a). Building M_{negperm(a)}
    // means: hint_0 = negperm(a)·d (matches ring), query = M_{negperm(a)}^T·s = a·s (matches ring).
    let mut out = vec![0u64; poly_len * db_rows];
    for block in 0..num_blocks {
        // Generate random polynomial a(X)
        let mut poly = vec![0u64; poly_len];
        for idx in 0..poly_len {
            poly[idx] = rng_pub.sample::<u64, _>(rand::distributions::Standard);
        }

        // Apply negperm in Z_{2^64}: ā = [a_0, -a_{N-1}, -a_{N-2}, ..., -a_1]
        let mut nega_poly = vec![0u64; poly_len];
        nega_poly[0] = poly[0];
        for k in 1..poly_len {
            nega_poly[k] = (0u64).wrapping_sub(poly[poly_len - k]);
        }

        // Build negacyclic matrix M_{ā} where ā = negperm(a):
        //   M[i,j] = ā[i-j]       if i >= j
        //   M[i,j] = -ā[N+i-j]    if i < j
        for i in 0..poly_len {
            for j in 0..poly_len {
                let col = block * poly_len + j;
                let val = if i >= j {
                    nega_poly[i - j]
                } else {
                    (0u64).wrapping_sub(nega_poly[poly_len + i - j])
                };
                out[i * db_rows + col] = val;
            }
        }
    }
    out
}

/// W=2^32 variant: same RNG stream as generate_pseudorandom_matrix_word,
/// but values truncated to u32 (low 32 bits). Negacyclic arithmetic wraps at 2^32.
/// Output stored as Vec<u64> with upper 32 bits zero for compatibility.
pub fn generate_pseudorandom_matrix_w32(
    seed: u8,
    poly_len: usize,
    db_rows: usize,
) -> Vec<u64> {
    assert_eq!(db_rows % poly_len, 0);
    let num_blocks = db_rows / poly_len;
    let mut rng_pub = ChaCha20Rng::from_seed(get_seed(seed));

    let mut out = vec![0u64; poly_len * db_rows];
    for block in 0..num_blocks {
        let mut poly = vec![0u32; poly_len];
        for idx in 0..poly_len {
            // Consume u64 from RNG (same stream as W=2^64 path) but use low 32 bits
            let full: u64 = rng_pub.gen();
            poly[idx] = full as u32;
        }

        // negperm in Z_{2^32}
        let mut nega_poly = vec![0u32; poly_len];
        nega_poly[0] = poly[0];
        for k in 1..poly_len {
            nega_poly[k] = 0u32.wrapping_sub(poly[poly_len - k]);
        }

        // Build negacyclic matrix in Z_{2^32}
        for i in 0..poly_len {
            for j in 0..poly_len {
                let col = block * poly_len + j;
                let val: u32 = if i >= j {
                    nega_poly[i - j]
                } else {
                    0u32.wrapping_sub(nega_poly[poly_len + i - j])
                };
                out[i * db_rows + col] = val as u64;
            }
        }
    }
    out
}

impl<'p, 'c> YClient<'p, 'c> {
    pub fn new(inner: &'c mut Client<'p>, params: &'p Params) -> Self {
        Self {
            inner,
            params,
            lwe_client: LWEClient::new(LWEParams::default()),
        }
    }

    pub fn lwe_client(&self) -> &LWEClient {
        &self.lwe_client
    }

    /// Returns the last row of the LWE ciphertext (the 'b' scalar) for each RLWE ciphertext.
    fn rlwes_to_lwes(&self, ct: &[PolyMatrixRaw<'p>]) -> Vec<u64> {
        let v = ct
            .iter()
            .map(|ct| rlwe_to_lwe_last_row(self.params, ct))
            .collect::<Vec<_>>();
        concat_horizontal(&v, 1, self.params.poly_len)
    }

    pub fn generate_query_impl(
        &self,
        public_seed_idx: u8,
        dim_log2: usize,
        packing_type: PackingType,
        index: usize,
        embedding_width: Option<usize>,
        weights: Option<&[u64]>,
    ) -> Vec<PolyMatrixRaw<'p>> {
        // let db_cols = 1 << (self.params.db_dim_2 + self.params.poly_len_log2);
        // let idx_dim1 = index / db_cols;

        let multiply_ct = true;

        let mut rng_pub = ChaCha20Rng::from_seed(get_seed(public_seed_idx));

        // Generate dim1_bits LWE samples under public randomness
        let mut out = Vec::new();

        let scale_k = self.params.modulus / self.params.pt_modulus;

        for i in 0..(1 << dim_log2) {
            // create a single polynomial of all zeroes (num_coeffs = rows * cols * params.poly_len)
            let mut scalar = PolyMatrixRaw::zero(self.params, 1, 1);
            if let Some(width) = embedding_width {
                // Embed a width-sized block starting at (index * width)
                let ws = weights.unwrap();
                assert!(self.params.poly_len % width == 0, "poly_len must be divisible by width (for now)");

                let base = index * width;
                let poly_idx = base / self.params.poly_len;

                if i == poly_idx {
                    for j in 0..width {
                        let pos = (base + j) % self.params.poly_len;
                        // weight * (q/p) in modulus space
                        let weight  = ws[j];
                        assert!(weight < self.params.pt_modulus);
                        let mu = weight * scale_k;
                        assert!(mu < self.params.modulus);
                        scalar.data[pos] = mu;
                    }
                }
            } else {
                let is_nonzero = i == (index / self.params.poly_len);

                if is_nonzero {
                    scalar.data[index % self.params.poly_len] = scale_k;
                }
            }

            // say we have a PolyMatrix. this is a matrix of polynomials. 1x1 matrix means we have one polynomial
            // converting a PolyMatrixRaw to PolyMatrixNTT would be an isomorphic transformation (1x1 matrix -> 1x1 matrix), but each element is an NTT instead of a polynomial (should be same size, but we do CRT so num_crt times larger)

            // at this point we have a polynomial in NTT form where (if is_nonzero) the desired index encrypts the value 1 (times the modulus scale (q/p) and divided by the inverse of the polynomial length for the NTT)
              // if public_seed_idx == SEED_0 {
            //     out.push(scalar.pad_top(1));
            //     continue;
            // }

            let ct = if multiply_ct && PackingType::CDKS == packing_type {
                // recall a RLWE CT: (a, -s*a + e + µ*floor(q/p))
                // when we do negacyclic NTT/iNTT, we scale the result by the dimension of the polynomial (recall: q \equiv 1 mod 2N)
                // upon decryption, we cancel -s*a, so the scale doesn't matter for those term. to correctly get µ, we have to downscale
                // both the µ and noise e have to be pre-scaled, hence these scalar_mutliply_alloc and encrypt_matrix_scaled_reg calls
                // InspiRING normalizes via mod_inv_poly = 1/N on mask side during offline precomp,
                // so the query should NOT be pre-divided by N.
                let factor =
                     invert_uint_mod(self.params.poly_len as u64, self.params.modulus).unwrap();

                scalar = scalar_multiply_alloc(
                    &PolyMatrixRaw::single_value(self.params, factor).ntt(),
                    &scalar.ntt(),
                )
                .raw();
                self.inner.encrypt_matrix_scaled_reg(
                    &scalar.ntt(),
                    &mut ChaCha20Rng::from_entropy(),
                    &mut rng_pub,
                    factor,
                )
            } else {
                // this just encrypts the single polynomial using RLWE (output: ct in Rq^2)
                self.inner.encrypt_matrix_reg(
                    &scalar.ntt(),
                    &mut ChaCha20Rng::from_entropy(),
                    &mut rng_pub,
                )
            };

            let ct_raw = ct.raw();
            out.push(ct_raw);

            // let mut ct = self.inner.encrypt_matrix_reg(
            //     &scalar.ntt(),
            //     &mut ChaCha20Rng::from_entropy(),
            //     &mut rng_pub,
            // );

            // if multiply_ct && packing {
            //     let factor =
            //         invert_uint_mod(self.params.poly_len as u64, self.params.modulus).unwrap();
            //     ct = scalar_multiply_alloc(
            //         &PolyMatrixRaw::single_value(self.params, factor).ntt(),
            //         &ct,
            //     );
            // };

            // if multiply_error && is_nonzero && packing {
            //     let factor =
            //         invert_uint_mod(self.params.poly_len as u64, self.params.modulus).unwrap();
            //     ct = scalar_multiply_alloc(
            //         &PolyMatrixRaw::single_value(self.params, factor).ntt(),
            //         &ct,
            //     );
            // }

            // let ct_0_nega = negacyclic_perm(ct_raw.get_poly(0, 0), 0, self.params.modulus);
            // let ct_1_nega = negacyclic_perm(ct_raw.get_poly(1, 0), 0, self.params.modulus);
            // let mut ct_nega = PolyMatrixRaw::zero(self.params, 2, 1);
            // ct_nega.get_poly_mut(0, 0).copy_from_slice(&ct_0_nega);
            // ct_nega.get_poly_mut(1, 0).copy_from_slice(&ct_1_nega);

            // self-test
            // {
            //     let test_ct = self.inner.encrypt_matrix_reg(
            //         &PolyMatrixRaw::single_value(self.params, scale_k * 7).ntt(),
            //         &mut ChaCha20Rng::from_entropy(),
            //         &mut ChaCha20Rng::from_entropy(),
            //     );
            //     let lwe = rlwe_to_lwe(self.params, &test_ct.raw());
            //     let result = self.decode_response(&lwe);
            //     assert_eq!(result[0], 7);
            // }

        }

        out
    }

    pub fn generate_query(
        &self,
        public_seed_idx: u8,
        dim_log2: usize,
        packing_type: PackingType,
        index_row: usize,
        embedding_width: Option<usize>,
        weights: Option<&[u64]>,
    ) -> Vec<u64> {
        if public_seed_idx == SEED_0 && packing_type == PackingType::NoPacking {
            let lwe_params = LWEParams::default();
            let dim = 1 << (dim_log2 + self.params.poly_len_log2);

            // lwes must be (n + 1) x (dim) matrix
            let mut lwes = vec![0u64; (lwe_params.n + 1) * dim];

            let scale_k = lwe_params.scale_k() as u32;
            let mut vals_to_encrypt = vec![0u32; dim];
            vals_to_encrypt[index_row] = scale_k;

            let mut rng_pub = ChaCha20Rng::from_seed(get_seed(public_seed_idx));

            for i in (0..dim).step_by(lwe_params.n) {
                let out = self
                    .lwe_client
                    .encrypt_many(&mut rng_pub, &vals_to_encrypt[i..i + lwe_params.n])
                    .iter()
                    .map(|x| *x as u64)
                    .collect::<Vec<_>>();
                assert_eq!(out.len(), (lwe_params.n + 1) * lwe_params.n);
                for r in 0..lwe_params.n + 1 {
                    for c in 0..lwe_params.n {
                        lwes[r * dim + i + c] = out[r * lwe_params.n + c];
                    }
                }
            }

            lwes // (n+1) × dim, where each column is an LWE ciphertext
        } else {
            // only do this if we're packing. what is packing?
            // weirdly I think this conditional is just a way of having the same interface for generating the two queries, even though the operation is entirely separate (different moduli space, encryption method)
            let out = self.generate_query_impl(public_seed_idx, dim_log2, packing_type, index_row, embedding_width, weights);
            // at this point, we have fully encrypted the vector with all 0s and 1 at index using RLWE.
            // say the length of that vector is l_2, and the length of the RLWE poly is d_2. then we have m_2 = l_2/d_2 RLWE CTs
            assert_eq!(out.len(), (1<<dim_log2));
            assert_eq!(out[0].cols, 1);
            assert_eq!(out[0].rows, 2);
            // what do we do now?
            // we just pull out all the RLWE non-random components, and stack them as u64s
            // the exact same as if we had done LWE with the Toeplitz matrix of the polynomial
            let lwes = self.rlwes_to_lwes(&out);
            lwes
        }
    }

    pub fn decode_response(&self, response: &[u64]) -> Vec<u64> {
        debug!("Decoding response: {:?}", &response[..16]);
        let db_cols = 1 << (self.params.db_dim_2 + self.params.poly_len_log2);

        let sk = self.inner.get_sk_reg().as_slice().to_vec();

        let mut out = Vec::new();
        for col in 0..db_cols {
            let mut sum = 0u128;
            for i in 0..self.params.poly_len {
                let v1 = response[i * db_cols + col];
                let v2 = sk[i];
                sum += v1 as u128 * v2 as u128;
            }

            sum += response[self.params.poly_len * db_cols + col] as u128;

            let result = (sum % self.params.modulus as u128) as u64;
            let result_rescaled = rescale(result, self.params.modulus, self.params.pt_modulus);
            out.push(result_rescaled);
        }

        out
    }

    /// Generate a word-based (plain LWE) query.
    /// For crt_count=2 (WordPIR): W=2^64, wrapping u64.
    /// For crt_count=1 (SandwichPIR): W=2^32, wrapping u32 (stored as u64).
    pub fn generate_query_word(
        &self,
        seed: u8,
        dim_log2: usize,
        target_row: usize,
    ) -> Vec<u64> {
        let db_rows = 1 << (dim_log2 + self.params.poly_len_log2);
        let poly_len = self.params.poly_len;
        let q = self.params.modulus;

        // Recover s = sk_reg coefficients as signed
        let sk_raw = self.inner.get_sk_reg();
        let sk_data = sk_raw.as_slice();
        let mut s = vec![0i64; poly_len];
        for i in 0..poly_len {
            let x = sk_data[i];
            s[i] = if x > q / 2 { x as i64 - q as i64 } else { x as i64 };
        }

        let mut rng = ChaCha20Rng::from_entropy();

        if self.params.crt_count == 1 {
            // SandwichPIR: W=2^32, wrapping u32 arithmetic
            let a = generate_pseudorandom_matrix_w32(seed, poly_len, db_rows);
            let scale: u32 = ((1u64 << 32) / self.params.pt_modulus) as u32;
            // noise_width * W/Q ≈ noise_width (since W ≈ Q)
            let word_noise_width = self.params.noise_width * (1u64 << 32) as f64 / q as f64;
            let dg = DiscreteGaussian::init(word_noise_width);

            let mut query = vec![0u64; db_rows];
            for j in 0..db_rows {
                let mut val: u32 = 0;
                for i in 0..poly_len {
                    val = val.wrapping_sub((s[i] as u32).wrapping_mul(a[i * db_rows + j] as u32));
                }
                let e_val = dg.weighted_index.sample(&mut rng) as i64 - dg.max_val;
                val = val.wrapping_add(e_val as u32);
                if j == target_row {
                    val = val.wrapping_add(scale);
                }
                query[j] = val as u64; // upper 32 bits zero
            }
            query
        } else {
            // WordPIR: W=2^64, wrapping u64 arithmetic
            let a = generate_pseudorandom_matrix_word(seed, poly_len, db_rows);
            let scale: u64 = (1u128 << 64).wrapping_div(self.params.pt_modulus as u128) as u64;
            let word_noise_width = self.params.noise_width * (u64::MAX as f64 + 1.0) / q as f64;
            let dg = DiscreteGaussian::init(word_noise_width);

            let mut query = vec![0u64; db_rows];
            for j in 0..db_rows {
                let mut val: u64 = 0;
                for i in 0..poly_len {
                    val = val.wrapping_sub((s[i] as u64).wrapping_mul(a[i * db_rows + j]));
                }
                let e_val = dg.weighted_index.sample(&mut rng) as i64 - dg.max_val;
                val = val.wrapping_add(e_val as u64);
                if j == target_row {
                    val = val.wrapping_add(scale);
                }
                query[j] = val;
            }
            query
        }
    }

    pub fn client(&self) -> &Client<'p> {
        self.inner
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_lwe() {
        let lwe_params = LWEParams::default();
        let client = LWEClient::new(lwe_params.clone());
        let pt = fastrand::u32(0..lwe_params.pt_modulus as u32);
        let scaled_pt = pt.wrapping_mul(lwe_params.scale_k() as u32);
        let ct = client.encrypt(&mut ChaCha20Rng::from_entropy(), scaled_pt);
        let pt_dec = client.decrypt(&ct);
        let result = rescale(pt_dec as u64, lwe_params.modulus, lwe_params.pt_modulus) as u32;
        assert_eq!(result, pt);
    }
}
