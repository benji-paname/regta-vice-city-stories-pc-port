# ReGTA: Vice City Stories (VCS-Rust) — Portage PC Natif en Pur Rust

<p align="center">
  <img src="docs/assets/badge-overall.svg" alt="Progression Globale" />
  <img src="docs/assets/badge-routines.svg" alt="Routines Portées" />
  <img src="docs/assets/tests-badge.svg" alt="Tests Unitaires" />
  <a href="https://discord.com/users/601648068277960714"><img src="https://img.shields.io/badge/Discord-benjamin060936-7289da?logo=discord&logoColor=white" alt="Discord Benjamin" /></a>
</p>

Portage PC natif clean-room et open-source de **Grand Theft Auto: Vice City Stories** (PlayStation 2 PAL `SLES-54622` / ELF `SLES_546.22`) développé en **100% pur Rust** (Windows x64).

> **Auteur & Mainteneur** : Benjamin (« benji-paname »)  
> **Contact Discord officiel** : [benjamin060936 (601648068277960714)](https://discord.com/users/601648068277960714)  
> **Vitrine & Tableaux de bord interactifs** : Consultez le site web complet sous `docs/` (`docs/index.html` et `docs/progress.html`).

---

## 1. Principes Fondamentaux & Éthique Clean-Room

1. **100% Rust Natif — Zéro Émulation CPU au Runtime** :
   - Aucun émulateur (ni MIPS, ni PCSX2, ni PPSSPP). Le code du jeu est compilé directement en binaire x86_64 natif pour garantir des performances optimales (taux de rafraîchissement débloqué, fluidité 60+ FPS, support 4K/Ultrawide).
2. **Légalité & Zéro Contenu Sous Droits d'Auteur dans le Dépôt** :
   - Le dépôt Git ne contient AUCUN asset propriétaire (textures, sons, modèles 3D, musiques).
   - Tous les assets originaux sont lus, décodés et convertis à la volée depuis l'image disque légale de l'utilisateur (`Grand Theft Auto - Vice City Stories.iso`).
3. **Règle du Progrès Visible (Visual-First)** :
   - Toute nouvelle mécanique (handling, balistique, circulation, météo, foule) est immédiatement raccordée au moteur de rendu OpenGL (`src/gpu.rs`) pour être visible et jouable en direct dans l'exécutable unique `VCS-Rust.exe`.

---

## 2. Commandes & Raccourcis Clavier

| Touche | Action | Contexte |
| :--- | :--- | :--- |
| **ZQSD** / **WASD** | Se déplacer (ou braquer / accélérer en véhicule) | Pied / Conduite |
| **Flèches** | Pivoter la caméra et vue libre | Partout |
| **Maj Gauche** | Courir / Sprint (ou accélérer la nage) | À pied / Nage |
| **Espace** | Sauter / Franchir un muret (ou frein à main) | À pied / Conduite |
| **Tab** | **Changer d'arme** (parcourt les 9 armes de l'arsenal) | À pied |
| **Clic Gauche** / **F** | **Tirer** / Attaquer au corps à corps | À pied |
| **Clic Droit** / **V** | **Viser** (active le réticule ou la lunette sniper) | À pied |
| **H** | **Activer le gyrophare d'urgence** | Véhicule / Caméra |
| **E** | Monter / Sortir de la Banshee | Proche du véhicule |
| **G** | Basculer entre marche et caméra libre | À pied |
| **F1** | Afficher / Masquer l'aide à l'écran | En jeu |
| **F2** | Basculer entre 1ère et 3ème personne | À pied |
| **F3** | Menu d'options (mode Original PS2 vs Moderne) | En jeu |
| **1 / 2 / 3 / 4** | Téléportation rapide (Beach, Mainland, Mall, Base militaire) | En jeu |
| **T** | Activer / Désactiver les textures | En jeu |
| **R** | Réinitialiser la position et le monde | En jeu |
| **Échap** | Quitter | En jeu |

---

## 3. Dernières Avancées & Jalons Opérationnels

- **Jalon A — Physique & Suspensions 4 Roues Indépendantes** :
  - Suspensions indépendantes sur la Banshee : calcul de déflexion verticale par raycast corner, compression dynamique des ressorts.
  - Transfert de masse dynamique : tangage réaliste (pitch) lors des fortes accélérations et freinages, roulis (body roll) en virage selon l'accélération centrifuge.
  - Animation 3D temps réel des roues (braquage et course verticale des moyeux).
- **Jalon B — Arsenal 3D & Balistique Complète** :
  - 9 types d'armes portés : Poings, Pistolet 9mm, Micro-SMG, Fusil à pompe, Fusil d'assaut, Fusil de sniper, Lance-roquettes, Grenade, Lance-flammes.
  - Balistique 3D en temps réel : roquettes propulsées avec traînées de fumée particulaire, grenades paraboliques avec gravité ($g = 9.81\text{ m/s}^2$), traînées lumineuses de balles (hitscan tracers) et gerbes d'étincelles aux impacts.
  - HUD de visée dynamique : réticules 2D adaptés à l'arme, cercle de dispersion de chevrotine, et lunette sniper plein écran avec repères mil-dots.
- **Jalon C — Feux Tricolores & Circulation Urbaine** :
  - Modèle 3D de feux tricolores (poteau, boîte et optiques lumineuses Vert/Orange/Rouge) aux intersections.
  - Respect des feux de signalisation : les véhicules du réseau Leeds ralentissent et s'immobilisent aux feux rouges et oranges.
  - Réaction aux véhicules d'urgence : à l'activation du gyrophare (`H`), les conducteurs se déportent vers le bord du trottoir.
- **Jalon D — Foule & Réaction de Panique** :
  - Génération de piétons sur les graphes de trottoir avec évitement d'obstacles.
  - Détection des coups de feu et explosions déclenchant la panique immédiate et la fuite désordonnée des PNJ loin de la menace.
- **Infrastructure & Vitrine Interactive** :
  - Site vitrine rétro-synthwave 1984 sous `docs/` (`index.html` et `progress.html`).
  - Outil automatisé `tools/progress/progress.py` générant les métriques, la cartographie SVG Treemap et les badges.
  - Script racine `Compiler.ps1 -Test` intégrant la compilation Release sous `-D warnings`, l'exécution des tests (126 tests à 100% passants) et la mise à jour des statistiques.

---

## 4. Compilation & Vérification

Pour compiler et vérifier l'ensemble du projet sans dépendance externe :
```powershell
powershell -ExecutionPolicy Bypass -File .\Compiler.ps1 -Test
```
L'exécutable autonome généré est `VCS-Rust.exe` à la racine.
