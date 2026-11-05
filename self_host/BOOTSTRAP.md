# Auto-hébergement du moteur Rose — Feuille de route

## Principe

L'auto-hébergement signifie que **le compilateur et le moteur Rose sont écrits
en Rose** et capables de se compiler et s'exécuter eux-mêmes.

## État actuel (v5.0)

| Composant | Langage | Statut |
|-----------|---------|--------|
| Lexer | Rose | ✅ `compilateur/lexeur.rose` |
| Analyseur (parser) | Rose | ✅ `compilateur/analyseur.rose` |
| Vérificateur de types | Rose | ✅ `compilateur/verifieur.rose` |
| Émetteur RIR | Rose | ✅ `compilateur/emetteur.rose` |
| Émetteur C++ | Rose | ✅ `compilateur/emetteur_cpp.rose` |
| Compilateur principal | Rose | ✅ `compilateur/compilateur.rose` |
| **Moteur d'exécution (interprète + JIT)** | **Rust** | ❌ `rose/src/` — **à réécrire en Rose** |
| **Moteur C++ de référence** | **C++** | ⚠️ `rose-cpp/` — obsolète, à remplacer |

## Jalon M5 : Auto-hébergement du moteur

### Objectif
Remplacer le moteur Rust (`rose/src/`) par un moteur écrit en Rose, compilé par
le compilateur Rose existant (`compilateur/`).

### Étapes

#### 5.1 — Interpréteur Rose minimal (ce dossier)
Écrire un interpréteur Rose capable d'exécuter un sous-ensemble de Rose :
- Entiers, additions, soustractions, multiplications
- Variables et affectations
- `si / sinon`
- `pour` et `tant_que`
- Affichage `>>`

Fichier : `self_host/moteur/interpreteur.rose`

#### 5.2 — Compilation de l'interpréteur
Le compilateur Rose (`compilateur/compilateur.rose`) compile
`self_host/moteur/interpreteur.rose` vers un binaire natif (via RIR → C++ → g++,
ou via AOT LLVM ORC quand il sera complet).

#### 5.3 — Bootstrap
Le binaire obtenu exécute des programmes Rose, y compris lui-même
(auto-application).

#### 5.4 — Remplacement du moteur Rust
Une fois l'interpréteur Rose fonctionnel et validé par parité avec le moteur
C++, le moteur Rust `rose/src/` est retiré et remplacé par le binaire auto-hébergé.

#### 5.5 — Suppression du moteur C++
Le moteur C++ `rose-cpp/` devient obsolète et est archivé.

## Commandes de test

```bash
# Compiler l'interpréteur Rose avec le compilateur Rose
rose compilateur/compilateur.rose self_host/moteur/interpreteur.rose -o self_host/rose_interp

# Exécuter un programme Rose avec l'interpréteur auto-hébergé
./self_host/rose_interp mon_programme.rose
```

## Critères de validation

- [ ] Interpréteur Rose écrit en Rose compile sans erreur
- [ ] Interpréteur exécute `bench_global.rose` avec sortie identique au C++
- [ ] Parité : 54/54 comparaisons, 46/46 dorés
- [ ] L'interpréteur peut s'auto-exécuter (compile et exécute son propre code source)
