use super::*;

impl TaskRuntime {
    /// Builds one empty task runtime with the default recursion guardrail.
    /// 构建一份带默认递归防护的空任务运行时。
    pub fn new() -> Self {
        let (subagent_mailbox_activity_tx, _) = watch::channel(0_u64);
        let (wait_agent_parent_interruption_tx, _) = watch::channel(0_u64);
        Self {
            sensitive_tool_output_values: Arc::new(RwLock::new(Vec::new())),
            tool_permission_host: RwLock::new(None),
            tool_chain_host: RwLock::new(None),
            environment_host: RwLock::new(None),
            configuration_host: RwLock::new(None),
            luaskills_debug_host: RwLock::new(None),
            feedback_host: RwLock::new(None),
            session_mcp: Arc::new(crate::mcp::session::SessionMcpRegistry::new()),
            durable_return_mailboxes: RwLock::new(HashMap::new()),
            sessions: RwLock::new(HashMap::new()),
            backends: RwLock::new(HashMap::new()),
            execution_contexts: RwLock::new(HashMap::new()),
            active_task_runs: RwLock::new(HashMap::new()),
            auxiliary_execution_reservations: RwLock::new(HashMap::new()),
            collaboration_lifecycle_bindings: RwLock::new(HashMap::new()),
            task_execution_state_locks: Mutex::new(HashMap::new()),
            detached_task_event_streams: RwLock::new(HashMap::new()),
            subagent_mailbox: InMemorySubagentMailbox::default(),
            subagent_mailbox_activity_tx,
            wait_agent_parent_interruption_tx,
            wait_agent_parent_interruption_sessions: RwLock::new(BTreeSet::new()),
            agent_inboxes: RwLock::new(HashMap::new()),
            subagent_registry: RwLock::new(SubagentRegistry::new()),
            subagent_catalog: RwLock::new(SubagentCatalog::default()),
            max_depth: DEFAULT_TASK_NESTING_LIMIT,
        }
    }

    /// Registers one exact host credential for tool-result redaction before model dispatch.
    /// 在模型分派前注册一条需要从工具结果中遮蔽的精确宿主凭据。
    ///
    /// `secret` is a nontrivial host-owned value; the method returns a diagnostic without echoing it.
    /// `secret` 是非平凡的宿主持有值；本方法返回的诊断不会回显该值。
    pub fn register_sensitive_tool_output_value(&self, secret: String) -> Result<(), String> {
        if secret.len() < 16 {
            return Err(String::from(
                "Sensitive tool-output value is too short to register.",
            ));
        }
        self.insert_sensitive_tool_output_value(secret, String::from("[redacted]"))
    }

    /// Registers one secret environment value for tool-result redaction; results show
    /// `[redacted:NAME]` in its place. Values shorter than
    /// [`crate::tooling::native::ENVIRONMENT_SECRET_REDACTION_MIN_CHARS`] are skipped, since replacing them would mangle
    /// ordinary output; returns whether the value is redacted.
    /// 注册一个密钥环境变量值用于工具结果遮蔽；结果中以 `[redacted:NAME]` 代替它。短于
    /// [`crate::tooling::native::ENVIRONMENT_SECRET_REDACTION_MIN_CHARS`] 的值会跳过，因为替换它们会破坏普通输出；返回该值是否被遮蔽。
    pub fn register_environment_secret(&self, name: &str, value: &str) -> Result<bool, String> {
        if value.chars().count() < crate::tooling::native::ENVIRONMENT_SECRET_REDACTION_MIN_CHARS {
            return Ok(false);
        }
        self.insert_sensitive_tool_output_value(value.to_string(), format!("[redacted:{name}]"))?;
        Ok(true)
    }

    /// Adds one value unless it is already registered, keeping the longest values first.
    /// 添加一个尚未注册的值，并保持最长的值在前。
    fn insert_sensitive_tool_output_value(
        &self,
        value: String,
        replacement: String,
    ) -> Result<(), String> {
        let mut values = self
            .sensitive_tool_output_values
            .write()
            .map_err(|_| String::from("Sensitive tool-output registry is unavailable."))?;
        if !values.iter().any(|existing| existing.value == value) {
            values.push(SensitiveToolOutputValue { value, replacement });
            values.sort_by_key(|existing| std::cmp::Reverse(existing.value.len()));
        }
        Ok(())
    }

    /// Replaces every registered sensitive value in `text`, as tool results are redacted.
    /// 替换 `text` 中每个已登记的敏感值，与工具结果的遮蔽方式相同。
    ///
    /// Used for output the service hands to clients outside a tool result, such as shell job logs.
    /// A poisoned registry fails closed and returns an error instead of unfiltered text.
    /// 用于服务在工具结果之外交给客户端的输出，例如 shell 任务日志。注册表损坏时闭锁失败，返回错误而非未过滤文本。
    pub fn redact_sensitive_text(&self, text: &str) -> Result<String, String> {
        let values = self
            .sensitive_tool_output_values
            .read()
            .map_err(|_| String::from("Sensitive tool-output registry is unavailable."))?;
        let mut redacted = text.to_string();
        for value in values.iter() {
            if redacted.contains(&value.value) {
                redacted = redacted.replace(&value.value, &value.replacement);
            }
        }
        Ok(redacted)
    }

    /// Returns every registered sensitive value with the label shown in its place, for host-side
    /// filters outside tool results that must hide the same values, such as feedback reports.
    /// A poisoned registry fails closed.
    /// 返回每个已登记的敏感值及其替换处显示的标签，供工具结果之外必须隐藏相同值的宿主过滤器使用，
    /// 例如反馈报告。注册表损坏时闭锁失败。
    pub fn sensitive_values_with_labels(&self) -> Result<Vec<(String, String)>, String> {
        let values = self
            .sensitive_tool_output_values
            .read()
            .map_err(|_| String::from("Sensitive tool-output registry is unavailable."))?;
        Ok(values
            .iter()
            .map(|entry| {
                let label = entry
                    .replacement
                    .strip_prefix("[redacted:")
                    .and_then(|rest| rest.strip_suffix(']'))
                    .unwrap_or("secret");
                (label.to_owned(), entry.value.clone())
            })
            .collect())
    }

    /// Shares the live redaction registry with the host tool bridge for this runtime generation.
    /// 与当前运行世代的宿主工具桥共享实时遮蔽注册表。
    pub(crate) fn sensitive_tool_output_values(&self) -> SensitiveToolOutputValues {
        Arc::clone(&self.sensitive_tool_output_values)
    }

    /// Installs the service kernel's environment host; later tool bridges hand it to native tools.
    /// 安装服务内核的环境变量宿主；之后构建的工具桥把它交给原生工具。
    pub fn set_environment_host(
        &self,
        host: Arc<dyn crate::tooling::native::NativeEnvironmentHost>,
    ) {
        // The slot only holds an `Arc`, so a poisoned lock still carries a consistent value.
        // 该槽位只保存 `Arc`，因此锁中毒后其值仍然一致。
        *self
            .environment_host
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(host);
    }

    /// Returns the environment access handed to native tools, or `None` without a host.
    /// 返回交给原生工具的环境变量访问；没有宿主时返回 `None`。
    pub(crate) fn native_environment_access(
        self: &Arc<Self>,
    ) -> Option<crate::tooling::native::NativeEnvironmentAccess> {
        let host = self
            .environment_host
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()?;
        Some(crate::tooling::native::NativeEnvironmentAccess::new(
            host,
            Arc::downgrade(self),
        ))
    }

    /// Installs the service kernel's configuration host; later tool bridges hand it to the
    /// configuration tools.
    /// 安装服务内核的配置宿主；之后构建的工具桥把它交给配置工具。
    pub fn set_configuration_host(
        &self,
        host: Arc<dyn crate::tooling::native::NativeConfigurationHost>,
    ) {
        // The slot only holds an `Arc`, so a poisoned lock still carries a consistent value.
        // 该槽位只保存 `Arc`，因此锁中毒后其值仍然一致。
        *self
            .configuration_host
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(host);
    }

    /// Installs the service kernel's luaskills-debug host; later tool bridges hand it to the
    /// `luaskills_debug` tool.
    /// 安装服务内核的 luaskills-debug 宿主；之后构建的工具桥把它交给 `luaskills_debug` 工具。
    pub fn set_luaskills_debug_host(
        &self,
        host: Arc<dyn crate::tooling::native::NativeLuaSkillsDebugHost>,
    ) {
        // The slot only holds an `Arc`, so a poisoned lock still carries a consistent value.
        // 该槽位只保存 `Arc`，因此锁中毒后其值仍然一致。
        *self
            .luaskills_debug_host
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(host);
    }

    /// Returns the luaskills-debug host handed to native tools, or `None` without one.
    /// 返回交给原生工具的 luaskills-debug 宿主；没有时返回 `None`。
    pub(crate) fn luaskills_debug_host(
        &self,
    ) -> Option<Arc<dyn crate::tooling::native::NativeLuaSkillsDebugHost>> {
        self.luaskills_debug_host
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Installs the service kernel's feedback host; later tool bridges register the `feedback` tool
    /// and hand it this host.
    /// 安装服务内核的反馈宿主；之后构建的工具桥注册 `feedback` 工具并把该宿主交给它。
    pub fn set_feedback_host(&self, host: Arc<dyn crate::tooling::native::NativeFeedbackHost>) {
        // The slot only holds an `Arc`, so a poisoned lock still carries a consistent value.
        // 该槽位只保存 `Arc`，因此锁中毒后其值仍然一致。
        *self
            .feedback_host
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(host);
    }

    /// Returns the feedback host handed to native tools, or `None` without one.
    /// 返回交给原生工具的反馈宿主；没有时返回 `None`。
    pub(crate) fn feedback_host(
        &self,
    ) -> Option<Arc<dyn crate::tooling::native::NativeFeedbackHost>> {
        self.feedback_host
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Returns the configuration host handed to native tools, or `None` without one.
    /// 返回交给原生工具的配置宿主；没有时返回 `None`。
    pub(crate) fn configuration_host(
        &self,
    ) -> Option<Arc<dyn crate::tooling::native::NativeConfigurationHost>> {
        self.configuration_host
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Returns the session MCP access handed to native tools.
    /// 返回交给原生工具的会话 MCP 访问。
    pub(crate) fn native_session_mcp_access(
        self: &Arc<Self>,
    ) -> crate::tooling::native::NativeSessionMcpAccess {
        crate::tooling::native::NativeSessionMcpAccess::new(
            Arc::clone(&self.session_mcp),
            Arc::downgrade(self),
        )
    }

    /// The root session of `session_id`: its origin session for a spawned agent, itself otherwise.
    /// `session_id` 的根会话：派生代理为其来源会话，否则为其自身。
    pub(crate) fn root_session_of(&self, session_id: SessionId) -> Result<SessionId, String> {
        Ok(self
            .sessions
            .read()
            .map_err(|_| {
                String::from(
                    "The task-session registry is locked and cannot resolve the root session.",
                )
            })?
            .get(&session_id)
            .map(|record| record.origin_session_id)
            .unwrap_or(session_id))
    }

    /// Closes every MCP connection `session_id` opened; returns shutdown failures.
    /// 关闭 `session_id` 打开的全部 MCP 连接；返回关闭失败。
    pub async fn close_session_mcp_connections(&self, session_id: SessionId) -> Vec<String> {
        self.session_mcp.close_session(session_id).await
    }

    /// Installs the service kernel's permission host; later tool bridges consult it per call.
    /// 安装服务内核的权限宿主；之后构建的工具桥在每次调用时咨询它。
    pub fn set_tool_permission_host(
        &self,
        host: Arc<dyn crate::agent_runtime::tool_permission::ToolPermissionHost>,
    ) {
        // The slot only holds an `Arc`, so a poisoned lock still carries a consistent value.
        // 该槽位只保存 `Arc`，因此锁中毒后其值仍然一致。
        *self
            .tool_permission_host
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(host);
    }

    /// Installs the service kernel's live `tool_chain` settings host.
    /// 安装服务内核的实时 `tool_chain` 设置宿主。
    pub fn set_tool_chain_host(
        &self,
        host: Arc<dyn crate::agent_runtime::tool_runtime_bridge::ToolChainHost>,
    ) {
        // The slot only holds an `Arc`, so a poisoned lock still carries a consistent value.
        // 该槽位只保存 `Arc`，因此锁中毒后其值仍然一致。
        *self
            .tool_chain_host
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(host);
    }

    /// Returns the installed `tool_chain` settings host, if any.
    /// 返回已安装的 `tool_chain` 设置宿主（如有）。
    pub(crate) fn tool_chain_host(
        &self,
    ) -> Option<Arc<dyn crate::agent_runtime::tool_runtime_bridge::ToolChainHost>> {
        self.tool_chain_host
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Returns the installed permission host, or `None` when no policy is configured.
    /// 返回已安装的权限宿主；未配置策略时返回 `None`。
    pub(crate) fn tool_permission_host(
        &self,
    ) -> Option<Arc<dyn crate::agent_runtime::tool_permission::ToolPermissionHost>> {
        self.tool_permission_host
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Returns the maximum supported nested task depth.
    /// 返回支持的最大嵌套任务深度。
    pub fn max_depth(&self) -> usize {
        self.max_depth
    }

    /// Takes queued session-boundary subagent mailbox events for one parent thread.
    /// 取出指定父线程下排队的 会话边界子代理 mailbox 事件。
    pub fn take_subagent_events_for_parent(
        &self,
        parent_thread_id: SessionThreadId,
    ) -> Result<Vec<SubagentMailboxEvent>, String> {
        self.subagent_mailbox
            .take_events_for_parent(parent_thread_id)
    }

    /// Takes queued subagent mailbox events for one parent when the event matches a caller predicate.
    /// 按调用方谓词取出单个父级下匹配的子代理 mailbox 事件。
    pub(super) fn take_matching_subagent_events_for_parent<Matches>(
        &self,
        parent_thread_id: SessionThreadId,
        matches: Matches,
    ) -> Result<Vec<SubagentMailboxEvent>, String>
    where
        Matches: FnMut(&SubagentMailboxEvent) -> bool,
    {
        self.subagent_mailbox
            .take_matching_events_for_parent(parent_thread_id, matches)
    }

    /// Returns whether one parent thread has queued subagent mailbox events.
    /// 返回单个父线程是否存在排队的子代理 mailbox 事件。
    pub fn has_subagent_events_for_parent(
        &self,
        parent_thread_id: SessionThreadId,
    ) -> Result<bool, String> {
        self.subagent_mailbox
            .has_events_for_parent(parent_thread_id)
    }

    /// Subscribes to subagent mailbox activity notifications.
    /// 订阅子代理 mailbox 活动通知。
    pub(super) fn subscribe_subagent_mailbox_activity(&self) -> watch::Receiver<u64> {
        self.subagent_mailbox_activity_tx.subscribe()
    }

    /// Notifies waiters that the subagent mailbox changed.
    /// 通知等待方子代理 mailbox 已变化。
    pub(super) fn notify_subagent_mailbox_activity(&self) {
        let next_version = self.subagent_mailbox_activity_tx.borrow().saturating_add(1);
        let _ = self.subagent_mailbox_activity_tx.send(next_version);
    }

    /// Subscribes to explicit parent interruptions that should release wait_agent.
    /// 订阅应释放 wait_agent 的显式父级打断通知。
    pub(super) fn subscribe_wait_agent_parent_interruption(&self) -> watch::Receiver<u64> {
        self.wait_agent_parent_interruption_tx.subscribe()
    }

    /// Takes one pending explicit parent interruption for a parent thread.
    /// 取出指定父线程下一次待处理的显式父级打断。
    pub(super) fn take_wait_agent_parent_interruption_for_parent(
        &self,
        parent_thread_id: SessionThreadId,
    ) -> Result<bool, String> {
        self.wait_agent_parent_interruption_sessions
            .write()
            .map_err(|_| String::from("The wait-agent interruption registry is locked."))
            .map(|mut sessions| sessions.remove(&parent_thread_id))
    }

    /// Records and publishes a real parent-side interruption for wait_agent.
    /// 记录并发布 wait_agent 的真实父级插队或停止事件。
    ///
    /// # Parameters
    /// 参数
    /// `parent_thread_id` identifies the parent thread whose active wait must be interrupted.
    /// `parent_thread_id` 标识其活跃等待必须被打断的父线程。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after the interruption fact is stored, or a registry diagnostic on failure.
    /// 中断事实存储成功后返回成功，失败时返回注册表诊断。
    pub fn notify_wait_agent_parent_interruption(
        &self,
        parent_thread_id: SessionThreadId,
    ) -> Result<(), String> {
        // sessions stores the authoritative interruption fact before any waiter is awakened.
        // sessions 在唤醒任何等待方之前存储权威中断事实。
        let mut sessions = self
            .wait_agent_parent_interruption_sessions
            .write()
            .map_err(|_| {
                format!(
                    "The wait-agent interruption registry is locked for parent thread {parent_thread_id}."
                )
            })?;
        sessions.insert(parent_thread_id);
        drop(sessions);
        // nextVersion is only published after the interruption fact is durable in the registry.
        // nextVersion 仅在中断事实已持久到注册表后发布。
        let next_version = self
            .wait_agent_parent_interruption_tx
            .borrow()
            .saturating_add(1);
        let _ = self.wait_agent_parent_interruption_tx.send(next_version);
        Ok(())
    }

    /// Returns a clone of the Vulcan-native subagent registry.
    /// 返回 Vulcan 原生子代理注册表的克隆快照。
    ///
    /// # Returns
    /// 返回值
    /// A registry snapshot that callers can inspect without holding runtime locks.
    /// 调用方可在不持有运行时锁的情况下检查的注册表快照。
    pub fn subagent_registry_snapshot(&self) -> Result<SubagentRegistry, String> {
        self.subagent_registry
            .read()
            .map_err(|_| String::from("The subagent registry is locked and cannot be read."))
            .map(|registry| registry.clone())
    }

    /// Seeds one test-only TaskRuntime resident subagent entry for service-layer tests.
    /// 为服务层测试播种一条仅测试使用的 TaskRuntime 常驻子代理条目。
    ///
    /// # Parameters
    /// 参数
    /// `seed` contains the exact task identity, ownership, display, and result fields to mirror.
    /// `seed` 包含需要镜像的精确任务标识、归属、展示与结果字段。
    ///
    /// # Returns
    /// 返回值
    /// `Ok(())` when the task record and registry entry are inserted successfully.
    /// 任务记录与注册表条目成功写入时返回 `Ok(())`。
    #[cfg(feature = "test-support")]
    pub fn seed_test_subagent_registry_entry(
        &self,
        seed: TestSubagentRegistryEntrySeed,
    ) -> Result<(), String> {
        let TestSubagentRegistryEntrySeed {
            root_session_id,
            parent_session_id,
            task_id,
            public_task_id,
            display_title,
            description,
            subagent,
            status,
            result_summary,
        } = seed;
        let prompt = result_summary
            .clone()
            .unwrap_or_else(|| display_title.clone());
        let mut record = build_task_session_record(TaskSessionRecordInit {
            task_id,
            public_task_id,
            origin_session_id: root_session_id,
            parent_task_id: (parent_session_id != root_session_id).then_some(parent_session_id),
            depth: if parent_session_id == root_session_id {
                1
            } else {
                2
            },
            description,
            subagent,
            invocation_config: TaskInvocationConfig::main(prompt.clone()),
            display_title,
            controller_config: None,
            snapshot: CoreConversationSnapshot::default(),
            last_prompt: prompt,
            last_status: status_name_for_task_session_status(status).to_string(),
        });
        record.last_result_text = result_summary.unwrap_or_default();
        self.sessions
            .write()
            .map_err(|_| {
                String::from(
                    "The task-session registry is locked and cannot seed debug subagent state.",
                )
            })?
            .insert(task_id, record.clone());
        let registry_summary = (!record.last_result_text.trim().is_empty())
            .then_some(record.last_result_text.as_str());
        self.upsert_subagent_registry_entry(&record, status, registry_summary)
    }

    /// Binds `execution_id` to an existing test-seeded `task_id` without changing its status or result.
    /// 将 `execution_id` 绑定到已有测试播种 `task_id`，不改变其状态或结果。
    /// Returns an explicit missing-record error; only cross-crate test-support builds expose this entry.
    /// 记录缺失时返回明确错误；仅跨 crate 测试支持构建暴露此入口。
    #[cfg(feature = "test-support")]
    pub fn bind_test_subagent_execution(
        &self,
        task_id: SessionId,
        execution_id: &str,
    ) -> Result<(), String> {
        if execution_id.is_empty() {
            return Err(String::from("Test child execution identity is empty."));
        }
        // Test fixture binding never creates a missing resident record or changes business completion.
        // 测试夹具绑定绝不创建缺失常驻记录，也不改变业务完成状态。
        let mut sessions = self
            .sessions
            .write()
            .map_err(|_| String::from("Test child registry is locked."))?;
        let record = sessions
            .get_mut(&task_id)
            .ok_or_else(|| String::from("Test child record is missing."))?;
        record.execution_id = Some(execution_id.to_owned());
        Ok(())
    }

    /// Replaces the merged subagent catalog used by future spawn_agent calls.
    /// 替换后续 spawn_agent 调用使用的合并子代理目录。
    ///
    /// # Parameters
    /// 参数
    /// `catalog` is the latest merged catalog from system and user roots.
    /// `catalog` 是来自系统根与用户根的最新合并目录。
    pub fn replace_subagent_catalog(&self, catalog: SubagentCatalog) -> Result<(), String> {
        self.subagent_catalog
            .write()
            .map_err(|_| String::from("The subagent catalog is locked and cannot be updated."))?
            .clone_from(&catalog);
        Ok(())
    }

    /// Updates one catalog definition enabled state in-place.
    /// 原地更新单个目录定义的启停状态。
    ///
    /// # Parameters
    /// 参数
    /// `agent_id` is the stable catalog definition id.
    /// `agent_id` 是稳定目录定义标识。
    /// `enabled` is the next model exposure state.
    /// `enabled` 是新的模型暴露状态。
    ///
    /// # Returns
    /// 返回值
    /// Whether a matching catalog definition was updated.
    /// 返回是否更新了匹配的目录定义。
    pub fn set_subagent_definition_enabled(
        &self,
        agent_id: &str,
        enabled: bool,
    ) -> Result<bool, String> {
        let mut catalog = self
            .subagent_catalog
            .write()
            .map_err(|_| String::from("The subagent catalog is locked and cannot be updated."))?;
        let Some(definition) = catalog
            .definitions
            .iter_mut()
            .find(|definition| definition.id == agent_id)
        else {
            return Ok(false);
        };
        definition.enabled = enabled;
        Ok(true)
    }

    /// Returns a clone of the merged subagent catalog.
    /// 返回合并子代理目录的克隆快照。
    ///
    /// # Returns
    /// 返回值
    /// A catalog snapshot that callers can inspect without holding runtime locks.
    /// 调用方可在不持有运行时锁的情况下检查的目录快照。
    pub fn subagent_catalog_snapshot(&self) -> Result<SubagentCatalog, String> {
        self.subagent_catalog
            .read()
            .map_err(|_| String::from("The subagent catalog is locked and cannot be read."))
            .map(|catalog| catalog.clone())
    }

    /// Resolves one enabled catalog definition by string id.
    /// 按字符串标识解析一个已启用目录定义。
    ///
    /// # Parameters
    /// 参数
    /// `agent_id` is the model-visible catalog id, usually a Markdown file stem.
    /// `agent_id` 是模型可见目录标识，通常为 Markdown 文件 stem。
    ///
    /// # Returns
    /// 返回值
    /// The enabled definition or an explicit error.
    /// 已启用定义或显式错误。
    pub fn enabled_subagent_definition(
        &self,
        agent_id: &str,
    ) -> Result<SubagentDefinition, String> {
        self.subagent_catalog
            .read()
            .map_err(|_| String::from("The subagent catalog is locked and cannot be read."))?
            .enabled_definition(agent_id)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "Subagent definition `{agent_id}` is not enabled or does not exist in the catalog."
                )
            })
    }

    /// Installs one backend handler for the given controller session and returns a guard that unregisters it on drop.
    /// 为给定控制器会话安装一个后端处理器，并返回一个在 Drop 时自动注销它的守卫。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` is the controller-session key to create or replace.
    /// `session_id` 是需要创建或替换的控制器会话键。
    /// `handler` is the task backend owned by the new registration generation.
    /// `handler` 是由新注册世代拥有的任务后端。
    ///
    /// # Returns
    /// 返回值
    /// A guard owning the exact installed generation, or a registry-lock error.
    /// 返回拥有精确安装世代的守卫，或注册表锁错误。
    pub fn install_backend(
        self: &Arc<Self>,
        session_id: SessionId,
        handler: TaskExecutionHandler,
    ) -> Result<TaskBackendGuard, String> {
        // registrationId identifies the exact backend generation owned by the returned guard.
        // registrationId 标识返回守卫所拥有的精确后端注册世代。
        let registration_id = Uuid::new_v4();
        self.backends
            .write()
            .map_err(|_| {
                String::from("The task backend registry is locked and cannot install a backend.")
            })?
            .insert(
                session_id,
                TaskRuntimeRegistration {
                    registration_id,
                    value: handler,
                },
            );
        Ok(TaskBackendGuard {
            runtime: Arc::clone(self),
            session_id,
            registration_id,
            released: false,
        })
    }

    /// Installs one shared execution context for the given controller session and returns a guard that unregisters it on drop.
    /// 为给定控制器会话安装一份共享执行上下文，并返回一个在 Drop 时自动注销它的守卫。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` is the controller-session key to create or replace.
    /// `session_id` 是需要创建或替换的控制器会话键。
    /// `context` is the shared execution context owned by the new registration generation.
    /// `context` 是由新注册世代拥有的共享执行上下文。
    ///
    /// # Returns
    /// 返回值
    /// A guard owning the exact installed generation, or a registry-lock error.
    /// 返回拥有精确安装世代的守卫，或注册表锁错误。
    pub(crate) fn install_execution_context(
        self: &Arc<Self>,
        session_id: SessionId,
        context: TaskExecutionRuntimeContext,
    ) -> Result<TaskExecutionRuntimeContextGuard, String> {
        // registrationId identifies the exact context generation owned by the returned guard.
        // registrationId 标识返回守卫所拥有的精确上下文注册世代。
        let registration_id = Uuid::new_v4();
        self.execution_contexts
            .write()
            .map_err(|_| {
                String::from(
                    "The task execution context registry is locked and cannot install context.",
                )
            })?
            .insert(
                session_id,
                TaskRuntimeRegistration {
                    registration_id,
                    value: context,
                },
            );
        Ok(TaskExecutionRuntimeContextGuard {
            runtime: Arc::clone(self),
            session_id,
            registration_id,
            released: false,
        })
    }

    /// Returns one shared execution context for the given controller session when the normal runtime bridge registered it.
    /// 当普通运行时桥已经注册时，返回给定控制器会话对应的一份共享执行上下文。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` identifies the controller session whose exact runtime context is requested.
    /// `session_id` 标识需要读取精确运行时上下文的控制器会话。
    ///
    /// # Returns
    /// 返回值
    /// Returns the registered context when present, `None` when genuinely absent, or an explicit registry-read error.
    /// 已注册时返回上下文，确实不存在时返回 `None`，注册表读取失败时返回明确错误。
    pub(super) fn execution_context_for_session(
        &self,
        session_id: SessionId,
    ) -> Result<Option<TaskExecutionRuntimeContext>, String> {
        // execution_contexts is the only authoritative session-to-runtime-context registry.
        // execution_contexts 是唯一权威的会话到运行时上下文注册表。
        let execution_contexts = self.execution_contexts.read().map_err(|_| {
            format!(
                "The task execution context registry is locked and cannot inspect session `{session_id}`."
            )
        })?;
        Ok(execution_contexts
            .get(&session_id)
            .map(|registration| registration.value.clone()))
    }

    /// Removes one shared execution context only when the caller owns the current registration generation.
    /// 仅当调用方拥有当前注册世代时，从注册表中移除一份共享执行上下文。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` identifies the controller-session registry key.
    /// `session_id` 标识控制器会话注册表键。
    /// `registration_id` identifies the exact context generation owned by the dropping guard.
    /// `registration_id` 标识正在销毁的守卫所拥有的精确上下文世代。
    ///
    /// # Returns
    /// 返回值
    /// Success when the owned generation is absent or removed, otherwise a registry-lock error.
    /// 所属世代已不存在或已移除时返回成功，否则返回注册表锁错误。
    pub(super) fn remove_execution_context(
        &self,
        session_id: SessionId,
        registration_id: Uuid,
    ) -> Result<(), String> {
        let mut execution_contexts = self.execution_contexts.write().map_err(|_| {
            format!(
                "The task execution context registry is locked and cannot remove context for session `{session_id}`."
            )
        })?;
        // ownsCurrentRegistration prevents a stale guard from deleting a newer replacement.
        // ownsCurrentRegistration 防止过期守卫删除更新的替代注册项。
        let owns_current_registration = execution_contexts
            .get(&session_id)
            .is_some_and(|registration| registration.registration_id == registration_id);
        if owns_current_registration {
            execution_contexts.remove(&session_id);
        }
        Ok(())
    }
}
