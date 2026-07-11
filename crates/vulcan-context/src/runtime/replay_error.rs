//! Errors produced by runtime event-log replay.
//! 运行时事件日志重放产生的错误。

/// Strict failures produced while replaying runtime event lines into canonical conversation state.
/// 将运行时事件行重放为规范会话状态时产生的严格失败。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeLogReplayError {
    /// A runtime line declares a schema version unsupported by this replay implementation.
    /// 运行时日志行声明了当前重放实现不支持的结构版本。
    UnsupportedSchemaVersion {
        /// One-based physical line position inside the supplied slice.
        /// 所提供切片中的一基物理行位置。
        line_number: usize,
        /// Unsupported schema version found on the line.
        /// 在该行发现的不受支持结构版本。
        schema_version: u32,
    },
    /// Runtime line sequences are not strictly contiguous in supplied order.
    /// 运行时日志行序号未按所提供顺序严格连续。
    SequenceMismatch {
        /// One-based physical line position inside the supplied slice.
        /// 所提供切片中的一基物理行位置。
        line_number: usize,
        /// Sequence required by the preceding validated line.
        /// 前一条已验证日志行要求的序号。
        expected: u64,
        /// Actual sequence declared by the current line.
        /// 当前日志行声明的实际序号。
        actual: u64,
    },
    /// A non-final runtime line leaves no representable sequence for its successor.
    /// 非末尾运行时日志行没有为其后继留下可表示序号。
    SequenceExhausted {
        /// One-based physical line position of the exhausted sequence.
        /// 序号耗尽位置的一基物理行号。
        line_number: usize,
        /// Exhausted sequence value.
        /// 已耗尽的序号值。
        sequence: u64,
    },
    /// A conversation-producing runtime event has no non-empty local message identifier.
    /// 一条生成会话消息的运行时事件没有非空本地消息标识。
    MissingMessageIdentifier {
        /// Runtime message kind that owns the malformed identifier.
        /// 持有异常标识的运行时消息类型。
        message_kind: &'static str,
    },
    /// A conversation-producing runtime event declares a non-canonical local message identifier.
    /// 一条生成会话消息的运行时事件声明了非规范本地消息标识。
    NonCanonicalMessageIdentifier {
        /// Runtime message kind that owns the malformed identifier.
        /// 持有异常标识的运行时消息类型。
        message_kind: &'static str,
        /// Exact non-canonical message identifier.
        /// 原始非规范消息标识。
        message_id: String,
    },
    /// Two conversation-producing runtime events declare the same local message identifier.
    /// 两条生成会话消息的运行时事件声明了相同本地消息标识。
    DuplicateMessageIdentifier {
        /// Conflicting canonical message identifier.
        /// 发生冲突的规范消息标识。
        message_id: String,
    },
    /// A runtime tool call cannot find the assistant message it declares as owner.
    /// 运行时工具调用找不到其声明的所属助手消息。
    MissingToolCallAssistant {
        /// Tool call that cannot be attached.
        /// 无法附加的工具调用标识。
        tool_call_id: String,
        /// Missing assistant message identifier.
        /// 缺失的助手消息标识。
        assistant_message_id: String,
    },
    /// A runtime tool-call event declares an empty runtime-owned identifier.
    /// 一条运行时工具调用事件声明了空的运行时自有标识。
    MissingToolCallIdentifier {
        /// Assistant message that declared the malformed tool call.
        /// 声明异常工具调用的助手消息。
        assistant_message_id: String,
    },
    /// Two runtime tool-call events declare the same runtime-owned identifier.
    /// 两条运行时工具调用事件声明了相同运行时自有标识。
    DuplicateToolCallIdentifier {
        /// Conflicting runtime-owned tool-call identifier.
        /// 发生冲突的运行时自有工具调用标识。
        tool_call_id: String,
    },
    /// Two runtime tool calls declare the same provider-visible identifier.
    /// 两条运行时工具调用声明了相同 Provider 可见标识。
    DuplicateProviderToolCallIdentifier {
        /// Conflicting provider-visible tool-call identifier.
        /// 发生冲突的 Provider 可见工具调用标识。
        provider_tool_call_id: String,
    },
    /// A runtime or provider identifier collides with the other identifier namespace.
    /// 一个运行时或 Provider 标识与另一身份命名空间发生冲突。
    AmbiguousToolCallIdentifier {
        /// Identifier that resolves ambiguously across both namespaces.
        /// 在两个命名空间中产生歧义解析的标识。
        tool_call_id: String,
    },
    /// A runtime tool-call event declares an empty batch identifier.
    /// 一条运行时工具调用事件声明了空批次标识。
    MissingToolBatchIdentifier {
        /// Runtime-owned call identifier whose batch is missing.
        /// 缺失批次的运行时自有调用标识。
        tool_call_id: String,
    },
    /// A batch control event declares an empty stable batch identifier.
    /// 一条批次控制事件声明了空的稳定批次标识。
    MissingToolBatchControlIdentifier {
        /// Stable control-event kind used for diagnostics.
        /// 用于诊断的稳定控制事件类型。
        control_event: &'static str,
    },
    /// Two batch-start events declare the same stable batch identifier.
    /// 两条批次开始事件声明了相同稳定批次标识。
    DuplicateToolBatchIdentifier {
        /// Conflicting runtime batch identifier.
        /// 发生冲突的运行时批次标识。
        tool_batch_id: String,
    },
    /// A batch-start event references an assistant message that has not been replayed.
    /// 一条批次开始事件引用了尚未重放的助手消息。
    MissingToolBatchAssistant {
        /// Runtime batch whose assistant owner is absent.
        /// 所属助手缺失的运行时批次。
        tool_batch_id: String,
        /// Missing assistant message identifier.
        /// 缺失的助手消息标识。
        assistant_message_id: String,
    },
    /// A tool call references a batch that has not been opened by a control event.
    /// 一条工具调用引用了尚未由控制事件打开的批次。
    MissingToolBatchStart {
        /// Runtime-owned call that cannot join the batch.
        /// 无法加入批次的运行时自有调用。
        tool_call_id: String,
        /// Missing runtime batch identifier.
        /// 缺失的运行时批次标识。
        tool_batch_id: String,
    },
    /// A tool call declares an assistant different from the owning batch.
    /// 一条工具调用声明的助手不同于所属批次。
    ToolCallBatchAssistantMismatch {
        /// Runtime-owned call with conflicting ownership.
        /// 归属冲突的运行时自有调用。
        tool_call_id: String,
        /// Runtime batch that owns the call.
        /// 拥有该调用的运行时批次。
        tool_batch_id: String,
        /// Assistant declared by the batch-start event.
        /// 批次开始事件声明的助手。
        expected_assistant_message_id: String,
        /// Assistant declared by the tool-call event.
        /// 工具调用事件声明的助手。
        actual_assistant_message_id: String,
    },
    /// A conversation-visible event appears after runtime batch completion has begun.
    /// 一条会话可见事件出现在运行时批次完成阶段开始之后。
    ConversationEventAfterToolBatchCompletion {
        /// Stable later-event kind used for diagnostics.
        /// 用于诊断的稳定后续事件类型。
        later_event: &'static str,
    },
    /// A conversation or completion event appears after runtime audit replay has begun.
    /// 一条会话或完成事件出现在运行时审计重放阶段开始之后。
    ProjectionEventAfterRuntimeAudit {
        /// Stable later-event kind used for diagnostics.
        /// 用于诊断的稳定后续事件类型。
        later_event: &'static str,
    },
    /// A completion event references a batch that has never been opened.
    /// 一条完成事件引用了从未打开的批次。
    UnknownToolBatchCompletion {
        /// Unknown runtime batch identifier.
        /// 未知的运行时批次标识。
        tool_batch_id: String,
    },
    /// More than one completion event closes the same runtime batch.
    /// 多条完成事件关闭了同一个运行时批次。
    DuplicateToolBatchCompletion {
        /// Repeated runtime batch identifier.
        /// 被重复关闭的运行时批次标识。
        tool_batch_id: String,
    },
    /// A completion event closes a batch for which no tool call was observed.
    /// 一条完成事件关闭了未观察到任何工具调用的批次。
    EmptyToolBatchCompletion {
        /// Runtime batch closed without calls.
        /// 在没有调用时被关闭的运行时批次。
        tool_batch_id: String,
    },
    /// A completion event disagrees with the exact observed batch state.
    /// 一条完成事件与观察到的精确批次状态不一致。
    ToolBatchSummaryMismatch {
        /// Runtime batch whose summary is inconsistent.
        /// 汇总不一致的运行时批次。
        tool_batch_id: String,
        /// Exact number of replayed calls.
        /// 已重放调用的精确数量。
        expected_tool_call_count: usize,
        /// Call count declared by the completion event.
        /// 完成事件声明的调用数量。
        actual_tool_call_count: usize,
        /// Exact number of matched completed or failed results.
        /// 已匹配完成或失败结果的精确数量。
        expected_tool_result_count: usize,
        /// Result count declared by the completion event.
        /// 完成事件声明的结果数量。
        actual_tool_result_count: usize,
        /// Completion value derived from exact observed counts.
        /// 从精确观察计数推导的完成值。
        expected_complete: bool,
        /// Completion value declared by the event.
        /// 事件声明的完成值。
        actual_complete: bool,
    },
    /// Exact replayed batch cardinality cannot be represented by `usize`.
    /// 精确重放批次基数无法由 `usize` 表示。
    ReplayToolBatchCardinalityOverflow {
        /// Runtime batch whose exact count overflowed.
        /// 精确计数溢出的运行时批次。
        tool_batch_id: String,
    },
    /// A matched tool result has neither runtime-owned nor provider-visible call identity.
    /// 一条已匹配工具结果既没有运行时自有身份，也没有 Provider 可见调用身份。
    MissingMatchedToolResultIdentifier {
        /// Stable local message identifier used for diagnostics.
        /// 用于诊断的稳定本地消息标识。
        message_id: String,
    },
    /// A matched tool result references a call identity absent from prior runtime events.
    /// 一条已匹配工具结果引用了前置运行时事件中不存在的调用身份。
    UnknownMatchedToolCall {
        /// Stable local message identifier used for diagnostics.
        /// 用于诊断的稳定本地消息标识。
        message_id: String,
        /// Unknown normalized call identifier.
        /// 未知的规范化调用标识。
        tool_call_id: String,
    },
    /// Runtime-owned and provider-visible result identities resolve to different prior calls.
    /// 结果的运行时自有身份与 Provider 可见身份解析到不同前置调用。
    ConflictingMatchedToolCallIdentifiers {
        /// Stable local message identifier used for diagnostics.
        /// 用于诊断的稳定本地消息标识。
        message_id: String,
        /// Call resolved from the runtime-owned result field.
        /// 从结果运行时自有字段解析出的调用。
        runtime_tool_call_id: String,
        /// Call resolved from the provider-visible result field.
        /// 从结果 Provider 可见字段解析出的调用。
        provider_tool_call_id: String,
    },
    /// More than one matched result attempts to consume the same runtime tool call.
    /// 多条已匹配结果尝试消费同一个运行时工具调用。
    DuplicateMatchedToolResult {
        /// Local message that attempted the duplicate match.
        /// 尝试重复匹配的本地消息标识。
        message_id: String,
        /// Runtime-owned tool call already consumed by an earlier matched result.
        /// 已被更早匹配结果消费的运行时自有工具调用。
        tool_call_id: String,
    },
    /// A matched result declares a batch different from its resolved call owner.
    /// 一条已匹配结果声明的批次不同于其已解析调用归属。
    ToolResultBatchMismatch {
        /// Stable local message identifier used for diagnostics.
        /// 用于诊断的稳定本地消息标识。
        message_id: String,
        /// Batch declared by the owning tool call.
        /// 所属工具调用声明的批次。
        expected_batch_id: String,
        /// Conflicting batch declared by the result.
        /// 结果声明的冲突批次。
        actual_batch_id: String,
    },
}

impl std::fmt::Display for RuntimeLogReplayError {
    /// Formats one stable, actionable runtime replay failure.
    /// 格式化一条稳定且可操作的运行时重放失败信息。
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
            Self::UnsupportedSchemaVersion {
                line_number,
                schema_version,
            } => write!(
                formatter,
                "Runtime log replay found unsupported schema version {schema_version} at line {line_number}."
            ),
            Self::SequenceMismatch {
                line_number,
                expected,
                actual,
            } => write!(
                formatter,
                "Runtime log replay expected sequence {expected} at line {line_number}, found {actual}."
            ),
            Self::SequenceExhausted {
                line_number,
                sequence,
            } => write!(
                formatter,
                "Runtime log replay cannot advance sequence {sequence} after non-final line {line_number}."
            ),
            Self::MissingMessageIdentifier { message_kind } => write!(
                formatter,
                "Runtime log replay requires a non-empty {message_kind} message identifier."
            ),
            Self::NonCanonicalMessageIdentifier {
                message_kind,
                message_id,
            } => write!(
                formatter,
                "Runtime log replay requires canonical {message_kind} message identifier `{message_id}` without surrounding whitespace."
            ),
            Self::DuplicateMessageIdentifier { message_id } => write!(
                formatter,
                "Runtime log replay found duplicate canonical message identifier `{message_id}`."
            ),
            Self::MissingToolCallAssistant {
                tool_call_id,
                assistant_message_id,
            } => write!(
                formatter,
                "Runtime tool call `{tool_call_id}` references missing assistant message `{assistant_message_id}`."
            ),
            Self::MissingToolCallIdentifier {
                assistant_message_id,
            } => write!(
                formatter,
                "Runtime tool call owned by assistant `{assistant_message_id}` has no runtime-owned identifier."
            ),
            Self::DuplicateToolCallIdentifier { tool_call_id } => write!(
                formatter,
                "Runtime log replay found duplicate tool-call identifier `{tool_call_id}`."
            ),
            Self::DuplicateProviderToolCallIdentifier {
                provider_tool_call_id,
            } => write!(
                formatter,
                "Runtime log replay found duplicate provider tool-call identifier `{provider_tool_call_id}`."
            ),
            Self::AmbiguousToolCallIdentifier { tool_call_id } => write!(
                formatter,
                "Runtime log replay found tool-call identifier `{tool_call_id}` in both runtime and provider namespaces."
            ),
            Self::MissingToolBatchIdentifier { tool_call_id } => write!(
                formatter,
                "Runtime tool call `{tool_call_id}` has no tool-batch identifier."
            ),
            Self::MissingToolBatchControlIdentifier { control_event } => write!(
                formatter,
                "Runtime `{control_event}` event has no tool-batch identifier."
            ),
            Self::DuplicateToolBatchIdentifier { tool_batch_id } => write!(
                formatter,
                "Runtime log replay found duplicate tool-batch identifier `{tool_batch_id}`."
            ),
            Self::MissingToolBatchAssistant {
                tool_batch_id,
                assistant_message_id,
            } => write!(
                formatter,
                "Runtime tool batch `{tool_batch_id}` references missing assistant message `{assistant_message_id}`."
            ),
            Self::MissingToolBatchStart {
                tool_call_id,
                tool_batch_id,
            } => write!(
                formatter,
                "Runtime tool call `{tool_call_id}` references unopened batch `{tool_batch_id}`."
            ),
            Self::ToolCallBatchAssistantMismatch {
                tool_call_id,
                tool_batch_id,
                expected_assistant_message_id,
                actual_assistant_message_id,
            } => write!(
                formatter,
                "Runtime tool call `{tool_call_id}` declares assistant `{actual_assistant_message_id}` for batch `{tool_batch_id}`, expected `{expected_assistant_message_id}`."
            ),
            Self::ConversationEventAfterToolBatchCompletion { later_event } => write!(
                formatter,
                "Runtime `{later_event}` event appears after tool-batch completion has begun."
            ),
            Self::ProjectionEventAfterRuntimeAudit { later_event } => write!(
                formatter,
                "Runtime `{later_event}` event appears after runtime audit replay has begun."
            ),
            Self::UnknownToolBatchCompletion { tool_batch_id } => write!(
                formatter,
                "Runtime completion references unknown tool batch `{tool_batch_id}`."
            ),
            Self::DuplicateToolBatchCompletion { tool_batch_id } => write!(
                formatter,
                "Runtime tool batch `{tool_batch_id}` has more than one completion event."
            ),
            Self::EmptyToolBatchCompletion { tool_batch_id } => write!(
                formatter,
                "Runtime tool batch `{tool_batch_id}` completed without any tool call."
            ),
            Self::ToolBatchSummaryMismatch {
                tool_batch_id,
                expected_tool_call_count,
                actual_tool_call_count,
                expected_tool_result_count,
                actual_tool_result_count,
                expected_complete,
                actual_complete,
            } => write!(
                formatter,
                "Runtime tool batch `{tool_batch_id}` summary mismatch: calls expected {expected_tool_call_count}, found {actual_tool_call_count}; results expected {expected_tool_result_count}, found {actual_tool_result_count}; complete expected {expected_complete}, found {actual_complete}."
            ),
            Self::ReplayToolBatchCardinalityOverflow { tool_batch_id } => write!(
                formatter,
                "Runtime log replay cannot represent the exact cardinality of tool batch `{tool_batch_id}`."
            ),
            Self::MissingMatchedToolResultIdentifier { message_id } => write!(
                formatter,
                "Matched runtime tool-result message `{message_id}` has no call identifier."
            ),
            Self::UnknownMatchedToolCall {
                message_id,
                tool_call_id,
            } => write!(
                formatter,
                "Matched runtime tool-result message `{message_id}` references unknown call `{tool_call_id}`."
            ),
            Self::ConflictingMatchedToolCallIdentifiers {
                message_id,
                runtime_tool_call_id,
                provider_tool_call_id,
            } => write!(
                formatter,
                "Matched runtime tool-result message `{message_id}` resolves runtime id `{runtime_tool_call_id}` and provider id `{provider_tool_call_id}` to different calls."
            ),
            Self::DuplicateMatchedToolResult {
                message_id,
                tool_call_id,
            } => write!(
                formatter,
                "Runtime tool-result message `{message_id}` duplicates the matched result for call `{tool_call_id}`."
            ),
            Self::ToolResultBatchMismatch {
                message_id,
                expected_batch_id,
                actual_batch_id,
            } => write!(
                formatter,
                "Runtime tool-result message `{message_id}` declares batch `{actual_batch_id}`, expected `{expected_batch_id}`."
            ),
        }
    }
}

impl std::error::Error for RuntimeLogReplayError {}
