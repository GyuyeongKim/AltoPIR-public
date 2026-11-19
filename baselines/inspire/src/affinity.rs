use std::collections::BTreeSet;

extern "C" {
    fn sched_setaffinity(pid: i32, size: usize, mask: *const u64) -> i32;
    fn syscall(number: i64, ...) -> i64;
}

const SYS_MIGRATE_PAGES: i64 = 256;

fn mask(cpus: &[usize]) -> [u64; 16] {
    let mut m = [0u64; 16];
    for &c in cpus {
        m[c / 64] |= 1 << (c % 64);
    }
    m
}

fn tids() -> BTreeSet<i32> {
    std::fs::read_dir("/proc/self/task").unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().parse().unwrap())
        .collect()
}

pub fn parse(s: &str) -> Vec<usize> {
    let mut v = Vec::new();
    for part in s.trim().split(',').filter(|p| !p.is_empty()) {
        match part.split_once('-') {
            Some((a, b)) => v.extend(a.parse::<usize>().unwrap()..=b.parse::<usize>().unwrap()),
            None => v.push(part.parse().unwrap()),
        }
    }
    v
}

pub fn restrict(cpus: &[usize]) {
    let want = mask(cpus);
    let (mut previous, mut stable) = (BTreeSet::new(), 0);
    while stable < 2 {
        let before = tids();
        for &t in &before {
            unsafe { sched_setaffinity(t, 128, want.as_ptr()) };
        }
        let after = tids();
        stable = if before == after && before == previous { stable + 1 } else { 0 };
        previous = after;
    }
}

pub fn pin(cpu: usize) {
    let want = mask(&[cpu]);
    assert_eq!(unsafe { sched_setaffinity(0, 128, want.as_ptr()) }, 0, "cannot run on CPU {cpu}");
}

fn node(cpu: usize) -> Option<usize> {
    std::fs::read_dir(format!("/sys/devices/system/cpu/cpu{cpu}")).ok()?
        .filter_map(|e| e.ok()?.file_name().to_str()?.strip_prefix("node")?.parse().ok())
        .next()
}

pub fn move_memory(cpu: usize) {
    let (Some(target), Ok(online)) = (node(cpu), std::fs::read_to_string("/sys/devices/system/node/online")) else { return };
    let nodes = parse(&online);
    let max = nodes.iter().copied().max().unwrap_or(0).max(target) + 1;
    let (mut from, mut to) = (vec![0u64; (max + 63) / 64], vec![0u64; (max + 63) / 64]);
    for &n in nodes.iter().filter(|&&n| n != target) {
        from[n / 64] |= 1 << (n % 64);
    }
    to[target / 64] |= 1 << (target % 64);
    if from.iter().any(|&w| w != 0) {
        unsafe { syscall(SYS_MIGRATE_PAGES, 0i64, (max + 1) as u64, from.as_ptr(), to.as_ptr()) };
    }
}
