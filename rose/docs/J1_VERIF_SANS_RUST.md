# J1 — vérification auto-hébergée dans le composite (`--verif`), Sans Rust

Worktree exclusif `/tmp/opencode/opt_aot`, HEAD `62ab54c91f1778f5c201898abf50607afbabcb8d`.
Aucun commit, aucune publication. Fichiers possédés uniquement :
`scripts/generer_rose_natif.py`, `compilateur/rose_natif.rose` et
`compilateur/rose_natif_pe.rose` (régénérés), `scripts/build-sans-rust.sh`,
ce document. `compilateur/emetteur.rose`, `emetteur_elf.rose`,
`emetteur_pe.rose`, `verifieur.rose` et `rose/src/**` sont **intacts**.

## 1. Ce qui a été fait

`compilateur/verifieur.rose` (79 `fn`, 2 805 lignes) existait mais n'était pas
câblé dans le composite auto-hébergé (`SELFHOST_EXTENSION.md` §2/§7). Il est
désormais injecté **dans la copie générée uniquement** par
`scripts/generer_rose_natif.py`, exactement comme les injections HOST-2
(`eval_expr`, `octet_brut`) :

1. **Renommage** : ses 79 `fn` reçoivent le préfixe `vf_`. Mesure : **74** de
   ses noms (`car`, `std_sig`, `check_expr`, `piece`, `reserver`, `env_put`, …)
   collisionnaient avec une `fn` du front-end `emetteur.rose`. Sans préfixe,
   `rose verif` du composite échoue par doubles définitions.
2. **Enveloppe** : son `$principal` (≈ 100 variables) est enfermé dans
   `fn verifier_source($src_brut: chaine) -> chaine` ; l'import
   `lire_texte(argument(0))` est remplacé par `vf_fusionner_imports($src_brut)`
   et les deux `>>` finaux deviennent `retour` (message d'erreur ou
   `"vérifié"`). Ces variables deviennent des **slots locaux** : aucune
   globale G5 supplémentaire.
3. **Dispatch** : en tête du chemin G5, avant la lecture de `argument(0)`
   comme source à compiler :
   ```
   si (nb_arguments() >= 2 et argument(0) == "--verif") {
     >> verifier_source(lire_texte(argument(1)))
     quitter(0)
   }
   ```
   Le reste du chemin (`rose_natif.rose source.rose > source.elf`) est
   inchangé.
4. **`RNAT_VERIF=0`** désactive tout ce bloc (A/B avant/après).

`scripts/build-sans-rust.sh` gagne `--verif-source` : `C0 --verif SOURCE` pour
chaque source, rapport TSV `CONFORME` / `REJETE` / `BLOQUE`, sans appel à
`rose/rose` ni à `cargo`.

## 2. Contrainte des 512 globales — MESURÉE

Mesure par `rose rir` du composite (la table des globales est le `noms [...]`
de la dernière `fonction` du dump, émise à `0x60004000 + 8*i`) :

| Composite | globales (slots) | indice max | borne |
|---|---|---|---|
| `rose_natif.rose` **avant** J1 (`RNAT_VERIF=0`) | **405** | 404 | 512 |
| `rose_natif.rose` **après** J1 | **405** | 404 | 512 |
| `rose_natif_pe.rose` après J1 | **52** | 51 | 512 |

**Zéro globale ajoutée** : l'enveloppe dans une `fn` suffit ; aucun
dépassement, aucun contournement. (Le chiffre 405 inclut la garde
`argument(1)` ajoutée par l'extension précédente ; le document
`SELFHOST_TOTAL.md` cite 402 pour la version antérieure à cette garde.)

Autres contrôles d'identité :

```
./rose/rose verif compilateur/rose_natif.rose     -> vérifié (rc=0)
./rose/rose verif compilateur/rose_natif_pe.rose  -> vérifié (rc=0)
./rose/rose rir compilateur/emetteur.rose | md5sum
  -> b3ed9262dd54776a6dbd07bc12368d0e  (inchangé)
grep -c '^fn ' compilateur/rose_natif.rose -> 364  (284 avant J1 + 79 vf_* + 1 verifier_source)
grep -c '^fn vf_' compilateur/rose_natif.rose -> 79
grep -c '^fn verifier_source' compilateur/rose_natif.rose -> 1
```

Taille générée : `rose_natif.rose` **548 713 o** (md5
`2f130a5c3534c3c54345378ee559d745`), `rose_natif_pe.rose` **319 517 o**
(md5 `aa4695dabfa7342725b3ecbccdc5dbfb`).

## 3. Le binaire C0 natif Sans Rust

Construit **une seule fois depuis C0_EXT existant** (graine Rust déjà
nécessaire pour produire le tout premier ELF, comme dans `SELFHOST_EXTENSION.md` §10) :

```
bash scripts/garde_fou_mem.sh 12000000 timeout 1200 \
  /tmp/opencode/C0_ext.elf compilateur/rose_natif.rose > /tmp/opencode/j1_C0_from_C0ext.elf
chmod +x /tmp/opencode/j1_C0_from_C0ext.elf
# -> ELF aarch64, 2 078 944 o, magic 7f454c4602010100, 13 min 17 s, rc=0
```

`/tmp/opencode/j1_C0_from_C0ext.elf --verif tests_selfhost/t01_entiers.rose`
imprime `vérifié` (rc=0). Le compilateur C0 n'appelle aucun Rust : ni `rose/rose`, ni `cargo`.

## 4. Corpus — taux réels (219 sources)

Script : `/tmp/opencode/j1_compare.py`. Rapports :
`/tmp/opencode/j1_comparaison.tsv` et `/tmp/opencode/j1_comparaison.json`.
Canonicalisation : le binaire Rust écrit le message sur **stderr** (précédé
d'une ligne vide), le composite et le vérificateur Rose sur **stdout** ; on
compare les lignes non vides dans l'ordre.

Corpus = **141** `tests_selfhost/*.rose` + **25** `rose/tests_aot8/programmes/*.rose`
(soit les **166** sources du jalon) + **53** `rose-cpp/tests/verifs/*.rose`
(corpus de rejets, messages d'erreur exacts) = **219** sources.

| Protocole | Résultat |
|---|---|
| **Fidélité du câblage** : `C0 --verif` vs `rose compilateur/verifieur.rose` | **219/219 (100 %)** |
| **Conformité oracle** : `C0 --verif` vs `rose verif` (moteur Rust) | **213/219 (97,3 %)** |
| Sur les 166 sources du jalon | **160/166 (96,4 %)** |
| Sur les 53 sources de rejets `verifs/` | **53/53 (100 %)** |

La fidélité 219/219 prouve que le `--verif` diffusé sans Rust est **exactement**
le vérificateur Rose existant : le câblage n'altère aucun verdict. Les 6 écarts
à l'oracle Rust sont **des écarts préexistants de `verifieur.rose` lui-même**,
reproduits à l'identique par `rose compilateur/verifieur.rose` (ils ne
viennent ni de l'enveloppe, ni du dispatch) :

| Source | `C0 --verif` / vérificateur Rose | `rose verif` (Rust) | Cause |
|---|---|---|---|
| `tests_selfhost/t14_type.rose` | `Fonction 'type' inconnue` | `vérifié` | `verifieur.rose::std_sig` n'expose pas l'alias `type` (injecté dans le composite pour la compilation, pas dans le vérificateur) |
| `tests_selfhost/t91_prng.rose` | `Fonction 'prng' inconnue` | `vérifié` | idem pour `prng` |
| `rose/tests_aot8/programmes/24_compose.rose` | `Addition valeur + chaine impossible` | `vérifié` | `check_expr` du vérificateur n'a pas le correctif `valeur` = Dyn du moteur Rust (le composite, lui, l'a) |
| `tests_selfhost/g5b513_rat_calc_boucle.rose` | `'s' est entier, on ne peut pas y stocker du rationnel` | `vérifié` | propagation de type du vérificateur sur une boucle qui change un entier en rationnel |
| `tests_selfhost/t104_surcharge_arith.rose` | `Argument de f: chaine reçu, entier attendu` (l.4) | `Fonction 'f' définie deux fois` (l.2) | le moteur Rust rejette la surcharge à la déclaration, le vérificateur à l'appel |
| `tests_selfhost/t105_surcharge_arite.rose` | `f attend 1 argument(s), reçu 2` (l.4) | `Fonction 'f' définie deux fois` (l.2) | idem |

Ces 6 causes sont dans `compilateur/verifieur.rose`, fichier **interdit** ici :
le jalon J1 porte sur le câblage, pas sur la correction du vérificateur.

## 5. Validation globale

```
RUST_TEST_THREADS=1 bash scripts/garde_fou_mem.sh 12000000 \
  bash scripts/validation_finale.sh
# -> RESULTAT: 0 échec(s)
#    tests rc=0 (314 + 327 passés) ; point fixe b3ed9262 ×2
#    AOT 75 OK, 0 ECHEC ; JIT 25/25 ; WASM 92 o ; RVM 317811
#    AZ 31 CONFORME, 0 non-conforme ; HOST-1 fib=317811
```

Vérification du mode `--verif-source` du script (Sans Rust, aucun appel au
binaire Rust) :

```
bash scripts/build-sans-rust.sh --compilateur /tmp/opencode/j1_C0_from_C0ext.elf \
  --sortie /tmp/opencode/j1_verif_out --rapport /tmp/opencode/j1_verif.tsv \
  --verif-source tests_selfhost/t01_entiers.rose tests_selfhost/t14_type.rose \
                 tests_selfhost/g5b513_rat_calc_boucle.rose
# CONFORME t01_entiers ; REJETE t14_type ; REJETE g5b513_rat_calc_boucle
```

## 6. Commandes exactes pour rejouer

```bash
cd /tmp/opencode/opt_aot
RNAT_VERIF=0 RNAT_OUT_ELF=/tmp/opencode/j1_base.rose python3 scripts/generer_rose_natif.py elf
# -> 284 fn, 405 globales (A/B avant J1)
python3 scripts/generer_rose_natif.py
./rose/rose verif compilateur/rose_natif.rose
./rose/rose verif compilateur/rose_natif_pe.rose
./rose/rose rir compilateur/emetteur.rose | md5sum      # b3ed9262…
./rose/rose rir compilateur/rose_natif.rose | grep '^  noms \[' | tail -1  # 405 slots, max 404

bash scripts/garde_fou_mem.sh 12000000 timeout 1200 \
  /tmp/opencode/C0_ext.elf compilateur/rose_natif.rose > /tmp/opencode/j1_C0_from_C0ext.elf
chmod +x /tmp/opencode/j1_C0_from_C0ext.elf
/tmp/opencode/j1_C0_from_C0ext.elf --verif tests_selfhost/t01_entiers.rose   # vérifié

bash scripts/garde_fou_mem.sh 12000000 python3 /tmp/opencode/j1_compare.py
RUST_TEST_THREADS=1 bash scripts/garde_fou_mem.sh 12000000 \
  bash scripts/validation_finale.sh
```

## 7. État final factuel

- **Fait** : `RNAT_VERIF=1` (défaut) injecte 79 `fn vf_*` + `verifier_source()` +
  le dispatch `--verif` dans les deux composites régénérés ; `RNAT_VERIF=0`
  désactive. `--verif-source` exposé dans `build-sans-rust.sh`.
- **Vérifié** : 405 globales < 512 (aucune ajoutée) ; `rose verif` des deux
  composites = `vérifié` ; point fixe `b3ed9262` inchangé ; fidélité
  **219/219** au vérificateur Rose ; conformité **213/219** à l'oracle Rust
  (6 causes précises §4) ; `validation_finale.sh` **0 échec**, AZ 31/31,
  AOT 75/75, JIT 25/25.
- **Diffusion `--verif` sans Rust** : **fonctionnelle** dans le C0 natif
  (`/tmp/opencode/j1_C0_from_C0ext.elf`), avec exactement les mêmes verdicts
  que `rose compilateur/verifieur.rose` sur 219/219 sources. Reste 6 écarts
  préexistants à l'oracle Rust, tous dans le fichier interdit `verifieur.rose`.
