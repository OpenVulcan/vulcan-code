use super::*;

#[test]
fn builds_panel_state_from_task_session_metadata() {
    let root_session_id = vulcan_session_core::SessionNodeId::new_v4();
    let task_session_id = vulcan_session_core::SessionNodeId::new_v4();
    let mut metadata = TaskSessionMetadata::new(
        root_session_id,
        task_session_id,
        ParentSessionRef::MainSession { root_session_id },
        "audit-task",
        "代码审核",
        vulcan_session_core::SessionRequestedConfig::default(),
        vulcan_session_core::SessionEffectiveConfig::default(),
        1,
        10,
    );
    metadata.status = TaskSessionStatus::Completed;
    metadata.description = Some(String::from("审核当前代码"));
    metadata.last_result_summary = Some(String::from("摘要结果"));
    let context = SessionContextDocument {
        schema_version: 1,
        node_id: task_session_id,
        provider_id: None,
        model_id: None,
        reasoning_effort: None,
        system_prompt_layers: Vec::new(),
        conversation_state: json!(SessionConversationState::default()),
        compaction_anchor: None,
        context_budget_fingerprint: None,
        tool_catalog_snapshot: Value::Null,
        provider_request_preview: None,
    };

    let state = subagent_task_panel_state_from_task_session(&metadata, Some(&context), None)
        .expect("panel state");

    assert_eq!(state.parent_session_id, root_session_id.as_uuid());
    assert_eq!(state.task_id, task_session_id.as_uuid());
    assert_eq!(state.public_task_id, "audit-task");
    assert_eq!(state.subagent_label, "代码审核");
    assert_eq!(state.run_state, SubagentTaskRunState::Completed);
    assert!(state.conversation_state.is_some());
}

/// Verifies the segmented list route uses metadata only and leaves detail loading to the selected task.
/// 验证分段列表路由仅使用元数据，并将详情加载留给选中的任务。
#[test]
fn loads_panel_states_from_segmented_task_session_store() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let store = vulcan_task_session::SegmentedTaskSessionStore::new(temp_dir.path());
    let root_session_id = vulcan_session_core::SessionNodeId::new_v4();
    let task_session_id = vulcan_session_core::SessionNodeId::new_v4();
    let mut metadata = TaskSessionMetadata::new(
        root_session_id,
        task_session_id,
        ParentSessionRef::MainSession { root_session_id },
        "audit-task",
        "代码审核",
        vulcan_session_core::SessionRequestedConfig::default(),
        vulcan_session_core::SessionEffectiveConfig::default(),
        1,
        10,
    );
    metadata.status = TaskSessionStatus::Completed;
    store
        .create_task_session(metadata)
        .expect("create segmented task session");
    store
        .append_event(vulcan_session_core::SessionEventRecord::new(
            "event-1",
            task_session_id,
            0,
            vulcan_session_core::SessionEventKind::AssistantOutput,
            20,
            json!({ "result": "完整结果" }),
        ))
        .expect("append segmented task event");
    let conversation_state = conversation_state_from_session_messages(&[SessionMessageEntry::new(
        SessionMessageKind::Assistant,
        "AI",
        "完整结果",
    )
    .with_runtime_metadata("assistant-1", "20")]);
    store
        .save_context(SessionContextDocument {
            schema_version: 1,
            node_id: task_session_id,
            provider_id: None,
            model_id: None,
            reasoning_effort: None,
            system_prompt_layers: Vec::new(),
            conversation_state: json!(conversation_state),
            compaction_anchor: None,
            context_budget_fingerprint: None,
            tool_catalog_snapshot: Value::Null,
            provider_request_preview: None,
        })
        .expect("save segmented context");

    let states = subagent_task_panel_states_from_segmented_task_session_store(
        &store,
        root_session_id.as_uuid(),
    );

    assert_eq!(states.len(), 1);
    assert_eq!(states[0].task_id, task_session_id.as_uuid());
    assert!(states[0].result_text.is_empty());
    assert!(!subagent_task_panel_state_has_recoverable_conversation(
        &states[0]
    ));

    let detail =
        subagent_task_panel_state_detail_from_segmented_task_session_store(&store, task_session_id)
            .expect("detail state");
    assert_eq!(detail.result_text, "完整结果");
    assert!(subagent_task_panel_state_has_recoverable_conversation(
        &detail
    ));
}

/// Verifies archived TaskSessions are omitted from the unclosed subagent list restored at startup.
/// 验证启动恢复未关闭子代理列表时会跳过已归档的 TaskSession。
#[test]
fn segmented_task_session_store_omits_archived_panel_states() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let store = vulcan_task_session::SegmentedTaskSessionStore::new(temp_dir.path());
    let root_session_id = vulcan_session_core::SessionNodeId::new_v4();
    let open_task_session_id = vulcan_session_core::SessionNodeId::new_v4();
    let archived_task_session_id = vulcan_session_core::SessionNodeId::new_v4();
    let mut open_metadata = TaskSessionMetadata::new(
        root_session_id,
        open_task_session_id,
        ParentSessionRef::MainSession { root_session_id },
        "open-task",
        "未关闭智能体",
        vulcan_session_core::SessionRequestedConfig::default(),
        vulcan_session_core::SessionEffectiveConfig::default(),
        1,
        10,
    );
    open_metadata.status = TaskSessionStatus::Completed;
    let mut archived_metadata = TaskSessionMetadata::new(
        root_session_id,
        archived_task_session_id,
        ParentSessionRef::MainSession { root_session_id },
        "closed-task",
        "已关闭智能体",
        vulcan_session_core::SessionRequestedConfig::default(),
        vulcan_session_core::SessionEffectiveConfig::default(),
        1,
        11,
    );
    archived_metadata.status = TaskSessionStatus::Archived;
    store
        .create_task_session(open_metadata)
        .expect("create open task session");
    store
        .create_task_session(archived_metadata)
        .expect("create archived task session");

    let states = subagent_task_panel_states_from_segmented_task_session_store(
        &store,
        root_session_id.as_uuid(),
    );

    assert_eq!(states.len(), 1);
    assert_eq!(states[0].task_id, open_task_session_id.as_uuid());
}

/// Verifies shared running-task collection walks nested child trees and skips terminal children.
/// 验证共享运行任务收集逻辑会遍历嵌套子树，并跳过已进入终态的子任务。
#[test]
fn collects_running_subagent_task_states_for_tree() {
    let root_session_id = SessionId::new_v4();
    let parent_task_id = SessionId::new_v4();
    let running_child_task_id = SessionId::new_v4();
    let completed_child_task_id = SessionId::new_v4();
    let build_state =
        |task_id: SessionId, run_state: SubagentTaskRunState| SubagentTaskPanelState {
            task_id,
            public_task_id: task_id.to_string(),
            parent_session_id: root_session_id,
            subagent_label: String::from("worker"),
            description: None,
            run_state,
            result_text: String::new(),
            conversation_state: None,
        };

    let running_states =
        collect_running_subagent_task_states_for_tree(root_session_id, |parent_id| {
            if parent_id == root_session_id {
                vec![build_state(parent_task_id, SubagentTaskRunState::Running)]
            } else if parent_id == parent_task_id {
                vec![
                    build_state(running_child_task_id, SubagentTaskRunState::Running),
                    build_state(completed_child_task_id, SubagentTaskRunState::Completed),
                ]
            } else {
                Vec::new()
            }
        });

    assert_eq!(running_states.len(), 2);
    assert_eq!(running_states[0].0, root_session_id);
    assert_eq!(running_states[0].1.task_id, parent_task_id);
    assert_eq!(running_states[1].0, parent_task_id);
    assert_eq!(running_states[1].1.task_id, running_child_task_id);
}

/// Verifies shared cache merging preserves live entries while using segmented TaskSession states as the persisted source.
/// 验证共享缓存合并会保留实时条目，并以分段 TaskSession 状态作为持久来源。
#[test]
fn merges_subagent_task_panel_cache_from_segmented_and_live_states() {
    let parent_session_id = SessionId::new_v4();
    let live_task_id = SessionId::new_v4();
    let stale_task_id = SessionId::new_v4();
    let segmented_task_id = SessionId::new_v4();
    let mut previous_states = BTreeMap::new();
    previous_states.insert(
        live_task_id,
        test_subagent_state(
            parent_session_id,
            live_task_id,
            SubagentTaskRunState::Running,
        ),
    );
    previous_states.insert(
        stale_task_id,
        test_subagent_state(
            parent_session_id,
            stale_task_id,
            SubagentTaskRunState::Running,
        ),
    );

    let mut segmented_state = test_subagent_state(
        parent_session_id,
        segmented_task_id,
        SubagentTaskRunState::Completed,
    );
    segmented_state.result_text = String::from("segmented result");

    let merged =
        merge_subagent_task_panel_cache(&previous_states, vec![segmented_state], |task_id| {
            task_id == live_task_id
        });

    assert_eq!(
        merged.get(&live_task_id).map(|state| state.run_state),
        Some(SubagentTaskRunState::Running)
    );
    assert_eq!(
        merged.get(&stale_task_id).map(|state| state.run_state),
        None
    );
    assert_eq!(
        merged
            .get(&segmented_task_id)
            .map(|state| state.result_text.as_str()),
        Some("segmented result")
    );
}

/// Verifies live runtime state wins over stale completed payloads.
/// 验证实时运行态优先于陈旧的已完成载荷。
#[test]
fn live_runtime_state_overrides_stale_completed_payload() {
    let parent_session_id = SessionId::new_v4();
    let task_id = SessionId::new_v4();
    let mut completed_state =
        test_subagent_state(parent_session_id, task_id, SubagentTaskRunState::Completed);
    completed_state.result_text = String::from("stale completed result");

    let normalized = normalize_subagent_task_panel_state(&completed_state, true, None);

    assert_eq!(normalized.run_state, SubagentTaskRunState::Running);

    let merged = merge_subagent_task_panel_cache(
        &BTreeMap::new(),
        vec![completed_state],
        |candidate_task_id| candidate_task_id == task_id,
    );

    assert_eq!(
        merged.get(&task_id).map(|state| state.run_state),
        Some(SubagentTaskRunState::Running)
    );
}

/// Verifies recoverable child conversations may come from protocol messages even when records are empty.
/// 验证即使 records 为空，也可以从协议 messages 判断子会话内容可复看。
#[test]
fn panel_state_recoverable_conversation_accepts_messages() {
    let task_id = SessionId::new_v4();
    let mut conversation_state = SessionConversationState::default();
    vulcan_context::append_assistant_text_message(&mut conversation_state, "assistant-1", "done");
    let state = SubagentTaskPanelState {
        task_id,
        public_task_id: task_id.to_string(),
        parent_session_id: SessionId::new_v4(),
        subagent_label: String::from("worker"),
        description: None,
        run_state: SubagentTaskRunState::Completed,
        result_text: String::new(),
        conversation_state: Some(conversation_state),
    };

    assert!(subagent_task_panel_state_has_recoverable_conversation(
        &state
    ));
}
