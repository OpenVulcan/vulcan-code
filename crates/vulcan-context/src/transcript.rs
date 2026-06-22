//! Transcript-facing context models shared by WebUI state, persistence, and context tooling.
//! 供WebUI状态、持久化与上下文工具共享的转录侧上下文模型。

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Message kinds rendered inside one workbench session transcript.
/// 渲染在某个工作台会话转录区中的消息类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionMessageKind {
    /// User-authored prompt or follow-up input.
    /// 用户撰写的提示词或跟进输入。
    User,
    /// Assistant-authored execution or reasoning output.
    /// Assistant 生成的执行结果或推理输出。
    Assistant,
    /// Error output emitted by the execution pipeline.
    /// 执行链路输出的错误消息。
    Error,
}

impl SessionMessageKind {
    /// Returns the compact label displayed in one transcript bubble header.
    /// 返回显示在转录气泡头部中的精简标签。
    pub const fn label(self) -> &'static str {
        match self {
            Self::User => "USER",
            Self::Assistant => "AI",
            Self::Error => "ERROR",
        }
    }
}

/// Durable record kinds used for visible transcript rows and hidden internal nodes.
/// 用于可见转录行与隐藏内部节点的持久化记录类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionMessageRecordKind {
    /// Plain transcript row shown directly to the user.
    /// 直接展示给用户的普通转录行。
    Transcript,
    /// Visible assistant thinking output kept outside future model context.
    /// 展示给用户但不纳入后续模型上下文的思考输出。
    ThinkingOutput,
    /// Visible assistant final output kept inside future model context.
    /// 展示给用户且纳入后续模型上下文的正式输出。
    AssistantOutput,
    /// Visible tool-call summary row kept outside future model context.
    /// 展示给用户但不纳入后续模型上下文的工具调用摘要行。
    ToolCallOutput,
    /// Visible tool-result row kept outside future model context.
    /// 展示给用户但不纳入后续模型上下文的工具返回行。
    ToolResultOutput,
    /// Hidden title-generation output kept for traceability.
    /// 为了可追溯性保留的隐藏标题生成输出。
    TitleOutput,
    /// Hidden summary output produced after one execution round.
    /// 在一次执行回合后产生的隐藏摘要输出。
    SummaryOutput,
    /// Hidden explore output produced when a session is seeded from one project.
    /// 从某个项目播种会话时产生的隐藏探索输出。
    ExploreOutput,
    /// Hidden tool-detail payload used for on-demand WebUI tool expansion.
    /// 用于 WebUI 工具按需展开的隐藏工具详情载荷。
    ToolDetailOutput,
    /// Hidden compaction summary used as the new context anchor.
    /// 作为新上下文锚点使用的隐藏压缩摘要。
    CompactionSummary,
    /// Visible context-compaction status row kept outside future model context.
    /// 展示给用户但不纳入后续模型上下文的上下文压缩状态行。
    CompactionStatus,
    /// Hidden terminated-turn marker used to close one interrupted or failed turn without a summary.
    /// 用于闭合一轮被中断或失败且没有 summary 的隐藏终止标记。
    TurnTerminated,
    /// Visible per-turn statistics chip that summarizes all provider calls after the turn reaches a terminal state.
    /// 在轮次进入终态后汇总全部 Provider 调用的可见按轮次统计标签。
    TurnStats,
}

impl SessionMessageRecordKind {
    /// Returns the persisted text key used by the SQLite workspace store.
    /// 返回 SQLite 工作台存储使用的持久化文本键。
    pub const fn as_persisted_key(self) -> &'static str {
        match self {
            Self::Transcript => "transcript",
            Self::ThinkingOutput => "thinking-output",
            Self::AssistantOutput => "assistant-output",
            Self::ToolCallOutput => "tool-call-output",
            Self::ToolResultOutput => "tool-result-output",
            Self::TitleOutput => "title-output",
            Self::SummaryOutput => "summary-output",
            Self::ExploreOutput => "explore-output",
            Self::ToolDetailOutput => "tool-detail-output",
            Self::CompactionSummary => "compaction-summary",
            Self::CompactionStatus => "compaction-status",
            Self::TurnTerminated => "turn-terminated",
            Self::TurnStats => "turn-stats",
        }
    }

    /// Returns whether the record should be included when rebuilding the next LLM context.
    /// 返回在重建下一轮 LLM 上下文时是否应纳入该记录。
    pub const fn contributes_to_context(self) -> bool {
        !matches!(
            self,
            Self::TitleOutput
                | Self::ThinkingOutput
                | Self::SummaryOutput
                | Self::ExploreOutput
                | Self::ToolDetailOutput
                | Self::CompactionStatus
                | Self::TurnTerminated
                | Self::TurnStats
        )
    }

    /// Parses one persisted text key back into the durable record kind enum.
    /// 将一条持久化文本键解析回持久化记录类型枚举。
    pub fn from_persisted_key(value: &str) -> Self {
        match value {
            "thinking-output" => Self::ThinkingOutput,
            "assistant-output" => Self::AssistantOutput,
            "tool-call-output" => Self::ToolCallOutput,
            "tool-result-output" => Self::ToolResultOutput,
            "title-output" => Self::TitleOutput,
            "summary-output" => Self::SummaryOutput,
            "explore-output" => Self::ExploreOutput,
            "tool-detail-output" => Self::ToolDetailOutput,
            "compaction-summary" => Self::CompactionSummary,
            "compaction-status" => Self::CompactionStatus,
            "turn-terminated" => Self::TurnTerminated,
            "turn-stats" => Self::TurnStats,
            _ => Self::Transcript,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SessionMessageRecordKind;

    /// Verifies hidden explore records remain audit-only and do not re-enter model context reconstruction.
    /// 验证隐藏探索记录仅作为审计信息保留，不会重新进入模型上下文重建。
    #[test]
    fn explore_output_does_not_contribute_to_context() {
        assert!(!SessionMessageRecordKind::ExploreOutput.contributes_to_context());
    }
}

/// Structured message entry shown inside one workbench session transcript.
/// 展示在某个工作台会话转录区中的结构化消息项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionMessageEntry {
    /// Stable message identifier stored in SQLite and referenced by compaction checkpoints.
    /// 存储在 SQLite 中并被压缩检查点引用的稳定消息标识。
    pub id: String,
    /// High-level message kind.
    /// 消息的高层类型。
    pub kind: SessionMessageKind,
    /// Durable record kind distinguishing visible transcript rows from hidden internal outputs.
    /// 用于区分可见转录行与隐藏内部输出的持久化记录类型。
    pub record_kind: SessionMessageRecordKind,
    /// Whether the message should be rendered in the visible transcript stream.
    /// 该消息是否应渲染到可见转录流中。
    pub visible: bool,
    /// Optional headline displayed above the message body.
    /// 展示在消息正文上方的可选标题。
    pub title: String,
    /// Main message body content.
    /// 主要消息正文内容。
    pub body: String,
    /// Optional structured metadata reserved for transcript-only UI enrichments and audit details.
    /// 为仅限转录的界面增强与审计细节保留的可选结构化元数据。
    pub metadata: Option<Value>,
    /// Unix-millis text captured when the entry is created or hydrated from storage.
    /// 在创建或从存储恢复时捕获的 Unix 毫秒文本。
    pub created_at: String,
}

impl SessionMessageEntry {
    /// Creates a new session message entry from explicit parts.
    /// 使用明确片段创建新的会话消息项。
    pub fn new(
        kind: SessionMessageKind,
        title: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        Self {
            id: String::new(),
            kind,
            record_kind: SessionMessageRecordKind::Transcript,
            visible: true,
            title: title.into(),
            body: body.into(),
            metadata: None,
            created_at: String::new(),
        }
    }

    /// Creates one hidden internal message entry with an explicit record kind.
    /// 使用显式记录类型创建一条隐藏内部消息项。
    pub fn hidden(
        kind: SessionMessageKind,
        record_kind: SessionMessageRecordKind,
        title: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        Self {
            id: String::new(),
            kind,
            record_kind,
            visible: false,
            title: title.into(),
            body: body.into(),
            metadata: None,
            created_at: String::new(),
        }
    }

    /// Attaches persisted runtime metadata to a freshly created session message entry.
    /// 为一条新创建的会话消息项附着持久化运行时元数据。
    pub fn with_runtime_metadata(
        mut self,
        id: impl Into<String>,
        created_at: impl Into<String>,
    ) -> Self {
        self.id = id.into();
        self.created_at = created_at.into();
        self
    }

    /// Returns whether the entry should be shown to the user inside the transcript stream.
    /// 返回该消息项是否应展示给用户的转录流中。
    pub const fn is_visible_to_user(&self) -> bool {
        self.visible
    }

    /// Returns whether the entry should participate in future model-context reconstruction.
    /// 返回该消息项是否应参与未来的模型上下文重建。
    pub const fn contributes_to_context(&self) -> bool {
        self.record_kind.contributes_to_context()
    }
}

/// Hidden compaction checkpoint bound to one session transcript.
/// 绑定到单个会话转录流的隐藏压缩检查点。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionCompactionEntry {
    /// Stable compaction identifier.
    /// 稳定的压缩标识。
    pub id: String,
    /// Last source message included by the compaction input window.
    /// 被本次压缩输入窗口覆盖的最后一条源消息标识。
    pub source_until_message_id: Option<String>,
    /// Last source user-turn index included by the compaction input window.
    /// 被本次压缩输入窗口覆盖的最后一个源用户轮次索引。
    pub source_until_turn_index: Option<i64>,
    /// First message identifier that should be used when rebuilding the next context window.
    /// 重建下一轮上下文窗口时应作为起点的第一条消息标识。
    pub context_start_message_id: Option<String>,
    /// Stable budget fingerprint captured when this compaction checkpoint was created.
    /// 创建该压缩检查点时捕获的稳定预算指纹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_budget_fingerprint: Option<String>,
    /// Stable layered-context fingerprint captured when this compaction checkpoint was created.
    /// 创建该压缩检查点时捕获的稳定分层上下文指纹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_assembly_fingerprint: Option<String>,
    /// First retained user-turn index that stayed verbatim after the compaction rewrite.
    /// 压缩改写后仍按原样保留的第一个用户轮次索引。
    pub retained_tail_start_turn_index: Option<i64>,
    /// Hidden summary message emitted by the compaction step.
    /// 压缩步骤输出的隐藏摘要消息标识。
    pub summary_message_id: Option<String>,
    /// Unix-millis text capturing when the checkpoint was created.
    /// 捕获该检查点创建时间的 Unix 毫秒文本。
    pub created_at: String,
}
