//! Vulcan environment variables for native tools: the host port, the access handle that command
//! tools use to build child environments, and the model-visible `env` tool.
//! 原生工具使用的 Vulcan 环境变量：宿主端口、命令类工具构建子进程环境所用的访问句柄，以及模型可见的
//! `env` 工具。

use super::*;
use crate::agent_runtime::task_runtime::TaskRuntime;
use std::collections::BTreeSet;
use std::sync::Weak;
use vulcan_tools_manager::VulcanToolInvocationContext;
pub use vulcan_workspace_store::workspace_store::{
    EffectiveEnvironmentVariable, EnvironmentVariableKind, EnvironmentVariableLayer,
};
use vulcan_workspace_store::workspace_store::{
    RECOMMENDED_ENVIRONMENT_VARIABLE_PREFIX, environment_variable_name_key,
    normalized_environment_variable_description, validate_environment_variable_name,
};

/// Stable identifier of the environment-variable tool.
/// 环境变量工具的稳定标识。
pub const ENVIRONMENT_TOOL_ID: &str = "env";

/// Shortest secret value replaced in tool results; shorter values would mangle ordinary output.
/// 工具结果中会被替换的最短密钥值；更短的值会破坏普通输出。
pub const ENVIRONMENT_SECRET_REDACTION_MIN_CHARS: usize = 4;

/// Most variables one value request may ask the user for.
/// 单次取值请求最多向用户索取的变量数。
pub const ENVIRONMENT_VALUE_REQUEST_MAX_VARIABLES: usize = 10;

/// One variable the AI asks the user to fill.
/// AI 请用户填写的一个变量。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentValueRequest {
    /// Variable name.
    /// 变量名。
    pub name: String,
    /// What the value is, shown to the user.
    /// 该值是什么，向用户展示。
    pub description: String,
    /// Whether the value is a secret.
    /// 该值是否为密钥。
    pub kind: EnvironmentVariableKind,
    /// Scope the AI suggests; the user may choose another.
    /// AI 建议的作用域；用户可以另选。
    pub layer: EnvironmentVariableLayer,
}

/// One request for values, presented in the Ask panel.
/// 一次取值请求，在 Ask 面板中展示。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentValuesPrompt {
    /// Why the values are needed, shown to the user.
    /// 需要这些值的原因，向用户展示。
    pub purpose: String,
    /// Requested variables in the AI's order.
    /// 按 AI 顺序排列的请求变量。
    pub variables: Vec<EnvironmentValueRequest>,
    /// Whether the session belongs to a project; set by the host that presents the request.
    /// 会话是否属于项目；由展示该请求的宿主设置。
    pub project_available: bool,
}

/// How one value request ended.
/// 一次取值请求的结束方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvironmentValuesAnswer {
    /// The user saved values; which ones are read back from the store.
    /// 用户保存了值；具体哪些从存储中读回。
    Saved,
    /// The user declined to provide values.
    /// 用户拒绝提供值。
    Declined,
}

/// Host-injected environment port used by command tools and the `env` tool.
/// 命令类工具与 `env` 工具使用的宿主注入环境变量端口。
#[async_trait::async_trait]
pub trait NativeEnvironmentHost: fmt::Debug + Send + Sync {
    /// Variables that apply to one session after scope precedence, in name order. Child sessions
    /// see their root session's variables.
    /// 经作用域优先级处理后对一个会话生效的变量，按名称排序。子会话看到其根会话的变量。
    async fn effective_environment(
        &self,
        session_id: SessionId,
    ) -> Result<Vec<EffectiveEnvironmentVariable>, String>;

    /// Sets or, with `value` absent, removes one plain variable of the session (the root session
    /// for a child). Secret session variables belong to the user and are refused.
    /// 设置会话（子会话则为其根会话）的一个普通变量；`value` 缺省时删除。会话密钥变量属于用户，一律拒绝。
    async fn set_session_variable(
        &self,
        session_id: SessionId,
        name: String,
        value: Option<String>,
        description: String,
    ) -> Result<(), String>;

    /// Asks the user for values in the Ask panel and waits; values go straight to storage and
    /// never come back through this call.
    /// 在 Ask 面板中向用户索取值并等待；值直接写入存储，绝不经由本调用返回。
    async fn request_values(
        &self,
        session_id: SessionId,
        context: VulcanToolInvocationContext,
        tool_name: String,
        arguments: Value,
        prompt: EnvironmentValuesPrompt,
    ) -> Result<EnvironmentValuesAnswer, String>;
}

/// Environment access handed to one native tool call: the host plus the redaction registry that
/// every secret is entered into before its value can reach a child process.
/// 交给一次原生工具调用的环境变量访问：宿主，以及每个密钥在其值到达子进程之前登记的遮蔽注册表。
#[derive(Clone)]
pub struct NativeEnvironmentAccess {
    /// Service-owned environment host.
    /// 服务持有的环境变量宿主。
    host: Arc<dyn NativeEnvironmentHost>,
    /// Task runtime owning the redaction registry; weak, because the runtime owns the host slot.
    /// 持有遮蔽注册表的任务运行时；使用弱引用，因为运行时持有宿主槽位。
    task_runtime: Weak<TaskRuntime>,
}

impl fmt::Debug for NativeEnvironmentAccess {
    /// Renders a label without exposing the host or registry.
    /// 渲染不暴露宿主与注册表的标签。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeEnvironmentAccess")
            .finish_non_exhaustive()
    }
}

impl NativeEnvironmentAccess {
    /// Pairs one host with the runtime whose registry redacts its secrets.
    /// 将一个宿主与遮蔽其密钥的运行时配对。
    pub(crate) fn new(
        host: Arc<dyn NativeEnvironmentHost>,
        task_runtime: Weak<TaskRuntime>,
    ) -> Self {
        Self { host, task_runtime }
    }

    /// Effective variables of one session; every secret is registered for redaction first.
    /// 一个会话的生效变量；每个密钥都会先登记遮蔽。
    pub(crate) async fn effective(
        &self,
        session_id: SessionId,
    ) -> Result<Vec<EffectiveEnvironmentVariable>, String> {
        let variables = self.host.effective_environment(session_id).await?;
        let task_runtime = self
            .task_runtime
            .upgrade()
            .ok_or_else(|| String::from("The task runtime is shutting down."))?;
        for variable in &variables {
            if variable.kind == EnvironmentVariableKind::Secret {
                task_runtime.register_environment_secret(&variable.name, &variable.value)?;
            }
        }
        Ok(variables)
    }

    /// The host itself.
    /// 宿主本身。
    pub(crate) fn host(&self) -> &Arc<dyn NativeEnvironmentHost> {
        &self.host
    }
}

/// Name and value pairs injected into one child process started by a native tool; empty when the
/// call has no session or no environment host.
/// 注入原生工具所启动子进程的名称与值；调用没有会话或没有环境变量宿主时为空。
pub(crate) async fn child_process_environment(
    request: &NativeToolExecutionRequest,
) -> Result<Vec<(String, String)>, NativeToolError> {
    let (Some(access), Some(session_id)) = (request.environment.as_ref(), request.session_id)
    else {
        return Ok(Vec::new());
    };
    let variables = access.effective(session_id).await.map_err(|error| {
        NativeToolError::new(format!(
            "Failed to read the Vulcan environment variables: {error}"
        ))
    })?;
    Ok(variables
        .into_iter()
        .map(|variable| (variable.name, variable.value))
        .collect())
}

/// Stable fingerprint of one injected environment, used to tell whether a running terminal was
/// started with the variables that apply now. It never contains the values themselves.
/// An empty environment has an empty fingerprint.
/// 一份注入环境的稳定指纹，用于判断运行中的终端启动时使用的变量是否仍是当前生效的变量。指纹本身绝不包含值。
/// 空环境的指纹为空。
pub(crate) fn environment_fingerprint(environment: &[(String, String)]) -> String {
    use sha2::{Digest, Sha256};
    if environment.is_empty() {
        return String::new();
    }
    let mut digest = Sha256::new();
    for (name, value) in environment {
        digest.update((name.len() as u64).to_le_bytes());
        digest.update(name.as_bytes());
        digest.update((value.len() as u64).to_le_bytes());
        digest.update(value.as_bytes());
    }
    hex::encode(digest.finalize())
}

/// Builds the model-visible `env` tool.
/// 构建模型可见的 `env` 工具。
pub(crate) fn build_environment_tool() -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from(ENVIRONMENT_TOOL_ID),
        name: String::from(ENVIRONMENT_TOOL_ID),
        description: environment_tool_description(),
        help_text: None,
        ai_visible: true,
        available_in_plan: true,
    };
    build_native_tool_definition(
        descriptor,
        environment_tool_args_schema(),
        move |request: NativeToolExecutionRequest| {
            Box::pin(async move { execute_environment_tool(request).await })
        },
    )
}

/// AI-facing description of the `env` tool.
/// `env` 工具面向 AI 的描述。
fn environment_tool_description() -> String {
    format!(
        "Manage Vulcan environment variables: user-defined values injected into every process that `shell_command`, `terminal_session` (at start) and MCP servers launch. Read them with the shell's own syntax (`$NAME`, `$env:NAME`, `%NAME%`); never paste values into commands.\n\
- `list`: variables that apply to this session (session over project over global; system variables are not listed). Plain values are shown; secrets show only their name.\n\
- `set_session`: set a plain variable for this session only, or remove it by omitting `value`. Use it for non-secret settings you need across commands.\n\
- `request`: ask the user to fill one or more values (API keys, tokens, endpoints). The user types them in Vulcan Code; values never return to you. After `saved`, use them by name.\n\
Prefer names starting with `{RECOMMENDED_ENVIRONMENT_VARIABLE_PREFIX}`. Never ask the user to paste secrets into the chat; use `request`."
    )
}

/// JSON Schema of the `env` tool arguments.
/// `env` 工具参数的 JSON Schema。
fn environment_tool_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": ["list", "set_session", "request"],
                "description": "Operation to perform."
            },
            "name": {
                "type": "string",
                "description": "set_session: variable name (letters, digits and `_`, not starting with a digit)."
            },
            "value": {
                "type": "string",
                "description": "set_session: new value; omit to remove the session variable."
            },
            "description": {
                "type": "string",
                "description": "set_session: what the variable is for."
            },
            "purpose": {
                "type": "string",
                "description": "request: why the values are needed, shown to the user."
            },
            "variables": {
                "type": "array",
                "minItems": 1,
                "maxItems": ENVIRONMENT_VALUE_REQUEST_MAX_VARIABLES,
                "description": "request: variables for the user to fill.",
                "items": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "Variable name." },
                        "description": { "type": "string", "description": "What the value is, shown to the user." },
                        "secret": { "type": "boolean", "description": "Whether the value is a secret such as an API key. Defaults to true." },
                        "scope": {
                            "type": "string",
                            "enum": ["session", "project", "global"],
                            "description": "Suggested scope; the user may choose another. Defaults to project."
                        }
                    },
                    "required": ["name", "description"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["action"],
        "additionalProperties": false
    })
}

/// Arguments accepted by the `env` tool.
/// `env` 工具接受的参数。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EnvironmentToolArgs {
    action: EnvironmentToolAction,
    name: Option<String>,
    value: Option<String>,
    description: Option<String>,
    purpose: Option<String>,
    variables: Option<Vec<EnvironmentRequestArg>>,
}

/// One action of the `env` tool.
/// `env` 工具的一种动作。
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum EnvironmentToolAction {
    List,
    SetSession,
    Request,
}

/// One requested variable in the `env` tool arguments.
/// `env` 工具参数中的一个请求变量。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EnvironmentRequestArg {
    name: String,
    description: String,
    secret: Option<bool>,
    scope: Option<EnvironmentVariableLayer>,
}

/// Executes one `env` call.
/// 执行一次 `env` 调用。
async fn execute_environment_tool(
    request: NativeToolExecutionRequest,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let args: EnvironmentToolArgs = serde_json::from_value(request.arguments.clone())
        .map_err(|error| NativeToolError::new(format!("Invalid `env` arguments: {error}")))?;
    let access = request.environment.clone().ok_or_else(|| {
        NativeToolError::new("Vulcan environment variables are not available in this run.")
    })?;
    let session_id = request
        .session_id
        .ok_or_else(|| NativeToolError::new("The `env` tool needs a session."))?;
    match args.action {
        EnvironmentToolAction::List => {
            let variables = access
                .effective(session_id)
                .await
                .map_err(NativeToolError::new)?;
            Ok(environment_list_result(&variables))
        }
        EnvironmentToolAction::SetSession => {
            let name = args
                .name
                .ok_or_else(|| NativeToolError::new("`set_session` requires `name`."))?;
            let name = validate_environment_variable_name(&name).map_err(NativeToolError::new)?;
            let removing = args.value.is_none();
            access
                .host()
                .set_session_variable(
                    session_id,
                    name.clone(),
                    args.value,
                    args.description.unwrap_or_default(),
                )
                .await
                .map_err(NativeToolError::new)?;
            let text = if removing {
                format!(
                    "Removed the session variable `{name}`. Processes started from now on no longer receive it from this session."
                )
            } else {
                format!(
                    "Set the session variable `{name}`. Processes started from now on receive it; running terminals keep the environment they started with."
                )
            };
            Ok(environment_text_result(
                text,
                json!({ "action": "set_session", "name": name, "removed": removing }),
            ))
        }
        EnvironmentToolAction::Request => {
            let prompt = environment_values_prompt(args.purpose, args.variables)?;
            let context = request.invocation_context.clone().ok_or_else(|| {
                NativeToolError::new("`request` needs a tool invocation identity.")
            })?;
            let answer = access
                .host()
                .request_values(
                    session_id,
                    context,
                    String::from(ENVIRONMENT_TOOL_ID),
                    request.arguments.clone(),
                    prompt.clone(),
                )
                .await
                .map_err(NativeToolError::new)?;
            if answer == EnvironmentValuesAnswer::Declined {
                return Ok(environment_text_result(
                    String::from(
                        "The user declined to provide the values. Do not ask again unless the user brings it up.",
                    ),
                    json!({ "action": "request", "answer": "declined" }),
                ));
            }
            let variables = access
                .effective(session_id)
                .await
                .map_err(NativeToolError::new)?;
            Ok(environment_request_result(&prompt, &variables))
        }
    }
}

/// Validates the `request` arguments into one prompt.
/// 将 `request` 参数校验为一个提示。
fn environment_values_prompt(
    purpose: Option<String>,
    variables: Option<Vec<EnvironmentRequestArg>>,
) -> Result<EnvironmentValuesPrompt, NativeToolError> {
    let purpose = purpose.as_deref().map(str::trim).unwrap_or_default();
    if purpose.is_empty() {
        return Err(NativeToolError::new(
            "`request` requires a non-empty `purpose`.",
        ));
    }
    let purpose =
        normalized_environment_variable_description(purpose).map_err(NativeToolError::new)?;
    let variables = variables.unwrap_or_default();
    if variables.is_empty() || variables.len() > ENVIRONMENT_VALUE_REQUEST_MAX_VARIABLES {
        return Err(NativeToolError::new(format!(
            "`request` asks for 1 to {ENVIRONMENT_VALUE_REQUEST_MAX_VARIABLES} variables."
        )));
    }
    let mut seen = BTreeSet::new();
    let mut requested = Vec::with_capacity(variables.len());
    for variable in variables {
        let name =
            validate_environment_variable_name(&variable.name).map_err(NativeToolError::new)?;
        if !seen.insert(environment_variable_name_key(&name)) {
            return Err(NativeToolError::new(format!(
                "`{name}` is requested twice."
            )));
        }
        requested.push(EnvironmentValueRequest {
            name,
            description: normalized_environment_variable_description(&variable.description)
                .map_err(NativeToolError::new)?,
            kind: if variable.secret.unwrap_or(true) {
                EnvironmentVariableKind::Secret
            } else {
                EnvironmentVariableKind::Plain
            },
            layer: variable.scope.unwrap_or(EnvironmentVariableLayer::Project),
        });
    }
    Ok(EnvironmentValuesPrompt {
        purpose,
        variables: requested,
        project_available: false,
    })
}

/// Renders the `list` result: plain values in full, secrets by name only.
/// 渲染 `list` 结果：普通值完整展示，密钥只展示名称。
fn environment_list_result(
    variables: &[EffectiveEnvironmentVariable],
) -> VulcanToolInvocationResult {
    let mut lines = vec![format!("Vulcan environment variables: {}", variables.len())];
    if variables.is_empty() {
        lines.push(String::from("(none)"));
    }
    let mut rows = Vec::with_capacity(variables.len());
    for variable in variables {
        let description = if variable.description.is_empty() {
            String::new()
        } else {
            format!(" - {}", variable.description)
        };
        match variable.kind {
            EnvironmentVariableKind::Plain => lines.push(format!(
                "- {} ({}) = {}{description}",
                variable.name,
                variable.layer.key(),
                variable.value
            )),
            EnvironmentVariableKind::Secret => lines.push(format!(
                "- {} ({}, secret, set){description}",
                variable.name,
                variable.layer.key()
            )),
        }
        rows.push(json!({
            "name": variable.name,
            "scope": variable.layer.key(),
            "secret": variable.kind == EnvironmentVariableKind::Secret,
            "description": variable.description,
        }));
    }
    environment_text_result(
        lines.join("\n"),
        json!({ "action": "list", "variables": rows }),
    )
}

/// Renders the `request` result: which requested names now have a value, and from which scope.
/// 渲染 `request` 结果：哪些请求名称现在有值，以及来自哪个作用域。
fn environment_request_result(
    prompt: &EnvironmentValuesPrompt,
    variables: &[EffectiveEnvironmentVariable],
) -> VulcanToolInvocationResult {
    let mut lines = vec![String::from("The user answered the value request:")];
    let mut rows = Vec::with_capacity(prompt.variables.len());
    for requested in &prompt.variables {
        let key = environment_variable_name_key(&requested.name);
        let found = variables
            .iter()
            .find(|variable| environment_variable_name_key(&variable.name) == key);
        match found {
            Some(variable) => lines.push(format!(
                "- {}: saved ({})",
                requested.name,
                variable.layer.key()
            )),
            None => lines.push(format!("- {}: not provided", requested.name)),
        }
        rows.push(json!({
            "name": requested.name,
            "saved": found.is_some(),
            "scope": found.map(|variable| variable.layer.key()),
        }));
    }
    lines.push(String::from(
        "Saved values are injected into processes started from now on; use them by name.",
    ));
    environment_text_result(
        lines.join("\n"),
        json!({ "action": "request", "answer": "saved", "variables": rows }),
    )
}

/// Wraps one text result with its host payload.
/// 为一条文本结果附上宿主载荷。
fn environment_text_result(text: String, payload: Value) -> VulcanToolInvocationResult {
    VulcanToolInvocationResult::from_model_output(Value::String(text)).with_host_result(
        VulcanToolHostResult {
            kind: String::from(ENVIRONMENT_TOOL_ID),
            payload,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Requests need a purpose and 1..=max distinct valid names; secret and project are defaults.
    /// 请求需要目的说明与 1 到上限个互不相同的有效名称；默认是密钥与项目作用域。
    #[test]
    fn value_requests_are_validated() {
        let arg = |name: &str| EnvironmentRequestArg {
            name: name.to_string(),
            description: String::from("d"),
            secret: None,
            scope: None,
        };
        let prompt =
            environment_values_prompt(Some(String::from("p")), Some(vec![arg("A")])).unwrap();
        assert_eq!(prompt.variables[0].kind, EnvironmentVariableKind::Secret);
        assert_eq!(prompt.variables[0].layer, EnvironmentVariableLayer::Project);
        assert!(environment_values_prompt(None, Some(vec![arg("A")])).is_err());
        assert!(environment_values_prompt(Some(String::from("p")), Some(Vec::new())).is_err());
        assert!(
            environment_values_prompt(Some(String::from("p")), Some(vec![arg("A"), arg("a")]))
                .is_err()
        );
        assert!(
            environment_values_prompt(Some(String::from("p")), Some(vec![arg("VULCAN_HOME")]))
                .is_err()
        );
        let too_many = (0..=ENVIRONMENT_VALUE_REQUEST_MAX_VARIABLES)
            .map(|index| arg(&format!("V{index}")))
            .collect();
        assert!(environment_values_prompt(Some(String::from("p")), Some(too_many)).is_err());
    }

    /// The list shows plain values but never a secret value.
    /// 列表展示普通值，但绝不展示密钥值。
    #[test]
    fn list_never_shows_secret_values() {
        let variables = vec![
            EffectiveEnvironmentVariable {
                name: String::from("PLAIN"),
                value: String::from("visible-value"),
                kind: EnvironmentVariableKind::Plain,
                description: String::new(),
                layer: EnvironmentVariableLayer::Global,
            },
            EffectiveEnvironmentVariable {
                name: String::from("TOKEN"),
                value: String::from("hidden-value"),
                kind: EnvironmentVariableKind::Secret,
                description: String::new(),
                layer: EnvironmentVariableLayer::Session,
            },
        ];
        let result = environment_list_result(&variables);
        let text = result.model_output.to_string();
        let payload = result
            .host_result
            .map(|host| host.payload.to_string())
            .unwrap_or_default();
        assert!(text.contains("visible-value"));
        assert!(!text.contains("hidden-value") && !payload.contains("hidden-value"));
    }

    /// The fingerprint changes with any name or value and never contains a value.
    /// 指纹随任一名称或值变化，且绝不包含值。
    #[test]
    fn fingerprint_tracks_names_and_values() {
        let base = vec![(String::from("A"), String::from("secret-value"))];
        let fingerprint = environment_fingerprint(&base);
        assert!(!fingerprint.contains("secret-value"));
        assert_ne!(
            fingerprint,
            environment_fingerprint(&[(String::from("A"), String::from("other"))])
        );
        assert_ne!(
            fingerprint,
            environment_fingerprint(&[(String::from("B"), String::from("secret-value"))])
        );
        assert_ne!(fingerprint, environment_fingerprint(&[]));
    }
}
