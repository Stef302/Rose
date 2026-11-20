# Thorn — exemple concret : accepter une preuve, en refuser une autre

Document écrit le 21/11/2026. Toutes les sorties ci-dessous ont été **réellement
exécutées** depuis la racine du dépôt (`/home/clausake/Stefan/ROSE`) avec le
binaire `./rose/rose` du dépôt. Les fichiers `.thn` de test sont dans
`/tmp/opencode/th3_agents/` (hors du dépôt). Aucun autre fichier du dépôt n'est
modifié par ce document.

## La phrase qui résume la différence

**Rose répond « combien vaut ? », Thorn répond « cette preuve est-elle valide ? ».**

Rose calcule et produit une valeur. Thorn ne calcule pas : il reçoit une
affirmation **et** une preuve, construit un terme de preuve, et vérifie que ce
terme a bien le type annoncé. Sa sortie n'est pas un résultat de calcul, c'est un
verdict : `OK` ou `ÉCHEC`, avec la raison.

---

## 1. Exemple POSITIF — Thorn accepte une bonne preuve

Fichier `/tmp/opencode/th3_agents/pos_refl.thn` :

```
théorème refl_zero : 0 = 0
preuve reflexivite
fin
```

Commande exacte (depuis la racine du dépôt) :

```
./rose/rose thorn/src/principal.rose /tmp/opencode/th3_agents/pos_refl.thn --trace
```

Sortie réelle (rc=0) :

```
  [trace] refl_zero but = ind(=)
  [trace] refl_zero terme = app(app(const(refl) : pi(A)(type[0] -> pi(x)(… -> …)) ind(Nat)) const(zero) : ind(Nat))
OK refl_zero
1 théorème(s), 1 réussi(s)
```

**Le terme de preuve construit** est, sur la ligne `[trace] … terme =` :

```
app(app(const(refl) : pi(A)(type[0] -> pi(x)(… -> …)) ind(Nat)) const(zero) : ind(Nat))
```

Lisiblement, c'est `refl Nat 0` : la constante de réflexivité `refl`, appliquée
au type `Nat`, puis au terme `0`. C'est bien un terme de type `0 = 0`. Sans
`--trace`, la même commande affiche seulement `OK refl_zero` et le compteur.

---

## 2. Exemple NÉGATIF — Thorn refuse une mauvaise preuve

Même forme, même tactique, une seule chose change : l'affirmation est fausse
(`0 = 1` au lieu de `0 = 0`).

Fichier `/tmp/opencode/th3_agents/neg_0_1.thn` :

```
théorème faux_zero_un : 0 = 1
preuve reflexivite
fin
```

Commande exacte :

```
./rose/rose thorn/src/principal.rose /tmp/opencode/th3_agents/neg_0_1.thn
```

Sortie réelle (rc=1) :

```
ÉCHEC faux_zero_un : réflexivité : les deux membres diffèrent
1 théorème(s), 0 réussi(s)
```

Avec `--trace`, on voit qu'**aucun terme n'a été construit** (il n'y a pas de
ligne `terme =`) — sortie réelle, rc=1 :

```
  [trace] faux_zero_un but = ind(=)
ÉCHEC faux_zero_un : réflexivité : les deux membres diffèrent
1 théorème(s), 0 réussi(s)
```

**C'est ici que Thorn apporte sa valeur :** il ne se contente pas d'afficher la
tactique choisie, il refuse l'affirmation et dit pourquoi — `0` et `1` sont deux
membres différents, donc la réflexivité ne peut pas produire de preuve de
`0 = 1`. Rose, lui, calcule `0 == 1` et rend `faux` (vérifié : `./rose/rose` sur
`>> 0 == 1` → `faux`, rc=0) : il répond à une question de valeur, pas de
validité.

---

## 3. Ce qui s'est passé, en 5 lignes

1. **Lecture et analyse** : le fichier `.thn` est découpé en jetons, le but est
   analysé puis normalisé : `0 = 0` devient `ind("=", [0, 0])` et `0 = 1`
   devient `ind("=", [0, 1])` — la trace affiche `but = ind(=)`.
2. **But vérifié** : `verifier(but, ctx)` confirme que le but est un type bien
   formé ; dans les deux cas cette étape passe.
3. **Construction du terme** : le séquenceur lance `reflexivite`. Pour `0 = 0`,
   les deux membres sont identiques : elle fabrique `refl Nat 0`. Pour `0 = 1`,
   les membres diffèrent : elle rend une erreur et **aucun terme n'est produit**.
4. **`verifier(terme, ctx)`** : dans le cas positif seulement, le noyau synthétise
   le type du terme `refl Nat 0` et confirme qu'il est valide.
5. **`convertible(type, but)`** : le type synthétisé (`0 = 0`) est déclaré
   convertible au but → `OK refl_zero`. Dans le cas négatif, le pilote s'arrête
   dès l'étape 3 sur l'erreur de la tactique → `ÉCHEC … : les deux membres diffèrent`
   (les étapes 4 et 5 ne sont même pas atteintes).

---

## 4. Rose vs Thorn

| Critère | Rose | Thorn |
|---|---|---|
| **Rôle** | Langage de programmation en français : il calcule et exécute. | Assistant de démonstration : il vérifie des preuves. |
| **Question** | « Combien vaut ? » / « que fait ce programme ? » | « Cette preuve est-elle valide ? » |
| **Sortie** | Une valeur ou un résultat de calcul (ex. `2`, `vrai`, du code). | `OK <nom>` ou `ÉCHEC <nom> : <raison>`, plus un terme de preuve visible avec `--trace`. |
| **Objets** | Programmes, valeurs, fonctions, entiers/rationnels/exacts. | Propositions (`pour_tout`, `=`, `implique`, `faux`…), termes de preuve, tactiques. |
| **Mécanisme** | Évaluation et compilation (interprète exact, JIT Cranelift, AOT LLVM ORC, WASM). | Synthèse de type `verifier` puis comparaison `convertible` (β + α) dans un noyau de types dépendants. |

---

## 5. Ce que Thorn ne fait pas

Ces limites sont documentées et vérifiables ; elles ne sont pas cachées (voir
`thorn/docs/PRESENTATION.md`, §4, et `thorn/docs/NOYAU.md`).

- **Pas de calcul.** Thorn n'évalue pas `1 + 1`. Il vérifie qu'un terme a bien le
  type annoncé. Le but n'est pas « obtenir une valeur », c'est « certifier un
  terme ».
- **Axiomes explicites.** `nat_rec`, `eq_rec`, `plus_0_l` et `plus_s_l` sont des
  constantes de signature, c'est-à-dire des axiomes assumés, pas des théorèmes
  démontrés par le noyau. Thorn ne prétend pas les avoir prouvés : il les
  documente. Il n'y a pas non plus de contrôle de termination ou de positivité.
- **Univers unique.** Tout vit dans `Type(0)`. Pas de polymorphisme d'univers,
  pas de cumulativité (`Type(0)` n'est pas un sous-type de `Type(1)`), pas d'η.
- **Pas d'ι-réduction.** Le noyau ne réduit que β et α (`beta`, `convertible`).
  Les équations `0 + n = n` et `succ m + n = succ (m + n)` sont donc des axiomes
  de signature, pas des calculs. La tactique `simplifier` les applique
  localement, mais son résultat n'est **pas** garanti convertible au terme initial
  pour le noyau. À noter aussi : l'égalité est sur-permissive — le noyau valide
  structurellement `ind("=", [a, b])` sans vérifier que `a` et `b` ont le même
  type.

---

## Reproductibilité

```
cd /home/clausake/Stefan/ROSE
./rose/rose thorn/src/principal.rose /tmp/opencode/th3_agents/pos_refl.thn --trace   # OK refl_zero, rc=0
./rose/rose thorn/src/principal.rose /tmp/opencode/th3_agents/neg_0_1.thn            # ÉCHEC faux_zero_un, rc=1
```

Les codes de sortie sont : **0** si tout passe, **1** si au moins une preuve
échoue, **2** si lecture/lexage/analyse est impossible (`thorn/docs/PILOTE.md`).
Utiliser `./rose/rose` (0.1.6, présent dans le dépôt) et non le `rose` du PATH
(0.1.5), qui ne connaît pas la commande `verif`.
