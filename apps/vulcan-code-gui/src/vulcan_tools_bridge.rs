//! GUI-side bridge that adapts native, LuaSkills, and MCP runtime sources into the standalone VulcanToolsManager crate.
//! 将原生工具、LuaSkills 与 MCP 运行时来源适配到独立 VulcanToolsManager crate 的 GUI 侧桥接层。

use crate::luaskills_runtime::{
    LuaSkillsRuntimeHandle, build_luaskills_tool_invocation_context, call_luaskills_tool_entry,
    luaskills_entry_args_schema, serialize_luaskills_invocation_result,
};
use crate::mcp::{McpRuntimeHandle, McpRuntimeToolDescriptor};
use crate::tooling::{NativeToolCatalog, NativeToolDescriptor};
use luaskills::RuntimeEntryDescriptor;
use vulcan_tools_manager::{
    VulcanPlanToolPolicy, VulcanToolCandidate, VulcanToolDescriptor, VulcanToolSourceKind,
    VulcanToolsManager,
};

/// Builds one standalone host-owned tools manager from the current GUI runtime surfaces.
/// 根据当前 GUI 运行时工具面构建独立宿主自有工具管理器。
pub fn build_vulcan_tools_manager(
    native_catalog: NativeToolCatalog,
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    mcp_runtime: Option<McpRuntimeHandle>,
    candidate_native_tool_ids: Vec<String>,
    candidate_luaskills_tools: Vec<RuntimeEntryDescriptor>,
    candidate_mcp_tools: Vec<McpRuntimeToolDescriptor>,
) -> VulcanToolsManager {
    let mut candidates = Vec::new();

    for tool_id in candidate_native_tool_ids {
        let Some(definition) = native_catalog.definition(&tool_id) else {
            continue;
        };
        let descriptor = native_descriptor_to_vulcan(&definition.descriptor);
        let catalog = native_catalog.clone();
        let tool_id = definition.descriptor.id.clone();
        candidates.push(VulcanToolCandidate {
            descriptor,
            parameters_schema: definition.args_schema.clone(),
            ai_visible: definition.descriptor.ai_visible,
            plan_policy: native_plan_policy(&definition.descriptor),
            invoke: std::sync::Arc::new(move |args, _session_id| {
                let catalog = catalog.clone();
                let tool_id = tool_id.clone();
                Box::pin(async move {
                    catalog
                        .invoke(&tool_id, args)
                        .await
                        .map_err(|error| error.to_string())
                })
            }),
        });
    }

    for entry in candidate_luaskills_tools {
        let descriptor = luaskills_descriptor_to_vulcan(&entry);
        let parameters_schema = luaskills_entry_args_schema(&entry);
        let canonical_name = entry.canonical_name.clone();
        let runtime = luaskills_runtime.clone();
        candidates.push(VulcanToolCandidate {
            descriptor,
            parameters_schema,
            ai_visible: true,
            plan_policy: VulcanPlanToolPolicy::HeuristicText(canonical_name.clone()),
            invoke: std::sync::Arc::new(move |args, session_id| {
                let runtime = runtime.clone();
                let canonical_name = canonical_name.clone();
                Box::pin(async move {
                    let Some(handle) = runtime else {
                        return Err(format!(
                            "LuaSkills 运行时当前不可用，无法调用工具 `{}`。",
                            canonical_name
                        ));
                    };
                    let invocation_context = build_luaskills_tool_invocation_context(
                        session_id,
                        "rig-tool",
                        &canonical_name,
                    );
                    let result = call_luaskills_tool_entry(
                        handle,
                        canonical_name.clone(),
                        args,
                        Some(invocation_context),
                    )
                    .await
                    .map_err(|error| {
                        format!("LuaSkills 工具 `{}` 调用失败：{}。", canonical_name, error)
                    })?;
                    serde_json::to_value(serialize_luaskills_invocation_result(&result)).map_err(
                        |error| {
                            format!(
                                "LuaSkills 工具 `{}` 结果序列化失败：{}。",
                                canonical_name, error
                            )
                        },
                    )
                })
            }),
        });
    }

    for tool in candidate_mcp_tools {
        let descriptor = mcp_descriptor_to_vulcan(&tool);
        let parameters_schema = tool.schema.args_schema.clone();
        let runtime = mcp_runtime.clone();
        let server_id = tool.server_id.clone();
        let tool_name = tool.schema.name.clone();
        candidates.push(VulcanToolCandidate {
            descriptor,
            parameters_schema,
            ai_visible: true,
            plan_policy: VulcanPlanToolPolicy::HeuristicText(tool_name.clone()),
            invoke: std::sync::Arc::new(move |args, _session_id| {
                let runtime = runtime.clone();
                let server_id = server_id.clone();
                let tool_name = tool_name.clone();
                Box::pin(async move {
                    let Some(runtime) = runtime.as_ref() else {
                        return Err(format!(
                            "MCP 运行时当前不可用，无法调用工具 `{}`。",
                            tool_name
                        ));
                    };
                    let Some(transport) = runtime.transport_for_server(&server_id) else {
                        return Err(format!(
                            "MCP 服务器 `{}` 当前未连接，无法调用工具 `{}`。",
                            server_id, tool_name
                        ));
                    };
                    transport
                        .call_tool(&tool_name, args)
                        .await
                        .map_err(|error| format!("MCP 工具 `{}` 调用失败：{}。", tool_name, error))
                })
            }),
        });
    }

    VulcanToolsManager::new(candidates)
}

/// Converts one native tool descriptor into the unified host-facing tool descriptor.
/// 将单条原生工具描述转换为统一宿主侧工具描述。
fn native_descriptor_to_vulcan(descriptor: &NativeToolDescriptor) -> VulcanToolDescriptor {
    VulcanToolDescriptor {
        id: descriptor.id.clone(),
        name: descriptor.name.clone(),
        description: descriptor.description.clone(),
        source: VulcanToolSourceKind::Native,
    }
}

/// Converts one LuaSkills runtime entry into the unified host-facing tool descriptor.
/// 将单条 LuaSkills 运行时入口转换为统一宿主侧工具描述。
fn luaskills_descriptor_to_vulcan(entry: &RuntimeEntryDescriptor) -> VulcanToolDescriptor {
    VulcanToolDescriptor {
        id: entry.canonical_name.clone(),
        name: entry.canonical_name.clone(),
        description: entry.description.clone(),
        source: VulcanToolSourceKind::LuaSkills,
    }
}

/// Converts one MCP runtime descriptor into the unified host-facing tool descriptor.
/// 将单条 MCP 运行时描述转换为统一宿主侧工具描述。
fn mcp_descriptor_to_vulcan(tool: &McpRuntimeToolDescriptor) -> VulcanToolDescriptor {
    VulcanToolDescriptor {
        id: format!("{}/{}", tool.server_id, tool.schema.name),
        name: tool.schema.name.clone(),
        description: tool.schema.description.clone(),
        source: VulcanToolSourceKind::Mcp,
    }
}

/// Converts one native-tool Plan availability flag into the standalone manager Plan policy.
/// 将单条原生工具的 Plan 可用标记转换为独立管理器的 Plan 策略。
fn native_plan_policy(descriptor: &NativeToolDescriptor) -> VulcanPlanToolPolicy {
    if descriptor.available_in_plan {
        VulcanPlanToolPolicy::Visible
    } else {
        VulcanPlanToolPolicy::Hidden
    }
}
