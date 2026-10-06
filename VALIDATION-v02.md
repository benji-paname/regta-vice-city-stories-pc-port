# Validation du jalon 0.2

Vérifié le 5 octobre 2026 sur l'ISO locale PS2 PAL SLES-54622.

SHA-256 de l'exécutable PS2 analysé : `1039bd48fbe57f3961c72507bfce526ef524dbe9b374d844a00b9a2db435439d`.

| Zone | Secteurs | Placements lus | Placements avec géométrie | Textures décodées | Géométrie non résolue |
|---|---:|---:|---:|---:|---:|
| BEACH | 623 | 5 103 | 5 103 | 2 683 | 0 |
| MAINLA | 601 | 4 695 | 4 695 | 2 716 | 0 |
| MALL | 1 152 | 478 | 478 | 546 | 0 |

Toutes les références de textures des triangles décodés sont résolues. Ces compteurs décrivent les ressources et les placements lus, pas leur présence simultanée à l'écran ni la complétion du jeu.

## Contrôles réalisés

- Compilation native Windows x64 avec Rust 1.90.0 GNU : réussie, hors ligne et sans avertissement.
- 29 tests unitaires : réussis. Ils couvrent notamment les lectures invalides, les palettes et mipmaps PS2, les collisions, les filtres de zone, les plafonds, les marches et les pentes.
- Lecture des trois zones, de GAME.DTZ et des 16 blocs COL2 : réussie.
- Collisions statiques : 10 265 placements lus, 7 887 avec collisions, aucune référence de modèle manquante. 412 298 triangles, dont les faces des 13 940 boîtes originales ; 8 537 sphères originales traitées séparément.
- Trois points de départ en marche : contact au sol stable après 120 pas de simulation chacun. Les modèles et surfaces touchés sont enregistrés dans `data/verification.json`.
- Six vérifications de fenêtre OpenGL, en vue aérienne et au sol pour chaque zone : huit images affichées par vérification, sorties sans erreur et captures produites.
- Repli logiciel : fenêtre de huit images en marche, sortie sans erreur.
- Captures des trois zones et des trois vues au sol : inspectées visuellement.

Pilote observé : GeForce RTX 5070 Ti, OpenGL 4.6, NVIDIA 617.14. Lors des six vérifications finales, le temps moyen de dessin était d'environ 3,7 à 5,9 ms par image. Ces relevés courts incluent le rendu de la carte et l'aide, sans gameplay ; ils ne constituent pas une mesure de performances du futur port complet. Le rendu logiciel de secours est nettement plus lent.

Les captures actuelles sont `apercu-beach-textures.png`, `apercu-mainla-textures.png`, `apercu-mall-textures.png` et leurs variantes `-marche.png`. `apercu-textures.png` reprend la vue aérienne de Beach. Les anciennes vues sans textures du premier jalon sont conservées sous leurs noms précédents.

## Portée

Les collisions sont celles des placements statiques initiaux. Les objets dynamiques et les activations par script restent à analyser. La visibilité utilise midi et l'état de script initial ; les passes de transparence et le cycle complet du jeu ne sont pas implémentés.

La marche utilise un contrôleur Rust expérimental avec des sondes par rayons. Il peut manquer des obstacles fins et ne reproduit pas la physique de VCS. La correction du point de départ Mall conserve sa dalle originale : la caméra de départ est placée sous cette dalle, dans le passage dégagé.

Aucun personnage animé, véhicule, mission ou partie complète n'a été testé : ces systèmes ne sont pas encore implémentés.
