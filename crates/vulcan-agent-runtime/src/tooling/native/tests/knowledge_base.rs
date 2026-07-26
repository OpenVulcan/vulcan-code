use super::*;

/// Verifies empty authoritative documents retain the imported one-line contract.
/// 验证空权威文档保留导入阶段的一行契约。
#[test]
fn knowledge_document_lines_preserve_empty_document_line() {
    assert_eq!(knowledge_document_lines(""), vec![""]);
    assert_eq!(
        knowledge_document_lines("first\nsecond\n"),
        vec!["first", "second"]
    );
}
use crate::tooling::native::NativeToolDefinition;

// knowledge_tools returns the complete read-only knowledge tool family.
// knowledge_tools 返回完整的只读知识库工具族。
fn knowledge_tools() -> Vec<Arc<NativeToolDefinition>> {
    vec![
        build_knowledge_search_tool(),
        build_knowledge_browse_tool(),
        build_knowledge_read_tool(),
    ]
}

/// Verifies every knowledge tool is AI-visible and allowed in Plan mode because the family is read-only.
/// 验证每个知识库工具均对 AI 可见且允许在计划模式使用，因为该工具族只读。
#[test]
fn knowledge_tools_are_read_only_plan_tools() {
    for tool in knowledge_tools() {
        assert!(
            tool.descriptor.ai_visible,
            "{} must be AI-visible",
            tool.descriptor.id
        );
        assert!(
            tool.descriptor.available_in_plan,
            "{} must be available in Plan mode",
            tool.descriptor.id
        );
        assert!(
            tool.descriptor.description.is_ascii(),
            "{} model-facing description must be English ASCII",
            tool.descriptor.id
        );
    }
}

/// Verifies strict provider schemas reject undeclared fields and expose no management operation.
/// 验证严格 Provider Schema 拒绝未声明字段且不暴露任何管理操作。
#[test]
fn knowledge_tool_schemas_are_strict_and_read_only() {
    for tool in knowledge_tools() {
        assert_eq!(tool.args_schema["type"], "object");
        assert_eq!(tool.args_schema["additionalProperties"], false);
        let schema = tool.args_schema.to_string();
        assert!(!schema.contains("sourceRoot"));
        assert!(!schema.contains("databasePath"));
        assert!(!schema.contains("sql"));
    }
}

/// Verifies a direct knowledge invocation without a turn access snapshot fails closed.
/// 验证缺少回合访问快照的直接知识库调用会失败关闭。
#[tokio::test]
async fn knowledge_tool_without_access_snapshot_fails_closed() {
    let error = build_knowledge_search_tool()
        .invoke(test_native_request(json!({ "queries": ["Jieba BM25"] })))
        .await
        .expect_err("knowledge search must require a turn access snapshot");

    assert!(
        error
            .message
            .contains("not enabled for this conversation turn")
    );
}

/// Verifies invalid batch cardinality is rejected before any storage access.
/// 验证无效批次数量会在访问任何存储前被拒绝。
#[tokio::test]
async fn knowledge_search_rejects_empty_query_batch() {
    let error = build_knowledge_search_tool()
        .invoke(test_native_request(json!({ "queries": [] })))
        .await
        .expect_err("empty query batches must be rejected");

    assert!(error.message.contains("requires between 1 and"));
}
