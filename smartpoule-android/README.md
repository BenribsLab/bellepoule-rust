# SmartPoule (Android)

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
