//! Production request regressions for model-visible User host message envelopes.
//! 面向模型的 User 宿主消息信封生产请求回归测试。

use super::super::ProviderExecutionClient;
use super::super::test_support::pinned_openrouter_route;
use serde_json::{Value, json};
use std::sync::Arc;
use vulcan_ai_contract::{
    AiExecutionRequest, AiMessage, ContentItem, HostMessageEnvelope, HostMessageKind, MessageRole,
    ResourceKind, ResourceReference, ResourceSourceKind, ToolCall, ToolDefinition, ToolResult,
};
use vulcan_ai_protocols::{
    ANTHROPIC_MESSAGES_PROFILE_ID, ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID,
    CLAUDE_CODE_MESSAGES_PROFILE_ID, GEMINI_GENERATE_CONTENT_PROFILE_ID,
    GEMINI_INTERACTIONS_PROFILE_ID, GOOGLE_CODE_ASSIST_PROFILE_ID, OPENAI_CHAT_PROFILE_ID,
    OPENAI_CODEX_RESPONSES_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID, ProtocolRegistry,
};

/// Stable top content used to prove dynamic events never move into the fixed prefix.
/// 用于证明动态事件不会移动到固定前缀的稳定顶部正文。
const STABLE_TOP: &str = "stable-top-policy";
/// First dynamic event body whose historical position must remain stable.
/// 第一条必须保持历史位置稳定的动态事件正文。
const DYNAMIC_A: &str = "dynamic-history-a";
/// Second dynamic event body appended after the tool result.
/// 追加在工具结果之后的第二条动态事件正文。
const DYNAMIC_B: &str = "dynamic-history-b";

/// Builds the same execution client used by live HTTP and WebSocket dispatch.
/// 构建与真实 HTTP 和 WebSocket 分派使用相同的执行客户端。
fn client() -> ProviderExecutionClient {
    ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("registered profiles")),
    )
}

/// Renders one host-authored dynamic event before it enters the provider-neutral User message.
/// 在进入供应商无关 User 消息前渲染一条宿主编写的动态事件。
fn rendered_host_message(event_id: &str, kind: HostMessageKind, body: &str) -> String {
    HostMessageEnvelope {
        event_id: event_id.to_string(),
        kind,
        source: String::from("source-session"),
        target: String::from("target-session"),
        task_id: Some(String::from("task-1")),
        question_id: None,
        scope: String::from("task"),
        body: body.to_string(),
    }
    .render()
    .expect("host message fixture must satisfy the closed envelope contract")
}

/// Creates one inline image that must stay a native media part on every covered target.
/// 创建一张必须在所有覆盖目标上保持原生媒体分片的内联图片。
fn inline_image() -> ResourceReference {
    ResourceReference {
        id: String::from("media-1"),
        kind: ResourceKind::Image,
        media_type: Some(String::from("image/png")),
        source_kind: ResourceSourceKind::InlineData,
        file_name: None,
        image_detail: None,
        resource_path: None,
        local_path: None,
        url: Some(String::from("data:image/png;base64,aGVsbG8=")),
    }
}

/// Builds a canonical history with dynamic User events around an intact assistant/tool pair.
/// 构建在完整助手/工具配对前后插入动态 User 事件的规范历史。
fn request_with_host_messages() -> AiExecutionRequest {
    let mut request = super::super::test_support::minimal_request();
    request.tools = vec![ToolDefinition {
        name: String::from("lookup"),
        description: String::from("Look up one value."),
        input_schema: json!({
            "type": "object",
            "properties": {"path": {"type": "string"}},
            "required": ["path"],
            "additionalProperties": false,
        }),
        input_format: Default::default(),
        strict: false,
    }];
    request.messages = vec![
        AiMessage {
            role: MessageRole::System,
            content: vec![ContentItem::Text {
                text: String::from(STABLE_TOP),
            }],
            provider_context_items: Vec::new(),
        },
        AiMessage {
            role: MessageRole::User,
            content: vec![ContentItem::Text {
                text: rendered_host_message("event-a", HostMessageKind::TaskSteer, DYNAMIC_A),
            }],
            provider_context_items: Vec::new(),
        },
        AiMessage {
            role: MessageRole::User,
            content: vec![
                ContentItem::Text {
                    text: String::from("ordinary-user"),
                },
                ContentItem::Resource {
                    resource: inline_image(),
                },
            ],
            provider_context_items: Vec::new(),
        },
        AiMessage {
            role: MessageRole::Assistant,
            content: vec![
                ContentItem::Text {
                    text: String::from("assistant-before-tool"),
                },
                ContentItem::ToolCall {
                    call: ToolCall {
                        id: String::from("call-1"),
                        name: String::from("lookup"),
                        arguments: json!({"path": "workspace/file.txt"}),
                        signature: None,
                    },
                },
            ],
            provider_context_items: Vec::new(),
        },
        AiMessage {
            role: MessageRole::Tool,
            content: vec![ContentItem::ToolResult {
                result: ToolResult {
                    call_id: String::from("call-1"),
                    is_error: false,
                    content: json!("tool-result"),
                },
            }],
            provider_context_items: Vec::new(),
        },
        AiMessage {
            role: MessageRole::User,
            content: vec![ContentItem::Text {
                text: rendered_host_message("event-b", HostMessageKind::ContextUpdate, DYNAMIC_B),
            }],
            provider_context_items: Vec::new(),
        },
        AiMessage {
            role: MessageRole::Assistant,
            content: vec![ContentItem::Text {
                text: String::from("final-assistant"),
            }],
            provider_context_items: Vec::new(),
        },
    ];
    request
}

/// Returns whether a JSON value contains one exact scalar string.
/// 返回 JSON 值是否包含一个完全相等的标量字符串。
fn contains_exact_string(value: &Value, expected: &str) -> bool {
    match value {
        Value::String(actual) => actual == expected,
        Value::Array(values) => values
            .iter()
            .any(|value| contains_exact_string(value, expected)),
        Value::Object(values) => values
            .values()
            .any(|value| contains_exact_string(value, expected)),
        Value::Null | Value::Bool(_) | Value::Number(_) => false,
    }
}

/// Reads the fixed top projection used by one exact production route.
/// 读取一条精确生产路线使用的固定顶部投影。
fn fixed_top_text<'a>(profile: &str, body: &'a Value) -> &'a str {
    let pointer = match profile {
        OPENAI_CHAT_PROFILE_ID => "/messages/0/content",
        OPENAI_RESPONSES_PROFILE_ID | OPENAI_CODEX_RESPONSES_PROFILE_ID => "/instructions",
        ANTHROPIC_MESSAGES_PROFILE_ID
        | ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID
        | CLAUDE_CODE_MESSAGES_PROFILE_ID => "/system",
        GEMINI_GENERATE_CONTENT_PROFILE_ID => "/systemInstruction/parts/0/text",
        GEMINI_INTERACTIONS_PROFILE_ID => "/system_instruction",
        GOOGLE_CODE_ASSIST_PROFILE_ID => "/request/systemInstruction/parts/0/text",
        _ => panic!("unregistered test profile {profile}"),
    };
    body.pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("{profile} missing fixed top projection at {pointer}"))
}

/// Asserts exact route-specific tool identity and role vocabulary.
/// 断言精确线路的工具身份与角色词汇。
fn assert_legal_route_output(profile: &str, body: &Value) {
    match profile {
        OPENAI_CHAT_PROFILE_ID => {
            let messages = body["messages"].as_array().expect("Chat messages");
            assert!(messages.iter().all(|message| {
                matches!(
                    message["role"].as_str(),
                    Some("system" | "user" | "assistant" | "tool")
                )
            }));
            assert!(messages.iter().any(|message| {
                message["role"] == "assistant"
                    && message["tool_calls"]
                        .as_array()
                        .is_some_and(|calls| calls.iter().any(|call| call["id"] == "call-1"))
            }));
            assert!(messages.iter().any(|message| {
                message["role"] == "tool" && message["tool_call_id"] == "call-1"
            }));
        }
        OPENAI_RESPONSES_PROFILE_ID | OPENAI_CODEX_RESPONSES_PROFILE_ID => {
            let input = body["input"].as_array().expect("Responses input");
            assert!(
                input
                    .iter()
                    .any(|item| { item["type"] == "function_call" && item["call_id"] == "call-1" })
            );
            assert!(input.iter().any(|item| {
                item["type"] == "function_call_output" && item["call_id"] == "call-1"
            }));
        }
        ANTHROPIC_MESSAGES_PROFILE_ID
        | ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID
        | CLAUDE_CODE_MESSAGES_PROFILE_ID => {
            let messages = body["messages"].as_array().expect("Anthropic messages");
            assert!(
                messages.iter().all(|message| {
                    matches!(message["role"].as_str(), Some("user" | "assistant"))
                })
            );
            assert!(messages.iter().any(|message| {
                message["content"].as_array().is_some_and(|blocks| {
                    blocks
                        .iter()
                        .any(|block| block["type"] == "tool_use" && block["id"] == "call-1")
                })
            }));
            assert!(messages.iter().any(|message| {
                message["content"].as_array().is_some_and(|blocks| {
                    blocks.iter().any(|block| {
                        block["type"] == "tool_result" && block["tool_use_id"] == "call-1"
                    })
                })
            }));
        }
        GEMINI_GENERATE_CONTENT_PROFILE_ID | GOOGLE_CODE_ASSIST_PROFILE_ID => {
            let body = if profile == GOOGLE_CODE_ASSIST_PROFILE_ID {
                &body["request"]
            } else {
                body
            };
            let contents = body["contents"].as_array().expect("Gemini contents");
            assert!(
                contents
                    .iter()
                    .all(|content| matches!(content["role"].as_str(), Some("user" | "model")))
            );
            assert!(contents.iter().any(|content| {
                content["parts"].as_array().is_some_and(|parts| {
                    parts
                        .iter()
                        .any(|part| part["functionCall"]["id"] == "call-1")
                })
            }));
            assert!(contents.iter().any(|content| {
                content["parts"].as_array().is_some_and(|parts| {
                    parts
                        .iter()
                        .any(|part| part["functionResponse"]["id"] == "call-1")
                })
            }));
        }
        GEMINI_INTERACTIONS_PROFILE_ID => {
            let input = body["input"].as_array().expect("Gemini Interactions input");
            assert!(
                input
                    .iter()
                    .any(|item| item["type"] == "function_call" && item["call_id"] == "call-1")
            );
            assert!(
                input.iter().any(|item| {
                    item["type"] == "function_result" && item["call_id"] == "call-1"
                })
            );
        }
        _ => panic!("unregistered test profile {profile}"),
    }
}

/// Verifies the real provider adapter path keeps host events as ordered User text for every route family.
/// 验证真实供应商适配器路径在所有线路族中将宿主事件保持为有序 User 文本。
#[test]
fn production_host_user_envelopes_preserve_top_prefix_and_tool_pairs() {
    let client = client();
    let profiles = [
        OPENAI_CHAT_PROFILE_ID,
        OPENAI_RESPONSES_PROFILE_ID,
        OPENAI_CODEX_RESPONSES_PROFILE_ID,
        ANTHROPIC_MESSAGES_PROFILE_ID,
        ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID,
        CLAUDE_CODE_MESSAGES_PROFILE_ID,
        GEMINI_GENERATE_CONTENT_PROFILE_ID,
        GEMINI_INTERACTIONS_PROFILE_ID,
        GOOGLE_CODE_ASSIST_PROFILE_ID,
    ];

    for profile in profiles {
        let base_profile = if profile == OPENAI_CODEX_RESPONSES_PROFILE_ID {
            OPENAI_RESPONSES_PROFILE_ID
        } else {
            OPENAI_CHAT_PROFILE_ID
        };
        let mut route = pinned_openrouter_route(base_profile);
        route.protocol_route.protocol_profile_id = profile.to_string();
        let request = request_with_host_messages();
        let before = request.clone();
        let encoded = client
            .encode_request(&route, &request)
            .unwrap_or_else(|error| panic!("{profile} production encoding failed: {error:?}"));
        let body = &encoded.body;
        let wire = body.to_string();

        // Only the explicit stable System source may reach the fixed top projection.
        // 只有明确的稳定 System 来源可以进入固定顶部投影。
        let top = fixed_top_text(profile, body);
        assert_eq!(top, STABLE_TOP, "{profile} changed the fixed top");
        assert!(
            !top.contains(DYNAMIC_A),
            "{profile} moved event A into the top"
        );
        assert!(
            !top.contains(DYNAMIC_B),
            "{profile} moved event B into the top"
        );
        assert!(
            !top.contains("vulcan_"),
            "{profile} put a host tag in the top"
        );

        // Each logical host event is rendered exactly once; ordinary content cannot acquire a host wrapper.
        // 每个逻辑宿主事件只渲染一次；普通内容不能获得宿主包装。
        assert_eq!(
            wire.matches("<vulcan_agent_message ").count(),
            1,
            "{profile}"
        );
        assert_eq!(
            wire.matches("<vulcan_context_update ").count(),
            1,
            "{profile}"
        );
        assert!(contains_exact_string(body, "ordinary-user"), "{profile}");
        assert!(
            contains_exact_string(body, "assistant-before-tool"),
            "{profile}"
        );
        assert!(contains_exact_string(body, "final-assistant"), "{profile}");
        assert!(
            wire.contains("aGVsbG8="),
            "{profile} lost the native image data"
        );

        // The serialized order is the production order after all Responses-to-target conversion.
        // 序列化顺序是完成 Responses 到目标线路转换后的真实生产顺序。
        let markers = [
            DYNAMIC_A,
            "ordinary-user",
            "assistant-before-tool",
            "call-1",
            "tool-result",
            DYNAMIC_B,
            "final-assistant",
        ];
        let positions = markers
            .iter()
            .map(|marker| {
                wire.find(marker)
                    .unwrap_or_else(|| panic!("{profile} lost ordered marker {marker}"))
            })
            .collect::<Vec<_>>();
        assert!(
            positions.windows(2).all(|pair| pair[0] < pair[1]),
            "{profile} reordered host text or interrupted the tool pair: {positions:?}"
        );

        assert_legal_route_output(profile, body);
        assert_eq!(request, before, "{profile} mutated canonical history");
    }
}
