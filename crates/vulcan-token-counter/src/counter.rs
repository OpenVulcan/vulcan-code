//! Stateful token counter implementation and public convenience constructors.
//! 有状态 Token 统计器实现与公开便捷构造函数。

use std::{
    collections::HashMap,
    fmt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

use tokie::Tokenizer as TokieTokenizer;

use crate::{
    resources::{
        LoadedOpenAiTokenizer, load_openai_tokenizer, safe_count_tokie_tokens,
        validate_openai_resource,
    },
    routing::{apply_multiplier, route_model, tier_openai_encoding},
    types::{
        EXACT_TOKENIZER_ASSETS, ExactTokenizerKey, OpenAiEncoding, TokenCounterError,
        TokenEstimate, TokenEstimatePrecision, TokenizerBackend, TokenizerTier,
        exact_tokenizer_asset,
    },
};

mod defaults;

pub use defaults::{
    default_openai_resource_dir, default_tokenizer_resource_dir, estimate, estimate_tokens,
};

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
            .and_then(|tokenizer| safe_count_tokie_tokens(&tokenizer, text).ok())
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
    /// The expected local tokenizer path from the shared asset descriptor.
    /// 来自共享资产描述的预期本地 tokenizer 路径。
    fn exact_tokenizer_path(&self, key: ExactTokenizerKey) -> PathBuf {
        let asset = exact_tokenizer_asset(key)
            .expect("every exact tokenizer key should be listed in EXACT_TOKENIZER_ASSETS");
        self.tokenizer_resource_root.join(asset.relative_path)
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
