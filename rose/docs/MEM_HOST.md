# MEM_HOST — chemin hébergé du compilateur : coût, profil, optimisation (22/11/2026)

Périmètre : `rose/src/run.rs`, `rose/src/interp.rs`, `rose/tests/mem_host.rs`
(nouveau) et ce document. Aucun fichier du compilateur, des scripts, de
`thorn/` ou des autres docs n'a été touché.
Binaire validé : `rose/target/release/rose` (build `cargo build --release`,
`lto=fat`, `codegen-units=1`). Aucun commit, aucune publication.

## 1. Point de départ

Deux chemins compilent le compilateur :

| chemin | état mesuré |
|---|---|
| natif G5 (C0) | 84 Gio RSS, swap épuisé, arrêté — allocateur tas montant sans GC (chantier séparé) |
| hébergé (`rose rose_natif.rose <source>`) | mémoire gérée par `Rc`, mais temps prohibitif : `emetteur.rose` > 420 s sans finir |

Le chemin hébergé n'avait **pas** de goulot de concaténation : `Valeur::Chaine`
porte déjà une rope (`ChaineRep`), `+` est O(1) (deux clones de `Rc`) et
`longueur` est O(1) (compte mémorisé). Le goulot réel est **`sous_chaine`** :

```
chaine_de(&a[0])        // Valeur::Chaine → ChaineRep::aplatir() : copie O(n) de TOUTE la chaîne
s.chars().collect()     // Vec<char> O(n) supplémentaire
cps[di..fi].iter()...   // 1..3 caractères rendus
```

Or le compilateur hébergé est un lexeur qui avance caractère par caractère :
`sous_chaine($src, $i, $i + 1)`, `sous_chaine($s, $i, $i + $m)`, etc.
Chaque tour recopiait donc l'intégralité de la source → **O(n²)**, avec en plus
une allocation de `Vec<char>` par tour.

## 2. Mesures (avant / après)

Méthode : `/usr/bin/time -v` + `bash scripts/garde_fou_mem.sh 16000000`, sources
de taille croissante. A/B **dans le même binaire** via `ROSE_PROF_NOFAST=1`
(rejoue l'ancien chemin) vs `ROSE_PROF_NOFAST` absent (chemin optimisé), les
deux côtés avec `ROSE_PROF_MEM=1` : le coût des compteurs s'annule.

| source | taille | ancien chemin | chemin optimisé | gain |
|---|---|---|---|---|
| `examples/fibonacci.rose` | 128 o | 7,22 s / 54 608 Ko | 5,69 s / 54 608 Ko | −21 % |
| `thorn/src/noyau.rose` | 54 075 o | 25,40 s / 55 400 Ko | **12,80 s / 55 568 Ko** | **−49,6 %** |
| `compilateur/emetteur.rose` | 155 504 o | > 420 s (rc=124, arrêt) | > 200 s (rc=124, arrêt) | non terminé dans la fenêtre, pic RSS 84 048 Ko |

Mesures complémentaires sans instrument (binaire d'origine) : fib 5,01 s /
50 160 Ko, noyau 25,08 s / 50 000 Ko. Le pic RSS reste **constant** avec la
taille de la source : ce n'est pas une fuite, c'est bien un coût de copie.

Compteurs (`ROSE_PROF_MEM=1`, sortie stderr en fin de processus via `atexit`,
car `quitter()` appelle `process::exit` et saute les destructeurs). Désactivés,
chaque site instrumenté ne coûte qu'une lecture atomique `Relaxed` déjà
initialisée (aucune allocation, aucun appel système) : 3 mesures de fib sans
variable d'environnement donnent 5,76 / 5,75 / 5,74 s, contre 5,69 s avec
compteurs actifs — différence dans le bruit).

| compteur | fib ancien | fib optimisé | noyau ancien | noyau optimisé |
|---|---|---|---|---|
| `aplatir` appels | 745 847 | 267 260 | 2 240 211 | 1 109 011 |
| `aplatir` octets copiés | 2 774 689 | 432 864 | 33 976 386 349 | 9 614 458 326 |
| `sous_chaine` appels | 478 587 | 478 587 | 1 131 200 | 1 131 200 |
| `tranche` appels | 0 | 478 587 | 0 | 1 131 200 |
| `concat` / `longueur` / `joindre` | 11 722 / 267 898 / 13 086 | identiques | 153 054 / 1 302 206 / 73 662 | identiques |

Donc 84 % des octets aplatis en moins sur fib, 72 % sur noyau, **sans toucher
ni à `+`, ni à `longueur`, ni à `joindre`** (leurs compteurs sont bit à bit
identiques des deux côtés).

## 3. Optimisation retenue

`ChaineRep::tranche(debut, fin)` (`rose/src/run.rs`) extrait une tranche de
points de code **sans aplatir la rope** :

- pile explicite sur les nœuds `Concat` (aucune récursion, pas de débordement
  de pile sur une rope profonde) ;
- une feuille ASCII est sautée en O(1) par découpe d'octets (`octet == caractère`) ;
- une feuille non-ASCII est munie d'une table `caractère → octet`
  (`Vec<u32>`, construite une seule fois dans `plein()` / `depuis_fichier()`
  quand la chaîne dépasse 32 octets) : saut en O(1) également ;
- les petites feuilles non indexées font un `char_indices` **local**, jamais global.

`appel_valeurs` (`sous_chaine` / `sub`) utilise `r.n` (déjà O(1)) pour valider
les bornes et retourne `Valeur::s(r.tranche(di, fi))`. Le repli historique
reste le même code, accessible avec `ROSE_PROF_NOFAST=1`. La sémantique est
inchangée : bornes négatives, `di > taille`, `fi < di`, clipping de `fi`, et
comptage en **points de code** (vérifié sur `a≈b𝄞c` : sorties identiques
fast/lent pour 4 tranches + `longueur`).

### Contrainte ABI rencontrée (et contournée)

`Valeur::Chaine` est un `Rc<ChaineRep>` : la taille de `Valeur` ne change pas.
Il était **impossible d'ajouter un champ à `ChaineRep`** sans modifier
`rose/src/jit.rs:9524` (fichier hors périmètre), qui construit la structure
littéralement. L'optimisation a donc été placée dans les **variantes de
`NoeudChaine`** (`PleinIndexe`, `FichierMmapIndexe`) : `jit.rs` continue de
construire `NoeudChaine::Plein(s)`, et toutes les correspondances existantes
utilisaient déjà `..`. Aucune signature publique, aucun layout de `Valeur`
et aucun appel JIT n'a changé.

## 4. Validation

- `cargo test --release` : **659 passed, 0 failed, 6 ignored** (rc=0) avant
  ajout des tests de non-régression ; `cargo test --release --test mem_host` :
  **2 passed, 0 failed** (total final **661 passed**).
- `bash scripts/validation_finale.sh` : point fixe
  `b3ed9262dd54776a6dbd07bc12368d0e` (identique des deux côtés), AOT 75/75,
  JIT 25/25, WASM 92 o, RVM 317811, AZ 31/31, HOST-1 fib=317811 — journal
  `/tmp/opencode/validation_finale.log`.
- Aucun fichier interdit modifié (`git status` : seuls `rose/src/interp.rs` et
  `rose/src/run.rs` portent des changements dans le périmètre, plus ce doc).

## 5. Recommandation C1

**Garder le natif G5 comme chemin principal**, mais **le chemin hébergé est
désormais une solution de rechange viable en mémoire** : pic RSS ~55 Mo sur
`noyau.rose`, ~84 Mo sur `emetteur.rose`, contre 84 Gio pour le natif.
Le gain est réel (−49,6 % sur `noyau.rose`) mais `emetteur.rose` ne termine
toujours pas en 200 s. La cause
résiduelle est mesurée : ~9,6 Gio d'octets aplatis sur `noyau.rose` après
l'optimisation, donc d'autres primitives (`chaine_de` sur résultats de
concaténation, comparaisons de chaînes, `eval_expr`, mise en forme) aplatissent
encore des ropes entières.

Ordre recommandé pour C1 :

1. **C1 par le natif G5** dès que l'allocateur avec récupération sera disponible
   (c'est le seul chemin qui produit l'ELF complet en un temps raisonnable).
2. **C1 par le hébergé en repli de correction** : il sert déjà d'oracle (RIR
   octet pour octet, AZ 31/31) et sa mémoire est bornée. Ne pas l'utiliser
   comme chemin de production tant que `emetteur.rose` ne termine pas.
3. Prochain changement minimal (hors périmètre de ce chantier) : appliquer
   `ChaineRep::tranche` à **toutes** les conversions `chaine_de` sur
   `Valeur::Chaine` qui ont besoin d'un préfixe/suffixe, puis équilibrer les
   ropes à la concaténation (la profondeur d'une rope construite par
   `$s = $s + $c` reste linéaire). Ce sont les deux seules sources restantes
   d'aplatisement mesurées.
