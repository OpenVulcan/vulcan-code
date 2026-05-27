//! Provider adapter traits owned by `vulcan-core`.
//! `vulcan-core` 自有的 Provider 适配器 trait。

use crate::types::{ControllerConfig, ConversationSnapshot, CoreError, ProviderTurnDelta};
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

    /// Validates one freshly built request before the adapter sends it to the provider.
    /// 在适配器向 Provider 发送请求前校验刚刚构建完成的请求。
    fn validate_built_request(&self, _request: &Self::Request) -> Result<(), CoreError> {
        Ok(())
    }
}

/// Receives normalized provider-stream deltas while one request is still in flight.
/// 在单次请求仍在执行期间接收规范化 Provider 流式增量。
pub trait VulcanProviderDeltaSink: Send {
    /// Pushes one provider delta into the current sink implementation.
    /// 向当前 sink 实现推送一条 Provider 增量。
    fn push_delta(&mut self, delta: ProviderTurnDelta);
}

/// Describes one provider adapter that can emit normalized stream deltas before returning the final turn response.
/// 表示一个能够在返回最终回合响应前发出规范化流式增量的 Provider 适配器。
#[async_trait]
pub trait VulcanStreamingProviderAdapter:
    VulcanProviderAdapter<Response = crate::types::ProviderTurnResponse>
{
    /// Sends one provider request, emits streaming deltas into the supplied sink, and then returns the normalized final turn response.
    /// 发送一份 Provider 请求，将流式增量写入给定 sink，随后返回规范化的最终回合响应。
    async fn execute_request_streaming<S>(
        &self,
        request: Self::Request,
        sink: &mut S,
    ) -> Result<Self::Response, CoreError>
    where
        S: VulcanProviderDeltaSink;
}
