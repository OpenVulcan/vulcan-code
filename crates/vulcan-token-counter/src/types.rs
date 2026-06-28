//! Public token-counter types and diagnostics.
//! Token 统计器公开类型与诊断信息。

use std::{fmt, path::PathBuf};

use thiserror::Error;

/// OpenAI tokenizer used by exact OpenAI routes and all lightweight proxy routes.
/// 精确 OpenAI 路由与所有轻量代理路由使用的 OpenAI tokenizer。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenAiEncoding {
    /// OpenAI `o200k_base` tokenizer for recent GPT, reasoning, and multilingual proxy routes.
    /// 用于较新 GPT、推理模型与多语言代理路由的 OpenAI `o200k_base` tokenizer。
    O200kBase,
    /// OpenAI `cl100k_base` tokenizer for older GPT routes and Claude/Cohere-style proxies.
    /// 用于较旧 GPT 路由与 Claude/Cohere 类代理的 OpenAI `cl100k_base` tokenizer。
    Cl100kBase,
}

impl OpenAiEncoding {
    /// Returns the canonical `wordchipper` vocabulary name for this encoding.
    /// 返回该编码对应的标准 `wordchipper` 词表名称。
    ///
    /// # Returns
    /// 返回值
    /// A static vocabulary identifier accepted by `wordchipper::load_vocab`.
    /// 一个可被 `wordchipper::load_vocab` 接受的静态词表标识。
    pub const fn wordchipper_vocab_name(self) -> &'static str {
        match self {
            Self::O200kBase => "openai:o200k_base",
            Self::Cl100kBase => "openai:cl100k_base",
        }
    }

    /// Returns the bundled `.tiktoken` file stem for this encoding.
    /// 返回该编码对应的内置 `.tiktoken` 文件名主体。
    ///
    /// # Returns
    /// 返回值
    /// A static file stem under `resources/tokenizers/openai`.
    /// 一个位于 `resources/tokenizers/openai` 下的静态文件名主体。
    pub const fn file_stem(self) -> &'static str {
        match self {
            Self::O200kBase => "o200k_base",
            Self::Cl100kBase => "cl100k_base",
        }
    }

    /// Returns a compact human-readable label for this encoding.
    /// 返回该编码的紧凑可读标签。
    ///
    /// # Returns
    /// 返回值
    /// A static label suitable for diagnostics and test assertions.
    /// 一个适合诊断与测试断言使用的静态标签。
    pub const fn label(self) -> &'static str {
        self.file_stem()
    }
}

impl fmt::Display for OpenAiEncoding {
    /// Formats one OpenAI encoding as its stable label.
    /// 将单个 OpenAI 编码格式化为稳定标签。
    ///
    /// # Parameters
    /// 参数
    /// `formatter` receives the destination formatter supplied by Rust formatting.
    /// `formatter` 接收 Rust 格式化流程提供的目标格式器。
    ///
    /// # Returns
    /// 返回值
    /// A formatting result from the destination formatter.
    /// 来自目标格式器的格式化结果。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

/// Tokenizer backend used for OpenAI base encodings.
/// 用于 OpenAI 基础编码的 tokenizer 后端。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenizerBackend {
    /// Stable `wordchipper` backend loading bundled `.tiktoken` rank files.
    /// 稳定的 `wordchipper` 后端，加载内置 `.tiktoken` rank 文件。
    Wordchipper,
    /// Experimental `tokie` backend loading bundled `.tkz` binary tokenizer files.
    /// 实验性的 `tokie` 后端，加载内置 `.tkz` 二进制 tokenizer 文件。
    Tokie,
}

impl TokenizerBackend {
    /// Returns the resource extension used by this backend.
    /// 返回该后端使用的资源扩展名。
    ///
    /// # Returns
    /// 返回值
    /// A static extension without a leading dot.
    /// 一个不带前导点的静态扩展名。
    pub const fn resource_extension(self) -> &'static str {
        match self {
            Self::Wordchipper => "tiktoken",
            Self::Tokie => "tkz",
        }
    }

    /// Returns a compact backend label for diagnostics and benchmarks.
    /// 返回用于诊断与基准测试的紧凑后端标签。
    ///
    /// # Returns
    /// 返回值
    /// A stable backend label.
    /// 一个稳定的后端标签。
    pub const fn label(self) -> &'static str {
        match self {
            Self::Wordchipper => "wordchipper",
            Self::Tokie => "tokie",
        }
    }
}

impl fmt::Display for TokenizerBackend {
    /// Formats the backend as its stable label.
    /// 将后端格式化为稳定标签。
    ///
    /// # Parameters
    /// 参数
    /// `formatter` receives the destination formatter supplied by Rust formatting.
    /// `formatter` 接收 Rust 格式化流程提供的目标格式器。
    ///
    /// # Returns
    /// 返回值
    /// A formatting result from the destination formatter.
    /// 来自目标格式器的格式化结果。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

/// High-level proxy engine selected when a model does not need exact OpenAI billing parity.
/// 当模型不需要 OpenAI 账单级精确对齐时选择的高层代理引擎。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyTokenizerFamily {
    /// Eastern bilingual/code models such as Qwen, DeepSeek, GLM, Kimi, Seed, and Doubao.
    /// Qwen、DeepSeek、GLM、Kimi、Seed 与豆包等东方双语/代码模型。
    EasternMixed,
    /// Western open/code models such as Llama, Mistral, Grok, Nemotron, and Phi.
    /// Llama、Mistral、Grok、Nemotron 与 Phi 等西方开源/代码模型。
    WesternOpen,
    /// Gemini-family models estimated through the multilingual OpenAI proxy.
    /// 通过多语言 OpenAI 代理估算的 Gemini 模型族。
    Gemini,
    /// Claude-family models estimated through a conservative OpenAI proxy.
    /// 通过保守 OpenAI 代理估算的 Claude 模型族。
    Claude,
    /// Cohere-family models estimated through a conservative OpenAI proxy.
    /// 通过保守 OpenAI 代理估算的 Cohere 模型族。
    Cohere,
    /// OpenAI-compatible open models without a dedicated exact local billing tokenizer.
    /// 没有专用本地账单 tokenizer 的 OpenAI 兼容开放模型。
    OpenAiCompatible,
    /// Unknown models estimated through the safest multilingual proxy path.
    /// 通过最安全多语言代理路径估算的未知模型。
    Unknown,
}

/// Exact local tokenizer asset backed by tokie's `.tkz` binary format.
/// 由 tokie `.tkz` 二进制格式承载的本地精确 tokenizer 资产。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExactTokenizerKey {
    /// Qwen generic flagship tokenizer, currently backed by Qwen3.5 35B A3B.
    /// Qwen 通用旗舰 tokenizer，当前由 Qwen3.5 35B A3B 承载。
    Qwen,
    /// Qwen coding tokenizer, currently backed by Qwen3 Coder 30B A3B Instruct.
    /// Qwen 编码 tokenizer，当前由 Qwen3 Coder 30B A3B Instruct 承载。
    QwenCoder,
    /// DeepSeek V3/V4-style tokenizer.
    /// DeepSeek V3/V4 风格 tokenizer。
    DeepSeek,
    /// DeepSeek R1 reasoning tokenizer.
    /// DeepSeek R1 推理 tokenizer。
    DeepSeekR1,
    /// MiniMax tokenizer converted from its Hugging Face tokenizer JSON.
    /// 从 Hugging Face tokenizer JSON 转换而来的 MiniMax tokenizer。
    MiniMax,
    /// GLM tokenizer converted from its Hugging Face tokenizer JSON.
    /// 从 Hugging Face tokenizer JSON 转换而来的 GLM tokenizer。
    Glm,
    /// Tencent Hunyuan Hy3 tokenizer converted from its Hugging Face tokenizer JSON.
    /// 从 Hugging Face tokenizer JSON 转换而来的腾讯混元 Hy3 tokenizer。
    Hunyuan,
    /// Baidu ERNIE 4.5 tokenizer converted from its Hugging Face tokenizer JSON.
    /// 从 Hugging Face tokenizer JSON 转换而来的百度 ERNIE 4.5 tokenizer。
    Ernie,
    /// Llama flagship tokenizer.
    /// Llama 旗舰 tokenizer。
    Llama,
    /// Mistral-family tokenizer.
    /// Mistral 系列 tokenizer。
    Mistral,
    /// Phi-family tokenizer.
    /// Phi 系列 tokenizer。
    Phi,
    /// Gemma-family tokenizer used for Gemma and Gemini-compatible budgeting.
    /// 用于 Gemma 与 Gemini 兼容预算的 Gemma 系列 tokenizer。
    Gemma,
    /// NVIDIA Nemotron tokenizer converted from its Hugging Face tokenizer JSON.
    /// 从 Hugging Face tokenizer JSON 转换而来的 NVIDIA Nemotron tokenizer。
    Nemotron,
}

impl ExactTokenizerKey {
    /// Returns the stable resource directory name for this exact tokenizer.
    /// 返回该精确 tokenizer 的稳定资源目录名。
    ///
    /// # Returns
    /// 返回值
    /// A static directory name under `resources/tokenizers/exact`.
    /// 一个位于 `resources/tokenizers/exact` 下的静态目录名。
    pub const fn resource_key(self) -> &'static str {
        match self {
            Self::Qwen => "qwen",
            Self::QwenCoder => "qwen_coder",
            Self::DeepSeek => "deepseek",
            Self::DeepSeekR1 => "deepseek_r1",
            Self::MiniMax => "minimax",
            Self::Glm => "glm",
            Self::Hunyuan => "hunyuan",
            Self::Ernie => "ernie",
            Self::Llama => "llama",
            Self::Mistral => "mistral",
            Self::Phi => "phi",
            Self::Gemma => "gemma",
            Self::Nemotron => "nemotron",
        }
    }

    /// Returns a compact human-readable label for this tokenizer key.
    /// 返回该 tokenizer key 的紧凑可读标签。
    ///
    /// # Returns
    /// 返回值
    /// A static label suitable for diagnostics.
    /// 一个适合诊断使用的静态标签。
    pub const fn label(self) -> &'static str {
        self.resource_key()
    }
}

impl fmt::Display for ExactTokenizerKey {
    /// Formats the exact tokenizer key as its stable label.
    /// 将精确 tokenizer key 格式化为稳定标签。
    ///
    /// # Parameters
    /// 参数
    /// `formatter` receives the destination formatter supplied by Rust formatting.
    /// `formatter` 接收 Rust 格式化流程提供的目标格式器。
    ///
    /// # Returns
    /// 返回值
    /// A formatting result from the destination formatter.
    /// 来自目标格式器的格式化结果。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

/// Tokenizer tier selected for one model name.
/// 单个模型名称选择出的 tokenizer 层级。
#[derive(Debug, Clone, PartialEq)]
pub enum TokenizerTier {
    /// Exact OpenAI-family tokenizer backed by `wordchipper`.
    /// 由 `wordchipper` 承载的 OpenAI 体系精确 tokenizer。
    NativeOpenAi {
        /// OpenAI encoding used by this exact route.
        /// 该精确路由使用的 OpenAI 编码。
        encoding: OpenAiEncoding,
    },
    /// Lightweight budget proxy backed by an OpenAI tokenizer and a safety multiplier.
    /// 由 OpenAI tokenizer 与安全倍率承载的轻量预算代理。
    BudgetProxy {
        /// Semantic proxy family used for diagnostics and tuning.
        /// 用于诊断与调优的语义代理族。
        family: ProxyTokenizerFamily,
        /// OpenAI encoding used as the proxy base.
        /// 作为代理基准的 OpenAI 编码。
        proxy_encoding: OpenAiEncoding,
        /// Safety multiplier applied with ceiling rounding.
        /// 使用向上取整应用的安全倍率。
        multiplier: f64,
    },
    /// Local `.tkz` tokenizer with explicit precision metadata and an OpenAI proxy fallback.
    /// 带显式精度元数据与 OpenAI 代理回退的本地 `.tkz` tokenizer。
    ExactTokie {
        /// Local tokenizer asset selected for this model family.
        /// 为该模型族选择的本地 tokenizer 资产。
        key: ExactTokenizerKey,
        /// Precision to report when the local `.tkz` tokenizer is available.
        /// 本地 `.tkz` tokenizer 可用时需要报告的精度。
        local_precision: TokenEstimatePrecision,
        /// Multiplier applied to the local `.tkz` tokenizer count.
        /// 应用于本地 `.tkz` tokenizer 统计结果的倍率。
        local_multiplier: f64,
        /// Semantic proxy family used when the exact asset is missing or invalid.
        /// 精确资产缺失或无效时使用的语义代理族。
        fallback_family: ProxyTokenizerFamily,
        /// OpenAI encoding used as the fallback proxy base.
        /// 作为回退代理基准的 OpenAI 编码。
        fallback_encoding: OpenAiEncoding,
        /// Safety multiplier used by the fallback proxy.
        /// 回退代理使用的安全倍率。
        fallback_multiplier: f64,
    },
}

/// Precision class attached to one token estimate.
/// 附加到单次 Token 估算上的精度类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenEstimatePrecision {
    /// Count is exact for the selected OpenAI tokenizer.
    /// 统计结果对所选 OpenAI tokenizer 精确。
    Exact,
    /// Count comes from an experimental backend that may not match provider billing exactly.
    /// 统计结果来自实验后端，可能无法与供应商账单完全一致。
    Experimental,
    /// Count is a conservative budget proxy.
    /// 统计结果是保守预算代理。
    Proxy,
}

/// Detailed token estimate returned by the counter.
/// 统计器返回的详细 Token 估算结果。
#[derive(Debug, Clone, PartialEq)]
pub struct TokenEstimate {
    /// Final token count after exact counting or proxy multiplier.
    /// 精确统计或代理倍率处理后的最终 Token 数。
    pub tokens: usize,
    /// Tokenizer tier selected from the model name.
    /// 根据模型名称选择出的 tokenizer 层级。
    pub routed_tier: TokenizerTier,
    /// Precision class of the returned count.
    /// 返回数量的精度类别。
    pub precision: TokenEstimatePrecision,
}

/// One exact tokenizer asset bundled with the crate.
/// 随 crate 内置的单个精确 tokenizer 资产。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExactTokenizerAsset {
    /// Stable tokenizer key used by routing and diagnostics.
    /// 路由与诊断使用的稳定 tokenizer key。
    pub key: ExactTokenizerKey,
    /// Relative asset path below `resources/tokenizers`.
    /// 位于 `resources/tokenizers` 下的相对资产路径。
    pub relative_path: &'static str,
}

/// Exact tokenizer assets supported by this crate when their `.tkz` files are bundled.
/// 本 crate 在对应 `.tkz` 文件内置时支持的精确 tokenizer 资产。
pub const EXACT_TOKENIZER_ASSETS: &[ExactTokenizerAsset] = &[
    ExactTokenizerAsset {
        key: ExactTokenizerKey::Qwen,
        relative_path: "exact/qwen/tokenizer.tkz",
    },
    ExactTokenizerAsset {
        key: ExactTokenizerKey::QwenCoder,
        relative_path: "exact/qwen_coder/tokenizer.tkz",
    },
    ExactTokenizerAsset {
        key: ExactTokenizerKey::DeepSeek,
        relative_path: "exact/deepseek/tokenizer.tkz",
    },
    ExactTokenizerAsset {
        key: ExactTokenizerKey::DeepSeekR1,
        relative_path: "exact/deepseek_r1/tokenizer.tkz",
    },
    ExactTokenizerAsset {
        key: ExactTokenizerKey::MiniMax,
        relative_path: "exact/minimax/tokenizer.tkz",
    },
    ExactTokenizerAsset {
        key: ExactTokenizerKey::Glm,
        relative_path: "exact/glm/tokenizer.tkz",
    },
    ExactTokenizerAsset {
        key: ExactTokenizerKey::Hunyuan,
        relative_path: "exact/hunyuan/tokenizer.tkz",
    },
    ExactTokenizerAsset {
        key: ExactTokenizerKey::Ernie,
        relative_path: "exact/ernie/tokenizer.tkz",
    },
    ExactTokenizerAsset {
        key: ExactTokenizerKey::Llama,
        relative_path: "exact/llama/tokenizer.tkz",
    },
    ExactTokenizerAsset {
        key: ExactTokenizerKey::Mistral,
        relative_path: "exact/mistral/tokenizer.tkz",
    },
    ExactTokenizerAsset {
        key: ExactTokenizerKey::Phi,
        relative_path: "exact/phi/tokenizer.tkz",
    },
    ExactTokenizerAsset {
        key: ExactTokenizerKey::Gemma,
        relative_path: "exact/gemma/tokenizer.tkz",
    },
    ExactTokenizerAsset {
        key: ExactTokenizerKey::Nemotron,
        relative_path: "exact/nemotron/tokenizer.tkz",
    },
];

/// Finds the bundled exact tokenizer asset for one key.
/// 查找单个 key 对应的内置精确 tokenizer 资产。
///
/// # Parameters
/// 参数
/// `key` selects the exact tokenizer asset to locate.
/// `key` 选择需要定位的精确 tokenizer 资产。
///
/// # Returns
/// 返回值
/// The shared asset descriptor when the key is represented by the bundled asset list.
/// 当该 key 已由内置资产清单表示时返回共享资产描述。
pub(crate) fn exact_tokenizer_asset(
    key: ExactTokenizerKey,
) -> Option<&'static ExactTokenizerAsset> {
    EXACT_TOKENIZER_ASSETS.iter().find(|asset| asset.key == key)
}

/// Errors returned by token counter construction and counting.
/// Token 统计器构造与统计过程中返回的错误。
#[derive(Debug, Error)]
pub enum TokenCounterError {
    /// A required OpenAI tokenizer resource is missing.
    /// 必需的 OpenAI tokenizer 资源缺失。
    #[error("missing OpenAI tokenizer resource {encoding} for backend {backend} at {path}")]
    MissingOpenAiResource {
        /// Backend whose local resource was required.
        /// 需要本地资源的后端。
        backend: TokenizerBackend,
        /// Encoding whose local resource was required.
        /// 需要本地资源的编码。
        encoding: OpenAiEncoding,
        /// Resource path that was expected to exist.
        /// 预期存在的资源路径。
        path: PathBuf,
    },
    /// An OpenAI tokenizer backend could not be loaded or executed.
    /// 无法加载或执行 OpenAI tokenizer 后端。
    #[error("failed to use OpenAI tokenizer {encoding} with backend {backend}: {message}")]
    NativeOpenAi {
        /// Backend selected for loading or counting.
        /// 加载或统计时选择的后端。
        backend: TokenizerBackend,
        /// Encoding selected for loading or counting.
        /// 加载或统计时选择的编码。
        encoding: OpenAiEncoding,
        /// Human-readable error message from the underlying tokenizer.
        /// 底层 tokenizer 返回的可读错误信息。
        message: String,
    },
}
