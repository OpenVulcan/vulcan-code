use super::*;

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
    /// Last credential connection identifier selected by the session.
    /// 该会话最近一次选择的凭据连接标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_connection_id: Option<String>,
    /// Last active model identifier used by the session.
    /// 最近一次被该会话使用的激活模型标识。
    pub active_model_id: Option<String>,
    /// Last reasoning-effort preset selected while this session was active.
    /// 最近一次该会话激活时选择的推理档位预设。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_reasoning_effort: Option<ReasoningEffortLevel>,
    /// Records whether the session reasoning preference was explicitly chosen by the user.
    /// 记录会话思考偏好是否由用户明确选择，用于区分旧版默认值与显式关闭。
    #[serde(default, skip_serializing_if = "is_false")]
    pub active_reasoning_effort_explicit: bool,
    /// Last optional temperature override selected while this session was active.
    /// 最近一次该会话激活时选择的可选温度覆盖。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_temperature: Option<f32>,
    /// Session-selected provider tool targets keyed by semantic capability.
    /// 按语义能力索引的会话级供应商工具目标选择。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub provider_tool_selections: BTreeMap<String, SessionProviderToolTargetSelection>,
    /// Provider tool capabilities enabled only for this session.
    /// 仅为当前会话启用的供应商工具能力。
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub provider_tool_enabled_capabilities: BTreeSet<String>,
    /// Whether provider tool defaults have been materialized into this session.
    /// 供应商工具默认值是否已经物化到当前会话。
    #[serde(default, skip_serializing_if = "is_false")]
    pub provider_tool_settings_initialized: bool,
    /// Whether this session owns an explicit provider-tool selection instead of inheriting global defaults.
    /// 当前会话是否拥有显式供应商工具选择，而不是继承全局默认值。
    #[serde(default, skip_serializing_if = "is_false")]
    pub provider_tool_selections_overridden: bool,
    /// Last user-selected work mode key for this session.
    /// 该会话最近一次选择的工作模式键。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_work_mode: Option<String>,
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
    /// Session knowledge-library selection persisted exclusively in the addon document.
    /// 仅持久化到附属状态文档的会话知识库选择。
    #[serde(skip)]
    pub knowledge_library_selection: SessionKnowledgeLibrarySelection,
    /// Session-level todolist shown in the conversation tool panel.
    /// 在会话工具面板中展示的会话级 todolist。
    #[serde(default, skip_serializing_if = "SessionTodolistState::is_empty")]
    pub todolist: SessionTodolistState,
    /// Session-level Plan mode marker and latest generated plan artifact.
    /// 会话级计划模式标记与最近生成的计划产物。
    #[serde(default, skip_serializing_if = "SessionPlanModeState::is_empty")]
    pub plan_mode: SessionPlanModeState,
    /// Session output-style override and the last boundary-applied snapshot.
    /// 会话输出风格覆盖与最近一次边界应用的快照。
    #[serde(default, skip_serializing_if = "SessionOutputStyleState::is_empty")]
    pub output_style: SessionOutputStyleState,
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
    /// Consecutive automatic context-compaction failures observed for this session.
    /// 该会话连续自动上下文压缩失败次数。
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    pub auto_compaction_consecutive_failures: usize,
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
    #[serde(default, skip_serializing_if = "session_conversation_state_is_empty")]
    pub conversation_state: SessionConversationState,
    /// Open tool-call record ids keyed by tool_call_id for direct result attachment.
    /// 按 tool_call_id 索引的未闭合工具调用记录标识，用于直接挂接工具返回。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tool_call_record_ids: BTreeMap<String, String>,
    /// Open tool-request record ids keyed by request_stream_id before canonical execution starts.
    /// 在规范执行开始前按 request_stream_id 索引的未闭合工具请求记录标识。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tool_request_record_ids: BTreeMap<String, String>,
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
    /// Active precheck turn id that is allowed to continue into the provider request.
    /// 当前允许继续进入 Provider 请求的活跃 precheck 回合标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_precheck_turn_id: Option<String>,
    /// Persisted single-source session runtime kernel used to restore mailbox delivery state.
    /// 用于恢复 mailbox 投递状态的持久化单一事实会话运行时内核。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_runtime_kernel: Option<SessionRuntimeKernel>,
    /// Unix-millis text capturing when the runtime state was last refreshed.
    /// 捕获运行时状态最近刷新时间的 Unix 毫秒文本。
    pub updated_at: String,
}

/// Identifies one exact provider tool target selected for a session.
/// 标识会话选中的一个精确供应商工具目标。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionProviderToolTargetSelection {
    /// Credential connection that exposes the target.
    /// 暴露该目标的凭据连接。
    pub connection_id: String,
    /// Provider-declared target identifier.
    /// 供应商声明的目标标识。
    pub target_id: String,
}
