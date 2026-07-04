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
        build_agent_orchestration_info_tool_candidate(Arc::clone(&task_runtime)),
        build_send_message_tool_candidate(Arc::clone(&task_runtime)),
        build_wait_agent_tool_candidate(Arc::clone(&task_runtime)),
        build_interrupt_agent_tool_candidate(Arc::clone(&task_runtime)),
        build_close_agent_tool_candidate(Arc::clone(&task_runtime)),
        build_list_agents_tool_candidate(task_runtime),
    ]
}

/// Builds the model-visible non-blocking child-agent spawn tool.
/// 构建模型可见的非阻塞子代理启动工具。
pub(super) fn build_spawn_agent_tool_candidate(
    task_runtime: Arc<TaskRuntime>,
) -> VulcanToolCandidate {
    VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: String::from(SPAWN_AGENT_TOOL),
            name: String::from(SPAWN_AGENT_TOOL),
            description: spawn_agent_tool_description(),
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

/// Builds the stable spawn_agent description.
/// 构建稳定的 spawn_agent 描述。
fn spawn_agent_tool_description() -> String {
    String::from(
        "Create an open child agent for one concrete bounded subtask. Use creation_mode `agent` with an agent_id returned by agent_orchestration_info, `mirror` only when the child must inherit the current conversation context, `custom` with a one-off system_prompt, or `default` for a plain child agent. Do not guess agent_id or model selectors; query agent_orchestration_info first when you need them.",
    )
}

/// Builds the model-visible child-agent orchestration information tool.
/// 构建模型可见的子代理调度信息查询工具。
pub(super) fn build_agent_orchestration_info_tool_candidate(
    task_runtime: Arc<TaskRuntime>,
) -> VulcanToolCandidate {
    VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: String::from(AGENT_ORCHESTRATION_INFO_TOOL),
            name: String::from(AGENT_ORCHESTRATION_INFO_TOOL),
            description: String::from(
                "Return read-only child-agent orchestration information: enabled agent ids, creation modes, model selectors when available, and runtime counts. This tool never changes agents, models, or runtime state.",
            ),
            help_text: None,
            source: VulcanToolSourceKind::Native,
        },
        parameters_schema: agent_orchestration_info_args_schema(),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Hidden,
        invoke: Arc::new(move |args, session_id| {
            let task_runtime = Arc::clone(&task_runtime);
            Box::pin(async move {
                let session_id = parse_subagent_tool_session_id(
                    session_id.as_deref(),
                    AGENT_ORCHESTRATION_INFO_TOOL,
                )?;
                task_runtime
                    .invoke_agent_orchestration_info(session_id, args)
                    .await
            })
        }),
    }
}

/// Builds the model-visible child-agent message tool.
/// 构建模型可见的子代理消息工具。
pub(super) fn build_send_message_tool_candidate(
    task_runtime: Arc<TaskRuntime>,
) -> VulcanToolCandidate {
    VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: String::from(SEND_MESSAGE_TOOL),
            name: String::from(SEND_MESSAGE_TOOL),
            description: String::from(
                "Send a message to an open child agent. Use delivery_mode `queue` to run after the current task, or `interrupt` to stop the current task and make this message the next task immediately. If the target is idle, either mode starts a new turn.",
            ),
            help_text: None,
            source: VulcanToolSourceKind::Native,
        },
        parameters_schema: send_message_args_schema(),
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
                "List known open child agents and their current status, including completed agents that have not been closed. This tool does not collect child-agent results and does not consume mailbox updates; use wait_agent to wait for activity, then continue normally so pending inter-agent messages can enter the next model request.",
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
                "Interrupt an open child agent's current turn, if any, and return its previous status. The agent remains open and available for future send_message calls.",
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

/// Builds the model-visible child-agent close tool.
/// 构建模型可见的子代理关闭工具。
pub(super) fn build_close_agent_tool_candidate(
    task_runtime: Arc<TaskRuntime>,
) -> VulcanToolCandidate {
    VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: String::from(CLOSE_AGENT_TOOL),
            name: String::from(CLOSE_AGENT_TOOL),
            description: String::from(
                "Close an open child agent and stop any active work it owns. A closed agent is no longer returned by default list_agents and cannot receive send_message calls.",
            ),
            help_text: None,
            source: VulcanToolSourceKind::Native,
        },
        parameters_schema: close_agent_args_schema(),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Hidden,
        invoke: Arc::new(move |args, session_id| {
            let task_runtime = Arc::clone(&task_runtime);
            Box::pin(async move {
                let session_id =
                    parse_subagent_tool_session_id(session_id.as_deref(), CLOSE_AGENT_TOOL)?;
                task_runtime.invoke_close_agent(session_id, args).await
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
            "creation_mode": {
                "type": "string",
                "enum": ["default", "mirror", "agent", "custom"],
                "description": "How to create the child agent. Use `agent` with agent_id from agent_orchestration_info, `mirror` only to inherit the current conversation context, `custom` with system_prompt, or `default` for a plain child agent."
            },
            "agent_id": {
                "type": "string",
                "description": "Enabled agent id returned by agent_orchestration_info. Required only when creation_mode is `agent`."
            },
            "system_prompt": {
                "type": "string",
                "description": "One-off custom system prompt. Required only when creation_mode is `custom`."
            },
            "model": {
                "type": "string",
                "description": "Optional model selector returned by agent_orchestration_info. Invalid or unavailable selectors fail the spawn call."
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

/// Builds the JSON Schema advertised to the model for `agent_orchestration_info`.
/// 为模型端 `agent_orchestration_info` 工具构建对外声明的 JSON Schema。
pub(super) fn agent_orchestration_info_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "sections": {
                "type": "array",
                "items": {
                    "type": "string",
                    "enum": ["agents", "models", "modes", "runtime"]
                },
                "description": "Optional sections to include. Omit to include all sections."
            },
            "agent_id": {
                "type": "string",
                "description": "Optional enabled agent id to focus model diagnostics on."
            },
            "query": {
                "type": "string",
                "description": "Optional case-insensitive text filter for agent id, name, description, or category."
            }
        },
        "additionalProperties": false
    })
}

/// Builds the JSON Schema advertised to the model for `send_message`.
/// 为模型端 `send_message` 工具构建对外声明的 JSON Schema。
pub(super) fn send_message_args_schema() -> Value {
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
            },
            "delivery_mode": {
                "type": "string",
                "enum": ["queue", "interrupt"],
                "description": "Use `queue` to run after the target's current task, or `interrupt` to stop the current task and run this message next. Defaults to `queue`."
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
            "include_closed": {
                "type": "boolean",
                "description": "Include closed agents for diagnostics. Omit or false for the default open-agent view."
            }
        },
        "additionalProperties": false
    })
}

/// Builds the JSON Schema advertised to the model for `close_agent`.
/// 为模型端 `close_agent` 工具构建对外声明的 JSON Schema。
pub(super) fn close_agent_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "target": {
                "type": "string",
                "description": "Agent id or canonical task name to close (from spawn_agent)."
            },
            "cascade": {
                "type": "boolean",
                "description": "Whether to close descendants as well. Defaults to true."
            }
        },
        "required": ["target"],
        "additionalProperties": false
    })
}

/// Reads the required close target from a close_agent request.
/// 从 close_agent 请求中读取必填关闭目标。
pub(super) fn close_agent_target(args: &Value) -> Result<String, String> {
    let Value::Object(map) = args else {
        return Err(String::from("close_agent arguments must be a JSON object."));
    };
    let unsupported_keys = map
        .keys()
        .filter(|key| !matches!(key.as_str(), "target" | "cascade"))
        .cloned()
        .collect::<Vec<_>>();
    if !unsupported_keys.is_empty() {
        return Err(format!(
            "close_agent received unsupported keys: {}",
            unsupported_keys.join(", ")
        ));
    }
    string_field(map, &["target"])
        .ok_or_else(|| String::from("close_agent requires target with the child-agent task name."))
}

/// Reads the optional close cascade flag from a close_agent request.
/// 从 close_agent 请求中读取可选级联关闭标记。
pub(super) fn close_agent_cascade(args: &Value) -> Result<bool, String> {
    let Value::Object(map) = args else {
        return Err(String::from("close_agent arguments must be a JSON object."));
    };
    Ok(map.get("cascade").and_then(Value::as_bool).unwrap_or(true))
}

/// Parsed request for agent_orchestration_info.
/// agent_orchestration_info 的已解析请求。
#[derive(Debug, Clone)]
pub(super) struct AgentOrchestrationInfoRequest {
    /// Sections requested by the caller.
    /// 调用方请求的分区。
    sections: Option<BTreeSet<String>>,
    /// Optional focused agent id.
    /// 可选聚焦智能体标识。
    pub agent_id: Option<String>,
    /// Optional text query for agent metadata.
    /// 可选智能体元数据文本查询。
    pub query: Option<String>,
}

impl AgentOrchestrationInfoRequest {
    /// Returns whether one section should be included.
    /// 返回是否应包含某个分区。
    pub(super) fn includes(&self, section: &str) -> bool {
        self.sections
            .as_ref()
            .is_none_or(|sections| sections.contains(section))
    }
}

/// Parses one agent_orchestration_info request.
/// 解析一次 agent_orchestration_info 请求。
pub(super) fn agent_orchestration_info_request(
    args: &Value,
) -> Result<AgentOrchestrationInfoRequest, String> {
    let Value::Object(map) = args else {
        return Err(String::from(
            "agent_orchestration_info arguments must be a JSON object.",
        ));
    };
    let unsupported_keys = map
        .keys()
        .filter(|key| !matches!(key.as_str(), "sections" | "agent_id" | "query"))
        .cloned()
        .collect::<Vec<_>>();
    if !unsupported_keys.is_empty() {
        return Err(format!(
            "agent_orchestration_info received unsupported keys: {}",
            unsupported_keys.join(", ")
        ));
    }
    let sections = map
        .get("sections")
        .map(agent_orchestration_info_sections)
        .transpose()?;
    Ok(AgentOrchestrationInfoRequest {
        sections,
        agent_id: string_field(map, &["agent_id"]),
        query: string_field(map, &["query"]),
    })
}

/// Parses the optional agent_orchestration_info sections list.
/// 解析可选的 agent_orchestration_info 分区列表。
fn agent_orchestration_info_sections(value: &Value) -> Result<BTreeSet<String>, String> {
    let Value::Array(items) = value else {
        return Err(String::from(
            "agent_orchestration_info.sections must be an array.",
        ));
    };
    let mut sections = BTreeSet::new();
    for item in items {
        let Some(section) = item
            .as_str()
            .and_then(|value| normalize_optional_task_label(Some(value)))
        else {
            return Err(String::from(
                "agent_orchestration_info.sections only accepts non-empty strings.",
            ));
        };
        match section.as_str() {
            "agents" | "models" | "modes" | "runtime" => {
                sections.insert(section);
            }
            other => {
                return Err(format!(
                    "agent_orchestration_info.sections contains unsupported section `{other}`."
                ));
            }
        }
    }
    Ok(sections)
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
                "message" | "task_name" | "creation_mode" | "agent_id" | "system_prompt" | "model"
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    if !unsupported_keys.is_empty() {
        return Err(format!(
            "`spawn_agent` only accepts message, task_name, creation_mode, agent_id, system_prompt, and model; unsupported keys: {}",
            unsupported_keys.join(", ")
        ));
    }
    let creation_mode =
        normalize_subagent_creation_mode(string_field(&map, &["creation_mode"]).as_deref())?;
    let agent_id = string_field(&map, &["agent_id"]);
    let system_prompt = string_field(&map, &["system_prompt"]);
    if creation_mode == "agent" && agent_id.is_none() {
        return Err(String::from(
            "`spawn_agent` requires `agent_id` when creation_mode is `agent`.",
        ));
    }
    if creation_mode != "agent" && agent_id.is_some() {
        return Err(String::from(
            "`spawn_agent.agent_id` is only valid when creation_mode is `agent`.",
        ));
    }
    if creation_mode == "custom" && system_prompt.is_none() {
        return Err(String::from(
            "`spawn_agent` requires `system_prompt` when creation_mode is `custom`.",
        ));
    }
    if creation_mode != "custom" && system_prompt.is_some() {
        return Err(String::from(
            "`spawn_agent.system_prompt` is only valid when creation_mode is `custom`.",
        ));
    }
    let model = string_field(&map, &["model"]);
    let (agent_mode, fork_turns) = spawn_agent_internal_mode_and_fork(&creation_mode);
    let mut task_args = serde_json::Map::new();
    task_args.insert(String::from("prompt"), Value::String(prompt));
    task_args.insert(String::from("task_id"), Value::String(task_id.clone()));
    task_args.insert(String::from("description"), Value::String(task_id));
    task_args.insert(String::from("agent_mode"), Value::String(agent_mode));
    if let Some(agent_id) = agent_id {
        task_args.insert(String::from("subagent_profile_id"), Value::String(agent_id));
    }
    if let Some(system_prompt) = system_prompt {
        task_args.insert(String::from("system_prompt"), Value::String(system_prompt));
    }
    if let Some(model) = model {
        let (provider_id, model_id) = split_provider_model_selector(&model)?;
        task_args.insert(
            String::from("provider_model"),
            json!({
                "provider_id": provider_id,
                "model_id": model_id,
            }),
        );
    }
    task_args.insert(String::from("fork_turns"), Value::String(fork_turns));
    Ok(Value::Object(task_args))
}

/// Resolves the internal TaskSession mode and fork selector for one creation mode.
/// 为单个创建模式解析内部 TaskSession 模式与上下文派生选择器。
///
/// # Parameters
/// 参数
/// `creation_mode` is the model-visible child-agent creation mode.
/// `creation_mode` 是模型可见的子代理创建模式。
///
/// # Returns
/// 返回值
/// A pair of internal mode and fork selector strings.
/// 内部模式与派生选择器字符串对。
pub(super) fn spawn_agent_internal_mode_and_fork(creation_mode: &str) -> (String, String) {
    match creation_mode {
        "mirror" => (String::from(SUBAGENT_MODE_MIRROR), String::from("all")),
        "agent" => (String::from(SUBAGENT_MODE_EXPERT), String::from("none")),
        "custom" | "default" => (String::from(SUBAGENT_MODE_DEFAULT), String::from("none")),
        _ => (String::from(SUBAGENT_MODE_DEFAULT), String::from("none")),
    }
}

/// Parses list_agents arguments using path_prefix plus include_closed.
/// 按 path_prefix 加 include_closed 解析 list_agents 参数。
pub(super) fn list_agents_path_prefix(args: &Value) -> Result<Option<String>, String> {
    let Some(object) = args.as_object() else {
        return Err(String::from("list_agents arguments must be a JSON object."));
    };
    let unsupported_keys = object
        .keys()
        .filter(|key| !matches!(key.as_str(), "path_prefix" | "include_closed"))
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

/// Parses the include_closed diagnostics flag for list_agents.
/// 解析 list_agents 的 include_closed 诊断标记。
///
/// # Parameters
/// 参数
/// `args` is the raw list_agents argument object.
/// `args` 是原始 list_agents 参数对象。
///
/// # Returns
/// 返回值
/// `Ok(true)` when the caller requested closed agents as diagnostics.
/// 调用方请求关闭代理诊断信息时返回 `Ok(true)`。
pub(super) fn list_agents_include_closed(args: &Value) -> Result<bool, String> {
    let Some(object) = args.as_object() else {
        return Err(String::from("list_agents arguments must be a JSON object."));
    };
    Ok(object
        .get("include_closed")
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
pub(super) fn subagent_registry_entry_status_payload(
    entry: &SubagentRegistryEntry,
    queued_message_count: usize,
) -> Value {
    json!({
        "agent_name": &entry.agent_id,
        "agent_id": &entry.agent_id,
        "agent_status": subagent_registry_agent_status_payload(entry),
        "lifecycle_state": subagent_registry_lifecycle_state(entry.status),
        "run_state": subagent_registry_run_state(entry.status, queued_message_count),
        "status": subagent_registry_status_name(entry.status),
        "stop_reason": entry.stop_reason.clone(),
        "last_task_message": entry.last_task_message.clone(),
        "last_result_summary": entry.result_summary.clone(),
        "has_final_result": subagent_registry_has_final_result(entry),
        "queued_message_count": queued_message_count,
        "can_send": entry.status != SubagentStatus::Archived,
        "can_interrupt": entry.status != SubagentStatus::Archived && entry.status.is_running(),
        "can_close": entry.status != SubagentStatus::Archived,
        "can_wait": entry.status != SubagentStatus::Archived,
        "updated_at": entry.updated_at_ms.to_string(),
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
pub(super) fn subagent_registry_entry_host_status_payload(
    entry: &SubagentRegistryEntry,
    queued_message_count: usize,
) -> Value {
    json!({
        "agent_name": &entry.agent_id,
        "agent_id": &entry.agent_id,
        "agent_status": subagent_registry_agent_status_payload(entry),
        "lifecycle_state": subagent_registry_lifecycle_state(entry.status),
        "run_state": subagent_registry_run_state(entry.status, queued_message_count),
        "status": subagent_registry_status_name(entry.status),
        "stop_reason": entry.stop_reason.clone(),
        "last_task_message": entry.last_task_message.clone(),
        "last_result_summary": entry.result_summary.clone(),
        "has_final_result": subagent_registry_has_final_result(entry),
        "queued_message_count": queued_message_count,
        "can_send": entry.status != SubagentStatus::Archived,
        "can_interrupt": entry.status != SubagentStatus::Archived && entry.status.is_running(),
        "can_close": entry.status != SubagentStatus::Archived,
        "can_wait": entry.status != SubagentStatus::Archived,
        (SUBAGENT_TASK_ID_KEY): entry.session_id.to_string(),
        "display_title": &entry.agent_id,
        "agent_kind": entry.agent_kind,
        "prompt_source": entry.prompt_source.clone(),
        "updated_at": entry.updated_at_ms.to_string(),
        "segments": entry.segments.clone(),
    })
}

/// Returns the management lifecycle state for one registry status.
/// 返回单个注册表状态对应的管理生命周期状态。
fn subagent_registry_lifecycle_state(status: SubagentStatus) -> &'static str {
    match status {
        SubagentStatus::Archived => "closed",
        _ => "open",
    }
}

/// Returns the execution run state for one registry status.
/// 返回单个注册表状态对应的执行状态。
fn subagent_registry_run_state(
    status: SubagentStatus,
    queued_message_count: usize,
) -> &'static str {
    if status != SubagentStatus::Archived && queued_message_count > 0 {
        return "queued";
    }
    match status {
        SubagentStatus::Created => "idle",
        SubagentStatus::Running
        | SubagentStatus::WaitingChildren
        | SubagentStatus::WaitingFollowup
        | SubagentStatus::Compacting => "running",
        SubagentStatus::Completed => "completed",
        SubagentStatus::Failed => "failed",
        SubagentStatus::Stopped => "stopped",
        SubagentStatus::Archived => "closed",
    }
}

/// Returns the stable status label for one registry status.
/// 返回单个注册表状态的稳定状态标签。
fn subagent_registry_status_name(status: SubagentStatus) -> &'static str {
    match status {
        SubagentStatus::Created => "created",
        SubagentStatus::Running => "running",
        SubagentStatus::WaitingChildren => "waiting_children",
        SubagentStatus::WaitingFollowup => "waiting_followup",
        SubagentStatus::Completed => "completed",
        SubagentStatus::Failed => "failed",
        SubagentStatus::Stopped => "stopped",
        SubagentStatus::Compacting => "compacting",
        SubagentStatus::Archived => "archived",
    }
}

/// Returns whether a registry entry has a completed final result summary.
/// 返回注册表条目是否具有已完成的最终结果摘要。
fn subagent_registry_has_final_result(entry: &SubagentRegistryEntry) -> bool {
    entry.status == SubagentStatus::Completed
        && entry
            .result_summary
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
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
    queued_message_counts: &BTreeMap<String, usize>,
    timeout_ms: u64,
    completed: bool,
) -> VulcanToolInvocationResult {
    let model_agents = entries
        .iter()
        .map(|entry| {
            let queued_message_count = queued_message_counts
                .get(entry.agent_id.as_str())
                .copied()
                .unwrap_or(0);
            subagent_registry_entry_status_payload(entry, queued_message_count)
        })
        .collect::<Vec<_>>();
    let host_agents = entries
        .iter()
        .map(|entry| {
            let queued_message_count = queued_message_counts
                .get(entry.agent_id.as_str())
                .copied()
                .unwrap_or(0);
            subagent_registry_entry_host_status_payload(entry, queued_message_count)
        })
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
