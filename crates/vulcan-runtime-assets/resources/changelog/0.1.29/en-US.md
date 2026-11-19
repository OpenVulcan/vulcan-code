Vulcan Code 0.1.29 adds heartbeats for long-running external MCP tool calls and updates the OpenAI model catalog.

### External MCP

```ui-checklist
ok | Heartbeats for long tool calls | When a client accepts SSE, a long-running call sends periodic heartbeats to reduce idle disconnects. The same request receives the final result when the tool finishes.
ok | Run long commands in the background | Client-side request deadlines still apply. If a foreground Shell request disconnects, the service stops its process tree. For a long command, pass `background: true` when starting it and check the result with `shell_job` using the same `sid`. This requires the `shell_job` grant.
ok | Clearer token diagnostics | Service logs distinguish missing or malformed authentication headers from invalid tokens and record manual rotation events. Entries contain only the diagnostic category or client ID.
```

### Models

```ui-checklist
ok | GPT-6.2 Sol and Astra | Both models are available through the OpenAI API and Codex subscriptions. Codex subscriptions use the context window reported by the model catalog.
ok | GPT Image 2.6 | Flare and Sunburst are now available in the OpenAI media picker.
```
