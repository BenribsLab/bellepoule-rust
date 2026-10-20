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
L'utilisateur a franchi « Tour suivant » : l'onglet « Poules / Tour n° 1 » n'affiche que le texte
« Poules » car **les écrans des phases suivantes n'existent pas** (`stages/mod.rs` renvoie `Pending`
pour tout sauf le pointage). Plainte : « les poules ne se créent pas ».

**Travail en cours : écran de composition des poules** (`PoolAllocatorStage`).
- Fichier écrit mais **non compilé et non déclaré** : `crates/bellepoule/src/stages/allocator.rs`.
  À faire : `pub mod allocator;` dans `stages/mod.rs`, brancher
  `StageKind::PoolAllocator => allocator::AllocatorScreen::new(page, index)` dans `build()`,
  puis corriger les erreurs de compilation. Défaut connu : `StageScreen::refresh(&self)` n'a pas de
  `Rc<Self>` alors que `apply` en demande un (le glisser-déposer en a besoin) → faire prendre `&self` à
  `apply` et stocker un `Weak<Self>` dans la structure (`Rc::new_cyclic`), ou donner `refresh(self: &Rc<Self>)`
  au trait. `apply_ref` n'existe pas, c'est un reste à supprimer.
- Contenu : listes déroulantes « nombre de poules » / « tireurs par poule » (modèle `combo_liststore`,
  col. 0 taille, col. 1 nombre, col. 2 icône), grille de poules (2 colonnes) avec glisser-déposer des
  tireurs (`DragSource`/`DropTarget`, type `PlayerRef`), icônes déplacé/erreur, critères de séparation
  (cases + critère supplémentaire), liste des tireurs (bouton bascule), filtre « Données ».
  Reste à faire dans cet écran : retardataires/absents (`on_not_available`), impression.
- API cœur à utiliser : `PoolRound` dans `bp-core/src/rounds/pool/round.rs` (`configs`, `selected_config`,
  `select_config`, `pools`, `move_fencer`, `check_distribution`, `set_swapping_criteria`, `swapper`, `is_moved`,
  `pool_is_balanced`, `allocator_filter`…). Si les poules sont vides à l'affichage, vérifier que
  `garnish`/`setup` ont bien été appelés à l'entrée dans la phase (`Schedule::enter_next`).
- Référence C++ : `reference/v5.7/sources/BellePoule/supervisor/rounds/poule/pool_allocator.cpp`
  (`Display`, `FillPoolTable`, `DisplayPlayer`, `DropObject`, `OnSwappingToggled`, `SetUpCombobox`).

## Suite (dans l'ordre)
1. Finir l'écran allocateur ci-dessus ; demander à l'utilisateur de tester (nouveau fichier → ajouter des
   tireurs → les marquer présents → « Tour suivant » → vérifier les poules, glisser-déposer, formules).
2. Écran du superviseur de poules (`pool_supervisor.ui`, `pool.ui`) : feuilles de poule, saisie des scores,
   classement du tour, arbitres/pistes.
3. Écrans tableaux (`table_supervisor.ui`, `table.ui`), barrage, scission, classement général.
4. Menu des langues (`language.rs` : code mort), impression (PlayersList, poules, tableaux via
   `GtkPrintOperation`), dialogues retardataires/absents (`pillow_list.ui`), publication/réseau (menus
   désactivés), exports restants (AskFred : incohérence du magasin d'arbitres).
5. Marshaller (interface + logique, seulement ~30 %), intégration LivePoule, routage des messages réseau.
6. Retirer les crochets de débogage : variables `BP_DEBUG_SHOW_FORM` et `BP_DEBUG_TOGGLE`
   (`stages/checkin.rs`), `PlayersListView::debug_toggle`.
7. Tenir `AVANCEMENT.md` à jour (global ≈ 55 %).

## Pièges connus
- L'appel (pointage) est **verrouillé** après « Tour suivant » : « Tour précédent » le déverrouille. Un
  fichier enregistré après ce passage s'ouvre verrouillé (comportement identique au C++).
- Le tri aléatoire doit passer par `bp_core::sort::glib_sort_by` (le tri Rust plante sur un ordre non total).
- Arbitres : toujours `<Arbitre>`, magasin séparé (`append_referee`).
- GTK4 : ComboBox et TreeView sont dépréciés mais utilisés (`#![allow(deprecated)]`).
- Les fichiers v3 de tableaux ne se rechargent pas (le C++ non plus).
