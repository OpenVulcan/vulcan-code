### Calling an implicit LuaSkills tool

Read the bundled guide with `skill` when the task matches its description.
Use the public tool name and parameters shown in the guide. Supply `args` as
name/value pairs with string values. Encode arrays and objects as JSON text.
Use an empty array for a call without arguments.

For a guide that documents a string parameter and an integer parameter:

```json
{"name":"example_package_search","args":[{"name":"query","value":"source ownership"},{"name":"limit","value":"4"}]}
```
