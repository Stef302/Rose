# SYMBOLIQUE — valeur paresseuse pour entiers géants (VAL1)

**État : implémenté et testé le 21/11/2026. Dépôt `/home/clausake/Stefan/ROSE`,
HEAD de départ `af8164fd00d6de316bc6279eec0682d3e33a29e7`. Aucun commit, aucune
publication. Conception : `DETAIL_CONCEPTION.md` (sous-agent RECH1), couche de
calcul : API MATH1 déjà commitée dans `i128x.rs`.**

---

## 1. Ce que fait VAL1

`Valeur::symbolique_factorielle(n)` crée un entier géant **décrit, non
matérialisé**. Son rendu est `<30 chiffres>…` (le nombre complet s'il tient en
moins de 30 chiffres). Le cas cible `10^37!` s'affiche en 30 chiffres sans
calculer le nombre : aucun `factorielle_parallele`, aucun `to_string()` d'un
BigInt géant sur ce chemin.

Le mode est **opt-in**. `n!`, `factorielle(n)` et `factorielle_parallele(n)`
gardent exactement leur sémantique et leur coût : aucun de ces trois corps
n'a été modifié. INT1 câblera la primitive Rose `entier_geant_factorielle(n)`.

---

## 2. Types ajoutés (`run.rs`)

| Type | Rôle |
|---|---|
| `EntierSymbolique { Factorielle(BigInt) }` | description de l'entier. Une seule variante pour l'instant. |
| `CacheParesseux` | `log10_entier`, `log10_fraction`, `premiers`, `chiffres_calcules`, `precision_premiers`. |
| `ObjetParesseux { kind, chiffres_demandes: Cell<usize>, cache: RefCell<CacheParesseux> }` | descripteur partageable, 30 chiffres par défaut. |
| `ProprietesParesseux` | `longueur`, `zeros_finaux`, `derniers_non_nuls`, `log10_entier`, `log10_fraction`, `log10_complet`. |
| `Valeur::Symbolique(Rc<ObjetParesseux>)` | 13ᵉ variante de `Valeur`. |

**Écart factuel avec la consigne littérale** : la consigne demandait
`Factorielle(u64)`. Or `u64::MAX = 18 446 744 073 709 551 615 ≈ 1,8×10¹⁹`,
tandis que `10³⁷` demande ~123 bits : `10^37` **ne tient pas dans un `u64`**.
Le document RECH1 affirme « 10^37 oui » (§3.1) : c'est une erreur arithmétique.
L'API MATH1 commitée prend du reste `&Gz` pour cette raison. Le stockage retenu
est donc `BigInt`, qui accepte aussi les petits `n`. Le constructeur public
`Valeur::symbolique_factorielle(n: u64)` demandé est fourni et délègue au
constructeur général.

**Pourquoi pas `Gz`** : `Gz` dérive `Clone` mais ne clone qu'un pointeur brut
(`i128x.rs:458`, `Drop` en 623). Le stocker dans une valeur clonable provoquerait
un double-free. Un `Gz` éphémère est construit à chaque appel MATH1 via
`Gz::de_bigint(n.clone())` et libéré normalement.

---

## 3. Branchements exacts dans `run.rs`

Deux branches, et deux seulement :

1. `Valeur::ty()` → `Valeur::Symbolique(..) => TyKind::Entier`. Un nouveau
   `TyKind` obligerait à modifier `front.rs` et tous les appels qui décident
   dessus, sans bénéfice fonctionnel : un symbole est un entier pour le langage.
2. `chaine_de_p()` → `Valeur::Symbolique(o) => o.premiers_tronques()`.

`afficher`, `afficher_formule` et `chaine_de` héritent du comportement par
leurs rejets `_` existants : **aucune autre modification d'un match sur
`Valeur`**. VAL1 ne modifie aucun autre fichier source que `run.rs` (d'autres
agents peuvent modifier `interp.rs`/`jit.rs`/`repl.rs` concurrently dans le
même arbre de travail, mais ces changements ne font pas partie de VAL1).

---

## 4. API publique (pour INT1)

```rust
// Constructeurs
Valeur::symbolique_factorielle(n: u64) -> Valeur          // consigne littérale
Valeur::symbolique_factorielle_big(n: BigInt) -> Valeur   // 10^37, 10^100…
Valeur::symbolique_factorielle_gz(n: &Gz) -> Valeur       // depuis vers_z()

// ObjetParesseux
ObjetParesseux::factorielle(n: u64) -> Self
ObjetParesseux::factorielle_big(n: BigInt) -> Self
ObjetParesseux::factorielle_gz(n: &Gz) -> Self
fn premiers_tronques(&self) -> String        // 30 chiffres + « … »
fn demander_plus(&self, k: usize) -> String   // +k chiffres, recalcule, rend le nouveau texte
fn longueur(&self) -> BigInt                  // nombre de chiffres, sans matérialiser
fn proprietes(&self) -> ProprietesParesseux   // k = 9 derniers chiffres non nuls
fn proprietes_avec_k(&self, k: usize) -> ProprietesParesseux
```

`ProprietesParesseux` est une structure à champs publics ; INT1 peut câbler
`detail()` en les lisant directement, ou faire `format!("{}", p.log10_complet)`.

**Règle de non-chevauchement** : `detail()` doit dispatcher sur la **variante**
de `Valeur` (`Valeur::Symbolique(o) => o.proprietes()`), jamais sur une chaîne.

**Coût** : `proprietes()` utilise k = 9. Au-delà, `derniers_non_nuls` de MATH1
retombe sur le produit direct O(n) (`i128x.rs:1539-1548`), intraitable pour
`10^37`. Mesuré : l'ensemble des 7 tests VAL1, y compris `proprietes()` sur
`10^37`, s'exécute en 0,53 s en release.

---

## 5. Oracles vérifiés (indépendants de MATH1)

| Cas | Longueur | Préfixe (30 chiffres) | Zéros finaux |
|---|---|---|---|
| `100!` | 158 | `933262154439441526816992388562` | 24 |
| `1000000!` | 5 565 709 | `826393168833124006237664610317` | 249 998 |
| `(10^37)!` | 365657055180967481723488710810833949196 | `901820028144300502340101362711` | 2499999999999999999999999999999999990 |

Sources : `math.factorial(1000000)` (Python, conversion autorisée à 10⁷
chiffres) et `mpmath` à 120/160 décimales pour `loggamma(10^37+1)/ln 10` :
`log10((10^37)!) = 365657055180967481723488710810833949195.955119876…`,
mantisse `9.01820028144300502340…`.

`demander_plus(30)` sur `10^37` donne 60 chiffres :
`901820028144300502340101362711510022742780300404027294022725…`
(oracle mpmath), et le préfixe de 30 reste bit-à-bit identique.

---

## 6. Tests

`#[cfg(test)] mod tests_val1` en fin de `run.rs` : 7 tests, tous verts en
release. Ils couvrent 100!, 1000000!, 10^37!, `demander_plus` (100! et 10^37),
l'absence de « … » pour 10! (nombre complet `3628800`), `ty() == TyKind::Entier`,
l'API `proprietes()` et l'intégrité du chemin exact.

---

## 7. Limites documentées

- `chiffres_demandes` est plafonné par la mémoire disponible ; la conception
  RECH1 proposait D ≤ 500, NON appliquée ici (aucun test au-delà de 60).
- `premiers_tronques()` ne matérialise jamais `n!`, même pour `n = 10^1000` :
  il ne passe que par `log10_factorielle_precision` et
  `premiers_chiffres_factorielle` de MATH1.
- Le nombre complet n'est accessible que par le chemin exact ordinaire ; il
  n'existe pas de matérialiseur pour un objet paresseux (hors périmètre).
- `:more`, le bouton GUI `more digits`, `detail()` et la primitive Rose
  `entier_geant_factorielle` restent à câbler par INT1 / l'agent GUI.
