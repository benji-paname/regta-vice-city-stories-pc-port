# Changelog

All notable changes to the **ReGTA VCS — PC Recomp** project are documented here.
The project is in active alpha development; version numbers reflect milestone achievements rather than final gameplay parity.

## [0.3.0-alpha] — 2026-10-26

### Added

- **3D Dynamic Weather & Tropical Storms (`src/weather.rs`, `src/gpu.rs`)**:
  - Implemented 6 dynamic climate states: `ExtraSunny`, `Sunny`, `Cloudy`, `Rainy`, `Thunderstorm`, and `SunsetSynthwave`.
  - 3D rain streaks (`RainStreak3D`) rendered directly in the OpenGL viewport with camera-relative velocity and ground splash rings.
  - Storm lightning simulation with ambient flash illumination and exponential fog modulation.
  - Real-time procedural audio thunder rumbles (`synthesize_thunder` in `src/audio.rs`) triggered dynamically with lightning flashes.
  - Interactive weather cycle hotkey (<kbd>K</kbd>) to instantly preview all climate profiles in real time.

- **Vehicle Dynamics, Transmission & Tire Friction (`src/driving.rs`, `src/transmission.rs`, `src/tires.rs`)**:
  - `CTransmissionDynamics`: 5-speed automatic transmission with realistic gear ratios, engine RPM curves, torque delivery, and automatic downshift guards during manual acceleration.
  - `CTireFrictionModel`: Pacejka Magic Formula implementing longitudinal and lateral grip coefficients, slip angles, and skidmark generation on heavy braking.
  - 4-wheel independent corner raycasts on the Banshee model, modeling suspension travel, mass transfer (pitch and roll), and high-speed stability.

- **Heavy Weapons, Ballistics & Pyrotechnics (`src/weapons.rs`, `src/projectiles.rs`, `src/crosshair.rs`)**:
  - `CProjectileRocket`: Propelled RPG rocket kinematics with volumetric particle smoke trail and explosive blast radius.
  - `CFireFlamethrower`: Forward thermal fire jet with cone collision detection, point-burn verification, and dynamic particle expansion.
  - `CSniperScopeView`: Authentic mil-dot sniper overlay with respiratory sway stabilization and variable zoom levels.
  - `CWeaponCrosshairHUD`: Dynamic spread crosshair reticles responsive to player velocity, weapon accuracy, and recoil.

- **Crowd Threat Assessment & Pedestrian Panic AI (`src/panic.rs`)**:
  - `CPedestrianPanicReaction`: Threat flee vectors calculated relative to gunshots, explosions, and vehicle collisions.
  - Crowd scatter dispersal angles and sprint navigation away from danger zones.

- **SCM Mission Scripting & Radio Tuning (`src/script.rs`, `src/audio.rs`)**:
  - SCM script engine executing the Fort Baxter prologue story mission.
  - 3D pulsating neon coronas marking mission start locations with real-time minimap blips.
  - Radio tuner (<kbd>B</kbd>) supporting all 8 Vice City radio stations: Wave 103, Flash FM, Emotion 98.3, V-Rock, Fever 105, Radio Espantoso, VCFL, and Fresh 105.

- **Brand Identity & Vector Logos (`docs/assets/`, `tools/research/generate_brand_logos.py`)**:
  - Designed authentic high-resolution vector logos using Pricedown Black (for `ReGTA`) and Rage Italic (for `Vice City Stories`), matching the official branding.
  - Generated stacked (`logo.svg`, 900x540) and horizontal (`logo-vcs.svg`, 1200x280) logo variants with transparent and black background editions.
  - Created high-resolution SVG favicon (`favicon.svg`) with Vice City magenta/cyan color palette.

- **Showcase Site & Verifiable Progress Dashboard (`docs/`)**:
  - Interactive multilingual showcase website (`docs/index.html`) in English, French, and Spanish with language persistence.
  - Squarified SVG progress treemap (`docs/assets/progress.svg`) tracking 690 ported routines across all 7 subsystems (93.6% code coverage).
  - Standalone progress dashboard (`docs/progress.html`) and verified metrics specification (`docs/STATUS.md`).

### Quality & Validation

- **171 passing unit tests** across all modules with 0 failures (`Compiler.ps1 -Test`).
- Strict zero-warning enforcement under `-D warnings` in Cargo build profile.
- 100% clean-room native code with zero copyrighted assets in Git repository.
