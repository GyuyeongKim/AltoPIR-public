use log::debug;
use serde_json::Value;

use spiral_rs::{arith::*, params::*};

use crate::lwe::LWEParams;

fn ext_params_from_json(json_str: &str) -> Params {
    let v: Value = serde_json::from_str(json_str).unwrap();

    let n = v["n"].as_u64().unwrap() as usize;
    let db_dim_1 = v["nu_1"].as_u64().unwrap() as usize;
    let db_dim_2 = v["nu_2"].as_u64().unwrap() as usize;
    let instances = v["instances"].as_u64().unwrap_or(1) as usize;
    let p = v["p"].as_u64().unwrap();
    let q2_bits = u64::max(v["q2_bits"].as_u64().unwrap(), MIN_Q2_BITS);
    let t_gsw = v["t_gsw"].as_u64().unwrap() as usize;
    let t_conv = v["t_conv"].as_u64().unwrap() as usize;
    let t_exp_left = v["t_exp_left"].as_u64().unwrap() as usize;
    let t_exp_right = v["t_exp_right"].as_u64().unwrap() as usize;
    let do_expansion = v.get("direct_upload").is_none();

    let mut db_item_size = v["db_item_size"].as_u64().unwrap_or(0) as usize;
    if db_item_size == 0 {
        db_item_size = instances * n * n;
        db_item_size = db_item_size * 2048 * log2_ceil(p) as usize / 8;
    }

    let version = v["version"].as_u64().unwrap_or(0) as usize;

    let poly_len = v["poly_len"].as_u64().unwrap_or(2048) as usize;
    let moduli = v["moduli"]
        .as_array()
        .map(|x| {
            x.as_slice()
                .iter()
                .map(|y| {
                    y.as_u64()
                        .unwrap_or_else(|| y.as_str().unwrap().parse().unwrap())
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or(vec![4294955009u64]);
    let noise_width = v["noise_width"].as_f64().unwrap_or(6.4);

    Params::init(
        poly_len,
        &moduli,
        noise_width,
        n,
        p,
        q2_bits,
        t_conv,
        t_exp_left,
        t_exp_right,
        t_gsw,
        do_expansion,
        db_dim_1,
        db_dim_2,
        instances,
        db_item_size,
        version,
    )
}

pub trait GetQPrime {
    fn get_q_prime_1(&self) -> u64;
    fn get_q_prime_2(&self) -> u64;
}

impl GetQPrime for Params {
    fn get_q_prime_1(&self) -> u64 {
        // SandwichPIR: crt_count=1, q22 = 2^10
        1 << 10
    }

    fn get_q_prime_2(&self) -> u64 {
        // SandwichPIR: crt_count=1, q21 = 2^18
        1 << 18
    }
}

impl GetQPrime for LWEParams {
    fn get_q_prime_1(&self) -> u64 {
        u64::MAX
    }

    fn get_q_prime_2(&self) -> u64 {
        if self.q2_bits == (self.modulus as f64).log2().ceil() as usize {
            self.modulus
        } else {
            Q2_VALUES[self.q2_bits as usize]
        }
    }
}

/// SandwichPIR parameters: single NTT prime Q=4294955009, p=256, d=2048,
/// Xs=Xe=D(0.5), t=4, z=256, q21=2^18, q22=2^10.
/// InspiRING only. 192-bit security, log2(delta)=-105.
pub fn params_for_sandwichpir(num_items: usize, item_size_bits: usize) -> Params {
    let db_rows = num_items;
    let modulus_width = 8; // p = 256 = 2^8
    let db_cols = (item_size_bits as f64 / (2048.0 * modulus_width as f64)).ceil() as usize;

    debug!("db_rows: {}, db_cols: {}", db_rows, db_cols);

    let nu_1 = (db_rows.next_power_of_two().trailing_zeros() as usize)
        .checked_sub(11)
        .unwrap_or(0);
    debug!("chose nu_1: {}", nu_1);

    // D(0.5): noise_width = 0.5 * sqrt(2*pi)
    let noise_width = 0.5 * (2.0 * std::f64::consts::PI).sqrt();

    let mut params = ext_params_from_json(&format!(
        r#"{{
            "n": 1, "nu_1": {nu_1}, "nu_2": 1, "p": 256, "q2_bits": 28,
            "t_gsw": 3, "t_conv": 4, "t_exp_left": 4, "t_exp_right": 2,
            "instances": 1, "db_item_size": 0,
            "moduli": ["4294955009"], "noise_width": {noise_width}
        }}"#
    ));
    params.instances = db_cols;
    params
}

/// Server-side mode flag. SimplePIR uses a single matmul round; DoublePIR uses two.
/// SandwichPIR always sets `is_simplepir = true`.
#[derive(Debug, Clone, Default)]
pub struct YPIRParams {
    pub is_simplepir: bool,
}
