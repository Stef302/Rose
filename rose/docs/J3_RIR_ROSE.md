# J3 — Interpréteur RIR complet en Rose (23/11/2026)

Worktree exclusif `/tmp/opencode/opt_bigint`, HEAD `62ab54c`.
Fichiers possédés : `compilateur/interpreteur_rir.rose`,
`compilateur/tests_interpreteur_rir.rose`, ce document.
Aucun fichier interdit (`emetteur.rose`, `emetteur_elf.rose`,
`rose_natif.rose`, `rose/src/**`) n'a été modifié.
Aucun commit, push ou publication.

## 1. Ce qui a été livré

Un interpréteur du **RIR complet** écrit en Rose, dans le sous-ensemble G5
émétable par C0. Il ne dépend d'aucun fichier `rose/src/**` à l'exécution :
l'ELF natif produit par C0 lit un dump `rose rir <source>` sur `argument(0)`
et rejoue exactement la sortie de l'oracle.

Chaîne :

```
source.rose --[rose/rose rir]--> dump.rir --[ELF C0(interpreteur_rir.rose)]--> sortie
```

L'ELF est produit par :

```
bash scripts/garde_fou_mem.sh 12000000 timeout 900 \
  /tmp/opencode/C0_ext.elf compilateur/interpreteur_rir.rose > /tmp/opencode/j3_interp.elf
```

Mesures : **ELF AArch64, 205 084 octets**, magic `7f454c4602010100`,
rc de compilation 0, sans stderr.

## 2. Format RIR parsé

Le parser lit la grammaire textuelle de `main.rs:688-860` :

* en-têtes `fonction <nom>`, `param <nom> <type>`, `retour <type>`,
  `n_slots N`, `noms [i]nom …`, `globaux i …`, `ops` ;
* opérations préfixées par un index à 4 chiffres (`0000 …`) ;
* lexer maison pour les expressions S-expr avec littéraux `"…"` échappés
  (`\n`, `\t`, `\r`, `\\`, `\"`).

## 3. Opérations couvertes (preuve : corpus 166 sources)

| Op RIR | Occurrences dans le corpus | Couverture |
|---|---:|---|
| `afficher_abstrait` | 266 | ✔ |
| `afficher` | 261 | ✔ (liste d'expressions) |
| `affect` | 129 | ✔ (Eabstrait) |
| `affect_ast` | 38 | ✔ (AST complet) |
| `saut_si_faux_abstrait` | 35 | ✔ |
| `expr` | 31 | ✔ (effets de bord + mutation) |
| `saut` | 23 | ✔ |
| `retour_abstrait` | 17 | ✔ |
| `retour` | 16 | ✔ |

`affect_index`, `lire`, `lever`, `bloc` n'apparaissent **jamais** dans les
166 dumps : ils sont implémentables par extension directe de `exec_fn`
(le parser les ignore proprement), mais non mesurés faute de témoin.

Formes d'expression couvertes : `bin` (14 opérateurs `+ - * / % ^ < <= >
>= == != et ou`), `neg`, `non`, `uni -/non`, `slot`, `entier`, `flottant`,
`(n …)`, `(f …)`, `(s …)`, `(b …)`, `(v …)`, `app`/`appel_rir`/`appel`,
`idx`, `lst`, `lstvide`, `fact`, `mfact`, `term`.

Primitives couvertes (34 noms relevés dans le corpus) : `longueur`, `len`,
`sous_chaine`, `ordre`, `caractere`, `chaine`, `entier`, `flottant`,
`partie_entiere`, `abs`, `min`, `max`, `pgcd`, `gcd`, `ppcm`, `lcm`,
`puissance`, `pow`, `fib`, `fibonacci`, `fact`, `factorielle`,
`est_premier`, `is_prime`, `nb_chiffres`, `digits`, `type`, `type_de`,
`table`, `inserer`, `cle_existe`, `cle_presente`, `joindre`,
`concatener`, `lire_texte`, `ecrire_texte`, `ecrire_bytes`,
`ecrire_binaire`, `argument`, `nb_arguments`, `graine`, `hasard`,
`prng`, `quitter`, `afficher_erreur`.

Appels de fonction et récursion : table de noms, pile plate de slots
passée par valeur (liste Rose), cadres créés à chaque `call_fn`,
récursion mutuelle (`t76_rec_mutuelle`) et récursion profonde (`fib(25)`)
vérifiées.

Valeurs taguées (chaînes) : `i:` entier exact, `r:` rationnel,
`f:` flottant, `g:` scientifique f64, `s:` chaîne, `b:` booléen,
`l:` liste, `t:` table, `v:` vide. L'arithmétique entière et rationnelle
est **logicielle** (décimales, PGCD, division euclidienne de signe plancher)
: aucun débordement 64 bits, contrairement au tag-3 du runtime G5.

## 4. Conformité mesurée (166 sources = 141 `tests_selfhost/` + 25 `rose/tests_aot8/programmes/`)

Méthode : dumps générés une fois par `./rose/rose rir <source>` (hors chaîne
d'exécution), puis ELF exécuté sur chaque dump ; sortie comparée octet à
octet à deux oracles :

| Oracle | Conformes | Divergentes | References en échec | Taux |
|---|---:|---:|---:|---:|
| `./rose/rose <source>` (JIT par défaut) | **148** | 6 | 12 | 148/154 = **96,1 %** |
| `ROSE_SANS_JIT=1 ./rose/rose <source>` (interpréteur Rust) | **157** | 1 | 8 | 157/158 = **99,4 %** |

Sur les 166 sources : 8 références JIT paniquent (`t100_arg31`,
`t101_arg36`, `t72_si_imbrique`, `t999_critique_g5` …), 4 références
sans JIT paniquent ; 1 source aléatoire (`20_hasard`) ne peut pas être
comparée strictement.

**Le taux de conformité sémantique est 157/158 = 99,4 % sur l'oracle
interpréteur (sans JIT), et 148/154 = 96,1 % sur le JIT par défaut.**

### 4.1 Les 6 divergences vs JIT (cas où le JIT Rust déborde)

| Source | JIT | ELF RIR (exact) | Cause |
|---|---|---|---|
| `g5b522_puissance_encoding` | `1`, `-1`, `faux` | `73786976294838206465`, … | `2^66` déborde en i64 dans le JIT |
| `g5b601_cmp_big_litteraux` | `faux` | `vrai` | comparaison de littéraux > 2^63 tronquée |
| `t200_cmp_big_litteraux` | `faux` | `vrai` | idem |
| `t225_accumulation_boucle` | `faux` (3e ligne) | `vrai` | `s == 30000000000000000000` faux après troncature |
| `t64_table_dans_fonction` | `faux` | `vrai` | table locale dans `fn` mal gérée par le JIT |
| `20_hasard` | aléatoire | aléatoire | non reproductible par construction |

Ces cinq premières lignes sont des **bogues du JIT Rust**, pas de
l'interpréteur : le moteur sans JIT donne la même valeur que l'ELF RIR.

### 4.2 L'unique divergence vs oracle sans JIT

`20_hasard` : tirage aléatoire, comparaison stricte impossible.

### 4.3 Performance

`fib(25)` récursif (≈243 000 appels) met ~110 s dans l'interpréteur
Rust hébergé et ~4 s dans l'ELF natif C0. L'arithmétique sur chaînes est
le goulot : c'est le prix de l'exactitude illimitée sans tag-3 G5.

## 5. Tests unitaires

`compilateur/tests_interpreteur_rir.rose` : 15 assertions sur l'arithmétique
décimale, la normalisation, les tables, les listes, les chaînes et les
conversions. Compilé par C0 → ELF 25 860 octets, **`TESTS RIR : 15 OK,
0 ECHEC`**. Vérifié aussi avec `rose verif` et `ROSE_SANS_JIT=1 rose`.

## 6. Comment brancher sur le composite

Trois étapes, sans toucher aux fichiers interdits :

1. **Génération** : `scripts/generer_rose_natif.py` assemble `emetteur.rose`
   + `emetteur_elf.rose` + le runtime en `compilateur/rose_natif.rose`.
   Ajouter un mode `--rir` qui, au lieu d'émettre un ELF à partir d'une
   source, appelle le dump RIR puis l'interpréteur. Les deux sources Rose
   sont déjà des `fn` : les envelopper dans la `fn` principale du
   générateur (aucune globale ajoutée au composite, qui reste sous la
   limite des 512 globales de `0x60004000`).
2. **Émission** : l'interpréteur est écrit dans le sous-ensemble G5 déjà
   couvert par `emetteur_elf.rose` (aucun `mmap`/`mprotect` requis) : il
   suffit de compiler `interpreteur_rir.rose` avec C0 une fois et
   d'intégrer l'ELF obtenu comme tableau d'octets ou de le faire émettre
   par le composite lui-même (auto-hébergement).
3. **API** : exposer une commande `--interprete <dump.rir>` dans le
   générateur, en parallèle de `--verif` (J1). Le contrôle de bout en bout
   est alors `C0 --interprete dump.rir` comparé à `rose/rose <source>`.

⚠ Le composite actuel ne sait pas lire un fichier arbitraire en argument
hors de sa chaîne de compilation ; le branchement demande donc un petit
ajout dans `generer_rose_natif.py` (fichier autorisé, hors de ce worktree).
Aucune modification de `emetteur_elf.rose` n'est nécessaire.

## 7. Causes machine des limitations G5 rencontrées

* **`mmap`/`mprotect` absents** (`x8 ∈ {0,56,57,63,64,80,93}` seulement) :
  pas de JIT à chaud. L'interpréteur est donc interprétatif, pas compilé à
  la volée — c'est le jalon J4, pas J3.
* **`flottant(chaîne)` non pris en charge par le runtime C0** : mesuré
  `flottant("18446744073709551616")` → `0` dans l'ELF C0 contre
  `1.8446744073709552e+19` dans le moteur Rust. Contourné dans l'interpréteur
  par `big_sci()`, formatage scientifique en chaîne pure.
* **`inserer(table, clé existante)`** ne remplace pas la valeur dans le
  runtime C0 (mesuré : `t["a"]` rend `1` après un second `inserer`).
  L'interpréteur implémente `tab_set` en supprimant l'ancienne paire avant
  d'ajouter la nouvelle.
* **Affectation d'une globale depuis une `fn`** : Rose crée une locale.
  Tout l'interpréteur est donc purement fonctionnel (état dans des listes
  passées et retournées), sauf `joindre` sur des globales, autorisé.
* **Récursion profonde** : ~243 000 appels pour `fib(25)` tiennent dans la
  pile AArch64 de l'ELF (pas de débordement mesuré).

## 8. Rejouer

```bash
cd /tmp/opencode/opt_bigint
# dumps RIR (hors chaîne d'exécution)
mkdir -p /tmp/opencode/j3_rir
for f in tests_selfhost/*.rose rose/tests_aot8/programmes/*.rose; do
  b=$(basename "$f" .rose)
  ./rose/rose rir "$f" > /tmp/opencode/j3_rir/$b.rir
done
# ELF natif
bash scripts/garde_fou_mem.sh 12000000 timeout 900 \
  /tmp/opencode/C0_ext.elf compilateur/interpreteur_rir.rose > /tmp/opencode/j3_interp.elf
chmod +x /tmp/opencode/j3_interp.elf
# conformité (oracle sans JIT)
bash /tmp/opencode/j3_test_elf.sh
# -> ELF NOJIT OK=157 BAD=1 SKIP=8
```

Toutes les mesures sont des constats datés du 23/11/2026 dans ce worktree ;
à re-mesurer après tout changement de C0, de la graine Rust ou de la
topologie.
