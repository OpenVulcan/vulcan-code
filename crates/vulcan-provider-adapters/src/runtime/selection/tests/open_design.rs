//! Production request regressions for OpenDesign's verified Chat, Responses and Messages routes.
//! OpenDesign 已验证 Chat、Responses 与 Messages 线路的生产请求回归测试。

use crate::execution::ProviderExecutionClient;
use crate::execution::test_support::minimal_request;
use crate::runtime::PublicAiRuntime;
use crate::runtime::test_support::{connection, install_source_catalog, with_protocol_variants};
use crate::{PersistedAiConfiguration, parse_openai_model_catalog};
use serde_json::{Value, json};
use std::sync::Arc;
use vulcan_ai_contract::{
    ContentItem, MessageRole, ReasoningEffort, ResourceKind, ResourceReference, ResourceSourceKind,
};
use vulcan_llm_core::{
    ANTHROPIC_MESSAGES_PROFILE_ID, OPENAI_CHAT_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID,
    ProtocolRegistry,
};

/// Returns an offline client using the registered production encoders; takes no parameters.
/// 返回使用已注册生产编码器的离线客户端；无参数。
fn client() -> ProviderExecutionClient {
    ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("registered profiles")),
    )
}

/// Activates an isolated runtime for the supplied discovered `model_ids`; returns its configuration too.
/// 为传入的已发现 `model_ids` 激活隔离运行时，同时返回对应配置。
fn open_design_runtime(model_ids: &[&str]) -> (PublicAiRuntime, PersistedAiConfiguration) {
    // Load the bundled manifest through the same registry as production startup.
    // 通过与生产启动相同的注册表加载内置清单。
    let runtime = PublicAiRuntime::unavailable().expect("isolated provider runtime");
    // This in-memory connection contains references only and never reads a real credential.
    // 内存连接只包含引用，绝不读取真实凭据。
    let mut account = connection("open-design-test");
    account.provider_id = String::from("open-design");
    account.service_entry_id = String::from("design-plan");
    // Enable only the supplied synthetic discovery results for the test account.
    // 仅为测试账号启用传入的合成发现结果。
    let mut configuration = PersistedAiConfiguration::default();
    configuration
        .connection_graph
        .add_connection(&runtime.providers().expect("provider registry"), account)
        .expect("OpenDesign connection");
    for model_id in model_ids {
        configuration.set_model_enabled("open-design", model_id, true);
    }
    // Discovery contributes identities; the compiled manifest owns executable capabilities.
    // 发现只提供身份，可执行能力由已编译清单持有。
    let discovery = json!({
        "data": model_ids.iter().map(|model_id| json!({"id": model_id})).collect::<Vec<_>>()
    });
    // Discovery offers every service route; the manifest rules then keep only each model's own protocol.
    // 发现为每条服务线路提供变体，随后清单规则只保留各模型自身的协议。
    let catalog = with_protocol_variants(
        parse_openai_model_catalog("open-design-test", 1, &discovery.to_string())
            .expect("synthetic discovery catalog"),
        &[
            OPENAI_CHAT_PROFILE_ID,
            OPENAI_RESPONSES_PROFILE_ID,
            ANTHROPIC_MESSAGES_PROFILE_ID,
        ],
    );
    install_source_catalog(&runtime, catalog.clone());
    runtime
        .replace_catalog(catalog)
        .expect("catalog installation");
    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("manifest-backed catalog activation");
    (runtime, configuration)
}

/// Checks model-scoped reasoning projection and streaming fields; returns unit on success.
/// 检查模型作用域的思考投影与流式字段；验证成功返回单元值。
#[test]
fn open_design_projects_reasoning_without_affecting_other_models() {
    // One account must resolve an independent frozen projection for each model.
    // 同一账号必须为每个模型解析独立冻结的投影。
    let (runtime, configuration) = open_design_runtime(&["deepseek-v4.1-flash", "mimo-v2.6-pro"]);
    // Reuse the production encoder without dispatching any paid requests.
    // 复用生产编码器，不发送任何付费请求。
    let client = client();
    // Each case checks a wire behavior, not a snapshot of the model directory.
    // 每个用例检查线路行为，不锁定模型目录内容快照。
    let cases = [
        ("deepseek-v4.1-flash", ReasoningEffort::High, Some("high")),
        ("mimo-v2.6-pro", ReasoningEffort::None, Some("none")),
    ];
    for (model_id, effort, wire_effort) in cases {
        // Production selection applies model-owned profile references to the route.
        // 生产选择链把模型持有的 Profile 引用应用到线路。
        let selection = runtime
            .resolve_connection_model_selection(
                &configuration,
                "open-design-test",
                "open-design",
                model_id,
            )
            .expect("model selection must resolve from the manifest");
        assert_eq!(
            selection.route.protocol_route.protocol_profile_id,
            OPENAI_CHAT_PROFILE_ID
        );
        // Model identity and user effort enter through the public execution request.
        // 模型身份与用户思考档位通过公共执行请求进入。
        let mut request = minimal_request();
        request.model_id = model_id.to_string();
        request.reasoning_effort = Some(effort);
        // Exercise the same profile selection and projection used before HTTP dispatch.
        // 执行 HTTP 分派前使用的同一 Profile 选择和投影流程。
        let encoded = client
            .encode_request(&selection.route, &request)
            .unwrap_or_else(|error| panic!("{model_id}: {error:?}"));
        assert_eq!(
            encoded.body.get("reasoning_effort"),
            wire_effort.map(Value::from).as_ref(),
            "reasoning projection for {model_id}"
        );
        assert_eq!(encoded.body["model"], model_id);
        assert_eq!(encoded.body["stream"], true);
        assert_eq!(encoded.body["stream_options"]["include_usage"], true);
        // The production internal-Responses conversion maps the output limit to Chat max_tokens.
        // 生产内部 Responses 转换把输出上限映射为 Chat 的 max_tokens。
        assert_eq!(
            encoded.body["max_tokens"],
            json!(request.output_token_limit)
        );
        assert!(encoded.body.get("max_completion_tokens").is_none());
        assert_eq!(request.reasoning_effort, Some(effort));
    }
}

/// Checks that GPT uses Responses and Claude uses Messages with their own effort shapes; returns unit on success.
/// 检查 GPT 走 Responses、Claude 走 Messages 且各自使用自身思考形状；验证成功返回单元值。
#[test]
fn open_design_routes_gpt_to_responses_and_claude_to_messages() {
    let (runtime, configuration) = open_design_runtime(&["gpt-6-luna", "claude-haiku-5.5"]);
    let client = client();
    // Each case names the expected route and a check of the encoded effort projection.
    // 每个用例给出期望线路以及对已编码思考投影的检查。
    let cases: [(&str, ReasoningEffort, &str, fn(&Value)); 3] = [
        (
            "gpt-6-luna",
            ReasoningEffort::High,
            OPENAI_RESPONSES_PROFILE_ID,
            |body| {
                assert_eq!(body.pointer("/reasoning/effort"), Some(&json!("high")));
                assert!(body.get("reasoning_effort").is_none());
            },
        ),
        (
            "claude-haiku-5.5",
            ReasoningEffort::High,
            ANTHROPIC_MESSAGES_PROFILE_ID,
            |body| {
                assert_eq!(body.pointer("/thinking/type"), Some(&json!("adaptive")));
                assert_eq!(body.pointer("/output_config/effort"), Some(&json!("high")));
                assert!(body.get("reasoning_effort").is_none());
            },
        ),
        (
            "claude-haiku-5.5",
            ReasoningEffort::None,
            ANTHROPIC_MESSAGES_PROFILE_ID,
            |body| {
                assert_eq!(body.pointer("/thinking/type"), Some(&json!("disabled")));
                assert!(body.pointer("/output_config/effort").is_none());
            },
        ),
    ];
    for (model_id, effort, protocol, check) in cases {
        let selection = runtime
            .resolve_connection_model_selection(
                &configuration,
                "open-design-test",
                "open-design",
                model_id,
            )
            .expect("model selection must resolve from the manifest");
        assert_eq!(
            selection.route.protocol_route.protocol_profile_id, protocol,
            "{model_id}"
        );
        let mut request = minimal_request();
        request.model_id = model_id.to_string();
        request.reasoning_effort = Some(effort);
        let encoded = client
            .encode_request(&selection.route, &request)
            .unwrap_or_else(|error| panic!("{model_id}: {error:?}"));
        assert_eq!(encoded.body["model"], model_id);
        check(&encoded.body);
    }
}

/// Checks verified media wire shapes and private-path isolation; returns unit on success.
/// 检查已验证媒体线路形状与私有路径隔离；验证成功返回单元值。
#[test]
fn open_design_encodes_verified_media_without_private_paths() {
    // Production selection freezes the model-specific resource constraints as well as the route.
    // 生产选择链同时冻结模型专属资源约束与线路。
    let (runtime, configuration) = open_design_runtime(&[
        "deepseek-v4.1-flash",
        "glm-5.3-flashx",
        "claude-haiku-5.5",
        "mimo-v2.6-pro",
    ]);
    // Keep one encoder for all supported media families.
    // 对所有支持的媒体族复用同一编码器。
    let client = client();
    // These synthetic bytes check serialization, while live acceptance is recorded separately.
    // 合成字节只检查序列化，真实网关验收另行记录。
    let cases = [
        (
            "deepseek-v4.1-flash",
            ResourceKind::Image,
            "image/png",
            "image_url",
            "/image_url/url",
            json!("data:image/png;base64,aGVsbG8="),
        ),
        (
            "glm-5.3-flashx",
            ResourceKind::File,
            "application/pdf",
            "file",
            "/file/file_data",
            json!("data:application/pdf;base64,aGVsbG8="),
        ),
        (
            "claude-haiku-5.5",
            ResourceKind::File,
            "application/pdf",
            "document",
            "/source/data",
            json!("aGVsbG8="),
        ),
        (
            "mimo-v2.6-pro",
            ResourceKind::Audio,
            "audio/wav",
            "input_audio",
            "/input_audio",
            json!({"data": "aGVsbG8=", "format": "wav"}),
        ),
    ];
    for (model_id, kind, mime, block_type, pointer, expected) in cases {
        // Resolve the exact model rather than applying a generic protocol-only route.
        // 解析精确模型，而不是应用仅含通用协议的线路。
        let selection = runtime
            .resolve_connection_model_selection(
                &configuration,
                "open-design-test",
                "open-design",
                model_id,
            )
            .expect("media model selection");
        // Host-only paths deliberately accompany the valid inline resource source.
        // 有效内联资源来源有意携带仅供宿主使用的路径。
        let resource = ResourceReference {
            id: String::from("open-design-media"),
            kind,
            media_type: Some(mime.to_string()),
            source_kind: ResourceSourceKind::InlineData,
            file_name: Some(String::from("document.pdf")),
            image_detail: None,
            resource_path: Some(String::from(".vulcan/attachments/private-owner")),
            local_path: Some(String::from("D:/private-owner/media")),
            url: Some(format!("data:{mime};base64,aGVsbG8=")),
        };
        // Select the user message by role so fixture growth cannot move the assertion target.
        // 按角色选择用户消息，避免夹具增长改变断言目标。
        let mut request = minimal_request();
        request.model_id = model_id.to_string();
        request
            .messages
            .iter_mut()
            .find(|message| message.role == MessageRole::User)
            .expect("request fixture must contain a user message")
            .content
            .push(ContentItem::Resource { resource });
        // Preserve a before-image to detect accidental mutation during projection.
        // 保留投影前镜像，检查投影期间是否意外修改请求。
        let before = serde_json::to_value(&request).expect("request serialization");
        // Encode using the manifest route rather than a test-only media converter.
        // 使用清单线路编码，不使用测试专属媒体转换器。
        let encoded = client
            .encode_request(&selection.route, &request)
            .unwrap_or_else(|error| panic!("{model_id}: {error:?}"));
        // Locate the semantic media block without depending on an array position.
        // 按语义定位媒体内容块，不依赖数组位置。
        let block = encoded.body["messages"]
            .as_array()
            .expect("encoded messages")
            .iter()
            .find(|message| message["role"] == "user")
            .expect("encoded user message")["content"]
            .as_array()
            .expect("multimodal content")
            .iter()
            .find(|part| part["type"] == block_type)
            .expect("declared media block must survive projection");
        assert_eq!(block.pointer(pointer), Some(&expected), "{model_id}");
        assert!(!encoded.body.to_string().contains("private-owner"));
        assert!(!encoded.body.to_string().contains("resource_path"));
        assert_eq!(
            serde_json::to_value(&request).expect("request serialization"),
            before
        );
    }
}
