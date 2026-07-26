use super::*;

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
    /// Last user-selected work mode key for this session.
    /// 该会话最近一次选择的工作模式键。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_work_mode: Option<String>,
    /// Session-scoped AI capability routing configuration cloned from the default template.
    /// 从默认模板复制而来的会话级 AI 能力路由配置。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_tool_configuration: Option<PersistedAiToolConfiguration>,
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
    /// Session-owned VMM lease state persisted exclusively in the independent `vmm.json` document.
    /// 仅持久化到独立 `vmm.json` 文档的会话归属 VMM 租约状态。
    #[serde(skip)]
    pub vmm_session_memory: VmmSessionMemoryState,
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

impl SessionRuntimeStateEntry {
    /// Ensures the session owns an AI capability configuration cloned from the default template.
    /// 确保当前会话拥有一份从默认模板复制而来的 AI 能力配置。
    ///
    /// # Parameters
    /// 参数
    /// `default_configuration` is the current default AI capability template.
    /// `default_configuration` 是当前默认 AI 能力配置模板。
    ///
    /// # Returns
    /// 返回值
    /// `true` when the session configuration was initialized by this call.
    /// 当本次调用初始化了会话配置时返回 `true`。
    pub fn ensure_ai_tool_configuration(
        &mut self,
        default_configuration: &PersistedAiToolConfiguration,
    ) -> bool {
        if self.ai_tool_configuration.is_some() {
            return false;
        }
        self.ai_tool_configuration = Some(default_configuration.clone());
        true
    }

    /// Returns the session AI capability configuration or the default template when not initialized yet.
    /// 返回会话 AI 能力配置；若尚未初始化则返回默认模板。
    ///
    /// # Parameters
    /// 参数
    /// `default_configuration` is the fallback default AI capability template.
    /// `default_configuration` 是作为回退使用的默认 AI 能力配置模板。
    ///
    /// # Returns
    /// 返回值
    /// The effective session-scoped AI capability configuration reference.
    /// 生效的会话级 AI 能力配置引用。
    pub fn ai_tool_configuration_or_default<'a>(
        &'a self,
        default_configuration: &'a PersistedAiToolConfiguration,
    ) -> &'a PersistedAiToolConfiguration {
        self.ai_tool_configuration
            .as_ref()
            .unwrap_or(default_configuration)
    }
}
