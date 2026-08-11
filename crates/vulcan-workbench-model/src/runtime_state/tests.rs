use super::*;

/// Builds one minimal visible pending-prompt panel entry for order tests.
/// 为顺序测试构建一条最小可见待发送提示词面板项。
fn test_panel_entry(id: &str) -> PendingPromptPanelEntry {
    PendingPromptPanelEntry {
        id: id.to_string(),
        prompt: format!("prompt-{id}"),
        hidden_model_guidance: None,
        preview: format!("prompt-{id}"),
        attachments: Vec::new(),
        model_reference: None,
        runtime_options: PendingPromptRuntimeOptions::default(),
        state: PendingPromptPanelEntryState::Ready,
        created_at: String::from("1000"),
    }
}

/// Builds one minimal internal pending prompt entry for queue-drain tests.
/// 为队列取出测试构建一条最小内部待发送提示词项。
fn test_internal_prompt_entry(id: &str, panel_entry_id: Option<&str>) -> PendingSessionPromptEntry {
    PendingSessionPromptEntry {
        id: id.to_string(),
        prompt: format!("prompt-{id}"),
        hidden_model_guidance: None,
        preview: format!("prompt-{id}"),
        attachments: Vec::new(),
        model_reference: None,
        runtime_options: PendingPromptRuntimeOptions::default(),
        source: PendingSessionPromptSource::UserFollowup,
        task_id: None,
        panel_entry_id: panel_entry_id.map(str::to_string),
        created_at: String::from("1000"),
    }
}

/// Returns the visible pending-prompt panel queue ids in order.
/// 按顺序返回可见待发送提示词面板队列标识。
fn panel_entry_ids(ui_state: &SessionTranscriptUiState) -> Vec<String> {
    ui_state
        .pending_prompt_panel_queue
        .iter()
        .map(|entry| entry.id.clone())
        .collect()
}

/// Verifies lightweight persisted runtime state can omit canonical conversation history.
/// 验证轻量持久化运行态可以省略规范会话历史。
#[test]
fn runtime_state_deserializes_without_conversation_state() {
    let mut value = serde_json::to_value(SessionRuntimeStateEntry::default())
        .expect("default runtime state should serialize");
    value
        .as_object_mut()
        .expect("runtime state should serialize as object")
        .remove("conversation_state");

    let restored: SessionRuntimeStateEntry =
        serde_json::from_value::<SessionRuntimeStateEntry>(value)
            .expect("runtime state should restore without conversation_state");

    assert!(session_conversation_state_is_empty(
        &restored.conversation_state
    ));
}

/// Verifies the real v1 duplicate layout merges sparse top-level usage into richer ledger evidence.
/// 验证真实 v1 重复布局会把稀疏顶层用量兼容合并到更丰富的账本证据中。
#[test]
fn runtime_state_migrates_compatible_duplicate_legacy_usage() {
    let rich_usage = ProviderUsageSnapshot {
        input_tokens: Some(9_000),
        uncached_input_tokens: Some(8_243),
        output_tokens: Some(100),
        total_tokens: Some(9_100),
        ..ProviderUsageSnapshot::default()
    };
    let sparse_usage = ProviderUsageSnapshot {
        input_tokens: Some(9_000),
        output_tokens: Some(100),
        total_tokens: Some(9_100),
        ..ProviderUsageSnapshot::default()
    };
    let mut value = serde_json::to_value(SessionRuntimeStateEntry::default())
        .expect("default runtime state should serialize");
    let object = value
        .as_object_mut()
        .expect("runtime state should serialize as an object");
    object.insert(
        String::from("provider_usage"),
        serde_json::json!({
            "last_usage": rich_usage,
            "usage_revision": 1
        }),
    );
    object.insert(
        String::from("last_provider_usage"),
        serde_json::to_value(sparse_usage.clone()).expect("sparse usage should serialize"),
    );
    object.insert(
        String::from("last_successful_provider_usage"),
        serde_json::to_value(sparse_usage).expect("successful usage should serialize"),
    );
    let mut restored: SessionRuntimeStateEntry =
        serde_json::from_value(value).expect("legacy runtime state should deserialize");

    assert!(
        restored
            .migrate_legacy_provider_usage_summaries()
            .expect("compatible duplicate usage should migrate")
    );
    assert!(restored.last_provider_usage.is_none());
    assert!(restored.last_successful_provider_usage.is_none());
    let legacy_import = restored
        .provider_call_ledger
        .legacy_import
        .as_ref()
        .expect("legacy import evidence should remain available");
    assert_eq!(
        legacy_import
            .last_usage
            .as_ref()
            .and_then(|usage| usage.uncached_input_tokens),
        Some(8_243)
    );
    assert_eq!(
        legacy_import
            .last_successful_usage
            .as_ref()
            .and_then(|usage| usage.input_tokens),
        Some(9_000)
    );
}

/// Verifies ordinary runtime-state documents have no VMM ownership field.
/// 验证普通运行态文档不存在 VMM 所有权字段。
#[test]
fn runtime_state_has_no_vmm_session_memory_field() {
    // runtime_state represents the complete ordinary persistence model.
    // runtime_state 表示完整的普通持久化模型。
    let runtime_state = SessionRuntimeStateEntry::default();

    let value = serde_json::to_value(runtime_state).expect("runtime state should serialize");

    assert!(value.get("vmm_session_memory").is_none());
}

/// Verifies the persisted runtime state can carry the session kernel mailbox cursor.
/// 验证持久化运行态可以携带会话内核 mailbox 游标。
#[test]
fn runtime_state_serializes_session_runtime_kernel() {
    let session_id = uuid::Uuid::new_v4();
    let runtime_state = SessionRuntimeStateEntry {
        session_runtime_kernel: Some(SessionRuntimeKernel::new(
            vulcan_session_core::SessionNodeId::from(session_id),
        )),
        ..SessionRuntimeStateEntry::default()
    };

    let value =
        serde_json::to_value(&runtime_state).expect("runtime state with kernel should serialize");
    let restored: SessionRuntimeStateEntry =
        serde_json::from_value(value).expect("runtime state with kernel should deserialize");

    assert!(restored.session_runtime_kernel.is_some());
}

#[test]
fn pending_prompt_panel_drop_target_matches_webui_before_semantics() {
    let mut ui_state = SessionTranscriptUiState {
        pending_prompt_panel_queue: vec![
            test_panel_entry("a"),
            test_panel_entry("b"),
            test_panel_entry("c"),
        ],
        ..SessionTranscriptUiState::default()
    };

    let changed = apply_pending_prompt_panel_drop_target(
        &mut ui_state,
        "a",
        &PendingPromptPanelDropTarget::Before(String::from("c")),
    );

    assert!(changed);
    assert_eq!(panel_entry_ids(&ui_state), vec!["b", "a", "c"]);
}

#[test]
fn pending_prompt_panel_drop_target_matches_webui_end_semantics() {
    let mut ui_state = SessionTranscriptUiState {
        pending_prompt_panel_queue: vec![
            test_panel_entry("a"),
            test_panel_entry("b"),
            test_panel_entry("c"),
        ],
        ..SessionTranscriptUiState::default()
    };

    let changed = apply_pending_prompt_panel_drop_target(
        &mut ui_state,
        "a",
        &PendingPromptPanelDropTarget::End,
    );

    assert!(changed);
    assert_eq!(panel_entry_ids(&ui_state), vec!["b", "c", "a"]);
}

/// Verifies visible queued prompts keep later submissions on the pending lane.
/// 验证可见排队提示词会让后续提交继续进入待发送通道。
#[test]
fn pending_prompt_lane_busy_includes_visible_panel_queue() {
    let ui_state = SessionTranscriptUiState {
        pending_prompt_panel_queue: vec![test_panel_entry("a")],
        ..SessionTranscriptUiState::default()
    };

    assert!(pending_prompt_lane_is_busy(
        &ui_state,
        PendingPromptBusyLaneContext {
            dispatch: PendingPromptDispatchContext {
                has_running_subagents: false,
                has_active_run: false,
                run_phase: None,
                has_active_compaction: false,
            },
            has_precheck_progress: false,
        },
    ));
}

/// Verifies active provider phases keep WebUI submissions on the queued lane even when the task handle has already settled.
/// 验证即使任务句柄已经稳定，活跃 Provider 阶段也会让 WebUI 提交继续进入排队通道。
#[test]
fn pending_prompt_lane_busy_includes_active_provider_phase() {
    for phase in [
        crate::SessionRunPhase::Requesting,
        crate::SessionRunPhase::Thinking,
        crate::SessionRunPhase::ToolRunning,
    ] {
        assert!(pending_prompt_lane_is_busy(
            &SessionTranscriptUiState::default(),
            PendingPromptBusyLaneContext {
                dispatch: PendingPromptDispatchContext {
                    has_running_subagents: false,
                    has_active_run: false,
                    run_phase: Some(phase),
                    has_active_compaction: false,
                },
                has_precheck_progress: false,
            },
        ));
        assert_eq!(
            pending_prompt_dispatch_block_reason(
                PendingPromptDispatchContext {
                    has_running_subagents: false,
                    has_active_run: false,
                    run_phase: Some(phase),
                    has_active_compaction: false,
                },
                false,
            ),
            Some(PendingPromptDispatchBlockReason::ActiveRun)
        );
    }
}

/// Verifies stale preparation without active precheck does not trap new prompts on the queue lane.
/// 验证没有活跃预检查的陈旧准备阶段不会把新提示词困在排队通道。
#[test]
fn pending_prompt_lane_busy_ignores_stale_preparing_without_precheck() {
    assert!(!pending_prompt_lane_is_busy(
        &SessionTranscriptUiState::default(),
        PendingPromptBusyLaneContext {
            dispatch: PendingPromptDispatchContext {
                has_running_subagents: false,
                has_active_run: false,
                run_phase: Some(crate::SessionRunPhase::Preparing),
                has_active_compaction: false,
            },
            has_precheck_progress: false,
        },
    ));
}

/// Verifies queued prompt dispatch treats running subagents as independent work across provider runtimes.
/// 验证待发送提示词分发在所有 Provider 运行时下都将运行中子代理视为独立工作。
#[test]
fn pending_prompt_dispatch_allows_running_subagents_for_all_runtimes() {
    assert_eq!(
        pending_prompt_dispatch_block_reason(
            PendingPromptDispatchContext {
                has_running_subagents: true,
                has_active_run: false,
                run_phase: None,
                has_active_compaction: false,
            },
            true,
        ),
        None
    );
    assert_eq!(
        pending_prompt_dispatch_block_reason(
            PendingPromptDispatchContext {
                has_running_subagents: false,
                has_active_run: false,
                run_phase: None,
                has_active_compaction: false,
            },
            true,
        ),
        None
    );
}

/// Verifies pending-prompt mailbox inputs are blocked only by an active parent run.
/// 验证待发送 mailbox 输入只会被活跃父会话运行阻塞。
#[test]
fn pending_prompt_dispatch_allows_session_mailbox_during_running_siblings() {
    assert_eq!(
        pending_prompt_dispatch_block_reason_for_source(
            PendingPromptDispatchContext {
                has_running_subagents: true,
                has_active_run: false,
                run_phase: None,
                has_active_compaction: false,
            },
            false,
            Some(PendingSessionPromptSource::SessionMailbox),
        ),
        None
    );
    assert_eq!(
        pending_prompt_dispatch_block_reason_for_source(
            PendingPromptDispatchContext {
                has_running_subagents: true,
                has_active_run: true,
                run_phase: None,
                has_active_compaction: false,
            },
            false,
            Some(PendingSessionPromptSource::SessionMailbox),
        ),
        Some(PendingPromptDispatchBlockReason::ActiveRun)
    );
}

/// Verifies a full internal queue rejects ordinary user follow-ups.
/// 验证满内部队列会拒绝普通用户跟进消息。
#[test]
fn internal_queue_enqueue_rejects_user_prompt_when_full() {
    let mut ui_state = SessionTranscriptUiState {
        pending_prompt_queue: (0..PENDING_PROMPT_QUEUE_LIMIT)
            .map(|index| test_internal_prompt_entry(&format!("user-{index}"), None))
            .collect(),
        ..SessionTranscriptUiState::default()
    };
    let user_prompt = test_internal_prompt_entry("extra-user", None);

    let error = enqueue_pending_prompt_internal_queue_entry(&mut ui_state, user_prompt, false)
        .expect_err("full queue should reject ordinary user prompts");

    assert_eq!(error, PendingPromptInternalQueueEnqueueError::QueueFull);
    assert_eq!(
        ui_state.pending_prompt_queue.len(),
        PENDING_PROMPT_QUEUE_LIMIT
    );
}

/// Verifies manually guided prompts preserve the user's click order.
/// 验证手动引导的提示词会保留用户的点击顺序。
#[test]
fn routed_guidance_prompts_keep_first_in_first_out_order() {
    let mut ui_state = SessionTranscriptUiState {
        pending_prompt_panel_queue: vec![test_panel_entry("a"), test_panel_entry("b")],
        ..SessionTranscriptUiState::default()
    };

    route_pending_prompt_panel_entry_into_internal_queue(
        &mut ui_state,
        "a",
        String::from("internal-a"),
        String::from("1001"),
        PendingSessionPromptSource::UserInterrupt,
    )
    .expect("route first guidance prompt");
    route_pending_prompt_panel_entry_into_internal_queue(
        &mut ui_state,
        "b",
        String::from("internal-b"),
        String::from("1002"),
        PendingSessionPromptSource::UserInterrupt,
    )
    .expect("route second guidance prompt");

    let first = take_next_internal_pending_prompt_entry(&mut ui_state, false)
        .expect("take first guided prompt");
    let second = take_next_internal_pending_prompt_entry(&mut ui_state, false)
        .expect("take second guided prompt");

    assert_eq!(first.panel_entry_id.as_deref(), Some("a"));
    assert_eq!(second.panel_entry_id.as_deref(), Some("b"));
    assert_eq!(first.source, PendingSessionPromptSource::UserInterrupt);
    assert_eq!(second.source, PendingSessionPromptSource::UserInterrupt);
}

/// Verifies runtime guidance drains consecutive inserted panel-linked prompts without consuming passive internal work.
/// 验证运行时引导会取出连续已插入的面板关联提示词，同时不消耗被动内部工作。
#[test]
fn take_guided_pending_prompt_entries_keeps_passive_internal_queue() {
    let mut ui_state = SessionTranscriptUiState {
        pending_prompt_panel_queue: vec![test_panel_entry("a"), test_panel_entry("b")],
        pending_prompt_queue: Vec::new(),
        ..SessionTranscriptUiState::default()
    };
    route_pending_prompt_panel_entry_into_internal_queue(
        &mut ui_state,
        "a",
        String::from("internal-a"),
        String::from("1001"),
        PendingSessionPromptSource::UserInterrupt,
    )
    .expect("route first guidance prompt");
    route_pending_prompt_panel_entry_into_internal_queue(
        &mut ui_state,
        "b",
        String::from("internal-b"),
        String::from("1002"),
        PendingSessionPromptSource::UserInterrupt,
    )
    .expect("route second guidance prompt");
    ui_state
        .pending_prompt_queue
        .push(test_internal_prompt_entry("passive", None));

    let guided_entries = take_guided_pending_prompt_entries(&mut ui_state, false);

    assert_eq!(
        guided_entries
            .iter()
            .filter_map(|entry| entry.panel_entry_id.as_deref())
            .collect::<Vec<_>>(),
        vec!["a", "b"]
    );
    assert_eq!(ui_state.pending_prompt_queue.len(), 1);
    assert_eq!(ui_state.pending_prompt_queue[0].id, "passive");
    assert!(ui_state.pending_prompt_panel_queue.is_empty());
}

/// Verifies runtime intervention injects user guidance without waiting for independent subagents.
/// 验证运行时干预会注入用户引导，而不等待独立子代理。
#[test]
fn runtime_intervention_guidance_ignores_running_subagents() {
    let mut ui_state = SessionTranscriptUiState {
        pending_prompt_panel_queue: vec![test_panel_entry("a")],
        ..SessionTranscriptUiState::default()
    };
    route_pending_prompt_panel_entry_into_internal_queue(
        &mut ui_state,
        "a",
        String::from("internal-a"),
        String::from("1001"),
        PendingSessionPromptSource::UserInterrupt,
    )
    .expect("route guidance prompt");
    let context = PendingPromptDispatchContext {
        has_running_subagents: true,
        has_active_run: true,
        run_phase: Some(crate::SessionRunPhase::ToolRunning),
        has_active_compaction: false,
    };

    let guided_entries =
        take_runtime_intervention_guided_pending_prompt_entries(&mut ui_state, context);

    assert_eq!(
        guided_entries
            .iter()
            .filter_map(|entry| entry.panel_entry_id.as_deref())
            .collect::<Vec<_>>(),
        vec!["a"]
    );
    assert!(ui_state.pending_prompt_queue.is_empty());
    assert!(ui_state.pending_prompt_panel_queue.is_empty());
}

/// Verifies runtime intervention still injects user guidance after delegated siblings have settled.
/// 验证委派兄弟任务收敛后，运行时干预仍会注入用户引导。
#[test]
fn runtime_intervention_guidance_runs_after_subagents_settle() {
    let mut ui_state = SessionTranscriptUiState {
        pending_prompt_panel_queue: vec![test_panel_entry("a")],
        ..SessionTranscriptUiState::default()
    };
    route_pending_prompt_panel_entry_into_internal_queue(
        &mut ui_state,
        "a",
        String::from("internal-a"),
        String::from("1001"),
        PendingSessionPromptSource::UserInterrupt,
    )
    .expect("route guidance prompt");
    let context = PendingPromptDispatchContext {
        has_running_subagents: false,
        has_active_run: true,
        run_phase: Some(crate::SessionRunPhase::ToolRunning),
        has_active_compaction: false,
    };

    let guided_entries =
        take_runtime_intervention_guided_pending_prompt_entries(&mut ui_state, context);

    assert_eq!(
        guided_entries
            .iter()
            .filter_map(|entry| entry.panel_entry_id.as_deref())
            .collect::<Vec<_>>(),
        vec!["a"]
    );
    assert!(ui_state.pending_prompt_queue.is_empty());
    assert!(ui_state.pending_prompt_panel_queue.is_empty());
}

/// Verifies dragging visible rows also updates the internal guided dispatch order.
/// 验证拖动可见行也会同步更新内部引导派发顺序。
#[test]
fn pending_prompt_panel_drop_target_reorders_inserted_internal_queue() {
    let mut ui_state = SessionTranscriptUiState {
        pending_prompt_panel_queue: vec![
            test_panel_entry("a"),
            test_panel_entry("b"),
            test_panel_entry("c"),
        ],
        ..SessionTranscriptUiState::default()
    };
    route_pending_prompt_panel_entry_into_internal_queue(
        &mut ui_state,
        "a",
        String::from("internal-a"),
        String::from("1001"),
        PendingSessionPromptSource::UserInterrupt,
    )
    .expect("route first guidance prompt");
    route_pending_prompt_panel_entry_into_internal_queue(
        &mut ui_state,
        "b",
        String::from("internal-b"),
        String::from("1002"),
        PendingSessionPromptSource::UserInterrupt,
    )
    .expect("route second guidance prompt");

    let changed = apply_pending_prompt_panel_drop_target(
        &mut ui_state,
        "b",
        &PendingPromptPanelDropTarget::Before(String::from("a")),
    );

    assert!(changed);
    assert_eq!(panel_entry_ids(&ui_state), vec!["b", "a", "c"]);
    let first = take_next_internal_pending_prompt_entry(&mut ui_state, false)
        .expect("take first guided prompt");
    let second = take_next_internal_pending_prompt_entry(&mut ui_state, false)
        .expect("take second guided prompt");
    assert_eq!(first.panel_entry_id.as_deref(), Some("b"));
    assert_eq!(second.panel_entry_id.as_deref(), Some("a"));
}

/// Verifies stale session mailbox prompts can be removed without touching user prompts.
/// 验证可清理过时会话 mailbox 提示词且不影响用户提示词。
#[test]
fn remove_internal_pending_prompt_entries_by_source_keeps_user_prompts() {
    let mut ui_state = SessionTranscriptUiState {
        pending_prompt_queue: vec![
            PendingSessionPromptEntry {
                source: PendingSessionPromptSource::SessionMailbox,
                ..test_internal_prompt_entry("session-mailbox", None)
            },
            test_internal_prompt_entry("user-followup", None),
        ],
        ..SessionTranscriptUiState::default()
    };

    let removed = remove_internal_pending_prompt_entries_by_source(
        &mut ui_state,
        PendingSessionPromptSource::SessionMailbox,
    );

    assert_eq!(removed, 1);
    assert_eq!(ui_state.pending_prompt_queue.len(), 1);
    assert_eq!(ui_state.pending_prompt_queue[0].id, "user-followup");
    assert_eq!(
        ui_state.pending_prompt_queue[0].source,
        PendingSessionPromptSource::UserFollowup
    );
}

/// Verifies prompt sources keep user follow-ups and interrupts in different turn roles.
/// 验证提示词来源会将用户跟进与插队提示词保持在不同轮次角色中。
#[test]
fn prompt_source_semantics_separate_followup_and_interrupt_turns() {
    assert!(PendingSessionPromptSource::UserFollowup.starts_user_parent_turn());
    assert!(PendingSessionPromptSource::UserFollowup.runs_user_prompt_precheck());
    assert!(PendingSessionPromptSource::UserFollowup.allows_user_prompt_edit());
    assert!(PendingSessionPromptSource::UserFollowup.appends_visible_user_prompt_record());

    assert!(!PendingSessionPromptSource::UserInterrupt.starts_user_parent_turn());
    assert!(!PendingSessionPromptSource::UserInterrupt.runs_user_prompt_precheck());
    assert!(!PendingSessionPromptSource::UserInterrupt.allows_user_prompt_edit());
    assert!(PendingSessionPromptSource::UserInterrupt.persists_canonical_user_message());
    assert!(PendingSessionPromptSource::UserInterrupt.appends_visible_user_prompt_record());
    assert!(PendingSessionPromptSource::UserInterrupt.folds_into_active_turn_process());
}

/// Verifies internal pending prompts preserve provider reasoning defaults unless explicitly set.
/// 验证内部待发送提示词在未显式设置时保留供应商推理默认值。
#[test]
fn pending_prompt_runtime_defaults_do_not_force_reasoning_effort() {
    assert_eq!(
        PendingPromptRuntimeOptions::default().reasoning_effort,
        vulcan_provider_config::config::ReasoningEffortLevel::Off
    );
}
