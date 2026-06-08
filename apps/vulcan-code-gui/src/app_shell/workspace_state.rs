//! Workbench workspace and navigation state extracted from the main workbench module.
//! 从主工作台模块中抽取的工作区与导航状态。

use super::*;

/// Startup lifecycle shown by the splash and bootstrap overlay.
/// 启动闪屏与引导遮罩显示的启动生命周期。
#[derive(Debug, Clone)]
pub(super) enum StartupLifecycle {
    /// The application is ready and no bootstrap overlay should be shown.
    /// 应用已就绪，不应显示引导遮罩。
    Ready,
    /// One startup bootstrap is still in progress.
    /// 一次启动引导仍在进行中。
    Bootstrapping { message: String },
    /// Bootstrap failed and the failure text should be rendered to the user.
    /// 引导失败，应该向用户展示失败文本。
    Failed { message: String },
}

/// Transcript surface mode used by the selected session view.
/// 选中会话视图使用的转录表面模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AgentTranscriptSurfaceMode {
    /// Legacy segmented rendered-preview stream kept as the primary per-block visual inspection surface.
    /// 作为首要逐块视觉检查表面的旧式分段渲染流。
    DetailCards,
    /// Selectable raw-source projection backed by the unified agent-view document.
    /// 由统一 agent-view 文档支撑的可选原始源码投影。
    RawSource,
    /// Read-only preview of the last full provider request captured before one real API call starts.
    /// 在一次真实 API 调用开始前捕获的最后一次完整 Provider 请求只读预览。
    ApiRequest,
}

/// Context-injection side panel tab used to separate expiring and persistent context.
/// 上下文注入侧栏页签，用于区分时效上下文与静态上下文。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ContextInjectionPanelTab {
    /// Shows turn-TTL dynamic injections such as recalled memories.
    /// 显示按回合 TTL 失效的动态注入，例如召回记忆。
    Dynamic,
    /// Shows persistent context such as the cached profile baseline and profile deltas.
    /// 显示持久上下文，例如缓存画像基点与画像增量。
    Static,
}

impl Default for ContextInjectionPanelTab {
    /// Returns the default tab because most live checks focus on expiring context.
    /// 返回默认页签，因为多数实时检查关注时效上下文。
    fn default() -> Self {
        Self::Dynamic
    }
}

/// Shared prepared inline transcript document reused by the main session surface without recloning the full block tree on every GUI pass.
/// 主会话表面复用的共享内联转录文档快照，避免每次界面刷新都重新深拷贝整棵区块树。
#[derive(Debug, Clone)]
pub(super) struct SessionInlineAgentViewDocumentCache {
    /// Shared prepared inline transcript document reused by the main session surface without recloning the full block tree on every GUI pass.
    /// 主会话表面复用的共享内联转录文档快照，避免每次界面刷新都重新深拷贝整棵区块树。
    pub(super) document: Rc<vulcan_agent_view::AgentViewDocument>,
}

/// Prompt-resource watch event category used to avoid broad cache invalidation.
/// 提示词资源监听事件类别，用于避免过宽的缓存失效。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PromptResourceWatchKind {
    /// Workspace-local project prompt file changed.
    /// 工作区本地项目提示词文件发生变化。
    ProjectPrompt,
    /// Workspace-local project prompt configuration changed.
    /// 工作区本地项目提示词配置发生变化。
    ProjectPromptConfig,
    /// User-level prompt composer resources changed.
    /// 用户级提示词组合资源发生变化。
    UserPromptResources,
}

/// Stable archived-item deletion target retained while the user is going through the destructive second confirmation.
/// 当用户正在经过破坏性二次确认流程时保留的稳定归档项删除目标。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ArchivedDeleteTarget {
    /// Permanently deletes one archived project together with all child archived sessions.
    /// 彻底删除一个归档项目及其全部子归档会话。
    Project(String),
    /// Permanently deletes one archived session row.
    /// 彻底删除一条归档会话行。
    Session(SessionId),
}

/// Pending destructive archived-item deletion confirmation shown inline inside the archived area.
/// 显示在归档区域内部的待确认破坏性归档项删除状态。
#[derive(Debug, Clone)]
pub(super) struct ArchivedDeleteConfirmationState {
    /// Stable deletion target that will be removed permanently after the second confirmation.
    /// 完成二次确认后将被永久移除的稳定删除目标。
    pub(super) target: ArchivedDeleteTarget,
    /// Human-readable title echoed back to the user in the destructive confirmation copy.
    /// 在破坏性确认文案中回显给用户的人类可读标题。
    pub(super) title: String,
}

/// Result payload returned after one archived-item hard-delete request finishes.
/// 单次归档项硬删除请求完成后返回的结果载荷。
#[derive(Debug, Clone)]
pub(super) struct ArchivedDeleteOutcome {
    /// Stable deletion target that has already been removed from persistence.
    /// 已经从持久化层移除的稳定删除目标。
    pub(super) target: ArchivedDeleteTarget,
    /// Human-readable title used for status and confirmation messages.
    /// 用于状态与确认文案的人类可读标题。
    pub(super) title: String,
}

/// Drop-target previews used while the task-tree project cards are being reordered.
/// 任务树项目卡片重排期间使用的插入目标预览类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TaskProjectDropTarget {
    /// Inserts the dragged project before one concrete project row.
    /// 将正在拖拽的项目插入到某个具体项目行之前。
    Before(String),
    /// Inserts the dragged pinned project at the end of the pinned section.
    /// 将正在拖拽的置顶项目插入到置顶分区末尾。
    EndOfPinned,
    /// Inserts the dragged regular project at the end of the regular section.
    /// 将正在拖拽的普通项目插入到普通分区末尾。
    EndOfRegular,
}

/// Drop-target previews used while pending-prompt queue rows are being reordered.
/// 待发送队列行重排期间使用的插入目标预览类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PendingPromptDropTarget {
    /// Inserts the dragged queue entry before one concrete queue row.
    /// 将正在拖拽的队列项插入到某个具体队列行之前。
    Before(String),
    /// Inserts the dragged queue entry at the tail of the current queue.
    /// 将正在拖拽的队列项插入到当前队列尾部。
    End,
}

/// Drop-target previews used while enabled AI capability targets are being reordered.
/// 已启用 AI 能力目标重排期间使用的插入目标预览类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum AiCapabilityRouteDropTarget {
    /// Inserts the dragged capability target before one concrete enabled row.
    /// 将正在拖拽的能力目标插入到某个具体已启用行之前。
    Before(String),
    /// Inserts the dragged capability target at the end of the current capability route.
    /// 将正在拖拽的能力目标插入到当前能力路由尾部。
    End,
}
