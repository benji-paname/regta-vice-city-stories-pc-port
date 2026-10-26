# Brand Identity & Vector Assets — ReGTA VCS

This document describes the typography, color palette, and vector asset generation for the **ReGTA VCS — PC Recomp** project.

## Typography & Construction

- **Header Wordmark (`ReGTA`)**:
  - Font: **Pricedown Black** by Typodermic Fonts, with a reduced lowercase `e` (k = 0.78 scale factor) conforming to GTA title styling.
  - Geometry: Heavy 20px dark drop shadow (`#080808`), crisp 7px dark outline, and off-white (`#fffdf5`) vector fill.
- **Title Wordmark (`Vice City`)**:
  - Font: **Rage Italic**, the authentic typeface used by Rockstar Games for Grand Theft Auto: Vice City and Vice City Stories.
  - Geometry: 16px deep shadow, glowing 8px cyan keyline (`#00f0ff`), and vibrant neon magenta/pink (`#ff2a85`) fill.
- **Subtitle Banner (`STORIES`)**:
  - Font: **Pricedown** tracked caps in sunset gold (`#f2c35b`) with dark drop shadow and outline.
- **Tagline (`PC RECOMP`)**:
  - Crisp tracked sans-serif in off-white (`#fffdf5`).

## Color Palette

| Token | Hex Code | Purpose |
|---|---|---|
| `--bg` | `#0b0d11` | Primary background |
| `--panel` | `#14171d` | Card and table surface |
| `--pink` | `#ff2a85` | Vice City neon magenta / primary highlight |
| `--cyan` | `#00f0ff` | 1984 neon cyan / accent keylines |
| `--gold` | `#f2c35b` | Sunset gold / badge and button accents |
| `--green` | `#3fa34d` | Pass indicators and verified coverage |
| `--ink` | `#f3f1ea` | High-contrast body text |
| `--muted` | `#b1b4bb` | Secondary and description text |

## Vector Logo Files (`docs/assets/`)

- `logo.svg` / `logo-transparent.svg` (900x540): Canonical stacked wordmark for GitHub README and website hero banner.
- `logo-black.svg` (900x540): Solid black background edition for dark media and press previews.
- `logo-vcs.svg` / `logo-vcs-transparent.svg` (1200x280): Horizontal banner edition for navigation bars and wide displays.
- `logo-vcs-black.svg` (1200x280): Horizontal edition with solid black background.
- `favicon.svg` (64x64): Site favicon featuring the styled "R" and cyan neon underline.

All logos are regenerated deterministically via `python tools/research/generate_brand_logos.py`.
