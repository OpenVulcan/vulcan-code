//! GUI-side bridge that adapts native, LuaSkills, and MCP runtime sources into the standalone VulcanToolsManager crate.
//! 将原生工具、LuaSkills 与 MCP 运行时来源适配到独立 VulcanToolsManager crate 的 GUI 侧桥接层。

use crate::luaskills_runtime::{
    LUASKILL_SID_PARAMETER_NAME, LUASKILLS_HOST_MANAGED_SID_PREFIX, LuaSkillsRuntimeHandle,
    build_luaskills_tool_invocation_context, call_luaskills_tool_entry,
    luaskills_entry_args_schema, render_luaskills_invocation_result_text,
};
use crate::mcp::{McpRuntimeHandle, McpRuntimeToolView};
use crate::tooling::{NativeToolCatalog, NativeToolDescriptor};
use luaskills::RuntimeEntryDescriptor;
use serde_json::Value;
use std::path::PathBuf;
use uuid::Uuid;
use vulcan_tools_manager::{
    VulcanPlanToolPolicy, VulcanToolCandidate, VulcanToolDescriptor, VulcanToolHostResult,
    VulcanToolInvocationResult, VulcanToolSourceKind, VulcanToolsManager,
};

/// Builds one standalone host-owned tools manager from the current GUI runtime surfaces.
/// 根据当前 GUI 运行时工具面构建独立宿主自有工具管理器。
pub fn build_vulcan_tools_manager(
    native_catalog: NativeToolCatalog,
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    mcp_runtime: Option<McpRuntimeHandle>,
    workspace_root: Option<PathBuf>,
    candidate_native_tool_ids: Vec<String>,
    candidate_luaskills_tools: Vec<RuntimeEntryDescriptor>,
    candidate_mcp_tools: Vec<McpRuntimeToolView>,
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
                        .map(VulcanToolInvocationResult::from_model_output)
                        .map_err(|error| error.to_string())
                })
            }),
        });
    }

    for entry in candidate_luaskills_tools {
        let descriptor = luaskills_descriptor_to_vulcan(&entry);
        let parameters_schema = luaskills_entry_args_schema(&entry, true);
        let canonical_name = entry.canonical_name.clone();
        let skill_id = entry.skill_id.clone();
        let entry_declares_luaskill_sid = entry_declares_luaskill_sid(&entry);
        let runtime = luaskills_runtime.clone();
        let workspace_root = workspace_root.clone();
        candidates.push(VulcanToolCandidate {
            descriptor,
            parameters_schema,
            ai_visible: true,
            plan_policy: VulcanPlanToolPolicy::HeuristicText(canonical_name.clone()),
            invoke: std::sync::Arc::new(move |args, session_id| {
                let runtime = runtime.clone();
                let canonical_name = canonical_name.clone();
                let skill_id = skill_id.clone();
                let workspace_root = workspace_root.clone();
                Box::pin(async move {
                    let Some(handle) = runtime else {
                        return Err(format!(
                            "LuaSkills 运行时当前不可用，无法调用工具 `{}`。",
                            canonical_name
                        ));
                    };
                    let args = inject_host_managed_luaskill_sid_if_needed(
                        args,
                        entry_declares_luaskill_sid,
                        session_id.as_deref(),
                        &canonical_name,
                    )?;
                    let parsed_session_id = session_id
                        .as_deref()
                        .and_then(|value| Uuid::parse_str(value).ok());
                    let invocation_context = build_luaskills_tool_invocation_context(
                        parsed_session_id,
                        "host-tool",
                        &canonical_name,
                    );
                    let call_handle = handle.clone();
                    let result = call_luaskills_tool_entry(
                        call_handle,
                        canonical_name.clone(),
                        args,
                        Some(invocation_context),
                    )
                    .await
                    .map_err(|error| {
                        format!("LuaSkills 工具 `{}` 调用失败：{}。", canonical_name, error)
                    })?;
                    let model_output = Value::String(render_luaskills_invocation_result_text(
                        &handle,
                        &canonical_name,
                        Some(&skill_id),
                        &result,
                        workspace_root.as_deref(),
                    ));
                    let invocation_result = if let Some(host_result) = result.host_result.as_ref() {
                        VulcanToolInvocationResult::from_model_output(model_output)
                            .with_host_result(VulcanToolHostResult {
                                kind: host_result.kind.clone(),
                                payload: host_result.payload.clone(),
                            })
                    } else {
                        VulcanToolInvocationResult::from_model_output(model_output)
                    };
                    Ok(invocation_result)
                })
            }),
        });
    }

    for tool in candidate_mcp_tools {
        let descriptor = mcp_tool_view_to_vulcan(&tool);
        let parameters_schema = tool.args_schema().clone();
        let runtime = mcp_runtime.clone();
        let tool_id = tool.id().to_string();
        let tool_name = tool.name().to_string();
        let runtime_unavailable_text =
            format!("MCP 运行时当前不可用，无法调用工具 `{}`。", tool_name);
        candidates.push(VulcanToolCandidate {
            descriptor,
            parameters_schema,
            ai_visible: true,
            plan_policy: VulcanPlanToolPolicy::HeuristicText(tool_name.clone()),
            invoke: std::sync::Arc::new(move |args, _session_id| {
                let runtime = runtime.clone();
                let tool_id = tool_id.clone();
                let runtime_unavailable_text = runtime_unavailable_text.clone();
                Box::pin(async move {
                    let Some(runtime) = runtime.as_ref() else {
                        return Err(runtime_unavailable_text);
                    };
                    runtime
                        .call_tool_by_id(&tool_id, args)
                        .await
                        .map(VulcanToolInvocationResult::from_model_output)
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
fn mcp_tool_view_to_vulcan(tool: &McpRuntimeToolView) -> VulcanToolDescriptor {
    VulcanToolDescriptor {
        id: tool.id().to_string(),
        name: tool.name().to_string(),
        description: tool.description().to_string(),
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

/// Returns whether one LuaSkills runtime entry declares the reserved managed identity field.
/// 返回单个 LuaSkills 运行时入口是否声明了保留托管身份字段。
fn entry_declares_luaskill_sid(entry: &RuntimeEntryDescriptor) -> bool {
    entry
        .parameters
        .iter()
        .any(|parameter| parameter.name == LUASKILL_SID_PARAMETER_NAME)
}

/// Inject one host-managed LuaSkills SID when the entry declares it and the caller did not provide it.
/// 当入口声明该字段且调用方未提供时，注入一份宿主管理 LuaSkills SID。
fn inject_host_managed_luaskill_sid_if_needed(
    args: Value,
    entry_declares_luaskill_sid: bool,
    session_id: Option<&str>,
    tool_name: &str,
) -> Result<Value, String> {
    if !entry_declares_luaskill_sid {
        return Ok(args);
    }

    let stable_session_id = session_id.ok_or_else(|| {
        format!("LuaSkills 工具 `{tool_name}` 需要宿主管理的 LUASKILL_SID，但当前会话标识不可用。")
    })?;

    let mut object = match args {
        Value::Object(map) => map,
        _ => {
            return Err(format!(
                "LuaSkills 工具 `{tool_name}` 的参数必须是 JSON 对象，才能注入宿主管理 LUASKILL_SID。"
            ));
        }
    };

    if !object.contains_key(LUASKILL_SID_PARAMETER_NAME) {
        object.insert(
            String::from(LUASKILL_SID_PARAMETER_NAME),
            Value::String(format!(
                "{LUASKILLS_HOST_MANAGED_SID_PREFIX}{stable_session_id}"
            )),
        );
    }

    Ok(Value::Object(object))
}
