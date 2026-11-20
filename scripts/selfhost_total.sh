#!/usr/bin/env bash
# selfhost_total.sh — pipeline de BOOTSTRAP vérifiable pour le self-hosting total.
#
# Constat daté, jamais une promesse. Ce script N'écrase aucun fichier source :
# il ne lit que compilateur/*.rose, tests_selfhost/*.rose et le binaire Rose
# fourni, et n'écrit que dans WORKDIR (par défaut /tmp/opencode/selfhost_total.$$).
#
# Usage :
#   scripts/selfhost_total.sh [CHEMIN_BINAIRE_ROSE] [--full|--quick] [options]
#
#   --full        pipeline complet (émission C0 ~374 s mesurée, timeout 900 s).
#   --quick       contrôles rapides (a, b, fib natif) ; c/d/e/f/g/h NON ÉVALUÉS.
#   --with-c13    étape g = C1.3 : générer F1 depuis le source courant, exécuter
#                 F1 sur son propre RIR -> F2, comparer md5. ~2×430 s en --quick ;
#                 quasi-gratuit en --full (réutilise P0/P1 de l'étape e, même source).
#                 F1 sert aussi de compilateur généré pour l'étape h.
#   --inventaire  tableau lisible des primitives G5 (faits documentés SH5–SH11),
#                 sans mesure et sans écriture hors WORKDIR. Sortie immédiate, rc=0.
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
#   SELFHOST_C13_MD5         md5 attendu de F1=F2 (défaut 6f2715e5...)
#   SELFHOST_C13_MIN_SIZE    taille minimale de F1/F2 en octets (défaut 400000)
#
# Verdicts et codes de sortie :
#   0  A–Z ATTEINT   a..e PASS, g PASS, h PASS (F1 re produit chaque sortie native)
#   2  A–Z PARTIEL   a..e PASS, g PASS, h PARTIEL (fib OK, bigints/tables bloqués)
#   3  BLOQUÉ        au moins une étape a..e BLOQUÉE, portée --quick, ou g/h BLOQUÉS
#   4  NON ÉVALUÉ    a..e PASS mais C1.3 non demandé (g IGNORÉ) : A–Z non mesuré
#   1  ERREUR        binaire/source introuvable, environnement inutilisable

set -uo pipefail
# Volontairement PAS de `set -e` : chaque étape capture son rc et le rapporte.

# ── Options ──────────────────────────────────────────────────────────────────
usage() { sed -n '2,39p' "${BASH_SOURCE[0]}"; }

MODE="full"
C13_OPT=0
INVENTAIRE=0
ARG_ROSE_BIN=""
WORKDIR_OPT=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --quick)   MODE="quick"; shift ;;
    --full)    MODE="full";  shift ;;
    --with-c13)  C13_OPT=1;  shift ;;
    --inventaire) INVENTAIRE=1; shift ;;
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

# ── --inventaire : faits DOCUMENTÉS, aucune mesure, aucune invention ─────────
# Sources : SH5 = SELFHOST_TOTAL_PLAN.md, SH6 = G5_PRIMITIVES.md,
# SH7 = G5_C0.md, SH10 = G5_C1.md, SH11 = C13_FIX.md (tous dans rose/docs/).
if [[ $INVENTAIRE -eq 1 ]]; then
  echo "============================================================"
  echo " inventaire G5 — faits documentés SH5/SH6/SH7/SH10/SH11"
  echo "============================================================"
  echo "date : $(date -u +'%Y-%m-%dT%H:%M:%SZ')   (constat daté, aucune mesure ici)"
  echo ""
  echo "Légende : OK = conforme mesurée ; PARTIEL = cas limites connus divergents ;"
  echo "          ABSENT = pas de routine, stub0, ou changement d'ABI nécessaire."
  echo ""
  cat <<'TAB'
| Primitive / capacité                     | État    | Preuve documentée (doc, section)                         |
|------------------------------------------|---------|-----------------------------------------------------------|
| quitter (C0.1)                           | OK      | SH7 G5_C0.md §2 : natif=ELF, rc=7 octet pour octet        |
| caractere (C0.2)                         | OK      | SH7 G5_C0.md §2/§3 : A/1/AX natif=ELF                     |
| partie_entiere (C0.3)                    | OK      | SH7 G5_C0.md §2/§3 : 3/3 natif=ELF                        |
| globales G5 en RW (C1)                   | OK      | SH10 G5_C1.md §2/§4 : 512×8 à 0x60004000, g1..g3 conformes|
| auto-exécution de l'émetteur (C1)        | OK      | SH10 G5_C1.md §6 : e_self5.elf tiny.rir = référence native|
| F1 == F2 (C1.3, auto-régénération)       | OK      | SH11 C13_FIX.md §5.2 : 432544 o, md5 6f2715e5… identiques|
| point fixe b3ed9262 (dump emetteur.rose) | OK      | SH11 C13_FIX.md §6 : identique ×2 (dump ELF) + ×2 (natif) |
| affichage des listes                     | OK      | SH6 G5_PRIMITIVES.md §3 : 7 tests natif=ELF octet pour octet|
| argument(i) hors limites                 | OK      | SH6 G5_PRIMITIVES.md §4 : rc=1 + stderr identiques        |
| flottants >= 1e16                        | OK      | SH6 G5_PRIMITIVES.md §5 : 1e+20 etc. natif=ELF            |
| fib / arithmétique entière simple        | OK      | SH11 C13_FIX.md §7 : fib25 75025 / 75025                  |
| longueur(0) (tag non chaîne)             | PARTIEL | SH5 SELFHOST_TOTAL_PLAN.md l.91 : natif erreur / ELF rc=139|
| inserer(t,c,v) : valeur v ignorée        | PARTIEL | SH5 l.96 + SH6 G5_PRIMITIVES.md §7.2                      |
| table() = liste de clés                  | PARTIEL | SH5 l.101 + SH6 §7.2 : t["a"] n'est pas une lecture       |
| rationnels négatifs / calculés           | PARTIEL | SH7 G5_C0.md §4 + SH11 C13_FIX.md §8.2 (16 divergences)   |
| Unicode des sources arbitraires          | PARTIEL | SH11 C13_FIX.md §3/§8 : g5_cps corrige l'émetteur, lecture|
|                                          |         | G5 par octets vs natif par points de code subsiste        |
| type / abs / min / max, graine / hasard  | PARTIEL | SH11 C13_FIX.md §8.2 : divergences documentées            |
| grands entiers > 64 bits (ABI multi-limbes)| ABSENT | SH6 G5_PRIMITIVES.md §7.1 : 2^64 -> 0 ; SH5 l.116         |
| valeurs de tables + supprimer()          | ABSENT  | SH6 G5_PRIMITIVES.md §7.2 : supprimer = stub               |
| afficher_erreur (routine générique)      | ABSENT  | SH5 SELFHOST_TOTAL_PLAN.md l.114 : stub0                  |
| eval_expr (ancien compilateur)           | ABSENT  | SH5 l.115 ; contourné par quitter (SH7 §2)               |
| HOF / surcharge / listes de listes / fact| ABSENT  | SH7 G5_C0.md §4 + SH10 G5_C1.md §7.2 : 7 blocages SH9     |
TAB
  echo ""
  echo "Batterie SH9 (80 programmes) — dernière mesure documentée (SH11 §7) :"
  echo "  57 CONFORMES / 16 DIVERGENTS / 7 BLOQUÉS (aucune régression SH7→SH11)."
  echo "Validation globale documentée (SH11 §7) : cargo 515 passés/0 échec ;"
  echo "  jardins 9/10 (gui_native.rose : faux positif commentaire « Rust ») ; fib25 OK."
  echo ""
  echo "NOTE: SH12 (tables réelles + rationnels négatifs) est un chantier CONCURRENT."
  echo "      Son résultat n'est PAS repris ici : seuls les faits SH5–SH11 sont affichés."
  exit 0
fi

ROSE_BIN="${ARG_ROSE_BIN:-${SELFHOST_ROSE_BIN:-$ROSE_ROOT/rose/rose}}"
EMETTEUR="${SELFHOST_EMETTEUR:-$ROSE_ROOT/compilateur/emetteur.rose}"
EMETTEUR_ELF="${SELFHOST_EMETTEUR_ELF:-$ROSE_ROOT/compilateur/emetteur_elf.rose}"
WORKDIR="${WORKDIR_OPT:-${SELFHOST_WORKDIR:-/tmp/opencode/selfhost_total.$$}}"
EXPECTED_RIR_MD5="${SELFHOST_RIR_MD5:-b3ed9262dd54776a6dbd07bc12368d0e}"
EMIT_TIMEOUT="${SELFHOST_EMIT_TIMEOUT:-900}"
C0_MIN_SIZE="${SELFHOST_C0_MIN_SIZE:-100000}"
C0_REUSE="${SELFHOST_C0_PATH:-}"
C13_MD5="${SELFHOST_C13_MD5:-6f2715e5c5fe26c3e26f6ac404051bd2}"
C13_MIN_SIZE="${SELFHOST_C13_MIN_SIZE:-400000}"

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
order=(a b c d e f g h)
label[a]="verif(emetteur_elf.rose)"
label[b]="self.rir + md5 b3ed9262"
label[c]="émission C0 (timeout ${EMIT_TIMEOUT}s)"
label[d]="C0(emetteur.rose) == self.rir"
label[e]="point fixe compilateur P0 == P1"
label[f]="C0 compile fib(25) == natif (diagnostic)"
label[g]="C1.3 : F1 == F2 (auto-régénération, --with-c13)"
label[h]="A–Z : F1 compile des sources arbitraires == natif"

pass() { STATE["$1"]="PASS"; DETAIL["$1"]="$2"; echo "[PASS]   $1 — ${label[$1]} : $2"; }
block(){ STATE["$1"]="BLOQUÉ"; DETAIL["$1"]="$2"; echo "[BLOQUÉ] $1 — ${label[$1]} : $2"; }
partiel(){ STATE["$1"]="PARTIEL"; DETAIL["$1"]="$2"; echo "[PARTIEL] $1 — ${label[$1]} : $2"; }
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
echo "mode        : $MODE   (--with-c13=$C13_OPT)"
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
    block f "natif=75025, C0 rc=0 mais sortie NON-ELF (entête '$(head_line "$FC0")') : C0 = émetteur RIR (\$principal de emetteur.rose), pas compilateur source→ELF"
  fi
fi

# ── g. C1.3 : F1 == F2, auto-régénération du compilateur ─────────────────────
# Chaîne exacte (SH11/C13_FIX.md §5.2) :
#   rose rir emetteur_elf.rose -> e.rir
#   rose emetteur_elf.rose e.rir -> F1        (compilateur généré par le natif)
#   ./F1 e.rir -> F2                          (F1 s'auto-régénère, sans patch externe)
#   cmp F1 F2 -> IDENTIQUE  (432544 o, md5 6f2715e5c5fe26c3e26f6ac404051bd2)
# En --full, l'étape e a déjà produit P0/P1 avec la MÊME chaîne et la MÊME source :
# on les réutilise (aucune émission rejouée, ~2×430 s économisés) et on le dit.
G_RIR="$WORKDIR/C13.rir"; GRE="$WORKDIR/C13.rir.err"
F1="$WORKDIR/F1.elf"; F1E="$WORKDIR/F1.emit.err"
F2="$WORKDIR/F2.elf"; F2E="$WORKDIR/F2.emit.err"
if [[ $C13_OPT -ne 1 ]]; then
  skip g "C1.3 non demandé (--with-c13 ; ~2×430 s en --quick, réutilisation de e en --full)"
elif [[ "${STATE[e]:-BLOQUÉ}" == "PASS" && -s "$P0" && -s "$P1" ]]; then
  cp -f "$P0" "$F1"; cp -f "$P1" "$F2"
  H1="$(md5sum "$F1" | awk '{print $1}')"; H2="$(md5sum "$F2" | awk '{print $1}')"
  SZ1="$(stat -c%s "$F1")"; SZ2="$(stat -c%s "$F2")"
  if [[ "$H1" != "$H2" ]]; then
    block g "F1=$H1 ($SZ1 o) != F2=$H2 ($SZ2 o) — pas de point fixe C1.3"
  elif [[ "$SZ1" -lt "$C13_MIN_SIZE" ]] || ! is_elf "$F1"; then
    block g "F1=F2=$H1 mais $SZ1 o < $C13_MIN_SIZE o ou non-ELF"
  else
    NOTE=""; [[ "$H1" != "$C13_MD5" ]] && NOTE=" (réf. documentée $C13_MD5, source différente)"
    pass g "F1==F2=$H1, $SZ1 o, cmp=0 — réutilisés de l'étape e (même source courant, 0 émission rejouée)$NOTE"
  fi
elif [[ "${STATE[b]:-BLOQUÉ}" != "PASS" ]]; then
  block g "réflexivité non disponible : étape b (self.rir) BLOQUÉE"
else
  "$ROSE_BIN" rir "$EMETTEUR_ELF" >"$G_RIR" 2>"$GRE"; RC=$?
  if [[ $RC -ne 0 || ! -s "$G_RIR" ]]; then
    block g "rir(emetteur_elf.rose) rc=$RC, stderr='$(tail_stderr "$GRE")'"
  else
    T0=$SECONDS
    timeout "${EMIT_TIMEOUT}s" "$ROSE_BIN" "$EMETTEUR_ELF" "$G_RIR" >"$F1" 2>"$F1E"; RC=$?
    if [[ $RC -ne 0 ]] || ! is_elf "$F1"; then
      block g "émission F1 rc=$RC, non-ELF ou échec en $((SECONDS-T0))s, stderr='$(tail_stderr "$F1E")'"
    else
      chmod 755 "$F1" 2>/dev/null || true
      timeout "${EMIT_TIMEOUT}s" "$F1" "$G_RIR" >"$F2" 2>"$F2E"; RC2=$?
      if [[ $RC2 -ne 0 ]] || ! is_elf "$F2"; then
        block g "F1(e.rir) rc=$RC2, non-ELF ou échec en $((SECONDS-T0))s, stderr='$(tail_stderr "$F2E")'"
      else
        H1="$(md5sum "$F1" | awk '{print $1}')"; H2="$(md5sum "$F2" | awk '{print $1}')"
        SZ1="$(stat -c%s "$F1")"; SZ2="$(stat -c%s "$F2")"
        if [[ "$H1" != "$H2" ]] || ! cmp -s "$F1" "$F2"; then
          block g "F1=$H1 ($SZ1 o) != F2=$H2 ($SZ2 o) — cmp non nul, pas de C1.3"
        elif [[ "$SZ1" -lt "$C13_MIN_SIZE" ]]; then
          block g "F1=F2=$H1 mais $SZ1 o < $C13_MIN_SIZE o"
        else
          NOTE=""; [[ "$H1" != "$C13_MD5" ]] && NOTE=" (réf. documentée $C13_MD5, source différente)"
          pass g "F1==F2=$H1, $SZ1 o, cmp=0, en $((SECONDS-T0))s$NOTE"
        fi
      fi
    fi
  fi
fi

# ── h. A–Z : F1 compile une source arbitraire et reproduit le natif ──────────
# Critère opérationnel : pour chaque source, rose rir -> RIR, F1(RIR) -> ELF,
# ELF exécuté -> sortie. On compare stdout, stderr et rc au binaire natif.
# Sources représentatives : fib récursif (marche), bigint 2^64 (bloqué ABI),
# table avec lecture de valeur (bloqué SH6 §7.2). Le verdict distingue
# MARCHE / BLOQUÉ par source : c'est une mesure, pas une affirmation.
AZ_TAB="$WORKDIR/az_tabval.rose"
cat >"$AZ_TAB" <<'ROSE'
$t = table()
inserer($t, "a", 10)
>> cle_existe($t, "a")
>> $t["a"]
>> longueur($t)
ROSE
AZ_SRC=( "$ROSE_ROOT/tests_selfhost/t74_fib_rec.rose"
         "$ROSE_ROOT/tests_selfhost/t33_big_2_64.rose"
         "$AZ_TAB" )
AZ_TAG=( fib bigint tables )
AZ_EMITTER=""
AZ_HOW=""
if [[ "${STATE[g]:-BLOQUÉ}" == "PASS" && -s "$F1" ]]; then
  AZ_EMITTER="$F1"; AZ_HOW="F1 (C1.3)"
elif [[ "${STATE[e]:-BLOQUÉ}" == "PASS" && -s "$P0" ]]; then
  AZ_EMITTER="$P0"; AZ_HOW="P0 (étape e, même compilateur généré, graine native)"
fi

if [[ -z "$AZ_EMITTER" ]]; then
  if [[ "$MODE" == "quick" ]]; then
    skip h "aucun compilateur généré disponible en --quick (utiliser --full ou --with-c13)"
  else
    block h "aucun compilateur généré (étapes e et g indisponibles) : A–Z non mesurable"
  fi
else
  echo "   compilateur généré utilisé : $AZ_HOW"
  AZ_OK=0; AZ_BAD=0; AZ_FIB_OK=0
  for i in "${!AZ_SRC[@]}"; do
    SRC="${AZ_SRC[$i]}"; TAG="${AZ_TAG[$i]}"
    NO="$WORKDIR/az_${TAG}.native.out"; NE="$WORKDIR/az_${TAG}.native.err"
    RR="$WORKDIR/az_${TAG}.rir"; RE="$WORKDIR/az_${TAG}.rir.err"
    EF="$WORKDIR/az_${TAG}.elf"; EE="$WORKDIR/az_${TAG}.elf.err"
    XO="$WORKDIR/az_${TAG}.elf.out"; XE="$WORKDIR/az_${TAG}.elf.err.out"
    if [[ ! -r "$SRC" ]]; then
      echo "    • $TAG : BLOQUÉ — source introuvable : $SRC"
      AZ_BAD=$((AZ_BAD+1)); continue
    fi
    "$ROSE_BIN" "$SRC" >"$NO" 2>"$NE"; NR=$?
    if [[ $NR -ne 0 ]]; then
      echo "    • $TAG : BLOQUÉ — référence native rc=$NR, stderr='$(tail_stderr "$NE")'"
      AZ_BAD=$((AZ_BAD+1)); continue
    fi
    "$ROSE_BIN" rir "$SRC" >"$RR" 2>"$RE"; RRc=$?
    if [[ $RRc -ne 0 || ! -s "$RR" ]]; then
      echo "    • $TAG : BLOQUÉ — rir rc=$RRc, stderr='$(tail_stderr "$RE")'"
      AZ_BAD=$((AZ_BAD+1)); continue
    fi
    timeout "${EMIT_TIMEOUT}s" "$AZ_EMITTER" "$RR" >"$EF" 2>"$EE"; ER=$?
    if [[ $ER -ne 0 ]] || ! is_elf "$EF"; then
      echo "    • $TAG : BLOQUÉ — émission par $AZ_HOW rc=$ER, non-ELF, stderr='$(tail_stderr "$EE")'"
      AZ_BAD=$((AZ_BAD+1)); continue
    fi
    chmod 755 "$EF" 2>/dev/null || true
    "$EF" >"$XO" 2>"$XE"; XR=$?
    if [[ $XR -eq $NR ]] && cmp -s "$NO" "$XO" && cmp -s "$NE" "$XE"; then
      echo "    • $TAG ($(basename "$SRC")) : MARCHE — rc=$XR, stdout='$(head_line "$XO")', cmp=0 avec le natif"
      AZ_OK=$((AZ_OK+1))
      [[ "$TAG" == "fib" ]] && AZ_FIB_OK=1
    else
      echo "    • $TAG ($(basename "$SRC")) : BLOQUÉ — natif rc=$NR stdout='$(head_line "$NO")' | ELF rc=$XR stdout='$(head_line "$XO")'"
      AZ_BAD=$((AZ_BAD+1))
    fi
  done
  if [[ $AZ_FIB_OK -eq 0 ]]; then
    block h "fib natif vs compilateur généré divergence ($AZ_OK MARCHE / $AZ_BAD BLOQUÉ)"
  elif [[ $AZ_BAD -gt 0 ]]; then
    partiel h "$AZ_OK MARCHE / $AZ_BAD BLOQUÉ (via $AZ_HOW) — voir lignes ci-dessus"
  else
    pass h "$AZ_OK/$AZ_OK sources reproduites octet pour octet via $AZ_HOW — A–Z atteint sur l'échantillon"
  fi
fi

# ── Verdict ──────────────────────────────────────────────────────────────────
# f est un DIAGNOSTIC : C0 = émetteur RIR, il ne compile pas une source en ELF ;
# son BLOQUÉ est attendu et documenté, il ne détermine plus le verdict A–Z.
# Le verdict porte sur a..e (chaîne de bootstrap), g (C1.3) et h (A–Z).
BAD_ABE=0
for s in a b c d e; do
  st="${STATE[$s]:-BLOQUÉ}"
  [[ "$st" == "BLOQUÉ" || "$st" == "IGNORÉ" ]] && BAD_ABE=1
done
if [[ $BAD_ABE -eq 1 ]]; then
  VERDICT="BLOQUÉ"; CODE=3
elif [[ "${STATE[g]:-BLOQUÉ}" == "IGNORÉ" ]]; then
  VERDICT="A–Z NON ÉVALUÉ"; CODE=4
elif [[ "${STATE[g]:-BLOQUÉ}" != "PASS" || "${STATE[h]:-BLOQUÉ}" == "BLOQUÉ" ]]; then
  VERDICT="BLOQUÉ"; CODE=3
elif [[ "${STATE[h]:-BLOQUÉ}" == "PASS" ]]; then
  VERDICT="A–Z ATTEINT"; CODE=0
elif [[ "${STATE[h]:-BLOQUÉ}" == "PARTIEL" ]]; then
  VERDICT="A–Z PARTIEL"; CODE=2
else
  VERDICT="A–Z NON ÉVALUÉ"; CODE=4
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
if [[ "${STATE[f]:-BLOQUÉ}" == "BLOQUÉ" ]]; then
  echo "NOTE: f est un diagnostic attendu — C0 est un émetteur RIR (émis depuis emetteur.rose) :"
  echo "      il ne compile PAS une source en ELF. Le critère A–Z est l'étape h, avec F1/P0."
fi
if [[ "$VERDICT" == "A–Z PARTIEL" ]]; then
  echo "NOTE: A–Z atteint exige h PASS sur TOUTES les sources (y compris bigints et tables),"
  echo "      pas seulement fib. Les lignes • ci-dessus nomment précisément ce qui bloque."
fi
if [[ "$VERDICT" == "A–Z NON ÉVALUÉ" ]]; then
  echo "NOTE: rejouer avec --with-c13 (et de préférence --full) pour mesurer A–Z."
fi

{
  echo "# Rapport selfhost_total — $(date -u +'%Y-%m-%dT%H:%M:%SZ')"
  echo ""
  echo "- mode : $MODE (--with-c13=$C13_OPT)"
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
