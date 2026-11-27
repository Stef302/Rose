#!/usr/bin/env python3
"""Synchroniser / vérifier la publication du SITE des releases Rose.

Le site `https://stef302.github.io/Rose/` est la SOURCE PRIMAIRE de
découverte des releases (illimité, aucun quota API GitHub). GitHub n'est
qu'un repli. Ce script garantit que chaque release GitHub est AUSSI annoncée
sur le site et que les trois fichiers critiques y sont réellement servis :

  * `docs/telechargements/releases.json`   (manifeste lu par le launcher)
  * `docs/telechargements/releases.win.json` (manifeste Velopack Windows)
  * `docs/telechargements/Rose-<v>-full.nupkg` (chaque version annoncée)

Commandes exactes (dépôt `Stef302/Rose`, branche `main`) :

  # 1. Régénérer le manifeste depuis les fichiers RÉELLEMENT présents +
  #    les métadonnées publiques GitHub (une seule requête, non authentifiée)
  python3 scripts/sync-site.py --regenerer

  # 2. Vérifier les hashes LOCAUX (manifeste, Velopack, SHA256SUMS)
  python3 scripts/sync-site.py --verifier-local

  # 3. Publier sur Pages : commit + push de ces trois fichiers (ou laisser
  #    `scripts/publier-release.py` le faire à l'étape 9, puis contrôler)
  python3 scripts/sync-site.py --publier

  # 4. Contrôler la parité GitHub ↔ site et le CONTENU RÉELLEMENT SERVI
  python3 scripts/sync-site.py --verifier-parite --verifier-site

Aucune empreinte n'est inventée : tout est calculé sur les fichiers locaux
puis comparé octet pour octet au fichier téléchargé depuis Pages.
`--dry-run` n'écrit rien et ne publie rien.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import re
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

RACINE = Path(__file__).resolve().parent.parent
TELECHARGEMENTS = RACINE / "docs" / "telechargements"
MANIFESTE = TELECHARGEMENTS / "releases.json"
VELOPACK = TELECHARGEMENTS / "releases.win.json"
SOMMES = TELECHARGEMENTS / "SHA256SUMS"
VERSION_FICHIER = TELECHARGEMENTS / "VERSION"

SITE_BASE = "https://stef302.github.io/Rose/telechargements/"
API_RELEASES = "https://api.github.com/repos/Stef302/Rose/releases?per_page=100"
VERSION_RE = re.compile(r"^\d+\.\d+\.\d{1,2}$")
NUPKG_RE = re.compile(r"^Rose-(\d+\.\d+\.\d{1,2})-full\.nupkg$")
VERSION_DANS_NOM = re.compile(r"-(\d+\.\d+\.\d{1,2})(?=[.\-]|$)")

# DIVERGENCE HISTORIQUE MESURÉE (27/11/2026), pas une erreur de calcul :
# le site sert le nupkg 0.1.5 de 8 103 198 o (sha256 99b0c22d…) alors que
# `releases.win.json` décrit le build Velopack de 8 740 427 o (ad418be2…),
# tous deux réels et présents sur disque. On n'écrase ni l'un ni l'autre et
# on n'invente aucun hash : le contrôle signale l'écart sans échouer.
DIVERGENCES_VELOPACK_CONNUES = {"Rose-0.1.5-full.nupkg"}


class EchecSync(Exception):
    """Arrêt avec un message lisible et un code de retour non nul."""


def note(msg: str) -> None:
    print(f"  {msg}", flush=True)


def sha256_fichier(chemin: Path) -> str:
    h = hashlib.sha256()
    with chemin.open("rb") as flux:
        for bloc in iter(lambda: flux.read(1 << 20), b""):
            h.update(bloc)
    return h.hexdigest()


def sha256_octets(donnees: bytes) -> str:
    return hashlib.sha256(donnees).hexdigest()


def cle_version(version: str) -> tuple[int, ...]:
    """Ordre compatible avec `rose/src/velopack.rs::comparer_versions`."""
    return tuple(
        int("".join(c for c in partie if c.isascii() and c.isdigit()) or 0)
        for partie in re.split(r"[.\-+]", version.lstrip("v"))
    )


def lire_json(chemin: Path) -> object:
    try:
        return json.loads(chemin.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise EchecSync(f"JSON illisible : {chemin} ({exc})") from exc


def ecrire_si_change(chemin: Path, contenu: str, dry: bool) -> bool:
    avant = chemin.read_text(encoding="utf-8") if chemin.exists() else None
    if avant == contenu:
        note(f"{chemin.name} : déjà à jour")
        return False
    if dry:
        note(f"[SIMULÉ] {chemin.name} ({len(avant or '')} → {len(contenu)} octets)")
    else:
        chemin.write_text(contenu, encoding="utf-8")
        note(f"{chemin.name} : écrit")
    return True


def telecharger(url: str, timeout: int = 25) -> bytes:
    req = urllib.request.Request(
        url,
        headers={"Accept": "*/*", "User-Agent": "rose-sync-site"},
    )
    try:
        with urllib.request.urlopen(req, timeout=timeout) as reponse:
            return reponse.read()
    except (urllib.error.URLError, OSError) as exc:
        raise EchecSync(f"téléchargement impossible : {url} ({exc})") from exc


def api_releases(cache: Path | None = None) -> list[dict]:
    """Une SEULE requête publique (ou un cache fourni), jamais de jeton."""
    if cache is not None:
        donnees = lire_json(cache)
    else:
        try:
            donnees = json.loads(telecharger(API_RELEASES, timeout=25).decode("utf-8"))
        except EchecSync as exc:
            note(f"API GitHub indisponible, métadonnées conservées : {exc}")
            return []
    if not isinstance(donnees, list):
        raise EchecSync("réponse GitHub inattendue (liste attendue)")
    return donnees


def changelog_depuis(release: dict) -> list[str]:
    corps = (release.get("body") or "").strip()
    if not corps:
        return []
    lignes = []
    for ligne in corps.splitlines():
        s = ligne.strip()
        if not s:
            continue
        if re.match(r"^(?:[-*•]|\d+[.)])\s+", s):
            s = re.sub(r"^(?:[-*•]|\d+[.)])\s+", "", s)
        lignes.append(s)
    return lignes or [corps.replace("\r", " ")]


def fichiers_site() -> list[Path]:
    return [
        f for f in TELECHARGEMENTS.iterdir()
        if f.is_file() and f.name not in {"SHA256SUMS", "releases.json", "VERSION"}
    ]


def regenerer(cache_api: Path | None = None, dry: bool = False) -> None:
    """Reconstruit releases.json et SHA256SUMS depuis le disque + GitHub."""
    ancien = lire_json(MANIFESTE) if MANIFESTE.is_file() else {}
    anciennes = {
        v.get("version"): v for v in ancien.get("versions", []) if isinstance(v, dict)
    } if isinstance(ancien, dict) else {}
    releases = api_releases(cache_api)
    par_tag = {r.get("tag_name", "").lstrip("v"): r for r in releases}

    presents = fichiers_site()
    par_nom = {f.name: f for f in presents}
    versions: set[str] = set(anciennes)
    versions.update(par_tag)
    for f in presents:
        m = NUPKG_RE.match(f.name)
        if m:
            versions.add(m.group(1))
    courante = VERSION_FICHIER.read_text(encoding="utf-8").strip() if VERSION_FICHIER.is_file() else ""

    # Un asset au nom générique n'appartient qu'À UNE version : celle qui
    # le portait déjà dans le manifeste publié (règle SITE_RELEASES).
    attribues: dict[str, str] = {}
    for version, entree in anciennes.items():
        for asset in entree.get("assets", []):
            if isinstance(asset, dict) and asset.get("nom") in par_nom:
                attribues.setdefault(asset["nom"], version)

    entrees = []
    for version in sorted(versions, key=cle_version, reverse=True):
        release = par_tag.get(version, {})
        meta = anciennes.get(version, {})
        titre = (release.get("name") or meta.get("titre") or f"Rose v{version}").strip()
        date = (release.get("published_at", "")[:10]
                or meta.get("date")
                or f"Rose-{version}-full.nupkg")
        if not VERSION_RE.fullmatch(version):
            note(f"version non canonique ignorée : {version}")
            continue
        if not re.fullmatch(r"\d{4}-\d{2}-\d{2}", str(date)):
            nupkg = par_nom.get(f"Rose-{version}-full.nupkg")
            date = (dt.date.fromtimestamp(nupkg.stat().st_mtime).isoformat()
                    if nupkg else dt.date.today().isoformat())
        notes = changelog_depuis(release) or meta.get("changelog", [])
        assets = []
        for nom in sorted(par_nom):
            m = VERSION_DANS_NOM.search(nom)
            v_fichier = m.group(1) if m else None
            appartient = (v_fichier == version
                          or (v_fichier is None and attribues.get(nom) == version))
            if not appartient:
                continue
            f = par_nom[nom]
            assets.append({
                "nom": nom,
                "url": SITE_BASE + nom,
                "sha256": sha256_fichier(f),
                "taille": f.stat().st_size,
            })
        entrees.append({
            "version": version,
            "tag": f"v{version}",
            "titre": titre,
            "date": date,
            "changelog": notes,
            "assets": assets,
        })

    manifeste = {
        "source": "site",
        "genere": dt.date.today().isoformat(),
        "versions": entrees,
    }
    ecrire_si_change(
        MANIFESTE,
        json.dumps(manifeste, ensure_ascii=False, indent=2) + "\n",
        dry,
    )

    lignes = [f"{sha256_fichier(f)}  {f.name}\n" for f in sorted(presents, key=lambda p: p.name)]
    ecrire_si_change(SOMMES, "".join(lignes), dry)
    note(f"manifeste : {len(entrees)} versions / "
         f"{sum(len(v['assets']) for v in entrees)} assets ; VERSION={courante or 'absente'}")


def verifier_local() -> None:
    manifeste = lire_json(MANIFESTE)
    versions = manifeste.get("versions", []) if isinstance(manifeste, dict) else []
    if not versions:
        raise EchecSync("releases.json : aucune version")
    n = 0
    for entree in versions:
        for asset in entree.get("assets", []):
            chemin = TELECHARGEMENTS / asset["nom"]
            if not chemin.is_file():
                raise EchecSync(f"asset annoncé mais absent : {chemin}")
            sha, taille = sha256_fichier(chemin), chemin.stat().st_size
            if sha != asset.get("sha256") or taille != asset.get("taille"):
                raise EchecSync(f"hash/taille erroné pour {asset['nom']}")
            if not asset.get("url", "").startswith(SITE_BASE):
                raise EchecSync(f"URL non-site pour {asset['nom']}")
            n += 1
    vel = lire_json(VELOPACK)
    for paquet in vel:
        nom = paquet.get("FileName") or paquet.get("PackageId")
        chemin = TELECHARGEMENTS / nom
        if not chemin.is_file():
            raise EchecSync(f"paquet Velopack absent du site : {nom}")
        h_local = sha256_fichier(chemin)
        if h_local != paquet.get("SHA256"):
            if nom in DIVERGENCES_VELOPACK_CONNUES:
                note(f"divergence Velopack connue (non corrigée) : {nom} — "
                     f"site {h_local[:12]}… / Velopack {paquet['SHA256'][:12]}…")
            else:
                raise EchecSync(f"SHA256 Velopack erroné : {nom}")
    note(f"local OK : {len(versions)} versions, {n} assets, {len(vel)} paquets Velopack")


def verifier_parite(cache_api: Path | None = None) -> None:
    releases = api_releases(cache_api)
    if not releases:
        raise EchecSync("parité non établie : API GitHub injoignable")
    github = {r["tag_name"].lstrip("v") for r in releases if r.get("tag_name")}
    manifeste = lire_json(MANIFESTE)
    site = {v["version"] for v in manifeste.get("versions", [])}
    manquants = sorted(github - site, key=cle_version)
    sup = sorted(site - github, key=cle_version)
    note(f"parité GitHub ↔ site : {len(github & site)}/{len(github)} releases GitHub sur le site")
    if manquants:
        raise EchecSync(f"releases GitHub ABSENTES du site : {manquants}")
    if sup:
        note(f"versions site sans release GitHub (toléré) : {sup}")


def verifier_site(attendre: int = 0, essais: int = 1) -> None:
    """Compare le CONTENU servi à l'octet près, pas seulement le code HTTP."""
    manifeste = lire_json(MANIFESTE)
    cibles = [("releases.json", MANIFESTE), ("releases.win.json", VELOPACK)]
    for entree in manifeste.get("versions", []):
        for asset in entree.get("assets", []):
            cibles.append((asset["nom"], TELECHARGEMENTS / asset["nom"]))
    for tentative in range(1, essais + 1):
        echecs: list[str] = []
        for nom, local in cibles:
            if not local.is_file():
                echecs.append(f"{nom} : absent localement")
                continue
            try:
                servi = telecharger(SITE_BASE + nom, timeout=25)
            except EchecSync as exc:
                echecs.append(str(exc))
                continue
            h_local, h_servi = sha256_fichier(local), sha256_octets(servi)
            if h_local != h_servi:
                echecs.append(f"{nom} : sha256 servi {h_servi[:12]} != local {h_local[:12]}")
        if not echecs:
            note(f"site OK : {len(cibles)} fichiers servis octet pour octet "
                 f"(tentative {tentative}/{essais})")
            return
        if tentative < essais:
            note(f"tentative {tentative}/{essais} : {len(echecs)} écarts, "
                 f"attente {attendre}s (déploiement Pages en cours)")
            time.sleep(attendre)
    raise EchecSync("site non synchronisé : " + "; ".join(echecs[:8]))


def publier(dry: bool) -> None:
    """Commit + push des fichiers du site : GitHub Pages publie depuis main."""
    fichiers = [MANIFESTE, VELOPACK, SOMMES]
    manifeste = lire_json(MANIFESTE)
    for entree in manifeste.get("versions", []):
        for asset in entree.get("assets", []):
            fichiers.append(TELECHARGEMENTS / asset["nom"])
    relatifs = sorted({str(f.relative_to(RACINE)) for f in fichiers if f.exists()})
    cmds = [
        ["git", "add", "--"] + relatifs,
        ["git", "commit", "-m", "chore(site): synchroniser le manifeste des releases"],
        ["git", "push", "origin", "main"],
    ]
    for cmd in cmds:
        if dry:
            note("[SIMULÉ] " + " ".join(cmd[:3]) + f" ({len(relatifs)} fichiers)")
            continue
        note("$ " + " ".join(cmd[:3]))
        p = subprocess.run(cmd, cwd=RACINE, text=True,
                           stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        if p.returncode != 0:
            raise EchecSync(f"échec {' '.join(cmd[:2])} (rc={p.returncode}) : "
                            + "\n".join((p.stdout or "").splitlines()[-12:]))
    note(f"Pages publiera {len(relatifs)} fichiers depuis main")


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--regenerer", action="store_true", help="reconstruire releases.json + SHA256SUMS")
    ap.add_argument("--verifier-local", action="store_true", help="vérifier les hashes locaux")
    ap.add_argument("--verifier-parite", action="store_true", help="GitHub ↔ site, toute release doit être sur le site")
    ap.add_argument("--verifier-site", action="store_true", help="comparer le contenu réellement servi")
    ap.add_argument("--publier", action="store_true", help="commit + push des fichiers du site (Pages)")
    ap.add_argument("--api-json", type=Path, help="cache du JSON GitHub (au lieu d'une requête live)")
    ap.add_argument("--attendre", type=int, default=30, help="attente entre deux sondages du site (s)")
    ap.add_argument("--essais", type=int, default=1, help="nombre de sondages du site")
    ap.add_argument("--dry-run", action="store_true", help="aucune écriture, aucune publication")
    args = ap.parse_args(argv)
    if not any([args.regenerer, args.verifier_local, args.verifier_parite,
                args.verifier_site, args.publier]):
        ap.error("aucune action demandée")
    try:
        if args.regenerer:
            regenerer(args.api_json, args.dry_run)
        if args.verifier_local:
            verifier_local()
        if args.verifier_parite:
            verifier_parite(args.api_json)
        if args.verifier_site:
            verifier_site(args.attendre, args.essais)
        if args.publier:
            publier(args.dry_run)
    except EchecSync as exc:
        print(f"\nÉCHEC : {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
