//! Token budget runtime bridge used by the Core Service runtime.
//! Core Service 运行时使用的 Token 预算运行时桥接层。

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use vulcan_token_counter::{TokenCounter, TokenizerBackend};

/// Environment variable that explicitly opts into the legacy `.tiktoken` backend.
/// 显式启用旧版 `.tiktoken` 后端的环境变量。
const ENABLE_TIKTOKEN_ENV: &str = "VULCAN_TOKEN_BUDGET_ENABLE_TIKTOKEN";

/// Runtime-owned token budget calculator and diagnostics.
/// 运行时持有的 Token 预算计算器与诊断信息。
#[derive(Debug)]
pub struct TauriTokenBudgetRuntime {
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
    state: TauriTokenBudgetRuntimeState,
}

/// Initialization state for the shared token budget runtime.
/// 共享 Token 预算运行时的初始化状态。
#[derive(Debug, Clone, PartialEq, Eq)]
enum TauriTokenBudgetRuntimeState {
    /// Tokenizer loading finished successfully.
    /// tokenizer 加载已成功完成。
    Ready,
    /// Tokenizer loading failed with a diagnostic message.
    /// tokenizer 加载失败并携带诊断信息。
    Failed(String),
}

impl TauriTokenBudgetRuntime {
    /// Initializes the token budget runtime from bundled tokenizer resources.
    /// 从内置 tokenizer 资源初始化 Token 预算运行时。
    pub fn initialize() -> Arc<Self> {
        Arc::new(Self::initialize_from_resource_directory(
            vulcan_runtime_assets::resolve_tokenizer_resource_directory(),
        ))
    }

    /// Initializes the token budget runtime from one explicit resource directory.
    /// 从一个显式资源目录初始化 Token 预算运行时。
    fn initialize_from_resource_directory(resource_directory: impl Into<PathBuf>) -> Self {
        let resource_directory = resource_directory.into();
        let backend = resolve_default_backend();
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
                state: TauriTokenBudgetRuntimeState::Ready,
            },
            Err(error) => Self {
                resource_directory,
                counter: None,
                backend,
                state: TauriTokenBudgetRuntimeState::Failed(error.to_string()),
            },
        }
    }

    /// Returns whether the token counter is available.
    /// 返回 Token 统计器是否可用。
    pub fn is_available(&self) -> bool {
        self.counter.is_some()
    }

    /// Returns the tokenizer resource directory used by this runtime.
    /// 返回该运行时使用的 tokenizer 资源目录。
    pub fn resource_directory(&self) -> &Path {
        &self.resource_directory
    }

    /// Returns the selected tokenizer backend.
    /// 返回当前选择的 tokenizer 后端。
    pub const fn backend(&self) -> TokenizerBackend {
        self.backend
    }

    /// Builds one compact diagnostic row for runtime summaries.
    /// 为运行时摘要构建一条紧凑诊断行。
    pub fn status_row(&self) -> String {
        match &self.state {
            TauriTokenBudgetRuntimeState::Ready => format!(
                "Token 预算计算器已初始化：{}（后端：{}）",
                self.resource_directory().display(),
                self.backend().label()
            ),
            TauriTokenBudgetRuntimeState::Failed(error) => format!(
                "Token 预算计算器不可用：{}（资源目录：{}，后端：{}）",
                error,
                self.resource_directory().display(),
                self.backend().label()
            ),
        }
    }
}

/// Resolves the default tokenizer backend from the explicit opt-in switch.
/// 根据显式开关解析默认 tokenizer 后端。
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
