# Premier véhicule — analyse préparée

Vérifié sur l'ISO PAL locale le 5 octobre 2026. Le lecteur et le rendu sont intégrés ; le contrôleur Banshee est un prototype arcade, pas une reconstitution des paramètres de conduite PS2.

## Correction de peinture extérieure — 12 octobre 2026

- La couleur claire est lue dans les entrées de palette du modèle Banshee de GAME.DTZ ; la variante la plus claire est retenue pour correspondre à la référence blanche.
- Les textures de détail spécifiques et de salissure sont recolorées uniquement sur les panneaux extérieurs. Les textures du tableau de bord, des sièges, des vitres et des pneus gardent leur rendu d'origine.
- Le rendu OpenGL conserve maintenant l'ordre des matériaux des bandes du modèle, au lieu de regrouper toutes les surfaces par texture.
- Capture rapprochée contrôlée avec `--vehicle-preview fichier.bmp` : 2 237 triangles dessinés, peinture extérieure claire, vitres, plaque et roues visibles. L'éclairage et les reflets du jeu original restent à reproduire.

## Résultats

- Catalogue : 111 entrées de modèles de véhicules, IDs 170 à 280.
- Premier modèle : Banshee, ID 175, dictionnaire de textures 171.
- Modèle lu dans GTA3PS2.IMG : secteur relatif 7 389, 36 secteurs, longueur logique 72 471 octets.
- Texture spécifique : secteur relatif 7 384, cinq secteurs, longueur logique 8 476 octets ; nom banshee868bit_sl.
- Vingt objets de géométrie décodés, dont les variantes endommagées et le modèle simplifié. Treize retenus pour l'apparence initiale détaillée : 1 582 triangles non dégénérés pour la carrosserie et l'intérieur.
- Les comptes de sommets des bandes VIF correspondent aux en-têtes ; positions et coordonnées de textures finies.
- Les matrices hiérarchiques placent les portes, capot, pare-chocs et éléments intérieurs dans le repère du véhicule.
- Quatre points de fixation de roues trouvés dans la hiérarchie.
- Roue commune : modèle 323, géométrie embarquée dans GAME.DTZ. Décodage vérifié : 450 sommets sources et 164 triangles non dégénérés par roue. Diamètre local voisin de 1 ; échelles avant/arrière de la Banshee : 0,7.
- Textures communes retrouvées dans le dictionnaire generic embarqué : carrosserie générique, salissures, plaque, pneus et variantes endommagées.
- Une adresse de table de handling de la Banshee est référencée dans GAME.DTZ ; sa disposition et ses unités ne sont pas encore établies. Le prototype utilise actuellement accélération/freinage et braquage génériques, et ne prétend pas reproduire le handling PS2.
- MAIN.SCM est présent dans l'ISO ; son contenu et son exécution restent à analyser.

Le rendu des données, la résolution des textures et l'assemblage sont intégrés au moteur. L'entrée/sortie, l'animation des roues, le recalage au sol par quatre contacts et une caméra de poursuite sont présents. L'enveloppe de collision reste approximée par des rayons contre les collisions statiques.

## Points utiles pour l'intégration Rust

Le bloc MDL véhicule a son pointeur de groupe à +0x20 ; le lecteur du personnage utilisait +0x24. Le groupe contient une liste d'objets et des matrices de nœuds. Chaque géométrie native a ici les flags 0x7 : positions, UV et normales, sans poids de squelette ni couleurs de sommets.

Les ressources MDL utilisent des noms de textures dans leurs matériaux. Les objets déjà embarqués dans GAME.DTZ ont des pointeurs de textures résolus : leur nom se lit dans l'objet texture à +0x10. Ne pas confondre ces deux représentations.

Dans le modèle de véhicule de cette édition, l'ID de roue est à +0x270. Dans CSimpleModelInfo, +0x28 pointe vers une table d'objets : lire le premier pointeur de cette table avant l'objet de roue. La roue 323 est déjà embarquée ; elle n'a pas besoin d'être retrouvée dans une étendue de streaming non vide.

Les variantes endommagées ont ici leur drapeau de dessin désactivé. Exclure également chassis_vlo pour la première apparence détaillée. Préserver l'alpha des matériaux et les possibilités de couleurs de carrosserie ; les lecteurs de carte et du personnage ne suffisent pas à les reproduire entièrement.

## Reprise

Le catalogue et le résumé vérifié sont conservés dans data/vehicle-catalog.json et data/vehicle-probe.json. Les outils reproductibles sont work/probe_vehicle_resources.py et work/probe_vehicle_geometry.py, à la racine du chat. Les géométries intermédiaires restent dans work/vehicle-resource-probe/geometry-decoded.json, avec les pièces et les matrices.

Suite : poursuivre d'abord l'analyse structurale du handling documentée dans `HANDLING-RECHERCHE.md`; ne pas supposer que les floats apparents ont des unités connues. Ensuite, utiliser uniquement les champs vérifiés pour améliorer la conduite, animer les roues et remplacer l'approximation par une collision de véhicule plus stable. Puis poursuivre avec le trafic.
