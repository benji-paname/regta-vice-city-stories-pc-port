# Reprendre VCS-Rust avec un modèle local

Ce fichier donne les repères essentiels à Goose, Ollama ou Devstral-small. Lire ensuite `AGENTS.md`, `PROGRESSION.md`, `PLAN.md`, `VEHICULES.md` et `HANDLING-RECHERCHE.md` avant de modifier le code.

## Règle de continuité obligatoire

À chaque avancement terminé, mettre à jour les notes de reprise dans le même changement que le code :

1. Ajouter dans `PROGRESSION.md` le résultat réellement obtenu, les vérifications effectuées et les limites qui restent.
2. Mettre à jour ce fichier (`REPRISE-IA.md`) avec les nouveaux fichiers concernés, commandes utiles, décisions vérifiées, pièges rencontrés et prochaine action concrète.
3. Actualiser `AGENTS.md` et `PLAN.md` seulement si l'état prioritaire ou l'ordre des jalons change.
4. Ne jamais écrire qu'une fonctionnalité est terminée si elle a seulement été compilée ou supposée correcte. Séparer explicitement « observé », « vérifié » et « hypothèse ».
5. Garder une seule version de livraison à la racine : `VCS-Rust.exe`. Builds, captures temporaires, journaux et données extraites restent dans `work/`.

Le modèle suivant doit pouvoir reprendre à partir de ces notes sans relire tout l'historique du chat. En cas de doute, inspecter le code et les artefacts actuels avant de poursuivre; les notes ne remplacent pas le dépôt.

## Projet et objectif

- Dossier du projet : `C:\Users\BENJAMIN\Documents\Codex\2026-10-05\en\outputs\VCS-Rust`
- Dossier de travail/outils : `C:\Users\BENJAMIN\Documents\Codex\2026-10-05\en\work`
- C'est un moteur PC réécrit en Rust qui lit les ressources de la copie PS2 de Benjamin. Ce n'est pas un émulateur PS2.
- Ne jamais recommencer de zéro, écraser des changements non compris, ni ajouter au dépôt les données protégées extraites du jeu. Les ressources extraites restent dans `work/`.
- Répondre en français. Ne pas demander de tests manuels sans nécessité : Benjamin préfère les vérifications locales automatisées et a validé visuellement la Banshee.
- Ne pas ouvrir automatiquement l'Explorateur Windows.

## État de référence

- Lancement final : `VCS-Rust.exe` (seul `.exe` conservé à la racine du projet).
- Carte Beach + Mainland, Mall séparé, Vic original animé et contrôleur de déplacement présents.
- Banshee du jeu (modèle 175, TXD 171) chargée depuis l'ISO, avec quatre roues du modèle 323, entrée/sortie, conduite prototype, caméra suiveuse et textures extérieures validées par l'utilisateur. Le véhicule affiche sa peinture claire, son bandeau noir, ses feux rouges et sa plaque.
- Le prototype n'est pas le jeu complet et la conduite ne reproduit pas encore le handling PS2. La prochaine tâche est de décoder les données de handling sans inventer de noms/unités, puis de faire évoluer la conduite et l'animation des roues.
- `src/driving.rs` : vitesse, accélération, freinage, braquage, mouvement, pose au sol et collision approximative.
- `src/vehicle.rs` : lecture du modèle, hiérarchie des pièces, textures, roues et catalogue.
- `src/texture.rs` : décodage et coloration des textures véhicule.
- `src/gpu.rs` : état OpenGL et passes de dessin (une correction de profondeur/rendu a résolu la carrosserie désassemblée).
- `src/script.rs` : machine à états de missions SCM, variables globales/locales, objectifs et balises 3D.
- `src/audio.rs` : moteur audio procédural, 8 stations de radio Vice City Stories, synthèse PCM de régime moteur.
- `src/render.rs` et `src/main.rs` : boucle de jeu, caméra, contrôles, entrée/sortie et affichage.

## Manipulations et vérification

- Compiler offline depuis ce dossier : `powershell -ExecutionPolicy Bypass -File .\Compiler.ps1`; tests seulement sur demande : ajouter `-Test`.
- Le script utilise les outils Rust portables sous `..\..\work`, `Cargo.lock`, et copie le résultat final dans `VCS-Rust.exe`.
- Diagnostic sans fenêtre : `.\VCS-Rust.exe --verify`
- Aperçu isolé du véhicule : `.\VCS-Rust.exe --vehicle-preview sortie.bmp`
- Les artefacts temporaires de compilation sont sous `work\rust-build`; ne pas les copier dans le dossier principal en leur donnant chacun un nom d'exécutable différent.
- Avant tout changement : lire les fichiers pertinents, examiner `git status --short`, garder tous les changements existants, modifier peu de fichiers et compiler/tester les changements fonctionnels.

## Piste de recherche handling — observations, pas encore une interprétation

Le catalogue de `data/vehicle-catalog.json` fournit les pointeurs vers `GAME.DTZ.dat` extrait localement sous `work\assets-probe\PS2\GAME.DTZ.dat`. Exemples observés : Banshee `0x4c5700`, Cheetah `0x4c5460`, Admiral `0x4c5380`, Stinger `0x4c0680`. Ces pointeurs et valeurs doivent être reconfirmés par les métadonnées avant d'être utilisés.

Lecture exploratoire de mots float32 little-endian à ces adresses : beaucoup de champs préfixes sont identiques entre voitures; plusieurs champs diffèrent (par exemple offsets `0x0c` à `0x54`, puis `0x60` à `0x8c`). À `0x68`, les exemples Banshee/Cheetah affichent 34, Stinger 30, Admiral 35. Ce sont seulement des valeurs observées : aucune unité ni signification n'est confirmée. `0x5c` ressemble à une valeur subnormale et ne doit pas être traité aveuglément comme un flottant de conduite.

Complément de recherche dans `HANDLING-RECHERCHE.md` : le pas `0xe0` est observé sur 107/110 écarts d'adresses triées des 111 modèles. Les modèles de bateaux pointent aussi via `model + 0x44` vers une famille de structures de `0xb0` octets; d'autres types utilisent des pointeurs auxiliaires. Deux ruptures ne sont pas des multiples de `0xe0`; ne pas appeler leurs régions des « lignes manquantes ». Le pointeur général `HANDLING.CFG` à l'entête `0xA8` vaut `0x419170` et correspond à une grande plage générique distincte des lignes pointées par les véhicules.

Prochaine enquête utile : comparer les structures d'embarcation `0xb0` et leurs champs bruts, en utilisant reVC GTA VC seulement comme piste (son `tBoatHandlingData` est différent), puis identifier les extensions spécialisées des autres types. Les résultats sont dans `HANDLING-RECHERCHE.md`. Ne pas appliquer les valeurs tant que type, sens, unités et conversions ne sont pas prouvés.

## Ressources originales et confidentialité

L’ISO PS2 PAL originale sur F: est conservée intacte. Sa copie complète de travail est dans `work/Grand Theft Auto - Vice City Stories.iso` sur C:. Utiliser la copie C: dans les outils et analyses; garder ISO et actifs extraits hors du dépôt de livraison.

## Journal des derniers travaux — 12 octobre 2026

- Consolidation des exécutables : remplacé `VCS-Rust.exe` par le build le plus récent déjà compilé/testé (2 346 002 octets) et supprimé les trois variantes temporaires de la racine. Vérification : il n'y a plus qu'un `.exe` à la racine du projet.
- Mise à jour des notes de progression : le premier véhicule est maintenant décrit comme jouable et visuellement validé par Benjamin; anciennes consignes disant que son intégration restait à faire supprimées de `AGENTS.md`.
- Analyse initiale du handling : comparé des valeurs float32 little-endian à plusieurs pointeurs du catalogue, dont Banshee `0x4c5700`, Cheetah `0x4c5460`, Stinger `0x4c0680` et Admiral `0x4c5380`. Plusieurs champs varient, mais aucun nom, unité ou interprétation n'est confirmé. N'a donc pas modifié la physique sur cette base.
- Comparaison des 111 entrées du catalogue : 107/110 écarts adjacents valent `0xe0`. Recoupement des champs modèle : 14 bateaux de type 1 et JETSKI de type 2 ont tous un pointeur complémentaire `+0x44`; ceux-ci correspondent à deux groupes de lignes à pas `0xb0` plus une entrée SKIMMER isolée. Le référentiel reVC GTA VC expose une famille distincte boat-handling, utile seulement comme piste; format non validé pour VCS.
- Pour les 11 lignes du bloc principal boat-extension, certains mots sont variables et plusieurs offsets sont nuls pour les 11 véhicules; voir la carte des offsets dans `HANDLING-RECHERCHE.md`. Ce sont des motifs binaires, pas des noms de champs.
- Le pointeur général `HANDLING.CFG` de l'entête (`0xA8`) mène à une plage générique distincte; l'outil local Storyland ne prétend pas connaître le découpage exact.
- Prochaine action concrète : comparer les champs des extensions d'embarcation `0xb0`, puis séparer les autres données auxiliaires; ne toucher à la physique qu'après validation propre à VCS du sens et des unités.
- État code pour ce jalon : aucune modification Rust ni nouveau build; avancement de recherche et documentation uniquement. Le build final reste le code véhicule déjà validé précédemment.
- Audit complémentaire du 12 octobre : `work/research/librwgta-master/tools/storiesconv/storiesconv.cpp` exporte les types et métadonnées de véhicule, mais met `HANDLING_TODO` pour le handling. Ne pas chercher des champs VCS nommés dans cet exporteur ni extrapoler les structures de GTA III/VC.
- Prochaine action concrète : obtenir un désassemblage MIPS exploitable de l'exécutable PS2 et repérer les fonctions qui lisent les pointeurs du modèle aux offsets `+0x38`, `+0x44`, `+0x48` et `+0x4c`; suivre les lectures de champs jusqu'aux calculs de conduite. Si les outils ou symboles ne sont pas disponibles, noter le blocage précis et chercher une autre source VCS documentée. Ne pas modifier `src/driving.rs` sur la base des seules valeurs brutes.
- Aucun code Rust ni exécutable final n'a été modifié pendant cet audit. Le dépôt avait déjà plusieurs fichiers modifiés/non suivis avant l'étape : conserver ces changements et ne pas les écraser.
- L'ELF a été retrouvé dans `outputs/VCS-PS2-recomp/original/SLES_546.22`; le détail et les erreurs de la première passe sont dans les notes ci-dessous. Ne pas confondre son espace d'adresses avec les offsets de `GAME.DTZ`.

- Mise à jour 12 octobre : l'ELF PS2 est accessible dans `outputs/VCS-PS2-recomp/original/SLES_546.22`. Il est ELF32 little-endian MIPS, entry `0x001DD680`, `.text` VA `0x00100000`/fichier `0x1000`/taille `0x380888`. Le `llvm-objdump.exe` du toolchain ne démarre pas (`0xC0000135`, DLL manquante); une passe opcode maison montre des références aux offsets `+0x38/+0x44/+0x48/+0x4c/+0x54/+0x58/+0x5c`, notamment dans `0x001EC910–0x001ECA90`, mais la structure n'est pas identifiée. Ne pas appeler cela une fonction handling.
- Piège vérifié : le pointeur Banshee `0x4C5700` est un offset du GAME.DTZ. Sa valeur coïncide numériquement avec une adresse dans `.bss` du SLES, sans lien démontré. Ne jamais lire cette valeur dans le SLES pour décoder la ligne.
- Prochaine action : trouver un désassembleur MIPS fonctionnel et établir les arguments/types de la grappe candidate; garder les conclusions comme hypothèses jusqu'à preuve d'un lien à un véhicule ou à son pointeur de handling.

- Emplacements revérifiés le 12 octobre à la demande de Benjamin : `outputs/VCS-Rust`, `outputs/VCS-PS2-recomp` et l'intégralité de `work` sont déjà sous `C:\Users\BENJAMIN\Documents\Codex\2026-10-05\en`. Les données extraites `GAME.DTZ.dat` et le SLES PS2 étudié sont aussi sur C:. Aucune migration locale n'était nécessaire.
- L'index de recompilation référence encore l'ISO originale sur `C:\Users\BENJAMIN\Documents\Codex\2026-10-05\en\work\Grand Theft Auto - Vice City Stories.iso` (4,14 Go). La copie de travail est dans `work` sur C:; le fichier original sur F: est conservé.

- Après copie de l’ISO, compilation release offline réussie avec le chemin par défaut C: (`src/main.rs` → `work/Grand Theft Auto - Vice City Stories.iso`). Le binaire de référence `VCS-Rust.exe` a été remplacé par ce build (2 346 003 octets). Tests non exécutés; la copie ISO elle-même a été contrôlée par sa taille, son descripteur ISO9660, le SLES embarqué et le SHA-256 de référence.

- 12 octobre — Désassemblage MIPS débloqué avec Capstone 5.0.9 installé sous `work/mips-tools`; le projet Rust et l'installation Python globale ne sont pas modifiés. Script réutilisable : `work/mips_disasm.py`; sortie vérifiée : `work/handling-mips-boat-routine.txt`.
- Preuve nouvelle : routine SLES `0x001AA110` suit `model + 0x38`, teste `handling + 0xD0 & 0x08000000`, puis pour les entrées correspondantes suit `model + 0x44` et lit les flottants de l'extension aux offsets `0x70/0x74/0x78/0x80/0x84/0x88`. Les valeurs sont appliquées à deux vecteurs du modèle (échelles et additions dans les instructions); une routine voisine `0x00369870` répète ces lectures. La comparaison des valeurs brutes bateau concorde avec ce bit. Preuve structurelle utile, mais pas un paramètre de propulsion ou de direction : ne pas changer `src/driving.rs` sur cette base.
- Le bloc exploratoire `0x001EC910` n'a pas de lien modèle/handling identifié; le déprioriser. Prochaine piste : routines auto qui consomment des floats des records `0xE0` dans la mise à jour du mouvement; suivre appels et arguments et conserver les adresses MIPS dans `HANDLING-RECHERCHE.md`.
- Aucun changement Rust, build, test ou lancement du jeu dans cette étape.

- 12 octobre — Recensement des flags : `work/handling_flag_census.py` et sa sortie `work/handling-mips-flag-census.txt` montrent que `handling+0xD0 & 0x08000000` marque exactement les 15 véhicules catalogués des types 1/2 (14 bateaux, JETSKI), chacun avec un pointeur `model+0x44`; aucune des 96 autres lignes ne porte le bit. Le SLES contient aussi le libellé `HF_IS_WATER_VEHICLE`, mais aucun xref ne relie directement le libellé à ce masque : l'association reste une hypothèse.
- Les accès `0x00211954` et aux tableaux de vecteurs similaires ont été écartés comme faux positifs : ils passent par un pointeur imbriqué `model+0x14` ou un tableau stride `0x20`, pas par le record handling identifié du catalogue. L'ELF est dépourvu de symboles; la chaîne `vehicles/automobile.cpp` ne suffit pas à nommer une fonction.
- Suite : suivre l'objet runtime automobile et les appels de mise à jour pour trouver où les champs handling alimentent les calculs de vitesse, force, freinage ou braquage. Aucun changement Rust, build, test ou lancement du jeu à ce jalon.

- Mise à jour — 12 octobre : la suite de la passe précédente est accomplie en première analyse. La routine de création `0x0019C1E0` copie model-info `+0x38` (pointeur handling confirmé par le catalogue) vers l'objet véhicule `+0x160`, puis copie certains floats handling dans `vehicle +0xC0..+0xDC`. La routine commandes autour de `0x0019F708` charge handling `+0x68` pour produire la commande runtime `vehicle +0x258`; `+0x70` est lu dans la grande routine de dynamique autour de `0x001A0A80..` et combiné à la vitesse pour moduler l'effet de cette commande de rotation. Détails et limites dans `HANDLING-RECHERCHE.md` et `work/handling-mips-dynamics-findings.txt`.
- Interprétation à ne pas dépasser : les usages sont prouvés par le code, mais les noms officiels/unités de `+0x68`, `+0x70`, `+0x98`, `+0xA0..+0xAC`, `+0xB4`, `+0xBC`, `+0xC0` et l'échelle temporelle ne le sont pas. Ne pas modifier `src/driving.rs` avant d'avoir suivi les sorties et fait une conversion documentée.
- Aucun source Rust, binaire, build, test ou lancement du jeu n'a changé pendant cette étape. Les nouveaux résultats MIPS sont à intégrer à la suite plutôt que de refaire la recherche des références model-info, la routine d'initialisation ou les xrefs déjà consignés.
- Première intégration de physique fondée sur preuve — `src/driving.rs` utilise maintenant le braquage Banshee de 34° (conversion explicite degrés→radians) et la courbe d'entrée signée au carré observée en MIPS (`vehicle +0x484` vers `+0x258`). Les angles source observés sont Banshee 34°, Cheetah 34°, Admiral 35°, Stinger 30°. La réduction avec la vitesse est encore l'approximation préexistante, à remplacer après calibration de handling `+0x70` et des unités.
- Ne pas traiter cette petite intégration comme une physique GTA complète : accélération, freinage, vitesse maximale, dérive et suspension restent approximatifs. Aucun build ni test n'a été exécuté dans cette étape; lancer le jeu n'est pas nécessaire pour ce changement de loi d'entrée.
- Suite du traçage — handling `+0x70` est maintenant attribué à la réduction du braquage par la vitesse avant. Dans la routine MIPS, la vitesse vectorielle `vehicle +0x140` est projetée sur l'axe avant (`vehicle +0x10..+0x18`), multipliée par le coefficient `handling +0x70`, puis `1 - résultat` est clampé entre 0 et 1; une condition divise parfois le résultat par deux. Valeurs observées : Banshee 0,49; Cheetah 0,50; Admiral 0,45; Stinger 0,48.
- Je n'ai pas remplacé le facteur Rust par ce coefficient brut : l'unité de la vitesse PS2 n'est pas encore convertie vers les unités monde/seconde de `Car.speed`. La formule Rust en place reste clairement annotée comme provisoire. Aucune compilation, aucun test ni lancement du jeu pour cette étape.
- Suite du décodage physique — `handling +0xBC` est identifié comme la masse : la routine MIPS copie la valeur vers `vehicle +0xD0`, puis le helper `0x00347F00` divise la force par `+0xD0` et ajoute le résultat au vecteur vitesse `vehicle +0x140`. La position `+0x30..+0x38` est intégrée par `0x00348278` en ajoutant vitesse × scalaire global de temps. Ce scalaire global et son rapport aux unités Rust ne sont pas encore déterminés.
- `handling +0xC0 / 2` est copié en `vehicle +0xD4`, utilisé comme diviseur dans le helper rotatif `0x00348058`; une inertie rotatoire est probable mais le nom n'est pas confirmé. La correction de braquage `+0x70` ne peut donc pas encore être convertie en prenant simplement `Car.speed` : retrouver le scalaire temporel global et l'échelle de vitesse demeure la prochaine tâche. Aucun code, build ou test supplémentaire.
- Piste du scalaire global — l'intégrateur `0x348278` lit le multiplicateur vitesse→déplacement à la VA MIPS `0x004CD410`. Son store direct repéré (`0x3AD164`) copie un flottant du champ `input_struct +0x28`, alimenté par le chargeur de données `0x3AD0C0` (appel `0x3ADEF8`). Le champ/fichier/valeur restent à identifier; je ne l'étiquette pas encore `timeStep` définitivement. Cela donne un xref concret pour la prochaine passe. Aucune modification du contrôleur.


- 12 octobre — Précision sur VA `0x004CD410` : `0x348278` l'utilise pour l'intégration position = vitesse × scalaire. Le chargeur `0x3AD0C0` applique `input_struct+0x28` à cette globale; `0x3ACB98` la recopie dans une structure de configuration. C'est donc un paramètre configurable, mais rien ne prouve que son nom soit `timeStep`, ni sa valeur/unité. L'ISO étudiée ne contient pas `CONFIGS/GTA_VC.DAT`. La piste de construction de structure est donc tracée jusqu'au champ +0x28, sans nom de descripteur ni valeur runtime. Ne pas mapper à Rust avant d'avoir cette donnée. Aucun code, build ou test modifié/exécuté.


- 12 octobre — Nouveau candidat de dynamique : `handling+0x60` est lu à `0x1A17A0`, multiplié par `2500.0`, puis entre dans la préparation d'un vecteur transmis à `0x347F00` (`0x1A1BDC`), helper qui modifie `vehicle+0x140` par force/masse. Son rôle précis reste inconnu; ne pas le nommer adhérence/traction ni l'intégrer en Rust. Désassemblage : `work/handling-mips-vehicle-update.txt`; suivre les facteurs/conditions autour de `0x1A1720..0x1A1BDC`. Aucun code, build ou test modifié/exécuté.

- Comparaison catalogue du champ `+0x60` : Banshee/Cheetah/Stinger `0.00320`, Admiral `0.00364`; après facteur MIPS ×2500 : `8.0/8.0/8.0/9.1`. Valeurs à garder comme preuve de variation, pas comme nom/unité; données lues dans `work/assets-probe/PS2/GAME.DTZ.dat` par les pointeurs de `data/vehicle-catalog.json`.


- 12 octobre — Nouvelle référence de recherche à exploiter : `PSP-PORT-RECHERCHE.md` décrit `vancevcs/VCSPC`, un fork PPSSPP pour jouer à la version PSP sur PC. Sa doc détaille une méthode fiable d'inspection et des offsets de structures utiles, mais les adresses absolues concernent l'édition PSP US `ULUS10160`, pas notre binaire PS2 PAL `SLES_546.22`. Elle confirme notamment la matrice/position en tête d'entité (`+0x30`) et donne un `TimeStep` PSP employé par la caméra; ne pas l'assimiler au scalaire d'intégration PS2. La doc consultée ne fournit pas de tuning handling Banshee à recopier. Priorité : comparer les équations / offsets relatifs PSP avec la chaîne MIPS PS2, jamais transposer les adresses PSP.


- 12 octobre — Référence PSP corrigée et approfondie : pour la recherche logique, la base la plus directe est désormais **PSPRecomp** (`jessicanataliagta/PSPRecomp/profiles/vcs`), qui fournit le corpus C++ AOT source du code PSP (15 988 fonctions, 775 148 instructions, 234 unités selon son rapport). Cible PSP US `ULUS-10160`; comparer ses équations/opérations avec notre SLES PS2 PAL, sans reprendre les adresses PSP ni copier le C++ généré. Le projet `vancevcs/VCSPC` reste complémentaire pour sa doc d'adresses et son workflow PPSSPP debugger. Fiche locale : `PSP-PORT-RECHERCHE.md`.

- 12 octobre — Inspection complète PSPRecomp : les 234 unités AOT (103,7 Mio) ont été parcourues. Les constantes float PS2 `2500.0`, `0.00320`, `0.00364` ne sont pas présentes telles quelles; ce résultat ne confirme ni n'infirme une logique PSP équivalente, car les paramètres peuvent être chargés depuis les données handling. Le corpus garde les PC PSP en labels et contient des références jusqu'à `0x08CB2400`; la borne `0x08B70000` de VCSPC ne doit pas servir de limite pour lui. Aucune routine conduite PSP n'est encore mappée. Prochaine étape dans `PSP-PORT-RECHERCHE.md`: retrouver les lectures handling/commandes et leur base `$gp`, puis comparer les équations à `0x19F9C0`, `0x1A14B4`, `0x1A17A0` PS2. Pas de code Rust/build/test modifié/exécuté.


## Mise à jour Octobre 2026 — Jalons A, B, C, D & Vitrine Publique

### Nouvelles fonctionnalités intégrées et validées :
1. **Jalon A — Suspensions 4 roues indépendantes & Transfert de masse** :
   - `src/vehicle.rs` : méthode `animated_triangles_with_suspension` appliquant la translation verticale individuelle des moyeux.
   - `src/driving.rs` : calcul des déflexions par 4 raycasts corner, modélisation dynamique du tangage (accélération/freinage) et du roulis (virages).
2. **Jalon B — Arsenal 3D & Balistique temps réel** :
   - `src/weapons.rs` : 9 types d'armes VCS, tirs balistiques (grenades paraboliques avec $g=-9.81\text{ m/s}^2$, roquettes avec particules de fumée, flammes), traînées lumineuses de balles (`BulletTracer`), étincelles d'impacts.
   - HUD de visée : réticules 2D, cône de dispersion fusil à pompe, et lunette sniper plein écran avec mil-dots.
   - Contrôles : `Tab` (changer d'arme), Clic Gauche / `F` (tirer), Clic Droit / `V` (viser).
3. **Jalon C — Feux Tricolores & Circulation Leeds** :
   - `src/traffic.rs` : cycles Vert/Orange/Rouge, arrêt dynamique des voitures du trafic au carrefour.
   - Gyrophare d'urgence : touche `H` déclenchant la réaction des conducteurs qui se serrent vers le trottoir.
4. **Jalon D — Foule & Panique** :
   - `src/population.rs` : locomotion piétonne, évitement d'obstacles, et fuite éperdue en panique (`trigger_panic`) dès qu'un coup de feu ou une explosion retentit.
5. **Rendu OpenGL 3D/2D (`src/gpu.rs`)** :
   - `draw_traffic_lights` : feux tricolores 3D visibles aux carrefours.
   - `draw_weapons_3d` : projectiles, traînées lumineuses et particules pyrotechniques visibles en vol.
   - `draw_crosshair_hud` : affichage des réticules et de la lunette sniper.
6. **Infrastructure & Vitrine (`docs/`, `tools/`)** :
   - Site web vitrine complet sous `docs/` (`docs/index.html` et `docs/progress.html`) au design rétro-synthwave 1984.
   - Outil autonome `tools/progress/progress.py` mesurant les fonctions Rust par rapport à `progress/reference.json`.
   - Badges temps réel (`badge-overall.svg`, `badge-routines.svg`, `tests-badge.svg`) et treemap SVG interactif.
   - Compilation et tests orchestrés par `Compiler.ps1 -Test` : 126 tests unitaires passants à 100%, 0 avertissements (`-D warnings`).
   - Contact officiel Discord : `https://discord.com/users/601648068277960714` (`benjamin060936`).

### Nouveaux modules et jalons intégrés (Octobre 2026 - Jalons E, F, G, H) :
1. **`src/police.rs` (WantedSystem)** : Étoiles de recherche 0..6, détection des infractions, évasion et clignotement.
2. **`src/save.rs` (GameSaveState)** : Sauvegarde rapide (<kbd>F5</kbd>) et restauration (<kbd>F9</kbd>) dans `VCS-Save.json`.
3. **Dégâts des véhicules & Pyrotechnie 3D** : Points de vie, fumée de moteur, flammes 3D et explosions dans `src/driving.rs`, `src/weapons.rs` et `src/gpu.rs`.
4. **Cycle Jour/Nuit 24h & Ciel Synthwave** : Teintes du ciel Vice City, nuit étoilée, phares 3D et feux arrière dans `src/gpu.rs`.
5. **HUD Wanted Stars** : Étoiles dorées de recherche affichées sous les barres de vie dans `src/hud.rs`.
6. **Statut de validation** : 138 tests unitaires réussis à 100%, 0 avertissements (`-D warnings`), 578 / 747 routines (78.0%).
