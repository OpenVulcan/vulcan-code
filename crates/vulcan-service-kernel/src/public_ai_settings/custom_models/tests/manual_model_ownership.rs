//! Manual-model saves retain custom-provider ownership, template choices, and declared protocols.
//! 手工模型保存保留自定义供应商归属、模板选择与声明协议。

use super::*;
use std::sync::{Arc, Mutex};
use vulcan_ai_contract::ModelCapabilities;
use vulcan_provider_adapters::{
    CredentialConnection, CredentialMode, CustomModelTemplateSelection,
    CustomProviderAuthentication, CustomProviderDefinition, CustomProviderModelDiscoveryMode,
    ServiceEntryKind, SystemManualModelBinding,
};

/// Exact provider identity shared only by isolated test instances.
/// 仅在隔离测试实例间共享的精确供应商身份。
const PROVIDER_ID: &str = "custom:manual-save-ownership";
/// Synthetic credential identity; saving models never reads a real secret or calls an API.
/// 合成凭据身份；保存模型绝不读取真实密钥或调用 API。
const CONNECTION_ID: &str = "manual-save-account";

/// Owns a private persistence root and a production runtime with one activated custom provider.
/// 持有私有持久化根目录及已激活一个自定义供应商的生产运行时。
struct Fixture {
    /// Directory containing only this test's configuration and standalone manifests.
    /// 仅包含本测试配置与独立清单的目录。
    directory: tempfile::TempDir,
    /// Runtime whose catalogs and model selections are checked after real saves.
    /// 用于检查真实保存后的目录与模型选择的运行时。
    runtime: Arc<PublicAiRuntime>,
    /// Service bridge used by the same save function as the settings UI.
    /// 设置界面同一保存函数使用的服务桥接。
    state: SharedServiceBridgeState,
}

impl Fixture {
    /// Activates a custom provider using `protocol`, returning isolated state without network access.
    /// 使用 `protocol` 激活自定义供应商，返回不访问网络的隔离状态。
    fn new(protocol: CustomProviderProtocol) -> Self {
        // directory confines every subsequent real save and reload to a disposable root.
        // directory 将后续所有真实保存与重载限制在可丢弃的根目录中。
        let directory = tempfile::tempdir().expect("private configuration root");
        // runtime has no credential resolver, so the fixture cannot dispatch authenticated requests.
        // runtime 没有凭据解析器，因此夹具无法派发带认证的请求。
        let runtime = Arc::new(PublicAiRuntime::unavailable().expect("runtime should initialize"));
        // provider follows the same descriptor-to-manifest projection as custom-provider settings.
        // provider 遵循与自定义供应商设置相同的描述符到清单投影。
        let provider = CustomProviderDefinition {
            id: PROVIDER_ID.into(),
            display_name: "Manual save fixture".into(),
            base_url: "https://manual-save.example.invalid/v1".into(),
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
        .expect("custom descriptor should validate");
        // manifest must already be active before the model save to reproduce the ownership bug.
        // manifest 必须在模型保存前激活，才能复现归属判断错误。
        let manifest =
            vulcan_provider_adapters::provider_manifest::project_custom_descriptor_manifest(
                &provider,
            )
            .expect("custom manifest should project");
        // configuration carries the exact custom descriptor, manifest, and credential route.
        // configuration 携带精确自定义描述符、清单与凭据路由。
        let mut configuration = PersistedAiConfiguration::default();
        configuration
            .custom_providers
            .insert(PROVIDER_ID.into(), provider);
        configuration
            .custom_provider_manifests
            .insert(PROVIDER_ID.into(), manifest);
        configuration
            .custom_provider_file_refs
            .insert(PROVIDER_ID.into());
        configuration.connection_graph.connections.insert(
            CONNECTION_ID.into(),
            CredentialConnection {
                id: CONNECTION_ID.into(),
                provider_id: PROVIDER_ID.into(),
                service_entry_id: "default".into(),
                credential_profile_id: "api-key".into(),
                credential_mode: CredentialMode::Token,
                credential_id: "synthetic-credential".into(),
                display_name: "Synthetic account".into(),
                secret_reference: "synthetic-secret".into(),
                management_secret_reference: None,
                service_origin_override: None,
            },
        );
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("activate custom provider");
        // bridge reuses production service wiring while keeping project/session publication out of scope.
        // bridge 复用生产服务接线，同时排除工程与会话发布。
        let mut bridge = crate::tests::test_support::test_agent_runtime_bridge();
        bridge.public_ai_runtime = runtime.clone();
        // state owns no workspace, so saving cannot touch any actual session.
        // state 不持有工作区，因此保存不会触碰任何实际会话。
        let mut state = crate::tests::test_support::test_service_state_with_parent_session(
            uuid::Uuid::new_v4(),
        );
        state.agent_runtime = Some(bridge);
        state.workspace_snapshot = None;
        Self {
            directory,
            runtime,
            state: Arc::new(Mutex::new(state)),
        }
    }
}

/// Returns the identity-only model submitted by the custom-provider form for `model_id`.
/// 返回自定义供应商表单为 `model_id` 提交的仅声明身份的模型。
fn manual_model(model_id: &str) -> CustomModelDefinition {
    CustomModelDefinition {
        id: model_id.into(),
        display_name: model_id.into(),
        enabled: true,
        capabilities: ModelCapabilities {
            operations: vec![AiOperation::ConversationRespond],
            ..ModelCapabilities::default()
        },
        request_projection: Default::default(),
        tool_schema_projection: Default::default(),
        system_binding: None,
    }
}

/// Verifies Anthropic, Chat, and Responses custom models retain template decisions and routes after reload.
/// 验证 Anthropic、Chat 与 Responses 自定义模型在重载后保留模板决策与路由。
/// No inputs or return value; the real service transaction writes only inside each task-local fixture.
/// 无输入或返回值；真实服务事务仅写入各任务局部夹具。
#[tokio::test]
async fn manual_model_ownership_custom_saves_templates_and_declared_protocols() {
    for (protocol, model_id) in [
        (CustomProviderProtocol::AnthropicMessages, "claude-opus-5-5"),
        (CustomProviderProtocol::OpenAiChat, "gpt-6.1-sol"),
        (CustomProviderProtocol::OpenAiResponses, "gpt-6.1-sol"),
    ] {
        // suggestion comes from the production matcher rather than a pinned catalog snapshot.
        // suggestion 来自生产匹配器，而非固定目录快照。
        let suggestion = service_public_ai_custom_model_template_suggestions(
            PublicAiCustomModelTemplateSuggestionsRequest {
                model_id: model_id.into(),
            },
        )
        .expect("template lookup should succeed")
        .into_iter()
        .find(|item| item.template_key == model_id)
        .expect("exact model template should exist; the fixture model may need updating");
        for apply_template in [false, true] {
            // fixture is fresh for each form choice and protocol.
            // fixture 对每种表单选择和协议都是全新的。
            let fixture = Fixture::new(protocol);
            // decision matches both choices exposed by the manual-model form.
            // decision 对应手工模型表单提供的两种选择。
            let decision = if apply_template {
                CustomModelTemplateDecision::Applied {
                    selection: CustomModelTemplateSelection {
                        template_key: suggestion.template_key.clone(),
                        template_revision: suggestion.template_revision.clone(),
                        context_limit: suggestion.context_limit,
                        supports_text_input: suggestion.supports_text_input,
                        supports_text_output: suggestion.supports_text_output,
                        input_resources: suggestion.input_resources.clone(),
                        output_resources: Vec::new(),
                    },
                }
            } else {
                CustomModelTemplateDecision::Rejected {
                    template_key: suggestion.template_key.clone(),
                    template_revision: suggestion.template_revision.clone(),
                }
            };
            crate::tests::public_ai_persistence_scope::PUBLIC_AI_CONFIGURATION_ROOT
                .scope(fixture.directory.path().to_path_buf(), async {
                    // request crosses the browser's actual JSON contract with no system binding.
                    // request 经过浏览器真实 JSON 契约，且不含系统绑定。
                    let request = serde_json::from_value(serde_json::json!({
                        "providerId": PROVIDER_ID,
                        "models": [manual_model(model_id)],
                        "templateDecisions": [{"modelId": model_id, "decision": decision}],
                    }))
                    .expect("manual form request should deserialize");
                    service_save_public_ai_custom_models(
                        ServiceEventEmitter::new(Vec::new()),
                        fixture.state.clone(),
                        request,
                    )
                    .await
                    .expect("custom manual model should save with its reviewed template choice");
                    // reloaded independently hydrates both the central document and standalone custom manifest.
                    // reloaded 独立补水中央文档及独立自定义清单。
                    let reloaded =
                        vulcan_provider_config::config::load_public_ai_connections_file_at_path(
                            &fixture.directory.path().join("ai-connections.toml"),
                        )
                        .expect("saved custom configuration should reload")
                        .configuration;
                    assert_eq!(
                        reloaded.custom_model(PROVIDER_ID, model_id),
                        Some(&manual_model(model_id))
                    );
                    assert_eq!(
                        reloaded.custom_model_template_decisions.get(
                            &PersistedAiConfiguration::provider_model_key(PROVIDER_ID, model_id)
                        ),
                        Some(&decision),
                    );
                    // restarted proves the persisted custom route remains executable without discovery or inferred bindings.
                    // restarted 证明持久化的自定义路由无需发现或推断绑定即可执行。
                    let restarted =
                        PublicAiRuntime::unavailable().expect("cold runtime should initialize");
                    restarted
                        .reapply_model_catalog_overlays(&reloaded)
                        .expect("cold configuration should activate");
                    for runtime in [fixture.runtime.as_ref(), &restarted] {
                        // selection freezes the single user-declared protocol and the effective template/default capabilities.
                        // selection 冻结用户声明的唯一协议及生效的模板或默认能力。
                        let selection = runtime
                            .resolve_connection_model_selection(
                                &reloaded,
                                CONNECTION_ID,
                                PROVIDER_ID,
                                model_id,
                            )
                            .expect("saved custom model should be selectable");
                        assert_eq!(
                            selection.route.protocol_route.protocol_profile_id,
                            protocol.profile_id()
                        );
                        assert_eq!(selection.route.connection.provider_id, PROVIDER_ID);
                        assert_eq!(
                            selection.model.capabilities.context_limit,
                            Some(if apply_template {
                                suggestion.context_limit
                            } else {
                                CustomProviderModelDefaults::default().context_limit
                            })
                        );
                    }
                })
                .await;
        }
    }
}

/// Verifies custom models reject system bindings and system models still require their exact signed route.
/// 验证自定义模型拒绝系统绑定，系统模型仍要求精确签名路由。
/// No inputs or return value; invalid requests must preserve configuration before a valid system save.
/// 无输入或返回值；无效请求必须保留配置，然后再执行一次有效系统模型保存。
#[tokio::test]
async fn manual_model_ownership_keeps_system_validation_strict() {
    // fixture activates a custom manifest alongside the bundled system manifests.
    // fixture 在随包系统清单旁激活一份自定义清单。
    let fixture = Fixture::new(CustomProviderProtocol::AnthropicMessages);
    // manifest supplies the exact supported service and protocol instead of guessing a route.
    // manifest 提供精确受支持服务与协议，避免猜测路由。
    let manifest = fixture
        .runtime
        .system_manifest("anthropic")
        .expect("system manifest");
    // entry selects a real conversation service with a verified route.
    // entry 选择具有已验证路由的真实会话服务。
    let entry = manifest
        .descriptor
        .service_entries
        .iter()
        .find(|entry| {
            entry.kind == ServiceEntryKind::Conversation
                && entry.protocol_routes.iter().any(|route| route.verified)
        })
        .expect("system conversation service");
    // binding is explicitly copied from the verified system authority.
    // binding 明确复制自已验证的系统权威。
    let binding = SystemManualModelBinding {
        service_entry_id: entry.id.clone(),
        protocol_profile_id: entry
            .protocol_routes
            .iter()
            .find(|route| route.verified)
            .expect("verified route")
            .protocol_profile_id
            .clone(),
    };
    crate::tests::public_ai_persistence_scope::PUBLIC_AI_CONFIGURATION_ROOT
        .scope(fixture.directory.path().to_path_buf(), async {
            // before is retained to check that every rejected save is side-effect free.
            // before 用于检查每次拒绝保存均无副作用。
            let before = fixture
                .runtime
                .configuration_snapshot()
                .expect("before configuration");
            for provider_id in [PROVIDER_ID, "anthropic"] {
                // model deliberately violates the binding contract of its selected ownership kind.
                // model 故意违反所选归属类别的绑定契约。
                let mut model = manual_model("manual-ownership-check");
                if provider_id == PROVIDER_ID {
                    model.system_binding = Some(binding.clone());
                }
                assert!(
                    service_save_public_ai_custom_models(
                        ServiceEventEmitter::new(Vec::new()),
                        fixture.state.clone(),
                        PublicAiCustomModelsSaveRequest {
                            provider_id: provider_id.into(),
                            models: vec![model],
                            template_decisions: Vec::new()
                        },
                    )
                    .await
                    .is_err(),
                    "wrong ownership binding must be rejected"
                );
                assert_eq!(
                    fixture
                        .runtime
                        .configuration_snapshot()
                        .expect("unchanged configuration"),
                    before
                );
            }
            assert!(
                !fixture
                    .directory
                    .path()
                    .join("ai-connections.toml")
                    .exists()
            );
            // model supplies all mandatory system-manual facts after the rejection checks.
            // model 在拒绝检查后提供全部必需的系统手工模型事实。
            let mut model = manual_model("manual-ownership-check");
            model.system_binding = Some(binding);
            model.capabilities.context_limit =
                Some(CustomProviderModelDefaults::default().context_limit);
            model.capabilities.output_limit =
                Some(CustomProviderModelDefaults::default().output_limit);
            service_save_public_ai_custom_models(
                ServiceEventEmitter::new(Vec::new()),
                fixture.state.clone(),
                PublicAiCustomModelsSaveRequest {
                    provider_id: "anthropic".into(),
                    models: vec![model.clone()],
                    template_decisions: Vec::new(),
                },
            )
            .await
            .expect("explicitly bound system manual model should still save");
            assert_eq!(
                fixture
                    .runtime
                    .configuration_snapshot()
                    .expect("saved configuration")
                    .custom_model("anthropic", &model.id),
                Some(&model)
            );
        })
        .await;
}

/// Verifies enabling discovery during a Chat or Responses edit retains the fetched models after saving.
/// 验证编辑为 Chat 或 Responses 并启用发现后，保存仍保留已拉取模型。
/// No inputs or return value; synthetic discovery traverses production activation and isolated persistence.
/// 无输入或返回值；合成发现结果经过生产激活与隔离持久化流程。
#[tokio::test]
async fn custom_protocol_edit_keeps_discovered_models_after_save() {
    for protocol in [
        CustomProviderProtocol::OpenAiChat,
        CustomProviderProtocol::OpenAiResponses,
    ] {
        // fixture starts in manual Anthropic mode, matching a provider later edited to use discovery.
        // fixture 从手工 Anthropic 模式开始，对应稍后编辑为使用发现的供应商。
        let fixture = Fixture::new(CustomProviderProtocol::AnthropicMessages);
        // configuration preserves the existing provider identity and credential references.
        // configuration 保留已有供应商身份与凭据引用。
        let mut configuration = fixture
            .runtime
            .configuration_snapshot()
            .expect("current configuration");
        // edited declares the new protocol and standard model-list source explicitly.
        // edited 显式声明新协议与标准模型列表来源。
        let edited = CustomProviderDefinition {
            id: PROVIDER_ID.into(),
            display_name: "Discovered model fixture".into(),
            base_url: "https://manual-save.example.invalid/v1".into(),
            protocols: vec![protocol],
            authentication: CustomProviderAuthentication::BearerAuthorization,
            model_discovery_mode: CustomProviderModelDiscoveryMode::Standard,
            model_discovery_protocol:
                vulcan_provider_adapters::CustomProviderModelDiscoveryProtocol::OpenAi,
            model_discovery_path: None,
            protocol_routes: Vec::new(),
            request_headers: BTreeMap::new(),
            session_header: None,
        }
        .into_descriptor()
        .expect("edited provider should validate");
        configuration
            .custom_providers
            .insert(PROVIDER_ID.into(), edited);
        vulcan_provider_config::custom_manifest::synchronize_custom_provider_manifest(
            &mut configuration,
            PROVIDER_ID,
        )
        .expect("edited provider manifest should synchronize");
        // source carries a genuinely unlisted identity so stale declared-only membership cannot pass unnoticed.
        // source 携带确实未列入清单的身份，避免过期的仅声明成员策略逃过检查。
        let mut source = vulcan_provider_adapters::parse_openai_model_catalog(
            CONNECTION_ID,
            1,
            r#"{"data":[{"id":"fixture-discovered-model"}]}"#,
        )
        .expect("synthetic discovery should parse");
        for model in &mut source.models {
            model.protocol_variants.insert(
                protocol.profile_id().into(),
                vulcan_provider_adapters::ProtocolModelVariant {
                    capabilities: model.capabilities.clone(),
                    capability_evidence: Default::default(),
                    discovery_signature: "fixture-discovery".into(),
                    catalog_generation: source.generation,
                },
            );
        }
        fixture
            .runtime
            .stage_source_catalog_for_testing(source)
            .expect("stage discovered models");
        fixture
            .runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("activate edited provider");
        crate::tests::public_ai_persistence_scope::PUBLIC_AI_CONFIGURATION_ROOT
            .scope(fixture.directory.path().to_path_buf(), async {
                service_save_public_ai_custom_models(
                    ServiceEventEmitter::new(Vec::new()),
                    fixture.state.clone(),
                    PublicAiCustomModelsSaveRequest {
                        provider_id: PROVIDER_ID.into(),
                        models: Vec::new(),
                        template_decisions: Vec::new(),
                    },
                )
                .await
                .expect("save model settings after provider protocol edit");
                // reloaded checks the exact persisted route while runtime retains the process-local discovery evidence.
                // reloaded 检查精确持久化路由，运行时同时保留进程内发现证据。
                let reloaded =
                    vulcan_provider_config::config::load_public_ai_connections_file_at_path(
                        &fixture.directory.path().join("ai-connections.toml"),
                    )
                    .expect("reload edited provider")
                    .configuration;
                // selection proves the fetched identity remains executable using the requested protocol.
                // selection 证明已拉取身份仍可通过请求的协议执行。
                let selection = fixture
                    .runtime
                    .resolve_connection_model_selection(
                        &reloaded,
                        CONNECTION_ID,
                        PROVIDER_ID,
                        "fixture-discovered-model",
                    )
                    .expect("fetched model must remain available after the protocol edit and save");
                assert_eq!(
                    selection.route.protocol_route.protocol_profile_id,
                    protocol.profile_id()
                );
                assert!(
                    reloaded
                        .custom_model(PROVIDER_ID, "fixture-discovered-model")
                        .is_none(),
                    "discovery must not silently create a manual model"
                );
            })
            .await;
    }
}
