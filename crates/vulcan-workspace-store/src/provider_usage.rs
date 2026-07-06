//! Shared provider-usage helpers reused by runtime state, budget calibration, and persistence glue.
//! 供运行时状态、预算校准与持久化胶水层复用的共享 Provider 用量辅助逻辑。

use crate::conversation_protocol::{SessionConversationPayload, SessionConversationState};
use vulcan_core::types::ProviderUsageSnapshot;

/// Returns the most recent provider usage snapshot carried by the canonical conversation window.
/// 返回规范会话窗口中携带的最近一次 Provider 用量快照。
pub fn latest_provider_usage_from_conversation_state(
    state: &SessionConversationState,
) -> Option<ProviderUsageSnapshot> {
    state
        .messages
        .iter()
        .rev()
        .find_map(|message| match &message.payload {
            SessionConversationPayload::Assistant { usage, .. } => usage
                .as_ref()
                .filter(|snapshot| snapshot.has_any_tokens())
                .cloned(),
            _ => None,
        })
}

/// Serializes one provider usage snapshot into compact JSON when it contains concrete token counts.
/// 当用量快照包含具体 Token 数量时，将其序列化为紧凑 JSON。
#[cfg(test)]
pub(crate) fn serialize_provider_usage_to_json(
    usage: Option<&ProviderUsageSnapshot>,
) -> Option<String> {
    let snapshot = usage?;
    let usage = snapshot.has_any_tokens().then_some(snapshot)?;
    serde_json::to_string(&usage).ok()
}

/// Returns the observed provider input-token count when the provider reported enough usage data.
/// 当 Provider 返回了足够的用量数据时，返回实测输入 Token 数。
#[cfg(test)]
pub(crate) fn observed_input_tokens_from_usage(
    usage: Option<&ProviderUsageSnapshot>,
) -> Option<usize> {
    let snapshot = usage?;
    if let Some(input_tokens) = snapshot.input_tokens {
        return Some(input_tokens);
    }
    snapshot.total_tokens.map(|total_tokens| {
        let known_non_input_tokens = snapshot
            .output_tokens
            .unwrap_or(0usize)
            .saturating_add(snapshot.reasoning_tokens.unwrap_or(0usize));
        total_tokens
            .saturating_sub(known_non_input_tokens)
            .max(1usize)
    })
}

/// Returns one calibrated input-token estimate, preferring provider-observed usage over host heuristics.
/// 返回校准后的输入 Token 估算值，优先采用 Provider 实测用量而不是宿主启发式估算。
#[cfg(test)]
pub(crate) fn calibrated_input_token_estimate(
    estimated_input_tokens: usize,
    usage: Option<&ProviderUsageSnapshot>,
) -> usize {
    observed_input_tokens_from_usage(usage).unwrap_or(estimated_input_tokens)
}

#[cfg(test)]
mod tests {
    //! Provider-usage regression tests covering extraction, persistence, and conservative calibration.
    //! 覆盖提取、持久化与保守校准行为的 Provider 用量回归测试。

    use super::{
        calibrated_input_token_estimate, latest_provider_usage_from_conversation_state,
        observed_input_tokens_from_usage, serialize_provider_usage_to_json,
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
            cached_input_tokens: None,
            cache_creation_input_tokens: None,
        };
        let latest_usage = ProviderUsageSnapshot {
            input_tokens: Some(1024),
            output_tokens: Some(128),
            reasoning_tokens: Some(256),
            total_tokens: Some(1408),
            cached_input_tokens: Some(512),
            cache_creation_input_tokens: None,
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

    /// Skips empty assistant placeholders so aborted turns do not hide the last valid usage snapshot.
    /// 跳过空的助手占位用量，避免中止轮次覆盖上一条有效用量快照。
    #[test]
    fn latest_provider_usage_skips_empty_assistant_placeholder() {
        let valid_usage = ProviderUsageSnapshot {
            input_tokens: Some(1536),
            output_tokens: Some(128),
            reasoning_tokens: Some(64),
            total_tokens: Some(1728),
            cached_input_tokens: Some(1024),
            cache_creation_input_tokens: None,
        };
        let state = SessionConversationState {
            messages: vec![
                SessionConversationMessage {
                    id: String::from("assistant-valid"),
                    payload: SessionConversationPayload::Assistant {
                        text: String::from("有效返回"),
                        reasoning: String::new(),
                        provider_message_id: None,
                        provider_message_state: None,
                        tool_calls: Vec::new(),
                        usage: Some(valid_usage.clone()),
                    },
                },
                SessionConversationMessage {
                    id: String::from("assistant-empty"),
                    payload: SessionConversationPayload::Assistant {
                        text: String::new(),
                        reasoning: String::new(),
                        provider_message_id: None,
                        provider_message_state: None,
                        tool_calls: Vec::new(),
                        usage: Some(ProviderUsageSnapshot::default()),
                    },
                },
            ],
            records: Vec::new(),
            continuation: Default::default(),
        };

        assert_eq!(
            latest_provider_usage_from_conversation_state(&state),
            Some(valid_usage)
        );
    }

    /// Serializes concrete usage snapshots while skipping empty placeholders.
    /// 仅在存在具体 Token 数量时序列化用量快照，同时跳过空占位值。
    #[test]
    fn provider_usage_json_serialization_skips_empty_snapshots() {
        let usage = ProviderUsageSnapshot {
            input_tokens: Some(2048),
            output_tokens: Some(256),
            reasoning_tokens: Some(512),
            total_tokens: Some(2816),
            cached_input_tokens: Some(1024),
            cache_creation_input_tokens: Some(256),
        };

        let raw = serialize_provider_usage_to_json(Some(&usage))
            .expect("usage with concrete token counts should serialize");
        let serialized_value: serde_json::Value =
            serde_json::from_str(&raw).expect("usage JSON should remain valid");
        assert_eq!(serialized_value["input_tokens"], 2048);
        assert_eq!(serialized_value["output_tokens"], 256);
        assert!(
            serialize_provider_usage_to_json(Some(&ProviderUsageSnapshot::default())).is_none()
        );
    }

    /// Prefers latest observed provider input usage over host estimates when both values are available.
    /// 当实测 Provider 输入用量与宿主估算同时存在时，优先采用实测值。
    #[test]
    fn calibrated_input_token_estimate_prefers_observed_input_usage() {
        let usage = ProviderUsageSnapshot {
            input_tokens: Some(4096),
            output_tokens: Some(512),
            reasoning_tokens: None,
            total_tokens: Some(4608),
            cached_input_tokens: None,
            cache_creation_input_tokens: None,
        };

        assert_eq!(calibrated_input_token_estimate(1024, Some(&usage)), 4096);
        assert_eq!(calibrated_input_token_estimate(8192, Some(&usage)), 4096);
        assert_eq!(calibrated_input_token_estimate(1024, None), 1024);
    }

    /// Derives observed input usage from total minus known output-side tokens when direct input usage is missing.
    /// 当缺少直接输入用量时，通过总量减去已知输出侧 Token 推导输入用量。
    #[test]
    fn observed_input_tokens_from_usage_derives_from_total_when_needed() {
        let usage = ProviderUsageSnapshot {
            input_tokens: None,
            output_tokens: Some(256),
            reasoning_tokens: Some(128),
            total_tokens: Some(1408),
            cached_input_tokens: None,
            cache_creation_input_tokens: None,
        };

        assert_eq!(observed_input_tokens_from_usage(Some(&usage)), Some(1024));
        assert_eq!(calibrated_input_token_estimate(4096, Some(&usage)), 1024);
        assert_eq!(observed_input_tokens_from_usage(None), None);
    }
}
