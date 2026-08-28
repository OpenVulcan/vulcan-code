use super::*;

/// Builds one minimal visible V1 queue projection row.
/// 构建一条最小可见 V1 队列投影行。
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

/// Verifies isolated Provider retry attachments retain their request-local delivery decision.
/// 验证隔离的 Provider 重试附件会保留其单次请求局部传输决策。
#[test]
fn provider_retry_attachment_round_trip_retains_delivery_mode() {
    let attachment = vulcan_core::types::ConversationAttachment {
        kind: vulcan_core::types::ConversationAttachmentKind::Image,
        file_name: String::from("screen.png"),
        local_path: String::from("D:/workspace/.vulcan/attachments/screen.png"),
        resource_path: Some(String::from(".vulcan/attachments/screen.png")),
        source_path: None,
        asset_id: Some(String::from("asset-screen")),
        byte_len: Some(128),
        source_sha256: Some(String::from("sha256-screen")),
        delivery_mode: Some(vulcan_core::types::ConversationAttachmentDeliveryMode::MetadataOnly),
        preview_local_path: None,
        mime_type: String::from("image/png"),
    };

    let value = serde_json::to_value(&attachment)
        .expect("provider retry attachment should serialize with its delivery mode");
    assert_eq!(value["delivery_mode"], serde_json::json!("metadata_only"));
    let restored: vulcan_core::types::ConversationAttachment = serde_json::from_value(value)
        .expect("provider retry attachment should deserialize with its delivery mode");

    assert_eq!(
        restored.delivery_mode,
        Some(vulcan_core::types::ConversationAttachmentDeliveryMode::MetadataOnly)
    );
    let mut canonical = restored;
    canonical.delivery_mode = None;
    let canonical_value = serde_json::to_value(canonical)
        .expect("canonical attachment should serialize without transient delivery mode");
    assert!(canonical_value.get("delivery_mode").is_none());
}

/// Verifies the visible V1 queue projection keeps later submissions on the admission lane.
/// 验证可见 V1 队列投影会让后续提交继续进入准入通道。
#[test]
fn pending_prompt_lane_busy_includes_visible_v1_projection() {
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

/// Verifies active Provider phases keep submissions on the admission lane.
/// 验证活跃 Provider 阶段会让提交保持在准入通道。
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
    }
}

/// Verifies stale preparation without active precheck does not trap new submissions.
/// 验证没有活跃预检查的陈旧准备阶段不会困住新提交。
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

/// Verifies prompt sources keep user follow-ups and interrupts in different Turn roles.
/// 验证提示词来源会将用户跟进与插队提示词保持在不同回合角色中。
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
    assert_eq!(
        PendingSessionPromptSource::UserInterrupt.for_idle_dispatch(),
        PendingSessionPromptSource::UserFollowup
    );
    assert_eq!(
        PendingSessionPromptSource::GoalContinuation.for_idle_dispatch(),
        PendingSessionPromptSource::GoalContinuation
    );
}

/// Verifies process-local prompt adapters preserve Provider reasoning defaults unless explicitly set.
/// 验证进程内提示词适配器在未显式设置时保留 Provider 推理默认值。
#[test]
fn pending_prompt_runtime_defaults_do_not_force_reasoning_effort() {
    assert_eq!(
        PendingPromptRuntimeOptions::default().reasoning_effort,
        vulcan_provider_config::config::ReasoningEffortLevel::Off
    );
}
