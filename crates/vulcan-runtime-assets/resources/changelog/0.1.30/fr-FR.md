Vulcan Code 0.1.30 ajoute le mode Économie, la connexion au forfait ChatGPT et le transfert de configuration. MCP, le navigateur intégré et la reprise des messages en attente ont aussi été améliorés.

### Sessions et contexte

```ui-checklist
ok | Mode Économie par session | Définissez une fenêtre d’historique pour les sessions ordinaires. Par défaut, à partir de 40 tours terminés, les 20 plus anciens tours admissibles peuvent être retirés des futures requêtes. L’historique complet reste consultable. L’activation sur une ancienne session peut entraîner une requête supplémentaire au modèle.
ok | Envoyer un message en attente après un échec | Une fois le tour en échec terminé, envoyez le message sélectionné avec ses pièces jointes et ses paramètres d’origine. L’inspecteur indique les résumés, l’historique conservé et l’expiration de la mémoire.
```

### Comptes et modèles

```ui-checklist
ok | Connexions OpenAI distinctes | Connectez séparément des comptes OpenAI API, Codex et forfait ChatGPT. La connexion Codex et la consultation des quotas fonctionnent à nouveau via HTTP. Les pièces jointes et certaines réponses en flux du forfait ChatGPT ont été corrigées.
ok | Transfert sélectif de configuration | Exportez les identifiants et fournisseurs personnalisés de votre choix dans un fichier `.vc_mcfg` protégé par mot de passe. Les comptes à jetons rotatifs exigent une reconnexion sur l’appareil de destination.
ok | Consommation et retouche d’images | Consultez la consommation du Step Plan de StepFun dans les deux régions. Retouchez des images via l’API OpenAI avec GPT Image 2 ou 2.5, ou utilisez des images de référence avec GPT Image 2 via Codex.
```

### Outils et espace de travail

```ui-checklist
ok | Raccourcis MCP et LuaSkills par projet | Limitez les serveurs MCP à certains projets et utilisez-les à la demande. Choisissez une entrée disponible dans le menu `#` de la zone de saisie. Les autorisations de la session restent applicables.
ok | Interaction directe avec le navigateur | Interagissez avec les pages du navigateur intégré. Les longues conversations conservent mieux la position de lecture. Les cartes de commande affichent la durée écoulée et la limite de temps.
```

### Mise à niveau

```ui-checklist
ok | Mise à niveau automatique des données | L’application met à niveau la base et les anciennes sessions à leur ouverture et conserve une sauvegarde préalable. Les sessions ordinaires existantes restent en mode standard jusqu’à l’activation du mode Économie.
ok | Installation et reprise sous Windows | L’installateur 0.2.19 installe le runtime Visual C++ x64 s’il est absent. Si la vérification du démarrage échoue, les fichiers vérifiés sont conservés. Seuls les éléments manquants ou endommagés sont retéléchargés.
```
