# Fidélité des oracles M4 — état réel (19/11/2026)

## Constat daté

Corpus = `examples + jardins + bench + rose-cpp/tests` = **162 fichiers `.rose`**.
Comparaison octet pour octet entre le binaire Rust (`rose tokens|ast|verif|rir`)
et les compilateurs écrits en Rose (`compilateur/lexeur|analyseur|verifieur|emetteur.rose`).

| Oracle | Avant corrections | Après | Écarts restants |
|---|---:|---:|---|
| M4.1 lexeur | 162/0 | **162/0** | 0 |
| M4.2 analyseur | 160/2 | **162/0** | 0 |
| M4.3 vérificateur | 157/5 | 157/5 | 5 |
| M4.4 émetteur RIR | 157/5 | 157/5 | 5 |
| Point fixe M4.5 | `bdbef636…` | `bdbef636…` | inchangé |

Les 5 fichiers en écart (identiques pour vérificateur et émetteur) :

| Fichier | Erreur du compilateur Rose |
|---|---|
| `jardins/gui.rose` | `Index sur un ), une liste est attendue` (l.259) |
| `jardins/tests/gui_demo.rose` | `Index sur un ), une liste est attendue` (l.381) — et `'c' est table, on ne peut pas y stocker` en vérification |
| `jardins/math.rose` | `Fonction 'type_de' inconnue` (l.284) |
| `jardins/tests/math.rose` | `Fonction 'type_de' inconnue` (l.495) |
| `jardins/tests/chaines.rose` | `Fonction 'len' inconnue` (l.31) ; `concatener_jardin` : `liste<entier>` reçu, `liste<chaine>` attendu |

## Ce qui a été corrigé

`compilateur/analyseur.rose` appelait `typex()` même quand un paramètre n'avait
pas d'annotation `:`, consommant le séparateur `,` ou `)` suivant et produisant
`param w )` au lieu de `param w ?`. Le front-end Rust accepte ces paramètres
implicites, utilisés massivement dans les jardins. Correctif : émettre
directement `param <nom> ?` sans consommer de jeton. **Oracle M4.2 : 160/2 → 162/0**,
point fixe inchangé (l'émetteur n'a pas été modifié).

## Ce qui reste (et pourquoi ce n'est pas corrigé ici)

Trois causes distinctes, toutes dans les compilateurs **vérificateur/émetteur Rose** :

1. **Paramètres sans annotation** (`?`) : après correction, le vérificateur Rose
   sait les lire mais refuse `Index sur un ?` et `Opération ? * flottant impossible`.
   Le front-end Rust, lui, autorise le typage dynamique dans ces expressions.
2. **Primitives manquantes** : `type_de` et `len` ne sont pas dans la table des
   signatures du vérificateur/émetteur Rose (elles sont dans `front.rs:651,649`).
   Les ajouter fait avancer l'analyse mais révèle les limitations (1) et (3).
3. **Vérification de types plus stricte côté Rose** : `concatener_jardin` reçoit
   `liste<entier>` alors que le Rose vérificateur exige `liste<chaine>` — le
   moteur Rust accepte (le jardin passe `./rose/rose verif`).

Une tentative complète (paramètres + `type_de` + `len`) a été faite sur une copie
isolée : elle supprime le crash `sous_chaine : bornes invalides` (causé par un
type `?` sans `|` passé à `NOD`, qui appelait `sous_chaine(n+1, n)`) mais se heurte
aux limitations de typage dynamique ci-dessus. **Elle n'a pas été appliquée au
dépôt** : elle n'améliorerait pas les compteurs d'oracles et introduirait un
risque de divergence sémantique. Les fichiers ont été restaurés depuis HEAD.

## Conséquence honnête sur le self-hosting

Le rapport `SELF_HOSTED_REEL.md` concluait à **50 % des étapes du compilateur**
et **0 % de reconstruction du binaire sans rustc**. Ce constat reste vrai.
Les 5 écarts d'oracles ajoutent une précision : les jardins écrits en Rose
utilisent des formes (paramètres implicites, `type_de`, `len`, typage dynamique)
que le **vérificateur et l'émetteur Rose** ne savent pas encore traiter — alors
que le moteur Rust exécute ces jardins correctement.

Aucun de ces écarts n'affecte l'exécution : `./rose/rose verif <jardin>` répond
« vérifié » pour les 5 fichiers, et tous les jardins passent `rose-jardin.sh`.

## Rejouer

```bash
cd /home/clausake/Stefan/ROSE
CORPUS="$(find examples jardins bench rose-cpp/tests -name '*.rose' | sort)"
for f in $CORPUS; do
  ./rose/rose verif "$f" > /tmp/ref 2>&1
  ./rose/rose compilateur/verifieur.rose "$f" > /tmp/out 2>&1
  cmp -s /tmp/ref /tmp/out || echo "écart : $f"
done
bash scripts/audit-final.sh
```

Constat daté, pas consigne. Re-mesurer après tout changement des compilateurs Rose.
