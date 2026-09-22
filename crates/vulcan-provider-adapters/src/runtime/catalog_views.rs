//! Published-generation catalog queries, settings views, and candidate-overlay validation.
//! 已发布代际的目录查询、设置视图与候选覆盖校验。

use super::activation::runtime_configuration_for_manifest_catalog;
use super::catalog_policy::{
    apply_model_catalog_overlays_with_account_access, manual_custom_catalog,
    manual_custom_catalog_for_provider, model_supports_text_conversation,
    retain_models_with_valid_limits, validate_catalog_model_count,
    validate_resolved_catalog_limits, validate_resolved_model_capability_limits,
};
use super::catalog_sources::{empty_model_source_catalog, resolved_model_source_policy};
use super::model_overrides::validate_model_overrides;
use super::tool_targets::{AvailableProviderToolTarget, provider_tool_target_allows_credential};
use super::{PublicAiRuntime, ResolvedProviderGeneration};
use crate::{
    ConnectionModelAccessPolicy, ConnectionModelCatalog, PersistedAiConfiguration,
    ProviderRegistry, ProviderToolCapabilityKind, ResolvedProviderManifest,
    RuntimeConnectionCatalogs,
};
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::Arc;
use vulcan_ai_contract::AccountUsageSnapshot;

/// Selects whether a catalog is executable or retains invalid models for management diagnostics.
/// 选择目录用于执行，还是保留异常模型供管理诊断。
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CatalogProjectionUse {
    /// Only models with valid effective limits can reach request selection.
    /// 只有最终限制有效的模型可进入请求选择。
    Execution,
    /// The settings inventory preserves source identities and their invalid limit evidence.
    /// 设置目录保留来源模型身份及其无效限制证据。
    Management,
}

/// One browser-settings catalog projection derived from a single immutable provider generation.
/// 从单一不可变供应商代际派生的一份浏览器设置目录投影。
pub struct PublicAiRuntimeSettingsCatalogSnapshot {
    /// Runtime-only configuration projected against the same captured manifest generation.
    /// 针对同一已捕获 Manifest 代际投影的仅运行时配置。
    pub runtime_configuration: PersistedAiConfiguration,
    /// Effective providers resolved from the captured manifest catalog.
    /// 从已捕获 Manifest 目录解析的有效供应商。
    pub providers: ProviderRegistry,
    /// Effective per-connection conversation catalogs under current access policy.
    /// 当前访问策略下的逐连接有效对话目录。
    pub catalogs: BTreeMap<String, ConnectionModelCatalog>,
    /// Access-independent per-connection conversation support catalogs.
    /// 独立于访问开关的逐连接对话支持目录。
    pub model_support_catalogs: BTreeMap<String, ConnectionModelCatalog>,
    /// Discovery source catalogs frozen before user overlays.
    /// 应用用户覆盖前冻结的发现来源目录。
    pub source_catalogs: BTreeMap<String, ConnectionModelCatalog>,
    /// Exact manifest revision per provider in the captured generation.
    /// 已捕获代际中每个供应商的精确 Manifest 修订号。
    pub manifest_revisions: BTreeMap<String, u64>,
    /// Enablement-independent provider-tool template model management inventory.
    /// 独立于启用状态的供应商工具模板模型管理目录。
    pub provider_tool_template_models: Vec<DeclaredProviderToolTemplateModel>,
    /// Inherited model access defaults keyed by the canonical provider/model identity.
    /// 按规范供应商/模型身份索引的继承模型访问默认值。
    pub model_default_enabled_by_key: BTreeMap<String, bool>,
    /// Signed initial checkbox policy for each account before any user-owned decision.
    /// 用户尚未作出决定时，每个账号的签名初始勾选策略。
    pub connection_model_initial_policies: BTreeMap<String, ConnectionModelAccessPolicy>,
    /// Provider-tool targets currently eligible under credential and model access policy.
    /// 当前在凭据与模型访问策略下合格的供应商工具目标。
    pub available_provider_tool_targets: Vec<AvailableProviderToolTarget>,
}

/// One coherent manifest plus model-catalog snapshot for provider diagnostics and preview.
/// 用于供应商诊断与预览的一份一致 Manifest 与模型目录快照。
#[derive(Clone)]
pub struct ProviderManifestRuntimeSnapshot {
    /// Exact compiled system manifest from the captured published generation.
    /// 来自已捕获发布代际的精确已编译系统 Manifest。
    pub manifest: Arc<ResolvedProviderManifest>,
    /// Effective connection catalogs used by request selection in the same generation.
    /// 同一代际中请求选择使用的有效连接目录。
    pub effective_catalogs: BTreeMap<String, ConnectionModelCatalog>,
    /// Access-independent model-support catalogs reconstructed from the same generation.
    /// 从同一代际重建且独立于账号访问开关的模型支持目录。
    pub model_support_catalogs: BTreeMap<String, ConnectionModelCatalog>,
    /// Discovery source catalogs paired with the same generation before user overlays.
    /// 与同一代际配对且位于用户覆盖之前的发现来源目录。
    pub source_catalogs: BTreeMap<String, ConnectionModelCatalog>,
    /// Exact manifest catalog generation captured with every field above.
    /// 与以上全部字段共同捕获的精确 Manifest 目录代际。
    pub manifest_generation: u64,
    /// Content-addressed digest of the complete captured manifest catalog.
    /// 完整已捕获 Manifest 目录的内容寻址摘要。
    pub catalog_revision: String,
}

/// Resolves one model's inherited default from an exact manifest snapshot and manual declaration.
/// 根据一份精确清单快照与手工声明解析一个模型的继承默认值。
///
/// # Parameters
/// 参数
/// `configuration` owns manual models; provider, service, credential, and model identities select
/// the exact scope; `manifest` is captured from the same immutable selection generation.
/// `configuration` 持有手工模型；供应商、服务、凭据与模型身份选择精确作用域；`manifest`
/// 从同一不可变选择代际捕获。
///
/// # Returns
/// 返回值
/// A manual model's declared default, then the selected manifest account default. Undeclared
/// models fail closed when no manifest exists.
/// 依次返回手工模型声明默认值、所选 Manifest 账号默认值；无 Manifest 时未声明模型失败关闭。
fn runtime_model_account_default_enabled_from_manifest(
    configuration: &PersistedAiConfiguration,
    provider_id: &str,
    service_entry_id: &str,
    credential_profile_id: Option<&str>,
    model_id: &str,
    manifest: Option<&ResolvedProviderManifest>,
) -> bool {
    configuration
        .custom_model(provider_id, model_id)
        .map_or_else(
            || {
                manifest.is_some_and(|manifest| {
                    crate::provider_manifest::manifest_default_enabled(
                        manifest,
                        service_entry_id,
                        credential_profile_id,
                        model_id,
                    )
                })
            },
            |model| model.enabled,
        )
}

/// Resolves the conversation-purpose default independently from initial account checks.
/// 独立于账号初始勾选解析会话用途默认值。
///
/// # Parameters
/// 参数
/// `configuration` owns manual models; route identifiers select the exact manifest rule;
/// `manifest` is captured from the immutable generation.
/// `configuration` 持有手工模型；路由标识选择精确清单规则；`manifest` 来自不可变代际。
///
/// # Returns
/// 返回值
/// Account opt-in defaults do not turn off the provider-wide conversation purpose; otherwise
/// the existing manual and manifest defaults remain authoritative.
/// 账号选择式默认值不会关闭供应商范围的会话用途；其他情况保留原有手工与清单默认值。
fn runtime_model_conversation_default_enabled_from_manifest(
    configuration: &PersistedAiConfiguration,
    provider_id: &str,
    service_entry_id: &str,
    credential_profile_id: Option<&str>,
    model_id: &str,
    manifest: Option<&ResolvedProviderManifest>,
) -> bool {
    if let Some(model) = configuration.custom_model(provider_id, model_id) {
        return model.enabled;
    }
    if manifest.is_some_and(|manifest| !manifest.default_enabled) {
        return true;
    }
    runtime_model_account_default_enabled_from_manifest(
        configuration,
        provider_id,
        service_entry_id,
        credential_profile_id,
        model_id,
        manifest,
    )
}

/// Resolves model enablement from one exact manifest snapshot and persisted user ownership.
/// 根据一份精确清单快照与持久化用户所有权解析模型启用状态。
///
/// # Parameters
/// 参数
/// `configuration` owns user overrides/manual models; provider and model identify support;
/// `manifest` is captured from the immutable generation.
/// `configuration` 持有用户覆盖/手动模型；供应商与模型身份选择受支持模型；
/// `manifest` 从不可变代际捕获。
///
/// # Returns
/// 返回值
/// Explicit user/manual-model ownership first, then manifest-backed support. The manifest's
/// `default_enabled` belongs to initial account access, not a supplier-wide admission gate.
/// 显式用户/手工模型所有权优先，其次是清单支持；清单 `default_enabled` 属于账号初始访问，
/// 不是供应商范围的准入门禁。
pub(super) fn runtime_model_is_enabled_from_manifest(
    configuration: &PersistedAiConfiguration,
    provider_id: &str,
    model_id: &str,
    manifest: Option<&ResolvedProviderManifest>,
) -> bool {
    // Account defaults are resolved by catalog overlay. A supported manifest model must remain
    // selectable after its account checkbox is enabled, even when it was initially unchecked.
    // 账号默认值由目录覆盖层解析；清单支持的模型即使初始未勾选，账号勾选后仍必须可选。
    let support_default = manifest.is_some()
        || configuration
            .custom_model(provider_id, model_id)
            .is_some_and(|model| model.enabled);
    configuration.model_is_enabled_with_default(provider_id, model_id, support_default)
}

/// Builds the exact effective-catalog view owned by one immutable published generation.
/// 构建一份不可变已发布代际持有的精确有效目录视图。
///
/// # Parameters
/// 参数
/// `generation` owns the exact activated catalogs captured at the same activation boundary.
/// `generation` 持有在同一激活边界捕获的精确已激活目录。
///
/// # Returns
/// 返回值
/// The exact activated connection catalog map.
/// 返回精确的已激活连接目录映射。
pub(super) fn effective_catalog_snapshot_from_generation(
    generation: &ResolvedProviderGeneration,
) -> BTreeMap<String, ConnectionModelCatalog> {
    generation.activated_effective_catalogs.snapshot()
}

/// Applies one catalog overlay pass from one exact immutable provider generation.
/// 从一个精确不可变供应商代际应用一次目录覆盖。
///
/// # Parameters
/// 参数
/// `configuration` owns user policy; provider and service identifiers select the exact rule
/// scope; `account_usage` is frozen account evidence; `generation` owns the matching Manifest;
/// and `catalog` is rebuilt in place.
/// `configuration` 持有用户策略；供应商与服务标识选择精确规则作用域；`account_usage`
/// 是冻结账号证据；`generation` 持有匹配 Manifest；`catalog` 会被原位重建。
///
/// # Returns
/// 返回值
/// Success after source policy and every overlay resolve from the same generation.
/// 来源策略与全部覆盖均从同一代际解析完成后返回成功。
fn apply_model_catalog_overlays_from_generation(
    configuration: &PersistedAiConfiguration,
    provider_id: &str,
    service_entry_id: &str,
    account_usage: Option<&AccountUsageSnapshot>,
    generation: &ResolvedProviderGeneration,
    catalog: &mut ConnectionModelCatalog,
) -> Result<(), String> {
    let manifest = generation.manifests.manifest(provider_id);
    let source_policy =
        resolved_model_source_policy(configuration, provider_id, service_entry_id, manifest)?;
    apply_model_catalog_overlays_with_account_access(
        configuration,
        provider_id,
        service_entry_id,
        account_usage,
        source_policy,
        manifest,
        catalog,
    )
}

/// Resolves candidate model overlays from one explicit immutable input set.
/// 从一组显式不可变输入解析候选模型覆盖。
///
/// # Parameters
/// 参数
/// `configuration` supplies candidate user/account policy; `manifests`, `source_catalogs`, and
/// `usage_cache` must all come from the same published generation or the same locked activation.
/// `configuration` 提供候选用户/账号策略；`manifests`、`source_catalogs` 与 `usage_cache` 必须
/// 全部来自同一已发布代际或同一次加锁激活。
///
/// # Returns
/// 返回值
/// A complete effective catalog set after every model rule, access layer, capability layer, and
/// strict size limit is applied.
/// 返回应用全部模型规则、访问层、能力层与严格大小限制后的完整有效目录集合。
pub(super) fn resolve_model_catalog_overlays_from_inputs(
    configuration: &PersistedAiConfiguration,
    manifests: &crate::provider_manifest::ResolvedProviderCatalog,
    source_catalogs: &RuntimeConnectionCatalogs,
    usage_cache: &BTreeMap<String, AccountUsageSnapshot>,
    projection_use: CatalogProjectionUse,
) -> Result<RuntimeConnectionCatalogs, String> {
    // effective accumulates only catalogs reconstructable from the explicit input set.
    // effective 仅累积可从显式输入集合重建的目录。
    let mut effective = RuntimeConnectionCatalogs::default();
    for connection in configuration.connection_graph.connections.values() {
        // Disabled credentials retain persisted metadata but contribute no executable catalog.
        // 已停用凭据保留持久化元数据，但不贡献任何可执行目录。
        if !configuration.connection_is_enabled(connection.id.as_str()) {
            continue;
        }
        // An invalid standalone custom Manifest isolates only its provider. The persisted
        // connection remains available for repair, but it must never enter an executable catalog.
        // 无效的独立自定义 Manifest 只隔离其所属供应商。持久化连接仍保留供修复，但绝不能
        // 进入可执行目录。
        if configuration.custom_provider_manifest_is_isolated(connection.provider_id.as_str()) {
            continue;
        }
        // manifest and sourcePolicy come from the same immutable catalog supplied to this pass.
        // manifest 与 sourcePolicy 来自本次解析传入的同一不可变目录。
        let manifest = manifests.manifest(connection.provider_id.as_str());
        if manifest.is_some_and(|manifest| {
            manifest
                .descriptor
                .service_entries
                .iter()
                .find(|entry| entry.id == connection.service_entry_id)
                .is_some_and(|entry| entry.kind == crate::ServiceEntryKind::ProviderTools)
        }) {
            continue;
        }
        let source_policy = resolved_model_source_policy(
            configuration,
            connection.provider_id.as_str(),
            connection.service_entry_id.as_str(),
            manifest,
        )?;
        // catalog starts from frozen source evidence. Manifest/manual policies may independently
        // reconstruct a baseline when no discovery source has ever been published.
        // catalog 从冻结来源证据开始；从未发布发现来源时，Manifest/手工策略可独立重建基线。
        let mut catalog = match source_catalogs.get(connection.id.as_str()).cloned() {
            Some(catalog) => catalog,
            None if source_policy.includes_manifest() => empty_model_source_catalog(
                connection.id.as_str(),
                1,
                crate::client::MANIFEST_STATIC_DISCOVERY_SCHEMA_REVISION,
            ),
            None if source_policy.includes_manual()
                || super::catalog_policy::has_system_manual_models_for_entry(
                    configuration,
                    connection.provider_id.as_str(),
                    connection.service_entry_id.as_str(),
                ) =>
            {
                let Some(catalog) = manual_custom_catalog(configuration, connection, 1) else {
                    continue;
                };
                catalog
            }
            None => continue,
        };
        // account_usage is frozen with the same source generation as every catalog source.
        // account_usage 与所有目录来源冻结在同一个来源代际中。
        let account_usage = usage_cache.get(connection.id.as_str());
        apply_model_catalog_overlays_with_account_access(
            configuration,
            connection.provider_id.as_str(),
            connection.service_entry_id.as_str(),
            account_usage,
            source_policy,
            manifest,
            &mut catalog,
        )?;
        match projection_use {
            CatalogProjectionUse::Execution => {
                retain_models_with_valid_limits(&mut catalog);
                validate_resolved_catalog_limits(&catalog)?;
            }
            CatalogProjectionUse::Management => validate_catalog_model_count(&catalog)?,
        }
        effective.replace(catalog);
    }
    Ok(effective)
}

/// Reports conversation-model support against one exact immutable provider generation.
/// 针对一个精确不可变供应商代际报告对话模型支持状态。
///
/// # Parameters
/// 参数
/// `configuration` owns manual declarations and connection membership; `provider_id` and
/// `model_id` select the model; `generation` freezes discovery, plans, and manifest rules.
/// `configuration` 持有手工声明与连接成员关系；`provider_id` 和 `model_id` 选择模型；
/// `generation` 冻结发现、套餐与 Manifest 规则。
///
/// # Returns
/// 返回值
/// `true` when a manual declaration or any access-independent account catalog contains the model.
/// 当手工声明或任一独立于访问开关的账号目录包含该模型时返回 `true`。
fn provider_conversation_model_is_supported_in_generation(
    configuration: &PersistedAiConfiguration,
    provider_id: &str,
    model_id: &str,
    generation: &ResolvedProviderGeneration,
) -> Result<bool, String> {
    let runtime_configuration =
        runtime_configuration_for_manifest_catalog(configuration, generation.manifests.as_ref());
    let configuration = &runtime_configuration;
    if configuration.custom_model(provider_id, model_id).is_some() {
        return Ok(true);
    }
    // support_configuration removes only account access policy; provider membership remains authoritative.
    // support_configuration 只移除账号访问策略；供应商成员关系仍保持权威。
    let mut support_configuration = configuration.clone();
    support_configuration
        .connection_model_access_policies
        .clear();
    // catalogs reconstructs support from the same frozen source, plan, and manifest generation.
    // catalogs 从同一冻结来源、套餐与 Manifest 代际重建支持目录。
    let catalogs = resolve_model_catalog_overlays_from_inputs(
        &support_configuration,
        generation.manifests.as_ref(),
        &generation.source_catalogs,
        &generation.usage_cache,
        CatalogProjectionUse::Execution,
    )?;
    Ok(support_configuration
        .connection_graph
        .connections
        .values()
        .filter(|connection| connection.provider_id == provider_id)
        .any(|connection| {
            catalogs
                .get(connection.id.as_str())
                .is_some_and(|catalog| catalog.models.iter().any(|model| model.id == model_id))
        }))
}

/// Lists every credential-eligible model declared by reviewed provider-tool templates in one generation.
/// 列出一个代际中由已审核供应商工具模板声明且凭据合格的全部模型。
///
/// # Parameters
/// 参数
/// `configuration` supplies configured credentials and pending-validation state; `generation`
/// freezes the manifests, template provenance, and concrete target contracts for the whole scan.
/// `configuration` 提供已配置凭据与待验证状态；`generation` 为整次扫描冻结 Manifest、模板来源
/// 与具体目标契约。
///
/// # Returns
/// 返回值
/// A deterministic connection/model list independent from provider-level model enablement, or an
/// explicit invariant failure when compiled provenance cannot resolve its target.
/// 返回与供应商级模型启用状态无关的确定性连接/模型列表；若编译来源无法解析其目标则返回显式
/// 不变量错误。
fn provider_tool_template_models_in_generation(
    configuration: &PersistedAiConfiguration,
    generation: &ResolvedProviderGeneration,
) -> Result<Vec<DeclaredProviderToolTemplateModel>, String> {
    // declarations deduplicates models that instantiate several reviewed targets for one connection.
    // declarations 对同一连接实例化多个已审核目标的模型进行去重。
    let mut declarations = BTreeMap::new();
    for connection in configuration.connection_graph.connections.values() {
        if !configuration.connection_is_enabled(connection.id.as_str())
            || configuration.connection_is_pending_validation(connection.id.as_str())
        {
            continue;
        }
        // manifest is the exact provider document frozen in the selected generation.
        // manifest 是在所选代际中冻结的精确供应商文档。
        let Some(manifest) = generation
            .manifests
            .manifest(connection.provider_id.as_str())
        else {
            continue;
        };
        if !manifest
            .descriptor
            .service_entries
            .iter()
            .any(|entry| entry.id == connection.service_entry_id)
        {
            return Err(format!(
                "Service entry `{}` is not registered for provider `{}`.",
                connection.service_entry_id, connection.provider_id
            ));
        }
        for instance in manifest.tool_target_template_instances.values() {
            if instance.service_entry_id != connection.service_entry_id {
                continue;
            }
            // target resolves the exact credential scope and AI-visibility contract of this instance.
            // target 解析该实例的精确凭据作用域与 AI 可见性契约。
            let target = manifest
                .tool_targets
                .iter()
                .find(|target| target.target_id == instance.target_id)
                .ok_or_else(|| {
                    format!(
                        "Provider `{}` template instance `{}` has no compiled target.",
                        connection.provider_id, instance.target_id
                    )
                })?;
            if !target.contract.capability.is_ai_visible()
                || target.service_entry_id != connection.service_entry_id
                || !provider_tool_target_allows_credential(
                    target,
                    connection.credential_profile_id.as_str(),
                )
            {
                continue;
            }
            // declaration_key preserves account identity while collapsing multiple operations for one model.
            // declaration_key 保留账号身份，同时折叠同一模型的多个操作。
            let declaration_key = (connection.id.clone(), instance.model_id.clone());
            declarations
                .entry(declaration_key)
                .and_modify(|declaration: &mut DeclaredProviderToolTemplateModel| {
                    declaration.capabilities.insert(target.contract.capability);
                })
                .or_insert_with(|| DeclaredProviderToolTemplateModel {
                    provider_id: connection.provider_id.clone(),
                    connection_id: connection.id.clone(),
                    service_entry_id: connection.service_entry_id.clone(),
                    model_id: instance.model_id.clone(),
                    display_name: instance.model_display_name.clone(),
                    capabilities: std::collections::BTreeSet::from([target.contract.capability]),
                });
        }
    }
    Ok(declarations.into_values().collect())
}

impl PublicAiRuntime {
    /// Builds every browser-settings catalog view from one immutable provider generation.
    /// 从一个不可变供应商代际构建全部浏览器设置目录视图。
    ///
    /// # Parameters
    /// 参数
    /// `_configuration` is retained for API compatibility; the captured generation owns the
    /// authoritative configuration, manifests, and discovery inputs for every projection.
    /// `_configuration` 仅为 API 兼容保留；已捕获代际持有各项投影所用的权威配置、
    /// Manifest 与发现输入。
    ///
    /// # Returns
    /// 返回值
    /// One coherent settings projection containing providers, conversation catalogs, template
    /// models, tool targets, and manifest revisions from the same publication boundary.
    /// 返回一份一致设置投影，其中供应商、对话目录、模板模型、工具目标与 Manifest 修订号均
    /// 来自同一发布边界。
    pub fn settings_catalog_snapshot(
        &self,
        _configuration: &PersistedAiConfiguration,
    ) -> Result<PublicAiRuntimeSettingsCatalogSnapshot, String> {
        // generation is retained for the complete projection so concurrent activation cannot mix views.
        // generation 在完整投影期间持续持有，避免并发激活混合视图。
        let generation = self.selection_catalog_snapshot()?;
        let configuration = generation.configuration.as_ref();
        // runtimeConfiguration removes provider identities without executable authority in this
        // exact captured generation while preserving the caller's persisted repair image.
        // runtimeConfiguration 移除当前精确捕获代际中没有可执行权威的供应商身份，同时保留
        // 调用方的持久化修复镜像。
        let runtime_configuration = runtime_configuration_for_manifest_catalog(
            configuration,
            generation.manifests.as_ref(),
        );
        let configuration = &runtime_configuration;
        // system is the registry compiled from the captured manifest catalog.
        // system 是从已捕获 Manifest 目录编译的注册表。
        let system = ProviderRegistry::from_catalog(generation.manifests.as_ref())?;
        // providers applies the explicit custom-provider compatibility layer to that registry.
        // providers 把显式自定义供应商兼容层应用到该注册表。
        let providers =
            ProviderRegistry::effective_from_system(system, &configuration.custom_providers)?;
        // catalogs applies current access and capability layers to frozen source inputs.
        // catalogs 把当前访问与能力层应用到冻结来源输入。
        let catalogs = resolve_model_catalog_overlays_from_inputs(
            configuration,
            generation.manifests.as_ref(),
            &generation.source_catalogs,
            &generation.usage_cache,
            CatalogProjectionUse::Execution,
        )?
        .snapshot();
        // support_configuration removes only account access switches for the management inventory.
        // support_configuration 仅为管理目录移除账号访问开关。
        let mut support_configuration = configuration.clone();
        for connection_id in configuration.connection_graph.connections.keys() {
            support_configuration.allow_all_connection_models_for_support(connection_id);
        }
        // model_support_catalogs retains disabled account membership from the same generation.
        // model_support_catalogs 从同一代际保留已关闭账号成员关系。
        let model_support_catalogs = resolve_model_catalog_overlays_from_inputs(
            &support_configuration,
            generation.manifests.as_ref(),
            &generation.source_catalogs,
            &generation.usage_cache,
            CatalogProjectionUse::Management,
        )?
        .snapshot();
        // source_catalogs is the pre-overlay discovery inventory captured with this generation.
        // source_catalogs 是与本代际共同捕获的覆盖前发现目录。
        let source_catalogs = generation
            .source_catalogs
            .snapshot()
            .into_iter()
            .filter(|(connection_id, _)| {
                configuration
                    .connection_graph
                    .connections
                    .contains_key(connection_id.as_str())
            })
            .collect();
        // manifest_revisions exposes the exact revision identity of the retained generation.
        // manifest_revisions 暴露所持有代际的精确修订身份。
        let manifest_revisions = generation
            .manifests
            .manifests
            .iter()
            .map(|(provider_id, manifest)| (provider_id.clone(), manifest.revision))
            .collect();
        // provider_tool_template_models remains visible independently from enabled state.
        // provider_tool_template_models 独立于启用状态保持可见。
        let provider_tool_template_models =
            provider_tool_template_models_in_generation(configuration, generation.as_ref())?;
        // model_default_enabled_by_key freezes browser access defaults with the same manifest generation.
        // model_default_enabled_by_key 把浏览器访问默认值与同一 Manifest 代际共同冻结。
        let mut model_default_enabled_by_key = BTreeMap::new();
        // connection_model_initial_policies keeps exact account defaults separate from the
        // provider/model OR projection used only by the conversation-purpose settings row.
        // connection_model_initial_policies 保留精确账号默认值，不与仅供会话用途设置行使用的
        // 供应商/模型 OR 投影混合。
        let mut connection_model_initial_policies = BTreeMap::new();
        for connection in configuration.connection_graph.connections.values() {
            let scoped_manifest = generation
                .manifests
                .manifest(connection.provider_id.as_str());
            let default_enabled = scoped_manifest.is_none_or(|manifest| manifest.default_enabled);
            let mut model_enabled_overrides = BTreeMap::new();
            if let Some(manifest) = scoped_manifest.filter(|_| !default_enabled) {
                // Exact signed choices stay inherited even when an account is edited before
                // every featured model appears in its remote discovery response.
                // 即使账号在远端发现全部精选模型前已被编辑，精确签名选择仍继续继承。
                for rule in &manifest.rule_instances {
                    if rule.service_entry_id != connection.service_entry_id
                        || rule.matcher.kind != crate::provider_manifest::MatcherKind::Exact
                        || (!rule.credential_profile_ids.is_empty()
                            && !rule
                                .credential_profile_ids
                                .contains(&connection.credential_profile_id))
                    {
                        continue;
                    }
                    let rule_default = crate::provider_manifest::manifest_default_enabled(
                        manifest,
                        connection.service_entry_id.as_str(),
                        Some(connection.credential_profile_id.as_str()),
                        rule.matcher.value.as_str(),
                    );
                    if rule_default != default_enabled {
                        model_enabled_overrides.insert(rule.matcher.value.clone(), rule_default);
                    }
                }
            }
            connection_model_initial_policies.insert(
                connection.id.clone(),
                ConnectionModelAccessPolicy {
                    default_enabled,
                    model_enabled_overrides,
                },
            );
        }
        for (connection_id, catalog) in &model_support_catalogs {
            // connection resolves the owning provider for this exact account catalog.
            // connection 解析该精确账号目录所属的供应商。
            let Some(connection) = configuration
                .connection_graph
                .connections
                .get(connection_id.as_str())
            else {
                continue;
            };
            let scoped_manifest = generation
                .manifests
                .manifest(connection.provider_id.as_str());
            let mut initial_policy = connection_model_initial_policies
                .remove(connection_id)
                .ok_or_else(|| format!("Account `{connection_id}` has no initial model policy."))?;
            for model in &catalog.models {
                // scoped_default preserves the exact entry and credential rule scope.
                // scoped_default 保留精确入口与凭据规则作用域。
                let scoped_default = runtime_model_account_default_enabled_from_manifest(
                    configuration,
                    connection.provider_id.as_str(),
                    connection.service_entry_id.as_str(),
                    Some(connection.credential_profile_id.as_str()),
                    model.id.as_str(),
                    scoped_manifest,
                );
                if !initial_policy.default_enabled && scoped_default {
                    initial_policy
                        .model_enabled_overrides
                        .insert(model.id.clone(), scoped_default);
                }
                // The conversation-purpose row is independent of account opt-in; exact route
                // defaults still aggregate with a stable, order-free OR.
                // 会话用途行独立于账号初始勾选；精确路由默认值仍以稳定且无顺序依赖的 OR 聚合。
                let conversation_default = runtime_model_conversation_default_enabled_from_manifest(
                    configuration,
                    connection.provider_id.as_str(),
                    connection.service_entry_id.as_str(),
                    Some(connection.credential_profile_id.as_str()),
                    model.id.as_str(),
                    scoped_manifest,
                );
                model_default_enabled_by_key
                    .entry(PersistedAiConfiguration::provider_model_key(
                        connection.provider_id.as_str(),
                        model.id.as_str(),
                    ))
                    .and_modify(|default_enabled| *default_enabled |= conversation_default)
                    .or_insert(conversation_default);
            }
            connection_model_initial_policies.insert(connection_id.clone(), initial_policy);
        }
        for model in &provider_tool_template_models {
            // connection restores the credential scope intentionally omitted from the public inventory row.
            // connection 恢复公共管理目录行刻意省略的凭据作用域。
            let connection = configuration
                .connection_graph
                .connections
                .get(model.connection_id.as_str())
                .ok_or_else(|| {
                    format!(
                        "Provider-tool template model `{}/{}` references missing connection `{}`.",
                        model.provider_id, model.model_id, model.connection_id
                    )
                })?;
            let scoped_default = runtime_model_conversation_default_enabled_from_manifest(
                configuration,
                model.provider_id.as_str(),
                model.service_entry_id.as_str(),
                Some(connection.credential_profile_id.as_str()),
                model.model_id.as_str(),
                generation.manifests.manifest(model.provider_id.as_str()),
            );
            model_default_enabled_by_key
                .entry(PersistedAiConfiguration::provider_model_key(
                    model.provider_id.as_str(),
                    model.model_id.as_str(),
                ))
                .and_modify(|default_enabled| *default_enabled |= scoped_default)
                .or_insert(scoped_default);
        }
        // available_provider_tool_targets applies current access against the same template authority.
        // available_provider_tool_targets 针对同一模板权限应用当前访问状态。
        let available_provider_tool_targets = Self::available_provider_tool_targets_in_generation(
            configuration,
            generation.as_ref(),
            &providers,
        )?;
        Ok(PublicAiRuntimeSettingsCatalogSnapshot {
            runtime_configuration,
            providers,
            catalogs,
            model_support_catalogs,
            source_catalogs,
            manifest_revisions,
            provider_tool_template_models,
            model_default_enabled_by_key,
            connection_model_initial_policies,
            available_provider_tool_targets,
        })
    }

    /// Resolves the inherited default of one model from the published manifest generation.
    /// 从已发布清单代际解析一个模型的继承默认值。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` owns manual model declarations and configured route scopes; `provider_id`
    /// and `model_id` identify the provider-level management row being projected.
    /// `configuration` 持有手工模型声明与已配置路由作用域；`provider_id` 与 `model_id` 标识待投影的
    /// 供应商级管理行。
    ///
    /// # Returns
    /// 返回值
    /// The stable OR of exact conversation-purpose defaults. Without a configured route, the
    /// manual or manifest conversation-purpose default is returned; account opt-in is separate.
    /// 返回精确会话用途默认值的稳定 OR；无已配置路由时返回手工或清单的会话用途默认值；
    /// 账号初始勾选独立计算。
    pub fn model_default_enabled(
        &self,
        configuration: &PersistedAiConfiguration,
        provider_id: &str,
        model_id: &str,
    ) -> bool {
        // manifest retains one published default snapshot for this complete lookup.
        // manifest 为本次完整查询保留一份已发布默认值快照。
        let manifest = self
            .resolved_provider_generation
            .read()
            .ok()
            .and_then(|generation| generation.manifests.manifest_arc(provider_id));
        // scoped_default aggregates all concrete route scopes without relying on map order.
        // scoped_default 不依赖 Map 顺序地聚合全部具体路由作用域。
        let scoped_default = configuration
            .connection_graph
            .connections
            .values()
            .filter(|connection| connection.provider_id == provider_id)
            .map(|connection| {
                runtime_model_conversation_default_enabled_from_manifest(
                    configuration,
                    provider_id,
                    connection.service_entry_id.as_str(),
                    Some(connection.credential_profile_id.as_str()),
                    model_id,
                    manifest.as_deref(),
                )
            })
            .reduce(|left, right| left || right);
        scoped_default.unwrap_or_else(|| {
            let Some(manifest) = manifest.as_deref() else {
                return runtime_model_conversation_default_enabled_from_manifest(
                    configuration,
                    provider_id,
                    "",
                    None,
                    model_id,
                    None,
                );
            };
            // declared_scope_default projects a provider-level management row across every
            // manifest-declared service and credential when no account is configured yet.
            // declared_scope_default 在尚未配置账号时跨全部 Manifest 声明的服务与凭据投影
            // 供应商级管理行。
            manifest.descriptor.service_entries.iter().any(|entry| {
                if entry.credential_profiles.is_empty() {
                    return runtime_model_conversation_default_enabled_from_manifest(
                        configuration,
                        provider_id,
                        entry.id.as_str(),
                        None,
                        model_id,
                        Some(manifest),
                    );
                }
                entry.credential_profiles.iter().any(|profile| {
                    runtime_model_conversation_default_enabled_from_manifest(
                        configuration,
                        provider_id,
                        entry.id.as_str(),
                        Some(profile.id.as_str()),
                        model_id,
                        Some(manifest),
                    )
                })
            })
        })
    }

    /// Reports one model's manifest-first enabled state for an exact configured credential route.
    /// 报告一个模型在精确已配置凭据路由上的 Manifest-first 启用状态。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` owns user access layers; `connection_id` selects the exact service and
    /// credential scope; `model_id` identifies the model inside that route.
    /// `configuration` 持有用户访问层；`connection_id` 选择精确服务与凭据作用域；`model_id`
    /// 标识该路由内的模型。
    ///
    /// # Returns
    /// 返回值
    /// The exact scoped manifest/manual default after the provider-level user override, or an
    /// explicit error when the connection or published generation cannot be read.
    /// 返回应用供应商级用户覆盖后的精确作用域 Manifest/手工默认值；连接或已发布代际不可读时显式报错。
    pub fn model_is_enabled_for_connection(
        &self,
        configuration: &PersistedAiConfiguration,
        connection_id: &str,
        model_id: &str,
    ) -> Result<bool, String> {
        // connection is the unique persisted owner of service-entry and credential scope.
        // connection 是服务入口与凭据作用域的唯一持久化所有者。
        let connection = configuration
            .connection_graph
            .connections
            .get(connection_id)
            .ok_or_else(|| format!("Connection `{connection_id}` is not configured."))?;
        let generation = self.selection_catalog_snapshot()?;
        let manifest = generation
            .manifests
            .manifest(connection.provider_id.as_str());
        Ok(runtime_model_is_enabled_from_manifest(
            configuration,
            connection.provider_id.as_str(),
            model_id,
            manifest,
        ))
    }

    /// Reports the manifest-first enabled state of one model under the published manifests.
    /// 报告一个模型在已发布清单下的 manifest-first 启用状态。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` is the exact persisted connection graph; `provider_id` and `model_id`
    /// identify the model whose default-enabled state is queried.
    /// `configuration` 是精确持久化连接图；`provider_id` 与 `model_id` 标识被查询默认启用状态的模型。
    ///
    /// # Returns
    /// 返回值
    /// Explicit user/manual-model state wins, then the published conversation-purpose default;
    /// undeclared models without a manifest fail closed.
    /// 显式用户/手工模型状态优先；其次采用已发布的会话用途默认值；无 Manifest 的未声明模型失败关闭。
    pub fn model_is_enabled(
        &self,
        configuration: &PersistedAiConfiguration,
        provider_id: &str,
        model_id: &str,
    ) -> bool {
        configuration.model_is_enabled_with_default(
            provider_id,
            model_id,
            self.model_default_enabled(configuration, provider_id, model_id),
        )
    }

    /// Returns whether any configured connection for a provider currently exposes a model.
    /// 返回某个供应商是否存在任一已配置连接在当前进程内目录中暴露指定模型。
    ///
    /// # Parameters
    /// 参数
    ///
    /// `configuration` is the exact persisted connection graph; `provider_id` and `model_id`
    /// identify the model whose current runtime visibility is queried.
    /// `configuration` 是精确的持久化连接图；`provider_id` 与 `model_id` 标识待查询当前运行时可见性的模型。
    ///
    /// # Returns
    /// 返回值
    ///
    /// `true` only when a matching configured connection has a current in-memory catalog entry.
    /// 仅当匹配的已配置连接具有当前内存目录条目时返回 `true`。
    pub fn model_is_available(
        &self,
        configuration: &PersistedAiConfiguration,
        provider_id: &str,
        model_id: &str,
    ) -> Result<bool, String> {
        // generation freezes enablement defaults and catalog membership for this whole decision.
        // generation 为本次完整判断冻结启用默认值与目录成员关系。
        let generation = self.selection_catalog_snapshot()?;
        let manifest = generation.manifests.manifest(provider_id);
        // catalogs applies the same source-aware fallback rule as execution selection.
        // catalogs 应用与执行选择相同的来源感知回退规则。
        let catalogs = effective_catalog_snapshot_from_generation(&generation);
        Ok(configuration
            .connection_graph
            .connections
            .values()
            .filter(|connection| connection.provider_id == provider_id)
            .filter(|connection| {
                configuration.connection_is_enabled(connection.id.as_str())
                    && !configuration.connection_is_pending_validation(connection.id.as_str())
            })
            .any(|connection| {
                runtime_model_is_enabled_from_manifest(
                    configuration,
                    provider_id,
                    model_id,
                    manifest,
                ) && catalogs
                    .get(connection.id.as_str())
                    .is_some_and(|catalog| catalog.models.iter().any(|model| model.id == model_id))
            }))
    }

    /// Returns every provider/model selector currently exposed by configured in-memory catalogs.
    /// 返回当前已配置内存目录暴露的全部供应商/模型选择器。
    ///
    /// # Parameters
    /// 参数
    ///
    /// `configuration` is the exact persisted connection graph used to filter orphan catalogs.
    /// `configuration` 是用于过滤孤立目录的精确持久化连接图。
    ///
    /// # Returns
    /// 返回值
    ///
    /// A stable deduplicated `provider/model` selector set.
    /// 返回稳定去重后的 `provider/model` 选择器集合。
    pub fn available_model_selectors(
        &self,
        configuration: &PersistedAiConfiguration,
    ) -> Result<std::collections::BTreeSet<String>, String> {
        // generation and catalogs remain paired throughout selector construction.
        // generation 与 catalogs 在整个选择器构建过程中保持配对。
        let generation = self.selection_catalog_snapshot()?;
        let catalogs = effective_catalog_snapshot_from_generation(&generation);
        Ok(configuration
            .connection_graph
            .connections
            .values()
            .filter(|connection| {
                configuration.connection_is_enabled(connection.id.as_str())
                    && !configuration.connection_is_pending_validation(connection.id.as_str())
            })
            .filter_map(|connection| {
                catalogs
                    .get(connection.id.as_str())
                    .map(|catalog| (connection, catalog))
            })
            .flat_map(|(connection, catalog)| {
                catalog
                    .models
                    .iter()
                    .filter(|model| {
                        runtime_model_is_enabled_from_manifest(
                            configuration,
                            connection.provider_id.as_str(),
                            model.id.as_str(),
                            generation
                                .manifests
                                .manifest(connection.provider_id.as_str()),
                        ) && model_supports_text_conversation(&model.capabilities)
                    })
                    .map(move |model| format!("{}/{}", connection.provider_id, model.id))
            })
            .collect())
    }

    /// Applies user overlays and manifest-owned account model access to one connection catalog.
    /// 对一个连接目录应用用户覆盖与 Manifest 持有的账号模型访问规则。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` owns user layers; `provider_id` and `service_entry_id` identify the
    /// exact manifest scope; `account_usage` is confirmed provider evidence; `catalog`
    /// is rebuilt in place.
    /// `configuration` 持有用户层；`provider_id` 与 `service_entry_id` 标识精确 Manifest 作用域；
    /// `account_usage` 是已确认供应商证据；`catalog` 会被原位重建。
    ///
    /// # Returns
    /// 返回值
    /// Success after the captured published manifest and user layers produce one valid catalog.
    /// 已捕获的已发布 Manifest 与用户层生成一份有效目录后返回成功。
    pub(super) fn apply_model_catalog_overlays_with_account_access(
        &self,
        configuration: &PersistedAiConfiguration,
        provider_id: &str,
        service_entry_id: &str,
        account_usage: Option<&AccountUsageSnapshot>,
        catalog: &mut ConnectionModelCatalog,
    ) -> Result<(), String> {
        let generation = self.selection_catalog_snapshot()?;
        apply_model_catalog_overlays_from_generation(
            configuration,
            provider_id,
            service_entry_id,
            account_usage,
            generation.as_ref(),
            catalog,
        )
    }

    /// Validates every currently reconstructable account catalog for one candidate configuration.
    /// 校验一个候选配置下当前所有可重建的账号目录。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` contains the complete proposed provider defaults, template decisions,
    /// manual declarations, availability choices, and user capability differences.
    /// `configuration` 包含完整候选供应商默认值、模板决策、手工声明、可用性选择与用户能力差异。
    ///
    /// # Returns
    /// 返回值
    /// Success when every source-backed or manual account projection has valid final limits.
    /// 当每个源目录或手工账号投影都具有有效最终限制时返回成功。
    pub fn validate_model_catalog_overlays(
        &self,
        configuration: &PersistedAiConfiguration,
    ) -> Result<(), String> {
        self.resolve_model_catalog_overlays(configuration)
            .map(|_| ())
    }

    /// Resolves every currently reconstructable account catalog without mutating runtime state.
    /// 在不修改运行时状态的前提下解析当前全部可重建账号目录。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` is the complete candidate configuration whose overlays are evaluated.
    /// `configuration` 是待求值覆盖的完整候选配置。
    ///
    /// # Returns
    /// 返回值
    /// A stable connection-to-effective-catalog snapshot suitable for validation and fallback decisions.
    /// 返回稳定的连接到有效目录快照，可用于校验与降级决策。
    pub fn resolved_model_catalog_snapshot(
        &self,
        configuration: &PersistedAiConfiguration,
    ) -> Result<BTreeMap<String, ConnectionModelCatalog>, String> {
        Ok(self
            .resolve_model_catalog_overlays(configuration)?
            .snapshot())
    }

    /// Previews one configuration against the exact mutable sources awaiting activation.
    /// 针对等待激活的精确可变来源预览一份配置。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` is the complete candidate whose active selection and model membership
    /// must be reconciled before durable persistence.
    /// `configuration` 是必须在持久化前协调活动选择与模型成员关系的完整候选。
    ///
    /// # Returns
    /// 返回值
    /// A detached effective-catalog snapshot built in an isolated runtime. No request-visible
    /// generation or mutable main-runtime catalog is changed.
    /// 返回在隔离运行时中构建的独立有效目录快照；不会改变任何请求可见代际或主运行时可变目录。
    pub fn preview_staged_model_catalog_overlays(
        &self,
        configuration: &PersistedAiConfiguration,
    ) -> Result<BTreeMap<String, ConnectionModelCatalog>, String> {
        let _refresh_guard = self.catalog_refresh_flight.try_lock().map_err(|_| {
            String::from(
                "Provider manifest catalog activation is in progress; retry the staged catalog preview after activation completes.",
            )
        })?;
        // beforeImage freezes every staged source and the published Manifest authority used by
        // the detached preview.
        // beforeImage 冻结独立预览使用的全部暂存来源与已发布 Manifest 权威。
        let before_image = self.capture_activation_snapshot()?;
        // isolatedRuntime ensures candidate configuration compilation cannot publish any field.
        // isolatedRuntime 确保候选配置编译不会发布任何字段。
        let isolated_runtime = self.isolated_catalog_activation_runtime(
            &before_image,
            Arc::clone(&before_image.resolved_provider_generation.manifests),
        );
        isolated_runtime.reapply_model_catalog_overlays(configuration)?;
        isolated_runtime.catalog_snapshot()
    }

    /// Reports conversation-model support independently from account access switches.
    /// 报告独立于账号访问开关的对话模型支持状态。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` owns provider membership, while `provider_id` and `model_id` select one model.
    /// `configuration` 持有供应商成员关系，`provider_id` 与 `model_id` 选择一个模型。
    ///
    /// # Returns
    /// 返回值
    /// `true` when a manual declaration or any account conversation support catalog contains the model.
    /// 当手工声明或任一账号对话支持目录包含该模型时返回 `true`。
    pub fn provider_conversation_model_is_supported(
        &self,
        configuration: &PersistedAiConfiguration,
        provider_id: &str,
        model_id: &str,
    ) -> Result<bool, String> {
        // generation binds support reconstruction to one published source/rule/plan snapshot.
        // generation 把支持目录重建绑定到一份已发布来源/规则/套餐快照。
        let generation = self.selection_catalog_snapshot()?;
        provider_conversation_model_is_supported_in_generation(
            configuration,
            provider_id,
            model_id,
            generation.as_ref(),
        )
    }

    /// Reports all manageable provider model identities independently from account access switches.
    /// 报告独立于账号访问开关的全部可管理供应商模型身份。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` owns provider membership, while `provider_id` and `model_id` select one
    /// conversation, manual, or signed provider-tool template model.
    /// `configuration` 持有供应商成员关系，`provider_id` 与 `model_id` 选择一个对话、手工或
    /// 签名供应商工具模板模型。
    ///
    /// # Returns
    /// 返回值
    /// `true` when either the conversation inventory or the signed provider-tool template inventory
    /// declares the exact model.
    /// 当对话目录或签名供应商工具模板目录声明该精确模型时返回 `true`。
    pub fn provider_model_is_supported(
        &self,
        configuration: &PersistedAiConfiguration,
        provider_id: &str,
        model_id: &str,
    ) -> Result<bool, String> {
        // generation keeps conversation and provider-tool inventories on one publication boundary.
        // generation 让对话目录与供应商工具目录处于同一发布边界。
        let generation = self.selection_catalog_snapshot()?;
        if provider_conversation_model_is_supported_in_generation(
            configuration,
            provider_id,
            model_id,
            generation.as_ref(),
        )? {
            return Ok(true);
        }
        Ok(
            provider_tool_template_models_in_generation(configuration, generation.as_ref())?
                .iter()
                .any(|model| model.provider_id == provider_id && model.model_id == model_id),
        )
    }

    /// Returns provider-tool template models independently from their provider-level enabled state.
    /// 返回独立于供应商级启用状态的供应商工具模板模型。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` supplies the exact configured connections and credential profiles used to
    /// decide which signed template bindings are manageable on this installation.
    /// `configuration` 提供精确已配置连接与凭据配置档，用于判断本机可管理哪些签名模板绑定。
    ///
    /// # Returns
    /// 返回值
    /// One deterministic connection/model declaration list from a single immutable generation.
    /// 返回来自单一不可变代际的确定性连接/模型声明列表。
    pub fn provider_tool_template_models(
        &self,
        configuration: &PersistedAiConfiguration,
    ) -> Result<Vec<DeclaredProviderToolTemplateModel>, String> {
        // generation keeps template provenance and concrete target contracts coherent for this call.
        // generation 让本次调用中的模板来源与具体目标契约保持一致。
        let generation = self.selection_catalog_snapshot()?;
        provider_tool_template_models_in_generation(configuration, generation.as_ref())
    }

    /// Applies one candidate configuration to immutable source catalogs in memory.
    /// 在内存中把一份候选配置应用到不可变源目录。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` supplies account access, manual membership, and capability differences.
    /// `configuration` 提供账号访问、手工成员关系与能力差异。
    ///
    /// # Returns
    /// 返回值
    /// Complete effective catalogs after every limit has passed strict validation.
    /// 返回全部限制通过严格校验后的完整有效目录。
    fn resolve_model_catalog_overlays(
        &self,
        configuration: &PersistedAiConfiguration,
    ) -> Result<RuntimeConnectionCatalogs, String> {
        // generation supplies every published input as one coherent immutable value.
        // generation 以一个一致不可变值提供全部已发布输入。
        let generation = self.selection_catalog_snapshot()?;
        let runtime_configuration = runtime_configuration_for_manifest_catalog(
            configuration,
            generation.manifests.as_ref(),
        );
        resolve_model_catalog_overlays_from_inputs(
            &runtime_configuration,
            generation.manifests.as_ref(),
            &generation.source_catalogs,
            &generation.usage_cache,
            CatalogProjectionUse::Execution,
        )
    }

    /// Resolves effective catalogs against one exact manifest catalog without publishing it.
    /// 针对一个精确清单目录解析有效模型目录，但不发布它。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` supplies account/user overlays and `manifests` freezes all system rules
    /// used by this complete resolution pass.
    /// `configuration` 提供账号/用户覆盖，`manifests` 冻结本次完整解析使用的全部系统规则。
    ///
    /// # Returns
    /// 返回值
    /// One complete effective catalog set derived from immutable sources and the supplied rules.
    /// 返回从不可变来源与传入规则派生的一份完整有效目录集合。
    pub(super) fn resolve_model_catalog_overlays_with_manifests(
        &self,
        configuration: &PersistedAiConfiguration,
        manifests: &crate::provider_manifest::ResolvedProviderCatalog,
    ) -> Result<RuntimeConnectionCatalogs, String> {
        // source_catalogs captures the staged discovery inputs while the activation lock is held.
        // source_catalogs 在持有激活锁期间捕获暂存发现输入。
        let source_catalogs = self
            .source_catalogs
            .read()
            .map_err(|_| String::from("The source AI model catalog lock is poisoned."))?
            .clone();
        // usage_cache captures plan observations serialized by the same catalog mutation lock.
        // usage_cache 捕获由同一目录变更锁串行化的套餐观测。
        let usage_cache = self.usage_cache_snapshot_locked()?;
        resolve_model_catalog_overlays_from_inputs(
            configuration,
            manifests,
            &source_catalogs,
            &usage_cache,
            CatalogProjectionUse::Execution,
        )
    }

    /// Determines whether one exact connection still exposes a model after applying candidate overlays.
    /// 判断一个精确连接在应用候选覆盖后是否仍暴露指定模型。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` is the complete candidate configuration, `connection_id` selects the exact
    /// credential catalog, and `model_id` is the upstream model identifier.
    /// `configuration` 是完整候选配置，`connection_id` 选择精确凭据目录，`model_id`
    /// 是上游模型标识。
    ///
    /// # Returns
    /// 返回值
    /// `true` when the immutable discovery snapshot or manual fallback still contains the model
    /// after all defaults and user differences resolve; otherwise `false`.
    /// 当不可变发现快照或手工回退目录在解析全部默认值与用户差异后仍包含该模型时返回
    /// `true`，否则返回 `false`。
    pub fn connection_catalog_contains_model(
        &self,
        configuration: &PersistedAiConfiguration,
        connection_id: &str,
        model_id: &str,
    ) -> Result<bool, String> {
        let connection = configuration
            .connection_graph
            .connections
            .get(connection_id)
            .ok_or_else(|| format!("Credential connection `{connection_id}` is not configured."))?;
        // Candidate validation deliberately sees staged discovery evidence, but captures source,
        // plan, and published Manifest rules under one mutation lock so activation cannot expose
        // an internally mixed image.
        // 候选校验刻意读取暂存发现证据，但会在同一变更锁下捕获来源、套餐与已发布 Manifest
        // 规则，因此激活不会暴露内部混合镜像。
        let (source_catalog, account_usage, generation) = {
            let _mutation_guard = self
                .catalog_mutation_lock
                .lock()
                .map_err(|_| String::from("The AI catalog mutation lock is poisoned."))?;
            let source_catalog = self
                .source_catalogs
                .read()
                .map_err(|_| String::from("The source AI model catalog lock is poisoned."))?
                .get(connection_id)
                .cloned();
            let account_usage = self
                .usage_cache
                .read()
                .map_err(|_| String::from("The AI account-usage cache lock is poisoned."))?
                .get(connection.id.as_str())
                .cloned();
            (
                source_catalog,
                account_usage,
                self.selection_catalog_snapshot()?,
            )
        };
        let Some(mut catalog) =
            source_catalog.or_else(|| manual_custom_catalog(configuration, connection, 1))
        else {
            return Ok(false);
        };
        apply_model_catalog_overlays_from_generation(
            configuration,
            connection.provider_id.as_str(),
            connection.service_entry_id.as_str(),
            account_usage.as_ref(),
            generation.as_ref(),
            &mut catalog,
        )?;
        retain_models_with_valid_limits(&mut catalog);
        validate_resolved_catalog_limits(&catalog)?;
        Ok(catalog.models.iter().any(|model| model.id == model_id))
    }

    /// Validates one proposed provider-model capability difference against every visible account.
    /// 针对每个可见账号校验一项候选供应商模型能力差异。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` is the persisted baseline, `provider_id` and `model_id` identify the exact
    /// model, and `overrides` is the replacement difference or `None` when clearing it.
    /// `configuration` 是持久化基线，`provider_id` 与 `model_id` 标识精确模型，`overrides`
    /// 是替换差异，清除差异时为 `None`。
    ///
    /// # Returns
    /// 返回值
    /// Success only when the fully resolved limits remain valid for every account that exposes
    /// the model from an immutable discovery snapshot or an explicit manual declaration.
    /// 仅当不可变发现快照或显式手工声明中暴露该模型的每个账号，其最终解析限制均有效时返回成功。
    pub fn validate_model_capability_override_candidate(
        &self,
        configuration: &PersistedAiConfiguration,
        provider_id: &str,
        model_id: &str,
        overrides: Option<&crate::ModelCapabilityOverrides>,
    ) -> Result<(), String> {
        if let Some(overrides) = overrides {
            overrides.validate_for_provider(provider_id)?;
        }
        let mut candidate_configuration = configuration.clone();
        // Provider-level capability settings must remain editable when every account hides the model.
        // 即使全部账号都隐藏该模型，供应商级能力设置仍必须可编辑。
        candidate_configuration
            .connection_model_access_policies
            .clear();
        let provider_model_key =
            PersistedAiConfiguration::provider_model_key(provider_id, model_id);
        if let Some(overrides) = overrides {
            candidate_configuration
                .model_capability_overrides
                .insert(provider_model_key, overrides.clone());
        } else {
            candidate_configuration
                .model_capability_overrides
                .remove(provider_model_key.as_str());
        }
        candidate_configuration.validate_context_window_policy_layers()?;

        // Candidate validation consumes staged discovery and usage as one immutable local image.
        // 候选校验把暂存发现与用量作为一份不可变本地镜像使用。
        let (source_catalogs, usage_cache, generation) = {
            let _mutation_guard = self
                .catalog_mutation_lock
                .lock()
                .map_err(|_| String::from("The AI catalog mutation lock is poisoned."))?;
            let source_catalogs = self
                .source_catalogs
                .read()
                .map_err(|_| String::from("The source AI model catalog lock is poisoned."))?
                .snapshot();
            let usage_cache = self
                .usage_cache
                .read()
                .map_err(|_| String::from("The AI account-usage cache lock is poisoned."))?
                .clone();
            (
                source_catalogs,
                usage_cache,
                self.selection_catalog_snapshot()?,
            )
        };
        let mut matched_connection = false;
        for connection in candidate_configuration
            .connection_graph
            .connections
            .values()
            .filter(|connection| connection.provider_id == provider_id)
        {
            let Some(mut catalog) = source_catalogs
                .get(connection.id.as_str())
                .cloned()
                .or_else(|| manual_custom_catalog(&candidate_configuration, connection, 1))
            else {
                continue;
            };
            let account_usage = usage_cache.get(connection.id.as_str());
            apply_model_catalog_overlays_from_generation(
                &candidate_configuration,
                provider_id,
                connection.service_entry_id.as_str(),
                account_usage,
                generation.as_ref(),
                &mut catalog,
            )?;
            let Some(model) = catalog.models.iter().find(|model| model.id == model_id) else {
                continue;
            };
            matched_connection = true;
            validate_resolved_model_capability_limits(
                connection.id.as_str(),
                model.id.as_str(),
                &model.capabilities,
                &model.capability_evidence,
            )?;
            if let Some(overrides) = overrides
                && let Some(projection) = overrides.request_projection.as_ref()
            {
                projection.validate_for_capabilities(&model.capabilities)?;
            }
        }
        if !matched_connection
            && let Some(mut catalog) = manual_custom_catalog_for_provider(
                &candidate_configuration,
                provider_id,
                "provider-settings-validation",
                1,
            )
        {
            apply_model_catalog_overlays_from_generation(
                &candidate_configuration,
                provider_id,
                "",
                None,
                generation.as_ref(),
                &mut catalog,
            )?;
            if let Some(model) = catalog.models.iter().find(|model| model.id == model_id) {
                matched_connection = true;
                validate_resolved_model_capability_limits(
                    "provider-settings-validation",
                    model.id.as_str(),
                    &model.capabilities,
                    &model.capability_evidence,
                )?;
                if let Some(overrides) = overrides
                    && let Some(projection) = overrides.request_projection.as_ref()
                {
                    projection.validate_for_capabilities(&model.capabilities)?;
                }
            }
        }
        if !matched_connection {
            return Err(format!(
                "Provider `{provider_id}` has no immutable or manual catalog source for model `{model_id}`. Synchronize the model catalog before saving capability overrides."
            ));
        }
        Ok(())
    }

    /// Validates one provider-level capability layer against every synchronized provider model.
    /// 针对供应商全部已同步模型校验一层供应商级能力配置。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` is the current persisted state, `provider_id` identifies the provider,
    /// and `overrides` is the candidate sparse layer or `None` for restoration.
    /// `configuration` 是当前持久化状态，`provider_id` 标识供应商，`overrides` 是候选稀疏层，
    /// 为 `None` 时表示恢复继承。
    ///
    /// # Returns
    /// 返回值
    /// Success only when every provider model resolves valid limits and request projections.
    /// 仅当供应商每个模型的限制与请求投影均解析有效时返回成功。
    pub fn validate_provider_capability_override_candidate(
        &self,
        configuration: &PersistedAiConfiguration,
        provider_id: &str,
        overrides: Option<&crate::ModelCapabilityOverrides>,
    ) -> Result<(), String> {
        if let Some(overrides) = overrides {
            overrides.validate_for_provider(provider_id)?;
        }
        let mut candidate_configuration = configuration.clone();
        candidate_configuration
            .connection_model_access_policies
            .clear();
        if let Some(overrides) = overrides {
            candidate_configuration
                .provider_capability_overrides
                .insert(provider_id.to_string(), overrides.clone());
        } else {
            candidate_configuration
                .provider_capability_overrides
                .remove(provider_id);
        }
        candidate_configuration.validate_context_window_policy_layers()?;
        let catalogs = self.resolved_model_catalog_snapshot(&candidate_configuration)?;
        let matched = candidate_configuration
            .connection_graph
            .connections
            .values()
            .filter(|connection| connection.provider_id == provider_id)
            .any(|connection| {
                catalogs
                    .get(connection.id.as_str())
                    .is_some_and(|catalog| !catalog.models.is_empty())
            });
        if !matched {
            return Err(format!(
                "Provider `{provider_id}` has no synchronized model catalog for provider-level capability settings."
            ));
        }
        Ok(())
    }

    /// Validates one protocol-specific model setting against every account support route.
    /// 针对每条账号支持路由校验一项协议专属模型设置。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` owns immutable membership; provider, model, and protocol identify the
    /// shared route; `overrides` contains the proposed exact adapter controls.
    /// `configuration` 持有不可变成员关系；供应商、模型与协议标识共享路由；`overrides`
    /// 包含候选精确适配器控制项。
    ///
    /// # Returns
    /// 返回值
    /// Success only when at least one support route exists and every matching model accepts the settings.
    /// 仅当至少存在一条支持路由且每个匹配模型都接受设置时返回成功。
    pub fn validate_model_execution_override_candidate(
        &self,
        configuration: &PersistedAiConfiguration,
        provider_id: &str,
        model_id: &str,
        protocol_id: &str,
        overrides: &crate::ModelExecutionOverrides,
    ) -> Result<(), String> {
        // support_configuration removes account visibility without weakening provider membership.
        // support_configuration 移除账号可见性，但不会削弱供应商成员关系。
        let mut support_configuration = configuration.clone();
        support_configuration
            .connection_model_access_policies
            .clear();
        let catalogs = self.resolved_model_catalog_snapshot(&support_configuration)?;
        let mut matched_connection = false;
        for connection in support_configuration
            .connection_graph
            .connections
            .values()
            .filter(|connection| connection.provider_id == provider_id)
        {
            let Some(model) = catalogs
                .get(connection.id.as_str())
                .and_then(|catalog| catalog.models.iter().find(|model| model.id == model_id))
            else {
                continue;
            };
            // effectiveProtocolId mirrors request selection and accepts only frozen model bindings.
            // effectiveProtocolId 镜像请求选择，并且只接受冻结的模型绑定。
            let effective_protocol_id = model
                .resolved_protocol_binding
                .as_ref()
                .map(|binding| binding.protocol_profile_id.as_str());
            if effective_protocol_id != Some(protocol_id) {
                continue;
            }
            matched_connection = true;
            validate_model_overrides(model, protocol_id, overrides)?;
        }
        if !matched_connection {
            return Err(format!(
                "Provider `{provider_id}` model `{model_id}` has no synchronized account with effective protocol `{protocol_id}`."
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;

/// One provider-tool template model declared for an eligible configured credential.
/// 为一个合格已配置凭据声明的供应商工具模板模型。
///
/// This management inventory intentionally ignores the model's enabled state so a newly synced
/// `default_enabled=false` model remains visible and can be enabled without a software update.
/// 此管理目录刻意忽略模型启用状态，使新同步且 `default_enabled=false` 的模型仍可见，并可在
/// 无需更新软件的情况下启用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeclaredProviderToolTemplateModel {
    /// Exact owning provider identifier.
    /// 精确所属供应商标识。
    pub provider_id: String,
    /// Exact configured credential connection identifier.
    /// 精确已配置凭据连接标识。
    pub connection_id: String,
    /// Exact regional or product service entry identifier.
    /// 精确区域或产品服务入口标识。
    pub service_entry_id: String,
    /// Exact upstream media or provider-tool model identifier.
    /// 精确上游媒体或供应商工具模型标识。
    pub model_id: String,
    /// User-visible model name supplied by the signed binding catalog.
    /// 签名绑定目录提供的用户可见模型名称。
    pub display_name: String,
    /// AI-visible signed capabilities supplied by this model's template targets.
    /// 此模型模板目标声明的 AI 可见能力。
    pub capabilities: std::collections::BTreeSet<ProviderToolCapabilityKind>,
}
