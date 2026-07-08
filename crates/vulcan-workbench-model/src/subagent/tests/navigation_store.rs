use super::*;

/// Builds one task metadata document for navigation-store tests.
/// 为导航存储测试构建单个任务元数据文档。
fn navigation_store_test_metadata(
    root_session_id: vulcan_session_core::SessionNodeId,
    task_session_id: vulcan_session_core::SessionNodeId,
    parent: ParentSessionRef,
    public_task_id: &str,
    display_title: &str,
    depth: u16,
) -> TaskSessionMetadata {
    TaskSessionMetadata::new(vulcan_task_session::TaskSessionMetadataInit {
        root_session_id,
        task_session_id,
        parent,
        public_task_id: public_task_id.to_string(),
        display_title: display_title.to_string(),
        requested_config: vulcan_session_core::SessionRequestedConfig::default(),
        effective_config: vulcan_session_core::SessionEffectiveConfig::default(),
        depth,
        timestamp_ms: u64::from(depth).saturating_mul(10),
    })
}

/// Verifies the shared navigation visibility gate preserves every unclosed child route.
/// 验证共享导航可见性门禁会保留每个未关闭子代理入口。
#[test]
fn subagent_navigation_surface_visibility_matches_current_round() {
    assert!(!subagent_navigation_surface_is_visible(false, false, false));
    assert!(subagent_navigation_surface_is_visible(true, false, false));
    assert!(subagent_navigation_surface_is_visible(false, true, false));
    assert!(subagent_navigation_surface_is_visible(false, false, true));
}

/// Verifies the shared round retention gate keeps navigation only while parent or child runtime work remains.
/// 验证共享轮次保留门禁只会在父级或子级运行工作仍存在时继续保留导航。
#[test]
fn subagent_navigation_round_visibility_tracks_coordination_work() {
    assert!(!subagent_navigation_round_should_remain_visible(
        false, false
    ));
    assert!(subagent_navigation_round_should_remain_visible(true, false));
    assert!(subagent_navigation_round_should_remain_visible(false, true));
}

/// Verifies the segmented store route lists nested TaskSessions by their direct task-session parent.
/// 验证分段存储路由可以按直接 TaskSession 父级列出嵌套子代理。
#[test]
fn loads_nested_panel_states_from_segmented_task_session_store() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let store = vulcan_task_session::SegmentedTaskSessionStore::new(temp_dir.path());
    let root_session_id = vulcan_session_core::SessionNodeId::new_v4();
    let parent_task_session_id = vulcan_session_core::SessionNodeId::new_v4();
    let child_task_session_id = vulcan_session_core::SessionNodeId::new_v4();
    store
        .create_task_session(navigation_store_test_metadata(
            root_session_id,
            parent_task_session_id,
            ParentSessionRef::MainSession { root_session_id },
            "parent-task",
            "父子代理",
            1,
        ))
        .expect("create parent task session");
    store
        .create_task_session(navigation_store_test_metadata(
            root_session_id,
            child_task_session_id,
            ParentSessionRef::TaskSession {
                root_session_id,
                parent_task_session_id,
            },
            "child-task",
            "嵌套子代理",
            2,
        ))
        .expect("create child task session");

    let root_states = subagent_task_panel_states_from_segmented_task_session_store(
        &store,
        root_session_id.as_uuid(),
    )
    .expect("load root states");
    let nested_states = subagent_task_panel_states_from_segmented_task_session_store(
        &store,
        parent_task_session_id.as_uuid(),
    )
    .expect("load nested states");

    assert_eq!(root_states.len(), 1);
    assert_eq!(root_states[0].task_id, parent_task_session_id.as_uuid());
    assert_eq!(nested_states.len(), 1);
    assert_eq!(
        nested_states[0].parent_session_id,
        parent_task_session_id.as_uuid()
    );
    assert_eq!(nested_states[0].task_id, child_task_session_id.as_uuid());
}

/// Verifies restart-time list projection restores completed state from metadata without reading task JSONL files.
/// 验证重启时的列表投影会从元数据恢复完成态，而不读取子任务 JSONL 文件。
#[test]
fn restores_completed_panel_state_from_segmented_task_session_store() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let store = vulcan_task_session::SegmentedTaskSessionStore::new(temp_dir.path());
    let root_session_id = vulcan_session_core::SessionNodeId::new_v4();
    let task_session_id = vulcan_session_core::SessionNodeId::new_v4();
    let mut metadata = navigation_store_test_metadata(
        root_session_id,
        task_session_id,
        ParentSessionRef::MainSession { root_session_id },
        "image-worker-a",
        "生成图片 A",
        1,
    );
    metadata.description = Some(String::from("生成湖景图片"));
    store
        .create_task_session(metadata.clone())
        .expect("create task session");
    metadata.status = TaskSessionStatus::Completed;
    metadata.last_result_summary = Some(String::from("metadata fallback result"));
    metadata.updated_at = 20;
    store
        .save_task_session(metadata)
        .expect("save completed metadata");
    store
        .append_event(vulcan_session_core::SessionEventRecord::new(
            "task-result",
            task_session_id,
            1,
            vulcan_session_core::SessionEventKind::TaskTerminal,
            20,
            json!({ "result": "latest event image result" }),
        ))
        .expect("append latest result event");

    let states = subagent_task_panel_states_from_segmented_task_session_store(
        &store,
        root_session_id.as_uuid(),
    )
    .expect("load states");

    assert_eq!(states.len(), 1);
    assert_eq!(states[0].task_id, task_session_id.as_uuid());
    assert_eq!(states[0].public_task_id, "image-worker-a");
    assert_eq!(states[0].run_state, SubagentTaskRunState::Completed);
    assert_eq!(states[0].description.as_deref(), Some("生成湖景图片"));
    assert_eq!(states[0].result_text, "metadata fallback result");
    assert_eq!(states[0].subagent_label, "生成图片 A");

    let detail =
        subagent_task_panel_state_detail_from_segmented_task_session_store(&store, task_session_id)
            .expect("load detail state")
            .expect("detail state");
    assert_eq!(detail.result_text, "latest event image result");
}
