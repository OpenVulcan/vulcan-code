//! Shared streaming adapter support helpers reused by provider-specific transport adapters.
//! 供各 Provider 专用传输适配器复用的共享流式适配器支持辅助模块。

use crate::agent_runtime::provider_protocol_bridge::core_provider_turn_delta_to_runtime_delta;
use crate::agent_runtime::provider_transport_live_delta::ProviderLiveDeltaSender;
use futures_util::{Stream, StreamExt};
use std::fmt::Display;
use vulcan_core::provider::VulcanProviderDeltaSink;
use vulcan_core::types::{
    CoreError as VulcanCoreError, ProviderTurnDelta as CoreProviderTurnDelta,
};

/// Formats one provider label prefix so mixed English/Chinese runtime messages keep natural spacing automatically.
/// 格式化单个 Provider 标签前缀，使中英混排运行时文案能够自动保持自然空格。
fn format_provider_label_prefix(provider_label: &str) -> String {
    let needs_space = provider_label
        .chars()
        .last()
        .map(|character| character.is_ascii_alphanumeric())
        .unwrap_or(false);
    if needs_space {
        format!("{provider_label} ")
    } else {
        provider_label.to_string()
    }
}

/// Builds the standard invalid-state error returned when one streaming-only adapter is asked to run the non-streaming execution path.
/// 当仅支持流式的适配器被要求执行非流式路径时，构建其返回的标准无效状态错误。
pub(crate) fn unsupported_streaming_execute<T>(provider_label: &str) -> Result<T, VulcanCoreError> {
    let provider_label_prefix = format_provider_label_prefix(provider_label);
    Err(VulcanCoreError::InvalidState {
        message: format!(
            "{provider_label_prefix}流式适配器不支持非流式 execute_request 调用，请改用 execute_request_streaming。"
        ),
    })
}

/// Forwards one streaming provider content stream into both the `vulcan-core` delta sink and the GUI live-delta sender.
/// 将单个流式 Provider 内容流同时转发到 `vulcan-core` 增量汇聚器与 GUI 实时增量发送器。
pub(crate) async fn forward_streaming_provider_deltas<S, St, C, E, F>(
    provider_id: &str,
    provider_label: &str,
    live_sender: &ProviderLiveDeltaSender,
    stream: &mut St,
    sink: &mut S,
    mut convert_content: F,
) -> Result<(), VulcanCoreError>
where
    S: VulcanProviderDeltaSink,
    St: Stream<Item = Result<C, E>> + Unpin,
    E: Display,
    F: FnMut(C) -> Vec<CoreProviderTurnDelta>,
{
    let provider_label_prefix = format_provider_label_prefix(provider_label);
    while let Some(item) = stream.next().await {
        let content = item.map_err(|error| VulcanCoreError::ProviderFailure {
            provider_id: provider_id.to_string(),
            message: format!("{provider_label_prefix}流式执行失败：{error}"),
        })?;
        for delta in convert_content(content) {
            sink.push_delta(delta.clone());
            let _ = live_sender.send(Ok(core_provider_turn_delta_to_runtime_delta(delta)));
        }
    }

    Ok(())
}
