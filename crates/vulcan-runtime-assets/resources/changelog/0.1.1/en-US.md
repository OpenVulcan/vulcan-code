Vulcan Code 0.1.1 adds in-app updates, a new way to install, and an optional user service.

### In-app updates

```ui-checklist
ok | Check and download | The service checks for new versions, downloads only the files that changed, and switches only after the signature checks pass
ok | Restart when you confirm | The desktop app, the browser WebUI and the TUI all show available updates and restart into the new version once you confirm
ok | Automatic rollback | When a new version fails to start, the launcher switches back to the previous one and the app tells you
ok | Full updates | When a full install is needed, the app downloads and runs the installer and reopens when it finishes
```

### Installation and service

```ui-cards
Setup program | The new vulcan-code-setup installs per user on Windows, macOS and Linux, with optional desktop app, terminal client, menu entries and desktop shortcut
User service | The service can start when you sign in: a Windows startup entry, a macOS LaunchAgent or a Linux systemd user service
Port range | The ports the service uses can be changed in the setup program's advanced settings
Terminal client | When no service is running, the TUI starts the installed one, just like the desktop app
```

```ui-callout info Upgrading from 0.1.0
0.1.0 has no in-app updates yet, so run the installer or the install command again to move to 0.1.1; later versions update inside the app.
```
