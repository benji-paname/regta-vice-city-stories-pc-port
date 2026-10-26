# Publish ReGTA VCS — GitHub Pages

The repository name is **regta-vice-city-stories-pc-port** under account **benji-paname**.
The showcase website is static and lives entirely in `docs/`; it requires no build pipeline or external generator.

## Setup and URLs

1. **Repository URL**: `https://github.com/benji-paname/regta-vice-city-stories-pc-port`
2. **Git Remote**: `git remote set-url origin https://github.com/benji-paname/regta-vice-city-stories-pc-port.git`
3. **GitHub Pages Deployment**:
   - Go to repository **Settings** &rarr; **Pages**
   - Source: **Deploy from a branch**
   - Branch: **`main`**
   - Folder: **`/docs`**
   - Click **Save**
4. **Live Website URL**: **`https://benji-paname.github.io/regta-vice-city-stories-pc-port/`**

On that Pages URL, all repository and documentation links automatically resolve to Benjamin's official repository.

## Safety and Clean-Room Policy

Only native Rust source code, vector assets, and markdown documentation are committed to Git.
Proprietary ISOs, game executables, raw textures, and extracted sound banks remain strictly ignored in `.gitignore`.
