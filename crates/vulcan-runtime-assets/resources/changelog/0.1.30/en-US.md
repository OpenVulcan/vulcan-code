Vulcan Code 0.1.30 adds Economy Mode, ChatGPT plan sign-in, and configuration transfer, with improvements to MCP, the built-in browser, and queued-message recovery.

### Sessions and context

```ui-checklist
ok | Economy Mode per session | Set a history window for regular sessions. By default, when 40 turns have settled, the oldest 20 eligible turns can be dropped from future model requests. The full timeline stays visible. Enabling the mode on an existing session may make one additional model request.
ok | Send queued messages after a failed turn | Once the failed turn has settled, you can send a selected queued message with its original attachments and model settings. The request inspector shows turn summaries, retained history, and memory expiry.
```

### Accounts and models

```ui-checklist
ok | Separate OpenAI sign-in options | Connect OpenAI API, Codex, and ChatGPT plan accounts separately. Codex sign-in and quota checks work over HTTP again. ChatGPT plan support includes fixes for attachments and some streaming responses.
ok | Transfer selected model service settings | Export selected provider credentials and custom providers in a password-protected `.vc_mcfg` file. Accounts with rotating tokens require sign-in on the destination device.
ok | Usage and image editing | Check StepFun Step Plan usage in either region. Edit images with GPT Image 2 or 2.5 through the OpenAI API, or edit from reference images with GPT Image 2 through Codex.
```

### Tools and workspace

```ui-checklist
ok | Project-scoped MCP and LuaSkills shortcuts | Restrict MCP servers to selected projects and make them available on demand. Pick an available MCP server or LuaSkills entry from the composer's `#` menu. Session permissions still apply.
ok | Direct interaction in the browser | Interact directly with pages in the built-in browser. Long conversations keep your reading position more reliably, and command cards show elapsed time and the timeout setting together.
```

### Upgrading

```ui-checklist
ok | Automatic data upgrades | The app upgrades its database and older session files as it opens them and saves a pre-upgrade backup. Existing regular sessions stay in Standard mode until you enable Economy Mode.
ok | Windows installation and retry | Setup 0.2.19 installs the Visual C++ x64 runtime when it is missing. If the startup check fails, setup retains the verified files and re-downloads only missing or damaged content on retry.
```
