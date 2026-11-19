# Fidélité des oracles M4 — état final (19/11/2026)

## Constat daté (mesuré, pas cité)

Sous-agent « continue self host ». Binaire construit dans
`/tmp/opencode/jd_agents/rose` (depuis `rose/Cargo.toml`, `cargo build --release`,
`CARGO_TARGET_DIR=/tmp/opencode/jd_agents/target`, garde-fou `systemd-run
--user --scope -p MemoryMax=4G`). `rose/rose` n'a **pas** été remplacé.
Mesures pinées `taskset -c 10`. Aucun commit, aucune publication.

### Compteurs

| Oracle | AVANT (corpus 162) | APRÈS (corpus 165) |
|---|---:|---:|
| M4.1 lexeur | 162/0 | **165/0** |
| M4.2 analyseur | 162/0 | **165/0** |
| M4.3 vérificateur | **157/5** | **165/0** |
| M4.4 émetteur RIR | **157/5** | **165/0** |

« AVANT » re-mesuré à 10:32:46+01:00 avec les sauvegardes HEAD des deux
compilateurs (`/tmp/opencode/jd_agents/*.head`) sur le corpus initial de 162
fichiers : `157/5` pour M4.3 et M4.4, écarts = exactement les 5 fichiers
annoncés. « APRÈS » re-mesuré à 10:21:05→10:27:07+01:00 : **165/0 sur les 4**.

Le corpus est passé de 162 à **165** fichiers pendant le chantier : trois
jardins `gui_native` ont été ajoutés par un autre agent
(`jardins/gui_native.rose`, `jardins/tests/gui_native_demo.rose`,
`jardins/tests/gui_native_fenetre.rose`). Ils étaient initialement en écart
(`fenetre_attendre` inconnue) et ont été corrigés aussi — voir cause 5.

### Point fixe M4.5

| | MD5 |
|---|---|
| AVANT (`bdbef636…`, reproduit sur la sauvegarde HEAD) | `bdbef63681608a0b22a5637539e9d738` |
| APRÈS — `rose rir compilateur/emetteur.rose` | `de5e7b5465bb850e300891abbfa8fa5a` |
| APRÈS — `rose compilateur/emetteur.rose compilateur/emetteur.rose` | `de5e7b5465bb850e300891abbfa8fa5a` |

**Le hash a changé** parce que la source de l'émetteur a été modifiée ; les
deux voies restent **idempotentes octet pour octet**. Le nouveau hash est
`de5e7b5465bb850e300891abbfa8fa5a`. L'ancien `bdbef636…` n'est plus le point
fixe courant.

### Autres portes

- `cargo test --release` : 198 + 214 + 6 + 1 + 0 + 0 tests, **0 échec**.
- `./rose/rose jardins/tests/premiers.rose` : `20 tests, 0 échec`.
- `fib(25)` : `75025`.
- `./rose/rose verif` sur les 8 fichiers en écart historique : **8× « vérifié »**.

---

## Causes corrigées, une par une, avec mesure

Toutes les règles ajoutées sont des **miroirs explicites de `rose/src/front.rs`**
(ligne citée en commentaire dans le code). Aucune détente ad hoc de types.

1. **Paramètres sans annotation** (`compilateur/verifieur.rose` et
   `compilateur/emetteur.rose`, `def_fn`). Le vérificateur appelait `typex()`
   même sans `:` et consommait le `)` ou `,` suivant : le paramètre `w` de
   `html_widget(w)` avait le type `)` → `Index sur un )`. Correctif : sans
   annotation, émettre directement `?|vide` sans consommer de jeton (le `|`
   est indispensable : `NOD()` fait `sous_chaine(q+1,n)` et planterait sur un
   `?` nu — piège signalé dans `ORACLES_FIDELITE_RESTANTE.md`). Mesure :
   `Index sur un )` → `Index sur un ?` sur `jardins/gui.rose`, puis plus rien
   après la cause 3.

2. **Appel de fonction avec paramètre `Dyn`** (`front.rs:2157`). Ajout de
   `BASE($t_att) != "?"` comme garde : un paramètre sans annotation accepte
   n'importe quel argument. Mesuré indirectement : aucune régression sur les
   162+ fichiers.

3. **Typage dynamique `?`** — la cause la plus épaisse (miroir `front.rs:1589,
   1585-1587, 1917, 1986-2084`). Ajout, dans les deux compilateurs :
   - index sur une base `?` accepté, résultat `?|vide` (`front.rs:1989`) ;
   - unaire hors `non` sur `?` → `?|vide` (`front.rs:1917`) ;
   - comparaison `==/!=…` et ordre `< <= > >=` avec un `?` acceptés
     (`front.rs:2021,2035`) ;
   - `+`, `-`, `*`, `/` avec un `?` → `?|vide` (`front.rs:2044,2058,2070`) ;
   - affectation à une variable `?` ou depuis une valeur `?` acceptée
     (`front.rs:1589`) ;
   - retour de fonction dont le type déclaré est `?` : n'importe quel type
     accepté (`front.rs:1701`) — **cette dernière règle existait déjà dans
     l'émetteur, elle manquait seulement dans le vérificateur**.

   Mesures intermédiaires (vérificateur, sur `jardins/gui.rose`) :
   `Index sur un )` → `Index sur un ?` (cause 1) → **`vérifié`** (cause 3).
   `jardins/tests/gui_demo.rose` : `'c' est table, on ne peut pas y stocker du
   chaine` → **`vérifié`** après retrait du repli vers `$gn` (voir ci-dessous).
   `jardins/math.rose` et `jardins/tests/math.rose` : `La fonction retourne
   rationnel, attendu ?` → **`vérifié`** après la règle de retour Dyn.

   **Corollaire non trivial** : le vérificateur avait, contrairement à
   l'émetteur, un repli `si ($ex == "") { $ex = env_get($nom, $gn, $gt) }`
   dans le contrôle d'affectation. Dans `echapper()` de `gui.rose`, `$c` est
   une *locale* chaine mais `$c` est aussi une *globale* table déclarée dans
   `gui_demo.rose` : le repli comparait la locale à la globale et rejetait.
   `front.rs:1585-1587` traite une globale comme un **shadow** (`existante =
   None`). Retirer le repli (l'émetteur ne l'a pas) reproduit exactement le
   moteur. Mesure : `vérifié` sur les deux fichiers GUI, sans régression.

4. **`len` et `type_de` absents de `std_sig`** (`front.rs:650-651`). Ajout de
   `len → entier|lct` (identique à `longueur`) et `type_de → chaine|tout`
   (code `tout` = non vérifié par `vargs`, comme `afficher_erreur`). Mesure :
   `Fonction 'type_de' inconnue` (l.284/l.495), `Fonction 'len' inconnue`
   (l.31) → plus aucune de ces erreurs.

5. **`[0 : chaine]` typé `liste<entier>` par le vérificateur** — cause de
   `concatener_jardin : liste<entier> reçu, liste<chaine> attendu`. Le moteur
   Rust fait `Expr::ListeVideTypee(elt) → Ty::liste(elt)` (`front.rs:1904`),
   donc `liste<chaine>`. Le vérificateur Rose fabriquait un littéral `0`
   factice et en déduisait `liste<entier>`. Correctif : pour `chaine`,
   élément factice de genre `ch` (texte vide) au lieu de `lit "0"`.
   Mesure : `Argument de concatener_jardin…` → **`vérifié`**. L'émetteur avait
   déjà le bon encodage (`lstvide`), aucune modification nécessaire.

6. **Signatures STDLIB manquantes pour les primitives GUI natives** —
   découvert après coup, quand le corpus est passé à 165 fichiers.
   `front.rs:709-716` contient `time`, `ms`, `fenetre_native`,
   `fenetre_afficher`, `fenetre_attendre`, `fenetre_fermer`, `message_native` ;
   aucune n'était dans les tables Rose. Ajout des 7 signatures dans les deux
   compilateurs. Mesure : `Fonction 'fenetre_attendre' inconnue` → **`vérifié`**
   et RIR identique octet pour octet sur les 3 nouveaux jardins.

---

## Ce qui reste

**Rien sur le corpus mesuré** : 165/0 sur les quatre oracles, point fixe
idempotent, 0 test en échec, jardin 20/20, fib25 conforme.

Limites honnêtes, hors périmètre ou non mesurées :
- Le typage dynamique est maintenant **`?` (paramètres/retours sans
  annotation)** et **`valeur` (index de table)** : deux encodages distincts
  subsistent. Leur conflit potentiel n'a pas été exploré au-delà du corpus ;
  aucun fichier mesuré ne le déclenche.
- Seul `[0 : chaine]` a été corrigé dans le vérificateur ; les autres
  annotations de liste vide (`flottant`, `booleen`, `rationnel`, `complexe`,
  `table`) gardent le comportement historique `liste<entier>`. Le corpus
  n'en contient aucune (`[0 : entier]` ×13, `[0 : chaine]` ×1), donc c'est un
  constat de non-couverture, pas une affirmation de conformité.
- Le vérificateur ne vérifie pas les corps de fonctions après la première
  erreur (garde `longueur($err) > 1`) : comportement historique conservé.
- D'autres agents modifient le dépôt en parallèle (HEAD passé de `d20bed0` à
  `7cdf810` durant le chantier, `rose/rose` reconstruit à 10:07). Toutes les
  mesures ci-dessus ont été prises **après** ces changements ; elles sont des
  constats datés, à re-mesurer après toute modification supplémentaire.

## Rejouer

```bash
cd /home/clausake/Stefan/ROSE
CORPUS="$(find examples jardins bench rose-cpp/tests -name '*.rose' | sort)"
for f in $CORPUS; do
  ./rose/rose verif "$f" > /tmp/ref 2>&1
  /tmp/opencode/jd_agents/rose compilateur/verifieur.rose "$f" > /tmp/out 2>&1
  cmp -s /tmp/ref /tmp/out || echo "ecart verif : $f"
  ./rose/rose rir "$f" > /tmp/ref 2>&1
  /tmp/opencode/jd_agents/rose compilateur/emetteur.rose "$f" > /tmp/out 2>&1
  cmp -s /tmp/ref /tmp/out || echo "ecart rir : $f"
done
M1="$(./rose/rose rir compilateur/emetteur.rose | md5sum | awk '{print $1}')"
M2="$(/tmp/opencode/jd_agents/rose compilateur/emetteur.rose compilateur/emetteur.rose | md5sum | awk '{print $1}')"
echo "$M1 / $M2"
```

Fichiers possédés et modifiés : `compilateur/verifieur.rose`,
`compilateur/emetteur.rose`, `rose/docs/ORACLES_FIDELITE_FINALE.md`.
Aucun autre fichier touché.
