# =====================================================================
# ROSE v3.12 - test express sur Windows (reseau Tailonly / Tailscale)
# Usage :  irm http://100.67.23.83:18080/installer.ps1 | iex
# Telecharge les trois moteurs (C++, Rust, Zig), deplie dans
# %USERPROFILE%\rose, lance les 46 tests dores + 12 roses de torture,
# puis installe un lanceur rose.cmd pour vos fichiers .rose.
# =====================================================================
$ErrorActionPreference = 'Stop'
$Url    = 'http://100.67.23.83:18080/rose-windows.zip'
$Racine = Join-Path $env:USERPROFILE 'rose'
$Zip    = Join-Path $env:TEMP 'rose-windows.zip'

Write-Host '[ROSE] Telechargement des moteurs (C++, Rust, Zig) ...'
Invoke-RestMethod -Uri $Url -OutFile $Zip
$sha = (Get-FileHash -Algorithm SHA256 -Path $Zip).Hash.ToLower()
Write-Host ('[ROSE] Archive : ' + (Get-Item $Zip).Length + ' octets, sha256 ' + $sha)

Write-Host ('[ROSE] Deploiement dans ' + $Racine)
Expand-Archive -Path $Zip -DestinationPath $Racine -Force
Get-ChildItem -Path (Join-Path $Racine 'bin') -Filter '*.exe' | Unblock-File

Write-Host '[ROSE] Suite de comparaison : 46 tests dores + 12 roses, 3 moteurs ...'
powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $Racine 'tester.ps1')
$code = $LASTEXITCODE

# Lanceur : rose.cmd mon_fichier.rose
$lines = @('@echo off', '"%~dp0bin\rose-win64.exe" %*')
Set-Content -Path (Join-Path $Racine 'rose.cmd') -Value $lines -Encoding Ascii

if ($code -eq 0) {
    Write-Host ''
    Write-Host '[ROSE] Tout est fleuri : 0 divergence entre les moteurs.' -ForegroundColor Green
} else {
    Write-Host ''
    Write-Host ('[ROSE] La suite a signale des echecs (code ' + $code + '). Copiez les lignes ECHEC.') -ForegroundColor Yellow
}
Write-Host ('[ROSE] Pour jouer : ' + $Racine + '\rose.cmd mon_fichier.rose')
Write-Host ('[ROSE] Ou en direct : ' + $Racine + '\bin\rose-win64.exe mon_fichier.rose')
