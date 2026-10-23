# Chronomètre et cartons SmartPoule — 22 octobre 2026

## Demande et choix

L'application Android doit recevoir les paramètres de temps définis dans la formule du web ou du
logiciel de bureau. Défauts explicitement demandés : **poules 5 touches / 1 manche de 3 minutes** ;
**tableaux 15 touches / 3 manches de 3 minutes**. Premier nom en haut de la feuille = vert par défaut.
Les côtés affichés sont inversables sans modifier l'ordre des tireurs dans le modèle ni dans l'API.
Les cartons doivent être enregistrés sur le match concerné et apparaître sur sa feuille.

L'image fournie guide la disposition du téléphone : grand décompte, scores rouge/vert sur noir,
grand bouton démarrer/pause et cartons par tireur. Il ne s'agit pas d'une image insérée dans l'écran.

## Sources modifiées

- Cœur de référence : `crates/bp-core/src/officiating.rs` (`BoutTiming`, `CardEvent`, `Officiating`),
  `stage.rs`, `bout.rs`, `smartpoule.rs`, `rounds/pool/{round,stages,pool}.rs`,
  `rounds/table/{supervisor,table_set}.rs`.
- Bureau : `widgets/bout_timing.rs`, `schedule_view.rs`, `smartpoule.rs`, impressions des poules et
  tableaux, `widgets/score_sheet.rs`. Le wrapper des préférences affiche durée en secondes et manches.
- Android : `BoutClock.kt`, `BoutActivity.kt`, `Api.kt`, `SheetActivity.kt` ; version source 1.1.
- Projet voisin `../Bellepoule Web` : API `src/{smartpoule,stages}.rs`, vues du client
  `src/stages/{mod,pools,groups,sheets}.rs`, copie `bellepoule-web-api/bp-core/src`.
  La copie du cœur a été synchronisée sans suppression de fichiers ; rester cohérent avec
  `tools/sync_web.py` pour les prochaines évolutions. Ne pas traiter la copie web comme source maîtresse.

## Contrat

Configuration : `BoutTiming { period_seconds: u32, periods: u32 }`, durée 1–3600 secondes,
nombre de manches 1–20. Les deux phases d'un tour de poules partagent la configuration du `PoolRound`.
Les tableaux utilisent celle du `StageBase`. Les nouveaux tableaux ont 15 touches ; les valeurs
explicitement enregistrées dans les anciens fichiers ne sont pas remplacées.

Le JSON de la feuille SmartPoule inclut `timing`. Chaque match inclut `officiating` s'il en possède un.
Le téléphone peut envoyer un état d'arbitrage seul avant de valider le résultat : `/phone/score` reçoit
`values` facultatif et `officiating` facultatif (au moins l'un des deux). Ancien client à scores seuls compatible.
Le relais transmet les deux champs au bureau via `/pending`. Les accès sont contrôlés par club,
affectation de l'arbitre et indice réel du match, y compris les poules multi-pistes.

Un état contient révision, manche, temps restant, brouillon des touches, indice du tireur vert et journal.
Un carton contient ID stable, indice du tireur, couleur, manche, temps restant, horodatage et annulation.
Les états plus anciens sont refusés ; une réémission identique est acceptée. Les événements déjà
enregistrés ne peuvent pas disparaître du journal : une correction les marque annulés.

Le chrono tourne localement sur une horloge monotone. Les instantanés envoyés sont des temps restants,
pas une commande pour faire tourner un second chrono distant. Pause en arrière-plan, sauvegarde locale,
réessai manuel en cas de réseau absent, envoi immédiat des cartons et envoi groupé avec le résultat.
Il n'y a pas d'application automatique des sanctions sportives : le journal des cartons est distinct
de la saisie des touches/exclusions. Il n'y a pas de temps de repos automatique entre manches.

## Persistance et feuilles

- Formule XML : attributs `DureeManche` (secondes), `NbManches`.
- Match XML : attribut `Arbitrage`, contenant du JSON échappé ; relecture explicite des matchs de
  poules et de tableaux (ces deux chargeurs n'utilisent pas tous `Match::load_header`).
- Android : cache privé par serveur/club/arbitre/jeton/indice de match ; ID des cartons indépendant
  de l'affichage rouge/vert. Les données restent disponibles si la synchronisation échoue.
- Relais : `smartpoule-relay.json` dans le dossier du club, sauvegarde par fichier temporaire ;
  les saisies en attente ne sont plus purgées avec les anciens résultats traités.
- Listes Android et web, feuilles de poule web/bureau et feuilles de tableau imprimables : journal
  affiché au match concerné. Les impressions identifient les tireurs par T1/T2 dans l'ordre de la feuille.

## État et reprise

**Mise à jour de livraison Android (22/10)** : l'APK resté en 1.0 a été reconstruit à la suite du retour
utilisateur. Une accolade manquante dans `BoutActivity.refresh` a été corrigée ; `assembleDebug`
réussit et produit la version 1.1 (code 2), copiée sous `smartpoule-android/SmartPoule-1.1.apk`.
Aucun essai interactif effectué. Le bilan initial ci-dessous précède cette livraison ; il reste
valable pour l'absence de reconstruction/déploiement de l'API et du bureau par l'agent.

Correctif après retour utilisateur (gel du PC à l'ouverture d'une compétition) : dans la branche
poule de `smartpoule_sheet`, ne pas appeler `stage.bout_timing()` pendant que `a.round()` est
verrouillé. Ce getter reprend le même Mutex. Le JSON lit maintenant `round.bout_timing`
directement, dans les sources du bureau et la copie web. Pas de compilation ni d'essai par l'agent.

**Aucune compilation, aucun test, aucun lancement d'application ou de conteneur n'a été effectué
pour ce chantier**, conformément à la consigne utilisateur. L'APK et les services actuellement installés
ne contiennent donc pas encore ces changements. Les contrôles fonctionnels et la construction des
trois parties reviennent à l'utilisateur ; ne pas annoncer une version validée ou déployée.

Scénarios à lui proposer s'il demande une liste : configuration et relecture `.cotcot`, match de poule
et de tableau, départ/pause/correction/fin du temps, passage de manche, inversion des côtés avec score
et cartons déjà saisis, annulation d'un carton, retour à la feuille, réseau absent puis réessai,
relance du téléphone et du serveur, affectation multi-pistes, impressions avec plusieurs événements.

Ne pas reconstruire ou démarrer les services de sa propre initiative. Aucun commit n'a été créé.
