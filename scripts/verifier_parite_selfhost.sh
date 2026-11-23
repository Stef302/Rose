#!/usr/bin/env bash
# verifier_parite_selfhost.sh — harnais de parité REPRODUCTIBLE du jalon self-hosting.
#
#   bash scripts/verifier_parite_selfhost.sh [options]
#
# Mesure : 141 sources tests_selfhost/*.rose exécutées deux fois, une par
#   l'ORACLE RUST   : ROSE_SANS_JIT=1 rose/rose SOURCE.rose [args]
#   et une par le MOTEUR SELF-HOSTÉ : moteur_complet.elf SOURCE.rose [args]
# Comparaison des rc et des octets de stdout (jamais stderr, qui contient les
# diagnostics propres à chaque moteur).
#
# Classes (esprit de tests_selfhost/lancer.sh, adapté à deux exécuteurs) :
#   CONFORME  les deux rc=0 et stdout octet pour octet identique
#   DIVERGENT rcn!=0 et rcs=0 : Rose accepte un programme que Rust refuse
#             OU les deux rc=0 mais stdout différent
#   BLOQUE    rcs!=0 et rcn!=0 : les deux moteurs refusent le programme
#             OU timeout / artefact manquant des deux côtés
# Un cas rcn=0 et rcs!=0 est BLOQUE avec la note `rust_accepte_self_refuse`.
#
# VERDICT (code de sortie) : 0 si, sur TOUS les programmes que Rust accepte
# (rcn=0), Rose self-hosté rend le même rc ET les mêmes octets ; 1 sinon.
# Les 5 DIVERGENTS historiques (t104/t105 : surcharges refusées par Rust ;
# t83/t84/t86 : fichiers externes absents) et les 3 BLOQUES historiques
# (t55/t106/t107 : refus des deux côtés) ne sont DONC PAS des erreurs Rose.
#
# Options :
#   --workdir DIR     artefacts (défaut /tmp/opencode/parite_selfhost_<UTC>_<PID>)
#   --moteur ELF      ELF du moteur self-hosté (sinon découverte, puis compilation)
#   --c0 ELF          compilateur C0 à utiliser pour la compilation (sinon découverte)
#   --rose-bin BIN    binaire Rust de référence (sinon découverte)
#   --timeout N       timeout par exécution en secondes (défaut 30)
#   --filtre MOT      ne jouer que les sources dont le nom contient MOT
#   --sans-fixtures   ne pas créer les fixtures SH9 (rejouer l'état « fichiers absents »)
#   --sans-args       ne pas passer critique_entree.txt à t80/t81/t999
#   --forcer-build    reconstruire le moteur même si un ELF valide est trouvé
#   --regen-source    régénérer compilateur/moteur_complet.rose via
#                     scripts/build_moteur_sans_rust.sh --source-only AVANT C0
#                     (ATTENTION : écrase les patchs appliqués après génération)
#
# Variables d'environnement acceptées : PARITE_MOTEUR, PARITE_C0, PARITE_ROSE_BIN,
# PARITE_TIMEOUT, PARITE_OUT (dossier des artefacts). Un TSV horodaté est toujours
# écrit dans le workdir ; une copie stable est laissée dans /tmp/opencode/.
# Aucun fichier source suivi par git n'est modifié (sauf --regen-source explicite).
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TESTDIR="$ROOT/tests_selfhost"
GARDE_FOU="$ROOT/scripts/garde_fou_mem.sh"
BUILD="$ROOT/scripts/build_moteur_sans_rust.sh"
COMPOSITE="$ROOT/compilateur/moteur_complet.rose"
ROSE_SOURCE="$ROOT/compilateur/rose_natif.rose"

WORKDIR_OPT=""
MOTEUR_OPT="${PARITE_MOTEUR:-}"
C0_OPT="${PARITE_C0:-}"
ROSE_OPT="${PARITE_ROSE_BIN:-}"
TIMEOUT_OPT="${PARITE_TIMEOUT:-30}"
FILTRE=""
FORCER=0
REGEN=0
FIXTURES=1
ARGS=1
while [[ $# -gt 0 ]]; do
  case "$1" in
    --workdir) WORKDIR_OPT="${2:-}"; shift 2 ;;
    --moteur) MOTEUR_OPT="${2:-}"; shift 2 ;;
    --c0) C0_OPT="${2:-}"; shift 2 ;;
    --rose-bin) ROSE_OPT="${2:-}"; shift 2 ;;
    --timeout) TIMEOUT_OPT="${2:-}"; shift 2 ;;
    --filtre) FILTRE="${2:-}"; shift 2 ;;
    --sans-fixtures) FIXTURES=0; shift ;;
    --sans-args) ARGS=0; shift ;;
    --forcer-build) FORCER=1; shift ;;
    --regen-source) REGEN=1; shift ;;
    -h|--help) sed -n '2,44p' "${BASH_SOURCE[0]}"; exit 0 ;;
    -*) echo "OPTION INCONNUE : $1" >&2; exit 1 ;;
    *) break ;;
  esac
done

TS="$(date -u +'%Y%m%dT%H%M%SZ')"
OUT="${WORKDIR_OPT:-${PARITE_OUT:-/tmp/opencode/parite_selfhost_${TS}_$$}}"
mkdir -p "$OUT" || exit 1
cd "$ROOT" || exit 1

is_elf() {
  [[ -s "$1" ]] && [[ "$(od -An -tx1 -N4 "$1" 2>/dev/null | tr -d ' \n')" == "7f454c46" ]] \
    && [[ "$(od -An -tu2 -j18 -N2 "$1" 2>/dev/null | tr -d ' \n')" == "183" ]]
}
md5() { md5sum "$1" 2>/dev/null | cut -d' ' -f1; }
head_err() { head -n 1 "$1" 2>/dev/null | head -c 160; }

# ── 1. oracle Rust ───────────────────────────────────────────────────────────
if [[ -n "$ROSE_OPT" ]]; then
  ROSE_BIN="$ROSE_OPT"
elif [[ -x "$ROOT/rose/rose" ]]; then
  ROSE_BIN="$ROOT/rose/rose"
elif [[ -x /home/clausake/Stefan/ROSE/rose/rose ]]; then
  ROSE_BIN=/home/clausake/Stefan/ROSE/rose/rose
elif [[ -x /home/clausake/Stefan/ROSE/rose/target/release/rose ]]; then
  ROSE_BIN=/home/clausake/Stefan/ROSE/rose/target/release/rose
else
  echo "ERREUR : binaire Rust de référence introuvable (--rose-bin / PARITE_ROSE_BIN)" >&2
  exit 1
fi
if [[ ! -x "$ROSE_BIN" ]]; then echo "ERREUR : $ROSE_BIN non exécutable" >&2; exit 1; fi

# ── 2. fixtures externes (idempotentes) ─────────────────────────────────────
FIXED=/tmp/opencode/sh9_agents/battery
mkdir -p "$FIXED" /tmp/aotwork || exit 1
if [[ $FIXTURES -eq 1 ]]; then
  printf 'input SH9\n' > "$FIXED/input.txt"
  printf 'fonction principale : test SH9\n' > "$FIXED/critique_entree.txt"
  printf 'BONJOUR\n' > /tmp/aotwork/entree.txt
  : > "$FIXED/out.txt"; : > "$FIXED/out2.txt"
  echo "[FIXTURES] $FIXED/{input.txt,critique_entree.txt,out.txt,out2.txt}, /tmp/aotwork/entree.txt"
else
  echo "[FIXTURES] désactivées (--sans-fixtures) : les tests de fichiers externes échoueront des deux côtés"
fi

# ── 3. point d'entrée du générateur (fichier hors dépôt, souvent perdu) ─────
# build_moteur_sans_rust.sh --source-only le lit à cette adresse exacte. On le
# reconstruit une seule fois depuis le composite SUIVI, jamais depuis un ELF.
if [[ ! -s /tmp/opencode/point_entree_complet.txt ]]; then
  awk '/^# ══════════════════ POINT D.ENTRÉE UNIQUE/{f=1} f' "$COMPOSITE" \
    > /tmp/opencode/point_entree_complet.txt
fi

# ── 4. C0 : découverte puis, à défaut, émission depuis rose_natif.rose ──────
trouver_c0() {
  local candidat
  for candidat in "${C0_OPT:-}" "$OUT/C0.elf"; do
    [[ -n "$candidat" && -x "$candidat" ]] && is_elf "$candidat" && { echo "$candidat"; return 0; }
  done
  for candidat in $(ls -t /tmp/opencode/C0*.elf 2>/dev/null); do
    [[ -x "$candidat" ]] && is_elf "$candidat" && { echo "$candidat"; return 0; }
  done
  return 1
}

MOTEUR="$MOTEUR_OPT"
if [[ -z "$MOTEUR" && $FORCER -eq 0 ]]; then
  if [[ -x "$OUT/moteur_complet.elf" ]] && is_elf "$OUT/moteur_complet.elf"; then
    MOTEUR="$OUT/moteur_complet.elf"
  else
    for candidat in $(ls -t /tmp/opencode/moteur*.elf 2>/dev/null); do
      if [[ -x "$candidat" ]] && is_elf "$candidat"; then MOTEUR="$candidat"; break; fi
    done
  fi
fi

# ── 5. compilation du moteur self-hosté si absent ────────────────────────────
if [[ -z "$MOTEUR" || $FORCER -eq 1 ]]; then
  C0="$(trouver_c0 || true)"
  if [[ -z "$C0" ]]; then
    echo "[C0] aucun C0 valide ; émission depuis compilateur/rose_natif.rose par l'oracle Rust"
    C0="$OUT/C0.elf"
    if ! bash "$GARDE_FOU" 12000000 "$ROSE_BIN" "$ROSE_SOURCE" > "$C0" 2> "$OUT/C0.err"; then
      echo "ERREUR : émission de C0 impossible : $(head_err "$OUT/C0.err")" >&2; exit 1
    fi
    chmod 755 "$C0"
  fi
  echo "[C0] $C0 (md5 $(md5 "$C0"))"

  if [[ $REGEN -eq 1 || ! -s "$COMPOSITE" ]]; then
    echo "[SOURCE] régénération via scripts/build_moteur_sans_rust.sh --source-only"
    bash "$BUILD" --source-only || exit 1
  else
    echo "[SOURCE] $COMPOSITE (md5 $(md5 "$COMPOSITE")) — patchs post-génération conservés"
  fi

  MOTEUR="$OUT/moteur_complet.elf"
  : > "$MOTEUR"
  echo "[ELF] compilation C0 -> $MOTEUR (garde-fou 12 Go, timeout 900 s)"
  if ! timeout 900 bash "$GARDE_FOU" 12000000 "$C0" "$COMPOSITE" \
        > "$MOTEUR" 2> "$OUT/moteur_complet.err"; then
    echo "ERREUR : compilation du moteur impossible : $(head_err "$OUT/moteur_complet.err")" >&2; exit 1
  fi
  chmod 755 "$MOTEUR"
fi

if [[ ! -x "$MOTEUR" ]] || ! is_elf "$MOTEUR"; then
  echo "ERREUR : moteur self-hosté invalide : $MOTEUR" >&2; exit 1
fi
echo "[MOTEUR] $MOTEUR (md5 $(md5 "$MOTEUR"), $(stat -c%s "$MOTEUR") octets)"
echo "[ORACLE] $ROSE_BIN (md5 $(md5 "$ROSE_BIN"))"

# ── 6. batterie 141 sources ──────────────────────────────────────────────────
TSV="$OUT/parite_${TS}.tsv"
printf 'nom\tclasse\trc_rust\trc_self\tmd5_rust\tmd5_self\terreur_selfhost\tnote\n' > "$TSV"
: > "$OUT/_div.txt"; : > "$OUT/_blk.txt"

conforme=0; divergent=0; bloque=0; erreurs=0; total=0
debut="$(date +%s)"
for prog in "$TESTDIR"/*.rose; do
  nom="$(basename "$prog" .rose)"
  [[ -n "$FILTRE" && "$nom" != *"$FILTRE"* ]] && continue
  total=$((total+1))
  base="$OUT/$nom"
  args=()
  if [[ $ARGS -eq 1 ]]; then
    case "$nom" in
      t80_*|t81_*|t999_*) args=("$FIXED/critique_entree.txt") ;;
    esac
  fi

  ROSE_SANS_JIT=1 timeout "$TIMEOUT_OPT" "$ROSE_BIN" "$prog" "${args[@]}" \
    > "$base.nat" 2> "$base.nat.err"; rcn=$?
  timeout "$TIMEOUT_OPT" "$MOTEUR" "$prog" "${args[@]}" \
    > "$base.self" 2> "$base.self.err"; rcs=$?

  md5n="$(md5 "$base.nat")"; md5s="$(md5 "$base.self")"
  err=0; note=""
  if [[ $rcn -ne 0 && $rcs -ne 0 ]]; then
    classe=BLOQUE; bloque=$((bloque+1)); note="refus_des_deux(rc=$rcn/$rcs)"
  elif [[ $rcn -ne 0 && $rcs -eq 0 ]]; then
    classe=DIVERGENT; divergent=$((divergent+1)); note="rust_refuse(rc=$rcn)"
  elif [[ $rcn -eq 0 && $rcs -ne 0 ]]; then
    classe=BLOQUE; bloque=$((bloque+1)); err=1; note="rust_accepte_self_refuse(rc=$rcs)"
  elif ! cmp -s "$base.nat" "$base.self"; then
    classe=DIVERGENT; divergent=$((divergent+1)); err=1; note="sortie"
  else
    classe=CONFORME; conforme=$((conforme+1))
  fi
  [[ $err -eq 1 ]] && erreurs=$((erreurs+1))

  # Contrôle de déterminisme (esprit lancer.sh) sur les cas non conformes.
  if [[ "$classe" != CONFORME ]]; then
    ROSE_SANS_JIT=1 timeout "$TIMEOUT_OPT" "$ROSE_BIN" "$prog" "${args[@]}" \
      > "$base.nat2" 2> /dev/null; rcn2=$?
    timeout "$TIMEOUT_OPT" "$MOTEUR" "$prog" "${args[@]}" \
      > "$base.self2" 2> /dev/null; rcs2=$?
    [[ $rcn -ne $rcn2 || $rcs -ne $rcs2 ]] && note="${note},rc_instable"
    if [[ $rcn -eq 0 && $rcn2 -eq 0 ]] && ! cmp -s "$base.nat" "$base.nat2"; then
      note="${note},rust_non_deterministe"
    fi
    if [[ $rcs -eq 0 && $rcs2 -eq 0 ]] && ! cmp -s "$base.self" "$base.self2"; then
      note="${note},self_non_deterministe"; err=1; erreurs=$((erreurs+1))
    fi
  fi

  if [[ "$classe" == DIVERGENT ]]; then
    printf '  %-26s %-38s nat=%s | self=%s\n' "$nom" "$note" \
      "$(tr '\n' '|' < "$base.nat" | head -c 90)" \
      "$(tr '\n' '|' < "$base.self" | head -c 90)" >> "$OUT/_div.txt"
  elif [[ "$classe" == BLOQUE ]]; then
    printf '  %-26s %-38s %s%s\n' "$nom" "$note" \
      "$(head_err "$base.nat.err")" "$(head_err "$base.self.err")" >> "$OUT/_blk.txt"
  fi
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$nom" "$classe" "$rcn" "$rcs" "$md5n" "$md5s" "$err" "$note" >> "$TSV"
done
fin="$(date +%s)"

RAPPORT="$OUT/rapport.txt"
{
  echo "BATTERIE DE PARITÉ SELF-HOST — $(date -u +'%Y-%m-%dT%H:%M:%SZ') — durée $((fin-debut)) s"
  echo "moteur self-hosté : $MOTEUR (md5 $(md5 "$MOTEUR"))"
  echo "oracle Rust       : $ROSE_BIN (md5 $(md5 "$ROSE_BIN"))"
  echo "fixtures externes : $FIXTURES   arguments t80/t81/t999 : $ARGS"
  echo "comparaison       : rc + stdout octet pour octet (stderr = diagnostic, non comparé)"
  echo "tests=$total  CONFORMES=$conforme  DIVERGENTS=$divergent  BLOQUES=$bloque"
  echo "ERREURS self-host sur programme accepté par Rust = $erreurs"
  echo
  echo "=== DIVERGENCES ==="; cat "$OUT/_div.txt"; [[ -s "$OUT/_div.txt" ]] || echo "  (aucune)"
  echo
  echo "=== BLOQUES ==="; cat "$OUT/_blk.txt"; [[ -s "$OUT/_blk.txt" ]] || echo "  (aucun)"
  echo
  echo "TSV : $TSV"
  echo "Artefacts : $OUT"
} > "$RAPPORT"
cat "$RAPPORT"

# Copie stable du dernier TSV (le nom horodaté reste la preuve).
cp "$TSV" /tmp/opencode/parite_selfhost_last.tsv
cp "$RAPPORT" /tmp/opencode/parite_selfhost_last.txt

if [[ $erreurs -eq 0 ]]; then
  echo "VERDICT : 0 cas où Rose self-hosté se trompe sur un programme que Rust accepte — RC=0"
  exit 0
fi
echo "VERDICT : $erreurs cas où Rose self-hosté se trompe sur un programme que Rust accepte — RC=1"
exit 1
