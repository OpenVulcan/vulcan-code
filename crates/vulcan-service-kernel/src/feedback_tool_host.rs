//! Host of the AI `feedback` tool. Drafts never reach it; it only links pull requests, through the
//! same service function as the `service_link_feedback_pr` RPC, so validation and the user-center
//! call exist once.
//! AI `feedback` 工具的宿主。草稿从不到达这里；它只经与 `service_link_feedback_pr` RPC 相同的服务函数
//! 关联拉取请求，因此校验与用户中心调用只存在一处。

use std::sync::{Arc, Mutex, Weak};

use vulcan_agent_runtime::model::SessionId;
use vulcan_agent_runtime::tooling::native::NativeFeedbackHost;

use crate::ServiceBridgeState;
use crate::session_runtime_registry_v1::SessionRuntimeRegistry;

/// Links pull requests to feedback items for the `feedback` tool.
/// 为 `feedback` 工具将拉取请求关联到反馈条目。
pub(crate) struct KernelFeedbackHost {
    /// Process session registry; tells root sessions from spawned agents.
    /// 进程会话注册表；区分根会话与派生代理。
    registry: Arc<SessionRuntimeRegistry>,
    /// Service state; weak, because the state owns the runtime that owns this host.
    /// 服务状态；使用弱引用，因为该状态拥有持有本宿主的运行时。
    state: Weak<Mutex<ServiceBridgeState>>,
}

impl std::fmt::Debug for KernelFeedbackHost {
    /// Renders a label without exposing service state.
    /// 渲染不暴露服务状态的标签。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("KernelFeedbackHost")
            .finish_non_exhaustive()
    }
}

impl KernelFeedbackHost {
    /// Builds the host over the process session registry and the service state.
    /// 基于进程会话注册表与服务状态构建宿主。
    pub(crate) fn new(
        registry: Arc<SessionRuntimeRegistry>,
        state: Weak<Mutex<ServiceBridgeState>>,
    ) -> Self {
        Self { registry, state }
    }
}

#[async_trait::async_trait]
impl NativeFeedbackHost for KernelFeedbackHost {
    async fn link_pr(
        &self,
        session_id: SessionId,
        feedback_id: String,
        url: String,
    ) -> Result<serde_json::Value, String> {
        // Spawned agents never see the tool; this check keeps the host closed if one ever did.
        // 派生代理从不看到该工具；即使出现例外，此检查也保持宿主关闭。
        if crate::tool_permission_host::root_session_of(&self.registry, session_id)? != session_id {
            return Err(String::from(
                "Spawned agents cannot link feedback; only the user's own session can.",
            ));
        }
        let state = self
            .state
            .upgrade()
            .ok_or_else(|| String::from("The service is shutting down."))?;
        // Localized refusals render in the default (English) catalog language for the model.
        // 目录化的拒绝以默认（英文）目录语言渲染给模型。
        let links = crate::feedback_services::service_link_feedback_pr(state, feedback_id, url)
            .await
            .map_err(|error| error.to_string())?;
        serde_json::to_value(links).map_err(|error| error.to_string())
    }
}
