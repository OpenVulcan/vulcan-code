use super::*;

/// 暴露给宿主运行时与执行规划器的稳定 MCP 工具 Schema。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct McpRuntimeToolSchema {
    /// Stable MCP tool name.
    /// 稳定 MCP 工具名称。
    name: String,
    /// Human-readable MCP tool description.
    /// 人类可读的 MCP 工具描述。
    description: String,
    /// JSON Schema describing the tool arguments.
    /// 描述工具参数的 JSON Schema。
    args_schema: serde_json::Value,
}

/// Stable MCP tool descriptor exposed to the host runtime and execution planner.
/// 暴露给宿主运行时与执行规划器的稳定 MCP 工具描述。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct McpRuntimeToolDescriptor {
    /// Stable server identifier that currently owns the tool.
    /// 当前拥有该工具的稳定服务器标识。
    server_id: String,
    /// Tool schema discovered from the remote MCP endpoint.
    /// 从远端 MCP 端点发现得到的工具 schema。
    schema: McpRuntimeToolSchema,
}

impl McpRuntimeToolDescriptor {
    /// Builds one stable MCP runtime tool descriptor from raw transport schema parts.
    /// 根据原始传输 schema 片段构建单个稳定 MCP 运行时工具描述。
    pub(super) fn new(
        server_id: impl Into<String>,
        tool_name: impl Into<String>,
        tool_description: impl Into<String>,
        args_schema: serde_json::Value,
    ) -> Self {
        Self {
            server_id: server_id.into(),
            schema: McpRuntimeToolSchema {
                name: tool_name.into(),
                description: tool_description.into(),
                args_schema,
            },
        }
    }

    /// Returns the stable server identifier that currently owns the tool.
    /// 返回当前拥有该工具的稳定服务器标识。
    pub fn server_id(&self) -> &str {
        &self.server_id
    }

    /// Returns one preferred public tool name before conflict resolution is applied.
    /// 返回应用冲突消解前的首选公开工具名。
    pub(super) fn preferred_public_tool_name(&self) -> String {
        build_prefixed_public_tool_name("mcp", &[self.server_id(), self.tool_name()])
    }

    /// Returns one host-facing read-only tool view derived from the runtime descriptor and one resolved public identifier.
    /// 根据运行时描述与一份已解析公开标识生成宿主侧只读工具视图。
    pub(super) fn host_tool_view(&self, public_tool_id: String) -> McpRuntimeToolView {
        McpRuntimeToolView {
            id: public_tool_id.clone(),
            name: public_tool_id,
            description: self.tool_description().to_string(),
            args_schema: self.args_schema().clone(),
            origin_tool_id: self.qualified_name(),
        }
    }

    /// Returns the stable MCP tool name.
    /// 返回稳定 MCP 工具名称。
    pub(super) fn tool_name(&self) -> &str {
        &self.schema.name
    }

    /// Returns the human-readable MCP tool description.
    /// 返回人类可读的 MCP 工具描述。
    pub(super) fn tool_description(&self) -> &str {
        &self.schema.description
    }

    /// Returns the JSON Schema describing the tool arguments.
    /// 返回描述工具参数的 JSON Schema。
    pub(super) fn args_schema(&self) -> &serde_json::Value {
        &self.schema.args_schema
    }

    /// Returns the host-facing qualified tool name composed from server and tool identifiers.
    /// 返回由服务器与工具标识组合而成的宿主侧限定工具名。
    pub(super) fn qualified_name(&self) -> String {
        format!("{}/{}", self.server_id(), self.tool_name())
    }

    /// Returns whether the descriptor matches one optional server/tool allowlist pair.
    /// 返回该描述是否匹配一组选填的服务器与工具白名单。
    pub fn matches_allowed_sets(
        &self,
        allowed_server_ids: Option<&BTreeSet<String>>,
        allowed_tool_names: Option<&BTreeSet<String>>,
    ) -> bool {
        let server_allowed =
            allowed_server_ids.is_none_or(|allowed| allowed.contains(self.server_id()));
        let tool_allowed = allowed_tool_names.is_none_or(|allowed| {
            allowed.contains(self.tool_name()) || allowed.contains(&self.qualified_name())
        });
        server_allowed && tool_allowed
    }
}

/// Host-facing read-only MCP tool view used by upper runtime projections and bridge layers.
/// 供上层运行时投影与桥接层使用的宿主侧只读 MCP 工具视图。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct McpRuntimeToolView {
    /// Stable host-facing tool identifier.
    /// 稳定的宿主侧工具标识。
    id: String,
    /// Human-readable tool name.
    /// 人类可读工具名称。
    name: String,
    /// Human-readable tool description.
    /// 人类可读工具描述。
    description: String,
    /// JSON Schema describing the tool arguments.
    /// 描述工具参数的 JSON Schema。
    args_schema: serde_json::Value,
    /// Original qualified MCP tool identifier preserved for live invocation.
    /// 为实时调用保留的原始限定 MCP 工具标识。
    origin_tool_id: String,
}

impl McpRuntimeToolView {
    /// Returns the stable host-facing tool identifier.
    /// 返回稳定的宿主侧工具标识。
    pub(super) fn id(&self) -> &str {
        &self.id
    }

    /// Returns the human-readable tool name.
    /// 返回人类可读工具名称。
    pub(super) fn name(&self) -> &str {
        &self.name
    }

    /// Returns the human-readable tool description.
    /// 返回人类可读工具描述。
    pub(super) fn description(&self) -> &str {
        &self.description
    }

    /// Returns the JSON Schema describing the tool arguments.
    /// 返回描述工具参数的 JSON Schema。
    pub(super) fn args_schema(&self) -> &serde_json::Value {
        &self.args_schema
    }

    /// Returns the original qualified MCP tool identifier preserved for live invocation.
    /// 返回为实时调用保留的原始限定 MCP 工具标识。
    pub(super) fn origin_tool_id(&self) -> &str {
        &self.origin_tool_id
    }
}

/// Stable MCP runtime tool binding that couples one host-facing tool view with its live invocation capability.
/// 将宿主侧工具视图与实时调用能力耦合在一起的稳定 MCP 运行时工具绑定。
#[derive(Debug, Clone)]
pub struct McpRuntimeToolBinding {
    /// Host-facing read-only tool view.
    /// 宿主侧只读工具视图。
    view: McpRuntimeToolView,
    /// Live runtime handle used to resolve the owning transport at invocation time.
    /// 在调用时用于解析所属传输的实时运行时句柄。
    runtime: McpRuntimeHandle,
}

/// Stable MCP runtime access surface that resolves live-versus-configured reads inside the MCP domain.
/// 在 MCP 域内部解析实时与配置读取来源的稳定 MCP 运行时访问面。
#[derive(Debug, Clone)]
pub struct McpRuntimeAccess {
    /// Lightweight configured MCP overview retained for fallback reads.
    /// 为回退读取保留的轻量 MCP 配置概览。
    configured_overview: McpRuntimeOverview,
    /// Optional live MCP runtime handle used when real runtime state is available.
    /// 在真实运行时状态可用时使用的可选实时 MCP 运行时句柄。
    live_runtime: Option<McpRuntimeHandle>,
}

impl McpRuntimeAccess {
    /// Builds one stable MCP runtime access surface from configured summary data and one optional live runtime handle.
    /// 根据配置摘要数据与一个可选实时运行时句柄构建单个稳定 MCP 运行时访问面。
    pub(super) fn new(
        configured_overview: McpRuntimeOverview,
        live_runtime: Option<McpRuntimeHandle>,
    ) -> Self {
        Self {
            configured_overview,
            live_runtime,
        }
    }

    /// Returns one stable MCP runtime overview backed by live runtime state when available, or configured fallback data otherwise.
    /// 返回一份在实时运行时可用时优先使用实时状态、否则回退到配置摘要数据的稳定 MCP 运行时概览。
    pub fn overview(&self) -> McpRuntimeOverview {
        self.live_runtime
            .as_ref()
            .map(McpRuntimeHandle::overview)
            .unwrap_or_else(|| self.configured_overview.clone())
    }

    /// Returns currently available MCP tool bindings filtered by one allowlist when live runtime state is available.
    /// 当实时运行时状态可用时，返回按单份白名单过滤后的当前可用 MCP 工具绑定集合。
    pub fn available_tool_bindings_filtered(
        &self,
        allowlist: &McpRuntimeToolAllowlist,
    ) -> Vec<McpRuntimeToolBinding> {
        self.live_runtime
            .as_ref()
            .map(|runtime| runtime.available_tool_bindings_filtered(allowlist))
            .unwrap_or_default()
    }

    /// Requests the live MCP runtime to stop when one real runtime handle is currently available.
    /// 当当前存在真实运行时句柄时，请求实时 MCP 运行时停止。
    pub fn shutdown(&self) {
        if let Some(runtime) = self.live_runtime.as_ref() {
            runtime.shutdown();
        }
    }

    /// Returns the current OAuth status label for one configured MCP server when live runtime state is available.
    /// 当实时运行时状态可用时，返回单个已配置 MCP 服务器的当前 OAuth 状态标签。
    pub async fn server_oauth_status_label(&self, server_id: &str) -> Result<String, String> {
        let Some(runtime) = self.live_runtime.as_ref() else {
            return Ok(String::from("运行时未就绪"));
        };
        runtime.server_auth_status_label(server_id).await
    }

    /// Starts one host-driven OAuth flow for the target MCP server and returns the browser challenge payload.
    /// 为目标 MCP 服务器启动一条宿主驱动的 OAuth 流程，并返回浏览器挑战载荷。
    pub async fn start_server_oauth(
        &self,
        server_id: &str,
        client_name: &str,
    ) -> Result<McpAuthChallenge, String> {
        let Some(runtime) = self.live_runtime.as_ref() else {
            return Err(String::from(
                "MCP 运行时尚未就绪，暂时无法启动 OAuth 授权。",
            ));
        };
        runtime.start_server_auth(server_id, client_name).await
    }

    /// Finishes one host-driven OAuth flow for the target MCP server with the returned authorization code and state.
    /// 使用回传的授权码与 state 为目标 MCP 服务器完成一条宿主驱动的 OAuth 流程。
    pub async fn finish_server_oauth(
        &self,
        server_id: &str,
        authorization_code: &str,
        oauth_state: &str,
    ) -> Result<McpAuthStatus, String> {
        let Some(runtime) = self.live_runtime.as_ref() else {
            return Err(String::from(
                "MCP 运行时尚未就绪，暂时无法完成 OAuth 授权。",
            ));
        };
        runtime
            .finish_server_auth(server_id, authorization_code, oauth_state)
            .await
    }

    /// Removes stored OAuth credentials for the target MCP server when live runtime state is available.
    /// 当实时运行时状态可用时，为目标 MCP 服务器移除已存储的 OAuth 凭据。
    pub async fn remove_server_oauth(&self, server_id: &str) -> Result<(), String> {
        let Some(runtime) = self.live_runtime.as_ref() else {
            return Err(String::from(
                "MCP 运行时尚未就绪，暂时无法清理 OAuth 凭据。",
            ));
        };
        runtime.remove_server_auth(server_id).await
    }
}

/// Builds one stable MCP runtime access surface from provider configuration without a live runtime handle.
/// 根据供应商配置构建一个不包含实时运行时句柄的稳定 MCP 运行时访问面。
pub fn configured_mcp_runtime_access(config: &ProviderConfigurationFile) -> McpRuntimeAccess {
    McpRuntimeAccess::new(configured_mcp_registry_overview(config), None)
}

impl McpRuntimeToolBinding {
    /// Builds one MCP runtime tool binding from one host-facing tool view and one live runtime handle.
    /// 根据一份宿主侧工具视图与一份实时运行时句柄构建单个 MCP 运行时工具绑定。
    pub(super) fn new(view: McpRuntimeToolView, runtime: McpRuntimeHandle) -> Self {
        Self { view, runtime }
    }

    /// Returns the stable host-facing tool identifier.
    /// 返回稳定的宿主侧工具标识。
    pub fn id(&self) -> &str {
        self.view.id()
    }

    /// Returns the human-readable tool name.
    /// 返回人类可读工具名称。
    pub fn name(&self) -> &str {
        self.view.name()
    }

    /// Returns the human-readable tool description.
    /// 返回人类可读工具描述。
    pub fn description(&self) -> &str {
        self.view.description()
    }

    /// Returns the JSON Schema describing the tool arguments.
    /// 返回描述工具参数的 JSON Schema。
    pub fn args_schema(&self) -> &serde_json::Value {
        self.view.args_schema()
    }

    /// Returns the original runtime-qualified MCP tool identifier.
    /// 返回原始运行时限定 MCP 工具标识。
    pub fn origin_tool_id(&self) -> &str {
        self.view.origin_tool_id()
    }

    /// Returns the source MCP server identifier parsed from the original tool id.
    /// 返回从原始工具标识解析出的来源 MCP 服务标识。
    pub fn server_id(&self) -> Option<&str> {
        self.view
            .origin_tool_id()
            .split_once('/')
            .map(|(server_id, _)| server_id)
            .filter(|server_id| !server_id.is_empty())
    }

    /// Returns the source MCP tool name parsed from the original tool id.
    /// 返回从原始工具标识解析出的来源 MCP 工具名称。
    pub fn origin_tool_name(&self) -> Option<&str> {
        self.view
            .origin_tool_id()
            .split_once('/')
            .map(|(_, tool_name)| tool_name)
            .filter(|tool_name| !tool_name.is_empty())
    }

    /// Invokes the bound MCP tool through its stable host-facing tool identifier.
    /// 通过稳定宿主侧工具标识调用当前绑定的 MCP 工具。
    pub async fn invoke(&self, args: serde_json::Value) -> Result<serde_json::Value, String> {
        self.runtime
            .call_tool_by_id(self.view.origin_tool_id(), args)
            .await
    }
}
