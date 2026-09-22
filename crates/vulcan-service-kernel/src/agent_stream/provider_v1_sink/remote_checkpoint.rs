//! Projection of durable opaque checkpoints into logical and physical histories.
//! 可靠不透明检查点向逻辑与物理历史的投影。

use super::*;
use vulcan_context::{SessionMessageEntry, conversation_state_from_session_messages};
use vulcan_session_core::v1::context::RemoteResponsesCheckpoint;
use vulcan_session_core::v1::ids::CompactionId;

/// Returns the stable planning identity for the published `compaction_id`.
/// 返回已发布`compaction_id`的稳定规划身份。
pub(crate) fn planning_message_id(compaction_id: CompactionId) -> String {
    format!("v1-remote-compaction-{}", compaction_id.value())
}

/// Projects `checkpoint` into a native-only controller carrier; returns shape errors without text fallback.
/// 将`checkpoint`投影为纯原生控制器载体；结构错误直接返回，不降级为文本。
pub(super) fn controller_payload(
    checkpoint: &RemoteResponsesCheckpoint,
) -> Result<SessionConversationPayload, String> {
    checkpoint.validate().map_err(|error| error.to_string())?;
    Ok(SessionConversationPayload::Assistant {
        text: String::new(),
        reasoning: String::new(),
        provider_message_id: None,
        provider_message_state: Some(serde_json::json!({
            "vulcan_ai": {
                "responses_compaction": {
                    "provider_id": checkpoint.provider_id,
                    "connection_id": checkpoint.connection_id,
                    "route_fingerprint": checkpoint.route_fingerprint,
                    "protocol_profile_id": checkpoint.protocol_profile_id,
                    "model_id": checkpoint.model_id,
                    "items": checkpoint.output_items,
                    "response_id": checkpoint.response_id,
                }
            }
        })),
        usage: None,
        tool_calls: Vec::new(),
    })
}

/// Restores canonical `messages` from the same durable `snapshot`, attaching opaque state only by typed node identity.
/// 从同一可靠`snapshot`恢复规范`messages`，只按类型化节点身份附加不透明状态。
/// Returns exact planning history, or an error if any checkpoint row is missing, repeated, or altered.
/// 返回精确规划历史；检查点行缺失、重复或被修改时返回错误。
pub(crate) fn compaction_state_from_projection(
    snapshot: &SessionProviderProjectionSnapshot,
    messages: &[SessionMessageEntry],
) -> Result<SessionConversationState, String> {
    // Rebuild ordinary transcript rows before restoring node-owned native state.
    // 在恢复节点持有的原生状态前重建普通转录行。
    let mut state = conversation_state_from_session_messages(messages)?;
    for item in &snapshot.items {
        // Only the explicit durable checkpoint variant may supply opaque state.
        // 只有明确的可靠检查点变体可以提供不透明状态。
        let ProjectedItem::RemoteCompactionCheckpoint {
            compaction_id,
            checkpoint,
        } = item
        else {
            continue;
        };
        // The planner's empty carrier has a node-owned identity, never a guessed message position.
        // 规划器的空载体具有节点持有身份，绝不猜测消息位置。
        let id = planning_message_id(*compaction_id);
        // Enforce a unique carrier rather than overwriting the first approximate match.
        // 强制载体唯一，不覆盖第一个近似匹配项。
        let mut matching = state.messages.iter_mut().filter(|message| message.id == id);
        // Missing identity is a failed projection, never permission to drop the checkpoint.
        // 身份缺失表示投影失败，绝不意味着可以丢弃检查点。
        let message = matching.next().ok_or_else(|| {
            String::from("Remote compaction checkpoint is missing from the planning source.")
        })?;
        if matching.next().is_some()
            || !matches!(&message.payload, SessionConversationPayload::Assistant { text, reasoning, provider_message_id: None, provider_message_state: None, usage: None, tool_calls } if text.is_empty() && reasoning.is_empty() && tool_calls.is_empty())
        {
            return Err(String::from(
                "Remote compaction planning carrier was duplicated or altered.",
            ));
        }
        message.payload = controller_payload(checkpoint)?;
    }
    Ok(state)
}

/// Builds the native-only wire message for `checkpoint` after comparing every field with actual `route`.
/// 在与实际`route`逐字段比较后，为`checkpoint`构建纯原生出站消息。
/// Missing or mismatching route identity is an explicit error, regardless of reasoning replay policy.
/// 无论推理重放策略如何，路由身份缺失或不匹配都明确报错。
pub(super) fn physical_message(
    checkpoint: &RemoteResponsesCheckpoint,
    route: Option<&V1ReplayRouteIdentity>,
) -> Result<AiMessage, SessionRuntimeError> {
    checkpoint
        .validate()
        .map_err(provider_projection_runtime_error)?;
    // Real request preparation must provide the route that will actually dispatch.
    // 真实请求准备必须提供将实际分派的路由。
    let route = route.ok_or_else(|| {
        provider_projection_runtime_error(
            "Opaque compaction checkpoint requires a frozen execution route.",
        )
    })?;
    // Each identity dimension is mandatory and compared without normalization or fallback.
    // 每个身份维度都强制比较，不执行规范化或回退。
    for (field, source, target) in [
        (
            "provider",
            checkpoint.provider_id.as_str(),
            route.provider_id.as_str(),
        ),
        (
            "connection",
            checkpoint.connection_id.as_str(),
            route.connection_id.as_str(),
        ),
        (
            "endpoint",
            checkpoint.route_fingerprint.as_str(),
            route.route_fingerprint.as_str(),
        ),
        (
            "protocol",
            checkpoint.protocol_profile_id.as_str(),
            route.protocol_profile_id.as_str(),
        ),
        (
            "model",
            checkpoint.model_id.as_str(),
            route.model_id.as_str(),
        ),
    ] {
        if source != target {
            return Err(provider_projection_runtime_error(format!(
                "Opaque compaction checkpoint route mismatch: {field}."
            )));
        }
    }
    Ok(AiMessage {
        role: MessageRole::Assistant,
        content: Vec::new(),
        provider_context_items: checkpoint.output_items.clone(),
    })
}

/// Validates selected `message` against the durable `checkpoint` and actual frozen `route` before minimal dispatch.
/// 在最小请求发送前，根据可靠`checkpoint`与实际冻结`route`校验选定`message`。
/// Returns a controlled error for any route, ordering, byte, or synthetic-content change.
/// 路由、顺序、字节或合成内容发生任何变化时返回受控错误。
pub(super) fn validate_physical_message(
    checkpoint: &RemoteResponsesCheckpoint,
    message: &AiMessage,
    route: Option<&V1ReplayRouteIdentity>,
) -> Result<(), SessionRuntimeError> {
    // Exact native reconstruction prevents trusting an independently assembled pre-gate payload.
    // 精确原生重建避免信任独立装配的闸门前载荷。
    let expected = physical_message(checkpoint, route)?;
    if serde_json::to_value(message).map_err(provider_projection_runtime_error)?
        != serde_json::to_value(expected).map_err(provider_projection_runtime_error)?
    {
        return Err(provider_projection_runtime_error(
            "Compaction candidate native checkpoint payload changed.",
        ));
    }
    Ok(())
}
