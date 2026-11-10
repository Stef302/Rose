#!/bin/bash
# Banc comparatif du calcul complexe (z = re + im*i ; w = z*z ; accumulateurs).
#
#   ./bench/suite/run_bench.sh [N]
#
# Mesure le temps de PROCESSUS COMPLET (lancement compris), comme le ferait
# un utilisateur : c'est la réalité d'un `rose run`.
# Affichage : min / médiane sur REPS répétitions.
set -u
cd "$(dirname "$0")/../.."
N="${1:-1000000}"
REPS="${REPS:-15}"
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

# meme calcul que complexe.rose : produit complexe GENERAL (a+bi)(c+di)
cat > "$TMP/bench.cpp" <<EOF
#include <cstdio>
struct C{double re,im;};
static inline C mul(C a,C b){double p=a.re*b.im;C r;r.re=a.re*b.re-a.im*b.im;r.im=p+p;return r;}
int main(){const long N=$N;double tr=0,ti=0;
for(long k=1;k<=N;++k){double zr=(double)k*0.000001,zi=1.0-zr;
C zir;zir.re=zi*0.0;zir.im=zi*1.0;
C z;z.re=zr+zir.re;z.im=0.0+zir.im;
C w=mul(z,z);tr+=w.re;ti+=w.im;}
printf("%f\n%f\n",tr,ti);return 0;}
EOF

cat > "$TMP/bench.rs" <<EOF
#[derive(Clone,Copy)]struct C{re:f64,im:f64}
#[inline]fn mul(a:C,b:C)->C{let p=a.re*b.im;C{re:a.re*b.re-a.im*b.im,im:p+p}}
fn main(){const N:usize=$N;let(mut tr,mut ti)=(0.0f64,0.0f64);
for k in 1..=N{let zr=k as f64*0.000001;let zi=1.0-zr;
let zi2=C{re:zi*0.0,im:zi*1.0};
let z=C{re:zr+zi2.re,im:0.0+zi2.im};
let w=mul(z,z);tr+=w.re;ti+=w.im;}
println!("{}",tr);println!("{}",ti);}
EOF

cat > "$TMP/bench.py" <<EOF
N=$N
tr=0.0;ti=0.0
for k in range(1,N+1):
    zr=k*0.000001;zi=1.0-zr
    zir=zi*0.0;zii=zi*1.0
    zr2=zr+zir;zi2=0.0+zii
    tr+=zr2*zr2-zi2*zi2
    p=zr2*zi2;ti+=p+p
print(tr);print(ti)
EOF

g++ -O3 -o "$TMP/bench_cpp" "$TMP/bench.cpp" 2>/dev/null
rustc -O -o "$TMP/bench_rs" "$TMP/bench.rs" 2>/dev/null

mesure() { # $1 = libellé, reste = commande
  local label="$1"; shift
  local vals=()
  for _ in $(seq 1 "$REPS"); do
    local s e ms
    s=$(date +%s%N); "$@" >/dev/null 2>&1; e=$(date +%s%N)
    ms=$(( (e-s)/1000 ))   # microsecondes
    vals+=("$ms")
  done
  printf '%s\n' "${vals[@]}" | sort -n | awk -v l="$label" '
    {a[NR]=$1}
    END{printf "  %-26s min %7.2f ms   médiane %7.2f ms\n", l, a[1]/1000, a[int((NR+1)/2)]/1000}'
}

echo "Banc complexe — N = $N   (processus complet, $REPS répétitions)"
mesure "C++   (-O3)"           "$TMP/bench_cpp"
mesure "Rust  (-O)"            "$TMP/bench_rs"
mesure "Rose  (JIT Cranelift)" "$ROSE_BIN" run "$TMP/bench.rose"
mesure "Rose  (interpréteur)"  env ROSE_SANS_JIT=1 "$ROSE_BIN" run "$TMP/bench.rose"
mesure "Python (CPython)"      python3 "$TMP/bench.py"

rm -rf "$TMP"
