//! Service-kernel side of the tool permission gate.
//! 工具权限闸门的服务内核侧实现。

use std::sync::Arc;

use vulcan_agent_runtime::agent_runtime::tool_permission::ToolPermissionHost;
use vulcan_agent_runtime::model::SessionId;
use vulcan_session_core::v1::config::PermissionMode;

use crate::session_runtime_registry_v1::SessionRuntimeRegistry;

/// Reads each session's live permission mode from its registered V1 document.
/// 从已注册的 V1 文档读取每个会话的实时权限模式。
pub(crate) struct KernelToolPermissionHost {
    /// Process session registry; every session that runs tools is registered here.
    /// 进程会话注册表；所有运行工具的会话都在此注册。
    registry: Arc<SessionRuntimeRegistry>,
}

impl KernelToolPermissionHost {
    /// Builds the host over the process session registry.
    /// 基于进程会话注册表构建宿主。
    pub(crate) fn new(registry: Arc<SessionRuntimeRegistry>) -> Self {
        Self { registry }
    }
}

impl ToolPermissionHost for KernelToolPermissionHost {
    /// Child agent sessions follow their root user session, so a child can never run with more
    /// permission than the session the user controls; a missing root fails closed.
    /// 子代理会话跟随其根用户会话，因此子会话的权限绝不会超过用户控制的会话；根会话缺失时失败即关闭。
    fn permission_mode(&self, session_id: SessionId) -> Result<PermissionMode, String> {
        let (own_mode, root) = self.read(session_id, |document| {
            (
                document.metadata.mode.access.permission,
                document
                    .metadata
                    .child_execution_profile
                    .as_ref()
                    .map(|profile| profile.root_session_id.clone()),
            )
        })?;
        let Some(root) = root else {
            return Ok(own_mode);
        };
        let root_id = SessionId::parse_str(&root)
            .map_err(|error| format!("invalid root session `{root}`: {error}"))?;
        self.read(root_id, |document| document.metadata.mode.access.permission)
    }
}

impl KernelToolPermissionHost {
    /// Reads one value from a registered session document.
    /// 从已注册会话文档读取一个值。
    fn read<T>(
        &self,
        session_id: SessionId,
        reader: impl FnOnce(&vulcan_session_core::v1::document::SessionDocument) -> T,
    ) -> Result<T, String> {
        crate::session_goal_lifecycle::registered_session_v1_handle(&self.registry, session_id)?
            .read(reader)
            .map_err(|error| error.to_string())
    }
}
