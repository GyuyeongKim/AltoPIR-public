#!/bin/sh
set -eu
cd "$(dirname "$0")/.."

BIN=target/release/altopir
ADDR=127.0.0.1:7000
ROWS=8192
METHOD=amx
grep -q amx_int8 /proc/cpuinfo || METHOD=vnni

[ -x "$BIN" ] || cargo build --release

LOG=$(mktemp)
"$BIN" server --rows "$ROWS" --listen "$ADDR" --method "$METHOD" > "$LOG" 2>&1 &
PID=$!
trap 'kill "$PID" 2>/dev/null || true; rm -f "$LOG"' EXIT

echo "Starting a server with $ROWS synthetic records of 32 KB ($METHOD)..."
until grep -q '"event":"ready"' "$LOG"; do
    kill -0 "$PID" 2>/dev/null || { cat "$LOG"; exit 1; }
    sleep 1
done
cat "$LOG"

echo
echo "Fetching three records:"
"$BIN" query --server "$ADDR" --rows "$ROWS" --index 0,17,8191 --check

echo
echo "Sending 200 queries per second for 5 seconds:"
"$BIN" load --server "$ADDR" --rows "$ROWS" --rate 200 --seconds 5 --conns 4 --verify --drain-s 2

"$BIN" stop "$ADDR"
wait "$PID" || true
tail -n 1 "$LOG"
