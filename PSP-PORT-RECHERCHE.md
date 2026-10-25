# Portages PSP de VCS sur PC — références de recherche

Vérifié le 12 octobre 2026. Deux projets distincts sont pertinents. La référence principale pour la logique du jeu est **PSPRecomp**; `vancevcs/VCSPC` est un bon complément pour ses outils d'inspection runtime.

## 1. PSPRecomp : port natif par recompilation statique

Dépôt : [`jessicanataliagta/PSPRecomp`](https://github.com/jessicanataliagta/PSPRecomp) et profil VCS [`profiles/vcs`](https://github.com/jessicanataliagta/PSPRecomp/tree/main/profiles/vcs). Le framework analyse un ELF Allegrex/MIPS, émet du C++ et l'exécute sur hôte PC; le profil VCS inclut le corpus AOT généré du jeu, le host et l'intégration graphique/audio/input. Le rapport généré public annonce 15 988 fonctions découvertes, 775 148 instructions émises et 234 unités C++. C'est une vue native lisible des fonctions PSP, donc une excellente deuxième représentation à comparer au désassemblage PS2.

Le corpus vise la version PSP américaine `ULUS-10160`. Il ne faut pas reprendre ses adresses absolues pour le SLES PS2 PAL. Il faut rechercher des séquences d'opérations équivalentes (offsets relatifs, constantes, branches, formules) et confronter ces équivalences au code PS2 déjà établi. Le projet sépare le code généré, les fonctions host/fast-path et le framework; consulter sa provenance/licence avant toute réutilisation de code. Pour VCS-Rust, garder les observations de comportement et les équations indépendamment documentées, sans importer de source générée ni d'assets.

## 2. VCSPC de vancevcs : fork PPSSPP enrichi pour PC

Dépôt : [`vancevcs/VCSPC`](https://github.com/vancevcs/VCSPC); sa documentation de reverse engineering est [`docs/VCS_ADDRESSES.md`](https://github.com/vancevcs/VCSPC/blob/master/docs/VCS_ADDRESSES.md). Ce projet fait tourner le jeu PSP sous PPSSPP avec des commandes PC et des outils/debug UI. Le README exclut les éditions PSP PAL/JPN de sa table d'adresses. Sa documentation donne une méthode pratique de snapshots, diffs et breakpoints, plus quelques repères de structures : pointeur véhicule occupé `0x08BB4064`, model ID à `PlayerVehicle+0x56`, position monde de l'entité à `+0x30`, base d'orientation à `+0x10`. Elle documente aussi `PlayerBase+0x140/+0x144` comme vitesse du piéton — ce n'est pas une preuve sur le champ voiture.

Son `TimeStep` PSP à `0x08BB3B5C` est présenté comme le facteur de vitesses angulaires de caméra. Ne pas l'assimiler au scalaire PS2 `0x004CD410`, utilisé par l'intégrateur de position et issu d'un paramètre de configuration encore non nommé.

## Démarche concrète pour VCS-Rust

1. Dans le corpus AOT PSPRecomp, localiser les fonctions PSP de contrôle véhicule à partir d'accès au handling, aux commandes, à la vitesse et à la position; s'appuyer sur les adresses du profil PSP et son rapport de fonctions.
2. Relever les équations et constantes, sans reprendre les noms supposés ni les adresses PSP.
3. Chercher dans `SLES_546.22` les mêmes signatures d'opérations et les offsets relatifs homologues. Confirmer chaque correspondance avec des xrefs et les valeurs des records de la PS2 PAL.
4. Utiliser la doc VCSPC pour guider l'inspection runtime PSP seulement si une vérification dynamique est nécessaire; aucun essai manuel utilisateur n'est requis pour le travail statique.
5. Consigner chaque rapprochement comme hypothèse jusqu'à confirmation côté PS2.

Le dépôt homonyme [`GTAmodding/VCSPC`](https://github.com/GTAmodding/VCSPC) est une ancienne adaptation sur le moteur GTA San Andreas; ce n'est pas le port PSP recompilé et ce n'est pas la référence principale ici.

## Inspection du corpus AOT (12 octobre 2026)

Le rapport officiel [`auto_codegen_report.json`](https://github.com/jessicanataliagta/PSPRecomp/blob/main/profiles/vcs/generated/auto_codegen_report.json) confirme le mode `automatic_global_cfg`, le partitionnement par plages d'adresses de 16 Kio, 15 988 seeds de fonctions, 775 148 instructions uniques et 234 unités. Les fichiers C++ générés gardent des étiquettes contenant les adresses virtuelles PSP (par exemple `L_088061A0`); le nom de l'unité seul n'est pas un nom de fonction. J'ai inspecté les 234 unités publiques (103,7 Mio) en lecture seule, sans importer leur code dans VCS-Rust.

Résultat négatif utile : les motifs binaires IEEE-754 des constantes PS2 `2500.0`, `0.00320` et `0.00364` (`0x451C4000`, `0x3B51B717`, `0x3B6E8D11`) ne figurent pas tels quels dans le corpus PSP généré. Cela ne réfute pas une formule homologue : les valeurs PSP peuvent venir des données de handling, être calculées différemment, ou utiliser une échelle distincte. Aucune routine PSP n'est donc encore associée avec certitude à la routine PS2 autour de `0x1A17A0`.

Les unités contiennent des références de PC jusqu'à `0x08CB2400`. La plage `0x08804000–0x08B70000` notée dans VCSPC décrit son propre travail de scan; ne pas l'utiliser comme limite universelle pour le corpus PSPRecomp. Les deux projets ciblent `ULUS-10160`, mais leurs révisions/outils et leur couverture doivent toujours être vérifiés.

### Suite ciblée

1. Dans les unités AOT, trouver les blocs qui lisent les données de handling véhicule et suivent la globale PSP `PlayerVehicle` (`0x08BB4064` selon VCSPC); tenir compte des accès relatifs au registre `$gp`, pas seulement des références absolues littérales.
2. Relever les offsets et opérations dans une fonction candidate, puis vérifier qu'elle transforme commandes/vitesse en mouvement du véhicule avant de la rapprocher de `0x19F9C0`, `0x1A14B4` ou `0x1A17A0` côté PS2.
3. Marquer tout rapprochement comme hypothèse tant qu'une seconde preuve (xrefs, structure ou données PSP) manque. Le résultat négatif des trois constantes n'autorise aucun changement de `src/driving.rs`.
4. Le répertoire temporaire local d'exploration `work/psprecomp-aot` ne fait pas partie du projet final; les liens publics et cette procédure suffisent pour le retrouver si nécessaire.
