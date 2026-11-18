Vulcan Code 0.1.27 adds a request context inspector, makes skills and plugins easier to invoke on demand, and improves subagent coordination and MCP client grants.

### Conversations and workspace

```ui-checklist
ok | Inspect request context | Open Request Context from the conversation menu to review the prompts, messages, and tool definitions used in the latest request. Before the conversation sends its first request, the inspector previews the context it would assemble.
ok | Keep the main task moving | The parent conversation can handle priority messages and receive results while subagents are still running. Returned results are included in subsequent requests.
ok | Scroll position and elapsed time | Streaming updates preserve your reading position when you scroll back. Reopening a conversation restores the elapsed time of an active request from Core events.
```

### Skills, plugins, and MCP

```ui-checklist
ok | Mention skills and plugins in the composer | Type `@` to select a skill or project path, and `#` to select a plugin. Suggestions show short names.
ok | Load tools on demand | Vulcan Browser, Context7, and session MCP connection management can be invoked on demand. LuaSkills 0.9.1 lets you switch package tools with guides to on-demand invocation.
ok | Grant MCP tools by group | Configure session, standalone, and multimodal permissions separately for each client. Grant access to individual file, shell, CodeKit, and knowledge base tools and set project or library scopes.
```

### Installation, models, and feedback

```ui-checklist
ok | Standalone installer 0.2.18 | On first install, the installer measures response times and recommends the faster download route. You can change the selection, and LuaSkills uses the Hub and GitHub download source for the selected region.
ok | LuaSkills startup diagnostics | Startup shows localized progress for dependency setup and identifies the dependency that failed to install.
ok | Qwen4 Max | The generally available Qwen4 Max is now listed for Alibaba Cloud Token Plan accounts.
ok | Optional feedback contact details | Choose whether to include and preview contact details when submitting feedback. Public replies are available in the in-app message center.
```
