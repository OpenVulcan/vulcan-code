//! Non-persistent accounting projection: media bytes are input resources, not prompt text.
//! 非持久记账投影：媒体字节是输入资源，不是提示词正文。

use super::*;
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use vulcan_core::types::{
    ConversationAttachment, ConversationMessage, ConversationPayload, ProviderRequestSnapshot,
};

/// Projects `snapshot` for the existing text/attachment estimator without mutating durable controller history.
/// 为既有文本及附件估算器投影`snapshot`，不修改持久控制器历史。
/// Returns borrowed ordinary input or an owned media-aware snapshot; malformed native envelopes fail explicitly.
/// 返回借用的普通输入或持有的媒体感知快照；畸形原生信封明确失败。
pub(crate) fn budget_snapshot(
    snapshot: &ProviderRequestSnapshot,
) -> Result<Cow<'_, ProviderRequestSnapshot>, String> {
    // Resolve each exact tool envelope once before allocating replacement history.
    // 分配替换历史之前，逐一解析精确工具信封。
    let envelopes = snapshot
        .history
        .iter()
        .enumerate()
        .filter_map(|(index, message)| match &message.payload {
            ConversationPayload::ToolResult { text, .. } => {
                Some(parse_envelope(text).map(|envelope| (index, envelope)))
            }
            _ => None,
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter_map(|(index, envelope)| envelope.map(|envelope| (index, envelope)))
        .collect::<std::collections::BTreeMap<_, _>>();
    if envelopes.is_empty() {
        return Ok(Cow::Borrowed(snapshot));
    }
    // Keep accounting and immutable execution snapshots separate.
    // 分离记账快照与不可变执行快照。
    let mut projected = snapshot.clone();
    projected.history.clear();
    for (index, message) in snapshot.history.iter().enumerate() {
        // Ordinary messages retain their exact payload and identity.
        // 普通消息保留精确载荷及身份。
        let mut message = message.clone();
        if let Some(envelope) = envelopes.get(&index) {
            if let ConversationPayload::ToolResult { text, .. } = &mut message.payload {
                *text = envelope.text.clone();
            }
            projected.history.push(message.clone());
            // Reuse the existing attachment estimator instead of charging Base64 characters as text tokens.
            // 复用既有附件估算器，不把Base64字符计为文本Token。
            let resource = &envelope.resource;
            // MIME was validated when the native result was produced; malformed persisted values fail here too.
            // 原生结果产出时已校验MIME；损坏的持久值在此也明确失败。
            let mime = resource
                .media_type
                .as_deref()
                .ok_or("Native media budget has no MIME type.")?;
            // The closed host registry owns attachment categories, including PDF.
            // 封闭宿主注册表持有附件分类，包括PDF。
            let descriptor = crate::attachment_assets::attachment_descriptor_from_mime_type(mime)
                .ok_or("Native media budget has unregistered MIME.")?;
            // A digest preserves content-sensitive budget evidence without retaining private reference bytes as text.
            // 摘要保留内容敏感的预算证据，不把私有引用字节保留为正文。
            let digest = hex::encode(Sha256::digest(
                serde_json::to_vec(envelope).map_err(|error| error.to_string())?,
            ));
            projected.history.push(ConversationMessage {
                id: format!("{}:native-media-budget", message.id),
                payload: ConversationPayload::UserText {
                    text: String::new(),
                    attachments: vec![ConversationAttachment {
                        kind: descriptor.kind,
                        file_name: resource
                            .file_name
                            .clone()
                            .ok_or("Native media budget has no filename.")?,
                        local_path: String::new(),
                        resource_path: resource.resource_path.clone(),
                        source_path: None,
                        asset_id: None,
                        byte_len: Some(envelope.byte_len),
                        source_sha256: Some(digest),
                        delivery_mode: None,
                        preview_local_path: None,
                        mime_type: mime.into(),
                    }],
                },
            });
        } else {
            projected.history.push(message);
        }
    }
    Ok(Cow::Owned(projected))
}
