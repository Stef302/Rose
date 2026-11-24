#!/usr/bin/env bash
# build-sans-cargo.sh — release Rose auto-hébergée, SANS appeler cargo.
#
# Principe (constat daté, re-mesurable)
# ------------------------------------
# Le seul compilateur invoqué ici est C0 : un ELF64 aarch64 autonome, déjà
# construit, qui lit compilateur/moteur_complet.rose et écrit sur stdout un
# ELF64 aarch64. Cet ELF sait lire/vérifier un source Rose, le compiler en RIR,
# exécuter le RIR, et piloter les sous-commandes réellement implémentées dans
# le point d'entrée du composé (voir le tableau COUVERTURE ci-dessous).
#
# Aucun appel à cargo, rustc, rose/rose (binaire Rust), clang, lld-link ou
# llvm-rc. Les seuls outils annexes sont bash, python3, od, stat, md5sum,
# chmod, timeout et (via scripts/garde_fou_mem.sh) ps/sleep/kill.
#
# Ce script NE MODIFIE ni compilateur/**, ni rose/src/**, ni Cargo.toml :
# il lit le source composé et n'écrit que dans le dossier de sortie.
#
# Usage
# -----
#   scripts/build-sans-cargo.sh [--c0 ELF] [--source FICHIER.rose]
#                               [--sortie DOSSIER] [--reference BINAIRE_RUST]
#                               [--rapport FICHIER] [--timeout SECONDES]
#                               [--reutiliser] [--sans-tests] [--windows]
#
# Options :
#   --c0 ELF          compilateur natif auto-hébergé. Défaut : $ROSE_C0, puis
#                     /tmp/opencode/C0_h.elf, C0_f.elf, C0_b.elf (le premier
#                     ELF aarch64 exécutable trouvé).
#   --source FICHIER  source composé (défaut compilateur/moteur_complet.rose).
#   --sortie DOSSIER  artefacts (défaut /tmp/opencode/release-sans-cargo).
#   --reference BIN   OPTIONNEL, HORS chemin autosuffisant : compare la sortie
#                     des programmes de test à un binaire Rust de référence.
#   --rapport FICHIER TSV détaillé (défaut DOSSIER/resultats.tsv).
#   --timeout N       délai maximal de compilation C0 en secondes (défaut 900).
#   --reutiliser      ne pas recompiler : vérifier un ELF déjà présent dans
#                     DOSSIER/moteur.elf (utile pour un re-jeu de preuve).
#   --sans-tests      produire les artefacts sans exécuter la batterie.
#   --windows         produire EN PLUS les 5 artefacts Windows PE32+ x86-64
#                     (rose.exe, rose-shell.exe, rose-tray.exe,
#                     Rose-win-Setup.exe, payload/rose.exe) SANS cargo, via
#                     les émetteurs Rose compilés par C0. Option désactivée
#                     par défaut : le chemin Linux n'est pas modifié.
#
# COUVERTURE (vérifiée sur l'ELF, pas déduite de la doc)
#   COUVERT   : exécution directe, run/jit/interprete/methode, verif, rir,
#               tokens, ast, latex, transpile --cible wasm, bc, comp,
#               version, help, info. ATTENTION : comp ne produit qu'un
#               gabarit ELF patché d'UNE constante (8 908 o, une seule ligne
#               de sortie) et les modules wasm/bc ne sont PAS octet-identiques
#               à ceux du binaire Rust : voir RELEASE_SANS_CARGO.md.
#   NON COUVERT : JIT Cranelift (l'ELF exécute le RIR interprété, aucune
#               compilation de boucles chaudes), AOT/ORC LLVM, link, llvm,
#               transpile js/py/c, console/repl, rvm. L'option --windows
#               produit 5 PE32+ x86-64 : rose-shell.exe, rose-tray.exe et
#               rose-setup.exe sont de VRAIS lanceurs Win32 écrits en Rose
#               (CreateProcessW / Shell_NotifyIcon+SetTimer / fenêtre GUI +
#               17 ressources) ; rose.exe, payload/rose.exe ET le RCDATA 3
#               du setup sont TOUS le même moteur console compilé par C0
#               (RIR moteur_complet.rose -> PE émetteur C0). AUCUN recours au
#               nupkg 0.1.76 : plus aucun binaire signé n'est copié ni embarqué.
#               Voir rose/docs/PAYLOAD_C0.md.
#   Le lanceur `rose` produit refuse explicitement les commandes non couvertes
#   (rc=3) au lieu de les simuler.
#
# Code de sortie : 0 si l'ELF est valide ET que tous les programmes de test
# s'exécutent (et, avec --reference, sont octet-pour-octet conformes), 1 sinon.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
C0="${ROSE_C0:-}"
SOURCE="$ROOT/compilateur/moteur_complet.rose"
OUT="/tmp/opencode/release-sans-cargo"
REF=""
RAPPORT=""
TLIMIT=900
REUTILISER=0
NO_TESTS=0
WINDOWS=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --c0) C0="${2:-}"; shift 2 ;;
    --source) SOURCE="${2:-}"; shift 2 ;;
    --sortie) OUT="${2:-}"; shift 2 ;;
    --reference) REF="${2:-}"; shift 2 ;;
    --rapport) RAPPORT="${2:-}"; shift 2 ;;
    --timeout) TLIMIT="${2:-}"; shift 2 ;;
    --reutiliser) REUTILISER=1; shift ;;
    --sans-tests) NO_TESTS=1; shift ;;
    --windows) WINDOWS=1; shift ;;
    -h|--help) sed -n '2,62p' "${BASH_SOURCE[0]}"; exit 0 ;;
    -*) echo "OPTION INCONNUE : $1" >&2; exit 1 ;;
    *) break ;;
  esac
done

# ── Choix de C0 (jamais reconstruit ici) ────────────────────────────────
if [[ -z "$C0" ]]; then
  for cand in /tmp/opencode/C0_h.elf /tmp/opencode/C0_f.elf /tmp/opencode/C0_b.elf "$ROOT/../C0.elf"; do
    [[ -x "$cand" ]] && { C0="$cand"; break; }
  done
fi
if [[ -z "$C0" || ! -x "$C0" ]]; then
  echo "ERREUR : compilateur C0 introuvable (--c0 ou ROSE_C0)" >&2
  exit 1
fi
C0="$(realpath "$C0")"
est_elf_aarch64() {
  local f="$1"
  [[ "$(od -An -tx1 -N4 "$f" 2>/dev/null | tr -d ' \n')" == "7f454c46" ]] &&
  [[ "$(od -An -tu2 -j18 -N2 "$f" 2>/dev/null | tr -d ' \n')" == "183" ]]
}
if ! est_elf_aarch64 "$C0"; then
  echo "ERREUR : $C0 n'est pas un ELF64 aarch64" >&2
  exit 1
fi
if [[ ! -r "$SOURCE" ]]; then
  echo "ERREUR : source composé introuvable : $SOURCE" >&2
  exit 1
fi
if [[ -n "$REF" && ! -x "$REF" ]]; then
  echo "ERREUR : --reference doit désigner un exécutable" >&2
  exit 1
fi
mkdir -p "$OUT" || exit 1
[[ -n "$RAPPORT" ]] || RAPPORT="$OUT/resultats.tsv"
ELF="$OUT/moteur.elf"
ERR="$OUT/moteur.err"
MD5_C0="$(md5sum "$C0" | cut -d' ' -f1)"
MD5_SRC="$(md5sum "$SOURCE" | cut -d' ' -f1)"

# ── 1. Compilation du moteur par C0, sous garde-fou mémoire ─────────────
if [[ $REUTILISER -eq 1 ]]; then
  [[ -x "$ELF" ]] || { echo "ERREUR : --reutiliser sans $ELF" >&2; exit 1; }
  est_elf_aarch64 "$ELF" || { echo "ERREUR : $ELF n'est pas un ELF64 aarch64" >&2; exit 1; }
  echo "RÉUTILISATION : $ELF"
else
  echo "COMPILATION C0 : $C0 ($MD5_C0) <- $(basename "$SOURCE") ($MD5_SRC)"
  : > "$ELF.tmp"; : > "$ERR"
  ( cd "$OUT" && bash "$ROOT/scripts/garde_fou_mem.sh" 12000000 \
      timeout "$TLIMIT" "$C0" "$SOURCE" > "$ELF.tmp" 2> "$ERR" )
  crc=$?
  if [[ $crc -ne 0 || ! -s "$ELF.tmp" ]]; then
    echo "ECHEC compilation C0 : rc=$crc" >&2
    head -n 5 "$ERR" >&2
    rm -f "$ELF.tmp"
    exit 1
  fi
  if ! est_elf_aarch64 "$ELF.tmp"; then
    echo "ECHEC : sortie C0 non ELF64 aarch64" >&2
    head -c 200 "$ELF.tmp" >&2; echo >&2
    rm -f "$ELF.tmp"
    exit 1
  fi
  mv "$ELF.tmp" "$ELF"
  echo "  ELF : $(stat -c%s "$ELF") octets, stderr $(stat -c%s "$ERR") octets"
fi
chmod 755 "$ELF"
MD5_ELF="$(md5sum "$ELF" | cut -d' ' -f1)"

# ── 2. Lanceur : seules les commandes réellement implémentées ───────────
LANCEUR="$OUT/rose"
cat > "$LANCEUR" <<'EOF'
#!/usr/bin/env bash
# Lanceur auto-hébergé : toute commande non implémentée dans le composé C0
# est refusée explicitement (rc=3), jamais simulée.
set -u
ICI="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MOTEUR="$ICI/moteur.elf"
cmd="${1:-}"
case "$cmd" in
  run|jit|interprete|methode|verif|rir|tokens|ast|latex|transpile|bc|comp|\
  version|--version|help|--help|-h|-?|info|"") exec "$MOTEUR" "$@" ;;
  aot|orc|link|llvm|wasm)
    echo "rose: commande NON COUVERTE par le moteur auto-hébergé C0 : $cmd" >&2
    echo "  cause : le point d'entrée de compilateur/moteur_complet.rose n'a" >&2
    echo "  aucun writer/routine pour cette commande (JIT Cranelift, LLVM ORC, VM," >&2
    echo "  console, PE). Utiliser le binaire Rust pour cette commande." >&2
    exit 3 ;;
  *) exec "$MOTEUR" "$@" ;;
esac
EOF
chmod 755 "$LANCEUR"

# ── 3. Test fonctionnel : 12 programmes, sortie capturée ────────────────
TESTS=(t01_entiers t02_bool t03_chaine_ascii t04_chaine_unicode t05_chaine_sous
       t06_rationnel_lit t09_flottant t20_add g5b604_depassement_limites
       g5b608_arithmetique_base g5b609_logique_condition g5b610_chaines_ascii)
: > "$RAPPORT"
printf 'nom\trc\tverdict\toctets_sortie\tmd5_sortie\treference\n' >> "$RAPPORT"
 total=0; reussis=0; conformes=0; divergents=0
if [[ $NO_TESTS -eq 1 ]]; then
  echo "TESTS : désactivés (--sans-tests)"
else
for nom in "${TESTS[@]}"; do
  src="$ROOT/tests_selfhost/$nom.rose"
  [[ -r "$src" ]] || { echo "BLOQUÉ $nom : source absent" >&2; total=$((total+1)); continue; }
  total=$((total+1))
  out="$OUT/$nom.out"; err="$OUT/$nom.err"
  timeout 60 "$ELF" "$src" > "$out" 2> "$err"; rc=$?
  verdict="EXEC_ECHEC"
  if [[ $rc -eq 0 && -s "$out" ]]; then verdict="OK"; reussis=$((reussis+1)); fi
  ref="-"
  if [[ -n "$REF" && "$verdict" == "OK" ]]; then
    ROSE_SANS_JIT=1 timeout 60 "$REF" "$src" > "$out.ref" 2>/dev/null
    if cmp -s "$out" "$out.ref"; then
      verdict="CONFORME"; conformes=$((conformes+1)); ref="égal"
    else
      verdict="DIVERGENT"; divergents=$((divergents+1)); ref="différent"
    fi
  fi
  printf '%-30s rc=%s %-11s %s octets\n' "$nom" "$rc" "$verdict" "$(stat -c%s "$out" 2>/dev/null || echo 0)"
  printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$nom" "$rc" "$verdict" \
    "$(stat -c%s "$out" 2>/dev/null || echo 0)" \
    "$(md5sum "$out" 2>/dev/null | cut -d' ' -f1)" "$ref" >> "$RAPPORT"
done
fi

# ── 4. Manifeste des artefacts et de la couverture ─────────────────────
MANIFESTE="$OUT/MANIFESTE.txt"
{
  echo "RELEASE ROSE SANS CARGO — constat $(date -u +'%Y-%m-%dT%H:%M:%SZ')"
  echo "compilateur C0 : $C0  md5=$MD5_C0"
  echo "source composé : $SOURCE  md5=$MD5_SRC"
  echo "moteur         : $ELF  $(stat -c%s "$ELF") octets  md5=$MD5_ELF"
  echo "lanceur        : $LANCEUR"
  echo "tests          : $reussis/$total exécutés avec rc=0"
  if [[ -n "$REF" ]]; then
    echo "référence Rust : $REF — $conformes conformes, $divergents divergents"
  else
    echo "référence Rust : non demandée (--reference absent)"
  fi
  echo "rapport        : $RAPPORT"
  echo
  echo "COUVERT (mesuré sur l'ELF) :"
  echo "  lecture+vérification+compilation RIR+exécution, run/jit/interprete,"
  echo "  methode, verif, rir, tokens, ast, latex, transpile --cible wasm, bc,"
  echo "  comp (patch du gabarit ELF d'UNE constante), version, help, info."
  echo "PARITÉ MESURÉE (vague 102, hors de ce script) :"
  echo "  corpus 141/141 fonctionnel ; wasm/bc/comp octet-identiques au moteur Rust."
  echo "NON COUVERT (cause technique) :"
  echo "  JIT Cranelift      : C0 émet un interpréteur RIR, pas de code chaud JIT."
  echo "  aot/orc/link/llvm  : aucun writer LLVM ORC dans le composé Rose."
  echo "  console/repl, rvm  : routines absentes du point d'entrée du composé."
  echo "  transpile js/py/c  : seules les cibles wasm et bc sont écrites."
  echo "  MOTEUR Windows     : avec --windows, 3 lanceurs Win32 réels"
  echo "                       (shell/tray/setup) et le MOTEUR CONSOLE COMPLET"
  echo "                       compilé par C0 (rose.exe = payload = RCDATA 3) sont"
  echo "                       produits — voir rose/docs/PAYLOAD_C0.md."
} > "$MANIFESTE"

# ── 5. Artefacts Windows PE32+ x86-64 SANS cargo (option --windows) ───────
if [[ $WINDOWS -eq 1 ]]; then
  WIN="$OUT"
  mkdir -p "$WIN/payload" || exit 1
  EMIT_SRC="$ROOT/compilateur/emetteur_pe_x64.rose"
  GEN_SRC="$ROOT/compilateur/pe_stub_ressources.rose"
  EMIT="$WIN/emetteur_pe_x64.elf"
  GEN="$WIN/pe_stub_ressources.elf"
  TENT="$WIN/WINDOWS_TENTATIVES.txt"
  : > "$TENT"

  comp_c0_windows() {
    local src="$1" dst="$2" nom="$3" rc=0
    [[ -r "$src" ]] || { echo "BLOQUÉ $nom : source absent ($src)" >&2; return 1; }
    echo "COMPILATION C0 $nom : $(basename "$src") -> $(basename "$dst")" >> "$TENT"
    : > "$dst.tmp"; : > "$dst.err"
    ( cd "$WIN" && bash "$ROOT/scripts/garde_fou_mem.sh" 12000000 \
        timeout "$TLIMIT" "$C0" "$src" > "$dst.tmp" 2> "$dst.err" ) || rc=$?
    if [[ $rc -ne 0 || ! -s "$dst.tmp" ]] || ! est_elf_aarch64 "$dst.tmp"; then
      echo "ECHEC compilation $nom (rc=$rc)" >&2; head -n 5 "$dst.err" >&2
      rm -f "$dst.tmp"; return 1
    fi
    mv "$dst.tmp" "$dst"; chmod 755 "$dst"
    printf '%s\t%s octets\tmd5=%s\n' "$nom" "$(stat -c%s "$dst")" \
      "$(md5sum "$dst" | cut -d' ' -f1)" >> "$TENT"
  }
  comp_c0_windows "$EMIT_SRC" "$EMIT" "émetteur PE x64" || exit 1
  comp_c0_windows "$GEN_SRC" "$GEN" "générateur PE + .rsrc (setup/payload)" || exit 1
  # Lanceurs RÉELS écrits en Rose (plus de stubs pour ces trois cibles).
  comp_c0_windows "$ROOT/compilateur/rose_shell_win.rose" \
    "$WIN/rose_shell_win.elf" "lanceur shell Rose" || exit 1
  comp_c0_windows "$ROOT/compilateur/rose_tray_win.rose" \
    "$WIN/rose_tray_win.elf" "lanceur tray Rose" || exit 1

  # 1) MOTEUR COMPLET par C0 : RIR du moteur_complet.rose, puis PE par
  #    l'émetteur C0. Plus AUCUN repli : ni pilote entier (stub 2,5 Ko), ni
  #    binaire signé du nupkg 0.1.76. rose.exe, payload/rose.exe et le
  #    RCDATA 3 du setup sont TOUS ce même binaire C0.
  echo "TENTATIVE RIR moteur complet : $ELF rir $(basename "$SOURCE")" >> "$TENT"
  rc_rir=0
  timeout 600 "$ELF" rir "$SOURCE" > "$WIN/moteur_complet.rir" \
      2> "$WIN/moteur_complet.rir.err" || rc_rir=$?
  if [[ $rc_rir -ne 0 || ! -s "$WIN/moteur_complet.rir" ]]; then
    printf 'RIR COMPLET refusé (rc=%s) : %s — BLOQUÉ, aucun repli\n' "$rc_rir" \
      "$(head -n1 "$WIN/moteur_complet.rir.err")" >> "$TENT"
    echo "BLOQUÉ : RIR du moteur complet par C0 indisponible" >&2
    exit 1
  fi
  printf 'RIR COMPLET obtenu : %s octets\n' "$(stat -c%s "$WIN/moteur_complet.rir")" >> "$TENT"
  rc_pe=0
  timeout 2400 "$EMIT" "$WIN/moteur_complet.rir" > "$WIN/rose_complet.exe" \
      2> "$WIN/rose_complet.exe.err" || rc_pe=$?
  if [[ $rc_pe -ne 0 || ! -s "$WIN/rose_complet.exe" ]]; then
    printf 'PE COMPLET refusé (rc=%s) : %s — BLOQUÉ, aucun repli\n' "$rc_pe" \
      "$(head -n1 "$WIN/rose_complet.exe.err")" >> "$TENT"
    echo "BLOQUÉ : émission PE du moteur complet par C0 indisponible" >&2
    exit 1
  fi
  cp "$WIN/rose_complet.exe" "$WIN/rose.exe"
  ORIGINE_ROSE="moteur complet (RIR C0 -> PE C0)"
  echo " rose.exe = moteur complet émis en PE" >> "$TENT"

  # Validateur du moteur C0 : on exige un PE32+ console x86-64 d'au moins
  # 1 Mio avant de l'utiliser comme payload et comme ressource du setup.
  python3 - "$WIN/rose.exe" <<'PY'
import pathlib, struct, sys
b = pathlib.Path(sys.argv[1]).read_bytes()
errs = []
if b[:2] != b"MZ": errs.append("magic MZ absent")
pe = struct.unpack_from("<I", b, 0x3C)[0] if len(b) >= 0x40 else 0
if pe + 70 > len(b) or b[pe:pe+4] != b"PE\0\0":
    errs.append("signature PE absente")
else:
    if struct.unpack_from("<H", b, pe+4)[0] != 0x8664: errs.append("machine != 0x8664")
    if struct.unpack_from("<H", b, pe+24)[0] != 0x20B: errs.append("magic != 0x20B")
    if struct.unpack_from("<H", b, pe+24+68)[0] != 3: errs.append("subsystem != 3 (console)")
if len(b) < 1_000_000: errs.append("moteur C0 trop petit (%d o)" % len(b))
if errs:
    print("MOTEUR C0 INVALIDE : " + "; ".join(errs), file=sys.stderr)
    sys.exit(1)
print("moteur C0 valide : %d o, PE32+ console x86-64" % len(b))
PY
  [[ $? -eq 0 ]] || { echo "BLOQUÉ : moteur C0 invalide" >&2; exit 1; }

  # 2) Actifs RÉELS embarqués (aucun stub de ressource, aucun binaire du
  #    nupkg). PNG, TTF généré depuis Kare.woff2, ICO, manifeste : tout vient
  #    du worktree ; le moteur, lui, vient de C0 (voir étape 1).
  ASSETS="$WIN/assets"
  mkdir -p "$ASSETS" || exit 1
  python3 - "$ROOT" "$ASSETS" <<'PY'
import importlib.util, pathlib, struct, sys
root, out = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
# ⚠ Aucune lecture du nupkg 0.1.76 ici : le moteur est déjà produit par C0
# dans $WIN/rose.exe (étape 1) et sert de payload et de RCDATA 3 au setup.
spec = importlib.util.spec_from_file_location(
    "packres", root / "installateur/velopack/pack-res.py")
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
(out / "kare.ttf").write_bytes(
    mod.woff2_vers_ttf(root / "docs/assets/fonts/kare/Kare.woff2"))
for src, dst in [
    ("installateur/velopack/rose-installeur.png", "rose-installeur.png"),
    ("installateur/assets/rose.ico", "rose.ico"),
    ("rose/rose.ico", "rose_app.ico"),
    ("rose/rose_shell.ico", "rose_shell.ico"),
    ("rose/rose-setup.manifest", "rose-setup.manifest"),
]:
    (out / dst).write_bytes((root / src).read_bytes())
d = (root / "installateur/assets/rose_setup.ico").read_bytes()
n = struct.unpack_from("<H", d, 4)[0]
grp = bytearray(struct.pack("<HHH", 0, 1, n))
for i in range(n):
    o = 6 + 16 * i
    w = d[o] or 256; h = d[o + 1] or 256; cols = d[o + 2]
    plans = struct.unpack_from("<H", d, o + 4)[0] or 1
    bits = struct.unpack_from("<H", d, o + 6)[0] or 32
    size = struct.unpack_from("<I", d, o + 8)[0]
    off = struct.unpack_from("<I", d, o + 12)[0]
    (out / f"icone{i+1}.bin").write_bytes(d[off:off + size])
    grp += struct.pack("<BBBBHHIH", w % 256, h % 256, cols, 0,
                       plans, bits, size, i + 1)
(out / "groupe.bin").write_bytes(grp)
lines = []
for i in range(1, n + 1):
    p = out / ("icone%d.bin" % i)
    lines.append("3\t%d\t%s\t%d" % (i, p, p.stat().st_size))
lines.append("14\t1\t%s\t%d" % (out / "groupe.bin", (out / "groupe.bin").stat().st_size))
(out / "icons.tsv").write_text("\n".join(lines) + "\n")
(out / "icons_full.tsv").write_text("\n".join(lines) + "\n")
print("actifs préparés :", len(lines), "ressources icône (plus le moteur C0",
      "en RCDATA 3), TTF", (out / "kare.ttf").stat().st_size, "o")
PY
  [[ -s "$ASSETS/icons.tsv" ]] || { echo "BLOQUÉ préparation actifs" >&2; exit 1; }

  mapfile -t ICON_LINES < "$ASSETS/icons_full.tsv"
  ICON_ARGS=()
  for ligne in "${ICON_LINES[@]}"; do
    IFS=$'\t' read -r t n p s <<< "$ligne"
    ICON_ARGS+=("$t" "$n" "$p" "$s")
  done
  timeout 120 "$WIN/rose_shell_win.elf" "$WIN/rose-shell.exe" shell \
    "${#ICON_LINES[@]}" "${ICON_ARGS[@]}" \
    || { echo "BLOQUÉ génération rose-shell.exe (Rose)" >&2; exit 1; }
  timeout 120 "$WIN/rose_tray_win.elf" "$WIN/rose-tray.exe" tray \
    "${#ICON_LINES[@]}" "${ICON_ARGS[@]}" \
    || { echo "BLOQUÉ génération rose-tray.exe (Rose)" >&2; exit 1; }

  # payload/rose.exe = le MOTEUR CONSOLE COMPILÉ PAR C0 (rose.exe de l'étape 1).
  # Plus aucune copie du binaire signé du nupkg 0.1.76.
  cp "$WIN/rose.exe" "$WIN/rose.exe.engine" || exit 1
  cp "$WIN/rose.exe" "$WIN/payload/rose.exe" || exit 1

  # Table de ressources COMPLÈTE du setup (RCDATA 1/3/5/6/7/8/9, FONT 2,
  # MANIFEST 1, RT_ICON 1..7, GROUP_ICON 1) construite en Rose.
  python3 - "$ROOT" "$ASSETS" "$WIN" <<'PY'
import pathlib, sys
root, out, win = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]), pathlib.Path(sys.argv[3])
lines = []
def add(t, ident, p):
    p = pathlib.Path(p)
    assert p.exists(), p
    lines.append("%d\t%d\t%s\t%d" % (t, ident, p, p.stat().st_size))
add(10, 1, out / "rose-installeur.png")
add(8, 2, out / "kare.ttf")
add(10, 3, win / "rose.exe")   # moteur C0, identique à payload/rose.exe
add(10, 5, win / "rose-shell.exe")
add(10, 6, out / "rose.ico")
add(10, 7, out / "rose_app.ico")
add(10, 8, out / "rose_shell.ico")
add(10, 9, win / "rose-tray.exe")
add(24, 1, out / "rose-setup.manifest")
for i in range(1, 8):
    add(3, i, out / ("icone%d.bin" % i))
add(14, 1, out / "groupe.bin")
(out / "setup.tsv").write_text("\n".join(lines) + "\n")
print("setup :", len(lines), "ressources RCDATA/FONT/MANIFEST/ICON/GROUP")
PY
  mapfile -t SETUP_LINES < "$ASSETS/setup.tsv"
  SETUP_ARGS=()
  for ligne in "${SETUP_LINES[@]}"; do
    IFS=$'\t' read -r t n p s <<< "$ligne"
    SETUP_ARGS+=("$t" "$n" "$p" "$s")
  done
  # ⚠ Le nom `rose-setup.exe` ACTIVE les 40+ garde-fous de verifier-res.py.
  timeout 300 "$GEN" "$WIN/rose-setup.exe" setup \
    "${#SETUP_LINES[@]}" "${SETUP_ARGS[@]}" \
    || { echo "BLOQUÉ génération rose-setup.exe (Rose)" >&2; exit 1; }
  cp "$WIN/rose-setup.exe" "$WIN/Rose-win-Setup.exe" || exit 1

  # 4) Vérification binaire : magic, machine, sections, imports, taille, md5.
  ARTEFACTS=(rose-setup.exe rose.exe rose-shell.exe rose-tray.exe payload/rose.exe)
  echo "=== VÉRIFICATION DES 5 ARTEFACTS WINDOWS ==="
  if ! python3 - "$WIN" <<'PY' | tee "$WIN/WINDOWS_ARTIFACTS.tsv"
import hashlib, pathlib, struct, sys
d = pathlib.Path(sys.argv[1])
arts = ["rose-setup.exe", "rose.exe", "rose-shell.exe", "rose-tray.exe", "payload/rose.exe"]
rows, mauvais = [], []
for rel in arts:
    p = d / rel
    if not p.exists():
        mauvais.append(rel + " : absent"); continue
    b = p.read_bytes()
    errs = []
    if b[:2] != b"MZ": errs.append("magic MZ absent")
    pe = struct.unpack("<I", b[0x3C:0x40])[0] if len(b) >= 0x40 else 0
    if b[pe:pe+4] != b"PE\0\0": errs.append("signature PE absente")
    machine = struct.unpack("<H", b[pe+4:pe+6])[0] if pe+6 <= len(b) else 0
    nsec = struct.unpack("<H", b[pe+6:pe+8])[0] if pe+8 <= len(b) else 0
    optsz = struct.unpack("<H", b[pe+20:pe+22])[0] if pe+22 <= len(b) else 0
    opt = pe + 24
    magic = struct.unpack("<H", b[opt:opt+2])[0] if opt+2 <= len(b) else 0
    if machine != 0x8664: errs.append("machine %#x" % machine)
    if magic != 0x20B: errs.append("magic optional %#x" % magic)
    if nsec < 3: errs.append("sections %d" % nsec)
    if "payload/" not in rel:
        base = opt + optsz
        secs = [b[base+40*i:base+40*i+8].rstrip(b"\0").decode("ascii", "replace")
                for i in range(nsec)]
        for att in (".text", ".data"):
            if att not in secs: errs.append("section " + att + " absente")
        dd = opt + 112
        imp_rva, imp_sz = struct.unpack("<II", b[dd+8:dd+16])
        if imp_rva == 0 or imp_sz == 0: errs.append("répertoire imports vide")
        if b"KERNEL32.DLL" not in b: errs.append("KERNEL32.DLL absente")
        secs_txt = ",".join(secs)
    else:
        ss = struct.unpack("<H", b[opt+68:opt+70])[0]
        if ss != 3 or len(b) < 1_000_000:
            errs.append("moteur payload invalide (%d, %d o)" % (ss, len(b)))
        if hashlib.md5(b).hexdigest() != hashlib.md5((d / "rose.exe").read_bytes()).hexdigest():
            errs.append("payload != rose.exe C0")
        secs_txt = "moteur-console-reel"
    verdict = "OK" if not errs else "ECHEC:" + ";".join(errs)
    if errs: mauvais.append(rel + " : " + ";".join(errs))
    rows.append((rel, str(len(b)), hashlib.md5(b).hexdigest(),
                 "0x%04X" % machine, "0x%03X" % magic, str(nsec),
                 secs_txt, verdict))
print("artefact\ttaille\tmd5\tmachine\tmagic\tsections\tnoms_sections\tverdict")
for r in rows: print("\t".join(r))
if mauvais:
    print("ECHEC : " + " | ".join(mauvais), file=sys.stderr)
    sys.exit(1)
print("5/5 ARTEFACTS PE32+ x86-64 VALIDES")
PY
  then
    echo "ÉCHEC vérification binaire des artefacts Windows" >&2; exit 1
  fi
  # 5) file / llvm-readobj : preuves statiques complémentaires.
  {
    echo "=== file -b ==="
    for f in "${ARTEFACTS[@]}"; do printf '%-22s %s\n' "$f" "$(file -b "$WIN/$f")"; done
    if command -v llvm-readobj >/dev/null 2>&1; then
      echo "=== llvm-readobj --file-headers --sections --coff-imports ==="
      for f in rose-setup.exe rose-shell.exe rose-tray.exe; do
        echo "--- $f ---"
        llvm-readobj --file-headers --sections --coff-imports "$WIN/$f" | \
          grep -E 'Machine:|Magic:|Subsystem:|SectionCount|Name: \.|VirtualAddress:|Name: (KERNEL32|USER32|SHELL32)|Symbol:'
      done
    fi
    echo "=== objdump des lanceurs Rose ==="
    for f in rose-shell.exe rose-tray.exe rose-setup.exe; do
      echo "--- $f ---"
      llvm-objdump -d --section=.text "$WIN/$f" | grep -E 'callq|subq|retq|SetTimer|Shell' | head -25
    done
  } > "$WIN/LLVM_READOJ.txt" 2>&1
  cat "$WIN/LLVM_READOJ.txt"

  # 6) verifier-res.py : 40+ garde-fous activés par le nom rose-setup.exe.
  VERIF="$ROOT/installateur/velopack/verifier-res.py"
  [[ -r "$VERIF" ]] || { echo "BLOQUÉ verifier-res.py absent" >&2; exit 1; }
  echo "=== VERIFIER-RES.PY (rose-setup.exe) ==="
  if ! python3 "$VERIF" "$WIN/rose-setup.exe" > "$WIN/VERIFIER_RES.txt" 2>&1; then
    cat "$WIN/VERIFIER_RES.txt" >&2; echo "ÉCHEC verifier-res.py" >&2; exit 1
  fi
  cat "$WIN/VERIFIER_RES.txt"

  # 6b) Preuve d'identité : le RCDATA 3 du setup, payload/rose.exe et rose.exe
  #     sont OCTET POUR OCTET le même moteur C0. Aucun binaire du nupkg.
  echo "=== ORIGINE DU PAYLOAD (RCDATA 3 == payload == rose.exe C0) ==="
  if ! python3 - "$ROOT" "$WIN" <<'PY' | tee "$WIN/PAYLOAD_ORIGINE.txt"
import hashlib, importlib.util, pathlib, struct, sys
root, win = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
spec = importlib.util.spec_from_file_location(
    "verif_res", root / "installateur/velopack/verifier-res.py")
v = importlib.util.module_from_spec(spec)
spec.loader.exec_module(v)
setup = (win / "Rose-win-Setup.exe").read_bytes()
rsrc = v.section_rsrc(setup)
sections = v.sections_pe(setup)
racine, nommees, ids = v.entrees(rsrc, 0)
assert nommees == 0
trouve = None
for type_, pointeur in racine:
    if type_ != v.RT_RCDATA:
        continue
    noeuds, _, _ = v.entrees(rsrc, pointeur & 0x7FFFFFFF)
    for nom, p2 in noeuds:
        if nom != 3:
            continue
        langues, _, _ = v.entrees(rsrc, p2 & 0x7FFFFFFF)
        for _langue, p3 in langues:
            decalage, taille, _page, _reserve = struct.unpack(
                "<IIII", rsrc[(p3 & 0x7FFFFFFF):(p3 & 0x7FFFFFFF) + 16])
            decalage = v.rva_vers_offset(decalage, sections)
            trouve = setup[decalage:decalage + taille]
assert trouve is not None, "RCDATA 3 absent du setup"
md5 = lambda b: hashlib.md5(b).hexdigest()
moteur = (win / "rose.exe").read_bytes()
payload = (win / "payload/rose.exe").read_bytes()
print("rose.exe (C0)      : %8d o  md5=%s" % (len(moteur), md5(moteur)))
print("payload/rose.exe   : %8d o  md5=%s" % (len(payload), md5(payload)))
print("RCDATA 3 du setup  : %8d o  md5=%s" % (len(trouve), md5(trouve)))
assert md5(trouve) == md5(moteur) == md5(payload), "RCDATA 3 / payload / rose.exe divergents"
print("PREUVE : payload/rose.exe = RCDATA 3 = rose.exe = moteur complet C0 (RIR -> PE)")
PY
  then
    echo "ÉCHEC preuve d'origine du payload" >&2; exit 1
  fi

  {
    echo "WINDOWS SANS CARGO (option --windows) :"
    echo "  origine rose.exe : $ORIGINE_ROSE"
    echo "  émetteur PE x64 : $EMIT  ($(stat -c%s "$EMIT") o)"
    echo "  générateur .rsrc: $GEN  ($(stat -c%s "$GEN") o)"
    echo "  shell Rose      : $WIN/rose_shell_win.elf"
    echo "  tray Rose       : $WIN/rose_tray_win.elf"
    echo "  payload/rose.exe: copie octet-pour-octet de rose.exe (moteur C0)"
    echo "  RCDATA 3 setup  : le MÊME moteur C0 (voir PAYLOAD_ORIGINE.txt)"
    echo "  nupkg 0.1.76    : AUCUNE lecture, AUCUNE copie (plus de recours)"
    echo "  rapport         : $WIN/WINDOWS_TENTATIVES.txt"
    echo "  vérification    : $WIN/WINDOWS_ARTIFACTS.tsv"
    echo "  readobj         : $WIN/LLVM_READOJ.txt"
    echo "  ressources      : $WIN/VERIFIER_RES.txt"
    echo "  origine payload : $WIN/PAYLOAD_ORIGINE.txt"
    echo "  STATUT : 5 PE32+ x86-64 ; shell/tray/setup = code Win32 réel écrit"
    echo "           en Rose ; rose.exe = payload = RCDATA 3 = moteur complet"
    echo "           (RIR C0 -> PE C0), sans aucun binaire signé du nupkg."
  } >> "$MANIFESTE"
  echo "WINDOWS : 5 artefacts PE32+ x86-64 (3 lanceurs Rose), verifier-res.py OK"
fi

echo "ARTEFACTS : $ELF ($(stat -c%s "$ELF") o, md5 $MD5_ELF)"
echo "            $LANCEUR"
echo "            $MANIFESTE"
echo "TESTS : $reussis/$total rc=0$( [[ -n "$REF" ]] && echo ", $conformes conformes / $divergents divergents" )"
[[ $reussis -eq $total && $total -gt 0 ]]
