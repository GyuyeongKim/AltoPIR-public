#[cfg(target_feature = "avx512f")]
use std::arch::x86_64::*;
use std::time::Instant;

use log::debug;

use spiral_rs::{arith::*, discrete_gaussian::*, gadget::*, ntt::*, number_theory::*, params::*, poly::*};

use crate::server::Precomp;
use crate::measurement::get_vec_pm_size_bytes;
use crate::client::raw_generate_expansion_params;

use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

use crate::util::*;

fn homomorphic_automorph<'a>(
    params: &'a Params,
    t: usize,
    t_exp: usize,
    ct: &PolyMatrixNTT<'a>,
    pub_param: &PolyMatrixNTT<'a>,
) -> PolyMatrixNTT<'a> {
    assert_eq!(ct.rows, 2);
    assert_eq!(ct.cols, 1);

    let ct_raw = ct.raw();
    let ct_auto = automorph_alloc(&ct_raw, t);

    let mut ginv_ct = PolyMatrixRaw::zero(params, t_exp, 1);
    gadget_invert_rdim(&mut ginv_ct, &ct_auto, 1);
    let mut ginv_ct_ntt = PolyMatrixNTT::zero(params, t_exp, 1);
    for i in 1..t_exp {
        let pol_src = ginv_ct.get_poly(i, 0);
        let pol_dst = ginv_ct_ntt.get_poly_mut(i, 0);
        reduce_copy(params, pol_dst, pol_src);
        ntt_forward(params, pol_dst);
    }
    // let ginv_ct_ntt = ginv_ct.ntt();
    let w_times_ginv_ct = pub_param * &ginv_ct_ntt;

    let mut ct_auto_1 = PolyMatrixRaw::zero(params, 1, 1);
    ct_auto_1
        .data
        .as_mut_slice()
        .copy_from_slice(ct_auto.get_poly(1, 0));
    let ct_auto_1_ntt = ct_auto_1.ntt();

    &ct_auto_1_ntt.pad_top(1) + &w_times_ginv_ct
}

fn pack_lwes_inner_non_recursive<'a>(
    params: &'a Params,
    ell: usize,
    _start_idx: usize,
    rlwe_cts: &[PolyMatrixNTT<'a>],
    pub_params: &[PolyMatrixNTT<'a>],
    y_constants: &(Vec<PolyMatrixNTT<'a>>, Vec<PolyMatrixNTT<'a>>),
    prepared_vals: Option<&[PolyMatrixNTT<'a>]>,
    mut output_prepared_vals: Option<&mut Vec<PolyMatrixNTT<'a>>>,
) -> PolyMatrixNTT<'a> {
    assert!(pub_params.len() == params.poly_len_log2 || pub_params.len() == 0);
    assert_eq!(params.crt_count, 2);

    let mut working_set = rlwe_cts.to_vec();

    let mut y_times_ct_odd = PolyMatrixNTT::zero(params, 2, 1);
    let mut neg_y_times_ct_odd = PolyMatrixNTT::zero(params, 2, 1);
    let mut ct_sum_1 = PolyMatrixNTT::zero(params, 2, 1);

    let mut ct_raw = PolyMatrixRaw::zero(params, 1, 1);
    let mut ct_auto = PolyMatrixRaw::zero(params, 1, 1);
    let mut ginv_ct = PolyMatrixRaw::zero(params, params.t_exp_left, 1);
    let mut ginv_ct_ntt = PolyMatrixNTT::zero(params, params.t_exp_left, 1);
    let mut ct_auto_1_ntt = PolyMatrixNTT::zero(params, 1, 1);
    let mut w_times_ginv_ct = PolyMatrixNTT::zero(params, 2, 1);
    let mut scratch = PolyMatrixNTT::zero(params, 2, 1);
    let scratch_mut_slc = scratch.as_mut_slice();

    let mut total_0 = 0;
    let mut total_1 = 0;
    let mut total_2 = 0;
    let mut total_3 = 0;
    let mut total_4 = 0;

    let mut num_ntts = 0;

    let mut ct_raw_1_auto_ntt;

    for cur_ell in 1..=ell {
        let num_in = 1 << (ell - cur_ell + 1);
        let num_out = num_in >> 1;

        let (first_half, second_half) = (&mut working_set[..num_in]).split_at_mut(num_out);

        for i in 0..num_out {
            let now = Instant::now();
            let ct_even = &mut first_half[i];
            let ct_odd = &second_half[i];

            let (y, neg_y) = (&y_constants.0[cur_ell - 1], &y_constants.1[cur_ell - 1]);

            // if i == 5 {
            //     debug!("neg_y: {:?}", neg_y.as_slice());
            // }

            scalar_multiply_avx(&mut y_times_ct_odd, &y, &ct_odd);
            scalar_multiply_avx(&mut neg_y_times_ct_odd, &neg_y, &ct_odd);

            // if i == 5 && cur_ell == 1 && output_prepared_vals.is_none() {
            //     debug!(
            //         "ct_even[0]: {:?}",
            //         params.crt_compose(ct_even.get_poly(1, 0), 0)
            //     );
            //     debug!(
            //         "ct_odd[0]: {:?}",
            //         params.crt_compose(ct_odd.get_poly(1, 0), 0)
            //     );
            // }

            ct_sum_1.as_mut_slice().copy_from_slice(ct_even.as_slice());
            add_into(&mut ct_sum_1, &neg_y_times_ct_odd);
            fast_add_into_no_reduce(ct_even, &y_times_ct_odd);
            total_3 += now.elapsed().as_micros();

            {
                let ct: &PolyMatrixNTT<'_> = &ct_sum_1;
                let t = (1 << cur_ell) + 1;
                let t_exp = params.t_exp_left;
                let (cur_ginv_ct_ntt, cur_ct_auto_1_ntt) = if cur_ell == 1
                    && prepared_vals.is_some()
                {
                    let ginv_ct_ntt = &prepared_vals.unwrap()[i];

                    // In this first round, this value is always zero
                    ct_raw_1_auto_ntt = PolyMatrixNTT::zero(params, 1, 1);
                    (ginv_ct_ntt, &ct_raw_1_auto_ntt)
                } else {
                    let now = Instant::now();
                    // let ct_raw = ct.raw();
                    // nb: scratch has 2nd row of ct in uncrtd form,
                    //     ct_raw has only first row
                    from_ntt_scratch(&mut ct_raw, scratch_mut_slc, ct);
                    if cur_ell == 1 {
                        num_ntts += 2;
                    }
                    total_0 += now.elapsed().as_micros();
                    let now = Instant::now();
                    automorph(&mut ct_auto, &ct_raw, t);
                    total_1 += now.elapsed().as_micros();

                    gadget_invert_rdim(&mut ginv_ct, &ct_auto, 1);

                    let skip_first_gadget_dim = true;
                    if skip_first_gadget_dim {
                        for i in 1..t_exp {
                            let pol_src = ginv_ct.get_poly(i, 0);
                            let pol_dst = ginv_ct_ntt.get_poly_mut(i, 0);
                            pol_dst[..params.poly_len].copy_from_slice(pol_src);
                            pol_dst[params.poly_len..].copy_from_slice(pol_src);

                            ntt_forward(params, pol_dst);
                            if cur_ell == 1 {
                                num_ntts += 1;
                            }
                        }
                    } else {
                        to_ntt(&mut ginv_ct_ntt, &ginv_ct);
                        // num_ntts += ginv_ct_ntt.rows * ginv_ct_ntt.cols;
                    }

                    let now = Instant::now();
                    automorph_poly_uncrtd(params, ct_auto_1_ntt.as_mut_slice(), scratch_mut_slc, t);
                    ntt_forward(params, ct_auto_1_ntt.as_mut_slice());
                    // num_ntts += 1;

                    total_4 += now.elapsed().as_micros();

                    (&ginv_ct_ntt, &ct_auto_1_ntt)
                };

                if output_prepared_vals.is_some() {
                    let opv_mut = output_prepared_vals.as_deref_mut();
                    opv_mut.unwrap().push(cur_ginv_ct_ntt.clone());
                    continue;
                }

                let pub_param = &pub_params[params.poly_len_log2 - 1 - (cur_ell - 1)];
                // let ginv_ct_ntt = ginv_ct.ntt();
                // let w_times_ginv_ct = pub_param * &ginv_ct_ntt;
                w_times_ginv_ct.as_mut_slice().fill(0);
                multiply_no_reduce(&mut w_times_ginv_ct, &pub_param, &cur_ginv_ct_ntt, 1);

                // &ct_auto_1_ntt.pad_top(1) + &w_times_ginv_ct
                let now = Instant::now();
                add_into_at_no_reduce(ct_even, &cur_ct_auto_1_ntt, 1, 0);
                add_into(ct_even, &w_times_ginv_ct);
                total_2 += now.elapsed().as_micros();
            };
        }

        if output_prepared_vals.is_some() {
            return PolyMatrixNTT::zero(params, 2, 1);
        }
    }

    if false {
        debug!("num_ntts: {}", num_ntts);
        debug!("total_0: {} us", total_0);
        debug!("total_1: {} us", total_1);
        debug!("total_2: {} us", total_2);
        debug!("total_3: {} us", total_3);
        debug!("total_4: {} us", total_4);
    }

    // let mut res = PolyMatrixNTT::zero(params, 2, 1);
    // // let mut res = working_set[0].clone();
    // add_into(&mut res, &working_set[0]);
    // res

    working_set[0].clone()
}

pub fn precompute_pack<'a>(
    params: &'a Params,
    ell: usize,
    rlwe_cts: &[PolyMatrixNTT<'a>],
    fake_pub_params: &[PolyMatrixNTT<'a>],
    y_constants: &(Vec<PolyMatrixNTT<'a>>, Vec<PolyMatrixNTT<'a>>),
) -> (PolyMatrixNTT<'a>, Vec<PolyMatrixNTT<'a>>, Vec<Vec<usize>>) {
    assert!(fake_pub_params.len() == params.poly_len_log2);
    assert_eq!(params.crt_count, 2);

    let mut working_set = rlwe_cts.to_vec();
    assert_eq!(working_set.len(), 1 << ell);

    let mut y_times_ct_odd = PolyMatrixNTT::zero(params, 2, 1);
    let mut neg_y_times_ct_odd = PolyMatrixNTT::zero(params, 2, 1);
    let mut ct_sum_1 = PolyMatrixNTT::zero(params, 2, 1);

    let mut ct_raw = PolyMatrixRaw::zero(params, 1, 1);
    let mut ct_auto = PolyMatrixRaw::zero(params, 1, 1);
    let mut ginv_ct = PolyMatrixRaw::zero(params, params.t_exp_left, 1);
    let mut ginv_ct_ntt = PolyMatrixNTT::zero(params, params.t_exp_left, 1);
    let mut ct_auto_1_ntt = PolyMatrixNTT::zero(params, 1, 1);
    let mut w_times_ginv_ct = PolyMatrixNTT::zero(params, 2, 1);
    let mut scratch = PolyMatrixNTT::zero(params, 2, 1);
    let scratch_mut_slc = scratch.as_mut_slice();

    let mut total_0 = 0;
    let mut total_1 = 0;
    let mut total_2 = 0;
    let mut total_3 = 0;
    let mut total_4 = 0;

    let mut num_ntts = 0;

    let mut res = Vec::new();


    // Algo2, but we skip the base case
    for cur_ell in 1..=ell {
        // last step: two RLWEs in, one RLWE out
        let num_in = 1 << (ell - cur_ell + 1);
        let num_out = num_in >> 1;

        let (first_half, second_half) = (&mut working_set[..num_in]).split_at_mut(num_out);

        // for each call to Algo2 at this level of the tree
        for i in 0..num_out {
            let now = Instant::now();
            let ct_even = &mut first_half[i];
            let ct_odd = &second_half[i];

            let (y, neg_y) = (&y_constants.0[cur_ell - 1], &y_constants.1[cur_ell - 1]);

            // ct <- LHS + automorph(RHS, 2^curr_ell+1)
            // LHS: ct_even + y_times_ct_odd
            // RHS: ct_even + neg_y_times_ct_odd
            scalar_multiply_avx(&mut y_times_ct_odd, &y, &ct_odd);
            scalar_multiply_avx(&mut neg_y_times_ct_odd, &neg_y, &ct_odd);

            ct_sum_1.as_mut_slice().copy_from_slice(ct_even.as_slice());
            add_into(&mut ct_sum_1, &neg_y_times_ct_odd);
            fast_add_into_no_reduce(ct_even, &y_times_ct_odd);
            total_3 += now.elapsed().as_micros();

            // all this is to calculate the automorphism, so we can add it into ct_even
            {
                let ct: &PolyMatrixNTT<'_> = &ct_sum_1; // we want to automorph him
                let t = (1 << cur_ell) + 1; // tau
                let t_exp = params.t_exp_left;
                let (cur_ginv_ct_ntt, _cur_ct_auto_1_ntt) = {
                    let now = Instant::now();
                    // let ct_raw = ct.raw();

                    // nb: scratch has 2nd row of ct in uncrtd form,
                    //     ct_raw has only first row
                    from_ntt_scratch(&mut ct_raw, scratch_mut_slc, ct);
                    // ct_raw now stores the random portion in raw form
                    // 2 NTTs because 2 CRT moduli
                    // only counting for the base level, just for debug purposes
                    if cur_ell == 1 {
                        num_ntts += 2;
                    }
                    total_0 += now.elapsed().as_micros();
                    let now = Instant::now();
                    // we had to get in raw because the first part of the automorphism before calling KeySwitch applies the automorphism in the raw
                    // ct_auto stores automorph(ct_raw, t)
                    automorph(&mut ct_auto, &ct_raw, t);
                    total_1 += now.elapsed().as_micros();

                    // we'll now copy the automorphed ct into t_exp_left (log_z(q2)) many rows using base_z decomposition
                    gadget_invert_rdim(&mut ginv_ct, &ct_auto, 1);

                    let skip_first_gadget_dim = false;
                    if skip_first_gadget_dim {
                        for i in 1..t_exp {
                            let pol_src = ginv_ct.get_poly(i, 0);
                            let pol_dst = ginv_ct_ntt.get_poly_mut(i, 0);
                            pol_dst[..params.poly_len].copy_from_slice(pol_src);
                            pol_dst[params.poly_len..].copy_from_slice(pol_src);

                            ntt_forward(params, pol_dst);
                            if cur_ell == 1 {
                                num_ntts += 1;
                            }
                        }
                    } else {
                        to_ntt(&mut ginv_ct_ntt, &ginv_ct);
                        // num_ntts += ginv_ct_ntt.rows * ginv_ct_ntt.cols;
                    }

                    // ginv_ct_ntt stores the g^-1(ct_auto) in NTT form

                    let now = Instant::now();
                    // scratch_mut_slc stored the CRT-level raw form of the polynomial
                    // we then automorph this CRT-raw form, then we put it back in NTT form
                    automorph_poly_uncrtd(params, ct_auto_1_ntt.as_mut_slice(), scratch_mut_slc, t);
                    ntt_forward(params, ct_auto_1_ntt.as_mut_slice());
                    // num_ntts += 1;

                    total_4 += now.elapsed().as_micros();

                    (&ginv_ct_ntt, &ct_auto_1_ntt)
                };

                // println!(
                //     "ct_auto_1_ntt.raw(): {:?}",
                //     &ct_auto_1_ntt.raw().as_slice()[..30]
                // );


                // we are caching g^−1𝑧(𝜏 (𝑐0)), so once we get the real KS matrix we can compute the final
                res.push(condense_matrix(params, cur_ginv_ct_ntt));

                // the correct KS key? we have log(d2) many keys
                // at the current level, we multiplied with y_constant[curr_ell - 1]. y_constants are stored as X^(d2/2^L) from L = [1..log2(d2)]
                // we have, as input, 2^(ell-cur_ell+1) many CTs, so t =ell-cur_ell+1 which coresponds to key t and -1 for 1-indexing
                let pub_param = &fake_pub_params[params.poly_len_log2 - 1 - (cur_ell - 1)];
                // let ginv_ct_ntt = ginv_ct.ntt();
                // let w_times_ginv_ct = pub_param * &ginv_ct_ntt;
                w_times_ginv_ct.as_mut_slice().fill(0);
                // we multiply to get the full W_t*g^-1(auto(ct_sum_1))
                // we only need the random portion, but the pub_param is an KS key on a key of zeroes? so it at least has noise? is this ignored?
                multiply_no_reduce(&mut w_times_ginv_ct, &pub_param, &cur_ginv_ct_ntt, 0);

                // &ct_auto_1_ntt.pad_top(1) + &w_times_ginv_ct
                let now = Instant::now();
                // i have no clue why we are adding cur_ct_auto_1_ntt into the non-random portion?
                // add_into_at_no_reduce(ct_even, &cur_ct_auto_1_ntt, 1, 0);
                // yep! this line is a bug..., but ignored because the online portion just overwrites it.
                add_into(ct_even, &w_times_ginv_ct);
                total_2 += now.elapsed().as_micros();
            };
        }
    }

    if false {
        debug!("num_ntts: {}", num_ntts);
        debug!("total_0: {} us", total_0);
        debug!("total_1: {} us", total_1);
        debug!("total_2: {} us", total_2);
        debug!("total_3: {} us", total_3);
        debug!("total_4: {} us", total_4);
    }

    // After computing W · g −1 𝑧 (𝜏 (𝑐0)), the server needs to apply the automorphism 𝜏 to the message-embedding component of the ciphertext (i.e., compute 𝜏 (𝑐1)). If 𝑐1 is in NTT representation, then the NTT representation of 𝜏 (𝑐1) is simply a permutation on the NTT representation of 𝑐1. In our implementation, the server simply pre-computes and caches the log𝑑2 permutations used by CDKS.Pack. This way, the server does not need to perform additional NTTs when computing CDKS.Pack.
    let tables = generate_automorph_tables_brute_force(&params);

    (working_set[0].clone(), res, tables)
}

// online packing, given the tuple
// res: random portion
// vals: NTT of the ginv(a) at each non-leaf in FFT tree
// tables: table[tau][idx] -> new_idx
pub fn pack_using_precomp_vals<'a>(
    params: &'a Params,
    ell: usize,
    pub_params: &[PolyMatrixNTT<'a>],
    b_values: &[u64],
    precomp_res: &PolyMatrixNTT<'a>,
    precomp_vals: &[PolyMatrixNTT<'a>],
    precomp_tables: &[Vec<usize>],
    y_constants: &(Vec<PolyMatrixNTT<'a>>, Vec<PolyMatrixNTT<'a>>),
) -> PolyMatrixNTT<'a> {
    // packing algorithm  for n = 2^ell ciphertexts require (n − 1) automorphisms
    // FFT tree's height is log2(2048)+1 = 12, 2^12-1 nodes, each non-leaf node does one automorphism -> 4095-2048=2047
    assert_eq!(precomp_vals.len(), (1 << ell)-1);

    // Q: working set initialized to half of what it was in precompute_pack??
    // A: the reason is because we are working only with the non-random portion of the RLWE ct
    // we initialize the entire working_set to the 0 polynomial (NTT form), then we do the FFT to calculate the 
    let mut working_set = Vec::with_capacity(1 << (ell - 1));
    for _ in 0..(1 << (ell - 1)) {
        working_set.push(PolyMatrixNTT::zero(params, 1, 1));
    }

    let mut y_times_ct_odd = PolyMatrixNTT::zero(params, 1, 1);
    let mut neg_y_times_ct_odd = PolyMatrixNTT::zero(params, 1, 1);
    let mut ct_sum_1 = PolyMatrixNTT::zero(params, 1, 1);
    let mut w_times_ginv_ct = PolyMatrixNTT::zero(params, 1, 1);

    let mut time_0 = 0;
    let mut time_1 = 0;
    let mut time_2 = 0;
    let mut time_3 = 0;
    let mut time_4 = 0;

    let mut idx_precomp = 0;
    let mut num_muls = 0;
    for cur_ell in 1..=ell {
        let mut num_in = 1 << (ell - cur_ell + 1);
        let num_out = num_in >> 1;

        if num_in == params.poly_len {
            num_in = num_out;
        }

        let (first_half, second_half) = (&mut working_set[..num_in]).split_at_mut(num_out);

        for i in 0..num_out {
            let now = Instant::now();
            let ct_even = &mut first_half[i];

            let (y, neg_y) = (&y_constants.0[cur_ell - 1], &y_constants.1[cur_ell - 1]);

            if cur_ell > 1 {
                let ct_odd = &mut second_half[i];
                scalar_multiply_avx(&mut y_times_ct_odd, &y, &ct_odd);
                scalar_multiply_avx(&mut neg_y_times_ct_odd, &neg_y, &ct_odd);
            }

            time_0 += now.elapsed().as_micros();

            let now = Instant::now();
            if cur_ell > 1 {
                ct_sum_1.as_mut_slice().copy_from_slice(ct_even.as_slice());
                fast_add_into_no_reduce(&mut ct_sum_1, &neg_y_times_ct_odd);
                fast_add_into_no_reduce(ct_even, &y_times_ct_odd);
            }
            time_1 += now.elapsed().as_micros();

            let now = Instant::now();
            let ct: &PolyMatrixNTT<'_> = &ct_sum_1;
            let t = (1 << cur_ell) + 1;

            let cur_ginv_ct_ntt = &precomp_vals[idx_precomp];
            idx_precomp += 1;

            let w = &pub_params[params.poly_len_log2 - 1 - (cur_ell - 1)];
            // both w and g^-1(ct) are in condensed form
            fast_multiply_no_reduce(params, &mut w_times_ginv_ct, &w, &cur_ginv_ct_ntt, 0);
            num_muls += 1;
            time_2 += now.elapsed().as_micros();

            if cur_ell > 1 {
                let now = Instant::now();
                apply_automorph_ntt(params, &precomp_tables, &ct, ct_even, t);

                // fast_add_into_no_reduce(ct_even, &ct_auto_1_ntt);
                time_3 += now.elapsed().as_micros();
                let now = Instant::now();

                // second condition prevents overflow
                // since we store things in u64, we don't have to worry about reducing everytime
                // reducing is some fn of the CRT moduli, u64, the multiplication dims (t==3), and the addition
                if i < num_out / 2 && ((cur_ell - 1) % 5 != 0) {
                    fast_add_into_no_reduce(ct_even, &w_times_ginv_ct);
                } else {
                    // reduction right before or after addition is much faster than at multiplication time
                    fast_add_into(ct_even, &w_times_ginv_ct);
                }
                time_4 += now.elapsed().as_micros();
            } else {
                let now = Instant::now();
                if i < num_out / 2 {
                    fast_add_into_no_reduce(ct_even, &w_times_ginv_ct);
                } else {
                    fast_add_into(ct_even, &w_times_ginv_ct);
                }
                time_4 += now.elapsed().as_micros();
            }
        }
    }

    if false {
        println!("time_0: {} us", time_0);
        println!("time_1: {} us", time_1);
        println!("time_2: {} us", time_2);
        println!("time_3: {} us", time_3);
        println!("time_4: {} us", time_4);
        println!("idx_precomp: {}", idx_precomp);
        println!("num_muls: {}", num_muls);
    }

    assert_eq!(idx_precomp, precomp_vals.len());

    let mut resulting_row_1 = working_set[0].clone();
    fast_reduce(&mut resulting_row_1);

    let resulting_row_1 = resulting_row_1.as_slice();

    let mut res = precomp_res.clone();
    res.get_poly_mut(1, 0).copy_from_slice(resulting_row_1);

    // recall: PolyMatrixRaw is never CRT, always composed
    let mut out_raw = res.raw();


    // all the packing magic is just about the random portion and how it re-encodes with the automorphism keys (affects both a and b)
    // if we have a (R)LWE ct encoded under the secret key, we can always just add it into the phase without worrying!
    // recall the LWEs that are packed are scaled by inv(d2) by generate_query_impl
    // because the b's didn't go through that automorphism, we reverse that scaling!
    for z in 0..params.poly_len {
        let val = barrett_reduction_u128(params, b_values[z] as u128 * params.poly_len as u128);
        let idx = params.poly_len + z; // second row
        out_raw.data[idx] += val;
        // works only if you can guarantee out_raw.data[idx] + val < 2*modulus (true because each already % modulus)
        if out_raw.data[idx] >= params.modulus {
            out_raw.data[idx] -= params.modulus;
        }
    }
    let out = out_raw.ntt();

    out
}

// Algo1 CDKS, n = 1, N=poly_len
pub fn pack_single_lwe<'a>(
    params: &'a Params,
    pub_params: &[PolyMatrixNTT<'a>],
    lwe_ct: &PolyMatrixNTT<'a>,
) -> PolyMatrixNTT<'a> {
    // computing:
    // r0 = f
    // r1 = r0 + automorph(r0, ts[0])
    // r2 = r1 + automorph(r1, ts[1])
    // ...
    // r_\log d = ...

    let mut cur_r = lwe_ct.clone();
    for i in 0..params.poly_len_log2 {
        let t = (params.poly_len / (1 << i)) + 1;
        let pub_param = &pub_params[i];
        let tau_of_r = homomorphic_automorph(params, t, params.t_exp_left, &cur_r, pub_param);
        add_into(&mut cur_r, &tau_of_r);
    }
    cur_r
}

// pub fn fast_scalar_multiply_avx(res: &mut PolyMatrixNTT, a: &PolyMatrixNTT, b: &PolyMatrixNTT) {
//     assert_eq!(a.rows, 1);
//     assert_eq!(a.cols, 1);
//     assert_eq!(b.rows, 1);
//     assert_eq!(b.cols, 1);
//     assert_eq!(res.rows, 1);
//     assert_eq!(res.cols, 1);

//     // this is totally custom for cur_ell == 2

//     let params = res.params;
//     // let pol2 = a.get_poly(0, 0);
//     for i in 0..b.rows {
//         for j in 0..b.cols {
//             let res_slc = res.get_poly_mut(i, j);
//             let a_slc = a.get_poly(0, 0);
//             let b_slc = b.get_poly(i, j);
//             unsafe {
//                 let a_ptr = a_slc.as_ptr();
//                 let b_ptr = b_slc.as_ptr();
//                 let res_ptr = res_slc.as_mut_ptr();

//                 for m in 0..8 {
//                     // all the values in the NTT form of the scalar polynomial are the same
//                     let a_val = *a_ptr.add(m * 512);
//                     let x = _mm256_set1_epi64x(a_val as i64);
//                     for z in (0..512).step_by(4) {
//                         let p_y = b_ptr.add(m * 512 + z);
//                         let y = _mm256_load_si256(p_y as *const _);
//                         let product = _mm256_mul_epu32(x, y);

//                         let p_z = res_ptr.add(m * 512 + z);
//                         _mm256_store_si256(p_z as *mut _, product);
//                     }
//                 }
//             }
//         }
//     }
// }

pub fn fast_barrett_raw_u64(input: u64, const_ratio_1: u64, modulus: u64) -> u64 {
    let tmp = (((input as u128) * (const_ratio_1 as u128)) >> 64) as u64;

    // Barrett subtraction
    let res = input - tmp * modulus;

    res
}

pub fn fast_add_into(res: &mut PolyMatrixNTT, a: &PolyMatrixNTT) {
    assert!(res.rows == a.rows);
    assert!(res.cols == a.cols);

    let params = res.params;
    for i in 0..res.rows {
        for j in 0..res.cols {
            let res_poly = res.get_poly_mut(i, j);
            let a_poly = a.get_poly(i, j);
            for c in 0..params.crt_count {
                for i in 0..params.poly_len {
                    let idx = c * params.poly_len + i;
                    unsafe {
                        let p_res = res_poly.as_mut_ptr().add(idx);
                        let p_a = a_poly.as_ptr().add(idx);
                        let val = *p_res + *p_a;
                        let reduced =
                            fast_barrett_raw_u64(val, params.barrett_cr_1[c], params.moduli[c]);
                        *p_res = reduced;
                    }
                }
            }
        }
    }
}

#[cfg(target_feature = "avx512f")]
pub fn fast_multiply_no_reduce(
    params: &Params,
    res: &mut PolyMatrixNTT,
    a: &PolyMatrixNTT,
    b: &PolyMatrixNTT,
    _start_inner_dim: usize,
) {
    assert_eq!(res.rows, a.rows);
    assert_eq!(res.cols, b.cols);
    assert_eq!(res.rows, 1);
    assert_eq!(res.cols, 1);

    assert_eq!(a.cols, b.rows);
    assert_eq!(params.crt_count * params.poly_len, 2 * 2048);

    unsafe {
        let a_ptr = a.as_slice().as_ptr();
        let b_ptr = b.as_slice().as_ptr();
        let res_ptr = res.as_mut_slice().as_mut_ptr();
        let pol_sz = params.poly_len;

        for idx in (0..pol_sz).step_by(8) {
            let mut sum_lo = _mm512_setzero_si512();
            let mut sum_hi = _mm512_setzero_si512();
            for k in 0..a.cols {
                let p_x = a_ptr.add(k * 2 * pol_sz + idx);
                let p_y = b_ptr.add(k * 2 * pol_sz + idx);

                let x = _mm512_load_si512(p_x as *const _);
                let x_lo = x;
                let x_hi = _mm512_srli_epi64(x, 32);
                let y = _mm512_load_si512(p_y as *const _);
                let y_lo = y;
                let y_hi = _mm512_srli_epi64(y, 32);

                let product_lo = _mm512_mul_epu32(x_lo, y_lo);
                let product_hi = _mm512_mul_epu32(x_hi, y_hi);

                sum_lo = _mm512_add_epi64(sum_lo, product_lo);
                sum_hi = _mm512_add_epi64(sum_hi, product_hi);
            }

            let p_z = res_ptr.add(idx);
            _mm512_store_si512(p_z as *mut _, sum_lo);
            let p_z = res_ptr.add(pol_sz + idx);
            _mm512_store_si512(p_z as *mut _, sum_hi);
        }
    }
}

#[cfg(not(target_feature = "avx512f"))]
pub fn fast_multiply_no_reduce(
    params: &Params,
    res: &mut PolyMatrixNTT,
    a: &PolyMatrixNTT,
    b: &PolyMatrixNTT,
    _start_inner_dim: usize,
) {
    assert_eq!(res.rows, a.rows);
    assert_eq!(res.cols, b.cols);
    assert_eq!(res.rows, 1);
    assert_eq!(res.cols, 1);

    assert_eq!(a.cols, b.rows);
    assert_eq!(params.crt_count * params.poly_len, 2 * 2048);

    // Fallback scalar implementation
    let a_slc = a.as_slice();
    let b_slc = b.as_slice();
    let res_slc = res.as_mut_slice();
    let pol_sz = params.poly_len;

    // Zero out result
    for i in 0..res_slc.len() {
        res_slc[i] = 0;
    }

    for k in 0..a.cols {
        for idx in 0..pol_sz {
            let a_val = a_slc[k * 2 * pol_sz + idx];
            let b_val = b_slc[k * 2 * pol_sz + idx];

            // Split into low and high 32 bits
            let a_lo = a_val & 0xFFFFFFFF;
            let a_hi = a_val >> 32;
            let b_lo = b_val & 0xFFFFFFFF;
            let b_hi = b_val >> 32;

            // Multiply and accumulate
            res_slc[idx] = res_slc[idx].wrapping_add(a_lo.wrapping_mul(b_lo));
            res_slc[pol_sz + idx] = res_slc[pol_sz + idx].wrapping_add(a_hi.wrapping_mul(b_hi));
        }
    }
}

// combines two CRT moduli < 2^32 into a 64 bit word, element wise
pub fn condense_matrix<'a>(params: &'a Params, a: &PolyMatrixNTT<'a>) -> PolyMatrixNTT<'a> {
    // SandwichPIR (crt_count=1): no CRT to condense, identity
    if params.crt_count == 1 { return a.clone(); }
    let mut res = PolyMatrixNTT::zero(params, a.rows, a.cols);
    for i in 0..a.rows {
        for j in 0..a.cols {
            let res_poly = &mut res.get_poly_mut(i, j);
            let a_poly = a.get_poly(i, j);
            for z in 0..params.poly_len {
                res_poly[z] = a_poly[z] | (a_poly[z + params.poly_len] << 32);
            }
        }
    }
    res
}

pub fn uncondense_matrix<'a>(params: &'a Params, a: &PolyMatrixNTT<'a>) -> PolyMatrixNTT<'a> {
    // SandwichPIR (crt_count=1): no CRT to uncondense, identity
    if params.crt_count == 1 { return a.clone(); }
    let mut res = PolyMatrixNTT::zero(params, a.rows, a.cols);
    for i in 0..a.rows {
        for j in 0..a.cols {
            let res_poly = &mut res.get_poly_mut(i, j);
            let a_poly = a.get_poly(i, j);
            for z in 0..params.poly_len {
                res_poly[z] = a_poly[z] & ((1u64 << 32) - 1);
                res_poly[z + params.poly_len] = a_poly[z] >> 32;
            }
        }
    }
    res
}

#[cfg(target_feature = "avx512f")]
pub fn multiply_add_poly_avx(_params: &Params, res: &mut [u64], a: &[u64], b: &[u64]) {
    unsafe {
        let a_ptr = a.as_ptr();
        let b_ptr = b.as_ptr();
        let res_ptr = res.as_mut_ptr();
        for i in (0..res.len()).step_by(8) {
            let p_x = a_ptr.add(i);
            let p_y = b_ptr.add(i);
            let p_z = res_ptr.add(i);

            let x = _mm512_load_si512(p_x as *const _);
            let y = _mm512_load_si512(p_y as *const _);
            let z = _mm512_load_si512(p_z as *const _);

            let product = _mm512_mul_epu32(x, y);
            let out = _mm512_add_epi64(z, product);

            _mm512_store_si512(p_z as *mut _, out);
        }
    }
}

#[cfg(not(target_feature = "avx512f"))]
pub fn multiply_add_poly_avx(_params: &Params, res: &mut [u64], a: &[u64], b: &[u64]) {
    for i in 0..res.len() {
        let product = (a[i] & 0xFFFFFFFF).wrapping_mul(b[i] & 0xFFFFFFFF);
        res[i] = res[i].wrapping_add(product);
    }
}

/// SandwichPIR: multiply-accumulate for a single NTT modulus (crt_count=1).
/// Products of two values < Q ≈ 2^32 fit in u64 (Q² < 2^64). Barrett-reduce
/// the product, then add mod Q. No u128 division in the hot loop.
pub fn multiply_add_poly_single_mod(params: &Params, res: &mut [u64], a: &[u64], b: &[u64]) {
    let q = params.moduli[0];
    let cr = params.barrett_cr_1[0];
    for i in 0..res.len() {
        let prod = a[i].wrapping_mul(b[i]); // < Q² < 2^64, wrapping is exact
        let reduced = barrett_raw_u64(prod, cr, q);
        // res[i] < Q, reduced < Q, so sum < 2Q < 2^33 — fits in u64
        let sum = res[i] + reduced;
        res[i] = if sum >= q { sum - q } else { sum };
    }
}

/// Dispatch wrapper: calls single-mod or CRT multiply-add based on crt_count.
pub fn multiply_add_poly(params: &Params, res: &mut [u64], a: &[u64], b: &[u64]) {
    if params.crt_count == 1 {
        multiply_add_poly_single_mod(params, res, a, b);
    } else {
        multiply_add_poly_avx(params, res, a, b);
    }
}

/// SandwichPIR: inner product of polynomial ranges for a single NTT modulus.
/// Replaces `multiply_poly_avx_in_range` for crt_count=1.
/// Accumulates in u128 (length products of ~32-bit values can exceed u64),
/// but only reduces mod Q once per coefficient (not per product).
pub fn multiply_poly_single_mod(params: &Params, res: &mut [u64], a: &[u64], b: &[u64],
    begin_a: usize, begin_b: usize, length: usize)
{
    let poly_sz = params.poly_len;
    let q = params.moduli[0] as u128;
    for i in 0..poly_sz {
        let mut sum: u128 = 0;
        for k in 0..length {
            sum += a[(begin_a + k) * poly_sz + i] as u128
                 * b[(begin_b + k) * poly_sz + i] as u128;
        }
        res[i] = (sum % q) as u64; // single mod per coefficient, not per product
    }
}

#[cfg(target_feature = "avx512f")]
pub fn multiply_poly_avx(_params: &Params, res: &mut [u64], a: &[u64], b: &[u64]) {
    unsafe {
        let a_ptr = a.as_ptr();
        let b_ptr = b.as_ptr();
        let res_ptr = res.as_mut_ptr();

        for i in (0..res.len()).step_by(8) {
            let p_x = a_ptr.add(i);
            let p_y = b_ptr.add(i);
            let p_z = res_ptr.add(i);

            let x = _mm512_load_si512(p_x as *const _);
            let y = _mm512_load_si512(p_y as *const _);

            let product = _mm512_mul_epu32(x, y);

            _mm512_store_si512(p_z as *mut _, product);
        }
    }
}

#[cfg(not(target_feature = "avx512f"))]
pub fn multiply_poly_avx(_params: &Params, res: &mut [u64], a: &[u64], b: &[u64]) {
    for i in 0..res.len() {
        res[i] = (a[i] & 0xFFFFFFFF).wrapping_mul(b[i] & 0xFFFFFFFF);
    }
}

pub fn fast_add_into_no_reduce(res: &mut PolyMatrixNTT, a: &PolyMatrixNTT) {
    assert!(res.rows == a.rows);
    assert!(res.cols == a.cols);

    let a_slc = a.as_slice();
    let res_slc = res.as_mut_slice();
    for (res_chunk, a_chunk) in res_slc.chunks_exact_mut(8).zip(a_slc.chunks_exact(8)) {
        for i in 0..8 {
            res_chunk[i] += a_chunk[i];
        }
    }
}

pub fn fast_reduce(res: &mut PolyMatrixNTT) {
    let params = res.params;
    let res_slc = res.as_mut_slice();
    for m in 0..params.crt_count {
        for i in 0..params.poly_len {
            let idx = m * params.poly_len + i;
            // res_slc[idx] = barrett_coeff_u64(params, res_slc[idx], m);
            unsafe {
                let p = res_slc.as_mut_ptr().add(idx);
                *p = barrett_coeff_u64(params, *p, m);
            }
        }
    }
}

pub fn prep_pack_lwes<'a>(
    params: &'a Params,
    lwe_cts: &[u64],
    cols_to_do: usize,
) -> Vec<PolyMatrixNTT<'a>> {
    let lwe_cts_size = params.poly_len * (params.poly_len + 1); // CDKS square
    assert_eq!(lwe_cts.len(), lwe_cts_size);

    assert!(cols_to_do == params.poly_len);

    let mut rlwe_cts = Vec::new();
    for i in 0..cols_to_do {
        let mut rlwe_ct = PolyMatrixRaw::zero(params, 2, 1);

        // 'a' vector
        // put this in negacyclic order
        let mut poly = Vec::new();
        for j in 0..params.poly_len {
            poly.push(lwe_cts[j * params.poly_len + i])
        }
        // rotate per CDKS 3.2 / JeremyKun (sign-flip and reorder them)
        let nega = negacyclic_perm(&poly, 0, params.modulus);

        for j in 0..params.poly_len {
            rlwe_ct.get_poly_mut(0, 0)[j] = nega[j];
        }
        // 'b' scalar (skip)

        rlwe_cts.push(rlwe_ct.ntt());
    }

    rlwe_cts
}

pub fn prep_pack_many_lwes<'a>(
    params: &'a Params,
    lwe_cts: &[u64],
    num_rlwe_outputs: usize,
) -> Vec<Vec<PolyMatrixNTT<'a>>> {
    let lwe_cts_size = (params.poly_len + 1) * (num_rlwe_outputs * params.poly_len);
    assert_eq!(lwe_cts.len(), lwe_cts_size);

    // vecs stores rho LWE squares
    // each LWE square is of dim d2x(d2+1) (each column is a single LWE)
    let mut vecs = Vec::new();
    for i in 0..num_rlwe_outputs {
        let mut v = Vec::new();
        // for each row
        for j in 0..params.poly_len + 1 {
            // grab d2 many elements
            v.extend(
                &lwe_cts[j * (num_rlwe_outputs * params.poly_len) + i * params.poly_len..][..params.poly_len],
            );
        }
        // v stores poly_len x poly_len+1 
        vecs.push(v);
    }

    let mut res = Vec::new();
    for i in 0..num_rlwe_outputs {
        res.push(prep_pack_lwes(params, &vecs[i], params.poly_len));
    }

    res
}

pub fn pack_lwes<'a>(
    params: &'a Params,
    b_values: &[u64],
    preped_rlwe_cts: &[PolyMatrixNTT<'a>],
    preped_packed_vals: &[PolyMatrixNTT<'a>],
    cols_to_do: usize,
    pub_params: &[PolyMatrixNTT<'a>],
    y_constants: &(Vec<PolyMatrixNTT<'a>>, Vec<PolyMatrixNTT<'a>>),
) -> PolyMatrixNTT<'a> {
    assert_eq!(preped_rlwe_cts.len(), cols_to_do);
    assert_eq!(cols_to_do, params.poly_len);
    assert_eq!(b_values.len(), params.poly_len);

    let now = Instant::now();
    let preped_packed_val_opt = if preped_packed_vals.len() == 0 {
        None
    } else {
        Some(preped_packed_vals)
    };
    let out = pack_lwes_inner_non_recursive(
        params,
        params.poly_len_log2,
        0,
        &preped_rlwe_cts,
        pub_params,
        y_constants,
        preped_packed_val_opt,
        None,
    );
    let mut out_raw = out.raw();
    for z in 0..params.poly_len {
        let val = barrett_reduction_u128(params, b_values[z] as u128 * params.poly_len as u128);
        out_raw.get_poly_mut(1, 0)[z] = barrett_u64(params, out_raw.get_poly(1, 0)[z] + val);
    }
    let res = out_raw.ntt();
    debug!("True pack took {} us", now.elapsed().as_micros());

    res
}

pub fn pack_many_lwes<'a>(
    params: &'a Params,
    prep_rlwe_cts: &[Vec<PolyMatrixNTT<'a>>],
    precomp: &Precomp<'a>,
    b_values: &[u64],
    num_rlwe_outputs: usize,
    pack_pub_params_row_1s: &[PolyMatrixNTT<'a>],
    y_constants: &(Vec<PolyMatrixNTT<'a>>, Vec<PolyMatrixNTT<'a>>),
) -> Vec<PolyMatrixNTT<'a>> {
    assert_eq!(prep_rlwe_cts.len(), num_rlwe_outputs);
    assert_eq!(prep_rlwe_cts[0].len(), params.poly_len);
    assert_eq!(b_values.len(), num_rlwe_outputs * params.poly_len);

    let mut res = Vec::new();
    for i in 0..num_rlwe_outputs {
        let (precomp_res, precomp_vals, precomp_tables) = &precomp[i];

        let packed = pack_using_precomp_vals(
            &params,
            params.poly_len_log2,
            &pack_pub_params_row_1s,
            &b_values[i * params.poly_len..(i + 1) * params.poly_len],
            &precomp_res,
            &precomp_vals,
            &precomp_tables,
            &y_constants,
        );

        res.push(packed);
    }

    res
}

// similar to generate_y_constants
fn rotation_poly<'a>(params: &'a Params, amount: usize) -> PolyMatrixNTT<'a> {
    let mut res = PolyMatrixRaw::zero(params, 1, 1);
    res.data[amount] = 1;
    res.ntt()
}
pub fn pack_using_single_with_offset<'a>(
    params: &'a Params,
    pub_params: &[PolyMatrixNTT<'a>],
    cts: &[PolyMatrixNTT<'a>],
    offset: usize,
) -> PolyMatrixNTT<'a> {
    let mut res = PolyMatrixNTT::zero(params, 2, 1);
    for i in 0..cts.len() {
        let packed_single = pack_single_lwe(params, pub_params, &cts[i]);
        let rotation = rotation_poly(params, offset + i);
        let rotated = scalar_multiply_alloc(&rotation, &packed_single);
        add_into(&mut res, &rotated);
    }
    res
}

// this is a really neat trick
// in NTT space, an automorphism is just a permutation of the NTT values (unlike in coefficient space)
// so:
// we randomly sample a poly, get its NTT
// we automorph the poly (for each possible tau (#log(d2))), get its NTT
// we match NTT values to find which slot mapped to which
// Compute NTT-domain automorphism permutation tables analytically.
// NTT index i evaluates at ψ^(2·bitrev(i)+1). Automorphism τ_t maps ψ^s → ψ^(ts mod 2n).
// So table[j] = bitrev((t·(2·bitrev(j)+1) mod 2n - 1) / 2).
// Pure integer arithmetic — no NTTs, no allocations, no probe polynomials.
pub fn generate_automorph_tables_brute_force(params: &Params) -> Vec<Vec<usize>> {
    let n = params.poly_len;
    let log_n = params.poly_len_log2;
    let two_n = 2 * n;

    // Precompute bitrev table
    let bitrev: Vec<usize> = (0..n).map(|i| {
        let mut r = 0usize;
        let mut x = i;
        for _ in 0..log_n {
            r = (r << 1) | (x & 1);
            x >>= 1;
        }
        r
    }).collect();

    let mut tables = Vec::with_capacity(n);
    for t in (1..two_n).step_by(2) {
        let mut table = vec![0usize; n];
        for j in 0..n {
            let s_j = 2 * bitrev[j] + 1;       // evaluation point for index j
            let s_prime = (t * s_j) % two_n;    // mapped evaluation point (always odd)
            let k = (s_prime - 1) / 2;          // index in bitrev domain
            table[j] = bitrev[k];               // NTT index for that evaluation point
        }
        tables.push(table);
    }
    tables
}

pub fn apply_automorph_ntt_raw<'a>(
    params: &Params,
    poly: &[u64],
    out: &mut [u64],
    t: usize,
    tables: &[Vec<usize>],
) {
    let poly_len = params.poly_len;
    let table_idx = (t - 1) / 2;
    let table = &tables[table_idx];

    for i in 0..poly_len {
        out[i] += poly[table[i]];
    }
}

pub fn apply_automorph_ntt<'a>(
    params: &'a Params,
    tables: &[Vec<usize>],
    mat: &PolyMatrixNTT<'a>,
    res: &mut PolyMatrixNTT<'a>,
    t: usize,
) {
    // run apply_automorph_ntt on each poly in the matrix
    // let mut res = PolyMatrixNTT::zero(params, mat.rows, mat.cols);
    for i in 0..mat.rows {
        for j in 0..mat.cols {
            let poly = mat.get_poly(i, j);
            let res_poly = res.get_poly_mut(i, j);
            for (chunk, res_chunk) in poly
                .chunks_exact(params.poly_len)
                .zip(res_poly.chunks_exact_mut(params.poly_len))
            {
                apply_automorph_ntt_raw(params, chunk, res_chunk, t, tables);
            }
        }
    }
    // res
}


#[cfg(test)]
mod test {
    use rand::SeedableRng;
    use rand_chacha::ChaCha20Rng;
    use spiral_rs::{client::Client, number_theory::invert_uint_mod, util::get_test_params};

    use crate::{
        client::raw_generate_expansion_params,
        server::generate_y_constants,
    };

    // Stub for tests: SandwichPIR doesn't use DoublePIR params
    fn params_for_scenario(num_items: usize, _item_size_bits: usize) -> spiral_rs::params::Params {
        crate::params::params_for_sandwichpir(num_items, 1 << 20)
    }

    use super::*;

    #[test]
    fn test_packing() {
        let params = get_test_params();
        let mut client = Client::init(&params);
        client.generate_secret_keys();
        let y_constants = generate_y_constants(&params);

        let pack_seed = [1u8; 32];
        let cts_seed = [2u8; 32];
        let mut ct_pub_rng = ChaCha20Rng::from_seed(cts_seed);

        let pack_pub_params = raw_generate_expansion_params(
            &params,
            client.get_sk_reg(),
            params.poly_len_log2,
            params.t_exp_left,
            &mut ChaCha20Rng::from_entropy(),
            &mut ChaCha20Rng::from_seed(pack_seed),
        );

        // generate poly_len ciphertexts
        let mut v_ct = Vec::new();
        let mut b_values = Vec::new();
        for i in 0..params.poly_len {
            let mut pt = PolyMatrixRaw::zero(&params, 1, 1);
            let val = i as u64 % params.pt_modulus;
            let scale_k = params.modulus / params.pt_modulus;
            let mod_inv = invert_uint_mod(params.poly_len as u64, params.modulus).unwrap();
            let val_to_enc = multiply_uint_mod(val * scale_k, mod_inv, params.modulus);
            pt.data[0] = val_to_enc;
            let ct = client.encrypt_matrix_reg(
                &pt.ntt(),
                &mut ChaCha20Rng::from_entropy(),
                &mut ct_pub_rng,
            );
            let mut ct_raw = ct.raw();

            // get the b value
            b_values.push(ct_raw.get_poly(1, 0)[0]);

            // zero out all of the second poly
            ct_raw.get_poly_mut(1, 0).fill(0);
            v_ct.push(ct_raw.ntt());
        }

        let now = Instant::now();
        let packed = pack_lwes(
            &params,
            &b_values,
            &v_ct,
            &[],
            params.poly_len,
            &pack_pub_params,
            &y_constants,
        );
        println!("Packing took {} us", now.elapsed().as_micros());

        let packed_raw = packed.raw();
        println!("packed_0: {:?}", &packed_raw.get_poly(0, 0)[..10]);
        assert_eq!(packed_raw.get_poly(0, 0)[0], 47649720264253743u64);

        // decrypt + decode
        let dec = client.decrypt_matrix_reg(&packed);
        let dec_raw = dec.raw();

        // rescale
        let mut rescaled = PolyMatrixRaw::zero(&params, 1, 1);
        for i in 0..params.poly_len {
            rescaled.data[i] = rescale(dec_raw.data[i], params.modulus, params.pt_modulus);
        }

        println!("rescaled: {:?}", &rescaled.as_slice()[..50]);
        let mut gold = PolyMatrixRaw::zero(&params, 1, 1);
        for i in 0..params.poly_len {
            gold.data[i] = i as u64 % params.pt_modulus;
        }
        assert_eq!(rescaled.as_slice(), gold.as_slice());
    }

    #[test]
    #[ignore]
    fn test_precompute_packing() {
        let params = params_for_scenario(1 << 30, 1);
        println!("modulus: {}", params.modulus);
        let mut client = Client::init(&params);
        client.generate_secret_keys();
        let y_constants = generate_y_constants(&params);

        let pack_seed = [1u8; 32];
        let cts_seed = [2u8; 32];
        let mut ct_pub_rng = ChaCha20Rng::from_seed(cts_seed);

        let pack_pub_params = raw_generate_expansion_params(
            &params,
            client.get_sk_reg(),
            params.poly_len_log2,
            params.t_exp_left,
            &mut ChaCha20Rng::from_entropy(),
            &mut ChaCha20Rng::from_seed(pack_seed),
        );

        let mut fake_pack_pub_params = pack_pub_params.clone();
        // zero out all of the second rows
        for i in 0..pack_pub_params.len() {
            for col in 0..pack_pub_params[i].cols {
                fake_pack_pub_params[i].get_poly_mut(1, col).fill(0);
            }
        }

        let mut pack_pub_params_row_1s = pack_pub_params.clone();
        for i in 0..pack_pub_params.len() {
            pack_pub_params_row_1s[i] =
                pack_pub_params[i].submatrix(1, 0, 1, pack_pub_params[i].cols);
            pack_pub_params_row_1s[i] = condense_matrix(&params, &pack_pub_params_row_1s[i]);
        }

        // generate poly_len ciphertexts
        let mut v_ct = Vec::new();
        let mut b_values = Vec::new();
        for i in 0..params.poly_len {
            let mut pt = PolyMatrixRaw::zero(&params, 1, 1);
            let val = i as u64 % params.pt_modulus;
            let scale_k = params.modulus / params.pt_modulus;
            let mod_inv = invert_uint_mod(params.poly_len as u64, params.modulus).unwrap();
            let val_to_enc = multiply_uint_mod(val * scale_k, mod_inv, params.modulus);
            pt.data[0] = val_to_enc;
            let ct = client.encrypt_matrix_reg(
                &pt.ntt(),
                &mut ChaCha20Rng::from_entropy(),
                &mut ct_pub_rng,
            );
            let mut ct_raw = ct.raw();

            // get the b value
            b_values.push(ct_raw.get_poly(1, 0)[0]);

            // zero out all of the second poly
            ct_raw.get_poly_mut(1, 0).fill(0);
            v_ct.push(ct_raw.ntt());
        }

        let now = Instant::now();
        let (precomp_res, precomp_vals, precomp_tables) = precompute_pack(
            &params,
            params.poly_len_log2,
            &v_ct,
            &fake_pack_pub_params,
            &y_constants,
        );
        println!(
            "Precomputing for packing took {} us",
            now.elapsed().as_micros()
        );

        println!("t_exp_left: {}", params.t_exp_left);

        let now = Instant::now();
        let packed = pack_using_precomp_vals(
            &params,
            params.poly_len_log2,
            &pack_pub_params_row_1s,
            &b_values,
            &precomp_res,
            &precomp_vals,
            &precomp_tables,
            &y_constants,
        );
        println!("Packing took {} us", now.elapsed().as_micros());

        let packed_raw = packed.raw();
        println!("packed_0: {:?}", &packed_raw.get_poly(0, 0)[..10]);
        // assert_eq!(packed_raw.get_poly(0, 0)[0], 17210016925609510u64);

        // decrypt + decode
        let dec = client.decrypt_matrix_reg(&packed);
        let dec_raw = dec.raw();

        // rescale
        let mut rescaled = PolyMatrixRaw::zero(&params, 1, 1);
        for i in 0..params.poly_len {
            rescaled.data[i] = rescale(dec_raw.data[i], params.modulus, params.pt_modulus);
        }

        println!("rescaled: {:?}", &rescaled.as_slice()[..50]);
        let mut gold = PolyMatrixRaw::zero(&params, 1, 1);
        for i in 0..params.poly_len {
            gold.data[i] = i as u64 % params.pt_modulus;
        }
        assert_eq!(rescaled.as_slice(), gold.as_slice());
    }

    #[test]
    #[ignore]
    fn test_single_packing() {
        let params = params_for_scenario(1 << 30, 1);
        let mut client = Client::init(&params);
        client.generate_secret_keys();

        let pack_seed = [1u8; 32];
        let cts_seed = [2u8; 32];
        let mut ct_pub_rng = ChaCha20Rng::from_seed(cts_seed);

        let pack_pub_params = raw_generate_expansion_params(
            &params,
            client.get_sk_reg(),
            params.poly_len_log2,
            params.t_exp_left,
            &mut ChaCha20Rng::from_entropy(),
            &mut ChaCha20Rng::from_seed(pack_seed),
        );

        // generate 1 ciphertext
        let sentinel_val = 99;
        let mut v_ct = Vec::new();
        for _i in 0..1 {
            let mut pt = PolyMatrixRaw::zero(&params, 1, 1);
            let val = sentinel_val % params.pt_modulus;
            let scale_k = params.modulus / params.pt_modulus;
            let mod_inv = invert_uint_mod(params.poly_len as u64, params.modulus).unwrap();
            let val_to_enc = multiply_uint_mod(val * scale_k, mod_inv, params.modulus);
            pt.data[0] = val_to_enc;
            let ct = client.encrypt_matrix_reg(
                &pt.ntt(),
                &mut ChaCha20Rng::from_entropy(),
                &mut ct_pub_rng,
            );
            v_ct.push(ct);
        }
        let now = Instant::now();
        let packed = pack_single_lwe(&params, &pack_pub_params, &v_ct[0]);
        println!("Packing took {} us", now.elapsed().as_micros());

        let packed_raw = packed.raw();
        println!("packed_0: {:?}", &packed_raw.get_poly(0, 0)[..10]);

        // decrypt + decode
        let dec = client.decrypt_matrix_reg(&packed);
        let dec_raw = dec.raw();

        // rescale
        let mut rescaled = PolyMatrixRaw::zero(&params, 1, 1);
        for i in 0..params.poly_len {
            rescaled.data[i] = rescale(dec_raw.data[i], params.modulus, params.pt_modulus);
        }

        println!("rescaled: {:?}", &rescaled.as_slice()[..50]);
        assert_eq!(rescaled.as_slice()[0], sentinel_val);
    }

    #[test]
    fn test_automorph_tables() {
        let params = params_for_scenario(1 << 30, 1);

        let now = Instant::now();
        let tables = generate_automorph_tables_brute_force(&params);
        println!("Generating tables took {} us", now.elapsed().as_micros());

        let mut rng = ChaCha20Rng::from_seed([7u8; 32]);

        for t in ([3, 9, 17, 33, 65, 129, 257, 513, 1025, 2049])
            .into_iter()
            .rev()
        {
            let poly = PolyMatrixRaw::random_rng(&params, 1, 1, &mut rng);
            let poly_ntt = poly.ntt();

            let poly_auto = automorph_alloc(&poly, t);
            let poly_auto_ntt = poly_auto.ntt();
            let mut poly_auto_ntt_using_tables = PolyMatrixNTT::zero(&params, 1, 1);
            apply_automorph_ntt(
                &params,
                &tables,
                &poly_ntt,
                &mut poly_auto_ntt_using_tables,
                t,
            );

            println!("poly_ntt: {:?}", &poly_ntt.as_slice()[..30]);
            println!(
                "poly_auto_ntt_using_tables: {:?}",
                &poly_auto_ntt_using_tables.as_slice()[..30]
            );

            assert_eq!(
                &poly_auto_ntt.as_slice(),
                &poly_auto_ntt_using_tables.as_slice(),
                "t: {}",
                t
            );
        }
    }
}

// ============================================================================
// InspiRING Packing
// ============================================================================

pub fn gadget_invert_transposed_alloc<'a>(
    inp: &PolyMatrixRaw<'a>,
    num_digits: usize,
) -> PolyMatrixRaw<'a> {
    assert_eq!(inp.cols, 1);

    let params = inp.params;
    let mut out = PolyMatrixRaw::zero(&params, inp.rows, num_digits);

    let num_elems = num_digits;
    let bits_per = get_bits_per(params, num_elems);
    let mask = (1u64 << bits_per) - 1;

    for i in 0..inp.rows {
        for z in 0..params.poly_len {
            let val = inp.get_poly(i, 0)[z];
            for k in 0..num_elems {
                let bit_offs = k * bits_per;
                let piece = if bit_offs >= 64 { 0 } else { (val >> bit_offs) & mask };
                out.get_poly_mut(i, k)[z] = piece;
            }
        }
    }
    out
}

#[cfg(target_feature = "avx512f")]
pub fn fast_multiply_no_reduce_in_range(
    params: &Params,
    res: &mut PolyMatrixNTT,
    a: &PolyMatrixNTT,
    b: &PolyMatrixNTT,
    begin_a: usize,
    begin_b: usize,
    length: usize,
) {
    assert_eq!(params.crt_count, 2);
    assert_eq!(params.poly_len % 1024, 0);

    unsafe {
        let a_ptr = a.as_slice().as_ptr();
        let b_ptr = b.as_slice().as_ptr();
        let res_ptr = res.as_mut_slice().as_mut_ptr();
        let pol_sz = params.poly_len;

        for idx in (0..pol_sz).step_by(8) {
            let mut sum_lo = _mm512_setzero_si512();
            let mut sum_hi = _mm512_setzero_si512();
            for k in 0..length {
                let p_x = a_ptr.add((begin_a + k) * 2 * pol_sz + idx);
                let p_y = b_ptr.add((begin_b + k) * 2 * pol_sz + idx);

                let x = _mm512_load_si512(p_x as *const _);
                let x_lo = x;
                let x_hi = _mm512_srli_epi64(x, 32);
                let y = _mm512_load_si512(p_y as *const _);
                let y_lo = y;
                let y_hi = _mm512_srli_epi64(y, 32);

                let product_lo = _mm512_mul_epu32(x_lo, y_lo);
                let product_hi = _mm512_mul_epu32(x_hi, y_hi);

                sum_lo = _mm512_add_epi64(sum_lo, product_lo);
                sum_hi = _mm512_add_epi64(sum_hi, product_hi);
            }

            let p_z = res_ptr.add(idx);
            _mm512_store_si512(p_z as *mut _, sum_lo);
            let p_z = res_ptr.add(pol_sz + idx);
            _mm512_store_si512(p_z as *mut _, sum_hi);
        }
    }
}

#[cfg(not(target_feature = "avx512f"))]
pub fn fast_multiply_no_reduce_in_range(
    params: &Params,
    res: &mut PolyMatrixNTT,
    a: &PolyMatrixNTT,
    b: &PolyMatrixNTT,
    begin_a: usize,
    begin_b: usize,
    length: usize,
) {
    assert_eq!(params.crt_count, 2);

    let a_slc = a.as_slice();
    let b_slc = b.as_slice();
    let res_slc = res.as_mut_slice();
    let pol_sz = params.poly_len;

    for idx in 0..pol_sz {
        let mut sum_lo: u64 = 0;
        let mut sum_hi: u64 = 0;
        for k in 0..length {
            let ax = a_slc[(begin_a + k) * 2 * pol_sz + idx];
            let bx = b_slc[(begin_b + k) * 2 * pol_sz + idx];
            sum_lo = sum_lo.wrapping_add((ax & 0xFFFFFFFF).wrapping_mul(bx & 0xFFFFFFFF));
            sum_hi = sum_hi.wrapping_add((ax >> 32).wrapping_mul(bx >> 32));
        }
        res_slc[idx] = sum_lo;
        res_slc[pol_sz + idx] = sum_hi;
    }
}

#[cfg(target_feature = "avx512f")]
pub fn multiply_poly_avx_in_range(_params: &Params, res: &mut [u64], a: &[u64], b: &[u64], begin_a: usize, begin_b: usize, length: usize) {
    unsafe {
        let a_ptr = a.as_ptr();
        let b_ptr = b.as_ptr();
        let res_ptr = res.as_mut_ptr();
        let poly_sz = _params.poly_len;

        for i in (0..poly_sz).step_by(8) {
            let p_z = res_ptr.add(i);
            let mut sum = _mm512_setzero_si512();
            for k in 0..length {
                let p_x = a_ptr.add((begin_a + k) * poly_sz + i);
                let p_y = b_ptr.add((begin_b + k) * poly_sz + i);

                let x = _mm512_load_si512(p_x as *const _);
                let y = _mm512_load_si512(p_y as *const _);

                let product = _mm512_mul_epu32(x, y);
                sum = _mm512_add_epi64(sum, product);
            }
            _mm512_store_si512(p_z as *mut _, sum);
        }
    }
}

#[cfg(not(target_feature = "avx512f"))]
pub fn multiply_poly_avx_in_range(_params: &Params, res: &mut [u64], a: &[u64], b: &[u64], begin_a: usize, begin_b: usize, length: usize) {
    let poly_sz = _params.poly_len;
    for i in 0..poly_sz {
        let mut sum: u64 = 0;
        for k in 0..length {
            let ax = a[(begin_a + k) * poly_sz + i];
            let bx = b[(begin_b + k) * poly_sz + i];
            sum = sum.wrapping_add((ax & 0xFFFFFFFF).wrapping_mul(bx & 0xFFFFFFFF));
        }
        res[i] = sum;
    }
}

pub fn fast_multiply_no_reduce_in_range_generic(
    params: &Params,
    res: &mut PolyMatrixNTT,
    a: &PolyMatrixNTT,
    b: &PolyMatrixNTT,
    begin_a: usize,
    begin_b: usize,
    length: usize,
) {
    if params.crt_count == 1 {
        let a_ptr = a.as_slice();
        let b_ptr = b.as_slice();
        let res_ptr = res.as_mut_slice();
        multiply_poly_single_mod(params, res_ptr, a_ptr, b_ptr, begin_a, begin_b, length);
    } else {
        fast_multiply_no_reduce_in_range(params, res, a, b, begin_a, begin_b, length);
    }
}

pub fn apply_automorph_ntt_double<'a>(
    params: &'a Params,
    tables: &[Vec<usize>],
    mat: &PolyMatrixNTT<'a>,
    res_1: &mut PolyMatrixNTT<'a>,
    res_2: &mut PolyMatrixNTT<'a>,
    t: usize,
) {
    for i in 0..mat.rows {
        for j in 0..mat.cols {
            let poly = mat.get_poly(i, j);
            let mut res_1_poly: Vec<&mut [u64]> = res_1.get_poly_mut(i, j).chunks_exact_mut(params.poly_len).collect();
            let mut res_2_poly: Vec<&mut [u64]> = res_2.get_poly_mut(i, j).chunks_exact_mut(params.poly_len).collect();
            for (index, chunk) in poly.chunks_exact(params.poly_len).enumerate()
            {
                apply_automorph_ntt_raw(params, chunk, res_1_poly[index], t, tables);
                apply_automorph_ntt_raw(params, chunk, res_2_poly[index], 2*params.poly_len - t, tables);
            }
        }
    }
}

// --- InspiRING Types ---

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PackingType {
    NoPacking,
    CDKS,
    InspiRING,
}

impl Default for PackingType {
    fn default() -> Self {
        PackingType::CDKS
    }
}

#[derive(Clone)]
pub struct PackParams<'a> {
    pub params: &'a Params,
    pub num_to_pack: usize,
    pub tables: Vec<Vec<usize>>,
    pub gen_pows: Vec<usize>,
    pub mod_inv_poly: PolyMatrixNTT<'a>,
    pub monomial_ntts: Vec<PolyMatrixNTT<'a>>,
    pub neg_monomial_ntts: Vec<PolyMatrixNTT<'a>>,
}

impl PackParams<'_> {
    pub fn new<'a>(params: &'a Params, num_to_pack: usize) -> PackParams<'a> {
        Self::new_inner(params, num_to_pack, false)
    }

    /// GPU path: skip monomial NTTs (GPU computes its own via sw_compute_monomials).
    /// Saves 2*poly_len NTT calls on CPU.
    pub fn new_for_gpu<'a>(params: &'a Params, num_to_pack: usize) -> PackParams<'a> {
        Self::new_inner(params, num_to_pack, true)
    }

    fn new_inner<'a>(params: &'a Params, num_to_pack: usize, skip_monomials: bool) -> PackParams<'a> {
        debug!("Starting tables");
        let tables = generate_automorph_tables_brute_force(params);
        debug!("Got tables");
        let gen: usize =
            if num_to_pack < params.poly_len { (2 * params.poly_len / num_to_pack) + 1 } else { 5 };

        let mut gen_pows = Vec::new();
        for i in 0..params.poly_len {
            gen_pows
                .push(exponentiate_uint_mod(gen as u64, i as u64, 2 * params.poly_len as u64)
                    as usize);
        }
        let mod_inv = invert_uint_mod(num_to_pack as u64, params.modulus).unwrap();
        let mod_inv_poly = single_poly(params, mod_inv).ntt();

        let (monomial_ntts, neg_monomial_ntts) = if skip_monomials {
            (vec![], vec![])
        } else {
            let mut mono = Vec::with_capacity(params.poly_len);
            let mut neg_mono = Vec::with_capacity(params.poly_len);
            for j in 0..params.poly_len {
                let mut monomial = PolyMatrixRaw::zero(params, 1, 1);
                monomial.get_poly_mut(0, 0)[j] = 1;
                let mono_ntt = monomial.ntt();
                mono.push(mono_ntt.clone());
                neg_mono.push(-&mono_ntt);
            }
            (mono, neg_mono)
        };

        PackParams {
            params,
            num_to_pack,
            tables,
            gen_pows,
            mod_inv_poly,
            monomial_ntts,
            neg_monomial_ntts,
        }
    }

    pub fn new_fast<'a>(params: &'a Params, num_to_pack: usize) -> PackParams<'a> {
        let gen: usize =
            if num_to_pack < params.poly_len { (2 * params.poly_len / num_to_pack) + 1 } else { 5 };

        let mut gen_pows = Vec::new();
        for i in 0..params.poly_len {
            gen_pows
                .push(exponentiate_uint_mod(gen as u64, i as u64, 2 * params.poly_len as u64)
                    as usize);
        }

        PackParams {
            params,
            num_to_pack,
            tables: vec![],
            gen_pows,
            mod_inv_poly: PolyMatrixNTT::zero(&params, 1, 1),
            monomial_ntts: vec![],
            neg_monomial_ntts: vec![],
        }
    }

    pub fn empty<'a>(params: &'a Params) -> PackParams<'a> {
        PackParams {
            params,
            num_to_pack: 0,
            tables: vec![],
            gen_pows: vec![],
            mod_inv_poly: PolyMatrixNTT::zero(&params, 1, 1),
            monomial_ntts: vec![],
            neg_monomial_ntts: vec![],
        }
    }
}

#[derive(Clone)]
pub struct PrecompInsPIR<'a> {
    pub a_hat: PolyMatrixRaw<'a>,
    pub bold_t_condensed: PolyMatrixNTT<'a>,
    pub bold_t_bar_condensed: PolyMatrixNTT<'a>,
    pub bold_t_hat_condensed: PolyMatrixNTT<'a>,
}

pub fn generate_rotations<'a>(
    packing_params: &PackParams<'a>,
    to_rotate: &PolyMatrixNTT<'a>,
) -> PolyMatrixNTT<'a> {
    let params = packing_params.params;
    let num_to_pack = packing_params.num_to_pack;
    let tables = &packing_params.tables;
    let gen_pows = &packing_params.gen_pows;

    let mut rotations_all = PolyMatrixNTT::zero(&params, num_to_pack - 1, params.t_exp_left);
    for i in 0..num_to_pack - 1 {
        let mut rotated_w = PolyMatrixNTT::zero(&params, 1, params.t_exp_left);
        apply_automorph_ntt(&params, &tables, &to_rotate, &mut rotated_w, gen_pows[i]);
        rotations_all.copy_into(&rotated_w, i, 0);
    }
    rotations_all
}

pub fn generate_rotations_double<'a>(
    packing_params: &PackParams<'a>,
    to_rotate: &PolyMatrixNTT<'a>,
) -> (PolyMatrixNTT<'a>, PolyMatrixNTT<'a>) {
    let params = packing_params.params;
    let num_to_pack = packing_params.num_to_pack;
    let tables = &packing_params.tables;
    let gen_pows = &packing_params.gen_pows;
    assert_eq!(num_to_pack, params.poly_len);

    let num_rotations = params.poly_len / 2 - 1;

    let mut rotations_all = PolyMatrixNTT::zero(&params, num_rotations, params.t_exp_left);
    let mut rotations_bar_all = PolyMatrixNTT::zero(&params, num_rotations, params.t_exp_left);
    for i in 0..num_rotations {
        let mut rotated_w_1 = PolyMatrixNTT::zero(&params, 1, params.t_exp_left);
        let mut rotated_w_2 = PolyMatrixNTT::zero(&params, 1, params.t_exp_left);
        apply_automorph_ntt_double(&params, &tables, &to_rotate, &mut rotated_w_1, &mut rotated_w_2, gen_pows[i]);
        rotations_all.copy_into(&rotated_w_1, i, 0);
        rotations_bar_all.copy_into(&rotated_w_2, i, 0);
    }
    (rotations_all, rotations_bar_all)
}

// --- Offline/Online Packing Keys ---

#[derive(Clone)]
pub struct OfflinePackingKeys<'a> {
    pub full_key: bool,
    pub w_seed: [u8; 32],
    pub v_seed: [u8; 32],
    pub w_mask: Option<PolyMatrixNTT<'a>>,
    pub v_mask: Option<PolyMatrixNTT<'a>>,
    pub w_all: Option<PolyMatrixNTT<'a>>,
    pub w_bar_all: Option<PolyMatrixNTT<'a>>,
}

impl OfflinePackingKeys<'_> {
    pub fn init_empty<'a>() -> OfflinePackingKeys<'a> {
        OfflinePackingKeys {
            full_key: false,
            w_seed: [0; 32],
            v_seed: [0; 32],
            w_mask: None,
            w_all: None,
            w_bar_all: None,
            v_mask: None,
        }
    }

    pub fn init<'a>(packing_params: &PackParams<'a>, w_seed: [u8; 32]) -> OfflinePackingKeys<'a> {
        let w_mask = PolyMatrixNTT::random_rng(
            &packing_params.params,
            1,
            packing_params.params.t_exp_left,
            &mut ChaCha20Rng::from_seed(w_seed),
        );
        let x = generate_rotations(&packing_params, &w_mask);
        let w_all = Some(x);
        OfflinePackingKeys {
            full_key: false,
            w_seed,
            v_seed: [0; 32],
            w_mask: Some(w_mask),
            w_all,
            w_bar_all: None,
            v_mask: None,
        }
    }

    pub fn init_full<'a>(packing_params: &PackParams<'a>, w_seed: [u8; 32], v_seed: [u8; 32]) -> OfflinePackingKeys<'a> {
        let w_mask = PolyMatrixNTT::random_rng(
            &packing_params.params,
            1,
            packing_params.params.t_exp_left,
            &mut ChaCha20Rng::from_seed(w_seed),
        );
        let v_mask = PolyMatrixNTT::random_rng(
            &packing_params.params,
            1,
            packing_params.params.t_exp_left,
            &mut ChaCha20Rng::from_seed(v_seed),
        );

        let (x, y) = generate_rotations_double(&packing_params, &w_mask);
        let (w_all, w_bar_all) = (Some(x), Some(y));

        OfflinePackingKeys {
            full_key: true,
            w_seed,
            v_seed,
            w_mask: Some(w_mask),
            w_all,
            w_bar_all,
            v_mask: Some(v_mask),
        }
    }

    /// GPU path: only generate w_mask and v_mask from seeds.
    /// Skips generate_rotations_double (GPU precomp computes its own rotations).
    pub fn init_masks_only<'a>(params: &'a Params, w_seed: [u8; 32], v_seed: [u8; 32]) -> OfflinePackingKeys<'a> {
        let w_mask = PolyMatrixNTT::random_rng(
            params, 1, params.t_exp_left,
            &mut ChaCha20Rng::from_seed(w_seed),
        );
        let v_mask = PolyMatrixNTT::random_rng(
            params, 1, params.t_exp_left,
            &mut ChaCha20Rng::from_seed(v_seed),
        );
        OfflinePackingKeys {
            full_key: true,
            w_seed,
            v_seed,
            w_mask: Some(w_mask),
            w_all: None,
            w_bar_all: None,
            v_mask: Some(v_mask),
        }
    }
}

#[derive(Clone)]
pub struct PackingKeys<'a> {
    pub packing_type: PackingType,
    // InspiRING fields
    pub full_key: bool,
    pub gamma: usize,
    pub y_body: Option<PolyMatrixNTT<'a>>,
    pub z_body: Option<PolyMatrixNTT<'a>>,
    pub y_body_condensed: Option<PolyMatrixNTT<'a>>,
    pub z_body_condensed: Option<PolyMatrixNTT<'a>>,
    pub expanded: bool,
    pub y_all_condensed: Option<PolyMatrixNTT<'a>>,
    pub y_bar_all_condensed: Option<PolyMatrixNTT<'a>>,
    // CDKS fields
    pub params: Option<Params>,
    pub pack_pub_params_row_1s: Vec<PolyMatrixNTT<'a>>,
    pub fake_pack_pub_params: Vec<PolyMatrixNTT<'a>>,
}

impl<'a> PackingKeys<'a> {
    pub fn init_full(
        packing_params: &PackParams<'a>,
        sk_reg: &PolyMatrixRaw<'a>,
        w_seed: [u8; 32],
        v_seed: [u8; 32],
    ) -> PackingKeys<'a> {
        let w_mask: PolyMatrixNTT<'_> = PolyMatrixNTT::random_rng(
            &packing_params.params,
            1,
            packing_params.params.t_exp_left,
            &mut ChaCha20Rng::from_seed(w_seed),
        );
        let v_mask: PolyMatrixNTT<'_> = PolyMatrixNTT::random_rng(
            &packing_params.params,
            1,
            packing_params.params.t_exp_left,
            &mut ChaCha20Rng::from_seed(v_seed),
        );
        let y_body = generate_ksk_body(
            &packing_params.params,
            &sk_reg,
            packing_params.gen_pows[1],
            &w_mask,
            &mut ChaCha20Rng::from_entropy(),
        );
        let z_body = generate_ksk_body(
            &packing_params.params,
            &sk_reg,
            2 * packing_params.params.poly_len - 1,
            &v_mask,
            &mut ChaCha20Rng::from_entropy(),
        );
        let y_body_condensed = condense_matrix(&packing_params.params, &y_body);
        let z_body_condensed = condense_matrix(&packing_params.params, &z_body);
        PackingKeys {
            packing_type: PackingType::InspiRING,
            gamma: packing_params.num_to_pack,
            full_key: true,
            y_body: Some(y_body),
            z_body: Some(z_body),
            y_body_condensed: Some(y_body_condensed),
            z_body_condensed: Some(z_body_condensed),
            expanded: false,
            y_all_condensed: None,
            y_bar_all_condensed: None,
            params: None,
            pack_pub_params_row_1s: vec![],
            fake_pack_pub_params: vec![],
        }
    }

    pub fn init(
        packing_params: &PackParams<'a>,
        sk_reg: &PolyMatrixRaw<'a>,
        w_seed: [u8; 32],
    ) -> PackingKeys<'a> {
        let w_mask: PolyMatrixNTT<'_> = PolyMatrixNTT::random_rng(
            &packing_params.params,
            1,
            packing_params.params.t_exp_left,
            &mut ChaCha20Rng::from_seed(w_seed),
        );
        let y_body = generate_ksk_body(
            &packing_params.params,
            &sk_reg,
            packing_params.gen_pows[1],
            &w_mask,
            &mut ChaCha20Rng::from_entropy(),
        );
        let y_body_condensed = condense_matrix(&packing_params.params, &y_body);
        PackingKeys {
            packing_type: PackingType::InspiRING,
            gamma: packing_params.num_to_pack,
            full_key: false,
            y_body: Some(y_body),
            z_body: None,
            y_body_condensed: Some(y_body_condensed),
            z_body_condensed: None,
            expanded: false,
            y_all_condensed: None,
            y_bar_all_condensed: None,
            params: None,
            pack_pub_params_row_1s: vec![],
            fake_pack_pub_params: vec![],
        }
    }

    pub fn init_cdks(
        params: &'a Params,
        sk_reg: &PolyMatrixRaw<'a>,
        static_seed_2: [u8; 32],
    ) -> PackingKeys<'a> {
        let pack_pub_params = raw_generate_expansion_params(
            &params,
            &sk_reg,
            params.poly_len_log2,
            params.t_exp_left,
            &mut ChaCha20Rng::from_entropy(),
            &mut ChaCha20Rng::from_seed(static_seed_2),
        );
        let mut pack_pub_params_row_1s = pack_pub_params.to_vec();
        for i in 0..pack_pub_params.len() {
            pack_pub_params_row_1s[i] =
                pack_pub_params[i].submatrix(1, 0, 1, pack_pub_params[i].cols);
            pack_pub_params_row_1s[i] =
                condense_matrix(&params, &pack_pub_params_row_1s[i]);
        }

        let mut fake_pack_pub_params = pack_pub_params.clone();
        for i in 0..pack_pub_params.len() {
            for col in 0..pack_pub_params[i].cols {
                fake_pack_pub_params[i].get_poly_mut(1, col).fill(0);
            }
        }

        PackingKeys {
            packing_type: PackingType::CDKS,
            gamma: 0,
            full_key: false,
            y_body: None,
            z_body: None,
            y_body_condensed: None,
            z_body_condensed: None,
            expanded: false,
            y_all_condensed: None,
            y_bar_all_condensed: None,
            params: Some(params.clone()),
            pack_pub_params_row_1s,
            fake_pack_pub_params,
        }
    }

    /// Create CDKS PackingKeys from pre-computed condensed row_1s.
    pub fn init_cdks_from_keys(
        params: Params,
        pack_pub_params_row_1s: Vec<PolyMatrixNTT<'a>>,
    ) -> PackingKeys<'a> {
        PackingKeys {
            packing_type: PackingType::CDKS,
            gamma: 0,
            full_key: false,
            y_body: None,
            z_body: None,
            y_body_condensed: None,
            z_body_condensed: None,
            expanded: false,
            y_all_condensed: None,
            y_bar_all_condensed: None,
            params: Some(params),
            pack_pub_params_row_1s,
            fake_pack_pub_params: vec![],
        }
    }

    /// Create InspiRING PackingKeys from uploaded (condensed) keys.
    /// Used by the server to reconstruct keys from a stateless client request.
    pub fn init_from_uploaded(
        params: Params,
        gamma: usize,
        y_body: Option<PolyMatrixNTT<'a>>,
        z_body: Option<PolyMatrixNTT<'a>>,
        y_body_condensed: Option<PolyMatrixNTT<'a>>,
        z_body_condensed: Option<PolyMatrixNTT<'a>>,
    ) -> PackingKeys<'a> {
        PackingKeys {
            packing_type: PackingType::InspiRING,
            gamma,
            full_key: z_body.is_some(),
            y_body,
            z_body,
            y_body_condensed,
            z_body_condensed,
            expanded: false,
            y_all_condensed: None,
            y_bar_all_condensed: None,
            params: Some(params),
            pack_pub_params_row_1s: vec![],
            fake_pack_pub_params: vec![],
        }
    }

    pub fn get_gamma(&self) -> usize {
        if self.packing_type == PackingType::InspiRING {
            self.gamma
        } else {
            self.params.as_ref().unwrap().poly_len
        }
    }

    pub fn get_size_bytes(&self) -> usize {
        if self.packing_type == PackingType::InspiRING {
            if self.full_key {
                get_vec_pm_size_bytes(&vec![self.y_body.as_ref().unwrap().clone()])
                    + get_vec_pm_size_bytes(&vec![self.z_body.as_ref().unwrap().clone()])
            } else {
                get_vec_pm_size_bytes(&vec![self.y_body.as_ref().unwrap().clone()])
            }
        } else {
            get_vec_pm_size_bytes(&self.pack_pub_params_row_1s)
        }
    }

    pub fn expand(&mut self, packing_params: &PackParams<'a>) {
        assert_eq!(self.packing_type, PackingType::InspiRING);
        if !self.expanded {
            if self.full_key {
                let (x, y) = generate_rotations_double(packing_params, &self.y_body_condensed.as_ref().unwrap());
                (self.y_all_condensed, self.y_bar_all_condensed) = (Some(x), Some(y));
            } else {
                let x = generate_rotations(packing_params, &self.y_body_condensed.as_ref().unwrap());
                self.y_all_condensed = Some(x);
            }
            self.expanded = true;
        }
    }
}

// --- Key Switching Key Body ---

pub fn generate_ksk_body<'a>(
    params: &'a Params,
    sk_reg: &PolyMatrixRaw<'a>,
    gen: usize,
    mask: &PolyMatrixNTT<'a>,
    rng: &mut ChaCha20Rng,
) -> PolyMatrixNTT<'a> {
    let tau_sk_reg = automorph_alloc(&sk_reg, gen);
    let minus_s_times_mask = &sk_reg.ntt() * &(-mask);
    let error_poly = PolyMatrixRaw::noise(
        &params,
        1,
        params.t_exp_left,
        &DiscreteGaussian::init(params.noise_width),
        rng,
    );
    let g_exp_ntt = build_gadget(&params, 1, params.t_exp_left).ntt();
    let ksk = &tau_sk_reg.ntt() * &g_exp_ntt;
    let body = &minus_s_times_mask + &error_poly.ntt();
    let result = &body + &ksk;
    result
}

// --- Core InspiRING Packing Functions ---

fn precompute_aggregates_direct<'a>(
    packing_params: &PackParams<'a>, a_ct_tilde: &[PolyMatrixNTT<'a>],
) -> (Vec<PolyMatrixNTT<'a>>, Vec<PolyMatrixNTT<'a>>) {
    let params = packing_params.params;
    let tables = &packing_params.tables;
    let gen_pows = &packing_params.gen_pows;
    let monomial_ntts = &packing_params.monomial_ntts;
    let neg_monomial_ntts = &packing_params.neg_monomial_ntts;
    let mod_inv_poly = &packing_params.mod_inv_poly;

    let num_to_pack = packing_params.num_to_pack;
    assert_eq!(num_to_pack, params.poly_len);
    let num_to_pack_half = num_to_pack >> 1;

    let non_zeros = a_ct_tilde.len();

    let mut r_all = Vec::with_capacity(num_to_pack_half);
    let mut r_bar_all = Vec::with_capacity(num_to_pack_half);
    for i in 0..num_to_pack_half {
        let mut r_pow_i = PolyMatrixNTT::zero(&params, 1, 1);
        let mut r_bar_pow_i = PolyMatrixNTT::zero(&params, 1, 1);
        for j in 0..non_zeros {
            let pol2 = a_ct_tilde[j].get_poly(0, 0);

            let res_poly = r_pow_i.get_poly_mut(0, 0);
            // For each component i, we will apply τ_{g^i} at the end.
            // To place LWE j at slot X^j after rotation, we pre-multiply by X^{j · g^{-i}}
            // so that τ_{g^i}(X^{j · g^{-i}}) = X^j.
            // This is an optimization to avoid computing d many MLWEs, instead computing it all at once (only d-1 + 1 automorphs)
            let index = (j * packing_params.gen_pows[(params.poly_len - i) % params.poly_len])
                % (2 * params.poly_len);
            let pol1 = if index < params.poly_len {
                monomial_ntts[index % params.poly_len].get_poly(0, 0)
            } else {
                neg_monomial_ntts[index % params.poly_len].get_poly(0, 0)
            };
            multiply_add_poly(&params, res_poly, pol1, pol2);

            let res_poly = r_bar_pow_i.get_poly_mut(0, 0);
            let index = (2 * params.poly_len
                - ((j * packing_params.gen_pows[(params.poly_len - i) % params.poly_len])
                    % (2 * params.poly_len)))
                % (2 * params.poly_len);
            let pol1 = if index < params.poly_len {
                monomial_ntts[index % params.poly_len].get_poly(0, 0)
            } else {
                neg_monomial_ntts[index % params.poly_len].get_poly(0, 0)
            };
            multiply_add_poly(&params, res_poly, pol1, pol2);
            let reduction_steps = if params.crt_count == 1 { 1 } else { 1 << (64 - 2 * params.q2_bits - 1) };
            if (j + 1) % reduction_steps == 0 {
                fast_reduce(&mut r_pow_i);
                fast_reduce(&mut r_bar_pow_i);
            }
        }
        fast_reduce(&mut r_pow_i);
        fast_reduce(&mut r_bar_pow_i);

        let mut r_pow_i_mul = PolyMatrixNTT::zero(&params, 1, 1);
        let mut r_bar_pow_i_mul = PolyMatrixNTT::zero(&params, 1, 1);
        scalar_multiply(&mut r_pow_i_mul, &mod_inv_poly, &r_pow_i);
        scalar_multiply(&mut r_bar_pow_i_mul, &mod_inv_poly, &r_bar_pow_i);

        let mut r_pow_i_mul_rotated = PolyMatrixNTT::zero(&params, 1, 1);
        let mut r_bar_pow_i_mul_rotated = PolyMatrixNTT::zero(&params, 1, 1);
        apply_automorph_ntt(&params, &tables, &r_pow_i_mul, &mut r_pow_i_mul_rotated, gen_pows[i]);
        apply_automorph_ntt(
            &params,
            &tables,
            &r_bar_pow_i_mul,
            &mut r_bar_pow_i_mul_rotated,
            2 * params.poly_len - gen_pows[i],
        );

        r_all.push(r_pow_i_mul_rotated);
        r_bar_all.push(r_bar_pow_i_mul_rotated);
    }

    (r_all,r_bar_all)
}

fn precompute_aggregates_ntt<'a>(
    packing_params: &PackParams<'a>, a_ct_tilde: &[PolyMatrixNTT<'a>],
) -> (Vec<PolyMatrixNTT<'a>>, Vec<PolyMatrixNTT<'a>>) {
    let params=packing_params.params; let d=params.poly_len;
    assert_eq!(params.crt_count,1);
    assert_eq!(packing_params.num_to_pack,d);
    assert!(a_ct_tilde.len()<=d);
    let mut evaluations=vec![0u64;d*d];
    let mut row=PolyMatrixRaw::zero(params,1,1);
    for v in 0..d {
        row.as_mut_slice().fill(0);
        for (j,a) in a_ct_tilde.iter().enumerate() {
            assert_eq!((a.rows,a.cols),(1,1));
            row.get_poly_mut(0,0)[j]=a.get_poly(0,0)[v]%params.modulus;
        }
        let transformed=row.ntt();
        evaluations[v*d..(v+1)*d].copy_from_slice(transformed.get_poly(0,0));
    }
    let gather=|e:usize| {
        let table=&packing_params.tables[(e-1)/2];
        let mut unscaled=PolyMatrixNTT::zero(params,1,1);
        for slot in 0..d {unscaled.get_poly_mut(0,0)[slot]=evaluations[table[slot]*d+slot]%params.modulus;}
        let mut result=PolyMatrixNTT::zero(params,1,1);
        scalar_multiply(&mut result,&packing_params.mod_inv_poly,&unscaled);
        result
    };
    let mut r=Vec::with_capacity(d/2);let mut bar=Vec::with_capacity(d/2);
    for i in 0..d/2 {
        let e=packing_params.gen_pows[i];r.push(gather(e));bar.push(gather(2*d-e));
    }
    (r,bar)
}

pub fn full_packing_with_preprocessing_offline<'a>(
    packing_params: &PackParams<'a>,w_all: &PolyMatrixNTT<'a>,w_bar_all: &PolyMatrixNTT<'a>,
    v_mask: &PolyMatrixNTT<'a>,a_ct_tilde: &Vec<PolyMatrixNTT<'a>>,
) -> PrecompInsPIR<'a> {
    let fast=packing_params.params.crt_count==1;
    full_packing_offline_mode(packing_params,w_all,w_bar_all,v_mask,a_ct_tilde,fast)
}

fn full_packing_offline_mode<'a>(
    packing_params: &PackParams<'a>,w_all: &PolyMatrixNTT<'a>,w_bar_all: &PolyMatrixNTT<'a>,
    v_mask: &PolyMatrixNTT<'a>,a_ct_tilde: &[PolyMatrixNTT<'a>],fast:bool,
) -> PrecompInsPIR<'a> {
    let params=packing_params.params;
    let num_to_pack_half=packing_params.num_to_pack/2;
    let (mut r_all,mut r_bar_all)=if fast {precompute_aggregates_ntt(packing_params,a_ct_tilde)}
        else {precompute_aggregates_direct(packing_params,a_ct_tilde)};
    let mut bold_t_g = PolyMatrixNTT::zero(&params, num_to_pack_half - 1, params.t_exp_left);
    let mut bold_t_bar_g = PolyMatrixNTT::zero(&params, num_to_pack_half - 1, params.t_exp_left);

    // walk backwards
    for i in (0..num_to_pack_half - 1).rev() {
        let gadget = gadget_invert_transposed_alloc(&r_all[i + 1].raw(), params.t_exp_left).ntt();
        bold_t_g.copy_into(&gadget, i, 0);

        let mut res = PolyMatrixNTT::zero(&params, 1, 1);
        let poly_res = res.get_poly_mut(0, 0);
        for k in 0..params.t_exp_left {
            let poly1 = w_all.get_poly(i, k);
            let poly2 = bold_t_g.get_poly(i, k);
            multiply_add_poly(params, poly_res, poly1, poly2);
        }
        fast_add_into(&mut r_all[i], &res);

        let gadget = gadget_invert_transposed_alloc(&r_bar_all[i + 1].raw(), params.t_exp_left).ntt();
        bold_t_bar_g.copy_into(&gadget, i, 0);

        let mut res = PolyMatrixNTT::zero(&params, 1, 1);
        let poly_res = res.get_poly_mut(0, 0);
        for k in 0..params.t_exp_left {
            let poly1 = w_bar_all.get_poly(i, k);
            let poly2 = bold_t_bar_g.get_poly(i, k);
            multiply_add_poly(params, poly_res, poly1, poly2);
        }
        fast_add_into(&mut r_bar_all[i], &res);

    }

    // Debug: dump bold_t first values
    let bold_t_hat = gadget_invert_transposed_alloc(&r_bar_all[0].raw(), params.t_exp_left).ntt();

    let mut res = PolyMatrixNTT::zero(&params, 1, 1);
    let poly_res = res.get_poly_mut(0, 0);
    for k in 0..params.t_exp_left {
        let poly1 = v_mask.get_poly(0, k);
        let poly2 = bold_t_hat.get_poly(0, k);
        multiply_add_poly(params, poly_res, poly1, poly2);
    }
    fast_add_into(&mut r_all[0], &res);

    let result=PrecompInsPIR {
        a_hat: r_all[0].raw(),
        bold_t_condensed: condense_matrix(&params, &bold_t_g), // first half NTTs
        bold_t_bar_condensed: condense_matrix(&params, &bold_t_bar_g), // second half NTTs
        bold_t_hat_condensed: condense_matrix(&params, &bold_t_hat), // final NTT
    };
    result
}

pub fn full_packing_with_preprocessing_offline_without_rotations<'a>(
    packing_params: &PackParams<'a>,
    w_all: &PolyMatrixNTT<'a>,
    w_bar_all: &PolyMatrixNTT<'a>,
    v_mask: &PolyMatrixNTT<'a>,
    a_ct_tilde: &Vec<PolyMatrixNTT<'a>>,
) -> PrecompInsPIR<'a> {
    let params = packing_params.params;
    let tables = &packing_params.tables;
    let gen_pows = &packing_params.gen_pows;
    let monomial_ntts = &packing_params.monomial_ntts;
    let neg_monomial_ntts = &packing_params.neg_monomial_ntts;
    let mod_inv_poly = &packing_params.mod_inv_poly;

    let num_to_pack = packing_params.num_to_pack;
    assert_eq!(num_to_pack, params.poly_len);
    let num_to_pack_half = num_to_pack >> 1;

    let non_zeros = a_ct_tilde.len();

    let mut r_all = Vec::new();
    let mut r_bar_all = Vec::new();
    for i in 0..num_to_pack_half {
        let mut r_pow_i = PolyMatrixNTT::zero(&params, 1, 1);
        let mut r_bar_pow_i = PolyMatrixNTT::zero(&params, 1, 1);
        for j in 0..non_zeros {
            let pol2 = a_ct_tilde[j].get_poly(0, 0);

            let res_poly = r_pow_i.get_poly_mut(0, 0);
            let index = (j * packing_params.gen_pows[(params.poly_len - i) % params.poly_len])
                % (2 * params.poly_len);
            let pol1 = if index < params.poly_len {
                monomial_ntts[index % params.poly_len].get_poly(0, 0)
            } else {
                neg_monomial_ntts[index % params.poly_len].get_poly(0, 0)
            };
            multiply_add_poly(&params, res_poly, pol1, pol2);

            let res_poly = r_bar_pow_i.get_poly_mut(0, 0);
            let index = (2 * params.poly_len
                - ((j * packing_params.gen_pows[(params.poly_len - i) % params.poly_len])
                    % (2 * params.poly_len)))
                % (2 * params.poly_len);
            let pol1 = if index < params.poly_len {
                monomial_ntts[index % params.poly_len].get_poly(0, 0)
            } else {
                neg_monomial_ntts[index % params.poly_len].get_poly(0, 0)
            };
            multiply_add_poly(&params, res_poly, pol1, pol2);
            let reduction_steps = if params.crt_count == 1 { 1 } else { 1 << (64 - 2 * params.q2_bits - 1) };
            if (j + 1) % reduction_steps == 0 {
                fast_reduce(&mut r_pow_i);
                fast_reduce(&mut r_bar_pow_i);
            }
        }
        fast_reduce(&mut r_pow_i);
        fast_reduce(&mut r_bar_pow_i);

        let mut r_pow_i_mul = PolyMatrixNTT::zero(&params, 1, 1);
        let mut r_bar_pow_i_mul = PolyMatrixNTT::zero(&params, 1, 1);
        scalar_multiply(&mut r_pow_i_mul, &mod_inv_poly, &r_pow_i);
        scalar_multiply(&mut r_bar_pow_i_mul, &mod_inv_poly, &r_bar_pow_i);

        let mut r_pow_i_mul_rotated = PolyMatrixNTT::zero(&params, 1, 1);
        let mut r_bar_pow_i_mul_rotated = PolyMatrixNTT::zero(&params, 1, 1);
        apply_automorph_ntt(&params, &tables, &r_pow_i_mul, &mut r_pow_i_mul_rotated, gen_pows[i]);
        apply_automorph_ntt(
            &params,
            &tables,
            &r_bar_pow_i_mul,
            &mut r_bar_pow_i_mul_rotated,
            2 * params.poly_len - gen_pows[i],
        );

        r_all.push(r_pow_i_mul_rotated.raw());
        r_bar_all.push(r_bar_pow_i_mul_rotated.raw());
    }

    let mut bold_t_g = PolyMatrixNTT::zero(&params, num_to_pack_half - 1, params.t_exp_left);
    let mut bold_t_bar_g = PolyMatrixNTT::zero(&params, num_to_pack_half - 1, params.t_exp_left);
    let mut bold_t_g_prime = PolyMatrixNTT::zero(&params, num_to_pack_half - 1, params.t_exp_left);
    let mut bold_t_bar_g_prime = PolyMatrixNTT::zero(&params, num_to_pack_half - 1, params.t_exp_left);

    for i in (0..num_to_pack_half - 1).rev() {
        let gadget = gadget_invert_transposed_alloc(&r_all[i + 1], params.t_exp_left).ntt();
        bold_t_g.copy_into(&gadget, i, 0);

        let mut rotated_gadget = PolyMatrixNTT::zero(&params, 1, params.t_exp_left);
        apply_automorph_ntt(&params, &packing_params.tables, &gadget, &mut rotated_gadget, gen_pows[(params.poly_len - i) % params.poly_len]);
        bold_t_g_prime.copy_into(&rotated_gadget, i, 0);

        let mut res = PolyMatrixNTT::zero(&params, 1, 1);
        let poly_res = res.get_poly_mut(0, 0);
        for k in 0..params.t_exp_left {
            let poly1 = w_all.get_poly(i, k);
            let poly2 = bold_t_g.get_poly(i, k);
            multiply_add_poly(params, poly_res, poly1, poly2);
        }
        fast_reduce(&mut res);
        r_all[i] = &r_all[i] + &res.raw();

        let gadget = gadget_invert_transposed_alloc(&r_bar_all[i + 1], params.t_exp_left).ntt();
        bold_t_bar_g.copy_into(&gadget, i, 0);

        let mut rotated_gadget = PolyMatrixNTT::zero(&params, 1, params.t_exp_left);
        apply_automorph_ntt(&params, &packing_params.tables, &gadget, &mut rotated_gadget, 2*params.poly_len - gen_pows[(params.poly_len - i) % params.poly_len]);
        bold_t_bar_g_prime.copy_into(&rotated_gadget, i, 0);

        let mut res = PolyMatrixNTT::zero(&params, 1, 1);
        let poly_res = res.get_poly_mut(0, 0);
        for k in 0..params.t_exp_left {
            let poly1 = w_bar_all.get_poly(i, k);
            let poly2 = bold_t_bar_g.get_poly(i, k);
            multiply_add_poly(params, poly_res, poly1, poly2);
        }
        fast_reduce(&mut res);
        r_bar_all[i] = &r_bar_all[i] + &res.raw();
    }

    let bold_t_hat = gadget_invert_transposed_alloc(&r_bar_all[0], params.t_exp_left).ntt();

    let mut res = PolyMatrixNTT::zero(&params, 1, 1);
    let poly_res = res.get_poly_mut(0, 0);
    for k in 0..params.t_exp_left {
        let poly1 = v_mask.get_poly(0, k);
        let poly2 = bold_t_hat.get_poly(0, k);
        multiply_add_poly(params, poly_res, poly1, poly2);
    }
    fast_reduce(&mut res);
    r_all[0] = &r_all[0] + &res.raw();

    PrecompInsPIR {
        a_hat: r_all[0].clone(),
        bold_t_condensed: condense_matrix(&params, &bold_t_g_prime),
        bold_t_bar_condensed: condense_matrix(&params, &bold_t_bar_g_prime),
        bold_t_hat_condensed: condense_matrix(&params, &bold_t_hat),
    }
}

pub fn full_packing_with_preprocessing_online<'a>(
    packing_params: &'a PackParams<'a>,
    precomp_inspiring: &PrecompInsPIR<'a>,
    b_poly: &PolyMatrixRaw<'a>,
    y_all_condensed: &PolyMatrixNTT<'a>,
    y_bar_all_condensed: &PolyMatrixNTT<'a>,
    z_body_condensed: &PolyMatrixNTT<'a>,
) -> PolyMatrixRaw<'a> {
    let params = packing_params.params;

    let a_hat = &precomp_inspiring.a_hat;
    let bold_t_condensed = &precomp_inspiring.bold_t_condensed;
    let bold_t_bar_condensed = &precomp_inspiring.bold_t_bar_condensed;
    let bold_t_hat_condensed = &precomp_inspiring.bold_t_hat_condensed;

    let now = Instant::now();

    let num_to_pack = params.poly_len;
    let num_to_pack_half = num_to_pack >> 1;

    let mut sum_poly = PolyMatrixNTT::zero(&params, 1, 1);
    let mut temp_poly = PolyMatrixNTT::zero(&params, 1, 1);

    let addition_capacity = if params.crt_count == 1 { 1 } else { 1 << (64 - 2 * params.q2_bits - 1) };
    let mut num_added = 0;

    for i in 0..num_to_pack_half - 1 {
        fast_multiply_no_reduce_in_range_generic(
            &params,
            &mut temp_poly,
            &y_all_condensed,
            &bold_t_condensed,
            i * params.t_exp_left,
            i * params.t_exp_left,
            params.t_exp_left,
        );
        fast_add_into_no_reduce(&mut sum_poly, &temp_poly);
        num_added += params.t_exp_left;

        fast_multiply_no_reduce_in_range_generic(
            &params,
            &mut temp_poly,
            &y_bar_all_condensed,
            &bold_t_bar_condensed,
            i * params.t_exp_left,
            i * params.t_exp_left,
            params.t_exp_left,
        );
        fast_add_into_no_reduce(&mut sum_poly, &temp_poly);
        num_added += params.t_exp_left;

        if (num_added >= addition_capacity) || (i == num_to_pack_half - 2) {
            fast_reduce(&mut sum_poly);
            num_added = 0;
        }
    }

    // Final term
    fast_multiply_no_reduce_in_range_generic(
        &params,
        &mut temp_poly,
        &z_body_condensed,
        &bold_t_hat_condensed,
        0,
        0,
        params.t_exp_left,
    );
    fast_add_into(&mut sum_poly, &temp_poly);

    let mut final_b_poly = PolyMatrixRaw::zero(&params, 1, 1);
    add_raw(&mut final_b_poly, &b_poly, &sum_poly.raw());

    let mut packed_raw = PolyMatrixRaw::zero(&params, 2, 1);
    packed_raw.copy_into(&a_hat, 0, 0);
    packed_raw.copy_into(&final_b_poly, 1, 0);

    debug!("Packing online took {} us", now.elapsed().as_micros());

    packed_raw
}

pub fn finish_packing_from_beta<'a>(
    params: &'a Params,
    precomp_inspiring: &PrecompInsPIR<'a>,
    b_poly: &PolyMatrixRaw<'a>,
    beta: &[u64],
    z_body_condensed: &PolyMatrixNTT<'a>,
) -> PolyMatrixRaw<'a> {
    let mut sum_poly = PolyMatrixNTT::zero(params, 1, 1);
    sum_poly.as_mut_slice()[..params.poly_len].copy_from_slice(&beta[..params.poly_len]);
    let mut temp_poly = PolyMatrixNTT::zero(params, 1, 1);
    fast_multiply_no_reduce_in_range_generic(
        params,
        &mut temp_poly,
        z_body_condensed,
        &precomp_inspiring.bold_t_hat_condensed,
        0,
        0,
        params.t_exp_left,
    );
    fast_add_into(&mut sum_poly, &temp_poly);

    let mut final_b_poly = PolyMatrixRaw::zero(params, 1, 1);
    add_raw(&mut final_b_poly, b_poly, &sum_poly.raw());

    let mut packed_raw = PolyMatrixRaw::zero(params, 2, 1);
    packed_raw.copy_into(&precomp_inspiring.a_hat, 0, 0);
    packed_raw.copy_into(&final_b_poly, 1, 0);
    packed_raw
}

pub fn full_packing_with_preprocessing_online_without_rotations<'a>(
    packing_params: &'a PackParams<'a>,
    precomp_inspiring: &PrecompInsPIR<'a>,
    b_poly: &PolyMatrixRaw<'a>,
    y_condensed: &PolyMatrixNTT<'a>,
    z_body_condensed: &PolyMatrixNTT<'a>,
) -> PolyMatrixRaw<'a> {
    let params = packing_params.params;

    let a_hat = &precomp_inspiring.a_hat;
    let bold_t_condensed = &precomp_inspiring.bold_t_condensed;
    let bold_t_bar_condensed = &precomp_inspiring.bold_t_bar_condensed;
    let bold_t_hat_condensed = &precomp_inspiring.bold_t_hat_condensed;

    let now = Instant::now();

    let num_to_pack = params.poly_len;
    let num_to_pack_half = num_to_pack >> 1;

    let mut sum_poly = PolyMatrixNTT::zero(&params, 1, 1);
    let mut temp_poly = PolyMatrixNTT::zero(&params, 1, 1);

    let addition_capacity = if params.crt_count == 1 { 1 } else { 1 << (64 - 2 * params.q2_bits - 1) };
    let mut num_added = 0;

    for i in 0..num_to_pack_half - 1 {
        // bold_t: multiply then automorph directly into sum_poly
        fast_multiply_no_reduce_in_range_generic(
            &params,
            &mut temp_poly,
            &y_condensed,
            &bold_t_condensed,
            0,
            i * params.t_exp_left,
            params.t_exp_left,
        );
        apply_automorph_ntt(&params, &packing_params.tables, &temp_poly, &mut sum_poly, packing_params.gen_pows[i]);
        num_added += params.t_exp_left;

        // bold_t_bar: multiply then automorph directly into sum_poly
        fast_multiply_no_reduce_in_range_generic(
            &params,
            &mut temp_poly,
            &y_condensed,
            &bold_t_bar_condensed,
            0,
            i * params.t_exp_left,
            params.t_exp_left,
        );
        apply_automorph_ntt(&params, &packing_params.tables, &temp_poly, &mut sum_poly, 2 * params.poly_len - packing_params.gen_pows[i]);
        num_added += params.t_exp_left;

        if (num_added >= addition_capacity) || (i == num_to_pack_half - 2) {
            fast_reduce(&mut sum_poly);
            num_added = 0;
        }
    }

    // Final term
    fast_multiply_no_reduce_in_range_generic(
        &params,
        &mut temp_poly,
        &z_body_condensed,
        &bold_t_hat_condensed,
        0,
        0,
        params.t_exp_left,
    );
    fast_add_into(&mut sum_poly, &temp_poly);

    let mut final_b_poly = PolyMatrixRaw::zero(&params, 1, 1);
    add_raw(&mut final_b_poly, &b_poly, &sum_poly.raw());

    let mut packed_raw = PolyMatrixRaw::zero(&params, 2, 1);
    packed_raw.copy_into(&a_hat, 0, 0);
    packed_raw.copy_into(&final_b_poly, 1, 0);

    debug!("Packing online took {} us", now.elapsed().as_micros());

    packed_raw
}

// --- Pack Many LWEs (InspiRING entry points) ---

pub fn pack_many_lwes_inspir<'a>(
    packing_params: &'a PackParams,
    precomp_inspir_vec: &Vec<PrecompInsPIR<'a>>,
    b_values: &[u64],
    packing_keys: &PackingKeys<'a>,
    gamma: usize,
) -> Vec<PolyMatrixRaw<'a>> {
    let num_rlwe_outputs = (b_values.len() as f64 / gamma as f64).ceil() as usize;

    let mut res = Vec::with_capacity(num_rlwe_outputs);
    let group_size = packing_params.params.poly_len / gamma;
    for i in 0..num_rlwe_outputs {
        let which_group = i / group_size;
        let within_group = i % group_size;

        let mut b_poly = PolyMatrixRaw::zero(&packing_params.params, 1, 1);

        for j in 0..gamma {
            let index = which_group * packing_params.params.poly_len + within_group * gamma + j;
            if index >= b_values.len() {
                continue;
            }
            b_poly.get_poly_mut(0, 0)[j] = b_values[index];
        }

        let packed = if gamma <= packing_params.params.poly_len / 2 {
            let y_all_condensed = packing_keys.y_all_condensed.as_ref().unwrap();
            full_packing_with_preprocessing_online(
                &packing_params,
                &precomp_inspir_vec[i],
                &b_poly,
                &y_all_condensed,
                // For half-packing we'd use packing_with_preprocessing_online,
                // but gamma=poly_len always so we use full_packing path
                packing_keys.y_bar_all_condensed.as_ref().unwrap(),
                packing_keys.z_body_condensed.as_ref().unwrap(),
            )
        } else {
            let y_all_condensed = packing_keys.y_all_condensed.as_ref().unwrap();
            let y_bar_all_condensed = packing_keys.y_bar_all_condensed.as_ref().unwrap();
            let z_body_condensed = packing_keys.z_body_condensed.as_ref().unwrap();

            full_packing_with_preprocessing_online(
                &packing_params,
                &precomp_inspir_vec[i],
                &b_poly,
                &y_all_condensed,
                &y_bar_all_condensed,
                &z_body_condensed,
            )
        };

        res.push(packed);
    }

    res
}

pub fn pack_many_lwes_inspir_without_rotations<'a>(
    packing_params: &'a PackParams,
    precomp_inspir_vec: &Vec<PrecompInsPIR<'a>>,
    b_values: &[u64],
    packing_keys: &PackingKeys<'a>,
    gamma: usize,
) -> Vec<PolyMatrixRaw<'a>> {
    let num_rlwe_outputs = (b_values.len() as f64 / gamma as f64).ceil() as usize;

    let mut res = Vec::with_capacity(num_rlwe_outputs);
    let group_size = packing_params.params.poly_len / gamma;
    for i in 0..num_rlwe_outputs {
        let which_group = i / group_size;
        let within_group = i % group_size;

        let mut b_poly = PolyMatrixRaw::zero(&packing_params.params, 1, 1);

        for j in 0..gamma {
            let index = which_group * packing_params.params.poly_len + within_group * gamma + j;
            if index >= b_values.len() {
                continue;
            }
            b_poly.get_poly_mut(0, 0)[j] = b_values[index];
        }

        let packed = {
            let y_condensed = packing_keys.y_body_condensed.as_ref().unwrap();
            let z_body_condensed = packing_keys.z_body_condensed.as_ref().unwrap();
            full_packing_with_preprocessing_online_without_rotations(
                &packing_params,
                &precomp_inspir_vec[i],
                &b_poly,
                &y_condensed,
                &z_body_condensed,
            )
        };

        res.push(packed);
    }

    res
}

/// Fully online InspiRING packing for a small number of CTs known only at online time.
/// Does offline precomp + online packing in one call (gamma = poly_len, full packing).
pub fn packing_fully_online_without_rotations<'a>(
    packing_params: &'a PackParams<'a>,
    offline_packing_keys: &OfflinePackingKeys<'a>,
    a_ct_tilde: &Vec<PolyMatrixNTT<'a>>,
    packing_keys: &PackingKeys<'a>,
    b_poly: &PolyMatrixRaw<'a>,
) -> PolyMatrixRaw<'a> {
    let w_all = offline_packing_keys.w_all.as_ref().unwrap();
    let w_bar_all = offline_packing_keys.w_bar_all.as_ref().unwrap();
    let v_mask = offline_packing_keys.v_mask.as_ref().unwrap();

    let precomp_inspiring = full_packing_with_preprocessing_offline_without_rotations(
        packing_params, w_all, w_bar_all, v_mask, a_ct_tilde,
    );
    let y_condensed = packing_keys.y_body_condensed.as_ref().unwrap();
    let z_body_condensed = packing_keys.z_body_condensed.as_ref().unwrap();
    full_packing_with_preprocessing_online_without_rotations(
        packing_params, &precomp_inspiring, b_poly, y_condensed, z_body_condensed,
    )
}

#[cfg(test)]
mod precomp_ntt_tests {
    use super::*;
    use rand::Rng;
    fn same(a:&PrecompInsPIR<'_>,b:&PrecompInsPIR<'_>) {
        assert_eq!(a.a_hat.as_slice(),b.a_hat.as_slice());
        assert_eq!(a.bold_t_condensed.as_slice(),b.bold_t_condensed.as_slice());
        assert_eq!(a.bold_t_bar_condensed.as_slice(),b.bold_t_bar_condensed.as_slice());
        assert_eq!(a.bold_t_hat_condensed.as_slice(),b.bold_t_hat_condensed.as_slice());
    }
    fn compare(params:&Params,case:usize,partial:bool) {
        let d=params.poly_len;let pp=PackParams::new(params,d);
        let mut rng=ChaCha20Rng::seed_from_u64(917+case as u64);
        let count=if partial {d-3}else{d};
        let mut inputs=Vec::new();
        for j in 0..count {
            let mut p=PolyMatrixRaw::zero(params,1,1);
            for k in 0..d {p.get_poly_mut(0,0)[k]=match case {
                0=>0,1=>params.modulus-1,2=>if (j==0 || j==count-1) && (k==0 || k==d-1) {1}else{0},
                _=>rng.gen::<u64>()%params.modulus,
            };}
            inputs.push(p.ntt());
        }
        let direct=precompute_aggregates_direct(&pp,&inputs);
        let fast=precompute_aggregates_ntt(&pp,&inputs);
        for (x,y) in direct.0.iter().chain(&direct.1).zip(fast.0.iter().chain(&fast.1)) {assert_eq!(x.as_slice(),y.as_slice());}
        let w=PolyMatrixRaw::random_rng(params,d/2-1,params.t_exp_left,&mut rng).ntt();
        let bar=PolyMatrixRaw::random_rng(params,d/2-1,params.t_exp_left,&mut rng).ntt();
        let v=PolyMatrixRaw::random_rng(params,1,params.t_exp_left,&mut rng).ntt();
        let a=full_packing_offline_mode(&pp,&w,&bar,&v,&inputs,false);
        let b=full_packing_offline_mode(&pp,&w,&bar,&v,&inputs,true);
        same(&a,&b);
    }
    #[test]
    fn aggregate_and_cascade_edges() {
        let params=Params::init(32,&[4294955009],0.5*(2.*std::f64::consts::PI).sqrt(),1,256,28,4,4,2,3,true,0,1,1,32768,0);
        for case in 0..4 {for partial in [false,true] {compare(&params,case,partial);}}
    }
    #[test]
    #[ignore]
    fn production_dimension_differential() {
        let params=crate::params::params_for_sandwichpir(32768,262144);
        compare(&params,3,false);
    }
}
