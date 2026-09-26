//! Conversation error projection after trusted error-envelope validation.
//! 可信错误封套验证后的会话错误投影。

use serde_json::Value;
use vulcan_tools_manager::result_markdown::{fenced_code, inline_code};

/// Projects an already validated tool-owned failure without repeating the original arguments.
/// 投影已经验证的工具自有失败，不重复原始参数。
/// Takes the exact tool name and validated status/error/details envelope; returns readable facts.
/// 接收精确工具名及已验证的状态、错误、详情封套，返回可读事实。
pub(super) fn error_markdown(tool: &str, envelope: &Value) -> String {
    let error = envelope["error"]
        .as_str()
        .expect("error envelope validated before projection");
    let mut body = format!("# Tool failed: {}\n\n{}\n", inline_code(tool), error);
    // Domain details deliberately accept arbitrary JSON in the existing contract. Preserve nested
    // business data verbatim; flatten only scalar facts, including false and null.
    // 既有契约明确允许任意 JSON 领域详情。原样保留嵌套业务数据；仅平铺标量事实，包括假值和空值。
    match &envelope["details"] {
        Value::Object(details) => {
            for (key, value) in details {
                if key == "message" && value.as_str() == Some(error) {
                    continue;
                }
                match value {
                    Value::String(text) => {
                        body.push_str(&format!("\n- {}: {}", inline_code(key), inline_code(text)))
                    }
                    Value::Bool(_) | Value::Number(_) | Value::Null => {
                        body.push_str(&format!("\n- {}: {}", inline_code(key), value))
                    }
                    _ => body.push_str(&format!(
                        "\n\n## {}\n\n{}",
                        inline_code(key),
                        fenced_code(&value.to_string())
                    )),
                }
            }
        }
        Value::Null => {}
        details => body.push_str(&format!(
            "\n## Details\n\n{}",
            fenced_code(&details.to_string())
        )),
    }
    body.push('\n');
    body
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Ensures partial side effects and unknown facts cannot disappear during error slimming.
    /// 确保错误精简不会丢失部分副作用和未知事实。
    #[test]
    fn structured_error_keeps_partial_effects_without_duplicate_message() {
        let body = error_markdown(
            "close_agent",
            &json!({"error":"Archive failed","details":{
                "message":"Archive failed", "stopped":true,"closed":false,"archive_committed":false,
                "retryable":null,"stopped_subagent_task_ids":["child-a"]
            }}),
        );
        assert_eq!(body.matches("Archive failed").count(), 1);
        for fact in [
            "stopped",
            "true",
            "closed",
            "false",
            "archive_committed",
            "null",
            "child-a",
        ] {
            assert!(body.contains(fact), "Missing side-effect fact: {fact}");
        }
    }
}
