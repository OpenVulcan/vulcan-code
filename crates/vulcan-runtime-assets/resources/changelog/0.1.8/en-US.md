Vulcan Code 0.1.8 runs shell commands as jobs that can continue in the background, adds a project home page, lets the tray icon open your chosen client and switch the language, and brings several hundred fixes from a full review before release. It comes with setup 0.2.10.

### Updating to this version

```ui-checklist
ok | Full update | This version changes the installer again, so it is installed as a full update: on Windows the app downloads and runs the installer once
ok | Setup installations | Installations made with the setup program update by running vulcan-code-setup update; the app says so instead of offering a download
```

### Shell commands

```ui-checklist
ok | Live output | An expanded command card shows the latest lines of a running command, so long commands no longer look stuck
ok | Background jobs | A command can run in the background, and a message sent while a command runs moves it there; you can also move it from the command card
ok | Finished jobs | When a background command finishes after the AI has stopped, the session continues by itself; the finish shows as a context card with the command's name and result
ok | No size limit | Output goes to a rolling log instead of stopping the command at 8 MiB; the AI gets the newest lines and can read earlier ones when needed
ok | Activity panel | The session panel counts subagents and background commands and opens a list of them with their logs; restarting for an update warns about running jobs
ok | Always there | The counts show in every session, even without Git, subagents or running commands
```

### Projects

```ui-checklist
ok | Project home | The project name in the sidebar opens a project page with rename, quick actions, session statistics, a searchable session list and every project setting
ok | Unsaved edits | Unsaved project memory edits are kept when you leave the project page and come back
```

### Tray

```ui-checklist
ok | Double click | On Windows, a double click on the tray icon opens the client you choose: the desktop app, the WebUI or the terminal client; the menu moves to the right button
ok | Language | A Language menu in the tray switches the interface language, and open clients follow it
```

### Interface

```ui-checklist
ok | Reading width | Every conversation width preset is wider, and a wider column no longer folds the navigation on ordinary windows
ok | Status cue | The status cue now sits on the top edge of the input box instead of floating over the conversation
ok | Dark themes | Selected text is visible in dark themes
ok | Answer components | Long keys no longer squeeze values into a narrow column, and formulas in several groups before a legend display correctly
ok | Tool runs | Running tool cards in a folded run stay together
```

### Fixes

```ui-checklist
ok | Plan mode | Only a submitted plan waits for confirmation, and it is restored after a restart
ok | Empty answers | A response that ends with no answer is retried under your retry settings; one cut off by the output limit before any answer says so
ok | Continuing a session | Sending a message after the context was settled no longer fails to start
ok | Model parameters | A parameter set for certain models is not sent to other models
ok | MCP | Tool lists spread over several pages are read completely, and headers you set win over environment headers
ok | Windows installer | When switching to the new version fails, the installer shows why and keeps the files
ok | Damaged installation | A desktop app whose installation is incomplete says so instead of starting the wrong service
ok | Review fixes | Several hundred smaller fixes across sessions, storage, memory, files, permissions, plugins, the WebUI and the terminal client
```

### Security

```ui-checklist
ok | Local browser | The built-in browser blocks every local address form of the Core's protected port
ok | Provider errors | API keys are removed from provider errors before they are shortened or shown
ok | Plugin tools | Arguments that do not match a plugin tool's parameters are refused before the tool runs
```

### Terminal client

```ui-checklist
ok | Activity | An Activity tab, a count in the session bar and a log view for background commands, plus a warning before restarting
ok | Fixes | Pickers, pasting into ask answers, mouse selection, long menus and code fences behave correctly
```

### Setup program

```ui-checklist
ok | Retry after failure | Running setup again after a failed version switch registers the service again
ok | Several installations | Uninstalling one installation leaves the launchers, menu entries and services of another installation alone
```
