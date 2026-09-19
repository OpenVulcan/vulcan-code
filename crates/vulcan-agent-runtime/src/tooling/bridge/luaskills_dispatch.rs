//! LuaSkills implicit help, dispatch, and host-managed argument injection helpers.
//! LuaSkills 隐式帮助、调度与宿主管理参数注入辅助逻辑。

use super::*;

/// Builds the AI-visible LuaSkill help tool paired with the implicit dispatcher.
/// 构建与隐式调度器配对的 AI 可见 LuaSkill 帮助工具。
pub(super) fn build_luaskills_help_candidate(
    entries: Vec<LuaSkillPromptEntry>,
) -> VulcanToolCandidate {
    let entries = Arc::new(entries);
    VulcanToolCandidate {
        descriptor: VulcanToolDescriptor::new(
            LUASKILL_HELP_TOOL_NAME,
            LUASKILL_HELP_TOOL_NAME,
            String::from(
                "Return full prompt help and structured Markdown argument schema help for one enabled implicit LuaSkills entry by complete canonical name.",
            ),
            None,
            VulcanToolOrigin::new(
                VulcanToolSourceKind::Native,
                "native",
                LUASKILL_HELP_TOOL_NAME,
            ),
        ),
        parameters_schema: luaskill_help_args_schema(),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::HeuristicText(String::from(
            "luaskill implicit LuaSkills help and Markdown schema",
        )),
        request_streaming: None,
        invoke: Arc::new(move |args, _session_id| {
            let entries = Arc::clone(&entries);
            Box::pin(async move { execute_luaskills_help_tool(entries, args).await })
        }),
    }
}

/// Builds the AI-visible LuaSkills dispatcher used for host-side implicit entry calls.
/// 构建 AI 可见的 LuaSkills 调度器，用于宿主侧隐式入口调用。
pub(super) fn build_luaskills_dispatch_candidate(
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    workspace_root: Option<PathBuf>,
    mut dispatch_entries: Vec<RuntimeEntryDescriptor>,
) -> VulcanToolCandidate {
    dispatch_entries.sort_by(|left, right| {
        left.skill_id
            .cmp(&right.skill_id)
            .then_with(|| left.local_name.cmp(&right.local_name))
            .then_with(|| left.canonical_name.cmp(&right.canonical_name))
    });
    let dispatch_entries = Arc::new(dispatch_entries);
    VulcanToolCandidate {
        descriptor: VulcanToolDescriptor::new(
            CALL_LUASKILL_TOOL_NAME,
            CALL_LUASKILL_TOOL_NAME,
            String::from(
                "Call one enabled LuaSkills entry by its complete canonical tool name with compact argument pairs. Use this dispatcher for implicit LuaSkills entries that are not registered as direct function tools.",
            ),
            None,
            VulcanToolOrigin::new(
                VulcanToolSourceKind::Native,
                "native",
                CALL_LUASKILL_TOOL_NAME,
            ),
        ),
        parameters_schema: call_luaskill_args_schema(),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::HeuristicText(String::from(
            "call_luaskill LuaSkills implicit entry dispatcher",
        )),
        request_streaming: None,
        invoke: Arc::new(move |args, context| {
            let runtime = luaskills_runtime.clone();
            let workspace_root = workspace_root.clone();
            let dispatch_entries = Arc::clone(&dispatch_entries);
            Box::pin(async move {
                execute_luaskills_dispatch_tool(
                    runtime,
                    workspace_root,
                    dispatch_entries,
                    args,
                    Some(context.session_id),
                )
                .await
            })
        }),
    }
}

/// Builds the compact JSON schema accepted by the `luaskill` help tool.
/// 构建 `luaskill` 帮助工具接受的紧凑 JSON Schema。
fn luaskill_help_args_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "name": {
                "type": "string",
                "description": "Complete canonical LuaSkills tool name, such as vulcan-codekit-ast-tree."
            }
        },
        "required": ["name"]
    })
}

/// Executes the LuaSkill help lookup tool from one implicit dispatch snapshot.
/// 基于单份隐式调度快照执行 LuaSkill 帮助查询工具。
async fn execute_luaskills_help_tool(
    entries: Arc<Vec<LuaSkillPromptEntry>>,
    args: Value,
) -> Result<VulcanToolInvocationResult, String> {
    let Value::Object(mut object) = args else {
        return Err(String::from("`luaskill` arguments must be a JSON object."));
    };
    let name = object
        .remove("name")
        .and_then(|value| value.as_str().map(str::trim).map(str::to_string))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| String::from("`luaskill.name` must be a non-empty string."))?;
    let output = render_luaskill_help_output(&name, &entries)?;
    Ok(VulcanToolInvocationResult::from_model_output(
        Value::String(output),
    ))
}

/// Executes one LuaSkills entry through the compact host-side dispatcher.
/// 通过紧凑的宿主侧调度器执行单个 LuaSkills 入口。
async fn execute_luaskills_dispatch_tool(
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    workspace_root: Option<PathBuf>,
    dispatch_entries: Arc<Vec<RuntimeEntryDescriptor>>,
    args: Value,
    session_id: Option<String>,
) -> Result<VulcanToolInvocationResult, String> {
    let Some(handle) = luaskills_runtime else {
        return Err(String::from(
            "The LuaSkills runtime is currently unavailable, so `call_luaskill` cannot invoke entries.",
        ));
    };
    let request = parse_call_luaskill_request(args)?;
    let entry_views = dispatch_entries
        .iter()
        .map(luaskills_entry_to_call_view)
        .collect::<Vec<_>>();
    let Some(entry_view) = resolve_call_luaskill_entry_by_name(&entry_views, &request.name) else {
        return Err(format!(
            "LuaSkills entry `{}` is not available for host dispatch. It may be disabled, not installed, or not enabled in the current runtime.",
            request.name
        ));
    };
    let Some(entry) = dispatch_entries
        .iter()
        .find(|entry| entry.canonical_name == entry_view.canonical_name)
    else {
        return Err(format!(
            "LuaSkills entry `{}` disappeared from the dispatch snapshot.",
            request.name
        ));
    };

    let request_args = restore_call_luaskill_args_for_entry(request.args, entry_view)?;
    let entry_declares_luaskill_sid = entry_declares_luaskill_sid(entry);
    let entry_declares_pwd = entry_declares_pwd(entry);
    let call_args = inject_host_managed_luaskill_sid_if_needed(
        request_args,
        entry_declares_luaskill_sid,
        session_id.as_deref(),
        &entry.canonical_name,
    )?;
    let call_args = inject_host_managed_pwd_if_needed(
        call_args,
        entry_declares_pwd,
        workspace_root.as_deref(),
        &entry.canonical_name,
    )?;
    let parsed_session_id = session_id
        .as_deref()
        .and_then(|value| Uuid::parse_str(value).ok());
    let invocation_context = build_luaskills_tool_invocation_context(
        parsed_session_id,
        "call-luaskill",
        &entry.canonical_name,
    );
    let result = call_luaskills_tool_entry(
        handle.clone(),
        entry.canonical_name.clone(),
        call_args,
        Some(invocation_context),
    )
    .await
    .map_err(|error| {
        format!(
            "LuaSkills entry `{}` in skill `{}` failed: {}.",
            entry.local_name, entry.skill_id, error
        )
    })?;
    let model_output = Value::String(render_luaskills_invocation_result_text(
        &handle,
        &entry.canonical_name,
        Some(&entry.skill_id),
        &result,
        workspace_root.as_deref(),
    )?);
    let invocation_result = if let Some(host_result) = result.host_result.as_ref() {
        VulcanToolInvocationResult::from_model_output(model_output).with_host_result(
            VulcanToolHostResult {
                kind: host_result.kind.clone(),
                payload: host_result.payload.clone(),
            },
        )
    } else {
        VulcanToolInvocationResult::from_model_output(model_output)
    };
    Ok(invocation_result)
}

/// Converts one LuaSkills runtime entry into the shared dispatcher entry view.
/// 将单个 LuaSkills 运行时入口转换为共享调度器入口视图。
pub(super) fn luaskills_entry_to_call_view(
    entry: &RuntimeEntryDescriptor,
) -> LuaSkillCallEntryView<'_> {
    LuaSkillCallEntryView {
        canonical_name: &entry.canonical_name,
        input_schema: &entry.input_schema,
    }
}

/// Inject one host-managed LuaSkills SID when the entry declares it and the caller did not provide it.
/// 当入口声明该字段且调用方未提供时，注入一份宿主管理 LuaSkills SID。
pub(super) fn inject_host_managed_luaskill_sid_if_needed(
    args: Value,
    entry_declares_luaskill_sid: bool,
    session_id: Option<&str>,
    tool_name: &str,
) -> Result<Value, String> {
    if !entry_declares_luaskill_sid {
        return Ok(args);
    }

    let stable_session_id = session_id.ok_or_else(|| {
        format!(
            "LuaSkills tool `{tool_name}` requires a host-managed LUASKILL_SID, but the current session identifier is unavailable."
        )
    })?;

    let mut object = match args {
        Value::Object(map) => map,
        _ => {
            return Err(format!(
                "LuaSkills tool `{tool_name}` expects a JSON object so a host-managed LUASKILL_SID can be injected."
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

/// Inject one host-managed workspace path when the entry declares `PWD`, the host can resolve a current project directory, and the caller did not provide one.
/// 当入口声明 `PWD`、宿主能够解析当前项目目录且调用方未提供时，注入一份宿主管理工作区路径。
pub(super) fn inject_host_managed_pwd_if_needed(
    args: Value,
    entry_declares_pwd: bool,
    workspace_root: Option<&std::path::Path>,
    tool_name: &str,
) -> Result<Value, String> {
    if !entry_declares_pwd {
        return Ok(args);
    }

    let Some(workspace_root) = workspace_root.filter(|path| !path.as_os_str().is_empty()) else {
        return Ok(args);
    };

    let mut object = match args {
        Value::Object(map) => map,
        _ => {
            return Err(format!(
                "LuaSkills tool `{tool_name}` expects a JSON object so a host-managed PWD can be injected."
            ));
        }
    };

    if !object.contains_key(PWD_PARAMETER_NAME) {
        object.insert(
            String::from(PWD_PARAMETER_NAME),
            Value::String(native_workspace_path_text_from_path(workspace_root)),
        );
    }

    Ok(Value::Object(object))
}
