//! Fake inference over real SQLite and LanceDB lifecycle behavior.
//! 在真实 SQLite 与 LanceDB 生命周期行为上使用假推理。

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// Finite local worker deadline; no paid inference is used.
/// 有限本地工作器截止时间；不使用付费推理。
const WORKER_TIMEOUT: Duration = Duration::from_secs(30);

/// Creates a synthetic route for a declared mode; returns no network endpoint.
/// 为声明模式创建合成路由；不返回网络端点。
fn vector_settings(mode: KnowledgeIndexMode, name: &str) -> KnowledgeLibrarySettings {
    KnowledgeLibrarySettings {
        index_mode: mode,
        embedding: mode.has_vectors().then(|| KnowledgeEmbeddingSettings {
            route: KnowledgeRouteReference {
                consumer_profile_id: "fixture".into(),
                provider_id: "fixture".into(),
                protocol: "fixture".into(),
                binding_kind: KnowledgeRouteBindingKind::Protocol,
                service_entry_id: "fixture".into(),
                model_id: name.into(),
                usage_scope: "fixture".into(),
                connection_id: None,
            },
            model_fingerprint: name.into(),
            dimensions: 2,
            batch_size: KNOWLEDGE_DEFAULT_EMBEDDING_BATCH_SIZE,
        }),
    }
}

/// Returns deterministic finite fake vectors in exact input order.
/// 按精确输入顺序返回确定的有限假向量。
fn fake_vectors(request: &KnowledgeEmbeddingRequest) -> Vec<Vec<f32>> {
    request
        .inputs
        .iter()
        .map(|input| vec![input.bytes().map(u32::from).sum::<u32>() as f32, 1.0])
        .collect()
}

/// Returns a fake executor counting only document requests.
/// 返回仅计数文档请求的假执行器。
fn fake_executor(calls: Arc<AtomicUsize>) -> KnowledgeEmbeddingExecutor {
    Arc::new(move |request| {
        if !request.is_query {
            calls.fetch_add(1, Ordering::SeqCst);
        }
        Box::pin(async move { Ok(fake_vectors(&request)) })
    })
}

/// Waits for this exact library worker to exit and returns authoritative embedding status.
/// 等待精确知识库工作器退出并返回权威嵌入状态。
async fn settled(runtime: &KnowledgeBaseRuntime, library: &str) -> KnowledgeEmbeddingStatus {
    tokio::time::timeout(WORKER_TIMEOUT, async {
        loop {
            if !runtime
                .shared
                .vector_workers
                .lock()
                .await
                .contains_key(library)
            {
                return runtime
                    .embedding_status(library, MAX_KNOWLEDGE_PAGE_SIZE, 0)
                    .await
                    .expect("status");
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("worker must settle before the local deadline")
}

/// Verifies four modes, real searches and removal of both derived capabilities on downgrade.
/// 验证四种模式、真实检索以及降级时移除两种派生能力。
#[tokio::test]
async fn four_modes_and_downgrade_obey_the_declared_capabilities() {
    // Every file and inference request belongs to an isolated local fixture.
    // 全部文件和推理请求属于隔离本地夹具。
    let (_root, runtime, library) = fixture(KnowledgeIndexMode::Browse).await;
    // The counter excludes query embeddings so background behavior can be assessed independently.
    // 计数器排除查询嵌入，使后台行为可以独立评估。
    let calls = Arc::new(AtomicUsize::new(0));
    runtime
        .set_embedding_executor(fake_executor(calls.clone()))
        .await
        .expect("fake");
    // A browse-mode write must not issue inference.
    // 浏览模式写入不得发起推理。
    let document = runtime
        .write_content(
            &library,
            "project",
            KnowledgeWriteRequest {
                embed: Some(KnowledgeEmbedPolicy::Full),
                ..create("search.md", "specialneedle searchable content")
            },
            ai(),
        )
        .await
        .expect("create full-vector document")
        .documents
        .into_iter()
        .find(|document| document.path == "search.md")
        .expect("created document path may have changed");
    settled(&runtime, &library).await;
    assert!(
        runtime
            .search(
                std::slice::from_ref(&library),
                &["specialneedle".into()],
                20
            )
            .await
            .expect("browse search")
            .hits
            .is_empty()
    );
    for mode in [
        KnowledgeIndexMode::Bm25,
        KnowledgeIndexMode::Vector,
        KnowledgeIndexMode::Hybrid,
    ] {
        // Each transition has an independently queryable durable identity.
        // 每次过渡都有可独立查询的持久身份。
        let operation = runtime
            .set_index_mode(&library, vector_settings(mode, "space-a"))
            .await
            .expect("transition");
        // Completion requires every currently referenced hash, not historical orphan rows.
        // 完成要求全部当前引用哈希，而非历史孤立行。
        let state = settled(&runtime, &library).await;
        assert_eq!((state.pending, state.failed), (0, 0));
        assert_eq!(
            runtime
                .get_operation(&library, Some(&operation.id))
                .await
                .expect("job")
                .expect("stored job")
                .state,
            "completed"
        );
        // Retrieval must report the actual mode and preserve visible capability errors.
        // 检索必须报告实际模式并保留可见能力错误。
        let found = runtime
            .search(
                std::slice::from_ref(&library),
                &["specialneedle".into()],
                20,
            )
            .await
            .expect("search");
        assert!(found.failures.is_empty(), "{:?}", found.failures);
        assert!(found.hits.iter().any(|hit| hit.document_id == document.id));
        assert!(
            found
                .libraries
                .iter()
                .any(|status| status.index_mode == mode)
        );
        if mode == KnowledgeIndexMode::Bm25 {
            assert_eq!(calls.load(Ordering::SeqCst), 0);
        }
        if mode == KnowledgeIndexMode::Vector {
            assert!(found.hits.iter().all(|hit| hit.match_kinds == ["vector"]));
        }
        if mode == KnowledgeIndexMode::Hybrid {
            assert!(
                found
                    .hits
                    .iter()
                    .any(|hit| hit.match_kinds.contains(&"bm25".into())
                        && hit.match_kinds.contains(&"vector".into()))
            );
        }
    }
    runtime
        .set_index_mode(
            &library,
            vector_settings(KnowledgeIndexMode::Browse, "unused"),
        )
        .await
        .expect("downgrade");
    // Downgrading removes derived state and preserves authoritative content.
    // 降级移除派生状态并保留权威内容。
    let state = settled(&runtime, &library).await;
    assert!(state.active_space_id.is_none());
    assert_eq!((state.pending, state.ready, state.failed), (0, 0, 0));
    assert!(
        !runtime
            .index_integrity(&library)
            .await
            .expect("integrity")
            .fts_present
    );
    assert!(
        runtime
            .read_document(&library, Some(&document.id), None, None, None)
            .await
            .is_ok()
    );
}

/// Verifies automatic attempt exhaustion and explicit selected-document retry.
/// 验证自动尝试耗尽与显式所选文档重试。
#[tokio::test]
async fn exhausted_failures_stop_and_human_retry_recovers() {
    let (_root, runtime, library) = fixture(KnowledgeIndexMode::Browse).await;
    // Every automatic failing batch increments one observable fake request.
    // 每个自动失败批次增加一次可观测假请求。
    let failures = Arc::new(AtomicUsize::new(0));
    runtime
        .set_embedding_executor({
            let failures = failures.clone();
            Arc::new(move |_| {
                failures.fetch_add(1, Ordering::SeqCst);
                Box::pin(async { Err("fixture route unavailable".into()) })
            })
        })
        .await
        .expect("failing fake");
    let document = add(&runtime, &library, "failed.md", "content needing vectors").await;
    runtime
        .set_index_mode(
            &library,
            vector_settings(KnowledgeIndexMode::Vector, "failure-space"),
        )
        .await
        .expect("mode");
    let failed = settled(&runtime, &library).await;
    assert_eq!(
        failures.load(Ordering::SeqCst),
        MAX_KNOWLEDGE_EMBEDDING_ATTEMPTS as usize
    );
    assert!(failed.failed > 0);
    assert!(
        failed
            .failures
            .iter()
            .all(|item| item.attempts == MAX_KNOWLEDGE_EMBEDDING_ATTEMPTS
                && item.document_id == document.id)
    );
    runtime.wake_knowledge_worker(&library).await;
    settled(&runtime, &library).await;
    assert_eq!(
        failures.load(Ordering::SeqCst),
        MAX_KNOWLEDGE_EMBEDDING_ATTEMPTS as usize
    );
    // Replacing the executor alone never resets exhausted item state.
    // 仅替换执行器绝不重置已耗尽条目状态。
    let successes = Arc::new(AtomicUsize::new(0));
    runtime
        .set_embedding_executor(fake_executor(successes.clone()))
        .await
        .expect("healthy fake");
    runtime
        .retry_embeddings(&library, std::slice::from_ref(&document.id))
        .await
        .expect("explicit retry");
    let ready = settled(&runtime, &library).await;
    assert_eq!((ready.pending, ready.failed), (0, 0));
    assert!(ready.ready > 0);
    assert!(successes.load(Ordering::SeqCst) > 0);
    // The default policy indexes only summary chunks even when body text exists.
    // 默认策略即使存在正文也只索引摘要切块。
    let result = runtime
        .search(std::slice::from_ref(&library), &["content".into()], 20)
        .await
        .expect("summary-only search");
    assert!(result.hits.iter().all(|hit| hit.kind == "summary"));
}

/// Verifies restore reuses historical ready hashes without another embedding request.
/// 验证还原复用历史就绪哈希，不再发起嵌入请求。
#[tokio::test]
async fn restore_reuses_ready_hash_vectors() {
    let (_root, runtime, library) = fixture(KnowledgeIndexMode::Browse).await;
    let calls = Arc::new(AtomicUsize::new(0));
    runtime
        .set_embedding_executor(fake_executor(calls.clone()))
        .await
        .expect("fake");
    runtime
        .set_index_mode(
            &library,
            vector_settings(KnowledgeIndexMode::Vector, "restore-space"),
        )
        .await
        .expect("mode");
    settled(&runtime, &library).await;
    let document = runtime
        .write_content(
            &library,
            "project",
            KnowledgeWriteRequest {
                embed: Some(KnowledgeEmbedPolicy::Full),
                ..create("history.md", "original body")
            },
            ai(),
        )
        .await
        .expect("create full-vector document")
        .documents
        .into_iter()
        .find(|document| document.path == "history.md")
        .expect("created document path may have changed");
    settled(&runtime, &library).await;
    // Select the creation point by operation, independently of row position.
    // 按操作选择创建点，不依赖行位置。
    let point = runtime
        .list_changes(&library, None, 20)
        .await
        .expect("history")
        .into_iter()
        .find(|change| change.operation == "create")
        .expect("creation")
        .seq;
    runtime
        .write_content(
            &library,
            "project",
            KnowledgeWriteRequest {
                action: "update".into(),
                document_id: Some(document.id),
                expected_revision: Some(document.revision),
                body: Some("different body".into()),
                ..Default::default()
            },
            ai(),
        )
        .await
        .expect("update");
    settled(&runtime, &library).await;
    let before = calls.load(Ordering::SeqCst);
    runtime
        .restore_content(&library, point, human(), "recover original")
        .await
        .expect("restore");
    assert_eq!(settled(&runtime, &library).await.pending, 0);
    assert_eq!(calls.load(Ordering::SeqCst), before);
}

/// Verifies the old route remains searchable while replacement generation is deliberately blocked.
/// 验证更换生成被刻意阻塞时，旧路由仍可检索。
#[tokio::test]
async fn replacement_keeps_old_space_until_every_current_hash_is_ready() {
    let (_root, runtime, library) = fixture(KnowledgeIndexMode::Browse).await;
    runtime
        .set_embedding_executor(fake_executor(Arc::new(AtomicUsize::new(0))))
        .await
        .expect("fake");
    let document = add(&runtime, &library, "active.md", "active content").await;
    runtime
        .set_index_mode(&library, vector_settings(KnowledgeIndexMode::Vector, "old"))
        .await
        .expect("old mode");
    let old = settled(&runtime, &library)
        .await
        .active_space_id
        .expect("old space");
    // These notifications are deterministic inference boundaries rather than timing assumptions.
    // 这些通知是确定推理边界，而非时间假设。
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    runtime
        .set_embedding_executor({
            let started = started.clone();
            let release = release.clone();
            Arc::new(move |request| {
                let started = started.clone();
                let release = release.clone();
                Box::pin(async move {
                    if !request.is_query && request.embedding.model_fingerprint == "new" {
                        started.notify_one();
                        release.notified().await;
                    }
                    Ok(fake_vectors(&request))
                })
            })
        })
        .await
        .expect("gated fake");
    settled(&runtime, &library).await;
    runtime
        .set_index_mode(&library, vector_settings(KnowledgeIndexMode::Vector, "new"))
        .await
        .expect("replace");
    tokio::time::timeout(WORKER_TIMEOUT, started.notified())
        .await
        .expect("started");
    let status = runtime
        .embedding_status(&library, 20, 0)
        .await
        .expect("status");
    assert_eq!(status.active_space_id.as_deref(), Some(old.as_str()));
    assert_ne!(status.active_space_id, status.desired_space_id);
    let result = runtime
        .search(std::slice::from_ref(&library), &["active".into()], 20)
        .await
        .expect("old-space search");
    assert!(result.hits.iter().any(|hit| hit.document_id == document.id));
    assert!(
        result
            .libraries
            .iter()
            .any(|state| state.embeddings.pending > 0)
    );
    release.notify_one();
    let current = settled(&runtime, &library).await;
    assert_eq!(current.active_space_id, current.desired_space_id);
    assert_ne!(current.active_space_id.as_deref(), Some(old.as_str()));
}

/// Verifies cancellation after request admission blocks delayed vector publication.
/// 验证请求准入后取消阻止延迟向量发布。
#[tokio::test]
async fn cancellation_prevents_delayed_vector_publication() {
    let (_root, runtime, library) = fixture(KnowledgeIndexMode::Browse).await;
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    runtime
        .set_embedding_executor({
            let started = started.clone();
            let release = release.clone();
            Arc::new(move |request| {
                let started = started.clone();
                let release = release.clone();
                Box::pin(async move {
                    if !request.is_query {
                        started.notify_one();
                        release.notified().await;
                    }
                    Ok(fake_vectors(&request))
                })
            })
        })
        .await
        .expect("gated fake");
    add(&runtime, &library, "cancel.md", "content").await;
    let operation = runtime
        .set_index_mode(
            &library,
            vector_settings(KnowledgeIndexMode::Vector, "cancel"),
        )
        .await
        .expect("mode");
    tokio::time::timeout(WORKER_TIMEOUT, started.notified())
        .await
        .expect("started");
    assert!(
        runtime
            .cancel_operation(&library, &operation.id)
            .await
            .expect("cancel")
    );
    release.notify_one();
    let state = settled(&runtime, &library).await;
    assert!(state.paused);
    assert_eq!(state.ready, 0);
    assert!(state.pending > 0);
    assert!(state.active_space_id.is_none());
    assert_eq!(
        runtime
            .get_operation(&library, Some(&operation.id))
            .await
            .expect("operation")
            .expect("stored job")
            .state,
        "cancelled"
    );
}

/// Reopens the same durable fixture using a fresh host and empty binding caches.
/// 使用全新宿主与空绑定缓存重新打开同一持久夹具。
async fn reopen(
    root: &tempfile::TempDir,
    runtime: KnowledgeBaseRuntime,
    library: &str,
    calls: Arc<AtomicUsize>,
) -> KnowledgeBaseRuntime {
    // The old worker must have released every inference and publication lease before shutdown.
    // 关闭前旧工作器必须已释放全部推理和发布租约。
    assert!(
        !runtime
            .shared
            .vector_workers
            .lock()
            .await
            .contains_key(library)
    );
    runtime
        .release_library_binding(library)
        .await
        .expect("close old bindings");
    drop(runtime);
    // These paths are the exact fixture contract, not candidate locations.
    // 这些路径是精确夹具契约，而非候选位置。
    let database = root.path().join("sqlite/app.sqlite3");
    // A new host owns new runtime caches while preserving all existing durable bytes.
    // 新宿主拥有新运行时缓存，同时保留全部已有持久字节。
    let runtime = KnowledgeBaseRuntime::new(HostRuntimeHandle::new_for_test(HostRuntimeMetadata {
        data_root: root.path().to_path_buf(),
        sqlite_path: database.clone(),
        sqlite_binding_id: format!("restart-{}", Uuid::new_v4()),
        system_statistics_sqlite_path: database,
        system_statistics_sqlite_binding_id: format!("restart-stat-{}", Uuid::new_v4()),
    }));
    runtime
        .set_embedding_executor(fake_executor(calls))
        .await
        .expect("restore host executor");
    runtime
}

/// Verifies a fresh host resumes durable pending work without recreating or duplicating its vector space.
/// 验证全新宿主恢复持久待处理工作，不重建或重复向量空间。
#[tokio::test]
async fn fresh_host_recovers_pending_vectors_and_running_jobs() {
    let (root, runtime, library) = fixture(KnowledgeIndexMode::Browse).await;
    runtime
        .set_embedding_executor(fake_executor(Arc::new(AtomicUsize::new(0))))
        .await
        .expect("fake");
    add(&runtime, &library, "restart.md", "durable content").await;
    let operation = runtime
        .set_index_mode(
            &library,
            vector_settings(KnowledgeIndexMode::Vector, "restart"),
        )
        .await
        .expect("mode");
    let before = settled(&runtime, &library).await;
    // Simulate a crash after durable reservation and before provider completion.
    // 模拟持久保留后、供应商完成前的崩溃。
    let (_, binding) = runtime
        .ensure_library_binding(&library)
        .await
        .expect("binding");
    local_sql::execute_script_async(
        &runtime.host_runtime.local,
        runtime.resolve_library_binding(binding).expect("resolve"),
        "UPDATE kb_vector_state SET state='pending',attempts=1;",
        vec![],
    )
    .await
    .expect("interrupted items");
    local_sql::execute_script_async(&runtime.host_runtime.local,LogicalDatabase::AppState,
        "UPDATE knowledge_operations SET state='running',phase='running',finished_at_ms=NULL WHERE id=?;",vec![text(&operation.id)]).await.expect("interrupted job");
    // A fresh registration recovers jobs and explicit pending state.
    // 全新注册恢复任务与显式待处理状态。
    let calls = Arc::new(AtomicUsize::new(0));
    let runtime = reopen(&root, runtime, &library, calls.clone()).await;
    let after = settled(&runtime, &library).await;
    assert_eq!(after.active_space_id, before.active_space_id);
    assert_eq!(after.ready, before.ready);
    assert_eq!((after.pending, after.failed), (0, 0));
    assert!(calls.load(Ordering::SeqCst) > 0);
    assert_eq!(
        runtime
            .get_operation(&library, Some(&operation.id))
            .await
            .expect("job")
            .expect("durable operation")
            .state,
        "completed"
    );
    assert!(
        runtime
            .search(std::slice::from_ref(&library), &["durable".into()], 20)
            .await
            .expect("reopened search")
            .failures
            .is_empty()
    );
}

/// Verifies retained historical vectors cannot occupy the only result slot after their document is deleted.
/// 验证对应文档删除后，保留历史向量不能占据唯一结果位置。
#[tokio::test]
async fn historical_nearest_vectors_never_crowd_out_live_documents() {
    let (_root, runtime, library) = fixture(KnowledgeIndexMode::Browse).await;
    runtime
        .set_embedding_executor(fake_executor(Arc::new(AtomicUsize::new(0))))
        .await
        .expect("fake");
    runtime
        .set_index_mode(
            &library,
            vector_settings(KnowledgeIndexMode::Vector, "current-only"),
        )
        .await
        .expect("mode");
    settled(&runtime, &library).await;
    let deleted = runtime
        .write_content(
            &library,
            "project",
            KnowledgeWriteRequest {
                embed: Some(KnowledgeEmbedPolicy::Full),
                ..create("deleted.md", "closest")
            },
            ai(),
        )
        .await
        .expect("first")
        .documents
        .into_iter()
        .find(|d| d.path == "deleted.md")
        .expect("document");
    settled(&runtime, &library).await;
    let live = runtime
        .write_content(
            &library,
            "project",
            KnowledgeWriteRequest {
                embed: Some(KnowledgeEmbedPolicy::Full),
                ..create("live.md", "farther text with a very different vector")
            },
            ai(),
        )
        .await
        .expect("second")
        .documents
        .into_iter()
        .find(|d| d.path == "live.md")
        .expect("document");
    settled(&runtime, &library).await;
    runtime
        .write_content(
            &library,
            "project",
            KnowledgeWriteRequest {
                action: "delete".into(),
                document_id: Some(deleted.id.clone()),
                expected_revision: Some(deleted.revision),
                ..Default::default()
            },
            ai(),
        )
        .await
        .expect("delete");
    settled(&runtime, &library).await;
    let result = runtime
        .search(std::slice::from_ref(&library), &["closest".into()], 1)
        .await
        .expect("current-only query");
    assert_eq!(result.hits.len(), 1);
    assert!(
        result
            .hits
            .iter()
            .all(|hit| hit.document_id == live.id && hit.document_id != deleted.id)
    );
}

/// Verifies space publication waits for an in-flight old-space query and does not deadlock its final mapping.
/// 验证空间发布等待进行中的旧空间查询，且不会使其最终映射死锁。
#[tokio::test]
async fn active_query_lease_survives_publication_and_old_table_cleanup() {
    let (_root, runtime, library) = fixture(KnowledgeIndexMode::Browse).await;
    runtime
        .set_embedding_executor(fake_executor(Arc::new(AtomicUsize::new(0))))
        .await
        .expect("fake");
    add(&runtime, &library, "lease.md", "leased content").await;
    runtime
        .set_index_mode(
            &library,
            vector_settings(KnowledgeIndexMode::Vector, "old-lease"),
        )
        .await
        .expect("mode");
    settled(&runtime, &library).await;
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    runtime
        .set_embedding_executor({
            let started = started.clone();
            let release = release.clone();
            Arc::new(move |request| {
                let started = started.clone();
                let release = release.clone();
                Box::pin(async move {
                    if request.is_query && request.embedding.model_fingerprint == "old-lease" {
                        started.notify_one();
                        release.notified().await;
                    }
                    Ok(fake_vectors(&request))
                })
            })
        })
        .await
        .expect("query gate");
    settled(&runtime, &library).await;
    let query = tokio::spawn({
        let runtime = runtime.clone();
        let library = library.clone();
        async move { runtime.search(&[library], &["leased".into()], 20).await }
    });
    tokio::time::timeout(WORKER_TIMEOUT, started.notified())
        .await
        .expect("query entered old space");
    let operation = runtime
        .set_index_mode(
            &library,
            vector_settings(KnowledgeIndexMode::Vector, "new-lease"),
        )
        .await
        .expect("replace");
    tokio::time::timeout(WORKER_TIMEOUT, async {
        loop {
            let state = runtime
                .embedding_status(&library, 20, 0)
                .await
                .expect("new space progress");
            if state.pending == 0 && state.desired_space_id == state.active_space_id {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("new space reached publication");
    assert_ne!(
        runtime
            .get_operation(&library, Some(&operation.id))
            .await
            .expect("operation")
            .expect("job")
            .state,
        "completed"
    );
    release.notify_one();
    let result = tokio::time::timeout(WORKER_TIMEOUT, query)
        .await
        .expect("old query must finish")
        .expect("query task")
        .expect("old query");
    assert!(result.failures.is_empty(), "{:?}", result.failures);
    assert!(!result.hits.is_empty());
    settled(&runtime, &library).await;
    assert_eq!(
        runtime
            .get_operation(&library, Some(&operation.id))
            .await
            .expect("operation")
            .expect("job")
            .state,
        "completed"
    );
}

/// Verifies empty spaces and explicitly excluded documents never spend query or document inference.
/// 验证空空间与显式排除文档绝不消耗查询或文档推理。
#[tokio::test]
async fn empty_vector_library_and_none_policy_need_no_inference() {
    // The executor counts all requests here, including retrieval queries.
    // 此处执行器计数全部请求，包括检索查询。
    let (_root, runtime, _browse) = fixture(KnowledgeIndexMode::Browse).await;
    // An empty vector library must become ready using only local derived storage.
    // 空向量库必须仅使用本地派生存储成为就绪。
    let calls = Arc::new(AtomicUsize::new(0));
    runtime
        .set_embedding_executor({
            let calls = calls.clone();
            Arc::new(move |request| {
                calls.fetch_add(1, Ordering::SeqCst);
                Box::pin(async move { Ok(fake_vectors(&request)) })
            })
        })
        .await
        .expect("counting fake");
    // The explicit vector route is synthetic and performs no network calls.
    // 显式向量路由为合成路由，不执行网络调用。
    let library = runtime
        .create_library(
            "Empty vector",
            "No paid work",
            &["project".into()],
            &vector_settings(KnowledgeIndexMode::Vector, "empty-space"),
        )
        .await
        .expect("create vector library");
    // The active empty space is still a complete space.
    // 活动空空间仍是完整空间。
    let status = settled(&runtime, &library.id).await;
    assert_eq!(status.active_space_id, status.desired_space_id);
    assert!(status.active_space_id.is_some());
    runtime
        .write_content(
            &library.id,
            "project",
            KnowledgeWriteRequest {
                embed: Some(KnowledgeEmbedPolicy::None),
                ..create("excluded.md", "visible only through read")
            },
            ai(),
        )
        .await
        .expect("excluded document");
    settled(&runtime, &library.id).await;
    // Empty vector membership is checked before query inference is requested.
    // 请求查询推理前检查空向量成员集合。
    let found = runtime
        .search(std::slice::from_ref(&library.id), &["read".into()], 20)
        .await
        .expect("empty-vector search");
    assert!(found.hits.is_empty());
    assert!(found.failures.is_empty());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
