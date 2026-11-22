#!/usr/bin/env bash
# verifier_selfhost_final.sh — jalon final du self-hosting A-Z.
#   C0 = émetteur machine du RIR du composite (≈80 min)
#   C1 = C0(rose_natif.rose), C2 = C1(rose_natif.rose), cmp C1 C2
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
B="$ROOT/rose/target/release/rose"
SRC="$ROOT/compilateur/rose_natif.rose"
D=/tmp/opencode
RIR=$D/rnat3.rir
C0=$D/C0_final.elf
C1=$D/C1_final.elf
C2=$D/C2_final.elf
LOG=$D/selfhost_final.log
: > "$LOG"
log() { echo "[$(date -u +%H:%M:%S)] $*" | tee -a "$LOG"; }
magic() { xxd -p -l 8 "$1" 2>/dev/null | tr -d '\n'; }
MAGIC=7f454c4602010100
cd "$ROOT"

log "RIR du composite"
$B rir "$SRC" > "$RIR" 2>/dev/null
log "  $(wc -c < "$RIR") octets"

log "émission de C0 (~80 min)"
timeout 9000 $B compilateur/emetteur_elf.rose "$RIR" > "$C0" 2> "$D/C0_final.err"
rc=$?
log "  C0: rc=$rc taille=$(wc -c < "$C0") magic=$(magic "$C0")"
[ "$rc" -eq 0 ] && [ "$(magic "$C0")" = "$MAGIC" ] || { log "ECHEC C0"; exit 1; }
chmod +x "$C0"

log "test C0 sur fib.rose (en-tête et exécution)"
$B "$SRC" /tmp/opencode/fib28.rose > /tmp/opencode/ref_final.elf 2>/dev/null
timeout 600 "$C0" /tmp/opencode/fib28.rose > /tmp/opencode/c0fib_final.elf 2> "$D/c0fib.err"
rc=$?
hdr=$(xxd -p -s 16 -l 4 /tmp/opencode/c0fib_final.elf 2>/dev/null | tr -d '\n')
chmod +x /tmp/opencode/c0fib_final.elf 2>/dev/null
out=$(timeout 60 /tmp/opencode/c0fib_final.elf 2>/dev/null)
log "  rc=$rc header16=$hdr (attendu 0200b700) sortie=$out (attendu 317811)"
[ "$hdr" = "0200b700" ] && [ "$out" = "317811" ] || { log "ECHEC test fib C0"; exit 1; }
log "  cmp avec le compilateur hébergé: $(cmp -s /tmp/opencode/c0fib_final.elf /tmp/opencode/ref_final.elf && echo OK || echo DIFFERENT)"

log "C1 = C0(rose_natif.rose)"
timeout 3600 "$C0" "$SRC" > "$C1" 2> "$D/C1_final.err"
rc=$?
log "  C1: rc=$rc taille=$(wc -c < "$C1") magic=$(magic "$C1")"
[ "$rc" -eq 0 ] && [ "$(magic "$C1")" = "$MAGIC" ] || { log "ECHEC C1"; head -3 "$D/C1_final.err"; exit 1; }

log "C2 = C1(rose_natif.rose)"
chmod +x "$C1"
timeout 3600 "$C1" "$SRC" > "$C2" 2> "$D/C2_final.err"
rc=$?
log "  C2: rc=$rc taille=$(wc -c < "$C2") magic=$(magic "$C2")"
[ "$rc" -eq 0 ] && [ "$(magic "$C2")" = "$MAGIC" ] || { log "ECHEC C2"; head -3 "$D/C2_final.err"; exit 1; }

if cmp -s "$C1" "$C2"; then
  log "SUCCES — C1 == C2, point fixe étendu ATTEINT, md5=$(md5sum "$C1" | cut -d' ' -f1)"
else
  log "ECHEC — C1 != C2 : $(cmp "$C1" "$C2" 2>&1 | head -1)"
  exit 1
fi
