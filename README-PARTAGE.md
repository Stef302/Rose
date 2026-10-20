# Rose pour Windows x64 — setup de test

Copie depuis gx10-2 (100.67.23.83, tailnet). Date : 2026-10-20 (soir).
Génération du moteur : **v3.12.0 + chantiers M4.1/M4.2** (rose_ent 16
octets, comblement des lacunes AOT, contrat anti-silence, divergence
rust/zig sur les tables corrigée).

## Copier chez toi

Méthode 1 — un seul fichier (recommandé), depuis PowerShell :

    Invoke-WebRequest http://100.67.23.83:18080/rose-windows.zip -OutFile $HOME\Downloads\rose-windows.zip

Méthode 2 — navigateur : http://100.67.23.83:18080/rose-windows/ puis
télécharge fichier par fichier (conserver la hiérarchie des dossiers).

Vérifier l'intégrité du zip :

    Get-FileHash $HOME\Downloads\rose-windows.zip -Algorithm SHA256
    # attendu : la 1re ligne de SHA256SUMS.txt ci-dessous

Dézipper dans un dossier (clic droit → Extraire tout), puis :

    cd <dossier>
    powershell -ExecutionPolicy Bypass -File .\tester.ps1

## Ce que teste tester.ps1

Les **3 moteurs autonomes** contre :

1. les **44 dorés** du dépôt (`moteur\tests\dorés\`), sortie comparée en
   octets bruts ;
2. les **12 « roses à tester »** (`moteur\tests\roses\`) — programmes de
   torture multi-features : notation symbolique, mixtes rationnel⊛flottant,
   tables complètes, puissances tordues, boucles, chaînes UTF-8, grands
   entiers GMP, flottants exacts, récursion, erreurs runtime. Chacune a
   son `.attendu` (octet pour octet) ou `.attendu.err` (code + message
   d'erreur). Voir `moteur\tests\roses\MANIFEST.md`.

| exécutable            | moteur                    | état       |
|-----------------------|---------------------------|------------|
| `bin\rose-win64.exe`  | C++/GMP (le moteur de référence) | **nouveau** : reconstruit aujourd'hui (M4.2), JAMAIS EXÉCUTÉ sous Windows avant toi |
| `bin\rose-rs-win64.exe` | Rust/GMP               | reconstruit — bug d'affichage des tables corrigé ce soir |
| `bin\rose-zig-win64.exe` | Zig/GMP                 | reconstruit — même correction |

Résultat attendu : tout vert, zéro ECHEC. Le C++ est le juge : ses
sorties constituent les dorés et les `.attendu` (identiques à Linux,
parité 116/116 + 12 roses vérifiées ici sur ces arbres).

Si une rose ROUGE t'arrive : copie l'afficheur complet (ligne ECHEC +
code) — chaque divergence est une information, rien n'est censé bouger.

## Les roses, dans le détail

- `rose_symboles` — `⋅ × ÷ √ π ² ³ ^`, affectation ←, fonction flottante
- `rose_mixtes` — `1/4 + 0.5`, Q⊛F plié et dynamique
- `rose_tables` — cycle de vie des tables, rationnel DEDANS (rendu « a/b »),
  valeurs (rendu décimal), `==` ordre-indépendant
- `rose_puissances` — `2^-3` → `1/8`, `9 ^ 0.5` → 3, `(-8) ^ (1/3)` → 1
  (plancher de l'exposant — quirks fidèlement rendus)
- `rose_boucles` — `pour … vers`, `tant_que(…)`, indices de listes
- `rose_chaines` — UTF-8 : `longueur("🌹")=1`, `caractere(127801)`=🌹,
  `chaine(3/4)` → « 3/4 » (pas 0.75 — c'est LA règle des deux rendus)
- `rose_grands` — factorielle(50), fibonacci(90), 2^200, promotion GMP
- `rose_flottants` — affichage exact des doubles (périodes `[6]`…)
- `rose_recursif` — fibo récursif, boucle dans fonction
- `rose_cle_absente` / `rose_div_zero` — erreurs runtime : code 1 + message
- `rose_puissance_dyn` — z^e à exposant variable (démonstration, pas un
  test d'échec : l'interprète répond, le mode compilable AOT rejette
  poliment — sous Windows tout le monde est interprète, ça doit passer)

## Limites connues du moteur C++ sous Windows

- Pas de JIT natif Windows (repli interpréteur, décisions identiques) ;
- `rose build` (AOT, nécessite g++) y est un rejet poli documenté.
