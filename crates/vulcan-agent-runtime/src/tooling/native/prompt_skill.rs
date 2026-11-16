use super::*;

/// Stable model-facing name for the plugin action dispatcher.
/// 插件动作调度入口的稳定模型工具名。
pub(crate) const PLUGIN_CALL_TOOL_ID: &str = "plugin_call";

/// Builds the shared declaration for calling a plugin tool listed by `skill`.
/// 构建调用 `skill` 所列插件工具的共享声明。
/// The returned definition is executed by the bridge to apply the target tool's own policy.
/// 返回的定义由桥接层执行，以便使用目标工具自身的策略。
pub(crate) fn build_plugin_call_tool() -> Arc<NativeToolDefinition> {
    // The bridge resolves the concrete action before approval and dispatch.
    // 桥接层在审批与调度前解析具体动作。
    let descriptor = NativeToolDescriptor {
        id: String::from(PLUGIN_CALL_TOOL_ID),
        name: String::from(PLUGIN_CALL_TOOL_ID),
        description: String::from(
            "Call a plugin action advertised by an enabled prompt skill. First read the skill with `skill` and use the exact action name and argument schema returned by it.",
        ),
        help_text: None,
        ai_visible: true,
        available_in_plan: true,
    };
    build_native_tool_definition(
        descriptor,
        json!({
            "type": "object",
            "properties": {
                "skill": { "type": "string", "description": "Exact prompt skill name." },
                "action": { "type": "string", "description": "Exact action ID returned by skill." },
                "arguments": { "type": "object", "description": "Arguments matching the action input schema." }
            },
            "required": ["skill", "action", "arguments"],
            "additionalProperties": false
        }),
        |_| {
            Box::pin(async {
                Err(NativeToolError::new(
                    "Plugin actions require host dispatch.",
                ))
            })
        },
    )
}

/// Envelope from the model naming one plugin action and its exact argument object.
/// 模型用于指明单个插件动作和精确参数对象的调用封装。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PluginCallArguments {
    /// Namespaced skill name from the enabled skill catalog.
    /// 已启用技能目录中的命名空间技能名。
    pub skill: String,
    /// Stable tool ID advertised by that skill.
    /// 该技能公开的稳定工具 ID。
    pub action: String,
    /// Input to the target tool's own validation and execution path.
    /// 交给目标工具自身校验和执行路径的输入。
    pub arguments: serde_json::Map<String, Value>,
}

/// Builds the AI-visible prompt-skill detail tool.
/// 构建 AI 可见的提示词技能详情工具。
pub(crate) fn build_prompt_skill_detail_tool() -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from("skill"),
        name: String::from("skill"),
        description: String::from(
            "Load the full SKILL.md instructions for one enabled prompt skill by name. Use this before applying a listed prompt skill.",
        ),
        help_text: None,
        ai_visible: true,
        available_in_plan: true,
    };

    build_native_tool_definition(
        descriptor,
        prompt_skill_detail_args_schema(),
        move |request: NativeToolExecutionRequest| {
            Box::pin(async move { execute_prompt_skill_detail_tool(request).await })
        },
    )
}

/// One typed argument payload accepted by the host-owned `skill` native tool.
/// 宿主自有 `skill` 原生工具接受的一份强类型参数载荷。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PromptSkillDetailToolArgs {
    /// Prompt skill name listed in the runtime prompt-skill catalog.
    /// 运行时技能目录中列出的技能名称。
    pub(crate) name: String,
}

/// Builds the JSON Schema advertised to the model for the host-owned `skill` tool.
/// 为宿主自有 `skill` 工具构建对模型声明的 JSON Schema。
pub(crate) fn prompt_skill_detail_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "name": {
                "type": "string",
                "description": "Exact prompt skill name from the enabled skills list."
            }
        },
        "required": ["name"],
        "additionalProperties": false
    })
}

pub(crate) fn parse_prompt_skill_detail_tool_arguments(
    arguments: Value,
) -> Result<PromptSkillDetailToolArgs, NativeToolError> {
    let parsed =
        serde_json::from_value::<PromptSkillDetailToolArgs>(arguments).map_err(|error| {
            NativeToolError::new(format!("Failed to parse skill arguments: {error}"))
        })?;
    if parsed.name.trim().is_empty() {
        return Err(NativeToolError::new(
            "The `name` argument for skill cannot be empty.",
        ));
    }
    Ok(parsed)
}

pub(crate) async fn execute_prompt_skill_detail_tool(
    request: NativeToolExecutionRequest,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let parsed_args = parse_prompt_skill_detail_tool_arguments(request.arguments)?;
    let requested_name = parsed_args.name.trim();
    // The turn-level enabled list is authoritative, so disabled skills cannot be loaded by name.
    // 本回合启用列表是最终约束，因此已关闭技能不能通过名称绕过加载。
    if !request
        .enabled_prompt_skill_names
        .iter()
        .any(|name| name.eq_ignore_ascii_case(requested_name))
    {
        return Err(NativeToolError::new(format!(
            "Prompt skill `{requested_name}` is not enabled for this execution turn."
        )));
    }
    let Some(skill) = request
        .prepared_prompt_skills
        .iter()
        .find(|skill| skill.name.eq_ignore_ascii_case(requested_name))
    else {
        let available = request
            .prepared_prompt_skills
            .iter()
            .map(|skill| skill.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let suffix = if available.is_empty() {
            String::from("No enabled prompt skills were found.")
        } else {
            format!("Enabled prompt skills: {available}.")
        };
        return Err(NativeToolError::new(format!(
            "Prompt skill `{}` is not enabled or was not found. {suffix}",
            parsed_args.name.trim()
        )));
    };

    Ok(VulcanToolInvocationResult::from_model_output(
        Value::String(render_skill_tool_output(
            skill,
            &skill.resources.sampled_files,
        )),
    ))
}
