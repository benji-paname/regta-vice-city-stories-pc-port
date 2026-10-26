# Build and local validation — ReGTA VCS

## Standard Windows build

Requirements: Windows x64 and a Rust toolchain with a matching Windows linker (MSVC or GNU).
From the repository root, run in PowerShell:

```powershell
.\Compiler.ps1 -Test
```

The script builds release mode with `-D warnings`, runs all 171 unit tests, and delivers the executable to `VCS-Rust.exe` at the repository root only upon complete success.

To launch the executable with an original game ISO:

```powershell
.\VCS-Rust.exe --iso "C:\Path\To\Grand Theft Auto - Vice City Stories.iso"
```

## Portable or offline builds

For portable or offline environments (such as cached toolchains):

```powershell
.\Compiler.ps1 -Test -Offline
```

The compiler uses Cargo dependency cache and builds cleanly without external network calls.

## Tests without game assets

All 171 unit tests generate synthetic mathematical fixtures in memory. They validate binary decoders, quaternion arithmetic, Pacejka curves, projectile physics, and weather logic in under one second without requiring the game ISO.
