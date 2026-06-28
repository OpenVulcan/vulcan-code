//! Descriptor conversion and reserved-parameter helpers for host tools.
//! 宿主工具描述转换与保留参数辅助逻辑。

use super::*;

/// Converts one native tool descriptor into the unified host-facing tool descriptor.
/// 将单条原生工具描述转换为统一宿主侧工具描述。
pub(super) fn native_descriptor_to_vulcan(
    descriptor: &NativeToolDescriptor,
) -> VulcanToolDescriptor {
    VulcanToolDescriptor {
        id: descriptor.id.clone(),
        name: descriptor.name.clone(),
        description: descriptor.description.clone(),
        help_text: descriptor.help_text.clone(),
        source: VulcanToolSourceKind::Native,
    }
}

/// Returns whether a native tool id belongs to the VMM memory tool family.
/// 返回单个原生工具标识是否属于 VMM 记忆工具族。
pub(super) fn native_tool_id_is_vmm_memory_tool(tool_id: &str) -> bool {
    matches!(
        tool_id,
        "vmm_memory_search" | "vmm_turn_details" | "vmm_memory_write" | "vmm_memory_delete"
    )
}

/// Converts one LuaSkills runtime entry into the unified host-facing tool descriptor.
/// 将单条 LuaSkills 运行时入口转换为统一宿主侧工具描述。
pub(super) fn luaskills_descriptor_to_vulcan(
    entry: &RuntimeEntryDescriptor,
    occupied_public_tool_ids: &mut BTreeSet<String>,
) -> VulcanToolDescriptor {
    let preferred_public_id = build_direct_public_tool_name(&entry.canonical_name);
    let public_id = if occupied_public_tool_ids.contains(&preferred_public_id) {
        allocate_unique_public_tool_name(
            &build_prefixed_public_tool_name("ls", &[&entry.canonical_name]),
            occupied_public_tool_ids,
        )
    } else {
        allocate_unique_public_tool_name(&preferred_public_id, occupied_public_tool_ids)
    };

    VulcanToolDescriptor {
        id: public_id.clone(),
        name: public_id,
        description: entry.description.clone(),
        help_text: None,
        source: VulcanToolSourceKind::LuaSkills,
    }
}

/// Converts one MCP runtime binding into the unified host-facing tool descriptor.
/// 将单条 MCP 运行时工具绑定转换为统一宿主侧工具描述。
pub(super) fn mcp_tool_binding_to_vulcan(tool: &McpRuntimeToolBinding) -> VulcanToolDescriptor {
    VulcanToolDescriptor {
        id: tool.id().to_string(),
        name: tool.name().to_string(),
        description: tool.description().to_string(),
        help_text: None,
        source: VulcanToolSourceKind::Mcp,
    }
}

/// Converts one native-tool Plan execution flag into the standalone manager Plan policy.
/// 将单条原生工具的 Plan 执行许可标记转换为独立管理器的 Plan 策略。
pub(super) fn native_plan_policy(descriptor: &NativeToolDescriptor) -> VulcanPlanToolPolicy {
    if descriptor.available_in_plan {
        VulcanPlanToolPolicy::Visible
    } else {
        VulcanPlanToolPolicy::Hidden
    }
}

/// Returns whether one LuaSkills runtime entry declares the reserved managed identity field.
/// 返回单个 LuaSkills 运行时入口是否声明了保留托管身份字段。
pub(super) fn entry_declares_luaskill_sid(entry: &RuntimeEntryDescriptor) -> bool {
    entry_declares_reserved_parameter(entry, LUASKILL_SID_PARAMETER_NAME)
}

/// Returns whether one LuaSkills runtime entry declares the reserved managed project-path field.
/// 返回单个 LuaSkills 运行时入口是否声明了保留托管项目路径字段。
pub(super) fn entry_declares_pwd(entry: &RuntimeEntryDescriptor) -> bool {
    entry_declares_reserved_parameter(entry, PWD_PARAMETER_NAME)
}

/// Returns whether one LuaSkills runtime entry declares one reserved parameter name in either schema properties or legacy parameter descriptors.
/// 返回单个 LuaSkills 运行时入口是否在 schema properties 或旧版参数描述中声明了某个保留参数名。
fn entry_declares_reserved_parameter(entry: &RuntimeEntryDescriptor, parameter_name: &str) -> bool {
    entry
        .input_schema
        .get("properties")
        .and_then(Value::as_object)
        .is_some_and(|properties| properties.contains_key(parameter_name))
        || entry
            .parameters
            .iter()
            .any(|parameter| parameter.name == parameter_name)
}
