//! Provider runtime terminal-error stream helper.
//! Provider 运行时终止错误流辅助模块。

use crate::agent_runtime::provider_stream::ProviderDeltaStream;
use async_stream::stream;

/// Builds a one-shot provider stream that yields one terminal error and then closes immediately.
/// 构建一条一次性 Provider 流，使其仅产出一条终止错误后立即关闭。
///
/// # Parameters
/// 参数
///
/// `error` is the exact user-facing terminal error emitted by the stream.
/// `error` 是该流发出的精确用户可见终止错误。
///
/// # Returns
/// 返回值
///
/// A provider delta stream containing exactly one error item.
/// 返回只包含一个错误项的 Provider 增量流。
pub(crate) fn one_shot_provider_error_stream(error: impl Into<String>) -> ProviderDeltaStream {
    let error = error.into();

    Box::pin(stream! {
        yield Err(error);
    })
}
