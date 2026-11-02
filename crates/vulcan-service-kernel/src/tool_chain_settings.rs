//! System and session settings of the `tool_chain` tool.
//! `tool_chain` 工具的系统与会话设置。
//!
//! The session choice lives in `config.selection.provider_tools.tool_chain_enabled` of the root
//! session's metadata; `None` follows the system switch. The tool bridge reads both at call time,
//! so turning the switch off during a turn answers later chains with "disabled".
//! 会话选择位于根会话元数据的 `config.selection.provider_tools.tool_chain_enabled`；`None`
//! 表示跟随系统开关。工具桥在调用时读取两者，因此回合中途关闭开关后，之后的链条会得到“已禁用”。

use std::sync::{Arc, Mutex, Weak};

use serde::Serialize;
use vulcan_agent_runtime::agent_runtime::tool_runtime_bridge::{ToolChainHost, ToolChainSettings};
use vulcan_agent_runtime::model::SessionId;
use vulcan_tool_chain::DEFAULT_TOOL_CHAIN_MAX_STEPS;

use crate::ServiceBridgeState;
use crate::session_runtime_registry_v1::SessionRuntimeRegistry;

/// System `tool_chain` settings currently projected into the workbench.
/// 当前投影到工作台中的系统 `tool_chain` 设置。
fn system_settings(state: &ServiceBridgeState) -> ToolChainSettings {
    ToolChainSettings {
        enabled: state.workbench.tool_chain_enabled.unwrap_or(true),
        max_steps: state
            .workbench
            .tool_chain_max_steps
            .unwrap_or(DEFAULT_TOOL_CHAIN_MAX_STEPS),
    }
}

/// Reads live `tool_chain` settings for the tool bridge.
/// 为工具桥读取实时 `tool_chain` 设置。
pub(crate) struct KernelToolChainHost {
    /// Process session registry holding every session that runs tools.
    /// 持有所有运行工具会话的进程会话注册表。
    registry: Arc<SessionRuntimeRegistry>,
    /// Service state holding the system settings; weak, because the state owns the runtime that
    /// owns this host.
    /// 持有系统设置的服务状态；使用弱引用，因为该状态拥有持有本宿主的运行时。
    state: Weak<Mutex<ServiceBridgeState>>,
}

impl KernelToolChainHost {
    /// Builds the host over the session registry and service state.
    /// 基于会话注册表与服务状态构建宿主。
    pub(crate) fn new(
        registry: Arc<SessionRuntimeRegistry>,
        state: Weak<Mutex<ServiceBridgeState>>,
    ) -> Self {
        Self { registry, state }
    }
}

impl ToolChainHost for KernelToolChainHost {
    fn settings(&self, session_id: SessionId) -> Result<ToolChainSettings, String> {
        let state = self
            .state
            .upgrade()
            .ok_or_else(|| String::from("The service state is no longer available."))?;
        let system = system_settings(
            &*state
                .lock()
                .map_err(|error| format!("Failed to lock service state: {error}"))?,
        );
        // Agents follow the switch of the user session they belong to.
        // 代理跟随其所属用户会话的开关。
        let root = crate::tool_permission_host::root_session_of(&self.registry, session_id)?;
        let session_choice =
            crate::session_goal_lifecycle::registered_session_v1_handle(&self.registry, root)?
                .read(|document| {
                    document
                        .metadata
                        .config
                        .selection
                        .provider_tools
                        .tool_chain_enabled
                })
                .map_err(|error| error.to_string())?;
        Ok(ToolChainSettings {
            enabled: session_choice.unwrap_or(system.enabled),
            max_steps: system.max_steps,
        })
    }
}

/// Session `tool_chain` state shown by the composer.
/// 输入区展示的会话 `tool_chain` 状态。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionToolChainSnapshot {
    /// Whether the session offers `tool_chain`.
    /// 会话是否提供 `tool_chain`。
    enabled: bool,
    /// Whether the session has no choice of its own and follows the system switch.
    /// 会话是否没有自己的选择而跟随系统开关。
    follows_system: bool,
    /// Whether the session already prepared a provider request, so a change breaks context.
    /// 会话是否已准备过供应商请求，即修改会打破上下文。
    requests_started: bool,
}

/// Builds the snapshot from the session's stored choice and the system switch.
/// 根据会话已存选择与系统开关构建快照。
fn session_snapshot(
    state: &crate::SharedServiceBridgeState,
    access: &crate::session_request_config_v1::SessionProviderToolConfiguration,
) -> Result<SessionToolChainSnapshot, String> {
    let system = system_settings(
        &*state
            .lock()
            .map_err(|error| format!("Failed to lock service state: {error}"))?,
    );
    let choice = access.read()?.tool_chain_enabled;
    Ok(SessionToolChainSnapshot {
        enabled: choice.unwrap_or(system.enabled),
        follows_system: choice.is_none(),
        requests_started: access.requests_started()?,
    })
}

/// Parses a conversation id into a session id.
/// 将会话标识解析为会话 ID。
fn session_id_of(conversation_id: &str) -> Result<crate::SessionId, String> {
    conversation_id
        .parse::<crate::SessionId>()
        .map_err(|error| format!("Invalid conversation id `{conversation_id}`: {error}"))
}

/// Returns the session's effective `tool_chain` state without writing anything.
/// 返回会话生效的 `tool_chain` 状态，不写入任何内容。
pub(crate) async fn service_session_tool_chain(
    state: crate::SharedServiceBridgeState,
    conversation_id: String,
) -> Result<SessionToolChainSnapshot, String> {
    let access = crate::session_request_config_v1::SessionProviderToolConfiguration::resolve(
        &state,
        session_id_of(&conversation_id)?,
    )
    .await?;
    session_snapshot(&state, &access)
}

/// Saves the session's explicit `tool_chain` choice.
/// 保存会话对 `tool_chain` 的显式选择。
///
/// The tool list changes from the session's next provider request. A change after the first
/// request is marked so the model is told about it; the client confirms the context break first.
/// 工具清单从会话下一次供应商请求起变化。首次请求之后的修改会被标记以便告知模型；客户端会先确认
/// 打破上下文。
pub(crate) async fn service_save_session_tool_chain(
    state: crate::SharedServiceBridgeState,
    conversation_id: String,
    enabled: bool,
) -> Result<SessionToolChainSnapshot, String> {
    let access = crate::session_request_config_v1::SessionProviderToolConfiguration::resolve(
        &state,
        session_id_of(&conversation_id)?,
    )
    .await?;
    let mut selection = access.read()?;
    selection.tool_chain_enabled = Some(enabled);
    access.save(
        selection,
        crate::session_goal_lifecycle::current_v1_goal_unix_millis()?,
    )?;
    session_snapshot(&state, &access)
}
