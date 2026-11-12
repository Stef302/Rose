param(
    [string]$HtmlPath = (Join-Path $PSScriptRoot "ui\index.html"),
    [string]$ThemePath = (Join-Path $PSScriptRoot "ui\theme.css"),
    [string]$Target = "x86_64-pc-windows-msvc"
)

$ErrorActionPreference = "Stop"

$projectRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$binary = Join-Path $projectRoot "src-tauri\target\$target\release\html-installer.exe"
$dist = Join-Path $projectRoot "dist"
$payload = Join-Path $projectRoot "payload"
$frontend = Join-Path $projectRoot "ui"

$htmlSource = (Resolve-Path -LiteralPath $HtmlPath).Path
if ([string]::IsNullOrWhiteSpace($ThemePath)) {
    $ThemePath = (Join-Path (Split-Path -Parent $htmlSource) "theme.css")
}
elseif (-not (Test-Path -LiteralPath $ThemePath)) {
    throw "Theme CSS introuvable : $ThemePath"
}

if (-not (Test-Path -LiteralPath $payload)) {
    throw "Dossier payload introuvable : $payload. Ajoutez rose.exe dedans avant de builder."
}

if (Test-Path -LiteralPath $frontend) {
    Remove-Item -LiteralPath $frontend -Recurse -Force | Out-Null
}
New-Item -ItemType Directory -Path $frontend -Force | Out-Null

Copy-Item -LiteralPath $htmlSource -Destination (Join-Path $frontend "index.html")
if (Test-Path -LiteralPath $ThemePath) {
    Copy-Item -LiteralPath $ThemePath -Destination (Join-Path $frontend "theme.css")
}
foreach ($assetName in @("installer.js", "installer.json")) {
    $assetPath = Join-Path (Split-Path -Parent $htmlSource) $assetName
    if (Test-Path -LiteralPath $assetPath) {
        Copy-Item -LiteralPath $assetPath -Destination (Join-Path $frontend $assetName)
    }
}

Push-Location $projectRoot
try {
    Write-Output "Build Tauri pour $target..."
    rustup run stable-x86_64-pc-windows-msvc cargo tauri build --no-bundle --target $target
    if ($LASTEXITCODE -ne 0) {
        throw "Le build Tauri a echoue (code $LASTEXITCODE)."
    }

    if (-not (Test-Path $binary)) {
        throw "Binaire introuvable apres le build : $binary"
    }

    New-Item -ItemType Directory -Path $dist -Force | Out-Null
    Copy-Item -Path $binary -Destination (Join-Path $dist "Rose-Setup.exe") -Force
    $distPayload = Join-Path $dist "payload"
    if (Test-Path -LiteralPath $distPayload) {
        Remove-Item -LiteralPath $distPayload -Recurse -Force
    }
    New-Item -ItemType Directory -Path $distPayload -Force | Out-Null
    Get-ChildItem -LiteralPath $payload -Force | Copy-Item -Destination $distPayload -Recurse -Force
    Write-Output "`nInstalleur pret : $(Join-Path $dist 'Rose-Setup.exe')"
    Write-Output "Payload : $distPayload"
} finally {
    Pop-Location
}
