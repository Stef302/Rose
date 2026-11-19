#!/usr/bin/env bash
# RA 19/11 — toutes les comparaisons racines. A/A/A entrelacé, 9 échantillons, c6.
set -u
export PATH="$HOME/.cargo/bin:$PATH"
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
ROSE="$ROOT/rose/target/release/rose"
REF="/tmp/opencode/ra_agents/rust_ref/target/release/racines_ref"
PY="python3 $HERE/harness.py"
N=9
OUT="${1:-/tmp/opencode/ra_agents/bench_out}"
mkdir -p "$OUT"

run() { local tag="$1"; shift; echo "################ $tag ################"; $PY --label "$tag" --n "$N" "$@" --json "$OUT/$tag.json"; }

# --- Niveau LANGAGE : Rose (interpréteur/JIT) vs Rust (primitive directe) ---
run C1_i64_2M --a "$ROSE" "$HERE/racines_i64.rose" 2000000 --b "$REF" f64 2000000
run C2_big_20k --a "$ROSE" "$HERE/racines_big.rose" 20000 --b "$REF" big 20000
run C3_dec100_5k --a "$ROSE" "$HERE/racines_dec.rose" 5000 100 --b "$REF" decs 5000 100
run C4_dec1000_500 --a "$ROSE" "$HERE/racines_dec.rose" 500 1000 --b "$REF" decs 500 1000

# --- Niveau ALGORITHME : original vs candidat, même binaire Rust ---
run A1_i64_f64_vs_newton --a "$REF" f64 2000000 --b "$REF" inewton 2000000
run A2_dec100_orig_vs_newton2048 --a "$REF" dec 5000 100 --b "$REF" dec2 5000 100
run A3_dec1000_orig_vs_newton2048 --a "$REF" dec 500 1000 --b "$REF" dec2 500 1000
run A4_big_orig_vs_newton2048 --a "$REF" big 20000 --b "$REF" big2 20000
run A5_dec1000_str_orig_vs_newton2048 --a "$REF" decs 500 1000 --b "$REF" decs2 500 1000
