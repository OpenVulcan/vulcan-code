Vulcan Code 0.1.24 improves attachments and conversation recovery when switching models, with fixes for the built-in browser and terminal logs.

### Conversations and input

```ui-checklist
ok | Drop and paste files | Attachment previews are available again. Files the model cannot upload can be sent as references when the client can read their original paths. Sent file references expand separately from the message body.
ok | Model switching and recovery | Switching models preserves the original conversation history. If reasoning cannot be replayed safely, copy the visible messages into a new conversation draft and review it before sending.
ok | Compaction and scrolling | Context compaction shows separately while it runs, and new text stays visible when following the bottom of a conversation.
```

### Built-in browser

```ui-checklist
ok | Page rendering and editing | Fixed stylesheet and popup resource loading and improved editing. Passwords stay masked in the built-in browser when the page toggles password visibility.
```

### Logs and providers

```ui-checklist
ok | Terminal logs | Improved redaction of stored secret values across line breaks and clipped output. Background jobs can still be stopped when redaction is unavailable.
ok | Provider errors | Error cards retain readable provider messages and help links while masking URLs that contain credentials.
ok | Provider compatibility | Alibaba Cloud Model Studio Token Plan no longer attempts temporary OSS uploads. Fixed knowledge base requests that use OpenVulcan embeddings.
```
