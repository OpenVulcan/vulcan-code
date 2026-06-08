//! Shared helpers for building typed tool transcript blocks inside the GUI projection layer.
//! 图形界面投影层中构建强类型工具转录区块的共享辅助逻辑。

use crate::model::SessionMessageEntry;
use serde_json::Value;
use std::path::Path;
use vulcan_agent_view::{
    AgentViewAttributes, AgentViewDirectiveBlock, AgentViewDirectiveKind,
    AgentViewToolPresentationKind, render_directive_source,
};

use super::AgentViewBuildOptions;

/// One compact generated-attachment descriptor extracted from tool metadata for a dedicated transcript presentation family.
/// 为专属转录展示家族从工具元数据中提取出的一条紧凑生成附件描述符。
pub(crate) struct GeneratedAttachmentDescriptor {
    /// Human-readable output file name shown by the transcript card and host actions.
    /// 供转录卡片与宿主动作显示的人类可读输出文件名。
    pub(crate) file_name: String,
    /// Preferred local absolute path opened by transcript interactions, favoring the exported caller-visible copy when it still exists.
    /// 供转录交互打开的首选本地绝对路径；若调用方可见的导出副本仍存在，则优先使用它。
    pub(crate) local_path: String,
    /// Optional preview-local-path used by media cards that should display a derived preview asset.
    /// 当媒体卡片需要展示派生预览资源时使用的可选预览本地路径。
    pub(crate) preview_local_path: Option<String>,
    /// Optional mime type persisted for modality-specific host actions.
    /// 为模态专属宿主动作持久化的可选 MIME 类型。
    pub(crate) mime_type: Option<String>,
    /// Total number of generated outputs represented by the current tool result.
    /// 当前工具结果代表的生成输出总数。
    pub(crate) output_count: usize,
}

/// Resolves the effective tool presentation family for one visible transcript message.
/// 解析单条可见转录消息应使用的有效工具展示家族。
pub(crate) fn resolve_tool_presentation_kind(
    message: &SessionMessageEntry,
) -> AgentViewToolPresentationKind {
    if is_task_session_message(message) {
        return AgentViewToolPresentationKind::TaskSession;
    }

    let Some(metadata) = message.metadata.as_ref() else {
        return AgentViewToolPresentationKind::Generic;
    };
    let Some(host_result) = resolved_host_result_from_tool_metadata(metadata) else {
        return AgentViewToolPresentationKind::Generic;
    };
    match host_result.get("kind").and_then(Value::as_str) {
        Some("command_run") => AgentViewToolPresentationKind::CommandRun,
        Some("command_session") => AgentViewToolPresentationKind::CommandSession,
        Some("generated_attachments") => match generated_attachment_kind(host_result).as_deref() {
            Some("image") => AgentViewToolPresentationKind::GeneratedImage,
            Some("video") => AgentViewToolPresentationKind::GeneratedVideo,
            Some("audio") => AgentViewToolPresentationKind::GeneratedAudio,
            Some("pdf") | Some("document") => AgentViewToolPresentationKind::GeneratedDocument,
            _ => AgentViewToolPresentationKind::Generic,
        },
        _ => AgentViewToolPresentationKind::Generic,
    }
}

/// Builds the shared attribute set carried by every projected tool block before family-specific attributes are appended.
/// 构建每个投影工具区块在追加家族专属属性前都会携带的共享属性集合。
pub(crate) fn build_base_tool_attributes(
    message: &SessionMessageEntry,
    presentation_kind: AgentViewToolPresentationKind,
    options: &AgentViewBuildOptions,
) -> AgentViewAttributes {
    let mut attributes = AgentViewAttributes::new();
    attributes.insert(String::from("message-id"), message.id.clone());
    if !message.created_at.trim().is_empty() {
        attributes.insert(String::from("created-at"), message.created_at.clone());
    }
    if let Some(title) = super::localized_message_title(message, options) {
        attributes.insert(String::from("title"), title);
    }
    if let Some(tool_display_state) = message
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("tool_display_state"))
        .and_then(|value| value.as_str())
        .filter(|state| !state.trim().is_empty())
    {
        attributes.insert(
            String::from("tool-display-state"),
            tool_display_state.to_string(),
        );
    }
    presentation_kind.write_to_attributes(&mut attributes);
    attributes
}

/// Finalizes one tool directive block with an empty inline body so the main transcript can keep tool details outside the hot render path.
/// 以空内联正文完成一个工具指令区块，使主转录可以把工具详情留在热渲染路径之外。
pub(crate) fn build_tool_directive_block(
    kind: AgentViewDirectiveKind,
    attributes: AgentViewAttributes,
) -> AgentViewDirectiveBlock {
    let raw_source = render_directive_source(&kind, &attributes, "");
    AgentViewDirectiveBlock::new(kind, attributes, String::new(), raw_source, false)
}

/// Returns whether one visible transcript message belongs to the host-managed task-session family.
/// 返回单条可见转录消息是否属于宿主管理的 task-session 家族。
pub(crate) fn is_task_session_message(message: &SessionMessageEntry) -> bool {
    message.metadata.as_ref().is_some_and(|metadata| {
        metadata.get("tool_name").and_then(Value::as_str) == Some("spawn_agent")
            || metadata.get("kind").and_then(Value::as_str) == Some("task_session")
    })
}

/// Extracts the task-session payload object from one tool transcript message when available.
/// 当存在时，从单条工具转录消息中提取 task-session payload 对象。
pub(crate) fn task_session_payload(message: &SessionMessageEntry) -> Option<&Value> {
    message.metadata.as_ref()?.get("payload")
}

/// Extracts the normalized host-result payload from one tool transcript message when the payload kind matches the requested stable identifier.
/// 当载荷 kind 匹配请求的稳定标识时，从单条工具转录消息中提取规范化宿主结果 payload。
pub(crate) fn host_result_payload_by_kind<'a>(
    message: &'a SessionMessageEntry,
    expected_kind: &str,
) -> Option<&'a Value> {
    let metadata = message.metadata.as_ref()?;
    let host_result = resolved_host_result_from_tool_metadata(metadata)?;
    (host_result.get("kind").and_then(Value::as_str) == Some(expected_kind))
        .then(|| host_result.get("payload"))
        .flatten()
}

/// Extracts one generated attachment descriptor of the requested modality from tool metadata when available.
/// 当存在时，从工具元数据中提取指定模态的一条生成附件描述符。
pub(crate) fn generated_attachment_descriptor(
    metadata: &Value,
    expected_kind: &str,
) -> Option<GeneratedAttachmentDescriptor> {
    let host_result = resolved_host_result_from_tool_metadata(metadata)?;
    if host_result.get("kind").and_then(Value::as_str) != Some("generated_attachments") {
        return None;
    }
    let outputs = host_result
        .get("payload")
        .and_then(|payload| payload.get("outputs"))
        .and_then(Value::as_array)?;
    let output = outputs.iter().find(|output| {
        output
            .get("kind")
            .and_then(Value::as_str)
            .is_some_and(|value| value.trim().eq_ignore_ascii_case(expected_kind))
    })?;
    let file_name = output
        .get("file_name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let local_path = output
        .get("local_path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let source_path = output
        .get("source_path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let preview_local_path = output
        .get("preview_local_path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let mime_type = output
        .get("mime_type")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let preferred_local_path =
        preferred_generated_attachment_path(source_path.as_deref(), &local_path).to_string();

    Some(GeneratedAttachmentDescriptor {
        file_name,
        local_path: preferred_local_path,
        preview_local_path,
        mime_type,
        output_count: outputs.len(),
    })
}

/// Resolves the preferred interaction path for one generated attachment, favoring a caller-visible exported copy when it still exists on disk.
/// 解析单条生成附件的首选交互路径；若调用方可见的导出副本仍存在于磁盘上，则优先使用它。
fn preferred_generated_attachment_path<'a>(
    source_path: Option<&'a str>,
    managed_local_path: &'a str,
) -> &'a str {
    source_path
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter(|value| Path::new(value).is_file())
        .unwrap_or(managed_local_path)
}

/// Resolves the host-result payload stored on one tool transcript metadata object, accepting both nested and flattened runtime-event shapes.
/// 解析单条工具转录元数据中保存的宿主结果载荷，同时兼容嵌套与扁平两种运行时事件形状。
pub(crate) fn resolved_host_result_from_tool_metadata(metadata: &Value) -> Option<&Value> {
    metadata
        .get("host_result")
        .filter(|value| value.is_object())
        .or_else(|| {
            (metadata.get("kind").is_some() && metadata.get("payload").is_some())
                .then_some(metadata)
        })
}

/// Returns the first generated attachment kind stored on one normalized host-result payload.
/// 返回单个规范化宿主结果载荷中记录的首个生成附件模态种类。
fn generated_attachment_kind(host_result: &Value) -> Option<String> {
    host_result
        .get("payload")
        .and_then(|payload| payload.get("outputs"))
        .and_then(Value::as_array)?
        .first()?
        .get("kind")
        .and_then(Value::as_str)
        .map(|value| value.trim().to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    //! Focused generated-attachment descriptor tests for transcript rendering.
    //! 面向转录渲染的生成附件描述符定向测试。

    use super::generated_attachment_descriptor;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Builds one unique temporary file path under the system temp directory.
    /// 在系统临时目录下构建一条唯一临时文件路径。
    fn unique_temp_file_path(extension: &str) -> std::path::PathBuf {
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("vulcan-agent-view-{unique_id}.{extension}"))
    }

    /// Verifies generated-media transcript cards prefer the exported file path when that caller-visible copy still exists.
    /// 验证当调用方可见的导出副本仍然存在时，生成媒体转录卡片会优先使用该导出文件路径。
    #[test]
    fn generated_attachment_descriptor_prefers_existing_exported_path() {
        let managed_path = unique_temp_file_path("png");
        let exported_path = unique_temp_file_path("png");
        std::fs::write(&managed_path, [0_u8, 1, 2]).expect("managed file should write");
        std::fs::write(&exported_path, [3_u8, 4, 5]).expect("exported file should write");

        let descriptor = generated_attachment_descriptor(
            &json!({
                "kind": "generated_attachments",
                "payload": {
                    "outputs": [{
                        "kind": "image",
                        "file_name": "demo.png",
                        "local_path": managed_path.to_string_lossy(),
                        "source_path": exported_path.to_string_lossy(),
                        "mime_type": "image/png"
                    }]
                }
            }),
            "image",
        )
        .expect("descriptor should resolve");

        assert_eq!(
            descriptor.local_path,
            exported_path.to_string_lossy().into_owned()
        );

        let _ = std::fs::remove_file(managed_path);
        let _ = std::fs::remove_file(exported_path);
    }
}
