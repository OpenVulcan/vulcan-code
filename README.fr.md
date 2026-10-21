<div align="center">

<img src="apps/vulcan-code-web/public/vulcan-logo.png" alt="Vulcan Code" width="120" />

# Vulcan Code

**Un atelier de bureau d'AI Coding open source, avec interface graphique (GUI)**

Rust Core · Shell de bureau Electron · React WebUI

[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)
[![Issues](https://img.shields.io/github/issues/OpenVulcan/vulcan-code)](https://github.com/OpenVulcan/vulcan-code/issues)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](https://github.com/OpenVulcan/vulcan-code/pulls)

[简体中文](README.md) | [English](README.en.md) | [繁體中文](README.zh-TW.md) | [日本語](README.ja.md) | [한국어](README.ko.md) | [Español](README.es.md) | **Français** | [Deutsch](README.de.md) | [Русский](README.ru.md) | [Português](README.pt-BR.md)

</div>

> Ceci est une traduction du [README en chinois simplifié](README.md). En cas de divergence, la version chinoise fait foi. Les corrections de traduction sont les bienvenues via PR.

---

## À propos

Vulcan Code est une **application d'AI Coding dotée d'une interface graphique**. Elle réunit projets, sessions, terminaux, navigateur, revue de code et commits Git dans un seul atelier de bureau, afin que l'IA comprenne votre code, fasse avancer les tâches et livre des résultats que vous pouvez vérifier.

Le projet a été développé et mis au point en interne pendant longtemps ; il est désormais open source sous **licence MIT**. Nous voulons qu'il soit plus que le produit d'une seule équipe : un outil que la communauté façonne ensemble.

- 🐛 Vous avez trouvé un problème ou avez une idée ? Ouvrez une [Issue](https://github.com/OpenVulcan/vulcan-code/issues)
- 🔧 Vous avez corrigé un bug ou développé une fonctionnalité ? Envoyez une [Pull Request](https://github.com/OpenVulcan/vulcan-code/pulls)
- 🤝 Utilisateurs comme développeurs, tout le monde peut participer : construisons ensemble un logiciel d'AI Coding que chacun aimera

## Fonctionnalités principales

### 🧭 Atelier de bureau

- Gestion de plusieurs projets et sessions, avec des panneaux combinables : fichiers, revue, mémoire, sous-agents
- Terminal de projet intégré avec onglets ; l'IA peut conserver des sessions de commandes longues ou interactives
- Réponses en streaming avec rendu direct du Markdown, des blocs de code, des diagrammes Mermaid/SVG et des cartes d'appels d'outils
- Modifiez une question passée pour régénérer la réponse, ou créez une nouvelle session à partir de n'importe quel tour terminé
- Interface en chinois et en anglais, thèmes clair et sombre

### 🎯 Faire avancer les tâches

- **Mode Goal** : fixez un objectif et un budget de tokens ; l'IA poursuit le travail tour après tour, et vous pouvez mettre en pause, reprendre ou effacer l'objectif
- **Mode Plan** : exploration et rédaction d'un plan d'abord, exécution après votre validation
- **Guidage en cours d'exécution** : ajoutez des instructions ou mettez en file d'attente des demandes pendant qu'une tâche s'exécute
- **Sous-agents** : rôles intégrés comme Explore et Plan, plus de 260 modèles de rôles issus d'[agency-agents](https://github.com/msitarzewski/agency-agents) (désactivés par défaut, à activer selon les besoins)
- **Questions structurées** : quand l'IA a besoin de votre décision, elle vous interroge avec des cartes d'options

### 🧠 Comprendre le code et capitaliser l'expérience

- **Index sémantique de code VCE** : construit un index vectoriel du projet pour que l'IA trouve le code pertinent avant de le modifier ; l'index se synchronise automatiquement avec les changements
- **Base de connaissances du projet** : importez des dossiers Markdown, avec recherche BM25 et hybride vectorielle ; accès accordé par projet et sélection par session
- **Mémoire à long terme MemoryMesh** : extrait l'expérience après chaque tour et la rappelle avant le suivant ; les souvenirs peuvent être consultés, corrigés et supprimés, avec corbeille et traçabilité des sources

### 🔍 Des résultats vérifiables

- **Revue de diff** : vues unifiée/côte à côte, différences au niveau des mots, recherche plein texte, indexation (stage) et annulation
- **Commits Git** : message de commit généré par l'IA, commit sur la branche courante ou une nouvelle branche, puis push direct
- **Navigateur IA** : l'IA navigue, clique, prend des captures et lit la console dans une vraie page Chromium ; vous pouvez reprendre la main à tout moment

### 🧩 Extensible

- **Skills de prompts** (`SKILL.md`) : formalisez les méthodes de travail de votre équipe et réutilisez-les
- **LuaSkills** : regroupez des actions courantes en paquets de capacités programmables, configurables et installables
- **MCP** : serveurs MCP en processus local ou HTTP, avec diagnostic de connexion et inspection des schémas d'outils
- **Service MCP** : Vulcan est aussi un serveur MCP. D'autres outils d'IA comme Claude Code et Codex peuvent, selon les droits accordés, lister les projets et les sessions, envoyer un message et attendre la réponse, démarrer des sessions, arrêter des exécutions et répondre aux questions, et appeler des modèles d'image et de vidéo accordés séparément ; chaque outil a son propre jeton, et les approbations de permissions restent toujours dans Vulcan
- **Système de plugins** : les plugins peuvent fournir des outils et intégrer leur propre interface. Plugins intégrés : recherche de documentation Context7, navigateur IA, compression de sortie de commandes RTK, statistiques d'utilisation, outils de débogage CDP/DAP

### ⚙️ Modèles et fournisseurs

- Prise en charge des familles de protocoles OpenAI, Anthropic, Gemini et Cohere, avec des configurations intégrées pour OpenAI, Anthropic, Google, DeepSeek, Kimi, MiniMax, Alibaba, OpenRouter et d'autres ; possibilité d'ajouter des fournisseurs personnalisés
- Conversation, rappel de mémoire, indexation du code et génération multimédia peuvent chacun utiliser un modèle différent
- Capacités étendues : recherche et récupération web ; compréhension d'images, de vidéos et d'audio ; génération d'images et de vidéos ; synthèse vocale et transcription
- Politiques de fenêtre de contexte hiérarchisées, avec compaction automatique ou manuelle (`/compact`)
- Statistiques d'utilisation des tokens, de latence et de coût indicatif

## Feuille de route

L'**application de bureau GUI** est disponible dès aujourd'hui. Prochaines étapes :

- [ ] **Version TUI** : utiliser Vulcan Code dans le terminal
- [ ] **Connexion à distance depuis un mobile** : consulter et piloter depuis votre téléphone les sessions qui tournent sur votre bureau
- [ ] **Validation Linux / macOS** : les scripts de build existent, une validation complète sur chaque plateforme reste à faire

Si l'un de ces points vous intéresse ou si vous avez de nouvelles idées, discutons-en dans les Issues.

## Architecture

```
┌──────────────────────────┐      ┌──────────────────────────────────────────┐
│  Electron desktop shell  │      │  Rust Core (vulcan-code-service)         │
│  window / tray / host    │◀────▶│  session kernel · agent runtime · tools  │
│  ┌────────────────────┐  │ RPC  │  provider protocols · MCP · LuaSkills    │
│  │ React WebUI        │  │events│  VCE index · knowledge base · MemoryMesh │
│  └────────────────────┘  │      │  terminal host · browser host · plugins  │
└──────────────────────────┘      └──────────────────────────────────────────┘
```

Le service Core peut fonctionner seul en arrière-plan (zone de notification) ; fermer la fenêtre de bureau n'interrompt pas les tâches en cours.

| Répertoire | Description |
| --- | --- |
| `apps/vulcan-code-service` | Hôte du service Core : noyau de sessions, runtime d'agents et services RPC/streaming du WebUI |
| `apps/vulcan-code-desktop` | Shell de bureau Electron : fenêtre et zone de notification ; démarre le Core local et charge le WebUI |
| `apps/vulcan-code-web` | WebUI React/Vite : atelier, paramètres, sessions, diff, skills, etc. |
| `crates/vulcan-agent-runtime` | Runtime d'agents partagé : fournisseurs, outils, LuaSkills, MCP, exécution en streaming |
| `crates/vulcan-service-*` | API des services backend, noyau du service Core et adaptateurs de runtime |
| `crates/vulcan-memorymesh-*` | Mémoire à long terme MemoryMesh |
| `crates/vulcan-runtime-assets` | Ressources de référence : prompts système, manifestes de fournisseurs, modèles de sous-agents |
| `plugins/` | Plugins intégrés |
| `docs/contracts/` | Documents de contrats de développement |

## Démarrage rapide

> La validation complète a été effectuée sous **Windows x86_64**. Linux et macOS disposent du script `make.sh` correspondant, mais n'ont pas encore été testés de bout en bout ; toute aide pour les valider est bienvenue.

### Prérequis

- `make`
- [Rustup](https://rustup.rs/) : la version de la toolchain est fixée par `rust-toolchain.toml` et installée automatiquement
- PowerShell 7
- Node.js et pnpm

### Compiler et lancer

```bash
make init-system   # Première fois : installe les outils Rust, les dépendances hôte et Node/pnpm gérés
make sync          # Synchronise la configuration, les skills système, les plugins et le runtime navigateur dans output/
make build         # Compile le Core, le WebUI et le shell de bureau Electron
make run           # Lance l'application de bureau
```

Vous pouvez aussi utiliser `make br` pour compiler et lancer en une seule étape. Pour une build de release, utilisez `make release`.

Voir [`docs/contracts/development-environment.md`](docs/contracts/development-environment.md) pour les détails de l'environnement.

### Développement au quotidien

Le serveur de développement Vite, le service Core et le shell Electron sont indépendants et peuvent être démarrés et redémarrés séparément :

```bash
make vite start      # Démarre le serveur de développement Vite du WebUI (status / stop également)
make service         # Démarre uniquement le Core en arrière-plan, sans ouvrir la fenêtre
make core-restart    # Modifications du Core Rust uniquement : build incrémentale et remplacement à chaud (Windows Debug)
make sync            # Resynchronise après modification des ressources, de la configuration, des skills ou des plugins
```

- Pour des modifications du WebUI uniquement, laissez Vite et le shell de bureau tourner pour profiter du rechargement à chaud
- En mode debug, `make run` / `make br` s'assurent que Vite est démarré
- Ne modifiez pas `output/` directement ; les ressources de référence se trouvent dans `crates/vulcan-runtime-assets/resources/`

## Contribuer

Toutes les contributions sont les bienvenues : signalements de bugs, documentation, améliorations de l'UX, nouvelles plateformes, nouveaux fournisseurs, plugins et skills…

**Ouvrir une Issue**

- Bugs : indiquez votre système d'exploitation, les étapes de reproduction, le comportement attendu et observé, ainsi que les journaux ou captures pertinents
- Demandes de fonctionnalités : décrivez le cas d'usage et le problème à résoudre

**Proposer une PR**

1. Forkez le dépôt et créez une branche à partir de `main`
2. Avant toute modification, lisez [`AGENTS.md`](AGENTS.md). Il consigne les conventions durables du dépôt ; pour des domaines spécifiques, lisez aussi les documents correspondants dans [`docs/contracts/`](docs/contracts/)
3. Vérifiez localement avant de soumettre :

   ```bash
   # Rust : privilégiez des vérifications ciblées sur les crates modifiés
   cargo check -p <crate> --all-targets
   cargo nextest run -p <crate>

   # WebUI
   pnpm --dir apps/vulcan-code-web run lint
   pnpm --dir apps/vulcan-code-web run test
   pnpm --dir apps/vulcan-code-web run build
   ```

4. Dans la description de la PR, expliquez ce qui a changé, pourquoi, et comment vous l'avez vérifié

Quelques conventions :

- Les noms, descriptions et schémas d'outils destinés à l'IA sont rédigés en anglais ; les textes de l'interface destinés aux utilisateurs sont localisés
- La concurrence de compilation et de tests est limitée à 4 par défaut ; ne supprimez pas cette limite dans les commandes de vérification
- Une PR ne devrait traiter qu'un seul sujet, pour faciliter la revue

## Remerciements

Vulcan Code repose sur de nombreux excellents projets open source. Merci à leurs auteurs et à leurs communautés.

**Composants et outils intégrés**

- [RTK](https://github.com/rtk-ai/rtk) : compression de la sortie des commandes pour économiser des tokens (plugin Vulcan RTK)
- [Context7](https://github.com/upstash/context7) : documentation de bibliothèques à jour et adaptée à chaque version (plugin Context7)
- [Playwright](https://github.com/microsoft/playwright) / [Playwright MCP](https://github.com/microsoft/playwright-mcp) : pilote et outils du navigateur IA
- [LuaSkills](https://github.com/LuaSkills/luaskills) : runtime LuaSkills
- [ripgrep](https://github.com/BurntSushi/ripgrep) et [ast-grep](https://github.com/ast-grep/ast-grep) : outils fournis de recherche textuelle et d'arbre syntaxique
- [OpenAI Codex](https://github.com/openai/codex) Code Mode : le bac à sable JavaScript du mode code (runtime V8, processus hôte, protocole gRPC et client) est repris directement de Codex (Apache-2.0) ; les modifications locales sont signalées dans le code source et listées dans le `NOTICE.md` de chaque crate (`crates/vulcan-code-mode*/`, `native/code-mode-host/*/`)
- [rusty_v8](https://github.com/denoland/rusty_v8) : liaisons Rust du moteur V8 utilisé par le bac à sable du mode code

**Bureau et interface**

- [Electron](https://www.electronjs.org/) : shell de bureau
- [React](https://react.dev/) et [Vite](https://vite.dev/) : framework et outil de build du WebUI
- [shadcn/ui](https://ui.shadcn.com/), [Radix UI](https://www.radix-ui.com/), [Tailwind CSS](https://tailwindcss.com/) : composants et styles
- [xterm.js](https://xtermjs.org/) : rendu du terminal
- [Shiki](https://shiki.style/) : coloration syntaxique du code et des diffs
- [Mermaid](https://mermaid.js.org/) : rendu des diagrammes
- [Lucide](https://lucide.dev/), [Iconify](https://iconify.design/) : icônes

**Rust Core**

- [Tokio](https://tokio.rs/) et [Axum](https://github.com/tokio-rs/axum) : runtime asynchrone et framework de services
- [rmcp](https://github.com/modelcontextprotocol/rust-sdk) : SDK Rust officiel de MCP
- [LanceDB](https://github.com/lancedb/lancedb) et [Apache Arrow](https://arrow.apache.org/) : index vectoriel et données en colonnes
- [SQLite](https://sqlite.org/) ([rusqlite](https://github.com/rusqlite/rusqlite)) : stockage local des données
- [jieba-rs](https://github.com/messense/jieba-rs) : segmentation du chinois pour la recherche plein texte de la base de connaissances
- [portable-pty](https://github.com/wezterm/wezterm) (WezTerm) : pseudo-terminal multiplateforme

**Références de conception et d'implémentation**

- [OpenAI Codex](https://github.com/openai/codex) : référence pour le transport du protocole Responses, les nouvelles tentatives et la compaction du contexte ; la grammaire et l'analyseur en flux d'Apply Patch sont adaptés de Codex
- [cc-switch](https://github.com/farion1231/cc-switch) : référence pour la conversion multi-protocoles, les appels d'outils en streaming et les données de tarification des modèles
- [CLIProxyAPI](https://github.com/router-for-me/CLIProxyAPI) : référence pour le protocole Gemini et la conversion de compatibilité
- [OpenCode](https://github.com/anomalyco/opencode) : référence pour les catalogues de fournisseurs et les prompts système
- [agency-agents](https://github.com/msitarzewski/agency-agents) : source des modèles de rôles de sous-agents
- [Answer me with HTML](https://github.com/QingYunA/answer-me-with-html) : inspiration pour les réponses HTML, les brouillons compacts et les composants de modèles ; notre protocole, notre moteur de rendu intégré et notre lecteur isolé sont développés indépendamment, sans sa CLI

**Communauté**

- [LINUX DO](https://linux.do/) : merci à la communauté LINUX DO pour son soutien et ses échanges

La liste complète des dépendances tierces et de leurs licences se trouve dans [`THIRD_PARTY_NOTICES.md`](crates/vulcan-runtime-assets/resources/licenses/THIRD_PARTY_NOTICES.md) ; vous pouvez aussi la consulter dans l'application via **Settings → About and licenses**.

## Licence

[MIT](LICENSE) © 2026 OpenVulcan
