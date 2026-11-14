# Installeur Rose — WiX Toolset

Ce dossier contient le projet d'installeur Windows **Rose 0.1.4** écrit pour le
**WiX Toolset** (v3), équivalent fonctionnel de l'installeur NSIS historique
(`../rose.nsi`) et de la maquette HTML (`exe.html`).

## Contenu

- `Product.wxs` — source WiX principal (per-utilisateur, HKCU, sans élévation).
- `license.rtf` — licence MIT affichée dans l'assistant (WixUI_Mondo).
- `build-wix.ps1` — build Windows (WiX Toolset : `candle.exe` + `light.exe`).
- `build-wix-linux.sh` — validation Linux via `wixl` (produit un `.msi` sans UI).
- `rose.ico`, `setup.ico` — icônes.
- `rose.exe`, `LLVM-C.dll`, `README-PARTAGE.md`, `conformite.txt` — charge utile embarquée.

## Correspondance avec l'interface HTML

| Étape HTML | Page WiX (WixUI_Mondo) |
|---|---|
| 1. Bienvenue | WelcomeDlg |
| 2. Licence MIT | LicenseAgreementDlg |
| 3. Options | SetupTypeDlg + FeaturesDlg + InstallDirDlg |
| 4. Installation | ProgressDlg |
| 5. Terminé | ExitDialog |

Fonctionnalités (cases à cocher de l'étape 3) → `Feature` WiX :

- Moteur natif `rose.exe` (requis) → `ProductFeature`
- Ajouter `rose` au PATH → `FeatPath`
- Associer les `.rose` (+ menu « Modifier ») → `FeatAssoc`
- Raccourci Bureau → `FeatShortcut`
- Documentation hors ligne → `FeatDocs`
- Suite de conformité → `FeatTests`

## Build (Windows)

```powershell
cd installateur\wix
.\build-wix.ps1 -Version 0.1.4
```

Sortie : `installateur\dist\Rose-Setup-3.14.0.msi`.

## Validation (Linux)

```bash
cd installateur/wix
bash build-wix-linux.sh
```

Produit `installateur/dist/Rose-Setup-3.14.0.msi` (charge utile validée, sans UI).

## Note sur l'EXE

WiX produit nativement un `.msi`. Pour un `.exe` auto-extractible (comme la maquette),
utiliser **Burn** (Bootstrapper) du WiX Toolset sur Windows, en référençant ce `.msi`
comme payload principal. Le `.msi` reste l'artefact canonique.
