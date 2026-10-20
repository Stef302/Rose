# ═════════════════ Rose — auto-test sur Windows x64 ═════════════════
# Teste les trois moteurs autonomes (C++, Rust, Zig) contre les dorés
# du dépôt. La sortie des moteurs est capturée en OCTETS BRUTS (fichier
# de redirection, jamais par le décodage console) ; seule la forme des
# fins de ligne est normalisée — la CRT de Windows écrit « \r\n » en
# mode texte, Rust et Zig écrivent « \n » : les deux formes comptent
# comme identiques, tout le reste est comparé octet par octet.
#
# Aucun droit administrateur requis.
# Usage :  powershell -ExecutionPolicy Bypass -File .\tester.ps1

$Racine = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $Racine

$dore = Join-Path $Racine "moteur\tests\dorés"
$ok = 0; $ko = 0
function Succes([string]$msg) { $script:ok++; Write-Host "  OK    $msg" -ForegroundColor Green }
function Echec([string]$msg)  { $script:ko++; Write-Host "  ECHEC $msg" -ForegroundColor Red }

# Exécute un binaire, capture stdout en octets bruts (handle direct,
# sans passage par la console ni par cmd).
function Exec-Rose([string]$exe, [string[]]$params) {
    $tmp  = [IO.Path]::GetTempFileName()
    $err  = "$tmp.err"
    try {
        $p = Start-Process -FilePath $exe -ArgumentList $params `
             -NoNewWindow -Wait -PassThru `
             -RedirectStandardOutput $tmp -RedirectStandardError $err
        $code = $p.ExitCode
        $octets = [IO.File]::ReadAllBytes($tmp)
        $errtxt = [IO.File]::ReadAllText($err, [Text.Encoding]::UTF8)
    } catch {
        $code = -1; $octets = @(); $errtxt = ""
    }
    Remove-Item $tmp, $err -Force -ErrorAction SilentlyContinue
    return @{ code = $code; octets = $octets; errtxt = $errtxt }
}

# Normalise \r\n → \n (uniquement cette paire ; un \r seul est conservé).
function Normalise-Lf([byte[]]$b) {
    $ms = New-Object IO.MemoryStream
    for ($i = 0; $i -lt $b.Length; $i++) {
        if ($b[$i] -eq 13 -and $i + 1 -lt $b.Length -and $b[$i + 1] -eq 10) { continue }
        $ms.WriteByte($b[$i])
    }
    return $ms.ToArray()
}

function Identiq([byte[]]$attendu, [byte[]]$obtenu) {
    return ([Convert]::ToBase64String((Normalise-Lf $attendu)) -ceq
            [Convert]::ToBase64String((Normalise-Lf $obtenu)))
}

function Texte([byte[]]$b) { return [Text.Encoding]::UTF8.GetString($b) }

# Fichier .rose écrit en CRLF (« édité sous Windows »).
function Ecrire-Crlf([string]$chemin, [string]$contenu) {
    $crlf = ($contenu -replace "`r?`n", "`r`n")
    [IO.File]::WriteAllBytes($chemin, [Text.Encoding]::UTF8.GetBytes($crlf))
}

$moteurs = @(
    @{ exe = "bin\rose-win64.exe";     nom = "C++  "; bundle = $true;  ffi = $true  },
    @{ exe = "bin\rose-rs-win64.exe";  nom = "Rust "; bundle = $true;  ffi = $true  },
    @{ exe = "bin\rose-zig-win64.exe"; nom = "Zig  "; bundle = $false; ffi = $false }
)

Write-Host "== Rose — auto-test Windows x64 ==" -ForegroundColor Cyan
$v = Exec-Rose (Join-Path $Racine $moteurs[0].exe) @("version")
Write-Host ("moteur référence : " + (Texte $v.octets).Trim())
Write-Host ("machine : " + (Get-CimInstance Win32_OperatingSystem).Caption)

foreach ($m in $moteurs) {
    $exe = Join-Path $Racine $m.exe
    $nom = $m.nom
    Write-Host ""
    Write-Host "-- moteur $nom ($($m.exe)) --"
    if (-not (Test-Path $exe)) { Echec "$nom : binaire absent"; continue }

    # 1) version
    $r = Exec-Rose $exe @("version")
    if ($r.code -eq 0 -and (Texte $r.octets) -match "Rose 2\.0\.0") { Succes "$nom version" }
    else { Echec "$nom version (code $($r.code))" }

    # 2) dorés au caractère
    foreach ($paire in @(@("t_exact", "t_exact_c.txt"), @("t_machine", "out_c.txt"), @("alias", "alias_c.txt"))) {
        $attendu = [IO.File]::ReadAllBytes((Join-Path $dore $paire[1]))
        $prog = [IO.Path]::Combine($dore, ($paire[0] + ".rose"))
        $r = Exec-Rose $exe @($prog)
        if ($r.code -eq 0 -and (Identiq $attendu $r.octets)) { Succes "$nom doré $($paire[0])" }
        else { Echec "$nom doré $($paire[0]) (code $($r.code))" }
    }

    # 2b) v3 : briques de l'auto-hébergement (Unicode, sortie anticipée)
    $attendu = [IO.File]::ReadAllBytes((Join-Path $dore "t_v3_c.txt"))
    $r = Exec-Rose $exe @([IO.Path]::Combine($dore, "t_v3.rose"))
    if ($r.code -eq 0 -and (Identiq $attendu $r.octets)) { Succes "$nom doré v3 chaînes" }
    else { Echec "$nom doré v3 chaînes (code $($r.code))" }
    $r = Exec-Rose $exe @([IO.Path]::Combine($dore, "t_v3_quitter.rose"))
    if ($r.code -eq 3 -and (Texte (Normalise-Lf $r.octets)) -ceq "avant`n2`n") { Succes "$nom quitter(3) : code 3 et sorties préservées" }
    else { Echec "$nom quitter (code $($r.code))" }
    $r = Exec-Rose $exe @([IO.Path]::Combine($dore, "t_v3_idx.rose"))
    if ($r.code -eq 1) { Succes "$nom index chaîne hors limites → code 1" }
    else { Echec "$nom index chaîne (code $($r.code))" }

    # 2c) v3.2 : notation symbolique et arithmétique mixte
    $attendu = [IO.File]::ReadAllBytes((Join-Path $dore "t_symboles_c.txt"))
    $r = Exec-Rose $exe @([IO.Path]::Combine($dore, "t_symboles.rose"))
    if ($r.code -eq 0 -and (Identiq $attendu $r.octets)) { Succes "$nom doré v3.2 symboles" }
    else { Echec "$nom doré v3.2 symboles (code $($r.code))" }
    $attendu = [IO.File]::ReadAllBytes((Join-Path $dore "t_mixtes_c.txt"))
    $r = Exec-Rose $exe @([IO.Path]::Combine($dore, "t_mixtes.rose"))
    if ($r.code -eq 0 -and (Identiq $attendu $r.octets)) { Succes "$nom doré v3.2 mixtes rationnel-flottant" }
    else { Echec "$nom doré v3.2 mixtes (code $($r.code))" }

    # 2d) v3.1 : tables
    $attendu = [IO.File]::ReadAllBytes((Join-Path $dore "t_tables_c.txt"))
    $r = Exec-Rose $exe @([IO.Path]::Combine($dore, "t_tables.rose"))
    if ($r.code -eq 0 -and (Identiq $attendu $r.octets)) { Succes "$nom doré v3.1 tables" }
    else { Echec "$nom doré v3.1 tables (code $($r.code))" }

    # 2e) roses à tester (moteur\tests\roses) — torture multi-features M4.2.
    # Les .attendu comparent l'octet ; les .attendu.err comparent le code de
    # sortie et le message d'erreur (stdout+stderr confondus).
    $roses = Join-Path $Racine "moteur\tests\roses"
    if (Test-Path $roses) {
        foreach ($rose in (Get-ChildItem $roses -Filter *.rose | Sort-Object Name)) {
            $base = [IO.Path]::GetFullPath([IO.Path]::Combine($roses, $rose.BaseName))
            $r = Exec-Rose $exe @([IO.Path]::GetFullPath($rose.FullName))
            if (Test-Path "$base.attendu") {
                $attendu = [IO.File]::ReadAllBytes("$base.attendu")
                if ($r.code -eq 0 -and (Identiq $attendu $r.octets)) { Succes "$nom rose $($rose.BaseName)" }
                else { Echec "$nom rose $($rose.BaseName) (code $($r.code))" }
            } elseif (Test-Path "$base.attendu.err") {
                $lignes = (Texte ([IO.File]::ReadAllBytes("$base.attendu.err"))) -split "`n"
                $codeVoulu = 0
                $premier = ($lignes[0] -replace "`r", "")
                if ($premier -match "^CODE=(\d+)$") { $codeVoulu = [int]$Matches[1] }
                $motif = (($lignes | Select-Object -Skip 1) -join "`n").Trim()
                $obtenu = (((Texte $r.octets) + "`n" + $r.errtxt) -replace "`r", "").Trim()
                if ($r.code -eq $codeVoulu -and $obtenu -like "*$motif*") { Succes "$nom rose $($rose.BaseName) (erreur attendue, code $codeVoulu)" }
                else { Echec "$nom rose $($rose.BaseName) (code $($r.code), attendu $codeVoulu)" }
            }
        }
    }

    # 3) CLI : usage et fichier absent → code 1
    $r = Exec-Rose $exe @()
    if ($r.code -eq 1) { Succes "$nom usage sans argument (code 1)" } else { Echec "$nom usage (code $($r.code))" }
    $r = Exec-Rose $exe @("programme_inexistant.rose")
    if ($r.code -eq 1) { Succes "$nom fichier introuvable (code 1)" } else { Echec "$nom fichier introuvable (code $($r.code))" }

    # 4) source en CRLF : le même programme que t_exact, édition Windows
    $crlf = Join-Path $env:TEMP "rose_test_crlf.rose"
    Ecrire-Crlf $crlf ([IO.File]::ReadAllText((Join-Path $dore "t_exact.rose"), [Text.Encoding]::UTF8))
    $attendu = [IO.File]::ReadAllBytes((Join-Path $dore "t_exact_c.txt"))
    $r = Exec-Rose $exe @($crlf)
    Remove-Item $crlf -Force
    if ($r.code -eq 0 -and (Identiq $attendu $r.octets)) { Succes "$nom source CRLF identique au doré" }
    else { Echec "$nom source CRLF (code $($r.code))" }

    # 5) récursion profonde (pile dédiée 512 Mo du moteur)
    $rec = Join-Path $env:TEMP "rose_test_rec.rose"
    [IO.File]::WriteAllBytes($rec, [Text.Encoding]::UTF8.GetBytes("fn compte(n: entier) -> entier {`n    si (n == 0) { retour 0 }`n    retour 1 + compte(n - 1)`n}`n>> compte(20000)`n"))
    $r = Exec-Rose $exe @($rec)
    Remove-Item $rec -Force
    if ($r.code -eq 0 -and (Texte $r.octets).Trim() -eq "20000") { Succes "$nom récursion 20 000" }
    else { Echec "$nom récursion 20 000 (code $($r.code), sortie '$((Texte $r.octets).Trim())')" }

    # 6) bundle autonome (C++/Rust) : comp → .exe seul
    if ($m.bundle) {
        $bin = Join-Path $env:TEMP "rose_test_bundle"
        Remove-Item "$bin.exe" -Force -ErrorAction SilentlyContinue
        $prog = [IO.Path]::Combine($dore, "t_exact.rose")
        $null = Exec-Rose $exe @("comp", $prog, "-o", $bin)
        if (Test-Path "$bin.exe") {
            $attendu = [IO.File]::ReadAllBytes((Join-Path $dore "t_exact_c.txt"))
            $r2 = Exec-Rose "$bin.exe" @()
            Remove-Item "$bin.exe" -Force
            if ($r2.code -eq 0 -and (Identiq $attendu $r2.octets)) { Succes "$nom bundle autonome (comp → .exe)" }
            else { Echec "$nom bundle (exécution, code $($r2.code))" }
        } else { Echec "$nom bundle : .exe non produit" }
    }

    # 7) FFI vers la CRT de Windows (C++/Rust) : pow de msvcrt
    if ($m.ffi) {
        $ffi = Join-Path $env:TEMP "rose_test_ffi.rose"
        [IO.File]::WriteAllBytes($ffi, [Text.Encoding]::UTF8.GetBytes("@dll(`"msvcrt`")`n@sym(`"pow`")`nfn p(x: flottant, y: flottant) -> flottant`n>> p(2.0, 10.0)`n"))
        $r = Exec-Rose $exe @($ffi)
        Remove-Item $ffi -Force
        if ($r.code -eq 0 -and (Texte $r.octets).Trim() -eq "1024") { Succes "$nom FFI @dll msvcrt::pow" }
        else { Echec "$nom FFI msvcrt (code $($r.code), sortie '$((Texte $r.octets).Trim())')" }
    }
}

Write-Host ""
if ($ko -eq 0) {
    Write-Host "TOUT EST VERT ($ok tests) — Rose est opérationnel sur cette machine." -ForegroundColor Green
    exit 0
} else {
    Write-Host "BILAN : $ok réussis, $ko échoués." -ForegroundColor Red
    exit 1
}
