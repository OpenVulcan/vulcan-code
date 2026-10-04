//! WebUI prompt lifecycle pending prepare helpers.
//! WebUI 提示词生命周期 pending prepare 辅助逻辑。

use super::*;
use vulcan_session_core::v1::document::{
    ApiGroupCompletionReceipt, SubagentReturnReceipt, TurnOrigin,
};
use vulcan_session_core::v1::event::{EventBody, UserInputSource};
use vulcan_session_core::v1::ids::{
    ClientActionId, PendingInputId, ResourceId, Seq, SubagentTaskId, TurnId,
};
use vulcan_workbench_model::{PendingPromptRuntimeOptions, PendingSessionPromptEntry};

use crate::{
    conversation_submission::{
        effective_pending_prompt_reasoning_effort, exact_v1_dispatch_runtime_options,
    },
    emit_webui_subagent_navigation_changed_delta_for_parent,
    project_conversation_services::try_begin_pending_prompt_dispatch_exclusion,
    session_goal_lifecycle::{
        SessionGoalDispatchAuthority, registered_session_v1_handle_from_state,
        session_goal_dispatch_authority_from_v1_handle,
    },
    session_runtime_registry_v1::SessionHandle,
    start_reserved_webui_session_kernel_turn, start_reserved_webui_session_kernel_user_turn,
};

/// Exact V1 authority of one queued user input already promoted into an open Turn.
/// 已经提升为开放回合的一条排队用户输入的精确 V1 权威。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionQueuedDispatchAuthority {
    /// Open Turn created by the completion or Goal-supersession mother transaction.
    /// 由完成或目标替代母事务创建的开放回合。
    pub(crate) turn_id: TurnId,
    /// Reused pending-input identity.
    /// 复用的待输入身份。
    pub(crate) input_id: PendingInputId,
    /// Browser idempotency identity retained by the authoritative input event.
    /// 权威输入事件保留的浏览器幂等身份。
    pub(crate) client_action_id: ClientActionId,
    /// Exact seq of the promoted Provider-visible input.
    /// 已提升 Provider 可见输入的精确 seq。
    pub(crate) input_seq: Seq,
    /// Immutable user text copied from the authoritative input event.
    /// 从权威输入事件复制的不可变用户正文。
    pub(crate) text: String,
    /// Immutable ordered project resource identities copied from the input event.
    /// 从输入事件复制的不可变有序项目资源身份。
    pub(crate) resource_ids: Vec<ResourceId>,
}

/// Exact V1 authority for Provider-visible user inputs appended after one physical API terminal.
/// 在一次物理 API 终态之后追加的 Provider 可见用户输入的精确 V1 权威。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionPendingInTurnDispatchAuthority {
    /// Open logical Turn that owns both the terminal API and the appended inputs.
    /// 同时持有终态 API 与追加输入的开放逻辑回合。
    pub(crate) turn_id: TurnId,
    /// Exact physical API that became terminal before the appended inputs.
    /// 在追加输入之前进入终态的精确物理 API。
    pub(crate) api_call_id: vulcan_session_core::v1::ids::ApiCallId,
    /// Exact terminal ApiStats seq immediately preceding this continuation boundary.
    /// 紧邻当前续跑边界之前的精确终态 ApiStats seq。
    pub(crate) stats_seq: Seq,
    /// Provider-visible input events in authoritative seq order.
    /// 按权威 seq 顺序排列的 Provider 可见输入事件。
    pub(crate) inputs: Vec<SessionPendingInTurnDispatchInput>,
}

/// One exact Provider-visible in-turn user input selected by the terminal mother receipt.
/// 由终态母回执选定的一条精确 Provider 可见回合内用户输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionPendingInTurnDispatchInput {
    /// Existing session-local pending input identity.
    /// 既有的会话本地待输入身份。
    pub(crate) input_id: PendingInputId,
    /// Browser idempotency identity preserved by the durable input event.
    /// 可靠输入事件保留的浏览器幂等身份。
    pub(crate) client_action_id: ClientActionId,
    /// Exact Provider-visible event seq.
    /// 精确的 Provider 可见事件 seq。
    pub(crate) input_seq: Seq,
    /// Immutable user text copied from the durable event.
    /// 从可靠事件复制的不可变用户正文。
    pub(crate) text: String,
    /// Immutable ordered project resources copied from the durable event.
    /// 从可靠事件复制的不可变有序项目资源。
    pub(crate) resource_ids: Vec<ResourceId>,
}

/// Rebuilds and validates one same-Turn dispatch authority from an API terminal receipt.
/// 从一条 API 终态回执重建并校验一次同回合分发权威。
pub(crate) fn pending_in_turn_dispatch_authority_from_v1_handle(
    handle: &SessionHandle,
    receipt: &ApiGroupCompletionReceipt,
) -> Result<SessionPendingInTurnDispatchAuthority, String> {
    handle
        .read(|document| {
            let turn = document.active_turn().ok_or_else(|| {
                String::from("The promoted in-turn inputs have no authoritative open V1 Turn.")
            })?;
            if turn.closed || turn.turn_id != receipt.turn_id {
                return Err(String::from(
                    "The promoted inputs no longer belong to the authoritative open V1 Turn.",
                ));
            }
            if document.active_api().is_some() {
                return Err(String::from(
                    "The previous physical API is still active before in-turn redispatch.",
                ));
            }
            let stats_event = document
                .events()
                .iter()
                .find(|event| event.seq == receipt.stats_seq)
                .ok_or_else(|| String::from("The API terminal receipt seq is missing."))?;
            let EventBody::ApiStats(stats) = &stats_event.event else {
                return Err(String::from(
                    "The API terminal receipt does not point to ApiStats.",
                ));
            };
            if stats_event.turn_id != Some(receipt.turn_id)
                || stats.api_call_id != receipt.api_call_id
            {
                return Err(String::from(
                    "The API terminal receipt differs from the durable V1 terminal event.",
                ));
            }
            if receipt.promoted_input_seqs.is_empty() {
                return Err(String::from(
                    "The API terminal receipt contains no promoted in-turn input.",
                ));
            }
            let mut previous_seq = receipt.stats_seq;
            let mut inputs = Vec::with_capacity(receipt.promoted_input_seqs.len());
            for input_seq in &receipt.promoted_input_seqs {
                if *input_seq <= previous_seq {
                    return Err(String::from(
                        "Promoted in-turn input seq values are not strictly ordered.",
                    ));
                }
                let event = document
                    .events()
                    .iter()
                    .find(|event| event.seq == *input_seq)
                    .ok_or_else(|| String::from("A promoted in-turn input seq is missing."))?;
                let EventBody::UserInput(input) = &event.event else {
                    return Err(String::from(
                        "A promoted in-turn input receipt does not point to user_input.",
                    ));
                };
                if event.turn_id != Some(receipt.turn_id)
                    || input.input_source != UserInputSource::PendingInTurn
                {
                    return Err(String::from(
                        "A promoted input differs from the durable same-Turn V1 authority.",
                    ));
                }
                inputs.push(SessionPendingInTurnDispatchInput {
                    input_id: input.input_id,
                    client_action_id: input.client_action_id.clone(),
                    input_seq: *input_seq,
                    text: input.text.clone(),
                    resource_ids: input.resource_ids.clone(),
                });
                previous_seq = *input_seq;
            }
            let already_dispatched =
                document.business_input_dispatch_started(receipt.turn_id, previous_seq);
            if already_dispatched {
                return Err(String::from(
                    "The promoted in-turn input boundary already owns a later physical API.",
                ));
            }
            Ok(SessionPendingInTurnDispatchAuthority {
                turn_id: receipt.turn_id,
                api_call_id: receipt.api_call_id,
                stats_seq: receipt.stats_seq,
                inputs,
            })
        })
        .map_err(|error| format!("读取 V1 回合内输入分发权威失败：{error}"))?
}

/// Exact V1 authority of one root user input already owning an open Turn.
/// 已经持有开放回合的一条根用户输入的精确 V1 权威。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionUserRootDispatchAuthority {
    /// Open UserRoot Turn created by a new submission or rebuilt by edit publication.
    /// 由新提交创建或由编辑代次发布重建的开放 UserRoot 回合。
    pub(crate) turn_id: TurnId,
    /// Session-local input identity retained by the rewritten event.
    /// 已重写事件保留的会话本地输入身份。
    pub(crate) input_id: PendingInputId,
    /// Browser idempotency identity retained by the rewritten event.
    /// 已重写事件保留的浏览器幂等身份。
    pub(crate) client_action_id: ClientActionId,
    /// Exact seq of the Provider-visible root input.
    /// Provider 可见根输入的精确 seq。
    pub(crate) input_seq: Seq,
    /// Immutable root text copied from the authoritative event.
    /// 从权威事件复制的不可变根正文。
    pub(crate) text: String,
    /// Immutable ordered project resources copied from the authoritative event.
    /// 从权威事件复制的不可变有序项目资源。
    pub(crate) resource_ids: Vec<ResourceId>,
}

/// Exact V1 authority of one durable subagent return awaiting parent inference.
/// 等待父级推理的一条可靠子代理回流的精确 V1 权威。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionSubagentReturnDispatchAuthority {
    /// Exact parent history generation captured with the return source.
    /// 与回流来源一起捕获的精确父历史代次。
    pub(crate) generation: u64,
    /// Parent open Turn that owns the returned child result.
    /// 持有已回流子结果的父级开放回合。
    pub(crate) parent_turn_id: TurnId,
    /// Provider-visible subagent return seq.
    /// Provider 可见的子代理回流 seq。
    pub(crate) return_seq: Seq,
    /// Exact child session identity.
    /// 精确子会话身份。
    pub(crate) child_session_id: String,
    /// Exact child task identity.
    /// 精确子任务身份。
    pub(crate) child_task_id: SubagentTaskId,
    /// Exact child assistant-text seq selected as the returned body.
    /// 被选作回流正文的精确子会话 assistant 文本 seq。
    pub(crate) source_assistant_text_seq: Seq,
    /// Exact child terminal TurnStats seq copied by the parent event pair.
    /// 父级事件对复制的精确子会话终态 TurnStats seq。
    pub(crate) child_turn_stats_seq: Seq,
    /// Immutable returned body copied from the authoritative V1 event.
    /// 从权威 V1 事件复制的不可变回流正文。
    pub(crate) content: String,
}

impl SessionSubagentReturnDispatchAuthority {
    /// Reconstructs the exact source receipt for canonical revalidation without interpreting content.
    /// 为规范重验重建精确来源回执，不解释正文。
    pub(super) fn receipt(&self) -> SubagentReturnReceipt {
        SubagentReturnReceipt {
            parent_turn_id: self.parent_turn_id,
            return_seq: self.return_seq,
            child_session_id: self.child_session_id.clone(),
            child_task_id: self.child_task_id.clone(),
            source_assistant_text_seq: self.source_assistant_text_seq,
            child_turn_stats_seq: self.child_turn_stats_seq,
        }
    }
}

/// Reads and validates one subagent return directly from its parent V1 receipt.
/// 直接依据父会话 V1 回执读取并校验一条子代理回流。
pub(crate) fn subagent_return_dispatch_authority_from_v1_handle(
    handle: &SessionHandle,
    receipt: &SubagentReturnReceipt,
) -> Result<SessionSubagentReturnDispatchAuthority, String> {
    handle
        .read(|document| subagent_return_dispatch_authority_from_document(document, receipt))
        .map_err(|error| format!("读取 V1 子代理回流失败：{error}"))?
}

/// Validates a return receipt against one complete snapshot supplied by its storage owner.
/// 对照存储所有者提供的一份完整快照校验回流回执。
/// Returns the immutable source fields or rejects a different turn, event, or child identity.
/// 返回不可变来源字段，或拒绝不同回合、事件或子身份。
pub(super) fn subagent_return_dispatch_authority_from_document(
    document: &vulcan_session_core::v1::document::SessionDocument,
    receipt: &SubagentReturnReceipt,
) -> Result<SessionSubagentReturnDispatchAuthority, String> {
    // Validate the persisted content identity before resuming the parent.
    // 恢复父会话之前校验已持久化的内容身份。
    let turn = document
        .active_turn()
        .ok_or("The subagent return has no open parent Turn.")?;
    if turn.closed || turn.turn_id != receipt.parent_turn_id {
        return Err(String::from(
            "The subagent return no longer belongs to the open parent Turn.",
        ));
    }
    let record = document
        .events()
        .iter()
        .find(|event| event.seq == receipt.return_seq)
        .ok_or("The subagent return seq is missing.")?;
    let EventBody::SubagentReturn(returned) = &record.event else {
        return Err(String::from(
            "The receipt does not point to a subagent return.",
        ));
    };
    if record.turn_id != Some(receipt.parent_turn_id)
        || returned.child_session_id != receipt.child_session_id
        || returned.child_task_id != receipt.child_task_id
        || returned.child_turn_stats_seq != receipt.child_turn_stats_seq
        || returned.source_assistant_text_seq != receipt.source_assistant_text_seq
    {
        return Err(String::from(
            "The subagent return differs from its durable receipt.",
        ));
    }
    Ok(SessionSubagentReturnDispatchAuthority {
        generation: document.segment_generation(),
        parent_turn_id: receipt.parent_turn_id,
        return_seq: receipt.return_seq,
        child_session_id: receipt.child_session_id.clone(),
        child_task_id: receipt.child_task_id.clone(),
        source_assistant_text_seq: receipt.source_assistant_text_seq,
        child_turn_stats_seq: receipt.child_turn_stats_seq,
        content: returned.content.clone(),
    })
}

/// Reads and validates one queued successor directly from the authoritative V1 event.
/// 直接从权威 V1 事件读取并校验一个排队后继。
pub(crate) fn queued_dispatch_authority_from_v1_handle(
    handle: &SessionHandle,
    turn_id: TurnId,
    input_id: PendingInputId,
    client_action_id: &ClientActionId,
    input_seq: Seq,
) -> Result<SessionQueuedDispatchAuthority, String> {
    handle
        .read(|document| {
            let turn = document.active_turn().ok_or_else(|| {
                String::from("The queued successor has no authoritative open V1 Turn.")
            })?;
            if turn.closed
                || turn.origin != TurnOrigin::UserRoot
                || turn.turn_id != turn_id
                || turn.start_seq != input_seq
            {
                return Err(String::from(
                    "The queued successor no longer owns the authoritative open V1 Turn.",
                ));
            }
            // Revalidate automatic post-QA dispatch after acquiring its reservation, not just before it.
            // 取得分发预留后再次校验自动答疑后分发，而不只是预留之前校验。
            // The reserved caller prevents Stop from crossing this proof and kernel admission.
            // 持有预留的调用者阻止 Stop 跨越此证明与内核准入。
            let auxiliary_successor = document
                .collaboration_state()
                .question_executions
                .values()
                .any(|execution| {
                    execution
                        .queued_successor
                        .as_ref()
                        .is_some_and(|successor| successor.turn_id == turn_id)
                });
            if auxiliary_successor
                && document.collaboration_run_control()
                    != vulcan_session_core::v1::collaboration::CollaborationRunControl::Running
            {
                return Err(String::from(
                    "The queued question successor is stopped and cannot dispatch automatically.",
                ));
            }
            let event = document
                .events()
                .iter()
                .find(|event| event.seq == input_seq)
                .ok_or_else(|| String::from("The queued successor input seq is missing."))?;
            if event.turn_id != Some(turn_id) {
                return Err(String::from(
                    "The queued successor input belongs to another V1 Turn.",
                ));
            }
            let EventBody::UserInput(input) = &event.event else {
                return Err(String::from(
                    "The queued successor seq is not a V1 user-input event.",
                ));
            };
            if input.input_source != UserInputSource::QueuedScheduled
                || input.input_id != input_id
                || &input.client_action_id != client_action_id
            {
                return Err(String::from(
                    "The queued successor identity differs from its durable completion receipt.",
                ));
            }
            Ok(SessionQueuedDispatchAuthority {
                turn_id,
                input_id,
                client_action_id: client_action_id.clone(),
                input_seq,
                text: input.text.clone(),
                resource_ids: input.resource_ids.clone(),
            })
        })
        .map_err(|error| format!("读取 V1 排队后继失败：{error}"))?
}

/// Reads one undispatched root input directly from the authoritative open V1 Turn.
/// 直接从权威开放 V1 回合读取一条尚未分发的根输入。
///
/// # Parameters / 参数
/// `handle` owns the exact resident generation and `input_seq` is the root boundary published by
/// edit or fork.
/// `handle` 持有精确常驻代次；`input_seq` 是编辑或分叉发布的根边界。
///
/// # Returns / 返回值
/// Returns the exact immutable payload when no business API has started and no physical API is active.
/// 在尚未启动业务 API 且没有活动物理 API 时返回精确不可变载荷。
pub(crate) fn user_root_dispatch_authority_from_v1_handle(
    handle: &SessionHandle,
    input_seq: Seq,
) -> Result<SessionUserRootDispatchAuthority, String> {
    handle
        .read(|document| {
            let turn = document
                .active_turn()
                .ok_or_else(|| String::from("The root input has no authoritative open V1 Turn."))?;
            if turn.closed || turn.origin != TurnOrigin::UserRoot || turn.start_seq != input_seq {
                return Err(String::from(
                    "The root input no longer owns the authoritative open V1 Turn.",
                ));
            }
            if document.active_api().is_some()
                || document.business_input_dispatch_started(turn.turn_id, input_seq)
            {
                return Err(String::from(
                    "The authoritative root Turn already owns a physical API.",
                ));
            }
            let event = document
                .events()
                .iter()
                .find(|event| event.seq == input_seq)
                .ok_or_else(|| String::from("The root input seq is missing."))?;
            if event.turn_id != Some(turn.turn_id) {
                return Err(String::from("The root input belongs to another V1 Turn."));
            }
            let EventBody::UserInput(input) = &event.event else {
                return Err(String::from(
                    "The root boundary is not a V1 user-input event.",
                ));
            };
            if input.input_source != UserInputSource::Root {
                return Err(String::from(
                    "The root dispatch boundary is not a root user input.",
                ));
            }
            Ok(SessionUserRootDispatchAuthority {
                turn_id: turn.turn_id,
                input_id: input.input_id,
                client_action_id: input.client_action_id.clone(),
                input_seq,
                text: input.text.clone(),
                resource_ids: input.resource_ids.clone(),
            })
        })
        .map_err(|error| format!("读取 V1 根输入分发权威失败：{error}"))?
}

/// Resolves the exact scheduled user Turn created for one browser action.
/// 解析为一个浏览器动作创建的精确已调度用户回合。
///
/// # Parameters
/// 参数
/// `handle` is the unique V1 session owner and `client_action_id` is the durable browser identity.
/// `handle` 是唯一 V1 会话 owner；`client_action_id` 是可靠浏览器身份。
///
/// # Returns
/// 返回值
/// Returns the fully validated queued-dispatch authority of the current open UserRoot Turn.
/// 返回当前开放 UserRoot 回合经过完整校验的排队分发权威。
pub(crate) fn queued_dispatch_authority_for_client_action(
    handle: &SessionHandle,
    client_action_id: &ClientActionId,
) -> Result<SessionQueuedDispatchAuthority, String> {
    let (turn_id, input_id, input_seq) = handle
        .read(|document| {
            let turn = document.active_turn().ok_or_else(|| {
                String::from("The scheduled browser action has no authoritative open V1 Turn.")
            })?;
            if turn.closed || turn.origin != TurnOrigin::UserRoot {
                return Err(String::from(
                    "The scheduled browser action does not own an open V1 UserRoot Turn.",
                ));
            }
            let event = document
                .events()
                .iter()
                .find(|event| event.seq == turn.start_seq)
                .ok_or_else(|| {
                    String::from("The scheduled browser action input seq is missing.")
                })?;
            let EventBody::UserInput(input) = &event.event else {
                return Err(String::from(
                    "The scheduled browser action Turn does not start with a V1 user input.",
                ));
            };
            if input.input_source != UserInputSource::QueuedScheduled
                || &input.client_action_id != client_action_id
            {
                return Err(String::from(
                    "The open V1 Turn belongs to a different scheduled browser action.",
                ));
            }
            Ok((turn.turn_id, input.input_id, event.seq))
        })
        .map_err(|error| format!("读取 V1 已调度用户回合失败：{error}"))??;
    queued_dispatch_authority_from_v1_handle(handle, turn_id, input_id, client_action_id, input_seq)
}

/// Exact durable authority consumed by one process-local Provider admission.
/// 一次进程内 Provider 准入所消费的精确可靠权威。
pub(super) enum ExactPendingPromptDispatch {
    /// Accepted business guidance resumes its own original turn.
    /// 已接纳业务方向恢复其原回合。
    Collaboration {
        /// Exact immutable local acceptance anchor.
        /// 精确不可变本地接纳锚点。
        authority: vulcan_session_core::v1::collaboration::CollaborationContinuation,
        /// Non-persistent host dispatch adapter.
        /// 非持久化宿主分发适配对象。
        entry: PendingSessionPromptEntry,
    },
    /// Agent task input already claimed into its independent business turn.
    /// 已认领到独立业务回合的代理任务输入。
    AgentTask {
        /// Exact claim, execution and input identity.
        /// 精确认领、执行及输入身份。
        authority: SessionAgentTaskDispatchAuthority,
        /// Hydrated target candidate; only its session slice may be published.
        /// 已水合目标候选；只允许发布其中的目标会话切片。
        workspace: WorkspaceSnapshot,
        /// Non-persistent adapter of the canonical agent input.
        /// 规范代理输入的非持久适配对象。
        entry: PendingSessionPromptEntry,
    },
    /// One open Goal-continuation Turn selected by the V1 domain transaction.
    /// 由 V1 领域事务选定的开放 Goal 续跑回合。
    Goal {
        /// Exact durable Goal and Turn identity.
        /// 精确可靠的 Goal 与回合身份。
        authority: SessionGoalDispatchAuthority,
        /// Non-persistent execution adapter built from that authority.
        /// 由该权威构建且不持久化的执行适配对象。
        entry: PendingSessionPromptEntry,
    },
    /// One queued user input already promoted into an open Turn.
    /// 已经提升为开放回合的一条排队用户输入。
    Queued {
        /// Exact durable queued-input identity.
        /// 精确可靠的排队输入身份。
        authority: SessionQueuedDispatchAuthority,
        /// Non-persistent execution adapter built from that authority.
        /// 由该权威构建且不持久化的执行适配对象。
        entry: PendingSessionPromptEntry,
    },
    /// One root user input published by a new submission or edited generation.
    /// 由新提交或编辑代际发布的一条根用户输入。
    Root {
        /// Exact durable root-input identity.
        /// 精确可靠的根输入身份。
        authority: SessionUserRootDispatchAuthority,
        /// Non-persistent execution adapter built from that authority.
        /// 由该权威构建且不持久化的执行适配对象。
        entry: PendingSessionPromptEntry,
    },
    /// One durable subagent return appended to its parent Turn.
    /// 已追加到父回合的一条可靠子代理回流。
    SubagentReturn {
        /// Exact durable parent-return identity.
        /// 精确可靠的父级回流身份。
        authority: SessionSubagentReturnDispatchAuthority,
        /// Non-persistent execution adapter built from that authority.
        /// 由该权威构建且不持久化的执行适配对象。
        entry: PendingSessionPromptEntry,
    },
    /// Provider-visible inputs appended after one physical API terminal.
    /// 在一次物理 API 终态后追加的 Provider 可见输入。
    PendingInTurn {
        /// Exact durable same-Turn continuation identity.
        /// 精确可靠的同回合续跑身份。
        authority: SessionPendingInTurnDispatchAuthority,
        /// Non-persistent execution adapter built from that authority.
        /// 由该权威构建且不持久化的执行适配对象。
        entry: PendingSessionPromptEntry,
    },
}

impl ExactPendingPromptDispatch {
    /// Returns the process-local adapter attached to the durable authority.
    /// 返回附着于可靠权威的进程内适配对象。
    fn entry(&self) -> &PendingSessionPromptEntry {
        match self {
            Self::Goal { entry, .. }
            | Self::Collaboration { entry, .. }
            | Self::AgentTask { entry, .. }
            | Self::Queued { entry, .. }
            | Self::Root { entry, .. }
            | Self::SubagentReturn { entry, .. }
            | Self::PendingInTurn { entry, .. } => entry,
        }
    }

    /// Returns the exact prompt source without consulting any process mirror.
    /// 不查询任何进程镜像，返回精确提示词来源。
    fn source(&self) -> PendingSessionPromptSource {
        match self {
            Self::Goal { .. } => PendingSessionPromptSource::GoalContinuation,
            Self::Collaboration { .. } => PendingSessionPromptSource::AgentContinuation,
            Self::AgentTask { authority, .. } => authority.prompt_source(),
            Self::Queued { .. } | Self::Root { .. } | Self::PendingInTurn { .. } => {
                PendingSessionPromptSource::UserFollowup
            }
            Self::SubagentReturn { .. } => PendingSessionPromptSource::SubagentReturnContinuation,
        }
    }

    /// Returns the canonical V1 input sequence owning a newly admitted human or agent business turn.
    /// 返回持有新准入人类或代理业务回合的规范 V1 输入序号。
    fn canonical_user_input_seq(&self) -> Option<Seq> {
        match self {
            Self::Queued { authority, .. } => Some(authority.input_seq),
            Self::AgentTask { authority, .. } => Some(authority.input_seq),
            Self::Root { authority, .. } => Some(authority.input_seq),
            Self::Goal { .. }
            | Self::Collaboration { .. }
            | Self::SubagentReturn { .. }
            | Self::PendingInTurn { .. } => None,
        }
    }
}

/// Starts the claimed agent task using its canonical input and the target's active settings.
/// 使用规范输入及目标当前设置，启动已认领代理任务。
/// Returns false while another service execution owns admission; errors preserve the durable claim.
/// 其他服务执行持有准入时返回 false；错误保留可靠认领事实。
pub(crate) async fn maybe_prepare_webui_agent_task(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &HostRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    authority: SessionAgentTaskDispatchAuthority,
) -> Result<bool, String> {
    // The entry is an execution adapter; the tagged message is already owned by V1 history.
    // 此项是执行适配对象；带标签消息已由 V1 历史持有。
    let handle = registered_session_v1_handle_from_state(state, session_id)?;
    let workspace = crate::session_hydration::load_workspace_for_submission(
        state,
        controller_runtime.clone(),
        Some(session_id),
    )
    .await?;
    let entry = PendingSessionPromptEntry {
        id: authority.execution_id.as_str().to_owned(),
        prompt: authority.text.clone(),
        hidden_model_guidance: None,
        preview: authority.text.clone(),
        attachments: Vec::new(),
        model_reference: None,
        runtime_options: exact_v1_dispatch_runtime_options(handle.as_ref())?,
        source: authority.prompt_source(),
        task_id: None,
        panel_entry_id: None,
        created_at: current_unix_millis_text(),
    };
    prepare_webui_pending_prompt(
        app,
        state,
        controller_runtime,
        agent_runtime,
        session_id,
        ExactPendingPromptDispatch::AgentTask {
            authority,
            workspace,
            entry,
        },
    )
    .await
}

/// Starts one Goal continuation directly from its exact durable V1 authority.
/// 直接从精确可靠的 V1 权威启动一次 Goal 续跑。
///
/// # Parameters
/// 参数
/// `authority` contains the exact Goal stage and open V1 Turn captured for this admission.
/// `authority` 包含本次准入捕获的精确 Goal 阶段与开放 V1 回合。
///
/// # Returns
/// 返回值
/// `true` only when this invocation owns and starts the formal PromptSubmission.
/// 仅当本次调用取得并启动正式 PromptSubmission 所有权时返回 `true`。
pub(crate) async fn maybe_prepare_webui_goal_continuation(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &HostRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    authority: SessionGoalDispatchAuthority,
) -> Result<bool, String> {
    let entry = exact_goal_continuation_prompt_entry(&authority, current_unix_millis_text());
    prepare_webui_pending_prompt(
        app,
        state,
        controller_runtime,
        agent_runtime,
        session_id,
        ExactPendingPromptDispatch::Goal { authority, entry },
    )
    .await
}

/// Starts one queued user successor directly from its exact durable V1 authority.
/// 直接从精确可靠的 V1 权威启动一个排队用户后继。
pub(crate) async fn maybe_prepare_webui_queued_successor(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &HostRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    authority: SessionQueuedDispatchAuthority,
    entry: PendingSessionPromptEntry,
) -> Result<bool, String> {
    prepare_webui_pending_prompt(
        app,
        state,
        controller_runtime,
        agent_runtime,
        session_id,
        ExactPendingPromptDispatch::Queued { authority, entry },
    )
    .await
}

/// Starts one root user Turn directly from its exact durable V1 authority.
/// 直接从精确可靠的 V1 权威启动一个根用户回合。
pub(crate) async fn maybe_prepare_webui_user_root(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &HostRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    authority: SessionUserRootDispatchAuthority,
    entry: PendingSessionPromptEntry,
) -> Result<bool, String> {
    prepare_webui_pending_prompt(
        app,
        state,
        controller_runtime,
        agent_runtime,
        session_id,
        ExactPendingPromptDispatch::Root { authority, entry },
    )
    .await
}

/// Starts one parent continuation directly from an exact durable subagent return.
/// 直接从一条精确可靠的子代理回流启动父级续跑。
pub(crate) async fn maybe_prepare_webui_subagent_return_continuation(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &HostRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    authority: SessionSubagentReturnDispatchAuthority,
) -> Result<bool, String> {
    let handle = registered_session_v1_handle_from_state(state, session_id)?;
    let entry = PendingSessionPromptEntry {
        id: format!(
            "webui-v1-subagent-return-{}-{}",
            authority.parent_turn_id.value(),
            authority.return_seq.value()
        ),
        prompt: String::from("Continue the active turn using the completed subagent return."),
        hidden_model_guidance: None,
        preview: authority.content.clone(),
        attachments: Vec::new(),
        model_reference: None,
        runtime_options: exact_v1_dispatch_runtime_options(handle.as_ref())?,
        source: PendingSessionPromptSource::SubagentReturnContinuation,
        task_id: Some(authority.child_task_id.as_str().to_string()),
        panel_entry_id: None,
        created_at: current_unix_millis_text(),
    };
    prepare_webui_pending_prompt(
        app,
        state,
        controller_runtime,
        agent_runtime,
        session_id,
        ExactPendingPromptDispatch::SubagentReturn { authority, entry },
    )
    .await
}

/// Starts one physical continuation from exact Provider-visible inputs already appended to the open Turn.
/// 从已经追加到开放回合的精确 Provider 可见输入启动一次物理续跑。
///
/// # Parameters
/// 参数
/// `authority` is rebuilt exclusively from the durable API terminal receipt and V1 events.
/// `authority` 仅由可靠 API 终态回执与 V1 事件重建。
/// `entry` is a process-local admission adapter and is never persisted as another user input.
/// `entry` 是进程局部准入适配对象，绝不会再次持久化为用户输入。
///
/// # Returns
/// 返回值
/// `true` only when this invocation owns and starts the next physical Provider request.
/// 仅当本次调用取得所有权并启动下一次物理 Provider 请求时返回 `true`。
pub(crate) async fn maybe_prepare_webui_pending_in_turn_continuation(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &HostRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    authority: SessionPendingInTurnDispatchAuthority,
    entry: PendingSessionPromptEntry,
) -> Result<bool, String> {
    prepare_webui_pending_prompt(
        app,
        state,
        controller_runtime,
        agent_runtime,
        session_id,
        ExactPendingPromptDispatch::PendingInTurn { authority, entry },
    )
    .await
}

/// Builds the process-local prompt adapter for one exact durable Goal Turn.
/// 为一个精确可靠的 Goal 回合构建进程局部提示词适配对象。
///
/// # Parameters
/// 参数
/// `authority` supplies the exact Goal and Turn identity; `created_at` is display-only timing.
/// `authority` 提供精确 Goal 与回合身份；`created_at` 仅用于显示时间。
///
/// # Returns
/// 返回值
/// A non-persisted adapter whose identity is unique to the authoritative V1 Turn.
/// 返回一个不持久化且身份唯一对应权威 V1 回合的适配对象。
fn exact_goal_continuation_prompt_entry(
    authority: &SessionGoalDispatchAuthority,
    created_at: String,
) -> PendingSessionPromptEntry {
    PendingSessionPromptEntry {
        id: format!(
            "webui-goal-auto-continuation-turn-{}",
            authority.turn_id.value()
        ),
        prompt: String::from("Continue working toward the active thread goal."),
        hidden_model_guidance: None,
        preview: String::from("继续执行目标"),
        attachments: Vec::new(),
        model_reference: None,
        runtime_options: PendingPromptRuntimeOptions::default(),
        source: PendingSessionPromptSource::GoalContinuation,
        task_id: Some(authority.goal_id.clone()),
        panel_entry_id: None,
        created_at,
    }
}

/// Executes one queued or exact-authority pending-prompt admission.
/// 执行一次队列或精确权威待发送提示词准入。
///
/// # Parameters
/// 参数
/// Exact dispatch inputs bypass legacy queues and remain local to this admission.
/// 精确分发输入绕过旧队列，并且仅存在于本次准入局部。
///
/// # Returns
/// 返回值
/// Whether the selected prompt started a formal Provider turn.
/// 返回选定提示词是否已启动正式 Provider 回合。
pub(super) async fn prepare_webui_pending_prompt(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &HostRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    exact_dispatch: ExactPendingPromptDispatch,
) -> Result<bool, String> {
    // Keep preparation observable until the detached continuation or stream assumes ownership.
    // 在分离续接或流接管所有权前，保持准备过程可被观测。
    let _dispatch_owner =
        crate::agent_collaboration::dispatch_owner::DispatchOwner::acquire(state, session_id)?;
    let (
        mut workspace_snapshot,
        has_active_stream_task,
        has_active_kernel_turn,
        has_active_session_run,
        has_pending_plan_artifact,
    ) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace_snapshot = match &exact_dispatch {
            ExactPendingPromptDispatch::AgentTask { workspace, .. } => workspace.clone(),
            _ => guard
                .workspace_snapshot
                .clone()
                .ok_or_else(|| String::from("workspace snapshot is not ready"))?,
        };
        let pending_plan = session_has_pending_plan_artifact(&workspace_snapshot, session_id);
        (
            workspace_snapshot,
            guard.active_stream_tasks.contains_key(&session_id),
            webui_session_kernel_has_active_turn(&guard, session_id),
            webui_session_has_active_run(&guard, session_id),
            pending_plan,
        )
    };
    webui_trace(
        "pending_prompt_prepare_check",
        format!(
            "session={session_id} has_active_stream_task={has_active_stream_task} has_active_kernel_turn={has_active_kernel_turn} has_active_session_run={has_active_session_run} has_pending_plan_artifact={has_pending_plan_artifact}",
        ),
    );
    if has_pending_plan_artifact {
        webui_trace(
            "pending_prompt_prepare_waiting_for_plan_decision",
            format!("session={session_id}"),
        );
        return Ok(false);
    }
    if has_active_stream_task || has_active_kernel_turn || has_active_session_run {
        return Ok(false);
    }
    let next_source = exact_dispatch.source();
    let goal_authority = match &exact_dispatch {
        ExactPendingPromptDispatch::Goal { authority, .. } => Some(authority),
        _ => None,
    };
    // User-authored work and Goal continuation both need one admission owner across validation,
    // media preparation, Hook setup, and kernel registration.
    // 用户编写工作与 Goal 续跑都需要一个准入所有者，覆盖校验、媒体准备、Hook 设置与 kernel 注册。
    let Some(pending_prompt_dispatch_guard) =
        try_begin_pending_prompt_dispatch_exclusion(state, session_id)?
    else {
        webui_trace(
            "pending_prompt_prepare_admission_busy",
            format!("session={session_id} next_source={next_source:?}"),
        );
        return Ok(false);
    };
    let pending_prompt_dispatch_authorization =
        pending_prompt_dispatch_guard.existing_submission_authorization(session_id)?;
    if let Some(expected_goal_authority) = goal_authority {
        let handle = registered_session_v1_handle_from_state(state, session_id)?;
        let current_goal_authority =
            session_goal_dispatch_authority_from_v1_handle(handle.as_ref(), session_id)?;
        if current_goal_authority.as_ref() != Some(expected_goal_authority) {
            webui_trace(
                "pending_prompt_goal_authority_changed",
                format!(
                    "session={session_id} expected_turn_id={}",
                    expected_goal_authority.turn_id.value()
                ),
            );
            return Ok(false);
        }
    }
    let Some(session_title) = workspace_snapshot
        .sessions
        .iter()
        .find(|session| session.id == session_id)
        .map(|session| session.title.clone())
    else {
        webui_trace(
            "pending_prompt_prepare_missing_session_title",
            format!("session={session_id} next_source={next_source:?}"),
        );
        return Ok(false);
    };
    let mut entry = exact_dispatch.entry().clone();
    if entry.panel_entry_id.is_some() {
        return Err(String::from(
            "Exact V1 prompt admission cannot own a legacy pending-panel identity.",
        ));
    }
    // Only an exact in-turn continuation keeps interrupt semantics after the prior physical API becomes idle.
    // 仅精确回合内续跑会在上一物理 API 空闲后继续保留插队语义。
    if !matches!(
        &exact_dispatch,
        ExactPendingPromptDispatch::PendingInTurn { .. }
    ) {
        entry.source = entry.source.for_idle_dispatch();
    }
    webui_trace(
        "pending_prompt_prepare_start",
        format!(
            "session={session_id} entry_id={} source={:?} task_id={} prompt_chars={} attachments={}",
            entry.id,
            entry.source,
            entry.task_id.as_deref().unwrap_or("<none>"),
            entry.prompt.chars().count(),
            entry.attachments.len()
        ),
    );
    let subagent_navigation_state_changed = if entry.source.starts_user_parent_turn() {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let mut changed = guard
            .session_subagent_navigation_rounds
            .remove(&session_id)
            .is_some();
        if guard
            .active_subagent_panel
            .is_some_and(|selection| selection.parent_session_id == session_id)
        {
            guard.active_subagent_panel = None;
            changed = true;
        }
        changed
    } else {
        false
    };
    if subagent_navigation_state_changed {
        emit_webui_subagent_navigation_changed_delta_for_parent(app, state, session_id)?;
    }
    let workspace_path = workspace_path_for_session(&workspace_snapshot, session_id);
    let runtime_options = entry.runtime_options.clone();
    let entry_agent_runtime = agent_runtime_for_pending_prompt(agent_runtime, &entry);
    // Actual V1 preparation owns the ordered prompt provenance, original receipt and transfer lifecycle.
    // 真实V1准备持有有序提示词来源、原回执及传输生命周期。
    let provider_attachments =
        crate::media_upload::defer_attachment_delivery_to_v1(&entry.attachments);
    // reasoning_effort prevents a neutral internal queue placeholder from changing the active reasoning selection.
    // reasoning_effort 防止中性的内部队列占位值改变当前推理强度选择。
    let reasoning_effort = effective_pending_prompt_reasoning_effort(
        entry.source,
        runtime_options.reasoning_effort,
        active_reasoning_selection_effort(&entry_agent_runtime.provider_configuration),
    );
    let prompt_text = entry.prompt.clone();
    // canonicalAttachments strip every request-local delivery mode before the queued user message
    // enters durable history; providerAttachments retain only this dispatch's authoritative copy.
    // canonicalAttachments 在排队用户消息进入持久历史前移除全部单次请求传输模式；
    // providerAttachments 仅保留本次分发的权威副本。
    let canonical_attachments = entry
        .attachments
        .iter()
        .cloned()
        .map(|mut attachment| {
            attachment.delivery_mode = None;
            attachment
        })
        .collect();
    // vmmReadiness is the service-owned prerequisite captured before this process-local admission.
    // vmmReadiness 是在本次进程内准入前捕获的服务级权威前提事实。
    let hook_owner_handle = registered_session_v1_handle_from_state(state, session_id)?;
    if let ExactPendingPromptDispatch::AgentTask { authority, .. } = &exact_dispatch {
        super::agent_task_prepare::prepare_agent_task_hook_owner(
            &hook_owner_handle,
            &entry_agent_runtime,
            authority,
        )?;
    }
    // Internal sources restore their original owner before live timeline synchronization.
    // 内部来源在活动时间线同步之前恢复原所有者。
    match &exact_dispatch {
        ExactPendingPromptDispatch::Goal { authority, .. } => {
            super::internal_continuation_prepare::prepare_internal_continuation_hook_owner(
                &hook_owner_handle,
                &entry_agent_runtime,
                super::internal_continuation_prepare::InternalContinuationHookSource::Goal(
                    authority,
                ),
            )?;
        }
        ExactPendingPromptDispatch::SubagentReturn { authority, .. } => {
            super::internal_continuation_prepare::prepare_internal_continuation_hook_owner(
                &hook_owner_handle, &entry_agent_runtime,
                super::internal_continuation_prepare::InternalContinuationHookSource::SubagentReturn(authority),
            )?;
        }
        _ => {}
    }
    crate::session_business_hook::synchronize_live_business_hook_inputs(
        hook_owner_handle.as_ref(),
        &entry_agent_runtime,
    )?;
    let vmm_readiness = webui_vmm_runtime_readiness(state)?;
    let mut prompt_turn_begin_input = SharedPromptTurnBeginInput {
        session_id,
        session_title: &session_title,
        workspace_path: &workspace_path,
        prompt_entry_id: entry.id.clone(),
        continuation_action_id: None,
        prompt: &prompt_text,
        hidden_model_guidance: entry.hidden_model_guidance.as_deref(),
        provider_visible_agent_messages: Vec::new(),
        attachments: canonical_attachments,
        provider_attachments: Some(provider_attachments),
        existing_canonical_user_message_id: None,
        append_canonical_user_message: false,
        append_visible_user_prompt_record: false,
        source: entry.source,
        vmm_runtime_ready: vmm_readiness.service_ready,
        vmm_precheck_ready: vmm_readiness.precheck_ready,
        execution_mode: String::from("build"),
        reasoning_effort,
    };
    let precheck_start = if let Some(expected_goal_authority) = goal_authority {
        let handle = registered_session_v1_handle_from_state(state, session_id)?;
        let current_goal_authority =
            session_goal_dispatch_authority_from_v1_handle(handle.as_ref(), session_id)?;
        if current_goal_authority.as_ref() != Some(expected_goal_authority) {
            return Err(format!(
                "Goal dispatch authority changed for session `{session_id}` before Hook setup"
            ));
        }
        let authorization = pending_prompt_dispatch_authorization;
        // The submission reservation spans Hook preparation, durable capture and kernel registration.
        // 提交预留覆盖 Hook 准备、可靠捕获以及 kernel 注册。
        let mut hook_runtime = entry_agent_runtime
            .lock_conversation_hooks()
            .map_err(|error| format!("会话 hook runtime 加锁失败：{error}"))?;
        let precheck_start = begin_shared_prompt_turn(
            &mut workspace_snapshot,
            &mut hook_runtime,
            prompt_turn_begin_input,
        )?;
        drop(hook_runtime);
        crate::session_business_hook::publish_prepared_business_hook(
            handle.as_ref(),
            &entry_agent_runtime,
            &precheck_start,
        )?;
        // Disk publication completed without holding either shared service or Hook state.
        // 磁盘发布已完成，其间未持有共享服务状态或 Hook 状态。
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let active_turn_kind = if entry.source == PendingSessionPromptSource::GoalContinuation {
            ActiveTurnKind::GoalContinuation
        } else {
            ActiveTurnKind::UserPrompt
        };
        start_reserved_webui_session_kernel_turn(
            &mut guard,
            session_id,
            &precheck_start.preparation_turn_id,
            active_turn_kind,
            authorization.reservation_id,
        )?;
        precheck_start
    } else {
        if let ExactPendingPromptDispatch::AgentTask { authority, .. } = &exact_dispatch {
            let handle = registered_session_v1_handle_from_state(state, session_id)?;
            let current = agent_task_dispatch_authority_from_v1_handle(
                handle.as_ref(),
                &authority.delivery_id,
            )?;
            if &current != authority {
                return Err(String::from(
                    "Agent task authority changed before Hook setup.",
                ));
            }
        }
        if let ExactPendingPromptDispatch::Collaboration { authority, .. } = &exact_dispatch {
            let handle = registered_session_v1_handle_from_state(state, session_id)?;
            super::collaboration_prepare::validate_collaboration_dispatch(
                handle.as_ref(),
                authority,
            )?;
        }
        if let ExactPendingPromptDispatch::Queued {
            authority: expected_queued_authority,
            ..
        } = &exact_dispatch
        {
            let handle = registered_session_v1_handle_from_state(state, session_id)?;
            let current_queued_authority = queued_dispatch_authority_from_v1_handle(
                handle.as_ref(),
                expected_queued_authority.turn_id,
                expected_queued_authority.input_id,
                &expected_queued_authority.client_action_id,
                expected_queued_authority.input_seq,
            )?;
            if &current_queued_authority != expected_queued_authority {
                return Err(format!(
                    "Queued dispatch authority changed for session `{session_id}` before Hook setup"
                ));
            }
            // A retained receipt identifies the input, but cannot authorize another business dispatch.
            // 保留的回执标识输入，但不能授权再次执行业务分发。
            let occupied = handle
                .read(|document| {
                    document.active_api().is_some()
                        || document.business_input_dispatch_started(
                            expected_queued_authority.turn_id,
                            expected_queued_authority.input_seq,
                        )
                })
                .map_err(|error| error.to_string())?;
            if occupied {
                return Err(String::from(
                    "Queued input is physically occupied or already dispatched.",
                ));
            }
        }
        if let ExactPendingPromptDispatch::Root {
            authority: expected_root_authority,
            ..
        } = &exact_dispatch
        {
            let handle = registered_session_v1_handle_from_state(state, session_id)?;
            let current_root_authority = user_root_dispatch_authority_from_v1_handle(
                handle.as_ref(),
                expected_root_authority.input_seq,
            )?;
            if &current_root_authority != expected_root_authority {
                return Err(format!(
                    "Root dispatch authority changed for session `{session_id}` before Hook setup"
                ));
            }
        }
        if let ExactPendingPromptDispatch::SubagentReturn {
            authority: expected_subagent_authority,
            ..
        } = &exact_dispatch
        {
            let handle = registered_session_v1_handle_from_state(state, session_id)?;
            let current_subagent_authority = subagent_return_dispatch_authority_from_v1_handle(
                handle.as_ref(),
                &SubagentReturnReceipt {
                    parent_turn_id: expected_subagent_authority.parent_turn_id,
                    return_seq: expected_subagent_authority.return_seq,
                    child_session_id: expected_subagent_authority.child_session_id.clone(),
                    child_task_id: expected_subagent_authority.child_task_id.clone(),
                    child_turn_stats_seq: expected_subagent_authority.child_turn_stats_seq,
                    source_assistant_text_seq: expected_subagent_authority
                        .source_assistant_text_seq,
                },
            )?;
            if &current_subagent_authority != expected_subagent_authority {
                return Err(format!(
                    "Subagent return authority changed for session `{session_id}` before Hook setup"
                ));
            }
        }
        if let ExactPendingPromptDispatch::PendingInTurn {
            authority: expected_pending_authority,
            ..
        } = &exact_dispatch
        {
            let handle = registered_session_v1_handle_from_state(state, session_id)?;
            let current_pending_authority = pending_in_turn_dispatch_authority_from_v1_handle(
                handle.as_ref(),
                &ApiGroupCompletionReceipt {
                    api_call_id: expected_pending_authority.api_call_id,
                    turn_id: expected_pending_authority.turn_id,
                    stats_seq: expected_pending_authority.stats_seq,
                    promoted_input_seqs: expected_pending_authority
                        .inputs
                        .iter()
                        .map(|input| input.input_seq)
                        .collect(),
                },
            )?;
            if &current_pending_authority != expected_pending_authority {
                return Err(format!(
                    "Pending in-turn dispatch authority changed for session `{session_id}` before Hook setup"
                ));
            }
        }
        if let Some(canonical_user_input_seq) = exact_dispatch.canonical_user_input_seq() {
            let handle = registered_session_v1_handle_from_state(state, session_id)?;
            let projection = handle
                .capture_controller_projection_snapshot()
                .map_err(|error| error.to_string())?;
            let canonical_user_message_id =
                crate::agent_stream::provider_v1_sink::provider_projection_user_message_id_for_source(
                    &projection,
                    canonical_user_input_seq,
                )?;
            let projected_state =
                crate::agent_stream::provider_v1_sink::provider_projection_controller_state(
                    &projection,
                )?;
            let runtime_state = workspace_snapshot
                .session_runtime_state
                .entry(session_id)
                .or_default();
            // V1 owns the complete logical history for every newly admitted root or queued successor.
            // V1 持有每个新准入根输入或排队后继的完整逻辑历史。
            runtime_state.conversation_state = projected_state;
            prompt_turn_begin_input.existing_canonical_user_message_id =
                Some(canonical_user_message_id);
        }
        // The retained submission reservation serializes this admission without holding outer locks over I/O.
        // 保留的提交预留串行化本次准入，无须跨 I/O 持有外层锁。
        let mut hook_runtime = entry_agent_runtime
            .lock_conversation_hooks()
            .map_err(|error| format!("会话 hook runtime 加锁失败：{error}"))?;
        // Exact process inputs retain their individual canonical identities even when Provider preparation uses a batch prompt.
        // 即使 Provider 准备使用批次提示，精确过程输入仍保留各自规范身份。
        let precheck_start = if let ExactPendingPromptDispatch::PendingInTurn {
            authority, ..
        } = &exact_dispatch
        {
            let inputs = authority.inputs.iter().map(|input|
                vulcan_agent_runtime::agent_runtime::conversation_hooks::CanonicalInTurnHookInput {
                    client_action_id: input.client_action_id.clone(), input_id: input.input_id,
                    input_seq: input.input_seq, text: input.text.clone(),
                }).collect::<Vec<_>>();
            vulcan_agent_runtime::workbench_prompt_turn::begin_shared_prompt_turn_with_canonical_inputs(
                &mut workspace_snapshot, &mut hook_runtime, prompt_turn_begin_input, &inputs,
            )?
        } else {
            begin_shared_prompt_turn(
                &mut workspace_snapshot,
                &mut hook_runtime,
                prompt_turn_begin_input,
            )?
        };
        drop(hook_runtime);
        let handle = registered_session_v1_handle_from_state(state, session_id)?;
        crate::session_business_hook::publish_prepared_business_hook(
            handle.as_ref(),
            &entry_agent_runtime,
            &precheck_start,
        )?;
        // Register only after the exact prepared Hook has durable ownership.
        // 仅在精确准备的 Hook 已具有可靠归属后注册。
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        if matches!(
            &exact_dispatch,
            ExactPendingPromptDispatch::SubagentReturn { .. }
                | ExactPendingPromptDispatch::Collaboration { .. }
                | ExactPendingPromptDispatch::AgentTask { .. }
        ) {
            start_reserved_webui_session_kernel_turn(
                &mut guard,
                session_id,
                &precheck_start.preparation_turn_id,
                ActiveTurnKind::MailboxTriggered,
                pending_prompt_dispatch_authorization.reservation_id,
            )?;
        } else {
            start_reserved_webui_session_kernel_user_turn(
                &mut guard,
                session_id,
                &precheck_start.preparation_turn_id,
                pending_prompt_dispatch_authorization.reservation_id,
            )?;
        }
        precheck_start
    };
    if matches!(
        &exact_dispatch,
        ExactPendingPromptDispatch::AgentTask { .. }
    ) {
        return finish_target_prompt_preparation(
            app,
            state,
            controller_runtime,
            &entry_agent_runtime,
            workspace_snapshot,
            precheck_start,
            entry.source,
        )
        .await;
    }
    if precheck_start.vmm_runtime_ready {
        refresh_exact_vmm_hook_session_projection(
            &entry_agent_runtime.vmm_hook_runtime,
            &workspace_snapshot,
            session_id,
        )?;
    }
    let precheck_plan = if entry.source.runs_user_prompt_precheck() {
        webui_shared_prompt_precheck_plan(
            &entry_agent_runtime,
            &workspace_snapshot,
            session_id,
            &workspace_path,
            precheck_start.vmm_precheck_ready,
        )?
    } else {
        SharedPromptPrecheckPlan {
            has_precheck_hooks: false,
            should_skip_precheck: true,
            progress_labels: Vec::new(),
        }
    };
    commit_workspace_snapshot_and_emit_session_phase_delta(
        app,
        state,
        workspace_snapshot,
        session_id,
        SessionRunPhase::Preparing,
    )?;
    // The exact V1 queued, Goal, subagent, or in-turn authority was durable before this process-local dispatch adapter ran.
    // 精确 V1 队列、Goal、子代理或回合内权威事实已在当前进程内分派适配器运行前可靠落盘。
    start_webui_precheck_or_continue(
        app,
        state,
        controller_runtime,
        &entry_agent_runtime,
        precheck_start,
        precheck_plan,
    )?;
    webui_trace(
        "pending_prompt_prepare_started",
        format!("session={session_id}"),
    );
    Ok(true)
}

#[cfg(test)]
mod tests {
    //! Exact Goal-continuation admission adapter tests.
    //! 精确 Goal 续跑准入适配测试。

    use super::*;
    use vulcan_session_core::v1::ids::{Seq, TurnId};

    /// Verifies two open Turns of the same Goal can never share one dispatch identity.
    /// 验证同一 Goal 的两个开放回合绝不会共享同一个分发身份。
    #[test]
    fn exact_goal_adapter_identity_includes_the_v1_turn() {
        let first = SessionGoalDispatchAuthority {
            generation: 1,
            goal_id: String::from("goal-v1-8"),
            turn_id: TurnId::try_new(11).expect("first Turn id should be valid"),
            context_seq: Seq::try_new(10).expect("first context seq should be valid"),
        };
        let second = SessionGoalDispatchAuthority {
            generation: 1,
            goal_id: first.goal_id.clone(),
            turn_id: TurnId::try_new(12).expect("second Turn id should be valid"),
            context_seq: Seq::try_new(11).expect("second context seq should be valid"),
        };

        let first_entry = exact_goal_continuation_prompt_entry(&first, String::from("1000"));
        let second_entry = exact_goal_continuation_prompt_entry(&second, String::from("1001"));

        assert_ne!(first_entry.id, second_entry.id);
        assert_eq!(first_entry.task_id, second_entry.task_id);
        assert_eq!(
            first_entry.source,
            PendingSessionPromptSource::GoalContinuation
        );
        assert!(first_entry.panel_entry_id.is_none());
    }

    /// Verifies root and queued successor admissions both expose their authoritative canonical user input.
    /// 验证根输入与排队后继准入都会暴露其权威规范用户输入。
    #[test]
    fn user_turn_dispatches_expose_canonical_user_input_sequence() {
        // input_seq is the single durable source shared by both user-turn dispatch variants.
        // input_seq 是两个用户回合分发变体共享的唯一可靠来源。
        let input_seq = Seq::try_new(7).expect("input seq should be valid");
        // entry supplies the process-local adapter that never owns canonical identity.
        // entry 提供绝不持有规范身份的进程内适配对象。
        let entry = PendingSessionPromptEntry {
            id: String::from("queued-entry"),
            prompt: String::from("queued prompt"),
            hidden_model_guidance: None,
            preview: String::from("queued prompt"),
            attachments: Vec::new(),
            model_reference: None,
            runtime_options: PendingPromptRuntimeOptions::default(),
            source: PendingSessionPromptSource::UserFollowup,
            task_id: None,
            panel_entry_id: None,
            created_at: String::from("1000"),
        };
        // queued models a durable successor promoted after a prior turn becomes terminal.
        // queued 模拟上一回合终态后提升的可靠后继输入。
        let queued = ExactPendingPromptDispatch::Queued {
            authority: SessionQueuedDispatchAuthority {
                turn_id: TurnId::try_new(9).expect("queued Turn id should be valid"),
                input_id: PendingInputId::try_new(3).expect("queued input id should be valid"),
                client_action_id: ClientActionId::try_new("queued-action")
                    .expect("queued action id should be valid"),
                input_seq,
                text: String::from("queued prompt"),
                resource_ids: Vec::new(),
            },
            entry: entry.clone(),
        };
        // root models a newly submitted or edited authoritative root input.
        // root 模拟新提交或编辑后的权威根输入。
        let root = ExactPendingPromptDispatch::Root {
            authority: SessionUserRootDispatchAuthority {
                turn_id: TurnId::try_new(10).expect("root Turn id should be valid"),
                input_id: PendingInputId::try_new(4).expect("root input id should be valid"),
                client_action_id: ClientActionId::try_new("root-action")
                    .expect("root action id should be valid"),
                input_seq,
                text: String::from("root prompt"),
                resource_ids: Vec::new(),
            },
            entry,
        };

        assert_eq!(queued.canonical_user_input_seq(), Some(input_seq));
        assert_eq!(root.canonical_user_input_seq(), Some(input_seq));
    }
}
