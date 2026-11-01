Vulcan Code 0.1.0 is the first public preview: a graphical AI workbench for writing code and for everyday work tasks. Projects, sessions, terminals, the browser, code review and Git commits all live in one place.

```ui-kv
Release | Preview
Clients | Desktop app, browser WebUI, terminal client (TUI)
Interface languages | 11
Packages | Python, Node.js and browser components included; install with one command
License | MIT
```

### Highlights

````ui-tabs
### Compaction window
Every session can switch its compaction window at any time from the context capacity dial in the composer, so a long task never needs a new session.

```ui-kv
Sizes | Five steps, by default 200K, 256K, 272K, 373K and 500K tokens, editable in Settings
Follow the model | Choose "Default (follow model)" to compact by the current model's window
Scope | Changes only when automatic compaction happens, not the model's own context window
Manual compaction | Run /compact on the current session at any time
```

### Tool cards and folding
Every kind of tool has its own card, so even many tool calls never bury the answer.

```ui-checklist
ok | Purpose-built cards | Files, commands, terminals, subagents, search, media, questions, environment variables, configuration changes and more
ok | Timing and links | Every call shows how long it took; files on a card open straight in the files panel
ok | Folding on demand | Open while running and folded when done; consecutive calls group under one summary
ok | Switch any time | Pick the default in Settings, or switch per session with "Tool folding" in the composer or /collapse
```

### Multimodal
Under "Multimodal extensions" each capability gets one model or several; "Extended capabilities" turns them on per session.

```ui-cards
Web | Web search and page extraction
Understanding | Image, video and audio understanding
Generation | Image and video generation; images can also use a Codex subscription
Speech and music | Speech synthesis and transcription, music generation and covers
```

### Environment variables
Built-in environment variable management keeps secrets on your machine; the AI only sees variable names.

```ui-checklist
ok | Three layers | Global, project and session layers edited in the interface; the page can be shown in the sidebar
ok | Values stay hidden | Values are never shown to the AI or the interface, and values that appear in tool results are masked
ok | Filled on request | When the AI is missing a variable it asks you in a form, and the value never passes through the conversation
ok | Injected automatically | Commands and terminals get the variables of their layer; MCP servers use the global ones
```

### Subagent models
Subagents only use the models you allow.

```ui-kv
Two levels | The system setting is the default; each session can set its own
Checked by default | The calling model is included by default and can be unchecked
Out of range | Refused outright, never silently replaced with another model
Session settings | Change it any time under Session settings below the input box, or go back to the system default in one click
```
````

### Workbench

```ui-cards
Projects and sessions | Many projects and sessions side by side in your own order, with file tree, review, memory and subagent panels
Project terminals | Multi-tab terminals; the AI can keep long-running or interactive command sessions
Files panel | Mention files with @, drag them into the composer, open paths from terminals and output, find and go to line
Rich answers | Streaming Markdown, code and Mermaid diagrams; interactive answers add checklists, comparisons, charts, flows and more, and decision cards insert or send your reply
Branch and retry | Edit an earlier question to regenerate, or fork a new session from any finished turn
Command palette | Keyboard shortcuts, find in the conversation, and copy a conversation as Markdown
Dedicated assistants | Sessions with a fixed base prompt and frozen settings for one recurring job
```

### Getting work done

```ui-cards
Goal mode | Set a goal and a token budget; the AI keeps working turn by turn and can be paused, resumed or cleared
Plan mode | Explore and write a plan first, then execute once you approve it
Steering | Add guidance or queue follow-up requests while a task runs
Subagents | Built-in explore and plan roles plus optional role templates
Questions | When the AI needs a decision it asks with option cards inside the session
```

### Code, knowledge and memory

```ui-cards
Code navigation | CodeKit reads the live working tree: symbols, definitions and references
Knowledge base | Libraries the AI maintains for you, with permanent history and per-project access
Memory | Markdown memory in your user folder, or MemoryMesh long-term memory recalled before each turn and shown in the work process
```

### Safety and review

```ui-checklist
ok | Permission modes | Read only, workspace write, auto-approve and full access
ok | Approval pipeline | Rules, a judge model and a review model; anything uncertain goes to you
ok | Judge model | Runs on a local model, in the cloud or through an API
ok | Diff review | Unified and split views, word-level changes, staging and revert
ok | Git | Pull, AI-written commit messages, commit to the current or a new branch, then push
```

### Extending Vulcan Code

```ui-cards
Skills | Prompt skills (SKILL.md) and programmable LuaSkills packages, installable from the LuaSkills Hub
MCP | Connect local and HTTP MCP servers, or let the AI connect one for a single session
MCP service | Other AI tools can list sessions, send messages and answer questions, each with its own token
AI configuration | The AI can add MCP servers and model providers for you, showing a preview first and asking you as your permission mode requires
Plugins | Plugins add tools and their own pages, such as the AI browser and usage statistics
RTK | Compresses command output and Git context to save tokens, and counts the savings
Models | OpenAI, Anthropic, Gemini and other protocol families, built-in and custom providers, with separate models per purpose
```

```ui-callout info Preview release
0.1.x is still a preview, and features and the interface will keep changing. On upgrade the app database and session data are backed up first, then upgraded version by version automatically, with nothing for you to do.
```
