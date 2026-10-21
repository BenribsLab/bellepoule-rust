# Reprise du travail — point d'arrêt (21/10/2026)

Document écrit pour qu'une autre IA (ou une autre personne) reprenne le portage sans perdre le fil.
À lire avec `ARCHITECTURE.md` (plan général) et `AVANCEMENT.md` (état par chantier).

## Transmission prioritaire à la prochaine IA

Cette section décrit la direction actuelle et prime sur les anciens passages décrivant encore
deux programmes obligatoires, une copie visuelle exacte de GTK2 ou un démarrage sur Organisation.
**Ne pas recommencer le portage : poursuivre les modifications présentes dans le répertoire.**

### 0. Version web mise à niveau (21/10/2026)

- `tools/sync_web.py` recopie `bp-core`, la logique `marshaller` (sans `ui/`, crate compilé sans
  l'option `gui`) et le type `Message` de `bp-net` dans `../Bellepoule Web/bellepoule-web-api`.
  À relancer après chaque évolution du cœur ou du Marshaller, puis `docker compose up -d --build`.
- Le crate `marshaller` a une option `gui` (par défaut) : `ui/` et les dépendances GTK n'existent
  qu'avec elle ; `config_dir` n'utilise `glib` qu'avec elle.
- API web : `src/organisation.rs` (salle par club, synchronisation colis ↔ feuilles de route,
  `/api/organisation` GET/POST, import d'arbitres), barrage en matchs (`barrage_score`),
  `/stages/{i}/match_sheets` (feuilles de match du barrage et des tableaux). Fuseau `TZ=Europe/Paris`.
- Client web : page `#/organisation` (`src/organisation.rs` : frise, salle SVG, lots, fiche de
  match, plusieurs pistes par poule, arbitres par arme, impressions tickets/carnet), saisie des
  matchs de barrage, bouton « Feuilles de match » (barrage, tableaux), couleurs bleues du bureau.

### 1. Décisions utilisateur à conserver

- Un seul programme à lancer pour la compétition et son organisation : **`lancer.bat`**.
- La gestion des arbitres et celle des pistes sont deux fonctions indépendantes et facultatives :
  aucune, arbitres seuls, pistes seules, les deux.
- Ne jamais imposer Organisation au démarrage. Ouvrir l'accueil BellePoule, ou le fichier passé
  au lanceur. Organisation s'ouvre à la demande et possède **Fermer l'organisation**.
- Extensions autorisées : plusieurs pistes par poule et un arbitre attitré par piste. Les matchs
  sont répartis en vagues, sans assauts simultanés pour un même tireur ; piste et arbitre figurent
  sur les feuilles de poule. Les arbitres déjà occupés doivent être exclus des choix proposés.
- L'apparence peut désormais s'écarter de l'original. Conserver les noms et les grandes positions
  des menus, mais suivre la maquette moderne fournie par l'utilisateur : navigation latérale,
  bandeau illustré, cartes de présence, actions compactes, tableau lisible, blanc et bleu.
- **Dernière instruction : arrêter les vérifications, l'utilisateur les fait lui-même.** Ne pas
  relancer de compilation, tests ou application de sa propre initiative sans nouvelle demande.
  Un seul chantier à la fois, pas d'agents parallèles.

### 2. Unification BellePoule / Marshaller réalisée

Il s'agit d'une intégration dans la même fenêtre et le même processus. Le crate `bellepoule`
dépend de la bibliothèque `marshaller`. L'ancien exécutable Marshaller est conservé pour l'usage
séparé ; il n'est plus nécessaire pour l'usage intégré.

- `crates/bellepoule/src/management.rs` : options mémorisées, création de la vue embarquée,
  synchronisation des compétitions et des affectations en mémoire, file de commandes et fermeture.
- `crates/marshaller/src/ui/mod.rs` : `Ui::embedded`, `ManagementFeatures`, réutilisation des vues
  sans deuxième fenêtre principale ni second `Ring::join`.
- `crates/bellepoule/src/network.rs` : application locale des messages et partage de la clé existante.
  Les affectations d'un Marshaller externe sont ignorées lorsque la gestion intégrée est active.
- `crates/marshaller/src/resources.rs` et `assign.rs` : affectation d'arbitres sans piste physique,
  réservation de pistes sans arbitre obligatoire, disponibilités et relecture des réservations.
- Catalogue d'arbitres : `local-referees.xml` dans le dossier de configuration Marshaller.
  Géométrie de salle : `hall.json`. Affectations de compétition : fichiers `.cotcot`.
- Les options désactivées et la fermeture de la vue ne suppriment pas les ressources ni les
  affectations. La synchronisation continue lorsque seule la vue est fermée.
- Armes visibles : seulement celles des compétitions ouvertes ; les quatre armes restent
  disponibles si aucune compétition n'est ouverte, pour préparer le catalogue.
- Vue Organisation : arbitres à gauche, pistes au-dessus des matchs à droite, séparateurs réglables.
  Dialogues dans `ui/pool_resources.rs` et `ui/job_referees.rs`, disposition dans `ui/embedded_layout.rs`.

Le modèle multi-pistes et les impressions touchent aussi `bp-core/src/rounds/pool/pool.rs`,
`bp-core/src/net.rs` et les superviseurs/impressions BellePoule. Les attributs XML `Pistes` et
`ArbitresPistes` complètent le champ historique `Piste`. Voir `INTEGRATION.md` pour la conception.

### 3. Refonte graphique réalisée

- `crates/bellepoule/src/shell.rs` : nouvelle structure, accueil, barre de marque, navigation
  latérale Participants / Poules / Tableau / Classement / Organisation, aide et préférences.
  La barre latérale peut être masquée. L'accueil ne ferme pas les compétitions ouvertes.
- `app.rs` : montage de la structure, choix de la compétition active, retour à la compétition,
  ouverture/fermeture d'Organisation. Fenêtre initiale de 1200 × 800.
- `schedule_view.rs` : raccourcis vers les phases déjà accessibles. Ils changent l'affichage,
  sans faire avancer la formule ni déverrouiller une phase. Les onglets historiques restent présents.
- `visuals.rs` et `contest_page.rs` : bandeau illustré avec les vraies informations de l'épreuve
  (titre, arme, sexe, catégorie, lieu, date, phase). Couleur de compétition conservée en bordure.
- `crates/bp-gtk/src/appearance.rs` et `appearance.css` : thème partagé, activé avant le chargement
  Glade ; traitement des fenêtres et dialogues, boutons et icônes compacts, onglets, cartes et listes.
  `glade.rs` applique les règles de disposition au chargement des interfaces.
- Correction des expansions GTK2 qui faisaient occuper une grande partie de l'écran aux boutons
  et options. Les listes et canevas conservent l'espace extensible ; les actions peuvent revenir à la ligne.
- `stages/checkin.rs` : cartes de présence, recherche multi-mots, filtres Tous / Présents / Absents
  et compteur des lignes affichées. Les actions globales de présence conservent leurs gestionnaires.
- `widgets/players_list.rs` : filtrage d'affichage combiné texte/présence, conservation des parents
  d'équipe lorsqu'un membre correspond, tri existant conservé, fonds alternés, noms renforcés,
  libellés de présence en plus des cases. Ces filtres ne modifient pas les données de compétition.
- Traductions françaises ajoutées dans `resources/translations/fr.po`.

Deux images ont été générées avec l'outil intégré **imagegen** et copiées dans le projet :
`resources/visuals/fencing-header.png` (matériel d'escrime) et `fencers-sidebar.png` (tireurs sur
fond transparent). Les icônes latérales sont des SVG dessinés en code dans `resources/visuals/icons`.
Les prompts et la provenance sont dans `resources/visuals/README.md`. Ne pas dépendre des copies
originales stockées dans le profil Codex. Conception graphique dans `DESIGN.md`.

### 4. État réel, limites et suite

L'intégration puis la refonte ont passé plusieurs compilations ; les tests de recherche ont été
compilés, pas exécutés. Le dernier CSS a été accepté par le parseur GTK sans erreur ni avertissement,
sans ouverture de fenêtre. **Les dernières retouches d'optimisation du filtrage/comptage n'ont pas
été recompilées : l'utilisateur a demandé l'arrêt des vérifications.** Aucune validation interactive
de la nouvelle maquette n'a été effectuée par l'agent ; ne pas annoncer une fidélité visuelle validée.

Attendre les retours de l'utilisateur et corriger précisément les problèmes rapportés : disposition
en fenêtre normale/maximisée, retour accueil/compétition/Organisation, recherche et pointage,
imports d'arbitres, affectations multi-pistes et sauvegarde/réouverture `.cotcot`.
Les compteurs et informations viennent du modèle ; aucune pagination ou statistique fictive n'a été ajoutée.

Crash déjà identifié et corrigé : `RefCell already borrowed` dans `App::close_page` lors du retrait
d'une compétition. L'emprunt de la liste des pages est maintenant libéré avant les signaux GTK.
Journal de panique : `%LOCALAPPDATA%\BellePoule\logs\bellepoule-panic.log` ; dumps Windows :
`%LOCALAPPDATA%\CrashDumps\bellepoule.exe.*.dmp`.

Le répertoire contient des modifications accumulées et de nouveaux fichiers ; aucun commit n'a été
créé pendant ces derniers travaux. Ne pas supposer que les anciens passages « tout est fusionné »
décrivent ces changements. Ne pas écraser ou nettoyer les modifications existantes.

Si l'utilisateur demande ensuite une compilation, conserver `CARGO_TARGET_DIR=target\agent` pour
ne pas bloquer son exécutable. DLL GTK : `C:\Users\ben\gtk4\bin`. Le lint Clippy `min_max` dans
`bp-core/src/rounds/youth.rs` est un problème préexistant, distinct de cette refonte.

## But
Réécrire **BellePoule 5.7** (gestionnaire de compétitions d'escrime, C++/GTK2, sources dans
`reference/v5.7`) en **Rust + GTK4 (gtk4-rs)**, comme un clone **parfait et autonome** (aucun lien avec
la version C++ à l'exécution ; seuls les formats d'échange sont conservés : .cotcot, FFF, FIE XML, CSV,
AskFred). Programmes : BellePoule (superviseur), Marshaller, LivePoule (+ backend USB).
Facebook, Twitter et OAuth sont exclus.

## Consignes de l'utilisateur
- Décider seul, ne pas demander de validation ; préserver les fonctions et formats du C++, avec
  les extensions et la modernisation graphique explicitement autorisées ci-dessus.
- **Un chantier à la fois, pas d'agents en parallèle** (quota).
- **Ne pas lancer de tests interactifs soi-même** : modifier le code, compiler (`cargo check`), puis dire à
  l'utilisateur quoi tester ; il lance l'application et rapporte.
- Capture d'écran : uniquement la fenêtre BellePoule (jamais l'écran entier).

## Organisation du code
- `crates/bp-core` : toute la logique (compétition, poules, tableaux, formats…). Terminée à ~90 %, ~60 tests.
- `crates/bp-gtk` : chargeur Glade, dialogues, canvas, icônes.
- `crates/bellepoule` : interface du superviseur (`app.rs`, `contest_page.rs`, `schedule_view.rs`,
  `stages/`, `widgets/`).
- `crates/marshaller`, `crates/livepoule`, `crates/bp-usb-backend`, `crates/bp-net`.
- Lancer : `lancer.bat [fichier.cotcot]` (PATH GTK4 = `C:\Users\ben\gtk4\bin`).
- Compiler sans gêner l'instance de l'utilisateur : `CARGO_TARGET_DIR=target\agent`.

## Ce qui marche (vérifié par l'utilisateur ou par capture)
Fenêtre principale, menus, onglets, nouvelle compétition, propriétés, ouverture/enregistrement, écran
« Appel » complet (ajout de tireurs, formulaire, présence, « tous », import, filtre).

## Point d'arrêt exact
**Dernière refonte : maquette utilisateur avec navigation latérale**. `shell.rs` construit l'accueil,
la barre de marque et la navigation Participants / Poules / Tableau / Classement / Organisation.
Les menus historiques restent en haut. Les raccourcis de phases ne montrent que les écrans déjà
accessibles ; ils n'avancent ni ne déverrouillent la compétition. Accueil ne ferme pas les compétitions ;
« Revenir à la compétition » les retrouve. Organisation garde son bouton de fermeture explicite.
`visuals.rs` crée un bandeau dont le titre, l'arme, la catégorie, le lieu, la date et la phase viennent
du modèle. Deux PNG générés par imagegen sont intégrés dans `resources/visuals` (prompts dans son README).
Appel : cartes de présence, recherche multi-mots et filtres Tous / Présents / Absents, compteur de lignes,
alternance des fonds, noms renforcés et libellés de présence. Les équipes gardent leur parent si un membre
correspond à la recherche. Tests de recherche écrits et compilés, non exécutés.
**Instruction utilisateur finale : arrêter les vérifications, il les fait lui-même.** Aucun lancement
interactif effectué. Dernier contrôle CSS accepté par GTK ; les dernières retouches de filtrage et de
comptage ont été faites après le dernier `cargo check` et ne sont pas revérifiées à sa demande.

**Nouvelle direction graphique autorisée** : moderniser l'application en conservant les noms et
les grandes positions des menus. Thème partagé `bp-gtk::appearance` / `appearance.css`, installé par
BellePoule avant Glade : surfaces claires, accent bleu, boutons compacts avec icônes de 18 px,
onglets et en-têtes de listes harmonisés, focus clavier visible. Les fenêtres/dialogues créés par
code reçoivent aussi le thème. `Glade::from_string` applique les règles seulement si le thème est activé.
Appel : résumé de présence et actions compactes avec retour à la ligne ; liste extensible.
Composition/configurations : expansion verticale des commandes retirée ; listes et canevas conservés.
Couleur de compétition limitée à une bordure sur l'onglet. Tableaux de joueurs : marges de cellules
harmonisées. Voir `DESIGN.md`. CSS validé sans erreur par GTK, sans fenêtre ; `cargo check -p bellepoule`
et `cargo build -p bellepoule` réussis, essais visuels à faire par l'utilisateur.

**Navigation et disposition Organisation revues** : démarrage sur l'accueil BellePoule habituel
(ou la compétition passée en argument), bouton Organisation à côté du menu, fermeture explicite par
« Fermer l'organisation » avec retour à la compétition précédente ou à l'accueil. Les choix d'activation
restent mémorisés sans imposer l'écran Organisation au démarrage. Vue intégrée compacte : arbitres
à gauche, pistes au-dessus des matchs à droite, deux séparateurs réglables ; commandes d'arbitres
sur deux petites lignes, barre de pistes avec retour à la ligne et liste d'arbitres défilable.
`cargo check -p bellepoule` et `cargo build -p bellepoule` réussis ; validation interactive à faire
par l'utilisateur : démarrage sans fichier, ouverture/fermeture Organisation avec/sans compétition,
fenêtre de 1000 × 700 et redimensionnement des panneaux.

**Organisation : armes pertinentes uniquement** : les onglets d'arbitres suivent désormais les
armes des compétitions ouvertes (fleuret seul → Fleuret uniquement). Sans compétition, le catalogue
complet reste accessible. Aucun arbitre supprimé ; indices des onglets conservés pour les actions
et imports. Compilation vérifiée ; affichage à valider par l'utilisateur.

**Plantage du 21/10 à 21:21 corrigé** : journal `%LOCALAPPDATA%\BellePoule\logs\bellepoule-panic.log`,
`App::close_page` (`app.rs:340`), `RefCell already borrowed`. Le `if let` conservait l'emprunt
en lecture de `pages` pendant sa modification. Recherche et retrait sont maintenant effectués dans
un seul emprunt mutable, libéré avant les signaux GTK de retrait de l'onglet. Compilation vérifiée ;
à revalider par l'utilisateur : Organisation → compétition/poules, puis fermeture d'une compétition.

**Nouvelle direction utilisateur (21/10) : un seul programme, avec organisation facultative intégrée.**
`lancer.bat` ouvre BellePoule ; le bouton **Organisation** donne accès à « Gérer les arbitres » et « Gérer les
pistes » sont deux options indépendantes, mémorisées. Aucun second Marshaller à lancer pour cet usage.
La conception et les quatre modes sont documentés dans `INTEGRATION.md`. Code compilé, interface non essayée.

Tous les types de phases ont maintenant leur écran : appel, composition/supervision des poules,
tableaux, barrage, scission et classement général. Les derniers ajouts compilent.
**Aucune validation interactive de ces nouveaux écrans n'a encore été faite.**
Menu des langues, impression de l'appel et impression de la formule (bouton « Print » du calendrier
des phases) sont faits (compilés, non vérifiés). Les crochets de débogage ont été retirés.
Le prochain chantier est la liaison réseau du superviseur (menu Publication, `ScoreCollector`,
aiguillage des messages) puis le Marshaller. NB : en v5.7 la liste des arbitres n'a pas d'écran dans
le superviseur (elle est alimentée par le réseau) ; l'interface des arbitres est celle du Marshaller. Ne pas confondre compilation et fidélité vérifiée.

### Composition des poules
- `stages/allocator.rs` : branché dans `stages/mod.rs`, rafraîchissement via `Weak<Self>` ; formules,
  séparation, poules en deux colonnes, déplacements, indicateurs, liste des tireurs et filtres.
- `schedule_view.rs` : l'allocateur a son propre onglet, comme `Schedule::PlugStage` en C++.
  Le filtre sur le nom XML vide le rendait inaccessible. Onglets retirés/réordonnés lors des
  changements de formule ; écrans invalidés si leur type change.
- `stages/pillow.rs` : retardataires, absents, ajout par formulaire, vérification de la formule,
  travail sur une copie jusqu'à OK (Annuler ne modifie pas le modèle).
- Le pointage inscrit tous les nouveaux joueurs avant de reverrouiller et renuméroter ; la table
  de renumérotation est propagée aux phases, puis la distribution est recalculée.
- `stages/allocator/print.rs` : impression de la composition ou de la liste via cairo et GTK.
- Les arbitres sont lus dans le magasin distinct de la compétition.

### Supervision des poules
- `stages/pool_supervisor.rs` : sélection de poule, matrice de scores et liste des matchs dans
  l'ordre du répartiteur, statuts Q/A/E/F, résultats, classements courant et cumulé, filtres.
- Saisie immédiate dans le modèle ; reconstruction différée de l'affichage avec restauration du
  focus/curseur et avance vers le prochain score. V, W3/V3, chiffres, effacement, incohérences
  signalées et verrouillage respecté.
- Paramètres (nom, score maximal, quota, répartition) accessibles en sélectionnant « Poules »
  dans la formule. `StageScreen::configuration()` est additive ; `ScheduleView` conserve un écran
  futur pour afficher ses paramètres avant le début de la phase.
- `stages/pool_supervisor/print.rs` : feuilles arbitres (signatures, ordre des matchs, cases de
  touches), résultats avec la densité du modèle, classements, une/toutes les poules, numéros de
  matchs en filigrane. Dialogue natif d'impression, donc PDF selon le pilote.
- `bp-gtk/src/print.rs::run_tiled` : pagination cairo commune aux deux écrans.
- `bout.rs` : une saisie Unicode est rejetée sans panique ni modification du score.

### Superviseur de tableaux (dernière reprise)
- `stages/table_supervisor.rs` : branché sur `TableSupervisor`, arbre cairo, suites principale et de
  classement, navigation entre colonnes, zoom, recherche de match et saisie rapide via `table.ui`.
- Saisie V, W3/V3, scores numériques et effacement ; A/E/F au clavier ou par clic droit sur un score,
  restauration par le menu contextuel. Modèle mis à jour immédiatement, dessin différé avec
  restauration du focus/curseur et passage à la case suivante. Les matchs incohérents sont signalés.
- Paramètres de formule : nom, score maximal, quota, aucune / 3e place / toutes les places ;
  vue de classement, filtres et contrôle du verrouillage.
- `stages/table_supervisor/scene.rs` : dessin commun à l'écran et au papier, construit sur les
  coordonnées et connecteurs de `TableSet::drawing`. Noms et organismes stylés comme la référence ;
  noms/prénoms seuls dans les colonnes compactes.
- `stages/table_supervisor/print.rs` : aperçu, 4/8/16 matchs par feuille, pagination via `PrintSession`,
  en-têtes répétés, classement, feuilles de match individuelles ou par tableau, toutes / en attente,
  groupement par arbitre et adaptation portrait/paysage. Marquage des feuilles seulement après
  confirmation de l'impression native. Les références de match sont imprimées en texte ; QR et
  circuit smartphone restent à intégrer avec le réseau.
- `TableSet::drawing_with_referees` et `score_sheet_with_referees` sont des API additives : elles
  distinguent le magasin des arbitres de celui des tireurs, même si les références coïncident.
- `table.ui` / `table_supervisor.ui` : les indices de nœuds/suites utilisent des colonnes `guint`
  à la place des pointeurs C++. Les deux fichiers valident avec `gtk4-builder-tool validate`.
- `CanvasView` retire les anciens widgets de score, regroupe les rafraîchissements et écoute aussi
  les clics sur le conteneur superposé. Ses callbacks ne retiennent plus leur propre état en cycle.
  `Canvas::reset()` réutilise le stockage entre les saisies ; il invalide les anciens identifiants,
  conserve vues/abonnements et évite la croissance de l'arène à chaque reconstruction.
- La police des entrées de score suit le zoom pour éviter le recouvrement des lignes.
- Pas de validation visuelle encore : comparer surtout les grands tableaux, les noms longs,
  les champs au zoom 0,1–1, les corrections d'un résultat et les suites de classement. Les aspects
  graphiques des filtres et l'exacte mise en page du C++ restent à contrôler. Ne pas annoncer ces
  fonctions comme vérifiées en compétition.

### Barrage, scission et classement général (dernière reprise)
- `stages/barrage.rs` : liste des ex æquo avec cases « promu », compteurs qualifiés/éliminés,
  classement complet, filtres et impression. Seuls les participants du barrage sont modifiables ;
  le verrouillage interdit la saisie et les compteurs réactualisent la navigation.
- `stages/splitting.rs` : cases « exporté », filtre, impression et nom de la compétition extraite
  dans les paramètres de formule (champ libre avec propositions du C++).
- Au franchissement d'une scission, `Tournament::split_current_contest` / `Contest::duplicate_split`
  créent une compétition sans poules, ouverte dans un nouvel onglet. Seuls les participants cochés
  sont copiés, avec leur position d'entrée (sans compacter les trous), attribut exporté remis à faux,
  nouveaux identifiants, attributs locaux retirés. Les équipes reconstruisent leurs liens vers les
  membres copiés et recalculent leur présence. La source poursuit avec les participants restants.
  Revenir puis franchir de nouveau une scission crée une nouvelle compétition, comme le C++.
- `stages/general_classification.rs` : liste finale, filtre, impression et exports HTML, FFF, PDF,
  AskFred. Le sélecteur mémorise le dossier ; FFF expose la place du vainqueur (décalage = place − 1).
  Les exports et rafraîchissements de vue lisent le classement calculé, sans le modifier.
- `widgets/list_document.rs` : cellules communes aux trois écrans pour impression/PDF, pagination
  de 25 lignes, en-têtes répétés et séparateurs de classement sur papier. Le PDF utilise cairo.
- `Contest::export_askfred` sérialise la formule ; les nouvelles variantes additives des écrivains
  FIE/AskFred lisent les arbitres dans leur magasin distinct, même en cas de références identiques.
- `Schedule::refresh_general_classification` restaure les rangs des phases précédentes avant de
  décaler les rangs pour exclus/exportés, afin de ne pas décaler plusieurs fois les anciens éliminés.
- Validation structurelle des trois `.ui` réussie. Fidélité visuelle et mise en page exacte du C++
  encore à contrôler : colonnes graphiques, présentation du classement et impressions/PDF.

### Vérifications réalisées
Avec `CARGO_TARGET_DIR=target/agent` :
- `cargo check --workspace --tests` : succès (tests compilés, pas exécutés).
- `cargo build --workspace` : succès.
- `cargo clippy --workspace --tests` : succès avec avertissements préexistants.
Application non lancée ; aucun test interactif ni test automatisé exécuté. Validation structurelle des UI de tableaux, barrage, scission et classement général avec gtk4-builder-tool.
Régressions ajoutées et compilées : saisie Unicode, plusieurs retardataires avant renumérotation,
`tests/table_view.rs` (collision d'identifiants arbitre/tireur, pagination 4/8/16, feuilles en attente)
et reconstruction répétée du canevas sans croissance de son stockage.
Ajouts de cette reprise : `tests/contest_splitting.rs` (individuels/équipes, positions, autonomie,
relecture) et `tests/contest_exports.rs` (collision arbitre/tireur AskFred, décalage FFF sans mutation,
stabilité du classement après recalcul avec export d'un éliminé). Compilés, non exécutés.

### Retours utilisateur traités (21/10/2026)
- Saisie des scores : passage automatique à la case suivante. Cause : `GtkEntry::has_focus()` est
  toujours faux en GTK4 (le focus est sur le `GtkText` interne), l'écran perdait donc le focus à chaque
  frappe (ce qui empêchait aussi « W3 »). Corrigé par `widgets::entry_has_focus` et
  `widgets::focus_entry_when_ready` (poules, tableaux, appel).
- Aperçu avant impression partout : `bp_gtk::print::run_document` (trait `PageSource`, `Tiled`,
  `Concat`) affiche un aperçu paginé puis lance l'impression native.
- Tableaux : le bouton Imprimer propose le tableau et/ou les feuilles de match (toutes / en attente).
- Classement général : impression du classement final seul ou des résultats complets (feuilles et
  classements de chaque tour de poules, tableau principal et classement de chaque phase de tableaux,
  puis classement final).
- Feuille de poule à l'écran : cases contournées, cases de la diagonale grisées (CSS `bp-cell`).
- Chaînes françaises ajoutées en fin de `resources/translations/fr.po` pour ces nouveaux libellés.

### Impression fidèle et entraînement des jeunes (21/10/2026)
- Bandeau et pied de page de toutes les impressions comme `Contest::DrawPage` (couleur, nom, arme -
  sexe - catégorie, nom de page, organisateur, date). Mise en page réglable depuis l'aperçu
  (« Mise en page… ») et mémorisée dans les préférences (`[PrintLayout]`) : titre, couleurs,
  tailles de police, cases grisées, noms, pied de page, numéros de page.
- Feuilles de poule imprimées comme `Pool::Draw` (arbitre, piste, grille, signatures, liste des matchs
  avec cases de touches, résultats en en-têtes inclinés). Rendu contrôlé par le test
  `render_pool_sheet_png` (variable `BP_SHEET_PNG` = dossier de sortie).
- Extension « Entraînement des jeunes » (absente du C++), **dans une seule compétition** :
  formule Appel → Entraînement des jeunes (création des groupes) → Poules → Tableaux → Classement.
  `rounds/youth.rs` (`StageKind::YouthTraining`, XML `EntrainementJeunes`) : réglages dans les
  préférences de la phase (groupes par division / au classement / au hasard avec décalage, poules en
  serpentin ou par niveau, places tirées, **points** : identiques, par victoire ou par place). Écran
  central : groupes modifiables (nombre, taille, formule de poules ★, glisser-déposer entre groupes).
  « Tour suivant » fige les groupes : attribut global `youth_group` (XML `Groupe`) sur chaque tireur.
  `Schedule::apply_youth_training` configure la phase suivante : `PoolRound::youth_groups` (poules
  de chaque groupe, « Groupe 2 · Poule n° 01 », `create_youth_pools`), `cross_output` par groupe,
  places tirées du tableau. Tableaux : une suite principale par groupe (`TableSet::group`,
  `base_place`, IDs `G1`, `G2`…). « Tour précédent » défait tout (l'attribut est retiré).
  Classement général : `youth::results` → classement de chaque groupe (place dans le groupe,
  victoires, points). Tests : `tests/youth_training.rs`, `rounds::youth::tests`.

### Marshaller et liaison réseau (21/10/2026)
- Validé par l'utilisateur : zoom des tableaux, barrage en vrais matchs (feuille de match comme
  celle des tableaux, cases de touches jusqu'à 60 sur plusieurs rangées, score saisi → promu),
  bouton « Enregistrer »/« Enregistré » + Ctrl+S, fermeture sans message si tout est enregistré,
  retour au premier plan après fermeture d'une fenêtre (`dialogs::keep_focus_in_app`).
- Barrage (extension) : `rounds/barrage.rs` crée les matchs entre ex aequo (tous contre tous),
  en autant de touches que la phase précédente (`Stage::bout_max_score`, `PreviousStage.max_score`),
  places = quota précédent − qualifiés d'office ; matchs enregistrés dans `<Barrage>` (`<Match>`).
- BellePoule → Marshaller : `bp-core/src/net.rs` (`Contest::parcels`) décrit les colis
  `BellePoule::Competition`, `::Referee`, `::Fencer` (après l'appel), `::Batch` (tour de poules)
  et `::Job` (poules). `bellepoule/src/network.rs` rejoint l'anneau (rôle utilisateur), diffuse
  chaque seconde ce qui a changé, rappelle ce qui a disparu, et applique `BellePoule2D::Roadmap`
  (`Contest::apply_roadmap` : piste, heure, arbitres de la poule), `::Referee`
  (`manage_referee_xml`), `::EndOfBurst`. Messages mis en attente si le tournoi est occupé.
- Marshaller : `assign.rs` (messages reçus, dépôt match/arbitre, placement automatique d'un lot
  avec affinités et maintien de piste, annulation, prolongation, déplacement d'un match),
  interface `ui/` (fenêtre, salle dessinée en cairo, frise, onglets compétitions/lots, tableau
  des matchs, listes d'arbitres par arme avec appel, import FFF/CSV, formulaire, tickets repas
  et carnet de paiement). `lancer-marshaller.bat`. Test d'échange complet : `marshaller/tests/link.rs`.
- Suite faite : tableaux (un lot par tableau prêt, un match par rencontre, feuille de route
  `apply_table_roadmap`), `ClockOffset` (`bout::set_clock_offset`, durées), SmartPoule
  (`ScoreSheetCall` → `BellePoule::ScoreSheet` chiffré par la clé de l'arbitre, `Score` → scores
  de poule ou de tableau ; clés des arbitres = mot de passe chiffré par la clé de l'anneau, QR code
  dans la fiche arbitre du Marshaller, réglage WIFI et code administrateur), menus « Publication »
  (FTP / Escrime Info à chaque enregistrement, `publication.rs`) et « BellePoule2D > Code d'accès ».
  Tests : `marshaller/tests/link.rs` (poules, tableaux, clé d'arbitre).
- Reste : diffusion vidéo, impression de la salle à comparer, report dans la copie web de
  `bp-core` (stage.rs, schedule.rs, barrage.rs, net.rs, bout.rs…).
- Piège : les tests du crate `marshaller` lient GTK ; lancer `cargo test` avec
  `C:\Users\ben\gtk4\bin` dans le PATH, sinon Windows bloque sur une boîte « DLL introuvable ».
- Piège : ne jamais appeler `Stage::name()` d'un tour de poules quand son `round()` est déjà
  verrouillé (Mutex non réentrant → blocage).

### SmartPoule (22/10/2026, codé, compile, non vérifié)
Application Android native (`smartpoule-android/`, Kotlin, voir son README) ; **le serveur
BellePoule Web sert de relais** : téléphones et PC n'ont plus besoin d'être sur le même réseau.
- `bp-core/src/smartpoule.rs` : `smartpoule_jobs(arbitre)`, `smartpoule_sheet(jeton, arbitre)`,
  `smartpoule_apply_score(jeton, match, [valeurs])` ; jetons `p~<compét.>~<tour>~<poule>` et
  `t~<compét.>~<tableau>~<suite>~<chemin>` ; valeurs « 3 », « W5 » (vainqueur), « V ».
- `bellepoule/src/smartpoule.rs` : toutes les 3 s, `PUT {serveur}/api/smartpoule/state` (clé API
  du club), relève `/pending`, applique, `POST /results`. Réglages groupe « SmartPoule » de la
  config ; menu BellePoule2D › SmartPoule… (serveur, clé API, QR codes, impression 12 par page).
  Clés d'arbitres tirées automatiquement (attribut `password`) ; arbitre 0 = administrateur.
- Web : `bellepoule-web-api/src/smartpoule.rs` (relais en mémoire + routes `/phone/*`) ;
  les compétitions gérées sur le web sont servies directement. Déployé le 22/10/2026.
- APK : `smartpoule-android/app/build/outputs/apk/debug/app-debug.apk`.
- Reste : QR codes SmartPoule côté client web ; scores en direct vers LivePoule.

## Suite (dans l'ordre) — liste complète de ce qui reste (21/10/2026)
1. **Validation par l'utilisateur** de tout ce qui est « écrit, compile, non vérifié » :
   - Marshaller relié à BellePoule (poules, tableaux, arbitres, placement auto, frise, fiche match,
     tickets repas / carnet de paiement, WIFI) ; impression de la salle à comparer au C++ ;
   - Publication FTP / Escrime Info, BellePoule2D > Code d'accès ;
   - écrans des phases non encore passés en revue : scission, classement général « normal »,
     écrans d'équipes (compétitions par équipes de bout en bout), AskFred, retardataires/absents ;
   - fidélité visuelle des feuilles et tableaux au canevas C++ (comparaison écran et papier).
2. **SmartPoule** : à valider sur téléphone (voir section SmartPoule ci-dessus).
3. **LivePoule** : scores en direct depuis SmartPoule (`SmartPoule::Score`, `Timer`) ; version
   Raspberry Pi (feature `raspberry`, jamais compilée ni essayée sur le matériel).
4. **Réseau (`bp-net`, ~85 %)** : diffusion vidéo (menu désactivé), site web de consultation et
   console à vérifier, indicateur réseau (partenaires) dans BellePoule et le Marshaller,
   appairage par clé USB à essayer.
5. **Imports / exports (~75 %)** : contrôler sur des fichiers réels les exports FFF, XML FIE,
   CSV, AskFred, HTML et les imports correspondants (comparaison avec le C++).
6. **Traductions** : chaînes ajoutées par le portage (Marshaller, barrage, jeunes, publication…)
   à traduire dans les autres langues que le français ; vérifier l'anglais.
7. **Installeur Windows** (et éventuellement paquets Linux) : exécutables, ressources, DLL GTK,
   règles pare-feu.
8. **Version web** (`../Bellepoule Web`) : reporter les nouveautés de `bp-core` (barrage en matchs,
   `stage.rs`, `schedule.rs`, `net.rs`, `bout.rs`…) et des écrans (enregistrement, barrage).
9. Tenir `AVANCEMENT.md` à jour (global encore estimé ~55–60 %) ; ne pas annoncer un clone
   terminé sans validation des fonctions restantes et comparaison à la référence C++.

## Scénarios à tester par l'utilisateur
- `lancer.bat` : nouvelle compétition, au moins 12 présents ; « Tour suivant » doit ouvrir
  « Composition des poules » avec des poules remplies. Modifier formule, séparation et colonnes ;
  déplacer un tireur et vérifier l'équilibre et la navigation.
- Prévoir un absent initial, le déclarer retardataire dans la composition. Déclarer un présent
  absent. Tester OK, Annuler et une modification impossible avec la formule. Ajouter plusieurs
  nouveaux retardataires ; sauvegarder/réouvrir sans mélange d'identités.
- « Tour suivant » : feuille de poule ; saisir V/3, W3/2, effacer/corriger, changer de poule,
  consulter les deux classements. V/V doit empêcher le passage à la suite. Tester abandon,
  exclusion, restauration, retour à la composition et réouverture des scores.
- Sélectionner « Poules » dans la formule : nom, maximum, quota. Répartition modifiable avant
  verrouillage de la composition, puis désactivée.
- Imprimer composition/liste, feuilles arbitres/résultats, une/toutes les poules, classements.
  Vérifier PDF, pagination, accents et absence de coupure d'une poule.
- Régressions ciblées à lancer : `cargo test -p bp-core --test contest_checkin` et
  `cargo test -p bp-core unicode_score_input_does_not_panic`.

### Scénarios tableaux
- Finir les poules puis « Tour suivant » : arbre avec exemptions, champs des matchs prêts,
  tableau précédent/suivant, zoom et recherche par numéro dans l'espace de noms affiché.
- Saisir V/3 et W3/2, effacer, corriger, passer au prochain score ; refaire en mode « Saisie »
  avec le menu de recherche rapide. V/V doit signaler une erreur et empêcher la suite.
- Clic droit sur un score : abandon / exclusion / forfait / restauration ; vérifier les adversaires
  suivants et le classement. Vérifier le blocage des modifications après « Tour suivant ».
- Dans la formule, sélectionner la phase « Tableau » et essayer les trois modes de places tirées.
  Terminer les demi-finales : la suite de 3e place doit apparaître si elle est demandée ; toutes
  les places doit exposer les suites correspondantes au fur et à mesure.
- Sauvegarder à mi-parcours, réouvrir et comparer scores, identités, statuts et classement.
- Imprimer arbre, classement, feuilles individuelles (icône près du match) et feuilles d'un tableau
  (icône d'en-tête). Tester annulation, toutes/en attente, 4/8/16, portrait/paysage et PDF.
- Commandes pour les régressions à exécuter par l'utilisateur : `cargo test -p bp-core --test table_view`
  et `cargo test -p bp-gtk repeated_scene_reconstruction_keeps_storage_bounded` (PATH GTK4 pour ce dernier).

### Scénarios barrage, scission et classement général
- Fixer un quota de poules coupant des ex æquo ; accepter le barrage. Seuls les ex æquo doivent
  figurer dans la liste de saisie. Cocher les promus, vérifier les deux compteurs et le classement,
  imprimer, puis passer à la suite. Revenir doit retirer le barrage comme dans le C++.
- Insérer une scission dans la formule, choisir un nom, cocher quelques tireurs et avancer :
  un nouvel onglet doit contenir uniquement les cochés, sans poules et avec les positions initiales
  comme classement d'entrée. La source continue sans eux. Tester aussi aucune sélection.
- Refaire avec des équipes : membres présents, liens vers la bonne équipe, aucun doublon, pas de
  participant importé dans la mauvaise compétition. Enregistrer/réouvrir les deux compétitions.
- Finir les tableaux : contrôler le classement final, les abandons/exclusions et les exportés.
  Modifier les colonnes, imprimer et exporter HTML/FFF/PDF/FRD. FFF : place du vainqueur 1 puis 9 ;
  le classement affiché ne doit pas changer. Contrôler accents, pays, pagination et noms longs.
- Régressions à lancer par l'utilisateur : `cargo test -p bp-core --test contest_splitting`
  et `cargo test -p bp-core --test contest_exports`.

## Pièges connus
- Organisation intégrée : `bellepoule/management.rs` synchronise en mémoire les compétitions ouvertes
  avec `marshaller::ui::Ui::embedded`. Pas de nouveau processus, fenêtre principale ou anneau réseau.
  Les menus/fermeture du Marshaller autonome ne sont pas branchés sur la fenêtre hôte. Les vues
  arbitres, salle et matchs partagent un seul modèle local. Un Marshaller externe ne pilote pas les
  affectations lorsque la gestion locale est active (éviter deux gestionnaires concurrents).
- Mode arbitres seuls : importer/ajouter dans Organisation, double-cliquer une poule/un match dans
  la liste puis « Affectations… ». `set_job_referees` permet une réservation sans piste ; sa relecture
  ne crée pas de piste physique. Le bouton d'affectation du lot ouvre aussi ce choix en mode arbitres.
  Pour une ancienne poule multi-pistes, les pistes existantes sont affichées en lecture seule afin
  de conserver les associations individuelles des arbitres.
- Mode pistes seules : plan et glisser-déposer disponibles ; l'affectation ne demande pas d'arbitre,
  y compris pour plusieurs pistes. Les préférences masquent les fonctions désactivées et ne suppriment
  pas les données existantes. Les arbitres ont un catalogue partagé persistant `local-referees.xml`
  dans le dossier de configuration du Marshaller ; la salle conserve `hall.json`.
- Les ordres locaux sont appliqués avant changement de mode et avant sauvegarde/fermeture ; la
  suppression d'un arbitre est désormais propagée au magasin de chaque compétition. Les modifications
  d'arbitres d'un créneau sont rediffusées même lorsque le statut du lot ne change pas.
- Vérifications intégration : `cargo check --workspace --tests`, `cargo build --workspace` ; Clippy
  passe avec l'exception préexistante `-A clippy::min_max`. Régressions
  `marshaller/tests/integrated_management.rs` ajoutées/compilées, non exécutées : arbitres sans pistes,
  pistes sans arbitres, réouverture et catalogue sans compétition ouverte. Aucun essai interactif.
  À vérifier par l'utilisateur : les quatre modes, passage de l'un à l'autre, catalogue/import,
  fermeture de fenêtres secondaires sans quitter BellePoule, enregistrement et réouverture.
- Choix des arbitres dans « Pistes et arbitres de la poule » : `pool_referee_available` applique
  le même contrôle au filtre du dialogue et à la validation : arme, présence, chevauchement sur
  tout le créneau de la poule (pas l'heure du curseur). La réservation de la poule elle-même est
  ignorée, pour conserver ses arbitres actuels. Un arbitre choisi pour une piste disparaît des
  menus des autres pistes cochées ; décocher la piste le rend disponible dans ces menus.
  Les listes se réactualisent à chaque choix et après un refus de validation. Régression ajoutée
  dans `marshaller/tests/pool_resources.rs`, compilée non exécutée ; interface à contrôler.
- Correction du glisser-déposer multi-pistes : la source `batch_draggable_jobs` conserve les
  poules déjà planifiées (l'ancien filtre ne conservait que les matchs en attente), et
  `job_drop_allowed` accepte une nouvelle piste libre à l'horaire de la poule. Le dépôt ouvre
  le dialogue après la fin du geste GTK, avec la piste cible précochée ; choisir son arbitre
  puis valider. Annuler conserve la réservation initiale. Un dépôt direct dans le modèle ne
  doit jamais créer un second créneau orphelin pour un match déjà affecté.
  Régression ajoutée dans `marshaller/tests/pool_resources.rs` : départ d'une poule planifiée,
  piste déjà affectée/bloquée/occupée, curseur à une autre heure, conservation du créneau initial.
  `cargo check -p marshaller --tests` et `cargo build -p marshaller` ; aucun essai interactif.
- Extension poules multi-pistes (21/10, demande utilisateur, hors référence C++) : dans Marshaller,
  placer la poule puis clic droit sur sa piste → « Pistes et arbitres de la poule… ». Cocher les
  pistes et choisir un arbitre différent en face de chacune. À une seule piste, la liste commune
  permet toujours plusieurs arbitres. Les ressources sont réservées au même créneau ; durée
  conservée, sans estimation automatique de réduction. Les conflits sont refusés avant modification.
  Pour changer l'arbitre d'une poule multi-pistes, utiliser ce dialogue (glisser un arbitre dessus
  est désactivé pour conserver les associations explicites).
- `marshaller/resources.rs` et `Slot::additional_pistes` : même créneau référencé par toutes les
  pistes, libération/retrait/déplacement communs, renumérotation rediffusée sans changer la piste
  principale. Les échanges et la relecture portent la liste `pistes` et les arbitres dans le même
  ordre. L'affichage de la salle montre l'arbitre propre à chaque piste.
- `Pool::match_assignments` calcule des vagues reproductibles : aucun tireur engagé deux fois dans
  une vague, charge équilibrée entre pistes, identifiants de matchs conservés. À trois tireurs,
  un seul match par vague est possible. La saisie d'un score ne recalcule pas un autre ordre.
  Feuille à l'écran et imprimée : vague, piste, arbitre nommément indiqué pour chaque match ;
  finir une vague avant de commencer la suivante. La grille conserve ses vrais indices de match.
- Sauvegarde `.cotcot` : `Piste` conservé pour compatibilité, `Pistes` et `ArbitresPistes` ajoutés
  aux poules. Rétablissement dans Marshaller lors de la réception d'une compétition enregistrée.
  Relecture des arbitres de poule corrigée : ne plus les chercher dans le magasin des tireurs.
- Vérifications de cette extension : `cargo check --workspace --tests` et compilation de BellePoule
  et Marshaller ; tests `bp-core/tests/pool_resources.rs` et `marshaller/tests/pool_resources.rs`
  ajoutés/compilés, non exécutés. Aucun lancement interactif. Clippy normal bloqué par un défaut
  préexistant dans `rounds/youth.rs:215` (`rounds.min(1).max(1)`, lint `clippy::min_max`).
  Clippy passe avec avertissements en autorisant uniquement ce lint préexistant
  (`cargo clippy -p bellepoule -p marshaller --tests -- -A clippy::min_max`).
  Scénario utilisateur : une poule de 6, deux pistes/arbitres, vérifier les vagues sur papier,
  saisir un score hors de l'ordre initial, retirer une piste, renuméroter, enregistrer/réouvrir.
- Renumérotation des pistes : clic droit sur une piste dans la salle → « Modifier le numéro… ».
  `Hall::renumber_piste` refuse zéro et les doublons, conserve la piste et ses affectations,
  rediffuse les feuilles de route des matchs concernés. La sauvegarde `hall.json` est automatique.
  Régressions ajoutées dans `hall.rs` et `tests/link.rs` ; compilées, non exécutées.
  Interface à vérifier par l'utilisateur après relancement de `lancer-marshaller.bat`.
- Plantage du 21/10 à 20:05 en ouvrant BellePoule2D : rapport Windows Application Error
  1000, code `0xc0000409`, dump `%LOCALAPPDATA%/CrashDumps/bellepoule.exe.38704.dmp`.
  Analyse du dump avec les symboles locaux : panique Rust dans le callback différé de
  `dialogs::keep_focus_in_app`, avec `gio::ListModelExtManual::iter<gtk::Window>` dans la pile.
  Corrigé : parcours par indices puis downcast des éléments, sans imposer `Window` au type
  déclaré du modèle de fenêtres GTK. Pas de relancement interactif par l'agent.
  BellePoule conserve désormais les futures paniques et piles d'appels dans
  `%LOCALAPPDATA%/BellePoule/logs/bellepoule-panic.log` (créé à la première panique).
  Pour vérifier la liaison sur le même PC : lancer `lancer.bat` puis `lancer-marshaller.bat` ;
  découverte automatique, pas de clic Connecter nécessaire. Préparer les poules dans BellePoule,
  vérifier leur réception dans Marshaller puis le retour d'une affectation piste/arbitre.
- L'appel (pointage) est **verrouillé** après « Tour suivant » : « Tour précédent » le déverrouille. Un
  fichier enregistré après ce passage s'ouvre verrouillé (comportement identique au C++).
- Le tri aléatoire doit passer par `bp_core::sort::glib_sort_by` (le tri Rust plante sur un ordre non total).
- Arbitres : toujours `<Arbitre>`, magasin séparé (`append_referee`).
- GTK4 : ComboBox et TreeView sont dépréciés mais utilisés (`#![allow(deprecated)]`).
- Les fichiers v3 de tableaux ne se rechargent pas (le C++ non plus).
