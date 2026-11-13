Vulcan Code 0.1.23 adds protected conversation history cleanup, updates the built-in browser, and fixes bundled Lua modules on Linux and macOS.

### Updating to this version

```ui-checklist
ok | Installer 0.2.12 | The install script reuses the existing setup program, which updates and restarts automatically. On Linux and macOS, choosing PATH integration makes the commands available in new terminals. Command-line setup now shows service commands and the WebUI ports.
```

### Conversations

```ui-checklist
ok | History cleanup | Use `/clean-history` or the composer’s add menu in an idle conversation after context compaction. Choose the last turn to clean. Cleanup removes old tool and reasoning details, keeps simplified questions and answers in the timeline, and protects the history the conversation still needs.
ok | Long conversations | Streaming Markdown updates independently of the rest of the transcript. The timeline preserves your reading position as older entries load, including cleaned history.
```

### Built-in browser

```ui-checklist
ok | Page rendering | WebUI renders the built-in browser’s page structure in place of the previous screenshot stream.
ok | Viewport sizes | Choose a desktop, portrait, or landscape preset, enter a custom size, or fit the viewport to the current window with one action.
```

### Workspace and runtime

```ui-checklist
ok | Knowledge base | New sidebar layouts pin Knowledge by default. Your saved layout takes precedence.
ok | macOS Lua runtime | Fixed loading of bundled Lua native modules on Intel and Apple silicon. Context7 now initializes its local storage when you first open its settings.
ok | Linux Lua runtime | Bundled Lua modules such as OpenSSL now load on ARM64 and x64. Release packaging checks that the files support glibc 2.35.
```
