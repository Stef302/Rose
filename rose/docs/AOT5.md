# AOT5 — vérité mesurée sur le binaire natif

Date : 21/11/2026. Dépôt : `Stefan/ROSE`. Aucun commit, aucune publication.

## Le mensonge initial (avant AOT5)

`rose aot prog.rose` produisait, pour `>> 2.5` et `>> hasard(10)`, un fichier
de **5 194 823 / 5 194 830 octets** : c'était le binaire `rose` entier (moteur
+ Cranelift + GUI + source collés) recopié par `comp_programme_aot`, avec le
message « exécutable AOT autonome ». Ce n'était PAS de la compilation anticipée
: c'était un interpréteur déguisé. Le message l'admettait (« autonome »), mais
la commande `aot` laissait croire le contraire.

**Décision AOT5 : ce repli est supprimé.** `rose aot` / `rose orc` ne copient
plus jamais le moteur. Quand le code machine ne peut pas être produit, ils
disent explicitement :

```
rose: AOT indisponible pour ce programme : <raison>
rose: aucun exécutable produit. `rose comp` donne un exécutable autonome
      (moteur + source), ce n'est PAS un AOT.
```

et sortent avec un code non nul. Aucun fichier n'est écrit. Le besoin
d'exécutable « marche partout » reste couvert par `rose comp`, sous son vrai
nom.

## Ce qui est un VRAI AOT natif (mesuré)

Machine de mesure : aarch64 (ASUS GX10), `cargo build --release`, binaire
`rose/target/release/rose`. Taille du moteur : 5 194 800 octets — tout binaire
AOT ci-dessous est **au moins 4 000× plus petit**.

| Programme | Taille | Sortie | Nature |
|---|---:|---|---|
| `>> 7*6` | 1 040 o | `42` | natif (AOT3, constante mémorisée) |
| `>> 565#` | 1 280 o | 228 chiffres | natif (AOT3, valeur exacte) |
| `n = 7` / `>> n * 2` | 1 128 o | `14` | natif (RIR → machine, appels directs) |
| `>> 2.5` | **1 040 o** | `2.5` | **natif** (AOT5, flottant constant mémorisé) |
| `>> hasard(10)` | **1 192 o** | valeur aléatoire 0–9 | **natif** (AOT5, splitmix64 en octets) |

Preuves :

- `>> 2.5` : le texte `2.5\n` est calculé à la compilation par
  `run::chaine_de` (le même formateur que l'interpréteur) puis stocké dans
  `.rodata` ; le stub n'exécute qu'un `write(1, addr, len)` syscall. Aucune
  instruction flottante n'est nécessaire à l'exécution : la valeur est
  **déterminée à la compilation**, donc l'absence de formateur IEEE→décimal
  en machine n'est pas un repli, c'est une optimisation légitime.
- `>> hasard(10)` : `natif_emetteur::emettre_hasard` écrit lui-même les octets
  de `getrandom` (syscall 278 aarch64 / 318 x86-64), de splitmix64
  (`C=9E3779B97F4A7C15`, `M1=BF58476D1CE4E5B9`, `M2=94D049BB133111EB`) et de
  la réduction de Lemire (`umulh` aarch64 / `mul` x86-64). Miroir exact de
  `run::rng_tirer` + `run::hasard`. Le résultat est rendu dans x0/rax et le
  stub l'imprime en décimal par le syscall `write` déjà présent. Aucune
  trampoline, aucun appel au moteur, aucune chaîne de nom.
  Vérifié : 20 tirages consécutifs donnent des valeurs variées dans [0,10) ;
  `hasard(2)` ne rend que 0 ou 1.
- Encodage croisé : `llvm-objdump -d --triple=aarch64` et `--triple=x86_64`
  montrent les séquences attendues (`svc #0` + `umulh` / `syscall` + `mul`).
  Un bug d'encodage x86 (`mov rbx,r9` au lieu de `mov [rbx],r9`, ModRM mod=11
  au lieu de 00) a été trouvé et corrigé grace à ce désassemblage.

## Ce qui n'est PAS un AOT natif — refus explicite

Ces programmes **refusent** désormais (aucun fichier produit, message
« AOT indisponible »), au lieu de livrer 5,19 Mo :

| Programme | Raison (preuve) |
|---|---|
| `x = 2.5` / `>> x * 2.0` | Flottant non constant. Le RIR sait abstraire `Flottant` et `+`, mais `link_natif` ne contient **que** un formateur i64→décimal (`sequence_affichage_*`). Il n'existe aucun formateur IEEE→décimal en code machine dans l'émetteur. Produire `3.5` exigerait de réimplémenter l'algorithme de plus court tournant (Ryu/Dragon4) en assembleur ; ce n'est pas fait, donc on refuse. |
| `>> hasard(3) + 1` | `hasard` est une primitive du catalogue : `rir::abstraire` rend `None` pour un appel non déclaré RIR. Seule la forme nue `>> hasard(n)` avec `n` littéral est émise. |
| `n = 10` / `>> hasard(n)` | borne variable → impossible à intégrer comme immédiat sans évaluateur d'expressions. |
| `>> chaine("coucou")` | primitive non native. |
| Programme avec E/S au clavier | aucun syscall `read` + parseur numérique machine dans `link_natif` ; refus. |
| Cible Windows/PE | le linker PE n'a pas encore de stub d'affichage natif (WriteFile/IAT présent mais non câblé pour les valeurs) ; refus. |

La sortie (`>>`) elle-même est native depuis toujours : `link_natif` émet un
`write` syscall Linux (ou `ExitProcess` via l'IAT sur PE). L'entrée (clavier)
reste hors du sous-ensemble et refuse honnêtement.

## Vérifications

- `cargo build --release` : OK.
- `cargo test --release` : **0 échec** (296 + 306 + 6 + 9 + 8 + 1 tests,
  3 ignorés). Un « flake » parallèle initial dans `setup_core` (temp dirs
  partagés) disparaît en rejeu et au compteur `--test-threads=1` : aucun
  lien avec AOT5.
- Point fixe `b3ed9262dd54776a6dbd07bc12368d0e` **×2** : identique avant et
  après (`rose rir compilateur/emetteur.rose` et
  `rose compilateur/emetteur.rose compilateur/emetteur.rose`), 263 520 o,
  stderr vide. `rir.rs` n'a pas été modifié.
- Messages honnêtes : succès = « produit (exécutable natif …) » ; échec =
  « AOT indisponible pour ce programme » + aucun fichier.

## Fichiers touchés

- `rose/src/cmd_llvm.rs` : suppression du repli, constantes affichables
  généralisées (flottants inclus), détection `hasard(n)`, tests.
- `rose/src/natif_emetteur.rs` : `emettre_hasard` (aarch64 + x86-64), tests.
- `rose/src/main.rs` : aide CLI corrigée, ancien chemin `comp_programme_aot`
  retiré.
- `rose/docs/AOT5.md` : ce document.
