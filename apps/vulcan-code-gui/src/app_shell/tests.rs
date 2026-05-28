use super::workbench_model_state::{
    format_openrouter_percentage, format_openrouter_price_per_mtokens,
    format_openrouter_throughput_tokens_per_second, value_as_f64,
};
use super::{
    ActiveSessionCompactionState, CommandSessionToolModalSummary, ComposerSlashCommand, Message,
    PromptCompactionExecution, SessionPromptCompactionTrigger, ToolModalPresentation,
    VulcanWorkbench, anthropic_pending_prompt_dispatch_is_blocked,
    build_background_legacy_maintenance_queue, build_command_session_stop_result_metadata,
    build_composer_slash_command_state, build_session_record_message_id,
    build_session_turn_stats_from_state, clear_selectable_transcript_render_cache_stash,
    compact_tool_detail_message_for_projection, composer_text_has_submittable_content,
    composer_visible_line_count, generated_attachments_from_tool_metadata,
    merge_projected_session_messages_into_existing_window, merge_tool_display_messages,
    pending_prompt_dispatch_index, prune_session_messages_to_limit,
    sanitize_workspace_history_search_query_input, session_event_store_missing_error,
    task_session_payload_from_tool_metadata, tool_modal_presentation_from_agent_view_attributes,
};
use crate::app_shell::markdown;
use crate::context_budget::{ContextBudgetSnapshot, ContextCompactionPolicy};
use crate::context_compaction::{CompactionPlan, CompactionSkillResult};
use crate::conversation_protocol::SessionConversationState;
use crate::model::{
    PendingPromptRuntimeOptions, PendingSessionPromptEntry, PendingSessionPromptSource, SessionId,
    SessionMessageEntry, SessionMessageKind, SessionMessageRecordKind, SessionRunPhase,
    SessionRuntimeStateEntry, SessionState, SessionSummary, SessionTurnStatsStatus,
};
use crate::tooling::command_session_runtime::{CommandSessionSnapshot, CommandSessionStatus};
use crate::workspace_store::SESSION_HISTORY_SEARCH_QUERY_MAX_CHARS;
use iced::Task;
use iced::widget::text_editor;
use serde_json::{Value, json};
use std::path::PathBuf;
use vulcan_agent_view::AgentViewBlock;
use vulcan_core::types::{ConversationAttachmentKind, ProviderUsageSnapshot};

/// Builds a compact session summary for queue-related unit tests.
/// 为队列相关单元测试构建一条紧凑会话摘要。
fn test_session_summary(id: SessionId, title: &str) -> SessionSummary {
    SessionSummary::new(
        id,
        None,
        title.to_string(),
        String::new(),
        SessionState::Draft,
        0,
        0,
        String::from("刚刚"),
    )
}

/// Builds one compact pending-session prompt entry for queue-order unit tests.
/// 为队列顺序相关单元测试构建一条紧凑的待发送提示词项。
fn test_pending_prompt_entry(
    source: PendingSessionPromptSource,
    prompt: &str,
) -> PendingSessionPromptEntry {
    PendingSessionPromptEntry {
        id: format!("entry-{prompt}"),
        prompt: prompt.to_string(),
        preview: prompt.to_string(),
        attachments: Vec::new(),
        model_reference: None,
        runtime_options: PendingPromptRuntimeOptions::default(),
        source,
        task_id: (source == PendingSessionPromptSource::DelegateCompletion)
            .then(|| format!("task-{prompt}")),
        panel_entry_id: None,
        created_at: String::from("1"),
    }
}

/// Builds a compact comparable fingerprint for one transcript window so tests can detect unintended locale-driven mutation without requiring serde support.
/// 为单份转录窗口构建紧凑可比指纹，使测试无需依赖 serde 支持也能发现语言驱动的意外变更。
fn message_window_fingerprint(messages: &[SessionMessageEntry]) -> Vec<String> {
    messages
        .iter()
        .map(|message| {
            format!(
                "{}|{:?}|{:?}|{}|{}|{}|{}",
                message.id,
                message.kind,
                message.record_kind,
                message.visible,
                message.title,
                message.body,
                message
                    .metadata
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_default()
            )
        })
        .collect()
}

/// Verifies command-session transcript attributes can be reconstructed into the richer tool-modal presentation metadata.
/// 验证命令会话转录属性可以重建为更丰富的工具弹层展示元数据。
#[test]
fn tool_modal_presentation_parses_command_session_attributes() {
    let mut attributes = vulcan_agent_view::AgentViewAttributes::new();
    attributes.insert(
        String::from("tool-presentation-kind"),
        String::from("command-session"),
    );
    attributes.insert(
        String::from("command-session-id"),
        String::from("00000000-0000-0000-0000-000000000111"),
    );
    attributes.insert(
        String::from("command-session-owner-id"),
        String::from("00000000-0000-0000-0000-000000000010"),
    );
    attributes.insert(
        String::from("command-session-status"),
        String::from("running"),
    );
    attributes.insert(String::from("command-session-action"), String::from("read"));
    attributes.insert(String::from("command-shell"), String::from("powershell"));
    attributes.insert(
        String::from("command-working-directory"),
        String::from("D:\\openvulcan\\vulcan-code"),
    );
    attributes.insert(
        String::from("command-text"),
        String::from("cargo test -p vulcan-code-gui"),
    );
    attributes.insert(String::from("command-last-seq"), String::from("17"));
    attributes.insert(String::from("command-stdout-bytes"), String::from("1024"));
    attributes.insert(String::from("command-stderr-bytes"), String::from("32"));
    attributes.insert(
        String::from("command-session-returned-chunk-count"),
        String::from("4"),
    );
    attributes.insert(
        String::from("command-session-bytes-written"),
        String::from("12"),
    );
    attributes.insert(
        String::from("command-session-has-more"),
        String::from("true"),
    );
    attributes.insert(
        String::from("command-output-overflowed"),
        String::from("false"),
    );

    let presentation = tool_modal_presentation_from_agent_view_attributes(&attributes);
    let ToolModalPresentation::CommandSession(summary) = presentation else {
        panic!("expected command-session modal presentation");
    };

    assert_eq!(
        summary.command_session_id,
        SessionId::parse_str("00000000-0000-0000-0000-000000000111")
            .expect("parse command session id")
    );
    assert_eq!(
        summary.owner_session_id,
        Some(
            SessionId::parse_str("00000000-0000-0000-0000-000000000010")
                .expect("parse owner session id"),
        )
    );
    assert_eq!(summary.status, "running");
    assert_eq!(summary.action.as_deref(), Some("read"));
    assert_eq!(summary.shell.as_deref(), Some("powershell"));
    assert_eq!(summary.last_seq, Some(17));
    assert_eq!(summary.stdout_bytes, Some(1024));
    assert_eq!(summary.stderr_bytes, Some(32));
    assert_eq!(summary.returned_chunk_count, Some(4));
    assert_eq!(summary.bytes_written, Some(12));
    assert_eq!(summary.has_more, Some(true));
    assert!(!summary.output_overflowed);
}

/// Verifies GUI-applied command-session snapshots update the modal metadata immediately after a local stop action.
/// 验证 GUI 应用命令会话快照后，会在本地停止动作完成后立即更新弹层元数据。
#[test]
fn tool_modal_presentation_applies_command_session_snapshot() {
    let command_session_id = SessionId::parse_str("00000000-0000-0000-0000-000000000222")
        .expect("parse command session id");
    let mut presentation = ToolModalPresentation::CommandSession(CommandSessionToolModalSummary {
        command_session_id,
        owner_session_id: Some(
            SessionId::parse_str("00000000-0000-0000-0000-000000000001")
                .expect("parse owner session id"),
        ),
        action: Some(String::from("read")),
        status: String::from("running"),
        shell: Some(String::from("powershell")),
        working_directory: Some(String::from("D:\\openvulcan\\vulcan-code")),
        command_text: Some(String::from("ping 127.0.0.1")),
        failure_reason: None,
        exit_code: None,
        last_seq: Some(3),
        stdout_bytes: Some(24),
        stderr_bytes: Some(0),
        returned_chunk_count: Some(1),
        bytes_written: None,
        has_more: Some(true),
        output_overflowed: false,
    });
    let snapshot = CommandSessionSnapshot {
        command_session_id,
        owner_session_id: SessionId::parse_str("00000000-0000-0000-0000-000000000001")
            .expect("parse owner session id"),
        command: String::from("ping 127.0.0.1"),
        shell_label: String::from("powershell"),
        launcher_shell_label: String::from("powershell"),
        cwd: PathBuf::from("D:\\openvulcan\\vulcan-code"),
        status: CommandSessionStatus::Stopped,
        exit_code: Some(1),
        failure_reason: Some(String::from("stopped by user")),
        stdout_bytes: 256,
        stderr_bytes: 8,
        last_seq: 11,
        pending_stdin_line: false,
    };

    assert!(presentation.apply_command_session_snapshot(&snapshot));
    let ToolModalPresentation::CommandSession(summary) = presentation else {
        panic!("expected command-session modal presentation");
    };
    assert_eq!(
        summary.owner_session_id,
        Some(
            SessionId::parse_str("00000000-0000-0000-0000-000000000001")
                .expect("parse owner session id"),
        )
    );
    assert_eq!(summary.status, "stopped");
    assert_eq!(summary.exit_code, Some(1));
    assert_eq!(summary.failure_reason.as_deref(), Some("stopped by user"));
    assert_eq!(summary.stdout_bytes, Some(256));
    assert_eq!(summary.stderr_bytes, Some(8));
    assert_eq!(summary.last_seq, Some(11));
    assert_eq!(summary.action.as_deref(), Some("read"));
}

/// Verifies command-session modal summaries fall back to `stopped` once the runtime can no longer resolve the referenced session id, instead of preserving a stale historical `running` state forever.
/// 验证当运行时已经无法解析被引用的命令会话标识时，命令会话弹层摘要会回退为 `stopped`，而不是永久保留过期的历史 `running` 状态。
#[test]
fn command_session_modal_summary_marks_missing_runtime_session_as_stopped() {
    let (workbench, _task) = VulcanWorkbench::new();
    let summary = CommandSessionToolModalSummary {
        command_session_id: SessionId::from_u128(0xBEEF),
        owner_session_id: Some(SessionId::from_u128(0xCAFE)),
        action: Some(String::from("read")),
        status: String::from("running"),
        shell: Some(String::from("pwsh")),
        working_directory: Some(String::from("D:\\openvulcan\\vulcan-code")),
        command_text: Some(String::from("cargo test")),
        failure_reason: None,
        exit_code: None,
        last_seq: Some(7),
        stdout_bytes: Some(256),
        stderr_bytes: Some(0),
        returned_chunk_count: Some(2),
        bytes_written: None,
        has_more: Some(true),
        output_overflowed: false,
    };

    let resolved = workbench.command_session_summary_with_live_runtime_state(
        summary.owner_session_id.unwrap(),
        &summary,
    );

    assert_eq!(resolved.status, "stopped");
    assert_eq!(resolved.has_more, Some(false));
    assert_eq!(resolved.command_session_id, summary.command_session_id);
    assert_eq!(resolved.owner_session_id, summary.owner_session_id);
}

/// Verifies rendered command-session transcript cards also fall back to `stopped` when their runtime session id is no longer present, so the main transcript cannot keep showing a dead session as running.
/// 验证当命令会话的运行时标识已经不存在时，渲染后的命令会话转录卡片同样会回退为 `stopped`，从而避免主转录继续把已死亡会话显示为 running。
#[test]
fn rendered_command_session_card_marks_missing_runtime_session_as_stopped() {
    let (mut workbench, _task) = VulcanWorkbench::new();
    let session_id = SessionId::from_u128(0x1200);
    workbench.sessions.clear();
    workbench
        .sessions
        .push(test_session_summary(session_id, "命令会话测试"));
    workbench.selected_session = session_id;
    workbench.session_messages.insert(
        session_id,
        vec![SessionMessageEntry {
            id: String::from("command-session-result-live-state"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::ToolResultOutput,
            visible: true,
            title: String::from("工具返回 shell_session"),
            body: String::from("命令会话历史上仍显示 running。"),
            metadata: Some(json!({
                "tool_call_id": "call-command-session-live-state",
                "tool_name": "shell_session",
                "tool_display_state": "completed",
                "kind": "command_session",
                "payload": {
                    "action": "read",
                    "owner_session_id": session_id.to_string(),
                    "command_session_id": "00000000-0000-0000-0000-00000000beef",
                    "command": "cargo test",
                    "cwd": r"D:\openvulcan\vulcan-code",
                    "shell": "pwsh",
                    "status": "running",
                    "last_seq": 3,
                    "stdout_bytes": 64,
                    "stderr_bytes": 0,
                    "returned_chunk_count": 1,
                    "has_more": true,
                    "output_overflowed": false
                }
            })),
            created_at: String::from("1"),
        }],
    );

    workbench.refresh_session_agent_view_buffers(session_id);

    let rendered = workbench
        .rendered_session_inline_agent_view_document(session_id)
        .expect("render inline transcript document");
    let status = rendered
        .blocks
        .iter()
        .find_map(|block| match block {
            AgentViewBlock::Directive(directive) => {
                directive.attributes.get("command-session-status").cloned()
            }
            AgentViewBlock::Markdown(_) => None,
        })
        .expect("read command-session status");

    assert_eq!(status, "stopped");
}

/// Verifies session title generation only runs for the first user turn and leaves later turns unchanged.
/// 验证会话标题生成只会在首条用户轮次触发，后续轮次不会继续改写标题。
#[test]
fn session_title_generation_runs_only_for_first_user_turn() {
    let (mut workbench, _task) = VulcanWorkbench::new();
    let session_id = SessionId::from_u128(70001);
    workbench.sessions.clear();
    workbench
        .sessions
        .push(test_session_summary(session_id, "新对话"));
    workbench.selected_session = session_id;

    workbench
        .append_user_prompt_to_session_conversation(
            session_id,
            String::from("请帮我分析这个项目结构"),
            Vec::new(),
            Vec::new(),
        )
        .expect("append first user prompt");
    assert_eq!(workbench.session_user_turn_count(session_id), 1);
    let first_title = workbench
        .maybe_generate_session_title_from_prompt(session_id, "请帮我分析这个项目结构")
        .expect("first prompt should generate title");

    workbench
        .append_user_prompt_to_session_conversation(
            session_id,
            String::from("继续看看 provider 部分"),
            Vec::new(),
            Vec::new(),
        )
        .expect("append second user prompt");
    assert_eq!(workbench.session_user_turn_count(session_id), 2);

    let second_title_attempt = if workbench.session_user_turn_count(session_id) == 1 {
        workbench.maybe_generate_session_title_from_prompt(session_id, "继续看看 provider 部分")
    } else {
        None
    };

    assert!(second_title_attempt.is_none());
    assert_eq!(
        workbench
            .session_by_id(session_id)
            .map(|session| session.title.as_str()),
        Some(first_title.as_str())
    );
}

/// Verifies missing event-store file errors are recognized so the workbench can offer a rebuild path instead of only showing a passive status line.
/// 验证缺失事件存储文件错误能够被识别，从而让工作台提供重建路径，而不是只显示被动状态栏报错。
#[test]
fn session_event_store_missing_error_detects_manifest_not_found() {
    let error = "读取 JSON 文档失败（C:\\Users\\20000\\.vulcan\\code\\sessions\\project\\session\\manifest.json）：系统找不到指定的路径。 (os error 3)";
    assert!(session_event_store_missing_error(error));
    assert!(!session_event_store_missing_error(
        "解析 JSON 文档失败：unexpected eof"
    ));
}

/// Verifies OpenRouter endpoint prices are rendered in shared USD-per-million-token units.
/// 验证 OpenRouter 端点价格会以统一的每百万 tokens 美元单位渲染。
#[test]
fn openrouter_price_format_uses_usd_per_mtokens() {
    assert_eq!(
        format_openrouter_price_per_mtokens(Some(0.000002_f64)).as_deref(),
        Some("$2.00/MTokens")
    );
    assert_eq!(
        format_openrouter_price_per_mtokens(Some(0.0000005_f64)).as_deref(),
        Some("$0.50/MTokens")
    );
}

/// Verifies OpenRouter availability values are rendered as percentages with two decimal places.
/// 验证 OpenRouter 可用性数值会被格式化为保留两位小数的百分比。
#[test]
fn openrouter_percentage_format_keeps_two_decimal_places() {
    assert_eq!(
        format_openrouter_percentage(Some(99.21630094043887_f64)).as_deref(),
        Some("99.22%")
    );
}

/// Verifies OpenRouter throughput values are rendered as tokens-per-second with two decimal places.
/// 验证 OpenRouter 吞吐速度数值会被格式化为保留两位小数的每秒 tokens。
#[test]
fn openrouter_throughput_format_keeps_two_decimal_places() {
    assert_eq!(
        format_openrouter_throughput_tokens_per_second(Some(42.678_f64)).as_deref(),
        Some("42.68 tok/s")
    );
}

/// Verifies object-shaped OpenRouter metrics fall back to the p50 percentile for compact display surfaces.
/// 验证对象结构的 OpenRouter 指标会回退到 p50 百分位，用于紧凑展示界面。
#[test]
fn openrouter_metric_object_prefers_p50_percentile() {
    let metric = json!({
        "p50": 415,
        "p75": 516,
        "p90": 669.9,
        "p99": 1312.18
    });

    assert_eq!(value_as_f64(Some(&metric)), Some(415.0));
}

/// Verifies that an empty composer still reserves one visible line.
/// 验证空提示词编辑器仍会保留一条可见逻辑行。
#[test]
fn composer_visible_line_count_keeps_one_line_for_empty_content() {
    let buffer = text_editor::Content::with_text("");

    assert_eq!(composer_visible_line_count(&buffer, 420.0), 1);
}

/// Verifies that a trailing newline immediately expands the composer to the next logical line.
/// 验证尾部换行会立刻把提示词编辑器扩展到下一条逻辑行。
#[test]
fn composer_visible_line_count_preserves_trailing_blank_line() {
    let buffer = text_editor::Content::with_text("第一行\n");

    assert_eq!(composer_visible_line_count(&buffer, 420.0), 2);
}

/// Verifies that pasted workspace-history search text is flattened and bounded before rendering.
/// 验证粘贴的工作台历史搜索文本在渲染前会被压平并限制长度。
#[test]
fn sanitize_workspace_history_search_query_input_limits_pasted_text() {
    let pasted = format!(
        "第一行\n\t第二行 {}",
        "x".repeat(SESSION_HISTORY_SEARCH_QUERY_MAX_CHARS + 64)
    );
    let sanitized = sanitize_workspace_history_search_query_input(&pasted);

    assert!(sanitized.starts_with("第一行 第二行 "));
    assert!(!sanitized.contains('\n'));
    assert_eq!(
        sanitized.chars().count(),
        SESSION_HISTORY_SEARCH_QUERY_MAX_CHARS
    );
}

/// Verifies that one long logical line expands into multiple visible lines when the composer width becomes narrow enough.
/// 验证当提示词编辑器宽度变窄时，单条超长逻辑行会自动扩展为多条可见行。
#[test]
fn composer_visible_line_count_expands_for_soft_wrapped_content() {
    let buffer = text_editor::Content::with_text(
        "This is a long composer line that should wrap automatically inside a narrow surface.",
    );

    assert!(composer_visible_line_count(&buffer, 180.0) > 1);
}

/// Verifies whitespace-only composer payloads stay non-submittable until visible content is entered.
/// 验证仅含空白字符的提示词载荷会保持不可提交，直到输入了可见内容。
#[test]
fn composer_text_requires_visible_content_before_submit() {
    assert!(!composer_text_has_submittable_content(""));
    assert!(!composer_text_has_submittable_content("   \n\t  "));
    assert!(composer_text_has_submittable_content("执行这个任务"));
    assert!(composer_text_has_submittable_content("\n  执行这个任务  "));
}

/// Verifies entering a bare slash exposes all supported inline slash commands in stable order.
/// 验证输入单独的斜杠时会按稳定顺序展示全部受支持的内联 slash 指令。
#[test]
fn composer_slash_command_state_lists_all_commands_for_root_trigger() {
    let state = build_composer_slash_command_state("/").expect("single slash should open commands");

    assert_eq!(state.filter_text, "");
    assert_eq!(
        state.visible_commands,
        vec![
            ComposerSlashCommand::Model,
            ComposerSlashCommand::Compact,
            ComposerSlashCommand::Reasoning,
        ]
    );
    assert_eq!(state.highlighted_index, Some(0));
}

/// Verifies unsupported slash inputs fall back to ordinary text while exact commands still support direct activation.
/// 验证不受支持的 slash 输入会回退为普通文本，而精确命令仍支持直接激活。
#[test]
fn composer_slash_command_state_rejects_unknown_queries_and_requires_exact_activation() {
    let filtered_state =
        build_composer_slash_command_state("/m").expect("prefix match should stay visible");
    assert_eq!(
        filtered_state.visible_commands,
        vec![ComposerSlashCommand::Model]
    );

    assert!(build_composer_slash_command_state("/ xasd").is_none());
    assert!(build_composer_slash_command_state("/abc 撒旦法撒旦").is_none());
    assert_eq!(
        ComposerSlashCommand::from_exact_input("/model"),
        Some(ComposerSlashCommand::Model)
    );
    assert_eq!(ComposerSlashCommand::from_exact_input("/model test"), None);
}

/// Verifies applying one prompt-style compaction result creates a checkpoint and rewrites the canonical anchor to a synthetic user continuation prompt.
/// 验证应用一份提示词式压缩结果会创建检查点，并将规范锚点改写为合成的用户续写提示词。
#[test]
fn apply_session_history_compaction_result_creates_checkpoint_for_prompt_summary() {
    let (mut workbench, _task) = VulcanWorkbench::new();
    let session_id = SessionId::from_u128(71001);
    workbench.sessions.clear();
    workbench
        .sessions
        .push(test_session_summary(session_id, "手动压缩测试"));
    workbench.selected_session = session_id;
    let mut messages = Vec::new();
    for turn_index in 0..5 {
        let user_ordinal = turn_index * 2 + 1;
        let assistant_ordinal = user_ordinal + 1;
        messages.push(SessionMessageEntry {
            id: format!("user-{user_ordinal}"),
            kind: SessionMessageKind::User,
            record_kind: SessionMessageRecordKind::Transcript,
            visible: true,
            title: String::from("用户"),
            body: format!("第 {} 轮用户问题", turn_index + 1),
            metadata: None,
            created_at: user_ordinal.to_string(),
        });
        messages.push(SessionMessageEntry {
            id: format!("assistant-{assistant_ordinal}"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::AssistantOutput,
            visible: true,
            title: String::from("输出"),
            body: format!("第 {} 轮助手回答", turn_index + 1),
            metadata: None,
            created_at: assistant_ordinal.to_string(),
        });
    }
    let conversation_state =
        crate::conversation_protocol::conversation_state_from_session_messages(&messages);
    workbench.session_messages.insert(session_id, messages);
    workbench.session_runtime_state.insert(
        session_id,
        SessionRuntimeStateEntry {
            live_context_budget_fingerprint: Some(String::from("budget-live")),
            live_context_assembly_fingerprint: Some(String::from("assembly-live")),
            conversation_state,
            updated_at: String::from("10"),
            ..SessionRuntimeStateEntry::default()
        },
    );

    let plan = CompactionPlan {
        source_messages: workbench
            .session_messages
            .get(&session_id)
            .cloned()
            .unwrap_or_default(),
        source_until_message_id: Some(String::from("assistant-4")),
        source_until_turn_index: Some(1),
        retained_tail_start_turn_index: Some(2),
        compaction_id: String::from("test-compaction-001"),
        created_at: String::from("20"),
        policy: ContextCompactionPolicy::default(),
    };
    let compaction_result = CompactionSkillResult::from_model_summary(
        &plan,
        "Keep using the synthetic continuation prompt for future turns.",
    );
    workbench.apply_session_history_compaction_result(
        session_id,
        compaction_result,
        "测试上下文压缩",
        String::from("测试压缩已完成。"),
    );
    let compacted_conversation = workbench.session_conversation_state(session_id);

    assert_eq!(
        workbench.session_compactions.get(&session_id).map(Vec::len),
        Some(1)
    );
    assert!(
        workbench
            .session_runtime_state
            .get(&session_id)
            .and_then(|state| state.last_compaction_id.as_ref())
            .is_some()
    );
    assert!(matches!(
        compacted_conversation
            .messages
            .first()
            .map(|message| &message.payload),
        Some(crate::conversation_protocol::SessionConversationPayload::UserText {
            text,
            ..
        }) if text.contains("Earlier context summary")
    ));
    assert_eq!(compacted_conversation.messages.len(), 3);
    assert!(
        workbench
            .session_runtime_state
            .get(&session_id)
            .and_then(|state| state.context_start_message_id.as_deref())
            .is_some()
    );
    assert_eq!(
        workbench
            .session_runtime_state
            .get(&session_id)
            .and_then(|state| state.anchor_context_budget_fingerprint.as_deref()),
        Some("budget-live")
    );
    assert_eq!(
        workbench
            .session_compactions
            .get(&session_id)
            .and_then(|entries| entries.first())
            .and_then(|entry| entry.context_budget_fingerprint.as_deref()),
        Some("budget-live")
    );
}

/// Verifies baseline mismatch recovery clears the active compaction anchor and stored context fingerprints before later automatic compaction runs.
/// 验证基线失配恢复会在后续自动压缩前清除活动压缩锚点与已存储上下文指纹。
#[test]
fn baseline_mismatch_recovery_clears_active_compaction_anchor() {
    let (mut workbench, _task) = VulcanWorkbench::new();
    let session_id = SessionId::from_u128(71009);
    workbench.sessions.clear();
    workbench
        .sessions
        .push(test_session_summary(session_id, "基线恢复测试"));
    workbench.selected_session = session_id;
    workbench.session_runtime_state.insert(
        session_id,
        SessionRuntimeStateEntry {
            last_compaction_id: Some(String::from("cp-restore-1")),
            context_start_message_id: Some(String::from("summary-restore-1")),
            live_context_budget_fingerprint: Some(String::from("budget-live")),
            live_context_assembly_fingerprint: Some(String::from("assembly-live")),
            anchor_context_budget_fingerprint: Some(String::from("budget-old")),
            anchor_context_assembly_fingerprint: Some(String::from("assembly-old")),
            updated_at: String::from("10"),
            ..SessionRuntimeStateEntry::default()
        },
    );

    workbench.recover_session_context_anchor_from_baseline_mismatch(
        session_id,
        &ContextBudgetSnapshot {
            baseline_matches_active_runtime: false,
            fingerprint: String::from("budget-new"),
            ..ContextBudgetSnapshot::default()
        },
    );

    let runtime_state = workbench
        .session_runtime_state
        .get(&session_id)
        .expect("runtime state should exist");
    assert!(runtime_state.last_compaction_id.is_none());
    assert!(runtime_state.context_start_message_id.is_none());
    assert!(runtime_state.anchor_context_budget_fingerprint.is_none());
    assert!(runtime_state.anchor_context_assembly_fingerprint.is_none());
    assert_eq!(
        runtime_state.live_context_budget_fingerprint.as_deref(),
        Some("budget-live")
    );
    assert_eq!(
        runtime_state.live_context_assembly_fingerprint.as_deref(),
        Some("assembly-live")
    );
    assert!(workbench.status_message.contains("上下文基线已失配"));
}

/// Verifies reopening one historical session without an established anchor baseline keeps the visible budget display stable instead of treating one live snapshot as a mismatch anchor.
/// 验证重新打开历史会话且尚未建立锚点基线时，可见预算展示会保持稳定，而不会把实时快照误当作失配锚点。
#[test]
fn session_budget_snapshot_without_real_request_returns_none() {
    let (mut workbench, _task) = VulcanWorkbench::new();
    let session_id = SessionId::from_u128(71010);
    workbench.sessions.clear();
    workbench
        .sessions
        .push(test_session_summary(session_id, "真实预算测试"));
    workbench.selected_session = session_id;
    workbench.session_runtime_state.insert(
        session_id,
        SessionRuntimeStateEntry {
            live_context_budget_fingerprint: Some(String::from("budget-old")),
            conversation_state: SessionConversationState::default(),
            ..SessionRuntimeStateEntry::default()
        },
    );

    let runtime_state = workbench
        .session_runtime_state
        .get(&session_id)
        .cloned()
        .expect("runtime state should exist");
    let snapshot =
        workbench.resolve_session_context_budget_snapshot(session_id, &runtime_state, &[], false);

    assert!(snapshot.is_none());
}

/// Verifies restored sessions show the last persisted provider usage while waiting for the next live request budget.
/// 验证恢复会话在等待下一次实时请求预算时，会显示最近一次持久化 Provider 用量。
#[test]
fn restored_session_context_usage_uses_last_provider_usage() {
    let (mut workbench, _task) = VulcanWorkbench::new();
    let session_id = SessionId::from_u128(71011);
    workbench.sessions.clear();
    workbench
        .sessions
        .push(test_session_summary(session_id, "历史用量恢复测试"));
    workbench.selected_session = session_id;
    workbench.session_runtime_state.insert(
        session_id,
        SessionRuntimeStateEntry {
            active_provider_id: Some(String::from("deepseek")),
            active_model_id: Some(String::from("deepseek-chat")),
            last_provider_usage: Some(ProviderUsageSnapshot {
                input_tokens: Some(14886),
                output_tokens: Some(191),
                reasoning_tokens: Some(17),
                total_tokens: Some(15094),
                cached_input_tokens: None,
                cache_creation_input_tokens: None,
            }),
            ..SessionRuntimeStateEntry::default()
        },
    );

    workbench.sync_context_usage_from_session_budget(session_id);

    let context_budget = workbench
        .visible_context_budget_snapshot
        .as_ref()
        .expect("restored session should expose historical context usage");
    assert_eq!(
        context_budget
            .observed_usage
            .as_ref()
            .and_then(|usage| usage.input_tokens),
        Some(14886)
    );
    assert!(workbench.context_usage > 0.0);
    assert!(workbench.context_usage_detail.contains("最近实测"));
}

/// Verifies active prompt-compaction state keeps the session on the blocking system-work lane so queued prompts do not auto-dispatch early.
/// 验证活动提示词压缩状态会让会话持续占用阻塞型系统工作通道，从而避免排队提示词过早自动派发。
#[test]
fn session_has_blocking_system_prompt_work_includes_active_compaction() {
    let (mut workbench, _task) = VulcanWorkbench::new();
    let session_id = SessionId::from_u128(71002);
    workbench.sessions.clear();
    workbench
        .sessions
        .push(test_session_summary(session_id, "压缩忙碌状态测试"));
    let (_task, handle) = Task::perform(async {}, |_| Message::Noop).abortable();
    workbench.active_session_compactions.insert(
        session_id,
        ActiveSessionCompactionState {
            run_id: 42,
            handle,
            previous_phase: SessionRunPhase::Ready,
            trigger: SessionPromptCompactionTrigger::ManualSlash,
            status_message_id: Some(String::from("compaction-status-1")),
            output_message_id: None,
        },
    );

    assert!(workbench.session_has_blocking_system_prompt_work(session_id));
}

/// Verifies streamed prompt-compaction output is appended into the visible running boundary row.
/// 验证流式提示词压缩输出会追加到可见的运行中边界行。
#[test]
fn prompt_compaction_output_appends_to_running_boundary_row() {
    let (mut workbench, _task) = VulcanWorkbench::new();
    let session_id = SessionId::from_u128(71003);
    workbench.sessions.clear();
    workbench
        .sessions
        .push(test_session_summary(session_id, "压缩状态面板测试"));
    let (_task, handle) = Task::perform(async {}, |_| Message::Noop).abortable();
    let status_message_id = workbench.push_context_compaction_status_message(
        session_id,
        SessionPromptCompactionTrigger::ManualSlash,
        "测试预算",
    );
    workbench.active_session_compactions.insert(
        session_id,
        ActiveSessionCompactionState {
            run_id: 43,
            handle,
            previous_phase: SessionRunPhase::Ready,
            trigger: SessionPromptCompactionTrigger::ManualSlash,
            status_message_id: Some(status_message_id.clone()),
            output_message_id: None,
        },
    );

    let status_message = workbench
        .session_messages
        .get(&session_id)
        .and_then(|messages| {
            messages
                .iter()
                .find(|message| message.record_kind == SessionMessageRecordKind::CompactionStatus)
        })
        .expect("compaction should append one visible running boundary");
    assert!(status_message.visible);
    assert_eq!(status_message.title, "上下文正在压缩");
    assert_eq!(status_message.body, "");
    assert!(!status_message.contributes_to_context());

    workbench.append_context_compaction_output_delta_for_active_run(
        session_id,
        SessionPromptCompactionTrigger::ManualSlash,
        "测试预算",
        "压缩后的摘要",
    );

    let output_message = workbench
        .session_messages
        .get(&session_id)
        .and_then(|messages| {
            messages.iter().find(|message| {
                message.id != status_message_id
                    && message.record_kind == SessionMessageRecordKind::CompactionStatus
            })
        })
        .expect("compaction should append one visible section body message after output");
    assert!(output_message.visible);
    assert_eq!(output_message.title, "上下文总结");
    assert_eq!(output_message.body, "压缩后的摘要");
    assert_eq!(
        output_message
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("context_compaction_section_id"))
            .and_then(|value| value.as_str()),
        Some(status_message_id.as_str())
    );
    assert_eq!(
        output_message
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("context_compaction_role"))
            .and_then(|value| value.as_str()),
        Some("body")
    );
    assert!(!output_message.contributes_to_context());
}

/// Verifies active prompt-compaction thinking still drives the floating thinking overlay.
/// 验证活动提示词压缩中的思考内容仍会驱动浮动思考面板。
#[test]
fn active_prompt_compaction_thinking_is_available_for_floating_overlay() {
    let (mut workbench, _task) = VulcanWorkbench::new();
    let session_id = SessionId::from_u128(71005);
    workbench.sessions.clear();
    workbench
        .sessions
        .push(test_session_summary(session_id, "压缩思考浮层测试"));
    let (_task, handle) = Task::perform(async {}, |_| Message::Noop).abortable();
    let status_message_id = workbench.push_context_compaction_status_message(
        session_id,
        SessionPromptCompactionTrigger::ManualSlash,
        "测试预算",
    );
    workbench.active_session_compactions.insert(
        session_id,
        ActiveSessionCompactionState {
            run_id: 44,
            handle,
            previous_phase: SessionRunPhase::Ready,
            trigger: SessionPromptCompactionTrigger::ManualSlash,
            status_message_id: Some(status_message_id.clone()),
            output_message_id: None,
        },
    );

    workbench.append_streaming_thinking_delta(session_id, "压缩前的思考内容");

    let thinking_message = workbench
        .streaming_thinking_message(session_id)
        .expect("active compaction should expose streaming thinking to the overlay");
    assert_eq!(thinking_message.body, "压缩前的思考内容");
    assert_eq!(
        thinking_message
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("context_compaction_section_id"))
            .and_then(|value| value.as_str()),
        Some(status_message_id.as_str())
    );
    assert_eq!(
        thinking_message
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("context_compaction_role"))
            .and_then(|value| value.as_str()),
        Some("body")
    );
}

/// Verifies completed prompt compaction turns the boundary row into a collapsed automatic-compression marker.
/// 验证提示词压缩完成后会将边界行转换为默认折叠的自动压缩标记。
#[test]
fn finish_prompt_compaction_collapses_boundary_row() {
    let (mut workbench, _task) = VulcanWorkbench::new();
    let session_id = SessionId::from_u128(71004);
    workbench.sessions.clear();
    workbench
        .sessions
        .push(test_session_summary(session_id, "压缩完成边界测试"));
    let status_message_id = workbench.push_context_compaction_status_message(
        session_id,
        SessionPromptCompactionTrigger::BudgetAuto,
        "测试预算",
    );
    let output_message_id = workbench.push_visible_session_record_message_with_metadata(
        session_id,
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::CompactionStatus,
        "上下文总结",
        String::new(),
        Some(json!({
            "context_compaction_status": true,
            "context_compaction_section_id": status_message_id,
            "context_compaction_role": "body",
        })),
        false,
    );
    workbench.append_context_compaction_output_delta(
        session_id,
        &output_message_id,
        "压缩后的摘要",
    );
    let execution = PromptCompactionExecution {
        compaction_result: CompactionSkillResult::from_model_summary(
            &CompactionPlan {
                source_messages: Vec::new(),
                source_until_message_id: None,
                source_until_turn_index: None,
                retained_tail_start_turn_index: None,
                compaction_id: String::from("compaction-test-finish"),
                created_at: String::from("1000"),
                policy: ContextCompactionPolicy::default(),
            },
            "压缩后的摘要",
        ),
        fallback_used: false,
        provider_request_count: 1,
    };

    let _task = workbench.finish_session_prompt_compaction(
        session_id,
        SessionPromptCompactionTrigger::BudgetAuto,
        String::from("测试预算"),
        SessionRunPhase::Ready,
        execution,
        Some(status_message_id.clone()),
    );

    let messages = workbench
        .session_messages
        .get(&session_id)
        .expect("completed compaction should keep visible section messages");
    let start_message = messages
        .iter()
        .find(|message| message.id == status_message_id)
        .expect("completed compaction should keep the visible start boundary row");
    assert_eq!(start_message.title, "上下文正在压缩");
    assert_eq!(
        start_message
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("context_compaction_state"))
            .and_then(|value| value.as_str()),
        Some("completed")
    );
    let end_message = messages
        .iter()
        .find(|message| {
            message
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("context_compaction_role"))
                .and_then(|value| value.as_str())
                == Some("end")
        })
        .expect("completed compaction should append the visible end boundary row");
    assert_eq!(end_message.title, "上下文已自动压缩");
    let collapsed_block_id =
        format!("session-{session_id}-agent-view-context-compaction-{status_message_id}");
    assert_eq!(
        workbench
            .session_agent_view_block_collapsed
            .get(&session_id)
            .and_then(|states| states.get(&collapsed_block_id))
            .copied(),
        Some(true)
    );
}

/// Verifies that the background legacy-maintenance queue includes visible and archived sessions once.
/// 验证后台旧数据维护队列会将可见与归档会话各纳入一次。
#[test]
fn background_legacy_maintenance_queue_deduplicates_sessions() {
    let first_session_id = SessionId::from_u128(101);
    let second_session_id = SessionId::from_u128(102);
    let visible_sessions = vec![test_session_summary(first_session_id, "可见会话")];
    let archived_sessions = vec![
        test_session_summary(first_session_id, "重复归档会话"),
        test_session_summary(second_session_id, "归档会话"),
    ];

    let queue = build_background_legacy_maintenance_queue(&visible_sessions, &archived_sessions);

    assert_eq!(
        queue.into_iter().collect::<Vec<_>>(),
        vec![first_session_id, second_session_id]
    );
}

/// Verifies that the background legacy-maintenance queue ignores nil draft identifiers.
/// 验证后台旧数据维护队列会忽略空的草稿标识。
#[test]
fn background_legacy_maintenance_queue_skips_nil_sessions() {
    let session_id = SessionId::from_u128(103);
    let visible_sessions = vec![
        test_session_summary(SessionId::nil(), "空会话"),
        test_session_summary(session_id, "正常会话"),
    ];

    let queue = build_background_legacy_maintenance_queue(&visible_sessions, &[]);

    assert_eq!(queue.into_iter().collect::<Vec<_>>(), vec![session_id]);
}

/// Verifies switching GUI locale only rebuilds transcript projections and never mutates canonical context state or persisted transcript rows.
/// 验证切换图形界面语言只会重建转录投影，不会改动规范上下文状态或已持久化转录行。
#[test]
fn locale_refresh_rebuilds_projection_without_mutating_context_state() {
    let (mut workbench, _task) = VulcanWorkbench::new();
    let session_id = SessionId::from_u128(7001);
    workbench.sessions.clear();
    workbench
        .sessions
        .push(test_session_summary(session_id, "Locale Test"));
    workbench.selected_session = session_id;

    let messages = vec![
        SessionMessageEntry {
            id: String::from("user-1"),
            kind: SessionMessageKind::User,
            record_kind: SessionMessageRecordKind::Transcript,
            visible: true,
            title: String::from("用户指令"),
            body: String::from("请继续检查当前输出。"),
            metadata: Some(json!({
                "canonical_user_text": "Please continue checking the current output."
            })),
            created_at: String::from("1"),
        },
        SessionMessageEntry {
            id: String::from("tool-1"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::ToolResultOutput,
            visible: true,
            title: String::from("工具返回"),
            body: String::from("## Tool Result\n\nRead completed."),
            metadata: Some(json!({
                "tool_name": "read_file",
                "tool_call_id": "call-1",
            })),
            created_at: String::from("2"),
        },
    ];
    let conversation_state =
        crate::conversation_protocol::conversation_state_from_session_messages(&messages);
    workbench.session_messages.insert(session_id, messages);
    workbench.session_runtime_state.insert(
        session_id,
        SessionRuntimeStateEntry {
            conversation_state,
            updated_at: String::from("1"),
            ..SessionRuntimeStateEntry::default()
        },
    );

    workbench.refresh_session_agent_view_buffers(session_id);
    let messages_before =
        message_window_fingerprint(workbench.session_messages.get(&session_id).unwrap());
    let conversation_before = serde_json::to_value(
        &workbench
            .session_runtime_state
            .get(&session_id)
            .unwrap()
            .conversation_state,
    )
    .expect("serialize canonical conversation before locale refresh");

    workbench
        .language_manager
        .set_locale(crate::AppLocale::EnUs);
    clear_selectable_transcript_render_cache_stash();
    workbench.rebuild_quick_guide_markdown_buffer();
    workbench.refresh_all_session_agent_view_localizations();

    let messages_after =
        message_window_fingerprint(workbench.session_messages.get(&session_id).unwrap());
    let conversation_after = serde_json::to_value(
        &workbench
            .session_runtime_state
            .get(&session_id)
            .unwrap()
            .conversation_state,
    )
    .expect("serialize canonical conversation after locale refresh");
    assert_eq!(messages_after, messages_before);
    assert_eq!(conversation_after, conversation_before);

    let rendered = workbench
        .rendered_session_agent_view_document(session_id)
        .expect("rebuilt rendered document after locale refresh");
    let rendered_titles = rendered
        .blocks
        .iter()
        .filter_map(|block| match block {
            AgentViewBlock::Directive(directive) => directive.attributes.get("title").cloned(),
            AgentViewBlock::Markdown(_) => None,
        })
        .collect::<Vec<_>>();
    assert!(rendered_titles.iter().any(|title| title == "User Prompt"));
    assert!(
        rendered_titles
            .iter()
            .any(|title| title == "Tool Call read_file Completed")
    );
}

/// Verifies Anthropic queue dispatch prefers delegated completion callbacks over user-authored follow-ups once background subagents have fully settled.
/// 验证在后台子代理全部收敛后，Anthropic 队列分发会优先选择子代理完成回流项，而不是优先处理用户跟进消息。
#[test]
fn pending_prompt_dispatch_index_prefers_delegate_completion_for_anthropic() {
    let queue = vec![
        test_pending_prompt_entry(PendingSessionPromptSource::UserInterrupt, "interrupt"),
        test_pending_prompt_entry(
            PendingSessionPromptSource::DelegateCompletion,
            "delegate-finished",
        ),
        test_pending_prompt_entry(PendingSessionPromptSource::UserFollowup, "followup"),
    ];

    assert_eq!(pending_prompt_dispatch_index(&queue, true), 1);
    assert_eq!(pending_prompt_dispatch_index(&queue, false), 0);
}

/// Verifies Anthropic sessions block pending-prompt dispatch while detached subagents from the previous tool round are still running, whereas other runtimes keep the original behavior.
/// 验证当上一轮工具回合的分离子代理仍在运行时，Anthropic 会话会阻止待发送提示词分发，而其他运行时保持原有行为。
#[test]
fn anthropic_pending_prompt_dispatch_blocks_only_for_running_subagents() {
    assert!(anthropic_pending_prompt_dispatch_is_blocked(true, true));
    assert!(!anthropic_pending_prompt_dispatch_is_blocked(true, false));
    assert!(!anthropic_pending_prompt_dispatch_is_blocked(false, true));
    assert!(!anthropic_pending_prompt_dispatch_is_blocked(false, false));
}

/// Verifies that transcript pruning removes the oldest messages first.
/// 验证转录裁剪会优先移除最旧的消息。
#[test]
fn prune_session_messages_to_limit_removes_oldest_entries() {
    let mut messages = (0..5)
        .map(|index| SessionMessageEntry {
            id: format!("message-{index}"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::Transcript,
            visible: true,
            title: format!("消息 {index}"),
            body: format!("正文 {index}"),
            metadata: None,
            created_at: index.to_string(),
        })
        .collect::<Vec<_>>();

    let removed = prune_session_messages_to_limit(&mut messages, 3);

    assert_eq!(removed, vec!["message-0", "message-1"]);
    assert_eq!(
        messages
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec!["message-2", "message-3", "message-4"]
    );
}

/// Verifies that transcript pruning is a no-op when the current window is already small enough.
/// 验证当前窗口足够小时转录裁剪不会产生任何修改。
#[test]
fn prune_session_messages_to_limit_keeps_small_windows_unchanged() {
    let mut messages = vec![SessionMessageEntry::new(
        SessionMessageKind::User,
        "用户指令",
        "继续",
    )];

    let removed = prune_session_messages_to_limit(&mut messages, 3);

    assert!(removed.is_empty());
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].title, "用户指令");
}

/// Verifies one tool call and its matching tool result merge into a single human-facing transcript card.
/// 验证一条工具调用与其匹配的工具结果会合并成面向人类的单张转录卡片。
#[test]
fn merge_tool_display_messages_collapses_matching_tool_rows() {
    let messages = vec![
        SessionMessageEntry {
            id: String::from("tool-call-1"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::ToolCallOutput,
            visible: true,
            title: String::from("调用 read_file 工具 进行中"),
            body: String::from("## 调用参数\n\n```json\n{\"path\":\"src/main.rs\"}\n```"),
            metadata: Some(json!({
                "tool_call_id": "call-1",
                "tool_name": "read_file",
                "arguments": {
                    "path": "src/main.rs",
                },
            })),
            created_at: String::from("1"),
        },
        SessionMessageEntry {
            id: String::from("tool-result-1"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::ToolResultOutput,
            visible: true,
            title: String::from("工具返回 read_file"),
            body: String::from("## 工具返回\n\n成功读取文件。"),
            metadata: Some(json!({
                "tool_call_id": "call-1",
                "tool_result_id": "result-1",
                "host_result": {
                    "kind": "change_set",
                    "payload": {
                        "path": "src/main.rs",
                    },
                },
            })),
            created_at: String::from("2"),
        },
    ];

    let merged = merge_tool_display_messages(messages);
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].id, "tool-call-1");
    assert_eq!(
        merged[0].record_kind,
        SessionMessageRecordKind::ToolCallOutput
    );
    assert_eq!(merged[0].title, "调用 read_file 工具 已完成");
    assert!(merged[0].body.contains("## 调用参数"));
    assert!(merged[0].body.contains("## 工具返回"));
    assert_eq!(
        merged[0]
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("tool_result_id"))
            .and_then(|value| value.as_str()),
        Some("result-1")
    );
    assert_eq!(
        merged[0]
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("tool_display_state"))
            .and_then(|value| value.as_str()),
        Some("completed")
    );
}

/// Verifies an in-flight tool call still stays visible on its own when no matching result has arrived yet.
/// 验证当尚未收到匹配结果时，进行中的工具调用仍会单独保持可见。
#[test]
fn merge_tool_display_messages_keeps_inflight_tool_call_visible() {
    let messages = vec![SessionMessageEntry {
        id: String::from("tool-call-1"),
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::ToolCallOutput,
        visible: true,
        title: String::from("调用 read_file 工具 进行中"),
        body: String::from("## 调用参数\n\n```json\n{\"path\":\"src/main.rs\"}\n```"),
        metadata: Some(json!({
            "tool_call_id": "call-1",
            "tool_name": "read_file",
        })),
        created_at: String::from("1"),
    }];

    let merged = merge_tool_display_messages(messages);
    assert_eq!(merged.len(), 1);
    assert_eq!(
        merged[0].record_kind,
        SessionMessageRecordKind::ToolCallOutput
    );
    assert_eq!(merged[0].title, "调用 read_file 工具 进行中");
    assert_eq!(
        merged[0]
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("tool_display_state"))
            .and_then(|value| value.as_str()),
        Some("running")
    );
}

/// Verifies syncing visible transcript rows from the canonical conversation state preserves already hydrated older history rows that are not part of the live runtime tail.
/// 验证从规范 conversation state 同步可见转录行时，会保留已水合但不属于当前运行时尾部的更早历史行。
#[test]
fn merge_projected_session_messages_preserves_hydrated_history_prefix() {
    let existing_messages = vec![
        SessionMessageEntry {
            id: String::from("older-tool-1"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::ToolCallOutput,
            visible: true,
            title: String::from("调用历史工具"),
            body: String::new(),
            metadata: Some(json!({
                "tool_call_id": "older-call-1",
                "tool_display_state": "completed",
            })),
            created_at: String::from("1"),
        },
        SessionMessageEntry {
            id: String::from("assistant-tail-1"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::AssistantOutput,
            visible: true,
            title: String::from("输出"),
            body: String::from("旧尾部"),
            metadata: None,
            created_at: String::from("2"),
        },
    ];
    let projected_messages = vec![
        SessionMessageEntry {
            id: String::from("assistant-tail-1"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::AssistantOutput,
            visible: true,
            title: String::from("输出"),
            body: String::from("新尾部"),
            metadata: None,
            created_at: String::from("2"),
        },
        SessionMessageEntry {
            id: String::from("assistant-tail-2"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::AssistantOutput,
            visible: true,
            title: String::from("输出"),
            body: String::from("最新输出"),
            metadata: None,
            created_at: String::from("3"),
        },
    ];

    let merged = merge_projected_session_messages_into_existing_window(
        &existing_messages,
        projected_messages,
    );

    assert_eq!(
        merged
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec!["older-tool-1", "assistant-tail-1", "assistant-tail-2"]
    );
    assert_eq!(merged[1].body, "新尾部");
    assert_eq!(merged[2].body, "最新输出");
}

/// Verifies that main-transcript tool projections drop heavy bodies and retain only card metadata.
/// 验证主转录工具投影会移除重型正文，并仅保留卡片所需元数据。
#[test]
fn compact_tool_detail_projection_drops_hidden_payloads() {
    let message = SessionMessageEntry {
        id: String::from("tool-1"),
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::ToolResultOutput,
        visible: true,
        title: String::from("调用 read_file 工具"),
        body: "x".repeat(200_000),
        metadata: Some(json!({
            "tool_call_id": "call-1",
            "tool_display_state": "completed",
            "host_result": {
                "kind": "raw_payload",
                "payload": "y".repeat(200_000)
            },
            "payload": {
                "task_id": "task-a",
                "prompt": "z".repeat(1000),
                "conversation_state": {"large": "z".repeat(200_000)}
            }
        })),
        created_at: String::from("1"),
    };

    let projected = compact_tool_detail_message_for_projection(&message);

    assert!(projected.body.is_empty());
    let metadata = projected.metadata.expect("metadata should stay present");
    assert_eq!(
        metadata.get("tool_display_state").and_then(Value::as_str),
        Some("completed")
    );
    assert!(metadata.get("host_result").is_none());
    assert!(
        metadata
            .get("payload")
            .and_then(|payload| payload.get("conversation_state"))
            .is_none()
    );
}

/// Verifies transcript-window merge reuses one projected turn-stats row when only the runtime id drifted after reload, instead of keeping the stale row and appending a duplicate.
/// 验证当重载后仅有运行时 id 漂移时，转录窗口合并会复用投影出来的同一条 turn-stats 行，而不是保留旧行再追加重复项。
#[test]
fn merge_projected_messages_dedupes_semantically_identical_turn_stats() {
    let stats_metadata = Some(json!({
        "turn_stats": {
            "status": "completed",
            "provider_call_count": 1,
            "tool_call_count": 0,
            "input_tokens": 120,
            "output_tokens": 24,
            "reasoning_tokens": 0,
            "total_tokens": 144,
            "cached_input_tokens": 32,
            "duration_ms": 900
        }
    }));
    let existing_messages = vec![
        SessionMessageEntry {
            id: String::from("user-1"),
            kind: SessionMessageKind::User,
            record_kind: SessionMessageRecordKind::Transcript,
            visible: true,
            title: String::from("用户输入"),
            body: String::from("随便输出点内容"),
            metadata: None,
            created_at: String::from("1"),
        },
        SessionMessageEntry {
            id: String::from("legacy-turn-stats-1"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::TurnStats,
            visible: true,
            title: String::from("Turn Stats · Completed"),
            body: String::from("◔ 1 次调用 · 0 个工具 · 144 tokens"),
            metadata: stats_metadata.clone(),
            created_at: String::from("2"),
        },
    ];
    let projected_messages = vec![
        SessionMessageEntry {
            id: String::from("user-1"),
            kind: SessionMessageKind::User,
            record_kind: SessionMessageRecordKind::Transcript,
            visible: true,
            title: String::from("用户输入"),
            body: String::from("随便输出点内容"),
            metadata: None,
            created_at: String::from("1"),
        },
        SessionMessageEntry {
            id: String::from("projected-turn-stats-1"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::TurnStats,
            visible: true,
            title: String::from("Turn Stats · Completed"),
            body: String::from("◔ 1 次调用 · 0 个工具 · 144 tokens"),
            metadata: stats_metadata,
            created_at: String::from("2"),
        },
        SessionMessageEntry {
            id: String::from("user-2"),
            kind: SessionMessageKind::User,
            record_kind: SessionMessageRecordKind::Transcript,
            visible: true,
            title: String::from("用户输入"),
            body: String::from("再来一次"),
            metadata: None,
            created_at: String::from("3"),
        },
    ];

    let merged = merge_projected_session_messages_into_existing_window(
        &existing_messages,
        projected_messages,
    );

    assert_eq!(
        merged
            .iter()
            .filter(|message| message.record_kind == SessionMessageRecordKind::TurnStats)
            .count(),
        1
    );
    assert_eq!(
        merged
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec!["user-1", "projected-turn-stats-1", "user-2"]
    );
}

/// Verifies turn-stats rows still collapse to one visible row when the recalculated payload changes but the completed turn anchor stays the same.
/// 验证当重算后的载荷发生变化但已完成轮次锚点保持一致时，turn-stats 行仍会收敛为一条可见行。
#[test]
fn merge_projected_messages_dedupes_turn_stats_by_turn_anchor_when_payload_drifts() {
    let existing_messages = vec![
        SessionMessageEntry {
            id: String::from("user-1"),
            kind: SessionMessageKind::User,
            record_kind: SessionMessageRecordKind::Transcript,
            visible: true,
            title: String::from("用户输入"),
            body: String::from("随便输出点内容"),
            metadata: Some(json!({
                "canonical_user_text": "随便输出点内容"
            })),
            created_at: String::from("1"),
        },
        SessionMessageEntry {
            id: String::from("legacy-turn-stats-1"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::TurnStats,
            visible: true,
            title: String::from("Turn Stats · Completed"),
            body: String::from("◔ 1 次调用 · 0 个工具 · 144 tokens"),
            metadata: Some(json!({
                "turn_stats": {
                    "status": "completed",
                    "provider_call_count": 1,
                    "tool_call_count": 0,
                    "input_tokens": 120,
                    "output_tokens": 24,
                    "reasoning_tokens": 0,
                    "total_tokens": 144,
                    "cached_input_tokens": 32,
                    "duration_ms": 900
                }
            })),
            created_at: String::from("2"),
        },
    ];
    let projected_messages = vec![
        SessionMessageEntry {
            id: String::from("user-1"),
            kind: SessionMessageKind::User,
            record_kind: SessionMessageRecordKind::Transcript,
            visible: true,
            title: String::from("用户输入"),
            body: String::from("随便输出点内容"),
            metadata: Some(json!({
                "canonical_user_text": "随便输出点内容"
            })),
            created_at: String::from("1"),
        },
        SessionMessageEntry {
            id: String::from("projected-turn-stats-1"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::TurnStats,
            visible: true,
            title: String::from("Turn Stats · Completed"),
            body: String::from("◔ 1 次调用 · 0 个工具 · 147 tokens"),
            metadata: Some(json!({
                "turn_stats": {
                    "status": "completed",
                    "provider_call_count": 1,
                    "tool_call_count": 0,
                    "input_tokens": 121,
                    "output_tokens": 26,
                    "reasoning_tokens": 0,
                    "total_tokens": 147,
                    "cached_input_tokens": 35,
                    "duration_ms": 1000
                },
                "turn_closing_record_id": "assistant-1"
            })),
            created_at: String::from("2"),
        },
        SessionMessageEntry {
            id: String::from("user-2"),
            kind: SessionMessageKind::User,
            record_kind: SessionMessageRecordKind::Transcript,
            visible: true,
            title: String::from("用户输入"),
            body: String::from("再来一次"),
            metadata: None,
            created_at: String::from("3"),
        },
    ];

    let merged = merge_projected_session_messages_into_existing_window(
        &existing_messages,
        projected_messages,
    );

    assert_eq!(
        merged
            .iter()
            .filter(|message| message.record_kind == SessionMessageRecordKind::TurnStats)
            .count(),
        1
    );
    assert_eq!(
        merged
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec!["user-1", "projected-turn-stats-1", "user-2"]
    );
    assert_eq!(merged[1].body, "◔ 1 次调用 · 0 个工具 · 147 tokens");
}

/// Verifies reprojection from a full canonical history only appends the genuinely new tail, instead of backfilling older unseen history rows into the already hydrated window.
/// 验证从完整规范历史重新投影时，只会追加真正的新尾部，而不会把旧的未加载历史行反向灌回已经水合的窗口。
#[test]
fn merge_projected_messages_keeps_hydrated_window_boundary_and_appends_new_tail_only() {
    let existing_messages = vec![
        SessionMessageEntry {
            id: String::from("user-2"),
            kind: SessionMessageKind::User,
            record_kind: SessionMessageRecordKind::Transcript,
            visible: true,
            title: String::from("用户输入"),
            body: String::from("第二轮"),
            metadata: None,
            created_at: String::from("3"),
        },
        SessionMessageEntry {
            id: String::from("turn-stats-2"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::TurnStats,
            visible: true,
            title: String::from("Turn Stats · Completed"),
            body: String::from("◔ 1 次调用 · 0 个工具 · 144 tokens"),
            metadata: Some(json!({
                "turn_stats": {
                    "status": "completed",
                    "provider_call_count": 1,
                    "tool_call_count": 0,
                    "input_tokens": 120,
                    "output_tokens": 24,
                    "reasoning_tokens": 0,
                    "total_tokens": 144
                }
            })),
            created_at: String::from("4"),
        },
    ];
    let projected_messages = vec![
        SessionMessageEntry {
            id: String::from("user-1"),
            kind: SessionMessageKind::User,
            record_kind: SessionMessageRecordKind::Transcript,
            visible: true,
            title: String::from("用户输入"),
            body: String::from("第一轮"),
            metadata: None,
            created_at: String::from("1"),
        },
        SessionMessageEntry {
            id: String::from("turn-stats-1"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::TurnStats,
            visible: true,
            title: String::from("Turn Stats · Completed"),
            body: String::from("◔ 1 次调用 · 0 个工具 · 188 tokens"),
            metadata: Some(json!({
                "turn_stats": {
                    "status": "completed",
                    "provider_call_count": 1,
                    "tool_call_count": 0,
                    "input_tokens": 160,
                    "output_tokens": 28,
                    "reasoning_tokens": 0,
                    "total_tokens": 188
                }
            })),
            created_at: String::from("2"),
        },
        existing_messages[0].clone(),
        existing_messages[1].clone(),
        SessionMessageEntry {
            id: String::from("user-3"),
            kind: SessionMessageKind::User,
            record_kind: SessionMessageRecordKind::Transcript,
            visible: true,
            title: String::from("用户输入"),
            body: String::from("第三轮"),
            metadata: None,
            created_at: String::from("5"),
        },
    ];

    let merged = merge_projected_session_messages_into_existing_window(
        &existing_messages,
        projected_messages,
    );

    assert_eq!(
        merged
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec!["user-2", "turn-stats-2", "user-3"]
    );
}

/// Verifies session record ids advance with canonical record ordinals so hidden and visible rows appended in the same millisecond cannot collide anymore.
/// 验证会话记录标识会随规范记录序号推进，确保同一毫秒内连续追加的隐藏行与可见行不再发生碰撞。
#[test]
fn session_record_message_id_uses_canonical_record_ordinal() {
    let session_id = SessionId::from_u128(0x30cf28c9);
    let created_at = "1779720972348";

    let hidden_summary_id = build_session_record_message_id(session_id, created_at, 10);
    let visible_turn_stats_id = build_session_record_message_id(session_id, created_at, 11);

    assert_ne!(hidden_summary_id, visible_turn_stats_id);
    assert_eq!(
        hidden_summary_id,
        format!("session-{session_id}-message-{created_at}-10")
    );
    assert_eq!(
        visible_turn_stats_id,
        format!("session-{session_id}-message-{created_at}-11")
    );
}

/// Verifies compact tool projections preserve the command-session payload subset required by the dedicated command-session transcript cards after canonical records are rebuilt.
/// 验证紧凑工具投影在规范记录重建后，仍会保留专属命令会话转录卡片所需的 command-session 载荷子集。
#[test]
fn compact_tool_detail_projection_preserves_command_session_payload() {
    let message = SessionMessageEntry {
        id: String::from("tool-command-session-1"),
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::ToolResultOutput,
        visible: true,
        title: String::from("调用 shell_session 工具"),
        body: String::from("命令会话读取完成。"),
        metadata: Some(json!({
            "tool_call_id": "call-command-session-1",
            "tool_name": "shell_session",
            "kind": "command_session",
            "payload": {
                "action": "read",
                "owner_session_id": "00000000-0000-0000-0000-000000000010",
                "command_session_id": "00000000-0000-0000-0000-000000000011",
                "command": "cargo test -p vulcan-code-gui",
                "cwd": r"D:\openvulcan\vulcan-code",
                "shell": "powershell",
                "status": "running",
                "last_seq": 9,
                "stdout_bytes": 512,
                "stderr_bytes": 12,
                "returned_chunk_count": 2,
                "has_more": true,
                "output_overflowed": false
            },
            "host_result": {
                "kind": "command_session",
                "payload": {
                    "action": "read",
                    "owner_session_id": "00000000-0000-0000-0000-000000000010",
                    "command_session_id": "00000000-0000-0000-0000-000000000011",
                    "command": "cargo test -p vulcan-code-gui",
                    "cwd": r"D:\openvulcan\vulcan-code",
                    "shell": "powershell",
                    "status": "running",
                    "last_seq": 9,
                    "stdout_bytes": 512,
                    "stderr_bytes": 12,
                    "returned_chunk_count": 2,
                    "has_more": true,
                    "output_overflowed": false
                }
            }
        })),
        created_at: String::from("1"),
    };

    let projected = compact_tool_detail_message_for_projection(&message);
    let metadata = projected.metadata.expect("metadata should stay present");

    assert!(projected.body.is_empty());
    assert_eq!(
        metadata.get("kind").and_then(Value::as_str),
        Some("command_session")
    );
    assert_eq!(
        metadata
            .get("payload")
            .and_then(|payload| payload.get("command_session_id"))
            .and_then(Value::as_str),
        Some("00000000-0000-0000-0000-000000000011")
    );
    assert_eq!(
        metadata
            .get("host_result")
            .and_then(|host_result| host_result.get("payload"))
            .and_then(|payload| payload.get("action"))
            .and_then(Value::as_str),
        Some("read")
    );
}

/// Verifies compact tool projections preserve the command-run payload subset required by the dedicated command execution transcript cards.
/// 验证紧凑工具投影会保留专属命令执行转录卡片所需的 command-run 载荷子集。
#[test]
fn compact_tool_detail_projection_preserves_command_run_payload() {
    let message = SessionMessageEntry {
        id: String::from("tool-command-run-1"),
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::ToolResultOutput,
        visible: true,
        title: String::from("调用 shell_command 工具"),
        body: String::from("命令执行完成。"),
        metadata: Some(json!({
            "tool_call_id": "call-command-run-1",
            "tool_name": "shell_command",
            "kind": "command_run",
            "payload": {
                "command": "cargo check -p vulcan-code-gui",
                "cwd": r"D:\openvulcan\vulcan-code",
                "shell": "powershell",
                "status": "completed",
                "exit_code": 0,
                "duration_ms": 1024,
                "stdout_bytes": 4096,
                "stderr_bytes": 0,
                "combined_output_overflowed": false
            },
            "host_result": {
                "kind": "command_run",
                "payload": {
                    "command": "cargo check -p vulcan-code-gui",
                    "cwd": r"D:\openvulcan\vulcan-code",
                    "shell": "powershell",
                    "status": "completed",
                    "exit_code": 0,
                    "duration_ms": 1024,
                    "stdout_bytes": 4096,
                    "stderr_bytes": 0,
                    "combined_output_overflowed": false
                }
            }
        })),
        created_at: String::from("1"),
    };

    let projected = compact_tool_detail_message_for_projection(&message);
    let metadata = projected.metadata.expect("metadata should stay present");

    assert!(projected.body.is_empty());
    assert_eq!(
        metadata.get("kind").and_then(Value::as_str),
        Some("command_run")
    );
    assert_eq!(
        metadata
            .get("payload")
            .and_then(|payload| payload.get("command"))
            .and_then(Value::as_str),
        Some("cargo check -p vulcan-code-gui")
    );
    assert_eq!(
        metadata
            .get("host_result")
            .and_then(|host_result| host_result.get("payload"))
            .and_then(|payload| payload.get("duration_ms"))
            .and_then(Value::as_u64),
        Some(1024)
    );
}

/// Verifies compact tool projections preserve one compact `change_set` payload so inline tool cards can keep showing file-change statistics after the heavy raw payload is stripped.
/// 验证紧凑工具投影会保留一份紧凑 `change_set` 载荷，从而让内联工具卡在剥离重型原始载荷后仍能展示文件变更统计。
#[test]
fn compact_tool_detail_projection_preserves_change_set_payload() {
    let message = SessionMessageEntry {
        id: String::from("tool-change-set-1"),
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::ToolResultOutput,
        visible: true,
        title: String::from("工具返回"),
        body: String::from("Applied file edit."),
        metadata: Some(json!({
            "tool_call_id": "call-change-set-1",
            "tool_name": "write_file",
            "kind": "change_set",
            "payload": {
                "mode": "preview",
                "files": [{
                    "change": "create",
                    "path": r"D:\openvulcan\vulcan-code\src\main.rs",
                    "content": "fn main() {}\n"
                }]
            },
            "host_result": {
                "kind": "change_set",
                "payload": {
                    "mode": "preview",
                    "files": [{
                        "change": "create",
                        "path": r"D:\openvulcan\vulcan-code\src\main.rs",
                        "content": "fn main() {}\n"
                    }]
                }
            }
        })),
        created_at: String::from("1"),
    };

    let projected = compact_tool_detail_message_for_projection(&message);
    let metadata = projected.metadata.expect("metadata should stay present");

    assert!(projected.body.is_empty());
    assert_eq!(
        metadata.get("kind").and_then(Value::as_str),
        Some("change_set")
    );
    assert_eq!(
        metadata
            .get("payload")
            .and_then(|payload| payload.get("stats"))
            .and_then(|stats| stats.get("file_count"))
            .and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        metadata
            .get("host_result")
            .and_then(|host_result| host_result.get("payload"))
            .and_then(|payload| payload.get("verification"))
            .and_then(|verification| verification.get("status"))
            .and_then(Value::as_str),
        Some("skipped")
    );
}

/// Verifies GUI-generated command-session stop metadata keeps the same nested and flattened host-result structure as provider-originated tool results.
/// 验证 GUI 生成的命令会话停止元数据会保留与 Provider 原生工具结果相同的嵌套和扁平宿主结果结构。
#[test]
fn command_session_stop_result_metadata_keeps_runtime_shape() {
    let snapshot = CommandSessionSnapshot {
        command_session_id: SessionId::parse_str("00000000-0000-0000-0000-000000000321")
            .expect("parse command session id"),
        owner_session_id: SessionId::parse_str("00000000-0000-0000-0000-000000000123")
            .expect("parse owner session id"),
        command: String::from("cargo test -p vulcan-code-gui"),
        shell_label: String::from("powershell"),
        launcher_shell_label: String::from("powershell"),
        cwd: PathBuf::from("D:\\openvulcan\\vulcan-code"),
        status: CommandSessionStatus::Stopped,
        exit_code: Some(1),
        failure_reason: Some(String::from("stopped by user")),
        stdout_bytes: 2048,
        stderr_bytes: 16,
        last_seq: 27,
        pending_stdin_line: false,
    };

    let metadata = build_command_session_stop_result_metadata("call-command-session-1", &snapshot)
        .expect("metadata should exist");

    assert_eq!(
        metadata.get("tool_call_id").and_then(Value::as_str),
        Some("call-command-session-1")
    );
    assert_eq!(
        metadata.get("tool_name").and_then(Value::as_str),
        Some("shell_session")
    );
    assert_eq!(
        metadata.get("kind").and_then(Value::as_str),
        Some("command_session")
    );
    assert_eq!(
        metadata
            .get("payload")
            .and_then(|payload| payload.get("action"))
            .and_then(Value::as_str),
        Some("stop")
    );
    assert_eq!(
        metadata
            .get("host_result")
            .and_then(|host_result| host_result.get("payload"))
            .and_then(|payload| payload.get("command_session_id"))
            .and_then(Value::as_str),
        Some("00000000-0000-0000-0000-000000000321")
    );
}

/// Verifies turn statistics aggregate provider usage from canonical assistant messages even when the visible transcript metadata does not mirror `provider_usage`.
/// 验证即使可见转录元数据没有镜像 `provider_usage`，按轮次统计仍会从规范助手消息中聚合 Provider 用量。
#[test]
fn turn_stats_reads_usage_from_canonical_messages() {
    let conversation_state = crate::conversation_protocol::SessionConversationState {
        messages: vec![
            crate::conversation_protocol::SessionConversationMessage {
                id: String::from("user-1"),
                payload: crate::conversation_protocol::SessionConversationPayload::UserText {
                    text: String::from("请分析一下"),
                    attachments: Vec::new(),
                },
            },
            crate::conversation_protocol::SessionConversationMessage {
                id: String::from("assistant-1"),
                payload: crate::conversation_protocol::SessionConversationPayload::Assistant {
                    text: String::from("先调用工具"),
                    reasoning: String::new(),
                    provider_message_id: Some(String::from("resp-1")),
                    provider_message_state: None,
                    usage: Some(vulcan_core::types::ProviderUsageSnapshot {
                        input_tokens: Some(120),
                        output_tokens: Some(30),
                        reasoning_tokens: Some(10),
                        total_tokens: Some(160),
                        cached_input_tokens: Some(64),
                        cache_creation_input_tokens: None,
                    }),
                    tool_calls: vec![crate::conversation_protocol::SessionConversationToolCall {
                        id: String::from("tool-call-1"),
                        tool_call_id: Some(String::from("call-1")),
                        tool_name: String::from("read_file"),
                        arguments: json!({"path": "README.md"}),
                    }],
                },
            },
        ],
        records: vec![
            crate::conversation_protocol::SessionConversationRecord {
                id: String::from("user-1"),
                kind: SessionMessageKind::User,
                record_kind: SessionMessageRecordKind::Transcript,
                visible: true,
                title: String::from("用户"),
                body: String::from("请分析一下"),
                metadata: None,
                created_at: String::from("1000"),
            },
            crate::conversation_protocol::SessionConversationRecord {
                id: String::from("assistant-1"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::AssistantOutput,
                visible: true,
                title: String::from("输出"),
                body: String::from("先调用工具"),
                metadata: None,
                created_at: String::from("2000"),
            },
            crate::conversation_protocol::SessionConversationRecord {
                id: String::from("summary-1"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::SummaryOutput,
                visible: false,
                title: String::from("Summary"),
                body: String::from("done"),
                metadata: None,
                created_at: String::from("3000"),
            },
        ],
        continuation: Default::default(),
    };

    let stats = build_session_turn_stats_from_state(
        &conversation_state,
        "summary-1",
        SessionTurnStatsStatus::Completed,
        None,
    )
    .expect("turn stats should build");

    assert_eq!(stats.provider_call_count, 1);
    assert_eq!(stats.tool_call_count, 1);
    assert_eq!(stats.total_tokens, 160);
    assert_eq!(stats.cached_input_tokens, Some(64));
}

/// Verifies turn statistics still aggregate provider usage and tool counts when transcript record ids differ from canonical message ids.
/// 验证当转录记录标识与规范消息标识不一致时，轮次统计仍能正确聚合 Provider 用量与工具调用数量。
#[test]
fn turn_stats_do_not_require_record_message_id_alignment() {
    let conversation_state = crate::conversation_protocol::SessionConversationState {
        messages: vec![
            crate::conversation_protocol::SessionConversationMessage {
                id: String::from("protocol-user-1"),
                payload: crate::conversation_protocol::SessionConversationPayload::UserText {
                    text: String::from("随便调用两个工具"),
                    attachments: Vec::new(),
                },
            },
            crate::conversation_protocol::SessionConversationMessage {
                id: String::from("provider-assistant-1"),
                payload: crate::conversation_protocol::SessionConversationPayload::Assistant {
                    text: String::new(),
                    reasoning: String::from("先调用两个工具再总结。"),
                    provider_message_id: Some(String::from("resp-1")),
                    provider_message_state: None,
                    usage: Some(vulcan_core::types::ProviderUsageSnapshot {
                        input_tokens: Some(120),
                        output_tokens: Some(48),
                        reasoning_tokens: Some(16),
                        total_tokens: Some(184),
                        cached_input_tokens: Some(32),
                        cache_creation_input_tokens: None,
                    }),
                    tool_calls: vec![
                        crate::conversation_protocol::SessionConversationToolCall {
                            id: String::from("tool-call-1"),
                            tool_call_id: Some(String::from("call-1")),
                            tool_name: String::from("vulcan_file_list"),
                            arguments: json!({"path": "."}),
                        },
                        crate::conversation_protocol::SessionConversationToolCall {
                            id: String::from("tool-call-2"),
                            tool_call_id: Some(String::from("call-2")),
                            tool_name: String::from("vulcan_file_read"),
                            arguments: json!({"file": "README.md"}),
                        },
                    ],
                },
            },
            crate::conversation_protocol::SessionConversationMessage {
                id: String::from("tool-result-1"),
                payload: crate::conversation_protocol::SessionConversationPayload::ToolResult {
                    tool_result_id: String::from("tool-result-call-1"),
                    tool_call_id: Some(String::from("call-1")),
                    text: String::from("列出文件完成"),
                },
            },
            crate::conversation_protocol::SessionConversationMessage {
                id: String::from("tool-result-2"),
                payload: crate::conversation_protocol::SessionConversationPayload::ToolResult {
                    tool_result_id: String::from("tool-result-call-2"),
                    tool_call_id: Some(String::from("call-2")),
                    text: String::from("读取文件完成"),
                },
            },
        ],
        records: vec![
            crate::conversation_protocol::SessionConversationRecord {
                id: String::from("record-user-1"),
                kind: SessionMessageKind::User,
                record_kind: SessionMessageRecordKind::Transcript,
                visible: true,
                title: String::from("用户"),
                body: String::from("随便调用两个工具"),
                metadata: None,
                created_at: String::from("1000"),
            },
            crate::conversation_protocol::SessionConversationRecord {
                id: String::from("record-tool-call-1"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::ToolCallOutput,
                visible: true,
                title: String::from("调用 vulcan_file_list 工具"),
                body: String::from("{}"),
                metadata: Some(json!({
                    "tool_call_id": "call-1",
                    "tool_name": "vulcan_file_list"
                })),
                created_at: String::from("1100"),
            },
            crate::conversation_protocol::SessionConversationRecord {
                id: String::from("record-tool-result-1"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::ToolResultOutput,
                visible: true,
                title: String::from("工具返回 vulcan_file_list"),
                body: String::from("列出文件完成"),
                metadata: Some(json!({
                    "tool_call_id": "call-1",
                    "kind": "file_list"
                })),
                created_at: String::from("1200"),
            },
            crate::conversation_protocol::SessionConversationRecord {
                id: String::from("record-tool-call-2"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::ToolCallOutput,
                visible: true,
                title: String::from("调用 vulcan_file_read 工具"),
                body: String::from("{}"),
                metadata: Some(json!({
                    "tool_call_id": "call-2",
                    "tool_name": "vulcan_file_read"
                })),
                created_at: String::from("1300"),
            },
            crate::conversation_protocol::SessionConversationRecord {
                id: String::from("record-tool-result-2"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::ToolResultOutput,
                visible: true,
                title: String::from("工具返回 vulcan_file_read"),
                body: String::from("读取文件完成"),
                metadata: Some(json!({
                    "tool_call_id": "call-2",
                    "kind": "file_read"
                })),
                created_at: String::from("1400"),
            },
            crate::conversation_protocol::SessionConversationRecord {
                id: String::from("summary-1"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::SummaryOutput,
                visible: false,
                title: String::from("Summary"),
                body: String::from("done"),
                metadata: None,
                created_at: String::from("1500"),
            },
        ],
        continuation: Default::default(),
    };

    let stats = build_session_turn_stats_from_state(
        &conversation_state,
        "summary-1",
        SessionTurnStatsStatus::Completed,
        None,
    )
    .expect("turn stats should build");

    assert_eq!(stats.provider_call_count, 1);
    assert_eq!(stats.tool_call_count, 2);
    assert_eq!(stats.total_tokens, 184);
    assert_eq!(stats.cached_input_tokens, Some(32));
}

/// Verifies stopped turns still report completed safe-checkpoint tool calls and usage.
/// 验证已停止轮次仍会统计已完成安全检查点中的工具调用与用量。
#[test]
fn stopped_turn_stats_preserve_safe_checkpoint_tool_usage() {
    let mut conversation_state = crate::conversation_protocol::SessionConversationState {
        messages: vec![
            crate::conversation_protocol::SessionConversationMessage {
                id: String::from("protocol-user-1"),
                payload: crate::conversation_protocol::SessionConversationPayload::UserText {
                    text: String::from("随便调用两个工具"),
                    attachments: Vec::new(),
                },
            },
            crate::conversation_protocol::SessionConversationMessage {
                id: String::from("provider-assistant-1"),
                payload: crate::conversation_protocol::SessionConversationPayload::Assistant {
                    text: String::new(),
                    reasoning: String::from("准备调用工具。"),
                    provider_message_id: Some(String::from("resp-1")),
                    provider_message_state: None,
                    usage: Some(vulcan_core::types::ProviderUsageSnapshot {
                        input_tokens: Some(100),
                        output_tokens: Some(20),
                        reasoning_tokens: Some(5),
                        total_tokens: Some(125),
                        cached_input_tokens: None,
                        cache_creation_input_tokens: None,
                    }),
                    tool_calls: vec![
                        crate::conversation_protocol::SessionConversationToolCall {
                            id: String::from("tool-call-1"),
                            tool_call_id: Some(String::from("call-1")),
                            tool_name: String::from("web_search"),
                            arguments: json!({"query": "test"}),
                        },
                        crate::conversation_protocol::SessionConversationToolCall {
                            id: String::from("tool-call-2"),
                            tool_call_id: Some(String::from("call-2")),
                            tool_name: String::from("vulcan_file_list"),
                            arguments: json!({"path": "."}),
                        },
                    ],
                },
            },
            crate::conversation_protocol::SessionConversationMessage {
                id: String::from("tool-result-1"),
                payload: crate::conversation_protocol::SessionConversationPayload::ToolResult {
                    tool_result_id: String::from("tool-result-call-1"),
                    tool_call_id: Some(String::from("call-1")),
                    text: String::from("搜索完成"),
                },
            },
            crate::conversation_protocol::SessionConversationMessage {
                id: String::from("tool-result-2"),
                payload: crate::conversation_protocol::SessionConversationPayload::ToolResult {
                    tool_result_id: String::from("tool-result-call-2"),
                    tool_call_id: Some(String::from("call-2")),
                    text: String::from("列出文件完成"),
                },
            },
        ],
        records: vec![
            crate::conversation_protocol::SessionConversationRecord {
                id: String::from("record-user-1"),
                kind: SessionMessageKind::User,
                record_kind: SessionMessageRecordKind::Transcript,
                visible: true,
                title: String::from("用户"),
                body: String::from("随便调用两个工具"),
                metadata: None,
                created_at: String::from("1000"),
            },
            crate::conversation_protocol::SessionConversationRecord {
                id: String::from("record-tool-call-1"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::ToolCallOutput,
                visible: true,
                title: String::from("调用 web_search 工具"),
                body: String::from("{}"),
                metadata: Some(json!({"tool_call_id": "call-1"})),
                created_at: String::from("1100"),
            },
            crate::conversation_protocol::SessionConversationRecord {
                id: String::from("record-tool-result-1"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::ToolResultOutput,
                visible: true,
                title: String::from("工具返回 web_search"),
                body: String::from("搜索完成"),
                metadata: Some(json!({"tool_call_id": "call-1"})),
                created_at: String::from("1200"),
            },
            crate::conversation_protocol::SessionConversationRecord {
                id: String::from("record-tool-call-2"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::ToolCallOutput,
                visible: true,
                title: String::from("调用 vulcan_file_list 工具"),
                body: String::from("{}"),
                metadata: Some(json!({"tool_call_id": "call-2"})),
                created_at: String::from("1300"),
            },
            crate::conversation_protocol::SessionConversationRecord {
                id: String::from("record-tool-result-2"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::ToolResultOutput,
                visible: true,
                title: String::from("工具返回 vulcan_file_list"),
                body: String::from("列出文件完成"),
                metadata: Some(json!({"tool_call_id": "call-2"})),
                created_at: String::from("1400"),
            },
        ],
        continuation: Default::default(),
    };
    conversation_state
        .records
        .push(crate::conversation_protocol::SessionConversationRecord {
            id: String::from("terminated-1"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::TurnTerminated,
            visible: false,
            title: String::from("Turn Terminated"),
            body: String::from("stopped"),
            metadata: None,
            created_at: String::from("1500"),
        });

    let stats = build_session_turn_stats_from_state(
        &conversation_state,
        "terminated-1",
        SessionTurnStatsStatus::Stopped,
        Some(1),
    )
    .expect("stopped turn stats should build from checkpoint state");

    assert_eq!(stats.provider_call_count, 1);
    assert_eq!(stats.tool_call_count, 2);
    assert_eq!(stats.total_tokens, 125);
    assert_eq!(stats.status, SessionTurnStatsStatus::Stopped);
}

/// Verifies turn statistics prefer the exact provider request count captured by the runtime report instead of counting assistant payload segments.
/// 验证轮次统计会优先采用运行时报告捕获到的精确 Provider 请求次数，而不是按助手载荷片段数量计数。
#[test]
fn turn_stats_prefer_exact_provider_request_count_override() {
    let conversation_state = crate::conversation_protocol::SessionConversationState {
        messages: vec![
            crate::conversation_protocol::SessionConversationMessage {
                id: String::from("protocol-user-1"),
                payload: crate::conversation_protocol::SessionConversationPayload::UserText {
                    text: String::from("介绍下你自己"),
                    attachments: Vec::new(),
                },
            },
            crate::conversation_protocol::SessionConversationMessage {
                id: String::from("provider-assistant-thinking"),
                payload: crate::conversation_protocol::SessionConversationPayload::Assistant {
                    text: String::new(),
                    reasoning: String::from("先简短介绍自己。"),
                    provider_message_id: Some(String::from("resp-1")),
                    provider_message_state: None,
                    usage: None,
                    tool_calls: Vec::new(),
                },
            },
            crate::conversation_protocol::SessionConversationMessage {
                id: String::from("provider-assistant-output"),
                payload: crate::conversation_protocol::SessionConversationPayload::Assistant {
                    text: String::from("我是 VulcanCode。"),
                    reasoning: String::new(),
                    provider_message_id: Some(String::from("resp-1")),
                    provider_message_state: None,
                    usage: Some(vulcan_core::types::ProviderUsageSnapshot {
                        input_tokens: Some(100),
                        output_tokens: Some(40),
                        reasoning_tokens: Some(8),
                        total_tokens: Some(148),
                        cached_input_tokens: None,
                        cache_creation_input_tokens: None,
                    }),
                    tool_calls: Vec::new(),
                },
            },
        ],
        records: vec![
            crate::conversation_protocol::SessionConversationRecord {
                id: String::from("record-user-1"),
                kind: SessionMessageKind::User,
                record_kind: SessionMessageRecordKind::Transcript,
                visible: true,
                title: String::from("用户"),
                body: String::from("介绍下你自己"),
                metadata: None,
                created_at: String::from("1000"),
            },
            crate::conversation_protocol::SessionConversationRecord {
                id: String::from("record-assistant-1"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::AssistantOutput,
                visible: true,
                title: String::from("输出"),
                body: String::from("我是 VulcanCode。"),
                metadata: None,
                created_at: String::from("2000"),
            },
            crate::conversation_protocol::SessionConversationRecord {
                id: String::from("summary-1"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::SummaryOutput,
                visible: false,
                title: String::from("Summary"),
                body: String::from("done"),
                metadata: None,
                created_at: String::from("3000"),
            },
        ],
        continuation: Default::default(),
    };

    let stats = build_session_turn_stats_from_state(
        &conversation_state,
        "summary-1",
        SessionTurnStatsStatus::Completed,
        Some(1),
    )
    .expect("turn stats should build");

    assert_eq!(stats.provider_call_count, 1);
    assert_eq!(stats.total_tokens, 148);
}

/// Verifies turn-statistics alignment ignores one legacy duplicated `user-message-*` node when the canonical window already contains host-owned `protocol-user-*` anchors.
/// 验证当规范窗口已包含宿主拥有的 `protocol-user-*` 锚点时，轮次统计对齐会忽略旧版重复的 `user-message-*` 节点。
#[test]
fn turn_stats_ignore_legacy_duplicate_core_user_message_when_protocol_users_exist() {
    let conversation_state = crate::conversation_protocol::SessionConversationState {
        messages: vec![
            crate::conversation_protocol::SessionConversationMessage {
                id: String::from("session-1-protocol-user-1"),
                payload: crate::conversation_protocol::SessionConversationPayload::UserText {
                    text: String::from("随便输出一段内容"),
                    attachments: Vec::new(),
                },
            },
            crate::conversation_protocol::SessionConversationMessage {
                id: String::from("assistant-hidden-1"),
                payload: crate::conversation_protocol::SessionConversationPayload::Assistant {
                    text: String::from("标题生成说明"),
                    reasoning: String::new(),
                    provider_message_id: None,
                    provider_message_state: None,
                    usage: None,
                    tool_calls: Vec::new(),
                },
            },
            crate::conversation_protocol::SessionConversationMessage {
                id: String::from("user-message-legacy-1"),
                payload: crate::conversation_protocol::SessionConversationPayload::UserText {
                    text: String::from("随便输出一段内容"),
                    attachments: Vec::new(),
                },
            },
            crate::conversation_protocol::SessionConversationMessage {
                id: String::from("assistant-message-1"),
                payload: crate::conversation_protocol::SessionConversationPayload::Assistant {
                    text: String::from("Hello"),
                    reasoning: String::new(),
                    provider_message_id: Some(String::from("resp-1")),
                    provider_message_state: None,
                    usage: Some(vulcan_core::types::ProviderUsageSnapshot {
                        input_tokens: Some(8926),
                        output_tokens: Some(29),
                        reasoning_tokens: Some(0),
                        total_tokens: Some(8955),
                        cached_input_tokens: Some(0),
                        cache_creation_input_tokens: None,
                    }),
                    tool_calls: Vec::new(),
                },
            },
            crate::conversation_protocol::SessionConversationMessage {
                id: String::from("session-1-protocol-user-2"),
                payload: crate::conversation_protocol::SessionConversationPayload::UserText {
                    text: String::from("再输出一段内容呢"),
                    attachments: Vec::new(),
                },
            },
            crate::conversation_protocol::SessionConversationMessage {
                id: String::from("assistant-message-2"),
                payload: crate::conversation_protocol::SessionConversationPayload::Assistant {
                    text: String::from("第二段输出"),
                    reasoning: String::new(),
                    provider_message_id: Some(String::from("resp-2")),
                    provider_message_state: None,
                    usage: Some(vulcan_core::types::ProviderUsageSnapshot {
                        input_tokens: Some(8946),
                        output_tokens: Some(42),
                        reasoning_tokens: Some(0),
                        total_tokens: Some(8988),
                        cached_input_tokens: Some(8832),
                        cache_creation_input_tokens: None,
                    }),
                    tool_calls: Vec::new(),
                },
            },
        ],
        records: vec![
            crate::conversation_protocol::SessionConversationRecord {
                id: String::from("record-user-1"),
                kind: SessionMessageKind::User,
                record_kind: SessionMessageRecordKind::Transcript,
                visible: true,
                title: String::from("用户"),
                body: String::from("随便输出一段内容"),
                metadata: None,
                created_at: String::from("1000"),
            },
            crate::conversation_protocol::SessionConversationRecord {
                id: String::from("summary-1"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::SummaryOutput,
                visible: false,
                title: String::from("Summary"),
                body: String::from("done"),
                metadata: None,
                created_at: String::from("2000"),
            },
            crate::conversation_protocol::SessionConversationRecord {
                id: String::from("record-user-2"),
                kind: SessionMessageKind::User,
                record_kind: SessionMessageRecordKind::Transcript,
                visible: true,
                title: String::from("用户"),
                body: String::from("再输出一段内容呢"),
                metadata: None,
                created_at: String::from("3000"),
            },
            crate::conversation_protocol::SessionConversationRecord {
                id: String::from("summary-2"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::SummaryOutput,
                visible: false,
                title: String::from("Summary"),
                body: String::from("done"),
                metadata: None,
                created_at: String::from("4000"),
            },
        ],
        continuation: Default::default(),
    };

    let stats = build_session_turn_stats_from_state(
        &conversation_state,
        "summary-2",
        SessionTurnStatsStatus::Completed,
        Some(1),
    )
    .expect("turn stats should build");

    assert_eq!(stats.input_tokens, 8946);
    assert_eq!(stats.output_tokens, 42);
    assert_eq!(stats.total_tokens, 8988);
    assert_eq!(stats.cached_input_tokens, Some(8832));
}

/// Verifies that dedicated `task` tool cards use the robot dispatch title and upgrade from running to completed when the result arrives.
/// 验证专属 `task` 工具卡片会使用机器人调度标题，并在结果到达后从进行中升级为已完成。
#[test]
fn merge_tool_display_messages_formats_task_session_cards() {
    let messages = vec![
        SessionMessageEntry {
            id: String::from("task-call-1"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::ToolCallOutput,
            visible: true,
            title: String::from("调用 task 工具"),
            body: String::from("## 调用参数\n\n```json\n{\"description\":\"审查仓库\"}\n```"),
            metadata: Some(json!({
                "tool_call_id": "call-task-1",
                "tool_name": "task",
                "arguments": {
                    "description": "审查仓库",
                    "subagent": "explorer",
                },
            })),
            created_at: String::from("1"),
        },
        SessionMessageEntry {
            id: String::from("task-result-1"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::ToolResultOutput,
            visible: true,
            title: String::from("调用 task 工具"),
            body: String::from("## 工具返回\n\n子代理已完成。"),
            metadata: Some(json!({
                "tool_call_id": "call-task-1",
                "tool_name": "task",
                "kind": "task_session",
                "payload": {
                    "task_id": "00000000-0000-0000-0000-000000000001",
                    "description": "审查仓库",
                    "subagent": "explorer",
                    "execution_status": "completed",
                },
            })),
            created_at: String::from("2"),
        },
    ];

    let merged = merge_tool_display_messages(messages);
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].title, "调度 审查仓库（explorer） ✅ 完成");
    assert_eq!(
        merged[0]
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("tool_display_state"))
            .and_then(|value| value.as_str()),
        Some("completed")
    );
}

/// Verifies generated attachments can be recovered from flattened runtime-event metadata so inline media cards and tool modals work immediately after one successful capability call.
/// 验证可以从扁平化运行时事件元数据中恢复生成附件，从而让内联媒体卡片与工具弹层在能力调用成功后立即生效。
#[test]
fn generated_attachments_are_parsed_from_flattened_runtime_metadata() {
    let attachments = generated_attachments_from_tool_metadata(&json!({
        "tool_call_id": "call-media-1",
        "tool_result_id": "tool-result-call-media-1",
        "kind": "generated_attachments",
        "payload": {
            "outputs": [{
                "kind": "audio",
                "file_name": "demo.mp3",
                "local_path": r"D:\demo.mp3",
                "preview_local_path": r"D:\demo.preview.png",
                "mime_type": "audio/mpeg",
                "asset_id": "asset-1",
                "byte_len": 2048,
            }],
        },
    }));

    assert_eq!(attachments.len(), 1);
    assert_eq!(attachments[0].kind, ConversationAttachmentKind::Audio);
    assert_eq!(attachments[0].file_name, "demo.mp3");
    assert_eq!(attachments[0].local_path, r"D:\demo.mp3");
    assert_eq!(
        attachments[0].preview_local_path.as_deref(),
        Some(r"D:\demo.preview.png")
    );
}

#[test]
fn merge_tool_display_messages_prefers_terminal_task_payload_over_stale_running_result() {
    let messages = vec![
        SessionMessageEntry {
            id: String::from("task-call-1"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::ToolCallOutput,
            visible: true,
            title: String::from("调用 task 工具"),
            body: String::from("## 调用参数"),
            metadata: Some(json!({
                "tool_call_id": "call-task-1",
                "tool_name": "task",
                "kind": "task_session",
                "payload": {
                    "task_id": "count_10",
                    "task_session_id": "00000000-0000-0000-0000-000000000001",
                    "display_title": "数数到10（worker）",
                    "description": "数数到10",
                    "subagent": "worker",
                    "execution_status": "completed",
                },
            })),
            created_at: String::from("1"),
        },
        SessionMessageEntry {
            id: String::from("task-result-1"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::ToolResultOutput,
            visible: true,
            title: String::from("调用 task 工具"),
            body: String::from("## 工具返回\n\n运行中。"),
            metadata: Some(json!({
                "tool_call_id": "call-task-1",
                "tool_name": "task",
                "kind": "task_session",
                "payload": {
                    "task_id": "count_10",
                    "task_session_id": "00000000-0000-0000-0000-000000000001",
                    "display_title": "数数到10（worker）",
                    "description": "数数到10",
                    "subagent": "worker",
                    "execution_status": "running",
                },
            })),
            created_at: String::from("2"),
        },
    ];

    let merged = merge_tool_display_messages(messages);
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].title, "调度 数数到10（worker） ✅ 完成");
    assert_eq!(
        merged[0]
            .metadata
            .as_ref()
            .and_then(task_session_payload_from_tool_metadata)
            .and_then(|payload| payload.get("execution_status"))
            .and_then(Value::as_str),
        Some("completed")
    );
}

/// Verifies hidden streaming sessions skip heavyweight transcript cache refreshes while still applying the delta to canonical and visible message data.
/// 验证隐藏流式会话会跳过重型转录缓存刷新，同时仍将增量写入规范状态与可见消息数据。
#[test]
fn hidden_streaming_session_skips_view_cache_refresh() {
    let (mut workbench, _task) = VulcanWorkbench::new();
    let hidden_session_id = SessionId::from_u128(9801);
    let visible_session_id = SessionId::from_u128(9802);
    workbench.sessions.clear();
    workbench
        .sessions
        .push(test_session_summary(hidden_session_id, "后台会话"));
    workbench
        .sessions
        .push(test_session_summary(visible_session_id, "前台会话"));
    workbench.selected_session = visible_session_id;

    let hidden_message_id = String::from("assistant-hidden-1");
    let hidden_messages = vec![SessionMessageEntry {
        id: hidden_message_id.clone(),
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::AssistantOutput,
        visible: true,
        title: String::from("输出"),
        body: String::from("before"),
        metadata: None,
        created_at: String::from("1"),
    }];
    let hidden_conversation_state =
        crate::conversation_protocol::conversation_state_from_session_messages(&hidden_messages);
    workbench
        .session_messages
        .insert(hidden_session_id, hidden_messages);
    workbench.session_runtime_state.insert(
        hidden_session_id,
        SessionRuntimeStateEntry {
            conversation_state: hidden_conversation_state,
            updated_at: String::from("1"),
            ..SessionRuntimeStateEntry::default()
        },
    );

    workbench.refresh_session_agent_view_buffers(hidden_session_id);
    assert!(
        workbench
            .session_agent_view_documents
            .contains_key(&hidden_session_id)
    );

    workbench.append_stream_delta_to_session_message(
        hidden_session_id,
        &hidden_message_id,
        " after",
    );

    assert_eq!(
        workbench
            .session_message_entry(hidden_session_id, &hidden_message_id)
            .map(|message| message.body.as_str()),
        Some("before after")
    );
    assert!(
        !workbench
            .session_agent_view_documents
            .contains_key(&hidden_session_id)
    );
    assert!(
        !workbench
            .session_message_raw_buffers
            .contains_key(&hidden_message_id)
    );
    assert!(
        !workbench
            .session_message_markdown_buffers
            .contains_key(&hidden_message_id)
    );
}

/// Verifies the active transcript surface keeps heavyweight per-message caches stable during streaming so per-token updates do not repeatedly drop large buffers.
/// 验证当前活动转录表面会在流式阶段保持重型逐消息缓存稳定，避免逐 token 更新反复释放大型缓冲对象。
#[test]
fn visible_streaming_session_keeps_heavy_message_view_caches_stable() {
    let (mut workbench, _task) = VulcanWorkbench::new();
    let session_id = SessionId::from_u128(9803);
    workbench.sessions.clear();
    workbench
        .sessions
        .push(test_session_summary(session_id, "当前会话"));
    workbench.selected_session = session_id;

    let message_id = String::from("assistant-visible-1");
    let messages = vec![SessionMessageEntry {
        id: message_id.clone(),
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::AssistantOutput,
        visible: true,
        title: String::from("输出"),
        body: String::from("hello"),
        metadata: None,
        created_at: String::from("1"),
    }];
    let conversation_state =
        crate::conversation_protocol::conversation_state_from_session_messages(&messages);
    workbench.session_messages.insert(session_id, messages);
    workbench.session_runtime_state.insert(
        session_id,
        SessionRuntimeStateEntry {
            conversation_state,
            updated_at: String::from("1"),
            ..SessionRuntimeStateEntry::default()
        },
    );
    workbench
        .session_message_raw_buffers
        .insert(message_id.clone(), text_editor::Content::with_text("hello"));
    workbench.session_message_markdown.insert(
        message_id.clone(),
        markdown::Content::parse_transcript("hello"),
    );
    workbench
        .session_message_markdown_buffers
        .insert(message_id.clone(), text_editor::Content::with_text("hello"));
    workbench.refresh_session_agent_view_buffers(session_id);

    workbench.append_stream_delta_to_session_message(session_id, &message_id, " world");

    assert_eq!(
        workbench
            .session_message_entry(session_id, &message_id)
            .map(|message| message.body.as_str()),
        Some("hello world")
    );
    assert!(
        workbench
            .session_agent_view_documents
            .contains_key(&session_id)
    );
    assert!(
        workbench
            .session_message_raw_buffers
            .contains_key(&message_id)
    );
    assert!(workbench.session_message_markdown.contains_key(&message_id));
    assert!(
        workbench
            .session_message_markdown_buffers
            .contains_key(&message_id)
    );
}
