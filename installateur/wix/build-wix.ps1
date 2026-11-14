# ═══════════════════════════════════════════════════════════════════════
#  Construit l'installeur Windows « Rose-Setup-<version>.msi » (WiX Toolset).
#
#  Prérequis sur la machine de build (Windows x64) :
#    - WiX Toolset v3 (candle.exe, light.exe) dans le PATH
#      https://wixtoolset.org/releases/
#    - rose.exe + LLVM-C.dll dans ce dossier (ou staging/)
#  Sortie : ..\dist\Rose-Setup-<version>.msi (+ SHA256SUMS.txt)
# ═══════════════════════════════════════════════════════════════════════
param(
  [string]$Version = "3.14.0",
  [string]$WixDir  = $PSScriptRoot
)
$ErrorActionPreference = "Stop"
$Dist = Join-Path $WixDir "..\dist"
New-Item -ItemType Directory -Force -Path $Dist | Out-Null

# Staging des binaires si absents
if (-not (Test-Path (Join-Path $WixDir "rose.exe"))) {
  $src = Join-Path $WixDir "..\staging\rose.exe"
  if (Test-Path $src) { Copy-Item $src (Join-Path $WixDir "rose.exe") }
}
if (-not (Test-Path (Join-Path $WixDir "LLVM-C.dll"))) {
  $src = Join-Path $WixDir "..\staging\LLVM-C.dll"
  if (Test-Path $src) { Copy-Item $src (Join-Path $WixDir "LLVM-C.dll") }
}

Push-Location $WixDir
try {
  # Compile (UI activée via -dUI=1)
  & candle.exe -dUI=1 Product.wxs
  if ($LASTEXITCODE -ne 0) { throw "candle.exe a échoué" }

  # Link avec l'extension WixUI
  & light.exe Product.wixobj -ext WixUIExtension -o (Join-Path $Dist "Rose-Setup-$Version.msi")
  if ($LASTEXITCODE -ne 0) { throw "light.exe a échoué" }

  # Empreinte
  $msi = Join-Path $Dist "Rose-Setup-$Version.msi"
  $hash = (Get-FileHash $msi -Algorithm SHA256).Hash.ToLower()
  "$hash  Rose-Setup-$Version.msi" | Set-Content (Join-Path $Dist "SHA256SUMS.txt")
  Write-Host "✓ Installeur produit : $msi"
  Write-Host "  empreinte : $hash"
} finally {
  Pop-Location
}
