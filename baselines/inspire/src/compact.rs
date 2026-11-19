use spiral_rs::params::Params;
use crate::wire;

fn shape(p: &Params, rows: usize) -> (usize, usize) {
    assert_eq!((p.crt_count, p.poly_len, p.t_exp_left, p.t_gsw), (2, 2048, 3, 3));
    assert_eq!((p.moduli[0], p.moduli[1]), (268369921, 249561089));
    (2 * p.t_exp_left * p.poly_len + rows, 2 * p.t_gsw * p.crt_count * p.poly_len)
}

pub fn request_len(p: &Params, rows: usize) -> usize {
    let (pairs, limbs) = shape(p, rows);
    7 * pairs + 7 * (limbs / 2)
}

fn word(bytes: &[u8], i: usize) -> u64 {
    u64::from_ne_bytes(bytes[8 * i..8 * i + 8].try_into().unwrap())
}

fn put56(out: &mut Vec<u8>, low: u64, high: u64) {
    assert!(low < 1 << 28 && high < 1 << 28);
    out.extend_from_slice(&(low | (high << 28)).to_le_bytes()[..7]);
}

fn read56(bytes: &[u8], off: usize) -> (u64, u64) {
    let mut v = [0u8; 8];
    v[..7].copy_from_slice(&bytes[off..off + 7]);
    let v = u64::from_le_bytes(v);
    (v & ((1 << 28) - 1), v >> 28)
}

fn valid(p: &Params, low: u64, high: u64) -> Result<(), String> {
    if low >= p.moduli[0] || high >= p.moduli[1] { Err("noncanonical CRT pair".into()) } else { Ok(()) }
}

pub fn encode(p: &Params, native: &[u8], rows: usize) -> Result<Vec<u8>, String> {
    if native.len() != wire::request_len(p, rows) {
        return Err("native length".into());
    }
    let (pairs, limbs) = shape(p, rows);
    let mut out = Vec::with_capacity(request_len(p, rows));
    for i in 0..pairs {
        let v = word(native, i);
        let (low, high) = (v & 0xffffffff, v >> 32);
        valid(p, low, high)?;
        put56(&mut out, low, high);
    }
    for i in (0..limbs).step_by(2) {
        let modulus = p.moduli[(i / p.poly_len) % 2];
        let (low, high) = (word(native, pairs + i), word(native, pairs + i + 1));
        if low >= modulus || high >= modulus {
            return Err("noncanonical RGSW residue".into());
        }
        put56(&mut out, low, high);
    }
    Ok(out)
}

pub fn decode(p: &Params, bytes: &[u8], rows: usize) -> Result<Vec<u8>, String> {
    if bytes.len() != request_len(p, rows) {
        return Err("compact length".into());
    }
    let (pairs, limbs) = shape(p, rows);
    let mut out = Vec::with_capacity(wire::request_len(p, rows));
    for i in 0..pairs {
        let (low, high) = read56(bytes, 7 * i);
        valid(p, low, high)?;
        out.extend_from_slice(&(low | (high << 32)).to_ne_bytes());
    }
    for i in (0..limbs).step_by(2) {
        let (low, high) = read56(bytes, 7 * pairs + 7 * (i / 2));
        let modulus = p.moduli[(i / p.poly_len) % 2];
        if low >= modulus || high >= modulus {
            return Err("noncanonical RGSW residue".into());
        }
        out.extend_from_slice(&low.to_ne_bytes());
        out.extend_from_slice(&high.to_ne_bytes());
    }
    assert_eq!(out.len(), wire::request_len(p, rows));
    Ok(out)
}
