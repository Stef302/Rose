# Site `docs/index.html` branché sur le vrai moteur Rose WebAssembly

**Constat daté : 19/11/2026.** HEAD de départ `5665bf7`. Aucun commit, aucune publication.

**Périmètre strictement le mien** : `docs/index.html`, `docs/rose.wasm`, `docs/rose-wasm.js`,
`rose/web/test-site.mjs`, `rose/web/test-dom.mjs`, `rose/docs/SITE_WASM.md`. Je n'ai modifié
aucun `.rs`, `Cargo.toml`, `PLAN.md`, doc d'un autre agent ni asset binaire existant.

⚠ **Constat de fin de session** : d'autres agents travaillent en parallèle dans le même
worktree. `git status` montre aussi `rose/Cargo.toml` (version 0.1.5 → 0.1.6),
`rose/src/setup_gui.rs`, `rose/src/velopack.rs` et `rose/docs/RELEASE_0_1_6.md` modifiés
entre 14:05 et 14:10 — **ce n'est pas mon travail, je n'y ai pas touché**. Le `rose.wasm`
servi reste celui du 18/11 (moteur 0.1.5) : le glue rapporte `Rose 0.1.5 — compilateur
Rust + JIT Cranelift (sémantique exacte)`. Si un rebuild 0.1.6 produit un nouveau
`rose/web/rose.wasm`, il faudra le recopier vers `docs/rose.wasm` (voir §6).

Le moteur n'a pas été recompilé par mes soins : c'est une copie à l'octet près
de `rose/web/rose.wasm` (md5 `73050f2db15506723cb28a55fa40fd5d`, 736 683 o) qui est servie.

## 1. Ce qui a été remplacé par du vrai WASM

### 1.1 Le terminal factice de l'accueil → terminal réel

**Avant** (`docs/index.html`, anciennes lignes 566–585) : deux `<pre>` statiques. Le second
contenait la sortie **codée en dur** :

```html
<pre><span class="k">&gt;</span> rose bonjour.rose
Salut, Ada !
Salut, Grace !
Salut, Alan !</pre>
```

**Après** : la source du programme reste affichée (mise en couleur inchangée) dans
`<pre id="termSource">`, et le terminal de sortie est `<pre id="termOutput">`. Aucune ligne
de résultat n'est présente dans le HTML : la chaîne `Salut, Ada !` n'apparaît plus nulle part
dans `docs/index.html`. Le script de chargement extrait la **source réellement affichée** et
l'envoie au moteur :

```js
async function executerTerminal(){
  const srcEl = document.getElementById('termSource');
  const outEl = document.getElementById('termOutput');
  if (!srcEl || !outEl || !window.__rose.pret) return;
  outEl.dataset.chargement = '0';
  majBadgeTerminal('WASM : exécution…', '');
  try {
    const texte = await window.__rose.Rose.execute(srcEl.textContent);
    const lignes = texte.replace(/\n$/, '').split('\n');
    outEl.textContent = '';
    const invite = document.createElement('span');
    invite.className = 'k';
    invite.textContent = '> rose bonjour.rose\n';
    outEl.appendChild(invite);
    const resultat = document.createElement('span');
    resultat.textContent = lignes.join('\n') + '\n';
    outEl.appendChild(resultat);
    majBadgeTerminal('WASM ' + window.__rose.Rose.version().split('—')[0].trim(), 'ok');
  } catch (e) { /* badge ko + message explicite, jamais un faux résultat */ }
}
```

`textContent` décode les entités HTML : `&gt;&gt;` devient `>>`, `&lt;` devient `<`. Le
programme exécuté est donc exactement celui que le visiteur lit à gauche. Un bouton `↻`
(`#termRun`) permet de le réexécuter.

Sortie réellement produite par le moteur (mesurée, pas recopiée du HTML) :

```
"Salut, Ada !\nSalut, Grace !\nSalut, Alan !\n"
```

### 1.2 Le playground : interpréteur JavaScript de 640 lignes → vrai moteur wasm

C'était la plus grosse brique « JavaScript basique » du site : `parse()`, `execList()`,
`evn()`, `lexer()`, `qtxt()`… soit un **sous-ensemble** de Rose réécrit en JS, avec ses
propres approximations. L'ancien texte le disait : « *Cette page exécute un sous-ensemble de
Rose, écrit en JavaScript* ».

`runRose()` appelle désormais le moteur réel **en premier** :

```js
async function runRose(){
  const out = document.getElementById('consoleOut');
  out.innerHTML = '';
  const src = document.getElementById('codeEditor').value;
  const t0 = performance.now();
  const duree = () => (performance.now() - t0).toFixed(1);

  /* 1) Moteur réel : le binaire Rose compilé en WebAssembly, chargé en différé. */
  if (window.__rose && window.__rose.pret) {
    try {
      const texte = await window.__rose.Rose.execute(src);
      const lignes = texte.replace(/\n$/, '').split('\n').filter(l => l.length);
      lignes.forEach(l => logL('', l));
      if (!lignes.length) logL('log-info', '(aucune sortie)');
      logL('log-ok', '— moteur WebAssembly · terminé en ' + duree() + ' ms —');
      return;
    } catch (e) { /* repli journalisé */ }
  }
  /* 2) Repli honnête : l'ancien sous-ensemble JS, nommé comme tel. */
  …
}
```

Le texte de présentation du playground a été corrigé en conséquence (« Moteur WebAssembly »
au lieu de « Moteur navigateur », plus de mention de « sous-ensemble approximatif » comme
comportement normal). Les trois exemples (`bonjour`, `rationnels`, `fonctions`) ont été
rejoués contre le moteur : sorties identiques à celles que le site affichait.

### 1.3 Chargement différé

Le module est chargé **après le premier rendu**, par `import()` dynamique dans un dernier
bloc `<script>` (donc aucun `await` au niveau supérieur, aucune compilation bloquante) :

```js
function chargerRose(){
  const etat = window.__rose;
  if (etat.promesse) return etat.promesse;
  etat.promesse = (async () => {
    const mod = await import('./rose-wasm.js');
    const Rose = mod.default;
    Rose.configure({ url: './rose.wasm' });
    await Rose.ready();
    etat.Rose = Rose; etat.pret = true;
    executerTerminal();
    return Rose;
  })();
  return etat.promesse;
}
function demarrerWasm(){
  if ('requestIdleCallback' in window)
    requestIdleCallback(() => chargerRose().catch(() => {}), {timeout: 1500});
  else setTimeout(() => chargerRose().catch(() => {}), 200);
}
if (document.readyState === 'complete') demarrerWasm();
else window.addEventListener('load', demarrerWasm, {once: true});
```

Le glue fourni (`rose/web/rose-wasm.js`, copié à l'identique en `docs/rose-wasm.js`) utilise
`fetch()` + `WebAssembly.instantiateStreaming()` avec repli sur `WebAssembly.instantiate()` —
aucune dépendance, ni wasm-bindgen, ni bundler.

### 1.4 Fichiers ajoutés

| Fichier | Rôle | Taille |
|---|---|---|
| `docs/rose.wasm` | moteur réel servi au navigateur | 736 683 o, md5 `89c6b1…` |
| `docs/rose-wasm.js` | glue C ABI fourni (copie conforme de `rose/web/rose-wasm.js`) | 7 671 o |
| `rose/web/test-site.mjs` | test HTTP + parité des exemples + câblage statique | — |
| `rose/web/test-dom.mjs` | exécution du vrai script du site dans un DOM simulé, chemin normal **et** fallback | — |
| `rose/docs/SITE_WASM.md` | ce document | — |

`docs/telechargements/rose-wasm.wasm` (695 732 o, md5 `88d0e7…`) est un **ancien artefact de
téléchargement** avec entrée dans `SHA256SUMS`. Il n'a PAS été touché. Le site ne l'utilise pas.

## 2. Ce qui reste en JavaScript, et pourquoi

| Brique | Pourquoi elle reste en JS |
|---|---|
| `go()`, `toggleTheme()`, `showDoc()`, `setDocVer()`, `toast()` | Pure manipulation du DOM/CSS et de l'état de l'interface. Aucun calcul : appeler le moteur wasm pour changer une classe CSS serait un aller-retour sans sens. |
| Recherche dans la doc (`searchData`, `renderSearch()`) | Filtre une liste littérale de 20 entrées avec `String.includes()`. C'est du pilotage d'UI, pas un calcul ; le faire en wasm imposerait de sérialiser les données, d'envoyer la requête et de réinterpréter la sortie stdout pour un gain nul. |
| Particules canvas (`loop()`, `makeParticle()`) | Rendu graphique temps réel via l'API Canvas du navigateur. Le moteur Rose n'a ni Canvas ni accès au DOM. |
| Copie dans le presse-papiers (`copyTerm()`) | API navigateur (`navigator.clipboard`). |
| L'ancien interpréteur JS (640 lignes) | Conservé **uniquement comme repli** si `rose.wasm` ne charge pas. La console l'identifie explicitement (`moteur JavaScript (sous-ensemble)`). Aucun usage tant que le wasm est disponible. |

Donc : **non, tout le site n'est pas en Rose.** Le terminal d'accueil et le playground
exécutent le vrai moteur wasm ; l'habillage (navigation, thème, recherche, particules,
presse-papiers) reste en JavaScript parce que c'est du code de présentation, pas du calcul.

## 3. Tests réels (commandes + sorties)

### 3.1 Point fixe — parité native/wasm du moteur (15/15)

```
$ cd rose/web && node test-wasm.mjs ./rose.wasm
version : Rose 0.1.5 — compilateur Rust + JIT Cranelift (sémantique exacte)
natif   : /home/clausake/Stefan/ROSE/rose/target/release/rose
OK   parité native : addition
... (10 programmes)
OK   erreur runtime
OK   erreur d'analyse
OK   latex
OK   parité native : latex du programme
OK   analyse
Tous les tests passent.
```

### 3.2 Site servi par HTTP + `Rose.execute(">> 47")`

```
$ cd rose/web && node test-site.mjs
serveur : http://127.0.0.1:45617 → racine /home/clausake/Stefan/ROSE/docs

HTTP /index.html     → 200, Content-Type: text/html
HTTP /rose-wasm.js   → 200, Content-Type: text/javascript
HTTP /rose.wasm      → 200, Content-Type: application/wasm
version  : Rose 0.1.5 — compilateur Rust + JIT Cranelift (sémantique exacte)
instance : true

OK   Rose.execute(">> 47")
OK   terminal d’accueil = vraie sortie WASM
OK   playground bonjour.rose = vraie sortie WASM
OK   playground rationnels.rose = vraie sortie WASM
OK   playground fonctions.rose = vraie sortie WASM
OK   script #1/#2/#3 : syntaxe valide
OK   ID unique : termSource / termOutput / termLive / termRun / codeEditor / consoleOut
OK   aucune sortie « Salut, Ada ! » codée en dur
OK   chargement différé (import dynamique)
OK   fallback WASM explicite
OK   docs/rose.wasm = rose/web/rose.wasm
Tous les tests du site passent.
```

Le test demande `Rose.execute(">> 47")` et obtient **`"47\n"`** (avec le retour à la ligne
final produit par le moteur — c'est la valeur exacte, pas `"47"` littéral).

### 3.3 Exécution du vrai script du site (DOM simulé)

Firefox headless est présent (`/usr/bin/firefox`) mais **inutilisable ici** : le paquet snap
échoue avec `Sandbox: CanCreateUserNamespace() clone() failure: ENOSPC` et
`Could not find profile folder` ; aucun profil Firefox n'existe sur la machine. Il n'y a pas
de Playwright. Conformément à la consigne, le repli est utilisé : test du moteur + exécution
du code réel du site.

```
$ cd rose/web && node test-dom.mjs
OK   script du site exécutable
OK   module WASM prêt
OK   aucune erreur de chargement
OK   badge terminal = WASM
OK   terminal rempli par la sortie réelle du moteur
OK   invitation de commande conservée
Test DOM (normal) : OK.

$ cd rose/web && node test-dom.mjs --fallback
OK   _module absent → erreur enregistrée
OK   _module absent → pret = false
OK   _fallback honnête dans le badge
OK   _aucun faux résultat affiché
OK   _message explicite dans le terminal
Test DOM (fallback) : OK.
```

Ce test extrait le **dernier bloc `<script>` de `docs/index.html`**, l'écrit dans `docs/` sous
un nom temporaire (supprimé ensuite), l'importe comme module ESM avec un `fetch` réécrit vers
le serveur HTTP local. Il prouve que le **code réellement livré** appelle `Rose.ready()` puis
`Rose.execute()` et remplit `<pre id="termOutput">` avec la sortie du moteur. Ce n'est **pas**
un test de rendu visuel.

### 3.4 Curl direct (contrôle indépendant)

```
$ python3 -m http.server 18991 --bind 127.0.0.1 --directory docs &
$ curl -s -o /dev/null -w "%{http_code} %{content_type} %{size_download}\n" \
    http://127.0.0.1:18991/index.html \
    http://127.0.0.1:18991/rose-wasm.js \
    http://127.0.0.1:18991/rose.wasm
index.html   200 text/html 132838
rose-wasm.js 200 text/javascript 7671
rose.wasm    200 application/wasm 736683
```

## 4. Fallback

- Si `import('./rose-wasm.js')` ou `Rose.ready()` échoue : `window.__rose.erreur` est posé,
  le badge passe en `WASM indisponible` (rouge), et le terminal affiche
  `(moteur WebAssembly indisponible : …)` — **aucun résultat inventé**.
- Si l'utilisateur clique `EXÉCUTER ▶` dans le playground sans wasm disponible : la console
  affiche un avertissement `log-warn` (`Module WebAssembly indisponible (…)`) puis exécute
  l'ancien interpréteur JS **en l'identifiant** dans la ligne de fin
  (`moteur JavaScript (sous-ensemble) · terminé en … ms`).
- Si le wasm est encore en cours de chargement : message `en cours de chargement — repli
  temporaire sur le moteur JavaScript`.
- Le serveur doit fournir `application/wasm` (vérifié : Python 3.x le fait nativement) ;
  le glue sait sinon retomber sur `WebAssembly.instantiate(arrayBuffer)`.
- `file://` ne fonctionne pas (CORS sur les modules/wasm) : il faut servir `docs/` par HTTP.

## 5. Chemins

- Site : `docs/index.html` (modifié)
- Moteur servi : `docs/rose.wasm`, `docs/rose-wasm.js` (ajoutés, conformes à `rose/web/`)
- Glue d'origine : `rose/web/rose-wasm.js` (inchangé)
- Test moteur : `rose/web/test-wasm.mjs` (inchangé, rejoué 15/15)
- Tests site : `rose/web/test-site.mjs`, `rose/web/test-dom.mjs` (ajoutés)
- Ce document : `rose/docs/SITE_WASM.md`

## 6. Limites connues

- Aucun navigateur headless fonctionnel : le test DOM simulé couvre l'exécution du JS mais pas
  le rendu réel, le CSS ni les interactions clavier/souris. Une vérification visuelle dans un
  vrai navigateur reste à faire.
- Le wasm servi est figé dans `docs/` : toute reconstruction du moteur devra recopier
  `rose/web/rose.wasm` vers `docs/rose.wasm` (une commande de copie, pas une régénération
  automatique).
- Le moteur Rose n'expose pas de service de recherche : la recherche de la doc reste donc en
  JavaScript, volontairement (voir §2).
