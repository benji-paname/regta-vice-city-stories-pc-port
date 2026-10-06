# Validation du jalon 0.3

Vérifié le 5 octobre 2026 sur l'ISO locale PS2 PAL SLES-54622.

SHA-256 de l'exécutable PS2 analysé : 1039bd48fbe57f3961c72507bfce526ef524dbe9b374d844a00b9a2db435439d.

## Carte et collisions

| Secteur | Secteurs de données | Placements lus | Textures | Géométrie non résolue |
|---|---:|---:|---:|---:|
| BEACH | 623 | 5 103 | 2 683 | 0 |
| MAINLA | 601 | 4 695 | 2 716 | 0 |
| MALL | 1 152 | 478 | 546 | 0 |

Les références de textures des triangles décodés sont résolues. Les ressources sont décodées indépendamment de leur visibilité à l'écran.

Beach et Mainland sont dessinés ensemble avec une distance de 5 km et leurs collisions communes. Une capture depuis la destination BEACH montre les bâtiments, hangars et clôtures de la base militaire : les données sont bien dans l'ISO, le filtre de secteur empêchait leur affichage dans la version précédente. Les copies strictement identiques entre archives sont comparées par leur contenu visuel.

Collisions : 10 265 placements lus, 7 887 avec collisions, zéro modèle manquant, 412 298 triangles comprenant les faces de 13 940 boîtes, et 8 537 sphères originales. Les trois points de départ restent au sol après 120 pas stationnaires. Le point des abords de la base est également vérifié au sol.

## Personnage original

Vic : modèle 0, 25 os, 2 581 sommets sources, 1 644 triangles dessinables et huit textures. Les 915 triangles dégénérés des bandes sources ne sont pas dessinés.

| Animation originale | Durée | Pistes de rotation | Clés |
|---|---:|---:|---:|
| IDLE_stance | 8,0667 s | 21 | 185 |
| WALK_player | 1,3333 s | 22 | 570 |
| run_player | 0,7333 s | 22 | 340 |

L'erreur maximale de reconstruction de la pose initiale est 2,98 × 10⁻⁷ ; les poids normalisés ont une erreur mesurée nulle. Trois poses par animation ont été calculées : toutes les positions sont finies, les poses changent et l'échantillon à la durée complète revient au début de la boucle. Les captures dans les trois secteurs montrent le personnage texturé et articulé.

## Contrôles réalisés

- Compilation Windows x64 en mode optimisé, dépendances figées, hors ligne : réussie sans avertissement.
- 42 tests unitaires réussis : lectures invalides, textures PS2, collisions, plafonds et marches, filtres extérieurs, comparaison des copies visuelles, transparence, interpolation des rotations et caméra bloquée par un mur.
- Sept vérifications OpenGL de huit images chacune : base vue depuis BEACH, Vic dans les trois secteurs, deux déplacements et abords de la base au sol.
- Une vérification logicielle de huit images, avec Vic : réussie.
- Les huit captures ont été produites ; les vues de la base, de Vic dans les trois secteurs, de la course et du secours logiciel ont été inspectées visuellement.
- Déplacement de deux secondes sur Mainland : environ 3,60 m en marche et 9,00 m en course ; le personnage reste au sol.
- Le binaire installé est une copie de celui validé ; l'empreinte est dans data/distribution.json.

Pilote : NVIDIA GeForce RTX 5070 Ti, OpenGL 4.6, NVIDIA 617.14. Les essais courts mesurent 3,2 à 15,6 ms de dessin par image en OpenGL et environ 277 ms en logiciel. Le chronométrage exclut le calcul de la pose ; il ne mesure pas les performances d'un jeu complet.

Les rapports exacts sont dans data/validation-v03.json et data/v03-*.json. Les captures actuelles sont apercu-base-militaire.png et apercu-v03-*.png. La validation précédente est conservée dans VALIDATION-v02.md.

## Portée et limites

Les rotations proviennent des animations originales ; la translation de racine est bloquée et le contrôleur Rust assure le déplacement. Aucun ajustement des pieds au terrain, mélange de transitions ou animation de saut n'est encore implémenté.

Les collisions restent celles des placements statiques initiaux. La caméra teste un rayon central. La visibilité utilise midi et l'état initial des scripts ; les niveaux de détail, l'eau, les passes sous-marines et les autres intérieurs restent incomplets.

Les véhicules, autres piétons, IA, missions, combat, sauvegarde et son n'ont pas été validés : ces systèmes restent à implémenter. Cette étape constitue un explorateur avec un premier personnage animé, pas encore le jeu complet.
