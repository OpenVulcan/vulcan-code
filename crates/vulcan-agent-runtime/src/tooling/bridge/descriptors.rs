//! Descriptor conversion and reserved-parameter helpers for host tools.
//! 宿主工具描述转换与保留参数辅助逻辑。

use super::*;

/// Converts one native tool descriptor into the unified host-facing tool descriptor.
/// 将单条原生工具描述转换为统一宿主侧工具描述。
pub(super) fn native_descriptor_to_vulcan(
    descriptor: &NativeToolDescriptor,
) -> VulcanToolDescriptor {
    VulcanToolDescriptor::new(
        descriptor.id.clone(),
        descriptor.name.clone(),
        descriptor.description.clone(),
        descriptor.help_text.clone(),
        VulcanToolOrigin::new(
            VulcanToolSourceKind::Native,
            "native",
            descriptor.id.clone(),
        ),
    )
}

/// Returns whether a native tool id belongs to the VMM memory tool family.
/// 返回单个原生工具标识是否属于 VMM 记忆工具族。
pub(crate) fn native_tool_id_is_vmm_memory_tool(tool_id: &str) -> bool {
    matches!(
        tool_id,
        "vmm_memory_search" | "vmm_turn_details" | "vmm_memory_write" | "vmm_memory_delete"
    )
}

/// Returns whether a native tool belongs to the project knowledge family.
/// 返回原生工具是否属于项目知识库工具族。
pub(crate) fn native_tool_id_is_knowledge_tool(tool_id: &str) -> bool {
    matches!(
        tool_id,
        "knowledge_search" | "knowledge_list" | "knowledge_read" | "knowledge_write"
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

    VulcanToolDescriptor::new(
        public_id.clone(),
        public_id,
        entry.description.clone(),
        None,
        VulcanToolOrigin::new(
            VulcanToolSourceKind::LuaSkills,
            entry.skill_id.clone(),
            entry.canonical_name.clone(),
        ),
    )
}

/// Converts one MCP runtime binding into the unified host-facing tool descriptor.
/// 将单条 MCP 运行时工具绑定转换为统一宿主侧工具描述。
pub(super) fn mcp_tool_binding_to_vulcan(tool: &McpRuntimeToolBinding) -> VulcanToolDescriptor {
    // Hook declarations name the source MCP server, while the mutable partition key only
    // selects the live generation. Exposing that key here would make a static hook miss.
    // Hook 声明命名来源 MCP 服务；可变分区键仅选择活动代次。若在此暴露分区键，静态 Hook 将失配。
    VulcanToolDescriptor::new(
        tool.id().to_string(),
        tool.name().to_string(),
        tool.description().to_string(),
        None,
        VulcanToolOrigin::new(
            VulcanToolSourceKind::Mcp,
            tool.server_id()
                .expect("MCP runtime bindings must retain an original server identity"),
            tool.origin_tool_name()
                .expect("MCP runtime bindings must retain a qualified origin tool name"),
        ),
    )
}

/// Converts one native-tool Plan execution flag into the standalone manager Plan policy.
/// 将单条原生工具的 Plan 执行许可标记转换为独立管理器的 Plan 策略。
pub(super) fn native_plan_policy(descriptor: &NativeToolDescriptor) -> VulcanPlanToolPolicy {
    if descriptor.id == crate::tooling::native::FILE_TOOL_ID {
        VulcanPlanToolPolicy::StringArgumentAllowlist {
            field: String::from("action"),
            allowed_values: vec![String::from("list"), String::from("stat")],
        }
    } else if descriptor.id == crate::tooling::native::ENVIRONMENT_TOOL_ID {
        // Plan mode only reads variables; setting or requesting them changes later commands.
        // 计划模式只读取变量；设置或索取变量会改变后续命令。
        VulcanPlanToolPolicy::StringArgumentAllowlist {
            field: String::from("action"),
            allowed_values: vec![String::from("list")],
        }
    } else if descriptor.id == crate::tooling::native::MCP_CONNECT_TOOL_ID {
        // Plan mode only inspects connections; opening one starts a process or reaches a server.
        // 计划模式只查看连接；打开连接会启动进程或访问服务。
        VulcanPlanToolPolicy::StringArgumentAllowlist {
            field: String::from("action"),
            allowed_values: vec![String::from("status")],
        }
    } else if descriptor.id == crate::tooling::native::MCP_CONFIG_TOOL_ID
        || descriptor.id == crate::tooling::native::PROVIDER_CONFIG_TOOL_ID
    {
        // Plan mode only reads configuration; every other action changes it.
        // 计划模式只读取配置；其他动作都会修改配置。
        VulcanPlanToolPolicy::StringArgumentAllowlist {
            field: String::from("action"),
            allowed_values: vec![String::from(
                crate::tooling::native::CONFIGURATION_LIST_ACTION,
            )],
        }
    } else if descriptor.id == crate::tooling::native::LUASKILLS_DEBUG_TOOL_ID {
        // Plan mode only reads the installed state; installing downloads and writes a release.
        // 计划模式只读取安装状态；安装会下载并写入 release。
        VulcanPlanToolPolicy::StringArgumentAllowlist {
            field: String::from("action"),
            allowed_values: vec![String::from(
                crate::tooling::native::LUASKILLS_DEBUG_STATUS_ACTION,
            )],
        }
    } else if descriptor.id == crate::tooling::native::FEEDBACK_TOOL_ID {
        // Plan mode only drafts; linking a pull request writes to the OpenVulcan server.
        // 计划模式只起草；关联拉取请求会写入 OpenVulcan 服务端。
        VulcanPlanToolPolicy::StringArgumentAllowlist {
            field: String::from("action"),
            allowed_values: vec![String::from(crate::tooling::native::FEEDBACK_DRAFT_ACTION)],
        }
    } else if descriptor.id == crate::tooling::native::MCP_CALL_TOOL_ID {
        // Calls use the read-only name heuristic of configured MCP tools, on the called tool.
        // 调用对被调用工具使用已配置 MCP 工具的只读名称启发式。
        VulcanPlanToolPolicy::HeuristicStringArgument {
            field: String::from("tool"),
        }
    } else if descriptor.available_in_plan {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the mixed native file tool keeps only its read-only actions executable in Plan mode.
    /// 验证混合型原生文件工具在计划模式中只允许执行只读 action。
    #[test]
    fn native_file_plan_policy_is_action_scoped() {
        let descriptor = NativeToolDescriptor {
            id: String::from(crate::tooling::native::FILE_TOOL_ID),
            name: String::from("File"),
            description: String::from("File operations."),
            help_text: None,
            ai_visible: true,
            available_in_plan: true,
        };

        let VulcanPlanToolPolicy::StringArgumentAllowlist {
            field,
            allowed_values,
        } = native_plan_policy(&descriptor)
        else {
            panic!("file should use an action-scoped Plan policy");
        };
        assert_eq!(field, "action");
        assert_eq!(allowed_values, ["list", "stat"]);
        assert!(matches!(
            native_plan_policy(&crate::tooling::native::build_file_read_tool().descriptor),
            VulcanPlanToolPolicy::Visible
        ));
        assert!(matches!(
            native_plan_policy(&crate::tooling::native::build_file_write_tool().descriptor),
            VulcanPlanToolPolicy::Hidden
        ));
    }

    /// Plan mode inspects session MCP connections and calls only read-like tools on them.
    /// 计划模式只查看会话 MCP 连接，并且只调用其中的类只读工具。
    #[test]
    fn session_mcp_plan_policies_follow_the_effect() {
        use crate::tooling::native::{
            build_mcp_call_tool, build_mcp_connect_tool, build_mcp_tools_tool,
        };
        let VulcanPlanToolPolicy::StringArgumentAllowlist {
            field,
            allowed_values,
        } = native_plan_policy(&build_mcp_connect_tool().descriptor)
        else {
            panic!("mcp_connect should use an action-scoped Plan policy");
        };
        assert_eq!(field, "action");
        assert_eq!(allowed_values, ["status"]);
        assert!(matches!(
            native_plan_policy(&build_mcp_tools_tool().descriptor),
            VulcanPlanToolPolicy::Visible
        ));
        assert!(matches!(
            native_plan_policy(&build_mcp_call_tool().descriptor),
            VulcanPlanToolPolicy::HeuristicStringArgument { field } if field == "tool"
        ));
    }

    /// Plan mode drafts feedback but never links a pull request.
    /// 计划模式可以起草反馈，但从不关联拉取请求。
    #[test]
    fn feedback_only_drafts_in_plan_mode() {
        use crate::tooling::native::{FEEDBACK_DRAFT_ACTION, build_feedback_tool};
        let VulcanPlanToolPolicy::StringArgumentAllowlist {
            field,
            allowed_values,
        } = native_plan_policy(&build_feedback_tool().descriptor)
        else {
            panic!("feedback should use an action-scoped Plan policy");
        };
        assert_eq!(field, "action");
        assert_eq!(allowed_values, [FEEDBACK_DRAFT_ACTION]);
    }

    /// Plan mode only lists configuration; every configuration change stays out of it.
    /// 计划模式只列出配置；任何配置修改都不在计划模式中执行。
    #[test]
    fn configuration_tools_only_list_in_plan_mode() {
        use crate::tooling::native::{
            CONFIGURATION_LIST_ACTION, build_mcp_config_tool, build_provider_config_tool,
        };
        for tool in [build_mcp_config_tool(), build_provider_config_tool()] {
            let VulcanPlanToolPolicy::StringArgumentAllowlist {
                field,
                allowed_values,
            } = native_plan_policy(&tool.descriptor)
            else {
                panic!(
                    "{} should use an action-scoped Plan policy",
                    tool.descriptor.id
                );
            };
            assert_eq!(field, "action");
            assert_eq!(allowed_values, [CONFIGURATION_LIST_ACTION]);
        }
    }
}
