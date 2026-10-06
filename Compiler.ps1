$ErrorActionPreference = 'Stop'
$projectPath = $PSScriptRoot
$chatPath = Split-Path (Split-Path $projectPath -Parent) -Parent
$portablePath = Join-Path $chatPath 'work\rust-portable'
if (Test-Path -LiteralPath (Join-Path $portablePath 'bin\cargo.exe')) {
    $env:PATH = "$portablePath\bin;$portablePath\lib\rustlib\x86_64-pc-windows-gnu\bin\self-contained;" + $env:PATH
    $env:CARGO_HOME = Join-Path $chatPath 'work\cargo-home'
    $env:CARGO_TARGET_DIR = Join-Path $chatPath 'work\rust-build'
    $cargoExecutable = Join-Path $portablePath 'bin\cargo.exe'
    # Dependencies used in this chat are already cached. No local bridge is needed.
    & $cargoExecutable build --release --locked --offline --manifest-path (Join-Path $projectPath 'Cargo.toml')
    if ($LASTEXITCODE -ne 0) { throw 'La compilation a echoue.' }
    & $cargoExecutable test --release --locked --offline --manifest-path (Join-Path $projectPath 'Cargo.toml')
    if ($LASTEXITCODE -ne 0) { throw 'Un test a echoue.' }
    Copy-Item -LiteralPath (Join-Path $env:CARGO_TARGET_DIR 'release\vcs-rust.exe') -Destination (Join-Path $projectPath 'VCS-Rust.exe') -Force
} else {
    & cargo build --release --locked --manifest-path (Join-Path $projectPath 'Cargo.toml')
    if ($LASTEXITCODE -ne 0) { throw 'La compilation a echoue.' }
    Copy-Item -LiteralPath (Join-Path $projectPath 'target\release\vcs-rust.exe') -Destination (Join-Path $projectPath 'VCS-Rust.exe') -Force
}
Write-Host 'VCS-Rust.exe est pret.'
