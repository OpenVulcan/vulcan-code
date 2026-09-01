//! Pure provisional user-resource projection before the prompt owner prepares native delivery.
//! 在提示词所有者准备原生投递之前进行纯临时用户资源投影。

use vulcan_ai_contract::{AiMessage, ContentItem, MessageRole};
use vulcan_core::types::ConversationAttachment;

/// Projects `text` and ordered `attachments` without file reads, cache queries or native-delivery guesses.
/// 不读取文件、查询缓存或猜测原生投递，投影`text`及有序`attachments`。
/// Returns safe metadata only; a prompt-owned media projection must replace these items before native delivery.
/// 只返回安全元数据；原生投递前必须由提示词所有媒体投影替换这些条目。
pub(super) fn unprepared_user_message(
    text: &str,
    attachments: &[ConversationAttachment],
) -> AiMessage {
    // User text and duplicate attachment occurrences retain their original order.
    // 用户文本及重复附件出现保留原顺序。
    let mut content = vec![ContentItem::Text {
        text: text.to_string(),
    }];
    for attachment in attachments {
        // Canonical display facts do not prove a prior upload, inline mode, or original model.
        // 规范显示事实不能证明此前上传、内联方式或原模型。
        let metadata = serde_json::json!({
            "resource_path": safe_attachment_locator(attachment),
            "file_name": attachment.file_name,
            "mime_type": attachment.mime_type,
            "byte_len": attachment.byte_len,
            "native_delivery": "metadata_only",
            "reason": "no prompt-owned media delivery proof is available in this provisional projection"
        });
        content.push(ContentItem::Text { text: format!("ATTACHMENT: {metadata}. CONTENT_VISIBILITY: resource bytes were not provided; do not infer or guess the attachment contents.") });
    }
    AiMessage {
        role: MessageRole::User,
        content,
        provider_context_items: Vec::new(),
    }
}

/// Returns the disclosed safe locator for `attachment`, never a host path or an upstream reference.
/// 返回`attachment`已披露的安全定位符，绝不返回宿主路径或上游引用。
/// Explicit project IDs and canonical managed relative paths take precedence; otherwise use an exact staged asset ID.
/// 显式项目ID及规范托管相对路径优先；否则使用精确暂存资产ID。
fn safe_attachment_locator(attachment: &ConversationAttachment) -> String {
    if let Some(path) = attachment.resource_path.as_deref() {
        if matches!(
            vulcan_workspace_store::workspace_store::parse_project_resource_locator(path),
            Ok(Some(_))
        ) {
            return path.to_string();
        }
        // A provider URL, Data URL, absolute path or traversal cannot masquerade as a relative resource address.
        // 供应商URL、Data URL、绝对路径或路径穿越不能伪装成相对资源地址。
        if !path.is_empty()
            && !path.contains([':', '\\'])
            && !path.chars().any(char::is_control)
            && path
                .split('/')
                .all(|component| !component.is_empty() && component != "." && component != "..")
        {
            return path.to_string();
        }
    }
    // Only the explicit staged identity can supply the alternate declared locator form.
    // 只有显式暂存身份可以提供另一种声明定位符形态。
    match attachment.asset_id.as_deref().filter(|id| {
        !id.is_empty()
            && id.trim() == *id
            && !id.contains(['/', '\\', ':'])
            && !id.chars().any(char::is_control)
    }) {
        Some(id) => format!("attachment://{id}"),
        None => String::from("attachment://unavailable"),
    }
}

#[cfg(test)]
mod tests;
