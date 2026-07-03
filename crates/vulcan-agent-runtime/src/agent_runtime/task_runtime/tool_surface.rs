use super::*;

/// Default wait_agent timeout in milliseconds for the Vulcan core subagent tool.
/// Vulcan 核心子代理工具的 `wait_agent` 默认等待毫秒数。
pub(super) const WAIT_AGENT_DEFAULT_TIMEOUT_MS: u64 = 30_000;

/// Minimum wait_agent timeout in milliseconds used to guard against tight polling.
/// 用于防止密集轮询的 `wait_agent` 最小等待毫秒数。
pub(super) const WAIT_AGENT_MIN_TIMEOUT_MS: u64 = 10_000;

/// Maximum wait_agent timeout in milliseconds for the Vulcan core subagent tool.
/// Vulcan 核心子代理工具的 `wait_agent` 最大等待毫秒数。
pub(super) const WAIT_AGENT_MAX_TIMEOUT_MS: u64 = 3_600_000;

/// Builds the core child-agent tool surface backed by the shared subagent runtime.
/// 构建由共享 TaskSession 运行时承载的 会话代理风格子代理工具面。
pub fn build_subagent_tool_candidates(task_runtime: Arc<TaskRuntime>) -> Vec<VulcanToolCandidate> {
    vec![
        build_spawn_agent_tool_candidate(Arc::clone(&task_runtime)),
        build_send_message_tool_candidate(Arc::clone(&task_runtime)),
        build_followup_task_tool_candidate(Arc::clone(&task_runtime)),
        build_wait_agent_tool_candidate(Arc::clone(&task_runtime)),
        build_interrupt_agent_tool_candidate(Arc::clone(&task_runtime)),
        build_list_agents_tool_candidate(task_runtime),
    ]
}

/// Builds the model-visible non-blocking child-agent spawn tool.
/// 构建模型可见的非阻塞子代理启动工具。
pub(super) fn build_spawn_agent_tool_candidate(
    task_runtime: Arc<TaskRuntime>,
) -> VulcanToolCandidate {
    let description = spawn_agent_tool_description(&task_runtime);
    VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: String::from(SPAWN_AGENT_TOOL),
            name: String::from(SPAWN_AGENT_TOOL),
            description,
            help_text: None,
            source: VulcanToolSourceKind::Native,
        },
        parameters_schema: spawn_agent_args_schema(),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Hidden,
        invoke: Arc::new(move |args, session_id| {
            let task_runtime = Arc::clone(&task_runtime);
            Box::pin(async move {
                let trace_started_at = current_unix_millis_u64();
                let session_id =
                    parse_subagent_tool_session_id(session_id.as_deref(), SPAWN_AGENT_TOOL)?;
                task_runtime_trace(
                    "subagent_spawn_agent_start",
                    format!(
                        "parent={} args_chars={}",
                        session_id,
                        args.to_string().chars().count()
                    ),
                );
                let task_args = task_tool_args_from_spawn_agent_args(args)?;
                let result = task_runtime.invoke_spawn_agent(session_id, task_args).await;
                task_runtime_trace(
                    "subagent_spawn_agent_return",
                    format!(
                        "parent={} ok={} elapsed_ms={}",
                        session_id,
                        result.is_ok(),
                        current_unix_millis_u64().saturating_sub(trace_started_at)
                    ),
                );
                result
            })
        }),
    }
}

/// Builds the spawn_agent description with currently enabled expert catalog hints.
/// 使用当前已启用专家目录提示构建 spawn_agent 描述。
fn spawn_agent_tool_description(task_runtime: &TaskRuntime) -> String {
    let base = "Spawns an agent to work on the specified task. If your current task is `/root/task1` and you spawn_agent with task_name \"task_3\" the agent will have canonical task name `/root/task1/task_3`. You are then able to refer to this agent as `task_3` or `/root/task1/task_3` interchangeably. Only call this tool for a concrete, bounded subtask that can run independently alongside useful local work; otherwise continue locally. Ordinary Vulcan child agents do not receive subagent orchestration tools by default.";
    match enabled_expert_catalog_hint(task_runtime) {
        Some(hint) => format!("{base}\n\n{hint}"),
        None => base.to_string(),
    }
}

/// Builds a compact model-visible list of enabled expert agent ids.
/// 构建模型可见的紧凑已启用专家智能体标识列表。
fn enabled_expert_catalog_hint(task_runtime: &TaskRuntime) -> Option<String> {
    let catalog = task_runtime.subagent_catalog_snapshot().ok()?;
    let mut enabled_experts = catalog
        .definitions
        .iter()
        .filter(|definition| {
            definition.enabled
                && definition.mode == vulcan_task_session::SubagentDefinitionMode::Expert
        })
        .map(|definition| {
            let description = definition
                .description
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("No description provided.");
            format!(
                "- `{}`: {} - {}",
                definition.id, definition.name, description
            )
        })
        .collect::<Vec<_>>();
    if enabled_experts.is_empty() {
        return None;
    }
    let total_count = enabled_experts.len();
    enabled_experts.truncate(24);
    if total_count > enabled_experts.len() {
        enabled_experts.push(format!(
            "- ... {} additional enabled experts omitted from the tool description.",
            total_count - enabled_experts.len()
        ));
    }
    Some(format!(
        "Enabled catalog-backed experts for `agent_mode: \"expert\"` and `agent_id`:\n{}",
        enabled_experts.join("\n")
    ))
}

/// Builds the model-visible child-agent queue-only message tool.
/// 构建模型可见的子代理仅入队消息工具。
pub(super) fn build_send_message_tool_candidate(
    task_runtime: Arc<TaskRuntime>,
) -> VulcanToolCandidate {
    VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: String::from(SEND_MESSAGE_TOOL),
            name: String::from(SEND_MESSAGE_TOOL),
            description: String::from(
                "Send a message to an existing agent. The message will be delivered promptly. Does not trigger a new turn.",
            ),
            help_text: None,
            source: VulcanToolSourceKind::Native,
        },
        parameters_schema: inter_agent_message_args_schema(),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Hidden,
        invoke: Arc::new(move |args, session_id| {
            let task_runtime = Arc::clone(&task_runtime);
            Box::pin(async move {
                let session_id =
                    parse_subagent_tool_session_id(session_id.as_deref(), SEND_MESSAGE_TOOL)?;
                task_runtime.invoke_send_message(session_id, args).await
            })
        }),
    }
}

/// Builds the model-visible child-agent follow-up message tool.
/// 构建模型可见的子代理后续消息工具。
pub(super) fn build_followup_task_tool_candidate(
    task_runtime: Arc<TaskRuntime>,
) -> VulcanToolCandidate {
    VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: String::from(FOLLOWUP_TASK_TOOL),
            name: String::from(FOLLOWUP_TASK_TOOL),
            description: String::from(
                "Send a follow-up task to an existing non-root target agent and trigger a turn if it is idle. If the target is already running, deliver the task promptly at message boundaries while sampling, or after the pending tool call completes.",
            ),
            help_text: None,
            source: VulcanToolSourceKind::Native,
        },
        parameters_schema: inter_agent_message_args_schema(),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Hidden,
        invoke: Arc::new(move |args, session_id| {
            let task_runtime = Arc::clone(&task_runtime);
            Box::pin(async move {
                let session_id =
                    parse_subagent_tool_session_id(session_id.as_deref(), FOLLOWUP_TASK_TOOL)?;
                task_runtime.invoke_followup_task(session_id, args).await
            })
        }),
    }
}

/// Builds the model-visible child-agent wait tool.
/// 构建模型可见的子代理等待工具。
pub(super) fn build_wait_agent_tool_candidate(
    task_runtime: Arc<TaskRuntime>,
) -> VulcanToolCandidate {
    VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: String::from(WAIT_AGENT_TOOL),
            name: String::from(WAIT_AGENT_TOOL),
            description: String::from(
                "Wait for a mailbox update from any live agent, including queued messages and final-status notifications. The wait also ends early when new user input is steered into the active turn. Does not return the content; returns either a summary of which agents have updates (if any), an interruption summary for steered input, or a timeout summary if no activity arrives before the deadline.",
            ),
            help_text: None,
            source: VulcanToolSourceKind::Native,
        },
        parameters_schema: wait_agent_args_schema(),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Hidden,
        invoke: Arc::new(move |args, session_id| {
            let task_runtime = Arc::clone(&task_runtime);
            Box::pin(async move {
                let session_id =
                    parse_subagent_tool_session_id(session_id.as_deref(), WAIT_AGENT_TOOL)?;
                task_runtime.invoke_wait_agent(session_id, args).await
            })
        }),
    }
}

/// Builds the model-visible child-agent listing tool.
/// 构建模型可见的子代理列表工具。
pub(super) fn build_list_agents_tool_candidate(
    task_runtime: Arc<TaskRuntime>,
) -> VulcanToolCandidate {
    VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: String::from(LIST_AGENTS_TOOL),
            name: String::from(LIST_AGENTS_TOOL),
            description: String::from(
                "List known child agents and their current status. This tool does not collect child-agent results and does not consume mailbox updates; use wait_agent to wait for activity, then continue normally so pending inter-agent messages can enter the next model request.",
            ),
            help_text: None,
            source: VulcanToolSourceKind::Native,
        },
        parameters_schema: list_agents_args_schema(),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Hidden,
        invoke: Arc::new(move |args, session_id| {
            let task_runtime = Arc::clone(&task_runtime);
            Box::pin(async move {
                let session_id =
                    parse_subagent_tool_session_id(session_id.as_deref(), LIST_AGENTS_TOOL)?;
                task_runtime.invoke_list_agents(session_id, args).await
            })
        }),
    }
}

/// Builds the model-visible child-agent interrupt tool.
/// 构建模型可见的子代理中断工具。
pub(super) fn build_interrupt_agent_tool_candidate(
    task_runtime: Arc<TaskRuntime>,
) -> VulcanToolCandidate {
    VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: String::from(INTERRUPT_AGENT_TOOL),
            name: String::from(INTERRUPT_AGENT_TOOL),
            description: String::from(
                "Interrupt an agent's current turn, if any, and return its previous status. The agent remains available for messages and follow-up tasks.",
            ),
            help_text: None,
            source: VulcanToolSourceKind::Native,
        },
        parameters_schema: interrupt_agent_args_schema(),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Hidden,
        invoke: Arc::new(move |args, session_id| {
            let task_runtime = Arc::clone(&task_runtime);
            Box::pin(async move {
                let session_id =
                    parse_subagent_tool_session_id(session_id.as_deref(), INTERRUPT_AGENT_TOOL)?;
                task_runtime.invoke_interrupt_agent(session_id, args).await
            })
        }),
    }
}

/// Builds the JSON Schema advertised to the model for `spawn_agent`.
/// 为模型端 `spawn_agent` 工具构建对外声明的 JSON Schema。
pub(super) fn spawn_agent_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "message": {
                "type": "string",
                "description": "Initial plain-text task for the new agent."
            },
            "agent_type": {
                "type": "string",
                "description": "Optional agent type or role hint for the new agent."
            },
            "agent_mode": {
                "type": "string",
                "enum": ["default", "mirror", "expert"],
                "description": "Optional explicit child-agent mode. Use `expert` with agent_id for a catalog-backed expert, `mirror` for a mirror child agent, or `default` for an ordinary child agent."
            },
            "agent_id": {
                "type": "string",
                "description": "Optional string id of a catalog-backed subagent definition. Prefer the canonical category path such as `engineering/code-reviewer`; an unambiguous legacy file stem such as `code-reviewer` may also resolve. This is not a numeric id."
            },
            "fork_turns": {
                "type": "string",
                "description": "Optional number of turns to fork. Defaults to `all` for default and mirror agents. Expert agents always start from `none` so catalog roles do not inherit parent conversation turns. Use `none`, `all`, or a positive integer string such as `3` to fork only the most recent turns."
            },
            "model": {
                "type": "string",
                "description": "Model override for the new agent. Omit unless an explicit override is needed."
            },
            "reasoning_effort": {
                "type": "string",
                "description": "Reasoning effort override for the new agent. Omit to inherit the parent effort."
            },
            "task_name": {
                "type": "string",
                "description": "Task name for the new agent. Use lowercase letters, digits, and underscores."
            }
        },
        "required": ["task_name", "message"],
        "additionalProperties": false
    })
}

/// Builds the JSON Schema advertised to the model for inter-agent message tools.
/// 为模型端代理间消息工具构建对外声明的 JSON Schema。
pub(super) fn inter_agent_message_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "target": {
                "type": "string",
                "description": "Relative or canonical task name to message (from spawn_agent)."
            },
            "message": {
                "type": "string",
                "description": "Message text to send to the target agent."
            }
        },
        "required": ["target", "message"],
        "additionalProperties": false
    })
}

/// Builds the JSON Schema advertised to the model for `wait_agent`.
/// 为模型端 `wait_agent` 工具构建对外声明的 JSON Schema。
pub(super) fn wait_agent_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "timeout_ms": {
                "type": "integer",
                "minimum": WAIT_AGENT_MIN_TIMEOUT_MS,
                "maximum": WAIT_AGENT_MAX_TIMEOUT_MS,
                "description": "Timeout in milliseconds. Defaults to 30000, min 10000, max 3600000."
            }
        },
        "additionalProperties": false
    })
}

/// Builds the JSON Schema advertised to the model for `list_agents`.
/// 为模型端 `list_agents` 工具构建对外声明的 JSON Schema。
pub(super) fn list_agents_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "path_prefix": {
                "type": "string",
                "description": "Optional Vulcan task-path prefix filter without a trailing slash."
            },
            "include_all": {
                "type": "boolean",
                "description": "Deprecated compatibility flag. It is accepted but ignored; list_agents always returns status only and never consumes mailbox updates."
            }
        },
        "additionalProperties": false
    })
}

/// Builds the JSON Schema advertised to the model for `interrupt_agent`.
/// 为模型端 `interrupt_agent` 工具构建对外声明的 JSON Schema。
pub(super) fn interrupt_agent_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "target": {
                "type": "string",
                "description": "Agent id or canonical task name to interrupt (from spawn_agent)."
            }
        },
        "required": ["target"],
        "additionalProperties": false
    })
}

/// Reads the required interrupt target from an interrupt_agent request.
/// 从 interrupt_agent 请求中读取必填中断目标。
pub(super) fn interrupt_agent_target(args: &Value) -> Result<String, String> {
    let Value::Object(map) = args else {
        return Err(String::from(
            "interrupt_agent arguments must be a JSON object.",
        ));
    };
    let unsupported_keys = map
        .keys()
        .filter(|key| key.as_str() != "target")
        .cloned()
        .collect::<Vec<_>>();
    if !unsupported_keys.is_empty() {
        return Err(format!(
            "interrupt_agent received unsupported keys: {}",
            unsupported_keys.join(", ")
        ));
    }
    string_field(map, &["target"]).ok_or_else(|| {
        String::from("interrupt_agent requires target with the child-agent task name.")
    })
}

/// Parses the session identifier required by every session-agent-style subagent tool.
/// 解析每个 会话代理风格子代理工具都需要的会话标识。
pub(super) fn parse_subagent_tool_session_id(
    session_id: Option<&str>,
    tool_name: &str,
) -> Result<SessionId, String> {
    let Some(session_id) = session_id else {
        return Err(format!(
            "The `{tool_name}` tool requires a session identifier and cannot run without it."
        ));
    };
    Uuid::parse_str(session_id).map_err(|error| {
        format!("Failed to parse the session identifier for `{tool_name}`: {error}")
    })
}

/// Converts `spawn_agent` arguments into the internal subagent invocation shape.
/// 将 `spawn_agent` 参数转换为内部子代理调用形态。
pub(super) fn task_tool_args_from_spawn_agent_args(args: Value) -> Result<Value, String> {
    let Value::Object(map) = args else {
        return Err(String::from(
            "`spawn_agent` arguments must be a JSON object.",
        ));
    };
    let prompt = string_field(&map, &["message"]).ok_or_else(|| {
        String::from("`spawn_agent` requires `message` with the child-agent instruction.")
    })?;
    let task_id = string_field(&map, &["task_name"]).ok_or_else(|| {
        String::from("`spawn_agent` requires `task_name` with the child-agent task name.")
    })?;
    let unsupported_keys = map
        .keys()
        .filter(|key| {
            !matches!(
                key.as_str(),
                "message"
                    | "task_name"
                    | "agent_type"
                    | "agent_mode"
                    | "agent_id"
                    | "fork_turns"
                    | "model"
                    | "reasoning_effort"
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    if !unsupported_keys.is_empty() {
        return Err(format!(
            "`spawn_agent` only accepts message, task_name, agent_type, agent_mode, agent_id, fork_turns, model, and reasoning_effort; unsupported keys: {}",
            unsupported_keys.join(", ")
        ));
    }
    let agent_type = string_field(&map, &["agent_type"]);
    let agent_mode = normalize_subagent_mode(string_field(&map, &["agent_mode"]).as_deref())?;
    let agent_id = string_field(&map, &["agent_id"]);
    if agent_mode.as_deref() == Some(SUBAGENT_MODE_EXPERT) && agent_id.is_none() {
        return Err(String::from(
            "`spawn_agent` requires `agent_id` when agent_mode is `expert`.",
        ));
    }
    let model = string_field(&map, &["model"]);
    let reasoning_effort = string_field(&map, &["reasoning_effort"]);
    let fork_turns = effective_spawn_agent_fork_turns(
        agent_mode.as_deref(),
        agent_id.as_deref(),
        string_field(&map, &["fork_turns"]).as_deref(),
    )?;
    let mut task_args = serde_json::Map::new();
    task_args.insert(String::from("prompt"), Value::String(prompt));
    task_args.insert(String::from("task_id"), Value::String(task_id.clone()));
    task_args.insert(String::from("description"), Value::String(task_id));
    if let Some(agent_type) = agent_type {
        task_args.insert(String::from("subagent"), Value::String(agent_type));
    }
    if let Some(agent_mode) = agent_mode {
        task_args.insert(String::from("agent_mode"), Value::String(agent_mode));
    }
    if let Some(agent_id) = agent_id {
        task_args.insert(String::from("subagent_profile_id"), Value::String(agent_id));
    }
    if let Some(model) = model {
        task_args.insert(String::from("model"), Value::String(model));
    }
    if let Some(reasoning_effort) = reasoning_effort {
        task_args.insert(
            String::from("reasoning_effort"),
            Value::String(reasoning_effort),
        );
    }
    task_args.insert(String::from("fork_turns"), Value::String(fork_turns));
    Ok(Value::Object(task_args))
}

/// Resolves the effective fork selector for a model-visible spawn request.
/// 解析模型可见 spawn 请求的有效上下文派生选择器。
///
/// # Parameters
/// 参数
/// `agent_mode` is the normalized optional child-agent mode.
/// `agent_mode` 是已规范化的可选子代理模式。
/// `fork_turns` is the caller supplied optional fork selector.
/// `fork_turns` 是调用方提供的可选派生选择器。
///
/// # Returns
/// 返回值
/// The enforced fork selector used by the TaskSession runtime.
/// TaskSession 运行时使用的强制派生选择器。
pub(super) fn effective_spawn_agent_fork_turns(
    agent_mode: Option<&str>,
    agent_id: Option<&str>,
    fork_turns: Option<&str>,
) -> Result<String, String> {
    if agent_mode == Some(SUBAGENT_MODE_EXPERT)
        || agent_id.is_some_and(|value| !value.trim().is_empty())
    {
        return Ok(String::from("none"));
    }
    Ok(normalize_spawn_agent_fork_turns(fork_turns)?.unwrap_or_else(|| String::from("all")))
}

/// Normalizes the Vulcan fork_turns selector before it reaches execution.
/// 在进入执行层之前规范化 Vulcan 的 fork_turns 选择器。
pub(super) fn normalize_spawn_agent_fork_turns(
    fork_turns: Option<&str>,
) -> Result<Option<String>, String> {
    let Some(fork_turns) = fork_turns.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if fork_turns.eq_ignore_ascii_case("none") {
        return Ok(Some(String::from("none")));
    }
    if fork_turns.eq_ignore_ascii_case("all") {
        return Ok(Some(String::from("all")));
    }
    let turn_count = fork_turns.parse::<usize>().map_err(|_| {
        String::from("fork_turns must be `none`, `all`, or a positive integer string")
    })?;
    if turn_count == 0 {
        return Err(String::from(
            "fork_turns must be `none`, `all`, or a positive integer string",
        ));
    }
    Ok(Some(turn_count.to_string()))
}

/// Parses list_agents arguments using path_prefix plus deprecated include_all compatibility.
/// 按 path_prefix 加废弃 include_all 兼容参数的契约解析 list_agents 参数。
pub(super) fn list_agents_path_prefix(args: &Value) -> Result<Option<String>, String> {
    let Some(object) = args.as_object() else {
        return Err(String::from("list_agents arguments must be a JSON object."));
    };
    let unsupported_keys = object
        .keys()
        .filter(|key| !matches!(key.as_str(), "path_prefix" | "include_all"))
        .cloned()
        .collect::<Vec<_>>();
    if !unsupported_keys.is_empty() {
        return Err(format!(
            "list_agents received unsupported keys: {}",
            unsupported_keys.join(", ")
        ));
    }
    Ok(string_field(object, &["path_prefix"]))
}

/// Parses the deprecated include_all compatibility flag for list_agents.
/// 解析 list_agents 的废弃 include_all 兼容标记。
///
/// # Parameters
/// 参数
/// `args` is the raw list_agents argument object.
/// `args` 是原始 list_agents 参数对象。
///
/// # Returns
/// 返回值
/// `Ok(true)` when the caller supplied the compatibility flag as true.
/// 调用方以 true 提供兼容标记时返回 `Ok(true)`。
pub(super) fn list_agents_include_all(args: &Value) -> Result<bool, String> {
    let Some(object) = args.as_object() else {
        return Err(String::from("list_agents arguments must be a JSON object."));
    };
    Ok(object
        .get("include_all")
        .and_then(Value::as_bool)
        .unwrap_or(false))
}

/// Builds the Vulcan tree path for one registry entry when it belongs to the selected root.
/// 当注册表条目属于选定根会话时，构建它的 Vulcan 树路径。
pub(super) fn subagent_entry_tree_path(
    entry: &SubagentRegistryEntry,
    entries_by_session: &BTreeMap<SessionNodeId, SubagentRegistryEntry>,
    root_session_id: SessionNodeId,
) -> Option<String> {
    let mut names = vec![entry.agent_id.as_str()];
    let mut parent_session_id = entry.parent_session_id;
    let mut visited = BTreeSet::new();
    loop {
        if parent_session_id == root_session_id {
            names.reverse();
            return Some(format!("/root/{}", names.join("/")));
        }
        if !visited.insert(parent_session_id) {
            return None;
        }
        let parent_entry = entries_by_session.get(&parent_session_id)?;
        names.push(parent_entry.agent_id.as_str());
        parent_session_id = parent_entry.parent_session_id;
    }
}

/// Returns whether a Vulcan tree path matches one optional path prefix.
/// 返回 Vulcan 树路径是否匹配一个可选路径前缀。
pub(super) fn subagent_tree_path_matches_prefix(entry_path: &str, path_prefix: &str) -> bool {
    let prefix = path_prefix.trim().trim_end_matches('/');
    if prefix.is_empty() || prefix == "/root" {
        return true;
    }
    let normalized_prefix = if prefix.starts_with("/root/") {
        prefix.to_string()
    } else {
        format!("/root/{prefix}")
    };
    entry_path == normalized_prefix
        || entry_path
            .strip_prefix(normalized_prefix.as_str())
            .is_some_and(|rest| rest.starts_with('/'))
}

/// Reads the optional wait timeout from a subagent tool request.
/// 从子代理工具请求中读取可选等待超时。
pub(super) fn subagent_tool_timeout_ms(args: &Value) -> Option<u64> {
    let Value::Object(map) = args else {
        return None;
    };
    integer_field(map, &["timeout_ms"])
}

/// Parses wait_agent arguments using the Vulcan timeout-only contract.
/// 按 Vulcan 仅支持 timeout 的契约解析 wait_agent 参数。
pub(super) fn wait_agent_timeout_ms(args: &Value) -> Result<Option<u64>, String> {
    let Some(object) = args.as_object() else {
        return Err(String::from("wait_agent arguments must be a JSON object."));
    };
    let unsupported_keys = object
        .keys()
        .filter(|key| key.as_str() != "timeout_ms")
        .cloned()
        .collect::<Vec<_>>();
    if !unsupported_keys.is_empty() {
        return Err(format!(
            "wait_agent received unsupported keys: {}",
            unsupported_keys.join(", ")
        ));
    }
    let timeout_ms = subagent_tool_timeout_ms(args);
    if let Some(timeout_ms) = timeout_ms {
        if timeout_ms < WAIT_AGENT_MIN_TIMEOUT_MS {
            return Err(format!(
                "timeout_ms must be at least {WAIT_AGENT_MIN_TIMEOUT_MS}"
            ));
        }
        if timeout_ms > WAIT_AGENT_MAX_TIMEOUT_MS {
            return Err(format!(
                "timeout_ms must be at most {WAIT_AGENT_MAX_TIMEOUT_MS}"
            ));
        }
    }
    Ok(timeout_ms)
}

/// Builds one compact JSON status payload for a native subagent registry entry.
/// 为一条原生子代理注册表条目构建紧凑 JSON 状态载荷。
///
/// # Parameters
/// 参数
/// `entry` is the subagent registry entry to expose.
/// `entry` 是需要暴露的子代理注册表条目。
///
/// # Returns
/// 返回值
/// A model-visible JSON status payload.
/// 模型可见的 JSON 状态载荷。
pub(super) fn subagent_registry_entry_status_payload(entry: &SubagentRegistryEntry) -> Value {
    json!({
        "agent_name": &entry.agent_id,
        "agent_status": subagent_registry_agent_status_payload(entry),
        "last_task_message": entry.last_task_message.clone(),
    })
}

/// Builds one host-only diagnostic status payload for a native subagent registry entry.
/// 为一条原生子代理注册表条目构建仅宿主可见的诊断状态载荷。
///
/// # Parameters
/// 参数
/// `entry` is the subagent registry entry to expose to UI/debug surfaces.
/// `entry` 是需要暴露给 UI 和调试界面的子代理注册表条目。
///
/// # Returns
/// 返回值
/// A host-visible JSON status payload with stable model fields and runtime diagnostics.
/// 带稳定模型字段与运行时诊断信息的宿主可见 JSON 状态载荷。
pub(super) fn subagent_registry_entry_host_status_payload(entry: &SubagentRegistryEntry) -> Value {
    json!({
        "agent_name": &entry.agent_id,
        "agent_status": subagent_registry_agent_status_payload(entry),
        "last_task_message": entry.last_task_message.clone(),
        (SUBAGENT_TASK_ID_KEY): entry.session_id.to_string(),
        "display_title": &entry.agent_id,
        "agent_kind": entry.agent_kind,
        "prompt_source": entry.prompt_source.clone(),
        "updated_at": entry.updated_at_ms.to_string(),
        "segments": entry.segments.clone(),
    })
}

/// Converts a registry status into the Codex-style `agent_status` payload.
/// 将注册表状态转换为 Codex 风格的 `agent_status` 载荷。
///
/// # Parameters
/// 参数
/// `entry` is the registry entry whose status should be exposed.
/// `entry` 是需要暴露状态的注册表条目。
///
/// # Returns
/// 返回值
/// A compact status value compatible with the multi-agent V2 list semantics.
/// 与多代理 V2 列表语义兼容的紧凑状态值。
fn subagent_registry_agent_status_payload(entry: &SubagentRegistryEntry) -> Value {
    match entry.status {
        SubagentStatus::Created => json!("pending_init"),
        SubagentStatus::Running
        | SubagentStatus::WaitingChildren
        | SubagentStatus::WaitingFollowup
        | SubagentStatus::Compacting => json!("running"),
        SubagentStatus::Completed => json!({ "completed": entry.result_summary.clone() }),
        SubagentStatus::Failed => json!({
            "errored": entry
                .result_summary
                .clone()
                .unwrap_or_else(|| String::from("Agent errored.")),
        }),
        SubagentStatus::Stopped => json!("interrupted"),
        SubagentStatus::Archived => json!("shutdown"),
    }
}

/// Builds a model-visible status result from native subagent registry entries.
/// 根据原生子代理注册表条目构建模型可见状态结果。
///
/// # Parameters
/// 参数
/// `tool_name` is the subagent control tool name.
/// `tool_name` 是子代理控制工具名称。
/// `current_session_id` is the parent session id.
/// `current_session_id` 是父会话标识。
/// `entries` are the registry entries to expose.
/// `entries` 是需要暴露的注册表条目。
/// `timeout_ms` is the wait timeout represented in the tool result.
/// `timeout_ms` 是工具结果中表示的等待超时时间。
/// `completed` indicates whether the requested wait/list operation is complete.
/// `completed` 表示请求的等待或列出操作是否完成。
///
/// # Returns
/// 返回值
/// A model-visible tool invocation result.
/// 模型可见的工具调用结果。
pub(super) fn build_subagent_registry_status_tool_result(
    tool_name: &str,
    current_session_id: SessionId,
    entries: Vec<SubagentRegistryEntry>,
    timeout_ms: u64,
    completed: bool,
) -> VulcanToolInvocationResult {
    let model_agents = entries
        .iter()
        .map(subagent_registry_entry_status_payload)
        .collect::<Vec<_>>();
    let host_agents = entries
        .iter()
        .map(subagent_registry_entry_host_status_payload)
        .collect::<Vec<_>>();
    let model_payload = json!({
        "tool": tool_name,
        "parent_session_id": current_session_id.to_string(),
        "completed": completed,
        "timeout_ms": timeout_ms,
        "agents": model_agents,
    });
    let host_payload = json!({
        "tool": tool_name,
        "parent_session_id": current_session_id.to_string(),
        "completed": completed,
        "timeout_ms": timeout_ms,
        "agents": host_agents,
    });
    VulcanToolInvocationResult::from_model_output(model_payload).with_host_result(
        VulcanToolHostResult {
            kind: String::from("subagent_status"),
            payload: host_payload,
        },
    )
}

/// Builds a model-visible activity result for `wait_agent`.
/// 为 `wait_agent` 构建模型可见的活动唤醒结果。
///
/// # Parameters
/// 参数
/// `current_session_id` is the parent session id that invoked the wait.
/// `current_session_id` 是发起等待的父会话标识。
/// `timeout_ms` is the configured wait timeout represented for audit surfaces.
/// `timeout_ms` 是面向审计表面的等待超时时间。
/// `timed_out` indicates whether the wait ended by timeout.
/// `timed_out` 表示等待是否因超时结束。
///
/// # Returns
/// 返回值
/// A tool result that deliberately omits child-agent output text.
/// 一个刻意不包含子代理输出正文的工具结果。
pub(super) fn build_wait_agent_activity_tool_result(
    current_session_id: SessionId,
    timeout_ms: u64,
    timed_out: bool,
) -> VulcanToolInvocationResult {
    let activity = if timed_out {
        WaitAgentActivityResult::timed_out()
    } else {
        WaitAgentActivityResult::completed()
    };
    let payload = json!({
        "tool": WAIT_AGENT_TOOL,
        "parent_session_id": current_session_id.to_string(),
        "message": activity.message,
        "timed_out": activity.timed_out,
        "timeout_ms": timeout_ms,
    });
    VulcanToolInvocationResult::from_model_output(payload.clone()).with_host_result(
        VulcanToolHostResult {
            kind: String::from("subagent_wait"),
            payload,
        },
    )
}

/// Builds a model-visible user-steer interruption result for `wait_agent`.
/// 为 `wait_agent` 构建模型可见的用户插话中断结果。
///
/// # Parameters
/// 参数
/// `current_session_id` is the parent session id that invoked the wait.
/// `current_session_id` 是发起等待的父会话标识。
/// `timeout_ms` is the configured wait timeout represented for audit surfaces.
/// `timeout_ms` 是面向审计表面的等待超时时间。
///
/// # Returns
/// 返回值
/// A wait result that tells the parent model to handle the newly steered input.
/// 提醒父模型处理新插话输入的等待结果。
pub(super) fn build_wait_agent_steered_tool_result(
    current_session_id: SessionId,
    timeout_ms: u64,
) -> VulcanToolInvocationResult {
    let activity = WaitAgentActivityResult::steered();
    let payload = json!({
        "tool": WAIT_AGENT_TOOL,
        "parent_session_id": current_session_id.to_string(),
        "message": activity.message,
        "timed_out": activity.timed_out,
        "timeout_ms": timeout_ms,
        "interrupted": true,
    });
    VulcanToolInvocationResult::from_model_output(payload.clone()).with_host_result(
        VulcanToolHostResult {
            kind: String::from("subagent_wait"),
            payload,
        },
    )
}
