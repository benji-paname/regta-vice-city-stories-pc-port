# Plan de développement

Priorité confirmée le 5 octobre 2026 : progresser vers une portion jouable cohérente, puis étendre les systèmes à la ville. Limiter les essais manuels de Benjamin ; ne pas ouvrir automatiquement l'explorateur.

Le chemin principal est : monde et joueur → véhicule → PNJ et trafic → mécaniques → première mission complète. L'analyse des scripts commence avant les missions pour repérer leurs besoins. La fidélité complète de la carte et les variantes de contenu viennent progressivement, sans bloquer chaque système sur tous les détails du précédent.

## Déjà acquis — version 0.3

Lecture native de l'ISO, trois secteurs décodés et texturés, carte extérieure continue, collisions statiques, Vic original avec attente/marche/course et caméra à la troisième personne. Cette base est un explorateur ; la simulation du jeu PS2 n'est pas exécutée.

## 1. Stabiliser le joueur et une zone extérieure

Améliorer les transitions attente/marche/course et le comportement lors des déplacements latéraux, du saut et de l'arrêt contre un obstacle. Vérifier escaliers, pentes et passages étroits avec les collisions originales. Examiner les surfaces d'eau et les omissions visibles qui gênent le déplacement. Conserver l'accès aux trois secteurs.

Critère de passage : parcourir un petit trajet de référence, marcher, courir, sauter et changer de vue sans perdre le contact attendu avec le monde. Ne pas attendre de reproduire tous les détails de toute la carte.

## 2. Un premier véhicule original

**État au 12 octobre 2026 : prototype jouable intégré et visuellement validé par Benjamin.** La Banshee est rendue, on peut y entrer et en sortir avec E, accélérer, freiner et tourner. Les détails extérieurs (bande noire, feux rouges, plaque), une collision approximative et une caméra suiveuse sont présents. Le handling PS2, les roues animées et une collision robuste restent à faire avant de considérer le jalon stable.

Étudier les champs du handling dans les données originales de l'ISO, en déterminant structure et unités avant de les utiliser. Ensuite, améliorer progressivement la dynamique du véhicule, faire tourner les roues visibles, et renforcer les collisions. Ne pas attribuer de sens à un champ sur la seule base de sa valeur apparente.

Critère de passage : entrer, conduire un trajet court avec accélération/freinage/virages, heurter un obstacle et sortir en retrouvant le contrôle du personnage. Tester ensuite les autres modèles ; ne pas commencer par tout le parc automobile.

## 3. Quelques PNJ et du trafic

Décoder d'autres personnages et les données de chemins. Mettre en place un petit nombre d'entités, leurs états, déplacements et réactions au joueur. Introduire un premier véhicule de trafic sur un trajet vérifié, puis augmenter le nombre d'entités et mesurer le coût.

Critère de passage : PNJ et voiture de trafic visibles, mobiles, soumis aux collisions et capables de réagir à une interaction simple. Ne pas viser d'emblée toute la population de Vice City.

## 4. Mécaniques essentielles

Santé et dégâts, combat, première arme, réactions des PNJ et premiers niveaux de recherche de la police. Ajouter un état de jeu sérialisable et une sauvegarde locale minimale lorsque ces états sont définis. Réutiliser une représentation commune des entités pour le joueur, les PNJ, les véhicules et les scripts.

Critère de passage : une scène reproductible combine déplacement, conduite, interaction, dégâts et réaction de police ; l'état nécessaire peut être sauvegardé et repris.

## 5. Première mission complète

En amont, identifier les fichiers de scripts, leur format et les opérations nécessaires à une mission simple. Préparer un lecteur et une trace de diagnostic avant d'implémenter les comportements. Construire progressivement l'exécution des opérations nécessaires, avec variables, temporisations et références aux entités.

Critère de passage : déclencher une mission originale choisie après analyse, afficher ses objectifs, avancer entre ses étapes, réussir ou échouer, puis revenir à l'exploration. Une première mission sert à vérifier ensemble les systèmes avant d'élargir la compatibilité des scripts.

## Extension et fidélité

Étendre les véhicules, personnages, armes et opérations de scripts ; compléter eau, visibilité, intérieurs, collisions dynamiques et changements d'objets. Ajouter son, radio, cinématiques, menus, streaming et compatibilité des sauvegardes selon les besoins vérifiés. Mesurer les performances sur des scènes de plus en plus complètes.

Chaque étape produit un exécutable, des diagnostics et des critères vérifiés. Les étapes peuvent nécessiter plusieurs jalons. Aucune durée précise n'est annoncée avant analyse des formats et comportements concernés.

## Estimation provisoire demandée par Benjamin

Les fourchettes annoncées initialement dans le chat étaient trop peu étayées pour constituer un délai fiable. Aucun budget d'heures mesuré n'est actuellement établi. Réviser l'estimation après un premier véhicule utilisable et l'analyse des scripts, en s'appuyant sur les difficultés effectivement rencontrées. La proximité de la fin d'essai justifie de concentrer le travail sur des jalons utilisables et de conserver toutes les sources, outils et vérifications localement. Cela ne permet pas de garantir une bêta complète en deux jours, ni du travail automatique pendant que le chat est inactif.
