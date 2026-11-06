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
use vulcan_llm_core::OPENAI_CODEX_RESPONSES_PROFILE_ID;

use super::{
    EnginePrimitiveRegistry, FORBIDDEN_MANIFEST_HEADERS, MANIFEST_SCHEMA_VERSION,
    MAX_ASYNC_POLL_OR_BACKOFF_MS, MAX_ASYNC_TIMEOUT_MS, MAX_FIXED_HEADER_VALUE_BYTES,
    MAX_FIXED_HEADERS, MAX_JSON_DEPTH, MAX_JSON_NODES, MAX_MANIFEST_BYTES, MAX_MATCHER_VALUE_LEN,
    MAX_MIME_TYPES, MAX_MODEL_RULES, MAX_PROTOCOL_ROUTES, MAX_PROVIDER_IDENTIFIER_LEN,
    MAX_REQUEST_ID_HEADERS, MAX_SERVICES, MAX_STRING_LEN, MIN_ASYNC_POLL_INTERVAL_MS,
    ManifestValidationError, PROTECTED_REQUEST_BODY_ROOTS, ProviderManifestV1, UnknownModelPolicy,
};
use crate::ModelDiscoveryKind;
use crate::provider_manifest::schema::{
    ManifestEntryKind, ManifestProtocolRoute, ModelRule, ModelSourcePolicy,
};
use crate::{CredentialAuthorization, CredentialMode};

/// Hard maximum bytes accepted by one declared upload channel.
/// 单条声明上传通道允许接受的最大字节数硬上限。
const MAX_UPLOAD_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// Hard maximum item count accepted by one upload channel.
/// 单条上传通道允许接受的最大条目数硬上限。
const MAX_UPLOAD_ITEM_COUNT: u64 = 1_024;
/// Hard maximum fixed form fields attached to one upload request.
/// 单次上传请求可附加的固定表单字段硬上限。
const MAX_UPLOAD_FIXED_FIELDS: usize = 32;
/// Hard maximum bytes accepted by one dynamic session-header name.
/// 单个动态会话请求头名称允许接受的最大字节数硬上限。
/// Hard maximum process-local reuse lifetime for one provider upload reference.
/// 单个供应商上传引用在进程内复用的最长硬期限。
const MAX_UPLOAD_CACHE_DURATION_MS: u64 = 24 * 60 * 60 * 1_000;
/// Hard maximum requirement identifiers accepted by each engine-contract dimension.
/// 每个引擎契约维度允许声明的依赖标识硬上限。
const MAX_ENGINE_CONTRACT_REQUIREMENTS: usize = 64;
/// Minimum manifest engine required by configured additive total usage derivation.
/// 配置式用量总额加法派生所需的最低 Manifest 引擎版本。
const ADDITIVE_TOTAL_USAGE_DERIVATION_MIN_ENGINE: u32 = 2;
/// Minimum manifest engine supporting independent reasoning in normalized complete output.
/// 支持将独立思考归一化到完整输出的最低 Manifest 引擎版本。
const DISJOINT_REASONING_USAGE_MIN_ENGINE: u32 = 3;
/// Sensitive response headers that may never be reclassified as provider request identifiers.
/// 绝不能被重新归类为供应商请求标识的敏感响应头。
const FORBIDDEN_PROVIDER_REQUEST_ID_RESPONSE_HEADERS: &[&str] = &[
    "authentication-info",
    "cookie",
    "proxy-authentication-info",
    "set-cookie",
    "www-authenticate",
];

/// Determines whether one manifest-owned Batch endpoint is a safe service-root-relative path.
/// 判断一个 Manifest 持有的 Batch 端点是否为安全的服务根相对路径。
///
/// # Parameters
/// 参数
/// `endpoint` is the endpoint or endpoint template declared by the system manifest.
/// `endpoint` 是系统 Manifest 声明的端点或端点模板。
///
/// # Returns
/// 返回值
/// `true` only for an absolute-path reference without query or fragment components.
/// 仅对不含查询与片段部分的绝对路径引用返回 `true`。
fn valid_native_batch_endpoint(endpoint: &str) -> bool {
    endpoint.starts_with('/')
        && !endpoint.starts_with("//")
        && !endpoint.contains('?')
        && !endpoint.contains('#')
        && !endpoint.contains('\\')
        && !endpoint.chars().any(char::is_whitespace)
        && !endpoint
            .split('/')
            .any(|segment| matches!(segment, "." | ".."))
}

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
                "expected schema version `{MANIFEST_SCHEMA_VERSION}`, found `{}`; upgrade to a client release compatible with this manifest",
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
                "manifest requires manifest engine {} but this host supports {}; upgrade the client before installing this manifest",
                contract.minimum_manifest_engine,
                super::CURRENT_MANIFEST_ENGINE
            ),
        ));
    }
    for (profile_index, declaration) in manifest.usage_profiles.iter().enumerate() {
        let Some(payload) = declaration.payload.as_ref() else {
            continue;
        };
        if payload
            .derivations
            .contains(&vulcan_llm_core::UsageDerivation::OutputFromDisjointReasoning)
            && contract.minimum_manifest_engine < DISJOINT_REASONING_USAGE_MIN_ENGINE
        {
            errors.push(ManifestValidationError::new(
                "engine_contract_missing_usage_derivation_version",
                format!("$.usage_profiles[{profile_index}].payload.derivations"),
                format!("usage derivation `output_from_disjoint_reasoning` requires manifest engine {DISJOINT_REASONING_USAGE_MIN_ENGINE} or newer"),
            ));
        }
        if payload
            .derivations
            .contains(&vulcan_llm_core::UsageDerivation::TotalFromInputComponentsAndOutput)
            && contract.minimum_manifest_engine < ADDITIVE_TOTAL_USAGE_DERIVATION_MIN_ENGINE
        {
            errors.push(ManifestValidationError::new(
                "engine_contract_missing_usage_derivation_version",
                format!("$.usage_profiles[{profile_index}].payload.derivations"),
                format!(
                    "usage derivation `total_from_input_components_and_output` requires manifest engine {ADDITIVE_TOTAL_USAGE_DERIVATION_MIN_ENGINE} or newer"
                ),
            ));
        }
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
            format!(
                "protocol profile `{id}` is not registered by this engine; upgrade the client before installing this manifest"
            ),
        ));
    }
    for id in missing(
        &contract.required_execution_primitives,
        &registry.execution_primitives,
    ) {
        errors.push(ManifestValidationError::new(
            "unsupported_engine_contract",
            "engine_contract.required_execution_primitives",
            format!(
                "execution primitive `{id}` is not registered by this engine; upgrade the client before installing this manifest"
            ),
        ));
    }
    for id in missing(
        &contract.required_upload_primitives,
        &registry.upload_primitives,
    ) {
        errors.push(ManifestValidationError::new(
            "unsupported_engine_contract",
            "engine_contract.required_upload_primitives",
            format!(
                "upload primitive `{id}` is not registered by this engine; upgrade the client before installing this manifest"
            ),
        ));
    }
    for id in missing(&contract.required_transformers, &registry.transformers) {
        errors.push(ManifestValidationError::new(
            "unsupported_engine_contract",
            "engine_contract.required_transformers",
            format!(
                "transformer `{id}` is not registered by this engine; upgrade the client before installing this manifest"
            ),
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
    // A declared reasoning replay contract is engine capability too, so it must close over the
    // same engine contract as every provider tool primitive.
    // 已声明的推理回传契约同样属于引擎能力，因此必须与供应商工具原语一样在引擎契约中闭合。
    for (service_index, service) in manifest.services.iter().enumerate() {
        for (route_index, route) in service.protocol_routes.iter().enumerate() {
            let Some(contract) = route.reasoning_replay_contract else {
                continue;
            };
            let primitive = contract.primitive_id();
            if !required_execution.contains(primitive) {
                errors.push(ManifestValidationError::new(
                    "engine_contract_missing_used_execution_primitive",
                    format!(
                        "$.services[{service_index}].protocol_routes[{route_index}].reasoning_replay_contract"
                    ),
                    format!(
                        "used reasoning replay contract `{primitive}` is absent from engine_contract.required_execution_primitives"
                    ),
                ));
            }
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
        crate::ProviderToolExecutionKind::HttpJsonSse => "http_json_sse",
        crate::ProviderToolExecutionKind::HttpMultipartSse => "http_multipart_sse",
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

/// Validates one credential-scoped account model-access policy.
/// 验证一份凭据作用域的账号模型访问策略。
///
/// # Parameters
/// 参数
/// `policy` owns declarative plan rules, `usage_query` identifies the evidence source, `path`
/// owns the diagnostic location, and `errors` collects validation failures.
/// `policy` 持有声明式套餐规则，`usage_query` 标识证据来源，`path` 持有诊断位置，`errors`
/// 收集校验失败。
fn validate_account_model_access_policy(
    policy: &crate::AccountModelAccessPolicy,
    usage_query: crate::AccountUsageQueryKind,
    path: &str,
    errors: &mut Vec<ManifestValidationError>,
) {
    if matches!(usage_query, crate::AccountUsageQueryKind::Unsupported) {
        errors.push(ManifestValidationError::new(
            "account_model_access_without_usage_query",
            path,
            "account model access requires a supported account-usage query",
        ));
    }
    if policy.plan_rules.len() > 64 {
        errors.push(ManifestValidationError::new(
            "too_many_account_plan_rules",
            format!("{path}.plan_rules"),
            "account model-access policy exceeds 64 plan rules",
        ));
    }
    for (plan_identity, rule) in &policy.plan_rules {
        let rule_path = format!("{path}.plan_rules.{plan_identity}");
        if plan_identity.trim().is_empty() || plan_identity.len() > 256 {
            errors.push(ManifestValidationError::new(
                "invalid_account_plan_identity",
                format!("{path}.plan_rules"),
                "account plan identities must contain between 1 and 256 UTF-8 bytes",
            ));
        }
        if rule.allowed_model_groups.len() > 32 {
            errors.push(ManifestValidationError::new(
                "too_many_allowed_model_groups",
                format!("{rule_path}.allowed_model_groups"),
                "one account plan cannot admit more than 32 model groups",
            ));
        }
        for group in &rule.allowed_model_groups {
            validate_identifier(
                group,
                &format!("{rule_path}.allowed_model_groups"),
                errors,
                "allowed_model_group",
            );
        }
        if rule.blocked_model_ids.len() > 128 {
            errors.push(ManifestValidationError::new(
                "too_many_blocked_model_ids",
                format!("{rule_path}.blocked_model_ids"),
                "one account plan cannot block more than 128 model identifiers",
            ));
        }
        for model_id in &rule.blocked_model_ids {
            if model_id.trim().is_empty() || model_id.len() > 512 {
                errors.push(ManifestValidationError::new(
                    "invalid_blocked_model_id",
                    format!("{rule_path}.blocked_model_ids"),
                    "blocked model identifiers must contain between 1 and 512 UTF-8 bytes",
                ));
            }
        }
    }
    if policy.unrestricted_when_quota_remaining.len() > 32 {
        errors.push(ManifestValidationError::new(
            "too_many_unrestricted_quota_windows",
            format!("{path}.unrestricted_when_quota_remaining"),
            "account model-access policy cannot inspect more than 32 quota windows",
        ));
    }
    for window_id in &policy.unrestricted_when_quota_remaining {
        validate_identifier(
            window_id,
            &format!("{path}.unrestricted_when_quota_remaining"),
            errors,
            "quota_window_id",
        );
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
        validate_credential_authorization(
            profile.credential_mode,
            profile.authorization.as_ref(),
            ownership,
            &format!("$.services[{index}].credential_profiles[{profile_index}].authorization"),
            errors,
        );
        if let Some(policy) = profile.account_model_access.as_ref() {
            validate_account_model_access_policy(
                policy,
                profile.account_usage_query,
                &format!(
                    "$.services[{index}].credential_profiles[{profile_index}].account_model_access"
                ),
                errors,
            );
        }
    }

    validate_fixed_headers(
        &entry.request_headers,
        &path(".request_headers"),
        errors,
        "service",
    );
    if entry.session_header.is_some()
        && entry.kind != crate::provider_manifest::schema::ManifestEntryKind::Conversation
    {
        errors.push(ManifestValidationError::new(
            "session_header_requires_conversation",
            path(".session_header"),
            "session header is only supported by conversation service entries",
        ));
    }
    validate_session_header(
        entry.session_header.as_deref(),
        &path(".session_header"),
        errors,
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

    let mut generic_native_operations = BTreeSet::new();
    let mut model_specific_native_ids: BTreeMap<vulcan_ai_contract::AiOperation, BTreeSet<String>> =
        BTreeMap::new();
    let mut native_binding_ids = BTreeSet::new();
    for (binding_index, binding) in entry.native_inference_bindings.iter().enumerate() {
        let binding_path = path(&format!(".native_inference_bindings[{binding_index}]"));
        if !matches!(
            binding.operation,
            vulcan_ai_contract::AiOperation::EmbeddingCreate
                | vulcan_ai_contract::AiOperation::RerankDocuments
                | vulcan_ai_contract::AiOperation::DecisionEvaluate
        ) {
            errors.push(ManifestValidationError::new(
                "unsupported_native_inference_operation",
                format!("{binding_path}.operation"),
                "native inference binding supports only embedding_create, rerank_documents or decision_evaluate",
            ));
        }
        // Native projections admit only their exact declared operation families.
        // 原生投影仅准入准确声明的操作族。
        if match binding.projection {
            crate::NativeInferenceProjectionKind::OpenVulcan => !matches!(
                binding.operation,
                vulcan_ai_contract::AiOperation::EmbeddingCreate
                    | vulcan_ai_contract::AiOperation::DecisionEvaluate
            ),
            crate::NativeInferenceProjectionKind::SystemOne => {
                binding.operation != vulcan_ai_contract::AiOperation::DecisionEvaluate
            }
            _ => binding.operation == vulcan_ai_contract::AiOperation::DecisionEvaluate,
        } {
            errors.push(ManifestValidationError::new(
                "invalid_native_inference_projection",
                format!("{binding_path}.projection"),
                "native operation is incompatible with its declared projection",
            ));
        }
        if binding.model_ids.is_empty() {
            if !generic_native_operations.insert(binding.operation) {
                errors.push(ManifestValidationError::new(
                    "duplicate_native_inference_operation",
                    format!("{binding_path}.operation"),
                    format!(
                        "operation `{}` has more than one operation-wide native binding",
                        binding.operation.as_str()
                    ),
                ));
            }
        } else {
            let known_model_ids = model_specific_native_ids
                .entry(binding.operation)
                .or_default();
            for (model_index, model_id) in binding.model_ids.iter().enumerate() {
                if model_id.trim().is_empty()
                    || model_id.len() > 256
                    || !known_model_ids.insert(model_id.clone())
                {
                    errors.push(ManifestValidationError::new(
                        "invalid_native_inference_model_ids",
                        format!("{binding_path}.model_ids[{model_index}]"),
                        "model-specific native binding ids must be unique, non-empty, and at most 256 bytes",
                    ));
                }
            }
        }
        if binding.projection == crate::NativeInferenceProjectionKind::AlibabaTextRerank
            && binding.operation != vulcan_ai_contract::AiOperation::RerankDocuments
        {
            errors.push(ManifestValidationError::new(
                "invalid_native_inference_projection",
                format!("{binding_path}.projection"),
                "Alibaba text-rerank projection is valid only for rerank_documents",
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
        if let Some(batch) = binding.batch.as_ref() {
            if binding.operation != vulcan_ai_contract::AiOperation::EmbeddingCreate {
                errors.push(ManifestValidationError::new(
                    "native_batch_requires_embedding_operation",
                    format!("{binding_path}.batch"),
                    "native inference Batch is valid only for embedding_create",
                ));
            }
            if binding.model_ids.is_empty() {
                errors.push(ManifestValidationError::new(
                    "native_batch_requires_exact_models",
                    format!("{binding_path}.model_ids"),
                    "native inference Batch must be scoped to verified exact model ids",
                ));
            }
            if let Err(message) = batch.validate() {
                errors.push(ManifestValidationError::new(
                    "invalid_native_inference_batch",
                    format!("{binding_path}.batch"),
                    message,
                ));
            }
            for model_id in &binding.model_ids {
                // Batch augments an existing physical embedding route and must never replace its identity.
                // Batch 只增强既有物理向量路由，绝不能替换该路由的身份。
                let has_synchronous_binding =
                    entry.native_inference_bindings.iter().any(|candidate| {
                        candidate.operation == binding.operation
                            && candidate.batch.is_none()
                            && (candidate.model_ids.is_empty()
                                || candidate.model_ids.iter().any(|value| value == model_id))
                    });
                if !has_synchronous_binding {
                    errors.push(ManifestValidationError::new(
                        "native_batch_requires_synchronous_binding",
                        format!("{binding_path}.model_ids"),
                        format!(
                            "native inference Batch model `{model_id}` requires a matching synchronous embedding binding"
                        ),
                    ));
                }
            }
            for (field, endpoint) in [
                (
                    "submit_endpoint_path",
                    Some(batch.submit_endpoint_path.as_str()),
                ),
                (
                    "status_endpoint_path_template",
                    Some(batch.status_endpoint_path_template.as_str()),
                ),
                (
                    "cancel_endpoint_path_template",
                    batch.cancel_endpoint_path_template.as_deref(),
                ),
                (
                    "item_endpoint_path",
                    Some(batch.item_endpoint_path.as_str()),
                ),
                (
                    "file_upload_endpoint_path",
                    batch.file_upload_endpoint_path.as_deref(),
                ),
                (
                    "file_content_endpoint_path_template",
                    batch.file_content_endpoint_path_template.as_deref(),
                ),
                (
                    "file_delete_endpoint_path_template",
                    batch.file_delete_endpoint_path_template.as_deref(),
                ),
            ] {
                if endpoint.is_some_and(|endpoint| !valid_native_batch_endpoint(endpoint)) {
                    errors.push(ManifestValidationError::new(
                        "invalid_native_inference_batch_endpoint",
                        format!("{binding_path}.batch.{field}"),
                        "native inference Batch endpoints must be relative absolute paths without query or fragment",
                    ));
                }
            }
        }
        match binding.operation {
            vulcan_ai_contract::AiOperation::EmbeddingCreate
            | vulcan_ai_contract::AiOperation::DecisionEvaluate
                if binding.rerank_truncation.is_some() =>
            {
                errors.push(ManifestValidationError::new(
                    "embedding_binding_declares_rerank_truncation",
                    format!("{binding_path}.rerank_truncation"),
                    "only a rerank binding may declare rerank truncation",
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

/// Validates the acquisition declaration attached to one credential profile.
/// 校验附加到一个凭据配置档的获取声明。
///
/// # Parameters
/// 参数
/// `credential_mode` is the declared acquisition mode, `authorization` is its exact optional
/// host flow, `ownership` constrains trusted declarations, and `path` identifies the field.
/// `credential_mode` 是声明的获取模式，`authorization` 是其精确可选宿主流程，`ownership`
/// 约束可信声明，`path` 标识字段。
///
/// # Returns
/// 返回值
/// Appends every detected validation failure to `errors`.
/// 将检测到的每个校验失败追加到 `errors`。
fn validate_credential_authorization(
    credential_mode: CredentialMode,
    authorization: Option<&CredentialAuthorization>,
    ownership: ManifestOwnership,
    path: &str,
    errors: &mut Vec<ManifestValidationError>,
) {
    match (credential_mode, authorization) {
        (CredentialMode::Auth, None) => errors.push(ManifestValidationError::new(
            "missing_credential_authorization",
            path,
            "auth credential profiles must declare one authorization flow",
        )),
        (CredentialMode::Token, Some(_)) => errors.push(ManifestValidationError::new(
            "unexpected_credential_authorization",
            path,
            "token credential profiles must not declare an authorization flow",
        )),
        (_, None) => {}
        (_, Some(_)) if matches!(ownership, ManifestOwnership::Custom) => {
            errors.push(ManifestValidationError::new(
                "custom_credential_authorization_forbidden",
                path,
                "custom provider manifests cannot declare host-managed authorization flows",
            ));
        }
        (_, Some(CredentialAuthorization::RegisteredOauth { .. })) => {}
        (_, Some(CredentialAuthorization::BrowserCallback { profile })) => {
            validate_browser_callback_authorization(profile, path, errors);
        }
    }
}

/// Validates one bounded declarative browser-callback authorization contract.
/// 校验一个有界声明式浏览器回调授权契约。
///
/// # Parameters
/// 参数
/// `profile` is the exact callback contract and `path` identifies its manifest location.
/// `profile` 是精确回调契约，`path` 标识其 Manifest 位置。
///
/// # Returns
/// 返回值
/// Appends every detected validation failure to `errors`.
/// 将检测到的每个校验失败追加到 `errors`。
fn validate_browser_callback_authorization(
    profile: &crate::BrowserCallbackAuthorization,
    path: &str,
    errors: &mut Vec<ManifestValidationError>,
) {
    let profile_path = format!("{path}.profile");
    validate_http_url(
        &profile.authorization_url,
        &format!("{profile_path}.authorization_url"),
        errors,
        true,
    );
    if reqwest::Url::parse(profile.authorization_url.as_str())
        .ok()
        .and_then(|url| url.port())
        .is_some()
    {
        errors.push(ManifestValidationError::new(
            "browser_authorization_non_default_port",
            format!("{profile_path}.authorization_url"),
            "browser authorization URLs must not use a non-default port",
        ));
    }
    validate_browser_callback_query_parameter(
        &profile.callback_query_parameter,
        &format!("{profile_path}.callback_query_parameter"),
        errors,
    );
    validate_browser_callback_query_parameter(
        &profile.state_query_parameter,
        &format!("{profile_path}.state_query_parameter"),
        errors,
    );
    if profile.callback_query_parameter == profile.state_query_parameter {
        errors.push(ManifestValidationError::new(
            "duplicate_browser_callback_query_parameter",
            profile_path.clone(),
            "callback and State query parameters must be distinct",
        ));
    }

    let listener_path = format!("{profile_path}.listener");
    if profile.listener.path.is_empty()
        || profile.listener.path.len() > 256
        || !profile.listener.path.starts_with('/')
        || profile.listener.path.contains(['?', '#', '\\', '\n', '\r'])
    {
        errors.push(ManifestValidationError::new(
            "invalid_browser_callback_path",
            format!("{listener_path}.path"),
            "browser callback path must be an absolute path of at most 256 bytes without query, fragment, backslash, or control characters",
        ));
    }
    let port_count = profile
        .listener
        .port_end
        .checked_sub(profile.listener.port_start)
        .map_or(0_u32, |difference| u32::from(difference) + 1);
    if profile.listener.port_start == 0 || port_count == 0 || port_count > 32 {
        errors.push(ManifestValidationError::new(
            "invalid_browser_callback_port_range",
            listener_path.clone(),
            "browser callback port range must contain between 1 and 32 nonzero inclusive ports",
        ));
    }
    if !(30..=1_800).contains(&profile.listener.timeout_seconds) {
        errors.push(ManifestValidationError::new(
            "invalid_browser_callback_timeout",
            format!("{listener_path}.timeout_seconds"),
            "browser callback timeout must be between 30 and 1800 seconds",
        ));
    }
    if !(1..=65_536).contains(&profile.listener.max_body_bytes) {
        errors.push(ManifestValidationError::new(
            "invalid_browser_callback_body_limit",
            format!("{listener_path}.max_body_bytes"),
            "browser callback body limit must be between 1 and 65536 bytes",
        ));
    }

    let response_path = format!("{profile_path}.response");
    let mut pointers = BTreeSet::new();
    for (field_name, pointer) in [
        (
            "secret_json_pointer",
            profile.response.secret_json_pointer.as_str(),
        ),
        (
            "state_json_pointer",
            profile.response.state_json_pointer.as_str(),
        ),
        (
            "subject_json_pointer",
            profile.response.subject_json_pointer.as_str(),
        ),
        (
            "username_json_pointer",
            profile.response.username_json_pointer.as_str(),
        ),
        (
            "credential_label_json_pointer",
            profile.response.credential_label_json_pointer.as_str(),
        ),
    ] {
        validate_browser_callback_json_pointer(
            pointer,
            &format!("{response_path}.{field_name}"),
            errors,
        );
        if !pointers.insert(pointer) {
            errors.push(ManifestValidationError::new(
                "duplicate_browser_callback_json_pointer",
                response_path.clone(),
                "required browser callback JSON Pointers must be distinct",
            ));
        }
    }
    for (field_name, pointer) in [
        (
            "error_json_pointer",
            profile.response.error_json_pointer.as_deref(),
        ),
        (
            "error_description_json_pointer",
            profile.response.error_description_json_pointer.as_deref(),
        ),
    ] {
        if let Some(pointer) = pointer {
            validate_browser_callback_json_pointer(
                pointer,
                &format!("{response_path}.{field_name}"),
                errors,
            );
            if !pointers.insert(pointer) {
                errors.push(ManifestValidationError::new(
                    "duplicate_browser_callback_json_pointer",
                    response_path.clone(),
                    "browser callback JSON Pointers must be distinct",
                ));
            }
        }
    }
    if profile.response.error_description_json_pointer.is_some()
        && profile.response.error_json_pointer.is_none()
    {
        errors.push(ManifestValidationError::new(
            "orphan_browser_callback_error_description",
            response_path,
            "browser callback error description requires an error-code JSON Pointer",
        ));
    }
}

/// Validates one authorization-page query parameter name.
/// 校验一个授权页面查询参数名。
///
/// # Parameters
/// 参数
/// `value` is the exact parameter name and `path` identifies its field.
/// `value` 是精确参数名，`path` 标识其字段。
///
/// # Returns
/// 返回值
/// Appends a validation failure when the name is outside the closed grammar.
/// 当名称超出封闭语法时追加一个校验失败。
fn validate_browser_callback_query_parameter(
    value: &str,
    path: &str,
    errors: &mut Vec<ManifestValidationError>,
) {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        errors.push(ManifestValidationError::new(
            "invalid_browser_callback_query_parameter",
            path,
            "browser callback query parameter must contain 1 to 128 ASCII letters, digits, `_`, `-`, or `.`",
        ));
    }
}

/// Validates one RFC 6901 JSON Pointer used by the browser callback mapper.
/// 校验浏览器回调映射器使用的一个 RFC 6901 JSON Pointer。
///
/// # Parameters
/// 参数
/// `pointer` is the exact pointer and `path` identifies its field.
/// `pointer` 是精确指针，`path` 标识其字段。
///
/// # Returns
/// 返回值
/// Appends a validation failure when the pointer is empty, oversized, or improperly escaped.
/// 当指针为空、超长或转义不正确时追加一个校验失败。
fn validate_browser_callback_json_pointer(
    pointer: &str,
    path: &str,
    errors: &mut Vec<ManifestValidationError>,
) {
    let bytes = pointer.as_bytes();
    let mut index = 0_usize;
    let mut valid_escape = true;
    while index < bytes.len() {
        if bytes[index] == b'~' {
            valid_escape = bytes
                .get(index + 1)
                .is_some_and(|next| matches!(next, b'0' | b'1'));
            if !valid_escape {
                break;
            }
            index += 1;
        }
        index += 1;
    }
    if pointer.is_empty()
        || pointer.len() > 256
        || !pointer.starts_with('/')
        || pointer.chars().any(char::is_control)
        || !valid_escape
    {
        errors.push(ManifestValidationError::new(
            "invalid_browser_callback_json_pointer",
            path,
            "browser callback JSON Pointer must be a nonempty RFC 6901 pointer of at most 256 bytes",
        ));
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
        && route.protocol_profile_id != vulcan_llm_core::OPENAI_CODEX_RESPONSES_PROFILE_ID
    {
        errors.push(ManifestValidationError::new(
            "responses_features_on_non_codex_route",
            path,
            "Responses optional capabilities are only allowed on Codex Responses routes",
        ));
    }
    if route.responses_features.is_some_and(|features| {
        features.recovery
            == crate::ResponsesRecoveryPolicy::AgentSpaceResultAndUsageReconciliationV1
    }) && (manifest.provider_id != "agentspace" || entry.service_entry_id != "chatgpt-plan")
    {
        errors.push(ManifestValidationError::new(
            "agentspace_recovery_policy_scope_mismatch",
            format!("{path}.responses_features.recovery"),
            "AgentSpace reconciliation recovery is restricted to provider 'agentspace' service entry 'chatgpt-plan'",
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
    let effective_discovery_kinds = if manifest.provider_id.starts_with("custom:") {
        // Custom identity discovery belongs to the entry, independently from conversation routes.
        // 自定义模型身份发现属于入口，独立于会话路由。
        vec![entry_discovery_kind]
    } else {
        entry
            .protocol_routes
            .iter()
            .map(|route| route.model_discovery.or(entry_discovery_kind))
            .collect::<Vec<_>>()
    };
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
        && manifest
            .model_defaults
            .unknown_model_policy_for_entry(entry)
            == UnknownModelPolicy::DeclaredRuleRequired
        && let Some(models) = &catalog.models
    {
        for (model_index, model) in models.iter().enumerate() {
            for credential in entry.credential_profiles.iter().filter(|credential| {
                model.credential_profile_ids.is_empty()
                    || model
                        .credential_profile_ids
                        .contains(&credential.credential_profile_id)
            }) {
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
            // Validate every declared credential scope against the owning service entry.
            // 对照所属服务入口校验每个声明的凭据作用域。
            let mut credential_profile_ids = BTreeSet::new();
            for credential_profile_id in &model.credential_profile_ids {
                if !credential_profile_ids.insert(credential_profile_id.as_str()) {
                    errors.push(ManifestValidationError::new(
                        "duplicate_catalog_model_credential_profile_id",
                        format!("{model_path}.credential_profile_ids"),
                        format!(
                            "catalog model credential profile `{credential_profile_id}` is duplicated"
                        ),
                    ));
                }
                if !entry
                    .credential_profiles
                    .iter()
                    .any(|profile| profile.credential_profile_id == *credential_profile_id)
                {
                    errors.push(ManifestValidationError::new(
                        "catalog_model_references_unknown_credential_profile",
                        format!("{model_path}.credential_profile_ids"),
                        format!(
                            "catalog model references credential profile `{credential_profile_id}` that service entry `{}` does not declare",
                            entry.service_entry_id
                        ),
                    ));
                }
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
    if manifest.services.iter().any(|entry| {
        entry.kind == crate::provider_manifest::schema::ManifestEntryKind::Conversation
            && defaults.unknown_model_policy_for_entry(entry)
                == UnknownModelPolicy::DefaultBindingAllowed
    }) {
        if defaults.capabilities.is_none() {
            errors.push(ManifestValidationError::new(
                "default_binding_allowed_requires_capabilities",
                format!("{base}.capabilities"),
                "default_binding_allowed requires an explicit default capability snapshot",
            ));
        }
        for (entry_index, entry) in manifest.services.iter().enumerate().filter(|(_, entry)| {
            entry.kind == crate::provider_manifest::schema::ManifestEntryKind::Conversation
                && defaults.unknown_model_policy_for_entry(entry)
                    == UnknownModelPolicy::DefaultBindingAllowed
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
    validate_codex_model_behavior(rule, index, errors);

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

    if rule.account_access_groups.len() > 32 {
        errors.push(ManifestValidationError::new(
            "too_many_account_access_groups",
            path(".account_access_groups"),
            "rule account-access group list exceeds 32 entries",
        ));
    }
    let mut account_access_groups = BTreeSet::new();
    for group in &rule.account_access_groups {
        validate_identifier(
            group,
            &path(".account_access_groups"),
            errors,
            "account_access_group",
        );
        if !account_access_groups.insert(group.as_str()) {
            errors.push(ManifestValidationError::new(
                "duplicate_account_access_group",
                path(".account_access_groups"),
                format!("duplicate account-access group `{group}`"),
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

/// Validates the closed Codex behavior declared by one exact model rule.
/// 校验一条精确模型规则声明的封闭 Codex 行为。
///
/// # Parameters
/// 参数
/// The rule owns the declaration, the index identifies its JSON path, and errors receives every violation.
/// 规则持有声明，索引标识其 JSON 路径，errors 接收全部违规项。
fn validate_codex_model_behavior(
    rule: &ModelRule,
    index: usize,
    errors: &mut Vec<ManifestValidationError>,
) {
    let Some(behavior) = rule.codex_behavior.as_ref() else {
        return;
    };
    let base_path = format!("$.model_rules[{index}].codex_behavior");
    if rule
        .selected_protocol
        .as_deref()
        .is_some_and(|profile| profile != OPENAI_CODEX_RESPONSES_PROFILE_ID)
        || (!rule.supported_protocols.is_empty()
            && !rule
                .supported_protocols
                .iter()
                .any(|profile| profile == OPENAI_CODEX_RESPONSES_PROFILE_ID))
    {
        errors.push(ManifestValidationError::new(
            "codex_behavior_requires_native_protocol",
            base_path.as_str(),
            format!(
                "codex_behavior requires protocol profile '{OPENAI_CODEX_RESPONSES_PROFILE_ID}'"
            ),
        ));
    }
    if behavior.default_verbosity.is_some() && !behavior.supports_verbosity {
        errors.push(ManifestValidationError::new(
            "codex_default_verbosity_without_support",
            format!("{base_path}.default_verbosity"),
            "default_verbosity requires supports_verbosity",
        ));
    }
    if behavior.multi_agent_reasoning_effort.is_some() && behavior.multi_agent_version.is_none() {
        errors.push(ManifestValidationError::new(
            "codex_multi_agent_effort_without_version",
            format!("{base_path}.multi_agent_reasoning_effort"),
            "multi_agent_reasoning_effort requires multi_agent_version",
        ));
    }
    if behavior.node_repl_disabled && behavior.node_repl_auto_review_required {
        errors.push(ManifestValidationError::new(
            "codex_node_repl_disabled_conflict",
            format!("{base_path}.node_repl_disabled"),
            "node_repl_disabled conflicts with automatic node review",
        ));
    }
    if let Some(truncation) = behavior.tool_output_truncation
        && (truncation.limit == 0 || truncation.limit > 1_000_000)
    {
        errors.push(ManifestValidationError::new(
            "invalid_codex_tool_output_limit",
            format!("{base_path}.tool_output_truncation.limit"),
            "tool-output token limit must be between 1 and 1000000",
        ));
    }
    if behavior.experimental_supported_tools.len() > 64 {
        errors.push(ManifestValidationError::new(
            "too_many_codex_experimental_tools",
            format!("{base_path}.experimental_supported_tools"),
            "experimental tool allowlist exceeds 64 entries",
        ));
    }
    let mut tool_names = BTreeSet::new();
    for tool_name in &behavior.experimental_supported_tools {
        if tool_name.trim().is_empty() || tool_name.len() > 256 {
            errors.push(ManifestValidationError::new(
                "invalid_codex_experimental_tool",
                format!("{base_path}.experimental_supported_tools"),
                "experimental tool names must contain between 1 and 256 UTF-8 bytes",
            ));
        } else if !tool_names.insert(tool_name.as_str()) {
            errors.push(ManifestValidationError::new(
                "duplicate_codex_experimental_tool",
                format!("{base_path}.experimental_supported_tools"),
                format!("duplicate experimental tool name '{tool_name}'"),
            ));
        }
    }
    for (field, value) in [
        ("comp_hash", behavior.comp_hash.as_deref()),
        (
            "auto_review_model_override",
            behavior.auto_review_model_override.as_deref(),
        ),
    ] {
        if value.is_some_and(|value| value.trim().is_empty() || value.len() > 256) {
            errors.push(ManifestValidationError::new(
                "invalid_codex_behavior_string",
                format!("{base_path}.{field}"),
                "Codex behavior strings must contain between 1 and 256 UTF-8 bytes",
            ));
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
        (
            super::UploadPrimitiveKind::SignedObjectUpload,
            Some(super::UploadSigningPrimitiveKind::OpenAiCodexFiles),
        ) => Some("POST"),
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
            || declaration.response_id_encoding.is_some()
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

    if declaration.response_id_encoding.is_some() && declaration.response_id_pointer.is_none() {
        errors.push(ManifestValidationError::new(
            "upload_response_id_encoding_requires_pointer",
            format!("{path}.response_id_encoding"),
            "response_id_encoding requires response_id_pointer",
        ));
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
            crate::provider_manifest::schema::MappingStep::FitThinkingBudget {
                minimum, ..
            } if *minimum == 0 => {
                errors.push(ManifestValidationError::new(
                    "invalid_thinking_budget_minimum",
                    &step_path,
                    "fit_thinking_budget minimum must be positive",
                ));
            }
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
/// Rendering reads the same value, so a template accepted at compilation never exceeds the
/// runtime item limit.
/// 渲染读取同一值，因此编译时接受的模板不会超出运行时项数上限。
pub(crate) const JSON_TEMPLATE_COLLECTION_LIMIT: usize = 256;

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
        | crate::provider_manifest::schema::MappingStep::FitThinkingBudget { path, .. }
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
        MappingStep::FitThinkingBudget {
            path, limit_path, ..
        } => vec![
            (MappingStepSource::BodyJsonPointer, path.as_str()),
            (MappingStepSource::BodyJsonPointer, limit_path.as_str()),
        ],
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
        | crate::provider_manifest::schema::MappingStep::FitThinkingBudget { .. }
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
            vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID
                | vulcan_llm_core::OPENAI_CODEX_RESPONSES_PROFILE_ID
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
        validate_provider_tool_media_input_projection(target, target_path.as_str(), errors);
        validate_provider_tool_extension_profile(
            target.capability,
            target.extension_profile.as_ref(),
            target_path.as_str(),
            ownership,
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

/// Validates the host-only projection from the invariant media facade into native arguments.
/// 校验从固定媒体外壳到供应商原生参数的仅宿主可见投影。
///
/// # Parameters
/// 参数
/// `target` is one declared provider tool, `target_path` scopes diagnostics, and `errors`
/// accumulates every independent projection violation.
/// `target` 是一项已声明供应商工具，`target_path` 限定诊断位置，`errors` 收集每项独立投影违规。
///
/// # Returns
/// 返回值
/// The function appends deterministic validation errors and returns no value.
/// 本函数追加确定性的校验错误且不返回值。
fn validate_provider_tool_media_input_projection(
    target: &super::ToolTargetDeclaration,
    target_path: &str,
    errors: &mut Vec<ManifestValidationError>,
) {
    // isMediaUnderstanding closes projection authority to the three invariant media tools.
    // isMediaUnderstanding 将投影权限限定在三个固定媒体工具内。
    let is_media_understanding = matches!(
        target.capability,
        crate::ProviderToolCapabilityKind::ImageUnderstand
            | crate::ProviderToolCapabilityKind::VideoUnderstand
            | crate::ProviderToolCapabilityKind::AudioUnderstand
    );
    if !is_media_understanding {
        if target.media_input_projection.is_some() {
            errors.push(ManifestValidationError::new(
                "media_input_projection_on_non_media_target",
                format!("{target_path}.media_input_projection"),
                "only image, video, or audio understanding targets may declare a media input projection",
            ));
        }
        return;
    }
    // projection must be explicit so provider changes cannot alter the public tool schema.
    // projection 必须显式声明，避免供应商变化改动公开工具 Schema。
    let Some(projection) = target.media_input_projection.as_ref() else {
        errors.push(ManifestValidationError::new(
            "missing_media_input_projection",
            format!("{target_path}.media_input_projection"),
            "media-understanding targets require an explicit host-only input projection",
        ));
        return;
    };
    if projection.path_field.trim().is_empty()
        || projection.prompt_field.trim().is_empty()
        || projection.path_field == projection.prompt_field
        || projection
            .fixed_arguments
            .contains_key(projection.path_field.as_str())
        || projection
            .fixed_arguments
            .contains_key(projection.prompt_field.as_str())
    {
        errors.push(ManifestValidationError::new(
            "invalid_media_input_projection_fields",
            format!("{target_path}.media_input_projection"),
            "media input projection fields must be non-empty, distinct, and disjoint from fixed arguments",
        ));
        return;
    }
    // properties is the closed native target field catalog admitted by the input schema.
    // properties 是输入 Schema 接纳的封闭原生目标字段目录。
    let Some(properties) = target
        .input_schema
        .get("properties")
        .and_then(Value::as_object)
    else {
        errors.push(ManifestValidationError::new(
            "media_input_projection_requires_object_properties",
            format!("{target_path}.input_schema.properties"),
            "media-understanding target schemas must declare object properties",
        ));
        return;
    };
    // expectedPathType distinguishes a scalar native path from an explicit one-item array.
    // expectedPathType 区分原生标量路径与显式单项数组路径。
    let expected_path_type = if projection.path_as_single_item_array {
        "array"
    } else {
        "string"
    };
    if properties
        .get(projection.path_field.as_str())
        .and_then(|schema| schema.get("type"))
        .and_then(Value::as_str)
        != Some(expected_path_type)
    {
        errors.push(ManifestValidationError::new(
            "media_input_projection_path_type_mismatch",
            format!("{target_path}.media_input_projection.path_field"),
            format!(
                "projected path field `{}` must have native schema type `{expected_path_type}`",
                projection.path_field
            ),
        ));
    }
    if properties
        .get(projection.prompt_field.as_str())
        .and_then(|schema| schema.get("type"))
        .and_then(Value::as_str)
        != Some("string")
    {
        errors.push(ManifestValidationError::new(
            "media_input_projection_prompt_type_mismatch",
            format!("{target_path}.media_input_projection.prompt_field"),
            format!(
                "projected prompt field `{}` must have native schema type `string`",
                projection.prompt_field
            ),
        ));
    }
    for fixed_field in projection.fixed_arguments.keys() {
        if !properties.contains_key(fixed_field) {
            errors.push(ManifestValidationError::new(
                "media_input_projection_unknown_fixed_field",
                format!("{target_path}.media_input_projection.fixed_arguments.{fixed_field}"),
                format!(
                    "fixed projection field `{fixed_field}` is absent from the native input schema"
                ),
            ));
        }
    }
    // requiredFields must all be constructible from the fixed facade plus reviewed constants.
    // requiredFields 必须都能由固定外壳与已审核常量构造出来。
    let required_fields = target
        .input_schema
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str);
    for required_field in required_fields {
        if required_field != projection.path_field
            && required_field != projection.prompt_field
            && !projection.fixed_arguments.contains_key(required_field)
        {
            errors.push(ManifestValidationError::new(
                "media_input_projection_missing_required_field",
                format!("{target_path}.media_input_projection"),
                format!(
                    "required native field `{required_field}` is not supplied by the media projection"
                ),
            ));
        }
    }
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
    let is_sse = matches!(
        execution.execution_kind,
        crate::ProviderToolExecutionKind::HttpJsonSse
            | crate::ProviderToolExecutionKind::HttpMultipartSse
    );
    if is_sse != execution.sse_events.is_some() {
        errors.push(ManifestValidationError::new(
            "provider_tool_sse_contract_mismatch",
            format!("{execution_path}.sse_events"),
            "SSE execution families require exactly one structured SSE event contract",
        ));
    }
    let is_multipart =
        execution.execution_kind == crate::ProviderToolExecutionKind::HttpMultipartSse;
    if is_multipart != execution.multipart_request.is_some() {
        errors.push(ManifestValidationError::new(
            "provider_tool_multipart_contract_mismatch",
            format!("{execution_path}.multipart_request"),
            "the multipart SSE execution family requires exactly one multipart request contract",
        ));
    }
    if let Some(events) = execution.sse_events.as_ref()
        && (!events.event_type_path.starts_with('/')
            || events.terminal_event_types.is_empty()
            || events
                .terminal_event_types
                .iter()
                .chain(events.nonterminal_event_types.iter())
                .any(|event_type| event_type.trim().is_empty()))
    {
        errors.push(ManifestValidationError::new(
            "invalid_provider_tool_sse_contract",
            format!("{execution_path}.sse_events"),
            "SSE event contracts require a JSON Pointer and non-empty terminal event types",
        ));
    }
    if let Some(multipart) = execution.multipart_request.as_ref()
        && (multipart.fields.is_empty()
            || multipart.fields.iter().any(|field| {
                !field.argument_path.starts_with('/') || field.field_name.trim().is_empty()
            }))
    {
        errors.push(ManifestValidationError::new(
            "invalid_provider_tool_multipart_contract",
            format!("{execution_path}.multipart_request"),
            "multipart contracts require non-empty root JSON Pointers and field names",
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
        validate_provider_tool_extension_profile(
            template.capability,
            template.extension_profile.as_ref(),
            template_path.as_str(),
            ownership,
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

/// Validates one provider-tool extension profile against its closed exposure mode.
/// 根据封闭暴露模式校验一份供应商工具扩展说明。
///
/// # Parameters
/// 参数
/// `capability` selects the exposure mode, `profile` is the optional reviewed description,
/// `target_path` scopes diagnostics, `ownership` distinguishes system from custom authority, and
/// `errors` receives every independent validation failure.
/// `capability` 选择暴露模式，`profile` 是可选的已审核说明，`target_path` 限定诊断范围，
/// `ownership` 区分系统与自定义权限，`errors` 接收每项独立校验失败。
///
/// # Returns
/// 返回值
/// This function appends deterministic errors and returns no value.
/// 本函数追加确定性错误且不返回值。
fn validate_provider_tool_extension_profile(
    capability: crate::ProviderToolCapabilityKind,
    profile: Option<&crate::ProviderToolExtensionProfile>,
    target_path: &str,
    ownership: ManifestOwnership,
    errors: &mut Vec<ManifestValidationError>,
) {
    let profile_path = format!("{target_path}.extension_profile");
    if capability.is_direct_selection() {
        if profile.is_some() {
            errors.push(ManifestValidationError::new(
                "direct_provider_tool_has_extension_profile",
                profile_path,
                format!(
                    "direct-selection capability `{capability:?}` must not declare an extension profile"
                ),
            ));
        }
        return;
    }
    if !capability.is_model_extension() {
        return;
    }
    if ownership == ManifestOwnership::Custom {
        errors.push(ManifestValidationError::new(
            "custom_model_extension_not_supported",
            format!("{target_path}.capability"),
            format!("custom manifests cannot declare model-extension capability `{capability:?}`"),
        ));
    }
    let Some(profile) = profile else {
        // System manifests may rely on the compiler's deterministic schema-derived profile.
        // 系统清单可以依赖编译器根据精确 Schema 确定性生成的说明档案。
        return;
    };
    if profile.summary.trim().is_empty() || profile.summary.chars().count() > MAX_STRING_LEN {
        errors.push(ManifestValidationError::new(
            "invalid_provider_tool_extension_summary",
            format!("{profile_path}.summary"),
            format!(
                "extension summary must contain between 1 and {MAX_STRING_LEN} Unicode characters"
            ),
        ));
    }
    if profile.input_modes.is_empty() {
        errors.push(ManifestValidationError::new(
            "missing_provider_tool_extension_input_modes",
            format!("{profile_path}.input_modes"),
            "extension profile must declare at least one verified input mode",
        ));
    }
    if profile.evidence_refs.is_empty() {
        errors.push(ManifestValidationError::new(
            "missing_provider_tool_extension_evidence",
            format!("{profile_path}.evidence_refs"),
            "extension profile must declare at least one auditable evidence reference",
        ));
    }
    for (field, values) in [
        ("characteristics", &profile.characteristics),
        ("input_modes", &profile.input_modes),
        ("constraints", &profile.constraints),
        ("limitations", &profile.limitations),
        ("evidence_refs", &profile.evidence_refs),
    ] {
        for (index, value) in values.iter().enumerate() {
            if value.trim().is_empty() || value.chars().count() > MAX_STRING_LEN {
                errors.push(ManifestValidationError::new(
                    "invalid_provider_tool_extension_text",
                    format!("{profile_path}.{field}[{index}]"),
                    format!(
                        "extension profile text must contain between 1 and {MAX_STRING_LEN} Unicode characters"
                    ),
                ));
            }
        }
    }
    let visual_capability = matches!(
        capability,
        crate::ProviderToolCapabilityKind::ImageGenerate
            | crate::ProviderToolCapabilityKind::ImageEdit
            | crate::ProviderToolCapabilityKind::VideoGenerate
            | crate::ProviderToolCapabilityKind::VideoEdit
            | crate::ProviderToolCapabilityKind::VideoExtend
    );
    if visual_capability && profile.resolution_policy.is_none() {
        errors.push(ManifestValidationError::new(
            "missing_provider_tool_resolution_policy",
            format!("{profile_path}.resolution_policy"),
            format!(
                "visual model-extension capability `{capability:?}` requires a resolution policy"
            ),
        ));
    }
    if let Some(resolution) = profile.resolution_policy.as_ref() {
        match resolution.mode {
            crate::ProviderToolResolutionMode::FixedPresets => {
                if resolution.presets.is_empty() || resolution.custom_bounds.is_some() {
                    errors.push(ManifestValidationError::new(
                        "invalid_fixed_resolution_policy",
                        format!("{profile_path}.resolution_policy"),
                        "fixed_presets requires at least one preset and forbids custom bounds",
                    ));
                }
            }
            crate::ProviderToolResolutionMode::BoundedCustom => {
                let Some(bounds) = resolution.custom_bounds.as_ref() else {
                    errors.push(ManifestValidationError::new(
                        "missing_custom_resolution_bounds",
                        format!("{profile_path}.resolution_policy.custom_bounds"),
                        "bounded_custom requires explicit resolution bounds",
                    ));
                    return;
                };
                if bounds.minimum_width == 0
                    || bounds.minimum_height == 0
                    || bounds.step == 0
                    || bounds.minimum_width > bounds.maximum_width
                    || bounds.minimum_height > bounds.maximum_height
                    || bounds
                        .minimum_pixels
                        .zip(bounds.maximum_pixels)
                        .is_some_and(|(minimum, maximum)| minimum > maximum)
                {
                    errors.push(ManifestValidationError::new(
                        "invalid_custom_resolution_bounds",
                        format!("{profile_path}.resolution_policy.custom_bounds"),
                        "custom resolution bounds must be non-zero ordered ranges",
                    ));
                }
            }
            crate::ProviderToolResolutionMode::ProviderManaged => {
                if resolution.custom_bounds.is_some() {
                    errors.push(ManifestValidationError::new(
                        "provider_managed_resolution_has_bounds",
                        format!("{profile_path}.resolution_policy.custom_bounds"),
                        "provider_managed resolution must not declare custom bounds",
                    ));
                }
            }
        }
    }
    if let Some(duration) = profile.duration_policy.as_ref() {
        let default_invalid = duration.default_seconds.is_some_and(|value| {
            value < duration.minimum_seconds || value > duration.maximum_seconds
        });
        let preset_invalid = duration
            .presets_seconds
            .iter()
            .any(|value| *value < duration.minimum_seconds || *value > duration.maximum_seconds);
        if duration.minimum_seconds == 0
            || duration.minimum_seconds > duration.maximum_seconds
            || default_invalid
            || preset_invalid
        {
            errors.push(ManifestValidationError::new(
                "invalid_provider_tool_duration_policy",
                format!("{profile_path}.duration_policy"),
                "duration values must remain inside one non-zero ordered range",
            ));
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
    const MAX_RESOURCE_BYTES: u64 = crate::provider_tool_execution::GENERIC_RESOURCE_BYTE_LIMIT;
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
    // The generic decoder always consumes the contract source through an absolute JSON pointer.
    // 通用解码器始终通过绝对 JSON 指针消费契约来源。
    if !is_valid_json_pointer_path(&output.source_path) {
        errors.push(ManifestValidationError::new(
            "invalid_provider_tool_output_source_pointer",
            format!("{base_path}.source_path"),
            "provider tool output source must be an absolute JSON pointer",
        ));
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
        if let Some(path) = decoder.item_source_path.as_deref()
            && (!is_valid_json_pointer_path(path)
                || decoder.encoding == crate::ProviderToolResourceEncoding::BinaryBody)
        {
            errors.push(ManifestValidationError::new(
                "invalid_resource_decoder_item_pointer",
                format!("{decoder_path}.item_source_path"),
                "resource decoder item selector must be an absolute JSON pointer on a JSON carrier",
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
                | crate::ProviderToolResourceEncoding::BinaryBody
        );
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
        match (
            follow_up.identifier_source,
            follow_up.identifier_pointer.as_deref(),
        ) {
            (
                crate::provider_manifest::GenericJsonAsyncFollowUpIdentifierSource::ResponsePointer,
                Some(pointer),
            ) if is_valid_json_pointer_path(pointer) => {}
            (
                crate::provider_manifest::GenericJsonAsyncFollowUpIdentifierSource::ProviderTaskId,
                None,
            ) => {}
            _ => errors.push(ManifestValidationError::new(
                "invalid_generic_async_follow_up_identifier_source",
                format!("{follow_up_path}.identifier_pointer"),
                "success follow-up must declare exactly one valid response pointer or reuse the frozen provider task id",
            )),
        }
        match (
            follow_up.identifier_query_parameter.as_deref(),
            follow_up.identifier_in_path,
        ) {
            (Some(parameter), false)
                if !parameter.is_empty()
                    && parameter.chars().all(|character| {
                        character.is_ascii_alphanumeric() || matches!(character, '_' | '-')
                    }) => {}
            (None, true) => {}
            _ => errors.push(ManifestValidationError::new(
                "invalid_generic_async_follow_up_identifier_location",
                format!("{follow_up_path}.identifier_query_parameter"),
                "generic asynchronous success follow-up identifier must use exactly one valid query parameter or encoded path segment",
            )),
        }
        if let Some(suffix) = follow_up.identifier_path_suffix.as_deref()
            && (!follow_up.identifier_in_path
                || suffix.is_empty()
                || !suffix.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | '~')
                }))
        {
            errors.push(ManifestValidationError::new(
                "invalid_generic_async_follow_up_path_suffix",
                format!("{follow_up_path}.identifier_path_suffix"),
                "success follow-up path suffix requires path identifiers and one stable path segment",
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
        crate::ProviderToolCapabilityKind::ImageUnderstand => "image_understand",
        crate::ProviderToolCapabilityKind::VideoUnderstand => "video_understand",
        crate::ProviderToolCapabilityKind::AudioUnderstand => "audio_understand",
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
        if value.len() > MAX_FIXED_HEADER_VALUE_BYTES
            || value.chars().any(|character| character.is_control())
        {
            errors.push(ManifestValidationError::new(
                "invalid_header_value",
                path,
                format!("{owner} header `{name}` has an invalid value"),
            ));
        }
    }
}

/// Validates one optional dynamic session-header name against host-owned boundaries.
/// 对照宿主所有权边界验证一个可选动态会话请求头名称。
///
/// # Parameters
/// 参数
/// `header` is the configured header name, `path` owns stable diagnostics, and `errors` collects
/// validation failures.
/// `header` 是配置的请求头名称，`path` 负责稳定诊断位置，`errors` 收集校验失败。
fn validate_session_header(
    header: Option<&str>,
    path: &str,
    errors: &mut Vec<ManifestValidationError>,
) {
    let Some(header) = header else {
        return;
    };
    if !crate::provider_manifest::is_valid_session_header_name(header) {
        errors.push(ManifestValidationError::new(
            "invalid_session_header",
            path,
            format!(
                "session header name must use ASCII letters, digits, hyphens, underscores, or dots within {} bytes",
                crate::provider_manifest::MAX_SESSION_HEADER_NAME_BYTES
            ),
        ));
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
mod tests;
