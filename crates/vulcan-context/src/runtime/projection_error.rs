//! Errors produced by canonical conversation-to-runtime projection.
//! 规范会话到运行时投影产生的错误。

/// Strict failures produced while projecting canonical conversation state into runtime events.
/// 将规范会话状态投影为运行时事件时产生的严格失败。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeLogProjectionError {
    /// The current event sequence cannot advance to a representable next sequence.
    /// 当前事件序号无法推进到可表示的下一个序号。
    SequenceExhausted {
        /// Sequence that cannot be assigned while preserving a valid next sequence.
        /// 在保留有效下一序号的前提下无法分配的序号。
        sequence: u64,
    },
    /// A tool call has neither a non-empty provider call id nor a non-empty stable item id.
    /// 工具调用既没有非空 Provider 调用标识，也没有非空稳定条目标识。
    MissingToolCallIdentifier {
        /// Tool name attached to the malformed call for diagnostics.
        /// 附加到异常调用上的工具名称，用于诊断。
        tool_name: String,
    },
    /// Two projected tool calls resolve to the same runtime-owned identifier.
    /// 两个被投影的工具调用解析为相同的运行时自有标识。
    DuplicateToolCallIdentifier {
        /// Conflicting normalized runtime tool-call identifier.
        /// 发生冲突的规范化运行时工具调用标识。
        tool_call_id: String,
    },
    /// A canonical protocol message has no non-empty local identifier.
    /// 一条规范协议消息没有非空本地标识。
    MissingMessageIdentifier {
        /// Canonical payload kind that owns the malformed identifier.
        /// 持有异常标识的规范载荷类型。
        message_kind: &'static str,
    },
    /// A canonical protocol message identifier contains surrounding whitespace.
    /// 一条规范协议消息标识包含首尾空白。
    NonCanonicalMessageIdentifier {
        /// Canonical payload kind that owns the malformed identifier.
        /// 持有异常标识的规范载荷类型。
        message_kind: &'static str,
        /// Exact non-canonical message identifier.
        /// 原始非规范消息标识。
        message_id: String,
    },
    /// Two canonical protocol messages declare the same stable local identifier.
    /// 两条规范协议消息声明了相同稳定本地标识。
    DuplicateMessageIdentifier {
        /// Conflicting canonical message identifier.
        /// 发生冲突的规范消息标识。
        message_id: String,
    },
    /// Runtime tool-batch cardinality cannot be represented by the platform `usize`.
    /// 运行时工具批次基数无法由平台 `usize` 表示。
    ToolBatchCardinalityOverflow {
        /// Batch whose exact call or result count could not be represented.
        /// 无法精确表示调用数或结果数的批次。
        tool_batch_id: String,
    },
}

impl std::fmt::Display for RuntimeLogProjectionError {
    /// Formats one stable, actionable projection failure.
    /// 格式化一条稳定且可操作的投影失败信息。
    ///
    /// # Parameters
    /// 参数
    /// - `formatter`: Destination formatter supplied by the standard formatting runtime.
    /// - `formatter`：由标准格式化运行时提供的目标格式化器。
    ///
    /// # Returns
    /// 返回值
    /// Formatting success or the destination formatter failure.
    /// 返回格式化成功，或目标格式化器失败。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SequenceExhausted { sequence } => write!(
                formatter,
                "Runtime log projection cannot assign sequence {sequence} because no valid next sequence would remain."
            ),
            Self::MissingToolCallIdentifier { tool_name } => write!(
                formatter,
                "Runtime log projection requires a non-empty tool-call identifier for tool `{tool_name}`."
            ),
            Self::DuplicateToolCallIdentifier { tool_call_id } => write!(
                formatter,
                "Runtime log projection found duplicate tool-call identifier `{tool_call_id}`."
            ),
            Self::MissingMessageIdentifier { message_kind } => write!(
                formatter,
                "Runtime log projection requires a non-empty {message_kind} message identifier."
            ),
            Self::NonCanonicalMessageIdentifier {
                message_kind,
                message_id,
            } => write!(
                formatter,
                "Runtime log projection requires canonical {message_kind} message identifier `{message_id}` without surrounding whitespace."
            ),
            Self::DuplicateMessageIdentifier { message_id } => write!(
                formatter,
                "Runtime log projection found duplicate canonical message identifier `{message_id}`."
            ),
            Self::ToolBatchCardinalityOverflow { tool_batch_id } => write!(
                formatter,
                "Runtime log projection cannot represent the exact cardinality of tool batch `{tool_batch_id}`."
            ),
        }
    }
}

impl std::error::Error for RuntimeLogProjectionError {}
