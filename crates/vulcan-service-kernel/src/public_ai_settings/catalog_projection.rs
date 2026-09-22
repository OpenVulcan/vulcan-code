//! Read-only account directory semantics and frozen model-route projections.
//! 只读账号目录语义与冻结模型路由投影。

use serde::Serialize;
use std::collections::BTreeMap;
use vulcan_provider_adapters::{
    CodexModelBehavior, ConnectionModelAccessPolicy, ConnectionModelCatalog, CredentialConnection,
    ModelDiscoveryKind, PersistedAiConfiguration, ProviderRegistry, ServiceEntry,
};

/// Identifies how one credential account obtains its model membership.
/// 标识一个凭据账号如何获得其模型成员关系。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PublicAiModelDirectoryKind {
    /// Models are supplied by a reviewed built-in catalog.
    /// 模型由经过审查的内置目录提供。
    Fixed,
    /// Models are discovered from the exact credential at runtime.
    /// 模型在运行时通过精确凭据发现。
    Dynamic,
    /// Models are declared manually by the user.
    /// 模型由用户手工声明。
    Manual,
    /// The service entry exposes no conversation-model directory.
    /// 服务入口不暴露对话模型目录。
    Unavailable,
}

/// Browser-facing account model access policy with closed directory semantics.
/// 面向浏览器且带封闭目录语义的账号模型访问策略。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PublicAiConnectionModelAccessPolicyRecord {
    /// Exact credential connection identifier.
    /// 精确凭据连接标识。
    connection_id: String,
    /// Provider display baseline, or the explicit account-wide state after a bulk choice.
    /// 供应商展示基线，或批量选择后的显式账号全局状态。
    default_enabled: bool,
    /// Whether models without an individual choice inherit their signed or manual default.
    /// 未单独选择的模型是否继承签名或手工默认值。
    inherit_model_defaults: bool,
    /// Sparse exact model decisions that differ from the account default.
    /// 与账号默认值不同的稀疏精确模型决定。
    model_enabled_overrides: BTreeMap<String, bool>,
    /// Closed model-directory kind resolved by the backend registry.
    /// 后端注册表解析出的封闭模型目录类型。
    directory_kind: PublicAiModelDirectoryKind,
}

/// Browser-facing model route status resolved entirely by the backend.
/// 完全由后端解析的面向浏览器模型路由状态。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PublicAiModelRouteRecord {
    /// Exact provider identifier.
    /// 精确供应商标识。
    pub(super) provider_id: String,
    /// Exact credential connection identifier.
    /// 精确凭据连接标识。
    connection_id: String,
    /// Exact service-entry identifier.
    /// 精确服务入口标识。
    service_entry_id: String,
    /// Exact upstream model identifier.
    /// 精确上游模型标识。
    pub(super) model_id: String,
    /// Effective protocol frozen in the model binding, when executable.
    /// 可执行时冻结在模型绑定中的生效协议。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) protocol_id: Option<String>,
    /// Protocol variants available in this exact effective account catalog.
    /// 此精确生效账号目录中可用的协议变体。
    pub(super) available_protocols: Vec<String>,
    /// Stable source identifier of the frozen binding, when one exists.
    /// 存在冻结绑定时的稳定来源标识。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) binding_source: Option<String>,
    /// Stable rule identifier when a manifest rule produced the binding.
    /// Manifest 规则生成绑定时的稳定规则标识。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) binding_rule_id: Option<String>,
    /// Exact closed Codex behavior frozen by the matched model rule.
    /// 由匹配模型规则冻结的精确封闭 Codex 行为。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) codex_behavior: Option<CodexModelBehavior>,
    /// Stable runtime primitive required before this route can execute.
    /// 当前路由可执行前所需的稳定运行时原语。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) required_runtime_primitive: Option<String>,
    /// Activated provider-manifest revision that owns the binding.
    /// 持有绑定的已激活供应商 Manifest 修订。
    pub(super) manifest_revision: u64,
    /// Whether request execution can consume this exact account/model route.
    /// 请求执行是否可消费此精确账号/模型路由。
    pub(super) executable: bool,
    /// Stable backend-owned diagnostic when the route is not executable.
    /// 路由不可执行时由后端持有的稳定诊断。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) unavailable_reason: Option<String>,
}

/// Resolves one account's closed model-directory kind from its active Manifest routes.
/// 从账号的活动 Manifest 路由解析封闭模型目录类型。
///
/// # Parameters
/// 参数
/// `providers` is the effective registry and `connection` is the exact persisted account.
/// `providers` 是有效注册表，`connection` 是精确持久化账号。
///
/// # Returns
/// 返回值
/// A browser-safe aggregate kind. Any remote route makes the account synchronizable; otherwise
/// fixed identity outranks manual identity. Entries without a verified route are unavailable.
/// 返回浏览器安全的聚合类型。任一远端路由都会使账号可同步；否则固定身份优先于手工身份。
/// 没有已验证路由的入口返回不可用。
fn connection_model_directory_kind(
    providers: &ProviderRegistry,
    connection: &CredentialConnection,
) -> PublicAiModelDirectoryKind {
    let Some(provider) = providers.provider(connection.provider_id.as_str()) else {
        return PublicAiModelDirectoryKind::Unavailable;
    };
    let Some(entry) = provider
        .service_entries
        .iter()
        .find(|entry| entry.id == connection.service_entry_id)
    else {
        return PublicAiModelDirectoryKind::Unavailable;
    };
    service_entry_model_directory_kind(entry)
}

/// Resolves the closed aggregate model-directory kind for one exact service entry.
/// 解析一个精确服务入口的封闭聚合模型目录类型。
///
/// # Parameters
/// 参数
/// `entry` is the Manifest-compiled service entry whose verified routes own discovery semantics.
/// `entry` 是由已验证路由持有发现语义的 Manifest 编译服务入口。
///
/// # Returns
/// 返回值
/// Dynamic when any verified route can refresh remotely; otherwise fixed, manual, or unavailable.
/// 任一已验证路由可远端刷新时返回动态，否则返回固定、手工或不可用。
pub(super) fn service_entry_model_directory_kind(
    entry: &ServiceEntry,
) -> PublicAiModelDirectoryKind {
    // hasDynamic, hasFixed, and hasManual summarize route-owned discovery contracts.
    // hasDynamic、hasFixed 与 hasManual 汇总路由持有的发现契约。
    let mut has_dynamic = false;
    let mut has_fixed = false;
    let mut has_manual = false;
    for route in entry.protocol_routes.iter().filter(|route| route.verified) {
        match route.resolved_model_discovery(entry.model_discovery) {
            ModelDiscoveryKind::Manual => has_manual = true,
            ModelDiscoveryKind::ManifestStatic => has_fixed = true,
            ModelDiscoveryKind::OpenRouter
            | ModelDiscoveryKind::OpenAi
            | ModelDiscoveryKind::OpenAiCodex
            | ModelDiscoveryKind::Anthropic
            | ModelDiscoveryKind::AnthropicCompatible
            | ModelDiscoveryKind::Gemini
            | ModelDiscoveryKind::GoogleCodeAssist => has_dynamic = true,
            ModelDiscoveryKind::ProviderToolsOnly => {}
        }
    }
    if has_dynamic {
        PublicAiModelDirectoryKind::Dynamic
    } else if has_fixed {
        PublicAiModelDirectoryKind::Fixed
    } else if has_manual {
        PublicAiModelDirectoryKind::Manual
    } else {
        PublicAiModelDirectoryKind::Unavailable
    }
}

/// Projects every credential account's sparse model access policy for the browser.
/// 为浏览器投影每个凭据账号的稀疏模型访问策略。
///
/// # Parameters
/// 参数
/// `providers` resolves directory semantics; `configuration` owns user decisions;
/// `initial_policies` contains per-account defaults from the same settings generation.
/// `providers` 解析目录语义；`configuration` 持有用户决定；`initial_policies`
/// 包含来自同一设置代际的逐账号默认值。
///
/// # Returns
/// 返回值
/// Stable records in connection order, or an explicit generation mismatch.
/// 返回按连接顺序排列的稳定记录，代际不一致时显式报错。
pub(super) fn build_connection_model_access_policy_records(
    providers: &ProviderRegistry,
    configuration: &PersistedAiConfiguration,
    initial_policies: &BTreeMap<String, ConnectionModelAccessPolicy>,
) -> Result<Vec<PublicAiConnectionModelAccessPolicyRecord>, String> {
    configuration
        .connection_graph
        .connections
        .values()
        .map(|connection| {
            let policy: ConnectionModelAccessPolicy = configuration
                .connection_model_access_policies
                .get(connection.id.as_str())
                .cloned()
                .or_else(|| initial_policies.get(connection.id.as_str()).cloned())
                .ok_or_else(|| {
                    format!(
                        "Account `{}` has no initial model policy in the settings generation.",
                        connection.id
                    )
                })?;
            Ok(PublicAiConnectionModelAccessPolicyRecord {
                connection_id: connection.id.clone(),
                default_enabled: policy.default_enabled,
                inherit_model_defaults: policy.inherit_model_defaults,
                model_enabled_overrides: policy.model_enabled_overrides,
                directory_kind: connection_model_directory_kind(providers, connection),
            })
        })
        .collect()
}

/// Returns the only protocol authority accepted for one frozen catalog model.
/// 返回一个冻结目录模型唯一可接受的协议权威。
///
/// # Parameters
/// 参数
/// `model` owns the immutable model binding.
/// `model` 持有不可变模型绑定。
///
/// # Returns
/// 返回值
/// The frozen model protocol, or `None` when the model is not executable.
/// 返回冻结的模型协议；模型不可执行时返回 `None`。
pub(super) fn effective_catalog_model_protocol_id(
    model: &vulcan_provider_adapters::ConnectionModel,
) -> Option<&str> {
    model
        .resolved_protocol_binding
        .as_ref()
        .map(|binding| binding.protocol_profile_id.as_str())
}

/// Reports whether a catalog model is declared for text conversation in any authoritative view.
/// 报告一个目录模型是否在任一权威视图中声明文本会话能力。
///
/// # Parameters
/// 参数
/// `model` owns the effective projection plus every protocol-specific capability variant.
/// `model` 持有生效投影与全部协议专属能力变体。
///
/// # Returns
/// 返回值
/// `true` when the effective projection or any retained variant declares conversation. Inspecting
/// variants keeps a blocked conversation model manageable after its effective projection is cleared.
/// 当生效投影或任一保留变体声明会话时返回 `true`。检查变体可使生效投影被清空后的受阻会话模型
/// 仍可管理。
pub(super) fn catalog_model_supports_text_conversation(
    model: &vulcan_provider_adapters::ConnectionModel,
) -> bool {
    vulcan_provider_adapters::model_supports_text_conversation(&model.capabilities)
        || model.protocol_variants.values().any(|variant| {
            vulcan_provider_adapters::model_supports_text_conversation(&variant.capabilities)
        })
}

/// Returns the stable browser-facing identifier for one frozen model-binding source.
/// 返回一个冻结模型绑定来源的稳定浏览器标识。
///
/// # Parameters
/// 参数
/// `source` is the exact backend binding-source enum retained by the effective catalog.
/// `source` 是生效目录保留的精确后端绑定来源枚举。
///
/// # Returns
/// 返回值
/// A closed non-secret identifier suitable for settings UI and diagnostics.
/// 返回适用于设置界面与诊断的封闭非秘密标识。
fn public_ai_model_binding_source_id(
    source: vulcan_provider_adapters::ModelProtocolBindingSource,
) -> &'static str {
    match source {
        vulcan_provider_adapters::ModelProtocolBindingSource::SystemManifest => "system_manifest",
        vulcan_provider_adapters::ModelProtocolBindingSource::SystemUser => "system_user",
        vulcan_provider_adapters::ModelProtocolBindingSource::CustomUser => "custom_user",
        vulcan_provider_adapters::ModelProtocolBindingSource::CustomAutoDefault => {
            "custom_auto_default"
        }
    }
}

/// Explains why one custom-provider model has no frozen protocol binding.
/// 解释一个自定义供应商模型为何没有冻结协议绑定。
///
/// # Parameters
/// 参数
/// The configuration owns the exact preference, providers owns activated protocol routes,
/// the connection identifies the service entry, and the model owns observed protocol variants.
/// configuration 持有精确偏好，providers 持有已激活协议路由，connection 标识服务入口，
/// model 持有已观测协议变体。
///
/// # Returns
/// 返回值
/// A stable backend-owned diagnostic derived from the authoritative custom binding resolver.
/// 返回由权威自定义绑定解析器派生的稳定后端诊断。
fn custom_model_route_unavailable_reason(
    configuration: &PersistedAiConfiguration,
    providers: &ProviderRegistry,
    connection: &CredentialConnection,
    model: &vulcan_provider_adapters::ConnectionModel,
    manifest_revision: u64,
) -> String {
    let Some(provider) = providers.provider(connection.provider_id.as_str()) else {
        return format!(
            "Provider '{}' is absent from the activated provider generation.",
            connection.provider_id
        );
    };
    let Some(entry) = provider
        .service_entries
        .iter()
        .find(|entry| entry.id == connection.service_entry_id)
    else {
        return format!(
            "Service entry '{}' is absent from the activated provider '{}'.",
            connection.service_entry_id, connection.provider_id
        );
    };
    let enabled_protocols = entry
        .protocol_routes
        .iter()
        .map(|route| route.protocol_profile_id.as_str())
        .collect::<Vec<_>>();
    let available_protocols = model
        .protocol_variants
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let preference_key = PersistedAiConfiguration::provider_model_key(
        connection.provider_id.as_str(),
        model.id.as_str(),
    );
    let explicit_protocol = configuration
        .custom_model_protocol_preferences
        .get(preference_key.as_str())
        .and_then(|preference| (*preference).explicit_protocol())
        .map(|protocol| protocol.profile_id());
    match vulcan_provider_adapters::resolve_custom_model_protocol_binding(
        enabled_protocols.as_slice(),
        available_protocols.as_slice(),
        model.id.as_str(),
        manifest_revision,
        explicit_protocol,
    ) {
        Ok(binding) => format!(
            "Model '{}' resolves to protocol '{}', but the activated support catalog has no frozen binding; reactivate the provider catalog.",
            model.id, binding.protocol_profile_id
        ),
        Err(error) => error.to_string(),
    }
}

/// Builds backend-owned protocol status records for every conversation-capable catalog model.
/// 为目录中每个具备会话能力的模型构建由后端持有的协议状态记录。
///
/// # Parameters
/// 参数
/// The configuration owns exact connection identities and preferences; providers, catalogs,
/// and manifest revisions are frozen at the same runtime publication boundary.
/// configuration 持有精确连接身份与偏好；providers、catalogs 与 manifest revisions
/// 冻结在同一运行时发布边界。
///
/// # Returns
/// 返回值
/// Stable per-connection model statuses, including blocked models and their backend diagnostics.
/// 返回稳定的逐连接模型状态，包含被阻止模型及其后端诊断。
pub(super) fn build_public_ai_model_route_records(
    configuration: &PersistedAiConfiguration,
    providers: &ProviderRegistry,
    catalogs: &BTreeMap<String, ConnectionModelCatalog>,
    manifest_revisions: &BTreeMap<String, u64>,
) -> Vec<PublicAiModelRouteRecord> {
    let mut records = Vec::new();
    for (connection_id, catalog) in catalogs {
        let Some(connection) = configuration
            .connection_graph
            .connections
            .get(connection_id.as_str())
        else {
            continue;
        };
        for model in &catalog.models {
            // Only applicable text-token limits can block a management record; explicit
            // media-only output does not inherit conversation window requirements.
            // 只有适用的文本令牌限制才能阻止管理记录；明确的纯媒体输出不继承会话窗口要求。
            let limit_issue = vulcan_provider_adapters::model_capability_limit_issue(
                connection_id,
                model.id.as_str(),
                &model.capabilities,
                &model.capability_evidence,
            );
            // Conversation route records are intentionally narrower than the provider's full
            // system-model inventory. Generation-only system models belong to provider extensions;
            // custom providers remain conversation-only and keep their management diagnostics.
            // 会话路由记录有意窄于系统供应商完整模型清单；纯生成系统模型属于 Provider 扩展；
            // 自定义供应商仍仅支持会话并保留其管理诊断。
            if !configuration
                .custom_providers
                .contains_key(connection.provider_id.as_str())
                && !catalog_model_supports_text_conversation(model)
                && limit_issue.is_none()
            {
                continue;
            }
            // protocolId consumes only the frozen model-level binding.
            // protocolId 只消费冻结的模型级绑定。
            let protocol_id = effective_catalog_model_protocol_id(model).map(str::to_string);
            // binding is the immutable backend authority for source, rule, and revision facts.
            // binding 是来源、规则与修订事实的不可变后端权威。
            let binding = model.resolved_protocol_binding.as_ref();
            // bindingSource identifies the exact model-level binding authority.
            // bindingSource 单独标识当前模型级绑定来源。
            let binding_source = binding
                .map(|binding| public_ai_model_binding_source_id(binding.source))
                .map(str::to_string);
            let manifest_revision = binding
                .map(|binding| binding.manifest_revision)
                .or_else(|| {
                    manifest_revisions
                        .get(connection.provider_id.as_str())
                        .copied()
                })
                .unwrap_or_default();
            // requiredRuntimePrimitive is derived only from the exact frozen behavior.
            // requiredRuntimePrimitive 只从精确冻结行为派生。
            let required_runtime_primitive = binding
                .and_then(|binding| binding.codex_behavior.as_ref())
                .and_then(CodexModelBehavior::missing_runtime_primitive)
                .map(str::to_string);
            // unavailableReason is computed from the same captured provider/catalog generation.
            // unavailableReason 从同一捕获供应商/目录代际计算。
            let unavailable_reason = if let Some(limit_issue) = limit_issue {
                Some(limit_issue)
            } else if let Some(required_primitive) = required_runtime_primitive.as_deref() {
                Some(format!(
                    "Model '{}' requires runtime primitive '{}', which is not registered by this build.",
                    model.id, required_primitive
                ))
            } else {
                match protocol_id.as_deref() {
                    Some(bound_protocol)
                        if binding.is_some()
                            && !model.protocol_variants.contains_key(bound_protocol) =>
                    {
                        Some(format!(
                            "Model '{}' has no capability variant for its bound protocol '{}'; execution is blocked.",
                            model.id, bound_protocol
                        ))
                    }
                    Some(_) => None,
                    None if configuration
                        .custom_providers
                        .contains_key(connection.provider_id.as_str()) =>
                    {
                        Some(custom_model_route_unavailable_reason(
                            configuration,
                            providers,
                            connection,
                            model,
                            manifest_revision,
                        ))
                    }
                    None => Some(format!(
                        "Model '{}' has no protocol binding frozen in the activated provider generation.",
                        model.id
                    )),
                }
            };
            let executable = protocol_id.is_some() && unavailable_reason.is_none();
            records.push(PublicAiModelRouteRecord {
                provider_id: connection.provider_id.clone(),
                connection_id: connection.id.clone(),
                service_entry_id: connection.service_entry_id.clone(),
                model_id: model.id.clone(),
                protocol_id,
                available_protocols: model.protocol_variants.keys().cloned().collect(),
                binding_source,
                binding_rule_id: binding
                    .map(|binding| binding.rule_id.clone())
                    .filter(|rule_id| !rule_id.is_empty()),
                codex_behavior: binding.and_then(|binding| binding.codex_behavior.clone()),
                required_runtime_primitive,
                manifest_revision,
                executable,
                unavailable_reason,
            });
        }
    }
    records.sort_by(|left, right| {
        left.provider_id
            .cmp(&right.provider_id)
            .then_with(|| left.connection_id.cmp(&right.connection_id))
            .then_with(|| left.model_id.cmp(&right.model_id))
    });
    records
}

#[cfg(test)]
mod tests;
