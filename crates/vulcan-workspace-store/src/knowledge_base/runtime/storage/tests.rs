//! Storage identity and atomic publication regressions.
//! 存储身份与原子发布回归。

use super::*;
use crate::data_plane::{HostRuntimeHandle, HostRuntimeMetadata};
use tempfile::tempdir;

/// Verifies library IDs reject non-canonical and path-like input.
/// 验证知识库标识符拒绝非规范格式与路径式输入。
#[test]
fn library_id_requires_canonical_uuid() {
    assert!(validate_library_id("../../escape").is_err());
    assert!(validate_library_id("550E8400-E29B-41D4-A716-446655440000").is_err());
    assert!(validate_library_id("550e8400-e29b-41d4-a716-446655440000").is_ok());
}

/// Verifies atomic replacement overwrites an existing manifest on every supported platform.
/// 验证原子替换会在所有受支持平台覆盖既有清单。
#[test]
fn atomic_file_replacement_overwrites_existing_destination() {
    let temporary = tempdir().expect("create atomic replacement root");
    let destination = temporary.path().join("manifest.json");
    fs::write(&destination, "old manifest").expect("write existing destination");
    let mut source =
        tempfile::NamedTempFile::new_in(temporary.path()).expect("create replacement source");
    source
        .write_all(b"new manifest")
        .expect("write replacement source");

    persist_tempfile_atomically(source, &destination).expect("replace existing destination");

    assert_eq!(
        fs::read_to_string(&destination).expect("read replaced destination"),
        "new manifest"
    );
}

/// Opens an in-process knowledge base runtime over one exact data root and initializes the v1 schema.
/// 在一个精确数据根上打开进程内知识库运行时并初始化 v1 结构。
///
/// Taking the root as a parameter is what makes a genuine close-and-reopen testable: the same directory can
/// be opened twice, with the first runtime fully dropped in between, so the second open reads its state back
/// from disk rather than from an in-memory cache.
/// 把根目录作为参数正是让真正的关闭后重开可测的原因：同一目录可以被打开两次，中间完全丢弃第一个运行时，因此第二次
/// 打开是从磁盘而非内存缓存读回状态。
async fn open_over(root: &std::path::Path) -> KnowledgeBaseRuntime {
    let app_path = root.join("app.sqlite3");
    let runtime = KnowledgeBaseRuntime::new(HostRuntimeHandle::new_for_test(HostRuntimeMetadata {
        data_root: root.to_path_buf(),
        sqlite_path: app_path.clone(),
        sqlite_binding_id: String::from("binding-probe-app"),
        system_statistics_sqlite_path: app_path,
        system_statistics_sqlite_binding_id: String::from("binding-probe-statistics"),
    }));
    // The application schema is created in-process, exactly as the knowledge base now does it, so this
    // helper never asks an external process for anything. Running it against an already-initialized database is
    // also the v1 initialization path's own idempotency requirement, so reopening exercises both.
    // 应用库结构以进程内方式创建，与知识库现在的做法完全一致，因此本辅助器绝不向外部进程索取任何东西。对已初始化的
    // 数据库再次运行它，同时也是 v1 初始化路径自身的幂等要求，因此重开同时检验了这两点。
    crate::local_sql::execute_script_async(
        &runtime.host_runtime.local,
        LogicalDatabase::AppState,
        &crate::data_plane::build_application_schema_script(),
        Vec::new(),
    )
    .await
    .expect("initialize the application schema in-process");
    runtime
}

/// Builds an in-process knowledge base runtime over a fresh temporary data root.
/// 在全新的临时数据根上构建进程内知识库运行时。
async fn in_process_runtime(
    library_name: &str,
) -> (tempfile::TempDir, KnowledgeBaseRuntime, String) {
    let temporary = tempdir().expect("create in-process knowledge base root");
    let runtime = open_over(temporary.path()).await;
    let library = runtime
        .create_library(library_name, "", &[])
        .await
        .expect("create in-process library");
    (temporary, runtime, library.id)
}

/// One resolved vector generation table round-trips through create, write and search in process.
/// 一张已解析的向量代次表在进程内完成创建、写入与检索的往返。
///
/// This is the LanceDB half of the per-library migration. The knowledge base's vector path needs an
/// embedding model to run through its public entry points, so without this test the conversion from the
/// former LanceDB adapter methods to `LanceDbEngine` would be compile-verified only.
/// 这是按资料库迁移的 LanceDB 那一半。知识库的向量路径需要嵌入模型才能跑通其公开入口，因此没有本测试时，
/// 从原 LanceDB 适配器方法到 `LanceDbEngine` 的转换将仅有编译级验证。
#[tokio::test]
async fn a_resolved_vector_engine_round_trips_one_generation_table() {
    let (temporary, runtime, library_id) = in_process_runtime("向量往返探针").await;
    let generation = Uuid::new_v4().to_string();
    let table_name = runtime
        .prepare_vector_generation(&library_id, &generation, 4)
        .await
        .expect("create generation table in-process");
    let rows = vec![serde_json::json!({
        "id": String::from("chunk-1"),
        "display_chunk_id": String::from("display-1"),
        "generation": generation,
        "vector": [0.1_f32, 0.2, 0.3, 0.4],
    })];
    let written = runtime
        .write_vector_rows(&library_id, &table_name, &generation, &rows, 4)
        .await
        .expect("write one vector row in-process");
    assert_eq!(written, 1);

    let hits = runtime
        .search_vector_rows(
            &library_id,
            &table_name,
            &generation,
            vec![0.1_f32, 0.2, 0.3, 0.4],
            10,
        )
        .await
        .expect("search the generation table in-process");
    assert_eq!(hits.len(), 1);
    assert_eq!(
        hits[0].get("id").and_then(serde_json::Value::as_str),
        Some("chunk-1")
    );
    drop(temporary);
}

/// Builds one credential-free route identity for the vector publication guards.
/// 为向量发布护栏构建一条不含凭据的路由身份。
fn guard_route(entry: &str) -> crate::knowledge_base::models::KnowledgeRouteReference {
    use crate::knowledge_base::models::KnowledgeRouteReference;
    KnowledgeRouteReference {
        consumer_profile_id: String::from("knowledge_base"),
        provider_id: String::from("provider"),
        protocol: String::from("protocol"),
        service_entry_id: String::from(entry),
        model_id: String::from("model"),
        usage_scope: String::from("internal_background"),
        ..KnowledgeRouteReference::default()
    }
}

/// Builds display-hybrid settings that satisfy every embedding rule `validate()` enforces.
/// 构建满足 `validate()` 全部向量规则的显示库混合模式设置。
///
/// `display_index` is deliberately absent: `set_library_settings` discards the caller's display state and
/// keeps the persisted one, because the display lifecycle is owned by the display-generation flow rather
/// than by a settings write. The active generation is therefore established through
/// `mark_display_generation_ready`, which is the same call production uses.
/// `display_index` 被刻意省略：`set_library_settings` 会丢弃调用方的显示状态并保留持久化状态，因为显示库生命周期
/// 由显示库代次流程拥有，而不是由设置写入拥有。因此活动代次通过 `mark_display_generation_ready` 建立——生产使用
/// 的正是同一个调用。
fn hybrid_settings(
    fingerprint: Option<&str>,
) -> crate::knowledge_base::models::KnowledgeLibrarySettings {
    use crate::knowledge_base::models::{
        KnowledgeCurationSettings, KnowledgeEmbeddingSettings, KnowledgeLibrarySettings,
        KnowledgeRetrievalMode,
    };
    KnowledgeLibrarySettings {
        retrieval_mode: KnowledgeRetrievalMode::DisplayHybrid,
        curation: KnowledgeCurationSettings {
            enabled: true,
            route: Some(guard_route("curation")),
            prompt_version: String::from("knowledge-display-v1"),
            ..KnowledgeCurationSettings::default()
        },
        embedding: KnowledgeEmbeddingSettings {
            enabled: true,
            route: Some(guard_route("embedding")),
            configured_dimension: Some(4),
            desired_model_fingerprint: fingerprint.map(String::from),
            ..KnowledgeEmbeddingSettings::default()
        },
        ..KnowledgeLibrarySettings::default()
    }
}

/// Drives every guard on the vector-generation publication path, including the success that follows them.
/// 驱动向量代次发布路径上的每一条护栏，包括其后的成功路径。
///
/// `mark_vector_generation_ready` is reached in production only from the vector-index job in
/// `vulcan-service-kernel`, which needs a live embedding model to run. Its guards, however, are pure
/// decisions over already-persisted state, so they can and should be pinned without a model: a guard that
/// silently widened would otherwise let a mismatched generation be published as `Ready`.
/// `mark_vector_generation_ready` 在生产中只由 `vulcan-service-kernel` 的向量索引作业到达，而那需要活的嵌入模型。
/// 但它的护栏是对已持久化状态的纯判断，因此可以在没有模型的情况下、且应当被钉定：否则一条被悄然放宽的护栏会让
/// 不匹配的代次被发布为 `Ready`。
#[tokio::test]
async fn mark_vector_generation_ready_enforces_every_publication_guard() {
    use crate::knowledge_base::models::{
        KnowledgeDisplayIndexStatus, KnowledgeModelSnapshot, KnowledgeVectorIndexStatus,
    };
    let (temporary, runtime, library_id) = in_process_runtime("向量发布护栏").await;
    let generation = Uuid::new_v4().to_string();
    let snapshot = KnowledgeModelSnapshot {
        route: guard_route("embedding"),
        dimensions: Some(4),
        capability_revision: None,
        captured_at_ms: 1,
    };

    // Guard 1: a non-canonical generation identifier is refused before any state is read.
    // 护栏 1：非规范代次标识符在读取任何状态之前即被拒绝。
    let error = runtime
        .mark_vector_generation_ready(&library_id, "not-a-uuid", snapshot.clone(), 4)
        .await
        .expect_err("a non-UUID generation must be rejected");
    assert!(error.contains("UUID"), "unexpected error: {error}");

    // Guard 2: a zero dimension can never describe a vector space.
    // 护栏 2：维度为 0 永远无法描述一个向量空间。
    let error = runtime
        .mark_vector_generation_ready(&library_id, &generation, snapshot.clone(), 0)
        .await
        .expect_err("a zero dimension must be rejected");
    assert_eq!(error, "向量索引维度必须大于 0");

    // Guard 3: a library that never enabled vectors cannot publish a vector index.
    // 护栏 3：从未启用向量的资料库不能发布向量索引。
    let error = runtime
        .mark_vector_generation_ready(&library_id, &generation, snapshot.clone(), 4)
        .await
        .expect_err("a library without vectors must be rejected");
    assert_eq!(error, "当前资料库未启用向量模型");

    // Guard 4a: enabling vectors is not sufficient — a library with no active display generation yet must
    // be refused.
    // 护栏 4a：仅启用向量并不足够——尚无活动显示库代次的资料库必须被拒绝。
    runtime
        .set_library_settings(&library_id, &hybrid_settings(None))
        .await
        .expect("enable display-hybrid retrieval");
    let error = runtime
        .mark_vector_generation_ready(&library_id, &generation, snapshot.clone(), 4)
        .await
        .expect_err("a library without an active display generation must be rejected");
    assert_eq!(error, "显示库活动代次已变化，拒绝发布不匹配的向量索引");

    // Guard 4b: the same guard must reject a *different* active generation. Publishing a vector index
    // against a superseded display generation is the mismatch this guard exists to stop, so the superseded
    // generation is activated through the real display flow rather than written into settings.
    // 护栏 4b：同一条护栏也必须拒绝一个*不同的*活动代次。针对已被取代的显示库代次发布向量索引，正是这条护栏存在
    // 所要阻止的不匹配；因此被取代的代次是通过真实的显示库流程激活的，而不是写进设置的。
    let superseded = Uuid::new_v4().to_string();
    runtime
        .mark_display_generation_ready(&library_id, &superseded, 7, snapshot.clone(), 1)
        .await
        .expect("activate a superseded display generation");
    // Assert the precondition rather than assume it: the guard below reports the same message for an
    // *absent* generation, so without this the test would pass even if the display flow had failed to
    // activate anything, and the "different generation" claim would be untested.
    // 断言该前置条件而不是假定它：下方护栏对*缺失*代次会报出同样的消息，因此若没有这一步，即使显示库流程未能激活
    // 任何东西，本测试仍会通过，而"不同代次"这一主张就从未被测试过。
    let activated = runtime
        .get_library(&library_id)
        .await
        .expect("reload the superseded activation");
    assert_eq!(
        activated
            .settings
            .display_index
            .active_generation
            .as_deref(),
        Some(superseded.as_str()),
        "the superseded generation must really be the active one before the guard is exercised"
    );
    assert_eq!(
        activated.settings.display_index.status,
        KnowledgeDisplayIndexStatus::Ready
    );
    let error = runtime
        .mark_vector_generation_ready(&library_id, &generation, snapshot.clone(), 4)
        .await
        .expect_err("a superseded display generation must be rejected");
    assert_eq!(error, "显示库活动代次已变化，拒绝发布不匹配的向量索引");

    // Guard 5: with the generation now matching, a missing model fingerprint still blocks publication —
    // the index would otherwise be published without the immutable identity that makes it interpretable.
    // 护栏 5：代次匹配后，缺失的模型指纹仍然阻止发布——否则索引会在缺失使其可解释的不可变身份的情况下被发布。
    runtime
        .mark_display_generation_ready(&library_id, &generation, 7, snapshot.clone(), 1)
        .await
        .expect("activate the matching display generation");
    let matched = runtime
        .get_library(&library_id)
        .await
        .expect("reload the matching activation");
    assert_eq!(
        matched.settings.display_index.active_generation.as_deref(),
        Some(generation.as_str()),
        "the matching generation must really be active, or the next guard would pass for the wrong reason"
    );
    assert_eq!(
        matched.settings.embedding.desired_model_fingerprint, None,
        "the fingerprint must still be unconfigured for guard 5 to be the one under test"
    );
    let error = runtime
        .mark_vector_generation_ready(&library_id, &generation, snapshot.clone(), 4)
        .await
        .expect_err("a missing model fingerprint must be rejected");
    assert_eq!(error, "向量模型指纹尚未配置");

    // Past every guard: publication must persist the full indexed identity, not just the status.
    // 通过全部护栏之后：发布必须持久化完整的已索引身份，而不只是状态。
    runtime
        .set_library_settings(&library_id, &hybrid_settings(Some("fingerprint-4d")))
        .await
        .expect("configure the fingerprint");
    let published = runtime
        .mark_vector_generation_ready(&library_id, &generation, snapshot, 4)
        .await
        .expect("publish the matching vector generation");
    assert_eq!(
        published.settings.vector_index.status,
        KnowledgeVectorIndexStatus::Ready
    );
    assert_eq!(
        published.settings.vector_index.active_generation.as_deref(),
        Some(generation.as_str())
    );
    assert_eq!(
        published
            .settings
            .vector_index
            .indexed_model_fingerprint
            .as_deref(),
        Some("fingerprint-4d")
    );
    assert_eq!(published.settings.vector_index.indexed_dimension, Some(4));
    assert_eq!(published.settings.embedding.indexed_dimension, Some(4));
    assert_eq!(published.settings.embedding.observed_dimension, Some(4));
    // Publication must also clear the failure state, or a previously failed index would keep reporting an
    // error that no longer describes it.
    // 发布也必须清除失败状态，否则先前失败的索引会继续报出一个不再描述它的错误。
    assert_eq!(published.settings.vector_index.last_error, None);

    // The published state must survive a re-read: these assertions are on persisted settings, not on the
    // in-memory value this call happened to return.
    // 已发布状态必须经得起重新读取：以下断言针对持久化设置，而非本次调用恰好返回的内存值。
    let reloaded = runtime
        .get_library(&library_id)
        .await
        .expect("reload the published library");
    assert_eq!(
        reloaded.settings.vector_index.status,
        KnowledgeVectorIndexStatus::Ready
    );
    assert_eq!(
        reloaded.settings.vector_index.active_generation.as_deref(),
        Some(generation.as_str())
    );
    drop(temporary);
}

/// Verifies a settings write cannot reach around the display lifecycle to forge an active generation.
/// 验证设置写入无法绕过显示库生命周期来伪造活动代次。
///
/// This pins the reason the publication guards above can be trusted. If `set_library_settings` accepted a
/// caller-supplied active generation, any caller could make a vector index look publishable for a display
/// generation that was never built: the guard would still be written correctly and would still be
/// ineffective, which is exactly the kind of hole a status-level assertion cannot see.
/// 这将钉定上方发布护栏之所以可信的原因。若 `set_library_settings` 接受调用方提供的活动代次，任何调用方都能让
/// 一个向量索引看起来可以为某个从未构建过的显示库代次发布：护栏本身仍会被正确书写，却仍然无效——而正是这类
/// 漏洞，状态级断言无法看见。
#[tokio::test]
async fn settings_writes_cannot_forge_an_active_display_generation() {
    use crate::knowledge_base::models::{
        KnowledgeDisplayIndexState, KnowledgeDisplayIndexStatus, KnowledgeLibrarySettings,
    };
    let (temporary, runtime, library_id) = in_process_runtime("显示代次伪造护栏").await;
    let forged = Uuid::new_v4().to_string();
    let requested = KnowledgeLibrarySettings {
        display_index: KnowledgeDisplayIndexState {
            status: KnowledgeDisplayIndexStatus::Ready,
            active_generation: Some(forged.clone()),
            ..KnowledgeDisplayIndexState::default()
        },
        ..hybrid_settings(None)
    };
    assert!(
        requested.validate().is_ok(),
        "the forged state must be individually valid, so this test proves the write path rejects it"
    );

    let stored = runtime
        .set_library_settings(&library_id, &requested)
        .await
        .expect("apply display-hybrid settings");
    assert_ne!(
        stored.settings.display_index.active_generation.as_deref(),
        Some(forged.as_str()),
        "a settings write must not be able to install an active display generation"
    );
    assert_ne!(
        stored.settings.display_index.status,
        KnowledgeDisplayIndexStatus::Ready,
        "a settings write must not be able to mark the display index ready"
    );
    drop(temporary);
}

/// Verifies a fresh empty directory round-trips through a real close and reopen, for both engines.
/// 验证全新空目录在真正的关闭与重开后完成往返，且 SQLite 与 LanceDB 两个引擎都被覆盖。
///
/// This is the plan's acceptance item for a fresh directory, and it is the one claim no other test can make:
/// every other test keeps its runtime alive, so it proves nothing about state that has to survive a process
/// boundary. Here the first runtime is dropped before the second is created, so the reopened runtime can only
/// have learned the library, the document row and the vector row from disk.
/// 这是方案对全新目录的验收项，也是其他测试都无法主张的一点：其他测试都让运行时保持存活，因此无法证明任何需要
/// 跨越进程边界存活的状态。此处第二个运行时创建前第一个已被丢弃，所以重开后的运行时只可能从磁盘得知资料库、
/// 文档行与向量行。
///
/// The second half is the negative requirement: nothing may scan for, judge or migrate pre-plan storage. The
/// directory starts empty, so a passing run is evidence that no such step is needed for a fresh install.
/// 后半部分是否定要求：不得扫描、判断或迁移本方案之前的存储。目录起初为空，因此通过即证明全新安装不需要任何此类步骤。
#[tokio::test]
async fn a_fresh_directory_round_trips_through_a_real_close_and_reopen() {
    use crate::knowledge_base::models::{KnowledgeModelSnapshot, KnowledgeVectorIndexStatus};

    let temporary = tempdir().expect("create fresh data root");
    let root = temporary.path().to_path_buf();
    // A fresh root starts with no database files at all, so nothing below can be reading a pre-existing one.
    // 全新根目录起初没有任何数据库文件，因此下方不可能读到既有文件。
    assert_eq!(
        std::fs::read_dir(&root)
            .expect("list the fresh root")
            .count(),
        0,
        "the fixture root must start empty, or this test would not be about a fresh directory"
    );

    let generation = Uuid::new_v4().to_string();
    let library_id;
    let table_name;
    let source_root = root.join("source");
    std::fs::create_dir_all(&source_root).expect("create import source root");
    std::fs::write(
        source_root.join("plan.md"),
        "# 引擎内化\n\nv1 数据库由宿主进程内建，关闭后重新打开仍可检索。\n",
    )
    .expect("write import source");

    // First lifetime: create the v1 objects, persist a library, a document and one vector row.
    // 第一个生命周期：创建 v1 对象，持久化一个资料库、一篇文档与一行向量。
    {
        let runtime = open_over(&root).await;
        let library = runtime
            .create_library("关闭重开知识库", "", &[])
            .await
            .expect("create the library in the first lifetime");
        library_id = library.id.clone();
        let imported = runtime
            .import_markdown_directory(&library_id, &source_root)
            .await
            .expect("import in the first lifetime");
        assert!(imported.changed_files >= 1);
        let found = runtime
            .search(std::slice::from_ref(&library_id), "引擎内化", 10)
            .await
            .expect("search in the first lifetime");
        assert!(
            !found.hits.is_empty(),
            "the first lifetime must be able to retrieve what it just wrote"
        );

        table_name = runtime
            .prepare_vector_generation(&library_id, &generation, 4)
            .await
            .expect("create a vector generation in the first lifetime");
        let rows = vec![serde_json::json!({
            "id": String::from("reopen-chunk"),
            "display_chunk_id": String::from("reopen-display"),
            "generation": generation,
            "vector": [0.5_f32, 0.25, 0.125, 0.0625],
        })];
        let written = runtime
            .write_vector_rows(&library_id, &table_name, &generation, &rows, 4)
            .await
            .expect("write one vector row in the first lifetime");
        assert_eq!(written, 1);
    }

    // Second lifetime: a new runtime over the same directory, with the first one entirely gone.
    // 第二个生命周期：在同一个目录上建立新运行时，第一个已完全消失。
    let reopened = open_over(&root).await;

    let restored = reopened
        .get_library(&library_id)
        .await
        .expect("the library must survive a close and reopen");
    assert_eq!(restored.id, library_id);

    let found = reopened
        .search(std::slice::from_ref(&library_id), "引擎内化", 10)
        .await
        .expect("search after reopen");
    assert!(
        !found.hits.is_empty(),
        "documents written before the close must remain retrievable after the reopen"
    );

    // Every other public read entry point is exercised here, not argued about. The vector defect above was one
    // place where a method resolved a binding the new process had never registered, and the only way to know
    // there is not a second is to call the rest of them after a real reopen. Each failure below would name the
    // entry point that is missing its own activation.
    // 此处逐个执行其余全部公开读入口，而不是对它们进行论证。上面的向量缺陷是一处"方法解析了本进程从未登记的绑定"，
    // 而要知道是否还有第二处，唯一办法就是在真正的重开之后把其余的入口都调用一遍。下方任何失败都会指名那个缺少自身
    // 激活步骤的入口。
    let sources = reopened
        .list_sources(&library_id)
        .await
        .expect("list_sources after reopen");
    assert!(
        !sources.is_empty(),
        "the import source must survive the reopen"
    );
    let documents = reopened
        .list_documents(&library_id, None, 50, 0)
        .await
        .expect("list_documents after reopen");
    assert!(
        !documents.is_empty(),
        "the imported document must be listed after the reopen"
    );
    let document_id = documents[0].id.clone();
    reopened
        .read_document(&library_id, &document_id)
        .await
        .expect("read_document after reopen");
    let chunks = reopened.list_display_chunks(&library_id, 50, 0).await;
    // This library never built a display corpus, so the derived display database legitimately does not exist and
    // the call is expected to report that. What it must *not* report is a binding that this process never
    // registered: that would mean the display side has the same defect the vector side had. The assertion is
    // therefore about the kind of failure, because that is the property this sweep exists to check.
    // 该资料库从未构建显示库语料，因此派生显示库确实不存在，调用报出这一点是预期的。它**不得**报出的是"本进程从未
    // 登记的绑定"：那将意味着显示库侧存在与向量侧相同的缺陷。因此本断言针对失败的种类，因为那正是本次扫描要检验的性质。
    match chunks {
        Ok(_) => {}
        Err(error) => {
            assert!(
                error.contains("读取知识库派生数据库失败"),
                "an absent derived database is the only acceptable display failure after reopen: {error}"
            );
            assert!(
                !error.contains("named binding") && !error.contains("解析知识库"),
                "the display binding must be re-established by the read itself: {error}"
            );
        }
    }
    reopened
        .list_custom_words(&library_id)
        .await
        .expect("list_custom_words after reopen");
    reopened
        .inspect_index_integrity(&library_id)
        .await
        .expect("inspect_index_integrity after reopen");
    reopened
        .list_libraries(None)
        .await
        .expect("list_libraries after reopen");
    reopened
        .recover_interrupted_jobs()
        .await
        .expect("recover_interrupted_jobs after reopen");

    // LanceDB is directory-backed, so this read proves the vector data outlived the runtime that wrote it.
    // LanceDB 以目录为后端，因此本次读取证明向量数据比写入它的运行时活得更久。
    let hits = reopened
        .search_vector_rows(
            &library_id,
            &table_name,
            &generation,
            vec![0.5_f32, 0.25, 0.125, 0.0625],
            10,
        )
        .await
        .expect("search the vector generation after reopen");
    assert_eq!(hits.len(), 1);
    assert_eq!(
        hits[0].get("id").and_then(serde_json::Value::as_str),
        Some("reopen-chunk"),
        "the vector row must be the one written before the close"
    );

    // The reopened library must still be reachable far enough to hit publication's own business guards. The
    // point of this assertion is the *kind* of failure: a library that never enabled vectors must be refused by
    // guard 3, not by a storage or named-binding error. If the reopen had left the LanceDB binding unregistered,
    // this call would surface that instead — which is exactly the defect this test was written to catch.
    // 重开后的资料库必须仍能被走到发布自身的业务护栏。本断言的关键是失败的*种类*：从未启用向量的资料库必须被护栏 3
    // 拒绝，而不是被存储或具名绑定错误拒绝。若重开遗漏了 LanceDB 绑定的登记，本次调用就会暴露那一点——而这正是
    // 本测试要捕捉的缺陷。
    let published = reopened
        .mark_vector_generation_ready(
            &library_id,
            &generation,
            KnowledgeModelSnapshot {
                route: guard_route("embedding"),
                dimensions: Some(4),
                capability_revision: None,
                captured_at_ms: 1,
            },
            4,
        )
        .await;
    match published {
        Err(error) => {
            assert_eq!(
                error, "当前资料库未启用向量模型",
                "a reopened library must be refused by a business guard, not by storage"
            );
            assert!(
                !error.contains("named binding"),
                "the reopen must leave the LanceDB binding resolvable: {error}"
            );
        }
        Ok(summary) => assert_eq!(
            summary.settings.vector_index.status,
            KnowledgeVectorIndexStatus::Ready
        ),
    }

    drop(temporary);
}
