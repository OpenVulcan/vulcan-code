//! Real controller acceptance for native lexical production routing and mutation synchronization.
//! 原生词法生产路由与变更同步的真实控制器验收。

use super::*;
use std::path::PathBuf;
use vldb_controller_client::{
    ClientRegistration, ControllerClient, ControllerClientConfig, ControllerProcessMode,
    ControllerSqliteEnableRequest, SpaceKind, SpaceRegistration,
};
use vulcan_memorymesh_core::{
    CandidateQueryBoundary, MemoryCategory, MemoryId, MemoryLevel, MemoryRecord, MemoryScopeClass,
    MemorySourceKind, MemoryStability, ScopeAccess, ScopeId, SubjectId,
};
use vulcan_workspace_store::data_plane::{ControllerRuntimeHandle, ControllerRuntimeMetadata};
use vulcan_workspace_store::vmm_data_plane::bootstrap_vmm_data_plane;

#[path = "lexical_boundary_tests.rs"]
mod lexical_boundary_tests;

#[path = "precheck_origin_acceptance_tests.rs"]
mod precheck_origin_acceptance_tests;

/// Supplies only deterministic query embeddings while all storage effects execute on the production driver.
/// 仅提供确定性查询嵌入，全部存储效果仍在生产驱动执行。
struct FixedEmbeddingHost {
    /// Owns the real SQLite/LanceDB-backed effect adapter.
    /// 持有真实 SQLite/LanceDB 效果适配器。
    native: NativeVmmHostDriver,
    /// Counts actual Core embedding requests, proving the public command ran its embedding stage.
    /// 统计实际 Core 嵌入请求，证明公共命令执行了嵌入阶段。
    embedding_calls: usize,
}

impl AsyncHostDriver for FixedEmbeddingHost {
    /// Answers the exact two-dimensional watermelon fixture and delegates all other effects unchanged.
    /// 回答精确的二维西瓜查询夹具，原样委托全部其他效果。
    /// `request` retains its original effect ID; returns the typed result or a production driver error.
    /// `request` 保留原始效果 ID；返回强类型结果或生产驱动错误。
    fn execute_effect<'a>(
        &'a mut self,
        request: EffectRequest,
    ) -> Pin<Box<dyn Future<Output = Result<EffectResult, CoreError>> + Send + 'a>> {
        Box::pin(async move {
            if let HostEffect::Embedding(effect) = &request.effect {
                assert_eq!(effect.expected_dimension, 2);
                assert_eq!(effect.inputs, vec!["西瓜"]);
                self.embedding_calls += 1;
                return Ok(EffectResult {
                    effect_id: request.effect_id,
                    result: HostEffectResult::Embedding(EmbeddingBatchResult {
                        vectors: vec![vec![1.0, 0.0]],
                        result_indices: Vec::new(),
                        dropped: Vec::new(),
                    }),
                });
            }
            self.native.execute_effect(request).await
        })
    }
}

/// Drives nonempty real physical channels through the public Core search and its RRF projection.
/// 通过公共 Core 检索及其 RRF 投影驱动非空真实物理通道。
/// `runtime` and `before` are the isolated test's actual host and last committed snapshot.
/// `runtime` 和 `before` 是隔离测试的实际宿主及最后已提交快照。
async fn verify_nonempty_hybrid_search(runtime: &NativeVmmCoreRuntime, before: &StorageSnapshot) {
    // after holds three visible records: two lexical hits and a separate vector-only hit.
    // after 包含三条可见记录：两条词法命中和一条独立的纯向量命中。
    let mut after = before.clone();
    after.dataset.memories = (130..133)
        .map(|index| indexed_memory(index, "fusion"))
        .collect();
    after.dataset.memories[0].embedding = Some(vec![0.6, 0.8]);
    after.dataset.memories[1].embedding = Some(vec![1.0, 0.0]);
    after.dataset.memories[2].embedding = Some(vec![1.0, 0.0]);
    after.dataset.memories[2].summary = "数据库架构".to_owned();
    // The fixture explicitly owns this physical generation; no production capability is inferred or registered.
    // 夹具明确拥有此物理代次；不推断或注册任何生产能力。
    runtime
        .host
        .data_plane
        .ensure_vector_space("test-generation", 2, r#"{"fixture":"native-hybrid-v1"}"#)
        .await
        .expect("fixture vector binding");
    runtime
        .host
        .persist_prepared_storage_transition("fts-fusion", "test", before, &after, &())
        .await
        .expect("persist real vectors and text");
    // Persist real user ranking settings before preparing either immutable command snapshot.
    // 在准备两种不可变命令快照前持久化真实用户排序设置。
    let mut ranking = runtime
        .runtime_policy()
        .await
        .expect("read ranking defaults")
        .retrieval;
    ranking.rrf_k = "7".to_owned();
    ranking.rerank_top_n = 0;
    ranking.mmr_enabled = false;
    ranking.weibull_enabled = false;
    runtime
        .update_retrieval_policy(ranking.clone())
        .await
        .expect("persist ranking policy");
    // Two caller overrides query the same physical memory project without changing its ownership.
    // 两个调用方覆盖查询同一物理记忆项目，但不改变其归属。
    for (host_project_id, rrf_k) in [("fusion-host-a", 17), ("fusion-host-b", 31)] {
        let mut caller_ranking = ranking.clone();
        caller_ranking.rrf_k = rrf_k.to_string();
        runtime
            .update_host_project_policy(
                host_project_id,
                vulcan_service_api::VmmNativeHostProjectPolicyOverrides {
                    retrieval: Some(caller_ranking),
                    ..Default::default()
                },
            )
            .await
            .expect("save caller ranking");
    }
    for (host_project_id, rrf_k, mode) in
        [("", 7_u64), ("fusion-host-a", 17), ("fusion-host-b", 31)]
            .into_iter()
            .flat_map(|(id, k)| {
                [
                    (
                        id,
                        k,
                        vulcan_memorymesh_core::CandidateReadMode::SeparateChannels,
                    ),
                    (
                        id,
                        k,
                        vulcan_memorymesh_core::CandidateReadMode::SingleObservation,
                    ),
                ]
            })
    {
        // bundle freezes both physical contracts and disables optional ranking stages for an exact fusion assertion.
        // bundle 冻结两个物理契约并关闭可选排序阶段，以进行精确融合断言。
        let mut bundle = runtime
            .resources
            .current_host_project_bundle("native-hybrid-v1".to_owned(), 2, host_project_id)
            .expect("prepare actual saved ranking");
        bundle.name = "native-fts-fusion-test".to_owned();
        bundle.memory_policy.retrieval_policy.candidate_read_mode = mode;
        bundle.memory_policy.retrieval_policy.context.enabled = false;
        assert_eq!(bundle.memory_policy.retrieval_policy.rrf_k, rrf_k);
        runtime
            .core
            .register_config(bundle)
            .expect("register exact native scoring");
        runtime
            .core
            .activate_config("native-fts-fusion-test")
            .expect("activate test policy");
        // host substitutes only query inference, leaving physical reads and Core ranking untouched.
        // host 仅替换查询推理，不修改物理读取和 Core 排序。
        let mut host = FixedEmbeddingHost {
            native: runtime.host.clone(),
            embedding_calls: 0,
        };
        let result = runtime
            .core
            .run_to_completion_async(
                CoreCommand::MemorySearch(vulcan_memorymesh_core::MemorySearchCommand {
                    subject_id: SubjectId::new("subject").expect("subject"),
                    scope_access: ScopeAccess {
                        write_scope: ScopeId::new("fusion").expect("scope"),
                        visible_scopes: vec![ScopeId::new("fusion").expect("scope")],
                    },
                    query: "西瓜".to_owned(),
                    limit: 3,
                    vector_space_id: VectorSpaceId::new("test-generation").expect("generation"),
                    boundary: CandidateQueryBoundary {
                        candidate_scope: None,
                        session_recall_boundary: None,
                    },
                    now_ns: 10,
                }),
                &mut host,
            )
            .await
            .expect("real native hybrid search");
        let CommandResult::MemorySearch(hits) = result else {
            panic!("search result expected");
        };
        assert_eq!(host.embedding_calls, 1);
        assert_eq!(
            hits.iter()
                .map(|hit| hit.memory.id.as_str())
                .collect::<Vec<_>>(),
            vec!["vmm-memory-0131", "vmm-memory-0130", "vmm-memory-0132"]
        );
        for (hit, expected_vector_rank, expected_lexical_rank) in [
            (&hits[0], 0_u32, Some(1_u32)),
            (&hits[1], 2, Some(0)),
            (&hits[2], 1, None),
        ] {
            // explanation proves actual per-channel ranks, exact contributions and final projection.
            // explanation 证明实际通道排名、精确贡献及最终投影。
            let explanation = hit.explanation.as_ref().expect("score provenance");
            assert_eq!(
                explanation
                    .vector_channel
                    .as_ref()
                    .expect("vector provenance")
                    .physical_rank,
                expected_vector_rank
            );
            assert_eq!(
                explanation
                    .lexical_channel
                    .as_ref()
                    .map(|channel| channel.physical_rank),
                expected_lexical_rank
            );
            let denominator = (rrf_k + 1) as f64;
            let expected_rrf = 1.0 / (denominator + f64::from(expected_vector_rank))
                + expected_lexical_rank.map_or(0.0, |rank| 1.0 / (denominator + f64::from(rank)));
            assert_eq!(explanation.rrf_score, Some(expected_rrf));
            assert_eq!(explanation.rerank_score, None);
            if let Some(channel) = &explanation.lexical_channel {
                assert_eq!(channel.descriptor, lexical::score_contract());
                assert!(explanation.raw_lexical_score.expect("raw BM25") < 0.0);
            } else {
                assert_eq!(explanation.raw_lexical_score, None);
            }
        }
    }
}

/// Builds a text-only fixture with an explicit query generation; no model or vector inference is exercised.
/// 构造具有明确查询代次的纯文本夹具；不执行模型或向量推理。
/// `index` determines the stable identity; `scope` determines authoritative project visibility.
/// `index` 确定稳定身份；`scope` 确定权威项目可见性。
fn indexed_memory(index: usize, scope: &str) -> MemoryRecord {
    MemoryRecord {
        id: MemoryId::new(format!("vmm-memory-{index:04}")).expect("id"),
        access_class: MemoryAccessClass::SubjectPrivate,
        source_subject_id: Some(SubjectId::new("subject").expect("subject")),
        scope_id: ScopeId::new(scope).expect("scope"),
        scope_class: MemoryScopeClass::Environment,
        category: MemoryCategory::General,
        summary: "用户喜欢西瓜".to_owned(),
        content: "exact fixture details".to_owned(),
        source_kind: MemorySourceKind::Direct,
        origin_session_source_id: None,
        priority: 20,
        stability: MemoryStability::Stable,
        tags: Vec::new(),
        evidence: Vec::new(),
        evidence_source: None,
        state: MemoryState::Active,
        importance: 0.2,
        confidence: Some(0.9),
        created_at_ns: 1,
        updated_at_ns: 1,
        expires_at_ns: None,
        last_recalled_at_ns: None,
        reinforcement_count: 0,
        memory_level: MemoryLevel::Stable,
        refresh_weight: 1,
        dedupe_hash: None,
        recalled_count: 0,
        adopted_count: 0,
        cross_session_adopted_count: 0,
        decay_disabled: false,
        status_reason: None,
        last_adopted_at_ns: None,
        last_reinforced_at_ns: None,
        source_turn_id: None,
        version: 1,
        vector_space_id: Some(VectorSpaceId::new("test-generation").expect("generation")),
        embedding: None,
    }
}

/// Queries the real driver so physical paging, authority filtering and final score projection all execute.
/// 查询真实驱动，使物理分页、权限过滤及最终评分投影均实际执行。
/// `scope` and `text` form the exact query; returns the materialized lexical channel.
/// `scope` 与 `text` 构成精确查询；返回物化后的词法通道。
async fn search(host: &NativeVmmHostDriver, scope: &str, text: &str) -> CandidateChannelResult {
    // query deliberately requests two results beyond a full invisible physical page.
    // query 有意请求整页不可见物理命中之后的两条结果。
    let query = StorageQuery::LexicalCandidates {
        subject_id: SubjectId::new("subject").expect("subject"),
        scope_access: ScopeAccess {
            write_scope: ScopeId::new(scope).expect("scope"),
            visible_scopes: vec![ScopeId::new(scope).expect("scope")],
        },
        queries: vec![text.to_owned()],
        vector_space_id: VectorSpaceId::new("test-generation").expect("generation"),
        boundary: CandidateQueryBoundary {
            candidate_scope: None,
            session_recall_boundary: None,
        },
        limit: 2,
        expected_contract: lexical::score_contract().contract,
        now_ns: 10,
    };
    let HostEffectResult::StorageQuery(StorageQueryResult::LexicalCandidates(result)) = host
        .execute_storage_query(&query)
        .await
        .expect("driver query")
    else {
        panic!("lexical result expected");
    };
    result
}

/// Verifies saved cleanup settings change real Core planning and durable records without a model route.
/// 验证已保存清洗设置改变真实 Core 规划及持久记录，且不依赖模型路由。
async fn verify_saved_cleanup_policy_on_real_storage(runtime: NativeVmmCoreRuntime) {
    // The default project is created by bootstrap; unrelated lexical fixtures remain outside it.
    // 默认项目由引导创建；无关词法夹具仍位于该项目之外。
    let data_plane = runtime.host.data_plane.clone();
    let runtime = Arc::new(runtime);
    let maintenance = crate::native_vmm::NativeVmmMaintenance::new(
        Arc::clone(&runtime),
        data_plane,
        Arc::new(tokio::sync::RwLock::new(())),
    );
    maintenance.set_native_backend_enabled(true);
    verify_explicit_lexical_rebuild(&runtime, &maintenance).await;
    let before = runtime.storage_snapshot().expect("before cleanup fixture");
    let mut protected = indexed_memory(9000, "vmm-project:1");
    // Unlike lexical-only fixtures, a complete Core memory fact must include its declared vector.
    // 与仅词法夹具不同，完整 Core 记忆事实必须包含其已声明的向量。
    protected.embedding = Some(vec![0.6, 0.8]);
    protected.source_subject_id = Some(SubjectId::new("vmm-subject:1").expect("native subject"));
    protected.access_class = MemoryAccessClass::ScopeShared;
    protected.state = MemoryState::Deleted;
    runtime
        .transform_storage_snapshot(
            "cleanup-seed",
            "test",
            before.storage_version,
            move |snapshot| {
                snapshot.dataset.memories.push(protected);
                Ok(())
            },
        )
        .await
        .expect("persist cleanup fixture");
    let first = maintenance
        .run_retention_once()
        .await
        .expect("default cleanup");
    assert!(
        first.project_failures.is_empty(),
        "{:?}",
        first.project_failures
    );
    assert_eq!(first.recycled_memories, 0);
    assert!(
        runtime
            .storage_snapshot()
            .expect("protected snapshot")
            .dataset
            .memories
            .iter()
            .any(|memory| memory.id.as_str() == "vmm-memory-9000")
    );
    let mut settings = runtime.retention_settings().await.expect("read cleanup");
    settings.enabled = false;
    settings.scan_interval_minutes = 7;
    settings.profile_expiry_interval_seconds = 11;
    settings.protected_memory_level_floor = 4;
    settings.trash_retention_ms = 86_400_000;
    let saved = maintenance
        .update_retention_settings(settings.clone())
        .await
        .expect("save cleanup");
    assert_eq!(saved.retention, settings);
    let status = maintenance
        .retention_status()
        .await
        .expect("cleanup status");
    assert!(!status.enabled);
    assert_eq!(status.interval_minutes, 7);
    assert_eq!(status.profile_expiry_interval_seconds, 11);
    assert_eq!(status.policy.protected_memory_level_floor, 4);
    // Seed twelve completed turns so preview must include the newly enqueued cold-job follow-up.
    // 放入十二个已处理回合，使预览必须包含新入队冷作业的后续执行。
    let cold_version = runtime
        .storage_snapshot()
        .expect("before cold preview seed")
        .storage_version;
    runtime
        .transform_storage_snapshot("preview-cold-seed", "test", cold_version, |snapshot| {
            for index in 1..=12 {
                snapshot.dataset.turns.push(vulcan_memorymesh_core::Turn {
                    id: vulcan_memorymesh_core::TurnId::new(format!("preview-cold-{index}"))
                        .expect("turn"),
                    subject_id: SubjectId::new("vmm-subject:1").expect("subject"),
                    scope_id: ScopeId::new("vmm-project:1").expect("scope"),
                    session_source_id: vulcan_memorymesh_core::SessionSourceId::new(
                        "preview-cold-session",
                    )
                    .expect("session"),
                    timeline: Vec::new(),
                    previous_turn_ids: Vec::new(),
                    next_turn_ids: Vec::new(),
                    created_at_ns: index,
                    status: vulcan_memorymesh_core::TurnStatus::ProcessedUnknown,
                    user_input_kind: None,
                    analysis_details: None,
                });
            }
            Ok(())
        })
        .await
        .expect("persist cold preview fixture");
    // Preview is read-only, and neither wrong tokens nor stale policy may execute its targets.
    // 预览只读，错误令牌与过期策略均不得执行其目标。
    let preview_before = serde_json::to_value(runtime.storage_snapshot().expect("before preview"))
        .expect("serialize before");
    let preview = maintenance
        .preview_retention_cleanup()
        .await
        .expect("manual preview");
    assert_eq!(
        preview.projects[0].recycled_memory_ids,
        vec!["vmm-memory-9000"]
    );
    assert_eq!(preview.projects[0].recycled_turn_ids.len(), 4);
    assert_eq!(
        serde_json::to_value(runtime.storage_snapshot().expect("after preview"))
            .expect("serialize after"),
        preview_before
    );
    assert!(
        maintenance
            .execute_retention_cleanup("wrong-preview", "not-issued")
            .await
            .is_err()
    );
    let replaced = maintenance
        .preview_retention_cleanup()
        .await
        .expect("replace preview");
    assert!(
        maintenance
            .execute_retention_cleanup("replaced-preview", &preview.preview_id)
            .await
            .is_err()
    );
    let mut changed_settings = settings.clone();
    changed_settings.protected_priority_floor += 1;
    maintenance
        .update_retention_settings(changed_settings)
        .await
        .expect("change preview policy");
    assert!(
        maintenance
            .execute_retention_cleanup("changed-policy", &replaced.preview_id)
            .await
            .expect_err("policy changed")
            .contains("规则已改变")
    );
    maintenance
        .update_retention_settings(settings.clone())
        .await
        .expect("restore policy");
    let stale = maintenance
        .preview_retention_cleanup()
        .await
        .expect("version preview");
    runtime
        .transform_storage_snapshot(
            "preview-version-change",
            "test",
            stale.storage_version,
            |_| Ok(()),
        )
        .await
        .expect("version change");
    assert!(
        maintenance
            .execute_retention_cleanup("stale-storage", &stale.preview_id)
            .await
            .expect_err("version changed")
            .contains("规则已改变")
    );
    // The fresh plan executes exactly the displayed identities even with the periodic switch off.
    // 即使周期清洗开关关闭，新计划也准确执行显示的标识。
    let confirmed = maintenance
        .preview_retention_cleanup()
        .await
        .expect("fresh preview");
    // Time spent reading the preview must not shorten newly created recycle windows.
    // 阅读预览耗费的时间不得缩短新建回收批次的保留期限。
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let confirmation_started_at_ns = i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos(),
    )
    .expect("nanoseconds");
    let second = maintenance
        .execute_retention_cleanup("confirmed-cleanup", &confirmed.preview_id)
        .await
        .expect("confirmed manual cleanup remains available");
    assert!(
        second.project_failures.is_empty(),
        "{:?}",
        second.project_failures
    );
    assert_eq!(second.recycled_memories, 1);
    assert_eq!(second.recycled_turns, 4);
    assert_eq!(second.enqueued_cold_jobs, 1);
    assert_eq!(second.completed_cold_jobs, 1);
    // Repeated delivery reuses the recorded outcome, while another preview cannot reuse that operation.
    // 重复投递复用已记录结果，另一个预览则不能复用该操作。
    let committed_version = runtime
        .storage_snapshot()
        .expect("committed version")
        .storage_version;
    assert_eq!(
        maintenance
            .execute_retention_cleanup("confirmed-cleanup", &confirmed.preview_id)
            .await
            .expect("idempotent confirmation"),
        second
    );
    assert_eq!(
        runtime
            .storage_snapshot()
            .expect("replayed version")
            .storage_version,
        committed_version
    );
    let unrelated_preview = maintenance
        .preview_retention_cleanup()
        .await
        .expect("next preview");
    assert!(
        maintenance
            .execute_retention_cleanup("confirmed-cleanup", &unrelated_preview.preview_id)
            .await
            .is_err()
    );
    let recycled = runtime.storage_snapshot().expect("recycled snapshot");
    assert!(
        !recycled
            .dataset
            .memories
            .iter()
            .any(|memory| memory.id.as_str() == "vmm-memory-9000")
    );
    let batch = recycled
        .retention_trash_batches
        .iter()
        .find(|batch| {
            batch
                .dataset
                .memories
                .iter()
                .any(|memory| memory.id.as_str() == "vmm-memory-9000")
        })
        .expect("real recycle batch");
    assert_eq!(
        batch.expires_at_ns - batch.recycled_at_ns,
        86_400_000_000_000
    );
    let frozen_expiry = batch.expires_at_ns;
    // Inspect batches created by actual cleanup through the public maintenance facade, never a fabricated active dataset.
    // 通过公开维护门面查看真实清洗创建的批次，绝不伪造活动数据集。
    let recycle_bin = maintenance
        .recycle_bin(None)
        .await
        .expect("real recycle list");
    for summary in &recycle_bin.batches {
        for (kind, count) in [
            (
                vulcan_service_api::VmmNativeRecycleRecordKind::Turn,
                summary.turn_count,
            ),
            (
                vulcan_service_api::VmmNativeRecycleRecordKind::Memory,
                summary.memory_count,
            ),
            (
                vulcan_service_api::VmmNativeRecycleRecordKind::Profile,
                summary.profile_node_count,
            ),
            (
                vulcan_service_api::VmmNativeRecycleRecordKind::Evidence,
                summary.context_evidence_count,
            ),
        ] {
            let page = maintenance
                .recycle_records(vulcan_service_api::VmmNativeRecycleRecordQuery {
                    vmm_project_id: summary.project_id,
                    batch_id: summary.batch_id.clone(),
                    record_kind: kind,
                    offset: 0,
                    limit: 100,
                    expected_storage_version: Some(recycle_bin.storage_version),
                })
                .await
                .expect("real retained records");
            assert_eq!(page.total, count);
            assert_eq!(page.records.len() as u64, count);
            for record in page.records {
                if let vulcan_service_api::VmmNativeRecycleRecord::Memory {
                    memory_id,
                    content,
                    ..
                } = record
                {
                    let original = recycled
                        .retention_trash_batches
                        .iter()
                        .find(|item| item.batch_id == summary.batch_id)
                        .expect("retained batch")
                        .dataset
                        .memories
                        .iter()
                        .find(|item| item.id.as_str() == memory_id)
                        .expect("original memory");
                    assert_eq!(content, original.content);
                }
            }
        }
    }
    assert_eq!(
        runtime
            .storage_snapshot()
            .expect("unchanged after inspection"),
        recycled
    );
    assert!(batch.recycled_at_ns >= confirmation_started_at_ns);
    assert_eq!(
        batch.dataset.memories[0].embedding.as_deref(),
        Some([0.6_f32, 0.8].as_slice())
    );
    settings.trash_retention_ms = 1;
    maintenance
        .update_retention_settings(settings)
        .await
        .expect("shorten default");
    let third = maintenance
        .run_retention_once()
        .await
        .expect("changed default cleanup");
    assert!(
        third.project_failures.is_empty(),
        "{:?}",
        third.project_failures
    );
    assert_eq!(third.purged_trash_batches, 0);
    let after = runtime.storage_snapshot().expect("old deadline unchanged");
    assert_eq!(
        after.retention_trash_batches[0].expires_at_ns,
        frozen_expiry
    );
    // The mock has no registered capability; real cleanup must not attempt AI admission.
    // 模拟端口没有注册能力；真实清洗不得尝试 AI 准入。
    assert!(!maintenance.host_ai_status().checked);
    project_retention_acceptance_tests::verify(&runtime, &maintenance).await;
    // A real terminal vector-bearing record is admitted only after the worker's initial pass finishes.
    // 真实带向量终态记录仅在工作器首轮完成后准入。
    let mut scheduled = indexed_memory(9300, "vmm-project:1");
    scheduled.embedding = Some(vec![0.6, 0.8]);
    scheduled.source_subject_id = Some(SubjectId::new("vmm-subject:1").expect("fixed subject"));
    scheduled.state = MemoryState::Deleted;
    // Environment scope is protected independently of access class; use a level below the configured floor.
    // 环境作用域的保护独立于访问类别；此处使用低于已配置保护阈值的等级。
    scheduled.memory_level = MemoryLevel::Phase;
    crate::native_vmm::maintenance::retention_worker_tests::verify_worker_lifecycle(
        maintenance,
        scheduled,
    )
    .await;
}

#[path = "project_retention_acceptance_tests.rs"]
mod project_retention_acceptance_tests;

/// Creates an isolated real controller; returns its directory owner, runtime handle and shutdown client.
/// 创建隔离的真实控制器；返回目录所有者、运行时句柄与关闭客户端。
async fn isolated_controller() -> (tempfile::TempDir, ControllerRuntimeHandle, ControllerClient) {
    // temporary owns the entire isolated controller database tree for this acceptance.
    // temporary 拥有本次验收的完整隔离控制器数据库树。
    let temporary = tempfile::tempdir().expect("isolated database root");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("ephemeral endpoint");
    let port = listener.local_addr().expect("port").port();
    drop(listener);
    // executable is the real bundled VLDB server already used by the workspace-store acceptance.
    // executable 是工作区存储验收已经使用的真实随附 VLDB 服务端。
    let executable =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../output/bin/vldb-controller.exe");
    assert!(executable.is_file(), "bundled controller must exist");
    let endpoint = format!("http://127.0.0.1:{port}");
    let client = ControllerClient::new(
        ControllerClientConfig {
            endpoint: endpoint.clone(),
            auto_spawn: true,
            spawn_executable: Some(executable.display().to_string()),
            spawn_process_mode: ControllerProcessMode::Managed,
            minimum_uptime_secs: 0,
            idle_timeout_secs: 1,
            default_lease_ttl_secs: 30,
            connect_timeout_secs: 30,
            startup_timeout_secs: 15,
            startup_retry_interval_ms: 100,
            lease_renew_interval_secs: 10,
        },
        ClientRegistration {
            client_name: format!("vmm-driver-test-{}", uuid::Uuid::new_v4()),
            host_kind: "test".to_owned(),
            process_id: std::process::id(),
            process_name: "vmm-driver-test".to_owned(),
            lease_ttl_secs: Some(30),
        },
    );
    client.connect().await.expect("controller");
    let data_root = temporary.path().join("data");
    std::fs::create_dir_all(&data_root).expect("data root");
    let app_path = data_root.join("app.sqlite3");
    client
        .attach_space(SpaceRegistration {
            space_id: "app".to_owned(),
            space_label: "test".to_owned(),
            space_kind: SpaceKind::User,
            space_root: data_root.display().to_string(),
        })
        .await
        .expect("app space");
    client
        .enable_sqlite(ControllerSqliteEnableRequest {
            space_id: "app".to_owned(),
            binding_id: "test-app".to_owned(),
            db_path: app_path.display().to_string(),
            ..Default::default()
        })
        .await
        .expect("app sqlite");
    let controller = ControllerRuntimeHandle::new(
        client.clone(),
        ControllerRuntimeMetadata {
            endpoint,
            executable: executable.display().to_string(),
            data_root,
            sqlite_path: app_path.clone(),
            sqlite_binding_id: "test-app".to_owned(),
            system_statistics_sqlite_path: app_path,
            system_statistics_sqlite_binding_id: "test-app".to_owned(),
        },
    );
    (temporary, controller, client)
}

/// Verifies production indexing and reads against temporary SQLite, never the user's databases or model route.
/// 对临时 SQLite 验证生产索引与读取，绝不使用用户数据库或模型路由。
#[tokio::test]
async fn real_controller_lexical_driver_pages_and_tracks_mutations() {
    // Retain the isolated directory until the controller is explicitly shut down.
    // 保留隔离目录，直到控制器被明确关闭。
    let (_temporary, controller, client) = isolated_controller().await;
    // runtime uses the production bootstrap; the unregistered mock AI port must remain unused.
    // runtime 使用生产引导；未注册能力的模拟 AI 端口必须保持未使用。
    let runtime = NativeVmmCoreRuntime::bootstrap(
        Arc::new(vulcan_ai_contract::test_support::MockHostAiPort::new()),
        bootstrap_vmm_data_plane(&controller)
            .await
            .expect("VMM data plane"),
    )
    .await
    .expect("native runtime");
    let before = InMemoryStorage::new().snapshot();
    let mut after = before.clone();
    after.dataset.memories = (0..132)
        .map(|index| indexed_memory(index, if index < 130 { "hidden" } else { "selected" }))
        .collect();
    // Persist through the same admitted journal/commit path used by native maintenance.
    // 通过原生维护使用的同一接纳账本及提交路径持久化。
    runtime
        .host
        .persist_prepared_storage_transition("fts-seed", "test", &before, &after, &())
        .await
        .expect("seed transition");
    let found = search(&runtime.host, "selected", "西瓜").await;
    assert_eq!(
        found
            .candidates
            .iter()
            .map(|hit| hit.memory_id.as_str())
            .collect::<Vec<_>>(),
        vec!["vmm-memory-0130", "vmm-memory-0131"]
    );
    assert_eq!(
        found
            .candidates
            .iter()
            .map(|hit| hit.score.decision_score)
            .collect::<Vec<_>>(),
        vec![Some(1.0), Some(0.75)]
    );
    assert!(
        found
            .candidates
            .iter()
            .all(|hit| hit.score.raw_value < 0.0 && hit.score.raw_score_kind == RawScoreKind::Bm25)
    );
    // Expired Active records remain physically indexed but must not enter the visible candidate pool.
    // 已过期的活跃记录仍保留物理索引，但不得进入可见候选池。
    let mut expired = after.clone();
    expired.dataset.memories[130].expires_at_ns = Some(5);
    runtime
        .host
        .persist_prepared_storage_transition("fts-expiry", "test", &after, &expired, &())
        .await
        .expect("expiry transition");
    assert_eq!(
        search(&runtime.host, "selected", "西瓜")
            .await
            .candidates
            .iter()
            .map(|hit| hit.memory_id.as_str())
            .collect::<Vec<_>>(),
        vec!["vmm-memory-0131"]
    );
    runtime
        .host
        .persist_prepared_storage_transition("fts-expiry-reset", "test", &expired, &after, &())
        .await
        .expect("restore expiry fixture");
    // observation must use the same production lexical path, not invoke the reference token-overlap store.
    // observation 必须使用同一生产词法路径，不能调用参考词元重合度存储。
    let mut observation = vulcan_memorymesh_core::CandidateObservationQuery {
        subject_id: SubjectId::new("subject").expect("subject"),
        scope_access: ScopeAccess {
            write_scope: ScopeId::new("selected").expect("scope"),
            visible_scopes: vec![ScopeId::new("selected").expect("scope")],
        },
        query: "西瓜".to_owned(),
        query_embedding: vec![1.0, 0.0],
        vector_space_id: VectorSpaceId::new("test-generation").expect("generation"),
        boundary: CandidateQueryBoundary {
            candidate_scope: None,
            session_recall_boundary: None,
        },
        limit: 2,
        vector_contract: vulcan_memorymesh_core::RetrievalPolicy::v1()
            .vector_contract
            .contract,
        lexical_contract: Some(lexical::score_contract().contract),
        now_ns: 10,
    };
    let HostEffectResult::StorageQuery(StorageQueryResult::CandidateObservation(combined)) =
        runtime
            .host
            .execute_storage_query(&StorageQuery::CandidateObservation(observation.clone()))
            .await
            .expect("complete observation")
    else {
        panic!("observation result expected");
    };
    assert!(combined.vector_channel.candidates.is_empty());
    assert_eq!(
        combined
            .lexical_channel
            .expect("enabled lexical")
            .candidates,
        found.candidates
    );
    assert_eq!(combined.memories.len(), 2);
    observation.lexical_contract = None;
    let HostEffectResult::StorageQuery(StorageQueryResult::CandidateObservation(disabled)) =
        runtime
            .host
            .execute_storage_query(&StorageQuery::CandidateObservation(observation))
            .await
            .expect("disabled lexical observation")
    else {
        panic!("observation result expected");
    };
    assert!(disabled.lexical_channel.is_none());
    assert!(disabled.memories.is_empty());
    // moved updates ownership without changing IDs or embedding; only the new project may read it.
    // moved 更新归属但不改变 ID 或嵌入；仅新项目能够读取。
    let mut moved = after.clone();
    for memory in &mut moved.dataset.memories[130..] {
        memory.scope_id = ScopeId::new("destination").expect("scope");
    }
    runtime
        .host
        .persist_prepared_storage_transition("fts-move", "test", &after, &moved, &())
        .await
        .expect("move transition");
    assert!(
        search(&runtime.host, "selected", "西瓜")
            .await
            .candidates
            .is_empty()
    );
    assert_eq!(
        search(&runtime.host, "destination", "西瓜")
            .await
            .candidates
            .len(),
        2
    );
    // retired removes index rows through lifecycle state, and restoration uses the same stable identity.
    // retired 通过生命周期状态移除索引行，恢复时使用同一稳定身份。
    let mut retired = moved.clone();
    retired.dataset.memories[130].state = MemoryState::Deleted;
    runtime
        .host
        .persist_prepared_storage_transition("fts-retire", "test", &moved, &retired, &())
        .await
        .expect("retire");
    assert_eq!(
        search(&runtime.host, "destination", "西瓜")
            .await
            .candidates
            .len(),
        1
    );
    runtime
        .host
        .persist_prepared_storage_transition("fts-restore", "test", &retired, &moved, &())
        .await
        .expect("restore");
    assert_eq!(
        search(&runtime.host, "destination", "西瓜")
            .await
            .candidates
            .len(),
        2
    );
    // cleared removes the destination contents while leaving unrelated projects untouched.
    // cleared 清除目标内容，同时保留不相关项目。
    let mut cleared = moved.clone();
    cleared.dataset.memories.truncate(130);
    runtime
        .host
        .persist_prepared_storage_transition("fts-clear", "test", &moved, &cleared, &())
        .await
        .expect("clear");
    assert!(
        search(&runtime.host, "destination", "西瓜")
            .await
            .candidates
            .is_empty()
    );
    assert_eq!(
        search(&runtime.host, "hidden", "西瓜")
            .await
            .candidates
            .len(),
        2
    );
    lexical_boundary_tests::verify_query_boundaries(&runtime, &cleared).await;
    verify_nonempty_hybrid_search(&runtime, &cleared).await;
    verify_saved_cleanup_policy_on_real_storage(runtime).await;
    client
        .shutdown()
        .await
        .expect("isolated controller shutdown");
}

/// Verifies queued native extraction on its own vector generation and isolated controller.
/// 在独立向量代次及隔离控制器上验证排队的原生提炼。
#[tokio::test]
async fn real_controller_pending_analysis_preserves_caller_policy() {
    let (_temporary, controller, client) = isolated_controller().await;
    crate::native_vmm::client::pending_analysis_acceptance_tests::verify_caller_policy_in_real_queue(
        bootstrap_vmm_data_plane(&controller).await.expect("analysis data plane"),
        super::tests::embedding_capability_handle(BTreeSet::from([EmbeddingTask::ProviderDefault])).snapshot,
    ).await;
    client
        .shutdown()
        .await
        .expect("analysis controller shutdown");
}

/// Verifies both editable text controls through real service saves, durable intake and queued L1 input.
/// 通过真实服务保存、耐久入库及排队 L1 输入验证两个可编辑文本控制项。
#[tokio::test]
async fn real_controller_text_policy_preserves_admitted_fragments() {
    // This temporary controller owns every policy file, queue row and canonical turn used below.
    // 此临时控制器持有下文使用的每份策略文件、队列记录及规范回合。
    let (_temporary, controller, client) = isolated_controller().await;
    crate::native_vmm::client::pending_analysis_acceptance_tests::text_policy_service::verify(
        bootstrap_vmm_data_plane(&controller)
            .await
            .expect("text data plane"),
        super::tests::embedding_capability_handle(BTreeSet::from([EmbeddingTask::ProviderDefault]))
            .snapshot,
    )
    .await;
    client.shutdown().await.expect("text controller shutdown");
}

/// Verifies saved precheck controls through the real public native client and its host AI boundary.
/// 通过真实公开原生客户端及其宿主 AI 边界验证已保存预检控制项。
#[tokio::test]
async fn real_controller_precheck_policy_controls_admitted_commands() {
    // Own every native policy file and physical index in one isolated controller lifetime.
    // 在一个隔离控制器生命周期内持有每份原生策略文件及物理索引。
    let (_temporary, controller, client) = isolated_controller().await;
    crate::native_vmm::client::pending_analysis_acceptance_tests::precheck_policy::verify(
        bootstrap_vmm_data_plane(&controller)
            .await
            .expect("precheck policy data plane"),
        super::tests::embedding_capability_handle(BTreeSet::from([EmbeddingTask::ProviderDefault]))
            .snapshot,
    )
    .await;
    client
        .shutdown()
        .await
        .expect("precheck policy controller shutdown");
}

/// Verifies saved decay controls against actual search-time bounds and durable precheck adoption.
/// 对照真实搜索时间边界及持久预检采纳验证已保存衰减控制项。
#[tokio::test]
async fn real_controller_weibull_policy_controls_lifecycle_scores() {
    // Keep every policy and lifecycle mutation inside one private controller lifetime.
    // 将每份策略及生命周期变更保留在一个私有控制器生命周期内。
    let (_temporary, controller, client) = isolated_controller().await;
    crate::native_vmm::client::pending_analysis_acceptance_tests::weibull_policy::verify(
        bootstrap_vmm_data_plane(&controller)
            .await
            .expect("decay data plane"),
        super::tests::embedding_capability_handle(BTreeSet::from([EmbeddingTask::ProviderDefault]))
            .snapshot,
    )
    .await;
    client.shutdown().await.expect("decay controller shutdown");
}

/// Verifies saved replacement controls through direct writes, real queued analysis and durable states.
/// 通过主动写入、真实队列分析及持久状态验证已保存替代控制。
/// Takes no input; shuts down its private controller after the full acceptance succeeds.
/// 无输入；完整验收成功后关闭私有控制器。
#[tokio::test]
async fn real_controller_replacement_policy_controls_actual_writes() {
    // Every source and configuration mutation remains inside this isolated controller lifetime.
    // 每份来源及配置变更均保留在此隔离控制器生命周期内。
    let (_temporary, controller, client) = isolated_controller().await;
    crate::native_vmm::client::pending_analysis_acceptance_tests::replacement_policy::verify(
        bootstrap_vmm_data_plane(&controller)
            .await
            .expect("replacement data plane"),
        super::tests::embedding_capability_handle(BTreeSet::from([EmbeddingTask::ProviderDefault]))
            .snapshot,
    )
    .await;
    client
        .shutdown()
        .await
        .expect("replacement controller shutdown");
}

/// Runs real component HTTP phases against private storage; requires the explicit handshake directory.
/// 针对私有存储运行真实组件 HTTP 阶段；要求显式握手目录。
/// Takes no arguments and verifies all phases before shutting down the isolated controller.
/// 不接收参数，验证全部阶段后关闭隔离控制器。
#[tokio::test]
#[ignore = "Requires the explicit private HTTP frontend acceptance runner."]
async fn real_controller_policy_components_over_http() {
    // The caller must declare this test's directory; no user-service path is inferred.
    // 调用者必须声明本测试目录；不推断用户服务路径。
    let root = PathBuf::from(
        std::env::var("VULCAN_VMM_HTTP_ACCEPTANCE_DIR")
            .expect("explicit private HTTP handshake directory"),
    );
    assert!(root.is_absolute() && root.is_dir());
    // All relational and vector changes belong to this independent controller lifetime.
    // 全部关系及向量变更属于此独立控制器生命周期。
    let (_temporary, controller, client) = isolated_controller().await;
    crate::native_vmm::client::pending_analysis_acceptance_tests::replacement_policy::network::verify(
        bootstrap_vmm_data_plane(&controller).await.expect("HTTP data plane"),
        super::tests::embedding_capability_handle(BTreeSet::from([EmbeddingTask::ProviderDefault])).snapshot,
        &root,
    ).await;
    client.shutdown().await.expect("HTTP controller shutdown");
}

/// Serves the built workbench against a private native runtime for explicit browser acceptance.
/// 向显式浏览器验收提供连接私有原生运行时的已构建工作台。
/// Requires declared handshake/assets directories; returns after independent save/restore assertions and controller shutdown.
/// 要求声明握手／资源目录；独立保存恢复断言及控制器关闭后返回。
#[tokio::test]
#[ignore = "Requires explicit private workbench browser acceptance."]
async fn real_controller_native_workbench_over_http() {
    // Neither the user's service discovery nor the normal application data directory is consulted.
    // 不查询用户服务发现或正常应用数据目录。
    let root = PathBuf::from(
        std::env::var("VULCAN_VMM_WORKBENCH_ACCEPTANCE_DIR").expect("private workbench handshake"),
    );
    // Static files must come from an explicitly built frontend, not a synthetic page.
    // 静态文件必须来自显式构建的前端，而非合成页面。
    let assets = PathBuf::from(
        std::env::var("VULCAN_VMM_WORKBENCH_ASSETS_DIR").expect("built workbench assets"),
    );
    assert!(root.is_absolute() && root.is_dir());
    assert!(assets.is_absolute() && assets.join("index.html").is_file());
    // The real controller owns only disposable isolated SQLite and vector data.
    // 真实控制器仅持有可丢弃的隔离关系及向量数据。
    let (_temporary, controller, client) = isolated_controller().await;
    crate::native_vmm::client::pending_analysis_acceptance_tests::replacement_policy::network::workbench::verify(
        bootstrap_vmm_data_plane(&controller).await.expect("workbench data plane"),
        super::tests::embedding_capability_handle(BTreeSet::from([EmbeddingTask::ProviderDefault])).snapshot,
        &root,
        &assets,
    ).await;
    client
        .shutdown()
        .await
        .expect("workbench controller shutdown");
}

/// Verifies saved hybrid and RRF controls through actual vector/full-text search.
/// 通过真实向量／全文搜索验证已保存混合及倒数排名控制项。
#[tokio::test]
async fn real_controller_hybrid_policy_controls_physical_search() {
    // Keep every source, index and policy inside the private controller lifetime.
    // 将每份来源、索引及策略保留在私有控制器生命周期中。
    let (_temporary, controller, client) = isolated_controller().await;
    crate::native_vmm::client::pending_analysis_acceptance_tests::hybrid_policy::verify(
        bootstrap_vmm_data_plane(&controller)
            .await
            .expect("hybrid data plane"),
        super::tests::embedding_capability_handle(BTreeSet::from([EmbeddingTask::ProviderDefault]))
            .snapshot,
    )
    .await;
    client.shutdown().await.expect("hybrid controller shutdown");
}

/// Verifies formal rerank saves through the public search client and actual physical candidates.
/// 通过公开搜索客户端与真实物理候选验证正式重排保存。
#[tokio::test]
async fn real_controller_rerank_policy_controls_actual_search() {
    // Own all policy files and indexes in one private controller lifetime.
    // 在一个私有控制器生命周期内持有全部策略文件与索引。
    let (_temporary, controller, client) = isolated_controller().await;
    crate::native_vmm::client::pending_analysis_acceptance_tests::rerank_policy::verify(
        bootstrap_vmm_data_plane(&controller)
            .await
            .expect("rerank data plane"),
        super::tests::embedding_capability_handle(BTreeSet::from([EmbeddingTask::ProviderDefault]))
            .snapshot,
    )
    .await;
    client.shutdown().await.expect("rerank controller shutdown");
}

/// Verifies formal analysis-budget saves across real queued and already admitted PostAction commands.
/// 跨真实排队及已准入 PostAction 命令验证正式分析预算保存。
#[tokio::test]
async fn real_controller_postaction_budget_policy_freezes_admitted_analysis() {
    // Keep all policy files, canonical sources and queue receipts inside one private controller lifetime.
    // 将全部策略文件、规范来源及队列回执限定在一个私有控制器生命周期中。
    let (_temporary, controller, client) = isolated_controller().await;
    crate::native_vmm::client::pending_analysis_acceptance_tests::postaction_budget::verify(
        bootstrap_vmm_data_plane(&controller)
            .await
            .expect("postaction budget data plane"),
        super::tests::embedding_capability_handle(BTreeSet::from([EmbeddingTask::ProviderDefault]))
            .snapshot,
    )
    .await;
    client
        .shutdown()
        .await
        .expect("postaction budget controller shutdown");
}

/// Verifies saved history limits against real completed and pending turns at the PreCheck AI boundary.
/// 在预检 AI 边界对真实已完成及待处理回合验证保存的历史限制。
#[tokio::test]
async fn real_controller_precheck_history_policy_preserves_mixed_turns() {
    // The temporary controller owns the real policy files, turn records and pending jobs for this case.
    // 临时控制器持有此用例的真实策略文件、回合记录及待处理作业。
    let (_temporary, controller, client) = isolated_controller().await;
    crate::native_vmm::client::pending_analysis_acceptance_tests::precheck_policy::history::verify(
        bootstrap_vmm_data_plane(&controller)
            .await
            .expect("precheck history data plane"),
        super::tests::embedding_capability_handle(BTreeSet::from([EmbeddingTask::ProviderDefault]))
            .snapshot,
    )
    .await;
    client
        .shutdown()
        .await
        .expect("history controller shutdown");
}

/// Verifies nonempty extraction, both profile roles and physical indexes through the actual native queue.
/// 通过实际原生队列验证非空提炼、两种画像角色及物理索引。
#[tokio::test]
async fn real_controller_nonempty_analysis_materializes_memory_and_profiles() {
    let (_temporary, controller, client) = isolated_controller().await;
    crate::native_vmm::client::pending_analysis_acceptance_tests::nonempty::verify_nonempty_analysis(
        bootstrap_vmm_data_plane(&controller).await.expect("nonempty data plane"),
        super::tests::embedding_capability_handle(BTreeSet::from([EmbeddingTask::ProviderDefault])).snapshot,
    ).await;
    client
        .shutdown()
        .await
        .expect("nonempty controller shutdown");
}

/// Verifies the installed asynchronous VMM Hook after a real durable V1 compaction.
/// 验证真实耐久 V1 压缩后的已安装异步 VMM Hook。
#[tokio::test]
async fn real_controller_compaction_dispatches_builtin_vmm_hook() {
    // Each acceptance owns its controller process, bindings and temporary database root.
    // 每个验收拥有自身的控制器进程、绑定及临时数据库根目录。
    let (_temporary, controller, client) = isolated_controller().await;
    crate::native_vmm::client::pending_analysis_acceptance_tests::nonempty::compaction_hook_acceptance::verify(
        bootstrap_vmm_data_plane(&controller).await.expect("compaction Hook data plane"),
        super::tests::embedding_capability_handle(BTreeSet::from([EmbeddingTask::ProviderDefault])).snapshot,
    ).await;
    client
        .shutdown()
        .await
        .expect("compaction Hook controller shutdown");
}

/// Verifies queued and manual profile lifetimes against stable memory ownership in real storage.
/// 在真实存储中按稳定记忆归属验证排队及手动画像生命周期。
#[tokio::test]
async fn real_controller_project_profile_policy_preserves_fixed_user_and_queued_owner() {
    let (_temporary, controller, client) = isolated_controller().await;
    crate::native_vmm::client::pending_analysis_acceptance_tests::project_profiles::verify(
        bootstrap_vmm_data_plane(&controller)
            .await
            .expect("profile policy data plane"),
        super::tests::embedding_capability_handle(BTreeSet::from([EmbeddingTask::ProviderDefault]))
            .snapshot,
    )
    .await;
    client
        .shutdown()
        .await
        .expect("profile policy controller shutdown");
}

/// Verifies profile bundle partitions and write isolation through the real client and persisted scopes.
/// 通过真实客户端及持久化作用域验证画像包分区及写入隔离。
#[tokio::test]
async fn real_controller_profile_bundles_preserve_scope_and_mode() {
    let (_temporary, controller, client) = isolated_controller().await;
    crate::native_vmm::client::pending_analysis_acceptance_tests::nonempty::profile_bundle_acceptance::verify(
        bootstrap_vmm_data_plane(&controller).await.expect("profile bundle data plane"),
        super::tests::embedding_capability_handle(BTreeSet::from([EmbeddingTask::ProviderDefault])).snapshot,
    ).await;
    client
        .shutdown()
        .await
        .expect("profile bundle controller shutdown");
}

/// Verifies explicit full-text reconstruction repairs derived text without touching authoritative state.
/// 验证显式全文重建修复派生文本且不触碰权威状态。
/// Uses the already-bootstrapped controller/runtime and maintenance facade; no new provider is registered.
/// 使用已引导的控制器、运行时与维护门面，不注册新供应商。
async fn verify_explicit_lexical_rebuild(
    runtime: &NativeVmmCoreRuntime,
    maintenance: &crate::native_vmm::NativeVmmMaintenance,
) {
    // Preserve domain records, embeddings, recycle contents and the stable binding catalog for comparison.
    // 保存领域记录、向量、回收内容和稳定绑定目录用于比较。
    let before = runtime.storage_snapshot().expect("before lexical repair");
    let catalog_before = maintenance.project_catalog().await.expect("catalog");
    let active: Vec<_> = before
        .dataset
        .memories
        .iter()
        .filter(|memory| memory.state == MemoryState::Active)
        .collect();
    assert!(active.len() >= 2);
    let missing_id = active[0].id.to_string();
    let stale_id = active[1].id.to_string();
    // Delete one document, alter another, and add one orphan: equal row counts must not imply integrity.
    // 删除一个文档、篡改另一个并增加一个孤立行，行数相等不能代表完整。
    runtime
        .host
        .data_plane
        .delete_lexical_documents(std::slice::from_ref(&missing_id))
        .await
        .expect("remove derived row only");
    runtime
        .host
        .data_plane
        .upsert_lexical_documents(&[
            VmmLexicalDocument {
                memory_id: String::from("orphan-lexical-memory"),
                summary: String::from("孤立鲸鱼"),
                details: String::from("not in authority"),
            },
            VmmLexicalDocument {
                memory_id: stale_id,
                summary: String::from("陈旧刺猬"),
                details: String::from("incorrect derived text"),
            },
        ])
        .await
        .expect("seed stale derived rows");
    let status = maintenance
        .lexical_index_status()
        .await
        .expect("read actual index status");
    assert_eq!(status.active_memory_count, active.len() as u64);
    assert_eq!(status.document_count, status.active_memory_count);
    assert_eq!(status.indexed_row_count, status.active_memory_count);
    assert_eq!(
        runtime
            .host
            .data_plane
            .search_lexical_page("鲸鱼", 10, 0)
            .await
            .expect("orphan before repair")
            .len(),
        1
    );
    assert!(
        maintenance
            .rebuild_lexical_index(status.storage_version, false)
            .await
            .is_err()
    );
    assert!(
        maintenance
            .rebuild_lexical_index(status.storage_version + 1, true)
            .await
            .is_err()
    );
    maintenance.set_native_backend_enabled(false);
    assert!(
        maintenance
            .rebuild_lexical_index(status.storage_version, true)
            .await
            .is_err()
    );
    maintenance.set_native_backend_enabled(true);
    assert_eq!(
        runtime
            .host
            .data_plane
            .search_lexical_page("鲸鱼", 10, 0)
            .await
            .expect("guards did not repair")
            .len(),
        1
    );
    // One accepted action replaces derived text and runs the real upstream Jieba rebuild.
    // 一个准入动作替换派生文本并运行真实上游 Jieba 重建。
    let result = maintenance
        .rebuild_lexical_index(status.storage_version, true)
        .await
        .expect("explicit full-text rebuild");
    assert_eq!(result.reindexed_memory_count, status.active_memory_count);
    assert_eq!(result.storage_version, status.storage_version);
    assert_eq!(
        runtime.storage_snapshot().expect("unchanged authority"),
        before
    );
    assert_eq!(
        maintenance.project_catalog().await.expect("catalog after"),
        catalog_before
    );
    assert!(
        runtime
            .host
            .data_plane
            .search_lexical_page("鲸鱼", 10, 0)
            .await
            .expect("orphan removed")
            .is_empty()
    );
    assert!(
        runtime
            .host
            .data_plane
            .search_lexical_page("刺猬", 10, 0)
            .await
            .expect("stale tokens removed")
            .is_empty()
    );
    assert!(
        runtime
            .host
            .data_plane
            .search_lexical_page("西瓜", 1024, 0)
            .await
            .expect("missing summary restored")
            .iter()
            .any(|hit| hit.memory_id == missing_id)
    );
    assert!(!maintenance.host_ai_status().checked);
}
