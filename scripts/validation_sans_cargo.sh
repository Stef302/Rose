#!/usr/bin/env bash
# validation_sans_cargo.sh — les 7 familles de validation_finale.sh, SANS cargo.
#
# Appelé par `scripts/validation_finale.sh --sans-cargo`.
#
# Principe
# --------
# * §1 ne compile plus rien avec cargo : il construit le moteur auto-hébergé
#   C0 avec `scripts/build-sans-cargo.sh` (sous garde-fou mémoire), puis
#   prouve qu'il tourne (version, RIR figé, fib(28), AOT natif, module WASM
#   byte-identique à l'oracle sur un programme du sous-ensemble).
# * Les sections 2 à 7 gardent les MÊMES commandes, les MÊMES artefacts et
#   les MÊMES seuils que validation_finale.sh. Elles s'exécutent contre un
#   binaire Rose PRÉCONSTRUIT (aucune compilation, aucun toolchain Rust) :
#   c'est le seul moyen d'obtenir les comportements qui ne sont pas dans le
#   composé C0 actuel (ligne `principal=` du JIT, cibles AOT x86-64/PE,
#   sérialiseurs WASM/ROSEBC complets, commande rvm, et compilation
#   réflexive de emetteur.rose sous la limite de 12 Gio du garde-fou).
# * Un binaire `cargo` PIÈGE est placé en tête du PATH : tout appel à cargo
#   est journalisé et échoue (rc=99). La fin du script exige un journal vide.
#
# Variables :
#   ROSE_REFERENCE  binaire Rose préconstruit à utiliser pour les sections 2-7
#                   (défaut : premier exécutable trouvé parmi les chemins
#                   $ROOT/rose/target/release/rose, $ROOT/rose/rose,
#                   /home/clausake/Stefan/ROSE/rose/target/release/rose,
#                   /home/clausake/Stefan/ROSE/rose/rose).
#   VF_REUTILISER=1 réutilise le moteur C0 déjà construit dans $OUT.
#   VF_SOURCE_C0    source composée passée à build-sans-cargo.sh (défaut :
#                   compilateur/moteur_complet.rose du worktree).
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT=/tmp/opencode/vf_sans_cargo
LOG=/tmp/opencode/validation_finale_sans_cargo.log
MODEL="$OUT/moteur.elf"
FAILED=0
: > "$LOG"
log() { echo "[$(date -u +%H:%M:%S)] $*" | tee -a "$LOG"; }
ok()  { log "OK   $*"; }
bad() { log "ECHEC $*"; FAILED=$((FAILED+1)); }
cd "$ROOT"
mkdir -p "$OUT/bin" || exit 1

# ── piège à cargo ────────────────────────────────────────────────────────
CARGO_LOG="$OUT/cargo_appels.log"
: > "$CARGO_LOG"
cat > "$OUT/bin/cargo" <<EOF
#!/usr/bin/env bash
echo "\$*" >> "$CARGO_LOG"
echo "INTERDIT : cargo a ete appele pendant validation --sans-cargo" >&2
exit 99
EOF
chmod 755 "$OUT/bin/cargo"
export PATH="$OUT/bin:$PATH"

# ── binaire préconstruit pour les sections 2-7 ───────────────────────────
REF="${ROSE_REFERENCE:-}"
if [[ -z "$REF" || ! -x "$REF" ]]; then
  for cand in "$ROOT/rose/target/release/rose" "$ROOT/rose/rose" \
              /home/clausake/Stefan/ROSE/rose/target/release/rose \
              /home/clausake/Stefan/ROSE/rose/rose; do
    [[ -x "$cand" ]] && { REF="$cand"; break; }
  done
fi
if [[ -z "$REF" || ! -x "$REF" ]]; then
  echo "ERREUR : aucun binaire Rose préconstruit (poser ROSE_REFERENCE)" >&2
  exit 1
fi
REF="$(realpath "$REF")"
export ROSE="$REF"   # rose/tests_aot8/verifier.sh lit $ROSE
log "binaire de référence (préconstruit, aucun cargo) : $REF"

# tests_selfhost/lancer_az.sh a le chemin $ROOT/rose/rose en dur : on pose
# un lien symbolique temporaire, retiré à la sortie (aucun fichier suivi
# du dépôt n'est modifié).
SHIM="$ROOT/rose/rose"
SHIM_POSE=0
if [[ ! -e "$SHIM" ]]; then
  ln -s "$REF" "$SHIM" && SHIM_POSE=1
fi
nettoyer() { [[ $SHIM_POSE -eq 1 ]] && rm -f "$SHIM"; }
trap nettoyer EXIT

# ── 1. moteur C0 (aucun cargo) ───────────────────────────────────────────
log "=== 1. moteur C0 + tests (SANS cargo) ==="
SRC="${VF_SOURCE_C0:-$ROOT/compilateur/moteur_complet.rose}"
if [[ "${VF_REUTILISER:-0}" = "1" && -x "$MODEL" ]]; then
  # La batterie intégrée (12 programmes) reste active : build-sans-cargo.sh
  # termine avec rc=1 si --sans-tests rend total=0 (condition finale du
  # script), donc on la laisse tourner pour obtenir un vrai rc=0.
  bash scripts/build-sans-cargo.sh --sortie "$OUT" --source "$SRC" \
    --reutiliser > /tmp/opencode/vf_c0_build.log 2>&1
else
  bash scripts/garde_fou_mem.sh 12000000 \
    bash scripts/build-sans-cargo.sh --sortie "$OUT" --source "$SRC" \
      --timeout 1800 > /tmp/opencode/vf_c0_build.log 2>&1
fi
brc=$?
tail -4 /tmp/opencode/vf_c0_build.log | tee -a "$LOG"
[ "$brc" -eq 0 ] && [ -x "$MODEL" ] && ok "build C0 rc=0" || bad "build C0 rc=$brc"

# Preuves que le moteur C0 exécute réellement le langage (pas un stub).
c0_version=$(bash scripts/garde_fou_mem.sh 12000000 "$MODEL" version 2>/dev/null | head -1)
[[ -n "$c0_version" ]] && ok "C0 version: $c0_version" || bad "C0 version vide"
bash scripts/garde_fou_mem.sh 12000000 "$MODEL" rir compilateur/emetteur.rose \
  > "$OUT/c0_emetteur.rir" 2> /dev/null
hm=$(md5sum "$OUT/c0_emetteur.rir" | cut -d' ' -f1)
[ "$hm" = "b3ed9262dd54776a6dbd07bc12368d0e" ] && ok "C0 rir emetteur $hm" \
  || bad "C0 rir emetteur $hm"
fv=$(bash scripts/garde_fou_mem.sh 12000000 "$MODEL" /tmp/opencode/fib28.rose 2>/dev/null | tail -1)
[ "$fv" = "317811" ] && ok "C0 fib28=317811" || bad "C0 fib28=$fv"
# AOT natif : même valeur que l'interpréteur, binaire aarch64 exécuté.
bash scripts/garde_fou_mem.sh 12000000 "$MODEL" aot \
  rose/tests_aot8/programmes/01_arith.rose -o "$OUT/c0_aot.elf" \
  --target aarch64-unknown-linux-gnu > /dev/null 2>&1
av=$("$OUT/c0_aot.elf" 2>/dev/null)
iv=$("$REF" rose/tests_aot8/programmes/01_arith.rose 2>/dev/null)
[ -s "$OUT/c0_aot.elf" ] && [ "$av" = "$iv" ] && ok "C0 AOT natif=$av" \
  || bad "C0 AOT natif=$av / ref=$iv"
# WASM : module byte-identique à l'oracle sur un programme du sous-ensemble.
bash scripts/garde_fou_mem.sh 12000000 "$MODEL" transpile \
  tests_selfhost/t20_add.rose --cible wasm -o "$OUT/c0_t20.wasm" > /dev/null 2>&1
"$REF" transpile tests_selfhost/t20_add.rose --cible wasm -o "$OUT/ref_t20.wasm" > /dev/null 2>&1
cmp -s "$OUT/c0_t20.wasm" "$OUT/ref_t20.wasm" \
  && ok "C0 WASM byte-identique ($(wc -c < "$OUT/c0_t20.wasm") o)" \
  || bad "C0 WASM différent de l'oracle"

B="$REF"

# ── 2. point fixe émetteur ───────────────────────────────────────────────
log "=== 2. point fixe émetteur ==="
$B rir compilateur/emetteur.rose > /tmp/opencode/vf_p1.rir 2>/dev/null
$B compilateur/emetteur.rose compilateur/emetteur.rose > /tmp/opencode/vf_p2.rir 2>/dev/null
h1=$(md5sum /tmp/opencode/vf_p1.rir | cut -d' ' -f1)
h2=$(md5sum /tmp/opencode/vf_p2.rir | cut -d' ' -f1)
log "md5: $h1 / $h2"
[ "$h1" = "b3ed9262dd54776a6dbd07bc12368d0e" ] && [ "$h1" = "$h2" ] && ok "point fixe" || bad "point fixe"

# ── 3. AOT zéro refus ────────────────────────────────────────────────────
log "=== 3. AOT zéro refus ==="
bash rose/tests_aot8/verifier.sh > /tmp/opencode/vf_aot.log 2>&1
tail -2 /tmp/opencode/vf_aot.log | tee -a "$LOG"
grep -q '75 OK, 0 ECHEC' /tmp/opencode/vf_aot.log && ok "AOT 75/75" || bad "AOT"
grep -rq 'AOT indisponible\|utilisez `rose comp`' rose/src/ && bad "refus AOT présent" || ok "aucun refus AOT"

# ── 4. JIT 25/25 Cranelift ───────────────────────────────────────────────
log "=== 4. JIT 25/25 Cranelift ==="
nj=0; nt=0
for f in rose/tests_aot8/programmes/*.rose; do
  nt=$((nt+1))
  ROSE_TIMING=1 "$B" "$f" 2>&1 | grep -q 'principal=' && nj=$((nj+1))
done
log "$nj/$nt"
[ "$nj" -eq 25 ] && ok "JIT" || bad "JIT"

# ── 5. WASM + RVM ────────────────────────────────────────────────────────
log "=== 5. WASM + RVM ==="
"$B" transpile /tmp/opencode/fib28.rose --cible wasm -o /tmp/opencode/vf.wasm >/dev/null 2>&1
sz=$(wc -c < /tmp/opencode/vf.wasm 2>/dev/null || echo 0)
[ "$sz" -gt 0 ] && ok "WASM $sz o" || bad "WASM"
"$B" bc /tmp/opencode/fib28.rose -o /tmp/opencode/vf.bc >/dev/null 2>&1
r=$("$B" rvm /tmp/opencode/vf.bc 2>&1 | tail -1)
[ "$r" = "317811" ] && ok "RVM 317811" || bad "RVM: $r"

# ── 6. batterie AZ (Unicode) ─────────────────────────────────────────────
log "=== 6. batterie AZ (Unicode) ==="
AZDIR=/tmp/opencode/vf_az_$$
AZ_OUT="$AZDIR" bash tests_selfhost/lancer_az.sh > /tmp/opencode/vf_az.log 2>&1
awk -F'\t' 'NR>1{c[$4]++} END{for(k in c) print k, c[k]}' "$AZDIR/resultats.tsv" | tee -a "$LOG"
nok=$(awk -F'\t' 'NR>1 && $4=="CONFORME"{c++} END{print c+0}' "$AZDIR/resultats.tsv")
nbad=$(awk -F'\t' 'NR>1 && $4!="CONFORME"{c++} END{print c+0}' "$AZDIR/resultats.tsv")
[ "$nok" -ge 31 ] && [ "$nbad" -eq 0 ] && ok "AZ $nok CONFORME, 0 non-conforme" || bad "AZ $nok CONFORME, $nbad non-conforme"

# ── 7. self-host HOST-1 ──────────────────────────────────────────────────
log "=== 7. self-host HOST-1 ==="
"$B" compilateur/rose_natif.rose /tmp/opencode/fib28.rose > /tmp/opencode/vf_host.elf 2>/dev/null
m=$(xxd -p -l 8 /tmp/opencode/vf_host.elf 2>/dev/null | tr -d '\n')
chmod +x /tmp/opencode/vf_host.elf 2>/dev/null
out=$(/tmp/opencode/vf_host.elf 2>/dev/null)
[ "$m" = "7f454c4602010100" ] && [ "$out" = "317811" ] && ok "HOST-1 fib=317811" || bad "HOST-1: magic=$m out=$out"

# ── preuve : aucun appel cargo ───────────────────────────────────────────
nc=$(wc -l < "$CARGO_LOG")
[ "$nc" -eq 0 ] && ok "aucun appel cargo (journal vide)" || bad "$nc appel(s) cargo : $(cat "$CARGO_LOG")"

log "=== RESULTAT: $FAILED échec(s) ==="
[ "$FAILED" -eq 0 ]
