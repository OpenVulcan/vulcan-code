//! Strongly typed provider adapters and executable connection graphs.
//! 强类型供应商适配器与可执行连接图。

// Keep the source-level compatibility name while all protocol symbols are re-exported by the LLM core.
// 在所有协议符号由 LLM 核心重新导出期间保留源码兼容名称。
extern crate vulcan_llm_core as vulcan_ai_protocols;

mod alibaba_provider_tools;
pub mod client;
mod common_model_capabilities;
pub mod configuration;
mod connection_endpoint;
pub mod connection_graph;
pub mod credentials;
pub mod custom_model;
pub mod custom_provider;
pub mod discovery;
pub mod execution;
pub mod graph;
pub(crate) mod media;
mod native_inference;
pub mod oauth;
mod provider_task;
mod provider_tool_execution;
pub mod providers;
mod reasoning_catalog;
pub mod registry;
pub mod runtime;
pub mod tool_catalog;

pub use client::*;
pub use common_model_capabilities::*;
pub use configuration::*;
pub use connection_endpoint::normalize_connection_service_origin_override;
pub use connection_graph::*;
pub use credentials::*;
pub use custom_model::*;
pub use custom_provider::*;
pub use discovery::*;
pub use execution::*;
pub use graph::*;
pub use media::{MediaUploadCancellation, MediaUploadProgressCallback};
pub use native_inference::registered_native_action_binding_id;
pub use oauth::*;
pub use provider_task::{ProviderAsyncTaskPhase, ProviderAsyncTaskRecord};
pub use registry::*;
pub use runtime::*;
pub use tool_catalog::*;
