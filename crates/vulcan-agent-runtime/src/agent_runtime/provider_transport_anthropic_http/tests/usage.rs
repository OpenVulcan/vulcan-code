use super::super::{
    AnthropicStreamingEvent, anthropic_usage_to_snapshot, merge_anthropic_partial_usage,
};

/// Verifies one Kimi-compatible Anthropic stream takes final cache usage from `message_delta`.
/// 验证一条 Kimi 兼容 Anthropic 流会从最终 `message_delta` 取得缓存用量。
#[test]
fn kimi_anthropic_terminal_message_delta_updates_cache_usage() {
    let message_start = serde_json::from_str::<AnthropicStreamingEvent>(
        r#"{
            "type":"message_start",
            "message":{
                "id":"msg_kimi_1",
                "model":"k3",
                "usage":{"input_tokens":0,"output_tokens":0}
            }
        }"#,
    )
    .expect("Kimi-compatible message_start should decode");
    let mut usage_snapshot = match message_start {
        AnthropicStreamingEvent::MessageStart { message } => {
            anthropic_usage_to_snapshot(&message._usage)
        }
        _ => panic!("expected message_start event"),
    };

    let message_delta = serde_json::from_str::<AnthropicStreamingEvent>(
        r#"{
            "type":"message_delta",
            "delta":{"stop_reason":"end_turn","stop_sequence":null},
            "usage":{
                "input_tokens":13,
                "output_tokens":4,
                "cache_read_input_tokens":22000,
                "cache_creation_input_tokens":31
            }
        }"#,
    )
    .expect("Kimi-compatible message_delta should decode");
    if let AnthropicStreamingEvent::MessageDelta { _usage } = message_delta {
        usage_snapshot = merge_anthropic_partial_usage(usage_snapshot.take(), &_usage);
    } else {
        panic!("expected message_delta event");
    }

    let usage_snapshot = usage_snapshot.expect("terminal usage should produce a snapshot");
    assert_eq!(usage_snapshot.input_tokens, Some(13));
    assert_eq!(usage_snapshot.output_tokens, Some(4));
    assert_eq!(usage_snapshot.cached_input_tokens, Some(22000));
    assert_eq!(usage_snapshot.cache_creation_input_tokens, Some(31));
    assert_eq!(usage_snapshot.total_tokens, Some(22048));
}

/// Verifies missing terminal cache fields preserve cache values already reported at message start.
/// 验证终态缺少缓存字段时会保留消息首包已经报告的缓存值。
#[test]
fn anthropic_terminal_usage_preserves_missing_start_cache_fields() {
    let message_start = serde_json::from_str::<AnthropicStreamingEvent>(
        r#"{
            "type":"message_start",
            "message":{
                "id":"msg_anthropic_1",
                "model":"claude-test",
                "usage":{
                    "input_tokens":7,
                    "output_tokens":1,
                    "cache_read_input_tokens":100,
                    "cache_creation_input_tokens":9
                }
            }
        }"#,
    )
    .expect("Anthropic message_start should decode");
    let mut usage_snapshot = match message_start {
        AnthropicStreamingEvent::MessageStart { message } => {
            anthropic_usage_to_snapshot(&message._usage)
        }
        _ => panic!("expected message_start event"),
    };

    let message_delta = serde_json::from_str::<AnthropicStreamingEvent>(
        r#"{
            "type":"message_delta",
            "delta":{"stop_reason":"end_turn","stop_sequence":null},
            "usage":{"output_tokens":5}
        }"#,
    )
    .expect("Anthropic message_delta should decode");
    if let AnthropicStreamingEvent::MessageDelta { _usage } = message_delta {
        usage_snapshot = merge_anthropic_partial_usage(usage_snapshot.take(), &_usage);
    } else {
        panic!("expected message_delta event");
    }

    let usage_snapshot = usage_snapshot.expect("merged usage should produce a snapshot");
    assert_eq!(usage_snapshot.input_tokens, Some(7));
    assert_eq!(usage_snapshot.output_tokens, Some(5));
    assert_eq!(usage_snapshot.cached_input_tokens, Some(100));
    assert_eq!(usage_snapshot.cache_creation_input_tokens, Some(9));
    assert_eq!(usage_snapshot.total_tokens, Some(121));
}

/// Verifies explicit terminal zero cache values overwrite earlier non-zero cache values.
/// 验证终态显式返回零缓存值时会覆盖此前的非零缓存值。
#[test]
fn anthropic_terminal_usage_preserves_explicit_zero_cache_values() {
    let current = vulcan_core::types::ProviderUsageSnapshot {
        input_tokens: Some(7),
        output_tokens: Some(1),
        reasoning_tokens: None,
        total_tokens: Some(117),
        cached_input_tokens: Some(100),
        cache_creation_input_tokens: Some(9),
    };
    let message_delta = serde_json::from_str::<AnthropicStreamingEvent>(
        r#"{
            "type":"message_delta",
            "delta":{"stop_reason":"end_turn","stop_sequence":null},
            "usage":{
                "output_tokens":5,
                "cache_read_input_tokens":0,
                "cache_creation_input_tokens":0
            }
        }"#,
    )
    .expect("Anthropic message_delta with explicit zero cache values should decode");
    let usage_snapshot = match message_delta {
        AnthropicStreamingEvent::MessageDelta { _usage } => {
            merge_anthropic_partial_usage(Some(current), &_usage)
        }
        _ => panic!("expected message_delta event"),
    }
    .expect("merged usage should produce a snapshot");

    assert_eq!(usage_snapshot.cached_input_tokens, Some(0));
    assert_eq!(usage_snapshot.cache_creation_input_tokens, Some(0));
    assert_eq!(usage_snapshot.total_tokens, Some(12));
}
