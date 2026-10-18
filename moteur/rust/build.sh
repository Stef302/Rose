#!/bin/sh
# Construit le moteur Rust de Rose : binaire autonome, GMP liée statiquement.
# Dépendances machine : rustc (std uniquement, aucun crate externe) + libgmp.a.
#   GMP_LIB=/chemin/libgmp.a ./build.sh    (sinon cherche libgmp.a du système)
set -e
cd "$(dirname "$0")"
GMP_LIB="${GMP_LIB:-}"
if [ -z "$GMP_LIB" ]; then
    for c in /tmp/opencode/toolchain/gmp-install/lib/libgmp.a \
             /usr/lib/aarch64-linux-gnu/libgmp.a /usr/lib/x86_64-linux-gnu/libgmp.a \
             /usr/lib/libgmp.a; do
        [ -f "$c" ] && GMP_LIB="$c" && break
    done
fi
if [ ! -f "$GMP_LIB" ]; then
    echo "build.sh : libgmp.a introuvable — installez libgmp-dev ou renseignez GMP_LIB" >&2
    exit 1
fi
rustc -O --edition 2021 src/main.rs -o rose -C link-args="$GMP_LIB -lm -lc"
echo "moteur/rust/rose construit."
