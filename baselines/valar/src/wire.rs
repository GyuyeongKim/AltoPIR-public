use inspiring::RlweParams;
use ipir_sp::serialize::serialized_packing_keys_len;

pub const SETUP_LEN: usize = 64;
pub const C1_LEN: usize = 114688;
pub const BODY_LEN: usize = 40960;
pub const RESPONSE_LEN: usize = C1_LEN + BODY_LEN;

pub fn setup(rows: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(SETUP_LEN);
    out.extend_from_slice(b"VALARV01");
    out.extend_from_slice(&1u64.to_le_bytes());
    out.extend_from_slice(&(rows as u64).to_le_bytes());
    out.extend_from_slice(&32768u64.to_le_bytes());
    out.extend_from_slice(&[7; 32]);
    out
}

pub fn parse_setup(bytes: &[u8], rows: usize) -> Result<[u8; 32], String> {
    if bytes != setup(rows) {
        return Err("setup frame".into());
    }
    Ok(bytes[32..64].try_into().unwrap())
}

pub fn request_len(p: &RlweParams, rows: usize) -> usize {
    serialized_packing_keys_len(p) + rows * 49 / 8
}

pub fn split_request<'a>(p: &RlweParams, rows: usize, bytes: &'a [u8]) -> Result<(&'a [u8], &'a [u8]), String> {
    if bytes.len() != request_len(p, rows) {
        return Err("request length".into());
    }
    Ok(bytes.split_at(serialized_packing_keys_len(p)))
}

pub fn split_response<'a>(p: &RlweParams, bytes: &'a [u8]) -> Result<(&'a [u8], &'a [u8]), String> {
    if bytes.len() != RESPONSE_LEN {
        return Err("response length".into());
    }
    let (c1, body) = bytes.split_at(C1_LEN);
    for v in c1.chunks_exact(7) {
        let mut n = [0u8; 8];
        n[..7].copy_from_slice(v);
        if u64::from_le_bytes(n) >= p.q {
            return Err("noncanonical c1".into());
        }
    }
    Ok((c1, body))
}
