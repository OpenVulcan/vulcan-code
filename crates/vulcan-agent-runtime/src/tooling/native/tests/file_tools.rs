use crate::tooling::native::{
    APPLY_PATCH_LANGUAGE_NAME, APPLY_PATCH_TOOL_ID, FILE_EDIT_TOOL_ID, FILE_TOOL_ID,
    build_apply_patch_tool, build_file_edit_tool, build_file_tool,
};
use serde_json::json;
use vulcan_change_set::{
    ChangeSetFileChangeKind, PersistentLifecycleContent, persistent_change_set_from_host_result,
};
use vulcan_tools_manager::{
    IncrementalJsonEvent, IncrementalJsonPathSegment, IncrementalJsonScalar,
    ToolRequestParserContext, ToolRequestParserSession, VULCAN_TOOL_ERROR_DETAIL_KIND,
};

use super::test_native_request_with_workspace;

/// Verifies list/stat return readable results without a duplicate host file-access object.
/// 验证列表和属性返回可读结果，不携带重复的宿主文件访问对象。
#[tokio::test]
async fn native_file_inspections_share_markdown_without_host_copies() {
    // The workspace has one distinct file so all assertions locate facts rather than array positions.
    // 工作区包含一个独特文件，使断言定位事实而非数组下标。
    let directory = tempfile::tempdir().expect("temporary workspace");
    std::fs::write(directory.path().join("single.txt"), "unique-body\n").expect("source");
    for (action, path, heading) in [
        ("list", ".", "# Files:"),
        ("stat", "single.txt", "# File information:"),
    ] {
        // Each action traverses the real native adapter rather than a hand-built presentation payload.
        // 每个操作经过真实原生适配器，而非手工构建展示载荷。
        let result = build_file_tool()
            .invoke(test_native_request_with_workspace(
                json!({"action": action, "path": path}),
                directory.path().to_path_buf(),
            ))
            .await
            .expect("file inspection");
        let body = result.model_output.as_str().expect("Markdown output");
        assert!(body.starts_with(heading), "{body}");
        assert!(body.contains("single.txt"), "{body}");
        assert!(
            result.host_result.is_none(),
            "inspection must not retain a JSON duplicate"
        );
    }
}

/// Verifies mutation receipts omit source text while their host changesets still support review.
/// 验证修改回执不包含源文本，且宿主变更集仍支持审阅。
#[tokio::test]
async fn native_file_mutations_keep_review_structure_and_compact_receipts() {
    // Sequential fixtures cover exact replacement and patch output through their public adapters.
    // 顺序夹具通过公开适配器覆盖精确替换与补丁输出。
    let directory = tempfile::tempdir().expect("temporary workspace");
    let cases = [
        (
            build_file_tool(),
            json!({"action": "write", "path": "edit.txt", "content": "first-unique-source\n"}),
        ),
        (
            build_file_edit_tool(),
            json!({"path": "edit.txt", "old_string": "first-unique-source", "new_string": "second-unique-source"}),
        ),
        (
            build_apply_patch_tool(),
            json!({"patch": "*** Begin Patch\n*** Add File: patch.txt\n+patch-unique-source\n*** End Patch"}),
        ),
        (
            build_file_tool(),
            json!({"action": "move", "path": "patch.txt", "destination": "moved.txt"}),
        ),
        (
            build_file_tool(),
            json!({"action": "delete", "path": "moved.txt"}),
        ),
    ];
    for (tool, arguments) in cases {
        // The authoritative changeset must survive the model-only projection.
        // 权威变更集必须在仅面向模型的投影后保留。
        let result = tool
            .invoke(test_native_request_with_workspace(
                arguments,
                directory.path().to_path_buf(),
            ))
            .await
            .expect("mutation invocation");
        let body = result.model_output.as_str().expect("Markdown receipt");
        assert!(body.starts_with("# File changes applied"), "{body}");
        assert!(
            !body.contains("unique-source"),
            "receipt must not echo written content"
        );
        let host = result.host_result.expect("review changeset");
        assert_eq!(host.kind, "change_set");
        assert!(
            !host.payload["files"]
                .as_array()
                .expect("changed files")
                .is_empty()
        );
        assert!(
            !body.contains("operation_id"),
            "internal operation records are not model output"
        );
    }
}

/// Verifies native read dispatch, canonicalization, and persistence retain exactly one source body.
/// 验证原生读取调度、规范化与持久化仅保留一份源文件正文。
#[tokio::test]
async fn native_file_read_persists_one_markdown_body() {
    use crate::agent_runtime::tool_runtime_bridge::{
        canonicalize_tool_invocation_result, persistent_tool_output_from_result,
    };
    // A unique source marker detects duplication anywhere in the serialized durable envelope.
    // 唯一源文件标记用于检测序列化持久封装任意位置的正文重复。
    let directory = tempfile::tempdir().expect("temporary workspace");
    let marker = "unique-file-read-source-7319";
    std::fs::write(directory.path().join("sample.txt"), format!("{marker}\n"))
        .expect("write source fixture");
    let arguments = json!({"action": "read", "path": "sample.txt"});
    let result = build_file_tool()
        .invoke(test_native_request_with_workspace(
            arguments.clone(),
            directory.path().to_path_buf(),
        ))
        .await
        .expect("read invocation");
    let request = vulcan_core::types::ToolInvocationRequest {
        request_stream_id: String::from("read-stream"),
        tool_call_id: String::from("read-call"),
        tool_name: String::from("file"),
        arguments,
        argument_parse_error: None,
    };
    let canonical = canonicalize_tool_invocation_result(&request, result);
    assert!(canonical.model_output.starts_with("# File: sample.txt\n"));
    assert!(canonical.model_output.contains(marker));
    assert!(!canonical.is_error);
    let host = canonical
        .host_result
        .as_ref()
        .expect("presentation metadata");
    assert_eq!(host["kind"], json!("file_access"));
    assert_eq!(host["payload"]["schema"], json!("vulcan.file_read.v1"));
    assert!(!host.to_string().contains(marker));
    let persisted = persistent_tool_output_from_result(&canonical).expect("durable tool payload");
    let serialized = serde_json::to_string(&persisted).expect("serialize durable tool payload");
    assert_eq!(
        serialized.matches(marker).count(),
        1,
        "source text must be stored exactly once"
    );
}

/// Verifies the three public native file tools expose the frozen minimal contracts.
/// 验证三个公开原生文件工具暴露冻结的最小契约。
#[test]
fn native_file_tools_expose_contracts() {
    let file = build_file_tool();
    assert_eq!(file.descriptor.id, FILE_TOOL_ID);
    assert!(file.descriptor.ai_visible);
    assert!(file.descriptor.available_in_plan);
    assert!(file.request_streaming.is_some());
    assert_eq!(file.args_schema["additionalProperties"], json!(false));
    assert_eq!(
        file.args_schema["properties"]["action"]["enum"],
        json!(["read", "list", "stat", "write", "delete", "move"])
    );
    assert_eq!(file.args_schema["required"], json!(["action", "path"]));
    assert!(file.args_schema["properties"].get("targets").is_none());
    assert!(file.args_schema["properties"].get("query").is_none());
    assert_eq!(
        file.args_schema["properties"]["recursive"]["default"],
        json!(false)
    );
    assert_eq!(
        file.args_schema["properties"]["limit"]["default"],
        json!(200)
    );
    assert_eq!(
        file.args_schema["properties"]["bom"]["default"],
        json!("preserve")
    );
    assert_eq!(
        file.args_schema["properties"]["line_ending"]["default"],
        json!("preserve")
    );
    assert!(
        file.args_schema["properties"]["content"]["description"]
            .as_str()
            .unwrap_or_default()
            .contains("Required for write")
    );
    assert!(
        file.args_schema["properties"]["destination"]["description"]
            .as_str()
            .unwrap_or_default()
            .contains("Required for move")
    );
    assert!(
        file.descriptor
            .description
            .contains("does not recursively delete directories")
    );

    let edit = build_file_edit_tool();
    assert_eq!(edit.descriptor.id, FILE_EDIT_TOOL_ID);
    assert!(edit.descriptor.ai_visible);
    assert!(!edit.descriptor.available_in_plan);
    assert_eq!(edit.args_schema["additionalProperties"], json!(false));
    assert_eq!(
        edit.args_schema["required"],
        json!(["path", "old_string", "new_string"])
    );
    assert!(edit.args_schema["properties"].get("operations").is_none());
    assert!(edit.args_schema["properties"].get("old_lines").is_none());
    assert!(edit.args_schema["properties"].get("anchors").is_none());
    assert!(
        edit.args_schema["properties"]
            .get("expected_version")
            .is_none()
    );
    assert!(edit.args_schema["properties"].get("dry_run").is_none());
    assert!(
        edit.args_schema["properties"]
            .get("final_newline")
            .is_none()
    );
    assert!(
        edit.descriptor
            .description
            .contains("CRLF, LFCR, CR, and LF")
    );
    assert!(edit.request_streaming.is_some());
    assert_eq!(
        edit.args_schema["properties"]["replace_all"]["default"],
        json!(false)
    );

    let patch = build_apply_patch_tool();
    assert_eq!(patch.descriptor.id, APPLY_PATCH_TOOL_ID);
    assert!(patch.descriptor.ai_visible);
    assert!(!patch.descriptor.available_in_plan);
    assert_eq!(patch.args_schema["required"], json!(["patch"]));
    assert!(
        patch.args_schema["properties"]["patch"]["description"]
            .as_str()
            .unwrap_or_default()
            .contains(APPLY_PATCH_LANGUAGE_NAME)
    );
    assert!(patch.request_streaming.is_some());
    assert!(patch.descriptor.description.contains("*** Begin Patch"));
    assert!(
        patch
            .descriptor
            .description
            .contains("*** Update File: src/example.rs")
    );
    assert!(
        patch
            .descriptor
            .description
            .contains("Do not use Markdown fences")
    );
    assert!(
        patch
            .descriptor
            .description
            .contains("*** Move to: destination")
    );
    assert!(
        patch
            .descriptor
            .description
            .contains("@@ exact anchor line")
    );
}

/// Verifies the actual public file-write result is accepted by the canonical V1 ChangeSet parser.
/// 验证真实公开文件写入结果可被规范 V1 ChangeSet 解析器接纳。
#[tokio::test]
async fn file_changeset_v1_native_create_round_trips_through_persistent_contract() {
    // workspace isolates the real native file mutation from every user and shared project path.
    // workspace 把真实原生文件变更与所有用户及共享项目路径隔离。
    let workspace = tempfile::tempdir().expect("create isolated file tool workspace");
    // file invokes the same public native tool descriptor used by Provider execution.
    // file 调用 Provider 执行使用的同一公开原生工具描述符。
    let file = build_file_tool();
    // result retains the exact structured host result emitted after the file is committed.
    // result 保留文件提交后发出的精确结构化宿主结果。
    let result = file
        .invoke(test_native_request_with_workspace(
            json!({
                "action": "write",
                "path": "created.txt",
                "content": "第一行\nsecond line\n"
            }),
            workspace.path().to_path_buf(),
        ))
        .await
        .expect("native file write should succeed");
    // host_result serializes the real runtime envelope without hand-building an expected ChangeSet.
    // host_result 序列化真实运行时封套，不手工构造预期 ChangeSet。
    let host_result = result
        .host_result
        .as_ref()
        .map(|host_result| {
            json!({
                "kind": host_result.kind,
                "payload": host_result.payload,
            })
        })
        .expect("successful write should expose one host result");
    // changeset is the canonical persistent representation consumed by the V1 session owner.
    // changeset 是 V1 会话 owner 消费的规范持久化表示。
    let changeset = persistent_change_set_from_host_result(&host_result)
        .expect("native host result must satisfy the canonical parser")
        .expect("native write must declare one change-set");

    assert_eq!(
        std::fs::read_to_string(workspace.path().join("created.txt"))
            .expect("created file should remain readable"),
        "第一行\nsecond line\n"
    );
    assert_eq!(changeset.files.len(), 1);
    assert_eq!(changeset.files[0].change, ChangeSetFileChangeKind::Create);
    assert!(changeset.files[0].hunks.is_empty());
    assert!(matches!(
        changeset.files[0].lifecycle_content,
        Some(PersistentLifecycleContent::Full {
            ref content,
            total_line_count: 2
        }) if content == "第一行\nsecond line\n"
    ));
}

/// Verifies malformed Apply Patch argument shapes remain tool results instead of aborting the run loop.
/// 验证畸形 Apply Patch 参数结构会保留为工具结果，而不会中断运行循环。
#[tokio::test]
async fn native_apply_patch_argument_errors_are_structured() {
    let patch = build_apply_patch_tool();
    let workspace = std::env::current_dir().expect("current workspace");
    for arguments in [json!(null), json!({"patch": 7})] {
        let result = patch
            .invoke(test_native_request_with_workspace(
                arguments,
                workspace.clone(),
            ))
            .await
            .expect("apply_patch should return a structured failure result");
        assert_eq!(result.model_output["status"], json!("failed"));
        assert_eq!(
            result.model_output["details"]["code"],
            json!("invalid_arguments")
        );
        assert_eq!(result.model_output["details"]["retryable"], json!(false));
        assert_eq!(
            result.host_result.as_ref().map(|value| value.kind.as_str()),
            Some(VULCAN_TOOL_ERROR_DETAIL_KIND)
        );
    }
}

/// Verifies native file-edit failures retain machine-readable host error details.
/// 验证原生 file_edit 失败时仍保留机器可读的宿主错误详情。
#[tokio::test]
async fn native_file_edit_errors_are_structured() {
    let edit = build_file_edit_tool();
    let result = edit
        .invoke(test_native_request_with_workspace(
            json!({"path": "missing.txt", "old_string": "old"}),
            std::env::current_dir().expect("current workspace"),
        ))
        .await
        .expect("file_edit should return a structured failure result");
    assert_eq!(result.model_output["status"], json!("failed"));
    assert_eq!(
        result.model_output["details"]["code"],
        json!("invalid_arguments")
    );
    assert_eq!(
        result.host_result.as_ref().map(|value| value.kind.as_str()),
        Some(VULCAN_TOOL_ERROR_DETAIL_KIND)
    );
}

/// Verifies an incomplete simplified edit never falls through to the hidden operation-array contract.
/// 验证不完整的简化编辑请求绝不会落入隐藏的 operations 数组契约。
#[tokio::test]
async fn native_file_edit_path_only_uses_the_public_contract() {
    let edit = build_file_edit_tool();
    let result = edit
        .invoke(test_native_request_with_workspace(
            json!({"path": "missing.txt"}),
            std::env::current_dir().expect("current workspace"),
        ))
        .await
        .expect("file_edit should return a structured failure result");
    assert_eq!(
        result.model_output["details"]["code"],
        json!("invalid_arguments")
    );
    assert!(
        result.model_output["error"]
            .as_str()
            .unwrap_or_default()
            .contains("old_string")
    );
    assert!(!result.model_output.to_string().contains("operations array"));
}

/// Verifies a missing public file path reports the current contract instead of the hidden targets array.
/// 验证公开 file 缺少路径时会报告当前契约，而不是隐藏的 targets 数组。
#[tokio::test]
async fn native_file_missing_path_uses_the_public_contract() {
    let file = build_file_tool();
    let result = file
        .invoke(test_native_request_with_workspace(
            json!({"action": "read"}),
            std::env::current_dir().expect("current workspace"),
        ))
        .await
        .expect("file should return a structured failure result");
    assert_eq!(
        result.model_output["details"]["code"],
        json!("invalid_arguments")
    );
    assert!(
        result.model_output["error"]
            .as_str()
            .unwrap_or_default()
            .contains("path")
    );
    assert!(!result.model_output.to_string().contains("targets array"));
}

/// Verifies streamed exact-text previews expose dynamic old/new lines and normalized endings.
/// 验证流式精确文本预览暴露动态旧新行并归一化换行。
#[test]
fn file_edit_stream_preview_tracks_exact_strings() {
    let edit_tool = build_file_edit_tool();
    let capability = edit_tool
        .request_streaming
        .as_ref()
        .expect("file_edit streaming capability");
    let factory = capability
        .parser_factory
        .as_ref()
        .expect("file_edit parser factory");
    let mut parser = factory.create(ToolRequestParserContext {
        request_stream_id: String::from("request-1"),
        tool_name: String::from(FILE_EDIT_TOOL_ID),
    });
    push_string(&mut *parser, "path", "src/example.rs");
    push_string(&mut *parser, "old_string", "fn old() {\r\n}\n\r");
    push_string(&mut *parser, "new_string", "fn new() {\n}\r");
    push_bool(&mut *parser, "replace_all", true);

    let preview = parser.snapshot().expect("completed fields should preview");
    assert_eq!(preview.kind, "file_edit");
    assert_eq!(preview.affected_resources, vec!["src/example.rs"]);
    assert_eq!(preview.progress_fields["old_line_count"], json!(2));
    assert_eq!(preview.progress_fields["new_line_count"], json!(2));
    assert_eq!(preview.progress_fields["replace_all"], json!(true));
    assert_eq!(preview.structured_lines.len(), 4);
    assert_eq!(preview.structured_lines[0].side, "old");
    assert_eq!(preview.structured_lines[2].side, "new");
}

/// Verifies character-fragmented CRLF and LFCR pairs count as one logical boundary.
/// 验证按字符分片的 CRLF 与 LFCR 配对只计为一个逻辑边界。
#[test]
fn file_edit_stream_preview_counts_split_paired_endings_once() {
    let edit_tool = build_file_edit_tool();
    let factory = edit_tool
        .request_streaming
        .as_ref()
        .and_then(|capability| capability.parser_factory.as_ref())
        .expect("file_edit parser factory");
    let mut parser = factory.create(ToolRequestParserContext {
        request_stream_id: String::from("request-split-endings"),
        tool_name: String::from(FILE_EDIT_TOOL_ID),
    });
    push_string_fragments(&mut *parser, "path", "src/example.rs");
    push_string_fragments(&mut *parser, "old_string", "a\r\nb\n\rc");
    push_string_fragments(&mut *parser, "new_string", "x\n\ry\r\nz");

    let preview = parser
        .snapshot()
        .expect("fragmented strings should preview");
    assert_eq!(preview.progress_fields["old_line_count"], json!(3));
    assert_eq!(preview.progress_fields["new_line_count"], json!(3));
    assert_eq!(preview.structured_lines.len(), 6);
}

/// Verifies preview bodies and structured rows enforce their independent hard bounds.
/// 验证预览正文与结构化行会分别执行自身硬上限。
#[test]
fn file_edit_stream_preview_is_bounded_and_marks_truncation() {
    let edit_tool = build_file_edit_tool();
    let capability = edit_tool
        .request_streaming
        .as_ref()
        .expect("file_edit streaming capability");
    let factory = capability
        .parser_factory
        .as_ref()
        .expect("file_edit parser factory");
    let mut parser = factory.create(ToolRequestParserContext {
        request_stream_id: String::from("request-preview-limit"),
        tool_name: String::from(FILE_EDIT_TOOL_ID),
    });
    let long_text = (0..3_000)
        .map(|index| format!("line-{index:04}-{}\n", "x".repeat(32)))
        .collect::<String>();
    push_string(&mut *parser, "path", "src/example.rs");
    push_string(&mut *parser, "old_string", &long_text);

    let preview = parser.snapshot().expect("long string should preview");
    assert!(
        preview.body_preview.as_deref().unwrap_or_default().len()
            <= capability.sensitivity_policy.maximum_body_preview_bytes
    );
    assert!(
        preview.structured_lines.len()
            <= capability
                .sensitivity_policy
                .maximum_structured_preview_lines
    );
    assert!(
        preview
            .structured_lines
            .iter()
            .map(|line| line.content.len())
            .sum::<usize>()
            <= capability
                .sensitivity_policy
                .maximum_structured_preview_bytes
    );
    assert!(
        preview
            .redaction_flags
            .iter()
            .any(|flag| flag == "body_preview_truncated")
    );
    assert!(
        preview
            .redaction_flags
            .iter()
            .any(|flag| flag == "structured_preview_truncated")
    );
}

/// Verifies authoritative final arguments replace preview state instead of causing execution failure.
/// 验证权威最终参数替换预览状态而不是导致执行失败。
#[test]
fn file_edit_stream_finish_is_non_authoritative() {
    let edit_tool = build_file_edit_tool();
    let factory = edit_tool
        .request_streaming
        .as_ref()
        .and_then(|capability| capability.parser_factory.as_ref())
        .expect("file_edit parser factory");
    let mut parser = factory.create(ToolRequestParserContext {
        request_stream_id: String::from("request-final"),
        tool_name: String::from(FILE_EDIT_TOOL_ID),
    });
    push_string(&mut *parser, "path", "src/a.rs");
    push_string(&mut *parser, "old_string", "old");
    push_string(&mut *parser, "new_string", "preview");
    let final_preview = parser
        .finish(&json!({
            "path": "src/a.rs",
            "old_string": "old",
            "new_string": "authoritative"
        }))
        .expect("preview consumer must not reject final arguments")
        .expect("final preview");
    assert!(
        final_preview
            .body_preview
            .as_deref()
            .unwrap_or_default()
            .contains("+ authoritative")
    );
}

/// Verifies exact-text preview lines remain unmapped until the path is known.
/// 验证精确文本预览行在路径已知前保持未映射。
#[test]
fn file_edit_stream_preview_waits_for_path_mapping() {
    let edit_tool = build_file_edit_tool();
    let factory = edit_tool
        .request_streaming
        .as_ref()
        .and_then(|capability| capability.parser_factory.as_ref())
        .expect("file_edit parser factory");
    let mut parser = factory.create(ToolRequestParserContext {
        request_stream_id: String::from("request-path-order"),
        tool_name: String::from(FILE_EDIT_TOOL_ID),
    });
    push_string(&mut *parser, "old_string", "return 1;");
    let incomplete = parser.snapshot().expect("text should preview");
    assert!(incomplete.affected_resources.is_empty());

    push_string(&mut *parser, "path", "src/example.rs");
    let complete = parser.snapshot().expect("path should refresh preview");
    assert_eq!(complete.affected_resources, vec!["src/example.rs"]);
    assert_eq!(complete.structured_lines.len(), 1);
}

/// Verifies streamed Apply Patch previews map multiple files and diff sides.
/// 验证流式 Apply Patch 预览映射多文件与 Diff 两侧。
#[test]
fn apply_patch_stream_preview_maps_multiple_files() {
    let patch_tool = build_apply_patch_tool();
    let factory = patch_tool
        .request_streaming
        .as_ref()
        .and_then(|capability| capability.parser_factory.as_ref())
        .expect("apply_patch parser factory");
    let mut parser = factory.create(ToolRequestParserContext {
        request_stream_id: String::from("request-patch"),
        tool_name: String::from(APPLY_PATCH_TOOL_ID),
    });
    push_string(
        &mut *parser,
        "patch",
        "*** Begin Patch\n*** Update File: a.rs\n-old\n+new\n*** Add File: b.rs\n+created\n*** End Patch\n",
    );
    let preview = parser.snapshot().expect("patch should preview");
    assert_eq!(preview.affected_resources, vec!["a.rs", "b.rs"]);
    assert_eq!(preview.progress_fields["old_line_count"], json!(1));
    assert_eq!(preview.progress_fields["new_line_count"], json!(2));
    assert_eq!(preview.structured_lines.len(), 3);
    assert_eq!(preview.structured_lines[0].operation_index, 0);
    assert_eq!(preview.structured_lines[0].resource_index, 0);
    assert_eq!(preview.structured_lines[2].operation_index, 1);
    assert_eq!(preview.structured_lines[2].resource_index, 1);
    assert_eq!(preview.structured_lines[2].line_index, 0);
}

/// Verifies repeated paths keep distinct operation identities while sharing one resource.
/// 验证重复路径保留独立操作标识，同时共享同一资源映射。
#[test]
fn apply_patch_stream_preview_separates_operation_and_resource_indexes() {
    let patch_tool = build_apply_patch_tool();
    let factory = patch_tool
        .request_streaming
        .as_ref()
        .and_then(|capability| capability.parser_factory.as_ref())
        .expect("apply_patch parser factory");
    let mut parser = factory.create(ToolRequestParserContext {
        request_stream_id: String::from("request-repeated-path"),
        tool_name: String::from(APPLY_PATCH_TOOL_ID),
    });
    push_string(
        &mut *parser,
        "patch",
        "*** Begin Patch\n*** Update File: a.rs\n-old\n+new\n*** Update File: a.rs\n-new\n+newer\n*** End Patch\n",
    );
    let preview = parser.snapshot().expect("patch should preview");

    assert_eq!(preview.affected_resources, vec!["a.rs"]);
    assert_eq!(preview.title_fields["operation_count"], json!(2));
    assert_eq!(preview.structured_lines[0].operation_index, 0);
    assert_eq!(preview.structured_lines[0].resource_index, 0);
    assert_eq!(preview.structured_lines[2].operation_index, 1);
    assert_eq!(preview.structured_lines[2].resource_index, 0);
    assert_eq!(preview.structured_lines[2].line_index, 0);
}

/// Verifies Apply Patch context rows advance both sides without zero-index underflow.
/// 验证 Apply Patch 上下文行同时推进新旧两侧，并且不会发生零索引下溢。
#[test]
fn apply_patch_stream_preview_indexes_leading_context_rows() {
    let patch_tool = build_apply_patch_tool();
    let factory = patch_tool
        .request_streaming
        .as_ref()
        .and_then(|capability| capability.parser_factory.as_ref())
        .expect("apply_patch parser factory");
    let mut parser = factory.create(ToolRequestParserContext {
        request_stream_id: String::from("request-leading-context"),
        tool_name: String::from(APPLY_PATCH_TOOL_ID),
    });
    push_string(
        &mut *parser,
        "patch",
        "*** Begin Patch\n*** Update File: a.rs\n context\n-old\n+new\n trailing\n*** End Patch\n",
    );
    let preview = parser.snapshot().expect("patch should preview");

    assert_eq!(preview.structured_lines.len(), 4);
    assert_eq!(preview.structured_lines[0].side, "context");
    assert_eq!(preview.structured_lines[0].line_index, 0);
    assert_eq!(preview.structured_lines[1].side, "old");
    assert_eq!(preview.structured_lines[1].line_index, 1);
    assert_eq!(preview.structured_lines[2].side, "new");
    assert_eq!(preview.structured_lines[2].line_index, 1);
    assert_eq!(preview.structured_lines[3].side, "context");
    assert_eq!(preview.structured_lines[3].line_index, 2);
    assert_eq!(preview.progress_fields["old_line_count"], json!(1));
    assert_eq!(preview.progress_fields["new_line_count"], json!(1));
}

/// Verifies Apply Patch preview retention is bounded independently from final assembly.
/// 验证 Apply Patch 预览保留独立于最终参数累计并受到硬上限约束。
#[test]
fn apply_patch_stream_preview_is_bounded_and_marks_truncation() {
    let patch_tool = build_apply_patch_tool();
    let capability = patch_tool
        .request_streaming
        .as_ref()
        .expect("apply_patch streaming capability");
    let factory = capability
        .parser_factory
        .as_ref()
        .expect("apply_patch parser factory");
    let mut parser = factory.create(ToolRequestParserContext {
        request_stream_id: String::from("request-patch-limit"),
        tool_name: String::from(APPLY_PATCH_TOOL_ID),
    });
    let patch = format!(
        "*** Begin Patch\n*** Add File: large.txt\n+{}\n*** End Patch\n",
        "x".repeat(96 * 1024)
    );
    push_string(&mut *parser, "patch", &patch);

    let preview = parser.snapshot().expect("large patch should preview");
    assert!(
        preview.body_preview.as_deref().unwrap_or_default().len()
            <= capability.sensitivity_policy.maximum_body_preview_bytes
    );
    assert!(
        preview
            .structured_lines
            .iter()
            .map(|line| line.content.len())
            .sum::<usize>()
            <= capability
                .sensitivity_policy
                .maximum_structured_preview_bytes
    );
    assert!(
        preview
            .redaction_flags
            .iter()
            .any(|flag| flag == "body_preview_truncated")
    );
    assert!(
        preview
            .redaction_flags
            .iter()
            .any(|flag| flag == "structured_preview_truncated")
    );
}

/// Tests approved write counters before the content string closes; returns nothing.
/// 验证正文字符串闭合前的获准写入计数；不返回值。
#[test]
fn file_write_request_preview_counts_partial_content_and_field_order() {
    // Registered capability must be reachable through the actual native tool definition.
    // 注册能力必须能通过实际原生工具定义访问。
    let tool = build_file_tool();
    let factory = tool
        .request_streaming
        .as_ref()
        .unwrap()
        .parser_factory
        .as_ref()
        .unwrap();
    let mut parser = factory.create(ToolRequestParserContext {
        request_stream_id: "write-preview".into(),
        tool_name: FILE_TOOL_ID.into(),
    });
    push_string(&mut *parser, "action", "write");
    push_string(&mut *parser, "path", "example.html");
    // Mixed logical endings are deliberately split across decoded fragments.
    // 混合逻辑换行被刻意拆到不同解码碎片。
    let path = vec![IncrementalJsonPathSegment::Key("content".into())];
    parser
        .push_json_event(&IncrementalJsonEvent::StringStarted { path: path.clone() })
        .unwrap();
    for (content, lines) in [("one\r", 1), ("\ntwo\n", 2), ("\rthree", 3)] {
        parser
            .push_json_event(&IncrementalJsonEvent::StringContent {
                path: path.clone(),
                content: content.into(),
            })
            .unwrap();
        let preview = parser.snapshot().unwrap();
        assert_eq!(preview.progress_fields["new_line_count"], json!(lines));
        assert_eq!(preview.title_fields["path"], json!("example.html"));
        assert!(preview.body_preview.is_none());
        assert!(preview.structured_lines.is_empty());
    }
    parser.abort();
    assert!(parser.snapshot().is_none());
    // Content may precede action/path, but must never appear as a read preview.
    // 正文可以早于动作及路径，但绝不能作为读取预览出现。
    push_string_fragments(&mut *parser, "content", "a\r\nb\n\rc");
    push_string(&mut *parser, "path", "other.txt");
    assert!(parser.snapshot().is_none());
    push_string(&mut *parser, "action", "write");
    assert_eq!(
        parser.snapshot().unwrap().progress_fields["new_line_count"],
        json!(3)
    );
    let final_preview = parser
        .finish(&json!({"action":"write","path":"other.txt","content":"done\n"}))
        .unwrap()
        .unwrap();
    assert_eq!(final_preview.progress_fields["new_line_count"], json!(1));
    assert!(
        parser
            .finish(&json!({"action":"read","path":"other.txt"}))
            .unwrap()
            .is_none()
    );
}

/// Emits one completed top-level JSON string event.
/// 发出一个已完成的顶层 JSON 字符串事件。
fn push_string(parser: &mut dyn ToolRequestParserSession, field: &str, value: &str) {
    let path = vec![IncrementalJsonPathSegment::Key(String::from(field))];
    parser
        .push_json_event(&IncrementalJsonEvent::StringStarted { path: path.clone() })
        .expect("string start");
    parser
        .push_json_event(&IncrementalJsonEvent::StringContent {
            path: path.clone(),
            content: String::from(value),
        })
        .expect("string content");
    parser
        .push_json_event(&IncrementalJsonEvent::StringEnded { path })
        .expect("string end");
}

/// Emits one top-level JSON string as character-sized decoded fragments.
/// 将一个顶层 JSON 字符串按字符大小的已解码分片发出。
fn push_string_fragments(parser: &mut dyn ToolRequestParserSession, field: &str, value: &str) {
    let path = vec![IncrementalJsonPathSegment::Key(String::from(field))];
    parser
        .push_json_event(&IncrementalJsonEvent::StringStarted { path: path.clone() })
        .expect("string start");
    for character in value.chars() {
        parser
            .push_json_event(&IncrementalJsonEvent::StringContent {
                path: path.clone(),
                content: character.to_string(),
            })
            .expect("string fragment");
    }
    parser
        .push_json_event(&IncrementalJsonEvent::StringEnded { path })
        .expect("string end");
}

/// Emits one completed top-level JSON boolean event.
/// 发出一个已完成的顶层 JSON 布尔事件。
fn push_bool(parser: &mut dyn ToolRequestParserSession, field: &str, value: bool) {
    parser
        .push_json_event(&IncrementalJsonEvent::ScalarCompleted {
            path: vec![IncrementalJsonPathSegment::Key(String::from(field))],
            value: IncrementalJsonScalar::Boolean(value),
        })
        .expect("boolean scalar event");
}
