#!/usr/bin/env bash
# limites_mem.sh — audit (lecture seule) des scripts longs du dépôt.
#
# RÈGLE (à appliquer à toute compilation Rose longue) :
#   Une commande Rose qui compile un source .rose conséquent (emetteur.rose,
#   rose_natif.rose, boucles self-host C0/C1/C2, point fixe, ...) ne doit
#   JAMAIS être lancée sans plafond mémoire. Le temps (`timeout N`) ne protège
#   PAS du swap : il faut le garde-fou RSS.
#
#   Forme attendue :
#     scripts/garde_fou_mem.sh <MAX_RSS_KO> <commande Rose> [args...]
#   ou, pour une mesure reproductible :
#     scripts/prof_mem_rose.sh -- <commande Rose> [args...]
#
#   Valeur recommandée : 16777216 Ko (16 Gio) sur une machine à 121 Gio.
#   Le garde-fou tue la commande avec rc=75 dès que RSS > plafond, AVANT
#   l'épuisement du swap. Un `timeout 3600` seul laisse monter la RSS à 84 Gio.
#
# Ce script NE MODIFIE aucun fichier : il détecte et rapporte.
# Sortie : tableau + code 1 si au moins un script est non conforme (0 sinon).
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT" || exit 2

targets=()
for f in tests_selfhost/*.sh; do
  [ -f "$f" ] || continue
  [ "$f" = "tests_selfhost/limites_mem.sh" ] && continue   # outil d'audit, pas un script long
  targets+=("$f")
done
[ -f scripts/validation_finale.sh ] && targets+=("scripts/validation_finale.sh")

echo "=== Audit mémoire des scripts longs — $(date -u +'%Y-%m-%dT%H:%M:%SZ') ==="
echo "RÈGLE : toute compilation Rose longue passe par scripts/garde_fou_mem.sh"
echo "        (ou scripts/prof_mem_rose.sh qui l'appelle). 'timeout' SEUL ne suffit pas."
echo "Plafond recommandé : 16777216 Ko (16 Gio)."
echo
printf '%-42s %-12s %-10s %s\n' "SCRIPT" "GARDE-FOU" "APPELS" "VERDICT"
printf '%-42s %-12s %-10s %s\n' "------" "---------" "------" "-------"

bad=0
for f in "${targets[@]}"; do
  if grep -q 'garde_fou_mem\|prof_mem_rose' "$f"; then
    garde="oui"
  else
    garde="NON"
  fi
  # Lignes d'invocation Rose susceptibles d'être longues : timeout ... <binaire rose>
  # ou appel direct d'un compilateur .elf / d'un binaire Rose.
  n=$(grep -cE 'timeout[[:space:]]+[0-9]+.*(\$B|\$ROSE|\$SEED|\$C0|\$C1|\$SNAP|\.elf|rose/rose|\./rose)|\$B[[:space:]]+(rir|compilateur|\$?SRC)|\$B[[:space:]]+"?\$?SRC' "$f" 2>/dev/null)
  [ -n "$n" ] || n=0
  if [ "$garde" = "oui" ]; then
    verdict="CONFORME"
  else
    verdict="A PROTEGER"
    bad=$((bad+1))
  fi
  printf '%-42s %-12s %-10s %s\n' "$f" "$garde" "$n" "$verdict"
done

echo
echo "--- Scripts à protéger ($bad) ---"
for f in "${targets[@]}"; do
  grep -q 'garde_fou_mem\|prof_mem_rose' "$f" || echo "  $f"
done
echo
echo "--- Rappel ---"
echo "  * rc=75 = limite atteinte, arrêt propre par le garde-fou (à accepter, pas à masquer)."
echo "  * Ne jamais remplacer le plafond mémoire par un simple 'timeout' : la RSS peut"
echo "    monter à 84 Gio en 27 min et vider le swap avant l'échéance du timeout."
echo "  * Ce rapport est purement détectif : aucun script n'a été modifié."
exit $([ "$bad" -eq 0 ] && echo 0 || echo 1)
