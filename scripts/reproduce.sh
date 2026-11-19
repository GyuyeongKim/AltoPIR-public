#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

OUT=${OUT:-results}
SIZES=${SIZES:-"1 4 8 16 32 64"}
SCHEMES=${SCHEMES:-"altopir reinspire valar inspire ypir"}
PROCS=${PROCS:-1}
NODE=${NODE:-$(numactl -H | awk '/^available:/ {print $2 - 1}')}
CPUS=${CPUS:-$(lscpu -p=CPU,CORE,NODE | awk -F, -v n="$NODE" '!/^#/ && $3 == n && !seen[$2]++ {printf "%s%s", s, $1; s = ","}')}
CLIENT_CPUS=${CLIENT_CPUS:-$(lscpu -p=CPU,NODE | awk -F, -v n="$NODE" '!/^#/ && $2 != n {printf "%s%s", s, $1; s = ","}')}
CLIENT_CPUS=${CLIENT_CPUS:-$CPUS}
BUILD=target/baselines
TUN=glibc.malloc.trim_threshold=2147483647:glibc.malloc.mmap_threshold=33554432
N=$(echo "$CPUS" | tr , '\n' | awk -F- '{n += NF == 2 ? $2 - $1 + 1 : 1} END {print n}')
FIRST=$(echo "$CPUS" | cut -d, -f1 | cut -d- -f1)
B=$((4 * N))

METHODS=()
grep -qw amx_int8 /proc/cpuinfo && METHODS+=(amx)
grep -qw avx512_vnni /proc/cpuinfo && METHODS+=(vnni)
[ ${#METHODS[@]} -gt 0 ] || { echo "this CPU has neither AMX nor AVX-512 VNNI" >&2; exit 1; }

cargo build --release
for s in $SCHEMES; do
    [ "$s" = altopir ] || [ -x "$BUILD/$s/$s-bench" ] || bash "baselines/$s/build.sh" "$BUILD/$s"
done
mkdir -p "$OUT"
echo "memory node $NODE, engine cores $CPUS, client cores $CLIENT_CPUS, methods ${METHODS[*]}, schemes $SCHEMES" | tee "$OUT/setup.txt"

FAILED=()
run() {
    local out=$1; shift
    if [ -s "$out" ]; then echo "keep $out"; return; fi
    echo "run  $out"
    if "$@" > "$out.tmp"; then mv "$out.tmp" "$out"; else rm -f "$out.tmp"; FAILED+=("$out"); fi
}

bench() {
    run "$1" numactl --preferred="$NODE" target/release/altopir bench --client-cpus "$CLIENT_CPUS" "${@:2}"
}

baseline() {
    local out=$1 tun=$2; shift 2
    for p in $(seq 1 "$PROCS"); do
        run "${out}_p$p.txt" env -i PATH="$PATH" LC_ALL=C ${tun:+GLIBC_TUNABLES=$tun} numactl --preferred="$NODE" "$@"
    done
}

altopir() {
    local g=$1 rows=$(($1 * 32768)) c1=1,64
    bench "$OUT/altopir_${g}gb.txt" --cpus "$CPUS" --rows $rows --batches 1,32,64,128,256 --methods "${METHODS[0]}"
    [ "$g" = 1 ] && c1=1,64,128
    bench "$OUT/altopir_${g}gb_1core.txt" --cpus "$FIRST" --setup-cpus "$CPUS" --rows $rows --batches $c1 --methods "${METHODS[0]}"
    if [ "$g" = 1 ]; then
        for m in "${METHODS[@]}"; do
            bench "$OUT/altopir_batch_$m.txt" --cpus "$CPUS" --rows $rows --batches 1,2,4,8,16,32,64,128,256 --methods "$m"
        done
    fi
}

reinspire() {
    local g=$1 t=2 tun=$TUN cells="1:1,$B:$N"
    case $g in 4) return;; 1) t=1; tun=;; esac
    baseline "$OUT/reinspire_${g}gb" "$tun" "$BUILD/reinspire/reinspire-bench" --rows $(($1 * 32768)) --q-bits 54 --t $t --cpus "$CPUS" --cells "$cells"
}

valar() {
    local g=$1 w=1 cells
    case $g in 4) return;; 1) w=4;; 8) w=8;; esac
    [ $((N % w)) = 0 ] || w=1
    if [ $w = 1 ]; then cells="1:$N:1x$N"; else cells="$B:$N:${w}x$((N / w))"; fi
    cells="1:1:1x1,$cells"
    baseline "$OUT/valar_${g}gb" "$TUN" "$BUILD/valar/valar-bench" --rows $(($1 * 32768)) --cpus "$CPUS" --cells "$cells"
}

inspire() {
    local g=$1 bin=$BUILD/inspire/inspire-bench rows=$(($1 * 32768))
    case $g in
        4) ;;
        1|8)
            baseline "$OUT/inspire_${g}gb" "$TUN" $bin --rows $rows --cpus "$CPUS" --cells "$B:$N"
            baseline "$OUT/inspire_${g}gb_1core" "glibc.malloc.arena_max=1:$TUN" $bin --rows $rows --cpus "$CPUS" --cells 1:1;;
        *) baseline "$OUT/inspire_${g}gb" "$TUN" $bin --rows $rows --cpus "$CPUS" --cells "1:1,$B:$N";;
    esac
}

ypir() {
    local g=$1 out
    for p in $(seq 1 "$PROCS"); do
        out=$OUT/ypir_${g}gb_p$p.json
        if [ -s "$out" ]; then echo "keep $out"; continue; fi
        echo "run  $out"
        if env -i PATH="$PATH" LC_ALL=C numactl --physcpubind="$FIRST" --membind="$NODE" "$BUILD/ypir/ypir-bench" $(($1 * 32768)) 262144 1 5 "$out.tmp" -i > /dev/null; then
            mv "$out.tmp" "$out"
        else
            rm -f "$out.tmp"; FAILED+=("$out")
        fi
    done
}

for s in $SCHEMES; do
    for g in $SIZES; do
        $s "$g"
    done
done

python3 scripts/plot.py "$OUT" "$N"
[ ${#FAILED[@]} = 0 ] || { printf 'failed: %s\n' "${FAILED[@]}" >&2; exit 1; }
