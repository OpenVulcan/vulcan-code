//! Strongly typed provider adapters and executable connection graphs.
//! 强类型供应商适配器与可执行连接图。

pub mod client;
mod common_model_capabilities;
pub mod configuration;
mod connection_endpoint;
pub mod connection_graph;
pub mod credentials;
pub mod custom_model;
pub mod custom_provider;
pub mod discovery;
mod discovery_signature;
pub mod execution;
pub mod graph;
pub(crate) mod media;
pub mod media_delivery;
mod media_failure;
mod media_upload_deadline;
mod native_inference;
mod native_inference_batch;
pub mod oauth;
pub mod openvulcan;
mod operation_media_source;
pub mod provider_manifest;
pub mod provider_manifest_fixture;
mod provider_operational_telemetry;
mod provider_task;
mod provider_tool_execution;
pub mod registry;
pub mod runtime;
mod upload_engine;

pub use operation_media_source::{
    ProviderToolMediaSource, ProviderToolMediaSourceResolver, ProviderToolResourceContext,
};

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
pub use media::{
    MediaUploadCancellation, MediaUploadProgressCallback, RuntimeMediaUploadPhase,
    RuntimeMediaUploadProgressEvent, RuntimeMediaUploadProgressObserver,
};
pub use media_delivery::*;
pub use media_failure::{MediaUploadFailure, MediaUploadStage};
pub use media_upload_deadline::MediaUploadDeadline;
pub use native_inference::{
    registered_native_action_binding_id, registered_native_embedding_batch_capabilities,
};
pub use native_inference_batch::{EmbeddingBatchArtifactKind, EmbeddingBatchArtifactSink};
pub use oauth::*;
pub use provider_manifest::*;
pub use provider_operational_telemetry::*;
pub use provider_task::{ProviderAsyncTaskPhase, ProviderAsyncTaskRecord};
pub use registry::*;
pub use runtime::*;
pub use upload_engine::resolve_provider_tool_local_path;
// Re-export the shared context override DTO for provider-configuration persistence layers.
// 为供应商配置持久化层重新导出共享上下文覆盖 DTO。
pub use vulcan_context::ContextWindowPolicyOverrides;
// Re-export the exact model capability contract required by typed custom-model documents.
// 重新导出强类型自定义模型文档所需的精确模型能力契约。
pub use vulcan_ai_contract::ModelCapabilities;
