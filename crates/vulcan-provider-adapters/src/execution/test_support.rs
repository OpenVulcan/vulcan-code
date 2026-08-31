//! Shared manifest-backed fixtures for execution-layer tests only.
//! 仅供执行层测试使用的共享真实清单夹具。

use super::{ProtocolRuntimeSource, ProviderExecutionRoute};
use crate::{CredentialConnection, CredentialMode, SecondaryRouteSelection};
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use vulcan_ai_contract::{
    AiExecutionRequest, AiMessage, AiOperation, ContentItem, MessageRole, PromptCachePolicy,
};
use vulcan_ai_protocols::ProtocolRegistry;
use vulcan_llm_core::LlmProtocolRuntime;

/// Builds one exact execution route from a bundled manifest provider.
/// 从内置清单供应商构建一条精确执行路由。
///
/// # Parameters
/// 参数
/// `provider_id` selects the bundled manifest, `entry_id` the service entry, and `protocol_id`
/// the frozen protocol profile.
/// `provider_id` 选择内置清单，`entry_id` 选择服务入口，`protocol_id` 选择冻结协议 Profile。
///
/// # Returns
/// 返回值
/// A resolved route whose usage profiles come from the manifest declarations.
/// 返回已解析路由，其 usage Profile 来自清单声明。
pub(super) fn bundled_manifest_route(
    provider_id: &str,
    entry_id: &str,
    protocol_id: &str,
) -> ProviderExecutionRoute {
    let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
    let text = std::fs::read_to_string(directory.join(format!("{provider_id}.json")))
        .unwrap_or_else(|_| panic!("missing bundled manifest `{provider_id}`"));
    let manifest = crate::provider_manifest::compile_manifest_text(
        &text,
        crate::provider_manifest::ManifestOwnership::System,
    )
    .expect("bundled manifest must compile");
    let provider = manifest.provider().clone();
    let entry = provider
        .service_entries
        .iter()
        .find(|entry| entry.id == entry_id)
        .expect("manifest entry must exist")
        .clone();
    // The first registered credential profile is the deterministic test identity; entries
    // declare different profile ids per product (e.g. MiniMax payg/token-plan).
    // 首个注册凭据配置档是确定性测试身份；入口按产品声明不同配置档（如 MiniMax）。
    let credential_profile = entry.credential_profiles.first().cloned();
    let credential_profile_id = credential_profile
        .as_ref()
        .map(|profile| profile.id.clone())
        .unwrap_or_else(|| String::from("api-key"));
    let connection = CredentialConnection {
        id: String::from("connection"),
        provider_id: provider.id.clone(),
        service_entry_id: entry.id.clone(),
        credential_profile_id,
        credential_mode: credential_profile
            .map(|profile| profile.credential_mode)
            .unwrap_or(CredentialMode::Token),
        credential_id: String::from("credential"),
        display_name: String::from("Primary"),
        secret_reference: String::from("secret"),
        management_secret_reference: None,
        service_origin_override: None,
        prompt_cache_by_protocol: BTreeMap::new(),
    };
    let effective_protocol_id = protocol_id.to_string();
    let mut route = ProviderExecutionRoute::resolve(
        provider,
        entry,
        connection,
        SecondaryRouteSelection::Automatic,
        effective_protocol_id.as_str(),
    )
    .expect("bundled manifest route should resolve");
    route.upload_channels = manifest.upload_channels.clone();
    route
}

/// Builds one minimal provider-neutral conversation request.
/// 构建一个最小供应商无关对话请求。
pub(super) fn minimal_request() -> AiExecutionRequest {
    AiExecutionRequest {
        operation: AiOperation::ConversationRespond,
        model_id: String::from("openai/gpt-4.1"),
        cache_scope_id: None,
        continuation: None,
        messages: vec![AiMessage {
            role: MessageRole::User,
            content: vec![ContentItem::Text {
                text: String::from("hello"),
            }],
            provider_context_items: Vec::new(),
        }],
        tools: Vec::new(),
        native_tools: Vec::new(),
        tool_choice: Default::default(),
        parallel_tool_calls: Default::default(),
        structured_output: Default::default(),
        stop_sequences: Vec::new(),
        prompt_cache: PromptCachePolicy::ProviderDefault,
        reasoning_effort: None,
        reasoning_output: Default::default(),
        model_parameters: BTreeMap::new(),
        resources: Vec::new(),
        output_token_limit: Some(512),
    }
}

/// Resolves one pinned OpenRouter route for an exact verified protocol.
/// 为一个精确的已验证协议解析一条固定二级节点的 OpenRouter 路由。
///
/// # Parameters
/// 参数
/// `protocol_id` is one exact protocol profile registered by OpenRouter.
/// `protocol_id` 是 OpenRouter 注册的一个精确协议 Profile。
///
/// # Returns
/// 返回值
/// A frozen route with fallback disabled.
/// 返回禁用回退的冻结路由。
pub(super) fn pinned_openrouter_route(protocol_id: &str) -> ProviderExecutionRoute {
    let mut route = bundled_manifest_route("openrouter", "public-api", protocol_id);
    route.secondary_route = SecondaryRouteSelection::PinnedEndpoint {
        route_id: String::from("anthropic"),
        provider_display_name: String::from("Anthropic"),
        endpoint_display_name: String::from("Anthropic"),
    };
    route
}

/// Resolves one DeepSeek official API route for an exact compatible protocol.
/// 为一个精确兼容协议解析 DeepSeek 官方 API 路由。
///
/// # Parameters
/// 参数
/// `protocol_id` selects the credential-bound Anthropic or OpenAI route.
/// `protocol_id` 选择凭据绑定的 Anthropic 或 OpenAI 路由。
///
/// # Returns
/// 返回值
/// A frozen DeepSeek official API route.
/// 返回冻结的 DeepSeek 官方 API 路由。
pub(super) fn deepseek_route(protocol_id: &str) -> ProviderExecutionRoute {
    bundled_manifest_route("deepseek", "official-api", protocol_id)
}

/// Reads one bounded local HTTP request including its declared body.
/// 读取一项有界本地 HTTP 请求及其声明正文。
///
/// # Parameters
/// 参数
/// `stream` is the accepted loopback connection owned by one test request.
/// `stream` 是一项测试请求持有的已接受环回连接。
///
/// # Returns
/// 返回值
/// The complete UTF-8 request used only for transport assertions.
/// 返回仅用于传输断言的完整 UTF-8 请求。
pub(super) async fn read_test_http_request(stream: &mut TcpStream) -> String {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 4_096];
    let expected_length = loop {
        let read = stream.read(&mut chunk).await.expect("request should read");
        assert!(read > 0, "request headers must not end early");
        bytes.extend_from_slice(&chunk[..read]);
        assert!(
            bytes.len() <= 128 * 1_024,
            "test request must remain bounded"
        );
        let Some(header_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8(bytes[..header_end].to_vec())
            .expect("request headers should be UTF-8");
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            })
            .expect("request should declare content length");
        break header_end + 4 + content_length;
    };
    while bytes.len() < expected_length {
        let read = stream.read(&mut chunk).await.expect("body should read");
        assert!(read > 0, "request body must not end early");
        bytes.extend_from_slice(&chunk[..read]);
        assert!(
            bytes.len() <= 128 * 1_024,
            "test request must remain bounded"
        );
    }
    String::from_utf8(bytes).expect("request should be UTF-8")
}

impl ProtocolRuntimeSource for Arc<ProtocolRegistry> {
    /// Converts the legacy registry only for existing unit-test fixtures.
    /// 仅为既有单元测试夹具转换旧注册表。
    fn into_protocol_runtime(self) -> Arc<LlmProtocolRuntime> {
        Arc::new(LlmProtocolRuntime::new(self))
    }
}
