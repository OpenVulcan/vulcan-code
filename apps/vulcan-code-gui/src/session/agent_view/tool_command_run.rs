//! Dedicated command-run tool transcript projection helpers.
//! 专属命令执行工具转录投影辅助逻辑。

use super::AgentViewBuildOptions;
use super::tool_shared::{
    build_base_tool_attributes, build_tool_directive_block, host_result_payload_by_kind,
};
use crate::model::SessionMessageEntry;
use vulcan_agent_view::{
    AgentViewDirectiveBlock, AgentViewDirectiveKind, AgentViewToolPresentationKind,
};

/// Builds one command-run tool transcript block with the command execution attributes required by the GUI summary row and future command-centric renderers.
/// 构建一条携带命令执行属性的 command-run 工具转录区块，供 GUI 摘要行与后续命令专属渲染器使用。
pub(crate) fn build_command_run_tool_block(
    kind: AgentViewDirectiveKind,
    message: &SessionMessageEntry,
    options: &AgentViewBuildOptions,
) -> AgentViewDirectiveBlock {
    let mut attributes =
        build_base_tool_attributes(message, AgentViewToolPresentationKind::CommandRun, options);
    if let Some(payload) = host_result_payload_by_kind(message, "command_run") {
        append_command_run_payload_attributes(&mut attributes, payload);
    }
    build_tool_directive_block(kind, attributes)
}

/// Appends one normalized `command_run` payload onto the outgoing directive attribute map.
/// 将一份规范化 `command_run` 载荷追加到输出指令属性映射上。
fn append_command_run_payload_attributes(
    attributes: &mut vulcan_agent_view::AgentViewAttributes,
    payload: &serde_json::Value,
) {
    append_string_attribute(
        attributes,
        "command-run-status",
        payload.get("status").and_then(serde_json::Value::as_str),
    );
    append_string_attribute(
        attributes,
        "command-shell",
        payload.get("shell").and_then(serde_json::Value::as_str),
    );
    append_string_attribute(
        attributes,
        "command-working-directory",
        payload.get("cwd").and_then(serde_json::Value::as_str),
    );
    append_string_attribute(
        attributes,
        "command-text",
        payload.get("command").and_then(serde_json::Value::as_str),
    );
    append_string_attribute(
        attributes,
        "command-failure-reason",
        payload
            .get("failure_reason")
            .and_then(serde_json::Value::as_str),
    );
    append_numeric_attribute(
        attributes,
        "command-exit-code",
        payload.get("exit_code").and_then(serde_json::Value::as_i64),
    );
    append_numeric_attribute(
        attributes,
        "command-duration-ms",
        payload
            .get("duration_ms")
            .and_then(serde_json::Value::as_u64),
    );
    append_numeric_attribute(
        attributes,
        "command-stdout-bytes",
        payload
            .get("stdout_bytes")
            .and_then(serde_json::Value::as_u64),
    );
    append_numeric_attribute(
        attributes,
        "command-stderr-bytes",
        payload
            .get("stderr_bytes")
            .and_then(serde_json::Value::as_u64),
    );
    append_boolean_attribute(
        attributes,
        "command-output-overflowed",
        payload
            .get("combined_output_overflowed")
            .and_then(serde_json::Value::as_bool),
    );
}

/// Appends one non-empty string attribute into the outgoing directive attribute map.
/// 将一条非空字符串属性追加到输出指令属性映射上。
fn append_string_attribute(
    attributes: &mut vulcan_agent_view::AgentViewAttributes,
    key: &str,
    value: Option<&str>,
) {
    if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
        attributes.insert(key.to_string(), value.to_string());
    }
}

/// Appends one numeric attribute into the outgoing directive attribute map.
/// 将一条数字属性追加到输出指令属性映射上。
fn append_numeric_attribute(
    attributes: &mut vulcan_agent_view::AgentViewAttributes,
    key: &str,
    value: Option<impl ToString>,
) {
    if let Some(value) = value {
        attributes.insert(key.to_string(), value.to_string());
    }
}

/// Appends one boolean attribute into the outgoing directive attribute map.
/// 将一条布尔属性追加到输出指令属性映射上。
fn append_boolean_attribute(
    attributes: &mut vulcan_agent_view::AgentViewAttributes,
    key: &str,
    value: Option<bool>,
) {
    if let Some(value) = value {
        attributes.insert(key.to_string(), value.to_string());
    }
}
