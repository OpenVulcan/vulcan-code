Vulcan Code 0.1.1 ajoute les mises à jour dans l'application, un nouveau mode d'installation et un service utilisateur facultatif.

### Mises à jour dans l'application

```ui-checklist
ok | Vérification et téléchargement | Le service recherche les nouvelles versions, ne télécharge que les fichiers modifiés et ne bascule qu'après la vérification des signatures
ok | Redémarrage après confirmation | L'application de bureau, la WebUI du navigateur et la TUI affichent les mises à jour disponibles et redémarrent sur la nouvelle version une fois confirmé
ok | Retour automatique | Si une nouvelle version ne démarre pas, le lanceur revient à la précédente et l'application vous le signale
ok | Mises à jour complètes | Lorsqu'une installation complète est nécessaire, l'application télécharge et lance le programme d'installation, puis se rouvre à la fin
```

### Installation et service

```ui-cards
Programme d'installation | Le nouveau vulcan-code-setup installe par utilisateur sous Windows, macOS et Linux, avec en option l'application de bureau, le client terminal, les entrées de menu et un raccourci sur le bureau
Service utilisateur | Le service peut démarrer à l'ouverture de session : entrée de démarrage Windows, LaunchAgent macOS ou service utilisateur systemd sous Linux
Plage de ports | Les ports utilisés par le service se modifient dans les paramètres avancés du programme d'installation
Client terminal | Si aucun service ne tourne, la TUI démarre le service installé, comme l'application de bureau
```

```ui-callout info Mise à niveau depuis 0.1.0
0.1.0 n'a pas encore de mises à jour dans l'application : relancez le programme d'installation ou la commande d'installation pour passer à 0.1.1 ; les versions suivantes se mettent à jour dans l'application.
```
