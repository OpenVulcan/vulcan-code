Vulcan Code 0.1.0 is the first public preview: a graphical AI coding workbench that keeps projects, sessions, terminals, the browser, code review and Git commits in one place.

```ui-kv
Release | Preview
Clients | Desktop app, browser WebUI, terminal client (TUI)
Interface languages | 11
License | MIT
```

### Workbench

```ui-cards
Projects and sessions | Many projects and sessions side by side, with file tree, review, memory and subagent panels
Project terminals | Multi-tab terminals; the AI can keep long-running or interactive command sessions
Rich answers | Streaming Markdown, code, Mermaid diagrams and tool cards; interactive answers add visual components such as findings, comparisons, charts and flows
Branch and retry | Edit an earlier question to regenerate, or fork a new session from any finished turn
Dedicated assistants | Sessions with a fixed base prompt and frozen settings for one recurring job
```

### Getting work done

```ui-cards
Goal mode | Set a goal and a token budget; the AI keeps working turn by turn and can be paused, resumed or cleared
Plan mode | Explore and write a plan first, then execute once you approve it
Steering | Add guidance or queue follow-up requests while a task runs
Subagents | Built-in explore and plan roles plus optional role templates, each limited to the models you allow
Questions | When the AI needs a decision it asks with option cards inside the session
```

### Code, knowledge and memory

```ui-cards
Code navigation | CodeKit reads the live working tree: symbols, definitions and references
Knowledge base | Libraries the AI maintains for you, with permanent history and per-project access
Memory | Markdown memory in your user folder, or MemoryMesh long-term memory with recall before each turn
```

### Safety and review

```ui-checklist
ok | Permission modes | Read only, workspace write, auto-approve and full access
ok | Approval pipeline | Rules, a judge model and a review model; anything uncertain goes to you
ok | Diff review | Unified and split views, word-level changes, staging and revert
ok | Git commits | AI-written messages, commit to the current or a new branch, then push
ok | Secrets | Environment variable values are never shown to the AI or the interface
```

### Extending Vulcan Code

```ui-cards
Skills | Prompt skills (`SKILL.md`) and programmable LuaSkills packages
MCP | Connect local and HTTP MCP servers, or let the AI connect one for a single session
MCP service | Other AI tools can list sessions, send messages and answer questions, each with its own token
Plugins | Plugins add tools and their own pages, such as the AI browser and usage statistics
Models | OpenAI, Anthropic, Gemini and other protocol families, built-in providers and custom providers, with separate models per purpose
```

```ui-callout info Preview release
Saved data formats may still change between 0.1.x builds. Upgrades that migrate your data step by step start with 0.2.
```
