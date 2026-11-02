---
name: runlua
description: Required operating guide for the runlua and runlua_session tools. Before either tool is used, load this guide with the native skill tool using name runlua; never use luaskill to retrieve it.
---

# RunLua

Before calling `runlua` or `runlua_session`, load this guide through the native prompt-Skill tool:

```json
{"name":"runlua"}
```

The tool id is `skill`, singular. It reads prompt Skills. Do not call `luaskill`, a LuaSkills entry, or LuaSkills help to retrieve this guide.

## Choose the Execution Mode

- Use `runlua` for one isolated calculation or transformation that does not need retained Lua state.
- Use `runlua_session` when globals, required modules, or intermediate values must survive across multiple Lua evaluations.
- Use `shell_command` for ordinary command-line work. RunLua is not a replacement shell.
- Close persistent Lua sessions when their retained state is no longer needed.

## Runtime Versions

- LuaSkills: `0.8.1`.
- mlua: `0.12.0`.
- LuaJIT: `LuaJIT 2.1.ROLLING`.
- LuaJIT source: `luajit-src 210.7.2+b925b3e`.
- Language baseline: Lua 5.1 VM with Lua 5.2 compatibility enabled.

Do not assume Lua 5.3 or Lua 5.4-only APIs such as the standard `utf8` library.

## Guaranteed Runtime Surface

Lua states are created with mlua `StdLib::ALL`. The standard package, debug, IO, OS, and LuaJIT libraries are available, including LuaJIT `jit`, `bit`, and FFI facilities.

The runtime also registers these Vulcan API groups:

- `vulcan.fs`: file and directory operations.
- `vulcan.io`: managed open, text read/write/append, lines, popen, and tmpfile operations.
- `vulcan.path`: join, dirname, basename, stem, extname, normalize, and absolute-path checks.
- `vulcan.process`: launcher discovery, process execution, and executable lookup.
- `vulcan.json`: JSON encoding and decoding, typed `object()` / `array()` constructors, and the `null` sentinel. Use the typed values when empty containers or JSON null must survive a Lua round trip.
- `vulcan.runtime.python`: Python status, one-shot invoke, and persistent sessions.
- `vulcan.runtime.node`: Node.js status, one-shot invoke, and persistent sessions.
- `vulcan.host`: host tool discovery and invocation.
- `vulcan.models`: model availability, embedding, and LLM calls.
- `vulcan.call`: registered LuaSkills entry invocation.

Additional Lua or native modules are installation-dependent. When module availability is not guaranteed, probe it explicitly:

```lua
local ok, module_or_error = pcall(require, "module_name")
return { available = ok, detail = tostring(module_or_error) }
```

## SQLite

Anonymous RunLua execution does not receive a Skill-owned SQLite binding. Direct `vulcan.sqlite` operations are unavailable in this anonymous context.

Use a registered LuaSkills entry whose manifest explicitly enables SQLite, and invoke that entry through `vulcan.call`. Do not attempt to bypass database ownership by guessing filesystem database paths.

LuaSkills provides no LanceDB or vector-database capability: `vulcan.lancedb` does not exist in any context.

## One-Shot `runlua`

Required arguments:

- `task`: short label shown in the conversation card.
- `code`: inline Lua source.

Optional arguments:

- `args`: JSON object exposed as local `args`.
- `timeout_ms`: Lua instruction timeout from 1 to 60000; default 60000.

Example:

```json
{
  "task": "Summarize numeric values",
  "code": "local total = 0\nfor _, value in ipairs(args.values) do total = total + value end\nprint('count', #args.values)\nreturn total, total / #args.values",
  "args": {"values":[2,4,6,8]},
  "timeout_ms": 60000
}
```

One-shot behavior:

- Runs in a dedicated pooled VM whose request state is cleared after use.
- Binds relative filesystem and process paths to the current workspace; without a workspace, execution uses an isolated temporary directory.
- Captures `print` output.
- Preserves multiple returned Lua values in the rendered result.
- Disables JIT for the request.
- Does not expose file-mode execution; provide inline source only.
- Disables `vulcan.runtime.log`, persistent `vulcan.cache` access, and recursive RunLua execution.

Treat a completed response as execution completion, not proof that the Lua program succeeded. The LuaSkills one-shot API renders Lua runtime errors into its Markdown result.

## Persistent `runlua_session`

Supported actions:

- `list`: list active sessions owned by the current conversation.
- `start`: create one persistent Lua runtime lease.
- `eval`: execute Lua in an owned lease.
- `status`: query one owned lease.
- `close`: release one owned lease.

### Start

```json
{
  "action": "start",
  "name": "csv-analysis",
  "ttl_sec": 600
}
```

The host injects the current workspace as `cwd` and `workspace_root`. The tool does not accept caller-provided Lua roots, native roots, or mounts.

### Evaluate

```json
{
  "action": "eval",
  "session_id": "opaque id returned by start",
  "code": "counter = (counter or 0) + args.increment\nreturn { counter = counter }",
  "args": {"increment":1},
  "timeout_ms": 60000
}
```

Persistent eval returns one JSON-compatible Lua value. Use `return`; `print` output is not captured by the public lease eval API. Functions, threads, userdata, and cyclic tables are not JSON-compatible results.

### Inspect or Close

```json
{"action":"status","session_id":"opaque id"}
```

```json
{"action":"close","session_id":"opaque id"}
```

Persistent-session behavior:

- Globals and `package.loaded` survive successful eval calls.
- Default idle TTL is 600 seconds; maximum TTL is 3600 seconds.
- Successful eval refreshes the finite idle lease.
- Sessions are private to the conversation that created them.
- A LuaSkills runtime reload invalidates all previous public lease handles.
- At most four AI-created Lua sessions may be active in one conversation.

## Timeout and Resource Limits

- Tool output is limited to 64 KiB inline. Larger output is written to a project `.vulcan/temp/` page file with bounded read instructions.
- Lua source is limited to 128 KiB per call.
- The Lua timeout is an instruction hook. It cannot reliably interrupt an already-blocking native, FFI, process, Python, or Node call.
- Give `vulcan.process.exec`, Python, and Node operations their own finite timeouts.
- LuaSkills does not provide a per-session Lua memory quota. Avoid unbounded tables, recursion, generated source, and retained binary buffers.
- FFI and all standard libraries are powerful host capabilities. Use only paths and operations required by the current task.

## Finalization

Before completing a task:

1. Return the required result in a bounded form.
2. Close temporary `runlua_session` leases.
3. Keep a session open only when the user explicitly needs later stateful continuation.
4. Do not claim a one-shot Lua program succeeded merely because its tool call completed; inspect the returned Markdown for runtime errors.
