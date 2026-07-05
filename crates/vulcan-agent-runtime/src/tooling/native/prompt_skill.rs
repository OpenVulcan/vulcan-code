use super::*;

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
pub(crate) struct PromptSkillDetailToolArgs {
    /// Prompt skill name listed in the runtime prompt-skill catalog.
    /// 运行时技能目录中列出的技能名称。
    name: String,
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

/// Samples support files under one prompt-skill directory without reading their contents.
/// 在单个提示词技能目录下采样支持文件路径但不读取文件内容。
pub(crate) fn sample_prompt_skill_support_files(
    base_dir: &Path,
    skill_file: &Path,
) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_prompt_skill_support_files(base_dir, skill_file, base_dir, 0, &mut files);
    files.sort();
    files.truncate(12);
    files
}

/// Recursively collects bounded support-file paths for the prompt-skill detail output.
/// 为提示词技能详情输出递归收集有界支持文件路径。
pub(crate) fn collect_prompt_skill_support_files(
    root: &Path,
    skill_file: &Path,
    current: &Path,
    depth: usize,
    files: &mut Vec<PathBuf>,
) {
    if depth > 2 || files.len() >= 32 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(current) else {
        return;
    };
    for entry in entries.flatten() {
        if files.len() >= 32 {
            return;
        }
        let path = entry.path();
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if file_name.starts_with('.') || file_name.eq_ignore_ascii_case("target") {
            continue;
        }
        if path == skill_file {
            continue;
        }
        if path.is_dir() {
            collect_prompt_skill_support_files(root, skill_file, &path, depth + 1, files);
        } else if path.is_file() {
            files.push(path.strip_prefix(root).unwrap_or(&path).to_path_buf());
        }
    }
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
    let config = PromptSkillsGlobalConfig {
        enabled: true,
        user_skill_root: request.prompt_skill_user_root.clone(),
        system_skill_root: Some(
            vulcan_runtime_assets::resolve_system_prompt_skills_resource_directory(),
        ),
        default_skill_state: PromptSkillState::Enabled,
        skill_overrides: BTreeMap::new(),
    };
    let catalog = vulcan_prompt_skills::load_prompt_skills(&config);
    let resolved = resolve_prompt_skills(
        &catalog,
        &config,
        &PromptSkillsProjectConfig::default(),
        &PromptSkillsSessionConfig::default(),
    );
    let Some(skill) = resolved
        .iter()
        .find(|skill| {
            skill.state == PromptSkillState::Enabled
                && skill.skill.name.eq_ignore_ascii_case(requested_name)
        })
        .map(|resolved| &resolved.skill)
    else {
        let available = resolved
            .iter()
            .filter(|skill| skill.state == PromptSkillState::Enabled)
            .map(|skill| skill.skill.name.as_str())
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

    let sampled_files = sample_prompt_skill_support_files(&skill.base_dir, &skill.skill_file);
    Ok(VulcanToolInvocationResult::from_model_output(
        Value::String(render_skill_tool_output(skill, &sampled_files)),
    ))
}
