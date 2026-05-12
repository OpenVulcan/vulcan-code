//! Provider adapter traits owned by `vulcan-core`.
//! `vulcan-core` 自有的 Provider 适配器 trait。

use crate::types::{ControllerConfig, ConversationSnapshot, CoreError};
use async_trait::async_trait;
use serde::Serialize;
use std::fmt::Debug;

/// Describes one provider adapter that can build and send a request for the controller.
/// 表示一个可为控制器构建并发送请求的 Provider 适配器。
#[async_trait]
pub trait VulcanProviderAdapter: Send + Sync {
    /// Serializable provider request payload emitted by the adapter.
    /// 由适配器产出的可序列化 Provider 请求载荷。
    type Request: Send + Sync + Clone + Debug + Serialize;
    /// Serializable provider response payload returned by the adapter.
    /// 由适配器返回的可序列化 Provider 响应载荷。
    type Response: Send + Sync + Clone + Debug;

    /// Returns the stable provider identifier used by the adapter.
    /// 返回适配器使用的稳定 Provider 标识。
    fn provider_id(&self) -> &str;

    /// Returns the stable model identifier used by the adapter.
    /// 返回适配器使用的稳定模型标识。
    fn model_id(&self) -> &str;

    /// Builds one provider request from the canonical snapshot and controller configuration.
    /// 根据规范快照与控制器配置构建一份 Provider 请求。
    async fn build_request(
        &self,
        snapshot: &ConversationSnapshot,
        config: &ControllerConfig,
    ) -> Result<Self::Request, CoreError>;

    /// Sends one provider request and returns the raw provider response.
    /// 发送一份 Provider 请求并返回原始 Provider 响应。
    async fn execute_request(&self, request: Self::Request) -> Result<Self::Response, CoreError>;
}
