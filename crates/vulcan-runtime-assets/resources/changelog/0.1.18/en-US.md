Vulcan Code 0.1.18 fixes context compaction errors and improves message shortcuts, browser controls, and usage pricing.

### Context and conversations

```ui-checklist
ok | Context compaction | Manual compaction uses the selected model. Errors such as insufficient balance or failed authentication stop automatic retries. When the summary model requests a tool, its preview stays linked to the compaction request so a missing request ID no longer interrupts the flow.
ok | Automatic retry history | Errors from automatic retries of one request appear in a single card, where you can browse each attempt.
ok | Memory expiration notices | The conversation now identifies when previously attached memory context has expired.
```

### Controls and settings

```ui-checklist
ok | Queue and insert shortcuts | While a task is running, configure Enter to queue or insert a message. Ctrl+Enter uses the other action. In an idle conversation, messages send immediately.
ok | Default approvals and reasoning | Set the default approval mode for new conversations. Returning to a previously used model restores its saved reasoning level.
ok | Close the local browser | Closing the workspace browser panel immediately closes the current conversation's browser pages and frees their resources.
```

### Providers and usage

```ui-checklist
ok | Provider notices | NodeAI stays out of the provider list until configured. Providers marked as unstable display a warning.
ok | Filter requests by model | The recent requests list in usage statistics can be filtered by model.
ok | Tiered and provider pricing | Usage estimates can use model prices based on token tiers. You can also set separate model prices for a specific provider.
```

### Release notes

```ui-checklist
ok | Release notes | The notes for 0.1.14 through 0.1.17 use grouped checklists again, making earlier changes easier to scan.
```
