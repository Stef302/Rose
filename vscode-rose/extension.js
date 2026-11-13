const vscode = require('vscode');
const cp = require('child_process');
const fs = require('fs');
const os = require('os');
const path = require('path');

const KEYWORDS = ['si', 'sinon', 'tant_que', 'pour', 'de', 'vers', 'fn', 'retour', 'et', 'ou', 'non', 'vrai', 'faux', 'vide'];
const TYPES = ['entier', 'rationnel', 'flottant', 'booleen', 'chaine', 'complexe', 'irrationnel', 'liste', 'table', 'tableau', 'tampon', 'vide'];

const STDLIB = {
  'abs': { sig: 'abs(n)', doc: 'Valeur absolue' },
  'min': { sig: 'min(a, b)', doc: 'Minimum' },
  'max': { sig: 'max(a, b)', doc: 'Maximum' },
  'partie_entiere': { sig: 'partie_entiere(x)', doc: 'Partie entière' },
  'factorielle': { sig: 'factorielle(n)', doc: 'Factorielle exacte (multi-cœur, facteurs premiers)' },
  'fact': { sig: 'fact(n)', doc: 'Factorielle exacte' },
  'factorielle_rapide': { sig: 'factorielle_rapide(n)', doc: 'Factorielle exacte rapide — 1000000! en ~0,9 s (alias de l’opérateur `!!`)' },
  'rapide': { sig: 'rapide(n)', doc: 'Factorielle rapide (même chose que `n!!`)' },
  'approx': { sig: 'approx(n)', doc: 'Factorielle approximative par Stirling complet — renvoie une chaîne en notation scientifique (alias de `n!?`)' },
  'view': { sig: 'view(x)', doc: 'Visionneuse paginée façon nano pour les très grands nombres (Espace = page suivante, b = retour, q = quitter)' },
  'factorial_rapide': { sig: 'factorial_rapide(n)', doc: 'Factorielle rapide (nom anglais, même chose que `n!!`)' },
  'factorial_approx': { sig: 'factorial_approx(n)', doc: 'Factorielle approchée (nom anglais, même chose que `n!?`)' },
  'type_de': { sig: 'type_de(x)', doc: 'Nom du type d’une valeur (nom complet de `type`)' },
  'nombre_pi': { sig: 'nombre_pi(n)', doc: 'n décimales de π' },
  'nombre_e': { sig: 'nombre_e(n)', doc: 'n décimales de e' },
  'tout': { sig: 'tout(tableau)', doc: 'Vrai si tous les éléments sont vrais' },
  'saisir_entier': { sig: 'saisir_entier()', doc: 'Lit un entier au clavier' },
  'saisir_flottant': { sig: 'saisir_flottant()', doc: 'Lit un flottant au clavier' },
  'graine_systeme': { sig: 'graine_systeme()', doc: 'Amorce le PRNG depuis l’entropie système' },
  'graine_vrai': { sig: 'graine_vrai()', doc: 'Amorce le PRNG depuis une source vraie' },
  'ecrire_bytes': { sig: 'ecrire_bytes(chemin, octets)', doc: 'Écrit des octets bruts dans un fichier' },
  'type': { sig: 'type(x)', doc: 'Nom du type d’une valeur : entier, rationnel, flottant, booleen, chaine, complexe, irrationnel, liste, table…' },
  'fib': { sig: 'fib(n)', doc: 'Fibonacci' },
  'fibonacci': { sig: 'fibonacci(n)', doc: 'Fibonacci' },
  'pgcd': { sig: 'pgcd(a, b)', doc: 'Plus grand commun diviseur' },
  'gcd': { sig: 'gcd(a, b)', doc: 'Plus grand commun diviseur' },
  'ppcm': { sig: 'ppcm(a, b)', doc: 'Plus petit commun multiple' },
  'lcm': { sig: 'lcm(a, b)', doc: 'Plus petit commun multiple' },
  'puissance': { sig: 'puissance(x, n)', doc: 'Puissance' },
  'pow': { sig: 'pow(x, n)', doc: 'Puissance' },
  'nb_chiffres': { sig: 'nb_chiffres(n)', doc: 'Nombre de chiffres' },
  'digits': { sig: 'digits(n)', doc: 'Nombre de chiffres' },
  'est_premier': { sig: 'est_premier(n)', doc: 'Test de primalité' },
  'is_prime': { sig: 'is_prime(n)', doc: 'Test de primalité' },
  'compter_premiers': { sig: 'compter_premiers(n)', doc: 'Compte les premiers ≤ n' },
  'pi': { sig: 'pi / pi(n)', doc: 'π (constante ou n décimales)' },
  'e': { sig: 'e / e(n)', doc: 'e (constante ou n décimales)' },
  'phi': { sig: 'phi / phi(n)', doc: 'φ (constante ou n décimales)' },
  'racine': { sig: 'racine(n)', doc: '√n exact (irrationnel)' },
  'fraction': { sig: 'fraction(a, b)', doc: 'Rationnel exact a/b' },
  're': { sig: 're(z)', doc: 'Partie réelle' },
  'im': { sig: 'im(z)', doc: 'Partie imaginaire' },
  'conj': { sig: 'conj(z)', doc: 'Conjugué' },
  'arg': { sig: 'arg(z)', doc: 'Argument' },
  'modulus': { sig: 'modulus(z)', doc: 'Module |z|' },
  'longueur': { sig: 'longueur(s)', doc: 'Longueur en caractères' },
  'len': { sig: 'len(s)', doc: 'Longueur en caractères' },
  'ordre': { sig: 'ordre(s)', doc: 'Codepoint du premier caractère' },
  'caractere': { sig: 'caractere(n)', doc: 'Caractère du codepoint n' },
  'sous_chaine': { sig: 'sous_chaine(s, d, f)', doc: 'Sous-chaîne [d, f[' },
  'sub': { sig: 'sub(s, d, f)', doc: 'Sous-chaîne [d, f[' },
  'concatener': { sig: 'concatener(liste)', doc: 'Concatène une liste de chaînes' },
  'table': { sig: 'table()', doc: 'Table associative' },
  'map': { sig: 'map()', doc: 'Table associative' },
  'inserer': { sig: 'inserer(t, k, v)', doc: 'Insère clé/valeur' },
  'cle_existe': { sig: 'cle_existe(t, k)', doc: 'Clé présente ?' },
  'supprimer': { sig: 'supprimer(t, k)', doc: 'Supprime la clé' },
  'cles': { sig: 'cles(t)', doc: 'Liste des clés' },
  'valeurs': { sig: 'valeurs(t)', doc: 'Liste des valeurs' },
  'joindre': { sig: 'joindre(l, x)', doc: 'Ajoute x à la fin de la liste l' },
  'graine': { sig: 'graine(n)', doc: 'Graine du générateur' },
  'seed': { sig: 'seed(n)', doc: 'Graine du générateur' },
  'hasard': { sig: 'hasard(n)', doc: 'Entier aléatoire dans [0, n[' },
  'prng': { sig: 'prng(n)', doc: 'PRNG déterministe' },
  'hasard_systeme': { sig: 'hasard_systeme(n)', doc: 'Hasard système' },
  'rng_materiel': { sig: 'rng_materiel()', doc: 'RNG matériel' },
  'hasard_vrai': { sig: 'hasard_vrai(n)', doc: 'Hasard vrai' },
  'lire_texte': { sig: 'lire_texte(chemin)', doc: 'Lit un fichier texte' },
  'ecrire_texte': { sig: 'ecrire_texte(chemin, texte)', doc: 'Écrit un fichier texte' },
  'ecrire_binaire': { sig: 'ecrire_binaire(chemin, octets)', doc: 'Écrit un fichier binaire' },
  'argument': { sig: 'argument(i)', doc: 'Argument i de la ligne de commande' },
  'nb_arguments': { sig: 'nb_arguments()', doc: 'Nombre d’arguments' },
  'temps': { sig: 'temps()', doc: 'Timestamp' },
  'millisecondes': { sig: 'millisecondes()', doc: 'Millisecondes' },
  'ms': { sig: 'ms()', doc: 'Millisecondes' },
  'time': { sig: 'time()', doc: 'Temps' },
  'duree': { sig: 'duree(n)', doc: 'Durée' },
  'afficher': { sig: 'afficher(x)', doc: 'Affiche x' },
  'afficher_erreur': { sig: 'afficher_erreur(x)', doc: 'Affiche sur stderr' },
  'quitter': { sig: 'quitter(n)', doc: 'Termine avec code n' },
  'tampon': { sig: 'tampon(n)', doc: 'Tampon d’octets de taille n' },
  'octets': { sig: 'octets(chaine)', doc: 'Tampon d’octets' },
  'eval_expr': { sig: 'eval_expr(chaine)', doc: 'Évalue une expression Rose' },
  'breakpoint': { sig: 'breakpoint', doc: 'Point d’arrêt' },
  'watch': { sig: 'watch(var)', doc: 'Surveille une variable' }
};

const SNIPPETS = {
  'si': 'si (${1:cond}) {\n\t${2:}\n} sinon {\n\t${3:}\n}',
  'pour': 'pour ${1:i} de ${2:0} vers ${3:10} {\n\t${4:}\n}',
  'tant_que': 'tant_que (${1:cond}) {\n\t${2:}\n}',
  '?': '? (${1:cond}) [ ${2:instr} ; ${3:instr} ]',
  '@': '@ (${1:cond}) [ ${2:instr} ; ${3:instr} ]',
  'fn': 'fn ${1:nom}(${2:param}) {\n\t${3:}\n\tretour ${4:valeur}\n}',
  'fnt': 'fn ${1:nom}(${2:x}: entier) -> entier {\n\tsi (${2:x} < 2) { retour ${2:x} }\n\tretour ${1:nom}(${2:x} - 1) + ${1:nom}(${2:x} - 2)\n}'
};

let diagnosticCollection;
const validationTimers = new Map();

function cfg() { return vscode.workspace.getConfiguration('rose'); }

function parseSymbols(document) {
  const text = document.getText();
  const functions = [];
  const variables = new Set();
  const fnRegex = /\bfn\s+([a-zA-Z_]\w*)\s*\(([^)]*)\)\s*(?:->|→)?\s*(\w+)?/g;
  let m;
  while ((m = fnRegex.exec(text)) !== null) {
    const params = m[2].trim() === '' ? [] : m[2].split(',').map(p => p.trim()).filter(Boolean);
    functions.push({ name: m[1], params, returnType: m[3] || null, offset: m.index });
  }
  const varRegex = /\$([a-zA-Z_]\w*)\s*=(?!=)/g;
  while ((m = varRegex.exec(text)) !== null) {
    variables.add(m[1]);
  }
  return { functions, variables: Array.from(variables) };
}

function offsetToPosition(document, offset) {
  return document.positionAt(offset);
}

function getBinaryPath() {
  return cfg().get('binaryPath', 'rose');
}

function runRose(filePath, callback) {
  const bin = getBinaryPath();
  cp.execFile(bin, [filePath], { timeout: 15000 }, (err, stdout, stderr) => {
    callback(err, stdout || '', stderr || '');
  });
}

function validateDocument(document) {
  if (!document || document.languageId !== 'rose') return;
  if (!cfg().get('diagnostics', true)) {
    diagnosticCollection.delete(document.uri);
    return;
  }
  const timer = validationTimers.get(document.uri.toString());
  if (timer) clearTimeout(timer);
  const delay = cfg().get('diagnosticsDelay', 600);
  validationTimers.set(document.uri.toString(), setTimeout(() => {
    doValidate(document);
  }, delay));
}

function doValidate(document) {
  const diagnostics = [];
  const text = document.getText();
  // write to temp file for rose
  const tmpFile = path.join(os.tmpdir(), 'rose-validate-' + Date.now() + '.rose');
  try {
    fs.writeFileSync(tmpFile, text, 'utf8');
  } catch (e) {
    return;
  }
  runRose(tmpFile, (err, stdout, stderr) => {
    try { fs.unlinkSync(tmpFile); } catch (e) {}
    const output = (stderr || '') + '\n' + (stdout || '');
    const re = /\[Ligne\s+(\d+),\s*Colonne\s+(\d+)\]\s*(.*)/gi;
    let m;
    while ((m = re.exec(output)) !== null) {
      const line = Math.max(0, parseInt(m[1], 10) - 1);
      const col = Math.max(0, parseInt(m[2], 10));
      const msg = m[3].trim();
      const range = new vscode.Range(line, col, line, Math.max(col + 1, col));
      const diag = new vscode.Diagnostic(range, msg, vscode.DiagnosticSeverity.Error);
      diag.source = 'Rose';
      diagnostics.push(diag);
    }
    if (diagnostics.length === 0 && err) {
      const m2 = output.match(/Rose\s*:\s*erreur\s*[—-]\s*(.*)/i);
      if (m2) {
        const range = new vscode.Range(0, 0, 0, Math.min(text.split('\n')[0].length, 1));
        const diag = new vscode.Diagnostic(range, m2[1].trim(), vscode.DiagnosticSeverity.Error);
        diag.source = 'Rose';
        diagnostics.push(diag);
      }
    }
    // heuristics
    addHeuristicDiagnostics(document, diagnostics);
    diagnosticCollection.set(document.uri, diagnostics);
  });
}

function addHeuristicDiagnostics(document, diagnostics) {
  const text = document.getText();
  const lines = text.split('\n');
  // trailing whitespace
  lines.forEach((line, i) => {
    if (/[ \t]+$/.test(line)) {
      const range = new vscode.Range(i, line.trimEnd().length, i, line.length);
      const d = new vscode.Diagnostic(range, 'Espaces en fin de ligne', vscode.DiagnosticSeverity.Hint);
      d.source = 'Rose';
      diagnostics.push(d);
    }
  });
  // TODO/FIXME
  lines.forEach((line, i) => {
    const m = line.match(/#\s*(TODO|FIXME|HACK|XXX)\b(.*)/i);
    if (m) {
      const idx = line.indexOf(m[0]);
      const range = new vscode.Range(i, Math.max(0, idx), i, line.length);
      const d = new vscode.Diagnostic(range, m[0].trim(), vscode.DiagnosticSeverity.Information);
      d.source = 'Rose';
      diagnostics.push(d);
    }
  });
  // unused variables
  const declRegex = /\$([a-zA-Z_]\w*)\s*=(?!=)/g;
  const decls = new Map();
  let m;
  while ((m = declRegex.exec(text)) !== null) {
    if (!decls.has(m[1])) decls.set(m[1], m.index);
  }
  for (const [name, offset] of decls) {
    const occ = (text.match(new RegExp('\\$' + name + '\\b', 'g')) || []).length;
    if (occ <= 1) {
      const pos = document.positionAt(offset);
      const range = new vscode.Range(pos, pos.translate(0, name.length + 1));
      const d = new vscode.Diagnostic(range, `Variable '${name}' déclarée mais non utilisée`, vscode.DiagnosticSeverity.Warning);
      d.source = 'Rose';
      diagnostics.push(d);
    }
  }
}

function activate(context) {
  diagnosticCollection = vscode.languages.createDiagnosticCollection('rose');
  context.subscriptions.push(diagnosticCollection);

  // ligatures
  const typeListener = vscode.workspace.onDidChangeTextDocument((event) => {
    const editor = vscode.window.activeTextEditor;
    if (!editor || event.document.languageId !== 'rose') return;
    if (!cfg().get('ligatures', true)) return;
    const change = event.contentChanges[0];
    if (!change) return;
    const lineIndex = change.range.start.line;
    const line = editor.document.lineAt(lineIndex).text;
    const replacements = { '_^': '⩠', '=~': '≈', '!=': '≠', '<=': '≤', '>=': '≥' };
    for (const [key, value] of Object.entries(replacements)) {
      if (line.includes(key) && !line.includes(value)) {
        const idx = line.indexOf(key);
        const startPos = new vscode.Position(lineIndex, idx);
        const endPos = new vscode.Position(lineIndex, idx + key.length);
        editor.edit((eb) => { eb.replace(new vscode.Range(startPos, endPos), value); }, { undoStopBefore: false, undoStopAfter: false });
        break;
      }
    }
  });
  context.subscriptions.push(typeListener);

  // validation on change/open/save
  const validateOnEvent = (doc) => validateDocument(doc);
  context.subscriptions.push(vscode.workspace.onDidOpenTextDocument(validateOnEvent));
  context.subscriptions.push(vscode.workspace.onDidChangeTextDocument((e) => validateOnEvent(e.document)));
  context.subscriptions.push(vscode.workspace.onDidSaveTextDocument(validateOnEvent));
  vscode.workspace.textDocuments.forEach(validateOnEvent);

  // Hover
  const hoverProvider = vscode.languages.registerHoverProvider('rose', {
    provideHover(document, position) {
      const range = document.getWordRangeAtPosition(position, /\$?[a-zA-Z_][a-zA-Z0-9_]*/);
      if (!range) return;
      const raw = document.getText(range);
      const word = raw.replace(/^\$/, '');
      let md = null;
      if (KEYWORDS.includes(word)) md = new vscode.MarkdownString(`**Mot-clé Rose** : \`${word}\``);
      else if (TYPES.includes(word)) md = new vscode.MarkdownString(`**Type Rose** : \`${word}\``);
      else if (STDLIB[word]) md = new vscode.MarkdownString(`**Fonction standard**\n\n\`${STDLIB[word].sig}\`\n\n${STDLIB[word].doc}`);
      else {
        const syms = parseSymbols(document);
        const fn = syms.functions.find(f => f.name === word);
        if (fn) md = new vscode.MarkdownString(`**Fonction** \`${fn.name}(${fn.params.join(', ')})\`${fn.returnType ? ' → ' + fn.returnType : ''}`);
        else if (syms.variables.includes(word)) md = new vscode.MarkdownString(`**Variable** \`$${word}\``);
      }
      if (!md) return;
      return new vscode.Hover(md, range);
    }
  });
  context.subscriptions.push(hoverProvider);

  // Completion
  const completionProvider = vscode.languages.registerCompletionItemProvider('rose', {
    provideCompletionItems(document, position) {
      const items = [];
      for (const k of KEYWORDS) {
        const it = new vscode.CompletionItem(k, vscode.CompletionItemKind.Keyword);
        if (SNIPPETS[k]) { it.insertText = new vscode.SnippetString(SNIPPETS[k]); it.kind = vscode.CompletionItemKind.Snippet; }
        items.push(it);
      }
      for (const t of TYPES) {
        items.push(new vscode.CompletionItem(t, vscode.CompletionItemKind.TypeParameter));
      }
      for (const [name, info] of Object.entries(STDLIB)) {
        const it = new vscode.CompletionItem(name, vscode.CompletionItemKind.Function);
        it.detail = info.sig + ' — ' + info.doc;
        it.insertText = new vscode.SnippetString(`${name}($0)`);
        items.push(it);
      }
      const syms = parseSymbols(document);
      for (const fn of syms.functions) {
        const it = new vscode.CompletionItem(fn.name, vscode.CompletionItemKind.Function);
        it.detail = `fonction ${fn.name}(${fn.params.join(', ')})`;
        it.insertText = new vscode.SnippetString(`${fn.name}($0)`);
        items.push(it);
      }
      for (const v of syms.variables) {
        const it = new vscode.CompletionItem('$' + v, vscode.CompletionItemKind.Variable);
        it.insertText = '$' + v;
        items.push(it);
      }
      return items;
    }
  }, ' ', '(', '$');
  context.subscriptions.push(completionProvider);

  // Definition
  const defProvider = vscode.languages.registerDefinitionProvider('rose', {
    provideDefinition(document, position) {
      const range = document.getWordRangeAtPosition(position, /\$?[a-zA-Z_][a-zA-Z0-9_]*/);
      if (!range) return;
      const word = document.getText(range).replace(/^\$/, '');
      const text = document.getText();
      // function
      const fnRe = new RegExp('\\bfn\\s+' + word + '\\s*\\(', 'g');
      let m = fnRe.exec(text);
      if (m) {
        const pos = document.positionAt(m.index);
        return new vscode.Location(document.uri, pos);
      }
      // variable assignment
      const varRe = new RegExp('\\$' + word + '\\s*=(?!=)', 'g');
      m = varRe.exec(text);
      if (m) {
        const pos = document.positionAt(m.index);
        return new vscode.Location(document.uri, pos);
      }
      return null;
    }
  });
  context.subscriptions.push(defProvider);

  // Document symbols
  const symProvider = vscode.languages.registerDocumentSymbolProvider('rose', {
    provideDocumentSymbols(document) {
      const syms = parseSymbols(document);
      const result = [];
      const text = document.getText();
      const fnRe = /\bfn\s+([a-zA-Z_]\w*)\s*\(([^)]*)\)/g;
      let m;
      while ((m = fnRe.exec(text)) !== null) {
        const name = m[1];
        const params = m[2].trim() === '' ? [] : m[2].split(',').map(p => p.trim());
        const pos = document.positionAt(m.index);
        const endPos = document.positionAt(m.index + m[0].length);
        const range = new vscode.Range(pos, endPos);
        const selRange = new vscode.Range(pos, pos.translate(0, m[0].length));
        const sym = new vscode.DocumentSymbol(name, params.join(', '), vscode.SymbolKind.Function, range, selRange);
        result.push(sym);
      }
      const varRe = /\$([a-zA-Z_]\w*)\s*=(?!=)/g;
      while ((m = varRe.exec(text)) !== null) {
        const name = m[1];
        const pos = document.positionAt(m.index);
        const endPos = document.positionAt(m.index + m[0].length);
        const range = new vscode.Range(pos, endPos);
        const sym = new vscode.DocumentSymbol('$' + name, 'variable', vscode.SymbolKind.Variable, range, range);
        result.push(sym);
      }
      return result;
    }
  });
  context.subscriptions.push(symProvider);

  // Signature help
  const sigProvider = vscode.languages.registerSignatureHelpProvider('rose', {
    provideSignatureHelp(document, position, token, context) {
      // find function name before position
      const line = document.lineAt(position.line).text.substring(0, position.character);
      const m = line.match(/([a-zA-Z_]\w*)\s*\([^)]*$/);
      if (!m) return null;
      const name = m[1];
      let info = STDLIB[name];
      let params = [];
      if (info) {
        const sigMatch = info.sig.match(/\((.*)\)/);
        if (sigMatch && sigMatch[1].trim() !== '') params = sigMatch[1].split(',').map(p => p.trim());
      } else {
        const syms = parseSymbols(document);
        const fn = syms.functions.find(f => f.name === name);
        if (fn) params = fn.params;
        else return null;
      }
      const sig = new vscode.SignatureInformation((info ? info.sig : name + '(' + params.join(', ') + ')'), (info ? info.doc : ''));
      sig.parameters = params.map(p => new vscode.ParameterInformation(p));
      const help = new vscode.SignatureHelp();
      help.signatures = [sig];
      // active parameter based on commas
      const argsPart = line.slice(line.lastIndexOf('(') + 1);
      help.activeParameter = (argsPart.match(/,/g) || []).length;
      return help;
    }
  }, '(', ',');
  context.subscriptions.push(sigProvider);

  // Formatting
  const fmtProvider = vscode.languages.registerDocumentFormattingEditProvider('rose', {
    provideDocumentFormattingEdits(document) {
      const edits = [];
      const text = document.getText();
      const lines = text.split('\n');
      let indent = 0;
      const indentSize = cfg().get('format.indentSize', 4);
      const unit = ' '.repeat(indentSize);
      const newLines = [];
      for (let i = 0; i < lines.length; i++) {
        const trimmed = lines[i].trim();
        if (trimmed.startsWith('}')) indent = Math.max(0, indent - 1);
        newLines.push(unit.repeat(indent) + trimmed);
        const openCount = (trimmed.match(/\{/g) || []).length;
        const closeCount = (trimmed.match(/\}/g) || []).length;
        indent = Math.max(0, indent + openCount - closeCount);
      }
      const fullRange = new vscode.Range(0, 0, document.lineCount - 1, lines[lines.length - 1].length);
      edits.push(vscode.TextEdit.replace(fullRange, newLines.join('\n')));
      return edits;
    }
  });
  context.subscriptions.push(fmtProvider);

  // Folding
  const foldProvider = vscode.languages.registerFoldingRangeProvider('rose', {
    provideFoldingRanges(document) {
      const ranges = [];
      const text = document.getText();
      const lines = text.split('\n');
      const stack = [];
      lines.forEach((line, i) => {
        if (/\{/.test(line)) stack.push(i);
        if (/\}/.test(line) && stack.length) {
          const start = stack.pop();
          if (i - start > 0) ranges.push(new vscode.FoldingRange(start, i, vscode.FoldingRangeKind.Region));
        }
      });
      return ranges;
    }
  });
  context.subscriptions.push(foldProvider);

  // Code actions (quick fixes)
  const codeActionProvider = vscode.languages.registerCodeActionsProvider('rose', {
    provideCodeActions(document, range, context) {
      const actions = [];
      for (const diag of context.diagnostics) {
        if (diag.message && /Fonction .* inconnue/i.test(diag.message)) {
          const action = new vscode.CodeAction('Consulter la documentation Rose', vscode.CodeActionKind.QuickFix);
          action.command = { command: 'rose.openWebsite', title: 'Ouvrir le site' };
          actions.push(action);
        }
      }
      return actions;
    }
  });
  context.subscriptions.push(codeActionProvider);

  // Commands
  const runFile = vscode.commands.registerCommand('rose.runFile', async () => {
    const editor = vscode.window.activeTextEditor;
    if (!editor || editor.document.languageId !== 'rose') { vscode.window.showWarningMessage('Aucun fichier Rose actif.'); return; }
    await editor.document.save();
    const bin = getBinaryPath();
    const filePath = editor.document.uri.fsPath;
    if (cfg().get('runInTerminal', true)) {
      const terminal = vscode.window.createTerminal('Rose');
      terminal.show();
      terminal.sendText(`${bin} "${filePath}"`);
    } else {
      cp.execFile(bin, [filePath], (err, stdout, stderr) => {
        const out = (stdout || '') + (stderr || '');
        vscode.window.showInformationMessage((out || 'Terminé.').slice(0, 500));
      });
    }
  });
  context.subscriptions.push(runFile);

  const latexFile = vscode.commands.registerCommand('rose.latexFile', async () => {
    const editor = vscode.window.activeTextEditor;
    if (!editor || editor.document.languageId !== 'rose') { vscode.window.showWarningMessage('Aucun fichier Rose actif.'); return; }
    await editor.document.save();
    const bin = getBinaryPath();
    const filePath = editor.document.uri.fsPath;
    const terminal = vscode.window.createTerminal('Rose LaTeX');
    terminal.show();
    terminal.sendText(`${bin} latex "${filePath}"`);
  });
  context.subscriptions.push(latexFile);

  const consoleFile = vscode.commands.registerCommand('rose.consoleFile', async () => {
    const editor = vscode.window.activeTextEditor;
    if (!editor || editor.document.languageId !== 'rose') { vscode.window.showWarningMessage('Aucun fichier Rose actif.'); return; }
    await editor.document.save();
    const bin = getBinaryPath();
    const filePath = editor.document.uri.fsPath;
    const terminal = vscode.window.createTerminal('Rose Console');
    terminal.show();
    terminal.sendText(`${bin} "${filePath}" --console`);
  });
  context.subscriptions.push(consoleFile);

  const openTerminal = vscode.commands.registerCommand('rose.openTerminal', () => {
    const terminal = vscode.window.createTerminal('Rose');
    terminal.show();
    terminal.sendText('rose --help');
  });
  context.subscriptions.push(openTerminal);

  const openWebsite = vscode.commands.registerCommand('rose.openWebsite', () => {
    const url = cfg().get('website', 'https://stef302.github.io/Rose/');
    vscode.env.openExternal(vscode.Uri.parse(url));
  });
  context.subscriptions.push(openWebsite);

  const validateCmd = vscode.commands.registerCommand('rose.validate', () => {
    const editor = vscode.window.activeTextEditor;
    if (editor) validateDocument(editor.document);
  });
  context.subscriptions.push(validateCmd);

  // status bar
  const status = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 100);
  status.command = 'rose.runFile';
  context.subscriptions.push(status);
  const updateStatus = () => {
    const editor = vscode.window.activeTextEditor;
    if (editor && editor.document.languageId === 'rose') {
      status.text = '$(play) Rose';
      status.tooltip = 'Exécuter le fichier Rose (Ctrl+Alt+R)';
      status.show();
    } else {
      status.hide();
    }
  };
  context.subscriptions.push(vscode.window.onDidChangeActiveTextEditor(updateStatus));
  updateStatus();
}

function deactivate() {}

module.exports = { activate, deactivate };
