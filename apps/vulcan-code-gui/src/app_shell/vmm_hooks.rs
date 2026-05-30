//! Built-in VulcanMemoryMesh hook registration for conversation memory integration.
//! 面向会话记忆集成的内置 VulcanMemoryMesh hook 注册逻辑。

use super::conversation_hooks::{
    HOOK_CONVERSATION_COMPACTED, HOOK_CONVERSATION_TURN_COMPLETED, HOOK_CONVERSATION_TURN_PRECHECK,
    HOOK_METADATA_VMM_WRITEBACK_EFFECTS, VmmWritebackHookEffect,
};
use super::*;
use crate::model::VmmWritebackOutboxEntry;
use crate::provider_config;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use vulcan_hooks::{
    FnHookHandler, HookCapability, HookError, HookFailurePolicy, HookInvocation, HookMode,
    HookRegistration, HookResult, HookScope,
};
use vulcan_vmm::{
    TonicVmmClient, VmmClient, VmmConfigDiagnostic, VmmConfigLayer, VmmEffectiveConfig,
    VmmTransportConfig, chat_compact_request_from_payload, injection_from_precheck_response,
    post_action_request_from_completed_payload, precheck_request_from_payload,
};

/// Stable id used by the built-in VMM precheck hook.
/// 内置 VMM 预检查 hook 使用的稳定标识。
pub(super) const VMM_PRECHECK_HOOK_ID: &str = "builtin.vmm.precheck";
/// Stable id used by the built-in VMM post-action hook.
/// 内置 VMM 回合完成回写 hook 使用的稳定标识。
const VMM_POST_ACTION_HOOK_ID: &str = "builtin.vmm.post_action";
/// Stable id used by the built-in VMM chat-compaction hook.
/// 内置 VMM 会话压缩 hook 使用的稳定标识。
const VMM_CHAT_COMPACT_HOOK_ID: &str = "builtin.vmm.chat_compact";
/// Host-side ceiling for built-in VMM hook calls; request timeout remains user-configurable.
/// 内置 VMM hook 调用的宿主侧兜底上限；请求超时仍由用户配置控制。
const VMM_HOOK_TIMEOUT_CEILING_MS: u64 = 120_000;

/// Shared VMM configuration snapshot captured by hook closures.
/// 被 hook 闭包捕获的共享 VMM 配置快照。
#[derive(Debug, Clone, Default)]
pub(super) struct VmmHookConfigSnapshot {
    /// Global VMM configuration layer.
    /// 全局 VMM 配置层。
    pub global: VmmConfigLayer,
    /// Project-level VMM configuration layers keyed by workspace path.
    /// 按工作区路径索引的项目级 VMM 配置层。
    pub projects_by_workspace: BTreeMap<String, VmmConfigLayer>,
    /// Session-level VMM configuration layers keyed by session id string.
    /// 按会话标识字符串索引的会话级 VMM 配置层。
    pub sessions: BTreeMap<String, VmmConfigLayer>,
    /// Pending writeback outboxes keyed by session id string.
    /// 按会话标识字符串索引的待处理写回 outbox。
    pub outboxes: BTreeMap<String, Vec<VmmWritebackOutboxEntry>>,
}

/// Shared runtime handle used by built-in VMM hook handlers.
/// 内置 VMM hook 处理器使用的共享运行时句柄。
#[derive(Clone)]
pub(super) struct VmmHookRuntimeHandle {
    /// Latest configuration snapshot visible to hook handlers.
    /// hook 处理器可见的最新配置快照。
    state: Arc<RwLock<VmmHookConfigSnapshot>>,
}

impl VmmHookRuntimeHandle {
    /// Builds a shared VMM hook runtime handle.
    /// 构建共享 VMM hook 运行时句柄。
    pub(super) fn new(snapshot: VmmHookConfigSnapshot) -> Self {
        Self {
            state: Arc::new(RwLock::new(snapshot)),
        }
    }

    /// Replaces the full configuration snapshot.
    /// 替换完整配置快照。
    pub(super) fn replace_snapshot(&self, snapshot: VmmHookConfigSnapshot) {
        if let Ok(mut state) = self.state.write() {
            *state = snapshot;
        }
    }

    /// Resolves the effective VMM configuration for one hook invocation payload.
    /// 为一次 hook 调用载荷解析最终 VMM 配置。
    fn resolve_effective_config(&self, invocation: &HookInvocation) -> VmmEffectiveConfig {
        let snapshot = self
            .state
            .read()
            .map(|state| state.clone())
            .unwrap_or_default();
        let workspace_path = payload_string(&invocation.payload.value, "workspace_path");
        let project = workspace_path
            .as_deref()
            .and_then(|path| snapshot.projects_by_workspace.get(path));
        let session_id = invocation
            .context
            .session_id
            .clone()
            .or_else(|| payload_string(&invocation.payload.value, "session_id"));
        let session = session_id
            .as_deref()
            .and_then(|session_id| snapshot.sessions.get(session_id));
        VmmEffectiveConfig::resolve(&snapshot.global, project, session)
    }

    /// Loads the pending writeback outbox for one hook invocation.
    /// 为一次 hook 调用读取待处理写回 outbox。
    fn outbox_for_invocation(&self, invocation: &HookInvocation) -> Vec<VmmWritebackOutboxEntry> {
        let snapshot = self
            .state
            .read()
            .map(|state| state.clone())
            .unwrap_or_default();
        let session_id = invocation
            .context
            .session_id
            .clone()
            .or_else(|| payload_string(&invocation.payload.value, "session_id"));
        session_id
            .as_deref()
            .and_then(|session_id| snapshot.outboxes.get(session_id))
            .cloned()
            .unwrap_or_default()
    }
}

impl VulcanWorkbench {
    /// Builds the VMM hook configuration snapshot from global, project, and session state.
    /// 从全局、项目与会话状态构建 VMM hook 配置快照。
    pub(super) fn build_vmm_hook_config_snapshot(&self) -> VmmHookConfigSnapshot {
        let mut projects_by_workspace = BTreeMap::new();
        for project in self
            .task_projects
            .iter()
            .chain(self.archived_task_projects.iter())
        {
            let workspace_path = normalize_workspace_path_text(&project.path);
            let project_config =
                provider_config::load_project_configuration_file(&PathBuf::from(&workspace_path))
                    .unwrap_or_default();
            if !project_config.vmm.is_empty() {
                projects_by_workspace.insert(workspace_path, project_config.vmm);
            }
        }
        let sessions = self
            .session_runtime_state
            .iter()
            .chain(self.archived_session_runtime_state.iter())
            .filter_map(|(session_id, runtime_state)| {
                (!runtime_state.vmm.is_empty())
                    .then(|| (session_id.to_string(), runtime_state.vmm.clone()))
            })
            .collect::<BTreeMap<_, _>>();
        let outboxes = self
            .session_runtime_state
            .iter()
            .chain(self.archived_session_runtime_state.iter())
            .filter_map(|(session_id, runtime_state)| {
                (!runtime_state.vmm_writeback_outbox.is_empty()).then(|| {
                    (
                        session_id.to_string(),
                        runtime_state.vmm_writeback_outbox.clone(),
                    )
                })
            })
            .collect::<BTreeMap<_, _>>();
        VmmHookConfigSnapshot {
            global: self.provider_configuration.vmm.clone(),
            projects_by_workspace,
            sessions,
            outboxes,
        }
    }

    /// Synchronizes the shared VMM hook snapshot from the current workbench state.
    /// 根据当前工作台状态同步共享 VMM hook 快照。
    pub(super) fn refresh_vmm_hook_config_snapshot(&self) {
        self.vmm_hook_runtime
            .replace_snapshot(self.build_vmm_hook_config_snapshot());
    }

    /// Registers the built-in VMM hook handlers on the shared conversation hook dispatcher.
    /// 在共享会话 hook 调度器上注册内置 VMM hook 处理器。
    pub(super) fn register_builtin_vmm_hooks(&mut self) {
        let runtime = self.vmm_hook_runtime.clone();
        let _ = self.register_conversation_hook(
            vmm_hook_registration(
                VMM_PRECHECK_HOOK_ID,
                HOOK_CONVERSATION_TURN_PRECHECK,
                HookMode::Transform,
            ),
            Arc::new(FnHookHandler::new(move |invocation: HookInvocation| {
                let runtime = runtime.clone();
                async move { handle_vmm_precheck(runtime, invocation).await }
            })),
        );

        let runtime = self.vmm_hook_runtime.clone();
        let _ = self.register_conversation_hook(
            vmm_hook_registration(
                VMM_POST_ACTION_HOOK_ID,
                HOOK_CONVERSATION_TURN_COMPLETED,
                HookMode::Effect,
            ),
            Arc::new(FnHookHandler::new(move |invocation: HookInvocation| {
                let runtime = runtime.clone();
                async move { handle_vmm_post_action(runtime, invocation).await }
            })),
        );

        let runtime = self.vmm_hook_runtime.clone();
        let _ = self.register_conversation_hook(
            vmm_hook_registration(
                VMM_CHAT_COMPACT_HOOK_ID,
                HOOK_CONVERSATION_COMPACTED,
                HookMode::Effect,
            ),
            Arc::new(FnHookHandler::new(move |invocation: HookInvocation| {
                let runtime = runtime.clone();
                async move { handle_vmm_chat_compact(runtime, invocation).await }
            })),
        );
    }
}

/// Builds one built-in VMM hook registration with non-blocking failure behavior.
/// 构建一条具备非阻塞失败策略的内置 VMM hook 注册项。
fn vmm_hook_registration(id: &str, point: &str, mode: HookMode) -> HookRegistration {
    let mut registration = HookRegistration::new(id, point, mode);
    registration.scope = HookScope::Session;
    registration.priority = -100;
    registration.timeout_ms = Some(VMM_HOOK_TIMEOUT_CEILING_MS);
    registration.failure_policy = HookFailurePolicy::Continue;
    registration
        .requested_capabilities
        .insert(HookCapability::ConfigRead);
    registration
        .requested_capabilities
        .insert(HookCapability::Network);
    if mode == HookMode::Transform {
        registration
            .requested_capabilities
            .insert(HookCapability::InjectionWrite);
    }
    registration.display_name = Some(String::from("VulcanMemoryMesh"));
    if id == VMM_PRECHECK_HOOK_ID {
        registration.progress_label = Some(String::from("正在进行记忆召回"));
    }
    registration
}

/// Returns whether one effective VMM configuration should run automatic precheck now.
/// 返回一个最终 VMM 配置当前是否应运行自动 precheck。
fn should_run_vmm_precheck(config: &VmmEffectiveConfig) -> bool {
    config.precheck_enabled
}

/// Runs the VMM precheck request and returns context injections.
/// 执行 VMM 预检查请求并返回上下文注入。
async fn handle_vmm_precheck(
    runtime: VmmHookRuntimeHandle,
    invocation: HookInvocation,
) -> Result<HookResult, HookError> {
    let config = runtime.resolve_effective_config(&invocation);
    if !should_run_vmm_precheck(&config) {
        return Ok(HookResult::default());
    }
    ensure_vmm_ready(&config)?;
    let request = precheck_request_from_payload(&invocation.payload.value, &config)
        .map_err(|error| HookError::handler(error.to_string()))?;
    let client = connect_vmm_client(&config).await?;
    let response = client
        .pre_check(request)
        .await
        .map_err(|error| HookError::handler(error.to_string()))?;
    let injection = injection_from_precheck_response(&response, &config);
    Ok(HookResult {
        injections: injection.into_iter().collect(),
        ..HookResult::default()
    })
}

/// Runs the VMM post-action writeback for a completed logical turn.
/// 为已完成逻辑回合执行 VMM 回写。
async fn handle_vmm_post_action(
    runtime: VmmHookRuntimeHandle,
    invocation: HookInvocation,
) -> Result<HookResult, HookError> {
    let config = runtime.resolve_effective_config(&invocation);
    ensure_vmm_ready(&config)?;
    let request = post_action_request_from_completed_payload(&invocation.payload.value, &config)
        .map_err(|error| HookError::handler(error.to_string()))?;
    let client = match connect_vmm_client(&config).await {
        Ok(client) => client,
        Err(error) => {
            return Ok(vmm_writeback_effect_result(vec![
                VmmWritebackHookEffect::Enqueue {
                    request,
                    error: error.to_string(),
                },
            ]));
        }
    };
    let mut effects =
        flush_vmm_writeback_outbox(&client, runtime.outbox_for_invocation(&invocation)).await;
    if effects
        .iter()
        .any(|effect| matches!(effect, VmmWritebackHookEffect::Failed { .. }))
    {
        effects.push(VmmWritebackHookEffect::Enqueue {
            request,
            error: String::from(
                "Previous VMM writeback outbox entry is still pending; queued behind it.",
            ),
        });
    } else if let Err(error) = accepted_post_action(&client, request.clone()).await {
        effects.push(VmmWritebackHookEffect::Enqueue {
            request,
            error: error.to_string(),
        });
    }
    Ok(vmm_writeback_effect_result(effects))
}

/// Runs the VMM chat-compaction notification after a session summary is accepted.
/// 在会话摘要被接受后执行 VMM 会话压缩通知。
async fn handle_vmm_chat_compact(
    runtime: VmmHookRuntimeHandle,
    invocation: HookInvocation,
) -> Result<HookResult, HookError> {
    let config = runtime.resolve_effective_config(&invocation);
    ensure_vmm_ready(&config)?;
    let request = chat_compact_request_from_payload(&invocation.payload.value, &config)
        .map_err(|error| HookError::handler(error.to_string()))?;
    let client = connect_vmm_client(&config).await?;
    client
        .chat_compact(request)
        .await
        .map_err(|error| HookError::handler(error.to_string()))?;
    Ok(HookResult::default())
}

/// Connects a one-shot VMM tonic client for one hook invocation.
/// 为一次 hook 调用连接一个一次性 VMM tonic 客户端。
async fn connect_vmm_client(config: &VmmEffectiveConfig) -> Result<TonicVmmClient, HookError> {
    let transport_config = VmmTransportConfig::from_effective(config);
    TonicVmmClient::connect(transport_config)
        .await
        .map_err(|error| HookError::handler(error.to_string()))
}

/// Ensures the effective configuration is ready for outbound VMM calls.
/// 确保最终配置已准备好执行 VMM 出站调用。
fn ensure_vmm_ready(config: &VmmEffectiveConfig) -> Result<(), HookError> {
    match config.diagnose_memory_flow(true) {
        VmmConfigDiagnostic::Ready => Ok(()),
        diagnostic => Err(HookError::handler(format!(
            "VMM memory flow is not ready: {diagnostic:?}"
        ))),
    }
}

/// Flushes queued VMM writebacks in FIFO order until the first failure.
/// 按 FIFO 顺序冲刷排队的 VMM 写回，直到遇到第一个失败。
async fn flush_vmm_writeback_outbox(
    client: &TonicVmmClient,
    outbox: Vec<VmmWritebackOutboxEntry>,
) -> Vec<VmmWritebackHookEffect> {
    let mut flushed = Vec::new();
    let mut effects = Vec::new();
    for entry in outbox {
        match accepted_post_action(client, entry.request.clone()).await {
            Ok(()) => flushed.push(entry.id),
            Err(error) => {
                if !flushed.is_empty() {
                    effects.push(VmmWritebackHookEffect::Flushed {
                        entry_ids: std::mem::take(&mut flushed),
                    });
                }
                effects.push(VmmWritebackHookEffect::Failed {
                    entry_id: entry.id,
                    error: error.to_string(),
                });
                return effects;
            }
        }
    }
    if !flushed.is_empty() {
        effects.push(VmmWritebackHookEffect::Flushed { entry_ids: flushed });
    }
    effects
}

/// Calls VMM PostAction and requires an accepted response.
/// 调用 VMM PostAction 并要求返回 accepted。
async fn accepted_post_action(
    client: &TonicVmmClient,
    request: vulcan_vmm::PostActionRequest,
) -> Result<(), HookError> {
    let response = client
        .post_action(request)
        .await
        .map_err(|error| HookError::handler(error.to_string()))?;
    if response.accepted {
        Ok(())
    } else {
        Err(HookError::handler(format!(
            "VMM PostAction returned accepted=false trace_id={}",
            response.trace_id
        )))
    }
}

/// Builds one hook result carrying VMM writeback side effects.
/// 构建一条携带 VMM 写回副作用的 hook 结果。
fn vmm_writeback_effect_result(effects: Vec<VmmWritebackHookEffect>) -> HookResult {
    if effects.is_empty() {
        return HookResult::default();
    }
    let mut metadata = BTreeMap::new();
    metadata.insert(
        HOOK_METADATA_VMM_WRITEBACK_EFFECTS.to_string(),
        json!(effects),
    );
    HookResult {
        metadata,
        ..HookResult::default()
    }
}

/// Reads one non-empty string field from a JSON payload.
/// 从 JSON 载荷读取一个非空字符串字段。
fn payload_string(payload: &Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SessionId;

    /// Verifies configuration resolution follows global, project, then session ordering.
    /// 验证配置解析遵循全局、项目、会话的覆盖顺序。
    #[test]
    fn vmm_hook_config_snapshot_resolves_session_over_project_over_global() {
        let (mut workbench, _task) = VulcanWorkbench::new();
        let session_id = SessionId::from_u128(9001);
        workbench.provider_configuration.vmm.grpc_target = Some(String::from("global:1"));
        workbench.task_projects.push(TaskProjectSummary::new(
            "project-vmm",
            "Project VMM",
            r"D:\workspace\vmm",
            false,
            0,
            vec![session_id],
        ));
        workbench.sessions.push(SessionSummary::new(
            session_id,
            Some(String::from("project-vmm")),
            "VMM Session",
            "",
            SessionState::Draft,
            0,
            0,
            "",
        ));
        workbench.session_runtime_state.insert(
            session_id,
            SessionRuntimeStateEntry {
                vmm: VmmConfigLayer {
                    grpc_target: Some(String::from("session:3")),
                    user_id: Some(1),
                    project_id: Some(2),
                    ..VmmConfigLayer::default()
                },
                ..SessionRuntimeStateEntry::default()
            },
        );

        let snapshot = workbench.build_vmm_hook_config_snapshot();
        let runtime = VmmHookRuntimeHandle::new(snapshot);
        let invocation = HookInvocation {
            registration: vmm_hook_registration(
                VMM_PRECHECK_HOOK_ID,
                HOOK_CONVERSATION_TURN_PRECHECK,
                HookMode::Transform,
            ),
            context: vulcan_hooks::HookContext {
                session_id: Some(session_id.to_string()),
                ..vulcan_hooks::HookContext::new("test", HOOK_CONVERSATION_TURN_PRECHECK)
            },
            payload: vulcan_hooks::HookPayload {
                value: serde_json::json!({
                    "workspace_path": r"D:\workspace\vmm",
                    "session_id": session_id.to_string(),
                }),
            },
        };

        let config = runtime.resolve_effective_config(&invocation);
        assert_eq!(config.grpc_target, "session:3");
        assert_eq!(config.user_id, Some(1));
        assert_eq!(config.project_id, Some(2));
    }

    /// Verifies the precheck switch disables only automatic recall.
    /// 验证 precheck 开关只关闭自动前置召回。
    #[test]
    fn vmm_precheck_switch_can_disable_only_automatic_recall() {
        let config = VmmEffectiveConfig::resolve(
            &VmmConfigLayer {
                grpc_target: Some(String::from("127.0.0.1:50051")),
                user_id: Some(1),
                project_id: Some(2),
                precheck_enabled: Some(false),
                ..VmmConfigLayer::default()
            },
            None,
            None,
        );

        assert!(!should_run_vmm_precheck(&config));
        assert_eq!(
            config.diagnose_memory_flow(true),
            VmmConfigDiagnostic::Ready
        );
        assert_eq!(
            config.diagnose_precheck_flow(true),
            VmmConfigDiagnostic::DisabledByConfig
        );
    }
}
