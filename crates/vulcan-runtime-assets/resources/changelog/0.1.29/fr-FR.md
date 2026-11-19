Vulcan Code 0.1.29 maintient la connexion pendant les appels MCP externes de longue durée et actualise le catalogue de modèles OpenAI.

### MCP externe

```ui-checklist
ok | Maintien de la connexion pendant les appels longs | Si le client accepte SSE, un appel long envoie régulièrement des signaux pour limiter les déconnexions dues à l’inactivité. La même requête reçoit le résultat final à la fin de l’appel.
ok | Commandes longues en arrière-plan | Les délais fixés par le client restent applicables. Si une requête Shell au premier plan se déconnecte, le service arrête son arborescence de processus. Lancez les commandes longues avec `background: true`, puis consultez le résultat avec `shell_job` et le même `sid`. L’autorisation `shell_job` est requise.
ok | Diagnostic plus clair des jetons | Le journal du service distingue les en-têtes d’authentification manquants ou mal formés des jetons invalides et consigne les rotations manuelles. Il conserve uniquement la catégorie du diagnostic ou l’identifiant du client.
```

### Modèles

```ui-checklist
ok | GPT-6.2 Sol et Astra | Les deux modèles sont disponibles via l’API OpenAI et les abonnements Codex. Les abonnements Codex utilisent la fenêtre de contexte indiquée dans le catalogue de modèles.
ok | GPT Image 2.6 | Flare et Sunburst sont désormais disponibles dans le sélecteur multimédia OpenAI.
```
