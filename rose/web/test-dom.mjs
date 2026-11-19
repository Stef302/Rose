// Exécute le VRAI script de chargement extrait de docs/index.html dans un DOM
// simulé (Node), avec le VRAI rose.wasm servi par HTTP. Complète test-site.mjs
// (vérification statique) quand aucun navigateur headless n'est disponible.
//
//   node web/test-dom.mjs
//
// Ce n'est PAS un test de rendu visuel : c'est un test d'exécution du code
// JavaScript réellement livré dans la page (import() différé,Rose.ready(),
// Rose.execute(), remplissage de <pre id="termOutput">).
import fs from 'node:fs';
import path from 'node:path';
import net from 'node:net';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { spawn } from 'node:child_process';

const ici = path.dirname(fileURLToPath(import.meta.url));
const docs = path.resolve(ici, '../../docs');
const html = fs.readFileSync(path.join(docs, 'index.html'), 'utf8');

// ── Script du site : le dernier bloc <script> (celui qui charge le wasm) ───
const blocs = [...html.matchAll(/<script(?: [^>]*)?>([\s\S]*?)<\/script>/g)].map(x => x[1]);
const loader = blocs.find(s => s.includes('function chargerRose()'));
if (!loader) { console.error('Script de chargement WASM introuvable dans index.html'); process.exit(2); }

// ── Serveur HTTP (le vrai chemin fetch + instantiateStreaming) ─────────────
const port = await new Promise((res, rej) => {
  const s = net.createServer();
  s.once('error', rej);
  s.listen(0, '127.0.0.1', () => { const p = s.address().port; s.close(() => res(p)); });
});
const base = `http://127.0.0.1:${port}`;
const serveur = spawn('python3', ['-m', 'http.server', String(port), '--bind', '127.0.0.1'],
  { cwd: docs, stdio: 'ignore' });
for (let i = 0; i < 60; i++) {
  try { if ((await fetch(base + '/index.html')).ok) break; } catch { /* attente */ }
  await new Promise(r => setTimeout(r, 100));
}

// ── DOM minimal : seulement ce que le script du site utilise ───────────────
function element() {
  return {
    textContent: '', innerHTML: '', className: '', dataset: {}, children: [],
    appendChild(c) { this.children.push(c); this.textContent += (c.textContent || ''); },
    addEventListener() {},
  };
}
const termSource = element();
const sourceHTML = html.match(/<pre id="termSource">([\s\S]*?)<\/pre>/)[1];
termSource.textContent = sourceHTML.replace(/<[^>]+>/g, '')
  .replace(/&gt;/g, '>').replace(/&lt;/g, '<').replace(/&amp;/g, '&');
const els = {
  termSource,
  termOutput: Object.assign(element(), { dataset: { chargement: '1' } }),
  termLive: element(),
  termRun: element(),
};
globalThis.document = {
  readyState: 'complete',                      // → démarrage immédiat du chargement différé
  getElementById: id => els[id] || null,
  createElement: () => element(),
  addEventListener() {},
};
globalThis.window = globalThis;
globalThis.addEventListener = () => {};
globalThis.requestIdleCallback = cb => cb();   // pas d'attente oisive en Node

// Le glue configure une URL RELATIVE : on la réécrit vers le serveur local,
// exactement comme le navigateur le ferait depuis la page index.html.
const modeFallback = process.argv.includes('--fallback');
const fetchNode = globalThis.fetch;
globalThis.fetch = (u, o) => {
  const cible = typeof u === 'string' && u.startsWith('./') ? base + '/' + u.slice(2) : u;
  if (modeFallback && String(cible).endsWith('/rose.wasm')) {
    return Promise.resolve(new Response('introuvable', { status: 404 }));
  }
  return fetchNode(cible, o);
};

// Écrire le script TEL QUEL dans docs/ pour que import('./rose-wasm.js')
// se résolve comme dans la page, puis l'exécuter comme module ESM.
const tmp = path.join(docs, '__test_loader.mjs');
fs.writeFileSync(tmp, loader);
let errImport = null;
try { await import(pathToFileURL(tmp).href + '?t=' + Date.now()); }
catch (e) { errImport = e; }
finally { fs.unlinkSync(tmp); }

let echecs = 0;
function verifier(nom, cond, detail) {
  if (!cond) echecs++;
  console.log(`${cond ? 'OK  ' : 'FAIL'} ${nom}`);
  if (!cond && detail) console.log('      ' + detail);
}

for (let i = 0; i < 100 && !(globalThis.__rose?.pret || globalThis.__rose?.erreur); i++) {
  await new Promise(r => setTimeout(r, 50));
}
const etat = globalThis.__rose || {};
verifier('script du site exécutable', !errImport, errImport && errImport.message);
if (modeFallback) {
  verifier('_module absent → erreur enregistrée', !!etat.erreur);
  verifier('_module absent → pret = false', etat.pret === false);
  verifier('_fallback honnête dans le badge', /indisponible/.test(els.termLive.textContent), els.termLive.textContent);
  verifier('_aucun faux résultat affiché', !els.termOutput.textContent.includes('Salut, Ada !'),
    JSON.stringify(els.termOutput.textContent));
  verifier('_message explicite dans le terminal', els.termOutput.innerHTML.includes('moteur WebAssembly indisponible'),
    els.termOutput.innerHTML);
} else {
  verifier('module WASM prêt', etat.pret === true, etat.erreur && etat.erreur.message);
  verifier('aucune erreur de chargement', !etat.erreur, etat.erreur && etat.erreur.message);
  verifier('badge terminal = WASM', /^WASM /.test(els.termLive.textContent), els.termLive.textContent);
  verifier('terminal rempli par la sortie réelle du moteur',
    els.termOutput.textContent.includes('Salut, Ada !\nSalut, Grace !\nSalut, Alan !'),
    JSON.stringify(els.termOutput.textContent));
  verifier('invitation de commande conservée',
    els.termOutput.textContent.startsWith('> rose bonjour.rose'),
    JSON.stringify(els.termOutput.textContent));
}

serveur.kill();
console.log('\n' + (echecs === 0 ? 'Test DOM (' + (modeFallback ? 'fallback' : 'normal') + ') : OK.' : `${echecs} test(s) DOM en échec.`));
process.exit(echecs === 0 ? 0 : 1);
