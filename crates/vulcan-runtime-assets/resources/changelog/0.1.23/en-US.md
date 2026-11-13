Vulcan Code 0.1.23 adds protected conversation history cleanup, updates the built-in browser, and fixes bundled Lua modules on macOS.

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

### Workspace and macOS

```ui-checklist
ok | Knowledge base | New sidebar layouts pin Knowledge by default. Your saved layout takes precedence.
ok | macOS Lua runtime | Fixed loading of bundled Lua native modules on Intel and Apple silicon. Context7 now initializes its local storage when you first open its settings.
```
