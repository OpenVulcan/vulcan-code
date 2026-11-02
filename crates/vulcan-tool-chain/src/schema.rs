//! Model-facing description and JSON Schema of `tool_chain`.
//! `tool_chain` 面向模型的说明与 JSON Schema。

use serde_json::{Value, json};

/// Builds the English description that carries the whole usage guide.
/// 构建承载完整用法说明的英文描述。
///
/// The step limit is deliberately absent: it is a setting that may change mid-session, and the
/// description must stay stable so the cached request prefix is not broken.
/// 步骤上限刻意不写入：它是会话中途可能变化的设置，而描述必须保持稳定，避免打破请求前缀缓存。
pub fn tool_chain_description() -> String {
    String::from(
        r#"Run several tool calls in order, where later calls depend on earlier ones. Use it when the next call needs the success, failure or output of a previous call, so you do not have to wait for a round trip; independent calls should still be made in parallel as normal tool calls.

Steps use GitHub Actions syntax:
- `tool`: the name of the tool to call, exactly as you would call it directly. `arguments`: that tool's normal arguments as a JSON object.
- `id` (optional): a name for referring to this step later.
- `if` (optional): when to run the step. The default is `success()`: run only if no earlier step failed. Use `failure()` to run only after a failure, `always()` to run regardless, or any expression.
- `${{ expression }}` inside any argument string inserts a value. A string that is exactly one `${{ }}` keeps the value's JSON type.

Expressions:
- `steps.<id>.outcome` is 'success', 'failure' or 'skipped'; `steps.<id>.output` is the step's output text (trimmed); `steps.<id>.exit_code` is the exit code of command tools, otherwise null.
- Operators: `==`, `!=`, `&&`, `||`, `!`, parentheses. Strings use single quotes ('it''s' escapes a quote).
- Functions: success(), failure(), always(), contains(text_or_array, value), startsWith(text, prefix), match(text, regex) returns the first capture group (or the whole match) and fails the step when nothing matches, fromJSON(text) parses JSON so you can read `.field` or `[0]`.

Behavior:
- Each step goes through the same permission checks as a direct call. If a step is denied, the chain stops; retry that step on its own as a normal tool call if it is still needed.
- A failed command (non-zero exit code) or tool error marks the step as failed; later steps with the default `if` are skipped.
- If an expression cannot be evaluated, that step fails without calling its tool.
- The result lists every step with its outcome and output.
- Keep verification steps targeted (one test, one package) rather than whole-project builds.

Example: build, then test only if the build succeeded, then show the log if anything failed:
{"steps":[{"id":"build","tool":"shell_command","arguments":{"command":"cargo build -p app"}},{"id":"test","tool":"shell_command","arguments":{"command":"cargo test -p app parser"}},{"tool":"file_read","if":"failure()","arguments":{"path":"target/test.log"}}]}

Example: find a file, then read it:
{"steps":[{"id":"find","tool":"shell_command","arguments":{"command":"rg -l \"fn parse_config\" src"}},{"tool":"file_read","arguments":{"path":"${{ match(steps.find.output, '^(\\S+)') }}"}}]}"#,
    )
}

/// Builds the JSON Schema advertised for `tool_chain`.
/// 构建 `tool_chain` 对外声明的 JSON Schema。
pub fn tool_chain_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "steps": {
                "type": "array",
                "minItems": 1,
                "description": "Steps run in order. Pass a JSON array of step objects, not a string.",
                "items": {
                    "type": "object",
                    "properties": {
                        "id": {
                            "type": "string",
                            "description": "Optional step name used in `steps.<id>.*` by later steps. Letters, digits, `_` and `-`; must not start with a digit or `-`."
                        },
                        "tool": {
                            "type": "string",
                            "description": "Tool to call, by the same name used for a direct call."
                        },
                        "if": {
                            "type": "string",
                            "description": "Optional condition such as \"failure()\" or \"steps.test.outcome == 'success'\". Default: success()."
                        },
                        "arguments": {
                            "type": "object",
                            "description": "The tool's normal arguments. Strings may contain ${{ expression }}."
                        }
                    },
                    "required": ["tool", "arguments"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["steps"],
        "additionalProperties": false
    })
}
