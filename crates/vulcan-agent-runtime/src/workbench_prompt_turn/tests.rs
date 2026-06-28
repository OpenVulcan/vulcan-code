//! Tests for shared prompt-turn routing decisions.
//! 共享提示词回合路由决策测试。

use std::collections::{BTreeMap, BTreeSet};

use vulcan_provider_config::config::{
    PersistedAiToolConfiguration, ProjectVulcanCodeConfigurationFile,
};
use vulcan_session_boundary::{SessionBoundaryEventKind, SessionEventVisibility};
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
    assert!(continuation.request.prompt.contains("执行计划"));
    assert!(
        continuation
            .request
            .prompt
            .contains("<hidden_host_guidance>\nPlan file: D:/workspace/docs/plan/demo.md")
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
    );
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
