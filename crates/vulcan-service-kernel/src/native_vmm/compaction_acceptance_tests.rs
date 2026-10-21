//! Native nonempty memory across a durable V1 compaction completion.
//! 非空原生记忆跨持久 V1 压缩完成的验收。

use crate::agent_stream::provider_v1_sink::{
    ProviderV1CompletedAssistantText, ProviderV1FinishedDurability,
};
use crate::context_compaction::{
    ContextCompactionCommitMode, ContextCompactionFinishOutcome, WebuiContextCompactionExecution,
    finish_webui_context_compaction_execution,
};
use crate::session_runtime_registry_v1::{
    SessionFlushReason, SessionHandle, SessionMutationAdmission,
};
use crate::tests::test_support::{
    TestHostRuntimeRoot, test_agent_runtime_bridge, test_service_state_with_parent_session,
};
use std::sync::{Arc, Mutex};
use vulcan_agent_runtime::agent_runtime::{
    context_injection_runtime::core_provider_context_injection_from_state_for_turn,
    prompt_compaction::{PromptCompactionExecution, PromptCompactionExecutionProfile},
    vmm_hooks::VmmHookConfigSnapshot,
};
use vulcan_context::ContextCompactionPolicy;
use vulcan_context_compaction::{CompactionPlan, CompactionResult, CompactionTailSelectionReport};
use vulcan_service_api::ServiceEventEmitter;
use vulcan_session_core::v1::{
    document::{ApiCompletion, ClientInputSubmission, PrepareAndStartApiInput},
    event::{
        ApiCallStatus, ApiEndReason, ApiPurposeKind, ApiRequestSnapshot, ApiTimings,
        AssistantSemanticStatus, ContinuationEligibility, ProviderProjectionProof,
    },
    ids::{ClientActionId, Seq},
    projection::provider_visible_fingerprint,
    stats::UsageSnapshot,
};
use vulcan_vmm::VmmClient;
use vulcan_workbench_model::{SessionId, SessionRunPhase, SessionRuntimeStateEntry};
use vulcan_workspace_store::vmm_data_plane::VmmDataPlaneHandle;

#[path = "binding_cache_acceptance_tests.rs"]
mod binding_cache_acceptance;

/// Supplies the real native owners for the explicitly enabled background-Hook acceptance variant.
/// 为明确启用后台 Hook 的验收变体提供真实原生归属方。
pub(super) struct NativeHookAcceptance {
    /// The same client used by the fixture's public writes is installed in the built-in Hook runtime.
    /// 与夹具公开写入相同的客户端安装至内置 Hook 运行时。
    pub client: Arc<crate::native_vmm::client::NativeVmmClient>,
    /// The actual authoritative Core owner exposes committed boundary evidence.
    /// 实际权威 Core 归属方提供已提交边界证据。
    pub core: Arc<crate::native_vmm::driver::NativeVmmCoreRuntime>,
}

/// Commits a real V1 compaction while the native publication gate owns an existing nonempty memory.
/// 在原生发布门持有既有非空记忆时，提交真实 V1 压缩。
/// Uses isolated storage and the supplied real search hit; asserts durable cache and outgoing overlay identity.
/// 使用隔离存储及传入的真实搜索命中，断言持久缓存及传出覆盖层身份。
/// `native_hook` selects the separate installed-Hook variant; absence retains the original cache-only acceptance.
/// `native_hook` 选择独立的已安装 Hook 变体；缺省保留原始仅缓存验收。
pub(super) async fn verify(
    client: &crate::native_vmm::client::NativeVmmClient,
    data_plane: &VmmDataPlaneHandle,
    maintenance: &crate::native_vmm::NativeVmmMaintenance,
    project_id: u64,
    memory: &vulcan_vmm::MemorySearchHit,
    native_hook: Option<&NativeHookAcceptance>,
) {
    // The application project is bound explicitly; no user directory or display-name lookup is involved.
    // 显式绑定应用项目，不涉及用户目录或显示名称查找。
    data_plane
        .bind_vulcan_project("project-a", project_id)
        .await
        .expect("compaction binding");
    maintenance.set_native_backend_enabled(true);
    maintenance
        .validate_host_ai()
        .await
        .expect("native host AI admission");
    let session_id = SessionId::new_v4();
    let root = TestHostRuntimeRoot::new(session_id);
    let mut service = test_service_state_with_parent_session(session_id);
    // Load the initialized disk document so durable writes retain its active split tail.
    // 加载已初始化的磁盘文档，使持久写入保留其有效分片尾部。
    let loaded = vulcan_workspace_store::workspace_store::load_session_v1_for_runtime(
        &root.v1_storage_locator(),
    )
    .expect("load initialized V1 document");
    let handle = SessionHandle::new(session_id, root.v1_storage_locator(), loaded.document);
    service
        .session_runtime_registry
        .register(handle.clone())
        .expect("register V1");
    service.controller_runtime = Some(root.host_runtime_handle());
    service.native_vmm_maintenance = Some(maintenance.clone());
    // Readiness and Hook publication share the same explicit native configuration.
    // 就绪状态与 Hook 发布共享同一明确原生配置。
    let mut agent = test_agent_runtime_bridge();
    agent.provider_configuration.vmm.enabled = Some(true);
    agent.provider_configuration.vmm.backend_mode = Some(vulcan_vmm::VmmBackendMode::Native);
    if native_hook.is_some() {
        agent.provider_configuration.vmm.user_id = Some(1);
        agent.provider_configuration.vmm.project_id = Some(project_id);
    }
    agent
        .vmm_hook_runtime
        .replace_snapshot(VmmHookConfigSnapshot {
            global: agent.provider_configuration.vmm.clone(),
            ..Default::default()
        })
        .expect("native Hook config");
    // Only the independent variant installs the actual built-in dispatcher and admits a real pending turn.
    // 仅独立变体安装实际内置分发器并准入真实待提炼回合。
    let native_hook_baseline = if let Some(hook) = native_hook {
        agent
            .vmm_hook_runtime
            .install_native_client(hook.client.clone())
            .expect("install actual compaction client");
        {
            // Release the dispatcher lock before awaiting actual native intake.
            // 等待实际原生入库前释放分发器锁。
            let mut hooks = agent
                .conversation_hook_runtime
                .lock()
                .expect("compaction Hook dispatcher");
            vulcan_agent_runtime::agent_runtime::vmm_hooks::register_builtin_vmm_hooks(
                hooks.dispatcher_mut(),
                agent.vmm_hook_runtime.clone(),
            )
            .expect("register actual built-in VMM hooks");
        }
        hook.client
            .post_action(vulcan_vmm::PostActionRequest {
                host_project_id: "project-a".to_owned(),
                project_id,
                user_id: 1,
                session_id: session_id.to_string(),
                operation_id: "compaction-hook-pending-turn".to_owned(),
                user_content: "此项目采用 PostgreSQL 保存业务数据，压缩后仍须保留该决定。"
                    .to_owned(),
                assistant_content: "已确认数据库决定，后续继续使用 PostgreSQL。".to_owned(),
                ..Default::default()
            })
            .await
            .expect("actual pre-compaction native intake");
        Some((
            hook,
            hook.core
                .storage_snapshot()
                .expect("Core before compaction Hook"),
            data_plane
                .list_active_pending_analysis_jobs()
                .await
                .expect("pending queue before compaction Hook"),
        ))
    } else {
        None
    };
    service.agent_runtime = Some(agent.clone());
    service
        .workspace_snapshot
        .as_mut()
        .expect("workspace")
        .session_runtime_state
        .insert(session_id, SessionRuntimeStateEntry::default());
    let state = Arc::new(Mutex::new(service));
    assert!(
        crate::webui_vmm_runtime_readiness(&state)
            .expect("readiness")
            .service_ready
    );

    // The real document admits the input and API; no success receipt is fabricated from summary text.
    // 真实文档准入输入与 API，不从摘要文本伪造成功回执。
    handle
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("native-compaction-root").expect("action"),
                "keep this context",
                Vec::new(),
            ),
            2,
        )
        .expect("root input");
    let request = ApiRequestSnapshot::new("provider-1", "model-1");
    let preparation = handle
        .read(|document| {
            let through_seq = document.events().last().expect("root event").seq;
            PrepareAndStartApiInput {
                purpose: ApiPurposeKind::Compaction,
                request: request.clone(),
                request_budget_evidence:
                    crate::session_runtime_registry_v1::test_provider_request_budget_evidence(),
                projection: ProviderProjectionProof {
                    segment_generation: document.segment_generation(),
                    document_revision: document.memory_revision(),
                    retained_from_seq: Seq::try_new(1).expect("retained root"),
                    through_seq,
                    fingerprint_sha256: provider_visible_fingerprint(
                        document.events(),
                        through_seq,
                    )
                    .expect("fingerprint"),
                },
                locked_baseline: crate::agent_stream::provider_v1_sink::host_guided_locked_baseline(
                ),
                continuation_action_id: None,
                now_ms: 3,
            }
        })
        .expect("API preparation");
    let api = handle
        .prepare_and_start_api_durable(preparation)
        .expect("durable API")
        .value
        .api_call_id;
    let summary = "Keep the PostgreSQL decision and fruit preferences.";
    let semantic = handle
        .update_assistant_text(api, summary, AssistantSemanticStatus::Completed, 4)
        .expect("assistant semantic");
    let terminal = handle
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ApiCompleted,
            |document| {
                document.complete_api_group(
                    api,
                    ApiCompletion {
                        request,
                        usage: UsageSnapshot::default(),
                        status: ApiCallStatus::Completed,
                        end_reason: Some(ApiEndReason::CompletedSuccessfully),
                        error: None,
                        continuation_eligibility: ContinuationEligibility::Forbidden,
                        timing: ApiTimings {
                            completed_at_ms: Some(5),
                            ..Default::default()
                        },
                        provider_source: None,
                        completed_at_ms: 5,
                    },
                )
            },
        )
        .expect("durable API terminal");
    // Seed only a genuine stored memory; native visibility reconciliation must retain it.
    // 仅播种真实已存储记忆，原生可见性对账必须保留它。
    let publication = maintenance
        .prepare_recall_publication(Some("project-a"), &session_id.to_string())
        .await
        .expect("native publication");
    let source = publication.profile_read_source().expect("source");
    drop(publication);
    let controller = crate::vmm_session_controller::open_registered_session_vmm_controller(
        &state, session_id, "6",
    )
    .expect("VMM controller");
    let old_context = vulcan_vmm::VmmContextEpoch::new(1, None).expect("old context");
    controller
        .execute_command("before-compaction", "6", |candidate| {
            candidate.select_read_source(&session_id.to_string(), Some(&source), "6")?;
            candidate.synchronize_stable_anchor_rendering(true, "6")?;
            candidate.synchronize_context_epoch(&old_context, "6")?;
            candidate.apply_recall_batch(
                &session_id.to_string(),
                "v1-provider-message-1",
                &vulcan_vmm::VmmRecallBatch {
                    ttl_turns: 3,
                    injection_tag: "VMM_MEMORY".to_owned(),
                    injection_description: "Relevant memories.".to_owned(),
                    queries: Vec::new(),
                    items: vec![vulcan_vmm::ContextItem {
                        memory_id: memory.memory_id,
                        text: memory.abstract_text.clone(),
                        ..Default::default()
                    }],
                },
                "6",
            )?;
            Ok(())
        })
        .expect("seed real recalled memory");
    assert!(
        !controller
            .snapshot()
            .expect("old cache")
            .cache_epoch
            .anchors
            .is_empty()
    );
    // A bounded channel owns the test stream; dropping the sender also terminates it on assertion failure.
    // 有界通道持有测试流；断言失败时发送端析构也会使其终结。
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let task_id = uuid::Uuid::new_v4();
    crate::spawn_registered_active_stream_task_with_mode(
        &state,
        session_id,
        task_id,
        None,
        crate::ActiveStreamTaskRegistrationMode::RejectIfOccupied,
        async {
            let _ = stopped.await;
        },
    )
    .expect("compaction stream owner");
    let policy = ContextCompactionPolicy::default();
    let plan = CompactionPlan {
        source_messages: Vec::new(),
        source_until_message_id: None,
        source_until_turn_index: None,
        retained_tail_start_turn_index: None,
        compaction_id: "display-only".to_owned(),
        created_at: "6".to_owned(),
        policy,
        tail_selection: CompactionTailSelectionReport {
            context_limit: policy.tail_context_limit,
            policy_source: policy.tail_policy_source,
            nominal_token_budget: policy.preserve_recent_tokens,
            segment_limit: policy.tail_turns,
            retained_segment_count: 1,
            retained_message_ids: vec!["v1-provider-message-2".to_owned()],
            retained_token_estimate: 0,
            previewed_tool_output_count: 0,
            omitted_tool_output_token_estimate: 0,
            newest_segment_exceeds_nominal_budget: false,
        },
    };
    // Only the public compaction facts are supplied; the private logical-turn receipt remains absent.
    // 仅提供公开压缩事实，私有逻辑回合回执保持缺失。
    let mut durability = ProviderV1FinishedDurability::default();
    durability.api_terminal = Some(terminal);
    durability.completed_assistant_text = Some(ProviderV1CompletedAssistantText {
        api_call_id: api,
        seq: semantic.value,
        text: summary.to_owned(),
    });
    durability.locked_baseline =
        Some(crate::agent_stream::provider_v1_sink::host_guided_locked_baseline());
    let outcome = finish_webui_context_compaction_execution(
        &ServiceEventEmitter::new(Vec::new()),
        &state,
        &agent,
        session_id,
        SessionRunPhase::Ready,
        "compaction-section",
        "test budget",
        crate::context_compaction::MANUAL_CONTEXT_COMPACTION_TRIGGER,
        ContextCompactionCommitMode::FinishActiveStream { task_id },
        WebuiContextCompactionExecution {
            execution: PromptCompactionExecution {
                compaction_result: CompactionResult::from_model_summary(&plan, summary),
                fallback_used: false,
                provider_request_count: 1,
                last_successful_provider_usage: None,
                provider_id: None,
                model_id: None,
                execution_profile: PromptCompactionExecutionProfile::CachePreservingFullFidelity,
                input_prune_report: None,
            },
            // Retain assistant-only context to prove no synthetic user is required for memory continuity.
            // 保留仅有助手的上下文，证明记忆连续性不依赖合成用户。
            retained_from_seq: semantic.value,
            durability,
        },
    )
    .await
    .expect("native compaction completion");
    let _ = stop.send(());
    assert!(matches!(
        outcome,
        ContextCompactionFinishOutcome::Committed { .. }
    ));
    if let Some((hook, before, pending_jobs)) = native_hook_baseline {
        // Wait for the real asynchronous notification's committed effect, never call ChatCompact from the test.
        // 等待真实异步通知的已提交效果，测试绝不直接调用 ChatCompact。
        let after = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                // Each read observes the authoritative Core rather than a synthetic Hook receipt.
                // 每次读取都观察权威 Core，而非合成 Hook 回执。
                let snapshot = hook
                    .core
                    .storage_snapshot()
                    .expect("observe compaction Hook");
                if snapshot
                    .dataset
                    .chat_compact_boundaries
                    .iter()
                    .any(|boundary| boundary.session_source_id.as_str() == session_id.to_string())
                {
                    break snapshot;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("built-in compaction Hook must commit its native boundary");
        // Resolve the exact native turn admitted for this UUID before the V1 completion.
        // 解析 V1 完成前为此 UUID 准入的准确原生回合。
        let turns = before
            .dataset
            .turns
            .iter()
            .filter(|turn| turn.session_source_id.as_str() == session_id.to_string())
            .collect::<Vec<_>>();
        assert_eq!(turns.len(), 1);
        assert_eq!(pending_jobs.len(), 1);
        assert_eq!(after.dataset.chat_compact_boundaries.len(), 1);
        // The boundary's owner and turn must come from actual intake, not from the latest UI message ID.
        // 边界归属与回合必须来自实际入库，而非最新界面消息 ID。
        let boundary = &after.dataset.chat_compact_boundaries[0];
        assert_eq!(boundary.subject_id, turns[0].subject_id);
        assert_eq!(boundary.scope_id, turns[0].scope_id);
        assert_eq!(boundary.session_source_id, turns[0].session_source_id);
        assert_eq!(boundary.compacted_turn_id, turns[0].id);
        assert!(boundary.compacted_at_ns >= turns[0].created_at_ns);
        assert_eq!(after.dataset.turns, before.dataset.turns);
        assert_eq!(after.dataset.memories, before.dataset.memories);
        assert_eq!(after.dataset.profile_nodes, before.dataset.profile_nodes);
        assert_eq!(
            after.dataset.context_evidence,
            before.dataset.context_evidence
        );
        assert_eq!(after.memory_sessions, before.memory_sessions);
        assert_eq!(after.analysis_attempts, before.analysis_attempts);
        assert_eq!(
            data_plane
                .list_active_pending_analysis_jobs()
                .await
                .expect("pending queue after compaction Hook"),
            pending_jobs
        );
        // The same boundary must already be present in the real durable Core document.
        // 同一边界必须已存在于真实耐久 Core 文档。
        let durable: vulcan_memorymesh_core::StorageSnapshot = serde_json::from_str(
            &data_plane
                .load_core_snapshot_json()
                .await
                .expect("durable compaction Hook snapshot")
                .expect("Core document"),
        )
        .expect("decode compaction Hook snapshot");
        assert_eq!(durable, after.without_embeddings());
    }
    let projection = handle
        .capture_controller_projection_snapshot()
        .expect("committed projection");
    assert!(!projection.items.iter().any(|item| matches!(
        item,
        vulcan_session_core::v1::projection::ProjectedItem::UserInput { .. }
    )));
    let context = vulcan_vmm::VmmContextEpoch::new(
        projection.segment_generation,
        projection
            .current_compaction_id
            .map(|id| id.value().to_string()),
    )
    .expect("committed context");
    let cache = controller.snapshot().expect("committed cache");
    assert_eq!(cache.cache_epoch.epoch_id, context.cache_epoch_id());
    assert!(cache.cache_epoch.anchors.is_empty());
    assert_eq!(cache.active_leases.len(), 1);
    assert!(cache.has_committed_controller_command("before-compaction"));
    let injection = core_provider_context_injection_from_state_for_turn(
        &Default::default(),
        &cache,
        None,
        true,
    )
    .expect("post-compaction memory overlay");
    assert!(injection.anchored_prefixes.is_empty());
    assert!(
        injection
            .persistent_blocks
            .iter()
            .any(|block| block.contains(&memory.abstract_text))
    );
    assert_eq!(
        injection.cache_epoch_id.as_deref(),
        Some(context.cache_epoch_id().as_str())
    );
    // Inspect the actual outgoing message array, not only the intermediate overlay.
    // 检查实际传出消息数组，而非仅检查中间覆盖层。
    // Persistent memory enters the wire through durable dynamic context, so publish it first.
    // 持久记忆经由可靠动态上下文进入线上请求，因此先发布它。
    drop(projection);
    let physical =
        crate::agent_stream::provider_v1_sink::rebuild_memory_acceptance_request_for_handle(
            &handle, injection,
        )
        .expect("physical request after compaction");
    let occurrences = physical
        .exact_request
        .messages
        .iter()
        .flat_map(|message| &message.content)
        .filter_map(|content| match content {
            vulcan_ai_contract::ContentItem::Text { text } => {
                Some(text.matches(&memory.abstract_text).count())
            }
            _ => None,
        })
        .sum::<usize>();
    assert_eq!(occurrences, 1, "active memory must occur once on the wire");
    // The canonical summary must not also be injected through the Agent's display wrapper.
    // 规范摘要不得再经 Agent 显示包装重复注入。
    let summary_occurrences = physical
        .exact_request
        .messages
        .iter()
        .flat_map(|message| &message.content)
        .filter_map(|content| match content {
            vulcan_ai_contract::ContentItem::Text { text } => Some(text.matches(summary).count()),
            _ => None,
        })
        .sum::<usize>();
    assert_eq!(summary_occurrences, 1, "canonical summary must occur once");
    // Canonical history carries active memory only through the typed persistent context update,
    // never through ordinary user or assistant content.
    // 规范历史只通过类型化持久上下文更新携带活动记忆，绝不经由普通用户或助手内容。
    let carriers = physical
        .controller_snapshot
        .messages
        .iter()
        .filter_map(|message| match &message.payload {
            vulcan_core::types::ConversationPayload::UserText { text, .. }
                if text.contains(&memory.abstract_text) =>
            {
                Some(Some(text.as_str()))
            }
            payload
                if serde_json::to_string(payload)
                    .expect("canonical payload")
                    .contains(&memory.abstract_text) =>
            {
                Some(None)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        carriers.len(),
        1,
        "canonical history must carry memory once"
    );
    assert!(
        carriers[0].is_some_and(|text| text.starts_with("<vulcan_context_update")
            && text.contains("injection.persistent")),
        "active memory must stay inside the persistent context update envelope"
    );
    binding_cache_acceptance::verify(
        &state,
        client,
        data_plane,
        maintenance,
        session_id,
        project_id,
        &source,
    )
    .await;
    // Restore the isolated fixture's initial disabled state before the existing recycle admission test.
    // 在既有回收准入测试之前，恢复隔离夹具初始的禁用状态。
    maintenance.set_native_backend_enabled(false);
}
