# AZ_ROADMAP — feuille de route factuelle pour finir le self-hosting A–Z

Mission AZR (vague 69, `PLAN.md` l.1158-1165). Un seul fichier écrit :
`rose/docs/AZ_ROADMAP.md`. **Aucun commit, aucune publication, aucun fichier
source modifié.** Constat daté : **20/11/2026**, dépôt
`/home/clausake/Stefan/ROSE`.

Sources lues : `rose/docs/AZZ.md`, `G5_AZ.md`, `SELFHOST_TOTAL_PLAN.md`,
`C13_FIX.md`, `BOOTSTRAP_AZ.md`, `PLAN.md`, et le rapport de sous-agent
`/tmp/opencode/azz_agents/RAPPORT.md` (az4), plus les sondes concurrentes
`/tmp/opencode/az_probe/`.

**Règle de ce document** : ce qui est « MESURÉ » a été exécuté et porte sa
source ; ce qui est « DOCUMENTÉ » est repris d'un rapport avec sa référence ;
ce qui est « ESTIMATION » est un chiffre de travail non mesuré, étiqueté comme
tel. Aucune spéculation n'est présentée comme un fait.

---

## 1. État de départ — ce qui est prouvé, ce qui ne l'est plus

Le md5 de l'émetteur courant au moment de la rédaction est
`4e6a1923a44e4eb17a727a84d1acf3c7` (HEAD `ff3c4a9`). Il **diffère** des md5
cités dans les rapports (`0c37cbc5…` dans `AZZ.md` §5, `dd2af608…` pour la copie
avec comparaison). Des agents concurrents (AZC, AZT, RCx, TPx) travaillent sur
le même chantier : **tout chiffre ancien est un constat daté, à re-mesurer**,
pas une consigne.

### 1.1 Jalons rejoués à la rédaction (MESURÉ, ce jour)

| Jalon | Commande | Résultat |
|---|---|---|
| `verif` | `./rose/rose verif compilateur/emetteur_elf.rose` | `vérifié`, rc=0 |
| Point fixe | `./rose/rose rir compilateur/emetteur.rose \| md5sum` | `b3ed9262dd54776a6dbd07bc12368d0e`, 263 520 o |
| `t999` | compilation avec l'émetteur courant + exécution avec `critique_entree.txt` | natif = ELF, `cmp`=0, **20/20 lignes**, rc=0 des deux côtés |

Artefacts de ce rejeu : `/tmp/opencode/roadmap_t999.{rir,elf,nat,elfout}`.

### 1.2 Jalons documentés (à rejouer avant livraison)

| Jalon | Dernière preuve | Réserve |
|---|---|---|
| F1 == F2 | `G5_AZ.md` §4 : 464 400 o, md5 `785a2493…`, `cmp`=0 ; `C13_FIX.md` §5.2 : 432 544 o, md5 `6f2715e5…`, `cmp`=0 ; `AZZ.md` §7.2 : `full128.elf` reproduit `F1c.elf` octet pour octet (md5 `67283c07…`) après passage du tas à 128 Gio | `AZZ.md` §7.3 dit « à compléter après régénération de F1 avec le mapping 128 Gio ». Le mapping est bien dans la source courante (`emetteur_elf.rose` l.3253 : `137438953472`), mais la chaîne complète n'a pas été rejouée sur le md5 `4e6a1923…`. |
| Batterie SH9 | `AZZ.md` §4 : **67 conformes / 4 divergents / 7 bloqués** après l'addition bigint (avant : 66/7/7) | À rejouer sur la source courante. |
| Verdict A–Z | `BOOTSTRAP_AZ.md` §5.4 : **A–Z PARTIEL, code 2** ; `h` = 2 MARCHE / 1 BLOQUÉ (le bigint `2^64`) | Le seul blocage de l'échantillon `h` est le bigint. |
| Validation Rust | `G5_AZ.md` §4 : `cargo test --release` 515 passés / 0 échec, 6 ignorés ; jardins 9/10 (`gui_native.rose`, faux positif commentaire « Rust ») | Documenté SH12, non rejoué ici. |

**Conséquence :** le self-hosting A–Z n'est pas « atteint ». Il est « partiel »
avec trois jalons verts sur la source courante (`verif`, point fixe, `t999`) et
une propriété F1==F2 prouvée sur des états antérieurs, pas encore sur l'état
courant.

---

## 2. Les cinq jalons de garde — à rejouer après CHAQUE blocage

Aucun correctif n'est accepté si l'un de ces jalons casse (`BOOTSTRAP_AZ.md` §2,
`G5_AZ.md` §5.3).

1. **`verif`** — `./rose/rose verif compilateur/emetteur_elf.rose` → `vérifié`, rc=0.
   Le `rose` du dépôt doit être utilisé ; `~/.local/bin/rose` est une 2.0 sans
   sous-commande `verif` (`C13_FIX.md` §8.3).
2. **Point fixe `b3ed9262`** —
   `./rose/rose rir compilateur/emetteur.rose > fix.rir` (263 520 o, md5
   `b3ed9262…`), puis `./rose/rose compilateur/emetteur_elf.rose fix.rir >
   fix.elf`, `./fix.elf compilateur/emetteur.rose > dump.rir`, `cmp dump.rir
   fix.rir` → 0. `C13_FIX.md` §6 l'a mesuré **×2**.
3. **`t999` 20/20** — voir §1.1, avec
   `/tmp/opencode/sh9_agents/battery/critique_entree.txt` comme argument.
   Sans argument, le runner simplifié le classe « bloqué » ; ce n'est pas une
   régression (`AZZ.md` §4).
4. **F1 == F2** — chaîne :
   ```
   ./rose/rose rir compilateur/emetteur_elf.rose > e.rir
   ./rose/rose compilateur/emetteur_elf.rose e.rir > F1.elf
   ./F1.elf e.rir > F2.elf
   cmp F1.elf F2.elf
   ```
   Porte : `cmp` muet, rc=0 des deux côtés, **sans patch manuel de l'en-tête**.
   Coût mesuré : ~15 min par émission (`AZZ.md` §7.1), plus ~15 min pour la
   régénération ; ~80 Gio de pointeur final sous le mapping RW 128 Gio
   (`AZZ.md` §7.2). Le patch manuel de `p_memsz` n'est plus acceptable en
   livraison : il a servi au diagnostic uniquement.
5. **Batterie et non-régression** — batterie SH9 (80 programmes) ≥ 67
   conformes, zéro nouvelle divergence, `t999` inclus ; `fib25` = 75025/75025.

Ordre recommandé global : **B0 → B1 → B2 → B3 → B4 → B5**, en rejouant les
cinq jalons à chaque étape. B0 est le seul correctif déjà écrit et validé en
copie ; c'est donc le premier à intégrer.

---

## 3. Tableau récapitulatif des blocages

| Ordre | Blocage | Nature du changement | Effort (ESTIMATION sauf B0) | Critère de succès |
|---|---|---|---|---|
| B0 | Comparaison des grands entiers (`< > == != <= >=`) | **~180 lignes**, extension du tag 3 existant | 180 lignes **mesurées** dans un patch livré en copie ; 2×15 min de F1/F2 à rejouer | 6/6 opérateurs + 10/10 cas mixtes natif=ELF ; jalons verts |
| B1 | `-`, unaire `-`, `*`, `/`, `%`, `abs`, `entier()` à l'exécution sur tag 3 | Extension du tag 3 (même ABI, nouvelles routines) | 100–200 lignes pour `-`/`-a`/`abs` ; plusieurs centaines pour `* / %` et le parseur décimal | Sondes `p2/p3/p6` natif=ELF octet pour octet |
| B2 | Rationnel calculé (`t08`, `22/7`) | **Changement d'ABI complète** (nouveau tag numérateur/dénominateur) | Plusieurs milliers de lignes, dizaines de Kio (ordre de grandeur SH5 §6) | `t08` natif `3.[142857]` = ELF ; `t07/t23/t24` restent conformes |
| B3 | Unicode des sources arbitraires (`t04/t41/t44`) | **Changement d'ABI complète** des chaînes | Diagnostic de la régression F1==F2 d'abord ; puis plusieurs jours (ESTIMATION) | t04/t41/t44 conformes **et** F1==F2, point fixe, t999 |
| B4 | `type()` sur booléen variable | Changement d'ABI partiel (tag 0 partagé entier/booléen) | À chiffrer après décision d'ABI ; pas « quelques lignes » | `t14` natif=ELF, y compris variables |
| B5 | `fact`, surcharges, HOF, listes de listes, `argument` hors limites | Front-end / émetteur, pas l'ABI des valeurs | Faible à moyen (ESTIMATION), à chiffrer par cas | 7 « bloqués » pris en charge, batterie ≥ 67 sans régression |

---

## 4. Détail des blocages

### B0 — Comparaison des grands entiers (premier à intégrer)

- **Cause technique (DOCUMENTÉ).** Les grands entiers sont des descripteurs
  tag 3. Le chemin de comparaison compare les **pointeurs** : l'ordre dépend de
  l'allocation, pas de la valeur. `AZZ.md` §6 confirme qu'aucune routine de
  comparaison multi-limbes n'a été livrée (seule l'addition signée l'a été).
- **Preuve (DOCUMENTÉ, rapport az4 §2).** `cmp_probe2.rose` avec
  `$g = 100000000000000000000` et `$p = 18446744073709551616` : natif
  `faux / vrai`, ELF `vrai / faux`. Confirmé par une sonde concurrente du
  20/11 22:54 : `/tmp/opencode/az_probe/p.rose` donne natif `vrai / vrai / vrai`
  et ELF `faux / faux / faux` sur les trois comparaisons `>`, `==`, `>=` de
  cette sonde (artefact concurrent, non intégré, à rejouer).
- **Correctif existant (DOCUMENTÉ, rapport az4 §3).**
  `/tmp/opencode/azz_agents/emetteur_cmp.rose` (md5 `dd2af608…`) ajoute
  `g5_emit_bigneg` (5 labels), `g5_emit_bigcmp` (11 labels : signe puis
  magnitudes du limb de poids fort vers le faible, rend `-8/0/8` tagué) et un
  dispatch à déplacement constant (aucun label par expression `bin`, pour ne
  pas ralentir `g5_patch` sur les 2051 `(bin …)` du RIR auto-hébergé).
  Diff : `/tmp/opencode/azz_agents/comparaison.patch`, **180 lignes**.
  Deux pièges déjà corrigés dans ce patch : `g5_alloc_inline()` détruit x2
  (sauvegarde dans x10), et la boucle doit partir du limb de poids fort.
- **Effort (MESURÉ pour le code).** 180 lignes de patch ; le coût réel restant
  est la validation : ~2×15 min de F1/F2 + batterie.
- **Ordre : 1.** C'est le correctif le moins cher et le seul déjà prouvé en
  copie. Il doit être intégré par l'agent AZC (propriétaire du fichier), pas
  dupliqué ici.
- **Critère de succès.** 6/6 opérateurs sur deux grands positifs, 10/10 cas
  mixtes (`cmp_mix.rose` : positif/petit, négatif/zéro, chaînes, petits
  entiers), `t33/t34/t999` inchangés, puis les **5 jalons de garde** verts.
  Le rapport az4 a mesuré ces tests sur la **copie**, mais **pas** F1==F2 ni la
  batterie complète (§4 du rapport) : ces deux preuves restent à produire.

### B1 — Autres opérations sur grands entiers variables

- **Cause technique (DOCUMENTÉ).** `AZZ.md` §6 : la promotion, l'addition
  signée, le chargement des littéraux et l'impression sont les seules routines
  bigint émises. Comparaison, soustraction, multiplication, division, modulo,
  puissance et `entier()` à l'exécution restent sur l'ancien chemin 64 bits.
  `g5_emit_entier` accumule en 64 bits puis `<<3`.
- **Preuve (DOCUMENTÉ + sonde concurrente).** `AZZ.md` §3 prouve l'addition
  correcte (8 lignes de `t_var.rose` identiques). En revanche les sondes
  `/tmp/opencode/az_probe/p2.rose`, `p3.rose`, `p6.rose` (20/11 22:54, non
  intégrées) montrent natif ≠ ELF : `$a - $b` → `-` au lieu de
  `18446744073709551615` ; `$a * $b`, `$a / $b`, `abs($a)` → `201330688` ;
  unaire `-$a` → `-201330689` ; `$c / $d` avec `$c = -7` → `-2` au lieu de
  `-3.[3]` ; `$a + -$b` → `-201330689` au lieu de `-9223372036854775807`.
- **Nature.** Ce n'est **pas** un nouveau changement d'ABI : le tag 3 et son
  descripteur existent. C'est l'ajout de routines dans la même famille que
  `g5_emit_bigadd` (même base 10^9, mêmes scratchs 0x60005000/0x60006000).
- **Effort (ESTIMATION).**Soustraction et négation : ~100–200 lignes miroir de
  `bigadd` (emprunt, élagage des limbs nuls) ; `abs` : ~20 lignes ;
  multiplication, division, modulo : plusieurs centaines de lignes ;
  `entier()` multi-limbs à l'exécution : ~100–200 lignes de parseur décimal.
  Attention à la limite de scratch documentée (`AZZ.md` §6) : les tests vont
  jusqu'à 10 limbs ; au-delà, une zone voisine serait écrasée.
- **Ordre : 2**, après B0. Soustraction/negation d'abord (elles servent aux
  négatifs), puis `* / %`, puis `entier()`.
- **Critère de succès.** `p2/p3/p6` (ou leurs équivalents intégrés à la
  batterie) rendent natif = ELF octet pour octet, `t32/t33/t34/t35` restent
  conformes, puis les 5 jalons de garde.

### B2 — Rationnel calculé à l'exécution (`t08`, `22/7`)

- **Cause technique (DOCUMENTÉ).** `G5_AZ.md` §5.2 : les littéraux rationnels
  sont corrects parce qu'ils sont repliés à l'émission ; un **résultat** de
  division a besoin d'un nouveau tag « rationnel » avec numérateur et
  dénominateur. Changement d'ABI, non tenté par SH12.
- **Preuve (DOCUMENTÉ).** `BOOTSTRAP_AZ.md` §5.6 : `t08` natif
  `3.[142857]/1/`, ELF `3/1/`. `AZZ.md` §4 le compte parmi les 4 divergences
  inchangées après le chantier bigint. `G5_AZ.md` §3 a corrigé les rationnels
  **négatifs** et le modulo plancher (`t07/t23/t24` conformes), pas ce cas.
- **Nature.** **Changement d'ABI complète** : nouveau tag (4), descripteur
  num/den (eux-mêmes potentiellement multi-limbes si le numérateur dépasse
  2^59), formateur `.[…]`, réduction, et dispatch arithmétique. Le tag 3 ne
  suffit pas : il représente des entiers, pas des fractions.
- **Effort (ESTIMATION).** Plusieurs milliers de lignes, dizaines de Kio de
  code machine, sur le modèle du coût BigInt complet chiffré par
  `SELFHOST_TOTAL_PLAN.md` §6.
- **Ordre : 3.** Après B0/B1, pour ne pas multiplier les tags à la fois.
- **Critère de succès.** `t08` natif = ELF (`3.[142857]`) ; `t06` (littéral),
  `t07`, `t23`, `t24` restent conformes ; batterie ≥ 67 ; les 5 jalons verts.

### B3 — Unicode des sources arbitraires (`t04/t41/t44`)

- **Cause technique (DOCUMENTÉ, deux couches distinctes).**
  1. *Idempotence de l'émetteur : **résolue**.* `C13_FIX.md` §3 : `rose rir`
     lit les points de code Unicode, l'ELF G5 lit des octets (`lire_texte` =
     openat/read). `g5_make_str` produisait donc 2 octets `c3 a9` pour `é`
     contre 1 point de code 233 en natif, décalant le code de +24 octets dans
     F2. `g5_cps` (`C13_FIX.md` §4) recombine les suites UTF-8 valides ; F1==F2
     est alors devenu vrai (432 544 o, md5 `6f2715e5…`).
  2. *Sources arbitraires : **non résolue**.* `G5_AZ.md` §5.3 : la tentative
     complète (descripteurs UTF-8, `len` en octets, `longueur()` recompte les
     points de code, `sous_chaine`/`idx`/`ordre` décodent, `caractere` replié
     à l'émission) a corrigé `t04/t41/t44` (69/80 conformes, `t999` 20/20,
     point fixe ×2 OK) **mais a cassé F1==F2** : F1 émis sortait par
     `exit(2)` immédiat sur sa propre RIR (`e_fix.rir`, 340 378 o), alors
     qu'il émettait correctement une petite RIR. Cause non identifiée dans le
     temps imparti (chemin `g5_err` instrumenté : non atteint). Le bloc a été
     **annulé** ; l'émetteur livré est l'état validé.
- **Preuve complémentaire (DOCUMENTÉ).** `BOOTSTRAP_AZ.md` §5.6 : `t04` natif
  `été radieux/11/🚀/`, ELF octets `e9 74 e9 20 … 80`. Sonde concurrente
  `/tmp/opencode/az_probe/p5.rose` : natif `é🚀中 / 3 / é / …`, ELF caractères
  de remplacement et 233 au lieu de `é` (à rejouer).
- **Nature.** **Changement d'ABI complète** des chaînes. Ce n'est pas
  l'absence de `g5_cps` : ce correctif est déjà intégré (`emetteur_elf.rose`
  l.553, `g5_cps`). Le risque mesuré est la non-idempotence du compilateur
  généré, pas le simple affichage.
- **Effort (ESTIMATION).** D'abord un diagnostic ciblé de la régression
  `exit(2)` sur la grande RIR (quelques heures), puis la réimplémentation de
  la couche UTF-8 et la validation F1/F2 (plusieurs jours). Le coût d'une
  seule salve F1/F2 est de ~30 min machine (`AZZ.md` §7.1).
- **Ordre : 4**, après B2. Ne pas réintroduire la couche Unicode avant d'avoir
  un test F1==F2 automatisé qui échoue **avant** tout autre changement.
- **Critère de succès.** `t04/t41/t44` conformes, `t999` 20/20, point fixe
  `b3ed9262` identique, **F1==F2 sur la grande RIR**, batterie ≥ 67.
  Le critère « 69/80 conformes » de la tentative annulée **ne suffit pas**.

### B4 — `type()` sur booléen variable

- **Cause technique (DOCUMENTÉ).** `G5_AZ.md` §2 et §5 : booléen et entier
  partagent le tag 0. `type` distingue correctement les littéraux et les slots
  connus booléens à l'émission (replis), mais retombe sur `entier` pour une
  variable non connue.
- **Preuve (DOCUMENTÉ).** `BOOTSTRAP_AZ.md` §5.6 : `t14` natif
  `entier/chaine/liste/booleen/`, ELF `0/0/0/0/` sur la version antérieure à
  SH12 ; `G5_AZ.md` §2 rapporte la correction des cas littéraux avec la limite
  ABI ci-dessus.
- **Nature.** Changement d'ABI **partiel** : il faut un tag distinct pour le
  booléen (ou une table de slots typée conservée à l'exécution). Les cas
  « connus à l'émission » sont déjà faits en quelques lignes ; le cas variable
  ne l'est pas.
- **Effort (ESTIMATION).** À chiffrer après choix de représentation. Ne pas le
  compter comme « quelques lignes » : toucher au tag 0 touche tous les
  opérateurs.
- **Ordre : 5.** Le repousser derrière les blocages qui empêchent déjà `h`
  d'être PASS.
- **Critère de succès.** `t14` natif = ELF avec une variable booléenne, sans
  régression de `t999`, du point fixe ni de F1==F2.

### B5 — Blocages de langage / d'émission (7 programmes « BLOQUÉS »)

- **Cause technique (DOCUMENTÉ).** `G5_AZ.md` §6 : surcharges (`t104/t105`),
  HOF (`t106/t107`), factorielle (`t35`), listes de listes (`t55`), argument
  hors limites (`t81/t82`) sont des erreurs de syntaxe/implémentation
  **naturelles**, classées bloquées des deux côtés — pas des régressions G5.
  Un cas est toutefois spécifiquement G5 : `t35` échoue à l'émission avec
  `G5 : forme parenthésée non couverte : fact` (`AZZ.md` §4).
- **Preuve (MESURÉ sur artefacts).** `/tmp/opencode/bigint/battery_new/`
  contient les messages natifs : `Fonction 'f' définie…` (t104/t105),
  `Fonction 'fn' inconnu` (t106), `Variable 'carre' utilisée…` (t107),
  `Listes de listes non…` (t55), `index hors des limites des arguments`
  (t81/t82). `t999` est lui aussi classé bloqué sans argument mais conforme
  20/20 avec l'argument fourni (`AZZ.md` §4-5).
- **Nature.** Front-end et émetteur Rose, **pas l'ABI des valeurs**. Le coût
  dépend de chaque cas ; `fact` est une forme à ajouter à `g5_eval`, les
  surcharges/HOF sont des changements de sémantique du langage.
- **Effort (ESTIMATION).** Faible pour `fact` (une forme parenthésée) ;
  moyen à élevé pour HOF/surcharges (à chiffrer dans un chantier séparé).
- **Ordre : 6**, en dernier : ces programmes ne bloquent pas l'échantillon
  `h` de SH13.
- **Critère de succès.** Chaque programme ajouté à la batterie passe de
  « bloqué des deux côtés » à « natif = ELF » ; le compte de conformes monte
  sans qu'aucun conforme ne régresse ; les 5 jalons restent verts.

---

## 5. « iota / tableaux » — périmètre voisin, à ne pas confondre avec A–Z

Les mots « iota » et « tableaux » apparaissent dans les rapports, mais **ils ne
désignent pas un blocage G5 mesuré du self-hosting A–Z** :

- **Tables G5 : livrées, pas bloquées.** `G5_AZ.md` §1 mesure le passage de
  `vrai / 0 / 2 / vrai / 2` (ELF avant) à `vrai / 10 / 2 / faux / 1` (ELF
  après), `cmp`=0 avec le natif ; clés **et valeurs**, croissance corrigée.
  `BOOTSTRAP_AZ.md` §5.4 (`h` : tables MARCHE) et §5.5 (C0→RIR tables `cmp`=0)
  le confirment. Le seul résidu de table signalé est le `type()` de B4 et la
  garde de clé mixte déjà corrigée (`G5_AZ.md` §1.4).
- **Listes de listes : bloquées des deux côtés** (`t55`, voir B5), pas une
  divergence G5.
- **iota : chantier Thorn, hors du dépôt self-host G5.** `thorn/src/noyau.rose`
  l.572 définit `noy_iota(t: table)` avec les réductions `plus`/`nat_rec` ;
  `thorn/docs/QUALITE.md` §4.4 rapporte la suite `tests_noyau` passée de 35 à
  87 puis 88 cas au fil des ajouts `iota`/`nat_rec` d'un chantier concurrent ;
  `git log` cite `93860d7` (ι-réduction, univers, Eq typé). **Aucun effort ni
  critère G5 n'est à en déduire.** Si le front Thorn (TP1) est visé, sa porte
  est propre : `tests_noyau` ≥ 88 cas et les 17 contrôles de CI de
  `QUALITE.md` au vert — mais ce n'est **pas** une condition du verdict
  `A–Z ATTEINT` défini par `BOOTSTRAP_AZ.md` §2.

---

## 6. Ce qui ferait passer A–Z de PARTIEL à ATTEINT

Condition minimale mesurable, d'après `BOOTSTRAP_AZ.md` §6 : corriger le
bigint `2^64` **dans l'ABI G5 et répercuter dans C0 pour le RIR**, puis rejouer
`scripts/selfhost_total.sh --full --with-c13`. Le verdict doit afficher
`3 MARCHE / 0 BLOQUÉ` pour `h` et `A–Z ATTEINT (code=0)`.

Dans l'état courant, le correctif B0 (comparaison) ne suffira pas à lui seul :
la sonde `p.rose` montre que la comparaison de grands entiers est fausse, et
`AZZ.md` §6 documente que le reste des opérations bigint est toujours sur le
chemin 64 bits. La séquence réaliste est donc : **B0 → B1 → rejeu du pipeline
A–Z**, puis seulement B2/B3/B4/B5 pour élargir la couverture au-delà de
l'échantillon SH13. Chaque étape est acceptée uniquement si les cinq jalons de
garde du §2 sont verts.

Aucune nouvelle mesure n'a été lancée sur F1/F2 complète pendant la rédaction
de ce document (coût ~30 min machine et CPU partagé avec les agents
concurrents) : le chiffre F1==F2 cité plus haut reste un **constat daté**, pas
une garantie sur le md5 `4e6a1923…`.
