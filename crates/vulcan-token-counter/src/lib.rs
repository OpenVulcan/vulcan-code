//! Standalone model-aware token budgeting primitives for Vulcan context control.
//! 面向 Vulcan 上下文控制的独立模型感知 Token 预算基础能力。

mod counter;
mod resources;
mod routing;
mod types;

pub use counter::{
    TokenCounter, default_openai_resource_dir, default_tokenizer_resource_dir, estimate,
    estimate_tokens,
};
pub use routing::route_model;
pub use types::{
    EXACT_TOKENIZER_ASSETS, ExactTokenizerAsset, ExactTokenizerKey, OpenAiEncoding,
    ProxyTokenizerFamily, TokenCounterError, TokenEstimate, TokenEstimatePrecision,
    TokenizerBackend, TokenizerTier,
};
