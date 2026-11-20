# CLI_MORE — commande REPL `:more` et affichage de `detail()`

> Document written le 21/11/2026 (horodatage machine : 20/11 10:xx UTC).
> Mission CLI1. HEAD annoncé `af8164f` ; HEAD **mesuré** au moment de
> l'implémentation : `c3df12a` (un autre agent a commité MATH1 entre les deux).
> Fichiers touchés **uniquement** : `rose/src/repl.rs`, `rose/src/main.rs`,
> ce document. Aucun commit, aucune publication, aucun binaire installé
> (`CARGO_TARGET_DIR=/tmp/opencode/cli1_agents/target`).

---

## 1. Ce qui est réellement livré

### 1.1 Commandes

| Commande | Effet |
|---|---|
| `:more` | demande **30** chiffres de plus sur le dernier résultat symbolique |
| `:more N` | demande **N** chiffres de plus (`N` = entier décimal positif ou nul) |
| `:plus`, `:digits` | alias exacts de `:more`, y compris avec argument |

Les trois noms sont dans :

- l'aide `aide()` (`rose/src/repl.rs`) — 10 entrées au lieu de 8 ;
- la complétion Tab `COMPLETIONS` (`:mo` → `:more`) ;
- le `match cmd` du dispatch (10 lignes avant le rejet « commande inconnue ») ;
- l'aide courte de `depuis_arguments()` (`rose-shell --help`) ;
- l'usage CLI `usage()` dans `rose/src/main.rs` (lignes `rose console` / `rose repl`).

### 1.2 Parsing (fonctions publiques et testées)

- `parse_commande(ligne) -> (&str, &str)` — factorise le
  `split_once(char::is_whitespace)` qui était en ligne dans `lancer()`.
- `analyser_argument_more(arg) -> Result<usize, String>` :
  - chaîne vide ou espaces → `Ok(30)` ;
  - entier décimal → `Ok(N)` ;
  - tout le reste → `Err("rose: :more attend un nombre de chiffres, reçu : …")`
    (aucun panic, le buffer n'est pas modifié).

`:more 100` est donc parsé comme `(":more", "100")` puis `Ok(100)`.

### 1.3 Comportement exact

Dans `lancer()`, la branche `:more` :

1. parse l'argument (message rouge et `continue` si invalide) ;
2. appelle `appliquer_more(n)` ;
3. `Ok(texte)` → `println!("{}", texte)` ;
   `Err(e)` → `println!("{}", couleur.erreur(&e))`.

`appliquer_more(n)` consulte un **fournisseur** enregistré dans un
`thread_local!` :

- avec fournisseur : appelle `demander_plus(n)`, incrémente le compteur
  cumulé de `n`, rend la chaîne du fournisseur ;
- sans fournisseur : rend exactement
  `rose: :more nécessite d'abord detail(expr) (résultat non symbolique)`.

Le compteur `chiffres_demandes()` est remis à zéro par `:reset`, par `:load`,
par `:type`, et avant chaque nouvelle ligne de code exécutée : un nouveau
résultat n'est plus « le dernier résultat », donc l'ancien fournisseur est
oublié. C'est factuel dans le code (`oublier_plus_de_chiffres()`).

### 1.4 Affichage de la table de `detail(expr)` (INT1)

`detail(expr)` n'est **pas** implémenté ici (mission INT1). Le REPL fournit
le contrat de rendu, dans `format_table_detail(&crate::run::Table) -> Vec<String>` :

```rust
format!("  {:<28} : {}", libelle, valeur)
```

soit deux espaces, libellé aligné à gauche sur 28 caractères, séparateur
` : `, valeur via `run::chaine_de`. Exemple vérifié par test :

```
  type                         : entier
  longueur                     : 7
```

Aucune couleur ANSI dans cette fonction : la sortie reste capturable et
comparable octet pour octet, comme l'exige `DETAIL_UX.md` §2.2.

---

## 2. Dépendance à VAL1 — état factuel (re-mesuré en fin de mission)

Au début de la mission, `Valeur::Symbolique`, `ObjetParesseux`,
`demander_plus(k)` et `premiers_tronques()` étaient **absents** de `run.rs`.
VAL1 a livré entre-temps (fichier `run.rs` modifié, non commité à 11:05 UTC)
les éléments attendus, avec les signatures exactes :

- `Valeur::Symbolique(Rc<ObjetParesseux>)` (`run.rs:369`) ;
- `ObjetParesseux::demander_plus(&self, plus: usize) -> String` (`run.rs:514`) ;
- `ObjetParesseux::premiers_tronques(&self) -> String` (`run.rs:500`).

L'adaptateur a donc été **activé sans `#[cfg]`**, et un test sur un vrai
`ObjetParesseux::factorielle(100)` vérifie les chiffres réels. Aucune
modification de `run.rs` n'a été faite par CLI1 (interdiction respectée).

Le REPL définit son propre trait :

```rust
pub trait PlusDeChiffres {
    fn demander_plus(&self, k: usize) -> Result<String, String>;
}
pub fn enregistrer_plus_de_chiffres(f: Box<dyn PlusDeChiffres>);
pub fn oublier_plus_de_chiffres();
```

L'adaptateur `AdaptateurVal1` implémente ce trait pour
`Rc<ObjetParesseux>` en appelant `self.0.demander_plus(k)` ; la fonction
publique `repl::enregistrer_objet_val1(o)` le brancher en une ligne.

**Contrat d'intégration restant pour VAL1/INT1** (sans toucher `repl.rs`) :
au point où une `Valeur::Symbolique(o)` est affichée (dans `interp.rs`),
appeler `repl::enregistrer_objet_val1(o.clone())`. Tant que ce branchement
n'est pas fait, le REPL n'a pas d'objet à « reprendre » et `:more` rend le
message clair « nécessite d'abord detail(expr) ». Ce n'est PAS un défaut de
`:more` : le parsing, le dispatch, le compteur, le test sur objet réel et
l'affichage sont déjà en place. Dans l'arbre courant, VAL1 n'a ajouté aucune
primitive Rose créant une valeur symbolique (`grep symbolique` dans
`interp.rs` : 0 occurrence au moment de la mesure), donc le chemin
interactif complet n'est pas encore rejouable de bout en bout.

---

## 3. Tests et validation mesurés

Commandes exécutées avec `PATH="$HOME/.cargo/bin:$PATH"` et
`CARGO_TARGET_DIR=/tmp/opencode/cli1_agents/target` :

Mesurés en fin de mission, avec l'arbre contenant VAL1 (non commité) :

- `cargo build --release` → `Finished \`release\` profile … in 51.35s`, 0 erreur
  (warnings préexistants, dont le `dlopen` statique de `rose-setup`).
- `cargo test --release` → **245 passed, 0 failed, 3 ignored** (lib),
  `229 passed, 0 failed, 3 ignored` (binaire `rose`), `6 passed` (`rose-shell`),
  `1 passed` (`mul_cmp`), doc-tests 0. Aucun échec.
- 5 tests CLI1 verts dans `repl::tests` : `parsing_de_more`,
  `more_sans_fournisseur_ne_panique_pas`,
  `more_appelle_le_fournisseur_avec_le_bon_pas`,
  `more_sur_un_objet_val1_reel`, `gabarit_de_la_table_detail`.
  Le test réel vérifie `100!` : 30 chiffres
  `933262154439441526816992388562…`, puis 60
  `…667004907159682643816214685929…`, puis demande 160 chiffres → VAL1 rend
  les 158 chiffres complets **sans** `…` (comportement documenté de
  `premiers_tronques`), préfixe stable.

Contrôles de non-régression rejoués avec le binaire isolé
(`/tmp/opencode/cli1_agents/target/release/rose`, `rose/rose` inchangé) :

| Contrôle | Sortie mesurée |
|---|---|
| `jardins/tests/premiers.rose` | `── jardin/premiers : 20 tests, 0 échec ──` |
| `fibonacci(25)` | `75025` |
| `rose rir compilateur/emetteur.rose` | md5 `b3ed9262dd54776a6dbd07bc12368d0e` |
| `rose compilateur/emetteur.rose compilateur/emetteur.rose` | md5 `b3ed9262dd54776a6dbd07bc12368d0e` (×2, stderr séparé) |
| Valeurs 9 domaines (scripts `/tmp/opencode/cli1_agents/d1..d9.rose`) | fib35 `9227465`, somme 1..999999 `499999500000`, `compter_premiers(50000)` `5133`, `nb_chiffres(5000!)` `16326`, `pgcd` (petit oracle `9`), chaîne `20000`, liste `4999950000`, table `3000`, tampon `10485760` |

⚠ La case PGCD n'a pas été rejouée sur deux entiers de 1000 chiffres (oracle
`500` du banc) : seul un petit oracle `pgcd(1234567890, 987654321) = 9` a été
vérifié. Le banc de **performances** 9 domaines (`cmp2_results.csv`, scripts
dans `/tmp/opencode/cmp2_agents`, aujourd'hui absents) n'a pas été rejoué :
seules les **valeurs** ont été contrôlées.

---

## 4. Reste à faire

1. Brancher `repl::enregistrer_objet_val1(o)` au point de `interp.rs` où une
   `Valeur::Symbolique(o)` est affichée (hors périmètre CLI1).
2. INT1 livre la primitive Rose `detail(expr)` rendant une `Valeur::Table` ;
   le formatage est déjà prêt dans `format_table_detail`.
3. Rejouer alors les critères UX `DETAIL_UX.md` §4.1 en interactif
   (30 → 60 → 90 chiffres, `:more 10`, `:reset`) sur le binaire intégré :
   le test unitaire couvre déjà la mécanique, pas la frappe clavier.
