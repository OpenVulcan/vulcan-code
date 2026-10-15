//! Closed wire-shape checks without runtime or provider side effects.
//! 封闭线协议形状检查，不产生运行时或供应商副作用。

use super::*;
use serde_json::json;

/// Verifies removed session/search strategy fields are rejected; takes no inputs and returns no value.
/// 验证已移除的会话及检索策略字段被拒绝；无输入及返回值。
#[test]
fn knowledge_selection_and_search_reject_obsolete_retrieval_fields() {
    // Current session selection contains identities and authorization inheritance only.
    // 当前会话选择仅包含身份及授权继承。
    let mut selection = json!({"projectId":"project","conversationId":"session","mode":"explicit","libraryIds":["library"]});
    assert!(
        serde_json::from_value::<SessionKnowledgeLibrarySelectionRequest>(selection.clone())
            .is_ok()
    );
    selection["retrievalMode"] = json!("raw_bm25");
    assert!(serde_json::from_value::<SessionKnowledgeLibrarySelectionRequest>(selection).is_err());
    // Each selected library supplies its own search mode.
    // 每个选定库提供自身检索模式。
    let mut search = json!({"projectId":"project","libraryIds":["library"],"query":"query"});
    assert!(serde_json::from_value::<KnowledgeSearchRequest>(search.clone()).is_ok());
    search["retrievalMode"] = json!("display_hybrid");
    assert!(serde_json::from_value::<KnowledgeSearchRequest>(search).is_err());
}

/// Verifies package imports can create only a new library from an inspected archive; no I/O or return value.
/// 验证套件导入仅可从已检查归档创建新库；无 I/O 及返回值。
#[test]
fn knowledge_package_import_rejects_replacement_and_requires_fingerprint() {
    // Inspection fingerprint is mandatory to bind the import to the reviewed bytes.
    // 检查指纹为必填，使导入绑定到已审阅字节。
    let mut request = json!({"archivePath":"package.zip","name":"new","expectedSha256":"hash"});
    assert!(serde_json::from_value::<KnowledgePackageImportRequest>(request.clone()).is_ok());
    request["strategy"] = json!("replace");
    assert!(serde_json::from_value::<KnowledgePackageImportRequest>(request.clone()).is_err());
    request.as_object_mut().unwrap().remove("strategy");
    request.as_object_mut().unwrap().remove("expectedSha256");
    assert!(serde_json::from_value::<KnowledgePackageImportRequest>(request).is_err());
}

/// Verifies mode and route declarations are explicit and host authorization is not browser-controlled.
/// 验证模式及路由声明明确，且宿主授权不由浏览器控制。
/// Takes no parameters and returns no value; storage performs semantic admission after decoding.
/// 无参数及返回值；存储在解码后执行语义准入。
#[test]
fn knowledge_creation_requires_mode_and_rejects_portable_consumer_identity() {
    // Even an empty library requires a declared indexing mode.
    // 即使空库也需要声明索引模式。
    let mut create = json!({"name":"name","description":"purpose","settings":{"indexMode":"browse","embedding":null}});
    assert!(serde_json::from_value::<KnowledgeLibraryCreateRequest>(create.clone()).is_ok());
    create.as_object_mut().unwrap().remove("settings");
    assert!(serde_json::from_value::<KnowledgeLibraryCreateRequest>(create).is_err());
    // A route may identify credentials but cannot set the library-owned capability namespace.
    // 路由可标识凭据，但不能设置库拥有的能力命名空间。
    let mut route = json!({"providerId":"provider","protocol":"protocol","bindingKind":"protocol","serviceEntryId":"entry","modelId":"model","connectionId":"connection"});
    assert!(serde_json::from_value::<KnowledgeRouteReferenceRequest>(route.clone()).is_ok());
    route["consumerProfileId"] = json!("foreign-library");
    assert!(serde_json::from_value::<KnowledgeRouteReferenceRequest>(route).is_err());
}
