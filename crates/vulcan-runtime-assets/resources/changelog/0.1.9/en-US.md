Vulcan Code 0.1.9 fixes long sessions that stayed on a request for minutes after a message was sent and could not be stopped, and fixes two desktop problems on Linux and macOS.

### Updating to this version

```ui-checklist
ok | Incremental update | Updating from 0.1.8 downloads only the files that changed; earlier versions still install as a full update
ok | Setup installations | Installations made with the setup program update by running vulcan-code-setup update; the app says so instead of offering a download
```

### Sessions

```ui-checklist
ok | No more hangs | Long sessions whose history spans several segments, such as sessions with a lot of tool output, no longer wait for minutes after a message is sent, and the stop button takes effect at once
ok | After a restart | After the app restarts, earlier replies in such sessions are still sent back to the model in its own format, thinking included, instead of falling back to plain text
```

### Linux and macOS

```ui-checklist
ok | Moving the window | On Linux, the title bar can move the window again right after it is restored from maximized
ok | Built-in browser | On Linux and macOS, the installed built-in browser works
```
