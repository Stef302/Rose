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
# Usage : sh scripts/build-windows-rebuild.sh [--cargo]
#
#   --cargo    : chemin AVEC cargo (cross-build x86_64-pc-windows-msvc via
#                xwin). Ce chemin reste disponible mais n'est plus le défaut.
#   --selfhost : chemin SANS cargo (défaut). On appelle
#                scripts/build-sans-cargo.sh --windows, qui compile par C0 le
#                moteur, l'émetteur PE x64 et le générateur de ressources
#                Win32, puis produit et vérifie les 5 artefacts PE32+ x86-64
#                du stage : Rose-win-Setup.exe, rose.exe, rose-shell.exe,
#                rose-tray.exe, payload/rose.exe. Le moteur Windows complet
#                n'est pas encore émissible (sous-ensemble RIR entiers) :
#                voir rose/docs/RELEASE_100.md.
set -eu

MODE=selfhost
for arg in "$@"; do
  case "$arg" in
    --cargo) MODE=cargo ;;
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

# Délai maximal (s) alloué à la compilation C0 du moteur (build-sans-cargo.sh
# utilise 900 s par défaut : insuffisant pour le moteur complet, qui met
# ~9-15 min sur ce nœud). Re-mesurable via ROSE_SELFHOST_TIMEOUT.
SELFHOST_TIMEOUT="${ROSE_SELFHOST_TIMEOUT:-3600}"

TARGET=x86_64-pc-windows-msvc
R="$RACINE/rose/target/$TARGET/release"
STAGE="$V/stage"

# ── Chemin SANS cargo (--selfhost) ─────────────────────────────────────
if [ "$MODE" = selfhost ]; then
  SOURCE_OUT="${ROSE_SELFHOST_OUT:-/tmp/opencode/release-sans-cargo}"
  echo "=== MODE SELFHOST : compilation par C0, AUCUNE commande cargo ==="
  ARGS="--c0 ${ROSE_C0:-/tmp/opencode/C0_h.elf} --sortie $SOURCE_OUT --windows --timeout $SELFHOST_TIMEOUT"
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

  ARTEFACTS="Rose-win-Setup.exe rose.exe rose-shell.exe rose-tray.exe payload/rose.exe"
  for f in $ARTEFACTS; do
    [ -s "$SOURCE_OUT/$f" ] || { echo "ARTEFACT WINDOWS ABSENT : $f" >&2; exit 1; }
  done

  rm -rf "$STAGE"
  mkdir -p "$STAGE/payload"
  cp "$MOTEUR" "$SOURCE_OUT/rose" "$SOURCE_OUT/MANIFESTE.txt" \
     "$SOURCE_OUT/resultats.tsv" "$STAGE/"
  cp "$SOURCE_OUT/Rose-win-Setup.exe" "$SOURCE_OUT/rose.exe" \
     "$SOURCE_OUT/rose-shell.exe" "$SOURCE_OUT/rose-tray.exe" "$STAGE/"
  cp "$SOURCE_OUT/payload/rose.exe" "$STAGE/payload/rose.exe"
  for rapport in WINDOWS_ARTIFACTS.tsv WINDOWS_TENTATIVES.txt \
                 LLVM_READOJ.txt VERIFIER_RES.txt \
                 emetteur_pe_x64.elf pe_stub_ressources.elf; do
    [ -f "$SOURCE_OUT/$rapport" ] && cp "$SOURCE_OUT/$rapport" "$STAGE/"
  done
  cat > "$STAGE/NON_COUVERT_SELFHOST.txt" <<'TXT'
RELEASE AUTO-HÉBERGÉE — COUVERTURE RÉELLE (constat daté, re-mesurable)

PRODUIT ici, SANS cargo :
  moteur.elf (ELF64 aarch64 compilé par C0 depuis moteur_complet.rose),
  rose (lanceur), MANIFESTE.txt, resultats.tsv,
  et les 5 artefacts Windows PE32+ x86-64 : Rose-win-Setup.exe, rose.exe,
  rose-shell.exe, rose-tray.exe, payload/rose.exe.

VÉRIFIÉ : magic MZ/PE\0\0, machine 0x8664, magic PE32+ 0x20B, sections
.text/.data (et .rsrc pour 4 artefacts, .pdata pour rose.exe), imports
KERNEL32.DLL, tailles/md5 dans WINDOWS_ARTIFACTS.tsv, et
`verifier-res.py Rose-win-Setup.exe` -> RESSOURCES VALIDES.

STUBS (structure valide, pas un moteur Windows fonctionnel) :
  rose.exe           : PE émis par C0 depuis un pilote RIR entier (>> 73).
                       Cause : l'émetteur G5 x64 couvre entiers/+,-,*,/,%,
                       comparaisons, slots, appels résolus, sauts — pas les
                       chaînes/listes/tables du RIR du moteur complet (705
                       fonctions). Le RIR complet est refusé par le moteur C0
                       en ligne 8562 ("Addition valeur + entier impossible")
                       et, via la graine Rust, par l'émetteur ("forme non
                       couverte : lst"). Voir WINDOWS_TENTATIVES.txt.
  rose-shell.exe     : source Rose absente (rose-shell.rs = 308 l. Win32) ;
                       PE stub avec .rsrc.
  rose-tray.exe      : source Rose absente (rose-tray.rs = 670 l., icône +
                       timer 6 h) ; PE stub avec .rsrc.
  Rose-win-Setup.exe : GUI setup_gui.rs (4 648 l.) absent du composé ;
                       stub PE avec une vraie table .rsrc (icône 16x16)
                       validée par verifier-res.py.
  payload/rose.exe   : même cause que rose.exe ; PE stub avec .rsrc.

Le lanceur `rose` refuse explicitement (rc=3) aot/orc/link/llvm/console/
repl/rvm/wasm. Geste de remplacement complet : rose/docs/RELEASE_100.md.
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
  echo "=== 5 ARTEFACTS WINDOWS (stage) ==="
  cat "$STAGE/WINDOWS_ARTIFACTS.tsv"
  echo "=== RESSOURCES DU SETUP (stage) ==="
  python3 "$RACINE/installateur/velopack/verifier-res.py" "$STAGE/Rose-win-Setup.exe"
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
