//! Projects individually authorized Core and CodeKit tools to the external MCP endpoint.
//! 为对外 MCP 端点投影独立授权的 Core 与 CodeKit 工具。

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use vulcan_agent_runtime::{
    ExecutionKnowledgeAccessContext,
    luaskills_runtime::{
        LuaSkillsRuntimeHandle, build_luaskills_tool_invocation_context,
        call_implicit_luaskills_tool_entry, list_luaskills_registration_snapshot,
        render_luaskills_invocation_result_text,
    },
    tooling::native::external,
};
use vulcan_host_tools::{
    file_runtime::{
        apply_patch_write_paths, path_resolves_inside_workspace, path_targets_git_metadata,
    },
    project_directory::require_bound_project_directory,
    shell_host::HostCommandShellSupport,
};
use vulcan_provider_config::provider_config::McpServiceClient;
use vulcan_workbench_model::TaskProjectSummary;
use vulcan_workspace_store::knowledge_base::KnowledgeBaseRuntime;

use crate::state::SharedServiceBridgeState;

/// Returns the bundled CodeKit entry and its runtime handle for the named tool.
/// 返回指定工具对应的随包 CodeKit 入口和运行时句柄。
fn codekit_entry(
    state: &SharedServiceBridgeState,
    name: &str,
) -> Result<(LuaSkillsRuntimeHandle, luaskills::RuntimeEntryDescriptor), String> {
    let runtime = state
        .lock()
        .map_err(|error| error.to_string())?
        .agent_runtime
        .as_ref()
        .and_then(|agent| agent.luaskills_runtime.clone())
        .ok_or("CodeKit is unavailable.")?;
    let (entries, registrations) = list_luaskills_registration_snapshot(&runtime)?;
    // The dispatch contract requires a guide-backed bundle. Do not advertise legacy entries.
    // 调度要求包声明随包指南，不公告只有旧版清单的入口。
    if !registrations.iter().any(|registration| {
        registration.skill_id == "vulcan-codekit"
            && registration.root_name == "ROOT"
            && registration.declaration.supports_implicit()
    }) {
        return Err("The bundled CodeKit package does not support MCP dispatch.".into());
    }
    let entry = entries
        .into_iter()
        .find(|entry| {
            entry.skill_id == "vulcan-codekit"
                && entry.root_name == "ROOT"
                && entry.public_tool_name() == name
        })
        .ok_or("This bundled CodeKit entry is unavailable.")?;
    Ok((runtime, entry))
}

/// Returns the schema for one supported external tool from its runtime source.
/// 从工具运行时来源返回一个支持的外部工具 Schema。
pub(super) fn definition(state: &SharedServiceBridgeState, name: &str) -> Option<Value> {
    if name.starts_with("vulcan_codekit_") {
        let (_, entry) = codekit_entry(state, name).ok()?;
        let mut schema = entry.input_schema;
        schema["properties"].as_object()?.get("PWD")?;
        schema["properties"]["PWD"]["description"] =
            json!("Required absolute working directory supplied by the MCP client.");
        let required = schema
            .as_object_mut()?
            .entry("required")
            .or_insert_with(|| json!([]))
            .as_array_mut()?;
        if !required.contains(&json!("PWD")) {
            required.push(json!("PWD"));
        }
        return Some(json!({"name":name,"description":entry.description,"inputSchema":schema}));
    }
    if name.starts_with("knowledge_") && state.lock().ok()?.controller_runtime.is_none() {
        return None;
    }
    external::tool_definition(
        name,
        &HostCommandShellSupport::from_configured_default_id(None),
    )
}

/// Checks a source or destination against the call's workspace and protects Git metadata.
/// 按本次调用的工作目录检查来源或目标，同时保护 Git 元数据。
fn check_path(root: &Path, path: &str, writing: bool) -> Result<(), String> {
    // The caller can select a directory inside .git when working without a registered project.
    // 使用无项目限制的目录时，调用方可能传入位于 .git 内的工作目录。
    let protected_root = writing
        && root
            .canonicalize()
            .map_err(|error| error.to_string())?
            .components()
            .any(|component| {
                component
                    .as_os_str()
                    .to_string_lossy()
                    .trim_end_matches(['.', ' '])
                    .eq_ignore_ascii_case(".git")
            });
    if path.trim().is_empty()
        || !path_resolves_inside_workspace(root, path).map_err(|error| error.message)?
        || protected_root
        || (writing && path_targets_git_metadata(root, path).map_err(|error| error.message)?)
    {
        return Err(
            "The requested path is outside the working directory scope or protected.".into(),
        );
    }
    Ok(())
}

/// Reads a required path or identity from an argument object.
/// 从参数对象读取必填路径或身份。
fn field<'a>(input: &'a Value, name: &str) -> Result<&'a str, String> {
    input
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("Field `{name}` is required."))
}

/// Reads the current registered projects when a call explicitly needs a project binding.
/// 调用明确需要项目绑定时，读取当前登记的项目。
pub(super) fn registered_projects(
    state: &SharedServiceBridgeState,
) -> Result<Vec<TaskProjectSummary>, String> {
    let guard = state.lock().map_err(|error| error.to_string())?;
    let workspace = guard
        .workspace_snapshot
        .as_ref()
        .ok_or("The project registry is unavailable.")?;
    Ok(workspace.task_projects.clone())
}

/// Reads the caller-supplied directory and rejects paths that are relative, missing or files.
/// 读取调用方提供的目录，拒绝相对路径、不存在的路径和文件。
fn working_directory(arguments: &Value, field_name: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(field(arguments, field_name)?);
    if !path.is_absolute() || !path.is_dir() {
        return Err(format!(
            "Field `{field_name}` must be an existing absolute directory."
        ));
    }
    Ok(path)
}

/// Chooses the most specific permitted project containing the already canonical directory.
/// 选取包含已规范化目录的最具体授权项目。
fn project_for_directory(
    projects: &[TaskProjectSummary],
    client: &McpServiceClient,
    canonical: &Path,
) -> Result<TaskProjectSummary, String> {
    projects
        .iter()
        .filter(|project| client.tool_project_ids.contains(&project.id))
        .filter_map(|project| {
            std::fs::canonicalize(&project.path)
                .ok()
                .map(|path| (path, project))
        })
        .filter(|(root, _)| canonical.starts_with(root))
        .max_by_key(|(root, _)| root.components().count())
        .map(|(_, project)| project.clone())
        .ok_or_else(|| "The working directory is outside the authorized projects.".into())
}

/// Checks every target that the published CodeKit entry can read or modify.
/// 校验公开 CodeKit 入口可读取或修改的每个目标。
fn check_codekit_paths(
    root: &Path,
    cwd: &Path,
    name: &str,
    arguments: &Value,
) -> Result<(), String> {
    // Resolve relative inputs against the caller's unchanged PWD.
    // 按调用方原样提供的 PWD 解析相对路径。
    let canonical_root = root.canonicalize().map_err(|error| error.to_string())?;
    let check = |value: &str, writing| {
        let value = value.trim();
        if value.is_empty() || value.contains("${env:") {
            return Err("A literal CodeKit path is required.".into());
        }
        let path = cwd.join(value);
        let target = path.canonicalize().map_err(|error| error.to_string())?;
        if !target.starts_with(&canonical_root) {
            return Err("The CodeKit target is outside the authorized project.".into());
        }
        check_path(root, path.to_str().ok_or("Invalid CodeKit path.")?, writing)
    };

    match name {
        "vulcan_codekit_repo_map" | "vulcan_codekit_ast_tree" | "vulcan_codekit_rg" => {
            check(field(arguments, "dir")?, false)?;
        }
        "vulcan_codekit_ast_detail" | "vulcan_codekit_markdown_menu" => {
            let paths = if name == "vulcan_codekit_ast_detail" {
                field(arguments, "paths")?
            } else {
                arguments.get("path").and_then(Value::as_str).unwrap_or(".")
            };
            for path in paths.lines().filter(|line| !line.trim().is_empty()) {
                check(path, false)?;
            }
        }
        "vulcan_codekit_node_source" | "vulcan_codekit_patch" => {
            let writing = name == "vulcan_codekit_patch";
            if writing && arguments.get("file").is_some() {
                check(field(arguments, "file")?, true)?;
            }
            let key = if writing { "patches" } else { "nodes" };
            if let Some(entries) = arguments.get(key).and_then(Value::as_array) {
                for entry in entries {
                    check(field(entry, "file")?, writing)?;
                }
            }
        }
        _ => return Err("Unknown CodeKit tool.".into()),
    }
    Ok(())
}

/// Executes one selected Core or CodeKit tool using the authenticated client's scopes.
/// 使用认证客户端的授权范围执行一个选定的 Core 或 CodeKit 工具。
pub(super) async fn call(
    state: &SharedServiceBridgeState,
    client: &McpServiceClient,
    name: &str,
    mut arguments: Value,
    session_id: uuid::Uuid,
) -> Result<Value, String> {
    let knowledge = name.starts_with("knowledge_");
    let codekit = name.starts_with("vulcan_codekit_");
    let shell = name == "shell_command";
    let restricted = !client.tool_project_ids.is_empty();
    let uses_project_id =
        knowledge || (!codekit && !shell && arguments.get("project_id").is_some());
    let (cwd, project) = if uses_project_id {
        if !knowledge && arguments.get("cwd").is_some() {
            return Err("Provide either `cwd` or `project_id`, not both.".into());
        }
        let id = field(&arguments, "project_id")?.to_owned();
        arguments
            .as_object_mut()
            .ok_or("Tool arguments must be an object.")?
            .remove("project_id");
        let project = registered_projects(state)?
            .into_iter()
            .find(|project| {
                project.id == id
                    && (knowledge || !restricted || client.tool_project_ids.contains(&project.id))
            })
            .ok_or("The requested project was not found or is outside the authorized scope.")?;
        (PathBuf::from(&project.path), Some(project))
    } else {
        let cwd = working_directory(&arguments, if codekit { "PWD" } else { "cwd" })?;
        // Freeze the existing directory for file tools while keeping the original shell and Lua arguments.
        // 固定文件工具的实际目录，同时保留 Shell 和 Lua 的全部原始目录参数。
        let canonical = cwd.canonicalize().map_err(|error| error.to_string())?;
        let project = if restricted {
            Some(project_for_directory(
                &registered_projects(state)?,
                client,
                &canonical,
            )?)
        } else {
            None
        };
        if !shell && !codekit {
            arguments
                .as_object_mut()
                .ok_or("Tool arguments must be an object.")?
                .remove("cwd");
        }
        (canonical, project)
    };
    // Shell holds its own launch lease and releases it immediately after acquiring the child.
    // Shell 使用独立启动租约，子进程建立后立即释放。
    let _directory_lease = match &project {
        Some(project) if !shell => Some(
            crate::project_directories::commit_current_project_directory(state, project)
                .await?
                .ok_or("The project directory changed during tool preparation.")?,
        ),
        _ => None,
    };
    let root = project
        .as_ref()
        .map_or(cwd.as_path(), |project| Path::new(&project.path));
    if let Some(project) = &project
        && (!knowledge || arguments.get("action") == Some(&json!("ingest")))
    {
        require_bound_project_directory(root, project.root_volume_id.as_deref())?;
    }

    if codekit {
        let (runtime, entry) = codekit_entry(state, name)?;
        let canonical_name = entry.canonical_name.clone();
        // An authorized parent project admits the session but does not widen its CodeKit scope.
        // 已授权的父项目允许该会话使用目录，但不会扩大 CodeKit 的访问范围。
        check_codekit_paths(&cwd, Path::new(field(&arguments, "PWD")?), name, &arguments)?;
        let invocation =
            build_luaskills_tool_invocation_context(Some(session_id), "mcp", &canonical_name);
        let result =
            call_implicit_luaskills_tool_entry(runtime.clone(), entry, arguments, Some(invocation))
                .await?;
        return render_luaskills_invocation_result_text(
            &runtime,
            &canonical_name,
            Some("vulcan-codekit"),
            &result,
            None,
        )
        .map(Value::String);
    }

    if knowledge {
        let project = project.ok_or("Knowledge calls require a registered project.")?;
        let controller = state
            .lock()
            .map_err(|error| error.to_string())?
            .controller_runtime
            .clone()
            .ok_or("Knowledge base is unavailable.")?;
        let runtime = KnowledgeBaseRuntime::new(controller.clone());
        let writing = name == "knowledge_write";
        let allowed: Vec<_> = runtime
            .authorized_library_ids(&project.id, false)
            .await?
            .into_iter()
            .filter(|id| {
                client
                    .knowledge_libraries
                    .get(id)
                    .is_some_and(|write| !writing || *write)
            })
            .collect();
        if allowed.is_empty() {
            return Err("No authorized knowledge libraries are available in this project.".into());
        }
        if writing
            && arguments.get("action") == Some(&json!("ingest"))
            && ((restricted && !client.tool_project_ids.contains(&project.id))
                || !client
                    .tool_ids
                    .as_ref()
                    .is_some_and(|ids| ids.iter().any(|id| id == "file_read")))
        {
            return Err(
                "Knowledge ingest requires file_read access to the source workspace.".into(),
            );
        }
        let access = ExecutionKnowledgeAccessContext {
            project_id: project.id.clone(),
            library_ids: allowed,
            search_enabled: true,
        };
        let mut request = external::workspace_request(Some(project.id), cwd, arguments);
        request.session_id = Some(session_id);
        request.knowledge_access = Some(access);
        request.controller_runtime = Some(controller);
        return external::invoke(
            name,
            request,
            &HostCommandShellSupport::from_configured_default_id(None),
            &client.id,
        )
        .await;
    }

    match name {
        "apply_patch" => {
            for path in apply_patch_write_paths(field(&arguments, "patch")?)
                .map_err(|error| error.message)?
            {
                check_path(&cwd, &path, true)?;
            }
        }
        "file" => {
            if !matches!(field(&arguments, "action")?, "list" | "stat") {
                return Err("Use the separately granted file_move or file_delete tool.".into());
            }
            check_path(&cwd, field(&arguments, "path")?, false)?;
        }
        "file_move" => {
            check_path(&cwd, field(&arguments, "path")?, true)?;
            check_path(&cwd, field(&arguments, "destination")?, true)?;
        }
        "file_delete" | "file_write" | "file_edit" | "file_read" => {
            check_path(&cwd, field(&arguments, "path")?, name != "file_read")?;
        }
        "shell_command" => {}
        _ => return Err("Unknown external tool.".into()),
    }
    let native_root = if shell { root.to_path_buf() } else { cwd };
    let mut request = external::workspace_request(
        project.as_ref().map(|project| project.id.clone()),
        native_root,
        arguments,
    );
    if shell && let Some(project) = project {
        request.project_shell_guard = Some(crate::project_directories::project_shell_guard(
            state, project,
        ));
    }
    request.session_id = Some(session_id);
    external::invoke(
        name,
        request,
        &HostCommandShellSupport::from_configured_default_id(None),
        &client.id,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies CodeKit resolves paths from the injected PWD and checks every nested target.
    /// 验证 CodeKit 按注入的 PWD 解析路径，并检查每个嵌套目标。
    #[test]
    fn external_codekit_paths_check_each_batch_target() {
        let area = tempfile::tempdir().expect("fixture directory");
        let root = area.path().join("project");
        let subdir = root.join("src");
        std::fs::create_dir_all(&subdir).expect("project");
        std::fs::write(subdir.join("in.rs"), "").expect("project file");
        std::fs::write(root.join("parent.rs"), "").expect("file in authorized parent project");
        std::fs::write(area.path().join("out.rs"), "").expect("out-of-scope file");
        let inside = json!({"nodes":[{"file":"in.rs","structural_path":"main"}]});
        assert!(
            check_codekit_paths(&subdir, &subdir, "vulcan_codekit_node_source", &inside).is_ok()
        );
        for path in ["../parent.rs", "../../out.rs"] {
            let outside = json!({"patches":[{"file":"in.rs"},{"file":path}]});
            assert!(
                check_codekit_paths(&subdir, &subdir, "vulcan_codekit_patch", &outside).is_err()
            );
        }
    }
}
