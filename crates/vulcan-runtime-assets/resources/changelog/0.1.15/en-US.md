### Context compaction fixes

- Compaction preserves the full conversation history, tool results, and native media in the summary request. If the full input exceeds the model's context limit, compaction reports the error and leaves the history intact.
- Eligible unfinished turns can be compacted manually while no request is active. The original task and queued input are preserved, so you can continue the task afterward.
- During manual compaction, queued messages you choose to insert are sent in click order after compaction completes successfully. If compaction fails, is stopped, or the process restarts, those messages stay queued.

### Other improvements

- Subagents can use a reasoning level supported by their target model. Subagent returns, queued input, and the pending-message panel now stay in sync.
- Usage statistics retain the selected time range and show hourly usage for short ranges.
- Going back in the desktop app keeps the startup loading screen closed. Web file prompts fit the window, and the web and terminal clients display `ok` findings as successful checks.
