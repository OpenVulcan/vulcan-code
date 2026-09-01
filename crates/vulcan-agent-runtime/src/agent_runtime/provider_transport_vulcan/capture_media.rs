//! Exact capture projection without media-upload or prompt-publication authority.
//! 不具备媒体上传或提示词发布权限的精确捕获投影。

use super::VulcanPublicContractRequest;
use crate::agent_runtime::events::ProviderRequestMediaContext;
use crate::prompt_media_projection::PromptMediaProjectionRuntime;
use std::collections::BTreeMap;
use vulcan_ai_contract::{AiMessage, MessageRole};
use vulcan_core::types::{ConversationMessage, ConversationPayload};

/// Projects `request` through its optional exact `projection` under frozen `context` before wire encoding.
/// 在线上编码之前，在冻结`context`下通过可选精确`projection`投影`request`。
/// Returns success only after all replacements validate; missing proof or fresh-upload requirements fail explicitly.
/// 仅在全部替换项验证后返回成功；缺少证明或需要新上传时明确失败。
pub(super) async fn prepare_capture_user_media(
    request: &mut VulcanPublicContractRequest,
    context: ProviderRequestMediaContext,
    projection: Option<&dyn PromptMediaProjectionRuntime>,
) -> Result<(), String> {
    let Some(projection) = projection else {
        return validate_capture_authority(&request.controller_snapshot.messages);
    };
    // Source order comes from the exact assembled snapshot, not lexical map order or latest session state.
    // 来源顺序取自精确组装快照，而非映射字典序或最新会话状态。
    let source_ids = request
        .controller_snapshot
        .messages
        .iter()
        .map(|message| message.id.clone())
        .collect::<Vec<_>>();
    // The host enforces upload-free capture using the same frozen execution as final encoding.
    // 宿主使用与最终编码相同的冻结执行强制无上传捕获。
    let replacements = projection.project_capture(&source_ids, context).await?;
    apply_capture_media(
        &request.controller_snapshot.messages,
        &request.source_message_positions,
        &mut request.public.messages,
        replacements,
    )
}

/// Validates source `messages` when no prompt-owned projection exists; text-only requests need no media proof.
/// 在缺少提示词所有投影时验证来源`messages`；纯文本请求无需媒体证明。
/// Returns an explicit capture error for attached user prompts instead of reporting provisional placeholders as final wire.
/// 对含附件的用户提示词返回明确捕获错误，不将临时占位符报告为最终线上请求。
fn validate_capture_authority(messages: &[ConversationMessage]) -> Result<(), String> {
    if messages.iter().any(|message| matches!(&message.payload, ConversationPayload::UserText { attachments, .. } if !attachments.is_empty())) {
        return Err(String::from("Exact media request capture requires the owning user prompt's frozen source projection; provisional attachment metadata is not a final request."));
    }
    Ok(())
}

/// Applies `replacements` to `public_messages` using the assembly-time `positions` and original `sources`.
/// 使用组装时`positions`及原`sources`将`replacements`应用至`public_messages`。
/// Returns an error without mutation for unknown IDs, wrong roles, invalid positions or omitted resource-bearing prompts.
/// 遇到未知ID、错误角色、无效位置或遗漏含资源提示词时返回错误且不修改内容。
fn apply_capture_media(
    sources: &[ConversationMessage],
    positions: &BTreeMap<String, usize>,
    public_messages: &mut [AiMessage],
    replacements: BTreeMap<String, AiMessage>,
) -> Result<(), String> {
    for source in sources {
        if matches!(&source.payload, ConversationPayload::UserText { attachments, .. } if !attachments.is_empty())
            && !replacements.contains_key(&source.id)
        {
            return Err(String::from(
                "Exact media capture omitted a resource-bearing user prompt.",
            ));
        }
    }
    // Validate every replacement before applying any, keeping capture failures atomic.
    // 在应用任何替换之前验证全部替换，使捕获失败保持原子性。
    for (id, message) in &replacements {
        let Some(source) = sources.iter().find(|source| &source.id == id) else {
            return Err(String::from(
                "Exact media capture returned an unknown source message.",
            ));
        };
        let Some(position) = positions.get(id) else {
            return Err(String::from(
                "Exact media capture source has no assembly position.",
            ));
        };
        if !matches!(&source.payload, ConversationPayload::UserText { .. })
            || message.role != MessageRole::User
            || public_messages
                .get(*position)
                .is_none_or(|original| original.role != MessageRole::User)
        {
            return Err(String::from(
                "Exact media capture source role or assembly position is invalid.",
            ));
        }
    }
    for (id, message) in replacements {
        public_messages[positions[&id]] = message;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
