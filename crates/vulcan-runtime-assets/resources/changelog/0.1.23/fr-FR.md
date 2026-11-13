Vulcan Code 0.1.23 ajoute le nettoyage protégé de l’historique, améliore le navigateur intégré et corrige le chargement des modules Lua fournis sous Linux et macOS.

### Mise à jour vers cette version

```ui-checklist
ok | Programme d’installation 0.2.12 | Le script réutilise le programme d’installation existant, qui se met à jour et redémarre automatiquement. Sous Linux et macOS, l’ajout au PATH rend les commandes disponibles dans les nouveaux terminaux. L’installation en ligne de commande affiche les commandes du service et les ports de la WebUI.
```

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

### Espace de travail et environnement d’exécution

```ui-checklist
ok | Base de connaissances | Les nouvelles barres latérales épinglent la base de connaissances par défaut. La disposition enregistrée reste prioritaire.
ok | Environnement Lua sur macOS | Le chargement des modules Lua natifs fournis est corrigé sur les Mac Intel et Apple Silicon. Context7 initialise son stockage local à la première ouverture de ses réglages.
ok | Environnement Lua sous Linux | Les modules Lua natifs fournis, comme OpenSSL, se chargent désormais sur ARM64 et x64. La création des paquets vérifie la compatibilité avec glibc 2.35.
```
