param([switch]$Test, [switch]$BuildOnly)

$ErrorActionPreference = 'Stop'
$projectPath = $PSScriptRoot
$chatPath = Split-Path (Split-Path $projectPath -Parent) -Parent
$portablePath = Join-Path $chatPath 'work\rust-portable'

$env:RUSTFLAGS = "-D warnings"

if (Test-Path -LiteralPath (Join-Path $portablePath 'bin\cargo.exe')) {
    $env:PATH = "$portablePath\bin;$portablePath\lib\rustlib\x86_64-pc-windows-gnu\bin\self-contained;" + $env:PATH
    $env:CARGO_HOME = Join-Path $chatPath 'work\cargo-home'
    $env:CARGO_TARGET_DIR = Join-Path $chatPath 'work\rust-build'
    $cargoExecutable = Join-Path $portablePath 'bin\cargo.exe'
    & $cargoExecutable build --release --locked --offline --manifest-path (Join-Path $projectPath 'Cargo.toml')
    if ($LASTEXITCODE -ne 0) { throw 'La compilation a echoue.' }
    if ($Test) {
        & $cargoExecutable test --release --locked --offline --manifest-path (Join-Path $projectPath 'Cargo.toml')
        if ($LASTEXITCODE -ne 0) { throw 'Un test a echoue.' }
    }
    if (!$BuildOnly) { 
        Copy-Item -LiteralPath (Join-Path $env:CARGO_TARGET_DIR 'release\vcs-rust.exe') -Destination (Join-Path $projectPath 'VCS-Rust.exe') -Force 
    }
} else {
    & cargo build --release --locked --manifest-path (Join-Path $projectPath 'Cargo.toml')
    if ($LASTEXITCODE -ne 0) { throw 'La compilation a echoue.' }
    if ($Test) {
        & cargo test --release --locked --manifest-path (Join-Path $projectPath 'Cargo.toml')
        if ($LASTEXITCODE -ne 0) { throw 'Un test a echoue.' }
    }
    if (!$BuildOnly) { 
        Copy-Item -LiteralPath (Join-Path $projectPath 'target\release\vcs-rust.exe') -Destination (Join-Path $projectPath 'VCS-Rust.exe') -Force 
    }
}

# Synchronisation automatique des badges et de la progression
$progressScript = Join-Path $projectPath 'tools\progress\progress.py'
if (Test-Path -LiteralPath $progressScript) {
    try {
        & python $progressScript
    } catch {
        Write-Warning "Impossible d'executer le script de progression: $_"
    }
}

if ($BuildOnly) { 
    Write-Host 'Compilation prete pour verification interne.' 
} else { 
    Write-Host 'VCS-Rust.exe est pret.' 
}
