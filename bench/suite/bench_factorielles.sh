#!/bin/bash
# Bench COMPARATIF : factorielles étendues, Rose contre les autres langages.
#
# Compare Rose (avec et sans backend GMP) à :
#   - GMP/C        : la référence absolue (Toom-Cook + FFT)
#   - Python       : math.factorial (GMP-like via son propre algorithme)
#   - C++ GMP      : gmpxx, même moteur que C mais avec les classes
#
# ⚠ Chaque implémentation calcule EXACTEMENT les mêmes opérations, et on
# vérifie le NOMBRE DE CHIFFRES (pas seulement le temps) : un bench qui ne
# vérifie rien ne prouve rien.
#
# Usage : ./bench/suite/bench_factorielles.sh [N]

set -u
cd "$(dirname "$0")/../.."
N="${1:-100000}"
TMP="${TMPDIR:-/tmp}/rose_bench_fact_$$"
mkdir -p "$TMP"

# ⚠ `cargo` vit souvent dans `~/.cargo/bin`, absent du PATH par défaut :
# sans ceci le bench Rust/rug affiche « cargo absent » alors que Rust est là.
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"

ROSE_BIN="${ROSE_BIN:-./rose/rose}"

echo "══════════════════════════════════════════════════════════════════"
echo "  Bench factorielles étendues — N = $N"
echo "══════════════════════════════════════════════════════════════════"

# ── 1. Rose (num-bigint) ────────────────────────────────────────────────
echo
echo "── Rose (num-bigint) ──"
if [ -x "$ROSE_BIN" ]; then
  "$ROSE_BIN" bench/suite/factorielles.rose "$N" 2>&1 | grep -E "ms, " | sed 's/.*   /  /'
else
  echo "  (binaire absent : $ROSE_BIN)"
fi

# ── 3. GMP/C ────────────────────────────────────────────────────────────
echo
echo "── GMP/C ──"
if command -v gcc >/dev/null 2>&1 && [ -f /usr/include/gmp.h -o -f /usr/include/aarch64-linux-gnu/gmp.h -o -f /usr/include/x86_64-linux-gnu/gmp.h ]; then
  if gcc -O3 -march=native bench/suite/fact_gmp.c -o "$TMP/fact_gmp" -lgmp 2>/dev/null; then
    "$TMP/fact_gmp" "$N" | sed 's/^/  /'
  else
    echo "  (échec de compilation GMP)"
  fi
else
  echo "  (GMP absent : installer libgmp-dev)"
fi

# ── 3 bis. GMP/C++ (gmpxx) ──────────────────────────────────────────────
echo
echo "── GMP/C++ (gmpxx) ──"
if command -v g++ >/dev/null 2>&1 && [ -f /usr/include/gmpxx.h -o -f /usr/include/aarch64-linux-gnu/gmpxx.h ]; then
  if g++ -O3 -march=native bench/suite/fact_gmp.cpp -o "$TMP/fact_gmp_cpp" -lgmpxx -lgmp 2>/dev/null; then
    "$TMP/fact_gmp_cpp" "$N" | sed 's/^/  /'
  else
    echo "  (échec de compilation gmpxx)"
  fi
else
  echo "  (gmpxx absent : installer libgmp-dev / libgmpxx)"
fi

# ── 3 ter. Rust + rug (GMP lié en Rust) ─────────────────────────────────
echo
echo "── Rust + rug (GMP) ──"
RUG_DIR="${TMPDIR:-/tmp}/rose_bench_rug"
if command -v cargo >/dev/null 2>&1; then
  if [ ! -x "$RUG_DIR/target/release/fact_rs" ]; then
    mkdir -p "$RUG_DIR"
    ( cd "$RUG_DIR" && cargo init --name fact_rs --bin -q 2>/dev/null || cargo init --name fact_rs --bin ) >/dev/null 2>&1
    ( cd "$RUG_DIR" && cargo add rug --features integer -q ) >/dev/null 2>&1
    cp bench/suite/fact_rs.rs "$RUG_DIR/src/main.rs"
    ( cd "$RUG_DIR" && cargo build --release -q ) >/dev/null 2>&1
  fi
  if [ -x "$RUG_DIR/target/release/fact_rs" ]; then
    "$RUG_DIR/target/release/fact_rs" "$N" | sed 's/^/  /'
  else
    echo "  (échec de compilation rug)"
  fi
else
  echo "  (cargo absent)"
fi

# ── 3 quater. Rust PUR (num-bigint, AUCUNE dépendance GMP) ──────────────
echo
echo "── Rust pur (num-bigint) ──"
PUR_DIR="${TMPDIR:-/tmp}/rose_bench_rs_pur"
if command -v cargo >/dev/null 2>&1; then
  if [ ! -x "$PUR_DIR/target/release/fact_rs_pur" ]; then
    mkdir -p "$PUR_DIR"
    ( cd "$PUR_DIR" && cargo init --name fact_rs_pur --bin -q 2>/dev/null || cargo init --name fact_rs_pur --bin ) >/dev/null 2>&1
    ( cd "$PUR_DIR" && cargo add num-bigint num-traits -q ) >/dev/null 2>&1
    cp bench/suite/fact_rs_pur.rs "$PUR_DIR/src/main.rs"
    ( cd "$PUR_DIR" && cargo build --release -q ) >/dev/null 2>&1
  fi
  if [ -x "$PUR_DIR/target/release/fact_rs_pur" ]; then
    "$PUR_DIR/target/release/fact_rs_pur" "$N" | sed 's/^/  /'
  else
    echo "  (échec de compilation num-bigint)"
  fi
else
  echo "  (cargo absent)"
fi

# ── 4. Python ───────────────────────────────────────────────────────────
echo
echo "── Python (math.factorial) ──"
python3 - "$N" <<'PY' | sed 's/^/  /'
import sys, time
sys.set_int_max_str_digits(100_000_000)
N = int(sys.argv[1])
def nc(x): return len(str(x))

t = time.perf_counter(); f = __import__('math').factorial(N)
print(f"n!       : {(time.perf_counter()-t)*1000:.2f} ms, {nc(f)} chiffres")

t = time.perf_counter()
d = 1
for i in range(N, 0, -2): d *= i
print(f"n!!      : {(time.perf_counter()-t)*1000:.2f} ms, {nc(d)} chiffres")

t = time.perf_counter()
tr = 1
for i in range(N, 0, -3): tr *= i
print(f"n!!!     : {(time.perf_counter()-t)*1000:.2f} ms, {nc(tr)} chiffres")

t = time.perf_counter()
q = 1
for i in range(N, 0, -4): q *= i
print(f"n!!!!    : {(time.perf_counter()-t)*1000:.2f} ms, {nc(q)} chiffres")

# !n : récurrence
t = time.perf_counter()
dn = 1
for i in range(1, N+1):
    dn = i*dn + (1 if i % 2 == 0 else -1)
print(f"!n       : {(time.perf_counter()-t)*1000:.2f} ms, {nc(dn)} chiffres")
PY

echo
echo "══════════════════════════════════════════════════════════════════"
rm -rf "$TMP"
