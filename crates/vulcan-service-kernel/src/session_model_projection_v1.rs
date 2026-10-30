//! Metadata-only restoration of session-local model controls.
//! 会话自有模型控件的纯元数据恢复。

use std::collections::BTreeMap;

use vulcan_session_core::v1::config::{ActiveRequestConfig, PendingRequestConfig};
use vulcan_workbench_model::{SessionPendingModelSelection, SessionRuntimeStateEntry};
use vulcan_workspace_store::workspace_store::{
    SessionV1StoragePreflightOutcome, WorkspaceSnapshot, preflight_session_v1_storage,
};

use crate::session_runtime_registry_v1::{
    SessionLifecycleTransitionGuard, SessionRuntimeError, SessionRuntimeRegistry,
};
use crate::{HostRuntimeHandle, SessionId};

/// Read-only session request facts retained through navigation or snapshot publication.
/// 贯穿导航或快照发布过程保留的只读会话请求事实。
pub(crate) struct SessionModelProjectionV1 {
    /// Exact session identity; never inferred from the currently selected UI tab.
    /// 精确会话身份，绝不从当前选中的界面标签推断。
    session_id: SessionId,
    /// Active request captured from the resident owner or indexed metadata.
    /// 从常驻 owner 或索引 metadata 捕获的活动请求。
    active: ActiveRequestConfig,
    /// Latest user choice waiting for the next API boundary.
    /// 等待下个 API 边界生效的最新用户选择。
    pending: Option<PendingRequestConfig>,
    /// Session-owned compaction node captured from the same metadata revision as the route.
    /// 与路由从同一元数据修订捕获的会话自有压缩节点。
    context_compaction_window_node_id: Option<vulcan_context::ContextCompactionWindowNodeId>,
    /// Keeps archive/delete from invalidating the selected owner before publication.
    /// 防止归档或删除在发布前使选中的 owner 失效。
    _lifecycle: SessionLifecycleTransitionGuard,
}

/// Isolates an automatic session restore from workspace availability.
/// 将会话自动恢复与工作台可用性隔离。
pub(crate) struct AutomaticSessionModelProjectionV1 {
    /// Exact selection examined before asynchronous publication.
    /// 异步发布前检查的精确选中身份。
    session_id: SessionId,
    /// Successfully captured metadata and its publication lifecycle guard.
    /// 成功捕获的元数据及其发布生命周期守卫。
    projection: Option<SessionModelProjectionV1>,
    /// True only after exact-path preflight confirmed absence and removed the index.
    /// 仅在精确路径预检确认缺失且已删除索引后为真。
    missing_storage: bool,
}

/// Reads optional startup/refresh metadata for `session_id` using the indexed `controller` and `registry`.
/// 使用索引 `controller` 与 `registry` 读取 `session_id` 的可选启动或刷新元数据。
/// Returns an infallible publication result; read failures are ignored and confirmed missing indexes removed.
/// 返回不会失败的发布结果；读取失败忽略，确认缺失的索引删除。
pub(crate) async fn read_automatic_session_model_projection_v1(
    controller: &HostRuntimeHandle,
    registry: &SessionRuntimeRegistry,
    session_id: SessionId,
) -> AutomaticSessionModelProjectionV1 {
    // Keep nil selection free of storage access and never classify a read error as file absence.
    // 空选择不访问存储，且绝不把读取错误视为文件缺失。
    let mut result = AutomaticSessionModelProjectionV1 {
        session_id,
        projection: None,
        missing_storage: false,
    };
    if session_id != SessionId::nil() {
        match preflight_session_v1_storage(controller, &session_id.to_string()).await {
            Ok(SessionV1StoragePreflightOutcome::Ready { .. }) => {
                result.projection =
                    read_session_model_projection_v1(controller, registry, session_id)
                        .await
                        .ok();
            }
            Ok(SessionV1StoragePreflightOutcome::MissingStorageIndexRemoved { .. }) => {
                result.missing_storage = true;
            }
            Err(_) => {}
        }
    }
    result
}

impl AutomaticSessionModelProjectionV1 {
    /// Applies captured metadata to `workspace` using the latest resident `registry` state.
    /// 使用 `registry` 最新常驻状态将已捕获元数据应用到 `workspace`。
    /// Returns no error; unavailable selections are cleared without overriding concurrent navigation.
    /// 不返回错误；清除不可用选择，但不覆盖并发导航。
    pub(crate) fn apply(
        &self,
        registry: &SessionRuntimeRegistry,
        workspace: &mut WorkspaceSnapshot,
    ) {
        if self.missing_storage {
            // Remove every in-memory reference to the deleted index, keeping files and other sessions intact.
            // 移除已删除索引的所有内存引用，保留文件及其他会话。
            workspace
                .sessions
                .retain(|session| session.id != self.session_id);
            workspace
                .archived_sessions
                .retain(|session| session.id != self.session_id);
            for project in workspace
                .task_projects
                .iter_mut()
                .chain(&mut workspace.archived_task_projects)
            {
                project.session_ids.retain(|id| *id != self.session_id);
            }
            workspace.session_runtime_state.remove(&self.session_id);
            workspace
                .archived_session_runtime_state
                .remove(&self.session_id);
            workspace.session_run_phases.remove(&self.session_id);
            workspace.hydrated_session_ids.remove(&self.session_id);
        }
        if workspace.selected_session != self.session_id {
            return;
        }
        if self.projection.as_ref().is_none_or(|projection| {
            projection
                .apply(registry, &mut workspace.session_runtime_state)
                .is_err()
        }) {
            workspace.selected_session = SessionId::nil();
        }
    }
}

/// Reads one indexed session's model settings without constructing its interaction runtime.
/// 读取一个索引会话的模型设置，不构建其交互运行时。
/// `controller` resolves the exact locator; `registry` serializes access; `session_id` owns the settings.
/// `controller` 解析精确定位器；`registry` 串行化访问；`session_id` 是设置所有者。
/// Returns a read-only projection guard, or an explicit configuration error; never falls back to a global model.
/// 返回只读投影守卫或显式配置错误，绝不回退全局模型。
pub(crate) async fn read_session_model_projection_v1(
    controller: &HostRuntimeHandle,
    registry: &SessionRuntimeRegistry,
    session_id: SessionId,
) -> Result<SessionModelProjectionV1, String> {
    // Lifecycle admission precedes indexed lookup, matching other metadata-only configuration access.
    // 生命周期准入先于索引查询，与其他纯元数据配置访问保持一致。
    let lifecycle = registry
        .acquire_lifecycle_transition(session_id)
        .await
        .map_err(|error| error.to_string())?;
    // The indexed locator is the sole source of the metadata path.
    // 索引定位器是 metadata 路径的唯一来源。
    let locator = vulcan_workspace_store::workspace_store::locate_session_v1_storage(
        controller,
        &session_id.to_string(),
    )
    .await
    .map_err(|error| format!("Failed to locate the session model configuration: {error}"))?;
    // A resident writer may hold a newer pending choice than the latest disk publication.
    // 常驻写入器可能持有比最新磁盘发布更新的待生效选择。
    let (active, pending, context_compaction_window_node_id) = registry.with_configuration_access(
        session_id,
        |handle| {
            handle
                .read(|document| {
                    (
                        document.metadata.config.active_request.clone(),
                        document.metadata.config.pending_request.clone(),
                        document.metadata.config.context_compaction_window_node_id,
                    )
                })
                .map_err(|error| format!("Failed to read the session model configuration: {error}"))
        },
        || {
            locator
                .read_request_configuration()
                .map_err(|error| format!("Failed to read the session model configuration: {error}"))
        },
    )?;
    Ok(SessionModelProjectionV1 {
        session_id,
        active,
        pending,
        context_compaction_window_node_id,
        _lifecycle: lifecycle,
    })
}

impl SessionModelProjectionV1 {
    /// Projects request facts into `states`, preferring the latest resident owner at publication time.
    /// 将请求事实投影到 `states`，发布时优先采用常驻 owner 的最新状态。
    /// `registry` is read without loading or waiting for a new runtime; returns no persisted mutations.
    /// 读取 `registry` 时不加载或等待新运行时；不产生任何持久化修改。
    pub(crate) fn apply(
        &self,
        registry: &SessionRuntimeRegistry,
        states: &mut BTreeMap<SessionId, SessionRuntimeStateEntry>,
    ) -> Result<(), String> {
        // Recheck a resident owner to avoid publishing a snapshot older than a completed model selection.
        // 重新检查常驻 owner，避免发布比已完成模型选择更旧的快照。
        let (active, pending, context_compaction_window_node_id) =
            match registry.get(self.session_id) {
                Ok(handle) => handle
                    .read(|document| {
                        (
                            document.metadata.config.active_request.clone(),
                            document.metadata.config.pending_request.clone(),
                            document.metadata.config.context_compaction_window_node_id,
                        )
                    })
                    .map_err(|error| {
                        format!("Failed to read the session model configuration: {error}")
                    })?,
                Err(SessionRuntimeError::SessionNotRegistered(_)) => (
                    self.active.clone(),
                    self.pending.clone(),
                    self.context_compaction_window_node_id,
                ),
                Err(error) => return Err(error.to_string()),
            };
        // One exact session row receives all controls from the same authoritative revision.
        // 一个精确会话行接收来自同一权威修订的全部控制项。
        let state = states.entry(self.session_id).or_default();
        apply_request_model_projection_v1(state, &active, pending.as_ref());
        // Metadata-only navigation must restore the planning control before the first submission.
        // 纯元数据导航必须在首次提交前恢复规划控制项。
        state.context_compaction_window_node_id = context_compaction_window_node_id;
        Ok(())
    }
}

/// Applies only request-owned control fields, keeping runtime context, goals and tool configuration intact.
/// 仅应用请求自有控件字段，保持运行时上下文、目标与工具配置不变。
/// `state` is the presentation adapter; `active` and `pending` are canonical V1 configuration, not new user intent.
/// `state` 是展示适配器；`active` 和 `pending` 是规范 V1 配置，不代表新的用户意图。
/// Returns no value and never changes the frozen in-flight Provider route or writes metadata.
/// 无返回值，绝不修改冻结的执行中 Provider 路由或写入 metadata。
pub(crate) fn apply_request_model_projection_v1(
    state: &mut SessionRuntimeStateEntry,
    active: &ActiveRequestConfig,
    pending: Option<&PendingRequestConfig>,
) {
    state.active_provider_id.clone_from(&active.provider);
    state.active_connection_id.clone_from(&active.connection_id);
    state.active_model_id.clone_from(&active.model);
    state.pending_model_selection = pending.and_then(|selection| {
        Some(SessionPendingModelSelection {
            provider_id: selection.provider.clone()?,
            connection_id: selection.connection_id.clone()?,
            model_id: selection.model.clone()?,
        })
    });
    state.active_reasoning_effort = pending
        .and_then(|selection| selection.thinking_level.as_deref())
        .or(active.thinking_level.as_deref())
        .and_then(crate::parse_reasoning_effort);
    state.active_temperature = active
        .temperature
        .and_then(|value| value.to_string().parse::<f32>().ok())
        .filter(|value| value.is_finite());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_runtime_registry_v1::{SessionHandle, SessionPersistenceBackend};
    use std::sync::Arc;
    use vulcan_session_core::v1::document::{FlushCandidate, SessionDocument};
    use vulcan_workspace_store::workspace_store::{SessionV1FlushResult, SessionV1StoreError};

    /// A read-only backend fails if model projection ever attempts a durable mutation.
    /// 只读后端在模型投影尝试可靠修改时失败。
    struct NoWrites;
    impl SessionPersistenceBackend for NoWrites {
        /// Rejects every `candidate`; successful model restoration must never call this method.
        /// 拒绝所有 `candidate`；成功的模型恢复绝不能调用该方法。
        fn flush(
            &self,
            _candidate: &FlushCandidate,
        ) -> Result<SessionV1FlushResult, SessionV1StoreError> {
            panic!("model projection must be read-only");
        }
    }

    /// Builds a route fixture from exact `provider`, `account`, and `model` strings.
    /// 根据精确 `provider`、`account` 和 `model` 字符串构建路线夹具。
    /// Returns initialized active request metadata without creating a Turn.
    /// 返回已初始化的活动请求 metadata，不创建回合。
    fn active(provider: &str, account: &str, model: &str) -> ActiveRequestConfig {
        ActiveRequestConfig {
            provider: Some(provider.into()),
            connection_id: Some(account.into()),
            model: Some(model.into()),
            thinking_level: Some("max".into()),
            ..Default::default()
        }
    }

    /// Cold sessions keep their distinct exact model/account choices across repeated navigation.
    /// 冷会话在反复导航中保持各自精确的模型及账号选择。
    #[tokio::test]
    async fn cold_restart_and_navigation_restore_each_sessions_model() {
        // A fresh registry represents the restarted process with no resident conversation.
        // 全新注册表代表重启后没有常驻会话的进程。
        let registry = SessionRuntimeRegistry::default();
        // Distinct session identities prevent accidental reuse of one global choice.
        // 不同会话身份防止意外复用同一个全局选择。
        let first_id = SessionId::from_u128(0x7101);
        // The second session deliberately uses a different provider and account.
        // 第二个会话故意使用不同供应商和账号。
        let second_id = SessionId::from_u128(0x7102);
        // The first projection mirrors active-only metadata after an executed request.
        // 第一份投影模拟已执行请求后仅包含活动路线的 metadata。
        let first = SessionModelProjectionV1 {
            session_id: first_id,
            active: active("kimi", "kimi-account", "k3-256k"),
            pending: None,
            context_compaction_window_node_id: Some(
                vulcan_context::ContextCompactionWindowNodeId::CompressionWindow4,
            ),
            _lifecycle: registry
                .acquire_lifecycle_transition(first_id)
                .await
                .unwrap(),
        };
        // The second projection retains a user choice that has not been executed yet.
        // 第二份投影保留尚未执行的用户选择。
        let second = SessionModelProjectionV1 {
            session_id: second_id,
            active: active("deepseek", "ds-account", "deepseek-v4"),
            context_compaction_window_node_id: Some(
                vulcan_context::ContextCompactionWindowNodeId::CompressionWindow2,
            ),
            pending: Some(PendingRequestConfig {
                provider: Some("minimax".into()),
                connection_id: Some("mm-account".into()),
                model: Some("MiniMax-M2.7".into()),
                set_at_ms: 2,
                thinking_level: None,
                temperature: None,
            }),
            _lifecycle: registry
                .acquire_lifecycle_transition(second_id)
                .await
                .unwrap(),
        };
        // Empty presentation adapters are rebuilt solely from each session's metadata.
        // 空展示适配器仅根据各会话自身 metadata 重建。
        let mut states = BTreeMap::new();
        for projection in [&first, &second, &first, &second] {
            projection.apply(&registry, &mut states).unwrap();
        }
        assert_eq!(
            states[&first_id].context_compaction_window_node_id,
            first.context_compaction_window_node_id
        );
        assert_eq!(
            states[&second_id].context_compaction_window_node_id,
            second.context_compaction_window_node_id
        );
        assert_eq!(
            states[&first_id].active_model_id.as_deref(),
            Some("k3-256k")
        );
        assert_eq!(
            states[&first_id].active_connection_id.as_deref(),
            Some("kimi-account")
        );
        assert_eq!(
            states[&second_id].active_model_id.as_deref(),
            Some("deepseek-v4")
        );
        assert_eq!(
            states[&second_id]
                .pending_model_selection
                .as_ref()
                .unwrap()
                .model_id,
            "MiniMax-M2.7"
        );
        assert!(matches!(
            registry.get(first_id),
            Err(SessionRuntimeError::SessionNotRegistered(_))
        ));
        assert!(matches!(
            registry.get(second_id),
            Err(SessionRuntimeError::SessionNotRegistered(_))
        ));
    }

    /// A model selection completed during refresh outranks the earlier captured metadata.
    /// 刷新期间完成的模型选择优先于更早捕获的 metadata。
    #[tokio::test]
    async fn publication_uses_latest_resident_request_and_preserves_unrelated_state() {
        // This registry initially has no runtime, matching a metadata-only navigation capture.
        // 注册表初始没有运行时，对应纯 metadata 导航捕获。
        let registry = SessionRuntimeRegistry::default();
        // One stable identity owns both the old capture and the later resident document.
        // 一个稳定身份拥有旧捕获及后来的常驻文档。
        let session_id = SessionId::from_u128(0x7103);
        // The captured old route must not win after a resident owner appears.
        // 常驻 owner 出现后，已捕获旧路线不能胜出。
        let projection = SessionModelProjectionV1 {
            session_id,
            active: active("old", "old-account", "old-model"),
            pending: None,
            context_compaction_window_node_id: Some(
                vulcan_context::ContextCompactionWindowNodeId::CompressionWindow2,
            ),
            _lifecycle: registry
                .acquire_lifecycle_transition(session_id)
                .await
                .unwrap(),
        };
        // The registered document supplies the current metadata without a single open Turn.
        // 已注册文档提供当前 metadata，且不存在任何开放回合。
        let mut document = SessionDocument::new(session_id.to_string(), "project", "test", 1);
        document.metadata.config.active_request = active("kimi", "kimi-account", "k3-256k");
        document.metadata.config.context_compaction_window_node_id =
            Some(vulcan_context::ContextCompactionWindowNodeId::CompressionWindow4);
        // Record the original revision to prove projection does not create a model-switch event.
        // 记录原修订号，证明投影不会创建模型切换事件。
        let revision = document.metadata.document_revision;
        // This handle rejects any persistence attempt by the projection.
        // 该句柄拒绝投影产生任何持久化尝试。
        let handle = SessionHandle::with_backend(session_id, document, Arc::new(NoWrites));
        registry.register(Arc::clone(&handle)).unwrap();
        // Non-request controls must survive applying the new model projection.
        // 非请求控件必须在应用新模型投影后保留。
        let mut states = BTreeMap::from([(
            session_id,
            SessionRuntimeStateEntry {
                active_work_mode: Some("plan".into()),
                provider_tool_settings_initialized: true,
                ..Default::default()
            },
        )]);
        projection.apply(&registry, &mut states).unwrap();
        assert_eq!(
            states[&session_id].active_model_id.as_deref(),
            Some("k3-256k")
        );
        assert_eq!(
            states[&session_id].active_work_mode.as_deref(),
            Some("plan")
        );
        assert!(states[&session_id].provider_tool_settings_initialized);
        assert_eq!(
            states[&session_id].context_compaction_window_node_id,
            Some(vulcan_context::ContextCompactionWindowNodeId::CompressionWindow4)
        );
        handle
            .read(|document| {
                assert_eq!(document.metadata.document_revision, revision);
                assert!(document.events().is_empty());
            })
            .unwrap();
    }
}
