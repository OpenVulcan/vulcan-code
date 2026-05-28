# call_luaskill Usage

Use `call_luaskill` only for enabled LuaSkills entries that are available by host dispatch but not exposed as direct function tools.

The target is always the complete canonical tool name, such as `vulcan-codekit-ast-tree`. Do not split it into skill id and entry name.

Required fields:

- `name`: complete canonical LuaSkills tool name.
- `args`: argument pairs for the target tool.

Argument rule:

- Each item in `args` has `name` and `value`.
- `value` is always text.
- For string parameters, write the raw text.
- For integer, number, or boolean parameters, write the literal as text, such as `60000` or `true`.
- For object or array parameters, write JSON text inside `value`.

Example intent:

- Target tool: `vulcan-curl-get`
- Parameter `url`: `https://example.com`
- Parameter `headers`: `{"Accept":"application/json"}` as JSON text

Important:

- Do not copy the original LuaSkill schema into this dispatcher call.
- Do not put nested JSON directly in `value`; encode it as text.
- Do not invent host-managed fields such as `LUASKILL_SID` or `PWD`.
- If the target LuaSkill is disabled, unavailable, or hidden from dispatch, the call must fail.
