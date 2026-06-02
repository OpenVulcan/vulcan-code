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
        continuation_message_id: request_snapshot
            .continuation
            .last_provider_message_id
            .as_deref(),
    })
}
