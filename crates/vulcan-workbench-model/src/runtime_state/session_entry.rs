use super::*;

/// Selects one explicit session override for a media-replacement model's reasoning effort.
/// 选择媒体替代模型的一项显式会话级推理档位覆盖。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionMediaUnderstandingReasoningEffort {
    /// Explicitly disables reasoning when the route declares that level.
    /// 当路由声明该档位时显式关闭推理。
    None,
    /// Uses provider-managed adaptive reasoning.
    /// 使用供应商管理的自适应推理。
    Adaptive,
    /// Uses the smallest declared reasoning effort.
    /// 使用已声明的最小推理档位。
    Minimal,
    /// Uses a low reasoning effort.
    /// 使用低推理档位。
    Low,
    /// Uses a balanced reasoning effort.
    /// 使用中等推理档位。
    Medium,
    /// Uses a high reasoning effort.
    /// 使用高推理档位。
    High,
    /// Uses an extra-high reasoning effort.
    /// 使用超高推理档位。
    #[serde(rename = "xhigh", alias = "x_high", alias = "x-high")]
    XHigh,
    /// Uses the provider-declared maximum reasoning effort.
    /// 使用供应商声明的最大推理档位。
    Max,
    /// Uses a provider-specific ultra reasoning effort.
    /// 使用供应商特有的极限推理档位。
    Ultra,
}

/// Exact replacement-model route selected by one session media-understanding capability.
/// 一项会话媒体理解能力选择的精确替代模型路由。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionMediaUnderstandingModelSelection {
    /// Stable provider identifier.
    /// 稳定供应商标识。
    pub provider_id: String,
    /// Exact provider model identifier.
    /// 精确供应商模型标识。
    pub model_id: String,
    /// Explicit credential connection identifier.
    /// 显式凭据连接标识。
    pub connection_id: String,
}

/// Exact backend route owned by one session media-understanding capability.
/// 一项会话媒体理解能力持有的精确后端路由。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum SessionMediaUnderstandingSelection {
    /// Execute one exact Manifest-owned provider tool target.
    /// 执行一个精确的 Manifest 供应商工具目标。
    ProviderTool {
        /// Exact provider target and credential selection.
        /// 精确供应商目标与凭据选择。
        selection: SessionProviderToolTargetSelection,
    },
    /// Execute one exact multimodal model as the replacement backend.
    /// 使用一个精确多模态模型作为替代理解后端。
    ModelReplacement {
        /// Exact model and credential selection.
        /// 精确模型与凭据选择。
        selection: SessionMediaUnderstandingModelSelection,
        /// Optional exact reasoning level selected for this route.
        /// 为此路由选择的可选精确推理档位。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reasoning_effort: Option<SessionMediaUnderstandingReasoningEffort>,
    },
}

impl From<SessionMediaUnderstandingReasoningEffort> for vulcan_ai_contract::ReasoningEffort {
    /// Converts the session-level media effort into the public AI request contract.
    /// 将会话级媒体推理档位转换为公共 AI 请求契约。
    ///
    /// # Parameters
    /// 参数
    /// `effort` is the exact session override selected for one media-understanding capability.
    /// `effort` 是为一项媒体理解能力选择的精确会话覆盖值。
    ///
    /// # Returns
    /// 返回值
    /// The equivalent provider-neutral request effort.
    /// 等价的供应商无关请求档位。
    fn from(effort: SessionMediaUnderstandingReasoningEffort) -> Self {
        match effort {
            SessionMediaUnderstandingReasoningEffort::None => Self::None,
            SessionMediaUnderstandingReasoningEffort::Adaptive => Self::Adaptive,
            SessionMediaUnderstandingReasoningEffort::Minimal => Self::Minimal,
            SessionMediaUnderstandingReasoningEffort::Low => Self::Low,
            SessionMediaUnderstandingReasoningEffort::Medium => Self::Medium,
            SessionMediaUnderstandingReasoningEffort::High => Self::High,
            SessionMediaUnderstandingReasoningEffort::XHigh => Self::XHigh,
            SessionMediaUnderstandingReasoningEffort::Max => Self::Max,
            SessionMediaUnderstandingReasoningEffort::Ultra => Self::Ultra,
        }
    }
}

impl From<vulcan_ai_contract::ReasoningEffort> for SessionMediaUnderstandingReasoningEffort {
    /// Converts the public AI request effort into the exact session persistence contract.
    /// 将公共 AI 请求档位转换为精确的会话持久化契约。
    ///
    /// # Parameters
    /// 参数
    /// `effort` is the provider-neutral reasoning level selected by the user.
    /// `effort` 是用户选择的供应商无关推理档位。
    ///
    /// # Returns
    /// 返回值
    /// The equivalent process-local session effort.
    /// 等价的进程内会话档位。
    fn from(effort: vulcan_ai_contract::ReasoningEffort) -> Self {
        match effort {
            vulcan_ai_contract::ReasoningEffort::None => Self::None,
            vulcan_ai_contract::ReasoningEffort::Adaptive => Self::Adaptive,
            vulcan_ai_contract::ReasoningEffort::Minimal => Self::Minimal,
            vulcan_ai_contract::ReasoningEffort::Low => Self::Low,
            vulcan_ai_contract::ReasoningEffort::Medium => Self::Medium,
            vulcan_ai_contract::ReasoningEffort::High => Self::High,
            vulcan_ai_contract::ReasoningEffort::XHigh => Self::XHigh,
            vulcan_ai_contract::ReasoningEffort::Max => Self::Max,
            vulcan_ai_contract::ReasoningEffort::Ultra => Self::Ultra,
        }
    }
}

impl From<SessionMediaUnderstandingReasoningEffort>
    for vulcan_session_core::v1::config::ProviderMediaUnderstandingReasoningEffort
{
    /// Converts the runtime session effort into the canonical V1 persistence contract.
    /// 将运行时会话档位转换为规范 V1 持久化契约。
    ///
    /// # Parameters
    /// 参数
    /// `effort` is the exact process-local session override.
    /// `effort` 是精确的进程内会话覆盖值。
    ///
    /// # Returns
    /// 返回值
    /// The equivalent provider-neutral V1 effort.
    /// 等价的供应商无关 V1 档位。
    fn from(effort: SessionMediaUnderstandingReasoningEffort) -> Self {
        match effort {
            SessionMediaUnderstandingReasoningEffort::None => Self::None,
            SessionMediaUnderstandingReasoningEffort::Adaptive => Self::Adaptive,
            SessionMediaUnderstandingReasoningEffort::Minimal => Self::Minimal,
            SessionMediaUnderstandingReasoningEffort::Low => Self::Low,
            SessionMediaUnderstandingReasoningEffort::Medium => Self::Medium,
            SessionMediaUnderstandingReasoningEffort::High => Self::High,
            SessionMediaUnderstandingReasoningEffort::XHigh => Self::XHigh,
            SessionMediaUnderstandingReasoningEffort::Max => Self::Max,
            SessionMediaUnderstandingReasoningEffort::Ultra => Self::Ultra,
        }
    }
}

impl From<vulcan_session_core::v1::config::ProviderMediaUnderstandingReasoningEffort>
    for SessionMediaUnderstandingReasoningEffort
{
    /// Restores one canonical V1 media effort into the runtime session contract.
    /// 将一项规范 V1 媒体档位恢复为会话运行时契约。
    ///
    /// # Parameters
    /// 参数
    /// `effort` is the validated provider-neutral V1 value.
    /// `effort` 是经过校验的供应商无关 V1 值。
    ///
    /// # Returns
    /// 返回值
    /// The equivalent process-local session effort.
    /// 等价的进程内会话档位。
    fn from(
        effort: vulcan_session_core::v1::config::ProviderMediaUnderstandingReasoningEffort,
    ) -> Self {
        use vulcan_session_core::v1::config::ProviderMediaUnderstandingReasoningEffort as V1Effort;

        match effort {
            V1Effort::None => Self::None,
            V1Effort::Adaptive => Self::Adaptive,
            V1Effort::Minimal => Self::Minimal,
            V1Effort::Low => Self::Low,
            V1Effort::Medium => Self::Medium,
            V1Effort::High => Self::High,
            V1Effort::XHigh => Self::XHigh,
            V1Effort::Max => Self::Max,
            V1Effort::Ultra => Self::Ultra,
        }
    }
}

/// Persisted non-secret identity of the provider route leased by one session.
/// 单个会话租用的供应商路由之持久化非秘密身份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionProviderRouteLeaseIdentity {
    /// Stable schema version for fail-closed restoration.
    /// 用于封闭失败恢复的稳定 Schema 版本。
    pub schema_version: u32,
    /// Exact provider identifier frozen by the lease.
    /// 租约冻结的精确供应商标识。
    pub provider_id: String,
    /// Exact credential connection identifier frozen by the lease.
    /// 租约冻结的精确凭据连接标识。
    pub connection_id: String,
    /// Exact model identifier frozen by the lease.
    /// 租约冻结的精确模型标识。
    pub model_id: String,
    /// Exact protocol profile selected for the frozen route.
    /// 冻结路由选择的精确协议 Profile。
    pub protocol_profile_id: String,
    /// Provider Manifest revision captured for diagnostics, not restoration identity.
    /// 为诊断捕获的供应商 Manifest 修订号，不作为恢复身份。
    pub manifest_revision: u64,
    /// Process-local Manifest catalog generation captured for diagnostics, not restoration identity.
    /// 为诊断捕获的进程内 Manifest 目录代数，不作为恢复身份。
    pub manifest_generation: u64,
    /// Content revision of the effective model catalog captured for diagnostics, not restoration identity.
    /// 为诊断捕获的生效模型目录内容修订号，不作为恢复身份。
    pub catalog_revision: String,
    /// Content-addressed revision of the complete execution route.
    /// 完整执行路由的内容寻址修订号。
    pub route_revision: String,
}

/// Exact session model choice accepted while the current turn keeps its frozen route.
/// 当前回合保持冻结路由时接受的精确会话模型选择。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionPendingModelSelection {
    /// Exact provider identifier selected for the next real Provider request.
    /// 为下一次真实 Provider 请求选择的精确供应商标识。
    pub provider_id: String,
    /// Exact credential connection identifier selected for the next real Provider request.
    /// 为下一次真实 Provider 请求选择的精确凭据连接标识。
    pub connection_id: String,
    /// Exact model identifier selected for the next real Provider request.
    /// 为下一次真实 Provider 请求选择的精确模型标识。
    pub model_id: String,
}

/// Session-scoped runtime state required for context reconstruction and agent tracing.
/// 上下文重建与代理追踪所需的按会话归属运行时状态。
/// Session-scoped output-style selection and the last safely applied prompt snapshot.
/// 会话级输出风格选择与最近一次安全应用的提示词快照。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionOutputStyleState {
    /// Explicit session override; `None` inherits the global style.
    /// 会话显式覆盖；为 `None` 时继承全局风格。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub override_style_id: Option<String>,
    /// Style identifier captured in the last accepted prompt boundary.
    /// 最近一个已接受提示词边界捕获的风格标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_style_id: Option<String>,
    /// Exact prompt text captured for continuity across catalog changes.
    /// 为保持连续性保存边界捕获时的精确提示词正文。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_prompt_text: Option<String>,
    /// Human-readable label captured with the applied snapshot.
    /// 与应用快照一起保存的人类可读标签。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_label: Option<String>,
    /// Stable prompt fingerprint captured with the applied snapshot.
    /// 与应用快照一起保存的稳定提示词指纹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_fingerprint: Option<u64>,
    /// Resource source captured with the applied snapshot.
    /// 与应用快照一起保存的资源来源标签。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_source_label: Option<String>,
    /// Activation boundary label, such as `new-session` or `context-compaction`.
    /// 激活边界标签，例如 `new-session` 或 `context-compaction`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_boundary: Option<String>,
    /// Compaction identifier that produced the current applied snapshot.
    /// 生成当前应用快照的压缩标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_compaction_id: Option<String>,
    /// Unix-millis text recording when the snapshot became effective.
    /// 记录快照生效时间的 Unix 毫秒文本。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_at: Option<String>,
}

impl SessionOutputStyleState {
    /// Returns whether no output-style state has been materialized.
    /// 返回是否尚未物化任何输出风格状态。
    pub fn is_empty(&self) -> bool {
        self.override_style_id.is_none()
            && self.applied_style_id.is_none()
            && self.applied_prompt_text.is_none()
            && self.applied_label.is_none()
            && self.applied_fingerprint.is_none()
            && self.applied_source_label.is_none()
            && self.applied_boundary.is_none()
            && self.applied_compaction_id.is_none()
            && self.applied_at.is_none()
    }

    /// Resolves the desired style identifier against the global selection.
    /// 按“会话覆盖优先、全局选择回退”规则解析目标风格标识。
    pub fn desired_style_id(&self, global_style_id: &str) -> String {
        self.override_style_id
            .clone()
            .unwrap_or_else(|| global_style_id.to_string())
    }

    /// Returns whether the desired style still waits for a safe boundary.
    /// 返回目标风格是否仍等待安全边界才能生效。
    pub fn has_pending_activation(&self, global_style_id: &str) -> bool {
        let desired = self.desired_style_id(global_style_id);
        self.applied_style_id.as_deref() != Some(desired.as_str())
    }

    /// Captures an immutable snapshot at a new-session or compaction boundary.
    /// 在新会话或压缩边界捕获并保存不可变风格快照。
    // Keep snapshot fields explicit because this method is the typed process-local projection boundary.
    // 保持快照字段显式，因为此方法是强类型进程内投影边界。
    #[allow(clippy::too_many_arguments)]
    pub fn set_applied_snapshot(
        &mut self,
        style_id: String,
        label: String,
        prompt_text: String,
        fingerprint: u64,
        source_label: String,
        boundary: String,
        compaction_id: Option<String>,
        applied_at: String,
    ) {
        self.applied_style_id = Some(style_id);
        self.applied_label = Some(label);
        self.applied_prompt_text = (!prompt_text.trim().is_empty()).then_some(prompt_text);
        self.applied_fingerprint = Some(fingerprint);
        self.applied_source_label = Some(source_label);
        self.applied_boundary = Some(boundary);
        self.applied_compaction_id = compaction_id;
        self.applied_at = Some(applied_at);
    }
}

/// Process-local execution adapter state that must never become session persistence, Provider history, or transcript authority.
/// 仅用于进程内执行适配的状态，绝不能成为会话持久化、Provider 历史或转录权威。
#[derive(Debug, Clone, Default)]
pub struct SessionRuntimeStateEntry {
    /// Last active agent identifier used by the session.
    /// 最近一次被该会话使用的激活代理标识。
    pub active_agent_id: Option<String>,
    /// Last active provider identifier used by the session.
    /// 最近一次被该会话使用的激活供应商标识。
    pub active_provider_id: Option<String>,
    /// Last credential connection identifier selected by the session.
    /// 该会话最近一次选择的凭据连接标识。
    pub active_connection_id: Option<String>,
    /// Last active model identifier used by the session.
    /// 最近一次被该会话使用的激活模型标识。
    pub active_model_id: Option<String>,
    /// Exact model selection waiting for the next real Provider-request boundary.
    /// 等待下一次真实 Provider 请求边界生效的精确模型选择。
    pub pending_model_selection: Option<SessionPendingModelSelection>,
    /// Process-local non-secret mirror of the session's frozen provider route.
    /// 当前会话冻结供应商路由的进程内非秘密镜像。
    pub provider_route_lease: Option<SessionProviderRouteLeaseIdentity>,
    /// Fully resolved context-window policy frozen at the latest accepted turn boundary.
    /// 最近一次接受的回合边界所冻结的完整上下文窗口策略。
    pub active_context_window_policy: Option<ContextWindowPolicy>,
    /// Stable context-compaction window node selected only for this session.
    /// 仅为当前会话选择的稳定上下文压缩窗口节点。
    pub context_compaction_window_node_id: Option<ContextCompactionWindowNodeId>,
    /// Last reasoning-effort preset selected while this session was active.
    /// 最近一次该会话激活时选择的推理档位预设。
    pub active_reasoning_effort: Option<ReasoningEffortLevel>,
    /// Records whether the session reasoning preference was explicitly chosen by the user.
    /// 记录会话思考偏好是否由用户明确选择，用于区分旧版默认值与显式关闭。
    pub active_reasoning_effort_explicit: bool,
    /// Last optional temperature override selected while this session was active.
    /// 最近一次该会话激活时选择的可选温度覆盖。
    pub active_temperature: Option<f32>,
    /// Session-selected provider tool targets keyed by semantic capability.
    /// 按语义能力索引的会话级供应商工具目标选择。
    pub provider_tool_selections: BTreeMap<String, SessionProviderToolTargetSelection>,
    /// Exact session-owned media-understanding routes keyed by semantic capability.
    /// 按语义能力索引的会话自有精确媒体理解路由。
    pub media_understanding_selections: BTreeMap<String, SessionMediaUnderstandingSelection>,
    /// Provider tool capabilities enabled only for this session.
    /// 仅为当前会话启用的供应商工具能力。
    pub provider_tool_enabled_capabilities: BTreeSet<String>,
    /// Process-local provider media-reference mirror for the active uncompressed conversation window.
    /// 活动未压缩会话窗口使用的进程内供应商媒体引用镜像。
    pub media_reference_ledger: SessionMediaReferenceLedger,
    /// Session-selected native model-extension target sets keyed by extension capability.
    /// 按扩展能力索引的会话级原生模型扩展目标集合。
    pub provider_extension_target_sets:
        BTreeMap<String, BTreeSet<SessionProviderToolTargetSelection>>,
    /// Whether provider tool defaults have been materialized into this session.
    /// 供应商工具默认值是否已经物化到当前会话。
    pub provider_tool_settings_initialized: bool,
    /// Whether this session owns an explicit provider-tool selection instead of inheriting global defaults.
    /// 当前会话是否拥有显式供应商工具选择，而不是继承全局默认值。
    pub provider_tool_selections_overridden: bool,
    /// Last user-selected work mode key for this session.
    /// 该会话最近一次选择的工作模式键。
    pub active_work_mode: Option<String>,
    /// Session-level prompt skill state overrides.
    /// 会话级提示词技能状态覆盖。
    pub prompt_skill_overrides: BTreeMap<String, PersistedPromptSkillState>,
    /// Session-level context-injection runtime state kept outside the visible transcript.
    /// 保留在可见转录之外的会话级上下文注入运行态。
    pub context_injection_state: ContextInjectionState,
    /// Session-level VMM memory integration overrides.
    /// 会话级 VMM 记忆集成覆盖配置。
    pub vmm: VmmConfigLayer,
    /// Process-local mirror of the session knowledge-library selection.
    /// 会话知识库选择的进程内镜像。
    pub knowledge_library_selection: SessionKnowledgeLibrarySelection,
    /// Session-level todolist shown in the conversation tool panel.
    /// 在会话工具面板中展示的会话级 todolist。
    pub todolist: SessionTodolistState,
    /// Session-level Plan mode marker and latest generated plan artifact.
    /// 会话级计划模式标记与最近生成的计划产物。
    pub plan_mode: SessionPlanModeState,
    /// Session output-style override and the last boundary-applied snapshot.
    /// 会话输出风格覆盖与最近一次边界应用的快照。
    pub output_style: SessionOutputStyleState,
    /// Pending VMM PostAction writebacks retained for FIFO replay.
    /// 为 FIFO 重放保留的待处理 VMM PostAction 写回队列。
    pub vmm_writeback_outbox: Vec<VmmWritebackOutboxEntry>,
    /// Session-scoped VMM profile synchronization state.
    /// 会话级 VMM 画像同步状态。
    pub vmm_profile_sync: VmmProfileSyncState,
    /// Latest compaction identifier attached to the session.
    /// 附着在该会话上的最近一次压缩标识。
    pub last_compaction_id: Option<String>,
    /// Consecutive automatic context-compaction failures observed for this session.
    /// 该会话连续自动上下文压缩失败次数。
    pub auto_compaction_consecutive_failures: usize,
    /// Current message identifier used as the next context reconstruction anchor.
    /// 当前用作下一轮上下文重建锚点的消息标识。
    pub context_start_message_id: Option<String>,
    /// Live budget fingerprint captured from the latest provider-facing request shape.
    /// 从最近一次面向 Provider 请求形态捕获的实时预算指纹。
    pub live_context_budget_fingerprint: Option<String>,
    /// Live layered-context fingerprint captured from the latest provider-facing request shape.
    /// 从最近一次面向 Provider 请求形态捕获的实时分层上下文指纹。
    pub live_context_assembly_fingerprint: Option<String>,
    /// Anchor budget fingerprint captured when the active compaction baseline was accepted.
    /// 在接受当前压缩基线时捕获的锚点预算指纹。
    pub anchor_context_budget_fingerprint: Option<String>,
    /// Anchor layered-context fingerprint captured when the active compaction baseline was accepted.
    /// 在接受当前压缩基线时捕获的锚点分层上下文指纹。
    pub anchor_context_assembly_fingerprint: Option<String>,
    /// Last process-local provider-request preview Markdown used by the dedicated API request transcript tab.
    /// 供专用 API 请求转录页签使用的最近一次进程内 Provider 请求预览 Markdown。
    pub last_provider_request_preview_markdown: Option<String>,
    /// Process-local transcript UI mirror projected from current V1 facts.
    /// 从当前 V1 事实投影出的进程内转录界面镜像。
    pub ui_state: SessionTranscriptUiState,
    /// Temporary conversation adapter state used only before the V1 Provider projection boundary.
    /// 仅在 V1 Provider 投影边界前使用的临时会话适配状态。
    pub conversation_state: SessionConversationState,
    /// Open tool-call record ids keyed by tool_call_id for direct result attachment.
    /// 按 tool_call_id 索引的未闭合工具调用记录标识，用于直接挂接工具返回。
    pub tool_call_record_ids: BTreeMap<String, String>,
    /// Open tool-request record ids keyed by request_stream_id before canonical execution starts.
    /// 在规范执行开始前按 request_stream_id 索引的未闭合工具请求记录标识。
    pub tool_request_record_ids: BTreeMap<String, String>,
    /// Latest client submission whose first semantic latency was recorded in this process.
    /// 当前进程中最近一次已经记录首语义延迟的客户端提交标识。
    pub observed_first_semantic_submission_id: Option<String>,
    /// Absolute input-token baseline for the current BodyAfterPrefix compaction window.
    /// 当前 BodyAfterPrefix 压缩窗口使用的绝对输入 Token 基线。
    pub auto_compact_prefill_input_tokens: Option<usize>,
    /// Active precheck turn id that is allowed to continue into the provider request.
    /// 当前允许继续进入 Provider 请求的活跃 precheck 回合标识。
    pub active_precheck_turn_id: Option<String>,
    /// Process-local session runtime-kernel mirror used for active mailbox delivery.
    /// 用于活动 mailbox 投递的进程内会话运行时内核镜像。
    pub session_runtime_kernel: Option<SessionRuntimeKernel>,
    /// Unix-millis text capturing when the runtime state was last refreshed.
    /// 捕获运行时状态最近刷新时间的 Unix 毫秒文本。
    pub updated_at: String,
}

/// Identifies one exact provider tool target selected for a session.
/// 标识会话选中的一个精确供应商工具目标。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SessionProviderToolTargetSelection {
    /// Credential connection that exposes the target.
    /// 暴露该目标的凭据连接。
    pub connection_id: String,
    /// Provider-declared target identifier.
    /// 供应商声明的目标标识。
    pub target_id: String,
}
