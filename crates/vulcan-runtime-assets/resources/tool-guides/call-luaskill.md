### Calling an implicit LuaSkill

First call `luaskill` with the complete canonical name from the enabled implicit
LuaSkills catalog. Read the returned usage and input schema before dispatching.
Do not guess a short entry name, a package alias, or arguments from another version.

Call `call_luaskill` with the same complete canonical name and an `args` array.
Every array element contains exactly a parameter `name` and a string `value`.
Use each parameter name only once. Encode object and array values as JSON text;
encode numbers and booleans as text matching the target schema. String parameters
remain literal strings. The host restores supported JSON types from that schema.
Use an empty array when the selected entry has no arguments.

For example, after discovering an entry with string `query` and integer `limit`
parameters, the dispatcher argument shape is:

```json
{"name":"example-package-search","args":[{"name":"query","value":"source ownership"},{"name":"limit","value":"4"}]}
```

The example name is illustrative, not an installed capability. Dispatch only a
name currently returned by discovery. Required fields and additional constraints
come from that entry's schema and help. Do not invent defaults or bypass a refused
call by selecting another entry. Report an explicit host or skill failure rather
than treating it as a successful empty result. Missing skill configuration must
be supplied through the supported configuration UI, not copied into tool hints.
