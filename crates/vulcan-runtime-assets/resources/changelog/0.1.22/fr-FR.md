Vulcan Code 0.1.22 ajoute la mémoire gratuite dans le cloud et améliore l’envoi d’images dans les longues conversations, les messages d’erreur et l’enregistrement vocal.

### Mémoire

```ui-checklist
ok | Mémoire gratuite dans le cloud | Connectez-vous à OpenVulcan sur un site compatible. Avant de l’activer, consultez les modèles, les régions de traitement et les modalités de gestion des données. L’usage est détaillé par étape.
ok | Reconstruction des vecteurs | Consultez les lots terminés et le temps d’attente en cas de limitation du débit. La reconstruction reprend après le délai.
ok | Mémoire Markdown | En cas de conflit, la révision provisoire du lot est distinguée de la dernière révision enregistrée.
```

### Conversations

```ui-checklist
ok | Nouvelles images dans une longue conversation | Si la compaction automatique est activée, les anciennes images peuvent être résumées par lots pour libérer de la place. Les nouvelles images sont envoyées dans la requête d’origine.
ok | Erreurs des fournisseurs | Les erreurs de quota de Bailian sont traitées comme des limites de débit. Les problèmes de solde et de réseau apparaissent dans des cartes d’erreur. Les avis ordinaires apparaissent comme des lignes d’état.
```

### Saisie vocale et outils

```ui-checklist
ok | Durée d’enregistrement | Les enregistrements qui atteignent la durée maximale peuvent être envoyés à la transcription.
ok | Statistiques RTK | Le classement des commandes utilise la base de statistiques RTK existante sans parcourir les journaux de conversation de Claude.
```
