# AOT2 — appels de fonction dans `rose aot` (sans runtime externe)

État mesuré le 21/11/2026 sur l'arbre de travail (vague 77).

## Problème

`rose aot fib.rose` refusait le programme récursif :

```
rose: compilation native impossible : le programme ne rend pas de valeur calculable directement
```

Deux causes :

1. `cmd_llvm::orc` n'appelait jamais `rir::declarer_rir` : `abstraire` ne
   produisait donc pas de `Eabstrait::AppelRir` pour `>> f(28)`.
2. Même avec un `AppelRir`, l'émetteur utilisait une trampoline, et
   `link_natif` n'embarquait qu'un stub rendant `0`. Le linker refusait donc
   tout programme avec `emis.appels` non vide (jamais de faux exécutable).

## Solution retenue : tout compiler dans un seul segment, appels directs

Aucun runtime externe, aucune trampoline, aucun linker tiers.

| Fichier | Changement |
|---|---|
| `src/natif_emetteur.rs` | Nouveau `emettre_programme(fonctions, arch)` : émet **toutes** les fonctions dans un seul `code`, résout chaque `AppelRir` en `bl`/`call` relatif (`AppelLie`, `ProgrammeEmis`). Le mode « lié » **refuse** toute expression qui demanderait la trampoline (division, flottant, …). |
| `src/cmd_llvm.rs` | Passe AOT `Lineariseur` (spécifique à `aot`, **sans toucher `rir.rs`**) : `f(n-1)+f(n-2)` devient `t0=n-1 ; t1=f(t0) ; t2=n-2 ; t3=f(t2) ; retour t1+t3`. Déclare les fonctions RIR, abaisse **toutes** les fonctions, écarte les fonctions non émettables inatteignables, refuse proprement une fonction non émettable appelée. |
| `src/link_natif.rs` | `Options::afficher_valeur` (ELF seulement) : le stub d'entrée écrit la valeur **en décimal sur stdout** avant le `exit`. Les valeurs > 255 ne sont plus invisibles. |
| `src/rir.rs` | **Inchangé** (point fixe `b3ed9262` préservé). |

### Détail machine

- Toutes les trames ont la même taille `frame_bytes = align16(max(n_slots+16)*8)`.
- À chaque appel : `sub sp, sp, frame_bytes` (aarch64, forme *extended register*
  `0xCB29…`, seul encodage avec `sp` comme `Rn`/`Rd`) ou `sub rsp, imm32`
  (x86-64) ; les arguments sont copiés dans la trame enfant ; `x0`/`rdi`
  reçoit la nouvelle base ; `bl`/`call rel32` vers la fonction cible ;
  la pile est restaurée **après** l'appel (x9/r10 rechargé : il est
  caller-saved). La récursion est donc naturelle, sans espace réservé fixe.
- `x19`/`rbx` (base) et `x20`/`r12` (trampoline inutilisée, mise à 0) sont
  callee-saved : les cadres parents restent valides pendant les appels.
- `corriger_sauts` est appelé pour chaque fonction (sinon les `si`/`retour`
  sautent à l'offset 0 — piège rencontré et corrigé).

## Preuves d'exécution

Binaire : `cargo build --release --bin rose` → `rose/target/release/rose`.

```
# Programme exact du signalement
fn f(n: entier) -> entier {
  si (n < 2) { retour n }
  retour f(n-1) + f(n-2)
}
>> f(28)

$ rose aot fib.rose -o fib
rose: fib produit (exécutable natif aarch64, code de retour = valeur du programme)
$ ./fib
317811
$ echo $?
115
```

⚠ **Le code de retour POSIX est un octet** : 317811 mod 256 = 115. Aucun
système ne peut rendre 317811 (ni 832040) dans un `exit()` ; c'est pour cela
que la valeur complète est aussi écrite sur stdout. Le binaire interprété/JIT
donne la même valeur : `rose fib.rose` → `317811`.

À propos du chiffre du signalement : avec la source donnée (`si n<2 retour n`),
`f(28) = 317811` ; **832040 est `f(30)`**. Preuve : `rose fib.rose` → 317811,
et `f(30)` compilé en AOT → 832040 stdout / 8 code de retour.

Autres cas exécutés :

| Programme | Résultat AOT aarch64 | Résultat AOT x86-64 (qemu-user) |
|---|---|---|
| `n=7 ; r=n*6 ; >> r` | `42` (rc 42) | `42` (rc 42) |
| fib récursif `>> f(7)` | `13` (rc 13) | `13` (rc 13) |
| fib récursif `>> f(28)` | `317811` (rc 115) | `317811` (rc 115) |
| appels croisés `pair`/`impair`, `>> pair(10)` | `10` (rc 10) | — |
| `>> f(30)` | `832040` (rc 40) | — |

`file` : ELF64 aarch64 / ELF64 x86-64 statiques ; `--target x86_64-pc-windows-msvc`
produit toujours un PE32+ valide (non exécuté ici).

### Refus propres (aucun fichier écrit)

| Programme | Message |
|---|---|
| `fn g(n)->entier { retour n div 3 } ; >> g(10)` | `compilation native impossible : la fonction g utilise des valeurs qui demandent le moteur d'exécution` |
| fonction non émettable **non appelée** + `>> 7*6` | compile quand même → `42` |
| appel à une fonction absente du segment | `fonction ... appelée par le programme n'est pas émettable` |

## Validation

- `cargo build --release --bin rose` : OK (0 erreur).
- `cargo test --release` : **285 tests (lib) + 280 (bin), 0 échec** (3 ignorés),
  dont les nouveaux tests `programme_emetteur_appel_direct_sans_trampoline`,
  `programme_emetteur_refuse_fonction_inconnue_et_fallback`,
  `linearisation_aot_rend_les_appels_emettables`,
  `stub_avec_affichage_est_plus_long_et_contient_write`.
- Tests existants `natif_emetteur` / `link_natif` verts (trampoline et
  stubs historiques inchangés quand `afficher_valeur = false`).
- Point fixe `b3ed9262dd54776a6dbd07bc12368d0e` ×2, 263 520 octets, `cmp`=0,
  stderr=0 des deux côtés (`rose rir …` et `rose … …`).
- Aucune régression JIT / interpréteur : `rose fib.rose` → 317811,
  `rose mutuel.rose` → 10.

## Limites

- Valeurs demandant le moteur (division rationnelle, flottants, listes,
  E/S, …) : la fonction est refusée ; les autres fonctions du programme
  restent compilables si elle est inatteignable.
- Variables globales lues/écrites depuis une fonction : refusées (pas de
  deuxième pointeur de trame). Le principal seul reste pris en charge.
- Plus de 4 arguments sur un appel : refusé (limite d'ABI de l'émetteur).
- Le stub décimal ne gère pas les valeurs négatives (non produites par le
  sous-ensemble AOT actuel).
- PE/Windows : format produit et vérifié, exécution non testée sur cette
  machine (pas de Windows ici) ; l'affichage décimal n'y est pas émis.
