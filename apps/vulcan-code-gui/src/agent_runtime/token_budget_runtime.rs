//! Shared token-budget runtime initialized during host startup.
//! 在宿主启动期间初始化的共享 Token 预算运行时。

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use vulcan_context::budget::ContextTokenEstimator;
use vulcan_token_counter::{TokenCounter, TokenizerBackend};

/// Environment variable that explicitly opts into the legacy `.tiktoken` backend.
/// 显式启用旧版 `.tiktoken` 后端的环境变量。
const ENABLE_TIKTOKEN_ENV: &str = "VULCAN_TOKEN_BUDGET_ENABLE_TIKTOKEN";

/// Runtime-owned token budget calculator and its initialization diagnostics.
/// 运行时持有的 Token 预算计算器及其初始化诊断信息。
pub struct TokenBudgetRuntime {
    /// Tokenizer resource directory used to initialize the counter.
    /// 用于初始化统计器的 tokenizer 资源目录。
    resource_directory: PathBuf,
    /// Shared model-aware token counter available after successful initialization.
    /// 初始化成功后可用的共享模型感知 Token 统计器。
    counter: Option<Arc<TokenCounter>>,
    /// Selected OpenAI-base tokenizer backend used by this runtime.
    /// 该运行时使用的 OpenAI 基础 tokenizer 后端。
    backend: TokenizerBackend,
    /// Current initialization state retained for diagnostics and UI summaries.
    /// 为诊断与界面摘要保留的当前初始化状态。
    state: TokenBudgetRuntimeState,
}

/// Initialization state for the shared token budget runtime.
/// 共享 Token 预算运行时的初始化状态。
#[derive(Debug, Clone, PartialEq, Eq)]
enum TokenBudgetRuntimeState {
    /// Tokenizer loading has not finished yet.
    /// tokenizer 加载尚未完成。
    Pending,
    /// Tokenizer loading finished successfully.
    /// tokenizer 加载已成功完成。
    Ready,
    /// Tokenizer loading failed with a diagnostic message.
    /// tokenizer 加载失败并携带诊断信息。
    Failed(String),
}

impl fmt::Debug for TokenBudgetRuntime {
    /// Formats only lightweight diagnostics without expanding the tokenizer internals.
    /// 只格式化轻量诊断信息，避免展开 tokenizer 内部结构。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TokenBudgetRuntime")
            .field("resource_directory", &self.resource_directory)
            .field("backend", &self.backend)
            .field("is_available", &self.is_available())
            .field("state", &self.state)
            .finish()
    }
}

impl TokenBudgetRuntime {
    /// Builds a lightweight pending runtime without loading tokenizer assets.
    /// 构建不加载 tokenizer 资产的轻量待初始化运行时。
    ///
    /// # Returns
    /// 返回值
    /// A pending runtime wrapper that lets the UI reach the first frame before warm-up starts.
    /// 一个待初始化运行时包装对象，使界面可在 warm-up 前进入首帧。
    pub fn pending() -> Self {
        Self {
            resource_directory: vulcan_runtime_assets::resolve_tokenizer_resource_directory(),
            counter: None,
            backend: Self::resolve_default_backend(),
            state: TokenBudgetRuntimeState::Pending,
        }
    }

    /// Initializes the shared token budget runtime from the active runtime-assets directory.
    /// 从当前 runtime-assets 目录初始化共享 Token 预算运行时。
    ///
    /// # Returns
    /// 返回值
    /// A runtime wrapper that either owns a warmed counter or records the initialization failure.
    /// 一个持有已预热统计器或记录初始化失败原因的运行时包装对象。
    pub fn initialize() -> Self {
        Self::initialize_from_resource_directory(
            vulcan_runtime_assets::resolve_tokenizer_resource_directory(),
        )
    }

    /// Initializes the shared token budget runtime on a blocking worker thread.
    /// 在阻塞工作线程上初始化共享 Token 预算运行时。
    ///
    /// # Returns
    /// 返回值
    /// A shared runtime wrapper containing either the warmed counter or a retained failure state.
    /// 一个共享运行时包装对象，包含已预热统计器或保留的失败状态。
    pub async fn initialize_on_background_thread() -> Arc<Self> {
        match tokio::task::spawn_blocking(Self::initialize).await {
            Ok(runtime) => Arc::new(runtime),
            Err(error) => Arc::new(Self::failed_with_error(
                vulcan_runtime_assets::resolve_tokenizer_resource_directory(),
                Self::resolve_default_backend(),
                format!("Token 预算初始化任务异常结束：{error}"),
            )),
        }
    }

    /// Initializes the shared token budget runtime from one explicit resource directory.
    /// 从一个显式资源目录初始化共享 Token 预算运行时。
    ///
    /// # Parameters
    /// 参数
    /// `resource_directory` points at the directory containing `openai` and optional `exact` tokenizer assets.
    /// `resource_directory` 指向包含 `openai` 与可选 `exact` tokenizer 资产的目录。
    ///
    /// # Returns
    /// 返回值
    /// A runtime wrapper with diagnostics preserved even when initialization fails.
    /// 一个即使初始化失败也保留诊断信息的运行时包装对象。
    pub fn initialize_from_resource_directory(resource_directory: impl Into<PathBuf>) -> Self {
        Self::initialize_from_resource_directory_with_backend(
            resource_directory,
            Self::resolve_default_backend(),
        )
    }

    /// Initializes the shared token budget runtime from one explicit resource directory and backend.
    /// 从一个显式资源目录与后端初始化共享 Token 预算运行时。
    ///
    /// # Parameters
    /// 参数
    /// `resource_directory` points at the directory containing tokenizer assets.
    /// `resource_directory` 指向包含 tokenizer 资产的目录。
    /// `backend` selects whether OpenAI base encodings use `.tkz` or `.tiktoken`.
    /// `backend` 选择 OpenAI 基础编码使用 `.tkz` 还是 `.tiktoken`。
    ///
    /// # Returns
    /// 返回值
    /// A runtime wrapper with diagnostics preserved even when initialization fails.
    /// 一个即使初始化失败也保留诊断信息的运行时包装对象。
    pub fn initialize_from_resource_directory_with_backend(
        resource_directory: impl Into<PathBuf>,
        backend: TokenizerBackend,
    ) -> Self {
        let resource_directory = resource_directory.into();
        match TokenCounter::from_assets_dir_with_backend(&resource_directory, backend).and_then(
            |counter| {
                counter.warm_up()?;
                Ok(counter)
            },
        ) {
            Ok(counter) => Self {
                resource_directory,
                counter: Some(Arc::new(counter)),
                backend,
                state: TokenBudgetRuntimeState::Ready,
            },
            Err(error) => Self {
                resource_directory: resource_directory.clone(),
                counter: None,
                backend,
                state: TokenBudgetRuntimeState::Failed(error.to_string()),
            },
        }
    }

    /// Builds a failed runtime wrapper from one explicit diagnostic message.
    /// 根据一条显式诊断信息构建失败状态的运行时包装对象。
    ///
    /// # Parameters
    /// 参数
    /// `resource_directory` points at the tokenizer resource directory attempted by initialization.
    /// `resource_directory` 指向初始化尝试使用的 tokenizer 资源目录。
    /// `error` is the human-readable failure diagnostic.
    /// `error` 是面向人的失败诊断信息。
    ///
    /// # Returns
    /// 返回值
    /// A runtime wrapper that preserves the failure and keeps estimates on the fallback path.
    /// 一个保留失败信息并让估算保持回退路径的运行时包装对象。
    fn failed_with_error(
        resource_directory: impl Into<PathBuf>,
        backend: TokenizerBackend,
        error: impl Into<String>,
    ) -> Self {
        Self {
            resource_directory: resource_directory.into(),
            counter: None,
            backend,
            state: TokenBudgetRuntimeState::Failed(error.into()),
        }
    }

    /// Returns whether the token budget counter is available for runtime estimates.
    /// 返回 Token 预算统计器是否可用于运行时估算。
    ///
    /// # Returns
    /// 返回值
    /// `true` when initialization succeeded and the warmed counter is retained.
    /// 当初始化成功且已保留预热后的统计器时返回 `true`。
    pub fn is_available(&self) -> bool {
        self.counter.is_some()
    }

    /// Returns the tokenizer resource directory used by this runtime.
    /// 返回该运行时使用的 tokenizer 资源目录。
    ///
    /// # Returns
    /// 返回值
    /// The resource directory path captured during initialization.
    /// 初始化期间捕获的资源目录路径。
    pub fn resource_directory(&self) -> &Path {
        &self.resource_directory
    }

    /// Returns the selected tokenizer backend for diagnostics.
    /// 返回用于诊断的当前 tokenizer 后端。
    ///
    /// # Returns
    /// 返回值
    /// The backend selected for OpenAI base encodings.
    /// OpenAI 基础编码选择的后端。
    pub const fn backend(&self) -> TokenizerBackend {
        self.backend
    }

    /// Returns this runtime as a context token estimator when initialization succeeded.
    /// 当初始化成功时将该运行时作为上下文 Token 估算器返回。
    ///
    /// # Returns
    /// 返回值
    /// A trait-object estimator reference, or `None` when initialization failed.
    /// trait-object 估算器引用；初始化失败时返回 `None`。
    pub fn estimator(&self) -> Option<&dyn ContextTokenEstimator> {
        self.is_available()
            .then_some(self as &dyn ContextTokenEstimator)
    }

    /// Builds one compact diagnostic row for runtime summaries.
    /// 为运行时摘要构建一条紧凑诊断行。
    ///
    /// # Returns
    /// 返回值
    /// A human-readable status string describing the active token budget runtime.
    /// 描述当前 Token 预算运行时状态的人类可读状态字符串。
    pub fn status_row(&self) -> String {
        match &self.state {
            TokenBudgetRuntimeState::Pending => format!(
                "Token 预算计算器正在初始化：{}（后端：{}）",
                self.resource_directory().display(),
                self.backend().label()
            ),
            TokenBudgetRuntimeState::Ready => format!(
                "Token 预算计算器已初始化：{}（后端：{}）",
                self.resource_directory().display(),
                self.backend().label()
            ),
            TokenBudgetRuntimeState::Failed(error) => format!(
                "Token 预算计算器不可用：{}（资源目录：{}，后端：{}）",
                error,
                self.resource_directory().display(),
                self.backend().label()
            ),
        }
    }

    /// Resolves the default tokenizer backend from the temporary explicit opt-in switch.
    /// 根据临时显式开关解析默认 tokenizer 后端。
    ///
    /// # Returns
    /// 返回值
    /// `TokenizerBackend::Tokie` unless `VULCAN_TOKEN_BUDGET_ENABLE_TIKTOKEN` is truthy.
    /// 除非 `VULCAN_TOKEN_BUDGET_ENABLE_TIKTOKEN` 为真值，否则返回 `TokenizerBackend::Tokie`。
    fn resolve_default_backend() -> TokenizerBackend {
        let enable_tiktoken = std::env::var(ENABLE_TIKTOKEN_ENV)
            .ok()
            .map(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false);
        if enable_tiktoken {
            TokenizerBackend::Wordchipper
        } else {
            TokenizerBackend::Tokie
        }
    }
}

impl ContextTokenEstimator for TokenBudgetRuntime {
    /// Estimates one text payload with the warmed model-aware token counter.
    /// 使用已预热的模型感知 Token 统计器估算单段文本。
    ///
    /// # Parameters
    /// 参数
    /// `model_id` is the provider model identifier used for tokenizer routing.
    /// `model_id` 是用于 tokenizer 路由的 Provider 模型标识。
    /// `text` is the provider-visible text payload to estimate.
    /// `text` 是需要估算的 Provider 可见文本载荷。
    ///
    /// # Returns
    /// 返回值
    /// The estimated token count, or `None` when the counter is unavailable or fails.
    /// 估算得到的 Token 数；当统计器不可用或统计失败时返回 `None`。
    fn estimate_text_tokens(&self, model_id: &str, text: &str) -> Option<usize> {
        self.counter
            .as_ref()
            .and_then(|counter| counter.estimate_tokens_responsive(model_id, text).ok())
    }
}
