
/* ══════════════════════════════════════════════════════════════════
   Moteur navigateur — sous-ensemble de Rose v3.14.
   Miroir du moteur de référence : entiers exacts (BigInt), rationnels
   rendus en décimale périodique « 0.1[6] », flottants IEEE-754,
   variable typée par sa première affectation, erreurs nommées.
   ══════════════════════════════════════════════════════════════════ */
const EXEMPLES = {
bonjour: `// bonjour.rose — fonctions, listes, boucle
fn saluer($nom: chaine) -> chaine {
  $msg = "Salut, " + $nom + " !"
  retour $msg
}

$noms = ["Ada", "Grace", "Alan"]
$i = 0
tant_que ($i < longueur($noms)) {
  >> saluer($noms[$i])
  $i = $i + 1
}`,
rationnels: `// rationnels.rose — le calcul sans arrondi
>> 1 / 3
>> 1 / 3 + 1 / 3
>> 1 / 3 + 1 / 3 + 1 / 3
>> 7 / 2
>> 2 ^ -3
>> 2 ^ 64

// somme des inverses de 3 à 10, exacte
$exact = 1 / 2
pour $k de 3 vers 11 { $exact = $exact + 1 / $k }
>> $exact`,
fonctions: `// fonctions.rose — récursion et bibliothèque
fn fib($n: entier) -> entier {
  si ($n < 2) { retour $n }
  retour fib($n - 1) + fib($n - 2)
}

>> fib(20)
>> pgcd(462, 1071)
>> factorielle(15)
>> nb_chiffres(factorielle(50))
>> est_premier(97)
`};

/* — valeurs : {t:'z'|'f'|'q'|'b'|'s'|'l'|'v', v} — */
const Z = v => ({t:'z', v}), F = v => ({t:'f', v}), B = v => ({t:'b', v}),
      S = v => ({t:'s', v}), L = v => ({t:'l', v}), VID = {t:'v'};
const Q = (p, q) => {
  if (q === 0n) throw {m:'division par zéro'};
  if (q < 0n) { p = -p; q = -q; }
  const g = bgcd(p, q);
  p /= g; q /= g;
  return q === 1n ? Z(p) : {t:'q', v:[p, q]};
};
function bgcd(a, b){ a = a < 0n ? -a : a; b = b < 0n ? -b : b; while (b) { const t = a % b; a = b; b = t; } return a || 1n; }

const NOMTY = {z:'entier', f:'flottant', q:'rationnel', b:'booleen', s:'chaine', l:'liste', v:'vide'};

/* — rendus fidèles au moteur (miroir de ecri() dans rose.cpp) — */
function ftxt(x){
  if (Number.isNaN(x)) return 'non_nombre';
  if (!isFinite(x)) return x > 0 ? 'infini' : '-infini';
  if (Number.isInteger(x) && Math.abs(x) < 1e16) return BigInt(Math.trunc(x)).toString();
  // chiffres les plus courts (round-trip IEEE) + choix fixe/scientifique de printf %g
  const m = Math.abs(x).toExponential().split('e');
  const mant = m[0].replace('.', '').replace(/0+$/, '') || '0';
  const e = parseInt(m[1]);
  const p = mant.length;
  const neg = x < 0 ? '-' : '';
  if (e < -4 || e >= p)
    return neg + mant[0] + (p > 1 ? '.' + mant.slice(1) : '') + 'e'
           + (e >= 0 ? '+' : '-') + (Math.abs(e) < 10 ? '0' : '') + Math.abs(e);
  let d;
  if (e >= 0) { d = mant.padEnd(e + 1, '0');
    d = e + 1 < p ? d.slice(0, e + 1) + '.' + d.slice(e + 1) : d + '.0'; }
  else d = '0.' + '0'.repeat(-e - 1) + mant;
  return neg + d;
}
function qtxt(pr){
  let [p, q] = pr.v;                       // q > 0, fraction réduite
  if (q === 1n) return p.toString();
  if (p.toString().length + q.toString().length > 500) return p.toString() + '/' + q.toString();
  let w = p / q;                           // JS = troncature → plancher (fdiv du moteur)
  if (p % q !== 0n && p < 0n) w -= 1n;
  let r = p < 0n ? -p : p; r %= q;         // le moteur part de |num| mod den
  const vu = []; const pos = [];
  let digits = ''; let peri = -1;
  while (r !== 0n && digits.length < 200) {
    const j = vu.findIndex(x => x === r);
    if (j >= 0) { peri = pos[j]; break; }
    vu.push(r); pos.push(digits.length);
    r *= 10n;
    digits += (r / q).toString();
    r %= q;
  }
  let out = w.toString();
  if (digits.length) {
    out += '.' + digits.slice(0, peri >= 0 ? peri : digits.length);
    if (peri >= 0) out += '[' + digits.slice(peri) + ']';
    else if (digits.length >= 200) out += '…';
  }
  return out;
}
const fmt = v => v.t === 'z' ? v.v.toString()
  : v.t === 'f' ? ftxt(v.v)
  : v.t === 'q' ? qtxt(v)
  : v.t === 'b' ? (v.v ? 'vrai' : 'faux')
  : v.t === 's' ? v.v
  : v.t === 'l' ? '[' + v.v.map(fmt).join(', ') + ']'
  : '';

/* — lexicateur — */
const MOTS = new Set(['fn','si','sinon','tant_que','pour','de','vers','retour','vrai','faux','vide','et','ou','non']);
function lexer(src){
  const T = []; let i = 0, l = 1, c = 1;
  const adv = n => { for (let j = 0; j < n && i < src.length; j++) { if (src[i] === '\n') { l++; c = 1; } else c++; i++; } };
  while (i < src.length) {
    const ch = src[i], L0 = l, C0 = c;
    if (ch === '\n' || ch === ' ' || ch === '\t' || ch === '\r') { adv(1); continue; }
    if (ch === '/' && src[i+1] === '/') { while (i < src.length && src[i] !== '\n') adv(1); continue; }
    if (ch === '$') { adv(1); let s = ''; while (i < src.length && /[\p{L}\p{N}_]/u.test(src[i])) { s += src[i]; adv(1); } if (!s) throw {l:L0,c:C0,m:'nom de variable attendu après « $ »'}; T.push({k:'var', v:s, l:L0, c:C0}); continue; }
    if (/[\p{L}_]/u.test(ch)) { let s = ''; while (i < src.length && /[\p{L}\p{N}_]/u.test(src[i])) { s += src[i]; adv(1); } T.push({k:MOTS.has(s) ? 'mot' : 'nom', v:s, l:L0, c:C0}); continue; }
    if (/\d/.test(ch)) { let s = ''; while (/\d/.test(src[i] || '')) { s += src[i]; adv(1); }
      if (src[i] === '.' && /\d/.test(src[i+1] || '')) { s += src[i]; adv(1); while (/\d/.test(src[i] || '')) { s += src[i]; adv(1); } T.push({k:'flott', v:parseFloat(s), l:L0, c:C0}); }
      else T.push({k:'ent', v:BigInt(s), l:L0, c:C0}); continue; }
    if (ch === '"') { adv(1); let s = ''; while (i < src.length && src[i] !== '"') { if (src[i] === '\\') { const n = src[i+1]; s += (n === 'n' ? '\n' : n === 't' ? '\t' : n); adv(2); } else { s += src[i]; adv(1); } } if (src[i] !== '"') throw {l:L0,c:C0,m:'chaîne non terminée'}; adv(1); T.push({k:'chaine', v:s, l:L0, c:C0}); continue; }
    const d2 = src.substr(i, 2);
    if (['>>','==','!=','<=','>='].includes(d2)) { T.push({k:'op', v:d2, l:L0, c:C0}); adv(2); continue; }
    if ('+-*/%^=<>(),.:;?@≈[]{}'.includes(ch)) { T.push({k:'op', v:ch, l:L0, c:C0}); adv(1); continue; }
    throw {l:L0, c:C0, m:'caractère « ' + ch + ' » non reconnu'};
  }
  T.push({k:'eof', l, c});
  return T;
}

/* — analyseur — */
function parse(tokens){
  let p = 0;
  const at = () => tokens[p], nv = () => tokens[p++];
  const est = (k, v) => tokens[p].k === k && (v === undefined || tokens[p].v === v);
  const opp = (a, b) => { const v = b === undefined ? a : b;
    if (!est('op', v)) throw {l:at().l, c:at().c, m:'« ' + v + ' » attendu'}; return nv(); };
  const tyNom = () => { let n = nv().v; if (est('op','<')) { nv(); n += '<' + tyNom() + '>'; opp('op','>'); } return n; };

  function bloc(fin){
    const ouvr = nv().v, fer = ouvr === '{' ? '}' : ']';
    const ins = [];
    while (!est('op', fer) && !est('eof')) ins.push(instr());
    opp('op', fer); return ins;
  }
  function corps(){
    if (est('op','{')) return bloc();
    if (est('op','[')) return bloc();
    throw {l:at().l, c:at().c, m:'bloc « { … } » attendu'};
  }
  function instr(){
    while (est('op',';')) nv();
    const t = at();
    if (est('op','>>')) { nv(); return {k:'out', e:expr(), l:t.l}; }
    if (est('mot','fn')) {
      nv(); const nom = nv().v; opp('op','(');
      const params = [];
      while (!est('op',')')) {
        const pn = nv().v; opp('op',':'); const pt = tyNom();
        params.push({n:pn, t:pt});
        if (est('op',',')) nv();
      }
      opp('op',')');
      let ret = 'vide';
      if (est('op','-')) { nv(); opp('op','>'); ret = tyNom(); }
      return {k:'fn', nom, params, ret, corps:corps(), l:t.l};
    }
    if (est('mot','si')) { nv(); opp('op','('); const cond = expr(); opp('op',')');
      const bras = [{c:cond, b:corps()}];
      let sinon = null;
      while (est('mot','sinon')) { nv();
        if (est('mot','si')) { nv(); opp('op','('); const c2 = expr(); opp('op',')'); bras.push({c:c2, b:corps()}); }
        else sinon = corps(); }
      return {k:'si', bras, sinon, l:t.l}; }
    if (est('mot','tant_que')) { nv(); opp('op','('); const c = expr(); opp('op',')'); return {k:'tq', c, b:corps(), l:t.l}; }
    if (est('mot','pour')) { nv(); const v = nv().v; if (est('mot','de')) nv(); const de = expr(); if (est('mot','vers')) nv(); const vers = expr(); return {k:'pour', v, de, vers, b:corps(), l:t.l}; }
    if (est('mot','retour')) { nv();
      if (est('op','}') || est('op',']') || est('eof') || est('mot','sinon')) return {k:'ret', e:null, l:t.l};
      return {k:'ret', e:expr(), l:t.l}; }
    if (est('op','?')) { nv(); opp('op','('); const cond = expr(); opp('op',')');
      const bras = [{c:cond, b:corps()}]; let sinon = null;
      if (est('mot','sinon')) { nv(); if (est('op','?')) { nv(); opp('op','('); const c2 = expr(); opp('op',')'); bras.push({c:c2, b:corps()}); } else sinon = corps(); }
      return {k:'si', bras, sinon, l:t.l}; }
    if (est('op','@')) { nv(); opp('op','('); const c = expr(); opp('op',')'); return {k:'tq', c, b:corps(), l:t.l}; }
    if (est('var')) {
      const nom = nv().v;
      if (est('op','[')) { nv(); const ix = expr(); opp('op',']'); opp('op','='); return {k:'affidx', nom, ix, e:expr(), l:t.l}; }
      opp('op','='); return {k:'aff', nom, e:expr(), l:t.l};
    }
    return {k:'expr', e:expr(), l:t.l};
  }
  function expr(){ return ou(); }
  function ou(){ let a = et(); while (est('mot','ou')) { nv(); a = {k:'bin', op:'ou', a, b:et()}; } return a; }
  function et(){ let a = nonN(); while (est('mot','et')) { nv(); a = {k:'bin', op:'et', a, b:nonN()}; } return a; }
  function nonN(){ if (est('mot','non')) { nv(); return {k:'non', a:nonN()}; } return cmp(); }
  function cmp(){ let a = add();
    for (;;) { const t = at();
      if (t.k==='op' && ['==','!=','<','<=','>','>=','≈'].includes(t.v)) { nv(); a = {k:'bin', op:t.v, a, b:add(), l:t.l}; } else return a; } }
  function add(){ let a = mul();
    for (;;) { const t = at();
      if (t.k==='op' && (t.v==='+'||t.v==='-')) { nv(); a = {k:'bin', op:t.v, a, b:mul(), l:t.l}; } else return a; } }
  function mul(){ let a = powL();
    for (;;) { const t = at();
      if (t.k==='op' && ['*','/','%'].includes(t.v)) { nv(); a = {k:'bin', op:t.v, a, b:powL(), l:t.l}; }
      else if ((t.k==='mot'||t.k==='nom') && t.v==='div') { nv(); a = {k:'bin', op:'div', a, b:powL(), l:t.l}; }
      else return a; } }
  /* le unaire est plus serré que ^ : « -2 ^ 2 » rend 4, comme l'oracle */
  function powL(){ let a = unaire();
    if (est('op','^')) { nv(); return {k:'bin', op:'^', a, b:powExp()}; } return a; }
  function unaire(){ if (est('op','-')) { nv(); return {k:'uni', op:'-', a:unaire()}; }
    if (est('mot','non')) { nv(); return {k:'non', a:unaire()}; } return postfix(); }
  function powExp(){ if (est('op','-')) { nv(); return {k:'uni', op:'-', a:powExp()}; }
    const a = postfix();
    if (est('op','^')) { nv(); return {k:'bin', op:'^', a, b:powExp()}; } return a; }
  function postfix(){ let a = prim();
    for (;;) {
      if (est('op','[')) { nv();
        if (est('op',':')) { nv(); const b = expr(); opp('op',']'); a = {k:'tr', a, de:null, vers:b}; continue; }
        const i1 = expr();
        if (est('op',':')) { nv(); if (est('op',']')) { nv(); a = {k:'tr', a, de:i1, vers:null}; continue; } const i2 = expr(); opp('op',']'); a = {k:'tr', a, de:i1, vers:i2}; continue; }
        opp('op',']'); a = {k:'idx', a, i:i1}; continue; }
      return a; } }
  function prim(){
    const t = nv();
    if (t.k === 'ent') return {k:'lit', v:Z(t.v)};
    if (t.k === 'flott') return {k:'lit', v:F(t.v)};
    if (t.k === 'chaine') return {k:'lit', v:S(t.v)};
    if (t.k === 'mot' && t.v === 'vrai') return {k:'lit', v:B(true)};
    if (t.k === 'mot' && t.v === 'faux') return {k:'lit', v:B(false)};
    if (t.k === 'mot' && t.v === 'vide') return {k:'lit', v:VID};
    if (t.k === 'var' || t.k === 'nom') {
      if (est('op','(')) { nv(); const args = [];
        while (!est('op',')')) { args.push(expr()); if (est('op',',')) nv(); }
        opp('op',')'); return {k:'app', nom:t.v, args, l:t.l}; }
      return {k:'var', nom:t.v, l:t.l}; }
    if (t.k === 'op' && t.v === '(') { const e = expr(); opp('op',')'); return e; }
    if (t.k === 'op' && t.v === '[') { const el = [];
      while (!est('op',']')) { el.push(expr()); if (est('op',',')) nv(); }
      opp('op',']'); return {k:'liste', el}; }
    throw {l:t.l, c:t.c, m:'expression attendue'};
  }
  const prog = [];
  while (!est('eof')) prog.push(instr());
  return prog;
}

/* — évaluateur — */
let steps = 0, profondeur = 0;
const PAS = Symbol('retour'), FIN = Symbol('quitter');
function errE(l, m){ const e = new Error(m); e.rose = true; throw e; }            // runtime : le moteur ne donne pas de position
function errS(l, m){ const e = new Error(m); e.statique = true; e.ligne = l; e.rose = true; throw e; } // contrôle : « [Ligne N, Colonne 0] »
function val(l, nom, attendu){ errS(l, nom + ' : ' + attendu + ' est attendue'); }

function numTo(x, cible){ // conversions numériques exactes
  if (x.t === cible) return x;
  if (cible === 'f') return F(toF(x));
  if (cible === 'q' && x.t === 'z') return Q(x.v, 1n);
  errE(null, 'conversion ' + NOMTY[x.t] + ' → ' + NOMTY[cible] + ' impossible ici');
}
const toF = x => x.t === 'f' ? x.v
  : x.t === 'z' ? Number(x.v)
  : x.t === 'q' ? Number(x.v[0]) / Number(x.v[1])
  : x.t === 'b' ? (x.v ? 1 : 0)
  : errE(null, 'conversion vers flottant impossible');

function binop(op, a, b, l){
  if (op === 'et' || op === 'ou') {
    if (a.t !== 'b' || b.t !== 'b') errE(l, '« ' + op + ' » exige des booleens');
    return B(op === 'et' ? (a.v && b.v) : (a.v || b.v));
  }
  if (op === '+' && a.t === 's' && b.t === 's') return S(a.v + b.v);
  const num = ['z','q','f'];
  if (['+','-','*','/','^','div','%'].includes(op) && !(num.includes(a.t) && num.includes(b.t)))
    errE(l, '« ' + op + ' » ne sait pas opérer sur « ' + NOMTY[a.t] + ' » et « ' + NOMTY[b.t] + ' »');
  const zero = x => (x.t==='z' && x.v===0n) || (x.t==='q' && x.v[0]===0n) || (x.t==='f' && x.v===0);
  if ((op==='/'||op==='div'||op==='%') && zero(b))
    errE(l, op === 'div' ? 'division euclidienne par zéro' : op === '%' ? 'modulo par zéro' : 'division par zéro');
  if (op === '^') {
    if (a.t === 'f' || b.t === 'f') return F(Math.pow(toF(a), toF(b)));
    if (b.t === 'z') {
      const e = b.v;
      if (e > 100000n || e < -100000n) errE(l, 'exposant trop grand pour le moteur navigateur');
      if (a.t === 'z') {
        if (e >= 0n) return Z(a.v ** e);
        if (a.v === 0n) errE(l, 'division par zéro');
        return Q(1n, a.v ** (-e)); // la normalisation de Q reporte le signe
      }
      if (a.t === 'q') {
        const [p, q] = a.v;
        if (e >= 0n) return Q(p ** e, q ** e);
        return Q(q ** (-e), p ** (-e)); // BigInt gère les bases négatives
      }
    }
    return F(Math.pow(toF(a), toF(b)));
  }
  if (op === '/') {
    if (a.t === 'z' && b.t === 'z') return Q(a.v, b.v);
    if (a.t === 'q' && b.t === 'z') return Q(a.v[0], a.v[1] * b.v);
    if (a.t === 'z' && b.t === 'q') return Q(a.v * b.v[1], b.v[0]);
    if (a.t === 'q' && b.t === 'q') return Q(a.v[0] * b.v[1], a.v[1] * b.v[0]);
    const r = toF(a) / toF(b);
    if (!isFinite(r)) errE(l, 'division par zéro');
    return F(r);
  }
  if (op === 'div' || op === '%') {
    if (a.t === 'z' && b.t === 'z') {
      let d = a.v / b.v;
      if (a.v % b.v !== 0n && (a.v < 0n) !== (b.v < 0n)) d -= 1n;
      return op === 'div' ? Z(d) : Z(a.v - d * b.v);
    }
    const fl = Math.floor(toF(a) / toF(b));
    return op === 'div' ? Z(BigInt(fl)) : F(toF(a) - fl * toF(b));
  }
  const f = a.t === 'f' || b.t === 'f';
  if (f) { const x = toF(a), y = toF(b);
    return F(op === '+' ? x + y : op === '-' ? x - y : x * y); }
  const za = a.t === 'z' ? a.v : a.v[0], qa = a.t === 'z' ? 1n : a.v[1];
  const zb = b.t === 'z' ? b.v : b.v[0], qb = b.t === 'z' ? 1n : b.v[1];
  if (op === '+') return Q(za * qb + zb * qa, qa * qb);
  if (op === '-') return Q(za * qb - zb * qa, qa * qb);
  return Q(za * zb, qa * qb);
}
const ESTNUM = t => t === 'z' || t === 'q' || t === 'f';
function cmpop(op, a, b, l){
  if (!((ESTNUM(a.t) && ESTNUM(b.t)) || (a.t === 's' && b.t === 's') || (a.t === 'b' && b.t === 'b')))
    errS(l, 'Ordre ' + NOMTY[a.t] + ' ' + op + ' ' + NOMTY[b.t] + ' impossible');
  if (a.t === 's' && b.t === 's') { const r = a.v < b.v ? -1 : a.v > b.v ? 1 : 0;
    return B(op==='=='?r===0:op==='!='?r!==0:op==='<'?r<0:op==='<='?r<=0:op==='>'?r===1:r>=0); }
  if (a.t === 'b' && b.t === 'b' && (op==='<'||op==='>'||op==='<='||op==='>=')) errS(l, 'Ordre booleen ' + op + ' booleen impossible');
  if (a.t === 'b' && b.t === 'b') {
    return B(op==='==' ? a.v===b.v : a.v!==b.v); }
  if (op === '≈') { const x = toF(a), y = toF(b);
    return B(x === y || Math.abs(x - y) <= 1e-9 * Math.max(1, Math.abs(x), Math.abs(y))); }
  let r;
  if (a.t==='f' || b.t==='f') { const x = toF(a), y = toF(b); r = x < y ? -1 : x > y ? 1 : (Number.isNaN(x)||Number.isNaN(y)) ? NaN : 0; }
  else { const za = a.t==='z'?a.v:a.v[0], qa = a.t==='z'?1n:a.v[1];
         const zb = b.t==='z'?b.v:b.v[0], qb = b.t==='z'?1n:b.v[1];
         const d = za*qb - zb*qa; r = d < 0n ? -1 : d > 0n ? 1 : 0; }
  if (Number.isNaN(r)) errE(l, 'comparaison avec non_nombre sans issue');
  return B(op==='=='?r===0:op==='!='?r!==0:op==='<'?r===-1:op==='<='?r<=0:op==='>'?r===1:r>=1);
}

const FONCTIONS = new Map(); // fn Roses
const STD = new Set(['longueur','sous_chaine','ordre','caractere','joindre','concatener',
  'pgcd','ppcm','factorielle','fibonacci','est_premier','compter_premiers','nb_chiffres',
  'puissance','partie_entiere','fraction','abs','min','max','entier','flottant','chaine',
  'afficher','quitter']);
const BLOQUES = new Set(['table','inserer','cle_existe','supprimer','cles','valeurs','temps','duree',
  'millisecondes','hasard','graine','saisir_entier','saisir_flottant','lire_texte','ecrire_texte',
  'nb_arguments','argument','tampon','octets','nombre_pi','lire','ecrire','tampon_case']);

function builtin(nom, args, l){
  const un = t => { if (args.length !== 1) errE(l, nom + ' attend 1 argument'); return args[0]; };
  if (BLOQUES.has(nom)) errE(l, '« ' + nom + ' » demande le moteur installé — installe Rose (rubrique Installer)');
  switch (nom) {
    case 'longueur': { const v = un();
      if (v.t === 's') return Z(BigInt(Array.from(v.v).length));
      if (v.t === 'l') return Z(BigInt(v.v.length));
      return val(l, nom, 'chaine ou liste'); }
    case 'sous_chaine': { const [s,a,b] = args; if (!s || s.t!=='s' || !a || a.t!=='z' || !b || b.t!=='z') return val(l, nom, 'chaine, entier, entier');
      const cs = Array.from(s.v); let x = Number(a.v), y = Number(b.v);
      x = Math.max(0, Math.min(cs.length, x)); y = Math.max(x, Math.min(cs.length, y));
      return S(cs.slice(x, y).join('')); }
    case 'ordre': { const v = un(); if (v.t !== 's' || !v.v) return val(l, nom, 'chaine d’un caractère');
      return Z(BigInt(v.v.codePointAt(0))); }
    case 'caractere': { const v = un(); if (v.t !== 'z') return val(l, nom, 'entier');
      return S(String.fromCodePoint(Number(v.v))); }
    case 'joindre': { const [lst, x] = args; if (!lst || lst.t !== 'l' || x === undefined) return val(l, nom, 'liste, tout');
      lst.v.push(x); return VID; }
    case 'concatener': { const v = un(); if (v.t !== 'l') return val(l, nom, 'liste');
      for (const e of v.v) if (e.t !== 's') return val(l, nom, 'liste de chaines');
      return S(v.v.map(e => e.v).join('')); }
    case 'pgcd': { const [a, b] = args; if (!a || a.t!=='z' || !b || b.t!=='z') return val(l, nom, 'entier, entier');
      return Z(bgcd(a.v, b.v)); }
    case 'ppcm': { const [a, b] = args; if (!a || a.t!=='z' || !b || b.t!=='z') return val(l, nom, 'entier, entier');
      if (a.v === 0n || b.v === 0n) return Z(0n);
      const g = bgcd(a.v, b.v); const m = (a.v / g) * b.v;
      return Z(m < 0n ? -m : m); }
    case 'factorielle': { const v = un(); if (v.t !== 'z') return val(l, nom, 'entier');
      if (v.v < 0n) errE(l, "factorielle d'un négatif : impossible");
      if (v.v > 20000n) errE(l, 'factorielle trop grande pour le moteur navigateur');
      let r = 1n; for (let k = 2n; k <= v.v; k++) r *= k; return Z(r); }
    case 'fibonacci': { const v = un(); if (v.t !== 'z') return val(l, nom, 'entier');
      if (v.v < 0n) errE(l, "fibonacci d'un négatif : impossible");
      if (v.v > 100000n) errE(l, 'fibonacci trop grand pour le moteur navigateur');
      let a = 0n, b = 1n; for (let k = 0n; k < v.v; k++) { const c = a + b; a = b; b = c; } return Z(a); }
    case 'est_premier': { const v = un(); if (v.t !== 'z') return val(l, nom, 'entier');
      let n = v.v; if (n < 2n) return B(false); if (n % 2n === 0n) return B(n === 2n);
      for (let d = 3n; d * d <= n; d += 2n) if (n % d === 0n) return B(false); return B(true); }
    case 'compter_premiers': { const v = un(); if (v.t !== 'z') return val(l, nom, 'entier');
      if (v.v > 2000000n) errE(l, 'trop grand pour le moteur navigateur');
      const n = Number(v.v); const crib = new Uint8Array(n + 1); let cpt = 0;
      for (let i = 2; i <= n; i++) if (!crib[i]) { cpt++; for (let j = i * 2; j <= n; j += i) crib[j] = 1; } return Z(BigInt(cpt)); }
    case 'nb_chiffres': { const v = un(); if (v.t !== 'z') return val(l, nom, 'entier');
      return Z(BigInt((v.v < 0n ? -v.v : v.v).toString().length)); }
    case 'puissance': { const [a, e] = args; if (!a || !e || e.t !== 'z') return val(l, nom, 'nombre, entier');
      return binop('^', a, e, l); }
    case 'partie_entiere': { const v = un();
      if (v.t === 'z') return v;
      if (v.t === 'q') { let [p, q] = v.v; let d = p / q; if (p % q !== 0n && (p < 0n) !== (q < 0n)) d -= 1n; return Z(d); }
      if (v.t === 'f') return Z(BigInt(Math.floor(v.v)));
      return val(l, nom, 'nombre'); }
    case 'fraction': { const [p, q] = args; if (!p || p.t!=='z' || !q || q.t!=='z') return val(l, nom, 'entier, entier');
      return Q(p.v, q.v); }
    case 'abs': { const v = un();
      if (v.t === 'z') return Z(v.v < 0n ? -v.v : v.v);
      if (v.t === 'q') return Q(v.v[0] < 0n ? -v.v[0] : v.v[0], v.v[1]);
      if (v.t === 'f') return F(Math.abs(v.v));
      return val(l, nom, 'nombre'); }
    case 'min': case 'max': { const [a, b] = args; if (!a || !b) errE(l, nom + ' attend 2 arguments');
      const estMin = nom === 'min';
      const c = cmpv(a, b);
      return estMin ? (c <= 0 ? a : b) : (c >= 0 ? a : b); }
    case 'entier': { const v = un();
      if (v.t === 'z') return v;
      if (v.t === 'f') { if (!isFinite(v.v)) errE(l, 'conversion vers entier impossible'); return Z(BigInt(Math.trunc(v.v))); }
      if (v.t === 'q') { let [p, q] = v.v; let d = p / q; if (p % q !== 0n && (p < 0n) !== (q < 0n)) d -= 1n; return Z(d); }
      if (v.t === 'b') return Z(v.v ? 1n : 0n);
      if (v.t === 's') { try { return Z(BigInt(v.v.trim())); } catch(e){
          const f = parseFloat(v.v); if (!Number.isNaN(f)) return Z(BigInt(Math.trunc(f))); }
        errE(l, 'conversion chaine → entier impossible'); }
      return val(l, nom, 'de quoi convertir'); }
    case 'flottant': { const v = un();
      if (v.t === 's') { const f = parseFloat(v.v);
        if (Number.isNaN(f)) { if (v.v === 'infini') return F(Infinity); if (v.v === '-infini') return F(-Infinity); errE(l, 'conversion chaine → flottant impossible'); }
        return F(f); }
      return F(toF(v)); }
    case 'chaine': { const v = un(); return S(cht(v)); }
    case 'afficher': { SORTIE.push(fmt(un())); return VID; }
    case 'quitter': throw { [FIN]: true, code: args[0] && args[0].t === 'z' ? Number(args[0].v) : 0 };
  }
  errS(l, "Fonction '" + nom + "' inconnue");
}
/* chaine() du moteur = chaine_de() : le rationnel y rend « p/q », pas la période */
function cht(v){
  return v.t === 'z' ? v.v.toString()
    : v.t === 'f' ? ftxt(v.v)
    : v.t === 'q' ? v.v[0].toString() + '/' + v.v[1].toString()
    : v.t === 'b' ? (v.v ? 'vrai' : 'faux')
    : v.t === 's' ? v.v
    : v.t === 'l' ? '[' + v.v.map(cht).join(', ') + ']'
    : '';
}
function cmpv(a, b){
  if (a.t==='f'||b.t==='f') { const x = toF(a), y = toF(b); return x<y?-1:x>y?1:0; }
  const za = a.t==='z'?a.v:a.v[0], qa = a.t==='z'?1n:a.v[1];
  const zb = b.t==='z'?b.v:b.v[0], qb = b.t==='z'?1n:b.v[1];
  const d = za*qb - zb*qa; return d<0n?-1:d>0n?1:0;
}

let SORTIE = null;
function evn(n, env){ // valeur d'expression
  if (++steps > 5e6) throw {rose:true, message:'exécution interrompue : trop d’étapes (moteur navigateur)'};
  switch (n.k) {
    case 'lit': return n.v;
    case 'var': { if (!env.has(n.nom)) errS(n.l, "Variable '" + n.nom + "' utilisée sans valeur"); return env.get(n.nom).v; }
    case 'liste': return L(n.el.map(e => evn(e, env)));
    case 'non': { const a = evn(n.a, env); if (a.t !== 'b') errE(n.l ?? 0, '« non » exige un booleen'); return B(!a.v); }
    case 'uni': { const a = evn(n.a, env);
      if (!['z','q','f'].includes(a.t)) errE(0, 'unaire « - » sur « ' + NOMTY[a.t] + ' » impossible');
      return a.t==='z' ? Z(-a.v) : a.t==='q' ? Q(-a.v[0], a.v[1]) : F(-a.v); }
    case 'bin': { const a = evn(n.a, env), b = evn(n.b, env);
      return ['==','!=','<','<=','>','>=','≈'].includes(n.op) ? cmpop(n.op, a, b, n.l) : binop(n.op, a, b, n.l); }
    case 'idx': { const base = evn(n.a, env), i = evn(n.i, env);
      if (i.t !== 'z') errE(n.l, 'index entier attendu');
      if (base.t === 's') { const cs = Array.from(base.v); const k = Number(i.v);
        if (k < 0 || k >= cs.length) errE(n.l, 'index hors des limites de la chaîne'); return S(cs[k]); }
      if (base.t === 'l') { const k = Number(i.v);
        if (k < 0 || k >= base.v.length) errE(n.l, 'index hors des limites de la liste'); return base.v[k]; }
      errE(n.l, 'indexation impossible sur « ' + NOMTY[base.t] + ' »'); return null; }
    case 'tr': { const base = evn(n.a, env);
      const n0 = n.de ? evn(n.de, env) : Z(0n), n1 = n.vers ? evn(n.vers, env) : null;
      if (n0.t !== 'z' || (n1 && n1.t !== 'z')) errE(n.l, 'bornes entières attendues');
      if (base.t === 's') { const cs = Array.from(base.v);
        let x = Number(n0.v), y = n1 ? Number(n1.v) : cs.length;
        x = Math.max(0, Math.min(cs.length, x)); y = Math.max(x, Math.min(cs.length, y));
        return S(cs.slice(x, y).join('')); }
      if (base.t === 'l') { let x = Number(n0.v), y = n1 ? Number(n1.v) : base.v.length;
        x = Math.max(0, Math.min(base.v.length, x)); y = Math.max(x, Math.min(base.v.length, y));
        return L(base.v.slice(x, y)); }
      errE(n.l, 'troncature impossible sur « ' + NOMTY[base.t] + ' »'); return null; }
    case 'app': {
      const args = n.args.map(a => evn(a, env));
      if (FONCTIONS.has(n.nom)) return appeler(FONCTIONS.get(n.nom), args, n.l);
      if (BLOQUES.has(n.nom)) errE(n.l, '« ' + n.nom + ' » demande le moteur installé — installe Rose (rubrique Installer)');
      if (!STD.has(n.nom)) errS(n.l, "Fonction '" + n.nom + "' inconnue");
      return builtin(n.nom, args, n.l); }
  }
  throw {rose:true, message:'nœud inconnu'};
}
const TYOK = {entier:'z', flottant:'f', rationnel:'q', booleen:'b', chaine:'s', liste:'l', vide:'v'};
function typeOK(v, ty){
  if (ty === 'tout' || ty === 'num') return true;
  const base = ty.split('<')[0];
  if (base === 'liste' || base === 'table') return NOMTY[v.t] === (base === 'liste' ? 'liste' : 'table');
  return TYOK[base] === v.t;
}
function appeler(f, args, l){
  if (args.length !== f.params.length) errE(l, '« ' + f.nom + ' » attend ' + f.params.length + ' argument(s)');
  if (++profondeur > 1400) throw {rose:true, message:'récursion trop profonde (moteur navigateur)'};
  const env = new Map();
  f.params.forEach((p, i) => {
    if (!typeOK(args[i], p.t)) errE(l, 'type inattendu pour le paramètre « ' + p.n + ' » de « ' + f.nom + ' » — attendu ' + p.t);
    env.set(p.n, {v:args[i]});
  });
  try { execList(f.corps, env); }
  catch (r) { if (r && r[PAS] !== undefined) { profondeur--; return r[PAS]; } profondeur--; throw r; }
  profondeur--;
  return VID;
}
function setVar(env, nom, v, l){
  if (env.has(nom)) {
    const an = env.get(nom).v;
    const OK = { z:['z'], q:['z','q'], f:['z','f','q'], b:['b'], s:['s'], l:['l'], v:null };
    const allowed = OK[an.t];
    if (allowed && !allowed.includes(v.t))
      errS(l, "'" + nom + "' est " + NOMTY[an.t] + ", on ne peut pas y stocker du " + NOMTY[v.t]);
  }
  env.set(nom, {v});
}
function execList(ins, env){ for (const i of ins) exec(i, env); }
function exec(n, env){
  if (++steps > 5e6) throw {rose:true, message:'exécution interrompue : trop d’étapes (moteur navigateur)'};
  switch (n.k) {
    case 'fn': FONCTIONS.set(n.nom, n); return;
    case 'aff': setVar(env, n.nom, evn(n.e, env), n.l); return;
    case 'affidx': { const base = env.has(n.nom) ? env.get(n.nom).v : errS(n.l, "Variable '" + n.nom + "' utilisée sans valeur");
      const i = evn(n.ix, env);
      if (base.t !== 'l' || i.t !== 'z') errE(n.l, 'affectation par index permise ici sur liste seulement');
      const k = Number(i.v); if (k < 0 || k >= base.v.length) errE(n.l, 'index hors des limites de la liste');
      base.v[k] = evn(n.e, env); return; }
    case 'out': SORTIE.push(fmt(evn(n.e, env))); return;
    case 'expr': evn(n.e, env); return;
    case 'ret': throw { [PAS]: n.e ? evn(n.e, env) : VID };
    case 'si': {
      for (const b of n.bras) { const c = evn(b.c, env);
        if (c.t !== 'b') errS(n.l, 'La condition de si doit être booléenne, trouvé ' + NOMTY[c.t]);
        if (c.v) return execList(b.b, env); }
      if (n.sinon) execList(n.sinon, env); return; }
    case 'tq': {
      for (;;) { const c = evn(n.c, env);
        if (c.t !== 'b') errS(n.l, 'La condition de tant_que doit être booléenne, trouvé ' + NOMTY[c.t]);
        if (!c.v) break; execList(n.b, env); }
      return; }
    case 'pour': {
      let c = evn(n.de, env);
      const fin = evn(n.vers, env);
      if (c.t !== 'z' || fin.t !== 'z') errE(n.l, 'bornes entières attendues pour « pour »');
      for (; c.v < fin.v; c = Z(c.v + 1n)) { env.set(n.v, {v:c}); execList(n.b, env); }
      return; }
  }
}

/* — fils rouges de la console — */
function logL(classe, msg){
  const out = document.getElementById('consoleOut');
  const d = document.createElement('div');
  d.className = 'log-line ' + classe;
  d.textContent = msg;
  out.appendChild(d);
  out.scrollTop = out.scrollHeight;
}
function clearConsole(){ document.getElementById('consoleOut').innerHTML = ''; }
function runRose(){
  const out = document.getElementById('consoleOut');
  out.innerHTML = '';
  const src = document.getElementById('codeEditor').value;
  SORTIE = []; FONCTIONS.clear(); steps = 0; profondeur = 0;
  const t0 = performance.now();
  try {
    const prog = parse(lexer(src));
    const env = new Map();
    execList(prog, env);
    SORTIE.forEach(l => logL('', l));
    if (!SORTIE.length) logL('log-info', '(aucune sortie)');
    logL('log-ok', '— terminé en ' + (performance.now() - t0).toFixed(1) + ' ms —');
  } catch (r) {
    if (r && r[FIN]) { logL('log-info', '— programme arrêté, code ' + r.code + ' —'); return; }
    SORTIE.forEach(l => logL('', l));
    const pos = r.statique ? '[Ligne ' + r.ligne + ', Colonne 0] '
      : (r.l !== undefined && r.l !== null ? '[Ligne ' + r.l + ', Colonne ' + (r.c || 0) + '] ' : '');
    logL('log-err', 'Rose : erreur — ' + pos + (r.message || r.m || String(r)));
  }
}
function switchTab(f){
  document.querySelectorAll('#page-play .editor-tabs .tab').forEach(t => t.classList.toggle('active', t.dataset.file === f));
  document.getElementById('fileName').textContent = f + '.rose';
  document.getElementById('codeEditor').value = EXEMPLES[f];
}
function resetCode(){ const actif = document.querySelector('#page-play .editor-tabs .tab.active'); switchTab(actif ? actif.dataset.file : 'bonjour'); toast('Code réinitialisé'); }
document.getElementById('codeEditor').value = EXEMPLES.bonjour;
