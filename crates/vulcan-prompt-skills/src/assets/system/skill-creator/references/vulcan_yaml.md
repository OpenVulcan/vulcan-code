# vulcan.yaml fields (full example + descriptions)

`vulcan.yaml` is VulcanCode-specific UI metadata stored in the skill root. It is intended for the application to read, not the agent.

## Full example

```yaml
schema_version: 3
interface:
  display_name: "Optional user-facing name"
  short_description: "Optional user-facing description"
  emoji: "🧭"
  color: "#3B82F6"
  locales:
    zh-CN:
      display_name: "可选显示名称"
      short_description: "可选显示说明"
catalog:
  category_id: "uncategorized"
  tags: []
origin:
  kind: "local-created"
  managed: true
```

## Field descriptions and constraints

Top-level constraints:

- Quote all string values.
- Keep keys unquoted.
- Keep this file focused on the fields listed below. Ignore upstream or third-party skill metadata fields unless VulcanCode explicitly implements them later.

- `interface.display_name`: Human-facing title shown in UI skill lists and chips.
- `interface.short_description`: Human-facing short UI blurb for quick scanning. Keep it concise; 80 characters or fewer is recommended.
- `interface.emoji`: Optional user-maintainable emoji rendered by the agent-style marker.
- `interface.color`: Optional hexadecimal RGB or RGBA marker color.
