# Build de l'installeur Velopack de Rose (Windows x64).
#
#   .\build.ps1                 # version lue dans rose\Cargo.toml
#   .\build.ps1 -Version 0.1.4  # version explicite
#   .\build.ps1 -Local          # installe localement (vpk local) au lieu de packer
#
# Prérequis : Rust MSVC, Visual C++ Build Tools + Windows SDK, CLI Tauri 2,
# CLI Velopack (dotnet tool install -g vpk).
param(
    [string]$Version = "",
    [string]$Target = "x86_64-pc-windows-msvc",
    [switch]$Local
)

$ErrorActionPreference = "Stop"

$repo = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$moteur = Join-Path $repo "rose"
$assistant = Join-Path $repo "installateur\tauri-installer"
$stage = Join-Path $PSScriptRoot "stage"
$releases = Join-Path $PSScriptRoot "Releases"

if ([string]::IsNullOrWhiteSpace($Version)) {
    $cargoToml = Get-Content (Join-Path $moteur "Cargo.toml")
    $Version = (($cargoToml | Select-String -Pattern '^version\s*=\s*"([^"]+)"').Matches[0].Groups[1].Value)
}
Write-Output "Version : $Version"

# 1. moteur natif
Push-Location $moteur
try {
    cargo build --release --target $Target
    if ($LASTEXITCODE -ne 0) { throw "Le build du moteur a echoue (code $LASTEXITCODE)." }
} finally { Pop-Location }

# 2. assistant graphique (Tauri, sans bundle : Velopack fait l'installeur)
Push-Location $assistant
try {
    # payload = binaire du moteur, copie par l'assistant
    $payload = Join-Path $assistant "payload"
    New-Item -ItemType Directory -Path $payload -Force | Out-Null
    Copy-Item (Join-Path $moteur "target\$Target\release\rose.exe") (Join-Path $payload "rose.exe") -Force
    (Get-Content (Join-Path $assistant "ui\installer.json") -Raw) `
        -replace '"version"\s*:\s*"[^"]*"', "`"version`": `"$Version`"" |
        Set-Content (Join-Path $assistant "ui\installer.json") -NoNewline
    cargo tauri build --no-bundle --target $Target
    if ($LASTEXITCODE -ne 0) { throw "Le build Tauri a echoue (code $LASTEXITCODE)." }
} finally { Pop-Location }

# 3. dossier à packer : rose.exe + l'assistant
if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
New-Item -ItemType Directory -Path $stage -Force | Out-Null
Copy-Item (Join-Path $moteur "target\$Target\release\rose.exe") $stage -Force
Copy-Item (Join-Path $assistant "src-tauri\target\$Target\release\html-installer.exe") `
    (Join-Path $stage "Rose-Setup.exe") -Force
Copy-Item (Join-Path $assistant "payload") (Join-Path $stage "payload") -Recurse -Force

# 4. packaging Velopack
$vpkArgs = @(
    "pack",
    "--packId", "Rose",
    "--packVersion", $Version,
    "--packDir", $stage,
    "--mainExe", "rose.exe",
    "--packTitle", "Rose",
    "--packAuthors", "Stef302",
    "--outputDir", $releases
)
if ($Local) {
    vpk local --packageId Rose --mainExe rose.exe
} else {
    vpk @vpkArgs
    if ($LASTEXITCODE -ne 0) { throw "vpk pack a echoue (code $LASTEXITCODE)." }
}

Write-Output ""
Write-Output "Publies dans $releases :"
Write-Output "  Rose-win-Setup.exe      installeur Velopack"
Write-Output "  Rose-$Version-full.nupkg paquet complet"
Write-Output "  releases.win.json        manifeste lu par 'rose update'"
Write-Output ""
Write-Output "Publier sur GitHub : vpk push, ou deposer ces fichiers dans la release"
Write-Output "a cote de rose-aarch64-linux (assets de Stef302/Rose)."
