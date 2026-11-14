//! Vulcan environment variables: the native-tool host, the manager and Ask-panel services, and the
//! global layer published to MCP servers.
//! Vulcan 环境变量：原生工具宿主、管理器与 Ask 面板服务，以及发布给 MCP 服务的全局层。
//!
//! Global and project variables live in the application database; a session's own variables live
//! beside its V1 document, and child agent sessions use their root session's. Values reach child
//! processes only; secret values never reach the model or the WebUI.
//! 全局与项目变量保存在应用数据库中；会话自身的变量保存在其 V1 文档旁，子代理会话使用其根会话的变量。
//! 值只到达子进程；密钥值绝不到达模型或 WebUI。

use std::sync::{Arc, Mutex, Weak};

use serde_json::Value;
use vulcan_agent_runtime::agent_runtime::question_runtime::{
    ENVIRONMENT_DECLINED_LABEL, ENVIRONMENT_SAVED_LABEL, QuestionRuntime,
};
use vulcan_agent_runtime::model::SessionId;
use vulcan_agent_runtime::tooling::native::{
    EffectiveEnvironmentVariable, EnvironmentValuesAnswer, EnvironmentValuesPrompt,
    EnvironmentVariableKind, EnvironmentVariableLayer, NativeEnvironmentHost,
};
use vulcan_service_api::{
    EnvironmentValuesSubmitRequest, EnvironmentVariableEntry, EnvironmentVariableNameCheck,
    EnvironmentVariableRules, EnvironmentVariableSaveRequest, EnvironmentVariablesSnapshot,
    SessionEnvironmentSnapshot, SessionEnvironmentVariableEntry,
};
use vulcan_tools_manager::VulcanToolInvocationContext;
use vulcan_workspace_store::data_plane::HostRuntimeHandle;
use vulcan_workspace_store::workspace_store::{
    ENVIRONMENT_VARIABLE_DESCRIPTION_MAX_CHARS, ENVIRONMENT_VARIABLE_NAME_MAX_CHARS,
    ENVIRONMENT_VARIABLE_VALUE_MAX_BYTES, EnvironmentVariableDraft, EnvironmentVariableRecord,
    EnvironmentVariableScope, RECOMMENDED_ENVIRONMENT_VARIABLE_PREFIX,
    RESERVED_ENVIRONMENT_VARIABLE_NAMES, SessionEnvironmentVariable, create_environment_variable,
    delete_environment_variable, environment_variable_name_key, list_environment_variables,
    load_session_environment, resolve_effective_environment, save_session_environment,
    store_environment_variable_value, update_environment_variable,
    validate_environment_variable_name,
};

use crate::session_runtime_registry_v1::SessionRuntimeRegistry;
use crate::{ServiceBridgeState, SharedServiceBridgeState, TaskRuntime};

/// Serializes read-modify-write cycles of session environment files within this process.
/// 在本进程内串行化会话环境文件的读取-修改-写入周期。
static SESSION_ENVIRONMENT_WRITES: Mutex<()> = Mutex::new(());

/// Serves environment variables to native tools from the database and session files.
/// 从数据库与会话文件向原生工具提供环境变量。
pub(crate) struct KernelEnvironmentHost {
    /// Process session registry; every session that runs tools is registered here.
    /// 进程会话注册表；所有运行工具的会话都在此注册。
    registry: Arc<SessionRuntimeRegistry>,
    /// Shared question runtime whose Ask panel presents value requests.
    /// 以 Ask 面板展示取值请求的共享问题运行时。
    question_runtime: Arc<QuestionRuntime>,
    /// Service state holding the database handle and workspace; weak, because the state owns the
    /// runtime that owns this host.
    /// 持有数据库句柄与工作区的服务状态；使用弱引用，因为该状态拥有持有本宿主的运行时。
    state: Weak<Mutex<ServiceBridgeState>>,
}

impl std::fmt::Debug for KernelEnvironmentHost {
    /// Renders a label without exposing service state.
    /// 渲染不暴露服务状态的标签。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("KernelEnvironmentHost")
            .finish_non_exhaustive()
    }
}

impl KernelEnvironmentHost {
    /// Builds the host over the process session registry and question runtime.
    /// 基于进程会话注册表与问题运行时构建宿主。
    pub(crate) fn new(
        registry: Arc<SessionRuntimeRegistry>,
        question_runtime: Arc<QuestionRuntime>,
        state: Weak<Mutex<ServiceBridgeState>>,
    ) -> Self {
        Self {
            registry,
            question_runtime,
            state,
        }
    }

    /// The root session of `session_id` and its project.
    /// `session_id` 的根会话及其项目。
    fn scope(&self, session_id: SessionId) -> Result<SessionScope, String> {
        let root = crate::tool_permission_host::root_session_of(&self.registry, session_id)?;
        let state = self
            .state
            .upgrade()
            .ok_or_else(|| String::from("The service is shutting down."))?;
        let (_, project_id) = database_and_project(&state, root)?;
        Ok(SessionScope { root, project_id })
    }

    /// Storage locator of one registered session.
    /// 一个已注册会话的存储定位器。
    fn locator(
        &self,
        session_id: SessionId,
    ) -> Result<vulcan_workspace_store::workspace_store::SessionV1StorageLocator, String> {
        crate::session_goal_lifecycle::registered_session_v1_handle(&self.registry, session_id)?
            .storage_locator()
            .map_err(|error| error.to_string())
    }
}

/// Where one session's variables come from.
/// 一个会话的变量来源。
struct SessionScope {
    /// Root user session whose own variables apply.
    /// 其自身变量生效的根用户会话。
    root: SessionId,
    /// Project of the root session, when it belongs to one.
    /// 根会话所属的项目（如有）。
    project_id: Option<String>,
}

#[async_trait::async_trait]
impl NativeEnvironmentHost for KernelEnvironmentHost {
    async fn effective_environment(
        &self,
        session_id: SessionId,
    ) -> Result<Vec<EffectiveEnvironmentVariable>, String> {
        let state = self
            .state
            .upgrade()
            .ok_or_else(|| String::from("The service is shutting down."))?;
        session_effective_environment(&self.registry, &state, session_id).await
    }

    async fn historical_job_secrets(
        &self,
        session_id: SessionId,
    ) -> Result<Vec<(String, String)>, String> {
        let state = self
            .state
            .upgrade()
            .ok_or_else(|| String::from("The service is shutting down."))?;
        historical_job_secrets(&state, &session_id.to_string()).await
    }

    async fn set_session_variable(
        &self,
        session_id: SessionId,
        name: String,
        value: Option<String>,
        description: String,
    ) -> Result<(), String> {
        let scope = self.scope(session_id)?;
        let locator = self.locator(scope.root)?;
        update_session_environment(&locator, |variables| {
            let key = environment_variable_name_key(&name);
            let position = variables
                .iter()
                .position(|variable| environment_variable_name_key(&variable.name) == key);
            if let Some(index) = position
                && variables[index].kind == EnvironmentVariableKind::Secret
            {
                return Err(format!(
                    "The session variable `{name}` is a secret the user entered; only the user can change it."
                ));
            }
            match (value, position) {
                (None, Some(index)) => {
                    variables.remove(index);
                }
                (None, None) => {}
                (Some(value), position) => {
                    let variable = SessionEnvironmentVariable {
                        name,
                        value,
                        kind: EnvironmentVariableKind::Plain,
                        description,
                        updated_at_ms: crate::session_goal_lifecycle::current_v1_goal_unix_millis(
                        )?,
                    };
                    match position {
                        Some(index) => variables[index] = variable,
                        None => variables.push(variable),
                    }
                }
            }
            Ok(())
        })
    }

    async fn request_values(
        &self,
        session_id: SessionId,
        context: VulcanToolInvocationContext,
        tool_name: String,
        arguments: Value,
        mut prompt: EnvironmentValuesPrompt,
    ) -> Result<EnvironmentValuesAnswer, String> {
        let scope = self.scope(session_id)?;
        if scope.root != session_id {
            return Err(String::from(
                "Subagents cannot ask the user for values. Report which variables are missing to the parent agent instead.",
            ));
        }
        let unattended = crate::session_goal_lifecycle::registered_session_v1_handle(
            &self.registry,
            session_id,
        )?
        .read(|document| document.metadata.mode.access.unattended.is_some())
        .map_err(|error| error.to_string())?;
        if unattended {
            return Err(String::from(
                "The session runs unattended, so nobody can enter values now. Continue without them or stop and report what is missing.",
            ));
        }
        prompt.project_available = scope.project_id.is_some();
        self.question_runtime
            .request_environment_values(session_id, context, tool_name, arguments, prompt)
            .await
    }
}

/// Variables that apply to `session_id`: its root session's own, then its project's, then the
/// global ones.
/// 适用于 `session_id` 的变量：先是其根会话自身的，再是其项目的，最后是全局的。
pub(crate) async fn session_effective_environment(
    registry: &SessionRuntimeRegistry,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<Vec<EffectiveEnvironmentVariable>, String> {
    let root = crate::tool_permission_host::root_session_of(registry, session_id)?;
    let locator = crate::session_goal_lifecycle::registered_session_v1_handle(registry, root)?
        .storage_locator()
        .map_err(|error| error.to_string())?;
    effective_environment_at(state, &locator).await
}

/// Resolves the existing environment layers for a root session with a verified storage locator.
/// 为具有已校验存储定位器的根会话解析现有环境变量层。
async fn effective_environment_at(
    state: &SharedServiceBridgeState,
    locator: &vulcan_workspace_store::workspace_store::SessionV1StorageLocator,
) -> Result<Vec<EffectiveEnvironmentVariable>, String> {
    // The indexed locator retains the project even when the workspace view omits the session.
    // 即使工作区视图不包含该会话，已索引的定位器仍保留所属项目。
    let database = database_of(state)?;
    let records = list_environment_variables(&database).await?;
    let session = load_session_environment(locator)?;
    Ok(resolve_effective_environment(
        &records,
        Some(locator.project_id()),
        &session,
    ))
}

/// Reads the database handle and the project of one session from the service state.
/// 从服务状态读取数据库句柄与一个会话的项目。
fn database_and_project(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<(HostRuntimeHandle, Option<String>), String> {
    let guard = state
        .lock()
        .map_err(|error| format!("Failed to lock service state: {error}"))?;
    let database = guard
        .controller_runtime
        .clone()
        .ok_or_else(|| String::from("The service is still starting; storage is not ready."))?;
    let project_id = guard.workspace_snapshot.as_ref().and_then(|workspace| {
        crate::runtime_workspace_helpers::workspace_session_summary_for_session(
            workspace, session_id,
        )
        .and_then(crate::runtime_workspace_helpers::session_summary_project_id_owned)
    });
    Ok((database, project_id))
}

/// Applies `change` to one session's variables under the process write lock.
/// 在进程写锁下对一个会话的变量应用 `change`。
fn update_session_environment(
    locator: &vulcan_workspace_store::workspace_store::SessionV1StorageLocator,
    change: impl FnOnce(&mut Vec<SessionEnvironmentVariable>) -> Result<(), String>,
) -> Result<(), String> {
    let _writing = SESSION_ENVIRONMENT_WRITES
        .lock()
        .map_err(|_| String::from("Session environment writes are locked after a failure."))?;
    let mut variables = load_session_environment(locator)?;
    change(&mut variables)?;
    save_session_environment(locator, &variables)
}

/// Publishes the global variables to MCP servers and registers the global secrets for redaction.
/// Called at startup and after every change.
/// 将全局变量发布给 MCP 服务，并登记全局密钥以便遮蔽。在启动时及每次变更后调用。
pub(crate) async fn publish_global_environment(
    state: &SharedServiceBridgeState,
) -> Result<(), String> {
    let (database, task_runtime) = {
        let guard = state
            .lock()
            .map_err(|error| format!("Failed to lock service state: {error}"))?;
        (
            guard.controller_runtime.clone().ok_or_else(|| {
                String::from("The service is still starting; storage is not ready.")
            })?,
            guard
                .agent_runtime
                .as_ref()
                .map(|runtime| Arc::clone(&runtime.task_runtime)),
        )
    };
    let global = list_environment_variables(&database)
        .await?
        .into_iter()
        .filter(|record| record.scope == EnvironmentVariableScope::Global)
        .collect::<Vec<_>>();
    if let Some(task_runtime) = task_runtime {
        for record in &global {
            if record.kind == EnvironmentVariableKind::Secret {
                task_runtime.register_environment_secret(&record.name, &record.value)?;
            }
        }
    }
    vulcan_agent_runtime::mcp::publish_mcp_environment_overlay(
        global
            .into_iter()
            .map(|record| (record.name, record.value))
            .collect(),
    );
    Ok(())
}

/// Lists the global and project variables with the naming rules.
/// 列出全局与项目变量以及命名规则。
pub(crate) async fn service_environment_variables(
    state: SharedServiceBridgeState,
) -> Result<EnvironmentVariablesSnapshot, String> {
    let database = database_of(&state)?;
    let variables = list_environment_variables(&database).await?;
    Ok(EnvironmentVariablesSnapshot {
        variables: variables.iter().map(entry_from_record).collect(),
        rules: EnvironmentVariableRules {
            recommended_prefix: String::from(RECOMMENDED_ENVIRONMENT_VARIABLE_PREFIX),
            reserved_names: RESERVED_ENVIRONMENT_VARIABLE_NAMES
                .iter()
                .map(|name| String::from(*name))
                .collect(),
            name_max_chars: ENVIRONMENT_VARIABLE_NAME_MAX_CHARS,
            value_max_bytes: ENVIRONMENT_VARIABLE_VALUE_MAX_BYTES,
            description_max_chars: ENVIRONMENT_VARIABLE_DESCRIPTION_MAX_CHARS,
        },
    })
}

/// Creates or updates one variable and republishes the global layer.
/// 创建或更新一条变量，并重新发布全局层。
pub(crate) async fn service_save_environment_variable(
    state: SharedServiceBridgeState,
    request: EnvironmentVariableSaveRequest,
) -> Result<EnvironmentVariablesSnapshot, String> {
    let database = database_of(&state)?;
    let kind = EnvironmentVariableKind::from_key(&request.kind)?;
    let scope = if request.global {
        EnvironmentVariableScope::Global
    } else {
        EnvironmentVariableScope::Projects(request.project_ids)
    };
    match request.id {
        None => {
            let value = request
                .value
                .ok_or_else(|| String::from("A new environment variable needs a value."))?;
            create_environment_variable(
                &database,
                EnvironmentVariableDraft {
                    name: request.name,
                    value,
                    kind,
                    description: request.description,
                    scope,
                },
            )
            .await?;
        }
        Some(id) => {
            let value = match request.value {
                Some(value) => value,
                None => list_environment_variables(&database)
                    .await?
                    .into_iter()
                    .find(|record| record.id == id)
                    .map(|record| record.value)
                    .ok_or_else(|| format!("Environment variable `{id}` does not exist."))?,
            };
            update_environment_variable(
                &database,
                &id,
                EnvironmentVariableDraft {
                    name: request.name,
                    value,
                    kind,
                    description: request.description,
                    scope,
                },
            )
            .await?
            .ok_or_else(|| format!("Environment variable `{id}` does not exist."))?;
        }
    }
    publish_global_environment(&state).await?;
    service_environment_variables(state).await
}

/// Deletes one variable and republishes the global layer.
/// 删除一条变量，并重新发布全局层。
pub(crate) async fn service_delete_environment_variable(
    state: SharedServiceBridgeState,
    id: String,
) -> Result<EnvironmentVariablesSnapshot, String> {
    let database = database_of(&state)?;
    if !delete_environment_variable(&database, &id).await? {
        return Err(format!("Environment variable `{id}` does not exist."));
    }
    publish_global_environment(&state).await?;
    service_environment_variables(state).await
}

/// Checks one name the user is typing: validity, system definition and the recommended prefix.
/// 检查用户正在输入的名称：有效性、系统定义与推荐前缀。
pub(crate) fn service_check_environment_variable_name(
    name: String,
) -> Result<EnvironmentVariableNameCheck, String> {
    let invalid_reason = validate_environment_variable_name(&name).err();
    let trimmed = name.trim();
    Ok(EnvironmentVariableNameCheck {
        defined_by_system: !trimmed.is_empty() && std::env::var_os(trimmed).is_some(),
        has_recommended_prefix: environment_variable_name_key(trimmed)
            .starts_with(RECOMMENDED_ENVIRONMENT_VARIABLE_PREFIX),
        invalid_reason,
        name,
    })
}

/// Lists the variables one session keeps itself (its root session's for a child).
/// 列出一个会话自身保存的变量（子会话则为其根会话的变量）。
pub(crate) async fn service_session_environment(
    state: SharedServiceBridgeState,
    session_id: String,
) -> Result<SessionEnvironmentSnapshot, String> {
    let (root, locator) = session_locator(&state, &session_id).await?;
    let variables = load_session_environment(&locator)?;
    Ok(SessionEnvironmentSnapshot {
        session_id: root.to_string(),
        variables: variables
            .into_iter()
            .map(|variable| SessionEnvironmentVariableEntry {
                value: visible_value(variable.kind, variable.value),
                name: variable.name,
                kind: String::from(variable.kind.key()),
                description: variable.description,
                updated_at_ms: variable.updated_at_ms,
            })
            .collect(),
    })
}

/// Removes one variable a session keeps itself.
/// 删除一个会话自身保存的一个变量。
pub(crate) async fn service_delete_session_environment_variable(
    state: SharedServiceBridgeState,
    session_id: String,
    name: String,
) -> Result<SessionEnvironmentSnapshot, String> {
    let (_, locator) = session_locator(&state, &session_id).await?;
    let key = environment_variable_name_key(&name);
    update_session_environment(&locator, |variables| {
        variables.retain(|variable| environment_variable_name_key(&variable.name) != key);
        Ok(())
    })?;
    service_session_environment(state, session_id).await
}

/// Stores the values the user entered for one pending value request, then resumes the asking
/// `env` call with a fixed label. Values never pass through the question runtime.
/// 存储用户为一条待处理取值请求输入的值，然后以固定标签恢复发起请求的 `env` 调用。值从不经过问题运行时。
pub(crate) async fn service_submit_environment_values(
    state: SharedServiceBridgeState,
    request: EnvironmentValuesSubmitRequest,
) -> Result<bool, String> {
    let question_runtime = {
        let guard = state
            .lock()
            .map_err(|error| format!("Failed to lock service state: {error}"))?;
        guard
            .agent_runtime
            .as_ref()
            .map(|runtime| Arc::clone(&runtime.question_runtime))
            .ok_or_else(|| String::from("The service is still starting; requests are not ready."))?
    };
    let pending = question_runtime
        .list_pending()?
        .into_iter()
        .find(|pending| pending.id == request.request_id)
        .ok_or_else(|| {
            format!(
                "Value request `{}` is no longer active.",
                request.request_id
            )
        })?;
    let prompt = pending.environment.as_ref().ok_or_else(|| {
        format!(
            "Request `{}` does not ask for environment values.",
            request.request_id
        )
    })?;
    if request.declined && !request.values.is_empty() {
        return Err(String::from("A declined value request carries no values."));
    }
    let (database, project_id) = database_and_project(&state, pending.session_id)?;
    let mut global_changed = false;
    let mut session_values = Vec::new();
    for submission in &request.values {
        let key = environment_variable_name_key(&submission.name);
        let requested = prompt
            .variables
            .iter()
            .find(|variable| environment_variable_name_key(&variable.name) == key)
            .ok_or_else(|| format!("`{}` was not requested.", submission.name))?;
        if submission.value.is_empty() {
            continue;
        }
        match layer_from_key(&submission.scope)? {
            EnvironmentVariableLayer::Session => session_values.push(SessionEnvironmentVariable {
                name: requested.name.clone(),
                value: submission.value.clone(),
                kind: requested.kind,
                description: requested.description.clone(),
                updated_at_ms: crate::session_goal_lifecycle::current_v1_goal_unix_millis()?,
            }),
            EnvironmentVariableLayer::Project => {
                let project_id = project_id.as_deref().ok_or_else(|| {
                    String::from("This session has no project; choose the session or global scope.")
                })?;
                store_environment_variable_value(
                    &database,
                    Some(project_id),
                    &requested.name,
                    &submission.value,
                    requested.kind,
                    &requested.description,
                )
                .await?;
            }
            EnvironmentVariableLayer::Global => {
                store_environment_variable_value(
                    &database,
                    None,
                    &requested.name,
                    &submission.value,
                    requested.kind,
                    &requested.description,
                )
                .await?;
                global_changed = true;
            }
        }
    }
    if !session_values.is_empty() {
        let handle =
            crate::session_goal_lifecycle::session_v1_handle_from_state(&state, pending.session_id)
                .await?;
        let locator = handle
            .storage_locator()
            .map_err(|error| error.to_string())?;
        update_session_environment(&locator, |variables| {
            for mut value in session_values {
                let key = environment_variable_name_key(&value.name);
                // A plain request never downgrades a secret session variable.
                // 普通请求绝不降级已有的会话密钥变量。
                if variables.iter().any(|variable| {
                    environment_variable_name_key(&variable.name) == key
                        && variable.kind == EnvironmentVariableKind::Secret
                }) {
                    value.kind = EnvironmentVariableKind::Secret;
                }
                variables.retain(|variable| environment_variable_name_key(&variable.name) != key);
                variables.push(value);
            }
            Ok(())
        })?;
    }
    if global_changed {
        publish_global_environment(&state).await?;
    }
    let label = if request.declined {
        ENVIRONMENT_DECLINED_LABEL
    } else {
        ENVIRONMENT_SAVED_LABEL
    };
    let answered = question_runtime.reply(&request.request_id, vec![vec![String::from(label)]])?;
    if !answered {
        return Err(format!(
            "Value request `{}` is no longer active.",
            request.request_id
        ));
    }
    Ok(true)
}

/// The application database handle.
/// 应用数据库句柄。
fn database_of(state: &SharedServiceBridgeState) -> Result<HostRuntimeHandle, String> {
    state
        .lock()
        .map_err(|error| format!("Failed to lock service state: {error}"))?
        .controller_runtime
        .clone()
        .ok_or_else(|| String::from("The service is still starting; storage is not ready."))
}

/// Resolves one regular session to its root session and that session's storage locator.
/// 将一个普通会话解析为其根会话及该会话的存储定位器。
async fn session_locator(
    state: &SharedServiceBridgeState,
    raw_session_id: &str,
) -> Result<
    (
        SessionId,
        vulcan_workspace_store::workspace_store::SessionV1StorageLocator,
    ),
    String,
> {
    let session_id = SessionId::parse_str(raw_session_id)
        .map_err(|_| String::from("Invalid session identity."))?;
    let handle =
        crate::session_goal_lifecycle::session_v1_handle_from_state(state, session_id).await?;
    let root = handle
        .read(|document| {
            document
                .metadata
                .child_execution_profile
                .as_ref()
                .map(|profile| profile.root_session_id.clone())
        })
        .map_err(|error| error.to_string())?;
    let Some(root) = root else {
        let locator = handle
            .storage_locator()
            .map_err(|error| error.to_string())?;
        return Ok((session_id, locator));
    };
    let root = SessionId::parse_str(&root)
        .map_err(|error| format!("invalid root session `{root}`: {error}"))?;
    let locator = crate::session_goal_lifecycle::session_v1_handle_from_state(state, root)
        .await?
        .storage_locator()
        .map_err(|error| error.to_string())?;
    Ok((root, locator))
}

/// Resolves the environment locator for history without loading a session runtime.
/// 解析历史记录的环境定位器，无需加载会话运行时。
/// Returns the indexed session's locator or its declared root's locator.
/// 返回已索引会话的定位器，子会话则返回其声明的根会话定位器。
async fn historical_job_environment_locator(
    state: &SharedServiceBridgeState,
    raw_session_id: &str,
) -> Result<vulcan_workspace_store::workspace_store::SessionV1StorageLocator, String> {
    // Resolve the same exact owner used to open the log, including registered embedded children.
    // 解析与打开日志相同的精确归属，包含已注册的内嵌子会话。
    let (_, locator, _) =
        crate::conversation_events_v1::resolve_exact_locator_and_registry(state, raw_session_id)
            .await?;
    let root = locator
        .read_environment_root_session_id()
        .map_err(|error| error.to_string())?;
    if let Some(root) = root {
        let (_, root_locator, _) =
            crate::conversation_events_v1::resolve_exact_locator_and_registry(state, &root).await?;
        locator
            .validate_environment_root(&root_locator)
            .map_err(|error| error.to_string())?;
        Ok(root_locator)
    } else {
        Ok(locator)
    }
}

/// Registers the environment that applies to an owned job before stored text is projected.
/// 在投影已存储文本之前，登记适用于该归属任务的环境变量。
pub(crate) async fn register_activity_environment_secrets(
    state: &SharedServiceBridgeState,
    raw_session_id: &str,
    task_runtime: &TaskRuntime,
) -> Result<(), String> {
    for (name, value) in historical_job_secrets(state, raw_session_id).await? {
        task_runtime.register_environment_secret(&name, &value)?;
    }
    Ok(())
}

/// Collects all applicable stored secret values for one job owner, including shadowed names.
/// 收集一个任务归属会话所有适用的已存储密钥值，包括被同名覆盖的值。
/// Returns the name/value pairs only for registration with the output filter.
/// 返回仅用于输出过滤器登记的名称和值。
async fn historical_job_secrets(
    state: &SharedServiceBridgeState,
    raw_session_id: &str,
) -> Result<Vec<(String, String)>, String> {
    let locator = historical_job_environment_locator(state, raw_session_id).await?;
    let database = database_of(state)?;
    let records = list_environment_variables(&database).await?;
    let session = load_session_environment(&locator)?;

    // A retained log can predate the current scope precedence, so filter by scope before any name merging.
    // 保留的日志可能早于当前作用域优先级，因此先按作用域过滤，不合并同名变量。
    let applicable = records
        .into_iter()
        .filter(|record| {
            record.kind == EnvironmentVariableKind::Secret
                && match &record.scope {
                    EnvironmentVariableScope::Global => true,
                    EnvironmentVariableScope::Projects(projects) => projects
                        .iter()
                        .any(|project_id| project_id == locator.project_id()),
                }
        })
        .map(|record| (record.name, record.value));
    let local = session
        .into_iter()
        .filter(|variable| variable.kind == EnvironmentVariableKind::Secret)
        .map(|variable| (variable.name, variable.value));
    Ok(applicable.chain(local).collect())
}

/// Projects one stored variable for the manager; a secret value is never sent.
/// 为管理器投影一条存储变量；密钥值从不发送。
fn entry_from_record(record: &EnvironmentVariableRecord) -> EnvironmentVariableEntry {
    let (global, project_ids) = match &record.scope {
        EnvironmentVariableScope::Global => (true, Vec::new()),
        EnvironmentVariableScope::Projects(project_ids) => (false, project_ids.clone()),
    };
    EnvironmentVariableEntry {
        id: record.id.clone(),
        name: record.name.clone(),
        kind: String::from(record.kind.key()),
        value: visible_value(record.kind, record.value.clone()),
        description: record.description.clone(),
        global,
        project_ids,
        created_at_ms: record.created_at_ms,
        updated_at_ms: record.updated_at_ms,
    }
}

/// The value a client may see: plain values in full, secrets as nothing.
/// 客户端可见的值：普通值完整展示，密钥为空。
fn visible_value(kind: EnvironmentVariableKind, value: String) -> String {
    match kind {
        EnvironmentVariableKind::Plain => value,
        EnvironmentVariableKind::Secret => String::new(),
    }
}

/// Parses one scope key submitted by a client.
/// 解析客户端提交的一个作用域键。
fn layer_from_key(key: &str) -> Result<EnvironmentVariableLayer, String> {
    [
        EnvironmentVariableLayer::Session,
        EnvironmentVariableLayer::Project,
        EnvironmentVariableLayer::Global,
    ]
    .into_iter()
    .find(|layer| layer.key() == key)
    .ok_or_else(|| format!("Unknown environment variable scope `{key}`."))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Secret values never reach a client projection.
    /// 密钥值绝不进入客户端投影。
    #[test]
    fn manager_projection_never_carries_secret_values() {
        let record = EnvironmentVariableRecord {
            id: String::from("id"),
            name: String::from("VULCAN_ENV_TOKEN"),
            value: String::from("secret-value"),
            kind: EnvironmentVariableKind::Secret,
            description: String::new(),
            scope: EnvironmentVariableScope::Global,
            created_at_ms: 1,
            updated_at_ms: 1,
        };
        assert!(entry_from_record(&record).value.is_empty());
        let plain = EnvironmentVariableRecord {
            kind: EnvironmentVariableKind::Plain,
            ..record
        };
        assert_eq!(entry_from_record(&plain).value, "secret-value");
    }

    /// Registers all stored same-name secrets on the first history read after startup.
    /// 重启后首次读取历史记录时，登记全部已存储的同名密钥。
    #[tokio::test]
    async fn historical_job_redaction_keeps_same_name_secrets_from_all_layers() {
        use crate::tests::test_support::{
            TestHostRuntimeRoot, test_agent_runtime_bridge, test_service_state_with_parent_session,
        };
        use vulcan_host_tools::shell_job::{
            SHELL_JOBS_DIRECTORY, ShellJobRuntime, ShellJobSpawn, ShellJobStatus,
        };

        use vulcan_workspace_store::{
            data_plane::build_application_schema_script, local_database_runtime::LogicalDatabase,
            local_sql,
        };

        // Store only synthetic values in an isolated indexed session.
        // 仅在隔离的已索引会话中保存合成值。
        let session_id = SessionId::new_v4();
        let fixture = TestHostRuntimeRoot::new(session_id);
        let controller = fixture.attached_host_runtime_handle().await;
        local_sql::execute_script(
            &controller.local,
            LogicalDatabase::AppState,
            build_application_schema_script(),
            &[],
        )
        .expect("initialize the application schema");
        let locator = fixture.v1_storage_locator();
        save_session_environment(
            &locator,
            &[SessionEnvironmentVariable {
                name: String::from("VULCAN_ENV_FIXTURE"),
                value: String::from("synthetic-history-value"),
                kind: EnvironmentVariableKind::Secret,
                description: String::new(),
                updated_at_ms: 1,
            }],
        )
        .unwrap();

        // Bind a project secret to the authoritative project of the indexed session.
        // 将项目密钥绑定到已索引会话的权威项目。
        let project = vulcan_workbench_model::TaskProjectSummary::new(
            locator.project_id(),
            "Fixture",
            fixture.data_root().to_string_lossy(),
            false,
            0,
            None,
            None,
            Vec::new(),
        );
        vulcan_workspace_store::workspace_store::persist_new_project(controller.clone(), &project)
            .await
            .unwrap();
        create_environment_variable(
            &controller,
            EnvironmentVariableDraft {
                name: String::from("VULCAN_ENV_FIXTURE"),
                value: String::from("synthetic-project-history-value"),
                kind: EnvironmentVariableKind::Secret,
                description: String::new(),
                scope: EnvironmentVariableScope::Projects(vec![locator.project_id().to_owned()]),
            },
        )
        .await
        .unwrap();

        create_environment_variable(
            &controller,
            EnvironmentVariableDraft {
                name: String::from("VULCAN_ENV_FIXTURE"),
                value: String::from("synthetic-global-history-value"),
                kind: EnvironmentVariableKind::Secret,
                description: String::new(),
                scope: EnvironmentVariableScope::Global,
            },
        )
        .await
        .unwrap();

        // Write a finished job into the indexed session using an isolated runtime.
        // 使用独立运行时把已完成任务写入已索引会话。
        let runtime = ShellJobRuntime::new();
        #[cfg(windows)]
        let mut child = {
            let mut command = tokio::process::Command::new("cmd.exe");
            command.args(["/D", "/C", "echo synthetic-history-value synthetic-project-history-value synthetic-global-history-value"]);
            command
        };
        #[cfg(not(windows))]
        let mut child = {
            let mut command = tokio::process::Command::new("printf");
            command.args(["%s\n", "synthetic-history-value synthetic-project-history-value synthetic-global-history-value"]);
            command
        };
        child.current_dir(fixture.data_root());
        let job = runtime
            .spawn(
                ShellJobSpawn {
                    owner_session_id: session_id.to_string(),
                    origin: None,
                    command: String::from("fixture command"),
                    purpose: String::from("fixture job"),
                    shell_label: String::from("fixture"),
                    cwd: fixture.data_root().to_owned(),
                    jobs_dir: locator.session_directory().join(SHELL_JOBS_DIRECTORY),
                    stdin: None,
                    background: false,
                    timeout: Some(std::time::Duration::from_secs(10)),
                    capture_streams: false,
                },
                child,
            )
            .unwrap();
        assert_eq!(job.wait().await.status, ShellJobStatus::Completed);

        // The fresh service knows only the persisted history and its authoritative locator.
        // 此新服务仅通过存储读取历史记录和权威定位器。
        let mut restarted = test_service_state_with_parent_session(session_id);
        restarted.workspace_snapshot = None;
        restarted.controller_runtime = Some(controller);
        restarted.agent_runtime = Some(test_agent_runtime_bridge());
        let restarted = Arc::new(Mutex::new(restarted));
        let page = crate::session_activity_services::service_read_shell_job_output(
            Arc::clone(&restarted),
            vulcan_service_api::ShellJobOutputRequest {
                session_id: session_id.to_string(),
                job_id: job.id().to_owned(),
                before_line: None,
                from_line: None,
                max_lines: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(page.rows.len(), 1);
        assert_eq!(
            page.rows[0].text.trim(),
            "[redacted:VULCAN_ENV_FIXTURE] [redacted:VULCAN_ENV_FIXTURE] [redacted:VULCAN_ENV_FIXTURE]"
        );

        // The model's host port must supply the same values, independently of the WebUI filter state.
        // 模型宿主端口须独立于 WebUI 过滤器状态，提供相同的值。
        let registry = Arc::clone(&restarted.lock().unwrap().session_runtime_registry);
        let host = KernelEnvironmentHost::new(
            registry,
            Arc::new(QuestionRuntime::new()),
            Arc::downgrade(&restarted),
        );
        let model_secrets = host.historical_job_secrets(session_id).await.unwrap();
        for value in [
            "synthetic-history-value",
            "synthetic-project-history-value",
            "synthetic-global-history-value",
        ] {
            assert!(
                model_secrets
                    .iter()
                    .any(|(name, secret)| name == "VULCAN_ENV_FIXTURE" && secret == value)
            );
        }
        let effective = effective_environment_at(&restarted, &locator)
            .await
            .unwrap();
        assert_eq!(effective.len(), 1);
        assert_eq!(effective[0].layer, EnvironmentVariableLayer::Session);
        assert_eq!(effective[0].value, "synthetic-history-value");
    }

    /// Every layer key round-trips and unknown keys are refused.
    /// 每个层键都能往返解析，未知键被拒绝。
    #[test]
    fn scope_keys_round_trip() {
        for layer in [
            EnvironmentVariableLayer::Session,
            EnvironmentVariableLayer::Project,
            EnvironmentVariableLayer::Global,
        ] {
            assert_eq!(layer_from_key(layer.key()), Ok(layer));
        }
        assert!(layer_from_key("system").is_err());
    }
}
