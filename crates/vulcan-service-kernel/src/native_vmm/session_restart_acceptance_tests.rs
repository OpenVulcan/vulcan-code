//! Real recycle, restore, permanent purge and explicit restart through product service actions.
//! 通过产品服务动作完成真实回收、恢复、永久清除及明确重新启用。
use super::*;
use vulcan_service_api::{
    ServiceEvent, ServiceEventEmitter, ServiceEventSink, VmmMemoryActionRequest,
    VmmMemoryActionResponse,
};

#[path = "session_archive_resident_acceptance_tests.rs"]
mod archive_resident_acceptance;

/// Captures actual service notifications emitted by native lifecycle commits.
/// 捕获原生生命周期提交发送的实际服务通知。
#[derive(Default)]
struct LifecycleEvents {
    /// Ordered production events from the synchronous host emitter.
    /// 来自同步宿主发送器的有序生产事件。
    events: StdMutex<Vec<ServiceEvent>>,
}

impl ServiceEventSink for LifecycleEvents {
    /// Records an owned event and returns after its synchronous capture.
    /// 记录事件所有权，并在同步捕获后返回。
    fn emit_service_event(&self, event: ServiceEvent) {
        self.events.lock().expect("lifecycle events").push(event);
    }
}

/// Invokes the real product management boundary and returns its typed result or exact rejection.
/// 调用真实产品管理边界，并返回类型化结果或准确拒绝。
/// Takes isolated state, emitter, stable command ID, action and payload; performs no response substitution.
/// 接收隔离状态、发送器、稳定命令 ID、动作及参数；不替换响应。
async fn action(
    state: &crate::state::SharedServiceBridgeState,
    app: &ServiceEventEmitter,
    command_id: &str,
    name: &str,
    payload: serde_json::Value,
) -> Result<VmmMemoryActionResponse, String> {
    // Poll each real service request as an independent task instead of nesting its stack in the full acceptance chain.
    // 将每个真实服务请求作为独立任务轮询，避免把其调用栈嵌入完整验收链。
    let result = tokio::spawn(crate::service_run_vmm_memory_action(
        app.clone(),
        state.clone(),
        VmmMemoryActionRequest {
            command_id: command_id.to_owned(),
            action: name.to_owned(),
            scope: "global".to_owned(),
            project_id: None,
            conversation_id: None,
            payload,
        },
    ))
    .await
    .expect("service action task must not panic");
    result.map(|result| result.data)
}

/// Proves a real paused session rejects intake and memory tools without changing storage, queue or AI usage.
/// 证明真实暂停会话拒绝入库及记忆工具，不改变存储、队列或 AI 用量。
/// Takes the actual client, capture port, exact owner/session and expected lifecycle status.
/// 接收实际客户端、捕获端口、准确归属及会话，以及预期生命周期状态。
async fn assert_paused(
    client: &NativeVmmClient,
    port: &CapturingAnalysisPort,
    project_id: u64,
    session_id: &str,
    expected: SessionMemoryStatus,
) {
    // Capture full authoritative data, not only the selected session count.
    // 捕获完整权威数据，而非仅捕获所选会话数量。
    let before = client.runtime.storage_snapshot().expect("paused storage");
    // Queue immutability is independent of the Core relationship snapshot.
    // 队列不可变性独立于 Core 关系快照。
    let queue = client
        .data_plane
        .list_active_pending_analysis_jobs()
        .await
        .expect("paused queue");
    // Verify both paid inference lanes remain untouched during rejected operations.
    // 验证拒绝操作期间两条计费推理通道均未触及。
    let calls = (
        port.requests.lock().expect("LLM count").len(),
        port.embedding_requests
            .lock()
            .expect("embedding count")
            .len(),
    );
    // These requests use the same session that the management action actually paused.
    // 这些请求使用管理动作实际暂停的同一会话。
    let precheck = client
        .pre_check(PreCheckRequest {
            host_project_id: "restart-owner".to_owned(),
            project_id,
            user_id: 1,
            session_id: session_id.to_owned(),
            operation_id: format!("paused-precheck-{expected:?}"),
            user_content: "Do not resume old memories.".to_owned(),
            ..Default::default()
        })
        .await
        .expect("paused precheck");
    assert_eq!(precheck.session_memory_status, expected);
    // Rejected completion must not create a second source turn or extraction job.
    // 被拒绝的回合完成不能创建第二条来源回合或提炼作业。
    let postaction = client
        .post_action(PostActionRequest {
            host_project_id: "restart-owner".to_owned(),
            project_id,
            user_id: 1,
            session_id: session_id.to_owned(),
            operation_id: format!("paused-postaction-{expected:?}"),
            user_content: "This must not enter memory.".to_owned(),
            assistant_content: "Do not store.".to_owned(),
            ..Default::default()
        })
        .await
        .expect("paused postaction");
    assert_eq!(postaction.session_memory_status, expected);
    assert!(!postaction.accepted);
    // The trusted runtime session also gates explicit memory writes.
    // 可信运行时会话同样限制显式记忆写入。
    let error = client
        .write_memories(WriteMemoriesRequest {
            host_project_id: "restart-owner".to_owned(),
            project_id,
            user_id: 1,
            session_id: session_id.to_owned(),
            items: vec![vulcan_vmm::WriteMemoryItem {
                abstract_text: "Rejected memory".to_owned(),
                details: "A paused session cannot write.".to_owned(),
                ..Default::default()
            }],
        })
        .await
        .expect_err("paused tool write");
    assert_eq!(error.kind, "session_memory_paused");
    // Search must fail before embeddings or any read-side memory effect can run.
    // 搜索必须在向量化或任何读取侧记忆效果运行之前失败。
    let error = client
        .search_memory_events(SearchMemoryEventsRequest {
            host_project_id: "restart-owner".to_owned(),
            project_id,
            user_id: 1,
            runtime_session_id: Some(session_id.to_owned()),
            queries: vec!["PostgreSQL".to_owned()],
            top_k: 1,
        })
        .await
        .expect_err("paused tool search");
    assert_eq!(error.kind, "session_memory_paused");
    assert_eq!(
        client
            .runtime
            .storage_snapshot()
            .expect("after paused calls"),
        before
    );
    assert_eq!(
        client
            .data_plane
            .list_active_pending_analysis_jobs()
            .await
            .expect("unchanged queue"),
        queue
    );
    assert_eq!(
        (
            port.requests.lock().expect("LLM count").len(),
            port.embedding_requests
                .lock()
                .expect("embedding count")
                .len()
        ),
        calls
    );
}

/// Reads the same typed session list used by the product page and checks explicit lifecycle classification.
/// 读取产品页面使用的同一类型化会话列表，并检查明确生命周期分类。
/// Takes exact project/session and expected status; asserts the live title and independent peer remain present.
/// 接收准确项目及会话和预期状态；断言活动标题及独立其他会话仍存在。
async fn assert_list_status(
    state: &crate::state::SharedServiceBridgeState,
    app: &ServiceEventEmitter,
    project_id: u64,
    session_id: &str,
    status: vulcan_service_api::VmmNativeSessionStatus,
) {
    // Serialize the public closed query, not guessed JSON filter names or defaults.
    // 序列化公开封闭查询，不猜测 JSON 筛选字段或默认值。
    let query = vulcan_service_api::VmmNativeSessionListQuery {
        vmm_project_id: project_id,
        status: vulcan_service_api::VmmNativeSessionListStatus::All,
        extraction_status: vulcan_service_api::VmmNativeSessionExtractionFilter::All,
        sort: vulcan_service_api::VmmNativeSessionListSort::LastActivityDesc,
        search: String::new(),
        last_activity_from_unix_ms: None,
        last_activity_before_unix_ms: None,
        offset: 0,
        limit: 20,
        expected_storage_version: None,
    };
    // A real service read must return the same page variant consumed by the WebUI.
    // 真实服务读取必须返回 WebUI 消费的相同分页变体。
    let result = action(
        state,
        app,
        &uuid::Uuid::new_v4().to_string(),
        "native_session_list",
        serde_json::to_value(query).expect("list query"),
    )
    .await
    .expect("real session list");
    let VmmMemoryActionResponse::NativeSessionList(page) = result else {
        panic!("session list response")
    };
    assert_eq!(page.total, 2);
    // Exact source identity distinguishes lifecycle state from zero body counts or similar display titles.
    // 准确来源身份将生命周期状态与零正文数量或相似显示标题区分开。
    let selected = page
        .sessions
        .iter()
        .find(|entry| entry.session_source_id == session_id)
        .expect("selected session");
    assert_eq!(selected.status, Some(status));
    assert_eq!(selected.display_title.as_deref(), Some("生命周期会话"));
    assert!(
        page.sessions
            .iter()
            .any(|entry| entry.session_source_id == "active-session")
    );
}

/// Validates preview replacement, stale rejection, mode checks, replay and future-only intake.
/// 验证预览替换、过期版本拒绝、模式检查、重放及仅未来入库。
/// Takes isolated production facades and AI capture; does not claim to exercise the pending purge workflow.
/// 接收隔离生产门面及 AI 捕获，不宣称覆盖尚待完成的永久清除流程。
pub(super) async fn verify(
    client: &NativeVmmClient,
    runtime: &NativeVmmCoreRuntime,
    data_plane: &VmmDataPlaneHandle,
    maintenance: &crate::native_vmm::NativeVmmMaintenance,
    port: &CapturingAnalysisPort,
) {
    // Ordinary no-recall intake creates exact metadata without inventing historical content.
    // 普通无需召回的准入创建准确元数据，不编造历史内容。
    let project = maintenance
        .create_and_bind_project("restart-owner", "重新启用验收")
        .await
        .expect("project");
    // Use the actual host UUID as Core session identity so the resident controller observes every transition.
    // 使用实际宿主 UUID 作为 Core 会话身份，使驻留控制器观察每次转换。
    let session_id = uuid::Uuid::new_v4();
    // The canonical text identity is passed unchanged through native requests and management selections.
    // 规范文本身份不经改变地传递给原生请求及管理选择。
    let restart_session = session_id.to_string();
    // Product management commands require UUIDs; retries must reuse these exact admitted identities.
    // 产品管理命令要求 UUID；重试必须复用这些准确准入身份。
    let purge_command = uuid::Uuid::new_v4().to_string();
    // Restart's UUID is shared by the service confirmation and later direct idempotency checks.
    // 重新启用的 UUID 由服务确认及后续直接幂等检查共用。
    let restart_command = uuid::Uuid::new_v4().to_string();
    for session in [restart_session.as_str(), "active-session"] {
        client
            .pre_check(PreCheckRequest {
                host_project_id: "restart-owner".to_owned(),
                project_id: project.id,
                user_id: 1,
                session_id: session.to_owned(),
                operation_id: format!("admit-{session}"),
                user_content: "你好。".to_owned(),
                ..Default::default()
            })
            .await
            .expect("admission");
    }
    // A normal intake supplies real nonempty source data and its durable queue row before any removal.
    // 在任何移除之前，通过普通入库提供真实非空来源数据及其持久队列行。
    let original = client
        .post_action(PostActionRequest {
            host_project_id: "restart-owner".to_owned(),
            project_id: project.id,
            user_id: 1,
            session_id: restart_session.clone(),
            operation_id: "restart-source-original".to_owned(),
            user_content: "原始项目采用 PostgreSQL 保存业务数据。".to_owned(),
            assistant_content: "记录原始配置。".to_owned(),
            ..Default::default()
        })
        .await
        .expect("original real intake");
    assert!(original.accepted);
    // Keep the real source job identity to distinguish permanent purge from a recoverable move.
    // 保留真实来源作业身份，以区分永久清除与可恢复移动。
    let original_job = data_plane
        .load_pending_analysis_job("restart-source-original")
        .await
        .expect("original queue read")
        .expect("original job");
    // Retain the exact removed ID to prove restart cannot recreate this original turn.
    // 保留准确移除身份，以证明重新启用不能重建此原始回合。
    let original_turn = original_job
        .turn_id
        .as_ref()
        .expect("original turn")
        .clone();
    // Each real V1 cache has its own isolated lifetime and declared storage locator.
    // 每个真实 V1 缓存拥有独立隔离生命周期及声明存储定位器。
    let root = crate::tests::test_support::TestControllerRuntimeRoot::new(session_id);
    // Service metadata uses the same real host binding as intake.
    // 服务元数据使用与入库相同的真实宿主绑定。
    let mut service =
        crate::tests::test_support::test_service_state_with_parent_session(session_id);
    // The management dispatcher reads this actual provider configuration, independently of controller cache state.
    // 管理派发器读取此实际供应商配置，独立于控制器缓存状态。
    let mut agent = crate::tests::test_support::test_agent_runtime_bridge();
    agent.provider_configuration.vmm.enabled = Some(true);
    agent.provider_configuration.vmm.backend_mode = Some(vulcan_vmm::VmmBackendMode::Native);
    service.agent_runtime = Some(agent);
    service.native_vmm_maintenance = Some(maintenance.clone());
    service
        .workspace_snapshot
        .as_mut()
        .expect("workspace")
        .sessions = vec![vulcan_workbench_model::SessionSummary::new(
        session_id,
        Some("restart-owner".to_owned()),
        "生命周期会话",
        "",
        vulcan_workbench_model::SessionState::Completed,
        0,
        0,
        "1",
    )];
    service
        .workspace_snapshot
        .as_mut()
        .expect("workspace")
        .session_runtime_state
        .insert(session_id, Default::default());
    service.workbench.selected_conversation_id = Some(restart_session.clone());
    // Derive the cache source from real admission instead of guessing its numeric project mapping.
    // 从真实准入派生缓存来源，不猜测其数字项目映射。
    let source = client
        .memory_read_source("restart-owner", 1, project.id)
        .await
        .expect("actual source");
    // Install only the sole exact-session controller; no history scan creates additional owners.
    // 只安装唯一准确会话控制器；不通过历史扫描创建其他归属。
    let controller = service
        .vmm_controller_registry
        .get_or_open(root.v1_storage_locator(), session_id, "1")
        .expect("resident controller");
    controller
        .execute_command("admitted-lifecycle-source", "1", |cached| {
            cached.select_read_source(&restart_session, Some(&source), "1")?;
            Ok(())
        })
        .expect("admitted cache source");
    // Keep the service alive across the observer and every actual management request.
    // 在观察器及每个实际管理请求期间保持服务存活。
    let state = Arc::new(StdMutex::new(service));
    // Capture actual service emissions rather than fabricating notification counts.
    // 捕获实际服务发送，而非虚构通知数量。
    let events = Arc::new(LifecycleEvents::default());
    // Both request dispatch and commit observation use the same production emitter.
    // 请求派发及提交观察使用同一个生产发送器。
    let app = ServiceEventEmitter::new(vec![events.clone()]);
    runtime
        .install_content_projection(&state, &app)
        .expect("lifecycle content observer");
    maintenance.set_native_backend_enabled(true);
    // Freeze all preexisting profile data so source deletion cannot silently take either profile role with it.
    // 冻结全部既有画像数据，防止来源删除静默连带任一画像角色。
    let profiles_before = runtime
        .storage_snapshot()
        .expect("profile baseline")
        .dataset
        .profile_nodes;
    // A complete real source snapshot proves a restore roundtrip before testing irreversible purge.
    // 完整真实来源快照在验证不可逆清除之前证明恢复往返。
    let source_before = runtime
        .storage_snapshot()
        .expect("source roundtrip baseline");
    let preview = action(
        &state,
        &app,
        &uuid::Uuid::new_v4().to_string(),
        "native_removal_preview",
        serde_json::to_value(vulcan_service_api::VmmNativeRemovalQuery {
            vmm_project_id: project.id,
            selection: vulcan_service_api::VmmNativeRemovalTarget::Sessions {
                ids: vec![restart_session.clone()],
            },
        })
        .expect("removal query"),
    )
    .await
    .expect("real removal preview");
    let VmmMemoryActionResponse::NativeRemovalPreview(preview) = preview else {
        panic!("typed removal preview")
    };
    assert!(
        action(
            &state,
            &app,
            &uuid::Uuid::new_v4().to_string(),
            "native_removal_recycle",
            serde_json::json!({"previewId":preview.preview_id,"confirmed":false})
        )
        .await
        .is_err()
    );
    let moved = action(
        &state,
        &app,
        &uuid::Uuid::new_v4().to_string(),
        "native_removal_recycle",
        serde_json::json!({"previewId":preview.preview_id,"confirmed":true}),
    )
    .await
    .expect("real recycle");
    let VmmMemoryActionResponse::NativeRemoval(moved) = moved else {
        panic!("typed removal result")
    };
    assert_eq!(
        controller
            .snapshot()
            .expect("immediate recycled cache")
            .session_memory_status,
        SessionMemoryStatus::Recycled
    );
    assert_eq!(
        data_plane
            .load_pending_analysis_job("restart-source-original")
            .await
            .expect("recoverable queue")
            .as_ref(),
        Some(&original_job)
    );
    assert_paused(
        client,
        port,
        project.id,
        &restart_session,
        SessionMemoryStatus::Recycled,
    )
    .await;
    assert_list_status(
        &state,
        &app,
        project.id,
        &restart_session,
        vulcan_service_api::VmmNativeSessionStatus::Recycled,
    )
    .await;
    // Restore through the product boundary first: active status, original source rows and queue must all return intact.
    // 先通过产品边界恢复：活动状态、原始来源行及队列必须全部完整返回。
    let restored = action(
        &state,
        &app,
        &uuid::Uuid::new_v4().to_string(),
        "native_recycle_restore",
        serde_json::json!({"batchId":moved.batch_id,"storageVersion":moved.storage_version}),
    )
    .await
    .expect("real source restore");
    assert!(matches!(
        restored,
        VmmMemoryActionResponse::NativeRecycleBatch(_)
    ));
    assert_eq!(
        controller
            .snapshot()
            .expect("immediate restored status")
            .session_memory_status,
        SessionMemoryStatus::Active
    );
    // Compare actual full content and session metadata, not merely record counts.
    // 比较实际完整内容及会话元数据，而非仅比较记录数量。
    let restored_storage = runtime.storage_snapshot().expect("restored source storage");
    assert_eq!(restored_storage.dataset, source_before.dataset);
    assert_eq!(
        restored_storage.memory_sessions,
        source_before.memory_sessions
    );
    assert_eq!(
        data_plane
            .load_pending_analysis_job("restart-source-original")
            .await
            .expect("restored queue")
            .as_ref(),
        Some(&original_job)
    );
    // The same paused query boundary now permits the exact restored source ID.
    // 同一暂停查询边界现在允许准确恢复来源身份。
    let details = client
        .get_turn_details(GetTurnDetailsRequest {
            runtime_session_id: Some(restart_session.clone()),
            host_project_id: "restart-owner".to_owned(),
            project_id: project.id,
            user_id: 1,
            turn_ids: vec![
                parse_core_numeric_id(&original_turn, "vmm-turn-").expect("restored numeric turn"),
            ],
        })
        .await
        .expect("restored source is readable");
    assert_eq!(details.turns.len(), 1);
    assert_list_status(
        &state,
        &app,
        project.id,
        &restart_session,
        vulcan_service_api::VmmNativeSessionStatus::Active,
    )
    .await;
    // Recycle again with a new real preview before exercising permanent removal of this same session.
    // 对同一会话使用新的真实预览再次回收，然后验证永久移除。
    let preview = action(
        &state,
        &app,
        &uuid::Uuid::new_v4().to_string(),
        "native_removal_preview",
        serde_json::to_value(vulcan_service_api::VmmNativeRemovalQuery {
            vmm_project_id: project.id,
            selection: vulcan_service_api::VmmNativeRemovalTarget::Sessions {
                ids: vec![restart_session.clone()],
            },
        })
        .expect("second removal query"),
    )
    .await
    .expect("second real removal preview");
    let VmmMemoryActionResponse::NativeRemovalPreview(preview) = preview else {
        panic!("second removal preview")
    };
    let moved = action(
        &state,
        &app,
        &uuid::Uuid::new_v4().to_string(),
        "native_removal_recycle",
        serde_json::json!({"previewId":preview.preview_id,"confirmed":true}),
    )
    .await
    .expect("second real recycle");
    let VmmMemoryActionResponse::NativeRemoval(moved) = moved else {
        panic!("second removal")
    };
    assert_eq!(
        controller
            .snapshot()
            .expect("recycled again")
            .session_memory_status,
        SessionMemoryStatus::Recycled
    );
    // A stale version cannot purge a different observation of the same recycle batch.
    // 过时版本不能永久清除同一回收批次的另一份观察。
    assert!(
        action(
            &state,
            &app,
            &uuid::Uuid::new_v4().to_string(),
            "native_recycle_purge",
            serde_json::json!({"batchId":moved.batch_id,"storageVersion":moved.storage_version-1})
        )
        .await
        .is_err()
    );
    let purged = action(
        &state,
        &app,
        &purge_command,
        "native_recycle_purge",
        serde_json::json!({"batchId":moved.batch_id,"storageVersion":moved.storage_version}),
    )
    .await
    .expect("real purge");
    assert!(matches!(
        purged,
        VmmMemoryActionResponse::NativeRecycleBatch(_)
    ));
    assert_eq!(
        controller
            .snapshot()
            .expect("immediate forgotten cache")
            .session_memory_status,
        SessionMemoryStatus::Forgotten
    );
    assert!(
        data_plane
            .load_pending_analysis_job("restart-source-original")
            .await
            .expect("purged queue")
            .is_none()
    );
    assert_eq!(
        action(
            &state,
            &app,
            &purge_command,
            "native_recycle_purge",
            serde_json::json!({"batchId":moved.batch_id,"storageVersion":moved.storage_version})
        )
        .await
        .expect("purge replay"),
        purged
    );
    assert_paused(
        client,
        port,
        project.id,
        &restart_session,
        SessionMemoryStatus::Forgotten,
    )
    .await;
    assert_list_status(
        &state,
        &app,
        project.id,
        &restart_session,
        vulcan_service_api::VmmNativeSessionStatus::Forgotten,
    )
    .await;
    // A worker that selected the old queue row before purge may not restore or reschedule it afterward.
    // 永久清除前已选中旧队列行的工作器，此后不能恢复或重新排期该行。
    let after_purge = runtime.storage_snapshot().expect("after real purge");
    assert!(
        maintenance
            .restore_recycle_batch(
                "cannot-restore-purged-session",
                &moved.batch_id,
                after_purge.storage_version
            )
            .await
            .is_err()
    );
    client
        .execute_pending_analysis_job(&original_job)
        .await
        .expect("stale worker stops");
    client
        .reschedule_pending_analysis_job(&original_job, "stale purged work")
        .await
        .expect("stale reschedule stops");
    assert_eq!(
        runtime.storage_snapshot().expect("no stale worker writes"),
        after_purge
    );
    assert!(
        data_plane
            .load_pending_analysis_job("restart-source-original")
            .await
            .expect("no resurrected queue")
            .is_none()
    );
    assert_eq!(
        runtime
            .storage_snapshot()
            .expect("preserved profiles")
            .dataset
            .profile_nodes,
        profiles_before
    );
    // This status now comes from the real service purge, never a fixture-only tombstone mutation.
    // 此状态现在来自真实服务永久清除，绝非仅用于夹具的墓碑变更。
    let scope = project_scope_id(project.id).expect("scope");
    let selection = vulcan_service_api::VmmNativeSessionRestartSelection {
        vmm_project_id: project.id,
        session_source_ids: vec![restart_session.clone()],
    };
    let replaced = maintenance
        .preview_session_restart(selection.clone())
        .await
        .expect("first preview");
    let stale = maintenance
        .preview_session_restart(selection.clone())
        .await
        .expect("replacement preview");
    assert!(
        maintenance
            .execute_session_restart("replaced-restart", &replaced.preview_id)
            .await
            .is_err()
    );
    // A separate legitimate classification change invalidates the earlier authoritative version.
    // 另一个合法分类变更使此前权威版本失效。
    let archive = maintenance
        .preview_session_archive(vulcan_service_api::VmmNativeSessionArchiveSelection {
            vmm_project_id: project.id,
            session_source_ids: vec!["active-session".to_owned()],
            archived: true,
        })
        .await
        .expect("archive preview");
    maintenance
        .execute_session_archive("restart-other-archive", &archive.preview_id)
        .await
        .expect("archive other");
    assert!(
        maintenance
            .execute_session_restart("stale-restart", &stale.preview_id)
            .await
            .is_err()
    );
    let before = runtime.storage_snapshot().expect("restart before");
    // Preserve the complete queue, including unrelated projects, instead of inferring job ownership.
    // 保留包含无关项目的完整队列，不推断作业归属。
    let jobs_before = data_plane
        .list_active_pending_analysis_jobs()
        .await
        .expect("queue before restart");
    let llm_count = port.requests.lock().expect("LLM count").len();
    let embedding_count = port
        .embedding_requests
        .lock()
        .expect("embedding count")
        .len();
    // The final preview also traverses the real service schema before explicit confirmation.
    // 最终预览也在明确确认前经过真实服务结构校验。
    let preview = action(
        &state,
        &app,
        &uuid::Uuid::new_v4().to_string(),
        "native_session_restart_preview",
        serde_json::to_value(selection.clone()).expect("restart selection"),
    )
    .await
    .expect("actual restart preview");
    let VmmMemoryActionResponse::NativeSessionRestartPreview(preview) = preview else {
        panic!("typed restart preview")
    };
    assert_eq!(
        runtime.storage_snapshot().expect("read-only preview"),
        before
    );
    maintenance.set_native_backend_enabled(false);
    assert!(
        maintenance
            .execute_session_restart("disabled-restart", &preview.preview_id)
            .await
            .is_err()
    );
    maintenance.set_native_backend_enabled(true);
    assert!(
        action(
            &state,
            &app,
            &uuid::Uuid::new_v4().to_string(),
            "native_session_restart",
            serde_json::json!({"previewId":preview.preview_id,"confirmed":false})
        )
        .await
        .is_err()
    );
    let result = action(
        &state,
        &app,
        &restart_command,
        "native_session_restart",
        serde_json::json!({"previewId":preview.preview_id,"confirmed":true}),
    )
    .await
    .expect("actual service restart");
    let VmmMemoryActionResponse::NativeSessionRestart(result) = result else {
        panic!("typed restart result")
    };
    assert_eq!(
        controller
            .snapshot()
            .expect("immediate active cache")
            .session_memory_status,
        SessionMemoryStatus::Active
    );
    assert_eq!(result.selection, selection);
    assert_list_status(
        &state,
        &app,
        project.id,
        &restart_session,
        vulcan_service_api::VmmNativeSessionStatus::Active,
    )
    .await;
    let mut expected = before;
    expected.storage_version += 1;
    expected
        .memory_sessions
        .iter_mut()
        .find(|session| {
            session.scope_id == scope
                && session.session_source_id.as_str() == restart_session.as_str()
        })
        .expect("expected session")
        .status = vulcan_memorymesh_core::StorageSessionStatus::Active;
    assert_eq!(runtime.storage_snapshot().expect("after restart"), expected);
    let durable: vulcan_memorymesh_core::StorageSnapshot = serde_json::from_str(
        &data_plane
            .load_core_snapshot_json()
            .await
            .expect("durable read")
            .expect("durable bytes"),
    )
    .expect("durable snapshot");
    assert_eq!(durable, expected.without_embeddings());
    assert_eq!(
        vulcan_workspace_store::workspace_store::load_session_vmm_state(&root.v1_storage_locator())
            .expect("read durable host lifecycle")
            .expect("host cache"),
        controller.snapshot().expect("current host cache")
    );
    assert_eq!(
        maintenance
            .execute_session_restart(&restart_command, &preview.preview_id)
            .await
            .expect("idempotent restart"),
        result
    );
    assert!(
        maintenance
            .execute_session_restart(&restart_command, "other-preview")
            .await
            .is_err()
    );
    assert!(
        maintenance
            .preview_session_restart(selection)
            .await
            .is_err()
    );
    assert_eq!(port.requests.lock().expect("LLM count").len(), llm_count);
    assert_eq!(
        port.embedding_requests
            .lock()
            .expect("embedding count")
            .len(),
        embedding_count
    );
    // Only a new ordinary PostAction can create a new turn; restart itself queued nothing.
    // 只有新的普通 PostAction 才能创建新回合，重新启用本身不入队任何内容。
    assert_eq!(
        data_plane
            .list_active_pending_analysis_jobs()
            .await
            .expect("queue before new input"),
        jobs_before
    );
    // The next real PreCheck publishes Active to the host hook before any new turn is persisted.
    // 下一次真实 PreCheck 在持久化新回合前向宿主 Hook 发布 Active 状态。
    let precheck = client
        .pre_check(PreCheckRequest {
            host_project_id: "restart-owner".to_owned(),
            project_id: project.id,
            user_id: 1,
            session_id: restart_session.clone(),
            operation_id: "precheck-after-restart".to_owned(),
            user_content: "你好。".to_owned(),
            ..Default::default()
        })
        .await
        .expect("precheck after restart");
    assert_eq!(precheck.session_memory_status, SessionMemoryStatus::Active);
    let response = client
        .post_action(PostActionRequest {
            host_project_id: "restart-owner".to_owned(),
            project_id: project.id,
            user_id: 1,
            session_id: restart_session.clone(),
            operation_id: "after-explicit-restart".to_owned(),
            user_content: "重新启用后，我决定项目使用 PostgreSQL 数据库。".to_owned(),
            assistant_content: "了解新的数据库决定。".to_owned(),
            ..Default::default()
        })
        .await
        .expect("new turn after restart");
    assert!(response.accepted);
    assert_eq!(response.session_memory_status, SessionMemoryStatus::Active);
    let records = maintenance
        .memory_records(project.id, Some(&restart_session))
        .await
        .expect("new records");
    assert_eq!(records.turns.len(), 1);
    assert_eq!(records.turns[0].extracted, Some(false));
    assert!(
        !runtime
            .storage_snapshot()
            .expect("no original resurrection")
            .dataset
            .turns
            .iter()
            .any(|turn| turn.id.as_str() == original_turn)
    );
    // Recycle, restore, recycle again, purge and restart each publish once; retries and reads publish no duplicates.
    // 回收、恢复、再次回收、永久清除及重新启用各发布一次；重试及读取不重复发布。
    {
        let captured = events.events.lock().expect("lifecycle event capture");
        assert_eq!(captured.len(), 5);
        for event in captured.iter() {
            let ServiceEvent::WorkbenchDelta(envelope) = event else {
                panic!("workbench event")
            };
            let vulcan_service_api::WorkbenchDelta::ContextInjectionChanged(delta) =
                &envelope.delta
            else {
                panic!("context event")
            };
            assert_eq!(delta.conversation_id, restart_session);
        }
    }
    // Keep the independent archive fixture's future off the enclosing lifecycle acceptance stack.
    // 使独立归档夹具的 Future 不占用外围生命周期验收调用栈。
    Box::pin(archive_resident_acceptance::verify(
        client,
        runtime,
        data_plane,
        maintenance,
        port,
    ))
    .await;
}
