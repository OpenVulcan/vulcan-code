Vulcan Code 0.1.8 exécute les commandes shell comme des tâches qui peuvent continuer en arrière-plan, ajoute une page d'accueil de projet, permet à l'icône de la zone de notification d'ouvrir le client de votre choix et de changer de langue, et apporte plusieurs centaines de corrections issues d'une revue complète avant publication. Il inclut le programme d'installation 0.2.10.

### Mettre à jour vers cette version

```ui-checklist
ok | Mise à jour complète | Cette version modifie de nouveau l'installateur ; elle s'installe donc comme une mise à jour complète : sous Windows, l'application télécharge et exécute l'installateur une fois
ok | Installations par setup | Les installations faites avec le programme d'installation se mettent à jour avec vulcan-code-setup update ; l'application l'indique au lieu de proposer un téléchargement
```

### Commandes shell

```ui-checklist
ok | Sortie en direct | Une carte de commande dépliée affiche les dernières lignes d'une commande en cours ; les longues commandes ne semblent plus bloquées
ok | Tâches en arrière-plan | Une commande peut tourner en arrière-plan ; un message envoyé pendant son exécution l'y fait passer, et la carte de commande permet aussi de l'y déplacer
ok | Plus de limite de taille | La sortie va dans un journal tournant au lieu d'arrêter la commande à 8 Mio ; l'IA reçoit les lignes les plus récentes et peut lire les précédentes au besoin
ok | Panneau d'activité | Le panneau de session compte les sous-agents et les commandes en arrière-plan et ouvre leur liste avec leurs journaux ; un redémarrage pour mise à jour avertit des tâches en cours
```

### Projets

```ui-checklist
ok | Accueil du projet | Le nom du projet dans la barre latérale ouvre une page de projet avec renommage, actions rapides, statistiques de sessions, liste de sessions consultable et tous les réglages du projet
ok | Modifications non enregistrées | Les modifications non enregistrées de la mémoire du projet sont conservées quand on quitte la page du projet puis qu'on y revient
```

### Zone de notification

```ui-checklist
ok | Double-clic | Sous Windows, un double-clic sur l'icône ouvre le client choisi : l'application de bureau, la WebUI ou le client terminal ; le menu passe au clic droit
ok | Langue | Un menu Langue dans la zone de notification change la langue de l'interface, et les clients ouverts suivent
```

### Interface

```ui-checklist
ok | Largeur de lecture | Toutes les largeurs prédéfinies de la conversation sont plus grandes, et une colonne plus large ne replie plus la navigation dans les fenêtres ordinaires
ok | Indicateur d'état | L'indicateur d'état se place désormais sur le bord supérieur de la zone de saisie au lieu de flotter au-dessus de la conversation
ok | Thèmes sombres | Le texte sélectionné est visible dans les thèmes sombres
ok | Composants de réponse | Les clés longues ne compriment plus les valeurs dans une colonne étroite, et les formules en plusieurs groupes avant une légende s'affichent correctement
ok | Exécutions d'outils | Les cartes d'outils en cours dans une exécution repliée restent groupées
```

### Corrections

```ui-checklist
ok | Mode plan | Seul un plan soumis attend une confirmation, et il est restauré après un redémarrage
ok | Réponses vides | Une réponse qui se termine sans contenu est relancée selon vos réglages de nouvelle tentative ; si la limite de sortie la coupe avant toute réponse, cela est indiqué
ok | Reprise d'une session | Envoyer un message après le rangement du contexte ne fait plus échouer le démarrage
ok | Paramètres du modèle | Un paramètre défini pour certains modèles n'est pas envoyé aux autres modèles
ok | MCP | Les listes d'outils sur plusieurs pages sont lues en entier, et les en-têtes que vous définissez l'emportent sur ceux des variables d'environnement
ok | Installateur Windows | Si le passage à la nouvelle version échoue, l'installateur en donne la raison et conserve les fichiers
ok | Installation endommagée | Une application de bureau dont l'installation est incomplète le signale au lieu de démarrer le mauvais service
ok | Corrections de la revue | Plusieurs centaines de petites corrections dans les sessions, le stockage, la mémoire, les fichiers, les autorisations, les plugins, la WebUI et le client terminal
```

### Sécurité

```ui-checklist
ok | Navigateur intégré | Le navigateur intégré bloque toutes les formes d'adresse locale du port protégé du Core
ok | Erreurs du fournisseur | Les clés d'API sont retirées des erreurs du fournisseur avant qu'elles soient raccourcies ou affichées
ok | Outils de plugins | Les arguments qui ne correspondent pas aux paramètres d'un outil de plugin sont refusés avant son exécution
```

### Client terminal

```ui-checklist
ok | Activité | Un onglet Activité, un compteur dans la barre de session et une vue des journaux des commandes en arrière-plan, avec un avertissement avant le redémarrage
ok | Corrections | Les sélecteurs, le collage dans les réponses aux questions, la sélection à la souris, les longs menus et les blocs de code fonctionnent correctement
```

### Programme d'installation

```ui-checklist
ok | Nouvelle tentative après échec | Relancer setup après un changement de version échoué réenregistre le service
ok | Plusieurs installations | Désinstaller une installation ne touche pas aux lanceurs, entrées de menu ni services d'une autre
```
