//! Native source retention and corruption rejection through real pruning.
//! 通过真实剪枝验证原生来源保留及损坏拒绝。

use super::*;
use serde_json::json;

/// Returns a canonical checkpoint and oversized dialogue without external provider activity.
/// 返回规范检查点与超大对话，不进行外部供应商活动。
fn source_state() -> SessionConversationState {
    // The explicit native envelope owns this identity; display labels alone never do.
    // 明确原生封套持有此身份；仅展示标签没有此权威。
    let mut state = SessionConversationState {
        messages: vec![SessionConversationMessage {
            id: "checkpoint".into(),
            payload: SessionConversationPayload::Assistant {
                text: String::new(),
                reasoning: String::new(),
                provider_message_id: None,
                provider_message_state: Some(json!({"vulcan_ai":{"responses_compaction":{
                    "provider_id":"provider", "connection_id":"connection",
                    "route_fingerprint":"https://provider.invalid/responses",
                    "protocol_profile_id":"openai-codex-responses.v1", "model_id":"model",
                    "items":[{"type":"compaction","encrypted_content":"opaque".repeat(128)}],
                    "response_id":"receipt-only"
                }}})),
                usage: None,
                tool_calls: Vec::new(),
            },
        }],
        ..Default::default()
    };
    vulcan_context::append_user_text_message(
        &mut state,
        "old-user",
        "old-input".repeat(5000),
        Vec::new(),
    )
    .unwrap();
    vulcan_context::append_assistant_text_message(&mut state, "latest", "recent-text".repeat(1500))
        .unwrap();
    state
}

/// Pruning reduces ordinary text while preserving opaque bytes, order, source immutability, and honest budget overflow.
/// 剪枝缩减普通文字，同时保留不透明字节、顺序、来源不可变性及真实预算溢出状态。
#[test]
fn native_source_survives_real_pruning_and_too_small_budget() {
    // Preserve an exact source comparison across both budget branches.
    // 在两个预算分支中保留精确来源比较。
    let state = source_state();
    // Serialization checks the complete envelope, not only its type marker.
    // 序列化比较完整封套，不仅比较类型标记。
    let before = serde_json::to_value(&state).unwrap();
    // Planner rows deliberately contain no native metadata.
    // 规划行有意不携带原生元数据。
    let rows = conversation_state_to_compaction_source_messages(&state).unwrap();
    // Captured authority remains borrowed from the unchanged canonical state.
    // 捕获权威始终借自未改变的规范状态。
    let native = NativeCompactionSources::capture(&state).unwrap();
    for target in [512, 1] {
        // A tiny target may remain over budget, but cannot authorize checkpoint truncation.
        // 极小目标可能仍超预算，但不能授权截断检查点。
        let prepared = native
            .prepare(
                &rows,
                CompactionInputBudget {
                    target_input_budget_tokens: target,
                    safety_reserve_tokens: 0,
                },
                ContextCompactionPolicy::default(),
                CompactionInputPruneMode::RetryAfterFailure,
            )
            .unwrap();
        assert_eq!(prepared.source_messages[0], rows[0]);
        assert_eq!(prepared.source_messages.len(), 2);
        assert_eq!(prepared.prune_report.dropped_source_message_count, 1);
        assert_eq!(prepared.prune_report.truncated_source_message_count, 1);
        assert!(
            prepared.prune_report.pruned_token_estimate
                < prepared.prune_report.original_token_estimate
        );
        assert_eq!(prepared.prune_report.target_input_budget_tokens, target);
        assert_eq!(
            prepared.prune_report.pruned_token_estimate,
            native.estimate_tokens(&prepared.source_messages)
        );
        // Restore native state only; removed business text must not return with it.
        // 只恢复原生状态；被删除业务文字不能随之返回。
        let restored = native.restore(&prepared.source_messages).unwrap();
        assert_eq!(
            serde_json::to_value(&restored.messages[0]).unwrap(),
            before["messages"][0]
        );
        assert!(
            !serde_json::to_string(&restored)
                .unwrap()
                .contains("old-input")
        );
        assert!(restored.continuation.last_provider_message_id.is_none());
        if target == 1 {
            assert!(prepared.prune_report.still_exceeds_budget);
        }
    }
    assert_eq!(serde_json::to_value(&state).unwrap(), before);
}

/// Corrupt source selections cannot drop, duplicate, reorder, or replace the opaque prefix.
/// 损坏的来源选择不能丢弃、重复、重排或替换不透明前缀。
#[test]
fn native_source_rejects_selection_and_carrier_corruption() {
    // Share one valid authority across independent corruption cases.
    // 在独立损坏用例间共享一个有效权威。
    let state = source_state();
    // Start from the real planner conversion rather than hand-authored carrier rows.
    // 从真实规划转换开始，不手写载体行。
    let rows = conversation_state_to_compaction_source_messages(&state).unwrap();
    // The immutable capture cannot inherit any subsequent row mutation.
    // 不可变捕获不能继承后续行变更。
    let native = NativeCompactionSources::capture(&state).unwrap();
    assert!(native.restore(&rows[1..]).is_err());
    for case in 0..5 {
        // Each scenario alters exactly one source invariant.
        // 每个场景只改变一项来源约束。
        let mut altered = rows.clone();
        match case {
            0 => altered.insert(1, altered[0].clone()),
            1 => altered.swap(0, 1),
            2 => altered[0].body = "substitute text".into(),
            3 => altered[0].id = "unknown".into(),
            4 => altered[0].kind = vulcan_context::SessionMessageKind::User,
            _ => unreachable!(),
        }
        assert!(native.restore(&altered).is_err());
    }
    // Malformed native content is rejected without printing opaque bytes.
    // 无效原生内容直接拒绝，且不打印不透明字节。
    let mut malformed = state.clone();
    // Mutate the actual assistant extension, not a separate display record.
    // 修改实际助手扩展，不修改独立展示记录。
    let SessionConversationPayload::Assistant {
        provider_message_state: Some(metadata),
        ..
    } = &mut malformed.messages[0].payload
    else {
        unreachable!()
    };
    metadata["vulcan_ai"]["responses_compaction"]["items"] = json!([]);
    assert!(NativeCompactionSources::capture(&malformed).is_err());
}
