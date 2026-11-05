Vulcan Code 0.1.4 adds a quick way to update, shows what your model account reports in the status bar, and keeps dialogs usable on small screens.

### Updates

```ui-checklist
ok | Update button | When a newer version is available, an Update button appears next to the Vulcan Code name in the sidebar; it shows what is new and downloads and restarts without opening Settings
ok | Skipped versions | When an update skips versions, its preview shows the notes of each skipped version, newest first; this applies to updates from 0.1.4 on
```

### Conversation and status bar

```ui-checklist
ok | Steady conversation | The cues for sending a request, recalling memory and preparing a tool call now float above the input box, so the conversation no longer jumps when they change
ok | Model account | The status bar shows the account's balance, a small bar with its remaining quota, or both; an account that reports neither shows only its name instead of "Balance -"
```

### Small screens

```ui-checklist
ok | Dialogs | Dialogs with buttons at the bottom scroll their content on short screens, so their buttons stay reachable, including the memory quick setup and Ask approvals
ok | Memory quick setup | The model step shows each purpose on one row, so all of them fit on one screen
```

### Git

```ui-checklist
ok | Pull | When the local branch and its upstream have both moved on, pulling explains this with the number of commits on each side instead of showing Git's hints
ok | Commit settings | After saving the commit message model, the page no longer reports unsaved changes
```
