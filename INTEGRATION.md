# Organisation intégrée dans BellePoule

Décision du 21 octobre 2026 : un seul lancement, `lancer.bat`, et une seule fenêtre principale.
Le découpage historique superviseur/Marshaller est conservé dans le code comme séparation de
responsabilités ; il ne constitue plus une obligation pour l'utilisateur.

## Quatre usages

| Arbitres | Pistes | Usage |
|---|---|---|
| Non | Non | Compétition habituelle ; aucun outil d'organisation imposé |
| Oui | Non | Catalogue, import, présence et affectation des arbitres sans piste obligatoire |
| Non | Oui | Plan de salle, numéros, planning et affectation des pistes sans arbitre obligatoire |
| Oui | Oui | Gestion complète, avec plusieurs pistes et un arbitre attitré par piste |

BellePoule démarre sur son accueil habituel, ou sur la compétition passée au lanceur.
Le bouton **Organisation**, dans la navigation latérale, ouvre la vue seulement à la demande.
**Fermer l'organisation** retire cette vue et revient à la compétition précédente ou à l'accueil.
Cette fermeture conserve les ressources et leur synchronisation.

Les deux options sont dans la vue **Organisation** et sont mémorisées pour le prochain lancement.
Les désactiver masque les outils ; cela ne supprime ni les arbitres ni les affectations enregistrées.
Une poule déjà répartie sur plusieurs pistes conserve ses associations, même si l'on ne gère
ensuite que ses arbitres : les numéros sont affichés en lecture seule dans le dialogue.

La vue intégrée utilise une colonne d'arbitres compacte, avec défilement horizontal de la liste.
Les pistes et les matchs sont placés l'un au-dessus de l'autre à droite. Deux séparateurs permettent
de régler la largeur de la colonne et la hauteur de la liste de matchs. Les commandes d'arbitres
ne s'étirent plus en hauteur ; la barre d'outils des pistes peut revenir à la ligne.

## Responsabilités

- `bp-core` reste propriétaire des compétitions et de leurs résultats. Les affectations reçues
  sont enregistrées dans les `.cotcot`, comme les affectations effectuées auparavant par Marshaller.
- La bibliothèque `marshaller` contient le modèle de salle, les disponibilités et les vues réutilisables.
  `Ui::embedded` construit ses widgets dans la fenêtre de BellePoule, sans `Ring::join`, sans processus
  secondaire et sans gestionnaire de fermeture de l'application hôte.
- `bellepoule::management` synchronise les compétitions ouvertes et les ordres d'affectation en mémoire.
  Il ne fait pas transiter la gestion locale par HTTP, UDP ou le pare-feu.
- Les ressources sont communes aux compétitions ouvertes : un arbitre ou une piste réservé pour
  l'une n'est pas disponible simultanément pour une autre. Les contrôles restent dans le modèle.
- Le catalogue des arbitres est conservé séparément (`local-referees.xml` dans la configuration de
  la salle), même si aucune compétition n'est ouverte. La géométrie des pistes garde `hall.json`.

## Affectations indépendantes

- Sans pistes : `set_job_referees` crée une réservation horaire sans numéro de piste. Il est possible
  d'affecter les arbitres aux poules et aux matchs, puis de sauvegarder/réouvrir ces affectations.
- Sans arbitres : la réservation de pistes ne réclame pas d'arbitres et ne force pas l'ouverture d'une
  liste d'arbitres. Les affectations précédentes sont conservées si elles existent.
- Avec les deux : les réservations communes, les conflits et les vagues de matchs des poules
  multi-pistes restent ceux de l'extension déjà écrite.

Dans la liste des matchs, un double-clic ouvre la fiche. Le bouton **Ressources…** donne accès à
l'affectation adaptée au mode actif. La salle conserve également le glisser-déposer et son menu contextuel.

## Réseau et compatibilité

Un seul anneau réseau appartient à BellePoule. Il reste disponible pour les autres services et les
smartphones. La vue intégrée utilise sa clé pour les arbitres, mais ne crée pas un second serveur.
Quand la gestion locale est active, les affectations d'un Marshaller externe sont ignorées pour éviter
deux gestionnaires concurrents. L'ancien exécutable reste disponible pour les installations séparées ;
il n'est pas nécessaire pour le fonctionnement intégré.

## Validation

Compilation et tests compilés uniquement par l'agent ; essais interactifs à faire par l'utilisateur.
Vérifier les quatre combinaisons, les changements d'options avec affectations existantes,
les compétitions simultanées, les imports/suppressions d'arbitres, la fermeture et la réouverture.
