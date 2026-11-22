#!/usr/bin/env bash
# prof_mem_rose.sh — mesure reproductible (temps réel, pic RSS, swap libre)
# d'une commande Rose, TOUJOURS exécutée sous le garde-fou mémoire.
#
# Usage :
#   scripts/prof_mem_rose.sh [options] -- <commande> [args...]
#
# Options :
#   -m, --max-gio GIO   plafond RSS passé au garde-fou, en Gio (défaut 16)
#   -n, --runs N        nombre d'exécutions, >= 3 (défaut 3)
#   -o, --report FICHIER rapport lisible (défaut /tmp/opencode/prof_mem_rose_rapport.txt)
#   -h, --help          aide
#
# Variables d'environnement équivalentes : PROF_CAP_GIO, PROF_RUNS, PROF_REPORT.
#
# Garanties :
#   * refuse de lancer si MemAvailable < 20 Gio (code 74) ;
#   * appelle systématiquement scripts/garde_fou_mem.sh avec le plafond choisi ;
#   * >= 3 exécutions, médianes du temps et du pic RSS, swap libre échantillonné ;
#   * le rapport est imprimé sur stdout ET écrit dans FICHIER.
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
GARDE="$ROOT/scripts/garde_fou_mem.sh"
MEMINFO=/proc/meminfo
MIN_AVAIL_KO=$((20 * 1024 * 1024))   # 20 Gio, seuil de refus
CAP_GIO=${PROF_CAP_GIO:-16}
RUNS=${PROF_RUNS:-3}
REPORT=${PROF_REPORT:-/tmp/opencode/prof_mem_rose_rapport.txt}

usage() {
  sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    -m|--max-gio) CAP_GIO=$2; shift 2 ;;
    -n|--runs)    RUNS=$2; shift 2 ;;
    -o|--report)  REPORT=$2; shift 2 ;;
    -h|--help)    usage; exit 0 ;;
    --)           shift; break ;;
    -*) echo "prof_mem_rose: option inconnue: $1" >&2; usage >&2; exit 2 ;;
    *)            break ;;
  esac
done
[ "$#" -gt 0 ] || { echo "prof_mem_rose: aucune commande fournie" >&2; usage >&2; exit 2; }
case "$CAP_GIO" in ''|*[!0-9]*) echo "prof_mem_rose: --max-gio doit être un entier" >&2; exit 2 ;; esac
case "$RUNS" in ''|*[!0-9]*) echo "prof_mem_rose: --runs doit être un entier" >&2; exit 2 ;; esac
[ "$RUNS" -ge 3 ] || { echo "prof_mem_rose: au moins 3 exécutions sont requises" >&2; exit 2; }
[ -x "$GARDE" ] || { echo "prof_mem_rose: garde-fou introuvable: $GARDE" >&2; exit 2; }

CAP_KO=$((CAP_GIO * 1024 * 1024))

memval() { awk -v k="$1:" '$1 == k {print $2; exit}' "$MEMINFO"; }

AVAIL0=$(memval MemAvailable)
if [ -z "${AVAIL0:-}" ] || [ "$AVAIL0" -lt "$MIN_AVAIL_KO" ]; then
  echo "REFUS: MemAvailable=${AVAIL0:-?} Ko < 20971520 Ko (20 Gio)." >&2
  echo "Différer la mesure : la machine est déjà sous pression mémoire." >&2
  exit 74
fi

TMP=$(mktemp -d /tmp/opencode/prof_mem_XXXXXX) || exit 1
trap 'rm -rf "$TMP"' EXIT

# médiane d'une colonne de nombres ; $1 = précision printf, $2 = fichier
median() {
  sort -n "$2" | awk -v fmt="$1" '
    { v[NR] = $1 + 0 }
    END {
      if (NR == 0) { printf fmt, 0; exit }
      if (NR % 2) printf fmt, v[(NR + 1) / 2];
      else        printf fmt, (v[NR / 2] + v[NR / 2 + 1]) / 2;
      printf "\n"
    }'
}

# Convertit le « h:mm:ss ou m:ss » de GNU time en secondes décimales.
elapsed_of() {
  awk '/Elapsed \(wall clock\)/ {
    sub(/^.*: /, ""); t = $0; n = split(t, a, ":");
    if (n == 3) printf "%.2f", a[1]*3600 + a[2]*60 + a[3];
    else if (n == 2) printf "%.2f", a[1]*60 + a[2];
    else printf "%.2f", a[1] + 0;
    exit
  }' "$1"
}
rss_of() {
  awk '/Maximum resident set size/ { print $NF; exit }' "$1"
}

CMD="$*"
: > "$TMP/times"; : > "$TMP/rss"; : > "$TMP/swapmin"

echo "=== prof_mem_rose — $(date -u +'%Y-%m-%dT%H:%M:%SZ') ==="
echo "commande      : $CMD"
echo "garde-fou     : $GARDE $CAP_KO Ko (${CAP_GIO} Gio)"
echo "exécutions    : $RUNS (médianes)"
echo "MemAvailable  : ${AVAIL0} Ko ($(awk -v v="$AVAIL0" 'BEGIN{printf "%.1f", v/1048576}') Gio) au départ"
echo "NOTE          : le temps réel inclut la latence de polling du garde-fou (0 à 5 s)."
echo
echo " run | rc  | temps réel | pic RSS        | swap libre min pendant le run"
echo "-----+-----+------------+----------------+-------------------------------"

for i in $(seq 1 "$RUNS"); do
  SW0=$(memval SwapFree)
  /usr/bin/time -v -o "$TMP/time.$i" "$GARDE" "$CAP_KO" "$@" >/dev/null 2>"$TMP/err.$i" &
  TPID=$!
  (
    min=999999999999
    while kill -0 "$TPID" 2>/dev/null; do
      s=$(memval SwapFree)
      [ -n "${s:-}" ] && [ "$s" -lt "$min" ] && min=$s
      sleep 1
    done
    echo "$min" > "$TMP/swapmin.$i"
  ) &
  SAMPLER=$!
  wait "$TPID"; rc=$?
  wait "$SAMPLER" 2>/dev/null
  SW1=$(memval SwapFree)

  t=$(elapsed_of "$TMP/time.$i"); t=${t:-0}
  r=$(rss_of "$TMP/time.$i");     r=${r:-0}
  sm=$(cat "$TMP/swapmin.$i" 2>/dev/null); sm=${sm:-$SW0}
  [ "$sm" -lt "$SW1" ] && sm=$SW1
  echo "$t"  >> "$TMP/times"
  echo "$r"  >> "$TMP/rss"
  echo "$sm" >> "$TMP/swapmin"
  printf '%4d | %3d | %10s | %14s | %s Ko (début %s, fin %s)\n' \
    "$i" "$rc" "$t" "$r" "$sm" "$SW0" "$SW1"
  LAST_RC=$rc
done

MT=$(median '%.2f' "$TMP/times")
MR=$(median '%.0f' "$TMP/rss")
MS=$(median '%.0f' "$TMP/swapmin")

echo
echo "--- MÉDIANES ($RUNS exécutions) ---"
echo "temps réel            : ${MT} s (inclut 0 à 5 s de polling du garde-fou)"
echo "pic RSS               : ${MR} Ko ($(awk -v v="$MR" 'BEGIN{printf "%.2f", v/1048576}') Gio)"
echo "swap libre min        : ${MS} Ko ($(awk -v v="$MS" 'BEGIN{printf "%.2f", v/1048576}') Gio)"
echo "plafond garde-fou     : ${CAP_KO} Ko — marge : $(awk -v a="$CAP_KO" -v b="$MR" 'BEGIN{printf "%.2f", (a-b)/1048576}') Gio"
echo "dernier rc            : ${LAST_RC} (75 = limite mémoire atteinte, arrêt propre par le garde-fou)"
echo "trace garde-fou       : /tmp/opencode/garde_fou_mem.log"
echo "stderr dernière passe : $TMP/err.$RUNS (supprimé à la sortie)"

{
  echo "=== prof_mem_rose — $(date -u +'%Y-%m-%dT%H:%M:%SZ') ==="
  echo "commande=$CMD"
  echo "garde-fou=$GARDE plafond=${CAP_KO}Ko runs=$RUNS"
  echo "median temps_reel=${MT}s pic_RSS=${MR}Ko swap_libre_min=${MS}Ko rc=$LAST_RC"
} >> /tmp/opencode/prof_mem_rose_journal.tsv

mkdir -p "$(dirname "$REPORT")" 2>/dev/null
{
  echo "=== prof_mem_rose — rapport détaillé ==="
  echo "date        : $(date -u +'%Y-%m-%dT%H:%M:%SZ')"
  echo "commande    : $CMD"
  echo "garde-fou   : $GARDE $CAP_KO Ko"
  echo "runs        : $RUNS"
  echo "median temps_reel=${MT} s ; pic RSS=${MR} Ko ; swap libre min=${MS} Ko ; rc=${LAST_RC}"
  echo "NOTE : le temps réel inclut la latence de polling du garde-fou (0 à 5 s par exécution)."
} > "$REPORT"
exit "$LAST_RC"
