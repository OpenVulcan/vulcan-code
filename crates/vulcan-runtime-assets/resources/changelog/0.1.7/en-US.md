Vulcan Code 0.1.7 lets the Linux desktop app start from the dock and the app menu on Ubuntu 23.10 and later, makes reinstalling and updating on Windows safer, closes several security and data-loss gaps, and comes with setup 0.2.9.

### Updating to this version

```ui-checklist
ok | Full update | This version changes the installer and the install layout, so it is installed as a full update: on Windows the app downloads and runs the installer once
ok | Setup installations | Installations made with the setup program update by running vulcan-code-setup update; the app says so instead of offering a download
```

### Linux

```ui-checklist
ok | Dock and app menu | On Ubuntu 23.10 and later, setup 0.2.9 installs an AppArmor profile once, so the desktop app starts from the dock and the app menu and not only from a desktop icon
ok | Profile from the app | When the profile is missing or outdated, the update settings offer to install it with one administrator password, or show the command to run in a terminal
```

### Windows

```ui-checklist
ok | Safe reinstall | Installing into a folder without an earlier installation never removes its files; when reinstalling the same version does not finish, the previous one is restored
ok | Trial launch | Opening the desktop app while the previous Core is still running no longer counts as a failed start of the new version or rolls it back
```

### Security

```ui-checklist
ok | Local service | The Core accepts only requests addressed to a loopback host name, so a web page that points its own address at this computer cannot read the service
ok | Attachments | SVG attachments open sandboxed and cannot run scripts, and attachment paths cannot leave their folder
ok | Read-only checks | Extension tools whose names contain a write verb, the writing options of git branch and the exec options of fd are no longer treated as read-only
ok | Secrets | Storing a plain value over a secret environment variable keeps it secret
```

### Fixes

```ui-checklist
ok | Skills and projects | Installing a local skill from its own folder no longer deletes it, and deleting a project also removes its project variables
ok | Memory | Markdown memory reads headings in Chinese, Japanese and Korean correctly
ok | Plan mode | Turning plan mode on or off after a turn has ended no longer fails
ok | Tool parameters | Parameters that refer to shared definitions, such as lists of allowed values, stay visible and required
ok | Model parameters | Decimal parameters are sent as numbers, and Tavily extract works with only URLs
ok | File edits | Inserting at the end of a file keeps its last line and trailing blank lines
ok | WebUI | Arrow keys in the commit message move the cursor, diff lines that start with --- or +++ are shown, and code examples keep their image links as written
```

### Terminal client

```ui-checklist
ok | Follow-up prompts | Prompts to an existing session are accepted, and a prompt that is not accepted goes back to the input box
ok | Git | A lost connection no longer locks commit and message generation until restart
ok | Reasoning levels | Every reasoning level is sent as chosen
```

### Setup program

```ui-checklist
ok | AppArmor profile | Setup 0.2.9 installs the profile through sudo when the system needs it and leaves the exact command in the notes when the password is not given; uninstalling removes it
```
