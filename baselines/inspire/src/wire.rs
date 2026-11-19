use spiral_rs::params::Params;
use inspire::params::GetQPrime;

pub fn request_len(p: &Params, rows: usize) -> usize {
    8 * (2 * p.t_exp_left * p.poly_len + rows + 2 * p.t_gsw * p.crt_count * p.poly_len)
}

pub fn response_part_len(p: &Params) -> usize {
    let a = (p.get_q_prime_2() as f64).log2().ceil() as usize;
    let b = (p.get_q_prime_1() as f64).log2().ceil() as usize;
    ((p.poly_len * (a + b) + 63) / 64) * 8
}

pub fn response_len(p: &Params) -> usize {
    p.instances * response_part_len(p)
}

pub fn validate_request(p: &Params, bytes: &[u8], rows: usize) -> Result<(), String> {
    if bytes.len() != request_len(p, rows) {
        return Err("request length".into());
    }
    let start = 2 * p.t_exp_left * p.poly_len * 8;
    for chunk in bytes[start..start + rows * 8].chunks_exact(8) {
        let v = u64::from_ne_bytes(chunk.try_into().unwrap());
        if v & 0xffffffff >= p.moduli[0] || v >> 32 >= p.moduli[1] {
            return Err("query residue out of range".into());
        }
    }
    Ok(())
}
