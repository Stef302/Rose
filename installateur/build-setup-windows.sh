#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════
#  Construit l'installeur graphique Rose pour Windows x64 (64 bits natif).
#
#  Ce n'est PLUS du NSIS : c'est le binaire « rose-setup », écrit en Rust
#  (interface Win32 maison : bandeau rose, pétale, boutons dessinés au GDI).
#
#  Prérequis :
#    - toolchain Rust avec la cible x86_64-pc-windows-msvc
#      (cargo-xwin fournit le SDK Windows : cargo xwin build …)
#    - makensis n'est PLUS nécessaire
#
#  Sortie : installateur/dist/Rose-Setup-<version>.exe (+ SHA256SUMS.txt)
# ═══════════════════════════════════════════════════════════════════════
set -euo pipefail

RACINE="$(cd "$(dirname "$0")/.." && pwd)"
I="$RACINE/installateur"
R="$RACINE/rose"
VERSION="${VERSION:-5.10.1}"
CIBLE="x86_64-pc-windows-msvc"

export PATH="$HOME/.cargo/bin:$PATH"

echo "── 1/5  moteur rose.exe (${CIBLE}) ──────────────────────────────"
# le config crt-static casse les proc-macros inkwell : on le met de côté
if [ -f "$R/.cargo/config.toml" ]; then
  mv "$R/.cargo/config.toml" "$R/.cargo/config.toml.bak"
  trap 'mv -f "$R/.cargo/config.toml.bak" "$R/.cargo/config.toml" 2>/dev/null || true' EXIT
fi
( cd "$R" && cargo xwin build --release --target "$CIBLE" )
if [ -f "$R/.cargo/config.toml.bak" ]; then
  mv "$R/.cargo/config.toml.bak" "$R/.cargo/config.toml"
fi

echo "── 2/5  préparation des fichiers embarqués ─────────────────────"
cp "$R/target/$CIBLE/release/rose.exe" "$R/src/bin/rose-embedded.exe"
cp "$I/assets/rose.ico" "$R/src/bin/rose-ico.ico"
cp "$I/assets/setup.ico" "$R/src/bin/setup-ico.ico"

echo "── 3/5  installeur graphique rose-setup ──────────────────────────"
( cd "$R" && cargo xwin build --release --target "$CIBLE" --features setup --bin rose-setup )

echo "── 4/5  nettoyage ───────────────────────────────────────────────"
rm -f "$R/src/bin/rose-embedded.exe" "$R/src/bin/rose-ico.ico" "$R/src/bin/setup-ico.ico"

echo "── 5/5  paquetage ──────────────────────────────────────────────"
mkdir -p "$I/dist"
cp "$R/target/$CIBLE/release/rose-setup.exe" "$I/dist/Rose-Setup-${VERSION}.exe"
( cd "$I/dist" && sha256sum "Rose-Setup-${VERSION}.exe" > SHA256SUMS.txt )

echo "------------------------------------------------------------"
file "$I/dist/Rose-Setup-${VERSION}.exe"
echo "✓ Installeur graphique produit : $I/dist/Rose-Setup-${VERSION}.exe"
echo "  empreinte : $(cat "$I/dist/SHA256SUMS.txt")"
