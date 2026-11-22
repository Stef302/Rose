# `eval_expr` et Unicode — diagnostic du 22/11/2026

Périmètre : `compilateur/emetteur_elf.rose` uniquement (repli `eval_expr`
écrit en Rose + helpers `ee_*`). Aucun autre fichier modifié ; C0 non réémis,
`rose_natif.rose` non régénéré.

## 1. Repro (binaire C0 déjà émis, `/tmp/opencode/C0_final.elf`)

| Source compilée | Résultat |
|---|---|
| `fn p(a,b) { si (a==63 et b==61) { retour "zz" } … }` | rc=0, ELF 9 436 o |
| idem avec `"≈"` | rc=124 (boucle CPU, aucun syscall), 0 octet |
| idem avec `"中"` | rc=124 |
| idem avec `"é"` | rc=2 |
| `>> "≈"` (sans fonction) | rc=0, ELF 8 904 o **mais contenu faux** : `e0 a0 a9 0a 00 00 0a` au lieu de `e2 89 88 0a` |
| `fn p(a) { retour "≈" }` sans appel | rc=2 |
| `fn p() { si (vrai) { retour "≈" } … }` | rc=124 |

Le même source compilé par le composite hébergé par Rust
(`rose rose_natif.rose …`) passe en ~2 s : le défaut est bien dans le
chemin natif G5.

## 2. Isolation du repli `eval_expr`

`ee_trim`, `ee_trouve`, `ee_tokens`, `ee_var`, `ee_num`, `ee_norm_num`,
`ee_op`, `ee_prec`, `ee_apply`, `ee_puissance`, `ee_atom`, `ee_parse`,
`eval_expr` (lignes 25–391) ont été extraits dans
`/tmp/opencode/ee_extrait.rose` (`eval_expr` renommé `ee_eval`) et appelés
sur les 7 expressions `"zz"`, `"≈"`, `"é"`, `"中"`, `"🚀"`,
`"été radieux"`, `"a≈b"`.

**Sous `rose/target/release/rose`** : `ee_eval` == primitive native
`eval_expr` sur **7/7** cas (y compris les 4 de la mission). Le repli Rose
est donc sémantiquement correct : ce n'est PAS lui qui boucle.

**Sous G5** (mêmes helpers compilés par C0, programme
`/tmp/opencode/g5probe_*.rose`) : sonde avant **et** après correctif :

```
T1=[z]  T2=[)≈ ]  T3=[é ]  T4=[3]
```

Les chaînes d'entrée sont déjà mangles AVANT `eval_expr` : la primitive
`longueur`/`sous_chaine`/`ordre` G5 est correcte (mesuré :
`longueur(lire_texte(e2 89 88))=1`, `ordre(...)=8776`,
`caractere(8776)` → `e2 89 88`), mais un littéral `"` présent dans la
source est substitué par `)` / espace / `+` dès que la source contient
beaucoup de littéraux. `eval_expr` reçoit donc `)≈ ` au lieu de `"≈"` et
rend l'expression non substituée — comportement conforme à son contrat.

**Cause exacte** : le chemin G5 du compilateur (front-end embarqué + table
d'internement des littéraux de `g5_make_str`, `emetteur_elf.rose:1028`)
altère les littéraux non-ASCII et le caractère guillemet avant l'appel à
`eval_expr`. Le blocage (`rc=124`) se produit dans le traitement du corps
de fonction (`extraire_retour`/`resoudre_corps`, lignes 4716/4747), en
amont du repli. Les helpers `ee_*` ne sont pas à l'origine de la boucle.

## 3. Correctif appliqué (robustesse, sans changement de sémantique)

Dans `compilateur/emetteur_elf.rose` :

- `fn ee_car($s, $i)` : accès borné à un POINT DE CODE (`""` hors bornes) ;
- `fn ee_oc($s, $i)` : `ordre(ee_car(...))`, -1 hors bornes (les primitives
  G5 décodent déjà 1–4 octets : aucun octet brut réintroduit) ;
- `ee_trouve` compare point de code par point de code via `ee_car` au lieu
  d'un `sous_chaine($s,$i,$i+$m)` non gardé.

Le correctif ne prétend PAS réparer le mangage des littéraux décrit en §2 :
la sonde G5 est identique avant/après (preuve ci-dessus). Il garantit
au moins qu'aucun helper ne lit d'octet de continuation ni d'index hors
bornes, et prépare le terrain quand le chemin G5 des littéraux sera corrigé.

## 4. Non-régressions mesurées

- `rose verif compilateur/emetteur_elf.rose` → **vérifié**.
- Point fixe : `rose rir emetteur.rose` et
  `rose emetteur.rose emetteur.rose` → tous deux
  `b3ed9262dd54776a6dbd07bc12368d0e`.
- Batterie AZ (`bash tests_selfhost/lancer_az.sh`) : **31/31 CONFORMES**,
  0 divergent, 0 bloqué, avant comme après (comparaisons 31/31 sur les 4
  axes).
- En-tête ELF émis : `7f454c46 0201 0100 0000…` → `e_machine = b7 00`
  (`u16le(183)` intact).
- Sonde Rust `ee_eval` vs `eval_expr` : 7/7 identiques.
