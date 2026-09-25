//! V1 typed collaboration facts and recovery state.
//! V1 强类型协作事实与恢复状态。
//!
//! The source session owns questions and sender-side delivery progress. The
//! target session owns accepted, projected, and failed inbound deliveries.
//! Questions never use mailbox text or model summaries as authoritative state.
//! 来源会话拥有问题与发送侧投递进度；目标会话拥有已接纳、已投影和失败的入站投递。
//! 问题绝不把 mailbox 文本或模型摘要当作权威状态。

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

use crate::v1::errors::DomainError;
use crate::v1::ids::{
    ClientActionId, JS_SAFE_INTEGER_MAX, PendingInputId, Seq, SubagentTaskId, ToolCallId, TurnId,
};

mod child_result_projection;
mod child_results;
mod continuation;
mod task_messages;
pub use child_result_projection::*;
pub use child_results::*;
pub use continuation::*;
pub use task_messages::*;
mod task_result_consumption;
mod task_result_delivery;
pub use task_result_consumption::*;
mod task_result_projection;
mod task_results;
pub use task_result_delivery::*;
pub use task_results::*;

/// Current typed collaboration metadata version.
/// 当前强类型协作 metadata 版本。
pub const COLLABORATION_METADATA_VERSION: u32 = 1;

/// Maximum UTF-8 byte length of one collaboration identity.
/// 一个协作身份允许的最大 UTF-8 字节长度。
pub const COLLABORATION_ID_MAX_BYTES: usize = 512;

/// Maximum UTF-8 byte length of one persisted collaboration body.
/// 一条持久化协作正文允许的最大 UTF-8 字节长度。
pub const COLLABORATION_BODY_MAX_BYTES: usize = 64 * 1024;

/// Maximum UTF-8 byte length of one persisted collaboration reason.
/// 一条持久化协作原因允许的最大 UTF-8 字节长度。
pub const COLLABORATION_REASON_MAX_BYTES: usize = 4 * 1024;

macro_rules! define_collaboration_id {
    ($(#[$meta:meta])* $name:ident, $field:literal) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Constructs one exact bounded collaboration identity.
            /// 构造一个精确且有界的协作身份。
            pub fn try_new(value: impl Into<String>) -> Result<Self, DomainError> {
                let value = value.into();
                if value.trim().is_empty()
                    || value.trim() != value
                    || value.len() > COLLABORATION_ID_MAX_BYTES
                {
                    return Err(DomainError::InvalidStatistics { field: $field });
                }
                Ok(Self(value))
            }

            /// Returns the exact persisted identity without normalization.
            /// 返回未经归一化的精确持久化身份。
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl<'de> Deserialize<'de> for $name {
            /// Deserializes and validates one persisted collaboration identity.
            /// 反序列化并校验一个持久化协作身份。
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::try_new(value).map_err(D::Error::custom)
            }
        }
    };
}

define_collaboration_id!(
    /// Stable idempotency identity of one collaboration operation.
    /// 一次协作操作的稳定幂等身份。
    CollaborationOperationId,
    "collaboration_operation_id"
);

define_collaboration_id!(
    /// Stable identity of one question transaction.
    /// 一次问答事务的稳定身份。
    CollaborationQuestionId,
    "collaboration_question_id"
);

define_collaboration_id!(
    /// Stable identity of one delivery attempt.
    /// 一次投递尝试的稳定身份。
    CollaborationDeliveryId,
    "collaboration_delivery_id"
);

define_collaboration_id!(
    /// Stable identity of one one-way notification transaction.
    /// 一次单向通知事务的稳定身份。
    CollaborationNotificationId,
    "collaboration_notification_id"
);

define_collaboration_id!(
    /// Stable identity of one user-managed collaboration grant.
    /// 一条用户管理协作准入记录的稳定身份。
    CollaborationGrantId,
    "collaboration_grant_id"
);

define_collaboration_id!(
    /// Stable identity of one temporary question execution.
    /// 一次临时问题执行的稳定身份。
    CollaborationExecutionId,
    "collaboration_execution_id"
);

/// Logical collaboration message kind used by the future projection layer.
/// 供后续投影层使用的逻辑协作消息类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollaborationMessageKind {
    /// Start a separate business task.
    /// 启动一个独立业务任务。
    TaskStart,
    /// Steer an existing active task at its next legal boundary.
    /// 在下一个合法边界引导既有活动任务。
    TaskSteer,
    /// Ask a non-terminal temporary question.
    /// 发起一个非终态临时问题。
    Question,
    /// Deliver a question answer to its source execution.
    /// 向问题源执行交付答案。
    QuestionAnswer,
    /// Cancel one pending question.
    /// 撤销一个仍待结算的问题。
    QuestionCancelled,
    /// Add one dynamic context node.
    /// 增加一个动态上下文节点。
    ContextUpdate,
    /// Expire one previously delivered context node.
    /// 使一条已投影上下文节点失效。
    ContextExpired,
}

/// Current business settlement state of one question.
/// 一个问题当前的业务结算状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollaborationQuestionState {
    /// The question has no winning answer or cancellation yet.
    /// 问题尚未产生胜出的回答或撤销。
    Pending,
    /// The first authorized answer settled the question.
    /// 首个获授权回答已结算问题。
    Answered,
    /// The first authorized cancellation settled the question.
    /// 首个获授权撤销已结算问题。
    Cancelled,
}

/// Independent progress of one delivery record.
/// 一条投递记录的独立进度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollaborationDeliveryState {
    /// The source recorded the question and delivery obligation.
    /// 来源已记录问题与投递义务。
    Recorded,
    /// The target durably accepted the delivery.
    /// 目标已可靠接纳投递。
    Accepted,
    /// The target projected the delivery into a legal request boundary.
    /// 目标已把投递投影到合法请求边界。
    Projected,
    /// The target could not deliver the question for the recorded reason.
    /// 目标因记录的原因无法投递问题。
    Failed,
}

/// Independent lifecycle of one typed one-way notification.
/// 一条强类型单向通知的独立生命周期。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollaborationNotificationState {
    /// The source durably recorded the notification in its outbox.
    /// 来源已将通知可靠记录到出站箱。
    Recorded,
    /// The target durably accepted the notification for a later legal boundary.
    /// 目标已可靠接纳通知，等待后续合法边界。
    Accepted,
    /// The target projected the notification into a real provider input event.
    /// 目标已把通知投影为真实 Provider 输入事件。
    Projected,
    /// The target recorded an explicit delivery failure.
    /// 目标已记录明确的投递失败。
    Failed,
}

/// Terminal settlement state acknowledged by a target after source settlement wins.
/// 鐩爣鍦ㄦ潵婧愮粨绠楄儉璇佸悗鎸佷箙鍖栫殑缁堟缁撶畻鐘舵€併€?
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollaborationTargetSettlementState {
    /// The source question was answered by the authorized target.
    /// 鏉ユ簮闂宸茬敱鑾锋巿鏉冪殑鐩爣鍥炵瓟銆?
    Answered,
    /// The source question was cancelled by the authorized source.
    /// 鏉ユ簮闂宸茬敱鑾锋巿鏉冩潵婧愭挙閿€銆?
    Cancelled,
}

/// Explicit reason for a failed delivery.
/// 投递失败的明确原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollaborationDeliveryFailureReason {
    /// The target session is stopped and must not be revived.
    /// 目标会话已停止且不得被复活。
    TargetStopped,
    /// The target locator no longer resolves to a live document.
    /// 目标定位已无法解析到活动文档。
    TargetMissing,
    /// The accepted target work lost its physical execution and cannot produce a result.
    /// 已接纳目标工作丢失物理执行，无法再产出结果。
    TargetExecutionLost,
    /// The source document could not be read or committed.
    /// 来源文档无法读取或可靠提交。
    SourceUnavailable,
    /// The authorization needed for the delivery was revoked.
    /// 投递所需授权已撤销。
    AuthorizationRevoked,
    /// The target generation no longer matches the accepted delivery.
    /// 目标代际已不再匹配已接纳投递。
    GenerationMismatch,
    /// The accepted target binding is no longer valid.
    /// 已接纳的目标绑定已失效。
    InvalidBinding,
    /// The host has no more specific stable failure classification.
    /// 宿主没有更具体的稳定失败分类。
    Unknown,
}

/// Route used to deliver an already answered question.
/// 已回答问题使用的交付路径。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollaborationAnswerDeliveryKind {
    /// The answer was returned by a real wait tool result.
    /// 答案通过真实等待工具结果返回。
    WaitTool,
    /// The answer was appended as a new projected User event.
    /// 答案作为新的投影 User 事件追加。
    UserEvent,
}

/// Independent delivery state of one answer.
/// 一条答案的独立交付状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollaborationAnswerDeliveryState {
    /// The answer is authoritative but has not entered the source execution.
    /// 答案已成为权威事实但尚未进入来源执行。
    Undelivered,
    /// The answer has been delivered at the recorded revision.
    /// 答案已在记录的修订处完成交付。
    Delivered,
    /// Delivery was denied before the answer body entered the source model context.
    /// 答案正文进入来源模型上下文前，交付被拒绝。
    Failed,
}

/// Lifecycle state of one child-task reservation.
/// 一条子任务预留的生命周期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollaborationChildTaskState {
    /// The parent reserved the task but execution has not started.
    /// 父回合已预留任务但执行尚未开始。
    Reserved,
    /// The reserved child task is executing.
    /// 已预留的子任务正在执行。
    Active,
    /// The child task reached an explicit terminal outcome.
    /// 子任务已到达明确终态。
    Terminal,
}

/// Terminal outcome recorded for one child task.
/// 一条子任务记录的终态结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollaborationChildTaskTerminalOutcome {
    /// Child task completed successfully.
    /// 子任务成功完成。
    Completed,
    /// Child task failed.
    /// 子任务失败。
    Failed,
    /// Child task was stopped by the host.
    /// 子任务被宿主停止。
    Stopped,
    /// Child task was cancelled.
    /// 子任务被取消。
    Cancelled,
}

/// Distinct host run-control state used by P03.
/// P03 使用的彼此区分的宿主运行控制状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollaborationRunControl {
    /// Main and child execution are available.
    /// 主执行与子执行均可用。
    Running,
    /// The business task completed naturally and the session is idle.
    /// 业务任务自然完成且会话空闲。
    NaturallyIdle,
    /// The target is stopped and cannot accept new work.
    /// 目标已停止且不能接纳新工作。
    Stopped,
    /// Main execution is stopped while children remain available.
    /// 主执行已停止但子执行仍可用。
    MainOnly,
    /// Main and child execution were stopped as one tree.
    /// 主执行与子执行作为一棵树共同停止。
    MainAndSubagents,
}

/// Purpose of one collaboration-related physical execution.
/// 一次协作相关物理执行的用途。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollaborationExecutionPurpose {
    /// Ordinary business task execution.
    /// 普通业务任务执行。
    NormalTask,
    /// Answering a question while the original turn remains associated.
    /// 原回合仍保持关联时执行问题回答。
    InTurnQuestion,
    /// Answering one question after the previous task became naturally idle.
    /// 之前业务任务自然空闲后执行一次问题回答。
    NaturallyIdleQuestion,
}

/// Lifecycle state of one temporary question execution.
/// 一次临时问题执行的生命周期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollaborationQuestionExecutionState {
    /// The dedicated question execution is still open.
    /// 专用问题执行仍然开放。
    Running,
    /// The dedicated question execution ended without closing the old task.
    /// 专用问题执行已结束且没有关闭旧业务任务。
    Completed,
}

/// Outcome of one dedicated question execution.
/// 一次专用问题执行的结束结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollaborationQuestionExecutionOutcome {
    /// A reply was submitted or the question was already cancelled.
    /// 已提交回答或问题已经被撤销。
    Settled,
    /// Execution ended normally without a reply and without treating the question as a failure.
    /// 执行正常结束但没有回答，且不把问题视为故障。
    Unanswered,
    /// Execution ended without settling the question.
    /// 执行结束但没有结算问题。
    Failed,
}

/// Action allowed by one recorded collaboration grant.
/// 一条协作准入记录允许的动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollaborationGrantAction {
    /// Read a bounded target summary or record.
    /// 读取有界目标摘要或记录。
    Read,
    /// Ask the target a temporary question.
    /// 向目标发起临时问题。
    Ask,
    /// Send a one-way notification without creating a question or task.
    /// 发送单向通知，不创建问题或任务。
    Notify,
    /// Steer an existing target task.
    /// 引导目标既有任务。
    Steer,
    /// Start a task on an existing target session.
    /// 在既有目标会话启动任务。
    Start,
    /// Reply to a question addressed to the target.
    /// 回答发给目标的问题。
    Reply,
}

/// Authority kind that created a collaboration grant.
/// 创建协作授权记录的权威主体类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollaborationGrantAuthority {
    /// The local host user authority admitted the grant.
    /// 本地宿主用户权威准入了该授权。
    LocalUser,
}

/// Stable host-owned identity of one collaboration participant.
/// 一名协作参与者的宿主权威稳定身份。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationParticipant {
    /// Actual session directory identity.
    /// 真实会话目录身份。
    pub session_id: String,
    /// Root-session locator used for embedded child sessions.
    /// 用于嵌入式子会话的根会话定位。
    pub root_session_id: String,
    /// Host-verified project identity.
    /// 宿主验证过的项目身份。
    pub project_id: String,
    /// Event-tree generation at which this participant was captured.
    /// 捕获该参与者时的事件树代际。
    pub segment_generation: u64,
    /// Real logical turn associated with the participant, when one exists.
    /// 参与者关联的真实逻辑回合（如有）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<TurnId>,
    /// Real task identity when this participant is a child task.
    /// 参与者作为子任务时使用的真实任务身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<SubagentTaskId>,
}

impl CollaborationParticipant {
    /// Constructs and validates one participant identity.
    /// 构造并校验一名参与者身份。
    pub fn new(
        session_id: impl Into<String>,
        root_session_id: impl Into<String>,
        project_id: impl Into<String>,
        segment_generation: u64,
        turn_id: Option<TurnId>,
        task_id: Option<SubagentTaskId>,
    ) -> Result<Self, DomainError> {
        let participant = Self {
            session_id: session_id.into(),
            root_session_id: root_session_id.into(),
            project_id: project_id.into(),
            segment_generation,
            turn_id,
            task_id,
        };
        participant.validate("participant", false)?;
        Ok(participant)
    }

    /// Validates exact fields without inferring a missing turn or task.
    /// 校验精确字段，不推测缺失的回合或任务。
    pub fn validate(&self, field: &'static str, require_turn: bool) -> Result<(), DomainError> {
        for value in [&self.session_id, &self.root_session_id, &self.project_id] {
            if value.trim().is_empty() || value.trim() != value {
                return Err(DomainError::InvalidStatistics { field });
            }
        }
        if !(1..=JS_SAFE_INTEGER_MAX).contains(&self.segment_generation) {
            return Err(DomainError::InvalidStatistics { field });
        }
        if require_turn && self.turn_id.is_none() {
            return Err(DomainError::InvalidStatistics { field });
        }
        Ok(())
    }

    /// Returns whether two participants identify exactly the same execution.
    /// 返回两名参与者是否精确指向同一执行。
    pub fn same_execution(&self, other: &Self) -> bool {
        self == other
    }
}

/// Answer record stored by the source question authority.
/// 来源问题权威持有的答案记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationAnswerRecord {
    /// Operation that won the question settlement.
    /// 胜出问题结算的操作。
    pub operation_id: CollaborationOperationId,
    /// Authorized target participant that submitted the answer.
    /// 提交答案的获授权目标参与者。
    pub responder: CollaborationParticipant,
    /// Exact answer body, including explicit inability to answer.
    /// 精确答案正文，包括明确无法回答的结果。
    pub body: String,
    /// Answer settlement time in Unix milliseconds.
    /// 答案结算时间（Unix 毫秒）。
    pub settled_at_ms: i64,
    /// Independent source-side answer delivery record.
    /// 独立的来源侧答案交付记录。
    pub delivery: CollaborationAnswerDeliveryRecord,
}

/// Independent source-side delivery revision of one answer.
/// 一条答案的独立来源侧交付修订。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationAnswerDeliveryRecord {
    /// Current answer delivery state.
    /// 当前答案交付状态。
    pub state: CollaborationAnswerDeliveryState,
    /// Stable reason when the answer was denied before model delivery.
    /// 答案进入模型前被拒绝时的稳定原因。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<CollaborationDeliveryFailureReason>,
    /// Monotonic delivery revision, independent from question settlement.
    /// 独立于问题结算状态的单调交付修订。
    pub delivery_revision: u64,
    /// Last selected delivery path.
    /// 最近一次选择的交付路径。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<CollaborationAnswerDeliveryKind>,
    /// Physical projected event seq when User-event delivery was used.
    /// 使用 User 事件交付时对应的物理投影事件 seq。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projected_seq: Option<Seq>,
    /// Stable UserInput identity when the answer uses a User event.
    /// 绛旀浣跨敤 User 浜嬩欢浜や粯鏃剁殑绋冲畾 UserInput 韬唤銆?
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projected_input_id: Option<PendingInputId>,
    /// Stable tool call identity when the answer uses a tool result.
    /// 绛旀浣跨敤宸ュ叿缁撴灉浜や粯鏃剁殑绋冲畾宸ュ叿璋冪敤韬唤銆?
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<ToolCallId>,
    /// Physical ToolCall event seq carrying the completed result.
    /// 鎼哄甫宸插畬鎴愮粨鏋滅殑鐗╃悊 ToolCall 浜嬩欢 seq銆?
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_result_seq: Option<Seq>,
    /// Operation that last changed this delivery record.
    /// 最近一次修改交付记录的操作。
    pub last_operation_id: CollaborationOperationId,
    /// Last delivery update time in Unix milliseconds.
    /// 最近一次交付更新时间（Unix 毫秒）。
    pub updated_at_ms: i64,
}

/// Source-owned question record and its immutable identity.
/// 来源拥有的问题记录及其不可变身份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationQuestionRecord {
    /// Operation that created the question.
    /// 创建问题的操作。
    pub operation_id: CollaborationOperationId,
    /// Stable question identity.
    /// 稳定问题身份。
    pub question_id: CollaborationQuestionId,
    /// Stable delivery identity created with the question.
    /// 与问题一同创建的稳定投递身份。
    pub delivery_id: CollaborationDeliveryId,
    /// Source participant that owns the question.
    /// 拥有问题的来源参与者。
    pub source: CollaborationParticipant,
    /// Target participant that is allowed to answer.
    /// 被允许回答问题的目标参与者。
    pub target: CollaborationParticipant,
    /// Exact untrusted body supplied by the source model.
    /// 来源模型提供的精确不可信正文。
    pub body: String,
    /// Execution purpose requested for the target.
    /// 请求目标使用的执行用途。
    pub execution_purpose: CollaborationExecutionPurpose,
    /// Current question settlement state.
    /// 当前问题结算状态。
    pub state: CollaborationQuestionState,
    /// Answer record after the first winning settlement.
    /// 首个胜出结算后的答案记录。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<CollaborationAnswerRecord>,
    /// Cancellation operation and reason after cancellation wins.
    /// 撤销胜出后的操作与原因。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancellation: Option<CollaborationCancellationRecord>,
    /// Creation time in Unix milliseconds.
    /// 创建时间（Unix 毫秒）。
    pub created_at_ms: i64,
}

/// Source-owned cancellation record.
/// 来源拥有的撤销记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationCancellationRecord {
    /// Operation that won cancellation settlement.
    /// 胜出撤销结算的操作。
    pub operation_id: CollaborationOperationId,
    /// Authorized source participant that cancelled the question.
    /// 撤销问题的获授权来源参与者。
    pub requester: CollaborationParticipant,
    /// Explicit redacted cancellation reason.
    /// 明确的脱敏撤销原因。
    pub reason: String,
    /// Cancellation time in Unix milliseconds.
    /// 撤销时间（Unix 毫秒）。
    pub cancelled_at_ms: i64,
}

/// Target-side terminal settlement acknowledgement for one accepted delivery.
/// 鐩爣渚у鏉ユ簮缁撶畻鐨勭粓鎬佷氦浠樼‘璁よ褰曘€?
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationInboundSettlementRecord {
    /// Terminal settlement selected by the source authority.
    /// 鏉ユ簮鏉冨▉閫夋嫨鐨勭粓鎬佺粨绠楃姸鎬併€?
    pub state: CollaborationTargetSettlementState,
    /// Source event seq that established the winning settlement.
    /// 寤虹珛鑳滃嚭缁撶畻鐨勬潵婧愪簨浠?seq銆?
    pub source_event_seq: Seq,
    /// Source operation that established the winning settlement.
    /// 寤虹珛鑳滃嚭缁撶畻鐨勬潵婧愭搷浣滄搷浣滅紪鍙枫€?
    pub source_operation_id: CollaborationOperationId,
    /// Target operation that recorded this acknowledgement.
    /// 璁板綍璇ユ帴鏀ょ‘璁ょ殑鐩爣鎿嶄綔缂栧彿銆?
    pub target_operation_id: CollaborationOperationId,
    /// Target acknowledgement time in Unix milliseconds.
    /// 鐩爣纭鏃堕棿锛圲nix 姣锛夈€?
    pub acknowledged_at_ms: i64,
}

/// Source-side durable acknowledgement that the target observed settlement.
/// 鏉ユ簮渚у皢鐩爣宸插畬鎴愮粨绠楃殑鎯呭喌鎸佷箙鍖栦负纭璁板綍銆?
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationTargetSettlementAck {
    /// Terminal settlement observed on the target.
    /// 鐩爣宸茶瑙夌殑缁堟€佺粨绠楃姸鎬併€?
    pub state: CollaborationTargetSettlementState,
    /// Source event seq that established the winning settlement.
    /// 寤虹珛鑳滃嚭缁撶畻鐨勬潵婧愪簨浠?seq銆?
    pub source_event_seq: Seq,
    /// Target event seq that recorded the acknowledgement.
    /// 璁板綍鐩爣纭鐨勭洰鏍囦簨浠?seq銆?
    pub target_event_seq: Seq,
    /// Target document revision at acknowledgement.
    /// 鐩爣纭鏃剁殑鏂囨。淇鍙枫€?
    pub target_revision: u64,
    /// Source acknowledgement time in Unix milliseconds.
    /// 鏉ユ簮淇濆瓨纭鐨勬椂闂达紙Unix 姣锛夈€?
    pub acknowledged_at_ms: i64,
}

/// Source-owned outbound delivery progress.
/// 来源拥有的出站投递进度。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationDeliveryRecord {
    /// Stable delivery identity.
    /// 稳定投递身份。
    pub delivery_id: CollaborationDeliveryId,
    /// Related source-owned question.
    /// 关联的来源问题。
    pub question_id: CollaborationQuestionId,
    /// Source operation that created the delivery.
    /// 创建投递的来源操作。
    pub operation_id: CollaborationOperationId,
    /// Source participant locator.
    /// 来源参与者定位。
    pub source: CollaborationParticipant,
    /// Target participant locator.
    /// 目标参与者定位。
    pub target: CollaborationParticipant,
    /// Current sender-side delivery state.
    /// 当前发送侧投递状态。
    pub state: CollaborationDeliveryState,
    /// Stable failure reason, when delivery failed.
    /// 投递失败时的稳定原因。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<CollaborationDeliveryFailureReason>,
    /// Target document revision acknowledged by the sender.
    /// 发送侧确认的目标文档修订。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_revision: Option<u64>,
    /// Target event seq used for projection acknowledgement.
    /// 投影确认使用的目标事件 seq。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_event_seq: Option<Seq>,
    /// Durable target terminal settlement acknowledgement, if observed.
    /// 宸茶瑙夌洰鏍囩粓鎬佺粨绠楁椂淇濆瓨鐨勬寔涔呯‘璁ゃ€?
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settlement_ack: Option<CollaborationTargetSettlementAck>,
    /// Last sender-side update time in Unix milliseconds.
    /// 最近一次发送侧更新时间（Unix 毫秒）。
    pub updated_at_ms: i64,
}

/// Target-owned accepted and projected delivery record.
/// 目标拥有的已接纳与已投影投递记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationInboundDeliveryRecord {
    /// Stable delivery identity.
    /// 稳定投递身份。
    pub delivery_id: CollaborationDeliveryId,
    /// Related source question identity.
    /// 关联的来源问题身份。
    pub question_id: CollaborationQuestionId,
    /// Source operation identity retained for exact locator checks.
    /// 为精确定位校验保留的来源操作身份。
    pub operation_id: CollaborationOperationId,
    /// Source participant including root-session locator.
    /// 包含根会话定位的来源参与者。
    pub source: CollaborationParticipant,
    /// Target participant bound at acceptance.
    /// 接纳时绑定的目标参与者。
    pub target: CollaborationParticipant,
    /// Optional target turn that will receive the projected message.
    /// 将接收投影消息的可选目标回合。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bound_turn_id: Option<TurnId>,
    /// Intended execution purpose.
    /// 计划使用的执行用途。
    pub execution_purpose: CollaborationExecutionPurpose,
    /// Current target-side delivery state.
    /// 当前目标侧投递状态。
    pub state: CollaborationDeliveryState,
    /// Stable failure reason, when target delivery failed.
    /// 目标投递失败时的稳定原因。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<CollaborationDeliveryFailureReason>,
    /// Target document revision of acceptance or projection.
    /// 目标接纳或投影时的文档修订。
    pub target_revision: u64,
    /// Projected collaboration event seq, if projection happened.
    /// 已经投影时对应的协作事件 seq。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projected_seq: Option<Seq>,
    /// Stable UserInput identity proved by the projection event.
    /// 投影事件证明的稳定流程输入标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projected_input_id: Option<PendingInputId>,
    /// Logical turn carried by the projection UserInput event.
    /// 投影 UserInput 事件携带的逻辑回合。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projected_turn_id: Option<TurnId>,
    /// Target terminal settlement acknowledgement, independent from projection.
    /// 鐩爣缁堟€佺粨绠楃‘璁や笌鎶曞奖鐘舵€佸垎绂诲瓨鍌ㄣ€?
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settlement: Option<CollaborationInboundSettlementRecord>,
    /// Last target-side update time in Unix milliseconds.
    /// 最近一次目标侧更新时间（Unix 毫秒）。
    pub updated_at_ms: i64,
}

/// Source-owned outbox record for one one-way notification.
/// 一条单向通知的来源出站箱记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationNotificationRecord {
    /// Stable notification identity.
    /// 稳定通知身份。
    pub notification_id: CollaborationNotificationId,
    /// Stable delivery identity shared by source and target records.
    /// 来源与目标记录共享的稳定投递身份。
    pub delivery_id: CollaborationDeliveryId,
    /// Source operation that created the notification.
    /// 创建通知的来源操作身份。
    pub operation_id: CollaborationOperationId,
    /// Source participant locator.
    /// 来源参与者定位。
    pub source: CollaborationParticipant,
    /// Target participant locator.
    /// 目标参与者定位。
    pub target: CollaborationParticipant,
    /// Exact notification body retained by the source authority.
    /// 来源权威保留的精确通知正文。
    pub body: String,
    /// Current source-side notification state.
    /// 当前来源侧通知状态。
    pub state: CollaborationNotificationState,
    /// Stable failure reason, when delivery failed.
    /// 投递失败时的稳定失败原因。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<CollaborationDeliveryFailureReason>,
    /// Target document revision observed by the source.
    /// 来源观察到的目标文档修订。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_revision: Option<u64>,
    /// Target event sequence observed by the source.
    /// 来源观察到的目标事件序号。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_event_seq: Option<Seq>,
    /// Notification creation time in Unix milliseconds.
    /// 通知创建时间（Unix 毫秒）。
    pub created_at_ms: i64,
    /// Last source-side update time in Unix milliseconds.
    /// 最近一次来源侧更新时间（Unix 毫秒）。
    pub updated_at_ms: i64,
}

/// Target-owned acceptance and projection record for one notification.
/// 一条通知的目标接纳与投影记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationInboundNotificationRecord {
    /// Stable notification identity copied from the source outbox.
    /// 从来源出站箱复制的稳定通知身份。
    pub notification_id: CollaborationNotificationId,
    /// Stable delivery identity copied from the source outbox.
    /// 从来源出站箱复制的稳定投递身份。
    pub delivery_id: CollaborationDeliveryId,
    /// Source operation identity copied from the source outbox.
    /// 从来源出站箱复制的来源操作身份。
    pub source_operation_id: CollaborationOperationId,
    /// Target operation that accepted or projected the notification.
    /// 接纳或投影通知的目标操作身份。
    pub operation_id: CollaborationOperationId,
    /// Source participant locator copied from the source authority.
    /// 从来源权威复制的来源参与者定位。
    pub source: CollaborationParticipant,
    /// Target participant captured at acceptance.
    /// 接纳时捕获的目标参与者定位。
    pub target: CollaborationParticipant,
    /// Exact notification body retained by the target authority.
    /// 目标权威保留的精确通知正文。
    pub body: String,
    /// Current target-side notification state.
    /// 当前目标侧通知状态。
    pub state: CollaborationNotificationState,
    /// Stable failure reason, when projection failed.
    /// 投影失败时的稳定失败原因。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<CollaborationDeliveryFailureReason>,
    /// Target document revision of the latest accepted or projected state.
    /// 最近一次接纳或投影状态对应的目标文档修订。
    pub target_revision: u64,
    /// Physical event sequence proving projection.
    /// 证明投影发生的物理事件序号。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projected_seq: Option<Seq>,
    /// Stable UserInput identity proved by the projection event.
    /// 投影事件证明的稳定 UserInput 身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projected_input_id: Option<PendingInputId>,
    /// Logical turn carried by the projection event.
    /// 投影事件携带的逻辑回合。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projected_turn_id: Option<TurnId>,
    /// Acceptance time in Unix milliseconds.
    /// 接纳时间（Unix 毫秒）。
    pub accepted_at_ms: i64,
    /// Last target-side update time in Unix milliseconds.
    /// 最近一次目标侧更新时间（Unix 毫秒）。
    pub updated_at_ms: i64,
}

/// Parent-bound child task reservation and terminal state.
/// 按父回合绑定的子任务预留及终态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationChildTaskRecord {
    /// Original authenticated reservation operation, retained after its physical API ends.
    /// 原始已认证预留操作，在其物理 API 结束后保留。
    pub reservation_operation_id: CollaborationOperationId,
    /// Canonical source ToolCall name used to verify the original invocation fingerprint.
    /// 用于校验原始调用指纹的规范来源 ToolCall 名称。
    pub source_tool_name: String,
    /// Immutable execution identity, distinct from the reusable child session.
    /// 不可变执行身份，与可复用的子会话分离。
    pub execution_id: CollaborationExecutionId,
    /// Real child task identity.
    /// 真实子任务身份。
    pub task_id: SubagentTaskId,
    /// Parent logical turn that owns the completion obligation.
    /// 拥有完成义务的父逻辑回合。
    pub parent_turn_id: TurnId,
    /// Parent document generation captured at reservation time.
    /// 预留时捕获的父文档代次。
    pub parent_segment_generation: u64,
    /// Child participant locator.
    /// 子参与者定位。
    pub child: CollaborationParticipant,
    /// Current child-task state.
    /// 当前子任务状态。
    pub state: CollaborationChildTaskState,
    /// Reservation time in Unix milliseconds.
    /// 预留时间（Unix 毫秒）。
    pub reserved_at_ms: i64,
    /// Activation time in Unix milliseconds.
    /// 激活时间（Unix 毫秒）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_at_ms: Option<i64>,
    /// Terminal time in Unix milliseconds.
    /// 终态时间（Unix 毫秒）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_at_ms: Option<i64>,
    /// Explicit child terminal outcome.
    /// 明确的子任务终态结果。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_outcome: Option<CollaborationChildTaskTerminalOutcome>,
    /// Exact durable non-success report; absence does not authorize inventing a successful response.
    /// 精确可靠非成功报告；缺失不授权虚构成功回复。
    pub non_success_result: Option<CollaborationChildTaskNonSuccessResult>,
    /// Durable complete input batch for the non-success result, separate from its original receipt.
    /// 非成功结果的可靠完整输入批次，与原回执分离。
    pub non_success_projection: Option<CollaborationChildTaskResultProjection>,
}

/// Exact queued user task selected after an auxiliary execution ended.
/// 辅助执行结束后选定的精确排队用户任务。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationQueuedUserSuccessor {
    /// New business turn identity.
    /// 新业务回合身份。
    pub turn_id: TurnId,
    /// Preserved queued input identity.
    /// 保留的排队输入身份。
    pub input_id: PendingInputId,
    /// Preserved user action identity.
    /// 保留的用户动作身份。
    pub client_action_id: ClientActionId,
    /// Exact provider-visible scheduled input event.
    /// 精确的供应商可见已调度输入事件。
    pub input_seq: Seq,
}

/// Dedicated question execution state.
/// 专用问题执行状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationQuestionExecutionRecord {
    /// Stable execution identity.
    /// 稳定执行身份。
    pub execution_id: CollaborationExecutionId,
    /// Operation that started or completed the execution.
    /// 启动或完成该执行的操作身份。
    pub operation_id: CollaborationOperationId,
    /// Question answered by this execution.
    /// 该执行回答的问题。
    pub question_id: CollaborationQuestionId,
    /// Target participant that owns the execution.
    /// 拥有该执行的目标参与者。
    pub participant: CollaborationParticipant,
    /// Explicit execution purpose; NormalTask is not valid here.
    /// 明确执行用途；此处不允许 NormalTask。
    pub purpose: CollaborationExecutionPurpose,
    /// Parent turn retained for in-turn question execution.
    /// 原回合内问题执行保留的父回合。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_turn_id: Option<TurnId>,
    /// Auxiliary turn created for a naturally-idle question execution.
    /// NaturallyIdle 闂鎵ц鎵€寤虹珯鐨勮緭鍑哄洖鍚堬紝InTurn 鐢ㄦ棤銆?
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_turn_id: Option<TurnId>,
    /// Current dedicated execution state.
    /// 当前专用执行状态。
    pub state: CollaborationQuestionExecutionState,
    /// Start time in Unix milliseconds.
    /// 启动时间（Unix 毫秒）。
    pub started_at_ms: i64,
    /// Completion time in Unix milliseconds.
    /// 完成时间（Unix 毫秒）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at_ms: Option<i64>,
    /// Explicit execution outcome after completion.
    /// 完成后的明确执行结果。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<CollaborationQuestionExecutionOutcome>,
    /// Durable queued successor, present only after explicit post-execution scheduling.
    /// 可靠队列后继，仅在明确执行结束后调度时存在。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queued_successor: Option<CollaborationQueuedUserSuccessor>,
}

/// User-owned authorization grant record.
/// 用户拥有的协作授权记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationGrantRecord {
    /// Stable grant identity.
    /// 稳定授权身份。
    pub grant_id: CollaborationGrantId,
    /// Authority that admitted this grant.
    /// 准入该授权的权威主体。
    pub authority: CollaborationGrantAuthority,
    /// Authorized source session scope.
    /// 获授权来源会话范围。
    pub source_session_id: String,
    /// Authorized target session scope.
    /// 获授权目标会话范围。
    pub target_session_id: String,
    /// Actions granted by the user.
    /// 用户授予的动作集合。
    pub actions: BTreeSet<CollaborationGrantAction>,
    /// Grant creation revision supplied by the admitting service.
    /// 准入服务提供的授权创建修订。
    pub created_revision: u64,
    /// Grant creation time in Unix milliseconds.
    /// 授权创建时间（Unix 毫秒）。
    pub created_at_ms: i64,
    /// Revocation time, if the grant was removed.
    /// 授权移除时间（如已移除）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revoked_at_ms: Option<i64>,
    /// Document revision that revoked the grant, when revoked.
    /// 撤销该授权的文档修订号（如已撤销）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revoked_revision: Option<u64>,
}

/// One run-control transition retained for audit and recovery.
/// 一条为审计和恢复保留的运行控制转换。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationRunControlRecord {
    /// Operation that produced the transition.
    /// 产生该转换的操作。
    pub operation_id: CollaborationOperationId,
    /// New explicit run-control state.
    /// 新的明确运行控制状态。
    pub state: CollaborationRunControl,
    /// Transition time in Unix milliseconds.
    /// 转换时间（Unix 毫秒）。
    pub changed_at_ms: i64,
}

/// Persisted source and target collaboration summary.
/// 持久化的来源与目标协作摘要。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationState {
    /// Typed state version.
    /// 强类型状态版本。
    pub version: u32,
    /// Source-owned question authority.
    /// 来源拥有的问题权威。
    pub questions: BTreeMap<CollaborationQuestionId, CollaborationQuestionRecord>,
    /// Source-owned outbound delivery progress.
    /// 来源拥有的出站投递进度。
    pub outbound_deliveries: BTreeMap<CollaborationDeliveryId, CollaborationDeliveryRecord>,
    /// Target-owned accepted and projected delivery records.
    /// 目标拥有的接纳与投影投递记录。
    pub inbound_deliveries: BTreeMap<CollaborationDeliveryId, CollaborationInboundDeliveryRecord>,
    /// Source-owned one-way notification outbox.
    /// 来源拥有的单向通知出站箱。
    pub notifications: BTreeMap<CollaborationNotificationId, CollaborationNotificationRecord>,
    /// Target-owned one-way notification acceptance and projection records.
    /// 目标拥有的单向通知接纳与投影记录。
    pub inbound_notifications:
        BTreeMap<CollaborationNotificationId, CollaborationInboundNotificationRecord>,
    /// Source-owned queued task and steering intents with exact acceptance acknowledgements.
    /// 来源拥有的排队任务和方向意图及精确接纳确认。
    pub outbound_task_messages: BTreeMap<CollaborationDeliveryId, CollaborationTaskMessageRecord>,
    /// Target-owned accepted task intents, pending their legal execution boundary.
    /// 目标拥有的已接纳任务意图，等待其合法执行边界。
    pub inbound_task_messages:
        BTreeMap<CollaborationDeliveryId, CollaborationTaskMessageAcceptance>,
    /// Target application or failure receipts, separate from immutable acceptance.
    /// 目标应用或失败回执，与不可变接纳分离。
    pub task_message_effects: BTreeMap<CollaborationDeliveryId, CollaborationTaskMessageEffect>,
    /// Target-owned final business results, independent of delivery and application receipts.
    /// 目标拥有的最终业务结果，独立于投递及应用回执。
    pub task_results: BTreeMap<CollaborationDeliveryId, CollaborationTaskResult>,
    /// Original requester-owned acceptance of complete terminal results.
    /// 原请求者拥有的完整终态结果接纳。
    pub received_task_results: BTreeMap<CollaborationDeliveryId, CollaborationTaskResultAcceptance>,
    /// Requester-owned proofs of complete insertion into the original source turn.
    /// 请求者拥有的完整插入原来源回合凭据。
    pub task_result_projections:
        BTreeMap<CollaborationDeliveryId, CollaborationTaskResultProjection>,
    /// Executor-owned acknowledgement that the original requester persisted the exact result.
    /// 执行者拥有的确认，证明原请求者持久化了精确结果。
    pub task_result_acknowledgements:
        BTreeMap<CollaborationDeliveryId, CollaborationTaskResultAcknowledgement>,
    /// Parent-turn-bound child task obligations.
    /// 按父回合绑定的子任务完成义务。
    pub child_tasks: BTreeMap<CollaborationExecutionId, CollaborationChildTaskRecord>,
    /// Dedicated question execution records.
    /// 专用问题执行记录。
    pub question_executions:
        BTreeMap<CollaborationExecutionId, CollaborationQuestionExecutionRecord>,
    /// User-owned grant history.
    /// 用户拥有的授权历史。
    pub grants: BTreeMap<CollaborationGrantId, CollaborationGrantRecord>,
    /// Current explicit run-control state.
    /// 当前明确的运行控制状态。
    pub run_control: CollaborationRunControl,
    /// Run-control history in occurrence order.
    /// 按发生顺序保存的运行控制历史。
    pub run_control_history: Vec<CollaborationRunControlRecord>,
    /// Operation log used for exact idempotency and conflict detection.
    /// 用于精确幂等与冲突检测的操作日志。
    pub operations: BTreeMap<CollaborationOperationId, CollaborationOperationRecord>,
}

impl CollaborationState {
    /// Returns whether edit or delete must preserve unresolved collaboration facts.
    /// 判断编辑或删除是否必须保留未解决的协作事实。
    ///
    /// # Returns
    /// 返回值
    /// Returns true when any question, delivery, task message, child task, question execution or
    /// notification still owes a peer a fact that replacing or deleting this history would discard.
    /// 当仍存在任何问题、投递、任务消息、子任务、问题执行或通知对某个对端欠有会被替换或删除历史的
    /// 事实时返回真。
    ///
    /// # Notes
    /// 说明
    /// This is the single definition of that boundary: the canonical document delegates to it, and a
    /// cold member verified through its locator reads the same state and applies the same predicate.
    /// 这是该边界的唯一定义：规范文档委托给它，而通过定位器校验的冷态成员读取同一状态并施加同一判定。
    pub fn requires_edit_or_delete_settlement(&self) -> bool {
        self.outbound_task_messages
            .iter()
            .any(|(delivery_id, record)| {
                record.acceptance.is_none() || !self.received_task_results.contains_key(delivery_id)
            })
            || self.inbound_task_messages.keys().any(|delivery_id| {
                match self.task_message_effects.get(delivery_id) {
                    Some(effect) => match effect.outcome {
                        CollaborationTaskMessageEffectKind::Claimed { .. }
                        | CollaborationTaskMessageEffectKind::Steered { .. } => {
                            !self.task_result_acknowledgements.contains_key(delivery_id)
                        }
                        // Failed applications use the same durable receiver and executor receipts.
                        // 应用前失败同样使用可靠的接收方与执行方回执。
                        CollaborationTaskMessageEffectKind::Failed { .. } => {
                            !self.task_result_acknowledgements.contains_key(delivery_id)
                        }
                    },
                    None => true,
                }
            })
            || self.questions.values().any(|question| {
                question.state == CollaborationQuestionState::Pending
                    || question.answer.as_ref().is_some_and(|answer| {
                        answer.delivery.state == CollaborationAnswerDeliveryState::Undelivered
                    })
            })
            || self.outbound_deliveries.values().any(|delivery| {
                // A rejection before target acceptance has no target settlement to acknowledge.
                // 目标接纳前的拒收没有需要确认的目标结算。
                delivery.settlement_ack.is_none()
                    && delivery.state != CollaborationDeliveryState::Failed
                    && self
                        .questions
                        .get(&delivery.question_id)
                        .is_some_and(|question| {
                            question.state != CollaborationQuestionState::Pending
                        })
            })
            || self.inbound_deliveries.values().any(|delivery| {
                delivery.settlement.is_none()
                    && matches!(
                        delivery.state,
                        CollaborationDeliveryState::Accepted
                            | CollaborationDeliveryState::Projected
                    )
            })
            || self.child_tasks.values().any(|task| {
                matches!(
                    task.state,
                    CollaborationChildTaskState::Reserved | CollaborationChildTaskState::Active
                )
            })
            || self
                .question_executions
                .values()
                .any(|execution| execution.state == CollaborationQuestionExecutionState::Running)
            || self.notifications.values().any(|notification| {
                matches!(
                    notification.state,
                    CollaborationNotificationState::Recorded
                        | CollaborationNotificationState::Accepted
                )
            })
            || self.inbound_notifications.values().any(|notification| {
                matches!(notification.state, CollaborationNotificationState::Accepted)
            })
    }

    /// Builds a fresh empty collaboration state.
    /// 构建一份全新的空协作状态。
    pub fn new() -> Self {
        Self {
            version: COLLABORATION_METADATA_VERSION,
            questions: BTreeMap::new(),
            outbound_deliveries: BTreeMap::new(),
            inbound_deliveries: BTreeMap::new(),
            notifications: BTreeMap::new(),
            inbound_notifications: BTreeMap::new(),
            outbound_task_messages: BTreeMap::new(),
            inbound_task_messages: BTreeMap::new(),
            task_message_effects: BTreeMap::new(),
            task_results: BTreeMap::new(),
            received_task_results: BTreeMap::new(),
            task_result_projections: BTreeMap::new(),
            task_result_acknowledgements: BTreeMap::new(),
            child_tasks: BTreeMap::new(),
            question_executions: BTreeMap::new(),
            grants: BTreeMap::new(),
            run_control: CollaborationRunControl::NaturallyIdle,
            run_control_history: Vec::new(),
            operations: BTreeMap::new(),
        }
    }

    /// Validates the state shape without consulting the event stream.
    /// 在不读取事件流的情况下校验状态结构。
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.version != COLLABORATION_METADATA_VERSION {
            return Err(DomainError::InvalidStatistics {
                field: "collaboration.version",
            });
        }
        for (delivery_id, record) in &self.outbound_task_messages {
            record.validate()?;
            if record.message.delivery_id != *delivery_id {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_message.outbound_key",
                });
            }
        }
        for (delivery_id, acceptance) in &self.inbound_task_messages {
            acceptance.validate()?;
            if acceptance.message.delivery_id != *delivery_id {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_message.inbound_key",
                });
            }
        }
        for (delivery_id, effect) in &self.task_message_effects {
            let acceptance = self.inbound_task_messages.get(delivery_id).ok_or(
                DomainError::CollaborationStateViolation {
                    field: "collaboration.task_message.effect_missing_acceptance",
                },
            )?;
            effect.validate(acceptance)?;
            if effect.delivery_id != *delivery_id {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_message.effect_key",
                });
            }
        }
        for (delivery_id, result) in &self.task_results {
            result.validate()?;
            if result.effect.delivery_id != *delivery_id
                || self.inbound_task_messages.get(delivery_id) != Some(&result.acceptance)
                || self.task_message_effects.get(delivery_id) != Some(&result.effect)
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_result.authority",
                });
            }
        }
        self.validate_task_result_transfers()?;
        self.validate_task_result_projections()?;
        for (question_id, question) in &self.questions {
            if question.question_id != *question_id {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.question.key",
                });
            }
            question
                .source
                .validate("collaboration.question.source", true)?;
            question
                .target
                .validate("collaboration.question.target", false)?;
            validate_body("collaboration.question.body", &question.body)?;
            validate_timestamp(
                "collaboration.question.created_at_ms",
                question.created_at_ms,
            )?;
            match question.state {
                CollaborationQuestionState::Pending => {
                    if question.answer.is_some() || question.cancellation.is_some() {
                        return Err(DomainError::InvalidStatistics {
                            field: "collaboration.question.pending_settlement",
                        });
                    }
                }
                CollaborationQuestionState::Answered => {
                    if question.answer.is_none() || question.cancellation.is_some() {
                        return Err(DomainError::InvalidStatistics {
                            field: "collaboration.question.answer_settlement",
                        });
                    }
                    let answer = question
                        .answer
                        .as_ref()
                        .expect("answered question has answer");
                    if answer.responder != question.target {
                        return Err(DomainError::CollaborationStateViolation {
                            field: "collaboration.question.answer.responder",
                        });
                    }
                    validate_body("collaboration.answer.body", &answer.body)?;
                    validate_timestamp("collaboration.answer.settled_at_ms", answer.settled_at_ms)?;
                    validate_answer_delivery_record(&answer.delivery)?;
                }
                CollaborationQuestionState::Cancelled => {
                    if question.cancellation.is_none() || question.answer.is_some() {
                        return Err(DomainError::InvalidStatistics {
                            field: "collaboration.question.cancel_settlement",
                        });
                    }
                    let cancellation = question
                        .cancellation
                        .as_ref()
                        .expect("cancelled question has cancellation");
                    validate_reason(
                        "collaboration.question.cancellation.reason",
                        &cancellation.reason,
                    )?;
                    validate_timestamp(
                        "collaboration.question.cancellation.cancelled_at_ms",
                        cancellation.cancelled_at_ms,
                    )?;
                }
            }
        }
        for (delivery_id, delivery) in &self.outbound_deliveries {
            if delivery.delivery_id != *delivery_id {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.outbound_delivery.key",
                });
            }
            validate_delivery_record(delivery)?;
            let Some(question) = self.questions.get(&delivery.question_id) else {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.outbound_delivery.question",
                });
            };
            if question.delivery_id != delivery.delivery_id
                || question.source != delivery.source
                || question.target != delivery.target
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.outbound_delivery.binding",
                });
            }
            if let Some(ack) = &delivery.settlement_ack {
                let expected = match ack.state {
                    CollaborationTargetSettlementState::Answered => {
                        CollaborationQuestionState::Answered
                    }
                    CollaborationTargetSettlementState::Cancelled => {
                        CollaborationQuestionState::Cancelled
                    }
                };
                if question.state != expected {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "collaboration.outbound_delivery.settlement_binding",
                    });
                }
            }
        }
        for (delivery_id, delivery) in &self.inbound_deliveries {
            if delivery.delivery_id != *delivery_id {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.inbound_delivery.key",
                });
            }
            validate_inbound_delivery_record(delivery)?;
        }
        for (notification_id, notification) in &self.notifications {
            if notification.notification_id != *notification_id {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.notification.key",
                });
            }
            validate_notification_record(notification)?;
        }
        for (notification_id, notification) in &self.inbound_notifications {
            if notification.notification_id != *notification_id {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.inbound_notification.key",
                });
            }
            validate_inbound_notification_record(notification)?;
        }
        // Recovery must reject simultaneous live owners of the same reusable task.
        // 恢复必须拒绝同一可复用任务具有多个活动所有者。
        let mut live_child_tasks = BTreeSet::new();
        for (execution_id, child) in &self.child_tasks {
            if child.execution_id != *execution_id {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.child_task.key",
                });
            }
            validate_child_task_record(child)?;
            if child.state != CollaborationChildTaskState::Terminal
                && !live_child_tasks.insert(&child.task_id)
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.child_task.concurrent_execution",
                });
            }
        }
        for child in self.child_tasks.values() {
            if let Some(result) = &child.non_success_result {
                if !self
                    .operations
                    .get(&result.operation_id)
                    .is_some_and(|operation| {
                        operation.event_seq == result.event_seq
                            && operation.semantic
                                == (CollaborationOperationSemantic::RecordChildTaskNonSuccess {
                                    execution_id: child.execution_id.clone(),
                                    proof: result.proof.clone(),
                                })
                            && operation.result
                                == (CollaborationOperationResult::ChildTask {
                                    execution_id: child.execution_id.clone(),
                                    task_id: child.task_id.clone(),
                                    state: CollaborationChildTaskState::Terminal,
                                })
                    })
                {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "collaboration.child_result.operation",
                    });
                }
            }
            // Terminal callbacks stay anchored to the authenticated reservation.
            // 终态回调始终锚定已认证预留。
            let expected = CollaborationOperationSemantic::ReserveChildTask {
                source_tool_name: child.source_tool_name.clone(),
                execution_id: child.execution_id.clone(),
                task_id: child.task_id.clone(),
                parent_turn_id: child.parent_turn_id,
                child: child.child.clone(),
            };
            if !self
                .operations
                .get(&child.reservation_operation_id)
                .is_some_and(|operation| {
                    operation.semantic == expected
                        && operation.result
                            == (CollaborationOperationResult::ChildTask {
                                execution_id: child.execution_id.clone(),
                                task_id: child.task_id.clone(),
                                state: CollaborationChildTaskState::Reserved,
                            })
                })
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.child_task.reservation_origin",
                });
            }
        }
        self.validate_child_result_projection_operations()?;
        for (execution_id, execution) in &self.question_executions {
            if execution.execution_id != *execution_id {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.question_execution.key",
                });
            }
            validate_question_execution_record(execution)?;
        }
        for (grant_id, grant) in &self.grants {
            if grant.grant_id != *grant_id {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.grant.key",
                });
            }
            validate_grant_record(grant)?;
        }
        for transition in &self.run_control_history {
            validate_timestamp(
                "collaboration.run_control.changed_at_ms",
                transition.changed_at_ms,
            )?;
        }
        if self
            .run_control_history
            .last()
            .is_some_and(|transition| transition.state != self.run_control)
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.run_control.current",
            });
        }
        // Operation sequence, not opaque ID ordering, determines the current source task receipt.
        // 由操作事件序号决定当前来源任务回执，不能使用不透明 ID 的排序。
        let mut latest_task_receipts: BTreeMap<
            CollaborationDeliveryId,
            (Seq, &CollaborationTaskMessageRecord),
        > = BTreeMap::new();
        for (operation_id, operation) in &self.operations {
            if operation.operation_id != *operation_id {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.operation.key",
                });
            }
            operation.validate()?;
            match &operation.result {
                CollaborationOperationResult::TaskResultProjected { .. }
                | CollaborationOperationResult::TaskResultAccepted { .. }
                | CollaborationOperationResult::TaskResultAcknowledged { .. } => {
                    // The transfer validator checks both forward and reverse operation bindings.
                    // 投递校验器检查操作的正向与反向绑定。
                }
                CollaborationOperationResult::TaskResult { result } => {
                    if result.operation_id != operation.operation_id
                        || result.event_seq != operation.event_seq
                        || operation.semantic
                            != (CollaborationOperationSemantic::RecordTaskResult {
                                delivery_id: result.effect.delivery_id.clone(),
                            })
                        || self.task_results.get(&result.effect.delivery_id) != Some(result)
                    {
                        return Err(DomainError::CollaborationStateViolation {
                            field: "collaboration.task_result.operation",
                        });
                    }
                }
                CollaborationOperationResult::Question {
                    question_id,
                    delivery_id,
                } => {
                    if !self.questions.contains_key(question_id)
                        || !self.outbound_deliveries.contains_key(delivery_id)
                    {
                        return Err(DomainError::CollaborationStateViolation {
                            field: "collaboration.operation.question_result",
                        });
                    }
                }
                CollaborationOperationResult::Delivery { delivery_id, .. } => {
                    if !self.outbound_deliveries.contains_key(delivery_id)
                        && !self.inbound_deliveries.contains_key(delivery_id)
                    {
                        return Err(DomainError::CollaborationStateViolation {
                            field: "collaboration.operation.delivery_result",
                        });
                    }
                }
                CollaborationOperationResult::DeliverySettlement { delivery_id, .. } => {
                    if !self.outbound_deliveries.contains_key(delivery_id)
                        && !self.inbound_deliveries.contains_key(delivery_id)
                    {
                        return Err(DomainError::CollaborationStateViolation {
                            field: "collaboration.operation.delivery_settlement_result",
                        });
                    }
                }
                CollaborationOperationResult::TaskMessageEffect { effect } => {
                    let expected_semantic = match effect.outcome {
                        CollaborationTaskMessageEffectKind::Claimed { .. } => {
                            CollaborationOperationSemantic::ClaimTaskMessage {
                                delivery_id: effect.delivery_id.clone(),
                            }
                        }
                        CollaborationTaskMessageEffectKind::Steered { .. } => {
                            CollaborationOperationSemantic::ApplyTaskSteering {
                                delivery_id: effect.delivery_id.clone(),
                            }
                        }
                        CollaborationTaskMessageEffectKind::Failed { reason } => {
                            CollaborationOperationSemantic::FailTaskMessage {
                                delivery_id: effect.delivery_id.clone(),
                                reason,
                            }
                        }
                    };
                    if effect.operation_id != operation.operation_id
                        || effect.event_seq != operation.event_seq
                        || operation.semantic != expected_semantic
                        || self.task_message_effects.get(&effect.delivery_id) != Some(effect)
                    {
                        return Err(DomainError::CollaborationStateViolation {
                            field: "collaboration.task_message.effect_operation",
                        });
                    }
                }
                CollaborationOperationResult::TaskMessage { record } => {
                    record.validate()?;
                    let matches_semantic = match &operation.semantic {
                        CollaborationOperationSemantic::SendTaskMessage { message } => {
                            record.message == *message
                                && record.acceptance.is_none()
                                && message.operation_id == operation.operation_id
                        }
                        CollaborationOperationSemantic::AcknowledgeTaskMessage { acceptance } => {
                            record.message == acceptance.message
                                && record.acceptance.as_ref() == Some(acceptance)
                        }
                        _ => false,
                    };
                    if !matches_semantic
                        || !self
                            .outbound_task_messages
                            .contains_key(&record.message.delivery_id)
                    {
                        return Err(DomainError::CollaborationStateViolation {
                            field: "collaboration.task_message.operation_result",
                        });
                    }
                    if let Some((previous_seq, _)) =
                        latest_task_receipts.get(&record.message.delivery_id)
                    {
                        if *previous_seq == operation.event_seq {
                            return Err(DomainError::CollaborationStateViolation {
                                field: "collaboration.task_message.duplicate_receipt_seq",
                            });
                        }
                        if *previous_seq > operation.event_seq {
                            continue;
                        }
                    }
                    latest_task_receipts.insert(
                        record.message.delivery_id.clone(),
                        (operation.event_seq, record),
                    );
                }
                CollaborationOperationResult::TaskMessageAcceptance { acceptance } => {
                    acceptance.validate()?;
                    if acceptance.operation_id != operation.operation_id
                        || acceptance.event_seq != operation.event_seq
                        || operation.semantic
                            != (CollaborationOperationSemantic::AcceptTaskMessage {
                                message: acceptance.message.clone(),
                            })
                        || !self
                            .inbound_task_messages
                            .contains_key(&acceptance.message.delivery_id)
                    {
                        return Err(DomainError::CollaborationStateViolation {
                            field: "collaboration.task_message.acceptance_result",
                        });
                    }
                }
                CollaborationOperationResult::Notification { notification } => {
                    validate_notification_record(notification)?;
                    if !self
                        .notifications
                        .contains_key(&notification.notification_id)
                    {
                        return Err(DomainError::CollaborationStateViolation {
                            field: "collaboration.operation.notification_result",
                        });
                    }
                }
                CollaborationOperationResult::NotificationDelivery { snapshot } => {
                    let present = match snapshot {
                        CollaborationNotificationOperationSnapshot::Outbound { notification } => {
                            validate_notification_record(notification)?;
                            self.notifications
                                .contains_key(&notification.notification_id)
                        }
                        CollaborationNotificationOperationSnapshot::Inbound { notification } => {
                            validate_inbound_notification_record(notification)?;
                            self.inbound_notifications
                                .contains_key(&notification.notification_id)
                        }
                    };
                    if !present {
                        return Err(DomainError::CollaborationStateViolation {
                            field: "collaboration.operation.notification_delivery_result",
                        });
                    }
                }
                CollaborationOperationResult::Settlement { question_id, .. }
                | CollaborationOperationResult::AnswerDelivery { question_id, .. } => {
                    if !self.questions.contains_key(question_id) {
                        return Err(DomainError::CollaborationStateViolation {
                            field: "collaboration.operation.question_result",
                        });
                    }
                }
                CollaborationOperationResult::ChildTask {
                    execution_id,
                    task_id,
                    ..
                } => {
                    if let CollaborationOperationSemantic::RecordChildTaskNonSuccess {
                        execution_id: owner,
                        proof,
                    } = &operation.semantic
                    {
                        if owner != execution_id
                            || !self
                                .child_tasks
                                .get(owner)
                                .and_then(|task| task.non_success_result.as_ref())
                                .is_some_and(|result| {
                                    result.proof == *proof
                                        && result.operation_id == operation.operation_id
                                        && result.event_seq == operation.event_seq
                                })
                        {
                            return Err(DomainError::CollaborationStateViolation {
                                field: "collaboration.child_result.operation_owner",
                            });
                        }
                    }
                    if !self
                        .child_tasks
                        .get(execution_id)
                        .is_some_and(|task| task.task_id == *task_id)
                    {
                        return Err(DomainError::CollaborationStateViolation {
                            field: "collaboration.operation.child_result",
                        });
                    }
                }
                CollaborationOperationResult::QuestionExecution { execution } => {
                    validate_question_execution_record(execution)?;
                    if execution.operation_id != operation.operation_id
                        || !self
                            .question_executions
                            .contains_key(&execution.execution_id)
                    {
                        return Err(DomainError::CollaborationStateViolation {
                            field: "collaboration.operation.execution_result",
                        });
                    }
                    // A receipt records this operation's result, never the execution's latest state.
                    // 回执记录当前操作的结果，绝不表示执行的最新状态。
                    let matches_semantic = match &operation.semantic {
                        CollaborationOperationSemantic::BeginQuestionExecution {
                            execution_id,
                            question_id,
                            participant,
                            purpose,
                            parent_turn_id,
                        } => {
                            execution.execution_id == *execution_id
                                && execution.question_id == *question_id
                                && execution.participant == *participant
                                && execution.purpose == *purpose
                                && execution.parent_turn_id == *parent_turn_id
                                && execution.state == CollaborationQuestionExecutionState::Running
                        }
                        CollaborationOperationSemantic::CompleteQuestionExecution {
                            execution_id,
                            outcome,
                        } => {
                            execution.execution_id == *execution_id
                                && execution.outcome == Some(*outcome)
                                && execution.state == CollaborationQuestionExecutionState::Completed
                                && execution.queued_successor.is_none()
                        }
                        CollaborationOperationSemantic::ScheduleQuestionSuccessor {
                            execution_id,
                        } => {
                            execution.execution_id == *execution_id
                                && execution.queued_successor.is_some()
                                && execution.state == CollaborationQuestionExecutionState::Completed
                        }
                        _ => false,
                    };
                    if !matches_semantic {
                        return Err(DomainError::CollaborationStateViolation {
                            field: "collaboration.operation.execution_semantic",
                        });
                    }
                }
                CollaborationOperationResult::RunControl { .. } => {}
                CollaborationOperationResult::Grant { grant_id } => {
                    if !self.grants.contains_key(grant_id) {
                        return Err(DomainError::CollaborationStateViolation {
                            field: "collaboration.operation.grant_result",
                        });
                    }
                }
            }
        }
        for (delivery_id, record) in &self.outbound_task_messages {
            if latest_task_receipts
                .get(delivery_id)
                .map(|(_, snapshot)| *snapshot)
                != Some(record)
                || self
                    .operations
                    .get(&record.message.operation_id)
                    .is_none_or(|operation| {
                        operation.semantic
                            != (CollaborationOperationSemantic::SendTaskMessage {
                                message: record.message.clone(),
                            })
                    })
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_message.latest_source_receipt",
                });
            }
        }
        for result in self.task_results.values() {
            if self
                .operations
                .get(&result.operation_id)
                .is_none_or(|operation| {
                    operation.result
                        != (CollaborationOperationResult::TaskResult {
                            result: result.clone(),
                        })
                })
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_result.receipt",
                });
            }
        }
        for effect in self.task_message_effects.values() {
            if self
                .operations
                .get(&effect.operation_id)
                .is_none_or(|operation| {
                    operation.result
                        != (CollaborationOperationResult::TaskMessageEffect {
                            effect: effect.clone(),
                        })
                })
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_message.effect_receipt",
                });
            }
        }
        for acceptance in self.inbound_task_messages.values() {
            if self
                .operations
                .get(&acceptance.operation_id)
                .is_none_or(|operation| {
                    operation.result
                        != (CollaborationOperationResult::TaskMessageAcceptance {
                            acceptance: acceptance.clone(),
                        })
                })
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_message.target_receipt",
                });
            }
        }
        Ok(())
    }
}

impl Default for CollaborationState {
    /// Defaults to an empty naturally idle state until host admission starts execution.
    /// 默认使用空的自然空闲状态，直到宿主准入开始执行。
    fn default() -> Self {
        Self::new()
    }
}

/// Required metadata wrapper around the recoverable collaboration state.
/// 包裹可恢复协作状态的必需 metadata 结构。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationMetadata {
    /// Typed metadata schema version.
    /// 强类型 metadata schema 版本。
    pub version: u32,
    /// Last collaboration event seq represented by this summary.
    /// 此摘要表达的最后一条协作事件 seq。
    pub last_event_seq: Option<Seq>,
    /// Inclusive history boundary before which collaboration facts are read-only history.
    /// 协作事实只读历史的包含边界序号。
    pub history_boundary_seq: Option<Seq>,
    /// Complete current state used for tail-only recovery.
    /// 用于仅尾段恢复的完整当前状态。
    pub state: CollaborationState,
}

impl CollaborationMetadata {
    /// Builds the required empty collaboration metadata.
    /// 构建必需的空协作 metadata。
    pub fn new() -> Self {
        Self {
            version: COLLABORATION_METADATA_VERSION,
            last_event_seq: None,
            history_boundary_seq: None,
            state: CollaborationState::new(),
        }
    }

    /// Validates the metadata wrapper and its typed state.
    /// 校验 metadata 包装结构及其强类型状态。
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.version != COLLABORATION_METADATA_VERSION {
            return Err(DomainError::InvalidStatistics {
                field: "collaboration_metadata.version",
            });
        }
        if self
            .history_boundary_seq
            .is_some_and(|boundary| boundary.value() == 0)
        {
            return Err(DomainError::InvalidStatistics {
                field: "collaboration_metadata.history_boundary_seq",
            });
        }
        self.state.validate()
    }
}

impl Default for CollaborationMetadata {
    /// Defaults to the required empty collaboration metadata.
    /// 默认使用必需的空协作 metadata。
    fn default() -> Self {
        Self::new()
    }
}

/// Kind of one persisted typed collaboration mutation.
/// 一次持久化强类型协作变更的类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollaborationEventKind {
    /// The requester atomically appended every result chunk to its original business turn.
    /// 请求者将全部结果分块原子追加到原业务回合。
    TaskResultProjected,
    /// The target froze a real business terminal and its final response.
    /// 目标冻结真实业务终态及最终回复。
    TaskResultRecorded,
    /// The original requester persisted the exact terminal result.
    /// 原请求者持久化了精确终态结果。
    TaskResultAccepted,
    /// The executor confirmed the requester's durable receipt without a model reply.
    /// 执行者确认请求者的可靠回执，不需要模型回复。
    TaskResultAcknowledged,
    /// A task message was applied as steering or explicitly failed before application.
    /// 任务消息已作为方向应用，或在应用前明确失败。
    TaskMessageEffect,
    /// A source recorded an immutable queue or steering intent.
    /// 来源记录不可变的排队或方向引导意图。
    TaskMessageSent,
    /// A target accepted an exact task intent.
    /// 目标接纳精确任务意图。
    TaskMessageAccepted,
    /// A source durably acknowledged target acceptance.
    /// 来源可靠确认目标接纳。
    TaskMessageAcknowledged,
    /// A source created a question and its initial delivery.
    /// 来源创建问题及其初始投递。
    QuestionCreated,
    /// A source acknowledged target acceptance or progress.
    /// 来源确认目标接纳或投递进度。
    DeliveryRecorded,
    /// A target accepted an inbound delivery.
    /// 目标接纳入站投递。
    DeliveryAccepted,
    /// A target projected an inbound delivery.
    /// 目标投影入站投递。
    DeliveryProjected,
    /// A target failed an inbound delivery.
    /// 目标使入站投递失败。
    DeliveryFailed,
    /// A target recorded the source question's winning terminal settlement.
    /// 鐩爣璁板綍鏉ユ簮闂鐨勮儨鍑虹粓鎬佺粨绠楃‘璁ゃ€?
    DeliverySettled,
    /// A source durably recorded target settlement acknowledgement.
    /// 鏉ユ簮鎸佷箙鍖栫洰鏍囩粨绠楃‘璁ゃ€?
    DeliverySettlementAcknowledged,
    /// A source durably recorded a one-way notification in its outbox.
    /// 来源将一条单向通知可靠记录到出站箱。
    NotificationSent,
    /// A source recorded target-side notification progress.
    /// 来源记录目标侧通知投递进度。
    NotificationProgress,
    /// A target durably accepted a one-way notification.
    /// 目标可靠接纳一条单向通知。
    NotificationAccepted,
    /// A target projected a one-way notification at a legal provider boundary.
    /// 目标在合法 Provider 边界投影一条单向通知。
    NotificationProjected,
    /// A target recorded a one-way notification delivery failure.
    /// 目标记录一条单向通知投递失败。
    NotificationFailed,
    /// A target submitted the first winning answer.
    /// 目标提交首个胜出答案。
    QuestionAnswered,
    /// A source submitted the first winning cancellation.
    /// 来源提交首个胜出撤销。
    QuestionCancelled,
    /// A source recorded an independent answer delivery revision.
    /// 来源记录独立答案交付修订。
    AnswerDelivered,
    /// A source recorded that an authoritative answer cannot enter its model context.
    /// 来源记录权威答案无法进入其模型上下文。
    AnswerDeliveryFailed,
    /// A parent reserved a child task.
    /// 父回合预留子任务。
    ChildTaskReserved,
    /// A reserved child task became active.
    /// 已预留子任务进入活动状态。
    ChildTaskActivated,
    /// A child task reached an explicit terminal state.
    /// 子任务到达明确终态。
    ChildTaskTerminal,
    /// Durable stopped or failed child result recorded under the original reservation.
    /// 在原预留下记录的可靠停止或失败子结果。
    ChildTaskNonSuccessRecorded,
    /// Every non-success result chunk entered the original parent business context.
    /// 非成功结果的全部分块已进入原父业务上下文。
    ChildTaskNonSuccessProjected,
    /// A dedicated question execution started.
    /// 专用问题执行开始。
    QuestionExecutionStarted,
    /// A dedicated question execution completed.
    /// 专用问题执行完成。
    QuestionExecutionCompleted,
    /// A completed auxiliary execution selected a real queued user task.
    /// 已完成辅助执行选定真实排队用户任务。
    QuestionSuccessorScheduled,
    /// A user grant was added.
    /// 增加用户授权。
    GrantAdded,
    /// A user grant was revoked.
    /// 撤销用户授权。
    GrantRemoved,
    /// Explicit run control changed.
    /// 明确运行控制发生变化。
    RunControlChanged,
    /// A new history generation cleared executable ownership.
    /// 新历史代际清除了可执行所有权。
    GenerationOwnershipCleared,
}

/// Typed after-state for exactly the entity changed by one collaboration event.
/// 一条协作事件只保存其变更实体的强类型后状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CollaborationAfterState {
    /// Exact immutable proof of result insertion, without duplicating its body.
    /// 结果插入的精确不可变凭据，不重复其正文。
    TaskResultProjection {
        /// Complete ordered input coordinates.
        /// 完整有序输入坐标。
        projection: CollaborationTaskResultProjection,
    },
    /// Requester-owned result acceptance, distinct from model projection.
    /// 请求者所有的结果接纳，与模型投影区分。
    ReceivedTaskResult {
        /// Immutable receiver publication receipt.
        /// 不可变接收方发布回执。
        acceptance: CollaborationTaskResultAcceptance,
    },
    /// Executor-owned confirmation of receiver persistence.
    /// 执行者所有的接收方持久化确认。
    TaskResultAcknowledgement {
        /// Exact acknowledgement of the original receiver receipt.
        /// 对原接收回执的精确确认。
        acknowledgement: CollaborationTaskResultAcknowledgement,
    },
    /// Immutable final result, separate from task acceptance and application.
    /// 不可变最终结果，与任务接纳及应用分离。
    TaskResult {
        /// Exact target-owned result.
        /// 精确目标所有结果。
        result: CollaborationTaskResult,
    },
    /// Target task-message application or failure after-state.
    /// 目标任务消息应用或失败后状态。
    TaskMessageEffect {
        /// Exact target effect receipt.
        /// 精确目标效果回执。
        effect: CollaborationTaskMessageEffect,
    },
    /// Source task-message creation or acknowledgement snapshot.
    /// 来源任务消息创建或确认快照。
    OutboundTaskMessage {
        /// Exact source record after this operation.
        /// 此操作后的精确来源记录。
        record: CollaborationTaskMessageRecord,
    },
    /// Target task-message acceptance snapshot.
    /// 目标任务消息接纳快照。
    InboundTaskMessage {
        /// Exact immutable acceptance receipt.
        /// 精确不可变接纳回执。
        acceptance: CollaborationTaskMessageAcceptance,
    },
    /// Question creation or settlement changed a source-owned question and its delivery.
    /// 问题创建或结算修改了来源问题及其投递。
    Question {
        /// Complete source question after-state.
        /// 完整来源问题后状态。
        question: CollaborationQuestionRecord,
        /// Complete source delivery after-state.
        /// 完整来源投递后状态。
        delivery: CollaborationDeliveryRecord,
    },
    /// Source-side delivery progress after-state.
    /// 来源侧投递进度后状态。
    OutboundDelivery {
        /// Complete outbound delivery record.
        /// 完整出站投递记录。
        delivery: CollaborationDeliveryRecord,
    },
    /// Target-side delivery acceptance, projection, or failure after-state.
    /// 目标侧投递接纳、投影或失败后状态。
    InboundDelivery {
        /// Complete inbound delivery record.
        /// 完整入站投递记录。
        delivery: CollaborationInboundDeliveryRecord,
    },
    /// Source-owned one-way notification outbox after-state.
    /// 来源单向通知出站箱后状态。
    OutboundNotification {
        /// Complete source notification record.
        /// 完整来源通知记录。
        notification: CollaborationNotificationRecord,
    },
    /// Target-owned one-way notification after-state.
    /// 目标单向通知记录后状态。
    InboundNotification {
        /// Complete target notification record.
        /// 完整目标通知记录。
        notification: CollaborationInboundNotificationRecord,
    },
    /// Answer delivery changed the source-owned question's nested answer record.
    /// 答案交付修改了来源问题内嵌的答案记录。
    AnswerDelivery {
        /// Related question identity.
        /// 关联问题身份。
        question_id: CollaborationQuestionId,
        /// Complete answer delivery after-state.
        /// 完整答案交付后状态。
        delivery: CollaborationAnswerDeliveryRecord,
    },
    /// Child task reservation or transition after-state.
    /// 子任务预留或转换后状态。
    ChildTask {
        /// Complete child-task record.
        /// 完整子任务记录。
        task: CollaborationChildTaskRecord,
    },
    /// Dedicated question execution transition after-state.
    /// 专用问题执行转换后状态。
    QuestionExecution {
        /// Complete question-execution record.
        /// 完整问题执行记录。
        execution: CollaborationQuestionExecutionRecord,
    },
    /// Grant add or revoke after-state.
    /// 授权增加或撤销后状态。
    Grant {
        /// Complete grant record.
        /// 完整授权记录。
        grant: CollaborationGrantRecord,
    },
    /// Run-control transition after-state.
    /// 运行控制转换后状态。
    RunControl {
        /// Complete run-control transition record.
        /// 完整运行控制转换记录。
        transition: CollaborationRunControlRecord,
    },
    /// Generation reset has no executable entity after-state.
    /// 代际重置没有可执行实体后状态。
    GenerationOwnershipCleared,
}

/// Complete typed collaboration event payload.
/// 完整的强类型协作事件载荷。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationEvent {
    /// Event payload version.
    /// 事件载荷版本。
    pub version: u32,
    /// Stable operation identity that produced this event.
    /// 产生该事件的稳定操作身份。
    pub operation_id: CollaborationOperationId,
    /// Typed mutation kind.
    /// 强类型变更类型。
    pub event_kind: CollaborationEventKind,
    /// Complete changed-entity after-state for atomic publication and replay checks.
    /// 用于原子发布与重放校验的完整变更实体后状态。
    pub after_state: CollaborationAfterState,
}

impl CollaborationEvent {
    /// Validates the event version, kind-specific after-state, and record shape.
    /// 校验事件版本、类型对应的后状态以及记录结构。
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.version != COLLABORATION_METADATA_VERSION {
            return Err(DomainError::InvalidStatistics {
                field: "collaboration_event.version",
            });
        }
        let valid_kind = matches!(
            (&self.event_kind, &self.after_state),
            (
                CollaborationEventKind::TaskResultProjected,
                CollaborationAfterState::TaskResultProjection { .. }
            ) | (
                CollaborationEventKind::TaskResultRecorded,
                CollaborationAfterState::TaskResult { .. }
            ) | (
                CollaborationEventKind::TaskResultAccepted,
                CollaborationAfterState::ReceivedTaskResult { .. }
            ) | (
                CollaborationEventKind::TaskResultAcknowledged,
                CollaborationAfterState::TaskResultAcknowledgement { .. }
            ) | (
                CollaborationEventKind::TaskMessageEffect,
                CollaborationAfterState::TaskMessageEffect { .. }
            ) | (
                CollaborationEventKind::TaskMessageSent
                    | CollaborationEventKind::TaskMessageAcknowledged,
                CollaborationAfterState::OutboundTaskMessage { .. }
            ) | (
                CollaborationEventKind::TaskMessageAccepted,
                CollaborationAfterState::InboundTaskMessage { .. }
            ) | (
                CollaborationEventKind::QuestionCreated
                    | CollaborationEventKind::QuestionAnswered
                    | CollaborationEventKind::QuestionCancelled,
                CollaborationAfterState::Question { .. }
            ) | (
                CollaborationEventKind::DeliveryRecorded,
                CollaborationAfterState::OutboundDelivery { .. }
            ) | (
                CollaborationEventKind::DeliveryAccepted,
                CollaborationAfterState::InboundDelivery { .. }
            ) | (
                CollaborationEventKind::DeliveryProjected,
                CollaborationAfterState::InboundDelivery { .. }
            ) | (
                CollaborationEventKind::DeliveryFailed,
                CollaborationAfterState::InboundDelivery { .. }
            ) | (
                CollaborationEventKind::DeliverySettled,
                CollaborationAfterState::InboundDelivery { .. }
            ) | (
                CollaborationEventKind::DeliverySettlementAcknowledged,
                CollaborationAfterState::OutboundDelivery { .. }
            ) | (
                CollaborationEventKind::NotificationSent
                    | CollaborationEventKind::NotificationProgress,
                CollaborationAfterState::OutboundNotification { .. }
            ) | (
                CollaborationEventKind::NotificationAccepted,
                CollaborationAfterState::InboundNotification { .. }
            ) | (
                CollaborationEventKind::NotificationProjected,
                CollaborationAfterState::InboundNotification { .. }
            ) | (
                CollaborationEventKind::NotificationFailed,
                CollaborationAfterState::InboundNotification { .. }
            ) | (
                CollaborationEventKind::AnswerDelivered,
                CollaborationAfterState::AnswerDelivery { .. }
            ) | (
                CollaborationEventKind::AnswerDeliveryFailed,
                CollaborationAfterState::AnswerDelivery { .. }
            ) | (
                CollaborationEventKind::ChildTaskReserved
                    | CollaborationEventKind::ChildTaskActivated
                    | CollaborationEventKind::ChildTaskTerminal
                    | CollaborationEventKind::ChildTaskNonSuccessRecorded
                    | CollaborationEventKind::ChildTaskNonSuccessProjected,
                CollaborationAfterState::ChildTask { .. }
            ) | (
                CollaborationEventKind::QuestionExecutionStarted
                    | CollaborationEventKind::QuestionExecutionCompleted
                    | CollaborationEventKind::QuestionSuccessorScheduled,
                CollaborationAfterState::QuestionExecution { .. }
            ) | (
                CollaborationEventKind::GrantAdded | CollaborationEventKind::GrantRemoved,
                CollaborationAfterState::Grant { .. }
            ) | (
                CollaborationEventKind::RunControlChanged,
                CollaborationAfterState::RunControl { .. }
            ) | (
                CollaborationEventKind::GenerationOwnershipCleared,
                CollaborationAfterState::GenerationOwnershipCleared
            )
        );
        if !valid_kind {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration_event.kind_after_state",
            });
        }
        if matches!(self.event_kind, CollaborationEventKind::DeliverySettled)
            && !matches!(
                &self.after_state,
                CollaborationAfterState::InboundDelivery {
                    delivery: CollaborationInboundDeliveryRecord {
                        settlement: Some(_),
                        ..
                    }
                }
            )
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration_event.delivery_settlement_after_state",
            });
        }
        if matches!(
            self.event_kind,
            CollaborationEventKind::DeliverySettlementAcknowledged
        ) && !matches!(
            &self.after_state,
            CollaborationAfterState::OutboundDelivery {
                delivery: CollaborationDeliveryRecord {
                    settlement_ack: Some(_),
                    ..
                }
            }
        ) {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration_event.delivery_settlement_ack_after_state",
            });
        }
        match &self.after_state {
            CollaborationAfterState::TaskResultProjection { projection } => {
                projection.validate()?;
                if projection.operation_id != self.operation_id {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "collaboration_event.task_result.projection_operation",
                    });
                }
            }
            CollaborationAfterState::ReceivedTaskResult { acceptance } => {
                acceptance.validate()?;
                if acceptance.operation_id != self.operation_id {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "collaboration_event.task_result.accept_operation",
                    });
                }
            }
            CollaborationAfterState::TaskResultAcknowledgement { acknowledgement } => {
                acknowledgement.validate()?;
                if acknowledgement.operation_id != self.operation_id {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "collaboration_event.task_result.ack_operation",
                    });
                }
            }
            CollaborationAfterState::TaskResult { result } => {
                result.validate()?;
                if result.operation_id != self.operation_id {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "collaboration_event.task_result.operation",
                    });
                }
            }
            CollaborationAfterState::TaskMessageEffect { effect } => {
                validate_timestamp(
                    "collaboration_event.task_message.effect_time",
                    effect.occurred_at_ms,
                )?;
                if effect.operation_id != self.operation_id || effect.target_revision == 0 {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "collaboration_event.task_message.effect_operation",
                    });
                }
            }
            CollaborationAfterState::OutboundTaskMessage { record } => {
                record.validate()?;
                if (self.event_kind == CollaborationEventKind::TaskMessageSent)
                    != record.acceptance.is_none()
                {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "collaboration_event.task_message.acceptance",
                    });
                }
            }
            CollaborationAfterState::InboundTaskMessage { acceptance } => acceptance.validate()?,
            CollaborationAfterState::Question { question, delivery } => {
                if question.delivery_id != delivery.delivery_id
                    || question.question_id != delivery.question_id
                {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "collaboration_event.question_delivery_binding",
                    });
                }
                question
                    .source
                    .validate("collaboration_event.question.source", true)?;
                question
                    .target
                    .validate("collaboration_event.question.target", false)?;
                validate_body("collaboration_event.question.body", &question.body)?;
                validate_delivery_record(delivery)?;
                let expected_state = match self.event_kind {
                    CollaborationEventKind::QuestionCreated => CollaborationQuestionState::Pending,
                    CollaborationEventKind::QuestionAnswered => {
                        CollaborationQuestionState::Answered
                    }
                    CollaborationEventKind::QuestionCancelled => {
                        CollaborationQuestionState::Cancelled
                    }
                    _ => unreachable!("validated question event kind"),
                };
                if question.state != expected_state {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "collaboration_event.question.state",
                    });
                }
            }
            CollaborationAfterState::OutboundDelivery { delivery } => {
                validate_delivery_record(delivery)?;
            }
            CollaborationAfterState::InboundDelivery { delivery } => {
                validate_inbound_delivery_record(delivery)?;
            }
            CollaborationAfterState::OutboundNotification { notification } => {
                validate_notification_record(notification)?;
            }
            CollaborationAfterState::InboundNotification { notification } => {
                validate_inbound_notification_record(notification)?;
            }
            CollaborationAfterState::AnswerDelivery { delivery, .. } => {
                if delivery.delivery_revision == 0 {
                    return Err(DomainError::InvalidStatistics {
                        field: "collaboration_event.answer_delivery.delivery_revision",
                    });
                }
                validate_timestamp(
                    "collaboration_event.answer_delivery.updated_at_ms",
                    delivery.updated_at_ms,
                )?;
                let shape_matches_kind = match self.event_kind {
                    CollaborationEventKind::AnswerDelivered => {
                        delivery.state == CollaborationAnswerDeliveryState::Delivered
                            && delivery.kind.is_some()
                    }
                    CollaborationEventKind::AnswerDeliveryFailed => {
                        delivery.state == CollaborationAnswerDeliveryState::Failed
                            && delivery.failure.is_some()
                    }
                    _ => false,
                };
                if !shape_matches_kind {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "collaboration_event.answer_delivery.state",
                    });
                }
            }
            CollaborationAfterState::ChildTask { task } => {
                validate_child_task_record(task)?;
                if self.event_kind == CollaborationEventKind::ChildTaskNonSuccessRecorded {
                    if task.non_success_projection.is_some()
                        || !task
                            .non_success_result
                            .as_ref()
                            .is_some_and(|result| result.operation_id == self.operation_id)
                    {
                        return Err(DomainError::CollaborationStateViolation {
                            field: "collaboration.child_result.event",
                        });
                    }
                } else if self.event_kind == CollaborationEventKind::ChildTaskNonSuccessProjected {
                    if !task
                        .non_success_projection
                        .as_ref()
                        .is_some_and(|projection| projection.operation_id == self.operation_id)
                    {
                        return Err(DomainError::CollaborationStateViolation {
                            field: "collaboration.child_result.projection_event",
                        });
                    }
                } else if task.non_success_result.is_some() {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "collaboration.child_result.lifecycle_event",
                    });
                }
            }
            CollaborationAfterState::QuestionExecution { execution } => {
                validate_question_execution_record(execution)?
            }
            CollaborationAfterState::Grant { grant } => validate_grant_record(grant)?,
            CollaborationAfterState::RunControl { transition } => {
                validate_timestamp(
                    "collaboration_event.run_control.changed_at_ms",
                    transition.changed_at_ms,
                )?;
            }
            CollaborationAfterState::GenerationOwnershipCleared => {}
        }
        Ok(())
    }

    /// Verifies that one persisted after-state is present in the complete metadata state.
    /// 校验持久化后状态已经存在于完整 metadata 状态中。
    pub fn matches_state(&self, state: &CollaborationState) -> bool {
        match &self.after_state {
            CollaborationAfterState::TaskResultProjection { projection } => {
                state.task_result_projections.get(&projection.delivery_id) == Some(projection)
            }
            CollaborationAfterState::TaskResult { result } => {
                state.task_results.get(&result.effect.delivery_id) == Some(result)
            }
            CollaborationAfterState::ReceivedTaskResult { acceptance } => {
                state
                    .received_task_results
                    .get(&acceptance.result.effect.delivery_id)
                    == Some(acceptance)
            }
            CollaborationAfterState::TaskResultAcknowledgement { acknowledgement } => {
                state
                    .task_result_acknowledgements
                    .get(&acknowledgement.acceptance.result.effect.delivery_id)
                    == Some(acknowledgement)
            }
            CollaborationAfterState::TaskMessageEffect { effect } => {
                state.task_message_effects.get(&effect.delivery_id) == Some(effect)
            }
            CollaborationAfterState::OutboundTaskMessage { record } => {
                state
                    .outbound_task_messages
                    .get(&record.message.delivery_id)
                    == Some(record)
            }
            CollaborationAfterState::InboundTaskMessage { acceptance } => {
                state
                    .inbound_task_messages
                    .get(&acceptance.message.delivery_id)
                    == Some(acceptance)
            }
            CollaborationAfterState::Question { question, delivery } => {
                state.questions.get(&question.question_id) == Some(question)
                    && state.outbound_deliveries.get(&delivery.delivery_id) == Some(delivery)
            }
            CollaborationAfterState::OutboundDelivery { delivery } => {
                state.outbound_deliveries.get(&delivery.delivery_id) == Some(delivery)
            }
            CollaborationAfterState::InboundDelivery { delivery } => {
                state.inbound_deliveries.get(&delivery.delivery_id) == Some(delivery)
            }
            CollaborationAfterState::OutboundNotification { notification } => {
                state.notifications.get(&notification.notification_id) == Some(notification)
            }
            CollaborationAfterState::InboundNotification { notification } => {
                state
                    .inbound_notifications
                    .get(&notification.notification_id)
                    == Some(notification)
            }
            CollaborationAfterState::AnswerDelivery {
                question_id,
                delivery,
            } => state
                .questions
                .get(question_id)
                .and_then(|question| question.answer.as_ref())
                .is_some_and(|answer| answer.delivery == *delivery),
            CollaborationAfterState::ChildTask { task } => {
                state.child_tasks.get(&task.execution_id) == Some(task)
            }
            CollaborationAfterState::QuestionExecution { execution } => {
                state.question_executions.get(&execution.execution_id) == Some(execution)
            }
            CollaborationAfterState::Grant { grant } => {
                state.grants.get(&grant.grant_id) == Some(grant)
            }
            CollaborationAfterState::RunControl { transition } => {
                state.run_control == transition.state
                    && state.run_control_history.last() == Some(transition)
            }
            CollaborationAfterState::GenerationOwnershipCleared => {
                state.questions.is_empty()
                    && state.outbound_deliveries.is_empty()
                    && state.inbound_deliveries.is_empty()
                    && state.notifications.is_empty()
                    && state.inbound_notifications.is_empty()
                    && state.outbound_task_messages.is_empty()
                    && state.inbound_task_messages.is_empty()
                    && state.task_message_effects.is_empty()
                    && state.task_results.is_empty()
                    && state.received_task_results.is_empty()
                    && state.task_result_acknowledgements.is_empty()
                    && state.task_result_projections.is_empty()
                    && state.child_tasks.is_empty()
                    && state.question_executions.is_empty()
            }
        }
    }
}

/// Typed semantic fingerprint for one idempotent operation.
/// 一次幂等操作的强类型语义指纹。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CollaborationOperationSemantic {
    /// Insert the already accepted complete result into its original source turn.
    /// 将已接纳完整结果插入原来源回合。
    ProjectTaskResult {
        /// Original task delivery resolved from local result acceptance.
        /// 由本地结果接纳解析的原任务投递。
        delivery_id: CollaborationDeliveryId,
    },
    /// Accept the exact executor result at its original requester.
    /// 在原请求者接纳精确执行者结果。
    AcceptTaskResult {
        /// Immutable authoritative result.
        /// 不可变权威结果。
        result: CollaborationTaskResult,
        /// Frozen disclosure decision for this requester receipt.
        /// 此请求者回执冻结的披露决策。
        disclosure: CollaborationTaskResultDisclosure,
    },
    /// Confirm the exact requester's receipt at the executor.
    /// 在执行者确认精确请求者回执。
    AcknowledgeTaskResult {
        /// Original receiver acceptance.
        /// 原接收方接纳。
        acceptance: CollaborationTaskResultAcceptance,
    },
    /// Freeze the actual completed business task bound to an accepted delivery.
    /// 冻结绑定已接纳投递的实际已结束业务任务。
    RecordTaskResult {
        /// Original task-message delivery identity.
        /// 原任务消息投递身份。
        delivery_id: CollaborationDeliveryId,
    },
    /// Claim a queued task into its own business turn exactly once.
    /// 将排队任务恰好认领到自己的业务回合一次。
    ClaimTaskMessage {
        /// Exact accepted task delivery.
        /// 精确已接纳任务投递。
        delivery_id: CollaborationDeliveryId,
    },
    /// Apply one accepted steering intent once.
    /// 将一条已接纳方向意图应用一次。
    ApplyTaskSteering {
        /// Exact accepted delivery.
        /// 精确已接纳投递。
        delivery_id: CollaborationDeliveryId,
    },
    /// Fail one accepted task intent before it is applied.
    /// 在应用前使一条已接纳任务意图失败。
    FailTaskMessage {
        /// Exact accepted delivery.
        /// 精确已接纳投递。
        delivery_id: CollaborationDeliveryId,
        /// Stable failure classification.
        /// 稳定失败分类。
        reason: CollaborationDeliveryFailureReason,
    },
    /// Source intent fingerprint, excluding retry observation time.
    /// 来源意图指纹，不包括重试观察时间。
    SendTaskMessage {
        /// Complete immutable task intent.
        /// 完整不可变任务意图。
        message: CollaborationTaskMessage,
    },
    /// Target acceptance fingerprint, including the entire immutable body and binding.
    /// 目标接纳指纹，包含完整不可变正文和绑定。
    AcceptTaskMessage {
        /// Complete immutable task intent.
        /// 完整不可变任务意图。
        message: CollaborationTaskMessage,
    },
    /// Exact target receipt acknowledged on the source.
    /// 来源确认的精确目标回执。
    AcknowledgeTaskMessage {
        /// Original target acceptance, never a recomputed receipt.
        /// 原始目标接纳，绝不重新计算回执。
        acceptance: CollaborationTaskMessageAcceptance,
    },
    /// Question creation fingerprint.
    /// 问题创建语义指纹。
    CreateQuestion {
        /// Stable question identity.
        /// 稳定问题身份。
        question_id: CollaborationQuestionId,
        /// Stable delivery identity.
        /// 稳定投递身份。
        delivery_id: CollaborationDeliveryId,
        /// Source participant.
        /// 来源参与者。
        source: CollaborationParticipant,
        /// Target participant.
        /// 目标参与者。
        target: CollaborationParticipant,
        /// Question body.
        /// 问题正文。
        body: String,
        /// Requested target execution purpose.
        /// 请求的目标执行用途。
        execution_purpose: CollaborationExecutionPurpose,
    },
    /// Delivery acceptance fingerprint.
    /// 投递接纳语义指纹。
    AcceptDelivery {
        /// Delivery identity.
        /// 投递身份。
        delivery_id: CollaborationDeliveryId,
        /// Question identity.
        /// 问题身份。
        question_id: CollaborationQuestionId,
        /// Source locator.
        /// 来源定位。
        source: CollaborationParticipant,
        /// Target locator.
        /// 目标定位。
        target: CollaborationParticipant,
        /// Bound target turn.
        /// 绑定目标回合。
        bound_turn_id: Option<TurnId>,
        /// Target execution purpose.
        /// 目标执行用途。
        execution_purpose: CollaborationExecutionPurpose,
    },
    /// Source delivery-progress fingerprint.
    /// 来源投递进度语义指纹。
    DeliveryProgress {
        /// Delivery identity.
        /// 投递身份。
        delivery_id: CollaborationDeliveryId,
        /// Question identity.
        /// 问题身份。
        question_id: CollaborationQuestionId,
        /// New delivery state.
        /// 新投递状态。
        state: CollaborationDeliveryState,
        /// Failure reason.
        /// 失败原因。
        failure: Option<CollaborationDeliveryFailureReason>,
        /// Target revision observed by source.
        /// 来源观察到的目标修订。
        target_revision: Option<u64>,
        /// Target event seq observed by source.
        /// 来源观察到的目标事件 seq。
        target_event_seq: Option<Seq>,
    },
    /// Target projection fingerprint.
    /// 目标投影语义指纹。
    ProjectDelivery {
        /// Delivery identity.
        /// 投递身份。
        delivery_id: CollaborationDeliveryId,
        /// Question identity.
        /// 问题身份。
        question_id: CollaborationQuestionId,
        /// Projected event seq.
        /// 投影事件 seq。
        projected_seq: Seq,
        /// Stable UserInput identity at the projection event.
        /// 鎶曞奖浜嬩欢涓殑绋冲畾 UserInput 韬唤銆?
        projected_input_id: PendingInputId,
        /// Logical turn carried by the physical projection event.
        /// 鐗╃悊鎶曞奖浜嬩欢鎼哄甫鐨勯€昏緫鍥炲悎銆?
        projected_turn_id: TurnId,
    },
    /// Target delivery failure fingerprint.
    /// 目标投递失败语义指纹。
    FailDelivery {
        /// Delivery identity.
        /// 投递身份。
        delivery_id: CollaborationDeliveryId,
        /// Question identity.
        /// 问题身份。
        question_id: CollaborationQuestionId,
        /// Failure reason.
        /// 失败原因。
        reason: CollaborationDeliveryFailureReason,
    },
    /// Target settlement acknowledgement fingerprint.
    /// 鐩爣缁堟€佺粨绠楃‘璁ょ殑璇箟鎸囩汗銆?
    SettleInboundDelivery {
        /// Delivery identity.
        /// 鎶曢€掕韩浠姐€?
        delivery_id: CollaborationDeliveryId,
        /// Question identity.
        /// 闂韬唤銆?
        question_id: CollaborationQuestionId,
        /// Source operation that won settlement.
        /// 璧㈡簮缁撶畻鑳滃嚭鐨勬潵婧愭搷浣溿€?
        source_operation_id: CollaborationOperationId,
        /// Winning terminal settlement.
        /// 鑳滃嚭鐨勭粓鎬佺粨绠楃姸鎬併€?
        settlement: CollaborationTargetSettlementState,
        /// Source event seq that established settlement.
        /// 寤虹珛缁撶畻鐨勬潵婧愪簨浠?seq銆?
        source_event_seq: Seq,
    },
    /// Source acknowledgement of a target terminal settlement.
    /// 鏉ユ簮纭鐩爣缁堟€佺粨绠楃殑璇箟鎸囩汗銆?
    AcknowledgeDeliverySettlement {
        /// Delivery identity.
        /// 鎶曢€掕韩浠姐€?
        delivery_id: CollaborationDeliveryId,
        /// Question identity.
        /// 闂韬唤銆?
        question_id: CollaborationQuestionId,
        /// Winning terminal settlement.
        /// 鑳滃嚭鐨勭粓鎬佺粨绠楃姸鎬併€?
        settlement: CollaborationTargetSettlementState,
        /// Source event seq that established settlement.
        /// 寤虹珛缁撶畻鐨勬潵婧愪簨浠?seq銆?
        source_event_seq: Seq,
        /// Target event seq that recorded acknowledgement.
        /// 璁板綍鐩爣纭鐨勭洰鏍囦簨浠?seq銆?
        target_event_seq: Seq,
    },
    /// One-way notification creation fingerprint.
    /// 单向通知创建语义指纹。
    SendNotification {
        /// Stable notification identity.
        /// 稳定通知身份。
        notification_id: CollaborationNotificationId,
        /// Stable delivery identity.
        /// 稳定投递身份。
        delivery_id: CollaborationDeliveryId,
        /// Source participant.
        /// 来源参与者。
        source: CollaborationParticipant,
        /// Target participant.
        /// 目标参与者。
        target: CollaborationParticipant,
        /// Exact notification body.
        /// 精确通知正文。
        body: String,
    },
    /// Source notification progress fingerprint.
    /// 来源通知进度语义指纹。
    NotificationProgress {
        /// Stable notification identity.
        /// 稳定通知身份。
        notification_id: CollaborationNotificationId,
        /// Stable delivery identity.
        /// 稳定投递身份。
        delivery_id: CollaborationDeliveryId,
        /// New source-side state.
        /// 新的来源侧状态。
        state: CollaborationNotificationState,
        /// Stable failure reason when failed.
        /// 失败时的稳定失败原因。
        failure: Option<CollaborationDeliveryFailureReason>,
        /// Target revision observed by the source.
        /// 来源观察到的目标修订。
        target_revision: Option<u64>,
        /// Target event sequence observed by the source.
        /// 来源观察到的目标事件序号。
        target_event_seq: Option<Seq>,
    },
    /// One-way notification acceptance fingerprint.
    /// 单向通知接纳语义指纹。
    AcceptNotification {
        /// Stable notification identity.
        /// 稳定通知身份。
        notification_id: CollaborationNotificationId,
        /// Stable delivery identity.
        /// 稳定投递身份。
        delivery_id: CollaborationDeliveryId,
        /// Source operation identity copied from the source outbox.
        /// 从来源出站箱复制的来源操作身份。
        source_operation_id: CollaborationOperationId,
        /// Source participant copied from the source authority.
        /// 从来源权威复制的来源参与者。
        source: CollaborationParticipant,
        /// Target participant admitted by the target host.
        /// 目标宿主准入的目标参与者。
        target: CollaborationParticipant,
    },
    /// One-way notification projection fingerprint.
    /// 单向通知投影语义指纹。
    ProjectNotification {
        /// Stable notification identity.
        /// 稳定通知身份。
        notification_id: CollaborationNotificationId,
        /// Stable delivery identity.
        /// 稳定投递身份。
        delivery_id: CollaborationDeliveryId,
        /// Physical projection event sequence.
        /// 物理投影事件序号。
        projected_seq: Seq,
        /// Stable projected UserInput identity.
        /// 稳定投影 UserInput 身份。
        projected_input_id: PendingInputId,
        /// Logical turn carrying the projection.
        /// 承载投影的逻辑回合。
        projected_turn_id: TurnId,
    },
    /// One-way notification failure fingerprint.
    /// 单向通知失败语义指纹。
    FailNotification {
        /// Stable notification identity.
        /// 稳定通知身份。
        notification_id: CollaborationNotificationId,
        /// Stable delivery identity.
        /// 稳定投递身份。
        delivery_id: CollaborationDeliveryId,
        /// Stable failure reason.
        /// 稳定失败原因。
        reason: CollaborationDeliveryFailureReason,
    },
    /// Answer settlement fingerprint.
    /// 答案结算语义指纹。
    ReplyQuestion {
        /// Question identity.
        /// 问题身份。
        question_id: CollaborationQuestionId,
        /// Responder participant.
        /// 回答参与者。
        responder: CollaborationParticipant,
        /// Answer body.
        /// 答案正文。
        body: String,
    },
    /// Cancellation fingerprint.
    /// 撤销语义指纹。
    CancelQuestion {
        /// Question identity.
        /// 问题身份。
        question_id: CollaborationQuestionId,
        /// Requester participant.
        /// 请求撤销的参与者。
        requester: CollaborationParticipant,
        /// Cancellation reason.
        /// 撤销原因。
        reason: String,
    },
    /// Independent answer-delivery fingerprint.
    /// 独立答案交付语义指纹。
    DeliverAnswer {
        /// Question identity.
        /// 问题身份。
        question_id: CollaborationQuestionId,
        /// Delivery path.
        /// 交付路径。
        delivery_kind: CollaborationAnswerDeliveryKind,
        /// Optional projected event seq.
        /// 可选投影事件 seq。
        projected_seq: Option<Seq>,
        /// Stable UserInput identity for User-event delivery.
        /// User 浜嬩欢浜や粯鏃剁殑绋冲畾 UserInput 韬唤銆?
        projected_input_id: Option<PendingInputId>,
        /// Stable tool call identity for Tool-result delivery.
        /// ToolResult 浜や粯鏃剁殑绋冲畾宸ュ叿璋冪敤韬唤銆?
        tool_call_id: Option<ToolCallId>,
        /// Physical ToolCall event seq for Tool-result delivery.
        /// ToolResult 浜や粯鎵€鐢ㄧ殑鐗╃悊 ToolCall 浜嬩欢 seq銆?
        tool_result_seq: Option<Seq>,
    },
    /// Failed answer-delivery fingerprint.
    /// 失败答案交付语义指纹。
    FailAnswerDelivery {
        /// Question identity.
        /// 问题身份。
        question_id: CollaborationQuestionId,
        /// Stable failure reason.
        /// 稳定失败原因。
        reason: CollaborationDeliveryFailureReason,
    },
    /// Child reservation fingerprint.
    /// 子任务预留语义指纹。
    ReserveChildTask {
        /// Canonical authenticated source tool name.
        /// 已认证来源工具的规范名称。
        source_tool_name: String,
        /// Exact child execution identity.
        /// 精确子执行身份。
        execution_id: CollaborationExecutionId,
        /// Child task identity.
        /// 子任务身份。
        task_id: SubagentTaskId,
        /// Parent turn identity.
        /// 父回合身份。
        parent_turn_id: TurnId,
        /// Child participant.
        /// 子参与者。
        child: CollaborationParticipant,
    },
    /// Child activation fingerprint.
    /// 子任务激活语义指纹。
    ActivateChildTask {
        /// Exact child execution identity.
        /// 精确子执行身份。
        execution_id: CollaborationExecutionId,
        /// Child task identity.
        /// 子任务身份。
        task_id: SubagentTaskId,
    },
    /// Child terminal fingerprint.
    /// 子任务终态语义指纹。
    TerminalChildTask {
        /// Exact child execution identity.
        /// 精确子执行身份。
        execution_id: CollaborationExecutionId,
        /// Child task identity.
        /// 子任务身份。
        task_id: SubagentTaskId,
        /// Terminal outcome.
        /// 终态结果。
        outcome: CollaborationChildTaskTerminalOutcome,
    },
    /// Immutable child result fingerprint, including its authoritative source proof.
    /// 不可变子结果指纹，包含权威来源凭据。
    RecordChildTaskNonSuccess {
        /// Exact child execution independently of reusable session identity.
        /// 独立于可复用会话身份的精确子执行。
        execution_id: CollaborationExecutionId,
        /// Complete validated source facts retained for exact conflict detection.
        /// 用于精确冲突检测的完整已校验来源事实。
        proof: CollaborationChildTaskNonSuccessProof,
    },
    /// Exact local non-success result selected for complete model projection.
    /// 为完整模型投影选定的精确本地非成功结果。
    ProjectChildTaskNonSuccess {
        /// Original child execution.
        /// 原子执行。
        execution_id: CollaborationExecutionId,
        /// Immutable parent report event.
        /// 不可变父报告事件。
        result_seq: Seq,
    },
    /// Question-execution start fingerprint.
    /// 问题执行启动语义指纹。
    BeginQuestionExecution {
        /// Execution identity.
        /// 执行身份。
        execution_id: CollaborationExecutionId,
        /// Question identity.
        /// 问题身份。
        question_id: CollaborationQuestionId,
        /// Target participant.
        /// 目标参与者。
        participant: CollaborationParticipant,
        /// Dedicated execution purpose.
        /// 专用执行用途。
        purpose: CollaborationExecutionPurpose,
        /// Exact retained parent turn, included in idempotency comparison.
        /// 精确保留父回合，纳入幂等比较。
        parent_turn_id: Option<TurnId>,
    },
    /// Question-execution completion fingerprint.
    /// 问题执行完成语义指纹。
    CompleteQuestionExecution {
        /// Execution identity.
        /// 执行身份。
        execution_id: CollaborationExecutionId,
        /// Execution outcome.
        /// 执行结果。
        outcome: CollaborationQuestionExecutionOutcome,
    },
    /// Stable identity of a post-auxiliary queued-user scheduling operation.
    /// 辅助结束后排队用户调度操作的稳定身份。
    ScheduleQuestionSuccessor {
        /// Completed auxiliary execution that permits scheduling.
        /// 允许调度的已完成辅助执行。
        execution_id: CollaborationExecutionId,
    },
    /// Run-control fingerprint.
    /// 运行控制语义指纹。
    SetRunControl {
        /// Requested run-control state.
        /// 请求的运行控制状态。
        state: CollaborationRunControl,
    },
    /// Grant-add fingerprint.
    /// 增加授权语义指纹。
    AddGrant {
        /// Grant identity.
        /// 授权身份。
        grant_id: CollaborationGrantId,
        /// Authority that admitted the grant.
        /// 准入该授权的权威主体。
        authority: CollaborationGrantAuthority,
        /// Source session scope.
        /// 来源会话范围。
        source_session_id: String,
        /// Target session scope.
        /// 目标会话范围。
        target_session_id: String,
        /// Granted actions.
        /// 授予动作集合。
        actions: BTreeSet<CollaborationGrantAction>,
    },
    /// Grant-remove fingerprint.
    /// 移除授权语义指纹。
    RemoveGrant {
        /// Grant identity.
        /// 授权身份。
        grant_id: CollaborationGrantId,
    },
    /// Generation ownership-reset fingerprint.
    /// 代际所有权重置语义指纹。
    ClearGenerationOwnership,
}

/// Stable original result retained for operation retries.
/// 为操作重试保留的稳定原始结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CollaborationOperationResult {
    /// Original immutable projection proof returned across retries.
    /// 重试时返回的原始不可变投影凭据。
    TaskResultProjected {
        /// Exact input and publication coordinates.
        /// 精确输入及发布坐标。
        projection: CollaborationTaskResultProjection,
    },
    /// Immutable requester acceptance returned by this operation.
    /// 此操作返回的不可变请求者接纳。
    TaskResultAccepted {
        /// Original receipt, unchanged by later projection.
        /// 原始回执，不随后续投影改变。
        acceptance: CollaborationTaskResultAcceptance,
    },
    /// Immutable executor confirmation returned by this operation.
    /// 此操作返回的不可变执行者确认。
    TaskResultAcknowledged {
        /// Exact internal delivery confirmation.
        /// 精确内部投递确认。
        acknowledgement: CollaborationTaskResultAcknowledgement,
    },
    /// Original immutable task result returned on every retry.
    /// 每次重试返回的原始不可变任务结果。
    TaskResult {
        /// Exact target-owned final result.
        /// 精确目标所有最终结果。
        result: CollaborationTaskResult,
    },
    /// Original immutable target effect.
    /// 原始不可变目标效果。
    TaskMessageEffect {
        /// Exact application or failure receipt.
        /// 精确应用或失败回执。
        effect: CollaborationTaskMessageEffect,
    },
    /// Original source creation or acknowledgement result.
    /// 原始来源创建或确认结果。
    TaskMessage {
        /// Record at the original operation revision.
        /// 原操作修订时的记录。
        record: CollaborationTaskMessageRecord,
    },
    /// Original target acceptance result.
    /// 原始目标接纳结果。
    TaskMessageAcceptance {
        /// Immutable target receipt.
        /// 不可变目标回执。
        acceptance: CollaborationTaskMessageAcceptance,
    },
    /// Question and delivery identities returned by creation.
    /// 创建操作返回的问题与投递身份。
    Question {
        /// Question identity.
        /// 问题身份。
        question_id: CollaborationQuestionId,
        /// Delivery identity.
        /// 投递身份。
        delivery_id: CollaborationDeliveryId,
    },
    /// Delivery state result.
    /// 投递状态结果。
    Delivery {
        /// Delivery identity.
        /// 投递身份。
        delivery_id: CollaborationDeliveryId,
        /// Delivery state at the operation boundary.
        /// 操作边界处的投递状态。
        state: CollaborationDeliveryState,
    },
    /// Target settlement result with the terminal acknowledgement state.
    /// 鐩爣缁堟€佺粨绠楃‘璁ょ粨鏋溿€?
    DeliverySettlement {
        /// Delivery identity.
        /// 鎶曢€掕韩浠姐€?
        delivery_id: CollaborationDeliveryId,
        /// Winning terminal settlement.
        /// 鑳滃嚭鐨勭粓鎬佺粨绠楃姸鎬併€?
        state: CollaborationTargetSettlementState,
    },
    /// Source notification creation result.
    /// 来源通知创建结果。
    Notification {
        /// Complete source snapshot returned by the original operation.
        /// 原始操作返回的完整来源快照。
        notification: CollaborationNotificationRecord,
    },
    /// Notification acceptance, projection, or failure result.
    /// 通知接纳、投影或失败结果。
    NotificationDelivery {
        /// Complete snapshot returned by the original operation.
        /// 原始操作返回的完整快照。
        snapshot: CollaborationNotificationOperationSnapshot,
    },
    /// Question settlement result.
    /// 问题结算结果。
    Settlement {
        /// Question identity.
        /// 问题身份。
        question_id: CollaborationQuestionId,
        /// Winning question state.
        /// 胜出的问题状态。
        state: CollaborationQuestionState,
    },
    /// Answer-delivery result.
    /// 答案交付结果。
    AnswerDelivery {
        /// Question identity.
        /// 问题身份。
        question_id: CollaborationQuestionId,
        /// Independent answer delivery revision.
        /// 独立答案交付修订。
        delivery_revision: u64,
    },
    /// Child-task state result.
    /// 子任务状态结果。
    ChildTask {
        /// Exact child execution identity.
        /// 精确子执行身份。
        execution_id: CollaborationExecutionId,
        /// Child task identity.
        /// 子任务身份。
        task_id: SubagentTaskId,
        /// Child state at the operation boundary.
        /// 操作边界处的子任务状态。
        state: CollaborationChildTaskState,
    },
    /// Question-execution state result.
    /// 问题执行状态结果。
    QuestionExecution {
        /// Immutable execution snapshot at this operation's committed boundary.
        /// 当前操作已提交边界处的不可变执行快照。
        execution: CollaborationQuestionExecutionRecord,
    },
    /// Run-control result.
    /// 运行控制结果。
    RunControl {
        /// Current run-control state.
        /// 当前运行控制状态。
        state: CollaborationRunControl,
    },
    /// Grant result.
    /// 授权结果。
    Grant {
        /// Grant identity.
        /// 授权身份。
        grant_id: CollaborationGrantId,
    },
}

/// Persisted operation identity, semantic fingerprint, and original result.
/// 持久化的操作身份、语义指纹与原始结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationOperationRecord {
    /// Stable operation identity.
    /// 稳定操作身份。
    pub operation_id: CollaborationOperationId,
    /// Exact operation semantic fingerprint.
    /// 精确操作语义指纹。
    pub semantic: CollaborationOperationSemantic,
    /// Collaboration event seq committed by the original operation.
    /// 原始操作提交的协作事件 seq。
    pub event_seq: Seq,
    /// Original result returned to the caller.
    /// 返回给调用方的原始结果。
    pub result: CollaborationOperationResult,
}

impl CollaborationOperationRecord {
    /// Validates the operation record shape.
    /// 校验操作记录结构。
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.operation_id.as_str().is_empty() {
            return Err(DomainError::InvalidStatistics {
                field: "collaboration.operation.operation_id",
            });
        }
        if self.event_seq.value() == 0 {
            return Err(DomainError::InvalidStatistics {
                field: "collaboration.operation.event_seq",
            });
        }
        Ok(())
    }
}

/// Original notification snapshot retained for exact operation retries.
/// 为精确操作重试保留的通知原始快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CollaborationNotificationOperationSnapshot {
    /// Source outbox snapshot committed by a send or progress operation.
    /// 发送或进度操作提交的来源出站箱快照。
    Outbound {
        /// Complete source notification record at the operation boundary.
        /// 操作边界处的完整来源通知记录。
        notification: CollaborationNotificationRecord,
    },
    /// Target snapshot committed by an accept, projection, or failure operation.
    /// 接纳、投影或失败操作提交的目标通知快照。
    Inbound {
        /// Complete target notification record at the operation boundary.
        /// 操作边界处的完整目标通知记录。
        notification: CollaborationInboundNotificationRecord,
    },
}

/// Metadata-level recovery summary plus the last event coverage marker.
/// 带最后事件覆盖标记的 metadata 级恢复摘要。
pub type CollaborationRecoveryState = CollaborationMetadata;

/// Input used to create a question and its source-owned delivery record.
/// 创建问题及来源拥有投递记录的输入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateQuestionInput {
    /// Stable operation idempotency identity.
    /// 稳定操作幂等身份。
    pub operation_id: CollaborationOperationId,
    /// Stable question identity.
    /// 稳定问题身份。
    pub question_id: CollaborationQuestionId,
    /// Stable delivery identity.
    /// 稳定投递身份。
    pub delivery_id: CollaborationDeliveryId,
    /// Host-owned source participant.
    /// 宿主拥有的来源参与者。
    pub source: CollaborationParticipant,
    /// Host-owned target participant.
    /// 宿主拥有的目标参与者。
    pub target: CollaborationParticipant,
    /// Exact untrusted question body.
    /// 精确的不可信问题正文。
    pub body: String,
    /// Target execution purpose.
    /// 目标执行用途。
    pub execution_purpose: CollaborationExecutionPurpose,
    /// Semantic creation time in Unix milliseconds.
    /// 语义创建时间（Unix 毫秒）。
    pub created_at_ms: i64,
}

/// Input used by a target document to accept one source delivery.
/// 目标文档接纳一条来源投递的输入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptDeliveryInput {
    /// Stable target-side operation identity.
    /// 稳定目标侧操作身份。
    pub operation_id: CollaborationOperationId,
    /// Source delivery identity.
    /// 来源投递身份。
    pub delivery_id: CollaborationDeliveryId,
    /// Source question identity.
    /// 来源问题身份。
    pub question_id: CollaborationQuestionId,
    /// Source participant locator copied from the source authority.
    /// 从来源权威复制的来源参与者定位。
    pub source: CollaborationParticipant,
    /// Target participant captured by the admitting host.
    /// 准入宿主捕获的目标参与者。
    pub target: CollaborationParticipant,
    /// Optional target turn receiving the eventual projection.
    /// 接收最终投影的可选目标回合。
    pub bound_turn_id: Option<TurnId>,
    /// Target execution purpose.
    /// 目标执行用途。
    pub execution_purpose: CollaborationExecutionPurpose,
    /// Semantic acceptance time in Unix milliseconds.
    /// 语义接纳时间（Unix 毫秒）。
    pub accepted_at_ms: i64,
}

/// Input used by the source document to record target-side progress.
/// 来源文档记录目标侧进度的输入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordDeliveryProgressInput {
    /// Stable source-side acknowledgement operation identity.
    /// 稳定来源侧确认操作身份。
    pub operation_id: CollaborationOperationId,
    /// Source delivery identity.
    /// 来源投递身份。
    pub delivery_id: CollaborationDeliveryId,
    /// Source question identity.
    /// 来源问题身份。
    pub question_id: CollaborationQuestionId,
    /// New sender-side delivery state.
    /// 新的发送侧投递状态。
    pub state: CollaborationDeliveryState,
    /// Failure reason when state is Failed.
    /// 状态为 Failed 时的失败原因。
    pub failure: Option<CollaborationDeliveryFailureReason>,
    /// Target document revision observed by the source.
    /// 来源观察到的目标文档修订。
    pub target_revision: Option<u64>,
    /// Target event seq observed by the source.
    /// 来源观察到的目标事件 seq。
    pub target_event_seq: Option<Seq>,
    /// Semantic acknowledgement time in Unix milliseconds.
    /// 语义确认时间（Unix 毫秒）。
    pub updated_at_ms: i64,
}

/// Input used by a target document to mark a delivery projected.
/// 目标文档标记投递已投影的输入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectDeliveryInput {
    /// Stable target-side projection operation identity.
    /// 稳定目标侧投影操作身份。
    pub operation_id: CollaborationOperationId,
    /// Delivery identity.
    /// 投递身份。
    pub delivery_id: CollaborationDeliveryId,
    /// Question identity.
    /// 问题身份。
    pub question_id: CollaborationQuestionId,
    /// Projected event seq retained by the target.
    /// 目标保留的投影事件 seq。
    pub projected_seq: Seq,
    /// Stable UserInput identity at the projection event.
    /// 鎶曞奖浜嬩欢涓殑绋冲畾 UserInput 韬唤銆?
    pub projected_input_id: PendingInputId,
    /// Logical turn carried by the physical projection event.
    /// 鐗╃悊鎶曞奖浜嬩欢鎼哄甫鐨勯€昏緫鍥炲悎銆?
    pub projected_turn_id: TurnId,
    /// Semantic projection time in Unix milliseconds.
    /// 语义投影时间（Unix 毫秒）。
    pub projected_at_ms: i64,
}

/// Input used by a target document to record delivery failure.
/// 目标文档记录投递失败的输入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FailDeliveryInput {
    /// Stable target-side failure operation identity.
    /// 稳定目标侧失败操作身份。
    pub operation_id: CollaborationOperationId,
    /// Delivery identity.
    /// 投递身份。
    pub delivery_id: CollaborationDeliveryId,
    /// Question identity.
    /// 问题身份。
    pub question_id: CollaborationQuestionId,
    /// Stable failure reason.
    /// 稳定失败原因。
    pub reason: CollaborationDeliveryFailureReason,
    /// Semantic failure time in Unix milliseconds.
    /// 语义失败时间（Unix 毫秒）。
    pub failed_at_ms: i64,
}

/// Input used by the source to create a typed one-way notification.
/// 来源创建一条强类型单向通知的输入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SendNotificationInput {
    /// Stable source operation identity.
    /// 稳定来源操作身份。
    pub operation_id: CollaborationOperationId,
    /// Stable notification identity.
    /// 稳定通知身份。
    pub notification_id: CollaborationNotificationId,
    /// Stable delivery identity.
    /// 稳定投递身份。
    pub delivery_id: CollaborationDeliveryId,
    /// Source participant captured from the current execution.
    /// 从当前执行捕获的来源参与者。
    pub source: CollaborationParticipant,
    /// Target participant admitted by the host.
    /// 宿主准入的目标参与者。
    pub target: CollaborationParticipant,
    /// Exact notification body.
    /// 精确通知正文。
    pub body: String,
    /// Semantic creation time in Unix milliseconds.
    /// 语义创建时间（Unix 毫秒）。
    pub created_at_ms: i64,
}

/// Input used by the source to record target-side notification progress.
/// 来源记录目标侧通知进度的输入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordNotificationProgressInput {
    /// Stable source operation identity.
    /// 稳定来源操作身份。
    pub operation_id: CollaborationOperationId,
    /// Stable notification identity.
    /// 稳定通知身份。
    pub notification_id: CollaborationNotificationId,
    /// Stable delivery identity.
    /// 稳定投递身份。
    pub delivery_id: CollaborationDeliveryId,
    /// New source-side notification state.
    /// 新的来源侧通知状态。
    pub state: CollaborationNotificationState,
    /// Stable failure reason when state is Failed.
    /// 状态为 Failed 时的稳定失败原因。
    pub failure: Option<CollaborationDeliveryFailureReason>,
    /// Target document revision observed by the source.
    /// 来源观察到的目标文档修订。
    pub target_revision: Option<u64>,
    /// Target event sequence observed by the source.
    /// 来源观察到的目标事件序号。
    pub target_event_seq: Option<Seq>,
    /// Semantic progress time in Unix milliseconds.
    /// 语义进度时间（Unix 毫秒）。
    pub updated_at_ms: i64,
}

/// Input used by the target to accept one typed one-way notification.
/// 目标接纳一条强类型单向通知的输入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptNotificationInput {
    /// Stable target operation identity.
    /// 稳定目标操作身份。
    pub operation_id: CollaborationOperationId,
    /// Stable notification identity.
    /// 稳定通知身份。
    pub notification_id: CollaborationNotificationId,
    /// Stable delivery identity.
    /// 稳定投递身份。
    pub delivery_id: CollaborationDeliveryId,
    /// Source operation identity copied from the source outbox.
    /// 从来源出站箱复制的来源操作身份。
    pub source_operation_id: CollaborationOperationId,
    /// Source participant copied from the source authority.
    /// 从来源权威复制的来源参与者。
    pub source: CollaborationParticipant,
    /// Target participant captured by the target host.
    /// 目标宿主捕获的目标参与者。
    pub target: CollaborationParticipant,
    /// Exact source body copied after host verification.
    /// 宿主校验后复制的精确来源正文。
    pub body: String,
    /// Semantic acceptance time in Unix milliseconds.
    /// 语义接纳时间（Unix 毫秒）。
    pub accepted_at_ms: i64,
}

/// Input used by the target to prove one notification projection.
/// 目标证明一条通知投影的输入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectNotificationInput {
    /// Stable target operation identity.
    /// 稳定目标操作身份。
    pub operation_id: CollaborationOperationId,
    /// Stable notification identity.
    /// 稳定通知身份。
    pub notification_id: CollaborationNotificationId,
    /// Stable delivery identity.
    /// 稳定投递身份。
    pub delivery_id: CollaborationDeliveryId,
    /// Physical Provider UserInput event sequence.
    /// 物理 Provider UserInput 事件序号。
    pub projected_seq: Seq,
    /// Stable UserInput identity proved by that event.
    /// 该事件证明的稳定 UserInput 身份。
    pub projected_input_id: PendingInputId,
    /// Logical turn carried by the projection event.
    /// 投影事件携带的逻辑回合。
    pub projected_turn_id: TurnId,
    /// Semantic projection time in Unix milliseconds.
    /// 语义投影时间（Unix 毫秒）。
    pub projected_at_ms: i64,
}

/// Input used by the target to record one notification delivery failure.
/// 目标记录一条通知投递失败的输入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FailNotificationInput {
    /// Stable target operation identity.
    /// 稳定目标操作身份。
    pub operation_id: CollaborationOperationId,
    /// Stable notification identity.
    /// 稳定通知身份。
    pub notification_id: CollaborationNotificationId,
    /// Stable delivery identity.
    /// 稳定投递身份。
    pub delivery_id: CollaborationDeliveryId,
    /// Stable failure reason.
    /// 稳定失败原因。
    pub reason: CollaborationDeliveryFailureReason,
    /// Semantic failure time in Unix milliseconds.
    /// 语义失败时间（Unix 毫秒）。
    pub failed_at_ms: i64,
}

/// Input used by the target to record a source-owned terminal settlement.
/// 鐩爣鏂囨。璁板綍鏉ユ簮鎷ユ湁缁堟€佺粨绠楃殑杈撳叆銆?
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettleInboundDeliveryInput {
    /// Stable target-side settlement acknowledgement operation identity.
    /// 绋冲畾鐩爣渚х粨绠楃‘璁ゆ搷浣滆韩浠姐€?
    pub operation_id: CollaborationOperationId,
    /// Source delivery identity.
    /// 鏉ユ簮鎶曢€掕韩浠姐€?
    pub delivery_id: CollaborationDeliveryId,
    /// Source question identity.
    /// 鏉ユ簮闂韬唤銆?
    pub question_id: CollaborationQuestionId,
    /// Source settlement operation that won the question.
    /// 鍦ㄦ潵婧愮粨绠楃珯鑳滃嚭鐨勬潵婧愭搷浣滄搷浣溿€?
    pub source_operation_id: CollaborationOperationId,
    /// Winning terminal settlement state.
    /// 鑳滃嚭鐨勭粓鎬佺粨绠楃姸鎬併€?
    pub settlement: CollaborationTargetSettlementState,
    /// Source event seq that established the winning settlement.
    /// 寤虹珛鑳滃嚭缁撶畻鐨勬潵婧愪簨浠?seq銆?
    pub source_event_seq: Seq,
    /// Semantic acknowledgement time in Unix milliseconds.
    /// 璇箟纭鏃堕棿锛圲nix 姣锛夈€?
    pub acknowledged_at_ms: i64,
}

/// Input used by the source to persist target settlement acknowledgement.
/// 鏉ユ簮鏂囨。鎸佷箙鍖栫洰鏍囩粨绠楃‘璁ょ殑杈撳叆銆?
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcknowledgeDeliverySettlementInput {
    /// Stable source-side acknowledgement operation identity.
    /// 绋冲畾鏉ユ簮渚у洖鏀剁‘璁ゆ搷浣滆韩浠姐€?
    pub operation_id: CollaborationOperationId,
    /// Source delivery identity.
    /// 鏉ユ簮鎶曢€掕韩浠姐€?
    pub delivery_id: CollaborationDeliveryId,
    /// Source question identity.
    /// 鏉ユ簮闂韬唤銆?
    pub question_id: CollaborationQuestionId,
    /// Terminal settlement observed on the target.
    /// 鐩爣宸茶瑙夌殑缁堟€佺粨绠楃姸鎬併€?
    pub settlement: CollaborationTargetSettlementState,
    /// Source event seq that established the winning settlement.
    /// 寤虹珛鑳滃嚭缁撶畻鐨勬潵婧愪簨浠?seq銆?
    pub source_event_seq: Seq,
    /// Target event seq that recorded the acknowledgement.
    /// 璁板綍鐩爣纭鐨勭洰鏍囦簨浠?seq銆?
    pub target_event_seq: Seq,
    /// Target document revision at acknowledgement.
    /// 鐩爣纭鏃剁殑鏂囨。淇鍙枫€?
    pub target_revision: u64,
    /// Semantic acknowledgement time in Unix milliseconds.
    /// 璇箟纭鏃堕棿锛圲nix 姣锛夈€?
    pub acknowledged_at_ms: i64,
}

/// Input used by the authorized target participant to answer a question.
/// 获授权目标参与者回答问题的输入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplyQuestionInput {
    /// Stable settlement operation identity.
    /// 稳定结算操作身份。
    pub operation_id: CollaborationOperationId,
    /// Question identity.
    /// 问题身份。
    pub question_id: CollaborationQuestionId,
    /// Authorized target participant.
    /// 获授权目标参与者。
    pub responder: CollaborationParticipant,
    /// Exact answer body or explicit inability-to-answer result.
    /// 精确答案正文或明确无法回答的结果。
    pub body: String,
    /// Semantic answer time in Unix milliseconds.
    /// 语义回答时间（Unix 毫秒）。
    pub answered_at_ms: i64,
}

/// Input used by the authorized source participant to cancel a question.
/// 获授权来源参与者撤销问题的输入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CancelQuestionInput {
    /// Stable settlement operation identity.
    /// 稳定结算操作身份。
    pub operation_id: CollaborationOperationId,
    /// Question identity.
    /// 问题身份。
    pub question_id: CollaborationQuestionId,
    /// Authorized source participant.
    /// 获授权来源参与者。
    pub requester: CollaborationParticipant,
    /// Explicit redacted cancellation reason.
    /// 明确的脱敏撤销原因。
    pub reason: String,
    /// Semantic cancellation time in Unix milliseconds.
    /// 语义撤销时间（Unix 毫秒）。
    pub cancelled_at_ms: i64,
}

/// Input used to independently deliver an already authoritative answer.
/// 独立交付已成为权威答案的输入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeliverAnswerInput {
    /// Stable delivery operation identity.
    /// 稳定交付操作身份。
    pub operation_id: CollaborationOperationId,
    /// Question identity.
    /// 问题身份。
    pub question_id: CollaborationQuestionId,
    /// Selected answer delivery path.
    /// 选择的答案交付路径。
    pub delivery_kind: CollaborationAnswerDeliveryKind,
    /// Physical projected event seq for User-event delivery.
    /// User 事件交付使用的物理投影事件 seq。
    pub projected_seq: Option<Seq>,
    /// Stable UserInput identity required for User-event delivery.
    /// User 浜嬩欢浜や粯鎵€闇€鐨勭ǔ瀹氭祦绋嬭緭鍏ユ爣璇嗐€?
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projected_input_id: Option<PendingInputId>,
    /// Stable tool call identity required for Tool-result delivery.
    /// ToolResult 浜や粯鎵€闇€鐨勭ǔ瀹氬伐鍏疯皟鐢ㄨ韩浠姐€?
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<ToolCallId>,
    /// Physical ToolCall event seq required for Tool-result delivery.
    /// ToolResult 浜や粯鎵€闇€鐨勭墿鐞嗚緭鍑哄疄浣?seq銆?
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_result_seq: Option<Seq>,
    /// Semantic delivery time in Unix milliseconds.
    /// 语义交付时间（Unix 毫秒）。
    pub delivered_at_ms: i64,
}

/// Input used by the source to close an answer that cannot be shown to its model.
/// 来源用于结清无法向其模型展示的答案的输入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FailAnswerDeliveryInput {
    /// Stable source operation identity.
    /// 稳定来源操作身份。
    pub operation_id: CollaborationOperationId,
    /// Exact answered question identity.
    /// 精确的已回答问题身份。
    pub question_id: CollaborationQuestionId,
    /// Stable reason for withholding the body.
    /// 不展示正文的稳定原因。
    pub reason: CollaborationDeliveryFailureReason,
    /// Failure time in Unix milliseconds.
    /// 失败时间（Unix 毫秒）。
    pub failed_at_ms: i64,
}

/// Input used to reserve one child task against a parent turn.
/// 将一条子任务预留绑定到父回合的输入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReserveChildTaskInput {
    /// Canonical authenticated source tool name.
    /// 已认证来源工具的规范名称。
    pub source_tool_name: String,
    /// Exact child execution identity.
    /// 精确子执行身份。
    pub execution_id: CollaborationExecutionId,
    /// Stable reservation operation identity.
    /// 稳定预留操作身份。
    pub operation_id: CollaborationOperationId,
    /// Real child task identity.
    /// 真实子任务身份。
    pub task_id: SubagentTaskId,
    /// Parent logical turn identity.
    /// 父逻辑回合身份。
    pub parent_turn_id: TurnId,
    /// Child participant locator.
    /// 子参与者定位。
    pub child: CollaborationParticipant,
    /// Semantic reservation time in Unix milliseconds.
    /// 语义预留时间（Unix 毫秒）。
    pub reserved_at_ms: i64,
}

/// Input used to activate one reserved child task.
/// 激活一条已预留子任务的输入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivateChildTaskInput {
    /// Exact child execution identity.
    /// 精确子执行身份。
    pub execution_id: CollaborationExecutionId,
    /// Stable activation operation identity.
    /// 稳定激活操作身份。
    pub operation_id: CollaborationOperationId,
    /// Real child task identity.
    /// 真实子任务身份。
    pub task_id: SubagentTaskId,
    /// Semantic activation time in Unix milliseconds.
    /// 语义激活时间（Unix 毫秒）。
    pub active_at_ms: i64,
}

/// Input used to terminally settle one child task.
/// 终态结算一条子任务的输入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminalChildTaskInput {
    /// Exact child execution identity.
    /// 精确子执行身份。
    pub execution_id: CollaborationExecutionId,
    /// Stable terminal operation identity.
    /// 稳定终态操作身份。
    pub operation_id: CollaborationOperationId,
    /// Real child task identity.
    /// 真实子任务身份。
    pub task_id: SubagentTaskId,
    /// Explicit child terminal outcome.
    /// 明确的子任务终态结果。
    pub outcome: CollaborationChildTaskTerminalOutcome,
    /// Semantic terminal time in Unix milliseconds.
    /// 语义终态时间（Unix 毫秒）。
    pub terminal_at_ms: i64,
}

/// Input used to begin a dedicated question execution.
/// 开始一次专用问题执行的输入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BeginQuestionExecutionInput {
    /// Stable operation identity.
    /// 稳定操作身份。
    pub operation_id: CollaborationOperationId,
    /// Stable execution identity.
    /// 稳定执行身份。
    pub execution_id: CollaborationExecutionId,
    /// Question identity.
    /// 问题身份。
    pub question_id: CollaborationQuestionId,
    /// Target participant that owns this execution.
    /// 拥有该执行的目标参与者。
    pub participant: CollaborationParticipant,
    /// Dedicated execution purpose.
    /// 专用执行用途。
    pub purpose: CollaborationExecutionPurpose,
    /// Parent turn retained for in-turn question execution.
    /// 原回合内问题执行保留的父回合。
    pub parent_turn_id: Option<TurnId>,
    /// Semantic start time in Unix milliseconds.
    /// 语义启动时间（Unix 毫秒）。
    pub started_at_ms: i64,
}

/// Input used to complete a dedicated question execution.
/// 完成一次专用问题执行的输入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompleteQuestionExecutionInput {
    /// Stable operation identity.
    /// 稳定操作身份。
    pub operation_id: CollaborationOperationId,
    /// Stable execution identity.
    /// 稳定执行身份。
    pub execution_id: CollaborationExecutionId,
    /// Explicit execution outcome.
    /// 明确执行结果。
    pub outcome: CollaborationQuestionExecutionOutcome,
    /// Semantic completion time in Unix milliseconds.
    /// 语义完成时间（Unix 毫秒）。
    pub completed_at_ms: i64,
}

/// Input used to set the explicit run-control state.
/// 设置明确运行控制状态的输入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetRunControlInput {
    /// Stable operation identity.
    /// 稳定操作身份。
    pub operation_id: CollaborationOperationId,
    /// New explicit run-control state.
    /// 新的明确运行控制状态。
    pub state: CollaborationRunControl,
    /// Semantic transition time in Unix milliseconds.
    /// 语义转换时间（Unix 毫秒）。
    pub changed_at_ms: i64,
}

/// Input used to record one user-granted collaboration scope.
/// 记录一条用户授予协作范围的输入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddCollaborationGrantInput {
    /// Stable operation identity.
    /// 稳定操作身份。
    pub operation_id: CollaborationOperationId,
    /// Stable grant identity.
    /// 稳定授权身份。
    pub grant_id: CollaborationGrantId,
    /// Host-provided authority that admitted the grant.
    /// 宿主提供的授权准入主体。
    pub authority: CollaborationGrantAuthority,
    /// Authorized source session scope.
    /// 获授权来源会话范围。
    pub source_session_id: String,
    /// Authorized target session scope.
    /// 获授权目标会话范围。
    pub target_session_id: String,
    /// Actions granted by the user.
    /// 用户授予的动作集合。
    pub actions: BTreeSet<CollaborationGrantAction>,
    /// Semantic creation time in Unix milliseconds.
    /// 语义创建时间（Unix 毫秒）。
    pub created_at_ms: i64,
}

/// Input used to revoke one previously recorded grant.
/// 撤销一条已有授权记录的输入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoveCollaborationGrantInput {
    /// Stable operation identity.
    /// 稳定操作身份。
    pub operation_id: CollaborationOperationId,
    /// Grant identity.
    /// 授权身份。
    pub grant_id: CollaborationGrantId,
    /// Semantic revocation time in Unix milliseconds.
    /// 语义撤销时间（Unix 毫秒）。
    pub revoked_at_ms: i64,
}

/// Input used to clear executable collaboration ownership at a new generation boundary.
/// 在新代次边界清除可执行协作所有权的输入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClearGenerationOwnershipInput {
    /// Stable operation identity.
    /// 稳定操作身份。
    pub operation_id: CollaborationOperationId,
    /// Semantic reset time in Unix milliseconds.
    /// 语义重置时间（Unix 毫秒）。
    pub reset_at_ms: i64,
}

/// Source/target question state returned by a successful mutation.
/// 成功变更返回的来源/目标问题状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationQuestionReceipt {
    /// Current question snapshot.
    /// 当前问题快照。
    pub question: CollaborationQuestionRecord,
    /// Source delivery snapshot.
    /// 来源投递快照。
    pub delivery: CollaborationDeliveryRecord,
    /// Collaboration event seq committed by the mutation.
    /// 本次变更提交的协作事件 seq。
    pub event_seq: Seq,
}

/// Delivery receipt returned by target or source progress mutations.
/// 目标接纳或来源进度变更返回的投递回执。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationDeliveryReceipt {
    /// Delivery identity.
    /// 投递身份。
    pub delivery_id: CollaborationDeliveryId,
    /// Related question identity.
    /// 关联问题身份。
    pub question_id: CollaborationQuestionId,
    /// Current delivery state.
    /// 当前投递状态。
    pub state: CollaborationDeliveryState,
    /// Current target or source update revision.
    /// 当前目标或来源更新修订。
    pub revision: u64,
    /// Collaboration event seq committed by the mutation.
    /// 本次变更提交的协作事件 seq。
    pub event_seq: Seq,
}

/// Source notification outbox receipt.
/// 来源通知出站箱回执。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationNotificationReceipt {
    /// Current source notification snapshot.
    /// 当前来源通知快照。
    pub notification: CollaborationNotificationRecord,
    /// Collaboration event seq committed by the mutation.
    /// 本次变更提交的协作事件 seq。
    pub event_seq: Seq,
}

/// Target notification acceptance or projection receipt.
/// 目标通知接纳或投影回执。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationInboundNotificationReceipt {
    /// Current target notification snapshot.
    /// 当前目标通知快照。
    pub notification: CollaborationInboundNotificationRecord,
    /// Collaboration event seq committed by the mutation.
    /// 本次变更提交的协作事件 seq。
    pub event_seq: Seq,
}

/// Terminal settlement receipt returned by target and source acknowledgement mutations.
/// 鐩爣缁撶畻鎴栨潵婧愭寔涔呯‘璁ゆ搷浣滆繑鍥炵殑缁堟€佸洖鎵с€?
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationDeliverySettlementReceipt {
    /// Delivery identity.
    /// 鎶曢€掕韩浠姐€?
    pub delivery_id: CollaborationDeliveryId,
    /// Related question identity.
    /// 鍏宠仈闂韬唤銆?
    pub question_id: CollaborationQuestionId,
    /// Target settlement snapshot when available.
    /// 鍙瓨鍦ㄦ椂鐨勭洰鏍囩粨绠楀揩鐓с€?
    pub settlement: Option<CollaborationInboundSettlementRecord>,
    /// Source settlement acknowledgement snapshot when available.
    /// 鍙瓨鍦ㄦ椂鐨勬潵婧愮粨绠楃‘璁ゅ揩鐓с€?
    pub settlement_ack: Option<CollaborationTargetSettlementAck>,
    /// Collaboration event seq committed by the mutation.
    /// 鏈鍙樻洿鎻愪氦鐨勫崗浣滀簨浠?seq銆?
    pub event_seq: Seq,
}

/// Question settlement receipt returned by answer or cancellation.
/// 回答或撤销返回的问题结算回执。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationSettlementReceipt {
    /// Question identity.
    /// 问题身份。
    pub question_id: CollaborationQuestionId,
    /// Authoritative winning state.
    /// 权威胜出状态。
    pub state: CollaborationQuestionState,
    /// Current independent answer-delivery state.
    /// 当前独立答案交付状态。
    pub answer_delivery: Option<CollaborationAnswerDeliveryRecord>,
    /// Collaboration event seq committed by the mutation.
    /// 本次变更提交的协作事件 seq。
    pub event_seq: Seq,
}

/// Independent answer-delivery receipt.
/// 独立答案交付回执。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationAnswerDeliveryReceipt {
    /// Question identity.
    /// 问题身份。
    pub question_id: CollaborationQuestionId,
    /// New independent delivery revision.
    /// 新的独立交付修订。
    pub delivery_revision: u64,
    /// Current answer-delivery state.
    /// 当前答案交付状态。
    pub state: CollaborationAnswerDeliveryState,
    /// Collaboration event seq committed by the mutation.
    /// 本次变更提交的协作事件 seq。
    pub event_seq: Seq,
}

/// Child-task mutation receipt.
/// 子任务变更回执。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationChildTaskReceipt {
    /// Child task snapshot.
    /// 子任务快照。
    pub task: CollaborationChildTaskRecord,
    /// Collaboration event seq committed by the mutation.
    /// 本次变更提交的协作事件 seq。
    pub event_seq: Seq,
}

/// Dedicated question-execution mutation receipt.
/// 专用问题执行变更回执。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationQuestionExecutionReceipt {
    /// Execution snapshot.
    /// 执行快照。
    pub execution: CollaborationQuestionExecutionRecord,
    /// Collaboration event seq committed by the mutation.
    /// 本次变更提交的协作事件 seq。
    pub event_seq: Seq,
}

/// Run-control mutation receipt.
/// 运行控制变更回执。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationRunControlReceipt {
    /// Current run-control state.
    /// 当前运行控制状态。
    pub state: CollaborationRunControl,
    /// Collaboration event seq committed by the mutation.
    /// 本次变更提交的协作事件 seq。
    pub event_seq: Seq,
}

/// Grant mutation receipt.
/// 授权变更回执。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationGrantReceipt {
    /// Grant snapshot.
    /// 授权快照。
    pub grant: CollaborationGrantRecord,
    /// Collaboration event seq committed by the mutation.
    /// 本次变更提交的协作事件 seq。
    pub event_seq: Seq,
}

/// Parent-turn completion obligations derived from the typed state.
/// 从强类型状态派生的父回合完成义务。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationCompletionObligations {
    /// Parent turn being checked.
    /// 正在检查的父回合。
    pub turn_id: TurnId,
    /// Pending questions sent by this turn.
    /// 该回合发出的待结算问题。
    pub outgoing_question_ids: Vec<CollaborationQuestionId>,
    /// Accepted inbound questions still requiring settlement.
    /// 已接纳且仍需结算的入站问题。
    pub incoming_question_ids: Vec<CollaborationQuestionId>,
    /// Answered questions whose answer is not yet delivered.
    /// 已回答但答案尚未交付的问题。
    pub undelivered_answer_question_ids: Vec<CollaborationQuestionId>,
    /// Settled questions whose target settlement acknowledgement is still missing.
    /// 已结算但仍缺少目标结算确认的来源问题。
    pub unacknowledged_settlement_question_ids: Vec<CollaborationQuestionId>,
    /// Child tasks still reserved or active for this turn.
    /// 该回合仍处于预留或活动状态的子任务。
    pub open_child_task_ids: Vec<SubagentTaskId>,
    /// Accepted deliveries still waiting for projection.
    /// 已接纳且仍等待投影的投递。
    pub pending_delivery_ids: Vec<CollaborationDeliveryId>,
}

/// Validates one collaboration timestamp.
/// 校验一个协作时间戳。
pub(crate) fn validate_timestamp(field: &'static str, value: i64) -> Result<(), DomainError> {
    if value < 0 {
        return Err(DomainError::InvalidTimestamp { field, value });
    }
    Ok(())
}

/// Validates one bounded body without trimming or rewriting it.
/// 校验一条有界正文且不裁剪、不改写正文。
pub fn validate_body(field: &'static str, value: &str) -> Result<(), DomainError> {
    if value.trim().is_empty() || value.len() > COLLABORATION_BODY_MAX_BYTES {
        return Err(DomainError::InvalidStatistics { field });
    }
    Ok(())
}

/// Validates the independent delivery state nested in one settled answer.
/// 校验已结算答案中嵌套的独立交付状态。
pub fn validate_answer_delivery_record(
    record: &CollaborationAnswerDeliveryRecord,
) -> Result<(), DomainError> {
    validate_timestamp(
        "collaboration.answer_delivery.updated_at_ms",
        record.updated_at_ms,
    )?;
    match record.state {
        CollaborationAnswerDeliveryState::Undelivered => {
            if record.delivery_revision != 0
                || record.failure.is_some()
                || record.kind.is_some()
                || record.projected_seq.is_some()
                || record.projected_input_id.is_some()
                || record.tool_call_id.is_some()
                || record.tool_result_seq.is_some()
            {
                return Err(DomainError::InvalidStatistics {
                    field: "collaboration.answer_delivery.undelivered_shape",
                });
            }
        }
        CollaborationAnswerDeliveryState::Delivered => {
            if record.delivery_revision == 0 || record.failure.is_some() || record.kind.is_none() {
                return Err(DomainError::InvalidStatistics {
                    field: "collaboration.answer_delivery.delivered_shape",
                });
            }
            match record.kind {
                Some(CollaborationAnswerDeliveryKind::WaitTool)
                    if record.projected_seq.is_some()
                        || record.projected_input_id.is_some()
                        || record.tool_call_id.is_none()
                        || record.tool_result_seq.is_none() =>
                {
                    return Err(DomainError::InvalidStatistics {
                        field: "collaboration.answer_delivery.wait_tool_seq",
                    });
                }
                Some(CollaborationAnswerDeliveryKind::UserEvent)
                    if record.projected_seq.is_none()
                        || record.projected_input_id.is_none()
                        || record.tool_call_id.is_some()
                        || record.tool_result_seq.is_some() =>
                {
                    return Err(DomainError::InvalidStatistics {
                        field: "collaboration.answer_delivery.user_event_seq",
                    });
                }
                _ => {}
            }
        }
        CollaborationAnswerDeliveryState::Failed => {
            if record.delivery_revision == 0
                || record.failure.is_none()
                || record.kind.is_some()
                || record.projected_seq.is_some()
                || record.projected_input_id.is_some()
                || record.tool_call_id.is_some()
                || record.tool_result_seq.is_some()
            {
                return Err(DomainError::InvalidStatistics {
                    field: "collaboration.answer_delivery.failed_shape",
                });
            }
        }
    }
    Ok(())
}

/// Validates one bounded redacted reason.
/// 校验一条有界脱敏原因。
pub fn validate_reason(field: &'static str, value: &str) -> Result<(), DomainError> {
    if value.trim().is_empty()
        || value.trim() != value
        || value.len() > COLLABORATION_REASON_MAX_BYTES
    {
        return Err(DomainError::InvalidStatistics { field });
    }
    Ok(())
}

/// Validates a source-owned delivery record.
/// 校验一条来源拥有的投递记录。
pub fn validate_delivery_record(record: &CollaborationDeliveryRecord) -> Result<(), DomainError> {
    record
        .source
        .validate("collaboration.delivery.source", false)?;
    record
        .target
        .validate("collaboration.delivery.target", false)?;
    validate_timestamp("collaboration.delivery.updated_at_ms", record.updated_at_ms)?;
    if record.state == CollaborationDeliveryState::Failed && record.failure.is_none() {
        return Err(DomainError::InvalidStatistics {
            field: "collaboration.delivery.failure",
        });
    }
    if record.state != CollaborationDeliveryState::Failed && record.failure.is_some() {
        return Err(DomainError::InvalidStatistics {
            field: "collaboration.delivery.failure_without_failed_state",
        });
    }
    match record.state {
        CollaborationDeliveryState::Recorded
            if record.target_revision.is_some() || record.target_event_seq.is_some() =>
        {
            return Err(DomainError::InvalidStatistics {
                field: "collaboration.delivery.recorded_shape",
            });
        }
        CollaborationDeliveryState::Accepted if record.target_revision.is_none() => {
            return Err(DomainError::InvalidStatistics {
                field: "collaboration.delivery.accepted_revision",
            });
        }
        CollaborationDeliveryState::Projected
            if record.target_revision.is_none() || record.target_event_seq.is_none() =>
        {
            return Err(DomainError::InvalidStatistics {
                field: "collaboration.delivery.projected_shape",
            });
        }
        _ => {}
    }
    if let Some(ack) = &record.settlement_ack {
        validate_target_settlement_ack(ack)?;
    }
    Ok(())
}

/// Validates a source-owned target settlement acknowledgement.
/// 鏍￠獙鏉ユ簮鎷ユ湁鐨勭洰鏍囩粨绠楃‘璁わ紒
pub fn validate_target_settlement_ack(
    ack: &CollaborationTargetSettlementAck,
) -> Result<(), DomainError> {
    if ack.source_event_seq.value() == 0
        || ack.target_event_seq.value() == 0
        || ack.target_revision == 0
    {
        return Err(DomainError::InvalidStatistics {
            field: "collaboration.delivery.settlement_ack.identity",
        });
    }
    validate_timestamp(
        "collaboration.delivery.settlement_ack.acknowledged_at_ms",
        ack.acknowledged_at_ms,
    )
}

/// Validates one target-owned inbound terminal settlement.
/// 鏍￠獙涓€鏉＄洰鏍囨嫢鏈夌殑缁堟€佺粨绠楃‘璁わ紒
pub fn validate_inbound_settlement(
    settlement: &CollaborationInboundSettlementRecord,
) -> Result<(), DomainError> {
    if settlement.source_event_seq.value() == 0
        || settlement.source_operation_id.as_str().is_empty()
        || settlement.target_operation_id.as_str().is_empty()
    {
        return Err(DomainError::InvalidStatistics {
            field: "collaboration.inbound.settlement.identity",
        });
    }
    validate_timestamp(
        "collaboration.inbound.settlement.acknowledged_at_ms",
        settlement.acknowledged_at_ms,
    )
}

/// Validates a target-owned inbound delivery record.
/// 校验一条目标拥有的入站投递记录。
pub fn validate_inbound_delivery_record(
    record: &CollaborationInboundDeliveryRecord,
) -> Result<(), DomainError> {
    record
        .source
        .validate("collaboration.inbound.source", false)?;
    record
        .target
        .validate("collaboration.inbound.target", false)?;
    validate_timestamp("collaboration.inbound.updated_at_ms", record.updated_at_ms)?;
    if record.state == CollaborationDeliveryState::Failed && record.failure.is_none() {
        return Err(DomainError::InvalidStatistics {
            field: "collaboration.inbound.failure",
        });
    }
    if record.state != CollaborationDeliveryState::Failed && record.failure.is_some() {
        return Err(DomainError::InvalidStatistics {
            field: "collaboration.inbound.failure_without_failed_state",
        });
    }
    match record.state {
        CollaborationDeliveryState::Recorded => {
            return Err(DomainError::InvalidStatistics {
                field: "collaboration.inbound.recorded_state",
            });
        }
        CollaborationDeliveryState::Accepted
            if record.target_revision == 0 || record.projected_seq.is_some() =>
        {
            return Err(DomainError::InvalidStatistics {
                field: "collaboration.inbound.accepted_shape",
            });
        }
        CollaborationDeliveryState::Projected
            if record.target_revision == 0 || record.projected_seq.is_none() =>
        {
            return Err(DomainError::InvalidStatistics {
                field: "collaboration.inbound.projected_shape",
            });
        }
        CollaborationDeliveryState::Failed if record.projected_seq.is_some() => {
            return Err(DomainError::InvalidStatistics {
                field: "collaboration.inbound.failed_shape",
            });
        }
        _ => {}
    }
    if record.state == CollaborationDeliveryState::Failed && record.settlement.is_some() {
        return Err(DomainError::InvalidStatistics {
            field: "collaboration.inbound.failed_settlement",
        });
    }
    if let Some(settlement) = &record.settlement {
        validate_inbound_settlement(settlement)?;
    }
    Ok(())
}

/// Validates one source-owned one-way notification outbox record.
/// 校验一条来源拥有的单向通知出站箱记录。
pub fn validate_notification_record(
    record: &CollaborationNotificationRecord,
) -> Result<(), DomainError> {
    record
        .source
        .validate("collaboration.notification.source", false)?;
    record
        .target
        .validate("collaboration.notification.target", false)?;
    validate_body("collaboration.notification.body", &record.body)?;
    validate_timestamp(
        "collaboration.notification.created_at_ms",
        record.created_at_ms,
    )?;
    validate_timestamp(
        "collaboration.notification.updated_at_ms",
        record.updated_at_ms,
    )?;
    match record.state {
        CollaborationNotificationState::Recorded => {
            if record.failure.is_some()
                || record.target_revision.is_some()
                || record.target_event_seq.is_some()
            {
                return Err(DomainError::InvalidStatistics {
                    field: "collaboration.notification.recorded_shape",
                });
            }
        }
        CollaborationNotificationState::Accepted => {
            if record.failure.is_some() || record.target_revision.is_none() {
                return Err(DomainError::InvalidStatistics {
                    field: "collaboration.notification.accepted_shape",
                });
            }
        }
        CollaborationNotificationState::Projected => {
            if record.failure.is_some()
                || record.target_revision.is_none()
                || record.target_event_seq.is_none()
            {
                return Err(DomainError::InvalidStatistics {
                    field: "collaboration.notification.projected_shape",
                });
            }
        }
        CollaborationNotificationState::Failed => {
            if record.failure.is_none() {
                return Err(DomainError::InvalidStatistics {
                    field: "collaboration.notification.failure",
                });
            }
        }
    }
    if record.target_revision.is_some_and(|revision| revision == 0)
        || record.target_event_seq.is_some_and(|seq| seq.value() == 0)
    {
        return Err(DomainError::InvalidStatistics {
            field: "collaboration.notification.revision",
        });
    }
    Ok(())
}

/// Validates one target-owned one-way notification record.
/// 校验一条目标拥有的单向通知记录。
pub fn validate_inbound_notification_record(
    record: &CollaborationInboundNotificationRecord,
) -> Result<(), DomainError> {
    record
        .source
        .validate("collaboration.inbound_notification.source", false)?;
    record
        .target
        .validate("collaboration.inbound_notification.target", false)?;
    validate_body("collaboration.inbound_notification.body", &record.body)?;
    validate_timestamp(
        "collaboration.inbound_notification.accepted_at_ms",
        record.accepted_at_ms,
    )?;
    validate_timestamp(
        "collaboration.inbound_notification.updated_at_ms",
        record.updated_at_ms,
    )?;
    if record.target_revision == 0 {
        return Err(DomainError::InvalidStatistics {
            field: "collaboration.inbound_notification.target_revision",
        });
    }
    match record.state {
        CollaborationNotificationState::Recorded => {
            return Err(DomainError::InvalidStatistics {
                field: "collaboration.inbound_notification.recorded_state",
            });
        }
        CollaborationNotificationState::Accepted => {
            if record.failure.is_some()
                || record.projected_seq.is_some()
                || record.projected_input_id.is_some()
                || record.projected_turn_id.is_some()
            {
                return Err(DomainError::InvalidStatistics {
                    field: "collaboration.inbound_notification.accepted_shape",
                });
            }
        }
        CollaborationNotificationState::Projected => {
            if record.failure.is_some()
                || record.projected_seq.is_none()
                || record.projected_input_id.is_none()
                || record.projected_turn_id.is_none()
            {
                return Err(DomainError::InvalidStatistics {
                    field: "collaboration.inbound_notification.projected_shape",
                });
            }
        }
        CollaborationNotificationState::Failed => {
            if record.failure.is_none()
                || record.projected_seq.is_some()
                || record.projected_input_id.is_some()
                || record.projected_turn_id.is_some()
            {
                return Err(DomainError::InvalidStatistics {
                    field: "collaboration.inbound_notification.failed_shape",
                });
            }
        }
    }
    if record.projected_seq.is_some_and(|seq| seq.value() == 0) {
        return Err(DomainError::InvalidStatistics {
            field: "collaboration.inbound_notification.projected_seq",
        });
    }
    Ok(())
}

/// Validates one child-task record.
/// 校验一条子任务记录。
pub fn validate_child_task_record(
    record: &CollaborationChildTaskRecord,
) -> Result<(), DomainError> {
    if let Some(result) = &record.non_success_result {
        result.validate(record)?;
    }
    if let Some(projection) = &record.non_success_projection {
        projection.validate(record)?;
    }
    if record.source_tool_name.trim().is_empty()
        || record.source_tool_name.trim() != record.source_tool_name
    {
        return Err(DomainError::CollaborationStateViolation {
            field: "collaboration.child_task.source_tool_name",
        });
    }
    record
        .child
        .validate("collaboration.child_task.child", false)?;
    if record.child.task_id.as_ref() != Some(&record.task_id) {
        return Err(DomainError::CollaborationStateViolation {
            field: "collaboration.child_task.task_binding",
        });
    }
    if !(1..=JS_SAFE_INTEGER_MAX).contains(&record.parent_segment_generation) {
        return Err(DomainError::InvalidStatistics {
            field: "collaboration.child_task.parent_segment_generation",
        });
    }
    validate_timestamp(
        "collaboration.child_task.reserved_at_ms",
        record.reserved_at_ms,
    )?;
    match record.state {
        CollaborationChildTaskState::Reserved => {
            if record.active_at_ms.is_some()
                || record.terminal_at_ms.is_some()
                || record.terminal_outcome.is_some()
            {
                return Err(DomainError::InvalidStatistics {
                    field: "collaboration.child_task.reserved_shape",
                });
            }
        }
        CollaborationChildTaskState::Active => {
            if record.active_at_ms.is_none()
                || record.terminal_at_ms.is_some()
                || record.terminal_outcome.is_some()
            {
                return Err(DomainError::InvalidStatistics {
                    field: "collaboration.child_task.active_shape",
                });
            }
        }
        CollaborationChildTaskState::Terminal => {
            if record.terminal_at_ms.is_none()
                || record.terminal_outcome.is_none()
                || (record.active_at_ms.is_none()
                    && !matches!(
                        record.terminal_outcome,
                        Some(
                            CollaborationChildTaskTerminalOutcome::Failed
                                | CollaborationChildTaskTerminalOutcome::Stopped
                        )
                    ))
            {
                return Err(DomainError::InvalidStatistics {
                    field: "collaboration.child_task.terminal_shape",
                });
            }
        }
    }
    Ok(())
}

/// Validates one dedicated question execution record.
/// 校验一条专用问题执行记录。
pub fn validate_question_execution_record(
    record: &CollaborationQuestionExecutionRecord,
) -> Result<(), DomainError> {
    if let Some(successor) = &record.queued_successor
        && (record.state != CollaborationQuestionExecutionState::Completed
            || record.purpose != CollaborationExecutionPurpose::NaturallyIdleQuestion
            || record.execution_turn_id == Some(successor.turn_id))
    {
        return Err(DomainError::CollaborationStateViolation {
            field: "collaboration.question_execution.queued_successor",
        });
    }
    record
        .participant
        .validate("collaboration.question_execution.participant", false)?;
    if record.purpose == CollaborationExecutionPurpose::NormalTask {
        return Err(DomainError::InvalidStatistics {
            field: "collaboration.question_execution.purpose",
        });
    }
    match record.purpose {
        CollaborationExecutionPurpose::InTurnQuestion if record.parent_turn_id.is_none() => {
            return Err(DomainError::InvalidStatistics {
                field: "collaboration.question_execution.parent_turn_id",
            });
        }
        CollaborationExecutionPurpose::InTurnQuestion if record.execution_turn_id.is_some() => {
            return Err(DomainError::InvalidStatistics {
                field: "collaboration.question_execution.execution_turn_id",
            });
        }
        CollaborationExecutionPurpose::NaturallyIdleQuestion if record.parent_turn_id.is_some() => {
            return Err(DomainError::InvalidStatistics {
                field: "collaboration.question_execution.parent_turn_id",
            });
        }
        CollaborationExecutionPurpose::NaturallyIdleQuestion
            if record.execution_turn_id.is_none() =>
        {
            return Err(DomainError::InvalidStatistics {
                field: "collaboration.question_execution.execution_turn_id",
            });
        }
        _ => {}
    }
    validate_timestamp(
        "collaboration.question_execution.started_at_ms",
        record.started_at_ms,
    )?;
    match record.state {
        CollaborationQuestionExecutionState::Running => {
            if record.completed_at_ms.is_some() || record.outcome.is_some() {
                return Err(DomainError::InvalidStatistics {
                    field: "collaboration.question_execution.running_shape",
                });
            }
        }
        CollaborationQuestionExecutionState::Completed => {
            if record.completed_at_ms.is_none() || record.outcome.is_none() {
                return Err(DomainError::InvalidStatistics {
                    field: "collaboration.question_execution.completed_shape",
                });
            }
        }
    }
    Ok(())
}

/// Validates one grant record.
/// 校验一条授权记录。
pub fn validate_grant_record(record: &CollaborationGrantRecord) -> Result<(), DomainError> {
    if record.source_session_id.trim().is_empty()
        || record.source_session_id.trim() != record.source_session_id
        || record.target_session_id.trim().is_empty()
        || record.target_session_id.trim() != record.target_session_id
        || record.actions.is_empty()
    {
        return Err(DomainError::InvalidStatistics {
            field: "collaboration.grant",
        });
    }
    if record.created_revision == 0 {
        return Err(DomainError::InvalidStatistics {
            field: "collaboration.grant.created_revision",
        });
    }
    validate_timestamp("collaboration.grant.created_at_ms", record.created_at_ms)?;
    if let Some(revoked_at_ms) = record.revoked_at_ms {
        validate_timestamp("collaboration.grant.revoked_at_ms", revoked_at_ms)?;
    }
    if record.revoked_at_ms.is_some() != record.revoked_revision.is_some() {
        return Err(DomainError::InvalidStatistics {
            field: "collaboration.grant.revocation",
        });
    }
    Ok(())
}
