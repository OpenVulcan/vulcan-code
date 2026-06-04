//! Shared session runtime-state and queued-prompt models reused by GUI, persistence, and runtime layers.
//! 在 GUI、持久化与运行时层之间复用的共享会话运行时状态与排队提示词模型。

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use vulcan_context::SessionConversationState;
use vulcan_context_injection::ContextInjectionState;
use vulcan_core::types::{ConversationAttachment, ProviderUsageSnapshot};
use vulcan_provider_config::config::PersistedPromptSkillState;
use vulcan_provider_config::config::{
    AiToolKind, OpenRouterEndpointRouteSelection, ReasoningEffortLevel,
};
use vulcan_vmm::{PostActionRequest, ProfileNodeEntry, VmmConfigLayer};

/// Maximum number of pending prompts retained in either visible or internal queues.
/// 可见队列或内部队列中允许保留的最大待发送提示词数量。
pub const PENDING_PROMPT_QUEUE_LIMIT: usize = 10;

/// Stable source kind attached to one pending session prompt queue entry.
/// 附着在单条待发送会话提示词队列项上的稳定来源类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PendingSessionPromptSource {
    /// User-authored follow-up that should wait for the current run to finish naturally.
    /// 应等待当前运行自然结束后再发送的用户跟进消息。
    UserFollowup,
    /// User-authored follow-up that should interrupt the current run before being dispatched.
    /// 应先打断当前运行再发送的用户跟进消息。
    UserInterrupt,
    /// Synthetic follow-up generated when one delegated subagent reports back asynchronously.
    /// 某个委派子代理异步回报后生成的合成跟进消息。
    DelegateCompletion,
}

/// Returns the internal queue index that should dispatch next.
/// 返回内部队列下一条应被分发的索引。
///
/// # Parameters
/// 参数
/// `queue` is the internal pending prompt queue owned by one session.
/// `queue` 是单个会话拥有的内部待发送提示词队列。
/// `prefer_delegate_completion` asks the router to close delegated callbacks before user follow-ups.
/// `prefer_delegate_completion` 要求路由器在用户跟进前优先闭合委派回调。
///
/// # Returns
/// 返回值
/// The queue index that should be removed and dispatched next.
/// 应被移除并优先分发的队列索引。
pub fn pending_prompt_dispatch_index(
    queue: &[PendingSessionPromptEntry],
    prefer_delegate_completion: bool,
) -> usize {
    if prefer_delegate_completion
        && let Some(index) = queue
            .iter()
            .position(|entry| entry.source == PendingSessionPromptSource::DelegateCompletion)
    {
        return index;
    }
    0
}

/// Runtime facts required by the shared pending-prompt dispatch gate.
/// 共享待发送提示词分发门禁所需的运行态事实。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingPromptDispatchContext {
    /// Whether the current provider uses the Anthropic runtime family.
    /// 当前 Provider 是否使用 Anthropic 运行时族。
    pub uses_anthropic_runtime: bool,
    /// Whether delegated subagents are still running under the same parent session.
    /// 同一父会话下是否仍有委派子代理正在运行。
    pub has_running_subagents: bool,
    /// Whether the parent session still owns an active root model stream.
    /// 父会话是否仍拥有活跃的根模型流。
    pub has_active_run: bool,
    /// Current visible session run phase.
    /// 当前可见会话运行阶段。
    pub run_phase: Option<crate::SessionRunPhase>,
    /// Whether a prompt compaction job is still active for this session.
    /// 当前会话是否仍有活跃的提示词压缩任务。
    pub has_active_compaction: bool,
}

/// Reason why a queued prompt cannot be dispatched yet.
/// 排队提示词暂时不能分发的原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingPromptDispatchBlockReason {
    /// Anthropic parent dispatch must wait for running delegated subagents.
    /// Anthropic 父会话分发必须等待运行中的委派子代理。
    AnthropicRunningSubagents,
    /// A root model stream is still active.
    /// 根模型流仍在运行。
    ActiveRun,
    /// The session is still in preparation.
    /// 会话仍处于准备阶段。
    Preparing,
    /// A prompt compaction job is still active.
    /// 提示词压缩任务仍在运行。
    ActiveCompaction,
    /// Automatic dispatch is blocked by a manually stopped phase.
    /// 自动分发被手动停止阶段阻塞。
    Stopped,
}

/// Returns the shared iced-compatible reason that blocks queued prompt dispatch.
/// 返回与 iced 兼容的共享排队提示词分发阻塞原因。
///
/// # Parameters
/// 参数
/// `context` contains UI-specific runtime facts collected by the caller.
/// `context` 包含调用方收集的界面专属运行态事实。
/// `allow_from_stopped_phase` mirrors iced manual insert behavior.
/// `allow_from_stopped_phase` 对齐 iced 手动引导行为。
///
/// # Returns
/// 返回值
/// A block reason when dispatch must wait, otherwise `None`.
/// 必须等待时返回阻塞原因，否则返回 `None`。
pub fn pending_prompt_dispatch_block_reason(
    context: PendingPromptDispatchContext,
    allow_from_stopped_phase: bool,
) -> Option<PendingPromptDispatchBlockReason> {
    if context.uses_anthropic_runtime && context.has_running_subagents {
        return Some(PendingPromptDispatchBlockReason::AnthropicRunningSubagents);
    }
    if context.has_active_run {
        return Some(PendingPromptDispatchBlockReason::ActiveRun);
    }
    if context.run_phase == Some(crate::SessionRunPhase::Preparing) {
        return Some(PendingPromptDispatchBlockReason::Preparing);
    }
    if context.has_active_compaction {
        return Some(PendingPromptDispatchBlockReason::ActiveCompaction);
    }
    if !allow_from_stopped_phase && context.run_phase == Some(crate::SessionRunPhase::Stopped) {
        return Some(PendingPromptDispatchBlockReason::Stopped);
    }
    None
}

/// Runtime facts required by the shared pending-prompt busy-lane check.
/// 共享待发送提示词繁忙通道判断所需的运行态事实。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingPromptBusyLaneContext {
    /// Dispatch context shared with the queued prompt router.
    /// 与排队提示词路由器共享的分发上下文。
    pub dispatch: PendingPromptDispatchContext,
    /// Whether the session still owns an active precheck progress row.
    /// 当前会话是否仍拥有活跃的预检查进度行。
    pub has_precheck_progress: bool,
}

/// Returns whether the internal system lane is still busy using iced semantics.
/// 使用 iced 语义判断内部系统通道是否仍然繁忙。
///
/// # Parameters
/// 参数
/// `ui_state` is the session-scoped queue state.
/// `ui_state` 是会话级队列状态。
/// `context` contains UI-specific runtime facts collected by the caller.
/// `context` 包含调用方收集的界面专属运行态事实。
///
/// # Returns
/// 返回值
/// `true` when a submitted prompt must enter the visible pending queue.
/// 当提交的提示词必须进入可见待发送队列时返回 `true`。
pub fn pending_prompt_lane_is_busy(
    ui_state: &SessionTranscriptUiState,
    context: PendingPromptBusyLaneContext,
) -> bool {
    context.dispatch.has_active_run
        || (context.dispatch.run_phase == Some(crate::SessionRunPhase::Preparing)
            && !context.dispatch.has_active_run
            && context.has_precheck_progress)
        || context.dispatch.has_active_compaction
        || !ui_state.pending_prompt_queue.is_empty()
        || context.dispatch.has_running_subagents
}

/// Error returned when a visible pending prompt cannot be routed into the internal queue.
/// 当可见待发送提示词无法路由进内部队列时返回的错误。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingPromptQueueRouteError {
    /// The requested visible panel entry no longer exists.
    /// 请求的可见面板条目已不存在。
    MissingPanelEntry,
    /// The internal queue is already at capacity.
    /// 内部队列已经达到容量上限。
    InternalQueueFull,
}

/// Outcome returned after routing a visible pending prompt into the internal queue.
/// 将可见待发送提示词路由进内部队列后的结果。
#[derive(Debug, Clone)]
pub enum PendingPromptQueueRouteOutcome {
    /// A fresh internal queue entry was created and inserted.
    /// 已创建并插入新的内部队列项。
    Inserted(PendingSessionPromptEntry),
    /// The visible row had already been inserted earlier.
    /// 该可见行此前已经插入过。
    AlreadyInserted,
}

/// Builds one internal pending prompt entry from a visible panel entry.
/// 从可见面板条目构建一条内部待发送提示词项。
///
/// # Parameters
/// 参数
/// `panel_entry` is the visible queue row selected by the user or passive dispatcher.
/// `panel_entry` 是用户或被动分发器选中的可见队列行。
/// `internal_entry_id` is the stable id assigned by the caller.
/// `internal_entry_id` 是调用方分配的稳定内部条目标识。
/// `created_at` is the Unix-millis text captured for the internal entry.
/// `created_at` 是为内部条目捕获的 Unix 毫秒文本。
///
/// # Returns
/// 返回值
/// The internal prompt entry that can be appended to a session execution queue.
/// 可追加到会话执行队列中的内部提示词条目。
pub fn pending_session_prompt_entry_from_panel_entry(
    panel_entry: &PendingPromptPanelEntry,
    internal_entry_id: String,
    created_at: String,
) -> PendingSessionPromptEntry {
    PendingSessionPromptEntry {
        id: internal_entry_id,
        prompt: panel_entry.prompt.clone(),
        preview: panel_entry.preview.clone(),
        attachments: panel_entry.attachments.clone(),
        model_reference: panel_entry.model_reference.clone(),
        runtime_options: panel_entry.runtime_options.clone(),
        source: PendingSessionPromptSource::UserFollowup,
        task_id: None,
        panel_entry_id: Some(panel_entry.id.clone()),
        created_at,
    }
}

/// Runtime controls captured when a queued prompt is authored or inserted.
/// 在排队提示词创建或插入时捕获的运行控制参数。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PendingPromptWebSearchOverride {
    /// Follow the globally configured web-search routing mode.
    /// 跟随全局配置的网页搜索路由模式。
    FollowGlobal,
    /// Force provider-native web search only.
    /// 强制仅使用模型原生网页搜索。
    NativeOnly,
    /// Force the configured third-party search chain only.
    /// 强制仅使用已配置的第三方搜索链。
    ThirdPartyOnly,
    /// Disable unified web search for this queued prompt.
    /// 为当前排队提示词关闭统一网页搜索。
    Disabled,
    /// Force one specific third-party provider entry.
    /// 强制使用单个指定的第三方搜索条目。
    SpecificProvider,
}

impl Default for PendingPromptWebSearchOverride {
    /// Returns the default queued web-search override behavior.
    /// 返回排队提示词默认使用的网页搜索覆盖行为。
    fn default() -> Self {
        Self::FollowGlobal
    }
}

/// Runtime controls captured when a queued prompt is authored or inserted.
/// 在排队提示词创建或插入时捕获的运行控制参数。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingPromptRuntimeOptions {
    /// Shared reasoning-effort preset that must travel with the queued prompt.
    /// 必须随排队提示词一起传递的共享推理档位预设。
    pub reasoning_effort: ReasoningEffortLevel,
    /// Optional sampling temperature override captured from the workbench controls.
    /// 从工作台控制区捕获的可选采样温度覆盖值。
    pub temperature: Option<f32>,
    /// Queued web-search override mode captured from the workbench controls.
    /// 从工作台控制区捕获的排队网页搜索覆盖模式。
    #[serde(default)]
    pub web_search_override: PendingPromptWebSearchOverride,
    /// Optional third-party provider id captured when one specific search entry is selected.
    /// 当选中单个指定搜索条目时捕获的可选第三方供应商标识。
    #[serde(default)]
    pub web_search_provider_id: Option<String>,
    /// Semantic AI capability kinds temporarily disabled for the current send chain without mutating persisted target preferences.
    /// 在不修改持久化目标偏好的前提下，为当前发送链路临时关闭的语义 AI 能力类型集合。
    #[serde(default)]
    pub disabled_ai_capability_kinds: BTreeSet<AiToolKind>,
}

impl Default for PendingPromptRuntimeOptions {
    /// Returns the neutral runtime option snapshot used when persisted queue entries predate explicit snapshots.
    /// 返回早于显式快照的持久化队列项所使用的中性运行参数快照。
    fn default() -> Self {
        Self {
            reasoning_effort: ReasoningEffortLevel::Medium,
            temperature: None,
            web_search_override: PendingPromptWebSearchOverride::default(),
            web_search_provider_id: None,
            disabled_ai_capability_kinds: BTreeSet::new(),
        }
    }
}

/// Provider/model identity captured when a queued prompt is authored or inserted.
/// 在排队提示词创建或插入时捕获的供应商与模型身份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingPromptModelReference {
    /// Stable provider identifier used to resolve credentials and transport configuration.
    /// 用于解析凭据与传输配置的稳定供应商标识。
    pub provider_id: String,
    /// Stable model identifier used by the provider request.
    /// Provider 请求使用的稳定模型标识。
    pub model_id: String,
    /// Human-readable provider label retained for queue diagnostics.
    /// 为队列诊断保留的人类可读供应商标签。
    #[serde(default)]
    pub provider_label: String,
    /// Human-readable model label retained for queue diagnostics.
    /// 为队列诊断保留的人类可读模型标签。
    #[serde(default)]
    pub model_name: String,
    /// Optional OpenRouter endpoint route locked together with the queued provider/model identity.
    /// 与排队供应商/模型身份一起锁定的可选 OpenRouter 端点路由。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openrouter_endpoint_route: Option<OpenRouterEndpointRouteSelection>,
}

/// One persisted pending session prompt retained for queued follow-ups and delegated-task callbacks.
/// 为排队跟进与委派任务回调保留的一条持久化待发送会话提示词。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingSessionPromptEntry {
    /// Stable queue entry identifier used for deduplication and persistence.
    /// 用于去重与持久化的稳定队列项标识。
    pub id: String,
    /// Full prompt text that should be dispatched into the next session run.
    /// 应在下一次会话运行中发送的完整提示词文本。
    pub prompt: String,
    /// Compact preview text rendered inside the pending-queue strip.
    /// 渲染在待发送队列条中的紧凑预览文本。
    pub preview: String,
    /// Structured attachments that should be replayed together with the queued prompt text.
    /// 应与排队提示词文本一起回放的结构化附件集合。
    #[serde(default)]
    pub attachments: Vec<ConversationAttachment>,
    /// Optional provider/model identity locked when the prompt entered a queue.
    /// 提示词进入队列时锁定的可选供应商与模型身份。
    #[serde(default)]
    pub model_reference: Option<PendingPromptModelReference>,
    /// Runtime controls locked when the prompt entered a queue.
    /// 提示词进入队列时锁定的运行控制参数。
    #[serde(default)]
    pub runtime_options: PendingPromptRuntimeOptions,
    /// Stable source kind attached to the queued prompt.
    /// 附着在该排队提示词上的稳定来源类型。
    pub source: PendingSessionPromptSource,
    /// Optional delegated task identifier associated with the queued follow-up.
    /// 与该排队跟进关联的可选委派任务标识。
    pub task_id: Option<String>,
    /// Optional visible-panel queue entry identifier from which this system prompt was inserted.
    /// 该系统提示词由哪个可见面板队列项插入而来的可选标识。
    #[serde(default)]
    pub panel_entry_id: Option<String>,
    /// Unix-millis text captured when the queue entry was created.
    /// 捕获该队列项创建时间的 Unix 毫秒文本。
    pub created_at: String,
}

/// Stable state retained by one visible prompt-panel queue entry shown above the composer.
/// 显示在提示词输入框上方的可见队列项所保留的稳定状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PendingPromptPanelEntryState {
    /// Entry is still waiting for an explicit user insertion into the internal system queue.
    /// 当前条目仍在等待用户显式插入到内部系统队列。
    Ready,
    /// Entry has already been inserted into the internal system queue and is now waiting there silently.
    /// 当前条目已经插入到内部系统队列，并正在该队列中静默等待。
    Inserted,
}

/// Drop target used to reorder one visible pending-prompt panel entry.
/// 用于重排单条可见待发送提示词面板项的落点。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingPromptPanelDropTarget {
    /// Insert the dragged entry before one concrete visible row.
    /// 将正在拖拽的条目插入到某个具体可见行之前。
    Before(String),
    /// Insert the dragged entry at the tail of the visible queue.
    /// 将正在拖拽的条目插入到可见队列尾部。
    End,
}

impl Default for PendingPromptPanelEntryState {
    /// Returns the default visible prompt-panel entry state used for older persisted snapshots.
    /// 返回旧持久化快照恢复时使用的默认可见队列项状态。
    fn default() -> Self {
        Self::Ready
    }
}

/// One visible user-authored prompt retained inside the dedicated insertion panel above the composer.
/// 保留在提示词输入框上方独立插入面板中的一条用户可见提示词项。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingPromptPanelEntry {
    /// Stable panel-entry identifier used for reordering, editing, and insertion tracking.
    /// 用于重排、编辑与插入跟踪的稳定面板项标识。
    pub id: String,
    /// Full user-authored prompt text retained until it is explicitly inserted or removed.
    /// 在被显式插入或移除之前一直保留的完整用户提示词文本。
    pub prompt: String,
    /// Compact preview text rendered inside the visible prompt-panel row.
    /// 渲染在可见提示词面板行中的紧凑预览文本。
    pub preview: String,
    /// Structured attachments retained until the row is inserted or removed.
    /// 在该行被插入或移除前一直保留的结构化附件集合。
    #[serde(default)]
    pub attachments: Vec<ConversationAttachment>,
    /// Optional provider/model identity locked when the row entered the visible queue.
    /// 可见队列项进入队列时锁定的可选供应商与模型身份。
    #[serde(default)]
    pub model_reference: Option<PendingPromptModelReference>,
    /// Runtime controls locked when the row entered the visible queue.
    /// 可见队列项进入队列时锁定的运行控制参数。
    #[serde(default)]
    pub runtime_options: PendingPromptRuntimeOptions,
    /// Stable insertion state used to hide controls after the prompt has entered the internal system queue.
    /// 提示词进入内部系统队列后用于隐藏控制按钮的稳定插入状态。
    #[serde(default)]
    pub state: PendingPromptPanelEntryState,
    /// Unix-millis text captured when the visible prompt-panel entry was created.
    /// 捕获该可见提示词面板项创建时间的 Unix 毫秒文本。
    pub created_at: String,
}

/// Session-scoped composer draft state retained independently for each session.
/// 按会话独立保留的输入框草稿状态。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SessionComposerDraftState {
    /// Draft text currently staged inside the session-scoped composer.
    /// 当前暂存在按会话归属输入框中的草稿文本。
    #[serde(default)]
    pub text: String,
    /// Draft attachments currently staged inside the session-scoped composer.
    /// 当前暂存在按会话归属输入框中的草稿附件集合。
    #[serde(default)]
    pub attachments: Vec<ConversationAttachment>,
    /// Vertical scroll offset mirrored from the session-scoped composer viewport.
    /// 从按会话归属输入框视口镜像出的垂直滚动偏移。
    #[serde(default)]
    pub scroll_offset: f32,
}

impl SessionComposerDraftState {
    /// Returns whether the draft carries no staged text, attachments, or scroll state.
    /// 返回该草稿是否不包含任何已暂存文本、附件或滚动状态。
    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty() && self.attachments.is_empty() && self.scroll_offset <= 0.0
    }
}

/// Session-scoped persisted transcript UI state.
/// 按会话持久化保存的转录界面状态。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionTranscriptUiState {
    /// Collapsed visible message identifiers persisted across restarts.
    /// 跨重启持久化保存的可见消息折叠标识集合。
    #[serde(default)]
    pub collapsed_message_ids: BTreeSet<String>,
    /// Pending prompt queue persisted with the session so queued follow-ups survive reloads.
    /// 随会话一起持久化保存的待发送提示词队列，用于让排队跟进跨重载保留。
    #[serde(default)]
    pub pending_prompt_queue: Vec<PendingSessionPromptEntry>,
    /// Visible prompt-panel queue persisted with the session so user-authored staged inserts survive reloads independently from the internal system queue.
    /// 随会话一起持久化保存的可见提示词面板队列，使用户分批插入项可以独立于内部系统队列跨重载保留。
    #[serde(default)]
    pub pending_prompt_panel_queue: Vec<PendingPromptPanelEntry>,
    /// Composer draft state persisted per session so switching sessions restores independent input text and attachments.
    /// 按会话持久化保存的输入框草稿状态，用于在切换会话时恢复独立的输入文本与附件。
    #[serde(default)]
    pub composer_draft: SessionComposerDraftState,
    /// Whether the session has unread assistant activity for the sidebar.
    /// 会话是否存在需要侧栏提示的未读助手活动。
    #[serde(default)]
    pub unread: bool,
}

impl SessionTranscriptUiState {
    /// Returns whether the transcript UI state carries no persisted user-visible state.
    /// 返回该转录界面状态是否不包含任何需要持久化的用户可见状态。
    pub fn is_empty(&self) -> bool {
        self.collapsed_message_ids.is_empty()
            && self.pending_prompt_queue.is_empty()
            && self.pending_prompt_panel_queue.is_empty()
            && self.composer_draft.is_empty()
            && !self.unread
    }
}

/// Routes one visible panel entry into the internal pending prompt queue.
/// 将单条可见面板条目路由进内部待发送提示词队列。
///
/// # Parameters
/// 参数
/// `ui_state` is the session-scoped UI state containing both prompt queues.
/// `ui_state` 是同时包含两个提示词队列的会话级 UI 状态。
/// `panel_entry_id` identifies the visible row to insert.
/// `panel_entry_id` 标识需要插入的可见行。
/// `internal_entry_id` is the stable id assigned to the new internal entry.
/// `internal_entry_id` 是分配给新内部条目的稳定标识。
/// `created_at` is the creation timestamp for the internal entry.
/// `created_at` 是内部条目的创建时间。
///
/// # Returns
/// 返回值
/// The route outcome or a queue routing error.
/// 路由结果或队列路由错误。
pub fn route_pending_prompt_panel_entry_into_internal_queue(
    ui_state: &mut SessionTranscriptUiState,
    panel_entry_id: &str,
    internal_entry_id: String,
    created_at: String,
) -> Result<PendingPromptQueueRouteOutcome, PendingPromptQueueRouteError> {
    let Some(panel_index) = ui_state
        .pending_prompt_panel_queue
        .iter()
        .position(|entry| entry.id == panel_entry_id)
    else {
        return Err(PendingPromptQueueRouteError::MissingPanelEntry);
    };
    if ui_state.pending_prompt_panel_queue[panel_index].state
        == PendingPromptPanelEntryState::Inserted
    {
        return Ok(PendingPromptQueueRouteOutcome::AlreadyInserted);
    }
    if ui_state.pending_prompt_queue.len() >= PENDING_PROMPT_QUEUE_LIMIT {
        return Err(PendingPromptQueueRouteError::InternalQueueFull);
    }
    let panel_entry = ui_state.pending_prompt_panel_queue[panel_index].clone();
    let internal_entry =
        pending_session_prompt_entry_from_panel_entry(&panel_entry, internal_entry_id, created_at);
    ui_state.pending_prompt_queue.push(internal_entry.clone());
    if let Some(panel_entry) = ui_state.pending_prompt_panel_queue.get_mut(panel_index) {
        panel_entry.state = PendingPromptPanelEntryState::Inserted;
    }
    Ok(PendingPromptQueueRouteOutcome::Inserted(internal_entry))
}

/// Takes the next dispatchable prompt from the internal queue or clones the first ready visible row.
/// 从内部队列取出下一条可分发提示词，或克隆第一条待命可见行。
///
/// # Parameters
/// 参数
/// `ui_state` is the session-scoped UI state containing both prompt queues.
/// `ui_state` 是同时包含两个提示词队列的会话级 UI 状态。
/// `prefer_delegate_completion` mirrors the iced Anthropic dispatch preference.
/// `prefer_delegate_completion` 对齐 iced 的 Anthropic 分发优先级。
/// `fallback_internal_entry_id` is used only when a visible ready row becomes the dispatch entry.
/// `fallback_internal_entry_id` 仅在可见待命行转为分发项时使用。
/// `fallback_created_at` is used only when a visible ready row becomes the dispatch entry.
/// `fallback_created_at` 仅在可见待命行转为分发项时使用。
///
/// # Returns
/// 返回值
/// The dispatch prompt entry when one is available.
/// 存在可分发条目时返回待分发提示词项。
pub fn take_next_pending_prompt_entry(
    ui_state: &mut SessionTranscriptUiState,
    prefer_delegate_completion: bool,
    fallback_internal_entry_id: String,
    fallback_created_at: String,
) -> Option<PendingSessionPromptEntry> {
    if let Some(entry) =
        take_next_internal_pending_prompt_entry(ui_state, prefer_delegate_completion)
    {
        return Some(entry);
    }
    let panel_index = ui_state
        .pending_prompt_panel_queue
        .iter()
        .position(|entry| entry.state == PendingPromptPanelEntryState::Ready)?;
    let panel_entry = ui_state.pending_prompt_panel_queue[panel_index].clone();
    Some(pending_session_prompt_entry_from_panel_entry(
        &panel_entry,
        fallback_internal_entry_id,
        fallback_created_at,
    ))
}

/// Takes the next already-internalized pending prompt without promoting a visible ready row.
/// 只取出下一条已经进入内部队列的待发送提示词，不提升可见待命行。
///
/// # Parameters
/// 参数
/// `ui_state` is the session-scoped UI state containing both prompt queues.
/// `ui_state` 是同时包含两个提示词队列的会话级 UI 状态。
/// `prefer_delegate_completion` mirrors the iced Anthropic dispatch preference.
/// `prefer_delegate_completion` 对齐 iced 的 Anthropic 分发优先级。
///
/// # Returns
/// 返回值
/// The internal dispatch prompt entry when one is available.
/// 存在内部可分发条目时返回待分发提示词项。
pub fn take_next_internal_pending_prompt_entry(
    ui_state: &mut SessionTranscriptUiState,
    prefer_delegate_completion: bool,
) -> Option<PendingSessionPromptEntry> {
    if ui_state.pending_prompt_queue.is_empty() {
        return None;
    }
    let dispatch_index = pending_prompt_dispatch_index(
        ui_state.pending_prompt_queue.as_slice(),
        prefer_delegate_completion,
    );
    Some(ui_state.pending_prompt_queue.remove(dispatch_index))
}

/// Removes one visible panel entry and any linked internal queue entries.
/// 移除单条可见面板条目及其关联的内部队列项。
///
/// # Parameters
/// 参数
/// `ui_state` is the session-scoped UI state containing both prompt queues.
/// `ui_state` 是同时包含两个提示词队列的会话级 UI 状态。
/// `panel_entry_id` identifies the visible row to remove.
/// `panel_entry_id` 标识需要移除的可见行。
///
/// # Returns
/// 返回值
/// The removed visible panel entry when it existed.
/// 若可见面板条目存在，则返回被移除的条目。
pub fn remove_pending_prompt_panel_entry(
    ui_state: &mut SessionTranscriptUiState,
    panel_entry_id: &str,
) -> Option<PendingPromptPanelEntry> {
    let panel_index = ui_state
        .pending_prompt_panel_queue
        .iter()
        .position(|entry| entry.id == panel_entry_id)?;
    let removed_entry = ui_state.pending_prompt_panel_queue.remove(panel_index);
    ui_state
        .pending_prompt_queue
        .retain(|entry| entry.panel_entry_id.as_deref() != Some(panel_entry_id));
    Some(removed_entry)
}

/// Applies the iced pending-prompt drop-target semantics to the shared visible panel queue.
/// 将 iced 的待发送提示词落点语义应用到共享可见面板队列。
///
/// # Parameters
/// 参数
/// `ui_state` is the session-scoped UI state containing both prompt queues.
/// `ui_state` 是同时包含两个提示词队列的会话级 UI 状态。
/// `dragged_entry_id` identifies the visible row being moved.
/// `dragged_entry_id` 标识正在移动的可见行。
/// `drop_target` is the iced-compatible insertion target.
/// `drop_target` 是与 iced 兼容的插入落点。
///
/// # Returns
/// 返回值
/// Whether the visible queue order changed.
/// 返回可见队列顺序是否发生变化。
pub fn apply_pending_prompt_panel_drop_target(
    ui_state: &mut SessionTranscriptUiState,
    dragged_entry_id: &str,
    drop_target: &PendingPromptPanelDropTarget,
) -> bool {
    let queue = &mut ui_state.pending_prompt_panel_queue;
    let Some(current_index) = queue.iter().position(|entry| entry.id == dragged_entry_id) else {
        return false;
    };
    let mut target_index = match drop_target {
        PendingPromptPanelDropTarget::Before(target_entry_id) => {
            let Some(index) = queue.iter().position(|entry| entry.id == *target_entry_id) else {
                return false;
            };
            index
        }
        PendingPromptPanelDropTarget::End => queue.len(),
    };
    let entry = queue.remove(current_index);
    if current_index < target_index {
        target_index = target_index.saturating_sub(1);
    }
    if target_index == current_index {
        queue.insert(current_index, entry);
        return false;
    }
    let bounded_target_index = target_index.min(queue.len());
    queue.insert(bounded_target_index, entry);
    true
}

/// Clears internal queued prompts and restores linked visible entries to ready.
/// 清空内部待发送提示词，并把关联的可见条目恢复为待命状态。
///
/// # Parameters
/// 参数
/// `ui_state` is the session-scoped UI state containing both prompt queues.
/// `ui_state` 是同时包含两个提示词队列的会话级 UI 状态。
///
/// # Returns
/// 返回值
/// Whether any queue state changed.
/// 是否发生了任何队列状态变更。
pub fn clear_internal_pending_prompt_queue_and_restore_panel(
    ui_state: &mut SessionTranscriptUiState,
) -> bool {
    let had_internal_entries = !ui_state.pending_prompt_queue.is_empty();
    let linked_panel_ids = ui_state
        .pending_prompt_queue
        .iter()
        .filter_map(|entry| entry.panel_entry_id.clone())
        .collect::<BTreeSet<_>>();
    ui_state.pending_prompt_queue.clear();
    let mut changed = had_internal_entries;
    for panel_entry in &mut ui_state.pending_prompt_panel_queue {
        if (linked_panel_ids.is_empty() || linked_panel_ids.contains(&panel_entry.id))
            && panel_entry.state == PendingPromptPanelEntryState::Inserted
        {
            panel_entry.state = PendingPromptPanelEntryState::Ready;
            changed = true;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one minimal visible pending-prompt panel entry for order tests.
    /// 为顺序测试构建一条最小可见待发送提示词面板项。
    fn test_panel_entry(id: &str) -> PendingPromptPanelEntry {
        PendingPromptPanelEntry {
            id: id.to_string(),
            prompt: format!("prompt-{id}"),
            preview: format!("prompt-{id}"),
            attachments: Vec::new(),
            model_reference: None,
            runtime_options: PendingPromptRuntimeOptions::default(),
            state: PendingPromptPanelEntryState::Ready,
            created_at: String::from("1000"),
        }
    }

    /// Returns the visible pending-prompt panel queue ids in order.
    /// 按顺序返回可见待发送提示词面板队列标识。
    fn panel_entry_ids(ui_state: &SessionTranscriptUiState) -> Vec<String> {
        ui_state
            .pending_prompt_panel_queue
            .iter()
            .map(|entry| entry.id.clone())
            .collect()
    }

    #[test]
    fn pending_prompt_panel_drop_target_matches_iced_before_semantics() {
        let mut ui_state = SessionTranscriptUiState {
            pending_prompt_panel_queue: vec![
                test_panel_entry("a"),
                test_panel_entry("b"),
                test_panel_entry("c"),
            ],
            ..SessionTranscriptUiState::default()
        };

        let changed = apply_pending_prompt_panel_drop_target(
            &mut ui_state,
            "a",
            &PendingPromptPanelDropTarget::Before(String::from("c")),
        );

        assert!(changed);
        assert_eq!(panel_entry_ids(&ui_state), vec!["b", "a", "c"]);
    }

    #[test]
    fn pending_prompt_panel_drop_target_matches_iced_end_semantics() {
        let mut ui_state = SessionTranscriptUiState {
            pending_prompt_panel_queue: vec![
                test_panel_entry("a"),
                test_panel_entry("b"),
                test_panel_entry("c"),
            ],
            ..SessionTranscriptUiState::default()
        };

        let changed = apply_pending_prompt_panel_drop_target(
            &mut ui_state,
            "a",
            &PendingPromptPanelDropTarget::End,
        );

        assert!(changed);
        assert_eq!(panel_entry_ids(&ui_state), vec!["b", "c", "a"]);
    }

    /// Verifies manually guided prompts preserve the user's click order.
    /// 验证手动引导的提示词会保留用户的点击顺序。
    #[test]
    fn routed_guidance_prompts_keep_first_in_first_out_order() {
        let mut ui_state = SessionTranscriptUiState {
            pending_prompt_panel_queue: vec![test_panel_entry("a"), test_panel_entry("b")],
            ..SessionTranscriptUiState::default()
        };

        route_pending_prompt_panel_entry_into_internal_queue(
            &mut ui_state,
            "a",
            String::from("internal-a"),
            String::from("1001"),
        )
        .expect("route first guidance prompt");
        route_pending_prompt_panel_entry_into_internal_queue(
            &mut ui_state,
            "b",
            String::from("internal-b"),
            String::from("1002"),
        )
        .expect("route second guidance prompt");

        let first = take_next_internal_pending_prompt_entry(&mut ui_state, false)
            .expect("take first guided prompt");
        let second = take_next_internal_pending_prompt_entry(&mut ui_state, false)
            .expect("take second guided prompt");

        assert_eq!(first.panel_entry_id.as_deref(), Some("a"));
        assert_eq!(second.panel_entry_id.as_deref(), Some("b"));
    }
}

/// One persisted VMM PostAction writeback retry entry.
/// 一条持久化的 VMM PostAction 写回重试项。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmmWritebackOutboxEntry {
    /// Stable local outbox entry identifier.
    /// 稳定的本地 outbox 条目标识。
    pub id: String,
    /// Original VMM PostAction request to replay.
    /// 需要重放的原始 VMM PostAction 请求。
    pub request: PostActionRequest,
    /// Number of failed delivery attempts recorded so far.
    /// 当前累计记录的失败投递次数。
    #[serde(default)]
    pub attempt_count: u32,
    /// Last failure message observed during delivery.
    /// 最近一次投递时观察到的失败消息。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    /// Unix-millis text captured when the entry was created.
    /// 捕获该条目创建时间的 Unix 毫秒文本。
    pub created_at: String,
    /// Unix-millis text captured when the entry was last attempted or updated.
    /// 捕获该条目最近尝试或更新时刻的 Unix 毫秒文本。
    pub updated_at: String,
}

/// Session-scoped VMM profile synchronization state.
/// 会话级 VMM 画像同步状态。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmmProfileSyncState {
    /// Latest compaction id already covered by a full profile hook injection.
    /// 已被全量画像 hook 注入覆盖的最近压缩标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub covered_compaction_id: Option<String>,
    /// Number of completed user turns since the latest profile sync check.
    /// 自最近一次画像同步检查以来已经完成的用户回合数。
    #[serde(default)]
    pub completed_turns_since_sync: u32,
    /// Latest active profile node snapshot keyed by VMM profile node id.
    /// 按 VMM 画像节点标识记录的最近 active 画像节点快照。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub nodes_by_id: BTreeMap<u64, ProfileNodeEntry>,
    /// Baseline full profile content reused on every provider request.
    /// 每次 Provider 请求都会复用的基础全量画像内容。
    #[serde(
        default,
        alias = "system_prompt_content",
        skip_serializing_if = "Option::is_none"
    )]
    pub base_profile_content: Option<String>,
    /// Latest full profile content used as the incremental refresh comparison baseline.
    /// 用作增量刷新对比基准的最近全量画像内容。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta_profile_content: Option<String>,
    /// Unix-millis text captured when the latest profile sync completed.
    /// 最近一次画像同步完成时捕获的 Unix 毫秒文本。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

impl VmmProfileSyncState {
    /// Returns whether no profile sync state has been initialized yet.
    /// 返回当前是否尚未初始化任何画像同步状态。
    pub fn is_empty(&self) -> bool {
        self.covered_compaction_id.is_none()
            && self.completed_turns_since_sync == 0
            && self.nodes_by_id.is_empty()
            && self.base_profile_content.is_none()
            && self.delta_profile_content.is_none()
            && self.updated_at.is_none()
    }

    /// Records one completed logical user turn after provider success.
    /// 在 Provider 成功后记录一个已完成逻辑用户回合。
    pub fn record_completed_turn(&mut self) {
        self.completed_turns_since_sync = self.completed_turns_since_sync.saturating_add(1);
    }
}

/// Session-scoped runtime state required for context reconstruction and agent tracing.
/// 上下文重建与代理追踪所需的按会话归属运行时状态。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionRuntimeStateEntry {
    /// Last active agent identifier used by the session.
    /// 最近一次被该会话使用的激活代理标识。
    pub active_agent_id: Option<String>,
    /// Last active provider identifier used by the session.
    /// 最近一次被该会话使用的激活供应商标识。
    pub active_provider_id: Option<String>,
    /// Last active model identifier used by the session.
    /// 最近一次被该会话使用的激活模型标识。
    pub active_model_id: Option<String>,
    /// Last active OpenRouter endpoint route locked by the session.
    /// 最近一次被该会话锁定使用的 OpenRouter 端点路由。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_openrouter_endpoint_route: Option<OpenRouterEndpointRouteSelection>,
    /// Last reasoning-effort preset selected while this session was active.
    /// 最近一次该会话激活时选择的推理档位预设。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_reasoning_effort: Option<ReasoningEffortLevel>,
    /// Last optional temperature override selected while this session was active.
    /// 最近一次该会话激活时选择的可选温度覆盖。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_temperature: Option<f32>,
    /// Session-level prompt skill state overrides.
    /// 会话级提示词技能状态覆盖。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub prompt_skill_overrides: BTreeMap<String, PersistedPromptSkillState>,
    /// Session-level context-injection runtime state kept outside the visible transcript.
    /// 保留在可见转录之外的会话级上下文注入运行态。
    #[serde(default, skip_serializing_if = "ContextInjectionState::is_empty")]
    pub context_injection_state: ContextInjectionState,
    /// Session-level VMM memory integration overrides.
    /// 会话级 VMM 记忆集成覆盖配置。
    #[serde(default, skip_serializing_if = "VmmConfigLayer::is_empty")]
    pub vmm: VmmConfigLayer,
    /// Pending VMM PostAction writebacks retained for FIFO replay.
    /// 为 FIFO 重放保留的待处理 VMM PostAction 写回队列。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vmm_writeback_outbox: Vec<VmmWritebackOutboxEntry>,
    /// Session-scoped VMM profile synchronization state.
    /// 会话级 VMM 画像同步状态。
    #[serde(default, skip_serializing_if = "VmmProfileSyncState::is_empty")]
    pub vmm_profile_sync: VmmProfileSyncState,
    /// Latest compaction identifier attached to the session.
    /// 附着在该会话上的最近一次压缩标识。
    pub last_compaction_id: Option<String>,
    /// Current message identifier used as the next context reconstruction anchor.
    /// 当前用作下一轮上下文重建锚点的消息标识。
    pub context_start_message_id: Option<String>,
    /// Live budget fingerprint captured from the latest provider-facing request shape.
    /// 从最近一次面向 Provider 请求形态捕获的实时预算指纹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_context_budget_fingerprint: Option<String>,
    /// Live layered-context fingerprint captured from the latest provider-facing request shape.
    /// 从最近一次面向 Provider 请求形态捕获的实时分层上下文指纹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_context_assembly_fingerprint: Option<String>,
    /// Anchor budget fingerprint captured when the active compaction baseline was accepted.
    /// 在接受当前压缩基线时捕获的锚点预算指纹。
    #[serde(
        default,
        alias = "last_context_budget_fingerprint",
        skip_serializing_if = "Option::is_none"
    )]
    pub anchor_context_budget_fingerprint: Option<String>,
    /// Anchor layered-context fingerprint captured when the active compaction baseline was accepted.
    /// 在接受当前压缩基线时捕获的锚点分层上下文指纹。
    #[serde(
        default,
        alias = "last_context_assembly_fingerprint",
        skip_serializing_if = "Option::is_none"
    )]
    pub anchor_context_assembly_fingerprint: Option<String>,
    /// Last persisted provider-request preview Markdown used by the dedicated API request transcript tab.
    /// 供专用 API 请求转录页签使用的最近一次持久化 Provider 请求预览 Markdown。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_provider_request_preview_markdown: Option<String>,
    /// Persisted transcript UI state restored with the session runtime metadata.
    /// 随会话运行时元数据一起恢复的持久化转录界面状态。
    pub ui_state: SessionTranscriptUiState,
    /// Canonical conversation state used by the runtime instead of rebuilding history from transcript projections.
    /// 运行时使用的规范会话状态，用于替代从转录投影重建历史。
    pub conversation_state: SessionConversationState,
    /// Latest provider usage snapshot retained for budget calibration across turns.
    /// 为跨轮预算校准保留的最近一次 Provider 用量快照。
    pub last_provider_usage: Option<ProviderUsageSnapshot>,
    /// Latest successful provider usage snapshot captured from the final real provider turn.
    /// 从最终真实 Provider 回合中捕获到的最近一次成功用量快照。
    pub last_successful_provider_usage: Option<ProviderUsageSnapshot>,
    /// Provider request count captured at the latest safe runtime checkpoint.
    /// 最近一次安全运行时检查点捕获到的 Provider 请求次数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_safe_provider_request_count: Option<usize>,
    /// Unix-millis text capturing when the runtime state was last refreshed.
    /// 捕获运行时状态最近刷新时间的 Unix 毫秒文本。
    pub updated_at: String,
}
