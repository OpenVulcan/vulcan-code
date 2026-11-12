use super::*;
use vulcan_context::{
    ContextCompactionPolicy, SessionMessageEntry, SessionMessageKind, SessionMessageRecordKind,
};

/// Image retirement can compact a short conversation while retaining its complete text.
/// 图片退役可以压缩短会话并完整保留其文本。
#[test]
fn image_compaction_can_retain_the_entire_text_tail() {
    let messages = [test_context_message(
        "new-image",
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "Current prompt",
    )];
    let input = || CompactionPlanInput {
        session_label: "short-conversation",
        messages: &messages,
        all_messages: &messages,
        context_start_message_id: None,
        source_until_message_id: None,
        policy: ContextCompactionPolicy::default(),
    };
    assert!(plan_context_compaction(input()).is_none());
    let plan = plan_conversation_image_compaction(input()).expect("image-only compaction plan");
    assert!(plan.source_messages.is_empty());
    assert_eq!(plan.tail_selection.retained_message_ids, ["new-image"]);
}

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

/// Builds an empty frozen tail report aligned with one test policy.
/// 构建与测试策略对齐的空冻结尾部报告。
///
/// # Parameters
/// # 参数
/// `policy` is the exact policy copied into the test report.
/// `policy` 是复制到测试报告中的精确策略。
///
/// # Returns
/// # 返回值
/// A structurally valid zero-message selection report.
/// 返回结构有效的零消息选择报告。
fn test_tail_selection(policy: ContextCompactionPolicy) -> CompactionTailSelectionReport {
    CompactionTailSelectionReport {
        context_limit: policy.tail_context_limit,
        policy_source: policy.tail_policy_source,
        nominal_token_budget: policy.preserve_recent_tokens,
        segment_limit: policy.tail_turns,
        retained_segment_count: 0,
        retained_message_ids: Vec::new(),
        retained_token_estimate: 0,
        previewed_tool_output_count: 0,
        omitted_tool_output_token_estimate: 0,
        newest_segment_exceeds_nominal_budget: false,
    }
}

/// Verifies prompt-shaped compaction results keep a compact preview separate from the injected continuation body.
/// 验证提示词形态压缩结果会把紧凑预览与注入的续写正文分开保留。
#[test]
fn compaction_result_carries_compact_preview() {
    let policy = ContextCompactionPolicy::default();
    let plan = CompactionPlan {
        source_messages: Vec::new(),
        source_until_message_id: Some(String::from("m-1")),
        source_until_turn_index: Some(1),
        retained_tail_start_turn_index: Some(2),
        compaction_id: String::from("cp-1"),
        created_at: String::from("20"),
        policy,
        tail_selection: test_tail_selection(policy),
    };

    let result = CompactionResult::from_model_summary(
        &plan,
        "保留目标、约束和未完成工作，同时不要重复工具说明。",
    );

    assert_eq!(result.tail_selection, plan.tail_selection);
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
    let policy = ContextCompactionPolicy::default();
    let plan = CompactionPlan {
        source_messages: Vec::new(),
        source_until_message_id: Some(String::from("m-1")),
        source_until_turn_index: Some(1),
        retained_tail_start_turn_index: None,
        compaction_id: String::from("cp-1"),
        created_at: String::from("20"),
        policy,
        tail_selection: test_tail_selection(policy),
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
    let policy = ContextCompactionPolicy::default();
    let plan = CompactionPlan {
        source_messages: Vec::new(),
        source_until_message_id: Some(String::from("m-1")),
        source_until_turn_index: Some(1),
        retained_tail_start_turn_index: Some(2),
        compaction_id: String::from("cp-1"),
        created_at: String::from("20"),
        policy,
        tail_selection: test_tail_selection(policy),
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

/// Verifies the live preview hides protocol tags and switches to the summary once it starts.
/// 验证实时预览隐藏协议标签，并在摘要开始后切换为摘要正文。
#[test]
fn preview_compaction_model_output_follows_the_streaming_protocol() {
    assert_eq!(
        preview_compaction_model_output("<analysis>\ndraft line"),
        "draft line"
    );
    assert_eq!(
        preview_compaction_model_output(
            "<analysis>\ndraft\n</analysis>\n<summary>\nNext Step:\n- partial"
        ),
        "Next Step:\n- partial"
    );
    assert_eq!(
        preview_compaction_model_output(
            "<analysis>a</analysis>\n<summary>\nDone\n</summary>\ntrailing"
        ),
        "Done"
    );
    assert_eq!(
        preview_compaction_model_output("untagged summary"),
        "untagged summary"
    );
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
        source_until_message_id: Some("user-1"),
        policy: ContextCompactionPolicy::default(),
    })
    .expect("explicit source boundary should produce a compaction plan");

    assert_eq!(plan.source_until_message_id.as_deref(), Some("user-1"));
    assert_eq!(
        plan.source_messages
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec!["user-0", "assistant-0", "user-1"]
    );
    assert_eq!(plan.retained_tail_start_turn_index, Some(1));
    assert_eq!(
        plan.tail_selection.retained_message_ids,
        vec!["assistant-1"]
    );
}

/// Verifies an explicit source boundary cannot split one assistant/provider execution segment.
/// 验证明示来源边界不能切开同一 Assistant/Provider 执行段。
#[test]
fn plan_context_compaction_rejects_explicit_boundary_inside_segment() {
    let messages = vec![
        test_context_message(
            "user-0",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "older request",
        ),
        test_context_message(
            "assistant-0",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "assistant output",
        ),
        test_tool_result_message("tool-0", "tool output in the same segment"),
        test_context_message(
            "assistant-1",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "new segment",
        ),
    ];

    let plan = plan_context_compaction(CompactionPlanInput {
        session_label: "unsafe-explicit-boundary",
        messages: &messages,
        all_messages: &messages,
        context_start_message_id: None,
        source_until_message_id: Some("assistant-0"),
        policy: ContextCompactionPolicy::default(),
    });

    assert!(plan.is_none());
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
            tail_turns: 1,
            preserve_recent_tokens: 10_000,
            ..ContextCompactionPolicy::default()
        },
    })
    .expect("provider-grouped API rounds should produce a compaction plan");

    assert_eq!(plan.source_until_message_id.as_deref(), Some("assistant-b"));
    assert_eq!(
        plan.source_messages
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec!["user-0", "assistant-a", "assistant-b"]
    );
}

/// Verifies history before the first Assistant is counted inside that first execution segment.
/// 验证首条 Assistant 之前的历史会计入第一个执行段，而不是独立成段。
#[test]
fn plan_context_compaction_assigns_leading_history_to_first_segment() {
    let messages = vec![
        test_context_message(
            "user-leading",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "leading request",
        ),
        test_context_message(
            "assistant-first",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "first provider segment",
        ),
        test_context_message(
            "user-trailing-first",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "input consumed before the next provider output",
        ),
        test_context_message(
            "assistant-newest",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "newest provider segment",
        ),
    ];

    let plan = plan_context_compaction(CompactionPlanInput {
        session_label: "leading-history",
        messages: &messages,
        all_messages: &messages,
        context_start_message_id: None,
        source_until_message_id: None,
        policy: ContextCompactionPolicy {
            tail_turns: 1,
            preserve_recent_tokens: 5_000,
            ..ContextCompactionPolicy::default()
        },
    })
    .expect("the complete first execution segment should be compacted");

    assert_eq!(
        plan.source_messages
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec!["user-leading", "assistant-first", "user-trailing-first"]
    );
    assert_eq!(
        plan.tail_selection.retained_message_ids,
        vec!["assistant-newest"]
    );
    assert_eq!(plan.tail_selection.retained_segment_count, 1);
}

/// Verifies long tasks without new user messages retain assistant/provider segments and preview closed tool output.
/// 验证没有新 User 消息的长任务仍按 Assistant/Provider 执行段保留，并预览已闭合工具输出。
#[test]
fn plan_context_compaction_selects_segments_without_user_turns() {
    let mut oldest = test_context_message(
        "assistant-oldest",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::AssistantOutput,
        "oldest provider segment",
    );
    oldest.metadata = Some(serde_json::json!({ "provider_message_id": "provider-0" }));
    let mut historical = test_context_message(
        "assistant-historical",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::ToolCallOutput,
        "historical tool call",
    );
    historical.metadata = Some(serde_json::json!({ "provider_message_id": "provider-1" }));
    let historical_tool = test_tool_result_message("tool-historical", &"中".repeat(3_500));
    let mut newest = test_context_message(
        "assistant-newest",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::AssistantOutput,
        "newest provider segment",
    );
    newest.metadata = Some(serde_json::json!({ "provider_message_id": "provider-2" }));
    let messages = vec![oldest, historical, historical_tool, newest];
    let policy = ContextCompactionPolicy {
        tail_turns: 2,
        preserve_recent_tokens: 5_000,
        ..ContextCompactionPolicy::default()
    };

    let plan = plan_context_compaction(CompactionPlanInput {
        session_label: "long-task",
        messages: &messages,
        all_messages: &messages,
        context_start_message_id: None,
        source_until_message_id: None,
        policy,
    })
    .expect("the oldest provider segment should be compacted");

    assert_eq!(
        plan.source_until_message_id.as_deref(),
        Some("assistant-oldest")
    );
    assert_eq!(
        plan.tail_selection.retained_message_ids,
        vec![
            "assistant-historical",
            "tool-historical",
            "assistant-newest"
        ]
    );
    assert_eq!(plan.tail_selection.retained_segment_count, 2);
    assert_eq!(plan.tail_selection.previewed_tool_output_count, 1);
    assert!(plan.tail_selection.omitted_tool_output_token_estimate > 0);
}

/// Verifies hundreds of no-user provider segments retain exactly twenty complete parallel-tool groups at 1M.
/// 验证数百个无 User 的 Provider 执行段在 1M 窗口下精确保留二十个完整并行工具组。
#[test]
fn plan_context_compaction_scales_to_hundreds_of_parallel_tool_segments() {
    // messages models 300 uninterrupted provider rounds, each with two parallel calls and two results.
    // messages 模拟 300 个无间断 Provider 执行轮次，每轮包含两个并行调用与两个结果。
    let mut messages = Vec::new();
    for segment_index in 0..300usize {
        // providerMessageId groups both parallel tool-call rows into one execution segment.
        // providerMessageId 把两条并行工具调用行归入同一执行段。
        let provider_message_id = format!("provider-{segment_index}");
        for call_index in 0..2usize {
            // toolCall is one Assistant-owned parallel call row within the current provider round.
            // toolCall 是当前 Provider 轮次中一条归属 Assistant 的并行调用行。
            let mut tool_call = test_context_message(
                &format!("assistant-{call_index}-{segment_index}"),
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::ToolCallOutput,
                &format!("parallel call {call_index} for segment {segment_index}"),
            );
            tool_call.metadata = Some(serde_json::json!({
                "provider_message_id": provider_message_id.clone(),
                "tool_name": "shell_command",
                "tool_call_id": format!("call-{call_index}-{segment_index}"),
            }));
            messages.push(tool_call);
        }
        messages.push(test_tool_result_message(
            &format!("tool-a-{segment_index}"),
            "parallel result a",
        ));
        messages.push(test_tool_result_message(
            &format!("tool-b-{segment_index}"),
            "parallel result b",
        ));
    }
    // resolution is the single shared 1M formula result used to construct the exact test policy.
    // resolution 是用于构造精确测试策略的唯一共享 1M 公式结果。
    let resolution = vulcan_context::resolve_context_compaction_tail_policy(Some(1_000_000));
    // policy carries the formula result into the production planner shared by all three entry paths.
    // policy 把公式结果带入三条入口共享的生产规划器。
    let policy = ContextCompactionPolicy {
        tail_turns: resolution.tail_segments,
        preserve_recent_tokens: resolution.preserve_recent_tokens,
        tail_context_limit: resolution.context_limit,
        tail_policy_source: resolution.source,
        ..ContextCompactionPolicy::default()
    };

    // plan must retain the maximum complete suffix without splitting any four-row parallel group.
    // plan 必须保留最大完整后缀，且不能拆分任何四行并行工具组。
    let plan = plan_context_compaction(CompactionPlanInput {
        session_label: "hundreds-of-parallel-segments",
        messages: &messages,
        all_messages: &messages,
        context_start_message_id: None,
        source_until_message_id: None,
        policy,
    })
    .expect("the first 280 complete provider segments should be summarized");

    assert_eq!(plan.tail_selection.segment_limit, 20);
    assert_eq!(plan.tail_selection.nominal_token_budget, 50_000);
    assert_eq!(plan.tail_selection.retained_segment_count, 20);
    assert_eq!(plan.tail_selection.retained_message_ids.len(), 80);
    assert_eq!(
        plan.tail_selection
            .retained_message_ids
            .first()
            .map(String::as_str),
        Some("assistant-0-280")
    );
    assert_eq!(
        plan.tail_selection
            .retained_message_ids
            .last()
            .map(String::as_str),
        Some("tool-b-299")
    );
    assert_eq!(plan.source_until_message_id.as_deref(), Some("tool-b-279"));
}

/// Verifies an oversized newest execution segment remains complete and excludes every older segment.
/// 验证超预算最新执行段保持完整，并排除所有更旧执行段。
#[test]
fn plan_context_compaction_keeps_oversized_newest_segment_alone() {
    let mut old = test_context_message(
        "assistant-old",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::AssistantOutput,
        "old segment",
    );
    old.metadata = Some(serde_json::json!({ "provider_message_id": "provider-old" }));
    let mut newest = test_context_message(
        "assistant-newest",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::ToolCallOutput,
        "newest tool call",
    );
    newest.metadata = Some(serde_json::json!({ "provider_message_id": "provider-new" }));
    let newest_tool = test_tool_result_message("tool-newest", &"新".repeat(7_000));
    let messages = vec![old, newest, newest_tool];
    let policy = ContextCompactionPolicy {
        tail_turns: 5,
        preserve_recent_tokens: 5_000,
        ..ContextCompactionPolicy::default()
    };

    let plan = plan_context_compaction(CompactionPlanInput {
        session_label: "oversized-newest",
        messages: &messages,
        all_messages: &messages,
        context_start_message_id: None,
        source_until_message_id: None,
        policy,
    })
    .expect("the older segment should be compacted");

    assert_eq!(
        plan.tail_selection.retained_message_ids,
        vec!["assistant-newest", "tool-newest"]
    );
    assert_eq!(plan.tail_selection.retained_segment_count, 1);
    assert_eq!(plan.tail_selection.previewed_tool_output_count, 0);
    assert!(plan.tail_selection.newest_segment_exceeds_nominal_budget);
}

/// Verifies a conversation without any Assistant message is one protected newest segment.
/// 验证完全没有 Assistant 消息的会话会作为一个受保护最新执行段。
#[test]
fn plan_context_compaction_protects_all_messages_without_assistant() {
    let messages = vec![
        test_context_message(
            "user-0",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "first user message",
        ),
        test_context_message(
            "user-1",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "second user message",
        ),
    ];

    let plan = plan_context_compaction(CompactionPlanInput {
        session_label: "no-assistant",
        messages: &messages,
        all_messages: &messages,
        context_start_message_id: None,
        source_until_message_id: None,
        policy: ContextCompactionPolicy::default(),
    });

    assert!(plan.is_none());
}

/// Verifies a trailing user message remains inside the newest Assistant/provider segment.
/// 验证尾随 User 消息会保留在最新 Assistant/Provider 执行段中。
#[test]
fn plan_context_compaction_keeps_trailing_user_in_newest_segment() {
    let messages = vec![
        test_context_message(
            "assistant-old",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "old segment",
        ),
        test_context_message(
            "assistant-newest",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "newest provider output",
        ),
        test_context_message(
            "user-trailing",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "user input not consumed by another assistant output",
        ),
    ];

    let plan = plan_context_compaction(CompactionPlanInput {
        session_label: "trailing-user",
        messages: &messages,
        all_messages: &messages,
        context_start_message_id: None,
        source_until_message_id: None,
        policy: ContextCompactionPolicy {
            tail_turns: 1,
            preserve_recent_tokens: 5_000,
            ..ContextCompactionPolicy::default()
        },
    })
    .expect("the old assistant segment should be compacted");

    assert_eq!(
        plan.tail_selection.retained_message_ids,
        vec!["assistant-newest", "user-trailing"]
    );
    assert_eq!(plan.tail_selection.retained_segment_count, 1);
}

/// Verifies the segment limit stops at the newest contiguous suffix.
/// 验证执行段数量上限会在最新连续后缀处停止。
#[test]
fn plan_context_compaction_stops_at_segment_limit() {
    let messages = (0..4)
        .map(|index| {
            test_context_message(
                &format!("assistant-{index}"),
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                &format!("segment {index}"),
            )
        })
        .collect::<Vec<_>>();

    let plan = plan_context_compaction(CompactionPlanInput {
        session_label: "segment-limit",
        messages: &messages,
        all_messages: &messages,
        context_start_message_id: None,
        source_until_message_id: None,
        policy: ContextCompactionPolicy {
            tail_turns: 2,
            preserve_recent_tokens: 5_000,
            ..ContextCompactionPolicy::default()
        },
    })
    .expect("two oldest segments should be compacted");

    assert_eq!(
        plan.tail_selection.retained_message_ids,
        vec!["assistant-2", "assistant-3"]
    );
    assert_eq!(plan.tail_selection.retained_segment_count, 2);
}

/// Verifies a too-large nearer segment blocks every older segment instead of allowing a gap.
/// 验证过大的较近执行段会阻止全部更旧执行段，不能跳过它形成时间空洞。
#[test]
fn plan_context_compaction_does_not_skip_large_nearer_segment() {
    let messages = vec![
        test_context_message(
            "assistant-old-small",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "old small segment",
        ),
        test_context_message(
            "assistant-near-large",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            &"大".repeat(6_000),
        ),
        test_context_message(
            "assistant-newest-small",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "newest small segment",
        ),
    ];

    let plan = plan_context_compaction(CompactionPlanInput {
        session_label: "no-gap",
        messages: &messages,
        all_messages: &messages,
        context_start_message_id: None,
        source_until_message_id: None,
        policy: ContextCompactionPolicy {
            tail_turns: 5,
            preserve_recent_tokens: 5_000,
            ..ContextCompactionPolicy::default()
        },
    })
    .expect("the nearer large segment should stop backward expansion");

    assert_eq!(
        plan.tail_selection.retained_message_ids,
        vec!["assistant-newest-small"]
    );
    assert_eq!(
        plan.source_until_message_id.as_deref(),
        Some("assistant-near-large")
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
    let long_output = (0..400)
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
            target_input_budget_tokens: 1_500,
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
            .contains("[Context compaction omitted: ")
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
    let long_output = (0..400)
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
            target_input_budget_tokens: 1_500,
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
