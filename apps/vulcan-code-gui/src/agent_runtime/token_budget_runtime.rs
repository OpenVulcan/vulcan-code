//! Shared token-budget runtime initialized during host startup.
//! 在宿主启动期间初始化的共享 Token 预算运行时。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use vulcan_context::budget::ContextTokenEstimator;
use vulcan_token_counter::TokenCounter;

/// Runtime-owned token budget calculator and its initialization diagnostics.
/// 运行时持有的 Token 预算计算器及其初始化诊断信息。
pub struct TokenBudgetRuntime {
    /// Tokenizer resource directory used to initialize the counter.
    /// 用于初始化统计器的 tokenizer 资源目录。
    resource_directory: PathBuf,
    /// Shared model-aware token counter available after successful initialization.
    /// 初始化成功后可用的共享模型感知 Token 统计器。
    counter: Option<Arc<TokenCounter>>,
    /// Human-readable initialization error retained for diagnostics.
    /// 为诊断保留的人类可读初始化错误。
    initialization_error: Option<String>,
}

impl TokenBudgetRuntime {
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
        let resource_directory = resource_directory.into();
        match TokenCounter::from_assets_dir(&resource_directory).and_then(|counter| {
            counter.warm_up()?;
            Ok(counter)
        }) {
            Ok(counter) => Self {
                resource_directory,
                counter: Some(Arc::new(counter)),
                initialization_error: None,
            },
            Err(error) => Self {
                resource_directory,
                counter: None,
                initialization_error: Some(error.to_string()),
            },
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

    /// Returns the initialization error when the counter is unavailable.
    /// 当统计器不可用时返回初始化错误。
    ///
    /// # Returns
    /// 返回值
    /// An optional diagnostic string describing the initialization failure.
    /// 描述初始化失败原因的可选诊断字符串。
    pub fn initialization_error(&self) -> Option<&str> {
        self.initialization_error.as_deref()
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
        match self.initialization_error() {
            Some(error) => format!(
                "Token 预算计算器不可用：{}（资源目录：{}）",
                error,
                self.resource_directory().display()
            ),
            None => format!(
                "Token 预算计算器已初始化：{}",
                self.resource_directory().display()
            ),
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
