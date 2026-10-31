//! Explicit validation exercises production routing, streaming and persistence on private HTTPS.
//! 显式验证通过私有 HTTPS 覆盖生产路由、流式处理与持久化。

use super::*;
use std::sync::{Arc, Mutex};
use vulcan_ai_contract::ModelCapabilities;
use vulcan_provider_adapters::{
    CredentialConnection, CredentialMode, CustomProviderAuthentication, CustomProviderDefinition,
    CustomProviderModelDiscoveryMode,
};

mod https;

/// Exact custom identity owned only by the isolated validation fixture.
/// 仅由隔离验证夹具持有的精确自定义身份。
const PROVIDER: &str = "custom:validation-test";
/// Pending account under test; a second account proves isolation of validation promotion.
/// 被测待验证账号；第二个账号用于证明验证放行的隔离性。
const ACCOUNT: &str = "validation-account";
/// Manual model identity independent of the bundled evolving model inventory.
/// 独立于随包模型目录演进的手工模型身份。
const MODEL: &str = "validation-model";

/// Builds isolated service state for `protocol` and `http`; returns the activated runtime and bridge.
/// 为 `protocol` 与 `http` 构建隔离服务状态，返回已激活运行时和桥接。
fn fixture(
    protocol: CustomProviderProtocol,
    http: reqwest::Client,
) -> (Arc<PublicAiRuntime>, SharedServiceBridgeState) {
    // Only the synthetic secret and pinned loopback client can execute in this fixture.
    // 本夹具只能使用合成秘密与固定回环客户端执行。
    let runtime = Arc::new(
        PublicAiRuntime::first_batch_with_http_client_for_testing(Arc::new(https::Secrets), http)
            .unwrap(),
    );
    // The descriptor follows the production manual-provider save contract.
    // 描述符遵循生产手工供应商保存契约。
    let provider = CustomProviderDefinition {
        id: PROVIDER.into(),
        display_name: "Validation fixture".into(),
        base_url: format!("https://{}/v1", https::HOST),
        protocols: vec![protocol],
        authentication: CustomProviderAuthentication::BearerAuthorization,
        model_discovery_mode: CustomProviderModelDiscoveryMode::Manual,
        model_discovery_protocol:
            vulcan_provider_adapters::CustomProviderModelDiscoveryProtocol::OpenAi,
        model_discovery_path: None,
        protocol_routes: Vec::new(),
        request_headers: BTreeMap::new(),
        session_header: None,
    }
    .into_descriptor()
    .unwrap();
    // Preserve real manifest compilation and frozen per-model protocol binding.
    // 保留真实清单编译与冻结的逐模型协议绑定。
    let manifest =
        vulcan_provider_adapters::provider_manifest::project_custom_descriptor_manifest(&provider)
            .unwrap();
    // Both pending accounts and disabled workbench visibility remain authoritative during the probe.
    // 探测期间两个账号的待验证状态与工作台禁用可见性仍保持权威。
    let mut configuration = PersistedAiConfiguration::default();
    configuration
        .custom_providers
        .insert(PROVIDER.into(), provider);
    configuration
        .custom_provider_manifests
        .insert(PROVIDER.into(), manifest);
    configuration
        .custom_provider_file_refs
        .insert(PROVIDER.into());
    configuration
        .custom_models
        .entry(PROVIDER.into())
        .or_default()
        .insert(
            MODEL.into(),
            CustomModelDefinition {
                id: MODEL.into(),
                display_name: MODEL.into(),
                enabled: true,
                capabilities: ModelCapabilities {
                    operations: vec![AiOperation::ConversationRespond],
                    ..ModelCapabilities::default()
                },
                request_projection: Default::default(),
                tool_schema_projection: Default::default(),
                system_binding: None,
            },
        );
    configuration.set_model_enabled(PROVIDER, MODEL, false);
    for id in [ACCOUNT, "other-pending-account"] {
        configuration.connection_graph.connections.insert(
            id.into(),
            CredentialConnection {
                id: id.into(),
                provider_id: PROVIDER.into(),
                service_entry_id: "default".into(),
                credential_profile_id: "api-key".into(),
                credential_mode: CredentialMode::Token,
                credential_id: format!("credential-{id}"),
                display_name: id.into(),
                secret_reference: "validation-secret".into(),
                management_secret_reference: None,
                service_origin_override: None,
            },
        );
        configuration
            .set_connection_pending_validation(id, true)
            .unwrap();
    }
    runtime
        .reapply_model_catalog_overlays(&configuration)
        .unwrap();
    // Every service component shares the exact runtime; no unavailable bridge runtime can mask dispatch.
    // 所有服务组件共用精确运行时，避免桥接中的不可用运行时掩盖派发。
    let mut bridge = crate::tests::test_support::test_agent_runtime_bridge();
    bridge.public_ai_runtime = runtime.clone();
    bridge.configured_inference_service =
        Arc::new(crate::configured_inference::ConfiguredInferenceService::new(runtime.clone()));
    // No workspace means the settings transaction cannot touch user sessions.
    // 无工作区意味着设置事务无法触碰用户会话。
    let mut state =
        crate::tests::test_support::test_service_state_with_parent_session(uuid::Uuid::new_v4());
    state.agent_runtime = Some(bridge);
    state.workspace_snapshot = None;
    (runtime, Arc::new(Mutex::new(state)))
}

/// Runs the real settings validation for `protocol` and `reply`; returns after isolated state checks.
/// 为 `protocol` 与 `reply` 运行真实设置验证，在隔离状态检查后返回。
async fn run_validation(protocol: CustomProviderProtocol, reply: https::Reply) {
    // The server withholds its response until the test proves normal routing remains blocked.
    // 服务端暂扣回复，直到测试证明普通路由仍被阻止。
    let mut server = https::Server::start(protocol, reply).await;
    // Runtime and service state share the same immutable catalog and local transport.
    // 运行时与服务状态共用同一不可变目录与本地传输。
    let (runtime, state) = fixture(protocol, server.http.clone());
    // All settings writes stay in this disposable directory.
    // 所有设置写入均限制在此可丢弃目录内。
    let directory = tempfile::tempdir().unwrap();
    // The exact before-image proves temporary validation never publishes its relaxed gates.
    // 精确前镜像证明临时验证绝不发布其放宽后的门禁。
    let baseline = runtime.configuration_snapshot().unwrap();
    // Generation identity detects unintended catalog activation during the probe.
    // 代际身份检测探测期间意外发生的目录激活。
    let generation = runtime.catalog_generation();
    crate::tests::public_ai_persistence_scope::PUBLIC_AI_CONFIGURATION_ROOT
        .scope(directory.path().to_path_buf(), async {
            // Pin the real service future so request observation can be inspected before its response.
            // 固定真实服务 Future，便于在回复前检查请求观察事实。
            let mut validation = Box::pin(service_validate_public_ai_custom_model(
                ServiceEventEmitter::new(vec![]),
                state,
                PublicAiCustomModelValidationRequest {
                    provider_id: PROVIDER.into(),
                    connection_id: ACCOUNT.into(),
                    model_id: MODEL.into(),
                },
            ));
            // A pending-validation error before this point reproduces the user-facing deadlock.
            // 此处之前的待验证错误会复现用户界面的死锁。
            let observed = tokio::select! {
                result = &mut validation => panic!("explicit validation must reach the local endpoint: {result:?}"),
                request = &mut server.received => request.expect("request should reach pinned endpoint"),
            };
            assert_eq!(observed.model, MODEL);
            assert!(observed.streaming);
            assert_eq!(runtime.configuration_snapshot().unwrap(), baseline);
            assert_eq!(runtime.catalog_generation(), generation);
            assert!(runtime.resolve_connection_model_selection(&baseline, ACCOUNT, PROVIDER, MODEL).is_err());
            server.release.take().unwrap().send(()).unwrap();
            // Only successful visible text may promote the exact account in the durable transaction.
            // 仅成功的可见正文可在可靠事务中放行精确账号。
            let result = validation.await;
            // Inspect the authoritative post-response snapshot, not a detached caller configuration.
            // 检查响应后的权威快照，而非调用方的独立配置。
            let after = runtime.configuration_snapshot().unwrap();
            assert!(after.connection_is_pending_validation("other-pending-account"));
            assert_eq!(after.model_enabled_overrides, baseline.model_enabled_overrides);
            if matches!(reply, https::Reply::Visible) {
                // A successful service receipt contains visible text from the actual wire decoder.
                // 成功的服务回执包含实际线协议解码器提供的可见正文。
                let response = result.expect("visible response should validate the pending account");
                assert!(!response.response_text.trim().is_empty());
                assert!(response.response_characters > 0);
                assert!(!after.connection_is_pending_validation(ACCOUNT));
                assert_eq!(after.connection_graph.account_preferences.provider_defaults.get(PROVIDER).map(String::as_str), Some(ACCOUNT));
                // A cold configuration reload proves promotion was persisted, not merely returned to the UI.
                // 冷配置重载证明放行已持久化，而不只是返回给界面。
                let persisted = vulcan_provider_config::config::load_public_ai_connections_file_at_path(
                    &directory.path().join("ai-connections.toml"),
                ).unwrap().configuration;
                assert!(!persisted.connection_is_pending_validation(ACCOUNT));
                assert!(persisted.connection_is_pending_validation("other-pending-account"));
            } else {
                assert!(result.is_err(), "empty and failed responses must not validate an account");
                assert_eq!(after, baseline);
                assert_eq!(runtime.catalog_generation(), generation);
                assert!(!directory.path().join("ai-connections.toml").exists());
            }
            // Validation never silently enables a deliberately disabled conversation model.
            // 验证绝不静默启用被明确禁用的会话模型。
            assert!(runtime.resolve_connection_model_selection(&after, ACCOUNT, PROVIDER, MODEL).is_err());
        })
        .await;
    server.finish().await;
}

/// Visible responses on each declared protocol promote only the explicitly tested account.
/// 每个声明协议的可见回复仅放行显式测试的账号。
/// Returns after local HTTPS, configuration persistence and ordinary-route isolation assertions.
/// 在本地 HTTPS、配置持久化与普通路由隔离断言后返回。
#[tokio::test]
async fn explicit_validation_accepts_pending_accounts_across_protocols() {
    for protocol in [
        CustomProviderProtocol::AnthropicMessages,
        CustomProviderProtocol::OpenAiChat,
        CustomProviderProtocol::OpenAiResponses,
    ] {
        run_validation(protocol, https::Reply::Visible).await;
    }
}

/// Empty or rejected replies leave pending accounts and model visibility unchanged on every protocol.
/// 各协议的空回复或拒绝回复都保持待验证账号与模型可见性不变。
/// Returns after real service failures and no-publication assertions.
/// 在真实服务失败与未发布断言后返回。
#[tokio::test]
async fn explicit_validation_failure_never_promotes_pending_accounts() {
    for protocol in [
        CustomProviderProtocol::AnthropicMessages,
        CustomProviderProtocol::OpenAiChat,
        CustomProviderProtocol::OpenAiResponses,
    ] {
        for reply in [https::Reply::Empty, https::Reply::Rejected] {
            run_validation(protocol, reply).await;
        }
    }
}
