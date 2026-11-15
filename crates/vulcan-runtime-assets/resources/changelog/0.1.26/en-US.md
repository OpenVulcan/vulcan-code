Vulcan Code 0.1.26 lets you reconnect a missing project folder without losing sessions, and improves app updates and tool results.

### Projects and sessions

```ui-checklist
ok | Recover a project folder | If a folder moves or a drive disconnects, recheck the original location or choose a new folder and keep your session history. The change appears in the conversation timeline, and the next model request uses the new folder.
ok | Browse mounted drives and typed paths | The folder picker lists mounted volumes on Linux and macOS. After typing a path, open it before confirming the folder selection.
ok | Open the latest message when switching conversations | Switching conversations takes you to the latest message so you can pick up where the conversation left off.
```

### Installation and updates

```ui-checklist
ok | Prepare updates before restarting | Automatic downloads are on by default. The app lets you know when a downloaded and verified update is ready, then switches versions when you confirm the restart. You can still turn off automatic downloads and update manually.
ok | Standalone installer 0.2.17 | Windows has a signed graphical installer, Linux has AppImage packages, and macOS has a DMG. After a terminal install on macOS, `vcode manager gui` opens the graphical manager. On first install, LuaSkills selects the Mainland China or international Hub that matches the install route.
```

### Browser and document tools

```ui-checklist
ok | Browser action cards | Playwright cards show the action, target, and result for navigation, clicks, screenshots, and other operations. The bundled Vulcan Browser skill guides the assistant through browser actions and verification.
ok | Vulcan Work document tools | Vulcan Code installs Vulcan Work from the selected LuaSkills Hub. Its tools work with PDF, Word, Excel, and PowerPoint. Dedicated cards show files, spreadsheet ranges, or results as relevant. Help for implicitly registered LuaSkills tools now lists the parameters each tool accepts.
```
