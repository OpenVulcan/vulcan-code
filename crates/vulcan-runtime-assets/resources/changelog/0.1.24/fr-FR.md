Vulcan Code 0.1.24 améliore les pièces jointes et la reprise lors d’un changement de modèle, et corrige le navigateur intégré ainsi que les journaux du terminal.

### Conversations et saisie

```ui-checklist
ok | Glisser et coller des fichiers | L’aperçu des pièces jointes est rétabli. Un fichier que le modèle ne peut pas téléverser peut être envoyé comme référence si son chemin d’origine est accessible. Les références envoyées se déplient à part du message.
ok | Changement de modèle et reprise | L’historique d’origine est conservé. Si le raisonnement ne peut pas être rejoué de façon sûre, vous pouvez copier les messages visibles dans le brouillon d’une nouvelle conversation et les relire avant l’envoi.
ok | Compaction et défilement | La compaction en cours s’affiche séparément. Le nouveau texte reste visible quand vous suivez la fin de la conversation.
```

### Navigateur intégré

```ui-checklist
ok | Affichage et édition | Le chargement des feuilles de style et des ressources des fenêtres contextuelles est corrigé, et l’édition est améliorée. Les mots de passe restent masqués dans le navigateur intégré lorsque la page change leur visibilité.
```

### Journaux et fournisseurs

```ui-checklist
ok | Journaux du terminal | Le masquage des secrets couvre mieux les sauts de ligne et les sorties tronquées. Vous pouvez toujours arrêter une tâche en arrière-plan si le masquage est indisponible.
ok | Erreurs du fournisseur | Les cartes conservent les messages lisibles et les liens d’aide tout en masquant les URL contenant des identifiants.
ok | Compatibilité avec les fournisseurs | Token Plan d’Alibaba Cloud Model Studio ne tente plus de téléversements temporaires vers OSS. Les requêtes des bases de connaissances utilisant les embeddings OpenVulcan sont corrigées.
```
