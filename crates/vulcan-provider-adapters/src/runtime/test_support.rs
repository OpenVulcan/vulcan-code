//! Shared connection, model-catalog, and immutable credential fixtures for runtime-domain tests.
//! 运行时各职责域测试共享的连接、模型目录与不可变凭据夹具。

use super::{CredentialSecretResolver, CredentialSecretStore, PublicAiRuntime};
use crate::{
    ConnectionModelCatalog, CredentialConnection, CredentialMode, CredentialSecretEnvelope,
    PersistedAiConfiguration, parse_openrouter_model_catalog,
};
use async_trait::async_trait;
use std::collections::BTreeMap;
use vulcan_ai_contract::AccountUsageSnapshot;
use vulcan_ai_protocols::{
    ANTHROPIC_MESSAGES_PROFILE_ID, OPENAI_CHAT_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID,
};

/// Supplies one immutable API key to catalog-refresh tests.
/// 为目录刷新测试提供一个不可变 API Key。
#[derive(Clone)]
pub(super) struct TestStaticSecretStore {
    /// Exact versioned test secret returned for every reference.
    /// 为每个引用返回的精确版本化测试秘密。
    secret: CredentialSecretEnvelope,
}

impl TestStaticSecretStore {
    /// Builds one deterministic in-memory test secret.
    /// 构建一个确定性的内存测试秘密。
    ///
    /// # Returns
    /// 返回值
    /// A store containing one revision-one API key envelope.
    /// 返回包含第一修订版 API Key 封装的存储。
    pub(super) fn api_key() -> Self {
        Self {
            secret: CredentialSecretEnvelope {
                version: 1,
                revision: 1,
                material: crate::CredentialSecretMaterial::ApiKey {
                    value: String::from("sk-sp-manifest-activation-test"),
                },
            },
        }
    }
}

#[async_trait]
impl CredentialSecretResolver for TestStaticSecretStore {
    /// Returns the immutable test secret for any exact reference.
    /// 为任意精确引用返回不可变测试秘密。
    async fn read_optional_secret(
        &self,
        _secret_reference: &str,
    ) -> Result<Option<CredentialSecretEnvelope>, String> {
        Ok(Some(self.secret.clone()))
    }
}

#[async_trait]
impl CredentialSecretStore for TestStaticSecretStore {
    /// Rejects mutation because this fixture is intentionally immutable.
    /// 拒绝变更，因为该夹具被刻意设计为不可变。
    async fn create_secret(
        &self,
        _secret_reference: &str,
        _secret: &CredentialSecretEnvelope,
    ) -> Result<(), String> {
        Err(String::from("The test secret store is immutable."))
    }

    /// Rejects replacement because this fixture is intentionally immutable.
    /// 拒绝替换，因为该夹具被刻意设计为不可变。
    async fn replace_secret(
        &self,
        _secret_reference: &str,
        _expected_revision: u64,
        _secret: &CredentialSecretEnvelope,
    ) -> Result<(), String> {
        Err(String::from("The test secret store is immutable."))
    }

    /// Rejects deletion because this fixture is intentionally immutable.
    /// 拒绝删除，因为该夹具被刻意设计为不可变。
    async fn delete_secret(&self, _secret_reference: &str) -> Result<(), String> {
        Err(String::from("The test secret store is immutable."))
    }

    /// Rejects restoration because this fixture is intentionally immutable.
    /// 拒绝恢复，因为该夹具被刻意设计为不可变。
    async fn restore_secret(
        &self,
        _secret_reference: &str,
        _secret: &CredentialSecretEnvelope,
    ) -> Result<(), String> {
        Err(String::from("The test secret store is immutable."))
    }
}

/// Builds one verified OpenRouter token connection.
/// 构建一个已验证的 OpenRouter Token 连接。
///
/// # Parameters
/// 参数
/// `id` is the stable fixture connection identity.
/// `id` 是夹具连接的稳定身份。
///
/// # Returns
/// 返回值
/// A deterministic token connection with matching credential and secret references.
/// 返回凭据及秘密引用相互匹配的确定性 Token 连接。
pub(super) fn connection(id: &str) -> CredentialConnection {
    CredentialConnection {
        id: id.to_string(),
        provider_id: String::from("openrouter"),
        service_entry_id: String::from("public-api"),
        credential_profile_id: String::from("api-key"),
        credential_mode: CredentialMode::Token,
        credential_id: format!("credential-{id}"),
        display_name: id.to_string(),
        secret_reference: format!("secret-{id}"),
        management_secret_reference: None,
        service_origin_override: None,
        prompt_cache_by_protocol: BTreeMap::new(),
    }
}

/// Adds one protocol variant per protocol id to every model of one catalog.
/// 给一个目录的每个模型按协议 ID 各添加一个协议变体。
///
/// # Parameters
/// 参数
/// `catalog` is the credential-scoped source; `protocol_profile_ids` are the exact bound protocols.
/// `catalog` 是凭据作用域来源；`protocol_profile_ids` 是精确绑定协议。
/// Each model receives one variant per protocol so execution can verify the exact bound variant.
/// 每个模型按协议接收一个变体，使执行路径能够校验精确绑定变体。
///
/// # Returns
/// 返回值
/// The same catalog with independent evidence for each declared protocol.
/// 返回为每条声明协议保留独立证据的同一目录。
pub(super) fn with_protocol_variants(
    mut catalog: ConnectionModelCatalog,
    protocol_profile_ids: &[&str],
) -> ConnectionModelCatalog {
    for model in &mut catalog.models {
        for protocol_profile_id in protocol_profile_ids {
            model.protocol_variants.insert(
                (*protocol_profile_id).to_string(),
                crate::ProtocolModelVariant {
                    capabilities: model.capabilities.clone(),
                    capability_evidence: Default::default(),
                    discovery_signature: String::from("test-signature"),
                    catalog_generation: catalog.generation,
                },
            );
        }
    }
    catalog
}

/// Builds one deterministic credential-scoped catalog.
/// 构建一个确定性的凭据作用域目录。
///
/// # Parameters
/// 参数
/// `connection_id` identifies the exact fixture account.
/// `connection_id` 标识精确夹具账号。
///
/// # Returns
/// 返回值
/// One documented OpenRouter model with three explicit protocol variants.
/// 返回一个包含三条显式协议变体的文档化 OpenRouter 模型。
pub(super) fn catalog(connection_id: &str) -> ConnectionModelCatalog {
    with_protocol_variants(
        parse_openrouter_model_catalog(
            connection_id,
            1,
            r#"{"data":[{"id":"openai/gpt-5.6-sol","name":"Model","context_length":8192,"supported_parameters":["tools","temperature"],"architecture":{"input_modalities":["text"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":4096}}]}"#,
        )
        .expect("documented model fixture should parse"),
        &[
            OPENAI_CHAT_PROFILE_ID,
            OPENAI_RESPONSES_PROFILE_ID,
            ANTHROPIC_MESSAGES_PROFILE_ID,
        ],
    )
}

/// Builds a runtime and two configured accounts that expose the same model.
/// 构建一个运行时以及两个暴露相同模型的已配置账号。
///
/// # Returns
/// 返回值
/// The initialized runtime and its matching two-account configuration.
/// 返回已初始化运行时及其配对的双账号配置。
pub(super) fn two_account_runtime() -> (PublicAiRuntime, PersistedAiConfiguration) {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let mut configuration = PersistedAiConfiguration::default();
    configuration
        .connection_graph
        .add_connection(
            &runtime
                .providers()
                .expect("provider registry should resolve"),
            connection("one"),
        )
        .expect("first connection should register");
    configuration
        .connection_graph
        .add_connection(
            &runtime
                .providers()
                .expect("provider registry should resolve"),
            connection("two"),
        )
        .expect("second connection should register");
    let first_catalog = catalog("one");
    install_source_catalog(&runtime, first_catalog.clone());
    runtime
        .replace_catalog(first_catalog)
        .expect("first catalog should install");
    let second_catalog = catalog("two");
    install_source_catalog(&runtime, second_catalog.clone());
    runtime
        .replace_catalog(second_catalog)
        .expect("second catalog should install");
    configuration.set_model_enabled("openrouter", "openai/gpt-5.6-sol", true);
    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("two-account generation should publish");
    (runtime, configuration)
}

/// Installs one immutable source catalog for candidate-overlay validation tests.
/// 为候选覆盖校验测试安装一个不可变源目录。
///
/// # Parameters
/// 参数
/// `runtime` owns the process-local catalog maps and `catalog` is the exact provider snapshot.
/// `runtime` 持有进程内目录映射，`catalog` 是精确供应商快照。
///
/// # Returns
/// 返回值
/// No value; installs the exact source fixture and panics if the test lock is poisoned.
/// 无返回值；安装精确来源夹具，测试锁中毒时直接失败。
pub(super) fn install_source_catalog(runtime: &PublicAiRuntime, catalog: ConnectionModelCatalog) {
    runtime
        .source_catalogs
        .write()
        .expect("source catalog lock should remain healthy")
        .replace(catalog);
}

/// Builds one custom-provider connection for multi-instance isolation tests.
/// 为多实例隔离测试构建一个自定义供应商连接。
///
/// # Parameters
/// 参数
/// `id` identifies the test account and `provider_id` selects its exact custom provider.
/// `id` 标识测试账号，`provider_id` 选择其精确自定义供应商。
///
/// # Returns
/// 返回值
/// A token connection with deterministic credential and secret references.
/// 返回凭据和秘密引用确定的 Token 连接。
pub(super) fn custom_connection(id: &str, provider_id: &str) -> CredentialConnection {
    CredentialConnection {
        id: id.to_string(),
        provider_id: provider_id.to_string(),
        service_entry_id: String::from("default"),
        credential_profile_id: String::from("api-key"),
        credential_mode: CredentialMode::Token,
        credential_id: format!("credential-{id}"),
        display_name: id.to_string(),
        secret_reference: format!("secret-{id}"),
        management_secret_reference: None,
        service_origin_override: None,
        prompt_cache_by_protocol: BTreeMap::new(),
    }
}

/// Builds one multi-protocol custom provider definition.
/// 构建一个多协议自定义供应商定义。
///
/// # Parameters
/// 参数
/// `provider_id` is the stable provider identity and `protocols` are its declared protocols.
/// `provider_id` 是稳定供应商身份，`protocols` 是其声明协议。
///
/// # Returns
/// 返回值
/// A validated custom descriptor using standard model discovery.
/// 返回使用标准模型发现的已校验自定义描述。
pub(super) fn multi_protocol_custom_definition(
    provider_id: &str,
    protocols: &[crate::CustomProviderProtocol],
) -> crate::ProviderDescriptor {
    custom_definition_with_discovery_mode(
        provider_id,
        protocols,
        crate::CustomProviderModelDiscoveryMode::Standard,
    )
}

/// Builds one custom provider definition with an exact model-source mode.
/// 使用精确模型来源模式构建一个自定义供应商定义。
///
/// # Parameters
/// 参数
/// `provider_id` is the stable custom identity, `protocols` are its declared execution
/// protocols, and `model_discovery_mode` is the authoritative identity-source mode.
/// `provider_id` 是稳定自定义身份，`protocols` 是其声明执行协议，
/// `model_discovery_mode` 是权威身份来源模式。
///
/// # Returns
/// 返回值
/// A validated runtime descriptor for test configuration and connection construction.
/// 返回用于测试配置与连接构造的已校验运行时描述。
pub(super) fn custom_definition_with_discovery_mode(
    provider_id: &str,
    protocols: &[crate::CustomProviderProtocol],
    model_discovery_mode: crate::CustomProviderModelDiscoveryMode,
) -> crate::ProviderDescriptor {
    crate::CustomProviderDefinition {
        id: provider_id.to_string(),
        display_name: String::from("Multi Protocol Fixture"),
        base_url: String::from("https://fixture.example.test/v1"),
        protocols: protocols.to_vec(),
        authentication: crate::CustomProviderAuthentication::BearerAuthorization,
        model_discovery_mode,
        model_discovery_path: None,
        protocol_routes: Vec::new(),
        request_headers: BTreeMap::new(),
    }
    .into_descriptor()
    .expect("multi-protocol fixture should project")
}

/// Builds one deterministic credential-scoped account-usage observation.
/// 构建一份确定性的凭据作用域账号用量观测。
///
/// # Parameters
/// 参数
/// `credential_id` identifies the account, `plan_name` and `plan_id` provide the two explicit
/// identity channels, and `quota_windows` carries exact access-relevant balances.
/// `credential_id` 标识账号，`plan_name` 与 `plan_id` 提供两条显式身份通道，
/// `quota_windows` 承载精确的访问相关余额。
///
/// # Returns
/// 返回值
/// A supported normalized usage snapshot.
/// 返回一份受支持的归一化用量快照。
pub(super) fn account_usage(
    credential_id: &str,
    plan_name: Option<&str>,
    plan_id: Option<&str>,
    quota_windows: Vec<vulcan_ai_contract::AccountUsageWindow>,
) -> AccountUsageSnapshot {
    AccountUsageSnapshot {
        credential_id: credential_id.to_string(),
        availability: vulcan_ai_contract::AccountUsageAvailability::Supported,
        plan_name: plan_name.map(str::to_string),
        plan_id: plan_id.map(str::to_string),
        used_amount: None,
        remaining_amount: None,
        currency: None,
        balance_amount: None,
        quota_windows,
        observed_at: None,
        refresh_message: None,
    }
}

/// Installs one current custom descriptor together with its same-generation Manifest.
/// 同时安装一个当前自定义描述符及其同代际 Manifest。
///
/// # Parameters
/// 参数
/// `configuration` receives both authorities and `provider` is the exact current descriptor.
/// `configuration` 接收两项权威，`provider` 是精确的当前描述符。
///
/// # Returns
/// 返回值
/// No value; installs the matching descriptor and manifest or fails the fixture explicitly.
/// 无返回值；安装匹配的描述符与清单，夹具无效时显式失败。
pub(super) fn install_current_custom_provider(
    configuration: &mut PersistedAiConfiguration,
    provider: crate::ProviderDescriptor,
) {
    // manifest is projected before the descriptor moves into persisted configuration.
    // manifest 在描述符移入持久化配置前完成投影。
    let manifest = crate::provider_manifest::project_custom_descriptor_manifest(&provider)
        .expect("current custom descriptor must project to a Manifest");
    configuration
        .custom_provider_manifests
        .insert(provider.id.clone(), manifest);
    configuration
        .custom_providers
        .insert(provider.id.clone(), provider);
}
