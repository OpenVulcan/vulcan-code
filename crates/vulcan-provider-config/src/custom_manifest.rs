//! Standalone custom provider manifest migration and hydration.
//! 独立自定义供应商清单的迁移与补水（hydration）。
//!
//! Every user-defined `custom:` provider owns exactly one JSON file under the store's
//! `provider-manifests/custom` directory (plan §R6.1/§R6.2). The centralized
//! `PersistedAiConfiguration.custom_providers` map is the in-memory authority; persisted
//! bytes live in the standalone files while the centralized document keeps only the
//! reference set `custom_provider_file_refs`. Migration is idempotent, resumable, and
//! never leaves a half-migrated configuration behind: a failed write keeps the old
//! centralized map untouched and the next attempt resumes where it stopped.
//! 每个用户自定义 `custom:` 供应商在存储的 `provider-manifests/custom` 目录下独占一份
//! JSON 文件（方案 §R6.1/§R6.2）。集中式 `PersistedAiConfiguration.custom_providers`
//! 映射是内存权威；持久化字节存放在独立文件中，而集中文档只保留引用集
//! `custom_provider_file_refs`。迁移具备幂等性、可恢复性，绝不留下半迁移配置：
//! 写入失败会原样保留旧集中式映射，下次尝试从断点继续。

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

use serde::{Deserialize, Serialize};
use vulcan_provider_adapters::{
    ContextWindowPolicyOverrides, CustomModelDefinition, CustomModelProtocolPreference,
    CustomModelTemplateDecision, CustomProviderManifestFailure, CustomProviderModelDefaults,
    InternalModelOverrides, MAX_STRING_LEN, ManifestOwnership, ModelCapabilityOverrides,
    ModelExecutionOverrides, PersistedAiConfiguration, ProviderDescriptor, ProviderManifestV1,
    validate_custom_provider_identifier,
};

use crate::manifest_store::ManifestStoreLayout;

/// Maximum serialized bytes accepted for one custom-provider transaction journal.
/// 单份自定义供应商事务日志接受的最大序列化字节数。
const CUSTOM_PROVIDER_TRANSACTION_JOURNAL_MAX_BYTES: u64 = 64 * 1024 * 1024;

/// Maximum serialized bytes retained for the durable pre-migration backup.
/// 持久迁移前备份允许保留的最大序列化字节数。
const CUSTOM_PROVIDER_MIGRATION_BACKUP_MAX_BYTES: u64 = 64 * 1024 * 1024;

/// Maximum serialized bytes accepted for the migration-completion marker.
/// 迁移完成标记允许接受的最大序列化字节数。
const CUSTOM_PROVIDER_MIGRATION_MARKER_MAX_BYTES: u64 = 4 * 1024;

/// Maximum isolated custom-provider diagnostics exposed through one runtime snapshot.
/// 单份运行时快照允许暴露的最大隔离自定义供应商诊断数。
const CUSTOM_PROVIDER_HYDRATION_FAILURE_MAX_COUNT: usize = 256;

/// Maximum displayed characters for one isolated custom-provider identity.
/// 单个隔离自定义供应商身份允许展示的最大字符数。
const CUSTOM_PROVIDER_HYDRATION_FAILURE_ID_MAX_CHARACTERS: usize = MAX_STRING_LEN;

/// Maximum displayed characters for one isolated custom-provider repair diagnostic.
/// 单条隔离自定义供应商修复诊断允许展示的最大字符数。
const CUSTOM_PROVIDER_HYDRATION_FAILURE_MESSAGE_MAX_CHARACTERS: usize = 2_048;

/// Fixed schema identifier for one complete standalone custom-provider document.
/// 单份完整独立自定义供应商文档的固定 Schema 标识。
pub const CUSTOM_PROVIDER_MANIFEST_SCHEMA_VERSION: &str = "vulcan.custom-provider-manifest.v1";

/// Durable before/target images for one custom-provider and central-configuration transaction.
/// 一次自定义供应商与中央配置事务的持久 before/target 镜像。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CustomProviderTransactionJournal {
    /// Journal schema version.
    /// 日志 Schema 版本。
    schema_version: u32,
    /// Central configuration text before the transaction, or absence.
    /// 事务前的中央配置文本，或不存在状态。
    before_configuration: Option<String>,
    /// Exact central configuration text to commit.
    /// 待提交的精确中央配置文本。
    target_configuration: String,
    /// Complete standalone custom manifest set before the transaction.
    /// 事务前的完整独立自定义清单集合。
    before_manifests: BTreeMap<String, String>,
    /// Complete standalone custom manifest set to commit.
    /// 待提交的完整独立自定义清单集合。
    target_manifests: BTreeMap<String, String>,
}

/// Immutable first before-image retained across successful custom-provider migration.
/// 跨成功自定义供应商迁移永久保留的首次前镜像。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CustomProviderMigrationBackup {
    /// Exact backup schema version.
    /// 精确备份 Schema 版本。
    schema_version: u32,
    /// Central configuration bytes before the first migration attempt, or absence.
    /// 首次迁移尝试前的中央配置字节，或不存在状态。
    central_configuration: Option<String>,
    /// Complete standalone custom-manifest set before the first migration attempt.
    /// 首次迁移尝试前的完整独立自定义 Manifest 集合。
    custom_manifests: BTreeMap<String, String>,
}

/// Durable proof that the backed-up migration reached a validated current image.
/// 证明已备份迁移到达已验证当前镜像的持久记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CustomProviderMigrationMarker {
    /// Exact marker schema version.
    /// 精确标记 Schema 版本。
    schema_version: u32,
    /// Lowercase SHA-256 of the immutable backup bytes completed by this migration.
    /// 本次迁移完成所对应不可变备份字节的小写 SHA-256。
    backup_sha256: String,
}

/// Reports legacy standalone descriptors discovered while hydrating custom providers.
/// 报告补水自定义供应商时发现的旧版独立描述符。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CustomProviderHydrationReport {
    /// Provider identifiers whose files must be transactionally rewritten as complete documents.
    /// 文件必须通过事务重写为完整文档的供应商标识。
    pub legacy_descriptor_provider_ids: Vec<String>,
    /// Bounded failures for providers isolated while every healthy provider remains available.
    /// 当所有健康供应商仍可用时，被隔离供应商的有界失败记录。
    pub failures: Vec<CustomProviderManifestFailure>,
    /// Total isolated provider count before the diagnostic list bound was applied.
    /// 应用诊断列表上限前的隔离供应商总数。
    pub failure_total_count: u64,
    /// Whether the bounded failure list omits one or more additional providers.
    /// 有界失败列表是否省略了一个或多个额外供应商。
    pub failures_truncated: bool,
}

/// One validated standalone document together with its exact on-disk schema provenance.
/// 一份已校验的独立文档及其精确磁盘 Schema 来源。
struct ParsedCustomProviderDocument {
    /// Complete current document consumed by runtime hydration.
    /// 运行时补水消费的完整当前文档。
    document: CustomProviderManifestDocumentV1,
    /// Whether the source bytes used the explicit descriptor-only version-zero schema.
    /// 源字节是否使用明确的仅描述符零版本 Schema。
    legacy_descriptor_v0: bool,
}

/// User-owned provider/model state that belongs beside one custom provider manifest.
/// 应与一份自定义供应商 Manifest 同文件保存的用户自有供应商/模型状态。
///
/// Connection-, credential-, session-, and inference-profile state deliberately remains in the
/// central configuration. Composite keys are retained exactly so existing collision-resistant
/// persistence semantics and migrations remain authoritative.
/// 连接、凭据、会话和推理配置档状态有意继续保留在中央配置中。组合键按原样保留，确保
/// 现有抗冲突持久化语义和迁移继续具有权威性。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomProviderLocalOverridesV1 {
    /// Manual model definitions keyed by exact upstream model identifier.
    /// 按精确上游模型标识索引的手工模型定义。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub custom_models: BTreeMap<String, CustomModelDefinition>,
    /// Optional provider-wide model defaults.
    /// 可选的供应商级模型默认值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_model_defaults: Option<CustomProviderModelDefaults>,
    /// User-reviewed template decisions with their existing provider/model keys.
    /// 使用现有供应商/模型键的用户审查模板决策。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub model_template_decisions: BTreeMap<String, CustomModelTemplateDecision>,
    /// Explicit per-model protocol preferences with their existing provider/model keys.
    /// 使用现有供应商/模型键的显式逐模型协议偏好。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub model_protocol_preferences: BTreeMap<String, CustomModelProtocolPreference>,
    /// Provider/model enablement differences.
    /// 供应商/模型启用差异。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub model_enabled_overrides: BTreeMap<String, bool>,
    /// Optional provider-wide capability override.
    /// 可选的供应商级能力覆盖。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_capability_override: Option<ModelCapabilityOverrides>,
    /// Exact provider/model capability overrides.
    /// 精确供应商/模型能力覆盖。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub model_capability_overrides: BTreeMap<String, ModelCapabilityOverrides>,
    /// Optional provider-wide context-window policy override.
    /// 可选的供应商级上下文窗口策略覆盖。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_context_window_policy_override: Option<ContextWindowPolicyOverrides>,
    /// Exact provider/model context-window policy overrides.
    /// 精确供应商/模型上下文窗口策略覆盖。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub model_context_window_policy_overrides: BTreeMap<String, ContextWindowPolicyOverrides>,
    /// Exact provider/model/protocol execution overrides.
    /// 精确供应商/模型/协议执行覆盖。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub model_execution_overrides: BTreeMap<String, ModelExecutionOverrides>,
    /// Internal operation enablement differences owned by this provider.
    /// 由该供应商持有的内部操作启用差异。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub internal_model_enabled_overrides: BTreeMap<String, bool>,
    /// Internal operation defaults owned by this provider.
    /// 由该供应商持有的内部操作默认值。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub internal_model_overrides: BTreeMap<String, InternalModelOverrides>,
}

/// Complete versioned document persisted for exactly one custom provider.
/// 为且仅为一个自定义供应商持久化的完整版本化文档。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomProviderManifestDocumentV1 {
    /// Fixed standalone-document schema identifier.
    /// 固定独立文档 Schema 标识。
    pub schema_version: String,
    /// Full provider manifest compiled with custom ownership.
    /// 以自定义所有权编译的完整供应商 Manifest。
    pub provider_manifest: ProviderManifestV1,
    /// Provider-owned local settings excluded from the central configuration file.
    /// 排除在中央配置文件之外的供应商自有本地设置。
    #[serde(default)]
    pub local_overrides: CustomProviderLocalOverridesV1,
}

/// Backward-compatible public name for callers that previously referenced the standalone DTO.
/// 供此前引用独立 DTO 的调用方使用的向后兼容公开名称。
pub type StandaloneCustomManifestDocument = CustomProviderManifestDocumentV1;

/// Returns the collision-resistant prefix shared by provider/model configuration keys.
/// 返回供应商/模型配置键共享的抗冲突前缀。
fn provider_model_prefix(provider_id: &str) -> String {
    format!("{}:{provider_id}:", provider_id.len())
}

/// Reports whether one existing composite key belongs to the exact provider.
/// 报告一个现有组合键是否属于精确供应商。
fn composite_key_belongs_to_provider(key: &str, provider_id: &str) -> bool {
    key.starts_with(provider_model_prefix(provider_id).as_str())
}

/// Extracts every provider-owned local setting without moving connection-owned state.
/// 提取全部供应商自有本地设置，同时不移动连接自有状态。
fn local_overrides_from_configuration(
    configuration: &PersistedAiConfiguration,
    provider_id: &str,
) -> CustomProviderLocalOverridesV1 {
    let filter = |key: &&String| composite_key_belongs_to_provider(key, provider_id);
    CustomProviderLocalOverridesV1 {
        custom_models: configuration
            .custom_models
            .get(provider_id)
            .cloned()
            .unwrap_or_default(),
        provider_model_defaults: configuration
            .custom_provider_model_defaults
            .get(provider_id)
            .cloned(),
        model_template_decisions: configuration
            .custom_model_template_decisions
            .iter()
            .filter(|(key, _)| filter(key))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
        model_protocol_preferences: configuration
            .custom_model_protocol_preferences
            .iter()
            .filter(|(key, _)| filter(key))
            .map(|(key, value)| (key.clone(), *value))
            .collect(),
        model_enabled_overrides: configuration
            .model_enabled_overrides
            .iter()
            .filter(|(key, _)| filter(key))
            .map(|(key, value)| (key.clone(), *value))
            .collect(),
        provider_capability_override: configuration
            .provider_capability_overrides
            .get(provider_id)
            .cloned(),
        model_capability_overrides: configuration
            .model_capability_overrides
            .iter()
            .filter(|(key, _)| filter(key))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
        provider_context_window_policy_override: configuration
            .provider_context_window_policy_overrides
            .get(provider_id)
            .cloned(),
        model_context_window_policy_overrides: configuration
            .model_context_window_policy_overrides
            .iter()
            .filter(|(key, _)| filter(key))
            .map(|(key, value)| (key.clone(), *value))
            .collect(),
        model_execution_overrides: configuration
            .model_overrides
            .iter()
            .filter(|(key, _)| filter(key))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
        internal_model_enabled_overrides: configuration
            .internal_model_enabled_overrides
            .iter()
            .filter(|(key, _)| filter(key))
            .map(|(key, value)| (key.clone(), *value))
            .collect(),
        internal_model_overrides: configuration
            .internal_model_overrides
            .iter()
            .filter(|(key, _)| filter(key))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    }
}

/// Rejects a standalone document whose local keys escape its provider ownership boundary.
/// 拒绝本地键逃逸出其供应商所有权边界的独立文档。
fn validate_local_override_ownership(
    provider_id: &str,
    overrides: &CustomProviderLocalOverridesV1,
) -> Result<(), String> {
    for (kind, keys) in [
        (
            "model template decision",
            overrides
                .model_template_decisions
                .keys()
                .collect::<Vec<_>>(),
        ),
        (
            "model protocol preference",
            overrides
                .model_protocol_preferences
                .keys()
                .collect::<Vec<_>>(),
        ),
        (
            "model enabled override",
            overrides.model_enabled_overrides.keys().collect::<Vec<_>>(),
        ),
        (
            "model capability override",
            overrides
                .model_capability_overrides
                .keys()
                .collect::<Vec<_>>(),
        ),
        (
            "model context-window override",
            overrides
                .model_context_window_policy_overrides
                .keys()
                .collect::<Vec<_>>(),
        ),
        (
            "model execution override",
            overrides
                .model_execution_overrides
                .keys()
                .collect::<Vec<_>>(),
        ),
        (
            "internal model enabled override",
            overrides
                .internal_model_enabled_overrides
                .keys()
                .collect::<Vec<_>>(),
        ),
        (
            "internal model override",
            overrides
                .internal_model_overrides
                .keys()
                .collect::<Vec<_>>(),
        ),
    ] {
        if let Some(key) = keys
            .into_iter()
            .find(|key| !composite_key_belongs_to_provider(key, provider_id))
        {
            return Err(format!(
                "Standalone custom manifest for `{provider_id}` contains {kind} key `{key}` owned by another provider."
            ));
        }
    }
    for (model_id, definition) in &overrides.custom_models {
        if model_id != &definition.id {
            return Err(format!(
                "Standalone custom manifest for `{provider_id}` indexes model `{model_id}` with definition `{}`.",
                definition.id
            ));
        }
        definition.validate()?;
    }
    if let Some(defaults) = &overrides.provider_model_defaults {
        defaults.validate()?;
    }
    Ok(())
}

/// Removes only the provider-owned fields that are authoritative in one standalone document.
/// 仅移除由一份独立文档权威持有的供应商自有字段。
fn remove_hydrated_provider_owned_state(
    configuration: &mut PersistedAiConfiguration,
    provider_id: &str,
) {
    configuration.custom_models.remove(provider_id);
    configuration
        .custom_provider_model_defaults
        .remove(provider_id);
    configuration
        .provider_capability_overrides
        .remove(provider_id);
    configuration
        .provider_context_window_policy_overrides
        .remove(provider_id);
    let retain_other_provider = |key: &String| !composite_key_belongs_to_provider(key, provider_id);
    configuration
        .custom_model_template_decisions
        .retain(|key, _| retain_other_provider(key));
    configuration
        .custom_model_protocol_preferences
        .retain(|key, _| retain_other_provider(key));
    configuration
        .model_enabled_overrides
        .retain(|key, _| retain_other_provider(key));
    configuration
        .model_capability_overrides
        .retain(|key, _| retain_other_provider(key));
    configuration
        .model_context_window_policy_overrides
        .retain(|key, _| retain_other_provider(key));
    configuration
        .model_overrides
        .retain(|key, _| retain_other_provider(key));
    configuration
        .internal_model_enabled_overrides
        .retain(|key, _| retain_other_provider(key));
    configuration
        .internal_model_overrides
        .retain(|key, _| retain_other_provider(key));
}

/// Hydrates one validated local override set into the compatibility configuration projection.
/// 把一组已验证本地覆盖补水到兼容配置投影中。
fn apply_local_overrides(
    configuration: &mut PersistedAiConfiguration,
    provider_id: &str,
    overrides: CustomProviderLocalOverridesV1,
) -> Result<(), String> {
    validate_local_override_ownership(provider_id, &overrides)?;
    remove_hydrated_provider_owned_state(configuration, provider_id);
    if !overrides.custom_models.is_empty() {
        configuration
            .custom_models
            .insert(provider_id.to_string(), overrides.custom_models);
    }
    if let Some(defaults) = overrides.provider_model_defaults {
        configuration
            .custom_provider_model_defaults
            .insert(provider_id.to_string(), defaults);
    }
    if let Some(value) = overrides.provider_capability_override {
        configuration
            .provider_capability_overrides
            .insert(provider_id.to_string(), value);
    }
    if let Some(value) = overrides.provider_context_window_policy_override {
        configuration
            .provider_context_window_policy_overrides
            .insert(provider_id.to_string(), value);
    }
    configuration
        .custom_model_template_decisions
        .extend(overrides.model_template_decisions);
    configuration
        .custom_model_protocol_preferences
        .extend(overrides.model_protocol_preferences);
    configuration
        .model_enabled_overrides
        .extend(overrides.model_enabled_overrides);
    configuration
        .model_capability_overrides
        .extend(overrides.model_capability_overrides);
    configuration
        .model_context_window_policy_overrides
        .extend(overrides.model_context_window_policy_overrides);
    configuration
        .model_overrides
        .extend(overrides.model_execution_overrides);
    configuration
        .internal_model_enabled_overrides
        .extend(overrides.internal_model_enabled_overrides);
    configuration
        .internal_model_overrides
        .extend(overrides.internal_model_overrides);
    Ok(())
}

/// Builds the complete standalone document for one in-memory custom provider.
/// 为一个内存自定义供应商构建完整独立文档。
fn document_from_configuration(
    configuration: &PersistedAiConfiguration,
    provider_id: &str,
    descriptor: &ProviderDescriptor,
) -> Result<CustomProviderManifestDocumentV1, String> {
    let mut provider_manifest = match configuration.custom_provider_manifests.get(provider_id) {
        Some(provider_manifest) => provider_manifest.clone(),
        None => vulcan_provider_adapters::project_custom_descriptor_manifest(descriptor)?,
    };
    let generated_compatibility_manifest =
        vulcan_provider_adapters::project_custom_descriptor_manifest(descriptor)?;
    let compiled = vulcan_provider_adapters::compile_manifest(
        &provider_manifest,
        ManifestOwnership::Custom,
    )
    .map_err(|error| format!("Custom provider manifest `{provider_id}` is invalid: {error}"))?;
    let descriptor_changed =
        !vulcan_provider_adapters::custom_manifest_matches_activated_descriptor(
            compiled.provider(),
            descriptor,
        );
    if descriptor_changed {
        let mut generated = generated_compatibility_manifest.clone();
        preserve_existing_protocol_profile_refs(
            &provider_manifest.services,
            &mut generated.services,
        );
        provider_manifest.display = generated.display;
        provider_manifest.credential_registration = generated.credential_registration;
        provider_manifest.services = generated.services;
        provider_manifest.model_defaults.supported_protocols =
            generated.model_defaults.supported_protocols;
        provider_manifest.model_defaults.selected_protocol =
            generated.model_defaults.selected_protocol;
        provider_manifest.revision =
            next_manifest_revision(provider_id, provider_manifest.revision)?;
    }
    let usage_refs_changed = materialize_missing_standard_usage_refs(
        &generated_compatibility_manifest.services,
        &mut provider_manifest.services,
    );
    let usage_profiles_changed = merge_missing_usage_profile_declarations(
        &mut provider_manifest.usage_profiles,
        generated_compatibility_manifest.usage_profiles,
    );
    if !descriptor_changed && (usage_refs_changed || usage_profiles_changed) {
        provider_manifest.revision =
            next_manifest_revision(provider_id, provider_manifest.revision)?;
    }
    let local_overrides = local_overrides_from_configuration(configuration, provider_id);
    let document = CustomProviderManifestDocumentV1 {
        schema_version: CUSTOM_PROVIDER_MANIFEST_SCHEMA_VERSION.to_string(),
        provider_manifest,
        local_overrides,
    };
    validate_document(provider_id, &document)?;
    Ok(document)
}

/// Advances one custom Manifest revision without allowing identity reuse after exhaustion.
/// 推进一份自定义 Manifest 修订，且不允许在耗尽后复用身份。
///
/// # Parameters
/// 参数
/// `provider_id` identifies the standalone custom provider and scopes the controlled diagnostic.
/// `provider_id` 标识独立自定义供应商并限定受控诊断范围。
/// `current_revision` is the revision whose content is about to change.
/// `current_revision` 是内容即将发生变化的当前修订。
///
/// # Returns
/// 返回值
/// The exact next revision, or a fail-closed error when the unsigned range is exhausted.
/// 返回精确下一修订；无符号范围耗尽时返回失败关闭错误。
fn next_manifest_revision(provider_id: &str, current_revision: u64) -> Result<u64, String> {
    current_revision.checked_add(1).ok_or_else(|| {
        format!(
            "Standalone custom manifest `{provider_id}` cannot advance beyond revision {current_revision}."
        )
    })
}

/// Preserves advanced protocol-level Profile refs for routes that survive a descriptor edit.
/// 为在描述符编辑后仍存在的路由保留高级协议级 Profile 引用。
///
/// `existing_services` is the standalone Manifest authority before the UI-owned descriptor edit.
/// `existing_services` 是 UI 持有的描述符编辑前的独立 Manifest 权威。
/// `generated_services` contains updated connection facts and receives matching advanced refs.
/// `generated_services` 包含更新后的连接事实，并接收匹配路由的高级引用。
/// The function mutates matching generated routes and returns no value.
/// 此函数会修改匹配的生成路由且不返回值。
fn preserve_existing_protocol_profile_refs(
    existing_services: &[vulcan_provider_adapters::provider_manifest::ManifestServiceEntry],
    generated_services: &mut [vulcan_provider_adapters::provider_manifest::ManifestServiceEntry],
) {
    for generated_service in generated_services {
        let Some(existing_service) = existing_services.iter().find(|existing_service| {
            existing_service.service_entry_id == generated_service.service_entry_id
        }) else {
            continue;
        };
        for generated_route in &mut generated_service.protocol_routes {
            let Some(existing_route) = existing_service
                .protocol_routes
                .iter()
                .find(|route| route.protocol_profile_id == generated_route.protocol_profile_id)
            else {
                continue;
            };
            if !existing_route.usage_profile_refs.is_empty() {
                generated_route.usage_profile_refs = existing_route.usage_profile_refs.clone();
            }
            generated_route.request_profile_refs = existing_route.request_profile_refs.clone();
            generated_route.response_profile_refs = existing_route.response_profile_refs.clone();
        }
    }
}

/// Materializes standard Usage refs only on routes that do not already declare provider rules.
/// 仅在尚未声明供应商规则的路由上物化标准用量引用。
///
/// `generated_services` owns the protocol-specific compatibility refs generated from the descriptor.
/// `generated_services` 持有根据描述符生成的协议专属兼容引用。
/// `target_services` is the standalone Manifest and retains every non-empty user-owned ref list.
/// `target_services` 是独立 Manifest，并保留每个非空的用户持有引用列表。
/// Returns whether at least one previously implicit route gained explicit compatibility refs.
/// 返回是否至少有一条原先隐式的路由获得显式兼容引用。
fn materialize_missing_standard_usage_refs(
    generated_services: &[vulcan_provider_adapters::provider_manifest::ManifestServiceEntry],
    target_services: &mut [vulcan_provider_adapters::provider_manifest::ManifestServiceEntry],
) -> bool {
    let mut changed = false;
    for target_service in target_services {
        let Some(generated_service) = generated_services.iter().find(|generated_service| {
            generated_service.service_entry_id == target_service.service_entry_id
        }) else {
            continue;
        };
        for target_route in &mut target_service.protocol_routes {
            if !target_route.usage_profile_refs.is_empty() {
                continue;
            }
            let Some(generated_route) = generated_service
                .protocol_routes
                .iter()
                .find(|route| route.protocol_profile_id == target_route.protocol_profile_id)
            else {
                continue;
            };
            target_route.usage_profile_refs = generated_route.usage_profile_refs.clone();
            changed = changed || !target_route.usage_profile_refs.is_empty();
        }
    }
    changed
}

/// Adds newly materialized standard Usage declarations without replacing user-owned profiles.
/// 添加新物化的标准用量声明，同时不替换用户持有的 Profile。
///
/// `existing` owns the current standalone definitions and `generated` contains compatibility defaults.
/// `existing` 持有当前独立定义，`generated` 包含兼容默认值。
/// Existing stable IDs win; only absent declarations are appended in deterministic route order.
/// 已存在的稳定标识优先；仅按确定性路由顺序追加缺失声明。
/// Returns whether any missing declaration was appended.
/// 返回是否追加了任何缺失声明。
fn merge_missing_usage_profile_declarations(
    existing: &mut Vec<vulcan_provider_adapters::provider_manifest::UsageProfileDeclaration>,
    generated: Vec<vulcan_provider_adapters::provider_manifest::UsageProfileDeclaration>,
) -> bool {
    let mut changed = false;
    for declaration in generated {
        if existing
            .iter()
            .any(|current| current.profile_id == declaration.profile_id)
        {
            continue;
        }
        existing.push(declaration);
        changed = true;
    }
    changed
}

/// Validates one complete custom document and its exact file identity.
/// 校验一份完整自定义文档及其精确文件身份。
fn validate_document(
    provider_id: &str,
    document: &CustomProviderManifestDocumentV1,
) -> Result<ProviderDescriptor, String> {
    if document.schema_version != CUSTOM_PROVIDER_MANIFEST_SCHEMA_VERSION {
        return Err(format!(
            "Standalone custom manifest for `{provider_id}` uses unsupported schema `{}`.",
            document.schema_version
        ));
    }
    if document.provider_manifest.provider_id != provider_id {
        return Err(format!(
            "Standalone custom manifest file for `{provider_id}` declares provider `{}`.",
            document.provider_manifest.provider_id
        ));
    }
    validate_local_override_ownership(provider_id, &document.local_overrides)?;
    let compiled = vulcan_provider_adapters::compile_manifest(
        &document.provider_manifest,
        ManifestOwnership::Custom,
    )
    .map_err(|error| {
        format!("Standalone custom manifest for `{provider_id}` is invalid: {error}")
    })?;
    Ok(compiled.provider().clone())
}

/// Parses the current full document or the one explicit descriptor-only legacy schema.
/// 解析当前完整文档，或唯一明确的仅描述符旧版 Schema。
fn parse_document(provider_id: &str, text: &str) -> Result<ParsedCustomProviderDocument, String> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|error| {
        format!("Standalone custom manifest for `{provider_id}` is invalid JSON: {error}")
    })?;
    vulcan_provider_adapters::provider_manifest::validate_runtime_json_bounds(
        &value,
        "Standalone custom provider manifest",
    )?;
    let (document, legacy_descriptor_v0) = if value
        .get("schema_version")
        .and_then(serde_json::Value::as_str)
        == Some(CUSTOM_PROVIDER_MANIFEST_SCHEMA_VERSION)
    {
        (
            serde_json::from_value(value).map_err(|error| {
                format!("Standalone custom manifest for `{provider_id}` is invalid: {error}")
            })?,
            false,
        )
    } else if value.get("id").and_then(serde_json::Value::as_str) == Some(provider_id)
        && value.get("provider_manifest").is_none()
    {
        let descriptor: ProviderDescriptor = serde_json::from_value(value).map_err(|error| {
            format!("Legacy custom provider descriptor for `{provider_id}` is invalid: {error}")
        })?;
        (
            CustomProviderManifestDocumentV1 {
                schema_version: CUSTOM_PROVIDER_MANIFEST_SCHEMA_VERSION.to_string(),
                provider_manifest: vulcan_provider_adapters::project_custom_descriptor_manifest(
                    &descriptor,
                )?,
                local_overrides: CustomProviderLocalOverridesV1::default(),
            },
            true,
        )
    } else {
        return Err(format!(
            "Standalone custom manifest for `{provider_id}` has no supported explicit schema."
        ));
    };
    validate_document(provider_id, &document)?;
    Ok(ParsedCustomProviderDocument {
        document,
        legacy_descriptor_v0,
    })
}

/// Serializes one complete standalone custom-provider document deterministically.
/// 确定性序列化一份完整独立自定义供应商文档。
fn serialize_document(document: &CustomProviderManifestDocumentV1) -> Result<String, String> {
    serde_json::to_string_pretty(document)
        .map_err(|error| format!("Failed to serialize custom provider manifest: {error}"))
}

/// Truncates one public hydration diagnostic component at a Unicode-character boundary.
/// 在 Unicode 字符边界截断一个公开补水诊断组件。
///
/// # Parameters
/// 参数
/// `value` is the non-secret source text and `maximum_characters` is its closed display ceiling.
/// `value` 是非秘密源文本；`maximum_characters` 是其封闭展示上限。
///
/// # Returns
/// 返回值
/// The original value within the bound, or a suffix-marked truncated value.
/// 返回上限内原值，或带后缀标记的截断值。
fn bounded_custom_provider_hydration_component(value: &str, maximum_characters: usize) -> String {
    if value.chars().count() <= maximum_characters {
        return value.to_string();
    }
    // retainedCharacters reserves the suffix inside the published character ceiling.
    // retainedCharacters 在发布的字符上限内为截断后缀保留空间。
    let retained_characters = maximum_characters.saturating_sub(3);
    // bounded is the exact prefix safe to expose through logs, RPC, and WebUI.
    // bounded 是可通过日志、RPC 与 WebUI 暴露的精确安全前缀。
    let mut bounded = value.chars().take(retained_characters).collect::<String>();
    bounded.push_str("...");
    bounded
}

/// Records one isolated provider failure under deterministic count and text ceilings.
/// 在确定性数量与文本上限下记录一条隔离供应商失败。
///
/// # Parameters
/// 参数
/// `report` owns the bounded public result; `provider_id` and `message` identify the exact failed
/// source and repair reason before display truncation.
/// `report` 持有有界公开结果；`provider_id` 与 `message` 在展示截断前标识精确
/// 失败源与修复原因。
fn record_custom_provider_hydration_failure(
    report: &mut CustomProviderHydrationReport,
    provider_id: &str,
    message: &str,
) {
    report.failure_total_count = report.failure_total_count.saturating_add(1);
    if report.failures.len() >= CUSTOM_PROVIDER_HYDRATION_FAILURE_MAX_COUNT {
        report.failures_truncated = true;
        return;
    }
    report.failures.push(CustomProviderManifestFailure {
        provider_id: bounded_custom_provider_hydration_component(
            provider_id,
            CUSTOM_PROVIDER_HYDRATION_FAILURE_ID_MAX_CHARACTERS,
        ),
        message: bounded_custom_provider_hydration_component(
            message,
            CUSTOM_PROVIDER_HYDRATION_FAILURE_MESSAGE_MAX_CHARACTERS,
        ),
    });
}

/// Hydrates the in-memory custom provider map from the standalone manifest files.
/// 从独立清单文件为内存自定义供应商映射补水。
///
/// # Parameters
/// 参数
/// `store` owns the custom manifest directory; `configuration` carries the reference set.
/// `store` 持有自定义清单目录；`configuration` 携带引用集。
///
/// # Returns
/// 返回值
/// A report after every referenced standalone file is loaded; a missing or invalid file is a
/// controlled failure, and explicit descriptor-only sources are identified for transactional
/// rewriting by the startup owner.
/// 每个被引用独立文件加载后返回报告；文件缺失或无效时受控失败，明确的仅描述符源会被
/// 标识出来，由启动流程所有者通过事务重写。
pub fn load_custom_provider_manifests(
    store: &ManifestStoreLayout,
    configuration: &mut PersistedAiConfiguration,
) -> Result<CustomProviderHydrationReport, String> {
    // provider_ids freezes the exact central reference set before any standalone bytes are read.
    // provider_ids 在读取任何独立文件字节前冻结精确的中央引用集合。
    let provider_ids = configuration
        .custom_provider_file_refs
        .iter()
        .cloned()
        .collect::<Vec<_>>();
    // Every central reference is validated before any descriptor removal, file read, or isolation
    // projection so a malformed system-like identity can never shadow a built-in provider.
    // 每个中央引用都在移除描述符、读取文件或生成隔离投影前完成校验，确保损坏的
    // 系统式身份绝不可能遮蔽内置供应商。
    for provider_id in &provider_ids {
        validate_custom_provider_identifier(provider_id.as_str())
            .map_err(|error| format!("Invalid standalone custom provider reference: {error}"))?;
    }
    // parsed_documents validates every referenced file before the caller-visible configuration changes.
    // parsed_documents 在调用方可见配置发生变化前校验每一份被引用文件。
    let mut parsed_documents = Vec::with_capacity(provider_ids.len());
    let mut report = CustomProviderHydrationReport::default();
    for provider_id in &provider_ids {
        // text is the exact bounded standalone source; read failures isolate only this provider.
        // text 是精确有界独立源；读取失败只隔离当前供应商。
        let text = match store.read_custom_manifest(provider_id) {
            Ok(Some(text)) => text,
            Ok(None) => {
                record_custom_provider_hydration_failure(
                    &mut report,
                    provider_id,
                    format!(
                        "Standalone custom manifest for `{provider_id}` is referenced but missing."
                    )
                    .as_str(),
                );
                continue;
            }
            Err(error) => {
                record_custom_provider_hydration_failure(&mut report, provider_id, error.as_str());
                continue;
            }
        };
        // parsed is accepted only through the two explicit current/legacy document shapes.
        // parsed 只通过两种明确的当前/历史文档形态接受。
        let parsed = match parse_document(provider_id.as_str(), &text) {
            Ok(parsed) => parsed,
            Err(error) => {
                record_custom_provider_hydration_failure(&mut report, provider_id, error.as_str());
                continue;
            }
        };
        if parsed.legacy_descriptor_v0 {
            report
                .legacy_descriptor_provider_ids
                .push(provider_id.clone());
        }
        let document = parsed.document;
        // descriptor is the only activation-ready projection accepted from this document.
        // descriptor 是从该文档接受的唯一可激活投影。
        let descriptor = match validate_document(provider_id.as_str(), &document) {
            Ok(descriptor) => descriptor,
            Err(error) => {
                record_custom_provider_hydration_failure(&mut report, provider_id, error.as_str());
                continue;
            }
        };
        parsed_documents.push((provider_id.clone(), document, descriptor));
    }
    // candidate guarantees a late ownership or hydration failure cannot expose a partial provider set.
    // candidate 保证后期所有权或补水失败不会暴露部分供应商集合。
    let mut candidate = configuration.clone();
    for provider_id in &provider_ids {
        // A referenced standalone source is authoritative even when invalid; removing any embedded
        // descriptor prevents an undeclared fallback to stale central provider bytes.
        // 被引用的独立源即使无效也是权威来源；移除任何内嵌描述可防止未声明
        // 回退到过期中央供应商字节。
        candidate.custom_providers.remove(provider_id);
        candidate.custom_provider_manifests.remove(provider_id);
    }
    for (provider_id, document, descriptor) in parsed_documents {
        // providerCandidate isolates a late local-override failure from every previously accepted
        // provider and from the caller-visible before-image.
        // providerCandidate 将后期本地覆盖失败与每个已接受供应商及调用者
        // 可见前镜像隔离。
        let mut provider_candidate = candidate.clone();
        if let Err(error) = apply_local_overrides(
            &mut provider_candidate,
            provider_id.as_str(),
            document.local_overrides.clone(),
        ) {
            record_custom_provider_hydration_failure(
                &mut report,
                provider_id.as_str(),
                error.as_str(),
            );
            continue;
        }
        provider_candidate
            .custom_providers
            .insert(provider_id.clone(), descriptor);
        provider_candidate
            .custom_provider_manifests
            .insert(provider_id.clone(), document.provider_manifest);
        candidate = provider_candidate;
    }
    candidate.custom_provider_manifest_failures = report.failures.clone();
    candidate.custom_provider_manifest_failure_total_count = report.failure_total_count;
    candidate.custom_provider_manifest_failures_truncated = report.failures_truncated;
    *configuration = candidate;
    Ok(report)
}

/// Synchronizes the standalone manifest files with the in-memory custom provider map.
/// 将独立清单文件与内存自定义供应商映射同步。
///
/// Every in-memory provider is written (or refreshed) to its standalone file; every
/// previously migrated provider that no longer exists in memory has its file removed so
/// deletion is durable and the reference set stays exact (plan §R6.2).
/// 每个内存供应商被写入（或刷新）到独立文件；已迁移但内存中已不存在的供应商其文件
/// 被删除，使删除持久化且引用集保持精确（方案 §R6.2）。
///
/// # Parameters
/// 参数
/// `store` owns the custom manifest directory; `configuration` carries the authoritative map.
/// `store` 持有自定义清单目录；`configuration` 携带权威映射。
pub fn sync_custom_provider_manifests(
    store: &ManifestStoreLayout,
    configuration: &PersistedAiConfiguration,
) -> Result<(), String> {
    for (provider_id, descriptor) in &configuration.custom_providers {
        let document = document_from_configuration(configuration, provider_id, descriptor)?;
        store.write_custom_manifest(provider_id, serialize_document(&document)?.as_bytes())?;
    }
    for provider_id in configuration.custom_provider_file_refs.iter() {
        if !configuration.custom_providers.contains_key(provider_id) {
            store.remove_custom_manifest(provider_id)?;
        }
    }
    Ok(())
}

/// Commits standalone custom manifests and the central reference document as one recoverable transaction.
/// 把独立自定义清单与中央引用文档作为一个可恢复事务提交。
///
/// # Parameters
/// 参数
/// `store` owns standalone files, `central_path` is exact, `configuration` is the in-memory
/// authority, and `central_bytes` is its already projected TOML representation.
/// `store` 持有独立文件，`central_path` 为精确路径，`configuration` 是内存权威，
/// `central_bytes` 是其已投影 TOML 表示。
///
/// # Returns
/// 返回值
/// Success only after both file sets are durable and the journal is removed. A synchronous
/// failure restores the exact before image; a crash leaves the target image for startup recovery.
/// 仅在两组文件均持久且日志已移除后返回成功。同步失败恢复精确 before 镜像；进程崩溃时
/// 留下 target 镜像供启动恢复。
pub fn persist_custom_provider_transaction(
    store: &ManifestStoreLayout,
    central_path: &Path,
    configuration: &PersistedAiConfiguration,
    central_bytes: &[u8],
) -> Result<(), String> {
    persist_custom_provider_transaction_with_before_central_write(
        store,
        central_path,
        configuration,
        central_bytes,
        || Ok(()),
    )
}

/// Commits one custom-provider transaction with an observable pre-central-write boundary.
/// 在可观察的中央文件写入前边界提交一项自定义供应商事务。
///
/// # Parameters
/// 参数
/// `store`, `central_path`, `configuration`, and `central_bytes` are the exact production
/// transaction inputs. `before_central_write` runs after the complete target manifest image is
/// installed and before the central target image is touched.
/// `store`、`central_path`、`configuration` 与 `central_bytes` 是精确的生产事务输入；
/// `before_central_write` 在完整目标 Manifest 镜像安装后、中央目标镜像被触碰前运行。
///
/// # Returns
/// 返回值
/// Success after the journal is removed, or a failure after exact synchronous rollback.
/// 日志移除后返回成功；失败时在完成精确同步回滚后返回错误。
fn persist_custom_provider_transaction_with_before_central_write<F>(
    store: &ManifestStoreLayout,
    central_path: &Path,
    configuration: &PersistedAiConfiguration,
    central_bytes: &[u8],
    before_central_write: F,
) -> Result<(), String>
where
    F: FnOnce() -> Result<(), String>,
{
    recover_custom_provider_transaction(store, central_path)?;
    let before_configuration = read_optional_utf8_file(central_path, "central AI configuration")?;
    let before_manifests = read_all_custom_manifest_texts(store)?;
    let mut target_manifests = configuration
        .custom_providers
        .iter()
        .map(|(provider_id, descriptor)| {
            document_from_configuration(configuration, provider_id, descriptor).and_then(
                |document| serialize_document(&document).map(|text| (provider_id.clone(), text)),
            )
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    for provider_id in &configuration.custom_provider_file_refs {
        if target_manifests.contains_key(provider_id.as_str()) {
            continue;
        }
        // originalText is copied byte-for-byte into the transaction target only for a referenced
        // source that failed hydration. Missing files remain missing, while corrupt bytes cannot
        // be rewritten or deleted by an unrelated settings mutation.
        // originalText 仅针对补水失败且仍被引用的源逐字节复制到事务目标；缺失文件继续缺失，
        // 损坏字节则不会被无关设置变更重写或删除。
        if let Some(original_text) = before_manifests.get(provider_id.as_str()) {
            target_manifests.insert(provider_id.clone(), original_text.clone());
        }
    }
    let target_configuration = std::str::from_utf8(central_bytes)
        .map_err(|error| format!("Central AI configuration is not UTF-8: {error}"))?
        .to_string();
    let journal = CustomProviderTransactionJournal {
        schema_version: 1,
        before_configuration,
        target_configuration,
        before_manifests,
        target_manifests,
    };
    write_custom_provider_transaction_journal(store, &journal)?;
    let commit = apply_custom_provider_manifest_image(store, &journal.target_manifests)
        .and_then(|()| before_central_write())
        .and_then(|()| {
            apply_custom_provider_central_image(
                central_path,
                Some(journal.target_configuration.as_str()),
            )
        });
    if let Err(error) = commit {
        let rollback = apply_custom_provider_transaction_image(
            store,
            central_path,
            &journal.before_manifests,
            journal.before_configuration.as_deref(),
        );
        if rollback.is_ok() {
            remove_custom_provider_transaction_journal(store)?;
        }
        return Err(format!(
            "Custom provider transaction commit failed: {error}; rollback: {rollback:?}"
        ));
    }
    remove_custom_provider_transaction_journal(store)
}

/// Completes a crash-interrupted custom-provider transaction before configuration hydration.
/// 在配置补水前完成一项因进程崩溃而中断的自定义供应商事务。
///
/// # Parameters
/// 参数
/// `store` and `central_path` must identify the same application data plane used for commit.
/// `store` 与 `central_path` 必须标识提交时使用的同一应用数据平面。
///
/// # Returns
/// 返回值
/// Success when no journal exists or its complete target image has been durably replayed.
/// 日志不存在或其完整 target 镜像已持久重放时返回成功。
pub fn recover_custom_provider_transaction(
    store: &ManifestStoreLayout,
    central_path: &Path,
) -> Result<(), String> {
    let path = custom_provider_transaction_journal_path(store);
    let Some(text) = read_optional_bounded_utf8_file(
        &path,
        CUSTOM_PROVIDER_TRANSACTION_JOURNAL_MAX_BYTES,
        "custom provider transaction journal",
    )?
    else {
        return Ok(());
    };
    let value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|error| format!("Custom provider transaction journal is invalid: {error}"))?;
    vulcan_provider_adapters::provider_manifest::validate_runtime_json_bounds(
        &value,
        "Custom provider transaction journal",
    )?;
    let journal: CustomProviderTransactionJournal = serde_json::from_value(value)
        .map_err(|error| format!("Custom provider transaction journal is invalid: {error}"))?;
    if journal.schema_version != 1 {
        return Err(format!(
            "Unsupported custom provider transaction journal schema version {}.",
            journal.schema_version
        ));
    }
    apply_custom_provider_transaction_image(
        store,
        central_path,
        &journal.target_manifests,
        Some(journal.target_configuration.as_str()),
    )?;
    remove_custom_provider_transaction_journal(store)
}

/// Returns the fixed journal path below the manifest store root.
/// 返回清单存储根目录下的固定日志路径。
fn custom_provider_transaction_journal_path(store: &ManifestStoreLayout) -> std::path::PathBuf {
    store
        .manifests_root()
        .join("custom-provider-transaction.v1.json")
}

/// Returns the immutable pre-migration backup path below the manifest store root.
/// 返回 Manifest 存储根目录下不可变迁移前备份路径。
///
/// # Parameters
/// 参数
/// `store` owns the exact custom-provider persistence root.
/// `store` 持有精确的自定义供应商持久化根目录。
///
/// # Returns
/// 返回值
/// The fixed version-one backup path.
/// 返回固定的第一版备份路径。
pub fn custom_provider_migration_backup_path(store: &ManifestStoreLayout) -> std::path::PathBuf {
    store
        .manifests_root()
        .join("custom-provider-migration-backup.v1.json")
}

/// Returns the migration-completion marker path below the manifest store root.
/// 返回 Manifest 存储根目录下迁移完成标记路径。
///
/// # Parameters
/// 参数
/// `store` owns the exact custom-provider persistence root.
/// `store` 持有精确的自定义供应商持久化根目录。
///
/// # Returns
/// 返回值
/// The fixed version-one completion-marker path.
/// 返回固定的第一版完成标记路径。
pub fn custom_provider_migration_marker_path(store: &ManifestStoreLayout) -> std::path::PathBuf {
    store
        .manifests_root()
        .join("custom-provider-migration-complete.v1.json")
}

/// Creates the first immutable pre-migration backup without ever overwriting an existing one.
/// 创建首次不可变迁移前备份，且绝不覆盖现有备份。
///
/// # Parameters
/// 参数
/// `store` owns all standalone manifests and `central_configuration` is the exact legacy central
/// document, or absence.
/// `store` 持有全部独立 Manifest；`central_configuration` 是精确旧中央文档，或不存在状态。
///
/// # Returns
/// 返回值
/// Lowercase SHA-256 of the validated immutable backup bytes.
/// 返回已校验不可变备份字节的小写 SHA-256。
pub fn ensure_custom_provider_migration_backup(
    store: &ManifestStoreLayout,
    central_configuration: Option<&str>,
) -> Result<String, String> {
    let path = custom_provider_migration_backup_path(store);
    if let Some(bytes) = read_optional_bounded_bytes(
        &path,
        CUSTOM_PROVIDER_MIGRATION_BACKUP_MAX_BYTES,
        "custom provider migration backup",
    )? {
        validate_custom_provider_migration_backup(bytes.as_slice())?;
        return Ok(ManifestStoreLayout::sha256_hex(bytes.as_slice()));
    }
    let backup = CustomProviderMigrationBackup {
        schema_version: 1,
        central_configuration: central_configuration.map(str::to_string),
        custom_manifests: read_all_custom_manifest_texts(store)?,
    };
    let bytes = serde_json::to_vec(&backup).map_err(|error| {
        format!("Failed to serialize custom provider migration backup: {error}")
    })?;
    if bytes.len() as u64 > CUSTOM_PROVIDER_MIGRATION_BACKUP_MAX_BYTES {
        return Err(format!(
            "Custom provider migration backup exceeds the {CUSTOM_PROVIDER_MIGRATION_BACKUP_MAX_BYTES} byte bound."
        ));
    }
    validate_custom_provider_migration_backup(bytes.as_slice())?;
    crate::atomic_file::write_file_atomically(&path, &bytes)?;
    Ok(ManifestStoreLayout::sha256_hex(bytes.as_slice()))
}

/// Returns the digest of an existing validated migration backup.
/// 返回现有已校验迁移备份的摘要。
///
/// # Parameters
/// 参数
/// `store` owns the fixed backup path.
/// `store` 持有固定备份路径。
///
/// # Returns
/// 返回值
/// The lowercase backup SHA-256, or `None` when migration has never started.
/// 返回小写备份 SHA-256；迁移从未开始时返回 `None`。
pub fn custom_provider_migration_backup_digest(
    store: &ManifestStoreLayout,
) -> Result<Option<String>, String> {
    let path = custom_provider_migration_backup_path(store);
    let Some(bytes) = read_optional_bounded_bytes(
        &path,
        CUSTOM_PROVIDER_MIGRATION_BACKUP_MAX_BYTES,
        "custom provider migration backup",
    )?
    else {
        return Ok(None);
    };
    validate_custom_provider_migration_backup(bytes.as_slice())?;
    Ok(Some(ManifestStoreLayout::sha256_hex(bytes.as_slice())))
}

/// Persists or verifies the completion marker for one exact immutable migration backup.
/// 持久化或校验一份精确不可变迁移备份的完成标记。
///
/// # Parameters
/// 参数
/// `store` owns the fixed marker path and `backup_sha256` identifies the retained before-image.
/// `store` 持有固定标记路径；`backup_sha256` 标识保留的前镜像。
///
/// # Returns
/// 返回值
/// Success after an identical marker is durable; a conflicting marker fails closed.
/// 相同标记持久化后返回成功；冲突标记保持失败关闭。
pub fn mark_custom_provider_migration_complete(
    store: &ManifestStoreLayout,
    backup_sha256: &str,
) -> Result<(), String> {
    validate_lowercase_sha256(backup_sha256, "migration backup SHA-256")?;
    let path = custom_provider_migration_marker_path(store);
    if let Some(bytes) = read_optional_bounded_bytes(
        &path,
        CUSTOM_PROVIDER_MIGRATION_MARKER_MAX_BYTES,
        "custom provider migration marker",
    )? {
        let marker = parse_custom_provider_migration_marker(bytes.as_slice())?;
        if marker.backup_sha256 != backup_sha256 {
            return Err(String::from(
                "Custom provider migration marker refers to a different immutable backup.",
            ));
        }
        return Ok(());
    }
    let marker = CustomProviderMigrationMarker {
        schema_version: 1,
        backup_sha256: backup_sha256.to_string(),
    };
    let bytes = serde_json::to_vec(&marker).map_err(|error| {
        format!("Failed to serialize custom provider migration marker: {error}")
    })?;
    if bytes.len() as u64 > CUSTOM_PROVIDER_MIGRATION_MARKER_MAX_BYTES {
        return Err(String::from(
            "Custom provider migration marker exceeds its byte bound.",
        ));
    }
    crate::atomic_file::write_file_atomically(&path, &bytes)
}

/// Validates one immutable migration backup document and all bounded structural invariants.
/// 校验一份不可变迁移备份文档及全部有界结构不变量。
fn validate_custom_provider_migration_backup(bytes: &[u8]) -> Result<(), String> {
    let value: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|error| format!("Custom provider migration backup is invalid: {error}"))?;
    vulcan_provider_adapters::provider_manifest::validate_runtime_json_bounds(
        &value,
        "Custom provider migration backup",
    )?;
    let backup: CustomProviderMigrationBackup = serde_json::from_value(value)
        .map_err(|error| format!("Custom provider migration backup is invalid: {error}"))?;
    if backup.schema_version != 1 {
        return Err(format!(
            "Unsupported custom provider migration backup schema version {}.",
            backup.schema_version
        ));
    }
    if backup
        .custom_manifests
        .keys()
        .any(|provider_id| !provider_id.starts_with("custom:"))
    {
        return Err(String::from(
            "Custom provider migration backup contains a non-custom provider identifier.",
        ));
    }
    Ok(())
}

/// Parses one bounded migration marker through its strict versioned schema.
/// 通过严格版本化 Schema 解析一份有界迁移标记。
fn parse_custom_provider_migration_marker(
    bytes: &[u8],
) -> Result<CustomProviderMigrationMarker, String> {
    let marker: CustomProviderMigrationMarker = serde_json::from_slice(bytes)
        .map_err(|error| format!("Custom provider migration marker is invalid: {error}"))?;
    if marker.schema_version != 1 {
        return Err(format!(
            "Unsupported custom provider migration marker schema version {}.",
            marker.schema_version
        ));
    }
    validate_lowercase_sha256(marker.backup_sha256.as_str(), "migration backup SHA-256")?;
    Ok(marker)
}

/// Validates one exact lowercase SHA-256 field.
/// 校验一个精确的小写 SHA-256 字段。
fn validate_lowercase_sha256(value: &str, label: &str) -> Result<(), String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!("Custom provider {label} is invalid."));
    }
    Ok(())
}

/// Persists one bounded transaction journal atomically before any target file changes.
/// 在任何目标文件变化前原子持久化一份有界事务日志。
fn write_custom_provider_transaction_journal(
    store: &ManifestStoreLayout,
    journal: &CustomProviderTransactionJournal,
) -> Result<(), String> {
    let bytes = serde_json::to_vec(journal)
        .map_err(|error| format!("Failed to serialize custom provider transaction: {error}"))?;
    if bytes.len() as u64 > CUSTOM_PROVIDER_TRANSACTION_JOURNAL_MAX_BYTES {
        return Err(format!(
            "Custom provider transaction journal exceeds the {} byte bound.",
            CUSTOM_PROVIDER_TRANSACTION_JOURNAL_MAX_BYTES
        ));
    }
    crate::atomic_file::write_file_atomically(
        &custom_provider_transaction_journal_path(store),
        &bytes,
    )
}

/// Removes the durable transaction journal after commit or exact synchronous rollback.
/// 在提交或精确同步回滚后移除持久事务日志。
fn remove_custom_provider_transaction_journal(store: &ManifestStoreLayout) -> Result<(), String> {
    let path = custom_provider_transaction_journal_path(store);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "Failed to remove custom provider transaction journal `{}`: {error}",
            path.display()
        )),
    }
}

/// Reads the complete standalone custom manifest set as exact UTF-8 text.
/// 读取完整独立自定义清单集的精确 UTF-8 文本。
fn read_all_custom_manifest_texts(
    store: &ManifestStoreLayout,
) -> Result<BTreeMap<String, String>, String> {
    let mut manifests = BTreeMap::new();
    let mut total_bytes = 0_u64;
    for provider_id in store.list_custom_manifest_ids()? {
        let text = store.read_custom_manifest(&provider_id)?.ok_or_else(|| {
            format!("Custom provider manifest `{provider_id}` disappeared during snapshot.")
        })?;
        total_bytes = total_bytes.saturating_add(text.len() as u64);
        if total_bytes > CUSTOM_PROVIDER_TRANSACTION_JOURNAL_MAX_BYTES {
            return Err(format!(
                "Custom provider manifest snapshot exceeds the {} byte aggregate bound.",
                CUSTOM_PROVIDER_TRANSACTION_JOURNAL_MAX_BYTES
            ));
        }
        manifests.insert(provider_id, text);
    }
    Ok(manifests)
}

/// Reads one optional UTF-8 file without guessing around I/O or encoding failures.
/// 读取一份可选 UTF-8 文件，不对 I/O 或编码失败进行猜测式兜底。
fn read_optional_utf8_file(path: &Path, kind: &str) -> Result<Option<String>, String> {
    read_optional_bounded_utf8_file(path, CUSTOM_PROVIDER_TRANSACTION_JOURNAL_MAX_BYTES, kind)
}

/// Reads one optional regular UTF-8 file under an exact post-open byte boundary.
/// 在精确的打开后字节边界内读取一份可选普通 UTF-8 文件。
///
/// # Parameters
/// 参数
/// `path` is exact, `max_bytes` is the hard cap, and `kind` scopes diagnostics.
/// `path` 为精确路径，`max_bytes` 为硬上限，`kind` 限定诊断范围。
///
/// # Returns
/// 返回值
/// Exact text, absence, or a controlled type, I/O, replacement, size, or encoding failure.
/// 返回精确文本、缺失，或受控的类型、I/O、替换、大小或编码失败。
fn read_optional_bounded_utf8_file(
    path: &Path,
    max_bytes: u64,
    kind: &str,
) -> Result<Option<String>, String> {
    let Some(bytes) = read_optional_bounded_bytes(path, max_bytes, kind)? else {
        return Ok(None);
    };
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|error| format!("{kind} `{}` is not valid UTF-8: {error}", path.display()))
}

/// Reads one optional regular file under an exact post-open byte boundary.
/// 在精确的打开后字节边界内读取一份可选普通文件。
///
/// # Parameters
/// 参数
/// `path` is exact, `max_bytes` is the hard cap, and `kind` scopes diagnostics.
/// `path` 为精确路径，`max_bytes` 为硬上限，`kind` 限定诊断范围。
///
/// # Returns
/// 返回值
/// Exact bytes, absence, or a controlled type, I/O, replacement, or size failure.
/// 返回精确字节、缺失，或受控的类型、I/O、替换及大小失败。
fn read_optional_bounded_bytes(
    path: &Path,
    max_bytes: u64,
    kind: &str,
) -> Result<Option<Vec<u8>>, String> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "Failed to stat {kind} `{}`: {error}",
                path.display()
            ));
        }
    };
    if !metadata.file_type().is_file() {
        return Err(format!(
            "{kind} `{}` is not a regular file.",
            path.display()
        ));
    }
    if metadata.len() > max_bytes {
        return Err(format!(
            "{kind} `{}` exceeds the {max_bytes} byte bound.",
            path.display()
        ));
    }
    let file = std::fs::File::open(path)
        .map_err(|error| format!("Failed to open {kind} `{}`: {error}", path.display()))?;
    let mut limited = file.take(max_bytes.saturating_add(1));
    let mut bytes = Vec::new();
    limited
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Failed to read {kind} `{}`: {error}", path.display()))?;
    if bytes.len() as u64 > max_bytes {
        return Err(format!(
            "{kind} `{}` changed while reading and exceeds the {max_bytes} byte bound.",
            path.display()
        ));
    }
    Ok(Some(bytes))
}

/// Applies one complete manifest/configuration image in a deterministic, idempotent order.
/// 以确定且幂等的顺序应用一份完整清单/配置镜像。
fn apply_custom_provider_transaction_image(
    store: &ManifestStoreLayout,
    central_path: &Path,
    manifests: &BTreeMap<String, String>,
    central_configuration: Option<&str>,
) -> Result<(), String> {
    apply_custom_provider_manifest_image(store, manifests)?;
    apply_custom_provider_central_image(central_path, central_configuration)
}

/// Applies the complete standalone-manifest half of one transaction image.
/// 应用一份事务镜像的完整独立 Manifest 半区。
///
/// # Parameters
/// 参数
/// `store` owns the manifest directory and `manifests` is the exact desired file set.
/// `store` 持有 Manifest 目录，`manifests` 是精确的期望文件集合。
///
/// # Returns
/// 返回值
/// Success only after every desired file exists and every stale file is absent.
/// 仅当全部期望文件存在且全部过期文件缺失时返回成功。
fn apply_custom_provider_manifest_image(
    store: &ManifestStoreLayout,
    manifests: &BTreeMap<String, String>,
) -> Result<(), String> {
    for (provider_id, text) in manifests {
        store.write_custom_manifest(provider_id, text.as_bytes())?;
    }
    for provider_id in store.list_custom_manifest_ids()? {
        if !manifests.contains_key(&provider_id) {
            store.remove_custom_manifest(&provider_id)?;
        }
    }
    Ok(())
}

/// Applies the optional central-configuration half of one transaction image.
/// 应用一份事务镜像的可选中央配置半区。
///
/// # Parameters
/// 参数
/// `central_path` is the exact file and `central_configuration` is its desired UTF-8 content;
/// `None` means the exact desired state is file absence.
/// `central_path` 是精确文件，`central_configuration` 是其期望 UTF-8 内容；`None` 表示
/// 精确期望状态为文件不存在。
///
/// # Returns
/// 返回值
/// Success after the requested central image is durable or absent.
/// 请求的中央镜像持久化或确认不存在后返回成功。
fn apply_custom_provider_central_image(
    central_path: &Path,
    central_configuration: Option<&str>,
) -> Result<(), String> {
    match central_configuration {
        Some(text) => crate::atomic_file::write_file_atomically(central_path, text.as_bytes())?,
        None => match std::fs::remove_file(central_path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "Failed to remove central AI configuration `{}`: {error}",
                    central_path.display()
                ));
            }
        },
    }
    Ok(())
}

/// Projects one configuration into its persisted shape: the standalone files own the
/// provider bytes, the centralized document keeps only the reference set.
/// 将配置投影为持久化形态：独立文件拥有供应商字节，集中文档只保留引用集。
///
/// # Parameters
/// 参数
/// `configuration` is the authoritative in-memory map; the returned clone carries the exact
/// reference set and an empty provider map so a later save never duplicates bytes.
/// `configuration` 是权威内存映射；返回的克隆携带精确引用集与空供应商映射，后续保存
/// 绝不重复字节。
pub fn project_persisted_configuration(
    configuration: &PersistedAiConfiguration,
) -> PersistedAiConfiguration {
    let mut projected = configuration.clone();
    // fileRefs retains isolated standalone authorities exactly while adding every hydrated
    // provider. A healthy settings save therefore cannot erase a corrupt or temporarily missing
    // source that still belongs to the central reference set.
    // fileRefs 精确保留已隔离的独立权威，同时加入每个已补水供应商；因此健康设置保存不会
    // 擦除仍属于中央引用集的损坏或暂时缺失源。
    let hydrated_provider_ids = projected
        .custom_providers
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    projected
        .custom_provider_file_refs
        .extend(hydrated_provider_ids.iter().cloned());
    for provider_id in hydrated_provider_ids {
        remove_hydrated_provider_owned_state(&mut projected, provider_id.as_str());
    }
    projected.custom_providers.clear();
    projected.custom_provider_manifests.clear();
    projected
}

/// Returns the standalone custom manifest directory path below one application data root.
/// 返回一个应用数据根目录下的独立自定义清单目录路径。
///
/// # Parameters
/// 参数
/// `app_data_root` is the host-resolved application data directory.
/// `app_data_root` 是宿主解析的应用数据目录。
pub fn custom_manifests_directory(app_data_root: &Path) -> std::path::PathBuf {
    ManifestStoreLayout::new(app_data_root).custom_manifests_dir()
}

/// Loads every standalone custom provider descriptor without touching a configuration.
/// 不触碰配置，直接加载全部独立自定义供应商描述。
///
/// # Parameters
/// 参数
/// `store` owns the custom manifest directory.
/// `store` 持有自定义清单目录。
///
/// # Returns
/// 返回值
/// Every standalone provider keyed by identifier, or a controlled failure.
/// 返回按标识索引的全部独立供应商，或受控失败。
pub fn load_all_custom_provider_manifests(
    store: &ManifestStoreLayout,
) -> Result<BTreeMap<String, ProviderDescriptor>, String> {
    let mut providers = BTreeMap::new();
    for provider_id in store.list_custom_manifest_ids()? {
        let text = store
            .read_custom_manifest(&provider_id)?
            .ok_or_else(|| format!("Standalone custom manifest for `{provider_id}` vanished."))?;
        let document = parse_document(&provider_id, &text)?.document;
        let descriptor = validate_document(&provider_id, &document)?;
        providers.insert(provider_id, descriptor);
    }
    Ok(providers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use vulcan_provider_adapters::{
        AccountUsageQueryKind, AuthenticationProjection, CredentialBillingKind,
        CredentialDirectoryVisibility, CredentialMode, CredentialProfile,
        CredentialRegistrationKind, ModelDiscoveryKind, ProtocolRoute, ProviderCategory,
        ProviderKind, RequestProjectorKind, ServiceEntry, ServiceEntryKind,
    };

    /// Immutable provider identity embedded in the checked-in descriptor-only v0 fixture.
    /// 签入的仅描述符 v0 夹具中嵌入的不可变供应商标识。
    const LEGACY_DESCRIPTOR_FIXTURE_PROVIDER_ID: &str = "custom:fixture-provider";

    /// Exact descriptor-only bytes emitted by the historical standalone writer.
    /// 历史独立写入器曾输出的精确仅描述符字节。
    const LEGACY_DESCRIPTOR_FIXTURE: &str =
        include_str!("../tests/fixtures/custom-provider-descriptor-v0.json");

    /// Builds one isolated store layout inside a temporary directory.
    /// 在临时目录内构建一个隔离的存储布局。
    fn layout() -> (ManifestStoreLayout, tempfile::TempDir) {
        let directory = tempfile::tempdir().expect("temp dir should create");
        let store = ManifestStoreLayout::new(directory.path());
        (store, directory)
    }

    /// Builds one minimal custom provider descriptor for migration tests.
    /// 构建一个用于迁移测试的最小自定义供应商描述。
    fn custom_descriptor(provider_id: &str) -> ProviderDescriptor {
        ProviderDescriptor {
            id: provider_id.to_string(),
            display_name: format!("Provider {provider_id}"),
            kind: ProviderKind::Custom,
            category: ProviderCategory::Unclassified,
            credential_directory_visibility: CredentialDirectoryVisibility::ConfiguredOnly,
            credential_registration: CredentialRegistrationKind::Direct,
            promotion: None,
            service_entries: vec![ServiceEntry {
                id: String::from("default"),
                display_name: String::from("Compatible API"),
                kind: ServiceEntryKind::Conversation,
                base_url: String::from("https://example.test/v1"),
                credential_profiles: vec![CredentialProfile {
                    id: String::from("api-key"),
                    display_name: String::from("API Key"),
                    credential_mode: CredentialMode::Token,
                    authorization: None,
                    billing_kind: CredentialBillingKind::ProviderManaged,
                    account_usage_query: AccountUsageQueryKind::Unsupported,
                    authentication: None,
                    key_prefix: None,
                    credential_role: None,
                    default_usage_scopes: Default::default(),
                }],
                authentication: AuthenticationProjection::BearerAuthorization,
                request_headers: BTreeMap::new(),
                service_origin_override_policy: None,
                native_inference_bindings: Vec::new(),
                model_discovery: ModelDiscoveryKind::OpenAi,
                request_projector: RequestProjectorKind::Direct,
                protocol_routes: vec![ProtocolRoute {
                    protocol_profile_id: String::from("openai-responses.v1"),
                    endpoint_path: Some(String::from("/responses")),
                    base_url_override: None,
                    request_headers: BTreeMap::new(),
                    authentication: None,
                    model_discovery: Some(ModelDiscoveryKind::OpenAi),
                    model_discovery_path: None,
                    model_discovery_base_url_override: None,
                    responses_features: None,
                    input_resource_constraints: Vec::new(),
                    usage_extraction_profiles: Vec::new(),
                    request_projection_profiles: Vec::new(),
                    response_projection_profiles: Vec::new(),
                    provider_request_id_headers: Vec::new(),
                    verified: true,
                }],
                default_usage_scopes: BTreeSet::new(),
            }],
        }
    }

    /// Materializes the checked-in descriptor-only legacy fixture for one exact provider id.
    /// 为一个精确供应商标识实例化签入的仅描述符旧版夹具。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` replaces the fixture identity in both the id and display-name fields.
    /// `provider_id` 会替换夹具的标识字段与显示名称字段。
    ///
    /// # Returns
    /// 返回值
    /// Historical JSON bytes independent of the current Rust descriptor defaults.
    /// 返回不依赖当前 Rust 描述默认值的历史 JSON 字节。
    fn legacy_descriptor_text(provider_id: &str) -> String {
        assert!(
            provider_id.starts_with("custom:"),
            "legacy fixture identity must remain custom-owned"
        );
        assert_eq!(
            LEGACY_DESCRIPTOR_FIXTURE
                .matches(LEGACY_DESCRIPTOR_FIXTURE_PROVIDER_ID)
                .count(),
            2,
            "legacy fixture must contain only its id and derived display name"
        );
        LEGACY_DESCRIPTOR_FIXTURE.replace(LEGACY_DESCRIPTOR_FIXTURE_PROVIDER_ID, provider_id)
    }

    /// Verifies the checked-in descriptor-only v0 fixture migrates without current defaults.
    /// 验证签入的仅描述符 v0 夹具无需依赖当前默认值即可迁移。
    #[test]
    fn checked_in_descriptor_only_v0_fixture_migrates_to_full_document() {
        let provider_id = "custom:historical";
        let parsed = parse_document(provider_id, legacy_descriptor_text(provider_id).as_str())
            .expect("historical descriptor fixture should migrate");
        assert!(parsed.legacy_descriptor_v0);
        let document = parsed.document;

        assert_eq!(
            document.schema_version,
            CUSTOM_PROVIDER_MANIFEST_SCHEMA_VERSION
        );
        assert_eq!(document.provider_manifest.provider_id, provider_id);
        assert_eq!(
            document.local_overrides,
            CustomProviderLocalOverridesV1::default()
        );
        assert_eq!(
            validate_document(provider_id, &document)
                .expect("migrated full document should validate")
                .display_name,
            "Provider custom:historical"
        );
    }

    /// Builds one explicit manual model used by full-document ownership tests.
    /// 构建一份供完整文档所有权测试使用的显式手工模型。
    fn custom_model(model_id: &str) -> CustomModelDefinition {
        CustomModelDefinition {
            id: model_id.to_string(),
            display_name: format!("Display {model_id}"),
            enabled: true,
            capabilities: vulcan_provider_adapters::ModelCapabilities::default(),
            request_projection: vulcan_provider_adapters::CustomModelRequestProjection::default(),
        }
    }

    /// Verifies the complete standalone schema round-trips every provider-owned override exactly.
    /// 验证完整独立 Schema 对每项供应商自有覆盖执行精确往返。
    #[test]
    fn standalone_custom_manifest_contains_full_manifest_and_owned_overrides() {
        let provider_id = "custom:round-trip";
        let model_id = "model-a";
        let model_key = PersistedAiConfiguration::provider_model_key(provider_id, model_id);
        let execution_key = PersistedAiConfiguration::provider_model_protocol_key(
            provider_id,
            model_id,
            vulcan_provider_adapters::CustomProviderProtocol::OpenAiResponses.profile_id(),
        );
        let mut configuration = PersistedAiConfiguration::default();
        let descriptor = custom_descriptor(provider_id);
        configuration
            .custom_providers
            .insert(provider_id.to_string(), descriptor.clone());
        configuration.custom_models.insert(
            provider_id.to_string(),
            BTreeMap::from([(model_id.to_string(), custom_model(model_id))]),
        );
        configuration.custom_model_protocol_preferences.insert(
            model_key.clone(),
            CustomModelProtocolPreference::Explicit {
                protocol: vulcan_provider_adapters::CustomProviderProtocol::OpenAiResponses,
            },
        );
        configuration
            .model_enabled_overrides
            .insert(model_key.clone(), false);
        configuration.model_overrides.insert(
            execution_key.clone(),
            ModelExecutionOverrides {
                output_limit: Some(2048),
                ..ModelExecutionOverrides::default()
            },
        );

        let document = document_from_configuration(&configuration, provider_id, &descriptor)
            .expect("complete document should build");
        let serialized = serialize_document(&document).expect("complete document should serialize");
        let parsed = parse_document(provider_id, serialized.as_str())
            .expect("complete document should parse");

        assert!(!parsed.legacy_descriptor_v0);
        assert_eq!(&parsed.document, &document);
        assert!(
            !parsed.document.provider_manifest.usage_profiles.is_empty(),
            "standalone custom manifests must materialize standard Usage profiles"
        );
        assert!(
            !parsed.document.provider_manifest.services[0].protocol_routes[0]
                .usage_profile_refs
                .is_empty()
                && parsed.document.provider_manifest.services[0].protocol_routes[0]
                    .usage_profile_refs
                    .iter()
                    .all(|profile_id| !profile_id.trim().is_empty()),
            "each compatible protocol route must persist explicit Usage profile refs"
        );
        assert!(
            parsed
                .document
                .provider_manifest
                .services
                .iter()
                .flat_map(|service| {
                    service
                        .model_catalog
                        .as_ref()
                        .and_then(|catalog| catalog.models.as_ref())
                        .into_iter()
                        .flatten()
                })
                .all(|model| model.model_id != model_id),
            "manual models must have exactly one persisted owner in local_overrides"
        );
        assert!(
            parsed
                .document
                .local_overrides
                .custom_models
                .contains_key(model_id),
            "the local override must retain the manual model definition"
        );
        assert_eq!(
            parsed
                .document
                .local_overrides
                .model_protocol_preferences
                .get(model_key.as_str()),
            Some(&CustomModelProtocolPreference::Explicit {
                protocol: vulcan_provider_adapters::CustomProviderProtocol::OpenAiResponses,
            })
        );
        assert_eq!(
            parsed
                .document
                .local_overrides
                .model_execution_overrides
                .get(execution_key.as_str())
                .and_then(|overrides| overrides.output_limit),
            Some(2048)
        );
    }

    /// Verifies a UI-owned descriptor edit preserves advanced per-protocol Usage profile refs.
    /// 验证 UI 持有的描述符编辑会保留高级协议级用量 Profile 引用。
    #[test]
    fn descriptor_edit_preserves_existing_protocol_usage_profiles() {
        let provider_id = "custom:profile-preservation";
        let descriptor = custom_descriptor(provider_id);
        let mut existing_manifest =
            vulcan_provider_adapters::project_custom_descriptor_manifest(&descriptor)
                .expect("baseline custom manifest should build");
        let mut custom_declaration = existing_manifest
            .usage_profiles
            .first()
            .cloned()
            .expect("standard Usage declaration should exist");
        custom_declaration.profile_id = String::from("custom.responses.usage.v7");
        custom_declaration
            .payload
            .as_mut()
            .expect("Usage payload should exist")
            .profile_id = custom_declaration.profile_id.clone();
        existing_manifest.usage_profiles = vec![custom_declaration.clone()];
        existing_manifest.services[0].protocol_routes[0].usage_profile_refs =
            vec![custom_declaration.profile_id.clone()];
        let mut configuration = PersistedAiConfiguration::default();
        configuration
            .custom_providers
            .insert(provider_id.to_string(), descriptor.clone());
        configuration
            .custom_provider_manifests
            .insert(provider_id.to_string(), existing_manifest);

        let document = document_from_configuration(&configuration, provider_id, &descriptor)
            .expect("edited custom document should preserve advanced profiles");

        assert_eq!(
            document.provider_manifest.services[0].protocol_routes[0].usage_profile_refs,
            vec![String::from("custom.responses.usage.v7")]
        );
        assert!(
            document
                .provider_manifest
                .usage_profiles
                .iter()
                .any(|declaration| declaration.profile_id == "custom.responses.usage.v7")
        );
        assert!(
            document.provider_manifest.usage_profiles.len() > 1,
            "standard declarations remain available for later protocol additions"
        );
    }

    /// Verifies a changed custom Manifest cannot reuse the terminal unsigned revision.
    /// 验证发生变化的自定义 Manifest 不能复用无符号修订终值。
    #[test]
    fn custom_manifest_revision_exhaustion_fails_closed() {
        let provider_id = "custom:revision-exhaustion";
        let descriptor = custom_descriptor(provider_id);
        let mut existing_manifest =
            vulcan_provider_adapters::project_custom_descriptor_manifest(&descriptor)
                .expect("baseline custom manifest should build");
        existing_manifest.revision = u64::MAX;
        existing_manifest.display.name = String::from("Outdated display name");
        let mut configuration = PersistedAiConfiguration::default();
        configuration
            .custom_providers
            .insert(provider_id.to_string(), descriptor.clone());
        configuration
            .custom_provider_manifests
            .insert(provider_id.to_string(), existing_manifest);

        let error = document_from_configuration(&configuration, provider_id, &descriptor)
            .expect_err("revision exhaustion must reject changed content");

        assert!(error.contains("cannot advance beyond revision"), "{error}");
    }

    /// Verifies central persistence drops provider-owned definitions while retaining connection-owned state.
    /// 验证中央持久化移除供应商自有定义，同时保留连接自有状态。
    #[test]
    fn central_configuration_does_not_persist_custom_provider_owned_definition() {
        let provider_id = "custom:projection";
        let model_id = "model-a";
        let model_key = PersistedAiConfiguration::provider_model_key(provider_id, model_id);
        let mut configuration = PersistedAiConfiguration::default();
        configuration
            .custom_providers
            .insert(provider_id.to_string(), custom_descriptor(provider_id));
        configuration.custom_models.insert(
            provider_id.to_string(),
            BTreeMap::from([(model_id.to_string(), custom_model(model_id))]),
        );
        configuration
            .model_enabled_overrides
            .insert(model_key, false);
        configuration
            .custom_model_connection_enabled_overrides
            .insert(String::from("connection-owned-key"), true);

        let projected = project_persisted_configuration(&configuration);

        assert!(projected.custom_providers.is_empty());
        assert!(projected.custom_models.is_empty());
        assert!(projected.model_enabled_overrides.is_empty());
        assert_eq!(
            projected
                .custom_model_connection_enabled_overrides
                .get("connection-owned-key"),
            Some(&true)
        );
        assert!(projected.custom_provider_file_refs.contains(provider_id));
    }

    /// Verifies three standalone providers hydrate without leaking model preferences across files.
    /// 验证三份独立供应商文件补水时不会跨文件泄漏模型偏好。
    #[test]
    fn three_complete_custom_documents_hydrate_in_isolation() {
        let (store, _temporary) = layout();
        let mut source = PersistedAiConfiguration::default();
        for suffix in ["alpha", "beta", "gamma"] {
            let provider_id = format!("custom:{suffix}");
            let model_id = format!("model-{suffix}");
            let model_key = PersistedAiConfiguration::provider_model_key(
                provider_id.as_str(),
                model_id.as_str(),
            );
            source
                .custom_providers
                .insert(provider_id.clone(), custom_descriptor(provider_id.as_str()));
            source.custom_models.insert(
                provider_id.clone(),
                BTreeMap::from([(model_id.clone(), custom_model(model_id.as_str()))]),
            );
            source.custom_model_protocol_preferences.insert(
                model_key,
                CustomModelProtocolPreference::Explicit {
                    protocol: vulcan_provider_adapters::CustomProviderProtocol::OpenAiChat,
                },
            );
        }
        sync_custom_provider_manifests(&store, &source)
            .expect("three complete manifests should persist");

        // hydrated starts with only standalone manifest references, matching a post-restart central configuration.
        // hydrated 初始仅包含独立清单引用，用于模拟重启后的中央配置。
        let mut hydrated = PersistedAiConfiguration {
            custom_provider_file_refs: source.custom_providers.keys().cloned().collect(),
            ..PersistedAiConfiguration::default()
        };
        load_custom_provider_manifests(&store, &mut hydrated)
            .expect("three complete manifests should hydrate");

        assert_eq!(hydrated.custom_providers.len(), 3);
        assert_eq!(hydrated.custom_provider_manifests.len(), 3);
        for suffix in ["alpha", "beta", "gamma"] {
            let provider_id = format!("custom:{suffix}");
            let model_id = format!("model-{suffix}");
            let model_key = PersistedAiConfiguration::provider_model_key(
                provider_id.as_str(),
                model_id.as_str(),
            );
            assert_eq!(
                hydrated
                    .custom_models
                    .get(provider_id.as_str())
                    .map(BTreeMap::len),
                Some(1)
            );
            assert!(
                hydrated
                    .custom_model_protocol_preferences
                    .contains_key(model_key.as_str())
            );
        }
    }

    /// Verifies a malformed full document is isolated without cross-file or embedded fallback.
    /// 验证损坏的完整文档会被隔离，且不会跨文件或回退到内嵌定义。
    #[test]
    fn malformed_custom_document_isolates_exact_provider() {
        let (store, _temporary) = layout();
        store
            .write_custom_manifest("custom:alpha", br#"{"schema_version":"broken"}"#)
            .expect("malformed alpha file should write");
        let mut configuration = PersistedAiConfiguration::default();
        configuration.custom_providers.insert(
            String::from("custom:alpha"),
            custom_descriptor("custom:alpha"),
        );
        configuration
            .custom_provider_file_refs
            .insert(String::from("custom:alpha"));

        let report = load_custom_provider_manifests(&store, &mut configuration)
            .expect("malformed provider must be isolated without failing healthy configuration");

        assert_eq!(report.failure_total_count, 1);
        assert_eq!(report.failures.len(), 1);
        assert_eq!(report.failures[0].provider_id, "custom:alpha");
        assert!(report.failures[0].message.contains("custom:alpha"));
        assert!(configuration.custom_providers.is_empty());
        assert!(
            configuration
                .custom_provider_file_refs
                .contains("custom:alpha")
        );
        assert_eq!(
            configuration.custom_provider_manifest_failure_total_count,
            1
        );
    }

    /// Verifies a malformed central reference fails before it can shadow a system provider.
    /// 验证损坏的中央引用会在其可能遮蔽系统供应商前失败。
    #[test]
    fn non_custom_central_reference_fails_without_mutating_configuration() {
        let (store, _temporary) = layout();
        let mut configuration = PersistedAiConfiguration::default();
        configuration
            .custom_provider_file_refs
            .insert(String::from("openai"));
        let before = configuration.clone();

        let error = load_custom_provider_manifests(&store, &mut configuration)
            .expect_err("a system-like standalone reference must fail closed");

        assert!(error.contains("custom:"), "{error}");
        assert_eq!(configuration, before);
    }

    /// Verifies isolated-provider diagnostics stay deterministically bounded under hostile input.
    /// 验证在恶意输入下，隔离供应商诊断仍保持确定性有界。
    #[test]
    fn custom_provider_hydration_failures_are_counted_and_bounded() {
        let (store, _temporary) = layout();
        let mut configuration = PersistedAiConfiguration::default();
        for index in 0..=CUSTOM_PROVIDER_HYDRATION_FAILURE_MAX_COUNT {
            configuration
                .custom_provider_file_refs
                .insert(format!("custom:missing-{index:03}"));
        }

        let report = load_custom_provider_manifests(&store, &mut configuration)
            .expect("missing providers should be isolated under a bounded report");

        assert_eq!(
            report.failure_total_count,
            (CUSTOM_PROVIDER_HYDRATION_FAILURE_MAX_COUNT + 1) as u64
        );
        assert_eq!(
            report.failures.len(),
            CUSTOM_PROVIDER_HYDRATION_FAILURE_MAX_COUNT
        );
        assert!(report.failures_truncated);
        assert_eq!(
            configuration.custom_provider_manifest_failure_total_count,
            report.failure_total_count
        );
        assert!(configuration.custom_provider_manifest_failures_truncated);
    }

    /// Verifies one corrupt file does not hide a healthy sibling or mutate the corrupt source.
    /// 验证一份损坏文件不会隐藏健康同级供应商，也不会修改损坏源。
    #[test]
    fn multi_provider_hydration_isolates_only_corrupt_provider() {
        let (store, _directory) = layout();
        let mut source = PersistedAiConfiguration::default();
        source.custom_providers.insert(
            String::from("custom:alpha"),
            custom_descriptor("custom:alpha"),
        );
        sync_custom_provider_manifests(&store, &source)
            .expect("the valid first provider should persist");
        store
            .write_custom_manifest("custom:beta", br#"{"schema_version":"broken"}"#)
            .expect("the malformed second provider should persist as test input");
        let mut configuration = PersistedAiConfiguration {
            custom_provider_file_refs: BTreeSet::from([
                String::from("custom:alpha"),
                String::from("custom:beta"),
            ]),
            ..PersistedAiConfiguration::default()
        };
        let report = load_custom_provider_manifests(&store, &mut configuration)
            .expect("the malformed sibling must be isolated");

        assert_eq!(report.failure_total_count, 1);
        assert_eq!(report.failures[0].provider_id, "custom:beta");
        assert!(configuration.custom_providers.contains_key("custom:alpha"));
        assert!(!configuration.custom_providers.contains_key("custom:beta"));
        assert!(
            configuration
                .custom_provider_file_refs
                .contains("custom:alpha")
        );
        assert!(
            configuration
                .custom_provider_file_refs
                .contains("custom:beta")
        );
        assert_eq!(
            store
                .read_custom_manifest("custom:beta")
                .expect("corrupt provider should remain readable"),
            Some(String::from(r#"{"schema_version":"broken"}"#))
        );
    }

    /// Verifies sync writes new providers and removes files for deleted providers.
    /// 验证同步写入新供应商并删除已删除供应商的文件。
    #[test]
    fn sync_writes_new_and_removes_deleted_provider_files() {
        let (store, _directory) = layout();
        let mut configuration = PersistedAiConfiguration::default();
        configuration.custom_providers.insert(
            "custom:alpha".to_string(),
            custom_descriptor("custom:alpha"),
        );
        configuration
            .custom_provider_file_refs
            .insert("custom:beta".to_string());
        // beta is referenced but absent from memory; sync must remove its stale file.
        // beta 被引用但不在内存中；同步必须移除其过期文件。
        store
            .write_custom_manifest("custom:beta", br#"{"stale":true}"#)
            .expect("stale file should write");

        sync_custom_provider_manifests(&store, &configuration).expect("sync should succeed");
        assert!(
            store
                .read_custom_manifest("custom:alpha")
                .expect("alpha file should exist")
                .is_some()
        );
        assert!(
            store
                .read_custom_manifest("custom:beta")
                .expect("beta file should be gone")
                .is_none()
        );
    }

    /// Verifies hydration isolates a referenced but missing standalone file.
    /// 验证补水隔离被引用但缺失的独立文件。
    #[test]
    fn hydration_isolates_missing_referenced_file() {
        let (store, _directory) = layout();
        let mut configuration = PersistedAiConfiguration::default();
        configuration
            .custom_provider_file_refs
            .insert("custom:ghost".to_string());
        let report = load_custom_provider_manifests(&store, &mut configuration)
            .expect("missing referenced file should be isolated");
        assert_eq!(report.failure_total_count, 1);
        assert!(report.failures[0].message.contains("missing"));
        assert!(
            configuration
                .custom_provider_file_refs
                .contains("custom:ghost")
        );
        assert!(!configuration.custom_providers.contains_key("custom:ghost"));
    }

    /// Verifies standalone manifests load all files without touching configuration state.
    /// 验证独立清单可在不触碰配置状态的情况下加载全部文件。
    #[test]
    fn standalone_manifests_load_all_files_directly() {
        let (store, _directory) = layout();
        store
            .write_custom_manifest(
                "custom:alpha",
                legacy_descriptor_text("custom:alpha").as_bytes(),
            )
            .expect("alpha should write");
        store
            .write_custom_manifest(
                "custom:beta",
                legacy_descriptor_text("custom:beta").as_bytes(),
            )
            .expect("beta should write");
        let providers =
            load_all_custom_provider_manifests(&store).expect("standalone manifests should load");
        assert_eq!(providers.len(), 2);
        assert_eq!(
            providers.keys().cloned().collect::<Vec<_>>(),
            vec![String::from("custom:alpha"), String::from("custom:beta")]
        );
    }

    /// Verifies projection retains an unresolved standalone authority beside healthy providers.
    /// 验证投影会在健康供应商旁保留一项尚未解析的独立权威。
    #[test]
    fn projected_configuration_preserves_isolated_custom_reference() {
        let mut configuration = PersistedAiConfiguration::default();
        configuration
            .custom_provider_file_refs
            .insert(String::from("custom:isolated"));
        configuration.custom_providers.insert(
            String::from("custom:active"),
            custom_descriptor("custom:active"),
        );

        let projected = project_persisted_configuration(&configuration);

        assert_eq!(
            projected.custom_provider_file_refs,
            std::collections::BTreeSet::from([
                String::from("custom:active"),
                String::from("custom:isolated"),
            ])
        );
        assert!(projected.custom_providers.is_empty());
    }

    /// Verifies the first migration before-image is immutable and owns one matching marker.
    /// 验证首次迁移前镜像不可变，并且只对应一份匹配标记。
    #[test]
    fn custom_provider_migration_backup_is_retained_and_marker_is_digest_bound() {
        let (store, directory) = layout();
        let central_path = directory.path().join("ai-connections.toml");
        let before_central = "legacy-central-exact\n";
        let before_manifest = legacy_descriptor_text("custom:before");
        std::fs::write(&central_path, before_central)
            .expect("legacy central configuration should write");
        store
            .write_custom_manifest("custom:before", before_manifest.as_bytes())
            .expect("legacy standalone manifest should write");

        let digest = ensure_custom_provider_migration_backup(&store, Some(before_central))
            .expect("first migration backup should persist");
        let backup_path = custom_provider_migration_backup_path(&store);
        let before_backup_bytes = std::fs::read(&backup_path).expect("backup should read");
        let backup: CustomProviderMigrationBackup =
            serde_json::from_slice(&before_backup_bytes).expect("backup should decode");
        assert_eq!(
            backup.central_configuration.as_deref(),
            Some(before_central)
        );
        assert_eq!(
            backup.custom_manifests.get("custom:before"),
            Some(&before_manifest)
        );

        std::fs::write(&central_path, "current-central\n")
            .expect("current central configuration should write");
        store
            .write_custom_manifest(
                "custom:before",
                legacy_descriptor_text("custom:changed").as_bytes(),
            )
            .expect("changed standalone manifest should write");
        let repeated_digest =
            ensure_custom_provider_migration_backup(&store, Some("current-central\n"))
                .expect("existing backup should validate");
        assert_eq!(repeated_digest, digest);
        assert_eq!(
            std::fs::read(&backup_path).expect("immutable backup should read"),
            before_backup_bytes
        );

        mark_custom_provider_migration_complete(&store, digest.as_str())
            .expect("matching completion marker should persist");
        mark_custom_provider_migration_complete(&store, digest.as_str())
            .expect("matching completion marker should be idempotent");
        let conflicting_digest = "0".repeat(64);
        let error = mark_custom_provider_migration_complete(&store, conflicting_digest.as_str())
            .expect_err("conflicting completion marker must fail closed");
        assert!(error.contains("different immutable backup"));
    }

    /// Verifies a synchronous failure before the central write restores the exact before image.
    /// 验证中央文件写入前的同步失败会恢复精确的保存前镜像。
    #[test]
    fn custom_provider_transaction_failure_restores_exact_before_image() {
        let (store, directory) = layout();
        let central_path = directory.path().join("ai-connections.toml");
        let before_central = "before-central-exact\n";
        let before_manifest = legacy_descriptor_text("custom:before");
        store
            .write_custom_manifest("custom:before", before_manifest.as_bytes())
            .expect("before manifest should write");
        std::fs::write(&central_path, before_central)
            .expect("before central configuration should write");

        let mut target_configuration = PersistedAiConfiguration::default();
        target_configuration.custom_providers.insert(
            String::from("custom:target"),
            custom_descriptor("custom:target"),
        );
        let in_memory_before = target_configuration.clone();
        let error = persist_custom_provider_transaction_with_before_central_write(
            &store,
            &central_path,
            &target_configuration,
            b"target-central\n",
            || {
                assert_eq!(
                    store
                        .list_custom_manifest_ids()
                        .expect("target manifest ids should list"),
                    vec![String::from("custom:target")],
                    "failure boundary must run after the complete target manifest image"
                );
                Err(String::from("injected failure before central write"))
            },
        )
        .expect_err("injected transaction failure must surface");

        assert!(
            error.contains("injected failure before central write"),
            "{error}"
        );
        assert_eq!(
            std::fs::read_to_string(&central_path).expect("before central should remain readable"),
            before_central
        );
        assert_eq!(
            read_all_custom_manifest_texts(&store).expect("rolled-back manifests should read"),
            BTreeMap::from([(String::from("custom:before"), before_manifest)])
        );
        assert_eq!(target_configuration, in_memory_before);
        assert!(!custom_provider_transaction_journal_path(&store).exists());
    }

    /// Verifies startup recovery completes the target image of an interrupted custom transaction.
    /// 验证启动恢复会完成一项中断自定义事务的 target 镜像。
    #[test]
    fn custom_provider_transaction_recovery_completes_target_image() {
        let (store, directory) = layout();
        let central_path = directory.path().join("ai-connections.toml");
        store
            .write_custom_manifest(
                "custom:before",
                legacy_descriptor_text("custom:before").as_bytes(),
            )
            .expect("before manifest should write");
        std::fs::write(&central_path, "before").expect("before central configuration should write");
        let journal = CustomProviderTransactionJournal {
            schema_version: 1,
            before_configuration: Some(String::from("before")),
            target_configuration: String::from("target"),
            before_manifests: BTreeMap::from([(
                String::from("custom:before"),
                legacy_descriptor_text("custom:before"),
            )]),
            target_manifests: BTreeMap::from([(
                String::from("custom:target"),
                legacy_descriptor_text("custom:target"),
            )]),
        };
        write_custom_provider_transaction_journal(&store, &journal).expect("journal should write");
        store
            .write_custom_manifest(
                "custom:target",
                legacy_descriptor_text("custom:target").as_bytes(),
            )
            .expect("partial target write should succeed");

        recover_custom_provider_transaction(&store, &central_path)
            .expect("recovery should complete");

        assert_eq!(
            store
                .list_custom_manifest_ids()
                .expect("manifest ids should list"),
            vec![String::from("custom:target")]
        );
        assert_eq!(
            std::fs::read_to_string(&central_path).expect("target central should read"),
            "target"
        );
        assert!(!custom_provider_transaction_journal_path(&store).exists());
    }
}
