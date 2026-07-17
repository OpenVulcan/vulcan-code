use super::*;

/// Verifies a configured non-Claude output limit reaches the final Anthropic max_tokens payload field.
/// 验证已配置的非 Claude 输出限制能够到达最终 Anthropic max_tokens 载荷字段。
#[test]
fn configured_output_limit_reaches_anthropic_payload() {
    // snapshot carries the already resolved model output limit as a protocol request parameter.
    // snapshot 将已解析的模型输出限制作为协议请求参数携带。
    let mut snapshot = build_test_request_snapshot("请生成长文件");
    snapshot.model_id = String::from("deepseek-v4-pro");
    snapshot.additional_params = Some(json!({ "max_tokens": 32_000 }));
    // request preserves the exact snapshot through the Anthropic payload builder.
    // request 在 Anthropic 载荷构建器中保留准确快照。
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot,
        current_history: Vec::new(),
        current_prompt: CoreConversationMessage {
            id: String::from("user-long-file"),
            payload: CoreConversationPayload::UserText {
                text: String::from("请生成长文件"),
                attachments: Vec::new(),
            },
        },
        interleaved_reasoning: None,
    };

    // payload is the final JSON body sent to the Anthropic-compatible endpoint.
    // payload 是发送给 Anthropic 兼容端点的最终 JSON Body。
    let payload = build_anthropic_request_payload("deepseek-v4-pro", request, &[])
        .expect("anthropic-compatible payload should build");

    assert_eq!(payload["max_tokens"], json!(32_000));
}
