use super::*;

impl TaskRuntime {
    /// Builds one empty task runtime with the default recursion guardrail.
    /// 构建一份带默认递归防护的空任务运行时。
    pub fn new() -> Self {
        let (subagent_mailbox_activity_tx, _) = watch::channel(0_u64);
        let (parent_input_activity_tx, _) = watch::channel(0_u64);
        Self {
            sessions: RwLock::new(HashMap::new()),
            backends: RwLock::new(HashMap::new()),
            execution_contexts: RwLock::new(HashMap::new()),
            active_task_runs: RwLock::new(HashMap::new()),
            detached_task_event_streams: RwLock::new(HashMap::new()),
            subagent_mailbox: InMemorySubagentMailbox::default(),
            subagent_mailbox_activity_tx,
            parent_input_activity_tx,
            parent_input_activity_sessions: RwLock::new(BTreeSet::new()),
            agent_inboxes: RwLock::new(HashMap::new()),
            subagent_registry: RwLock::new(SubagentRegistry::new()),
            task_session_store_roots: RwLock::new(HashMap::new()),
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

    /// Subscribes to parent input activity notifications.
    /// 订阅父级输入活动通知。
    pub(super) fn subscribe_parent_input_activity(&self) -> watch::Receiver<u64> {
        self.parent_input_activity_tx.subscribe()
    }

    /// Takes pending parent input activity for one parent thread.
    /// 取出指定父线程下待处理的父级输入活动。
    pub(super) fn take_parent_input_activity_for_parent(
        &self,
        parent_thread_id: SessionThreadId,
    ) -> Result<bool, String> {
        self.parent_input_activity_sessions
            .write()
            .map_err(|_| String::from("The parent-input activity registry is locked."))
            .map(|mut sessions| sessions.remove(&parent_thread_id))
    }

    /// Notifies waiters that new parent-side input or steering arrived.
    /// 通知等待方父级已有新的输入或插话到达。
    pub fn notify_parent_input_activity(&self, parent_thread_id: SessionThreadId) {
        if let Ok(mut sessions) = self.parent_input_activity_sessions.write() {
            sessions.insert(parent_thread_id);
        }
        let next_version = self.parent_input_activity_tx.borrow().saturating_add(1);
        let _ = self.parent_input_activity_tx.send(next_version);
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
        Ok(())
    }

    /// Installs one backend handler for the given controller session and returns a guard that unregisters it on drop.
    /// 为给定控制器会话安装一个后端处理器，并返回一个在 Drop 时自动注销它的守卫。
    pub fn install_backend(
        self: &Arc<Self>,
        session_id: SessionId,
        handler: TaskExecutionHandler,
    ) -> TaskBackendGuard {
        self.backends
            .write()
            .expect("task backend registry lock")
            .insert(session_id, handler);
        TaskBackendGuard {
            runtime: Arc::clone(self),
            session_id,
        }
    }

    /// Installs one shared execution context for the given controller session and returns a guard that unregisters it on drop.
    /// 为给定控制器会话安装一份共享执行上下文，并返回一个在 Drop 时自动注销它的守卫。
    pub(crate) fn install_execution_context(
        self: &Arc<Self>,
        session_id: SessionId,
        context: TaskExecutionRuntimeContext,
    ) -> TaskExecutionRuntimeContextGuard {
        self.execution_contexts
            .write()
            .expect("task execution context registry lock")
            .insert(session_id, context);
        TaskExecutionRuntimeContextGuard {
            runtime: Arc::clone(self),
            session_id,
        }
    }

    /// Returns one shared execution context for the given controller session when the normal runtime bridge registered it.
    /// 当普通运行时桥已经注册时，返回给定控制器会话对应的一份共享执行上下文。
    pub(super) fn execution_context_for_session(
        &self,
        session_id: SessionId,
    ) -> Option<TaskExecutionRuntimeContext> {
        self.execution_contexts
            .read()
            .expect("task execution context registry lock")
            .get(&session_id)
            .cloned()
    }

    /// Removes one shared execution context from the registry.
    /// 从注册表中移除一份共享执行上下文。
    pub(super) fn remove_execution_context(&self, session_id: SessionId) {
        self.execution_contexts
            .write()
            .expect("task execution context registry lock")
            .remove(&session_id);
    }
}
