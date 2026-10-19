#!/bin/sh
# Banc « générateur de mot de passe » multi-langages.
# Même tâche partout : N caractères au hasard dans un alphabet de 64,
# dans un buffer initialisé à zéro (le coût d'allocation est à égalité),
# auto-chronométré côté programme (millisecondes monotones), 3 passes,
# minimum retenu. numpy est mesuré à 1/10e puis extrapolé ×10 (sa
# sur-allocation intermédiaire de 4 Go/int32 serait fatale au format long).
#
# Outils utilisés s'ils existent : g++, rustc (ou $HOME/.cargo/bin), zig
# ($ZIG ou /tmp/opencode/zig-aarch64-*/zig), python3+numpy, et le moteur
# Rose du dépôt (ROSE=... pour le changer).
#
# Usage : ./bench.sh [N]     (défaut 1 000 000 000)
set -e
HERE=$(cd "$(dirname "$0")" && pwd)
ROSE=${ROSE:-$HERE/../../moteur/cpp/rose}
N=${1:-1000000000}
CC=${CC:-g++}
rustc_bin=$(command -v rustc || echo "$HOME/.cargo/bin/rustc")
ZIG=${ZIG:-}
if [ -z "$ZIG" ]; then
    for c in "$HOME/zig/zig" /tmp/opencode/zig-aarch64-*/zig /usr/local/bin/zig; do
        [ -x "$c" ] && ZIG=$c && break
    done
fi
min3 () { # $1=libellé, reste = commande : 3 passes, min des ms affichées
    label=$1; shift
    m=999999999
    for i in 1 2 3; do
        v=$("$@" 2>/dev/null | tail -1 | tr -dc '0-9')
        if [ -n "$v" ] && [ "$v" -lt "$m" ]; then m=$v; fi
    done
    printf '%-22s %8s ms\n' "$label" "$m"
}
echo "── banc mot de passe — N = $N ──"
cd "$HERE"
$CC -O2 -march=native -std=c++20 mdp_splitmix.cpp -o mdp_splitmix
$CC -O2 -march=native mdp_mt19937.cpp -o mdp_mt19937
$CC -O2 -march=native -x c mdp_rand.c -o mdp_rand_c
[ -x "$rustc_bin" ] && "$rustc_bin" -O mdp.rs -o mdp_rs 2>/dev/null
[ -n "$ZIG" ] && "$ZIG" build-exe mdp.zig -O ReleaseSafe -femit-bin=mdp_zig 2>/dev/null
min3 'C++ (splitmix64)'      ./mdp_splitmix "$N"
min3 'C++ (mt19937 naïf)'    ./mdp_mt19937 "$N"
min3 'C   (rand glibc)'      ./mdp_rand_c "$N"
[ -x ./mdp_rs ]  && min3 'Rust (splitmix64)'  ./mdp_rs "$N"
[ -x ./mdp_zig ] && min3 'Zig (splitmix64)'   ./mdp_zig "$N"
min3 'Rose (mot_de_passe)'   "$ROSE" "$HERE/mot_de_passe.rose" "$N"
if python3 -c 'import numpy' 2>/dev/null; then
    ms10=$(python3 mdp_numpy.py "$((N / 10))")
    printf '%-22s %8s ms\n' 'numpy (1/10e ×10)' "$((ms10 * (N / (N / 10))))"
fi
