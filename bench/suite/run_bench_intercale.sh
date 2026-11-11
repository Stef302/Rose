#!/bin/bash
# Mesure entrelacée : chaque binaire est mesuré à tour de rôle, ce qui
# neutralise les variations de charge de la machine. On retient le minimum
# (le bruit ne fait que gonfler les temps).
#
#   ./bench/suite/run_bench_intercale.sh [N]
set -u
cd "$(dirname "$0")/../.."
N="${1:-1000000}"
ROUNDS="${ROUNDS:-40}"
ROSE_BIN="${ROSE_BIN:-./rose/rose}"
TMP="${TMPDIR:-/tmp}/rose_intercale_$$"
mkdir -p "$TMP"

cat > "$TMP/b.rose" <<EOF
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
cat > "$TMP/b.rs" <<EOF
#[derive(Clone,Copy)]struct C{re:f64,im:f64}
#[inline]fn mul(a:C,b:C)->C{let p=a.re*b.im;C{re:a.re*b.re-a.im*b.im,im:p+p}}
fn main(){const N:usize=$N;let(mut tr,mut ti)=(0.0f64,0.0f64);
for k in 1..=N{let zr=k as f64*0.000001;let zi=1.0-zr;
let zi2=C{re:zi*0.0,im:zi*1.0};
let z=C{re:zr+zi2.re,im:0.0+zi2.im};
let w=mul(z,z);tr+=w.re;ti+=w.im;}
println!("{}",tr);println!("{}",ti);}
EOF
cat > "$TMP/b.cpp" <<EOF
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
rustc -O -o "$TMP/b_rs" "$TMP/b.rs" 2>/dev/null
g++ -O3 -o "$TMP/b_cpp" "$TMP/b.cpp" 2>/dev/null
# AOT (LLVM ORC) : compilation native avec vectorisation NEON.
# On compile UNE FOIS hors de la boucle de mesure : le temps de compilation
# (≈ 0,5 s) ne doit pas polluer la mesure de l'exécution.
"$ROSE_BIN" orc "$TMP/b.rose" -o "$TMP/b_aot.o" -O3 >/dev/null 2>&1 && gcc -O3 -o "$TMP/b_aot" "$TMP/b_aot.o" -lm 2>/dev/null

declare -A best
best[rose]=999999999; best[rust]=999999999; best[cpp]=999999999
best[rose_void]=999999999; best[rust_void]=999999999; best[rose_aot]=999999999
for r in $(seq 1 "$ROUNDS"); do
  for who in rose rust cpp rose_aot rose_void rust_void; do
    case $who in
      rose)      cmd=("$ROSE_BIN" run "$TMP/b.rose");;
      rust)      cmd=("$TMP/b_rs");;
      cpp)       cmd=("$TMP/b_cpp");;
      rose_aot)  cmd=("$TMP/b_aot");;
      rose_void) cmd=("$ROSE_BIN" run /tmp/opencode/trivial.rose);;
      rust_void) cmd=("$TMP/b_rs_empty");;
    esac
    [ "$who" = rust_void ] && continue
    [ "$who" = rose_aot ] && [ ! -x "$TMP/b_aot" ] && continue
    s=$(date +%s%N); "${cmd[@]}" >/dev/null 2>&1; e=$(date +%s%N)
    us=$(( (e-s)/1000 ))
    [ "$us" -lt "${best[$who]}" ] && best[$who]=$us
  done
done

printf "Banc complexe — N = %s  (%s tours entrelacés, minimum)\n" "$N" "$ROUNDS"
printf "  %-28s %8.3f ms\n" "Rose (JIT Cranelift)"  "$(echo "scale=3; ${best[rose]}/1000" | bc)"
printf "  %-28s %8.3f ms\n" "Rose (AOT LLVM ORC)"   "$(echo "scale=3; ${best[rose_aot]}/1000" | bc)"
printf "  %-28s %8.3f ms\n" "C++   (-O3)"           "$(echo "scale=3; ${best[cpp]}/1000" | bc)"
printf "  %-28s %8.3f ms\n" "Rust  (-O)"            "$(echo "scale=3; ${best[rust]}/1000" | bc)"
printf "  %-28s %8.3f ms\n" "Rose (démarrage seul)" "$(echo "scale=3; ${best[rose_void]}/1000" | bc)"
rm -rf "$TMP"
