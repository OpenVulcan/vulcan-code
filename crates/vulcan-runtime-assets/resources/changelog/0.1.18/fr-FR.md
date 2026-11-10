Vulcan Code 0.1.18 corrige des erreurs de compression du contexte et améliore les raccourcis d’envoi, la fermeture du navigateur et l’estimation des coûts.

### Contexte et conversations

```ui-checklist
ok | Compression du contexte | La compression manuelle utilise le modèle sélectionné. Un solde insuffisant ou un échec d’authentification arrête les nouvelles tentatives automatiques. Si le modèle de résumé demande un outil, son aperçu reste associé à la bonne requête de compression.
ok | Historique des tentatives | Les erreurs des nouvelles tentatives automatiques d’une même requête sont regroupées dans une carte qui permet de les parcourir.
ok | Expiration de la mémoire | La conversation signale quand un contexte de mémoire ajouté précédemment a expiré.
```

### Commandes et réglages

```ui-checklist
ok | Raccourcis pour mettre en file ou insérer | Pendant une tâche, vous pouvez choisir si Entrée met le message en file ou l’insère. Ctrl+Entrée effectue l’autre action. Quand la conversation est inactive, le message est envoyé immédiatement.
ok | Approbation par défaut et raisonnement | Définissez le mode d’approbation des nouvelles conversations. Si vous revenez à un modèle déjà utilisé, son niveau de raisonnement enregistré est rétabli.
ok | Fermeture du navigateur local | Fermer le panneau du navigateur dans l’espace de travail ferme immédiatement les pages de la conversation en cours et libère leurs ressources.
```

### Fournisseurs et utilisation

```ui-checklist
ok | Avertissements sur les fournisseurs | NodeAI apparaît dans la liste une fois configuré. Les fournisseurs signalés comme instables affichent un avertissement.
ok | Filtrer les requêtes par modèle | La liste des requêtes récentes des statistiques d’utilisation peut être filtrée par modèle.
ok | Tarifs par palier et par fournisseur | L’estimation des coûts peut appliquer des tarifs par palier de jetons. Il est aussi possible de définir un tarif de modèle propre à un fournisseur.
```

### Notes de version

```ui-checklist
ok | Notes de version | Les notes de 0.1.14 à 0.1.17 sont à nouveau regroupées en listes pour faciliter leur lecture.
```
