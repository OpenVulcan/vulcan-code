//! Shared provider-usage helpers reused by runtime state, budget calibration, and persistence glue.
//! 供运行时状态、预算校准与持久化胶水层复用的共享 Provider 用量辅助逻辑。

use crate::conversation_protocol::{SessionConversationPayload, SessionConversationState};
use vulcan_core::types::ProviderUsageSnapshot;

/// Returns the most recent provider usage snapshot carried by the canonical conversation window.
/// 返回规范会话窗口中携带的最近一次 Provider 用量快照。
pub(crate) fn latest_provider_usage_from_conversation_state(
    state: &SessionConversationState,
) -> Option<ProviderUsageSnapshot> {
    state
        .messages
        .iter()
        .rev()
        .find_map(|message| match &message.payload {
            SessionConversationPayload::Assistant { usage, .. } => usage.clone(),
            _ => None,
        })
}

/// Serializes one provider usage snapshot into compact JSON when it contains concrete token counts.
/// 当用量快照包含具体 Token 数量时，将其序列化为紧凑 JSON。
pub(crate) fn serialize_provider_usage_to_json(
    usage: Option<&ProviderUsageSnapshot>,
) -> Option<String> {
    let usage = usage?.has_any_tokens().then(|| usage.clone())?;
    serde_json::to_string(&usage).ok()
}

/// Restores one provider usage snapshot from compact JSON payload text.
/// 从紧凑 JSON 文本载荷中恢复一份 Provider 用量快照。
pub(crate) fn parse_provider_usage_from_json(raw: &str) -> Option<ProviderUsageSnapshot> {
    serde_json::from_str::<ProviderUsageSnapshot>(raw)
        .ok()
        .filter(ProviderUsageSnapshot::has_any_tokens)
}

/// Returns one conservative calibrated input-token estimate using the latest observed provider usage as a floor.
/// 使用最近一次实测 Provider 用量作为下限，返回一份保守校准后的输入 Token 估算值。
pub(crate) fn calibrated_input_token_estimate(
    estimated_input_tokens: usize,
    usage: Option<&ProviderUsageSnapshot>,
) -> usize {
    usage
        .and_then(|snapshot| snapshot.input_tokens.or(snapshot.total_tokens))
        .map(|observed| observed.max(estimated_input_tokens))
        .unwrap_or(estimated_input_tokens)
}

/// Builds one compact provider-usage label suitable for diagnostics and context-budget summaries.
/// 构建一条适合诊断与上下文预算摘要展示的紧凑 Provider 用量标签。
pub(crate) fn provider_usage_label(usage: &ProviderUsageSnapshot) -> String {
    let input = usage
        .input_tokens
        .map(|value| value.to_string())
        .unwrap_or_else(|| String::from("?"));
    let output = usage
        .output_tokens
        .map(|value| value.to_string())
        .unwrap_or_else(|| String::from("?"));
    let reasoning = usage
        .reasoning_tokens
        .map(|value| value.to_string())
        .unwrap_or_else(|| String::from("?"));
    let total = usage
        .total_tokens
        .map(|value| value.to_string())
        .unwrap_or_else(|| String::from("?"));
    format!("最近实测 输入 {input} / 输出 {output} / 推理 {reasoning} / 总计 {total}")
}

#[cfg(test)]
mod tests {
    //! Provider-usage regression tests covering extraction, persistence, and conservative calibration.
    //! 覆盖提取、持久化与保守校准行为的 Provider 用量回归测试。

    use super::{
        calibrated_input_token_estimate, latest_provider_usage_from_conversation_state,
        parse_provider_usage_from_json, serialize_provider_usage_to_json,
    };
    use crate::conversation_protocol::{
        SessionConversationMessage, SessionConversationPayload, SessionConversationState,
    };
    use vulcan_core::types::ProviderUsageSnapshot;

    /// Prefers the latest assistant-carried usage snapshot inside the canonical conversation window.
    /// 优先返回规范会话窗口中最近一条助手消息携带的用量快照。
    #[test]
    fn latest_provider_usage_prefers_latest_assistant_message() {
        let first_usage = ProviderUsageSnapshot {
            input_tokens: Some(256),
            output_tokens: Some(64),
            reasoning_tokens: None,
            total_tokens: Some(320),
        };
        let latest_usage = ProviderUsageSnapshot {
            input_tokens: Some(1024),
            output_tokens: Some(128),
            reasoning_tokens: Some(256),
            total_tokens: Some(1408),
        };
        let state = SessionConversationState {
            messages: vec![
                SessionConversationMessage {
                    id: String::from("assistant-1"),
                    payload: SessionConversationPayload::Assistant {
                        text: String::from("第一轮"),
                        reasoning: String::new(),
                        provider_message_id: None,
                        provider_message_state: None,
                        tool_calls: Vec::new(),
                        usage: Some(first_usage),
                    },
                },
                SessionConversationMessage {
                    id: String::from("assistant-2"),
                    payload: SessionConversationPayload::Assistant {
                        text: String::from("第二轮"),
                        reasoning: String::new(),
                        provider_message_id: None,
                        provider_message_state: None,
                        tool_calls: Vec::new(),
                        usage: Some(latest_usage.clone()),
                    },
                },
            ],
            records: Vec::new(),
            continuation: Default::default(),
        };

        assert_eq!(
            latest_provider_usage_from_conversation_state(&state),
            Some(latest_usage)
        );
    }

    /// Serializes and restores concrete usage snapshots while skipping empty placeholders.
    /// 仅在存在具体 Token 数量时序列化并恢复用量快照，同时跳过空占位值。
    #[test]
    fn provider_usage_json_roundtrip_skips_empty_snapshots() {
        let usage = ProviderUsageSnapshot {
            input_tokens: Some(2048),
            output_tokens: Some(256),
            reasoning_tokens: Some(512),
            total_tokens: Some(2816),
        };

        let raw = serialize_provider_usage_to_json(Some(&usage))
            .expect("usage with concrete token counts should serialize");
        assert_eq!(parse_provider_usage_from_json(&raw), Some(usage));
        assert!(
            serialize_provider_usage_to_json(Some(&ProviderUsageSnapshot::default())).is_none()
        );
        assert!(parse_provider_usage_from_json("{}").is_none());
    }

    /// Uses the latest observed provider usage as a floor so host estimates remain conservative.
    /// 使用最近一次实测 Provider 用量作为下限，确保宿主估算保持保守。
    #[test]
    fn calibrated_input_token_estimate_uses_observed_usage_as_floor() {
        let usage = ProviderUsageSnapshot {
            input_tokens: Some(4096),
            output_tokens: Some(512),
            reasoning_tokens: None,
            total_tokens: Some(4608),
        };

        assert_eq!(calibrated_input_token_estimate(1024, Some(&usage)), 4096);
        assert_eq!(calibrated_input_token_estimate(8192, Some(&usage)), 8192);
        assert_eq!(calibrated_input_token_estimate(1024, None), 1024);
    }
}
