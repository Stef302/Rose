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
- Parité self-host (harnais `scripts/verifier_parite_selfhost.sh`) : **110
  conformes / 28 divergents / 3 bloqués** sur 141 programmes. Les 28
  divergents sont tous du sous-ensemble grands entiers / rationnels du C0, les
  3 bloqués = `t100_arg31` (segfault), `t107_hof_var` et `t55_liste_imbriquee`
  (refus identiques des deux côtés).

## Retour arrière

Pour retrouver le comportement cargo d'origine, lancer avec `--cargo`.
