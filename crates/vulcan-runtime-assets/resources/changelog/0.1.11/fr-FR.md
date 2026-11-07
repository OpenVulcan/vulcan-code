Vulcan Code 0.1.11 ajoute les avis, un centre de messages et un moyen de corriger un problème sur votre propre appareil, et met à jour les modèles d’OpenAI, TokenRhythm, OpenCode Go et OpenDesign.

### Mise à jour vers cette version

```ui-checklist
ok | Mise à jour incrémentielle | La mise à jour depuis 0.1.8, 0.1.9 ou 0.1.10 ne télécharge que les fichiers modifiés ; les versions plus anciennes s’installent toujours en mise à jour complète
ok | Base de données de l’application | Au premier démarrage, la base de données de l’application est sauvegardée puis mise à niveau automatiquement vers la nouvelle structure, sans action de votre part
```

### Avis et messages

```ui-checklist
ok | Envoyer un avis | Une fois connecté, « Envoyer un avis » permet de signaler un problème ou de faire une suggestion ; avant l’envoi, les secrets, chemins, votre nom d’utilisateur, les URL et les données personnelles sont filtrés sur votre appareil et vous voyez exactement le contenu qui sera envoyé
ok | Signaler depuis une carte d’erreur | Quand une conversation échoue, sa carte d’erreur propose un signalement en un clic ; seulement pour les échecs que Vulcan Code peut causer, pas pour la connexion, le solde, les limites de débit, les pannes de service, les délais dépassés, la politique de contenu ou l’annulation
ok | Centre de messages | Le « Centre de messages » affiche les annonces, les messages personnels et les réponses des mainteneurs à vos avis ; les annonces importantes apparaissent aussi en bannière
ok | Corriger sur cet appareil | Choisissez un dossier : une copie de travail de vulcan-code est préparée comme projet, avec une conversation dotée de la compétence de correction ; rien n’est poussé ni publié avant que vous confirmiez la pull request dans cette conversation
ok | Entrées GitHub | Le menu du compte ajoute « Suggérer sur GitHub (issue) » et « Proposer une pull request sur GitHub »
ok | Client terminal | Le client terminal dispose aussi de la page d’avis
```

### Modèles et fournisseurs

```ui-checklist
ok | OpenAI | Huit modèles de l’API publique déclarent une sortie structurée stricte json_schema ; GPT-5.6 Sol/Terra/Luna, GPT-6 Sol/Luna et GPT-6.1 Terra peuvent désactiver le raisonnement
ok | TokenRhythm | Ajout de DeepSeek V4.1 Flash 1017, LongCat 2.0, MiniMax M3.1 Flash et des modèles de test Egg-Alpha et Neohorse 1 9B, retrait de minimax-m2.5 et mimo-v2.5-pro ; la sortie structurée est déclarée, les modèles Messages envoient désormais l’intensité de réflexion et peuvent la désactiver, DeepSeek V4 Pro 0813 et GLM-5.2 passent à Responses et Kimi K2.6 à Chat
ok | OpenCode Go | Ajout de Gemini 4 Argon, Hy4 et des modèles gratuits Beam, Iceman et Crouton ; dix modèles déclarent une sortie structurée
ok | OpenDesign | GPT-6 Luna utilise désormais Responses et Claude Haiku 5.5 Messages ; ajout de deepseek-v4.1-flash-eco, hy4-preview, minimax-m3.1-flash et de 14 modèles de conversation Plus/Pro/Max ; la génération d’images avec GPT Image 2, Nano Banana 2/2 Lite et Seedream 5.0/5.0 Pro est disponible dans les offres qui l’incluent ; les niveaux de réflexion se limitent à ceux publiés par chaque fournisseur
ok | Modèles de mémoire | Davantage de modèles peuvent désactiver la réflexion et renvoyer une sortie structurée, et peuvent donc servir de modèles de mémoire
```
