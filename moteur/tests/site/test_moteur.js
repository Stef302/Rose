/* Harnais : exécute le moteur du site (extrait du <script>) hors navigateur. */
const fs = require('fs');
const code = fs.readFileSync(require('path').join(__dirname, 'moteur-site.js'), 'utf8');

const out = { innerHTML: '', lines: [], appendChild(el) { this.lines.push(el.textContent); }, scrollTop: 0, scrollHeight: 0 };
const editor = { value: '' };
const element = { textContent: '', value: '', dataset: {}, classList: { toggle() {}, add() {}, remove() {} }, style: {}, getAttribute() { return ''; }, click() {} };
const document = {
  getElementById(id) { if (id === 'consoleOut') return out; if (id === 'codeEditor') return editor; return element; },
  createElement() { return { className: '', textContent: '' }; },
  querySelectorAll() { return []; },
  querySelector() { return null; },
  addEventListener() {},
  body: { appendChild() {}, removeChild() {} },
};
const performance = { now: () => 0 };
const navigator = { clipboard: null };
const window = {};

const factory = new Function('document', 'performance', 'navigator', 'window',
  code + '\n; return { runRose, EXEMPLES, lexer, parse };');
const api = factory(document, performance, navigator, window);

function run(src) { out.innerHTML = ''; out.lines.length = 0; editor.value = src; api.runRose(); return out.lines.filter(l => !l.startsWith('— terminé')); }

let echecs = 0, tests = 0;
function chk(nom, src, attendu) {
  tests++;
  let rendu;
  try { rendu = run(src); } catch (e) { rendu = ['EXCEPTION JS: ' + (e.stack || e)]; }
  const a = Array.isArray(attendu) ? attendu : [attendu];
  const ok = JSON.stringify(rendu) === JSON.stringify(a);
  if (!ok) { echecs++; console.log('ÉCHEC ' + nom + '\n  attendu: ' + JSON.stringify(a) + '\n  rendu  : ' + JSON.stringify(rendu)); }
  else console.log('ok  ' + nom);
}

chk('tiers', '>> 1 / 3', '0.[3]');
chk('sixiemes', '>> 1 / 6', '0.1[6]');
chk('sept sur sept', '>> 22 / 7', '3.[142857]');
chk('demi', '>> 1 / 2', '0.5');
chk('negatif quirk', '>> 0 - 1 / 3', '-1.[3]');
chk('somme complete', '>> 1 / 3 + 1 / 3 + 1 / 3', '1');
chk('chaine rationnel', '>> chaine(1 / 3)', '1/3');
chk('flottant entier', '>> 10.0', '10');
chk('chaine flottant', '>> chaine(10.0)', '10');
chk('add flottant', '>> 0.1 + 0.2', '0.30000000000000004');
chk('unaire avant puissance', '>> -2 ^ 2', '4');
chk('exposant negatif', '>> 2 ^ -3', '0.125');
chk('exposant rationnel neg', '>> 0 - 1 / 8', '-1.125');
chk('grand entier', '>> 2 ^ 64', '18446744073709551616');
chk('fraction affichee', '>> fraction(22, 7)', '3.[142857]');
chk('fraction chaine', '>> chaine(fraction(22, 7))', '22/7');
chk('partie entiere', '>> partie_entiere(0 - 1 / 2)', '-1');
chk('div florentine', '>> -7 div 2\n>> -7 % 2', ['-4', '1']);
chk('tranches', '$ch = "bonjour"\n>> $ch[1:3]\n>> $ch[:2]\n>> $ch[2:]', ['on', 'bo', 'njour']);
chk('liste indexee', '$l = [1, 2]\njoindre($l, 3)\n$l[0] = 9\n>> $l', '[9, 2, 3]');
chk('typage variable', '$x = 1\n$x = 2.5\n>> $x', "Rose : erreur — [Ligne 2, Colonne 0] 'x' est entier, on ne peut pas y stocker du flottant");
chk('promotion z vers q', '$q = 1 / 2\n$q = 5\n>> $q', '5');
chk('non init', '>> $nope', "Rose : erreur — [Ligne 1, Colonne 0] Variable 'nope' utilisée sans valeur");
chk('div runtime', '$a = 0\n>> 5 div $a', 'Rose : erreur — division euclidienne par zéro');
chk('modulo runtime', '$d = 0\n>> 1 % $d', 'Rose : erreur — modulo par zéro');
chk('index chaine', '$ch = "a"\n>> $ch[5]', 'Rose : erreur — index hors des limites de la chaîne');
chk('ordre mixte', '>> 1 < "a"', 'Rose : erreur — [Ligne 1, Colonne 0] Ordre entier < chaine impossible');
chk('condition booléenne', 'si (1) { >> 1 }', 'Rose : erreur — [Ligne 1, Colonne 0] La condition de si doit être booléenne, trouvé entier');
chk('fonction inconnue', '>> zoulou(1)', "Rose : erreur — [Ligne 1, Colonne 0] Fonction 'zoulou' inconnue");
chk('bloque table', '>> table()', "Rose : erreur — « table » demande le moteur installé — installe Rose (rubrique Installer)");
chk('compact si', '? (1 < 2) [ >> "oui" ]', 'oui');
chk('compact tant que', '$i = 0\n@ ($i < 3) [ >> $i; $i = $i + 1 ]', ['0', '1', '2']);
chk('sinon si', '$n = 7\nsi ($n >= 10) { >> "admis" } sinon si ($n >= 5) { >> "rattrapage" } sinon { >> "recalcul" }', 'rattrapage');
chk('pour borne exclusive', 'pour k de 0 vers 3 { >> k }', ['0', '1', '2']);
chk('chaine + chaine', '>> "Salut, " + "Ada" + " !"', 'Salut, Ada !');
chk('concat + longueur', '$c = ["ab", "cd"]\n>> concatener($c)\n>> longueur("café")\n>> ordre("a")\n>> caractere(233)', ['abcd', '4', '97', 'é']);
chk('abs min max', '>> abs(0 - 5)\n>> min(2.5, 2)\n>> max(1 / 2, 0.4)', ['5', '2', '0.5']);
chk('conversions', '>> entier("42")\n>> flottant(1 / 2)\n>> chaine(vrai)\n>> chaine(faux)', ['42', '0.5', 'vrai', 'faux']);

chk('exemple bonjour', api.EXEMPLES.bonjour, ['Salut, Ada !', 'Salut, Grace !', 'Salut, Alan !']);
chk('exemple rationnels', api.EXEMPLES.rationnels,
  ['0.[3]', '0.[6]', '1', '3.5', '0.125', '18446744073709551616', '1.928[968253]']);
chk('exemple fonctions', api.EXEMPLES.fonctions,
  ['6765', '21', '1307674368000', '65', 'vrai']);

/* — contre-épreuve : mêmes programmes rendus par le moteur C++ — */
const { execFileSync } = require('child_process');
const fsx = fs;
function oracle(src, idx) {
  fsx.writeFileSync('/tmp/opencode/hn.rose', src);
  try {
    const o = execFileSync('/home/clausake/Stefan/ROSE/moteur/cpp/rose', ['/tmp/opencode/hn.rose'],
      { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], env: { ...process.env, ROSE_GMP: '/tmp/opencode/gmp-linux' } });
    return o.replace(/\n$/, '').split('\n');
  } catch (e) {
    return ((e.stdout || '') + (e.stderr || '')).replace(/\n$/, '').split('\n').filter(s => s.length);
  }
}
const oracleCases = [
  ['tiers', '>> 1 / 3'], ['negatif quirk', '>> 0 - 1 / 3'], ['sixiemes', '>> 1 / 6'],
  ['chaine rationnel', '>> chaine(1 / 3)'], ['flottant entier', '>> 10.0'],
  ['unaire avant puissance', '>> -2 ^ 2'], ['exposant neg', '>> 2 ^ -3'],
  ['rationnel neg oct', '>> 0 - 1 / 8'], ['div florentine', '>> -7 div 2\n>> -7 % 2'],
  ['typage', '$x = 1\n$x = 2.5\n>> $x'],
  ['promotion', '$q = 1 / 2\n$q = 5\n>> $q'],
  ['div runtime', '$a = 0\n>> 5 div $a'],
  ['slash runtime', '$b = 0\n>> 1 / $b'],
  ['modulo runtime', '$d = 0\n>> 1 % $d'],
  ['index chaine', '$ch = "a"\n>> $ch[5]'],
  ['ordre mixte', '>> 1 < "a"'],
  ['condition booleenne', 'si (1) { >> 1 }'],
  ['fonction inconnue', '>> zoulou(1)'],
  ['variable sans valeur', '>> $nope'],
  ['abs/min/max', '>> abs(0 - 5)\n>> min(2.5, 2)\n>> max(1 / 2, 0.4)'],
  ['conversions', '>> entier("42")\n>> flottant(1 / 2)\n>> chaine(vrai)\n>> chaine(faux)'],
  ['partie entiere', '>> partie_entiere(0 - 1 / 2)'],
  ['somme complete', '>> 1 / 3 + 1 / 3 + 1 / 3'],
  ['longueur cafe', '>> longueur("café")'],
  ['caractere', '>> caractere(233)'],
  ['compact', '$i = 0\n@ ($i < 3) [ >> $i; $i = $i + 1 ]'],
];
for (const [nom, src] of oracleCases) {
  tests++;
  const js = run(src);
  const c = oracle(src);
  const ok = JSON.stringify(js) === JSON.stringify(c);
  if (!ok) { echecs++; console.log('DIVERGENCE vs ORACLE ' + nom + '\n  oracle: ' + JSON.stringify(c) + '\n  site  : ' + JSON.stringify(js)); }
  else console.log('≡   ' + nom);
}

console.log('\nTests : ' + (tests - echecs) + '/' + tests + ' réussis');
process.exit(echecs ? 1 : 0);
