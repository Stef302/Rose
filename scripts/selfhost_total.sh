#!/usr/bin/env bash
# selfhost_total.sh — pipeline de BOOTSTRAP vérifiable pour le self-hosting total.
#
# Constat daté, jamais une promesse. Ce script N'écrase aucun fichier source :
# il ne lit que compilateur/*.rose et le binaire Rose fourni, et n'écrit que
# dans WORKDIR (par défaut /tmp/opencode/selfhost_total.$$).
#
# Usage :
#   scripts/selfhost_total.sh [CHEMIN_BINAIRE_ROSE] [--full|--quick] [options]
#
#   --full    pipeline complet (émission C0 ~374 s mesurée, timeout 900 s).
#   --quick   contrôles rapides (a, b, fib natif) ; c/d/e/f sont IGNORÉS.
#
# Variables d'environnement (toutes optionnelles) :
#   SELFHOST_ROSE_BIN        binaire Rose à utiliser (sinon 1er argument positionnel)
#   SELFHOST_ROSE_ROOT       racine du dépôt (défaut : parent de ce script)
#   SELFHOST_EMETTEUR        source de l'émetteur RIR  (défaut compilateur/emetteur.rose)
#   SELFHOST_EMETTEUR_ELF    source de l'émetteur ELF  (défaut compilateur/emetteur_elf.rose)
#   SELFHOST_WORKDIR         répertoire de travail (défaut /tmp/opencode/selfhost_total.$$)
#   SELFHOST_RIR_MD5         md5 attendu de self.rir (défaut b3ed9262...)
#   SELFHOST_EMIT_TIMEOUT    timeout d'émission en secondes (défaut 900, OBLIGATOIRE)
#   SELFHOST_C0_MIN_SIZE     taille minimale de C0 en octets (défaut 100000)
#   SELFHOST_C0_PATH         C0 déjà émis à réutiliser (rejeu ; l'émission n'est PAS rejouée,
#                            mais le fichier EST vérifié : ELF aarch64, taille, exécution en d)
#
# Verdicts et codes de sortie :
#   0  TOTAL     toutes les étapes a..f sont PASS
#   2  PARTIEL   a..e PASS, f BLOQUÉ
#   3  BLOQUÉ    au moins une étape among a..e est BLOQUÉE (ou invocation invalide)
#   1  ERREUR    binaire/source introuvable, environnement inutilisable

set -uo pipefail
# Volontairement PAS de `set -e` : chaque étape capture son rc et le rapporte.

# ── Options ──────────────────────────────────────────────────────────────────
usage() { sed -n '2,30p' "${BASH_SOURCE[0]}"; }

MODE="full"
ARG_ROSE_BIN=""
WORKDIR_OPT=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --quick)   MODE="quick"; shift ;;
    --full)    MODE="full";  shift ;;
    --rose-bin)   ARG_ROSE_BIN="${2:-}"; shift 2 ;;
    --workdir)    WORKDIR_OPT="${2:-}"; shift 2 ;;
    -h|--help)  usage; exit 0 ;;
    -*)        echo "OPTION INCONNUE : $1" >&2; usage >&2; exit 1 ;;
    *)         if [[ -z "$ARG_ROSE_BIN" ]]; then ARG_ROSE_BIN="$1"; shift
               else echo "TROP D'ARGUMENTS : $1" >&2; exit 1; fi ;;
  esac
done

# ── Chemins ──────────────────────────────────────────────────────────────────
ROSE_ROOT="${SELFHOST_ROSE_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
ROSE_BIN="${ARG_ROSE_BIN:-${SELFHOST_ROSE_BIN:-$ROSE_ROOT/rose/rose}}"
EMETTEUR="${SELFHOST_EMETTEUR:-$ROSE_ROOT/compilateur/emetteur.rose}"
EMETTEUR_ELF="${SELFHOST_EMETTEUR_ELF:-$ROSE_ROOT/compilateur/emetteur_elf.rose}"
WORKDIR="${WORKDIR_OPT:-${SELFHOST_WORKDIR:-/tmp/opencode/selfhost_total.$$}}"
EXPECTED_RIR_MD5="${SELFHOST_RIR_MD5:-b3ed9262dd54776a6dbd07bc12368d0e}"
EMIT_TIMEOUT="${SELFHOST_EMIT_TIMEOUT:-900}"
C0_MIN_SIZE="${SELFHOST_C0_MIN_SIZE:-100000}"
C0_REUSE="${SELFHOST_C0_PATH:-}"

if [[ ! -x "$ROSE_BIN" ]]; then
  echo "ERREUR : binaire Rose introuvable ou non exécutable : $ROSE_BIN" >&2
  exit 1
fi
for f in "$EMETTEUR" "$EMETTEUR_ELF"; do
  if [[ ! -r "$f" ]]; then
    echo "ERREUR : source Rose illisible : $f" >&2
    exit 1
  fi
done
mkdir -p "$WORKDIR" || { echo "ERREUR : impossible de créer $WORKDIR" >&2; exit 1; }

# ── Journal d'état ───────────────────────────────────────────────────────────
declare -A STATE DETAIL label
order=(a b c d e f)
label[a]="verif(emetteur_elf.rose)"
label[b]="self.rir + md5 b3ed9262"
label[c]="émission C0 (timeout ${EMIT_TIMEOUT}s)"
label[d]="C0(emetteur.rose) == self.rir"
label[e]="point fixe compilateur C1 == C2"
label[f]="C0 compile fib(25) == natif"

pass() { STATE["$1"]="PASS"; DETAIL["$1"]="$2"; echo "[PASS]   $1 — ${label[$1]} : $2"; }
block(){ STATE["$1"]="BLOQUÉ"; DETAIL["$1"]="$2"; echo "[BLOQUÉ] $1 — ${label[$1]} : $2"; }
skip() { STATE["$1"]="IGNORÉ"; DETAIL["$1"]="$2"; echo "[IGNORÉ] $1 — ${label[$1]} : $2"; }

# Détecte un ELF (magic + machine AArch64 = 183) sans dépendre de readelf.
is_elf()       { [[ -s "$1" ]] && [[ "$(od -An -tx1 -N4 "$1" 2>/dev/null | tr -d ' \n')" == "7f454c46" ]]; }
is_aarch64()   { [[ "$(od -An -tu2 -j18 -N2 "$1" 2>/dev/null | tr -d ' \n')" == "183" ]]; }
head_line()    { head -n 1 "$1" 2>/dev/null | cut -c1-120; }
tail_stderr()  { tail -n 3 "$1" 2>/dev/null | tr '\n' ' ' | cut -c1-240; }

echo "============================================================"
echo " selfhost_total — bootstrap vérifiable (constat daté)"
echo "============================================================"
echo "date        : $(date -u +'%Y-%m-%dT%H:%M:%SZ')"
echo "mode        : $MODE"
echo "binaire Rose: $ROSE_BIN"
echo "md5 binaire : $(md5sum "$ROSE_BIN" | awk '{print $1}')"
echo "uname -m    : $(uname -m)"
echo "workdir     : $WORKDIR"
echo "timeout émis: ${EMIT_TIMEOUT}s"
echo ""

# ── a. vérification de l'émetteur ELF ────────────────────────────────────────
VA="$WORKDIR/verif.out"; VE="$WORKDIR/verif.err"
"$ROSE_BIN" verif "$EMETTEUR_ELF" >"$VA" 2>"$VE"; RC=$?
if [[ $RC -eq 0 && -s "$VA" ]]; then
  pass a "rc=0, sortie=$(head_line "$VA")"
else
  block a "rc=$RC, stdout='$(head_line "$VA")', stderr='$(tail_stderr "$VE")'"
fi

# ── b. self.rir + md5 ────────────────────────────────────────────────────────
SR="$WORKDIR/self.rir"; SE="$WORKDIR/self.rir.err"
"$ROSE_BIN" rir "$EMETTEUR" >"$SR" 2>"$SE"; RC=$?
if [[ $RC -ne 0 ]]; then
  block b "rc=$RC, stderr='$(tail_stderr "$SE")'"
else
  GOT_MD5="$(md5sum "$SR" | awk '{print $1}')"
  SZ="$(stat -c%s "$SR")"
  if [[ "$GOT_MD5" != "$EXPECTED_RIR_MD5" ]]; then
    block b "md5 obtenu $GOT_MD5 != attendu $EXPECTED_RIR_MD5 ($SZ o) — re-mesurer la source"
  elif [[ "$SZ" -eq 0 ]]; then
    block b "self.rir vide (0 octet)"
  else
    NOTE=""
    [[ -s "$SE" ]] && NOTE=" (stderr non vide : $(tail_stderr "$SE"))"
    pass b "$SZ o, md5=$GOT_MD5$NOTE"
  fi
fi

# ── c. émission de C0 ────────────────────────────────────────────────────────
C0="$WORKDIR/C0.elf"; CE="$WORKDIR/C0.emit.err"
if [[ "$MODE" == "quick" ]]; then
  skip c "mode --quick : émission C0 non tentée (nécessite --full, ~374 s mesurées)"
elif [[ "${STATE[b]:-}" != "PASS" ]]; then
  block c "étape b BLOQUÉE : pas de self.rir fiable à émettre"
else
  REUSED=0
  if [[ -n "$C0_REUSE" && -s "$C0_REUSE" ]]; then
    cp -f "$C0_REUSE" "$C0" && REUSED=1
  fi
  if [[ $REUSED -eq 0 ]]; then
    T0=$SECONDS
    timeout "${EMIT_TIMEOUT}s" "$ROSE_BIN" "$EMETTEUR_ELF" "$SR" >"$C0" 2>"$CE"; RC=$?
    DUR=$((SECONDS-T0))
  else
    RC=0; DUR=0
  fi
  if [[ $RC -ne 0 ]]; then
    block c "rc=$RC ($([ $RC -eq 124 ] && echo "timeout après ${EMIT_TIMEOUT}s" || echo "échec en ${DUR}s")), stderr='$(tail_stderr "$CE")'"
  elif ! is_elf "$C0"; then
    block c "rc=0 mais sortie non-ELF (entête '$(head_line "$C0")'), ${DUR}s"
  elif ! is_aarch64 "$C0"; then
    block c "ELF mais e_machine != AArch64 (183), ${DUR}s"
  elif [[ "$(stat -c%s "$C0")" -lt "$C0_MIN_SIZE" ]]; then
    block c "ELF aarch64 mais taille $(stat -c%s "$C0") o < $C0_MIN_SIZE o, ${DUR}s"
  else
    chmod 755 "$C0" 2>/dev/null || true
    PREFIX=""; [[ $REUSED -eq 1 ]] && PREFIX="RÉUTILISÉ (SELFHOST_C0_PATH), émission non rejouée — "
    pass c "${PREFIX}$(stat -c%s "$C0") o, ELF aarch64, md5=$(md5sum "$C0" | awk '{print $1}'), ${DUR}s, rc=0"
  fi
fi

# ── d. C0 exécute $principal de emetteur.rose ────────────────────────────────
DUMP="$WORKDIR/dump.out"; DE="$WORKDIR/dump.err"
if [[ "$MODE" == "quick" ]]; then
  skip d "mode --quick : C0 absent"
elif [[ "${STATE[c]:-}" != "PASS" ]]; then
  block d "étape c BLOQUÉE : aucun C0 exécutable"
else
  "$C0" "$EMETTEUR" >"$DUMP" 2>"$DE"; RC=$?
  if [[ $RC -ne 0 ]]; then
    block d "rc=$RC, stderr='$(tail_stderr "$DE")'"
  elif cmp -s "$DUMP" "$SR"; then
    pass d "rc=0, $(stat -c%s "$DUMP") o, cmp octet-pour-octet == self.rir (b3ed9262)"
  else
    FIRST="$(cmp -l "$DUMP" "$SR" 2>/dev/null | head -n 1)"
    block d "rc=0 mais dump != self.rir (premier écart octet : ${FIRST:-inconnu})"
  fi
fi

# ── e. point fixe du compilateur (C1 == C2) ─────────────────────────────────
# Amorçage : P0 (= C1) est émis par le binaire natif depuis le RIR de
# emetteur_elf.rose. P1 (= C2) est émis par P0 depuis le MÊME RIR. P0 et P1
# sont des ELF aarch64 exécutant $principal de emetteur_elf.rose (émetteur
# RIR→ELF). sha256(P0)==sha256(P1) prouve l'idempotence du compilateur.
# LE BINAIRE NATIF SERT DE GRAINE : ce n'est PAS C0 qui émet C1 (C0 = émetteur
# RIR uniquement, voir piège documenté dans BOOTSTRAP_TOTAL.md).
ERIR="$WORKDIR/emetteur_elf.rir"; ERE="$WORKDIR/emetteur_elf.rir.err"
P0="$WORKDIR/P0.elf"; P0E="$WORKDIR/P0.emit.err"
P1="$WORKDIR/P1.elf"; P1E="$WORKDIR/P1.emit.err"
if [[ "$MODE" == "quick" ]]; then
  skip e "mode --quick : émissions P0/P1 non tentées"
elif [[ "${STATE[c]:-}" != "PASS" ]]; then
  block e "étape c BLOQUÉE : C0 absent, aucun compilateur auto-hébergé disponible"
else
  "$ROSE_BIN" rir "$EMETTEUR_ELF" >"$ERIR" 2>"$ERE"; RC=$?
  if [[ $RC -ne 0 || ! -s "$ERIR" ]]; then
    block e "rir(emetteur_elf.rose) rc=$RC, stderr='$(tail_stderr "$ERE")'"
  else
    T0=$SECONDS
    timeout "${EMIT_TIMEOUT}s" "$ROSE_BIN" "$EMETTEUR_ELF" "$ERIR" >"$P0" 2>"$P0E"; RC=$?
    if [[ $RC -ne 0 ]] || ! is_elf "$P0"; then
      block e "amorçage P0 rc=$RC, non-ELF ou échec en $((SECONDS-T0))s, stderr='$(tail_stderr "$P0E")'"
    else
      chmod 755 "$P0" 2>/dev/null || true
      "$P0" "$ERIR" >"$P1" 2>"$P1E"; RC1=$?
      if [[ $RC1 -ne 0 ]] || ! is_elf "$P1"; then
        block e "P0(emetteur_elf.rir) rc=$RC1, stderr='$(tail_stderr "$P1E")'"
      else
        chmod 755 "$P1" 2>/dev/null || true
        H1="$(sha256sum "$P0" | awk '{print $1}')"
        H2="$(sha256sum "$P1" | awk '{print $1}')"
        if [[ "$H1" == "$H2" ]]; then
          pass e "sha256(P0)=sha256(P1)=$H1 ($(stat -c%s "$P0") o) — graine = binaire natif, PAS C0"
        else
          block e "sha256(P0)=$H1 != sha256(P1)=$H2 — pas de point fixe compilateur"
        fi
      fi
    fi
  fi
fi

# ── f. C0 compile un programme arbitraire (fib 25) ──────────────────────────
FIB="$WORKDIR/fib25.rose"; FN="$WORKDIR/fib.native.out"; FNE="$WORKDIR/fib.native.err"
FC0="$WORKDIR/fib.C0.out"; FC0E="$WORKDIR/fib.C0.err"
cat >"$FIB" <<'ROSE'
fn fib(n: entier) -> entier {
  si (n < 2) { retour n }
  retour fib(n - 1) + fib(n - 2)
}
>> fib(25)
ROSE
"$ROSE_BIN" "$FIB" >"$FN" 2>"$FNE"; RC=$?
if [[ $RC -ne 0 || "$(tr -d '[:space:]' <"$FN")" != "75025" ]]; then
  block f "référence native rc=$RC, sortie='$(head_line "$FN")', stderr='$(tail_stderr "$FNE")'"
elif [[ "$MODE" == "quick" ]]; then
  block f "natif=75025 OK mais C0 absent (mode --quick) : compilation par C0 non démontrée"
elif [[ "${STATE[c]:-}" != "PASS" ]]; then
  block f "référence native=75025 OK mais étape c BLOQUÉE : aucun C0 à tester"
else
  "$C0" "$FIB" >"$FC0" 2>"$FC0E"; RC=$?
  if [[ $RC -ne 0 ]]; then
    block f "natif=75025, C0 rc=$RC, stderr='$(tail_stderr "$FC0E")'"
  elif is_elf "$FC0"; then
    chmod 755 "$FC0" 2>/dev/null || true
    "$FC0" >"$WORKDIR/fib.C0.run.out" 2>"$WORKDIR/fib.C0.run.err"; RC=$?
    if [[ $RC -eq 0 && "$(tr -d '[:space:]' <"$WORKDIR/fib.C0.run.out")" == "75025" ]]; then
      pass f "natif=75025, C0 a produit un ELF exécutable rendant 75025"
    else
      block f "ELF C0 rc=$RC, sortie='$(head_line "$WORKDIR/fib.C0.run.out")' != 75025"
    fi
  else
    block f "natif=75025, C0 rc=0 mais sortie NON-ELF (entête '$(head_line "$FC0")') : C0 = émetteur RIR ($principal de emetteur.rose), pas compilateur source→ELF"
  fi
fi

# ── Verdict ──────────────────────────────────────────────────────────────────
BAD_ABE=0
for s in a b c d e; do [[ "${STATE[$s]:-BLOQUÉ}" != "PASS" ]] && BAD_ABE=1; done
if [[ "${STATE[f]:-BLOQUÉ}" == "PASS" && $BAD_ABE -eq 0 ]]; then
  VERDICT="TOTAL"; CODE=0
elif [[ $BAD_ABE -eq 0 ]]; then
  VERDICT="PARTIEL"; CODE=2
else
  VERDICT="BLOQUÉ"; CODE=3
fi

echo ""
echo "============================================================"
echo " RAPPORT — $VERDICT"
echo "============================================================"
for s in "${order[@]}"; do
  printf '  %-7s %-7s %s\n' "$s" "${STATE[$s]:-BLOQUÉ}" "${DETAIL[$s]:-}"
done
echo ""
echo "VERDICT: $VERDICT   (code=$CODE)"
if [[ "$MODE" == "quick" ]]; then
  echo "NOTE: mode --quick — portée réduite, ce verdict ne prouve PAS le self-hosting total."
fi
if [[ "$VERDICT" == "PARTIEL" ]]; then
  echo "NOTE: le point fixe compilateur (e) a été démontré avec le binaire natif comme GRAINE,"
  echo "      pas avec C0. C0 est un émetteur RIR : il ne compile pas une source en ELF (f)."
fi

{
  echo "# Rapport selfhost_total — $(date -u +'%Y-%m-%dT%H:%M:%SZ')"
  echo ""
  echo "- mode : $MODE"
  echo "- binaire Rose : $ROSE_BIN (md5 $(md5sum "$ROSE_BIN" | awk '{print $1}'))"
  echo "- verdict : **$VERDICT** (code $CODE)"
  echo ""
  echo "| Étape | État | Détail |"
  echo "|---|---|---|"
  for s in "${order[@]}"; do
    echo "| $s — ${label[$s]} | ${STATE[$s]:-BLOQUÉ} | ${DETAIL[$s]:-} |"
  done
} >"$WORKDIR/RAPPORT.md"

exit "$CODE"
