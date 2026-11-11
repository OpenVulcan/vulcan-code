Vulcan Code 0.1.20 ajoute la transcription dans le cloud OpenVulcan et les dossiers rattachés aux projets, tout en améliorant la reprise des tours en file d’attente et le diagnostic des erreurs des fournisseurs.

### Saisie vocale et espace de travail

```ui-checklist
ok | Transcription dans le cloud | Connectez-vous à OpenVulcan sur un site qui propose le service. Dans les réglages du microphone, choisissez le modèle recommandé par le site, un modèle cloud précis ou un modèle de reconnaissance vocale d’un fournisseur configuré.
ok | Raccourci d’enregistrement | Configurez un raccourci de saisie vocale, puis choisissez la langue de reconnaissance et les options prises en charge par le modèle.
ok | Dossiers rattachés au projet | Ajoutez des dossiers locaux depuis l’accueil du projet pour les parcourir dans le panneau des fichiers et ouvrir leurs fichiers depuis les conversations. Les dépôts Git rattachés apparaissent dans la liste des modifications du projet.
```

### Conversations et mémoire

```ui-checklist
ok | Reprise des tours en attente | Les tours enregistrés reprennent avec l’identifiant et les réglages de leur envoi initial. Les objectifs des conversations existantes ne démarrent qu’une fois.
ok | Branches de conversation | Créez une branche à partir d’un tour terminé pendant qu’une conversation est en cours. Les cartes d’outils conservées dans la conversation affichent leur état final à la fin de l’exécution.
ok | Reconstruction des vecteurs de mémoire | La taille des lots respecte le nombre d’entrées accepté par requête par le modèle d’embedding. L’aperçu affiche le nombre de lots utilisé lors de la reconstruction.
```

### Modèles et dépannage

```ui-checklist
ok | Recherche de modèles du fournisseur | Récupérez les modèles de conversation disponibles sur le compte sélectionné d’un fournisseur intégré, puis préremplissez le formulaire de création avec votre choix.
ok | Modèles Alibaba Cloud | Token Plan ajoute Qwen4.0 Max Preview. Les API de Model Studio à Pékin et à Singapour proposent les modèles Qwen3.8.
ok | Détails des erreurs et nouvelles tentatives | Les cartes d’erreur affichent un diagnostic court et permettent de copier le JSON complet ou d’ajouter l’erreur à la liste des nouvelles tentatives. Modifiez chaque règle à l’aide de mots-clés ou d’expressions régulières.
```
