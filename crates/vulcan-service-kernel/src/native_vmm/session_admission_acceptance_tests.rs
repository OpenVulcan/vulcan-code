//! Real direct-write admission, archive preservation and cross-session deduplication acceptance.
//! 真实主动写入准入、归档保留及跨会话去重验收。
use super::*;

/// Checks actual profile reads against binding and linked-scope changes without touching memory contents.
/// 核对实际画像读取对绑定及连带范围变化的校验，不修改记忆内容。
/// Takes isolated production facades and the fixture owner; restores its original owner-only access.
/// 接收隔离生产门面及夹具归属；恢复其原有仅归属项目访问范围。
async fn verify_profile_read_binding(
    client: &NativeVmmClient,
    runtime: &NativeVmmCoreRuntime,
    maintenance: &crate::native_vmm::NativeVmmMaintenance,
    project_id: u64,
) {
    // The fixture was created with one owner and no linked projects immediately before this call.
    // 本调用前夹具刚以一个归属且无连带项目创建。
    let source = client
        .memory_read_source("direct-host", 1, 1)
        .await
        .expect("read source");
    assert_eq!(source.visible_project_ids, vec![project_id]);
    let before = runtime.storage_snapshot().expect("before profile reads");
    let request = vulcan_vmm::GetProfileNodesRequest {
        expected_read_source: Some(source.clone()),
        host_project_id: "direct-host".to_owned(),
        user_id: 1,
        project_id: 1,
        target: vulcan_vmm::ProfileTarget::ProfileTargetProject,
        limit: 50,
    };
    client
        .get_profile_nodes(request.clone())
        .await
        .expect("matching read");
    maintenance
        .set_project_access("direct-host", project_id, &[1])
        .await
        .expect("link default");
    let changed = client
        .memory_read_source("direct-host", 1, 1)
        .await
        .expect("linked source");
    assert_eq!(changed.owner, source.owner);
    assert_eq!(changed.visible_project_ids, vec![project_id, 1]);
    assert_eq!(
        client
            .get_profile_nodes(request.clone())
            .await
            .expect_err("reject old scope")
            .kind,
        "memory_read_source_changed"
    );
    client
        .get_profile_nodes(vulcan_vmm::GetProfileNodesRequest {
            expected_read_source: Some(changed),
            ..request.clone()
        })
        .await
        .expect("new scope read");
    maintenance
        .bind_existing_project("direct-host", 1)
        .await
        .expect("rebind owner");
    assert_eq!(
        client
            .get_profile_nodes(request.clone())
            .await
            .expect_err("reject old owner")
            .kind,
        "memory_read_source_changed"
    );
    maintenance
        .set_project_access("direct-host", project_id, &[])
        .await
        .expect("restore access");
    client
        .get_profile_nodes(request)
        .await
        .expect("restored scope");
    assert_eq!(
        runtime.storage_snapshot().expect("after profile reads"),
        before
    );
}

/// Executes direct writes without PostAction and verifies persistent session identity and unchanged watermarks.
/// 不经过 PostAction 执行主动写入，验证持久化会话身份及未推进水位。
/// Takes isolated production facades and AI capture; leaves only fixture-owned records in the temporary data plane.
/// 接收隔离生产门面及 AI 捕获；仅在临时数据面保留夹具拥有的记录。
pub(super) async fn verify(
    client: &NativeVmmClient,
    runtime: &NativeVmmCoreRuntime,
    data_plane: &VmmDataPlaneHandle,
    maintenance: &crate::native_vmm::NativeVmmMaintenance,
    port: &CapturingAnalysisPort,
) {
    // A new exact owner prevents pre-existing extracted turns from concealing missing admission.
    // 全新准确归属防止既有提炼回合掩盖缺失准入。
    let project = data_plane
        .create_project("Direct", "direct")
        .await
        .expect("direct project");
    data_plane
        .bind_vulcan_project("direct-host", project.id)
        .await
        .expect("direct binding");
    let scope = super::super::project_scope_id(project.id).expect("scope");
    // A stale delivery must fail before session registration, queue insertion, or any AI admission.
    // 旧来源投递必须在会话登记、队列插入及任何 AI 准入前失败。
    let before_stale = runtime.storage_snapshot().expect("before stale delivery");
    let stale_error = client
        .post_action(vulcan_vmm::PostActionRequest {
            expected_source: Some(vulcan_vmm::VmmMemorySource::native(1).expect("old source")),
            host_project_id: "direct-host".to_owned(),
            project_id: 1,
            user_id: 1,
            session_id: "must-not-register-stale-delivery".to_owned(),
            operation_id: "stale-delivery".to_owned(),
            user_content: "旧来源内容不得进入重新绑定后的记忆项目。".to_owned(),
            ..Default::default()
        })
        .await
        .expect_err("reject stale source before intake");
    assert_eq!(stale_error.kind, "memory_source_changed");
    assert_eq!(
        runtime.storage_snapshot().expect("after rejected delivery"),
        before_stale
    );
    verify_writeback_rebinding(client, runtime, maintenance, project.id).await;
    verify_profile_read_binding(client, runtime, maintenance, project.id).await;
    let request = WriteMemoriesRequest {
        host_project_id: "direct-host".to_owned(),
        project_id: project.id,
        user_id: 1,
        session_id: "direct-only".to_owned(),
        items: vec![vulcan_vmm::WriteMemoryItem {
            scope_level: 2,
            category: 0,
            priority: 2,
            memory_level: 3,
            abstract_text: "项目采用 PostgreSQL".to_owned(),
            details: "这是用户主动记录的项目数据库决定。".to_owned(),
        }],
    };
    let first = client
        .write_memories(request.clone())
        .await
        .expect("first direct write");
    assert_eq!(first.items.len(), 1);
    assert!(!first.items[0].deduped);
    let records = maintenance
        .memory_records(project.id, Some("direct-only"))
        .await
        .expect("direct records");
    assert!(records.turns.is_empty());
    assert_eq!(records.memories.len(), 1);
    assert_eq!(records.sessions.len(), 1);
    // The lightweight list must agree exactly with the established provenance view without loading its bodies.
    // 轻量列表必须与既有来源视图准确一致，但不加载其正文。
    let (list_version, listed_sessions) = maintenance
        .session_summaries(project.id)
        .await
        .expect("lightweight sessions");
    assert_eq!(listed_sessions, records.sessions);
    assert_eq!(
        list_version,
        runtime
            .storage_snapshot()
            .expect("list version")
            .storage_version
    );
    assert_eq!(
        records.sessions[0].status,
        Some(vulcan_memorymesh_core::StorageSessionStatus::Active)
    );
    let preview = maintenance
        .preview_session_archive(vulcan_service_api::VmmNativeSessionArchiveSelection {
            vmm_project_id: project.id,
            session_source_ids: vec!["direct-only".to_owned()],
            archived: true,
        })
        .await
        .expect("direct archive preview");
    maintenance
        .execute_session_archive("archive-direct-session", &preview.preview_id)
        .await
        .expect("direct archive");
    let before_retry = runtime.storage_snapshot().expect("before retry");
    // Existing archived admission keeps concurrent readers available while still excluding maintenance writes.
    // 既有归档会话准入保持并发读取可用，同时仍排斥维护写入。
    let (guard, access) = client
        .admit_memory_session(
            "direct-host",
            project.id,
            &super::super::subject_id(1).expect("subject"),
            &SessionSourceId::new("direct-only").expect("session"),
            None,
            None,
        )
        .await
        .expect("existing admission");
    assert_eq!(access.write_scope, scope);
    assert!(client.operation_gate.try_read().is_ok());
    assert!(client.operation_gate.try_write().is_err());
    drop(guard);
    assert_eq!(
        runtime.storage_snapshot().expect("read-only admission"),
        before_retry
    );
    // Hold the actual native mutation gate so the delivery is waiting before any session registration.
    // 持有实际原生变更守卫，使投递在任何会话登记之前等待。
    let blocked = client.operation_gate.write().await;
    let permit = vulcan_vmm::VmmWritebackPermit::new().expect("delivery permit");
    let delayed_request = vulcan_vmm::PostActionRequest {
        expected_source: Some(vulcan_vmm::VmmMemorySource::native(project.id).expect("same owner")),
        host_project_id: "direct-host".to_owned(),
        project_id: 1,
        user_id: 1,
        session_id: "cancelled-while-waiting".to_owned(),
        operation_id: "cancelled-while-waiting".to_owned(),
        user_content: "已撤销的写回不能因目的地 ID 相同而恢复。".to_owned(),
        ..Default::default()
    };
    let mut delivery = Box::pin(client.post_action_with_permit(delayed_request, &permit));
    // Poll delivery first to establish a real pending lock wait without a timer-based assumption.
    // 先轮询投递以建立真实待锁状态，不以定时器猜测。
    tokio::select! {
        biased;
        result = &mut delivery => panic!("delivery crossed held mutation gate: {result:?}"),
        () = std::future::ready(()) => {}
    }
    permit.revoke();
    drop(blocked);
    assert_eq!(
        delivery.await.expect_err("cancel after waiting").kind,
        "writeback_cancelled"
    );
    assert_eq!(
        runtime
            .storage_snapshot()
            .expect("cancelled admission unchanged"),
        before_retry
    );
    let embedding_count = port
        .embedding_requests
        .lock()
        .expect("embedding count")
        .len();
    let repeated = client
        .write_memories(request.clone())
        .await
        .expect("deduplicated direct write");
    assert!(repeated.items[0].deduped);
    assert_eq!(repeated.items[0].memory_id, first.items[0].memory_id);
    assert_eq!(
        runtime.storage_snapshot().expect("after retry"),
        before_retry
    );
    assert_eq!(
        port.embedding_requests
            .lock()
            .expect("embedding count")
            .len(),
        embedding_count
    );

    // Semantic deduplication may return another session's record, but must still register the caller.
    // 语义去重可能返回另一会话的记录，但仍须登记调用方。
    let cross_session = client
        .write_memories(WriteMemoriesRequest {
            session_id: "dedup-only".to_owned(),
            ..request
        })
        .await
        .expect("cross-session dedupe");
    assert!(cross_session.items[0].deduped);
    assert_eq!(cross_session.items[0].memory_id, first.items[0].memory_id);
    let after = runtime.storage_snapshot().expect("after cross-session");
    let sessions = after
        .memory_sessions
        .iter()
        .filter(|session| session.scope_id == scope)
        .collect::<Vec<_>>();
    assert_eq!(sessions.len(), 2);
    for session in &sessions {
        assert_eq!(session.last_extraction_observed_at_ns, 0);
        assert_eq!(session.last_extraction_completed_at_ns, None);
    }
    assert_eq!(
        sessions
            .iter()
            .find(|session| session.session_source_id.as_str() == "direct-only")
            .expect("original")
            .status,
        vulcan_memorymesh_core::StorageSessionStatus::Archived
    );
    assert_eq!(
        sessions
            .iter()
            .find(|session| session.session_source_id.as_str() == "dedup-only")
            .expect("caller")
            .status,
        vulcan_memorymesh_core::StorageSessionStatus::Active
    );
    assert_eq!(after.dataset.memories, before_retry.dataset.memories);
    assert_eq!(after.dataset.turns, before_retry.dataset.turns);
    let durable: vulcan_memorymesh_core::StorageSnapshot = serde_json::from_str(
        &data_plane
            .load_core_snapshot_json()
            .await
            .expect("durable snapshot")
            .expect("stored"),
    )
    .expect("parse durable");
    assert_eq!(durable, after.without_embeddings());
    // A successful no-recall PreCheck and an empty compact still create independently manageable sessions.
    // 成功但无需召回的 PreCheck 及空压缩仍创建可独立管理的会话。
    let precheck = client
        .pre_check(PreCheckRequest {
            host_project_id: "direct-host".to_owned(),
            project_id: project.id,
            user_id: 1,
            session_id: "precheck-only".to_owned(),
            operation_id: "admit-precheck".to_owned(),
            user_content: "你好。".to_owned(),
            ..Default::default()
        })
        .await
        .expect("precheck admission");
    assert!(!precheck.should_inject);
    assert_eq!(precheck.session_memory_status, SessionMemoryStatus::Active);
    assert_eq!(
        precheck.read_source.owner,
        vulcan_vmm::VmmMemorySource::native(project.id).expect("admitted owner")
    );
    assert_eq!(precheck.read_source.visible_project_ids, vec![project.id]);
    let compact = client
        .chat_compact(ChatCompactRequest {
            host_project_id: "direct-host".to_owned(),
            project_id: project.id,
            user_id: 1,
            session_id: "compact-only".to_owned(),
            operation_id: "admit-compact".to_owned(),
        })
        .await
        .expect("empty compact admission");
    assert!(!compact.updated);
    assert_eq!(compact.compacted_turn_id, 0);
    for name in ["precheck-only", "compact-only"] {
        let records = maintenance
            .memory_records(project.id, Some(name))
            .await
            .expect("empty registered records");
        assert!(records.turns.is_empty());
        assert!(records.memories.is_empty());
        assert_eq!(
            records
                .sessions
                .iter()
                .find(|session| session.session_source_id == name)
                .expect("registered session")
                .status,
            Some(vulcan_memorymesh_core::StorageSessionStatus::Active)
        );
        let preview = maintenance
            .preview_session_archive(vulcan_service_api::VmmNativeSessionArchiveSelection {
                vmm_project_id: project.id,
                session_source_ids: vec![name.to_owned()],
                archived: true,
            })
            .await
            .expect("empty session archive preview");
        maintenance
            .execute_session_archive(&format!("archive-{name}"), &preview.preview_id)
            .await
            .expect("empty session archive");
    }
}

/// Uses real maintenance bindings to prove unchanged owners survive while an A-to-default-to-A roundtrip cancels old work.
/// 使用真实维护绑定，证明相同归属保留许可，而 A 到 default 再到 A 会取消旧工作。
/// Takes the isolated production client, runtime, maintenance and actual owner ID; leaves binding and stored memory unchanged.
/// 接收隔离生产客户端、运行时、维护器及实际归属 ID；结束时绑定及已存记忆保持不变。
async fn verify_writeback_rebinding(
    client: &NativeVmmClient,
    runtime: &NativeVmmCoreRuntime,
    maintenance: &crate::native_vmm::NativeVmmMaintenance,
    project_id: u64,
) {
    use vulcan_agent_runtime::agent_runtime::conversation_hooks::{
        HOOK_CONVERSATION_TURN_COMPLETED, VmmWritebackHookEffect,
    };
    use vulcan_agent_runtime::agent_runtime::vmm_hooks::{
        VmmHookConfigSnapshot, VmmHookRuntimeHandle,
    };
    // The real hook owner is installed before cloning the maintenance facade used for this fixture.
    // 在克隆本夹具使用的维护门面之前安装真实 Hook 归属方。
    let hooks = VmmHookRuntimeHandle::new(VmmHookConfigSnapshot {
        global: vulcan_vmm::VmmConfigLayer {
            backend_mode: Some(vulcan_vmm::VmmBackendMode::Native),
            enabled: Some(true),
            ..Default::default()
        },
        ..Default::default()
    });
    let maintenance = maintenance.clone().with_writeback_runtime(hooks.clone());
    let session = uuid::Uuid::from_u128(9160601);
    let invocation = vulcan_hooks::HookInvocation {
        registration: vulcan_hooks::HookRegistration::new(
            "native-binding-writeback-test",
            HOOK_CONVERSATION_TURN_COMPLETED,
            vulcan_hooks::HookMode::Effect,
        ),
        context: vulcan_hooks::HookContext::new(
            "native-binding-writeback-test",
            HOOK_CONVERSATION_TURN_COMPLETED,
        ),
        payload: vulcan_hooks::HookPayload {
            value: serde_json::json!({ "session_id": session.to_string(), "project_id": "direct-host" }),
        },
    };
    let source = client
        .memory_source("direct-host", 1, 1)
        .await
        .expect("actual source");
    assert_eq!(
        source,
        vulcan_vmm::VmmMemorySource::native(project_id).expect("actual project")
    );
    let old = hooks
        .admit_writeback(&invocation, &source)
        .expect("old admission");
    let entry = vulcan_workbench_model::VmmWritebackOutboxEntry {
        permit_id: old.id(),
        id: format!("pending-{}", old.id()),
        request: vulcan_vmm::PostActionRequest {
            expected_source: Some(source.clone()),
            host_project_id: "direct-host".to_owned(),
            session_id: session.to_string(),
            user_id: 1,
            project_id: 1,
            ..Default::default()
        },
        attempt_count: 1,
        last_error: Some("offline".to_owned()),
        created_at: "1".to_owned(),
        updated_at: "1".to_owned(),
    };
    hooks
        .replace_session_projection(
            session,
            &Default::default(),
            std::slice::from_ref(&entry),
            None,
        )
        .expect("pending mirror");
    let before = runtime.storage_snapshot().expect("before binding changes");
    maintenance
        .bind_existing_project("direct-host", project_id)
        .await
        .expect("same binding");
    assert!(old.ensure_active().is_ok());
    maintenance
        .bind_existing_project("direct-host", 1)
        .await
        .expect("bind protected default");
    maintenance
        .bind_existing_project("direct-host", project_id)
        .await
        .expect("restore binding");
    assert_eq!(
        client
            .memory_source("direct-host", 1, 1)
            .await
            .expect("restored actual source"),
        source
    );
    assert!(old.ensure_active().is_err());
    assert!(
        hooks
            .current_writeback_outbox(session, std::slice::from_ref(&entry))
            .expect("cancelled mirror")
            .is_empty()
    );
    assert_eq!(
        client
            .post_action_with_permit(entry.request.clone(), &old)
            .await
            .expect_err("old request remains cancelled")
            .kind,
        "writeback_cancelled"
    );
    let fresh = hooks
        .admit_writeback(&invocation, &source)
        .expect("fresh generation");
    assert_ne!(fresh.id(), old.id());
    hooks
        .with_current_writeback_effects(
            session,
            vec![
                VmmWritebackHookEffect::Enqueue {
                    permit_id: old.id(),
                    request: entry.request,
                    error: "late failed result".to_owned(),
                },
                VmmWritebackHookEffect::SessionStatus {
                    permit_id: old.id(),
                    status: vulcan_vmm::SessionMemoryStatus::Active,
                },
            ],
            |effects| assert!(effects.is_empty()),
        )
        .expect("reject late effects");
    assert_eq!(
        runtime.storage_snapshot().expect("after binding roundtrip"),
        before
    );
}
