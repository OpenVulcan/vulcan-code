use crate::tooling::native::{
    FILE_EDIT_TOOL_ID, FILE_TOOL_ID, build_file_edit_tool, build_file_tool,
};
use serde_json::json;
use vulcan_tools_manager::{
    IncrementalJsonEvent, IncrementalJsonPathSegment, IncrementalJsonScalar,
    ToolRequestParserContext, ToolRequestParserSession, VULCAN_TOOL_ERROR_DETAIL_KIND,
};

use super::test_native_request_with_workspace;

/// Verifies the native file tools expose exactly the intended visibility and schema boundaries.
/// 验证原生文件工具暴露出预期的可见性与 Schema 边界。
#[test]
fn native_file_tools_expose_contracts() {
    let file = build_file_tool();
    assert_eq!(file.descriptor.id, FILE_TOOL_ID);
    assert!(file.descriptor.ai_visible);
    assert!(file.descriptor.available_in_plan);
    assert_eq!(file.args_schema["additionalProperties"], json!(false));
    assert_eq!(
        file.args_schema["properties"]["action"]["enum"],
        json!(["read", "list", "search", "stat"])
    );
    assert_eq!(
        file.args_schema["properties"]["targets"]["maxItems"],
        json!(10)
    );
    assert!(file.args_schema.get("oneOf").is_none());
    assert!(file.args_schema.get("allOf").is_none());

    let edit = build_file_edit_tool();
    assert_eq!(edit.descriptor.id, FILE_EDIT_TOOL_ID);
    assert!(edit.descriptor.ai_visible);
    assert!(!edit.descriptor.available_in_plan);
    assert_eq!(edit.args_schema["additionalProperties"], json!(false));
    assert_eq!(
        edit.args_schema["properties"]["operations"]["items"]["additionalProperties"],
        json!(false)
    );
    assert!(
        edit.args_schema["properties"]["operations"]["items"]
            .get("oneOf")
            .is_none()
    );
    assert!(
        edit.args_schema["properties"]["operations"]["items"]
            .get("allOf")
            .is_none()
    );
    assert!(
        edit.args_schema["properties"]["operations"]["description"]
            .as_str()
            .unwrap_or_default()
            .contains("delete_file")
    );
    assert!(edit.args_schema.get("allOf").is_none());
    assert!(edit.request_streaming.is_some());
}

/// Verifies native file-edit failures retain machine-readable host error details.
/// 验证原生 file_edit 失败时仍保留机器可读的宿主错误详情。
#[tokio::test]
async fn native_file_edit_errors_are_structured() {
    let edit = build_file_edit_tool();
    let result = edit
        .invoke(test_native_request_with_workspace(
            json!({"operations": []}),
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

/// Verifies streamed previews contain complete array-line changes and reject final drift.
/// 验证流式预览包含完整数组行变更，并拒绝最终参数漂移。
#[test]
fn file_edit_stream_preview_requires_final_consistency() {
    let edit_tool = build_file_edit_tool();
    let capability = edit_tool
        .request_streaming
        .as_ref()
        .expect("file_edit streaming capability");
    let factory = capability
        .parser_factory
        .as_ref()
        .expect("file_edit parser factory");
    let context = ToolRequestParserContext {
        request_stream_id: String::from("request-1"),
        tool_name: String::from(FILE_EDIT_TOOL_ID),
    };
    let mut parser = factory.create(context);
    push_string(&mut *parser, "id", "change-a");
    push_string(&mut *parser, "operation", "replace");
    push_string(&mut *parser, "path", "src/example.rs");
    push_string_at(&mut *parser, "old_lines", 0, "return 1;");
    push_string_at(&mut *parser, "new_lines", 0, "return 2;");

    let preview = parser.snapshot().expect("completed fields should preview");
    assert_eq!(preview.kind, "file_edit");
    assert_eq!(preview.affected_resources, vec!["src/example.rs"]);
    assert!(
        preview
            .body_preview
            .as_deref()
            .unwrap_or_default()
            .contains("- return 1;")
    );
    assert!(
        preview
            .body_preview
            .as_deref()
            .unwrap_or_default()
            .contains("+ return 2;")
    );
    assert_eq!(preview.structured_lines.len(), 2);
    assert_eq!(preview.structured_lines[0].side, "old");
    assert_eq!(preview.structured_lines[0].operation_index, 0);
    assert_eq!(preview.structured_lines[0].resource_index, 0);
    assert_eq!(preview.structured_lines[1].side, "new");
    let first_revision = preview.revision;
    push_string_at(&mut *parser, "new_lines", 1, "return 3;");
    let next_preview = parser
        .snapshot()
        .expect("a later completed line should preview");
    assert!(next_preview.revision > first_revision);

    let arguments = json!({
        "operations": [{
            "id": "change-a",
            "operation": "replace",
            "path": "src/example.rs",
            "old_lines": ["return 1;"],
            "new_lines": ["return 2;", "return 3;"]
        }]
    });
    assert!(
        parser
            .finish(&arguments)
            .expect("final consistency check")
            .is_some()
    );

    let drifted = json!({
        "operations": [{
            "id": "change-a",
            "operation": "replace",
            "path": "src/example.rs",
            "old_lines": ["return 1;"],
            "new_lines": ["return 2;", "return 4;"]
        }]
    });
    assert!(parser.finish(&drifted).is_err());
}

/// Verifies a streamed move preview rejects destination drift before execution.
/// 验证流式移动预览会在执行前拒绝目标路径漂移。
#[test]
fn file_edit_stream_preview_checks_move_destination() {
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
        request_stream_id: String::from("request-move"),
        tool_name: String::from(FILE_EDIT_TOOL_ID),
    });
    push_string(&mut *parser, "id", "move-a");
    push_string(&mut *parser, "operation", "move_file");
    push_string(&mut *parser, "path", "src/a.rs");
    push_string(&mut *parser, "destination", "src/b.rs");
    push_string(&mut *parser, "expected_version", "opaque");

    let arguments = json!({
        "operations": [{
            "id": "move-a",
            "operation": "move_file",
            "path": "src/a.rs",
            "destination": "src/b.rs",
            "expected_version": "opaque"
        }]
    });
    assert!(
        parser
            .finish(&arguments)
            .expect("final consistency check")
            .is_some()
    );

    let drifted = json!({
        "operations": [{
            "id": "move-a",
            "operation": "move_file",
            "path": "src/a.rs",
            "destination": "src/c.rs",
            "expected_version": "opaque"
        }]
    });
    assert!(parser.finish(&drifted).is_err());
}

/// Verifies streamed edit previews reject changes to matching and commit semantics.
/// 验证流式编辑预览会拒绝定位和提交语义发生变化的最终参数。
#[test]
fn file_edit_stream_preview_checks_semantic_options() {
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
        request_stream_id: String::from("request-options"),
        tool_name: String::from(FILE_EDIT_TOOL_ID),
    });
    push_bool(&mut *parser, "dry_run", true);
    push_string(&mut *parser, "id", "change-a");
    push_string(&mut *parser, "operation", "replace");
    push_string(&mut *parser, "path", "src/example.rs");
    push_string(&mut *parser, "match_mode", "ignore_trailing_whitespace");
    push_string(&mut *parser, "expected_version", "version-a");
    push_string(&mut *parser, "final_newline", "absent");
    push_string_at(&mut *parser, "old_lines", 0, "return 1;");
    push_string_at(&mut *parser, "new_lines", 0, "return 2;");

    let arguments = json!({
        "dry_run": true,
        "operations": [{
            "id": "change-a",
            "operation": "replace",
            "path": "src/example.rs",
            "match_mode": "ignore_trailing_whitespace",
            "expected_version": "version-a",
            "final_newline": "absent",
            "old_lines": ["return 1;"],
            "new_lines": ["return 2;"]
        }]
    });
    assert!(
        parser
            .finish(&arguments)
            .expect("matching options")
            .is_some()
    );

    let mut mode_drift = arguments.clone();
    mode_drift["operations"][0]["match_mode"] = json!("exact");
    assert!(parser.finish(&mode_drift).is_err());

    let mut version_drift = arguments.clone();
    version_drift["operations"][0]["expected_version"] = json!("version-b");
    assert!(parser.finish(&version_drift).is_err());

    let mut newline_drift = arguments.clone();
    newline_drift["operations"][0]["final_newline"] = json!("present");
    assert!(parser.finish(&newline_drift).is_err());

    let mut drifted = arguments.clone();
    drifted["dry_run"] = json!(false);
    assert!(parser.finish(&drifted).is_err());
}

/// Verifies incomplete paths cannot map structured lines to an unrelated file.
/// 验证路径尚未完成时结构化行不会映射到无关文件。
#[test]
fn file_edit_stream_preview_waits_for_path_before_structured_mapping() {
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
        request_stream_id: String::from("request-path-order"),
        tool_name: String::from(FILE_EDIT_TOOL_ID),
    });
    push_string(&mut *parser, "id", "change-a");
    push_string(&mut *parser, "operation", "replace");
    push_string_at(&mut *parser, "old_lines", 0, "return 1;");

    let incomplete = parser.snapshot().expect("line completion should preview");
    assert!(incomplete.affected_resources.is_empty());
    assert!(incomplete.structured_lines.is_empty());

    push_string(&mut *parser, "path", "src/example.rs");
    let complete = parser
        .snapshot()
        .expect("path completion should refresh preview");
    assert_eq!(complete.affected_resources, vec!["src/example.rs"]);
    assert_eq!(complete.structured_lines.len(), 1);
    assert_eq!(complete.structured_lines[0].resource_index, 0);
}

/// Emits one completed scalar JSON string event for an operation field.
/// 为操作字段发出一个已完成的 JSON 字符串事件。
fn push_string(parser: &mut dyn ToolRequestParserSession, field: &str, value: &str) {
    push_string_at(parser, field, 0, value);
}

/// Emits one completed JSON string event at an operation line-array index.
/// 在操作行数组索引处发出一个已完成的 JSON 字符串事件。
fn push_string_at(
    parser: &mut dyn ToolRequestParserSession,
    field: &str,
    line_index: usize,
    value: &str,
) {
    let mut path = vec![IncrementalJsonPathSegment::Key(String::from("operations"))];
    path.push(IncrementalJsonPathSegment::Index(0));
    path.push(IncrementalJsonPathSegment::Key(String::from(field)));
    if field == "old_lines" || field == "new_lines" || field == "anchors" {
        path.push(IncrementalJsonPathSegment::Index(line_index));
    }
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

/// Emits one completed JSON boolean event at the top-level request path.
/// 在顶层请求路径发出一个已完成的 JSON 布尔值事件。
fn push_bool(parser: &mut dyn ToolRequestParserSession, field: &str, value: bool) {
    parser
        .push_json_event(&IncrementalJsonEvent::ScalarCompleted {
            path: vec![IncrementalJsonPathSegment::Key(String::from(field))],
            value: IncrementalJsonScalar::Boolean(value),
        })
        .expect("boolean scalar event");
}
