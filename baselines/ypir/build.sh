#!/usr/bin/env bash
set -euo pipefail
out=$(mkdir -p "$1" && cd "$1" && pwd)
if [ ! -d "$out/ypir/.git" ]; then
    git init -q "$out/ypir"
    git -C "$out/ypir" fetch -q --depth 1 https://github.com/menonsamir/ypir a73e550a469605c2fa23945a4d96c20fb2f4a839
    git -C "$out/ypir" checkout -q FETCH_HEAD
fi
cd "$out/ypir"
rustup toolchain install --no-self-update nightly-2024-02-07 --profile minimal > /dev/null
cargo build -q --release --bin run
cp target/release/run "$out/ypir-bench"
