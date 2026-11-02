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
- `continue_on_error` (optional, default false): when true, a failure of this step is still reported as 'failure' but does not make `failure()` true and does not skip later default steps.
- `${{ expression }}` inside any argument string inserts a value. A string that is exactly one `${{ }}` keeps the value's JSON type (number, boolean, object); a `${{ }}` mixed with other text is inserted as text.

Expressions:
- `steps.<id>.outcome` is 'success', 'failure' or 'skipped'.
- `steps.<id>.output` is the text you would see for a direct call (trimmed). For commands it includes headers such as the exit code, and long output may be replaced by a page file notice, so prefer the fields below.
- `steps.<id>.stdout` and `steps.<id>.stderr` are a command's raw standard output and error without trailing whitespace, available even when the visible output was paged. `steps.<id>.exit_code` is a command's exit code, otherwise null.
- `steps.<id>.result` is the structured JSON a tool returned instead of text (for example an MCP tool that returns structured content only), otherwise null.
- Operators: `==`, `!=`, `&&`, `||`, `!`, parentheses. Strings use single quotes ('it''s' escapes a quote).
- Functions: success(), failure(), always(), contains(text_or_array, value), startsWith(text, prefix), test(text, regex) is true when the regex matches, match(text, regex) returns the first capture group (or the whole match) and fails the step when nothing matches, fromJSON(text) parses JSON so you can read `.field` or `[0]`.
- Every `steps.<id>` must name an earlier step, even inside a branch that would not be evaluated; the whole plan is checked before anything runs.

Commands:
- A `shell_command` `command` only accepts numbers and booleans from `${{ }}`. Text from an earlier step must not become shell syntax, so pass it through `stdin` and read standard input in the command.

Behavior:
- Each step goes through the same permission checks as a direct call. If a step is denied, the chain stops, whatever `if` or `continue_on_error` say; retry that step on its own as a normal tool call if it is still needed.
- A failed command (non-zero exit code) or tool error marks the step as failed; later steps with the default `if` are skipped.
- If an expression cannot be evaluated, that step fails without calling its tool.
- The result starts with SUCCEEDED, FAILED or STOPPED and lists every step with its outcome and output. A failure stays FAILED even if a later `failure()` step succeeds; use `continue_on_error` for a failure you expect.
- Nothing is rolled back: steps that already ran keep their effects. Do not chain long sequences of writes, deletes or publishing; check the result and continue with direct calls instead.
- Keep verification steps targeted (one test, one package) rather than whole-project builds.

Example: build, then test only if the build succeeded, then show the log if anything failed:
{"steps":[{"id":"build","tool":"shell_command","arguments":{"command":"cargo build -p app"}},{"id":"test","tool":"shell_command","arguments":{"command":"cargo test -p app parser"}},{"tool":"file_read","if":"failure()","arguments":{"path":"target/test.log"}}]}

Example: find a file, then read it:
{"steps":[{"id":"find","tool":"shell_command","arguments":{"command":"rg -l \"fn parse_config\" src"}},{"tool":"file_read","arguments":{"path":"${{ match(steps.find.stdout, '^(.+)') }}"}}]}

Example: pass an earlier command's output to another command through stdin, allowing the lint step to fail:
{"steps":[{"id":"lint","tool":"shell_command","continue_on_error":true,"arguments":{"command":"cargo clippy -p app --message-format short"}},{"tool":"shell_command","if":"test(steps.lint.stderr, 'warning:')","arguments":{"command":"grep -c warning:","stdin":"${{ steps.lint.stderr }}"}}]}"#,
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
                        "continue_on_error": {
                            "type": "boolean",
                            "description": "Optional. When true, a failure of this step does not make failure() true or skip later steps. Default: false."
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
