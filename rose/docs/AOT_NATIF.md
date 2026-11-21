# AOT natif intégré — état mesuré (21/11/2026)

`rose aot` et `rose orc` produisent maintenant un **vrai exécutable** à partir
d'un programme Rose, sans rien installer et sans aucun programme tiers.
La chaîne est entièrement dans le binaire :

```
source Rose → front (analyse) → rir (abaissement)
            → natif_emetteur (octets machine) → link_natif (ELF64 / PE64)
```

## Ce qui a changé

| Fichier | Changement |
|---|---|
| `src/cmd_llvm.rs` | `orc` / `aot` réécrits sur l'émetteur machine intégré ; `link` vérifie/copie un produit ; `llvm` (transpilation vers C) inchangé. |
| `src/rir.rs` | Nouvelle fonction `abaisser_principal_sortie` : le dernier `>> expression` devient la valeur de retour du code machine. `abaisser_principal` est inchangé (donc `rose run`, `rose rir`, `rose bc` gardent leur comportement). |
| `src/main.rs` | Câblage minimal : `aot`, `orc`, `link` vont directement dans `cmd_llvm` ; déclaration des modules `natif_emetteur` et `link_natif` dans le binaire. |
| `src/natif_emetteur.rs`, `src/link_natif.rs` | Aucune modification : leurs API publiques étaient déjà prêtes. |

La valeur du programme est son **dernier affichage** ; elle devient le code de
retour du processus (le `retour` n'existe qu'à l'intérieur des fonctions).
Sur Windows, `.exe` est ajouté au nom demandé s'il manque. `--target` choisit
l'architecture (`aarch64` / `x86_64`) et le format (`windows` → PE, sinon ELF)
et accepte les anciennes options sans effet (`-O1/-O2/-O3`, `--pgo-*`,
`--link`) pour ne pas casser les scripts.

## Preuves d'exécution (binaire `cargo build --release --bin rose`)

Programme 1 — `f(7) = 42` calculé directement :

```rose
n = 7
r = n * 6
>> r
```

```
$ rose aot simple42.rose -o p42
rose: p42 produit (exécutable natif aarch64, code de retour = valeur du programme)
$ ./p42 ; echo $?
42
```

Programme 2 — `fib(10) = 55` en boucle :

```rose
n = 10
a = 0
b = 1
tant_que (n > 0) {
    t = a + b
    a = b
    b = t
    n = n - 1
}
>> a
```

```
$ rose orc fib_iter.rose -o pfib
rose: pfib produit (exécutable natif aarch64, code de retour = valeur du programme)
$ ./pfib ; echo $?
55
```

`file` confirme des exécutables réels : `ELF 64-bit LSB executable, ARM
aarch64, statically linked`. Le câblage croisé a aussi été vérifié :
`--target x86_64-unknown-linux-gnu` → ELF x86-64,
`--target x86_64-pc-windows-msvc` → `PE32+ executable`, nommé `pwin.exe`.

`rose link p42 -o copie` copie le produit en lui gardant les droits
d'exécution (`./copie` → 42) ; sans `-o`, il confirme que le fichier est déjà
un exécutable natif.

## Refus propres (jamais de faux exécutable)

| Programme | Message |
|---|---|
| `>> f(7)` (appel de fonction) | `compilation native impossible : le programme ne rend pas de valeur calculable directement` + `utilisez rose comp…` |
| `>> 2^200` (grand nombre) | `compilation native impossible : certaines opérations demandent le moteur d'exécution` + `utilisez rose comp…` |
| `>> 2.5` (flottant) | `compilation native impossible : ce programme fait appel à des fonctions ou à des valeurs qui demandent le moteur d'exécution` + `utilisez rose comp…` |

Dans les trois cas, le fichier de sortie n'est PAS créé. Aucun message ne
cite un logiciel, une chaîne d'outils ni une étape d'installation.

## Validation

- `cargo build --release --bin rose` : OK.
- `cargo test --release` : 0 échec ; `cargo t --release` : 279 tests passés,
  0 échec (3 ignorés), dont 3 tests ajoutés pour le câblage AOT.
- `rose aot` (42) et `rose orc` (55) : exécutés, codes de retour vérifiés.
- `rose llvm simple42.rose -o simple42.c` : toujours un fichier C valide.
- Point fixe `b3ed9262dd54776a6dbd07bc12368d0e` ×2 (263 520 octets, `cmp` = 0)
  avec `rose rir compilateur/emetteur.rose` et
  `rose compilateur/emetteur.rose compilateur/emetteur.rose`.

## Limites connues

- Un seul corps principal par exécutable, sans appel à une autre fonction
  Rose : ces programmes restent couverts par `rose comp`.
- Les valeurs qui demandent le moteur (grands nombres, flottants, listes,
  entrées/sorties) sont refusées explicitement.
- L'émetteur produit un binaire statique non relogeable ; la cible croisée
  écrit le format demandé, sans garantie d'exécution sur cette machine.
