//! Tests for shared prompt-turn routing decisions.
//! 共享提示词回合路由决策测试。

use std::collections::{BTreeMap, BTreeSet};

use vulcan_provider_config::config::{
    PersistedAiToolConfiguration, ProjectVulcanCodeConfigurationFile,
};
use vulcan_session_boundary::{SessionBoundaryEventKind, SessionEventVisibility};
use vulcan_vmm::{ContextItem, VmmPromptAnchorKind, VmmRecallBatch, VmmSessionMemoryState};
use vulcan_workbench_model::{
    PendingSessionPromptSource, SessionId, SessionMessageEntry, SessionMessageKind,
    SessionRuntimeStateEntry,
};

use crate::{
    agent_runtime::conversation_hooks::ConversationHookRuntime,
    conversation_protocol::{
        SessionConversationPayload, append_user_text_message,
        conversation_state_to_session_messages,
    },
};

use super::*;

/// Verifies canonical user ids cannot collide when multiple starts occur within one timestamp tick.
/// 验证多次启动发生在同一时间戳粒度内时规范用户标识仍不会碰撞。
#[test]
fn canonical_user_message_ids_are_unique_and_session_scoped() {
    let session_id = SessionId::from_u128(902);
    let first = new_canonical_user_message_id(session_id);
    let second = new_canonical_user_message_id(session_id);

    assert_ne!(first, second);
    assert!(first.starts_with(&format!("session-{session_id}-protocol-user-")));
    assert!(second.starts_with(&format!("session-{session_id}-protocol-user-")));
}

/// Begins one user-authored prompt turn used by session boundary tests.
/// 启动一条供 会话边界测试使用的用户提示词回合。
fn begin_user_prompt(runtime_state: &mut SessionRuntimeStateEntry) -> SharedPromptTurnStart {
    let mut hook_runtime = ConversationHookRuntime::new();
    begin_shared_prompt_turn_with_runtime_state(
        runtime_state,
        &mut hook_runtime,
        SharedPromptTurnRuntimeBeginInput {
            session_id: SessionId::from_u128(902),
            session_title: "主会话",
            workspace_path: "D:/workspace",
            prompt_entry_id: String::from("user-entry"),
            prompt: "生成一张图片",
            hidden_model_guidance: None,
            provider_visible_agent_messages: Vec::new(),
            attachments: Vec::new(),
            append_canonical_user_message: true,
            append_visible_user_prompt_record: true,
            source: PendingSessionPromptSource::UserFollowup,
            execution_mode: String::from("Build"),
            reasoning_effort: None,
            disabled_ai_capability_kinds: BTreeSet::new(),
            disabled_ai_capability_target_selectors: BTreeSet::new(),
            ai_capability_target_order_overrides: BTreeMap::new(),
            owning_project_id: Some(String::from("project-a")),
            prior_session_messages: None,
        },
    )
    .expect("user prompt should start")
}

/// Verifies user prompt begins expose visible session user-input boundary events.
/// 验证用户提示词开始时会暴露可见的 会话用户输入边界事件。
#[test]
fn user_prompt_begin_exports_visible_boundary_events() {
    let mut runtime_state = SessionRuntimeStateEntry::default();
    let start = begin_user_prompt(&mut runtime_state);

    assert_eq!(start.boundary_events.len(), 2);
    assert_eq!(
        start.boundary_events[0].kind,
        SessionBoundaryEventKind::TurnStarted
    );
    assert_eq!(
        start.boundary_events[0].visibility,
        SessionEventVisibility::Hidden
    );
    assert_eq!(
        start.boundary_events[1].kind,
        SessionBoundaryEventKind::UserInput
    );
    assert_eq!(
        start.boundary_events[1].visibility,
        SessionEventVisibility::Visible
    );
    assert_eq!(
        start.boundary_events[1].payload["source"],
        serde_json::json!("user_followup")
    );
}

/// Verifies session-mailbox turns cannot accidentally expose visible user boundary or transcript records.
/// 验证会话 mailbox 回合不会意外暴露可见用户边界事件或转录记录。
#[test]
fn session_mailbox_begin_keeps_user_input_hidden_even_if_visibility_flag_is_true() {
    let mut runtime_state = SessionRuntimeStateEntry::default();
    let mut hook_runtime = ConversationHookRuntime::new();
    let start = begin_shared_prompt_turn_with_runtime_state(
        &mut runtime_state,
        &mut hook_runtime,
        SharedPromptTurnRuntimeBeginInput {
            session_id: SessionId::from_u128(903),
            session_title: "主会话",
            workspace_path: "D:/workspace",
            prompt_entry_id: String::from("mailbox-entry"),
            prompt: "<subagent_mailbox_event>\n{}\n</subagent_mailbox_event>",
            hidden_model_guidance: None,
            provider_visible_agent_messages: Vec::new(),
            attachments: Vec::new(),
            append_canonical_user_message: true,
            append_visible_user_prompt_record: true,
            source: PendingSessionPromptSource::SessionMailbox,
            execution_mode: String::from("Build"),
            reasoning_effort: None,
            disabled_ai_capability_kinds: BTreeSet::new(),
            disabled_ai_capability_target_selectors: BTreeSet::new(),
            ai_capability_target_order_overrides: BTreeMap::new(),
            owning_project_id: Some(String::from("project-a")),
            prior_session_messages: None,
        },
    )
    .expect("mailbox prompt should start");

    assert!(runtime_state.conversation_state.messages.is_empty());
    assert!(runtime_state.conversation_state.records.is_empty());
    assert_eq!(
        start.boundary_events[1].visibility,
        SessionEventVisibility::Hidden
    );
    assert_eq!(
        start.boundary_events[1].payload["source"],
        serde_json::json!("session_mailbox")
    );
    assert_eq!(
        start.boundary_events[1].payload["canonical"],
        serde_json::json!(false)
    );
}

/// Verifies goal continuation turns never persist hidden prompts as reopen-visible user text.
/// 验证目标续跑回合绝不会把隐藏提示持久化成重开后可见的用户文本。
#[test]
fn goal_continuation_begin_never_persists_user_message() {
    let mut runtime_state = SessionRuntimeStateEntry::default();
    let mut hook_runtime = ConversationHookRuntime::new();
    let start = begin_shared_prompt_turn_with_runtime_state(
        &mut runtime_state,
        &mut hook_runtime,
        SharedPromptTurnRuntimeBeginInput {
            session_id: SessionId::from_u128(904),
            session_title: "主会话",
            workspace_path: "D:/workspace",
            prompt_entry_id: String::from("goal-entry"),
            prompt: "Continue working toward the active thread goal.",
            hidden_model_guidance: None,
            provider_visible_agent_messages: Vec::new(),
            attachments: Vec::new(),
            append_canonical_user_message: true,
            append_visible_user_prompt_record: true,
            source: PendingSessionPromptSource::GoalContinuation,
            execution_mode: String::from("Build"),
            reasoning_effort: None,
            disabled_ai_capability_kinds: BTreeSet::new(),
            disabled_ai_capability_target_selectors: BTreeSet::new(),
            ai_capability_target_order_overrides: BTreeMap::new(),
            owning_project_id: Some(String::from("project-a")),
            prior_session_messages: None,
        },
    )
    .expect("goal continuation prompt should start");

    assert!(runtime_state.conversation_state.messages.is_empty());
    assert!(runtime_state.conversation_state.records.is_empty());
    assert_eq!(
        start.boundary_events[1].visibility,
        SessionEventVisibility::Hidden
    );
    assert_eq!(
        start.boundary_events[1].payload["source"],
        serde_json::json!("goal_continuation")
    );
    assert_eq!(
        start.boundary_events[1].payload["canonical"],
        serde_json::json!(false)
    );
}

/// Verifies hidden model guidance reaches the provider request without polluting transcript records.
/// 验证隐藏模型引导会进入 Provider 请求，但不会污染转录记录。
#[test]
fn hidden_model_guidance_stays_out_of_visible_transcript() {
    let mut runtime_state = SessionRuntimeStateEntry::default();
    let mut hook_runtime = ConversationHookRuntime::new();
    let start = begin_shared_prompt_turn_with_runtime_state(
        &mut runtime_state,
        &mut hook_runtime,
        SharedPromptTurnRuntimeBeginInput {
            session_id: SessionId::from_u128(905),
            session_title: "主会话",
            workspace_path: "D:/workspace",
            prompt_entry_id: String::from("plan-execute-entry"),
            prompt: "执行计划",
            hidden_model_guidance: Some("Plan file: D:/workspace/docs/plan/demo.md"),
            provider_visible_agent_messages: Vec::new(),
            attachments: Vec::new(),
            append_canonical_user_message: true,
            append_visible_user_prompt_record: true,
            source: PendingSessionPromptSource::UserFollowup,
            execution_mode: String::from("Build"),
            reasoning_effort: None,
            disabled_ai_capability_kinds: BTreeSet::new(),
            disabled_ai_capability_target_selectors: BTreeSet::new(),
            ai_capability_target_order_overrides: BTreeMap::new(),
            owning_project_id: Some(String::from("project-a")),
            prior_session_messages: None,
        },
    )
    .expect("plan execution prompt should start");
    let precheck_outcome = start.precheck_request.clone().skip();
    // expected_hook_turn_id is the validated precheck correlation that must survive request construction.
    // expected_hook_turn_id 是必须在请求构建后保留的已验证预检关联。
    let expected_hook_turn_id = precheck_outcome.turn_id.clone();
    let continuation = continue_shared_prompt_turn_after_precheck_with_runtime_state(
        &mut runtime_state,
        SharedPromptTurnContinueInput {
            start,
            precheck_outcome,
            ai_tool_configuration: PersistedAiToolConfiguration::default(),
            project_configuration: ProjectVulcanCodeConfigurationFile::default(),
            runtime_reasoning_effort: None,
            provider_reasoning_effort: None,
            patch_visible_user_prompt_record: true,
        },
    )
    .expect("prompt should continue");
    let transcript_text = conversation_state_to_session_messages(&runtime_state.conversation_state)
        .into_iter()
        .map(|message| message.body)
        .collect::<Vec<_>>()
        .join("\n");

    assert_eq!(transcript_text, "执行计划");
    assert_eq!(
        continuation.request.conversation_hook_turn_id.as_deref(),
        Some(expected_hook_turn_id.as_str())
    );
    assert!(continuation.request.prompt.contains("执行计划"));
    assert!(
        continuation
            .request
            .prompt
            .contains("<hidden_host_guidance>\nPlan file: D:/workspace/docs/plan/demo.md")
    );
}

/// Verifies remote PreCheck failure leaves expired VMM state resident without adding a prompt anchor.
/// 验证远端 PreCheck 失败时，过期 VMM 状态保持驻留且不会新增提示词锚点。
#[test]
fn precheck_failure_preserves_expired_vmm_resident_and_registers_terminal_turn() {
    // session_id matches the fixed owner used by begin_user_prompt.
    // session_id 与 begin_user_prompt 使用的固定所有者一致。
    let session_id = SessionId::from_u128(902);
    // runtime_state starts with one canonical message and one lease that expires before the tested turn.
    // runtime_state 初始包含一条规范消息，以及一条在被测回合前到期的租约。
    let mut runtime_state = SessionRuntimeStateEntry::default();
    append_user_text_message(
        &mut runtime_state.conversation_state,
        "old-user",
        "old question",
        Vec::new(),
    )
    .expect("old canonical user message should append");
    runtime_state.vmm_session_memory = VmmSessionMemoryState::default();
    runtime_state
        .vmm_session_memory
        .apply_recall_batch(
            &session_id.to_string(),
            "old-user",
            &VmmRecallBatch {
                ttl_turns: 1,
                injection_tag: String::from("VMM_MEMORY"),
                injection_description: String::from("Relevant memories."),
                items: vec![ContextItem {
                    text: String::from("expiring memory"),
                    memory_id: 7,
                    ..ContextItem::default()
                }],
            },
            "1",
        )
        .expect("initial VMM recall should succeed");
    runtime_state
        .vmm_session_memory
        .register_active_turn("old-turn", "1")
        .expect("old logical turn should register");
    runtime_state
        .vmm_session_memory
        .consume_turn("old-turn", "2")
        .expect("old logical turn should expire the lease");
    // start creates the exact next canonical user message before the failing hook outcome is applied.
    // start 会在应用失败 Hook 结果前创建精确的下一条规范用户消息。
    let start = begin_user_prompt(&mut runtime_state);
    // target_message_id is the authoritative new anchor target under test.
    // target_message_id 是被测的新锚点权威目标。
    let target_message_id = start
        .canonical_user_message_id
        .clone()
        .expect("user followup must create a canonical message");
    // precheck_outcome preserves correlation while replacing the remote report with a failure.
    // precheck_outcome 保留关联标识，同时将远端报告替换为失败结果。
    let mut precheck_outcome = start.precheck_request.clone().skip();
    precheck_outcome.report = Err(String::from("simulated remote precheck failure"));
    // expected_turn_id is the exact boundary that must be registered before Provider execution.
    // expected_turn_id 是 Provider 执行前必须登记的精确边界。
    let expected_turn_id = precheck_outcome.turn_id.clone();

    let continuation = continue_shared_prompt_turn_after_precheck_with_runtime_state(
        &mut runtime_state,
        SharedPromptTurnContinueInput {
            start,
            precheck_outcome,
            ai_tool_configuration: PersistedAiToolConfiguration::default(),
            project_configuration: ProjectVulcanCodeConfigurationFile::default(),
            runtime_reasoning_effort: None,
            provider_reasoning_effort: None,
            patch_visible_user_prompt_record: true,
        },
    )
    .expect("local VMM state should continue despite remote precheck failure");

    assert!(continuation.precheck_failure_detail.is_some());
    assert!(
        runtime_state
            .vmm_session_memory
            .pending_prompt_events
            .is_empty()
    );
    assert!(
        runtime_state
            .vmm_session_memory
            .active_panel_card()
            .is_none()
    );
    assert!(
        runtime_state
            .vmm_session_memory
            .expired_panel_card()
            .is_some()
    );
    assert_eq!(
        runtime_state.vmm_session_memory.active_turn_id.as_deref(),
        Some(expected_turn_id.as_str())
    );
    assert_eq!(
        runtime_state
            .vmm_session_memory
            .cache_epoch
            .anchors
            .last()
            .map(|anchor| (&anchor.kind, anchor.target_message_id.as_str())),
        Some((&VmmPromptAnchorKind::Memory, "old-user"))
    );
    assert_ne!(
        runtime_state
            .vmm_session_memory
            .cache_epoch
            .anchors
            .last()
            .map(|anchor| anchor.target_message_id.as_str()),
        Some(target_message_id.as_str())
    );
}

/// Verifies a loaded dialogue message window is not overwritten by display-only fallback rows.
/// 验证已加载的对话层消息窗口不会被仅显示层的回退行覆盖。
#[test]
fn begin_prompt_keeps_runtime_messages_when_records_are_empty() {
    // runtime_state simulates a restarted session whose API context restored messages but not display records.
    // runtime_state 模拟重启后恢复了 API 消息上下文但没有显示记录的会话。
    let mut runtime_state = SessionRuntimeStateEntry::default();
    append_user_text_message(
        &mut runtime_state.conversation_state,
        "dialogue-history",
        "对话层历史",
        Vec::new(),
    )
    .expect("valid user prompt identity");
    // display_rows simulate a visible turn window that must not become the API source of truth.
    // display_rows 模拟不应变成 API 真相来源的可见 turn 窗口。
    let display_rows = vec![SessionMessageEntry::new(
        SessionMessageKind::User,
        "用户",
        "显示层历史",
    )];
    let mut hook_runtime = ConversationHookRuntime::new();

    begin_shared_prompt_turn_with_runtime_state(
        &mut runtime_state,
        &mut hook_runtime,
        SharedPromptTurnRuntimeBeginInput {
            session_id: SessionId::from_u128(906),
            session_title: "主会话",
            workspace_path: "D:/workspace",
            prompt_entry_id: String::from("followup-entry"),
            prompt: "继续",
            hidden_model_guidance: None,
            provider_visible_agent_messages: Vec::new(),
            attachments: Vec::new(),
            append_canonical_user_message: true,
            append_visible_user_prompt_record: true,
            source: PendingSessionPromptSource::UserFollowup,
            execution_mode: String::from("Build"),
            reasoning_effort: None,
            disabled_ai_capability_kinds: BTreeSet::new(),
            disabled_ai_capability_target_selectors: BTreeSet::new(),
            ai_capability_target_order_overrides: BTreeMap::new(),
            owning_project_id: Some(String::from("project-a")),
            prior_session_messages: Some(&display_rows),
        },
    )
    .expect("prompt should start");

    let bodies = runtime_state
        .conversation_state
        .messages
        .iter()
        .map(|message| match &message.payload {
            SessionConversationPayload::UserText { text, .. } => text.as_str(),
            _ => "",
        })
        .collect::<Vec<_>>();
    assert_eq!(bodies, vec!["对话层历史", "继续"]);
}

/// Verifies invalid hydrated message ownership is rejected without changing the active runtime state.
/// 验证无效的水合消息归属会被拒绝且不修改活动运行态。
#[test]
fn begin_prompt_rejects_invalid_hydration_atomically() {
    // runtime_state is the empty active transaction target that must remain untouched.
    // runtime_state 是必须保持不变的空活动事务目标。
    let mut runtime_state = SessionRuntimeStateEntry::default();
    runtime_state.ui_state.unread = true;
    runtime_state.updated_at = String::from("before-hydration");
    // original_state captures every field before detached hydration validation begins.
    // original_state 捕获分离水合校验开始前的每个字段。
    let original_state =
        serde_json::to_value(&runtime_state).expect("runtime state should serialize");
    // first_row and second_row intentionally claim the same canonical message identity.
    // first_row 与 second_row 故意占用相同的规范消息身份。
    let mut first_row = SessionMessageEntry::new(SessionMessageKind::User, "用户", "first");
    first_row.id = String::from("duplicate-history");
    let mut second_row = SessionMessageEntry::new(SessionMessageKind::User, "用户", "second");
    second_row.id = String::from("duplicate-history");
    // display_rows is the invalid detached hydration source presented to prompt start.
    // display_rows 是提交给提示词启动流程的无效分离水合来源。
    let display_rows = vec![first_row, second_row];
    // hook_runtime remains available only if canonical preflight succeeds.
    // hook_runtime 仅在规范预检成功后才会被实际使用。
    let mut hook_runtime = ConversationHookRuntime::new();

    // error proves hydration is rejected before assignment or unread-state mutation.
    // error 证明水合会在赋值或 unread 状态修改前被拒绝。
    let error = begin_shared_prompt_turn_with_runtime_state(
        &mut runtime_state,
        &mut hook_runtime,
        SharedPromptTurnRuntimeBeginInput {
            session_id: SessionId::from_u128(907),
            session_title: "主会话",
            workspace_path: "D:/workspace",
            prompt_entry_id: String::from("followup-entry"),
            prompt: "继续",
            hidden_model_guidance: None,
            provider_visible_agent_messages: Vec::new(),
            attachments: Vec::new(),
            append_canonical_user_message: true,
            append_visible_user_prompt_record: true,
            source: PendingSessionPromptSource::UserFollowup,
            execution_mode: String::from("Build"),
            reasoning_effort: None,
            disabled_ai_capability_kinds: BTreeSet::new(),
            disabled_ai_capability_target_selectors: BTreeSet::new(),
            ai_capability_target_order_overrides: BTreeMap::new(),
            owning_project_id: Some(String::from("project-a")),
            prior_session_messages: Some(&display_rows),
        },
    )
    .expect_err("invalid hydrated identities must fail");

    assert!(error.contains("duplicates id"));
    assert_eq!(
        serde_json::to_value(&runtime_state).expect("runtime state should serialize after failure"),
        original_state
    );
}
