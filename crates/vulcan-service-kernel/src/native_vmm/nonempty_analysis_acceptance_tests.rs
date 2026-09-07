//! Nonempty native L1/L2, profiles, provenance and physical-index acceptance.
//! 非空原生 L1/L2、画像、来源与物理索引验收。

use super::*;

#[path = "session_admission_acceptance_tests.rs"]
mod admission_acceptance;
#[path = "session_archive_acceptance_tests.rs"]
mod archive_acceptance;
#[path = "recycle_restore_acceptance_tests.rs"]
mod recycle_restore_acceptance;
#[path = "session_restart_acceptance_tests.rs"]
mod restart_acceptance;
#[path = "turn_details_acceptance_tests.rs"]
mod turn_details_acceptance;

#[path = "compaction_acceptance_tests.rs"]
mod compaction_acceptance;

#[path = "compaction_hook_acceptance_tests.rs"]
pub(in crate::native_vmm) mod compaction_hook_acceptance;

#[path = "global_config_save_acceptance_tests.rs"]
mod global_config_save_acceptance;

#[path = "profile_bundle_acceptance_tests.rs"]
pub(in crate::native_vmm) mod profile_bundle_acceptance;

/// Returns exact fixture JSON for the supplied production purpose and actual model-visible input.
/// 根据生产用途及模型实际可见输入返回准确夹具 JSON。
/// L2 echoes the emitted indexes, so renderer/parser disagreement cannot be hidden by canned indexes.
/// L2 回显实际发出的索引，避免固定索引掩盖渲染及解析不一致。
pub(super) fn response(purpose: &str, input: &serde_json::Value) -> serde_json::Value {
    match purpose {
        "profile_instruction" if input["instruction"] == "Refresh the project-policy fixture." => {
            // This explicit fixture instruction replaces the one exact active node supplied by production.
            // 此显式夹具指令替换生产输入提供的准确活动节点。
            let nodes = input["active_nodes"]
                .as_array()
                .expect("instruction active nodes")
                .iter()
                .filter(|node| node["content"] == "Project B requires transactional writes.")
                .collect::<Vec<_>>();
            assert_eq!(nodes.len(), 1, "one exact active refresh target");
            serde_json::json!({
                "accepted_nodes": [{
                    "normalized_content": "Project B requires transactional writes.",
                    "priority": "P1", "level": "L2", "level_reason": "Confirmed current project rule.",
                    "supersede_nodes": [{"node_id": nodes[0]["id"], "reason": "Explicit refresh."}]
                }],
                "retired_nodes": [], "reason": "Refresh the exact project rule."
            })
        }
        "profile_instruction" => serde_json::json!({
            "accepted_nodes": [{
                "normalized_content": input["instruction"], "priority": "P1", "level": "L2",
                "level_reason": "Explicit instruction in the authorized profile scope.", "supersede_nodes": []
            }],
            "retired_nodes": [], "reason": "Preserve the explicit authorized instruction."
        }),
        "precheck_l1" => serde_json::json!({
            "reason": "The current greeting needs no historical memory.",
            "need_memory": false, "queries": [],
        }),
        "postaction_l1" => serde_json::json!({
            "user_input_kind": "statement",
            "turn_id": input["target_turn"]["turn_id"],
            "details": "用户喜欢西瓜和葡萄，并选择 PostgreSQL 作为项目数据库。",
            "memory_nodes": [{
                "category": 0, "abstract": "项目数据库使用 PostgreSQL",
                "details": "用户明确决定项目使用 PostgreSQL 存储业务数据。",
                "evidence_source": "user_asserted", "admission": "keep", "admission_reason": ""
            }],
            "profile_nodes": [
                {"profile_type":0,"content":"用户喜欢西瓜。","evidence_source":"user_asserted","admission":"keep","admission_reason":""},
                {"profile_type":1,"content":"项目使用 PostgreSQL 数据库。","evidence_source":"user_asserted","admission":"keep","admission_reason":""},
                {"profile_type":0,"content":"用户喜欢葡萄。","evidence_source":"user_asserted","admission":"keep","admission_reason":""}
            ]
        }),
        "postaction_l2" => {
            // Mirror every actual request index without correcting any producer mistake in the fixture.
            // 原样回显每个实际请求索引，夹具不修正生产者的任何错误。
            let mut output = serde_json::json!({});
            // The production renderer omits sections with no candidates after hard deduplication.
            // 生产渲染器在硬去重后省略没有候选的分区。
            if let Some(memories) = input.get("memory") {
                output["memory"] = serde_json::json!({"accepted_candidates": memories.as_array().expect("memory candidates").iter().map(|candidate| {
                    serde_json::json!({"candidate_index":candidate["candidate_index"],"supersede_memory_ids":[]})
                }).collect::<Vec<_>>(), "dropped_candidates":[], "reason":"Explicit current decision."});
            }
            for role in ["user", "project"] {
                // An omitted role is not reviewed; a present role must contain the declared candidate array.
                // 省略的角色不参与审核；存在的角色必须包含契约声明的候选数组。
                let Some(section) = input.get(role) else {
                    continue;
                };
                output[role] = serde_json::json!({
                    "accepted_candidates": section["new_candidates"].as_array().expect("profile candidates").iter().map(|candidate| {
                        serde_json::json!({
                            "candidate_index":candidate["candidate_index"], "normalized_content":candidate["content"],
                            "priority":"P1", "level":"L2", "level_reason":"Explicit stable preference or project decision.", "supersede_node_ids":[]
                        })
                    }).collect::<Vec<_>>(),
                    "invalid_candidate_indexes":[], "retire_only_node_ids":[], "reason":"Explicit user evidence."
                });
            }
            output
        }
        _ => panic!("unexpected nonempty fixture purpose: {purpose}"),
    }
}

/// Builds a validated deterministic dense embedding response matching every actual input ID and dimension.
/// 构造与每个实际输入 ID 及维度匹配的已校验确定性稠密向量响应。
/// `host_request` remains unchanged; physical persistence still runs through production VLDB/LanceDB.
/// `host_request` 保持不变；物理持久化仍经过生产 VLDB/LanceDB。
pub(super) fn embedding_response(host_request: &HostAiEmbeddingRequest) -> EmbeddingResponse {
    // The native contract requests provider-default dense float, not semantic-similarity task routing.
    // 原生契约请求供应商默认稠密浮点，不使用语义相似度任务路由。
    let request = &host_request.request;
    assert_eq!(request.task, EmbeddingTask::ProviderDefault);
    assert_eq!(request.output_kind, EmbeddingOutputKind::Dense);
    assert_eq!(request.encoding, EmbeddingEncoding::Float);
    assert_eq!(request.dimensions, Some(1_536));
    EmbeddingResponse {
        request_id: request.request_id.clone(),
        route: ExecutionRouteSnapshot {
            provider_id: "fixture".to_owned(),
            service_entry_id: "fixture".to_owned(),
            connection_id: "fixture".to_owned(),
            model_id: "fixture".to_owned(),
            protocol_binding_id: Some("fixture-embedding".to_owned()),
            action_binding_id: None,
            secondary_route_id: None,
            secondary_provider_name: None,
            secondary_endpoint_name: None,
            capability_revision: "fixture-v1".to_owned(),
            catalog_revision: "fixture-v1".to_owned(),
            route_revision: "fixture-v1".to_owned(),
            usage_scope: InferenceUsageScope::InternalBackground,
        },
        fingerprint: EmbeddingSpaceFingerprint {
            provider_id: "fixture".to_owned(),
            service_entry_id: "fixture".to_owned(),
            deployment_id: None,
            secondary_route_id: None,
            model_id: "fixture".to_owned(),
            binding_id: "fixture-embedding".to_owned(),
            capability_revision: "fixture-v1".to_owned(),
            task: EmbeddingTask::ProviderDefault,
            dimensions: 1_536,
            output_kind: EmbeddingOutputKind::Dense,
            encoding: EmbeddingEncoding::Float,
            normalization: EmbeddingNormalization::Unknown,
            distance_metric: DistanceMetric::Unknown,
        },
        results: request
            .inputs
            .iter()
            .enumerate()
            .map(|(index, input)| EmbeddingResult {
                input_id: input.input_id.clone(),
                input_index: u32::try_from(index).expect("fixture input index"),
                embedding: EmbeddingPayload::Dense {
                    values: vec![1.0; 1_536],
                },
            })
            .collect(),
        dropped: Vec::new(),
        usage: RequestUsage::default(),
        duration_ms: 0,
    }
}

/// Executes one production queued turn that creates one memory and interleaved user/project profiles.
/// 执行一个生产排队回合，生成一条记忆及交错的用户和项目画像。
/// `data_plane` and `capability` belong to an isolated real controller and a fixture-only host AI route.
/// `data_plane` 与 `capability` 属于隔离真实控制器及仅供夹具使用的宿主 AI 路由。
/// Returns after verifying completion, physical storage, profile reads and exact maintenance provenance.
/// 在验证完成状态、物理存储、画像读取及准确维护来源后返回。
pub(in crate::native_vmm) async fn verify_nonempty_analysis(
    data_plane: VmmDataPlaneHandle,
    capability: InferenceCapabilitySnapshot,
) {
    // Only inference is deterministic; all native command, queue and persistence paths remain production code.
    // 仅推理使用确定性夹具；所有原生命令、队列及持久路径保持生产代码。
    let port = Arc::new(CapturingAnalysisPort {
        inner: MockHostAiPort::new(),
        requests: StdMutex::new(Vec::new()),
        reject_next_turn: AtomicBool::new(false),
        fixture: AnalysisFixture::Nonempty,
        embedding_requests: StdMutex::new(Vec::new()),
        profile_instruction_pause: StdMutex::new(None),
        postaction_pause: StdMutex::new(None),
    });
    port.inner
        .set_capability_snapshot(capability)
        .expect("nonempty capability");
    // Bootstrap against the temporary real data plane using only the configured fixture AI port.
    // 使用已配置的夹具 AI 端口，在临时真实数据面上引导。
    let runtime = Arc::new(
        NativeVmmCoreRuntime::bootstrap(port.clone(), data_plane.clone())
            .await
            .expect("nonempty runtime"),
    );
    // Isolate extraction from unrelated semantic-noise prototype embeddings while keeping normal intake.
    // 将提炼与无关的语义噪音原型向量隔离，同时保留正常入库。
    let mut settings = runtime
        .resource_catalog()
        .await
        .expect("resources")
        .settings;
    settings.noise_semantic_enabled = false;
    runtime
        .update_resource_settings(settings)
        .await
        .expect("disable unrelated semantic noise");
    // This stable memory ID owns all project records regardless of its display name.
    // 该稳定记忆 ID 拥有全部项目记录，不依赖显示名称。
    let project = data_plane
        .create_project("Nonempty", "nonempty")
        .await
        .expect("project");
    data_plane
        .bind_vulcan_project("nonempty-host", project.id)
        .await
        .expect("binding");
    // Distinct caller extraction defaults must reach durable records without changing profile policy.
    // 独立调用方提炼默认值必须进入持久记录，不能改变画像策略。
    let mut extraction = runtime
        .runtime_policy()
        .await
        .expect("global policy")
        .recall;
    extraction.postaction_memory_priority = 73;
    extraction.postaction_memory_stability =
        vulcan_service_api::VmmNativeExtractedMemoryStability::Archival;
    extraction.postaction_memory_level =
        vulcan_service_api::VmmNativeExtractedMemoryLevel::Persistent;
    extraction.postaction_memory_refresh_weight = 9;
    extraction.postaction_memory_expiry_ms = 31_536_000_001;
    // Share the actual maintenance/intake operation gate between both production facades.
    // 在两个生产门面之间共享实际维护与入库操作门。
    let gate = Arc::new(RwLock::new(()));
    // Save through the actual product service before the following queue materializes all five business defaults.
    // 在后续队列落地全部五项业务默认值之前，通过实际产品服务保存。
    let service = policy_service::service(runtime.clone(), data_plane.clone(), gate.clone());
    // Preserve inherited groups while supplying the exact caller-owned extraction rules.
    // 提供准确调用方提炼规则，同时保留继承组。
    let overrides = vulcan_service_api::VmmNativeHostProjectPolicyOverrides {
        recall: Some(extraction.clone()),
        ..Default::default()
    };
    // Only the formal dispatcher may produce this typed successful save response.
    // 仅正式派发器可以产生此类型化保存成功响应。
    let saved = policy_service::action(
        &service,
        "native_host_project_policy_update",
        serde_json::json!({
            "hostProjectId": "nonempty-host", "overrides": overrides
        }),
    )
    .await
    .expect("caller extraction service policy");
    // Do not accept a different successful action as evidence that the intended group was saved.
    // 不接受其他成功动作作为目标组已保存的证据。
    let vulcan_service_api::VmmMemoryActionResponse::NativeHostProjectPolicy(saved) = saved else {
        panic!("expected extraction policy response");
    };
    assert_eq!(saved.host_project_id, "nonempty-host");
    assert_eq!(saved.overrides, overrides);
    assert_eq!(saved.effective.recall, extraction);
    // Do not spawn a perpetual worker; call its actual bounded drain under explicit test ownership.
    // 不启动永久工作器；在测试明确拥有生命周期的前提下调用实际有界排空。
    let client = NativeVmmClient::new(
        runtime.clone(),
        data_plane.clone(),
        gate.clone(),
        Arc::new(AtomicBool::new(true)),
        Arc::new(Notify::new()),
    );
    client
        .post_action(PostActionRequest {
            host_project_id: "nonempty-host".to_owned(),
            project_id: project.id,
            user_id: 1,
            session_id: "nonempty-session".to_owned(),
            operation_id: "nonempty-operation".to_owned(),
            user_content: "我喜欢西瓜和葡萄。这个项目确定使用 PostgreSQL 作为数据库，请长期记住。"
                .to_owned(),
            assistant_content: "了解你的水果偏好以及项目数据库决定。".to_owned(),
            ..Default::default()
        })
        .await
        .expect("nonempty intake");
    assert!(
        client
            .drain_due_pending_analysis_jobs()
            .await
            .expect("nonempty drain")
    );
    // Read the durable acknowledgement rather than inferring completion from a successful intake response.
    // 读取耐久确认，不从成功入库响应推断提炼已经完成。
    let job = data_plane
        .load_pending_analysis_job("nonempty-operation")
        .await
        .expect("job")
        .expect("audit job");
    assert_eq!(
        job.state,
        vulcan_workspace_store::vmm_data_plane::VMM_PENDING_ANALYSIS_COMPLETED,
        "{:?}",
        job.last_error
    );
    assert_eq!(job.attempt_count, 0);
    // Inspect the exact structured input sent for review, including separate zero-based role indexes.
    // 检查送往审核的准确结构化输入，包含分组后独立从零开始的角色索引。
    let requests = port.requests.lock().expect("LLM captures").clone();
    assert_eq!(
        requests
            .iter()
            .map(|request| request.purpose_id.as_str())
            .collect::<Vec<_>>(),
        vec!["postaction_l1", "postaction_l2"]
    );
    // Decode the actual L2 request independently of the response fixture's accepted output.
    // 独立解码实际 L2 请求，不依赖响应夹具的接纳输出。
    let review = analysis_input(&requests[1]);
    assert_eq!(review["user"]["new_candidates"][0]["candidate_index"], 0);
    assert_eq!(review["user"]["new_candidates"][1]["candidate_index"], 1);
    assert_eq!(review["project"]["new_candidates"][0]["candidate_index"], 0);
    assert!(
        !port
            .embedding_requests
            .lock()
            .expect("embedding captures")
            .is_empty()
    );
    // Re-read SQLite facts and verify both source attribution and the independent fixed-user scope.
    // 重读 SQLite 事实，同时验证来源归属及独立的固定用户作用域。
    let snapshot: vulcan_memorymesh_core::StorageSnapshot = serde_json::from_str(
        &data_plane
            .load_core_snapshot_json()
            .await
            .expect("snapshot bytes")
            .expect("snapshot"),
    )
    .expect("typed snapshot");
    assert_eq!(snapshot.dataset.memories.len(), 1);
    assert_eq!(snapshot.dataset.profile_nodes.len(), 3);
    assert_eq!(snapshot.dataset.turns.len(), 1);
    assert_eq!(snapshot.dataset.turns[0].status, TurnStatus::Completed);
    // The single extracted decision must keep the source turn and owning project.
    // 唯一提取决定必须保留来源回合及归属项目。
    let memory = &snapshot.dataset.memories[0];
    assert_eq!(memory.priority, 73);
    assert_eq!(
        memory.stability,
        vulcan_memorymesh_core::MemoryStability::Archival
    );
    assert_eq!(
        memory.memory_level,
        vulcan_memorymesh_core::MemoryLevel::Persistent
    );
    assert_eq!(memory.refresh_weight, 9);
    assert_eq!(
        memory.expires_at_ns,
        Some(snapshot.dataset.turns[0].created_at_ns + 31_536_000_001_000_000)
    );
    assert_eq!(
        memory.source_turn_id.as_ref().map(|id| id.as_str()),
        job.turn_id.as_deref()
    );
    assert_eq!(
        memory.scope_id,
        project_scope_id(project.id).expect("project scope")
    );
    assert_eq!(
        snapshot
            .dataset
            .profile_nodes
            .iter()
            .filter(|node| node.scope_id == subject_scope_id(1).expect("fixed user scope"))
            .count(),
        2
    );
    assert_eq!(
        snapshot
            .dataset
            .profile_nodes
            .iter()
            .filter(|node| node.scope_id == project_scope_id(project.id).expect("project scope"))
            .count(),
        1
    );
    for node in &snapshot.dataset.profile_nodes {
        assert_eq!(node.source_turn_ids.len(), 1);
        assert_eq!(
            Some(node.source_turn_ids[0].as_str()),
            job.turn_id.as_deref()
        );
    }
    for (content, expected_scope) in [
        ("用户喜欢西瓜。", subject_scope_id(1).expect("user scope")),
        (
            "项目使用 PostgreSQL 数据库。",
            project_scope_id(project.id).expect("project scope"),
        ),
        ("用户喜欢葡萄。", subject_scope_id(1).expect("user scope")),
    ] {
        // Assert content-to-owner mapping, not merely the counts of each role.
        // 断言内容与归属的对应关系，而不仅比较每种角色的数量。
        let node = snapshot
            .dataset
            .profile_nodes
            .iter()
            .find(|node| node.content == content)
            .expect("exact accepted content");
        assert_eq!(node.scope_id, expected_scope);
    }
    // Resolve and read the actual LanceDB row, then query the actual full-text index.
    // 解析并读取实际 LanceDB 行，再查询实际全文索引。
    let vector_space = memory
        .vector_space_id
        .as_ref()
        .expect("memory vector space");
    // Resolve the sole physical binding recorded for this memory's logical vector generation.
    // 解析该记忆逻辑向量代次记录的唯一物理绑定。
    let binding = data_plane
        .resolve_vector_space(vector_space.as_str())
        .await
        .expect("physical binding")
        .expect("binding exists");
    // Load the stable memory primary key from LanceDB rather than reusing the SQLite vector copy.
    // 从 LanceDB 加载稳定记忆主键，不复用 SQLite 中的向量副本。
    let vectors = data_plane
        .load_vectors(&binding, &[memory.id.to_string()])
        .await
        .expect("physical vector row");
    assert_eq!(vectors.len(), 1);
    assert_eq!(vectors[0].memory_id, memory.id.as_str());
    assert_eq!(vectors[0].vector.len(), 1_536);
    assert!(
        (vectors[0]
            .vector
            .iter()
            .map(|value| f64::from(*value).powi(2))
            .sum::<f64>()
            - 1.0)
            .abs()
            < 0.0001
    );
    assert!(
        data_plane
            .search_lexical_page("PostgreSQL", 10, 0)
            .await
            .expect("physical FTS")
            .iter()
            .any(|hit| hit.memory_id == memory.id.as_str())
    );
    for (target, expected_count) in [
        (ProfileTarget::ProfileTargetUser, 2),
        (ProfileTarget::ProfileTargetProject, 1),
    ] {
        // Query each native profile class through the same client used by the application.
        // 通过应用实际使用的同一客户端查询每种原生画像类别。
        let response = client
            .get_profile_nodes(GetProfileNodesRequest {
                host_project_id: "nonempty-host".to_owned(),
                project_id: project.id,
                user_id: 1,
                target,
                limit: 10,
                ..Default::default()
            })
            .await
            .expect("native profile read");
        assert_eq!(response.nodes.len(), expected_count);
    }
    // The actual maintenance facade must expose the same per-turn generated identities.
    // 实际维护门面必须暴露相同的逐回合生成身份。
    // Search through the production client so physical provenance survives the public response boundary.
    // 通过生产客户端检索，确保物理来源保留到公开响应边界。
    let search = client
        .search_memory_events(SearchMemoryEventsRequest {
            host_project_id: "nonempty-host".to_owned(),
            project_id: project.id,
            user_id: 1,
            runtime_session_id: Some("nonempty-session".to_owned()),
            queries: vec!["PostgreSQL".to_owned()],
            top_k: 4,
        })
        .await
        .expect("native search with provenance");
    assert_eq!(search.results.len(), 1);
    assert_eq!(search.results[0].hits.len(), 1);
    let retrieval = search.results[0].hits[0]
        .retrieval
        .as_ref()
        .expect("observed native provenance");
    assert_eq!(retrieval.source_kind, "turn");
    assert_eq!(retrieval.origin, "hybrid_rrf");
    assert!(retrieval.final_score.is_finite());
    assert!(retrieval.context_observed);
    assert_eq!(retrieval.matched_context_score_delta, Some(0.0));
    let encoded = serde_json::to_value(&search).expect("search response JSON");
    assert_eq!(
        encoded["results"][0]["hits"][0]["retrieval"]["origin"],
        "hybrid_rrf"
    );
    let maintenance =
        crate::native_vmm::NativeVmmMaintenance::new(runtime.clone(), data_plane.clone(), gate);
    // Capture one coherent project/session projection for the visible source-record page.
    // 为可见来源记录页捕获一份一致的项目及会话投影。
    let records = maintenance
        .memory_records(project.id, Some("nonempty-session"))
        .await
        .expect("real maintenance records");
    assert_eq!(records.turns.len(), 1);
    assert_eq!(records.turns[0].extracted, Some(true));
    assert_eq!(records.turns[0].generated_memory_ids.len(), 1);
    assert_eq!(records.turns[0].generated_profile_node_ids.len(), 3);
    assert_eq!(records.memories.len(), 1);
    assert_eq!(records.profile_nodes.len(), 3);
    global_config_save_acceptance::verify(
        &runtime,
        &data_plane,
        &maintenance,
        project.id,
        &search.results[0].hits[0],
    )
    .await;
    compaction_acceptance::verify(
        &client,
        &data_plane,
        &maintenance,
        project.id,
        &search.results[0].hits[0],
        None,
    )
    .await;
    recycle_restore_acceptance::verify(
        &client,
        &runtime,
        &data_plane,
        &maintenance,
        &port,
        project.id,
    )
    .await;
    archive_acceptance::verify(&runtime, &maintenance, &data_plane, &port, project.id).await;
    verify_migration_and_continuation(
        &client,
        &runtime,
        &data_plane,
        &port,
        &maintenance,
        project.id,
    )
    .await;
    admission_acceptance::verify(&client, &runtime, &data_plane, &maintenance, &port).await;
    turn_details_acceptance::verify(&client, &runtime, &data_plane, &maintenance).await;
    restart_acceptance::verify(&client, &runtime, &data_plane, &maintenance, &port).await;
}

/// Moves extracted content through the real maintenance facade and resumes after explicit rebinding.
/// 通过真实维护门面移动已提炼内容，并在显式重新绑定后继续提炼。
/// Takes the isolated client, runtime, data plane, capturing port, maintenance and source project ID.
/// 接收隔离的客户端、运行时、数据面、捕获端口、维护门面及来源项目 ID。
/// Returns after validating source isolation, unchanged vectors, moved watermarks and subsequent history.
/// 在验证来源隔离、向量不变、水位移动及后续历史之后返回。
async fn verify_migration_and_continuation(
    client: &NativeVmmClient,
    runtime: &NativeVmmCoreRuntime,
    data_plane: &VmmDataPlaneHandle,
    port: &CapturingAnalysisPort,
    maintenance: &crate::native_vmm::NativeVmmMaintenance,
    source_project_id: u64,
) {
    // SQLite deliberately strips embeddings; compare its exact relational projection and test LanceDB separately.
    // SQLite 明确剥离向量；比较准确关系投影，并独立验证 LanceDB。
    let before = runtime
        .storage_snapshot()
        .expect("before migration")
        .without_embeddings();
    // A separate empty project avoids conflating explicit content movement with session-name conflicts.
    // 独立空项目避免将显式内容移动与会话名称冲突混为一谈。
    let target = data_plane
        .create_project("Moved", "moved")
        .await
        .expect("target project");
    // The physical vector binding and values must survive without an embedding call.
    // 物理向量绑定和值必须保留，且不调用向量模型。
    let binding = data_plane
        .resolve_vector_space(
            before.dataset.memories[0]
                .vector_space_id
                .as_ref()
                .expect("space")
                .as_str(),
        )
        .await
        .expect("resolve vector binding")
        .expect("binding");
    // Read the physical row before accepting a maintenance operation.
    // 在确认维护操作前读取物理行。
    let vector_before = data_plane
        .load_vectors(&binding, &[before.dataset.memories[0].id.to_string()])
        .await
        .expect("vector before");
    // Record inference counts independently of the storage snapshot.
    // 独立于存储快照记录推理次数。
    let embedding_count = port
        .embedding_requests
        .lock()
        .expect("embedding requests")
        .len();
    // A versioned read-only preview describes the exact content selected by the user.
    // 带版本的只读预检描述用户选中的准确内容。
    let preview = maintenance
        .preview_migration(source_project_id, target.id)
        .await
        .expect("migration preview");
    assert_eq!(preview.source.sessions, 1);
    assert_eq!(preview.source.turns, 1);
    assert_eq!(preview.source.memories, 1);
    assert_eq!(preview.source.profile_nodes, 1);
    assert_eq!(preview.target_before.sessions, 0);
    assert_eq!(preview.target_after.sessions, 1);
    assert!(preview.session_renames.is_empty());
    assert_eq!(
        runtime
            .storage_snapshot()
            .expect("preview unchanged")
            .without_embeddings(),
        before
    );
    maintenance
        .execute_migration(
            "nonempty-migration",
            source_project_id,
            target.id,
            preview.storage_version,
            &preview.preview_fingerprint,
            false,
        )
        .await
        .expect("confirmed content move");
    // Read authoritative SQLite after the real transform, not only the in-memory runtime.
    // 在真实转换后读取权威 SQLite，不只读取内存运行时。
    let moved: vulcan_memorymesh_core::StorageSnapshot = serde_json::from_str(
        &data_plane
            .load_core_snapshot_json()
            .await
            .expect("moved bytes")
            .expect("moved snapshot"),
    )
    .expect("typed moved snapshot");
    assert_eq!(
        moved,
        runtime
            .storage_snapshot()
            .expect("runtime agrees")
            .without_embeddings()
    );
    // Only the owning project changes; IDs, evidence, dates, values and source session remain exact.
    // 只有归属项目改变；身份、证据、日期、值及来源会话保持准确。
    let mut expected_memory = before.dataset.memories[0].clone();
    expected_memory.scope_id = project_scope_id(target.id).expect("target scope");
    assert_eq!(moved.dataset.memories, vec![expected_memory]);
    // Preserve the completed observation window when moving the authoritative session record.
    // 移动权威会话记录时保留已完成的观察窗口。
    let mut expected_session = before.memory_sessions[0].clone();
    expected_session.scope_id = project_scope_id(target.id).expect("target scope");
    assert_eq!(moved.memory_sessions, vec![expected_session.clone()]);
    for original in &before.dataset.profile_nodes {
        // Personal profiles are global; only project-owned nodes change their scope.
        // 个人画像为全局归属；只有项目节点修改作用域。
        let mut expected = original.clone();
        if expected.scope_id == project_scope_id(source_project_id).expect("source scope") {
            expected.scope_id = project_scope_id(target.id).expect("target scope");
        }
        assert_eq!(
            moved
                .dataset
                .profile_nodes
                .iter()
                .find(|node| node.id == expected.id),
            Some(&expected)
        );
    }
    assert_eq!(
        port.embedding_requests
            .lock()
            .expect("embedding requests")
            .len(),
        embedding_count
    );
    // Inspect the unchanged physical embedding values under the same stable memory key.
    // 通过同一稳定记忆键检查未变化的物理向量值。
    let vector_after = data_plane
        .load_vectors(&binding, &[before.dataset.memories[0].id.to_string()])
        .await
        .expect("vector after");
    assert_eq!(vector_after.len(), 1);
    assert_eq!(vector_after[0].memory_id, vector_before[0].memory_id);
    assert_eq!(vector_after[0].vector, vector_before[0].vector);
    // Repeating the preview must show an empty source instead of offering the same content again.
    // 再次预检必须显示空来源，而不是再次提供同一内容。
    let repeat = maintenance
        .preview_migration(source_project_id, target.id)
        .await
        .expect("empty source preview");
    assert_eq!(repeat.source, Default::default());
    assert!(repeat.records.is_empty());
    assert!(
        maintenance
            .memory_records(source_project_id, None)
            .await
            .expect("source records")
            .turns
            .is_empty()
    );
    // Binding is a separate user action; content migration must not silently redirect host projects.
    // 绑定是独立用户操作；内容迁移不得静默重定向宿主项目。
    assert_eq!(
        data_plane
            .resolve_project_access("nonempty-host")
            .await
            .expect("unchanged binding")
            .owner
            .id,
        source_project_id
    );
    data_plane
        .bind_vulcan_project("nonempty-host", target.id)
        .await
        .expect("explicit rebind");
    // Archived sessions remain writable and report their true lifecycle to the host.
    // 已归档会话仍可写入，并向宿主报告真实生命周期。
    let continued_response = client
        .post_action(PostActionRequest {
            host_project_id: "nonempty-host".to_owned(),
            project_id: target.id,
            user_id: 1,
            session_id: "nonempty-session".to_owned(),
            operation_id: "after-migration".to_owned(),
            user_content: "继续确认之前的项目数据库决定和水果偏好，请保留有效记录。".to_owned(),
            assistant_content: "继续使用已经确认的决定与偏好。".to_owned(),
            ..Default::default()
        })
        .await
        .expect("continued intake");
    assert!(continued_response.accepted);
    assert_eq!(
        continued_response.session_memory_status,
        SessionMemoryStatus::Archived
    );
    assert!(
        client
            .drain_due_pending_analysis_jobs()
            .await
            .expect("continued drain")
    );
    // Success must be acknowledged by the durable queue, not inferred from intake.
    // 成功必须由持久队列确认，不能从入库结果推断。
    let job = data_plane
        .load_pending_analysis_job("after-migration")
        .await
        .expect("continued job")
        .expect("job exists");
    assert_eq!(
        job.state,
        vulcan_workspace_store::vmm_data_plane::VMM_PENDING_ANALYSIS_COMPLETED,
        "{:?}",
        job.last_error
    );
    // Inspect the actual new L1 request and its exact completed predecessor.
    // 检查实际新 L1 请求及其准确的已完成前序回合。
    let requests = port.requests.lock().expect("requests").clone();
    let input = analysis_input(
        requests
            .iter()
            .filter(|request| request.purpose_id == "postaction_l1")
            .nth(1)
            .expect("second L1"),
    );
    assert_eq!(
        input["target_turn"]["turn_id"].as_str(),
        job.turn_id.as_deref()
    );
    assert_eq!(
        input["reference_turns"]
            .as_array()
            .expect("references")
            .len(),
        1
    );
    assert_eq!(
        input["reference_turns"][0]["turn_id"],
        before.dataset.turns[0].id.as_str()
    );
    // The moved watermark advances only for the newly completed analysis.
    // 已移动水位只为新完成的分析推进。
    let after = runtime.storage_snapshot().expect("continued snapshot");
    assert_eq!(after.memory_sessions.len(), 1);
    assert!(
        after.memory_sessions[0].last_extraction_observed_at_ns
            > expected_session.last_extraction_observed_at_ns
    );
    assert_eq!(after.memory_sessions[0].scope_id, expected_session.scope_id);
    assert_eq!(
        after.memory_sessions[0].status,
        vulcan_memorymesh_core::StorageSessionStatus::Archived
    );
    verify_paused_session_intake(client, runtime, data_plane, port, target.id).await;
}

/// Verifies both paused lifecycle states return authoritative status without model or queue side effects.
/// 验证两种暂停生命周期返回权威状态，且不产生模型或队列副作用。
/// Takes isolated production facades and the owning project; state seeding is test-only, not a management API.
/// 接收隔离生产门面及归属项目；状态设置仅供测试，不代表管理接口。
async fn verify_paused_session_intake(
    client: &NativeVmmClient,
    runtime: &NativeVmmCoreRuntime,
    data_plane: &VmmDataPlaneHandle,
    port: &CapturingAnalysisPort,
    project_id: u64,
) {
    for (stored, reported, operation) in [
        (
            vulcan_memorymesh_core::StorageSessionStatus::Recycled,
            SessionMemoryStatus::Recycled,
            "paused-recycled",
        ),
        (
            vulcan_memorymesh_core::StorageSessionStatus::Forgotten,
            SessionMemoryStatus::Forgotten,
            "paused-forgotten",
        ),
    ] {
        // Isolate runtime admission from the still-separate recycle/purge management implementation.
        // 将运行时准入与仍独立实施的回收、永久清除管理逻辑隔离。
        let version = runtime
            .storage_snapshot()
            .expect("seed version")
            .storage_version;
        runtime
            .transform_storage_snapshot(operation, "fixture-paused-state", version, |snapshot| {
                assert_eq!(snapshot.memory_sessions.len(), 1);
                snapshot.memory_sessions[0].status = stored;
                Ok(())
            })
            .await
            .expect("seed paused lifecycle");
        // Capture exact authoritative facts and AI counts before any attempted ordinary intake.
        // 在尝试普通入库前捕获准确权威事实及 AI 次数。
        let before = runtime.storage_snapshot().expect("paused before");
        let llm_count = port.requests.lock().expect("requests").len();
        let embedding_count = port.embedding_requests.lock().expect("embeddings").len();
        let precheck = client
            .pre_check(PreCheckRequest {
                host_project_id: "nonempty-host".to_owned(),
                project_id,
                user_id: 1,
                session_id: "nonempty-session".to_owned(),
                operation_id: operation.to_owned(),
                user_content: "检查暂停会话不应触发召回".to_owned(),
                ..Default::default()
            })
            .await
            .expect("paused precheck");
        assert_eq!(precheck.session_memory_status, reported);
        assert!(!precheck.should_inject);
        assert!(precheck.context_items.is_empty());
        let postaction = client
            .post_action(PostActionRequest {
                host_project_id: "nonempty-host".to_owned(),
                project_id,
                user_id: 1,
                session_id: "nonempty-session".to_owned(),
                operation_id: operation.to_owned(),
                user_content: "检查暂停会话不应产生新回合".to_owned(),
                assistant_content: "不写入记忆".to_owned(),
                ..Default::default()
            })
            .await
            .expect("paused postaction");
        assert_eq!(postaction.session_memory_status, reported);
        assert!(!postaction.accepted);
        // Every runtime tool uses the trusted caller session, even when requesting another record.
        // 每个运行时工具均使用可信调用方会话，即使请求的是其他记录。
        let search_error = client
            .search_memory_events(SearchMemoryEventsRequest {
                runtime_session_id: Some("nonempty-session".to_owned()),
                host_project_id: "nonempty-host".to_owned(),
                project_id,
                user_id: 1,
                queries: vec!["PostgreSQL".to_owned()],
                top_k: 1,
            })
            .await
            .expect_err("paused search");
        assert_eq!(search_error.kind, "session_memory_paused");
        // Use a real canonical ID so authorization failure cannot be confused with a missing record.
        // 使用真实规范 ID，避免将准入失败混淆为记录不存在。
        let source_turn_id =
            parse_core_numeric_id(&before.dataset.turns[0].id, "vmm-turn-").expect("turn id");
        let details_error = client
            .get_turn_details(GetTurnDetailsRequest {
                runtime_session_id: Some("nonempty-session".to_owned()),
                host_project_id: "nonempty-host".to_owned(),
                project_id,
                user_id: 1,
                turn_ids: vec![source_turn_id],
            })
            .await
            .expect_err("paused details");
        assert_eq!(details_error.kind, "session_memory_paused");
        let write_error = client
            .write_memories(WriteMemoriesRequest {
                session_id: "nonempty-session".to_owned(),
                host_project_id: "nonempty-host".to_owned(),
                project_id,
                user_id: 1,
                items: vec![vulcan_vmm::WriteMemoryItem {
                    abstract_text: "应被拦截的记忆".to_owned(),
                    details: "暂停会话不可主动写入".to_owned(),
                    ..Default::default()
                }],
            })
            .await
            .expect_err("paused write");
        assert_eq!(write_error.kind, "session_memory_paused");
        let delete_error = client
            .delete_memories(DeleteMemoriesRequest {
                runtime_session_id: Some("nonempty-session".to_owned()),
                host_project_id: "nonempty-host".to_owned(),
                project_id,
                user_id: 1,
                memory_ids: vec![
                    parse_core_numeric_id(&before.dataset.memories[0].id, "vmm-memory-")
                        .expect("memory id"),
                ],
                reason: "must not execute".to_owned(),
            })
            .await
            .expect_err("paused delete");
        assert_eq!(delete_error.kind, "session_memory_paused");
        let compact = client
            .chat_compact(ChatCompactRequest {
                session_id: "nonempty-session".to_owned(),
                operation_id: format!("{operation}-compact"),
                host_project_id: "nonempty-host".to_owned(),
                project_id,
                user_id: 1,
            })
            .await
            .expect("paused compact");
        assert!(!compact.accepted);
        assert!(!compact.updated);
        assert_eq!(compact.compacted_turn_id, 0);
        // Explicit management inspection remains available; it is not a runtime invocation.
        // 明确的管理查看仍然可用；它不属于运行时调用。
        let inspection = client
            .get_turn_details(GetTurnDetailsRequest {
                runtime_session_id: None,
                host_project_id: "nonempty-host".to_owned(),
                project_id,
                user_id: 1,
                turn_ids: vec![source_turn_id],
            })
            .await
            .expect("management inspection");
        assert_eq!(inspection.turns.len(), 1);
        assert!(
            data_plane
                .load_pending_analysis_job(operation)
                .await
                .expect("paused queue")
                .is_none()
        );
        assert_eq!(runtime.storage_snapshot().expect("paused after"), before);
        assert_eq!(port.requests.lock().expect("requests").len(), llm_count);
        assert_eq!(
            port.embedding_requests.lock().expect("embeddings").len(),
            embedding_count
        );
    }
}
