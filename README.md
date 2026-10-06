# VCS Rust 0.3 — carte continue et Vic animé

État au 5 octobre 2026 : **exploration native avec carte texturée, Vic animé et caméra à la troisième personne**. Ce prototype Rust lit votre ISO PS2. Les missions et le gameplay complet restent à réimplémenter.

## Ouvrir

Double-cliquer sur VCS-Rust.exe. Le programme démarre en marche avec Vic visible. Aucun BIOS PS2, émulateur ou installation de Rust n'est nécessaire.

ISO utilisée : F:\JEUX EMULATION\PS2 GAMES\Grand Theft Auto - Vice City Stories.iso

Le projet est sur C: et l'ISO reste sur F:, sans modification. Seule l'édition PAL SLES-54622, avec l'empreinte vérifiée de son exécutable, est prise en charge.

## Commandes

- ZQSD ou WASD : avancer et se déplacer de côté.
- Flèches : orienter la vue ; Maj gauche : courir.
- F2 : basculer entre la troisième personne et la première personne.
- G : basculer entre marche et caméra libre.
- Espace : sauter en marche ; Espace / Ctrl gauche : monter / descendre en caméra libre.
- 1 / 2 : rejoindre Beach / Mainland ; les deux parties extérieures restent chargées ensemble.
- 3 : rejoindre Mall ; 4 : rejoindre les abords de la base militaire.
- T : activer ou désactiver les textures ; F1 : afficher ou masquer l'aide.
- R : revenir au dernier point de départ ; Échap : fermer.

## Nouveautés de cette étape

Le manque de carte vers la base militaire venait du chargement d'une seule partie extérieure à la fois et d'une distance d'affichage limitée. Beach et Mainland sont maintenant réunis, avec les collisions correspondantes et une distance de dessin de 5 km. Les copies visuelles strictement identiques présentes dans les deux archives sont éliminées, en comparant géométrie, placement et textures. Mall reste un intérieur séparé.

Vic utilise le modèle, les huit textures, les 25 os et les trois animations originales lus dans votre ISO : attente, marche et course. Les rotations des os sont interpolées ; le contrôleur Rust déplace le personnage. La caméra suit Vic et se rapproche lorsqu'une collision statique bloque sa vue.

Les surfaces transparentes des passes 5 et 7 sont dessinées après les surfaces opaques, de loin en proche.

## Fonctions déjà présentes

- Lecteur ISO9660, vérification SHA-256 et décompression des ressources.
- Géométrie PS2 VIF, placements, textures à palettes, alpha, mipmaps et filtrage.
- Références de géométrie et de textures résolues pour les placements décodés des trois secteurs.
- Rendu Windows accéléré via OpenGL, avec secours logiciel.
- Collisions originales statiques : triangles, boîtes et sphères.
- Marche, gravité, saut, petits obstacles et glissement contre les murs à pas fixes.

Le programme dessine les données avec son propre code Rust ; il n'exécute pas le programme PS2. Les compteurs de ressources ne mesurent pas la complétion du jeu.

## Limites

Le contrôleur ne reproduit pas encore la physique originale. Les animations conservent leurs rotations originales mais la translation de leur racine est bloquée : le déplacement vient du contrôleur. Il n'y a pas encore de correction des pieds selon le terrain, de transition fondue entre animations ni d'animation de saut. Les pieds peuvent donc glisser ou se décaler légèrement du sol. La caméra utilise un rayon central ; ses bords peuvent croiser un obstacle fin.

Conduite, trafic, autres piétons, IA, missions, combat, sauvegarde et son restent à développer. Les changements d'objets par script, les collisions dynamiques et le streaming final restent incomplets.

La visibilité utilise midi et l'état initial des scripts. Les niveaux de détail, les passes sous-marines, l'eau et les autres intérieurs ne sont pas encore reproduits complètement. Le filtre de collisions conserve les objets globaux et les deux niveaux extérieurs, ou Mall ; trois objets du niveau 250 restent exclus.

## Vérification et compilation

Les sources sont dans src/ et les dépendances sont figées dans Cargo.lock. Compiler.ps1 utilise les outils portables déjà préparés sur C: ; compilation et tests fonctionnent hors ligne. L'exécutable final n'a pas besoin d'internet.

Options de diagnostic :

- --verify : vérifier les ressources, les contacts au sol et les animations sans fenêtre.
- --walk / --fly : choisir marche ou caméra libre au démarrage.
- --first-person : démarrer sans afficher le personnage.
- --zone BEACH|MAINLA|MALL : choisir le point de départ.
- --headless : effectuer huit images de vérification avec une fenêtre masquée.
- --capture fichier.bmp : enregistrer le rendu puis fermer après huit images.
- --smoke : effectuer huit images dans une fenêtre visible.
- --software : utiliser le rendu logiciel.
- --preview fichier.bmp : produire une image de diagnostic en logiciel.
- --camera x,y,z,yaw,pitch : imposer une position et une orientation.
- --animation idle|walk|run|bind et --animation-time secondes : choisir une pose de diagnostic.
- --simulate secondes : simuler un déplacement en mode --headless --walk, au maximum 30 secondes.
- --iso chemin.iso et --output dossier : choisir les données et les diagnostics.

VALIDATION.md et data/validation-v03.json consignent les résultats. Les aperçus nommés apercu-v03-* et apercu-base-militaire.png sont issus de cette version ; les captures précédentes restent conservées.

## Recherche et crédits

Formats comparés avec [aap/librwgta](https://github.com/aap/librwgta) et [spicybung/Storyland](https://github.com/spicybung/Storyland). [SK8-ENGINE/skate-3-rust-engine](https://github.com/SK8-ENGINE/skate-3-rust-engine) et [vladtrc/iw4L](https://github.com/vladtrc/iw4L) ont servi de références pour l'approche de réécriture native.

Le code du prototype a été écrit pour ce projet. Les dépendances conservent leurs licences. Les ressources GTA viennent de votre copie locale ; les sources ne contiennent pas d'archive de ressources originales à redistribuer.
