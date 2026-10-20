# État d'avancement du portage — 20/10/2026

**Tout est fusionné dans `master`** (une seule branche) et `cargo check --workspace` passe sans erreur.
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
| Tableaux, barrage, scission | 🟡 | ~60 % |
| Poules | 🟡 écrit mais non branché | ~50 % |
| Compétition, pointage, classement | 🟡 | ~40 % |
| Socle graphique + écrans `.ui` | 🟡 | ~35 % |
| Marshaller | 🟡 | ~30 % |
| Vague 2 (écrans des phases, impression, liaison réseau) | ❌ | 0 % |

**Global : ~35 %.**

---

## Ordre des chantiers restants (un seul à la fois)

Chaque étape débloque la suivante. Ne rien lancer en parallèle.

### Étape 1 — Compétition (`contest`, `schedule`, `tournament`) ← PROCHAINE
Sans elle, on ne peut ni ouvrir ni enregistrer un fichier : tout le reste en dépend.
- `contest.rs` : chargement et enregistrement complets du `.cotcot`, propriétés, duplication, compression (`zipper`) ;
- `schedule.rs` : suite des phases, chaînage entrée/sortie, verrouillage, phase courante, ajout automatique des phases ;
- `tournament.rs` : tout sauf les préférences (déjà faites) ;
- `attendees`, `error`, `score_collector`, `publication` ;
- brancher les formats (équipes, `ConvertFromBaseToResult`, `LoadAskFred`, bloc `<Phases>`) ;
- corriger le test `rounds::checkin::import::tests::csv` ;
- ajouter la méthode `set_previous` sur les appels du chaînage.

### Étape 2 — Poules
- déclarer `dispatcher`, `neo_swapper`, `pool`, `round` dans `rounds/pool/mod.rs` et corriger la compilation (jamais compilés) ;
- `allocator.rs` : phase `PoolAllocator` ; `supervisor.rs` : phase `PoolSupervisor` ; `pillow.rs` ;
- données imprimables de l'allocateur ;
- fondation : `player::compare` avec graine doit comparer l'attribut avant le tirage au sort.

### Étape 3 — Tableaux : relecture de complétude
Écrits sans rapport de l'agent : comparer avec `table_set.cpp` (~4000 lignes C++), compléter.

### Étape 4 — Réseau et formats : nettoyage
- réseau : 3 avertissements ; pages PHP du site web non portées ; FTPS absent ;
- formats : intégration avec la compétition ; aucun test écrit sur `tests-data/`.

### Étape 5 — Socle graphique et application `bellepoule`
- vérifier que chaque `.ui` se charge (`gtk4-builder-tool validate`) ;
- `Module`, liste de joueurs générique, impression (livres, chapitres, sections), export PDF/SVG/PNG ;
- fenêtre principale du tournoi, menus, langues, À propos, dialogue de compétition, vue des phases, `--smoke-test`.

### Étape 6 — Écrans des phases (vague 2)
Pointage, composition des poules, poules, tableaux, barrage, scission, classements, arbitres, planning.
Impression et PDF des feuilles ; exports branchés sur le modèle.

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
*(à remplir au fil de l'eau : quoi lancer, résultat attendu, retour)*

Aucun pour l'instant.
