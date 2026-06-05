//! Structured composer-part helpers shared by previews, transcript projection, and future context part expansion.
//! 供预览、转录投影与后续上下文 Part 扩展复用的结构化提示词 Part 辅助模块。

use vulcan_core::types::ConversationAttachment;

/// Structured composer input part retained by the host before it is projected into canonical conversation messages.
/// 在投影为规范会话消息之前，由宿主保留的结构化提示词输入 Part。
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComposerInputPart {
    /// Plain user-authored free-form text.
    /// 用户编写的自由文本。
    Text(String),
    /// Structured multimodal attachment staged beside the user-authored text.
    /// 与用户文本并列暂存的结构化多模态附件。
    Attachment(ConversationAttachment),
    /// Future file-context placeholder reserved for `@file`-style expansion.
    /// 为 `@file` 风格扩展预留的未来文件上下文占位 Part。
    FileContext {
        /// Stable file label shown to the user.
        /// 展示给用户的稳定文件标签。
        label: String,
    },
    /// Future selection-context placeholder reserved for highlighted ranges or code excerpts.
    /// 为高亮范围或代码摘录预留的未来选区上下文占位 Part。
    SelectionContext {
        /// Stable selection label shown to the user.
        /// 展示给用户的稳定选区标签。
        label: String,
    },
    /// Future agent-hint placeholder reserved for explicit coordinator or worker steering text.
    /// 为显式协调器或工作代理引导文本预留的未来代理提示占位 Part。
    AgentHint {
        /// Stable agent hint label shown to the user.
        /// 展示给用户的稳定代理提示标签。
        label: String,
    },
}

impl ComposerInputPart {
    /// Returns the compact human-readable label used by previews when the current part is non-textual.
    /// 返回当前 Part 为非文本类型时供预览使用的紧凑人类可读标签。
    fn preview_label(&self) -> Option<String> {
        match self {
            Self::Text(_) => None,
            Self::Attachment(attachment) => Some(format!(
                "{} `{}`",
                attachment.kind.display_label(),
                attachment.file_name
            )),
            Self::FileContext { label } => Some(format!("文件上下文 `{label}`")),
            Self::SelectionContext { label } => Some(format!("选区上下文 `{label}`")),
            Self::AgentHint { label } => Some(format!("代理提示 `{label}`")),
        }
    }
}

/// Builds the structured composer-part list from the current raw text and staged attachments.
/// 根据当前原始文本与已暂存附件构建结构化提示词 Part 列表。
pub fn build_composer_input_parts(
    text: &str,
    attachments: &[ConversationAttachment],
) -> Vec<ComposerInputPart> {
    let mut parts = Vec::new();

    if !text.is_empty() {
        parts.push(ComposerInputPart::Text(text.to_string()));
    }

    parts.extend(
        attachments
            .iter()
            .cloned()
            .map(ComposerInputPart::Attachment),
    );

    parts
}

/// Returns whether the provided composer-part list still contains any dispatchable payload.
/// 返回给定提示词 Part 列表是否仍包含任何可分发载荷。
pub fn composer_parts_have_submittable_content(parts: &[ComposerInputPart]) -> bool {
    parts.iter().any(|part| match part {
        ComposerInputPart::Text(text) => !text.trim().is_empty(),
        ComposerInputPart::Attachment(_)
        | ComposerInputPart::FileContext { .. }
        | ComposerInputPart::SelectionContext { .. }
        | ComposerInputPart::AgentHint { .. } => true,
    })
}

/// Builds one compact single-line preview used by queue entries and pending prompt panels.
/// 构建供队列条目与待发送面板使用的一条紧凑单行预览文本。
pub fn composer_parts_preview_text(parts: &[ComposerInputPart], max_chars: usize) -> String {
    let preview = composer_parts_label_text(parts);
    if preview.chars().count() <= max_chars {
        preview
    } else {
        format!(
            "{}…",
            preview
                .chars()
                .take(max_chars.saturating_sub(1))
                .collect::<String>()
        )
    }
}

/// Builds one full single-line label used by the visible pending-prompt queue panel.
/// 构建显示在可见待发送队列面板中的完整单行标签文本。
pub fn composer_parts_label_text(parts: &[ComposerInputPart]) -> String {
    let normalized_text = parts
        .iter()
        .find_map(|part| match part {
            ComposerInputPart::Text(text) => Some(normalize_preview_text(text)),
            _ => None,
        })
        .unwrap_or_default();
    let non_text_labels = parts
        .iter()
        .filter_map(ComposerInputPart::preview_label)
        .collect::<Vec<_>>();

    match (
        normalized_text.trim().is_empty(),
        non_text_labels.is_empty(),
        non_text_labels.len(),
    ) {
        (true, true, _) => String::from("空白消息"),
        (true, false, 1) => format!("[{}]", non_text_labels[0]),
        (true, false, _) => format!("[附件 {} 个]", non_text_labels.len()),
        (false, true, _) => normalized_text,
        (false, false, 1) => format!("{normalized_text} [{}]", non_text_labels[0]),
        (false, false, _) => format!("{normalized_text} [附件 {} 个]", non_text_labels.len()),
    }
}

/// Projects the current composer-part list back into the canonical user-text payload consumed by the session protocol.
/// 将当前提示词 Part 列表重新投影为会话协议消费的规范用户文本载荷。
pub fn composer_parts_to_user_payload(
    parts: &[ComposerInputPart],
) -> (String, Vec<ConversationAttachment>) {
    let text = parts
        .iter()
        .find_map(|part| match part {
            ComposerInputPart::Text(text) => Some(text.clone()),
            _ => None,
        })
        .unwrap_or_default();
    let attachments = parts
        .iter()
        .filter_map(|part| match part {
            ComposerInputPart::Attachment(attachment) => Some(attachment.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();

    (text, attachments)
}

/// Normalizes one raw preview text payload into a compact single-line representation.
/// 将一段原始预览文本规范化为紧凑单行表示。
fn normalize_preview_text(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or(text)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    //! Composer-part regression tests covering preview labels, transcript bodies, and empty-state handling.
    //! 覆盖预览标签、转录正文与空状态处理的提示词 Part 回归测试。

    use super::{
        ComposerInputPart, build_composer_input_parts, composer_parts_have_submittable_content,
        composer_parts_label_text, composer_parts_preview_text, composer_parts_to_user_payload,
    };
    use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};

    /// Builds one minimal attachment used by the composer-part tests.
    /// 构建供提示词 Part 测试使用的一条最小附件。
    fn test_attachment(file_name: &str) -> ConversationAttachment {
        ConversationAttachment {
            kind: ConversationAttachmentKind::Image,
            file_name: file_name.to_string(),
            local_path: String::from("managed/test.png"),
            source_path: Some(String::from("source/test.png")),
            asset_id: Some(String::from("asset-test")),
            byte_len: Some(4),
            preview_local_path: None,
            mime_type: String::from("image/png"),
        }
    }

    /// Builds preview output from one mixed text-and-attachment composer payload.
    /// 根据一组混合文本与附件的提示词载荷构建预览输出。
    #[test]
    fn composer_parts_render_preview_for_mixed_payload() {
        let parts = build_composer_input_parts("请总结最新改动", &[test_attachment("diagram.png")]);

        assert!(composer_parts_have_submittable_content(&parts));
        assert_eq!(
            composer_parts_label_text(&parts),
            "请总结最新改动 [图片 `diagram.png`]"
        );
        assert_eq!(
            composer_parts_preview_text(&parts, 48),
            "请总结最新改动 [图片 `diagram.png`]"
        );
    }

    /// Falls back to one attachment-only label when the current composer payload has no text.
    /// 当当前提示词载荷没有文本时，回退到仅附件标签。
    #[test]
    fn composer_parts_support_attachment_only_payloads() {
        let parts = vec![ComposerInputPart::Attachment(test_attachment(
            "diagram.png",
        ))];

        assert_eq!(composer_parts_label_text(&parts), "[图片 `diagram.png`]");
    }

    /// Projects the composer parts back into the canonical user payload without losing staged attachments.
    /// 将提示词 Part 重新投影回规范用户载荷，同时不丢失已暂存附件。
    #[test]
    fn composer_parts_round_trip_into_user_payload() {
        let parts = build_composer_input_parts("请分析附件", &[test_attachment("diagram.png")]);

        let (text, attachments) = composer_parts_to_user_payload(&parts);

        assert_eq!(text, "请分析附件");
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].file_name, "diagram.png");
    }
}
