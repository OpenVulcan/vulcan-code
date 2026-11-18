Vulcan Code 0.1.27 permet d’inspecter le contexte des requêtes, facilite l’utilisation à la demande des compétences et des extensions et améliore la coordination des sous-agents et les autorisations des clients MCP.

### Conversations et espace de travail

```ui-checklist
ok | Inspecter le contexte d’une requête | Ouvrez le contexte depuis le menu de la conversation pour examiner les instructions, les messages et les définitions d’outils de la dernière requête. Avant l’envoi de la première requête, l’inspecteur présente un aperçu du contexte qui serait assemblé.
ok | Poursuivre la tâche principale | La conversation parente peut traiter les messages prioritaires et les résultats reçus pendant l’exécution des sous-agents. Les résultats sont repris dans les requêtes suivantes.
ok | Position de lecture et temps écoulé | Les mises à jour en continu préservent la position lorsque vous relisez l’historique. À la réouverture, le temps écoulé d’une requête active est rétabli à partir des événements de Core.
```

### Compétences, extensions et MCP

```ui-checklist
ok | Mentionner des compétences et des extensions | Saisissez `@` pour choisir une compétence ou un chemin du projet et `#` pour choisir une extension. Les suggestions affichent des noms courts.
ok | Charger les outils à la demande | Vulcan Browser, Context7 et la gestion des connexions MCP de la conversation sont accessibles à la demande. LuaSkills 0.9.1 permet de passer à ce mode pour les outils de paquets accompagnés d’un guide.
ok | Attribuer les autorisations MCP par groupe | Configurez séparément les autorisations de conversation, d’outils autonomes et de fonctions multimodales pour chaque client. Autorisez individuellement les outils de fichiers, de shell, de CodeKit et de bases de connaissances, avec des limites par projet ou bibliothèque.
```

### Installation, modèles et retours

```ui-checklist
ok | Programme d’installation autonome 0.2.18 | À la première installation, le programme mesure les temps de réponse et recommande le chemin de téléchargement le plus rapide. Vous pouvez le modifier. LuaSkills utilise le Hub et la source de téléchargement GitHub de la région choisie.
ok | Diagnostic de démarrage de LuaSkills | L’écran de démarrage affiche la progression de la préparation des dépendances dans la langue choisie et identifie celles dont l’installation a échoué.
ok | Qwen4 Max | La version stable de Qwen4 Max figure désormais parmi les modèles d’Alibaba Cloud Token Plan.
ok | Coordonnées facultatives pour les retours | Vous pouvez joindre et vérifier vos coordonnées avant d’envoyer un retour. Les réponses publiques sont accessibles dans le centre de messages de l’application.
```
