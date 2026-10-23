# SmartPoule (Android)

## Chronomètre et cartons — 22/10/2026

Version **1.1 (versionCode 2)**, APK reconstruit le 22/10 après le signalement d'une installation
restée en 1.0. Construction `assembleDebug` réussie ; aucun lancement ni test interactif par l'agent.
Installer **`smartpoule-android/SmartPoule-1.1.apk`** depuis le projet de bureau (ou `SmartPoule-1.1.apk`
depuis ce dossier). Sortie équivalente : `app/build/outputs/apk/debug/app-debug.apk`.
Le chrono apparaît dans **poule/tableau → match** ; l'accueil ne montre pas le chrono.

- Défauts des nouvelles phases : poules **5 touches, 1 × 180 s** ; tableaux **15 touches, 3 × 180 s**.
  Les réglages existants des fichiers ouverts sont conservés.
- Durée et nombre de manches se règlent dans la formule : préférences de la phase dans BellePoule,
  rubrique « Chronomètre — formule des matchs » sur le web. La feuille API transmet ces réglages.
- Écran Android : grand chrono sur fond clair, scores sur fond noir rouge/vert, démarrer/pause,
  corrections du temps à l'arrêt, réinitialisation du chrono et manche suivante. Pas d'avancement
  automatique de manche ni de validation automatique du résultat à zéro. Signal sonore en fin de temps.
- Horloge monotone (`elapsedRealtime`), écran maintenu allumé ; passage en arrière-plan met le chrono
  en pause et conserve l'état local. Réinitialiser le chrono conserve les scores et les cartons.
- **Premier nom de la feuille = vert par défaut**, affiché à droite. « Inverser les côtés rouge / vert »
  échange les panneaux et les couleurs sans échanger les identités, scores ou cartons. L'ordre des
  tireurs dans les appels API reste toujours celui de la feuille.
- Cartons jaune, rouge, noir par tireur, confirmation, manche et temps restant. L'annulation conserve
  une entrée marquée annulée. Les cartons sont consignés ; les conséquences sportives sur les touches
  et les exclusions restent à décider/saisir par l'arbitre et l'organisation.
- Les saisies sont conservées localement et envoyées même avant la fin du match. Un échec réseau
  conserve la saisie ; « Synchroniser chrono et cartons » permet de réessayer. Le résultat final joint
  le même journal. L'écran distingue l'envoi au relais de la validation effective par BellePoule.

Le serveur API et le logiciel de bureau doivent être mis à jour en même temps que l'APK : ancien
serveur et nouveau téléphone ne partagent pas tout le protocole. Aucun serveur n'a été redémarré.

### Protocole et persistance

`timing = { period_seconds, periods }` est fourni par `/phone/sheet`.
`POST /phone/score` accepte les anciens `values`, un champ `officiating`, ou les deux.
`officiating = { revision, period, remaining_ms, scores, green_side, cards }`.
Chaque carton contient `{ id, side, color, period, remaining_ms, issued_at_ms, cancelled }` ;
`side` est l'indice immuable du tireur dans la feuille (0 ou 1), et non sa position sur l'écran.
`green_side` vaut 0 par défaut. Les révisions évitent de remplacer silencieusement un état plus récent.

Les paramètres de phase sont conservés en XML sous `DureeManche` et `NbManches`. Le journal est
conservé sur le nœud `<Match>` dans l'attribut `Arbitrage` (JSON échappé par le sérialiseur XML).
Les cartons apparaissent dans les listes de matchs Android et web, ainsi que sur les feuilles de match
imprimables du bureau et du web. T1/T2 dans les impressions désignent l'ordre des deux tireurs.
Le relais web conserve désormais ses saisies en attente dans `smartpoule-relay.json`, dans le dossier
privé du club ; la purge conserve les saisies non encore appliquées.

Voir aussi `CHRONO_CARTONS.md` à la racine du projet de bureau pour la reprise technique.

Application d'arbitre : elle affiche les poules et matchs affectés à l'arbitre
et envoie les scores à BellePoule **en passant par le serveur BellePoule Web**
(plus besoin d'être sur le même réseau que l'ordinateur de la compétition).

```
Téléphone ──HTTPS──▶ BellePoule Web (/api/smartpoule/…) ◀──HTTPS── BellePoule (bureau)
```

- BellePoule (bureau) envoie toutes les 3 s l'état des compétitions ouvertes
  (`PUT /api/smartpoule/state`, clé API du club), relève les scores en attente
  (`GET /pending`) et renvoie le résultat (`POST /results`).
- Le téléphone lit ses matchs (`/phone/jobs`), une feuille (`/phone/sheet`),
  envoie un score (`POST /phone/score`) puis suit son sort (`/phone/result`).
- Les compétitions gérées directement sur le web sont servies sans relais.

## Appairage

Dans BellePoule : **BellePoule2D › SmartPoule…** — adresse du serveur et clé API
du club, puis impression des QR codes (un par arbitre, plus un QR
« administrateur » qui voit tous les matchs). Le QR code contient
`https://serveur/smartpoule/?c=<club>&r=<arbitre>&k=<clé>`.

## Compilation

- JDK 21 (Eclipse Adoptium), SDK Android de Unity (`local.properties`).
- `JAVA_HOME="C:\Program Files\Eclipse Adoptium\jdk-21.0.12.101-hotspot"`
- `gradlew assembleDebug` → `app/build/outputs/apk/debug/app-debug.apk`

Le SDK Unity est en lecture seule : `buildToolsVersion` est fixé à une version
déjà installée (36.0.0).
