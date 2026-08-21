//! Context compaction tests.
//! 上下文压缩测试。

use super::*;
use vulcan_agent_runtime::agent_runtime::prompt_compaction::PromptCompactionExecutionProfile;
use vulcan_agent_runtime::conversation_protocol::conversation_state_from_session_messages;
use vulcan_agent_runtime::provider_config::PersistedPromptOutputStyleSelection;
use vulcan_context_injection::{InjectionInsert, InjectionKind};
use vulcan_workbench_model::SessionTurnStats;

/// Builds one frozen tail report aligned with a test policy and canonical suffix.
/// 构建与测试策略及规范后缀对齐的冻结尾部报告。
///
/// # Parameters
/// 参数
/// `policy` is the exact policy carried by the plan and `retained_message_ids` are the ordered suffix IDs.
/// `policy` 是计划携带的精确策略，`retained_message_ids` 是有序后缀 ID。
///
/// # Returns
/// 返回值
/// Returns a structurally consistent single-segment fixture for draft-transaction tests.
/// 返回供候选事务测试使用的结构一致单执行段夹具。
fn test_tail_selection(
    policy: vulcan_context::ContextCompactionPolicy,
    retained_message_ids: &[&str],
) -> vulcan_context_compaction::CompactionTailSelectionReport {
    vulcan_context_compaction::CompactionTailSelectionReport {
        context_limit: policy.tail_context_limit,
        policy_source: policy.tail_policy_source,
        nominal_token_budget: policy.preserve_recent_tokens,
        segment_limit: policy.tail_turns,
        retained_segment_count: usize::from(!retained_message_ids.is_empty()),
        retained_message_ids: retained_message_ids
            .iter()
            .map(|message_id| (*message_id).to_string())
            .collect(),
        retained_token_estimate: 0,
        previewed_tool_output_count: 0,
        omitted_tool_output_token_estimate: 0,
        newest_segment_exceeds_nominal_budget: false,
    }
}

/// Creates strict controller-owned VMM state for one compaction test session.
/// 为一个压缩测试会话创建严格归属于控制器的 VMM 状态。
fn test_vmm_session_memory(session_id: SessionId) -> vulcan_vmm::VmmSessionMemoryState {
    vulcan_vmm::VmmSessionMemoryState::new(session_id.to_string(), "1")
        .expect("test VMM session owner should be valid")
}

/// Records one completed physical context-compaction call in a test runtime state.
/// 在测试运行态中记录一次已经完成的上下文压缩物理调用。
///
/// # Parameters
/// 参数
/// `runtime_state` owns the durable call ledger and `execution_id` identifies the compaction execution.
/// `runtime_state` 持有可靠调用账本，`execution_id` 标识压缩执行。
///
/// # Returns
/// 返回值
/// Returns no value and panics when the deterministic fixture violates ledger invariants.
/// 不返回值；当确定性夹具违反账本约束时触发 panic。
fn record_context_compaction_provider_call(
    runtime_state: &mut SessionRuntimeStateEntry,
    execution_id: &str,
) {
    let provider_call_id = format!("provider-call-{execution_id}");
    let identity = vulcan_workbench_model::SessionProviderCallIdentity {
        logical_request_id: format!("logical-request-{execution_id}"),
        execution_id: execution_id.to_string(),
        turn_id: None,
        attempt_index: 0,
        retry_number: 0,
        owner_kind: String::from("context_compaction"),
        owner_id: String::from("context-compaction"),
        owner_name_snapshot: String::from("Context Compaction"),
        operation: String::from("context_compaction"),
        provider_id: String::from("provider-test"),
        provider_name_snapshot: String::from("Provider Test"),
        model_id: String::from("model-test"),
        model_name_snapshot: String::from("Model Test"),
        connection_id: String::from("connection-test"),
        service_entry_id: String::from("public-api"),
        protocol_profile_id: String::from("openai.responses"),
        manifest_revision: 1,
        manifest_generation: 1,
        binding_source: String::from("system_manifest"),
        binding_rule: Some(String::from("test-model-rule")),
        route_revision: String::from("sha256:test-route"),
        request_profiles: Vec::new(),
        response_profiles: Vec::new(),
    };
    assert!(runtime_state.provider_call_ledger.observe_started(
        vulcan_workbench_model::SessionProviderCallStarted {
            provider_call_id: provider_call_id.clone(),
            identity: identity.clone(),
            prepared_at_ms: 1_780_000_000_050,
            request_assembly_fingerprint: Some(String::from("sha256:test-request")),
            context_generation_id: None,
            context_start_record_id: None,
            model_route_fingerprint: Some(String::from("sha256:test-route")),
            toolset_fingerprint: None,
            system_prompt_fingerprint: None,
            request_text_measurement: None,
            budget_accounting: None,
        }
    ));
    assert!(runtime_state.provider_call_ledger.observe_completed(
        vulcan_workbench_model::SessionProviderCallCompleted {
            provider_call_id,
            identity,
            status: vulcan_workbench_model::SessionProviderCallStatus::Succeeded,
            provider_usage_evidence: vulcan_workbench_model::SessionProviderUsageEvidence {
                availability: vulcan_workbench_model::SessionProviderUsageAvailability::Missing,
                provider_usage: None,
            },
            prepared_at_ms: 1_780_000_000_050,
            dispatched_at_ms: Some(1_780_000_000_051),
            first_event_at_ms: Some(1_780_000_000_052),
            first_token_at_ms: Some(1_780_000_000_053),
            completed_at_ms: 1_780_000_000_099,
            dispatch_outcome: Some(String::from("dispatched")),
            dispatch_outcome_known: Some(true),
            semantic_output_observed: Some(true),
            semantic_output_observed_known: Some(true),
            replay_safe: Some(false),
            error_category: None,
            error_code: None,
        }
    ));
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

/// Verifies an auto-compaction restart consumes the seeded checkpoint without appending another user prompt.
/// 验证自动压缩重启会消费已播种 checkpoint，而不会追加另一条用户提示词。
#[test]
fn refreshed_auto_compaction_request_uses_restored_checkpoint_start_mode() {
    // sessionId identifies the exact compacted runtime state and request owner.
    // sessionId 标识精确的已压缩运行态与请求所有者。
    let session_id = SessionId::new_v4();
    // compactedState contains the internal continuation seed already produced by the compaction draft.
    // compactedState 包含压缩候选已经生成的内部续接种子。
    let compacted_state = SessionConversationState {
        messages: vec![SessionConversationMessage {
            id: String::from("compacted-continuation-seed"),
            payload: SessionConversationPayload::UserText {
                text: String::from("继续"),
                attachments: Vec::new(),
            },
        }],
        ..SessionConversationState::default()
    };
    // serviceState provides the retained post-compaction runtime row read by the refresh helper.
    // serviceState 提供刷新辅助函数读取的压缩后保留运行态行。
    let mut service_state =
        crate::tests::test_support::test_service_state_with_parent_session(session_id);
    service_state
        .workspace_snapshot
        .as_mut()
        .expect("test workspace should exist")
        .session_runtime_state
        .insert(
            session_id,
            SessionRuntimeStateEntry {
                conversation_state: compacted_state.clone(),
                ..SessionRuntimeStateEntry::default()
            },
        );
    // sharedState exposes the same retained revision through the production locking contract.
    // sharedState 通过生产锁契约暴露同一份保留修订。
    let shared_state = std::sync::Arc::new(std::sync::Mutex::new(service_state));
    // request deliberately begins as a normal user prompt to prove refresh overrides that mode.
    // request 刻意从普通用户提示词模式开始，以证明刷新会覆盖该模式。
    let request = ExecutionPreparationRequest {
        session_id,
        managed_codebase_memory: None,
        execution_scope: vulcan_session_core::ExecutionScope::ContextCompaction {
            node_id: vulcan_session_core::SessionNodeId::from(session_id),
        },
        runtime_policy: vulcan_session_core::ExecutionRuntimePolicy::context_compaction(),
        conversation_hook_turn_id: None,
        execution_retry_snapshot: None,
        execution_agent_id: None,
        agent_prompt_snapshot: None,
        provider_id_override: None,
        model_id_override: None,
        connection_id_override: None,
        provider_route_lease_identity: None,
        provider_usage_budget_baseline: None,
        provider_usage_context_generation_id: None,
        provider_tool_selections_override: Default::default(),
        provider_tool_enabled_capabilities_override: Default::default(),
        provider_extension_target_sets_override: Default::default(),
        subagent_prompt_context: None,
        task_session_store_root: None,
        session_title: String::from("Compacted conversation"),
        workspace_path: String::from("D:/workspace"),
        prompt: String::from("继续"),
        output_style_snapshot: None,
        provider_visible_agent_messages: Vec::new(),
        turn_start_mode: Some(
            vulcan_agent_runtime::agent_runtime::execution_models::ExecutionTurnStartMode::UserPrompt,
        ),
        conversation_state: SessionConversationState::default(),
        context_compaction_policy: None,
        context_compaction_window_node_id: None,
        context_budget_override: None,
        context_window_prefill_input_tokens: None,
        execution_mode: String::from("Build"),
        temperature: None,
        reasoning_effort: ReasoningEffortLevel::None,
        tool_exposure_override: Default::default(),
        tool_request_rejection_reason: None,
        tool_request_rejection_all_tools: false,
        tool_request_rejection_tool_ids: Vec::new(),
        tool_request_hidden_tool_ids: Vec::new(),
        prompt_purpose: ExecutionPromptPurpose::UserTurn,
        prompt_skill_policy_override: Default::default(),
        project_prompt_skills: Default::default(),
        session_prompt_skill_overrides: Default::default(),
        context_injection: None,
        system_prompt_hook_blocks: Vec::new(),
        project_vmm: Default::default(),
        session_vmm: Default::default(),
        vmm_runtime_ready: false,
        boundary_events: Vec::new(),
        runtime_system_updates: Vec::new(),
        controller_runtime: None,
        knowledge_access: None,
        session_goal_context: None,
        session_plan_context: None,
        embedded_turn_usage_segments: Vec::new(),
    };
    // vmmSessionMemory supplies the exact empty injection owner required by the refresh boundary.
    // vmmSessionMemory 提供刷新边界所需的精确空注入所有者。
    let vmm_session_memory = test_vmm_session_memory(session_id);

    // refreshed must start from the retained checkpoint, never from a second UserPrompt append.
    // refreshed 必须从保留 checkpoint 启动，绝不能再次追加 UserPrompt。
    let refreshed = refreshed_execution_request_after_context_compaction(
        &shared_state,
        &request,
        &vmm_session_memory,
    )
    .expect("post-compaction request should refresh");

    assert_eq!(refreshed.conversation_state.messages.len(), 1);
    assert_eq!(
        refreshed.conversation_state.messages[0].id,
        "compacted-continuation-seed"
    );
    assert_eq!(
        refreshed.turn_start_mode,
        Some(
            vulcan_agent_runtime::agent_runtime::execution_models::ExecutionTurnStartMode::RestoredCheckpoint,
        )
    );
}

/// Verifies a style override change invalidates an in-flight compaction draft.
/// 验证压缩进行中修改输出风格覆盖会使旧候选失效。
#[test]
fn compaction_baseline_rejects_concurrent_output_style_override_change() {
    let mut runtime_state = SessionRuntimeStateEntry::default();
    runtime_state.output_style.override_style_id = Some(String::from("structured"));
    let baseline = WebuiContextCompactionBaseline::capture(&runtime_state);

    runtime_state.output_style.override_style_id = Some(String::from("rigorous"));

    assert!(!baseline.matches(&runtime_state));
}

/// Verifies route or budget changes invalidate an in-flight compaction draft without new messages.
/// 验证路由或预算在没有新消息时发生变化也会使进行中的压缩候选失效。
#[test]
fn compaction_baseline_rejects_concurrent_route_and_budget_change() {
    let mut runtime_state = SessionRuntimeStateEntry {
        live_context_budget_fingerprint: Some(String::from("budget-a")),
        live_context_assembly_fingerprint: Some(String::from("assembly-a")),
        active_provider_id: Some(String::from("provider-a")),
        active_connection_id: Some(String::from("connection-a")),
        active_model_id: Some(String::from("model-a")),
        ..SessionRuntimeStateEntry::default()
    };
    let budget_baseline = WebuiContextCompactionBaseline::capture(&runtime_state);

    runtime_state.live_context_budget_fingerprint = Some(String::from("budget-b"));
    assert!(!budget_baseline.matches(&runtime_state));

    runtime_state.live_context_budget_fingerprint = Some(String::from("budget-a"));
    let route_baseline = WebuiContextCompactionBaseline::capture(&runtime_state);
    runtime_state.active_model_id = Some(String::from("model-b"));
    assert!(!route_baseline.matches(&runtime_state));
}

/// Removes the variable last-user payload from an automatic compaction continuation prompt.
/// 从自动压缩续接提示中移除可变的最后用户消息载荷。
fn auto_compaction_continuation_shell(prompt: &str) -> String {
    let Some((before_user, after_start)) = prompt.split_once("<last_user_message>") else {
        return prompt.to_string();
    };
    let Some((_, after_user)) = after_start.split_once("</last_user_message>") else {
        return prompt.to_string();
    };
    format!("{before_user}<last_user_message></last_user_message>{after_user}")
}

/// Verifies compaction draft creation prunes expired transient injections without mutating active state.
/// 验证压缩候选状态会裁剪过期临时注入且不会修改活跃状态。
#[test]
fn webui_context_compaction_draft_prunes_expired_transient_injections_transactionally() {
    let session_id = SessionId::new_v4();
    let messages = vec![
        SessionMessageEntry::new(SessionMessageKind::User, "用户指令", "请实现上下文压缩")
            .with_runtime_metadata("user-1", "1780000000000"),
        SessionMessageEntry::new(SessionMessageKind::Assistant, "助手", "已开始实现")
            .with_runtime_metadata("assistant-1", "1780000000001"),
    ];
    let mut active_runtime_state = SessionRuntimeStateEntry {
        conversation_state: conversation_state_from_session_messages(&messages)
            .expect("valid replayed conversation identities"),
        live_context_budget_fingerprint: Some(String::from("budget-live")),
        live_context_assembly_fingerprint: Some(String::from("assembly-live")),
        ..SessionRuntimeStateEntry::default()
    };
    active_runtime_state.context_injection_state.apply_insert(
        InjectionInsert::single(
            InjectionKind::Transient,
            "vmm_inject",
            "Transient memory",
            "expired transient memory",
            Some(1),
        ),
        "turn-1",
    );
    let expired_item = active_runtime_state
        .context_injection_state
        .groups
        .get_mut("vmm_inject")
        .and_then(|group| group.items.values_mut().next())
        .expect("expired transient injection item should exist");
    expired_item.remaining_turns = Some(0);
    record_context_compaction_provider_call(&mut active_runtime_state, "test-compaction-execution");
    let policy = vulcan_context::ContextCompactionPolicy::default();
    let plan = CompactionPlan {
        source_messages: vec![messages[0].clone()],
        source_until_message_id: Some(String::from("user-1")),
        source_until_turn_index: Some(0),
        retained_tail_start_turn_index: Some(1),
        compaction_id: String::from("test-compaction"),
        created_at: String::from("1780000000100"),
        policy,
        tail_selection: test_tail_selection(policy, &["assistant-1"]),
    };
    let execution = PromptCompactionExecution {
        compaction_result: vulcan_context_compaction::CompactionResult::from_model_summary(
            &plan,
            "## Goal\n- 保留上下文压缩升级目标。",
        ),
        fallback_used: false,
        provider_request_count: 1,
        last_successful_provider_usage: None,
        provider_id: None,
        model_id: None,
        execution_profile: PromptCompactionExecutionProfile::CachePreservingFullFidelity,
        input_prune_report: None,
    };

    // vmm_session_memory models the isolated Core controller candidate for this compaction.
    // vmm_session_memory 模拟本次压缩使用的独立 Core 控制器候选状态。
    let mut vmm_session_memory = test_vmm_session_memory(session_id);
    let draft = build_webui_context_compaction_draft(
        &active_runtime_state,
        &mut vmm_session_memory,
        session_id,
        "section-test",
        "预算快照",
        MANUAL_CONTEXT_COMPACTION_TRIGGER,
        1,
        "## Goal\n- 保留上下文压缩升级目标。",
        &execution,
        PersistedPromptOutputStyleSelection::default(),
    )
    .expect("draft should be built from a valid summary");

    assert_eq!(draft.ttl_prune_report.removed_item_count, 1);
    assert!(
        active_runtime_state
            .context_injection_state
            .groups
            .contains_key("vmm_inject"),
        "active runtime state must remain untouched"
    );
    assert!(
        !draft
            .runtime_state
            .context_injection_state
            .groups
            .contains_key("vmm_inject"),
        "draft runtime state should remove expired transient injections"
    );
    assert!(active_runtime_state.context_start_message_id.is_none());
    assert!(draft.runtime_state.context_start_message_id.is_some());
    assert_eq!(
        draft.checkpoint.context_budget_fingerprint.as_deref(),
        Some("budget-live")
    );
}

/// Verifies a compaction draft built after a concurrent picker action preserves both pending authority and its audit record.
/// 验证在并发模型选择操作后构建的压缩候选会同时保留待生效权威及其审计记录。
#[test]
fn webui_context_compaction_draft_preserves_pending_model_switch_audit() {
    // sessionId identifies the exact runtime row participating in the compaction transaction.
    // sessionId 标识参与压缩事务的精确运行态行。
    let session_id = SessionId::new_v4();
    // modelSwitchEntry models the display-only record appended while summary generation is in flight.
    // modelSwitchEntry 模拟摘要生成期间追加的仅展示记录。
    let mut model_switch_entry = SessionMessageEntry::new(
        SessionMessageKind::Assistant,
        "",
        "provider_id=command-code\nconnection_id=command-code-account\nmodel_id=deepseek",
    )
    .with_runtime_metadata("model-switch-audit", "1780000000002");
    model_switch_entry.record_kind = SessionMessageRecordKind::DisplayNote;
    model_switch_entry.visible = true;
    model_switch_entry.metadata = Some(json!({
        "notice_kind": "model_switch",
        "activation": "next_submission",
        "provider_id": "command-code",
        "provider_name": "Command Code",
        "connection_id": "command-code-account",
        "connection_name": "Command Code Account",
        "model_id": "deepseek",
        "model_name": "DeepSeek",
    }));
    // messages contain the provider-visible history plus the concurrent display-only audit row.
    // messages 包含 Provider 可见历史以及并发写入的仅展示审计行。
    let messages = vec![
        SessionMessageEntry::new(SessionMessageKind::User, "用户指令", "请实现上下文压缩")
            .with_runtime_metadata("user-1", "1780000000000"),
        SessionMessageEntry::new(SessionMessageKind::Assistant, "助手", "已开始实现")
            .with_runtime_metadata("assistant-1", "1780000000001"),
        model_switch_entry,
    ];
    // pendingSelection is the future route selected by the user while the active route remains frozen.
    // pendingSelection 是用户选择的未来路由，而当前活动路由仍保持冻结。
    let pending_selection = vulcan_workbench_model::SessionPendingModelSelection {
        provider_id: String::from("command-code"),
        connection_id: String::from("command-code-account"),
        model_id: String::from("deepseek"),
    };
    // activeRuntimeState is the lock-current state from which the compaction draft must clone.
    // activeRuntimeState 是压缩候选必须克隆的锁内最新状态。
    let mut active_runtime_state = SessionRuntimeStateEntry {
        conversation_state: conversation_state_from_session_messages(&messages)
            .expect("valid replayed conversation identities"),
        pending_model_selection: Some(pending_selection.clone()),
        live_context_budget_fingerprint: Some(String::from("budget-live")),
        live_context_assembly_fingerprint: Some(String::from("assembly-live")),
        ..SessionRuntimeStateEntry::default()
    };
    record_context_compaction_provider_call(
        &mut active_runtime_state,
        "test-compaction-pending-model-switch",
    );
    // policy is shared by the frozen plan and its compaction result.
    // policy 由冻结计划及其压缩结果共同使用。
    let policy = vulcan_context::ContextCompactionPolicy::default();
    // plan summarizes only the older user row and retains the assistant suffix.
    // plan 仅摘要较早的用户行，并保留 Assistant 后缀。
    let plan = CompactionPlan {
        source_messages: vec![messages[0].clone()],
        source_until_message_id: Some(String::from("user-1")),
        source_until_turn_index: Some(0),
        retained_tail_start_turn_index: Some(1),
        compaction_id: String::from("test-compaction-pending-model-switch"),
        created_at: String::from("1780000000100"),
        policy,
        tail_selection: test_tail_selection(policy, &["assistant-1"]),
    };
    // execution carries the immutable model-produced summary used by the draft builder.
    // execution 携带候选构建器使用的不可变模型摘要。
    let execution = PromptCompactionExecution {
        compaction_result: vulcan_context_compaction::CompactionResult::from_model_summary(
            &plan,
            "## Goal\n- 保留上下文压缩升级目标。",
        ),
        fallback_used: false,
        provider_request_count: 1,
        last_successful_provider_usage: None,
        provider_id: None,
        model_id: None,
        execution_profile: PromptCompactionExecutionProfile::CachePreservingFullFidelity,
        input_prune_report: None,
    };
    // vmmSessionMemory models the isolated controller candidate committed with the same draft.
    // vmmSessionMemory 模拟与同一候选共同提交的独立控制器状态。
    let mut vmm_session_memory = test_vmm_session_memory(session_id);
    // draft is built from the latest serialized runtime state, including the picker mutation.
    // draft 基于包含模型选择变更的最新串行运行态构建。
    let draft = build_webui_context_compaction_draft(
        &active_runtime_state,
        &mut vmm_session_memory,
        session_id,
        "section-pending-model-switch",
        "预算快照",
        MANUAL_CONTEXT_COMPACTION_TRIGGER,
        1,
        "## Goal\n- 保留上下文压缩升级目标。",
        &execution,
        PersistedPromptOutputStyleSelection::default(),
    )
    .expect("draft should preserve the pending model switch");
    // retainedAudit is the exact localized-display source after record compaction.
    // retainedAudit 是记录压缩后用于本地化展示的精确来源。
    let retained_audit = draft
        .runtime_state
        .conversation_state
        .records
        .iter()
        .find(|record| record.id == "model-switch-audit")
        .expect("model-switch audit must remain visible after compaction");

    assert_eq!(
        draft.runtime_state.pending_model_selection,
        Some(pending_selection)
    );
    assert_eq!(
        retained_audit
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("notice_kind"))
            .and_then(Value::as_str),
        Some("model_switch")
    );
    assert_eq!(
        retained_audit
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("provider_name"))
            .and_then(Value::as_str),
        Some("Command Code")
    );
}

/// Verifies completed compaction markers are not anchored back into older visible turns.
/// 验证已完成压缩标记不会被锚回更早的可见轮次。
#[test]
fn webui_context_compaction_draft_keeps_status_markers_unanchored() {
    let session_id = SessionId::new_v4();
    let section_id = String::from("section-test");
    let section = SessionMessageEntry {
        id: section_id.clone(),
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::CompactionStatus,
        visible: true,
        title: String::from("上下文正在压缩"),
        body: String::new(),
        metadata: Some(json!({
            "context_compaction_status": true,
            "context_compaction_section_id": section_id.clone(),
            "context_compaction_role": "start",
            "context_compaction_state": "running",
        })),
        created_at: String::from("1780000000002"),
    };
    let messages = vec![
        SessionMessageEntry::new(SessionMessageKind::User, "用户", "压缩前问题")
            .with_runtime_metadata("user-before", "1780000000000"),
        SessionMessageEntry::new(SessionMessageKind::Assistant, "助手", "压缩前回答")
            .with_runtime_metadata("assistant-before", "1780000000001"),
        section,
        SessionMessageEntry::new(SessionMessageKind::User, "用户", "压缩后继续提问")
            .with_runtime_metadata("user-after", "1780000000003"),
    ];
    let mut active_runtime_state = SessionRuntimeStateEntry {
        conversation_state: conversation_state_from_session_messages(&messages)
            .expect("valid replayed conversation identities"),
        live_context_budget_fingerprint: Some(String::from("budget-live")),
        live_context_assembly_fingerprint: Some(String::from("assembly-live")),
        ..SessionRuntimeStateEntry::default()
    };
    record_context_compaction_provider_call(
        &mut active_runtime_state,
        "test-compaction-unanchored-execution",
    );
    let policy = vulcan_context::ContextCompactionPolicy::default();
    let plan = CompactionPlan {
        source_messages: vec![messages[0].clone()],
        source_until_message_id: Some(String::from("user-before")),
        source_until_turn_index: Some(0),
        retained_tail_start_turn_index: Some(1),
        compaction_id: String::from("test-compaction-unanchored"),
        created_at: String::from("1780000000100"),
        policy,
        tail_selection: test_tail_selection(policy, &["assistant-before", "user-after"]),
    };
    let execution = PromptCompactionExecution {
        compaction_result: vulcan_context_compaction::CompactionResult::from_model_summary(
            &plan,
            "## Goal\n- 保留上下文压缩升级目标。",
        ),
        fallback_used: false,
        provider_request_count: 1,
        last_successful_provider_usage: None,
        provider_id: None,
        model_id: None,
        execution_profile: PromptCompactionExecutionProfile::CachePreservingFullFidelity,
        input_prune_report: None,
    };

    // vmm_session_memory models the isolated Core controller candidate for this compaction.
    // vmm_session_memory 模拟本次压缩使用的独立 Core 控制器候选状态。
    let mut vmm_session_memory = test_vmm_session_memory(session_id);
    let draft = build_webui_context_compaction_draft(
        &active_runtime_state,
        &mut vmm_session_memory,
        session_id,
        &section_id,
        "预算快照",
        MANUAL_CONTEXT_COMPACTION_TRIGGER,
        1,
        "## Goal\n- 保留上下文压缩升级目标。",
        &execution,
        PersistedPromptOutputStyleSelection::default(),
    )
    .expect("draft should be built from a valid summary");

    let section_records = draft
        .runtime_state
        .conversation_state
        .records
        .iter()
        .filter(|record| {
            record.id == section_id
                || record
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("context_compaction_section_id"))
                    .and_then(Value::as_str)
                    == Some(section_id.as_str())
        })
        .collect::<Vec<_>>();
    assert!(
        section_records.len() >= 2,
        "completed compaction should keep visible section rows"
    );
    assert!(section_records.iter().any(|record| {
        record
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("context_compaction_role"))
            .and_then(Value::as_str)
            == Some("body")
            && record.body.contains("保留上下文压缩升级目标")
    }));
    assert!(section_records.iter().all(|record| {
        record
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("context_compaction_insert_before_message_id"))
            .is_none()
    }));
}

/// Verifies manual compaction owns draft statistics while automatic compaction owns continuation state.
/// 验证手动压缩在候选状态中持有统计记录，而自动压缩持有续接状态。
#[test]
fn webui_context_compaction_draft_separates_manual_stats_and_auto_continuation() {
    let session_id = SessionId::new_v4();
    let messages = vec![
        SessionMessageEntry::new(SessionMessageKind::User, "User", "Old question")
            .with_runtime_metadata("user-old", "1780000000000"),
        SessionMessageEntry::new(SessionMessageKind::Assistant, "Assistant", "Old answer")
            .with_runtime_metadata("assistant-old", "1780000000001"),
        SessionMessageEntry::new(
            SessionMessageKind::User,
            "User",
            "Please continue the real task",
        )
        .with_runtime_metadata("user-latest", "1780000000002"),
    ];
    let mut active_runtime_state = SessionRuntimeStateEntry {
        conversation_state: conversation_state_from_session_messages(&messages)
            .expect("valid replayed conversation identities"),
        live_context_budget_fingerprint: Some(String::from("budget-live")),
        live_context_assembly_fingerprint: Some(String::from("assembly-live")),
        ..SessionRuntimeStateEntry::default()
    };
    record_context_compaction_provider_call(
        &mut active_runtime_state,
        "test-compaction-manual-auto-execution",
    );
    // active_vmm_session_memory models the committed controller state before either draft.
    // active_vmm_session_memory 模拟两个候选构建前已经提交的控制器状态。
    let mut active_vmm_session_memory = test_vmm_session_memory(session_id);
    active_vmm_session_memory
        .apply_recall_batch(
            &session_id.to_string(),
            "user-old",
            &vulcan_vmm::VmmRecallBatch {
                ttl_turns: 3,
                injection_tag: String::from("VMM_MEMORY"),
                injection_description: String::from("Relevant memories."),
                items: vec![vulcan_vmm::ContextItem {
                    text: String::from("active-compaction-memory"),
                    memory_id: 77,
                    ..vulcan_vmm::ContextItem::default()
                }],
            },
            "1780000000003",
        )
        .expect("active VMM lease should be prepared before compaction");
    active_runtime_state
        .conversation_state
        .continuation
        .last_provider_message_id = Some(String::from("resp-before-compaction"));
    active_runtime_state
        .conversation_state
        .continuation
        .last_local_message_id = Some(String::from("assistant-before-compaction"));
    let policy = vulcan_context::ContextCompactionPolicy::default();
    let plan = CompactionPlan {
        source_messages: vec![messages[0].clone()],
        source_until_message_id: Some(String::from("user-old")),
        source_until_turn_index: Some(0),
        retained_tail_start_turn_index: Some(1),
        compaction_id: String::from("test-auto-continuation"),
        created_at: String::from("1780000000100"),
        policy,
        tail_selection: test_tail_selection(policy, &["assistant-old", "user-latest"]),
    };
    let manual_execution = PromptCompactionExecution {
        compaction_result: vulcan_context_compaction::CompactionResult::from_model_summary(
            &plan,
            "## Goal\n- Keep automatic compaction continuation semantics.",
        ),
        fallback_used: false,
        provider_request_count: 1,
        last_successful_provider_usage: None,
        provider_id: None,
        model_id: None,
        execution_profile: PromptCompactionExecutionProfile::CachePreservingFullFidelity,
        input_prune_report: None,
    };
    // manual_vmm_session_memory isolates manual candidate mutation from the committed state.
    // manual_vmm_session_memory 将手动压缩候选变更与已提交状态隔离。
    let mut manual_vmm_session_memory = active_vmm_session_memory.clone();
    let manual_draft = build_webui_context_compaction_draft(
        &active_runtime_state,
        &mut manual_vmm_session_memory,
        session_id,
        "section-manual",
        "Budget snapshot",
        MANUAL_CONTEXT_COMPACTION_TRIGGER,
        1,
        "## Goal\n- Keep automatic compaction continuation semantics.",
        &manual_execution,
        PersistedPromptOutputStyleSelection::default(),
    )
    .expect("manual draft should be built from a valid summary");
    assert!(
        !manual_draft
            .runtime_state
            .conversation_state
            .messages
            .iter()
            .any(|message| matches!(
                &message.payload,
                SessionConversationPayload::UserText { text, .. }
                    if text.starts_with(AUTO_COMPACTION_CONTINUATION_PREFIX)
            )),
        "manual compaction must not append the automatic continuation note"
    );
    assert_eq!(
        manual_vmm_session_memory.cache_epoch.epoch_id,
        "test-auto-continuation"
    );
    assert!(manual_vmm_session_memory.cache_epoch.anchors.is_empty());
    assert!(manual_vmm_session_memory.cache_epoch.needs_active_baseline);
    assert!(
        manual_draft
            .runtime_state
            .conversation_state
            .continuation
            .last_provider_message_id
            .is_none()
    );
    assert!(
        manual_draft
            .runtime_state
            .conversation_state
            .continuation
            .last_local_message_id
            .is_none()
    );
    assert_eq!(
        active_vmm_session_memory.cache_epoch.anchors.len(),
        1,
        "draft construction must not mutate the active VMM epoch"
    );
    // manual_turn_stats is materialized inside the draft before any active-state replacement occurs.
    // manual_turn_stats 在替换任何活动状态前已物化于候选状态内部。
    let manual_turn_stats = manual_draft
        .runtime_state
        .conversation_state
        .records
        .iter()
        .find(|record| record.record_kind == SessionMessageRecordKind::TurnStats)
        .and_then(|record| record.metadata.as_ref())
        .and_then(|metadata| metadata.get("turn_stats"))
        .cloned()
        .and_then(|value| serde_json::from_value::<SessionTurnStats>(value).ok())
        .expect("manual compaction draft should contain canonical turn statistics");
    assert_eq!(
        manual_turn_stats.turn_kind,
        SessionTurnStatsKind::ContextCompaction
    );
    assert_eq!(manual_turn_stats.provider_call_count, 1);
    assert_eq!(manual_turn_stats.usage_segments.len(), 1);
    assert_eq!(
        manual_turn_stats.usage_segments[0].kind,
        SessionTurnUsageSegmentKind::ContextCompaction
    );

    let auto_execution = PromptCompactionExecution {
        compaction_result: vulcan_context_compaction::CompactionResult::from_model_summary(
            &plan,
            "## Goal\n- Keep automatic compaction continuation semantics.",
        ),
        fallback_used: false,
        provider_request_count: 1,
        last_successful_provider_usage: None,
        provider_id: None,
        model_id: None,
        execution_profile: PromptCompactionExecutionProfile::CachePreservingFullFidelity,
        input_prune_report: None,
    };
    // auto_vmm_session_memory isolates automatic candidate mutation from the committed state.
    // auto_vmm_session_memory 将自动压缩候选变更与已提交状态隔离。
    let mut auto_vmm_session_memory = active_vmm_session_memory.clone();
    let auto_draft = build_webui_context_compaction_draft(
        &active_runtime_state,
        &mut auto_vmm_session_memory,
        session_id,
        "section-auto",
        "Budget snapshot",
        AUTO_CONTEXT_COMPACTION_TRIGGER,
        1,
        "## Goal\n- Keep automatic compaction continuation semantics.",
        &auto_execution,
        PersistedPromptOutputStyleSelection::default(),
    )
    .expect("automatic draft should be built from a valid summary");
    assert!(
        auto_draft
            .runtime_state
            .conversation_state
            .records
            .iter()
            .all(|record| record.record_kind != SessionMessageRecordKind::TurnStats),
        "automatic compaction must not append an independent turn statistics record"
    );
    let continuation_messages = auto_draft
        .runtime_state
        .conversation_state
        .messages
        .iter()
        .filter_map(|message| {
            let SessionConversationPayload::UserText { text, .. } = &message.payload else {
                return None;
            };
            text.starts_with(AUTO_COMPACTION_CONTINUATION_PREFIX)
                .then_some(text.as_str())
        })
        .collect::<Vec<_>>();
    assert_eq!(continuation_messages.len(), 1);
    assert!(continuation_messages[0].contains("synthetic continuation prompt"));
    assert!(continuation_messages[0].contains("resumed provider request"));
    assert!(continuation_messages[0].contains("Do not greet"));
    assert!(continuation_messages[0].contains("Please continue the real task"));
    assert!(
        !contains_cjk_text(continuation_messages[0]),
        "model-visible automatic compaction continuation message must remain fixed English in this host-generated test path"
    );
    assert!(
        !contains_cjk_text(&auto_compaction_continuation_shell(
            continuation_messages[0]
        )),
        "model-visible automatic compaction continuation shell must remain fixed English"
    );
    assert!(
        auto_draft
            .runtime_state
            .conversation_state
            .records
            .iter()
            .all(|record| !record.body.contains(AUTO_COMPACTION_CONTINUATION_PREFIX)),
        "automatic continuation is model-visible runtime state, not a visible transcript record"
    );
    // The synthetic continuation id is the only legal target for the automatic VMM baseline.
    // 合成续接消息标识是自动 VMM 基线唯一合法的目标。
    let synthetic_continuation_id = auto_draft
        .runtime_state
        .conversation_state
        .messages
        .iter()
        .find_map(|message| match &message.payload {
            SessionConversationPayload::UserText { text, .. }
                if text.starts_with(AUTO_COMPACTION_CONTINUATION_PREFIX) =>
            {
                Some(message.id.as_str())
            }
            _ => None,
        })
        .expect("automatic compaction should append one synthetic continuation id");
    let baseline_anchor = auto_vmm_session_memory
        .cache_epoch
        .anchors
        .first()
        .expect("automatic compaction should bind one active VMM baseline");
    assert_eq!(
        baseline_anchor.kind,
        vulcan_vmm::VmmPromptAnchorKind::Baseline
    );
    assert_eq!(baseline_anchor.target_message_id, synthetic_continuation_id);
    assert!(baseline_anchor.prefix.contains("active-compaction-memory"));
    assert!(!auto_vmm_session_memory.cache_epoch.needs_active_baseline);
}

/// Verifies repeated automatic compaction ignores previous continuation notes when finding the last user prompt.
/// 验证重复自动压缩查找最后用户提示时会忽略旧续接提示。
#[test]
fn last_runtime_user_message_text_skips_previous_auto_continuation() {
    let state = SessionConversationState {
        messages: vec![
            SessionConversationMessage {
                id: String::from("real-user"),
                payload: SessionConversationPayload::UserText {
                    text: String::from("真正的最后用户请求"),
                    attachments: Vec::new(),
                },
            },
            SessionConversationMessage {
                id: String::from("old-auto-continuation"),
                payload: SessionConversationPayload::UserText {
                    text: format!("{AUTO_COMPACTION_CONTINUATION_PREFIX} stale note"),
                    attachments: Vec::new(),
                },
            },
        ],
        ..SessionConversationState::default()
    };

    assert_eq!(
        last_runtime_user_message_text(&state).as_deref(),
        Some("真正的最后用户请求")
    );
}

/// Verifies context compaction persistence keeps the complete visible transcript window.
/// 验证上下文压缩持久化会保留完整可见转录窗口。
#[test]
fn context_compaction_complete_display_messages_preserve_visible_history() {
    let session_id = SessionId::new_v4();
    let section_id = String::from("compaction-section-1");
    let mut user_entry =
        SessionMessageEntry::new(SessionMessageKind::User, "用户指令", "循环调用工具");
    user_entry.id = String::from("user-1");
    let mut tool_entry =
        SessionMessageEntry::new(SessionMessageKind::Assistant, "", "已执行 sleep 5");
    tool_entry.id = String::from("tool-1");
    tool_entry.record_kind = SessionMessageRecordKind::ToolCallOutput;
    let mut interrupt_entry =
        SessionMessageEntry::new(SessionMessageKind::User, "用户指令", "不用继续了");
    interrupt_entry.id = String::from("interrupt-1");
    interrupt_entry.metadata = Some(json!({
        "source": "user_interrupt",
        "starts_user_turn": false,
        "turn_process": true,
        "editable": false,
    }));
    let mut compaction_entry =
        SessionMessageEntry::new(SessionMessageKind::Assistant, "上下文已压缩", "");
    compaction_entry.id = section_id.clone();
    compaction_entry.record_kind = SessionMessageRecordKind::CompactionStatus;
    let workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: Vec::new(),
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::from([(
            session_id,
            vec![
                user_entry.clone(),
                tool_entry.clone(),
                interrupt_entry.clone(),
                compaction_entry.clone(),
            ],
        )]),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: session_id,
        hydrated_session_ids: BTreeSet::from([session_id]),
    };

    let messages =
        context_compaction_complete_display_messages(&workspace_snapshot, session_id, &section_id);

    assert_eq!(messages.len(), 4);
    assert!(messages.iter().any(|message| message.id == tool_entry.id));
    assert!(
        messages
            .iter()
            .any(|message| message.id == interrupt_entry.id)
    );
    assert!(
        messages
            .iter()
            .any(|message| message.id == compaction_entry.id)
    );
}

/// Verifies compaction terminal records preserve clone-prune report metadata.
/// 验证压缩终态记录会保留克隆剪枝报告元数据。
#[test]
fn append_webui_context_compaction_end_message_preserves_input_prune_report() {
    let session_id = SessionId::new_v4();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    let prune_report = CompactionInputPruneReport {
        mode: CompactionInputPruneMode::RequiredForWindowDowngrade,
        target_input_budget_tokens: 1024,
        safety_reserve_tokens: 128,
        original_token_estimate: 4096,
        pruned_token_estimate: 900,
        pruned_tool_output_count: 2,
        preserved_tool_output_count: 3,
        dropped_source_message_count: 4,
        truncated_source_message_count: 5,
        still_exceeds_budget: false,
    };
    // tailSelection supplies persisted adaptive-policy diagnostics for the completed attempt.
    // tailSelection 为已完成尝试提供持久化自适应策略诊断。
    let policy = vulcan_context::ContextCompactionPolicy::default();
    let tail_selection = test_tail_selection(policy, &["assistant-retained"]);
    // recordReport supplies exact record-retention counts for the same candidate transaction.
    // recordReport 为同一候选事务提供精确记录保留计数。
    let record_report = ConversationRecordCompactionReport {
        original_record_count: 12,
        full_record_count: 4,
        compacted_historical_record_count: 3,
        deleted_record_count: 5,
        final_record_count: 7,
        unmatched_retained_tool_identity_count: 0,
    };

    let record_id = append_webui_context_compaction_end_message_to_state(
        &mut runtime_state,
        session_id,
        "section-a",
        "预算说明",
        AUTO_CONTEXT_COMPACTION_TRIGGER,
        7,
        Some(&prune_report),
        &tail_selection,
        &record_report,
    )
    .expect("valid compaction terminal record identity");

    let record = runtime_state
        .conversation_state
        .records
        .iter()
        .find(|record| record.id == record_id)
        .expect("terminal record should be appended");
    let metadata = record
        .metadata
        .as_ref()
        .expect("terminal record should carry metadata");
    assert_eq!(
        metadata
            .get("context_compaction_input_prune_report")
            .and_then(|value| value.get("mode"))
            .and_then(Value::as_str),
        Some("required_for_window_downgrade")
    );
    assert_eq!(
        metadata
            .get("context_compaction_input_prune_report")
            .and_then(|value| value.get("pruned_tool_output_count"))
            .and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        metadata
            .get("context_compaction_provider_request_count")
            .and_then(Value::as_u64),
        Some(7)
    );
    assert_eq!(
        metadata
            .get("context_compaction_trigger")
            .and_then(Value::as_str),
        Some(AUTO_CONTEXT_COMPACTION_TRIGGER)
    );
    assert_eq!(
        metadata
            .pointer("/context_compaction_tail_selection/policy_source")
            .and_then(Value::as_str),
        Some("missing_context_limit_minimum")
    );
    assert_eq!(
        metadata
            .pointer("/context_compaction_tail_selection/retained_message_count")
            .and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        metadata
            .pointer("/context_compaction_record_retention/deleted_record_count")
            .and_then(Value::as_u64),
        Some(5)
    );
}

/// Verifies failed auto-compaction records failure metadata and circuit-breaker progress.
/// 验证自动压缩失败会记录失败元数据并推进熔断计数。
#[test]
fn fail_webui_context_compaction_execution_records_auto_failure_metadata() {
    let session_id = SessionId::new_v4();
    let mut runtime_state = SessionRuntimeStateEntry {
        auto_compaction_consecutive_failures: 2,
        context_start_message_id: Some(String::from("existing-anchor")),
        ..SessionRuntimeStateEntry::default()
    };
    let mut workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            session_id,
            Some(String::from("project-a")),
            "压缩测试会话",
            "",
            SessionState::Running,
            0,
            0,
            "刚刚",
        )],
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::new(),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::from([(session_id, SessionRunPhase::Preparing)]),
        expanded_task_projects: BTreeSet::new(),
        selected_session: session_id,
        hydrated_session_ids: BTreeSet::new(),
    };
    let section_id = append_webui_context_compaction_status_message_to_workspace(
        &mut workspace_snapshot,
        session_id,
        "预算说明",
        AUTO_CONTEXT_COMPACTION_TRIGGER,
    )
    .expect("valid compaction status record identity");
    runtime_state.conversation_state = workspace_snapshot
        .session_runtime_state
        .remove(&session_id)
        .expect("status append should create runtime state")
        .conversation_state;
    workspace_snapshot
        .session_runtime_state
        .insert(session_id, runtime_state);
    apply_webui_context_compaction_failure_to_workspace(
        &mut workspace_snapshot,
        session_id,
        SessionRunPhase::Ready,
        &section_id,
        "预算说明",
        AUTO_CONTEXT_COMPACTION_TRIGGER,
        "测试失败",
    )
    .expect("valid compaction failure record identity");

    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .expect("runtime state should remain present");
    assert_eq!(runtime_state.auto_compaction_consecutive_failures, 3);
    assert_eq!(
        runtime_state.context_start_message_id.as_deref(),
        Some("existing-anchor")
    );
    assert_eq!(
        workspace_snapshot.session_run_phases.get(&session_id),
        Some(&SessionRunPhase::Ready)
    );
    assert!(
        workspace_snapshot
            .sessions
            .iter()
            .any(|session| session.id == session_id && session.status == SessionState::Completed)
    );
    let failed_record = runtime_state
        .conversation_state
        .records
        .iter()
        .find(|record| {
            record
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("context_compaction_role"))
                .and_then(Value::as_str)
                == Some("end")
        })
        .expect("failed terminal record should be appended");
    let metadata = failed_record
        .metadata
        .as_ref()
        .expect("failed terminal record should carry metadata");
    assert_eq!(
        metadata
            .get("context_compaction_state")
            .and_then(Value::as_str),
        Some("failed")
    );
    assert_eq!(
        metadata
            .get("context_compaction_trigger")
            .and_then(Value::as_str),
        Some(AUTO_CONTEXT_COMPACTION_TRIGGER)
    );
    assert_eq!(
        metadata
            .get("context_compaction_error")
            .and_then(Value::as_str),
        Some("测试失败")
    );
}
