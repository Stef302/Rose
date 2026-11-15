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
#   rose-shell.exe   console interactive de Rose (REPL, console)
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

# Ressources de l'installeur : rose PNG + police Kare + icônes multi-tailles
# dans rose/rose-setup.res (section .rsrc COFF), liée au seul rose-setup.
# L'arbre est VALIDÉ juste après : une section .rsrc mal formée (types
# dupliqués, pointeurs avec bit 31, entrées de 8 o) ne se voit pas à la
# compilation, mais rend la rose, la police et les grandes icônes
# introuvables à l'exécution.
if command -v python3 >/dev/null 2>&1; then
  python3 "$RACINE/installateur/velopack/embed-assets.py" >/dev/null
  # pack-res.py produit un `.res` MSVC et VÉRIFIE lui-même ses types ; le
  # contrôle du CONTENU (RVAs correctes, manifeste résoluble) se fait APRÈS
  # le lien, sur `rose-setup.exe` — c'est là que les bugs apparaissent.
  python3 "$RACINE/installateur/velopack/pack-res.py"
fi
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
cargo build --release --target "$TARGET" --bin rose --bin rose-aot --bin rose-setup --bin rose-shell

R="$RACINE/rose/target/$TARGET/release"

# Dossier de distribution Velopack : l'installeur graphique, les binaires
# qu'il pose et son payload (rose.exe à côté de l'installeur, lu par
# setup_gui pour afficher le SHA-256 et par setup_core pour la copie).
STAGE="$RACINE/installateur/velopack/stage"
mkdir -p "$STAGE/payload"
cp "$R/rose-setup.exe" "$STAGE/Rose-win-Setup.exe"
cp "$R/rose.exe" "$STAGE/rose.exe"
cp "$R/rose-aot.exe" "$STAGE/rose-aot.exe"
cp "$R/rose-shell.exe" "$STAGE/rose-shell.exe"
cp "$R/rose.exe" "$STAGE/payload/rose.exe"
cp "$RACINE/installateur/assets/rose.ico" "$STAGE/rose.ico"
cp "$RACINE/installateur/assets/setup.ico" "$STAGE/rose_app.ico"
cp "$RACINE/LICENSE" "$STAGE/LICENSE"
echo
echo "Binaires produits :"
for f in rose.exe rose-aot.exe rose-shell.exe rose-setup.exe; do
  printf '  %-16s %10s o  %s\n' "$f" "$(stat -c%s "$R/$f")" "$(file -b "$R/$f" | cut -d, -f1-2)"
done

# Contrôle de l'IMAGE LIÉE : un .rsrc valide peut malgré tout produire un PE
# que Windows refuse (« Cette application ne peut pas s'exécuter sur votre
# PC »). On vérifie l'arbre des ressources DU BINAIRE, le sous-système, et
# que le manifeste est ASCII avec processorArchitecture "*" et langue 1033.
if command -v python3 >/dev/null 2>&1; then
  echo
  python3 "$RACINE/installateur/velopack/verifier-res.py" "$R/rose-setup.exe"
fi

