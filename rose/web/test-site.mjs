// Test du site `docs/index.html` branché sur le VRAI moteur Rose WebAssembly.
//
//   node web/test-site.mjs
//
// 1. sert docs/ par python3 -m http.server (comme un vrai site statique) ;
// 2. vérifie que index.html, rose-wasm.js et rose.wasm répondent 200 avec le
//    bon type MIME ;
// 3. importe le GLUE FOURNI (docs/rose-wasm.js) et charge rose.wasm PAR HTTP
//    exactement comme le navigateur (fetch + WebAssembly.instantiateStreaming) ;
// 4. vérifie Rose.execute('>> 47') === '47\n' et rejoue les trois exemples du
//    site — y compris la source extraite du terminal de la page ;
// 5. vérifie statiquement le câblage HTML (IDs, scripts syntaxiquement valides,
//    absence de sortie codée en dur).
//
// Sortie : 0 si tout est conforme, 1 sinon. Aucun commit, aucune publication.
import fs from 'node:fs';
import path from 'node:path';
import net from 'node:net';
import vm from 'node:vm';
import { fileURLToPath } from 'node:url';
import { spawn } from 'node:child_process';

const ici = path.dirname(fileURLToPath(import.meta.url));
const docs = path.resolve(ici, '../../docs');
const htmlPath = path.join(docs, 'index.html');
const html = fs.readFileSync(htmlPath, 'utf8');

let echecs = 0;
function verifier(nom, attendu, obtenu) {
  const ok = attendu === obtenu;
  if (!ok) echecs++;
  console.log(`${ok ? 'OK  ' : 'FAIL'} ${nom}`);
  if (!ok) {
    console.log(`      attendu : ${JSON.stringify(attendu)}`);
    console.log(`      obtenu  : ${JSON.stringify(obtenu)}`);
  }
}
function assertVrai(nom, cond, detail) {
  if (!cond) echecs++;
  console.log(`${cond ? 'OK  ' : 'FAIL'} ${nom}`);
  if (!cond && detail) console.log('      ' + detail);
}

// ── 1. Port libre + serveur HTTP statique ──────────────────────────────────
const port = await new Promise((res, rej) => {
  const s = net.createServer();
  s.once('error', rej);
  s.listen(0, '127.0.0.1', () => {
    const p = s.address().port;
    s.close(() => res(p));
  });
});
const base = `http://127.0.0.1:${port}`;
const serveur = spawn('python3', ['-m', 'http.server', String(port), '--bind', '127.0.0.1'],
  { cwd: docs, stdio: ['ignore', 'pipe', 'pipe'] });
let errServeur = '';
serveur.stderr.on('data', d => { errServeur += d.toString(); });

async function attendreServeur() {
  for (let i = 0; i < 60; i++) {
    try {
      const r = await fetch(base + '/index.html');
      if (r.ok) return;
    } catch { /* pas encore prêt */ }
    await new Promise(r => setTimeout(r, 100));
  }
  throw new Error('serveur HTTP jamais prêt : ' + errServeur);
}
try { await attendreServeur(); }
catch (e) { console.error(e.message); serveur.kill(); process.exit(2); }
console.log('serveur :', base, '→ racine', docs, '\n');

// ── 2. Ressources servies ──────────────────────────────────────────────────
for (const [f, type] of [
  ['/index.html', 'text/html'],
  ['/rose-wasm.js', 'text/javascript'],
  ['/rose.wasm', 'application/wasm'],
]) {
  const r = await fetch(base + f);
  console.log(`HTTP ${f} → ${r.status}, Content-Type: ${r.headers.get('content-type')}`);
  verifier('GET ' + f + ' = 200', 200, r.status);
  assertVrai('MIME ' + f, (r.headers.get('content-type') || '').includes(type));
}

// ── 3. Le VRAI glue, chargé PAR HTTP comme dans le navigateur ──────────────
const { default: Rose } = await import(path.join(docs, 'rose-wasm.js'));
Rose.configure({ url: base + '/rose.wasm' });
await Rose.ready();
console.log('\nversion  :', Rose.version());
console.log('instance :', !!Rose.instance, '\n');

// Le test demandé explicitement.
verifier('Rose.execute(">> 47")', '47\n', await Rose.execute('>> 47'));

// ── 4. Les trois exemples réellement affichés par le site ──────────────────
const ATTENDU = {
  'bonjour.rose': 'Salut, Ada !\nSalut, Grace !\nSalut, Alan !\n',
  'rationnels.rose': '0.[3]\n0.[6]\n1\n3.5\n0.125\n18446744073709551616\n1.928[968253]\n',
  'fonctions.rose': '6765\n21\n1307674368000\n65\nvrai\n',
};

// 4a. Source du TERMINAL extraite du HTML (donc exactement ce que voit le JS).
const m = html.match(/<pre id="termSource">([\s\S]*?)<\/pre>/);
assertVrai('terminal : source <pre id="termSource"> trouvée', !!m);
if (m) {
  const source = m[1]
    .replace(/<[^>]+>/g, '')
    .replace(/&gt;/g, '>').replace(/&lt;/g, '<').replace(/&amp;/g, '&');
  verifier('terminal d’accueil = vraie sortie WASM',
    ATTENDU['bonjour.rose'], await Rose.execute(source));
}

// 4b. Sources des 3 onglets du playground, extraites du HTML.
const bloc = html.match(/const EXEMPLES = \{([\s\S]*?)\n\};/);
assertVrai('playground : EXEMPLES trouvé', !!bloc);
if (bloc) {
  const sandbox = {};
  vm.createContext(sandbox);
  try {
    vm.runInContext('var EXEMPLES = {' + bloc[1] + '\n};', sandbox);
    for (const [nom, attendu] of Object.entries(ATTENDU)) {
      const cle = nom.replace('.rose', '');
      if (sandbox.EXEMPLES[cle]) {
        verifier('playground ' + nom + ' = vraie sortie WASM',
          attendu, await Rose.execute(sandbox.EXEMPLES[cle]));
      }
    }
  } catch (e) {
    assertVrai('EXEMPLES évaluable', false, e.message);
  }
}

// ── 5. Câblage HTML statique ───────────────────────────────────────────────
const scripts = [...html.matchAll(/<script(?: [^>]*)?>([\s\S]*?)<\/script>/g)].map(x => x[1]);
assertVrai('3 blocs <script> présents', scripts.length === 3, `trouvé ${scripts.length}`);
scripts.forEach((s, i) => {
  try { new vm.Script(s); console.log(`OK   script #${i + 1} : syntaxe valide`); }
  catch (e) { echecs++; console.log(`FAIL script #${i + 1} : ${e.message}`); }
});
for (const id of ['termSource', 'termOutput', 'termLive', 'termRun', 'codeEditor', 'consoleOut']) {
  const n = (html.match(new RegExp('id="' + id + '"', 'g')) || []).length;
  verifier('ID unique : ' + id, 1, n);
}
assertVrai('aucune sortie « Salut, Ada ! » codée en dur', !html.includes('Salut, Ada !'));
assertVrai('chargement différé (import dynamique)', html.includes("await import('./rose-wasm.js')"));
assertVrai('fallback WASM explicite', html.includes('WASM indisponible'));
assertVrai('playground appelle Rose.execute', html.includes('window.__rose.Rose.execute(src)'));

// Le binaire servi doit être IDENTIQUE au moteur de rose/web (pas l'ancien
// artefact de telechargements/).
const crypto = await import('node:crypto');
const h = f => crypto.createHash('md5').update(fs.readFileSync(f)).digest('hex');
verifier('docs/rose.wasm = rose/web/rose.wasm',
  h(path.join(ici, 'rose.wasm')), h(path.join(docs, 'rose.wasm')));

serveur.kill();
console.log('\n' + (echecs === 0 ? 'Tous les tests du site passent.' : `${echecs} test(s) en échec.`));
process.exit(echecs === 0 ? 0 : 1);
