#!/usr/bin/env python3
"""Publier une release Rose complète en une seule commande.

Usage :
    python3 scripts/publier-release.py <version> "<titre>" <fichier_notes> [--dry-run] [--no-build]

Le script enchaîne les 10 étapes décrites dans rose/docs/PUBLICATION.md :
compte GitHub, version, Cargo, marqueurs, builds, artefacts, manifeste,
site, git, release GitHub + vérification, puis un contrôle POST-publication
du SITE (`scripts/sync-site.py`) : parité GitHub ↔ site et contenu réellement
servi par Pages. `--dry-run` est strictement sans écriture (aucun build,
aucun commit, aucune publication).
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import html
import json
import os
import re
import shutil
import subprocess
import sys
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

RACINE = Path(__file__).resolve().parent.parent
ROSE = RACINE / "rose"
TELECHARGEMENTS = RACINE / "docs" / "telechargements"
RELEASES_VEL = RACINE / "installateur" / "velopack" / "Releases"
STAGE = RACINE / "installateur" / "velopack" / "stage"
WASM_TARGET = ROSE / "target" / "wasm32-unknown-unknown" / "release"

TOKEN_PATH = Path.home() / ".config" / "opencode" / ".gh_token"
LOGIN_ATTENDU = "Stef302"
DEPOT = "Stef302/Rose"
API = "https://api.github.com"
SITE_BASE = "https://stef302.github.io/Rose/telechargements/"

VERSION_RE = re.compile(r"^\d+\.\d+\.\d{1,2}$")

MOIS_FR = {
    1: "JANVIER", 2: "FÉVRIER", 3: "MARS", 4: "AVRIL", 5: "MAI", 6: "JUIN",
    7: "JUILLET", 8: "AOÛT", 9: "SEPTEMBRE", 10: "OCTOBRE", 11: "NOVEMBRE",
    12: "DÉCEMBRE",
}

# Noms génériques : ils appartiennent à la version courante, pas à une
# version historique (voir rose/docs/SITE_RELEASES.md, règle d'attribution).
GENERIQUES = {
    "releases.win.json", "Rose-win-Setup.exe", "rose.exe", "rose-shell.exe",
    "rose-tray.exe",
    "rose-aarch64-linux", "rose-aot-aarch64-linux", "rose-wasm.wasm",
    "rose-skill.zip",
}


class EchecPublication(Exception):
    """Arrêt immédiat, avec un message lisible et un code de retour non nul."""


class Rapport:
    def __init__(self, dry: bool) -> None:
        self.dry = dry
        self.changements: list[str] = []
        self.commandes: list[str] = []

    @staticmethod
    def etape(num: int, titre: str) -> None:
        print(f"\n=== Étape {num}/10 : {titre} ===", flush=True)

    @staticmethod
    def ok(msg: str) -> None:
        print(f"  ✓ {msg}", flush=True)

    @staticmethod
    def info(msg: str) -> None:
        print(f"    {msg}", flush=True)

    @staticmethod
    def avert(msg: str) -> None:
        print(f"  ! {msg}", flush=True)

    def simule(self, cmd: str) -> None:
        self.commandes.append(cmd)
        print(f"    [SIMULÉ] {cmd}", flush=True)

    @staticmethod
    def commande(cmd: str) -> None:
        print(f"    $ {cmd}", flush=True)

    def change(self, msg: str) -> None:
        self.changements.append(msg)
        print(f"    CHANGEMENT : {msg}", flush=True)


def masquer(texte: str, token: str) -> str:
    """Ne jamais laisser fuir le jeton dans un message d'erreur."""
    if token:
        texte = texte.replace(token, "<JETON-MASQUÉ>")
    return re.sub(r"(?i)(bearer\s+)\S+", r"\1<JETON-MASQUÉ>", texte)


def lire(chemin: Path) -> str:
    try:
        return chemin.read_text(encoding="utf-8")
    except OSError as exc:
        raise EchecPublication(f"lecture impossible : {chemin} ({exc})") from exc


def ecrire(chemin: Path, contenu: str, rap: Rapport, dry: bool, label: str) -> bool:
    """Écrit seulement si le contenu change. Retourne True si modifié."""
    try:
        avant = chemin.read_text(encoding="utf-8") if chemin.exists() else None
    except OSError as exc:
        raise EchecPublication(f"lecture impossible : {chemin} ({exc})") from exc
    if avant == contenu:
        rap.info(f"{label} : déjà à jour")
        return False
    if dry:
        rap.change(f"{label} (simulé, {len(avant or '')} → {len(contenu)} octets)")
    else:
        try:
            chemin.write_text(contenu, encoding="utf-8")
        except OSError as exc:
            raise EchecPublication(f"écriture impossible : {chemin} ({exc})") from exc
        rap.change(label)
    return True


def sha256_fichier(chemin: Path) -> str:
    h = hashlib.sha256()
    with chemin.open("rb") as f:
        for bloc in iter(lambda: f.read(1 << 20), b""):
            h.update(bloc)
    return h.hexdigest()


def cle_version(version: str) -> tuple[int, ...]:
    """Ordre compatible avec rose/src/velopack.rs::comparer_versions."""
    segments = []
    for partie in re.split(r"[.\-+]", version.lstrip("v")):
        chiffres = "".join(c for c in partie if c.isascii() and c.isdigit())
        segments.append(int(chiffres) if chiffres else 0)
    return tuple(segments)


def date_fr(jour: dt.date) -> str:
    return f"{jour.day:02d} {MOIS_FR[jour.month]} {jour.year}"


def taille_humaine(n: int) -> str:
    if n >= 1_000_000:
        return f"{n / 1_000_000:.1f} Mo".replace(".", ",")
    if n >= 1_000:
        return f"{n / 1_000:.0f} Ko".replace(".", ",")
    return f"{n} o"


def lire_notes(chemin: Path) -> list[str]:
    """Extrait les puces du fichier de notes (listes -, *, • ou 1.)."""
    if not chemin.is_file():
        raise EchecPublication(f"fichier de notes introuvable : {chemin}")
    lignes = lire(chemin).splitlines()
    puces: list[str] = []
    for ligne in lignes:
        s = ligne.strip()
        if not s or s.startswith("#"):
            continue
        m = re.match(r"^(?:[-*•]|\d+[.)])\s+(.*)$", s)
        if m:
            puces.append(m.group(1).strip())
    if not puces:
        puces = [s for s in (l.strip() for l in lignes) if s and not s.startswith("#")]
    if not puces:
        raise EchecPublication(f"aucune puce trouvée dans {chemin}")
    return puces


# ─── Étape 1 ────────────────────────────────────────────────────────────
def verifier_compte(rap: Rapport) -> str:
    rap.etape(1, "vérification du compte GitHub")
    if not TOKEN_PATH.is_file():
        raise EchecPublication(f"jeton introuvable : {TOKEN_PATH}")
    token = lire(TOKEN_PATH).strip()
    if not token:
        raise EchecPublication(f"jeton vide : {TOKEN_PATH}")
    req = urllib.request.Request(
        f"{API}/user",
        headers={
            "Authorization": f"Bearer {token}",
            "Accept": "application/vnd.github+json",
            "User-Agent": "rose-publier-release",
        },
    )
    try:
        with urllib.request.urlopen(req, timeout=20) as rep:
            donnees = json.load(rep)
    except (urllib.error.URLError, OSError, json.JSONDecodeError) as exc:
        raise EchecPublication(
            f"échec de l'appel /user : {masquer(str(exc), token)}"
        ) from exc
    login = donnees.get("login")
    if login != LOGIN_ATTENDU:
        raise EchecPublication(
            f"compte GitHub {login!r} refusé : seul {LOGIN_ATTENDU!r} est autorisé. "
            "Aucune action distante n'a été effectuée."
        )
    rap.ok(f"compte vérifié : {login} (jeton lu dans {TOKEN_PATH}, jamais affiché)")
    return token


# ─── Étape 2 ────────────────────────────────────────────────────────────
def valider_version(brut: str, rap: Rapport) -> str:
    rap.etape(2, "validation du format de version")
    version = brut[1:] if brut.startswith("v") else brut
    if not VERSION_RE.fullmatch(version):
        raise EchecPublication(
            f"version {brut!r} invalide : attendu x.y.z ou x.y.zz (ex. 0.1.7, 0.1.72), "
            "comparable à comparer_versions (rose/src/velopack.rs)"
        )
    rap.ok(f"version validée : {version}")
    return version


def version_courante() -> str:
    txt = lire(ROSE / "Cargo.toml")
    m = re.search(r"^version\s*=\s*\"([^\"]+)\"", txt, re.MULTILINE)
    if not m:
        raise EchecPublication("version introuvable dans rose/Cargo.toml")
    return m.group(1)


# ─── Étape 3 ────────────────────────────────────────────────────────────
def maj_cargo(ancienne: str, version: str, rap: Rapport, dry: bool) -> None:
    rap.etape(3, "mise à jour de rose/Cargo.toml et rose/Cargo.lock")

    cargo = ROSE / "Cargo.toml"
    txt = lire(cargo)
    motif = re.compile(r'(\[package\][\s\S]*?^version\s*=\s*")[^"]+(")', re.MULTILINE)
    if not motif.search(txt):
        raise EchecPublication("section [package]/version introuvable dans rose/Cargo.toml")
    nouveau_txt, n = motif.subn(lambda m: m.group(1) + version + m.group(2), txt, count=1)
    if n != 1:
        raise EchecPublication("substitution Cargo.toml impossible")
    ecrire(cargo, nouveau_txt, rap, dry, f"rose/Cargo.toml : {ancienne} → {version}")

    lock = ROSE / "Cargo.lock"
    txt = lire(lock)
    motif = re.compile(r'(\[\[package\]\]\nname = "rose"\nversion = ")[^"]+(")')
    if not motif.search(txt):
        raise EchecPublication('section [[package]] name = "rose" introuvable dans rose/Cargo.lock')
    nouveau_txt, n = motif.subn(lambda m: m.group(1) + version + m.group(2), txt, count=1)
    if n != 1:
        raise EchecPublication("substitution Cargo.lock impossible")
    ecrire(lock, nouveau_txt, rap, dry, f"rose/Cargo.lock : {ancienne} → {version}")
    rap.info("édition directe des deux fichiers (déterministe, aucun cargo check requis)")


# ─── Étape 4 ────────────────────────────────────────────────────────────
def maj_marqueurs(ancienne: str, version: str, rap: Rapport, dry: bool) -> None:
    rap.etape(4, "mise à jour des marqueurs de version courante (hors historique)")

    # README : uniquement le badge shields.io, pas les versions historiques
    # du tableau de bench ni les mentions de release v0.1.5.
    p = RACINE / "README.md"
    txt = lire(p)
    lignes = []
    nb = 0
    for ligne in txt.splitlines(keepends=True):
        if "img.shields.io/badge/version-" in ligne:
            ligne2, n = re.subn(
                r"(?<=badge/version-)" + re.escape(ancienne) + r"(?=-)",
                version, ligne,
            )
            nb += n
            lignes.append(ligne2)
        else:
            lignes.append(ligne)
    if nb:
        ecrire(p, "".join(lignes), rap, dry, f"README.md : badge shields.io → {version}")
    else:
        rap.info("README.md : aucun badge à la version courante")

    # docs/index.html : seulement AVANT la section CHANGELOG ; l'historique
    # (changelog + releases) est traité à l'étape 8.
    p = RACINE / "docs" / "index.html"
    txt = lire(p)
    borne = txt.find('id="page-changelog"')
    if borne < 0:
        raise EchecPublication('docs/index.html : section id="page-changelog" introuvable')
    prefixe, reste = txt[:borne], txt[borne:]
    motif = re.compile(r"(?<![\d.])V?" + re.escape(ancienne) + r"(?![\d.])")
    prefixe2, nb = motif.subn(lambda m: m.group(0).replace(ancienne, version), prefixe)
    if nb:
        ecrire(p, prefixe2 + reste, rap, dry,
               f"docs/index.html : {nb} marqueur(s) d'état courant → {version}")
    else:
        rap.info("docs/index.html : aucun marqueur d'état courant à mettre à jour")

    # Wiki miroir : marqueurs explicites uniquement.
    p = RACINE / "wikimiroir" / "Home.md"
    txt = lire(p)
    nouveau, nb = re.subn(
        r"(versioned as \*\*)" + re.escape(ancienne) + r"(\*\*)",
        lambda m: m.group(1) + version + m.group(2), txt,
    )
    if nb:
        ecrire(p, nouveau, rap, dry, f"wikimiroir/Home.md : versioned as → {version}")

    p = RACINE / "wikimiroir" / "CLI.md"
    txt = lire(p)
    nouveau, nb = re.subn(
        r'("Rose )' + re.escape(ancienne) + r'(")',
        lambda m: m.group(1) + version + m.group(2), txt,
    )
    if nb:
        ecrire(p, nouveau, rap, dry, f"wikimiroir/CLI.md : titre « Rose {version} »")


# ─── Étape 5 ────────────────────────────────────────────────────────────
def construire(version: str, rap: Rapport, dry: bool, no_build: bool) -> None:
    rap.etape(5, "construction des artefacts")
    wasm_target = str(WASM_TARGET.parent.parent)
    commandes = [
        (["sh", "scripts/build-windows-rebuild.sh"], RACINE),
        (["sh", "installateur/velopack/pack-linux.sh", version], RACINE),
        (["cargo", "build", "--release", "--target", "wasm32-unknown-unknown", "--lib"], ROSE),
    ]
    env_wasm = dict(os.environ, CARGO_TARGET_DIR=wasm_target)

    if dry:
        for cmd, cwd in commandes:
            prefixe = "CARGO_TARGET_DIR=<rose/target> " if cmd[0] == "cargo" else ""
            rap.simule(f"(cd {cwd.relative_to(RACINE)}) && {prefixe}{' '.join(cmd)}")
        rap.info("builds non exécutés en --dry-run (aucune écriture)")
        rap.info("sources d'artefacts attendues :")
        for chemin, _ in sources_artefacts(version):
            rap.info(f"  {'PRÉSENT' if chemin.is_file() else 'ABSENT '} {chemin.relative_to(RACINE)}")
        return

    if no_build:
        rap.ok("--no-build : construction sautée")
    else:
        for cmd, cwd in commandes:
            rap.commande(f"(cd {cwd.relative_to(RACINE)}) && {' '.join(cmd)}")
            env = env_wasm if cmd[0] == "cargo" else None
            try:
                p = subprocess.run(cmd, cwd=cwd, env=env, text=True,
                                   stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
            except OSError as exc:
                raise EchecPublication(f"impossible de lancer {' '.join(cmd)} : {exc}") from exc
            if p.returncode != 0:
                fin = "\n".join((p.stdout or "").splitlines()[-25:])
                raise EchecPublication(
                    f"échec (rc={p.returncode}) : {' '.join(cmd)}\n{fin}"
                )
            rap.ok(f"rc=0 : {' '.join(cmd)}")

    rap.info("vérification des artefacts construits :")
    for chemin, _ in sources_artefacts(version):
        if not chemin.is_file():
            raise EchecPublication(f"artefact attendu absent : {chemin}")
        rap.ok(f"{chemin.name} · {chemin.stat().st_size} octets")


def sources_artefacts(version: str) -> list[tuple[Path, str]]:
    return [
        (RELEASES_VEL / f"Rose-{version}-full.nupkg", f"Rose-{version}-full.nupkg"),
        (RELEASES_VEL / "releases.win.json", "releases.win.json"),
        (STAGE / "Rose-win-Setup.exe", "Rose-win-Setup.exe"),
        (STAGE / "rose.exe", "rose.exe"),
        (STAGE / "rose-shell.exe", "rose-shell.exe"),
        (STAGE / "rose-tray.exe", "rose-tray.exe"),
        (WASM_TARGET / "rose.wasm", "rose-wasm.wasm"),
    ]


# ─── Étape 6 ────────────────────────────────────────────────────────────
def copier_artefacts(version: str, rap: Rapport, dry: bool) -> None:
    rap.etape(6, "copie des artefacts et recalcul de SHA256SUMS")
    if not dry:
        TELECHARGEMENTS.mkdir(parents=True, exist_ok=True)
    paires = sources_artefacts(version)
    for src, nom in paires:
        dst = TELECHARGEMENTS / nom
        if dry:
            etat = "PRÉSENT" if src.is_file() else "ABSENT (serait construit)"
            rap.info(f"[SIMULÉ] {src.relative_to(RACINE)} → telechargements/{nom} ({etat})")
            continue
        if not src.is_file():
            raise EchecPublication(f"source d'artefact absente : {src}")
        if src.resolve() != dst.resolve():
            shutil.copy2(src, dst)
        rap.ok(f"{nom} · {dst.stat().st_size} octets · sha256 {sha256_fichier(dst)[:16]}…")

    ecrire(TELECHARGEMENTS / "VERSION", version + "\n", rap, dry,
           f"telechargements/VERSION → {version}")

    presents = sorted(
        f for f in TELECHARGEMENTS.iterdir()
        if f.is_file() and f.name not in {"SHA256SUMS", "releases.json"}
    )
    lignes = []
    for f in presents:
        lignes.append(f"{sha256_fichier(f)}  {f.name}\n")
    sommes = "".join(lignes)
    if dry:
        rap.change(f"telechargements/SHA256SUMS (simulé, {len(presents)} empreintes)")
        for l in lignes:
            rap.info(l.rstrip())
    else:
        (TELECHARGEMENTS / "SHA256SUMS").write_text(sommes, encoding="utf-8")
        rap.change(f"telechargements/SHA256SUMS ({len(presents)} empreintes calculées)")
    rap.info("aucune empreinte n'est inventée : toutes sont calculées sur les fichiers présents")


# ─── Étape 7 ────────────────────────────────────────────────────────────
def charger_ancien_manifeste() -> dict:
    p = TELECHARGEMENTS / "releases.json"
    if not p.is_file():
        return {"versions": []}
    try:
        return json.loads(lire(p))
    except (json.JSONDecodeError, EchecPublication) as exc:
        raise EchecPublication(f"releases.json illisible : {exc}") from exc


def assets_pour(version: str, presents: list[Path],
                anciens_noms: set[str] | None = None,
                deja_attribues: set[str] | None = None,
                generiques_autorises: set[str] | None = None) -> list[dict]:
    """Construit les assets depuis les fichiers RÉELLEMENT présents.

    Pour une version historique, on conserve l'attribution déjà publiée
    (preuve issue de la release GitHub) mais seulement pour les fichiers
    encore présents sur disque ; aucun nom n'est inventé. Les fichiers au
    nom générique ne sont attribués qu'à la version courante, et seulement
    s'ils étaient déjà annoncés comme assets de la version courante
    précédente (règle SITE_RELEASES) — jamais en double.
    """
    par_nom = {f.name: f for f in presents}
    retenus: set[str] = set()
    for nom in par_nom:
        m = re.search(r"-(\d+\.\d+\.\d{1,2})(?=[.\-]|$)", nom)
        v_fichier = m.group(1) if m else None
        if v_fichier == version:
            retenus.add(nom)
        elif v_fichier is None and version == version_courante_fichier():
            if nom in (deja_attribues or set()):
                continue
            if generiques_autorises is not None and nom not in generiques_autorises:
                continue
            retenus.add(nom)
    if anciens_noms:
        retenus |= {n for n in anciens_noms if n in par_nom}
    assets = []
    for nom in sorted(retenus):
        f = par_nom[nom]
        assets.append({
            "nom": nom,
            "url": SITE_BASE + urllib.parse.quote(nom),
            "sha256": sha256_fichier(f),
            "taille": f.stat().st_size,
        })
    return assets


def version_courante_fichier() -> str:
    p = TELECHARGEMENTS / "VERSION"
    if p.is_file():
        v = lire(p).strip()
        if VERSION_RE.fullmatch(v):
            return v
    return ""


def construire_manifeste(version: str, titre: str, notes: list[str], jour: dt.date) -> dict:
    ancien = charger_ancien_manifeste()
    anciennes = {
        v.get("version"): v for v in ancien.get("versions", []) if isinstance(v, dict)
    }
    presents = [f for f in TELECHARGEMENTS.iterdir()
                if f.is_file() and f.name not in {
                    "SHA256SUMS", "releases.json", "VERSION",
                }]

    # Les versions déjà publiées gardent leurs assets (preuve GitHub), mais
    # seulement s'ils sont encore présents. La version courante hérite des
    # noms génériques de l'entrée courante précédente — pas de doublon.
    historiques = [v for v in anciennes.values() if v.get("version") != version]
    deja_attribues: set[str] = set()
    for v in historiques:
        for a in v.get("assets", []):
            if isinstance(a, dict) and a.get("nom"):
                deja_attribues.add(a["nom"])
    derniere = max(historiques, key=lambda v: cle_version(str(v.get("version", "0"))),
                   default=None)
    generiques_autorises: set[str] | None = None
    if derniere is not None:
        generiques_autorises = {
            a["nom"] for a in derniere.get("assets", [])
            if isinstance(a, dict) and a.get("nom")
            and not re.search(r"-(\d+\.\d+\.\d{1,2})(?=[.\-]|$)", a["nom"])
        }

    detection: set[str] = set()
    for f in presents:
        for m in re.finditer(r"(\d+\.\d+\.\d{1,2})", f.name):
            detection.add(m.group(1))
    courante = version_courante_fichier()
    if courante:
        detection.add(courante)
    detection.add(version)
    # Les noms génériques ne créent pas de version fantôme : ils sont
    # attribués à la version courante du fichier VERSION.
    if not courante and detection == {version}:
        detection = set()

    versions = []
    for v in sorted(detection, key=cle_version, reverse=True):
        meta = anciennes.get(v, {})
        if v == version:
            entree = {
                "version": v,
                "tag": f"v{v}",
                "titre": titre,
                "date": jour.isoformat(),
                "changelog": notes,
            }
        elif meta:
            entree = {
                "version": v,
                "tag": meta.get("tag", f"v{v}"),
                "titre": meta.get("titre", f"Rose v{v}"),
                "date": meta.get("date", jour.isoformat()),
                "changelog": meta.get("changelog", []),
            }
        else:
            nupkg = TELECHARGEMENTS / f"Rose-{v}-full.nupkg"
            date_iso = (dt.date.fromtimestamp(nupkg.stat().st_mtime).isoformat()
                        if nupkg.is_file() else jour.isoformat())
            entree = {
                "version": v,
                "tag": f"v{v}",
                "titre": f"Rose v{v}",
                "date": date_iso,
                "changelog": [],
            }
        anciens_assets = {
            a.get("nom") for a in meta.get("assets", [])
            if isinstance(a, dict) and a.get("nom")
        }
        entree["assets"] = assets_pour(v, presents, anciens_assets,
                                       deja_attribues, generiques_autorises)
        versions.append(entree)

    return {"source": "site", "genere": jour.isoformat(), "versions": versions}


def regenerer_manifeste(version: str, titre: str, notes: list[str],
                        jour: dt.date, rap: Rapport, dry: bool) -> dict:
    rap.etape(7, "génération de releases.json depuis les fichiers présents")
    manifeste = construire_manifeste(version, titre, notes, jour)
    rap.ok(f"{len(manifeste['versions'])} version(s), "
           f"{sum(len(v['assets']) for v in manifeste['versions'])} asset(s)")
    for v in manifeste["versions"]:
        rap.info(f"  v{v['version']} · {v['date']} · {len(v['assets'])} asset(s) · {v['titre']}")
        for a in v["assets"]:
            rap.info(f"    {a['nom']} · {a['taille']} o · sha256 {a['sha256'][:16]}…")
        if all("github.com" not in a["url"] and
               a["url"].startswith(SITE_BASE) for a in v["assets"]):
            rap.info("    URLs : site uniquement (jamais GitHub)")
        else:
            raise EchecPublication(f"URL non conforme pour v{v['version']}")
    if dry:
        rap.change("telechargements/releases.json (simulé)")
        rap.info("aperçu de l'entrée courante : " + json.dumps(
            manifeste["versions"][0], ensure_ascii=False)[:1200])
    else:
        p = TELECHARGEMENTS / "releases.json"
        ecrire(p, json.dumps(manifeste, ensure_ascii=False, indent=2) + "\n",
               rap, dry, "telechargements/releases.json")
    return manifeste


# ─── Étape 8 ────────────────────────────────────────────────────────────
def evenement_changelog(version: str, titre: str, notes: list[str], jour: dt.date) -> str:
    puces = "\n".join(f"          <li>{html.escape(p)}</li>" for p in notes)
    return (
        '\n      <div class="event">\n'
        '        <div class="event-head">\n'
        f'          <span class="event-ver">V{html.escape(version)}</span>\n'
        f'          <span class="event-date">{date_fr(jour)}</span>\n'
        '        </div>\n'
        f'        <h4 class="event-title">{html.escape(titre)}</h4>\n'
        '        <ul>\n'
        f'{puces}\n'
        '        </ul>\n'
        '      </div>\n'
    )


def evenement_releases(version: str, titre: str, notes: list[str], jour: dt.date,
                       entrees: dict) -> str:
    puces = "\n".join(f"          <li>{html.escape(p)}</li>" for p in notes)
    assets = entrees.get("assets", [])
    boutons = []
    for a in assets:
        nom = html.escape(a["nom"])
        sha = html.escape(a["sha256"])
        cls = "btn btn-rouge" if not boutons else "btn btn-fantome"
        boutons.append(
            f'          <a class="{cls}" href="telechargements/{urllib.parse.quote(a["nom"])}" '
            f'download title="SHA-256 {sha}">{html.escape(nom.upper())} · '
            f'{taille_humaine(a["taille"])}</a>'
        )
    bloc_boutons = ("        <div class=\"rel-assets\">\n" +
                    "\n".join(boutons) + "\n        </div>\n") if boutons else ""
    nb = len(assets)
    return (
        '\n      <div class="event">\n'
        '        <div class="event-head">\n'
        f'          <span class="event-ver">V{html.escape(version)}</span>\n'
        f'          <span class="event-date">{date_fr(jour)}</span>\n'
        '          <span class="event-date" style="color:var(--vert)">ACTUELLE</span>\n'
        '        </div>\n'
        f'        <h4 class="event-title">{html.escape(titre)}</h4>\n'
        '        <ul>\n'
        f'{puces}\n'
        '        </ul>\n'
        f'{bloc_boutons}'
        f'        <p class="rel-sha">{nb} artefact(s) · SHA-256 complets dans '
        '<a class="vert" href="telechargements/SHA256SUMS">SHA256SUMS</a> '
        'et dans le manifeste.</p>\n'
        '      </div>\n'
    )


def inserer_apres_timeline(texte: str, debut_tag: str, insertion: str) -> str | None:
    """Insère juste après la balise <div class="timeline …"> suivant debut_tag."""
    pos = texte.find(debut_tag)
    if pos < 0:
        return None
    m = re.search(r'<div class="timeline[^"]*">', texte[pos:])
    if not m:
        return None
    point = pos + m.end()
    return texte[:point] + insertion + texte[point:]


def maj_html(version: str, titre: str, notes: list[str], jour: dt.date,
             manifeste: dict, rap: Rapport, dry: bool) -> None:
    rap.etape(8, "ajout de l'article CHANGELOG et rafraîchissement de RELEASES")
    p = RACINE / "docs" / "index.html"
    txt = lire(p)
    entree = next((v for v in manifeste["versions"] if v["version"] == version), {})
    ver_re = re.compile(r'event-ver">V' + re.escape(version) + r'(?![\d.])')

    # CHANGELOG
    pos = txt.find('id="page-changelog"')
    fin = txt.find("</section>", pos) if pos >= 0 else -1
    section = txt[pos:fin] if pos >= 0 and fin > pos else ""
    if pos < 0 or not section:
        rap.avert("section #page-changelog absente : insertion ignorée")
    elif ver_re.search(section):
        rap.info(f"CHANGELOG : V{version} déjà présent, aucun doublon ajouté")
    else:
        evt = evenement_changelog(version, titre, notes, jour)
        nouveau = inserer_apres_timeline(txt, 'id="page-changelog"', evt)
        if nouveau is None:
            rap.avert("timeline du CHANGELOG introuvable : insertion ignorée")
        else:
            ecrire(p, nouveau, rap, dry, f"docs/index.html : article CHANGELOG V{version}")
            txt = nouveau

    # RELEASES (le texte peut avoir changé ci-dessus)
    pos = txt.find('id="page-releases"')
    fin = txt.find("</section>", pos) if pos >= 0 else -1
    section = txt[pos:fin] if pos >= 0 and fin > pos else ""
    if pos < 0 or not section:
        rap.avert("section #page-releases absente : insertion ignorée")
    elif ver_re.search(section):
        rap.info(f"RELEASES : V{version} déjà présent, aucun doublon ajouté")
    else:
        # Retirer l'ancien marqueur ACTUELLE AVANT d'insérer le nouvel event,
        # sinon on supprimerait celui qu'on vient d'ajouter.
        marqueur = '<span class="event-date" style="color:var(--vert)">ACTUELLE</span>'
        idx = section.find(marqueur)
        if idx >= 0:
            section2 = section[:idx] + section[idx + len(marqueur):]
            txt = txt[:pos] + section2 + txt[fin:]
        evt = evenement_releases(version, titre, notes, jour, entree)
        nouveau = inserer_apres_timeline(txt, 'id="page-releases"', evt)
        if nouveau is None:
            rap.avert("timeline de RELEASES introuvable : insertion ignorée")
        else:
            ecrire(p, nouveau, rap, dry, f"docs/index.html : événement RELEASES V{version}")
    rap.info("idempotence : une version déjà présente n'est jamais dupliquée")


# ─── Étape 9 ────────────────────────────────────────────────────────────
def git_publier(version: str, rap: Rapport, dry: bool, fichiers: set[Path]) -> None:
    rap.etape(9, "commit et push sur main")
    try:
        branche = subprocess.run(["git", "rev-parse", "--abbrev-ref", "HEAD"],
                                 cwd=RACINE, text=True, stdout=subprocess.PIPE,
                                 stderr=subprocess.STDOUT, check=True).stdout.strip()
    except subprocess.CalledProcessError as exc:
        raise EchecPublication(f"branche git illisible : {exc}") from exc
    if branche != "main":
        raise EchecPublication(f"branche {branche!r} : la publication exige 'main'")
    rap.ok(f"branche : {branche}")
    relatifs = sorted(str(f.relative_to(RACINE)) for f in fichiers if f.exists() or dry)
    if not relatifs:
        rap.avert("aucun fichier à committer")
        return
    rap.info(f"fichiers de la release ({len(relatifs)}) : " + ", ".join(relatifs))
    rap.info("les modifications d'autres agents ne sont pas ajoutées")
    cmds = [
        ["git", "add", "--"] + relatifs,
        ["git", "commit", "-m", f"chore(release): publier v{version}"],
        ["git", "push", "origin", "main"],
    ]
    if dry:
        for c in cmds:
            rap.simule(" ".join(c))
        return
    for c in cmds:
        if c[1] == "commit":
            vite = subprocess.run(["git", "diff", "--cached", "--quiet"], cwd=RACINE)
            if vite.returncode == 0:
                rap.info("rien de nouveau dans l'index : commit sauté (idempotence)")
                continue
        rap.commande(" ".join(c))
        p = subprocess.run(c, cwd=RACINE, text=True,
                           stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        if p.returncode != 0:
            raise EchecPublication(
                f"échec git (rc={p.returncode}) : {' '.join(c)}\n"
                + "\n".join((p.stdout or "").splitlines()[-25:])
            )
        rap.ok(f"rc=0 : {c[0]} {c[1] if len(c) > 1 else ''}")


# ─── Étape 10 ───────────────────────────────────────────────────────────
def api_get(chemin: str, token: str) -> tuple[int, object]:
    req = urllib.request.Request(
        f"{API}{chemin}",
        headers={
            "Authorization": f"Bearer {token}",
            "Accept": "application/vnd.github+json",
            "User-Agent": "rose-publier-release",
        },
    )
    try:
        with urllib.request.urlopen(req, timeout=20) as rep:
            return rep.status, json.load(rep)
    except urllib.error.HTTPError as exc:
        return exc.code, {}
    except (urllib.error.URLError, OSError, json.JSONDecodeError) as exc:
        raise EchecPublication(f"appel API {chemin} impossible : {masquer(str(exc), token)}") from exc


def gh_publier(version: str, titre: str, notes: Path, assets: list[Path],
               token: str, rap: Rapport, dry: bool, force: bool) -> None:
    rap.etape(10, "création et vérification de la release GitHub")
    tag = f"v{version}"
    code, release = api_get(f"/repos/{DEPOT}/releases/tags/{tag}", token)
    if code == 200:
        if not force:
            raise EchecPublication(
                f"la release {tag} existe déjà ; utilisez --force pour l'écraser"
            )
        rap.avert(f"release {tag} existante : --force demandé, suppression puis recréation")
        if not dry:
            p = subprocess.run(["gh", "release", "delete", tag, "--yes"], cwd=RACINE,
                               env=dict(os.environ, GH_TOKEN=token), text=True,
                               stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
            if p.returncode != 0:
                raise EchecPublication(f"suppression de {tag} impossible : {p.stdout}")
    elif code != 404:
        raise EchecPublication(f"vérification de {tag} impossible : HTTP {code}")
    else:
        rap.ok(f"release {tag} inexistante : création autorisée")

    cmd = ["gh", "release", "create", tag, "--title", titre,
           "--notes-file", str(notes)] + [str(a) for a in assets]
    rap.info(f"assets locaux ({len(assets)}) : " + ", ".join(a.name for a in assets))
    if dry:
        rap.simule("GH_TOKEN=<jeton masqué> " + " ".join(cmd))
        rap.simule("vérification API : tag présent, nombre d'assets, SHA-256 du nupkg téléchargé")
        return

    rap.commande("GH_TOKEN=<jeton masqué> " + " ".join(cmd))
    p = subprocess.run(cmd, cwd=RACINE, env=dict(os.environ, GH_TOKEN=token),
                       text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    if p.returncode != 0:
        raise EchecPublication(
            f"gh release create a échoué (rc={p.returncode}) : "
            + masquer(p.stdout or "", token)
        )
    rap.ok(f"release {tag} créée")

    code, release = api_get(f"/repos/{DEPOT}/releases/tags/{tag}", token)
    if code != 200 or release.get("tag_name") != tag:
        raise EchecPublication(f"vérification API : tag {tag} introuvable (HTTP {code})")
    distants = release.get("assets", [])
    if len(distants) != len(assets):
        raise EchecPublication(
            f"vérification API : {len(distants)} assets distants au lieu de {len(assets)}"
        )
    rap.ok(f"API : tag {tag} présent, {len(distants)} assets")

    nupkg = next((a for a in assets if a.suffix == ".nupkg" or a.name.endswith(".nupkg")), None)
    if nupkg is None:
        rap.avert("aucun .nupkg local : vérification de hash distante sautée")
        return
    import tempfile
    with tempfile.TemporaryDirectory(prefix="rose-release-") as d:
        p = subprocess.run(
            ["gh", "release", "download", tag, "--pattern", nupkg.name,
             "--dir", d, "--clobber"],
            cwd=RACINE, env=dict(os.environ, GH_TOKEN=token), text=True,
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
        )
        if p.returncode != 0:
            raise EchecPublication(f"téléchargement de contrôle impossible : {masquer(p.stdout or '', token)}")
        telecharge = Path(d) / nupkg.name
        if not telecharge.is_file():
            raise EchecPublication(f"nupkg téléchargé introuvable : {telecharge}")
        local_hash, distant_hash = sha256_fichier(nupkg), sha256_fichier(telecharge)
        if local_hash != distant_hash:
            raise EchecPublication(
                f"SHA-256 du nupkg téléchargé != local : {distant_hash} != {local_hash}"
            )
        rap.ok(f"SHA-256 du nupkg téléchargé == local ({local_hash[:16]}…)")


# ─── Contrôle SITE post-publication ─────────────────────────────────────
def verifier_site(version: str, rap: Rapport, dry: bool) -> None:
    """Garantit que la release GitHub est AUSSI publiée sur le site.

    Le push de l'étape 9 déclenche GitHub Pages ; l'étape 10 crée la release
    GitHub. On vérifie alors (a) les hashes locaux, (b) que TOUTES les releases
    GitHub figurent dans `releases.json`, (c) que les fichiers réellement
    servis par Pages sont octet pour octet identiques aux fichiers locaux.
    Un écart est un ÉCHEC de publication, pas un avertissement.
    """
    rap.etape(11, f"contrôle du site après publication de v{version}")
    cmd = [
        sys.executable, str(RACINE / "scripts" / "sync-site.py"),
        "--verifier-local", "--verifier-parite", "--verifier-site",
        "--attendre", "30", "--essais", "6",
    ]
    if dry:
        rap.simule(" ".join(cmd))
        rap.simule("parité GitHub ↔ site + SHA-256 servi == SHA-256 local")
        return
    rap.commande(" ".join(cmd))
    p = subprocess.run(cmd, cwd=RACINE, text=True,
                       stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    print(p.stdout or "", end="", flush=True)
    if p.returncode != 0:
        raise EchecPublication(
            f"le site n'a PAS publié v{version} correctement (rc={p.returncode}) : "
            "release GitHub sans manifeste/site synchronisé"
        )
    rap.ok(f"site synchronisé : v{version} annoncée et servie (source primaire)")


# ─── Orchestration ──────────────────────────────────────────────────────
def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(
        prog="publier-release.py",
        description="Publier une release Rose complète en une seule commande.",
        epilog="Exemple : python3 scripts/publier-release.py 0.1.72 "
               '"Rose v0.1.72 — correctifs" notes.md --dry-run',
    )
    ap.add_argument("version", help="version, par exemple 0.1.72")
    ap.add_argument("titre", help="titre de la release")
    ap.add_argument("notes", type=Path, help="fichier de notes (puces - / * / • / 1.)")
    ap.add_argument("--dry-run", action="store_true",
                    help="tout vérifier et simuler 1-8, sans aucune écriture ni publication")
    ap.add_argument("--no-build", action="store_true",
                    help="ne pas reconstruire les artefacts (ils doivent déjà exister)")
    ap.add_argument("--force", action="store_true",
                    help="autoriser l'écrasement d'une release GitHub existante")
    args = ap.parse_args(argv)

    rap = Rapport(args.dry_run)
    if args.dry_run:
        print("╔═══════════════════════════════════════════════════════════╗")
        print("║  DRY-RUN — aucune écriture, aucun build, aucune publication ║")
        print("╚═══════════════════════════════════════════════════════════╝")
    try:
        token = verifier_compte(rap)
        version = valider_version(args.version, rap)
        ancienne = version_courante()
        rap.info(f"version courante dans rose/Cargo.toml : {ancienne}")
        notes = lire_notes(args.notes)
        rap.ok(f"{len(notes)} puce(s) lue(s) dans {args.notes}")
        jour = dt.date.today()

        maj_cargo(ancienne, version, rap, args.dry_run)
        maj_marqueurs(ancienne, version, rap, args.dry_run)
        construire(version, rap, args.dry_run, args.no_build)
        copier_artefacts(version, rap, args.dry_run)
        manifeste = regenerer_manifeste(version, args.titre, notes, jour, rap, args.dry_run)
        maj_html(version, args.titre, notes, jour, manifeste, rap, args.dry_run)

        fichiers = {
            ROSE / "Cargo.toml", ROSE / "Cargo.lock", RACINE / "README.md",
            RACINE / "docs" / "index.html", RACINE / "wikimiroir" / "Home.md",
            RACINE / "wikimiroir" / "CLI.md",
            TELECHARGEMENTS / "VERSION", TELECHARGEMENTS / "SHA256SUMS",
            TELECHARGEMENTS / "releases.json",
        }
        fichiers.update(TELECHARGEMENTS / nom for _, nom in sources_artefacts(version))
        git_publier(version, rap, args.dry_run, fichiers)

        entre = next(v for v in manifeste["versions"] if v["version"] == version)
        assets = [TELECHARGEMENTS / a["nom"] for a in entre["assets"]]
        gh_publier(version, args.titre, args.notes, assets, token,
                   rap, args.dry_run, args.force)
        verifier_site(version, rap, args.dry_run)
    except EchecPublication as exc:
        print(f"\nÉCHEC : {masquer(str(exc), locals().get('token', ''))}", file=sys.stderr)
        print("Aucune publication n'a été effectuée par cette étape.", file=sys.stderr)
        return 1

    print("\n╔═══════════════════════════════════════════════════════════╗")
    if args.dry_run:
        print("║  RAPPORT DRY-RUN — simulation terminée (11 étapes)        ║")
        print("╚═══════════════════════════════════════════════════════════╝")
        print(f"Changements simulés : {len(rap.changements)}")
        for c in rap.changements:
            print(f"  - {c}")
        print(f"Commandes simulées : {len(rap.commandes)}")
        for c in rap.commandes:
            print(f"  - {c}")
        print("Aucun fichier modifié, aucun commit, aucune release créée.")
    else:
        print(f"║  PUBLICATION TERMINÉE — v{version}                       ║")
        print("╚═══════════════════════════════════════════════════════════╝")
        print(f"Changements appliqués : {len(rap.changements)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
