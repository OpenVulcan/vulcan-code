Vulcan Code 0.1.14 updates the macOS desktop experience and unread indicators, and improves context compaction and usage statistics.

### Desktop and sessions

```ui-checklist
ok | macOS desktop | The app bundle, Dock, and menu bar use the Vulcan Code name and icon. The native window controls sit in the title bar, and double-clicking the title bar toggles native full screen.
ok | Unread sessions | Background sessions show an unread dot when they finish and keep it across restarts. The selected session also shows an unread dot if it finishes while the window is hidden or unfocused, until you interact with the window again.
ok | Streaming status | The web interface closes stale reasoning popups when a request or turn ends and restores streaming status after reconnecting.
ok | Tool calls and decision cards | Long tool-call sequences fit in the existing header width. The web and terminal clients recognize recommended options with no space after the asterisk and decision cards that start directly with their options.
```

### Context and usage

```ui-checklist
ok | Context compaction | Compaction requests carry the source turn's tool list so providers that support prompt caching can reuse the cached prefix.
ok | Usage statistics | Token totals and reference cost estimates are corrected, and estimates based on partial usage are marked. If statistics change during a read, the dashboard retries once to get a consistent result.
```
