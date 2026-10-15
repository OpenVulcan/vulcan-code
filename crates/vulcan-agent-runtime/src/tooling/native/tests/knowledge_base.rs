use super::*;
use crate::agent_runtime::ExecutionKnowledgeAccessContext;
use crate::tooling::native::NativeToolDefinition;
use vulcan_session_core::v1::ids::{ApiCallId, RequestStreamId, Seq, ToolCallId, TurnId};
use vulcan_tools_manager::VulcanToolInvocationContext;
use vulcan_workspace_store::data_plane::{
    HostRuntimeHandle, HostRuntimeMetadata, prepare_application_schema_for_test,
};
use vulcan_workspace_store::knowledge_base::*;
use vulcan_workspace_store::local_database_runtime::LogicalDatabase;
use vulcan_workspace_store::local_sql;

/// Returns all four production definitions without host state.
/// 无需宿主状态，返回全部四项生产定义。
fn knowledge_tools() -> Vec<Arc<NativeToolDefinition>> {
    vec![
        build_knowledge_search_tool(),
        build_knowledge_list_tool(),
        build_knowledge_read_tool(),
        build_knowledge_write_tool(),
    ]
}

/// Checks closed schemas and the sole persistent mutation's Plan exclusion.
/// 检查封闭结构及唯一持久变更工具的计划模式排除。
#[test]
fn knowledge_tool_surface_is_closed_and_mode_specific() {
    for tool in knowledge_tools() {
        assert!(tool.descriptor.ai_visible);
        assert_eq!(
            tool.descriptor.available_in_plan,
            tool.descriptor.id != "knowledge_write"
        );
        assert_eq!(tool.args_schema["additionalProperties"], false);
        assert_eq!(tool.args_schema["type"], "object");
        assert!(tool.descriptor.description.is_ascii());
    }
    // Every supported projection preserves unambiguous file and provenance item types.
    // 全部受支持投影保留无歧义的文件及来源证据条目类型。
    for profile in [
        vulcan_tools_manager::VulcanToolSchemaProjectionProfile::PassThrough,
        vulcan_tools_manager::VulcanToolSchemaProjectionProfile::StandardObjectRoot,
        vulcan_tools_manager::VulcanToolSchemaProjectionProfile::GeminiSlim,
    ] {
        let schema = vulcan_tools_manager::project_tool_parameters_schema(
            &build_knowledge_write_tool().args_schema,
            profile,
        );
        assert_eq!(schema["properties"]["sources"]["items"]["type"], "string");
        assert_eq!(
            schema["properties"]["provenance"]["items"]["type"],
            "object"
        );
    }
}

/// Rejects legacy policies and target shapes instead of interpreting them as new requests.
/// 拒绝旧策略及目标形状，不能解释为新请求。
#[tokio::test]
async fn knowledge_tool_arguments_reject_obsolete_and_unknown_fields() {
    for (tool, arguments) in [
        (
            build_knowledge_search_tool(),
            json!({"queries":["q"],"retrievalMode":"raw_bm25"}),
        ),
        (
            build_knowledge_search_tool(),
            json!({"queries":["q"],"library_ids":["library"]}),
        ),
        (
            build_knowledge_read_tool(),
            json!({"libraryId":"l","targetType":"chunk","targetId":"c"}),
        ),
        (
            build_knowledge_list_tool(),
            json!({"libraryId":"l","directory":"old"}),
        ),
    ] {
        assert!(
            tool.invoke(test_native_request(arguments))
                .await
                .unwrap_err()
                .message
                .starts_with("ERR_KNOWLEDGE_ARGUMENTS")
        );
    }
    for tool in knowledge_tools() {
        // Each tool receives a minimally well-formed input so access admission is actually reached.
        // 每项工具接收最小合法输入，以实际到达访问准入。
        let arguments = match tool.descriptor.id.as_str() {
            "knowledge_search" => json!({"queries":["q"]}),
            "knowledge_read" => json!({"libraryId":"library","path":"guide.md"}),
            "knowledge_write" => {
                json!({"libraryId":"library","action":"folder","path":"folder","description":"purpose"})
            }
            _ => json!({}),
        };
        assert_eq!(
            tool.invoke(test_native_request(arguments))
                .await
                .unwrap_err()
                .message,
            "ERR_KNOWLEDGE_ACCESS_DISABLED"
        );
    }
}

/// Creates a private production-schema host and project-authorized library; returns their lifetime root.
/// 创建私有生产结构宿主及项目授权知识库；返回其生命周期根目录。
async fn fixture() -> (
    tempfile::TempDir,
    HostRuntimeHandle,
    KnowledgeBaseRuntime,
    String,
) {
    // This root owns every test database and ingested file.
    // 此根目录拥有全部测试数据库及搬运文件。
    let root = tempfile::tempdir().unwrap();
    // The real local runtime isolates each fixture's binding registry.
    // 真实本地运行时隔离每个夹具的绑定注册表。
    let host = HostRuntimeHandle::new_for_test(HostRuntimeMetadata {
        data_root: root.path().join("data"),
        sqlite_path: root.path().join("app.sqlite3"),
        sqlite_binding_id: Uuid::new_v4().to_string(),
        system_statistics_sqlite_path: root.path().join("stats.sqlite3"),
        system_statistics_sqlite_binding_id: Uuid::new_v4().to_string(),
    });
    prepare_application_schema_for_test(&host).await.unwrap();
    local_sql::execute_script_async(&host.local, LogicalDatabase::AppState, "INSERT INTO projects(id,name,root_path,created_at,updated_at) VALUES('project','Test','test','','');", vec![]).await.unwrap();
    // No inference or external service is needed for content admission.
    // 内容准入不需要推理或外部服务。
    let runtime = KnowledgeBaseRuntime::new(host.clone());
    // Library creation binds only this synthetic project.
    // 库创建仅绑定此合成项目。
    let library = runtime
        .create_library(
            "Test",
            "Reference purpose",
            &["project".into()],
            &KnowledgeLibrarySettings {
                index_mode: KnowledgeIndexMode::Bm25,
                embedding: None,
            },
        )
        .await
        .unwrap();
    (root, host, runtime, library.id)
}

/// Builds an enabled child-session invocation for host and library with exact synthetic turn/tool IDs.
/// 为宿主及知识库构建启用的子会话调用，携带精确合成回合及工具身份。
/// arguments becomes the actual model input; returns a fully owned native request.
/// arguments 成为实际模型输入；返回拥有完整所有权的原生请求。
fn request(
    host: &HostRuntimeHandle,
    library: &str,
    root: &Path,
    arguments: serde_json::Value,
) -> NativeToolExecutionRequest {
    // Distinct UUIDs ensure history cannot accidentally use a root/parent session.
    // 不同 UUID 确保历史无法意外使用根会话或父会话。
    let session = Uuid::new_v4();
    // Stable local IDs intentionally differ from one another and provider IDs.
    // 稳定本地身份特意彼此不同，并与 Provider 身份区分。
    let invocation = VulcanToolInvocationContext {
        session_id: session.to_string(),
        turn_id: TurnId::try_new(73).unwrap(),
        api_call_id: ApiCallId::try_new(91).unwrap(),
        tool_call_id: ToolCallId::try_new(127).unwrap(),
        tool_seq: Seq::try_new(188).unwrap(),
        request_stream_id: RequestStreamId::try_new("stream").unwrap(),
        controller_tool_call_id: "controller-call".into(),
        provider_tool_call_id: Some("provider-call".into()),
    };
    NativeToolExecutionRequest {
        invocation_context: Some(invocation),
        session_id: Some(session),
        host_project_id: Some("project".into()),
        controller_runtime: Some(host.clone()),
        workspace_root: Some(root.to_path_buf()),
        knowledge_access: Some(ExecutionKnowledgeAccessContext {
            project_id: "project".into(),
            library_ids: vec![library.into()],
            search_enabled: true,
        }),
        ..test_native_request(arguments)
    }
}

/// Verifies actual invocation identities, optimistic edits and single-copy exact reads.
/// 验证实际调用身份、乐观编辑及单副本精确读取。
#[tokio::test]
async fn knowledge_write_records_exact_actor_and_rejects_revision_conflicts() {
    // Real persistence verifies the bridge-to-storage identity boundary.
    // 真实持久化验证桥接至存储的身份边界。
    let (root, host, runtime, library) = fixture().await;
    // The source has CRLF so accidental newline rewriting is visible.
    // 来源使用 CRLF，使意外换行改写可见。
    let create = request(
        &host,
        &library,
        root.path(),
        json!({"libraryId":library,"action":"create","path":"guide.md","title":"Guide","summary":"Reference","body":"one\r\ntwo\r\n","provenance":[{"kind":"session","ref":"source-session","observedAtMs":1}]}),
    );
    // The expected session comes from the exact request, never a test-global placeholder.
    // 期望会话来自精确请求，绝非测试全局占位值。
    let session = create.session_id.unwrap().to_string();
    // Receipt metadata is consumed by UI without copying a document body.
    // 界面消费回执元数据，不复制文档正文。
    let result = build_knowledge_write_tool().invoke(create).await.unwrap();
    assert_eq!(
        result.host_result.as_ref().unwrap().kind,
        "knowledge_write_v1"
    );
    assert_eq!(
        result.host_result.as_ref().unwrap().payload["change_seq"],
        1
    );
    assert!(
        !result
            .host_result
            .as_ref()
            .unwrap()
            .payload
            .to_string()
            .contains("one")
    );
    // Canonical history proves the actual child identity and local IDs reached storage.
    // 规范历史证明实际子身份及本地身份到达存储。
    let history = runtime.get_change(&library, 1).await.unwrap();
    assert_eq!(
        history.change.actor.session_id.as_deref(),
        Some(session.as_str())
    );
    assert_eq!(history.change.actor.turn_id.as_deref(), Some("73"));
    assert_eq!(history.change.actor.tool_call_id.as_deref(), Some("127"));
    assert_eq!(history.items.len(), 1);
    // Read by logical path returns the exact stored bytes and usable revision.
    // 按逻辑路径读取返回精确存储字节及可用修订。
    let document = runtime
        .read_document(&library, None, Some("guide.md"), None, None)
        .await
        .unwrap();
    assert_eq!(document.content, "one\r\ntwo\r\n");
    assert!(
        document
            .document
            .provenance
            .iter()
            .any(|source| source.kind == "session" && source.reference == "source-session")
    );
    // A stale token is rejected without appending history.
    // 过期令牌被拒绝，不追加历史。
    let stale = request(
        &host,
        &library,
        root.path(),
        json!({"libraryId":library,"action":"edit","path":"guide.md","expectedRevision":document.document.revision+1,"edits":[{"oldText":"one","newText":"changed"}]}),
    );
    assert!(
        build_knowledge_write_tool()
            .invoke(stale)
            .await
            .unwrap_err()
            .message
            .contains("REVISION_CONFLICT")
    );
    assert_eq!(
        runtime
            .list_changes(&library, None, 20)
            .await
            .unwrap()
            .len(),
        1
    );
    // Exactly one source body is projected into a read result.
    // 仅一份来源正文投影至读取结果。
    let read = build_knowledge_read_tool()
        .invoke(request(
            &host,
            &library,
            root.path(),
            json!({"libraryId":library,"path":"guide.md"}),
        ))
        .await
        .unwrap();
    assert_eq!(
        read.model_output
            .as_str()
            .unwrap()
            .matches("one\r\ntwo")
            .count(),
        1
    );
}

/// Verifies raw host transfer, independent source failures, unchanged detection and stale summaries.
/// 验证宿主原文搬运、独立来源失败、无变化检测及摘要待复核。
#[tokio::test]
async fn knowledge_ingest_preserves_body_and_never_echoes_it() {
    // Every source remains under the temporary fixture root.
    // 全部来源留在临时夹具根目录下。
    let (root, host, runtime, library) = fixture().await;
    std::fs::create_dir_all(root.path().join("sources/nested")).unwrap();
    std::fs::create_dir_all(root.path().join("sources/.git")).unwrap();
    std::fs::write(
        root.path().join("sources/nested/guide.md"),
        "# Guide\r\nprivate-body-token\r\n## Topic\r\n",
    )
    .unwrap();
    std::fs::write(
        root.path().join("sources/binary.md"),
        [0, 0, 0, 0, 0xff, 0xfe],
    )
    .unwrap();
    std::fs::write(root.path().join("sources/.git/hidden.md"), "ignored").unwrap();
    // A glob retains the nested suffix and uses the shared file-tool ignores.
    // glob 保留嵌套后缀，并使用共享文件工具忽略规则。
    let mut arguments = json!({"libraryId":library,"action":"ingest","sources":["sources/**/*.md"],"targetFolder":"/refs/"});
    // A summary for a rejected binary source is reported without cancelling the valid source.
    // 被拒绝二进制来源的摘要会被报告，不会取消有效来源。
    let rejected_source = std::fs::canonicalize(root.path().join("sources/binary.md"))
        .unwrap()
        .to_str()
        .unwrap()
        .replace('\\', "/");
    arguments["summaries"] = json!({});
    arguments["summaries"][&rejected_source] = "Unused summary".into();
    // Only metadata and parsed headings may cross the model result boundary.
    // 仅元数据及已解析标题可跨越模型结果边界。
    let result = build_knowledge_write_tool()
        .invoke(request(&host, &library, root.path(), arguments.clone()))
        .await
        .unwrap();
    assert!(
        !result
            .model_output
            .to_string()
            .contains("private-body-token")
    );
    assert!(result.model_output.to_string().contains("binary.md"));
    assert!(result.model_output.to_string().contains("Topic"));
    assert!(
        result
            .model_output
            .to_string()
            .contains("ERR_KNOWLEDGE_SUMMARY_NOT_APPLIED")
    );
    // The accepted file has exact bytes after decoding, and the batch is one change.
    // 已接受文件具有解码后的精确字节，且批次仅为一条变更。
    let doc = runtime
        .read_document(&library, None, Some("refs/nested/guide.md"), None, None)
        .await
        .unwrap();
    assert_eq!(doc.content, "# Guide\r\nprivate-body-token\r\n## Topic\r\n");
    assert_eq!(doc.document.summary_state, "pending");
    assert_eq!(
        runtime.get_library(&library).await.unwrap().document_count,
        1
    );
    assert_eq!(
        runtime
            .list_changes(&library, None, 20)
            .await
            .unwrap()
            .len(),
        1
    );
    // An unchanged source does not generate a history sequence.
    // 无变化来源不生成历史序号。
    let unchanged = build_knowledge_write_tool()
        .invoke(request(&host, &library, root.path(), arguments.clone()))
        .await
        .unwrap();
    assert!(unchanged.host_result.unwrap().payload["change_seq"].is_null());
    // AI summary maintenance is a distinct explicit change.
    // AI 摘要维护为独立显式变更。
    build_knowledge_write_tool().invoke(request(&host, &library, root.path(), json!({"libraryId":library,"action":"update","path":doc.document.path,"expectedRevision":doc.document.revision,"summary":"AI summary"}))).await.unwrap();
    std::fs::write(
        root.path().join("sources/nested/guide.md"),
        "# Guide\nnew-private-body\n",
    )
    .unwrap();
    build_knowledge_write_tool()
        .invoke(request(&host, &library, root.path(), arguments))
        .await
        .unwrap();
    // Re-ingestion preserves the prior summary but marks it stale.
    // 再搬运保留先前摘要，但标记待复核。
    let changed = runtime
        .read_document(&library, None, Some("refs/nested/guide.md"), None, None)
        .await
        .unwrap();
    assert_eq!(changed.document.summary, "AI summary");
    assert_eq!(changed.document.summary_state, "stale");
    assert!(changed.document.revision > doc.document.revision);
}

/// Rechecks live authorization and exact actor identity before any content mutation.
/// 在任何内容变更前复核实时授权及精确执行者身份。
#[tokio::test]
async fn knowledge_write_revocation_and_missing_identity_fail_closed() {
    // Requests retain their original access ceiling while live binding state can change.
    // 请求保留原访问上限，同时实时绑定状态可变化。
    let (root, host, runtime, library) = fixture().await;
    // The write body is valid so failures must come from the claimed boundary.
    // 写入正文合法，使失败必须来自声明边界。
    let args =
        json!({"libraryId":library,"action":"folder","path":"folder","description":"purpose"});
    // Missing invocation identity cannot be replaced by session or API guesses.
    // 缺失调用身份不能由会话或 API 推测替代。
    let mut missing = request(&host, &library, root.path(), args.clone());
    missing.invocation_context = None;
    assert_eq!(
        build_knowledge_write_tool()
            .invoke(missing)
            .await
            .unwrap_err()
            .message,
        "ERR_KNOWLEDGE_INVOCATION_REQUIRED"
    );
    // A mismatched exact session fails before history is written.
    // 精确会话不匹配时在历史写入前失败。
    let mut mismatch = request(&host, &library, root.path(), args.clone());
    mismatch.session_id = Some(Uuid::new_v4());
    assert_eq!(
        build_knowledge_write_tool()
            .invoke(mismatch)
            .await
            .unwrap_err()
            .message,
        "ERR_KNOWLEDGE_SESSION_MISMATCH"
    );
    // This request was enabled before project authorization was revoked.
    // 此请求在项目授权撤销前已启用。
    let revoked = request(&host, &library, root.path(), args);
    runtime
        .set_project_binding(&library, "project", false, false)
        .await
        .unwrap();
    assert_eq!(
        build_knowledge_write_tool()
            .invoke(revoked)
            .await
            .unwrap_err()
            .message,
        "ERR_KNOWLEDGE_NOT_AUTHORIZED"
    );
    assert!(
        runtime
            .list_changes(&library, None, 20)
            .await
            .unwrap()
            .is_empty()
    );
}
