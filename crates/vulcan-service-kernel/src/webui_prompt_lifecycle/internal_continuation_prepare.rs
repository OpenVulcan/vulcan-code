//! Canonical Hook ownership for internal Goal and child-return dispatch.
//! 内部 Goal 与子回流分发的规范 Hook 归属。

use super::pending_prepare::{
    SessionSubagentReturnDispatchAuthority, subagent_return_dispatch_authority_from_document,
};
use super::*;
use crate::session_goal_lifecycle::{
    SessionGoalDispatchAuthority, session_goal_dispatch_authority_from_document,
};
use crate::session_runtime_registry_v1::SessionHandle;
use vulcan_session_core::v1::business_hook::{BusinessHookMetadata, InitialBusinessPreparation};
use vulcan_session_core::v1::document::{SessionDocument, TurnOrigin};

/// Mutually exclusive first capture and already-published business recovery.
/// 相互排斥的首次捕获与已发布业务恢复。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MainBusinessHookPreparation {
    /// Proven unexecuted owner; the enclosing source must still be validated independently.
    /// 已证明尚未执行的所有者；外层来源仍必须独立校验。
    Initial(InitialBusinessPreparation),
    /// Original identity and the exact reliable revision authorizing this attempt.
    /// 原身份及授权本次尝试的精确可靠修订。
    Published {
        /// Complete persisted frontier, including auxiliary changes.
        /// 包含辅助变化的完整持久化前沿。
        revision: u64,
        /// Immutable admission and canonical observations of the original business.
        /// 原业务的不可变准入与规范观察。
        hook: BusinessHookMetadata,
    },
}

impl MainBusinessHookPreparation {
    /// Captures an original or proven first owner from a full durable snapshot.
    /// 从完整可靠快照捕获原所有者或已证明的首次所有者。
    /// Callers validate the source in the same snapshot; missing prior effects are not inferred.
    /// 调用方在同一快照内校验来源；不得推测不存在先前效果。
    pub(super) fn capture(document: &SessionDocument) -> Result<Self, String> {
        // Binding equality chooses the branch; recovery errors must never become first preparation.
        // 绑定相等决定分支；恢复错误绝不能转为首次准备。
        let turn = document
            .active_turn()
            .filter(|turn| !turn.closed)
            .ok_or("Business Hook preparation requires an open turn.")?;
        if document
            .metadata
            .business_hook
            .binding
            .as_ref()
            .is_some_and(|binding| binding.turn_id == turn.turn_id)
        {
            Ok(Self::Published {
                revision: document.durable_revision(),
                hook: document
                    .main_business_recovery_state()
                    .map_err(|error| error.to_string())?,
            })
        } else {
            Ok(Self::Initial(
                document
                    .main_business_initial_preparation()
                    .map_err(|error| error.to_string())?,
            ))
        }
    }

    /// Installs a freshly validated source while its caller retains submission exclusion.
    /// 在调用方保持提交互斥期间安装刚刚校验过的来源。
    /// Storage validation must precede this runtime lock; returns errors without synthesizing identities.
    /// 存储校验必须先于此运行时锁；返回错误而不合成替代身份。
    pub(super) fn install(&self, runtime: &AgentRuntimeBridge) -> Result<(), String> {
        // No storage locks or asynchronous effects are held while updating the process-local Hook owner.
        // 更新进程内 Hook 所有者时不持有存储锁或异步效果。
        let mut hooks = runtime
            .conversation_hook_runtime
            .lock()
            .map_err(|error| format!("Failed to lock business Hook runtime: {error}"))?;
        match self {
            Self::Initial(source) => hooks.discard_unpublished_business_preparation(source),
            Self::Published { hook, .. } => hooks.restore_business_hook_state(hook).map(|_| ()),
        }
    }
}

/// Exact immutable source selected by the internal dispatcher.
/// 内部分发器选择的精确不可变来源。
pub(super) enum InternalContinuationHookSource<'a> {
    /// Active Goal identity and its canonical continuation anchor.
    /// 活动 Goal 身份及其规范续跑锚点。
    Goal(&'a SessionGoalDispatchAuthority),
    /// Exact published child result and its source receipt.
    /// 精确已发布子结果及其来源回执。
    SubagentReturn(&'a SessionSubagentReturnDispatchAuthority),
}

/// Revalidates source and Hook in one full durable read immediately before shared preparation.
/// 在共享准备之前立即通过一次完整可靠读取重新校验来源及 Hook。
/// The caller holds submission exclusion; first initialization is limited to the source's own turn.
/// 调用方持有提交互斥；首次初始化仅限于来源自己的回合。
pub(super) fn prepare_internal_continuation_hook_owner(
    handle: &SessionHandle,
    runtime: &AgentRuntimeBridge,
    source: InternalContinuationHookSource<'_>,
) -> Result<(), String> {
    // Capture both proofs together so a different generation or business cannot borrow the source.
    // 一起捕获两份凭据，防止不同代次或业务借用该来源。
    let preparation = handle
        .read_full_durable(|document| {
            // First preparation additionally requires the source to be the actual business starting event.
            // 首次准备额外要求来源就是实际业务起始事件。
            let (initial_origin, initial_seq) = match source {
                InternalContinuationHookSource::Goal(expected) => {
                    if session_goal_dispatch_authority_from_document(document, handle.session_id())
                        .as_ref()
                        != Some(expected)
                    {
                        return Err(String::from(
                            "Goal authority changed before Hook restoration.",
                        ));
                    }
                    (TurnOrigin::GoalContinuation, expected.context_seq)
                }
                InternalContinuationHookSource::SubagentReturn(expected) => {
                    // Exact fields are verified against the canonical event rather than parsed from its text.
                    // 对照规范事件校验精确字段，而不从正文解析。
                    let receipt = expected.receipt();
                    if subagent_return_dispatch_authority_from_document(document, &receipt)?
                        != *expected
                        || document.subagent_return_dispatch_started(&receipt)
                    {
                        return Err(String::from(
                            "Child return authority changed or was already dispatched.",
                        ));
                    }
                    (TurnOrigin::SubagentReturn, expected.return_seq)
                }
            };
            // A late return cannot invent the missing Hook of an already executed parent user turn.
            // 迟到回流不能虚构已执行父用户回合缺失的 Hook。
            let preparation = MainBusinessHookPreparation::capture(document)?;
            if let MainBusinessHookPreparation::Initial(initial) = &preparation
                && (initial.origin != initial_origin || initial.start_seq != initial_seq)
            {
                return Err(String::from(
                    "Internal continuation is not the original business source.",
                ));
            }
            Ok(preparation)
        })
        .map_err(|error| error.to_string())??;
    preparation.install(runtime)
}

#[cfg(test)]
mod tests;
