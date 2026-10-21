<#
.SYNOPSIS
    Windows toolchain verification for Rowline.

.DESCRIPTION
    Runs the checks listed in docs/temp/WINDOWS_DEV_CHECKLIST.md section 9:

      * cargo build / cargo test (release, workspace, all features)
      * make test  -> layout gate PASSED + four-way gate ALL MATCH
      * make shot  -> offline render writes build/shot.png
      * winprobe.c compiles with the available C compiler

    Every command is teed to a log file.  A machine-readable summary is
    written to blackbox/windows_verify/<timestamp>/summary.json so the
    checklist can cite it as evidence.

    The script does not install anything.  It only locates an existing
    Python 3, Rust (cargo), and a MinGW gcc, then runs the gates.

.PARAMETER Python
    Path to a real python.exe.  Default: py -3, then python, then the newest
    %LOCALAPPDATA%\Programs\Python\Python3*\python.exe.

.PARAMETER OutDir
    Evidence directory.  Default: blackbox/windows_verify/<timestamp>.

.PARAMETER RustupHome
.PARAMETER CargoHome
    Override RUSTUP_HOME / CARGO_HOME.  Needed when the Windows user profile
    path contains non-ASCII characters: the MinGW linker cannot open the
    sysroot rlibs through such a path and proc-macro crates fail to link.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File tools\win\verify.ps1
#>
[CmdletBinding()]
param(
    [string]$Python,
    [string]$OutDir,
    [string]$RustupHome,
    [string]$CargoHome
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$Stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
if (-not $OutDir) { $OutDir = Join-Path $RepoRoot "blackbox\windows_verify\$Stamp" }
$OutDir = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($OutDir)
$LogDir = Join-Path $OutDir 'logs'
New-Item -ItemType Directory -Force -Path $LogDir | Out-Null

$Results = New-Object System.Collections.Generic.List[object]
$Versions = [ordered]@{}

function Write-Step([string]$Text) {
    Write-Host ''
    Write-Host "=== $Text ===" -ForegroundColor Cyan
}

function Add-Result([string]$Name, [bool]$Ok, [string]$Detail, [string]$Log) {
    $Results.Add([pscustomobject]@{
        name   = $Name
        ok     = $Ok
        detail = $Detail
        log    = if ($Log) { $Log.Substring($RepoRoot.Length).TrimStart('\') } else { $null }
    })
    $color = if ($Ok) { 'Green' } else { 'Red' }
    Write-Host ("  {0}  {1}" -f $(if ($Ok) { 'PASS' } else { 'FAIL' }), $Name) -ForegroundColor $color
    if ($Detail) { Write-Host "        $Detail" }
}

# ------------------------------------------------------------------ tool lookup
function Test-RealPython([string]$Exe) {
    if (-not $Exe) { return $false }
    if (-not (Test-Path -LiteralPath $Exe)) { return $false }
    # The Microsoft Store alias in WindowsApps is a 0-byte reparse point that
    # opens the Store instead of running Python.
    if ($Exe -match '\\WindowsApps\\python') { return $false }
    return $true
}

function Find-Python {
    if ($Python) {
        if (Test-RealPython $Python) { return (Resolve-Path -LiteralPath $Python).Path }
        throw "the -Python path is not a usable interpreter: $Python"
    }
    $py = Get-Command py -ErrorAction SilentlyContinue
    if ($py) {
        try {
            $candidate = & $py.Source -3 -c "import sys; print(sys.executable)" 2>$null
            if ($LASTEXITCODE -eq 0 -and (Test-RealPython $candidate)) { return $candidate }
        } catch { }
    }
    $candidate = (Get-Command python -ErrorAction SilentlyContinue).Source
    if (Test-RealPython $candidate) { return $candidate }

    $roots = @(
        (Join-Path $env:LOCALAPPDATA 'Programs\Python'),
        'C:\Python314', 'C:\Python313', 'C:\Python312', 'C:\Python311'
    )
    $found = @()
    foreach ($root in $roots) {
        if (Test-Path $root) {
            $found += Get-ChildItem $root -Filter python.exe -Recurse -ErrorAction SilentlyContinue |
                Where-Object { $_.FullName -match 'Python3\d+\\python\.exe$' } |
                Select-Object -ExpandProperty FullName
        }
    }
    if ($found.Count -gt 0) { return ($found | Sort-Object -Descending | Select-Object -First 1) }
    throw "no real Python 3 found. Install it, or pass -Python <path>. " +
          "The 'python.exe' in WindowsApps is a Store alias, not an interpreter."
}

function Find-CCompiler {
    $gcc = Get-Command gcc -ErrorAction SilentlyContinue
    if ($gcc) { return $gcc.Source }
    foreach ($p in @('C:\msys64\ucrt64\bin\gcc.exe', 'C:\msys64\mingw64\bin\gcc.exe')) {
        if (Test-Path $p) { return $p }
    }
    # The Makefile uses gcc-style flags (-std=c99, -I, -o), so cl.exe is not a
    # drop-in substitute here.
    return $null
}

# ------------------------------------------------------------------ process helper
function Quote-Arg([string]$Value) {
    if ($Value -match '[\s"]') { return '"' + ($Value -replace '"', '\"') + '"' }
    return $Value
}

# Run a native command with stdout and stderr both captured to one log.
# Using System.Diagnostics.Process avoids Windows PowerShell 5.1 turning a
# native stderr line into a terminating NativeCommandError.
function Invoke-Native {
    param(
        [string]$Exe,
        [string[]]$Arguments,
        [string]$Log,
        [string]$WorkDir = $RepoRoot
    )
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $Exe
    $psi.Arguments = ($Arguments | ForEach-Object { Quote-Arg $_ }) -join ' '
    $psi.WorkingDirectory = $WorkDir
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true
    $psi.StandardOutputEncoding = [System.Text.Encoding]::UTF8
    $psi.StandardErrorEncoding = [System.Text.Encoding]::UTF8
    $p = [System.Diagnostics.Process]::Start($psi)
    # Read both pipes concurrently; reading one to completion first can
    # deadlock when the child fills the other pipe's buffer.
    $outTask = $p.StandardOutput.ReadToEndAsync()
    $errTask = $p.StandardError.ReadToEndAsync()
    $p.WaitForExit()
    $out = $outTask.Result
    $err = $errTask.Result
    $text = $out
    if ($err) { $text += $err }
    [System.IO.File]::WriteAllText($Log, $text, (New-Object System.Text.UTF8Encoding($false)))
    Write-Host $text -NoNewline
    return $p.ExitCode
}

function Invoke-Step {
    param(
        [string]$Name,
        [string]$Exe,
        [string[]]$Arguments,
        [string]$LogName,
        [string]$WorkDir = $RepoRoot
    )
    Write-Step $Name
    $log = Join-Path $LogDir "$LogName.log"
    Write-Host ("  " + $Exe + " " + ($Arguments -join ' ')) -ForegroundColor DarkGray
    $code = Invoke-Native -Exe $Exe -Arguments $Arguments -Log $log -WorkDir $WorkDir
    $tail = ''
    if (Test-Path $log) {
        $tail = (Get-Content $log -Encoding UTF8 | Where-Object { $_.Trim() } |
                 Select-Object -Last 3) -join ' | '
    }
    Add-Result $Name ($code -eq 0) "exit=$code  $tail" $log
    return ($code -eq 0)
}

# ------------------------------------------------------------------ environment
$PythonExe = Find-Python
$env:PYTHON = $PythonExe
$env:PYTHONIOENCODING = 'utf-8'

# Keep the Rust toolchain inside an ASCII-only path.  GNU ld on Windows cannot
# link proc-macro rlibs when RUSTUP_HOME sits under a user profile whose name
# contains non-ASCII characters.
if (-not $RustupHome) { $RustupHome = $env:RUSTUP_HOME }
if (-not $CargoHome) { $CargoHome = $env:CARGO_HOME }
if (-not $RustupHome -and $env:USERPROFILE -match '[^\x00-\x7F]') {
    foreach ($pair in @(@('D:\rustup'), @('C:\rustup'))) {
        if (Test-Path (Join-Path $pair[0] 'toolchains')) { $RustupHome = $pair[0]; break }
    }
}
if (-not $CargoHome -and $env:USERPROFILE -match '[^\x00-\x7F]') {
    foreach ($pair in @(@('D:\cargo'), @('C:\cargo'))) {
        if (Test-Path (Join-Path $pair[0] 'bin\cargo.exe')) { $CargoHome = $pair[0]; break }
    }
}
if ($RustupHome) { $env:RUSTUP_HOME = $RustupHome }
if ($CargoHome) { $env:CARGO_HOME = $CargoHome }

$CC = Find-CCompiler
if (-not $CC) { throw "no gcc found. Install MSYS2/MinGW (the Makefile uses gcc-style flags), or add gcc to PATH." }
$ccDir = Split-Path -Parent $CC
$env:PATH = "$ccDir;$env:PATH"

$cargo = Get-Command cargo -ErrorAction SilentlyContinue
if (-not $cargo -and $CargoHome) {
    $guess = Join-Path $CargoHome 'bin\cargo.exe'
    if (Test-Path $guess) { $cargo = Get-Item $guess }
}
if (-not $cargo -and $env:USERPROFILE -notmatch '[^\x00-\x7F]') {
    # Ordinary rustup install with an ASCII profile and no PATH entry yet.
    $guess = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
    if (Test-Path $guess) { $cargo = Get-Item $guess }
}
if (-not $cargo) { throw "cargo not found. Install rustup, or pass -CargoHome." }
$cargoExe = if ($cargo -is [System.Management.Automation.ApplicationInfo]) { $cargo.Source } else { $cargo.FullName }
$env:PATH = "$(Split-Path -Parent $cargoExe);$env:PATH"

$make = Get-Command make -ErrorAction SilentlyContinue
if (-not $make) {
    $guess = Join-Path (Split-Path -Parent $ccDir) 'bin\make.exe'
    if (Test-Path $guess) { $make = Get-Item $guess }
}
if (-not $make) { throw "make not found (install MSYS2 make, or add it to PATH)." }
$makeExe = if ($make -is [System.Management.Automation.ApplicationInfo]) { $make.Source } else { $make.FullName }

# ------------------------------------------------------------------ versions
Write-Step 'versions'
$versions = [ordered]@{
    python = (& $PythonExe --version 2>&1) -join ' '
    cargo  = (& $cargoExe --version 2>&1) -join ' '
    rustc  = (& (Join-Path (Split-Path -Parent $cargoExe) 'rustc.exe') --version 2>&1) -join ' '
    cc     = (& $CC --version 2>&1 | Select-Object -First 1) -join ' '
    make   = (& $makeExe --version 2>&1 | Select-Object -First 1) -join ' '
    os     = [System.Environment]::OSVersion.VersionString
    host   = $env:COMPUTERNAME
    rustup_home = $env:RUSTUP_HOME
    cargo_home  = $env:CARGO_HOME
}
$versions.GetEnumerator() | ForEach-Object { Write-Host ("  {0,-12} {1}" -f $_.Key, $_.Value) }
$Versions = $versions

# ------------------------------------------------------------------ gates
$cargoDir = Join-Path $RepoRoot 'rust'

$ok = $true
$ok = (Invoke-Step 'cargo build --release --workspace --all-features' `
        $cargoExe @('build', '--release', '--workspace', '--all-features') `
        'cargo-build' $cargoDir) -and $ok

$ok = (Invoke-Step 'cargo test --release --workspace --all-features' `
        $cargoExe @('test', '--release', '--workspace', '--all-features') `
        'cargo-test' $cargoDir) -and $ok

$ok = (Invoke-Step 'make test (layout gate + four-way differential gate)' `
        $makeExe @("PYTHON=$PythonExe", "CC=$CC", 'test') `
        'make-test') -and $ok

$ok = (Invoke-Step 'make shot (offline render)' `
        $makeExe @("PYTHON=$PythonExe", "CC=$CC", 'shot') `
        'make-shot') -and $ok

# The render gate is only useful if a real PNG came out.
Write-Step 'check build/shot.png'
$shot = Join-Path $RepoRoot 'build\shot.png'
if (Test-Path $shot) {
    $bytes = [System.IO.File]::ReadAllBytes($shot)
    $w = ([int]$bytes[16] -shl 24) -bor ([int]$bytes[17] -shl 16) -bor ([int]$bytes[18] -shl 8) -bor [int]$bytes[19]
    $h = ([int]$bytes[20] -shl 24) -bor ([int]$bytes[21] -shl 16) -bor ([int]$bytes[22] -shl 8) -bor [int]$bytes[23]
    $pngOk = ($bytes.Length -gt 10000) -and ($w -eq 1280) -and ($h -eq 900)
    Add-Result 'build/shot.png is a non-trivial 1280x900 PNG' $pngOk ("{0}x{1}, {2} bytes" -f $w, $h, $bytes.Length) $null
    $ok = $pngOk -and $ok
} else {
    Add-Result 'build/shot.png exists' $false 'missing' $null
    $ok = $false
}

# winprobe is the runtime collector used by the black-box phase.  Compiling it
# here proves the Windows-side toolchain can build the collector itself.
Write-Step 'compile tools/blackbox/winprobe.c'
$winprobe = Join-Path $RepoRoot 'build\winprobe.exe'
$compileLog = Join-Path $LogDir 'winprobe.log'
$wpArgs = @('-O2', '-Wall', '-o', $winprobe,
            (Join-Path $RepoRoot 'tools\blackbox\winprobe.c'))
$wpCode = Invoke-Native -Exe $CC -Arguments $wpArgs -Log $compileLog
$wpOk = ($wpCode -eq 0) -and (Test-Path $winprobe)
Add-Result 'winprobe.c compiles' $wpOk "exit=$wpCode  -> $winprobe" $compileLog
$ok = $wpOk -and $ok

# Exercise the black-box pipeline end to end without the original executable:
# simulate.py draws a mock screenshot from the re-drawn sheet, analyze.py
# measures it.  This catches geometry/pngio/UTF-8 regressions on Windows.
Write-Step 'black-box pipeline (simulate -> analyze shot)'
$mock = Join-Path $OutDir 'mock_centred.png'
$shotDir = Join-Path $OutDir 'shot'
$simLog = Join-Path $LogDir 'simulate.log'
$analyzeLog = Join-Path $LogDir 'analyze-shot.log'
$simArgs = @('tools/blackbox/simulate.py',
             '--sheet', 'assets/sheet/rowline_sheet.png',
             '--out', $mock, '--size', '1024x768', '--seed', '7',
             '--layout', 'centred',
             '--config', (Join-Path $OutDir 'config.json'))
$simCode = Invoke-Native -Exe $PythonExe -Arguments $simArgs -Log $simLog
$anArgs = @('tools/blackbox/analyze.py', 'shot', $mock, '--out', $shotDir)
$anCode = Invoke-Native -Exe $PythonExe -Arguments $anArgs -Log $analyzeLog
$centredOk = $false
$detail = "simulate exit=$simCode, analyze exit=$anCode"
$shotJson = Join-Path $shotDir 'conformance_shot.json'
if ((Test-Path $shotJson) -and $anCode -eq 0) {
    $report = Get-Content $shotJson -Raw -Encoding UTF8 | ConvertFrom-Json
    $centred = $report.checks | Where-Object { $_.name -eq 'the centred hypothesis fits' }
    $colours = $report.checks | Where-Object { $_.name -eq 'distinct colours observed' }
    $centredOk = [bool]$centred.ok -and [bool]$colours.ok
    $detail = "centred delta=$($centred.measured); colours=$($colours.measured)"
}
Add-Result 'black-box simulator matches the centred layout' $centredOk $detail $analyzeLog
$ok = $centredOk -and $ok

# The 54-item checklist must load as UTF-8 and stay self-consistent.
Write-Step 'black-box checklist (summary + consistency)'
$sumLog = Join-Path $LogDir 'checklist-summary.log'
$chkLog = Join-Path $LogDir 'checklist-check.log'
$sumCode = Invoke-Native -Exe $PythonExe -Arguments @('tools/blackbox/checklist.py', 'summary') -Log $sumLog
$chkCode = Invoke-Native -Exe $PythonExe -Arguments @('tools/blackbox/checklist.py', 'check') -Log $chkLog
$chkOk = ($sumCode -eq 0) -and ($chkCode -eq 0)
Add-Result 'checklist.py loads and is consistent' $chkOk "summary exit=$sumCode, check exit=$chkCode" $sumLog
$ok = $chkOk -and $ok

# Read the real per-monitor DPI through the platform layer.  This is the
# product-side check from the checklist; it needs a desktop session.
Write-Step 'rowline-dpi-probe (real display DPI)'
$dpiLog = Join-Path $LogDir 'dpi-probe.log'
$dpiArgs = @('run', '--release', '-p', 'rowline-tools', '--features', 'dpi',
             '--bin', 'rowline-dpi-probe')
$dpiCode = Invoke-Native -Exe $cargoExe -Arguments $dpiArgs -Log $dpiLog -WorkDir $cargoDir
$dpiText = if (Test-Path $dpiLog) { Get-Content $dpiLog -Raw -Encoding UTF8 } else { '' }
$dpiOk = ($dpiCode -eq 0) -and ($dpiText -match 'monitor\(s\)')
$monitor = ([regex]::Match($dpiText, '(?m)^\s+monitor\s+(\S+)\s+(\d+x\d+)\s+px\s+scale\s+([\d.]+)'))
$dpiDetail = if ($monitor.Success) {
    "$($monitor.Groups[1].Value) $($monitor.Groups[2].Value)px scale $($monitor.Groups[3].Value)"
} else {
    "exit=$dpiCode"
}
Add-Result 'rowline-dpi-probe reads the display environment' $dpiOk $dpiDetail $dpiLog
$ok = $dpiOk -and $ok

# ------------------------------------------------------------------ summary
Write-Step 'summary'
$total = $Results.Count
$passed = @($Results | Where-Object ok).Count
$summary = [ordered]@{
    generated_at = (Get-Date).ToString('o')
    repo         = $RepoRoot
    ok           = $ok
    passed       = $passed
    total        = $total
    versions     = $Versions
    checks       = $Results
}
$summaryPath = Join-Path $OutDir 'summary.json'
$summary | ConvertTo-Json -Depth 5 | Set-Content -Encoding UTF8 $summaryPath

Write-Host ''
Write-Host ("{0}/{1} checks passed" -f $passed, $total) -ForegroundColor $(if ($ok) { 'Green' } else { 'Red' })
Write-Host "evidence: $OutDir"
if (-not $ok) { exit 1 }
exit 0
