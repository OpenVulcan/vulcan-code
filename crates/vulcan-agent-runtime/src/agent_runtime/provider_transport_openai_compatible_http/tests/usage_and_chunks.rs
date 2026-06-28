use super::*;

/// Verifies OpenAI-compatible streaming payloads explicitly request the terminal usage chunk while preserving caller-supplied stream-options fields.
/// 验证 OpenAI 兼容流式载荷会显式请求终态 usage 块，同时保留调用方自带的 stream-options 字段。
#[test]
fn request_payload_requests_stream_usage_without_overwriting_other_options() {
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: CoreProviderRequestSnapshot {
            provider_id: String::from("deepseek"),
            model_id: String::from("deepseek-v4-pro"),
            system_prompt: String::new(),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::new().clone(),
            ),
            prompt: String::from("hello"),
            history: Vec::new(),
            temperature: None,
            max_turns: 4,
            tool_schemas: Vec::new(),
            additional_params: Some(json!({
                "stream_options": {
                    "foo": "bar"
                }
            })),
            request_headers: None,
            continuation: CoreProviderContinuation::default(),
        },
        current_history: Vec::new(),
        current_prompt: CoreConversationMessage {
            id: String::from("user-1"),
            payload: CoreConversationPayload::UserText {
                text: String::from("hello"),
                attachments: Vec::new(),
            },
        },
        interleaved_reasoning: None,
    };

    let payload = build_openai_compatible_request_payload("deepseek-v4-pro", request, &[], true)
        .expect("payload should build");

    assert_eq!(payload["stream"], true);
    assert_eq!(payload["stream_options"]["include_usage"], true);
    assert_eq!(payload["stream_options"]["foo"], "bar");
}

/// Verifies providers whose catalog rules do not request `stream_options.include_usage` keep caller payloads unchanged while still using provider-declared terminal usage sources.
/// 验证目录规则未请求 `stream_options.include_usage` 的供应商会保持调用方载荷不变，同时仍使用供应商声明的终态 usage 来源。
#[test]
fn request_payload_skips_stream_usage_flag_when_provider_rule_disables_it() {
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: CoreProviderRequestSnapshot {
            provider_id: String::from("openrouter"),
            model_id: String::from("openai/gpt-5"),
            system_prompt: String::new(),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::new().clone(),
            ),
            prompt: String::from("hello"),
            history: Vec::new(),
            temperature: None,
            max_turns: 4,
            tool_schemas: Vec::new(),
            additional_params: Some(json!({
                "stream_options": {
                    "foo": "bar"
                }
            })),
            request_headers: None,
            continuation: CoreProviderContinuation::default(),
        },
        current_history: Vec::new(),
        current_prompt: CoreConversationMessage {
            id: String::from("user-1"),
            payload: CoreConversationPayload::UserText {
                text: String::from("hello"),
                attachments: Vec::new(),
            },
        },
        interleaved_reasoning: None,
    };

    let payload = build_openai_compatible_request_payload("openai/gpt-5", request, &[], true)
        .expect("payload should build");

    assert_eq!(payload["stream"], true);
    assert!(payload["stream_options"].get("include_usage").is_none());
    assert_eq!(payload["stream_options"]["foo"], "bar");
}

/// Verifies OpenAI-compatible chunk normalization can extract terminal usage nested inside one completed streamed choice, which matches Kimi-style terminal chunks.
/// 验证 OpenAI 兼容块规范化可以提取嵌套在已完成流式 choice 内部的终态 usage，这与 Kimi 风格终态块一致。
#[test]
fn normalize_chunk_extracts_terminal_choice_usage_from_provider_rule() {
    let chunk = normalize_openai_compatible_chunk(
        "moonshotai",
        "Moonshot AI",
        r#"{
            "id":"resp_1",
            "choices":[
                {
                    "delta":{"content":"done"},
                    "finish_reason":"stop",
                    "usage":{
                        "prompt_tokens":128,
                        "completion_tokens":32,
                        "total_tokens":160
                    }
                }
            ]
        }"#,
    )
    .expect("chunk should parse");

    let usage = chunk
        .usage
        .expect("terminal choice usage should be captured");
    assert_eq!(usage.input_tokens, Some(128));
    assert_eq!(usage.output_tokens, Some(32));
    assert_eq!(usage.total_tokens, Some(160));
}

/// Verifies OpenRouter-style OpenAI-compatible chunks map the plaintext `reasoning` alias into visible thinking output and raw replay state.
/// 验证 OpenRouter 风格 OpenAI 兼容块会把纯文本 `reasoning` 别名映射到可见思考输出与原始回放状态。
#[test]
fn normalize_chunk_reads_reasoning_alias_and_preserves_replay_field() {
    let chunk = normalize_openai_compatible_chunk(
        "openrouter",
        "OpenRouter",
        r#"{
            "id":"resp_1",
            "choices":[
                {
                    "delta":{
                        "reasoning":"先检查目录。",
                        "content":"已完成。"
                    },
                    "finish_reason":"stop"
                }
            ]
        }"#,
    )
    .expect("chunk should parse");

    let choice = chunk.choice.expect("choice should exist");
    assert_eq!(choice.reasoning.as_deref(), Some("先检查目录。"));
    assert_eq!(choice.text.as_deref(), Some("已完成。"));
    assert_eq!(
        choice.provider_visible_assistant_fields["reasoning"],
        json!("先检查目录。")
    );
}

/// Verifies OpenRouter-style structured `reasoning_details` payloads flatten into visible thinking text while remaining intact for native replay.
/// 验证 OpenRouter 风格结构化 `reasoning_details` 载荷会压平成可见思考文本，同时保持原样供原生回放使用。
#[test]
fn normalize_chunk_flattens_reasoning_details_and_preserves_structured_payload() {
    let chunk = normalize_openai_compatible_chunk(
        "openrouter",
        "OpenRouter",
        r#"{
            "id":"resp_1",
            "choices":[
                {
                    "delta":{
                        "reasoning_details":[
                            { "type":"summary", "text":"先检查目录。" },
                            {
                                "type":"summary",
                                "summary":[
                                    { "text":"再汇总结果。" }
                                ]
                            }
                        ]
                    },
                    "finish_reason":"stop"
                }
            ]
        }"#,
    )
    .expect("chunk should parse");

    let choice = chunk.choice.expect("choice should exist");
    assert_eq!(
        choice.reasoning.as_deref(),
        Some("先检查目录。\n再汇总结果。")
    );
    assert_eq!(
        choice.provider_visible_assistant_fields["reasoning_details"][1]["summary"][0]["text"],
        json!("再汇总结果。")
    );
}
