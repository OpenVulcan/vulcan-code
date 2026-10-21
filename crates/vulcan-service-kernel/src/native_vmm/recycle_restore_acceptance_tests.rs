//! Real manual source recycle and restore acceptance through production maintenance.
//! 通过生产维护入口验收真实人工来源回收及恢复。
use super::*;

/// Captures committed-content events and proves callbacks do not retain the service mutex.
/// 捕获已提交内容事件，并证明回调不保留服务互斥锁。
struct ContentEvents {
    /// Non-owning service identity used only for the synchronous unlocked-state assertion.
    /// 仅用于同步无锁状态断言的不拥有服务的身份。
    state: std::sync::Weak<std::sync::Mutex<crate::state::ServiceBridgeState>>,
    /// Ordered production events, never synthetic projection acknowledgements.
    /// 有序生产事件，绝非合成投影确认。
    events: std::sync::Mutex<Vec<vulcan_service_api::ServiceEvent>>,
}

impl vulcan_service_api::ServiceEventSink for ContentEvents {
    /// Accepts one real event after service-lock release and records it for exact-session assertions.
    /// 在服务锁释放后接收一个真实事件，并记录以便准确会话断言。
    fn emit_service_event(&self, event: vulcan_service_api::ServiceEvent) {
        // An event callback may synchronously read the service without deadlocking the Core commit.
        // 事件回调可以同步读取服务，不与 Core 提交形成死锁。
        let state = self
            .state
            .upgrade()
            .expect("live content projection service");
        drop(
            state
                .try_lock()
                .expect("event emitted outside service mutex"),
        );
        self.events.lock().expect("content events").push(event);
    }
}

/// Restores the sole extracted memory and checks exact facts, vectors, FTS and client recall.
/// 恢复唯一已提取记忆，检查准确事实、向量、全文索引及客户端召回。
/// Takes isolated production facades, inference capture and the exact owner ID; no record-move fixture is used.
/// 接收隔离的生产门面、推理捕获及准确归属 ID；不使用记录移动夹具。
pub(super) async fn verify(
    client: &NativeVmmClient,
    runtime: &NativeVmmCoreRuntime,
    data_plane: &VmmDataPlaneHandle,
    maintenance: &crate::native_vmm::NativeVmmMaintenance,
    port: &CapturingAnalysisPort,
    project_id: u64,
) {
    // The preceding normal PostAction produced this record; never synthesize its vector or identity.
    // 前序正常 PostAction 生成此记录，不合成其向量或身份。
    let before = runtime
        .storage_snapshot()
        .expect("before restore acceptance");
    assert_eq!(before.dataset.memories.len(), 1);
    let memory = before.dataset.memories[0].clone();
    // The isolated maintenance instance starts disabled just like the production controller.
    // 隔离维护实例像生产控制器一样以禁用状态启动。
    let disabled_query = vulcan_service_api::VmmNativeRemovalQuery {
        vmm_project_id: project_id,
        selection: vulcan_service_api::VmmNativeRemovalTarget::Turns {
            ids: vec![
                memory
                    .source_turn_id
                    .as_ref()
                    .expect("source turn")
                    .to_string(),
            ],
        },
    };
    assert!(
        maintenance
            .preview_manual_removal(disabled_query)
            .await
            .is_err()
    );
    assert_eq!(
        runtime.storage_snapshot().expect("disabled unchanged"),
        before
    );
    maintenance.set_native_backend_enabled(true);
    // Inspect real extracted sources through the production facade, without issuing deletion authority.
    // 通过生产门面检查真实提炼来源，不签发删除权限。
    let llm_before_inspection = port.requests.lock().expect("LLM capture").len();
    let embeddings_before_inspection = port
        .embedding_requests
        .lock()
        .expect("embedding capture")
        .len();
    for selection in [
        vulcan_service_api::VmmNativeRemovalTarget::Sessions {
            ids: vec![
                memory
                    .origin_session_source_id
                    .as_ref()
                    .expect("source session")
                    .to_string(),
            ],
        },
        vulcan_service_api::VmmNativeRemovalTarget::Turns {
            ids: vec![
                memory
                    .source_turn_id
                    .as_ref()
                    .expect("source turn")
                    .to_string(),
            ],
        },
    ] {
        let impact = maintenance
            .inspect_removal(vulcan_service_api::VmmNativeRemovalQuery {
                vmm_project_id: project_id,
                selection,
            })
            .await
            .expect("real source impact");
        assert_eq!(impact.storage_version, before.storage_version);
        assert_eq!(impact.memory_ids, [memory.id.to_string()]);
        assert_eq!(impact.vector_memory_ids, [memory.id.to_string()]);
        assert_eq!(
            impact.turn_ids,
            [memory
                .source_turn_id
                .as_ref()
                .expect("source turn")
                .to_string()]
        );
        assert!(impact.pending_turn_ids.is_empty());
        assert_eq!(impact.hot_window_turn_ids, impact.turn_ids);
        assert_eq!(impact.project_profile_node_ids.len(), 1);
        assert_eq!(impact.user_profile_node_ids.len(), 2);
        for node in &before.dataset.profile_nodes {
            if node.scope_id == memory.scope_id {
                assert!(
                    impact
                        .project_profile_node_ids
                        .contains(&node.id.to_string())
                );
            } else {
                assert_eq!(node.scope_id, subject_scope_id(1).expect("fixed user"));
                assert!(impact.user_profile_node_ids.contains(&node.id.to_string()));
            }
        }
    }
    assert_eq!(
        runtime.storage_snapshot().expect("inspection read only"),
        before
    );
    assert_eq!(
        port.requests.lock().expect("LLM capture").len(),
        llm_before_inspection
    );
    assert_eq!(
        port.embedding_requests
            .lock()
            .expect("embedding capture")
            .len(),
        embeddings_before_inspection
    );
    let binding = data_plane
        .resolve_vector_space(memory.vector_space_id.as_ref().expect("space").as_str())
        .await
        .expect("resolve space")
        .expect("physical binding");
    let vectors_before = data_plane
        .load_vectors(&binding, &[memory.id.to_string()])
        .await
        .expect("vectors before");
    assert_eq!(vectors_before.len(), 1);
    // Confirm the actual source turn through the production preview and move, without rewriting a snapshot.
    // 通过生产预览及移动确认实际来源回合，不改写快照。
    let preview = maintenance
        .preview_manual_removal(vulcan_service_api::VmmNativeRemovalQuery {
            vmm_project_id: project_id,
            selection: vulcan_service_api::VmmNativeRemovalTarget::Turns {
                ids: vec![
                    memory
                        .source_turn_id
                        .as_ref()
                        .expect("source turn")
                        .to_string(),
                ],
            },
        })
        .await
        .expect("production manual preview");
    assert_eq!(
        runtime.storage_snapshot().expect("preview read only"),
        before
    );
    assert!(
        maintenance
            .execute_manual_removal("wrong-preview", "not-issued")
            .await
            .is_err()
    );
    // A rejected token must not consume the valid confirmation.
    // 被拒绝令牌不能消费有效确认。
    let moved = maintenance
        .execute_manual_removal("manual-physical", &preview.preview_id)
        .await
        .expect("production manual recycle");
    let batch_id = moved.batch_id.as_str();
    assert_eq!(moved.impact, preview.impact);
    assert_eq!(moved.storage_version, before.storage_version + 1);
    assert_eq!(
        maintenance
            .execute_manual_removal("manual-physical", &preview.preview_id)
            .await
            .expect("same preview replay"),
        moved
    );
    assert!(
        maintenance
            .execute_manual_removal("new-operation", &preview.preview_id)
            .await
            .is_err()
    );
    assert!(
        data_plane
            .search_lexical_page("PostgreSQL", 10, 0)
            .await
            .expect("recycled FTS")
            .is_empty()
    );
    let recycled = runtime.storage_snapshot().expect("recycled snapshot");
    assert!(
        recycled
            .dataset
            .turns
            .iter()
            .all(|turn| Some(&turn.id) != memory.source_turn_id.as_ref())
    );
    assert_eq!(recycled.memory_sessions, before.memory_sessions);
    assert_eq!(recycled.dataset.profile_nodes, before.dataset.profile_nodes);
    assert_eq!(recycled.retention_trash_batches.len(), 1);
    assert_eq!(
        recycled.retention_trash_batches[0].dataset.memories,
        before.dataset.memories
    );
    assert_eq!(
        port.requests.lock().expect("LLM capture").len(),
        llm_before_inspection
    );
    assert_eq!(
        port.embedding_requests
            .lock()
            .expect("embedding capture")
            .len(),
        embeddings_before_inspection
    );
    let llm_count = port.requests.lock().expect("LLM capture").len();
    let embedding_count = port
        .embedding_requests
        .lock()
        .expect("embedding capture")
        .len();
    let result = maintenance
        .restore_recycle_batch("restore-physical", batch_id, recycled.storage_version)
        .await
        .expect("production restore");
    // Apart from the two committed versions, restore must return the entire original snapshot.
    // 除两次已提交版本递增外，恢复必须返回完整原始快照。
    let mut expected = before;
    expected.storage_version += 2;
    assert_eq!(
        runtime.storage_snapshot().expect("restored snapshot"),
        expected
    );
    let durable: vulcan_memorymesh_core::StorageSnapshot = serde_json::from_str(
        &data_plane
            .load_core_snapshot_json()
            .await
            .expect("durable read")
            .expect("snapshot bytes"),
    )
    .expect("typed durable snapshot");
    assert_eq!(durable, expected.without_embeddings());
    assert_eq!(port.requests.lock().expect("LLM capture").len(), llm_count);
    assert_eq!(
        port.embedding_requests
            .lock()
            .expect("embedding capture")
            .len(),
        embedding_count
    );
    let vectors_after = data_plane
        .load_vectors(&binding, &[memory.id.to_string()])
        .await
        .expect("restored vectors");
    assert_eq!(vectors_after.len(), 1);
    assert_eq!(vectors_after[0].memory_id, vectors_before[0].memory_id);
    assert_eq!(vectors_after[0].vector, vectors_before[0].vector);
    assert!(
        data_plane
            .search_lexical_page("PostgreSQL", 10, 0)
            .await
            .expect("restored FTS")
            .iter()
            .any(|hit| hit.memory_id == memory.id.as_str())
    );
    assert_eq!(
        maintenance
            .restore_recycle_batch("restore-physical", batch_id, recycled.storage_version)
            .await
            .expect("restore replay"),
        result
    );
    assert_eq!(
        runtime.storage_snapshot().expect("replay snapshot"),
        expected
    );
    assert!(
        maintenance
            .restore_recycle_batch("restore-new-operation", batch_id, expected.storage_version)
            .await
            .is_err()
    );
    // Final recall uses the same client as AI tools, after checking restore itself incurred no AI work.
    // 在检查恢复本身不产生 AI 工作后，最终召回使用与 AI 工具相同的客户端。
    let search = client
        .search_memory_events(SearchMemoryEventsRequest {
            host_project_id: "nonempty-host".to_owned(),
            project_id: result.project_id,
            user_id: 1,
            runtime_session_id: Some("nonempty-session".to_owned()),
            queries: vec!["PostgreSQL".to_owned()],
            top_k: 4,
            min_score: None,
        })
        .await
        .expect("restored client recall");
    assert_eq!(search.results.len(), 1);
    assert_eq!(search.results[0].hits.len(), 1);
    assert_eq!(
        search.results[0].hits[0]
            .retrieval
            .as_ref()
            .expect("retrieval")
            .origin,
        "hybrid_rrf"
    );
    verify_whole_session_recycle(client, runtime, maintenance, port, project_id).await;
    verify_publication_barrier(runtime, maintenance, project_id).await;
}

/// Checks a real native mutation cannot cross publication, then filters its now-stale recall result.
/// 检查真实原生变更不能跨越发布，随后过滤其现已失效的召回结果。
/// Takes the isolated Core runtime and maintenance owner; uses real records, not a replacement storage fixture.
/// 接收隔离 Core 运行时及维护归属；使用真实记录，而非替换存储夹具。
async fn verify_publication_barrier(
    runtime: &NativeVmmCoreRuntime,
    maintenance: &crate::native_vmm::NativeVmmMaintenance,
    project_id: u64,
) {
    use vulcan_agent_runtime::agent_runtime::conversation_hooks::{
        ConversationPrecheckOutcome, HOOK_METADATA_VMM_RECALL_BATCH_EFFECT,
        VmmRecallBatchHookEffect, vmm_recall_batch_effect_from_report,
    };
    // A second host session can recall this project memory without owning its source session.
    // 第二个宿主会话可以召回此项目记忆，而无需拥有其来源会话。
    let host_session_id = uuid::Uuid::nil();
    // Capture the real generated source and vector-bearing memory before recycling it.
    // 回收之前捕获真实生成来源及带向量记忆。
    let before = runtime.storage_snapshot().expect("publication baseline");
    // The test selects the only extracted row produced by the normal nonempty PostAction fixture.
    // 测试选择正常非空 PostAction 夹具生成的唯一提取记录。
    let memory = &before.dataset.memories[0];
    // Seed a real domain cache candidate with an earlier recall of that exact native identity.
    // 用该准确原生身份的先前召回构造真实领域缓存候选。
    let batch = vulcan_vmm::VmmRecallBatch {
        ttl_turns: 3,
        injection_tag: "vmm".to_owned(),
        injection_description: "memory".to_owned(),
        queries: Vec::new(),
        items: vec![vulcan_vmm::ContextItem {
            memory_id: parse_core_numeric_id(&memory.id, "vmm-memory-").expect("memory identity"),
            text: memory.summary.clone(),
            ..Default::default()
        }],
    };
    // The same old batch represents an inference result that has not reached host publication yet.
    // 同一旧批次表示尚未到达宿主发布阶段的推理结果。
    let mut report = vulcan_hooks::HookDispatchReport::empty(vulcan_hooks::HookPayload {
        value: serde_json::json!({}),
    });
    report.metadata.insert(
        HOOK_METADATA_VMM_RECALL_BATCH_EFFECT.to_owned(),
        vec![serde_json::json!(VmmRecallBatchHookEffect {
            read_source: vulcan_vmm::VmmMemoryReadSource::new(
                vulcan_vmm::VmmMemorySource::native(project_id).expect("owner"),
                vec![project_id],
            )
            .expect("read source"),
            batch: batch.clone()
        })],
    );
    // Correlation uses a real host UUID even though the original memory came from another session.
    // 即使原记忆来自另一会话，关联仍使用真实宿主 UUID。
    let mut outcome = ConversationPrecheckOutcome {
        session_id: host_session_id,
        turn_id: "delayed-native".to_owned(),
        report: Ok(report),
    };
    // No mutable host workspace is shared with the native storage transaction.
    // 不与原生存储事务共享可变宿主工作区。
    let mut candidate = vulcan_vmm::VmmSessionMemoryState::new(host_session_id.to_string(), "1")
        .expect("candidate");
    // Exercise real frozen prefixes, not only a lease cache with anchor rendering disabled.
    // 验证真实冻结前缀，而不只是关闭锚点渲染的租约缓存。
    candidate
        .synchronize_stable_anchor_rendering(true, "1")
        .expect("enable candidate anchors");
    candidate
        .select_read_source(
            &host_session_id.to_string(),
            Some(
                &vulcan_vmm::VmmMemoryReadSource::new(
                    vulcan_vmm::VmmMemorySource::native(project_id).expect("owner"),
                    vec![project_id],
                )
                .expect("source"),
            ),
            "1",
        )
        .expect("select cache source");
    candidate
        .apply_recall_batch(&host_session_id.to_string(), "user-1", &batch, "2")
        .expect("old cache");
    // The host cache now also lives in a real private V1 session document and controller registry.
    // 宿主缓存现在也存在于真实私有 V1 会话文档及控制器注册表中。
    let controller_root = crate::tests::test_support::TestHostRuntimeRoot::new(host_session_id);
    // This controller is the only writer of its private cache document.
    // 此控制器是其私有缓存文档的唯一写入方。
    let registry = crate::vmm_session_controller::SessionVmmControllerRegistry::default();
    // Opening uses the same locator-based load as the production provider boundary.
    // 打开使用与生产 Provider 边界相同的基于定位器加载。
    let controller = registry
        .get_or_open(controller_root.v1_storage_locator(), host_session_id, "1")
        .expect("host cache controller");
    controller
        .execute_command("provider_boundary:delayed-native", "2", |state| {
            // The durable cache must exercise the same explicit stable-anchor mode as the candidate.
            // 持久缓存必须验证与候选相同的显式稳定锚点模式。
            state.synchronize_stable_anchor_rendering(true, "2")?;
            // The durable fixture must carry the same proven source as the detached cache candidate.
            // 持久夹具必须携带与脱离缓存候选相同的已证明来源。
            state.select_read_source(
                &host_session_id.to_string(),
                candidate.read_source.as_ref(),
                "2",
            )?;
            state.apply_recall_batch(&host_session_id.to_string(), "user-1", &batch, "2")?;
            Ok(())
        })
        .expect("original durable recall");
    // The real registry is installed in a host service; native commits must update it without a new request.
    // 将真实注册表安装到宿主服务；原生提交必须在没有新请求时更新它。
    let mut service =
        crate::tests::test_support::test_service_state_with_parent_session(host_session_id);
    service.vmm_controller_registry = registry.clone();
    // Keep a genuine retained runtime row for the selected session's canonical UI projection.
    // 为选中会话的规范界面投影保留真实运行态行。
    let retained = vulcan_workbench_model::SessionRuntimeStateEntry {
        vmm_profile_sync: vulcan_workbench_model::VmmProfileSyncState {
            read_source: candidate.read_source.clone(),
            nodes_by_id: before
                .dataset
                .profile_nodes
                .iter()
                .map(|node| {
                    crate::native_vmm::client::profile_node_entry(node)
                        .map(|entry| (entry.profile_node_id, entry))
                })
                .collect::<Result<_, _>>()
                .expect("real retained profile nodes"),
            base_profile_content: Some("Retained personal and project profiles.".to_owned()),
            ..Default::default()
        },
        ..Default::default()
    };
    // Manual source recycle must preserve the complete profile cache because both durable profile roles remain active.
    // 人工来源回收必须保留完整画像缓存，因为两种持久画像角色均保持活动。
    let profiles_before_recycle = retained.vmm_profile_sync.clone();
    // Start with the canonical nonempty recall card generated from the real controller.
    // 从真实控制器生成的规范非空召回卡片开始。
    let mut initial_panel =
        crate::workbench_session_panels::context_injection_snapshot_from_runtime_state(
            Some(&retained),
            Some(&controller.snapshot().expect("initial resident")),
        )
        .expect("initial panel");
    service
        .workspace_snapshot
        .as_mut()
        .expect("workspace")
        .session_runtime_state
        .insert(host_session_id, retained);
    service.workbench.selected_conversation_id = Some(host_session_id.to_string());
    service.workbench.context_injection = Some(initial_panel.clone());
    // Nonempty unrelated cards make preservation assertions stronger than comparing empty groups.
    // 非空无关卡片使保留断言强于比较空分组。
    let mut knowledge_card = initial_panel.dynamic_cards[0].clone();
    knowledge_card.tag = "knowledge-context".to_owned();
    initial_panel.dynamic_cards.push(knowledge_card.clone());
    initial_panel.static_cards.push(knowledge_card.clone());
    // A second resident controller deliberately has no retained runtime row, only its existing UI panel.
    // 第二个驻留控制器刻意没有保留运行态行，只有其既有界面面板。
    let evicted_id = uuid::Uuid::new_v4();
    // The evicted runtime fixture still owns an independent real session document.
    // 驱逐运行态夹具仍拥有独立真实会话文档。
    let evicted_root = crate::tests::test_support::TestHostRuntimeRoot::new(evicted_id);
    // Register its sole durable cache writer without adding a workspace runtime row.
    // 注册其唯一持久缓存写入方，不添加工作区运行态行。
    let evicted = registry
        .get_or_open(evicted_root.v1_storage_locator(), evicted_id, "1")
        .expect("evicted controller");
    evicted
        .execute_command("evicted-original-recall", "2", |state| {
            state.synchronize_stable_anchor_rendering(true, "2")?;
            state.select_read_source(
                &evicted_id.to_string(),
                candidate.read_source.as_ref(),
                "2",
            )?;
            state.apply_recall_batch(&evicted_id.to_string(), "user-1", &batch, "2")?;
            Ok(())
        })
        .expect("evicted recall");
    // An external controller can have the same numeric memory ID but belongs to a different backend.
    // 外置控制器可以具有相同数字记忆 ID，但属于另一后端。
    let external_id = uuid::Uuid::new_v4();
    // Give each isolated controller its own declared V1 storage locator.
    // 为每个隔离控制器提供其自身声明的 V1 存储定位器。
    let external_root = crate::tests::test_support::TestHostRuntimeRoot::new(external_id);
    // External cache ownership uses the same registry but must not enter native reconciliation.
    // 外置缓存归属使用同一注册表，但不能进入原生对账。
    let external = registry
        .get_or_open(external_root.v1_storage_locator(), external_id, "1")
        .expect("external controller");
    // Use an explicit local test endpoint to construct identity; no remote request is made.
    // 使用明确本地测试端点构造身份；不发起远端请求。
    let external_source = vulcan_vmm::VmmMemoryReadSource::new(
        vulcan_vmm::VmmMemorySource::external(
            &vulcan_vmm::VmmTransportConfig::from_effective(
                &vulcan_vmm::VmmEffectiveConfig::resolve(
                    &vulcan_vmm::VmmConfigLayer {
                        grpc_target: Some("http://127.0.0.1:9000".to_owned()),
                        ..Default::default()
                    },
                    None,
                    None,
                ),
            ),
            1,
            project_id,
        )
        .expect("external source"),
        Vec::new(),
    )
    .expect("external read source");
    external
        .execute_command("external-original-recall", "2", |state| {
            state.select_read_source(&external_id.to_string(), Some(&external_source), "2")?;
            state.apply_recall_batch(&external_id.to_string(), "user-1", &batch, "2")?;
            Ok(())
        })
        .expect("external recall");
    // Native commits must preserve this complete cache, including its existing receipt lineage.
    // 原生提交必须保留此完整缓存，包括既有回执沿革。
    let external_before = external.snapshot().expect("external baseline");
    service.workbench.context_injection_by_conversation.push(
        vulcan_service_api::ContextInjectionChangedDelta {
            conversation_id: evicted_id.to_string(),
            context_injection: Some(initial_panel.clone()),
        },
    );
    // This displayed cold session is not opened merely because its panel exists.
    // 此已展示冷会话不会仅因面板存在而被打开。
    let cold_id = uuid::Uuid::new_v4();
    service.workbench.context_injection_by_conversation.push(
        vulcan_service_api::ContextInjectionChangedDelta {
            conversation_id: cold_id.to_string(),
            context_injection: Some(initial_panel.clone()),
        },
    );
    // A lifecycle-fenced identity remains absent throughout commit projection.
    // 生命周期栅栏中的身份在整个提交投影期间保持缺失。
    let fenced_id = uuid::Uuid::new_v4();
    // Keep the lifecycle guard alive through recycle and restore to prohibit hydration.
    // 在回收及恢复期间保持生命周期守卫存活，禁止水合。
    let _fence = registry
        .begin_lifecycle_transition([fenced_id])
        .expect("fenced cold owner");
    // The observer holds only a weak reference, avoiding a service/native-runtime cycle.
    // 观察器只持有弱引用，避免服务与原生运行时形成循环。
    let state = std::sync::Arc::new(std::sync::Mutex::new(service));
    // Capture actual production emissions through a weak service reference.
    // 通过弱服务引用捕获实际生产发送事件。
    let events = std::sync::Arc::new(ContentEvents {
        state: std::sync::Arc::downgrade(&state),
        events: std::sync::Mutex::new(Vec::new()),
    });
    // Use the ordinary emitter, including its synchronous sink invocation.
    // 使用普通发送器，包括其同步接收器调用。
    let emitter = vulcan_service_api::ServiceEventEmitter::new(vec![events.clone()]);
    runtime
        .install_content_projection(&state, &emitter)
        .expect("install commit observer");
    // Hold the exact production publication gate after model preparation.
    // 在模型准备之后持有准确生产发布门。
    let publication = maintenance
        .prepare_recall_publication(Some("nonempty-host"), &host_session_id.to_string())
        .await
        .expect("publication guard");
    // Profile publication uses the same frozen binding as recall, not the shared request-envelope ID.
    // 画像发布使用与召回相同的冻结绑定，而非共享请求封套 ID。
    let profile_source = publication
        .profile_read_source()
        .expect("frozen profile source");
    assert_eq!(
        profile_source.owner,
        vulcan_vmm::VmmMemorySource::native(project_id).expect("owner")
    );
    assert_eq!(
        profile_source.visible_project_ids.first(),
        Some(&project_id)
    );
    // The spawned attempt shares the actual maintenance lock and reports when it starts polling.
    // 新建尝试共享实际维护锁，并报告何时开始轮询。
    let competing = maintenance.clone();
    // Only the real source identity is carried into the competing preview.
    // 只将真实来源身份携带至竞争预览。
    let turn_id = memory.source_turn_id.as_ref().expect("source").to_string();
    // Synchronize test scheduling without pretending a sleep alone proves contention.
    // 同步测试调度，不把单纯睡眠当作竞争证明。
    let (started, observed) = tokio::sync::oneshot::channel();
    // This task is awaited after dropping the guard; it never outlives the test.
    // 此任务在释放守卫后被等待；绝不超出测试生命周期。
    let mut preview_task = tokio::spawn(async move {
        started.send(()).expect("attempt observed");
        competing
            .preview_manual_removal(vulcan_service_api::VmmNativeRemovalQuery {
                vmm_project_id: project_id,
                selection: vulcan_service_api::VmmNativeRemovalTarget::Turns { ids: vec![turn_id] },
            })
            .await
    });
    observed.await.expect("attempt started");
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), &mut preview_task)
            .await
            .is_err()
    );
    publication
        .reconcile(&mut candidate, &mut outcome, "3")
        .expect("live source admitted");
    assert_eq!(candidate.active_leases.len(), 1);
    // Replaying live sources is a read-only reconciliation and does not append a no-op receipt.
    // 重放活动来源是只读对账，不追加无变化回执。
    let live_state = controller.snapshot().expect("live host state");
    assert_eq!(live_state.active_leases.len(), 1);
    assert_eq!(live_state.cache_epoch.anchors.len(), 1);
    assert!(
        live_state.cache_epoch.anchors[0]
            .prefix
            .contains(&memory.summary)
    );
    // A cache-only boundary has no Hook result and must remain a durable no-op for the same live source.
    // 纯缓存边界没有 Hook 结果；同一活动来源必须保持持久无操作。
    assert_eq!(
        publication
            .reconcile_controller_cache(&controller, None, "3")
            .expect("unchanged cache-only boundary"),
        live_state
    );
    assert_eq!(
        publication
            .reconcile_replayed_boundary(&controller, &mut outcome, "3")
            .expect("unchanged replay"),
        live_state
    );
    drop(publication);
    // Completion proves the normal preview was waiting on the held production publication boundary.
    // 完成证明正常预览此前正在等待已持有的生产发布边界。
    let preview = tokio::time::timeout(std::time::Duration::from_secs(5), preview_task)
        .await
        .expect("preview unblocked")
        .expect("task")
        .expect("preview");
    // Execute the actual move, then recheck the earlier still-unpublished model result.
    // 执行实际移动，然后重新检查此前仍未发布的模型结果。
    let moved = maintenance
        .execute_manual_removal("publication-source-recycle", &preview.preview_id)
        .await
        .expect("real recycle");
    // No provider preparation or publication helper has run since the actual content commit.
    // 自实际内容提交以来尚未运行任何 Provider 准备或发布辅助函数。
    let committed_cache = controller.snapshot().expect("immediate committed cache");
    assert_eq!(
        external.snapshot().expect("external unchanged"),
        external_before
    );
    assert!(committed_cache.active_leases.is_empty());
    assert_eq!(
        committed_cache.cache_epoch.anchors,
        live_state.cache_epoch.anchors
    );
    assert!(
        evicted
            .snapshot()
            .expect("evicted committed cache")
            .active_leases
            .is_empty()
    );
    assert_eq!(
        vulcan_workspace_store::workspace_store::load_session_vmm_state(
            &controller_root.v1_storage_locator()
        )
        .expect("immediate disk cache")
        .expect("persisted cache"),
        committed_cache,
    );
    assert!(!registry.contains(cold_id).expect("cold remains unopened"));
    assert!(
        !registry
            .contains(fenced_id)
            .expect("fenced remains unopened")
    );
    {
        // Canonical active and evicted panels both reflect revoked leases at return from maintenance.
        // 规范活动及驱逐面板在维护返回时均反映已撤销租约。
        let guard = state.lock().expect("committed service");
        assert_eq!(
            guard
                .workspace_snapshot
                .as_ref()
                .expect("workspace")
                .session_runtime_state[&host_session_id]
                .vmm_profile_sync,
            profiles_before_recycle
        );
        // The selected mirror is read directly, not inferred from whichever list row exists.
        // 直接读取选中镜像，不从任意存在的列表行推断。
        let selected = guard
            .workbench
            .context_injection
            .as_ref()
            .expect("selected panel");
        assert!(
            !selected
                .dynamic_cards
                .iter()
                .any(|card| card.tag == "VMM_MEMORY")
        );
        // Resolve the evicted panel by its exact conversation ID.
        // 按准确会话 ID 解析驱逐面板。
        let evicted_panel = guard
            .workbench
            .context_injection_by_conversation
            .iter()
            .find(|entry| entry.conversation_id == evicted_id.to_string())
            .expect("evicted panel")
            .context_injection
            .as_ref()
            .expect("existing evicted panel");
        assert!(
            !evicted_panel
                .dynamic_cards
                .iter()
                .any(|card| card.tag == "VMM_MEMORY")
        );
        assert_eq!(evicted_panel.static_profile, initial_panel.static_profile);
        assert_eq!(evicted_panel.static_cards, initial_panel.static_cards);
        assert_eq!(
            evicted_panel
                .dynamic_cards
                .iter()
                .find(|card| card.tag == "knowledge-context"),
            Some(&knowledge_card)
        );
        assert_eq!(
            guard
                .workbench
                .context_injection_by_conversation
                .iter()
                .find(|entry| entry.conversation_id == cold_id.to_string())
                .expect("cold panel")
                .context_injection
                .as_ref(),
            Some(&initial_panel)
        );
    }
    // Only the two changed resident owners produce real events; no cold-session fanout occurs.
    // 只有两个变化的驻留归属产生真实事件；不向冷会话广播。
    let changed_ids = events
        .events
        .lock()
        .expect("captured deltas")
        .iter()
        .map(|event| {
            let vulcan_service_api::ServiceEvent::WorkbenchDelta(envelope) = event else {
                panic!("context delta event")
            };
            let vulcan_service_api::WorkbenchDelta::ContextInjectionChanged(delta) =
                &envelope.delta
            else {
                panic!("context delta")
            };
            delta.conversation_id.clone()
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        changed_ids,
        std::collections::BTreeSet::from([host_session_id.to_string(), evicted_id.to_string()])
    );
    // Reacquire against the current binding and source store, never a saved availability bit.
    // 根据当前绑定及来源存储重新获取，而非使用保存的可用标记。
    let publication = maintenance
        .prepare_recall_publication(Some("nonempty-host"), &host_session_id.to_string())
        .await
        .expect("new publication");
    publication
        .reconcile(&mut candidate, &mut outcome, "4")
        .expect("stale source filtered");
    assert!(candidate.active_leases.is_empty());
    assert!(
        vmm_recall_batch_effect_from_report(outcome.report.as_ref().expect("report"))
            .expect("typed recall")
            .is_none()
    );
    // The original provider receipt cannot run its mutator again, so invalidation needs its own command.
    // 原始 Provider 回执不能再次运行变更器，因此失效处理需要独立命令。
    let replay = controller
        .execute_command("provider_boundary:delayed-native", "4", |_| {
            panic!("duplicate provider boundary must not execute")
        })
        .expect("original receipt replay");
    assert!(!replay.applied);
    assert!(replay.state.active_leases.is_empty());
    // Use the original delayed batch again to prove both persisted and in-flight copies are removed.
    // 再次使用原始迟到批次，证明持久与在途副本均被移除。
    outcome.report.as_mut().expect("report").metadata.insert(
        HOOK_METADATA_VMM_RECALL_BATCH_EFFECT.to_owned(),
        vec![serde_json::json!(VmmRecallBatchHookEffect {
            read_source: vulcan_vmm::VmmMemoryReadSource::new(
                vulcan_vmm::VmmMemorySource::native(project_id).expect("owner"),
                vec![project_id],
            )
            .expect("read source"),
            batch,
        })],
    );
    // Reconcile the real cache without fabricating or consuming any PreCheck outcome.
    // 不伪造或消费任何 PreCheck 结果，对账真实缓存。
    let durable = publication
        .reconcile_controller_cache(&controller, None, "4")
        .expect("durable cache-only reconciliation");
    assert_eq!(durable, committed_cache);
    assert!(
        vmm_recall_batch_effect_from_report(outcome.report.as_ref().expect("report"))
            .expect("untouched delayed result")
            .is_some()
    );
    // The existing duplicate boundary still filters its real delayed output without appending another receipt.
    // 既有重复边界仍过滤其真实迟到输出，但不追加另一份回执。
    assert_eq!(
        publication
            .reconcile_replayed_boundary(&controller, &mut outcome, "4")
            .expect("durable replay reconciliation"),
        durable
    );
    assert!(
        vmm_recall_batch_effect_from_report(outcome.report.as_ref().expect("report"))
            .expect("filtered delayed result")
            .is_none()
    );
    assert!(durable.active_leases.is_empty());
    // Revocation preserves immutable historical prefixes; only a genuine epoch transition may rebuild them.
    // 撤销保留不可变历史前缀；只有真实纪元转换才可以重建它们。
    assert_eq!(durable.cache_epoch.anchors, live_state.cache_epoch.anchors);
    assert!(durable.has_committed_controller_command("provider_boundary:delayed-native"));
    assert_eq!(durable.committed_command_hashes.len(), 2);
    assert_eq!(controller.snapshot().expect("published cache"), durable);
    assert_eq!(
        vulcan_workspace_store::workspace_store::load_session_vmm_state(
            &controller_root.v1_storage_locator()
        )
        .expect("read durable host cache")
        .expect("cache document"),
        durable
    );
    assert_eq!(
        publication
            .reconcile_replayed_boundary(&controller, &mut outcome, "5")
            .expect("repeat reconciled boundary"),
        durable
    );
    drop(publication);
    maintenance
        .restore_recycle_batch(
            "publication-source-restore",
            &moved.batch_id,
            moved.storage_version,
        )
        .await
        .expect("restore after publication test");
    // Restoring source data never re-admits old leases or appends no-op cache receipts/events.
    // 恢复来源数据绝不重新准入旧租约，也不追加无变化缓存回执或事件。
    assert_eq!(
        controller.snapshot().expect("no recall resurrection"),
        durable
    );
    assert_eq!(
        events
            .events
            .lock()
            .expect("event count after restore")
            .len(),
        2
    );
    // The two source moves are reversible and leave every original record unchanged.
    // 两次来源移动可逆，并保持每条原记录不变。
    let mut expected = before;
    expected.storage_version += 2;
    assert_eq!(
        runtime.storage_snapshot().expect("publication roundtrip"),
        expected
    );
}

/// Verifies the whole-session production move blocks intake and restores original metadata and content.
/// 验证完整会话生产移动阻止入库，并恢复原始元数据及内容。
/// Takes isolated live facades and exact project identity; no lifecycle or record mutation is faked.
/// 接收隔离实时门面及准确项目身份；不伪造生命周期或记录变更。
async fn verify_whole_session_recycle(
    client: &NativeVmmClient,
    runtime: &NativeVmmCoreRuntime,
    maintenance: &crate::native_vmm::NativeVmmMaintenance,
    port: &CapturingAnalysisPort,
    project_id: u64,
) {
    // Preserve every original ID, profile, vector and watermark for roundtrip equality.
    // 保留每个原始 ID、画像、向量及水位，用于往返一致性比较。
    let before = runtime.storage_snapshot().expect("whole session baseline");
    assert_eq!(before.memory_sessions.len(), 1);
    // The fixture's host binding was admitted by normal PostAction, not a display-name lookup.
    // 夹具宿主绑定由正常 PostAction 准入，而非显示名称查询。
    let session_id = before.memory_sessions[0].session_source_id.to_string();
    // Preview and execution must not perform paid or deterministic AI work.
    // 预览和执行均不得进行付费或确定性 AI 工作。
    let llm_count = port.requests.lock().expect("LLM count").len();
    // Embedding vectors must move unchanged rather than being rebuilt.
    // 嵌入向量必须原样移动，而非重建。
    let embedding_count = port
        .embedding_requests
        .lock()
        .expect("embedding count")
        .len();
    // The server freezes the complete session selection and accurate impact.
    // 服务端冻结完整会话选择及准确影响。
    let preview = maintenance
        .preview_manual_removal(vulcan_service_api::VmmNativeRemovalQuery {
            vmm_project_id: project_id,
            selection: vulcan_service_api::VmmNativeRemovalTarget::Sessions {
                ids: vec![session_id.clone()],
            },
        })
        .await
        .expect("whole session preview");
    assert_eq!(
        runtime.storage_snapshot().expect("preview unchanged"),
        before
    );
    // Explicit disablement rejects execution before consuming its selection or touching records.
    // 明确禁用在消费选择或触及记录之前拒绝执行。
    maintenance.set_native_backend_enabled(false);
    assert!(
        maintenance
            .execute_manual_removal("whole-session-physical", &preview.preview_id)
            .await
            .is_err()
    );
    assert_eq!(
        runtime.storage_snapshot().expect("disabled unchanged"),
        before
    );
    maintenance.set_native_backend_enabled(true);
    // Re-enable the same isolated mode and execute the still-valid bounded confirmation.
    // 重新启用同一隔离模式，执行仍有效的有界确认。
    let result = maintenance
        .execute_manual_removal("whole-session-physical", &preview.preview_id)
        .await
        .expect("whole session recycle");
    // The persisted tombstone is authoritative before any runtime request can proceed.
    // 任何运行时请求继续之前，持久墓碑已成为权威。
    let recycled = runtime.storage_snapshot().expect("whole session recycled");
    assert!(recycled.dataset.turns.is_empty());
    assert!(recycled.dataset.memories.is_empty());
    assert_eq!(recycled.dataset.profile_nodes, before.dataset.profile_nodes);
    assert_eq!(
        recycled.memory_sessions[0].status,
        vulcan_memorymesh_core::StorageSessionStatus::Recycled
    );
    assert_eq!(
        recycled.memory_sessions[0].last_extraction_observed_at_ns,
        0
    );
    assert_eq!(
        recycled.memory_sessions[0].last_extraction_completed_at_ns,
        None
    );
    assert_eq!(
        recycled.retention_trash_batches[0].dataset.memories,
        before.dataset.memories
    );
    // Normal PreCheck must report the actual paused state with no recall.
    // 正常 PreCheck 必须报告实际暂停状态，且不召回。
    let precheck = client
        .pre_check(PreCheckRequest {
            host_project_id: "nonempty-host".to_owned(),
            project_id,
            user_id: 1,
            session_id: session_id.clone(),
            operation_id: "manual-recycled-precheck".to_owned(),
            user_content: "查询原有项目记忆".to_owned(),
            ..Default::default()
        })
        .await
        .expect("recycled precheck");
    assert_eq!(
        precheck.session_memory_status,
        SessionMemoryStatus::Recycled
    );
    assert!(!precheck.should_inject);
    assert!(precheck.context_items.is_empty());
    // Normal PostAction must not recreate the removed original turn or silently reactivate its session.
    // 正常 PostAction 不得重建移走的原回合或静默重新激活其会话。
    let postaction = client
        .post_action(PostActionRequest {
            host_project_id: "nonempty-host".to_owned(),
            project_id,
            user_id: 1,
            session_id,
            operation_id: "manual-recycled-postaction".to_owned(),
            user_content: "不应入库".to_owned(),
            assistant_content: "暂停记忆".to_owned(),
            ..Default::default()
        })
        .await
        .expect("recycled postaction");
    assert_eq!(
        postaction.session_memory_status,
        SessionMemoryStatus::Recycled
    );
    assert!(!postaction.accepted);
    assert_eq!(
        runtime.storage_snapshot().expect("paused intake unchanged"),
        recycled
    );
    maintenance
        .restore_recycle_batch(
            "whole-session-physical-restore",
            &result.batch_id,
            recycled.storage_version,
        )
        .await
        .expect("whole session restore");
    // Both production transactions increment the version; all original content and windows return exactly.
    // 两次生产事务递增版本；全部原始内容及水位准确返回。
    let mut expected = before;
    expected.storage_version += 2;
    assert_eq!(
        runtime.storage_snapshot().expect("whole session roundtrip"),
        expected
    );
    assert_eq!(port.requests.lock().expect("LLM count").len(), llm_count);
    assert_eq!(
        port.embedding_requests
            .lock()
            .expect("embedding count")
            .len(),
        embedding_count
    );
}
