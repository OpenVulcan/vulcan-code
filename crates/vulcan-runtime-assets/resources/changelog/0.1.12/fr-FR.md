Vulcan Code 0.1.12 termine dans l’application les mises à jour qui demandent une réinstallation, et corrige les boutons de fenêtre et l’icône sous macOS ainsi que les requêtes de titre et de validation que certains fournisseurs refusaient.

### Mise à jour vers cette version

```ui-checklist
ok | Mise à jour incrémentielle | La mise à jour depuis 0.1.8 à 0.1.11 ne télécharge que les fichiers modifiés ; les versions plus anciennes s’installent toujours en mise à jour complète
ok | Réinstallation dans l’application | Les prochaines mises à jour qui demandent une réinstallation ne vous renvoient plus vers le site : seuls les fichiers modifiés sont téléchargés, les ressources de base installées comme le navigateur, Electron et Node sont réutilisées, et le programme d’installation termine la mise à jour au redémarrage
ok | Copies installées avec l’installateur Windows | Une copie installée avec l’installateur Windows reçoit ces mises à jour sous forme d’un petit paquet de mise à jour signé au lieu de l’installateur complet
```

### Interface

```ui-checklist
ok | Boutons de fenêtre sous macOS | Seuls les boutons de fenêtre du système restent : réduire, agrandir et fermer n’apparaissent plus en double, et le bouton vert propose de nouveau le menu de mosaïque et de plein écran du système
ok | Icône sous macOS | L’icône dans le Dock et lorsque l’application est fermée a la même taille que celle des autres applications
ok | Cartes des comptes de service de modèles | Les cartes ont une largeur minimale qui suit l’échelle de l’interface, si bien que les lignes de quota ne passent plus sur plusieurs lignes
ok | Assistant de configuration | Si vous êtes connecté à OpenVulcan et qu’aucun modèle d’évaluation n’est configuré, l’étape d’évaluation présélectionne le modèle d’évaluation cloud ; rien n’est enregistré avant votre confirmation
```

### Modèles et fournisseurs

```ui-checklist
ok | Requêtes corrigées | Les titres de session, la validation de modèle dans les paramètres et la rédaction de prompt des assistants dédiés envoient désormais une portée de cache, si bien qu’OpenCode Go ne renvoie plus 400 et que Codex Responses Lite ne les refuse plus
ok | Plateformes cloud | CommandCode, OpenCode Go, Open Design, OpenRouter et TokenRhythm sont classés comme plateformes cloud ; les informations de modèle partagées proviennent d’abord des fournisseurs officiels, puis des plateformes cloud, puis des relais
ok | Liste des modèles de base | Ce qu’un modèle sait faire et ses limites sont consignés une seule fois dans une liste des modèles de base dont héritent les règles des fournisseurs ; la page d’état des manifestes de fournisseurs signale les champs qui en sont hérités
```
