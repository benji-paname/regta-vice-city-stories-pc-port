# Progression et essais

Préférence de Benjamin : avancer étape par étape, avec le minimum d'actions manuelles. **Dernière consigne : il n'a plus besoin de tester pour l'instant ; ne plus ouvrir automatiquement l'explorateur.** Vérifier en arrière-plan et annoncer les étapes effectivement terminées.

1. **Premier explorateur — terminé** : ISO identifié, lecteurs Rust, géométrie des trois secteurs et caméra libre.
2. **Version 0.2 — terminée** : textures, ressources conditionnelles, rendu accéléré, collisions statiques et marche expérimentale. Validation historique : VALIDATION-v02.md.
3. **Version 0.3 — terminée** : carte extérieure continue, correction de la visibilité vers la base militaire, transparence, Vic original avec attente/marche/course, caméra à la troisième personne avec obstacle. 42 tests et huit vérifications natives réussis, sans demander d'essai manuel.
4. **Premier véhicule — géométrie, roues et rendu terminés** : modèle Banshee (ID 175) et roues partagées (ID 323 de GAME.DTZ) décodés nativement en Rust. 13 atomics visibles (1 582 triangles), 4 roues instanciées (656 triangles) soit 2 238 triangles au total. Textures spécifiques (`banshee868bit_sl`) et communes (`tyres`, `xv_vehiclegeneric_sl`, `xv_vehiclegrunge_sl`, `xv_vcs_licenseplates_cs`) mappées. Rendu accéléré et logiciel intégrés, validé par 45 tests unitaires et rendu natif sans régression (`apercu-banshee.png`). Prochaine étape : simulation de conduite élémentaire et entrée/sortie du joueur.

La conduite, le trafic, l'IA, les missions, le combat, la sauvegarde et le son restent à développer. Le port complet n'a pas de date de livraison établie.

Le projet et les outils sont sur C:. L'ISO originale est lue sur F: sans déplacement ni modification. Les résultats détaillés sont dans VALIDATION.md et data/validation-v03.json.
