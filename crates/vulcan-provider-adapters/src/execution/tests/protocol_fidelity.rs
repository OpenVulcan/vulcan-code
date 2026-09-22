//! End-to-end protocol identity and canonical-history projection contracts.
//! 端到端协议身份与规范历史投影契约。

use super::super::request_encoding::request_with_codex_tool_input_contract;
use super::super::test_support::{
    bundled_manifest_route, deepseek_route, minimal_request, pinned_openrouter_route,
};
use super::super::{ProviderExecutionClient, upstream_protocol_for_profile_id};
use serde_json::{Value, json};
use std::sync::Arc;
use vulcan_ai_contract::{
    AiContinuation, AiMessage, AiStreamEvent, ContentItem, DispatchOutcome, MessageRole,
    ReasoningEffort,
};
use vulcan_ai_protocols::{
    ANTHROPIC_MESSAGES_PROFILE_ID, ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID,
    CLAUDE_CODE_MESSAGES_PROFILE_ID, GEMINI_GENERATE_CONTENT_PROFILE_ID,
    GEMINI_INTERACTIONS_PROFILE_ID, GOOGLE_CODE_ASSIST_PROFILE_ID, OPENAI_CHAT_PROFILE_ID,
    OPENAI_CODEX_RESPONSES_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID, ProtocolRegistry,
};
use vulcan_llm_core::UpstreamProtocol;

/// Verifies one declared Lite model projects only at the native Codex egress boundary.
/// 验证一项已声明 Lite 模型只在原生 Codex 出站边界投影。
#[test]
fn codex_lite_projection_matches_stable_prefix_and_namespace_contract() {
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    let mut route = pinned_openrouter_route(OPENAI_RESPONSES_PROFILE_ID);
    route.protocol_route.protocol_profile_id = String::from(OPENAI_CODEX_RESPONSES_PROFILE_ID);
    route.codex_behavior = Some(crate::provider_manifest::CodexModelBehavior {
        wire_dialect: crate::provider_manifest::CodexWireDialect::Lite,
        apply_patch_tool_type: crate::provider_manifest::CodexApplyPatchToolType::Freeform,
        supports_parallel_tool_calls: true,
        supports_verbosity: true,
        default_verbosity: Some(crate::provider_manifest::CodexTextVerbosity::Low),
        ..Default::default()
    });
    let mut request = minimal_request();
    request.cache_scope_id = Some(String::from("00000000-0000-0000-0000-000000000123"));
    request.messages.insert(
        0,
        AiMessage {
            role: MessageRole::System,
            content: vec![ContentItem::Text {
                text: String::from("system contract"),
            }],
            provider_context_items: Vec::new(),
        },
    );
    request.tools.push(vulcan_ai_contract::ToolDefinition {
        name: String::from(vulcan_ai_contract::APPLY_PATCH_TOOL_NAME),
        description: String::from("Apply one patch"),
        input_schema: json!({"type":"object","properties":{"patch":{"type":"string"}}}),
        input_format: Default::default(),
        strict: false,
    });

    let first = client
        .encode_request(&route, &request)
        .expect("Lite route should encode");
    let second = client
        .encode_request(&route, &request)
        .expect("repeated Lite route should encode");

    assert!(first.body.get("instructions").is_none());
    assert!(first.body.get("tools").is_none());
    assert_eq!(first.body["parallel_tool_calls"], false);
    assert_eq!(first.body["reasoning"]["context"], "all_turns");
    assert_eq!(first.body["text"]["verbosity"], "low");
    assert_eq!(first.body["input"][0]["type"], "additional_tools");
    assert_eq!(first.body["input"][0]["role"], "developer");
    assert_eq!(first.body["input"][1]["type"], "message");
    assert_eq!(first.body["input"][1]["role"], "developer");
    assert_eq!(
        first.body["input"][1]["content"][0]["text"],
        "system contract"
    );
    assert_eq!(
        first.body["input"][0]["id"], second.body["input"][0]["id"],
        "tool prefix identity must remain stable"
    );
    assert_eq!(
        first.body["input"][1]["id"], second.body["input"][1]["id"],
        "instruction prefix identity must remain stable"
    );
    let namespace = &first.body["input"][0]["tools"][0];
    assert_eq!(namespace["type"], "namespace");
    assert_eq!(namespace["name"], "functions");
    assert_eq!(namespace["tools"][0]["type"], "custom");
    assert_eq!(namespace["tools"][0]["name"], "apply_patch");
    assert_eq!(namespace["tools"][0]["format"]["syntax"], "lark");

    // effectiveRequest is the same immutable snapshot consumed by live HTTP and WebSocket decoders.
    // effectiveRequest 是真实 HTTP 与 WebSocket 解码器共同消费的同一不可变快照。
    let effective_request = request_with_codex_tool_input_contract(&route, &request);
    let registry = ProtocolRegistry::first_batch().expect("profiles should register");
    let mut decoder = registry
        .require(OPENAI_CODEX_RESPONSES_PROFILE_ID)
        .expect("Codex Responses profile should register")
        .create_stream_decoder(String::from("execution"), &effective_request);
    decoder
        .push_sse(
            Some("response.output_item.added"),
            r#"{"type":"response.output_item.added","output_index":0,"item":{"type":"custom_tool_call","id":"item","call_id":"call","name":"apply_patch","input":""}}"#,
        )
        .expect("freeform Apply Patch should start through the production registry");
    let patch = "*** Begin Patch\n*** Add File: note.txt\n+done\n*** End Patch";
    let completed = decoder
        .push_sse(
            Some("response.output_item.done"),
            serde_json::to_string(&json!({
                "type": "response.output_item.done",
                "output_index": 0,
                "item": {
                    "type": "custom_tool_call",
                    "id": "item",
                    "call_id": "call",
                    "name": "apply_patch",
                    "input": patch,
                },
            }))
            .expect("completion should serialize")
            .as_str(),
        )
        .expect("freeform Apply Patch should complete through the production registry");
    assert!(completed.iter().any(|event| matches!(
        event,
        AiStreamEvent::ToolCallCompleted { call, .. }
            if call.name == "apply_patch" && call.arguments == json!({"patch": patch})
    )));
}

/// Verifies the production Responses-first encoder carries Apply Patch schemas and replay across every protocol family.
/// 验证生产 Responses 优先编码链在所有协议族中保留 Apply Patch Schema 与历史重放。
/// No network is used; assertions cover fresh calls, canonical history and legacy custom history.
/// 不使用网络；断言覆盖首次调用、规范历史与旧 custom 历史。
#[test]
fn apply_patch_standard_function_survives_production_protocol_projection() {
    // client invokes the same public encoding boundary as live provider requests.
    // client 调用与真实供应商请求相同的公共编码边界。
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    // schema is the existing single-patch JSON contract, including its closed object boundary.
    // schema 是既有单 patch JSON 契约，包括封闭对象边界。
    let schema = json!({"type":"object","properties":{"patch":{"type":"string"}},"required":["patch"],"additionalProperties":false});
    // patch exercises escaping, Unicode and newlines without applying filesystem changes.
    // patch 验证转义、中文和换行，不实际修改文件系统。
    let patch =
        "*** Begin Patch\n*** Add File: note.txt\n+你好 \"quoted\" C:\\workspace\n*** End Patch";
    // cases use converter-owned JSON pointers rather than guessing equivalent field names.
    // cases 使用转换器持有的 JSON Pointer，不猜测等价字段名。
    let cases = [
        (
            OPENAI_RESPONSES_PROFILE_ID,
            "/tools/0/parameters",
            "/input/1/arguments",
            "/input/2/call_id",
        ),
        (
            OPENAI_CODEX_RESPONSES_PROFILE_ID,
            "/tools/0/parameters",
            "/input/1/arguments",
            "/input/2/call_id",
        ),
        (
            OPENAI_CHAT_PROFILE_ID,
            "/tools/0/function/parameters",
            "/messages/1/tool_calls/0/function/arguments",
            "/messages/2/tool_call_id",
        ),
        (
            ANTHROPIC_MESSAGES_PROFILE_ID,
            "/tools/0/input_schema",
            "/messages/1/content/0/input",
            "/messages/2/content/0/tool_use_id",
        ),
        (
            ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID,
            "/tools/0/input_schema",
            "/messages/1/content/0/input",
            "/messages/2/content/0/tool_use_id",
        ),
        (
            CLAUDE_CODE_MESSAGES_PROFILE_ID,
            "/tools/0/input_schema",
            "/messages/1/content/0/input",
            "/messages/2/content/0/tool_use_id",
        ),
        (
            GEMINI_GENERATE_CONTENT_PROFILE_ID,
            "/tools/0/functionDeclarations/0/parameters",
            "/contents/1/parts/0/functionCall/args",
            "/contents/2/parts/0/functionResponse/id",
        ),
        (
            GEMINI_INTERACTIONS_PROFILE_ID,
            "/tools/0/parameters",
            "/input/1/arguments",
            "/input/2/call_id",
        ),
        (
            GOOGLE_CODE_ASSIST_PROFILE_ID,
            "/request/tools/0/functionDeclarations/0/parameters",
            "/request/contents/1/parts/0/functionCall/args",
            "/request/contents/2/parts/0/functionResponse/id",
        ),
    ];
    for (profile_id, schema_pointer, arguments_pointer, output_id_pointer) in cases {
        // route is an isolated encoding fixture, not a change to any provider manifest.
        // route 是隔离的编码夹具，不修改任何供应商清单。
        let mut route = pinned_openrouter_route(OPENAI_CHAT_PROFILE_ID);
        route.protocol_route.protocol_profile_id = profile_id.to_string();
        let mut request = minimal_request();
        request.tools = vec![vulcan_ai_contract::ToolDefinition {
            name: String::from("apply_patch"),
            description: String::from("Apply an atomic patch."),
            input_schema: schema.clone(),
            input_format: Default::default(),
            strict: false,
        }];
        let fresh = client
            .encode_request(&route, &request)
            .unwrap_or_else(|error| panic!("fresh {profile_id}: {error:?}"));
        assert_eq!(
            fresh.body.pointer(schema_pointer),
            Some(&schema),
            "{profile_id}"
        );
        for legacy in [false, true] {
            // replay starts from the same user prompt with either canonical or persisted custom call history.
            // replay 从同一用户提示开始，分别携带规范调用或持久化 custom 调用历史。
            let mut replay = request.clone();
            replay.messages.push(AiMessage {
                role: MessageRole::Assistant,
                content: if legacy { Vec::new() } else { vec![ContentItem::ToolCall {
                    call: vulcan_ai_contract::ToolCall {
                        id: String::from("patch-1"), name: String::from("apply_patch"),
                        arguments: json!({"patch":patch}), signature: None,
                    },
                }] },
                provider_context_items: if legacy { vec![json!({"type":"custom_tool_call","id":"legacy-item","call_id":"patch-1","name":"apply_patch","input":patch})] } else { Vec::new() },
            });
            replay.messages.push(AiMessage {
                role: MessageRole::Tool,
                content: vec![ContentItem::ToolResult {
                    result: vulcan_ai_contract::ToolResult {
                        call_id: String::from("patch-1"),
                        is_error: false,
                        content: json!({"ok":true}),
                    },
                }],
                provider_context_items: Vec::new(),
            });
            let encoded = client
                .encode_request(&route, &replay)
                .unwrap_or_else(|error| panic!("replay {profile_id}, legacy={legacy}: {error:?}"));
            assert_eq!(
                encoded.body.pointer(schema_pointer),
                Some(&schema),
                "{profile_id}"
            );
            let arguments = encoded
                .body
                .pointer(arguments_pointer)
                .expect("tool arguments must survive");
            // Responses and Chat serialize arguments, while Anthropic and Gemini use JSON objects.
            // Responses 与 Chat 序列化参数，Anthropic 与 Gemini 使用 JSON 对象。
            let arguments = if matches!(
                profile_id,
                OPENAI_RESPONSES_PROFILE_ID
                    | OPENAI_CODEX_RESPONSES_PROFILE_ID
                    | OPENAI_CHAT_PROFILE_ID
            ) {
                serde_json::from_str::<Value>(arguments.as_str().expect("serialized arguments"))
                    .unwrap()
            } else {
                arguments.clone()
            };
            assert_eq!(arguments, json!({"patch":patch}), "{profile_id}");
            assert_eq!(
                encoded.body.pointer(output_id_pointer),
                Some(&json!("patch-1")),
                "{profile_id}"
            );
            assert!(
                !encoded.body.to_string().contains("custom_tool_call"),
                "{profile_id}"
            );
        }
    }
}

/// Verifies all production conversion families preserve canonical role semantics and reversible Responses history.
/// 验证全部生产转换协议族保留规范角色语义，并可从原历史重新投影为 Responses。
#[test]
fn protocol_roles_preserve_instruction_and_tool_identity_across_switches() {
    // client exercises the real Responses-first boundary instead of standalone encoders only.
    // client 验证真实 Responses 优先边界，而不只测试独立编码器。
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    // request includes each of the five canonical roles and a complete assistant/tool pair.
    // request 包含全部五类规范角色，以及完整的助手调用与工具结果配对。
    let mut request = minimal_request();
    request.messages.insert(
        0,
        AiMessage {
            role: MessageRole::System,
            content: vec![ContentItem::Text {
                text: String::from("root-policy"),
            }],
            provider_context_items: Vec::new(),
        },
    );
    request.messages.insert(
        1,
        AiMessage {
            role: MessageRole::Developer,
            content: vec![
                ContentItem::Text {
                    text: String::from("developer-policy-a"),
                },
                ContentItem::Text {
                    text: String::from("developer-policy-b"),
                },
            ],
            provider_context_items: Vec::new(),
        },
    );
    request.messages.extend([
        AiMessage {
            role: MessageRole::Assistant,
            content: vec![
                ContentItem::Text {
                    text: String::from("assistant-answer"),
                },
                ContentItem::ToolCall {
                    call: vulcan_ai_contract::ToolCall {
                        id: String::from("role-call-1"),
                        name: String::from("lookup"),
                        arguments: json!({}),
                        signature: None,
                    },
                },
            ],
            provider_context_items: Vec::new(),
        },
        AiMessage {
            role: MessageRole::Tool,
            content: vec![ContentItem::ToolResult {
                result: vulcan_ai_contract::ToolResult {
                    call_id: String::from("role-call-1"),
                    is_error: false,
                    content: json!("tool-result"),
                },
            }],
            provider_context_items: Vec::new(),
        },
    ]);
    // original proves protocol selection never overwrites original roles or tool identities.
    // original 证明协议选择不会覆盖原始角色或工具身份。
    let original = request.messages.clone();
    let mut route = pinned_openrouter_route(OPENAI_CHAT_PROFILE_ID);
    for profile in [
        OPENAI_CHAT_PROFILE_ID,
        ANTHROPIC_MESSAGES_PROFILE_ID,
        ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID,
        CLAUDE_CODE_MESSAGES_PROFILE_ID,
        GEMINI_GENERATE_CONTENT_PROFILE_ID,
        GEMINI_INTERACTIONS_PROFILE_ID,
        GOOGLE_CODE_ASSIST_PROFILE_ID,
        OPENAI_RESPONSES_PROFILE_ID,
        OPENAI_CODEX_RESPONSES_PROFILE_ID,
    ] {
        route.protocol_route.protocol_profile_id = profile.to_string();
        // encoded retains every semantic marker while applying the target's exact role vocabulary.
        // encoded 保留全部语义标记，同时使用目标协议的精确角色词汇。
        let encoded = client
            .encode_request(&route, &request)
            .expect("role projection");
        for marker in [
            "root-policy",
            "developer-policy-a",
            "developer-policy-b",
            "assistant-answer",
            "role-call-1",
            "tool-result",
            "hello",
        ] {
            assert!(
                encoded.body.to_string().contains(marker),
                "{profile} lost {marker}"
            );
        }
        match upstream_protocol_for_profile_id(profile).expect("production family") {
            UpstreamProtocol::OpenAiChatCompletions => {
                assert_eq!(encoded.body["messages"][0]["role"], "system");
                assert_eq!(
                    encoded.body["messages"][0]["content"],
                    "root-policy\n\ndeveloper-policy-a\ndeveloper-policy-b"
                );
                assert!(encoded.body["messages"].as_array().unwrap().iter().all(
                    |message| matches!(
                        message["role"].as_str(),
                        Some("system" | "user" | "assistant" | "tool")
                    )
                ));
                assert_eq!(encoded.body["messages"][3]["tool_call_id"], "role-call-1");
            }
            UpstreamProtocol::AnthropicMessages => {
                assert!(
                    encoded.body["system"]
                        .as_str()
                        .unwrap()
                        .contains("developer-policy-a")
                );
                assert!(
                    encoded.body["messages"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .all(|message| matches!(
                            message["role"].as_str(),
                            Some("user" | "assistant")
                        ))
                );
                assert_eq!(
                    encoded.body["messages"][2]["content"][0]["tool_use_id"],
                    "role-call-1"
                );
            }
            UpstreamProtocol::GeminiGenerateContent | UpstreamProtocol::GoogleCodeAssist => {
                // Code Assist wraps the same GenerateContent message contract in request.
                // Code Assist 在 request 内封装相同的 GenerateContent 消息契约。
                let body = if profile == GOOGLE_CODE_ASSIST_PROFILE_ID {
                    &encoded.body["request"]
                } else {
                    &encoded.body
                };
                assert_eq!(
                    body["systemInstruction"]["parts"].as_array().unwrap().len(),
                    3
                );
                assert!(
                    body["contents"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .all(|message| matches!(message["role"].as_str(), Some("user" | "model")))
                );
                assert_eq!(
                    body["contents"][2]["parts"][0]["functionResponse"]["id"],
                    "role-call-1"
                );
            }
            UpstreamProtocol::GeminiInteractions => {
                assert!(
                    encoded.body["system_instruction"]
                        .as_str()
                        .unwrap()
                        .contains("developer-policy-b")
                );
                assert_eq!(encoded.body["input"][0]["type"], "user_input");
                assert_eq!(encoded.body["input"][1]["type"], "model_output");
                assert_eq!(encoded.body["input"][2]["type"], "function_call");
                assert_eq!(encoded.body["input"][3]["type"], "function_result");
            }
            UpstreamProtocol::OpenAiResponses => {
                assert_eq!(encoded.body["input"][0]["role"], "developer");
                assert_eq!(encoded.body["input"][3]["type"], "function_call");
                assert_eq!(encoded.body["input"][4]["type"], "function_call_output");
            }
        }
        assert_eq!(request.messages, original);
    }
    // Returning to Responses starts from canonical messages, never by guessing merged system text.
    // 切回 Responses 从规范消息开始，绝不从合并后的 system 文本猜测原角色。
    route.protocol_route.protocol_profile_id = String::from(OPENAI_RESPONSES_PROFILE_ID);
    let restored = client
        .encode_request(&route, &request)
        .expect("restored Responses");
    assert_eq!(restored.body["input"][0]["role"], "developer");
    assert_eq!(
        restored.body["input"][0]["content"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

/// Verifies missing protocol converters fail before dispatch instead of forwarding a Responses body.
/// 验证缺少协议转换器时在发送前失败，而非转发 Responses 正文。
#[test]
fn protocol_roles_reject_missing_conversion_before_dispatch() {
    // client uses the same public boundary as normal provider requests.
    // client 使用与普通供应商请求相同的公共边界。
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    // route is an isolated fixture; registration alone must not imply a canonical converter exists.
    // route 是隔离夹具；仅注册协议不能推断规范转换器存在。
    let mut route = pinned_openrouter_route(OPENAI_CHAT_PROFILE_ID);
    for profile in [
        vulcan_ai_protocols::COHERE_CHAT_PROFILE_ID,
        "unregistered.protocol",
    ] {
        route.protocol_route.protocol_profile_id = profile.to_string();
        // failure must accurately report that no wire payload has been sent.
        // failure 必须准确报告尚未发送任何线路载荷。
        let failure = client
            .encode_request(&route, &minimal_request())
            .expect_err("missing converter");
        assert!(
            failure
                .error
                .message
                .contains("No Responses request converter")
        );
        assert_eq!(
            failure.error.dispatch_outcome,
            DispatchOutcome::NotDispatched
        );
        assert!(!failure.error.retryable);
    }
}

/// Verifies native Gemini routing preserves its dynamic model path and generation controls.
/// 验证原生 Gemini 路由保留动态模型路径与生成控制参数。
#[test]
fn gemini_generate_content_route_uses_native_model_path_and_controls() {
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    let route = bundled_manifest_route("google", "gemini-api", GEMINI_GENERATE_CONTENT_PROFILE_ID);
    let mut request = minimal_request();
    request.model_id = String::from("gemini-3-flash");
    request.output_token_limit = Some(65_536);
    request.reasoning_effort = Some(ReasoningEffort::Medium);

    let encoded = client
        .encode_request(&route, &request)
        .expect("Gemini GenerateContent request should encode");

    assert_eq!(
        encoded.path,
        "/models/gemini-3-flash:streamGenerateContent?alt=sse"
    );
    assert_eq!(
        route.request_url(encoded.path.as_str()),
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-3-flash:streamGenerateContent?alt=sse"
    );
    assert!(encoded.body.get("model").is_none());
    assert_eq!(encoded.body["generationConfig"]["maxOutputTokens"], 65_536);
    assert_eq!(
        encoded.body["generationConfig"]["thinkingConfig"]["thinkingLevel"],
        "MEDIUM"
    );
    assert_eq!(
        encoded.body["generationConfig"]["thinkingConfig"]["includeThoughts"],
        true
    );
}

/// Verifies APIKEY.FUN uses native Gemini execution without moving credential discovery to v1beta.
/// 验证 APIKEY.FUN 使用原生 Gemini 执行入口，同时不把凭据级模型发现迁移到 v1beta。
#[test]
fn apikeyfun_gemini_route_separates_execution_and_discovery_origins() {
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    let route = bundled_manifest_route("apikeyfun", "api", GEMINI_GENERATE_CONTENT_PROFILE_ID);
    let mut request = minimal_request();
    request.model_id = String::from("gemini-3-flash");

    let encoded = client
        .encode_request(&route, &request)
        .expect("APIKEY.FUN Gemini request should encode");

    assert_eq!(
        route.request_url(encoded.path.as_str()),
        "https://api.apikey.fan/v1beta/models/gemini-3-flash:streamGenerateContent?alt=sse"
    );
    assert_eq!(
        route
            .protocol_route
            .model_discovery_url(&route.entry, "/models"),
        "https://api.apikey.fan/v1/models"
    );
}

/// Verifies executable Gemini Interactions routes pass through the Responses conversion core.
/// 验证可执行 Gemini Interactions 路由会经过 Responses 转换核心。
#[test]
fn gemini_interactions_route_uses_responses_baseline_and_preserves_replay() {
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    let mut route = pinned_openrouter_route(OPENAI_CHAT_PROFILE_ID);
    route.protocol_route.protocol_profile_id = String::from(GEMINI_INTERACTIONS_PROFILE_ID);
    route.protocol_route.endpoint_path = Some(String::from("/interactions"));
    let mut request = minimal_request();
    request.messages.push(vulcan_ai_contract::AiMessage {
        role: vulcan_ai_contract::MessageRole::Assistant,
        content: vec![
            ContentItem::Reasoning {
                text: String::from("signed reasoning"),
                signature: Some(String::from("gemini#reasoning-signature")),
            },
            ContentItem::ToolCall {
                call: vulcan_ai_contract::ToolCall {
                    id: String::from("call-1"),
                    name: String::from("lookup"),
                    arguments: json!({"q":"x"}),
                    signature: Some(String::from("gemini#call-signature")),
                },
            },
        ],
        provider_context_items: Vec::new(),
    });
    request.tools.push(vulcan_ai_contract::ToolDefinition {
        name: String::from("lookup"),
        description: String::from("Lookup"),
        input_schema: json!({"type":"object"}),
        input_format: Default::default(),
        strict: false,
    });

    let encoded = client
        .encode_request(&route, &request)
        .expect("Interactions route should project from Responses");

    assert_eq!(encoded.path, "/interactions");
    assert!(encoded.body.get("contents").is_none());
    assert_eq!(encoded.body["input"][1]["type"], "thought");
    assert_eq!(encoded.body["input"][1]["signature"], "reasoning-signature");
    assert_eq!(encoded.body["input"][2]["type"], "function_call");
    assert!(
        encoded.body["input"][2]
            .get("x_vulcan_provider_signature")
            .is_none()
    );
}

/// Verifies the protocol encoder can project a cursor proven by the active WebSocket layer.
/// 验证协议编码器可投影由活动 WebSocket 层证明的游标。
#[test]
fn codex_protocol_projection_preserves_socket_proven_previous_response_id() {
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    let mut route = pinned_openrouter_route(OPENAI_RESPONSES_PROFILE_ID);
    route.protocol_route.protocol_profile_id = String::from(OPENAI_CODEX_RESPONSES_PROFILE_ID);
    let mut request = minimal_request();
    request.continuation = Some(AiContinuation {
        provider_id: route.provider.id.clone(),
        protocol_profile_id: String::from(OPENAI_CODEX_RESPONSES_PROFILE_ID),
        model_id: request.model_id.clone(),
        provider_message_id: String::from("resp-previous"),
        covered_local_message_id: String::from("local-boundary"),
        covered_message_count: 0,
    });

    let encoded = client
        .encode_request(&route, &request)
        .expect("Responses protocol should retain the socket-proven cursor");

    assert_eq!(encoded.body["previous_response_id"], "resp-previous");
    assert_eq!(
        encoded.body["input"]
            .as_array()
            .expect("Responses input should remain an array")
            .len(),
        1
    );
}

// Verifies that local continuation anchors never become Codex cursors on non-Codex routes.
// 验证非 Codex 路由上的本地延续锚点绝不会被投影为 Codex 游标。
#[test]
fn non_codex_routes_replay_history_without_projecting_codex_continuation() {
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );

    for protocol_profile_id in [ANTHROPIC_MESSAGES_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID] {
        let route = deepseek_route(protocol_profile_id);
        let mut request = minimal_request();
        request.model_id = if protocol_profile_id == ANTHROPIC_MESSAGES_PROFILE_ID {
            String::from("deepseek-v4-pro")
        } else {
            String::from("deepseek-v4-flash")
        };
        request.continuation = Some(AiContinuation {
            provider_id: route.provider.id.clone(),
            protocol_profile_id: String::from(protocol_profile_id),
            model_id: request.model_id.clone(),
            provider_message_id: String::from("provider-message-1"),
            covered_local_message_id: String::from("local-boundary-1"),
            covered_message_count: 1,
        });

        let encoded = client
            .encode_request(&route, &request)
            .expect("non-Codex routes should encode a full-history follow-up");

        assert!(encoded.body.get("previous_response_id").is_none());
    }
}
