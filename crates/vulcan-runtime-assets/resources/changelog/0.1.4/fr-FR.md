Vulcan Code 0.1.4 ajoute un moyen rapide de mettre à jour, affiche dans la barre d’état ce que signale votre compte de modèle et garde les boîtes de dialogue utilisables sur les petits écrans.

### Mises à jour

```ui-checklist
ok | Bouton de mise à jour | Lorsqu’une version plus récente est disponible, un bouton de mise à jour apparaît à côté du nom Vulcan Code dans la barre latérale ; il présente les nouveautés, puis télécharge et redémarre sans ouvrir les paramètres
ok | Versions sautées | Lorsqu’une mise à jour saute des versions, son aperçu affiche les notes de chaque version sautée, de la plus récente à la plus ancienne ; cela s’applique aux mises à jour à partir de la 0.1.4
```

### Conversation et barre d’état

```ui-checklist
ok | Conversation stable | Les indications d’envoi d’une requête, de rappel de la mémoire et de préparation d’un appel d’outil flottent désormais au-dessus de la zone de saisie, si bien que la conversation ne saute plus quand elles changent
ok | Compte de modèle | La barre d’état affiche le solde du compte, une petite barre avec le quota restant, ou les deux ; un compte qui ne signale ni l’un ni l’autre affiche seulement son nom au lieu de « Solde - »
```

### Petits écrans

```ui-checklist
ok | Boîtes de dialogue | Les boîtes de dialogue avec des boutons en bas font défiler leur contenu sur les écrans peu hauts, pour que leurs boutons restent accessibles, y compris la configuration rapide de la mémoire à long terme et les approbations Ask
ok | Configuration rapide de la mémoire à long terme | L’étape des modèles affiche chaque usage sur une ligne, si bien que tous tiennent sur un écran
```

### Git

```ui-checklist
ok | Pull | Lorsque la branche locale et sa branche amont ont toutes deux avancé, le pull l’explique avec le nombre de commits de chaque côté au lieu d’afficher les indications de Git
ok | Paramètres de commit | Après l’enregistrement du modèle des messages de commit, la page n’indique plus de modifications non enregistrées
```
