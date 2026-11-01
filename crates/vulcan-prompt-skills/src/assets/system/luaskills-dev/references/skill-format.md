# LuaSkill source format

This is a working summary. The authority is the developer manual of the LuaSkills version luaskills-debug embeds:
<https://github.com/LuaSkills/luaskills/blob/v0.8.0/docs/development/skill-development.md> (replace `v0.8.0` with
the `luaskills_version` that the `luaskills_debug` tool reports). Read it for any API or field not covered here.
LuaSkills rejects unknown manifest fields, so never add a field you have not seen there or in validate output.

## Layout

```text
my-skill/                 # directory name = skill_id
  skill.yaml              # required
  dependencies.yaml       # optional: tools, Lua packages, native libraries, managed runtimes
  runtime/*.lua           # entry modules
  schemas/*.json          # optional input schemas
  help/*.md               # help nodes referenced from skill.yaml
  resources/ native/ node/ python/ overflow_templates/ licenses/
  README.md LICENSE
```

Packages ship only `skill.yaml`, `dependencies.yaml`, README/LICENSE/third-party notice files and the directories
`runtime`, `node`, `python`, `native`, `help`, `overflow_templates`, `resources`, `licenses`, `schemas`. Ship any
other top-level entry with `--include <name>` on both `validate` and `package`.

## Names

`skill_id` and every `entries[].name` match `^[a-z]([a-z0-9-]*[a-z0-9])?$`. `skill.yaml` must not declare
`skill_id`; `name` is display text only. Tools are exposed as `{skill_id}-{entry_name}`.

## skill.yaml essentials

```yaml
name: My Skill
version: 0.1.0            # semver; release tags are v<version>
enable: true
entries:
  - name: hello
    description: One sentence the model reads to decide when to call the tool.
    lua_entry: runtime/hello.lua
    lua_module: hello
    help: hello           # optional help topic name
    parameters:           # simple flat input; use input_schema_file for nested input
      - name: name
        type: string
        description: Name to greet.
        required: false
help:
  main:
    description: What the skill does.
    file: help/help.md
  topics:
    - name: hello
      description: How hello works.
      file: help/hello.md
```

- `input_schema_file: schemas/<file>.json` (an object schema under `schemas/`) replaces `parameters` for nested
  input; do not combine it with inline `input_schema`.
- `config:` declares user settings (`key`, `type` string/integer/float/boolean/enum, `default`, `required`,
  `sensitive`, `description`, ...); read them with `vulcan.config.*`.
- `sqlite: { enable: true }` gives the skill its own SQLite database through `vulcan.sqlite.*`. In 0.8.0 `enable`
  is the only sqlite field, and the `lancedb` block is no longer accepted.

## Entry modules

Each entry file returns one function. It receives the decoded arguments table and returns the tool text, usually
JSON:

```lua
return function(args)
  local name = (type(args) == "table" and args.name) or "world"
  return vulcan.json.encode({ ok = true, message = "Hello, " .. name .. "!" })
end
```

Use the injected `vulcan.*` APIs instead of guessing paths or shelling out: `vulcan.fs`, `vulcan.path`,
`vulcan.process.exec`, `vulcan.json`, `vulcan.runtime.log`, `vulcan.context` (request, client budget,
`skill_dir`), `vulcan.deps` (installed dependency paths), `vulcan.config`, `vulcan.sqlite`. Errors raised with
`error(...)` reach the caller as tool errors; report them, do not swallow them.

## dependencies.yaml

Groups: `tool_dependencies` (executables), `lua_dependencies`, `ffi_dependencies` (native libraries),
`node_runtime` / `python_runtime` (managed child runtimes). Each dependency has `name`, `version`, optional
`required` (default `true`), a `source` and per-platform `packages` keyed by `windows-x64`, `windows-arm64`,
`linux-x64`, `linux-arm64`, `macos-x64`, `macos-arm64`.

- `source.type: github_release` downloads `asset_name` from `source.github.repo` at install time.
- `source.type: bundled` takes the payload from the package: each platform's `path` points inside the skill
  (for example `native/windows-x64/foo.dll`), optionally pinned with `sha256`. validate reports
  `bundled_payload_missing` when a declared file does not exist and `bundled_payload_not_packaged` when it lies
  outside the shipped entries.
- Code loads installed files through `vulcan.deps` paths, the same way for both sources.

Use the platform layout only when bundled payloads for several systems make the package large.
