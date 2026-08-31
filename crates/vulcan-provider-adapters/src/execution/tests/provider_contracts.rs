//! Integrated provider parameter, route, and authentication contracts.
//! 供应商参数、路由与认证的集成契约。

use super::super::test_support::{
    bundled_manifest_route, deepseek_route, minimal_request, pinned_openrouter_route,
};
use super::super::{ProviderExecutionClient, ProviderExecutionRoute, execution_headers};
use crate::{MINIMAX_REASONING_SPLIT_PARAMETER_ID, SAMPLING_TEMPERATURE_PARAMETER_ID};
use serde_json::json;
use std::collections::BTreeSet;
use std::sync::Arc;
use vulcan_ai_contract::{
    ModelParameterValue, PromptCachePolicy, ReasoningEffort, StructuredOutputPolicy,
};
use vulcan_ai_protocols::{
    ANTHROPIC_MESSAGES_PROFILE_ID, OPENAI_CHAT_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID,
    ProtocolRegistry,
};

/// Resolves one MiniMax route for an exact compatible protocol.
/// 为一个精确兼容协议解析 MiniMax 路由。
///
/// # Parameters
/// 参数
/// `protocol_id` selects the credential-bound Anthropic or OpenAI route.
/// `protocol_id` 选择凭据绑定的 Anthropic 或 OpenAI 路由。
///
/// # Returns
/// 返回值
/// A frozen MiniMax international API route.
/// 返回冻结的 MiniMax 国际 API 路由。
fn minimax_route(protocol_id: &str) -> ProviderExecutionRoute {
    bundled_manifest_route("minimax", "global", protocol_id)
}

/// Resolves one exact Kimi Coding Plan route.
/// 解析一条精确 Kimi Coding Plan 路由。
fn kimi_coding_route() -> ProviderExecutionRoute {
    bundled_manifest_route("kimi", "coding-plan", OPENAI_CHAT_PROFILE_ID)
}

/// Resolves one Alibaba Model Studio route with a credential-scoped workspace origin.
/// 使用凭据作用域工作空间源地址解析一条阿里云百炼路由。
///
/// # Parameters
/// 参数
/// `protocol_id` selects the exact OpenAI or Anthropic compatibility path.
/// `protocol_id` 选择精确 OpenAI 或 Anthropic 兼容路径。
///
/// # Returns
/// 返回值
/// A frozen Singapore route whose public origin is replaced by the credential setting.
/// 返回公共源地址已被凭据设置替换的冻结新加坡路由。
fn alibaba_workspace_route(protocol_id: &str) -> ProviderExecutionRoute {
    let mut route = bundled_manifest_route("alibaba", "model-studio-singapore", protocol_id);
    route.connection.service_origin_override = Some(String::from(
        "https://ws-example.ap-southeast-1.maas.aliyuncs.com",
    ));
    route
}

/// Verifies one workspace origin derives the correct protocol-specific request roots.
/// 验证同一个工作空间源地址派生正确的协议专属请求根地址。
#[test]
fn alibaba_workspace_origin_is_protocol_scoped() {
    let chat = alibaba_workspace_route(OPENAI_CHAT_PROFILE_ID);
    assert_eq!(
        chat.request_url("/chat/completions"),
        "https://ws-example.ap-southeast-1.maas.aliyuncs.com/compatible-mode/v1/chat/completions"
    );
    let anthropic = alibaba_workspace_route(ANTHROPIC_MESSAGES_PROFILE_ID);
    assert_eq!(
        anthropic.request_url("/v1/messages"),
        "https://ws-example.ap-southeast-1.maas.aliyuncs.com/apps/anthropic/v1/messages"
    );
}

/// Verifies capture encoding includes provider-specific secondary routing.
/// 验证捕获编码包含供应商特有的二级路由字段。
#[test]
fn encode_request_projects_pinned_openrouter_route_for_every_protocol() {
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    for (protocol_id, expected_path) in [
        (OPENAI_CHAT_PROFILE_ID, "/chat/completions"),
        (OPENAI_RESPONSES_PROFILE_ID, "/responses"),
        (ANTHROPIC_MESSAGES_PROFILE_ID, "/messages"),
    ] {
        let encoded = client
            .encode_request(&pinned_openrouter_route(protocol_id), &minimal_request())
            .expect("request should encode");

        assert_eq!(encoded.path, expected_path);
        assert_eq!(encoded.body["model"], "openai/gpt-4.1");
        assert_eq!(encoded.body["provider"]["order"], json!(["anthropic"]));
        assert_eq!(encoded.body["provider"]["allow_fallbacks"], false);
    }
}

/// Verifies Kimi Coding projection uses thinking.type and the OAuth-bound device identity.
/// 验证 Kimi Coding 投影使用 thinking.type 与 OAuth 绑定设备身份。
#[test]
fn kimi_coding_projection_uses_thinking_and_device_identity() {
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    let mut request = minimal_request();
    request.model_id = String::from("kimi-for-coding");
    request.reasoning_effort = Some(ReasoningEffort::High);
    let route = kimi_coding_route();
    let encoded = client
        .encode_request(&route, &request)
        .expect("Kimi request should encode");
    assert_eq!(encoded.body["thinking"]["type"], "enabled");
    assert!(encoded.body.get("reasoning_effort").is_none());
    let secret = crate::CredentialSecretEnvelope::new(
        crate::CredentialSecretMaterial::KimiCodingPlanOAuth {
            tokens: crate::OAuthTokenSet {
                access_token: String::from("access"),
                refresh_token: String::from("refresh"),
                issued_at_unix_ms: 1,
                expires_at_unix_ms: 2,
                scopes: BTreeSet::new(),
                account_subject: None,
                email: None,
            },
            device_id: String::from("123e4567-e89b-42d3-a456-426614174000"),
        },
    )
    .expect("Kimi material")
    .execution_secret()
    .expect("Kimi execution secret");
    let headers = execution_headers(&route, &encoded.headers, &secret).expect("Kimi headers");
    assert_eq!(
        headers
            .get("x-msh-device-id")
            .and_then(|value| value.to_str().ok()),
        Some("123e4567-e89b-42d3-a456-426614174000")
    );
    assert_eq!(
        headers
            .get("x-msh-platform")
            .and_then(|value| value.to_str().ok()),
        Some("CLIProxyAPI")
    );
}

/// Verifies MiniMax Chat projects adaptive thinking separately from reasoning output splitting.
/// 验证 MiniMax Chat 将自适应思考与思考输出拆分分别投影。
#[test]
fn minimax_openai_chat_projects_adaptive_thinking() {
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    let mut request = minimal_request();
    request.model_id = String::from("MiniMax-M3");
    request.reasoning_effort = Some(ReasoningEffort::Adaptive);
    request.model_parameters.insert(
        MINIMAX_REASONING_SPLIT_PARAMETER_ID.to_string(),
        ModelParameterValue::Boolean(true),
    );

    let encoded = client
        .encode_request(&minimax_route(OPENAI_CHAT_PROFILE_ID), &request)
        .expect("MiniMax Chat request should encode");

    assert_eq!(encoded.body["reasoning_split"], true);
    assert_eq!(encoded.body["stream_options"]["include_usage"], true);
    assert_eq!(encoded.body["thinking"]["type"], "adaptive");
    assert!(encoded.body.get("reasoning_effort").is_none());
}

/// Verifies MiniMax Anthropic projects the documented disabled thinking shape.
/// 验证 MiniMax Anthropic 投影文档化的关闭思考结构。
#[test]
fn minimax_anthropic_projects_disabled_thinking() {
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    let mut request = minimal_request();
    request.model_id = String::from("MiniMax-M3");
    request.reasoning_effort = Some(ReasoningEffort::None);

    let encoded = client
        .encode_request(&minimax_route(ANTHROPIC_MESSAGES_PROFILE_ID), &request)
        .expect("MiniMax Anthropic request should encode");
    assert_eq!(encoded.body["thinking"]["type"], "disabled");
    assert!(encoded.body.get("output_config").is_none());
}

/// Verifies MiniMax thinking projection preserves Anthropic native JSON Schema output format.
/// 验证 MiniMax 思考参数投影会保留 Anthropic 原生 JSON Schema 输出格式。
#[test]
fn minimax_anthropic_preserves_structured_output_format() {
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    let mut request = minimal_request();
    request.model_id = String::from("MiniMax-M3");
    request.reasoning_effort = Some(ReasoningEffort::None);
    request.structured_output = StructuredOutputPolicy::JsonSchema {
        name: String::from("vmm_turn_analysis"),
        description: Some(String::from("VMM turn analysis")),
        schema: json!({
            "type": "object",
            "properties": { "turn_id": { "type": "integer" } },
            "required": ["turn_id"],
            "additionalProperties": false,
        }),
        strict: true,
    };

    let encoded = client
        .encode_request(&minimax_route(ANTHROPIC_MESSAGES_PROFILE_ID), &request)
        .expect("MiniMax Anthropic structured request should encode");
    assert_eq!(encoded.body["thinking"]["type"], "disabled");
    assert_eq!(
        encoded.body["output_config"]["format"]["type"],
        "json_schema"
    );
    assert_eq!(
        encoded.body["output_config"]["format"]["schema"]["required"][0],
        "turn_id"
    );
    assert!(encoded.body["output_config"].get("effort").is_none());
}

/// Verifies MiniMax rejects undocumented strength controls before dispatch.
/// 验证 MiniMax 在分派前拒绝未文档化的强度控制。
#[test]
fn minimax_rejects_undocumented_reasoning_effort() {
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    let mut request = minimal_request();
    request.model_id = String::from("MiniMax-M3");
    request.reasoning_effort = Some(ReasoningEffort::High);

    let failure = client
        .encode_request(&minimax_route(OPENAI_CHAT_PROFILE_ID), &request)
        .expect_err("MiniMax high effort is not documented");
    assert!(
        failure
            .error
            .message
            .contains("not in the declared mapping table")
    );
}

/// Verifies DeepSeek Chat projects the documented disabled thinking shape.
/// 验证 DeepSeek Chat 投影文档化的关闭思考结构。
#[test]
fn deepseek_openai_chat_projects_disabled_thinking() {
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    let mut request = minimal_request();
    request.model_id = String::from("deepseek-v4-pro");
    request.reasoning_effort = Some(ReasoningEffort::None);
    request.model_parameters.insert(
        String::from(SAMPLING_TEMPERATURE_PARAMETER_ID),
        ModelParameterValue::Decimal(String::from("0.2")),
    );

    let encoded = client
        .encode_request(&deepseek_route(OPENAI_CHAT_PROFILE_ID), &request)
        .expect("DeepSeek Chat request should encode");
    assert_eq!(encoded.body["thinking"]["type"], "disabled");
    assert_eq!(encoded.body["temperature"], 0.2);
    assert!(encoded.body.get("reasoning_effort").is_none());
}

/// Verifies DeepSeek thinking requests omit temperature because the provider ignores it.
/// 验证 DeepSeek 思考请求省略供应商不会生效的温度参数。
#[test]
fn deepseek_thinking_omits_ineffective_temperature() {
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    let mut request = minimal_request();
    request.model_id = String::from("deepseek-v4-flash");
    request.reasoning_effort = Some(ReasoningEffort::High);
    request.model_parameters.insert(
        String::from(SAMPLING_TEMPERATURE_PARAMETER_ID),
        ModelParameterValue::Decimal(String::from("0.2")),
    );

    let encoded = client
        .encode_request(&deepseek_route(OPENAI_CHAT_PROFILE_ID), &request)
        .expect("DeepSeek thinking request should encode");
    assert_eq!(encoded.body["thinking"]["type"], "enabled");
    assert!(encoded.body.get("temperature").is_none());
}

/// Verifies DeepSeek Anthropic projects both disabled and documented effort controls.
/// 验证 DeepSeek Anthropic 同时投影关闭思考与文档化档位控制。
#[test]
fn deepseek_anthropic_projects_disabled_and_enabled_thinking() {
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    let route = deepseek_route(ANTHROPIC_MESSAGES_PROFILE_ID);
    let mut disabled = minimal_request();
    disabled.model_id = String::from("deepseek-v4-pro");
    disabled.reasoning_effort = Some(ReasoningEffort::None);
    disabled.model_parameters.insert(
        String::from(SAMPLING_TEMPERATURE_PARAMETER_ID),
        ModelParameterValue::Decimal(String::from("1.3")),
    );
    let disabled = client
        .encode_request(&route, &disabled)
        .expect("DeepSeek Anthropic disabled request should encode");
    assert_eq!(disabled.body["thinking"]["type"], "disabled");
    assert_eq!(disabled.body["temperature"], 1.3);
    assert!(disabled.body.get("output_config").is_none());

    let mut enabled = minimal_request();
    enabled.model_id = String::from("deepseek-v4-pro");
    enabled.reasoning_effort = Some(ReasoningEffort::Max);
    let enabled = client
        .encode_request(&route, &enabled)
        .expect("DeepSeek Anthropic enabled request should encode");
    assert_eq!(enabled.body["thinking"]["type"], "enabled");
    assert_eq!(enabled.body["output_config"]["effort"], "max");
}

/// Verifies CommandCode preserves native reasoning controls on each model-family endpoint.
/// 验证 CommandCode 在各模型系列端点保留原生思考控制字段。
#[test]
fn commandcode_projects_native_chat_and_anthropic_reasoning() {
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    let mut chat_request = minimal_request();
    chat_request.model_id = String::from("gpt-5.6-luna");
    chat_request.reasoning_effort = Some(ReasoningEffort::Max);
    let chat = client
        .encode_request(
            &bundled_manifest_route("commandcode", "provider-api", OPENAI_CHAT_PROFILE_ID),
            &chat_request,
        )
        .expect("CommandCode Chat request should encode");
    assert_eq!(chat.body["reasoning_effort"], "max");

    let mut anthropic_request = minimal_request();
    anthropic_request.model_id = String::from("claude-sonnet-5");
    anthropic_request.reasoning_effort = Some(ReasoningEffort::XHigh);
    anthropic_request.output_token_limit = Some(32_768);
    let anthropic = client
        .encode_request(
            &bundled_manifest_route("commandcode", "provider-api", ANTHROPIC_MESSAGES_PROFILE_ID),
            &anthropic_request,
        )
        .expect("CommandCode Anthropic request should encode");
    assert_eq!(anthropic.body["thinking"]["type"], "enabled");
    assert_eq!(anthropic.body["thinking"]["budget_tokens"], 16_384);
    assert!(anthropic.body.get("reasoning_effort").is_none());
}

/// Verifies DeepSeek Anthropic conversion keeps native web search instead of rejecting it.
/// 验证 DeepSeek Anthropic 转换保留原生 web search，而不是拒绝该工具。
#[test]
fn deepseek_anthropic_preserves_native_web_search() {
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    let mut request = minimal_request();
    request.model_id = String::from("deepseek-v4-flash");
    request.reasoning_effort = Some(ReasoningEffort::Max);
    request.native_tools = vec![vulcan_ai_contract::NativeToolKind::WebSearch];
    let encoded = client
        .encode_request(&deepseek_route(ANTHROPIC_MESSAGES_PROFILE_ID), &request)
        .expect("DeepSeek Anthropic request should encode");
    assert_eq!(encoded.body["tools"][0]["type"], "web_search_20250305");
    assert_eq!(encoded.body["tools"][0]["name"], "web_search");
    assert_eq!(encoded.body["thinking"]["type"], "enabled");
}

/// Verifies DeepSeek Responses projects native web search and removes unsupported fields.
/// 验证 DeepSeek Responses 投影原生网页搜索并移除不支持字段。
#[test]
fn deepseek_responses_projects_native_web_search() {
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    // modelIds covers both official models that share the DeepSeek Responses contract.
    // modelIds 覆盖共享 DeepSeek Responses 契约的两个官方模型。
    let model_ids = ["deepseek-v4-flash", "deepseek-v4-pro"];
    for model_id in model_ids {
        let mut request = minimal_request();
        request.model_id = String::from(model_id);
        request.reasoning_effort = Some(ReasoningEffort::Max);
        request.native_tools = vec![vulcan_ai_contract::NativeToolKind::WebSearch];
        let encoded = client
            .encode_request(&deepseek_route(OPENAI_RESPONSES_PROFILE_ID), &request)
            .expect("DeepSeek Responses request should encode");
        assert_eq!(encoded.path, "/responses", "model {model_id}");
        assert_eq!(
            encoded.body["tools"][0]["type"], "web_search",
            "model {model_id}"
        );
        assert!(encoded.body.get("store").is_none(), "model {model_id}");
        assert!(encoded.body.get("include").is_none(), "model {model_id}");
        assert_eq!(
            encoded.body["reasoning"]["effort"], "max",
            "model {model_id}"
        );
    }
}

/// Verifies every OpenRouter protocol route converges on the unified reasoning object.
/// 验证每条 OpenRouter 协议路由都会收敛到统一 reasoning 对象。
#[test]
fn encode_request_normalizes_openrouter_reasoning_for_every_protocol() {
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    for protocol_id in [
        OPENAI_CHAT_PROFILE_ID,
        OPENAI_RESPONSES_PROFILE_ID,
        ANTHROPIC_MESSAGES_PROFILE_ID,
    ] {
        let mut request = minimal_request();
        request.reasoning_effort = Some(vulcan_ai_contract::ReasoningEffort::Max);
        let encoded = client
            .encode_request(&pinned_openrouter_route(protocol_id), &request)
            .expect("reasoning request should encode");
        assert_eq!(encoded.body["reasoning"]["effort"], "max");
        assert!(encoded.body.get("reasoning_effort").is_none());
        if protocol_id == ANTHROPIC_MESSAGES_PROFILE_ID {
            assert_eq!(encoded.body["output_config"]["effort"], "max");
        } else {
            assert!(encoded.body.get("output_config").is_none());
        }
    }
}

/// Verifies OpenRouter provider-adaptive reasoning uses its unified enable switch.
/// 验证 OpenRouter 供应商自适应推理使用其统一启用开关。
#[test]
fn encode_request_projects_openrouter_adaptive_reasoning_as_enabled() {
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    for protocol_id in [
        OPENAI_CHAT_PROFILE_ID,
        OPENAI_RESPONSES_PROFILE_ID,
        ANTHROPIC_MESSAGES_PROFILE_ID,
    ] {
        let mut request = minimal_request();
        request.reasoning_effort = Some(vulcan_ai_contract::ReasoningEffort::Adaptive);

        let encoded = client
            .encode_request(&pinned_openrouter_route(protocol_id), &request)
            .expect("adaptive reasoning request should encode");
        assert_eq!(encoded.body["reasoning"]["enabled"], true);
        assert!(encoded.body["reasoning"].get("effort").is_none());
    }
}

/// Verifies OpenRouter rejects simultaneous effort and token-budget reasoning controls.
/// 验证 OpenRouter 会拒绝同时设置推理档位和 Token 预算。
#[test]
fn encode_request_rejects_conflicting_openrouter_reasoning_controls() {
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    let route = pinned_openrouter_route(OPENAI_CHAT_PROFILE_ID);
    assert_eq!(
        route.model_parameters_conflicting_with_request_field("reasoning_effort"),
        BTreeSet::from([String::from("reasoning.max_tokens")])
    );
    let mut request = minimal_request();
    request.reasoning_effort = Some(vulcan_ai_contract::ReasoningEffort::High);
    request.model_parameters.insert(
        String::from("reasoning.max_tokens"),
        ModelParameterValue::Integer(2048),
    );
    let error = client
        .encode_request(&route, &request)
        .expect_err("conflicting reasoning controls should fail");
    assert!(error.error.message.contains("mutually exclusive"));
}

/// Verifies an endpoint-specific prompt-cache restriction is enforced by the frozen manifest.
/// 验证端点特定的提示词缓存限制由冻结 Manifest 执行。
#[test]
fn encode_request_rejects_openrouter_explicit_prompt_cache_from_manifest() {
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    let mut request = minimal_request();
    request.prompt_cache = PromptCachePolicy::ManagedExplicit {
        ttl: vulcan_ai_contract::PromptCacheTtlSelection::ProfileDefault,
    };

    let error = client
        .encode_request(&pinned_openrouter_route(OPENAI_CHAT_PROFILE_ID), &request)
        .expect_err("manifest-declared prompt-cache restriction should fail");

    assert!(
        error
            .error
            .message
            .contains("verified model-endpoint capability")
    );
}

/// Verifies one MiniMax credential selects protocol-specific paths and authentication.
/// 验证同一个 MiniMax 凭据会选择协议专属路径与鉴权方式。
#[test]
fn minimax_protocol_selection_controls_wire_route_and_authentication() {
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    let secret = crate::CredentialSecretEnvelope::new_api_key(String::from("secret"))
        .expect("test key should validate")
        .execution_secret()
        .expect("test key should resolve");

    let anthropic_route = minimax_route(ANTHROPIC_MESSAGES_PROFILE_ID);
    let anthropic_request = minimal_request();
    let anthropic = client
        .encode_request(&anthropic_route, &anthropic_request)
        .expect("Anthropic request should encode");
    let anthropic_headers = execution_headers(&anthropic_route, &anthropic.headers, &secret)
        .expect("Anthropic headers should project");
    assert_eq!(anthropic.path, "/anthropic/v1/messages");
    assert_eq!(
        anthropic_headers
            .get(reqwest::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok()),
        Some("Bearer secret")
    );
    assert!(!anthropic_headers.contains_key("x-api-key"));

    let openai_route = minimax_route(OPENAI_CHAT_PROFILE_ID);
    let mut openai_request = minimal_request();
    openai_request.model_parameters.insert(
        MINIMAX_REASONING_SPLIT_PARAMETER_ID.to_string(),
        ModelParameterValue::Boolean(true),
    );
    let openai = client
        .encode_request(&openai_route, &openai_request)
        .expect("OpenAI request should encode");
    let openai_headers = execution_headers(&openai_route, &openai.headers, &secret)
        .expect("OpenAI headers should project");
    assert_eq!(openai.path, "/v1/chat/completions");
    assert_eq!(openai.body["reasoning_split"], true);
    assert_eq!(
        openai_headers
            .get(reqwest::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok()),
        Some("Bearer secret")
    );
    assert!(!openai_headers.contains_key("x-api-key"));
}
