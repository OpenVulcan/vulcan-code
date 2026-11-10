Vulcan Code 0.1.15 corrige le compactage pour conserver l’entrée complète et permet de reprendre les tours inachevés admissibles après un compactage manuel.

### Corrections du compactage du contexte

```ui-checklist
ok | Entrée complète lors du compactage | La requête de résumé conserve l’intégralité de l’historique, les résultats d’outils et les médias natifs. Si l’entrée complète dépasse la limite de contexte du modèle, le compactage signale l’erreur et conserve l’historique.
ok | Compactage manuel des tours inachevés | Les tours inachevés admissibles peuvent être compactés manuellement lorsqu’aucune requête n’est active. La tâche d’origine et les entrées en file d’attente sont conservées pour permettre de reprendre la tâche ensuite.
ok | Messages en attente après le compactage | Les messages en attente que vous choisissez d’insérer pendant le compactage manuel sont envoyés dans l’ordre des clics une fois le compactage réussi. Si le compactage échoue, s’arrête ou si le processus redémarre, ils restent en file d’attente.
```

### Autres améliorations

```ui-checklist
ok | Raisonnement des sous-agents et file d’attente | Les sous-agents peuvent utiliser un niveau de raisonnement pris en charge par leur modèle cible. Leurs résultats, les entrées en file d’attente et le panneau des messages en attente restent synchronisés.
ok | Utilisation par heure | Les statistiques conservent la période sélectionnée et affichent l’utilisation par heure pour les périodes courtes.
ok | Corrections du bureau et de l’interface | Revenir en arrière dans l’application de bureau laisse l’écran de démarrage fermé. Les invites de fichiers sur le Web s’adaptent à la largeur de la fenêtre, et les clients web et terminal affichent les résultats ok comme des vérifications réussies.
```
