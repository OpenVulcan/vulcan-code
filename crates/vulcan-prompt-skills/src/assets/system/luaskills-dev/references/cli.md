# luaskills-debug commands

`luaskills-debug --help` prints the full option list of the installed version; trust it over this summary.
Add `--output json` when you need to read the result; `call` and `list-tools` also accept `--output content`
(only the tool text).

| Command | Purpose |
| --- | --- |
| `new <dir> [--layout standard\|platform] [--name <display name>]` | Create a skill from the built-in template, with a release workflow. The directory name is the `skill_id`. |
| `validate --skill-path <dir> [--skill-id <id>] [--include <name>]...` | Check the source against the packaging rules of the embedded runtime; exits non-zero on issues. |
| `package --skill-path <dir> --out-dir <dir> [--layout standard\|platform] [--no-universal] [--include <name>]... [--expect-tag <tag>]` | Build the release assets. |
| `sync --runtime-root <dir> --skill-path <dir>` | Copy the skill into `<runtime-root>/skills/<skill_id>` without loading it. |
| `inspect --runtime-root <dir> --skill-path <dir>` | Sync and load the skill; report the loading result, dependencies and errors. |
| `list-tools --runtime-root <dir> --skill-path <dir>` | Sync, load and list the registered tool names and schemas. |
| `call --runtime-root <dir> --skill-path <dir> --tool <name> [--args-json <json> \| --args-file <path>]` | Sync, load and call one entry through the host path. |
| `probe-mirrors [--github-mirror <id\|url>]...` | Measure GitHub mirror speed. |

`inspect`, `list-tools` and `call` also accept `--skill-id <id>` instead of `--skill-path` to reuse a skill already
synced into the runtime root. `--tool` takes the local entry name (`hello`) or the canonical name
(`my-skill-hello`).

## Runtime root

Use the `runtime_root` that the `luaskills_debug` tool reports. Dependencies a skill declares are downloaded into
it on the first `inspect` or `call`; state and SQLite data stay under it. Delete a skill's folder under
`<runtime-root>/skills/` only to force a clean sync.

`sync` skips `target`, `.git`, `.svn`, `node_modules`, `__pycache__` and `.luaskills-debug` at every depth, and
`dist`, `out`, `temp`, `state`, `databases`, `dependencies`, `lua_packages` and `libs` directly under the skill.

## Host context

Every `call` injects the context of one developer invocation: `request_id`, `client_name` and
`matched_client_pattern` are `luaskills-debug`, and `vulcan.context.client_budget` allows 200000 bytes for
`tool_result` and `file_read`. To reproduce a specific host profile:

- `--context-file <path>`: JSON with `request_context`, `client_budget` and `tool_config`; replaces the default.
- `--client-budget <json>` / `--tool-config <json>`: override one member; not combined with `--context-file`.
- `--enable-host-result`: add the `host_result` client capability.

## Network

Dependency downloads use GitHub releases. `--github-mirror <id|url>` (repeatable) routes them through a mirror:
built-in ids `github`, `gh-proxy-com`, `ghfast-top`, `gh-llkk-cc`, `ghproxy-net`. `--github-token-env <NAME>`
reads a token from that environment variable for api.github.com rate limits; never pass a token literally.

## SQLite and managed runtimes

- A skill with `sqlite.enable: true` gets the tool's local SQLite host automatically; data stays under
  `<runtime-root>/databases/sqlite/<skill_id>/`.
- Skills that declare managed Python or Node runtimes need the distributions first: run
  `<package_root>/scripts/deps/fetch_managed_runtimes.ps1 -Target node|python|all` (Bash:
  `fetch_managed_runtimes.sh node|python|all`), then pass
  `--managed-runtime-distribution-root` and `--managed-runtime-environment-root` (absolute paths) to `call`.
