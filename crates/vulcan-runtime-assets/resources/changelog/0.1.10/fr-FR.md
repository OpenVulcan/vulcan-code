Vulcan Code 0.1.10 corrige la modification et la bifurcation dans les longues sessions, poursuit les tours malgré les limites de débit et les arguments d’outil mal formés, et améliore les cartes de commande et les composants de réponse.

### Mise à jour vers cette version

```ui-checklist
ok | Mise à jour incrémentielle | La mise à jour depuis 0.1.8 ou 0.1.9 ne télécharge que les fichiers modifiés ; les versions plus anciennes s’installent toujours en mise à jour complète
ok | Installations par le programme d’installation | Les installations faites avec le programme d’installation se mettent à jour avec vulcan-code-setup update ; l’application l’indique au lieu de proposer un téléchargement
```

### Sessions

```ui-checklist
ok | Modification et bifurcation | Dans les longues sessions dont l’historique couvre plusieurs segments, modifier un message envoyé ou bifurquer depuis un tour n’échoue plus avec l’erreur provider_source fingerprint mismatch
ok | Arguments d’outil mal formés | Un appel d’outil aux arguments illisibles reçoit un résultat d’erreur et le tour continue au lieu de s’arrêter à la deuxième erreur ; des arguments tronqués sont signalés comme probablement trop longs
ok | Limites de débit | Les limites HTTP 429 sont réessayées selon le calendrier des limites de débit lorsque « Attendre et réessayer en cas de limite de débit » est activé au lieu de terminer le tour ; un solde épuisé arrête toujours
```

### Outils et réponses

```ui-checklist
ok | Titres de commande | L’argument de description de shell_command devient un titre court de 80 caractères au plus, ce qui évite à certains modèles comme MiniMax-M3.1-Flash de casser l’appel d’outil à cet endroit
ok | Cartes de tâches en arrière-plan | Les cartes shell_job affichent l’objet de la commande après l’identifiant de la tâche, pour savoir quelle commande est attendue ; le client terminal titre ainsi les lignes de commande et de tâche
ok | Composants de réponse | Une troisième cellule dans une ligne ui-kv s’affiche comme note sous la valeur, et une variation signée comme +5 dans ui-metrics comme une variation, au lieu que tout le composant s’affiche en code
```

### Modèles et mémoire

```ui-checklist
ok | OpenCode Go DeepSeek | deepseek-v4-pro et deepseek-v4.1-flash peuvent désactiver la réflexion et peuvent donc servir de modèles de mémoire
ok | Note sur le modèle de mémoire | Les réglages expliquent désormais pourquoi seuls les modèles pouvant désactiver la réflexion sont proposés : le modèle de mémoire est appelé avant et après chaque réponse, la réflexion ajouterait à chaque fois de l’attente et de la consommation
```
