//! Shared live-delta transport primitives reused by host-controlled provider adapters and streaming runtime wrappers.
//! 供宿主控制 Provider 适配器与流式运行时包装器复用的共享实时增量传输原语。

use crate::agent_runtime::provider_stream::ProviderStreamDelta;
use tokio::sync::mpsc::UnboundedSender;

/// Shared live-delta sender used to forward request snapshots, streaming chunks, and runtime events from the host-owned controller task.
/// 用于从宿主自有控制器任务转发请求快照、流式分片与运行时事件的共享实时增量发送器。
pub(crate) type ProviderLiveDeltaSender = UnboundedSender<Result<ProviderStreamDelta, String>>;
