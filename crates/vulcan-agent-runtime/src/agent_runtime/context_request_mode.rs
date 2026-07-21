//! Runtime adapter for shared context request send-mode diagnostics.
//! 共享上下文请求发送模式诊断的运行时适配层。

use crate::agent_runtime::execution_models::ExecutionPreparation;
use vulcan_context::{ContextRequestMode, ContextRequestModeInput, resolve_context_request_mode};
use vulcan_core::types::ProviderRequestSnapshot;

/// Resolves the shared context request mode from one prepared runtime snapshot.
/// 根据单个已准备运行时快照解析共享上下文请求模式。
pub(crate) fn resolve_preparation_context_request_mode(
    preparation: &ExecutionPreparation,
    request_snapshot: &ProviderRequestSnapshot,
) -> ContextRequestMode {
    // local_boundary_is_proven requires the persisted cursor boundary to occur exactly once in current history.
    // local_boundary_is_proven 要求持久化游标边界在当前历史中精确出现一次。
    let local_boundary_is_proven = request_snapshot
        .continuation
        .last_local_message_id
        .as_deref()
        .is_some_and(|boundary_id| {
            request_snapshot
                .history
                .iter()
                .filter(|message| message.id == boundary_id)
                .count()
                == 1
        });
    // continuation_message_id is exposed to incremental diagnostics only with an exact local boundary.
    // continuation_message_id 仅在具备精确本地边界时暴露给增量诊断。
    let continuation_message_id = if local_boundary_is_proven {
        request_snapshot
            .continuation
            .last_provider_message_id
            .as_deref()
    } else {
        None
    };
    resolve_context_request_mode(ContextRequestModeInput {
        internal_request: preparation.prompt_purpose.disables_provider_tools(),
        provider_id: &request_snapshot.provider_id,
        model_id: &request_snapshot.model_id,
        history_message_count: request_snapshot.history.len(),
        context_assembly: preparation.context_assembly.as_ref(),
        context_baseline: preparation.context_baseline.as_ref(),
        context_budget: preparation.context_budget.as_ref(),
        continuation_provider_id: request_snapshot.continuation.provider_id.as_deref(),
        continuation_model_id: request_snapshot.continuation.model_id.as_deref(),
        continuation_message_id,
    })
}
