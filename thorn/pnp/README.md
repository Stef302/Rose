# Thorn — prototype « P = NP ? » en fichiers `.thn` (jouet, pour le fun)

Prototype ajouté le 23/11/2026. **Aucun fichier existant n'est modifié** :
tout vit dans `thorn/pnp/`. Aucune publication, aucun commit.

**Ce n'est PAS une démonstration de P = NP ni de P ≠ NP.** Le vrai problème
reste ouvert. Ce prototype joue avec le **pilote Thorn** (`principal.rose`),
c'est-à-dire avec de vrais fichiers de preuve `.thn` — pas un script qui
appellerait le noyau directement. Les mêmes énoncés sont aussi traduits en
**Lean 4 core** (`thorn/pnp/lean/`) pour comparer les verdicts. Un appendix
`.rose` montre le mécanisme nu au niveau des termes.

## Les fichiers `.thn`

| Fichier | Verdict attendu | Contenu |
|---|---|---|
| `preuves/p_sous_np_jouet.thn` | **OK** (rc=0) | inclusion facile `P ⊆ NP` dans le modèle jouet |
| `preuves/certificat_jouet.thn` | **OK ×2** (rc=0) | certificat `0` vérifié ; forme générale via `plus_0_l` |
| `preuves/p_eg_np_sans_preuve.thn` | **ÉCHEC** (rc=1) | « P = NP » affirmé sans preuve : réflexivité refuse |
| `preuves/np_sous_p_sans_preuve.thn` | **ÉCHEC** (rc=1) | direction dure, vraie mais hors de portée des tactiques |

Rejouer le tout :

```
bash thorn/pnp/jouer_thn.sh
```

Sortie réelle (23/11/2026) :

```
OK p_sous_np_jouet
1 théorème(s), 1 réussi(s)
OK certificat_zero
OK certificat_general
2 théorème(s), 2 réussi(s)
ÉCHEC p_eg_np_sans_preuve : réflexivité : les deux membres diffèrent
ÉCHEC np_sous_p_sans_preuve : réflexivité : les deux membres diffèrent
BILAN .thn : 2 acceptées, 2 refusées — conformément à l'attendu.
```

## Le modèle jouet

| Symbole | Traduction dans les `.thn` |
|---|---|
| entrée | `Nat` |
| décideur `D` (classe P) | `Nat → Nat`, `0` = accepte |
| certificat `C` | `Nat → Nat` |
| vérificateur `V` (classe NP) | `V(w, c) = D w + c`, accepte si `D w + C w = C w` |
| `P(D)` | `pour_tout w, D w = 0` |
| `NP(V)` | `pour_tout w, D w + C w = C w` |

Théorème **prouvé** : `∀ D C, (∀ w, D w = 0) → ∀ w, D w + C w = C w`.
Sa preuve `.thn` utilise `trans`, `congr_plus_l` et `plus_0_l`, et passe par
`verifier()` + `convertible()` du noyau — aucun `OK` n'est codé en dur.

Théorème **vrai mais non prouvé par Thorn** : `∀ D C, (∀ w, D w + C w = C w) → ∀ w, D w = 0`
(annulation de l'addition à droite). C'est la caricature de la direction
dure `NP ⊆ P` : le checkur refuse la preuve proposée, ce qui ne dit rien sur
la vérité de l'énoncé.

**Limites documentées** : `plus_0_l` est une constante de signature (axiome),
pas un théorème démontré ; `thorn/src/tactiques.rose` n'a pas de lemme
d'annulation ni de tactique d'existence ; le pilot `--trace` casse sur les buts
à plusieurs Π (mesuré dans `thorn/docs/CAPACITES.md`), donc les commandes ci-dessus
ne l'utilisent pas.

## Test avec Lean 4 core (`thorn/pnp/lean/`)

Mêmes énoncés jouets traduits en **Lean 4.36 core**, sans Mathlib. Lean est
installé hors dépôt dans `/dev/shm/leanhome` (voir `thorn/docs/BENCH_LEAN.md` §4),
le script positionne l'environnement tout seul :

```
bash thorn/pnp/lean/jouer.sh
```

| Fichier Lean | Équivalent Thorn | Verdict réel |
|---|---|---|
| `lean/pnp.lean` — `p_sous_np_jouet` | `p_sous_np_jouet.thn` | compilé (rc=0) |
| `lean/pnp.lean` — `certificat_zero`, `certificat_general` | `certificat_jouet.thn` | compilés (rc=0) |
| `lean/pnp.lean` — `np_sous_p_jouet` | `np_sous_p_sans_preuve.thn` | **compilé en Lean** (`Nat.add_right_cancel`) alors que Thorn refuse |
| `lean/faux_pnp.lean` | `p_eg_np_sans_preuve.thn` | refusé par `rfl` (rc=1), comme Thorn refuse la réflexivité |
| `lean/pnp.lean` — `axiom p_eg_np_axiome` | appendix `.rose` | accepté des deux côtés : un axiome n'est pas une preuve |

Sortie réelle du refus Lean :

```
error: Tactic `rfl` failed: The left-hand side
  ∀ (n : Nat), 0 + n = n
is not definitionally equal to the right-hand side
  ∀ (n : Nat), n + 0 = n
```

**Leçon du croisement** : Thorn et Lean refusent la même affirmation non
justifiée ; en revanche la direction dure du jouet est à la portée de Lean core
(une simple annulation à droite) alors que la table de tactiques de Thorn n'a
pas le lemme. C'est une différence de bibliothèque, pas de puissance logique —
et toujours aucun résultat sur le vrai P vs NP.

## Appendix : accès direct au noyau (`.rose`)

`pnp.rose`, `jouer.sh` : la même idée écrite en termes Thorn (`noy_lam`,
`existe_intro`, `verifier`, `convertible`). Elle montre entre autres qu'un
axiome `P = NP` est accepté par le noyau — ce qui illustre qu'un vérificateur
juge la forme d'un terme, pas la vérité d'une supposition.

```
./rose/rose verif thorn/pnp/pnp.rose
./rose/rose thorn/pnp/pnp.rose
```

## Mémoire

**Mesuré le 23/11/2026** avec `scripts/prof_mem_rose.sh -m 1 -n 3`
(garde-fou 1 Gio, 3 passages, médianes) :

| Charge | pic RSS médian | rc |
|---|---:|---|
| `./rose/rose thorn/src/principal.rose thorn/pnp/preuves/p_sous_np_jouet.thn` | **79 312 Ko (~78 Mio)** | 0 |
| appendix `./rose/rose thorn/pnp/pnp.rose` | **34 644 Ko (~34 Mio)** | 0 |
| `lean thorn/pnp/lean/pnp.lean` (garde-fou 2 Gio) | **253 500 Ko (~248 Mio)** | 0 |

Swap inchangé dans les trois cas. Temps réels sans garde-fou (`/usr/bin/time -v`)
: 0,06 s pour l'appendix Rose, **0,32 s pour Lean**. Le pilote coûte ~45 Mio de
plus que l'accès direct au noyau (il charge lexeur + analyse + tactiques +
principal) ; Lean coûte ~3,2× le pilote Thorn, c'est le prix fixe d'un compilateur
complet — mais rien à voir avec les ~469 Mio mesurés dans `BENCH_LEAN.md` le
22/11 ni avec les ~5,2 Gio de `tests_algebre`. Comme toute mesure, c'est un
**constat daté** : re-mesurer si les binaires changent.
