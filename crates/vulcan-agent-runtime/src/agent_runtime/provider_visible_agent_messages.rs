//! Provider-visible typed agent-message seeding helpers.
//! Provider 可见类型化智能体消息播种辅助模块。

use crate::agent_runtime::execution_models::ExecutionProviderVisibleAgentMessage;
use std::collections::BTreeSet;
use vulcan_core::types::{
    ConversationMessage as CoreConversationMessage, ConversationPayload as CoreConversationPayload,
    ConversationRecord as CoreConversationRecord, ConversationSnapshot as CoreConversationSnapshot,
    TranscriptRecordKind as CoreTranscriptRecordKind, TranscriptRole as CoreTranscriptRole,
};

/// Appends typed agent messages to one core snapshot without creating user prompts.
/// 将类型化智能体消息追加到核心快照中，不创建用户提示词。
///
/// # Parameters
/// 参数
/// `snapshot` is the core snapshot that will be handed to the controller.
/// `snapshot` 是即将交给控制器的核心快照。
/// `messages` are the provider-visible typed agent messages to seed.
/// `messages` 是需要播种的 Provider 可见类型化智能体消息。
///
/// # Returns
/// 返回值
/// `Ok(())` when every message is appended; otherwise an explicit validation error.
/// 全部消息追加成功时返回 `Ok(())`；否则返回明确校验错误。
pub(crate) fn append_provider_visible_agent_messages_to_core_snapshot(
    snapshot: &mut CoreConversationSnapshot,
    messages: &[ExecutionProviderVisibleAgentMessage],
) -> Result<(), String> {
    if messages.is_empty() {
        return Ok(());
    }

    let mut seen_delivery_ids = snapshot
        .messages
        .iter()
        .filter_map(|message| message.id.strip_prefix("agent-message:"))
        .map(ToString::to_string)
        .collect::<BTreeSet<_>>();

    for message in messages {
        validate_provider_visible_agent_message(message)?;
        if !seen_delivery_ids.insert(message.delivery_id.clone()) {
            return Err(format!(
                "重复的智能体消息投递标识 `{}`，无法播种 Provider 可见消息。",
                message.delivery_id
            ));
        }
        append_provider_visible_agent_message_to_core_snapshot(snapshot, message);
    }

    Ok(())
}

/// Validates one typed agent message before it enters the provider-visible snapshot.
/// 在类型化智能体消息进入 Provider 可见快照前进行校验。
fn validate_provider_visible_agent_message(
    message: &ExecutionProviderVisibleAgentMessage,
) -> Result<(), String> {
    for (field_name, field_value) in [
        ("delivery_id", message.delivery_id.as_str()),
        ("message_type", message.message_type.as_str()),
        ("task_name", message.task_name.as_str()),
        ("sender", message.sender.as_str()),
        ("recipient", message.recipient.as_str()),
        ("payload", message.payload.as_str()),
    ] {
        if field_value.trim().is_empty() {
            return Err(format!(
                "智能体消息字段 `{field_name}` 为空，无法播种 Provider 可见消息。"
            ));
        }
    }

    Ok(())
}

/// Appends one already validated typed agent message to the snapshot.
/// 将一条已校验的类型化智能体消息追加到快照。
fn append_provider_visible_agent_message_to_core_snapshot(
    snapshot: &mut CoreConversationSnapshot,
    message: &ExecutionProviderVisibleAgentMessage,
) {
    let message_id = format!("agent-message:{}", message.delivery_id);
    let record_id = format!("agent-record:{}", message.delivery_id);
    let created_at = message.timestamp_ms.to_string();
    let model_text = CoreConversationPayload::render_agent_message_text(
        &message.message_type,
        &message.task_name,
        &message.sender,
        &message.payload,
    );

    snapshot.messages.push(CoreConversationMessage {
        id: message_id,
        payload: CoreConversationPayload::AgentMessage {
            message_type: message.message_type.clone(),
            task_name: message.task_name.clone(),
            sender: message.sender.clone(),
            recipient: message.recipient.clone(),
            payload: message.payload.clone(),
            trigger_turn: message.trigger_turn,
            metadata: message.metadata.clone(),
        },
    });
    snapshot.records.push(CoreConversationRecord {
        id: record_id,
        role: CoreTranscriptRole::Assistant,
        record_kind: CoreTranscriptRecordKind::Visible,
        visible: false,
        title: String::from("智能体消息"),
        body: model_text,
        metadata: message.metadata.clone(),
        created_at,
    });
}
