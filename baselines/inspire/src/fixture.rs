pub const RECORD_BYTES: usize = 32768;
const SEED: u64 = 0x416c746f504952;

fn mix(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e3779b97f4a7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}

pub fn record(row: usize) -> Vec<u8> {
    let mut state = SEED ^ 0xd1b54a32d192ed03u64.wrapping_mul(row as u64 + 1);
    let mut out = Vec::with_capacity(RECORD_BYTES);
    for _ in 0..RECORD_BYTES / 2 {
        out.extend_from_slice(&((mix(&mut state) % 65535) as u16).to_le_bytes());
    }
    out
}

pub fn indices(trial: u64, batch: usize, rows: usize) -> Vec<usize> {
    let mut state = SEED ^ 0x72657175657374 ^ trial;
    (0..batch).map(|_| mix(&mut state) as usize & (rows - 1)).collect()
}

pub fn database(rows: usize) -> Vec<u8> {
    let mut out = vec![0u8; rows * RECORD_BYTES];
    let chunk_rows = (rows + 63) / 64;
    std::thread::scope(|scope| {
        for (group, chunk) in out.chunks_mut(chunk_rows * RECORD_BYTES).enumerate() {
            scope.spawn(move || {
                for (r, dst) in chunk.chunks_exact_mut(RECORD_BYTES).enumerate() {
                    dst.copy_from_slice(&record(group * chunk_rows + r));
                }
            });
        }
    });
    out
}
