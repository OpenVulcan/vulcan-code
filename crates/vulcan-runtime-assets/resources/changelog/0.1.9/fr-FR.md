Vulcan Code 0.1.9 corrige les longues sessions qui restaient plusieurs minutes sur une requête après l’envoi d’un message sans pouvoir être arrêtées, et corrige deux problèmes du bureau sous Linux et macOS.

### Passer à cette version

```ui-checklist
ok | Mise à jour incrémentielle | Depuis 0.1.8, seuls les fichiers modifiés sont téléchargés ; les versions plus anciennes s’installent toujours en mise à jour complète
ok | Installations par le programme d’installation | Les installations faites avec le programme d’installation se mettent à jour avec vulcan-code-setup update ; l’application l’indique au lieu de proposer un téléchargement
```

### Sessions

```ui-checklist
ok | Plus de blocages | Les longues sessions dont l’historique couvre plusieurs segments, par exemple avec beaucoup de sorties d’outils, n’attendent plus plusieurs minutes après l’envoi d’un message, et le bouton d’arrêt agit immédiatement
ok | Après un redémarrage | Après le redémarrage de l’application, les réponses précédentes de ces sessions sont toujours renvoyées au modèle dans son propre format, raisonnement compris, au lieu de texte brut
```

### Linux et macOS

```ui-checklist
ok | Déplacer la fenêtre | Sous Linux, la barre de titre déplace de nouveau la fenêtre dès qu’elle est restaurée depuis l’état maximisé
ok | Navigateur intégré | Sous Linux et macOS, le navigateur intégré installé fonctionne
```
