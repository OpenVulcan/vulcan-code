Vulcan Code 0.1.26 permet de réassocier le dossier d’un projet sans perdre ses sessions et améliore les mises à jour de l’application ainsi que l’affichage des résultats des outils.

### Projets et sessions

```ui-checklist
ok | Récupérer le dossier d’un projet | Si le dossier est déplacé ou le disque déconnecté, vérifiez à nouveau l’emplacement d’origine ou choisissez un autre dossier en conservant l’historique des sessions. Le changement apparaît dans le fil de la conversation. La prochaine requête au modèle utilise le nouveau dossier.
ok | Parcourir les volumes montés et les chemins saisis | Le sélecteur de dossiers affiche les volumes montés sous Linux et macOS. Après avoir saisi un chemin, vous pouvez ouvrir le dossier avant de confirmer votre choix.
ok | Afficher le dernier message en changeant de conversation | Le passage à une autre conversation ouvre le message le plus récent pour reprendre la lecture.
```

### Installation et mises à jour

```ui-checklist
ok | Préparer la mise à jour avant de redémarrer | Le téléchargement automatique est activé par défaut. L’application signale qu’une mise à jour est prête une fois téléchargée et vérifiée, puis change de version quand vous confirmez le redémarrage. Vous pouvez aussi désactiver le téléchargement automatique et lancer les mises à jour manuellement.
ok | Installateur autonome 0.2.17 | Windows dispose d’un installateur graphique signé, Linux de paquets AppImage et macOS d’un DMG. Après une installation depuis le terminal sous macOS, `vcode manager gui` ouvre le gestionnaire graphique. Lors de la première installation, LuaSkills choisit le Hub international ou celui de Chine continentale selon le circuit d’installation.
```

### Outils de navigation et de traitement de documents

```ui-checklist
ok | Cartes des actions du navigateur | Les cartes Playwright affichent l’action, la cible et le résultat pour la navigation, les clics, les captures d’écran et d’autres opérations. La compétence Vulcan Browser incluse guide aussi l’exécution et la vérification des actions.
ok | Outils documentaires Vulcan Work | Vulcan Code installe Vulcan Work depuis le Hub LuaSkills sélectionné pour traiter les fichiers PDF, Word, Excel et PowerPoint. Les cartes dédiées affichent selon l’opération les fichiers, les plages de cellules ou les résultats. L’aide des outils LuaSkills enregistrés implicitement affiche les paramètres acceptés par chacun.
```
