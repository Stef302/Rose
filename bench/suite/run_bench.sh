#!/bin/bash
# Banc comparatif du calcul complexe (z = re + im*i ; w = z*z ; accumulateurs).
#
#   ./bench/suite/run_bench.sh [N]
#
# Mesure le TEMPLE DE BOUCLE de chaque implementation (le processus est lance
# une fois par mesure, on retient le meilleur de 5 ; le demarrage du
# processus est donc inclus — c'est la realite d'un lancement `rose run`).
set -u
cd "$(dirname "$0")/../.."
N="${1:-1000000}"
REPS="${REPS:-5}"
ROSE_BIN="${ROSE_BIN:-./rose/rose}"
TMP="${TMPDIR:-/tmp}/rose_bench_$$"
mkdir -p "$TMP"

cat > "$TMP/bench.rose" <<EOF
fn calc(n: entier) -> flottant {
  \$total_re = 0.0
  \$total_im = 0.0
  pour k de 1 vers n + 1 {
    \$z_re = k * 0.000001
    \$z_im = 1.0 - \$z_re
    \$z = \$z_re + \$z_im * i
    \$w = \$z * \$z
    \$total_re = \$total_re + re(\$w)
    \$total_im = \$total_im + im(\$w)
  }
  retour \$total_re
}
>> calc($N)
EOF

cat > "$TMP/bench.cpp" <<EOF
#include <cstdio>
int main(){const long N=$N;double tr=0,ti=0;
for(long k=1;k<=N;++k){double zr=(double)k*0.000001,zi=1.0-zr;
double wr=zr*zr-zi*zi,wi=2.0*zr*zi;tr+=wr;ti+=wi;}
printf("%f\n%f\n",tr,ti);return 0;}
EOF

cat > "$TMP/bench.rs" <<EOF
fn main(){const N:usize=$N;let(mut tr,mut ti)=(0.0f64,0.0f64);
for k in 1..=N{let zr=k as f64*0.000001;let zi=1.0-zr;
let wr=zr*zr-zi*zi;let wi=2.0*zr*zi;tr+=wr;ti+=wi;}
println!("{}",tr);println!("{}",ti);}
EOF

cat > "$TMP/bench.py" <<EOF
N=$N
tr=0.0;ti=0.0
for k in range(1,N+1):
    zr=k*0.000001;zi=1.0-zr
    tr+=zr*zr-zi*zi;ti+=2.0*zr*zi
print(tr);print(ti)
EOF

g++ -O3 -o "$TMP/bench_cpp" "$TMP/bench.cpp" 2>/dev/null
rustc -O -o "$TMP/bench_rs" "$TMP/bench.rs" 2>/dev/null

best() { # $1 = libellé, reste = commande
  local label="$1"; shift
  local b=99999999
  for _ in $(seq 1 "$REPS"); do
    local s e
    s=$(date +%s%N); "$@" >/dev/null 2>&1; e=$(date +%s%N)
    local ms=$(( (e-s)/1000000 ))
    [ "$ms" -lt "$b" ] && b=$ms
  done
  printf "  %-28s %6d ms\n" "$label" "$b"
}

echo "Banc complexe — N = $N  (meilleur de $REPS, processus inclus)"
best "C++   (-O3)"            "$TMP/bench_cpp"
best "Rust  (-O)"             "$TMP/bench_rs"
best "Rose  (JIT Cranelift)"  "$ROSE_BIN" run "$TMP/bench.rose"
ROSE_SANS_JIT=1 best "Rose  (interpréteur)" env ROSE_SANS_JIT=1 "$ROSE_BIN" run "$TMP/bench.rose"
best "Python (CPython)"       python3 "$TMP/bench.py"

rm -rf "$TMP"
