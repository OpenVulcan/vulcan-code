Vulcan Code 0.1.28 ajoute des espaces de travail dédiés et un historique des appels pour les clients MCP externes. Au démarrage, il met à jour les compétences ROOT gérées dont la version est inférieure au minimum requis.

### MCP externe

```ui-checklist
ok | Projets et sessions dédiés | Les clients externes peuvent enregistrer leurs répertoires de travail et créer leurs propres sessions. Les outils de fichiers, CodeKit, shell et bases de connaissances utilisent l’espace associé à l’identifiant de session.
ok | Commandes en arrière-plan et terminaux | Avec les autorisations correspondantes, les clients peuvent exécuter des commandes en arrière-plan, consulter ou arrêter des tâches et accéder à des sessions de terminal.
ok | Historique facultatif | Activez la journalisation par client pour consulter les arguments, résultats et états. L’historique peut être filtré par client, projet ou session.
```

### Mise à niveau

```ui-checklist
ok | Les outils MCP autonomes requièrent un identifiant de session | Enregistrez un projet avec `vc_mcp_project`, créez une session avec `vc_mcp_session`, puis transmettez son `sid` aux outils autonomes. La session détermine le répertoire de travail.
```

### Compétences et vérifications de développement

```ui-checklist
ok | Mise à jour des compétences ROOT au démarrage | CodeKit, Vulcan Curl et Vulcan Work sont actualisés depuis la source configurée quand leur version est inférieure au minimum. En cas d’échec, la version utilisable est conservée et l’opération est retentée au prochain démarrage.
ok | Test d’acceptation HTTP des politiques VMM | `make test-vmm-http` vérifie les interactions HTTP entre les contrôles de politique de la WebUI et un service local.
```
