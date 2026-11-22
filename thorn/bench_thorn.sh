#!/usr/bin/env bash
# thorn/bench_thorn.sh — bench reproductible du noyau Thorn.
# Usage : bash thorn/bench_thorn.sh [N]   (N = nombre de repetitions, defaut 3)
# Sortie : /tmp/opencode/thorn_bench_raw.txt (brut) + tableau median sur stdout.
# Les N repetitions d'une meme charge sont lancees EN PARALLELE : sur cette
# machine (20 coeurs) chaquerose est mono-thread, donc le temps mural par
# repetition reste representatif ; la concurrence est documentee dans
# thorn/docs/BENCH_LEAN.md. Aucune modification du depot, aucun commit.
set -u
cd /home/clausake/Stefan/ROSE || exit 1
N="${1:-3}"
RAW=/tmp/opencode/thorn_bench_raw.txt
mkdir -p /tmp/opencode
: > "$RAW"

log() { printf '%s\n' "$*" | tee -a "$RAW"; }

log "=== BENCH THORN — $(date -u '+%Y-%m-%dT%H:%M:%SZ') ==="
log "version  : $(./rose/rose version 2>&1)"
log "nproc    : $(nproc)"
log "free     : $(free -h | tr '\n' ';')"
log "repetitions : $N (paralleles par charge)"
log "time fmt : %e s / %M Ko"

# Charge : nom | commande (une seule ligne, eval)
run_charge() {
  local nom="$1" cmd="$2" i repfile
  repfile="/tmp/opencode/bench_${nom}.reps"
  : > "$repfile"
  log ""
  log "--- $nom ---"
  log "commande: $cmd"
  for i in $(seq 1 "$N"); do
    ( /usr/bin/time -f "REP $i %e s / %M Ko" bash -c "$cmd" >"/dev/null" 2>>"$repfile" ) &
  done
  wait
  log "brut:"
  sed 's/^/  /' "$repfile"
  log "median_sec: $(awk '{print $3}' "$repfile" | sort -n | awk '{a[NR]=$1} END{print (NR%2)?a[(NR+1)/2]:(a[NR/2]+a[NR/2+1])/2}')"
}

run_charge tests_noyau     './rose/rose thorn/tests_noyau.rose > /dev/null'
run_charge tests_algebre   './rose/rose thorn/tests_algebre.rose > /dev/null'
run_charge tests_hierarchie './rose/rose thorn/tests_hierarchie.rose > /dev/null'
run_charge tests_instances './rose/rose thorn/tests_instances.rose > /dev/null'
run_charge preuve_thn      './rose/rose thorn/src/principal.rose thorn/preuves/associativite.thn > /dev/null'
run_charge terme_individuel './rose/rose thorn/exemples/ex11_mesure_terme.rose > /dev/null'

log ""
log "=== FIN $(date -u '+%Y-%m-%dT%H:%M:%SZ') ==="
