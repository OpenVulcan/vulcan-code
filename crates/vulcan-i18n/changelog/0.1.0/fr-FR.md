Vulcan Code 0.1.0 est la première préversion publique : un atelier graphique de programmation avec l’IA qui réunit au même endroit projets, conversations, terminaux, navigateur, revue de code et commits Git.

```ui-kv
Version | Préversion
Clients | Application de bureau, WebUI dans le navigateur, client terminal (TUI)
Langues de l’interface | 11
Licence | MIT
```

### Atelier

```ui-cards
Projets et conversations | Plusieurs projets et conversations côte à côte, avec arborescence de fichiers, revue, mémoire et panneaux de sous-agents
Terminaux de projet | Terminaux à onglets ; l’IA peut conserver des sessions de commande longues ou interactives
Réponses enrichies | Markdown, code, diagrammes Mermaid et cartes d’outils en streaming ; les réponses interactives ajoutent des composants visuels comme constats, comparaisons, graphiques et flux
Bifurquer et réessayer | Modifiez une question précédente pour régénérer la réponse, ou créez une nouvelle conversation à partir de n’importe quel tour terminé
Assistants dédiés | Conversations au prompt de base fixe et aux réglages figés, consacrées à une tâche récurrente
```

### Faire avancer le travail

```ui-cards
Mode objectif | Fixez un objectif et un budget de Token ; l’IA poursuit le travail tour après tour et peut être mise en pause, reprise ou réinitialisée
Mode plan | Explorez et rédigez d’abord un plan, puis exécutez-le une fois approuvé
Guidage en cours | Ajoutez des consignes ou mettez des demandes en file pendant l’exécution d’une tâche
Sous-agents | Rôles intégrés d’exploration et de planification, plus des modèles de rôle facultatifs, chacun limité aux modèles que vous autorisez
Questions | Quand l’IA a besoin d’une décision, elle pose la question avec des cartes d’options dans la conversation
```

### Code, connaissances et mémoire

```ui-cards
Navigation dans le code | CodeKit lit l’arbre de travail en direct : symboles, définitions et références
Base de connaissances | Des bibliothèques que l’IA tient à jour pour vous, avec un historique permanent et un accès par projet
Mémoire | Mémoire Markdown dans votre dossier utilisateur, ou mémoire à long terme MemoryMesh rappelée avant chaque tour
```

### Sécurité et revue

```ui-checklist
ok | Modes d’autorisation | Lecture seule, écriture dans l’espace de travail, approbation automatique et accès complet
ok | Chaîne d’approbation | Règles, modèle de décision et modèle réviseur ; tout ce qui est incertain vous revient
ok | Revue des Diff | Vues unifiée et côte à côte, changements au niveau du mot, indexation et annulation
ok | Commits Git | Messages rédigés par l’IA, commit sur la branche actuelle ou une nouvelle, puis push
ok | Secrets | Les valeurs des variables d’environnement ne sont jamais montrées à l’IA ni à l’interface
```

### Étendre Vulcan Code

```ui-cards
Compétences | Compétences de prompt (`SKILL.md`) et paquets LuaSkills programmables
MCP | Connectez des serveurs MCP locaux et HTTP, ou laissez l’IA en connecter un le temps d’une conversation
Service MCP | D’autres outils d’IA peuvent lister les conversations, envoyer des messages et répondre aux questions, chacun avec son propre Token
Extensions | Les extensions ajoutent des outils et leurs propres pages, comme le navigateur IA et les statistiques d’utilisation
Modèles | OpenAI, Anthropic, Gemini et d’autres familles de protocoles, fournisseurs intégrés et personnalisés, avec un modèle distinct par usage
```

```ui-callout info Préversion
Les formats de données enregistrées peuvent encore changer entre les versions 0.1.x. Les mises à niveau qui migrent vos données étape par étape commencent avec la 0.2.
```
