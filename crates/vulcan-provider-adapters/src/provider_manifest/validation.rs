//! Pure manifest validator: structure, semantic, and engine-contract checks.
//! 纯 Manifest 验证器：结构、语义与引擎契约检查。
//!
//! The validator never touches the network, the disk, or secrets. Every engine fact is injected
//! through `EnginePrimitiveRegistry`; every ownership fact through `ManifestOwnership`.
//! 验证器绝不访问网络、磁盘或 Secret。所有引擎事实通过 `EnginePrimitiveRegistry` 注入，
//! 所有权事实通过 `ManifestOwnership` 注入。

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;
use time::{OffsetDateTime, UtcOffset, format_description::well_known::Rfc3339};
use vulcan_ai_contract::{AiOperation, ResourceKind};

use super::{
    EnginePrimitiveRegistry, FORBIDDEN_MANIFEST_HEADERS, MANIFEST_SCHEMA_VERSION,
    MAX_ASYNC_POLL_OR_BACKOFF_MS, MAX_ASYNC_TIMEOUT_MS, MAX_FIXED_HEADERS, MAX_JSON_DEPTH,
    MAX_JSON_NODES, MAX_MANIFEST_BYTES, MAX_MATCHER_VALUE_LEN, MAX_MIME_TYPES, MAX_MODEL_RULES,
    MAX_PROTOCOL_ROUTES, MAX_PROVIDER_IDENTIFIER_LEN, MAX_REQUEST_ID_HEADERS, MAX_SERVICES,
    MAX_STRING_LEN, MIN_ASYNC_POLL_INTERVAL_MS, ManifestValidationError,
    PROTECTED_REQUEST_BODY_ROOTS, ProviderManifestV1, UnknownModelPolicy,
};
use crate::ModelDiscoveryKind;
use crate::provider_manifest::schema::{
    ManifestEntryKind, ManifestProtocolRoute, ModelRule, ModelSourcePolicy,
};

/// Hard maximum bytes accepted by one declared upload channel.
/// 单条声明上传通道允许接受的最大字节数硬上限。
const MAX_UPLOAD_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// Hard maximum item count accepted by one upload channel.
/// 单条上传通道允许接受的最大条目数硬上限。
const MAX_UPLOAD_ITEM_COUNT: u64 = 1_024;
/// Hard maximum fixed form fields attached to one upload request.
/// 单次上传请求可附加的固定表单字段硬上限。
const MAX_UPLOAD_FIXED_FIELDS: usize = 32;
/// Hard maximum process-local reuse lifetime for one provider upload reference.
/// 单个供应商上传引用在进程内复用的最长硬期限。
const MAX_UPLOAD_CACHE_DURATION_MS: u64 = 24 * 60 * 60 * 1_000;
/// Hard maximum requirement identifiers accepted by each engine-contract dimension.
/// 每个引擎契约维度允许声明的依赖标识硬上限。
const MAX_ENGINE_CONTRACT_REQUIREMENTS: usize = 64;
/// Sensitive response headers that may never be reclassified as provider request identifiers.
/// 绝不能被重新归类为供应商请求标识的敏感响应头。
const FORBIDDEN_PROVIDER_REQUEST_ID_RESPONSE_HEADERS: &[&str] = &[
    "authentication-info",
    "cookie",
    "proxy-authentication-info",
    "set-cookie",
    "www-authenticate",
];

/// Ownership class of one manifest document.
/// 单份清单文档的所有权类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestOwnership {
    /// Vulcan-managed system provider manifest.
    /// Vulcan 管理的系统供应商清单。
    System,
    /// User-owned custom provider manifest.
    /// 用户自有的自定义供应商清单。
    Custom,
}

/// Parses, bounds-checks, and validates one manifest document.
/// 解析、边界检查并验证一份清单文档。
///
/// # Parameters
/// 参数
/// `text` is the exact UTF-8 document; `ownership` and `registry` are injected facts.
/// `text` 是精确 UTF-8 文档；`ownership` 与 `registry` 是注入事实。
///
/// # Returns
/// 返回值
/// The validated manifest, or every detected failure.
/// 返回已验证清单，或全部检测到的失败。
pub fn parse_and_validate(
    text: &str,
    ownership: ManifestOwnership,
    registry: &EnginePrimitiveRegistry,
) -> Result<ProviderManifestV1, Vec<ManifestValidationError>> {
    let mut errors = Vec::new();

    if text.len() > MAX_MANIFEST_BYTES {
        errors.push(ManifestValidationError::new(
            "manifest_too_large",
            "$",
            format!("manifest exceeds the {} byte limit", MAX_MANIFEST_BYTES),
        ));
        return Err(errors);
    }

    let value: Value = serde_json::from_str(text).map_err(|error| {
        vec![ManifestValidationError::new(
            "invalid_json",
            "$",
            format!("manifest is not valid JSON: {error}"),
        )]
    })?;

    let mut node_count = 0_usize;
    if let Err(count_errors) = check_bounds(&value, 0, &mut node_count) {
        errors.extend(count_errors);
        return Err(errors);
    }

    if let Err(null_errors) = validate_capability_patch_nulls(&value) {
        errors.extend(null_errors);
        return Err(errors);
    }

    let manifest: ProviderManifestV1 = serde_json::from_value(value).map_err(|error| {
        vec![ManifestValidationError::new(
            "invalid_manifest_structure",
            "$",
            format!("manifest structure rejected: {error}"),
        )]
    })?;

    validate_manifest(&manifest, ownership, registry)?;
    Ok(manifest)
}

/// Rejects `null` on non-nullable capability patch fields before deserialization.
/// 在反序列化前拒绝非可空能力补丁字段上的 `null`。
///
/// Nullable fields (`embedding`, `rerank`, `context_limit`, `output_limit`) accept `null` as
/// Clear; every other patch field rejects `null` so authors cannot confuse missing with
/// explicit null (plan §6.4.2).
/// 可空字段（`embedding`、`rerank`、`context_limit`、`output_limit`）接受 `null` 作为 Clear；
/// 其余补丁字段拒绝 `null`，避免作者把缺失与显式 null 混淆（方案 §6.4.2）。
fn validate_capability_patch_nulls(value: &Value) -> Result<(), Vec<ManifestValidationError>> {
    const NULLABLE: &[&str] = &["embedding", "rerank", "context_limit", "output_limit"];
    let mut errors = Vec::new();
    let mut visit = |node: &Value, base: String| {
        let Some(object) = node.as_object() else {
            return;
        };
        for (field, field_value) in object {
            if field_value.is_null() && !NULLABLE.contains(&field.as_str()) {
                errors.push(ManifestValidationError::new(
                    "capability_null_not_allowed",
                    format!("{base}.{field}"),
                    format!(
                        "capability field `{field}` is not nullable; omit the field to inherit"
                    ),
                ));
            }
        }
    };
    if let Some(defaults) = value.pointer("/model_defaults/capabilities") {
        visit(defaults, String::from("$.model_defaults.capabilities"));
    }
    if let Some(rules) = value.pointer("/model_rules").and_then(Value::as_array) {
        for (index, rule) in rules.iter().enumerate() {
            if let Some(capabilities) = rule.get("capabilities") {
                visit(capabilities, format!("$.model_rules[{index}].capabilities"));
            }
            if let Some(protocol_map) = rule.get("protocol_capabilities").and_then(Value::as_object)
            {
                for (protocol, capabilities) in protocol_map {
                    visit(
                        capabilities,
                        format!("$.model_rules[{index}].protocol_capabilities.{protocol}"),
                    );
                }
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Validates an already parsed manifest document.
/// 验证一份已解析的清单文档。
pub fn validate_manifest(
    manifest: &ProviderManifestV1,
    ownership: ManifestOwnership,
    registry: &EnginePrimitiveRegistry,
) -> Result<(), Vec<ManifestValidationError>> {
    let mut errors = Vec::new();
    let expanded_manifest = match super::compiler::expand_tool_target_templates(manifest) {
        Ok(expanded) => expanded.manifest,
        Err(template_errors) => {
            errors.extend(template_errors);
            manifest.clone()
        }
    };
    let manifest = &expanded_manifest;
    let path = |suffix: &str| format!("$.{suffix}");

    if !manifest.has_current_schema() {
        errors.push(ManifestValidationError::new(
            "unsupported_schema_version",
            path("schema_version"),
            format!(
                "expected schema version `{MANIFEST_SCHEMA_VERSION}`, found `{}`",
                manifest.schema_version
            ),
        ));
    }

    validate_provider_identifier(&manifest.provider_id, ownership, &mut errors);
    if manifest.provider_id.is_empty() {
        errors.push(ManifestValidationError::new(
            "empty_provider_id",
            path("provider_id"),
            "provider id must not be empty",
        ));
    }
    if ownership == ManifestOwnership::System && manifest.provider_id.starts_with("custom:") {
        errors.push(ManifestValidationError::new(
            "system_provider_cannot_use_custom_prefix",
            path("provider_id"),
            "system manifest must not use the `custom:` identifier prefix",
        ));
    }
    if ownership == ManifestOwnership::Custom && !manifest.provider_id.starts_with("custom:") {
        errors.push(ManifestValidationError::new(
            "custom_provider_requires_custom_prefix",
            path("provider_id"),
            "custom manifest provider id must use the `custom:` prefix",
        ));
    }
    if ownership == ManifestOwnership::Custom
        && manifest.credential_registration != crate::CredentialRegistrationKind::Direct
    {
        errors.push(ManifestValidationError::new(
            "custom_provider_requires_direct_credential_registration",
            path("credential_registration"),
            "custom manifests must use direct credential registration",
        ));
    }
    if manifest.revision == 0 {
        errors.push(ManifestValidationError::new(
            "revision_must_be_positive",
            path("revision"),
            "manifest revision must be greater than zero",
        ));
    }
    if manifest.published_at.trim().is_empty() {
        errors.push(ManifestValidationError::new(
            "empty_published_at",
            path("published_at"),
            "published_at must not be empty",
        ));
    } else if !is_valid_utc_rfc3339(manifest.published_at.as_str()) {
        errors.push(ManifestValidationError::new(
            "invalid_published_at",
            path("published_at"),
            "published_at must be an exact RFC 3339 UTC timestamp ending in Z",
        ));
    }

    validate_engine_contract(manifest, registry, &mut errors);
    validate_display(manifest, &mut errors);
    validate_manifest_provenance(manifest, &mut errors);

    if manifest.services.is_empty() {
        errors.push(ManifestValidationError::new(
            "empty_services",
            path("services"),
            "manifest must declare at least one service entry",
        ));
    } else if manifest.services.len() > MAX_SERVICES {
        errors.push(ManifestValidationError::new(
            "too_many_services",
            path("services"),
            format!("manifest exceeds the {} service limit", MAX_SERVICES),
        ));
    }

    let mut service_ids = BTreeSet::new();
    for (index, entry) in manifest.services.iter().enumerate() {
        validate_service_entry(
            manifest,
            entry,
            ownership,
            registry,
            &mut service_ids,
            &mut errors,
            index,
        );
    }

    validate_model_defaults(manifest, registry, &mut errors);
    validate_model_rules(manifest, registry, &mut errors);
    validate_declaration_uniqueness(manifest, registry, &mut errors);
    validate_projection_profiles(manifest, &mut errors);
    validate_referenced_profile_payloads(manifest, &mut errors);
    validate_usage_profile_payloads(manifest, &mut errors);
    validate_operation_closure(manifest, ownership, registry, &mut errors);

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Validates the engine contract against the injected primitive registry.
/// 对照注入的原语注册表验证引擎契约。
fn validate_engine_contract(
    manifest: &ProviderManifestV1,
    registry: &EnginePrimitiveRegistry,
    errors: &mut Vec<ManifestValidationError>,
) {
    let contract = &manifest.engine_contract;
    validate_engine_requirement_list(
        contract.required_protocol_profiles.as_slice(),
        "$.engine_contract.required_protocol_profiles",
        "protocol profile",
        errors,
    );
    validate_engine_requirement_list(
        contract.required_execution_primitives.as_slice(),
        "$.engine_contract.required_execution_primitives",
        "execution primitive",
        errors,
    );
    validate_engine_requirement_list(
        contract.required_upload_primitives.as_slice(),
        "$.engine_contract.required_upload_primitives",
        "upload primitive",
        errors,
    );
    validate_engine_requirement_list(
        contract.required_transformers.as_slice(),
        "$.engine_contract.required_transformers",
        "transformer",
        errors,
    );
    if contract.minimum_manifest_engine > super::CURRENT_MANIFEST_ENGINE {
        errors.push(ManifestValidationError::new(
            "unsupported_engine_contract",
            "engine_contract.minimum_manifest_engine",
            format!(
                "manifest requires manifest engine {} but this host supports {}",
                contract.minimum_manifest_engine,
                super::CURRENT_MANIFEST_ENGINE
            ),
        ));
    }
    let missing = |required: &[String], registered: &BTreeSet<String>| {
        required
            .iter()
            .take(MAX_ENGINE_CONTRACT_REQUIREMENTS)
            .filter(|id| !registered.contains(*id))
            .cloned()
            .collect::<Vec<_>>()
    };
    for id in missing(
        &contract.required_protocol_profiles,
        &registry.protocol_profiles,
    ) {
        errors.push(ManifestValidationError::new(
            "unsupported_engine_contract",
            "engine_contract.required_protocol_profiles",
            format!("protocol profile `{id}` is not registered by this engine"),
        ));
    }
    for id in missing(
        &contract.required_execution_primitives,
        &registry.execution_primitives,
    ) {
        errors.push(ManifestValidationError::new(
            "unsupported_engine_contract",
            "engine_contract.required_execution_primitives",
            format!("execution primitive `{id}` is not registered by this engine"),
        ));
    }
    for id in missing(
        &contract.required_upload_primitives,
        &registry.upload_primitives,
    ) {
        errors.push(ManifestValidationError::new(
            "unsupported_engine_contract",
            "engine_contract.required_upload_primitives",
            format!("upload primitive `{id}` is not registered by this engine"),
        ));
    }
    for id in missing(&contract.required_transformers, &registry.transformers) {
        errors.push(ManifestValidationError::new(
            "unsupported_engine_contract",
            "engine_contract.required_transformers",
            format!("transformer `{id}` is not registered by this engine"),
        ));
    }
    let required_protocols = contract
        .required_protocol_profiles
        .iter()
        .take(MAX_ENGINE_CONTRACT_REQUIREMENTS)
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for (service_index, service) in manifest.services.iter().enumerate() {
        for (route_index, route) in service.protocol_routes.iter().enumerate() {
            if !required_protocols.contains(route.protocol_profile_id.as_str()) {
                errors.push(ManifestValidationError::new(
                    "engine_contract_missing_used_protocol",
                    format!(
                        "$.services[{service_index}].protocol_routes[{route_index}].protocol_profile_id"
                    ),
                    format!(
                        "used protocol profile `{}` is absent from engine_contract.required_protocol_profiles",
                        route.protocol_profile_id
                    ),
                ));
            }
        }
    }

    let required_execution = contract
        .required_execution_primitives
        .iter()
        .take(MAX_ENGINE_CONTRACT_REQUIREMENTS)
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for (target_index, target) in manifest.tool_targets.iter().enumerate() {
        let primitive = provider_tool_execution_primitive_id(target.execution.execution_kind);
        if !required_execution.contains(primitive) {
            errors.push(ManifestValidationError::new(
                "engine_contract_missing_used_execution_primitive",
                format!("$.tool_targets[{target_index}].execution.execution_kind"),
                format!(
                    "used execution primitive `{primitive}` is absent from engine_contract.required_execution_primitives"
                ),
            ));
        }
    }
    for (template_index, template) in manifest.tool_target_templates.iter().enumerate() {
        let primitive = provider_tool_execution_primitive_id(template.execution.execution_kind);
        if !required_execution.contains(primitive) {
            errors.push(ManifestValidationError::new(
                "engine_contract_missing_used_execution_primitive",
                format!(
                    "$.tool_target_templates[{template_index}].execution.execution_kind"
                ),
                format!(
                    "used execution primitive `{primitive}` is absent from engine_contract.required_execution_primitives"
                ),
            ));
        }
    }

    let required_transformers = contract
        .required_transformers
        .iter()
        .take(MAX_ENGINE_CONTRACT_REQUIREMENTS)
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for (profile_index, declaration) in manifest.request_profiles.iter().enumerate() {
        if let Some(payload) = declaration.payload.as_ref() {
            validate_mapping_transformer_contract(
                payload.steps.as_slice(),
                &required_transformers,
                format!("$.request_profiles[{profile_index}].payload.steps").as_str(),
                errors,
            );
        }
    }
    for (profile_index, declaration) in manifest.response_profiles.iter().enumerate() {
        if let Some(payload) = declaration.payload.as_ref() {
            validate_mapping_transformer_contract(
                payload.steps.as_slice(),
                &required_transformers,
                format!("$.response_profiles[{profile_index}].payload.steps").as_str(),
                errors,
            );
        }
    }
}

/// Validates one bounded, unique engine-contract requirement list.
/// 验证一组有界、唯一的引擎契约依赖声明。
///
/// # Parameters
/// 参数
/// `required` owns untrusted identifiers, `path` and `label` describe the exact contract
/// dimension, and `errors` collects deterministic validation failures.
/// `required` 持有不受信任的标识，`path` 与 `label` 描述精确契约维度，`errors` 收集确定性校验失败。
fn validate_engine_requirement_list(
    required: &[String],
    path: &str,
    label: &str,
    errors: &mut Vec<ManifestValidationError>,
) {
    if required.len() > MAX_ENGINE_CONTRACT_REQUIREMENTS {
        errors.push(ManifestValidationError::new(
            "too_many_engine_contract_requirements",
            path,
            format!(
                "{label} requirements exceed the {MAX_ENGINE_CONTRACT_REQUIREMENTS} item limit"
            ),
        ));
    }
    let mut seen = BTreeSet::new();
    for (index, identifier) in required
        .iter()
        .take(MAX_ENGINE_CONTRACT_REQUIREMENTS)
        .enumerate()
    {
        let identifier_path = format!("{path}[{index}]");
        validate_identifier(identifier, identifier_path.as_str(), errors, label);
        if !seen.insert(identifier.as_str()) {
            errors.push(ManifestValidationError::new(
                "duplicate_engine_contract_requirement",
                identifier_path,
                format!("{label} `{identifier}` is duplicated"),
            ));
        }
    }
}

/// Validates that every used mapping step is declared by the manifest engine contract.
/// 验证每个已使用映射步骤都由 Manifest 引擎契约声明。
///
/// # Parameters
/// 参数
/// `steps` is one closed projection program, `required_transformers` is the declared dependency
/// set, `path` scopes diagnostics, and `errors` collects every missing declaration.
/// `steps` 是一段封闭投影程序，`required_transformers` 是已声明依赖集合，`path` 限定诊断，
/// `errors` 收集每个缺失声明。
fn validate_mapping_transformer_contract(
    steps: &[crate::provider_manifest::MappingStep],
    required_transformers: &BTreeSet<&str>,
    path: &str,
    errors: &mut Vec<ManifestValidationError>,
) {
    for (step_index, step) in steps.iter().enumerate() {
        let transformer = step.transformer_id();
        if !required_transformers.contains(transformer) {
            errors.push(ManifestValidationError::new(
                "engine_contract_missing_used_transformer",
                format!("{path}[{step_index}].op"),
                format!(
                    "used transformer `{transformer}` is absent from engine_contract.required_transformers"
                ),
            ));
        }
    }
}

/// Returns the stable engine-contract identifier for one provider-tool execution family.
/// 返回一个供应商工具执行族的稳定引擎契约标识。
///
/// # Parameters
/// 参数
/// `kind` is the closed runtime execution enum.
/// `kind` 是封闭运行时执行枚举。
///
/// # Returns
/// 返回值
/// The exact snake-case primitive identifier shared by the registry and manifests.
/// 返回注册表与 Manifest 共用的精确蛇形命名原语标识。
fn provider_tool_execution_primitive_id(kind: crate::ProviderToolExecutionKind) -> &'static str {
    match kind {
        crate::ProviderToolExecutionKind::HttpJsonSync => "http_json_sync",
        crate::ProviderToolExecutionKind::HttpBinarySync => "http_binary_sync",
        crate::ProviderToolExecutionKind::ConversationMediaAnalyze => "conversation_media_analyze",
        crate::ProviderToolExecutionKind::HttpJsonAsyncPoll => "http_json_async_poll",
        crate::ProviderToolExecutionKind::McpHttpCall => "mcp_http_call",
    }
}

/// Validates the display block.
/// 验证展示块。
fn validate_display(manifest: &ProviderManifestV1, errors: &mut Vec<ManifestValidationError>) {
    if manifest.display.name.trim().is_empty() {
        errors.push(ManifestValidationError::new(
            "empty_display_name",
            "display.name",
            "display name must not be empty",
        ));
    }
    if let Some(promotion) = &manifest.display.promotion {
        validate_promotion_url(
            &promotion.signup_url,
            "display.promotion.signup_url",
            errors,
        );
        if let Some(referral) = &promotion.referral_url {
            validate_promotion_url(referral, "display.promotion.referral_url", errors);
        }
    }
}

/// Validates whole-manifest evidence sources and verification metadata.
/// 校验整份 Manifest 的证据来源与验证元数据。
///
/// # Parameters
/// 参数
/// `manifest` owns the untrusted provenance declaration and `errors` collects deterministic
/// validation failures.
/// `manifest` 持有不受信任的来源声明，`errors` 收集确定性验证失败。
fn validate_manifest_provenance(
    manifest: &ProviderManifestV1,
    errors: &mut Vec<ManifestValidationError>,
) {
    const MAX_PROVENANCE_SOURCES: usize = 64;
    let provenance = &manifest.provenance;
    if provenance.sources.len() > MAX_PROVENANCE_SOURCES {
        errors.push(ManifestValidationError::new(
            "too_many_provenance_sources",
            "$.provenance.sources",
            format!("a manifest may declare at most {MAX_PROVENANCE_SOURCES} evidence sources"),
        ));
    }
    if provenance.status == super::ProvenanceStatus::Verified && provenance.sources.is_empty() {
        errors.push(ManifestValidationError::new(
            "verified_manifest_requires_evidence_source",
            "$.provenance.sources",
            "verified provenance requires at least one exact evidence source",
        ));
    }
    let mut source_urls = BTreeSet::new();
    for (index, source) in provenance.sources.iter().enumerate() {
        let source_path = format!("$.provenance.sources[{index}]");
        if !is_safe_provenance_source(source.url.as_str()) {
            errors.push(ManifestValidationError::new(
                "invalid_provenance_source",
                format!("{source_path}.url"),
                "provenance source must be a credential-free HTTPS URL or bounded builtin: URI",
            ));
        } else if !source_urls.insert(source.url.as_str()) {
            errors.push(ManifestValidationError::new(
                "duplicate_provenance_source",
                format!("{source_path}.url"),
                format!("provenance source `{}` is duplicated", source.url),
            ));
        }
        if source.note.trim().is_empty()
            || source.note.len() > MAX_STRING_LEN
            || source.note.chars().any(char::is_control)
        {
            errors.push(ManifestValidationError::new(
                "invalid_provenance_note",
                format!("{source_path}.note"),
                "provenance source note must be non-empty, bounded, and control-free",
            ));
        }
    }
    if provenance
        .verified_at
        .as_deref()
        .is_some_and(|verified_at| !is_valid_utc_rfc3339(verified_at))
    {
        errors.push(ManifestValidationError::new(
            "invalid_provenance_verified_at",
            "$.provenance.verified_at",
            "provenance verified_at must be an exact RFC 3339 UTC timestamp ending in Z",
        ));
    }
}

/// Reports whether one audit timestamp is a bounded RFC 3339 UTC instant using the canonical Z suffix.
/// 报告一个审计时间戳是否为有界、使用标准 Z 后缀的 RFC 3339 UTC 时刻。
///
/// # Parameters
/// 参数
/// `value` is the untrusted timestamp text.
/// `value` 是不受信任的时间戳文本。
///
/// # Returns
/// 返回值
/// `true` only when parsing succeeds, the offset is UTC, and the source uses the exact Z suffix.
/// 仅当解析成功、偏移为 UTC 且原文使用精确 Z 后缀时返回 `true`。
fn is_valid_utc_rfc3339(value: &str) -> bool {
    value.len() <= 64
        && value.is_ascii()
        && value.ends_with('Z')
        && OffsetDateTime::parse(value, &Rfc3339)
            .is_ok_and(|timestamp| timestamp.offset() == UtcOffset::UTC)
}

/// Reports whether one provenance source uses the closed HTTPS or builtin URI policy.
/// 报告一个来源证据是否使用封闭的 HTTPS 或 builtin URI 策略。
///
/// # Parameters
/// 参数
/// `source` is the untrusted evidence locator.
/// `source` 是不受信任的证据定位符。
///
/// # Returns
/// 返回值
/// `true` only for a bounded credential-free HTTPS URL or stable builtin identifier.
/// 仅对有界、无凭据的 HTTPS URL 或稳定 builtin 标识返回 `true`。
fn is_safe_provenance_source(source: &str) -> bool {
    if source.is_empty() || source.len() > MAX_STRING_LEN || source.chars().any(char::is_control) {
        return false;
    }
    if let Some(identifier) = source.strip_prefix("builtin:") {
        return !identifier.is_empty()
            && identifier.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
            });
    }
    reqwest::Url::parse(source).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str().is_some()
            && url.port().is_none()
            && url.username().is_empty()
            && url.password().is_none()
    })
}

/// Validates one optional default/rule evidence reference against top-level sources.
/// 依据顶层来源校验一个可选默认层或规则证据引用。
///
/// # Parameters
/// 参数
/// `manifest` owns the evidence table, `reference` is the exact optional locator, `path` owns the
/// diagnostic location, and `errors` collects closure failures.
/// `manifest` 持有证据表，`reference` 是精确可选定位符，`path` 持有诊断位置，`errors` 收集
/// 闭包失败。
fn validate_provenance_reference(
    manifest: &ProviderManifestV1,
    reference: Option<&str>,
    path: &str,
    errors: &mut Vec<ManifestValidationError>,
) {
    let Some(reference) = reference else {
        return;
    };
    if !manifest
        .provenance
        .sources
        .iter()
        .any(|source| source.url == reference)
    {
        errors.push(ManifestValidationError::new(
            "dangling_provenance_reference",
            path,
            format!("provenance reference `{reference}` is not declared by $.provenance.sources"),
        ));
    }
}

/// Validates one service entry and its routes.
/// 验证单个服务入口及其路由。
#[allow(clippy::too_many_arguments)]
fn validate_service_entry(
    manifest: &ProviderManifestV1,
    entry: &crate::provider_manifest::schema::ManifestServiceEntry,
    ownership: ManifestOwnership,
    registry: &EnginePrimitiveRegistry,
    service_ids: &mut BTreeSet<String>,
    errors: &mut Vec<ManifestValidationError>,
    index: usize,
) {
    let path = |suffix: &str| format!("$.services[{index}]{suffix}");
    validate_identifier(
        &entry.service_entry_id,
        &path(""),
        errors,
        "service_entry_id",
    );
    if !service_ids.insert(entry.service_entry_id.clone()) {
        errors.push(ManifestValidationError::new(
            "duplicate_service_entry_id",
            path(".service_entry_id"),
            format!("duplicate service entry id `{}`", entry.service_entry_id),
        ));
    }
    if entry.display_name.trim().is_empty() {
        errors.push(ManifestValidationError::new(
            "empty_entry_display_name",
            path(".display_name"),
            "service entry display name must not be empty",
        ));
    }

    let require_https = matches!(ownership, ManifestOwnership::System);
    validate_http_url(&entry.base_url, &path(".base_url"), errors, require_https);

    let mut profile_ids = BTreeSet::new();
    for (profile_index, profile) in entry.credential_profiles.iter().enumerate() {
        validate_identifier(
            &profile.credential_profile_id,
            &format!("$.services[{index}].credential_profiles[{profile_index}]"),
            errors,
            "credential_profile_id",
        );
        if !profile_ids.insert(profile.credential_profile_id.clone()) {
            errors.push(ManifestValidationError::new(
                "duplicate_credential_profile_id",
                format!(
                    "$.services[{index}].credential_profiles[{profile_index}].credential_profile_id"
                ),
                format!(
                    "duplicate credential profile id `{}`",
                    profile.credential_profile_id
                ),
            ));
        }
        if let Some(credential_role) = profile.credential_role.as_deref() {
            validate_identifier(
                credential_role,
                &format!(
                    "$.services[{index}].credential_profiles[{profile_index}].credential_role"
                ),
                errors,
                "credential_role",
            );
        }
    }

    validate_fixed_headers(
        &entry.request_headers,
        &path(".request_headers"),
        errors,
        "service",
    );

    if let Some(policy) = entry.service_origin_override_policy.as_ref() {
        let policy_path = path(".service_origin_override_policy");
        if !policy.host_suffix.starts_with('.')
            || policy.host_suffix.len() < 4
            || policy.host_suffix.bytes().any(|byte| {
                !(byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-'))
            })
        {
            errors.push(ManifestValidationError::new(
                "invalid_service_origin_host_suffix",
                format!("{policy_path}.host_suffix"),
                "service origin host suffix must be a lowercase DNS suffix beginning with `.`",
            ));
        }
        if !policy.allowed_input_paths.contains("") {
            errors.push(ManifestValidationError::new(
                "service_origin_missing_canonical_path",
                format!("{policy_path}.allowed_input_paths"),
                "service origin policy must accept the empty canonical-origin path",
            ));
        }
        for allowed_path in &policy.allowed_input_paths {
            if !allowed_path.is_empty()
                && (!allowed_path.starts_with('/')
                    || allowed_path.ends_with('/')
                    || allowed_path.contains('?')
                    || allowed_path.contains('#'))
            {
                errors.push(ManifestValidationError::new(
                    "invalid_service_origin_path",
                    format!("{policy_path}.allowed_input_paths"),
                    format!("service origin path `{allowed_path}` is not canonical"),
                ));
            }
        }
        validate_http_url(
            &policy.example_origin,
            &format!("{policy_path}.example_origin"),
            errors,
            true,
        );
        for route in &entry.protocol_routes {
            match policy.protocol_base_paths.get(&route.protocol_profile_id) {
                Some(base_path) if policy.allowed_input_paths.contains(base_path) => {}
                Some(base_path) => errors.push(ManifestValidationError::new(
                    "service_origin_protocol_path_not_allowed",
                    format!("{policy_path}.protocol_base_paths"),
                    format!(
                        "protocol `{}` maps to undeclared path `{base_path}`",
                        route.protocol_profile_id
                    ),
                )),
                None => errors.push(ManifestValidationError::new(
                    "service_origin_protocol_path_missing",
                    format!("{policy_path}.protocol_base_paths"),
                    format!(
                        "protocol `{}` does not declare an origin override path",
                        route.protocol_profile_id
                    ),
                )),
            }
        }
    }

    let mut native_operations = BTreeSet::new();
    let mut native_binding_ids = BTreeSet::new();
    for (binding_index, binding) in entry.native_inference_bindings.iter().enumerate() {
        let binding_path = path(&format!(".native_inference_bindings[{binding_index}]"));
        if !matches!(
            binding.operation,
            vulcan_ai_contract::AiOperation::EmbeddingCreate
                | vulcan_ai_contract::AiOperation::RerankDocuments
        ) {
            errors.push(ManifestValidationError::new(
                "unsupported_native_inference_operation",
                format!("{binding_path}.operation"),
                "native inference binding supports only embedding_create or rerank_documents",
            ));
        }
        if !native_operations.insert(binding.operation) {
            errors.push(ManifestValidationError::new(
                "duplicate_native_inference_operation",
                format!("{binding_path}.operation"),
                format!(
                    "operation `{}` has more than one native binding",
                    binding.operation.as_str()
                ),
            ));
        }
        if binding.binding_id.trim().is_empty()
            || binding.binding_id.len() > 128
            || !native_binding_ids.insert(binding.binding_id.clone())
        {
            errors.push(ManifestValidationError::new(
                "invalid_native_inference_binding_id",
                format!("{binding_path}.binding_id"),
                "native inference binding id must be unique, non-empty, and at most 128 bytes",
            ));
        }
        if !binding.endpoint_path.starts_with('/')
            || binding.endpoint_path.contains('?')
            || binding.endpoint_path.contains('#')
        {
            errors.push(ManifestValidationError::new(
                "invalid_native_inference_endpoint",
                format!("{binding_path}.endpoint_path"),
                "native inference endpoint must be one relative absolute path without query or fragment",
            ));
        }
        match binding.operation {
            vulcan_ai_contract::AiOperation::EmbeddingCreate
                if binding.rerank_truncation.is_some() =>
            {
                errors.push(ManifestValidationError::new(
                    "embedding_binding_declares_rerank_truncation",
                    format!("{binding_path}.rerank_truncation"),
                    "embedding binding cannot declare rerank truncation",
                ));
            }
            vulcan_ai_contract::AiOperation::RerankDocuments
                if binding.rerank_truncation.is_none() =>
            {
                errors.push(ManifestValidationError::new(
                    "rerank_binding_missing_truncation",
                    format!("{binding_path}.rerank_truncation"),
                    "rerank binding must declare its exact truncation contract",
                ));
            }
            _ => {}
        }
        if binding.requires_service_origin_override {
            match (
                entry.service_origin_override_policy.as_ref(),
                binding.service_origin_base_path.as_ref(),
            ) {
                (Some(policy), Some(base_path))
                    if policy.allowed_input_paths.contains(base_path) => {}
                _ => errors.push(ManifestValidationError::new(
                    "native_binding_origin_contract_missing",
                    binding_path.clone(),
                    "origin-required native binding must reference an allowed service-origin base path",
                )),
            }
        } else if binding.service_origin_base_path.is_some() {
            errors.push(ManifestValidationError::new(
                "native_binding_unused_origin_path",
                format!("{binding_path}.service_origin_base_path"),
                "native binding cannot declare an origin base path unless the origin is required",
            ));
        }
        for (requested, aliases) in &binding.response_model_aliases {
            if requested.trim().is_empty()
                || aliases.is_empty()
                || aliases.iter().any(|alias| alias.trim().is_empty())
            {
                errors.push(ManifestValidationError::new(
                    "invalid_native_response_model_alias",
                    format!("{binding_path}.response_model_aliases"),
                    "native response model aliases require non-empty requested and reported identities",
                ));
            }
        }
    }

    if entry.kind == ManifestEntryKind::Conversation {
        if entry.protocol_routes.is_empty() {
            errors.push(ManifestValidationError::new(
                "conversation_entry_requires_route",
                path(".protocol_routes"),
                "conversation service entry must declare at least one protocol route",
            ));
        } else if entry.protocol_routes.len() > MAX_PROTOCOL_ROUTES {
            errors.push(ManifestValidationError::new(
                "too_many_protocol_routes",
                path(".protocol_routes"),
                format!(
                    "service entry exceeds the {} protocol route limit",
                    MAX_PROTOCOL_ROUTES
                ),
            ));
        }
        if entry.model_catalog.is_none() {
            errors.push(ManifestValidationError::new(
                "conversation_entry_requires_model_source_policy",
                path(".model_catalog"),
                "conversation service entry must declare an explicit model source policy",
            ));
        }
    } else if entry.model_catalog.is_some() {
        errors.push(ManifestValidationError::new(
            "provider_tools_entry_forbids_model_catalog",
            path(".model_catalog"),
            "provider-tools service entry cannot declare a conversation model catalog",
        ));
    }

    let mut route_protocols = BTreeSet::new();
    for (route_index, route) in entry.protocol_routes.iter().enumerate() {
        validate_protocol_route(
            manifest,
            entry,
            route,
            registry,
            &path(&format!(".protocol_routes[{route_index}]")),
            errors,
        );
        if !route_protocols.insert(route.protocol_profile_id.clone()) {
            errors.push(ManifestValidationError::new(
                "duplicate_protocol_route",
                format!("$.services[{index}].protocol_routes[{route_index}].protocol_profile_id"),
                format!("duplicate protocol route `{}`", route.protocol_profile_id),
            ));
        }
    }

    if let Some(catalog) = &entry.model_catalog {
        validate_model_catalog(manifest, catalog, entry, &path(".model_catalog"), errors);
    }

    if let Some(overlay) = &entry.model_defaults_override {
        if let Some(protocols) = &overlay.supported_protocols {
            for protocol in protocols {
                validate_registered_protocol(
                    protocol,
                    &format!("$.services[{index}].model_defaults_override.supported_protocols"),
                    errors,
                    registry,
                );
            }
        }
        if let Some(selected) = &overlay.selected_protocol {
            validate_registered_protocol(
                selected,
                &format!("$.services[{index}].model_defaults_override.selected_protocol"),
                errors,
                registry,
            );
        }
    }
}

/// Validates one protocol route declaration.
/// 验证单条协议路由声明。
fn validate_protocol_route(
    manifest: &ProviderManifestV1,
    entry: &crate::provider_manifest::schema::ManifestServiceEntry,
    route: &ManifestProtocolRoute,
    registry: &EnginePrimitiveRegistry,
    path: &str,
    errors: &mut Vec<ManifestValidationError>,
) {
    validate_registered_protocol(&route.protocol_profile_id, path, errors, registry);
    if let Some(endpoint) = &route.endpoint_path {
        validate_endpoint_path(endpoint, path, errors);
    }
    if let Some(base_url) = &route.base_url_override {
        validate_http_url(base_url, &format!("{path}.base_url_override"), errors, true);
    }
    if let Some(discovery_base) = &route.model_discovery_base_url_override {
        validate_http_url(
            discovery_base,
            &format!("{path}.model_discovery_base_url_override"),
            errors,
            true,
        );
    }
    if let Some(discovery_path) = &route.model_discovery_path {
        validate_endpoint_path(
            discovery_path,
            &format!("{path}.model_discovery_path"),
            errors,
        );
    }
    validate_fixed_headers(&route.request_headers, path, errors, "route");
    validate_input_resource_constraints(
        &route.input_resource_constraints,
        &format!("{path}.input_resource_constraints"),
        errors,
    );
    validate_input_resource_upload_refs(
        manifest,
        &route.input_resource_constraints,
        &format!("{path}.input_resource_constraints"),
        errors,
    );
    validate_provider_request_id_headers(&route.provider_request_id_headers, path, errors);
    validate_profile_refs(manifest, &route.usage_profile_refs, "usage", path, errors);
    validate_profile_refs(
        manifest,
        &route.request_profile_refs,
        "request",
        path,
        errors,
    );
    validate_profile_refs(
        manifest,
        &route.response_profile_refs,
        "response",
        path,
        errors,
    );
    if entry.kind == ManifestEntryKind::Conversation
        && route.responses_features.is_some()
        && route.protocol_profile_id != vulcan_ai_protocols::OPENAI_CODEX_RESPONSES_PROFILE_ID
    {
        errors.push(ManifestValidationError::new(
            "responses_features_on_non_codex_route",
            path,
            "Responses optional capabilities are only allowed on Codex Responses routes",
        ));
    }
}

/// Validates the bounded closed shape of one resource-constraint list.
/// 验证一组资源约束的有界封闭结构。
///
/// # Parameters
/// 参数
/// `constraints` is the exact declaration list, `path` identifies it, and `errors` receives
/// every deterministic validation failure.
/// `constraints` 是精确声明列表，`path` 标识该列表，`errors` 接收全部确定性校验失败。
fn validate_input_resource_constraints(
    constraints: &[crate::InputResourceConstraint],
    path: &str,
    errors: &mut Vec<ManifestValidationError>,
) {
    if constraints.len() > 4 {
        errors.push(ManifestValidationError::new(
            "too_many_input_resource_constraints",
            path,
            "a declaration can constrain at most the four registered resource kinds",
        ));
    }
    let mut constrained_resource_kinds = Vec::new();
    for (index, constraint) in constraints.iter().enumerate() {
        let constraint_path = format!("{path}[{index}]");
        if constrained_resource_kinds.contains(&constraint.resource_kind) {
            errors.push(ManifestValidationError::new(
                "duplicate_input_resource_constraint",
                format!("{constraint_path}.resource_kind"),
                format!(
                    "resource kind `{:?}` is constrained more than once",
                    constraint.resource_kind
                ),
            ));
        } else {
            constrained_resource_kinds.push(constraint.resource_kind);
        }
        if constraint.accepted_mime_types.len() > MAX_MIME_TYPES {
            errors.push(ManifestValidationError::new(
                "too_many_input_resource_mime_types",
                format!("{constraint_path}.accepted_mime_types"),
                format!("resource constraint exceeds the {MAX_MIME_TYPES} MIME type limit"),
            ));
        }
        let mut mime_types = BTreeSet::new();
        for (mime_index, mime_type) in constraint.accepted_mime_types.iter().enumerate() {
            if mime_type.trim().is_empty()
                || mime_type.chars().count() > MAX_STRING_LEN
                || mime_type.chars().any(char::is_control)
            {
                errors.push(ManifestValidationError::new(
                    "invalid_input_resource_mime_type",
                    format!("{constraint_path}.accepted_mime_types[{mime_index}]"),
                    format!(
                        "resource MIME type must be non-empty, control-free, and at most {MAX_STRING_LEN} characters"
                    ),
                ));
            } else if !mime_types.insert(mime_type.as_str()) {
                errors.push(ManifestValidationError::new(
                    "duplicate_input_resource_mime_type",
                    format!("{constraint_path}.accepted_mime_types[{mime_index}]"),
                    format!("resource MIME type `{mime_type}` is duplicated"),
                ));
            }
        }
        if constraint
            .max_bytes
            .is_some_and(|max_bytes| max_bytes == 0 || max_bytes > 1024 * 1024 * 1024)
        {
            errors.push(ManifestValidationError::new(
                "input_resource_constraint_requires_bounded_max_bytes",
                format!("{constraint_path}.max_bytes"),
                "input resource max_bytes must be within 1..=1 GiB",
            ));
        }
        if constraint
            .max_count
            .is_some_and(|max_count| max_count == 0 || max_count > 1024)
        {
            errors.push(ManifestValidationError::new(
                "input_resource_constraint_requires_bounded_max_count",
                format!("{constraint_path}.max_count"),
                "input resource max_count must be within 1..=1024",
            ));
        }
        if constraint.upload_channel_refs.len() > 16 {
            errors.push(ManifestValidationError::new(
                "too_many_input_resource_upload_references",
                format!("{constraint_path}.upload_channel_refs"),
                "an input resource may reference at most 16 upload channels",
            ));
        }
        let mut upload_refs = BTreeSet::new();
        for (reference_index, reference) in constraint.upload_channel_refs.iter().enumerate() {
            if reference.trim().is_empty() || !upload_refs.insert(reference.as_str()) {
                errors.push(ManifestValidationError::new(
                    "invalid_input_resource_upload_reference",
                    format!("{constraint_path}.upload_channel_refs[{reference_index}]"),
                    "input resource upload references must be non-empty and unique",
                ));
            }
        }
    }
}

/// Validates that every route/model resource upload reference resolves in the same manifest.
/// 校验每个路由或模型资源上传引用都能在同一 Manifest 内解析。
fn validate_input_resource_upload_refs(
    manifest: &ProviderManifestV1,
    constraints: &[crate::InputResourceConstraint],
    path: &str,
    errors: &mut Vec<ManifestValidationError>,
) {
    let known: BTreeSet<&str> = manifest
        .upload_channels
        .iter()
        .map(|channel| channel.upload_channel_id.as_str())
        .collect();
    for (constraint_index, constraint) in constraints.iter().enumerate() {
        for (reference_index, reference) in constraint.upload_channel_refs.iter().enumerate() {
            if !known.contains(reference.as_str()) {
                errors.push(ManifestValidationError::new(
                    "dangling_input_resource_upload_reference",
                    format!("{path}[{constraint_index}].upload_channel_refs[{reference_index}]"),
                    format!("upload channel reference `{reference}` is not declared"),
                ));
            }
        }
    }
}

/// Reports whether a model constraint preserves or narrows its provider route constraint.
/// 报告模型约束是否保持或收紧其供应商路由约束。
fn input_resource_constraint_not_expanded(
    candidate: &crate::InputResourceConstraint,
    bundled: &crate::InputResourceConstraint,
) -> bool {
    let mime_types_not_expanded = bundled.accepted_mime_types.is_empty()
        || (!candidate.accepted_mime_types.is_empty()
            && candidate
                .accepted_mime_types
                .iter()
                .all(|mime_type| bundled.accepted_mime_types.contains(mime_type)));
    let max_bytes_not_expanded = match (candidate.max_bytes, bundled.max_bytes) {
        (None, None) | (Some(_), None) => true,
        (None, Some(_)) => false,
        (Some(candidate), Some(bundled)) => candidate <= bundled,
    };
    let max_count_not_expanded = match (candidate.max_count, bundled.max_count) {
        (None, None) | (Some(_), None) => true,
        (None, Some(_)) => false,
        (Some(candidate), Some(bundled)) => candidate <= bundled,
    };
    let reference_policy_not_expanded = match bundled.reference_policy {
        crate::ResourceReferencePolicy::Any => true,
        crate::ResourceReferencePolicy::LocalOnly => {
            candidate.reference_policy == crate::ResourceReferencePolicy::LocalOnly
        }
        crate::ResourceReferencePolicy::RemoteOnly => {
            candidate.reference_policy == crate::ResourceReferencePolicy::RemoteOnly
        }
    };
    mime_types_not_expanded
        && candidate
            .upload_channel_refs
            .iter()
            .all(|reference| bundled.upload_channel_refs.contains(reference))
        && max_bytes_not_expanded
        && max_count_not_expanded
        && reference_policy_not_expanded
}

/// Validates one model-catalog declaration.
/// 验证单条模型目录声明。
fn validate_model_catalog(
    manifest: &ProviderManifestV1,
    catalog: &crate::provider_manifest::schema::ManifestModelCatalog,
    entry: &crate::provider_manifest::schema::ManifestServiceEntry,
    path: &str,
    errors: &mut Vec<ManifestValidationError>,
) {
    // entryDiscoveryKind mirrors compilation: a catalog default wins, otherwise the first
    // route-specific declaration becomes the inherited entry default.
    // entryDiscoveryKind 镜像编译语义：目录默认值优先，否则首个路由专属声明成为入口继承默认值。
    let entry_discovery_kind = catalog.discovery_kind.or_else(|| {
        entry
            .protocol_routes
            .iter()
            .find_map(|route| route.model_discovery)
    });
    // effectiveDiscoveryKinds verifies every route against the closed identity-source policy.
    // effectiveDiscoveryKinds 根据封闭身份来源策略校验每条路由。
    let effective_discovery_kinds = entry
        .protocol_routes
        .iter()
        .map(|route| route.model_discovery.or(entry_discovery_kind))
        .collect::<Vec<_>>();
    if catalog.source_policy.includes_discovery() {
        if effective_discovery_kinds.iter().any(Option::is_none) {
            errors.push(ManifestValidationError::new(
                "discovery_policy_requires_discovery_kind",
                path,
                "discovery-based source policy must resolve a discovery kind for every route",
            ));
        }
        for (route_index, discovery_kind) in effective_discovery_kinds.iter().enumerate() {
            if matches!(
                discovery_kind,
                Some(
                    ModelDiscoveryKind::Manual
                        | ModelDiscoveryKind::ProviderToolsOnly
                        | ModelDiscoveryKind::ManifestStatic
                )
            ) {
                errors.push(ManifestValidationError::new(
                    "discovery_policy_requires_remote_discovery_kind",
                    format!("{path}.discovery_kind"),
                    format!(
                        "discovery-based source policy resolves route {route_index} to a non-remote discovery kind"
                    ),
                ));
            }
        }
    } else {
        for (route_index, discovery_kind) in effective_discovery_kinds.iter().enumerate() {
            if discovery_kind.is_some_and(|kind| {
                !matches!(
                    kind,
                    ModelDiscoveryKind::Manual | ModelDiscoveryKind::ManifestStatic
                )
            }) {
                errors.push(ManifestValidationError::new(
                    "source_policy_forbids_remote_discovery",
                    format!("{path}.discovery_kind"),
                    format!(
                        "non-discovery source policy resolves route {route_index} to remote discovery"
                    ),
                ));
            }
        }
    }
    if catalog.models.is_some() && !catalog.source_policy.includes_manifest() {
        errors.push(ManifestValidationError::new(
            "catalog_models_require_manifest_source",
            format!("{path}.models"),
            "manifest catalog models require a source policy that includes manifest identity",
        ));
    }
    if catalog.source_policy == ModelSourcePolicy::ManifestOnly && catalog.models.is_none() {
        errors.push(ManifestValidationError::new(
            "manifest_only_requires_catalog_models",
            format!("{path}.models"),
            "manifest-only source policy requires an explicit complete model inventory",
        ));
    }
    // manifest_static_routes contains every route whose effective discovery authority is the
    // exact model inventory in this document.
    // manifest_static_routes 包含有效发现权威为本文档精确模型清单的每条路由。
    let manifest_static_routes = entry
        .protocol_routes
        .iter()
        .filter(|route| {
            route.model_discovery.or(catalog.discovery_kind)
                == Some(ModelDiscoveryKind::ManifestStatic)
        })
        .collect::<Vec<_>>();
    if !manifest_static_routes.is_empty() {
        // entry_path anchors route errors beside model_catalog instead of nesting a false path.
        // entry_path 把路由错误锚定在 model_catalog 同级，避免生成虚假嵌套路径。
        let entry_path = path.strip_suffix(".model_catalog").unwrap_or(path);
        if catalog.models.is_none() {
            errors.push(ManifestValidationError::new(
                "manifest_static_requires_catalog_models",
                format!("{path}.models"),
                "manifest_static discovery requires an explicit model inventory",
            ));
        }
        if catalog.path.is_some()
            && manifest_static_routes
                .iter()
                .any(|route| route.model_discovery.is_none())
        {
            errors.push(ManifestValidationError::new(
                "manifest_static_forbids_remote_discovery_location",
                format!("{path}.path"),
                "manifest_static discovery must not declare a remote discovery path",
            ));
        }
        for route in &manifest_static_routes {
            if route.model_discovery_path.is_some()
                || route.model_discovery_base_url_override.is_some()
            {
                // route_index identifies the exact declaration in the source service entry.
                // route_index 标识源服务入口中的精确声明。
                let route_index = entry
                    .protocol_routes
                    .iter()
                    .position(|candidate| std::ptr::eq(candidate, *route))
                    .expect("filtered manifest-static route must belong to its service entry");
                errors.push(ManifestValidationError::new(
                    "manifest_static_forbids_remote_discovery_location",
                    format!("{entry_path}.protocol_routes[{route_index}]"),
                    "manifest_static discovery must not declare a route-specific path or base URL",
                ));
            }
        }
    }
    if catalog.source_policy.includes_manifest()
        && manifest.model_defaults.unknown_model_policy == UnknownModelPolicy::DeclaredRuleRequired
        && let Some(models) = &catalog.models
    {
        for (model_index, model) in models.iter().enumerate() {
            for credential in &entry.credential_profiles {
                // covered proves every declared Manifest identity is executable for this exact
                // service and credential scope under declared_rule_required.
                // covered 证明在 declared_rule_required 下，每个已声明 Manifest 身份都可在
                // 此精确服务与凭据作用域执行。
                let covered = manifest.model_rules.iter().any(|rule| {
                    (rule.scope.service_entry_ids.is_empty()
                        || rule
                            .scope
                            .service_entry_ids
                            .contains(&entry.service_entry_id))
                        && (rule.scope.credential_profile_ids.is_empty()
                            || rule
                                .scope
                                .credential_profile_ids
                                .contains(&credential.credential_profile_id))
                        && rule.matcher.matches(model.model_id.as_str())
                });
                if !covered {
                    errors.push(ManifestValidationError::new(
                        "manifest_model_requires_rule",
                        format!("{path}.models[{model_index}].model_id"),
                        format!(
                            "Manifest model `{}` requires a rule covering service entry `{}` and credential profile `{}` under declared_rule_required",
                            model.model_id,
                            entry.service_entry_id,
                            credential.credential_profile_id
                        ),
                    ));
                }
            }
        }
    }
    if let Some(models) = &catalog.models {
        if models.len() > MAX_MODEL_RULES {
            errors.push(ManifestValidationError::new(
                "catalog_model_limit_exceeded",
                format!("{path}.models"),
                format!(
                    "manifest catalog exceeds the {} model identity limit",
                    MAX_MODEL_RULES
                ),
            ));
        }
        let mut model_ids = BTreeSet::new();
        for (index, model) in models.iter().enumerate() {
            let model_path = format!("{path}.models[{index}]");
            if model.model_id.trim().is_empty() {
                errors.push(ManifestValidationError::new(
                    "empty_catalog_model_id",
                    format!("{model_path}.model_id"),
                    "catalog model id must not be empty",
                ));
            } else if model.model_id.chars().count() > MAX_STRING_LEN {
                errors.push(ManifestValidationError::new(
                    "catalog_model_id_too_long",
                    format!("{model_path}.model_id"),
                    format!("catalog model id exceeds the {MAX_STRING_LEN} character limit"),
                ));
            }
            if !model_ids.insert(model.model_id.as_str()) {
                errors.push(ManifestValidationError::new(
                    "duplicate_catalog_model_id",
                    format!("{model_path}.model_id"),
                    format!("catalog model id `{}` is duplicated", model.model_id),
                ));
            }
            if model.display_name.trim().is_empty()
                || model.display_name.chars().count() > MAX_STRING_LEN
            {
                errors.push(ManifestValidationError::new(
                    "invalid_catalog_model_display_name",
                    format!("{model_path}.display_name"),
                    format!(
                        "catalog model display name must be non-empty and at most {MAX_STRING_LEN} characters"
                    ),
                ));
            }
        }
    }
}

/// Validates the provider-level model defaults.
/// 验证供应商级模型默认。
fn validate_model_defaults(
    manifest: &ProviderManifestV1,
    registry: &EnginePrimitiveRegistry,
    errors: &mut Vec<ManifestValidationError>,
) {
    let defaults = &manifest.model_defaults;
    let base = "$.model_defaults";
    for protocol in &defaults.supported_protocols {
        validate_registered_protocol(
            protocol,
            &format!("{base}.supported_protocols"),
            errors,
            registry,
        );
    }
    if let Some(selected) = &defaults.selected_protocol {
        validate_registered_protocol(
            selected,
            &format!("{base}.selected_protocol"),
            errors,
            registry,
        );
        if !defaults.supported_protocols.is_empty()
            && !defaults.supported_protocols.contains(selected)
        {
            errors.push(ManifestValidationError::new(
                "selected_protocol_not_supported",
                format!("{base}.selected_protocol"),
                "default selected protocol must belong to supported_protocols",
            ));
        }
    }
    if let Some(capabilities) = &defaults.capabilities {
        validate_capabilities(capabilities, base, errors);
    }
    if defaults.unknown_model_policy == UnknownModelPolicy::DefaultBindingAllowed {
        if defaults.capabilities.is_none() {
            errors.push(ManifestValidationError::new(
                "default_binding_allowed_requires_capabilities",
                format!("{base}.capabilities"),
                "default_binding_allowed requires an explicit default capability snapshot",
            ));
        }
        for (entry_index, entry) in manifest.services.iter().enumerate().filter(|(_, entry)| {
            entry.kind == crate::provider_manifest::schema::ManifestEntryKind::Conversation
        }) {
            let overlay = entry.model_defaults_override.as_ref();
            let supported_protocols = overlay
                .and_then(|overlay| overlay.supported_protocols.as_ref())
                .unwrap_or(&defaults.supported_protocols);
            let selected_protocol = overlay
                .and_then(|overlay| overlay.selected_protocol.as_ref())
                .or(defaults.selected_protocol.as_ref());
            let entry_base = format!("$.services[{entry_index}].model_defaults_override");
            if supported_protocols.is_empty() {
                errors.push(ManifestValidationError::new(
                    "default_binding_allowed_requires_supported_protocols",
                    format!("{entry_base}.supported_protocols"),
                    "default_binding_allowed requires a non-empty effective supported protocol set for every conversation entry",
                ));
            }
            let Some(selected_protocol) = selected_protocol else {
                errors.push(ManifestValidationError::new(
                    "default_binding_allowed_requires_selected_protocol",
                    format!("{entry_base}.selected_protocol"),
                    "default_binding_allowed requires an effective selected protocol for every conversation entry",
                ));
                continue;
            };
            if !supported_protocols.contains(selected_protocol) {
                errors.push(ManifestValidationError::new(
                    "selected_protocol_not_supported",
                    format!("{entry_base}.selected_protocol"),
                    "effective default selected protocol must belong to the entry's supported protocols",
                ));
            }
            match entry
                .protocol_routes
                .iter()
                .find(|route| route.protocol_profile_id == *selected_protocol)
            {
                None => errors.push(ManifestValidationError::new(
                    "default_protocol_missing_on_entry_route",
                    format!("{entry_base}.selected_protocol"),
                    format!(
                        "default protocol `{selected_protocol}` is not declared by service entry `{}`",
                        entry.service_entry_id
                    ),
                )),
                Some(route) if !route.verified => errors.push(ManifestValidationError::new(
                    "default_protocol_route_not_verified",
                    format!("{entry_base}.selected_protocol"),
                    format!(
                        "default protocol `{selected_protocol}` is not verified on service entry `{}`",
                        entry.service_entry_id
                    ),
                )),
                Some(_) => {}
            }
        }
    }
    validate_profile_refs(
        manifest,
        &defaults.default_usage_profile_refs,
        "usage",
        &format!("{base}.default_usage_profile_refs"),
        errors,
    );
    validate_profile_refs(
        manifest,
        &defaults.default_request_profile_refs,
        "request",
        &format!("{base}.default_request_profile_refs"),
        errors,
    );
    validate_profile_refs(
        manifest,
        &defaults.default_response_profile_refs,
        "response",
        &format!("{base}.default_response_profile_refs"),
        errors,
    );
    validate_operation_refs(
        manifest,
        &defaults.default_operation_refs,
        &format!("{base}.default_operation_refs"),
        errors,
    );
    validate_provenance_reference(
        manifest,
        defaults.provenance_ref.as_deref(),
        &format!("{base}.provenance_ref"),
        errors,
    );
}

/// Validates every model rule and its conflicts.
/// 验证全部模型规则及其冲突。
fn validate_model_rules(
    manifest: &ProviderManifestV1,
    registry: &EnginePrimitiveRegistry,
    errors: &mut Vec<ManifestValidationError>,
) {
    if manifest.model_rules.len() > MAX_MODEL_RULES {
        errors.push(ManifestValidationError::new(
            "too_many_model_rules",
            "$.model_rules",
            format!("manifest exceeds the {} model rule limit", MAX_MODEL_RULES),
        ));
        return;
    }

    let mut rule_ids = BTreeSet::new();
    for (index, rule) in manifest.model_rules.iter().enumerate() {
        validate_model_rule(manifest, rule, registry, index, errors);
        if !rule_ids.insert(rule.rule_id.clone()) {
            errors.push(ManifestValidationError::new(
                "duplicate_rule_id",
                format!("$.model_rules[{index}].rule_id"),
                format!("duplicate rule id `{}`", rule.rule_id),
            ));
        }
    }

    for (left_index, left) in manifest.model_rules.iter().enumerate() {
        for right in manifest.model_rules.iter().skip(left_index + 1) {
            if let Some(relation) = scoped_rule_conflict(manifest, left, right) {
                let (code, message) = match relation {
                    super::scope::ScopeRelation::Identical => (
                        "duplicate_rule_authority",
                        format!(
                            "rules `{}` and `{}` declare the identical target set at the same rank",
                            left.rule_id, right.rule_id
                        ),
                    ),
                    super::scope::ScopeRelation::Incomparable => (
                        "incomparable_rule_scopes",
                        format!(
                            "rules `{}` and `{}` overlap at the same rank without one being a strict subset",
                            left.rule_id, right.rule_id
                        ),
                    ),
                    _ => continue,
                };
                errors.push(ManifestValidationError::new(
                    code,
                    format!(
                        "$.model_rules[{}]",
                        manifest
                            .model_rules
                            .iter()
                            .position(|rule| rule.rule_id == right.rule_id)
                            .unwrap_or(0)
                    ),
                    message,
                ));
            }
        }
    }
}

/// Reports the deterministic conflict between two rules at the same precedence rank.
/// 报告两条规则在同一优先级下的确定性冲突。
///
/// The matcher must target the identical model set at the same rank; the scope is then
/// normalized to concrete target pairs so identical scopes are duplicate authority and
/// intersecting-but-incomparable scopes reject the whole manifest (plan §6.5.3/§6.5.5).
/// A strict subset/superset or disjoint pair never conflicts because the compiler orders
/// the first and the second cannot both cover one target pair.
/// 匹配器必须指向同一排名的相同模型集合；作用域随后归一化为具体目标组合，使完全相同
/// 作用域属于重复权威、相交但不可比较作用域拒绝整份清单（方案 §6.5.3/§6.5.5）。
/// 严格子集/超集与不相交组合永不冲突，因为编译器为前者排序、后者不可能同时覆盖
/// 同一目标组合。
fn scoped_rule_conflict(
    manifest: &ProviderManifestV1,
    left: &ModelRule,
    right: &ModelRule,
) -> Option<super::scope::ScopeRelation> {
    if !super::matcher::matchers_conflict(&left.matcher, &right.matcher) {
        return None;
    }
    let left_targets = super::scope::NormalizedRuleTargets::from_rule(manifest, left);
    let right_targets = super::scope::NormalizedRuleTargets::from_rule(manifest, right);
    match left_targets.relation(&right_targets) {
        super::scope::ScopeRelation::Identical => Some(super::scope::ScopeRelation::Identical),
        super::scope::ScopeRelation::Incomparable => {
            Some(super::scope::ScopeRelation::Incomparable)
        }
        super::scope::ScopeRelation::StrictSubset
        | super::scope::ScopeRelation::StrictSuperset
        | super::scope::ScopeRelation::Disjoint => None,
    }
}

/// Validates one model rule.
/// 验证单条模型规则。
fn validate_model_rule(
    manifest: &ProviderManifestV1,
    rule: &ModelRule,
    registry: &EnginePrimitiveRegistry,
    index: usize,
    errors: &mut Vec<ManifestValidationError>,
) {
    let path = |suffix: &str| format!("$.model_rules[{index}]{suffix}");
    validate_identifier(&rule.rule_id, &path(""), errors, "rule_id");
    validate_provenance_reference(
        manifest,
        rule.provenance_ref.as_deref(),
        &path(".provenance_ref"),
        errors,
    );
    if rule.matcher.value.trim().is_empty() {
        errors.push(ManifestValidationError::new(
            "empty_matcher_value",
            path(".matcher.value"),
            "rule matcher value must not be empty",
        ));
    } else if rule.matcher.value.chars().count() > MAX_MATCHER_VALUE_LEN {
        errors.push(ManifestValidationError::new(
            "matcher_value_too_long",
            path(".matcher.value"),
            format!(
                "rule matcher value exceeds the {} character limit",
                MAX_MATCHER_VALUE_LEN
            ),
        ));
    }
    if let Some(display_name) = &rule.display_name
        && (display_name.trim().is_empty() || display_name.chars().count() > 512)
    {
        errors.push(ManifestValidationError::new(
            "invalid_model_display_name",
            path(".display_name"),
            "model display name must contain between 1 and 512 characters",
        ));
    }
    validate_input_resource_constraints(
        &rule.input_resource_constraints,
        &path(".input_resource_constraints"),
        errors,
    );
    validate_input_resource_upload_refs(
        manifest,
        &rule.input_resource_constraints,
        &path(".input_resource_constraints"),
        errors,
    );

    for entry_id in &rule.scope.service_entry_ids {
        if !manifest
            .services
            .iter()
            .any(|entry| &entry.service_entry_id == entry_id)
        {
            errors.push(ManifestValidationError::new(
                "rule_scope_references_unknown_service",
                path(".scope.service_entry_ids"),
                format!("rule scope references unknown service entry `{entry_id}`"),
            ));
        }
    }
    for profile_id in &rule.scope.credential_profile_ids {
        // A scoped credential profile must be valid for every service entry the scope
        // covers; an entry-scoped rule may reference a profile another entry also declares,
        // but the covered entries are the authoritative acceptance set (plan §6.5.8).
        // 作用域内的凭据配置档必须对作用域覆盖的每个服务入口都有效；入口作用域规则
        // 可以引用其他入口也声明的配置档，但被覆盖的入口是权威接受集合（方案 §6.5.8）。
        let missing_entry = manifest
            .services
            .iter()
            .filter(|entry| rule.scope.covers_service_entry(&entry.service_entry_id))
            .filter(|entry| {
                !entry
                    .credential_profiles
                    .iter()
                    .any(|profile| &profile.credential_profile_id == profile_id)
            })
            .map(|entry| entry.service_entry_id.as_str())
            .next();
        if let Some(entry_id) = missing_entry {
            errors.push(ManifestValidationError::new(
                "rule_scope_references_unknown_credential",
                path(".scope.credential_profile_ids"),
                format!(
                    "rule scope references credential profile `{profile_id}` that service entry `{entry_id}` does not declare"
                ),
            ));
        }
    }

    for protocol in &rule.supported_protocols {
        validate_registered_protocol(protocol, &path(".supported_protocols"), errors, registry);
    }
    if let Some(selected) = &rule.selected_protocol {
        validate_registered_protocol(selected, &path(".selected_protocol"), errors, registry);
        if !rule.supported_protocols.is_empty() && !rule.supported_protocols.contains(selected) {
            errors.push(ManifestValidationError::new(
                "selected_protocol_not_supported",
                path(".selected_protocol"),
                "rule selected protocol must belong to supported_protocols",
            ));
        }
    }
    if rule.supported_protocols.is_empty() && rule.selected_protocol.is_none() {
        errors.push(ManifestValidationError::new(
            "rule_requires_protocol_declaration",
            path(""),
            "rule must declare supported_protocols or selected_protocol",
        ));
    }

    if let Some(capabilities) = &rule.capabilities {
        validate_capabilities(capabilities, &path(".capabilities"), errors);
    }
    for (protocol, capabilities) in &rule.protocol_capabilities {
        validate_registered_protocol(protocol, &path(".protocol_capabilities"), errors, registry);
        validate_capabilities(capabilities, &path(".protocol_capabilities"), errors);
    }

    validate_profile_refs(
        manifest,
        rule.usage_profile_refs.as_deref().unwrap_or_default(),
        "usage",
        &path(".usage_profile_refs"),
        errors,
    );
    validate_profile_refs(
        manifest,
        rule.request_profile_refs.as_deref().unwrap_or_default(),
        "request",
        &path(".request_profile_refs"),
        errors,
    );
    validate_profile_refs(
        manifest,
        rule.response_profile_refs.as_deref().unwrap_or_default(),
        "response",
        &path(".response_profile_refs"),
        errors,
    );
    validate_operation_refs(
        manifest,
        rule.operation_refs.as_deref().unwrap_or_default(),
        &path(".operation_refs"),
        errors,
    );

    if rule.allowed_account_plan_names.len() > 64 {
        errors.push(ManifestValidationError::new(
            "too_many_account_plan_names",
            path(".allowed_account_plan_names"),
            "rule account-plan allowlist exceeds 64 entries",
        ));
    }
    let mut account_plan_names = BTreeSet::new();
    for plan_name in &rule.allowed_account_plan_names {
        if plan_name.trim().is_empty() || plan_name.len() > 256 {
            errors.push(ManifestValidationError::new(
                "invalid_account_plan_name",
                path(".allowed_account_plan_names"),
                "account-plan names must contain between 1 and 256 UTF-8 bytes",
            ));
        } else if !account_plan_names.insert(plan_name.as_str()) {
            errors.push(ManifestValidationError::new(
                "duplicate_account_plan_name",
                path(".allowed_account_plan_names"),
                format!("duplicate account-plan name `{plan_name}`"),
            ));
        }
    }

    let mut parameter_ids = BTreeSet::new();
    for (parameter_index, parameter) in rule.model_parameters.iter().enumerate() {
        if let Err(error) = parameter.validate_definition() {
            errors.push(ManifestValidationError::new(
                "invalid_model_parameter_definition",
                path(format!(".model_parameters[{parameter_index}]").as_str()),
                error,
            ));
        }
        if !parameter_ids.insert(parameter.id.clone()) {
            errors.push(ManifestValidationError::new(
                "duplicate_model_parameter",
                path(".model_parameters"),
                format!("duplicate model parameter id `{}`", parameter.id),
            ));
        }
    }

    // Compile-time route presence: the selected protocol must exist on every entry covered by
    // the rule scope; unscoped rules are validated against every service entry.
    // 编译期路由存在性：所选协议必须存在于规则作用域覆盖的每个入口；
    // 未加作用域的规则按全部服务入口验证。
    let targets = manifest
        .services
        .iter()
        .filter(|entry| rule.scope.covers_service_entry(&entry.service_entry_id));
    for entry in targets {
        let effective_protocol = rule
            .selected_protocol
            .as_deref()
            .or_else(|| {
                entry
                    .model_defaults_override
                    .as_ref()
                    .and_then(|overlay| overlay.selected_protocol.as_deref())
            })
            .or(manifest.model_defaults.selected_protocol.as_deref());
        if let Some(protocol) = effective_protocol {
            let route = entry
                .protocol_routes
                .iter()
                .find(|route| route.protocol_profile_id == *protocol);
            let Some(route) = route else {
                errors.push(ManifestValidationError::new(
                    "rule_protocol_missing_on_entry_route",
                    path(".selected_protocol"),
                    format!(
                        "rule `{}` selects protocol `{protocol}` but service entry `{}` does not declare that route",
                        rule.rule_id, entry.service_entry_id
                    ),
                ));
                continue;
            };
            if !route.verified {
                errors.push(ManifestValidationError::new(
                    "rule_protocol_route_not_verified",
                    path(".selected_protocol"),
                    format!(
                        "rule `{}` selects unverified protocol `{protocol}` on service entry `{}`",
                        rule.rule_id, entry.service_entry_id
                    ),
                ));
                continue;
            }
            for constraint in &rule.input_resource_constraints {
                let Some(route_constraint) =
                    route
                        .input_resource_constraints
                        .iter()
                        .find(|route_constraint| {
                            route_constraint.resource_kind == constraint.resource_kind
                        })
                else {
                    continue;
                };
                if !input_resource_constraint_not_expanded(constraint, route_constraint) {
                    errors.push(ManifestValidationError::new(
                        "model_input_resource_constraint_expands_route",
                        path(".input_resource_constraints"),
                        format!(
                            "rule `{}` expands the {:?} input authority of protocol `{protocol}` on service entry `{}`",
                            rule.rule_id, constraint.resource_kind, entry.service_entry_id
                        ),
                    ));
                }
            }
        }
    }
}

/// Validates declaration identifier uniqueness across all named sections.
/// 验证所有具名声明的标识唯一性。
fn validate_declaration_uniqueness(
    manifest: &ProviderManifestV1,
    registry: &EnginePrimitiveRegistry,
    errors: &mut Vec<ManifestValidationError>,
) {
    let mut usage = BTreeSet::new();
    for (index, declaration) in manifest.usage_profiles.iter().enumerate() {
        validate_identifier(
            &declaration.profile_id,
            &format!("$.usage_profiles[{index}].profile_id"),
            errors,
            "usage profile id",
        );
        if !usage.insert(declaration.profile_id.clone()) {
            errors.push(ManifestValidationError::new(
                "duplicate_usage_profile_id",
                "$.usage_profiles",
                format!("duplicate usage profile id `{}`", declaration.profile_id),
            ));
        }
    }
    let mut request = BTreeSet::new();
    for (index, declaration) in manifest.request_profiles.iter().enumerate() {
        validate_identifier(
            &declaration.profile_id,
            &format!("$.request_profiles[{index}].profile_id"),
            errors,
            "request profile id",
        );
        if !request.insert(declaration.profile_id.clone()) {
            errors.push(ManifestValidationError::new(
                "duplicate_request_profile_id",
                "$.request_profiles",
                format!("duplicate request profile id `{}`", declaration.profile_id),
            ));
        }
    }
    let mut response = BTreeSet::new();
    for (index, declaration) in manifest.response_profiles.iter().enumerate() {
        validate_identifier(
            &declaration.profile_id,
            &format!("$.response_profiles[{index}].profile_id"),
            errors,
            "response profile id",
        );
        if !response.insert(declaration.profile_id.clone()) {
            errors.push(ManifestValidationError::new(
                "duplicate_response_profile_id",
                "$.response_profiles",
                format!("duplicate response profile id `{}`", declaration.profile_id),
            ));
        }
    }
    let mut channels = BTreeSet::new();
    for (index, declaration) in manifest.upload_channels.iter().enumerate() {
        validate_identifier(
            &declaration.upload_channel_id,
            &format!("$.upload_channels[{index}].upload_channel_id"),
            errors,
            "upload channel id",
        );
        if !channels.insert(declaration.upload_channel_id.clone()) {
            errors.push(ManifestValidationError::new(
                "duplicate_upload_channel_id",
                "$.upload_channels",
                format!(
                    "duplicate upload channel id `{}`",
                    declaration.upload_channel_id
                ),
            ));
        }
        validate_upload_channel(manifest, declaration, registry, errors);
    }
    let mut operations = BTreeSet::new();
    for (index, declaration) in manifest.operations.iter().enumerate() {
        validate_identifier(
            &declaration.operation_id,
            &format!("$.operations[{index}].operation_id"),
            errors,
            "operation id",
        );
        if !operations.insert(declaration.operation_id.clone()) {
            errors.push(ManifestValidationError::new(
                "duplicate_operation_id",
                "$.operations",
                format!("duplicate operation id `{}`", declaration.operation_id),
            ));
        }
    }
    let mut targets = BTreeSet::new();
    for (index, declaration) in manifest.tool_targets.iter().enumerate() {
        validate_extended_identifier(
            &declaration.target_id,
            &format!("$.tool_targets[{index}].target_id"),
            errors,
            "tool target id",
        );
        validate_identifier(
            &declaration.tool_id,
            &format!("$.tool_targets[{index}].tool_id"),
            errors,
            "AI-visible tool id",
        );
        if !targets.insert(declaration.target_id.clone()) {
            errors.push(ManifestValidationError::new(
                "duplicate_tool_target_id",
                "$.tool_targets",
                format!("duplicate tool target id `{}`", declaration.target_id),
            ));
        }
    }
    for (index, declaration) in manifest.tool_target_templates.iter().enumerate() {
        validate_identifier(
            &declaration.template_id,
            &format!("$.tool_target_templates[{index}].template_id"),
            errors,
            "tool target template id",
        );
        validate_identifier(
            &declaration.tool_id,
            &format!("$.tool_target_templates[{index}].tool_id"),
            errors,
            "AI-visible template tool id",
        );
    }
}

/// Validates one upload channel against the bounded host and size policy (plan §16.3).
/// 依据有界宿主与大小策略校验一个上传通道（方案 §16.3）。
///
/// # Parameters
/// 参数
/// `declaration` is the exact channel and `errors` collects controlled validation failures.
/// `declaration` 是精确通道，`errors` 收集受控校验失败。
fn validate_upload_channel(
    manifest: &ProviderManifestV1,
    declaration: &super::UploadChannelDeclaration,
    registry: &EnginePrimitiveRegistry,
    errors: &mut Vec<ManifestValidationError>,
) {
    let path = format!(
        "$.upload_channels[{}]",
        declaration.upload_channel_id.as_str()
    );
    let Some(primitive) = declaration.primitive else {
        errors.push(ManifestValidationError::new(
            "upload_channel_requires_primitive",
            &path,
            format!(
                "upload channel `{}` must declare a closed upload primitive",
                declaration.upload_channel_id
            ),
        ));
        return;
    };
    let primitive_id = upload_primitive_id(primitive);
    if !registry.upload_primitives.contains(primitive_id) {
        errors.push(ManifestValidationError::new(
            "unknown_upload_primitive",
            format!("{path}.primitive"),
            format!(
                "upload channel `{}` uses upload primitive `{primitive_id}` that is not registered by this engine",
                declaration.upload_channel_id
            ),
        ));
    }
    if !manifest
        .engine_contract
        .required_upload_primitives
        .iter()
        .any(|required| required == primitive_id)
    {
        errors.push(ManifestValidationError::new(
            "upload_primitive_missing_from_engine_contract",
            format!("{path}.primitive"),
            format!(
                "upload channel `{}` uses `{primitive_id}` without declaring it in engine_contract.required_upload_primitives",
                declaration.upload_channel_id
            ),
        ));
    }
    if let Some(credential_role) = declaration.credential_role.as_deref() {
        validate_identifier(
            credential_role,
            &format!("{path}.credential_role"),
            errors,
            "upload credential role",
        );
        let declared = manifest
            .services
            .iter()
            .flat_map(|entry| entry.credential_profiles.iter())
            .any(|profile| profile.credential_role.as_deref() == Some(credential_role));
        if !declared {
            errors.push(ManifestValidationError::new(
                "upload_channel_unknown_credential_role",
                format!("{path}.credential_role"),
                format!(
                    "upload channel `{}` references credential role `{credential_role}` that no credential profile declares",
                    declaration.upload_channel_id
                ),
            ));
        }
    }

    if declaration.accepted_mime_types.is_empty()
        || declaration.accepted_mime_types.len() > MAX_MIME_TYPES
    {
        errors.push(ManifestValidationError::new(
            "upload_channel_requires_bounded_mime_allowlist",
            format!("{path}.accepted_mime_types"),
            format!(
                "upload channel `{}` must declare 1..={MAX_MIME_TYPES} exact MIME types",
                declaration.upload_channel_id
            ),
        ));
    }
    let mut accepted_mime_types = BTreeSet::new();
    for (index, mime_type) in declaration.accepted_mime_types.iter().enumerate() {
        if !is_safe_upload_mime_type(mime_type) {
            errors.push(ManifestValidationError::new(
                "invalid_upload_channel_mime_type",
                format!("{path}.accepted_mime_types[{index}]"),
                format!(
                    "upload channel MIME `{mime_type}` must be one exact lowercase type/subtype without parameters or wildcards"
                ),
            ));
        } else if !accepted_mime_types.insert(mime_type.as_str()) {
            errors.push(ManifestValidationError::new(
                "duplicate_upload_channel_mime_type",
                format!("{path}.accepted_mime_types[{index}]"),
                format!("upload channel MIME `{mime_type}` is duplicated"),
            ));
        }
    }

    if declaration
        .max_count
        .is_none_or(|max_count| max_count == 0 || max_count > MAX_UPLOAD_ITEM_COUNT)
    {
        errors.push(ManifestValidationError::new(
            "upload_channel_requires_bounded_max_count",
            format!("{path}.max_count"),
            format!(
                "upload channel `{}` max_count must be within 1..={MAX_UPLOAD_ITEM_COUNT}",
                declaration.upload_channel_id
            ),
        ));
    }
    let consumes_bytes = matches!(
        primitive,
        super::UploadPrimitiveKind::InlineDataUrl
            | super::UploadPrimitiveKind::InlineBase64
            | super::UploadPrimitiveKind::MultipartFile
            | super::UploadPrimitiveKind::ProviderFileCreate
            | super::UploadPrimitiveKind::PresignedFormUpload
            | super::UploadPrimitiveKind::SignedObjectUpload
    );
    if (consumes_bytes && declaration.max_bytes.is_none())
        || declaration
            .max_bytes
            .is_some_and(|max_bytes| max_bytes == 0 || max_bytes > MAX_UPLOAD_BYTES)
    {
        errors.push(ManifestValidationError::new(
            "upload_channel_requires_bounded_max_bytes",
            format!("{path}.max_bytes"),
            format!(
                "upload channel `{}` max_bytes must be within 1..={MAX_UPLOAD_BYTES}",
                declaration.upload_channel_id
            ),
        ));
    }

    if primitive == super::UploadPrimitiveKind::SignedObjectUpload
        && declaration.signing_primitive.is_none()
    {
        errors.push(ManifestValidationError::new(
            "signed_upload_requires_signing_primitive",
            &path,
            format!(
                "upload channel `{}` must declare a registered signing primitive",
                declaration.upload_channel_id
            ),
        ));
    }
    if primitive != super::UploadPrimitiveKind::SignedObjectUpload
        && declaration.signing_primitive.is_some()
    {
        errors.push(ManifestValidationError::new(
            "signing_primitive_requires_signed_upload",
            &path,
            format!(
                "upload channel `{}` declares a signing primitive for a non-signed upload",
                declaration.upload_channel_id
            ),
        ));
    }
    let uses_returned_signed_host = primitive == super::UploadPrimitiveKind::SignedObjectUpload
        && declaration.signing_primitive
            == Some(super::UploadSigningPrimitiveKind::TemporaryOssPolicy);
    if declaration.response_host_suffixes.len() > 16 {
        errors.push(ManifestValidationError::new(
            "too_many_upload_response_host_suffixes",
            &path,
            "upload channels may declare at most 16 response host suffixes",
        ));
    }
    if uses_returned_signed_host && declaration.response_host_suffixes.is_empty() {
        errors.push(ManifestValidationError::new(
            "signed_upload_requires_response_host_suffix",
            &path,
            "temporary signed uploads require at least one allowed response host suffix",
        ));
    }
    if !uses_returned_signed_host && !declaration.response_host_suffixes.is_empty() {
        errors.push(ManifestValidationError::new(
            "upload_response_host_suffix_requires_signed_policy",
            &path,
            "response host suffixes are valid only for temporary signed upload policies",
        ));
    }
    for suffix in &declaration.response_host_suffixes {
        if !crate::provider_resource_host_suffix_is_safe(suffix) {
            errors.push(ManifestValidationError::new(
                "invalid_upload_response_host_suffix",
                &path,
                format!(
                    "upload response host suffix `{suffix}` is not a safe lowercase DNS suffix"
                ),
            ));
        }
    }

    let network_bound = matches!(
        primitive,
        super::UploadPrimitiveKind::MultipartFile
            | super::UploadPrimitiveKind::ProviderFileCreate
            | super::UploadPrimitiveKind::PresignedFormUpload
            | super::UploadPrimitiveKind::SignedObjectUpload
    );
    if network_bound {
        if declaration
            .timeout_ms
            .is_none_or(|timeout_ms| !(1_000..=MAX_ASYNC_TIMEOUT_MS).contains(&timeout_ms))
        {
            errors.push(ManifestValidationError::new(
                "network_upload_timeout_required",
                format!("{path}.timeout_ms"),
                format!(
                    "network upload channel `{}` timeout_ms must be within 1000..={MAX_ASYNC_TIMEOUT_MS}",
                    declaration.upload_channel_id
                ),
            ));
        }
        // Network-bound primitives must declare an explicit HTTPS host so the runtime never
        // resolves upload targets through free-form URLs (SSRF boundary).
        // 网络绑定原语必须声明显式 HTTPS 宿主，运行时绝不通过自由格式 URL 解析上传目标
        // （SSRF 边界）。
        let host_ok = declaration.endpoint_host.as_deref().is_some_and(|host| {
            !host.is_empty()
                && !host.contains(['/', '\\', '@', ':', ' '])
                && !host.starts_with('.')
                && host
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '.')
        });
        if !host_ok {
            errors.push(ManifestValidationError::new(
                "upload_channel_requires_explicit_host",
                format!("{path}.endpoint_host"),
                format!(
                    "upload channel `{}` must declare an explicit HTTPS host without path, port, credentials, or wildcards",
                    declaration.upload_channel_id
                ),
            ));
        }
        if declaration.endpoint_path.as_deref().is_none_or(|endpoint| {
            !endpoint.starts_with('/')
                || endpoint.contains(['?', '#', '\\'])
                || endpoint.chars().any(char::is_control)
        }) {
            errors.push(ManifestValidationError::new(
                "upload_channel_requires_safe_endpoint",
                format!("{path}.endpoint_path"),
                format!(
                    "upload channel `{}` endpoint must be a same-origin absolute path without query, fragment, backslash, or control characters",
                    declaration.upload_channel_id
                ),
            ));
        }
    } else {
        if declaration.endpoint_host.is_some() || declaration.endpoint_path.is_some() {
            errors.push(ManifestValidationError::new(
                "non_network_upload_forbids_endpoint",
                &path,
                format!(
                    "upload channel `{}` declares an endpoint for a non-network primitive",
                    declaration.upload_channel_id
                ),
            ));
        }
        if declaration.timeout_ms.is_some() {
            errors.push(ManifestValidationError::new(
                "non_network_upload_forbids_timeout",
                format!("{path}.timeout_ms"),
                format!(
                    "upload channel `{}` declares timeout_ms for a non-network primitive",
                    declaration.upload_channel_id
                ),
            ));
        }
    }

    let expected_method = match (primitive, declaration.signing_primitive) {
        (
            super::UploadPrimitiveKind::MultipartFile
            | super::UploadPrimitiveKind::ProviderFileCreate,
            _,
        ) => Some("POST"),
        (
            super::UploadPrimitiveKind::SignedObjectUpload,
            Some(super::UploadSigningPrimitiveKind::TemporaryOssPolicy),
        ) => Some("GET"),
        _ => None,
    };
    match (expected_method, declaration.method.as_deref()) {
        (Some(expected), Some(actual)) if actual == expected => {}
        (Some(expected), _) => errors.push(ManifestValidationError::new(
            "upload_channel_method_mismatch",
            format!("{path}.method"),
            format!(
                "upload channel `{}` must declare exact method `{expected}`",
                declaration.upload_channel_id
            ),
        )),
        (None, Some(_)) => errors.push(ManifestValidationError::new(
            "upload_channel_forbids_method",
            format!("{path}.method"),
            format!(
                "upload channel `{}` declares a method for a primitive without a registered HTTP request",
                declaration.upload_channel_id
            ),
        )),
        (None, None) => {}
    }

    if primitive == super::UploadPrimitiveKind::RemoteUrlPassthrough {
        if declaration.remote_url_host_suffixes.is_empty()
            || declaration.remote_url_host_suffixes.len() > 16
        {
            errors.push(ManifestValidationError::new(
                "remote_url_upload_requires_host_allowlist",
                format!("{path}.remote_url_host_suffixes"),
                "remote URL passthrough requires 1..=16 exact or dot-prefixed HTTPS host suffixes",
            ));
        }
        for (index, suffix) in declaration.remote_url_host_suffixes.iter().enumerate() {
            if !crate::provider_resource_host_suffix_is_safe(suffix) {
                errors.push(ManifestValidationError::new(
                    "invalid_remote_url_upload_host_suffix",
                    format!("{path}.remote_url_host_suffixes[{index}]"),
                    format!("remote URL host suffix `{suffix}` is not a safe DNS matcher"),
                ));
            }
        }
    } else if !declaration.remote_url_host_suffixes.is_empty() {
        errors.push(ManifestValidationError::new(
            "remote_url_host_allowlist_requires_passthrough",
            format!("{path}.remote_url_host_suffixes"),
            "remote URL host suffixes are valid only for remote_url_passthrough",
        ));
    }

    match primitive {
        super::UploadPrimitiveKind::MultipartFile => {
            if declaration
                .file_field
                .as_deref()
                .is_none_or(|field| !is_safe_upload_form_field_name(field))
            {
                errors.push(ManifestValidationError::new(
                    "multipart_upload_requires_file_field",
                    format!("{path}.file_field"),
                    "multipart upload requires a bounded ASCII file field name",
                ));
            }
        }
        _ if declaration.file_field.is_some() => {
            errors.push(ManifestValidationError::new(
                "upload_file_field_requires_multipart",
                format!("{path}.file_field"),
                "file_field is valid only for multipart_file",
            ));
        }
        _ => {}
    }

    validate_upload_fixed_fields(declaration, primitive, path.as_str(), errors);
    validate_upload_reference_contract(declaration, primitive, path.as_str(), errors);

    if declaration
        .cache_duration_ms
        .is_some_and(|duration| duration == 0 || duration > MAX_UPLOAD_CACHE_DURATION_MS)
    {
        errors.push(ManifestValidationError::new(
            "upload_cache_duration_out_of_bounds",
            format!("{path}.cache_duration_ms"),
            format!(
                "upload cache duration must be within 1..={MAX_UPLOAD_CACHE_DURATION_MS} milliseconds"
            ),
        ));
    }
    if declaration.cache_duration_ms.is_some()
        && !matches!(
            primitive,
            super::UploadPrimitiveKind::MultipartFile
                | super::UploadPrimitiveKind::ProviderFileCreate
                | super::UploadPrimitiveKind::SignedObjectUpload
        )
    {
        errors.push(ManifestValidationError::new(
            "upload_cache_requires_provider_object",
            format!("{path}.cache_duration_ms"),
            "cache_duration_ms is valid only for provider-created object references",
        ));
    }
}

/// Returns the stable engine-contract identifier for one upload primitive enum.
/// 返回一个上传原语枚举对应的稳定引擎契约标识。
///
/// # Parameters
/// 参数
/// `primitive` is the closed schema enum value.
/// `primitive` 是封闭 Schema 枚举值。
///
/// # Returns
/// 返回值
/// The exact snake_case identifier used by the engine registry and manifest contract.
/// 返回引擎注册表与 Manifest 契约使用的精确 snake_case 标识。
fn upload_primitive_id(primitive: super::UploadPrimitiveKind) -> &'static str {
    match primitive {
        super::UploadPrimitiveKind::InlineDataUrl => "inline_data_url",
        super::UploadPrimitiveKind::InlineBase64 => "inline_base64",
        super::UploadPrimitiveKind::RemoteUrlPassthrough => "remote_url_passthrough",
        super::UploadPrimitiveKind::MultipartFile => "multipart_file",
        super::UploadPrimitiveKind::ProviderFileCreate => "provider_file_create",
        super::UploadPrimitiveKind::PresignedFormUpload => "presigned_form_upload",
        super::UploadPrimitiveKind::SignedObjectUpload => "signed_object_upload",
        super::UploadPrimitiveKind::ManagedAssetReference => "managed_asset_reference",
    }
}

/// Reports whether one MIME value is a bounded exact lowercase type/subtype.
/// 报告一个 MIME 值是否为有界且精确的小写 type/subtype。
///
/// # Parameters
/// 参数
/// `mime_type` is the untrusted manifest string.
/// `mime_type` 是不受信任的 Manifest 字符串。
///
/// # Returns
/// 返回值
/// `true` only for one parameter-free non-wildcard MIME type.
/// 仅对不带参数且不含通配符的单个 MIME 类型返回 `true`。
fn is_safe_upload_mime_type(mime_type: &str) -> bool {
    if mime_type.is_empty() || mime_type.len() > 127 || mime_type != mime_type.to_ascii_lowercase()
    {
        return false;
    }
    let Some((kind, subtype)) = mime_type.split_once('/') else {
        return false;
    };
    if kind.is_empty() || subtype.is_empty() || subtype.contains('/') {
        return false;
    }
    let safe = |value: &str| {
        value.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(
                    character,
                    '!' | '#' | '$' | '&' | '^' | '_' | '.' | '+' | '-'
                )
        })
    };
    safe(kind) && safe(subtype)
}

/// Reports whether one multipart field name belongs to the bounded ASCII field alphabet.
/// 报告一个 Multipart 字段名是否属于有界 ASCII 字段字符集。
///
/// # Parameters
/// 参数
/// `field` is the untrusted manifest field name.
/// `field` 是不受信任的 Manifest 字段名。
///
/// # Returns
/// 返回值
/// `true` for a non-empty name of at most 128 characters without header/control syntax.
/// 对不含头部或控制语法、长度不超过 128 的非空名称返回 `true`。
fn is_safe_upload_form_field_name(field: &str) -> bool {
    !field.is_empty()
        && field.len() <= 128
        && field.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
}

/// Validates fixed upload fields against primitive ownership and collision boundaries.
/// 依据原语所有权与冲突边界校验固定上传字段。
///
/// # Parameters
/// 参数
/// `declaration` and `primitive` identify the exact channel, `path` is its diagnostic root, and
/// `errors` collects every deterministic violation.
/// `declaration` 与 `primitive` 标识精确通道，`path` 是诊断根，`errors` 收集全部确定性违规。
fn validate_upload_fixed_fields(
    declaration: &super::UploadChannelDeclaration,
    primitive: super::UploadPrimitiveKind,
    path: &str,
    errors: &mut Vec<ManifestValidationError>,
) {
    if declaration.fixed_fields.len() > MAX_UPLOAD_FIXED_FIELDS {
        errors.push(ManifestValidationError::new(
            "too_many_upload_fixed_fields",
            format!("{path}.fixed_fields"),
            format!("an upload channel may declare at most {MAX_UPLOAD_FIXED_FIELDS} fixed fields"),
        ));
    }
    if !matches!(
        primitive,
        super::UploadPrimitiveKind::MultipartFile | super::UploadPrimitiveKind::ProviderFileCreate
    ) && !declaration.fixed_fields.is_empty()
    {
        errors.push(ManifestValidationError::new(
            "upload_fixed_fields_require_payload_primitive",
            format!("{path}.fixed_fields"),
            "fixed_fields are valid only for multipart_file or provider_file_create",
        ));
    }
    for (name, value) in &declaration.fixed_fields {
        if !is_safe_upload_form_field_name(name)
            || value.len() > MAX_STRING_LEN
            || value.chars().any(char::is_control)
        {
            errors.push(ManifestValidationError::new(
                "invalid_upload_fixed_field",
                format!("{path}.fixed_fields.{name}"),
                "upload fixed fields require bounded ASCII names and control-free bounded values",
            ));
        }
        if primitive == super::UploadPrimitiveKind::MultipartFile
            && declaration.file_field.as_deref() == Some(name.as_str())
        {
            errors.push(ManifestValidationError::new(
                "upload_fixed_field_collides_with_file",
                format!("{path}.fixed_fields.{name}"),
                format!("fixed field `{name}` collides with the multipart file field"),
            ));
        }
        if primitive == super::UploadPrimitiveKind::ProviderFileCreate
            && matches!(name.as_str(), "mime_type" | "content_base64")
        {
            errors.push(ManifestValidationError::new(
                "upload_fixed_field_collides_with_host_field",
                format!("{path}.fixed_fields.{name}"),
                format!("fixed field `{name}` is owned by the host file-create payload"),
            ));
        }
    }
}

/// Validates response pointers and the closed provider-reference encoding contract.
/// 校验响应指针与封闭供应商引用编码契约。
///
/// # Parameters
/// 参数
/// `declaration` and `primitive` identify the exact channel, `path` owns diagnostics, and
/// `errors` collects every closure failure.
/// `declaration` 与 `primitive` 标识精确通道，`path` 持有诊断，`errors` 收集全部闭包失败。
fn validate_upload_reference_contract(
    declaration: &super::UploadChannelDeclaration,
    primitive: super::UploadPrimitiveKind,
    path: &str,
    errors: &mut Vec<ManifestValidationError>,
) {
    for (field, pointer) in [
        (
            "response_id_pointer",
            declaration.response_id_pointer.as_deref(),
        ),
        (
            "response_url_pointer",
            declaration.response_url_pointer.as_deref(),
        ),
    ] {
        if pointer.is_some_and(|pointer| !is_valid_json_pointer_path(pointer)) {
            errors.push(ManifestValidationError::new(
                "invalid_upload_response_pointer",
                format!("{path}.{field}"),
                format!(
                    "upload response pointer `{}` is not an absolute JSON Pointer",
                    pointer.unwrap_or_default()
                ),
            ));
        }
    }

    let response_driven = matches!(
        primitive,
        super::UploadPrimitiveKind::MultipartFile | super::UploadPrimitiveKind::ProviderFileCreate
    );
    if !response_driven {
        if declaration.response_id_pointer.is_some()
            || declaration.response_url_pointer.is_some()
            || declaration.reference_template.is_some()
        {
            errors.push(ManifestValidationError::new(
                "upload_reference_fields_require_response_primitive",
                path,
                "response pointers and reference_template are valid only for response-driven upload primitives",
            ));
        }
        return;
    }

    let encoding = declaration
        .reference_encoding
        .unwrap_or(super::UploadReferenceEncoding::ProviderFileId);
    match encoding {
        super::UploadReferenceEncoding::ProviderFileId => {
            if declaration.response_id_pointer.is_none() {
                errors.push(ManifestValidationError::new(
                    "upload_reference_requires_id_pointer",
                    format!("{path}.response_id_pointer"),
                    "provider_file_id encoding requires response_id_pointer",
                ));
            }
            if declaration.reference_template.is_some() {
                errors.push(ManifestValidationError::new(
                    "upload_reference_template_requires_template_encoding",
                    format!("{path}.reference_template"),
                    "reference_template requires template encoding",
                ));
            }
        }
        super::UploadReferenceEncoding::ResponseUrl => {
            if declaration.response_url_pointer.is_none() {
                errors.push(ManifestValidationError::new(
                    "upload_reference_requires_url_pointer",
                    format!("{path}.response_url_pointer"),
                    "response_url encoding requires response_url_pointer",
                ));
            }
            if declaration.reference_template.is_some() {
                errors.push(ManifestValidationError::new(
                    "upload_reference_template_requires_template_encoding",
                    format!("{path}.reference_template"),
                    "reference_template requires template encoding",
                ));
            }
        }
        super::UploadReferenceEncoding::Template => {
            let Some(template) = declaration.reference_template.as_deref() else {
                errors.push(ManifestValidationError::new(
                    "upload_template_encoding_requires_template",
                    format!("{path}.reference_template"),
                    "template encoding requires reference_template",
                ));
                return;
            };
            let uses_id = template.contains("{id}");
            let uses_url = template.contains("{url}");
            let remainder = template.replace("{id}", "").replace("{url}", "");
            if template.is_empty()
                || template.len() > MAX_STRING_LEN
                || template.chars().any(char::is_control)
                || (!uses_id && !uses_url)
                || remainder.contains(['{', '}'])
            {
                errors.push(ManifestValidationError::new(
                    "invalid_upload_reference_template",
                    format!("{path}.reference_template"),
                    "reference_template must be bounded and use only {id} and/or {url} placeholders",
                ));
            }
            if uses_id && declaration.response_id_pointer.is_none() {
                errors.push(ManifestValidationError::new(
                    "upload_template_requires_id_pointer",
                    format!("{path}.response_id_pointer"),
                    "a {id} reference template requires response_id_pointer",
                ));
            }
            if uses_url && declaration.response_url_pointer.is_none() {
                errors.push(ManifestValidationError::new(
                    "upload_template_requires_url_pointer",
                    format!("{path}.response_url_pointer"),
                    "a {url} reference template requires response_url_pointer",
                ));
            }
        }
    }
}

/// Validates that every referenced profile declaration exists.
/// 验证所有被引用的 Profile 声明存在。
fn validate_profile_refs(
    manifest: &ProviderManifestV1,
    refs: &[String],
    kind: &str,
    path: &str,
    errors: &mut Vec<ManifestValidationError>,
) {
    let known: BTreeSet<&str> = match kind {
        "usage" => manifest
            .usage_profiles
            .iter()
            .map(|declaration| declaration.profile_id.as_str())
            .collect(),
        "request" => manifest
            .request_profiles
            .iter()
            .map(|declaration| declaration.profile_id.as_str())
            .collect(),
        _ => manifest
            .response_profiles
            .iter()
            .map(|declaration| declaration.profile_id.as_str())
            .collect(),
    };
    for reference in refs {
        if !known.contains(reference.as_str()) {
            errors.push(ManifestValidationError::new(
                "dangling_profile_reference",
                path,
                format!("{kind} profile reference `{reference}` is not declared"),
            ));
        }
    }
}

/// Bounded host request fields a projection step may copy into the wire body.
/// 投影步骤可复制进 Wire 请求体的有界宿主请求字段。
const REQUEST_FIELD_ALLOWLIST: &[&str] = &[
    "reasoning_effort",
    "model_id",
    "output_token_limit",
    "cache_scope_id",
    "prompt_cache",
];

/// Minimum source count for one all-present rejection guard.
/// 单条全部存在拒绝守卫的最小来源数量。
const MIN_MAPPING_PRESENCE_CONDITIONS: usize = 2;

/// Maximum source count for one all-present rejection guard.
/// 单条全部存在拒绝守卫的最大来源数量。
const MAX_MAPPING_PRESENCE_CONDITIONS: usize = 8;

/// Maximum rendered length of one string template.
/// 单条字符串模板渲染后的最大长度。
const STRING_TEMPLATE_MAX_LENGTH: usize = 512;

/// Validates every request/response projection profile payload and its closed mapping steps.
/// 校验每个请求/响应投影 Profile 载荷及其封闭映射步骤。
///
/// Business invariant (plan §R8.3): host-authoritative roots (`model`, `messages`, `input`,
/// `tools`, `stream`, audit, attempt, Authorization) must never be overwritten by a Manifest;
/// repeated normalized target paths and invalid JSON Pointers are rejected at compile time.
/// 业务不变量（方案 §R8.3）：宿主权威根（`model`、`messages`、`input`、`tools`、`stream`、
/// 审计、attempt、Authorization）绝不能被 Manifest 覆盖；重复的归一化目标路径与非法
/// JSON Pointer 在编译期被拒绝。
fn validate_projection_profiles(
    manifest: &ProviderManifestV1,
    errors: &mut Vec<ManifestValidationError>,
) {
    for (index, declaration) in manifest.request_profiles.iter().enumerate() {
        validate_one_projection_payload(
            "request",
            declaration.profile_id.as_str(),
            declaration
                .payload
                .as_ref()
                .map(|payload| payload.steps.as_slice())
                .unwrap_or(&[]),
            &format!("$.request_profiles[{index}]"),
            errors,
        );
        if let Some(payload) = declaration.payload.as_ref()
            && payload.profile_id != declaration.profile_id
        {
            errors.push(ManifestValidationError::new(
                "profile_payload_id_mismatch",
                format!("$.request_profiles[{index}]"),
                format!(
                    "request profile payload id `{}` does not match declaration id `{}`",
                    payload.profile_id, declaration.profile_id
                ),
            ));
        }
    }
    for (index, declaration) in manifest.response_profiles.iter().enumerate() {
        validate_one_projection_payload(
            "response",
            declaration.profile_id.as_str(),
            declaration
                .payload
                .as_ref()
                .map(|payload| payload.steps.as_slice())
                .unwrap_or(&[]),
            &format!("$.response_profiles[{index}]"),
            errors,
        );
        if let Some(payload) = declaration.payload.as_ref()
            && payload.profile_id != declaration.profile_id
        {
            errors.push(ManifestValidationError::new(
                "profile_payload_id_mismatch",
                format!("$.response_profiles[{index}]"),
                format!(
                    "response profile payload id `{}` does not match declaration id `{}`",
                    payload.profile_id, declaration.profile_id
                ),
            ));
        }
    }
    for (operation_index, operation) in manifest.operations.iter().enumerate() {
        for profile_id in &operation.request_profile_refs {
            let Some(payload) = manifest
                .request_profiles
                .iter()
                .find(|declaration| declaration.profile_id == *profile_id)
                .and_then(|declaration| declaration.payload.as_ref())
            else {
                continue;
            };
            for (step_index, step) in payload.steps.iter().enumerate() {
                if operation_request_step_requires_host_context(step) {
                    errors.push(ManifestValidationError::new(
                        "operation_request_profile_requires_host_context",
                        format!(
                            "$.operations[{operation_index}].request_profile_refs.{profile_id}.steps[{step_index}]"
                        ),
                        format!(
                            "operation `{}` request profile `{profile_id}` may read only operation-body JSON pointers",
                            operation.operation_id
                        ),
                    ));
                }
            }
        }
    }
}

/// Reports whether a request mapping step needs conversation-only host request fields.
/// 报告一个请求映射步骤是否需要仅会话可用的宿主请求字段。
fn operation_request_step_requires_host_context(
    step: &crate::provider_manifest::schema::MappingStep,
) -> bool {
    match step {
        crate::provider_manifest::schema::MappingStep::CopyRequestField { .. } => true,
        crate::provider_manifest::schema::MappingStep::EnumMap { source, .. }
        | crate::provider_manifest::schema::MappingStep::ConditionalSet { source, .. }
        | crate::provider_manifest::schema::MappingStep::DeleteIfEquals { source, .. }
        | crate::provider_manifest::schema::MappingStep::DeleteUnlessEquals { source, .. }
        | crate::provider_manifest::schema::MappingStep::RejectIfPresent { source, .. } => {
            *source == crate::provider_manifest::schema::MappingStepSource::RequestField
        }
        crate::provider_manifest::schema::MappingStep::RejectIfAllPresent {
            conditions, ..
        } => conditions.iter().any(|condition| {
            condition.source == crate::provider_manifest::schema::MappingStepSource::RequestField
        }),
        _ => false,
    }
}

/// Validates the closed mapping steps of one projection profile payload.
/// 校验一个投影 Profile 载荷的封闭映射步骤。
fn validate_one_projection_payload(
    kind: &str,
    profile_id: &str,
    steps: &[crate::provider_manifest::schema::MappingStep],
    base_path: &str,
    errors: &mut Vec<ManifestValidationError>,
) {
    let mut written_targets = BTreeSet::new();
    for (index, step) in steps.iter().enumerate() {
        let step_path = format!("{base_path}.payload.steps[{index}]");
        if kind == "response" && response_projection_step_requires_request_context(step) {
            errors.push(ManifestValidationError::new(
                "response_profile_requires_request_context",
                &step_path,
                format!("response profile `{profile_id}` may only read response JSON pointers"),
            ));
        }
        let write_path = projection_step_write_path(step);
        let delete_path = projection_step_delete_path(step);
        for target in [write_path, delete_path].into_iter().flatten() {
            validate_projection_target_path(kind, profile_id, target, &step_path, errors);
        }
        if let Some(target) = write_path
            && !written_targets.insert(target.to_string())
        {
            errors.push(ManifestValidationError::new(
                "duplicate_projection_target",
                &step_path,
                format!(
                    "{kind} profile `{profile_id}` writes JSON pointer `{target}` more than once"
                ),
            ));
        }
        for (source_index, (source, source_path)) in
            projection_step_sources(step).into_iter().enumerate()
        {
            validate_projection_source(
                source,
                source_path,
                format!("{step_path}.sources[{source_index}]").as_str(),
                errors,
            );
        }
        match step {
            crate::provider_manifest::schema::MappingStep::EnumMap {
                source_path,
                mapping,
                ..
            } => {
                if source_path.trim().is_empty() {
                    errors.push(ManifestValidationError::new(
                        "empty_enum_map_source",
                        &step_path,
                        "enum_map source_path must not be empty",
                    ));
                }
                if mapping.is_empty() {
                    errors.push(ManifestValidationError::new(
                        "empty_enum_map_table",
                        &step_path,
                        "enum_map mapping table must not be empty",
                    ));
                }
            }
            crate::provider_manifest::schema::MappingStep::StringTemplate { template, .. } => {
                if template.len() > STRING_TEMPLATE_MAX_LENGTH {
                    errors.push(ManifestValidationError::new(
                        "string_template_too_long",
                        &step_path,
                        format!(
                            "string template exceeds the {STRING_TEMPLATE_MAX_LENGTH} byte bound"
                        ),
                    ));
                }
            }
            crate::provider_manifest::schema::MappingStep::CopyRequestField { field, .. } => {
                let allowed = REQUEST_FIELD_ALLOWLIST.contains(&field.as_str())
                    || field
                        .strip_prefix("model_parameters.")
                        .is_some_and(|parameter_id| !parameter_id.is_empty());
                if !allowed {
                    errors.push(ManifestValidationError::new(
                        "unregistered_request_field",
                        &step_path,
                        format!("request field `{field}` is not on the bounded allowlist"),
                    ));
                }
            }
            crate::provider_manifest::schema::MappingStep::RebuildJsonTemplate { template } => {
                validate_json_template_node(
                    template,
                    0,
                    format!("{step_path}.template").as_str(),
                    errors,
                );
            }
            crate::provider_manifest::schema::MappingStep::RejectIfAllPresent {
                conditions,
                ..
            } => {
                if !(MIN_MAPPING_PRESENCE_CONDITIONS..=MAX_MAPPING_PRESENCE_CONDITIONS)
                    .contains(&conditions.len())
                {
                    errors.push(ManifestValidationError::new(
                        "presence_condition_count_out_of_bounds",
                        &step_path,
                        format!(
                            "reject_if_all_present requires between {MIN_MAPPING_PRESENCE_CONDITIONS} and {MAX_MAPPING_PRESENCE_CONDITIONS} conditions"
                        ),
                    ));
                }
                let mut unique_conditions = BTreeSet::new();
                for condition in conditions {
                    if !unique_conditions.insert((condition.source, condition.source_path.as_str()))
                    {
                        errors.push(ManifestValidationError::new(
                            "duplicate_presence_condition",
                            &step_path,
                            format!(
                                "reject_if_all_present repeats source `{}`",
                                condition.source_path
                            ),
                        ));
                    }
                }
            }
            _ => {}
        }
        if let crate::provider_manifest::schema::MappingStep::RejectIfPresent { message, .. }
        | crate::provider_manifest::schema::MappingStep::RejectIfAllPresent { message, .. } =
            step
            && (message.is_empty() || message.len() > STRING_TEMPLATE_MAX_LENGTH)
        {
            errors.push(ManifestValidationError::new(
                "reject_message_out_of_bounds",
                &step_path,
                format!(
                    "rejection message must be between 1 and {STRING_TEMPLATE_MAX_LENGTH} bytes"
                ),
            ));
        }
    }
}

/// Maximum object/array entries accepted by one registered JSON-template form.
/// 单个已注册 JSON 模板形式允许的最大对象/数组项数。
const JSON_TEMPLATE_COLLECTION_LIMIT: usize = 256;

/// Validates one declarative JSON-template node completely during Manifest compilation.
/// 在 Manifest 编译期间完整校验一个声明式 JSON 模板节点。
fn validate_json_template_node(
    node: &Value,
    depth: usize,
    path: &str,
    errors: &mut Vec<ManifestValidationError>,
) {
    if depth > MAX_JSON_DEPTH {
        errors.push(ManifestValidationError::new(
            "json_template_depth_exceeded",
            path,
            format!("JSON template exceeds the {MAX_JSON_DEPTH} level depth limit"),
        ));
        return;
    }
    match node {
        Value::Array(values) => {
            if values.len() > JSON_TEMPLATE_COLLECTION_LIMIT {
                errors.push(ManifestValidationError::new(
                    "json_template_collection_too_large",
                    path,
                    format!(
                        "JSON template array exceeds the {JSON_TEMPLATE_COLLECTION_LIMIT} item limit"
                    ),
                ));
            }
            for (index, value) in values.iter().enumerate() {
                validate_json_template_node(
                    value,
                    depth + 1,
                    format!("{path}[{index}]").as_str(),
                    errors,
                );
            }
        }
        Value::Object(object) => {
            let reserved = object
                .iter()
                .filter(|(key, _)| key.starts_with("$vulcan_"))
                .collect::<Vec<_>>();
            if reserved.is_empty() {
                if object.len() > JSON_TEMPLATE_COLLECTION_LIMIT {
                    errors.push(ManifestValidationError::new(
                        "json_template_collection_too_large",
                        path,
                        format!(
                            "JSON template object exceeds the {JSON_TEMPLATE_COLLECTION_LIMIT} property limit"
                        ),
                    ));
                }
                for (key, value) in object {
                    validate_json_template_node(
                        value,
                        depth + 1,
                        format!("{path}.{}", key.replace('.', "\\.")).as_str(),
                        errors,
                    );
                }
                return;
            }
            if object.len() != 1 || reserved.len() != 1 {
                errors.push(ManifestValidationError::new(
                    "ambiguous_json_template_form",
                    path,
                    "a registered JSON template form must be the object's only property",
                ));
                return;
            }
            let (tag, specification) = reserved[0];
            validate_json_template_form(tag, specification, depth + 1, path, errors);
        }
        _ => {}
    }
}

/// Validates one exact registered JSON-template form and every nested template.
/// 校验一个精确已注册 JSON 模板形式及其全部嵌套模板。
fn validate_json_template_form(
    tag: &str,
    specification: &Value,
    depth: usize,
    path: &str,
    errors: &mut Vec<ManifestValidationError>,
) {
    match tag {
        "$vulcan_model_id" => {
            if specification != &Value::Bool(true) {
                errors.push(ManifestValidationError::new(
                    "invalid_json_template_model_id",
                    path,
                    "$vulcan_model_id must carry the literal boolean true",
                ));
            }
        }
        "$vulcan_concat" | "$vulcan_array_concat" | "$vulcan_coalesce" => {
            let Some(values) = specification.as_array() else {
                errors.push(ManifestValidationError::new(
                    "invalid_json_template_sequence",
                    path,
                    format!("{tag} must carry an array"),
                ));
                return;
            };
            if values.is_empty() || values.len() > JSON_TEMPLATE_COLLECTION_LIMIT {
                errors.push(ManifestValidationError::new(
                    "json_template_sequence_size_out_of_bounds",
                    path,
                    format!(
                        "{tag} must contain between 1 and {JSON_TEMPLATE_COLLECTION_LIMIT} items"
                    ),
                ));
            }
            for (index, value) in values.iter().enumerate() {
                validate_json_template_node(
                    value,
                    depth + 1,
                    format!("{path}.{tag}[{index}]").as_str(),
                    errors,
                );
            }
        }
        "$vulcan_ref"
        | "$vulcan_map"
        | "$vulcan_enum"
        | "$vulcan_if_equals"
        | "$vulcan_if_present"
        | "$vulcan_if_array_length"
        | "$vulcan_trim" => {
            let Some(object) = specification.as_object() else {
                errors.push(ManifestValidationError::new(
                    "invalid_json_template_form",
                    path,
                    format!("{tag} must carry an object specification"),
                ));
                return;
            };
            let allowed_keys: &[&str] = match tag {
                "$vulcan_ref" => &["scope", "path", "optional", "omit_if_empty"],
                "$vulcan_trim" => &["scope", "path", "optional"],
                "$vulcan_map" => &["scope", "path", "optional", "flatten", "template"],
                "$vulcan_enum" => &["scope", "path", "optional", "mapping", "default"],
                "$vulcan_if_equals" | "$vulcan_if_array_length" => {
                    &["scope", "path", "equals", "then", "otherwise"]
                }
                "$vulcan_if_present" => &["scope", "path", "non_empty", "then", "otherwise"],
                _ => unreachable!("registered object template form"),
            };
            for key in object.keys() {
                if !allowed_keys.contains(&key.as_str()) {
                    errors.push(ManifestValidationError::new(
                        "unregistered_json_template_form_key",
                        path,
                        format!("{tag} key `{key}` is not registered"),
                    ));
                }
            }
            validate_json_template_source(object, tag, path, errors);
            if let Some(optional) = object.get("optional")
                && !optional.is_boolean()
            {
                errors.push(ManifestValidationError::new(
                    "invalid_json_template_optional",
                    path,
                    format!("{tag} optional must be boolean"),
                ));
            }
            if let Some(omit_if_empty) = object.get("omit_if_empty")
                && !omit_if_empty.is_boolean()
            {
                errors.push(ManifestValidationError::new(
                    "invalid_json_template_omit_if_empty",
                    path,
                    format!("{tag} omit_if_empty must be boolean"),
                ));
            }
            if let Some(flatten) = object.get("flatten")
                && !flatten.is_boolean()
            {
                errors.push(ManifestValidationError::new(
                    "invalid_json_template_flatten",
                    path,
                    format!("{tag} flatten must be boolean"),
                ));
            }
            if let Some(non_empty) = object.get("non_empty")
                && !non_empty.is_boolean()
            {
                errors.push(ManifestValidationError::new(
                    "invalid_json_template_non_empty",
                    path,
                    format!("{tag} non_empty must be boolean"),
                ));
            }
            match tag {
                "$vulcan_map" => match object.get("template") {
                    Some(template) => validate_json_template_node(
                        template,
                        depth + 1,
                        format!("{path}.$vulcan_map.template").as_str(),
                        errors,
                    ),
                    None => errors.push(ManifestValidationError::new(
                        "missing_json_template_map_template",
                        path,
                        "$vulcan_map requires a template",
                    )),
                },
                "$vulcan_enum" => {
                    if !object
                        .get("mapping")
                        .and_then(Value::as_object)
                        .is_some_and(|mapping| {
                            !mapping.is_empty() && mapping.len() <= JSON_TEMPLATE_COLLECTION_LIMIT
                        })
                    {
                        errors.push(ManifestValidationError::new(
                            "invalid_json_template_enum_mapping",
                            path,
                            format!(
                                "$vulcan_enum mapping must contain between 1 and {JSON_TEMPLATE_COLLECTION_LIMIT} entries"
                            ),
                        ));
                    }
                }
                "$vulcan_if_equals" => {
                    if !object.contains_key("equals") {
                        errors.push(ManifestValidationError::new(
                            "missing_json_template_condition_value",
                            path,
                            "$vulcan_if_equals requires equals",
                        ));
                    }
                    for branch in ["then", "otherwise"] {
                        if let Some(template) = object.get(branch) {
                            validate_json_template_node(
                                template,
                                depth + 1,
                                format!("{path}.$vulcan_if_equals.{branch}").as_str(),
                                errors,
                            );
                        }
                    }
                }
                "$vulcan_if_present" => {
                    if !object.contains_key("then") && !object.contains_key("otherwise") {
                        errors.push(ManifestValidationError::new(
                            "missing_json_template_presence_branch",
                            path,
                            "$vulcan_if_present requires then or otherwise",
                        ));
                    }
                    for branch in ["then", "otherwise"] {
                        if let Some(template) = object.get(branch) {
                            validate_json_template_node(
                                template,
                                depth + 1,
                                format!("{path}.$vulcan_if_present.{branch}").as_str(),
                                errors,
                            );
                        }
                    }
                }
                "$vulcan_if_array_length" => {
                    if !object
                        .get("equals")
                        .and_then(Value::as_u64)
                        .and_then(|value| usize::try_from(value).ok())
                        .is_some_and(|value| value <= JSON_TEMPLATE_COLLECTION_LIMIT)
                    {
                        errors.push(ManifestValidationError::new(
                            "invalid_json_template_array_length",
                            path,
                            format!(
                                "$vulcan_if_array_length equals must be an integer from 0 through {JSON_TEMPLATE_COLLECTION_LIMIT}"
                            ),
                        ));
                    }
                    for branch in ["then", "otherwise"] {
                        if let Some(template) = object.get(branch) {
                            validate_json_template_node(
                                template,
                                depth + 1,
                                format!("{path}.$vulcan_if_array_length.{branch}").as_str(),
                                errors,
                            );
                        }
                    }
                }
                _ => {}
            }
        }
        _ => errors.push(ManifestValidationError::new(
            "unregistered_json_template_form",
            path,
            format!("JSON template form `{tag}` is not registered"),
        )),
    }
}

/// Validates the closed scope and absolute JSON Pointer used by one template source.
/// 校验一个模板来源使用的封闭作用域与绝对 JSON Pointer。
fn validate_json_template_source(
    specification: &serde_json::Map<String, Value>,
    tag: &str,
    path: &str,
    errors: &mut Vec<ManifestValidationError>,
) {
    let scope = specification
        .get("scope")
        .and_then(Value::as_str)
        .unwrap_or("body");
    if !matches!(scope, "body" | "item") {
        errors.push(ManifestValidationError::new(
            "invalid_json_template_scope",
            path,
            format!("{tag} scope `{scope}` is not registered"),
        ));
    }
    match specification.get("path").and_then(Value::as_str) {
        Some(pointer) if pointer.is_empty() || is_valid_json_pointer_path(pointer) => {}
        _ => errors.push(ManifestValidationError::new(
            "invalid_json_template_pointer",
            path,
            format!("{tag} requires an empty or absolute JSON Pointer path"),
        )),
    }
}

/// Returns the JSON pointer path this step writes, or `None` for steps without a write target.
/// 返回此步骤写入的 JSON Pointer 路径；无写入目标的步骤返回 `None`。
fn projection_step_write_path(
    step: &crate::provider_manifest::schema::MappingStep,
) -> Option<&str> {
    match step {
        crate::provider_manifest::schema::MappingStep::SetLiteral { path, .. }
        | crate::provider_manifest::schema::MappingStep::SetDefaultIfMissing { path, .. }
        | crate::provider_manifest::schema::MappingStep::EnumMap { path, .. }
        | crate::provider_manifest::schema::MappingStep::WrapSingletonArray { path }
        | crate::provider_manifest::schema::MappingStep::UnwrapSingletonArray { path }
        | crate::provider_manifest::schema::MappingStep::StringTemplate { path, .. }
        | crate::provider_manifest::schema::MappingStep::CopyRequestField { path, .. }
        | crate::provider_manifest::schema::MappingStep::ConditionalSet { path, .. } => Some(path),
        crate::provider_manifest::schema::MappingStep::CopyJsonPointer { to, .. }
        | crate::provider_manifest::schema::MappingStep::RenameJsonPointer { to, .. } => Some(to),
        crate::provider_manifest::schema::MappingStep::DeleteJsonPointer { .. }
        | crate::provider_manifest::schema::MappingStep::DeleteIfEquals { .. }
        | crate::provider_manifest::schema::MappingStep::DeleteUnlessEquals { .. }
        | crate::provider_manifest::schema::MappingStep::RemoveIfEmpty { .. }
        | crate::provider_manifest::schema::MappingStep::RejectIfPresent { .. }
        | crate::provider_manifest::schema::MappingStep::RejectIfAllPresent { .. }
        | crate::provider_manifest::schema::MappingStep::RebuildJsonTemplate { .. } => None,
    }
}

/// Returns the JSON pointer path this step may delete, or `None` for non-deleting steps.
/// 返回此步骤可能删除的 JSON Pointer 路径；非删除步骤返回 `None`。
fn projection_step_delete_path(
    step: &crate::provider_manifest::schema::MappingStep,
) -> Option<&str> {
    match step {
        crate::provider_manifest::schema::MappingStep::DeleteJsonPointer { path }
        | crate::provider_manifest::schema::MappingStep::DeleteIfEquals { path, .. }
        | crate::provider_manifest::schema::MappingStep::DeleteUnlessEquals { path, .. }
        | crate::provider_manifest::schema::MappingStep::RemoveIfEmpty { path } => Some(path),
        _ => None,
    }
}

/// Returns every typed source read by one closed mapping step.
/// 返回一个封闭映射步骤读取的全部强类型来源。
fn projection_step_sources(
    step: &crate::provider_manifest::schema::MappingStep,
) -> Vec<(crate::provider_manifest::schema::MappingStepSource, &str)> {
    use crate::provider_manifest::schema::{MappingStep, MappingStepSource};
    match step {
        MappingStep::CopyJsonPointer { from, .. } | MappingStep::RenameJsonPointer { from, .. } => {
            vec![(MappingStepSource::BodyJsonPointer, from.as_str())]
        }
        MappingStep::EnumMap {
            source,
            source_path,
            ..
        }
        | MappingStep::ConditionalSet {
            source,
            source_path,
            ..
        }
        | MappingStep::DeleteIfEquals {
            source,
            source_path,
            ..
        }
        | MappingStep::DeleteUnlessEquals {
            source,
            source_path,
            ..
        }
        | MappingStep::RejectIfPresent {
            source,
            source_path,
            ..
        } => vec![(*source, source_path.as_str())],
        MappingStep::RejectIfAllPresent { conditions, .. } => conditions
            .iter()
            .map(|condition| (condition.source, condition.source_path.as_str()))
            .collect(),
        _ => Vec::new(),
    }
}

/// Validates one write path: non-empty, pointer-shaped, and outside protected host roots.
/// 校验一个写入路径：非空、指针形态且不落在受保护宿主根内。
fn validate_projection_target_path(
    kind: &str,
    profile_id: &str,
    target: &str,
    step_path: &str,
    errors: &mut Vec<ManifestValidationError>,
) {
    if !is_valid_json_pointer_path(target) {
        errors.push(ManifestValidationError::new(
            "invalid_projection_pointer",
            step_path,
            format!("{kind} profile `{profile_id}` target `{target}` is not a valid JSON pointer"),
        ));
        return;
    }
    let first_segment = target
        .trim_start_matches('/')
        .split('/')
        .next()
        .unwrap_or("");
    if kind == "request" && PROTECTED_REQUEST_BODY_ROOTS.contains(&first_segment) {
        errors.push(ManifestValidationError::new(
            "protected_request_root",
            step_path,
            format!(
                "{kind} profile `{profile_id}` may not write host-authoritative root `{first_segment}`"
            ),
        ));
    }
}

/// Reports whether one mapping step depends on request-only host context.
/// 报告一个映射步骤是否依赖仅请求阶段可用的宿主上下文。
fn response_projection_step_requires_request_context(
    step: &crate::provider_manifest::schema::MappingStep,
) -> bool {
    match step {
        crate::provider_manifest::schema::MappingStep::StringTemplate { .. }
        | crate::provider_manifest::schema::MappingStep::CopyRequestField { .. } => true,
        crate::provider_manifest::schema::MappingStep::EnumMap { source, .. }
        | crate::provider_manifest::schema::MappingStep::ConditionalSet { source, .. }
        | crate::provider_manifest::schema::MappingStep::DeleteIfEquals { source, .. }
        | crate::provider_manifest::schema::MappingStep::DeleteUnlessEquals { source, .. }
        | crate::provider_manifest::schema::MappingStep::RejectIfPresent { source, .. } => {
            *source == crate::provider_manifest::schema::MappingStepSource::RequestField
        }
        crate::provider_manifest::schema::MappingStep::RejectIfAllPresent {
            conditions, ..
        } => conditions.iter().any(|condition| {
            condition.source == crate::provider_manifest::schema::MappingStepSource::RequestField
        }),
        _ => false,
    }
}

/// Validates one typed projection source without inferring its ownership from path syntax.
/// 校验一个强类型投影来源，不根据路径语法猜测其所有权。
fn validate_projection_source(
    source: crate::provider_manifest::schema::MappingStepSource,
    source_path: &str,
    step_path: &str,
    errors: &mut Vec<ManifestValidationError>,
) {
    match source {
        crate::provider_manifest::schema::MappingStepSource::BodyJsonPointer => {
            if !is_valid_json_pointer_path(source_path) {
                errors.push(ManifestValidationError::new(
                    "invalid_projection_pointer",
                    step_path,
                    format!("read path `{source_path}` is not a valid JSON pointer"),
                ));
            }
        }
        crate::provider_manifest::schema::MappingStepSource::RequestField => {
            let Some(field) = source_path.strip_prefix('@') else {
                errors.push(ManifestValidationError::new(
                    "invalid_request_field_source",
                    step_path,
                    format!("request field source `{source_path}` must start with `@`"),
                ));
                return;
            };
            let allowed = REQUEST_FIELD_ALLOWLIST.contains(&field)
                || field
                    .strip_prefix("model_parameters.")
                    .is_some_and(|parameter_id| !parameter_id.is_empty());
            if !allowed {
                errors.push(ManifestValidationError::new(
                    "unregistered_request_field",
                    step_path,
                    format!("request field `{source_path}` is not on the bounded allowlist"),
                ));
            }
        }
    }
}

/// Reports whether one path is an absolute JSON pointer with at least one segment.
/// 报告一个路径是否为至少含一个分段的绝对 JSON Pointer。
fn is_valid_json_pointer_path(path: &str) -> bool {
    if path.is_empty() || !path.starts_with('/') || path.contains('\0') {
        return false;
    }
    let bytes = path.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'~' {
            if index + 1 >= bytes.len() || !matches!(bytes[index + 1], b'0' | b'1') {
                return false;
            }
            index += 2;
        } else {
            index += 1;
        }
    }
    true
}

/// Rejects any referenced request/response profile whose payload is absent.
/// 拒绝任何被引用但载荷缺失的请求/响应 Profile。
///
/// A route or rule referencing an empty declaration would otherwise silently skip the
/// projection (plan §R8.2: compile-time closure, no silent empty payload).
/// 路由或规则引用空声明会导致投影被静默跳过（方案 §R8.2：编译期闭合，不允许静默空载荷）。
fn validate_referenced_profile_payloads(
    manifest: &ProviderManifestV1,
    errors: &mut Vec<ManifestValidationError>,
) {
    let request_payloads: BTreeSet<&str> = manifest
        .request_profiles
        .iter()
        .filter(|declaration| declaration.payload.is_some())
        .map(|declaration| declaration.profile_id.as_str())
        .collect();
    let response_payloads: BTreeSet<&str> = manifest
        .response_profiles
        .iter()
        .filter(|declaration| declaration.payload.is_some())
        .map(|declaration| declaration.profile_id.as_str())
        .collect();
    for route in &manifest.services {
        for protocol_route in &route.protocol_routes {
            for reference in &protocol_route.request_profile_refs {
                if !request_payloads.contains(reference.as_str()) {
                    errors.push(ManifestValidationError::new(
                        "profile_declared_without_payload",
                        "$.services[].protocol_routes[].request_profile_refs",
                        format!("request profile `{reference}` is referenced but has no payload"),
                    ));
                }
            }
            for reference in &protocol_route.response_profile_refs {
                if !response_payloads.contains(reference.as_str()) {
                    errors.push(ManifestValidationError::new(
                        "profile_declared_without_payload",
                        "$.services[].protocol_routes[].response_profile_refs",
                        format!("response profile `{reference}` is referenced but has no payload"),
                    ));
                }
            }
        }
    }
    for rule in &manifest.model_rules {
        for reference in rule.request_profile_refs.iter().flatten() {
            if !request_payloads.contains(reference.as_str()) {
                errors.push(ManifestValidationError::new(
                    "profile_declared_without_payload",
                    "$.model_rules[].request_profile_refs",
                    format!("request profile `{reference}` is referenced but has no payload"),
                ));
            }
        }
        for reference in rule.response_profile_refs.iter().flatten() {
            if !response_payloads.contains(reference.as_str()) {
                errors.push(ManifestValidationError::new(
                    "profile_declared_without_payload",
                    "$.model_rules[].response_profile_refs",
                    format!("response profile `{reference}` is referenced but has no payload"),
                ));
            }
        }
    }
    for operation in &manifest.operations {
        for reference in &operation.request_profile_refs {
            if !request_payloads.contains(reference.as_str()) {
                errors.push(ManifestValidationError::new(
                    "profile_declared_without_payload",
                    "$.operations[].request_profile_refs",
                    format!("request profile `{reference}` is referenced but has no payload"),
                ));
            }
        }
        for reference in &operation.response_profile_refs {
            if !response_payloads.contains(reference.as_str()) {
                errors.push(ManifestValidationError::new(
                    "profile_declared_without_payload",
                    "$.operations[].response_profile_refs",
                    format!("response profile `{reference}` is referenced but has no payload"),
                ));
            }
        }
    }
}

/// Validates usage profile payloads at compile time (plan §R8.7/§R8d).
/// 在编译期校验 usage Profile 载荷（方案 §R8.7/§R8d）。
///
/// A declared payload must pass the bounded usage extraction validation before freezing, its
/// profile id must match the declaration, and a referenced profile without a payload is
/// rejected instead of silently skipping extraction.
/// 声明的载荷必须在冻结前通过有界 usage 提取校验，其 Profile 标识必须与声明一致，
/// 被引用但无载荷的 Profile 被拒绝而不是静默跳过提取。
fn validate_usage_profile_payloads(
    manifest: &ProviderManifestV1,
    errors: &mut Vec<ManifestValidationError>,
) {
    let mut with_payload = BTreeSet::new();
    let mut payloads = BTreeMap::new();
    for (index, declaration) in manifest.usage_profiles.iter().enumerate() {
        let Some(payload) = declaration.payload.as_ref() else {
            continue;
        };
        with_payload.insert(declaration.profile_id.clone());
        payloads.insert(declaration.profile_id.as_str(), payload);
        if let Err(error) = payload.validate() {
            errors.push(ManifestValidationError::new(
                "invalid_usage_profile_payload",
                format!("$.usage_profiles[{index}].payload"),
                error.to_string(),
            ));
        }
        if payload.profile_id != declaration.profile_id {
            errors.push(ManifestValidationError::new(
                "usage_profile_payload_id_mismatch",
                format!("$.usage_profiles[{index}]"),
                format!(
                    "usage profile payload id `{}` does not match declaration id `{}`",
                    payload.profile_id, declaration.profile_id
                ),
            ));
        }
    }
    for route in &manifest.services {
        for protocol_route in &route.protocol_routes {
            for reference in &protocol_route.usage_profile_refs {
                if !with_payload.contains(reference) {
                    errors.push(ManifestValidationError::new(
                        "profile_declared_without_payload",
                        "$.services[].protocol_routes[].usage_profile_refs",
                        format!("usage profile `{reference}` is referenced but has no payload"),
                    ));
                } else if payloads.get(reference.as_str()).is_some_and(|payload| {
                    !usage_record_kind_matches_protocol(
                        payload.record_kind,
                        protocol_route.protocol_profile_id.as_str(),
                    )
                }) {
                    errors.push(ManifestValidationError::new(
                        "usage_record_kind_protocol_mismatch",
                        "$.services[].protocol_routes[].usage_profile_refs",
                        format!(
                            "usage profile `{reference}` record kind is incompatible with protocol `{}`",
                            protocol_route.protocol_profile_id
                        ),
                    ));
                }
            }
        }
    }
    for rule in &manifest.model_rules {
        for reference in rule.usage_profile_refs.iter().flatten() {
            if !with_payload.contains(reference) {
                errors.push(ManifestValidationError::new(
                    "profile_declared_without_payload",
                    "$.model_rules[].usage_profile_refs",
                    format!("usage profile `{reference}` is referenced but has no payload"),
                ));
            } else if let Some(selected_protocol) = rule.selected_protocol.as_deref()
                && payloads.get(reference.as_str()).is_some_and(|payload| {
                    !usage_record_kind_matches_protocol(payload.record_kind, selected_protocol)
                })
            {
                errors.push(ManifestValidationError::new(
                    "usage_record_kind_protocol_mismatch",
                    "$.model_rules[].usage_profile_refs",
                    format!(
                        "usage profile `{reference}` record kind is incompatible with selected protocol `{selected_protocol}`"
                    ),
                ));
            }
        }
    }
}

/// Reports whether one Usage transport record can be emitted by the selected protocol family.
/// 报告一个用量传输记录是否可能由已选协议族发出。
///
/// `record_kind` is the manifest extraction source and `protocol_profile_id` is the frozen route.
/// `record_kind` 是 Manifest 提取来源，`protocol_profile_id` 是冻结路由。
/// Returns `false` only for closed transport combinations that are structurally impossible.
/// 仅对结构上不可能的封闭传输组合返回 `false`。
fn usage_record_kind_matches_protocol(
    record_kind: vulcan_llm_core::UsageRecordKind,
    protocol_profile_id: &str,
) -> bool {
    match record_kind {
        vulcan_llm_core::UsageRecordKind::WebSocketFrame => matches!(
            protocol_profile_id,
            vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID
                | vulcan_ai_protocols::OPENAI_CODEX_RESPONSES_PROFILE_ID
        ),
        vulcan_llm_core::UsageRecordKind::JsonBody
        | vulcan_llm_core::UsageRecordKind::ErrorJsonBody
        | vulcan_llm_core::UsageRecordKind::SseEvent => true,
    }
}

/// Validates that every referenced operation template exists.
/// 验证所有被引用的操作模板存在。
fn validate_operation_refs(
    manifest: &ProviderManifestV1,
    refs: &[String],
    path: &str,
    errors: &mut Vec<ManifestValidationError>,
) {
    let known: BTreeSet<&str> = manifest
        .operations
        .iter()
        .map(|declaration| declaration.operation_id.as_str())
        .collect();
    for reference in refs {
        if !known.contains(reference.as_str()) {
            errors.push(ManifestValidationError::new(
                "dangling_operation_reference",
                path,
                format!("operation reference `{reference}` is not declared"),
            ));
        }
    }
}

/// Validates compile-time closure across operations, tool targets, and upload channels.
/// 校验操作、工具目标与上传通道之间的编译期引用闭合。
///
/// Every operation's `tool_target_refs`, `upload_channel_refs`, and every tool target's
/// `service_entry_id` / `credential_profile_ids` must resolve inside the same manifest so a
/// compiled manifest never owns a dangling execution path (plan §R9.4).
/// 每个操作的 `tool_target_refs`、`upload_channel_refs` 以及每个工具目标的
/// `service_entry_id` / `credential_profile_ids` 必须在本清单内解析，编译后的清单绝不
/// 持有悬空执行路径（方案 §R9.4）。
fn validate_operation_closure(
    manifest: &ProviderManifestV1,
    ownership: ManifestOwnership,
    registry: &EnginePrimitiveRegistry,
    errors: &mut Vec<ManifestValidationError>,
) {
    let targets: BTreeSet<&str> = manifest
        .tool_targets
        .iter()
        .map(|declaration| declaration.target_id.as_str())
        .collect();
    let channels: BTreeSet<&str> = manifest
        .upload_channels
        .iter()
        .map(|declaration| declaration.upload_channel_id.as_str())
        .collect();
    let entry_ids: BTreeSet<&str> = manifest
        .services
        .iter()
        .map(|entry| entry.service_entry_id.as_str())
        .collect();
    for (index, operation) in manifest.operations.iter().enumerate() {
        if operation.timeout_ms.is_none() {
            errors.push(ManifestValidationError::new(
                "operation_timeout_required",
                format!("$.operations[{index}].timeout_ms"),
                "operation timeout_ms is required so every execution has one absolute deadline",
            ));
        }
        if operation
            .timeout_ms
            .is_some_and(|timeout_ms| !(1_000..=86_400_000).contains(&timeout_ms))
        {
            errors.push(ManifestValidationError::new(
                "operation_timeout_out_of_bounds",
                format!("$.operations[{index}].timeout_ms"),
                "operation timeout must be between 1000 milliseconds and 24 hours",
            ));
        }
        if let Some(async_execution) = operation.async_execution.as_ref() {
            if !(MIN_ASYNC_POLL_INTERVAL_MS..=MAX_ASYNC_POLL_OR_BACKOFF_MS)
                .contains(&async_execution.poll_interval_ms)
            {
                errors.push(ManifestValidationError::new(
                    "async_poll_interval_out_of_bounds",
                    format!("$.operations[{index}].async_execution.poll_interval_ms"),
                    "asynchronous poll interval must be between 250 and 300000 milliseconds",
                ));
            }
            if async_execution.max_backoff_ms < async_execution.poll_interval_ms
                || async_execution.max_backoff_ms > MAX_ASYNC_POLL_OR_BACKOFF_MS
            {
                errors.push(ManifestValidationError::new(
                    "async_backoff_out_of_bounds",
                    format!("$.operations[{index}].async_execution.max_backoff_ms"),
                    "asynchronous maximum backoff must be at least one poll interval and at most 300000 milliseconds",
                ));
            }
            if async_execution.timeout_ms < async_execution.poll_interval_ms
                || async_execution.timeout_ms > MAX_ASYNC_TIMEOUT_MS
            {
                errors.push(ManifestValidationError::new(
                    "async_timeout_out_of_bounds",
                    format!("$.operations[{index}].async_execution.timeout_ms"),
                    "asynchronous timeout must be at least one poll interval and at most 24 hours",
                ));
            }
            if operation
                .timeout_ms
                .is_some_and(|timeout_ms| timeout_ms < async_execution.timeout_ms)
            {
                errors.push(ManifestValidationError::new(
                    "operation_timeout_shorter_than_async_timeout",
                    format!("$.operations[{index}].timeout_ms"),
                    "operation timeout cannot be shorter than its asynchronous lifecycle timeout",
                ));
            }
            if let Some(lifecycle) = async_execution.generic_json.as_ref() {
                validate_generic_json_async_lifecycle(
                    lifecycle,
                    async_execution.cancellation_supported,
                    index,
                    errors,
                );
            }
        }
        for reference in &operation.tool_target_refs {
            if !targets.contains(reference.as_str()) {
                errors.push(ManifestValidationError::new(
                    "dangling_tool_target_reference",
                    format!("$.operations[{index}].tool_target_refs"),
                    format!("tool target reference `{reference}` is not declared"),
                ));
            }
        }
        for reference in &operation.upload_channel_refs {
            if !channels.contains(reference.as_str()) {
                errors.push(ManifestValidationError::new(
                    "dangling_upload_channel_reference",
                    format!("$.operations[{index}].upload_channel_refs"),
                    format!("upload channel reference `{reference}` is not declared"),
                ));
            }
        }
        let operation_channels = operation
            .upload_channel_refs
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        let mut binding_targets = BTreeSet::new();
        for (binding_index, binding) in operation.upload_bindings.iter().enumerate() {
            let binding_path = format!("$.operations[{index}].upload_bindings[{binding_index}]");
            if !is_valid_json_pointer_path(&binding.source_path) {
                errors.push(ManifestValidationError::new(
                    "invalid_upload_binding_source_pointer",
                    format!("{binding_path}.source_path"),
                    format!(
                        "upload binding source `{}` is not an absolute JSON pointer",
                        binding.source_path
                    ),
                ));
            }
            if !is_valid_json_pointer_path(&binding.target_path) {
                errors.push(ManifestValidationError::new(
                    "invalid_upload_binding_target_pointer",
                    format!("{binding_path}.target_path"),
                    format!(
                        "upload binding target `{}` is not an absolute JSON pointer",
                        binding.target_path
                    ),
                ));
            }
            let item_mode =
                binding.item_source_path.is_some() || binding.item_target_path.is_some();
            if binding.item_source_path.is_some() != binding.item_target_path.is_some() {
                errors.push(ManifestValidationError::new(
                    "incoherent_upload_binding_item_paths",
                    binding_path.clone(),
                    "item_source_path and item_target_path must be declared or omitted together",
                ));
            }
            for (field, pointer) in [
                ("item_source_path", binding.item_source_path.as_deref()),
                ("item_target_path", binding.item_target_path.as_deref()),
                ("item_match_path", binding.item_match_path.as_deref()),
            ] {
                if pointer.is_some_and(|pointer| !is_valid_json_pointer_path(pointer)) {
                    errors.push(ManifestValidationError::new(
                        "invalid_upload_binding_item_pointer",
                        format!("{binding_path}.{field}"),
                        format!("upload binding {field} must be an absolute pointer relative to one array item"),
                    ));
                }
            }
            if binding.item_match_path.is_some() != binding.item_match_value.is_some() {
                errors.push(ManifestValidationError::new(
                    "incoherent_upload_binding_item_match",
                    binding_path.clone(),
                    "item_match_path and item_match_value must be declared or omitted together",
                ));
            }
            if !item_mode
                && (binding.item_match_path.is_some() || binding.item_match_value.is_some())
            {
                errors.push(ManifestValidationError::new(
                    "upload_binding_item_match_requires_item_paths",
                    binding_path.clone(),
                    "item matching requires item_source_path and item_target_path",
                ));
            }
            if item_mode && binding.source_path != binding.target_path {
                errors.push(ManifestValidationError::new(
                    "upload_binding_item_collection_mismatch",
                    format!("{binding_path}.target_path"),
                    "item-field upload bindings must update the source array in place",
                ));
            }
            let destination_key = if item_mode {
                format!(
                    "{}#{}#{}#{}",
                    binding.target_path,
                    binding.item_target_path.as_deref().unwrap_or_default(),
                    binding.item_match_path.as_deref().unwrap_or_default(),
                    binding
                        .item_match_value
                        .as_ref()
                        .map(Value::to_string)
                        .unwrap_or_default()
                )
            } else {
                binding.target_path.clone()
            };
            if !binding_targets.insert(destination_key) {
                errors.push(ManifestValidationError::new(
                    "duplicate_upload_binding_target",
                    format!("{binding_path}.target_path"),
                    format!(
                        "multiple upload bindings write target `{}`",
                        binding.target_path
                    ),
                ));
            }
            if binding.upload_channel_refs.is_empty() {
                errors.push(ManifestValidationError::new(
                    "empty_upload_binding_channels",
                    format!("{binding_path}.upload_channel_refs"),
                    "upload binding must reference at least one declared operation channel",
                ));
            }
            for reference in &binding.upload_channel_refs {
                if !operation_channels.contains(reference.as_str()) {
                    errors.push(ManifestValidationError::new(
                        "upload_binding_channel_outside_operation",
                        format!("{binding_path}.upload_channel_refs"),
                        format!(
                            "upload binding channel `{reference}` is not declared by the operation"
                        ),
                    ));
                }
            }
            if binding
                .mime_type
                .as_deref()
                .is_some_and(|mime_type| mime_type.trim().is_empty())
            {
                errors.push(ManifestValidationError::new(
                    "empty_upload_binding_mime",
                    format!("{binding_path}.mime_type"),
                    "upload binding MIME type must not be empty",
                ));
            }
            for (extension, mime_type) in &binding.mime_types_by_extension {
                if extension.is_empty()
                    || extension.starts_with('.')
                    || extension != &extension.to_ascii_lowercase()
                    || !extension
                        .chars()
                        .all(|character| character.is_ascii_alphanumeric())
                    || mime_type.trim().is_empty()
                {
                    errors.push(ManifestValidationError::new(
                        "invalid_upload_binding_extension_mime",
                        format!("{binding_path}.mime_types_by_extension"),
                        format!(
                            "upload binding extension `{extension}` and MIME `{mime_type}` must be lowercase alphanumeric and non-empty"
                        ),
                    ));
                }
            }
        }
    }
    for (index, target) in manifest.tool_targets.iter().enumerate() {
        let owning_operations = manifest
            .operations
            .iter()
            .filter(|operation| {
                operation
                    .tool_target_refs
                    .iter()
                    .any(|reference| reference == &target.target_id)
            })
            .collect::<Vec<_>>();
        if owning_operations.len() != 1 {
            errors.push(ManifestValidationError::new(
                "tool_target_operation_ownership",
                format!("$.tool_targets[{index}].target_id"),
                format!(
                    "tool target `{}` must be referenced by exactly one operation, found {}",
                    target.target_id,
                    owning_operations.len()
                ),
            ));
        } else {
            let operation = owning_operations[0];
            let expected_operation = provider_tool_capability_name(target.capability);
            if operation.operation != expected_operation {
                errors.push(ManifestValidationError::new(
                    "tool_target_operation_capability_mismatch",
                    format!("$.operations.{}.operation", operation.operation_id),
                    format!(
                        "operation `{}` declares `{}` but target `{}` requires `{expected_operation}`",
                        operation.operation_id, operation.operation, target.target_id
                    ),
                ));
            }
            let target_is_async = target.execution.execution_kind
                == crate::ProviderToolExecutionKind::HttpJsonAsyncPoll;
            if target_is_async != operation.async_execution.is_some() {
                errors.push(ManifestValidationError::new(
                    "tool_target_async_operation_mismatch",
                    format!("$.operations.{}.async_execution", operation.operation_id),
                    format!(
                        "operation `{}` and target `{}` disagree about asynchronous execution",
                        operation.operation_id, target.target_id
                    ),
                ));
            }
            let generic_lifecycle = operation
                .async_execution
                .as_ref()
                .and_then(|declaration| declaration.generic_json.as_ref());
            if target_is_async && generic_lifecycle.is_none() {
                errors.push(ManifestValidationError::new(
                    "missing_generic_json_async_lifecycle",
                    format!("$.operations.{}.async_execution", operation.operation_id),
                    format!(
                        "generic JSON asynchronous target `{}` requires a closed lifecycle declaration",
                        target.target_id
                    ),
                ));
            }
        }
        if !target.service_entry_id.is_empty()
            && !entry_ids.contains(target.service_entry_id.as_str())
        {
            errors.push(ManifestValidationError::new(
                "unknown_tool_target_service_entry",
                format!("$.tool_targets[{index}].service_entry_id"),
                format!(
                    "tool target `{}` references unknown service entry `{}`",
                    target.target_id, target.service_entry_id
                ),
            ));
        }
        for profile_id in &target.credential_profile_ids {
            let known = manifest
                .services
                .iter()
                .filter(|entry| {
                    target.service_entry_id.is_empty()
                        || entry.service_entry_id == target.service_entry_id
                })
                .flat_map(|entry| entry.credential_profiles.iter())
                .any(|profile| &profile.credential_profile_id == profile_id);
            if !known {
                errors.push(ManifestValidationError::new(
                    "unknown_tool_target_credential_profile",
                    format!("$.tool_targets[{index}].credential_profile_ids"),
                    format!(
                        "tool target `{}` references unknown credential profile `{profile_id}`",
                        target.target_id
                    ),
                ));
            }
        }
        if !target.capability.is_ai_visible() {
            errors.push(ManifestValidationError::new(
                "tool_target_capability_not_ai_visible",
                format!("$.tool_targets[{index}].capability"),
                format!(
                    "tool target `{}` uses retired non-AI-visible capability `{:?}`",
                    target.target_id, target.capability
                ),
            ));
        }
        let target_path = format!("$.tool_targets[{index}]");
        validate_provider_tool_input_schema(
            &target.input_schema,
            &target.input_numeric_product_ranges,
            target_path.as_str(),
            errors,
        );
        validate_provider_tool_output_contract(
            &target.output,
            format!("{target_path}.output").as_str(),
            errors,
        );
        validate_provider_tool_execution_contract(
            &target.execution,
            format!("{target_path}.execution").as_str(),
            matches!(ownership, ManifestOwnership::System),
            errors,
        );
        let primitive = provider_tool_execution_primitive_id(target.execution.execution_kind);
        if !registry.execution_primitives.contains(primitive) {
            errors.push(ManifestValidationError::new(
                "unknown_execution_primitive",
                format!("$.tool_targets[{index}].execution.execution_kind"),
                format!(
                    "tool target `{}` uses execution primitive `{primitive}` that is not registered by this engine",
                    target.target_id
                ),
            ));
        }
    }
    validate_provider_tool_templates(manifest, ownership, registry, errors);
}

/// Validates endpoint, header, and adapter authority shared by fixed targets and templates.
/// 校验固定目标与模板共享的端点、请求头与适配器权限。
///
/// # Parameters
/// 参数
/// `execution` is the exact contract, `execution_path` scopes diagnostics, `require_https`
/// selects system-provider transport policy, and `errors` accumulates every violation.
/// `execution` 是精确契约，`execution_path` 限定诊断，`require_https` 选择系统供应商传输策略，
/// `errors` 累积每项违规。
///
/// # Returns
/// 返回值
/// The function appends deterministic validation errors and returns no value.
/// 本函数追加确定性校验错误且不返回值。
fn validate_provider_tool_execution_contract(
    execution: &crate::ProviderToolExecutionContract,
    execution_path: &str,
    require_https: bool,
    errors: &mut Vec<ManifestValidationError>,
) {
    if let Some(base_url) = execution.endpoint_base_url.as_deref() {
        validate_http_url(
            base_url,
            format!("{execution_path}.endpoint_base_url").as_str(),
            errors,
            require_https,
        );
    }
    validate_endpoint_path(
        execution.endpoint_path.as_str(),
        format!("{execution_path}.endpoint_path").as_str(),
        errors,
    );
    if execution.execution_kind == crate::ProviderToolExecutionKind::HttpJsonAsyncPoll
        && execution.adapter != crate::ProviderToolAdapterKind::GenericJson
    {
        errors.push(ManifestValidationError::new(
            "async_provider_tool_requires_generic_json_adapter",
            format!("{execution_path}.adapter"),
            "the asynchronous JSON execution family requires the installed generic JSON adapter",
        ));
    }
    validate_provider_tool_fixed_headers(execution, execution_path, errors);
    validate_provider_tool_mcp_http_call(execution, execution_path, errors);
}

/// Revalidates the authority-bearing parts of one dispatch-frozen asynchronous task route.
/// 重新校验一条分派时冻结异步任务路由中承载权限的部分。
///
/// # Parameters
/// 参数
/// `provider_id` selects the system/custom transport policy; `entry`, `target`, and `operation`
/// are the exact non-secret snapshots loaded from durable task recovery storage.
/// `provider_id` 选择系统/自定义传输策略；`entry`、`target` 与 `operation` 是从持久任务恢复
/// 存储加载的精确非秘密快照。
///
/// # Returns
/// 返回值
/// Success only when the recovered URL, endpoint-path, header, adapter, and asynchronous lifecycle
/// authority still satisfies the same validator that admitted the original Manifest.
/// 仅当恢复后的 URL、端点路径、请求头、适配器与异步生命周期权限仍满足最初接纳 Manifest
/// 的同一验证器时返回成功。
pub(crate) fn validate_frozen_async_task_route_contract(
    provider_id: &str,
    entry: &crate::ServiceEntry,
    target: &crate::ProviderToolTarget,
    operation: &crate::provider_manifest::OperationDeclaration,
) -> Result<(), String> {
    // ownership is derived from the namespace rule enforced for every custom provider.
    // ownership 从每个自定义供应商都必须遵守的命名空间规则派生。
    let ownership = if provider_id.starts_with("custom:") {
        ManifestOwnership::Custom
    } else {
        ManifestOwnership::System
    };
    // errors accumulates every independent corruption before the snapshot is rejected.
    // errors 在拒绝快照前收集每项独立损坏。
    let mut errors = Vec::new();
    validate_provider_identifier(provider_id, ownership, &mut errors);
    validate_identifier(
        entry.id.as_str(),
        "$.route.frozen_service_entry.id",
        &mut errors,
        "service_entry_id",
    );
    validate_http_url(
        entry.base_url.as_str(),
        "$.route.frozen_service_entry.base_url",
        &mut errors,
        matches!(ownership, ManifestOwnership::System),
    );
    validate_fixed_headers(
        &entry.request_headers,
        "$.route.frozen_service_entry.request_headers",
        &mut errors,
        "service",
    );
    validate_provider_tool_execution_contract(
        &target.execution,
        "$.route.frozen_target.execution",
        matches!(ownership, ManifestOwnership::System),
        &mut errors,
    );
    if let Some(async_execution) = operation.async_execution.as_ref()
        && let Some(lifecycle) = async_execution.generic_json.as_ref()
    {
        validate_generic_json_async_lifecycle(
            lifecycle,
            async_execution.cancellation_supported,
            0,
            &mut errors,
        );
    }
    if errors.is_empty() {
        return Ok(());
    }
    Err(errors
        .into_iter()
        .map(|error| error.to_string())
        .collect::<Vec<_>>()
        .join("; "))
}

/// Applies the full provider-tool contract validator to every preauthorized target template.
/// 对每个预授权目标模板应用完整供应商工具契约验证器。
///
/// # Parameters
/// 参数
/// `manifest` owns template and operation closure, `ownership` selects transport policy,
/// `registry` is the installed primitive set, and `errors` receives every violation.
/// `manifest` 持有模板与操作闭包，`ownership` 选择传输策略，`registry` 是已安装原语集合，
/// `errors` 接收每项违规。
///
/// # Returns
/// 返回值
/// The function appends every independent template validation error and returns no value.
/// 本函数追加每项独立模板校验错误且不返回值。
fn validate_provider_tool_templates(
    manifest: &ProviderManifestV1,
    ownership: ManifestOwnership,
    registry: &EnginePrimitiveRegistry,
    errors: &mut Vec<ManifestValidationError>,
) {
    for (index, template) in manifest.tool_target_templates.iter().enumerate() {
        let template_path = format!("$.tool_target_templates[{index}]");
        if !template.capability.is_ai_visible() {
            errors.push(ManifestValidationError::new(
                "tool_target_template_capability_not_ai_visible",
                format!("{template_path}.capability"),
                format!(
                    "tool target template `{}` uses retired non-AI-visible capability `{:?}`",
                    template.template_id, template.capability
                ),
            ));
        }
        validate_provider_tool_input_schema(
            &template.input_schema,
            &template.input_numeric_product_ranges,
            template_path.as_str(),
            errors,
        );
        validate_provider_tool_output_contract(
            &template.output,
            format!("{template_path}.output").as_str(),
            errors,
        );
        validate_provider_tool_execution_contract(
            &template.execution,
            format!("{template_path}.execution").as_str(),
            matches!(ownership, ManifestOwnership::System),
            errors,
        );
        let primitive = provider_tool_execution_primitive_id(template.execution.execution_kind);
        if !registry.execution_primitives.contains(primitive) {
            errors.push(ManifestValidationError::new(
                "unknown_execution_primitive",
                format!("{template_path}.execution.execution_kind"),
                format!(
                    "tool target template `{}` uses execution primitive `{primitive}` that is not registered by this engine",
                    template.template_id
                ),
            ));
        }
        for service_entry_id in &template.allowed_service_entry_ids {
            let Some(entry) = manifest
                .services
                .iter()
                .find(|entry| &entry.service_entry_id == service_entry_id)
            else {
                continue;
            };
            for credential_profile_id in &template.credential_profile_ids {
                if !entry
                    .credential_profiles
                    .iter()
                    .any(|profile| &profile.credential_profile_id == credential_profile_id)
                {
                    errors.push(ManifestValidationError::new(
                        "unknown_tool_target_template_credential_profile",
                        format!("{template_path}.credential_profile_ids"),
                        format!(
                            "tool target template `{}` references credential profile `{credential_profile_id}` absent from authorized service entry `{service_entry_id}`",
                            template.template_id
                        ),
                    ));
                }
            }
        }
        let owning_operations = manifest
            .operations
            .iter()
            .filter(|operation| {
                operation
                    .tool_target_template_refs
                    .iter()
                    .any(|template_id| template_id == &template.template_id)
            })
            .collect::<Vec<_>>();
        if let [operation] = owning_operations.as_slice() {
            let expected_operation = provider_tool_capability_name(template.capability);
            if operation.operation != expected_operation {
                errors.push(ManifestValidationError::new(
                    "tool_target_template_operation_capability_mismatch",
                    format!("$.operations.{}.operation", operation.operation_id),
                    format!(
                        "operation `{}` declares `{}` but template `{}` requires `{expected_operation}`",
                        operation.operation_id, operation.operation, template.template_id
                    ),
                ));
            }
            let template_is_async = template.execution.execution_kind
                == crate::ProviderToolExecutionKind::HttpJsonAsyncPoll;
            if template_is_async != operation.async_execution.is_some() {
                errors.push(ManifestValidationError::new(
                    "tool_target_template_async_operation_mismatch",
                    format!("$.operations.{}.async_execution", operation.operation_id),
                    format!(
                        "operation `{}` and template `{}` disagree about asynchronous execution",
                        operation.operation_id, template.template_id
                    ),
                ));
            }
        }
    }
}

/// Validates bounded fixed tool headers and preserves authentication and transport authority.
/// 校验有界工具固定请求头，并保留鉴权与传输权威。
///
/// # Parameters
/// 参数
/// `execution` is the contract under review and `execution_path` scopes deterministic diagnostics.
/// `execution` 是待审核契约，`execution_path` 限定确定性诊断位置。
///
/// # Returns
/// 返回值
/// Validation errors are appended to `errors`; the function has no side effects.
/// 校验错误追加到 `errors`；函数无副作用。
fn validate_provider_tool_fixed_headers(
    execution: &crate::ProviderToolExecutionContract,
    execution_path: &str,
    errors: &mut Vec<ManifestValidationError>,
) {
    const MAX_FIXED_HEADERS: usize = 16;
    const MAX_FIXED_HEADER_VALUE_BYTES: usize = 1024;
    for (field, headers) in [
        ("fixed_headers", &execution.fixed_headers),
        ("start_fixed_headers", &execution.start_fixed_headers),
    ] {
        let base_path = format!("{execution_path}.{field}");
        if headers.len() > MAX_FIXED_HEADERS {
            errors.push(ManifestValidationError::new(
                "too_many_provider_tool_fixed_headers",
                base_path.as_str(),
                format!("provider tool fixed headers cannot exceed {MAX_FIXED_HEADERS} entries"),
            ));
        }
        for (name, value) in headers {
            let lowercase = name.to_ascii_lowercase();
            let forbidden = matches!(
                lowercase.as_str(),
                "authorization"
                    | "proxy-authorization"
                    | "content-type"
                    | "content-length"
                    | "host"
                    | "cookie"
                    | "set-cookie"
                    | "transfer-encoding"
                    | "connection"
            );
            if forbidden || reqwest::header::HeaderName::from_bytes(name.as_bytes()).is_err() {
                errors.push(ManifestValidationError::new(
                    "invalid_provider_tool_fixed_header_name",
                    format!("{base_path}.{name}"),
                    format!("provider tool fixed header `{name}` is invalid or engine-owned"),
                ));
            }
            if value.len() > MAX_FIXED_HEADER_VALUE_BYTES
                || reqwest::header::HeaderValue::from_bytes(value.as_bytes()).is_err()
            {
                errors.push(ManifestValidationError::new(
                    "invalid_provider_tool_fixed_header_value",
                    format!("{base_path}.{name}"),
                    format!(
                        "provider tool fixed header `{name}` must be a valid value within {MAX_FIXED_HEADER_VALUE_BYTES} bytes"
                    ),
                ));
            }
        }
    }
    if execution.execution_kind != crate::ProviderToolExecutionKind::HttpJsonAsyncPoll
        && !execution.start_fixed_headers.is_empty()
    {
        errors.push(ManifestValidationError::new(
            "start_fixed_headers_require_async_execution",
            format!("{execution_path}.start_fixed_headers"),
            "start-only fixed headers require the asynchronous JSON execution family",
        ));
    }
    let shared = execution
        .fixed_headers
        .keys()
        .map(|name| name.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    for name in execution.start_fixed_headers.keys() {
        if shared.contains(name.to_ascii_lowercase().as_str()) {
            errors.push(ManifestValidationError::new(
                "overlapping_provider_tool_fixed_header",
                format!("{execution_path}.start_fixed_headers.{name}"),
                format!("provider tool fixed header `{name}` cannot be declared in both scopes"),
            ));
        }
    }
    if execution.start_conditional_headers.len() > MAX_FIXED_HEADERS {
        errors.push(ManifestValidationError::new(
            "too_many_provider_tool_conditional_headers",
            format!("{execution_path}.start_conditional_headers"),
            format!("provider tool conditional headers cannot exceed {MAX_FIXED_HEADERS} entries"),
        ));
    }
    if execution.execution_kind != crate::ProviderToolExecutionKind::HttpJsonAsyncPoll
        && !execution.start_conditional_headers.is_empty()
    {
        errors.push(ManifestValidationError::new(
            "start_conditional_headers_require_async_execution",
            format!("{execution_path}.start_conditional_headers"),
            "conditional start headers require the asynchronous JSON execution family",
        ));
    }
    let mut declared_names = execution
        .fixed_headers
        .keys()
        .chain(execution.start_fixed_headers.keys())
        .map(|name| name.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    for (rule_index, rule) in execution.start_conditional_headers.iter().enumerate() {
        let rule_path = format!("{execution_path}.start_conditional_headers[{rule_index}]");
        let lowercase = rule.name.to_ascii_lowercase();
        let forbidden = matches!(
            lowercase.as_str(),
            "authorization"
                | "proxy-authorization"
                | "content-type"
                | "content-length"
                | "host"
                | "cookie"
                | "set-cookie"
                | "transfer-encoding"
                | "connection"
        );
        if forbidden
            || reqwest::header::HeaderName::from_bytes(rule.name.as_bytes()).is_err()
            || !declared_names.insert(lowercase)
        {
            errors.push(ManifestValidationError::new(
                "invalid_or_overlapping_provider_tool_conditional_header_name",
                format!("{rule_path}.name"),
                format!(
                    "conditional provider tool header `{}` is invalid, engine-owned, or overlaps another declaration",
                    rule.name
                ),
            ));
        }
        if rule.value.len() > MAX_FIXED_HEADER_VALUE_BYTES
            || reqwest::header::HeaderValue::from_bytes(rule.value.as_bytes()).is_err()
        {
            errors.push(ManifestValidationError::new(
                "invalid_provider_tool_conditional_header_value",
                format!("{rule_path}.value"),
                format!(
                    "conditional provider tool header `{}` must be a valid value within {MAX_FIXED_HEADER_VALUE_BYTES} bytes",
                    rule.name
                ),
            ));
        }
        if rule.when_any.is_empty() || rule.when_any.len() > 8 {
            errors.push(ManifestValidationError::new(
                "invalid_provider_tool_conditional_header_predicate_count",
                format!("{rule_path}.when_any"),
                "conditional provider tool header requires one through eight predicates",
            ));
        }
        for (predicate_index, predicate) in rule.when_any.iter().enumerate() {
            let wildcard_count = predicate
                .pointer_pattern
                .split('/')
                .filter(|segment| *segment == "*")
                .count();
            if !is_valid_json_pointer_path(predicate.pointer_pattern.as_str())
                || wildcard_count > 1
                || predicate.pointer_pattern.split('/').count() > 17
            {
                errors.push(ManifestValidationError::new(
                    "invalid_provider_tool_conditional_header_pointer_pattern",
                    format!("{rule_path}.when_any[{predicate_index}].pointer_pattern"),
                    "conditional header pointer pattern must be absolute, use at most one wildcard, and contain at most 16 segments",
                ));
            }
            if predicate
                .equals
                .as_ref()
                .is_some_and(|value| matches!(value, Value::Array(_) | Value::Object(_)))
            {
                errors.push(ManifestValidationError::new(
                    "invalid_provider_tool_conditional_header_equals",
                    format!("{rule_path}.when_any[{predicate_index}].equals"),
                    "conditional header exact comparison accepts only scalar JSON values",
                ));
            }
        }
    }
}

/// Validates the closed Streamable HTTP MCP contract and its engine-owned authority boundaries.
/// 校验封闭 Streamable HTTP MCP 契约及其由引擎持有的权威边界。
///
/// # Parameters
/// 参数
/// `execution` is the declaration under review, `execution_path` scopes diagnostics, and
/// `errors` receives every deterministic validation failure.
/// `execution` 是待审核声明，`execution_path` 限定诊断位置，`errors` 接收全部确定性校验失败。
///
/// # Returns
/// 返回值
/// The function appends errors without side effects and returns no value.
/// 本函数仅追加错误、没有副作用且不返回值。
fn validate_provider_tool_mcp_http_call(
    execution: &crate::ProviderToolExecutionContract,
    execution_path: &str,
    errors: &mut Vec<ManifestValidationError>,
) {
    let path = format!("{execution_path}.mcp_http_call");
    let is_mcp = execution.execution_kind == crate::ProviderToolExecutionKind::McpHttpCall;
    let Some(contract) = execution.mcp_http_call.as_ref() else {
        if is_mcp {
            errors.push(ManifestValidationError::new(
                "missing_provider_tool_mcp_http_contract",
                path,
                "the MCP HTTP call execution family requires its closed contract",
            ));
        }
        return;
    };
    if !is_mcp {
        errors.push(ManifestValidationError::new(
            "provider_tool_mcp_http_contract_requires_execution_family",
            path.clone(),
            "an MCP HTTP call contract is valid only for the MCP HTTP call execution family",
        ));
    }
    let version = contract.protocol_version.as_bytes();
    if version.len() != 10
        || !version[..4].iter().all(u8::is_ascii_digit)
        || version[4] != b'-'
        || !version[5..7].iter().all(u8::is_ascii_digit)
        || version[7] != b'-'
        || !version[8..].iter().all(u8::is_ascii_digit)
    {
        errors.push(ManifestValidationError::new(
            "invalid_provider_tool_mcp_protocol_version",
            format!("{path}.protocol_version"),
            "MCP protocol_version must use the exact YYYY-MM-DD revision form",
        ));
    }
    if !is_valid_json_pointer_path(contract.content_json_array_pointer.as_str()) {
        errors.push(ManifestValidationError::new(
            "invalid_provider_tool_mcp_content_pointer",
            format!("{path}.content_json_array_pointer"),
            "MCP embedded content selection must be an absolute JSON Pointer",
        ));
    }
    if !(1..=64).contains(&contract.max_content_blocks) {
        errors.push(ManifestValidationError::new(
            "invalid_provider_tool_mcp_content_block_limit",
            format!("{path}.max_content_blocks"),
            "MCP max_content_blocks must be between 1 and 64",
        ));
    }
    if !(1..=1000).contains(&contract.max_items) {
        errors.push(ManifestValidationError::new(
            "invalid_provider_tool_mcp_item_limit",
            format!("{path}.max_items"),
            "MCP max_items must be between 1 and 1000",
        ));
    }
    for header in execution.fixed_headers.keys() {
        if matches!(
            header.to_ascii_lowercase().as_str(),
            "accept" | "mcp-session-id"
        ) {
            errors.push(ManifestValidationError::new(
                "provider_tool_mcp_engine_owned_header",
                format!("{path}.fixed_headers.{header}"),
                format!("MCP request header `{header}` is owned by the execution engine"),
            ));
        }
    }
}

/// Validates one model-visible provider-tool JSON Schema without external resolution.
/// 在不进行外部解析的情况下校验一份模型可见供应商工具 JSON Schema。
///
/// # Parameters
/// 参数
/// `input_schema` and `input_numeric_product_ranges` are the exact frozen contract,
/// `target_path` scopes diagnostics, and `errors` receives every validation failure.
/// `input_schema` 与 `input_numeric_product_ranges` 是精确冻结契约，`target_path` 限定诊断，
/// `errors` 接收每项校验失败。
///
/// # Returns
/// 返回值
/// The function appends errors without resolving external references and returns no value.
/// 本函数在不解析外部引用的情况下追加错误且不返回值。
fn validate_provider_tool_input_schema(
    input_schema: &Value,
    input_numeric_product_ranges: &[crate::ProviderToolNumericProductRange],
    target_path: &str,
    errors: &mut Vec<ManifestValidationError>,
) {
    // The schema path scopes both meta-schema and reference-authority diagnostics.
    // Schema 路径同时限定元 Schema 与引用权威诊断。
    let schema_path = format!("{target_path}.input_schema");
    if let Err(error) = jsonschema::draft202012::meta::validate(input_schema) {
        errors.push(ManifestValidationError::new(
            "invalid_provider_tool_input_schema",
            schema_path.clone(),
            format!("provider tool input schema is not valid Draft 2020-12 JSON Schema: {error}"),
        ));
        return;
    }
    // Provider tools expose one JSON object; scalar or array roots cannot be called safely.
    // 供应商工具暴露一个 JSON 对象；标量或数组根无法被安全调用。
    if input_schema.get("type").and_then(Value::as_str) != Some("object") {
        errors.push(ManifestValidationError::new(
            "provider_tool_input_schema_root_not_object",
            schema_path.clone(),
            "provider tool input schema root must declare `type: object`",
        ));
    }
    if input_numeric_product_ranges.len() > 8 {
        errors.push(ManifestValidationError::new(
            "provider_tool_numeric_product_range_count_exceeded",
            format!("{target_path}.input_numeric_product_ranges"),
            "provider tool targets may declare at most 8 numeric product ranges",
        ));
    }
    let mut product_factor_pairs = BTreeSet::new();
    for (range_index, range) in input_numeric_product_ranges.iter().enumerate() {
        let range_path = format!("{target_path}.input_numeric_product_ranges[{range_index}]");
        if range.maximum == 0 || range.minimum > range.maximum {
            errors.push(ManifestValidationError::new(
                "invalid_provider_tool_numeric_product_bounds",
                range_path.clone(),
                "numeric product bounds must form a non-empty inclusive u64 range",
            ));
        }
        if range.factor_paths[0] == range.factor_paths[1] {
            errors.push(ManifestValidationError::new(
                "duplicate_provider_tool_numeric_product_factor",
                range_path.clone(),
                "numeric product factors must reference two different input fields",
            ));
        }
        if !product_factor_pairs.insert(range.factor_paths.clone()) {
            errors.push(ManifestValidationError::new(
                "duplicate_provider_tool_numeric_product_range",
                range_path.clone(),
                "numeric product factor pairs must be unique within one target",
            ));
        }
        for (factor_index, factor_path) in range.factor_paths.iter().enumerate() {
            let factor_error_path = format!("{range_path}.factor_paths[{factor_index}]");
            if !is_valid_json_pointer_path(factor_path) || factor_path[1..].contains('/') {
                errors.push(ManifestValidationError::new(
                    "invalid_provider_tool_numeric_product_factor_path",
                    factor_error_path,
                    "numeric product factors must be root-level absolute JSON Pointers",
                ));
                continue;
            }
            let property_schema_pointer = format!("/properties{factor_path}");
            if input_schema
                .pointer(property_schema_pointer.as_str())
                .and_then(|schema| schema.get("type"))
                .and_then(Value::as_str)
                != Some("integer")
            {
                errors.push(ManifestValidationError::new(
                    "provider_tool_numeric_product_factor_not_integer",
                    factor_error_path,
                    format!(
                        "numeric product factor `{factor_path}` must reference a root integer property"
                    ),
                ));
            }
        }
    }
    // Recursive reference inspection is bounded by the already-enforced manifest JSON limits.
    // 递归引用检查受到已执行 Manifest JSON 上限约束。
    fn visit_schema_references(
        node: &Value,
        path: &str,
        errors: &mut Vec<ManifestValidationError>,
    ) {
        match node {
            Value::Object(object) => {
                for (key, value) in object {
                    // Only document-local references are permitted; no network or filesystem URI resolves.
                    // 只允许文档本地引用；不会解析网络或文件系统 URI。
                    if matches!(key.as_str(), "$ref" | "$dynamicRef")
                        && value
                            .as_str()
                            .is_none_or(|reference| !reference.starts_with('#'))
                    {
                        errors.push(ManifestValidationError::new(
                            "external_provider_tool_schema_reference",
                            format!("{path}.{key}"),
                            "provider tool input schemas may use only document-local references",
                        ));
                    }
                    visit_schema_references(value, format!("{path}.{key}").as_str(), errors);
                }
            }
            Value::Array(values) => {
                for (index, value) in values.iter().enumerate() {
                    visit_schema_references(value, format!("{path}[{index}]").as_str(), errors);
                }
            }
            _ => {}
        }
    }
    visit_schema_references(input_schema, schema_path.as_str(), errors);
}

/// Validates one provider-tool output contract and every declarative resource extractor.
/// 校验一份供应商工具输出契约及其每个声明式资源提取器。
///
/// # Parameters
/// 参数
/// `output` is one frozen contract, `base_path` scopes JSON paths, and `errors` accumulates every
/// independent structural or authority violation.
/// `output` 是一份冻结契约，`base_path` 限定 JSON 路径，`errors` 累积每个独立的结构或权威违规。
fn validate_provider_tool_output_contract(
    output: &crate::ProviderToolOutputContract,
    base_path: &str,
    errors: &mut Vec<ManifestValidationError>,
) {
    // Extractor and mapping limits remain smaller than the global JSON collection ceiling.
    // 提取器与映射上限保持小于全局 JSON 集合上限。
    const MAX_RESOURCE_DECODERS: usize = 16;
    const MAX_REQUEST_MIME_MAPPINGS: usize = 64;
    const MAX_RESOURCE_BYTES: u64 = 100 * 1024 * 1024;
    // MIME declarations are an exact bounded allowlist shared by every extractor.
    // MIME 声明是由每个提取器共享的精确有界允许列表。
    if output.mime_types.len() > MAX_MIME_TYPES {
        errors.push(ManifestValidationError::new(
            "too_many_provider_tool_output_mime_types",
            format!("{base_path}.mime_types"),
            format!("provider tool output MIME types exceed the {MAX_MIME_TYPES} item limit"),
        ));
    }
    // Duplicate or empty MIME values never create a second fallback authority.
    // 重复或空 MIME 值绝不会形成第二套回退权威。
    let mut allowed_mime_types = BTreeSet::new();
    for (mime_index, mime_type) in output.mime_types.iter().enumerate() {
        if mime_type.trim().is_empty() || !allowed_mime_types.insert(mime_type.as_str()) {
            errors.push(ManifestValidationError::new(
                "invalid_provider_tool_output_mime_type",
                format!("{base_path}.mime_types[{mime_index}]"),
                "provider tool output MIME types must be non-empty and unique",
            ));
        }
    }
    if output.resource_decoders.is_empty() {
        if output.kind == crate::ProviderToolOutputKind::Resources {
            errors.push(ManifestValidationError::new(
                "resource_output_requires_declared_decoder",
                format!("{base_path}.resource_decoders"),
                "resource output contracts require at least one closed resource decoder",
            ));
        }
        return;
    }
    if output.kind != crate::ProviderToolOutputKind::Resources {
        errors.push(ManifestValidationError::new(
            "resource_decoder_output_kind_mismatch",
            format!("{base_path}.kind"),
            "declarative resource decoders require resources output kind",
        ));
    }
    if output.resource_decoders.len() > MAX_RESOURCE_DECODERS {
        errors.push(ManifestValidationError::new(
            "too_many_resource_decoders",
            format!("{base_path}.resource_decoders"),
            format!("resource decoders exceed the {MAX_RESOURCE_DECODERS} item limit"),
        ));
    }
    // Prefixes must remain unique so independently extracted resources receive stable unique IDs.
    // 前缀必须保持唯一，使独立提取的资源获得稳定唯一 ID。
    let mut output_prefixes = BTreeSet::new();
    for (decoder_index, decoder) in output.resource_decoders.iter().enumerate() {
        // Every decoder diagnostic uses its exact array location.
        // 每条解码器诊断都使用其精确数组位置。
        let decoder_path = format!("{base_path}.resource_decoders[{decoder_index}]");
        if !is_valid_json_pointer_path(&decoder.source_path) {
            errors.push(ManifestValidationError::new(
                "invalid_resource_decoder_source_pointer",
                format!("{decoder_path}.source_path"),
                "resource decoder source must be an absolute JSON pointer",
            ));
        }
        if decoder.resource_kind.trim().is_empty()
            || !decoder.resource_kind.chars().all(|character| {
                character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
            })
        {
            errors.push(ManifestValidationError::new(
                "invalid_resource_decoder_kind",
                format!("{decoder_path}.resource_kind"),
                "resource decoder kind must be a non-empty lowercase identifier",
            ));
        }
        // An omitted prefix has the same authority as the literal default `resource`.
        // 省略前缀与字面默认值 `resource` 具有相同权威。
        let output_prefix = decoder.output_id_prefix.as_deref().unwrap_or("resource");
        if output_prefix.is_empty()
            || output_prefix.len() > 64
            || !output_prefix.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
            })
            || !output_prefixes.insert(output_prefix)
        {
            errors.push(ManifestValidationError::new(
                "invalid_resource_decoder_output_prefix",
                format!("{decoder_path}.output_id_prefix"),
                "resource decoder output prefixes must be unique bounded ASCII identifiers",
            ));
        }
        // Inline carriers require a positive declared ceiling; URL carriers cannot invent one.
        // 内联载体要求正数声明上限；URL 载体不能凭空声明该上限。
        let inline_carrier = matches!(
            decoder.encoding,
            crate::ProviderToolResourceEncoding::DataUrl
                | crate::ProviderToolResourceEncoding::Base64
                | crate::ProviderToolResourceEncoding::Hex
        );
        let url_carrier = matches!(
            decoder.encoding,
            crate::ProviderToolResourceEncoding::HttpsUrl
                | crate::ProviderToolResourceEncoding::PublicHttpUrl
        );
        if decoder.download_host_suffixes.len() > 16 {
            errors.push(ManifestValidationError::new(
                "too_many_resource_decoder_download_host_suffixes",
                format!("{decoder_path}.download_host_suffixes"),
                "resource decoders may declare at most 16 download host suffixes",
            ));
        }
        if url_carrier && decoder.download_host_suffixes.is_empty() {
            errors.push(ManifestValidationError::new(
                "url_resource_decoder_requires_download_host_suffix",
                format!("{decoder_path}.download_host_suffixes"),
                "URL resource decoders require at least one reviewed download host suffix",
            ));
        }
        if !url_carrier && !decoder.download_host_suffixes.is_empty() {
            errors.push(ManifestValidationError::new(
                "inline_resource_decoder_has_download_host_suffix",
                format!("{decoder_path}.download_host_suffixes"),
                "inline resource decoders must not declare download host suffixes",
            ));
        }
        for suffix in &decoder.download_host_suffixes {
            if !crate::provider_resource_host_suffix_is_safe(suffix) {
                errors.push(ManifestValidationError::new(
                    "invalid_resource_decoder_download_host_suffix",
                    format!("{decoder_path}.download_host_suffixes"),
                    format!(
                        "resource decoder download host suffix `{suffix}` is not a safe lowercase host matcher"
                    ),
                ));
            }
        }
        if inline_carrier {
            if !decoder
                .max_bytes
                .is_some_and(|maximum| (1..=MAX_RESOURCE_BYTES).contains(&maximum))
            {
                errors.push(ManifestValidationError::new(
                    "invalid_resource_decoder_byte_limit",
                    format!("{decoder_path}.max_bytes"),
                    format!(
                        "inline resource decoder max_bytes must be between 1 and {MAX_RESOURCE_BYTES}"
                    ),
                ));
            }
        } else if decoder.max_bytes.is_some() {
            errors.push(ManifestValidationError::new(
                "url_resource_decoder_has_byte_limit",
                format!("{decoder_path}.max_bytes"),
                "URL resource decoders must not declare max_bytes",
            ));
        }
        // Data URLs carry their own MIME; every other encoding chooses one literal authority.
        // Data URL 自带 MIME；其他编码从一个字面量权威中选择。
        let has_fixed_mime = decoder
            .mime_type
            .as_deref()
            .is_some_and(|mime_type| !mime_type.trim().is_empty());
        let has_request_path = decoder.mime_type_request_path.is_some();
        let has_request_mapping = !decoder.mime_types_by_request_value.is_empty();
        if decoder.encoding == crate::ProviderToolResourceEncoding::DataUrl {
            if has_fixed_mime || has_request_path || has_request_mapping {
                errors.push(ManifestValidationError::new(
                    "data_url_resource_decoder_mime_conflict",
                    decoder_path.clone(),
                    "Data URL resource decoders must use only the embedded MIME authority",
                ));
            }
        } else if has_fixed_mime == (has_request_path && has_request_mapping) {
            errors.push(ManifestValidationError::new(
                "ambiguous_resource_decoder_mime_authority",
                decoder_path.clone(),
                "resource decoder must declare exactly one fixed or request-mapped MIME authority",
            ));
        }
        if decoder.mime_type.is_some() && !has_fixed_mime {
            errors.push(ManifestValidationError::new(
                "empty_resource_decoder_mime_type",
                format!("{decoder_path}.mime_type"),
                "resource decoder fixed MIME type must not be empty",
            ));
        }
        if let Some(mime_type) = decoder.mime_type.as_deref()
            && !allowed_mime_types.contains(mime_type)
        {
            errors.push(ManifestValidationError::new(
                "resource_decoder_mime_outside_contract",
                format!("{decoder_path}.mime_type"),
                format!(
                    "resource decoder MIME `{mime_type}` is not allowed by the output contract"
                ),
            ));
        }
        if let Some(request_path) = decoder.mime_type_request_path.as_deref()
            && !is_valid_json_pointer_path(request_path)
        {
            errors.push(ManifestValidationError::new(
                "invalid_resource_decoder_request_mime_pointer",
                format!("{decoder_path}.mime_type_request_path"),
                "resource decoder request MIME selector must be an absolute JSON pointer",
            ));
        }
        if decoder.mime_types_by_request_value.len() > MAX_REQUEST_MIME_MAPPINGS {
            errors.push(ManifestValidationError::new(
                "too_many_resource_decoder_request_mime_mappings",
                format!("{decoder_path}.mime_types_by_request_value"),
                format!(
                    "resource decoder request MIME mappings exceed the {MAX_REQUEST_MIME_MAPPINGS} item limit"
                ),
            ));
        }
        for (request_value, mime_type) in &decoder.mime_types_by_request_value {
            if request_value.is_empty()
                || request_value.len() > 128
                || mime_type.trim().is_empty()
                || !allowed_mime_types.contains(mime_type.as_str())
            {
                errors.push(ManifestValidationError::new(
                    "invalid_resource_decoder_request_mime_mapping",
                    format!("{decoder_path}.mime_types_by_request_value"),
                    format!(
                        "request MIME mapping `{request_value}` -> `{mime_type}` must be bounded and belong to the output allowlist"
                    ),
                ));
            }
        }
    }
}

/// Validates one closed generic JSON asynchronous lifecycle without contacting its provider.
/// 在不联系供应商的情况下校验一个封闭通用 JSON 异步生命周期。
fn validate_generic_json_async_lifecycle(
    lifecycle: &crate::provider_manifest::GenericJsonAsyncLifecycleDeclaration,
    cancellation_supported: bool,
    operation_index: usize,
    errors: &mut Vec<ManifestValidationError>,
) {
    let base_path = format!("$.operations[{operation_index}].async_execution.generic_json");
    validate_endpoint_path(
        lifecycle.poll_endpoint_path.as_str(),
        format!("{base_path}.poll_endpoint_path").as_str(),
        errors,
    );
    if lifecycle.poll_method == crate::provider_manifest::GenericJsonAsyncRequestMethod::Delete {
        errors.push(ManifestValidationError::new(
            "invalid_generic_async_poll_method",
            format!("{base_path}.poll_method"),
            "generic asynchronous polling cannot use HTTP DELETE",
        ));
    }
    if !is_valid_json_pointer_path(lifecycle.task_id_pointer.as_str()) {
        errors.push(ManifestValidationError::new(
            "invalid_generic_async_task_id_pointer",
            format!("{base_path}.task_id_pointer"),
            "generic asynchronous task id pointer must be absolute",
        ));
    }
    if !is_valid_json_pointer_path(lifecycle.status_pointer.as_str()) {
        errors.push(ManifestValidationError::new(
            "invalid_generic_async_status_pointer",
            format!("{base_path}.status_pointer"),
            "generic asynchronous status pointer must be absolute",
        ));
    }
    match (
        lifecycle.task_id_query_parameter.as_deref(),
        lifecycle.task_id_in_path,
    ) {
        (Some(parameter), false)
            if !parameter.is_empty()
                && parameter
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-')) => {}
        (None, true) => {}
        _ => errors.push(ManifestValidationError::new(
            "invalid_generic_async_task_location",
            format!("{base_path}.task_id_query_parameter"),
            "generic asynchronous task id must use exactly one valid query parameter or encoded path segment",
        )),
    }
    match (
        lifecycle.start_status_pointer.as_deref(),
        lifecycle.start_success_statuses.is_empty(),
    ) {
        (Some(pointer), false) if is_valid_json_pointer_path(pointer) => {}
        (None, true) => {}
        _ => errors.push(ManifestValidationError::new(
            "incoherent_generic_async_start_status",
            format!("{base_path}.start_status_pointer"),
            "start status pointer and non-empty accepted values must be declared or omitted together",
        )),
    }
    let mut start_statuses = BTreeSet::new();
    for status in &lifecycle.start_success_statuses {
        if status.trim().is_empty() || !start_statuses.insert(status.as_str()) {
            errors.push(ManifestValidationError::new(
                "invalid_generic_async_start_status",
                format!("{base_path}.start_success_statuses"),
                format!(
                    "generic asynchronous start status `{status}` must be non-empty and unique"
                ),
            ));
        }
    }
    if lifecycle
        .poll_task_id_pointer
        .as_deref()
        .is_some_and(|pointer| !is_valid_json_pointer_path(pointer))
    {
        errors.push(ManifestValidationError::new(
            "invalid_generic_async_poll_task_id_pointer",
            format!("{base_path}.poll_task_id_pointer"),
            "generic asynchronous poll task id pointer must be absolute",
        ));
    }
    if lifecycle
        .recovery_identity_source_pointer
        .as_deref()
        .is_some_and(|pointer| !is_valid_json_pointer_path(pointer))
    {
        errors.push(ManifestValidationError::new(
            "invalid_generic_async_recovery_identity_pointer",
            format!("{base_path}.recovery_identity_source_pointer"),
            "generic asynchronous recovery identity source pointer must be absolute",
        ));
    }
    for (field, requirements) in [
        ("start_requirements", lifecycle.start_requirements.as_ref()),
        ("poll_requirements", lifecycle.poll_requirements.as_ref()),
        (
            "poll_success_requirements",
            lifecycle.poll_success_requirements.as_ref(),
        ),
        (
            "cancel_requirements",
            lifecycle.cancel_requirements.as_ref(),
        ),
    ] {
        if let Some(requirements) = requirements {
            validate_generic_json_response_requirements(
                requirements,
                format!("{base_path}.{field}").as_str(),
                errors,
            );
        }
    }
    if lifecycle.success_statuses.is_empty() || lifecycle.failure_statuses.is_empty() {
        errors.push(ManifestValidationError::new(
            "incomplete_generic_async_terminal_statuses",
            format!("{base_path}.success_statuses"),
            "generic asynchronous lifecycle requires non-empty success and failure status sets",
        ));
    }
    let mut statuses = BTreeSet::new();
    for (field, values) in [
        ("queued_statuses", &lifecycle.queued_statuses),
        ("running_statuses", &lifecycle.running_statuses),
        ("success_statuses", &lifecycle.success_statuses),
        ("failure_statuses", &lifecycle.failure_statuses),
        ("cancelled_statuses", &lifecycle.cancelled_statuses),
    ] {
        for value in values {
            if value.trim().is_empty() || !statuses.insert(value.as_str()) {
                errors.push(ManifestValidationError::new(
                    "invalid_or_overlapping_generic_async_status",
                    format!("{base_path}.{field}"),
                    format!(
                        "generic asynchronous status `{value}` must be non-empty and belong to exactly one phase"
                    ),
                ));
            }
        }
    }
    if let Some(follow_up) = lifecycle.success_follow_up.as_ref() {
        let follow_up_path = format!("{base_path}.success_follow_up");
        validate_endpoint_path(
            follow_up.endpoint_path.as_str(),
            format!("{follow_up_path}.endpoint_path").as_str(),
            errors,
        );
        if follow_up.method == crate::provider_manifest::GenericJsonAsyncRequestMethod::Delete {
            errors.push(ManifestValidationError::new(
                "invalid_generic_async_follow_up_method",
                format!("{follow_up_path}.method"),
                "generic asynchronous success follow-up cannot use HTTP DELETE",
            ));
        }
        if !is_valid_json_pointer_path(follow_up.identifier_pointer.as_str()) {
            errors.push(ManifestValidationError::new(
                "invalid_generic_async_follow_up_identifier_pointer",
                format!("{follow_up_path}.identifier_pointer"),
                "generic asynchronous success follow-up identifier pointer must be absolute",
            ));
        }
        if follow_up.identifier_query_parameter.is_empty()
            || !follow_up
                .identifier_query_parameter
                .chars()
                .all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '_' | '-')
                })
        {
            errors.push(ManifestValidationError::new(
                "invalid_generic_async_follow_up_query_parameter",
                format!("{follow_up_path}.identifier_query_parameter"),
                "generic asynchronous success follow-up query parameter must use stable ASCII identifier characters",
            ));
        }
    }
    if lifecycle.success_follow_up.is_some() && lifecycle.success_batch_follow_up.is_some() {
        errors.push(ManifestValidationError::new(
            "multiple_generic_async_success_follow_ups",
            format!("{base_path}.success_batch_follow_up"),
            "generic asynchronous lifecycle may declare only one success follow-up mode",
        ));
    }
    if let Some(follow_up) = lifecycle.success_batch_follow_up.as_ref() {
        let follow_up_path = format!("{base_path}.success_batch_follow_up");
        if lifecycle.recovery_identity_source_pointer.is_none() {
            errors.push(ManifestValidationError::new(
                "generic_async_batch_follow_up_requires_recovery_identity",
                follow_up_path.clone(),
                "ordered batch follow-up requires a recovery identity source pointer",
            ));
        }
        for (field, pointer) in [
            ("items_pointer", follow_up.items_pointer.as_str()),
            (
                "item_identity_pointer",
                follow_up.item_identity_pointer.as_str(),
            ),
            (
                "item_status_pointer",
                follow_up.item_status_pointer.as_str(),
            ),
            ("item_url_pointer", follow_up.item_url_pointer.as_str()),
            (
                "response_items_pointer",
                follow_up.response_items_pointer.as_str(),
            ),
            (
                "response_text_pointer",
                follow_up.response_text_pointer.as_str(),
            ),
        ] {
            if !is_valid_json_pointer_path(pointer) {
                errors.push(ManifestValidationError::new(
                    "invalid_generic_async_batch_follow_up_pointer",
                    format!("{follow_up_path}.{field}"),
                    format!("generic asynchronous batch follow-up {field} must be a JSON pointer"),
                ));
            }
        }
        if follow_up.item_success_statuses.is_empty()
            || follow_up
                .item_success_statuses
                .iter()
                .any(|status| status.trim().is_empty())
            || follow_up
                .item_success_statuses
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                != follow_up.item_success_statuses.len()
        {
            errors.push(ManifestValidationError::new(
                "invalid_generic_async_batch_follow_up_statuses",
                format!("{follow_up_path}.item_success_statuses"),
                "batch follow-up item success statuses must be non-empty and unique",
            ));
        }
        if !matches!(
            follow_up.url_encoding,
            crate::ProviderToolResourceEncoding::HttpsUrl
                | crate::ProviderToolResourceEncoding::PublicHttpUrl
        ) {
            errors.push(ManifestValidationError::new(
                "invalid_generic_async_batch_follow_up_url_encoding",
                format!("{follow_up_path}.url_encoding"),
                "batch follow-up URLs must use https_url or public_http_url encoding",
            ));
        }
        if follow_up.url_host_suffixes.is_empty() || follow_up.url_host_suffixes.len() > 16 {
            errors.push(ManifestValidationError::new(
                "invalid_generic_async_batch_follow_up_host_suffix_count",
                format!("{follow_up_path}.url_host_suffixes"),
                "batch follow-up URLs require one through 16 reviewed host suffixes",
            ));
        }
        for suffix in &follow_up.url_host_suffixes {
            if !crate::provider_resource_host_suffix_is_safe(suffix) {
                errors.push(ManifestValidationError::new(
                    "invalid_generic_async_batch_follow_up_host_suffix",
                    format!("{follow_up_path}.url_host_suffixes"),
                    format!(
                        "batch follow-up host suffix `{suffix}` is not a safe lowercase host matcher"
                    ),
                ));
            }
        }
        if !(1..=256).contains(&follow_up.max_items) {
            errors.push(ManifestValidationError::new(
                "invalid_generic_async_batch_follow_up_limit",
                format!("{follow_up_path}.max_items"),
                "batch follow-up max_items must be between one and 256",
            ));
        }
        for (field, separator) in [
            (
                "response_item_separator",
                follow_up.response_item_separator.as_str(),
            ),
            ("result_separator", follow_up.result_separator.as_str()),
        ] {
            if separator.len() > 64 || separator.contains('\0') {
                errors.push(ManifestValidationError::new(
                    "invalid_generic_async_batch_follow_up_separator",
                    format!("{follow_up_path}.{field}"),
                    "batch follow-up separators must contain at most 64 bytes and no NUL",
                ));
            }
        }
    }
    match (
        lifecycle.cancel_endpoint_path.as_deref(),
        lifecycle.cancel_method,
        cancellation_supported,
    ) {
        (Some(path), Some(_), true) => validate_endpoint_path(
            path,
            format!("{base_path}.cancel_endpoint_path").as_str(),
            errors,
        ),
        (None, None, false) => {}
        _ => errors.push(ManifestValidationError::new(
            "incoherent_generic_async_cancellation",
            format!("{base_path}.cancel_endpoint_path"),
            "cancellation flag, endpoint path, and method must be declared or omitted together",
        )),
    }
    if let Some(suffix) = lifecycle.cancel_task_id_path_suffix.as_deref()
        && (!cancellation_supported
            || !lifecycle.task_id_in_path
            || suffix.is_empty()
            || !suffix.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | '~')
            }))
    {
        errors.push(ManifestValidationError::new(
            "invalid_generic_async_cancel_path_suffix",
            format!("{base_path}.cancel_task_id_path_suffix"),
            "cancel task path suffix requires path task ids and one stable path segment",
        ));
    }
    if !cancellation_supported && lifecycle.cancel_requirements.is_some() {
        errors.push(ManifestValidationError::new(
            "generic_async_cancel_requirements_without_cancellation",
            format!("{base_path}.cancel_requirements"),
            "cancel response requirements require a declared cancellation endpoint",
        ));
    }
}

/// Validates bounded non-empty and empty-string response predicates.
/// 校验有界的非空与空字符串响应断言。
///
/// # Parameters
/// 参数
/// `requirements` contains the closed pointer sets, `base_path` scopes diagnostics, and `errors`
/// accumulates every independent violation.
/// `requirements` 包含封闭 Pointer 集合，`base_path` 限定诊断路径，`errors` 累积每项独立违规。
fn validate_generic_json_response_requirements(
    requirements: &crate::provider_manifest::GenericJsonResponseRequirementsDeclaration,
    base_path: &str,
    errors: &mut Vec<ManifestValidationError>,
) {
    const MAX_REQUIREMENT_POINTERS: usize = 16;
    let total = requirements
        .non_empty_string_pointers
        .len()
        .saturating_add(requirements.empty_or_missing_string_pointers.len());
    if total == 0 || total > MAX_REQUIREMENT_POINTERS {
        errors.push(ManifestValidationError::new(
            "invalid_generic_json_response_requirement_count",
            base_path,
            format!(
                "response requirements must contain one through {MAX_REQUIREMENT_POINTERS} pointers"
            ),
        ));
    }
    let mut pointers = BTreeSet::new();
    for (field, values) in [
        (
            "non_empty_string_pointers",
            &requirements.non_empty_string_pointers,
        ),
        (
            "empty_or_missing_string_pointers",
            &requirements.empty_or_missing_string_pointers,
        ),
    ] {
        for pointer in values {
            if !is_valid_json_pointer_path(pointer) || !pointers.insert(pointer.as_str()) {
                errors.push(ManifestValidationError::new(
                    "invalid_or_overlapping_generic_json_response_requirement",
                    format!("{base_path}.{field}"),
                    format!("response requirement pointer `{pointer}` must be absolute and unique"),
                ));
            }
        }
    }
}

/// Returns the canonical manifest operation name for one AI-visible provider-tool capability.
/// 返回一个 AI 可见供应商工具能力对应的规范 Manifest 操作名称。
fn provider_tool_capability_name(capability: crate::ProviderToolCapabilityKind) -> &'static str {
    match capability {
        crate::ProviderToolCapabilityKind::WebSearch => "web_search",
        crate::ProviderToolCapabilityKind::WebExtract => "web_extract",
        crate::ProviderToolCapabilityKind::MediaAnalyze => "media_analyze",
        crate::ProviderToolCapabilityKind::ImageGenerate => "image_generate",
        crate::ProviderToolCapabilityKind::ImageEdit => "image_edit",
        crate::ProviderToolCapabilityKind::VideoGenerate => "video_generate",
        crate::ProviderToolCapabilityKind::VideoEdit => "video_edit",
        crate::ProviderToolCapabilityKind::VideoExtend => "video_extend",
        crate::ProviderToolCapabilityKind::SpeechSynthesize => "speech_synthesize",
        crate::ProviderToolCapabilityKind::SpeechTranscribe => "speech_transcribe",
        crate::ProviderToolCapabilityKind::MusicGenerate => "music_generate",
        crate::ProviderToolCapabilityKind::MusicCover => "music_cover",
        crate::ProviderToolCapabilityKind::EmbeddingCreate => "embedding_create",
        crate::ProviderToolCapabilityKind::RerankDocuments => "rerank_documents",
    }
}

/// Validates one registered protocol identifier against the injected registry.
/// 对照注入的注册表验证一个已注册协议标识。
fn validate_registered_protocol(
    protocol: &str,
    path: &str,
    errors: &mut Vec<ManifestValidationError>,
    registry: &EnginePrimitiveRegistry,
) {
    if !registry.protocol_profiles.contains(protocol) {
        errors.push(ManifestValidationError::new(
            "unknown_protocol_profile",
            path,
            format!("protocol profile `{protocol}` is not registered by this engine"),
        ));
    }
}

/// Validates one stable identifier shape.
/// 验证一个稳定标识形态。
fn validate_identifier(
    value: &str,
    path: &str,
    errors: &mut Vec<ManifestValidationError>,
    field: &str,
) {
    if value.is_empty() {
        errors.push(ManifestValidationError::new(
            "empty_identifier",
            path,
            format!("{field} must not be empty"),
        ));
        return;
    }
    if value.chars().count() > MAX_STRING_LEN {
        errors.push(ManifestValidationError::new(
            "identifier_too_long",
            path,
            format!("{field} exceeds the {} character limit", MAX_STRING_LEN),
        ));
    }
    if !value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.'))
    {
        errors.push(ManifestValidationError::new(
            "invalid_identifier_charset",
            path,
            format!(
                "{field} `{value}` contains characters outside the stable `[A-Za-z0-9._-]` set"
            ),
        ));
    }
}

/// Validates a stable identifier that may embed reviewed provider target syntax.
/// 验证可嵌入已审核供应商目标语法的稳定标识。
///
/// # Parameters
/// 参数
/// `value`, `path`, `errors`, and `field` have the same ownership as `validate_identifier`;
/// this variant additionally permits `:` and `+`, which existing explicit target identities use.
/// `value`、`path`、`errors` 与 `field` 的职责同 `validate_identifier`；该变体额外允许现有
/// 显式目标身份使用的 `:` 与 `+`。
fn validate_extended_identifier(
    value: &str,
    path: &str,
    errors: &mut Vec<ManifestValidationError>,
    field: &str,
) {
    if value.is_empty() {
        errors.push(ManifestValidationError::new(
            "empty_identifier",
            path,
            format!("{field} must not be empty"),
        ));
        return;
    }
    if value.chars().count() > MAX_STRING_LEN {
        errors.push(ManifestValidationError::new(
            "identifier_too_long",
            path,
            format!("{field} exceeds the {} character limit", MAX_STRING_LEN),
        ));
    }
    if !value.chars().all(|character| {
        character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | ':' | '+')
    }) {
        errors.push(ManifestValidationError::new(
            "invalid_identifier_charset",
            path,
            format!(
                "{field} `{value}` contains characters outside the stable `[A-Za-z0-9._:+-]` set"
            ),
        ));
    }
}

/// Validates one provider identifier against the namespace allowed by its manifest ownership.
/// 按 Manifest 所有权允许的命名空间校验一个供应商标识。
fn validate_provider_identifier(
    value: &str,
    ownership: ManifestOwnership,
    errors: &mut Vec<ManifestValidationError>,
) {
    if value.len() > MAX_PROVIDER_IDENTIFIER_LEN {
        errors.push(ManifestValidationError::new(
            "identifier_too_long",
            "provider_id",
            format!(
                "provider_id exceeds the {MAX_PROVIDER_IDENTIFIER_LEN} ASCII-byte filesystem-safe limit"
            ),
        ));
        return;
    }
    if ownership != ManifestOwnership::Custom || !value.starts_with("custom:") {
        validate_identifier(value, "provider_id", errors, "provider_id");
        return;
    }

    let suffix = value
        .strip_prefix("custom:")
        .expect("custom provider prefix was checked above");
    if suffix.is_empty() {
        errors.push(ManifestValidationError::new(
            "empty_identifier",
            "provider_id",
            "custom provider identifier suffix must not be empty",
        ));
        return;
    }
    if !suffix
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.'))
    {
        errors.push(ManifestValidationError::new(
            "invalid_identifier_charset",
            "provider_id",
            format!(
                "provider_id `{value}` must use `custom:` followed by the stable `[A-Za-z0-9._-]` set"
            ),
        ));
    }
}

/// Validates one endpoint path shape.
/// 验证一个端点路径形态。
fn validate_endpoint_path(path: &str, field_path: &str, errors: &mut Vec<ManifestValidationError>) {
    if !path.starts_with('/') || path.starts_with("//") {
        errors.push(ManifestValidationError::new(
            "invalid_endpoint_path",
            field_path,
            "endpoint path must be a same-origin absolute path starting with `/`",
        ));
    }
    // Query strings are legal for existing routes (for example Claude Code's `?beta=true`);
    // fragments, backslashes, and control characters are never legal.
    // 查询串对既有路由合法（例如 Claude Code 的 `?beta=true`）；
    // 片段、反斜杠与控制字符永不合法。
    if path.contains(['#', '\\', '\n', '\r']) {
        errors.push(ManifestValidationError::new(
            "invalid_endpoint_path",
            field_path,
            "endpoint path must not contain fragment, backslash, or control characters",
        ));
    }
}

/// Validates one HTTP(S) URL and its scheme policy.
/// 验证一个 HTTP(S) URL 及其协议策略。
fn validate_http_url(
    candidate: &str,
    path: &str,
    errors: &mut Vec<ManifestValidationError>,
    require_https: bool,
) {
    let parsed = match reqwest::Url::parse(candidate) {
        Ok(parsed) => parsed,
        Err(_) => {
            errors.push(ManifestValidationError::new(
                "invalid_url",
                path,
                format!("`{candidate}` is not a valid HTTP(S) URL"),
            ));
            return;
        }
    };
    let scheme_ok = if require_https {
        parsed.scheme() == "https"
    } else {
        matches!(parsed.scheme(), "http" | "https")
    };
    if !scheme_ok {
        errors.push(ManifestValidationError::new(
            "invalid_url_scheme",
            path,
            if require_https {
                "system manifest URLs must use https"
            } else {
                "custom manifest URLs must use http or https"
            },
        ));
    }
    if parsed.host_str().is_none() {
        errors.push(ManifestValidationError::new(
            "invalid_url",
            path,
            "URL must declare a host",
        ));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        errors.push(ManifestValidationError::new(
            "invalid_url",
            path,
            "URL must not contain embedded credentials",
        ));
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        errors.push(ManifestValidationError::new(
            "invalid_url",
            path,
            "URL must not contain a query or fragment",
        ));
    }
}

/// Validates one HTTPS promotion URL; query strings are legal for referral links.
/// 验证一个 HTTPS 推广 URL；查询串对推广链接合法。
fn validate_promotion_url(candidate: &str, path: &str, errors: &mut Vec<ManifestValidationError>) {
    let parsed = match reqwest::Url::parse(candidate) {
        Ok(parsed) => parsed,
        Err(_) => {
            errors.push(ManifestValidationError::new(
                "invalid_url",
                path,
                format!("`{candidate}` is not a valid HTTP(S) URL"),
            ));
            return;
        }
    };
    if parsed.scheme() != "https" {
        errors.push(ManifestValidationError::new(
            "invalid_url_scheme",
            path,
            "promotion URLs must use https",
        ));
    }
    if parsed.host_str().is_none() {
        errors.push(ManifestValidationError::new(
            "invalid_url",
            path,
            "URL must declare a host",
        ));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        errors.push(ManifestValidationError::new(
            "invalid_url",
            path,
            "URL must not contain embedded credentials",
        ));
    }
    if parsed.fragment().is_some() {
        errors.push(ManifestValidationError::new(
            "invalid_url",
            path,
            "URL must not contain a fragment",
        ));
    }
}

/// Validates fixed non-secret request headers against the forbidden set.
/// 对照禁用集合验证固定非敏感请求头。
fn validate_fixed_headers(
    headers: &std::collections::BTreeMap<String, String>,
    path: &str,
    errors: &mut Vec<ManifestValidationError>,
    owner: &str,
) {
    if headers.len() > MAX_FIXED_HEADERS {
        errors.push(ManifestValidationError::new(
            "too_many_fixed_headers",
            path,
            format!(
                "{owner} exceeds the {} fixed header limit",
                MAX_FIXED_HEADERS
            ),
        ));
    }
    for (name, value) in headers {
        let lower = name.to_ascii_lowercase();
        if FORBIDDEN_MANIFEST_HEADERS.contains(&lower.as_str()) {
            errors.push(ManifestValidationError::new(
                "forbidden_header",
                path,
                format!("{owner} header `{name}` is forbidden in manifests"),
            ));
        }
        if !name.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        }) {
            errors.push(ManifestValidationError::new(
                "invalid_header_name",
                path,
                format!("{owner} header name `{name}` contains invalid characters"),
            ));
        }
        if value.len() > 1_024 || value.chars().any(|character| character.is_control()) {
            errors.push(ManifestValidationError::new(
                "invalid_header_value",
                path,
                format!("{owner} header `{name}` has an invalid value"),
            ));
        }
    }
}

/// Validates response-header names that may be copied into provider request-id telemetry.
/// 验证可复制到供应商请求标识遥测中的响应头名称。
///
/// # Parameters
/// 参数
/// `headers` is the ordered manifest declaration, `path` owns stable diagnostics, and `errors`
/// collects every structural or sensitive-header violation.
/// `headers` 是 Manifest 有序声明，`path` 持有稳定诊断路径，`errors` 收集全部结构或敏感头违规。
fn validate_provider_request_id_headers(
    headers: &[String],
    path: &str,
    errors: &mut Vec<ManifestValidationError>,
) {
    if headers.len() > MAX_REQUEST_ID_HEADERS {
        errors.push(ManifestValidationError::new(
            "too_many_request_id_headers",
            format!("{path}.provider_request_id_headers"),
            format!(
                "route exceeds the {} request-id header limit",
                MAX_REQUEST_ID_HEADERS
            ),
        ));
    }
    // declared_names enforces HTTP's case-insensitive uniqueness rule before runtime lookup.
    // declared_names 在运行时查找前强制执行 HTTP 不区分大小写的唯一性规则。
    let mut declared_names = BTreeSet::new();
    for (index, name) in headers.iter().enumerate() {
        // lowercase is the canonical comparison key for both deny lists and duplicate detection.
        // lowercase 是禁用列表与重复检测共同使用的规范比较键。
        let lowercase = name.to_ascii_lowercase();
        // header_path identifies the exact rejected declaration without exposing any response value.
        // header_path 精确定位被拒声明，且不会暴露任何响应值。
        let header_path = format!("{path}.provider_request_id_headers[{index}]");
        if reqwest::header::HeaderName::from_bytes(name.as_bytes()).is_err() {
            errors.push(ManifestValidationError::new(
                "invalid_request_id_header_name",
                &header_path,
                format!("provider request-id header `{name}` is not a valid HTTP header name"),
            ));
        }
        if FORBIDDEN_MANIFEST_HEADERS.contains(&lowercase.as_str())
            || FORBIDDEN_PROVIDER_REQUEST_ID_RESPONSE_HEADERS.contains(&lowercase.as_str())
        {
            errors.push(ManifestValidationError::new(
                "forbidden_request_id_header",
                &header_path,
                format!(
                    "response header `{name}` may not be copied into provider request-id telemetry"
                ),
            ));
        }
        if !declared_names.insert(lowercase) {
            errors.push(ManifestValidationError::new(
                "duplicate_request_id_header",
                &header_path,
                format!("provider request-id header `{name}` is duplicated case-insensitively"),
            ));
        }
    }
}

/// Validates capability/resource consistency for media operations.
/// 验证媒体操作的能力/资源一致性。
fn validate_capabilities(
    capabilities: &super::schema::ManifestCapabilityPatch,
    path: &str,
    errors: &mut Vec<ManifestValidationError>,
) {
    if let Some(parameters) = capabilities.model_parameters.as_ref() {
        for (parameter_index, parameter) in parameters.iter().enumerate() {
            if let Err(error) = parameter.validate_definition() {
                errors.push(ManifestValidationError::new(
                    "invalid_model_parameter_definition",
                    format!("{path}.model_parameters[{parameter_index}]"),
                    error,
                ));
            }
        }
    }
    // Operation/resource contracts are checked only against the same patch: a dimension the
    // patch declares must satisfy every operation it declares, while an omitted dimension may
    // still be provided by a lower inheritance layer (plan §6.4.5).
    // 操作/资源契约仅在同一补丁内校验：补丁声明过的维度必须满足其声明的全部操作，
    // 而省略的维度仍可能由更低的继承层提供（方案 §6.4.5）。
    let Some(operations) = capabilities.operations.as_ref() else {
        return;
    };
    for operation in operations {
        if let Some((required_inputs, required_outputs)) = operation_resource_contract(*operation) {
            if let Some(input_resources) = capabilities.input_resources.as_ref() {
                for resource in required_inputs {
                    if !input_resources.contains(&resource) {
                        errors.push(ManifestValidationError::new(
                            "capability_operation_resource_mismatch",
                            path,
                            format!(
                                "operation `{}` requires input resource `{:?}`",
                                operation.as_str(),
                                resource
                            ),
                        ));
                    }
                }
            }
            if let Some(output_resources) = capabilities.output_resources.as_ref() {
                for resource in required_outputs {
                    if !output_resources.contains(&resource) {
                        errors.push(ManifestValidationError::new(
                            "capability_operation_resource_mismatch",
                            path,
                            format!(
                                "operation `{}` requires output resource `{:?}`",
                                operation.as_str(),
                                resource
                            ),
                        ));
                    }
                }
            }
        }
    }
}

/// Returns the mandatory input/output resource contract for one media operation.
/// 返回单个媒体操作的强制输入/输出资源契约。
fn operation_resource_contract(
    operation: AiOperation,
) -> Option<(Vec<ResourceKind>, Vec<ResourceKind>)> {
    use AiOperation::*;
    use ResourceKind::{Audio, Image, Video};
    Some(match operation {
        ImageUnderstand => (vec![Image], vec![]),
        ImageGenerate => (vec![], vec![Image]),
        ImageEdit => (vec![Image], vec![Image]),
        VideoUnderstand => (vec![Video], vec![]),
        VideoGenerate => (vec![], vec![Video]),
        VideoEdit => (vec![Video], vec![Video]),
        VideoExtend => (vec![Video], vec![Video]),
        AudioUnderstand => (vec![Audio], vec![]),
        SpeechSynthesize => (vec![], vec![Audio]),
        SpeechTranscribe => (vec![Audio], vec![]),
        VoiceClone => (vec![Audio], vec![Audio]),
        VoiceDesign => (vec![], vec![Audio]),
        MusicGenerate => (vec![], vec![Audio]),
        MusicCover => (vec![Audio], vec![Audio]),
        _ => return None,
    })
}

/// Counts JSON nodes and depth with hard limits.
/// 统计 JSON 节点与深度并施加硬上限。
///
/// The counter accumulates across sibling subtrees so `MAX_JSON_NODES` bounds the whole
/// document instead of only one branch.
/// 计数器在兄弟子树间累计，使 `MAX_JSON_NODES` 限制整份文档而不是仅一条分支。
fn check_bounds(
    value: &Value,
    depth: usize,
    count: &mut usize,
) -> Result<(), Vec<ManifestValidationError>> {
    if depth > MAX_JSON_DEPTH {
        return Err(vec![ManifestValidationError::new(
            "json_too_deep",
            "$",
            format!(
                "manifest exceeds the {} nesting depth limit",
                MAX_JSON_DEPTH
            ),
        )]);
    }
    *count += 1;
    if *count > MAX_JSON_NODES {
        return Err(vec![ManifestValidationError::new(
            "json_too_large",
            "$",
            format!("manifest exceeds the {} node limit", MAX_JSON_NODES),
        )]);
    }
    match value {
        Value::Object(map) => {
            for child in map.values() {
                check_bounds(child, depth + 1, count)?;
            }
        }
        Value::Array(items) => {
            for child in items {
                check_bounds(child, depth + 1, count)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::{
        AccountUsageQueryKind, CredentialBillingKind, CredentialMode,
        provider_manifest::matcher::{MatcherKind, RuleMatcher},
        provider_manifest::schema::{
            EngineContract, ManifestCapabilityPatch, ManifestCatalogModel, ManifestDisplay,
            ManifestModelCatalog, ManifestProvenance, ModelDefaults, ProvenanceStatus,
            ProviderManifestV1, RuleScope,
        },
    };

    /// Builds a minimal valid manifest fixture.
    /// 构建最小有效清单 fixture。
    fn minimal_manifest() -> ProviderManifestV1 {
        ProviderManifestV1 {
            schema_version: MANIFEST_SCHEMA_VERSION.to_string(),
            provider_id: String::from("example"),
            revision: 1,
            published_at: String::from("2026-08-06T00:00:00Z"),
            engine_contract: EngineContract {
                minimum_manifest_engine: 1,
                required_protocol_profiles: vec![String::from("openai-responses.v1")],
                required_execution_primitives: Vec::new(),
                required_upload_primitives: Vec::new(),
                required_transformers: Vec::new(),
            },
            display: ManifestDisplay {
                name: String::from("Example"),
                category: Default::default(),
                credential_directory_visibility: Default::default(),
                promotion: None,
            },
            credential_registration: Default::default(),
            services: vec![crate::provider_manifest::schema::ManifestServiceEntry {
                service_entry_id: String::from("public-api"),
                display_name: String::from("Public API"),
                kind: Default::default(),
                base_url: String::from("https://api.example.invalid/v1"),
                credential_profiles: vec![
                    crate::provider_manifest::schema::ManifestCredentialProfile {
                        credential_profile_id: String::from("api-key"),
                        display_name: String::from("API Key"),
                        credential_mode: CredentialMode::Token,
                        oauth_provider: None,
                        billing_kind: CredentialBillingKind::ProviderManaged,
                        account_usage_query: AccountUsageQueryKind::Unsupported,
                        authentication: None,
                        key_prefix: None,
                        credential_role: None,
                        allowed_usage_scopes: Default::default(),
                    },
                ],
                default_authentication: None,
                request_headers: Default::default(),
                service_origin_override_policy: None,
                native_inference_bindings: Vec::new(),
                request_projector: None,
                model_catalog: Some(ManifestModelCatalog {
                    source_policy: ModelSourcePolicy::ManualOnly,
                    discovery_kind: Some(ModelDiscoveryKind::Manual),
                    path: None,
                    models: None,
                }),
                protocol_routes: vec![crate::provider_manifest::schema::ManifestProtocolRoute {
                    protocol_profile_id: String::from("openai-responses.v1"),
                    endpoint_path: Some(String::from("/responses")),
                    base_url_override: None,
                    authentication: None,
                    request_headers: Default::default(),
                    model_discovery: None,
                    model_discovery_path: None,
                    model_discovery_base_url_override: None,
                    responses_features: None,
                    input_resource_constraints: Vec::new(),
                    usage_profile_refs: Vec::new(),
                    request_profile_refs: Vec::new(),
                    response_profile_refs: Vec::new(),
                    provider_request_id_headers: Vec::new(),
                    verified: true,
                }],
                model_defaults_override: None,
            }],
            model_defaults: ModelDefaults {
                default_enabled: true,
                catalog_membership_policy: Default::default(),
                unknown_model_policy: Default::default(),
                supported_protocols: vec![String::from("openai-responses.v1")],
                selected_protocol: Some(String::from("openai-responses.v1")),
                capabilities: None,
                default_usage_profile_refs: Vec::new(),
                default_request_profile_refs: Vec::new(),
                default_response_profile_refs: Vec::new(),
                default_operation_refs: Vec::new(),
                provenance_ref: None,
            },
            model_rules: Vec::new(),
            usage_profiles: Vec::new(),
            request_profiles: Vec::new(),
            response_profiles: Vec::new(),
            upload_channels: Vec::new(),
            operations: Vec::new(),
            tool_targets: Vec::new(),
            tool_target_templates: Vec::new(),
            tool_target_template_bindings: Vec::new(),
            provenance: ManifestProvenance {
                status: ProvenanceStatus::ExampleOnly,
                sources: Vec::new(),
                verified_at: None,
            },
        }
    }

    fn registry() -> EnginePrimitiveRegistry {
        EnginePrimitiveRegistry::current_protocol_baseline()
    }

    /// Verifies the minimal manifest validates cleanly.
    /// 验证最小清单能干净通过校验。
    #[test]
    fn minimal_manifest_validates() {
        let manifest = minimal_manifest();
        let result = validate_manifest(&manifest, ManifestOwnership::System, &registry());
        assert_eq!(result, Ok(()), "minimal manifest must validate: {result:?}");
    }

    /// Verifies every used protocol must be declared by the manifest engine contract.
    /// 验证每个已使用协议都必须由 Manifest 引擎契约声明。
    #[test]
    fn used_protocol_requires_engine_contract_declaration() {
        let mut manifest = minimal_manifest();
        manifest.engine_contract.required_protocol_profiles.clear();
        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("a used but undeclared protocol must fail");
        assert!(
            errors
                .iter()
                .any(|error| error.code == "engine_contract_missing_used_protocol")
        );
    }

    /// Verifies used execution and mapping primitives cannot hide outside the engine contract.
    /// 验证已使用执行与映射原语不能隐藏在引擎契约之外。
    #[test]
    fn used_execution_and_transformer_require_contract_declarations() {
        let path = vulcan_runtime_assets::source_bundled_provider_manifests_directory()
            .join("tavily.json");
        let text = std::fs::read_to_string(path).expect("Tavily manifest must be readable");
        let manifest: ProviderManifestV1 =
            serde_json::from_str(text.as_str()).expect("Tavily manifest must parse");

        let mut missing_execution = manifest.clone();
        missing_execution
            .engine_contract
            .required_execution_primitives
            .clear();
        let execution_errors =
            validate_manifest(&missing_execution, ManifestOwnership::System, &registry())
                .expect_err("a used but undeclared execution primitive must fail");
        assert!(
            execution_errors
                .iter()
                .any(|error| { error.code == "engine_contract_missing_used_execution_primitive" })
        );

        let mut missing_transformer = manifest;
        missing_transformer
            .engine_contract
            .required_transformers
            .clear();
        let transformer_errors =
            validate_manifest(&missing_transformer, ManifestOwnership::System, &registry())
                .expect_err("a used but undeclared transformer must fail");
        assert!(
            transformer_errors
                .iter()
                .any(|error| error.code == "engine_contract_missing_used_transformer")
        );
    }

    /// Verifies engine-contract arrays are bounded, unique, and stable identifiers.
    /// 验证引擎契约数组有界、唯一且只包含稳定标识。
    #[test]
    fn engine_contract_requirement_lists_are_closed() {
        let mut manifest = minimal_manifest();
        manifest.engine_contract.required_protocol_profiles =
            vec![String::from("openai-responses.v1"); MAX_ENGINE_CONTRACT_REQUIREMENTS + 1];
        manifest.engine_contract.required_execution_primitives = vec![
            String::from("http_json_sync"),
            String::from("http_json_sync"),
        ];
        manifest.engine_contract.required_transformers = vec![String::from("bad/transformer")];

        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("unbounded, duplicate, or unstable engine requirements must fail");
        for expected in [
            "too_many_engine_contract_requirements",
            "duplicate_engine_contract_requirement",
            "invalid_identifier_charset",
        ] {
            assert!(
                errors.iter().any(|error| error.code == expected),
                "missing {expected}: {errors:?}"
            );
        }
    }

    /// Verifies publication and provenance timestamps are real canonical UTC instants.
    /// 验证发布时间与来源验证时间均为真实、标准的 UTC 时刻。
    #[test]
    fn audit_timestamps_require_canonical_utc_rfc3339() {
        let mut manifest = minimal_manifest();
        manifest.published_at = String::from("2026-99-99T00:00:00Z");
        manifest.provenance.verified_at = Some(String::from("not-a-dateZ"));

        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("malformed audit timestamps must fail");
        assert!(
            errors
                .iter()
                .any(|error| error.code == "invalid_published_at")
        );
        assert!(
            errors
                .iter()
                .any(|error| error.code == "invalid_provenance_verified_at")
        );
    }

    /// Verifies a verified manifest cannot claim authority without an exact evidence source.
    /// 验证已验证清单不能在缺少精确证据来源时声明权威。
    #[test]
    fn verified_manifest_requires_evidence_source() {
        let mut manifest = minimal_manifest();
        manifest.provenance.status = ProvenanceStatus::Verified;
        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("verified provenance without evidence must fail");
        assert!(
            errors
                .iter()
                .any(|error| error.code == "verified_manifest_requires_evidence_source")
        );
    }

    /// Verifies model-level evidence references must resolve to an exact top-level source.
    /// 验证模型级证据引用必须精确解析到顶层来源。
    #[test]
    fn model_provenance_reference_must_resolve() {
        let mut manifest = minimal_manifest();
        manifest
            .provenance
            .sources
            .push(crate::provider_manifest::ProvenanceSource {
                url: String::from("https://docs.example.invalid/models"),
                note: String::from("Defines the reviewed model contract."),
            });
        manifest.model_defaults.provenance_ref =
            Some(String::from("https://docs.example.invalid/other"));
        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("a dangling model evidence reference must fail");
        assert!(
            errors
                .iter()
                .any(|error| error.code == "dangling_provenance_reference")
        );
    }

    /// Verifies default binding requires a signed capability snapshot and a verified entry route.
    /// 验证默认绑定要求签名能力快照与已验证入口路由。
    #[test]
    fn default_binding_authority_is_complete_per_entry() {
        let mut manifest = minimal_manifest();
        manifest.model_defaults.unknown_model_policy = UnknownModelPolicy::DefaultBindingAllowed;
        let missing_capabilities =
            validate_manifest(&manifest, ManifestOwnership::System, &registry())
                .expect_err("missing default capabilities must fail");
        assert!(
            missing_capabilities
                .iter()
                .any(|error| error.code == "default_binding_allowed_requires_capabilities")
        );

        manifest.model_defaults.capabilities = Some(ManifestCapabilityPatch {
            operations: Some(vec![vulcan_ai_contract::AiOperation::ConversationRespond]),
            evidence_revision: Some(String::from("validation.default-capabilities.v1")),
            ..ManifestCapabilityPatch::default()
        });
        manifest.services[0].protocol_routes[0].verified = false;
        let unverified_route = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("an unverified default route must fail");
        assert!(
            unverified_route
                .iter()
                .any(|error| error.code == "default_protocol_route_not_verified")
        );
    }

    /// Verifies every Manifest identity needs scoped rule authority under declared-rule policy.
    /// 验证 declared-rule 策略下每个 Manifest 身份都需要作用域规则权威。
    #[test]
    fn non_static_manifest_identity_requires_rule_coverage() {
        let mut manifest = minimal_manifest();
        manifest.services[0].model_catalog = Some(ManifestModelCatalog {
            source_policy: ModelSourcePolicy::ManifestAndDiscovery,
            discovery_kind: Some(ModelDiscoveryKind::OpenAi),
            path: Some(String::from("/models")),
            models: Some(vec![crate::provider_manifest::ManifestCatalogModel {
                model_id: String::from("declared-model"),
                display_name: String::from("Declared Model"),
            }]),
        });
        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("an unbound Manifest identity must fail");
        assert!(
            errors
                .iter()
                .any(|error| error.code == "manifest_model_requires_rule")
        );
    }

    /// Verifies manifest-static discovery requires closed inventory and rule authority.
    /// 验证 Manifest 静态发现要求封闭清单与规则权威。
    #[test]
    fn manifest_static_discovery_requires_inventory_and_rule_coverage() {
        // missing_inventory_manifest declares static authority without its required inventory.
        // missing_inventory_manifest 声明静态权威但缺少必需清单。
        let mut missing_inventory_manifest = minimal_manifest();
        missing_inventory_manifest.services[0].model_catalog = Some(ManifestModelCatalog {
            source_policy: ModelSourcePolicy::ManifestAndDiscovery,
            discovery_kind: Some(ModelDiscoveryKind::ManifestStatic),
            path: None,
            models: None,
        });
        // missing_inventory_errors must identify the exact closed-contract violation.
        // missing_inventory_errors 必须标识精确的封闭契约违规。
        let missing_inventory_errors = validate_manifest(
            &missing_inventory_manifest,
            ManifestOwnership::System,
            &registry(),
        )
        .expect_err("manifest-static discovery without models must fail");
        assert!(
            missing_inventory_errors
                .iter()
                .any(|error| error.code == "manifest_static_requires_catalog_models")
        );

        // missing_rule_manifest adds one identity but deliberately omits its execution rule.
        // missing_rule_manifest 添加一个身份但刻意省略其执行规则。
        let mut missing_rule_manifest = missing_inventory_manifest.clone();
        missing_rule_manifest.services[0]
            .model_catalog
            .as_mut()
            .expect("catalog must exist")
            .models = Some(vec![ManifestCatalogModel {
            model_id: String::from("future-model"),
            display_name: String::from("Future Model"),
        }]);
        // missing_rule_errors proves binary/common defaults cannot authorize a static identity.
        // missing_rule_errors 证明二进制/公共默认不能授权静态身份。
        let missing_rule_errors = validate_manifest(
            &missing_rule_manifest,
            ManifestOwnership::System,
            &registry(),
        )
        .expect_err("manifest-static identity without a rule must fail");
        assert!(
            missing_rule_errors
                .iter()
                .any(|error| error.code == "manifest_model_requires_rule")
        );

        // remote_location_manifest attempts to attach a network path to a non-network catalog.
        // remote_location_manifest 尝试给非网络目录附加网络路径。
        let mut remote_location_manifest = missing_rule_manifest;
        remote_location_manifest.services[0]
            .model_catalog
            .as_mut()
            .expect("catalog must exist")
            .path = Some(String::from("/models"));
        // remote_location_errors proves ManifestStatic cannot disguise a remote request.
        // remote_location_errors 证明 ManifestStatic 不能伪装远程请求。
        let remote_location_errors = validate_manifest(
            &remote_location_manifest,
            ManifestOwnership::System,
            &registry(),
        )
        .expect_err("manifest-static remote location must fail");
        assert!(
            remote_location_errors
                .iter()
                .any(|error| { error.code == "manifest_static_forbids_remote_discovery_location" })
        );
    }

    /// Verifies unknown schema versions are rejected.
    /// 验证未知 Schema 版本被拒绝。
    #[test]
    fn unknown_schema_version_rejected() {
        let mut manifest = minimal_manifest();
        manifest.schema_version = String::from("vulcan.provider-manifest.v2");
        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("unsupported schema must fail");
        assert!(
            errors
                .iter()
                .any(|e| e.code == "unsupported_schema_version")
        );
    }

    /// Verifies revision zero is rejected.
    /// 验证修订号为零被拒绝。
    #[test]
    fn revision_zero_rejected() {
        let mut manifest = minimal_manifest();
        manifest.revision = 0;
        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("zero revision must fail");
        assert!(errors.iter().any(|e| e.code == "revision_must_be_positive"));
    }

    /// Verifies system manifests cannot use the custom prefix.
    /// 验证系统清单不能使用 custom 前缀。
    #[test]
    fn system_manifest_rejects_custom_prefix() {
        let mut manifest = minimal_manifest();
        manifest.provider_id = String::from("custom:example");
        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("system custom prefix must fail");
        assert!(
            errors
                .iter()
                .any(|e| e.code == "system_provider_cannot_use_custom_prefix")
        );
    }

    /// Verifies custom manifests require the custom prefix.
    /// 验证自定义清单必须使用 custom 前缀。
    #[test]
    fn custom_manifest_requires_custom_prefix() {
        let mut manifest = minimal_manifest();
        manifest.provider_id = String::from("example");
        let errors = validate_manifest(&manifest, ManifestOwnership::Custom, &registry())
            .expect_err("custom manifest without prefix must fail");
        assert!(
            errors
                .iter()
                .any(|e| e.code == "custom_provider_requires_custom_prefix")
        );
    }

    /// Verifies custom manifests cannot activate a system credential-onboarding primitive.
    /// 验证自定义清单不能激活系统凭据接入原语。
    #[test]
    fn custom_manifest_requires_direct_credential_registration() {
        let mut manifest = minimal_manifest();
        manifest.provider_id = String::from("custom:example");
        manifest.credential_registration = crate::CredentialRegistrationKind::MiniMaxRegionalApiKey;
        let errors = validate_manifest(&manifest, ManifestOwnership::Custom, &registry())
            .expect_err("custom manifest with system onboarding must fail");
        assert!(errors.iter().any(|error| {
            error.code == "custom_provider_requires_direct_credential_registration"
        }));
    }

    /// Verifies unregistered protocol profiles are rejected.
    /// 验证未注册协议 Profile 被拒绝。
    #[test]
    fn unregistered_protocol_rejected() {
        let mut manifest = minimal_manifest();
        manifest.services[0].protocol_routes[0].protocol_profile_id =
            String::from("unknown-protocol.v9");
        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("unknown protocol must fail");
        assert!(errors.iter().any(|e| e.code == "unknown_protocol_profile"));
    }

    /// Verifies forbidden headers are rejected.
    /// 验证禁用请求头被拒绝。
    #[test]
    fn forbidden_header_rejected() {
        let mut manifest = minimal_manifest();
        manifest.services[0]
            .request_headers
            .insert(String::from("Authorization"), String::from("Bearer x"));
        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("forbidden header must fail");
        assert!(errors.iter().any(|e| e.code == "forbidden_header"));
    }

    /// Verifies request-id response headers are bounded, valid, unique, and non-sensitive.
    /// 验证请求标识响应头有界、有效、唯一且不敏感。
    #[test]
    fn invalid_provider_request_id_headers_are_rejected() {
        // manifest owns one route whose diagnostic header declaration is deliberately malformed.
        // manifest 持有一条诊断响应头声明被刻意破坏的路由。
        let mut manifest = minimal_manifest();
        // route is the exact protocol declaration under semantic validation.
        // route 是接受语义校验的精确协议声明。
        let route = &mut manifest.services[0].protocol_routes[0];
        route.provider_request_id_headers = vec![
            String::from("x-request-id"),
            String::from("X-Request-ID"),
            String::from("set-cookie"),
            String::from("bad header"),
            String::from("x-request-id-2"),
            String::from("x-request-id-3"),
            String::from("x-request-id-4"),
            String::from("x-request-id-5"),
            String::from("x-request-id-6"),
        ];

        // errors contains every independent violation so one bad field never hides another.
        // errors 包含每项独立违规，使单个坏字段不会掩盖其他问题。
        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("invalid request-id headers must fail");
        assert!(
            errors
                .iter()
                .any(|error| error.code == "too_many_request_id_headers")
        );
        assert!(
            errors
                .iter()
                .any(|error| error.code == "invalid_request_id_header_name")
        );
        assert!(
            errors
                .iter()
                .any(|error| error.code == "forbidden_request_id_header")
        );
        assert!(
            errors
                .iter()
                .any(|error| error.code == "duplicate_request_id_header")
        );
    }

    /// Verifies non-HTTPS URLs are rejected for system manifests.
    /// 验证系统清单的非 HTTPS URL 被拒绝。
    #[test]
    fn system_manifest_rejects_http_url() {
        let mut manifest = minimal_manifest();
        manifest.services[0].base_url = String::from("http://api.example.invalid/v1");
        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("http url must fail for system manifest");
        assert!(errors.iter().any(|e| e.code == "invalid_url_scheme"));
    }

    /// Verifies conflicting same-rank rules reject the whole manifest.
    /// 验证同优先级冲突规则拒绝整份清单。
    #[test]
    fn conflicting_same_rank_rules_reject_manifest() {
        let mut manifest = minimal_manifest();
        manifest.model_rules = vec![
            ModelRule {
                rule_id: String::from("r1"),
                scope: Default::default(),
                matcher: RuleMatcher {
                    kind: MatcherKind::Exact,
                    value: String::from("model-a"),
                },
                display_name: None,
                supported_protocols: vec![String::from("openai-responses.v1")],
                selected_protocol: Some(String::from("openai-responses.v1")),
                capabilities: None,
                protocol_capabilities: Default::default(),
                input_resource_constraints: Vec::new(),
                usage_profile_refs: None,
                request_profile_refs: None,
                response_profile_refs: None,
                model_parameters: Vec::new(),
                operation_refs: None,
                allowed_account_plan_names: Vec::new(),
                default_enabled: None,
                provenance_ref: None,
            },
            ModelRule {
                rule_id: String::from("r2"),
                scope: Default::default(),
                matcher: RuleMatcher {
                    kind: MatcherKind::Exact,
                    value: String::from("model-a"),
                },
                display_name: None,
                supported_protocols: vec![String::from("openai-responses.v1")],
                selected_protocol: Some(String::from("openai-responses.v1")),
                capabilities: None,
                protocol_capabilities: Default::default(),
                input_resource_constraints: Vec::new(),
                usage_profile_refs: None,
                request_profile_refs: None,
                response_profile_refs: None,
                model_parameters: Vec::new(),
                operation_refs: None,
                allowed_account_plan_names: Vec::new(),
                default_enabled: None,
                provenance_ref: None,
            },
        ];
        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("conflicting rules must fail");
        assert!(
            errors.iter().any(|e| e.code == "duplicate_rule_authority")
                || errors.iter().any(|e| e.code == "incomparable_rule_scopes"),
            "identical same-rank scopes are duplicate authority: {errors:?}"
        );
    }

    /// Verifies overlapping-but-incomparable same-rank scopes reject the whole manifest.
    /// 验证相交但不可比较的同优先级作用域拒绝整份清单。
    #[test]
    fn incomparable_same_rank_scopes_reject_manifest() {
        let mut manifest = minimal_manifest();
        // A second credential profile and a second entry make the two scopes intersect
        // without containment: r1 owns every profile of public-api, r2 owns api-key of
        // both entries.
        // 第二个凭据配置档与第二个入口使两个作用域相交但不互为子集：
        // r1 拥有 public-api 的全部配置档，r2 拥有两个入口的 api-key。
        manifest.services[0].credential_profiles.push(
            crate::provider_manifest::schema::ManifestCredentialProfile {
                credential_profile_id: String::from("admin-key"),
                display_name: String::from("Admin Key"),
                credential_mode: CredentialMode::Token,
                oauth_provider: None,
                billing_kind: CredentialBillingKind::ProviderManaged,
                account_usage_query: AccountUsageQueryKind::Unsupported,
                authentication: None,
                key_prefix: None,
                credential_role: None,
                allowed_usage_scopes: Default::default(),
            },
        );
        manifest
            .services
            .push(crate::provider_manifest::schema::ManifestServiceEntry {
                service_entry_id: String::from("tools-api"),
                display_name: String::from("Tools API"),
                kind: Default::default(),
                base_url: String::from("https://tools.example.invalid/v1"),
                credential_profiles: vec![
                    crate::provider_manifest::schema::ManifestCredentialProfile {
                        credential_profile_id: String::from("api-key"),
                        display_name: String::from("API Key"),
                        credential_mode: CredentialMode::Token,
                        oauth_provider: None,
                        billing_kind: CredentialBillingKind::ProviderManaged,
                        account_usage_query: AccountUsageQueryKind::Unsupported,
                        authentication: None,
                        key_prefix: None,
                        credential_role: None,
                        allowed_usage_scopes: Default::default(),
                    },
                ],
                default_authentication: None,
                request_headers: Default::default(),
                service_origin_override_policy: None,
                native_inference_bindings: Vec::new(),
                request_projector: None,
                model_catalog: None,
                protocol_routes: Vec::new(),
                model_defaults_override: None,
            });
        manifest.model_rules = vec![
            ModelRule {
                rule_id: String::from("r1"),
                scope: RuleScope {
                    service_entry_ids: vec![String::from("public-api")],
                    credential_profile_ids: Vec::new(),
                },
                matcher: RuleMatcher {
                    kind: MatcherKind::Exact,
                    value: String::from("model-a"),
                },
                display_name: None,
                supported_protocols: vec![String::from("openai-responses.v1")],
                selected_protocol: Some(String::from("openai-responses.v1")),
                capabilities: None,
                protocol_capabilities: Default::default(),
                input_resource_constraints: Vec::new(),
                usage_profile_refs: None,
                request_profile_refs: None,
                response_profile_refs: None,
                model_parameters: Vec::new(),
                operation_refs: None,
                allowed_account_plan_names: Vec::new(),
                default_enabled: None,
                provenance_ref: None,
            },
            ModelRule {
                rule_id: String::from("r2"),
                scope: RuleScope {
                    service_entry_ids: Vec::new(),
                    credential_profile_ids: vec![String::from("api-key")],
                },
                matcher: RuleMatcher {
                    kind: MatcherKind::Exact,
                    value: String::from("model-a"),
                },
                display_name: None,
                supported_protocols: vec![String::from("openai-responses.v1")],
                selected_protocol: Some(String::from("openai-responses.v1")),
                capabilities: None,
                protocol_capabilities: Default::default(),
                input_resource_constraints: Vec::new(),
                usage_profile_refs: None,
                request_profile_refs: None,
                response_profile_refs: None,
                model_parameters: Vec::new(),
                operation_refs: None,
                allowed_account_plan_names: Vec::new(),
                default_enabled: None,
                provenance_ref: None,
            },
        ];
        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("incomparable rules must fail");
        assert!(
            errors.iter().any(|e| e.code == "incomparable_rule_scopes"),
            "overlapping same-rank scopes without a subset relation must fail: {errors:?}"
        );
    }

    /// Verifies a rule protocol missing on the covered entry route is rejected at compile time.
    /// 验证规则协议在覆盖入口路由缺失时编译期拒绝。
    #[test]
    fn rule_protocol_missing_on_entry_route_rejected() {
        let mut manifest = minimal_manifest();
        manifest.model_rules = vec![ModelRule {
            rule_id: String::from("r1"),
            scope: Default::default(),
            matcher: RuleMatcher {
                kind: MatcherKind::Exact,
                value: String::from("model-a"),
            },
            display_name: None,
            supported_protocols: vec![String::from("anthropic-messages.v1")],
            selected_protocol: Some(String::from("anthropic-messages.v1")),
            capabilities: None,
            protocol_capabilities: Default::default(),
            input_resource_constraints: Vec::new(),
            usage_profile_refs: None,
            request_profile_refs: None,
            response_profile_refs: None,
            model_parameters: Vec::new(),
            operation_refs: None,
            allowed_account_plan_names: Vec::new(),
            default_enabled: None,
            provenance_ref: None,
        }];
        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("missing route must fail");
        assert!(
            errors
                .iter()
                .any(|e| e.code == "rule_protocol_missing_on_entry_route")
        );
    }

    /// Verifies unknown fields are rejected during parsing.
    /// 验证解析期间未知字段被拒绝。
    #[test]
    fn unknown_field_rejected_during_parse() {
        let text = r#"{"schema_version":"vulcan.provider-manifest.v1","unknown_top_level":1}"#;
        let errors = parse_and_validate(text, ManifestOwnership::System, &registry())
            .expect_err("unknown field must fail");
        assert!(
            errors
                .iter()
                .any(|e| e.code == "invalid_manifest_structure")
        );
    }

    /// Verifies JSON size limits reject oversized documents.
    /// 验证 JSON 大小限制拒绝超限文档。
    #[test]
    fn oversized_manifest_rejected() {
        let mut text = String::from("{\"padding\":\"");
        text.push_str(&"x".repeat(super::super::MAX_MANIFEST_BYTES + 1));
        text.push_str("\"}");
        let errors = parse_and_validate(&text, ManifestOwnership::System, &registry())
            .expect_err("oversized manifest must fail");
        assert!(errors.iter().any(|e| e.code == "manifest_too_large"));
    }

    /// Verifies that parse_and_validate round-trips a serialized minimal manifest.
    /// 验证 parse_and_validate 能往返序列化的最小清单。
    #[test]
    fn serialized_manifest_round_trips() {
        let manifest = minimal_manifest();
        let text = serde_json::to_string_pretty(&manifest).expect("serialize manifest");
        let parsed = parse_and_validate(&text, ManifestOwnership::System, &registry())
            .expect("round trip must validate");
        assert_eq!(parsed, manifest);
    }

    /// Verifies network-bound upload channels require an explicit safe host and bounded size.
    /// 验证网络绑定上传通道要求显式安全宿主与有界大小。
    #[test]
    fn upload_channels_require_safe_host_and_bounded_size() {
        let valid = serde_json::json!({
            "upload_channel_id": "oss-signed",
            "description": "Alibaba temporary OSS upload",
            "primitive": "signed_object_upload",
            "signing_primitive": "temporary_oss_policy",
            "endpoint_host": "dashscope.aliyuncs.com",
            "endpoint_path": "/api/v1/uploads",
            "response_host_suffixes": [".aliyuncs.com"],
            "method": "GET",
            "accepted_mime_types": ["image/png"],
            "max_bytes": 10485760,
            "max_count": 1,
            "timeout_ms": 60000,
            "reference_encoding": "response_url",
            "progress_supported": true,
            "cancellation_supported": false
        });
        let parsed: super::super::UploadChannelDeclaration =
            serde_json::from_value(valid.clone()).expect("valid upload channel should parse");
        let errors = validate_upload_channel_errors(&parsed);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");

        let mutations: &[(&str, &dyn Fn(serde_json::Value) -> serde_json::Value)] = &[
            ("missing host", &|mut v: serde_json::Value| {
                v.as_object_mut().expect("object").remove("endpoint_host");
                v
            }),
            ("host with port", &|mut v: serde_json::Value| {
                v["endpoint_host"] = serde_json::json!("dashscope.aliyuncs.com:8443");
                v
            }),
            ("host with path", &|mut v: serde_json::Value| {
                v["endpoint_host"] = serde_json::json!("dashscope.aliyuncs.com/v1");
                v
            }),
            ("host with credentials", &|mut v: serde_json::Value| {
                v["endpoint_host"] = serde_json::json!("user@example.com");
                v
            }),
            ("zero max bytes", &|mut v: serde_json::Value| {
                v["max_bytes"] = serde_json::json!(0);
                v
            }),
            ("missing max count", &|mut v: serde_json::Value| {
                v.as_object_mut().expect("object").remove("max_count");
                v
            }),
            ("unsafe endpoint", &|mut v: serde_json::Value| {
                v["endpoint_path"] = serde_json::json!("/uploads?target=https://evil");
                v
            }),
        ];
        for (label, mutate) in mutations {
            let candidate = mutate(valid.clone());
            let parsed_candidate: super::super::UploadChannelDeclaration =
                serde_json::from_value(candidate).expect("channel should parse");
            let errors = validate_upload_channel_errors(&parsed_candidate);
            assert!(
                !errors.is_empty(),
                "{label}: unsafe upload channel must be rejected"
            );
        }
    }

    /// Verifies every network upload channel declares a bounded total timeout.
    /// 验证每个网络上传通道都声明有界总超时。
    #[test]
    fn network_upload_channel_requires_bounded_total_timeout() {
        let declaration: super::super::UploadChannelDeclaration =
            serde_json::from_value(serde_json::json!({
                "upload_channel_id": "provider-upload",
                "description": "Provider upload channel",
                "primitive": "multipart_file",
                "endpoint_host": "api.example.com",
                "endpoint_path": "/v1/uploads",
                "method": "POST",
                "file_field": "file",
                "accepted_mime_types": ["image/png"],
                "max_bytes": 1048576,
                "max_count": 1,
                "response_id_pointer": "/id"
            }))
            .expect("upload channel should parse");
        let errors = validate_upload_channel_errors(&declaration);
        assert!(
            errors
                .iter()
                .any(|code| code == "network_upload_timeout_required"),
            "a network upload channel without a timeout must fail closed: {errors:?}"
        );
    }

    /// Verifies non-network materialization cannot declare a meaningless timeout.
    /// 验证非网络物化不能声明无意义的超时。
    #[test]
    fn non_network_upload_channel_forbids_total_timeout() {
        let declaration: super::super::UploadChannelDeclaration =
            serde_json::from_value(serde_json::json!({
                "upload_channel_id": "inline-data",
                "description": "Inline upload materialization",
                "primitive": "inline_data_url",
                "accepted_mime_types": ["image/png"],
                "max_bytes": 1024,
                "max_count": 1,
                "timeout_ms": 60000
            }))
            .expect("upload channel should parse");
        let errors = validate_upload_channel_errors(&declaration);
        assert!(
            errors
                .iter()
                .any(|code| code == "non_network_upload_forbids_timeout"),
            "a non-network upload channel with a timeout must fail closed: {errors:?}"
        );
    }

    /// Verifies schema-known but unimplemented upload primitives are rejected before activation.
    /// 验证 Schema 已知但尚未实现的上传原语在激活前被拒绝。
    #[test]
    fn unregistered_upload_primitive_fails_closed() {
        let declaration: super::super::UploadChannelDeclaration =
            serde_json::from_value(serde_json::json!({
                "upload_channel_id": "future-presigned",
                "description": "Unimplemented presigned upload",
                "primitive": "presigned_form_upload",
                "endpoint_host": "upload.example.com",
                "endpoint_path": "/v1/presign",
                "accepted_mime_types": ["image/png"],
                "max_bytes": 1024,
                "max_count": 1
            }))
            .expect("schema-known primitive should parse");
        let errors = validate_upload_channel_errors(&declaration);
        assert!(
            errors.iter().any(|code| code == "unknown_upload_primitive"),
            "an unregistered primitive must fail before runtime: {errors:?}"
        );
    }

    /// Verifies remote URL passthrough requires an explicit bounded host allowlist.
    /// 验证远端 URL 直通要求显式且有界的宿主允许列表。
    #[test]
    fn remote_url_passthrough_requires_host_allowlist() {
        let declaration: super::super::UploadChannelDeclaration =
            serde_json::from_value(serde_json::json!({
                "upload_channel_id": "remote-source",
                "description": "Remote source passthrough",
                "primitive": "remote_url_passthrough",
                "accepted_mime_types": ["image/png"],
                "max_count": 1,
                "reference_encoding": "response_url"
            }))
            .expect("remote channel should parse");
        let errors = validate_upload_channel_errors(&declaration);
        assert!(
            errors
                .iter()
                .any(|code| code == "remote_url_upload_requires_host_allowlist"),
            "a remote passthrough without host authority must fail: {errors:?}"
        );
    }

    /// Verifies upload reference templates close every placeholder against response pointers.
    /// 验证上传引用模板的每个占位符都与响应指针闭合。
    #[test]
    fn upload_reference_template_requires_matching_pointer() {
        let declaration: super::super::UploadChannelDeclaration =
            serde_json::from_value(serde_json::json!({
                "upload_channel_id": "multipart-template",
                "description": "Multipart template fixture",
                "primitive": "multipart_file",
                "endpoint_host": "upload.example.com",
                "endpoint_path": "/v1/files",
                "method": "POST",
                "file_field": "file",
                "accepted_mime_types": ["video/mp4"],
                "max_bytes": 1024,
                "max_count": 1,
                "response_id_pointer": "/id",
                "reference_template": "provider://{url}",
                "reference_encoding": "template"
            }))
            .expect("multipart channel should parse");
        let errors = validate_upload_channel_errors(&declaration);
        assert!(
            errors
                .iter()
                .any(|code| code == "upload_template_requires_url_pointer"),
            "a template placeholder without its response pointer must fail: {errors:?}"
        );
    }

    /// Verifies primitives without host-side network I/O do not require an upload endpoint.
    /// 验证不发起宿主侧网络 I/O 的原语不需要上传端点。
    #[test]
    fn non_network_upload_primitives_need_no_endpoint() {
        for (primitive, value) in [
            (
                "inline_data_url",
                serde_json::json!({
                    "upload_channel_id": "inline-data",
                    "description": "Inline upload materialization",
                    "primitive": "inline_data_url",
                    "accepted_mime_types": ["image/png"],
                    "max_bytes": 1024,
                    "max_count": 1
                }),
            ),
            (
                "remote_url_passthrough",
                serde_json::json!({
                    "upload_channel_id": "remote-url",
                    "description": "Remote URL materialization",
                    "primitive": "remote_url_passthrough",
                    "remote_url_host_suffixes": ["example.com"],
                    "accepted_mime_types": ["image/png"],
                    "max_count": 1
                }),
            ),
        ] {
            let parsed: super::super::UploadChannelDeclaration =
                serde_json::from_value(value).expect("non-network channel should parse");
            let errors = validate_upload_channel_errors(&parsed);
            assert!(
                errors.is_empty(),
                "{primitive} produced unexpected errors: {errors:?}"
            );
        }
    }

    /// Verifies an upload channel cannot reference a credential role absent from the manifest.
    /// 验证上传通道不能引用 Manifest 中不存在的凭据角色。
    #[test]
    fn upload_channel_rejects_unknown_credential_role() {
        // The minimal manifest declares one credential profile without an upload role.
        // 最小 Manifest 声明了一个不带上传角色的凭据配置档。
        let declaration: super::super::UploadChannelDeclaration =
            serde_json::from_value(serde_json::json!({
                "upload_channel_id": "provider-upload",
                "description": "Provider upload channel",
                "primitive": "multipart_file",
                "endpoint_host": "api.example.com",
                "endpoint_path": "/v1/uploads",
                "method": "POST",
                "credential_role": "file-upload",
                "file_field": "file",
                "accepted_mime_types": ["image/png"],
                "max_bytes": 1048576,
                "max_count": 1,
                "response_id_pointer": "/id"
            }))
            .expect("upload channel should parse");
        let errors = validate_upload_channel_errors(&declaration);
        assert!(
            errors
                .iter()
                .any(|code| code == "upload_channel_unknown_credential_role"),
            "an unknown credential role must fail closed: {errors:?}"
        );
    }

    /// Verifies declaration identifiers use the stable manifest identifier alphabet.
    /// 验证声明标识使用稳定的 Manifest 标识字符集。
    #[test]
    fn declaration_identifier_rejects_unstable_characters() {
        // A slash would make references ambiguous with paths and must be rejected before compile.
        // 斜杠会使引用与路径产生歧义，必须在编译前拒绝。
        let mut manifest = minimal_manifest();
        manifest.usage_profiles = vec![crate::provider_manifest::schema::UsageProfileDeclaration {
            profile_id: String::from("usage/invalid"),
            description: String::from("invalid identifier fixture"),
            payload: None,
        }];
        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("an unstable declaration identifier must fail");
        assert!(
            errors.iter().any(|error| {
                error.code == "invalid_identifier_charset"
                    && error.path == "$.usage_profiles[0].profile_id"
            }),
            "the exact invalid declaration path must be reported: {errors:?}"
        );
    }

    /// Verifies every object template form rejects keys owned by another form.
    /// 验证每个对象模板形式都会拒绝属于其他形式的键。
    #[test]
    fn json_template_forms_reject_cross_form_keys() {
        let cases = [
            (
                "$vulcan_ref",
                serde_json::json!({"path": "/value", "flatten": true}),
            ),
            (
                "$vulcan_map",
                serde_json::json!({"path": "/values", "template": true, "non_empty": true}),
            ),
            (
                "$vulcan_enum",
                serde_json::json!({"path": "/value", "mapping": {"a": "b"}, "then": true}),
            ),
            (
                "$vulcan_if_equals",
                serde_json::json!({"path": "/value", "equals": true, "optional": true}),
            ),
            (
                "$vulcan_if_present",
                serde_json::json!({"path": "/value", "then": true, "omit_if_empty": true}),
            ),
            (
                "$vulcan_if_array_length",
                serde_json::json!({"path": "/values", "equals": 1, "flatten": true}),
            ),
        ];
        for (tag, specification) in cases {
            let mut errors = Vec::new();
            validate_json_template_form(tag, &specification, 0, "$.template", &mut errors);
            assert!(
                errors
                    .iter()
                    .any(|error| error.code == "unregistered_json_template_form_key"),
                "{tag} must reject cross-form keys: {errors:?}"
            );
        }
    }

    /// Runs the channel validator and returns only error codes.
    /// 运行通道校验器并只返回错误码。
    fn validate_upload_channel_errors(
        declaration: &super::super::UploadChannelDeclaration,
    ) -> Vec<String> {
        let mut errors = Vec::new();
        let mut manifest = minimal_manifest();
        if let Some(primitive) = declaration.primitive {
            manifest
                .engine_contract
                .required_upload_primitives
                .push(upload_primitive_id(primitive).to_string());
        }
        validate_upload_channel(&manifest, declaration, &registry(), &mut errors);
        errors
            .iter()
            .map(|error| error.code.to_string())
            .collect::<Vec<_>>()
    }

    /// Verifies the node counter accumulates across sibling subtrees so a document whose
    /// branches each stay under the limit but whose total exceeds it is rejected.
    /// 验证节点计数器在兄弟子树间累计，使每条分支都低于上限但总量超限的文档被拒绝。
    #[test]
    fn json_node_limit_accumulates_across_sibling_branches() {
        // Two sibling arrays each hold more than half the node budget; per-branch counting
        // would pass, cumulative counting must reject.
        // 两个兄弟数组各持有超过节点预算一半的条目；按分支计数会通过，累计计数必须拒绝。
        let half = MAX_JSON_NODES / 2 + 1;
        let value = serde_json::json!({
            "first": vec![0_u8; half],
            "second": vec![0_u8; half],
        });
        let mut count = 0_usize;
        let result = check_bounds(&value, 0, &mut count);
        assert!(
            matches!(result, Err(errors) if errors.iter().any(|error| error.code == "json_too_large")),
            "the total node count must exceed the hard limit"
        );
        // A single branch at exactly half stays below the limit.
        // 单条分支恰好一半时保持在限制内。
        let value = serde_json::json!({
            "first": vec![0_u8; half],
        });
        let mut count = 0_usize;
        assert!(
            check_bounds(&value, 0, &mut count).is_ok(),
            "a document under the limit must pass"
        );
    }

    /// Builds a minimal manifest whose only protocol route references one request profile.
    /// 构建一条仅协议路由引用一个请求 Profile 的最小清单。
    fn manifest_with_request_profile(
        declaration: crate::provider_manifest::schema::RequestProfileDeclaration,
    ) -> ProviderManifestV1 {
        let mut manifest = minimal_manifest();
        manifest.request_profiles = vec![declaration];
        manifest.services[0].protocol_routes[0].request_profile_refs =
            vec![String::from("test.request.v1")];
        manifest
    }

    /// Verifies a projection step may never write a host-authoritative root (plan §R8.3).
    /// 验证投影步骤绝不能写入宿主权威根（方案 §R8.3）。
    #[test]
    fn projection_step_cannot_write_protected_request_root() {
        let manifest = manifest_with_request_profile(
            crate::provider_manifest::schema::RequestProfileDeclaration {
                profile_id: String::from("test.request.v1"),
                description: String::from("test"),
                payload: Some(crate::provider_manifest::schema::RequestProjectionProfile {
                    profile_id: String::from("test.request.v1"),
                    revision: 1,
                    steps: vec![crate::provider_manifest::schema::MappingStep::SetLiteral {
                        path: String::from("/model"),
                        value: serde_json::json!("overwritten"),
                    }],
                }),
            },
        );
        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("protected root must fail");
        assert!(
            errors
                .iter()
                .any(|error| error.code == "protected_request_root"),
            "protected root write must be rejected: {errors:?}"
        );
    }

    /// Verifies the payload profile id must match its declaration id.
    /// 验证载荷 Profile 标识必须与声明标识一致。
    #[test]
    fn projection_payload_id_mismatch_rejected() {
        let manifest = manifest_with_request_profile(
            crate::provider_manifest::schema::RequestProfileDeclaration {
                profile_id: String::from("test.request.v1"),
                description: String::from("test"),
                payload: Some(crate::provider_manifest::schema::RequestProjectionProfile {
                    profile_id: String::from("other.request.v1"),
                    revision: 1,
                    steps: vec![crate::provider_manifest::schema::MappingStep::SetLiteral {
                        path: String::from("/temperature"),
                        value: serde_json::json!(0.2),
                    }],
                }),
            },
        );
        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("payload id mismatch must fail");
        assert!(
            errors
                .iter()
                .any(|error| error.code == "profile_payload_id_mismatch"),
            "payload id mismatch must be rejected: {errors:?}"
        );
    }

    /// Verifies a referenced profile without a payload is rejected at compile time.
    /// 验证被引用但无载荷的 Profile 在编译期被拒绝。
    #[test]
    fn referenced_profile_without_payload_rejected() {
        let manifest = manifest_with_request_profile(
            crate::provider_manifest::schema::RequestProfileDeclaration {
                profile_id: String::from("test.request.v1"),
                description: String::from("test"),
                payload: None,
            },
        );
        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("empty payload must fail");
        assert!(
            errors
                .iter()
                .any(|error| error.code == "profile_declared_without_payload"),
            "empty payload reference must be rejected: {errors:?}"
        );
    }

    /// Verifies duplicate write targets and invalid pointers are rejected.
    /// 验证重复写入目标与非法指针被拒绝。
    #[test]
    fn duplicate_projection_target_and_invalid_pointer_rejected() {
        let manifest = manifest_with_request_profile(
            crate::provider_manifest::schema::RequestProfileDeclaration {
                profile_id: String::from("test.request.v1"),
                description: String::from("test"),
                payload: Some(crate::provider_manifest::schema::RequestProjectionProfile {
                    profile_id: String::from("test.request.v1"),
                    revision: 1,
                    steps: vec![
                        crate::provider_manifest::schema::MappingStep::SetLiteral {
                            path: String::from("/temperature"),
                            value: serde_json::json!(0.1),
                        },
                        crate::provider_manifest::schema::MappingStep::SetLiteral {
                            path: String::from("/temperature"),
                            value: serde_json::json!(0.2),
                        },
                        crate::provider_manifest::schema::MappingStep::SetLiteral {
                            path: String::from("temperature"),
                            value: serde_json::json!(0.3),
                        },
                    ],
                }),
            },
        );
        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("duplicate and invalid targets must fail");
        assert!(
            errors
                .iter()
                .any(|error| error.code == "duplicate_projection_target"),
            "duplicate target must be rejected: {errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|error| error.code == "invalid_projection_pointer"),
            "invalid pointer must be rejected: {errors:?}"
        );
    }

    /// Verifies an unregistered request field is rejected.
    /// 验证未注册的请求字段被拒绝。
    #[test]
    fn unregistered_request_field_rejected() {
        let manifest = manifest_with_request_profile(
            crate::provider_manifest::schema::RequestProfileDeclaration {
                profile_id: String::from("test.request.v1"),
                description: String::from("test"),
                payload: Some(crate::provider_manifest::schema::RequestProjectionProfile {
                    profile_id: String::from("test.request.v1"),
                    revision: 1,
                    steps: vec![
                        crate::provider_manifest::schema::MappingStep::CopyRequestField {
                            path: String::from("/secret_parameter"),
                            field: String::from("authorization_token"),
                        },
                    ],
                }),
            },
        );
        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("unregistered field must fail");
        assert!(
            errors
                .iter()
                .any(|error| error.code == "unregistered_request_field"),
            "unregistered request field must be rejected: {errors:?}"
        );
    }

    /// Verifies typed request-field sources require the explicit host-field marker.
    /// 验证强类型请求字段来源必须带显式宿主字段标记。
    #[test]
    fn typed_request_field_source_requires_marker() {
        let manifest = manifest_with_request_profile(
            crate::provider_manifest::schema::RequestProfileDeclaration {
                profile_id: String::from("test.request.v1"),
                description: String::from("test"),
                payload: Some(crate::provider_manifest::schema::RequestProjectionProfile {
                    profile_id: String::from("test.request.v1"),
                    revision: 1,
                    steps: vec![
                        crate::provider_manifest::schema::MappingStep::RejectIfPresent {
                            source:
                                crate::provider_manifest::schema::MappingStepSource::RequestField,
                            source_path: String::from("reasoning_effort"),
                            message: String::from("test rejection"),
                        },
                    ],
                }),
            },
        );

        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("an unmarked request-field source must fail");

        assert!(
            errors
                .iter()
                .any(|error| error.code == "invalid_request_field_source"),
            "an unmarked request-field source must be rejected: {errors:?}"
        );
    }

    /// Verifies typed request-field sources remain restricted to the bounded registry.
    /// 验证强类型请求字段来源仍受有界注册表限制。
    #[test]
    fn typed_request_field_source_rejects_unregistered_field() {
        let manifest = manifest_with_request_profile(
            crate::provider_manifest::schema::RequestProfileDeclaration {
                profile_id: String::from("test.request.v1"),
                description: String::from("test"),
                payload: Some(crate::provider_manifest::schema::RequestProjectionProfile {
                    profile_id: String::from("test.request.v1"),
                    revision: 1,
                    steps: vec![
                        crate::provider_manifest::schema::MappingStep::RejectIfPresent {
                            source:
                                crate::provider_manifest::schema::MappingStepSource::RequestField,
                            source_path: String::from("@authorization_token"),
                            message: String::from("test rejection"),
                        },
                    ],
                }),
            },
        );

        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("an unregistered request-field source must fail");

        assert!(
            errors
                .iter()
                .any(|error| error.code == "unregistered_request_field"),
            "an unregistered request-field source must be rejected: {errors:?}"
        );
    }

    /// Verifies all-present guards have bounded, unique typed conditions.
    /// 验证全部存在守卫只接受有界且唯一的强类型条件。
    #[test]
    fn reject_if_all_present_conditions_are_bounded_and_unique() {
        let duplicate_condition = crate::provider_manifest::schema::MappingPresenceCondition {
            source: crate::provider_manifest::schema::MappingStepSource::RequestField,
            source_path: String::from("@reasoning_effort"),
        };
        let manifest = manifest_with_request_profile(
            crate::provider_manifest::schema::RequestProfileDeclaration {
                profile_id: String::from("test.request.v1"),
                description: String::from("test"),
                payload: Some(crate::provider_manifest::schema::RequestProjectionProfile {
                    profile_id: String::from("test.request.v1"),
                    revision: 1,
                    steps: vec![
                        crate::provider_manifest::schema::MappingStep::RejectIfAllPresent {
                            conditions: vec![duplicate_condition.clone()],
                            message: String::from("too few conditions"),
                        },
                        crate::provider_manifest::schema::MappingStep::RejectIfAllPresent {
                            conditions: vec![duplicate_condition.clone(), duplicate_condition],
                            message: String::from("duplicate conditions"),
                        },
                    ],
                }),
            },
        );

        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("invalid all-present guards must fail");

        assert!(
            errors
                .iter()
                .any(|error| error.code == "presence_condition_count_out_of_bounds"),
            "an undersized guard must be rejected: {errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|error| error.code == "duplicate_presence_condition"),
            "duplicate guard conditions must be rejected: {errors:?}"
        );
    }

    /// Verifies response profiles cannot depend on host request fields.
    /// 验证响应 Profile 不能依赖宿主请求字段。
    #[test]
    fn response_profile_request_context_is_rejected() {
        let mut manifest = minimal_manifest();
        manifest.response_profiles = vec![
            crate::provider_manifest::schema::ResponseProfileDeclaration {
                profile_id: String::from("test.response.v1"),
                description: String::from("test"),
                payload: Some(
                    crate::provider_manifest::schema::ResponseProjectionProfile {
                        profile_id: String::from("test.response.v1"),
                        revision: 1,
                        steps: vec![
                            crate::provider_manifest::schema::MappingStep::CopyRequestField {
                                path: String::from("/model"),
                                field: String::from("model_id"),
                            },
                        ],
                    },
                ),
            },
        ];
        manifest.services[0].protocol_routes[0].response_profile_refs =
            vec![String::from("test.response.v1")];

        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("response request context must fail");
        assert!(
            errors
                .iter()
                .any(|error| error.code == "response_profile_requires_request_context"),
            "response request context must be rejected: {errors:?}"
        );
    }

    /// Verifies an invalid usage payload is rejected at compile time.
    /// 验证无效 usage 载荷在编译期被拒绝。
    #[test]
    fn invalid_usage_payload_rejected_at_compile_time() {
        let mut manifest = minimal_manifest();
        manifest.usage_profiles = vec![crate::provider_manifest::schema::UsageProfileDeclaration {
            profile_id: String::from("test.usage.v1"),
            description: String::from("test"),
            payload: Some(vulcan_llm_core::UsageExtractionProfile {
                profile_id: String::from("test.usage.v1"),
                revision: 1,
                record_kind: vulcan_llm_core::UsageRecordKind::JsonBody,
                update_kind: vulcan_ai_contract::UsageUpdateKind::Snapshot,
                event_type_pointer: Some(String::from("not-a-pointer")),
                event_type_value: Some(String::from("response.completed")),
                required_pointer: None,
                bindings: Vec::new(),
                derivations: Vec::new(),
            }),
        }];
        manifest.services[0].protocol_routes[0].usage_profile_refs =
            vec![String::from("test.usage.v1")];
        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("invalid usage payload must fail");
        assert!(
            errors
                .iter()
                .any(|error| error.code == "invalid_usage_profile_payload"),
            "invalid usage payload must be rejected: {errors:?}"
        );
    }

    /// Verifies a WebSocket usage record cannot be attached to a non-Responses protocol route.
    /// 验证 WebSocket 用量记录不能绑定到非 Responses 协议路由。
    #[test]
    fn usage_record_kind_must_match_protocol_route() {
        let mut manifest = minimal_manifest();
        manifest.usage_profiles = vec![crate::provider_manifest::schema::UsageProfileDeclaration {
            profile_id: String::from("test.usage.websocket.v1"),
            description: String::from("test"),
            payload: Some(vulcan_llm_core::UsageExtractionProfile {
                profile_id: String::from("test.usage.websocket.v1"),
                revision: 1,
                record_kind: vulcan_llm_core::UsageRecordKind::WebSocketFrame,
                update_kind: vulcan_ai_contract::UsageUpdateKind::Final,
                event_type_pointer: Some(String::from("/type")),
                event_type_value: Some(String::from("response.completed")),
                required_pointer: Some(String::from("/response/usage")),
                bindings: vec![vulcan_llm_core::UsageFieldBinding {
                    field: vulcan_llm_core::UsageField::InputTokens,
                    pointers: vec![String::from("/response/usage/input_tokens")],
                    encoding: vulcan_llm_core::UsageValueEncoding::UnsignedInteger,
                }],
                derivations: Vec::new(),
            }),
        }];
        manifest.services[0].protocol_routes[0].protocol_profile_id =
            String::from(vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID);
        manifest.services[0].protocol_routes[0].usage_profile_refs =
            vec![String::from("test.usage.websocket.v1")];

        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("WebSocket usage on a Chat route must fail");

        assert!(
            errors
                .iter()
                .any(|error| error.code == "usage_record_kind_protocol_mismatch"),
            "transport-incompatible usage profile must be rejected: {errors:?}"
        );
    }

    /// Verifies a referenced usage profile without a payload is rejected.
    /// 验证被引用但无载荷的 usage Profile 被拒绝。
    #[test]
    fn referenced_usage_profile_without_payload_rejected() {
        let mut manifest = minimal_manifest();
        manifest.usage_profiles = vec![crate::provider_manifest::schema::UsageProfileDeclaration {
            profile_id: String::from("test.usage.v1"),
            description: String::from("test"),
            payload: None,
        }];
        manifest.services[0].protocol_routes[0].usage_profile_refs =
            vec![String::from("test.usage.v1")];
        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("empty usage payload must fail");
        assert!(
            errors
                .iter()
                .any(|error| error.code == "profile_declared_without_payload"),
            "empty usage payload reference must be rejected: {errors:?}"
        );
    }

    /// Verifies a valid projection payload compiles without errors.
    /// 验证有效投影载荷编译无错误。
    #[test]
    fn valid_projection_payload_accepts_bounded_steps() {
        let mut manifest = manifest_with_request_profile(
            crate::provider_manifest::schema::RequestProfileDeclaration {
                profile_id: String::from("test.request.v1"),
                description: String::from("test"),
                payload: Some(crate::provider_manifest::schema::RequestProjectionProfile {
                    profile_id: String::from("test.request.v1"),
                    revision: 1,
                    steps: vec![
                        crate::provider_manifest::schema::MappingStep::DeleteJsonPointer {
                            path: String::from("/reasoning_effort"),
                        },
                        crate::provider_manifest::schema::MappingStep::SetLiteral {
                            path: String::from("/enable_thinking"),
                            value: serde_json::json!(true),
                        },
                        crate::provider_manifest::schema::MappingStep::StringTemplate {
                            path: String::from("/model_name"),
                            template: String::from("private/{model_id}"),
                        },
                        crate::provider_manifest::schema::MappingStep::CopyRequestField {
                            path: String::from("/effort"),
                            field: String::from("reasoning_effort"),
                        },
                    ],
                }),
            },
        );
        manifest.engine_contract.required_transformers = [
            "delete_json_pointer",
            "set_literal",
            "string_template",
            "copy_request_field",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect("bounded projection must validate");
    }

    /// Verifies an operation referencing an undeclared tool target is rejected.
    /// 验证引用未声明工具目标的操作被拒绝。
    #[test]
    fn operation_dangling_tool_target_reference_rejected() {
        let mut manifest = minimal_manifest();
        manifest.operations = vec![crate::provider_manifest::schema::OperationDeclaration {
            operation_id: String::from("op.image-generate"),
            operation: String::from("image_generate"),
            carrier: crate::provider_manifest::schema::OperationCarrier::ProviderTool,
            input_resources: Vec::new(),
            output_resources: Vec::new(),
            tool_target_refs: vec![String::from("missing.target")],
            tool_target_template_refs: Vec::new(),
            request_profile_refs: Vec::new(),
            response_profile_refs: Vec::new(),
            upload_channel_refs: Vec::new(),
            upload_bindings: Vec::new(),
            async_execution: None,
            timeout_ms: Some(60_000),
            idempotent: false,
        }];
        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("dangling tool target reference must fail");
        assert!(
            errors
                .iter()
                .any(|error| error.code == "dangling_tool_target_reference"),
            "undeclared tool target must be rejected: {errors:?}"
        );
    }

    /// Verifies an operation referencing an undeclared upload channel is rejected.
    /// 验证引用未声明上传通道的操作被拒绝。
    #[test]
    fn operation_dangling_upload_channel_reference_rejected() {
        let mut manifest = minimal_manifest();
        manifest.operations = vec![crate::provider_manifest::schema::OperationDeclaration {
            operation_id: String::from("op.image-generate"),
            operation: String::from("image_generate"),
            carrier: crate::provider_manifest::schema::OperationCarrier::ProviderTool,
            input_resources: Vec::new(),
            output_resources: Vec::new(),
            tool_target_refs: Vec::new(),
            tool_target_template_refs: Vec::new(),
            request_profile_refs: Vec::new(),
            response_profile_refs: Vec::new(),
            upload_channel_refs: vec![String::from("missing.channel")],
            upload_bindings: Vec::new(),
            async_execution: None,
            timeout_ms: Some(60_000),
            idempotent: false,
        }];
        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("dangling upload channel reference must fail");
        assert!(
            errors
                .iter()
                .any(|error| error.code == "dangling_upload_channel_reference"),
            "undeclared upload channel must be rejected: {errors:?}"
        );
    }

    /// Verifies a tool target referencing an unknown service entry is rejected.
    /// 验证引用未知服务入口的工具目标被拒绝。
    #[test]
    fn tool_target_unknown_service_entry_rejected() {
        let mut manifest = minimal_manifest();
        manifest.tool_targets = vec![tool_target_declaration(
            "missing.service",
            crate::ProviderToolCapabilityKind::WebSearch,
            crate::ProviderToolExecutionKind::HttpJsonSync,
        )];
        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("unknown service entry must fail");
        assert!(
            errors
                .iter()
                .any(|error| error.code == "unknown_tool_target_service_entry"),
            "unknown tool target service entry must be rejected: {errors:?}"
        );
    }

    /// Verifies a tool target referencing an unknown credential profile is rejected.
    /// 验证引用未知凭据配置档的工具目标被拒绝。
    #[test]
    fn tool_target_unknown_credential_profile_rejected() {
        let mut manifest = minimal_manifest();
        manifest.tool_targets = vec![tool_target_declaration(
            "public-api",
            crate::ProviderToolCapabilityKind::WebSearch,
            crate::ProviderToolExecutionKind::HttpJsonSync,
        )];
        manifest.tool_targets[0].credential_profile_ids =
            BTreeSet::from([String::from("missing-profile")]);
        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("unknown credential profile must fail");
        assert!(
            errors
                .iter()
                .any(|error| error.code == "unknown_tool_target_credential_profile"),
            "unknown tool target credential profile must be rejected: {errors:?}"
        );
    }

    /// Verifies fixed provider-tool targets cannot bypass system endpoint URL policy.
    /// 验证固定供应商工具目标无法绕过系统端点 URL 策略。
    #[test]
    fn tool_target_endpoint_base_url_uses_manifest_security_policy() {
        let mut manifest = minimal_manifest();
        let mut target = tool_target_declaration(
            "public-api",
            crate::ProviderToolCapabilityKind::WebSearch,
            crate::ProviderToolExecutionKind::HttpJsonSync,
        );
        target.execution.endpoint_base_url = Some(String::from(
            "https://embedded-user:embedded-secret@example.com",
        ));
        manifest.tool_targets = vec![target];
        manifest.operations = vec![crate::provider_manifest::schema::OperationDeclaration {
            operation_id: String::from("op.web-search"),
            operation: String::from("web_search"),
            carrier: crate::provider_manifest::schema::OperationCarrier::ProviderTool,
            input_resources: Vec::new(),
            output_resources: Vec::new(),
            tool_target_refs: vec![String::from("public-api.search")],
            tool_target_template_refs: Vec::new(),
            request_profile_refs: Vec::new(),
            response_profile_refs: Vec::new(),
            upload_channel_refs: Vec::new(),
            upload_bindings: Vec::new(),
            async_execution: None,
            timeout_ms: Some(60_000),
            idempotent: false,
        }];
        manifest
            .engine_contract
            .required_execution_primitives
            .push(String::from("http_json_sync"));

        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("embedded endpoint credentials must fail closed");
        assert!(
            errors.iter().any(|error| {
                error.code == "invalid_url"
                    && error.path == "$.tool_targets[0].execution.endpoint_base_url"
            }),
            "provider-tool endpoint credentials must be rejected: {errors:?}"
        );
    }

    /// Verifies preauthorized target templates receive the complete target validation chain.
    /// 验证预授权目标模板会进入完整目标校验链。
    #[test]
    fn tool_target_template_contract_is_validated_before_expansion() {
        let mut manifest = minimal_manifest();
        let target = tool_target_declaration(
            "public-api",
            crate::ProviderToolCapabilityKind::WebSearch,
            crate::ProviderToolExecutionKind::HttpJsonSync,
        );
        let mut execution = target.execution;
        execution.endpoint_base_url = Some(String::from("http://untrusted.example"));
        execution.fixed_headers.insert(
            String::from("Authorization"),
            String::from("forbidden-static-secret"),
        );
        manifest.tool_target_templates = vec![
            crate::provider_manifest::schema::ToolTargetTemplateDeclaration {
                template_id: String::from("template.web-search"),
                description: String::from("test template"),
                display_name_suffix: String::from("Search"),
                allowed_service_entry_ids: BTreeSet::from([String::from("public-api")]),
                credential_profile_ids: BTreeSet::from([String::from("api-key")]),
                tool_id: String::from("vulcan_web_search"),
                capability: crate::ProviderToolCapabilityKind::WebSearch,
                input_schema: serde_json::json!({"type": "string"}),
                input_numeric_product_ranges: Vec::new(),
                output: target.output,
                execution,
            },
        ];
        manifest.operations = vec![crate::provider_manifest::schema::OperationDeclaration {
            operation_id: String::from("op.template-web-search"),
            operation: String::from("web_search"),
            carrier: crate::provider_manifest::schema::OperationCarrier::ProviderTool,
            input_resources: Vec::new(),
            output_resources: Vec::new(),
            tool_target_refs: Vec::new(),
            tool_target_template_refs: vec![String::from("template.web-search")],
            request_profile_refs: Vec::new(),
            response_profile_refs: Vec::new(),
            upload_channel_refs: Vec::new(),
            upload_bindings: Vec::new(),
            async_execution: None,
            timeout_ms: Some(60_000),
            idempotent: false,
        }];
        manifest
            .engine_contract
            .required_execution_primitives
            .push(String::from("http_json_sync"));

        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("unsafe template contract must fail before expansion");
        assert!(
            errors.iter().any(|error| {
                error.code == "invalid_url_scheme"
                    && error.path == "$.tool_target_templates[0].execution.endpoint_base_url"
            }),
            "template endpoint scheme must be rejected: {errors:?}"
        );
        assert!(
            errors.iter().any(|error| {
                error.code == "invalid_provider_tool_fixed_header_name"
                    && error
                        .path
                        .contains("$.tool_target_templates[0].execution.fixed_headers")
            }),
            "template authorization header must be rejected: {errors:?}"
        );
        assert!(
            errors.iter().any(|error| {
                error.code == "provider_tool_input_schema_root_not_object"
                    && error.path == "$.tool_target_templates[0].input_schema"
            }),
            "template input schema must be rejected: {errors:?}"
        );
    }

    /// Verifies a tool target using a retired non-AI-visible capability is rejected.
    /// 验证使用已退役非 AI 可见能力的工具目标被拒绝。
    #[test]
    fn tool_target_retired_capability_rejected() {
        let mut manifest = minimal_manifest();
        manifest.tool_targets = vec![tool_target_declaration(
            "public-api",
            crate::ProviderToolCapabilityKind::EmbeddingCreate,
            crate::ProviderToolExecutionKind::HttpJsonSync,
        )];
        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("retired capability must fail");
        assert!(
            errors
                .iter()
                .any(|error| error.code == "tool_target_capability_not_ai_visible"),
            "retired capability must be rejected: {errors:?}"
        );
    }

    /// Verifies operation request profiles cannot read conversation-only host request fields.
    /// 验证操作请求 Profile 不能读取仅会话可用的宿主请求字段。
    #[test]
    fn operation_request_profile_rejects_conversation_host_context() {
        let mut manifest = minimal_manifest();
        manifest.request_profiles = vec![
            crate::provider_manifest::schema::RequestProfileDeclaration {
                profile_id: String::from("operation.request.v1"),
                description: String::from("test operation request"),
                payload: Some(crate::provider_manifest::schema::RequestProjectionProfile {
                    profile_id: String::from("operation.request.v1"),
                    revision: 1,
                    steps: vec![
                        crate::provider_manifest::schema::MappingStep::CopyRequestField {
                            path: String::from("/effort"),
                            field: String::from("reasoning_effort"),
                        },
                    ],
                }),
            },
        ];
        let mut target = tool_target_declaration(
            "public-api",
            crate::ProviderToolCapabilityKind::WebSearch,
            crate::ProviderToolExecutionKind::HttpJsonSync,
        );
        target.execution.adapter = crate::ProviderToolAdapterKind::GenericJson;
        manifest.tool_targets = vec![target];
        manifest.operations = vec![operation_with_request_profile()];

        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("operation host request context must fail");
        assert!(
            errors
                .iter()
                .any(|error| { error.code == "operation_request_profile_requires_host_context" }),
            "operation host context must be rejected: {errors:?}"
        );
    }

    /// Verifies a fully closed operation/target/channel manifest validates cleanly.
    /// 验证完全闭合的操作/目标/通道清单能干净通过校验。
    #[test]
    fn closed_operation_tool_target_manifest_validates() {
        let mut manifest = minimal_manifest();
        manifest
            .engine_contract
            .required_upload_primitives
            .push(String::from("inline_data_url"));
        manifest
            .engine_contract
            .required_execution_primitives
            .push(String::from("http_json_sync"));
        manifest.upload_channels =
            vec![crate::provider_manifest::schema::UploadChannelDeclaration {
                upload_channel_id: String::from("inline.data-url"),
                description: String::from("inline data url channel"),
                primitive: Some(
                    crate::provider_manifest::schema::UploadPrimitiveKind::InlineDataUrl,
                ),
                signing_primitive: None,
                endpoint_host: None,
                endpoint_path: None,
                response_host_suffixes: Vec::new(),
                remote_url_host_suffixes: Vec::new(),
                method: None,
                credential_role: None,
                file_field: None,
                accepted_mime_types: vec![String::from("image/png")],
                max_bytes: Some(1_024),
                max_count: Some(1),
                timeout_ms: None,
                response_id_pointer: None,
                response_url_pointer: None,
                reference_template: None,
                progress_supported: false,
                cancellation_supported: false,
                fixed_fields: Default::default(),
                cache_duration_ms: None,
                reference_encoding: None,
            }];
        manifest.tool_targets = vec![tool_target_declaration(
            "public-api",
            crate::ProviderToolCapabilityKind::WebSearch,
            crate::ProviderToolExecutionKind::HttpJsonSync,
        )];
        manifest.operations = vec![crate::provider_manifest::schema::OperationDeclaration {
            operation_id: String::from("op.web-search"),
            operation: String::from("web_search"),
            carrier: crate::provider_manifest::schema::OperationCarrier::ProviderTool,
            input_resources: Vec::new(),
            output_resources: Vec::new(),
            tool_target_refs: vec![String::from("public-api.search")],
            tool_target_template_refs: Vec::new(),
            request_profile_refs: Vec::new(),
            response_profile_refs: Vec::new(),
            upload_channel_refs: vec![String::from("inline.data-url")],
            upload_bindings: Vec::new(),
            async_execution: None,
            timeout_ms: Some(60_000),
            idempotent: false,
        }];
        validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect("closed operation manifest must validate");
    }

    /// Verifies every operation requires an explicit total execution deadline.
    /// 验证每项操作都必须声明显式总执行截止时间。
    #[test]
    fn operation_without_timeout_is_rejected() {
        let mut manifest = minimal_manifest();
        manifest.operations = vec![operation_with_request_profile()];
        manifest.operations[0].timeout_ms = None;

        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("operation without timeout must fail closed");

        assert!(
            errors
                .iter()
                .any(|error| error.code == "operation_timeout_required")
        );
    }

    /// Verifies asynchronous failure backoff cannot poll faster than the declared base interval.
    /// 验证异步失败退避不能比声明的基础轮询间隔更频繁。
    #[test]
    fn async_operation_backoff_cannot_undercut_poll_interval() {
        // sourcePath selects the production Alibaba manifest because it owns reviewed async tasks.
        // sourcePath 选择生产 Alibaba 清单，因为它持有已审核异步任务。
        let source_path = vulcan_runtime_assets::source_bundled_provider_manifests_directory()
            .join("alibaba.json");
        // manifest retains every surrounding production contract while one bound is mutated.
        // manifest 保留全部生产周边契约，只修改一个边界。
        let mut manifest: ProviderManifestV1 = serde_json::from_str(
            &std::fs::read_to_string(source_path).expect("Alibaba manifest must be readable"),
        )
        .expect("Alibaba manifest must parse");
        // asyncExecution is the first real asynchronous lifecycle in the bundled manifest.
        // asyncExecution 是随包清单中的第一条真实异步生命周期。
        let async_execution = manifest
            .operations
            .iter_mut()
            .find_map(|operation| operation.async_execution.as_mut())
            .expect("Alibaba must declare an asynchronous operation");
        async_execution.max_backoff_ms = async_execution.poll_interval_ms - 1;

        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("backoff below poll interval must fail closed");
        assert!(
            errors
                .iter()
                .any(|error| error.code == "async_backoff_out_of_bounds"),
            "backoff bound must be classified exactly: {errors:?}"
        );
    }

    /// Verifies model bindings cannot instantiate a reviewed template on an unauthorized service.
    /// 验证模型绑定不能在未经授权的服务入口实例化已审核模板。
    #[test]
    fn tool_target_template_binding_rejects_unauthorized_service_entry() {
        let path = vulcan_runtime_assets::source_bundled_provider_manifests_directory()
            .join("alibaba.json");
        let text = std::fs::read_to_string(path).expect("Alibaba manifest must be readable");
        let mut manifest: ProviderManifestV1 =
            serde_json::from_str(&text).expect("Alibaba manifest must parse");
        let binding = manifest
            .tool_target_template_bindings
            .iter_mut()
            .find(|binding| {
                binding
                    .template_refs
                    .iter()
                    .any(|reference| reference == "alibaba.model-studio-cn.qwen-image-generate.v1")
            })
            .expect("Alibaba must expose the reviewed image template binding");
        binding.service_entry_id = String::from("model-studio-singapore");

        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("an unauthorized template service entry must fail closed");
        assert!(
            errors
                .iter()
                .any(|error| error.code == "tool_target_template_service_not_authorized"),
            "unauthorized template service must be classified exactly: {errors:?}"
        );
    }

    /// Verifies resource URLs cannot compile without an explicit reviewed host authority.
    /// 验证资源 URL 在没有显式已审核宿主权限时无法编译。
    #[test]
    fn resource_url_decoder_requires_reviewed_host_suffix() {
        // The minimal manifest supplies the exact service and credential identities for the target.
        // 最小清单为目标提供精确服务与凭据身份。
        let mut manifest = minimal_manifest();
        // The target models one syntactically complete URL resource contract with no host authority.
        // 该目标模拟一个语法完整但没有宿主权限的 URL 资源契约。
        let mut target = tool_target_declaration(
            "public-api",
            crate::ProviderToolCapabilityKind::ImageGenerate,
            crate::ProviderToolExecutionKind::HttpJsonSync,
        );
        target.output = crate::ProviderToolOutputContract {
            kind: crate::ProviderToolOutputKind::Resources,
            source_path: String::from("/assets"),
            normalized_result_path: String::from("resources"),
            resource_kind: Some(String::from("image")),
            mime_types: vec![String::from("image/png")],
            resource_decoders: vec![crate::ProviderToolResourceDecoder {
                source_path: String::from("/assets"),
                encoding: crate::ProviderToolResourceEncoding::HttpsUrl,
                download_host_suffixes: Vec::new(),
                resource_kind: String::from("image"),
                mime_type: Some(String::from("image/png")),
                mime_type_request_path: None,
                mime_types_by_request_value: BTreeMap::new(),
                optional: false,
                max_bytes: None,
                output_id_prefix: Some(String::from("image")),
            }],
        };
        manifest.tool_targets = vec![target];

        // The exact diagnostic proves schema shape cannot substitute for network authority.
        // 精确诊断证明 Schema 形状不能替代网络权限。
        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("URL decoder without host authority must fail closed");
        assert!(
            errors.iter().any(|error| {
                error.code == "url_resource_decoder_requires_download_host_suffix"
            }),
            "missing resource host authority must be classified exactly: {errors:?}"
        );
    }

    /// Verifies resource output contracts cannot invoke the removed shape-guessing decoder.
    /// 验证资源输出契约无法调用已移除的形状猜测解码器。
    #[test]
    fn resource_output_requires_closed_decoder() {
        // The minimal manifest supplies one otherwise valid system-provider container.
        // 最小清单提供一个其他方面有效的系统供应商容器。
        let mut manifest = minimal_manifest();
        // The target intentionally declares a resource kind without any carrier extractor.
        // 该目标刻意声明资源类别但不提供任何载体提取器。
        let mut target = tool_target_declaration(
            "public-api",
            crate::ProviderToolCapabilityKind::ImageGenerate,
            crate::ProviderToolExecutionKind::HttpJsonSync,
        );
        target.output.kind = crate::ProviderToolOutputKind::Resources;
        target.output.resource_kind = Some(String::from("image"));
        target.output.mime_types = vec![String::from("image/png")];
        manifest.tool_targets = vec![target];

        // The validation result must name the missing closed decoder rather than fail at runtime.
        // 校验结果必须指出缺少封闭解码器，而不是延迟到运行时失败。
        let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
            .expect_err("resource output without decoder must fail closed");
        assert!(
            errors
                .iter()
                .any(|error| error.code == "resource_output_requires_declared_decoder"),
            "missing closed decoder must be classified exactly: {errors:?}"
        );
    }

    /// Verifies contradictory model-source declarations fail before compilation or runtime I/O.
    /// 验证矛盾的模型来源声明会在编译或运行时 I/O 前失败。
    #[test]
    fn model_source_policy_validation_fails_closed() {
        // missingPolicy proves every conversation entry must make identity authority explicit.
        // missingPolicy 证明每个对话入口都必须显式声明身份权威。
        let mut missing_policy = minimal_manifest();
        missing_policy.services[0].model_catalog = None;
        let errors = validate_manifest(&missing_policy, ManifestOwnership::System, &registry())
            .expect_err("missing source policy must fail");
        assert!(
            errors
                .iter()
                .any(|error| { error.code == "conversation_entry_requires_model_source_policy" })
        );

        // discoveryWithoutRemote cannot disguise the manual marker as remote evidence.
        // discoveryWithoutRemote 不能把手工标记伪装成远端证据。
        let mut discovery_without_remote = minimal_manifest();
        let catalog = discovery_without_remote.services[0]
            .model_catalog
            .as_mut()
            .expect("minimal fixture has a catalog");
        catalog.source_policy = ModelSourcePolicy::DiscoveryOnly;
        catalog.discovery_kind = Some(ModelDiscoveryKind::Manual);
        let errors = validate_manifest(
            &discovery_without_remote,
            ManifestOwnership::System,
            &registry(),
        )
        .expect_err("discovery source without remote implementation must fail");
        assert!(
            errors
                .iter()
                .any(|error| { error.code == "discovery_policy_requires_remote_discovery_kind" })
        );

        // manualWithRemote cannot perform discovery under a non-discovery policy.
        // manualWithRemote 不能在非发现策略下执行远端发现。
        let mut manual_with_remote = minimal_manifest();
        manual_with_remote.services[0]
            .model_catalog
            .as_mut()
            .expect("minimal fixture has a catalog")
            .discovery_kind = Some(ModelDiscoveryKind::OpenAi);
        let errors = validate_manifest(&manual_with_remote, ManifestOwnership::System, &registry())
            .expect_err("manual-only source with remote discovery must fail");
        assert!(
            errors
                .iter()
                .any(|error| error.code == "source_policy_forbids_remote_discovery")
        );

        // discoveryCatalog cannot smuggle Manifest identities through a discovery-only policy.
        // discoveryCatalog 不能通过 discovery-only 策略夹带 Manifest 身份。
        let mut discovery_catalog = minimal_manifest();
        let catalog = discovery_catalog.services[0]
            .model_catalog
            .as_mut()
            .expect("minimal fixture has a catalog");
        catalog.source_policy = ModelSourcePolicy::DiscoveryOnly;
        catalog.discovery_kind = Some(ModelDiscoveryKind::OpenAi);
        catalog.models = Some(vec![ManifestCatalogModel {
            model_id: String::from("manifest-model"),
            display_name: String::from("Manifest Model"),
        }]);
        let errors = validate_manifest(&discovery_catalog, ManifestOwnership::System, &registry())
            .expect_err("discovery-only source with Manifest inventory must fail");
        assert!(
            errors
                .iter()
                .any(|error| error.code == "catalog_models_require_manifest_source")
        );

        // manifestWithoutInventory cannot leave the sole identity source ambiguous.
        // manifestWithoutInventory 不能让唯一身份来源处于歧义状态。
        let mut manifest_without_inventory = minimal_manifest();
        let catalog = manifest_without_inventory.services[0]
            .model_catalog
            .as_mut()
            .expect("minimal fixture has a catalog");
        catalog.source_policy = ModelSourcePolicy::ManifestOnly;
        catalog.discovery_kind = Some(ModelDiscoveryKind::ManifestStatic);
        catalog.models = None;
        let errors = validate_manifest(
            &manifest_without_inventory,
            ManifestOwnership::System,
            &registry(),
        )
        .expect_err("manifest-only source without inventory must fail");
        assert!(
            errors
                .iter()
                .any(|error| error.code == "manifest_only_requires_catalog_models")
        );
    }

    /// Builds one default tool target declaration for closure tests.
    /// 为闭合测试构建一条默认工具目标声明。
    fn tool_target_declaration(
        service_entry_id: &str,
        capability: crate::ProviderToolCapabilityKind,
        execution_kind: crate::ProviderToolExecutionKind,
    ) -> crate::provider_manifest::schema::ToolTargetDeclaration {
        crate::provider_manifest::schema::ToolTargetDeclaration {
            target_id: String::from("public-api.search"),
            description: String::from("test search target"),
            service_entry_id: service_entry_id.to_string(),
            credential_profile_ids: BTreeSet::from([String::from("api-key")]),
            upstream_target_id: String::from("search"),
            display_name: String::from("Test Search"),
            tool_id: String::from("vulcan_web_search"),
            capability,
            input_schema: serde_json::json!({"type": "object"}),
            input_numeric_product_ranges: Vec::new(),
            output: crate::ProviderToolOutputContract {
                kind: crate::ProviderToolOutputKind::Search,
                source_path: String::from("results"),
                normalized_result_path: String::from("search"),
                resource_kind: None,
                mime_types: Vec::new(),
                resource_decoders: Vec::new(),
            },
            execution: crate::ProviderToolExecutionContract {
                endpoint_base_url: None,
                endpoint_path: String::from("/search"),
                authentication: crate::AuthenticationProjection::BearerAuthorization,
                adapter: crate::ProviderToolAdapterKind::GenericJson,
                execution_kind,
                fixed_headers: BTreeMap::new(),
                start_fixed_headers: BTreeMap::new(),
                start_conditional_headers: Vec::new(),
                mcp_http_call: None,
            },
        }
    }

    /// Builds one closed web-search operation referencing the operation request test profile.
    /// 构建一个引用操作请求测试 Profile 的闭合网页搜索操作。
    fn operation_with_request_profile() -> crate::provider_manifest::schema::OperationDeclaration {
        crate::provider_manifest::schema::OperationDeclaration {
            operation_id: String::from("op.web-search"),
            operation: String::from("web_search"),
            carrier: crate::provider_manifest::schema::OperationCarrier::ProviderTool,
            input_resources: Vec::new(),
            output_resources: Vec::new(),
            tool_target_refs: vec![String::from("public-api.search")],
            tool_target_template_refs: Vec::new(),
            request_profile_refs: vec![String::from("operation.request.v1")],
            response_profile_refs: Vec::new(),
            upload_channel_refs: Vec::new(),
            upload_bindings: Vec::new(),
            async_execution: None,
            timeout_ms: Some(60_000),
            idempotent: false,
        }
    }
}
