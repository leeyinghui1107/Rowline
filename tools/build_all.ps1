#Requires -Version 5
<#
.SYNOPSIS
    Windows entry point for tools/build_all.py.

.DESCRIPTION
    Finds a real Python 3 interpreter (the Windows Store `python` alias is a
    stub that exits with code 49) and forwards every argument to build_all.py.

.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File tools\build_all.ps1 --list
    powershell -NoProfile -ExecutionPolicy Bypass -File tools\build_all.ps1
#>
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot

$candidates = @()
$python = Get-Command python -ErrorAction SilentlyContinue
if ($python) { $candidates += $python.Source }
$py = Get-Command py -ErrorAction SilentlyContinue
if ($py) { $candidates += $py.Source }
$candidates += Get-ChildItem "$env:LOCALAPPDATA\Programs\Python\Python*\python.exe" `
    -ErrorAction SilentlyContinue | ForEach-Object { $_.FullName }
$candidates += Get-ChildItem "$env:ProgramFiles\Python*\python.exe" `
    -ErrorAction SilentlyContinue | ForEach-Object { $_.FullName }

foreach ($exe in ($candidates | Select-Object -Unique)) {
    if (-not (Test-Path $exe)) { continue }
    & $exe -c "import sys" 2>$null
    if ($LASTEXITCODE -eq 0) {
        & $exe (Join-Path $repo 'tools\build_all.py') @args
        exit $LASTEXITCODE
    }
}

throw "No working Python 3 found. Install it from https://www.python.org/downloads/"
