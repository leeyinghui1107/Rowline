#Requires -Version 5
<#
.SYNOPSIS
    Run cargo with a working Rust toolchain.

.DESCRIPTION
    Rust is often installed under a custom CARGO_HOME (this machine uses
    D:\cargo) and is not on PATH.  The rustup shim also needs RUSTUP_HOME when
    it has no default toolchain.  This wrapper finds a cargo that actually
    reports a version, sets the environment for the child process and
    forwards every argument.

.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File tools/win/cargo.ps1 build --release -p rowline-app
#>
$ErrorActionPreference = 'Stop'

$candidates = @()
$cmd = Get-Command cargo -ErrorAction SilentlyContinue
if ($cmd) { $candidates += $cmd.Source }
foreach ($home in @($env:CARGO_HOME, 'C:\cargo', 'D:\cargo', "$env:USERPROFILE\.cargo")) {
    if (-not $home) { continue }
    $candidates += Join-Path $home 'bin\cargo.exe'
}
$candidates += Get-ChildItem 'C:\cargo\bin\cargo.exe', 'D:\cargo\bin\cargo.exe' `
    -ErrorAction SilentlyContinue | ForEach-Object { $_.FullName }

foreach ($exe in ($candidates | Select-Object -Unique)) {
    if (-not (Test-Path $exe)) { continue }
    $cargoHome = Split-Path -Parent (Split-Path -Parent $exe)
    if (-not $env:CARGO_HOME -and
        ((Split-Path -Leaf $cargoHome) -in @('.cargo', 'cargo') -or
         (Test-Path (Join-Path $cargoHome 'registry')))) {
        $env:CARGO_HOME = $cargoHome
    }
    if (-not $env:RUSTUP_HOME) {
        foreach ($rustup in @((Join-Path (Split-Path -Parent $cargoHome) 'rustup'),
                             'C:\rustup', 'D:\rustup', "$env:USERPROFILE\.rustup")) {
            if (Test-Path (Join-Path $rustup 'toolchains')) {
                $env:RUSTUP_HOME = $rustup
                break
            }
        }
    }
    $env:PATH = (Split-Path -Parent $exe) + ';' + $env:PATH
    & $exe --version 2>$null | Out-Null
    if ($LASTEXITCODE -eq 0) {
        & $exe @args
        exit $LASTEXITCODE
    }
}

throw "cargo not found. Install Rust from https://rustup.rs/ (or set CARGO_HOME)."
