//! Segmented task-session routing and scheduling boundaries.
//! 分段子代理会话路由与调度边界。

/// Private crash-resistant file replacement shared by task-session persistence layers.
/// 任务会话持久化层共享的私有抗崩溃文件替换能力。
mod atomic_file;
/// Subagent catalog loading and profile conversion.
/// 子代理目录加载与配置转换。
pub mod catalog;
/// Task-session compaction contracts.
/// 子代理会话压缩契约。
pub mod compaction;
/// Task-session effective configuration resolution.
/// 子代理会话有效配置解析。
pub mod config_resolver;
/// Task-session private domain models.
/// 子代理会话私有领域模型。
pub mod model;
/// Subagent profile models.
/// 子代理配置模型。
pub mod profile;
/// Task-session router contracts.
/// 子代理会话路由契约。
pub mod router;
/// Task-session scheduler contracts.
/// 子代理会话调度契约。
pub mod scheduler;
/// Segmented task-session store implementation.
/// 分段子代理会话存储实现。
pub mod segmented_store;
/// Task-session store contracts.
/// 子代理会话存储契约。
pub mod store;

pub use catalog::{
    SUBAGENT_MODE_DEFAULT, SUBAGENT_MODE_EXPERT, SUBAGENT_MODE_MIRROR, SubagentCatalog,
    SubagentCatalogLoadError, SubagentDefinition, SubagentDefinitionMode, SubagentDefinitionScope,
    SubagentDefinitionSource, display_subagent_category_id,
    ensure_user_subagent_category_directories, load_subagent_catalog_from_roots,
    parse_subagent_definition_file, parse_subagent_definition_text,
    subagent_definition_to_markdown, user_subagent_definition_path, user_subagent_override_path,
    user_subagent_sidecar_path, write_user_subagent_definition,
    write_user_subagent_enabled_override, write_user_subagent_metadata_override,
    write_user_subagent_override,
};
pub use compaction::{
    TaskSessionCompactionApplication, TaskSessionCompactionJob, TaskSessionCompactionStore,
    apply_task_session_compaction_result,
};
pub use config_resolver::{
    TaskInvocationConfig, TaskSessionConfigResolutionInput, TaskSessionConfigResolver,
    TaskSessionResolvedConfig,
};
pub use model::{
    TaskInvocationEvent, TaskInvocationEventKind, TaskSessionId, TaskSessionMetadata,
    TaskSessionMetadataInit, TaskSessionMirrorSummary, TaskSessionStatus, TaskSessionSummary,
};
pub use profile::SubagentProfile;
pub use router::{TaskSessionHistoryPage, TaskSessionRouter};
pub use scheduler::{TaskSessionCreateRequest, TaskSessionScheduler};
pub use segmented_store::SegmentedTaskSessionStore;
pub use store::TaskSessionStore;

#[cfg(test)]
mod tests {
    use super::{
        SubagentProfile, TaskInvocationConfig, TaskSessionCompactionStore,
        TaskSessionConfigResolutionInput, TaskSessionConfigResolver, TaskSessionMetadata,
        TaskSessionMetadataInit, TaskSessionStatus, TaskSessionStore,
        apply_task_session_compaction_result,
    };
    use serde_json::json;
    use vulcan_context::{
        ContextCompactionPolicy, SessionConversationMessage, SessionConversationPayload,
        SessionConversationState, SessionMessageKind, SessionMessageRecordKind,
    };
    use vulcan_context_compaction::{CompactionResult, CompactionSummaryRecord};
    use vulcan_session_core::{
        ExecutionRuntimePolicyRequest, ParentSessionRef, ProviderModelSnapshot,
        SessionContextDocument, SessionEffectiveConfig, SessionEventKind, SessionEventRecord,
        SessionNodeId, SessionNodeStore, ToolExposureSnapshot,
    };

    /// Builds a parent config used by resolver tests.
    /// 构建解析器测试使用的父级配置。
    fn parent_config() -> SessionEffectiveConfig {
        let mut config = SessionEffectiveConfig {
            provider_model: ProviderModelSnapshot {
                provider_id: Some("deepseek".to_string()),
                model_id: Some("deepseek-v4".to_string()),
                reasoning_effort: Some("high".to_string()),
                execution_mode: Some("build".to_string()),
            },
            system_prompt_layers: vec!["parent prompt".to_string()],
            ..Default::default()
        };
        config.tool_exposure.allowed_tools = vec!["shell_command".to_string()];
        config
    }

    /// Verifies that task sessions inherit parent config while keeping side effects disabled.
    /// 验证子代理会话继承父配置但保持副作用关闭。
    #[test]
    fn task_session_inherits_parent_config_but_disables_side_effects() {
        let input = TaskSessionConfigResolutionInput {
            global_default_config: SessionEffectiveConfig::default(),
            parent_config: Some(parent_config()),
            profile: None,
            invocation: TaskInvocationConfig::main("review this change"),
        };

        let resolved = TaskSessionConfigResolver::resolve(input);

        assert!(resolved.effective_config.inherit_parent_config);
        assert_eq!(
            resolved
                .effective_config
                .provider_model
                .provider_id
                .as_deref(),
            Some("deepseek")
        );
        assert_eq!(
            resolved.effective_config.tool_exposure.allowed_tools,
            vec!["shell_command"]
        );
        assert!(!resolved.runtime_policy.hooks_enabled);
        assert!(!resolved.runtime_policy.vmm_enabled);
        assert!(!resolved.runtime_policy.memory_recall_enabled);
        assert!(!resolved.runtime_policy.memory_writeback_enabled);
        assert!(!resolved.runtime_policy.database_projection_enabled);
    }

    /// Verifies explicit TaskSession runtime overrides cannot enable parent-session side effects.
    /// 验证明示 TaskSession 运行时覆盖无法启用父会话副作用。
    #[test]
    fn task_session_runtime_overrides_keep_side_effects_forbidden() {
        let mut profile = SubagentProfile::new("memory-agent", "Memory agent");
        profile.runtime_policy = ExecutionRuntimePolicyRequest {
            hooks_enabled: Some(true),
            vmm_enabled: Some(true),
            memory_recall_enabled: Some(true),
            memory_writeback_enabled: Some(true),
            database_projection_enabled: Some(true),
        };
        let mut invocation = TaskInvocationConfig::with_profile("memory-agent", "recall context");
        invocation.runtime_policy = ExecutionRuntimePolicyRequest {
            hooks_enabled: Some(false),
            vmm_enabled: Some(true),
            memory_recall_enabled: Some(false),
            memory_writeback_enabled: Some(true),
            database_projection_enabled: Some(true),
        };

        let resolved = TaskSessionConfigResolver::resolve(TaskSessionConfigResolutionInput {
            global_default_config: SessionEffectiveConfig::default(),
            parent_config: Some(parent_config()),
            profile: Some(profile),
            invocation,
        });

        assert!(!resolved.runtime_policy.hooks_enabled);
        assert!(!resolved.runtime_policy.vmm_enabled);
        assert!(!resolved.runtime_policy.memory_recall_enabled);
        assert!(!resolved.runtime_policy.memory_writeback_enabled);
        assert!(!resolved.runtime_policy.database_projection_enabled);
    }

    /// Verifies that profile inheritance false prevents parent config from leaking in.
    /// 验证配置中的不继承会阻止父级配置泄漏。
    #[test]
    fn profile_can_disable_parent_inheritance() {
        let mut profile = SubagentProfile::new("agent-review", "Code reviewer");
        profile.inherit_parent_config = Some(false);
        profile.base_system_prompt = Some("profile prompt".to_string());

        let input = TaskSessionConfigResolutionInput {
            global_default_config: SessionEffectiveConfig::default(),
            parent_config: Some(parent_config()),
            profile: Some(profile),
            invocation: TaskInvocationConfig::with_profile("agent-review", "review"),
        };

        let resolved = TaskSessionConfigResolver::resolve(input);

        assert!(!resolved.effective_config.inherit_parent_config);
        assert_eq!(resolved.effective_config.provider_model.provider_id, None);
        assert_eq!(
            resolved.effective_config.system_prompt_layers,
            vec!["profile prompt"]
        );
    }

    /// Verifies that invocation overrides profile settings at the final layer.
    /// 验证调用参数会在最后一层覆盖 profile 设置。
    #[test]
    fn invocation_overrides_profile_settings() {
        let mut profile = SubagentProfile::new("film-dev", "Film developer");
        profile.provider_model.model_id = Some("profile-model".to_string());
        profile.tool_exposure.allowed_tools = vec!["image_generation".to_string()];

        let mut invocation = TaskInvocationConfig::with_profile("film-dev", "make storyboard");
        invocation.provider_model.model_id = Some("invocation-model".to_string());
        invocation.tool_exposure = ToolExposureSnapshot {
            allowed_tools: vec!["video_generation".to_string()],
            ..ToolExposureSnapshot::default()
        };
        invocation.system_prompt = Some("invocation system prompt".to_string());

        let input = TaskSessionConfigResolutionInput {
            global_default_config: SessionEffectiveConfig::default(),
            parent_config: Some(parent_config()),
            profile: Some(profile),
            invocation,
        };

        let resolved = TaskSessionConfigResolver::resolve(input);

        assert_eq!(
            resolved.effective_config.provider_model.model_id.as_deref(),
            Some("invocation-model")
        );
        assert_eq!(
            resolved.effective_config.tool_exposure.allowed_tools,
            vec!["video_generation"]
        );
        assert_eq!(
            resolved.effective_config.system_prompt_layers,
            vec!["invocation system prompt"]
        );
    }

    /// Verifies that task metadata records the immutable projection-isolation policy boundary.
    /// 验证子代理元数据会记录不可变的投影隔离策略边界。
    #[test]
    fn task_metadata_records_projection_isolation_boundary() {
        let root_session_id = SessionNodeId::new_v4();
        let task_session_id = SessionNodeId::new_v4();
        let parent = ParentSessionRef::MainSession { root_session_id };
        let resolved = TaskSessionConfigResolver::resolve(TaskSessionConfigResolutionInput {
            global_default_config: SessionEffectiveConfig::default(),
            parent_config: None,
            profile: None,
            invocation: TaskInvocationConfig::main("run task"),
        });

        let metadata = TaskSessionMetadata::new(TaskSessionMetadataInit {
            root_session_id,
            task_session_id,
            parent,
            public_task_id: String::from("task-1"),
            display_title: String::from("Run task"),
            requested_config: resolved.requested_config,
            effective_config: resolved.effective_config,
            depth: 1,
            timestamp_ms: 10,
        });

        assert_eq!(metadata.status, TaskSessionStatus::Created);
        assert!(!metadata.runtime_policy.database_projection_enabled);
    }

    /// Verifies that the segmented store writes and reads TaskSession artifacts from the session directory.
    /// 验证分段存储会从会话目录写入和读取 TaskSession 产物。
    #[test]
    fn segmented_store_round_trips_task_session_artifacts() {
        let temp_dir = std::env::temp_dir().join(format!(
            "vulcan-task-session-test-{}",
            SessionNodeId::new_v4()
        ));
        let store = super::SegmentedTaskSessionStore::new(temp_dir.clone());
        let root_session_id = SessionNodeId::new_v4();
        let task_session_id = SessionNodeId::new_v4();
        let parent = ParentSessionRef::MainSession { root_session_id };
        let resolved = TaskSessionConfigResolver::resolve(TaskSessionConfigResolutionInput {
            global_default_config: SessionEffectiveConfig::default(),
            parent_config: None,
            profile: None,
            invocation: TaskInvocationConfig::main("persist task"),
        });
        let mut metadata = TaskSessionMetadata::new(TaskSessionMetadataInit {
            root_session_id,
            task_session_id,
            parent: parent.clone(),
            public_task_id: String::from("task-file"),
            display_title: String::from("Task file"),
            requested_config: resolved.requested_config,
            effective_config: resolved.effective_config,
            depth: 1,
            timestamp_ms: 123,
        });

        store
            .create_task_session(metadata.clone())
            .expect("task metadata should be created");
        metadata.status = TaskSessionStatus::Running;
        store
            .save_task_session(metadata.clone())
            .expect("task metadata should be saved");

        let loaded = store
            .load_task_session(task_session_id)
            .expect("task metadata should load")
            .expect("task metadata should exist");
        assert_eq!(loaded.status, TaskSessionStatus::Running);
        let loaded_by_public_id = store
            .find_task_session_by_parent_and_public_id(parent.clone(), "task-file")
            .expect("task metadata should load by parent and public id")
            .expect("task metadata should exist by parent and public id");
        assert_eq!(loaded_by_public_id.task_session_id, task_session_id);
        assert!(
            store
                .find_task_session_by_parent_and_public_id(parent.clone(), "missing-task")
                .expect("missing public id lookup should not fail")
                .is_none()
        );
        let child_task_ids = store
            .list_child_task_session_ids(parent.clone())
            .expect("child task ids should load");
        assert_eq!(child_task_ids, vec![task_session_id]);

        let event = SessionEventRecord::new(
            "event-1",
            task_session_id,
            1,
            SessionEventKind::UserPrompt,
            124,
            json!({ "prompt": "persist task" }),
        );
        store
            .append_event(event.clone())
            .expect("task event should append");
        assert_eq!(
            store
                .load_events(task_session_id)
                .expect("task events should load"),
            vec![event]
        );

        let context = SessionContextDocument {
            schema_version: 1,
            node_id: task_session_id,
            provider_id: Some("provider".to_string()),
            model_id: Some("model".to_string()),
            reasoning_effort: None,
            system_prompt_layers: vec!["system".to_string()],
            conversation_state: json!({ "messages": [] }),
            compaction_anchor: None,
            context_budget_fingerprint: None,
            tool_catalog_snapshot: json!({ "tools": [] }),
            provider_request_preview: None,
        };
        store
            .save_context(context.clone())
            .expect("task context should save");
        assert_eq!(
            store
                .load_context(task_session_id)
                .expect("task context should load"),
            Some(context)
        );

        let compacted_context = SessionContextDocument {
            schema_version: 1,
            node_id: task_session_id,
            provider_id: Some("provider".to_string()),
            model_id: Some("model".to_string()),
            reasoning_effort: None,
            system_prompt_layers: vec!["compacted system".to_string()],
            conversation_state: json!({ "messages": [{ "role": "system", "content": "summary" }] }),
            compaction_anchor: Some("checkpoint-1".to_string()),
            context_budget_fingerprint: Some("budget-1".to_string()),
            tool_catalog_snapshot: json!({ "tools": ["task"] }),
            provider_request_preview: None,
        };
        store
            .save_compacted_context(compacted_context.clone())
            .expect("compacted task context should save");
        assert_eq!(
            store
                .load_context(task_session_id)
                .expect("compacted task context should load"),
            Some(compacted_context)
        );
        let checkpoint = vulcan_session_core::SessionCompactionCheckpoint {
            checkpoint_id: "checkpoint-1".to_string(),
            node_id: task_session_id,
            through_sequence: 1,
            timestamp_ms: 125,
            summary: "summary".to_string(),
        };
        store
            .save_compaction_checkpoint(checkpoint.clone())
            .expect("task compaction checkpoint should save");
        assert_eq!(
            store
                .load_task_compaction_checkpoints(task_session_id)
                .expect("task compaction checkpoints should load"),
            vec![checkpoint]
        );

        std::fs::remove_dir_all(temp_dir).expect("temporary task store should be removable");
    }

    /// Builds task metadata with deterministic configuration for segmented-store identity tests.
    /// 使用确定性配置为分段存储身份测试构建子任务元数据。
    ///
    /// Parameters: root and task session identifiers, the direct parent, the public task id, and the creation timestamp.
    /// 参数：根会话与子任务会话标识、直接父级、公开任务标识以及创建时间戳。
    ///
    /// Returns: a newly created task-session metadata document in its initial status.
    /// 返回：处于初始状态的新建子任务会话元数据文档。
    fn segmented_store_identity_test_metadata(
        root_session_id: SessionNodeId,
        task_session_id: SessionNodeId,
        parent: ParentSessionRef,
        public_task_id: &str,
        timestamp_ms: u64,
    ) -> TaskSessionMetadata {
        // resolved provides the authoritative requested and effective configurations persisted by the fixture.
        // resolved 提供夹具持久化的权威请求配置与有效配置。
        let resolved = TaskSessionConfigResolver::resolve(TaskSessionConfigResolutionInput {
            global_default_config: SessionEffectiveConfig::default(),
            parent_config: None,
            profile: None,
            invocation: TaskInvocationConfig::main("persist identity fixture"),
        });
        TaskSessionMetadata::new(TaskSessionMetadataInit {
            root_session_id,
            task_session_id,
            parent,
            public_task_id: public_task_id.to_string(),
            display_title: public_task_id.to_string(),
            requested_config: resolved.requested_config,
            effective_config: resolved.effective_config,
            depth: 1,
            timestamp_ms,
        })
    }

    /// Rejects ambiguous open ownership for one parent-scoped public task identifier.
    /// 拒绝同一父级作用域公开任务标识存在多个开放所有者的歧义状态。
    #[test]
    fn segmented_store_rejects_duplicate_open_parent_scoped_public_task_ids() {
        // temp_dir isolates both persisted metadata documents from other store tests.
        // temp_dir 将两个持久化元数据文档与其他存储测试隔离。
        let temp_dir = std::env::temp_dir().join(format!(
            "vulcan-task-session-duplicate-open-test-{}",
            SessionNodeId::new_v4()
        ));
        // store is the real segmented store under test.
        // store 是被测的真实分段存储。
        let store = super::SegmentedTaskSessionStore::new(temp_dir.clone());
        // root_session_id owns the direct parent scope used by both records.
        // root_session_id 拥有两条记录共同使用的直接父级作用域。
        let root_session_id = SessionNodeId::new_v4();
        // parent is the exact identity scope whose uniqueness must be enforced.
        // parent 是必须执行唯一性约束的精确身份作用域。
        let parent = ParentSessionRef::MainSession { root_session_id };
        // first_task_session_id identifies the first non-archived owner.
        // first_task_session_id 标识第一个未归档所有者。
        let first_task_session_id = SessionNodeId::new_v4();
        // second_task_session_id identifies the conflicting non-archived owner.
        // second_task_session_id 标识发生冲突的第二个未归档所有者。
        let second_task_session_id = SessionNodeId::new_v4();
        // first_metadata persists the first open record for the public slot.
        // first_metadata 持久化公开槽位的第一条开放记录。
        let first_metadata = segmented_store_identity_test_metadata(
            root_session_id,
            first_task_session_id,
            parent.clone(),
            "duplicate-slot",
            100,
        );
        // second_metadata persists a second open record in that exact same slot.
        // second_metadata 在完全相同的槽位中持久化第二条开放记录。
        let second_metadata = segmented_store_identity_test_metadata(
            root_session_id,
            second_task_session_id,
            parent.clone(),
            "duplicate-slot",
            200,
        );

        store
            .create_task_session(first_metadata)
            .expect("first open task metadata should be created");
        store
            .create_task_session(second_metadata)
            .expect("second open task metadata should be created");

        // error must expose the corrupted parent-scoped ownership instead of selecting one candidate.
        // error 必须暴露损坏的父级作用域所有权，而不是任选一个候选记录。
        let error = store
            .find_task_session_by_parent_and_public_id(parent, "duplicate-slot")
            .expect_err("duplicate open task ownership should be rejected");
        // error_message is inspected to prove that the diagnostic identifies both competing sessions.
        // error_message 用于证明诊断明确标识了两个竞争会话。
        let error_message = error.to_string();
        assert!(
            error_message.contains("multiple open sessions"),
            "unexpected duplicate ownership error: {error_message}"
        );
        assert!(error_message.contains(&first_task_session_id.to_string()));
        assert!(error_message.contains(&second_task_session_id.to_string()));

        std::fs::remove_dir_all(temp_dir).expect("temporary task store should be removable");
    }

    /// Prefers the unique open owner when archived history reuses the same public task identifier.
    /// 当归档历史复用相同公开任务标识时，优先返回唯一开放所有者。
    #[test]
    fn segmented_store_selects_unique_open_owner_over_archived_history() {
        // temp_dir isolates the archived-history reuse scenario from other store tests.
        // temp_dir 将归档历史复用场景与其他存储测试隔离。
        let temp_dir = std::env::temp_dir().join(format!(
            "vulcan-task-session-archived-reuse-test-{}",
            SessionNodeId::new_v4()
        ));
        // store is the real segmented store under test.
        // store 是被测的真实分段存储。
        let store = super::SegmentedTaskSessionStore::new(temp_dir.clone());
        // root_session_id owns the direct parent scope used by both generations.
        // root_session_id 拥有两代记录共同使用的直接父级作用域。
        let root_session_id = SessionNodeId::new_v4();
        // parent is the exact identity scope reused after archival.
        // parent 是归档后被复用的精确身份作用域。
        let parent = ParentSessionRef::MainSession { root_session_id };
        // archived_task_session_id identifies the historical generation.
        // archived_task_session_id 标识历史代记录。
        let archived_task_session_id = SessionNodeId::new_v4();
        // open_task_session_id identifies the sole current owner.
        // open_task_session_id 标识当前唯一所有者。
        let open_task_session_id = SessionNodeId::new_v4();
        // archived_metadata starts as a valid record before being explicitly archived.
        // archived_metadata 先作为有效记录创建，再被显式归档。
        let mut archived_metadata = segmented_store_identity_test_metadata(
            root_session_id,
            archived_task_session_id,
            parent.clone(),
            "reused-slot",
            100,
        );
        store
            .create_task_session(archived_metadata.clone())
            .expect("historical task metadata should be created");
        archived_metadata.status = TaskSessionStatus::Archived;
        store
            .save_task_session(archived_metadata)
            .expect("historical task metadata should be archived");

        // open_metadata is the current generation that owns the reused public slot.
        // open_metadata 是拥有复用公开槽位的当前代记录。
        let mut open_metadata = segmented_store_identity_test_metadata(
            root_session_id,
            open_task_session_id,
            parent.clone(),
            "reused-slot",
            200,
        );
        store
            .create_task_session(open_metadata.clone())
            .expect("current task metadata should be created");
        open_metadata.status = TaskSessionStatus::Running;
        store
            .save_task_session(open_metadata)
            .expect("current task metadata should be marked running");

        // selected is required to be the sole non-archived owner, regardless of archived history.
        // selected 必须是唯一未归档所有者，不受归档历史影响。
        let selected = store
            .find_task_session_by_parent_and_public_id(parent, "reused-slot")
            .expect("reused public task id lookup should succeed")
            .expect("the current open owner should exist");
        assert_eq!(selected.task_session_id, open_task_session_id);
        assert_eq!(selected.status, TaskSessionStatus::Running);

        std::fs::remove_dir_all(temp_dir).expect("temporary task store should be removable");
    }

    /// Verifies that shared compaction results can be applied directly to segmented TaskSession context documents.
    /// 验证共享压缩结果可以直接应用到分段 TaskSession 上下文文档。
    #[test]
    fn task_session_compaction_result_updates_context_and_checkpoint() {
        let task_session_id = SessionNodeId::new_v4();
        let conversation_state = SessionConversationState {
            messages: vec![
                task_user_message("message-1", "oldest"),
                task_user_message("message-2", "middle"),
                task_user_message("message-3", "latest"),
            ],
            records: Vec::new(),
            continuation: Default::default(),
        };
        let context = SessionContextDocument {
            schema_version: 1,
            node_id: task_session_id,
            provider_id: Some("provider".to_string()),
            model_id: Some("model".to_string()),
            reasoning_effort: None,
            system_prompt_layers: vec!["system".to_string()],
            conversation_state: serde_json::to_value(conversation_state)
                .expect("conversation state should serialize"),
            compaction_anchor: None,
            context_budget_fingerprint: None,
            tool_catalog_snapshot: json!({ "tools": [] }),
            provider_request_preview: None,
        };
        let compaction_result = CompactionResult {
            summary_record: CompactionSummaryRecord {
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::CompactionSummary,
                title: "Context compaction".to_string(),
                body: "Earlier task context summarized.".to_string(),
                preview: "Earlier task context summarized.".to_string(),
            },
            source_until_message_id: Some("message-2".to_string()),
            source_until_turn_index: Some(1),
            retained_tail_start_turn_index: Some(2),
            compaction_id: "task-compaction-1".to_string(),
            created_at: "456".to_string(),
            policy: ContextCompactionPolicy::default(),
        };

        let application = apply_task_session_compaction_result(context, &compaction_result, 7)
            .expect("task compaction should apply");
        let compacted_state: SessionConversationState =
            serde_json::from_value(application.context.conversation_state.clone())
                .expect("compacted state should deserialize");

        assert_eq!(
            application.context.compaction_anchor.as_deref(),
            Some("task-summary-task-compaction-1")
        );
        assert_eq!(application.checkpoint.node_id, task_session_id);
        assert_eq!(application.checkpoint.checkpoint_id, "task-compaction-1");
        assert_eq!(application.checkpoint.through_sequence, 7);
        assert_eq!(application.checkpoint.timestamp_ms, 456);
        assert_eq!(
            application.checkpoint.summary,
            "Earlier task context summarized."
        );
        assert_eq!(compacted_state.messages.len(), 2);
        assert!(compacted_state.messages[0].id.starts_with("task-summary-"));
        assert_eq!(compacted_state.messages[1].id, "message-3");
    }

    /// Builds one simple task-session user message for compaction tests.
    /// 为压缩测试构建一条简单的 TaskSession 用户消息。
    fn task_user_message(id: &str, text: &str) -> SessionConversationMessage {
        SessionConversationMessage {
            id: id.to_string(),
            payload: SessionConversationPayload::UserText {
                text: text.to_string(),
                attachments: Vec::new(),
            },
        }
    }
}
