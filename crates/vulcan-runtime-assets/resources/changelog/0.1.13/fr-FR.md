Vulcan Code 0.1.13 corrige le navigateur intégré sur les Mac Intel, les notifications qui ne se fermaient plus d'elles-mêmes et les installations d'environnements d'exécution qui téléchargeaient à nouveau les mêmes fichiers.

### Mise à jour vers cette version

```ui-checklist
ok | Mise à jour incrémentielle | Depuis les versions 0.1.8 à 0.1.12, seuls les fichiers modifiés sont téléchargés
```

### Interface

```ui-checklist
ok | Les notifications se ferment de nouveau d'elles-mêmes | Après la fermeture d'une notification avec son propre bouton de fermeture ou d'action, toutes les notifications suivantes restaient affichées jusqu'à leur fermeture manuelle ; elles disparaissent de nouveau d'elles-mêmes
ok | Navigateur intégré sur Mac Intel | La version macOS Intel (x64) de 0.1.12 contenait le navigateur pour puces Apple, qui ne fonctionne pas sur les Mac Intel ; cette version fournit le navigateur Intel, et chaque version macOS est désormais vérifiée afin que tous ses programmes correspondent à son processeur
```

### Environnements d'exécution

```ui-checklist
ok | Téléchargements réutilisés | L'installation ou la réinstallation de uv, Node.js ou pnpm réutilise une archive déjà téléchargée tant qu'elle correspond à sa somme de contrôle, au lieu de la télécharger à nouveau
```
