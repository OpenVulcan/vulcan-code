//! Current knowledge lifecycle tests use the public UI builders against isolated synthetic state.
//! 当前知识库生命周期测试通过公开界面构建器访问隔离合成状态。

use super::FakeService;
use crate::service_api::{ServiceCall, Slot, knowledge_base as kb};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use vulcan_service_api::*;

/// Builds an exact synthetic logical-directory request.
/// 构建精确合成逻辑目录请求。
/// Parameters: library/path/cursor own the scope; limit is the requested viewport. Returns an unexecuted call.
/// 参数：library、path 及 cursor 拥有范围；limit 为请求视口。返回未执行调用。
fn list(library: &str, path: &str, cursor: Option<String>, limit: u32) -> ServiceCall {
    kb::list_managed_knowledge_documents(KnowledgeLibraryDocumentListRequest {
        library_id: library.into(),
        path: path.into(),
        depth: 1,
        cursor,
        limit,
    })
}

/// Builds an exact document selector without corpus or source fields.
/// 构建不含语料或来源字段的精确文档选择器。
/// Parameters: library/id own the current document. Returns its initial body request.
/// 参数：library 及 id 拥有当前文档。返回其初始正文请求。
fn read(library: &str, id: &str) -> ServiceCall {
    kb::read_managed_knowledge_document(KnowledgeDocumentReadRequest {
        library_id: library.into(),
        document_id: Some(id.into()),
        path: None,
        start_line: None,
        end_line: None,
    })
}

/// Logical cursors isolate paths and current bodies preserve exact fixture whitespace.
/// 逻辑游标隔离路径，当前正文保留精确夹具空白。
#[test]
fn logical_directory_and_body_requests_round_trip_current_shapes() {
    // This service owns no real files, ports or model calls.
    // 此服务不拥有真实文件、端口或模型调用。
    let mut service = FakeService::new();
    // Root lists one implicit/explicit directory and the root document, without loading bodies.
    // 根目录列出一个隐式或显式目录及根文档，不加载正文。
    let root = service.answer(&list("demo-library", "", None, 7)).unwrap();
    assert!(
        root["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["kind"] == "folder" && entry["path"] == "guide")
    );
    assert!(
        root["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["document"]["id"] == "demo-document")
    );
    // Paging follows only returned opaque cursors; callers cannot reuse a cursor under another path.
    // 分页仅遵循返回不透明游标；调用方不能在另一条路径复用游标。
    let first = service
        .answer(&list("demo-library", "guide", None, 7))
        .unwrap();
    // A fixture edit may add or remove documents, but this test still verifies bounded, duplicate-free traversal.
    // 夹具编辑可增删文档，此测试仍验证有界且无重复遍历。
    let mut page = first.clone();
    // Stable document IDs, rather than mutable array positions, detect duplicate page entries.
    // 稳定文档身份检测重复分页条目，而非可变数组位置。
    let mut seen = BTreeSet::new();
    loop {
        assert!(page["entries"].as_array().unwrap().len() <= 7);
        for entry in page["entries"].as_array().unwrap() {
            assert!(seen.insert(entry["document"]["id"].as_str().unwrap().to_string()));
        }
        // Explicit null is the current service's exhausted continuation.
        // 明确空值为当前服务已耗尽续读标识。
        let Some(cursor) = page["nextCursor"].as_str().map(str::to_string) else {
            break;
        };
        page = service
            .answer(&list("demo-library", "guide", Some(cursor), 7))
            .unwrap();
    }
    assert!(
        seen.len() > 7,
        "guide fixture should retain enough records to exercise pagination"
    );
    assert!(
        service
            .answer(&list(
                "demo-library",
                "",
                Some(first["nextCursor"].as_str().unwrap().into()),
                7
            ))
            .is_err()
    );
    // Locate the source fixture by identity; body text remains the comparison authority.
    // 按身份定位来源夹具；正文文本保持为对比权威。
    let fixture: Value = serde_json::from_str(include_str!("service-fixtures.json")).unwrap();
    // Stable ID selection avoids asserting evolving fixture positions.
    // 稳定身份选择避免断言会演进的夹具位置。
    let entry = fixture["knowledgeDocuments"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["document"]["id"] == "demo-document")
        .expect("current fixture document may have changed");
    assert_eq!(
        service
            .answer(&read("demo-library", "demo-document"))
            .unwrap()["content"],
        entry["body"]
    );
    assert!(service.answer(&read("unknown", "demo-document")).is_err());
}

/// Inspection freezes bytes and importing creates a new unbound library with one independent history.
/// 预检冻结字节，导入创建不带绑定且具有一个独立历史的新知识库。
#[test]
fn package_confirmation_creates_independent_content_and_rejects_changed_bytes() {
    // Synthetic export never writes to a user's filesystem.
    // 合成导出绝不写入用户文件系统。
    let mut service = FakeService::new();
    // Freeze the current content and configuration through the actual export builder.
    // 通过实际导出构建器冻结当前内容及配置。
    let exported = service
        .answer(&kb::export_knowledge_package(
            KnowledgePackageExportRequest {
                library_id: "demo-library".into(),
                destination_directory: "X:/synthetic".into(),
            },
        ))
        .unwrap();
    // Host listing exposes the exact native archive path.
    // 宿主列表公开精确原生归档路径。
    let listing = service
        .answer(&kb::browse_knowledge_packages(None))
        .unwrap();
    assert!(
        listing["packages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|file| file["path"] == exported["archivePath"])
    );
    // Inspection does not create or change a library.
    // 预检不创建或改变知识库。
    let inspected = service
        .answer(&kb::inspect_knowledge_package(
            KnowledgePackageInspectRequest {
                archive_path: exported["archivePath"].as_str().unwrap().into(),
            },
        ))
        .unwrap();
    // A mismatched complete fingerprint must fail before registry mutation.
    // 完整指纹不匹配必须在注册表变更前失败。
    let mut call = kb::confirmed_package_import(&inspected).unwrap();
    call.params["request"]["name"] = json!("Imported");
    call.params["request"]["expectedSha256"] = json!("0".repeat(64));
    assert!(service.answer(&call).is_err());
    assert_eq!(
        service
            .answer(&kb::list_knowledge_libraries(None))
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        1
    );
    call.params["request"]["expectedSha256"] = inspected["sha256"].clone();
    // Only the explicitly edited name changes in the confirmed request.
    // 确认请求中仅明确编辑的名称变化。
    let result = service.answer(&call).unwrap();
    // Fresh physical identity owns new stable document IDs and no project grants.
    // 全新物理身份拥有新稳定文档身份，且不具有项目授权。
    let id = result["library"]["id"].as_str().unwrap();
    assert_ne!(id, "demo-library");
    assert_eq!(result["changeSeq"], 1);
    assert!(
        service
            .answer(&kb::list_knowledge_project_bindings(id))
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty()
    );
    // Import starts from exactly one current-content transaction.
    // 导入从精确一个当前内容事务开始。
    let changes = service
        .answer(&kb::list_knowledge_changes(KnowledgeChangeListRequest {
            library_id: id.into(),
            before_seq: None,
            limit: 7,
        }))
        .unwrap();
    assert_eq!(changes.as_array().unwrap().len(), 1);
    assert_eq!(changes[0]["operation"], "import");
    // Logical paths survive import while source-library identity never does.
    // 逻辑路径在导入后保留，来源知识库身份绝不保留。
    let imported = service
        .answer(&kb::read_managed_knowledge_document(
            KnowledgeDocumentReadRequest {
                library_id: id.into(),
                document_id: None,
                path: Some("readme.md".into()),
                start_line: None,
                end_line: None,
            },
        ))
        .unwrap();
    assert_ne!(imported["document"]["id"], "demo-document");
    assert_eq!(
        imported["content"],
        service
            .answer(&read("demo-library", "demo-document"))
            .unwrap()["content"]
    );
}

/// Restoration appends permanent history and historical body sides never fall back to current content.
/// 还原追加永久历史，历史正文侧别绝不兜底为当前内容。
#[test]
fn restores_preserve_history_and_append_monotonic_revisions() {
    // The initial fixture starts with one content import change.
    // 初始夹具从一个内容导入变更开始。
    let mut service = FakeService::new();
    // Capture the original current body and revision before changing content.
    // 改变内容前捕获原始当前正文及修订号。
    let original = service
        .answer(&read("demo-library", "demo-document"))
        .unwrap();
    service
        .answer(&kb::restore_knowledge_library(KnowledgeRestoreRequest {
            library_id: "demo-library".into(),
            target_seq: 0,
            note: String::new(),
        }))
        .unwrap();
    assert!(
        service
            .answer(&read("demo-library", "demo-document"))
            .is_err()
    );
    // Immutable after-side content remains readable after current deletion.
    // 当前删除后不可变后侧正文仍可读取。
    let mut historical = KnowledgeChangeDocumentReadRequest {
        library_id: "demo-library".into(),
        seq: 1,
        document_id: "demo-document".into(),
        side: KnowledgeChangeSideRequest::After,
        start_line: None,
        end_line: None,
    };
    assert_eq!(
        service
            .answer(&kb::read_knowledge_change_document(historical.clone()))
            .unwrap()["content"],
        original["content"]
    );
    historical.side = KnowledgeChangeSideRequest::Before;
    assert!(
        service
            .answer(&kb::read_knowledge_change_document(historical))
            .is_err()
    );
    service
        .answer(&kb::restore_knowledge_library(KnowledgeRestoreRequest {
            library_id: "demo-library".into(),
            target_seq: 1,
            note: String::new(),
        }))
        .unwrap();
    // Restoring old bytes still assigns a new optimistic token.
    // 还原旧字节仍分配新乐观令牌。
    let restored = service
        .answer(&read("demo-library", "demo-document"))
        .unwrap();
    assert_eq!(restored["content"], original["content"]);
    assert!(
        restored["document"]["revision"].as_u64().unwrap()
            > original["document"]["revision"].as_u64().unwrap()
    );
    // Every previous sequence remains addressable after both restore operations.
    // 两次还原操作后每个先前序号仍可寻址。
    let changes = service
        .answer(&kb::list_knowledge_changes(KnowledgeChangeListRequest {
            library_id: "demo-library".into(),
            before_seq: None,
            limit: 7,
        }))
        .unwrap();
    assert_eq!(changes.as_array().unwrap().len(), 3);
    assert!(
        changes
            .as_array()
            .unwrap()
            .iter()
            .any(|change| change["seq"] == 1)
    );
}

/// Operation cancellation uses stable IDs and retry resumes the same configured vector space.
/// 操作取消使用稳定身份，重试恢复相同已配置向量空间。
#[test]
fn durable_jobs_cancel_exactly_and_retry_without_replacing_the_space() {
    // Route facts are explicit synthetic protocol data, with no provider execution.
    // 路由事实为明确合成协议数据，不执行供应商。
    let mut service = FakeService::new();
    // This complete vector request mirrors an admitted fixture route.
    // 此完整向量请求镜像一个准入夹具路由。
    let mode:KnowledgeIndexModeRequestPayload=serde_json::from_value(json!({"libraryId":"demo-library","settings":{"indexMode":"vector","embedding":{"route":{"providerId":"demo-provider","protocol":"demo-protocol","bindingKind":"protocol","serviceEntryId":"demo-api","modelId":"demo-model-1","connectionId":"demo-connection"},"dimensions":384,"batchSize":null}}})).unwrap();
    // The queued receipt retains its stable ID after subsequent observations.
    // 排队回执在后续观察后保留稳定身份。
    let job = service.answer(&kb::set_knowledge_index_mode(mode)).unwrap();
    assert!(
        service
            .answer(&kb::rebuild_knowledge_index("demo-library"))
            .is_err()
    );
    assert_eq!(
        service
            .answer(&kb::cancel_knowledge_operation(
                KnowledgeOperationCancelRequest {
                    library_id: "demo-library".into(),
                    operation_id: "stale".into()
                }
            ))
            .unwrap(),
        false
    );
    assert_eq!(
        service
            .answer(&kb::cancel_knowledge_operation(
                KnowledgeOperationCancelRequest {
                    library_id: "demo-library".into(),
                    operation_id: job["id"].as_str().unwrap().into()
                }
            ))
            .unwrap(),
        true
    );
    // Capture the desired space before retrying paused work.
    // 重试暂停工作前捕获期望空间。
    let status = kb::get_knowledge_embedding_status(KnowledgeEmbeddingStatusRequest {
        library_id: "demo-library".into(),
        offset: 0,
        limit: 7,
    });
    // Current pending work is resumed without resetting ready hashes or allocating another space.
    // 当前待处理工作恢复，不重置就绪哈希或配置另一空间。
    let before = service.answer(&status).unwrap();
    service
        .answer(&kb::retry_knowledge_embeddings(
            KnowledgeEmbeddingRetryRequest {
                library_id: "demo-library".into(),
                document_ids: Vec::new(),
            },
        ))
        .unwrap();
    assert_eq!(
        service.answer(&status).unwrap()["desiredSpaceId"],
        before["desiredSpaceId"]
    );
    // Observations advance the synthetic job; the production store tests own real worker scheduling.
    // 观察推进合成任务；生产存储测试拥有真实工作者调度验证。
    let observe = kb::get_knowledge_operation(KnowledgeOperationRequest {
        library_id: "demo-library".into(),
        operation_id: None,
    });
    service.answer(&observe).unwrap();
    assert_eq!(service.answer(&observe).unwrap()["state"], "completed");
}

/// Live project grants bound saved selections, and old methods fail explicitly.
/// 实时项目授权约束已保存选择，旧方法明确失败。
#[test]
fn session_selection_rechecks_ownership_and_legacy_methods_are_refused() {
    // Ownership is taken from World's actual typed synthetic conversations.
    // 归属取自 World 实际强类型合成会话。
    let mut service = FakeService::new();
    // Select a saved session by its project fact rather than by a guessed ID.
    // 按项目事实选择已保存会话，而非猜测身份。
    let session = super::super::fixtures::conversations()
        .into_iter()
        .find(|session| session.project_id.as_deref() == Some("p-tui"))
        .expect("project-owned session fixture may have changed");
    // Inherited selection is a runtime read and does not create persistent IDs.
    // 继承选择为运行时读取，不创建持久身份。
    let get = kb::get_session_knowledge_libraries(&session.id);
    assert_eq!(
        service.answer(&get).unwrap()["effectiveLibraryIds"],
        json!(["demo-library"])
    );
    service
        .answer(&kb::set_knowledge_project_binding(
            KnowledgeProjectBindingRequest {
                library_id: "demo-library".into(),
                project_id: "p-tui".into(),
                authorized: false,
                default_enabled: false,
            },
        ))
        .unwrap();
    assert!(
        service.answer(&get).unwrap()["effectiveLibraryIds"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        service
            .answer(&kb::set_session_knowledge_libraries(
                SessionKnowledgeLibrarySelectionRequest {
                    project_id: "p-tui".into(),
                    conversation_id: session.id,
                    mode: "explicit".into(),
                    library_ids: vec!["demo-library".into()]
                }
            ))
            .is_err()
    );
    for method in [
        "service_list_knowledge_sources",
        "service_preview_knowledge_import",
        "service_get_knowledge_import_job",
        "service_build_knowledge_display_index",
    ] {
        assert!(
            service
                .answer(&ServiceCall::load(
                    "legacy",
                    method,
                    json!({}),
                    Slot::KnowledgeLibraries
                ))
                .is_err()
        );
    }
    service
        .answer(&kb::delete_knowledge_library("demo-library"))
        .unwrap();
    // Empty registry creation still requires a fully explicit current policy.
    // 空注册表创建仍要求完整明确当前策略。
    let created = service
        .answer(&kb::create_knowledge_library(
            KnowledgeLibraryCreateRequest {
                name: "New".into(),
                description: "Purpose".into(),
                project_ids: Vec::new(),
                settings: KnowledgeLibrarySettingsRequest {
                    index_mode: KnowledgeIndexModeRequest::Bm25,
                    embedding: None,
                },
            },
        ))
        .unwrap();
    assert!(
        service
            .answer(&list(created["id"].as_str().unwrap(), "", None, 7))
            .unwrap()["entries"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
