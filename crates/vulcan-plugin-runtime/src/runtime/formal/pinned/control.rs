//! Exact session invocation, owner controls and retained result acknowledgement.
//! 精确会话调用、所有者控制及保留结果确认。

use super::*;

impl VulcanPluginRuntimeManager {
    /// Invoke one validated `payload` on the exact owned session without holding host locks across execution.
    /// 在精确所属会话上调用已校验 `payload`，不跨执行持有宿主锁。
    /// Return the original result envelope; failed core records remain discoverable under this session's pool.
    /// 返回原结果信封；失败核心记录继续可通过此会话的池发现。
    pub(crate) async fn call_formal_session(
        &self,
        tool: &VulcanPluginToolDescriptor,
        id: &str,
        owner_id: &str,
        payload: Value,
        timeout_ms: u64,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        let (owner, session) = self.owned_pinned(id, owner_id, Some(&tool.plugin_id))?;
        self.start_pinned_monitor(&owner, &session)?;
        let deadline = deadline(timeout_ms)?;
        json_size(&payload, owner.settings.parent.max_value_bytes).map_err(core_error)?;
        let arguments = self.formal_envelope(payload, timeout_ms)?;
        let operation = self.submit_pinned(&owner, &session, tool, arguments, deadline, false)?;
        let mut observation = PinnedObservation {
            session: Arc::clone(&session),
            completed: false,
        };
        let snapshot = wait_operation(&operation).await?;
        let result = project_business(&snapshot);
        observation.completed = true;
        let mut state = session.lock()?;
        state.activity = Instant::now();
        if result.is_ok() && !state.acknowledged {
            owner
                .host
                .runtime()
                .forget_operation(operation.id())
                .map_err(core_error)?;
        }
        drop(state);
        self.acknowledge_clean_pinned(&owner, &session, true)?;
        result
            .map(|result| session_result(id, result, false))
            .map_err(|error| {
                VulcanPluginRuntimeError::new(format!(
                    "{error} [formal session {id}, operation {}]",
                    operation.id()
                ))
            })
    }

    /// Stop exact `id`, await bounded real retirement, and preserve the established stale-generation close projection.
    /// 停止精确 `id`，有界等待真实退役，并保留既有陈旧代次关闭投影。
    /// Return the closing envelope; physical cleanup failure remains queryable and is never reported as closed.
    /// 返回关闭信封；物理清理失败继续可查询，绝不报告为已关闭。
    pub(crate) async fn close_formal_session(
        &self,
        tool: &VulcanPluginToolDescriptor,
        id: &str,
        owner_id: &str,
        timeout_ms: u64,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        let (owner, session) = self.owned_pinned(id, owner_id, Some(&tool.plugin_id))?;
        let stale = session.plugin.generation != tool.plugin_generation;
        session.closing.store(true, Ordering::Release);
        self.start_pinned_monitor(&owner, &session)?;
        self.wait_pinned_retired(&session, deadline(timeout_ms)?)
            .await?;
        let finalization = session.lock()?.finalization.clone();
        let result = match finalization {
            Some(snapshot) => project_closing(&snapshot),
            None => Err(VulcanPluginRuntimeError::new(
                "Session retired before a closing callback became eligible.",
            )),
        };
        let succeeded = result.is_ok();
        let result = if stale {
            json!({"closed_stale_generation":true,"diagnostics":result.err().map(|error| vec![error.to_string()]).unwrap_or_default()})
        } else {
            result.unwrap_or_else(
                |error| json!({"graceful_shutdown":false,"diagnostic":error.to_string()}),
            )
        };
        if succeeded {
            self.acknowledge_clean_pinned(&owner, &session, false)?;
        }
        Ok(session_result(id, result, true))
    }

    /// Check this pinned descriptor against the sole current catalog before admitting new business.
    /// 新业务入场前将此固定描述符与唯一当前目录比对。
    /// Return an explicit stale/disabled failure, without selecting replacement package bytes.
    /// 返回明确陈旧／停用失败，不选择替代包字节。
    pub(super) fn require_pinned_current(
        &self,
        session: &PinnedSession,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let snapshot = self.snapshot();
        let current = snapshot
            .plugins
            .get(&session.plugin.id)
            .ok_or_else(|| VulcanPluginRuntimeError::new("Pinned plugin disappeared."))?;
        if current.generation != session.plugin.generation
            || current.runtime_state != crate::VulcanPluginRuntimeState::Enabled
        {
            return Err(VulcanPluginRuntimeError::new(
                "Formal session belongs to a stale or disabled plugin generation.",
            ));
        }
        Ok(())
    }

    /// Resolve only the exact `id` and declared controller, optionally requiring the tool's plugin identity.
    /// 仅解析精确 `id` 及声明控制器，并可要求工具的插件身份。
    /// Return strong original owners without retaining the registry lock during execution.
    /// 返回原强所有者，不在执行期间保留注册表锁。
    fn owned_pinned(
        &self,
        id: &str,
        owner_id: &str,
        plugin_id: Option<&str>,
    ) -> Result<(Arc<ManagedFormalRuntime>, Arc<PinnedSession>), VulcanPluginRuntimeError> {
        let owner = self.formal_runtime_owner()?.ok_or_else(|| {
            VulcanPluginRuntimeError::new("Formal plugin runtime is not attached.")
        })?;
        let session = owner
            .pinned
            .lock()
            .map_err(|_| registry_error())?
            .get(id)
            .filter(|session| {
                session.owner == owner_id
                    && plugin_id.is_none_or(|plugin| session.plugin.id == plugin)
            })
            .cloned()
            .ok_or_else(|| VulcanPluginRuntimeError::new("Owned formal session was not found."))?;
        Ok((owner, session))
    }

    /// Request cancellation for the exact host `id` owned by `owner_id`; returns after recording intent.
    /// 请求取消由 `owner_id` 所有的精确宿主 `id`；记录意图后返回。
    /// Physical cleanup stays with its monitor; interrupted monitors require an explicit drain to resume.
    /// 物理清理由原监督者负责；监督中断时需要显式排空以恢复。
    pub fn cancel_formal_session(
        &self,
        id: &str,
        owner_id: &str,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let (_, session) = self.owned_pinned(id, owner_id, None)?;
        session.closing.store(true, Ordering::Release);
        Ok(())
    }

    /// Query all retained identities belonging to `owner_id`, without creating a parent or changing supervision.
    /// 查询属于 `owner_id` 的全部保留身份，不创建父级或改变监督。
    pub fn formal_sessions(
        &self,
        owner_id: &str,
    ) -> Result<Vec<VulcanPluginFormalSessionStatus>, VulcanPluginRuntimeError> {
        let Some(owner) = self.formal_runtime_owner()? else {
            return Ok(Vec::new());
        };
        let sessions: Vec<_> = owner
            .pinned
            .lock()
            .map_err(|_| registry_error())?
            .values()
            .filter(|session| session.owner == owner_id)
            .cloned()
            .collect();
        sessions
            .iter()
            .map(|session| pinned_status(&owner, session))
            .collect()
    }

    /// Read one owner-filtered session; physical completion and closing business results remain separate.
    /// 读取单个按所有者过滤的会话；物理完成与关闭业务结果保持区分。
    pub fn formal_session_status(
        &self,
        id: &str,
        owner_id: &str,
    ) -> Result<VulcanPluginFormalSessionStatus, VulcanPluginRuntimeError> {
        let (owner, session) = self.owned_pinned(id, owner_id, None)?;
        pinned_status(&owner, &session)
    }

    /// Discover retained core operations under one exact owned pool, including failed calls and independent shutdown.
    /// 发现单个精确所属池下的保留核心操作，包含失败调用及独立关闭。
    pub fn formal_session_operations(
        &self,
        id: &str,
        owner_id: &str,
    ) -> Result<Vec<OperationSnapshot>, VulcanPluginRuntimeError> {
        let (owner, session) = self.owned_pinned(id, owner_id, None)?;
        let pool = session.lock()?.pool.clone();
        let Some(pool) = pool else {
            return Ok(Vec::new());
        };
        let page = owner
            .host
            .runtime()
            .list_operations(Some(&pool), None, owner.settings.parent.max_operations)
            .map_err(core_error)?;
        page.operation_ids
            .iter()
            .map(|id| {
                owner
                    .host
                    .runtime()
                    .operation(id)
                    .map_err(core_error)?
                    .snapshot()
                    .map_err(core_error)
            })
            .collect()
    }

    /// Acknowledge retired `id` and its retained operations; failures keep the original host identity for retry.
    /// 确认已退役的 `id` 及保留操作；失败使原宿主身份保留供重试。
    pub fn forget_formal_session(
        &self,
        id: &str,
        owner_id: &str,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let (owner, session) = self.owned_pinned(id, owner_id, None)?;
        let mut pinned = owner.pinned.lock().map_err(|_| registry_error())?;
        let mut state = session.lock()?;
        if !state.retired || state.preparing {
            return Err(VulcanPluginRuntimeError::new(
                "Formal session cleanup has not completed.",
            ));
        }
        if let Some(pool) = &state.pool {
            let page = owner
                .host
                .runtime()
                .list_operations(Some(pool), None, owner.settings.parent.max_operations)
                .map_err(core_error)?;
            for operation in page.operation_ids {
                owner
                    .host
                    .runtime()
                    .forget_operation(&operation)
                    .map_err(core_error)?;
            }
        }
        state.acknowledged = true;
        pinned.remove(id);
        Ok(())
    }

    /// Acknowledge only successful closing evidence with no earlier unacknowledged operations.
    /// 仅确认成功关闭证据，且不存在较早未确认操作。
    /// `idle_only` restricts automatic cleanup; exact original owners make concurrent acknowledgements idempotent.
    /// `idle_only` 限制自动清理；精确原所有者使并发确认保持幂等。
    pub(super) fn acknowledge_clean_pinned(
        &self,
        owner: &ManagedFormalRuntime,
        session: &PinnedSession,
        idle_only: bool,
    ) -> Result<bool, VulcanPluginRuntimeError> {
        let mut pinned = owner.pinned.lock().map_err(|_| registry_error())?;
        let mut state = session.lock()?;
        if state.acknowledged {
            return Ok(true);
        }
        if !state.retired || (idle_only && !state.idle_closed) {
            return Ok(false);
        }
        let Some(finalization) = &state.finalization else {
            return Ok(false);
        };
        if project_closing(finalization).is_err() {
            return Ok(false);
        }
        let pool = state.pool.as_ref().ok_or_else(|| {
            VulcanPluginRuntimeError::new("Finalized session has no original pool identity.")
        })?;
        let page = owner
            .host
            .runtime()
            .list_operations(Some(pool), None, owner.settings.parent.max_operations)
            .map_err(core_error)?;
        if page.operation_ids.len() != 1
            || page.operation_ids.first() != Some(&finalization.operation_id)
        {
            return Ok(false);
        }
        owner
            .host
            .runtime()
            .forget_operation(&finalization.operation_id)
            .map_err(core_error)?;
        state.acknowledged = true;
        pinned.remove(&session.id);
        Ok(true)
    }
}
