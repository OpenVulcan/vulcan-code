use super::*;

impl TaskRuntime {
    /// Builds one empty task runtime with the default recursion guardrail.
    /// 构建一份带默认递归防护的空任务运行时。
    pub fn new() -> Self {
        let (subagent_mailbox_activity_tx, _) = watch::channel(0_u64);
        let (wait_agent_parent_interruption_tx, _) = watch::channel(0_u64);
        Self {
            sessions: RwLock::new(HashMap::new()),
            backends: RwLock::new(HashMap::new()),
            execution_contexts: RwLock::new(HashMap::new()),
            active_task_runs: RwLock::new(HashMap::new()),
            task_execution_state_locks: Mutex::new(HashMap::new()),
            detached_task_event_streams: RwLock::new(HashMap::new()),
            subagent_mailbox: InMemorySubagentMailbox::default(),
            subagent_mailbox_activity_tx,
            wait_agent_parent_interruption_tx,
            wait_agent_parent_interruption_sessions: RwLock::new(BTreeSet::new()),
            agent_inboxes: RwLock::new(HashMap::new()),
            subagent_registry: RwLock::new(SubagentRegistry::new()),
            subagent_catalog: RwLock::new(SubagentCatalog::default()),
            task_session_store_roots: RwLock::new(HashMap::new()),
            restored_task_session_registry_roots: RwLock::new(BTreeSet::new()),
            max_depth: DEFAULT_TASK_NESTING_LIMIT,
        }
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

    /// Notifies wait_agent that a real parent-side interruption arrived.
    /// 通知 wait_agent 真实父级插队或停止事件已到达。
    pub fn notify_wait_agent_parent_interruption(&self, parent_thread_id: SessionThreadId) {
        if let Ok(mut sessions) = self.wait_agent_parent_interruption_sessions.write() {
            sessions.insert(parent_thread_id);
        }
        let next_version = self
            .wait_agent_parent_interruption_tx
            .borrow()
            .saturating_add(1);
        let _ = self.wait_agent_parent_interruption_tx.send(next_version);
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

    /// Seeds one debug-only TaskRuntime resident subagent entry for service-layer tests.
    /// 为服务层测试播种一条仅调试构建可用的 TaskRuntime 常驻子代理条目。
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
    #[cfg(debug_assertions)]
    pub fn seed_debug_subagent_registry_entry(
        &self,
        seed: DebugSubagentRegistryEntrySeed,
    ) -> Result<(), String> {
        let DebugSubagentRegistryEntrySeed {
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

    /// Registers the file-domain root used by TaskSession artifacts under one root main session.
    /// 注册某个根主会话下 TaskSession 文件产物使用的文件域根目录。
    pub fn configure_task_session_store_root(
        &self,
        root_session_id: SessionId,
        root_session_dir: PathBuf,
    ) -> Result<(), String> {
        self.task_session_store_roots
            .write()
            .map_err(|_| {
                String::from("The task-session store root registry is locked and cannot update.")
            })?
            .insert(root_session_id, root_session_dir);
        self.restored_task_session_registry_roots
            .write()
            .map_err(|_| {
                String::from(
                    "The restored task-session registry marker is locked and cannot update.",
                )
            })?
            .remove(&root_session_id);
        Ok(())
    }

    /// Ensures the task-session store root exists without invalidating an unchanged restore marker.
    /// 确保任务会话存储根目录存在，并且在目录未变化时不使已恢复标记失效。
    ///
    /// # Parameters
    /// 参数
    /// `root_session_id` is the root conversation session id that owns the subagent task store.
    /// `root_session_id` 是拥有子代理任务存储的根对话会话标识。
    /// `root_session_dir` is the filesystem directory that stores segmented task-session artifacts.
    /// `root_session_dir` 是保存分段任务会话产物的文件系统目录。
    ///
    /// # Returns
    /// 返回值
    /// `Ok(())` when the root is registered or already matches the requested directory.
    /// 当根目录已注册或已经与请求目录一致时返回 `Ok(())`。
    pub fn ensure_task_session_store_root(
        &self,
        root_session_id: SessionId,
        root_session_dir: PathBuf,
    ) -> Result<(), String> {
        let root_changed = {
            let mut roots = self.task_session_store_roots.write().map_err(|_| {
                String::from("The task-session store root registry is locked and cannot update.")
            })?;
            match roots.get(&root_session_id) {
                Some(existing_root) if existing_root == &root_session_dir => false,
                _ => {
                    roots.insert(root_session_id, root_session_dir);
                    true
                }
            }
        };
        if root_changed {
            self.restored_task_session_registry_roots
                .write()
                .map_err(|_| {
                    String::from(
                        "The restored task-session registry marker is locked and cannot update.",
                    )
                })?
                .remove(&root_session_id);
        }
        Ok(())
    }

    /// Installs one backend handler for the given controller session and returns a guard that unregisters it on drop.
    /// 为给定控制器会话安装一个后端处理器，并返回一个在 Drop 时自动注销它的守卫。
    pub fn install_backend(
        self: &Arc<Self>,
        session_id: SessionId,
        handler: TaskExecutionHandler,
    ) -> Result<TaskBackendGuard, String> {
        self.backends
            .write()
            .map_err(|_| {
                String::from("The task backend registry is locked and cannot install a backend.")
            })?
            .insert(session_id, handler);
        Ok(TaskBackendGuard {
            runtime: Arc::clone(self),
            session_id,
        })
    }

    /// Installs one shared execution context for the given controller session and returns a guard that unregisters it on drop.
    /// 为给定控制器会话安装一份共享执行上下文，并返回一个在 Drop 时自动注销它的守卫。
    pub(crate) fn install_execution_context(
        self: &Arc<Self>,
        session_id: SessionId,
        context: TaskExecutionRuntimeContext,
    ) -> Result<TaskExecutionRuntimeContextGuard, String> {
        self.execution_contexts
            .write()
            .map_err(|_| {
                String::from(
                    "The task execution context registry is locked and cannot install context.",
                )
            })?
            .insert(session_id, context);
        Ok(TaskExecutionRuntimeContextGuard {
            runtime: Arc::clone(self),
            session_id,
        })
    }

    /// Returns one shared execution context for the given controller session when the normal runtime bridge registered it.
    /// 当普通运行时桥已经注册时，返回给定控制器会话对应的一份共享执行上下文。
    pub(super) fn execution_context_for_session(
        &self,
        session_id: SessionId,
    ) -> Option<TaskExecutionRuntimeContext> {
        let Ok(execution_contexts) = self.execution_contexts.read() else {
            eprintln!(
                "Task execution context registry is locked; context lookup for session {session_id} was skipped."
            );
            return None;
        };
        execution_contexts.get(&session_id).cloned()
    }

    /// Removes one shared execution context from the registry.
    /// 从注册表中移除一份共享执行上下文。
    pub(super) fn remove_execution_context(&self, session_id: SessionId) {
        let Ok(mut execution_contexts) = self.execution_contexts.write() else {
            eprintln!(
                "Task execution context registry is locked; context removal for session {session_id} was skipped."
            );
            return;
        };
        execution_contexts.remove(&session_id);
    }
}
