# DETAIL_INT — câblage des primitives « grands nombres » dans l'interpréteur

**Constat daté : 21/11/2026** (horodatage machine 20/11, la machine est réglée un
jour en retard). Dépôt `/home/clausake/Stefan/ROSE`, HEAD de départ `b8dfbe2`.
Mission INT1. Binaire isolé :
`/tmp/opencode/int1_agents/target/release/rose` — `rose/rose` **n'a pas été
remplacé**. Aucun commit, aucune publication.

---

## 1. Verdict court

`detail()`, `chiffres()`, `plus_de_chiffres()`, `entier_geant_factorielle()` et
l'affichage paresseux sont **câblés et testés depuis un vrai programme Rose**.
Le pont GUI1 est **fonctionnel** : depuis un `.rose`, on crée une vraie fenêtre,
de vrais boutons enfants, on reçoit le clic (prouvé sous Xvfb pour X11 et sans
écran avec le backend simulé), on incrémente le champ « chiffres » du jardin.

Trois fichiers sources ont été modifiés, pas un seul. L'écart est **mesuré** et
détaillé en §2 : la consigne « `interp.rs` uniquement » est impossible à tenir
si les primitives doivent être appelables depuis Rose.

---

## 2. Écart de périmètre, preuve à l'appui

| Fichier | Pourquoi il a dû être touché | Preuve |
|---|---|---|
| `rose/src/interp.rs` | Périmètre principal : listes blanche, dispatch, rendu, tests. | — |
| `rose/src/front.rs` | Le vérificateur rejette tout appel à une fonction absente de `STDLIB`. Mesuré sur le binaire **avant** modification : `rose run` d'un fichier contenant `>> bidon(3)` → `Fonction 'bidon' inconnue`, rc=1. `front.rs:2130` (`nom_fonction_connue`) et `front.rs:2193` (contrôle strict de l'arité) l'imposent. | Ajout de 14 entrées `EntreeStd` + une exception d'arité pour `plus_de_chiffres`. |
| `rose/src/repl.rs` | CLI1 avait placé le fournisseur de `:more` dans un `thread_local!`. Or `repl::executer` (`repl.rs:424`) exécute chaque ligne dans un fil de 16 Mio : l'enregistrement fait par l'interpéteur mourait avec le fil. Mesuré : `>> entier_geant_factorielle(100)` puis `:more 30` → « nécessite d'abord detail(expr) ». Après correction : 30 → 60 → 90 → 158 chiffres. | `FOURNISSEUR_PLUS` et `CHIFFRES_DEMANDES` sont devenus des états globaux de processus (`Mutex` + `AtomicUsize`) ; l'adaptateur porte un `Arc<Mutex<ObjetParesseux>>` pour être `Send`. |

Aucune modification de `run.rs`, `i128x.rs`, `gui_native.rs`, `jit.rs`, ni des
jardins. Le garde `a_des_fonctions` supprimé le 20/11 n'a **pas** été réintroduit
(`grep -rn a_des_fonctions rose/src/` → aucun résultat). Le chemin JIT existant
n'a pas été modifié ; les entrées `STDLIB` ajoutées ont un type de retour non
JIT-émettable, donc `jit.rs:1062` les renvoie à l'interpréteur.

---

## 3. Primitives ajoutées

Toutes sont dans `nom_std_connu()` (interp.rs) **et** `stdlib_valeur()`
(interp.rs) **et** `front::STDLIB` (front.rs) :

| Nom Rose | Appel | Résultat |
|---|---|---|
| `entier_geant_factorielle(n)` | `vers_z(&a[0])` puis `Valeur::symbolique_factorielle_gz(&z)` | `Valeur::Symbolique`. Accepte `n > u64::MAX` : `10^37` passe, un `n` négatif donne une erreur explicite. Le `Gz` est éphémère, sa **valeur** est copiée (jamais son pointeur). |
| `detail(expr)` | `proprietes_detail(&a[0])` | `Valeur::Table` dont les clés sont les libellés et les valeurs des chaînes calculées. |
| `afficher_detail(table)` | `repl::format_table_detail(t)` puis `run::ecrire_out` | Rend le bloc multi-lignes avec le gabarit exact de CLI1 : deux espaces, libellé paddé à 28, ` : `. `>> detail(x)` rend la table compacte Rose ; `afficher_detail(detail(x))` rend le bloc lisible. |
| `chiffres(expr, n)` | `chiffres_int1` | Chaîne des `n` premiers chiffres. Symbolique : la demande est ajustée via `demander_plus` puis le texte est tronqué. Autres types : `chaine_de`, filtrage des chiffres (pas de signe, pas de séparateur, pas d'ellipse), sans padding. `n < 0` → erreur ; `n = 0` → chaîne vide. |
| `plus_de_chiffres(expr, n = 30)` | `plus_de_chiffres_int1` | Appelle `demander_plus(n)`, enregistre l'objet dans le REPL pour `:more`, rend la chaîne. Une ou deux arités sont acceptées. Défini **uniquement** pour un symbole ; sur un entier ordinaire : `plus_de_chiffres : attend un entier geant symbolique (entier_geant_factorielle)`. |

### Enregistrement du dernier objet

`enregistrer_symbolique_affiche(&Valeur)` est appelé aux cinq points de sortie
de l'interpéteur : `Instr::ExprStmt`, `afficher_valeur`, `afficher_expression`,
`afficher_abstrait`, et la primitive `afficher`. Il ne touche qu'aux
`Valeur::Symbolique`.

---

## 4. `detail()` : propriétés par type

Le dispatch est un `match` **exhaustif sur la variante** de `Valeur`, pas sur une
chaîne : aucune propriété ne peut déborder d'un type sur un autre.

| Type | Propriétés rendues (toutes calculées) |
|---|---|
| `Symbolique` | `Puissance de 10` = `10^` + `log10_complet` ; `Premiers chiffres` = `premiers_tronques()` ; `Longueur` = `longueur` + ` decimal digits` ; `Zeros finaux` = Legendre ; `Derniers non nuls` = k=9 (API `proprietes()`). |
| `Entier` | `Type` (`entier premier` seulement si `est_premier` est vrai, sinon `entier` — aucun mensonge de non-primalité) ; `Representation` ; `Longueur` ; `Parite`. |
| `Rationnel` | `Type` ; `Representation` (décimale périodique via `texte_rationnel`) ; `Numerateur` ; `Denominateur` ; `Periode` (longueur réelle entre crochets, 0 si aucune). |
| `Flottant` | `Type` ; `Classe` (fini / NaN / infini) ; `Representation` ; `Exposant binaire` (`to_bits`, ou « non applicable » pour 0/NaN/infini). |
| `Booleen` | `Type` ; `Valeur`. |
| `Chaine` | `Type` ; `Representation` ; `Longueur` en caractères ; `Apercu` si plus de 80 caractères. |
| `Liste` | `Type` ; `Representation` ; `Elements` ; `Premier element` ; `Dernier element` (si non vide). |
| `Table` | `Type` ; `Representation` ; `Entrees` ; `Premiere cle` (si non vide). |
| `Tampon`, `Tableau`, `Complexe`, `Irrationnel`, `Vide` | 2 à 4 propriétés du type : octets / cases ; parties réelle, imaginaire, module et type ; représentation et valeur sous-jacente ; type. |

Oracles réels pour `detail(entier_geant_factorielle(10^37))` (sortie copiée) :

```
  Puissance de 10              : 10^365657055180967481723488710810833949195.9551…
  Premiers chiffres            : 901820028144300502340101362711…
  Longueur                     : 365657055180967481723488710810833949196 decimal digits
  Zeros finaux                 : 2499999999999999999999999999999999990
  Derniers non nuls            : 467172352
```

`467172352` est bien la fin des 13 chiffres annoncés à l'écran
(`…6970467172352`). Aucun appel à `factorielle_parallele` ni à `to_string()`
d'un BigInt géant n'est fait sur ce chemin.

---

## 5. Pont GUI1

Neuf noms Rose ont été câblés vers l'API publique déjà présente dans
`gui_native.rs` :

| Nom Rose | Appel Rust |
|---|---|
| `fenetre_compter()` | `compter_fenetres()` |
| `fenetre_creer_simulee(titre, l, h)` | `creer_simulee()` |
| `fenetre_boutons_creer(id, geometrie)` | conversion de `B|id|libellé|x|y|w|h` puis `boutons_creer()` |
| `bouton_compter(id)` | `compter_boutons()` |
| `bouton_sous(geometrie, x, y)` | `bouton_sous()` (hit-test géométrique) |
| `bouton_attendre_clic(id)` | `attendre_clic()` |
| `bouton_attendre_clic_limite(id, ms)` | `attendre_clic_limite()` |
| `bouton_clic_simuler(id, bouton)` | `clic_simuler()` |
| `bouton_x11_envoyer_clic(id, bouton)` | `x11_envoyer_clic()` (UNIX uniquement) |

`boutons_depuis_geometrie()` convertit le format produit par
`geometrie_boutons($f)` du jardin ; les champs invalides sont ignorés, jamais
inventés. Aucun libellé métier n'est écrit en Rust.

**Fonctionnel, mesuré** :
- sans `DISPLAY`, `fenetre_creer_simulee` + `fenetre_boutons_creer` +
  `bouton_clic_simuler` + `bouton_attendre_clic_limite` rendent `1`, `1`, `1` :
  un vrai bouton enregistré, un vrai clic reçu, et
  `decrire_grand_nombre($f)` montre `CLICS|30` et la chaîne cumulée mise à jour ;
- sous `xvfb-run -a`, `fenetre_native` + `fenetre_boutons_creer` +
  `fenetre_afficher` + `bouton_x11_envoyer_clic($id, 1)` +
  `bouton_attendre_clic_limite($id, 3000)` rendent `1` puis `1` : aller-retour
  réel par le serveur X (`XSendEvent` → `XNextEvent`), rc=0.

**Non vérifié** : la branche Win32 (`WM_COMMAND`) n'a jamais été compilée ni
exécutée sur cette machine Linux. Le nom Rose `bouton_x11_envoyer_clic` n'existe
pas sous Windows (le Rust correspondant est `#[cfg(not(windows))]`).

---

## 6. Validation réelle

Binaire : `/tmp/opencode/int1_agents/target/release/rose`.
`PATH="$HOME/.cargo/bin:$PATH" CARGO_TARGET_DIR=/tmp/opencode/int1_agents/target`.

| Contrôle | Résultat mesuré |
|---|---|
| `cargo build --release` | `Finished \`release\` profile … in 52.63s`, 0 erreur. |
| `cargo test --release` | rc=0 ; **0 échec** partout : lib 250/0/3 ignorés, bin `rose` 234/0/3, `grands_nombres` 8/0, `rose-shell` 6/0, `mul_cmp` 1/0. Les 5 nouveaux tests `interp::tests_int1` passent dans la lib **et** dans le binaire. |
| 5 tests INT1 | oracles `10^37` (5 propriétés + gabarit CLI1), non-chevauchement par type, stabilité 30→60 chiffres, vérification Rose des noms/arités, conversion de `geometrie_boutons`. |
| Tests QAA concurrents déjà dans l'arbre (`rose/tests/grands_nombres.rs`, non écrits par INT1) | 8/8 OK, dont `repl_more_ajoute_30_chiffres_et_reste_stable` et `gui_grand_nombre_fenetre_bouton_et_clic_simule`. |
| Jardin 20/20 | `── jardin/premiers : 20 tests, 0 échec ──`. |
| `fibonacci(25)` | `75025`. |
| Point fixe b3ed9262 | `rose rir compilateur/emetteur.rose` → `b3ed9262dd54776a6dbd07bc12368d0e` ; `rose compilateur/emetteur.rose compilateur/emetteur.rose` → le **même** md5 ; stderr séparé, 0 octet dans les deux cas. |
| 9 domaines multi (`/tmp/opencode/speed2_agents/bench_multi.py` adapté, base = `rose_base`) | `pin=c6: 81/81 OK`, `pin=c16: 81/81 OK`, sorties octet pour octet identiques à la base. |
| `bench_global.rose` depuis un dossier temporaire | rc=0, 12 lignes, 11 valeurs **identiques** à `bench/global/full.txt`. |
| `rose verif jardins/gui_native.rose` | `vérifié`. |
| Console interactive | `>> entier_geant_factorielle(100)` puis `:more 30`, `:more`, `:more 200` → 30, 60, 90 puis 158 chiffres (préfixe stable) ; `:more` sans objet → message clair, pas de panique. |

---

## 7. Reste

1. Le rendu multi-lignes n'est PAS automatique sur `>> detail(x)` : cette forme
   affiche la table compacte Rose (`[clé => valeur, …]`). Il faut appeler
   `afficher_detail(detail(x))` pour le bloc aligné de `DETAIL_UX.md` §2.2.
   Le modifier impliquerait un marquage des tables de détail dans
   `run::afficher`/`chaine_de_p`, hors périmètre.
2. `Derniers non nuls` est limité à k=9 par l'API `proprietes()` de VAL1
   (au-delà, MATH1 retombe sur un produit O(n), intraitable pour `10^37`).
3. `chiffres(expr, n)` exige 2 arguments enRose (le vérificateur ne connaît pas
   de valeur par défaut) ; seul `plus_de_chiffres` accepte 1 ou 2 arguments.
4. Win32 non compilé ici ; bouton X11 prouvé seulement sous Xvfb.
5. La copie de l'`ObjetParesseux` dans un `Arc<Mutex<_>>` pour `:more` signifie
   que les appels ultérieurs à `plus_de_chiffres` sur l'objet Rose d'origine et
   les `:more` successifs peuvent diverger si le programme mêle les deux.
   Comportement observé : dans la console, une nouvelle ligne oublie le
   fournisseur, donc ce cas ne se produit pas lors d'une utilisation normale.

*Toute mesure ci-dessus est un constat daté : re-mesurer après tout changement
de `interp.rs`, `front.rs`, `repl.rs`, `run.rs` ou `gui_native.rs`.*
