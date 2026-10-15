//! Regression coverage for final structured-output and disabled-reasoning contracts.
//! 最终结构化输出与关闭思考契约的回归覆盖。

use super::super::test_support::{
    bundled_manifest_route, deepseek_route, minimal_request, pinned_openrouter_route,
};
use super::*;
use std::sync::Arc;
use vulcan_llm_core::ProtocolRegistry;

/// Verifies every schema-capable protocol retains one exact structured-output schema.
/// 验证每个支持 Schema 的协议都会保留同一份精确结构化输出 Schema。
#[test]
fn encode_request_preserves_structured_output_schema_for_supported_protocols() {
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    let schema = json!({
        "type": "object",
        "properties": { "accepted": { "type": "boolean" } },
        "required": ["accepted"],
        "additionalProperties": false,
    });

    for protocol_id in [
        OPENAI_CHAT_PROFILE_ID,
        OPENAI_RESPONSES_PROFILE_ID,
        ANTHROPIC_MESSAGES_PROFILE_ID,
        GEMINI_GENERATE_CONTENT_PROFILE_ID,
        GOOGLE_CODE_ASSIST_PROFILE_ID,
        GEMINI_INTERACTIONS_PROFILE_ID,
    ] {
        let mut request = minimal_request();
        request.structured_output = StructuredOutputPolicy::JsonSchema {
            name: String::from("result"),
            description: Some(String::from("Structured result")),
            schema: schema.clone(),
            strict: true,
        };
        // route reuses one valid provider shell while selecting the exact protocol under test.
        // route 复用一个有效供应商外壳，同时选择当前测试的准确协议。
        let mut route = pinned_openrouter_route(OPENAI_CHAT_PROFILE_ID);
        route.protocol_route.protocol_profile_id = protocol_id.to_owned();
        let encoded = client
            .encode_request(&route, &request)
            .expect("supported protocol should preserve structured output schema");
        let projected_schema = match protocol_id {
            OPENAI_CHAT_PROFILE_ID => encoded.body.pointer("/response_format/json_schema/schema"),
            OPENAI_RESPONSES_PROFILE_ID => encoded.body.pointer("/text/format/schema"),
            ANTHROPIC_MESSAGES_PROFILE_ID => encoded.body.pointer("/output_config/format/schema"),
            GEMINI_GENERATE_CONTENT_PROFILE_ID => {
                encoded.body.pointer("/generationConfig/responseJsonSchema")
            }
            GOOGLE_CODE_ASSIST_PROFILE_ID => encoded
                .body
                .pointer("/request/generationConfig/responseJsonSchema"),
            GEMINI_INTERACTIONS_PROFILE_ID => encoded.body.pointer("/response_format/schema"),
            _ => unreachable!("the loop only contains supported protocol profiles"),
        };
        assert_eq!(projected_schema, Some(&schema));
    }
}

/// Verifies every JSON-object protocol retains its native wire control after all projections.
/// 验证每个 JSON Object 协议在全部投影后仍保留其原生线路控制项。
#[test]
fn encode_request_preserves_json_object_mode_for_supported_protocols() {
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );

    for protocol_id in [
        OPENAI_CHAT_PROFILE_ID,
        OPENAI_RESPONSES_PROFILE_ID,
        GEMINI_GENERATE_CONTENT_PROFILE_ID,
        GOOGLE_CODE_ASSIST_PROFILE_ID,
        GEMINI_INTERACTIONS_PROFILE_ID,
    ] {
        let mut request = minimal_request();
        request.structured_output = StructuredOutputPolicy::JsonObject;
        // route reuses one valid provider shell while selecting the exact protocol under test.
        // route 复用一个有效供应商外壳，同时选择当前测试的准确协议。
        let mut route = pinned_openrouter_route(OPENAI_CHAT_PROFILE_ID);
        route.protocol_route.protocol_profile_id = protocol_id.to_owned();
        let encoded = client
            .encode_request(&route, &request)
            .expect("supported protocol should preserve JSON Object mode");
        let projected_type = match protocol_id {
            OPENAI_CHAT_PROFILE_ID => encoded
                .body
                .pointer("/response_format/type")
                .and_then(Value::as_str),
            OPENAI_RESPONSES_PROFILE_ID => encoded
                .body
                .pointer("/text/format/type")
                .and_then(Value::as_str),
            GEMINI_GENERATE_CONTENT_PROFILE_ID => encoded
                .body
                .pointer("/generationConfig/responseMimeType")
                .and_then(Value::as_str),
            GOOGLE_CODE_ASSIST_PROFILE_ID => encoded
                .body
                .pointer("/request/generationConfig/responseMimeType")
                .and_then(Value::as_str),
            GEMINI_INTERACTIONS_PROFILE_ID => encoded
                .body
                .pointer("/response_format/mime_type")
                .and_then(Value::as_str),
            _ => unreachable!("the loop only contains supported protocol profiles"),
        };
        let expected_type = if matches!(
            protocol_id,
            GEMINI_GENERATE_CONTENT_PROFILE_ID
                | GOOGLE_CODE_ASSIST_PROFILE_ID
                | GEMINI_INTERACTIONS_PROFILE_ID
        ) {
            "application/json"
        } else {
            "json_object"
        };
        assert_eq!(projected_type, Some(expected_type));
    }
}

/// Verifies a late provider projection cannot silently remove JSON Object mode.
/// 验证后置供应商投影不能静默移除 JSON Object 模式。
#[test]
fn encode_request_rejects_json_object_mode_loss_after_custom_projection() {
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    let mut route = pinned_openrouter_route(OPENAI_RESPONSES_PROFILE_ID);
    route.custom_model_projection = Some(CustomModelRequestProjection {
        delete: vec![String::from("text")],
        ..Default::default()
    });
    let mut request = minimal_request();
    request.structured_output = StructuredOutputPolicy::JsonObject;

    let error = client
        .encode_request(&route, &request)
        .expect_err("JSON Object mode loss must fail before dispatch");
    assert!(error.error.message.contains("JSON Object mode"));
}

/// Verifies the exact DeepSeek Responses route used by native VMM sends JSON mode with reasoning off.
/// 验证原生 VMM 使用的准确 DeepSeek Responses 路由会发送 JSON 模式并关闭思考。
#[test]
fn deepseek_responses_preserves_vmm_json_and_disabled_reasoning_controls() {
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    let mut request = minimal_request();
    request.structured_output = StructuredOutputPolicy::JsonObject;
    request.reasoning_effort = Some(ReasoningEffort::None);

    let encoded = client
        .encode_required_disabled_reasoning_request(
            &deepseek_route(OPENAI_RESPONSES_PROFILE_ID),
            &request,
            false,
        )
        .expect("DeepSeek VMM request must preserve both mandatory controls");

    assert_eq!(
        encoded.body.pointer("/text/format/type"),
        Some(&json!("json_object"))
    );
    assert_eq!(
        encoded.body.pointer("/reasoning/effort"),
        Some(&json!("none"))
    );
}

/// Builds one DeepSeek Responses request whose assistant turn carries durable provider items.
/// 构建一个助手回合携带可持久化供应商项的 DeepSeek Responses 请求。
fn deepseek_replay_request(provider_context_items: Vec<Value>) -> AiExecutionRequest {
    let mut request = minimal_request();
    request.model_id = String::from("deepseek-flash");
    request.messages.push(vulcan_ai_contract::AiMessage {
        role: vulcan_ai_contract::MessageRole::Assistant,
        content: vec![ContentItem::Text {
            text: String::from("durable-answer"),
        }],
        provider_context_items,
    });
    request
}

/// Verifies the DeepSeek Responses wire body replays the exact ordered reasoning items of a route.
/// 验证 DeepSeek Responses 线路请求体按序回放该路由的精确推理原始项。
#[test]
fn deepseek_responses_replays_original_reasoning_into_final_wire_body() {
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    // originalItems are the exact provider output items a durable V1 replay envelope retains.
    // originalItems 是可持久化 V1 重放封套保留的精确供应商输出项。
    let original_items = vec![
        json!({
            "type": "reasoning",
            "content": [{"type": "reasoning_text", "text": "durable-reasoning"}]
        }),
        json!({
            "type": "function_call",
            "call_id": "call-1",
            "name": "read_file",
            "arguments": "{}"
        }),
    ];

    let encoded = client
        .encode_request(
            &deepseek_route(OPENAI_RESPONSES_PROFILE_ID),
            &deepseek_replay_request(original_items.clone()),
        )
        .expect("the exact DeepSeek Responses route must encode");

    let input = encoded
        .body
        .pointer("/input")
        .and_then(Value::as_array)
        .expect("the Responses body must carry an ordered input array");
    assert!(
        input.contains(&original_items[0]),
        "the original reasoning item must reach the final wire body, not just the envelope: {input:#?}"
    );
    assert!(
        input.contains(&original_items[1]),
        "the original tool call item must reach the final wire body: {input:#?}"
    );
    // Original order is part of the contract, not an incidental detail: this endpoint merges
    // adjacent items, so a reordered replay would silently change the conversation the model sees.
    // 原始顺序是契约的一部分而非偶然细节：该端点会归并相邻项，
    // 因此重排后的回放会静默改变模型看到的对话。
    let positions = original_items
        .iter()
        .map(|item| {
            input
                .iter()
                .position(|candidate| candidate == item)
                .unwrap_or_else(|| {
                    panic!("every replayed item must reach the final wire body in order: {item:#?}")
                })
        })
        .collect::<Vec<_>>();
    assert!(
        positions.windows(2).all(|pair| pair[0] < pair[1]),
        "replayed items must keep their original relative order: positions {positions:?} in {input:#?}"
    );
    // A plaintext reasoning item must arrive as content, because this endpoint ignores summaries.
    // 明文推理项必须以 content 抵达，因为该端点会忽略摘要。
    let reasoning = input
        .iter()
        .find(|item| item["type"] == "reasoning")
        .expect("the replayed reasoning item must be present");
    assert!(
        reasoning["content"]
            .as_array()
            .is_some_and(|content| !content.is_empty()),
        "the replayed reasoning item must carry a non-empty plaintext content array: {reasoning:#?}"
    );
}

/// Builds one DeepSeek Chat request whose assistant turn carries durable provider items.
/// 构建一个助手回合携带可持久化供应商项的 DeepSeek Chat 请求。
fn deepseek_chat_replay_request(provider_context_items: Vec<Value>) -> AiExecutionRequest {
    let mut request = minimal_request();
    request.model_id = String::from("deepseek-chat");
    request.messages.push(vulcan_ai_contract::AiMessage {
        role: vulcan_ai_contract::MessageRole::Assistant,
        content: Vec::new(),
        provider_context_items,
    });
    request
}

/// Returns every assistant message of one projected Chat body.
/// 返回一份已投影 Chat 正文中的全部助手消息。
fn chat_assistant_messages(body: &Value) -> Vec<Value> {
    body["messages"]
        .as_array()
        .expect("the Chat body must carry an ordered messages array")
        .iter()
        .filter(|message| message["role"] == "assistant")
        .cloned()
        .collect()
}

/// Verifies the declared Chat route lands one recorded turn on exactly one assistant message.
/// 验证已声明 Chat 路由把一个已记录回合落在恰好一条助手消息上。
#[test]
fn deepseek_chat_replays_reasoning_body_and_tool_calls_on_one_assistant_message() {
    // recordedItems are the exact ordered items a durable V1 replay envelope retains for one turn.
    // recordedItems 是可持久化 V1 重放封套为一个回合保留的精确有序项。
    let recorded_items = vec![
        json!({
            "type": "reasoning",
            "content": [{"type": "reasoning_text", "text": "durable-reasoning"}]
        }),
        json!({
            "type": "message",
            "role": "assistant",
            "content": [{"type": "output_text", "text": "durable-answer"}]
        }),
        json!({
            "type": "function_call",
            "call_id": "call-1",
            "name": "read_file",
            "arguments": "{}"
        }),
    ];

    let encoded = encoding_client()
        .encode_request(
            &deepseek_route(OPENAI_CHAT_PROFILE_ID),
            &deepseek_chat_replay_request(recorded_items),
        )
        .expect("the declared DeepSeek Chat route must encode");

    let assistants = chat_assistant_messages(&encoded.body);
    assert_eq!(
        assistants.len(),
        1,
        "one recorded turn must reach the Chat wire body as exactly one assistant message: {assistants:#?}"
    );
    assert_eq!(assistants[0]["reasoning_content"], "durable-reasoning");
    assert_eq!(assistants[0]["content"], "durable-answer");
    assert_eq!(assistants[0]["tool_calls"][0]["id"], "call-1");
    assert_eq!(
        assistants[0]["tool_calls"][0]["function"]["name"],
        "read_file"
    );
}

/// Verifies the declared Chat route keeps the verified empty-body form for a tool-only turn.
/// 验证已声明 Chat 路由为仅含工具调用的回合保持已验证的空正文形态。
#[test]
fn deepseek_chat_uses_empty_body_for_recorded_tool_only_turn() {
    let recorded_items = vec![
        json!({
            "type": "reasoning",
            "content": [{"type": "reasoning_text", "text": "durable-reasoning"}]
        }),
        json!({
            "type": "function_call",
            "call_id": "call-1",
            "name": "read_file",
            "arguments": "{}"
        }),
    ];

    let encoded = encoding_client()
        .encode_request(
            &deepseek_route(OPENAI_CHAT_PROFILE_ID),
            &deepseek_chat_replay_request(recorded_items),
        )
        .expect("the declared DeepSeek Chat route must encode");

    let assistants = chat_assistant_messages(&encoded.body);
    assert_eq!(
        assistants.len(),
        1,
        "one turn stays one message: {assistants:#?}"
    );
    assert_eq!(assistants[0]["content"], "");
    assert_eq!(assistants[0]["reasoning_content"], "durable-reasoning");
    assert_eq!(assistants[0]["tool_calls"][0]["id"], "call-1");
}

/// Verifies a tool-carrying request replays every earlier reasoning turn, not only tool turns.
/// 验证携带工具的请求回放每个更早的推理回合，而不只是调用工具的回合。
#[test]
fn deepseek_chat_replays_every_prior_reasoning_turn_when_tools_are_present() {
    let mut request = minimal_request();
    request.model_id = String::from("deepseek-chat");
    // firstTurn reasons without calling a tool; only a whole-history replay can carry it forward.
    // firstTurn 进行了推理但没有调用工具；只有整段历史回放才能把它带下去。
    request.messages.push(vulcan_ai_contract::AiMessage {
        role: vulcan_ai_contract::MessageRole::Assistant,
        content: Vec::new(),
        provider_context_items: vec![json!({
            "type": "reasoning",
            "content": [{"type": "reasoning_text", "text": "first-reasoning"}]
        })],
    });
    request.messages.push(vulcan_ai_contract::AiMessage {
        role: vulcan_ai_contract::MessageRole::Assistant,
        content: Vec::new(),
        provider_context_items: vec![
            json!({
                "type": "reasoning",
                "content": [{"type": "reasoning_text", "text": "second-reasoning"}]
            }),
            json!({
                "type": "function_call",
                "call_id": "call-1",
                "name": "read_file",
                "arguments": "{}"
            }),
        ],
    });
    request.messages.push(vulcan_ai_contract::AiMessage {
        role: vulcan_ai_contract::MessageRole::Tool,
        content: vec![ContentItem::ToolResult {
            result: vulcan_ai_contract::ToolResult {
                call_id: String::from("call-1"),
                is_error: false,
                content: Value::String(String::from("file-body")),
            },
        }],
        provider_context_items: Vec::new(),
    });

    let encoded = encoding_client()
        .encode_request(&deepseek_route(OPENAI_CHAT_PROFILE_ID), &request)
        .expect("a multi-turn declared DeepSeek Chat request must encode");

    let assistants = chat_assistant_messages(&encoded.body);
    let reasonings = assistants
        .iter()
        .filter_map(|message| message.get("reasoning_content").and_then(Value::as_str))
        .collect::<Vec<_>>();
    assert_eq!(
        reasonings,
        vec!["first-reasoning", "second-reasoning"],
        "every prior reasoning turn must be replayed on its own message: {assistants:#?}"
    );
    // The tool-carrying turn keeps its own call, and the result follows that exact identity.
    // 调用工具的回合持有自身调用，且结果跟随该精确身份。
    let tool_turn = assistants
        .iter()
        .find(|message| message.get("tool_calls").is_some())
        .expect("the tool-carrying turn must keep its call");
    assert_eq!(tool_turn["reasoning_content"], "second-reasoning");
    assert_eq!(tool_turn["tool_calls"][0]["id"], "call-1");
    let messages = encoded.body["messages"].as_array().expect("messages");
    let tool_result = messages
        .iter()
        .find(|message| message["role"] == "tool")
        .expect("the tool result must reach the body");
    assert_eq!(tool_result["tool_call_id"], "call-1");
}

/// Verifies the declared Chat route refuses a recorded reasoning item it could never replay.
/// 验证已声明 Chat 路由拒绝其永远无法回放的已记录推理项。
#[test]
fn declared_deepseek_chat_route_rejects_unreplayable_reasoning_before_dispatch() {
    // encryptedOnly is a record whose text was never stored, so reasoning_content cannot carry it.
    // encryptedOnly 是一份从未存储文本的记录，因此 reasoning_content 无法承载它。
    let unreplayable = [
        json!({"type": "reasoning", "encrypted_content": "opaque"}),
        json!({"type": "reasoning", "content": [{"type": "reasoning_text", "text": ""}]}),
    ];

    for item in unreplayable {
        let outcome = encoding_client().encode_request(
            &deepseek_route(OPENAI_CHAT_PROFILE_ID),
            &deepseek_chat_replay_request(vec![item.clone()]),
        );
        let error = outcome.expect_err("an unreplayable record must fail before dispatch");
        assert_unreplayable_history(&error);
    }
}

/// Verifies an undeclared Chat route keeps its existing split projection byte for byte.
/// 验证未声明的 Chat 路由逐字节保持其既有拆分投影。
#[test]
fn undeclared_chat_route_never_merges_reasoning_into_a_tool_call_message() {
    let recorded_items = vec![
        json!({
            "type": "reasoning",
            "content": [{"type": "reasoning_text", "text": "durable-reasoning"}]
        }),
        json!({
            "type": "function_call",
            "call_id": "call-1",
            "name": "read_file",
            "arguments": "{}"
        }),
    ];

    let encoded = encoding_client()
        .encode_request(
            &pinned_openrouter_route(OPENAI_CHAT_PROFILE_ID),
            &deepseek_chat_replay_request(recorded_items),
        )
        .expect("the undeclared OpenRouter Chat route must encode");

    let assistants = chat_assistant_messages(&encoded.body);
    assert!(
        assistants.iter().all(|message| {
            message.get("reasoning_content").is_none() || message.get("tool_calls").is_none()
        }),
        "only a route declaring the contract may merge reasoning into a tool call message: {assistants:#?}"
    );
}

/// Verifies the declared Anthropic route replays the original signature instead of a bridge one.
/// 验证已声明 Anthropic 路由回放原始签名，而不是桥接签名。
#[test]
fn deepseek_anthropic_replays_original_signature_without_a_bridge_or_redaction() {
    let mut request = minimal_request();
    request.model_id = String::from("deepseek-chat");
    request.messages.push(vulcan_ai_contract::AiMessage {
        role: vulcan_ai_contract::MessageRole::Assistant,
        content: vec![
            ContentItem::Reasoning {
                text: String::from("recorded-thinking"),
                signature: Some(String::from("anthropic#original-payload")),
            },
            ContentItem::Text {
                text: String::from("recorded-body"),
            },
            ContentItem::ToolCall {
                call: vulcan_ai_contract::ToolCall {
                    id: String::from("call-1"),
                    name: String::from("read_file"),
                    arguments: json!({}),
                    signature: None,
                },
            },
        ],
        provider_context_items: Vec::new(),
    });

    let encoded = encoding_client()
        .encode_request(&deepseek_route(ANTHROPIC_MESSAGES_PROFILE_ID), &request)
        .expect("the declared DeepSeek Anthropic route must encode");

    let blocks = encoded.body["messages"]
        .as_array()
        .expect("Anthropic messages")
        .iter()
        .flat_map(|message| message["content"].as_array().cloned().unwrap_or_default())
        .collect::<Vec<Value>>();
    let kinds = blocks
        .iter()
        .filter_map(|block| block["type"].as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        vec!["text", "thinking", "text", "tool_use"],
        "thinking, body and tool_use must keep their order on the wire: {blocks:#?}"
    );
    let thinking = blocks
        .iter()
        .find(|block| block["type"] == "thinking")
        .expect("the recorded thinking block must be present");
    assert_eq!(thinking["thinking"], "recorded-thinking");
    // The bridge would have written `ccswitch-openai-reasoning-v1:` here.
    // 桥接会在这里写入 `ccswitch-openai-reasoning-v1:`。
    assert_eq!(thinking["signature"], "original-payload");
    assert!(
        blocks
            .iter()
            .all(|block| block["type"] != "redacted_thinking"),
        "a declared DeepSeek route must never emit redacted_thinking: {blocks:#?}"
    );
}

/// Verifies the declared Anthropic route replays recorded reasoning that carries no signature.
/// 验证已声明 Anthropic 路由回放不含签名的已记录推理。
#[test]
fn deepseek_anthropic_replays_signature_free_reasoning_as_native_thinking() {
    let mut request = minimal_request();
    request.model_id = String::from("deepseek-chat");
    request.messages.push(vulcan_ai_contract::AiMessage {
        role: vulcan_ai_contract::MessageRole::Assistant,
        content: vec![ContentItem::Reasoning {
            text: String::from("recorded-thinking"),
            signature: None,
        }],
        provider_context_items: Vec::new(),
    });

    let encoded = encoding_client()
        .encode_request(&deepseek_route(ANTHROPIC_MESSAGES_PROFILE_ID), &request)
        .expect("recorded reasoning without a signature must still be sendable");

    let thinking = encoded.body["messages"]
        .as_array()
        .expect("Anthropic messages")
        .iter()
        .flat_map(|message| message["content"].as_array().cloned().unwrap_or_default())
        .find(|block| block["type"] == "thinking")
        .expect("the recorded thinking block must be present");
    assert_eq!(thinking["thinking"], "recorded-thinking");
    assert!(
        thinking.get("signature").is_none(),
        "an absent signature must be omitted, never invented: {thinking:#?}"
    );
}

/// Verifies an undeclared Anthropic route keeps its existing bridge projection byte for byte.
/// 验证未声明的 Anthropic 路由逐字节保持其既有桥接投影。
#[test]
fn undeclared_anthropic_route_keeps_the_bridge_signature() {
    let mut request = minimal_request();
    request.model_id = String::from("deepseek-chat");
    request.messages.push(vulcan_ai_contract::AiMessage {
        role: vulcan_ai_contract::MessageRole::Assistant,
        content: Vec::new(),
        provider_context_items: vec![json!({
            "type": "reasoning",
            "summary": [{"type": "summary_text", "text": "recorded-thinking"}],
            "encrypted_content": "opaque"
        })],
    });

    let encoded = encoding_client()
        .encode_request(
            &pinned_openrouter_route(ANTHROPIC_MESSAGES_PROFILE_ID),
            &request,
        )
        .expect("the undeclared Anthropic route must encode");

    let signature = encoded.body["messages"]
        .as_array()
        .expect("Anthropic messages")
        .iter()
        .flat_map(|message| message["content"].as_array().cloned().unwrap_or_default())
        .find_map(|block| block["signature"].as_str().map(str::to_string))
        .expect("the undeclared route must keep its existing bridge signature");
    assert!(
        signature.starts_with("ccswitch-openai-reasoning-v1:"),
        "only a route declaring the contract may replace the bridge: {signature}"
    );
}

/// Verifies a legacy session whose reasoning was stored without text fails with the fixed copy.
/// 验证推理存储时不含文本的旧会话以固定文案失败。
///
/// # Remarks
/// 说明
/// The plan names this compatibility consequence explicitly: a session persisted with only an
/// opaque continuation cannot be replayed, and it must fail visibly instead of resuming without its
/// reasoning or being misreported as a new regression. The plan also names the tool-carrying shape
/// separately, because that is exactly when DeepSeek requires the whole prior reasoning back, so
/// both shapes are exercised on every declared route.
/// 计划明确列出这一兼容后果：仅以不透明延续持久化的会话无法回放，
/// 必须可见失败，而不是在丢失推理的情况下继续，或被误报为修复后的新回归。
/// 计划还单独点出「带工具」的形态，因为这正是 DeepSeek 要求回传全部先前推理的场合，
/// 因此两种形态都在每条已声明路由上检验。
#[test]
fn legacy_session_without_replayable_reasoning_fails_on_every_declared_route() {
    for protocol_id in [
        OPENAI_RESPONSES_PROFILE_ID,
        OPENAI_CHAT_PROFILE_ID,
        ANTHROPIC_MESSAGES_PROFILE_ID,
    ] {
        // tool_carrying selects the exact shape the plan calls out: a session that advertised tools.
        // tool_carrying 选择计划点出的精确形态：公布过工具的会话。
        for tool_carrying in [false, true] {
            let mut request = minimal_request();
            request.model_id = String::from("deepseek-chat");
            if tool_carrying {
                request.tools.push(vulcan_ai_contract::ToolDefinition {
                    name: String::from("read_file"),
                    description: String::from("Read one file"),
                    input_schema: json!({"type": "object", "properties": {}}),
                    input_format: Default::default(),
                    strict: false,
                });
            }
            request.messages.push(vulcan_ai_contract::AiMessage {
                role: vulcan_ai_contract::MessageRole::Assistant,
                content: Vec::new(),
                provider_context_items: vec![json!({
                    "type": "reasoning",
                    "encrypted_content": "opaque-legacy-continuation"
                })],
            });

            let error = match encoding_client()
                .encode_request(&deepseek_route(protocol_id), &request)
            {
                Ok(_) => panic!(
                    "a legacy session with tool_carrying={tool_carrying} must fail before dispatch"
                ),
                Err(error) => error,
            };
            assert_unreplayable_history(&error);
        }
    }
}

/// Verifies an ordinary session without reasoning keeps encoding on every declared route.
/// 验证不含推理的普通会话在所有已声明路由上仍可编码。
///
/// # Remarks
/// 说明
/// This is the control for the failure above: the contract must reject lost reasoning without
/// blocking history that genuinely never had any.
/// 这是上述失败的对照片：契约必须拒绝丢失的推理，同时不得阻止确实从未有过推理的历史。
#[test]
fn session_without_reasoning_history_keeps_encoding_on_declared_routes() {
    for protocol_id in [
        OPENAI_RESPONSES_PROFILE_ID,
        OPENAI_CHAT_PROFILE_ID,
        ANTHROPIC_MESSAGES_PROFILE_ID,
    ] {
        let mut request = minimal_request();
        request.model_id = String::from("deepseek-chat");

        encoding_client()
            .encode_request(&deepseek_route(protocol_id), &request)
            .unwrap_or_else(|error| {
                panic!("a session with no reasoning must keep encoding on {protocol_id}: {error}")
            });
    }
}

/// Builds one "tool call first, then continue" history with or without replayed reasoning.
/// 构建一段「先工具调用、再续跑」的历史，并可选是否携带已回放推理。
///
/// # Remarks
/// 说明
/// This reproduces a tool retrieval shape: the first turn calls a tool, the continuation re-sends
/// the whole history. The only difference between the two requests is whether that first turn still
/// carries its reasoning.
/// 这复现了 工具检索形态：第一个回合调用工具，续跑重发整段历史。
/// 两个请求之间唯一的差别是第一个回合是否仍携带其推理。
fn tool_call_then_continue_request(with_reasoning: bool) -> AiExecutionRequest {
    let mut request = minimal_request();
    request.model_id = String::from("deepseek-flash");
    request.messages.insert(
        0,
        vulcan_ai_contract::AiMessage {
            role: vulcan_ai_contract::MessageRole::System,
            content: vec![ContentItem::Text {
                text: String::from("frozen instructions"),
            }],
            provider_context_items: Vec::new(),
        },
    );
    request.tools.push(vulcan_ai_contract::ToolDefinition {
        name: String::from("read_file"),
        description: String::from("Read one file"),
        input_schema: json!({"type":"object","properties":{"path":{"type":"string"}}}),
        input_format: Default::default(),
        strict: false,
    });
    let mut provider_context_items = Vec::new();
    if with_reasoning {
        provider_context_items.push(json!({
            "type": "reasoning",
            "content": [{"type": "reasoning_text", "text": "durable-reasoning"}]
        }));
    }
    provider_context_items.push(json!({
        "type": "function_call",
        "call_id": "call-1",
        "name": "read_file",
        "arguments": "{}"
    }));
    request.messages.push(vulcan_ai_contract::AiMessage {
        role: vulcan_ai_contract::MessageRole::Assistant,
        content: Vec::new(),
        provider_context_items,
    });
    request.messages.push(vulcan_ai_contract::AiMessage {
        role: vulcan_ai_contract::MessageRole::Tool,
        content: vec![ContentItem::ToolResult {
            result: vulcan_ai_contract::ToolResult {
                call_id: String::from("call-1"),
                is_error: false,
                content: Value::String(String::from("file-body")),
            },
        }],
        provider_context_items: Vec::new(),
    });
    request.messages.push(vulcan_ai_contract::AiMessage {
        role: vulcan_ai_contract::MessageRole::User,
        content: vec![ContentItem::Text {
            text: String::from("continue"),
        }],
        provider_context_items: Vec::new(),
    });
    request
}

/// Hashes the frozen request prefix a provider cache would reuse across a continuation.
/// 对续跑期间供应商缓存会复用的冻结请求前缀求哈希。
///
/// # Remarks
/// 说明
/// The prefix is exactly `instructions` plus the tool schema plus the ordered non-assistant items,
/// so the hash isolates cache-relevant structure from the replayed reasoning turn.
/// 前缀恰好是 `instructions` 加工具 Schema 加有序非助手项，
/// 因此该哈希把与缓存相关的结构和被回放的推理回合区分开。
fn frozen_prefix_hash(body: &Value) -> String {
    let items = body["input"]
        .as_array()
        .expect("the Responses body must carry an ordered input array")
        .iter()
        .filter(|item| !matches!(item["type"].as_str(), Some("reasoning" | "function_call")))
        .cloned()
        .collect::<Vec<_>>();
    let prefix = json!({
        "instructions": body.get("instructions").cloned().unwrap_or(Value::Null),
        "tools": body.get("tools").cloned().unwrap_or(Value::Null),
        "items": items,
    });
    hex::encode(<sha2::Sha256 as sha2::Digest>::digest(
        serde_json::to_string(&prefix)
            .expect("frozen prefix serialization is infallible")
            .as_bytes(),
    ))
}

/// Verifies a continuation after remote compaction keeps the opaque item and its reasoning.
/// 验证远端压缩后的续跑保留不透明压缩项与其推理。
///
/// # Remarks
/// 说明
/// Remote compaction replaces earlier turns with one provider-owned opaque item. Losing it, or
/// re-encoding it, would silently discard the compacted history the model still needs.
/// 远端压缩用一项供应商持有的不透明项替代更早的回合。
/// 丢失它或重新编码它，都会静默丢弃模型仍然需要的已压缩历史。
#[test]
fn responses_continuation_after_compaction_keeps_the_opaque_item_and_reasoning() {
    let mut request = minimal_request();
    request.model_id = String::from("deepseek-flash");
    // originalItems are one post-compaction assistant turn, in the exact order it was produced.
    // originalItems 是一个压缩后的助手回合，顺序与其产生时完全一致。
    let original_items = vec![
        json!({"type": "compaction", "encrypted_content": "opaque-compaction-payload"}),
        json!({
            "type": "reasoning",
            "content": [{"type": "reasoning_text", "text": "durable-reasoning"}]
        }),
        json!({
            "type": "message",
            "role": "assistant",
            "content": [{"type": "output_text", "text": "durable-answer"}]
        }),
    ];
    request.messages.push(vulcan_ai_contract::AiMessage {
        role: vulcan_ai_contract::MessageRole::Assistant,
        content: Vec::new(),
        provider_context_items: original_items.clone(),
    });
    request.messages.push(vulcan_ai_contract::AiMessage {
        role: vulcan_ai_contract::MessageRole::User,
        content: vec![ContentItem::Text {
            text: String::from("continue"),
        }],
        provider_context_items: Vec::new(),
    });

    let encoded = encoding_client()
        .encode_request(&deepseek_route(OPENAI_RESPONSES_PROFILE_ID), &request)
        .expect("a post-compaction continuation must encode on the declared route");
    let input = encoded
        .body
        .pointer("/input")
        .and_then(Value::as_array)
        .expect("the Responses body must carry an ordered input array");

    // The opaque item and the reasoning both survive, in their original relative order.
    // 不透明项与推理都保留下来，且保持原始相对顺序。
    let positions = original_items
        .iter()
        .map(|item| {
            input
                .iter()
                .position(|candidate| candidate == item)
                .unwrap_or_else(|| {
                    panic!("a post-compaction item must reach the final wire body: {item:#?}")
                })
        })
        .collect::<Vec<_>>();
    assert!(
        positions.windows(2).all(|pair| pair[0] < pair[1]),
        "post-compaction items must keep their original order: positions {positions:?} in {input:#?}"
    );
    // The provider-owned payload is replayed verbatim; it is never rebuilt or summarized locally.
    // 供应商持有的载荷原样回放；绝不在本地重建或摘要化。
    let compaction = input
        .iter()
        .find(|item| item["type"] == "compaction")
        .expect("the opaque compaction item must be present");
    assert_eq!(compaction["encrypted_content"], "opaque-compaction-payload");
    // The continuation itself is still appended after the compacted history.
    // 续跑内容仍然追加在已压缩历史之后。
    assert_eq!(
        input.last().expect("a final item")["content"][0]["text"],
        "continue"
    );
}

/// Verifies a retrieval turn and its control keep the frozen prefix while reasoning is replayed.
/// 验证检索回合与其对照保持冻结前缀，同时推理被回放。
///
/// # Remarks
/// 说明
/// The plan requires this comparison explicitly and forbids using cache hit rate as the pass
/// condition, so the assertion is structural: identical instructions, identical tool schema,
/// identical non-reasoning items, and the historical reasoning present only where it existed.
/// 计划明确要求此对照，并禁止以缓存命中率作为通过条件，因此断言是结构性的：
/// `instructions` 相同、工具 Schema 相同、非推理项相同，且历史推理只出现在它确实存在的一侧。
#[test]
fn retrieval_turn_and_control_keep_the_frozen_prefix_while_reasoning_is_replayed() {
    let client = encoding_client();
    let route = deepseek_route(OPENAI_RESPONSES_PROFILE_ID);
    let retrieved = client
        .encode_request(&route, &tool_call_then_continue_request(true))
        .expect("the retrieval turn must encode");
    let control = client
        .encode_request(&route, &tool_call_then_continue_request(false))
        .expect("the control turn must encode");

    // The frozen prefix is shared: provider caches may reuse it across the continuation.
    // 冻结前缀是共用的：供应商缓存在续跑期间可以复用它。
    assert_eq!(
        retrieved.body["instructions"], control.body["instructions"],
        "frozen instructions must not change between a retrieval turn and its control"
    );
    assert_eq!(
        retrieved.body["tools"], control.body["tools"],
        "the tool schema must not change between a retrieval turn and its control"
    );
    assert_eq!(
        frozen_prefix_hash(&retrieved.body),
        frozen_prefix_hash(&control.body),
        "the serialized request prefix must be identical"
    );

    let retrieved_items = retrieved.body["input"]
        .as_array()
        .expect("ordered input")
        .clone();
    let control_items = control.body["input"]
        .as_array()
        .expect("ordered input")
        .clone();
    let replayed = retrieved_items
        .iter()
        .find(|item| item["type"] == "reasoning")
        .expect("the historical reasoning of the tool-calling turn must be replayed");
    assert_eq!(replayed["content"][0]["text"], "durable-reasoning");
    assert!(
        !control_items.iter().any(|item| item["type"] == "reasoning"),
        "the control never had reasoning and must not gain one: {control_items:#?}"
    );
    // The reasoning item is the single difference; the tool call keeps its exact identity.
    // 推理项是唯一差别；工具调用保持其精确身份。
    let without_reasoning = retrieved_items
        .iter()
        .filter(|item| item["type"] != "reasoning")
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(Value::Array(without_reasoning), Value::Array(control_items));
    assert_eq!(replayed["content"][0]["type"], "reasoning_text");
}

/// Verifies undeclared routes keep accepting an opaque continuation on every supported protocol.
/// 验证未声明路由在每个受支持协议上仍接受不透明延续。
///
/// # Remarks
/// 说明
/// This is the regression contrast the plan requires: an OpenAI-native encrypted reasoning record
/// must keep encoding on every route that declared no contract, so the repair never over-blocks
/// history it cannot interpret. Only a route that declared a plaintext contract may reject it.
/// 这是计划要求的回归对照：OpenAI 原生加密推理记录必须在所有未声明契约的路由上继续编码，
/// 使本次修复绝不误伤它无法解释的历史。只有声明了明文契约的路由才可以拒绝它。
#[test]
fn undeclared_routes_keep_accepting_opaque_reasoning_on_every_protocol() {
    for (provider_id, entry_id, protocol_id) in [
        ("openrouter", "public-api", OPENAI_RESPONSES_PROFILE_ID),
        ("openrouter", "public-api", OPENAI_CHAT_PROFILE_ID),
        ("openrouter", "public-api", ANTHROPIC_MESSAGES_PROFILE_ID),
        // Gemini is reached through its own provider, which declares no replay contract at all.
        // Gemini 通过其自有供应商抵达，该供应商完全没有声明回传契约。
        ("google", "gemini-api", GEMINI_GENERATE_CONTENT_PROFILE_ID),
    ] {
        let mut request = minimal_request();
        request.messages.push(vulcan_ai_contract::AiMessage {
            role: vulcan_ai_contract::MessageRole::Assistant,
            content: Vec::new(),
            provider_context_items: vec![json!({
                "type": "reasoning",
                "summary": [{"type": "summary_text", "text": "opaque-native-reasoning"}],
                "encrypted_content": "opaque-native-continuation"
            })],
        });

        encoding_client()
            .encode_request(
                &bundled_manifest_route(provider_id, entry_id, protocol_id),
                &request,
            )
            .unwrap_or_else(|error| {
                panic!(
                    "an undeclared {provider_id}/{protocol_id} route must keep accepting opaque reasoning: {error}"
                )
            });
    }
}

/// Verifies two recorded reasoning turns with identical text each need their own replayed entry.
/// 验证两条文本相同的已记录推理回合各自需要属于自己的回放项。
///
/// # Remarks
/// 说明
/// A "any output contains this text" check would let one replayed entry satisfy both turns, so a
/// dropped turn would pass unnoticed whenever two turns happen to share the exact same reasoning.
/// 「任意输出包含该文本」式校验会让同一个回放项同时满足两个回合，
/// 因此只要两个回合恰好共享同一段推理，被丢弃的那个回合就会无声通过。
#[test]
fn declared_chat_validation_requires_one_reasoning_entry_per_recorded_turn() {
    // The request records two turns whose reasoning text is byte-identical.
    // 该请求记录了两个推理正文逐字节相同的回合。
    let mut request = minimal_request();
    request.model_id = String::from("deepseek-chat");
    for _ in 0..2 {
        request.messages.push(vulcan_ai_contract::AiMessage {
            role: vulcan_ai_contract::MessageRole::Assistant,
            content: vec![ContentItem::Reasoning {
                text: String::from("identical-reasoning"),
                signature: None,
            }],
            provider_context_items: Vec::new(),
        });
    }

    // oneEntry carries a single assistant turn, which can only cover one of the two records.
    // oneEntry 只携带一个助手回合，最多只能覆盖两条记录中的一条。
    let one_entry = json!({"messages": [
        {"role": "assistant", "content": "", "reasoning_content": "identical-reasoning"}
    ]});
    let failure = super::validate_deepseek_chat_reasoning_content_replay(&request, &one_entry)
        .expect_err("one replayed entry must not cover two recorded turns");
    assert_unreplayable_history(&failure);

    // twoEntries proves the same request is genuinely sendable when both turns are replayed.
    // twoEntries 证明同一请求在两个回合都被回放时确实可以发送。
    let two_entries = json!({"messages": [
        {"role": "assistant", "content": "", "reasoning_content": "identical-reasoning"},
        {"role": "assistant", "content": "", "reasoning_content": "identical-reasoning"}
    ]});
    super::validate_deepseek_chat_reasoning_content_replay(&request, &two_entries)
        .expect("both recorded turns replayed individually must pass");
}

/// Verifies every declared route rejects a tool result that precedes its own call.
/// 验证每条已声明路由都拒绝先于自身调用的工具结果。
///
/// # Remarks
/// 说明
/// The plan requires the final wire validation to check call identifiers and their order, because a
/// result replayed before its call changes what the model reads even though both identifiers exist.
/// 计划要求最终线路校验检查调用标识与先后顺序：
/// 即使两个标识都在，先于调用回放的结果仍会改变模型读到的内容。
#[test]
fn declared_routes_reject_a_tool_result_that_precedes_its_call() {
    // The request records no reasoning, so only the ordering rule can reject these bodies.
    // 该请求不记录任何推理，因此只有顺序规则能拒绝这些正文。
    let request = minimal_request();

    // A result with no preceding call is rejected on every declared route.
    // 没有前置调用的结果在每条已声明路由上都被拒绝。
    let responses = json!({"input": [
        {"type": "function_call_output", "call_id": "call-1", "output": "orphan"}
    ]});
    super::validate_deepseek_responses_plaintext_replay(&request, &responses)
        .expect_err("an orphan Responses tool result must fail before dispatch");

    let chat = json!({"messages": [
        {"role": "tool", "tool_call_id": "call-1", "content": "orphan"}
    ]});
    super::validate_deepseek_chat_reasoning_content_replay(&request, &chat)
        .expect_err("an orphan Chat tool result must fail before dispatch");

    let anthropic = json!({"messages": [
        {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "call-1", "content": "orphan"}]}
    ]});
    super::validate_deepseek_anthropic_thinking_replay(&request, &anthropic)
        .expect_err("an orphan Anthropic tool result must fail before dispatch");

    // The same identifier used twice is refused on every declared route.
    // 同一标识被使用两次时，每条已声明路由都拒绝。
    let duplicated_responses = json!({"input": [
        {"type": "function_call", "call_id": "call-1", "name": "read_file", "arguments": "{}"},
        {"type": "function_call", "call_id": "call-1", "name": "read_file", "arguments": "{}"}
    ]});
    super::validate_deepseek_responses_plaintext_replay(&request, &duplicated_responses)
        .expect_err("a duplicated Responses tool call identity must fail before dispatch");

    let duplicated_chat = json!({"messages": [
        {"role": "assistant", "content": "", "tool_calls": [{"id": "call-1"}, {"id": "call-1"}]}
    ]});
    super::validate_deepseek_chat_reasoning_content_replay(&request, &duplicated_chat)
        .expect_err("a duplicated Chat tool call identity must fail before dispatch");

    let duplicated_anthropic = json!({"messages": [
        {"role": "assistant", "content": [
            {"type": "tool_use", "id": "call-1", "name": "read_file", "input": {}},
            {"type": "tool_use", "id": "call-1", "name": "read_file", "input": {}}
        ]}
    ]});
    super::validate_deepseek_anthropic_thinking_replay(&request, &duplicated_anthropic)
        .expect_err("a duplicated Anthropic tool_use identity must fail before dispatch");

    // The ordered forms are accepted, so the rejections above come from order and not from shape.
    // 有序形态被接受，因此上面的拒绝来自顺序而不是形态。
    let ordered_responses = json!({"input": [
        {"type": "function_call", "call_id": "call-1", "name": "read_file", "arguments": "{}"},
        {"type": "function_call_output", "call_id": "call-1", "output": "body-a"}
    ]});
    super::validate_deepseek_responses_plaintext_replay(&request, &ordered_responses)
        .expect("a call followed by its own result must pass");

    let ordered_chat = json!({"messages": [
        {"role": "assistant", "content": "", "tool_calls": [{"id": "call-1"}]},
        {"role": "tool", "tool_call_id": "call-1", "content": "body-a"}
    ]});
    super::validate_deepseek_chat_reasoning_content_replay(&request, &ordered_chat)
        .expect("a call followed by its own result must pass");

    let ordered_anthropic = json!({"messages": [
        {"role": "assistant", "content": [{"type": "tool_use", "id": "call-1", "name": "read_file", "input": {}}]},
        {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "call-1", "content": "body-a"}]}
    ]});
    super::validate_deepseek_anthropic_thinking_replay(&request, &ordered_anthropic)
        .expect("a call followed by its own result must pass");
}

/// Asserts one failure is the unreplayable-history consequence with its fixed user copy.
/// 断言某个失败是「推理历史无法回放」这一兼容后果，并携带固定用户文案。
///
/// # Remarks
/// 说明
/// The user-visible copy is the same for every unreplayable shape, so the machine-readable
/// subcategory carries the exact reason and the message must never grow into a diagnostic.
/// 每种无法回放的形态使用同一份用户可见文案，因此精确原因由机器可读子类别承载，
/// 而消息绝不能膨胀成诊断信息。
fn assert_unreplayable_history(failure: &ProviderExecutionFailure) {
    assert!(
        failure.error.message.is_ascii(),
        "runtime error copy must remain English"
    );
    assert_eq!(
        failure.error.subcategory.as_deref(),
        Some(crate::execution::REASONING_REPLAY_UNREPLAYABLE_SUBCATEGORY),
        "the failure must carry the stable machine reason"
    );
    assert_eq!(
        failure.error.message,
        crate::execution::REASONING_REPLAY_UNREPLAYABLE_MESSAGE,
        "the user must see the fixed copy"
    );
    assert_eq!(
        failure.error.dispatch_outcome,
        vulcan_ai_contract::DispatchOutcome::NotDispatched
    );
    assert!(
        failure.error.terminal && !failure.error.retryable,
        "no retry and no protocol fallback may replace this failure"
    );
    // The diagnostic is retained for logs but must never leak persisted reasoning text.
    // 诊断信息保留给日志，但绝不能泄露持久化推理正文。
    let diagnostic = failure.error.upstream_message.clone().unwrap_or_default();
    assert!(
        diagnostic.contains("reasoning") && !diagnostic.contains("durable-reasoning"),
        "the diagnostic must name the missing reasoning without quoting it: {diagnostic}"
    );
}

/// Builds the shared execution client used by the reasoning replay wire tests.
/// 构建推理回放线路测试共用的执行客户端。
fn encoding_client() -> ProviderExecutionClient {
    ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    )
}

/// Builds one DeepSeek Responses request whose assistant turn carries only canonical reasoning.
/// 构建一个助手回合仅携带规范推理的 DeepSeek Responses 请求。
fn signature_free_canonical_reasoning_request() -> AiExecutionRequest {
    let mut request = minimal_request();
    request.model_id = String::from("deepseek-flash");
    request.messages.push(vulcan_ai_contract::AiMessage {
        role: vulcan_ai_contract::MessageRole::Assistant,
        content: vec![ContentItem::Reasoning {
            text: String::from("durable-reasoning"),
            signature: None,
        }],
        provider_context_items: Vec::new(),
    });
    request
}

/// Verifies a model switch can replay canonical reasoning on the declared plaintext route.
/// 验证模型切换可以在声明明文契约的路由回放规范推理。
#[test]
fn declared_deepseek_route_preserves_unsigned_reasoning_before_dispatch() {
    // Encoded is the exact final wire request after every manifest projection and validator.
    // encoded 是经过全部清单投影与校验后的精确最终线路请求。
    let encoded = encoding_client()
        .encode_request(
            &deepseek_route(OPENAI_RESPONSES_PROFILE_ID),
            &signature_free_canonical_reasoning_request(),
        )
        .expect("a plaintext route must preserve canonical reasoning after a model switch");
    // Reasoning is located by its type instead of a shifting input position.
    // reasoning 按类型定位，而不是依赖会变化的输入位置。
    let reasoning = encoded.body["input"]
        .as_array()
        .expect("input array")
        .iter()
        .find(|item| item["type"] == "reasoning")
        .expect("plaintext reasoning must survive");
    assert_eq!(
        reasoning,
        &json!({"type":"reasoning", "content":[{"type":"reasoning_text", "text":"durable-reasoning"}]})
    );
}

/// Verifies plaintext replay preserves empty ranges, mixed native prefixes and complete tool order.
/// 验证明文回放保留空区间、混合原生前缀及完整工具顺序。
#[test]
fn deepseek_responses_preserves_canonical_reasoning_and_tool_order() {
    use vulcan_ai_contract::{AiMessage, MessageRole, ToolCall, ToolResult};
    // Request includes a reasoning-only turn that produced no generic input item.
    // request 包含未产生任何通用输入项的纯推理回合。
    let mut request = signature_free_canonical_reasoning_request();
    // Native prefix is preserved exactly while the same message's canonical suffix is rebuilt.
    // native_prefix 被精确保留，而同一消息的规范后缀被重建。
    let native_prefix = json!({"type":"message", "id":"native-prefix", "role":"assistant", "content":[{"type":"output_text", "text":"native body"}]});
    request.messages.push(AiMessage {
        role: MessageRole::User,
        content: vec![ContentItem::Text {
            text: "next task".into(),
        }],
        provider_context_items: vec![],
    });
    request.messages.push(AiMessage {
        role: MessageRole::Assistant,
        content: vec![
            ContentItem::Text {
                text: "before thinking".into(),
            },
            ContentItem::Reasoning {
                text: "second reasoning".into(),
                signature: Some("openai#opaque".into()),
            },
            ContentItem::Text {
                text: "before call".into(),
            },
            ContentItem::ToolCall {
                call: ToolCall {
                    id: "call-ordered".into(),
                    name: "read_file".into(),
                    arguments: json!({"path":"sample.txt"}),
                    signature: None,
                },
            },
            ContentItem::Reasoning {
                text: "third reasoning".into(),
                signature: None,
            },
        ],
        provider_context_items: vec![native_prefix.clone()],
    });
    request.messages.push(AiMessage {
        role: MessageRole::Tool,
        content: vec![ContentItem::ToolResult {
            result: ToolResult {
                call_id: "call-ordered".into(),
                is_error: false,
                content: json!("file body"),
            },
        }],
        provider_context_items: vec![],
    });
    request.messages.push(AiMessage {
        role: MessageRole::Assistant,
        content: vec![ContentItem::Text {
            text: "final answer".into(),
        }],
        provider_context_items: vec![],
    });
    // Encoded crosses both the shared internal envelope and every final route projection.
    // encoded 同时经过共享内部封装及全部最终路由投影。
    let encoded = encoding_client()
        .encode_request(&deepseek_route(OPENAI_RESPONSES_PROFILE_ID), &request)
        .expect("ordered portable history must reach the declared Responses wire");
    // Input excludes only the initial fixture user message, keeping all tested semantic boundaries.
    // input 仅排除初始夹具用户消息，保留全部被测语义边界。
    let input = encoded.body["input"].as_array().expect("input array");
    let first_reasoning = input
        .iter()
        .position(|item| item["type"] == "reasoning")
        .expect("first reasoning");
    assert_eq!(
        &input[first_reasoning..],
        &[
            json!({"type":"reasoning", "content":[{"type":"reasoning_text", "text":"durable-reasoning"}]}),
            json!({"role":"user", "content":[{"type":"input_text", "text":"next task"}]}),
            native_prefix,
            json!({"role":"assistant", "content":[{"type":"output_text", "text":"before thinking"}]}),
            json!({"type":"reasoning", "content":[{"type":"reasoning_text", "text":"second reasoning"}]}),
            json!({"role":"assistant", "content":[{"type":"output_text", "text":"before call"}]}),
            json!({"type":"function_call", "call_id":"call-ordered", "name":"read_file", "arguments":"{\"path\":\"sample.txt\"}"}),
            json!({"type":"reasoning", "content":[{"type":"reasoning_text", "text":"third reasoning"}]}),
            json!({"type":"function_call_output", "call_id":"call-ordered", "output":"file body"}),
            json!({"role":"assistant", "content":[{"type":"output_text", "text":"final answer"}]}),
        ],
        "plaintext replay must preserve each boundary without signatures or duplicated native items"
    );
}

/// Ensures final validation still rejects a late projection that removes recovered reasoning.
/// 确保最终校验仍拒绝后置投影移除已恢复推理的情况。
#[test]
fn deepseek_responses_rejects_plaintext_loss_after_custom_projection() {
    // Route empties input after encoding to exercise the final replay guard on valid wire syntax.
    // route 在编码后清空输入，以在线路语法有效时验证最终回放护栏。
    let mut route = deepseek_route(OPENAI_RESPONSES_PROFILE_ID);
    route.custom_model_projection = Some(CustomModelRequestProjection {
        overrides: BTreeMap::from([(String::from("input"), json!([]))]),
        ..Default::default()
    });
    let error = encoding_client()
        .encode_request(&route, &signature_free_canonical_reasoning_request())
        .expect_err("late projection must not drop reasoning");
    assert_unreplayable_history(&error);
}

/// Verifies an undeclared Responses route keeps its previous request bytes unchanged.
/// 验证未声明的 Responses 路由保持既有请求字节不变。
#[test]
fn undeclared_responses_route_keeps_signature_free_reasoning_bytes_unchanged() {
    // undeclared owns no reasoning replay contract, so its wire bytes must stay exactly as before.
    // undeclared 不持有推理回传契约，因此其线路字节必须与之前完全一致。
    let encoded = encoding_client()
        .encode_request(
            &pinned_openrouter_route(OPENAI_RESPONSES_PROFILE_ID),
            &signature_free_canonical_reasoning_request(),
        )
        .expect("an undeclared route must keep encoding exactly as it did before");

    let input = encoded
        .body
        .pointer("/input")
        .and_then(Value::as_array)
        .expect("the Responses body must carry an ordered input array");
    assert!(
        !input.iter().any(|item| item["type"] == "reasoning"),
        "an undeclared route must not gain a reasoning item it never had: {input:#?}"
    );
}

/// Verifies the declared route refuses every reasoning item that cannot carry plaintext history.
/// 验证已声明路由拒绝每一个无法承载明文历史的推理项。
#[test]
fn declared_deepseek_route_rejects_non_plaintext_reasoning_items() {
    // unusableItems are the exact shapes this endpoint ignores instead of merging into the turn.
    // unusableItems 是该端点会忽略而不是归并进回合的精确形态。
    let unusable_items = [
        json!({"type": "reasoning", "summary": [{"type": "summary_text", "text": "summary"}]}),
        json!({"type": "reasoning", "encrypted_content": "opaque"}),
        json!({"type": "reasoning", "content": []}),
        json!({"type": "reasoning", "content": [{"type": "summary_text", "text": "wrong"}]}),
        json!({"type": "reasoning", "content": [{"type": "reasoning_text", "text": ""}]}),
    ];

    for item in unusable_items {
        assert!(
            encoding_client()
                .encode_request(
                    &deepseek_route(OPENAI_RESPONSES_PROFILE_ID),
                    &deepseek_replay_request(vec![item.clone()]),
                )
                .is_err(),
            "a non-plaintext reasoning item must fail before dispatch: {item:#?}"
        );
    }

    // targetOnly lists the shapes that survive the shared allowlist yet still cannot carry DeepSeek
    // plaintext history, so only the declared contract can stop them.
    // targetOnly 列出能通过通用 allowlist、但仍无法承载 DeepSeek 明文历史的形态，
    // 因此只有已声明契约能够拦下它们。
    let target_only = [
        json!({"type": "reasoning", "encrypted_content": "opaque"}),
        json!({"type": "reasoning", "content": [{"type": "summary_text", "text": "wrong"}]}),
        json!({"type": "reasoning", "content": [{"type": "reasoning_text", "text": ""}]}),
    ];
    for item in target_only {
        let error = encoding_client()
            .encode_request(
                &deepseek_route(OPENAI_RESPONSES_PROFILE_ID),
                &deepseek_replay_request(vec![item.clone()]),
            )
            .expect_err("the declared contract must reject this shape before dispatch");
        assert_unreplayable_history(&error);
    }
}

/// Verifies the declared route refuses duplicated or unpaired replayed tool identities.
/// 验证已声明路由拒绝重复或未配对的已回放工具身份。
#[test]
fn declared_deepseek_route_rejects_duplicate_and_orphan_tool_identities() {
    let duplicated = vec![
        json!({"type": "function_call", "call_id": "call-1", "name": "read_file", "arguments": "{}"}),
        json!({"type": "function_call", "call_id": "call-1", "name": "read_file", "arguments": "{}"}),
    ];
    let error = encoding_client()
        .encode_request(
            &deepseek_route(OPENAI_RESPONSES_PROFILE_ID),
            &deepseek_replay_request(duplicated),
        )
        .expect_err("a duplicated tool identity must fail before dispatch");
    assert!(
        error.error.message.as_str().contains("call-1"),
        "the failure must name the duplicated identity: {}",
        error.error.message
    );

    let orphaned = vec![json!({
        "type": "function_call_output",
        "call_id": "call-9",
        "output": "done"
    })];
    let error = encoding_client()
        .encode_request(
            &deepseek_route(OPENAI_RESPONSES_PROFILE_ID),
            &deepseek_replay_request(orphaned),
        )
        .expect_err("an unpaired tool result must fail before dispatch");
    assert!(
        error.error.message.as_str().contains("call-9"),
        "the failure must name the orphaned result: {}",
        error.error.message
    );
}

/// Verifies a custom projector cannot silently remove the required Anthropic schema.
/// 验证自定义投影器不能静默删除 Anthropic 所需的 Schema。
#[test]
fn encode_request_rejects_structured_schema_loss_after_custom_projection() {
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    let mut route = pinned_openrouter_route(ANTHROPIC_MESSAGES_PROFILE_ID);
    route.custom_model_projection = Some(CustomModelRequestProjection {
        delete: vec![String::from("output_config")],
        ..Default::default()
    });
    let mut request = minimal_request();
    request.structured_output = StructuredOutputPolicy::JsonSchema {
        name: String::from("result"),
        description: None,
        schema: json!({
            "type": "object",
            "properties": { "accepted": { "type": "boolean" } },
            "required": ["accepted"],
            "additionalProperties": false,
        }),
        strict: true,
    };

    let error = client
        .encode_request(&route, &request)
        .expect_err("schema loss must fail before dispatch");
    assert!(error.error.message.contains("dropped or changed"));
}

/// Verifies custom model projections apply typed parameters, reasoning, deletion, and overrides deterministically.
/// 验证自定义模型投影会确定性地应用强类型参数、推理、删除与强制覆盖。
#[test]
fn custom_model_projection_applies_registered_request_mutations() {
    let mut request = minimal_request();
    request.reasoning_effort = Some(ReasoningEffort::High);
    request.model_parameters.insert(
        String::from("vendor.temperature"),
        ModelParameterValue::Decimal(String::from("0.3")),
    );
    let projection = CustomModelRequestProjection {
        reasoning: BTreeMap::from([(
            ReasoningEffort::High,
            crate::CustomRequestMutation {
                set: BTreeMap::from([(
                    String::from("thinking.mode"),
                    Value::String(String::from("enabled")),
                )]),
                delete: Vec::new(),
            },
        )]),
        model_parameter_paths: BTreeMap::from([(
            String::from("vendor.temperature"),
            String::from("sampling.temperature"),
        )]),
        defaults: BTreeMap::from([(String::from("vendor.keep"), Value::Bool(true))]),
        overrides: BTreeMap::from([(
            String::from("vendor.forced"),
            Value::String(String::from("yes")),
        )]),
        delete: vec![String::from("remove_me")],
    };
    let mut encoded = EncodedWireRequest {
        path: String::from("/v1/chat/completions"),
        headers: BTreeMap::new(),
        body: json!({"model": "custom-model", "messages": [], "remove_me": true}),
    };

    project_custom_model_request(&projection, &request, &mut encoded)
        .expect("custom projection should succeed");

    assert_eq!(encoded.body["thinking"]["mode"], "enabled");
    assert_eq!(encoded.body["sampling"]["temperature"], json!(0.3));
    assert_eq!(encoded.body["vendor"]["keep"], true);
    assert_eq!(encoded.body["vendor"]["forced"], "yes");
    assert!(encoded.body.get("remove_me").is_none());
}

/// Verifies a configured reasoning projection reaches the final encoded request body.
/// 验证已配置的思考投影会抵达最终编码请求体，而不是只停留在编辑器状态。
#[test]
fn encode_request_applies_custom_reasoning_projection_to_final_wire_body() {
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    let mut route = pinned_openrouter_route(OPENAI_CHAT_PROFILE_ID);
    route.custom_model_projection = Some(CustomModelRequestProjection {
        reasoning: BTreeMap::from([(
            ReasoningEffort::High,
            crate::CustomRequestMutation {
                set: BTreeMap::from([(
                    String::from("thinking.type"),
                    Value::String(String::from("enabled")),
                )]),
                delete: vec![String::from("reasoning_effort")],
            },
        )]),
        ..Default::default()
    });
    let mut request = minimal_request();
    request.reasoning_effort = Some(ReasoningEffort::High);

    let encoded = client
        .encode_request(&route, &request)
        .expect("custom reasoning projection should encode");

    assert_eq!(encoded.body["thinking"]["type"], "enabled");
    assert!(encoded.body.get("reasoning_effort").is_none());
}

/// Verifies required no-reasoning validation inspects the final body and fails closed.
/// 验证必需无思考校验检查最终请求体并采用失败关闭策略。
#[test]
fn required_disabled_reasoning_validation_rejects_dropped_or_contradictory_projection() {
    // client owns the same encoder and projection chain used by production dispatch.
    // client 持有与生产分派相同的编码器与投影链。
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    // request is the exact VMM semantic: reasoning effort must be none.
    // request 是精确 VMM 语义：思考档位必须为 none。
    let mut request = minimal_request();
    request.reasoning_effort = Some(ReasoningEffort::None);

    // explicitRoute proves the OpenRouter profile retains reasoning.effort=none.
    // explicitRoute 证明 OpenRouter Profile 保留 reasoning.effort=none。
    let explicit_route = pinned_openrouter_route(OPENAI_CHAT_PROFILE_ID);
    let explicit = client
        .encode_required_disabled_reasoning_request(&explicit_route, &request, false)
        .expect("explicit OpenRouter disabled projection should pass");
    assert_eq!(explicit.body["reasoning"]["effort"], "none");

    // omissionRoute proves a provider may define none as deleting its exact Chat field.
    // omissionRoute 证明供应商可以把删除其精确 Chat 字段定义为 none 的线路语义。
    let mut omission_route = pinned_openrouter_route(OPENAI_CHAT_PROFILE_ID);
    omission_route.request_projection_profiles =
        vec![crate::provider_manifest::RequestProjectionProfile {
            profile_id: String::from("vendor.chat.omit-disabled.v1"),
            revision: 1,
            steps: vec![crate::provider_manifest::MappingStep::DeleteIfEquals {
                path: String::from("/reasoning_effort"),
                source: crate::provider_manifest::MappingStepSource::RequestField,
                source_path: String::from("@reasoning_effort"),
                equals: String::from("none"),
            }],
        }];
    let omission = client
        .encode_required_disabled_reasoning_request(&omission_route, &request, false)
        .expect("a profile-declared disabled omission should pass");
    assert!(omission.body.get("reasoning_effort").is_none());

    // droppedRoute removes the only Chat provider profile; canonical Chat conversion omits
    // effort none, so a switchable model must be rejected rather than using provider default.
    // droppedRoute 移除唯一的 Chat 供应商 Profile；规范 Chat 转换会省略 none 档位，
    // 因此可切换模型必须被拒绝，而不能使用供应商默认值。
    let mut dropped_route = explicit_route.clone();
    dropped_route.request_projection_profiles.clear();
    let dropped = client
        .encode_required_disabled_reasoning_request(&dropped_route, &request, false)
        .expect_err("a dropped disabled projection must fail closed");
    assert!(dropped.error.message.contains("does not preserve"));

    // fixedOff accepts an absent toggle only because the frozen model capability says the
    // model can never reason; no name-based inference is involved.
    // fixedOff 仅因冻结模型能力声明模型永远不会思考，才接受缺失开关；不涉及模型名推断。
    client
        .encode_required_disabled_reasoning_request(&dropped_route, &request, true)
        .expect("fixed-off models do not require a redundant wire toggle");

    // contradictoryRoute simulates a late custom projection that re-enables thinking after
    // the provider profile emitted none; this must fail even for a fixed-off declaration.
    // contradictoryRoute 模拟供应商 Profile 输出 none 后，后置自定义投影重新开启思考；
    // 即使声明为固定关闭也必须失败。
    let mut contradictory_route = explicit_route;
    contradictory_route.custom_model_projection = Some(CustomModelRequestProjection {
        reasoning: BTreeMap::from([(
            ReasoningEffort::None,
            crate::CustomRequestMutation {
                set: BTreeMap::from([(
                    String::from("thinking.type"),
                    Value::String(String::from("enabled")),
                )]),
                delete: Vec::new(),
            },
        )]),
        ..Default::default()
    });
    let contradictory = client
        .encode_required_disabled_reasoning_request(&contradictory_route, &request, true)
        .expect_err("a contradictory enabled control must fail closed");
    assert!(contradictory.error.message.contains("enables reasoning"));

    // budgetRoute proves a disabled type cannot hide a positive reasoning budget left by a
    // later custom projection.
    // budgetRoute 证明关闭类型不能掩盖后置自定义投影残留的正数思考预算。
    let mut budget_route = pinned_openrouter_route(OPENAI_CHAT_PROFILE_ID);
    budget_route.custom_model_projection = Some(CustomModelRequestProjection {
        reasoning: BTreeMap::from([(
            ReasoningEffort::None,
            crate::CustomRequestMutation {
                set: BTreeMap::from([
                    (
                        String::from("thinking.type"),
                        Value::String(String::from("disabled")),
                    ),
                    (
                        String::from("thinking.budget_tokens"),
                        Value::Number(4096.into()),
                    ),
                ]),
                delete: Vec::new(),
            },
        )]),
        ..Default::default()
    });
    let budget_conflict = client
        .encode_required_disabled_reasoning_request(&budget_route, &request, true)
        .expect_err("a non-zero reasoning budget must fail closed");
    assert!(budget_conflict.error.message.contains("enables reasoning"));

    // unknownTypeRoute rejects every non-disabled thinking type instead of recognizing only
    // the historical enabled/adaptive labels.
    // unknownTypeRoute 拒绝所有非关闭 thinking 类型，而不是只识别历史 enabled/adaptive 标签。
    let mut unknown_type_route = pinned_openrouter_route(OPENAI_CHAT_PROFILE_ID);
    unknown_type_route.custom_model_projection = Some(CustomModelRequestProjection {
        reasoning: BTreeMap::from([(
            ReasoningEffort::None,
            crate::CustomRequestMutation {
                set: BTreeMap::from([(
                    String::from("thinking.type"),
                    Value::String(String::from("high")),
                )]),
                delete: Vec::new(),
            },
        )]),
        ..Default::default()
    });
    let unknown_type = client
        .encode_required_disabled_reasoning_request(&unknown_type_route, &request, true)
        .expect_err("an unknown non-disabled thinking type must fail closed");
    assert!(unknown_type.error.message.contains("enables reasoning"));

    // crossDialectResponses removes the authoritative Responses effort and leaves an
    // Anthropic-shaped disabled marker; protocol-specific validation must reject the decoy.
    // crossDialectResponses 移除权威 Responses effort 并留下 Anthropic 形态的关闭标记；
    // 协议专属校验必须拒绝该诱饵字段。
    let mut cross_dialect_responses = deepseek_route(OPENAI_RESPONSES_PROFILE_ID);
    cross_dialect_responses.custom_model_projection = Some(CustomModelRequestProjection {
        reasoning: BTreeMap::from([(
            ReasoningEffort::None,
            crate::CustomRequestMutation {
                set: BTreeMap::from([(
                    String::from("thinking.type"),
                    Value::String(String::from("disabled")),
                )]),
                delete: vec![String::from("reasoning.effort")],
            },
        )]),
        ..Default::default()
    });
    let cross_dialect_responses_failure = client
        .encode_required_disabled_reasoning_request(&cross_dialect_responses, &request, false)
        .expect_err("Responses must not accept an Anthropic-shaped disabled marker");
    assert!(
        cross_dialect_responses_failure
            .error
            .message
            .contains("does not preserve")
    );

    // crossDialectAnthropic removes the authoritative Anthropic thinking type and leaves a
    // Responses-shaped effort; the wrong protocol vocabulary must not satisfy VMM admission.
    // crossDialectAnthropic 移除权威 Anthropic thinking type 并留下 Responses 形态 effort；
    // 错误协议词汇不得满足 VMM 准入条件。
    let mut cross_dialect_anthropic = deepseek_route(ANTHROPIC_MESSAGES_PROFILE_ID);
    cross_dialect_anthropic.custom_model_projection = Some(CustomModelRequestProjection {
        reasoning: BTreeMap::from([(
            ReasoningEffort::None,
            crate::CustomRequestMutation {
                set: BTreeMap::from([(
                    String::from("reasoning.effort"),
                    Value::String(String::from("none")),
                )]),
                delete: vec![String::from("thinking.type")],
            },
        )]),
        ..Default::default()
    });
    let cross_dialect_anthropic_failure = client
        .encode_required_disabled_reasoning_request(&cross_dialect_anthropic, &request, false)
        .expect_err("Anthropic must not accept a Responses-shaped disabled marker");
    assert!(
        cross_dialect_anthropic_failure
            .error
            .message
            .contains("does not preserve")
    );

    // crossDialectChat removes the DeepSeek Chat profile's authoritative thinking object
    // and leaves a Responses-shaped effort; only the profile-declared path may prove none.
    // crossDialectChat 移除 DeepSeek Chat Profile 的权威 thinking 对象并留下
    // Responses 形态的 effort；只有 Profile 声明的路径才能证明 none。
    let mut cross_dialect_chat = deepseek_route(OPENAI_CHAT_PROFILE_ID);
    cross_dialect_chat.custom_model_projection = Some(CustomModelRequestProjection {
        reasoning: BTreeMap::from([(
            ReasoningEffort::None,
            crate::CustomRequestMutation {
                set: BTreeMap::from([(
                    String::from("reasoning.effort"),
                    Value::String(String::from("none")),
                )]),
                delete: vec![String::from("thinking.type")],
            },
        )]),
        ..Default::default()
    });
    let cross_dialect_chat_failure = client
        .encode_required_disabled_reasoning_request(&cross_dialect_chat, &request, false)
        .expect_err("DeepSeek Chat must not accept a Responses-shaped disabled marker");
    assert!(
        cross_dialect_chat_failure
            .error
            .message
            .contains("does not preserve")
    );
}

/// Verifies the bundled DeepSeek manifest closes the conversation upload chain only on routes
/// whose protocol owns a standard image file-id block, and that those routes encode the id.
/// 验证随包 DeepSeek 清单仅在协议拥有标准图片文件 ID 块的路由上闭合会话上传链，且这些路由
/// 会编码该 ID。
#[test]
fn deepseek_conversation_images_upload_only_on_file_id_capable_routes() {
    use vulcan_ai_contract::ResourceKind;
    let client = encoding_client();
    for (protocol_id, block_pointer) in [
        (OPENAI_RESPONSES_PROFILE_ID, "/input/0/content/1"),
        (ANTHROPIC_MESSAGES_PROFILE_ID, "/messages/0/content/1"),
    ] {
        let route = deepseek_route(protocol_id);
        let contract = client
            .conversation_media_upload_contract(&route, ResourceKind::Image, "image/png")
            .expect("upload contract resolution")
            .unwrap_or_else(|| panic!("{protocol_id} must resolve a real image upload"));
        assert_eq!(
            contract.reference_encoding,
            crate::UploadReferenceEncoding::ProviderFileId,
            "{protocol_id}"
        );
        let facts = crate::MediaDeliveryTarget {
            profile_id: protocol_id,
            model_inputs: &[ResourceKind::Image],
            constraint: None,
            upload: Some(&contract),
        }
        .facts(
            ResourceKind::Image,
            "image/png",
            1024,
            crate::MediaDeliveryOrigin::NewIntent,
            1,
        );
        assert!(
            matches!(
                facts.upload,
                crate::MediaUploadAvailability::Available { .. }
            ),
            "{protocol_id}: {:?}",
            facts.upload
        );
        // The uploaded id reaches the wire through the protocol's own file-reference block.
        // 已上传 ID 通过协议自身的文件引用块进入线路。
        let mut request = minimal_request();
        request.model_id = String::from("deepseek-flash");
        request.messages[0].content.push(ContentItem::Resource {
            resource: vulcan_ai_contract::ResourceReference {
                id: String::from("file-api-fixture"),
                kind: ResourceKind::Image,
                media_type: Some(String::from("image/png")),
                source_kind: vulcan_ai_contract::ResourceSourceKind::ProviderFileId,
                file_name: Some(String::from("image.png")),
                image_detail: None,
                resource_path: None,
                local_path: None,
                url: None,
            },
        });
        let encoded = client
            .encode_request(&route, &request)
            .unwrap_or_else(|error| panic!("{protocol_id} must encode the file id: {error:?}"));
        let block = encoded
            .body
            .pointer(block_pointer)
            .unwrap_or_else(|| panic!("{protocol_id} image block"));
        assert!(
            block
                .to_string()
                .contains("\"file_id\":\"file-api-fixture\""),
            "{protocol_id}: {block}"
        );
    }
    // OpenAI Chat has no standard image file-id block, so the Chat route keeps inline delivery.
    // OpenAI Chat 没有标准图片文件 ID 块，因此 Chat 路由保持内联投递。
    assert!(
        client
            .conversation_media_upload_contract(
                &deepseek_route(OPENAI_CHAT_PROFILE_ID),
                ResourceKind::Image,
                "image/png"
            )
            .expect("upload contract resolution")
            .is_none()
    );
}
