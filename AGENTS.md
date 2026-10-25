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
- GTA Vice City Stories — PS2 (ISO PAL; copie de travail sur `C:`)

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
- Copie de travail ISO : `..\..\work\Grand Theft Auto - Vice City Stories.iso` sur C:. L’ISO originale sur F: reste intacte.

## Principes de développement et préférences utilisateur

- **Langue** : Communiquer et formuler les rapports et propositions en **français**.
- **Autonomie et validation** : Benjamin souhaite avancer étape par étape avec le minimum d'actions manuelles. **Ne pas lui demander d'essais manuels pour l'instant.** Effectuer toutes les validations et tests en arrière-plan (captures d'écrans de contrôle, tests unitaires, vérifications natives).
- **Interface** : Ne JAMAIS ouvrir automatiquement l'explorateur de fichiers Windows.
- **Continuité** : Préserver l'architecture déjà mise en place. Éviter toute réécriture massive sans justification technique impérative.
- **Rigueur** : Ne pas inventer de structures ou de formats de données lorsqu'ils peuvent être vérifiés dans l'ISO ou via les scripts de reverse-engineering sous `work/`. Documenter clairement les hypothèses techniques.
- **Incrémentalité** : Modifier uniquement les fichiers nécessaires, compiler, tester et vérifier l'absence de régression.

## État actuel et priorité

- Le prototype natif comprend Beach + Mainland, Mall séparé, Vic animé, et la Banshee originale avec conduite, entrée/sortie et caméra suiveuse.
- La Banshee est visuellement validée par Benjamin. Ses détails de carrosserie, bande noire, feux arrière rouges et plaque sont préservés. L'exécutable de référence unique est `VCS-Rust.exe`.
- Le contrôleur de conduite reste arcade et approximatif. Priorité : étudier les données originales de handling de façon vérifiable, puis améliorer accélération, vitesse de pointe, braquage, glisse et roues sans casser le rendu.
- Le catalogue confirme notamment Banshee modèle 175 / TXD 171 / roues 323 / handling à `0x4c5700` dans l'image PS2 extraite. Première preuve de champ : `handling +0x68` est un angle de braquage en degrés (Banshee 34°), utilisé par le MIPS après conversion en radians et avec une entrée signée au carré. Le champ `+0x70`, l'échelle de temps et les autres paramètres restent à interpréter; ne pas appliquer des valeurs non vérifiées. Voir `HANDLING-RECHERCHE.md` et `work/handling-mips-dynamics-findings.txt`.
- Les notes de reprise pour modèles moins puissants sont dans `REPRISE-IA.md`.
- Le calcul du champ handling `+0x70` est maintenant localisé : coefficient appliqué à la projection de la vitesse `vehicle +0x140` sur l'axe avant, puis conversion en facteur de réduction du braquage `clamp(1 - résultat, 0, 1)` (avec un cas de demi-facteur). Valeurs observées 0,49 Banshee / 0,50 Cheetah / 0,45 Admiral / 0,48 Stinger. L'unité de vitesse PS2 n'est pas connue; ne pas appliquer au `Car.speed` Rust avant conversion. Voir la précision datée dans `HANDLING-RECHERCHE.md`.
- Décodage dynamique supplémentaire : `handling +0xBC` est confirmé comme masse (copié vers `vehicle +0xD0`, puis le helper `0x347F00` applique force/masse au vecteur vitesse `vehicle +0x140`). Le helper `0x348278` intègre la position avec vitesse × un scalaire global de temps, valeur/cadence encore inconnues. `handling +0xC0/2` (`vehicle +0xD4`) sert de diviseur dans le helper rotatif `0x348058`; inertie probable, ne pas la nommer définitivement. Ne pas appliquer le facteur handling `+0x70` directement à `Car.speed` avant conversion d'échelle. Détails dans `HANDLING-RECHERCHE.md` et la trace `work/handling-mips-dynamics-findings.txt`.
- Piste temps : intégration position prend la VA `0x004CD410`; son store provient de `input_struct+0x28` dans `0x3AD164`, via le chargeur `0x3AD0C0` et le chemin de lecture `0x3ADEF8`. Ne pas nommer ce champ `timeStep` en certitude sans identifier le fichier/schema ou obtenir sa valeur runtime. Suite dans `work/handling-mips-dynamics-findings.txt`.

- Précision du 12 octobre : la globale `0x004CD410`, utilisée pour multiplier la vitesse dans l'intégrateur position, reçoit le champ `input_struct+0x28` via le chargeur de configuration `0x3AD0C0`; le transfert inverse vers la structure est fait par `0x3ACB98`. Le rôle de paramètre configurable est prouvé, mais pas son nom, sa valeur ni son unité. Le fichier `CONFIGS/GTA_VC.DAT` est absent de l'ISO analysée. Ne pas le considérer comme un `delta time` connu ni calibrer le champ handling `+0x70` avec lui tant que le descripteur ou la valeur effective n'est pas retrouvé.

- Le champ handling `+0x60` apparaît dans une branche de préparation de force (`0x1A17A0..0x1A1BDC`), puis le vecteur est transmis au helper force/masse `0x347F00`. Son rôle exact n'est pas encore isolé; ne pas le nommer traction/adhérence ni le traduire en Rust. Voir `work/handling-mips-vehicle-update.txt` et la note datée dans `HANDLING-RECHERCHE.md`.

- Valeurs +0x60 vérifiées dans GAME.DTZ : Banshee/Cheetah/Stinger 0,00320; Admiral 0,00364 (champ multiplié par 2500 dans la routine). Conserver comme valeurs brutes; aucun nom/unité prouvé.

- Référence interversion : consulter `PSP-PORT-RECHERCHE.md` pour le projet PSP sur PC `vancevcs/VCSPC` et ses notes de reverse engineering. Ses adresses sont celles du jeu PSP US `ULUS10160`; ne pas les appliquer à l'ISO PS2 PAL. Les offsets relatifs/équations sont des pistes à recouper avec le MIPS PS2; le `TimeStep` PSP de caméra n'identifie pas le scalaire physique PS2.

- Pour la comparaison PSP/PS2, utiliser en priorité `PSP-PORT-RECHERCHE.md`: PSPRecomp offre le corpus AOT VCS (PSP US `ULUS-10160`) comme seconde représentation du code; repérer les mêmes équations côté SLES PS2 PAL, jamais reprendre les adresses PSP ou importer le code généré. VCSPC/PPSSPP sert de référence complémentaire pour outils et observations runtime.
- Résultat PSPRecomp du 12 octobre : les 234 unités AOT ont été inspectées; constantes float directes PS2 `2500.0`, `0.00320`, `0.00364` absentes, ce qui ne permet pas de conclure sur une équation équivalente. Les références de PC PSP vont jusqu'à `0x08CB2400`; ne pas borner la recherche à `0x08B70000` sous prétexte de la doc VCSPC. Le mapping d'une routine de conduite PSP reste à faire. Suivre `PSP-PORT-RECHERCHE.md`; aucune adresse PSP dans le code PS2, aucun code AOT tiers copié.
- `Compiler.ps1` met à jour l'unique exécutable de jeu `VCS-Rust.exe`; les tests ne tournent que si explicitement demandés avec `-Test`. Les exécutables divers dans `target/` sont des artefacts Cargo et ne sont pas des versions du jeu. Ne pas lancer de tests spontanément.
- Les roues ont déjà une animation de roulis et de braquage avant via `Vehicle::animated_triangles` dans `src/vehicle.rs`, alimentée par `Car::wheel_angle` et `Car::steering_angle`; vérifier cette chaîne avant de proposer une réimplémentation.
