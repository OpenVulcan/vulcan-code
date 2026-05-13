//! Built-in native tools exposed to the host runtime foundation.
//! 暴露给宿主运行时基础架构的内建原生工具。

use crate::agent_runtime::config::AgentRuntimeConfig;
use futures_util::future::BoxFuture;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::sync::Arc;

/// One lightweight descriptor for a registered native tool.
/// 单个已注册原生工具的轻量描述。
#[derive(Debug, Clone)]
pub struct NativeToolDescriptor {
    /// Stable tool identifier.
    /// 稳定工具标识。
    pub id: String,
    /// Human-readable tool name.
    /// 人类可读的工具名称。
    pub name: String,
    /// Human-readable tool description.
    /// 人类可读的工具描述。
    pub description: String,
    /// Whether the tool should be directly visible to AI agents.
    /// 当前工具是否应直接对 AI 代理可见。
    pub ai_visible: bool,
    /// Whether the tool may remain visible in Plan mode.
    /// 当前工具是否允许在 Plan 模式继续可见。
    pub available_in_plan: bool,
}

/// One serialisable runtime snapshot captured by the native tool layer.
/// 原生工具层捕获的一份可序列化运行时快照。
#[derive(Debug, Clone)]
pub struct NativeToolRuntimeSnapshot {
    /// Current workspace directory used by the desktop process.
    /// 当前桌面进程使用的工作区目录。
    pub workspace_directory: String,
    /// Current active model reference.
    /// 当前激活模型引用。
    pub active_model: Option<String>,
    /// Declared agent count.
    /// 已声明代理数量。
    pub agent_count: usize,
    /// Declared skill count.
    /// 已声明技能数量。
    pub skill_count: usize,
    /// Configured MCP server identifiers.
    /// 已配置的 MCP 服务器标识集合。
    pub configured_mcp_server_ids: Vec<String>,
    /// Enabled MCP server identifiers.
    /// 已启用的 MCP 服务器标识集合。
    pub enabled_mcp_server_ids: Vec<String>,
}

/// One lightweight native-tool error surfaced by the host-side tool layer.
/// 宿主侧工具层对外暴露的轻量原生工具错误。
#[derive(Debug, Clone)]
pub struct NativeToolError {
    /// Human-readable error message.
    /// 人类可读错误消息。
    pub message: String,
}

impl NativeToolError {
    /// Builds one native-tool error from a human-readable message.
    /// 根据一条人类可读消息构建原生工具错误。
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for NativeToolError {
    /// Render the native-tool error as one display string.
    /// 将原生工具错误渲染为一条展示字符串。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.message)
    }
}

impl Error for NativeToolError {}

/// Shared boxed future returned by one native-tool handler.
/// 单个原生工具处理器返回的共享装箱 future。
type NativeToolFuture = BoxFuture<'static, Result<Value, NativeToolError>>;

/// Shared async handler signature used by one host-owned native tool.
/// 单个宿主自有原生工具使用的共享异步处理器签名。
type NativeToolHandler = dyn Fn(Value) -> NativeToolFuture + Send + Sync;

/// One host-owned native tool definition used as the source of truth for AI and host surfaces.
/// 作为 AI 与宿主表面事实标准的单个宿主自有原生工具定义。
#[derive(Clone)]
pub struct NativeToolDefinition {
    /// Stable descriptor metadata.
    /// 稳定描述元数据。
    pub descriptor: NativeToolDescriptor,
    /// JSON Schema describing tool arguments.
    /// 描述工具参数的 JSON Schema。
    pub args_schema: Value,
    /// Shared async handler that executes the tool.
    /// 执行该工具的共享异步处理器。
    handler: Arc<NativeToolHandler>,
}

impl NativeToolDefinition {
    /// Executes the native tool handler with one JSON argument object.
    /// 使用单个 JSON 参数对象执行原生工具处理器。
    pub async fn invoke(&self, args: Value) -> Result<Value, NativeToolError> {
        (self.handler)(args).await
    }
}

/// Aggregate native tool catalog built for the current runtime configuration.
/// 为当前运行时配置构建的原生工具目录聚合体。
#[derive(Clone)]
pub struct NativeToolCatalog {
    /// Host-owned native tool definitions keyed by stable tool identifier.
    /// 以稳定工具标识为键保存的宿主自有原生工具定义。
    pub definitions: BTreeMap<String, Arc<NativeToolDefinition>>,
    /// Lightweight tool descriptors used by UI and diagnostics.
    /// 供界面与诊断使用的轻量工具描述集合。
    pub descriptors: Vec<NativeToolDescriptor>,
}

impl NativeToolCatalog {
    /// Resolves one native-tool definition by its stable identifier.
    /// 通过稳定标识解析单个原生工具定义。
    pub fn definition(&self, tool_id: &str) -> Option<Arc<NativeToolDefinition>> {
        self.definitions.get(tool_id).cloned()
    }

    /// Invokes one host-owned native tool through the unified definition table.
    /// 通过统一定义表调用单个宿主自有原生工具。
    pub async fn invoke(&self, tool_id: &str, args: Value) -> Result<Value, NativeToolError> {
        let Some(definition) = self.definition(tool_id) else {
            return Err(NativeToolError::new(format!(
                "未找到原生工具 `{tool_id}`。"
            )));
        };
        definition.invoke(args).await
    }
}

/// Builds the built-in native tool catalog from the runtime aggregate configuration.
/// 根据运行时聚合配置构建内建原生工具目录。
pub fn build_native_tool_catalog(config: &AgentRuntimeConfig) -> NativeToolCatalog {
    let snapshot = NativeToolRuntimeSnapshot {
        workspace_directory: std::env::current_dir()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| String::from(".")),
        active_model: config.active_model.clone(),
        agent_count: config.agents.len(),
        skill_count: config.skills.len(),
        configured_mcp_server_ids: config.configured_mcp_server_ids.clone(),
        enabled_mcp_server_ids: config.enabled_mcp_server_ids.clone(),
    };
    let snapshot = Arc::new(snapshot);

    let definitions = vec![
        build_workspace_describe_tool(snapshot.clone()),
        build_agent_list_tool(snapshot.clone(), config),
        build_skill_list_tool(snapshot.clone(), config),
        build_mcp_server_list_tool(snapshot),
    ];

    let mut definition_map = BTreeMap::new();
    let mut descriptors = Vec::new();
    for definition in definitions {
        descriptors.push(definition.descriptor.clone());
        definition_map.insert(definition.descriptor.id.clone(), definition);
    }

    NativeToolCatalog {
        definitions: definition_map,
        descriptors,
    }
}

/// Builds one host-owned native tool definition from the shared metadata and async handler.
/// 根据共享元数据与异步处理器构建单个宿主自有原生工具定义。
fn build_native_tool_definition(
    descriptor: NativeToolDescriptor,
    args_schema: Value,
    handler: impl Fn(Value) -> NativeToolFuture + Send + Sync + 'static,
) -> Arc<NativeToolDefinition> {
    Arc::new(NativeToolDefinition {
        descriptor,
        args_schema,
        handler: Arc::new(handler),
    })
}

/// Builds the workspace-describe host-owned native tool.
/// 构建工作区概览宿主自有原生工具。
fn build_workspace_describe_tool(
    snapshot: Arc<NativeToolRuntimeSnapshot>,
) -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from("workspace.describe"),
        name: String::from("工作区概览"),
        description: String::from("返回当前工作目录、激活模型以及基础运行时统计。"),
        ai_visible: true,
        available_in_plan: true,
    };

    build_native_tool_definition(
        descriptor,
        json!({"type": "object"}),
        move |_args: Value| {
            let snapshot = snapshot.clone();
            Box::pin(async move {
                Ok(json!({
                    "workspaceDirectory": snapshot.workspace_directory,
                    "activeModel": snapshot.active_model,
                    "agentCount": snapshot.agent_count,
                    "skillCount": snapshot.skill_count,
                    "configuredMcpServers": snapshot.configured_mcp_server_ids,
                    "enabledMcpServers": snapshot.enabled_mcp_server_ids,
                }))
            })
        },
    )
}

/// Builds the agent-list host-owned native tool.
/// 构建代理列表宿主自有原生工具。
fn build_agent_list_tool(
    snapshot: Arc<NativeToolRuntimeSnapshot>,
    config: &AgentRuntimeConfig,
) -> Arc<NativeToolDefinition> {
    let items = config
        .agents
        .iter()
        .map(|agent| {
            json!({
                "id": agent.id,
                "name": agent.name,
                "kind": agent.kind.label(),
                "description": agent.description,
                "systemPrompt": agent.system_prompt,
                "skills": agent.skill_ids,
                "nativeTools": agent.native_tools,
                "mcpTools": agent.mcp_tools,
                "delegates": agent.delegates,
                "toolPolicy": agent.tool_policy,
                "fallbackDelegate": agent.fallback_delegate,
                "model": agent.model,
                "temperature": agent.temperature,
                "maxTurns": agent.max_turns,
                "routes": agent
                    .routes
                    .iter()
                    .map(|route| {
                        json!({
                            "signal": route.signal,
                            "delegate": route.delegate,
                        })
                    })
                    .collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();
    let items = Arc::new(items);
    let descriptor = NativeToolDescriptor {
        id: String::from("registry.list_agents"),
        name: String::from("列出代理"),
        description: String::from("返回当前已声明的代理清单。"),
        ai_visible: false,
        available_in_plan: false,
    };

    build_native_tool_definition(
        descriptor,
        json!({"type": "object"}),
        move |_args: Value| {
            let items = items.clone();
            let snapshot = snapshot.clone();
            Box::pin(async move {
                Ok(json!({
                    "workspaceDirectory": snapshot.workspace_directory,
                    "agents": &*items,
                }))
            })
        },
    )
}

/// Builds the skill-list host-owned native tool.
/// 构建技能列表宿主自有原生工具。
fn build_skill_list_tool(
    snapshot: Arc<NativeToolRuntimeSnapshot>,
    config: &AgentRuntimeConfig,
) -> Arc<NativeToolDefinition> {
    let items = config
        .skills
        .iter()
        .map(|skill| {
            json!({
                "id": skill.id,
                "name": skill.name,
                "kind": skill.kind.label(),
                "description": skill.description,
                "enabled": skill.enabled,
                "triggerSignals": skill.trigger_signals,
                "emittedSignal": skill.emitted_signal,
                "confidenceDelta": skill.confidence_delta,
            })
        })
        .collect::<Vec<_>>();
    let items = Arc::new(items);
    let descriptor = NativeToolDescriptor {
        id: String::from("registry.list_skills"),
        name: String::from("列出技能"),
        description: String::from("返回当前已声明的技能清单。"),
        ai_visible: true,
        available_in_plan: true,
    };

    build_native_tool_definition(
        descriptor,
        json!({"type": "object"}),
        move |_args: Value| {
            let items = items.clone();
            let snapshot = snapshot.clone();
            Box::pin(async move {
                Ok(json!({
                    "workspaceDirectory": snapshot.workspace_directory,
                    "skills": &*items,
                }))
            })
        },
    )
}

/// Builds the MCP-server list host-owned native tool.
/// 构建 MCP 服务器列表宿主自有原生工具。
fn build_mcp_server_list_tool(
    snapshot: Arc<NativeToolRuntimeSnapshot>,
) -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from("mcp.list_servers"),
        name: String::from("列出 MCP 服务器"),
        description: String::from("返回当前 MCP 服务器配置及启用情况。"),
        ai_visible: true,
        available_in_plan: true,
    };

    build_native_tool_definition(
        descriptor,
        json!({"type": "object"}),
        move |_args: Value| {
            let snapshot = snapshot.clone();
            Box::pin(async move {
                Ok(json!({
                    "configured": snapshot.configured_mcp_server_ids,
                    "enabled": snapshot.enabled_mcp_server_ids,
                }))
            })
        },
    )
}

#[cfg(test)]
mod tests {
    use super::{build_agent_list_tool, build_skill_list_tool};
    use crate::agent_runtime::config::{
        AgentRuntimeConfig, RuntimeAgentDefinition, RuntimeAgentKind, RuntimeAgentRoute,
        RuntimeSkillDefinition, RuntimeSkillKind,
    };
    use std::collections::BTreeMap;
    use std::sync::Arc;

    /// Verifies that the agent registry tool exposes extended runtime fields for diagnostics and future delegate orchestration.
    /// 验证代理注册表工具会暴露扩展运行时字段，用于诊断与后续委派编排。
    #[tokio::test]
    async fn agent_registry_tool_exposes_extended_runtime_fields() {
        let snapshot = Arc::new(super::NativeToolRuntimeSnapshot {
            workspace_directory: String::from("D:/workspace"),
            active_model: Some(String::from("openai/gpt-5")),
            agent_count: 1,
            skill_count: 1,
            configured_mcp_server_ids: vec![String::from("server-a")],
            enabled_mcp_server_ids: vec![String::from("server-a")],
        });
        let config = AgentRuntimeConfig {
            active_model: Some(String::from("openai/gpt-5")),
            active_agent_id: String::from("agent-a"),
            agents: vec![RuntimeAgentDefinition {
                id: String::from("agent-a"),
                name: String::from("Agent A"),
                kind: RuntimeAgentKind::Coordinator,
                description: String::from("Coordinator agent"),
                system_prompt: String::from("system prompt"),
                skill_ids: vec![String::from("skill-a")],
                native_tools: vec![String::from("native-a")],
                mcp_tools: vec![String::from("mcp-tool-a")],
                delegates: vec![String::from("agent-b")],
                tool_policy: Some(String::from("policy-a")),
                routes: vec![RuntimeAgentRoute {
                    signal: String::from("needs-lua"),
                    delegate: String::from("lua-agent"),
                }],
                fallback_delegate: Some(String::from("agent-b")),
                model: Some(String::from("anthropic/claude")),
                temperature: Some(0.2),
                max_turns: Some(9),
            }],
            skills: vec![RuntimeSkillDefinition {
                id: String::from("skill-a"),
                name: String::from("Skill A"),
                kind: RuntimeSkillKind::Regular,
                description: String::from("Skill description"),
                enabled: true,
                trigger_signals: vec![String::from("signal-a")],
                emitted_signal: Some(String::from("signal-b")),
                confidence_delta: 0.5,
            }],
            tool_policies: BTreeMap::new(),
            configured_mcp_server_ids: vec![String::from("server-a")],
            enabled_mcp_server_ids: vec![String::from("server-a")],
        };

        let tool = build_agent_list_tool(snapshot, &config);
        let payload = tool
            .invoke(serde_json::Value::Object(Default::default()))
            .await
            .expect("agent registry tool should execute");

        let agent = &payload["agents"][0];
        assert_eq!(agent["description"], "Coordinator agent");
        assert_eq!(agent["systemPrompt"], "system prompt");
        assert_eq!(agent["fallbackDelegate"], "agent-b");
        assert_eq!(agent["model"], "anthropic/claude");
        let temperature = agent["temperature"]
            .as_f64()
            .expect("temperature should serialize as f64");
        assert!((temperature - 0.2).abs() < 1e-6);
        assert_eq!(agent["maxTurns"], 9);
        assert_eq!(agent["routes"][0]["signal"], "needs-lua");
        assert_eq!(agent["routes"][0]["delegate"], "lua-agent");
    }

    /// Verifies that the skill registry tool exposes skill metadata that used to stay hidden behind dead-code suppressions.
    /// 验证技能注册表工具会暴露此前被 dead-code 抑制隐藏的技能元数据。
    #[tokio::test]
    async fn skill_registry_tool_exposes_extended_runtime_fields() {
        let snapshot = Arc::new(super::NativeToolRuntimeSnapshot {
            workspace_directory: String::from("D:/workspace"),
            active_model: Some(String::from("openai/gpt-5")),
            agent_count: 1,
            skill_count: 1,
            configured_mcp_server_ids: vec![],
            enabled_mcp_server_ids: vec![],
        });
        let config = AgentRuntimeConfig {
            active_model: Some(String::from("openai/gpt-5")),
            active_agent_id: String::from("agent-a"),
            agents: vec![],
            skills: vec![RuntimeSkillDefinition {
                id: String::from("skill-a"),
                name: String::from("Skill A"),
                kind: RuntimeSkillKind::Automation,
                description: String::from("Skill description"),
                enabled: true,
                trigger_signals: vec![String::from("signal-a")],
                emitted_signal: Some(String::from("signal-b")),
                confidence_delta: 0.5,
            }],
            tool_policies: BTreeMap::new(),
            configured_mcp_server_ids: vec![],
            enabled_mcp_server_ids: vec![],
        };

        let tool = build_skill_list_tool(snapshot, &config);
        let payload = tool
            .invoke(serde_json::Value::Object(Default::default()))
            .await
            .expect("skill registry tool should execute");

        let skill = &payload["skills"][0];
        assert_eq!(skill["description"], "Skill description");
        assert_eq!(skill["emittedSignal"], "signal-b");
        assert_eq!(skill["confidenceDelta"], 0.5);
    }
}
