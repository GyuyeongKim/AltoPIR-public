//! SandwichPIR: Hybrid RLWE->WordPIR with InspiRING packing.
//!
//! Parameters: W=2^32, Q=4294955009, d=2048, p=256, Xs=Xe=D(0.5),
//! t=4, z=256, q21=2^18, q22=2^10. InspiRING only. 192-bit security, log2(delta)=-105.

use log::debug;

use spiral_rs::params::*;
use spiral_rs::poly::{PolyMatrix, PolyMatrixRaw};

use crate::client::*;
use crate::measurement::*;
use crate::packing::{PackingKeys, PackingType};

pub const STATIC_PUBLIC_SEED: [u8; 32] = [0u8; 32];
pub const SEED_0: u8 = 0;
pub const SEED_1: u8 = 1;

pub const STATIC_SEED_2: [u8; 32] = [
    2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0,
];

pub const W_SEED: [u8; 32] = [
    7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
    7, 7,
];
pub const V_SEED: [u8; 32] = [
    8, 8, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
    7, 7,
];

pub trait Sample {
    fn sample() -> Self;
}

impl Sample for u8 {
    fn sample() -> Self {
        fastrand::u8(..)
    }
}

impl Sample for u16 {
    fn sample() -> Self {
        fastrand::u16(..)
    }
}

pub fn generate_packing_keys<'a>(
    packing: PackingType,
    params: &'a Params,
    sk_reg: &PolyMatrixRaw<'a>,
    offline_packing_params: Option<&crate::packing::PackParams<'a>>,
) -> (usize, PackingKeys<'a>) {
    match packing {
        PackingType::InspiRING => {
            let pp = offline_packing_params.unwrap();
            let packing_keys = PackingKeys::init_full(pp, sk_reg, W_SEED, V_SEED);
            let size = packing_keys.get_size_bytes();
            debug!("InspiRING packing key size: {} bytes", size);
            (size, packing_keys)
        }
        _ => {
            use rand::SeedableRng;
            use rand_chacha::ChaCha20Rng;
            use crate::packing::condense_matrix;
            let pack_pub_params = raw_generate_expansion_params(
                params,
                sk_reg,
                params.poly_len_log2,
                params.t_exp_left,
                &mut ChaCha20Rng::from_entropy(),
                &mut ChaCha20Rng::from_seed(STATIC_SEED_2),
            );
            let mut pack_pub_params_row_1s = pack_pub_params.to_vec();
            for i in 0..pack_pub_params.len() {
                pack_pub_params_row_1s[i] =
                    pack_pub_params[i].submatrix(1, 0, 1, pack_pub_params[i].cols);
                pack_pub_params_row_1s[i] = condense_matrix(params, &pack_pub_params_row_1s[i]);
            }
            let size = get_vec_pm_size_bytes(&pack_pub_params_row_1s);
            debug!("pub params size: {} bytes", size);
            let packing_keys =
                PackingKeys::init_cdks_from_keys(params.clone(), pack_pub_params_row_1s);
            (size, packing_keys)
        }
    }
}
