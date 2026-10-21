// Code-mode `exec` and `wait` calls in the transcript, mirroring the WebUI code-mode card: the row
// names the script or the observed cell, and the output drops image payloads the terminal cannot show.
// 会话记录中的代码模式 `exec` 与 `wait` 调用，对应 WebUI 代码模式卡：该行标明脚本或被观察的单元，
// 输出去掉终端无法显示的图片载荷。

use serde_json::Value;

use crate::tr;

/// Marker of a tool result that carries images for the model.
/// 携带交给模型之图片的工具结果标记。
const MEDIA_MARKER: &str = "$vulcan_tool_result_media";

/// Pragma line that may open an `exec` script.
/// 可能位于 `exec` 脚本开头的编译指示行。
const EXEC_PRAGMA: &str = "// @exec:";

/// Returns whether one call is a code-mode `exec` or `wait` call; both carry their own argument shape.
/// 返回一次调用是否为代码模式 `exec` 或 `wait` 调用；两者各有专属参数形状。
pub fn is_code_mode_call(name: &str, input: &Value) -> bool {
    match name {
        "exec" => input.get("code").is_some_and(Value::is_string),
        "wait" => input.get("cell_id").is_some_and(Value::is_string),
        _ => false,
    }
}

/// Names a code-mode call by the script's first line, or by the cell a `wait` observes.
/// 以脚本首行或 `wait` 观察的单元命名代码模式调用。
pub fn code_mode_subject(input: &Value) -> String {
    if let Some(code) = input.get("code").and_then(Value::as_str) {
        return code
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty() && !line.starts_with(EXEC_PRAGMA))
            .unwrap_or_default()
            .to_string();
    }
    input
        .get("cell_id")
        .and_then(Value::as_str)
        .map(|id| tr!("toolCard.codeMode.cell", id = id))
        .unwrap_or_default()
}

/// Returns the text of a code-mode result, with each image replaced by a short label.
/// 返回代码模式结果的文本，每张图片以简短标签替代。
pub fn code_mode_output(output: &str) -> String {
    if !output.starts_with('{') || !output.contains(MEDIA_MARKER) {
        return output.to_string();
    }
    let Ok(envelope) = serde_json::from_str::<Value>(output) else {
        return output.to_string();
    };
    let Some(content) = envelope
        .get(MEDIA_MARKER)
        .and_then(Value::as_u64)
        .filter(|version| *version == 1)
        .and_then(|_| envelope.get("content"))
        .and_then(Value::as_array)
    else {
        return output.to_string();
    };
    let mut text = String::new();
    let mut images = 0;
    for block in content {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                // The first block is the header, which already ends with a line break.
                // 第一个块是标题，其本身以换行结尾。
                if !text.is_empty() && !text.ends_with('\n') {
                    text.push('\n');
                }
                text.push_str(
                    block
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                );
            }
            Some("image") => {
                images += 1;
                if !text.is_empty() && !text.ends_with('\n') {
                    text.push('\n');
                }
                text.push_str(&format!(
                    "[{}]",
                    tr!("toolCard.codeMode.image", index = images)
                ));
            }
            _ => {}
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// `exec` and `wait` are recognised by their argument shape, never by name alone.
    /// `exec` 与 `wait` 按参数形状识别，绝不只凭名称。
    #[test]
    fn code_mode_calls_are_recognised_by_argument_shape() {
        assert!(is_code_mode_call("exec", &json!({"code": "text(1)"})));
        assert!(is_code_mode_call("wait", &json!({"cell_id": "3"})));
        assert!(!is_code_mode_call("wait", &json!({"seconds": 3})));
        assert!(!is_code_mode_call("shell_command", &json!({"code": "x"})));
    }

    /// The subject skips the exec pragma and blank lines.
    /// 主旨跳过 exec 编译指示与空行。
    #[test]
    fn subject_skips_the_exec_pragma() {
        let input = json!({"code": "// @exec: {\"yield_time_ms\": 100}\n\nconst a = 1;\n"});
        assert_eq!(code_mode_subject(&input), "const a = 1;");
    }

    /// Image payloads never reach the terminal; text keeps its order.
    /// 图片载荷绝不进入终端；文本保持原有顺序。
    #[test]
    fn output_replaces_images_with_labels() {
        let envelope = json!({
            MEDIA_MARKER: 1,
            "content": [
                {"type": "text", "text": "Script completed\nWall time 0.1 seconds\nOutput:\n"},
                {"type": "text", "text": "chart ready"},
                {"type": "image", "data": "iVBORw0KGgo=", "mimeType": "image/png"},
            ],
        })
        .to_string();
        let output = code_mode_output(&envelope);
        assert!(
            output.starts_with("Script completed\nWall time 0.1 seconds\nOutput:\nchart ready\n[")
        );
        assert!(!output.contains("iVBORw0KGgo"));
        assert_eq!(code_mode_output("plain"), "plain");
    }
}
