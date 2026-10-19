#!/bin/sh
# ═══════════ Rose — build croisé x86-64 Linux (sans droits root) ═══════════
# Compiles le moteur C++ pour x86-64-linux-gnu depuis n'importe quelle hôte
# (testé depuis aarch64), puis le teste sous qemu-x86_64 statique.
#
# Outillage requis, tout téléchargeable sans root (voir scripts d'installation
# dans la note de version) :
#   CLANG   compilateur (clang >= 14, multi-cible)
#   LLD     repertoire contenant ld.lld          (LLD_LIBRARY_PATH pour libLLVM)
#   SYSROOT en-tetes + bibliotheques amd64 (libc6-dev, libgcc-13-dev,
#           libstdc++-13-dev, libgmp-dev, linux-libc-dev, extraits par dpkg-deb)
#   QEMU    qemu-x86_64-statique (paquet qemu-user-static 8.2, dpkg-deb -x)
#
# Usage :
#   ./build-x64.sh            construction de moteur/cpp/rose-x64
#   ./build-x64.sh --test     + suite moteur complete sous qemu
set -u
RACINE=$(cd "$(dirname "$0")/.." && pwd)
CLANG=${CLANG:-clang++}
SYSROOT=${SYSROOT:-/tmp/opencode/sysroot-amd64}
LDDIR=${LDDIR:-/tmp/opencode/lld18/usr/lib/llvm-18/bin}
QEMU=${QEMU:-/tmp/opencode/qemu82/usr/bin/qemu-x86_64-static}
LIBLLVM=${LIBLLVM:-/tmp/opencode/llvmlib}

S=$SYSROOT
[ -d "$S/usr/lib/x86_64-linux-gnu" ] || { echo "sysroot manquant: $S"; exit 1; }
# merged-usr : les scripts de lien libc référencent /lib et /lib64
[ -e "$S/lib" ]  || ln -s usr/lib "$S/lib"
[ -e "$S/lib64" ] || ln -s usr/lib "$S/lib64"
[ -e "$S/usr/lib/ld-linux-x86-64.so.2" ] || \
    ln -s x86_64-linux-gnu/ld-linux-x86-64.so.2 "$S/usr/lib/ld-linux-x86-64.so.2"

cd "$RACINE/moteur/cpp"
echo "── clang → x86_64-unknown-linux-gnu (sysroot: $S)"
LD_LIBRARY_PATH="$LIBLLVM" "$CLANG" -std=c++20 -O2 \
    -target x86_64-unknown-linux-gnu --sysroot="$S" \
    -B"$LDDIR" -B"$S/usr/lib/gcc/x86_64-linux-gnu/13" -fuse-ld=lld \
    -isystem "$S/usr/include/c++/13" \
    -isystem "$S/usr/include/x86_64-linux-gnu/c++/13" \
    -isystem "$S/usr/include/x86_64-linux-gnu" \
    -isystem "$S/usr/include" \
    src/rose.cpp src/jit.cpp src/jit_x64.cpp \
    -o rose-x64 \
    "$S/usr/lib/x86_64-linux-gnu/libgmpxx.a" "$S/usr/lib/x86_64-linux-gnu/libgmp.a" \
    -L"$S/usr/lib/x86_64-linux-gnu" -lstdc++ -lm -ldl || exit 1
file rose-x64 | sed 's/,.*LSB/ LSB/'

xrun() { LD_LIBRARY_PATH="$LIBLLVM" "$QEMU" -L "$S" "$@"; }

# Wrappeur d'exécution sous qemu (pour les suites et la parité croisée)
cat > rose-x64-qemu <<EOF
#!/bin/sh
exec env LD_LIBRARY_PATH="$LIBLLVM" "$QEMU" -L "$S" "$PWD/rose-x64" "\$@"
EOF
chmod +x rose-x64-qemu

if [ "${1:-}" = "--test" ]; then
    echo "── test_jit sous qemu (repli interpréteur attendu)"
    LD_LIBRARY_PATH="$LIBLLVM" "$CLANG" -std=c++20 -O2 \
        -target x86_64-unknown-linux-gnu --sysroot="$S" \
        -B"$LDDIR" -B"$S/usr/lib/gcc/x86_64-linux-gnu/13" -fuse-ld=lld \
        -isystem "$S/usr/include/c++/13" -isystem "$S/usr/include/x86_64-linux-gnu/c++/13" \
        -isystem "$S/usr/include/x86_64-linux-gnu" -isystem "$S/usr/include" \
        -I. src/test_jit.cpp src/jit.cpp src/jit_x64.cpp -o test_jit-x64 \
        "$S/usr/lib/x86_64-linux-gnu/libgmpxx.a" "$S/usr/lib/x86_64-linux-gnu/libgmp.a" \
        -L"$S/usr/lib/x86_64-linux-gnu" -lstdc++ -lm -ldl || exit 1
    xrun ./test_jit-x64 | tail -2
    echo "── suite moteur sous qemu (lente — émulation TCG)"
    ROSE_MOTEUR="$RACINE/moteur/cpp/rose-x64-qemu" python3 ../tests/courir.py | tail -3
fi
