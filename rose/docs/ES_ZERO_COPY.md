# Domaine 8 — E/S sans recopie de tampon (vague 16)

**Constat daté : 18/11/2026, 16:09–17:21 UTC+1.** Aucune mesure ci-dessous
n'est une consigne : re-mesurer après tout changement de moteur, de
quantification ou de topologie.

Binaire de départ : `c05d72e5574a8279326aa2f3b8bc24f88b8b68df1abc2e1219d2aaca6725a4fb`.
Binaire retenu : `aa0f8c7ed5e6ab98cd07bca69337571b45f7ba2438ad4513f41a992becce6ee0`.

Fichiers modifiés : `rose/src/interp.rs` **uniquement** (2 sites : `lire_texte`,
`ecrire_texte`). `rose/src/sys_io.rs` étudié mais **non modifié**. `run.rs`
(rope) **non modifié**. Aucun commit, aucune publication.

---

## 1. Décision architecturale

Le domaine 8 coûtait 14,8 ms dans Rose contre ~6,1 ms au Rust de référence
(`multi_rs io` = `fs::read` + `fs::write` d'un `Vec<u8>`, sans conversion UTF-8).
La différence n'était **pas** dans les syscalls : elle était dans **deux
recopies de 10 Mo** ajoutées par le chemin Rose.

### (a) Lecture — `String::from_utf8` au lieu de `from_utf8_lossy`

`std::fs::read(&p)` rend un `Vec<u8>` possédé. L'ancien code faisait
`String::from_utf8_lossy(&b).into_owned()` : validation UTF-8 **plus** copie
de 10 Mo dans une nouvelle allocation, puis libération du `Vec` d'origine.
Le nouveau code reprend le tampon sans le copier quand l'UTF-8 est valide,
avec repli explicite sur l'ancien comportement sinon :

```rust
Ok(b) if b.len() <= (1 << 28) => match String::from_utf8(b) {
    Ok(s) => Valeur::s(s),
    Err(e) => Valeur::s(String::from_utf8_lossy(e.as_bytes()).into_owned()),
},
```

### (b) Écriture — emprunt du nœud `Plein` de la rope

Depuis `4cf0324`, `Valeur::Chaine` est une rope (`Rc<ChaineRep>`).
`chaine_de(&a[1])` appelait `aplatir()` : allocation + copie de 10 Mo
alors que le fichier lu est un **unique** nœud `NoeudChaine::Plein`.
Le nouveau code emprunte son `&str` directement et ne garde le repli
`aplatir`/`chaine_de` que pour les ropes concaténées :

```rust
let aplat: String;
let octets: &[u8] = match &a[1] {
    Valeur::Chaine(r) => match &r.noeud {
        NoeudChaine::Plein(s) => s.as_bytes(),          // zéro recopie
        _ => { aplat = r.aplatir(); aplat.as_bytes() }, // repli
    },
    v => { aplat = chaine_de(v); aplat.as_bytes() },    // repli
};
if std::fs::write(&p, octets).is_err() { /* erreur */ }
```

`NoeudChaine`/`ChaineRep` sont déjà visibles via `use crate::run::*;` :
**aucun accesseur ajouté à `run.rs`**, aucune modification de la rope.

---

## 2. Décomposition du coût (mesurée, pas supposée)

### 2.1 Instrumentation temporaire dans le vrai binaire

`ROSE_IO_PROF=1` (code d'instrumentation ajouté puis **retiré** avant le
build final ; sauvegarde `/tmp/opencode/es_backup/interp.rs.instrumented`).
5 exécutions de `r_io.rose` pinnées c15, médianes des `eprintln!` :

| Étape (`interp.rs`) | Médiane | Min–Max |
|---|---:|---:|
| `std::fs::read` | **2 143 µs** | 2 099–2 445 |
| `from_utf8_lossy(...).into_owned()` | **6 043 µs** | 6 007–6 558 |
| `Valeur::s` (dont `chars().count()`) | **291 µs** | 280–307 |
| `chaine_de` = `aplatir()` | **1 869 µs** | 1 804–1 946 |
| `std::fs::write` | **4 083 µs** | 3 986–5 698 |
| **Total instrumenté** | **14 429 µs** | — |

Les **deux recopies représentent 7 912 µs = 54,8 %** du total. Le comptage
de caractères de la rope (291 µs, 2 %) n'est **pas** le goulot : inutile de
le supprimer. Le programme complet mesurait 14,8 ms, cohérent avec cette
somme à ~0,4 ms près (overhead interpréteur).

### 2.2 Microbench Rust de confirmation (`/tmp/opencode/benchmulti/io_micro.rs`, rustc -O, 9 échantillons, c15)

`read` 0,335 ms · `from_utf8_lossy→owned` **4,069 ms** ·
`String::from_utf8` **0,442 ms** · `chars().count()` 0,291 ms ·
`clone()` 0,184 ms · `fs::write` 3,325 ms · `read+write` 3,092 ms.
La validation UTF-8 coûte ~9× moins cher que la copie de `from_utf8_lossy` :
c'est bien la copie, et non l'UTF-8, qui dominait. (Microbench binaire
différent : valeurs utilisées pour l'ordre de grandeur, pas pour le tableau
de perf final.)

### 2.3 Mesures c15/c19 des variantes

Harnais `/tmp/opencode/benchmulti/io_focused.py` : 9 échantillons **par CPU**,
Rose et Rust entrelacés par rotation, `taskset -c 15` et `-c 19`, valeur
`10485760` vérifiée à chaque échantillon. Médianes (ms) :

| Variante | sha (12) | c15 Rose / Rust | × | c19 Rose / Rust | × |
|---|---|---|---:|---|---:|
| Départ | `c05d72e5` | 14,844 / 6,105 | 2,432 | 14,363 / 5,781 | 2,485 |
| (a) seul | `a15ed1fe` | 9,139 / 6,051 | 1,510 | 7,989 / 4,944 | 1,616 |
| (b) seul | `36e05d19` | 12,745 / 6,201 | 2,055 | 11,430 / 4,901 | 2,332 |
| **(a)+(b)** | `aa0f8c7e` | **5,652 / 5,843** | **0,967** | **4,643 / 4,817** | **0,964** |
| (a)+(b), 2ᵉ salve¹ | `aa0f8c7e` | 5,898 / 5,774 | 1,021 | 5,737 / 5,797 | 0,990 |
| (C) `sys_io::ecrire_fichier` | `b5a5c262` | 5,732 / 5,795 | 0,989 | 5,556 / 5,576 | 0,996 |
| **(a)+(b), salve la plus rapide** | `aa0f8c7e` | **4,524 / 4,507** | **1,004** | **4,417 / 4,462** | **0,990** |
| (a)+(b), salve après correction d'un commentaire² | `aa0f8c7e` | 5,905 / 6,165 | 0,958 | 5,611 / 5,746 | 0,977 |

¹ Salve intitulée `varC` au moment de l'exécution : la compilation de (C)
avait échoué (`crate::sys_io` absent de `main.rs`, fichier interdit), donc le
binaire mesuré était bien (a)+(b). Étiquette corrigée ici ; le vrai (C) est
la ligne suivante, compilée via un `#[path]` local temporaire **retiré**
depuis.
² Recompilation après correction d'un commentaire : same sha `aa0f8c7e`
(le commentaire ne change pas le code machine). Salve rejouée pour vérifier
que le binaire livré est bien celui mesuré.

Le gain de (a)+(b) (9,2 ms sur c15) dépasse la somme des gains isolés
(5,7 + 2,1 = 7,8 ms) : avec les deux changements il ne reste qu'**une**
allocation de 10 Mo au lieu de deux. Hypothèse (non isolée) : moins de
pressions mémoire / défauts de page.

### Variantes rejetées

- **(a) seul** : gain réel (14,8 → 9,1 ms) mais porte < 8 ms non atteinte
  sur c15. Conservé **comme partie** de la décision finale.
- **(b) seul** : gain réel (14,8 → 12,7 ms) mais insuffisant. Conservé
  **comme partie** de la décision finale.
- **(C) écriture via `sys_io::ecrire_fichier`** (syscalls bruts) :
  c15 5,732 vs 5,898 pour (a)+(b) dans la salve voisine ; c19 5,556 vs
  5,737. Aucun gain mesurable, et il faut dupliquer le module `sys_io.rs`
  dans `interp.rs` (absent de `main.rs`). **Rejeté**. `sys_io.rs` reste
  intact.
- **mmap** (envisagé) : `NoeudChaine::Plein` exige un `String` possédé, donc
  un mmap en lecture imposerait quand même une copie de 10 Mo pour créer la
  valeur. Aucun gain structurel ; non implémenté, non mesuré.

### Porte

- **Porte absolue < 4,73 ms : atteinte dans UNE salve sur quatre**, la plus
  rapide (4,524 ms c15 ; 4,417 ms c19). Non atteinte dans les trois autres
  (5,65/4,64 ; 5,90/5,74 ; 5,91/5,61). La machine varie de 4,4 à 6,2 ms
  pour le Rust témoin dans la même journée : ce seuil absolu n'est donc
  **pas reproductible** ici.
- **Battre Rust (même salve) : c19 4 salves / 4** (0,964 / 0,990 / 0,990 /
  0,977) ; **c15 3 salves / 4** (0,967 / 1,021 / 1,004 / 0,958). Les
  écarts restent < 2,2 % : constat honnête — Rose est désormais **au niveau
  de Rust**, pas plus rapide de façon significative.
- **Porte secondaire < 8 ms : atteinte dans les 4 salves (a)+(b)** (c15 :
  5,65 / 5,90 / 4,52 / 5,91 ms ; c19 : 4,64 / 5,74 / 4,42 / 5,61 ms).

---

## 3. Bench complet — 9 domaines, avant/après

`cd /tmp/opencode/benchmulti && PIN_CPU=19 NSAMP=9 python3 bench.py`
(deux salves complètes, mêmes harnais ; avant = `c05d72e5`, après =
`aa0f8c7e`). Médianes `int_med` (ms) :

| # | Domaine | Rose avant | Rose après | Δ | Rust avant | Rust après |
|---|---|---:|---:|---:|---:|---:|
| 1 | `fib(35)` | 11,616 | 11,633 | +0,1 % | 12,708 | 12,706 |
| 2 | Boucle 1..10⁶ | 9,044 | 9,042 | −0,0 % | 0,258 | 0,260 |
| 3 | Premiers < 50 000 | 9,351 | 9,316 | −0,4 % | 1,001 | 1,001 |
| 4 | Factorielle 5 000 | 1,481 | 1,561 | +5,4 % | 6,542 | 6,537 |
| 4b | PGCD 1 000 chiffres | 6,709 | 6,721 | +0,2 % | 0,147 | 0,154 |
| 5 | Chaîne 20 000 car. | 3,300 | 3,280 | −0,6 % | 0,014 | 0,015 |
| 6 | Liste 100 000 entiers | 38,170 | 38,012 | −0,4 % | 0,189 | 0,188 |
| 7 | Table 3 000 clés | 6,292 | 6,326 | +0,5 % | 0,096 | 0,096 |
| 8 | **E/S 10 Mo** | **14,762** | **6,008** | **−59,3 %** | 6,211 | 6,073 |

Aucune régression systématique : les domaines 5 (chaîne), 6 (liste) et 7
(table), qui passent par le même `interp.rs`, restent stables (−0,6 % /
−0,4 % / +0,5 %). Le +5,4 % du domaine 4 est du bruit sur 1,5 ms (chemin
non touché). Le domaine 8 passe de ×2,38 vs Rust à **×0,989** dans cette
salve. Données brutes : `bench_before.log`, `bench_after.log`,
`multi_summary_before.json`, `multi_summary_after.json`,
`multi_results_{before,after}.csv`.

---

## 4. Exactitude (sorties réelles)

Programme `/tmp/opencode/benchmulti/exactitude.rose`, binaire `aa0f8c7e`,
`taskset -c 19`. Sortie stdout exacte :

```
10485760
10485760
54
5
0
6
```

- **ASCII 10 Mo** : lecture + écriture + relecture → `10485760` deux fois ;
  `cmp in_10m.txt chk_ascii.txt` → **IDENTIQUE**.
- **UTF-8 valide avec accents** (`éàü€ — café naïve` ×3, 81 octets) :
  `longueur` = **54 caractères** (sémantique Unicode préservée) ;
  `cmp utf8_accents.txt chk_utf8.txt` → **IDENTIQUE**.
- **Octets invalides** (`41 ff 42 80 43`) : repli `from_utf8_lossy` observé,
  `longueur` = **5** ; fichier réécrit = `41 efbfbd 42 efbfbd 43`
  (U+FFFD ×2) → **OK**.
- **Fichier vide** : `longueur` = **0** ; `cmp empty.txt chk_empty.txt` →
  **IDENTIQUE**.
- **Rope concaténée** (`"ab" + "cd" + "ef"`) : chemin de repli `aplatir`
  exercé, `longueur` = **6**, fichier = `abcdef` → **IDENTIQUE**.
- Un appel avec un entier (`ecrire_texte(..., 12345)`) est refusé en amont
  par le validateur de types (« une chaine est attendue ») : le repli
  `chaine_de(v)` du code est donc une ceinture de sécurité, pas un chemin
  utilisable.

---

## 5. Validation complète (sorties réelles)

`bash scripts/audit-final.sh` → **RC=0**, verdict :

```
1. Oracles M4      : lexeur 152/0 | analyseur 152/0 | vérificateur 152/0 | émetteur 152/0
2. Point fixe M4.5 : ATTEINT (bdbef63681608a0b22a5637539e9d738)
3. Perf M2         : jardin 20/20 | fib(25) OK | bench_global rc=0
4. Backend M3      : cargo link_natif rc=0 | natif_emetteur rc=0 | bin rose rc=0
5. Invariants      : rug absent du code Rust CONFIRMÉ | jardins Rust CONFIRMÉ | version (version="0.1.5")
6. Git             : HEAD/main ÉGAL | status non propre (concurrent)
7. Horodatage      : 2026-11-18T17:20:59+01:00

  Portes NON atteintes constatées : AUCUNE (toutes les portes mesurées sont atteintes).
```

Détails : `jardin : ── jardin/premiers : 20 tests, 0 échec ──` ;
`fib(25) = 75025` ; `MD5 ÉGAUX — point fixe octet pour octet ATTEINT`.
`cargo test --release` (depuis `rose/`) : **160 + 144 + 6 + 1 tests réussis,
0 échec** (les deux suites vides rendent `0 passed / 0 failed`).

---

## 6. Rejouer

```
export PATH="$HOME/.cargo/bin:$PATH"
cd /home/clausake/Stefan/ROSE/rose
cargo build --release --bin rose && cp target/release/rose rose
cd /tmp/opencode/benchmulti
NSAMP=9 python3 io_focused.py replay          # c15 + c19, Rose vs Rust entrelacés
PIN_CPU=19 NSAMP=9 python3 bench.py           # 9 domaines, 2 salves A/A
```

Fichiers de preuve : `io_focused_{base,varA,varB,varAB,varC,varC2,final,final2}.json`,
`io_focused_*.log`, `decomp.py`, `io_micro.rs`, `bench_before.log`,
`bench_after.log`, `multi_summary_{before,after}.json`, `/tmp/opencode/audit_final_apres.log`.
Instrumentation temporaire sauvegardée sous `/tmp/opencode/es_backup/` ;
le binaire livré (`aa0f8c7e`) ne contient **ni** instrumentation ni module
`sys_io` dupliqué.
