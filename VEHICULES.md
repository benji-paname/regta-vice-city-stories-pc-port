# Premier véhicule — analyse préparée

Vérifié sur l'ISO PAL locale le 5 octobre 2026. Le binaire distribué reste la version 0.3 : aucun véhicule pilotable n'y est encore intégré.

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
- Les paramètres de conduite de la Banshee ont une référence dans GAME.DTZ ; leur disposition et leur interprétation ne sont pas encore établies.
- MAIN.SCM est présent dans l'ISO ; son contenu et son exécution restent à analyser.

Ces contrôles valident le décodage des données, pas encore leur rendu dans le moteur ni leur comportement en conduite. Les textures ont été localisées par nom ; leurs pixels doivent encore passer dans le lecteur commun pour ce véhicule.

## Points utiles pour l'intégration Rust

Le bloc MDL véhicule a son pointeur de groupe à +0x20 ; le lecteur du personnage utilisait +0x24. Le groupe contient une liste d'objets et des matrices de nœuds. Chaque géométrie native a ici les flags 0x7 : positions, UV et normales, sans poids de squelette ni couleurs de sommets.

Les ressources MDL utilisent des noms de textures dans leurs matériaux. Les objets déjà embarqués dans GAME.DTZ ont des pointeurs de textures résolus : leur nom se lit dans l'objet texture à +0x10. Ne pas confondre ces deux représentations.

Dans le modèle de véhicule de cette édition, l'ID de roue est à +0x270. Dans CSimpleModelInfo, +0x28 pointe vers une table d'objets : lire le premier pointeur de cette table avant l'objet de roue. La roue 323 est déjà embarquée ; elle n'a pas besoin d'être retrouvée dans une étendue de streaming non vide.

Les variantes endommagées ont ici leur drapeau de dessin désactivé. Exclure également chassis_vlo pour la première apparence détaillée. Préserver l'alpha des matériaux et les possibilités de couleurs de carrosserie ; les lecteurs de carte et du personnage ne suffisent pas à les reproduire entièrement.

## Reprise

Le catalogue et le résumé vérifié sont conservés dans data/vehicle-catalog.json et data/vehicle-probe.json. Les outils reproductibles sont work/probe_vehicle_resources.py et work/probe_vehicle_geometry.py, à la racine du chat. Les géométries intermédiaires restent dans work/vehicle-resource-probe/geometry-decoded.json, avec les pièces et les matrices.

Suite : intégrer un lecteur Rust commun pour ce modèle et les roues, résoudre les textures, vérifier le véhicule assemblé à l'écran en arrière-plan, puis ajouter une simulation de conduite minimale et des collisions avec la carte. La sortie et l'entrée du joueur viendront après le premier véhicule stable.
