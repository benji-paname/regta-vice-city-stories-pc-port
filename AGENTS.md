# VCS-RS — Agent Instructions

## Objectif du projet

VCS-RS est une réimplémentation / portage natif PC de **Grand Theft Auto: Vice City Stories** en **Rust**.

Il s'agit d'un projet déjà existant et fonctionnel. **NE JAMAIS RECOMMENCER DE ZÉRO.**

Avant toute modification :
1. Inspecter la structure du dépôt et lire la documentation existante (`PLAN.md`, `PROGRESSION.md`, `VEHICULES.md`, `VALIDATION.md`).
2. Lire le code Rust existant (`src/*.rs`).
3. Identifier les systèmes déjà en place, les systèmes inachevés, les TODOs et les erreurs.
4. Compiler / tester le projet de manière appropriée.
5. Préserver rigoureusement les fonctionnalités déjà acquises.

## Versions de référence

Référence principale comportement/données :
- GTA Vice City Stories — PSP

Référence secondaire :
- GTA Vice City Stories — PS2 (ISO PAL présente localement sur `F:`)

L'utilisateur possède des copies légitimes des deux versions.

## Droits d'auteur et assets

- Ne JAMAIS inclure ni redistribuer d'assets protégés par le droit d'auteur Rockstar dans le dépôt.
- Les données et assets du jeu doivent être chargés dynamiquement depuis les fichiers de jeu légalement possédés / extraits par l'utilisateur (lecteur d'ISO natif déjà implémenté).
- Garder le code du moteur / de réimplémentation strictement séparé des données du jeu original.

## Structure du projet

- Code Rust principal : `outputs/VCS-Rust/` (ou `vcs-rs/`)
  - `src/` : code source Rust (moteur, rendu logiciel/accéléré, collisions, joueur Vic, caméra, iso reader, etc.)
  - `data/` : métadonnées et catalogues extraits (sans assets sous copyright)
  - `Cargo.toml`, `Cargo.lock`
- Travaux préparatoires, outils de reverse-engineering et probes Python : `work/`
- ISO originale : lue directement sur `F:` sans altération ni déplacement.

## Principes de développement et préférences utilisateur

- **Langue** : Communiquer et formuler les rapports et propositions en **français**.
- **Autonomie et validation** : Benjamin souhaite avancer étape par étape avec le minimum d'actions manuelles. **Ne pas lui demander d'essais manuels pour l'instant.** Effectuer toutes les validations et tests en arrière-plan (captures d'écrans de contrôle, tests unitaires, vérifications natives).
- **Interface** : Ne JAMAIS ouvrir automatiquement l'explorateur de fichiers Windows.
- **Continuité** : Préserver l'architecture déjà mise en place. Éviter toute réécriture massive sans justification technique impérative.
- **Rigueur** : Ne pas inventer de structures ou de formats de données lorsqu'ils peuvent être vérifiés dans l'ISO ou via les scripts de reverse-engineering sous `work/`. Documenter clairement les hypothèses techniques.
- **Incrémentalité** : Modifier uniquement les fichiers nécessaires, compiler, tester et vérifier l'absence de régression.

## État actuel et priorité

- **Version 0.3 terminée** : Carte extérieure continue (Beach, Mainland, Mall) texturée, collisions statiques, modèle original de Vic avec états repos/marche/course et caméra 3e personne avec détection d'obstacles.
- **En cours — Jalon 2 (Premier véhicule original)** :
  - 111 modèles répertoriés. Premier véhicule cible : **Banshee** (modèle 175, dictionnaire 171) + roues communes (modèle 323 dans `GAME.DTZ`).
  - Géométrie des 13 pièces principales (1582 triangles) et matrices décodées dans `work/vehicle-resource-probe/`.
  - Prochaine étape : intégration du lecteur Rust pour charger la carrosserie et les roues, résolution des textures, rendu et capture de validation, puis physique/conduite minimale et entrée/sortie du joueur.
