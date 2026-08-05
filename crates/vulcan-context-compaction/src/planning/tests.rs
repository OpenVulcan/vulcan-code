use super::*;
use vulcan_context::{
    ContextCompactionPolicy, SessionMessageEntry, SessionMessageKind, SessionMessageRecordKind,
};

/// Returns whether text contains CJK characters.
/// 返回文本是否包含 CJK 字符。
fn contains_cjk_text(text: &str) -> bool {
    text.chars().any(|character| {
        let codepoint = character as u32;
        (0x4E00..=0x9FFF).contains(&codepoint)
            || (0x3400..=0x4DBF).contains(&codepoint)
            || (0xF900..=0xFAFF).contains(&codepoint)
    })
}

/// Removes the variable previous-summary payload from a continuation prompt.
/// 从续写提示中移除可变的 previous-summary 载荷。
fn continuation_prompt_shell(prompt: &str) -> String {
    let Some((before_summary, after_start)) = prompt.split_once(PREVIOUS_SUMMARY_START_TAG) else {
        return prompt.to_string();
    };
    let Some((_, after_summary)) = after_start.split_once(PREVIOUS_SUMMARY_END_TAG) else {
        return prompt.to_string();
    };
    format!("{before_summary}{PREVIOUS_SUMMARY_START_TAG}{PREVIOUS_SUMMARY_END_TAG}{after_summary}")
}

fn test_context_message(
    id: &str,
    kind: SessionMessageKind,
    record_kind: SessionMessageRecordKind,
    body: &str,
) -> SessionMessageEntry {
    SessionMessageEntry {
        id: id.to_string(),
        kind,
        record_kind,
        visible: true,
        title: String::new(),
        body: body.to_string(),
        metadata: None,
        created_at: String::new(),
    }
}

fn test_tool_result_message(id: &str, body: &str) -> SessionMessageEntry {
    SessionMessageEntry {
        id: id.to_string(),
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::ToolResultOutput,
        visible: true,
        title: String::from("Tool Result shell_command"),
        body: body.to_string(),
        metadata: Some(serde_json::json!({
            "tool_name": "shell_command",
        })),
        created_at: String::new(),
    }
}

/// Verifies prompt-shaped compaction results keep a compact preview separate from the injected continuation body.
/// 验证提示词形态压缩结果会把紧凑预览与注入的续写正文分开保留。
#[test]
fn compaction_result_carries_compact_preview() {
    let plan = CompactionPlan {
        source_messages: Vec::new(),
        source_until_message_id: Some(String::from("m-1")),
        source_until_turn_index: Some(1),
        retained_tail_start_turn_index: Some(2),
        compaction_id: String::from("cp-1"),
        created_at: String::from("20"),
        policy: ContextCompactionPolicy::default(),
    };

    let result = CompactionResult::from_model_summary(
        &plan,
        "保留目标、约束和未完成工作，同时不要重复工具说明。",
    );

    assert_eq!(
        result.summary_record.preview,
        "保留目标、约束和未完成工作，同时不要重复工具说明。"
    );
    assert!(
    result
        .summary_record
        .body
        .contains("<previous-summary>\n保留目标、约束和未完成工作，同时不要重复工具说明。\n</previous-summary>")
);
    assert!(
        result
            .summary_record
            .body
            .contains("Recent messages after this summary are preserved verbatim below.")
    );
}

/// Verifies remote compaction preserves opaque output and diagnostic metadata under one route.
/// 验证远端压缩会在单一路由下保留不透明输出与诊断元数据。
#[test]
fn remote_compaction_result_preserves_checkpoint_metadata() {
    let plan = CompactionPlan {
        source_messages: Vec::new(),
        source_until_message_id: Some(String::from("m-1")),
        source_until_turn_index: Some(1),
        retained_tail_start_turn_index: None,
        compaction_id: String::from("cp-remote"),
        created_at: String::from("30"),
        policy: ContextCompactionPolicy::default(),
    };
    let result = CompactionResult::from_remote_responses_with_metadata(
        &plan,
        ProviderContextRoute {
            provider_id: String::from("custom:proxy"),
            connection_id: String::from("connection-a"),
            route_fingerprint: String::from("https://proxy.example/v1/responses"),
            protocol_profile_id: String::from("openai-codex-responses.v1"),
            model_id: String::from("gpt-5-codex"),
        },
        vec![
            serde_json::json!({
                "type": "compaction_summary",
                "encrypted_content": "opaque"
            }),
            serde_json::json!({
                "type": "message",
                "role": "assistant",
                "content": [{"type": "output_text", "text": "remote summary"}]
            }),
        ],
        Some(serde_json::json!({"input_tokens": 12})),
        Some(String::from("resp-compact")),
    )
    .expect("valid remote compaction should build a checkpoint");

    assert_eq!(result.provider_context_items.len(), 2);
    assert_eq!(result.provider_context_items[0]["type"], "compaction");
    assert_eq!(result.summary_record.kind, SessionMessageKind::Assistant);
    assert_eq!(result.summary_record.title, "Remote Responses Checkpoint");
    assert_eq!(
        result.summary_record.body,
        "Remote Responses compaction checkpoint installed (2 provider items)."
    );
    assert_eq!(
        result.provider_context_usage,
        Some(serde_json::json!({"input_tokens": 12}))
    );
    assert_eq!(
        result.provider_context_response_id.as_deref(),
        Some("resp-compact")
    );
    assert_eq!(
        result
            .provider_context_route
            .as_ref()
            .expect("route identity should be present")
            .connection_id,
        "connection-a"
    );

    // empty_checkpoint proves the planning layer cannot materialize an unusable opaque continuation.
    // empty_checkpoint 证明规划层不能物化不可用的不透明续写状态。
    let empty_checkpoint = CompactionResult::from_remote_responses_with_metadata(
        &plan,
        ProviderContextRoute {
            provider_id: String::from("custom:proxy"),
            connection_id: String::from("connection-a"),
            route_fingerprint: String::from("https://proxy.example/v1/responses"),
            protocol_profile_id: String::from("openai-codex-responses.v1"),
            model_id: String::from("gpt-5-codex"),
        },
        vec![serde_json::json!({
            "type": "compaction",
            "encrypted_content": ""
        })],
        None,
        None,
    )
    .expect_err("empty opaque checkpoint must be rejected before materialization");
    assert!(empty_checkpoint.contains("invalid ordered replayable item list"));
}

/// Verifies continuation prompts only mention preserved recent messages when that fact is explicit.
/// 验证续写提示词只在明确存在最近消息保留事实时才提及该事实。
#[test]
fn continuation_prompt_tail_note_requires_explicit_option() {
    let default_prompt = build_compaction_continuation_prompt("摘要");
    let prompt_with_tail = build_compaction_continuation_prompt_with_options(
        "摘要",
        CompactionContinuationPromptOptions {
            recent_messages_preserved: true,
        },
    );

    assert!(!default_prompt.contains("preserved verbatim below"));
    assert!(prompt_with_tail.contains("preserved verbatim below"));
    assert!(
        !contains_cjk_text(&continuation_prompt_shell(&default_prompt)),
        "model-visible continuation prompt shell must remain fixed English"
    );
    assert!(
        !contains_cjk_text(&continuation_prompt_shell(&prompt_with_tail)),
        "model-visible retained-tail continuation prompt shell must remain fixed English"
    );
}

/// Verifies compaction results omit the retained-tail note when no tail remains.
/// 验证压缩结果在没有尾部保留时不会写入尾部保留提示。
#[test]
fn compaction_result_omits_tail_note_without_retained_tail() {
    let plan = CompactionPlan {
        source_messages: Vec::new(),
        source_until_message_id: Some(String::from("m-1")),
        source_until_turn_index: Some(1),
        retained_tail_start_turn_index: None,
        compaction_id: String::from("cp-1"),
        created_at: String::from("20"),
        policy: ContextCompactionPolicy::default(),
    };

    let result = CompactionResult::from_model_summary(&plan, "摘要");

    assert!(
        !result
            .summary_record
            .body
            .contains("preserved verbatim below")
    );
}

/// Verifies previous summaries are persisted in the tagged cache-stable form used by later compactions.
/// 验证历史摘要会以供后续压缩复用的带标签缓存稳定格式持久化。
#[test]
fn compaction_result_persists_previous_summary_block() {
    let plan = CompactionPlan {
        source_messages: Vec::new(),
        source_until_message_id: Some(String::from("m-1")),
        source_until_turn_index: Some(1),
        retained_tail_start_turn_index: Some(2),
        compaction_id: String::from("cp-1"),
        created_at: String::from("20"),
        policy: ContextCompactionPolicy::default(),
    };

    let result = CompactionResult::from_model_summary(
        &plan,
        "<previous-summary>\n保留架构决策与预算策略。\n</previous-summary>",
    );

    assert_eq!(result.summary_record.preview, "保留架构决策与预算策略。");
    assert_eq!(
        result
            .summary_record
            .body
            .matches("<previous-summary>")
            .count(),
        1
    );
    assert!(
        result
            .summary_record
            .body
            .contains("保留架构决策与预算策略。")
    );
}

/// Verifies model compaction output persists only the durable summary section.
/// 验证模型压缩输出只会持久化可延续的摘要段落。
#[test]
fn extract_compaction_model_summary_strips_analysis_block() {
    let output = "<analysis>\n逐条检查上下文。\n</analysis>\n<summary>\nPrimary Request and Intent:\n- 继续实现上下文压缩升级。\n</summary>";

    assert_eq!(
        extract_compaction_model_summary(output),
        Some(String::from(
            "Primary Request and Intent:\n- 继续实现上下文压缩升级。"
        ))
    );
}

/// Verifies analysis-only model output is rejected instead of becoming durable memory.
/// 验证只有分析草稿的模型输出会被拒绝而不是成为持久记忆。
#[test]
fn extract_compaction_model_summary_rejects_analysis_only_output() {
    let output = "<analysis>\n只有草稿，没有摘要。\n</analysis>";

    assert_eq!(extract_compaction_model_summary(output), None);
}

/// Verifies tagged model output ignores non-durable text outside summary.
/// 验证带标签的模型输出会忽略摘要之外的非持久文本。
#[test]
fn extract_compaction_model_summary_ignores_text_outside_summary() {
    let output = "draft preface\n<analysis>\n草稿。\n</analysis>\nleaked note\n<summary>\nNext Step:\n- 继续验证。\n</summary>\ntrailing note";

    assert_eq!(
        extract_compaction_model_summary(output),
        Some(String::from("Next Step:\n- 继续验证。"))
    );
}

/// Verifies tagged model output without a summary block is rejected.
/// 验证带标签但缺少摘要段的模型输出会被拒绝。
#[test]
fn extract_compaction_model_summary_rejects_tagged_output_without_summary() {
    let output = "draft preface\n<analysis>\n草稿。\n</analysis>\nleaked note";

    assert_eq!(extract_compaction_model_summary(output), None);
}

/// Verifies legacy continuation prompts still expose only their summary payload.
/// 验证旧版续写提示词仍然只会暴露其中的摘要正文。
#[test]
fn extract_compaction_previous_summary_supports_legacy_body() {
    let body = "This session is being continued from earlier context.\n\nEarlier context summary:\n保留目标、约束和未完成项。\n\nContinue from the newer messages below without restarting.";

    assert_eq!(
        extract_compaction_previous_summary(body),
        "保留目标、约束和未完成项。"
    );
}

/// Verifies chained compaction keeps the previous continuation summary inside the next source window.
/// 验证链式压缩会把上一轮续写摘要保留在下一次源窗口内。
#[test]
fn plan_context_compaction_keeps_previous_summary_in_next_source_window() {
    let previous_summary = test_context_message(
        "summary-1",
        SessionMessageKind::User,
        SessionMessageRecordKind::CompactionSummary,
        "This session is being continued from earlier context.\n\n<previous-summary>\n保留架构决策、预算策略和未完成项。\n</previous-summary>\n\nContinue from the newer messages below without restarting.",
    );
    let mut messages = vec![previous_summary.clone()];
    for index in 0..8 {
        messages.push(test_context_message(
            &format!("user-{index}"),
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            &format!("新增用户上下文 {index}"),
        ));
        messages.push(test_context_message(
            &format!("assistant-{index}"),
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            &format!("新增助手输出 {index}"),
        ));
    }

    let plan = plan_context_compaction(CompactionPlanInput {
        session_label: "test-session",
        messages: &messages,
        all_messages: &messages,
        context_start_message_id: Some("summary-1"),
        source_until_message_id: None,
        policy: ContextCompactionPolicy::default(),
    })
    .expect("chained context should still produce a compaction plan");

    assert_eq!(plan.source_messages.first(), Some(&previous_summary));
    assert!(
        plan.source_messages
            .iter()
            .any(|message| message.id == "summary-1"
                && message.record_kind == SessionMessageRecordKind::CompactionSummary)
    );
}

/// Verifies explicit partial source boundaries override the automatic retained-tail cut.
/// 验证明示 partial 源边界会覆盖自动保留尾部切分点。
#[test]
fn plan_context_compaction_respects_explicit_source_until_message_id() {
    let messages = vec![
        test_context_message(
            "user-0",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "旧用户请求",
        ),
        test_context_message(
            "assistant-0",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "旧助手回答",
        ),
        test_context_message(
            "user-1",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "保留的新用户请求",
        ),
        test_context_message(
            "assistant-1",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "保留的新助手回答",
        ),
    ];

    let plan = plan_context_compaction(CompactionPlanInput {
        session_label: "test-session",
        messages: &messages,
        all_messages: &messages,
        context_start_message_id: None,
        source_until_message_id: Some("assistant-0"),
        policy: ContextCompactionPolicy::default(),
    })
    .expect("explicit source boundary should produce a compaction plan");

    assert_eq!(plan.source_until_message_id.as_deref(), Some("assistant-0"));
    assert_eq!(
        plan.source_messages
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec!["user-0", "assistant-0"]
    );
    assert_eq!(plan.retained_tail_start_turn_index, Some(1));
}

/// Verifies assistant entries with the same provider message id count as one retained API round.
/// 验证拥有相同 Provider 消息标识的 Assistant 条目会作为一个保留 API 执行轮次。
#[test]
fn plan_context_compaction_groups_provider_message_id_as_one_api_round() {
    let mut assistant_a = test_context_message(
        "assistant-a",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::AssistantOutput,
        "first chunk from one provider round",
    );
    assistant_a.metadata = Some(serde_json::json!({ "provider_message_id": "provider-a" }));
    let mut assistant_b = test_context_message(
        "assistant-b",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::AssistantOutput,
        "second chunk from one provider round",
    );
    assistant_b.metadata = Some(serde_json::json!({ "provider_message_id": "provider-a" }));
    let mut assistant_c = test_context_message(
        "assistant-c",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::AssistantOutput,
        "newer provider round",
    );
    assistant_c.metadata = Some(serde_json::json!({ "provider_message_id": "provider-b" }));
    let messages = vec![
        test_context_message(
            "user-0",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "旧用户请求",
        ),
        assistant_a,
        assistant_b,
        assistant_c,
    ];

    let plan = plan_context_compaction(CompactionPlanInput {
        session_label: "test-session",
        messages: &messages,
        all_messages: &messages,
        context_start_message_id: None,
        source_until_message_id: None,
        policy: ContextCompactionPolicy {
            tail_turns: 2,
            preserve_recent_tokens: 10_000,
            ..ContextCompactionPolicy::default()
        },
    })
    .expect("provider-grouped API rounds should produce a compaction plan");

    assert_eq!(plan.source_until_message_id.as_deref(), Some("user-0"));
    assert_eq!(
        plan.source_messages
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec!["user-0"]
    );
}

/// Verifies compaction input keeps full source messages when the target budget is sufficient.
/// 验证目标预算充足时压缩输入会保留完整源消息。
#[test]
fn prepare_compaction_source_keeps_full_input_when_budget_fits() {
    let messages = vec![
        test_context_message(
            "user-0",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "请检查项目",
        ),
        test_tool_result_message("tool-0", "short successful output"),
    ];

    let prepared = prepare_compaction_source_for_target_budget(
        &messages,
        CompactionInputBudget {
            target_input_budget_tokens: 10_000,
            safety_reserve_tokens: 30_000,
        },
        ContextCompactionPolicy::default(),
        CompactionInputPruneMode::RequiredForWindowDowngrade,
    );

    assert_eq!(prepared.source_messages, messages);
    assert_eq!(prepared.prune_report.mode, CompactionInputPruneMode::None);
    assert_eq!(prepared.prune_report.pruned_tool_output_count, 0);
}

/// Verifies window-downgrade pruning only rewrites cloned old tool output.
/// 验证窗口降级剪枝只会改写克隆出来的旧工具输出。
#[test]
fn prepare_compaction_source_prunes_old_tool_output_clone_only() {
    let long_output = (0..300)
        .map(|index| format!("line {index}: successful search hit"))
        .collect::<Vec<_>>()
        .join("\n");
    let messages = vec![
        test_context_message(
            "user-0",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "旧请求",
        ),
        test_context_message(
            "assistant-old",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::ToolCallOutput,
            "old tool call",
        ),
        test_tool_result_message("tool-old", &long_output),
        test_context_message(
            "user-1",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "当前请求",
        ),
        test_context_message(
            "assistant-recent",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::ToolCallOutput,
            "recent tool call",
        ),
        test_tool_result_message("tool-recent", "recent output must stay"),
    ];
    let original = messages.clone();

    let prepared = prepare_compaction_source_for_target_budget(
        &messages,
        CompactionInputBudget {
            target_input_budget_tokens: 1_000,
            safety_reserve_tokens: 30_000,
        },
        ContextCompactionPolicy {
            tail_turns: 1,
            preserve_recent_tokens: 10_000,
            ..ContextCompactionPolicy::default()
        },
        CompactionInputPruneMode::RequiredForWindowDowngrade,
    );

    assert_eq!(messages, original, "source input must remain unchanged");
    assert!(
        prepared.source_messages[2]
            .body
            .contains("[Tool result compacted for context-compaction input only]")
    );
    assert_eq!(prepared.source_messages[5].body, "recent output must stay");
    assert_eq!(prepared.prune_report.pruned_tool_output_count, 2);
    assert_eq!(prepared.prune_report.preserved_tool_output_count, 2);
    assert_eq!(prepared.prune_report.dropped_source_message_count, 0);
}

/// Verifies clone-only pruning can drop oldest segments when tool-output pruning still exceeds budget.
/// 验证仅克隆剪枝在工具输出剪枝后仍超预算时可以丢弃最旧分段。
#[test]
fn prepare_compaction_source_drops_old_segments_when_tool_pruning_is_not_enough() {
    let huge_context = (0..200)
        .map(|index| format!("context line {index}: detailed non-tool discussion"))
        .collect::<Vec<_>>()
        .join("\n");
    let messages = vec![
        test_context_message(
            "user-0",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            &huge_context,
        ),
        test_context_message(
            "user-1",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            &huge_context,
        ),
        test_context_message(
            "user-2",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "recent request must stay",
        ),
    ];
    let original = messages.clone();

    let prepared = prepare_compaction_source_for_target_budget(
        &messages,
        CompactionInputBudget {
            target_input_budget_tokens: 1,
            safety_reserve_tokens: 30_000,
        },
        ContextCompactionPolicy {
            tail_turns: 1,
            preserve_recent_tokens: 10_000,
            ..ContextCompactionPolicy::default()
        },
        CompactionInputPruneMode::RequiredForWindowDowngrade,
    );

    assert_eq!(messages, original, "source input must remain unchanged");
    assert_eq!(prepared.source_messages.len(), 1);
    assert_eq!(prepared.source_messages[0].id, "user-2");
    assert_eq!(prepared.prune_report.dropped_source_message_count, 2);
    assert_eq!(prepared.prune_report.pruned_tool_output_count, 0);
}

/// Verifies clone-only pruning can split an oversized protected tail while keeping the newest message.
/// 验证仅克隆剪枝可以切分超大的受保护尾部，同时保留最新消息。
#[test]
fn prepare_compaction_source_splits_oversized_tail_when_needed() {
    let huge_context = (0..200)
        .map(|index| format!("tail line {index}: detailed recent discussion"))
        .collect::<Vec<_>>()
        .join("\n");
    let messages = vec![
        test_context_message(
            "user-0",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            &huge_context,
        ),
        test_context_message(
            "assistant-0",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            &huge_context,
        ),
        test_context_message(
            "assistant-1",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "newest detail must stay",
        ),
    ];
    let original = messages.clone();

    let prepared = prepare_compaction_source_for_target_budget(
        &messages,
        CompactionInputBudget {
            target_input_budget_tokens: 1,
            safety_reserve_tokens: 30_000,
        },
        ContextCompactionPolicy {
            tail_turns: 1,
            preserve_recent_tokens: 10_000,
            ..ContextCompactionPolicy::default()
        },
        CompactionInputPruneMode::RequiredForWindowDowngrade,
    );

    assert_eq!(messages, original, "source input must remain unchanged");
    assert_eq!(prepared.source_messages.len(), 1);
    assert_eq!(prepared.source_messages[0].id, "assistant-1");
    assert!(
        prepared.source_messages[0]
            .body
            .contains("[Message truncated for context-compaction input only]")
    );
    assert_eq!(prepared.prune_report.dropped_source_message_count, 2);
    assert_eq!(prepared.prune_report.truncated_source_message_count, 1);
}

/// Verifies error-like tool output is preserved even when clone-only pruning is enabled.
/// 验证启用仅克隆剪枝时仍会完整保留错误类工具输出。
#[test]
fn prepare_compaction_source_preserves_error_tool_output() {
    let long_output = (0..300)
        .map(|index| format!("line {index}: successful output"))
        .collect::<Vec<_>>()
        .join("\n");
    let messages = vec![
        test_context_message(
            "user-0",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "旧请求",
        ),
        test_context_message(
            "assistant-old",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::ToolCallOutput,
            "old tool call",
        ),
        test_tool_result_message("tool-old", &long_output),
        test_context_message(
            "assistant-error",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::ToolCallOutput,
            "error tool call",
        ),
        test_tool_result_message(
            "tool-error",
            "stderr: compilation failed\nerror[E0425]: cannot find value",
        ),
        test_context_message(
            "assistant-recent",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "recent final answer",
        ),
    ];

    let prepared = prepare_compaction_source_for_target_budget(
        &messages,
        CompactionInputBudget {
            target_input_budget_tokens: 1_000,
            safety_reserve_tokens: 30_000,
        },
        ContextCompactionPolicy {
            tail_turns: 1,
            preserve_recent_tokens: 1,
            ..ContextCompactionPolicy::default()
        },
        CompactionInputPruneMode::RequiredForWindowDowngrade,
    );

    assert_eq!(
        prepared.source_messages[4].body,
        "stderr: compilation failed\nerror[E0425]: cannot find value"
    );
    assert_eq!(prepared.prune_report.preserved_tool_output_count, 2);
    assert_eq!(prepared.prune_report.pruned_tool_output_count, 2);
    assert_eq!(prepared.prune_report.dropped_source_message_count, 0);
    assert_eq!(prepared.prune_report.truncated_source_message_count, 0);
}
