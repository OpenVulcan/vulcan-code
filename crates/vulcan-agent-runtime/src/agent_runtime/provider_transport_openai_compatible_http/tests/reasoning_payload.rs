use super::*;

/// Verifies the direct OpenAI-compatible request payload replays `reasoning_content` when the selected model explicitly declares a textual interleaved reasoning field.
/// 验证当所选模型显式声明文本型交错推理字段时，直连 OpenAI 兼容请求载荷会回放 `reasoning_content`。
#[test]
fn request_payload_replays_reasoning_content_and_preserves_tool_shapes() {
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: CoreProviderRequestSnapshot {
            provider_id: String::from("openai"),
            model_id: String::from("gpt-test"),
            system_prompt: String::from("system prompt"),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::from("system prompt").clone(),
            ),
            prompt: String::from("tool result body"),
            history: Vec::new(),
            temperature: Some(0.2),
            max_turns: 8,
            tool_schemas: Vec::new(),
            additional_params: Some(json!({
                "reasoning": { "effort": "high" }
            })),
            request_headers: None,
            continuation: CoreProviderContinuation::default(),
        },
        current_history: vec![CoreConversationMessage {
            id: String::from("assistant-1"),
            payload: CoreConversationPayload::Assistant {
                text: String::from("我先读取文件。"),
                reasoning: String::from("这段思考不应回放。"),
                provider_message_id: Some(String::from("resp-1")),
                provider_message_state: None,
                tool_calls: vec![CoreConversationToolCall {
                    id: String::from("tool-1"),
                    tool_call_id: Some(String::from("call-1")),
                    tool_name: String::from("read_file"),
                    arguments: json!({ "path": "src/main.rs" }),
                }],
                usage: None,
            },
        }],
        current_prompt: CoreConversationMessage {
            id: String::from("tool-result-1"),
            payload: CoreConversationPayload::ToolResult {
                tool_call_id: Some(String::from("call-1")),
                text: String::from("tool result body"),
            },
        },
        interleaved_reasoning: Some(ModelInterleavedCapability::Field(ModelInterleavedField {
            field: String::from("reasoning_content"),
        })),
    };

    let payload = build_openai_compatible_request_payload(
        "gpt-test",
        request,
        &[ProviderToolDefinition {
            name: String::from("read_file"),
            description: String::from("Read one file"),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            }),
        }],
        true,
    )
    .expect("payload should build");

    let messages = payload["messages"]
        .as_array()
        .expect("messages should be an array");
    assert_eq!(messages[0]["role"], "system");
    assert_eq!(messages[1]["role"], "assistant");
    assert_eq!(messages[1]["reasoning_content"], "这段思考不应回放。");
    assert_eq!(messages[1]["tool_calls"][0]["id"], "call-1");
    assert_eq!(
        messages[1]["tool_calls"][0]["function"]["name"],
        "read_file"
    );
    assert_eq!(
        messages[1]["tool_calls"][0]["function"]["arguments"],
        "{\"path\":\"src/main.rs\"}"
    );
    assert_eq!(messages[2]["role"], "tool");
    assert_eq!(messages[2]["tool_call_id"], "call-1");
    assert_eq!(messages[2]["content"], "tool result body");
    assert_eq!(payload["stream"], true);
    assert_eq!(payload["reasoning"]["effort"], "high");
    assert_eq!(payload["tools"][0]["function"]["name"], "read_file");
}

/// Verifies provider message replay does not synthesize a duplicate `reasoning_content` field when one native reasoning field is already preserved.
/// 验证当原生思考字段已经被保留时，Provider 消息回放不会再额外合成重复的 `reasoning_content` 字段。
#[test]
fn provider_message_state_avoids_duplicate_textual_reasoning_field() {
    let state = build_openai_compatible_provider_message_state(
        "已完成。",
        &[],
        Some(String::from("先检查目录。\n再汇总结果。")),
        JsonMap::from_iter([(
            String::from("reasoning_details"),
            json!([
                { "type": "summary", "text": "先检查目录。" },
                { "type": "summary", "text": "再汇总结果。" }
            ]),
        )]),
        "openai",
        "gpt-test",
    )
    .expect("provider message state should exist");

    let assistant_message =
        parse_openai_compatible_provider_message_state(&state, "openai", "gpt-test")
            .expect("assistant message should be one object");
    assert!(assistant_message.get("reasoning_content").is_none());
    assert_eq!(assistant_message["content"], json!("已完成。"));
    assert_eq!(
        assistant_message["reasoning_details"][0]["text"],
        json!("先检查目录。")
    );
}

/// Verifies OpenAI-compatible request replay preserves an explicitly empty `reasoning_content` field so DeepSeek-style thinking sessions can continue after tool calls.
/// 验证 OpenAI 兼容请求回放会保留显式为空的 `reasoning_content` 字段，以便 DeepSeek 风格的 thinking 会话在工具调用后继续执行。
#[test]
fn request_payload_preserves_empty_reasoning_content_when_interleaved() {
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: CoreProviderRequestSnapshot {
            provider_id: String::from("deepseek"),
            model_id: String::from("deepseek-v4-pro"),
            system_prompt: String::from("system prompt"),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::from("system prompt").clone(),
            ),
            prompt: String::from("tool result body"),
            history: Vec::new(),
            temperature: None,
            max_turns: 8,
            tool_schemas: Vec::new(),
            additional_params: Some(json!({})),
            request_headers: None,
            continuation: CoreProviderContinuation::default(),
        },
        current_history: vec![CoreConversationMessage {
            id: String::from("assistant-1"),
            payload: CoreConversationPayload::Assistant {
                text: String::from("先调用工具。"),
                reasoning: String::new(),
                provider_message_id: Some(String::from("resp-1")),
                provider_message_state: None,
                tool_calls: vec![CoreConversationToolCall {
                    id: String::from("tool-1"),
                    tool_call_id: Some(String::from("call-1")),
                    tool_name: String::from("read_file"),
                    arguments: json!({ "path": "src/main.rs" }),
                }],
                usage: None,
            },
        }],
        current_prompt: CoreConversationMessage {
            id: String::from("tool-result-1"),
            payload: CoreConversationPayload::ToolResult {
                tool_call_id: Some(String::from("call-1")),
                text: String::from("tool result body"),
            },
        },
        interleaved_reasoning: Some(ModelInterleavedCapability::Field(ModelInterleavedField {
            field: String::from("reasoning_content"),
        })),
    };

    let payload = build_openai_compatible_request_payload(
        "deepseek-v4-pro",
        request,
        &[ProviderToolDefinition {
            name: String::from("read_file"),
            description: String::from("Read one file"),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            }),
        }],
        true,
    )
    .expect("payload should build");

    let messages = payload["messages"]
        .as_array()
        .expect("messages should be an array");
    assert_eq!(messages[1]["role"], "assistant");
    assert_eq!(messages[1]["reasoning_content"], "");
}

/// Verifies OpenAI-compatible request replay injects an empty `content` field for reasoning-only assistant history so strict providers such as DeepSeek do not reject the assistant message as structurally incomplete.
/// 验证 OpenAI 兼容请求回放会为纯 reasoning 的助手历史注入空 `content` 字段，从而避免 DeepSeek 这类严格 Provider 因助手消息结构不完整而拒绝请求。
#[test]
fn request_payload_keeps_reasoning_only_assistant_history_provider_valid() {
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: CoreProviderRequestSnapshot {
            provider_id: String::from("deepseek"),
            model_id: String::from("deepseek-v4-pro"),
            system_prompt: String::from("system prompt"),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::from("system prompt").clone(),
            ),
            prompt: String::from("继续"),
            history: Vec::new(),
            temperature: None,
            max_turns: 8,
            tool_schemas: Vec::new(),
            additional_params: Some(json!({})),
            request_headers: None,
            continuation: CoreProviderContinuation::default(),
        },
        current_history: vec![CoreConversationMessage {
            id: String::from("assistant-reasoning-only-1"),
            payload: CoreConversationPayload::Assistant {
                text: String::new(),
                reasoning: String::from("继续保持思考上下文。"),
                provider_message_id: Some(String::from("resp-1")),
                provider_message_state: None,
                tool_calls: Vec::new(),
                usage: None,
            },
        }],
        current_prompt: CoreConversationMessage {
            id: String::from("user-1"),
            payload: CoreConversationPayload::UserText {
                text: String::from("继续"),
                attachments: Vec::new(),
            },
        },
        interleaved_reasoning: Some(ModelInterleavedCapability::Field(ModelInterleavedField {
            field: String::from("reasoning_content"),
        })),
    };

    let payload = build_openai_compatible_request_payload("deepseek-v4-pro", request, &[], true)
        .expect("payload should build");

    let messages = payload["messages"]
        .as_array()
        .expect("messages should be an array");
    assert_eq!(messages[1]["role"], "assistant");
    assert_eq!(messages[1]["content"], "");
    assert_eq!(messages[1]["reasoning_content"], "继续保持思考上下文。");
}

/// Verifies replaying one persisted OpenAI-compatible provider message state with reasoning-only assistant fields still injects empty `content` during request rebuild so previously stored sessions remain provider-valid.
/// 验证回放一条只含 reasoning 助手字段的已持久化 OpenAI 兼容 provider message state 时，请求重建仍会注入空 `content`，从而保证旧会话依然满足 Provider 要求。
#[test]
fn request_payload_repairs_reasoning_only_provider_message_state() {
    let assistant_state = json!({
        "protocol": "openai-compatible",
        "assistant_message": {
            "role": "assistant",
            "reasoning_content": "继续保持思考上下文。"
        }
    });
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: CoreProviderRequestSnapshot {
            provider_id: String::from("deepseek"),
            model_id: String::from("deepseek-v4-pro"),
            system_prompt: String::from("system prompt"),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::from("system prompt").clone(),
            ),
            prompt: String::from("继续"),
            history: Vec::new(),
            temperature: None,
            max_turns: 8,
            tool_schemas: Vec::new(),
            additional_params: Some(json!({})),
            request_headers: None,
            continuation: CoreProviderContinuation::default(),
        },
        current_history: vec![CoreConversationMessage {
            id: String::from("assistant-reasoning-only-state-1"),
            payload: CoreConversationPayload::Assistant {
                text: String::new(),
                reasoning: String::new(),
                provider_message_id: Some(String::from("resp-1")),
                provider_message_state: Some(assistant_state),
                tool_calls: Vec::new(),
                usage: None,
            },
        }],
        current_prompt: CoreConversationMessage {
            id: String::from("user-1"),
            payload: CoreConversationPayload::UserText {
                text: String::from("继续"),
                attachments: Vec::new(),
            },
        },
        interleaved_reasoning: Some(ModelInterleavedCapability::Field(ModelInterleavedField {
            field: String::from("reasoning_content"),
        })),
    };

    let payload = build_openai_compatible_request_payload("deepseek-v4-pro", request, &[], true)
        .expect("payload should build");

    let messages = payload["messages"]
        .as_array()
        .expect("messages should be an array");
    assert_eq!(messages[1]["role"], "assistant");
    assert_eq!(messages[1]["content"], "");
    assert_eq!(messages[1]["reasoning_content"], "继续保持思考上下文。");
}

/// Verifies OpenAI-compatible request replay restores raw provider-visible interleaved fields from provider_message_state when the active model declares a matching field-based capability.
/// 验证当当前模型声明匹配的字段型交错能力时，OpenAI 兼容请求回放会从 provider_message_state 恢复原生 Provider 可见交错字段。
#[test]
fn request_payload_replays_reasoning_details_from_provider_message_state() {
    let assistant_state = json!({
        "protocol": "openai-compatible",
        "assistant_message": {
            "role": "assistant",
            "content": "先总结已有上下文。",
            "reasoning_details": [
                { "type": "summary", "text": "已读取主要文件结构。" }
            ],
            "tool_calls": [
                {
                    "id": "tool-1",
                    "type": "function",
                    "function": {
                        "name": "read_file",
                        "arguments": "{\"path\":\"src/main.rs\"}"
                    }
                }
            ]
        }
    });
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: CoreProviderRequestSnapshot {
            provider_id: String::from("openai-compatible"),
            model_id: String::from("qwen-test"),
            system_prompt: String::from("system prompt"),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::from("system prompt").clone(),
            ),
            prompt: String::from("tool result body"),
            history: Vec::new(),
            temperature: None,
            max_turns: 8,
            tool_schemas: Vec::new(),
            additional_params: None,
            request_headers: None,
            continuation: CoreProviderContinuation::default(),
        },
        current_history: vec![CoreConversationMessage {
            id: String::from("assistant-1"),
            payload: CoreConversationPayload::Assistant {
                text: String::from("先总结已有上下文。"),
                reasoning: String::new(),
                provider_message_id: Some(String::from("resp-1")),
                provider_message_state: Some(assistant_state),
                tool_calls: vec![CoreConversationToolCall {
                    id: String::from("tool-1"),
                    tool_call_id: Some(String::from("call-1")),
                    tool_name: String::from("read_file"),
                    arguments: json!({ "path": "src/main.rs" }),
                }],
                usage: None,
            },
        }],
        current_prompt: CoreConversationMessage {
            id: String::from("tool-result-1"),
            payload: CoreConversationPayload::ToolResult {
                tool_call_id: Some(String::from("call-1")),
                text: String::from("tool result body"),
            },
        },
        interleaved_reasoning: Some(ModelInterleavedCapability::Field(ModelInterleavedField {
            field: String::from("reasoning_details"),
        })),
    };

    let payload = build_openai_compatible_request_payload(
        "qwen-test",
        request,
        &[ProviderToolDefinition {
            name: String::from("read_file"),
            description: String::from("Read one file"),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            }),
        }],
        true,
    )
    .expect("payload should build");

    let messages = payload["messages"]
        .as_array()
        .expect("messages should be an array");
    assert_eq!(messages[1]["role"], "assistant");
    assert_eq!(
        messages[1]["reasoning_details"][0]["text"],
        "已读取主要文件结构。"
    );
    assert_eq!(messages[1]["tool_calls"][0]["id"], "call-1");
}

/// Verifies OpenAI-compatible request replay restores provider-native interleaved fields for boolean-style capabilities and ignores them when the active model does not declare interleaved support.
/// 验证 OpenAI 兼容请求回放会为布尔型交错能力恢复 Provider 原生交错字段，并在当前模型未声明交错支持时忽略这些字段。
#[test]
fn request_payload_replays_or_ignores_provider_message_state_based_on_interleaved_capability() {
    let assistant_state = json!({
        "protocol": "openai-compatible",
        "assistant_message": {
            "role": "assistant",
            "content": "先做规划。",
            "reasoning_details": [
                { "type": "summary", "text": "这是隐藏规划。" }
            ]
        }
    });
    let assistant_message = CoreConversationMessage {
        id: String::from("assistant-1"),
        payload: CoreConversationPayload::Assistant {
            text: String::from("先做规划。"),
            reasoning: String::new(),
            provider_message_id: Some(String::from("resp-1")),
            provider_message_state: Some(assistant_state),
            tool_calls: Vec::new(),
            usage: None,
        },
    };
    let current_prompt = CoreConversationMessage {
        id: String::from("user-1"),
        payload: CoreConversationPayload::UserText {
            text: String::from("继续"),
            attachments: Vec::new(),
        },
    };

    let payload_with_interleaved = build_openai_compatible_request_payload(
        "qwen-test",
        OpenAiCompatibleCoreTurnRequest {
            snapshot: CoreProviderRequestSnapshot {
                provider_id: String::from("openai-compatible"),
                model_id: String::from("qwen-test"),
                system_prompt: String::new(),
                prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                    String::new().clone(),
                ),
                prompt: String::from("继续"),
                history: Vec::new(),
                temperature: None,
                max_turns: 4,
                tool_schemas: Vec::new(),
                additional_params: None,
                request_headers: None,
                continuation: CoreProviderContinuation::default(),
            },
            current_history: vec![assistant_message.clone()],
            current_prompt: current_prompt.clone(),
            interleaved_reasoning: Some(ModelInterleavedCapability::Enabled(true)),
        },
        &[],
        true,
    )
    .expect("payload should build");
    let payload_without_interleaved = build_openai_compatible_request_payload(
        "qwen-test",
        OpenAiCompatibleCoreTurnRequest {
            snapshot: CoreProviderRequestSnapshot {
                provider_id: String::from("openai-compatible"),
                model_id: String::from("qwen-test"),
                system_prompt: String::new(),
                prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                    String::new().clone(),
                ),
                prompt: String::from("继续"),
                history: Vec::new(),
                temperature: None,
                max_turns: 4,
                tool_schemas: Vec::new(),
                additional_params: None,
                request_headers: None,
                continuation: CoreProviderContinuation::default(),
            },
            current_history: vec![assistant_message],
            current_prompt,
            interleaved_reasoning: None,
        },
        &[],
        true,
    )
    .expect("payload should build");

    let with_messages = payload_with_interleaved["messages"]
        .as_array()
        .expect("messages should be an array");
    assert_eq!(
        with_messages[0]["reasoning_details"][0]["text"],
        "这是隐藏规划。"
    );

    let without_messages = payload_without_interleaved["messages"]
        .as_array()
        .expect("messages should be an array");
    assert!(without_messages[0].get("reasoning_details").is_none());
    assert_eq!(without_messages[0]["content"], "先做规划。");
}

/// Verifies scoped OpenAI-compatible provider state is ignored after provider or model switches.
/// 验证带作用域的 OpenAI 兼容 Provider 状态在切换 Provider 或模型后会被忽略。
#[test]
fn request_payload_ignores_scoped_openai_compatible_state_after_provider_switch() {
    let assistant_state = build_openai_compatible_provider_message_state(
        "先做规划。",
        &[],
        None,
        JsonMap::from_iter([(
            String::from("reasoning_details"),
            json!([{ "type": "summary", "text": "MiniMax 内部规划。" }]),
        )]),
        "minimax-cn-coding-plan",
        "minimax-m3",
    )
    .expect("provider message state should exist");
    let assistant_message = CoreConversationMessage {
        id: String::from("assistant-1"),
        payload: CoreConversationPayload::Assistant {
            text: String::from("先做规划。"),
            reasoning: String::new(),
            provider_message_id: Some(String::from("resp-1")),
            provider_message_state: Some(assistant_state),
            tool_calls: Vec::new(),
            usage: None,
        },
    };
    let current_prompt = CoreConversationMessage {
        id: String::from("user-1"),
        payload: CoreConversationPayload::UserText {
            text: String::from("继续"),
            attachments: Vec::new(),
        },
    };

    let payload = build_openai_compatible_request_payload(
        "qwen-test",
        OpenAiCompatibleCoreTurnRequest {
            snapshot: CoreProviderRequestSnapshot {
                provider_id: String::from("openai-compatible"),
                model_id: String::from("qwen-test"),
                system_prompt: String::new(),
                prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                    String::new().clone(),
                ),
                prompt: String::from("继续"),
                history: Vec::new(),
                temperature: None,
                max_turns: 4,
                tool_schemas: Vec::new(),
                additional_params: None,
                request_headers: None,
                continuation: CoreProviderContinuation::default(),
            },
            current_history: vec![assistant_message],
            current_prompt,
            interleaved_reasoning: Some(ModelInterleavedCapability::Enabled(true)),
        },
        &[],
        true,
    )
    .expect("payload should build");

    let messages = payload["messages"]
        .as_array()
        .expect("messages should be an array");
    assert!(messages[0].get("reasoning_details").is_none());
    assert_eq!(messages[0]["content"], "先做规划。");
}
