//! Offline common model capability catalog and exact matching.
//! 离线公共模型能力目录与精确匹配。

use crate::{CUSTOM_MODEL_TEMPLATE_REVISION, CustomModelTemplateSelection};
use serde::{Deserialize, Serialize};
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
    /// Declared context window in tokens, absent when a media model has no token limit.
    /// 已声明的上下文 Token 窗口；媒体模型没有 Token 上限时缺省。
    #[serde(default)]
    context_window: Option<u64>,
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
    /// Separator-normalized full identifiers that resolve without ambiguity.
    /// 分隔符规范化后且不存在歧义的完整模型标识。
    canonical_identifiers: BTreeMap<String, Option<DeterministicTemplateMatch>>,
    /// Provider-independent identifier tails that resolve without ambiguity.
    /// 去除供应商命名空间后且不存在歧义的模型标识尾部。
    canonical_tails: BTreeMap<String, Option<String>>,
}

/// Identifies one deterministic full-identifier match.
/// 标识一次确定性的完整模型标识匹配。
#[derive(Debug, Clone, PartialEq, Eq)]
struct DeterministicTemplateMatch {
    /// Canonical template key selected by the match.
    /// 匹配选中的规范模板键。
    template_key: String,
    /// Exact classification exposed to the consumer.
    /// 暴露给消费者的精确匹配分类。
    match_kind: CommonModelTemplateMatchKind,
}

/// Reports the resolved public defaults and whether an exact catalog match occurred.
/// 报告解析后的公共默认值以及是否发生精确目录匹配。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommonModelCapabilityResolution {
    /// Whether the catalog matched an exact key or registered alias.
    /// 目录是否命中了精确键或已登记别名。
    pub matched: bool,
    /// Resolved context window when one is actually known or a text fallback applies.
    /// 已知真实窗口或适用文本回退值时解析出的上下文窗口。
    pub context_window: Option<u64>,
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

/// Identifies how closely one user-entered model identifier matched a common template.
/// 标识用户输入模型标识与公共模板的匹配方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommonModelTemplateMatchKind {
    /// Exact canonical-key match.
    /// 精确规范键匹配。
    Exact,
    /// Exact registered-alias match.
    /// 精确已登记别名匹配。
    Alias,
    /// Exact model-name-tail match after removing a provider prefix.
    /// 移除供应商前缀后的精确模型名尾部匹配。
    ProviderAgnostic,
}

/// Describes one offline common-capability template recommendation.
/// 描述一项离线公共能力模板建议。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommonModelTemplateSuggestion {
    /// Canonical immutable template key.
    /// 不可变模板规范键。
    pub template_key: String,
    /// Immutable resource revision.
    /// 不可变资源修订号。
    pub template_revision: String,
    /// Match classification shown to the user.
    /// 向用户显示的匹配分类。
    pub match_kind: CommonModelTemplateMatchKind,
    /// Context window carried by the template.
    /// 模板携带的上下文窗口。
    pub context_limit: u64,
    /// Whether the template explicitly supports text input.
    /// 模板是否明确支持文本输入。
    pub supports_text_input: bool,
    /// Whether the template explicitly supports text output.
    /// 模板是否明确支持文本输出。
    pub supports_text_output: bool,
    /// Non-text input resources carried by the template.
    /// 模板携带的非文本输入资源。
    pub input_resources: Vec<ResourceKind>,
    /// Non-text output resources carried by the template.
    /// 模板携带的非文本输出资源。
    pub output_resources: Vec<ResourceKind>,
}

impl CommonModelTemplateSuggestion {
    /// Freezes this suggestion into the persisted user-reviewed template snapshot.
    /// 将此建议冻结为持久化的用户审查模板快照。
    ///
    /// # Returns
    /// 返回值
    /// A provider-neutral immutable selection.
    /// 返回供应商无关的不可变选择。
    pub fn selection(&self) -> CustomModelTemplateSelection {
        CustomModelTemplateSelection {
            template_key: self.template_key.clone(),
            template_revision: self.template_revision.clone(),
            context_limit: self.context_limit,
            supports_text_input: self.supports_text_input,
            supports_text_output: self.supports_text_output,
            input_resources: self.input_resources.clone(),
            // Generic custom providers expose text conversation output only; native providers own resource-producing models.
            // 通用自定义供应商仅公开文本会话输出；资源生成模型由原生供应商承载。
            output_resources: Vec::new(),
        }
    }
}

/// Returns the only deterministic offline common template for one model identifier.
/// 返回一个模型标识唯一确定的离线公共模板。
///
/// # Parameters
/// 参数
/// `model_id` is the exact identifier entered by the user or returned by `/models`.
/// `model_id` 是用户输入或 `/models` 返回的精确标识。
///
/// # Returns
/// 返回值
/// One exact text-conversation candidate, or an empty list when no safe match exists.
/// 返回一个精确的文本会话候选；不存在安全匹配时返回空列表。
pub fn suggest_common_model_templates(model_id: &str) -> Vec<CommonModelTemplateSuggestion> {
    deterministic_template_match(model_id)
        .and_then(|matched| {
            common_model_capability_index()
                .records
                .get(matched.template_key.as_str())
                .map(|record| template_suggestion(record, matched.match_kind))
        })
        .filter(|suggestion| {
            suggestion.supports_text_input
                && suggestion.supports_text_output
                && suggestion.output_resources.is_empty()
        })
        .into_iter()
        .collect()
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
    let index = common_model_capability_index();
    let record = deterministic_template_match(model_id)
        .and_then(|matched| index.records.get(matched.template_key.as_str()));
    match record {
        Some(record) => CommonModelCapabilityResolution {
            matched: true,
            context_window: record.context_window.or_else(|| {
                record
                    .output_modalities
                    .contains(&CommonModelModality::Text)
                    .then_some(COMMON_MODEL_FALLBACK_CONTEXT_WINDOW)
            }),
            supports_text_input: record.input_modalities.contains(&CommonModelModality::Text),
            supports_text_output: record
                .output_modalities
                .contains(&CommonModelModality::Text),
            input_resources: resource_kinds(record.input_modalities.as_slice()),
            output_resources: resource_kinds(record.output_modalities.as_slice()),
        },
        None => CommonModelCapabilityResolution {
            matched: false,
            context_window: Some(COMMON_MODEL_FALLBACK_CONTEXT_WINDOW),
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
        capabilities.context_limit = resolution.context_window;
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
        capabilities.context_limit = resolution.context_window;
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
                record.context_window.is_none_or(|limit| limit > 0),
                "embedded common model capability context windows must be positive when declared"
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
        let mut canonical_identifiers = BTreeMap::new();
        let mut canonical_tails = BTreeMap::new();
        for (key, record) in &records {
            register_deterministic_identifier(
                &mut canonical_identifiers,
                &mut canonical_tails,
                record.key.as_str(),
                key.as_str(),
                CommonModelTemplateMatchKind::Exact,
            );
            for alias in &record.aliases {
                register_deterministic_identifier(
                    &mut canonical_identifiers,
                    &mut canonical_tails,
                    alias.as_str(),
                    key.as_str(),
                    CommonModelTemplateMatchKind::Alias,
                );
            }
        }
        CommonModelCapabilityIndex {
            records,
            aliases,
            canonical_identifiers,
            canonical_tails,
        }
    })
}

/// Normalizes one externally supplied model identifier for exact matching.
/// 规范化一个外部模型标识以执行精确匹配。
fn normalize_model_key(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

/// Canonicalizes case and punctuation without guessing a different model name.
/// 仅规范化大小写与标点，不猜测其他模型名称。
fn canonical_model_identifier(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .split(['/', ':'])
        .map(canonical_model_segment)
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

/// Canonicalizes one namespace segment by collapsing punctuation into one hyphen.
/// 将一个命名空间片段中的标点折叠为单个连字符。
fn canonical_model_segment(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut separator_pending = false;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            if separator_pending && !output.is_empty() {
                output.push('-');
            }
            output.push(character);
            separator_pending = false;
        } else if !output.is_empty() {
            separator_pending = true;
        }
    }
    output
}

/// Returns the provider-independent tail of one canonical identifier.
/// 返回规范模型标识中与供应商无关的尾部。
fn canonical_model_tail(value: &str) -> &str {
    value.rsplit('/').next().unwrap_or(value)
}

/// Registers one deterministic identifier while turning cross-template collisions into no-match entries.
/// 注册一个确定性标识，并将跨模板冲突转换为不可匹配条目。
fn register_deterministic_identifier(
    identifiers: &mut BTreeMap<String, Option<DeterministicTemplateMatch>>,
    tails: &mut BTreeMap<String, Option<String>>,
    identifier: &str,
    template_key: &str,
    match_kind: CommonModelTemplateMatchKind,
) {
    let canonical = canonical_model_identifier(identifier);
    if canonical.is_empty() {
        return;
    }
    register_full_match(
        identifiers,
        canonical.clone(),
        DeterministicTemplateMatch {
            template_key: template_key.to_string(),
            match_kind,
        },
    );
    register_tail_match(
        tails,
        canonical_model_tail(canonical.as_str()).to_string(),
        template_key.to_string(),
    );
}

/// Preserves one unique full match and rejects canonical collisions.
/// 保留唯一完整匹配并拒绝规范化冲突。
fn register_full_match(
    identifiers: &mut BTreeMap<String, Option<DeterministicTemplateMatch>>,
    canonical: String,
    incoming: DeterministicTemplateMatch,
) {
    match identifiers.get_mut(canonical.as_str()) {
        None => {
            identifiers.insert(canonical, Some(incoming));
        }
        Some(None) => {}
        Some(Some(existing)) if existing.template_key != incoming.template_key => {
            identifiers.insert(canonical, None);
        }
        Some(Some(existing))
            if existing.match_kind == CommonModelTemplateMatchKind::Alias
                && incoming.match_kind == CommonModelTemplateMatchKind::Exact =>
        {
            *existing = incoming;
        }
        Some(Some(_)) => {}
    }
}

/// Preserves one unique namespace-free match and rejects cross-template collisions.
/// 保留唯一无命名空间匹配并拒绝跨模板冲突。
fn register_tail_match(
    tails: &mut BTreeMap<String, Option<String>>,
    tail: String,
    template_key: String,
) {
    match tails.get_mut(tail.as_str()) {
        None => {
            tails.insert(tail, Some(template_key));
        }
        Some(None) => {}
        Some(Some(existing)) if existing != &template_key => {
            tails.insert(tail, None);
        }
        Some(Some(_)) => {}
    }
}

/// Resolves only exact keys, explicit aliases, punctuation-normalized equals, or unique namespace tails.
/// 仅解析精确键、显式别名、标点规范化相等或唯一命名空间尾部。
fn deterministic_template_match(model_id: &str) -> Option<DeterministicTemplateMatch> {
    let normalized = normalize_model_key(model_id);
    if normalized.is_empty() {
        return None;
    }
    let index = common_model_capability_index();
    if index.records.contains_key(normalized.as_str()) {
        return Some(DeterministicTemplateMatch {
            template_key: normalized,
            match_kind: CommonModelTemplateMatchKind::Exact,
        });
    }
    if let Some(template_key) = index.aliases.get(normalized.as_str()) {
        return Some(DeterministicTemplateMatch {
            template_key: template_key.clone(),
            match_kind: CommonModelTemplateMatchKind::Alias,
        });
    }

    let canonical = canonical_model_identifier(model_id);
    if let Some(Some(matched)) = index.canonical_identifiers.get(canonical.as_str()) {
        return Some(matched.clone());
    }
    index
        .canonical_tails
        .get(canonical_model_tail(canonical.as_str()))
        .and_then(Option::as_ref)
        .map(|template_key| DeterministicTemplateMatch {
            template_key: template_key.clone(),
            match_kind: CommonModelTemplateMatchKind::ProviderAgnostic,
        })
}

/// Builds one public recommendation from an immutable catalog record.
/// 从不可变目录记录构建一项公共建议。
fn template_suggestion(
    record: &CommonModelCapabilityRecord,
    match_kind: CommonModelTemplateMatchKind,
) -> CommonModelTemplateSuggestion {
    CommonModelTemplateSuggestion {
        template_key: record.key.clone(),
        template_revision: String::from(CUSTOM_MODEL_TEMPLATE_REVISION),
        match_kind,
        context_limit: record
            .context_window
            .unwrap_or(COMMON_MODEL_FALLBACK_CONTEXT_WINDOW),
        supports_text_input: record.input_modalities.contains(&CommonModelModality::Text),
        supports_text_output: record
            .output_modalities
            .contains(&CommonModelModality::Text),
        input_resources: resource_kinds(record.input_modalities.as_slice()),
        output_resources: resource_kinds(record.output_modalities.as_slice()),
    }
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
        assert!(resolution.context_window.is_some_and(|limit| limit > 0));
    }

    /// Verifies an unknown identifier gets the documented conservative fallback.
    /// 验证未知标识获得文档规定的保守默认值。
    #[test]
    fn unknown_model_uses_text_only_two_hundred_thousand_fallback() {
        let resolution = resolve_common_model_capabilities("vendor/private-model-never-listed");
        assert!(!resolution.matched);
        assert_eq!(resolution.context_window, Some(200_000));
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
            "qwen3.7-text-embedding",
            &mut capabilities,
        );

        assert!(matched);
        assert!(
            !capabilities
                .operations
                .contains(&vulcan_ai_contract::AiOperation::ConversationRespond)
        );
    }

    /// Verifies a media model with no declared token window remains unspecified in the catalog.
    /// 验证未声明 Token 窗口的媒体模型在目录中继续保持未设置状态。
    #[test]
    fn media_model_without_declared_window_does_not_receive_text_fallback() {
        let resolution = resolve_common_model_capabilities("grok-imagine-video");
        assert!(resolution.matched);
        assert_eq!(resolution.context_window, None);
        assert!(!resolution.supports_text_output);
        let mut capabilities = ModelCapabilities::default();
        apply_common_model_capability_defaults("grok-imagine-video", &mut capabilities);
        assert_eq!(capabilities.context_limit, None);
        assert_eq!(capabilities.output_resources, vec![ResourceKind::Video]);
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

    /// Verifies punctuation and letter case normalize to one exact template.
    /// 验证标点与字母大小写会规范化到同一个精确模板。
    ///
    /// Record identities are owned by our own provider manifests, so the assertion pins the
    /// suffix matching contract instead of the upstream `vendor/model` spelling.
    /// 记录身份由我们自己的供应商清单拥有，因此这里断言后缀匹配契约，而不是上游的 `vendor/model` 写法。
    #[test]
    fn punctuation_and_case_normalization_remains_exact() {
        let suggestions = suggest_common_model_templates("DeepSeek:DeepSeek V3,2");
        assert_eq!(suggestions.len(), 1);
        assert!(
            suggestions[0].template_key.ends_with("deepseek-v3.2"),
            "unexpected template key {}",
            suggestions[0].template_key
        );
    }

    /// Verifies resource-producing native models are never suggested to custom conversation providers.
    /// 验证资源生成型原生模型绝不会推荐给自定义会话供应商。
    #[test]
    fn non_text_models_are_not_custom_conversation_templates() {
        assert!(suggest_common_model_templates("openai/gpt-image-1").is_empty());
    }

    /// Verifies the explicit GPT-5.3 Codex Spark record exposes its reviewed context window.
    /// 验证显式 GPT-5.3 Codex Spark 记录公开其已审核的上下文窗口。
    ///
    /// The catalog inventory and declared values now derive from our own provider manifests, so
    /// the reviewed `openai.json` declaration governs this number.
    /// 目录的清单与声明值现在派生自我们自己的供应商清单，因此该数值由已审核的 `openai.json` 声明决定。
    #[test]
    fn gpt_five_three_codex_spark_uses_explicit_context() {
        let resolution = resolve_common_model_capabilities("GPT 5,3 Codex Spark");
        assert!(resolution.matched);
        assert_eq!(resolution.context_window, Some(128_000));
        assert!(resolution.supports_text_input);
        assert!(resolution.supports_text_output);
    }

    /// Verifies GPT-6 Astra inherits the GPT-5.6 Sol default matching contract.
    /// 验证 GPT-6 Astra 继承 GPT-5.6 Sol 的默认匹配能力契约。
    #[test]
    fn gpt_six_astra_clones_gpt_five_six_sol_capabilities() {
        let astra = resolve_common_model_capabilities("gpt-6-astra");
        let sol = resolve_common_model_capabilities("gpt-5.6-sol");

        assert!(astra.matched);
        assert!(sol.matched);
        assert_eq!(astra.context_window, sol.context_window);
        assert_eq!(astra.supports_text_input, sol.supports_text_input);
        assert_eq!(astra.supports_text_output, sol.supports_text_output);
        assert_eq!(astra.input_resources, sol.input_resources);
        assert_eq!(astra.output_resources, sol.output_resources);
    }

    /// Verifies partial and version-adjacent names never produce guesses.
    /// 验证部分名称和相邻版本名称绝不会产生猜测。
    #[test]
    fn partial_and_adjacent_version_names_do_not_match() {
        let partial = suggest_common_model_templates("deepseek-v3-preview");
        assert!(
            partial.is_empty(),
            "unexpected deterministic match: {partial:#?}"
        );
        assert!(suggest_common_model_templates("deepseek/deepseek-v4.2").is_empty());
    }
}
