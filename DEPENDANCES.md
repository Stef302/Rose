# Dépendances externes — inventaire réel et route d'indépendance

Intention consignée le 21 octobre 2026 : « être plus indépendant — même si
on intègre nativement, on crée notre propre bibliothèque qui fait le même
travail que GMP, pareil pour les autres dépendances externes ».

Ce document est la photographie mesurée qui rend cette décision chiffrable.
Il sera tenu à jour à chaque jalon.

## 1. Photographie (v3.13.1, commit 4c04874)

Un seul lien tiers est expédié avec les moteurs : **GMP**. Le reste des
liens d'édition est la bibliothèque système, et le reste n'est que des
outils de fabrication (jamais livrés).

### Liens d'édition des trois moteurs natifs

| Bibliothèque | C++ | Rust | Zig | Dans le binaire AOT ? | Ce qu'elle fait pour Rose |
|---|---|---|---|---|---|
| GMP (libgmp ; gmpxx pour C++) | ✓ | ✓ (FFI) | ✓ (FFI) | **oui** (rose_runtime) | entiers géants (`rose_ent`), rationnels, pgcd/ppcm, `est_premier`, conversions ⇔ flottant, `fibonacci` grand n (Rust), exponentiation modulaire (Rust) |
| libm | ✓ | ✓ | ✓ | oui | `flottant(puissance)` (`pow`, `floor`), `racine`, formatage |
| libpthread | ✓ | ✓ | — | non | thread à grande pile (`fil_pipeline`) du moteur C++ |
| libdl | ✓ | ✓ | — | non | FFI `charger()`/`fermer()` (`dlopen`/`dlsym`) |
| libstdc++ | ✓ | — | — | oui | conteneurs, chaînes, flux du moteur C++ |
| msvcrt (Windows) | ✓ | ✓ | ✓ | oui | exécutif Windows (FFI `msvcrt::pow` testé) |

GMP Windows est une archive COFF (`libgmp-win.a`) croisée avec Zig ; GMP
Linux est statique (`libgmp.a`) — les moteurs livrés n'empruntent donc
rien à l'installation du voisin, mais **contiennent du GMP**.

### Outils de fabrication (non livrés)

`g++ -O3` (compilation AOT — le cœur de `rose build`), `zig` (moteur Zig
+ tous les builds croisés Windows/x64), `cargo` (moteur Rust), `julia`
(moteur vivant), `qemu` (validation croisée x64), `python3` (suites
différentielles). Un binaire Rose compilé sur la machine de l'utilisateur
exige un g++ + GMP système — c'est la dépendance la plus visible du AOT.

## 2. La surface GMP réellement appelée — le cahier des charges

Décompte des sites d'appel (mesuré) : **C++ 1277 · Rust 150 · Zig 143**.
La surface en symboles est petite — c'est ce qui rend l'aventure sérieuse :

- **2 types** : `mpz_t` (entier de précision arbitraire, signe signé),
  `mpq_t` (rationnel normalisé) ; classes C++ `mpz_class`, `mpq_class`.
- **44 fonctions mpz** (union des trois moteurs mesurée — chacun n'en
  appelle qu'une partie) :
  `init clear set set_ui set_si set_str set_d get_ui get_si get_str get_d
   sgn cmp cmp_ui abs neg add add_ui sub mul mul_ui mul_2exp pow_ui
   ui_pow_ui fdiv_q fdiv_r fdiv_ui tdiv_q mod divrem* gcd lcm tstbit
   fits_slong_p fits_ulong_p fits_sint_p sizeinbase export import
   probab_prime_p` (+ `powm`, `fib_ui` côté Rust seulement, `cmp_si` et
  `init2` côté Zig seulement).
- **15 fonctions mpq** :
  `init clear set set_str set_d set_z get_d get_str sgn cmp add sub mul
   div canonicalize`.
- **Formatage** : `gmp_printf` avec `%Zd` (affichage des grands entiers).

Régénérer cet inventaire : `grep -oh "mpz_[a-z_0-9]*\|mpq_[a-z_0-9]*"
moteur/cpp/src/*.{cpp,h} moteur/rust/src/*.rs moteur/zig/src/*.zig |
sort -u` (retirer ensuite `mpz_t/mpz_ptr/mpz_class/…` purement déclaratifs).

Sémantiques à reproduire **à l'octet près**, pas seulement des chiffres :

1. `fdiv` (plafond vers −∞) contre `tdiv` (troncature vers 0) — les deux
   sont utilisés, et le choix de l'une ou l'autre est une décision de
   langage (`div`, `%`, `vers_q`…).
2. `mpz_export`/`mpz_import` : aller-retour bits↔limites utilisé par les
   conversions flottant exactes — à remplacer par un passage en base 2
   interne direct (nos limites seront déjà binaires).
3. `mpz_probab_prime_p` : Miller-Rabin de GMP avec ses tours fixes ;
   `est_premier` dans les programmes dorés et génératifs dépend du
   verdict exact sur des nombres géants — nos bases et tours doivent être
   arrêtés et verrouillés par corpus, pas hérités du hasard.
4. `mpz_set_d` : GMP est « indéfini » hors plage — la référence est le
   comportement de GMP sur nos machines, mesuré, pas le manuel.
5. `mpq_class` : toujours normalisé, signe au numérateur.
6. `fib_ui` (GMP) contre doublement rapide : résultats identiques par
   construction, à prouver sur le corpus.

## 3. Route proposée (jalons, chacun livrable et réversible)

1. **rose_mpz / rose_mpq** — bibliothèque native dans `moteur/bignum/`,
   C++17 sans dépendance : limites base 2³² (accumulateurs u64),
   multiplication école + Karatsuba au-delà d'un seuil mesuré, division
   de Knuth, pgcd de Lehmer, Miller-Rabin à bases déterministes
   (vérifiées jusqu'à 3,3·10²⁴), rationnel = (mpz, mpz) normalisé au gcd.
   API « Rose » (pas un faux gmp.h) — on ne se marie pas à une API
   empruntée.
2. **Validation = corpus existant, sans exception** : 46 dorés, 12 roses,
   119 parités, 380 graines génératives, 10 empreintes de bancs. Une
   seule sortie qui bouge = jalon non passé. Seuil de performance : les
   bancs sont des valeurs < 10⁴⁰ — l'école + Karatsuba doivent rester à
   ≤ 20 % de GMP sur mot_de_passe/premiers (mesures en session thermique
   identique, HEAD comme base).
3. **Bascule moteur par moteur** : C++ d'abord (il porte déjà le runtime
   AOT en C++ pur), puis Rust et Zig via la façade C de rose_bignum.
   Pendant la transition, `ROSE_GMP=1` garde l'ancien chemin compilable —
   deux chemins, un seul corpus de vérité.
4. **Le gain visible du AOT** : `rose build` émettrait le bignum Rose
   inline dans le `.cpp` transpiré — le binaire compilé de l'utilisateur
   n'aurait plus besoin de GMP du tout, seulement d'un compilateur.
5. **Ensuite, les autres dépendances, une par une, avec le même rituel**
   (inventaire → surface → bibliothèque maison → corpus) :
   `libm` (le `pow` flottant et `floor` sont IEEE — portables), `libdl`
   (chargeur de DLL à la main : ELF + PE), `libpthread` (option pile
   grande par mmap + thread unique), `libstdc++` (dernier, et peut-être
   jamais : c'est la libc++ tout entière, pas une bibliothèque).

## 4. Risques nommés d'avance

- **Performance des grands entiers** : GMP a de l'assembleur et des
  algorithmes de Karatsuba/Toom/FFT réglés sur trente ans. Nos bancs
  restent petits (mesuré) — l'écart est acceptable et chiffré au jalon 2,
  pas espéré.
- **Verdicts `est_premier`** : tout écart de Miller-Rabin avec GMP sur le
  corpus est un échec de jalon ; les bases déterministes seront verrouillées
  par test, pas choisies à la mode.
- **Trois moteurs, une sémantique** : le bignum partagé passera par les
  119 parités — Rust et Zig n'auront pas le droit à leur propre interprétation.

*Première photographie : 21/10/2026. Tenir à jour à chaque jalon.*
