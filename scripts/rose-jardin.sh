#!/usr/bin/env bash
# rose-jardin.sh — utilitaire des « jardins » (bibliothèques Rose en Rose pur).
#
#   scripts/rose-jardin.sh liste               catalogue des jardins + signatures
#   scripts/rose-jardin.sh init <nom>          crée jardins/<nom>.rose documenté
#   scripts/rose-jardin.sh verif <fichier|nom> contrôle Rose pur + `rose verif`
#   scripts/rose-jardin.sh verif-tous          vérifie tous les jardins
#
# Aucune dépendance : bash + le binaire `rose/rose` du dépôt. Pas de Rust,
# pas de GMP, pas de runtime externe (un jardin est du Rose pur).

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
ROSE="$ROOT/rose/rose"
JARDINS="$ROOT/jardins"

if [[ ! -x "$ROSE" ]]; then
  echo "✗ binaire Rose introuvable ou non exécutable : $ROSE" >&2
  exit 1
fi

usage() {
  cat <<'EOF'
Usage : scripts/rose-jardin.sh <commande> [argument]

  liste                Liste les jardins et leurs fonctions exportées.
  init <nom>           Crée jardins/<nom>.rose avec un squelette documenté.
  verif <fichier|nom>  Vérifie : Rose pur (ni Rust, ni FFI, ni dépendance)
                       + `rose verif` + convention « fonctions seules ».
  verif-tous           Applique `verif` à tous les jardins du catalogue.
  aide                 Affiche cette aide.

Exemples :
  scripts/rose-jardin.sh liste
  scripts/rose-jardin.sh init fibonacci
  scripts/rose-jardin.sh verif hachage
  scripts/rose-jardin.sh verif jardins/hachage.rose
EOF
}

# Résout un nom court (« hachage ») ou un chemin vers un fichier de jardin.
resoudre() {
  local cible="$1"
  if [[ -f "$cible" ]]; then
    printf '%s\n' "$cible"
  elif [[ -f "$JARDINS/$cible" ]]; then
    printf '%s\n' "$JARDINS/$cible"
  elif [[ -f "$JARDINS/$cible.rose" ]]; then
    printf '%s\n' "$JARDINS/$cible.rose"
  else
    return 1
  fi
}

cmd_liste() {
  local fichiers=()
  shopt -s nullglob
  fichiers=("$JARDINS"/*.rose)
  shopt -u nullglob

  if (( ${#fichiers[@]} == 0 )); then
    echo "Aucun jardin dans $JARDINS"
    return 0
  fi

  echo "Jardins disponibles ($ROOT) :"
  echo
  local total_f=0
  for f in "${fichiers[@]}"; do
    local nom lignes ast nb
    nom="$(basename "$f")"
    lignes="$(wc -l < "$f" | tr -d ' ')"
    echo "── $nom ($lignes lignes) ──"

    # Oracle : `rose ast` donne des lignes « fonction NAME », « param X T »,
    # « retour T » au niveau du programme. Repli sur grep si AST indisponible.
    ast="$("$ROSE" ast "$f" 2>/dev/null || true)"
    if [[ -n "$ast" ]]; then
      printf '%s\n' "$ast" | awk '
        /^fonction / { if (nom != "") printf "  • %s\n", sig; nom=$2; sig=$2 "("; sep=0 }
        /^param /    { if (nom != "") sig = sig (sep++ ? ", " : "") $2 ": " $3 }
        /^retour /   { if (nom != "") sig = sig ") -> " $2 }
        END          { if (nom != "") printf "  • %s\n", sig }
      '
    else
      echo "  (repli grep) :"
      grep -nE '^[[:space:]]*fn[[:space:]]+' "$f" | sed 's/^/  • /'
    fi

    nb="$(grep -cE '^[[:space:]]*fn[[:space:]]+' "$f" || true)"
    total_f=$(( total_f + nb ))
    echo
  done
  echo "Total : ${#fichiers[@]} jardins, $total_f fonctions exportées."
  echo "Importer : importer \"jardins/<nom>.rose\""
}

cmd_init() {
  local nom="${1:-}"
  if [[ -z "$nom" ]]; then
    echo "✗ init exige un nom (ex. : scripts/rose-jardin.sh init fibonacci)" >&2
    return 2
  fi
  # Un nom de jardin = identifiant simple : pas de /, pas de '..', pas d'espace.
  if [[ ! "$nom" =~ ^[a-z][a-z0-9_]*$ ]]; then
    echo "✗ nom invalide « $nom » (attendu : minuscules, chiffres, '_' ; pas de chemin)" >&2
    return 2
  fi
  local dest="$JARDINS/$nom.rose"
  if [[ -e "$dest" ]]; then
    echo "✗ jardin déjà existant : $dest" >&2
    return 1
  fi

  cat > "$dest" <<EOF
# jardins/$nom.rose — <description courte en une ligne>.
#
# ⚠ CE FICHIER EST UNE BIBLIOTHÈQUE (un « jardin »), pas un programme :
# il ne s'exécute pas seul. On le charge avec :
#
#     importer "jardins/$nom.rose"
#
# Règles des jardins :
#   • ROSE PUR : ni Rust, ni C, ni wasm, ni FFI, ni runtime externe.
#   • aucune instruction programme (\`>>\`) : uniquement des \`fn\`.
#   • calcul en entier exact (BigInt natif de Rose, zéro GMP).
#   • noms de fonctions en français, en snake_case.
#   • ajouter les tests dans jardins/tests/$nom.rose avec verifier(...).

# ── ma_fonction(x) : <ce qu'elle calcule> ──────────────────────────────────
# <une phrase sur l'algorithme et sa complexité.>
fn ma_fonction(x: entier) -> entier {
  retour x
}
EOF
  echo "✓ jardin créé : $dest"
  echo "  Vérifie-le : scripts/rose-jardin.sh verif $nom"
  echo "  Importe-le : importer \"jardins/$nom.rose\""
}

cmd_verif() {
  local cible="${1:-}"
  if [[ -z "$cible" ]]; then
    echo "✗ verif exige un fichier ou un nom de jardin" >&2
    return 2
  fi
  local f
  if ! f="$(resoudre "$cible")"; then
    echo "✗ jardin introuvable : $cible (cherché dans $JARDINS)" >&2
    return 1
  fi

  local rc=0
  echo "── verification de $f ──"

  # 1) Le fichier doit exister et être non vide.
  if [[ ! -s "$f" ]]; then
    echo "  ✗ fichier vide ou absent"
    return 1
  fi
  echo "  ✓ fichier présent ($(wc -l < "$f" | tr -d ' ') lignes)"

  # 2) Rose pur : refuser tout marqueur de Rust / FFI / dépendance externe.
  #    On teste des marqueurs de CODE, pas le simple mot « Rust » dans un
  #    commentaire (les jardins existants expliquent justement pourquoi).
  local motif='use std::|extern crate|#!\[|unsafe|@dll|@ffi|charger\(|appeler0\(|appeler1\(|appeler2\(|\.rs([^a-z]|$)'
  if grep -nE "$motif" "$f" >/dev/null; then
    echo "  ✗ marqueur Rust/FFI interdit détecté :"
    grep -nE "$motif" "$f" | sed 's/^/      /'
    rc=1
  else
    echo "  ✓ aucun marqueur Rust/FFI (pas de Rust, pas de C, pas de wasm)"
  fi

  # 3) Pas d'instruction programme : un jardin ne contient que des fonctions.
  if grep -nE '^[[:space:]]*>>' "$f" >/dev/null; then
    echo "  ✗ instruction '>>' interdite dans un jardin (ce n'est pas un programme) :"
    grep -nE '^[[:space:]]*>>' "$f" | sed 's/^/      /'
    rc=1
  else
    echo "  ✓ bibliothèque pure (aucune instruction '>>')"
  fi

  # 4) Imports : seuls les autres jardins .rose sont autorisés.
  local import dep
  while IFS= read -r import; do
    dep="$(printf '%s\n' "$import" | sed -E 's/.*importer[[:space:]]+"([^"]+)".*/\1/')"
    if [[ "$dep" != *.rose ]]; then
      echo "  ✗ import non-Rose interdit : $dep"
      rc=1
    else
      echo "  ✓ import Rose autorisé : $dep"
    fi
  done < <(grep -nE '^[[:space:]]*importer[[:space:]]+"' "$f" || true)

  # 5) Le vérificateur Rose doit accepter le fichier (sortie exacte « vérifié »).
  local sortie
  sortie="$("$ROSE" verif "$f" 2>&1)"
  local code=$?
  if [[ $code -eq 0 && "$sortie" == *"vérifié"* ]]; then
    echo "  ✓ rose verif : $sortie"
  else
    echo "  ✗ rose verif a échoué (code $code) : $sortie"
    rc=1
  fi

  # 6) Liste des fonctions exportées (information).
  local fns
  fns="$(grep -cE '^[[:space:]]*fn[[:space:]]+' "$f" || true)"
  echo "  → $fns fonction(s) exportée(s)"

  if [[ $rc -eq 0 ]]; then
    echo "✓ JARDIN VALIDE : $f (Rose pur, vérifié)"
  else
    echo "✗ JARDIN INVALIDE : $f"
  fi
  return $rc
}

cmd_verif_tous() {
  local fichiers=() f rc=0
  shopt -s nullglob
  fichiers=("$JARDINS"/*.rose)
  shopt -u nullglob
  if (( ${#fichiers[@]} == 0 )); then
    echo "Aucun jardin à vérifier dans $JARDINS"
    return 0
  fi
  for f in "${fichiers[@]}"; do
    cmd_verif "$f" || rc=1
    echo
  done
  if [[ $rc -eq 0 ]]; then
    echo "✓ Tous les jardins (${#fichiers[@]}) sont valides."
  else
    echo "✗ Au moins un jardin est invalide."
  fi
  return $rc
}

case "${1:-aide}" in
  liste)      shift; cmd_liste "$@" ;;
  init)       shift; cmd_init "$@" ;;
  verif)      shift; cmd_verif "$@" ;;
  verif-tous) shift; cmd_verif_tous "$@" ;;
  aide|-h|--help|help) usage ;;
  *) echo "✗ commande inconnue : ${1:-}" >&2; usage >&2; exit 2 ;;
esac
