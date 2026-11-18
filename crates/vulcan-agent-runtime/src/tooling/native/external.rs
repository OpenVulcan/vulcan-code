//! Definitions and execution shared by the approved external MCP tools.
//! 经批准的对外 MCP 工具所共享的定义与执行入口。

use super::*;

/// Gives external MCP the same terminal schema with the directory supplied by its session.
/// 为外部 MCP 提供共用终端参数结构，工作目录由其会话提供。
pub fn terminal_schema(shell: &HostCommandShellSupport) -> Value {
    terminal::terminal_session_args_schema(shell)
}

/// Runs one external terminal operation against the separately owned terminal runtime.
/// 对独立持有的终端运行时执行一次外部终端操作。
pub async fn invoke_terminal(
    request: NativeToolExecutionRequest,
    runtime: Arc<CommandSessionRuntime>,
    shell: &HostCommandShellSupport,
) -> Result<Value, String> {
    let result = terminal::execute_terminal_session_tool(request, runtime, shell, &[])
        .await
        .map_err(|error| error.to_string())?;
    let payload = result
        .host_result
        .ok_or("The terminal result is missing its control metadata.")?
        .payload;
    if payload["action"] == "list" {
        let terminals = payload["terminal_sessions"]
            .as_array()
            .ok_or("The terminal list result is missing its entries.")?;
        return Ok(
            json!({"terminal_sessions": terminals.iter().map(external_terminal_entry).collect::<Vec<_>>()}),
        );
    }
    let mut output = external_terminal_entry(&payload);
    output["output"] = result.model_output;
    Ok(output)
}

/// Projects only the fields required to identify and control an external terminal.
/// 只投影识别与控制外部终端所需的字段。
fn external_terminal_entry(payload: &Value) -> Value {
    json!({"terminal_session_id":payload["terminal_session_id"], "name":payload["name"],
        "cwd":payload["cwd"], "status":payload["status"], "exit_code":payload["exit_code"]})
}

/// Resolves a supported native tool by external name.
/// 按外部名称解析受支持的原生工具。
fn definition(name: &str) -> Option<Arc<NativeToolDefinition>> {
    Some(match name {
        "file" | "file_move" | "file_delete" => build_file_tool(),
        FILE_READ_TOOL_ID => build_file_read_tool(),
        FILE_WRITE_TOOL_ID => build_file_write_tool(),
        FILE_EDIT_TOOL_ID => build_file_edit_tool(),
        APPLY_PATCH_TOOL_ID => build_apply_patch_tool(),
        KNOWLEDGE_LIST_TOOL_ID => build_knowledge_list_tool(),
        KNOWLEDGE_READ_TOOL_ID => build_knowledge_read_tool(),
        KNOWLEDGE_SEARCH_TOOL_ID => build_knowledge_search_tool(),
        KNOWLEDGE_WRITE_TOOL_ID => build_knowledge_write_tool(),
        _ => return None,
    })
}

/// Projects the selected native tool schema and description for an external caller.
/// 为外部调用方投影所选原生工具的 Schema 和说明。
pub fn tool_definition(name: &str, shell: &HostCommandShellSupport) -> Option<Value> {
    let (description, mut schema) = if name == SHELL_COMMAND_TOOL_ID {
        return Some(json!({
            "name": name,
            "description": "Run a foreground command in the supplied working directory and return its terminal status and bounded output.",
            "inputSchema": shell_command::external_shell_schema(shell),
        }));
    } else {
        let definition = definition(name)?;
        (
            definition.descriptor.description.clone(),
            definition.args_schema.clone(),
        )
    };

    // Tool-specific file actions are authorized independently.
    // 文件动作使用独立授权。
    let description = match name {
        "file" => {
            schema["properties"]["action"]["enum"] = json!(["list", "stat"]);
            "List directory entries or inspect a path in the supplied workspace.".to_owned()
        }
        "file_move" | "file_delete" => {
            schema["properties"].as_object_mut()?.remove("action");
            schema["required"]
                .as_array_mut()?
                .retain(|value| value != "action");
            if name == "file_move" {
                schema["required"]
                    .as_array_mut()?
                    .push(json!("destination"));
                "Move one file within the supplied workspace.".to_owned()
            } else {
                schema["properties"].as_object_mut()?.remove("destination");
                "Delete one regular file in the supplied workspace.".to_owned()
            }
        }
        _ => description,
    };
    schema["properties"]["project_id"] =
        json!({"type":"string","description":"Registered Vulcan Code project ID."});
    if name.starts_with("knowledge_") {
        schema
            .as_object_mut()?
            .entry("required")
            .or_insert_with(|| json!([]))
            .as_array_mut()?
            .push(json!("project_id"));
    } else {
        schema["properties"]["cwd"] = json!({"type":"string","description":"Absolute workspace directory supplied by the caller. Provide either cwd or project_id."});
        schema["oneOf"] = json!([{"required":["cwd"]},{"required":["project_id"]}]);
    }
    Some(json!({"name": name, "description": description, "inputSchema": schema}))
}

/// Builds a request for the supplied root and optional registered project, without a session.
/// 为指定目录和可选的已登记项目构建请求，不附加会话归属。
pub fn workspace_request(
    project_id: Option<String>,
    root: PathBuf,
    arguments: Value,
) -> NativeToolExecutionRequest {
    NativeToolExecutionRequest {
        invocation_context: None,
        arguments,
        session_id: None,
        native_browser_runtime: None,
        host_project_id: project_id,
        workspace_root: Some(root),
        project_shell_guard: None,
        controller_runtime: None,
        goal_runtime: None,
        environment: None,
        session_mcp: None,
        configuration: None,
        luaskills_debug: None,
        feedback: None,
        knowledge_access: None,
        spill_root: None,
        enabled_prompt_skill_names: Vec::new(),
        prepared_prompt_skills: Vec::new(),
        prompt_skill_user_root: None,
        project_vmm: Default::default(),
        session_vmm: Default::default(),
    }
}

/// Invokes a scoped native tool and returns its public result.
/// 执行已确定范围的原生工具并返回公开结果。
pub async fn invoke(
    name: &str,
    mut request: NativeToolExecutionRequest,
    shell: &HostCommandShellSupport,
    client_id: &str,
) -> Result<Value, String> {
    if name == SHELL_COMMAND_TOOL_ID {
        return shell_command::execute_external_shell(request, shell).await;
    }
    // Bind the independently authorized file action without accepting an override.
    // 绑定独立授权的文件动作，拒绝调用方覆盖。
    if let Some(action) = match name {
        "file_move" => Some("move"),
        "file_delete" => Some("delete"),
        _ => None,
    } {
        if request.arguments.get("action").is_some() {
            return Err("This tool has a fixed action.".into());
        }
        request.arguments["action"] = json!(action);
    }
    let result = if name == KNOWLEDGE_WRITE_TOOL_ID {
        knowledge_base::execute_knowledge_write_for_actor(request, Some(client_id)).await
    } else {
        definition(name)
            .ok_or("Unknown external native tool.")?
            .invoke(request)
            .await
    }
    .map_err(|error| error.to_string())?;
    if result.model_output["status"] == "failed"
        && let Some(error) = result.model_output["error"].as_str()
    {
        return Err(error.to_owned());
    }
    Ok(result.model_output)
}
