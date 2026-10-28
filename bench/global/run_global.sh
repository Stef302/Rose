#!/usr/bin/env bash
# Usage : bench/global/run_global.sh
# banc GRANDS ENTIERS (rose_ent au-dela de int64, donc GMP) : hors de la portee
# du banc englobant, qui reste sous 2^64 partout.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.."; pwd)"
ROSE="${ROSE:-$ROOT/moteur/cpp/rose}"
CC="${CC:-g++}"
RUSTC="${RUSTC:-}"
if [ -z "$RUSTC" ]; then
    RUSTC="$(command -v rustc || true)"
    [ -z "$RUSTC" ] && [ -x "$HOME/.cargo/bin/rustc" ] && RUSTC="$HOME/.cargo/bin/rustc"
fi
# Le jumeau C++ inclut « rose_runtime.h » : le compilateur doit connaitre le
# repertoire du runtime, sinon le build echoue et le banc tombe en silence.
ROSE_DIR="$(cd "$(dirname "$(readlink -f "$ROSE")")" && pwd)"
SRC_DIR=""
for c in "$ROSE_DIR/src" "$ROSE_DIR" "$ROOT/moteur/cpp/src"; do
    [ -f "$c/rose_runtime.h" ] && { SRC_DIR="$c"; break; }
done
[ -n "$SRC_DIR" ] || { echo "rose_runtime.h introuvable pres de $ROSE (ROSE_HOME ?)" >&2; exit 2; }
GMP="${ROSE_GMP:-/tmp/opencode/gmp-linux}"
GMP_FLAGS=""
if [ -f "$GMP/include/gmp.h" ]; then
    GMP_FLAGS="-I$GMP/include -I$GMP/include/aarch64-linux-gnu -L$GMP/lib"
fi

echo "── banc global ──"

echo "=== Rose ==="
cd "$HERE"
$ROSE bench_global.rose 2>&1 | tail -3

echo "=== C++ ==="
$CC -O3 -march=native -std=c++17 -I"$SRC_DIR" $GMP_FLAGS bench_global.cpp \
    "$SRC_DIR/rose_runtime.cpp" -o bench_global_cpp -lgmp -lgmpxx -lm -lpthread
./bench_global_cpp 2>&1 | tail -3

echo "=== Rust ==="
if [ -n "$RUSTC" ]; then
  $RUSTC -O bench_global.rs -o bench_global_rs 2>&1
  ./bench_global_rs 2>&1 | tail -3
else
  echo "rustc absent — skip"
fi

echo "=== Python ==="
python3 bench_global.py 2>&1 | tail -3