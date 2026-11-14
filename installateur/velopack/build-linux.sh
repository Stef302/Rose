#!/bin/sh
# Construit les binaires Windows de Rose DEPUIS Linux, sans Visual Studio.
#
# La cible est x86_64-pc-windows-msvc (la seule qui produit un PE natif
# sans toolchain mingw), liée par lld-link avec le SDK et l'UCRT fournis
# par cargo-xwin (~/.cache/cargo-xwin/xwin) et compilée par clang.
# Les trois exécutables sont STATIQUES (+crt-static) : aucune DLL
# VCRUNTIME140 / api-ms-win-crt-* n'est requise sur la machine cible,
# Windows 10/11 les lance directement.
#
# Sortie : rose/target/x86_64-pc-windows-msvc/release/
#   rose.exe         moteur (console)
#   rose-aot.exe     helper AOT LLVM ORC (console)
#   rose-setup.exe   installeur graphique (GUI, publié Rose-win-Setup.exe)
#
# Prérequis : rustup target add x86_64-pc-windows-msvc, cargo-xwin (pour le
# cache du SDK), clang, lld-link, llvm-lib.
#
# Usage : sh installateur/velopack/build-linux.sh
set -eu

RACINE="$(cd "$(dirname "$0")/../.." && pwd)"
export PATH="$HOME/.cargo/bin:$PATH"
X="${XWIN_SDK:-$HOME/.cache/cargo-xwin/xwin}"
TARGET=x86_64-pc-windows-msvc

if [ ! -d "$X/sdk/lib/um/x86_64" ]; then
  echo "SDK xwin introuvable dans $X — lancez une fois 'cargo xwin build'." >&2
  exit 1
fi
for outil in clang lld-link llvm-lib; do
  command -v "$outil" >/dev/null 2>&1 || { echo "$outil manquant" >&2; exit 1; }
done

WRAP="$(mktemp /tmp/opencode/link-cl.XXXXXX)"
trap 'rm -f "$WRAP"' EXIT
printf '#!/bin/sh\nexec lld-link "$@"\n' > "$WRAP"
chmod +x "$WRAP"

export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER="$WRAP"
export CC_x86_64_pc_windows_msvc=clang
export CXX_x86_64_pc_windows_msvc=clang
export AR_x86_64_pc_windows_msvc=llvm-lib
export CFLAGS_x86_64_pc_windows_msvc="-isystem $X/crt/include -isystem $X/sdk/include/ucrt -isystem $X/sdk/include/um -isystem $X/sdk/include/shared"
export RUSTFLAGS="-L $X/crt/lib/x86_64 -L $X/sdk/lib/ucrt/x86_64 -L $X/sdk/lib/um/x86_64 -C target-feature=+crt-static"

cd "$RACINE/rose"
cargo build --release --target "$TARGET" --bin rose --bin rose-aot --bin rose-setup

R="$RACINE/rose/target/$TARGET/release"
echo
echo "Binaires produits :"
for f in rose.exe rose-aot.exe rose-setup.exe; do
  printf '  %-16s %10s o  %s\n' "$f" "$(stat -c%s "$R/$f")" "$(file -b "$R/$f" | cut -d, -f1-2)"
done
