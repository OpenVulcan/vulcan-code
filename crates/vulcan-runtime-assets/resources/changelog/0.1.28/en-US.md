Vulcan Code 0.1.28 adds dedicated workspaces and call history for external MCP clients and updates managed ROOT skills at startup when they fall below their required versions.

### External MCP

```ui-checklist
ok | Dedicated projects and sessions | External clients can register working directories and create their own sessions. File, CodeKit, shell, and knowledge tools use the workspace bound to the session ID.
ok | Background commands and terminals | With the relevant grants, clients can run background commands, inspect or stop jobs, and use terminal sessions from an MCP session.
ok | Optional call history | Enable tool call logging per client to review arguments, results, and status. Filter the history by client, project, or session.
```

### Upgrading

```ui-checklist
ok | Standalone MCP calls now require a session ID | Register a project with `vc_mcp_project`, create a session with `vc_mcp_session`, and pass its `sid` to standalone tools. The session determines the working directory.
```

### Skills and developer verification

```ui-checklist
ok | Managed ROOT skills update at startup | CodeKit, Vulcan Curl, and Vulcan Work update from their configured sources when their installed versions are below the required minimum. A failed update keeps the usable version and is retried at the next start.
ok | VMM policy HTTP acceptance check | Developers can run `make test-vmm-http` to exercise the WebUI policy controls together with a local service over HTTP.
```
