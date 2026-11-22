#!/usr/bin/env bash
# validation_finale.sh — toutes les preuves d'exécution avant publication.
# Aucune affirmation sans commande exécutée. Sortie : /tmp/opencode/validation_finale.log
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
B="$ROOT/rose/target/release/rose"
LOG=/tmp/opencode/validation_finale.log
: > "$LOG"
log() { echo "[$(date -u +%H:%M:%S)] $*" | tee -a "$LOG"; }
ok()  { log "OK   $*"; }
bad() { log "ECHEC $*"; FAILED=$((FAILED+1)); }
FAILED=0
cd "$ROOT"

log "=== 1. build + tests Rust ==="
export PATH="$HOME/.cargo/bin:$PATH"
(cd rose && cargo build --release --bin rose 2>&1 | grep -E '^error|Finished' | tail -3) | tee -a "$LOG"
(cd rose && cargo test --release > /tmp/opencode/vf_tests.log 2>&1); rc=$?
grep '^test result' /tmp/opencode/vf_tests.log | head -3 | tee -a "$LOG"
[ "$rc" -eq 0 ] && ok "tests rc=0" || bad "tests rc=$rc"

log "=== 2. point fixe émetteur ==="
$B rir compilateur/emetteur.rose > /tmp/opencode/vf_p1.rir 2>/dev/null
$B compilateur/emetteur.rose compilateur/emetteur.rose > /tmp/opencode/vf_p2.rir 2>/dev/null
h1=$(md5sum /tmp/opencode/vf_p1.rir | cut -d' ' -f1)
h2=$(md5sum /tmp/opencode/vf_p2.rir | cut -d' ' -f1)
log "md5: $h1 / $h2"
[ "$h1" = "b3ed9262dd54776a6dbd07bc12368d0e" ] && [ "$h1" = "$h2" ] && ok "point fixe" || bad "point fixe"

log "=== 3. AOT zéro refus ==="
bash rose/tests_aot8/verifier.sh > /tmp/opencode/vf_aot.log 2>&1
tail -2 /tmp/opencode/vf_aot.log | tee -a "$LOG"
grep -q '75 OK, 0 ECHEC' /tmp/opencode/vf_aot.log && ok "AOT 75/75" || bad "AOT"
grep -rq 'AOT indisponible\|utilisez `rose comp`' rose/src/ && bad "refus AOT présent" || ok "aucun refus AOT"

log "=== 4. JIT 25/25 Cranelift ==="
nj=0; nt=0
for f in rose/tests_aot8/programmes/*.rose; do
  nt=$((nt+1))
  ROSE_TIMING=1 "$B" "$f" 2>&1 | grep -q 'principal=' && nj=$((nj+1))
done
log "$nj/$nt"
[ "$nj" -eq 25 ] && ok "JIT" || bad "JIT"

log "=== 5. WASM + RVM ==="
"$B" transpile /tmp/opencode/fib28.rose --cible wasm -o /tmp/opencode/vf.wasm >/dev/null 2>&1
sz=$(wc -c < /tmp/opencode/vf.wasm 2>/dev/null || echo 0)
[ "$sz" -gt 0 ] && ok "WASM $sz o" || bad "WASM"
"$B" bc /tmp/opencode/fib28.rose -o /tmp/opencode/vf.bc >/dev/null 2>&1
r=$("$B" rvm /tmp/opencode/vf.bc 2>&1 | tail -1)
[ "$r" = "317811" ] && ok "RVM 317811" || bad "RVM: $r"

log "=== 6. batterie AZ (Unicode) ==="
bash tests_selfhost/lancer_az.sh > /tmp/opencode/vf_az.log 2>&1
awk -F'\t' 'NR>1{c[$4]++} END{for(k in c) print k, c[k]}' /tmp/opencode/az_tests/battery/resultats.tsv | tee -a "$LOG"
nok=$(awk -F'\t' 'NR>1 && $4=="CONFORME"{c++} END{print c+0}' /tmp/opencode/az_tests/battery/resultats.tsv)
nbad=$(awk -F'\t' 'NR>1 && $4!="CONFORME"{c++} END{print c+0}' /tmp/opencode/az_tests/battery/resultats.tsv)
# La batterie AZ est passée de 31 à 47 programmes (vague QAA) : le contrôle
# porte sur ZÉRO non-conforme, avec un plancher de 31, pas sur un cardinal figé.
[ "$nok" -ge 31 ] && [ "$nbad" -eq 0 ] && ok "AZ $nok CONFORME, 0 non-conforme" || bad "AZ $nok CONFORME, $nbad non-conforme"

log "=== 7. self-host HOST-1 ==="
"$B" compilateur/rose_natif.rose /tmp/opencode/fib28.rose > /tmp/opencode/vf_host.elf 2>/dev/null
m=$(xxd -p -l 8 /tmp/opencode/vf_host.elf 2>/dev/null | tr -d '\n')
chmod +x /tmp/opencode/vf_host.elf 2>/dev/null
out=$(/tmp/opencode/vf_host.elf 2>/dev/null)
[ "$m" = "7f454c4602010100" ] && [ "$out" = "317811" ] && ok "HOST-1 fib=317811" || bad "HOST-1: magic=$m out=$out"

log "=== RESULTAT: $FAILED échec(s) ==="
[ "$FAILED" -eq 0 ]
