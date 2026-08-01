//! Offline common model capability catalog and exact matching.
//! 离线公共模型能力目录与精确匹配。

use serde::Deserialize;
use std::{collections::BTreeMap, sync::OnceLock};
use vulcan_ai_contract::{ModelCapabilities, ResourceKind};

/// Conservative context window used when no exact model record exists.
/// 没有精确模型记录时使用的保守上下文窗口。
pub const COMMON_MODEL_FALLBACK_CONTEXT_WINDOW: u64 = 200_000;

/// Carries the public capability fields permitted in the common catalog.
/// 承载公共目录允许保存的模型能力字段。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommonModelCapabilityRecord {
    /// Canonical exact-match key.
    /// 规范精确匹配键。
    key: String,
    /// Unambiguous exact-match aliases.
    /// 无歧义的精确匹配别名。
    #[serde(default)]
    aliases: Vec<String>,
    /// Maximum context window in tokens.
    /// 最大上下文 Token 窗口。
    context_window: u64,
    /// Supported input modalities.
    /// 支持的输入模态。
    input_modalities: Vec<CommonModelModality>,
    /// Supported output modalities.
    /// 支持的输出模态。
    output_modalities: Vec<CommonModelModality>,
}

/// Defines the closed modality vocabulary accepted from generated resources.
/// 定义生成资源允许使用的封闭模态词表。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CommonModelModality {
    /// Text content.
    /// 文本内容。
    Text,
    /// Image content.
    /// 图片内容。
    Image,
    /// Audio content.
    /// 音频内容。
    Audio,
    /// Video content.
    /// 视频内容。
    Video,
    /// Generic file content.
    /// 通用文件内容。
    File,
}

/// Describes the conservative fallback recorded by the generated resource.
/// 描述生成资源中记录的保守回退能力。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommonModelCapabilityFallback {
    /// Fallback context window in tokens.
    /// 回退上下文 Token 窗口。
    context_window: u64,
    /// Fallback input modalities.
    /// 回退输入模态。
    input_modalities: Vec<CommonModelModality>,
    /// Fallback output modalities.
    /// 回退输出模态。
    output_modalities: Vec<CommonModelModality>,
}

/// Describes the versioned generated common capability resource.
/// 描述版本化生成的公共能力资源。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommonModelCapabilityCatalog {
    /// Exact resource schema version.
    /// 精确资源 Schema 版本。
    schema_version: u32,
    /// Provenance entries used to generate this immutable snapshot.
    /// 生成此不可变快照时使用的来源条目。
    sources: Vec<String>,
    /// Conservative fallback declared by the generated snapshot.
    /// 生成快照声明的保守回退能力。
    fallback: CommonModelCapabilityFallback,
    /// Generated model records.
    /// 生成的模型记录。
    models: Vec<CommonModelCapabilityRecord>,
}

/// Stores one immutable process-local exact lookup index.
/// 保存一个进程内不可变的精确查询索引。
#[derive(Debug)]
struct CommonModelCapabilityIndex {
    /// Records owned by canonical key.
    /// 按规范键持有的记录。
    records: BTreeMap<String, CommonModelCapabilityRecord>,
    /// Alias to canonical-key mappings.
    /// 别名到规范键的映射。
    aliases: BTreeMap<String, String>,
}

/// Reports the resolved public defaults and whether an exact catalog match occurred.
/// 报告解析后的公共默认值以及是否发生精确目录匹配。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommonModelCapabilityResolution {
    /// Whether the catalog matched an exact key or registered alias.
    /// 目录是否命中了精确键或已登记别名。
    pub matched: bool,
    /// Resolved context window.
    /// 解析后的上下文窗口。
    pub context_window: u64,
    /// Whether the source explicitly supports text input.
    /// 来源是否明确支持文本输入。
    pub supports_text_input: bool,
    /// Whether the source explicitly supports text output.
    /// 来源是否明确支持文本输出。
    pub supports_text_output: bool,
    /// Resolved non-text input resources.
    /// 解析后的非文本输入资源。
    pub input_resources: Vec<ResourceKind>,
    /// Resolved non-text output resources.
    /// 解析后的非文本输出资源。
    pub output_resources: Vec<ResourceKind>,
}

/// Returns public defaults for one exact model identifier.
/// 返回一个精确模型标识的公共默认值。
///
/// # Parameters
/// 参数
/// `model_id` is the upstream model identifier exposed by the selected route.
/// `model_id` 是所选路由公开的上游模型标识。
///
/// # Returns
/// 返回值
/// The exact catalog result or the conservative 200K text-only fallback.
/// 返回精确目录结果或保守的 200K 纯文本默认值。
pub fn resolve_common_model_capabilities(model_id: &str) -> CommonModelCapabilityResolution {
    let normalized = normalize_model_key(model_id);
    let index = common_model_capability_index();
    let canonical = index
        .records
        .contains_key(normalized.as_str())
        .then_some(normalized.as_str())
        .or_else(|| index.aliases.get(normalized.as_str()).map(String::as_str));
    let record = canonical.and_then(|key| index.records.get(key));
    match record {
        Some(record) => CommonModelCapabilityResolution {
            matched: true,
            context_window: record.context_window,
            supports_text_input: record.input_modalities.contains(&CommonModelModality::Text),
            supports_text_output: record
                .output_modalities
                .contains(&CommonModelModality::Text),
            input_resources: resource_kinds(record.input_modalities.as_slice()),
            output_resources: resource_kinds(record.output_modalities.as_slice()),
        },
        None => CommonModelCapabilityResolution {
            matched: false,
            context_window: COMMON_MODEL_FALLBACK_CONTEXT_WINDOW,
            supports_text_input: true,
            supports_text_output: true,
            input_resources: Vec::new(),
            output_resources: Vec::new(),
        },
    }
}

/// Fills only capability fields that have not been declared by a more specific source.
/// 只填充尚未由更具体来源声明的能力字段。
///
/// # Parameters
/// 参数
/// `model_id` identifies the exact model, and `capabilities` is mutated in place.
/// `model_id` 标识精确模型，`capabilities` 会被原地修改。
///
/// # Returns
/// 返回值
/// Whether the public catalog matched this model exactly.
/// 公共目录是否精确命中此模型。
pub fn apply_common_model_capability_defaults(
    model_id: &str,
    capabilities: &mut ModelCapabilities,
) -> bool {
    let resolution = resolve_common_model_capabilities(model_id);
    if capabilities.context_limit.is_none() {
        capabilities.context_limit = Some(resolution.context_window);
    }
    if capabilities.input_resources.is_empty() {
        capabilities.input_resources = resolution.input_resources;
    }
    if capabilities.output_resources.is_empty() {
        capabilities.output_resources = resolution.output_resources;
    }
    if resolution.matched && (!resolution.supports_text_input || !resolution.supports_text_output) {
        capabilities
            .operations
            .retain(|operation| *operation != vulcan_ai_contract::AiOperation::ConversationRespond);
    }
    resolution.matched
}

/// Fills only a missing context limit while preserving explicit provider or custom modalities.
/// 仅填充缺失的上下文限制，并保留供应商或自定义模型显式声明的模态。
///
/// # Parameters
/// 参数
/// `model_id` identifies the exact model, and `capabilities` is mutated in place.
/// `model_id` 标识精确模型，`capabilities` 会被原地修改。
///
/// # Returns
/// 返回值
/// Whether the immutable common catalog matched the model exactly.
/// 不可变公共目录是否精确匹配该模型。
pub fn apply_common_model_context_default(
    model_id: &str,
    capabilities: &mut ModelCapabilities,
) -> bool {
    let resolution = resolve_common_model_capabilities(model_id);
    if capabilities.context_limit.is_none() {
        capabilities.context_limit = Some(resolution.context_window);
    }
    resolution.matched
}

/// Loads and validates the embedded generated catalog exactly once.
/// 仅一次加载并校验嵌入的生成目录。
fn common_model_capability_index() -> &'static CommonModelCapabilityIndex {
    static INDEX: OnceLock<CommonModelCapabilityIndex> = OnceLock::new();
    INDEX.get_or_init(|| {
        let catalog: CommonModelCapabilityCatalog =
            serde_json::from_str(include_str!("../resources/common_model_capabilities.json"))
                .expect("embedded common model capability catalog must be valid JSON");
        assert_eq!(
            catalog.schema_version, 1,
            "embedded common model capability catalog schema must be version one"
        );
        assert!(
            !catalog.sources.is_empty()
                && catalog
                    .sources
                    .iter()
                    .all(|source| !source.trim().is_empty()),
            "embedded common model capability catalog sources must not be blank"
        );
        assert_eq!(
            catalog.fallback.context_window, COMMON_MODEL_FALLBACK_CONTEXT_WINDOW,
            "embedded common model capability fallback context must match the runtime constant"
        );
        assert_eq!(
            catalog.fallback.input_modalities,
            vec![CommonModelModality::Text],
            "embedded common model capability fallback input must remain text-only"
        );
        assert_eq!(
            catalog.fallback.output_modalities,
            vec![CommonModelModality::Text],
            "embedded common model capability fallback output must remain text-only"
        );
        let mut records = BTreeMap::new();
        for record in catalog.models {
            assert!(
                !record.key.trim().is_empty(),
                "embedded common model capability keys must not be blank"
            );
            assert_eq!(
                record.key,
                normalize_model_key(record.key.as_str()),
                "embedded common model capability keys must be normalized"
            );
            assert!(
                record.context_window > 0,
                "embedded common model capability context windows must be positive"
            );
            let key = record.key.clone();
            assert!(
                records.insert(key.clone(), record).is_none(),
                "embedded common model capability key `{key}` must be unique"
            );
        }
        let mut aliases = BTreeMap::new();
        for (key, record) in &records {
            for alias in &record.aliases {
                assert!(
                    !alias.trim().is_empty(),
                    "embedded common model aliases must not be blank"
                );
                assert_eq!(
                    alias,
                    &normalize_model_key(alias.as_str()),
                    "embedded common model aliases must be normalized"
                );
                assert!(
                    !records.contains_key(alias),
                    "embedded common model alias `{alias}` conflicts with a canonical key"
                );
                assert!(
                    aliases.insert(alias.clone(), key.clone()).is_none(),
                    "embedded common model alias `{alias}` must be unique"
                );
            }
        }
        CommonModelCapabilityIndex { records, aliases }
    })
}

/// Normalizes one externally supplied model identifier for exact matching.
/// 规范化一个外部模型标识以执行精确匹配。
fn normalize_model_key(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

/// Projects non-text modalities into the provider-neutral resource vocabulary.
/// 将非文本模态投影为供应商无关的资源词表。
fn resource_kinds(modalities: &[CommonModelModality]) -> Vec<ResourceKind> {
    modalities
        .iter()
        .filter_map(|modality| match modality {
            CommonModelModality::Text => None,
            CommonModelModality::Image => Some(ResourceKind::Image),
            CommonModelModality::Audio => Some(ResourceKind::Audio),
            CommonModelModality::Video => Some(ResourceKind::Video),
            CommonModelModality::File => Some(ResourceKind::File),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies exact OpenRouter identifiers expose generated context values.
    /// 验证精确 OpenRouter 标识公开生成的上下文值。
    #[test]
    fn exact_identifier_resolves_generated_capabilities() {
        let resolution = resolve_common_model_capabilities("deepseek/deepseek-v3.2");
        assert!(resolution.matched);
        assert!(resolution.context_window > 0);
    }

    /// Verifies an unknown identifier gets the documented conservative fallback.
    /// 验证未知标识获得文档规定的保守默认值。
    #[test]
    fn unknown_model_uses_text_only_two_hundred_thousand_fallback() {
        let resolution = resolve_common_model_capabilities("vendor/private-model-never-listed");
        assert!(!resolution.matched);
        assert_eq!(resolution.context_window, 200_000);
        assert!(resolution.supports_text_input);
        assert!(resolution.supports_text_output);
        assert!(resolution.input_resources.is_empty());
        assert!(resolution.output_resources.is_empty());
    }

    /// Verifies an exact non-text-output model cannot inherit conversation availability.
    /// 验证精确的非文本输出模型不能继承会话可用性。
    #[test]
    fn exact_embedding_record_removes_conversation_operation() {
        let mut capabilities = ModelCapabilities {
            operations: vec![vulcan_ai_contract::AiOperation::ConversationRespond],
            ..ModelCapabilities::default()
        };

        let matched = apply_common_model_capability_defaults(
            "openai/text-embedding-3-small",
            &mut capabilities,
        );

        assert!(matched);
        assert!(
            !capabilities
                .operations
                .contains(&vulcan_ai_contract::AiOperation::ConversationRespond)
        );
    }

    /// Verifies specific declarations are never overwritten by the common catalog.
    /// 验证公共目录不会覆盖具体来源声明。
    #[test]
    fn common_defaults_only_fill_missing_fields() {
        let mut capabilities = ModelCapabilities {
            context_limit: Some(32_000),
            input_resources: vec![ResourceKind::File],
            ..ModelCapabilities::default()
        };
        apply_common_model_capability_defaults("deepseek/deepseek-v3.2", &mut capabilities);
        assert_eq!(capabilities.context_limit, Some(32_000));
        assert_eq!(capabilities.input_resources, vec![ResourceKind::File]);
    }

    /// Verifies context-only enrichment never invents modalities over an explicit text-only source.
    /// 验证仅上下文补全不会覆盖显式纯文本来源并虚构模态。
    #[test]
    fn context_only_default_preserves_explicit_text_only_modalities() {
        let mut capabilities = ModelCapabilities::default();
        apply_common_model_context_default("deepseek/deepseek-v3.2", &mut capabilities);
        assert!(capabilities.context_limit.is_some());
        assert!(capabilities.input_resources.is_empty());
        assert!(capabilities.output_resources.is_empty());
    }
}
