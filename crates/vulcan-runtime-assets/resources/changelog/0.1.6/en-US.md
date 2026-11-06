Vulcan Code 0.1.6 fixes restarting after an update and reopening the app on Linux, lets you close a desktop window whose startup failed, and comes with setup 0.2.8, which runs the whole installation in the interactive installer.

### Desktop

```ui-checklist
ok | Startup failure | When startup or the interface fails, the window shows reload and close buttons on Windows and Linux, so it can be closed again
ok | Busy data directory | When another process holds the data directory and does not answer, the startup page says to end the vulcan-code-service processes and open the app again
```

### Linux

```ui-checklist
ok | Update restart | After an update the Core restarts at once; the desktop app opens on the new version without waiting two minutes
ok | Reopening | Opening the app while it is closing opens it again instead of doing nothing, and the desktop app's helper processes no longer stay behind
ok | Dock icon | The menu entry from setup 0.2.8 matches the desktop app's window, so an icon pinned to the dock belongs to the app; running the setup program's update once rewrites an existing entry
```

### Fixes

```ui-checklist
ok | Provider errors | When a provider reports an error in the middle of a response, its own message is shown instead of a note that a response item was still open (OpenAI Chat and Anthropic protocols)
```

### Setup program

```ui-checklist
ok | Interactive installer | After the wizard, the review, the download progress and the result stay in the interactive installer, in the interface language; updates use it as well
ok | Exact download | The review shows exactly how much will be downloaded; files already present are not counted again
ok | Next time | The last page says how to change, update or uninstall the installation later, and that the setup program updates itself first
ok | Cancel | Going back or cancelling at the review changes nothing and ends quietly
ok | Install scripts | The install scripts show where they download from and the progress, and move to another server after 30 seconds without data
```
