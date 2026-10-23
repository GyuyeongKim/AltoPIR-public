#!/usr/bin/env bash
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
out=$(mkdir -p "$1" && cd "$1" && pwd)
src=$out/InsPIRe
if [ ! -d "$src/.git" ]; then
    git init -q "$src"
    git -C "$src" fetch -q --depth 1 https://github.com/google/InsPIRe acab7853e8fe11a70b5fa40e17487f7429e82810
    git -C "$src" checkout -q FETCH_HEAD
fi
cd "$src"
git checkout -q -- crypto/matrix.cc
sed -i 's/&data\[i \* condensed_cols + b \* N\]/\&data[static_cast<size_t>(i) * condensed_cols + b * N]/' crypto/matrix.cc
[ "$(grep -c 'static_cast<size_t>(i) \* condensed_cols + b \* N' crypto/matrix.cc)" = 3 ]
mkdir -p bench
cp "$here/BUILD.bazel" "$here/main.cc" "$here/wire.h" "$here/fixture.h" "$here/pool.h" bench/
bazel=${BAZEL:-$(command -v bazel || true)}
if [ -z "$bazel" ]; then
    bazel=$out/bazel-9.3.0
    [ -x "$bazel" ] || { curl -fsSL -o "$bazel.tmp" https://github.com/bazelbuild/bazel/releases/download/9.3.0/bazel-9.3.0-linux-x86_64 && chmod +x "$bazel.tmp" && mv "$bazel.tmp" "$bazel"; }
fi
USE_BAZEL_VERSION=9.3.0 "$bazel" --batch build -c opt --copt=-march=skylake-avx512 //bench:reinspire_bench
cp bazel-bin/bench/reinspire_bench "$out/reinspire-bench"
