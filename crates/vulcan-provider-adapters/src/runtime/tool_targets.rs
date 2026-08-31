//! Published provider-tool inventories, exact target authorization, and frozen route contracts.
//! 已发布供应商工具清单、精确目标准入与冻结路由契约。

use super::activation::runtime_configuration_for_manifest_catalog;
use super::{PublicAiRuntime, ResolvedProviderGeneration};
use crate::{
    ConnectionModelCatalog, CredentialConnection, PersistedAiConfiguration, ProviderRegistry,
    ProviderToolCapabilityKind, ProviderToolTarget, ProviderToolTargetSelection,
};

/// One provider tool target paired with an eligible configured credential.
/// 与一个合格已配置凭据配对的供应商工具目标。
#[derive(Debug, Clone, PartialEq)]
pub struct AvailableProviderToolTarget {
    /// Exact credential connection identifier.
    /// 精确凭据连接标识。
    pub connection_id: String,
    /// User-visible credential name.
    /// 用户可见凭据名称。
    pub connection_display_name: String,
    /// Frozen source layer that owns the selected target contract.
    /// 持有所选目标契约的冻结来源层。
    pub manifest_source_layer: crate::provider_manifest::ManifestSourceLayer,
    /// Exact model- or service-specific target contract.
    /// 精确的模型或服务专属目标契约。
    pub target: ProviderToolTarget,
}

/// Frozen internal provider tool route resolved before secret loading.
/// 秘密加载前解析的冻结内部供应商工具路由。
pub(super) struct ResolvedProviderToolTarget {
    /// Exact configured credential.
    /// 精确已配置凭据。
    pub(super) connection: CredentialConnection,
    /// Exact regional or product entry.
    /// 精确区域或产品入口。
    pub(super) entry: crate::ServiceEntry,
    /// Exact model-native tool target.
    /// 精确模型原生工具目标。
    pub(super) target: ProviderToolTarget,
    /// Unique manifest operation owning pre-dispatch upload bindings, when declared.
    /// 拥有分派前上传绑定的唯一 Manifest 操作（若有声明）。
    pub(super) operation: Option<crate::OperationDeclaration>,
    /// Frozen request profiles referenced by the owning operation.
    /// 拥有当前目标的操作所引用的冻结请求 Profile。
    pub(super) request_profiles: Vec<crate::provider_manifest::RequestProjectionProfile>,
    /// Frozen response profiles referenced by the owning operation.
    /// 拥有当前目标的操作所引用的冻结响应 Profile。
    pub(super) response_profiles: Vec<crate::provider_manifest::ResponseProjectionProfile>,
    /// Immutable upload channels from the same manifest generation as the target.
    /// 与目标同一 Manifest 代际的不可变上传通道。
    pub(super) upload_channels: Vec<crate::UploadChannelDeclaration>,
    /// Exact manifest revision captured with the target and operation.
    /// 与目标和操作共同捕获的精确 Manifest 修订号。
    pub(super) manifest_revision: u64,
    /// Exact immutable manifest generation captured with the target.
    /// 与目标共同捕获的精确不可变 Manifest 代际。
    pub(super) manifest_generation: u64,
    /// Content- and target-addressed non-secret route identity.
    /// 内容与目标寻址的非秘密路由身份。
    pub(super) route_revision: String,
}

impl ResolvedProviderToolTarget {
    /// Formats the immutable, secret-free operation route used by provider-tool diagnostics.
    /// 格式化供应商工具诊断使用的不可变且不含秘密的操作路由。
    ///
    /// # Returns
    /// 返回值
    /// The frozen provider, target, operation, and profile identities without credential values.
    /// 返回不含凭据值的冻结供应商、目标、操作及 Profile 身份。
    pub(super) fn diagnostic_context(&self) -> String {
        let operation_id = self
            .operation
            .as_ref()
            .map(|operation| operation.operation_id.as_str())
            .unwrap_or("none");
        let request_profiles = self
            .request_profiles
            .iter()
            .map(|profile| format!("{}@{}", profile.profile_id, profile.revision))
            .collect::<Vec<_>>()
            .join(",");
        let response_profiles = self
            .response_profiles
            .iter()
            .map(|profile| format!("{}@{}", profile.profile_id, profile.revision))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "provider={}, target={}, upstream_target={}, operation={}, manifest_revision={}, request_profiles=[{}], response_profiles=[{}]",
            self.connection.provider_id,
            self.target.target_id,
            self.target.upstream_target_id,
            operation_id,
            self.manifest_revision,
            request_profiles,
            response_profiles
        )
    }
}

/// Reports whether one provider-tool target authorizes an exact credential profile.
/// 报告一个供应商工具目标是否授权精确凭据配置档。
///
/// # Parameters
/// 参数
/// `target` owns the manifest-declared scope and `credential_profile_id` is the connection's
/// exact profile. The schema defines an empty target scope as every profile.
/// `target` 持有 Manifest 声明的作用域，`credential_profile_id` 是连接的精确配置档；
/// Schema 把空目标作用域定义为全部配置档。
///
/// # Returns
/// 返回值
/// `true` when the scope is global or explicitly contains the profile.
/// 当作用域为全局或显式包含该配置档时返回 `true`。
pub(super) fn provider_tool_target_allows_credential(
    target: &ProviderToolTarget,
    credential_profile_id: &str,
) -> bool {
    target.credential_profile_ids.is_empty()
        || target
            .credential_profile_ids
            .contains(credential_profile_id)
}

/// Reports whether one target's optional model-catalog gate is satisfied by the same credential.
/// 判断一个目标的可选模型目录门控是否由同一凭据满足。
///
/// # Parameters
/// 参数
/// `target` declares optional model membership and `catalog` belongs to the same credential.
/// `target` 声明可选模型成员要求，`catalog` 属于同一凭据。
///
/// # Returns
/// 返回值
/// Whether no gate is declared or the supplied catalog contains an explicitly eligible model.
/// 未声明门控，或传入目录包含显式合格模型时返回真。
fn provider_tool_target_availability_catalog_matches(
    target: &ProviderToolTarget,
    catalog: Option<&ConnectionModelCatalog>,
) -> bool {
    if target.availability_model_ids_any.is_empty() {
        return true;
    }
    let Some(catalog) = catalog else {
        return false;
    };
    catalog.models.iter().any(|model| {
        target
            .availability_model_ids_any
            .contains(model.id.as_str())
    })
}

impl PublicAiRuntime {
    /// Returns every provider tool target usable by currently configured credentials.
    /// 返回当前已配置凭据能够使用的全部供应商工具目标。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` is the authoritative connection graph.
    /// `configuration` 是权威连接图。
    ///
    /// # Returns
    /// 返回值
    /// Exact targets paired with eligible credential connection identifiers.
    /// 返回与合格凭据连接标识配对的精确目标。
    pub fn available_provider_tool_targets(
        &self,
        configuration: &PersistedAiConfiguration,
    ) -> Result<Vec<AvailableProviderToolTarget>, String> {
        // generation freezes manifest targets and template-model access for the complete listing.
        // generation 为完整列表冻结 Manifest 目标与模板模型访问状态。
        let generation = self.selection_catalog_snapshot()?;
        let runtime_configuration = runtime_configuration_for_manifest_catalog(
            configuration,
            generation.manifests.as_ref(),
        );
        let configuration = &runtime_configuration;
        // system projects the exact provider registry owned by the same generation.
        // system 投影由同一代际持有的精确供应商注册表。
        let system = ProviderRegistry::from_catalog(generation.manifests.as_ref())?;
        // registry adds only the caller's explicit custom-provider compatibility projection.
        // registry 仅添加调用方的显式自定义供应商兼容投影。
        let registry =
            ProviderRegistry::effective_from_system(system, &configuration.custom_providers)?;
        Self::available_provider_tool_targets_in_generation(
            configuration,
            generation.as_ref(),
            &registry,
        )
    }

    /// Lists eligible provider-tool targets from one already captured provider generation.
    /// 从一份已捕获供应商代际列出合格供应商工具目标。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` supplies credentials and model access; `generation` freezes tool/template
    /// authority; `registry` is the provider projection built from that same generation.
    /// `configuration` 提供凭据与模型访问；`generation` 冻结工具/模板权限；`registry` 是从
    /// 同一代际构建的供应商投影。
    ///
    /// # Returns
    /// 返回值
    /// Exact AI-visible targets paired with eligible credential connections in deterministic order.
    /// 返回按确定性顺序与合格凭据连接配对的精确 AI 可见目标。
    pub(super) fn available_provider_tool_targets_in_generation(
        configuration: &PersistedAiConfiguration,
        generation: &ResolvedProviderGeneration,
        registry: &ProviderRegistry,
    ) -> Result<Vec<AvailableProviderToolTarget>, String> {
        // available accumulates only targets satisfying service, credential, and model access.
        // available 仅累积满足服务、凭据与模型访问条件的目标。
        let mut available = Vec::new();
        for connection in configuration.connection_graph.connections.values() {
            if configuration.connection_is_pending_validation(connection.id.as_str()) {
                continue;
            }
            if configuration.custom_provider_manifest_is_isolated(connection.provider_id.as_str()) {
                continue;
            }
            let provider = registry
                .provider(connection.provider_id.as_str())
                .ok_or_else(|| {
                    format!("Provider `{}` is not registered.", connection.provider_id)
                })?;
            let entry = provider
                .service_entries
                .iter()
                .find(|entry| entry.id == connection.service_entry_id)
                .ok_or_else(|| {
                    format!(
                        "Service entry `{}` is not registered for provider `{}`.",
                        connection.service_entry_id, provider.id
                    )
                })?;
            let targets = Self::resolved_provider_tool_targets_in_generation(
                generation,
                provider.id.as_str(),
            );
            if targets.is_empty() {
                continue;
            }
            // providerManifest supplies immutable provenance for every projected target.
            // providerManifest 为每个投影目标提供不可变溯源信息。
            let provider_manifest = generation
                .manifests
                .manifest(provider.id.as_str())
                .ok_or_else(|| {
                    format!(
                        "Provider tool manifest for `{}` disappeared from the frozen generation.",
                        provider.id
                    )
                })?;
            for target in targets {
                if target.contract.capability.is_ai_visible()
                    && target.service_entry_id == entry.id
                    && provider_tool_target_allows_credential(
                        &target,
                        connection.credential_profile_id.as_str(),
                    )
                    // Provider-tool availability follows the credential's account-filtered catalog,
                    // while extension target selection independently controls whether AI may call
                    // the model. Conversation publication must not hide media targets.
                    // Provider Tool 可用性遵循凭据经账号权限过滤后的目录，扩展目标选择则独立控制 AI
                    // 是否允许调用该模型；会话发布开关不得隐藏媒体目标。
                    && provider_tool_target_availability_catalog_matches(
                        &target,
                        generation
                            .activated_effective_catalogs
                            .get(connection.id.as_str()),
                    )
                {
                    available.push(AvailableProviderToolTarget {
                        connection_id: connection.id.clone(),
                        connection_display_name: connection.display_name.clone(),
                        manifest_source_layer: provider_manifest.snapshot_provenance.source_layer,
                        target,
                    });
                }
            }
        }
        available.sort_by(|left, right| {
            left.target
                .contract
                .capability
                .cmp(&right.target.contract.capability)
                .then_with(|| left.target.display_name.cmp(&right.target.display_name))
                .then_with(|| left.connection_id.cmp(&right.connection_id))
        });
        Ok(available)
    }

    /// Resolves provider tool targets from the currently published manifest catalog.
    /// 从当前已发布清单目录解析供应商工具目标。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` identifies the exact provider; compiled manifests are the target authority.
    /// `provider_id` 标识精确供应商；已编译清单是目标权威。
    ///
    /// # Returns
    /// 返回值
    /// The frozen target list, or an empty list when the snapshot/provider is unavailable.
    /// 返回冻结目标列表；快照或供应商不可用时返回空列表。
    pub(super) fn resolved_provider_tool_targets(
        &self,
        provider_id: &str,
    ) -> Vec<ProviderToolTarget> {
        self.selection_catalog_snapshot()
            .ok()
            .and_then(|generation| generation.manifests.manifest_arc(provider_id))
            .map(|manifest| manifest.tool_targets.clone())
            .unwrap_or_default()
    }

    /// Resolves provider-tool targets from one already frozen provider generation.
    /// 从一份已冻结供应商代际解析供应商工具目标。
    ///
    /// # Parameters
    /// 参数
    /// `generation` is the request-owned immutable snapshot and `provider_id` is exact.
    /// `generation` 是请求持有的不可变快照，`provider_id` 为精确标识。
    ///
    /// # Returns
    /// 返回值
    /// The manifest-owned targets in their declaration order, or an empty list when absent.
    /// 返回按声明顺序排列的 Manifest 目标；不存在时返回空列表。
    fn resolved_provider_tool_targets_in_generation(
        generation: &ResolvedProviderGeneration,
        provider_id: &str,
    ) -> Vec<ProviderToolTarget> {
        generation
            .manifests
            .manifest_arc(provider_id)
            .map(|manifest| manifest.tool_targets.clone())
            .unwrap_or_default()
    }

    /// Resolves one persisted single-target selection without fallback.
    /// 解析一份持久化单目标选择且不执行回退。
    ///
    /// # Parameters
    /// 参数
    /// `_configuration` is retained for compatibility; the captured generation owns the
    /// configuration while `capability` and `selection` identify the exact target.
    /// `_configuration` 为兼容而保留；已捕获代际持有配置，`capability` 与 `selection`
    /// 标识精确目标。
    ///
    /// # Returns
    /// 返回值
    /// Frozen connection, entry, and target snapshots.
    /// 返回冻结的连接、入口与目标快照。
    pub(super) fn resolve_provider_tool_target(
        &self,
        _configuration: &PersistedAiConfiguration,
        capability: ProviderToolCapabilityKind,
        selection: &ProviderToolTargetSelection,
    ) -> Result<ResolvedProviderToolTarget, String> {
        if !capability.is_ai_visible() {
            return Err(format!(
                "Capability `{capability:?}` is internal inference and cannot resolve as an AI-visible provider tool."
            ));
        }
        // generation freezes every provider-tool input for this resolution.
        // generation 为本次解析冻结每项供应商工具输入。
        let generation = self.selection_catalog_snapshot()?;
        // configuration belongs to the retained immutable generation.
        // configuration 属于保留的不变代际。
        let configuration = generation.configuration.as_ref();
        let system = ProviderRegistry::from_catalog(generation.manifests.as_ref())?;
        let registry =
            ProviderRegistry::effective_from_system(system, &configuration.custom_providers)?;
        let connection = configuration
            .connection_graph
            .connections
            .get(selection.connection_id.as_str())
            .cloned()
            .ok_or_else(|| {
                format!(
                    "Provider tool connection `{}` no longer exists.",
                    selection.connection_id
                )
            })?;
        if configuration.connection_is_pending_validation(connection.id.as_str()) {
            return Err(format!(
                "Provider tool connection `{}` is pending explicit model validation.",
                connection.id
            ));
        }
        let provider = registry
            .provider(connection.provider_id.as_str())
            .cloned()
            .ok_or_else(|| format!("Provider `{}` is not registered.", connection.provider_id))?;
        let entry = provider
            .service_entries
            .iter()
            .find(|entry| entry.id == connection.service_entry_id)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "Provider tool service entry `{}` no longer exists.",
                    connection.service_entry_id
                )
            })?;
        let manifest = generation
            .manifests
            .manifest_arc(provider.id.as_str())
            .ok_or_else(|| {
                format!(
                    "Provider tool manifest for `{}` is not registered.",
                    provider.id
                )
            })?;
        let target = manifest
            .tool_targets
            .iter()
            .cloned()
            .into_iter()
            .find(|target| target.target_id == selection.target_id)
            .ok_or_else(|| {
                format!(
                    "Provider tool target `{}` is not registered.",
                    selection.target_id
                )
            })?;
        // Provider-tool execution revalidates the same account-filtered credential membership used
        // by settings projection. Conversation publication is a separate policy boundary.
        // Provider Tool 执行重新校验设置投影使用的同一账号权限过滤后凭据成员关系；会话发布属于
        // 独立策略边界。
        if !provider_tool_target_availability_catalog_matches(
            &target,
            generation
                .activated_effective_catalogs
                .get(connection.id.as_str()),
        ) {
            return Err(format!(
                "Provider tool target `{}` is unavailable for the current credential model catalog.",
                target.target_id
            ));
        }
        let target = crate::connection_endpoint::apply_connection_provider_tool_origin(
            &connection,
            &entry,
            target,
        );
        if target.contract.capability != capability
            || target.service_entry_id != entry.id
            || !provider_tool_target_allows_credential(
                &target,
                connection.credential_profile_id.as_str(),
            )
        {
            return Err(format!(
                "Provider tool target `{}` is not eligible for connection `{}`.",
                target.target_id, connection.id
            ));
        }
        let mut matching_operations = manifest.operations.iter().filter(|operation| {
            operation
                .tool_target_refs
                .iter()
                .any(|reference| reference == &target.target_id)
        });
        let operation = matching_operations.next().cloned();
        if let Some(duplicate) = matching_operations.next() {
            return Err(format!(
                "Provider tool target `{}` is referenced by ambiguous operations including `{}`.",
                target.target_id, duplicate.operation_id
            ));
        }
        let request_profiles = manifest.request_profiles_for_operation(operation.as_ref())?;
        let response_profiles = manifest.response_profiles_for_operation(operation.as_ref())?;
        // manifestGeneration is frozen from the same atomic catalog generation as the target.
        // manifestGeneration 从与目标相同的原子目录代际中冻结。
        let manifest_generation = generation.manifests.generation;
        // routeRevision binds the catalog content, connection, and target without credential data.
        // routeRevision 在不包含凭据数据的前提下绑定目录内容、连接与目标。
        let route_revision = format!(
            "{}:{}:{}",
            generation.manifests.content_sha256, connection.id, target.target_id
        );
        Ok(ResolvedProviderToolTarget {
            connection,
            entry,
            target,
            operation,
            request_profiles,
            response_profiles,
            upload_channels: manifest.upload_channels.clone(),
            manifest_revision: manifest.revision,
            manifest_generation,
            route_revision,
        })
    }
}

#[cfg(test)]
mod tests;
