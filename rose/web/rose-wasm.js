// Rose 2.0 — glue JavaScript pour le module WebAssembly Rose.
//
// Ce fichier ne dépend d'AUCUNE bibliothèque externe (pas de wasm-bindgen,
// pas de wasm-pack) : il instancie `rose.wasm` et appelle directement
// l'interface C ABI exportée par le moteur.
//
// API
// ---
//
//   await Rose.ready()                 // charge le module (une seule fois)
//   await Rose.execute(source)         // → sortie du programme (texte)
//   await Rose.latex(source)           // → formule LaTeX du programme
//   await Rose.analyse(source)         // → résumé de l'analyse
//   Rose.version()                     // → version du moteur
//
// Tout est asynchrone : `ready()` doit être attendu avant le premier appel
// (les appels `execute`/`latex`/`analyse` l'attendent eux-mêmes).
//
// L'interface C ABI (voir src/wasm.rs) :
//
//   rose_alloc(len)                → pointeur
//   rose_free(ptr, len)            → libération
//   rose_execute(ptr, len)         → tampon « u32 LE (longueur) + UTF-8 »
//   rose_execute_latex(ptr, len)   → idem
//   rose_analyse(ptr, len)         → idem
//   rose_version()                 → idem
//   rose_free_buffer(ptr)          → libération d'un tampon à en-tête
//   rose_prendre_sortie()          → sortie capturée (lecture après un arrêt)
//   rose_dernier_panic()           → message de la dernière panique
//
// ARRÊT DU MODULE : `wasm32-unknown-unknown` impose `panic = "abort"`.
// Une erreur runtime Rose (« ROSE_ERR:… ») arrête donc le module au lieu de
// remonter une exception : le glue attrape le piège (`WebAssembly.RuntimeError`),
// relit le message via `rose_dernier_panic()` et la sortie partielle via
// `rose_prendre_sortie()`, puis rend un texte « Rose : erreur — … ».
//
// Utilisation dans une page HTML :
//
//     <script type="module">
//       import Rose from './rose-wasm.js';
//       Rose.configure({ url: './rose.wasm' });
//       await Rose.ready();
//       document.body.textContent = await Rose.execute('>> 1 + 1');
//     </script>
//
// Licence MIT.

const Rose = (() => {
  'use strict';

  let instance = null;
  let chargement = null;          // Promise d'instanciation en cours
  let urlWasm = 'rose.wasm';      // URL du module (relative à la page)
  let octetsWasm = null;          // ou contenu binaire déjà chargé

  const encodeur = new TextEncoder();
  const decodeur = new TextDecoder('utf-8');

  /** Configure le module à charger. */
  function configure(options) {
    options = options || {};
    if (options.url) urlWasm = options.url;
    if (options.bytes) octetsWasm = options.bytes;
    if (options.instance) { instance = options.instance; chargement = Promise.resolve(instance); }
    return Rose;
  }

  /** Charge (une seule fois) et instancie le module WASM. */
  async function charger() {
    if (instance) return instance;
    if (chargement) return chargement;
    chargement = (async () => {
      let resultat;
      if (octetsWasm) {
        resultat = await WebAssembly.instantiate(octetsWasm, {});
      } else if (typeof WebAssembly.instantiateStreaming === 'function' && typeof fetch === 'function') {
        // Navigateur : streaming (le .wasm est compilé à la volée par le
        // navigateur). On retombe sur `instantiate` si le serveur ne sert
        // pas le bon Content-Type.
        const reponse = await fetch(urlWasm);
        if (!reponse.ok) {
          throw new Error(`Rose : impossible de charger ${urlWasm} (HTTP ${reponse.status})`);
        }
        try {
          resultat = await WebAssembly.instantiateStreaming(reponse, {});
        } catch (e) {
          const tampon = await reponse.arrayBuffer();
          resultat = await WebAssembly.instantiate(tampon, {});
        }
      } else {
        // Node.js (pas de `fetch` de fichier) : le fichier est fourni par
        // l'appelant via `configure({ bytes })`.
        throw new Error('Rose : aucun module WASM fourni (utilisez Rose.configure({ url, bytes }))');
      }
      instance = resultat.instance;
      return instance;
    })();
    try {
      return await chargement;
    } catch (e) {
      chargement = null;
      throw e;
    }
  }

  /** Attend que le module soit prêt. */
  async function ready() {
    return await charger();
  }

  /** Instance WASM courante (null avant `ready()`). */
  function getInstance() { return instance; }

  // ── Tampons ──────────────────────────────────────────────────────────────

  /** Écrit le source dans la mémoire du module et rend { ptr, len }. */
  function ecrireSource(source) {
    const octets = encodeur.encode(String(source));
    const ptr = instance.exports.rose_alloc(octets.length);
    // La mémoire peut avoir grandi : on relit le tampon à chaque fois.
    new Uint8Array(instance.exports.memory.buffer, ptr, octets.length).set(octets);
    return { ptr: ptr, len: octets.length };
  }

  /** Lit un tampon « u32 LE (longueur) + octets UTF-8 » et le libère. */
  function lireTampon(ptr) {
    if (!ptr) return '';
    const mem = instance.exports.memory.buffer;
    const n = new DataView(mem, ptr, 4).getUint32(0, true);
    const texte = decodeur.decode(new Uint8Array(mem, ptr + 4, n));
    instance.exports.rose_free_buffer(ptr);
    return texte;
  }

  /** Appelle une fonction `rose_*` qui rend un tampon, en capturant un
   *  éventuel arrêt du module (panic = abort en WASM). */
  async function appeler(nom, source, prefixeErreur) {
    await ready();
    const ec = ecrireSource(source);
    try {
      return lireTampon(instance.exports[nom](ec.ptr, ec.len));
    } catch (e) {
      // Le module s'est arrêté : on relit le message d'erreur et la sortie
      // partielle, puis on rend un texte au lieu de propager l'exception.
      let partiel = '';
      let message = '';
      try { partiel = lireTampon(instance.exports.rose_prendre_sortie()); } catch (e2) { /* ignoré */ }
      try { message = lireTampon(instance.exports.rose_dernier_panic()); } catch (e3) { /* ignoré */ }
      if (message) {
        const propre = message.replace(/^ROSE_ERR:/, '');
        return partiel + (prefixeErreur || 'Rose : erreur — ') + propre + '\n';
      }
      throw e;
    } finally {
      try { instance.exports.rose_free(ec.ptr, ec.len); } catch (e4) { /* ignoré */ }
    }
  }

  // ── API publique ─────────────────────────────────────────────────────────

  /** Exécute un programme Rose et rend sa sortie (stdout). */
  async function execute(source) {
    return await appeler('rose_execute', source, 'Rose : erreur — ');
  }

  /** Rend le programme Rose sous forme LaTeX. */
  async function latex(source) {
    return await appeler('rose_execute_latex', source, 'Erreur — ');
  }

  /** Analyse un programme Rose sans l'exécuter. */
  async function analyse(source) {
    return await appeler('rose_analyse', source, 'Rose : erreur — ');
  }

  /** Version du moteur. */
  function version() {
    if (!instance) return '';
    return lireTampon(instance.exports.rose_version());
  }

  return {
    configure: configure,
    charger: charger,
    ready: ready,
    execute: execute,
    latex: latex,
    analyse: analyse,
    version: version,
    get instance() { return instance; }
  };
})();

// Utilisable comme module ES (import) et comme global (window.Rose).
if (typeof globalThis !== 'undefined') globalThis.Rose = Rose;

export default Rose;
