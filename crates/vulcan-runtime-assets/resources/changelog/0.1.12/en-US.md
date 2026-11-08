Vulcan Code 0.1.12 finishes updates that need a reinstall inside the app, fixes the macOS window buttons and icon, and fixes title and validation requests that some providers rejected.

### Updating to this version

```ui-checklist
ok | Incremental update | Updating from 0.1.8 through 0.1.11 downloads only the files that changed; earlier versions still install as a full update
ok | Reinstalls finish in the app | Future updates that need a reinstall no longer send you to the website: only changed files are downloaded, installed base resources such as the browser, Electron and Node are reused, and the setup program finishes the update on restart
ok | Windows installer copies | A copy installed with the Windows installer gets such updates as a small signed update package instead of the full installer
```

### Interface

```ui-checklist
ok | macOS window buttons | Only the system window buttons remain, so minimize, maximize and close no longer appear twice, and the green button has the system tiling and full-screen menu again
ok | macOS icon | The icon in the Dock and while the app is closed is the same size as other apps' icons
ok | Model service account cards | Cards have a minimum width that follows the interface scale, so quota rows no longer wrap onto several lines
ok | Setup guide | When you are signed in to OpenVulcan and no judge is configured, the judge step preselects the cloud judge; nothing is saved until you confirm
```

### Models and providers

```ui-checklist
ok | Request fixes | Session titles, model validation in settings and prompt drafting for dedicated assistants now carry a cache scope, so OpenCode Go no longer returns 400 and Codex Responses Lite no longer rejects them
ok | Cloud platforms | CommandCode, OpenCode Go, Open Design, OpenRouter and TokenRhythm are classified as cloud platforms; shared model facts are taken from official providers first, then cloud platforms, then relays
ok | Core model list | What a model itself can do and its limits are recorded once in a core model list that provider rules inherit; the provider manifest status page marks fields inherited from it
```
