Vulcan Code 0.1.0 est la première préversion publique : un atelier graphique d’IA pour écrire du code et pour les tâches de travail quotidiennes. Projets, sessions, terminaux, navigateur, revue de code et commits Git sont réunis au même endroit.

```ui-kv
Version | Préversion
Clients | Application de bureau, WebUI dans le navigateur, client terminal (TUI)
Langues de l’interface | 11
Paquets | Python, Node.js et composants de navigateur inclus ; installation en une seule commande
Licence | MIT
```

### Points forts

````ui-tabs
### Fenêtre de compression
Chaque session peut changer de fenêtre de compression à tout moment depuis le cadran de capacité du contexte dans la zone de saisie : une tâche longue n’exige jamais de nouvelle session.

```ui-kv
Tailles | Cinq paliers, par défaut 200K, 256K, 272K, 373K et 500K tokens, modifiables dans les Paramètres
Suivre le modèle | Choisissez « Par défaut (selon le modèle) » pour compresser selon la fenêtre du modèle actuel
Portée | Ne change que le moment de la compression automatique, pas la fenêtre de contexte propre au modèle
Compression manuelle | Lancez /compact sur la session actuelle à tout moment
```

### Cartes d’outils et repli
Chaque type d’outil a sa propre carte : même de nombreux appels d’outils n’enfouissent jamais la réponse.

```ui-checklist
ok | Cartes dédiées | Fichiers, commandes, terminaux, sous-agents, recherche, médias, questions, variables d’environnement, modifications de configuration et plus encore
ok | Durée et liens | Chaque appel indique sa durée ; les fichiers d’une carte s’ouvrent directement dans le panneau des fichiers
ok | Repli à la demande | Ouvertes pendant l’exécution et repliées à la fin ; les appels consécutifs se regroupent sous un seul résumé
ok | Basculer à tout moment | Choisissez le comportement par défaut dans les Paramètres, ou basculez par session avec « Repli des outils » dans la zone de saisie ou /collapse
```

### Multimodal
Dans « Extensions multimodales », chaque capacité reçoit un ou plusieurs modèles ; « Capacité étendue » les active par session.

```ui-cards
Web | Recherche web et extraction de pages
Compréhension | Compréhension d’images, de vidéos et d’audio
Génération | Génération d’images et de vidéos ; les images peuvent aussi utiliser un abonnement Codex
Voix et musique | Synthèse et transcription vocales, génération musicale et reprises
```

### Variables d’environnement
La gestion intégrée des variables d’environnement garde les secrets sur votre machine ; l’IA ne voit que les noms des variables.

```ui-checklist
ok | Trois niveaux | Niveaux global, projet et session modifiés dans l’interface ; la page peut être affichée dans la barre latérale
ok | Valeurs masquées | Les valeurs ne sont jamais montrées à l’IA ni à l’interface, et celles qui apparaissent dans les résultats d’outils sont masquées
ok | Saisie à la demande | Quand il manque une variable à l’IA, elle vous la demande dans un formulaire, et la valeur ne transite jamais par la conversation
ok | Injection automatique | Les commandes et terminaux reçoivent les variables de leur niveau ; les serveurs MCP utilisent les variables globales
```

### Modèles des sous-agents
Les sous-agents n’utilisent que les modèles que vous autorisez.

```ui-kv
Deux niveaux | Le réglage système sert de valeur par défaut ; chaque session peut définir le sien
Coché par défaut | Le modèle appelant est inclus par défaut et peut être décoché
Hors périmètre | Refusé d’emblée, jamais remplacé en silence par un autre modèle
Paramètres de la session | Modifiez-le à tout moment dans « Paramètres de la session », sous la zone de saisie, ou revenez à la valeur par défaut du système en un clic
```
````

### Atelier

```ui-cards
Projets et sessions | De nombreux projets et sessions côte à côte, dans l’ordre de votre choix, avec arborescence de fichiers, revue, mémoire et panneaux de sous-agents
Terminaux de projet | Terminaux à onglets ; l’IA peut conserver des sessions de commande longues ou interactives
Panneau des fichiers | Mentionnez des fichiers avec @, glissez-les dans la zone de saisie, ouvrez des chemins depuis les terminaux et les sorties, recherchez et allez à une ligne
Réponses enrichies | Markdown, code et diagrammes Mermaid en streaming ; les réponses interactives ajoutent listes de contrôle, comparaisons, graphiques, flux et plus encore, et les cartes de décision insèrent ou envoient votre réponse
Bifurquer et réessayer | Modifiez une question précédente pour régénérer la réponse, ou dupliquez une nouvelle session à partir de n’importe quel tour terminé
Palette de commandes | Raccourcis clavier, recherche dans la conversation et copie d’une conversation en Markdown
Assistants dédiés | Sessions au prompt de base fixe et aux réglages figés, consacrées à une tâche récurrente
```

### Faire avancer le travail

```ui-cards
Mode objectif | Fixez un objectif et un budget de tokens ; l’IA poursuit le travail tour après tour, et l’objectif peut être mis en pause, repris ou supprimé
Mode plan | Explorez et rédigez d’abord un plan, puis exécutez-le une fois approuvé
Orientation en cours | Ajoutez des consignes ou mettez des demandes de suite en file d’attente pendant l’exécution d’une tâche
Sous-agents | Rôles intégrés d’exploration et de planification, plus des modèles de rôle facultatifs
Questions | Quand l’IA a besoin d’une décision, elle pose la question avec des cartes d’options dans la session
```

### Code, connaissances et mémoire

```ui-cards
Navigation dans le code | CodeKit lit l’arbre de travail en direct : symboles, définitions et références
Base de connaissances | Des bibliothèques que l’IA tient à jour pour vous, avec un historique permanent et un accès par projet
Mémoire | Mémoire Markdown dans votre dossier utilisateur, ou mémoire à long terme MemoryMesh rappelée avant chaque tour et affichée dans le déroulé du travail
```

### Sécurité et revue

```ui-checklist
ok | Modes d’autorisation | Lecture seule, écriture dans l’espace de travail, approbation automatique et accès complet
ok | Chaîne d’approbation | Règles, modèle d’évaluation et modèle réviseur ; tout ce qui est incertain vous revient
ok | Modèle d’évaluation | S’exécute sur un modèle local, dans le cloud ou via une API
ok | Revue des diffs | Vues unifiée et scindée, changements au niveau du mot, indexation et annulation
ok | Git | Pull, messages de commit rédigés par l’IA, commit sur la branche actuelle ou une nouvelle, puis push
```

### Étendre Vulcan Code

```ui-cards
Compétences | Compétences de prompt (SKILL.md) et paquets LuaSkills programmables, installables depuis LuaSkills Hub
MCP | Connectez des serveurs MCP locaux et HTTP, ou laissez l’IA en connecter un le temps d’une session
Service MCP | D’autres outils d’IA peuvent lister les sessions, envoyer des messages et répondre aux questions, chacun avec son propre jeton
Configuration IA | L’IA peut ajouter des serveurs MCP et des fournisseurs de modèles pour vous, en montrant d’abord un aperçu et en vous consultant selon votre mode d’autorisation
Plugins | Les plugins ajoutent des outils et leurs propres pages, comme le navigateur IA et les statistiques d’utilisation
RTK | Compresse la sortie des commandes et le contexte Git pour économiser des tokens, et comptabilise les économies
Modèles | OpenAI, Anthropic, Gemini et d’autres familles de protocoles, fournisseurs intégrés et personnalisés, avec un modèle distinct par usage
```

```ui-callout info Préversion
La 0.1.x reste une préversion : les fonctionnalités et l’interface continueront d’évoluer. Lors d’une mise à niveau, la base de données de l’application et les données de session sont d’abord sauvegardées, puis mises à niveau automatiquement version par version, sans aucune action de votre part.
```
