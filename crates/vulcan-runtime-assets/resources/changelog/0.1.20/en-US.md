Vulcan Code 0.1.20 adds OpenVulcan cloud transcription and project-attached folders, and improves queued turn recovery and provider troubleshooting.

### Voice input and workspace

```ui-checklist
ok | Cloud transcription | Sign in to OpenVulcan on a site that offers the service. Microphone settings let you choose the site-recommended model, a specific cloud model, or a configured provider’s speech recognition model.
ok | Recording shortcut | Set your own voice input shortcut and choose the recognition language and any options supported by the model.
ok | Project-attached folders | Add local folders from the project home to browse them in the file panel and open their files from conversations. Attached Git repositories appear in the project’s changes list.
```

### Conversations and memory

```ui-checklist
ok | Queued turn recovery | Saved queued turns resume with their original submission identity and settings. Goals in existing conversations also start only once.
ok | Conversation branches | Fork from a completed turn while the conversation is still running. Tool cards retained in the conversation show their final state when execution ends.
ok | Memory vector rebuild | Rebuild batches respect the embedding model’s per-request input limit. The preview shows the same batch count used during the rebuild.
```

### Models and troubleshooting

```ui-checklist
ok | Provider model discovery | Fetch candidate chat models from the selected built-in provider account and use your selection to prefill the new model form.
ok | Alibaba Cloud models | Token Plan adds Qwen4.0 Max Preview. The Beijing and Singapore Model Studio endpoints add Qwen3.8 models.
ok | Error details and retries | Error cards show a short diagnosis and let you copy the full JSON or add the current error to the retry list. Edit individual rules using keywords or regular expressions.
```
