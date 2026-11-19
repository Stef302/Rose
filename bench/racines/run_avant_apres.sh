#!/usr/bin/env bash
# RA 19/11 — AVANT (num-bigint .sqrt partout) vs APRES (hybride ≤2048 bits
# .sqrt / >2048 bits Newton échelonné 2048). 9 échantillons entrelacés, c6.
set -u
export PATH="$HOME/.cargo/bin:$PATH"
HERE="$(cd "$(dirname "$0")" && pwd)"
AVANT=/tmp/opencode/ra_agents/rose_AVANT
APRES=/tmp/opencode/ra_agents/rose_APRES
PY="python3 $HERE/harness.py"
OUT="${1:-/tmp/opencode/ra_agents/bench_out}"
mkdir -p "$OUT"
run() { local tag="$1"; shift; echo "################ $tag ################"; $PY --label "$tag" --n 9 "$@" --json "$OUT/$tag.json"; }

run B1_i64_2M --a "$AVANT" "$HERE/racines_i64.rose" 2000000 --b "$APRES" "$HERE/racines_i64.rose" 2000000
run B2_big_20k --a "$AVANT" "$HERE/racines_big.rose" 20000 --b "$APRES" "$HERE/racines_big.rose" 20000
run B3_dec100_5k --a "$AVANT" "$HERE/racines_dec.rose" 5000 100 --b "$APRES" "$HERE/racines_dec.rose" 5000 100
run B4_dec1000_500 --a "$AVANT" "$HERE/racines_dec.rose" 500 1000 --b "$APRES" "$HERE/racines_dec.rose" 500 1000
run B5_dec200_1000 --a "$AVANT" "$HERE/racines_dec.rose" 1000 200 --b "$APRES" "$HERE/racines_dec.rose" 1000 200
