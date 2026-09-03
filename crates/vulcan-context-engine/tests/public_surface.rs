//! Black-box coverage for the host-independent VCE public surface.
//! VCE 宿主无关公开接口的黑盒覆盖。

use serde_json::json;
use vulcan_context_engine::{
    CODEBASE_RETRIEVAL_TOOL_ID, EmbeddingSpace, RetrieveRequest, ScopeResolution, ToolMappingError,
    VceConfig, VceEngine, VceIndexBinding, VceToolCall, VceToolCatalog,
};

/// Build the vector compatibility contract used by black-box tests.
/// 构建黑盒测试使用的向量兼容契约。
fn test_embedding_space() -> EmbeddingSpace {
    EmbeddingSpace {
        vector_space_id: String::from("code-semantic:test"),
        physical_fingerprint: Some(String::from("provider/model@revision")),
        dimension: 2,
        preprocessing_version: String::from("vce-preprocess-v1"),
        chunker_version: String::from("vce-recursive-v1"),
    }
}

/// Prove that a consumer can plan, observe, snapshot, and restore without runtime services.
/// 证明调用方无需运行时服务即可规划、观察、快照和恢复。
#[test]
fn pure_functional_entry_is_available_to_workspace_consumers() {
    // The configuration binds state to one code-semantic checkpoint chain.
    // 配置将状态绑定到唯一的代码语义检查点链。
    let config = VceConfig::new("workspace:test", test_embedding_space())
        .expect("test configuration should be valid");
    // The engine is created without an LLM, vector database, filesystem watcher, or service.
    // 引擎创建过程不需要 LLM、向量数据库、文件监听器或服务。
    let mut engine = VceEngine::new(config.clone()).expect("engine should initialize");

    // An explicit editor observation remains local until a host requests refresh publication.
    // 显式编辑器观察在宿主请求刷新发布前始终保留在本地。
    let snapshot = engine
        .observe("src/lib.rs", "pub fn semantic_entry() {}\n")
        .expect("observation should be accepted");
    assert_eq!(snapshot.records.len(), 1);

    // The pure synchronization plan exposes the pending Blob without touching storage.
    // 纯同步计划无需访问存储即可暴露待处理 Blob。
    let sync_plan = engine.sync_plan();
    assert_eq!(sync_plan.added_blobs.len(), 1);
    assert_eq!(engine.current_scope(), ScopeResolution::MissingCheckpoint);

    // Query planning remains deterministic and provider independent.
    // 查询规划保持确定性且不依赖供应商。
    let query_plan = engine
        .plan_query("find `semantic_entry` implementation")
        .expect("query should be planned");
    assert_eq!(
        query_plan.original_query,
        "find `semantic_entry` implementation"
    );
    assert!(
        query_plan
            .identifiers
            .iter()
            .any(|name| name == "semantic_entry")
    );

    // Persisted state can be restored through the public, namespace-validated constructor.
    // 持久化状态可通过公开且校验命名空间的构造函数恢复。
    let state = engine.state_snapshot();
    let restored = VceEngine::from_state_snapshot(config, state)
        .expect("state snapshot should restore under the same namespace");
    assert_eq!(restored.workspace_snapshot(), engine.workspace_snapshot());
}

/// Prove that the catalog exposes one AI tool while retaining host-only lifecycle mappings.
/// 证明目录只暴露一个 AI 工具，同时保留宿主专用生命周期映射。
#[test]
fn tool_catalog_keeps_model_and_host_surfaces_separate() {
    // The static catalog does not register any runtime tool by itself.
    // 静态目录本身不会注册任何运行时工具。
    let catalog = VceToolCatalog::new();
    // Only the consolidated semantic retrieval entry is model-visible.
    // 只有收敛后的语义检索入口对模型可见。
    let ai_visible = catalog.ai_visible_definitions();
    assert_eq!(ai_visible.len(), 1);
    assert_eq!(ai_visible[0].descriptor.id, CODEBASE_RETRIEVAL_TOOL_ID);

    // The public payload contains intent only; workspace routing remains host-owned.
    // 公开载荷只包含检索意图，工作区路由仍由宿主持有。
    let parsed = catalog
        .parse_ai_call(
            CODEBASE_RETRIEVAL_TOOL_ID,
            json!({"information_request": "trace request routing"}),
        )
        .expect("public retrieval payload should parse");
    assert!(matches!(parsed, VceToolCall::CodebaseRetrieval(_)));

    // OCE lifecycle compatibility names remain unavailable to model-originated calls.
    // OCE 生命周期兼容名称对模型来源调用保持不可用。
    let host_only_error = catalog
        .parse_ai_call("sync_paths", json!({}))
        .expect_err("host-only operations must not be model-visible");
    assert!(matches!(
        host_only_error,
        ToolMappingError::ToolNotAiVisible { .. }
    ));

    // A model cannot select or widen the workspace identity.
    // 模型不能选择或扩大工作区身份。
    let routing_error = catalog
        .parse_ai_call(
            CODEBASE_RETRIEVAL_TOOL_ID,
            json!({
                "information_request": "trace request routing",
                "workspace_folder": "D:/other"
            }),
        )
        .expect_err("workspace routing must remain host-managed");
    assert!(matches!(
        routing_error,
        ToolMappingError::HostManagedArgument { .. }
    ));
}

/// Prove that the convenience retrieval request does not spend an LLM rewrite call by default.
/// 证明便捷检索请求默认不会消耗一次 LLM 查询改写调用。
#[test]
fn retrieval_request_defaults_keep_query_rewrite_opt_in() {
    let request = RetrieveRequest::current_checkpoint("request-1", "find configuration logic");
    assert!(!request.allow_query_rewrite);
    assert!(request.allow_rerank);
}

/// Prove every public namespace constructor rejects a present but blank physical fingerprint.
/// 证明所有公开命名空间构造入口都会拒绝已提供但为空的物理指纹。
#[test]
fn namespace_constructors_reject_blank_physical_fingerprint() {
    let mut space = test_embedding_space();
    space.physical_fingerprint = Some("   ".to_owned());

    assert!(VceConfig::new("workspace:test", space.clone()).is_err());
    assert!(VceIndexBinding::new("vce:workspace:test", "workspace:test", 1, space).is_err());
}
