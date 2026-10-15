//! Wire-format regression coverage for the complete current WebUI knowledge mirror.
//! 完整当前 WebUI 知识库镜像的线格式回归覆盖。

use super::{CallKind, Slot, knowledge_base as kb};
use serde_json::{Value, json};

/// Every shared request preserves field ownership, nullable values and the exact RPC envelope.
/// 每个共享请求保留字段归属、可空值及精确 RPC 封装。
#[test]
fn every_current_request_matches_the_webui_envelope() {
    // Actual builders and explicit protocol fixtures are evaluated without a transport.
    // 实际构建器及明确协议夹具在无传输情况下求值。
    let cases = [
        (kb::create_knowledge_library(serde_json::from_value::<vulcan_service_api::KnowledgeLibraryCreateRequest>(json!({"name":"Independent","description":"Purpose","projectIds":["project"],"settings":{"indexMode":"bm25","embedding":null}})).unwrap()), "service_create_knowledge_library", json!({"request":{"name":"Independent","description":"Purpose","projectIds":["project"],"settings":{"indexMode":"bm25","embedding":null}}})),
        (kb::update_knowledge_library_settings(serde_json::from_value::<vulcan_service_api::KnowledgeLibrarySettingsUpdateRequest>(json!({"libraryId":"library","name":"Renamed","description":"Purpose"})).unwrap()), "service_set_knowledge_library_settings", json!({"request":{"libraryId":"library","name":"Renamed","description":"Purpose"}})),
        (kb::rename_knowledge_library(serde_json::from_value::<vulcan_service_api::KnowledgeLibraryRenameRequest>(json!({"libraryId":"library","name":"Renamed","description":"Purpose"})).unwrap()), "service_rename_knowledge_library", json!({"request":{"libraryId":"library","name":"Renamed","description":"Purpose"}})),
        (kb::set_knowledge_index_mode(serde_json::from_value::<vulcan_service_api::KnowledgeIndexModeRequestPayload>(json!({"libraryId":"library","settings":{"indexMode":"bm25","embedding":null}})).unwrap()), "service_set_knowledge_index_mode", json!({"request":{"libraryId":"library","settings":{"indexMode":"bm25","embedding":null}}})),
        (kb::set_knowledge_project_binding(serde_json::from_value::<vulcan_service_api::KnowledgeProjectBindingRequest>(json!({"libraryId":"library","projectId":"project","authorized":true,"defaultEnabled":false})).unwrap()), "service_set_knowledge_project_binding", json!({"request":{"libraryId":"library","projectId":"project","authorized":true,"defaultEnabled":false}})),
        (kb::get_knowledge_operation(serde_json::from_value::<vulcan_service_api::KnowledgeOperationRequest>(json!({"libraryId":"library","operationId":"operation"})).unwrap()), "service_get_knowledge_operation", json!({"request":{"libraryId":"library","operationId":"operation"}})),
        (kb::cancel_knowledge_operation(serde_json::from_value::<vulcan_service_api::KnowledgeOperationCancelRequest>(json!({"libraryId":"library","operationId":"operation"})).unwrap()), "service_cancel_knowledge_operation", json!({"request":{"libraryId":"library","operationId":"operation"}})),
        (kb::upsert_knowledge_custom_word(serde_json::from_value::<vulcan_service_api::KnowledgeCustomWordRequest>(json!({"libraryId":"library","word":"Term","weight":17})).unwrap()), "service_upsert_knowledge_custom_word", json!({"request":{"libraryId":"library","word":"Term","weight":17}})),
        (kb::list_managed_knowledge_documents(serde_json::from_value::<vulcan_service_api::KnowledgeLibraryDocumentListRequest>(json!({"libraryId":"library","path":"guide","depth":2,"cursor":"opaque","limit":19})).unwrap()), "service_list_knowledge_library_documents", json!({"request":{"libraryId":"library","path":"guide","depth":2,"cursor":"opaque","limit":19}})),
        (kb::list_managed_knowledge_document_folders(serde_json::from_value::<vulcan_service_api::KnowledgeLibraryDocumentListRequest>(json!({"libraryId":"library","path":"guide","depth":2,"cursor":"opaque","limit":19})).unwrap()), "service_list_knowledge_library_folders", json!({"request":{"libraryId":"library","path":"guide","depth":2,"cursor":"opaque","limit":19}})),
        (kb::read_managed_knowledge_document(serde_json::from_value::<vulcan_service_api::KnowledgeDocumentReadRequest>(json!({"libraryId":"library","documentId":"document","path":null,"startLine":13,"endLine":27})).unwrap()), "service_read_knowledge_library_document", json!({"request":{"libraryId":"library","documentId":"document","path":null,"startLine":13,"endLine":27}})),
        (kb::search_managed_knowledge_library(serde_json::from_value::<vulcan_service_api::KnowledgeLibrarySearchRequest>(json!({"libraryId":"library","query":"query","limit":19})).unwrap()), "service_search_knowledge_library", json!({"request":{"libraryId":"library","query":"query","limit":19}})),
        (kb::list_knowledge_changes(serde_json::from_value::<vulcan_service_api::KnowledgeChangeListRequest>(json!({"libraryId":"library","beforeSeq":87,"limit":19})).unwrap()), "service_list_knowledge_changes", json!({"request":{"libraryId":"library","beforeSeq":87,"limit":19}})),
        (kb::get_knowledge_change(serde_json::from_value::<vulcan_service_api::KnowledgeChangeGetRequest>(json!({"libraryId":"library","seq":87,"offset":41,"limit":19})).unwrap()), "service_get_knowledge_change", json!({"request":{"libraryId":"library","seq":87,"offset":41,"limit":19}})),
        (kb::read_knowledge_change_document(serde_json::from_value::<vulcan_service_api::KnowledgeChangeDocumentReadRequest>(json!({"libraryId":"library","seq":87,"documentId":"document","side":"before","startLine":13,"endLine":27})).unwrap()), "service_read_knowledge_change_document", json!({"request":{"libraryId":"library","seq":87,"documentId":"document","side":"before","startLine":13,"endLine":27}})),
        (kb::restore_knowledge_library(serde_json::from_value::<vulcan_service_api::KnowledgeRestoreRequest>(json!({"libraryId":"library","targetSeq":87,"note":"Restore"})).unwrap()), "service_restore_knowledge_library", json!({"request":{"libraryId":"library","targetSeq":87,"note":"Restore"}})),
        (kb::get_knowledge_embedding_status(serde_json::from_value::<vulcan_service_api::KnowledgeEmbeddingStatusRequest>(json!({"libraryId":"library","offset":41,"limit":19})).unwrap()), "service_get_knowledge_embedding_status", json!({"request":{"libraryId":"library","offset":41,"limit":19}})),
        (kb::retry_knowledge_embeddings(serde_json::from_value::<vulcan_service_api::KnowledgeEmbeddingRetryRequest>(json!({"libraryId":"library","documentIds":["document"]})).unwrap()), "service_retry_knowledge_embeddings", json!({"request":{"libraryId":"library","documentIds":["document"]}})),
        (kb::export_knowledge_package(serde_json::from_value::<vulcan_service_api::KnowledgePackageExportRequest>(json!({"libraryId":"library","destinationDirectory":"X:/synthetic"})).unwrap()), "service_export_knowledge_package", json!({"request":{"libraryId":"library","destinationDirectory":"X:/synthetic"}})),
        (kb::inspect_knowledge_package(serde_json::from_value::<vulcan_service_api::KnowledgePackageInspectRequest>(json!({"archivePath":"X:/synthetic/package.vkb"})).unwrap()), "service_inspect_knowledge_package", json!({"request":{"archivePath":"X:/synthetic/package.vkb"}})),
        (kb::import_knowledge_package(serde_json::from_value::<vulcan_service_api::KnowledgePackageImportRequest>(json!({"archivePath":"X:/synthetic/package.vkb","name":"Independent","expectedSha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"})).unwrap()), "service_import_knowledge_package", json!({"request":{"archivePath":"X:/synthetic/package.vkb","name":"Independent","expectedSha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}})),
        (kb::list_knowledge_documents(serde_json::from_value::<vulcan_service_api::KnowledgeDocumentListRequest>(json!({"projectId":"project","libraryId":"library","path":"guide","depth":2,"cursor":"opaque","limit":19})).unwrap()), "service_list_knowledge_documents", json!({"request":{"projectId":"project","libraryId":"library","path":"guide","depth":2,"cursor":"opaque","limit":19}})),
        (kb::read_knowledge_document(serde_json::from_value::<vulcan_service_api::KnowledgeProjectDocumentReadRequest>(json!({"projectId":"project","document":{"libraryId":"library","documentId":null,"path":"guide/readme.md","startLine":13,"endLine":27}})).unwrap()), "service_read_knowledge_document", json!({"request":{"projectId":"project","document":{"libraryId":"library","documentId":null,"path":"guide/readme.md","startLine":13,"endLine":27}}})),
        (kb::search_knowledge(serde_json::from_value::<vulcan_service_api::KnowledgeSearchRequest>(json!({"projectId":"project","libraryIds":["library"],"query":"query","limit":19})).unwrap()), "service_search_knowledge", json!({"request":{"projectId":"project","libraryIds":["library"],"query":"query","limit":19}})),
        (kb::set_session_knowledge_libraries(serde_json::from_value::<vulcan_service_api::SessionKnowledgeLibrarySelectionRequest>(json!({"projectId":"project","conversationId":"session","mode":"explicit","libraryIds":[]})).unwrap()), "service_set_session_knowledge_libraries", json!({"request":{"projectId":"project","conversationId":"session","mode":"explicit","libraryIds":[]}})),
        (kb::list_knowledge_libraries(Some("project")), "service_list_knowledge_libraries", json!({"projectId":"project"})),
        (kb::delete_knowledge_library("library"), "service_delete_knowledge_library", json!({"libraryId":"library"})),
        (kb::list_knowledge_project_bindings("library"), "service_list_knowledge_project_bindings", json!({"libraryId":"library"})),
        (kb::list_project_knowledge_bindings("project"), "service_list_project_knowledge_bindings", json!({"projectId":"project"})),
        (kb::remove_knowledge_custom_word("library", "Term"), "service_remove_knowledge_custom_word", json!({"libraryId":"library","word":"Term"})),
        (kb::rebuild_knowledge_index("library"), "service_rebuild_knowledge_index", json!({"libraryId":"library"})),
        (kb::repair_knowledge_index("library"), "service_repair_knowledge_index", json!({"libraryId":"library"})),
        (kb::get_knowledge_index_integrity("library"), "service_get_knowledge_index_integrity", json!({"libraryId":"library"})),
        (kb::list_knowledge_custom_words("library"), "service_list_knowledge_custom_words", json!({"libraryId":"library"})),
        (kb::browse_knowledge_packages(None), "service_browse_knowledge_packages", json!({"path":null})),
        (kb::get_session_knowledge_libraries("session"), "service_get_session_knowledge_libraries", json!({"conversationId":"session"})),
    ];
    for (call, method, envelope) in cases {
        assert_eq!(call.method, method);
        assert_eq!(call.params, envelope, "{method}");
        if let CallKind::Mutate(slots) = call.kind {
            assert!(
                !slots.contains(&Slot::KnowledgeSearch),
                "Committed writes must not replay paid queries"
            );
        }
    }
    assert_eq!(
        kb::list_knowledge_libraries(None).params,
        json!({"projectId":null})
    );
}

/// Only the editable name changes after package inspection; the source bytes remain frozen.
/// 套件预检后仅改变可编辑名称；来源字节保持冻结。
#[test]
fn package_confirmation_preserves_the_archive_identity() {
    // This is the current inspection shape, with one complete archive identity.
    // 此为当前预检形状，具有一个完整归档身份。
    let inspection = json!({"archivePath":"X:/synthetic/package.vkb","sha256":"a".repeat(64),"manifest":{"name":"Independent"}});
    // Confirmation creates a new-library request without a replacement target.
    // 确认创建新库请求，不具有替换目标。
    let call = kb::confirmed_package_import(&inspection).unwrap();
    assert_eq!(
        call.param("/request/archivePath"),
        &inspection["archivePath"]
    );
    assert_eq!(call.param("/request/expectedSha256"), &inspection["sha256"]);
    assert_eq!(call.param("/request/name"), &inspection["manifest"]["name"]);
    for field in ["archivePath", "sha256", "manifest"] {
        // Required fields cannot be replaced with a legacy or guessed owner.
        // 必需字段不能替换为旧版或猜测所有者。
        let mut invalid = inspection.clone();
        invalid.as_object_mut().unwrap().remove(field);
        assert!(kb::confirmed_package_import(&invalid).is_none());
    }
    assert!(kb::confirmed_package_import(&Value::Null).is_none());
}
