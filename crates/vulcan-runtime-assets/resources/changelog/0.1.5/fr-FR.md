Vulcan Code 0.1.5 donne à chaque modèle sa propre fenêtre de compaction par défaut, démarre l'application de bureau dans une seule fenêtre et vous laisse choisir la fréquence de recherche des mises à jour.

### Fenêtre de contexte

```ui-checklist
ok | Valeur par défaut par modèle | Chaque modèle de conversation activé peut avoir sa propre fenêtre de compaction par défaut pour les nouvelles sessions : la valeur du système, suivre le modèle, un palier fixe ou une taille personnalisée ; les sessions existantes ne changent pas
ok | Fenêtres personnalisées | Une taille personnalisée accepte un nombre de tokens ou K pour mille (par exemple 235K), au moins 100K et au plus la fenêtre du modèle ; le sélecteur de fenêtre à côté de la zone de saisie peut aussi passer à une taille personnalisée
ok | Changer de modèle | Changer de modèle dans une session propose la fenêtre par défaut du modèle cible et rien ne change avant votre confirmation ; une fenêtre que le modèle cible ne peut pas contenir suit désormais le modèle au lieu de bloquer le changement
ok | Interface terminal | Les paramètres de la TUI permettent aussi de définir la fenêtre par défaut de chaque modèle
```

### Mises à jour

```ui-checklist
ok | Fréquence de recherche | Choisissez la fréquence de recherche automatique des mises à jour : toutes les heures, toutes les 6 heures, toutes les 12 heures ou chaque jour, toutes les 6 heures par défaut ; un changement s'applique sans redémarrage
ok | Page Mise à jour du logiciel | La page Mise à jour du logiciel des paramètres est réorganisée : d'abord la version actuelle avec rechercher et réparer, puis les paramètres de mise à jour, et enfin la mise à jour trouvée avec les notes de chaque version qu'elle apporte, de la plus récente à la plus ancienne ; la boîte de dialogue de mise à jour affiche la même liste de versions
ok | Redémarrage sous Linux | Sous Linux, un Core lancé depuis l'application de bureau redémarre après une mise à jour au lieu de rester sur « Redémarrage »
```

### Application de bureau

```ui-checklist
ok | Démarrage dans une seule fenêtre | Aucune fenêtre de démarrage séparée ne s'ouvre ; la fenêtre de travail affiche l'écran de démarrage avec les étapes réelles et les éventuels échecs, puis ouvre l'interface sur place dès que Core est prêt
ok | Apparence au démarrage | L'écran de démarrage reprend la langue et l'apparence de la dernière utilisation ; le premier démarrage suit le système
ok | Icône de l'application | Le dock Linux et le Dock macOS affichent l'icône de Vulcan Code au lieu d'une icône générique ; rouvrir l'application ramène au premier plan la fenêtre en cours de démarrage
```

### Corrections

```ui-checklist
ok | Modèles d'embeddings | Les modèles d'embeddings à taille de sortie réglable (comme Qwen3-Embedding-8B, de 32 à 4096 dimensions) ne rejettent plus toutes les réponses
ok | Composants de réponse | Les encadrés sans tonalité et les lignes de constats à quatre cellules s'affichent désormais au lieu de redevenir des blocs de code, dans la WebUI comme dans la TUI
```

### Programme d'installation

```ui-checklist
ok | Installation en une ligne sous macOS | Le script d'installation lancé via un tube curl sous macOS peut désormais lire le clavier
ok | Application macOS | L'application macOS créée par le programme d'installation a une icône, et celui-ci indique où elle se trouve à la fin
ok | Entrée de menu Linux | L'entrée de menu Linux correspond à la fenêtre en cours d'exécution, si bien qu'une icône épinglée au dock ouvre l'application
```
