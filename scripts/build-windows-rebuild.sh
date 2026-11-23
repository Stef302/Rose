#!/bin/sh
# Reconstruction COMPLÈTE des artefacts Windows de Rose depuis Linux, avec
# les sources actuelles (tous les commits récents) et le shim de lien mmap.
#
# Pourquoi ce script existe :
#   `installateur/velopack/build-linux.sh` est la chaîne de build normale,
#   mais depuis le commit M3.1 (1b73c74) `rose/src/natif_emetteur.rs` appelle
#   mmap/mprotect/munmap de façon inconditionnelle. Ces symboles POSIX
#   n'existent pas sur Windows et ne sont pas fournis par windows_compat.c :
#   le cross-build x86_64-pc-windows-msvc échoue au lien. Ce script compile
#   un shim (installateur/windows-compat/mmap_win.c, VirtualAlloc-based) et
#   l'injecte au lieur SANS toucher à rose/src/*.
#
# Deux passes sont nécessaires : pack-res.py embarque le rose.exe présent dans
# target/ AU MOMENT où il tourne. La passe 1 produit un moteur frais ; la
# passe 2 régénère rose-setup.res avec CE moteur frais, puis relie rose-setup.
#
# Usage : sh scripts/build-windows-rebuild.sh [--selfhost]
#
#   --selfhost : chemin SANS cargo. On appelle scripts/build-sans-cargo.sh,
#                qui compile compilateur/moteur_complet.rose avec C0 et
#                produit un ELF64 aarch64 vérifié. Les artefacts Windows PE
#                x86_64 (rose.exe, rose-shell.exe, rose-tray.exe,
#                rose-setup.exe) ne sont PAS produits : C0 n'émet que de
#                l'ELF aarch64 et le composé n'a ni writer PE x86_64 ni
#                ressources Win32. Leur absence est documentée dans le stage.
#                Le chemin cargo reste le défaut, inchangé (release v0.1.76).
set -eu

MODE=cargo
for arg in "$@"; do
  case "$arg" in
    --selfhost) MODE=selfhost ;;
    -h|--help) sed -n '2,24p' "$0"; exit 0 ;;
    *) echo "OPTION INCONNUE : $arg" >&2; exit 1 ;;
  esac
done

RACINE="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="$HOME/.cargo/bin:$PATH"
X="${XWIN_SDK:-$HOME/.cache/cargo-xwin/xwin}"
V="$RACINE/installateur/velopack"
LOGDIR="${ROSE_REBUILD_LOG:-/tmp/opencode/rose-rebuild}"
mkdir -p "$LOGDIR"

TARGET=x86_64-pc-windows-msvc
R="$RACINE/rose/target/$TARGET/release"
STAGE="$V/stage"

# ── Chemin SANS cargo (--selfhost) ─────────────────────────────────────
if [ "$MODE" = selfhost ]; then
  SOURCE_OUT="${ROSE_SELFHOST_OUT:-/tmp/opencode/release-sans-cargo}"
  echo "=== MODE SELFHOST : compilation par C0, AUCUNE commande cargo ==="
  ARGS="--c0 ${ROSE_C0:-/tmp/opencode/C0_h.elf} --sortie $SOURCE_OUT"
  if [ -n "${ROSE_REFERENCE:-}" ]; then
    ARGS="$ARGS --reference $ROSE_REFERENCE"
  fi
  # shellcheck disable=SC2086
  bash "$RACINE/scripts/build-sans-cargo.sh" $ARGS

  MOTEUR="$SOURCE_OUT/moteur.elf"
  magic="$(od -An -tx1 -N4 "$MOTEUR" | tr -d ' \n')"
  machine="$(od -An -tu2 -j18 -N2 "$MOTEUR" | tr -d ' \n')"
  [ "$magic" = "7f454c46" ] && [ "$machine" = "183" ] || {
    echo "ARTEFACT INVALIDE : magic=$magic machine=$machine" >&2; exit 1; }

  rm -rf "$STAGE"
  mkdir -p "$STAGE/payload"
  cp "$MOTEUR" "$SOURCE_OUT/rose" "$SOURCE_OUT/MANIFESTE.txt" \
     "$SOURCE_OUT/resultats.tsv" "$STAGE/"
  cat > "$STAGE/NON_COUVERT_SELFHOST.txt" <<'TXT'
RELEASE AUTO-HÉBERGÉE — COUVERTURE RÉELLE (constat daté, re-mesurable)

PRODUIT ici : moteur.elf (ELF64 aarch64 compilé par C0 depuis
compilateur/moteur_complet.rose), rose (lanceur), MANIFESTE.txt,
resultats.tsv (12 programmes, sortie capturée).

NON PRODUIT ici (et POURQUOI) :
  rose.exe           : C0 n'émet que de l'ELF64 aarch64 ; aucun writer PE
                       x86_64 dans le composé Rose. Le JIT Cranelift et
                       l'AOT LLVM ORC du binaire Rust n'ont pas d'équivalent
                       dans le point d'entrée C0.
  rose-shell.exe     : simple lanceur Win32 du moteur ; sans rose.exe PE,
                       il n'a rien à lancer.
  rose-tray.exe      : helper Win32 (icône + timer 6 h) écrit en Rust ;
                       aucune routine Win32 dans le composé.
  Rose-win-Setup.exe : fenêtre Win32 + ressources RCDATA/ICON/FONT/MANIFEST
                       générées par pack-res.py et liées par lld-link ;
                       C0 n'a ni table de ressources ni lieur PE.
  payload/rose.exe   : même cause que rose.exe.

Le lanceur `rose` refuse explicitement (rc=3) aot/orc/link/llvm/console/
repl/rvm/wasm. `comp` produit un gabarit ELF patché, pas un PE.
Jalon 100 % : writer PE x86_64 + ressources Win32 en Rose pur, puis
remplacement des 3 appels cargo par ce chemin dans ce script.
TXT

  echo "=== VÉRIFICATION ARTEFACTS SELFHOST ==="
  file -b "$STAGE/moteur.elf" | cut -d, -f1-2
  printf '  moteur.elf  %10s o  %s\n' "$(stat -c%s "$MOTEUR")" \
    "$(md5sum "$MOTEUR" | cut -d' ' -f1)"
  printf '  rose        %10s o  %s\n' "$(stat -c%s "$STAGE/rose")" \
    "$(md5sum "$STAGE/rose" | cut -d' ' -f1)"
  "$STAGE/rose" aot "$MOTEUR" >/dev/null 2>&1 && rc_refus=0 || rc_refus=$?
  echo "  commande non couverte aot : rc=$rc_refus (3 attendu)"
  [ "$rc_refus" -eq 3 ] || { echo "REFUS ATTENDU ABSENT" >&2; exit 1; }
  echo "=== ARTEFACTS WINDOWS ABSENTS (documentés) ==="
  echo "  Rose-win-Setup.exe rose.exe rose-shell.exe rose-tray.exe payload/rose.exe"
  echo "=== STAGE ==="; ls -la "$STAGE"
  exit 0
fi

# ── Prérequis ──────────────────────────────────────────────────────────
for outil in clang lld-link llvm-lib llvm-rc python3; do
  command -v "$outil" >/dev/null 2>&1 || { echo "$outil manquant" >&2; exit 1; }
done
[ -d "$X/sdk/lib/um/x86_64" ] || { echo "SDK xwin introuvable dans $X" >&2; exit 1; }

# ── 1. Shim mmap (symboles POSIX → Win32) ──────────────────────────────
# ⚠ CORRECTION 21/11 (constat de build): windows_compat.lib fournit DEJA
# mmap/mprotect/munmap (le code a évolué depuis M3.1). Injecter mmap_win.obj
# en plus donne "duplicate symbol: mmap/mprotect/munmap" au lien -> rc=101.
# On utilise donc lld-link directement, sans shim externe.
WRAP="$LOGDIR/link-cl-shim"
printf '#!/bin/sh\nexec lld-link "$@"\n' > "$WRAP"
chmod +x "$WRAP"

# ── 3. Environnement de build (identique à build-linux.sh) ─────────────
export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER="$WRAP"
export CC_x86_64_pc_windows_msvc=clang
export CXX_x86_64_pc_windows_msvc=clang
export AR_x86_64_pc_windows_msvc=llvm-lib
export CFLAGS_x86_64_pc_windows_msvc="-isystem $X/crt/include -isystem $X/sdk/include/ucrt -isystem $X/sdk/include/um -isystem $X/sdk/include/shared"
export RUSTFLAGS="-L $X/crt/lib/x86_64 -L $X/sdk/lib/ucrt/x86_64 -L $X/sdk/lib/um/x86_64 -C target-feature=+crt-static"

cd "$RACINE/rose"

# ── 4. Passe 1 : moteur + shell + setup ────────────────────────────────
echo "=== PASSE 1 : build rose / rose-shell / rose-tray / rose-setup ==="
# ⚠ CORRECTION 21/11: pack-res.py exige un rose.exe DEJA present. On construit
# donc le moteur AVANT la premiere passe d'emballage (sinon "moteur absent").
cargo build --release --target "$TARGET" --bin rose
python3 "$V/embed-assets.py" >/dev/null
python3 "$V/pack-res.py" >/dev/null
cargo build --release --target "$TARGET" --bin rose --bin rose-shell --bin rose-tray --bin rose-setup

# ── 5. Passe 2 : .res avec le moteur FRAIS, puis relink du setup ───────
echo "=== PASSE 2 : .res avec moteur frais + relink rose-setup ==="
python3 "$V/embed-assets.py" >/dev/null
python3 "$V/pack-res.py"
cargo build --release --target "$TARGET" --bin rose-setup

# ── 6. Dossier de distribution (repart de zéro) ─────────────────────────
rm -rf "$STAGE"
mkdir -p "$STAGE/payload"
cp "$R/rose-setup.exe" "$STAGE/Rose-win-Setup.exe"
cp "$R/rose.exe" "$STAGE/rose.exe"
cp "$R/rose-shell.exe" "$STAGE/rose-shell.exe"
# Helper tray : sans lui, l'icône Rose ne peut pas apparaître dans la zone
# des icônes cachées (vague 77). pack-res.py l'embarque comme RCDATA 9.
cp "$R/rose-tray.exe" "$STAGE/rose-tray.exe"
cp "$R/rose.exe" "$STAGE/payload/rose.exe"
# Trois icônes DISTINCTES (voir build-linux.sh / pack-res.py) :
#   installateur/assets/rose.ico = DOCUMENT .rose (type de fichier)
#   rose/rose.ico                = LA ROSE (logo application)
#   rose/rose_shell.ico          = CONSOLE (terminal + rose)
cp "$RACINE/installateur/assets/rose.ico" "$STAGE/rose.ico"
cp "$RACINE/rose/rose.ico" "$STAGE/rose_app.ico"
cp "$RACINE/rose/rose_shell.ico" "$STAGE/rose_shell.ico"
cp "$RACINE/installateur/assets/rose_setup.ico" "$STAGE/rose_setup.ico"
cp "$RACINE/LICENSE" "$STAGE/LICENSE"

# ── 7. Vérifications ───────────────────────────────────────────────────
echo "=== VÉRIFICATION RESSOURCES (rose-setup.exe lié) ==="
python3 "$V/verifier-res.py" "$R/rose-setup.exe"
echo "=== FICHIERS PRODUITS ==="
for f in rose.exe rose-shell.exe rose-tray.exe rose-setup.exe; do
  printf '  %-16s %10s o  %s\n' "$f" "$(stat -c%s "$R/$f")" "$(file -b "$R/$f" | cut -d, -f1-2)"
done
echo "=== HASHES DU STAGE ==="
md5sum "$STAGE/Rose-win-Setup.exe" "$STAGE/rose.exe" "$STAGE/rose-shell.exe" "$STAGE/rose-tray.exe" "$STAGE/payload/rose.exe"
