//! Core model list: the single home of model-level facts that provider rules inherit by reference.
//! 核心模型清单：模型级事实的唯一归属，供应商规则通过显式引用继承。
//!
//! A core entry owns only facts that belong to the model itself (operations, resources, native
//! reasoning, context and output limits, embedding and rerank shapes). Route facts such as the
//! protocol, structured output, prompt cache, native tools and parameters stay in provider rules.
//! A core entry never requires a provider manifest of the same vendor to exist.
//! 核心条目只持有属于模型本身的事实（操作、资源、原生思考、上下文与输出上限、向量与重排形态）。
//! 协议、结构化输出、提示缓存、原生工具与参数等路线事实仍留在供应商规则中。核心条目不要求存在
//! 同一原厂的供应商清单。

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{ManifestCapabilityPatch, ManifestProvenance, ManifestValidationError};

/// Frozen schema identifier of the core model list document.
/// 核心模型清单文档的冻结 Schema 标识。
pub const CORE_MODELS_SCHEMA_VERSION: &str = "vulcan.core-models.v1";

/// Maximum core entries accepted from one core model list.
/// 单份核心模型清单允许的最大条目数。
pub const MAX_CORE_MODELS: usize = 4_096;

/// Maximum aliases accepted for one core entry.
/// 单个核心条目允许的最大别名数。
pub const MAX_CORE_MODEL_ALIASES: usize = 32;

/// Capability fields a core entry may declare; every other patch field is a route fact.
/// 核心条目可以声明的能力字段；其余补丁字段都是路线事实。
pub const CORE_MODEL_CAPABILITY_FIELDS: &[&str] = &[
    "operations",
    "input_resources",
    "output_resources",
    "reasoning",
    "reasoning_control",
    "reasoning_efforts",
    "context_limit",
    "output_limit",
    "embedding",
    "rerank",
];

/// Verification state of one core entry.
/// 单个核心条目的核实状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoreModelVerification {
    /// Every declared value is backed by vendor documentation or a real request.
    /// 每个已声明值都有原厂文档或真实请求作为依据。
    Verified,
    /// Values were seeded from existing provider declarations and still await verification.
    /// 值由现有供应商声明播种，仍待核实。
    Seeded,
}

/// One core model entry.
/// 单个核心模型条目。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreModelEntry {
    /// Lowercase vendor-neutral model identity referenced by `core_model_ref`.
    /// 被 `core_model_ref` 引用的小写、与供应商无关的模型身份。
    pub model_id: String,
    /// Vendor-facing display name.
    /// 面向原厂的展示名称。
    pub display_name: String,
    /// Stable vendor identifier used for display and grouping only.
    /// 只用于展示与分组的稳定原厂标识。
    pub vendor: String,
    /// Additional lowercase identifiers used only by the common capability catalog.
    /// 只供公共能力目录使用的额外小写标识。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    /// Model-level capability patch inherited by referencing rules.
    /// 由引用规则继承的模型级能力补丁。
    pub capabilities: ManifestCapabilityPatch,
    /// Verification state of the declared values.
    /// 已声明值的核实状态。
    pub verification: CoreModelVerification,
    /// Optional evidence locator declared by `$.provenance.sources`.
    /// 由 `$.provenance.sources` 声明的可选证据定位符。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance_ref: Option<String>,
}

/// Complete core model list document.
/// 完整的核心模型清单文档。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreModelsDocument {
    /// Frozen schema identifier.
    /// 冻结的 Schema 标识。
    pub schema_version: String,
    /// Monotonic list revision.
    /// 单调递增的清单修订号。
    pub revision: u64,
    /// UTC publication timestamp.
    /// UTC 发布时间。
    pub published_at: String,
    /// Core entries in any order; identities are unique.
    /// 任意顺序的核心条目；身份唯一。
    pub models: Vec<CoreModelEntry>,
    /// Evidence table referenced by entry `provenance_ref` values.
    /// 被条目 `provenance_ref` 引用的证据表。
    pub provenance: ManifestProvenance,
}

/// Immutable compiled core model list shared by one runtime generation.
/// 由单个运行时代际共享的不可变已编译核心模型清单。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CoreModelCatalog {
    /// Core list revision; zero only for the empty catalog.
    /// 核心清单修订号；只有空清单为零。
    revision: u64,
    /// SHA-256 and byte length of the exact source bytes when loaded from a pinned file.
    /// 从固定文件加载时精确源字节的 SHA-256 与字节长度。
    source_pin: Option<(String, u64)>,
    /// Entries keyed by exact model identity.
    /// 按精确模型身份索引的条目。
    models: BTreeMap<String, CoreModelEntry>,
}

impl CoreModelCatalog {
    /// Returns an empty catalog that resolves no reference.
    /// 返回不解析任何引用的空清单。
    pub fn empty() -> Self {
        Self::default()
    }

    /// Returns the compiled core list revision.
    /// 返回已编译核心清单的修订号。
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the SHA-256 and byte length of the pinned source bytes, when known.
    /// 返回固定源字节的 SHA-256 与字节长度（已知时）。
    pub fn source_pin(&self) -> Option<(&str, u64)> {
        self.source_pin
            .as_ref()
            .map(|(sha256, bytes)| (sha256.as_str(), *bytes))
    }

    /// Records the SHA-256 and byte length of the exact pinned source bytes.
    /// 记录精确固定源字节的 SHA-256 与字节长度。
    pub(crate) fn with_source_pin(mut self, sha256: String, bytes: u64) -> Self {
        self.source_pin = Some((sha256, bytes));
        self
    }

    /// Returns one entry by exact model identity.
    /// 按精确模型身份返回一个条目。
    pub fn get(&self, model_id: &str) -> Option<&CoreModelEntry> {
        self.models.get(model_id)
    }

    /// Returns every entry in identity order.
    /// 按身份顺序返回全部条目。
    pub fn entries(&self) -> impl Iterator<Item = &CoreModelEntry> {
        self.models.values()
    }

    /// Returns the number of entries.
    /// 返回条目数量。
    pub fn len(&self) -> usize {
        self.models.len()
    }

    /// Reports whether the catalog holds no entry.
    /// 判断清单是否没有任何条目。
    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }
}

/// Parses and validates one core model list from its exact UTF-8 text.
/// 从精确 UTF-8 文本解析并校验一份核心模型清单。
///
/// # Parameters
/// 参数
/// `text` is the complete core model list document.
/// `text` 是完整的核心模型清单文档。
///
/// # Returns
/// 返回值
/// The compiled catalog, or the first closed validation failure.
/// 返回已编译清单，或首个封闭校验失败。
pub fn compile_core_models_text(text: &str) -> Result<CoreModelCatalog, ManifestValidationError> {
    let value: Value = serde_json::from_str(text).map_err(|error| {
        ManifestValidationError::new(
            "core_models_invalid_json",
            "$",
            format!("core model list is invalid JSON: {error}"),
        )
    })?;
    super::validate_runtime_json_bounds(&value, "Core model list")
        .map_err(|message| ManifestValidationError::new("core_models_too_large", "$", message))?;
    reject_capability_nulls(&value)?;
    let document: CoreModelsDocument = serde_json::from_value(value).map_err(|error| {
        ManifestValidationError::new(
            "core_models_invalid_structure",
            "$",
            format!("core model list has an invalid structure: {error}"),
        )
    })?;
    compile_core_models_document(document)
}

/// Validates one parsed core model list and compiles it into a catalog.
/// 校验一份已解析核心模型清单并编译为清单。
pub fn compile_core_models_document(
    document: CoreModelsDocument,
) -> Result<CoreModelCatalog, ManifestValidationError> {
    if document.schema_version != CORE_MODELS_SCHEMA_VERSION {
        return Err(ManifestValidationError::new(
            "core_models_schema_unsupported",
            "$.schema_version",
            format!(
                "core model list schema `{}` is unsupported; expected `{CORE_MODELS_SCHEMA_VERSION}`",
                document.schema_version
            ),
        ));
    }
    if document.revision == 0 {
        return Err(ManifestValidationError::new(
            "core_models_invalid_revision",
            "$.revision",
            "core model list revision must be positive",
        ));
    }
    if document.published_at.trim().is_empty() {
        return Err(ManifestValidationError::new(
            "core_models_missing_published_at",
            "$.published_at",
            "core model list must declare published_at",
        ));
    }
    if document.models.is_empty() || document.models.len() > MAX_CORE_MODELS {
        return Err(ManifestValidationError::new(
            "core_models_invalid_count",
            "$.models",
            format!("core model list must contain 1..={MAX_CORE_MODELS} models"),
        ));
    }
    let sources: BTreeSet<&str> = document
        .provenance
        .sources
        .iter()
        .map(|source| source.url.as_str())
        .collect();
    let mut identities = BTreeSet::new();
    for (index, entry) in document.models.iter().enumerate() {
        validate_core_entry(entry, index, &sources)?;
        if !identities.insert(entry.model_id.as_str()) {
            return Err(ManifestValidationError::new(
                "core_model_duplicate_identity",
                format!("$.models[{index}].model_id"),
                format!("core model `{}` is declared twice", entry.model_id),
            ));
        }
    }
    let mut aliases = BTreeSet::new();
    for (index, entry) in document.models.iter().enumerate() {
        for alias in &entry.aliases {
            if identities.contains(alias.as_str()) || !aliases.insert(alias.as_str()) {
                return Err(ManifestValidationError::new(
                    "core_model_ambiguous_alias",
                    format!("$.models[{index}].aliases"),
                    format!("alias `{alias}` is not unique across core identities and aliases"),
                ));
            }
        }
    }
    let revision = document.revision;
    let models = document
        .models
        .into_iter()
        .map(|entry| (entry.model_id.clone(), entry))
        .collect();
    Ok(CoreModelCatalog {
        revision,
        source_pin: None,
        models,
    })
}

/// Validates one core entry in isolation.
/// 独立校验单个核心条目。
fn validate_core_entry(
    entry: &CoreModelEntry,
    index: usize,
    sources: &BTreeSet<&str>,
) -> Result<(), ManifestValidationError> {
    let path = |suffix: &str| format!("$.models[{index}]{suffix}");
    if !is_core_model_identifier(&entry.model_id) {
        return Err(ManifestValidationError::new(
            "core_model_invalid_identity",
            path(".model_id"),
            format!(
                "core model id `{}` must be 1..={} lowercase characters from [a-z0-9._:/-] starting with a letter or digit",
                entry.model_id,
                super::MAX_MATCHER_VALUE_LEN
            ),
        ));
    }
    for alias in &entry.aliases {
        if !is_core_model_alias(alias) || alias == &entry.model_id {
            return Err(ManifestValidationError::new(
                "core_model_invalid_alias",
                path(".aliases"),
                format!(
                    "alias `{alias}` must be trimmed lowercase text of 1..={} bytes distinct from the model id",
                    super::MAX_MATCHER_VALUE_LEN
                ),
            ));
        }
    }
    if entry.aliases.len() > MAX_CORE_MODEL_ALIASES {
        return Err(ManifestValidationError::new(
            "core_model_too_many_aliases",
            path(".aliases"),
            format!("core model may declare at most {MAX_CORE_MODEL_ALIASES} aliases"),
        ));
    }
    for (field, value) in [
        ("display_name", entry.display_name.as_str()),
        ("vendor", entry.vendor.as_str()),
    ] {
        if value.trim().is_empty() || value.chars().count() > super::MAX_STRING_LEN {
            return Err(ManifestValidationError::new(
                "core_model_invalid_text",
                path(&format!(".{field}")),
                format!(
                    "{field} must contain 1..={} characters",
                    super::MAX_STRING_LEN
                ),
            ));
        }
    }
    if let Some(reference) = entry.provenance_ref.as_deref()
        && !sources.contains(reference)
    {
        return Err(ManifestValidationError::new(
            "dangling_provenance_reference",
            path(".provenance_ref"),
            format!("provenance reference `{reference}` is not declared by $.provenance.sources"),
        ));
    }
    let capabilities = &entry.capabilities;
    let route_fields = [
        (
            "structured_output",
            capabilities.structured_output.is_some(),
        ),
        (
            "provider_native_tools",
            capabilities.provider_native_tools.is_some(),
        ),
        ("prompt_cache", capabilities.prompt_cache.is_some()),
        ("model_parameters", capabilities.model_parameters.is_some()),
        (
            "evidence_revision",
            capabilities.evidence_revision.is_some(),
        ),
    ];
    if let Some((field, _)) = route_fields.iter().find(|(_, present)| *present) {
        return Err(ManifestValidationError::new(
            "core_model_route_field",
            path(&format!(".capabilities.{field}")),
            format!(
                "`{field}` is a route fact owned by provider rules and cannot be declared by a core model"
            ),
        ));
    }
    for (field, value) in [
        ("context_limit", capabilities.context_limit),
        ("output_limit", capabilities.output_limit),
    ] {
        match value {
            Some(None) => {
                return Err(ManifestValidationError::new(
                    "core_model_null_capability",
                    path(&format!(".capabilities.{field}")),
                    format!("core model `{field}` declares a fact; omit it instead of null"),
                ));
            }
            Some(Some(0)) => {
                return Err(ManifestValidationError::new(
                    "core_model_invalid_limit",
                    path(&format!(".capabilities.{field}")),
                    format!("core model `{field}` must be positive"),
                ));
            }
            _ => {}
        }
    }
    if let (Some(Some(context)), Some(Some(output))) =
        (capabilities.context_limit, capabilities.output_limit)
        && output > context
    {
        return Err(ManifestValidationError::new(
            "core_model_output_exceeds_context",
            path(".capabilities.output_limit"),
            format!("output limit {output} exceeds context limit {context}"),
        ));
    }
    if matches!(capabilities.embedding, Some(None)) || matches!(capabilities.rerank, Some(None)) {
        return Err(ManifestValidationError::new(
            "core_model_null_capability",
            path(".capabilities"),
            "core model embedding and rerank declare facts; omit them instead of null",
        ));
    }
    let mut errors = Vec::new();
    super::validation::validate_capability_patch_contracts(
        capabilities,
        &path(".capabilities"),
        &mut errors,
    );
    match errors.into_iter().next() {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

/// Rejects explicit `null` inside every core capability object before deserialization.
/// 在反序列化前拒绝每个核心能力对象中的显式 `null`。
fn reject_capability_nulls(value: &Value) -> Result<(), ManifestValidationError> {
    let Some(models) = value.get("models").and_then(Value::as_array) else {
        return Ok(());
    };
    for (index, model) in models.iter().enumerate() {
        let Some(capabilities) = model.get("capabilities").and_then(Value::as_object) else {
            continue;
        };
        if let Some((field, _)) = capabilities.iter().find(|(_, value)| value.is_null()) {
            return Err(ManifestValidationError::new(
                "core_model_null_capability",
                format!("$.models[{index}].capabilities.{field}"),
                format!("core model `{field}` declares a fact; omit it instead of null"),
            ));
        }
    }
    Ok(())
}

/// Returns whether one value is a valid core model alias: aliases match the normalized lookup
/// form (trimmed, lowercase) and may carry display spellings such as `vendor: name`.
/// 返回取值是否为合法核心模型别名：别名须为规范化查找形式（去空白、小写），可包含 `vendor: name`
/// 这类展示拼写。
pub fn is_core_model_alias(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= super::MAX_MATCHER_VALUE_LEN
        && value.trim() == value
        && value.to_lowercase() == value
}

/// Reports whether one value is a valid lowercase core model identifier.
/// 判断一个值是否为合法的小写核心模型标识。
pub fn is_core_model_identifier(value: &str) -> bool {
    let mut bytes = value.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    value.len() <= super::MAX_MATCHER_VALUE_LEN
        && (first.is_ascii_lowercase() || first.is_ascii_digit())
        && bytes.all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'.' | b'_' | b'-' | b':' | b'/')
        })
}

/// Verifies every `core_model_ref` of one manifest against the core list it compiles with.
/// 对照编译所用的核心清单校验一份清单中的每个 `core_model_ref`。
///
/// A reference must resolve. A referencing rule may not restate a model-level value it already
/// inherits from the provider defaults and the core entry: identical values are inherited, not
/// written. A rule that replaces a core value must carry evidence (`provenance_ref` or the
/// capability `evidence_revision`), because only a proven route difference may diverge.
/// 引用必须能解析。引用规则不得重写它已从供应商默认值与核心条目继承到的模型级值：相同的值
/// 直接继承而不书写。替换核心值的规则必须带有依据（`provenance_ref` 或能力的
/// `evidence_revision`），因为只有经证实的路线差异才能偏离核心。
///
/// # Parameters
/// 参数
/// `manifest` is the validated provider manifest and `core` the core list used to compile it.
/// `manifest` 是已校验的供应商清单，`core` 是编译它所用的核心清单。
///
/// # Returns
/// 返回值
/// Success, or the first closed reference failure.
/// 返回成功，或首个封闭引用失败。
pub fn validate_core_model_references(
    manifest: &super::ProviderManifestV1,
    core: &CoreModelCatalog,
) -> Result<(), ManifestValidationError> {
    let defaults = manifest.model_defaults.capabilities.as_ref();
    for (index, rule) in manifest.model_rules.iter().enumerate() {
        let Some(model_id) = rule.core_model_ref.as_deref() else {
            continue;
        };
        let Some(entry) = core.get(model_id) else {
            return Err(ManifestValidationError::new(
                "unknown_core_model_ref",
                format!("$.model_rules[{index}].core_model_ref"),
                format!(
                    "rule `{}` references core model `{model_id}`, which core model list revision {} does not declare",
                    rule.rule_id,
                    core.revision()
                ),
            ));
        };
        let Some(rule_caps) = rule.capabilities.as_ref() else {
            continue;
        };
        let has_evidence = rule.provenance_ref.is_some() || rule_caps.evidence_revision.is_some();
        macro_rules! check_fields {
            ($($field:ident),* $(,)?) => {
                $(
                    if let Some(value) = rule_caps.$field.as_ref() {
                        let core_value = entry.capabilities.$field.as_ref();
                        let inherited =
                            core_value.or_else(|| defaults.and_then(|caps| caps.$field.as_ref()));
                        let path = format!(
                            "$.model_rules[{index}].capabilities.{}",
                            stringify!($field)
                        );
                        if inherited == Some(value) {
                            return Err(ManifestValidationError::new(
                                "redundant_core_override",
                                path,
                                format!(
                                    "rule `{}` restates the inherited `{}` of core model `{model_id}`; remove the field to inherit it",
                                    rule.rule_id,
                                    stringify!($field)
                                ),
                            ));
                        }
                        if core_value.is_some() && !has_evidence {
                            return Err(ManifestValidationError::new(
                                "core_override_without_evidence",
                                path,
                                format!(
                                    "rule `{}` replaces `{}` of core model `{model_id}` without provenance_ref or evidence_revision",
                                    rule.rule_id,
                                    stringify!($field)
                                ),
                            ));
                        }
                    }
                )*
            };
        }
        check_fields!(
            operations,
            input_resources,
            output_resources,
            reasoning,
            reasoning_control,
            reasoning_efforts,
            context_limit,
            output_limit,
            embedding,
            rerank,
        );
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests;
