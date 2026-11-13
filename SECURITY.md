# Politique de sécurité

Rose est un langage de calcul exact auto-hébergé : le moteur compile et exécute
du code que **tu** lui donnes. Cette page explique ce que nous considérons
comme un problème de sécurité, comment le signaler, et ce que le projet
garantit (ou non).

## Signaler une vulnérabilité

- **Ne ouvre pas d'issue publique** pour un problème de sécurité.
- Écris à **security@stef302.dev** ou via
  [GitHub Private Vulnerability Reporting](https://github.com/Stef302/Rose/security/advisories/new).
- Accusé de réception visé sous 72 h ; correctif et avis public sous 30 jours
  selon la gravité.

## Surface du moteur et garanties

| Zone | Comportement attendu |
|---|---|
| Arithmétique | Exacte par construction (entiers arbitraires, rationnels, Stirling validé). Un résultat **faux** est un bug — ouvre une issue. |
| Dépassement de pile / mémoire | Un programme malveillant peut épuiser les ressources (comme tout exécutable local). Les plantages par entrées hostiles (`stack-overflow`, `heap-buffer-overflow`, UB) sont traités comme des vulnérabilités. |
| Téléchargements (`rose update`, helper AOT) | Chaque artefact est vérifié contre **SHA-256** (`SHA256SUMS` de la release). Sous Windows, Velopack signe et apply des deltas avec rollback. Un artefact dont le SHA-256 ne correspond pas doit être **rejeté** — toute version qui télécharge malgré tout est une vulnérabilité. |
| Installeur Windows | Mode utilisateur, HKCU uniquement, **aucun droit administrateur**. Toute écriture hors du dossier d'installation ou demande d'élévation est une vulnérabilité. |
| FFI (`@dll` / `@sym`) | Charge une bibliothèque système à la demande explicite de l'auteur du programme. Ce n'est pas une sandbox : ne pas utiliser `rose` comme isolant. |
| Playground web | Exécute un sous-ensemble en JavaScript dans le navigateur, sans accès réseau ni fichier. |

## Ce qui n'est PAS un problème de sécurité

- Un programme Rose qui tourne lentement ou consomme beaucoup de mémoire.
- La limite connue du visionneur sur des nombres de plusieurs dizaines de
  millions de chiffres.
- Tout comportement documenté de la FFI.

## Chaîne de confiance des releases

1. Les binaires sont produits par `cargo build --release` depuis la revue `main`.
2. Les SHA-256 sont publiés dans `SHA256SUMS` (dépôt + asset de release).
3. Les releases GitHub portent les artefacts ; le site (`docs/telechargements/`)
   en est le miroir, avec le même `SHA256SUMS`.
4. L'update Velopack (Windows) vérifie la signature du paquet et revient en
   arrière automatiquement si la nouvelle version casse.

## Version supportée

| Version | Supporté |
|---|---|
| 0.1.x | ✅ correctifs de sécurité |
| < 0.1.0 | ❌ (numérotation abandonnée) |
