#!/bin/sh
# Construit le paquet du skill Rose (`rose-skill.zip`).
#
# Le skill est la source de vérité : `.opencode/skills/rose-dev/`. Ce script
# en fait une archive prête à déposer dans `.opencode/skills/` d'un autre
# agent, ou à installer depuis une release.
#
# Usage : sh installateur/skill/construire.sh [sortie.zip]
set -e

RACINE=$(cd "$(dirname "$0")/../.." && pwd)
SORTIE=${1:-"$RACINE/docs/telechargements/rose-skill.zip"}
STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT

mkdir -p "$STAGE/rose-dev"
cp "$RACINE/.opencode/skills/rose-dev/SKILL.md" "$STAGE/rose-dev/"
mkdir -p "$STAGE/rose-dev/references"
cp "$RACINE/.opencode/skills/rose-dev/references/"*.md "$STAGE/rose-dev/references/"
# L'exemple vivant de toute la syntaxe : il accompagne le skill.
if [ -f "$RACINE/rose/exemples/tour.rose" ]; then
  cp "$RACINE/rose/exemples/tour.rose" "$STAGE/rose-dev/"
fi

cat > "$STAGE/README.md" <<'EOF'
# Skill Rose

Skill pour écrire, corriger et faire tourner des programmes dans le langage
**Rose** (langage compilé en français).

## Contenu

```
rose-dev/
  SKILL.md              le skill principal (règles, pièges, architecture)
  references/
    syntaxe.md          toute la syntaxe Rose : types, instructions,
                        opérateurs, bibliothèque standard
    cli.md              la ligne de commande : run / latex / comp / orc /
                        link / info / console, REPL, AOT, installation
  tour.rose             exemple vivant de toute la syntaxe
```

## Installation

Pour **opencode** : dépose `rose-dev/` dans `.opencode/skills/`.

Pour tout autre agent qui lit des skills au format Markdown : `SKILL.md`
porte le frontmatter `name` / `description` attendu.

## Vérifier que ça marche

```bash
cd rose && ./rose exemples/tour.rose
```

Le programme doit afficher les formules LaTeX et les résultats attendus.
EOF

mkdir -p "$(dirname "$SORTIE")"
rm -f "$SORTIE"
(cd "$STAGE" && zip -r -9 "$SORTIE" rose-dev README.md >/dev/null)

echo "$(basename "$SORTIE") : $(stat -c%s "$SORTIE") o"
