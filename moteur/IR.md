# RIR — Représentation Intermédiaire de Rose (jalons M1+)

L'IR est le centre de gravité du moteur : le vérificateur émet du RIR,
tous les modes d'exécution (interpréteur RIR, JIT, LLVM) consomment du
RIR. Une seule sémantique, trois backends.

## Forme générale

Un module RIR = en-tête + constantes + fonctions. Texte imprimable,
diffable (les dorés d'IR sont des fichiers de comparaison comme les
dorés de sortie). Chaque fonction Rose (y compris `principal`) devient
une fonction RIR à blocs de base, en SSA à un seul assignment par
nom.

```
rir 1
const z @c2 = 2                  # littéraux entiers (mpz)
const q @c2/3 = 2/3              # littéraux rationnels (mpq)
const f @c2.5 = 2.5              # flottants : bits IEEE-754 exacts
const s @ greeting = "bonjour"

fonction $fact(z %n) -> z {
.entree:
  %c0 = cst.z @c0
  %t1 = cmp.lt.z %n, @c1         # booléen b1
  br.cond %t1 .alors .sinon
.alors:
  ret.z @c1
.sinon:
  %t2 = sub.z %n, @c1
  %t3 = call $fact(z %t2) -> z
  %t4 = mul.z %n, %t3
  ret.z %t4
}
```

## Types

| type RIR | valeur | représentation C++ |
|----------|--------|--------------------|
| `z`  | entier arbitraire | `mpz` (GMP) |
| `q`  | rationnel exact   | `mpq` (GMP) |
| `f`  | flottant IEEE-754 double | `f64` |
| `b`  | booléen | `i1` |
| `s`  | chaîne UTF-8 | pointeur + longueur |
| `l<T>` | liste homogène | tableau de T + longueur |
| `u`  | vide (unit) | — |

## Famille d'instructions (M1)

- arithmétique typée par opérande : `add/addf/addq`, `sub…`, `mul…`,
  `div` (→ q), `divf`, `tdiv.z` (troncature entière), `rem.z`, `pow…`
  — **mêmes garde-fous d'erreur que le moteur actuel** (division par
  zéro, exposant démesuré) : l'émission place le test et saute vers
  le bloc d'erreur ;
- comparaisons : `cmp.<op>.<z|q|f|s>` → `b` ;
- contrôle : `br .label`, `br.cond %b .a .b`, `ret.<type>`,
  `erreur "message"` (l'exception Rose, unique point de sortie) ;
- appels : `call $nom(args…) -> type` (fonctions user),
  `call @builtin.z(args…)` (builtins → appels C vers GMP/moteur),
  `call @ext.bibliothèque.symbole(args…)` (FFI `@dll`, après
  résolution du symbole) ;
- structures : `liste.new<T>`, `liste.push`, `liste.get`,
  `liste.len`, `chaine.de <type>` (affichage canonique),
  `chaine.cat`, `saisir.<type>` ;
- effets : `afficher.<type>` (via `chaine.de`), `lecture` ;
- aucun UB possible : pas d'arithmétique non typée, pas de
  conversion silencieuse en RIR — toute promotion (entier→rationnel
  etc.) est matérialisée par une instruction `prom.zq`, `prom.zf`,
  etc., émise par le vérificateur.

## Contrat de migration

1. Le vérificateur existant garde la main sur les types ; l'émission
   RIR est un nouveau passage après `replier_constants` :
   arbre vérifié → RIR.
2. Un **interpréteur RIR** remplace l'interpréteur d'arbre ; le dump
   RIR textuel d'un programme devient testable (`rirs/` dorés).
3. Filet permanent à chaque étape : dorés 25 cas, parité 89, v2 147,
   `tester.sh`/`tester.ps1` verts.
4. JIT (M2) et LLVM (M3) n'ajoutent qu'un consommateur de RIR : ni la
   sémantique, ni les tests ne bougent.
5. L'auto-hébergement (M4) : `compilateur.rose` émettra le même RIR
   textuel, lu par le moteur — le format est un contrat public stable.

## Choix tranchés (et pourquoi)

- **SSA fonctionnelle, pas de φ explicites au départ** : les boucles
  passent par des blocs avec arguments (`.corps(%i1)`) — plus simple à
  émettre depuis l'arbre, et le JIT/LLVM les convertissent en
  enregistrements ou φ à leur tour.
- **Entiers/rationnels restent des appels GMP en RIR 1** : le JIT
  inline les cas petits (fits 64 bits) plus tard, l'IR ne change pas.
- **Le format texte est canonique** (tri stable, une instruction par
  ligne) : les dorés d'IR diffent au caractère comme les sorties.
