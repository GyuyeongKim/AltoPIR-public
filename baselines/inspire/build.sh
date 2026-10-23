#!/usr/bin/env bash
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
out=$(mkdir -p "$1" && cd "$1" && pwd)
src=$out/private-membership
if [ ! -d "$src/.git" ]; then
    git init -q "$src"
    git -C "$src" fetch -q --depth 1 https://github.com/google/private-membership 25b2fa3e5db40ef8bc16d7e4c43475032f792faf
    git -C "$src" checkout -q FETCH_HEAD
fi
cd "$src/research/InsPIRe"
mkdir -p src/bin/inspire_bench
cp "$here/src/bench.rs" src/bin/inspire_bench.rs
cp "$here/src/affinity.rs" "$here/src/fixture.rs" "$here/src/wire.rs" "$here/src/compact.rs" src/bin/inspire_bench/
python3 "$here/derive.py" src/bin/inspire.rs src/bin/inspire_bench
rustup toolchain install --no-self-update 1.95.0 --profile minimal > /dev/null
RUSTC_BOOTSTRAP=spiral_rs,inspire RUSTFLAGS='-C target-cpu=native' cargo +1.95.0 build -q --locked --release --bin inspire_bench
cp target/release/inspire_bench "$out/inspire-bench"
