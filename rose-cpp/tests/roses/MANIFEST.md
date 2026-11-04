# Roses à tester 🌹

Onze programmes de torture + démonstration, un par famille de features,
générés contre l'interpréteur C++ (la **spécification**), et vérifiés
identiques sur les quatre chemins : interpréteur, JIT, AOT (`rose build`),
moteurs Rust et Zig.

| rose | ce qu'elle torture | attendu |
|---|---|---|
| rose_symboles | notation symbolique : `⋅ × ÷ √ π ² ³ ^`, ← , fonctions flottantes | `.attendu` |
| rose_mixtes | arithmétique mixte rationnel⊛flottant, constants pliés et dynamiques | `.attendu` |
| rose_tables | tables complètes :inserer/cle_existe/supprimer/cles/valeurs/longueur/`==`, remplacement en place, **rationnel dans une table** (rendu « a/b ») et dans valeurs (rendu décimal) | `.attendu` |
| rose_puissances | `^` sous toutes ses coutures : plis constants, exposants négatifs, base flottante | `.attendu` |
| rose_boucles | `pour … vers`, `tant_que(…)`, `si`, indices de listes, boucles imbriquées | `.attendu` |
| rose_chaines | chaînes UTF-8 : longueur, sous_chaine (PLAGE début/fin), caractere(codepoint), joindre, concatener, chaine(rationnel)→« a/b », entier | `.attendu` |
| rose_grands | grands entiers GMP : factorielle(50), fibonacci(90), promotion par overflow, 2^200, nb_chiffres, pgcd, puissance | `.attendu` |
| rose_flottants | rendu flottant exact (règles d'affichage), Q⊛F | `.attendu` |
| rose_recursif | récursion (fibo), `tant_que` dans fonction, paramètres modifiés | `.attendu` |
| rose_cle_absente | erreur runtime « clé absente de la table » | `.attendu.err` (code 1 + message) |
| rose_div_zero | erreur runtime « division par zéro » | `.attendu.err` (code 1 + message) |
| rose_puissance_dyn | z^e à exposant non littéral : l'interprète peut produire un RATIONNEL dans un emplacement entier | `.attendu.err` + **rejet AOT attendu** (limite M4.2 documentée, jamais de faux silencieux) |

## Comparaison des rendus (piège à la mode M4.2)

Un même rationnel se rend **différemment selon le contexte**, et c'est la
spécification :

- `>> 3/4` → `0.75` (rendu ecri, décimal à période `[6]`)
- `$t["q"] = 3/4` puis `>> $t` → `[q => 3/4]` (rendu chaine_de, fraction)
- `>> valeurs($t)` → `[0.75]` (les listes rendent par ecri)
- `chaine(3/4)` → `"3/4"` (chaine_de)

Ces quatre lignes sont couvertes par rose_tables et rose_chaines.

## Sous Windows

`tester.ps1` exécute chaque rose sur les trois moteurs autonomes et
compare aux `.attendu` (octet pour octet, fins de ligne normalisées) ;
les deux roses d'erreur comparent le code de sortie et le message.
