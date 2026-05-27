//! Standalone model-aware token budgeting primitives for Vulcan context control.
//! 面向 Vulcan 上下文控制的独立模型感知 Token 预算基础能力。

use std::{
    collections::HashMap,
    fmt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

use thiserror::Error;
use tokie::Tokenizer as TokieTokenizer;
use wordchipper::{
    TokenEncoder, Tokenizer as WordchipperTokenizer, TokenizerOptions, WCError, load_vocab,
    support::resources::{KeyedResource, ResourceLoader},
};

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

/// Loaded tokenizer implementation for one OpenAI base encoding.
/// 单个 OpenAI 基础编码已加载的 tokenizer 实现。
enum LoadedOpenAiTokenizer {
    /// `wordchipper` tokenizer loaded from a local `.tiktoken` resource.
    /// 从本地 `.tiktoken` 资源加载的 `wordchipper` tokenizer。
    Wordchipper(Arc<WordchipperTokenizer<u32>>),
    /// `tokie` tokenizer loaded from a local `.tkz` resource.
    /// 从本地 `.tkz` 资源加载的 `tokie` tokenizer。
    Tokie(Box<TokieTokenizer>),
}

impl LoadedOpenAiTokenizer {
    /// Counts plain text tokens with the loaded backend.
    /// 使用已加载后端统计纯文本 Token。
    ///
    /// # Parameters
    /// 参数
    /// `backend` identifies the active tokenizer backend for diagnostics.
    /// `backend` 标识用于诊断的当前 tokenizer 后端。
    /// `encoding` identifies the OpenAI base encoding for diagnostics.
    /// `encoding` 标识用于诊断的 OpenAI 基础编码。
    /// `text` is the plain text to count.
    /// `text` 是需要统计的纯文本。
    ///
    /// # Returns
    /// 返回值
    /// The backend token count or a normalized counter error.
    /// 后端 Token 数或规范化统计错误。
    fn count(
        &self,
        backend: TokenizerBackend,
        encoding: OpenAiEncoding,
        text: &str,
    ) -> Result<usize, TokenCounterError> {
        match self {
            Self::Wordchipper(tokenizer) => tokenizer
                .try_encode(text, None)
                .map(|tokens| tokens.len())
                .map_err(|error| TokenCounterError::NativeOpenAi {
                    backend,
                    encoding,
                    message: error.to_string(),
                }),
            Self::Tokie(tokenizer) => Ok(tokenizer.count_tokens(text)),
        }
    }
}

/// Fast token counter optimized for startup latency and conservative context budgeting.
/// 针对启动延迟与保守上下文预算优化的快速 Token 统计器。
pub struct TokenCounter {
    /// Backend used to load and execute OpenAI base encodings.
    /// 用于加载并执行 OpenAI 基础编码的后端。
    backend: TokenizerBackend,
    /// Directory containing all bundled tokenizer resources.
    /// 包含全部内置 tokenizer 资源的目录。
    tokenizer_resource_root: PathBuf,
    /// Directory containing bundled OpenAI tokenizer resources.
    /// 包含内置 OpenAI tokenizer 资源的目录。
    openai_resource_root: PathBuf,
    /// Lazily loaded exact `.tkz` tokenizers keyed by model family.
    /// 按模型族惰性加载的精确 `.tkz` tokenizer。
    exact_tokenizers: Mutex<HashMap<ExactTokenizerKey, Result<Arc<TokieTokenizer>, String>>>,
    /// Lazily loaded `o200k_base` tokenizer.
    /// 惰性加载的 `o200k_base` tokenizer。
    openai_o200k_tokenizer: OnceLock<Result<Arc<LoadedOpenAiTokenizer>, String>>,
    /// Lazily loaded `cl100k_base` tokenizer.
    /// 惰性加载的 `cl100k_base` tokenizer。
    openai_cl100k_tokenizer: OnceLock<Result<Arc<LoadedOpenAiTokenizer>, String>>,
}

impl fmt::Debug for TokenCounter {
    /// Formats one token counter without dumping tokenizer internals.
    /// 格式化单个 Token 统计器，同时避免输出 tokenizer 内部结构。
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
        formatter
            .debug_struct("TokenCounter")
            .field("backend", &self.backend)
            .field("tokenizer_resource_root", &self.tokenizer_resource_root)
            .field("openai_resource_root", &self.openai_resource_root)
            .field(
                "exact_loaded",
                &self
                    .exact_tokenizers
                    .lock()
                    .map(|tokenizers| tokenizers.len())
                    .unwrap_or_default(),
            )
            .field(
                "o200k_loaded",
                &self.is_encoding_loaded(OpenAiEncoding::O200kBase),
            )
            .field(
                "cl100k_loaded",
                &self.is_encoding_loaded(OpenAiEncoding::Cl100kBase),
            )
            .finish()
    }
}

impl TokenCounter {
    /// Creates a token counter using this crate's bundled OpenAI asset directory.
    /// 使用本 crate 自带 OpenAI 资产目录创建 Token 统计器。
    ///
    /// # Returns
    /// 返回值
    /// A lazy counter that does not parse tokenizer resources during construction.
    /// 一个构造时不解析 tokenizer 资源的惰性统计器。
    pub fn new() -> Result<Self, TokenCounterError> {
        Self::from_resources_dir(default_tokenizer_resource_dir())
    }

    /// Creates a token counter using this crate's bundled assets and one backend.
    /// 使用本 crate 自带资产与指定后端创建 Token 统计器。
    ///
    /// # Parameters
    /// 参数
    /// `backend` selects the tokenizer implementation and resource format.
    /// `backend` 选择 tokenizer 实现与资源格式。
    ///
    /// # Returns
    /// 返回值
    /// A lazy counter that validates only the selected backend resources.
    /// 一个仅校验所选后端资源的惰性统计器。
    pub fn new_with_backend(backend: TokenizerBackend) -> Result<Self, TokenCounterError> {
        Self::from_resources_dir_with_backend(default_tokenizer_resource_dir(), backend)
    }

    /// Creates a token counter using a tokenizer resource root.
    /// 使用 tokenizer 资源根目录创建 Token 统计器。
    ///
    /// # Parameters
    /// 参数
    /// `resource_root` points to a directory containing the `openai` tokenizer subdirectory.
    /// `resource_root` 指向包含 `openai` tokenizer 子目录的目录。
    ///
    /// # Returns
    /// 返回值
    /// A lazy counter after cheap resource-path validation.
    /// 一个完成低成本资源路径校验后的惰性统计器。
    pub fn from_resources_dir(
        resource_root: impl Into<PathBuf>,
    ) -> Result<Self, TokenCounterError> {
        Self::from_resources_dir_with_backend(resource_root, TokenizerBackend::Wordchipper)
    }

    /// Creates a token counter using a tokenizer resource root and explicit backend.
    /// 使用 tokenizer 资源根目录与显式后端创建 Token 统计器。
    ///
    /// # Parameters
    /// 参数
    /// `resource_root` points to a directory containing the `openai` tokenizer subdirectory.
    /// `resource_root` 指向包含 `openai` tokenizer 子目录的目录。
    /// `backend` selects whether `.tiktoken` or `.tkz` assets are required.
    /// `backend` 选择需要 `.tiktoken` 还是 `.tkz` 资产。
    ///
    /// # Returns
    /// 返回值
    /// A lazy counter after cheap selected-backend resource validation.
    /// 一个完成所选后端低成本资源校验后的惰性统计器。
    pub fn from_resources_dir_with_backend(
        resource_root: impl Into<PathBuf>,
        backend: TokenizerBackend,
    ) -> Result<Self, TokenCounterError> {
        let resource_root = resource_root.into();
        let openai_resource_root = resource_root.join("openai");

        validate_openai_resource(backend, OpenAiEncoding::O200kBase, &openai_resource_root)?;
        validate_openai_resource(backend, OpenAiEncoding::Cl100kBase, &openai_resource_root)?;

        Ok(Self {
            backend,
            tokenizer_resource_root: resource_root,
            openai_resource_root,
            exact_tokenizers: Mutex::new(HashMap::new()),
            openai_o200k_tokenizer: OnceLock::new(),
            openai_cl100k_tokenizer: OnceLock::new(),
        })
    }

    /// Creates a token counter using a tokenizer asset root.
    /// 使用 tokenizer 资产根目录创建 Token 统计器。
    ///
    /// # Parameters
    /// 参数
    /// `asset_root` points to a directory containing the `openai` tokenizer subdirectory.
    /// `asset_root` 指向包含 `openai` tokenizer 子目录的目录。
    ///
    /// # Returns
    /// 返回值
    /// A lazy counter using that asset root.
    /// 一个使用该资产根目录的惰性统计器。
    pub fn from_assets_dir(asset_root: impl Into<PathBuf>) -> Result<Self, TokenCounterError> {
        Self::from_resources_dir(asset_root)
    }

    /// Creates a token counter using a tokenizer asset root and explicit backend.
    /// 使用 tokenizer 资产根目录与显式后端创建 Token 统计器。
    ///
    /// # Parameters
    /// 参数
    /// `asset_root` points to a directory containing the `openai` tokenizer subdirectory.
    /// `asset_root` 指向包含 `openai` tokenizer 子目录的目录。
    /// `backend` selects the tokenizer implementation and resource extension.
    /// `backend` 选择 tokenizer 实现与资源扩展名。
    ///
    /// # Returns
    /// 返回值
    /// A lazy counter using that asset root and backend.
    /// 一个使用该资产根目录与后端的惰性统计器。
    pub fn from_assets_dir_with_backend(
        asset_root: impl Into<PathBuf>,
        backend: TokenizerBackend,
    ) -> Result<Self, TokenCounterError> {
        Self::from_resources_dir_with_backend(asset_root, backend)
    }

    /// Returns the tokenizer backend used by this counter.
    /// 返回该统计器使用的 tokenizer 后端。
    ///
    /// # Returns
    /// 返回值
    /// The selected backend for OpenAI base encodings.
    /// OpenAI 基础编码所选的后端。
    pub const fn backend(&self) -> TokenizerBackend {
        self.backend
    }

    /// Returns the tokenizer asset root used by this counter.
    /// 返回该统计器使用的 tokenizer 资产根目录。
    ///
    /// # Returns
    /// 返回值
    /// The parent directory that contains the `openai` tokenizer subdirectory.
    /// 包含 `openai` tokenizer 子目录的父目录。
    pub fn asset_root(&self) -> &Path {
        &self.tokenizer_resource_root
    }

    /// Returns the OpenAI resource directory used by this counter.
    /// 返回该统计器使用的 OpenAI 资源目录。
    ///
    /// # Returns
    /// 返回值
    /// The directory containing bundled `.tiktoken` files.
    /// 包含内置 `.tiktoken` 文件的目录。
    pub fn openai_resource_root(&self) -> &Path {
        &self.openai_resource_root
    }

    /// Returns the exact `.tkz` tokenizer resource directory used by this counter.
    /// 返回该统计器使用的精确 `.tkz` tokenizer 资源目录。
    ///
    /// # Returns
    /// 返回值
    /// The directory containing exact tokenizer subdirectories.
    /// 包含精确 tokenizer 子目录的目录。
    pub fn exact_resource_root(&self) -> PathBuf {
        self.tokenizer_resource_root.join("exact")
    }

    /// Returns the number of heavyweight external tokenizer assets used by this implementation.
    /// 返回该实现使用的重量级外部 tokenizer 资产数量。
    ///
    /// # Returns
    /// 返回值
    /// The number of bundled exact `.tkz` assets that currently exist on disk.
    /// 当前磁盘上实际存在的内置精确 `.tkz` 资产数量。
    pub fn external_tokenizer_count(&self) -> usize {
        EXACT_TOKENIZER_ASSETS
            .iter()
            .filter(|asset| {
                self.tokenizer_resource_root
                    .join(asset.relative_path)
                    .is_file()
            })
            .count()
    }

    /// Warms up both bundled OpenAI tokenizers explicitly.
    /// 显式预热两个内置 OpenAI tokenizer。
    ///
    /// # Returns
    /// 返回值
    /// `Ok(())` when both OpenAI tokenizers can be loaded.
    /// 当两个 OpenAI tokenizer 都能加载时返回 `Ok(())`。
    pub fn warm_up(&self) -> Result<(), TokenCounterError> {
        self.warm_up_encoding(OpenAiEncoding::O200kBase)?;
        self.warm_up_encoding(OpenAiEncoding::Cl100kBase)?;
        Ok(())
    }

    /// Warms up one OpenAI tokenizer explicitly.
    /// 显式预热单个 OpenAI tokenizer。
    ///
    /// # Parameters
    /// 参数
    /// `encoding` selects the OpenAI tokenizer to load.
    /// `encoding` 选择需要加载的 OpenAI tokenizer。
    ///
    /// # Returns
    /// 返回值
    /// `Ok(())` when the selected tokenizer can be loaded.
    /// 当所选 tokenizer 可以加载时返回 `Ok(())`。
    pub fn warm_up_encoding(&self, encoding: OpenAiEncoding) -> Result<(), TokenCounterError> {
        self.openai_tokenizer(encoding).map(|_| ())
    }

    /// Returns whether one OpenAI tokenizer has already been initialized.
    /// 返回单个 OpenAI tokenizer 是否已经初始化。
    ///
    /// # Parameters
    /// 参数
    /// `encoding` selects the tokenizer cache to inspect.
    /// `encoding` 选择需要检查的 tokenizer 缓存。
    ///
    /// # Returns
    /// 返回值
    /// `true` when the tokenizer cache has been initialized successfully or with a stored error.
    /// 当 tokenizer 缓存已成功初始化或已记录错误时返回 `true`。
    pub fn is_encoding_loaded(&self, encoding: OpenAiEncoding) -> bool {
        match encoding {
            OpenAiEncoding::O200kBase => self.openai_o200k_tokenizer.get().is_some(),
            OpenAiEncoding::Cl100kBase => self.openai_cl100k_tokenizer.get().is_some(),
        }
    }

    /// Preloads the tokenizer that one model route will use.
    /// 预加载单个模型路由将要使用的 tokenizer。
    ///
    /// # Parameters
    /// 参数
    /// `model_name` is routed before selecting the OpenAI base tokenizer.
    /// `model_name` 会先路由，再选择 OpenAI 基准 tokenizer。
    ///
    /// # Returns
    /// 返回值
    /// `true` when a tokenizer was loaded or was already available.
    /// 当 tokenizer 已加载或已经可用时返回 `true`。
    pub fn preload_model_tokenizer(&self, model_name: &str) -> Result<bool, TokenCounterError> {
        match route_model(model_name) {
            TokenizerTier::ExactTokie { key, .. } => {
                if self.exact_tokenizer(key).is_some() {
                    return Ok(true);
                }
                self.warm_up_encoding(tier_openai_encoding(&route_model(model_name)))?;
            }
            tier => {
                self.warm_up_encoding(tier_openai_encoding(&tier))?;
            }
        }
        Ok(true)
    }

    /// Compatibility no-op for previous exact-external-tokenizer preload flows.
    /// 兼容旧精确外部 tokenizer 预加载流程的空操作。
    ///
    /// # Returns
    /// 返回值
    /// Always `Ok(())`, because this implementation has no heavyweight external tokenizers.
    /// 始终返回 `Ok(())`，因为该实现没有重量级外部 tokenizer。
    pub fn preload_external_tokenizers(&self) -> Result<(), TokenCounterError> {
        for asset in EXACT_TOKENIZER_ASSETS {
            let _ = self.exact_tokenizer(asset.key);
        }
        Ok(())
    }

    /// Returns whether the tokenizer needed by one model is already loaded.
    /// 返回单个模型所需的 tokenizer 是否已加载。
    ///
    /// # Parameters
    /// 参数
    /// `model_name` is routed before checking the OpenAI base tokenizer.
    /// `model_name` 会先路由，再检查 OpenAI 基准 tokenizer。
    ///
    /// # Returns
    /// 返回值
    /// `true` when the selected OpenAI base tokenizer has already been initialized.
    /// 当所选 OpenAI 基准 tokenizer 已初始化时返回 `true`。
    pub fn is_model_tokenizer_loaded(&self, model_name: &str) -> bool {
        match route_model(model_name) {
            TokenizerTier::ExactTokie { key, .. } => self
                .exact_tokenizers
                .lock()
                .ok()
                .and_then(|tokenizers| {
                    tokenizers
                        .get(&key)
                        .map(|tokenizer_result| tokenizer_result.is_ok())
                })
                .unwrap_or(false),
            tier => self.is_encoding_loaded(tier_openai_encoding(&tier)),
        }
    }

    /// Returns whether the tokenizer cache needed by one model has been initialized.
    /// 返回单个模型所需的 tokenizer 缓存是否已经初始化。
    ///
    /// # Parameters
    /// 参数
    /// `model_name` is routed before checking the tokenizer cache.
    /// `model_name` 会先路由，再检查 tokenizer 缓存。
    ///
    /// # Returns
    /// 返回值
    /// `true` when the selected tokenizer cache contains either a successful load or a stored error.
    /// 当所选 tokenizer 缓存包含成功加载结果或已记录错误时返回 `true`。
    pub fn is_model_tokenizer_initialized(&self, model_name: &str) -> bool {
        match route_model(model_name) {
            TokenizerTier::ExactTokie { key, .. } => self
                .exact_tokenizers
                .lock()
                .map(|tokenizers| tokenizers.contains_key(&key))
                .unwrap_or(false),
            tier => self.is_encoding_loaded(tier_openai_encoding(&tier)),
        }
    }

    /// Counts text tokens for one model and returns only the final numeric estimate.
    /// 为单个模型统计文本 Token，并仅返回最终数字估算。
    ///
    /// # Parameters
    /// 参数
    /// `model_name` is routed into one exact or proxy tier.
    /// `model_name` 会被路由到一个精确或代理层级。
    /// `text` is the plain text to count.
    /// `text` 是需要统计的纯文本。
    ///
    /// # Returns
    /// 返回值
    /// The final token count after exact counting or proxy fallback.
    /// 精确统计或代理兜底后的最终 Token 数。
    pub fn estimate_tokens(
        &self,
        model_name: &str,
        text: &str,
    ) -> Result<usize, TokenCounterError> {
        self.estimate(model_name, text)
            .map(|estimate| estimate.tokens)
    }

    /// Counts text tokens without a separate heavyweight external-tokenizer path.
    /// 在没有独立重量级外部 tokenizer 路径的情况下统计文本 Token。
    ///
    /// # Parameters
    /// 参数
    /// `model_name` is routed into one exact or proxy tier.
    /// `model_name` 会被路由到一个精确或代理层级。
    /// `text` is the plain text to count.
    /// `text` 是需要统计的纯文本。
    ///
    /// # Returns
    /// 返回值
    /// The same result as `estimate_tokens`, kept for UI hot-path compatibility.
    /// 与 `estimate_tokens` 相同的结果，用于保持 UI 热路径兼容。
    pub fn estimate_tokens_responsive(
        &self,
        model_name: &str,
        text: &str,
    ) -> Result<usize, TokenCounterError> {
        self.estimate_tokens(model_name, text)
    }

    /// Counts text tokens for one model and returns detailed routing metadata.
    /// 为单个模型统计文本 Token，并返回详细路由元数据。
    ///
    /// # Parameters
    /// 参数
    /// `model_name` is routed into the exact or proxy tier.
    /// `model_name` 会被路由到精确或代理层。
    /// `text` is the plain text to count.
    /// `text` 是需要统计的纯文本。
    ///
    /// # Returns
    /// 返回值
    /// A detailed token estimate including count, tier, and precision class.
    /// 包含数量、层级与精度类别的详细 Token 估算结果。
    pub fn estimate(
        &self,
        model_name: &str,
        text: &str,
    ) -> Result<TokenEstimate, TokenCounterError> {
        let routed_tier = route_model(model_name);
        let (tokens, precision) = match routed_tier {
            TokenizerTier::NativeOpenAi { encoding } => {
                let precision = if self.backend == TokenizerBackend::Wordchipper {
                    TokenEstimatePrecision::Exact
                } else {
                    TokenEstimatePrecision::Experimental
                };
                (self.count_openai_tokens(encoding, text)?, precision)
            }
            TokenizerTier::BudgetProxy {
                proxy_encoding,
                multiplier,
                ..
            } => (
                self.count_proxy_tokens(proxy_encoding, multiplier, text)?,
                TokenEstimatePrecision::Proxy,
            ),
            TokenizerTier::ExactTokie {
                key,
                local_precision,
                local_multiplier,
                fallback_encoding,
                fallback_multiplier,
                ..
            } => {
                if let Some(tokens) = self.count_exact_tokie_tokens(key, text) {
                    (apply_multiplier(tokens, local_multiplier), local_precision)
                } else {
                    (
                        self.count_proxy_tokens(fallback_encoding, fallback_multiplier, text)?,
                        TokenEstimatePrecision::Proxy,
                    )
                }
            }
        };

        Ok(TokenEstimate {
            tokens,
            routed_tier,
            precision,
        })
    }

    /// Counts text tokens for one model and returns detailed routing metadata.
    /// 为单个模型统计文本 Token，并返回详细路由元数据。
    ///
    /// # Parameters
    /// 参数
    /// `model_name` is routed into the exact or proxy tier.
    /// `model_name` 会被路由到精确或代理层。
    /// `text` is the plain text to count.
    /// `text` 是需要统计的纯文本。
    ///
    /// # Returns
    /// 返回值
    /// The same result as `estimate`, kept for UI hot-path compatibility.
    /// 与 `estimate` 相同的结果，用于保持 UI 热路径兼容。
    pub fn estimate_responsive(
        &self,
        model_name: &str,
        text: &str,
    ) -> Result<TokenEstimate, TokenCounterError> {
        self.estimate(model_name, text)
    }

    /// Counts text with this counter's loaded OpenAI tokenizer.
    /// 使用当前统计器已加载的 OpenAI tokenizer 统计文本。
    ///
    /// # Parameters
    /// 参数
    /// `encoding` selects the OpenAI tokenizer vocabulary.
    /// `encoding` 选择 OpenAI tokenizer 词表。
    /// `text` is the plain text to encode.
    /// `text` 是需要编码的纯文本。
    ///
    /// # Returns
    /// 返回值
    /// The exact `wordchipper` token count for the selected encoding.
    /// 所选编码下的精确 `wordchipper` Token 数。
    fn count_openai_tokens(
        &self,
        encoding: OpenAiEncoding,
        text: &str,
    ) -> Result<usize, TokenCounterError> {
        self.openai_tokenizer(encoding)?
            .count(self.backend, encoding, text)
    }

    /// Counts text with one OpenAI proxy tokenizer and applies a safety multiplier.
    /// 使用单个 OpenAI 代理 tokenizer 统计文本并应用安全倍率。
    ///
    /// # Parameters
    /// 参数
    /// `proxy_encoding` selects the OpenAI tokenizer used as the proxy base.
    /// `proxy_encoding` 选择作为代理基准的 OpenAI tokenizer。
    /// `multiplier` is applied with ceiling rounding.
    /// `multiplier` 会以向上取整方式应用。
    /// `text` is the plain text to encode.
    /// `text` 是需要编码的纯文本。
    ///
    /// # Returns
    /// 返回值
    /// The safety-adjusted proxy token count.
    /// 安全调整后的代理 Token 数。
    fn count_proxy_tokens(
        &self,
        proxy_encoding: OpenAiEncoding,
        multiplier: f64,
        text: &str,
    ) -> Result<usize, TokenCounterError> {
        Ok(apply_multiplier(
            self.count_openai_tokens(proxy_encoding, text)?,
            multiplier,
        ))
    }

    /// Counts text with an exact `.tkz` tokenizer if its asset can be loaded.
    /// 在精确 `.tkz` tokenizer 资产可加载时使用它统计文本。
    ///
    /// # Parameters
    /// 参数
    /// `key` selects the exact tokenizer asset.
    /// `key` 选择精确 tokenizer 资产。
    /// `text` is the plain text to count.
    /// `text` 是需要统计的纯文本。
    ///
    /// # Returns
    /// 返回值
    /// `Some(tokens)` when the exact tokenizer is available, otherwise `None`.
    /// 精确 tokenizer 可用时返回 `Some(tokens)`，否则返回 `None`。
    fn count_exact_tokie_tokens(&self, key: ExactTokenizerKey, text: &str) -> Option<usize> {
        self.exact_tokenizer(key)
            .map(|tokenizer| tokenizer.count_tokens(text))
    }

    /// Returns one lazily initialized exact `.tkz` tokenizer.
    /// 返回单个惰性初始化的精确 `.tkz` tokenizer。
    ///
    /// # Parameters
    /// 参数
    /// `key` selects the exact tokenizer asset.
    /// `key` 选择精确 tokenizer 资产。
    ///
    /// # Returns
    /// 返回值
    /// A shared tokenizer instance when the asset exists and loads successfully.
    /// 当资产存在且加载成功时返回共享 tokenizer 实例。
    fn exact_tokenizer(&self, key: ExactTokenizerKey) -> Option<Arc<TokieTokenizer>> {
        let mut tokenizers = self.exact_tokenizers.lock().ok()?;
        tokenizers.entry(key).or_insert_with(|| {
            let path = self.exact_tokenizer_path(key);
            if path.is_file() {
                TokieTokenizer::from_file(&path)
                    .map(Arc::new)
                    .map_err(|error| error.to_string())
            } else {
                Err(format!(
                    "missing exact tokenizer resource: {}",
                    path.display()
                ))
            }
        });

        tokenizers
            .get(&key)
            .and_then(|result| result.as_ref().ok().map(Arc::clone))
    }

    /// Returns the expected path for one exact `.tkz` tokenizer.
    /// 返回单个精确 `.tkz` tokenizer 的预期路径。
    ///
    /// # Parameters
    /// 参数
    /// `key` selects the exact tokenizer asset.
    /// `key` 选择精确 tokenizer 资产。
    ///
    /// # Returns
    /// 返回值
    /// The expected local tokenizer path.
    /// 预期的本地 tokenizer 路径。
    fn exact_tokenizer_path(&self, key: ExactTokenizerKey) -> PathBuf {
        self.tokenizer_resource_root
            .join("exact")
            .join(key.resource_key())
            .join("tokenizer.tkz")
    }

    /// Returns one lazily initialized OpenAI tokenizer.
    /// 返回单个惰性初始化的 OpenAI tokenizer。
    ///
    /// # Parameters
    /// 参数
    /// `encoding` selects the tokenizer cache to use.
    /// `encoding` 选择需要使用的 tokenizer 缓存。
    ///
    /// # Returns
    /// 返回值
    /// A shared tokenizer instance or a stable loading error.
    /// 一个共享 tokenizer 实例，或稳定的加载错误。
    fn openai_tokenizer(
        &self,
        encoding: OpenAiEncoding,
    ) -> Result<Arc<LoadedOpenAiTokenizer>, TokenCounterError> {
        let cell = match encoding {
            OpenAiEncoding::O200kBase => &self.openai_o200k_tokenizer,
            OpenAiEncoding::Cl100kBase => &self.openai_cl100k_tokenizer,
        };
        let result = cell.get_or_init(|| {
            load_openai_tokenizer(self.backend, encoding, &self.openai_resource_root)
        });

        result
            .as_ref()
            .map(Arc::clone)
            .map_err(|message| TokenCounterError::NativeOpenAi {
                backend: self.backend,
                encoding,
                message: message.clone(),
            })
    }
}

impl Default for TokenCounter {
    /// Creates a default lazy counter and panics only when required resource files are missing.
    /// 创建默认惰性统计器；仅当必需资源文件缺失时 panic。
    ///
    /// # Returns
    /// 返回值
    /// A counter using this crate's default tokenizer asset directory.
    /// 一个使用本 crate 默认 tokenizer 资产目录的统计器。
    fn default() -> Self {
        Self::new().expect("default token counter resources should exist")
    }
}

/// Returns this crate's default tokenizer resource directory.
/// 返回本 crate 默认 tokenizer 资源目录。
///
/// # Returns
/// 返回值
/// The `resources/tokenizers` path below this crate's manifest directory.
/// 本 crate 清单目录下的 `resources/tokenizers` 路径。
pub fn default_tokenizer_resource_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("tokenizers")
}

/// Returns the default bundled OpenAI vocabulary resource directory.
/// 返回默认内置 OpenAI 词表资源目录。
///
/// # Returns
/// 返回值
/// The `resources/tokenizers/openai` path below this crate's manifest directory.
/// 本 crate 清单目录下的 `resources/tokenizers/openai` 路径。
pub fn default_openai_resource_dir() -> PathBuf {
    default_tokenizer_resource_dir().join("openai")
}

/// Estimates tokens using a default counter.
/// 使用默认统计器估算 Token。
///
/// # Parameters
/// 参数
/// `model_name` is routed into one exact or proxy tier.
/// `model_name` 会被路由到精确或代理层。
/// `text` is the plain text to count.
/// `text` 是需要统计的纯文本。
///
/// # Returns
/// 返回值
/// The final token count.
/// 最终 Token 数。
pub fn estimate_tokens(model_name: &str, text: &str) -> Result<usize, TokenCounterError> {
    TokenCounter::new()?.estimate_tokens(model_name, text)
}

/// Estimates tokens using a default counter and returns detailed metadata.
/// 使用默认统计器估算 Token 并返回详细元数据。
///
/// # Parameters
/// 参数
/// `model_name` is routed into one exact or proxy tier.
/// `model_name` 会被路由到精确或代理层。
/// `text` is the plain text to count.
/// `text` 是需要统计的纯文本。
///
/// # Returns
/// 返回值
/// A detailed token estimate.
/// 详细 Token 估算结果。
pub fn estimate(model_name: &str, text: &str) -> Result<TokenEstimate, TokenCounterError> {
    TokenCounter::new()?.estimate(model_name, text)
}

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
fn tier_openai_encoding(tier: &TokenizerTier) -> OpenAiEncoding {
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
fn apply_multiplier(tokens: usize, multiplier: f64) -> usize {
    ((tokens as f64) * multiplier).ceil() as usize
}

/// Validates that one OpenAI backend resource file exists.
/// 校验单个 OpenAI 后端资源文件存在。
///
/// # Parameters
/// 参数
/// `backend` selects the required resource extension.
/// `backend` 选择必需的资源扩展名。
/// `encoding` selects the OpenAI resource file.
/// `encoding` 选择 OpenAI 资源文件。
/// `resource_root` points to the directory containing tokenizer files.
/// `resource_root` 指向包含 tokenizer 文件的目录。
///
/// # Returns
/// 返回值
/// `Ok(())` when the resource file exists.
/// 当资源文件存在时返回 `Ok(())`。
fn validate_openai_resource(
    backend: TokenizerBackend,
    encoding: OpenAiEncoding,
    resource_root: &Path,
) -> Result<(), TokenCounterError> {
    let path = openai_resource_path(backend, encoding, resource_root);
    if path.is_file() {
        Ok(())
    } else {
        Err(TokenCounterError::MissingOpenAiResource {
            backend,
            encoding,
            path,
        })
    }
}

/// Builds the resource path for one backend and OpenAI encoding.
/// 为单个后端与 OpenAI 编码构建资源路径。
///
/// # Parameters
/// 参数
/// `backend` selects the resource extension.
/// `backend` 选择资源扩展名。
/// `encoding` selects the resource file stem.
/// `encoding` 选择资源文件名主体。
/// `resource_root` points to the directory containing tokenizer files.
/// `resource_root` 指向包含 tokenizer 文件的目录。
///
/// # Returns
/// 返回值
/// The expected local resource path.
/// 预期的本地资源路径。
fn openai_resource_path(
    backend: TokenizerBackend,
    encoding: OpenAiEncoding,
    resource_root: &Path,
) -> PathBuf {
    resource_root.join(format!(
        "{}.{}",
        encoding.file_stem(),
        backend.resource_extension()
    ))
}

/// Loads one OpenAI tokenizer through the selected backend.
/// 通过所选后端加载单个 OpenAI tokenizer。
///
/// # Parameters
/// 参数
/// `backend` selects the tokenizer implementation.
/// `backend` 选择 tokenizer 实现。
/// `encoding` selects the OpenAI vocabulary to load.
/// `encoding` 选择需要加载的 OpenAI 词表。
/// `resource_root` points to the directory containing local tokenizer files.
/// `resource_root` 指向包含本地 tokenizer 文件的目录。
///
/// # Returns
/// 返回值
/// A shared loaded tokenizer instance, or a stringified loading error.
/// 一个共享已加载 tokenizer 实例，或字符串化加载错误。
fn load_openai_tokenizer(
    backend: TokenizerBackend,
    encoding: OpenAiEncoding,
    resource_root: &Path,
) -> Result<Arc<LoadedOpenAiTokenizer>, String> {
    match backend {
        TokenizerBackend::Wordchipper => load_openai_wordchipper_tokenizer(encoding, resource_root),
        TokenizerBackend::Tokie => load_openai_tokie_tokenizer(encoding, resource_root),
    }
}

/// Loads one OpenAI tokenizer through `wordchipper` and configures it for shared counting.
/// 通过 `wordchipper` 加载单个 OpenAI tokenizer，并配置为共享统计使用。
///
/// # Parameters
/// 参数
/// `encoding` selects the OpenAI vocabulary to load.
/// `encoding` 选择需要加载的 OpenAI 词表。
/// `resource_root` points to the directory containing local `.tiktoken` files.
/// `resource_root` 指向包含本地 `.tiktoken` 文件的目录。
///
/// # Returns
/// 返回值
/// A shared tokenizer instance, or a stringified loading error.
/// 一个共享 tokenizer 实例，或字符串化加载错误。
fn load_openai_wordchipper_tokenizer(
    encoding: OpenAiEncoding,
    resource_root: &Path,
) -> Result<Arc<LoadedOpenAiTokenizer>, String> {
    let mut resource_loader = LocalOpenAiResourceLoader::new(resource_root.to_path_buf());
    let loaded = load_vocab(encoding.wordchipper_vocab_name(), &mut resource_loader)
        .map_err(|error| error.to_string())?;
    let tokenizer = TokenizerOptions::default().build(loaded.vocab().clone());

    Ok(Arc::new(LoadedOpenAiTokenizer::Wordchipper(tokenizer)))
}

/// Loads one OpenAI tokenizer through `tokie` from a local `.tkz` file.
/// 通过 `tokie` 从本地 `.tkz` 文件加载单个 OpenAI tokenizer。
///
/// # Parameters
/// 参数
/// `encoding` selects the OpenAI vocabulary to load.
/// `encoding` 选择需要加载的 OpenAI 词表。
/// `resource_root` points to the directory containing local `.tkz` files.
/// `resource_root` 指向包含本地 `.tkz` 文件的目录。
///
/// # Returns
/// 返回值
/// A shared tokenizer instance, or a stringified loading error.
/// 一个共享 tokenizer 实例，或字符串化加载错误。
fn load_openai_tokie_tokenizer(
    encoding: OpenAiEncoding,
    resource_root: &Path,
) -> Result<Arc<LoadedOpenAiTokenizer>, String> {
    let path = openai_resource_path(TokenizerBackend::Tokie, encoding, resource_root);
    let tokenizer = TokieTokenizer::from_file(&path).map_err(|error| error.to_string())?;

    Ok(Arc::new(LoadedOpenAiTokenizer::Tokie(Box::new(tokenizer))))
}

/// Local resource loader that keeps `wordchipper` OpenAI vocab loading fully offline.
/// 让 `wordchipper` OpenAI 词表加载保持完全离线的本地资源加载器。
struct LocalOpenAiResourceLoader {
    /// Directory that contains bundled OpenAI `.tiktoken` files.
    /// 包含内置 OpenAI `.tiktoken` 文件的目录。
    resource_root: PathBuf,
}

impl LocalOpenAiResourceLoader {
    /// Creates a local OpenAI resource loader.
    /// 创建本地 OpenAI 资源加载器。
    ///
    /// # Parameters
    /// 参数
    /// `resource_root` points to the directory containing `.tiktoken` files.
    /// `resource_root` 指向包含 `.tiktoken` 文件的目录。
    ///
    /// # Returns
    /// 返回值
    /// A loader that maps `wordchipper` keyed resources to local files.
    /// 一个将 `wordchipper` keyed resource 映射到本地文件的加载器。
    fn new(resource_root: PathBuf) -> Self {
        Self { resource_root }
    }
}

impl ResourceLoader for LocalOpenAiResourceLoader {
    /// Resolves one `wordchipper` resource key to a bundled OpenAI file path.
    /// 将单个 `wordchipper` 资源键解析到内置 OpenAI 文件路径。
    ///
    /// # Parameters
    /// 参数
    /// `resource` is the keyed resource requested by `wordchipper`.
    /// `resource` 是 `wordchipper` 请求的 keyed resource。
    ///
    /// # Returns
    /// 返回值
    /// A local path to the bundled tokenizer resource.
    /// 指向内置 tokenizer 资源的本地路径。
    fn load_resource_path(&mut self, resource: &KeyedResource) -> wordchipper::WCResult<PathBuf> {
        let resource_name = resource
            .key
            .last()
            .map(String::as_str)
            .unwrap_or("unknown-openai-tokenizer");
        let path = self.resource_root.join(format!("{resource_name}.tiktoken"));

        if path.is_file() {
            Ok(path)
        } else {
            Err(WCError::ResourceNotFound(format!(
                "missing local OpenAI tokenizer resource: {}",
                path.display()
            )))
        }
    }
}
