#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════
#  Installeur Rose pour Linux (x86_64 / aarch64).
#
#  - Détecte une installation existante (par défaut /usr/local ou ~/.local).
#  - Propose : réinstaller (écraser), réparer, désinstaller, annuler.
#  - Installe le binaire rose + rose-aot (si présent) dans $PREFIX/bin.
#  - Ajoute $PREFIX/bin au PATH (via ~/.bashrc / ~/.profile) si absent.
#  - Enregistre le type MIME .rose (optionnel, si xdg-mime disponible).
# ═══════════════════════════════════════════════════════════════════════
set -euo pipefail

RACINE="$(cd "$(dirname "$0")/.." && pwd)"
ROSE_BIN="$RACINE/rose/rose"
ROSE_AOT="$RACINE/rose/target/release/rose-aot"
VERSION="${VERSION:-5.10.1}"

# ── détection du préfixe d'installation ──
detecter_prefix() {
    if [ -x /usr/local/bin/rose ]; then echo "/usr/local"; return; fi
    if [ -x "$HOME/.local/bin/rose" ]; then echo "$HOME/.local"; return; fi
    if [ -x /opt/rose/bin/rose ]; then echo "/opt/rose"; return; fi
    echo ""
}

PREFIX="$(detecter_prefix)"

if [ -n "$PREFIX" ]; then
    echo "Une installation de Rose a été détectée dans : $PREFIX"
    echo ""
    echo "Que souhaitez-vous faire ?"
    echo "  1) Réinstaller (écraser tous les fichiers)"
    echo "  2) Réparer (réinstaller les fichiers manquants)"
    echo "  3) Désinstaller"
    echo "  4) Annuler"
    read -r -p "Choix [1-4] : " choix
    case "$choix" in
        1|2)
            echo "Réinstallation en cours..."
            ;;
        3)
            echo "Désinstallation..."
            rm -f "$PREFIX/bin/rose" "$PREFIX/bin/rose-aot"
            rmdir "$PREFIX/bin" 2>/dev/null || true
            rmdir "$PREFIX" 2>/dev/null || true
            # retirer du PATH dans les fichiers de config
            for f in "$HOME/.bashrc" "$HOME/.profile" "$HOME/.zshrc"; do
                [ -f "$f" ] && sed -i "\|$PREFIX/bin|d" "$f"
            done
            echo "Rose désinstallé."
            exit 0
            ;;
        *)
            echo "Annulé."
            exit 0
            ;;
    esac
else
    # nouvelle installation : choisir le préfixe
    echo "Aucune installation détectée."
    if [ -w /usr/local/bin ] 2>/dev/null || [ "$(id -u)" = "0" ]; then
        PREFIX="/usr/local"
    else
        PREFIX="$HOME/.local"
    fi
    echo "Installation dans : $PREFIX"
fi

# ── installer les fichiers ──
mkdir -p "$PREFIX/bin"
# nettoyer les anciens binaires
rm -f "$PREFIX/bin/rose" "$PREFIX/bin/rose-20" "$PREFIX/bin/rose-aot"
# copier le nouveau binaire
if [ -f "$ROSE_BIN" ]; then
    cp "$ROSE_BIN" "$PREFIX/bin/rose"
    chmod +x "$PREFIX/bin/rose"
    echo "✓ rose installé"
else
    echo "⚠ binaire rose introuvable : $ROSE_BIN"
fi
if [ -f "$ROSE_AOT" ]; then
    cp "$ROSE_AOT" "$PREFIX/bin/rose-aot"
    chmod +x "$PREFIX/bin/rose-aot"
    echo "✓ rose-aot installé"
fi

# ── PATH ──
case ":$PATH:" in
    *":$PREFIX/bin:"*) ;;
    *)
        for f in "$HOME/.bashrc" "$HOME/.profile"; do
            if [ -f "$f" ] && ! grep -q "$PREFIX/bin" "$f"; then
                echo "export PATH=\"$PREFIX/bin:\$PATH\"" >> "$f"
            fi
        done
        export PATH="$PREFIX/bin:$PATH"
        ;;
esac

# ── type MIME (optionnel) ──
if command -v xdg-mime >/dev/null 2>&1; then
    mkdir -p "$HOME/.local/share/mime/packages"
    cat > "$HOME/.local/share/mime/packages/rose.xml" <<'XML'
<?xml version="1.0" encoding="UTF-8"?>
<mime-info xmlns="http://www.freedesktop.org/standards/shared-mime-info">
  <mime-type type="text/x-rose">
    <comment>Rose source file</comment>
    <glob pattern="*.rose"/>
  </mime-type>
</mime-info>
XML
    xdg-mime install "$HOME/.local/share/mime/packages/rose.xml" 2>/dev/null || true
    update-mime-database "$HOME/.local/share/mime" 2>/dev/null || true
    echo "✓ type MIME .rose enregistré"
fi

echo ""
echo "Rose $VERSION installé dans $PREFIX/bin"
echo "Vérification : rose --version"
"$PREFIX/bin/rose" --version
EOF
chmod +x installateur/install-linux.sh
echo "créé"