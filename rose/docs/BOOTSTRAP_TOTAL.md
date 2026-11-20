# BOOTSTRAP_TOTAL — protocole vérifiable de self-hosting total

**Constat daté, pas une promesse.** Dépôt `/home/clausake/Stefan/ROSE`,
`HEAD = b8875b6da3e2632a16e91b64aa7acf8ed3c860bf` au moment de l'écriture,
20–21/11/2026. Binaire Rose mesuré `rose/rose` md5
`12eac46007cf0fe9e1aec2dff6fbf665`, machine `aarch64`.
Aucun commit, aucune publication, aucun fichier `rose/src/**`, `compilateur/**`,
`Cargo.toml` ou `PLAN.md` modifié par ce chantier.

Script : `scripts/selfhost_total.sh`. Ce document décrit le protocole, pas un
résultat garanti : la sortie réellement obtenue est recopiée en §6.

---

## 1. Ce que les artefacts sont réellement

Il faut distinguer trois programmes, souvent confondus :

| Nom | Nature | Comment il est obtenu | Ce qu'il sait faire |
|---|---|---|---|
| `emetteur.rose` | source Rose | fichier du dépôt | produit le **RIR** d'une source Rose (`rose rir …`) |
| `emetteur_elf.rose` | source Rose | fichier du dépôt | produit un **ELF aarch64** depuis un **RIR** (voie G5) |
| **C0** | ELF aarch64 | `rose emetteur_elf.rose self.rir > C0.elf` | exécute le `$principal` de `emetteur.rose` : **source `.rose` → RIR sur stdout** |

Le md5 `b3ed9262dd54776a6dbd07bc12368d0e` est celui de `self.rir`, le dump
RIR de `compilateur/emetteur.rose` (263 520 octets mesurés). C'est une
**propriété datée de la source**, pas une constante : toute modification de
`emetteur.rose` la change. Un document plus ancien
(`POINT_FIXE_2026_11_19.md`) cite `8ac53eed…` pour une version antérieure de
cette même source ; les deux constats sont contemporains de leur source
respective, aucun n'est à réécrire.

Conséquence importante : **C0 est un émetteur RIR, pas un compilateur
source → ELF.** Il reproduit le point fixe M4.5 ; il ne « compile » pas un
programme arbitraire en binaire exécutable. C'est précisément ce que l'étape f
vérifie, et c'est le seuil du *self-hosting total*.

---

## 2. Protocole — commandes et critères

Toutes les commandes sont jouées depuis la racine du dépôt, avec `ROSE` =
le binaire fourni en argument au script. Chaque étape capture `stdout` et
`stderr` **séparément** (voir piège §5.3).

| Étape | Commande | Critère de réussite |
|---|---|---|
| **a** | `ROSE verif compilateur/emetteur_elf.rose` | `rc=0`, sortie `vérifié` |
| **b** | `ROSE rir compilateur/emetteur.rose > self.rir` | `rc=0`, `md5 = b3ed9262…`, 263 520 o |
| **c** | `timeout 900 ROSE compilateur/emetteur_elf.rose self.rir > C0.elf` | `rc=0`, magic `\x7fELF`, `e_machine=183` (AArch64), taille ≥ 100 000 o |
| **d** | `./C0.elf compilateur/emetteur.rose > dump.out` | `rc=0`, `cmp dump.out self.rir` (octet pour octet) |
| **e** | `ROSE rir compilateur/emetteur_elf.rose > emetteur_elf.rir` puis `ROSE emetteur_elf.rose emetteur_elf.rir > P0.elf` puis `./P0.elf emetteur_elf.rir > P1.elf` | `sha256(P0) = sha256(P1)`, les deux ELF aarch64 |
| **f** | `ROSE fib25.rose` (référence) puis `./C0.elf fib25.rose > fib.C0.out` | référence = `75025` ; sortie C0 = ELF exécutable rendant `75025` |

Programme de l'étape f (`fib25.rose`) :

```rose
fn fib(n: entier) -> entier {
  si (n < 2) { retour n }
  retour fib(n - 1) + fib(n - 2)
}
>> fib(25)
```

### 2.1 Ce que l'étape e prouve — et sa limite

`P0` est émis par le **binaire natif** (la graine), `P1` est émis par `P0`.
L'égalité `sha256(P0) == sha256(P1)` prouve que le compilateur émis est un
**point fixe** : ré-appliqué à sa propre description (le RIR de
`emetteur_elf.rose`), il se reproduit à l'identique.

Ce n'est **pas** encore « C0 émet C1 » : C0 ne sait pas émettre d'ELF. Le
binaire natif sert de graine exactement comme un bootstrap compilateur doit
avoir une graine externe. Le script l'écrit explicitement dans le détail de
l'étape e et dans la note finale si le verdict est `PARTIEL`.

---

## 3. Modes, variables d'environnement, codes de sortie

```bash
scripts/selfhost_total.sh /chemin/vers/rose --full
scripts/selfhost_total.sh /chemin/vers/rose --quick
```

- `--full` (défaut) : a→f, émissions comprises.
- `--quick` : a, b et la référence native de f ; c/d/e sont `IGNORÉ`, f est
  `BLOQUÉ` faute de C0. Ne prouve rien sur le self-hosting.

Variables (toutes optionnelles) :

| Variable | Défaut | Rôle |
|---|---|---|
| `SELFHOST_ROSE_BIN` | 1er argument, sinon `rose/rose` | binaire Rose à auditer |
| `SELFHOST_ROSE_ROOT` | parent du script | racine du dépôt |
| `SELFHOST_EMETTEUR` | `compilateur/emetteur.rose` | source émetteur RIR |
| `SELFHOST_EMETTEUR_ELF` | `compilateur/emetteur_elf.rose` | source émetteur ELF |
| `SELFHOST_WORKDIR` | `/tmp/opencode/selfhost_total.$$` | artefacts + `RAPPORT.md` |
| `SELFHOST_RIR_MD5` | `b3ed9262…` | md5 attendu de `self.rir` |
| `SELFHOST_EMIT_TIMEOUT` | `900` | timeout d'émission, **obligatoire** |
| `SELFHOST_C0_MIN_SIZE` | `100000` | taille minimale de C0 |
| `SELFHOST_C0_PATH` | vide | réutiliser un C0 déjà émis (rejeu ; l'émission n'est pas rejouée, mais le fichier est quand même vérifié en c puis exécuté en d/f) |

Verdicts :

| Verdict | Condition | Code |
|---|---|---|
| `TOTAL` | a, b, c, d, e, f tous `PASS` | 0 |
| `PARTIEL` | a→e `PASS`, f `BLOQUÉ` | 2 |
| `BLOQUÉ` | au moins une étape a→e `BLOQUÉ` (ou invocation invalide) | 3 |
| `ERREUR` | binaire ou source introuvable | 1 |

Un code non zéro est **normal** aujourd'hui : le script ne jamais
`echo success` faux.

---

## 4. Ce que prouve chaque étape

- **a** — la source de l'émetteur ELF est syntaxiquement et typiquement valide
  pour le vérificateur Rose.
- **b** — le dump RIR de référence est reproductible et identique à la valeur
  attendue `b3ed9262`. Sans ça, aucune comparaison ultérieure n'a de sens.
- **c** — l'émetteur Rose produit bien un ELF aarch64 autonome (pas un stub
  texte), dans le délai imparti.
- **d** — **point fixe M4.5** : C0 exécute réellement le `$principal` de
  `emetteur.rose` ; sa sortie est le RIR de départ, octet pour octet. C'est la
  preuve de fidélité de l'émission, pas encore de la capacité à compiler.
- **e** — **point fixe du compilateur** : l'émetteur ELF auto-émis se
  reproduit à l'identique (avec graine native, cf. §2.1).
- **f** — **seuil TOTAL** : C0 compile une source arbitraire en ELF exécutable
  dont le résultat d'exécution égale le natif. Tant que f est `BLOQUÉ`, le
  self-hosting total n'est pas atteint, quel que soit le résultat de a→e.

---

## 5. Pièges

### 5.1 Timeout d'émission — 900 s obligatoires
L'émission de C0 depuis `self.rir` a été mesurée à **374 s** par le chantier
SH3 (`SELF_HOST_SH3.md`). Un timeout plus court (par exemple 120 s) rend un
`rc=124` et un ELF tronqué à 0 octet : le script le rapporte `BLOQUÉ`, jamais
`PASS`. Ne jamais réduire `SELFHOST_EMIT_TIMEOUT` pour « faire passer » le test.

### 5.2 Entrée source `.rose` vs dump RIR
Les deux émetteurs n'attendent pas la même chose :

- C0 (`$principal` de `emetteur.rose`) lit une **source `.rose`** via
  `lire_texte(argument(0))`. Lui passer `self.rir` donne `rc=139` — c'est
  attendu, pas une régression.
- `emetteur_elf.rose` (voie G5) détecte un **RIR** au préfixe littéral `rir`
  de `argument(0)`. Lui passer une source le fait tomber dans la voie
  historique (ELF x86 par constant-folding), qui n'est pas la voie mesurée ici.

Le script passe toujours le bon type de fichier à chaque outil.

### 5.3 stderr qui pollue les md5
`rose rir … > self.rir 2>&1` mélange d'éventuels messages de diagnostic au
dump : le md5 change alors sans que le RIR ait changé. Le script redirige
toujours `stdout` et `stderr` dans deux fichiers distincts, calcule le md5
sur `stdout` seul, et affiche `stderr` dans le détail en cas d'échec.
Ne jamais comparer un md5 obtenu avec `2>&1`.

### 5.4 Valeurs datées, sources concurrentes
`emetteur_elf.rose` a déjà changé entre deux chantiers (md5 `afe…` →
`e994…` → `e668…` mesuré ici). Le RIR `b3ed9262` dépend de `emetteur.rose`,
pas de l'émetteur ELF. **Re-mesurer le md5 des deux sources avant toute
affirmation** ; les valeurs de ce document sont des constats du 20/11/2026.

### 5.5 Émissions concurrentes
Plusieurs émissions lourdes simultanées (self.rir → ELF, emetteur_elf.rir →
ELF) consomment plusieurs Gio d'espace d'adressage RW chacune. Sur cette
machine, l'émetteur émis réserve 2 Gio de mapping anonyme paresseux. Ne pas
lancer plusieurs `--full` en parallèle sans surveillance mémoire.

### 5.6 Ne pas confondre « ELF produit » et « ELF qui compile »
Un `rc=0` avec un ELF de 534 780 o prouve que l'émetteur a travaillé (c), pas
que l'ELF sait compiler (f). Le test f exige l'exécution effective et la
comparaison du résultat au natif.

---

## 6. Résultat réel de l'exécution

_(complété après le jet réel — voir ci-dessous)_
