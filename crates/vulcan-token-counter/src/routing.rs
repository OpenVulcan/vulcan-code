//! Model-name routing rules for tokenizer tier selection.
//! 用于 tokenizer 层级选择的模型名称路由规则。

use crate::types::{
    ExactTokenizerKey, OpenAiEncoding, ProxyTokenizerFamily, TokenEstimatePrecision, TokenizerTier,
};

/// Safety multiplier for eastern bilingual/code models using the multilingual proxy.
/// 东方双语/代码模型使用多语言代理时的安全倍率。
const EASTERN_MIXED_PROXY_MULTIPLIER: f64 = 1.08;

/// Safety multiplier for western open/code models using the multilingual proxy.
/// 西方开源/代码模型使用多语言代理时的安全倍率。
const WESTERN_OPEN_PROXY_MULTIPLIER: f64 = 1.05;

/// Safety multiplier for Gemini-family models without a dedicated local exact tokenizer.
/// Gemini 模型族没有专用本地精确 tokenizer 时的安全倍率。
const GEMINI_PROXY_MULTIPLIER: f64 = 1.08;

/// Safety multiplier for Claude-family models without a dedicated local exact tokenizer.
/// Claude 模型族没有专用本地精确 tokenizer 时的安全倍率。
const CLAUDE_PROXY_MULTIPLIER: f64 = 1.10;

/// Safety multiplier for Cohere-family models without a dedicated local exact tokenizer.
/// Cohere 模型族没有专用本地精确 tokenizer 时的安全倍率。
const COHERE_PROXY_MULTIPLIER: f64 = 1.15;

/// Safety multiplier for OpenAI-compatible open models without exact local billing metadata.
/// 没有精确本地账单元数据的 OpenAI 兼容开放模型安全倍率。
const OPENAI_COMPATIBLE_PROXY_MULTIPLIER: f64 = 1.03;

/// Safety multiplier for completely unknown model names.
/// 完全未知模型名称使用的安全倍率。
const UNKNOWN_MODEL_MULTIPLIER: f64 = 1.15;

/// Routes one model name into the tokenizer tier used for counting.
/// 将单个模型名称路由到用于统计的 tokenizer 层级。
///
/// # Parameters
/// 参数
/// `model_name` is matched case-insensitively against provider and model-family fragments.
/// `model_name` 会以大小写不敏感方式匹配供应商与模型族片段。
///
/// # Returns
/// 返回值
/// The tier that should be used by `TokenCounter::estimate`.
/// 应由 `TokenCounter::estimate` 使用的层级。
pub fn route_model(model_name: &str) -> TokenizerTier {
    let normalized = model_name.to_ascii_lowercase();

    if matches_any(&normalized, &["gpt-5", "gpt-4.5", "gpt-4.1", "gpt-4o"])
        || matches_any_model_token(&normalized, &["o1", "o3", "o4"])
        || matches_any_model_token(&normalized, &["codex"])
    {
        return native_openai_tier(OpenAiEncoding::O200kBase);
    }

    if matches_any(&normalized, &["gpt-4-turbo", "gpt-4", "gpt-3.5"]) {
        return native_openai_tier(OpenAiEncoding::Cl100kBase);
    }

    if normalized.contains("gpt-oss") {
        return proxy_tier(
            ProxyTokenizerFamily::OpenAiCompatible,
            OpenAiEncoding::O200kBase,
            OPENAI_COMPATIBLE_PROXY_MULTIPLIER,
        );
    }

    if normalized.contains("claude") || normalized.contains("anthropic") {
        return proxy_tier(
            ProxyTokenizerFamily::Claude,
            OpenAiEncoding::Cl100kBase,
            CLAUDE_PROXY_MULTIPLIER,
        );
    }

    if normalized.contains("gemini") {
        return exact_tier_with_local_policy(
            ExactTokenizerKey::Gemma,
            TokenEstimatePrecision::Proxy,
            GEMINI_PROXY_MULTIPLIER,
            ProxyTokenizerFamily::Gemini,
            OpenAiEncoding::O200kBase,
            GEMINI_PROXY_MULTIPLIER,
        );
    }

    if normalized.contains("gemma") {
        return exact_tier(
            ExactTokenizerKey::Gemma,
            ProxyTokenizerFamily::Gemini,
            OpenAiEncoding::O200kBase,
            GEMINI_PROXY_MULTIPLIER,
        );
    }

    if normalized.contains("qwen") && matches_any(&normalized, &["coder", "code"]) {
        return exact_tier(
            ExactTokenizerKey::QwenCoder,
            ProxyTokenizerFamily::EasternMixed,
            OpenAiEncoding::O200kBase,
            EASTERN_MIXED_PROXY_MULTIPLIER,
        );
    }

    if normalized.contains("qwen") || normalized.contains("qwq") {
        return exact_tier(
            ExactTokenizerKey::Qwen,
            ProxyTokenizerFamily::EasternMixed,
            OpenAiEncoding::O200kBase,
            EASTERN_MIXED_PROXY_MULTIPLIER,
        );
    }

    if normalized.contains("deepseek") && normalized.contains("r1") {
        return exact_tier(
            ExactTokenizerKey::DeepSeekR1,
            ProxyTokenizerFamily::EasternMixed,
            OpenAiEncoding::O200kBase,
            EASTERN_MIXED_PROXY_MULTIPLIER,
        );
    }

    if normalized.contains("deepseek") {
        return exact_tier(
            ExactTokenizerKey::DeepSeek,
            ProxyTokenizerFamily::EasternMixed,
            OpenAiEncoding::O200kBase,
            EASTERN_MIXED_PROXY_MULTIPLIER,
        );
    }

    if normalized.contains("minimax") {
        return exact_tier(
            ExactTokenizerKey::MiniMax,
            ProxyTokenizerFamily::EasternMixed,
            OpenAiEncoding::O200kBase,
            EASTERN_MIXED_PROXY_MULTIPLIER,
        );
    }

    if normalized.contains("glm") || normalized.contains("chatglm") {
        return exact_tier(
            ExactTokenizerKey::Glm,
            ProxyTokenizerFamily::EasternMixed,
            OpenAiEncoding::O200kBase,
            EASTERN_MIXED_PROXY_MULTIPLIER,
        );
    }

    if matches_any(&normalized, &["hunyuan", "hy3", "hy-3"]) {
        return exact_tier(
            ExactTokenizerKey::Hunyuan,
            ProxyTokenizerFamily::EasternMixed,
            OpenAiEncoding::O200kBase,
            EASTERN_MIXED_PROXY_MULTIPLIER,
        );
    }

    if normalized.contains("ernie") {
        return exact_tier(
            ExactTokenizerKey::Ernie,
            ProxyTokenizerFamily::EasternMixed,
            OpenAiEncoding::O200kBase,
            EASTERN_MIXED_PROXY_MULTIPLIER,
        );
    }

    if normalized.contains("llama") {
        return exact_tier(
            ExactTokenizerKey::Llama,
            ProxyTokenizerFamily::WesternOpen,
            OpenAiEncoding::O200kBase,
            WESTERN_OPEN_PROXY_MULTIPLIER,
        );
    }

    if matches_any(
        &normalized,
        &[
            "mistral",
            "mixtral",
            "magistral",
            "codestral",
            "devstral",
            "ministral",
            "pixtral",
            "voxtral",
        ],
    ) {
        return exact_tier(
            ExactTokenizerKey::Mistral,
            ProxyTokenizerFamily::WesternOpen,
            OpenAiEncoding::O200kBase,
            WESTERN_OPEN_PROXY_MULTIPLIER,
        );
    }

    if normalized.contains("nemotron") {
        return exact_tier(
            ExactTokenizerKey::Nemotron,
            ProxyTokenizerFamily::WesternOpen,
            OpenAiEncoding::O200kBase,
            WESTERN_OPEN_PROXY_MULTIPLIER,
        );
    }

    if matches_any_model_token(&normalized, &["phi"]) {
        return exact_tier(
            ExactTokenizerKey::Phi,
            ProxyTokenizerFamily::WesternOpen,
            OpenAiEncoding::O200kBase,
            WESTERN_OPEN_PROXY_MULTIPLIER,
        );
    }

    if is_eastern_mixed_model(&normalized) {
        return proxy_tier(
            ProxyTokenizerFamily::EasternMixed,
            OpenAiEncoding::O200kBase,
            EASTERN_MIXED_PROXY_MULTIPLIER,
        );
    }

    if normalized.contains("cohere")
        || normalized.contains("command-a")
        || normalized.contains("command-r")
        || normalized.contains("command_r")
    {
        return proxy_tier(
            ProxyTokenizerFamily::Cohere,
            OpenAiEncoding::Cl100kBase,
            COHERE_PROXY_MULTIPLIER,
        );
    }

    if is_western_open_model(&normalized) {
        return proxy_tier(
            ProxyTokenizerFamily::WesternOpen,
            OpenAiEncoding::O200kBase,
            WESTERN_OPEN_PROXY_MULTIPLIER,
        );
    }

    proxy_tier(
        ProxyTokenizerFamily::Unknown,
        OpenAiEncoding::O200kBase,
        UNKNOWN_MODEL_MULTIPLIER,
    )
}

/// Builds an exact OpenAI tier.
/// 构建一个精确 OpenAI 层级。
///
/// # Parameters
/// 参数
/// `encoding` selects the exact OpenAI tokenizer.
/// `encoding` 选择精确 OpenAI tokenizer。
///
/// # Returns
/// 返回值
/// The exact OpenAI tokenizer tier.
/// 精确 OpenAI tokenizer 层级。
fn native_openai_tier(encoding: OpenAiEncoding) -> TokenizerTier {
    TokenizerTier::NativeOpenAi { encoding }
}

/// Builds one budget proxy tier.
/// 构建一个预算代理层级。
///
/// # Parameters
/// 参数
/// `family` identifies the semantic proxy group.
/// `family` 标识语义代理分组。
/// `proxy_encoding` selects the OpenAI tokenizer used as the base.
/// `proxy_encoding` 选择作为基准的 OpenAI tokenizer。
/// `multiplier` is the safety multiplier for the proxy.
/// `multiplier` 是该代理使用的安全倍率。
///
/// # Returns
/// 返回值
/// The configured budget proxy tier.
/// 配置后的预算代理层级。
fn proxy_tier(
    family: ProxyTokenizerFamily,
    proxy_encoding: OpenAiEncoding,
    multiplier: f64,
) -> TokenizerTier {
    TokenizerTier::BudgetProxy {
        family,
        proxy_encoding,
        multiplier,
    }
}

/// Builds one exact tokie tier with an OpenAI proxy fallback.
/// 构建一个带 OpenAI 代理回退的精确 tokie 层级。
///
/// # Parameters
/// 参数
/// `key` selects the exact tokenizer asset.
/// `key` 选择精确 tokenizer 资产。
/// `fallback_family` identifies the semantic proxy group for fallback.
/// `fallback_family` 标识回退时使用的语义代理分组。
/// `fallback_encoding` selects the OpenAI tokenizer used as fallback base.
/// `fallback_encoding` 选择作为回退基准的 OpenAI tokenizer。
/// `fallback_multiplier` is the safety multiplier for fallback.
/// `fallback_multiplier` 是回退时使用的安全倍率。
///
/// # Returns
/// 返回值
/// The configured exact tokie tier.
/// 配置后的精确 tokie 层级。
fn exact_tier(
    key: ExactTokenizerKey,
    fallback_family: ProxyTokenizerFamily,
    fallback_encoding: OpenAiEncoding,
    fallback_multiplier: f64,
) -> TokenizerTier {
    exact_tier_with_local_policy(
        key,
        TokenEstimatePrecision::Exact,
        1.0,
        fallback_family,
        fallback_encoding,
        fallback_multiplier,
    )
}

/// Builds one tokie tier with custom local-count precision and multiplier.
/// 构建一个带自定义本地统计精度与倍率的 tokie 层级。
///
/// # Parameters
/// 参数
/// `key` selects the local tokenizer asset.
/// `key` 选择本地 tokenizer 资产。
/// `local_precision` is reported when the local tokenizer is available.
/// `local_precision` 是本地 tokenizer 可用时报告的精度。
/// `local_multiplier` is applied to local tokenizer counts.
/// `local_multiplier` 会应用到本地 tokenizer 统计结果。
/// `fallback_family` identifies the semantic proxy group for fallback.
/// `fallback_family` 标识回退时使用的语义代理分组。
/// `fallback_encoding` selects the OpenAI tokenizer used as fallback base.
/// `fallback_encoding` 选择作为回退基准的 OpenAI tokenizer。
/// `fallback_multiplier` is the safety multiplier for fallback.
/// `fallback_multiplier` 是回退时使用的安全倍率。
///
/// # Returns
/// 返回值
/// The configured tokie tier.
/// 配置后的 tokie 层级。
fn exact_tier_with_local_policy(
    key: ExactTokenizerKey,
    local_precision: TokenEstimatePrecision,
    local_multiplier: f64,
    fallback_family: ProxyTokenizerFamily,
    fallback_encoding: OpenAiEncoding,
    fallback_multiplier: f64,
) -> TokenizerTier {
    TokenizerTier::ExactTokie {
        key,
        local_precision,
        local_multiplier,
        fallback_family,
        fallback_encoding,
        fallback_multiplier,
    }
}

/// Returns the OpenAI encoding needed by one routed tier.
/// 返回单个已路由层级需要的 OpenAI 编码。
///
/// # Parameters
/// 参数
/// `tier` is a routed tokenizer tier.
/// `tier` 是一个已路由 tokenizer 层级。
///
/// # Returns
/// 返回值
/// The OpenAI encoding used by that tier.
/// 该层级使用的 OpenAI 编码。
pub(super) fn tier_openai_encoding(tier: &TokenizerTier) -> OpenAiEncoding {
    match tier {
        TokenizerTier::NativeOpenAi { encoding } => *encoding,
        TokenizerTier::BudgetProxy { proxy_encoding, .. } => *proxy_encoding,
        TokenizerTier::ExactTokie {
            fallback_encoding, ..
        } => *fallback_encoding,
    }
}

/// Returns whether one normalized model name belongs to the eastern mixed proxy group.
/// 返回单个规范化模型名称是否属于东方混合代理组。
///
/// # Parameters
/// 参数
/// `normalized` is the lowercase model name.
/// `normalized` 是小写模型名称。
///
/// # Returns
/// 返回值
/// `true` when the model should use the eastern mixed proxy.
/// 当该模型应使用东方混合代理时返回 `true`。
fn is_eastern_mixed_model(normalized: &str) -> bool {
    matches_any(
        normalized,
        &[
            "qwen",
            "qwq",
            "deepseek",
            "glm",
            "chatglm",
            "kimi",
            "moonshot",
            "minimax",
            "hunyuan",
            "hy3",
            "hy-3",
            "ernie",
            "baidu",
            "mimo",
            "xiaomi",
            "seed",
            "doubao",
            "bytedance",
            "volcengine",
        ],
    )
}

/// Returns whether one normalized model name belongs to the western open proxy group.
/// 返回单个规范化模型名称是否属于西方开源代理组。
///
/// # Parameters
/// 参数
/// `normalized` is the lowercase model name.
/// `normalized` 是小写模型名称。
///
/// # Returns
/// 返回值
/// `true` when the model should use the western open proxy.
/// 当该模型应使用西方开源代理时返回 `true`。
fn is_western_open_model(normalized: &str) -> bool {
    matches_any(
        normalized,
        &[
            "llama",
            "grok",
            "mistral",
            "mixtral",
            "magistral",
            "codestral",
            "devstral",
            "ministral",
            "pixtral",
            "voxtral",
            "nvidia",
            "nemotron",
        ],
    ) || matches_any_model_token(normalized, &["phi"])
}

/// Returns whether a string contains any literal fragment.
/// 返回字符串是否包含任意字面量片段。
///
/// # Parameters
/// 参数
/// `value` is the normalized model name.
/// `value` 是规范化后的模型名称。
/// `needles` contains fragments to search for.
/// `needles` 包含需要搜索的片段。
///
/// # Returns
/// 返回值
/// `true` when any fragment is present.
/// 当任意片段存在时返回 `true`。
fn matches_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

/// Returns whether a string contains any model token with non-alphanumeric boundaries.
/// 返回字符串是否包含任意带非字母数字边界的模型标记。
///
/// # Parameters
/// 参数
/// `value` is the normalized model name.
/// `value` 是规范化后的模型名称。
/// `tokens` contains short model tokens such as `o3` or `phi`.
/// `tokens` 包含 `o3` 或 `phi` 等短模型标记。
///
/// # Returns
/// 返回值
/// `true` when any token appears as a bounded model token.
/// 当任意标记以有边界模型标记形式出现时返回 `true`。
fn matches_any_model_token(value: &str, tokens: &[&str]) -> bool {
    tokens.iter().any(|token| {
        value.match_indices(token).any(|(index, _)| {
            let before = index
                .checked_sub(1)
                .and_then(|previous| value.as_bytes().get(previous))
                .is_none_or(|byte| !byte.is_ascii_alphanumeric());
            let after_index = index + token.len();
            let after = value
                .as_bytes()
                .get(after_index)
                .is_none_or(|byte| !byte.is_ascii_alphanumeric());
            before && after
        })
    })
}

/// Applies a safety multiplier with ceiling rounding.
/// 使用向上取整应用安全倍率。
///
/// # Parameters
/// 参数
/// `tokens` is the base token count.
/// `tokens` 是基础 Token 数。
/// `multiplier` is the safety multiplier.
/// `multiplier` 是安全倍率。
///
/// # Returns
/// 返回值
/// The multiplied token count.
/// 应用倍率后的 Token 数。
pub(super) fn apply_multiplier(tokens: usize, multiplier: f64) -> usize {
    ((tokens as f64) * multiplier).ceil() as usize
}
