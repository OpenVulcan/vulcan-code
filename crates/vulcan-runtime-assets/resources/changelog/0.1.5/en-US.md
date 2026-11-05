Vulcan Code 0.1.5 gives each model its own default compaction window, starts the desktop app in a single window, and lets you choose how often to check for updates.

### Context window

```ui-checklist
ok | Per-model defaults | Each enabled conversation model can have its own default compaction window for new sessions: the system default, follow the model, a fixed tier or a custom size; existing sessions are not affected
ok | Custom windows | A custom size takes a token count or K for thousands (such as 235K), at least 100K and no more than the model window; the window picker next to the input box can switch to a custom size too
ok | Switching models | Switching models in a session offers the target model's default window and changes nothing until you confirm; a window the target model cannot hold now follows the model instead of blocking the switch
ok | Terminal interface | The TUI settings can set the default window per model as well
```

### Updates

```ui-checklist
ok | Check interval | Choose how often to check for updates automatically: every hour, every 6 hours, every 12 hours or every day, every 6 hours by default; a change applies without a restart
ok | Software update page | The Software update page in Settings is rearranged: the current version with check and repair first, then the update settings, and last the update found with the notes of every version it brings, newest first; the update dialog shows the same list of versions
ok | Restart on Linux | On Linux, a Core started from the desktop app restarts after an update instead of staying at "Restarting"
```

### Desktop app

```ui-checklist
ok | Single-window startup | No separate startup window opens; the workbench window shows the startup screen with the real startup stages and any failure, then opens the interface in place once Core is ready
ok | Startup appearance | The startup screen uses the language and appearance you used last time; the first start follows the system
ok | App icon | The Linux dock and the macOS Dock show the Vulcan Code icon instead of a generic one; opening the app again brings its starting window to the front
```

### Fixes

```ui-checklist
ok | Embedding models | Embedding models with an adjustable output size (such as Qwen3-Embedding-8B, 32 to 4096 dimensions) no longer reject every response
ok | Answer components | Callouts without a tone and findings rows with four cells now render instead of falling back to code blocks, in both the WebUI and the TUI
```

### Installer

```ui-checklist
ok | macOS one-line install | Running the install script through a curl pipe on macOS can now read the keyboard
ok | macOS app | The macOS app the installer creates has an icon, and the installer says where the app is when it finishes
ok | Linux menu entry | The Linux menu entry matches the running window, so an icon pinned to the dock opens the app
```
