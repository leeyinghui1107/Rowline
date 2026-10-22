#Requires -Version 5
<#
.SYNOPSIS
    Run one of the repository's Python tools with a working interpreter.

.DESCRIPTION
    On Windows `python3` usually does not exist, and the `python.exe` in
    WindowsApps is a Store alias that exits with code 9009 instead of running
    anything.  This wrapper finds a real Python 3 (PATH, `py -3`, a per-user
    install, or Program Files) and forwards every argument to it, so the
    Makefile can simply call:

        powershell -NoProfile -ExecutionPolicy Bypass -File tools/win/py.ps1 tools/build_all.py

.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File tools/win/py.ps1 tools/package.py --out dist --zip
#>
$ErrorActionPreference = 'Stop'

$argumentsIn = @($args)
if ($argumentsIn.Count -eq 0) {
    throw "usage: py.ps1 <script.py> [args...]"
}

$candidates = @()
foreach ($name in 'python', 'python3') {
    $cmd = Get-Command $name -ErrorAction SilentlyContinue
    if ($cmd) { $candidates += $cmd.Source }
}
$py = Get-Command py -ErrorAction SilentlyContinue
if ($py) { $candidates += $py.Source }
$candidates += Get-ChildItem "$env:LOCALAPPDATA\Programs\Python\Python*\python.exe" `
    -ErrorAction SilentlyContinue | ForEach-Object { $_.FullName }
$candidates += Get-ChildItem "$env:ProgramFiles\Python*\python.exe" `
    -ErrorAction SilentlyContinue | ForEach-Object { $_.FullName }
$candidates += Get-ChildItem "$env:ProgramFiles\Python3*\python.exe" `
    -ErrorAction SilentlyContinue | ForEach-Object { $_.FullName }

foreach ($exe in ($candidates | Select-Object -Unique)) {
    if (-not (Test-Path $exe)) { continue }
    # The Store alias passes `Test-Path` but cannot run a script; this probe
    # is what tells the two apart.
    & $exe -c "import sys" 2>$null
    if ($LASTEXITCODE -eq 0) {
        & $exe @argumentsIn
        exit $LASTEXITCODE
    }
}

throw "No working Python 3 found. Install it from https://www.python.org/downloads/"
