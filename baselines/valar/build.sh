#!/usr/bin/env bash
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
out=$(mkdir -p "$1" && cd "$1" && pwd)
if [ ! -d "$out/upstream/.git" ]; then
    git init -q "$out/upstream"
    git -C "$out/upstream" fetch -q --depth 1 https://github.com/valargroup/ipir-sp 91e1965b9470fee4cd4cf20cd454dc93089eb28a
    git -C "$out/upstream" checkout -q FETCH_HEAD
fi
mkdir -p "$out/adapter"
cp -r "$here/Cargo.toml" "$here/src" "$out/adapter/"
[ -f "$out/adapter/Cargo.lock" ] || cp "$out/upstream/Cargo.lock" "$out/adapter/Cargo.lock"
cd "$out/adapter"
rustup toolchain install --no-self-update 1.95.0 --profile minimal > /dev/null
RUSTFLAGS='-C target-cpu=native' cargo +1.95.0 build -q --release
cp target/release/valar-bench "$out/valar-bench"
