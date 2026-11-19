//! Admits short MCP sessions to isolated workspace, job and terminal runtimes.
//! 将 MCP 短会话接入隔离的工作区、任务和终端运行时。

use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::HashMap, path::Path, sync::Arc, time::Duration};
use tokio::sync::{Mutex, watch};
use uuid::Uuid;
use vulcan_agent_runtime::tooling::native::external;
use vulcan_host_tools::{
    command_session_runtime::CommandSessionRuntime, shell_host::HostCommandShellSupport,
    shell_job::MAX_RUNNING_BACKGROUND_SHELL_JOBS,
};
use vulcan_provider_config::provider_config::{
    McpServiceClient, normalize_mcp_service_configuration,
};

use super::{contexts, workspace_tools};
use crate::SharedServiceBridgeState;

/// Maximum number of retained background results per MCP session.
/// 每个 MCP 会话保留的后台结果数量上限。
const RETAINED_JOBS: usize = 32;
/// Maximum time a client can wait for a job in a single polling request.
/// 客户端在单次轮询请求中等待任务的最长时间。
const MAX_JOB_WAIT_SECONDS: u64 = 30;

/// A single shell task and its cancellation and result channels.
/// 单次 Shell 任务及其取消与结果通道。
struct Job {
    id: String,
    cancel: watch::Sender<bool>,
    result: watch::Receiver<Option<Value>>,
}

/// Session-local operation lifetime and recent job results.
/// 会话本地操作的生命周期及近期任务结果。
struct LiveSession {
    owner: String,
    cancel: watch::Sender<bool>,
    jobs: Vec<Job>,
}

/// Serializes resource admission against retirement of a persistent session.
/// 将资源准入与持久会话注销串行化。
#[derive(Default)]
pub(super) struct ManagedRuntime {
    sessions: Mutex<HashMap<Uuid, LiveSession>>,
    terminals: Arc<CommandSessionRuntime>,
}

/// Returns the external schema whose directory is supplied by the owning MCP session.
/// 返回由所属 MCP 会话提供目录的外部工具参数结构。
pub(super) fn definition(state: &SharedServiceBridgeState, name: &str) -> Option<Value> {
    match name {
        "vc_mcp_project" => return Some(management_schema(name, true)),
        "vc_mcp_session" => return Some(management_schema(name, false)),
        _ => {}
    }
    let shell_support = HostCommandShellSupport::from_configured_default_id(None);
    let mut definition = match name {
        "terminal_session" => {
            json!({"name":name,"description":"Manage a persistent terminal owned by the MCP session.","inputSchema":external::terminal_schema(&shell_support)})
        }
        "shell_job" => json!({
            "name":name, "description":"List, query, wait for, or stop background Shell jobs in the MCP session.",
            "inputSchema":{"type":"object","additionalProperties":false,"required":["action"],"properties":{
                "action":{"type":"string","enum":["list","status","wait","stop"]},
                "job_id":{"type":"string"},
                "wait_seconds":{"type":"integer","minimum":0,"maximum":MAX_JOB_WAIT_SECONDS}
            }}
        }),
        _ => workspace_tools::definition(state, name)?,
    };
    let schema = definition["inputSchema"].as_object_mut()?;
    let properties = schema.get_mut("properties")?.as_object_mut()?;
    let native_file = properties.contains_key("project_id") && properties.contains_key("cwd");
    for key in ["project_id", "cwd", "PWD"] {
        properties.remove(key);
    }
    properties.insert(
        "sid".into(),
        json!({"type":"string","description":"MCP session identifier returned by vc_mcp_session."}),
    );
    if name == "shell_command" {
        // Separate the command deadline from the client's deadline for a single request.
        // 区分命令执行的截止时间与客户端等待单次请求的截止时间。
        properties["timeout_ms"]["description"] = json!(
            "Optional command time limit in milliseconds, including for background jobs. Omit for no command time limit. The client sets its own MCP request deadline."
        );
        properties.insert("background".into(), json!({"type":"boolean","description":"Requires the shell_job grant. Returns a job ID immediately and lets the command continue under the supplied timeout_ms. Query the job with shell_job and the same sid."}));
        definition["description"] = json!(
            "Run a Shell command in the MCP session directory. A foreground call is canceled if its MCP request disconnects. For commands that may outlast the client's request deadline, set background=true before starting and keep the requested timeout_ms. Use shell_job with the same sid to query or wait for the result. Background execution requires the shell_job grant."
        );
    }
    let schema = definition["inputSchema"].as_object_mut()?;
    if native_file {
        schema.remove("oneOf");
    }
    let required = schema
        .entry("required")
        .or_insert_with(|| json!([]))
        .as_array_mut()?;
    required.retain(|key| !matches!(key.as_str(), Some("project_id" | "cwd" | "PWD")));
    required.push(json!("sid"));
    Some(definition)
}

/// Defines the bootstrap actions and documents their required parameters.
/// 定义初始化操作并说明其必需参数。
fn management_schema(name: &str, project: bool) -> Value {
    let description = if project {
        "Manage private MCP projects. New requires an absolute directory and a name. Update sets a note. Remove retires the project and its sessions."
    } else {
        "Manage private MCP sessions. New requires project_id. Update sets a note. Use the returned sid for all standalone tools."
    };
    let mut properties = json!({
        "type":{"type":"string","enum":["new","list","update","remove"]},
        "project_id":{"type":"string"},"note":{"type":"string"}
    });
    if project {
        properties["name"] = json!({"type":"string"});
        properties["path"] = json!({"type":"string","description":"Existing absolute directory."});
    } else {
        properties["sid"] = json!({"type":"string"});
    }
    json!({"name":name,"description":description,"inputSchema":{
        "type":"object","required":["type"],"additionalProperties":false,"properties":properties
    }})
}

impl ManagedRuntime {
    /// Looks up the owner, admits a tool and manages the resources it creates.
    /// 查找所有者、准入工具并管理其创建的资源。
    pub(super) async fn call(
        &self,
        state: &SharedServiceBridgeState,
        root: &Path,
        client: &McpServiceClient,
        name: &str,
        mut arguments: Value,
    ) -> Result<Value, String> {
        let mut sessions = self.sessions.lock().await;
        // Recheck admission after waiting for retirement; an earlier request may hold a revoked token.
        // 等待注销后复查准入，较早请求可能持有已撤销令牌。
        let config = normalize_mcp_service_configuration(
            crate::service_state_access::provider_configuration_from_state(state)?.mcp_service,
        )?;
        if !config.enabled
            || !config.clients.iter().any(|current| {
                current.id == client.id
                    && current.enabled
                    && current.token_sha256 == client.token_sha256
                    && current.tool_ids == client.tool_ids
                    && current.tool_project_ids == client.tool_project_ids
                    && current.knowledge_libraries == client.knowledge_libraries
            })
        {
            return Err(
                "The MCP service or tool grants changed before the call could start.".into(),
            );
        }
        if matches!(name, "vc_mcp_project" | "vc_mcp_session") {
            if name == "vc_mcp_project"
                && arguments["type"] == "new"
                && !client.tool_project_ids.is_empty()
            {
                let path = arguments["path"]
                    .as_str()
                    .ok_or("An existing absolute project directory is required.")?;
                require_directory(state, client, Path::new(path))?;
            }
            let managed = contexts::manage(root, &client.id, name, arguments).await?;
            for uuid in managed.removed {
                sessions.remove(&uuid);
                self.stop_terminal_owner(uuid).await;
            }
            return Ok(managed.output);
        }

        let args = arguments
            .as_object_mut()
            .ok_or("The tool arguments must be an object.")?;
        let sid = args
            .remove("sid")
            .and_then(|value| value.as_str().map(str::to_owned))
            .ok_or("A sid from vc_mcp_session is required.")?;
        if ["cwd", "PWD", "project_id", "session_id"]
            .into_iter()
            .any(|name| args.contains_key(name))
        {
            return Err("The MCP session supplies the workspace and session identity.".into());
        }
        let scope = contexts::binding(root, &client.id, &sid, false)
            .await?
            .ok_or("MCP session not found.")?;
        let live = sessions.entry(scope.uuid).or_insert_with(|| LiveSession {
            owner: client.id.clone(),
            cancel: watch::channel(false).0,
            jobs: Vec::new(),
        });
        if name == "shell_job" {
            let action: JobArguments =
                serde_json::from_value(arguments).map_err(contexts::db_error)?;
            let job = select_job(live, &action)?;
            drop(sessions);
            return job_result(job, &action).await;
        }
        require_directory(state, client, &scope.cwd)?;
        let mut cancelled = live.cancel.subscribe();
        if name == "terminal_session" {
            let starting = arguments["action"] == "start";
            let mut request = external::workspace_request(None, scope.cwd, arguments);
            request.session_id = Some(scope.uuid);
            let shell_support = HostCommandShellSupport::from_configured_default_id(None);
            let invoke =
                external::invoke_terminal(request, Arc::clone(&self.terminals), &shell_support);
            // A start publishes a new process under the retirement lock. I/O waits must release it.
            // 启动在注销锁内登记新进程，等待终端输入输出时必须释放锁。
            let result = if starting {
                invoke.await
            } else {
                drop(sessions);
                tokio::select! {
                    biased;
                    _ = cancelled.changed() => Err("MCP session removed.".into()),
                    result = invoke => result,
                }
            };
            return hide_private_id(result, scope.uuid, &sid);
        }
        if name.starts_with("knowledge_") {
            let project = knowledge_project(state, &scope.cwd)?;
            args.insert("project_id".into(), json!(project));
        } else if name.starts_with("vulcan_codekit_") {
            args.insert("PWD".into(), json!(scope.cwd));
        } else {
            args.insert("cwd".into(), json!(scope.cwd));
        }
        let background = if name == "shell_command" {
            args.remove("background")
                .map(serde_json::from_value::<bool>)
                .transpose()
                .map_err(contexts::db_error)?
                .unwrap_or(false)
        } else {
            false
        };
        if background {
            if !client
                .tool_ids
                .as_ref()
                .is_some_and(|ids| ids.iter().any(|id| id == "shell_job"))
            {
                return Err("Background commands also require the shell_job grant.".into());
            }
            return start_job(
                live,
                state.clone(),
                client.clone(),
                scope.uuid,
                sid,
                arguments,
                cancelled,
            );
        }
        drop(sessions);
        tokio::select! {
            biased;
            _ = cancelled.changed() => Err("MCP session removed.".into()),
            result = workspace_tools::call(state, client, name, arguments, scope.uuid) => hide_private_id(result, scope.uuid, &sid),
        }
    }

    /// Stops terminals after retiring the owning session.
    /// 注销所属会话后关闭终端。
    async fn stop_terminal_owner(&self, uuid: Uuid) {
        for terminal in self
            .terminals
            .try_snapshot_sessions()
            .into_iter()
            .filter(|entry| entry.owner_session_id == uuid)
        {
            if let Err(error) = self
                .terminals
                .stop_session(uuid, terminal.command_session_id, None)
                .await
            {
                tracing::warn!(%error, "Could not stop an MCP terminal after retiring its session");
            }
        }
    }

    /// Stops live resources when the client is revoked.
    /// 客户端撤销时停止其实时资源。
    pub(super) async fn retire_client(&self, client_id: &str) {
        let mut sessions = self.sessions.lock().await;
        let owners: Vec<_> = sessions
            .iter()
            .filter(|(_, entry)| entry.owner == client_id)
            .map(|(id, _)| *id)
            .collect();
        for id in owners {
            sessions.remove(&id);
            self.stop_terminal_owner(id).await;
        }
    }
}

/// Validates the current directory and optional grant to a registered project.
/// 校验当前目录及对登记项目的可选授权。
fn require_directory(
    state: &SharedServiceBridgeState,
    client: &McpServiceClient,
    path: &Path,
) -> Result<(), String> {
    if !path.is_absolute() || !path.is_dir() {
        return Err("The MCP project directory is unavailable.".into());
    }
    if client.tool_project_ids.is_empty() {
        return Ok(());
    }
    let root = path.canonicalize().map_err(contexts::db_error)?;
    let found = workspace_tools::registered_projects(state)?
        .into_iter()
        .any(|project| {
            client.tool_project_ids.contains(&project.id)
                && Path::new(&project.path)
                    .canonicalize()
                    .is_ok_and(|path| root.starts_with(path))
        });
    if found {
        Ok(())
    } else {
        Err("The directory is outside the client's authorized projects.".into())
    }
}

/// Resolves a knowledge binding by the identical registered workspace directory.
/// 按同一登记工作区目录解析知识库绑定。
fn knowledge_project(state: &SharedServiceBridgeState, cwd: &Path) -> Result<String, String> {
    let canonical = cwd.canonicalize().map_err(contexts::db_error)?;
    let mut projects = workspace_tools::registered_projects(state)?
        .into_iter()
        .filter(|project| {
            Path::new(&project.path)
                .canonicalize()
                .is_ok_and(|path| path == canonical)
        });
    let project = projects
        .next()
        .ok_or("Knowledge tools require a knowledge library bound to this workspace.")?;
    if projects.next().is_some() {
        return Err("The MCP workspace matches multiple knowledge projects.".into());
    }
    Ok(project.id)
}

/// Removes the internal UUID from native terminal projections and tool diagnostics.
/// 从原生终端投影和工具诊断中移除内部 UUID。
fn hide_private_id(result: Result<Value, String>, uuid: Uuid, sid: &str) -> Result<Value, String> {
    let uuid = uuid.to_string();
    result
        .map(|mut value| {
            replace_private_id(&mut value, &uuid, sid);
            value
        })
        .map_err(|error| error.replace(&uuid, sid))
}

/// Replaces the private identifier only in string leaves.
/// 只在字符串叶节点中替换私有标识。
fn replace_private_id(value: &mut Value, uuid: &str, sid: &str) {
    match value {
        Value::String(text) => *text = text.replace(uuid, sid),
        Value::Array(items) => items
            .iter_mut()
            .for_each(|value| replace_private_id(value, uuid, sid)),
        Value::Object(fields) => fields
            .values_mut()
            .for_each(|value| replace_private_id(value, uuid, sid)),
        _ => {}
    }
}

/// Starts one bounded background command owned by the admitted session.
/// 启动一个由已准入会话持有的有界后台命令。
fn start_job(
    live: &mut LiveSession,
    state: SharedServiceBridgeState,
    client: McpServiceClient,
    owner: Uuid,
    sid: String,
    arguments: Value,
    mut retired: watch::Receiver<bool>,
) -> Result<Value, String> {
    if live
        .jobs
        .iter()
        .filter(|job| job.result.borrow().is_none())
        .count()
        >= MAX_RUNNING_BACKGROUND_SHELL_JOBS
    {
        return Err("Too many background jobs in this MCP session.".into());
    }
    if live.jobs.len() >= RETAINED_JOBS {
        let index = live
            .jobs
            .iter()
            .position(|job| job.result.borrow().is_some())
            .ok_or("No finished background job is available for eviction.")?;
        live.jobs.remove(index);
    }
    let id = Uuid::new_v4().to_string();
    let (cancel, mut stopped) = watch::channel(false);
    let (sender, result) = watch::channel(None);
    tokio::spawn(async move {
        let output = tokio::select! {
            biased;
            _ = retired.changed() => json!({"status":"cancelled"}),
            _ = stopped.changed() => json!({"status":"cancelled"}),
            result = workspace_tools::call(&state, &client, "shell_command", arguments, owner) => match hide_private_id(result, owner, &sid) {
                Ok(value) => value,
                Err(error) => json!({"status":"failed","error":error}),
            },
        };
        sender.send_replace(Some(output));
    });
    live.jobs.push(Job {
        id: id.clone(),
        cancel,
        result,
    });
    Ok(json!({"job_id":id,"status":"running"}))
}

/// Parsed query for a session-owned background job.
/// 会话持有的后台任务查询参数。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct JobArguments {
    action: JobAction,
    job_id: Option<String>,
    wait_seconds: Option<u64>,
}

/// Supported background job operations.
/// 后台任务支持的操作。
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum JobAction {
    List,
    Status,
    Wait,
    Stop,
}

/// Snapshots a job or list of jobs before releasing the session admission lock.
/// 在释放会话准入锁之前捕获一个任务或任务列表。
enum JobSelection {
    List(Value),
    One {
        id: String,
        result: watch::Receiver<Option<Value>>,
    },
}

/// Applies job ownership and signals stop before returning a result subscription.
/// 检查任务所有权并发出停止信号，再返回结果订阅。
fn select_job(live: &LiveSession, args: &JobArguments) -> Result<JobSelection, String> {
    if matches!(args.action, JobAction::List) {
        let rows: Vec<_> = live
            .jobs
            .iter()
            .rev()
            .map(|job| {
                let result = job.result.borrow();
                let status = result
                    .as_ref()
                    .and_then(|value| value["status"].as_str())
                    .unwrap_or("running");
                json!({"job_id":job.id,"status":status})
            })
            .collect();
        return Ok(JobSelection::List(json!({"jobs":rows})));
    }
    let id = args.job_id.as_deref().ok_or("A job_id is required.")?;
    let job = live
        .jobs
        .iter()
        .find(|job| job.id == id)
        .ok_or("MCP shell job not found.")?;
    if matches!(args.action, JobAction::Stop) {
        job.cancel.send_replace(true);
    }
    Ok(JobSelection::One {
        id: job.id.clone(),
        result: job.result.clone(),
    })
}

/// Returns the current or awaited result after releasing the runtime lock.
/// 释放运行时锁后返回当前结果或等待结果。
async fn job_result(selection: JobSelection, args: &JobArguments) -> Result<Value, String> {
    let (id, mut result) = match selection {
        JobSelection::List(list) => return Ok(list),
        JobSelection::One { id, result } => (id, result),
    };
    if matches!(args.action, JobAction::Wait | JobAction::Stop) && result.borrow().is_none() {
        let seconds = args.wait_seconds.unwrap_or(1).min(MAX_JOB_WAIT_SECONDS);
        let _ = tokio::time::timeout(
            Duration::from_secs(seconds),
            result.wait_for(|value| value.is_some()),
        )
        .await;
    }
    let mut value = result
        .borrow()
        .clone()
        .unwrap_or_else(|| json!({"status":"running"}));
    value["job_id"] = json!(id);
    Ok(value)
}
