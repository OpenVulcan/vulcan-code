Vulcan Code 0.1.15 fixes context compaction so it preserves the full input and lets eligible unfinished turns resume after manual compaction.

### Context compaction fixes

```ui-checklist
ok | Full input for compaction | Compaction preserves the full conversation history, tool results, and native media in the summary request. If the full input exceeds the model's context limit, compaction reports the error and leaves the history intact.
ok | Manual compaction of unfinished turns | Eligible unfinished turns can be compacted manually while no request is active. The original task and queued input are preserved, so you can continue the task afterward.
ok | Queued messages after compaction | During manual compaction, queued messages you choose to insert are sent in click order after compaction completes successfully. If compaction fails, is stopped, or the process restarts, those messages stay queued.
```

### Other improvements

```ui-checklist
ok | Subagent reasoning and queued input | Subagents can use a reasoning level supported by their target model. Subagent returns, queued input, and the pending-message panel now stay in sync.
ok | Usage statistics by hour | Usage statistics retain the selected time range and show hourly usage for short ranges.
ok | Desktop and interface fixes | Going back in the desktop app keeps the startup loading screen closed. Web file prompts fit the window, and the web and terminal clients display ok findings as successful checks.
```
