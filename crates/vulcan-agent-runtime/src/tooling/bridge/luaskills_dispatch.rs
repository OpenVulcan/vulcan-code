//! LuaSkills package help, implicit dispatch, and host-managed argument injection helpers.
//! LuaSkills 包帮助、隐式调度与宿主管理参数注入辅助逻辑。

use super::*;

/// Builds the AI-visible LuaSkills help tool over the help trees of every enabled package.
/// 基于每个已启用包的帮助树构建 AI 可见的 LuaSkills 帮助工具。
pub(super) fn build_luaskills_help_candidate(
    packages: Vec<LuaSkillPackageHelp>,
) -> VulcanToolCandidate {
    let packages = Arc::new(packages);
    VulcanToolCandidate {
        descriptor: VulcanToolDescriptor::new(
            LUASKILL_HELP_TOOL_NAME,
            LUASKILL_HELP_TOOL_NAME,
            String::from(
                "Read help for enabled LuaSkills packages. {skill} returns the package guide and topic index; {skill, topic} returns one topic; {name} returns the help of one entry by complete canonical name, plus its argument schema when the entry is implicit and called through call_luaskill.",
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
        plan_policy: VulcanPlanToolPolicy::Visible,
        request_streaming: None,
        invoke: Arc::new(move |args, _session_id| {
            let packages = Arc::clone(&packages);
            Box::pin(async move { execute_luaskills_help_tool(packages, args).await })
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
    // Plan mode admits a dispatch exactly when the same entry called directly would be admitted:
    // built-in read-only entries, plus entries whose names read as read-only.
    // 计划模式放行一次调度，当且仅当同一入口被直接调用时也会放行：内置只读入口，以及名称表现为只读的入口。
    let plan_readable_entries = dispatch_entries
        .iter()
        .map(|entry| entry.canonical_name.clone())
        .filter(|name| {
            crate::agent_runtime::tool_permission::builtin_luaskills::is_read_only_entry(name)
                || vulcan_tools_manager::text_looks_read_only(name)
        })
        .collect::<Vec<_>>();
    let dispatch_entries = Arc::new(dispatch_entries);
    VulcanToolCandidate {
        descriptor: VulcanToolDescriptor::new(
            CALL_LUASKILL_TOOL_NAME,
            CALL_LUASKILL_TOOL_NAME,
            String::from(
                "Call one enabled LuaSkills entry by its complete canonical tool name with compact argument pairs. Use this dispatcher for implicit LuaSkills entries that are not registered as direct function tools.",
            ),
            Some(CALL_LUASKILL_USAGE_MD.trim().to_string()),
            VulcanToolOrigin::new(
                VulcanToolSourceKind::Native,
                "native",
                CALL_LUASKILL_TOOL_NAME,
            ),
        ),
        parameters_schema: call_luaskill_args_schema(),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::StringArgumentAllowlist {
            field: String::from("name"),
            allowed_values: plan_readable_entries,
        },
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

/// Executes the LuaSkills help tool over one request-scoped package snapshot.
/// 基于单份请求级包快照执行 LuaSkills 帮助工具。
async fn execute_luaskills_help_tool(
    packages: Arc<Vec<LuaSkillPackageHelp>>,
    args: Value,
) -> Result<VulcanToolInvocationResult, String> {
    let request = parse_luaskill_help_request(&args)?;
    let output = render_luaskill_help_output(&request, &packages)?;
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
