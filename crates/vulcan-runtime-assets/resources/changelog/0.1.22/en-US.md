Vulcan Code 0.1.22 adds free cloud memory on supported OpenVulcan sites and improves how conversations handle images, provider failures, and voice recordings.

### Memory

```ui-checklist
ok | Free cloud memory | Sign in to OpenVulcan on a supported site. Review the models, processing regions, and data handling terms before enabling it. Reported token usage is broken down by memory stage.
ok | Vector rebuild | See completed batches and the retry countdown when rate-limited. Rebuilding resumes after the wait.
ok | Markdown memory | Revision conflicts now distinguish changes staged within the batch from the last saved revision.
```

### Conversations

```ui-checklist
ok | New images in long conversations | With automatic compaction enabled, earlier images can be summarized in batches to make room. The new images are then sent in the original request.
ok | Provider errors | Bailian quota errors are recognized as rate limits. Balance and network failures appear in error cards, and routine system notices appear as status rows.
```

### Voice input and tools

```ui-checklist
ok | Recording duration | Recordings that reach the time limit can be submitted for transcription.
ok | RTK statistics | Command rankings load without scanning Claude conversation logs. They still use the same RTK statistics database.
```
