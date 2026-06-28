//! Default token-counter resource paths and convenience functions.
//! 默认 Token 统计器资源路径与便捷函数。

use std::path::PathBuf;

use crate::{
    counter::TokenCounter,
    types::{TokenCounterError, TokenEstimate},
};

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

/// Returns the workspace runtime-assets tokenizer resource directory.
/// 返回 workspace runtime-assets tokenizer 资源目录。
///
/// # Returns
/// 返回值
/// The shared `crates/vulcan-runtime-assets/resources/tokenizers` path below the workspace root.
/// workspace 根目录下共享的 `crates/vulcan-runtime-assets/resources/tokenizers` 路径。
pub fn default_tokenizer_resource_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("vulcan-runtime-assets")
        .join("resources")
        .join("tokenizers")
}

/// Returns the default bundled OpenAI vocabulary resource directory.
/// 返回默认内置 OpenAI 词表资源目录。
///
/// # Returns
/// 返回值
/// The shared `resources/tokenizers/openai` directory below runtime-assets.
/// runtime-assets 下共享的 `resources/tokenizers/openai` 目录。
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
