Vulcan Code 0.1.13 fixes the built-in browser on Intel Macs, notifications that stopped closing on their own, and runtime installs that downloaded the same files again.

### Updating to this version

```ui-checklist
ok | Incremental update | Updating from 0.1.8 through 0.1.12 downloads only the files that changed
```

### Interface

```ui-checklist
ok | Notifications close on their own again | After you closed a notification with its own close or action button, every later notification stayed on screen until you closed it by hand; they now disappear on their own again
ok | Built-in browser on Intel Macs | The macOS Intel (x64) build of 0.1.12 contained the Apple silicon browser, which cannot run on Intel Macs; this version ships the Intel browser, and every macOS build is now checked so that all its programs match its processor
```

### Runtimes

```ui-checklist
ok | Runtime downloads are reused | Installing or reinstalling uv, Node.js or pnpm reuses an archive downloaded earlier when it still passes its checksum, instead of downloading it again
```
