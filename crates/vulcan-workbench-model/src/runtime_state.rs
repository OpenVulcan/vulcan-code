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
}

impl SessionTranscriptUiState {
    /// Returns whether the transcript UI state carries no persisted user-visible state.
    /// 返回该转录界面状态是否不包含任何需要持久化的用户可见状态。
    pub fn is_empty(&self) -> bool {
        self.collapsed_message_ids.is_empty()
            && self.pending_prompt_queue.is_empty()
            && self.pending_prompt_panel_queue.is_empty()
            && self.composer_draft.is_empty()
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
