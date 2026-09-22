//! Exact continuation anchors derived from durable business-directed messages.
//! 从可靠业务方向消息派生的精确续跑锚点。

use super::*;

/// Which immutable local acceptance requires another business request.
/// 哪份不可变本地接纳要求另一次业务请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CollaborationContinuationSource {
    /// A non-success result from an owned child that the original parent business must receive.
    /// 原父业务必须接收的所属子任务非成功结果。
    ChildTaskResult {
        /// Exact original child execution, never the latest run of its physical session.
        /// 精确原子执行，绝非其物理会话的最新运行。
        execution_id: CollaborationExecutionId,
    },
    /// Accepted guidance for the existing target turn.
    /// 针对已有目标回合的已接纳方向。
    Steering {
        /// Exact original steering delivery.
        /// 精确原方向投递。
        delivery_id: CollaborationDeliveryId,
    },
    /// Accepted final result addressed to the original requesting turn.
    /// 发给原请求回合的已接纳最终结果。
    TaskResult {
        /// Exact original task delivery.
        /// 精确原任务投递。
        delivery_id: CollaborationDeliveryId,
    },
}

/// Stable host-only dispatch authority; the acceptance remains its durable source of truth.
/// 稳定的仅宿主分发权威；接纳记录仍是其可靠事实来源。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationContinuation {
    /// Original still-open business turn.
    /// 原仍开放业务回合。
    pub turn_id: TurnId,
    /// Local immutable acceptance event, never a fabricated API completion sequence.
    /// 本地不可变接纳事件，绝不是伪造的 API 完成序号。
    pub acceptance_seq: Seq,
    /// Strongly typed acceptance owner.
    /// 强类型接纳所有者。
    pub source: CollaborationContinuationSource,
}
