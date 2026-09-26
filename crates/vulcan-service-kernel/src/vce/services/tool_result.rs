//! Conversation-only code context rendering; management API responses retain their typed JSON contract.
//! 仅供会话的代码上下文渲染；管理接口响应保留强类型 JSON 契约。

use vulcan_service_api::VceRetrievalResponse;
use vulcan_tools_manager::result_markdown::{fenced_code, inline_code};

/// Renders selected source hits once, preserving source positions and actual retrieval degradations.
/// 将选定源码命中渲染一次，保留来源位置与真实检索降级。
/// Accepts the exact service response and returns only the context needed by the conversation model.
/// 接受精确服务响应，仅返回会话模型需要的上下文。
pub(super) fn markdown(response: &VceRetrievalResponse) -> String {
    // Hits and formatted_context are two renderings of the same selected content; use hits only.
    // 命中与 formatted_context 是同一选中内容的两种呈现；仅使用命中。
    let mut output = String::from("# Code context\n");
    if response.hits.is_empty() {
        output.push_str("\nNo code context found.\n");
    }
    for hit in &response.hits {
        output.push_str(&format!(
            "\n## {} · lines {}-{}\n",
            inline_code(&hit.path),
            hit.start_line,
            hit.end_line,
        ));
        if hit.start_char_offset > 0 {
            output.push_str(&format!(
                "\nStart character offset: {}\n",
                hit.start_char_offset
            ));
        }
        output.push_str(&format!("\n{}\n", fenced_code(&hit.content)));
    }
    if !response.degradations.is_empty() {
        output.push_str("\n## Retrieval warnings\n");
        for warning in &response.degradations {
            output.push_str(&format!("\n- {warning}\n"));
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulcan_service_api::VceRetrievalHit;

    /// Verifies the model receives source text once without losing a partial-line offset or degradation.
    /// 验证模型只接收一次源码，同时不丢失部分行偏移和降级信息。
    #[test]
    fn code_context_omits_the_second_content_projection() {
        // The fixture repeats source in the service response to exercise the actual duplication boundary.
        // 夹具在服务响应中重复源码，以覆盖真实重复边界。
        let response = VceRetrievalResponse {
            project_id: String::from("project"),
            request_id: String::from("internal-request"),
            settings_revision: 1,
            checkpoint_version: 1,
            intent: String::from("implementation"),
            queries: vec![String::from("query")],
            hits: vec![VceRetrievalHit {
                id: String::from("occurrence"),
                blob_name: String::from("internal-blob"),
                path: String::from("src/main.rs"),
                start_line: 3,
                end_line: 3,
                start_char_offset: 5,
                content: String::from("unique-source-fragment"),
                score: 0.8,
                source: String::from("lexical"),
                kind: String::from("function"),
            }],
            formatted_context: String::from("unique-source-fragment"),
            degraded_features: vec![String::from("rerank")],
            degradations: vec![String::from("Rerank unavailable")],
        };
        // Compare the actual serialized service payload with the conversation projection.
        // 将真实序列化服务载荷与会话投影比较。
        let before = serde_json::to_string(&response).expect("service JSON");
        let body = markdown(&response);
        assert_eq!(body.matches("unique-source-fragment").count(), 1);
        assert!(body.contains("Start character offset: 5"));
        assert!(body.contains("lines 3-3"));
        assert!(body.contains("Rerank unavailable"));
        assert!(!body.contains("internal-blob"));
        assert!(body.len() < before.len());
    }
}
