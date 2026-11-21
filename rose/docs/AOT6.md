# AOT6 — runtime natif embarqué (vrai AOT, sans repli)

Date : 21/11/2026. Dépôt : `Stefan/ROSE`. **Aucun commit, aucune publication.**

## Ce que l'utilisateur a exigé

> « du vrai AOT de partout comme le JIT qui couvre tout, pas que quelques
> fonctions, ne masque pas sous un repli »

Avant AOT6, `rose aot` produisait un vrai binaire natif (~1 Ko) pour les
constantes, `fib`, `hasard(n)` nu, mais refusait (`AOT indisponible`) les
flottants calculés, `hasard(3)+1`, les bigints à exposant variable,
`lire_texte` et la cible PE. AOT6 supprime ces refus en intégrant **un
runtime Rose compilé en code machine dans le binaire émis**.

## Architecture — « compiler-rt » de Rose

Le runtime est écrit **une fois** en assembleur :

* `rose/runtime/aarch64.S` (AAPCS64, syscalls Linux 278/64/63),
* `rose/runtime/x86_64.s` (SysV, syscalls Linux 318/1/0).

`rose/runtime/generer.sh` les assemble (`aarch64-linux-gnu-as`,
`clang --target=x86_64-linux-gnu -c`) puis extrait les octets bruts :

| blob | taille | routines |
|---|---:|---|
| `aarch64.bin` | 2 384 o | `hasard`, `sortir`, `formater_flottant`, `lire_texte`, `bigint_puissance` |
| `x86_64.bin` | 2 186 o | idem |

Ces octets sont embarqués par `include_bytes!` dans
`src/runtime_natif.rs`, puis copiés **dans chaque exécutable AOT** par
`natif_emetteur::emettre_programme_aot`. À l'exécution, ce sont donc bien
des instructions machine qui tournent dans le binaire émis : ni
interpréteur Rust externe, ni moteur de 5,19 Mo recopié, ni trampoline.
Aucun outil externe n'est invoqué par `rose aot`.

Les routines ont la signature machine des fonctions Rose émises
`i64 f(base: *mut i64, tramp: *const Trampoline)` : elles sont dans la table
d'appels relatifs du segment, donc appelables par un simple `bl`/`call`.
Deux zones absolues sont réservées par le linker (ET_EXEC non-PIE,
base `0x40_0000`) : `SORTIE` à `0x40_2000` (`[len][texte]`) et le workspace
bigint à `0x40_3000` (`0x40_4000` pour les digits inversés).

Routines natives :

* **formateur IEEE-754 → décimal** : « plus court tournant » natif. Pour
  `p = 1..17`, `N = round(|d| × 10^k)`, correction par résidu FMA, puis
  reconstruction f64 et comparaison bit à bit. Émission `%g` de Rose
  (notation scientifique si `expo < -4` ou `expo >= p`, `".0"` si aucun
  point/`e`). Aucune table de Ryu, aucune libc.
* **hasard** : `getrandom` + splitmix64 + Lemire, **borne lue au runtime**
  (plus une immediat figée) : `hasard(3)+1` fonctionne.
* **bigint** : puissance par limbes binaires de 32 bits, conversion
  décimale par divisions répétées — exposition variable (`2^n`) couverte.
* **lire_texte** : `read(0, ...)` dans `SORTIE`, retour sous forme de chaîne.
* **sortir** : `write(1, SORTIE+8, len)` ; le stub appelle le formateur
  puis `sortir` avant `exit`.

## Résultats mesurés (machine aarch64 native, `cargo build --release`)

Tous les binaires sont des ELF64 ET_EXEC statiques, sans dépendance.

| Programme | aarch64 | x86-64 (qemu) | sortie | nature |
|---|---:|---:|---|---|
| `>> 7*6` | 1 040 o | 1 024 o | `42` | RIR natif (historique) |
| `>> 565#` | 1 280 o | — | 228 chiffres | valeur exacte mémorisée |
| `>> 2^200` | 1 096 o | 1 080 o | 61 chiffres exacts | valeur exacte mémorisée |
| `fib(28)` | 1 400 o | 1 368 o | `317811` | RIR natif, appels directs |
| `x = 2.5` / `>> x*2.0` | 20 480 o | 20 480 o\* | `5` | **formateur flottant natif** |
| `>> hasard(3)+1` | 20 480 o | 20 480 o | `[1,4)` | **runtime hasard natif** |
| `n = 200` / `>> 2^n` | 20 480 o | 20 480 o | 61 chiffres exacts | **runtime bigint natif** |
| `x = lire_texte("")` / `>> x` | 20 480 o | 20 480 o | la ligne lue | **runtime E/S natif** |
| `(2.5+1.5)*2.0` | 1 040 o | — | `8` | valeur exacte mémorisée |
| `>> "coucou"` | 1 048 o | — | `coucou` | valeur exacte mémorisée |

\* voir la limite x86-64 ci-dessous. 20 480 o = 20 Ko < 30 Ko.

Précision sur le premier objectif : l'utilisateur écrivait « affiche 5.0 »,
mais la sémantique de Rose est `chaine_de(Valeur::Flottant(5.0)) == "5"`
(partie entière, voir `run.rs:1182`). Le binaire affiche **octet pour octet
ce que `rose prog.rose` affiche**, donc `5`. Écrire `5.0` serait une divergence
de langage, pas une réussite.

Preuves d'exécution (aarch64) :

```
$ rose aot f1.rose -o f1.bin && ./f1.bin
5
$ rose aot h1.rose -o h1.bin && for i in 1 2 3 4 5; do ./h1.bin; done
1 1 2 3 3
$ rose aot b2.rose -o b2.bin && ./b2.bin
1606938044258990275541962092341162602522202993782792835301376
$ printf 'bonjour le monde\n' | ./entree.bin
bonjour le monde
```

Preuves que c'est du code machine (`objdump -D -b binary -m aarch64`) :

```
40015c: 1e690800  fmul d0, d0, d9       ← arithmétique IEEE native
40030c: 9e64002a  fcvtas x10, d1        ← conversion native
4001e8: 9bcf7d80  umulh x0, x12, x15    ← réduction de Lemire native
4009e0: 9acb09f1  udiv x17, x15, x11    ← bigint natif
400240: d4000001  svc #0                ← write/getrandom natifs
```

Le formateur a été validé séparément sur 16 valeurs (2,5 ; 5 ; 0,1+0,2 ;
1/3 ; 0,3 ; π ; 1e-5 ; 1e20 ; 123456789,123 ; -0,0 ; …) : sortie
**octet-identique** à `front::texte_flottant` / `run::chaine_de`.

## Couverture x86-64 (`qemu-x86_64-static`)

Vérifiés : constantes, `fib`, hasard composé, bigint à exposant variable,
`lire_texte` — tous en code machine x86-64 natif.

**Limite exacte (x86-64) : le formateur flottant est partiel.** Les routines
`{hasard,bigint,lire,sortir}` fonctionnent. `rt_formater_flottant` (x86)
produit le bon texte pour `2.5`, `π`, `(a*4.25)+0.5 = 6.875`, mais échoue
encore pour un résultat **entier** (`x*2.0 = 5.0` → octet nul) et pour les
valeurs `< 1` (`0.3` → `3.`, `1/3` → `3.33…`, `1e-5` → `1.`). Cause
technique : sur x86-64 l'adresse de retour est sur la pile (contrairement
à `x30` sur aarch64), donc le tampon de digits `[rsp+…]` du formateur
chevauchait l'adresse de retour ; après décalage du tampon, la copie finale
`%g` et le chemin « partie entière » ne sont pas encore alignés sur le bon
offset. Ce n'est **pas** un repli : aucun binaire x86 ne contient le moteur
Rose ; seul ce chemin de formatage reste à corriger. La version aarch64,
elle, est validée sur les 16 valeurs de référence.

## Ce qui reste hors périmètre (refus explicite, jamais un faux binaire)

* **Cible PE / Windows** : `rose aot --target ...windows...` refuse encore.
  Cause technique exacte : les routines du runtime appellent les syscalls
  Linux (`write`=1/64, `read`=0/63, `getrandom`=318/278). Un runtime Win64
  devrait remplacer ces syscalls par `kernel32!GetStdHandle`/`WriteFile`
  via la table d'imports du linker PE (`link_natif.rs` n'importe aujourd'hui
  que `ExitProcess`). Aucun binaire PE n'est produit sous un faux nom.
* **Valeurs composées** (listes, tables, tableaux) et primitives générales
  autres que `hasard`/`lire_texte` (`longueur`, factorielle de variable…)
  : l'abaissement RIR produit `AffectAst`, que l'émetteur machine ne sait
  pas traduire. Le refus dit explicitement quelle opération manque.
* **Flottants calculés sur x86-64** : voir la limite ci-dessus.

## Vérifications globales

* `cargo build --release` : OK.
* `cargo test --release` : **308 + 321 + 6 + 9 + 8 + 1 tests réussis,
  0 échec**.
* Point fixe `rose rir compilateur/emetteur.rose` : stable entre deux
  exécutions (`91ae26938101957d78605842323d27d68d39804364f81bd…`).
  `rir.rs` n'a pas été modifié.
* `rose link` accepte les produits (en-têtes ELF/PE valides).

## Fichiers touchés

* `rose/runtime/{aarch64.S,x86_64.s,generer.sh,aarch64.bin,x86_64.bin}` :
  runtime natif (sources + octets embarqués).
* `rose/src/runtime_natif.rs` (nouveau) : intégration, offsets, tags.
* `rose/src/natif_emetteur.rs` : `emettre_programme_aot`, arithmétique
  IEEE native, appels runtime, `ProgrammeEmis::runtime`.
* `rose/src/link_natif.rs` : zone de données du runtime, stub qui appelle
  formateur + sortie.
* `rose/src/cmd_llvm.rs` : chemin `orc_runtime`, primitives, analyse des
  types, suppression du refus pour les cas couverts.
* `rose/src/lib.rs` : déclaration du module.
* `rose/docs/AOT6.md` : ce document.
