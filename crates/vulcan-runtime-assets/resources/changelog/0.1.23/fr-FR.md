Vulcan Code 0.1.23 ajoute le nettoyage protégé de l’historique, améliore le navigateur intégré et corrige le chargement des modules Lua fournis sur macOS.

### Conversations

```ui-checklist
ok | Nettoyage rapide de l’historique | Dans une conversation ordinaire inactive après compaction, choisissez la limite du nettoyage depuis le menu d’ajout ou avec `/clean-history`. Les anciens détails d’outils et de raisonnement sont supprimés. Les questions et réponses simplifiées restent dans la chronologie, et l’historique nécessaire à la conversation est protégé.
ok | Défilement des longues conversations | Le Markdown en cours de génération se met à jour indépendamment. La position de lecture est conservée au chargement des anciens tours, y compris dans l’historique nettoyé.
```

### Navigateur intégré

```ui-checklist
ok | Rendu des pages | WebUI affiche les pages à partir de leur structure dans le navigateur intégré, en remplacement du flux de captures d’écran.
ok | Dimensions de la zone d’affichage | Choisissez un préréglage bureau, portrait ou paysage, saisissez des dimensions personnalisées ou ajustez la zone à la fenêtre actuelle en une seule action.
```

### Espace de travail et macOS

```ui-checklist
ok | Base de connaissances | Les nouvelles barres latérales épinglent la base de connaissances par défaut. La disposition enregistrée reste prioritaire.
ok | Environnement Lua sur macOS | Le chargement des modules Lua natifs fournis est corrigé sur les Mac Intel et Apple Silicon. Context7 initialise son stockage local à la première ouverture de ses réglages.
```
