# Thorn — recettes : écrire une preuve, déboguer, étendre la bibliothèque

**Constat daté : 20/11/2026.** Document pratique. Toutes les commandes ont été
réellement exécutées depuis `/home/clausake/Stefan/ROSE` avec `./rose/rose`
(0.1.6). Le bug `--trace` sur les termes `lam` est reproduit, expliqué et
contourné en §4.3. Les compléments théoriques sont dans `PRO_GUIDE.md` et
`PRO_LIMITES.md`.

---

## 1. Anatomie d'une preuve `.thn`

### 1.1 Le format accepté par le pilote

```
# commentaire
théorème <nom> : <type du but>
preuve <tactique ou terme>
fin
```

Exemple réel (`thorn/exemples/ex10_ok.thn`) :

```
# Exemple 10a — fichier .thn accepté par le pilote.
théorème ex10_refl_zero : 0 = 0
preuve reflexivite
fin
```

Alias acceptés : `theoreme`, `preuv`, `proof`, `refl`, `sym`, `intro`, `app`,
`ind`, `appliquer`, `simplification`.

### 1.2 Le pipeline exact (chaque étape est réelle)

```
lecture (lire_texte)
  → lexage (lexer) + normalisation des jetons
  → découpage en déclarations (p_declarations)
  → analyse du but (analyser) + normalisation syntaxique (ta_normalise)
  → verifier(but, ctx)                      # le but doit être un type bien formé
  → séquenceur de tactiques (p_exec_cmds)   # construit un terme, propage buts/ctx
  → verifier(terme, ctx)                    # synthèse de type du terme de preuve
  → convertible(type_synthétisé, but)       # égalité au but annoncé
  → OK <nom>  ou  ÉCHEC <nom> : <raison>
```

**Règle d'or** : une preuve n'est acceptée que si les **deux** dernières
étapes passent. Il n'y a aucun raccourci, aucun `ok` codé en dur. Le verdict
est calculé par le noyau.

---

## 2. Recette pas à pas

### Étape 1 — Écrire un but que Thorn peut analyser

Utiliser le vocabulaire supporté : `pour_tout`, `implique`, `et`, `ou`, `non`,
`vrai`, `faux`, `=`, `+`, `-`, `type`, `fn`, `existe`.

**Éviter** :
- le littéral `1` : il est analysé comme `const("1", type(0))`, pas comme
  `succ zero`. Mesuré :
  ```
  $ ./rose/rose thorn/src/principal.rose /tmp/opencode/thorn_probe/faux2.thn
  ÉCHEC ex10_faux2 : type invalide — variable non liee: succ
  ```
  et pour `0 = 1` : `ÉCHEC … : type invalide — membres d'une egalite de types differents`.
  Écrire les preuves avec des **variables** et des lemmes, pas des numeralia.
- `succ` écrit directement dans un `.thn` : c'est une variable libre.

### Étape 2 — Vérifier que le but est bien typé avant de chercher la preuve

Sonde Rose (fichier temporaire, exécuté depuis la racine) :

```rose
importer "thorn/src/tactiques.rose"
$rec_but = noy_pi("A", noy_type(0), noy_pi("x", noy_var("A"),
           ta_eq(noy_var("A"), noy_var("x"), noy_var("x"))))
$rec_r = verifier($rec_but, ta_ctx())
>> "but ok = " + chaine(noy_est_ok($rec_r))
>> "code   = " + noy_code($rec_r)
```

Si le but lui-même est refusé (`code` ≠ `OK`), aucune tactique ne pourra
réussir : corriger l'énoncé d'abord.

### Étape 3 — Choisir la tactique selon la forme du but

| Forme du but | Tactique | Terme fabriqué |
|---|---|---|
| `Πx:A, B` | `introduction x` (puis résoudre `B`) | `λx:A. ?trou` |
| `a = a` (membres convertibles) | `reflexivite` | `refl A a` (+ `λ` sous les Π) |
| `b = a` avec `h : a = b` | `symetrie` | `symm A a b h` |
| `Πm:Nat, P m` | `induction m` + `cas zero` / `cas succ k ih` | `nat_rec P base pas` |
| but égal à une hypothèse ou à un lemme | `application` / `appliquer` | application du terme trouvé |
| expression avec `0 + n` ou `succ m + n` | `simplifier` (aide locale, **non** validée par le noyau) | terme réduit |

### Étape 4 — Écrire et jouer la preuve

```
./rose/rose thorn/src/principal.rose <fichier.thn>
echo $?     # 0 = tout passe, 1 = échec de preuve, 2 = lecture/parsing
```

Sorties réelles des cinq preuves de démonstration (jouées une à une) :

```
$ ./rose/rose thorn/src/principal.rose thorn/preuves/identite.thn
OK identite
1 théorème(s), 1 réussi(s)
RC=0

$ ./rose/rose thorn/src/principal.rose thorn/preuves/reflexivite.thn
OK reflexivite
1 théorème(s), 1 réussi(s)
RC=0

$ ./rose/rose thorn/src/principal.rose thorn/preuves/symetrie.thn
OK symetrie
1 théorème(s), 1 réussi(s)
RC=0

$ ./rose/rose thorn/src/principal.rose thorn/preuves/associativite.thn
OK associativite_plus
1 théorème(s), 1 réussi(s)
RC=0

$ ./rose/rose thorn/src/principal.rose thorn/preuves/commutativite.thn
OK commutativite_plus
1 théorème(s), 1 réussi(s)
RC=0
```

Le raccourci collectif donne :

```
$ ./rose/rose thorn/src/principal.rose --demo
=== thorn/preuves/identite.thn ===
OK identite
1 théorème(s), 1 réussi(s)
...
5 théorème(s), 5 réussi(s)
RC=0
```

### Étape 5 — Contrôler le résultat

1. `OK <nom>` et `RC=0`.
2. Le compteur `1 théorème(s), 1 réussi(s)`.
3. Si `--demo`, qu'aucune ligne `ÉCHEC` n'apparaît.
4. En cas de doute, reconstruire le terme en Rose (§4.4) et lire
   `noy_code($r)` : il doit valoir `OK`.

---

## 3. Trois stratégies de preuve commentées

### 3.1 Identité : introduction + application

Fichier : `thorn/preuves/identite.thn`.

```
théorème identite :
  pour_tout A : type, pour_tout x : A, A
preuve
  introduction A
  introduction x
  application x
fin
```

`introduction` dépouille les deux Π en résultant le contexte `A : Type(0)`,
`x : A` ; `application x` trouve exactement `x : A`. Le terme final est
`λA:Type(0). λx:A. x`. Sortie réelle : `OK identite`, RC=0 (ci-dessus).

### 3.2 Symétrie : une hypothèse de type exact

Fichier : `thorn/preuves/symetrie.thn`.

```
théorème symetrie : pour_tout A : type, pour_tout a : A, pour_tout b : A, a = b -> b = a
preuve symetrie
```

La tactique traverse les Π, introduit `h : a = b`, puis fabrique
`symm A a b h`. `symm` n'est **pas** un axiome : c'est un terme construit avec
`eq_rec` et vérifié par le noyau (`ta_terme_symm()`, contrôlé par
`demo_lemmes()`). Sortie réelle : `OK symetrie`, RC=0.

### 3.3 Induction : associativité de `+`

Fichier : `thorn/preuves/associativite.thn`. Structure (commentaires du
fichier) :

```
théorème associativite_plus :
  pour_tout m : Nat, pour_tout n : Nat, pour_tout p : Nat,
    (m + n) + p = m + (n + p)
preuve
  induction m
  cas zero :
    introduction n
    introduction p
    appliquer trans (...)
  cas succ k ih :
    introduction n
    introduction p
    appliquer trans (...)
fin
```

Le squelette `induction m` ouvre deux buts : la base `P zero` et le pas
`Πn ih, P (succ n)`. Les deux branches sont fournies explicitement ; le
terme fermé `nat_rec P base pas` repasse par `verifier` **et**
`convertible`. Sortie réelle : `OK associativite_plus`, RC=0.

**Attention** : `trans h1 h2` est élaboré localement (`ta_elab`) en
`trans A a b c h1 h2` par décomposition des types synthétisés. Ce n'est pas
une unification générale : si l'élaboration échoue, écrire les arguments
explicitement.

---

## 4. Déboguer

### 4.1 Codes de sortie

| Code | Signification |
|---|---|
| `0` | tous les théorèmes passent |
| `1` | au moins un échec de preuve |
| `2` | lecture / lexage / analyse impossible, ou aucun théorème |

### 4.2 Messages les plus fréquents

| Message | Cause probable | Remède |
|---|---|---|
| `variable non liee: Nat` | `Nat` analysé comme `var` libre | le pilote applique `ta_normalise` ; si vous écrivez votre propre script, utilisez `ta_nat()` |
| `variable non liee: succ` | `succ` écrit dans un `.thn` | utiliser `ta_succ` côté Rose ; éviter les numeralia dans le `.thn` |
| `tactique inattendue '?'` | caractère inconnu dans le fichier | corriger le jeton ; le numéro de ligne est affiché |
| `type invalide — membres d'une egalite de types differents` | `0 = 1` (types `Nat` vs `Type(0)`) | écrire un but bien typé, ex. `∀A B:Type, A = B` |
| `réflexivité : les deux membres diffèrent` | les membres ne sont pas βια-convertibles | la preuve n'existe pas par `refl` ; changer de tactique ou d'énoncé |
| `symétrie : aucune hypothèse de type a = b` | pas d'hypothèse de type exact | introduire `h : a = b` puis `symetrie` |
| `type de la preuve ≠ but` | le terme a un type valide mais autre que le but | vérifier l'énoncé et les arguments de la tactique |
| `preuve incomplete` | un trou reste ouvert | fermer toutes les branches |
| `terme de preuve invalide — …` | `verifier` a refusé le terme | lire le code (`noy_code`) sur une sonde Rose |
| `Rose : erreur — clé absente de la table` | `--trace` sur un terme `lam` | voir §4.3 |

### 4.3 BUG CONNU — `--trace` plante sur les termes `lam`

**Reproduction réelle, le 20/11/2026 :**

```
$ ./rose/rose thorn/src/principal.rose thorn/preuves/identite.thn --trace
Rose : erreur — clé absente de la table
RC=1

$ ./rose/rose thorn/src/principal.rose thorn/preuves/reflexivite.thn --trace
Rose : erreur — clé absente de la table
RC=1
```

Idem pour `symetrie`, `associativite`, `commutativite`. **Sans** `--trace`,
les mêmes fichiers rendent `OK …` et RC=0.

**Ce qui fonctionne** (terme tracé sans `lam`) :

```
$ ./rose/rose thorn/src/principal.rose /tmp/opencode/thorn_probe/t1.thn --trace
  [trace] t1 but = ind(=)
  [trace] t1 terme = app(app(const(refl) : pi(A)(type[0] -> pi(x)(… -> …)) ind(Nat)) const(zero) : ind(Nat))
OK t1
1 théorème(s), 1 réussi(s)
RC=0
```

(fichier `t1.thn` = `théorème t1 : 0 = 0 / preuve reflexivite / fin`.)

**Cause exacte.** Dans `thorn/src/principal.rose`, l'afficheur de trace :

```rose
fn p_montrer(t, prof: entier) -> chaine {
  ...
  si (cle_existe(t, "domaine")) {
    $mt_s = $mt_s + "(" + p_montrer(t["domaine"], $mt_p) + " -> " +
           p_montrer(t["codomaine"], $mt_p) + ")"
  }
  si (cle_existe(t, "corps")) {
    $mt_s = $mt_s + "(corps " + p_montrer(t["corps"], $mt_p) + ")"
  }
  ...
}
```

Il teste la présence de `domaine` mais lit **aussi** `codomaine`. Or dans
`thorn/src/noyau.rose` :

```rose
fn noy_pi(n, d, c) -> table {
  ... inserer($nx_t, "domaine", d); inserer($nx_t, "codomaine", c); ...
}
fn noy_lam(n, d, c) -> table {
  ... inserer($nx_t, "domaine", d); inserer($nx_t, "corps", c); ...
}
```

Un `lam` a donc `domaine` **sans** `codomaine` : la lecture
`t["codomaine"]` lève « clé absente de la table ». Cette exception n'est pas
interceptée dans la boucle de `pilote_executer`, elle tue le processus (RC=1).
Le correctif serait de tester `codomaine` séparément (ou de choisir la branche selon
`genre`) — **hors périmètre de ce document** : aucun fichier existant n'a été
modifié ici.

**Pourquoi la CI ne le voit pas.** `thorn/tests_pilote.rose` écrit son propre
fichier de test :

```
théorème t_ok : type 1
preuve type 0
fin
```

Aucun `lam` n'y apparaît : les deux assertions `trace : étiquette` et
`trace : terme` passent (12/12 au total) alors que le bug existe sur les
preuves réelles. `QUALITE.md` le documente explicitement.

**Contournement recommandé :**
1. Ne pas utiliser `--trace` quand le but ou la preuve contient un `lam` ;
   l'utiliser sur des buts simples (`0 = 0`, `∀A B:Type, A = B`).
2. Déboguer au niveau Rose (§4.4) : les champs `terme` et `regle` sont
   disponibles sans passer par l'afficheur du pilote.
3. Écrire un afficheur local qui teste `codomaine` **et** `corps` — celui de
   `ex01_identite.rose` (fonction `m`) est un modèle vérifié.

### 4.4 Sonde Rose générique

```rose
importer "thorn/src/tactiques.rose"

$rec_but = noy_pi("A", noy_type(0), noy_pi("x", noy_var("A"),
           ta_eq(noy_var("A"), noy_var("x"), noy_var("x"))))
$rec_r = reflexivite($rec_but)
>> "ok    = " + chaine(ta_boo($rec_r, "ok"))
>> "regle = " + ta_txt($rec_r, "regle")
>> "terme = " + chaine(noy_genre(ta_term($rec_r)))
```

Sortie réelle : `ok = vrai`, `regle = reflexivite/lambda`, `terme = lam`.
On voit le `lam` **sans** déclencher le bug du pilote.

---

## 5. Pièges Rose à connaître

1. **Préfixer les globales** (`$rec_...`, `$nx_...`). Rose infère le type des
   variables par nom, tous scopes confondus : `$r`, `$but`, `$terme` entrent en
   collision avec les locaux importés (`Ordre entier > complexe impossible`).
2. **`tab["cle"]` est typé `?`** en Rose : les modules définissent des
   convertisseurs (`bl_tab`, `al_tab`, `tpl_tab`, `p_comme_table`) pour
   expliciter le retour `table`.
3. **Ordre d'import** : `analyse.rose` importe `lexeur.rose` ;
   `principal.rose` importe `tactiques`, `analyse`, `noyau`, `lexeur` dans cet
   ordre. Ne pas le changer à l'aveugle.
4. **Listes rembourrées** : contexte, `params`, `branches` et `buts` ont un
   sentinelle à l'index 0. Compter = `longueur(l) - 1`.
5. **`verif` avant exécution** : `./rose/rose verif <fichier.rose>` rend
   `vérifié`. Le `rose` du PATH (0.1.5) ne connaît pas `verif`.
6. **Ne pas modifier les fichiers existants** pour une extension : créer un
   nouveau fichier consommateur (voir §6).

---

## 6. Étendre la bibliothèque

### 6.1 Principe

Un nouveau fichier de lemmes est un **consommateur** : il importe
`tactiques.rose` (qui tire `noyau.rose`) et construit des termes soumis à
`verifier` + `convertible`. Il ne réimplémente aucune règle de typage.

### 6.2 Modèle minimal (testé le 20/11/2026)

```rose
importer "thorn/src/tactiques.rose"

fn tpl_tab(x) -> table {
  $v = table()
  $v = x
  retour $v
}

fn tpl_pair(b: table, t: table) -> table {
  $r = table()
  inserer($r, "but", b)
  inserer($r, "terme", t)
  retour $r
}

fn tpl_test(nom: chaine, but: table, terme: table) {
  $r = verifier(terme, ta_ctx())
  $s = noy_est_ok($r)
  $c = convertible(noy_type_res($r), but)
  si ($s et $c) {
    >> "OK " + nom
  } sinon {
    >> "ECHEC " + nom + " : " + noy_erreur($r)
  }
}

# Nouveau lemme : refl_succ : Πn:Nat, succ n = succ n.
fn lem_refl_succ() -> table {
  $but = noy_pi("n", ta_nat(),
          ta_eq(ta_nat(), ta_succ(noy_var("n")), ta_succ(noy_var("n"))))
  $terme = noy_lam("n", ta_nat(),
             ta_ap2(ta_refl_symbole(), ta_nat(), ta_succ(noy_var("n"))))
  retour tpl_pair($but, $terme)
}

$p = lem_refl_succ()
tpl_test("refl_succ", tpl_tab($p["but"]), tpl_tab($p["terme"]))
```

Résultat réel :

```
$ ./rose/rose verif /tmp/opencode/thorn_probe/template.rose
vérifié
$ ./rose/rose /tmp/opencode/thorn_probe/template.rose
OK refl_succ
RC=0
```

### 6.3 Le contrôle honnête (extrait réel de `bibliotheque.rose`)

```rose
fn bl_test(bl_nom: chaine, bl_but: table, bl_terme: table, bl_cpt: table) {
  $bl_r = verifier(bl_terme, ta_ctx())
  $bl_synthese = noy_est_ok($bl_r)
  $bl_conv = convertible(noy_type_res($bl_r), bl_but)
  $bl_ok = $bl_synthese et $bl_conv
  si ($bl_ok) {
    >> "OK " + bl_nom
  } sinon {
    $bl_pourquoi = "type synthétisé ≠ but"
    si (non $bl_synthese) { $bl_pourquoi = noy_erreur($bl_r) }
    >> "ECHEC " + bl_nom + " : " + $bl_pourquoi
  }
  bl_compter(bl_cpt, $bl_ok)
}
```

Et le moteur réel :

```
$ ./rose/rose thorn/src/bibliotheque.rose
Thorn — bibliothèque de lemmes (vérification par le noyau)
OK refl_succ
OK zero_plus_succ
OK succ_plus_zero
OK symm
OK trans
OK congr_succ
OK congr_plus_l
OK plus_zero_d
OK plus_assoc
OK plus_comm
10 lemme(s) vérifié(s), 0 échec(s)
RESULTAT: OK
RC=0
```

### 6.4 Ajouter un axiome : la discipline à respecter

Si le lemme ne peut pas être dérivé (ex. une équation de `mult`, une
propriété de `≤`), le déclarer explicitement comme constante de signature :

1. Construire `noy_const("<nom>", <type>)` avec un type annoté bien formé.
2. Le documenter dans l'en-tête du nouveau fichier **et** dans
   `PRO_LIMITES.md` §2 (statut `supposé`).
3. Ne jamais le présenter comme « démontré ».
4. Vérifier que le noyau accepte son type : `verifier(noy_const(...), ta_ctx())`
   doit rendre `code = OK`.

C'est exactement ce que fait `algebre.rose` pour `mult_0_r`, `mult_s_r`,
`le_refl_ax`, `succ_inj_ax`, etc. (`ALGEBRE.md` §3).

### 6.5 Checklist avant livraison d'un ajout

```
./rose/rose verif <nouveau_fichier.rose>
./rose/rose <nouveau_fichier.rose>                       # 0 échec
./rose/rose thorn/tests_noyau.rose                       # REUSSIS=126, ECHOUES=0
./rose/rose thorn/tests_syntaxe.rose                     # OK = 8   ECHEC = 0
./rose/rose thorn/tests_tactiques.rose                   # TOTAL_REUSSIS=33, TOTAL_ECHOUES=0
./rose/rose thorn/tests_pilote.rose                      # REUSSIS=12, ECHOUES=0
./rose/rose thorn/tests_algebre.rose                     # 34 vérifiés, 0 échec
./rose/rose thorn/src/bibliotheque.rose                  # 10 vérifiés, 0 échec
./rose/rose thorn/src/principal.rose --demo              # 5/5
bash thorn/tests_regression.sh                           # BILAN: OK
```

Si un nouveau `.thn` doit être couvert par `--demo`, l'ajouter à
`pilote_manifeste()` **et** à la porte de non-régression (sinon la vérification
d'honnêteté du harnais rend la CI rouge — voir `QUALITE.md` §4.2).

---

## 7. Résumé en une page

1. Écrire le but avec les mots supportés ; éviter `1` et `succ` littéraux.
2. Vérifier le but avec `verifier` avant de chercher la preuve.
3. Choisir la tactique selon la forme (Π → introduction, `a=a` →
   reflexivite, `h:a=b` → symetrie, Nat → induction).
4. Jouer avec `./rose/rose thorn/src/principal.rose <fichier>` ; exiger
   `OK`, `1 réussi(s)`, RC=0.
5. `--trace` : ne l'utiliser **que** sur des termes sans `lam` ; sinon
   `clé absente de la table` (bug documenté et non corrigé ici).
6. Étendre : nouveau fichier consommateur, `verifier` + `convertible`, jamais
   de `ok = vrai` codé en dur ; documenter tout axiome ajouté.
7. Rejouer les huit commandes de la checklist avant de livrer.
