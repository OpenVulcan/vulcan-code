//! Transcript modal state extracted from the main workbench module.
//! 从主工作台模块中抽取的转录弹层状态。

use super::*;

/// Summary data rendered by the dedicated command-run tool modal surface.
/// 命令执行专属工具弹层表面渲染的摘要数据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CommandRunToolModalSummary {
    /// Lifecycle status reported by the host-managed one-shot command execution.
    /// 宿主管理的一次性命令执行上报的生命周期状态。
    pub(super) status: String,
    /// Optional shell label used by the execution launcher.
    /// 执行启动器使用的可选 shell 标签。
    pub(super) shell: Option<String>,
    /// Optional working-directory text shown to the user.
    /// 展示给用户的可选工作目录文本。
    pub(super) working_directory: Option<String>,
    /// Optional original command text requested by the caller.
    /// 调用方请求的可选原始命令文本。
    pub(super) command_text: Option<String>,
    /// Optional failure summary synthesized by the host runtime.
    /// 宿主运行时综合生成的可选失败摘要。
    pub(super) failure_reason: Option<String>,
    /// Optional process exit code captured after completion.
    /// 完成后捕获到的可选进程退出码。
    pub(super) exit_code: Option<i32>,
    /// Optional duration spent by the command execution in milliseconds.
    /// 命令执行耗时的可选毫秒数。
    pub(super) duration_ms: Option<u64>,
    /// Optional decoded stdout byte count accumulated by the execution.
    /// 执行过程中累计的可选解码后标准输出字节数。
    pub(super) stdout_bytes: Option<u64>,
    /// Optional decoded stderr byte count accumulated by the execution.
    /// 执行过程中累计的可选解码后标准错误字节数。
    pub(super) stderr_bytes: Option<u64>,
    /// Whether the combined text output had to be truncated for rendering.
    /// 合并文本输出是否因渲染限制而被截断。
    pub(super) output_overflowed: bool,
}

/// Summary data rendered by the dedicated command-session tool modal surface.
/// 命令会话专属工具弹层表面渲染的摘要数据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CommandSessionToolModalSummary {
    /// Stable command-session identifier owned by the host runtime.
    /// 由宿主运行时持有的稳定命令会话标识。
    pub(super) command_session_id: SessionId,
    /// Optional owner session identifier recovered from transcript metadata so GUI controls can rebind to the live runtime session even after the historical tool result becomes stale.
    /// 从转录元数据恢复出的可选 owner 会话标识，使 GUI 控件即使在历史工具结果过期后也能重新绑定到实时运行时会话。
    pub(super) owner_session_id: Option<SessionId>,
    /// Most recent shell-session action represented by the transcript card.
    /// 当前转录卡片所代表的最近一次 shell-session 动作。
    pub(super) action: Option<String>,
    /// Current lifecycle status shown by the transcript card or live GUI override.
    /// 由转录卡片或 GUI 实时覆盖展示的当前生命周期状态。
    pub(super) status: String,
    /// Optional shell label used by the session launcher.
    /// 会话启动器使用的可选 shell 标签。
    pub(super) shell: Option<String>,
    /// Optional working-directory text shown to the user.
    /// 展示给用户的可选工作目录文本。
    pub(super) working_directory: Option<String>,
    /// Optional original command text requested by the caller.
    /// 调用方请求的可选原始命令文本。
    pub(super) command_text: Option<String>,
    /// Optional failure summary synthesized by the host runtime.
    /// 宿主运行时综合生成的可选失败摘要。
    pub(super) failure_reason: Option<String>,
    /// Optional process exit code captured after the process terminated.
    /// 进程终止后捕获到的可选退出码。
    pub(super) exit_code: Option<i32>,
    /// Optional highest buffered output sequence number currently known.
    /// 当前已知的可选最高缓冲输出序号。
    pub(super) last_seq: Option<u64>,
    /// Optional decoded stdout byte count accumulated so far.
    /// 当前累计的可选解码后标准输出字节数。
    pub(super) stdout_bytes: Option<u64>,
    /// Optional decoded stderr byte count accumulated so far.
    /// 当前累计的可选解码后标准错误字节数。
    pub(super) stderr_bytes: Option<u64>,
    /// Optional number of chunks returned by the most recent read action.
    /// 最近一次读取动作返回的可选分片数量。
    pub(super) returned_chunk_count: Option<u64>,
    /// Optional byte count written by the most recent write action.
    /// 最近一次写入动作写入的可选字节数。
    pub(super) bytes_written: Option<u64>,
    /// Optional flag indicating unread buffered chunks still remain.
    /// 表示是否仍有未读缓冲分片残留的可选标记。
    pub(super) has_more: Option<bool>,
    /// Whether rendered output text had to be truncated.
    /// 渲染输出文本是否被截断。
    pub(super) output_overflowed: bool,
}

impl CommandSessionToolModalSummary {
    /// Applies one live runtime snapshot onto this modal summary so status-oriented fields always reflect the latest command-session state instead of the stale historical transcript payload.
    /// 将一份实时运行时快照应用到当前弹层摘要上，使偏状态字段始终反映最新命令会话状态，而不是过期历史转录载荷。
    pub(super) fn apply_runtime_snapshot(&mut self, snapshot: &CommandSessionSnapshot) -> bool {
        if self.command_session_id != snapshot.command_session_id {
            return false;
        }
        self.owner_session_id = Some(snapshot.owner_session_id);
        self.status = snapshot.status.as_str().to_string();
        self.shell = Some(snapshot.shell_label.clone());
        self.working_directory = Some(snapshot.cwd.to_string_lossy().to_string());
        self.command_text = Some(snapshot.command.clone());
        self.exit_code = snapshot.exit_code;
        self.failure_reason = snapshot.failure_reason.clone();
        self.stdout_bytes = Some(snapshot.stdout_bytes as u64);
        self.stderr_bytes = Some(snapshot.stderr_bytes as u64);
        self.last_seq = Some(snapshot.last_seq);
        true
    }

    /// Rewrites one historically running modal summary to `stopped` when the referenced runtime session can no longer be found, so stale GUI state never keeps showing a live session after the host registry has already dropped it.
    /// 当引用的运行时会话已经无法找到时，将一份历史上仍显示 running 的弹层摘要改写为 `stopped`，从而避免过期 GUI 状态在宿主注册表已移除该会话后继续显示其仍然存活。
    pub(super) fn mark_runtime_missing_as_stopped(&mut self) {
        if self.status.trim() != "running" {
            return;
        }
        self.status = String::from("stopped");
        self.has_more = Some(false);
    }
}

/// High-level tool-modal presentation family derived from one rendered transcript block.
/// 从单个已渲染转录区块派生出的高层工具弹层展示家族。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ToolModalPresentation {
    /// Generic modal that only renders the transcript detail document.
    /// 仅渲染转录详情文档的通用弹层。
    Generic,
    /// Dedicated modal enriched with one-shot command execution summary fields.
    /// 携带一次性命令执行摘要字段的专属弹层。
    CommandRun(CommandRunToolModalSummary),
    /// Dedicated modal enriched with long-lived command-session summary fields.
    /// 携带长生命周期命令会话摘要字段的专属弹层。
    CommandSession(CommandSessionToolModalSummary),
    /// Dedicated modal enriched with structured file-change summary fields.
    /// 携带结构化文件变更摘要字段的专属弹层。
    ChangeSet(ChangeSetModalSummary),
}

impl ToolModalPresentation {
    /// Returns the embedded command-session summary when this modal presentation targets a host-managed shell session.
    /// 当该弹层展示针对宿主管理 shell 会话时，返回其内嵌的命令会话摘要。
    pub(super) fn command_session_summary(&self) -> Option<&CommandSessionToolModalSummary> {
        match self {
            Self::CommandSession(summary) => Some(summary),
            _ => None,
        }
    }

    /// Returns the mutable embedded command-session summary when this modal presentation targets a host-managed shell session.
    /// 当该弹层展示针对宿主管理 shell 会话时，返回其可变的内嵌命令会话摘要。
    pub(super) fn command_session_summary_mut(
        &mut self,
    ) -> Option<&mut CommandSessionToolModalSummary> {
        match self {
            Self::CommandSession(summary) => Some(summary),
            _ => None,
        }
    }

    /// Applies one live runtime snapshot onto the command-session summary so GUI stop actions can update the modal immediately.
    /// 将一份实时运行时快照应用到命令会话摘要上，使 GUI 停止动作可以立即更新弹层。
    pub(super) fn apply_command_session_snapshot(
        &mut self,
        snapshot: &CommandSessionSnapshot,
    ) -> bool {
        let Some(summary) = self.command_session_summary_mut() else {
            return false;
        };
        summary.apply_runtime_snapshot(snapshot)
    }
}

/// Centered modal payload reused by both thinking and tool surfaces.
/// 由思考弹层与工具弹层共同复用的居中弹层载荷。
#[derive(Debug, Clone)]
pub(super) struct ModalSurfacePayload {
    /// Stable title rendered in the modal header without needing to rebuild it on every animation tick.
    /// 渲染在弹层头部的稳定标题，无需在每个动画帧里重复重建。
    pub(super) title: String,
    /// Stable helper copy rendered below the modal title.
    /// 渲染在弹层标题下方的稳定辅助文案。
    pub(super) helper_text: String,
    /// Snapshot document rendered inside the modal body.
    /// 渲染在弹层正文中的快照文档。
    pub(super) document: vulcan_agent_view::AgentViewDocument,
    /// Generated media attachments extracted from host-only tool metadata for richer modal rendering.
    /// 从宿主专用工具元数据中提取出的生成媒体附件，用于更丰富的弹层渲染。
    pub(super) generated_attachments: Vec<ConversationAttachment>,
    /// Stable hash used by lazy body rendering so heavy transcript content only rebuilds when payload really changes.
    /// 供惰性正文渲染使用的稳定哈希，使重型转录内容仅在载荷真正变化时重建。
    pub(super) document_signature: u64,
    /// Optional richer presentation metadata used by specialized tool modals such as command-run and command-session.
    /// 专属命令执行与命令会话等特殊工具弹层使用的可选增强展示元数据。
    pub(super) tool_modal_presentation: ToolModalPresentation,
}

/// Centered modal state retained while one inline thinking block is explicitly opened above the transcript.
/// 当某个内联思考区块被显式打开到转录上方时保留的居中弹层状态。
#[derive(Debug, Clone)]
pub(super) struct ThinkingModalState {
    /// Session identifier owning the currently opened thinking block.
    /// 当前已打开思考区块所属的会话标识。
    pub(super) session_id: SessionId,
    /// Stable rendered-transcript block identifier of the opened thinking block.
    /// 当前已打开思考区块的稳定渲染转录区块标识。
    pub(super) block_id: String,
    /// Current lifecycle phase of the modal so opening and closing animations can share one state container.
    /// 当前弹层的生命周期阶段，以便打开与关闭动画共享同一状态容器。
    pub(super) phase: ThinkingModalPhase,
    /// Monotonic timestamp captured when the current phase began.
    /// 当前阶段开始时捕获的单调时间戳。
    pub(super) started_at: Instant,
    /// Stable snapshot payload reused across animation ticks.
    /// 跨动画帧复用的稳定快照载荷。
    pub(super) payload: ModalSurfacePayload,
}

/// Centered modal state retained while one inline tool block is explicitly opened above the transcript.
/// 当某个内联工具区块被显式打开到转录上方时保留的居中弹层状态。
#[derive(Debug, Clone)]
pub(super) struct ToolModalState {
    /// Session identifier owning the currently opened tool block.
    /// 当前已打开工具区块所属的会话标识。
    pub(super) session_id: SessionId,
    /// Stable rendered-transcript block identifier of the opened tool block.
    /// 当前已打开工具区块的稳定渲染转录区块标识。
    pub(super) block_id: String,
    /// Current lifecycle phase of the modal so opening and closing animations can share one state container.
    /// 当前弹层的生命周期阶段，以便打开与关闭动画共享同一状态容器。
    pub(super) phase: ThinkingModalPhase,
    /// Monotonic timestamp captured when the current phase began.
    /// 当前阶段开始时捕获的单调时间戳。
    pub(super) started_at: Instant,
    /// Stable snapshot payload reused across animation ticks.
    /// 跨动画帧复用的稳定快照载荷。
    pub(super) payload: ModalSurfacePayload,
    /// Whether one GUI-initiated stop request is currently in flight for the opened command-session modal.
    /// 当前已打开的命令会话弹层是否存在一条由 GUI 发起且仍在进行中的停止请求。
    pub(super) command_session_stop_in_flight: bool,
}
