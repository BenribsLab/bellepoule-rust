# État d'avancement du portage — 20/10/2026

**→ Pour reprendre le travail : lire `REPRISE.md` (point d’arrêt exact et suite).**

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
| Tableaux, barrage, scission | ✅ logique terminée (hors réseau et écrans) | ~90 % |
| Poules | ✅ logique terminée (hors réseau et écrans) | ~90 % |
| Compétition, pointage, classement | ✅ logique terminée (hors réseau) | ~90 % |
| Socle graphique + fenêtre principale | 🟡 fenêtre et tous les types de phases écrits ; validation interactive restante | ~45 % |
| Marshaller | 🟡 | ~30 % |
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
**Reste** : menu des langues, validation des phases (étape 6), impression de l'appel.

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
