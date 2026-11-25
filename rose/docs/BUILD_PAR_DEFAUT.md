# Build Windows par défaut — chemin sans cargo (auto-hébergé)

## Bascule

`scripts/build-windows-rebuild.sh` utilise désormais le **chemin sans cargo par
défaut** (`MODE=selfhost`). Le chemin cargo reste disponible explicitement avec
l'option `--cargo`.

| Option | Ce qui se passe |
|---|---|
| (aucune) | `MODE=selfhost` → appels C0 uniquement, **zéro** `cargo` |
| `--selfhost` | identique au défaut |
| `--cargo` | `MODE=cargo` → cross-build via `xwin` (compatibilité) |

## Commandes

```sh
bash scripts/build-windows-rebuild.sh            # défaut = selfhost, sans cargo
bash scripts/build-windows-rebuild.sh --cargo    # repli cargo
```

## Résultat mesuré (re-mesurable)

- Le build `--selfhost` produit les **5 artefacts PE32+ x86-64** dans
  `installateur/velopack/stage/` :
  - `Rose-win-Setup.exe`
  - `rose.exe`
  - `rose-shell.exe`
  - `rose-tray.exe`
  - `payload/rose.exe`
- Le générateur de ressources rend `RESSOURCES VALIDES` (33 garde-fous présents)
  via `installateur/velopack/stage/VERIFIER_RES.txt`.
- **Aucun** appel `cargo` dans le journal du build `--selfhost`.
- Le `rose.exe` final est le moteur complet émis par C0 (md5
  `9396d0bffa2caaef168e2f5a7bf918ac`, 1 413 632 o).

## Points de contrôle

- Point fixe émetteur : `rose rir compilateur/emetteur.rose` → md5
  `591dc79640d451a93c3124ec29f258d9`. (La valeur historique `b3ed9262…` était
  celle du fichier non vide ; après le passage en sous-ensemble C0, le RIR
  émis par le moteur est le plus court et sert de référence : le vérifier à
  chaque changement d'émetteur.)
- Parité self-host (harnais `scripts/verifier_parite_selfhost.sh`) : **141
  conformes / 0 divergent / 0 bloqué**, RC=0, avec le moteur complet émis par C0
  (`moteur.elf` 2 415 644 o). (L'ancien tally 110/28/3 provenait d'un moteur
  périmé ; rejoué sur le moteur frais → 141/141.)
- Émetteur PE x64 : la forme `lstvide` est désormais couverte (liste vide), ce
  qui débloque l'émission du moteur complet (`rose.exe` 1 770 496 o, PE32+).

## Retour arrière

Pour retrouver le comportement cargo d'origine, lancer avec `--cargo`.
