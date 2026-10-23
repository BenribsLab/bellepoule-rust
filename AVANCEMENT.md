# État d'avancement du portage — mise à jour du 22/10/2026

**→ Pour reprendre le travail : lire `REPRISE.md` (point d’arrêt exact et suite).**

**22/10 — Cartons Android 1.3 (versionCode 4)** : bouton jaune/rouge/noir → couleur en
plein écran, chrono en pause → appui sur l'écran → confirmation → enregistrement sur le match.
Retour ou annulation n'ajoutent aucun carton. APK : `smartpoule-android/SmartPoule-1.3.apk`.
Compilation de livraison, sans test interactif.

**22/10 — Disposition Android 1.2** : bandeau du match placé dans un agencement vertical
avec le contenu, thème sans ActionBar automatique et marges système/encoches explicites.
Le chrono reste sous la barre au lieu de passer derrière. Livraison `SmartPoule-1.2.apk`
(versionCode 3), sans lancement de l'application ni test interactif.

**22/10 — APK SmartPoule livré** : l'ancien fichier d'installation était encore en 1.0. Construction
réussie de la **1.1 (versionCode 2)** après correction d'une accolade dans le compteur de cartons.
Installer `smartpoule-android/SmartPoule-1.1.apk`, puis ouvrir un match pour voir le chrono.
Pas de test interactif ; cette livraison Android ne reconstruit ni le serveur web/API ni le bureau.

**22/10 — Gel PC / SmartPoule corrigé dans les sources** : la feuille d'une poule reprenait le
même verrou pour lire sa durée alors que le tour était déjà verrouillé. Cela bloquait l'instantané
SmartPoule dans le fil GTK. Lecture directe de `round.bout_timing` désormais utilisée, côté PC
et dans la copie web. Non compilé/non testé ; ancienne instance à fermer avant relance.

**22/10 — Chrono et cartons SmartPoule** : paramètres de formule partagés web/bureau (poules
5 touches, 1 × 3 min ; tableaux 15 touches, 3 × 3 min), transmission à Android, décompte monotone,
manches, inversion rouge/vert (premier nom vert par défaut), cartons par tireur avec journal et annulations.
État enregistré sur le match, API/relais et impressions adaptés ; copie du cœur web synchronisée.
Voir `CHRONO_CARTONS.md`. **Code écrit, non compilé/non testé à la demande de l'utilisateur ;
APK non reconstruit, aucun service redémarré.** Les cartons sont consignés, sans sanction automatique.

## Synthèse actuelle pour une autre IA

**La section « Transmission prioritaire » de `REPRISE.md` fait foi pour les dernières décisions.**
Les estimations en pourcentage et étapes plus anciennes ci-dessous sont historiques ; elles ne
constituent pas une mesure de la validation de la nouvelle interface.

| Chantier | Réalisé | État de validation |
|---|---|---|
| Unification | Une fenêtre BellePoule, bibliothèque Marshaller embarquée, échanges locaux en mémoire | Compilée ; essais utilisateur à poursuivre |
| Organisation facultative | Aucun outil, arbitres seuls, pistes seules ou les deux ; options mémorisées | Quatre modes codés ; validation interactive restante |
| Ressources | Catalogue persistant, disponibilités, arbitres sans piste et pistes sans arbitre | Persistance et affectations à confirmer en utilisation |
| Poules multi-pistes | Plusieurs pistes, arbitre par piste, vagues de matchs et indications sur les feuilles | Extension écrite ; scénario complet à valider |
| Navigation | Accueil au démarrage, Organisation à la demande, fermeture explicite, retour à la compétition | Nouvelle structure codée ; retours utilisateur attendus |
| Graphisme | Maquette moderne : barre latérale, bandeau illustré, cartes, actions compactes, tableaux harmonisés | CSS accepté par GTK ; rendu final non validé par l'agent |
| Appel | Recherche multi-mots, filtres de présence, compteur de lignes, tri et équipes conservés | Tests écrits/compilés, non exécutés ; dernières retouches non recompilées |

**Lancement : `lancer.bat`**, puis créer/ouvrir une compétition. L'ancien Marshaller séparé reste
disponible, mais n'est plus requis. Organisation est accessible depuis la barre latérale ; fermer
sa vue conserve les données et les affectations. Seules les armes des compétitions ouvertes sont
affichées ; sans compétition, toutes restent accessibles pour préparer les arbitres.

Fichiers d'entrée pour poursuivre :

- Unification : `bellepoule/src/management.rs`, `network.rs`, `marshaller/src/ui/mod.rs`,
  `resources.rs`, `assign.rs`, `ui/pool_resources.rs`, `ui/job_referees.rs` et `ui/embedded_layout.rs`
  (sous `crates/`). Conception : `INTEGRATION.md`.
- Graphisme : `bellepoule/src/shell.rs`, `visuals.rs`, `app.rs`, `contest_page.rs`,
  `schedule_view.rs`, `bp-gtk/src/appearance.rs`, `appearance.css`, `glade.rs`,
  `bellepoule/src/stages/checkin.rs` et `widgets/players_list.rs`. Conception : `DESIGN.md`.
- Images générées : `resources/visuals/fencing-header.png`, `fencers-sidebar.png` ; icônes SVG
  dans `resources/visuals/icons`. Prompts et provenance : `resources/visuals/README.md`.
- Suivi des plantages : `%LOCALAPPDATA%\BellePoule\logs\bellepoule-panic.log`.

**Consigne utilisateur en vigueur : arrêter les vérifications, il les fait lui-même.** Ne pas
relancer de tests, compilation ou application de sa propre initiative. Attendre ses retours sur
le rendu et les scénarios fonctionnels. Aucun commit de ces derniers travaux n'a été créé ;
préserver les modifications existantes du répertoire.

## Journal des changements récents

**Maquette utilisateur intégrée** — nouvelle structure avec barre latérale, accueil, bandeau illustré
de compétition, cartes de présence et recherche/filtres fonctionnels dans l'Appel. Deux images générées
et intégrées dans `resources/visuals` ; SVG de navigation cohérents. Menus historiques conservés en haut,
navigation des phases limitée à celles déjà accessibles. Aucun essai interactif par l'agent.
**L'utilisateur a demandé l'arrêt des vérifications et prend les tests à sa charge.**

**Refonte graphique autorisée (21/10)** — noms et emplacements généraux des menus conservés,
thème clair moderne partagé, boutons et onglets compacts, tableaux plus lisibles. Correction
des expansions GTK2 : commandes de l'Appel et des phases à hauteur naturelle ; listes et canevas
gardent l'espace. Actions de l'Appel adaptables à la largeur. Couleur de compétition en bordure.
Conception dans `DESIGN.md`. CSS accepté par GTK ; validation visuelle restant à faire par l'utilisateur.

**Organisation : navigation et place des panneaux** — accueil BellePoule par défaut ; bouton
Organisation dans l'en-tête et bouton explicite pour fermer la vue sans perdre les affectations.
Disposition compacte : arbitres à gauche, pistes et matchs superposés à droite, séparateurs
réglables, commandes sans étirement vertical et listes défilables. Validation interactive restante.

**Organisation : filtrage des armes** — seuls les onglets d'arbitres correspondant aux compétitions
ouvertes sont affichés ; sans compétition, les quatre armes restent accessibles pour préparer le
catalogue. Masquage uniquement, sans suppression des arbitres. Compilation vérifiée.

**21/10 — Plantage à la fermeture d'une compétition corrigé** : le journal de 21:21 situe
la panique dans `App::close_page` (`RefCell already borrowed`). La liste des onglets est désormais
modifiée dans un seul emprunt, libéré avant le retrait GTK. Compilation vérifiée ; validation
interactive Organisation → poules et fermeture d'une compétition à faire par l'utilisateur.

**21/10 — Organisation facultative intégrée à BellePoule** : un seul lancement `lancer.bat` ;
onglet Organisation avec deux options indépendantes, arbitres et pistes. Les quatre usages
(aucun, arbitres seuls, pistes seules, les deux) sont branchés. Synchronisation locale en mémoire,
catalogue d'arbitres persistant et affectation d'arbitres sans piste obligatoire. Voir `INTEGRATION.md`.
Compilé, tests compilés non exécutés ; validation interactive restante. Le Marshaller séparé reste
disponible pour l'ancien usage, sans être nécessaire au fonctionnement intégré.

**État historique avant les derniers changements** : le portage était indiqué comme fusionné dans
`master`, avec un `cargo check --workspace` réussi. Cela ne décrit pas les modifications récentes
de l'unification et du graphisme, encore présentes dans le répertoire de travail.
Aucun agent n'est actif. On avance **un chantier à la fois**, et les tests sont lancés
par l'utilisateur à la demande (voir « Tests demandés » en bas).

Légende : ✅ fait · 🟡 écrit, compile, non vérifié · ❌ pas commencé

## Vue d'ensemble

| Chantier | État | Avancement |
|---|---|---|
| Fondation commune | ✅ | 100 % |
| LivePoule + détecteur USB | ✅ | ~95 % |
| Réseau (`bp-net`) | 🟡 | ~85 % |
| Imports / exports (`formats`) | 🟡 | ~75 % |
| Tableaux, barrage, scission | ✅ logique terminée (hors réseau et écrans) | ~90 % |
| Poules | ✅ logique terminée (hors réseau et écrans) | ~90 % |
| Compétition, pointage, classement | ✅ logique terminée (hors réseau) | ~90 % |
| Socle graphique + fenêtre principale | 🟡 fenêtre et tous les types de phases écrits ; validation interactive restante | ~45 % |
| Marshaller | 🟡 poules et tableaux reliés à BellePoule (salle, frise, lots, arbitres, smartphones) ; à valider | ~85 % |
| Vague 2 (écrans des phases, impression, liaison réseau) | 🟡 tous les types de phases et leurs impressions compilés, non vérifiés | à réévaluer |

**Global : ~55 %.**

---

## Ordre des chantiers restants (un seul à la fois)

Chaque étape débloque la suivante. Ne rien lancer en parallèle.

### Étape 1 — Compétition ✅ TERMINÉE (20/10/2026)
Fait : `contest.rs` (propriétés, lecture/écriture du `.cotcot`, duplication, import AskFred et listes de
tireurs, archive ZIP), `schedule.rs` (chaînage des phases, fournisseur d'entrée, navigation, verrouillage,
suppression, phases ajoutées pour les fichiers externes), `tournament.rs` (ouverture de tous les types de
fichiers, fichiers récents, sauvegardes, liste des compétitions), liste des arbitres, `sort.rs` (tri
compatible GLib, nécessaire aux tirages au sort), test CSV corrigé, 3 avertissements réseau supprimés.
Vérifié par des tests sur vos fichiers d'exemple : `.cotcot` (aller-retour stable), FFF, ZIP, XML FIE
individuel et par équipes.
**Reste de cette étape (reporté volontairement)** :
- `score_collector` et `publication` : dépendent du réseau et des écrans de saisie → étapes 6 et 8 ;
- aiguillage des messages réseau (`Tournament::OnMessage`, `Contest::OnMessage/ManageReferee`) → étape 8 ;
- en attendant les poules, le tour de poules d'un fichier est conservé tel quel (`RawStage`) : l'ouverture et
  l'enregistrement ne perdent rien.

### Étape 2 — Poules ✅ TERMINÉE
Fait : l'allocateur et le superviseur sont de vraies phases (`PoolAllocatorStage`, `PoolSupervisorStage`)
qui partagent un tour (`PoolRound`) ; composition des poules, séparation, saisie, classement, qualifiés,
XML `<TourDePoules>` ; `Schedule` gère maintenant le fournisseur d'entrée, les noms partagés, le propriétaire
des données joueurs, le rang final des éliminés et le classement général.
Vérifié : les **46 poules de `melun.cotcot`** (victoires, touches données et reçues, place) sont identiques
au fichier d'origine ; ordre des matchs FIE conforme à `test.cpp` ; compétition complète de 24 tireurs
(pointage → poules → tableaux → classement final) ; enregistrement stable.
Défauts corrigés : arbitres écrits comme des tireurs (listes distinctes), ordre des arbitres inversé à chaque
relecture, tri aléatoire qui faisait planter Rust (`sort.rs`).
**Reste (reporté)** : dialogues « retardataires/absents » (la logique est dans `PoolRound`, l'écran est à faire),
impression de l'allocateur, aiguillage des messages réseau (`SmartPoule::Score`…) → étapes 5, 6 et 8.

### Étape 3 — Tableaux ✅ TERMINÉE
Se jouent de bout en bout avec toutes les options (aucune / 3e place / toutes les places) pour 8, 13, 20 et
33 tireurs ; barrage (ex æquo à la limite du quota) et point de scission testés ; enregistrement et relecture
d'un tableau à moitié joué sans perdre un score ; renumérotation des tireurs au chargement. Le C++ ne sait
pas non plus relire les tableaux des fichiers 3.x (identifiant de suite par chemin d'arborescence).
Exports branchés sur la compétition : HTML, FFF du classement final, CSV, archive ZIP des étapes.

### Étape 4 — Réseau et formats : nettoyage
- réseau : pages PHP du site web non portées ; FTPS absent ;
- formats : intégration avec la compétition ; aucun test écrit sur `tests-data/`.

### Étape 5 — Socle graphique et application `bellepoule` ← EN COURS
Fait : canevas cairo complété (couleurs, vue défilante/zoomable, icônes, contexte d'impression), les 17 écrans
`.ui` valident avec `gtk4-builder-tool`, fenêtre principale (menus, ouverture de tous les types de fichiers,
fichiers récents, exemples, modèle CSV, sauvegarde, À propos, quitter), onglet de compétition (couleur,
propriétés, calendrier, enregistrement automatique 5 s, fermeture), calendrier des phases (formule, précédent /
suivant, barrage, erreurs). Lancement vérifié par capture d'écran.
Écran d'appel (liste des tireurs avec icônes et cases de présence, tri, ajout/modification par le formulaire,
suppression, import FFF/CSV, import de classement, colonnes à afficher, présents/absents, équipes) ✅.
**Fait** : menu des langues, impression de l'appel, impression de la formule. **Reste** : validation interactive des phases (étape 6).

### Étape 6 — Écrans des phases (vague 2) ← EN COURS
🟡 Composition des poules branchée : formules, séparation, déplacements, filtres, liste,
retardataires/absents et formulaire ; impression de la composition/liste.
🟡 Superviseur de poules : matrice et ordre des matchs, saisie des scores et avance du focus,
statuts, résultats, classements courant/cumulé, paramètres de phase, impressions
(arbitres/résultats/classements, une ou toutes les poules).
🟡 Tableaux : arbre cairo partagé écran/impression, navigation, zoom, saisie directe/rapide,
recherche, statuts, suites de classement, paramètres, filtres et classement. Aperçu/pagination
4/8/16, feuilles individuelles/par tableau, toutes/en attente, portrait/paysage.
Magasin distinct des arbitres utilisé pour le dessin et les feuilles. Vue du canevas corrigée
(retrait des anciens champs, regroupement des rafraîchissements, stockage réutilisé).
Le calendrier expose l'onglet propre de l'allocateur et les paramètres des phases futures.
Ces ajouts sont **compilés, sans validation interactive** ; la fidélité exacte du rendu GTK/cairo
et des impressions doit encore être comparée à la référence.
🟡 Barrage : cases de promotion, compteurs, classement, filtres et impression.
🟡 Scission : sélection et nom ; nouvelle compétition dans un onglet, classement d'entrée conservé,
équipes et membres copiés avec leurs propres liens. Présence recalculée après copie.
🟡 Classement général : liste, filtre, impression et exports HTML/FFF/PDF/AskFred, place du vainqueur
pour FFF et dossier d'export mémorisé. Documents cairo paginés communs aux trois écrans.
AskFred lit maintenant le magasin distinct d'arbitres. Le recalcul du classement général restaure
les rangs bruts avant de décaler les exclus/exportés pour éviter les décalages répétés.
Ces derniers ajouts sont également **compilés, sans validation interactive**.
**Reste** : validation des écrans, arbitres/planning et réseau (dont QR des feuilles de tableau).
`cargo check --workspace --tests`, `cargo build --workspace` et `cargo clippy --workspace --tests`
passent ; avertissements préexistants. Tests compilés, non exécutés. Régressions ajoutées :
saisie Unicode, plusieurs retardataires avant renumérotation, arbitres/pagination/feuilles en attente
des tableaux, stockage du canevas après reconstructions répétées. Les deux UI des tableaux valident
avec `gtk4-builder-tool validate`, ainsi que barrage, scission et classement général.
Nouvelles régressions compilées : scission individuelle/par équipes, relecture et liens autonomes ;
AskFred avec collision arbitre/tireur, décalage FFF sans mutation et recalcul stable du classement.
Aucun lancement interactif de l'application.
Détails et scénarios : `REPRISE.md`.

### Étape 7 — Marshaller
Logique restante (`batch`, `job`, `job_board`, `timeline`, `lasso`, `clock`, `smartphones`…), interface
complète (7 `.glade` à convertir, dessin de la salle et de la timeline), réseau.

### Étape 8 — Liaison réseau et finitions
BellePoule ↔ Marshaller ↔ LivePoule ↔ smartphones via `bp_net::Ring` ; LivePoule sur Raspberry Pi
(feature `raspberry`, jamais compilée) ; traductions ; installeur.

---

## Détail par chantier (ce qui est écrit)

**Fondation** : `attribute`, `player`, `xml`, `stage` (trait `Stage`, `PreviousStage`, `RawStage`), `bout`
(Match, Score), `i18n`, `bp_net::Message`.

**LivePoule + USB** : écran GTK4, chronomètre, lampes, boutons, appareil de score, Wi-Fi (wpa_supplicant),
QR code, serveur HTTP chiffré ; détecteur USB (libusb / API native Windows). 35 tests passent.

**Réseau** : anneau de partenaires (UDP, poignée de main, battement de cœur), serveur HTTP, AES-256-CBC,
QR codes, envois HTTP/FTP/GREG/escrime-info, USB, console, site web intégré.

**Formats** : encodages, FFF, CSV, XML FIE, classements, AskFred (lecteur + écrivain), HTML,
pays/clubs/ligues/régions, JSON, config.ini, ZIP.

**Tableaux** : `table_set`, `table`, `supervisor`, `layout`, `print`, `html`, `barrage`, `splitting`.

**Poules** : tables FIE 2-17 tireurs, `neo_swapper`, `pool` (scores, classement, XML), `round` (état du tour).

**Compétition** : `weapon`, `category`, `fie_time`, `filter`, préférences, générateur aléatoire GLib,
joueurs et équipes, export HTML, phase de pointage, classement général.

**Socle graphique** : 17 `.ui` GTK4 (script de conversion `bp-gtk/tools/`), canevas, dialogues, ressources.

**Marshaller** : salle, pistes, créneaux, planning, affinités, arbitres.

---

## Tests demandés à l'utilisateur
- Ne lancer que `lancer.bat`. Dans Organisation, essayer aucune option, arbitres seuls, pistes
  seules puis les deux. Importer/ajouter des arbitres, double-cliquer une poule et utiliser
  « Affectations… » sans piste ; en mode pistes seules, affecter plusieurs pistes sans arbitre.
  Vérifier la conservation des données en changeant de mode puis après fermeture/réouverture.

- Dialogue des ressources : vérifier que les arbitres occupés ailleurs à l'horaire de la poule
  sont masqués, que les arbitres de la poule restent visibles et qu'un arbitre sélectionné sur
  une piste n'est plus proposé sur une autre piste cochée.

- Glisser-déposer multi-pistes corrigé : reprendre une poule déjà placée dans la liste « Matchs »
  et la déposer sur une deuxième piste libre. Le dialogue s'ouvre avec les deux pistes cochées ;
  choisir l'arbitre de la nouvelle piste puis OK. Annuler doit laisser la première affectation intacte.

- Extension multi-pistes : clic droit sur une piste occupée dans Marshaller → « Pistes et arbitres
  de la poule… ». Choisir plusieurs pistes et un arbitre distinct pour chacune. Réservation commune,
  contrôle des conflits, vagues de matchs sans double engagement, piste/arbitre indiqués sur chaque
  match à l'écran et sur la feuille imprimée. XML et réseau étendus ; relecture à vérifier.
  Tests `pool_resources` des crates `bp-core` et `marshaller` ajoutés et compilés, non exécutés.
  Pour une poule de trois tireurs, un seul match simultané reste possible.

- Marshaller : clic droit sur une piste → « Modifier le numéro… ». Vérifier sauvegarde/réouverture,
  refus d'un numéro déjà utilisé et retour du nouveau numéro dans BellePoule pour un match affecté.

- Correction du plantage BellePoule2D du 21/10 à 20:05 : la pile du dump Windows désigne
  la restauration du focus (`keep_focus_in_app`), pas la connexion réseau. Parcours des fenêtres
  corrigé ; journal persistant des futures paniques ajouté. À retester en ouvrant/fermant le
  dialogue et en lançant BellePoule + Marshaller sur le même PC (voir `REPRISE.md`).

*(à remplir au fil de l'eau : quoi lancer, résultat attendu, retour)*

À vérifier avec `lancer.bat` (détails dans `REPRISE.md`) :
1. Appel → composition : poules remplies, formules/séparation, déplacements.
2. Retardataires/absents : OK/Annuler, formule impossible, plusieurs nouveaux tireurs,
   sauvegarde/réouverture sans mélange d'identités.
3. Composition → poules : scores V/3 et W3/2, erreurs V/V, effacement, statuts,
   classements, retour précédent et relecture des scores.
4. Paramètres via « Poules » dans la formule ; impressions et PDF des compositions,
   listes, feuilles arbitres/résultats et classements.
5. Poules → tableaux : saisie directe/rapide, zoom, recherche, exemptions, modifications de scores,
   statuts, 3e place/toutes les places, sauvegarde/réouverture et impressions. Détails dans `REPRISE.md`.
6. Barrage : quota avec ex æquo, cases de promotion, compteurs et retour précédent.
7. Scission : sélection, nouvel onglet, équipes et membres, poursuite de la source et relecture.
8. Classement général : filtre, impressions et exports HTML/FFF/PDF/AskFred ; place FFF 1 puis 9.
Aucun test interactif lancé par l'agent.
