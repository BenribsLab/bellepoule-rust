# Présentation de BellePoule

## Maquette de référence fournie ensuite par l'utilisateur

La direction visuelle est désormais celle d'une application avec navigation latérale permanente :
barre de marque, menus historiques en haut, accueil permettant de créer/ouvrir/retrouver une compétition,
puis bandeau de compétition illustré et espace de travail. La navigation latérale peut être masquée avec
le bouton de l'en-tête. Les phases futures ne sont pas débloquées par les raccourcis.

Le bandeau affiche les vraies propriétés de l'épreuve. Les cartes de l'Appel affichent les compteurs
existants ; la recherche et les filtres agissent sur l'affichage des lignes, sans modifier les participants.
Les actions de présence globale conservent leurs gestionnaires d'origine. Les noms sont renforcés, les
fonds des lignes alternent et la présence bénéficie d'un libellé coloré en complément de la case.

Les visuels photographique et illustré proviennent de l'outil intégré imagegen ; ils sont conservés dans
`resources/visuals` avec leurs prompts. Les icônes de navigation restent des SVG dessinés en code.
L'utilisateur assure la validation interactive et a demandé à l'agent d'arrêter ses vérifications.

Décision utilisateur du 21 octobre 2026 : les noms des menus et leur emplacement général restent
familiers ; l'apparence peut s'écarter de BellePoule 5.7 pour devenir plus moderne et efficace.

## Principes

- Les données occupent l'espace libre : tireurs, poules, tableaux, pistes et matchs.
- Les commandes occupent leur hauteur naturelle. Aucun groupe d'options ne doit se partager
  l'espace vertical disponible avec une liste comme si c'était une seconde zone de travail.
- Menus en haut, compétitions puis phases en onglets, actions près de leur contenu.
- Interface claire : surfaces blanches, fond gris léger, texte ardoise, accent bleu.
- Petites icônes accompagnées de texte, focus visible au clavier, actions désactivées identifiables.
- Espacement cohérent : commandes de taille modérée, tableaux lisibles avec des lignes aérées.

## Règles de disposition (corrigées le 21/10/2026)

- Les illustrations décoratives (bandeau, barre latérale, accueil) sont des **fonds CSS**
  (`visuals::background_art`, classes `bp-hero-art` / `bp-sidebar-art`). Ne jamais utiliser
  `GtkPicture` pour une décoration : sa taille naturelle est celle du fichier, ce qui faisait
  occuper la moitié de la fenêtre au bandeau et à la barre latérale.
- Le bandeau de compétition a la hauteur naturelle de son texte (une seule ligne d'en-tête).
- La barre latérale est dans un `GtkPaned` : largeur réglable à la souris, mémorisée
  (`Tournament/sidebar_width`, 210 px par défaut, 150 px minimum), masquable par le bouton
  de l'en-tête.

## Implémentation

`bp-gtk::appearance` centralise le thème et les règles de disposition. BellePoule l'active avant
le chargement des interfaces. Le CSS est intégré au binaire (`appearance.css`). Les fenêtres créées
par code reçoivent également le thème, y compris les dialogues. Le Marshaller autonome conserve
sa présentation historique ; les vues embarquées héritent du thème de BellePoule.

La normalisation des commandes conserve les objets et les signaux d'origine. Elle retire les
propriétés d'expansion héritées de la conversion GTK2, met les petites icônes à côté des libellés
et compacte les barres d'actions. Les emplacements réservés aux données restent extensibles.
Les contrôles complexes (saisie, listes déroulantes) gardent leurs widgets internes.

L'Appel utilise un résumé de présence sur une petite ligne et une barre d'actions qui peut revenir
à la ligne. Les listes restent défilables dans les deux directions. Les options de composition
des poules et de configuration des phases sont compactées. La couleur d'une compétition apparaît
comme une bordure d'identification, au lieu de remplir tout son onglet.

Organisation reste accessible à la demande depuis le bouton près du menu principal ; sa fermeture
revient à la compétition ou à l'accueil. Voir `INTEGRATION.md` pour ses modes de fonctionnement.

## Vérification

Le thème est validé avec le parseur CSS de la bibliothèque GTK installée, sans ouvrir de fenêtre.
La compilation Rust vérifie les liaisons ; elle ne constitue pas une validation visuelle.
Les essais interactifs sont réservés à l'utilisateur.

À vérifier en fenêtre normale et maximisée : Appel avec des tireurs et avec des équipes,
composition des poules, saisie des scores, tableaux, dialogues et Organisation. Réduire la largeur
pour contrôler le retour à la ligne des actions et le défilement des listes. Vérifier la navigation
au clavier, les imports, la présence et les changements de phase.
