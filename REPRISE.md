# Reprise du travail — point d'arrêt (20/10/2026)

Document écrit pour qu'une autre IA (ou une autre personne) reprenne le portage sans perdre le fil.
À lire avec `ARCHITECTURE.md` (plan général) et `AVANCEMENT.md` (état par chantier).

## But
Réécrire **BellePoule 5.7** (gestionnaire de compétitions d'escrime, C++/GTK2, sources dans
`reference/v5.7`) en **Rust + GTK4 (gtk4-rs)**, comme un clone **parfait et autonome** (aucun lien avec
la version C++ à l'exécution ; seuls les formats d'échange sont conservés : .cotcot, FFF, FIE XML, CSV,
AskFred). Programmes : BellePoule (superviseur), Marshaller, LivePoule (+ backend USB).
Facebook, Twitter et OAuth sont exclus.

## Consignes de l'utilisateur
- Décider seul, ne pas demander de validation ; viser la fidélité exacte au C++.
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
Les écrans de composition et de supervision des poules sont branchés et compilent.
**Aucune validation interactive de ces nouveaux écrans n'a encore été faite.**
Le chantier suivant est l'écran des tableaux ; ne pas confondre compilation et fidélité vérifiée.

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

### Vérifications réalisées
Avec `CARGO_TARGET_DIR=target/agent` :
- `cargo check --workspace --tests` : succès (tests compilés, pas exécutés).
- `cargo build --workspace` : succès.
- `cargo clippy --workspace --tests` : succès avec avertissements préexistants.
Aucun programme GTK, test interactif ou test automatisé exécuté pendant cette reprise.
Deux régressions ajoutées et compilées : saisie Unicode et plusieurs retardataires avant renumérotation.

## Suite (dans l'ordre)
1. Vérifier les écrans de poules selon les scénarios ci-dessous. Comparer au C++ le rendu/zoom,
   les colonnes graphiques et dispositions de filtres à l'écran et sur papier, la navigation au
   clavier et les impressions (grandes poules, noms longs). La feuille utilise des widgets GTK ;
   sa fidélité visuelle exacte au canevas C++ n'est pas encore établie. Pistes/arbitres affichés ;
   aiguillage réseau encore à faire.
2. Écrans tableaux (`table_supervisor.ui`, `table.ui`), barrage, scission, classement général :
   ces phases affichent toujours `Pending`.
3. Menu des langues, impression de l'appel, publication/réseau, exports restants
   (AskFred : incohérence du magasin d'arbitres).
4. Marshaller (interface/logique ~30 %), intégration LivePoule, routage des messages.
5. Retirer `BP_DEBUG_SHOW_FORM`, `BP_DEBUG_TOGGLE` et `PlayersListView::debug_toggle`.
6. Tenir `AVANCEMENT.md` à jour ; ne pas annoncer un clone terminé sans validation des fonctions
   restantes et comparaison à la référence.

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

## Pièges connus
- L'appel (pointage) est **verrouillé** après « Tour suivant » : « Tour précédent » le déverrouille. Un
  fichier enregistré après ce passage s'ouvre verrouillé (comportement identique au C++).
- Le tri aléatoire doit passer par `bp_core::sort::glib_sort_by` (le tri Rust plante sur un ordre non total).
- Arbitres : toujours `<Arbitre>`, magasin séparé (`append_referee`).
- GTK4 : ComboBox et TreeView sont dépréciés mais utilisés (`#![allow(deprecated)]`).
- Les fichiers v3 de tableaux ne se rechargent pas (le C++ non plus).
