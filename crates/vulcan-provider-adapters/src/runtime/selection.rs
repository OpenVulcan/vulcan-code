//! Immutable model and account selection against one published provider generation.
//! 针对同一已发布供应商代际的不可变模型与账号选择。

use super::catalog_policy::model_supports_text_conversation;
use super::catalog_views::runtime_model_is_enabled_from_manifest;
use super::model_overrides::{
    apply_model_parameter_defaults, resolve_request_projection, validate_model_overrides,
};
use super::{PublicAiRuntime, ResolvedProviderGeneration};
use crate::{
    ActiveAiModelSelection, CredentialConnection, InferenceRouteBinding,
    InferenceRouteConfiguration, PersistedAiConfiguration, ProviderExecutionRoute,
    ProviderRegistry, RuntimeConnectionCatalogs, SecondaryRouteSelection,
    native_inference::validate_native_action_binding,
};
use std::sync::Arc;
use vulcan_ai_contract::{AiOperation, InferenceUsageScope};

mod profiles;
mod revision;

use profiles::{
    resolve_input_resource_constraints, resolve_request_projection_chain,
    resolve_response_projection_chain, resolve_usage_extraction_chain,
};
use revision::{effective_model_catalog_content_revision, provider_execution_route_revision};

/// Carries immutable public selection metadata without credential material.
/// 承载不含凭据材料的不可变公共选择元数据。
///
/// The selection is Clone and non-secret so a prepared turn can freeze the complete route,
/// model, overrides, and binding at the activation boundary (plan §6.3/§R4).
/// 选择可 Clone 且不含秘密，使已准备回合能在激活边界冻结完整路由、模型、覆盖与绑定
/// （方案 §6.3/§R4）。
#[derive(Debug, Clone)]
pub struct ResolvedPublicAiSelection {
    /// Exact provider execution route.
    /// 精确供应商执行路由。
    pub route: ProviderExecutionRoute,
    /// Exact credential-scoped model snapshot.
    /// 精确凭据作用域模型快照。
    pub model: crate::ConnectionModel,
    /// Local model execution overrides.
    /// 本地模型执行覆盖。
    pub overrides: crate::ModelExecutionOverrides,
    /// Complete media settings frozen independently of manifest and credential revisions.
    /// 独立于清单与凭据修订冻结的完整媒体设置。
    pub media_delivery_policy: crate::MediaDeliveryPolicy,
    /// Runtime catalog revision frozen with this selection.
    /// 与此选择一同冻结的运行时目录修订。
    pub catalog_revision: String,
    /// Provider-manifest catalog generation frozen with this selection.
    /// 与此选择一同冻结的供应商清单目录代数。
    pub manifest_generation: u64,
    /// Model-level protocol binding that produced this selection.
    /// 产生此选择的模型级协议绑定。
    pub protocol_binding: crate::provider_manifest::ModelProtocolBinding,
    /// Content-addressed revision of the exact LLM route and request projection snapshot.
    /// 精确 LLM 路由与请求投影快照的内容寻址修订号。
    pub route_revision: String,
}

/// Returns the exact active connection and rejects cross-provider selections.
/// 返回精确当前连接并拒绝跨供应商选择。
///
/// # Parameters
/// 参数
/// `configuration` owns the captured connection graph and `active` identifies its selected account.
/// `configuration` 持有已捕获连接图，`active` 标识其已选账号。
///
/// # Returns
/// 返回值
/// The exact owning connection, or an explicit missing-account or provider-mismatch error.
/// 返回精确所属连接，或明确的账号缺失／供应商不匹配错误。
fn exact_active_connection(
    configuration: &PersistedAiConfiguration,
    active: &ActiveAiModelSelection,
) -> Result<CredentialConnection, String> {
    let connection = configuration
        .connection_graph
        .connections
        .get(active.connection_id.as_str())
        .cloned()
        .ok_or_else(|| {
            format!(
                "Active credential connection `{}` is not configured.",
                active.connection_id
            )
        })?;
    if connection.provider_id != active.provider_id {
        return Err(format!(
            "Active connection `{}` does not belong to provider `{}`.",
            connection.id, active.provider_id
        ));
    }
    Ok(connection)
}

impl PublicAiRuntime {
    /// Resolves public model and route metadata without reading secret material.
    /// 在不读取秘密材料的情况下解析公共模型与路由元数据。
    ///
    /// # Parameters
    /// 参数
    /// `_configuration` is retained for API compatibility; the captured generation owns selection state.
    /// `_configuration` 为 API 兼容而保留；已捕获代际持有选择状态。
    ///
    /// # Returns
    /// 返回值
    /// The exact route, model, and local overrides needed during preparation.
    /// 返回准备阶段所需的精确路由、模型与本地覆盖。
    pub fn resolve_active_selection(
        &self,
        _configuration: &PersistedAiConfiguration,
    ) -> Result<ResolvedPublicAiSelection, String> {
        // generation is the sole request-boundary authority for active selection.
        // generation 是活动选择在请求边界的唯一权威。
        let generation = self.selection_catalog_snapshot()?;
        // configuration was atomically published with this generation's catalogs and routes.
        // configuration 与本代际目录和路由共同原子发布。
        let configuration = generation.configuration.as_ref();
        let active = configuration
            .active
            .as_ref()
            .ok_or_else(|| String::from("No active public AI model is configured."))?;
        let connection = exact_active_connection(configuration, active)?;
        self.resolve_selection_for_connection(
            configuration,
            connection,
            active.provider_id.as_str(),
            active.model_id.as_str(),
            None,
            Arc::clone(&generation),
        )
    }

    /// Resolves one request-scoped model through deterministic account preferences.
    /// 通过确定性的账号偏好解析一个请求级模型。
    ///
    /// # Parameters
    /// 参数
    ///
    /// `_configuration` is a compatibility input; the published generation owns connections and preferences.
    /// `_configuration` 是兼容输入；已发布代际持有连接与偏好。
    /// `provider_id` and `model_id` identify the requested public model.
    /// `provider_id` 与 `model_id` 标识请求的公共模型。
    ///
    /// # Returns
    /// 返回值
    ///
    /// A validated immutable selection without secret material.
    /// 返回不含凭据材料且已经校验的不可变选择。
    pub fn resolve_model_selection(
        &self,
        _configuration: &PersistedAiConfiguration,
        provider_id: &str,
        model_id: &str,
    ) -> Result<ResolvedPublicAiSelection, String> {
        // One published generation supplies both account eligibility and downstream route
        // resolution. No mutable runtime catalog is re-read after this point.
        // 一个已发布代际同时提供账号资格与下游路由解析；此后不再重新读取可变运行时目录。
        let generation = self.selection_catalog_snapshot()?;
        let configuration = generation.configuration.as_ref();
        let mut eligible_catalogs = generation.activated_effective_catalogs.snapshot();
        eligible_catalogs.retain(|connection_id, _| {
            configuration.connection_is_enabled(connection_id.as_str())
                && !configuration.connection_is_pending_validation(connection_id.as_str())
        });
        let eligible_catalogs = RuntimeConnectionCatalogs::from_snapshot(eligible_catalogs);
        let connection = configuration
            .connection_graph
            .resolve_connection(provider_id, model_id, &eligible_catalogs)?
            .clone();
        self.resolve_selection_for_connection(
            configuration,
            connection,
            provider_id,
            model_id,
            None,
            Arc::clone(&generation),
        )
    }

    /// Resolves one exact credential connection and model selected by a persisted session.
    /// 解析持久化会话选择的精确凭据连接与模型。
    ///
    /// # Parameters
    /// 参数
    /// `_configuration` is retained for compatibility; the published generation owns connection metadata.
    /// `_configuration` 为兼容而保留；已发布代际持有连接元数据。
    /// `connection_id`, `provider_id`, and `model_id` identify the exact session selection.
    /// `connection_id`、`provider_id` 与 `model_id` 标识精确会话选择。
    ///
    /// # Returns
    /// 返回值
    /// A validated immutable selection, or an error that the caller must surface without rerouting.
    /// 返回已校验的不可变选择；错误必须由调用方直接呈现且不得改道。
    pub fn resolve_connection_model_selection(
        &self,
        _configuration: &PersistedAiConfiguration,
        connection_id: &str,
        provider_id: &str,
        model_id: &str,
    ) -> Result<ResolvedPublicAiSelection, String> {
        // generation freezes the exact configuration and catalog accepted by this session lookup.
        // generation 冻结本次会话查找接受的精确配置与目录。
        let generation = self.selection_catalog_snapshot()?;
        // configuration cannot drift independently from the retained generation.
        // configuration 无法独立于所保留代际发生漂移。
        let configuration = generation.configuration.as_ref();
        // connection is the exact configured credential requested by the session.
        // connection 是会话请求的精确已配置凭据。
        let connection = configuration
            .connection_graph
            .connections
            .get(connection_id)
            .cloned()
            .ok_or_else(|| format!("Connection `{connection_id}` is not configured."))?;
        self.resolve_selection_for_connection(
            configuration,
            connection,
            provider_id,
            model_id,
            None,
            Arc::clone(&generation),
        )
    }

    /// Resolves one exact configured internal inference route without applying workbench access.
    /// 解析一条精确已配置内部推理路由且不应用工作台访问状态。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` owns the internal enable switch and `inference_route` freezes provider,
    /// product, credential, model, action, and usage scope.
    /// `configuration` 拥有内部启用开关，`inference_route` 冻结供应商、产品、凭据、
    /// 模型、动作与用途范围。
    ///
    /// # Returns
    /// 返回值
    /// An immutable selection whose operation and account usage scope are explicitly authorized.
    /// 返回操作与账号用途范围均已显式授权的不可变选择。
    pub fn resolve_internal_inference_selection(
        &self,
        configuration: &PersistedAiConfiguration,
        inference_route: &InferenceRouteConfiguration,
    ) -> Result<ResolvedPublicAiSelection, String> {
        // generation freezes every internal-inference authorization input.
        // generation 冻结每项内部推理授权输入。
        let generation = self.selection_catalog_snapshot()?;
        // The caller-owned configuration permits validation of one exact pending settings
        // transaction, while model membership and protocol routes remain frozen by generation.
        // 调用方持有的配置允许校验一个精确的待提交设置事务；模型成员关系与协议路由仍由代际冻结。
        let connection = configuration
            .connection_graph
            .connections
            .get(inference_route.connection_id.as_str())
            .cloned()
            .ok_or_else(|| {
                format!(
                    "Internal inference connection `{}` is not configured.",
                    inference_route.connection_id
                )
            })?;
        if connection.provider_id != inference_route.provider_id
            || connection.service_entry_id != inference_route.service_entry_id
        {
            return Err(format!(
                "Internal inference route `{}/{}` does not own connection `{}`.",
                inference_route.provider_id,
                inference_route.service_entry_id,
                inference_route.connection_id
            ));
        }
        let selection = self.resolve_selection_for_connection(
            configuration,
            connection,
            inference_route.provider_id.as_str(),
            inference_route.model_id.as_str(),
            Some(inference_route.operation),
            Arc::clone(&generation),
        )?;
        if inference_route.usage_scope == InferenceUsageScope::InteractiveAgent {
            return Err(format!(
                "Internal inference connection `{}` cannot use the interactive-agent scope.",
                inference_route.connection_id
            ));
        }
        match (&inference_route.binding, inference_route.operation) {
            (
                InferenceRouteBinding::Protocol {
                    protocol_profile_id,
                },
                AiOperation::ConversationRespond,
            ) if protocol_profile_id == &selection.route.protocol_route.protocol_profile_id => {}
            (
                InferenceRouteBinding::Action { action_binding_id },
                AiOperation::EmbeddingCreate | AiOperation::RerankDocuments,
            ) => validate_native_action_binding(
                &selection.route,
                inference_route.operation,
                inference_route.model_id.as_str(),
                action_binding_id.as_str(),
            )
            .map_err(|failure| failure.error.message.clone())?,
            _ => {
                return Err(format!(
                    "Internal inference binding does not match operation `{}`.",
                    inference_route.operation.as_str()
                ));
            }
        }
        Ok(selection)
    }

    /// Resolves one exact connection and model into an immutable public selection.
    /// 将一个精确连接与模型解析为不可变公共选择。
    ///
    /// # Parameters
    /// 参数
    ///
    /// `configuration` supplies local settings, `connection` is exact, and the provider/model
    /// pair is the validated request identity.
    /// `configuration` 提供本地设置，`connection` 为精确连接，供应商/模型组合为已校验请求身份。
    ///
    /// # Returns
    /// 返回值
    ///
    /// A route, model snapshot, and validated local overrides.
    /// 返回路由、模型快照与已校验本地覆盖。
    fn resolve_selection_for_connection(
        &self,
        configuration: &PersistedAiConfiguration,
        connection: CredentialConnection,
        provider_id: &str,
        model_id: &str,
        required_internal_operation: Option<AiOperation>,
        catalog_snapshot: Arc<ResolvedProviderGeneration>,
    ) -> Result<ResolvedPublicAiSelection, String> {
        if !configuration.connection_is_enabled(connection.id.as_str()) {
            return Err(format!(
                "Connection `{}` is disabled and cannot execute requests.",
                connection.id
            ));
        }
        if configuration.connection_is_pending_validation(connection.id.as_str()) {
            return Err(format!(
                "Connection `{}` is pending explicit model validation and cannot execute normal requests.",
                connection.id
            ));
        }
        // The caller captured this immutable Arc before account selection. Hot updates may
        // publish a newer generation concurrently, but no decision below re-reads mutable
        // manifest, source, or model catalogs (plan F3).
        // 调用方在账号选择前已捕获该不可变 Arc。热更新可并发发布新代际，但下方任何决策
        // 都不会重新读取可变 Manifest、来源目录或模型目录（方案 F3）。
        if required_internal_operation.is_none()
            && !runtime_model_is_enabled_from_manifest(
                configuration,
                provider_id,
                model_id,
                catalog_snapshot.manifests.manifest(provider_id),
            )
        {
            return Err(format!(
                "Model `{model_id}` is disabled for provider `{provider_id}`."
            ));
        }
        let (model, catalog_revision) = {
            // Selections read only the effective catalog frozen at the last explicit activation
            // boundary; unactivated configuration mutations and mutable catalogs stay invisible.
            // 选择只读取最近一次显式激活边界冻结的有效目录；未激活配置变更与可变目录保持不可见。
            let activated = catalog_snapshot
                .activated_effective_catalogs
                .get(connection.id.as_str())
                .cloned();
            let source_has_connection = catalog_snapshot
                .source_catalogs
                .get(connection.id.as_str())
                .is_some();
            let catalog = match activated {
                Some(catalog) => catalog,
                None if source_has_connection => {
                    return Err(format!(
                        "Connection `{}` has no effective catalog after applying persisted model access and capability overlays.",
                        connection.id
                    ));
                }
                None => {
                    return Err(format!(
                        "No activated catalog exists for connection `{}`.",
                        connection.id
                    ));
                }
            };
            let model = catalog
                .models
                .iter()
                .find(|model| model.id == model_id)
                .cloned()
                .ok_or_else(|| {
                    format!(
                        "Model `{}` is not present in the effective catalog for connection `{}`.",
                        model_id, connection.id
                    )
                })?;
            let catalog_revision = effective_model_catalog_content_revision(&catalog)?;
            (model, catalog_revision)
        };
        let provider = ProviderRegistry::effective_from_system(
            ProviderRegistry::from_catalog(catalog_snapshot.manifests.as_ref())?,
            &configuration.custom_providers,
        )?
        .provider(provider_id)
        .cloned()
        .ok_or_else(|| format!("Provider `{provider_id}` is not registered."))?;
        let entry = provider
            .service_entries
            .iter()
            .find(|entry| entry.id == connection.service_entry_id)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "Service entry `{}` is not registered for provider `{}`.",
                    connection.service_entry_id, provider.id
                )
            })?;
        if required_internal_operation.is_some() {
            let default_internal_enabled = provider.default_internal_use_for_connections(
                configuration
                    .connection_graph
                    .connections
                    .values()
                    .filter(|candidate| {
                        candidate.provider_id == provider_id
                            && configuration.connection_is_enabled(candidate.id.as_str())
                            && !configuration
                                .connection_is_pending_validation(candidate.id.as_str())
                    }),
            )?;
            if !configuration.internal_model_is_enabled(
                provider_id,
                model_id,
                default_internal_enabled,
            ) {
                return Err(format!(
                    "Model `{model_id}` is disabled for non-conversation internal use on provider `{provider_id}`."
                ));
            }
        }
        // Resolve the model-level protocol binding first; the same effective protocol then
        // drives capability projection, overrides, prompt cache, parameters, the execution
        // route, internal-inference checks, and route revision (plan §13.4).
        // 先解析模型级协议绑定；同一生效协议随后驱动能力投影、覆盖、提示词缓存、参数、
        // 执行路由、内部推理校验与路由修订（方案 §13.4）。
        // activeManifest is the only system routing authority for this frozen generation.
        // activeManifest 是当前冻结代际的唯一系统路由权威。
        let active_manifest = catalog_snapshot.manifests.manifest_arc(provider_id);
        let protocol_binding = match provider.kind {
            crate::ProviderKind::System => match &active_manifest {
                Some(manifest) => {
                    // The activation transaction freezes the system binding together with its
                    // model catalog. Request execution reads that immutable fact and verifies its
                    // manifest generation instead of traversing rules again.
                    // 激活事务把系统绑定与模型目录一同冻结。请求执行读取该不可变事实并校验其
                    // Manifest 代际，而不是再次遍历规则。
                    let binding = model.resolved_protocol_binding.clone().ok_or_else(|| {
                        format!(
                            "Model `{model_id}` is not executable: no system protocol binding was frozen at the active catalog boundary."
                        )
                    })?;
                    let source_is_current = match binding.source {
                        crate::provider_manifest::ModelProtocolBindingSource::SystemManifest => true,
                        crate::provider_manifest::ModelProtocolBindingSource::SystemUser => {
                            configuration
                                .custom_model(provider_id, model_id)
                                .and_then(|definition| definition.system_binding.as_ref())
                                .is_some_and(|manual| {
                                    manual.service_entry_id == connection.service_entry_id
                                        && manual.protocol_profile_id == binding.protocol_profile_id
                                })
                        }
                        _ => false,
                    };
                    if !source_is_current || binding.manifest_revision != manifest.revision {
                        return Err(format!(
                            "Model `{model_id}` frozen binding does not belong to active manifest revision {}.",
                            manifest.revision
                        ));
                    }
                    binding
                }
                None => {
                    return Err(format!(
                        "Provider `{provider_id}` has no active system manifest; model `{model_id}` execution is blocked."
                    ));
                }
            },
            crate::ProviderKind::Custom => {
                // Custom bindings freeze at the catalog activation boundary; execution reads
                // the frozen binding only. A missing binding is a blocked catalog fact and must
                // never trigger request-time Auto recomputation (plan §13.3/§13.6).
                // 自定义绑定在目录激活边界冻结；执行只读取冻结绑定。缺失绑定属于被阻止的目录事实，
                // 绝不能触发请求时 Auto 重算（方案 §13.3/§13.6）。
                let binding = model.resolved_protocol_binding.clone().ok_or_else(|| {
                    format!(
                        "Custom model `{model_id}` has no protocol binding frozen at the active catalog boundary; save a valid model protocol preference and reactivate the provider catalog."
                    )
                })?;
                // A standalone custom Manifest owns the binding revision exactly.
                // 独立自定义 Manifest 精确持有绑定修订。
                // sourceIsCustom rejects system authority from the custom execution branch.
                // sourceIsCustom 拒绝系统权威进入自定义执行分支。
                let source_is_custom = matches!(
                    binding.source,
                    crate::provider_manifest::ModelProtocolBindingSource::CustomUser
                        | crate::provider_manifest::ModelProtocolBindingSource::CustomAutoDefault
                );
                // revisionIsCurrent proves the binding belongs to the same activated contract.
                // revisionIsCurrent 证明绑定属于同一份已激活契约。
                let revision_is_current = active_manifest
                    .as_ref()
                    .is_some_and(|manifest| binding.manifest_revision == manifest.revision);
                if !source_is_custom || !revision_is_current {
                    return Err(format!(
                        "Custom model `{model_id}` frozen binding does not belong to the active custom manifest revision."
                    ));
                }
                binding
            }
        };
        // Project the effective binding's per-protocol variant onto the effective capability
        // fields; the top-level projection is never an authority for guessing the protocol.
        // 将生效绑定的按协议变体投影到有效能力字段；顶层投影绝不用于猜测协议。
        let mut model = model;
        if let Some(variant) = model
            .protocol_variants
            .get(&protocol_binding.protocol_profile_id)
        {
            model.capabilities = variant.capabilities.clone();
            model.capability_evidence = variant.capability_evidence.clone();
        } else {
            return Err(format!(
                "Model `{model_id}` has no capability variant for its bound protocol `{}`; execution is blocked.",
                protocol_binding.protocol_profile_id
            ));
        }
        // The protocol variant is the per-route capability authority, but persisted user
        // capability overrides are the highest layer and must win over the projection
        // (plan §R5.3); the overlay rebuild folds them into the catalog, so re-applying
        // them here preserves the exact persisted overrides after the variant swap.
        // 协议变体是按路由能力权威，但持久化用户能力覆盖是最高层，必须胜过投影
        // （方案 §R5.3）；覆盖重建已把覆盖并入目录，因此在此重新应用以在变体交换后
        // 保留精确持久化覆盖。
        if provider.kind == crate::ProviderKind::System {
            configuration.apply_model_capability_overrides(
                provider_id,
                model.id.as_str(),
                &mut model.capabilities,
            );
        }
        // Capability admission must consume the effective binding projection. Checking the raw
        // catalog compatibility fields before binding resolution can authorize or reject the
        // model with evidence from a different protocol (plan §13.1/§13.4).
        // 能力准入必须消费生效绑定投影。在绑定解析前检查原始目录兼容字段，可能会使用另一协议的
        // 证据错误放行或拒绝模型（方案 §13.1/§13.4）。
        if let Some(operation) = required_internal_operation
            && !model.capabilities.operations.contains(&operation)
        {
            return Err(format!(
                "Model `{model_id}` does not expose internal operation `{}`.",
                operation.as_str()
            ));
        }
        if required_internal_operation.is_none()
            && !model_supports_text_conversation(&model.capabilities)
        {
            return Err(format!(
                "Model `{model_id}` does not expose bidirectional text conversation."
            ));
        }
        // Every executable model owns one active model-level protocol binding.
        // 每个可执行模型都持有一项活动的模型级协议绑定。
        let effective_protocol_id = protocol_binding.protocol_profile_id.as_str();
        let override_key = PersistedAiConfiguration::provider_model_protocol_key(
            connection.provider_id.as_str(),
            model.id.as_str(),
            effective_protocol_id,
        );
        let mut overrides = configuration
            .model_overrides
            .get(override_key.as_str())
            .cloned()
            .unwrap_or_default();
        if overrides.prompt_cache.is_none() {
            overrides.prompt_cache = connection
                .prompt_cache_by_protocol
                .get(effective_protocol_id)
                .cloned();
        }
        apply_model_parameter_defaults(&model, effective_protocol_id, &mut overrides)?;
        validate_model_overrides(&model, effective_protocol_id, &overrides)?;
        let secondary_route_key = PersistedAiConfiguration::connection_model_key(
            connection.id.as_str(),
            model.id.as_str(),
        );
        let secondary_route = configuration
            .secondary_routes
            .get(secondary_route_key.as_str())
            .cloned()
            .unwrap_or(SecondaryRouteSelection::Automatic);
        let mut route = ProviderExecutionRoute::resolve(
            provider,
            entry.clone(),
            connection.clone(),
            secondary_route,
            effective_protocol_id,
        )
        .map_err(|failure| failure.error.message.clone())?;
        // Custom and native models share one merged projection chain instead of replacing an inherited layer.
        // 自定义模型与系统模型共用一条合并投影链，而不是替换掉已继承的上级层。
        route.custom_model_projection =
            resolve_request_projection(configuration, provider_id, model_id)?;
        // Codex behavior is copied from the exact frozen binding and never inferred from a model name.
        // Codex 行为从精确冻结绑定复制，绝不根据模型名称推断。
        route.codex_behavior = protocol_binding.codex_behavior.clone();
        if let Some(required_primitive) = route
            .codex_behavior
            .as_ref()
            .and_then(crate::provider_manifest::CodexModelBehavior::missing_runtime_primitive)
        {
            return Err(format!(
                "Model '{model_id}' requires runtime primitive '{required_primitive}', which is not registered by this build."
            ));
        }
        // toolSchemaProjection is resolved only from the active system Manifest or the exact
        // custom-model advanced setting. Protocol and model-name heuristics are intentionally absent.
        // toolSchemaProjection 只从活动系统 Manifest 或精确自定义模型高级设置解析。
        // 此处刻意不存在协议或模型名猜测。
        route.tool_schema_projection = match route.provider.kind {
            crate::ProviderKind::System => {
                let manifest = active_manifest.as_ref().ok_or_else(|| {
                    format!(
                        "Provider `{provider_id}` has no active system manifest for tool-schema projection."
                    )
                })?;
                manifest
                    .rule_for_model(
                        entry.id.as_str(),
                        Some(connection.credential_profile_id.as_str()),
                        model_id,
                    )
                    .map(|rule| rule.tool_schema_projection)
                    .unwrap_or(
                        manifest
                            .model_defaults_for_entry(entry.id.as_str())?
                            .tool_schema_projection,
                    )
            }
            crate::ProviderKind::Custom => configuration
                .custom_model(provider_id, model_id)
                .map(|definition| definition.tool_schema_projection)
                .unwrap_or_default(),
        };
        // A matching model rule's inherited profile refs replace the route default chain for
        // this exact model; every other model keeps the route default (plan §R8.4/§R8.5).
        // 命中模型规则的继承 Profile 引用替换该精确模型的路由默认链；其他模型保持
        // 路由默认（方案 §R8.4/§R8.5）。
        if let Some(manifest) = active_manifest.as_ref() {
            route.upload_channels = manifest.upload_channels.clone();
            route.protocol_route.input_resource_constraints = resolve_input_resource_constraints(
                manifest.as_ref(),
                entry.id.as_str(),
                Some(connection.credential_profile_id.as_str()),
                model_id,
                route.protocol_route.input_resource_constraints.clone(),
            );
            // The exact selected profile is required when an explicit empty model override
            // restores the standard protocol usage baseline.
            // 当显式空模型覆盖恢复标准协议 usage 基线时，需要精确的已选 Profile。
            let usage_protocol_profile_id = route.protocol_route.protocol_profile_id.clone();
            route.protocol_route.usage_extraction_profiles = resolve_usage_extraction_chain(
                manifest.as_ref(),
                entry.id.as_str(),
                Some(connection.credential_profile_id.as_str()),
                model_id,
                usage_protocol_profile_id.as_str(),
                route.protocol_route.usage_extraction_profiles.clone(),
            )?;
            route.request_projection_profiles = resolve_request_projection_chain(
                manifest.as_ref(),
                entry.id.as_str(),
                Some(connection.credential_profile_id.as_str()),
                model_id,
                route.protocol_route.request_projection_profiles.clone(),
            )?;
            route.response_projection_profiles = resolve_response_projection_chain(
                manifest.as_ref(),
                entry.id.as_str(),
                Some(connection.credential_profile_id.as_str()),
                model_id,
                route.protocol_route.response_projection_profiles.clone(),
            )?;
        }
        let route_revision = provider_execution_route_revision(&route, &model, &overrides)?;
        // The execution route carries the same immutable identity used by media-cache isolation.
        // 执行路由携带与媒体缓存隔离相同的不可变身份。
        route.manifest_revision = protocol_binding.manifest_revision;
        route.manifest_generation = catalog_snapshot.manifests.generation;
        route.route_revision = route_revision.clone();
        // Validate the captured configuration before exposing a frozen selection.
        // 公开冻结选择前校验已捕获配置。
        let media_delivery_policy = configuration.effective_media_delivery_policy();
        media_delivery_policy.validate()?;
        Ok(ResolvedPublicAiSelection {
            media_delivery_policy,
            route,
            model,
            overrides,
            catalog_revision,
            manifest_generation: catalog_snapshot.manifests.generation,
            route_revision,
            protocol_binding,
        })
    }
}

#[cfg(test)]
mod tests;
