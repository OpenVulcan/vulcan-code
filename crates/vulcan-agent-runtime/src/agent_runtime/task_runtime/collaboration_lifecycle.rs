//! Typed task-collaboration lifecycle requests crossing the runtime host port.
//! 跨运行时宿主端口传递的类型化任务协作生命周期请求。

use crate::model::SessionId;
use crate::tooling::collaboration::NativeAgentCollaborationRuntime;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use vulcan_session_core::v1::collaboration::{
    CollaborationChildTaskState, CollaborationChildTaskTerminalOutcome, CollaborationExecutionId,
    CollaborationExecutionPurpose, CollaborationOperationId, CollaborationParticipant,
    CollaborationQuestionExecutionOutcome, CollaborationQuestionExecutionState,
    CollaborationQuestionId,
};
use vulcan_session_core::v1::ids::{SubagentTaskId, TurnId};
use vulcan_tools_manager::VulcanToolInvocationContext;

/// Authenticated host port and tool coordinates passed directly by the native tool surface.
/// 由原生工具入口直接传递的已认证宿主端口与工具坐标。
#[derive(Clone)]
pub struct ChildTaskInvocation {
    /// Host authority for persistent child lifecycle mutations.
    /// 持久子生命周期修改的宿主权威。
    pub port: Arc<dyn NativeAgentCollaborationRuntime>,
    /// Original tool invocation; never reconstructed from display event metadata.
    /// 原始工具调用，不从显示事件元数据重建。
    pub invocation: VulcanToolInvocationContext,
}

/// Removes an unclaimed binding on cancellation or a backend that exits before runtime admission.
/// 在取消或后端于运行时准入前退出时移除未认领绑定。
pub(crate) struct ChildBindingRegistrationGuard {
    /// Runtime owning the transient binding registry.
    /// 拥有临时绑定注册表的运行时。
    pub runtime: Arc<super::TaskRuntime>,
    /// Exact registered execution; another execution is never removed.
    /// 精确已注册执行，不会移除其他执行。
    pub execution_id: String,
}

impl Drop for ChildBindingRegistrationGuard {
    /// Clears only an unclaimed binding; durable ownership belongs to the runtime after claim.
    /// 仅清理未认领绑定；认领后持久归属由运行时负责。
    fn drop(&mut self) {
        if let Err(error) = self
            .runtime
            .take_child_task_lifecycle_binding(&self.execution_id)
        {
            eprintln!("[task-runtime] child binding cleanup failed: {error}");
        }
    }
}

/// Prepared settings cross the host port using the same serializable domain contract.
/// 已准备设置使用同一可序列化领域契约跨越宿主端口。
pub use vulcan_session_core::v1::child_execution::ChildExecutionPreparation as ChildExecutionProfileCaptureRequest;

/// Stable runtime identity of one child execution attempt.
/// 一次子执行尝试的稳定运行时身份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildTaskExecutionIdentity {
    /// Physical TaskSession identity used by the runtime registry.
    /// 运行时注册表使用的物理 TaskSession 身份。
    pub task_session_id: SessionId,
    /// Concrete execution identity that owns the lifecycle reservation.
    /// 持有生命周期预留的具体执行身份。
    pub execution_id: String,
    /// Stable external task identity recorded by the collaboration domain.
    /// 协作领域记录的稳定外部任务身份。
    pub task_id: SubagentTaskId,
}

/// Request that reserves one child task before its provider launch can escape.
/// 在子任务 Provider 启动事件逸出前预留它的请求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChildTaskReservationRequest {
    /// Trusted invocation identity that owns retries and deduplication.
    /// 负责重试与去重的可信工具调用身份。
    pub invocation: VulcanToolInvocationContext,
    /// Stable collaboration operation identity derived from the invocation.
    /// 从调用身份派生的稳定协作操作身份。
    pub operation_id: CollaborationOperationId,
    /// Concrete child execution identity.
    /// 具体子执行身份。
    pub execution: ChildTaskExecutionIdentity,
    /// Semantic reservation timestamp in Unix milliseconds.
    /// 语义预留时间（Unix 毫秒）。
    pub reserved_at_ms: i64,
}

/// Request that activates a durable child reservation after launch admission.
/// 在启动准入后激活可靠子任务预留的请求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChildTaskActivationRequest {
    /// Trusted invocation identity that owns the reservation.
    /// 持有该预留的可信工具调用身份。
    pub invocation: VulcanToolInvocationContext,
    /// Stable collaboration operation identity used for the activation retry.
    /// 用于激活重试的稳定协作操作身份。
    pub operation_id: CollaborationOperationId,
    /// Concrete child execution identity being activated.
    /// 正在激活的具体子执行身份。
    pub execution: ChildTaskExecutionIdentity,
    /// Semantic activation timestamp in Unix milliseconds.
    /// 语义激活时间（Unix 毫秒）。
    pub active_at_ms: i64,
}

/// Request that settles a child reservation exactly once.
/// 精确结算一次子任务预留的请求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChildTaskTerminalRequest {
    /// Trusted invocation identity that owns the original reservation.
    /// 持有原始预留的可信工具调用身份。
    pub invocation: VulcanToolInvocationContext,
    /// Stable collaboration operation identity used for terminal retry.
    /// 用于终态重试的稳定协作操作身份。
    pub operation_id: CollaborationOperationId,
    /// Concrete child execution identity being settled.
    /// 正在结算的具体子执行身份。
    pub execution: ChildTaskExecutionIdentity,
    /// Explicit terminal outcome observed by the runtime.
    /// 运行时观测到的明确终态结果。
    pub outcome: CollaborationChildTaskTerminalOutcome,
    /// Semantic terminal timestamp in Unix milliseconds.
    /// 语义终态时间（Unix 毫秒）。
    pub terminal_at_ms: i64,
}

/// Typed acknowledgement returned after a child lifecycle mutation.
/// 子任务生命周期修改完成后返回的类型化确认。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildTaskLifecycleReceipt {
    /// Exact execution acknowledged independently of the reusable task session.
    /// 被确认的精确执行，独立于可复用任务会话。
    pub execution_id: CollaborationExecutionId,
    /// Stable operation identity acknowledged by the domain owner.
    /// 领域所有者确认的稳定操作身份。
    pub operation_id: CollaborationOperationId,
    /// Stable external task identity acknowledged by the domain owner.
    /// 领域所有者确认的稳定外部任务身份。
    pub task_id: SubagentTaskId,
    /// Child lifecycle state after the mutation.
    /// 修改后的子任务生命周期状态。
    pub state: CollaborationChildTaskState,
    /// Parent turn that owns this child completion obligation.
    /// 拥有该子完成义务的父回合。
    pub parent_turn_id: TurnId,
    /// Durable document revision containing the acknowledgement.
    /// 包含该确认的可靠文档修订号。
    pub document_revision: u64,
    /// Whether this invocation changed the authoritative document.
    /// 本次调用是否改变了权威文档。
    pub changed: bool,
}

/// Carries one trusted collaboration port and its parent-bound child reservation across launch phases.
/// 携带一个可信协作端口及其绑定原始父回合的子任务预留，贯穿启动各阶段。
#[derive(Clone)]
pub struct ChildTaskLifecycleBinding {
    /// Host port that owns durable collaboration mutations for this execution.
    /// 持有本次执行可靠协作变更权限的宿主端口。
    pub port: Arc<dyn NativeAgentCollaborationRuntime>,
    /// Reservation request whose invocation and parent turn remain stable through retries.
    /// 预留请求中的调用身份与父回合在所有重试中保持稳定。
    pub reservation: ChildTaskReservationRequest,
}

impl ChildTaskLifecycleBinding {
    /// Builds the typed activation request from the original reservation identity.
    /// 根据原始预留身份构建类型化激活请求。
    pub(crate) fn activation_request(
        &self,
        active_at_ms: i64,
    ) -> Result<ChildTaskActivationRequest, String> {
        Ok(ChildTaskActivationRequest {
            invocation: self.reservation.invocation.clone(),
            operation_id: phase_operation_id(&self.reservation.operation_id, "activate")?,
            execution: self.reservation.execution.clone(),
            active_at_ms,
        })
    }
}

/// Derives a stable phase operation id without changing the invocation-owned base identity.
/// 在不改变调用方基础身份的前提下派生稳定阶段操作身份。
fn phase_operation_id(
    base: &CollaborationOperationId,
    phase: &str,
) -> Result<CollaborationOperationId, String> {
    CollaborationOperationId::try_new(format!("{}:{phase}", base.as_str()))
        .map_err(|error| format!("failed to derive child lifecycle operation identity: {error}"))
}

/// Request that begins a dedicated question execution.
/// 开始一次专用问题执行的请求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionExecutionBeginRequest {
    /// Trusted invocation identity that owns the question execution.
    /// 持有问题执行的可信工具调用身份。
    pub invocation: VulcanToolInvocationContext,
    /// Stable operation identity used for idempotent begin.
    /// 用于幂等开始的稳定操作身份。
    pub operation_id: CollaborationOperationId,
    /// Stable execution identity for this physical QA run.
    /// 本次物理问答运行的稳定执行身份。
    pub execution_id: CollaborationExecutionId,
    /// Stable question identity being answered.
    /// 正在回答的稳定问题身份。
    pub question_id: CollaborationQuestionId,
    /// Verified target participant owning the execution.
    /// 拥有该执行的已校验目标参与者。
    pub participant: CollaborationParticipant,
    /// Dedicated question purpose; normal tasks are not accepted here.
    /// 专用问题用途；此处不接纳普通任务。
    pub purpose: CollaborationExecutionPurpose,
    /// Original parent turn retained for in-turn questions.
    /// 原回合内问题保留的原始父回合。
    pub parent_turn_id: Option<TurnId>,
    /// Semantic begin timestamp in Unix milliseconds.
    /// 语义开始时间（Unix 毫秒）。
    pub started_at_ms: i64,
}

/// Request that completes a dedicated question execution.
/// 完成一次专用问题执行的请求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionExecutionCompleteRequest {
    /// Trusted invocation identity that owns the question execution.
    /// 持有问题执行的可信工具调用身份。
    pub invocation: VulcanToolInvocationContext,
    /// Stable operation identity used for idempotent completion.
    /// 用于幂等完成的稳定操作身份。
    pub operation_id: CollaborationOperationId,
    /// Stable execution identity being completed.
    /// 正在完成的稳定执行身份。
    pub execution_id: CollaborationExecutionId,
    /// Explicit outcome observed by the execution owner.
    /// 执行所有者观测到的明确结果。
    pub outcome: CollaborationQuestionExecutionOutcome,
    /// Semantic completion timestamp in Unix milliseconds.
    /// 语义完成时间（Unix 毫秒）。
    pub completed_at_ms: i64,
}

/// Typed acknowledgement returned after a question execution mutation.
/// 问题执行变更完成后返回的类型化确认。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionExecutionLifecycleReceipt {
    /// Stable operation identity acknowledged by the domain owner.
    /// 领域所有者确认的稳定操作身份。
    pub operation_id: CollaborationOperationId,
    /// Stable physical question execution identity.
    /// 稳定的物理问题执行身份。
    pub execution_id: CollaborationExecutionId,
    /// Question execution state after the mutation.
    /// 变更后的问题执行状态。
    pub state: CollaborationQuestionExecutionState,
    /// Explicit terminal outcome when the execution has completed.
    /// 执行完成时记录的明确终态结果。
    pub outcome: Option<CollaborationQuestionExecutionOutcome>,
    /// Durable document revision containing the acknowledgement.
    /// 包含该确认的可靠文档修订号。
    pub document_revision: u64,
    /// Whether this invocation changed the authoritative document.
    /// 本次调用是否改变了权威文档。
    pub changed: bool,
}
