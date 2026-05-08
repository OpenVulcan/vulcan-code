//! Built-in native tools exposed to the compose runtime foundation.
//! 暴露给 compose 运行时基础架构的内建原生工具。

use crate::agent_runtime::config::AgentRuntimeConfig;
use rig_compose::registry::ToolRegistry;
use rig_compose::tool::{LocalTool, ToolSchema};
use serde_json::{Value, json};
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

/// Aggregate native tool catalog built for the current runtime configuration.
/// 为当前运行时配置构建的原生工具目录聚合体。
#[derive(Clone)]
pub struct NativeToolCatalog {
    /// Shared tool registry instance.
    /// 共享工具注册表实例。
    pub registry: ToolRegistry,
    /// Lightweight tool descriptors used by placeholder panels.
    /// 占位面板使用的轻量工具描述集合。
    pub descriptors: Vec<NativeToolDescriptor>,
}

/// Builds the built-in native tool catalog from the runtime aggregate configuration.
/// 根据运行时聚合配置构建内建原生工具目录。
pub fn build_native_tool_catalog(config: &AgentRuntimeConfig) -> NativeToolCatalog {
    let registry = ToolRegistry::new();
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

    let descriptors = vec![
        NativeToolDescriptor {
            id: String::from("workspace.describe"),
            name: String::from("工作区概览"),
            description: String::from("返回当前工作目录、激活模型以及基础运行时统计。"),
        },
        NativeToolDescriptor {
            id: String::from("registry.list_agents"),
            name: String::from("列出代理"),
            description: String::from("返回当前已声明的代理清单。"),
        },
        NativeToolDescriptor {
            id: String::from("registry.list_skills"),
            name: String::from("列出技能"),
            description: String::from("返回当前已声明的技能清单。"),
        },
        NativeToolDescriptor {
            id: String::from("mcp.list_servers"),
            name: String::from("列出 MCP 服务器"),
            description: String::from("返回当前 MCP 服务器配置及启用情况。"),
        },
    ];

    for descriptor in &descriptors {
        let tool = match descriptor.id.as_str() {
            "workspace.describe" => build_workspace_describe_tool(snapshot.clone()),
            "registry.list_agents" => build_agent_list_tool(snapshot.clone(), config),
            "registry.list_skills" => build_skill_list_tool(snapshot.clone(), config),
            "mcp.list_servers" => build_mcp_server_list_tool(snapshot.clone()),
            _ => continue,
        };
        registry.register(tool);
    }

    NativeToolCatalog {
        registry,
        descriptors,
    }
}

/// Builds the workspace-describe local tool.
/// 构建工作区概览本地工具。
fn build_workspace_describe_tool(
    snapshot: Arc<NativeToolRuntimeSnapshot>,
) -> Arc<dyn rig_compose::tool::Tool> {
    let schema = ToolSchema {
        name: String::from("workspace.describe"),
        description: String::from("Describe the current workspace runtime snapshot."),
        args_schema: json!({"type": "object"}),
        result_schema: json!({"type": "object"}),
    };

    Arc::new(LocalTool::new(schema, move |_args: Value| {
        let snapshot = snapshot.clone();
        async move {
            Ok(json!({
                "workspaceDirectory": snapshot.workspace_directory,
                "activeModel": snapshot.active_model,
                "agentCount": snapshot.agent_count,
                "skillCount": snapshot.skill_count,
                "configuredMcpServers": snapshot.configured_mcp_server_ids,
                "enabledMcpServers": snapshot.enabled_mcp_server_ids,
            }))
        }
    }))
}

/// Builds the agent-list local tool.
/// 构建代理列表本地工具。
fn build_agent_list_tool(
    snapshot: Arc<NativeToolRuntimeSnapshot>,
    config: &AgentRuntimeConfig,
) -> Arc<dyn rig_compose::tool::Tool> {
    let items = config
        .agents
        .iter()
        .map(|agent| {
            json!({
                "id": agent.id,
                "name": agent.name,
                "kind": agent.kind.label(),
                "skills": agent.skill_ids,
                "toolPolicy": agent.tool_policy,
            })
        })
        .collect::<Vec<_>>();
    let items = Arc::new(items);
    let schema = ToolSchema {
        name: String::from("registry.list_agents"),
        description: String::from("List every declared runtime agent."),
        args_schema: json!({"type": "object"}),
        result_schema: json!({"type": "array"}),
    };

    Arc::new(LocalTool::new(schema, move |_args: Value| {
        let items = items.clone();
        let snapshot = snapshot.clone();
        async move {
            Ok(json!({
                "workspaceDirectory": snapshot.workspace_directory,
                "agents": &*items,
            }))
        }
    }))
}

/// Builds the skill-list local tool.
/// 构建技能列表本地工具。
fn build_skill_list_tool(
    snapshot: Arc<NativeToolRuntimeSnapshot>,
    config: &AgentRuntimeConfig,
) -> Arc<dyn rig_compose::tool::Tool> {
    let items = config
        .skills
        .iter()
        .map(|skill| {
            json!({
                "id": skill.id,
                "name": skill.name,
                "kind": skill.kind.label(),
                "enabled": skill.enabled,
                "triggerSignals": skill.trigger_signals,
            })
        })
        .collect::<Vec<_>>();
    let items = Arc::new(items);
    let schema = ToolSchema {
        name: String::from("registry.list_skills"),
        description: String::from("List every declared runtime skill."),
        args_schema: json!({"type": "object"}),
        result_schema: json!({"type": "array"}),
    };

    Arc::new(LocalTool::new(schema, move |_args: Value| {
        let items = items.clone();
        let snapshot = snapshot.clone();
        async move {
            Ok(json!({
                "workspaceDirectory": snapshot.workspace_directory,
                "skills": &*items,
            }))
        }
    }))
}

/// Builds the MCP-server list local tool.
/// 构建 MCP 服务器列表本地工具。
fn build_mcp_server_list_tool(
    snapshot: Arc<NativeToolRuntimeSnapshot>,
) -> Arc<dyn rig_compose::tool::Tool> {
    let schema = ToolSchema {
        name: String::from("mcp.list_servers"),
        description: String::from("List configured MCP server identifiers."),
        args_schema: json!({"type": "object"}),
        result_schema: json!({"type": "object"}),
    };

    Arc::new(LocalTool::new(schema, move |_args: Value| {
        let snapshot = snapshot.clone();
        async move {
            Ok(json!({
                "configured": snapshot.configured_mcp_server_ids,
                "enabled": snapshot.enabled_mcp_server_ids,
            }))
        }
    }))
}
