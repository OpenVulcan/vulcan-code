//! Real archive classification and active deletion with nonempty resident recall and private V1 persistence.
//! 带非空驻留召回及私有 V1 持久化的真实归档分类与主动删除。

use super::*;

/// Verifies archive isolation followed by active deletion across two real resident consumers.
/// 验证归档隔离及随后跨两个真实驻留消费方的主动删除。
/// Takes the actual client, Core runtime, data plane, maintenance owner and deterministic inference observer.
/// 接收实际客户端、Core 运行时、数据面、维护归属及确定性推理观察器。
/// Returns after checking real service actions, durable cache/content, vectors, events and another resident session.
/// 检查真实服务动作、持久缓存及内容、向量、事件和另一驻留会话后返回。
pub(super) async fn verify(
    client: &NativeVmmClient,
    runtime: &NativeVmmCoreRuntime,
    data_plane: &VmmDataPlaneHandle,
    maintenance: &crate::native_vmm::NativeVmmMaintenance,
    port: &CapturingAnalysisPort,
) {
    maintenance.set_native_backend_enabled(true);
    // One explicit private host binding owns both sessions; no existing user's project is selected.
    // 一个明确私有宿主绑定拥有两个会话，不选择任何现有用户项目。
    let host_id = "archive-resident-owner";
    // Obtain the actual owner identity from the normal maintenance creation path.
    // 从常规维护创建路径取得实际所有者身份。
    let project = maintenance
        .create_and_bind_project(host_id, "驻留归档验收")
        .await
        .expect("actual archive project");
    // The same UUID owns Core metadata, the V1 controller and the visible workbench row.
    // 同一 UUID 拥有 Core 元数据、V1 控制器及可见工作台行。
    let session_id = uuid::Uuid::new_v4();
    // A second admitted session proves classification does not affect another consumer of the same memory.
    // 第二个已准入会话证明分类不会影响同一记忆的另一消费方。
    let other_id = uuid::Uuid::new_v4();
    // Normal active writing creates real memory, vector storage and exact source-session metadata.
    // 常规主动写入创建真实记忆、向量存储及准确来源会话元数据。
    let written = client
        .write_memories(WriteMemoriesRequest {
            host_project_id: host_id.to_owned(),
            project_id: project.id,
            user_id: 1,
            session_id: session_id.to_string(),
            items: vec![vulcan_vmm::WriteMemoryItem {
                scope_level: 2,
                category: 0,
                priority: 2,
                memory_level: 3,
                abstract_text: "归档项目仍使用 PostgreSQL 保存业务数据。".to_owned(),
                details: "归档只改变分类，已准入记忆和固定提示词锚点必须继续可用。".to_owned(),
            }],
        })
        .await
        .expect("real active memory write");
    assert_eq!(written.items.len(), 1);
    assert!(!written.items[0].deduped);
    client
        .pre_check(PreCheckRequest {
            host_project_id: host_id.to_owned(),
            project_id: project.id,
            user_id: 1,
            session_id: other_id.to_string(),
            operation_id: "archive-other-admission".to_owned(),
            user_content: "你好。".to_owned(),
            ..Default::default()
        })
        .await
        .expect("other actual session admission");
    // Select the memory by the exact reversible native envelope returned by the real write.
    // 按真实写入返回的准确可逆原生封套选择记忆。
    let memory_key = format!("vmm-memory-{}", written.items[0].memory_id);
    // Preserve the complete nonempty dataset, including unrelated projects and both profile roles.
    // 保留完整非空数据集，包括无关项目与两种画像角色。
    let baseline = runtime
        .storage_snapshot()
        .expect("archive content baseline");
    // Use the persisted record's own text and vector-space identity, not a fabricated recall body.
    // 使用持久记录自身的正文及向量空间身份，不伪造召回正文。
    let memory = baseline
        .dataset
        .memories
        .iter()
        .find(|memory| memory.id.as_str() == memory_key)
        .expect("actual written memory");
    // Read physical vectors through the actual storage binding before the classification changes.
    // 在分类变化前通过实际存储绑定读取物理向量。
    let vector_binding = data_plane
        .resolve_vector_space(
            memory
                .vector_space_id
                .as_ref()
                .expect("written vector space")
                .as_str(),
        )
        .await
        .expect("vector binding lookup")
        .expect("physical vector binding");
    // The one exact memory key bounds the physical comparison without copying other vectors.
    // 唯一准确记忆键限制物理比较范围，不复制其他向量。
    let memory_keys = vec![memory_key.clone()];
    // Keep actual vector values to prove no re-embedding or destructive classification occurred.
    // 保留实际向量值，以证明未发生重新嵌入或破坏性分类。
    let vectors = data_plane
        .load_vectors(&vector_binding, &memory_keys)
        .await
        .expect("initial vectors");
    assert_eq!(vectors.len(), 1);
    // A physical search permits absent rows; strict restart hydration deliberately rejects them.
    // 物理搜索允许记录不存在；严格重启回填则有意拒绝缺失记录。
    let physical_filter = vulcan_workspace_store::vmm_data_plane::VmmVectorSearchFilter {
        subject_id: memory
            .source_subject_id
            .as_ref()
            .expect("written memory subject")
            .to_string(),
        visible_scope_ids: vec![memory.scope_id.to_string()],
        candidate_scope_ids: Some(vec![memory.scope_id.to_string()]),
        origin_session_source_id: Some(session_id.to_string()),
    };
    assert_eq!(
        data_plane
            .search_vectors(&vector_binding, vectors[0].vector.clone(), &physical_filter)
            .await
            .expect("physical search before deletion"),
        vectors
    );
    // Both consumers use the real binding-derived read scope.
    // 两个消费方使用真实绑定派生的读取范围。
    let source = client
        .memory_read_source(host_id, 1, project.id)
        .await
        .expect("actual source");
    // This cache batch contains the actual active record returned by the normal write path.
    // 此缓存批次包含常规写入路径返回的实际活动记录。
    let batch = vulcan_vmm::VmmRecallBatch {
        ttl_turns: 4,
        injection_tag: "vmm".to_owned(),
        injection_description: "memory".to_owned(),
        queries: Vec::new(),
        items: vec![vulcan_vmm::ContextItem {
            memory_id: written.items[0].memory_id,
            text: memory.summary.clone(),
            ..Default::default()
        }],
    };
    // Each resident controller has its own private, real V1 document.
    // 每个驻留控制器都有自身私有的真实 V1 文档。
    let root = crate::tests::test_support::TestHostRuntimeRoot::new(session_id);
    // The unrelated consumer's complete persisted VMM state must remain unchanged.
    // 无关消费方的完整持久 VMM 状态必须保持不变。
    let other_root = crate::tests::test_support::TestHostRuntimeRoot::new(other_id);
    // Start from the existing service fixture, then install only explicitly declared runtime owners.
    // 从既有服务夹具开始，随后仅安装明确声明的运行时归属方。
    let mut service =
        crate::tests::test_support::test_service_state_with_parent_session(session_id);
    // Native maintenance routing reads actual retained Provider configuration.
    // 原生维护路由读取实际保留 Provider 配置。
    let mut agent = crate::tests::test_support::test_agent_runtime_bridge();
    agent.provider_configuration.vmm.enabled = Some(true);
    agent.provider_configuration.vmm.backend_mode = Some(vulcan_vmm::VmmBackendMode::Native);
    service.agent_runtime = Some(agent);
    service.native_vmm_maintenance = Some(maintenance.clone());
    // Open the sole cache writer for the selected UUID through the production registry.
    // 通过生产注册表打开选中 UUID 的唯一缓存写入方。
    let controller = service
        .vmm_controller_registry
        .get_or_open(root.v1_storage_locator(), session_id, "1")
        .expect("selected resident controller");
    // Open the second known identity without relying on current selection or a history scan.
    // 打开第二个已知身份，不依赖当前选择或历史扫描。
    let other = service
        .vmm_controller_registry
        .get_or_open(other_root.v1_storage_locator(), other_id, "1")
        .expect("other resident controller");
    for (id, resident) in [(session_id, &controller), (other_id, &other)] {
        resident
            .execute_command(&format!("archive-recall:{id}"), "2", |cached| {
                cached.synchronize_stable_anchor_rendering(true, "2")?;
                cached.select_read_source(&id.to_string(), Some(&source), "2")?;
                cached.apply_recall_batch(&id.to_string(), "user-anchor", &batch, "2")?;
                Ok(())
            })
            .expect("real durable nonempty cache");
    }
    // Workspace ownership maps are explicit inputs to the canonical projection.
    // 工作区归属映射是规范投影的明确输入。
    let workspace = service
        .workspace_snapshot
        .as_mut()
        .expect("actual workspace");
    workspace.sessions = [session_id, other_id]
        .into_iter()
        .map(|id| {
            vulcan_workbench_model::SessionSummary::new(
                id,
                Some(host_id.to_owned()),
                "驻留分类会话",
                "",
                vulcan_workbench_model::SessionState::Completed,
                0,
                0,
                "1",
            )
        })
        .collect();
    for id in [session_id, other_id] {
        workspace
            .session_runtime_state
            .insert(id, Default::default());
    }
    service.workbench.selected_conversation_id = Some(session_id.to_string());
    // Keep the actual state alive for the observer's weak reference and all management requests.
    // 在观察器弱引用及全部管理请求期间保持实际状态存活。
    let state = Arc::new(StdMutex::new(service));
    // Events come solely from the actual commit observer and product service calls.
    // 事件仅来自实际提交观察器及产品服务调用。
    let events = Arc::new(LifecycleEvents::default());
    // The observer and management service share the same capture boundary.
    // 观察器与管理服务共用相同捕获边界。
    let app = ServiceEventEmitter::new(vec![events.clone()]);
    runtime
        .install_content_projection(&state, &app)
        .expect("archive resident observer");
    // Freeze real queue and inference observations after fixture construction has finished.
    // 在夹具构建完成后冻结真实队列及推理观察。
    let jobs = data_plane
        .list_active_pending_analysis_jobs()
        .await
        .expect("archive queue baseline");
    // No model work belongs to either archive transition.
    // 两个归档转换均不应产生模型工作。
    let calls = (
        port.requests
            .lock()
            .expect("initial model observations")
            .len(),
        port.embedding_requests
            .lock()
            .expect("initial embedding observations")
            .len(),
    );
    // Compare the second resident's complete cache and persisted document across both transitions.
    // 跨两个转换比较第二个驻留会话的完整缓存及持久文档。
    let other_before = other.snapshot().expect("other cache baseline");
    // Each iteration must add exactly one lifecycle commit without changing content or the other session.
    // 每次迭代必须只增加一个生命周期提交，不改变内容或另一会话。
    let mut expected = baseline.clone();
    for (index, archived) in [true, false].into_iter().enumerate() {
        // Capture the complete admitted nonempty cache before calling the management service.
        // 在调用管理服务前捕获完整已准入非空缓存。
        let cache_before = controller.snapshot().expect("cache before classification");
        assert_eq!(cache_before.active_leases.len(), 1);
        assert_eq!(cache_before.cache_epoch.anchors.len(), 1);
        assert!(
            cache_before.cache_epoch.anchors[0]
                .prefix
                .contains(&memory.summary)
        );
        // A preview is read-only and returns the exact closed selection used by confirmation.
        // 预览为只读操作，返回确认所用的准确封闭选择。
        let preview = action(
            &state,
            &app,
            &uuid::Uuid::new_v4().to_string(),
            "native_session_archive_preview",
            serde_json::to_value(vulcan_service_api::VmmNativeSessionArchiveSelection {
                vmm_project_id: project.id,
                session_source_ids: vec![session_id.to_string()],
                archived,
            })
            .expect("archive selection"),
        )
        .await
        .expect("real archive preview");
        let VmmMemoryActionResponse::NativeSessionArchivePreview(preview) = preview else {
            panic!("typed archive preview");
        };
        assert_eq!(
            runtime.storage_snapshot().expect("Core after preview"),
            expected
        );
        assert_eq!(
            controller.snapshot().expect("cache after preview"),
            cache_before
        );
        assert_eq!(
            events
                .events
                .lock()
                .expect("preview event observations")
                .len(),
            index
        );
        // Retries reuse this admitted command UUID and exact server preview, never a fresh destructive choice.
        // 重试复用此已准入命令 UUID 及准确服务端预览，绝不使用新的破坏性选择。
        let command = uuid::Uuid::new_v4().to_string();
        // Confirmation has no replacement session list or inferred project owner.
        // 确认不携带替换会话列表或推断的项目所有者。
        let payload = serde_json::json!({ "previewId": preview.preview_id, "confirmed": true });
        // Wait for the complete real service action before inspecting immediate projections.
        // 在检查即时投影前等待完整真实服务动作完成。
        let result = action(
            &state,
            &app,
            &command,
            "native_session_archive",
            payload.clone(),
        )
        .await
        .expect("real archive confirmation");
        let VmmMemoryActionResponse::NativeSessionArchive(result) = result else {
            panic!("typed archive confirmation");
        };
        expected.storage_version += 1;
        expected
            .memory_sessions
            .iter_mut()
            .find(|session| session.session_source_id.as_str() == session_id.to_string())
            .expect("exact archived metadata")
            .status = if archived {
            vulcan_memorymesh_core::StorageSessionStatus::Archived
        } else {
            vulcan_memorymesh_core::StorageSessionStatus::Active
        };
        assert_eq!(
            runtime
                .storage_snapshot()
                .expect("Core after classification"),
            expected
        );
        assert_eq!(result.storage_version, expected.storage_version);
        // The resident's authoritative lifecycle is visible before any new PreCheck or history hydration.
        // 在任何新 PreCheck 或历史水合之前，驻留权威生命周期已可见。
        let after = controller.snapshot().expect("immediate classified cache");
        assert_eq!(
            after.session_memory_status,
            if archived {
                SessionMemoryStatus::Archived
            } else {
                SessionMemoryStatus::Active
            }
        );
        assert_eq!(after.revision, cache_before.revision + 1);
        assert_eq!(
            after.committed_command_hashes.len(),
            cache_before.committed_command_hashes.len() + 1
        );
        assert!(
            after
                .committed_command_hashes
                .starts_with(&cache_before.committed_command_hashes)
        );
        assert!(after.has_committed_controller_command(&format!(
            "native_content_commit:{}:{}",
            result.storage_version, cache_before.revision
        )));
        // Normalize only the explicitly allowed lifecycle, receipt and mutation timestamp fields.
        // 仅规范化明确允许变化的生命周期、回执及修改时间字段。
        let mut expected_cache = cache_before.clone();
        expected_cache.session_memory_status = after.session_memory_status;
        expected_cache.revision += 1;
        expected_cache.committed_command_hashes = after.committed_command_hashes.clone();
        expected_cache.updated_at = after.updated_at.clone();
        assert_eq!(after, expected_cache);
        assert_eq!(
            vulcan_workspace_store::workspace_store::load_session_vmm_state(
                &root.v1_storage_locator()
            )
            .expect("read classified V1")
            .expect("classified V1 exists"),
            after
        );
        assert_eq!(
            other.snapshot().expect("other cache after classification"),
            other_before
        );
        assert_eq!(
            vulcan_workspace_store::workspace_store::load_session_vmm_state(
                &other_root.v1_storage_locator()
            )
            .expect("read other V1")
            .expect("other V1 exists"),
            other_before
        );
        {
            // The canonical panel and event must belong to this exact UUID and preserve its real memory body.
            // 规范面板及事件必须归属于此准确 UUID，并保留其真实记忆正文。
            let guard = state.lock().expect("classified workbench state");
            // The selected projection comes from the current resident controller, not a default or another session.
            // 选中投影来自当前驻留控制器，而非默认值或另一会话。
            let panel = guard
                .workbench
                .context_injection
                .as_ref()
                .expect("immediate context panel");
            assert!(panel.dynamic_cards.iter().any(|card| {
                card.tag == "VMM_MEMORY"
                    && card.items.iter().any(|item| {
                        item.memory_id == Some(written.items[0].memory_id)
                            && item.detail.contains(&memory.summary)
                    })
            }));
            assert_eq!(
                Some(panel.clone()),
                crate::workbench_session_panels::context_injection_snapshot_from_runtime_state(
                    guard
                        .workspace_snapshot
                        .as_ref()
                        .expect("classified workspace")
                        .session_runtime_state
                        .get(&session_id),
                    Some(&after)
                )
            );
        }
        // An identical service retry must not append another controller receipt or workbench event.
        // 完全相同的服务重试不得追加另一控制器回执或工作台事件。
        let replay = action(&state, &app, &command, "native_session_archive", payload)
            .await
            .expect("archive retry");
        assert_eq!(
            replay,
            VmmMemoryActionResponse::NativeSessionArchive(result)
        );
        assert_eq!(controller.snapshot().expect("cache after replay"), after);
        assert_eq!(
            events
                .events
                .lock()
                .expect("replay event observations")
                .len(),
            index + 1
        );
        assert_resident_list_status(
            &state,
            &app,
            project.id,
            session_id,
            other_id,
            if archived {
                vulcan_service_api::VmmNativeSessionStatus::Archived
            } else {
                vulcan_service_api::VmmNativeSessionStatus::Active
            },
        )
        .await;
        assert_eq!(
            data_plane
                .list_active_pending_analysis_jobs()
                .await
                .expect("queue after classification"),
            jobs
        );
        assert_eq!(
            (
                port.requests
                    .lock()
                    .expect("model observations after classification")
                    .len(),
                port.embedding_requests
                    .lock()
                    .expect("embedding observations after classification")
                    .len()
            ),
            calls
        );
        // SQLite metadata and physical vector storage remain independently observable after each transition.
        // 每次转换后，SQLite 元数据及物理向量存储仍可独立观察。
        let durable: vulcan_memorymesh_core::StorageSnapshot = serde_json::from_str(
            &data_plane
                .load_core_snapshot_json()
                .await
                .expect("load classified Core document")
                .expect("durable Core snapshot"),
        )
        .expect("decode classified Core document");
        assert_eq!(durable, expected.clone().without_embeddings());
        // No archive operation may replace or drop the real vector.
        // 任何归档操作均不得替换或丢弃真实向量。
        let current_vectors = data_plane
            .load_vectors(&vector_binding, &memory_keys)
            .await
            .expect("read classified physical vector");
        assert_eq!(current_vectors.len(), 1);
        assert_eq!(current_vectors[0].memory_id, vectors[0].memory_id);
        assert_eq!(current_vectors[0].vector, vectors[0].vector);
    }
    // Only the selected lifecycle changed; the second real consumer never receives an invalidation.
    // 只有选中生命周期变化；第二个真实消费方从未收到失效通知。
    {
        let captured = events
            .events
            .lock()
            .expect("final archive event observations");
        assert_eq!(captured.len(), 2);
        for event in captured.iter() {
            let ServiceEvent::WorkbenchDelta(envelope) = event else {
                panic!("workbench event");
            };
            let vulcan_service_api::WorkbenchDelta::ContextInjectionChanged(delta) =
                &envelope.delta
            else {
                panic!("exact context event");
            };
            assert_eq!(delta.conversation_id, session_id.to_string());
        }
    }
    // Active deletion is an ordinary runtime-client operation, independent from management classification.
    // 主动删除是常规运行时客户端操作，独立于管理分类。
    // Both resident consumers must observe the real content commit before another provider request.
    // 在另一个 Provider 请求之前，两个驻留消费方必须观察到真实内容提交。
    let cache_before_delete = controller
        .snapshot()
        .expect("selected cache before active deletion");
    // Preserve the independent consumer's own receipt lineage and immutable historical anchors.
    // 保留独立消费方自身的回执沿革及不可变历史锚点。
    let other_before_delete = other
        .snapshot()
        .expect("other cache before active deletion");
    // Read actual physical FTS identities before deletion, retaining unrelated matches for exact comparison.
    // 删除前读取实际物理全文身份，保留无关命中以进行准确比较。
    let lexical_before_delete = data_plane
        .search_lexical_page("PostgreSQL", 1024, 0)
        .await
        .expect("physical FTS before active deletion");
    assert!(lexical_before_delete.len() < 1024);
    assert!(
        lexical_before_delete
            .iter()
            .any(|hit| hit.memory_id == memory_key)
    );
    assert_eq!(
        data_plane
            .delete_vectors(&vector_binding, &[])
            .await
            .expect("empty physical deletion batch"),
        0
    );
    // This request uses the same declared fields as the AI-facing native client implementation.
    // 此请求使用面向 AI 的原生客户端实现所声明的相同字段。
    let deletion = vulcan_vmm::DeleteMemoriesRequest {
        runtime_session_id: Some(session_id.to_string()),
        user_id: 1,
        project_id: project.id,
        host_project_id: host_id.to_owned(),
        memory_ids: vec![written.items[0].memory_id],
        reason: "Remove the exact resident acceptance memory.".to_owned(),
    };
    // No synthetic storage transform or manual cache invalidation substitutes for this operation.
    // 不以合成存储转换或手动缓存失效替代此操作。
    let deleted = client
        .delete_memories(deletion.clone())
        .await
        .expect("actual native active deletion");
    assert_eq!(deleted.deleted_memory_ids, deletion.memory_ids);
    assert!(deleted.not_found_memory_ids.is_empty());
    assert_eq!(deleted.deleted_vector_rows, 1);
    // Core deletion removes only the exact memory and its contextual evidence, not its session or profiles.
    // Core 删除只移除准确记忆及其情境证据，不移除会话或画像。
    let after_delete = runtime
        .storage_snapshot()
        .expect("Core after active deletion");
    expected.storage_version += 1;
    expected
        .dataset
        .memories
        .retain(|record| record.id.as_str() != memory_key);
    expected
        .dataset
        .context_evidence
        .retain(|record| record.memory_id.as_str() != memory_key);
    // An ordinary Core deletion adds one durable receipt, unlike host-only archive classification.
    // 常规 Core 删除增加一条耐久回执，与仅宿主管理分类的归档不同。
    let new_receipts = after_delete
        .outcomes
        .iter()
        .filter(|entry| {
            !expected
                .outcomes
                .iter()
                .any(|previous| previous.idempotency_key == entry.idempotency_key)
        })
        .collect::<Vec<_>>();
    assert_eq!(new_receipts.len(), 1);
    // Validate every deterministic receipt field before including its generated identity in the snapshot.
    // 将生成身份纳入快照前，校验回执的每个确定性字段。
    let deletion_receipt = new_receipts[0];
    let vulcan_memorymesh_core::MutationOutcome::Committed {
        storage_version,
        chat_compact,
        receipt,
    } = &deletion_receipt.outcome
    else {
        panic!("active deletion must produce a committed receipt");
    };
    assert_eq!(*storage_version, expected.storage_version);
    assert!(chat_compact.is_none());
    assert_eq!(receipt.operation_id, deletion_receipt.operation_id);
    assert_eq!(receipt.idempotency_key, deletion_receipt.idempotency_key);
    assert_eq!(
        receipt.request_fingerprint,
        deletion_receipt.request_fingerprint
    );
    assert!(receipt.entity_bindings.is_empty());
    assert!(receipt.committed_versions.is_empty());
    assert_eq!(receipt.side_effect_summary, "delete");
    expected.outcomes.push(deletion_receipt.clone());
    // Storage snapshots expose the durable ledger in its authoritative idempotency-key order.
    // 存储快照按权威幂等键顺序暴露耐久账本。
    expected
        .outcomes
        .sort_by(|left, right| left.idempotency_key.cmp(&right.idempotency_key));
    assert_eq!(after_delete, expected);
    for (id, resident, storage_root, before_cache) in [
        (session_id, &controller, &root, &cache_before_delete),
        (other_id, &other, &other_root, &other_before_delete),
    ] {
        // Capture the committed cache directly; no new PreCheck or publication helper has run.
        // 直接捕获已提交缓存；尚未运行新 PreCheck 或发布辅助器。
        let after_cache = resident
            .snapshot()
            .expect("immediate cache after active deletion");
        assert!(after_cache.active_leases.is_empty());
        assert_eq!(
            after_cache.session_memory_status,
            SessionMemoryStatus::Active
        );
        assert_eq!(
            after_cache.cache_epoch.anchors,
            before_cache.cache_epoch.anchors
        );
        assert_eq!(
            after_cache.completed_turn_sequence,
            before_cache.completed_turn_sequence
        );
        assert_eq!(after_cache.read_source, before_cache.read_source);
        assert_eq!(after_cache.pending_prompt_events.len(), 1);
        assert_eq!(
            after_cache.pending_prompt_events[0].semantic_content_hash,
            before_cache
                .active_leases
                .values()
                .next()
                .expect("previous real resident lease")
                .semantic_content_hash
        );
        assert_eq!(
            after_cache.committed_command_hashes.len(),
            before_cache.committed_command_hashes.len() + 1
        );
        assert_eq!(
            vulcan_workspace_store::workspace_store::load_session_vmm_state(
                &storage_root.v1_storage_locator()
            )
            .expect("load actively deleted cache")
            .expect("persisted deleted cache"),
            after_cache
        );
        // Resolve the published row by its exact UUID, including the non-selected consumer.
        // 按准确 UUID 解析已发布行，包括未选中的消费方。
        let guard = state.lock().expect("active deletion workbench");
        // This canonical scoped row is produced by the same content observer as the selected mirror.
        // 此规范作用域行由与选中镜像相同的内容观察器生产。
        let panel = guard
            .workbench
            .context_injection_by_conversation
            .iter()
            .find(|entry| entry.conversation_id == id.to_string())
            .expect("exact deletion projection")
            .context_injection
            .as_ref()
            .expect("deletion panel");
        assert!(
            !panel
                .dynamic_cards
                .iter()
                .any(|card| card.tag == "VMM_MEMORY")
        );
    }
    assert!(
        data_plane
            .search_vectors(&vector_binding, vectors[0].vector.clone(), &physical_filter)
            .await
            .expect("physical deletion check")
            .is_empty()
    );
    // A physical repeat reports zero even though one identity was requested; counts are not inferred from input.
    // 物理重复删除即使请求了一个身份也报告零；计数不是从输入推断。
    assert_eq!(
        data_plane
            .delete_vectors(&vector_binding, &memory_keys)
            .await
            .expect("physical deletion of an absent vector"),
        0
    );
    // Re-ranking may change BM25 scores, but deletion must remove only the requested FTS identity.
    // 重新排序可能改变 BM25 分数，但删除只能移除所请求的全文身份。
    let lexical_after_delete = data_plane
        .search_lexical_page("PostgreSQL", 1024, 0)
        .await
        .expect("physical FTS after active deletion");
    assert!(lexical_after_delete.len() < 1024);
    assert_eq!(
        lexical_after_delete
            .iter()
            .map(|hit| hit.memory_id.as_str())
            .collect::<std::collections::BTreeSet<_>>(),
        lexical_before_delete
            .iter()
            .filter(|hit| hit.memory_id != memory_key)
            .map(|hit| hit.memory_id.as_str())
            .collect::<std::collections::BTreeSet<_>>()
    );
    // A repeated content deletion is a no-op and must not append duplicate cache receipts or notifications.
    // 重复内容删除是无操作，不得追加重复缓存回执或通知。
    let selected_after_delete = controller.snapshot().expect("selected deletion baseline");
    // Keep the second consumer's complete state, not only its empty lease count.
    // 保留第二个消费方的完整状态，不只保留空租约数量。
    let other_after_delete = other.snapshot().expect("other deletion baseline");
    // The public response distinguishes an already removed identity from a newly deleted one.
    // 公开响应区分已移除身份与本次新删除身份。
    let repeated = client
        .delete_memories(deletion.clone())
        .await
        .expect("repeat active deletion");
    assert!(repeated.deleted_memory_ids.is_empty());
    assert_eq!(repeated.not_found_memory_ids, deletion.memory_ids);
    assert_eq!(repeated.deleted_vector_rows, 0);
    assert_eq!(
        runtime
            .storage_snapshot()
            .expect("Core after repeat delete"),
        after_delete
    );
    assert_eq!(
        controller.snapshot().expect("cache after repeat delete"),
        selected_after_delete
    );
    assert_eq!(
        other.snapshot().expect("other after repeat delete"),
        other_after_delete
    );
    assert_eq!(
        data_plane
            .list_active_pending_analysis_jobs()
            .await
            .expect("queue after active deletion"),
        jobs
    );
    assert_eq!(
        (
            port.requests
                .lock()
                .expect("model calls after deletion")
                .len(),
            port.embedding_requests
                .lock()
                .expect("embedding calls after deletion")
                .len()
        ),
        calls
    );
    // The retained Core document must contain the same deletion already reflected by both V1 caches.
    // 保留 Core 文档必须包含两个 V1 缓存已经反映的同一删除。
    let durable_delete: vulcan_memorymesh_core::StorageSnapshot = serde_json::from_str(
        &data_plane
            .load_core_snapshot_json()
            .await
            .expect("load deleted Core snapshot")
            .expect("deleted Core document exists"),
    )
    .expect("decode deleted Core document");
    assert_eq!(durable_delete, after_delete.without_embeddings());
    // Exactly two additional invalidations belong to the content deletion, one per real resident consumer.
    // 内容删除恰好产生两个额外失效通知，每个真实驻留消费方一个。
    let deletion_events = events.events.lock().expect("active deletion events");
    assert_eq!(deletion_events.len(), 4);
    // Stable UUIDs, not identical display titles, identify the complete affected consumer set.
    // 稳定 UUID 而非相同显示标题标识完整受影响消费方集合。
    let affected = deletion_events[2..]
        .iter()
        .map(|event| {
            let ServiceEvent::WorkbenchDelta(envelope) = event else {
                panic!("deletion workbench event");
            };
            let vulcan_service_api::WorkbenchDelta::ContextInjectionChanged(delta) =
                &envelope.delta
            else {
                panic!("deletion context event");
            };
            delta.conversation_id.clone()
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        affected,
        std::collections::BTreeSet::from([session_id.to_string(), other_id.to_string()])
    );
}

/// Checks both exact UUID rows returned by the real archive fixture's management list.
/// 检查真实归档夹具管理列表返回的两个准确 UUID 行。
/// Takes service, emitter, owner project, selected/other UUIDs and expected selected status.
/// 接收服务、发送器、所有者项目、选中及另一 UUID 和选中状态期望值。
/// Returns after validating exact identities, titles, lifecycle states and the bounded row count.
/// 验证准确身份、标题、生命周期状态及有界行数后返回。
async fn assert_resident_list_status(
    state: &crate::SharedServiceBridgeState,
    app: &ServiceEventEmitter,
    project_id: u64,
    session_id: uuid::Uuid,
    other_id: uuid::Uuid,
    status: vulcan_service_api::VmmNativeSessionStatus,
) {
    // This public query includes archived rows explicitly and stays within the fixture's exact owner.
    // 此公开查询明确包含归档行，并限制在夹具的准确所有者内。
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
    // Decode the same response consumed by the native content page, with no response substitution.
    // 解码原生内容页面消费的相同响应，不替换响应。
    let result = action(
        state,
        app,
        &uuid::Uuid::new_v4().to_string(),
        "native_session_list",
        serde_json::to_value(query).expect("resident list query"),
    )
    .await
    .expect("real resident list");
    let VmmMemoryActionResponse::NativeSessionList(page) = result else {
        panic!("resident list response");
    };
    assert_eq!(page.total, 2);
    assert_eq!(page.sessions.len(), 2);
    for (id, expected_status) in [
        (session_id, status),
        (other_id, vulcan_service_api::VmmNativeSessionStatus::Active),
    ] {
        // Equal display titles must not merge the selected session with the unaffected consumer.
        // 相同显示标题不得把选中会话与未受影响消费方合并。
        let listed = page
            .sessions
            .iter()
            .find(|entry| entry.session_source_id == id.to_string())
            .expect("exact resident list identity");
        assert_eq!(listed.status, Some(expected_status));
        assert_eq!(listed.display_title.as_deref(), Some("驻留分类会话"));
    }
}
