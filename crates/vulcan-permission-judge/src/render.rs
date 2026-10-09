//! The rendered call text the judge reads.
//! 判定器读取的调用渲染文本。

/// Renders one gated call exactly as the model was trained to read it.
/// 按模型训练时的读取形状渲染一次受管控调用。
///
/// `project_root` is omitted when unknown; `raw_arguments` is the argument string the agent model
/// sent, passed through unchanged so key order and spelling match what would run.
/// 项目根目录未知时省略；`raw_arguments` 是代理模型发送的参数字符串，原样传入，使键顺序与写法
/// 与将要执行的内容一致。
pub fn render_call(project_root: Option<&str>, tool: &str, raw_arguments: &str) -> String {
    let mut rendered = String::new();
    if let Some(root) = project_root {
        rendered.push_str("Project: ");
        rendered.push_str(root);
        rendered.push('\n');
    }
    rendered.push_str("Tool: ");
    rendered.push_str(tool);
    rendered.push_str("\nArguments: ");
    rendered.push_str(raw_arguments);
    rendered
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_line_is_omitted_when_unknown() {
        assert_eq!(
            render_call(Some("/w/demo"), "shell_command", r#"{"command":"ls"}"#),
            "Project: /w/demo\nTool: shell_command\nArguments: {\"command\":\"ls\"}"
        );
        assert_eq!(
            render_call(None, "shell_command", r#"{"b":1,"a":2}"#),
            "Tool: shell_command\nArguments: {\"b\":1,\"a\":2}"
        );
    }
}
