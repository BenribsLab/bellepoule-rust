# État d'avancement du portage — 20/10/2026

Les agents sont arrêtés. Chaque chantier est enregistré sur sa branche git
(dossier de travail entre parenthèses, sous `.claude/worktrees/`).
**Seul LivePoule est fusionné dans `master`.** Tout le reste attend d'être
fusionné, puis vérifié.

Légende : ✅ fait et testé · 🟡 écrit, compile, non testé · ❌ pas commencé

## Vue d'ensemble

| Chantier | Branche | Compile | Tests | Avancement |
|---|---|---|---|---|
| Fondation commune | `master` | ✅ | ✅ 14 tests | 100 % |
| LivePoule + détecteur USB | `master` (fusionné) | ✅ | ✅ 35 tests | ~95 % |
| Réseau (`bp-net`) | `worktree-agent-ae440e7b1ce30f1c4` | ✅ (3 avertissements) | 🟡 écrits, non lancés | ~85 % |
| Imports / exports (`formats`) | `core-formats` | ✅ | ❌ | ~75 % |
| Tableaux, barrage, scission | `core-table` | ✅ | ❌ | ~60 % (à vérifier) |
| Poules | `core-pool` | ❓ non déclaré, jamais compilé | ❌ | ~50 % |
| Compétition, pointage, classement | `core-contest` | ✅ | 1 test en échec | ~40 % |
| Socle graphique (`bp-gtk`) + écrans `.ui` | `worktree-agent-adcd3ff8d9a736b41` | ✅ | ❌ | ~35 % |
| Marshaller | `marshaller-port` (marshaller) | ✅ | ❌ | ~30 % |
| Écrans des phases et liaison (vague 2) | — | — | — | 0 % |

**Avancement global estimé : ~35 %.** Avant de reprendre le portage, il faut
fusionner toutes les branches et faire passer les tests (voir « Prochaines étapes »).

---

## Fondation — `master` ✅
`attribute`, `player`, `xml`, `stage` (trait `Stage`, `RawStage`), `bout` (Match, Score),
`i18n` (fichiers .po), `bp_net::Message`. Rien à faire.

## LivePoule + détecteur USB — `master` ✅
**Fait** : écran GTK4, chronomètre, lampes, boutons, appareil de score SG, Wi-Fi (wpa_supplicant),
QR code, serveur HTTP chiffré ; détecteur USB (libusb sous Linux, API native sous Windows).
**Reste** :
- compiler la feature `raspberry` (rppal) pour la cible ARM : jamais compilée, faute d'outils de compilation ARM ici ;
- brancher sur `bp_net::Ring` (trait `Network` déjà prévu).

## Réseau — `worktree-agent-ae440e7b1ce30f1c4` 🟡
**Fait** : anneau de partenaires (`ring` : découverte UDP, poignée de main, battement de cœur,
défi USB, identifiants de secours), serveur HTTP, chiffrement AES-256-CBC, QR codes, envois
HTTP/FTP/GREG/escrime-info, téléchargement, lecture des événements USB, console, site web.
**Reste** :
- lancer les tests (dont la découverte de deux anneaux en local), clippy, supprimer 3 avertissements ;
- pages PHP du site web (`my6.php`, `index.php`…) : non portées. Sans PHP installé, seuls les fichiers bruts et une page de sélection sont servis ;
- FTPS (seulement FTP en clair) ;
- non portés volontairement : Facebook, Twitter, OAuth.

## Imports / exports — `core-formats` 🟡
**Fait** : encodages (Latin-1, Windows-1252, UTF-8, UTF-16), FFF (import et export),
CSV (import et export), XML FIE (lecture et écriture), fichiers de classement (TXT/XML),
AskFred (lecteur et écrivain complets), HTML, listes pays/clubs/ligues/régions,
JSON, config.ini (GKeyFile), archives ZIP.
**Reste** :
- **tous les tests** sur `tests-data/` : FFF, XML FIE, classements, ZIP, AskFred aller-retour ;
- intégration côté compétition : `Team::SetAttendingFromMembers`, fin de `ConvertFromBaseToResult`,
  `RegisterNewTeam`, renumérotation au chargement, `Contest::LoadAskFred`, bloc `<Phases>`.

## Tableaux — `core-table` 🟡
**Fait (fichiers écrits)** : `table_set`, `table`, `supervisor` (phase `PhaseDeTableaux`),
`layout` (données de dessin), `print` (feuilles de match), `html`, `barrage`, `splitting`.
Ajouts au trait `Stage` (+16 lignes).
**Reste** :
- l'agent a été coupé sans rendre de rapport : relire la complétude par rapport à `table_set.cpp` (~4000 lignes C++) ;
- tests : têtes de série 8/16/32/64 avec exempts, déroulé complet, classement final, aller-retour XML, barrage, scission ;
- clippy.

## Poules — `core-pool` ❌ (code écrit, jamais compilé)
**Fait (écrit)** : ordre des matchs (tables FIE 2 à 17 tireurs, conformes à `test.cpp`),
`neo_swapper` (répartition en serpentin et séparation), `pool` (scores, V/M, indice, classement,
abandons, XML `<Poule>`, données imprimables), `round` (état partagé du tour, formules,
qualifiés, XML `<TourDePoules>`).
**Reste** :
- déclarer les modules dans `rounds/pool/mod.rs` et corriger la compilation ;
- `allocator.rs` : phase `PoolAllocator` (Load, Save, Garnish, Lock, Reset, IsOver, GetError) ;
- `supervisor.rs` : phase `PoolSupervisor` (`input_provider`, classement, sortie, scores aléatoires) ;
- `pillow.rs` : dialogue des retardataires et des absents ;
- données imprimables de l'allocateur ;
- tests : `test.cpp`, séparation, ex-aequo, aller-retour XML. `tests-data` n'a aucun `<Poule>` : utiliser `original/Exemples/engarde.cotcot`.
**Points à corriger dans la fondation** :
- `player::compare` avec une graine ne fait que le tirage au sort, sans comparer d'abord l'attribut ;
- poules : `NbQualifiesParIndice` à distinguer de `NbQualifies`.

## Compétition — `core-contest` 🟡
**Fait** : `weapon`, `category`, `fie_time`, `filter`, préférences utilisateur, générateur aléatoire
GLib (MT19937, pour reproduire les tirages), fonctions sur les joueurs et les équipes, export HTML,
**phase de pointage** (import FFF/CSV, `rank_importer`, sources), **classement général**
(avec export FFF). Ajouts à `player.rs` et `stage.rs` (+80 lignes).
**Reste** (le plus gros manque du projet) :
- **`contest.rs`** : chargement et enregistrement complets du `.cotcot`, propriétés de la compétition, duplication, compression (`zipper`) ;
- **`schedule.rs`** : suite des phases, chaînage, verrouillage, phase courante, ajout automatique des phases ;
- **`tournament.rs`** : tout sauf les préférences ;
- `attendees`, `error`, `score_collector`, `publication` ;
- test `rounds::checkin::import::tests::csv` en échec (reconnaissance des en-têtes CSV) ;
- 2 avertissements clippy ;
- ⚠ l'agent a converti `player.rs` et `stage.rs` en fins de ligne Windows (CRLF) : normaliser en LF avant fusion.

## Socle graphique + écrans — `worktree-agent-adcd3ff8d9a736b41` 🟡
**Fait** : les 17 écrans du superviseur convertis en GTK4 `.ui` (`resources/ui/`, avec le script
de conversion `bp-gtk/tools/glade_to_ui.py`) ; dans `bp-gtk` : canevas (`canvas/`), dialogues,
chargement des `.ui`, recherche des ressources, configuration utilisateur, utilitaires.
**Reste** :
- l'agent a été coupé sans rendre de rapport : vérifier que chaque `.ui` se charge (`gtk4-builder-tool validate`) ;
- `Module` (hiérarchie, glisser-déposer), liste de joueurs générique, impression (livres, chapitres, sections), export PDF/SVG/PNG du canevas ;
- application `bellepoule` : fenêtre principale du tournoi, menus, langues, À propos, dialogue de compétition, vue des phases, `--smoke-test` ;
- supprimer `bp-gtk/tools/__pycache__/` (commité par erreur).

## Marshaller — `marshaller-port` 🟡
**Fait (logique)** : salle (`hall`), pistes, créneaux (`slot`), planning, affinités, arbitres,
couleurs, armes, temps, configuration, chiffrement, Wi-Fi.
**Reste** :
- logique : `batch`, `batch_panel`, `job`, `job_board`, `job_details`, `competition`, `end_of_burst`, `lasso`, `timeline`, `clock`, `smartphones`, persistance ;
- **toute l'interface** : conversion de `marshaller`, `hall`, `job_board`, `timeline`, `batch`, `competition` et `details.glade`, dessin cairo de la salle et de la timeline ;
- trait `Network` et liste des messages, `--smoke-test`, tests.

## Vague 2 — pas commencée ❌
- Écrans des phases dans `bellepoule` : pointage, composition des poules, poules (saisie des scores), tableaux, barrage, scission, classements, arbitres, planning.
- Impression et PDF de toutes les feuilles (poules, tableaux, feuilles de match, classements).
- Liaison réseau BellePoule ↔ Marshaller ↔ LivePoule ↔ smartphones via `bp_net::Ring`.
- Exports (FFF, XML, HTML, AskFred, GREG) branchés sur le modèle de compétition.
- Tests de bout en bout : ouvrir chaque fichier de `tests-data/`, dérouler une compétition complète, réenregistrer.

---

## Prochaines étapes (dans l'ordre)
1. Ajouter un `.gitattributes` (fins de ligne LF) et fusionner les branches dans `master` dans
   l'ordre : réseau → formats → compétition → tableaux → poules → socle graphique → Marshaller.
   Conflits attendus : `Cargo.toml`, `Cargo.lock`, `stage.rs`, `player.rs`, `rounds/mod.rs`.
2. `cargo build --workspace`, `cargo test --workspace`, clippy : tout corriger.
3. Terminer les modules incomplets : compétition (`contest`/`schedule`/`tournament`) et poules (allocateur, superviseur).
4. Écrire les tests manquants sur `tests-data/`.
5. Socle graphique, puis vague 2.
