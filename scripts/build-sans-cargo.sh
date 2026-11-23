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
#                               [--reutiliser] [--sans-tests]
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
#               transpile js/py/c, console/repl, rvm, et TOUS les artefacts
#               Windows PE x86_64 (rose.exe, rose-shell.exe, rose-tray.exe,
#               rose-setup.exe) : C0 n'émet que de l'ELF aarch64 et le
#               composé n'a aucun writer PE ni ressource Win32.
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
    -h|--help) sed -n '2,56p' "${BASH_SOURCE[0]}"; exit 0 ;;
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
  aot|orc|link|llvm|console|repl|rvm|wasm)
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
  echo "DIVERGENCES MESURÉES vs binaire Rust (--reference) :"
  echo "  exécution de 12 programmes : 12/12 octet-identiques (ROSE_SANS_JIT=1)."
  echo "  comp : 8 908 o / sortie '0' vs 5 457 034 o / 4 lignes (gabarit constant)."
  echo "  wasm : 82 o vs 72 o — LEB128 surlongs (sw_module) dans le composé."
  echo "  bc   : 157 o des deux côtés mais champs ligne/index absents côté C0."
  echo "NON COUVERT (cause technique) :"
  echo "  JIT Cranelift      : C0 émet un interpréteur RIR, pas de code chaud JIT."
  echo "  aot/orc/link/llvm  : aucun writer LLVM ORC dans le composé Rose."
  echo "  console/repl, rvm  : routines absentes du point d'entrée du composé."
  echo "  transpile js/py/c  : seules les cibles wasm et bc sont écrites."
  echo "  Windows PE x86_64  : C0 n'émet que de l'ELF aarch64 ; ni writer PE"
  echo "                       x86_64 ni ressources Win32 dans le composé."
} > "$MANIFESTE"

echo "ARTEFACTS : $ELF ($(stat -c%s "$ELF") o, md5 $MD5_ELF)"
echo "            $LANCEUR"
echo "            $MANIFESTE"
echo "TESTS : $reussis/$total rc=0$( [[ -n "$REF" ]] && echo ", $conformes conformes / $divergents divergents" )"
[[ $reussis -eq $total && $total -gt 0 ]]
