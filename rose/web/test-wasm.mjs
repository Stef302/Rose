// Test du module WebAssembly Rose HORS navigateur (Node.js).
//
//   node web/test-wasm.mjs [chemin/vers/rose.wasm]
//
// Instancie le module avec le VRAI glue JavaScript (web/rose-wasm.js) et
// vérifie le comportement du moteur : sorties, erreurs runtime, erreurs
// d'analyse, rendu LaTeX, analyse.
//
// Le test le plus important est la PARITÉ AVEC LE MOTEUR NATIF : pour chaque
// programme, la sortie du module WASM doit être identique, au caractère
// près, à celle du binaire `rose` de la même arborescence. C'est cet
// invariant qui garantit que le web calcule exactement comme le natif.
//
// Si le binaire natif est absent (ou n'est pas exécutable ici), les
// comparaisons de parité sont sautées et seules les vérifications
// structurelles restent.
//
// Sortie : 0 si tout est conforme, 1 sinon.
import fs from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

const ici = path.dirname(fileURLToPath(import.meta.url));
const racine = path.resolve(ici, '..');
const wasm = process.argv[2] || path.join(racine, 'target/wasm32-unknown-unknown/wasm-release/rose.wasm');

if (!fs.existsSync(wasm)) {
  console.error(`Module WASM introuvable : ${wasm}`);
  console.error('Le construire :');
  console.error('  cargo build --profile wasm-release --target wasm32-unknown-unknown --no-default-features');
  console.error(`  cp target/wasm32-unknown-unknown/wasm-release/rose.wasm ${path.join(racine, 'web/')}`);
  process.exit(2);
}

// Binaire natif (s'il existe) : sert de référence de parité.
const natif = ['target/release/rose', 'rose', '../rose/rose']
  .map((p) => path.join(racine, p))
  .find((p) => fs.existsSync(p) && fs.statSync(p).isFile());

function sortieNative(source) {
  if (!natif) return null;
  const f = path.join('/tmp', `rose_parite_${process.pid}.rose`);
  fs.writeFileSync(f, source);
  try {
    const r = spawnSync(natif, [f], { encoding: 'utf8' });
    return (r.stdout || '') + (r.stderr || '');
  } finally {
    try { fs.unlinkSync(f); } catch { /* fichier déjà supprimé */ }
  }
}

const { default: Rose } = await import('./rose-wasm.js');
Rose.configure({ bytes: fs.readFileSync(wasm) });

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

await Rose.ready();
console.log('module  :', wasm);
console.log('version :', Rose.version());
console.log('natif   :', natif || '(absent — parité native non vérifiée)');
console.log('exports :', Object.keys(Rose.instance.exports).sort().join(', '));
console.log('');

// ── Programmes de référence : la sortie WASM doit être celle du natif ────
const programmes = [
  ['addition', '>> 1 + 2\n'],
  ['flottants', '>> 3.5 + 1.5\n'],
  ['rationnel exact', '>> 1 / 2 + 1 / 3\n'],
  ['entier exact', '>> 2 ^ 100\n'],
  ['boucle (pour)', '$t = 0\npour j de 1 vers 10 { $t = $t + j }\n>> $t\n'],
  ['fonction', 'fn carre(x: entier) -> entier { retour x * x }\n>> carre(7)\n'],
  ['sortie multiple', '>> "a"\n>> "b"\n>> "c"\n'],
  ['complexes', '$z = 3 + 4i\n>> $z\n>> modulus($z)\n>> re($z)\n'],
  ['irrationnels', '>> racine(2)\n>> pi\n'],
  ['comparaisons', '>> 1 < 2\n>> 2 <= 2\n>> "a" == "a"\n'],
];

for (const [nom, source] of programmes) {
  const wasmOut = await Rose.execute(source);
  const natOut = sortieNative(source);
  if (natOut === null) {
    // Sans binaire natif : on vérifie seulement que l'exécution aboutit.
    const ok = typeof wasmOut === 'string' && wasmOut.length > 0;
    if (!ok) echecs++;
    console.log(`${ok ? 'OK  ' : 'FAIL'} ${nom} (sans référence native)`);
    if (!ok) console.log(`      obtenu : ${JSON.stringify(wasmOut)}`);
  } else {
    verifier(`parité native : ${nom}`, natOut, wasmOut);
  }
}

// ── Comportements spécifiques du module WASM ─────────────────────────────
verifier('erreur runtime', 'Rose : erreur — division par zéro\n',
  await Rose.execute('$z = 0\n>> 5 / $z\n'));
verifier(
  'erreur d\'analyse',
  "Rose : erreur — [Ligne 5, Colonne 1] Attente d'une expression, trouvé ''\n",
  await Rose.execute('fn f(x: entier) -> entier {\n    retour x\n}\n>> f(\n'));
verifier('latex', '\\mathrm{afficher}\\left(2\\right)', (await Rose.latex('>> 1 + 1\n')).trim());
// LaTeX du programme ENTIER (fonctions comprises) : doit être identique à
// celui du moteur natif (`rose latex programme.rose`).
const programmeLatex = [
  '$s = 0\n',
  'pour j de 0 vers 10 { $s = $s + j }\n',
  'fn carre(x: entier) -> entier { retour x * x }\n',
  '>> $s\n',
  '>> carre(7)\n',
].join('');
if (natif) {
  const f = path.join('/tmp', `rose_latex_${process.pid}.rose`);
  fs.writeFileSync(f, programmeLatex);
  const r = spawnSync(natif, ['latex', f], { encoding: 'utf8' });
  try { fs.unlinkSync(f); } catch { /* déjà supprimé */ }
  verifier('parité native : latex du programme', r.stdout, await Rose.latex(programmeLatex));
}
verifier('analyse', true, (await Rose.analyse('fn f(x: entier) -> entier { retour x }\n>> f(1)\n')).startsWith('programme valide'));

console.log('');
console.log(echecs === 0 ? 'Tous les tests passent.' : `${echecs} test(s) en échec.`);
process.exit(echecs === 0 ? 0 : 1);
