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
        request_streaming: None,
        invoke: Arc::new(move |args, context| {
            let task_runtime = Arc::clone(&task_runtime);
            Box::pin(async move {
                let trace_started_at = current_unix_millis_u64();
                let session_id = parse_subagent_tool_session_id(
                    Some(context.session_id.as_str()),
                    SPAWN_AGENT_TOOL,
                )?;
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
    String::from("Create an open child agent and start its delegated task.")
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
                "Return enabled child-agent profiles, supported creation modes, available model selectors, and runtime counts without changing runtime state.",
            ),
            help_text: None,
            source: VulcanToolSourceKind::Native,
        },
        parameters_schema: agent_orchestration_info_args_schema(),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Hidden,
        request_streaming: None,
        invoke: Arc::new(move |args, context| {
            let task_runtime = Arc::clone(&task_runtime);
            Box::pin(async move {
                let session_id = parse_subagent_tool_session_id(
                    Some(context.session_id.as_str()),
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
                "Deliver a message to an open child agent using queue or interrupt mode.",
            ),
            help_text: None,
            source: VulcanToolSourceKind::Native,
        },
        parameters_schema: send_message_args_schema(),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Hidden,
        request_streaming: None,
        invoke: Arc::new(move |args, context| {
            let task_runtime = Arc::clone(&task_runtime);
            Box::pin(async move {
                let session_id = parse_subagent_tool_session_id(
                    Some(context.session_id.as_str()),
                    SEND_MESSAGE_TOOL,
                )?;
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
                "Wait for child-agent mailbox activity and return claimed feedback, an interruption summary, or a timeout summary. Without explicit targets, queued feedback is claimed first; when no child is active and no feedback remains, the tool returns immediately and must not be polled again until a child is started.",
            ),
            help_text: None,
            source: VulcanToolSourceKind::Native,
        },
        parameters_schema: wait_agent_args_schema(),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Hidden,
        request_streaming: None,
        invoke: Arc::new(move |args, context| {
            let task_runtime = Arc::clone(&task_runtime);
            Box::pin(async move {
                let session_id = parse_subagent_tool_session_id(
                    Some(context.session_id.as_str()),
                    WAIT_AGENT_TOOL,
                )?;
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
                "Return current status snapshots for known open child agents, including completed agents that have not been closed.",
            ),
            help_text: None,
            source: VulcanToolSourceKind::Native,
        },
        parameters_schema: list_agents_args_schema(),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Hidden,
        request_streaming: None,
        invoke: Arc::new(move |args, context| {
            let task_runtime = Arc::clone(&task_runtime);
            Box::pin(async move {
                let session_id = parse_subagent_tool_session_id(
                    Some(context.session_id.as_str()),
                    LIST_AGENTS_TOOL,
                )?;
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
                "Stop an open child agent's current turn, return its previous status, and keep the agent open.",
            ),
            help_text: None,
            source: VulcanToolSourceKind::Native,
        },
        parameters_schema: interrupt_agent_args_schema(),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Hidden,
        request_streaming: None,
        invoke: Arc::new(move |args, context| {
            let task_runtime = Arc::clone(&task_runtime);
            Box::pin(async move {
                let session_id = parse_subagent_tool_session_id(
                    Some(context.session_id.as_str()),
                    INTERRUPT_AGENT_TOOL,
                )?;
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
            description: String::from("Close an open child agent and stop its active run, if any."),
            help_text: None,
            source: VulcanToolSourceKind::Native,
        },
        parameters_schema: close_agent_args_schema(),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Hidden,
        request_streaming: None,
        invoke: Arc::new(move |args, context| {
            let task_runtime = Arc::clone(&task_runtime);
            Box::pin(async move {
                let session_id = parse_subagent_tool_session_id(
                    Some(context.session_id.as_str()),
                    CLOSE_AGENT_TOOL,
                )?;
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
                "minLength": 1,
                "pattern": "^[^/]+$",
                "description": "Parent-scoped task name for the new agent. It must be one path segment and cannot contain `/`."
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
            "target": {
                "type": "string",
                "description": "Optional child-agent task name or agent id to wait for."
            },
            "targets": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Optional child-agent task names or agent ids to wait for. The wait completes after each requested target has produced a message or terminal result."
            },
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
        .filter(|key| key.as_str() != "target")
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
    let agent_mode = spawn_agent_internal_mode(&creation_mode);
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
    Ok(Value::Object(task_args))
}

/// Resolves the internal TaskSession mode for one creation mode.
/// 为单个创建模式解析内部 TaskSession 模式。
///
/// # Parameters
/// 参数
/// `creation_mode` is the model-visible child-agent creation mode.
/// `creation_mode` 是模型可见的子代理创建模式。
///
/// # Returns
/// 返回值
/// The internal TaskSession mode string.
/// 内部 TaskSession 模式字符串。
pub(super) fn spawn_agent_internal_mode(creation_mode: &str) -> String {
    match creation_mode {
        "mirror" => String::from(SUBAGENT_MODE_MIRROR),
        "agent" => String::from(SUBAGENT_MODE_EXPERT),
        "custom" | "default" => String::from(SUBAGENT_MODE_DEFAULT),
        _ => String::from(SUBAGENT_MODE_DEFAULT),
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

/// Parses wait_agent arguments using the Vulcan target-aware wait contract.
/// 按 Vulcan 支持目标选择的等待契约解析 wait_agent 参数。
pub(super) fn wait_agent_timeout_ms(args: &Value) -> Result<Option<u64>, String> {
    let Some(object) = args.as_object() else {
        return Err(String::from("wait_agent arguments must be a JSON object."));
    };
    let unsupported_keys = object
        .keys()
        .filter(|key| !matches!(key.as_str(), "timeout_ms" | "target" | "targets"))
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

/// Parses explicit wait_agent target selectors while retaining the canonical task text.
/// 解析 wait_agent 显式目标选择器，同时保留规范化任务文本。
///
/// # Parameters
/// 参数
/// `args` is the JSON object passed to wait_agent.
/// `args` 是传给 wait_agent 的 JSON 对象。
///
/// # Returns
/// 返回值
/// A sorted unique selector list, or an empty list for live-agent default waiting.
/// 返回排序去重后的选择器列表；空列表表示默认等待运行中的智能体。
pub(super) fn wait_agent_target_selectors(args: &Value) -> Result<Vec<String>, String> {
    let Some(object) = args.as_object() else {
        return Err(String::from("wait_agent arguments must be a JSON object."));
    };
    let mut targets = Vec::new();
    if let Some(value) = object.get("target") {
        append_string_values(&mut targets, value);
    }
    if let Some(value) = object.get("targets") {
        append_string_values(&mut targets, value);
    }
    let mut normalized_targets = targets
        .iter()
        .map(|target| normalize_public_task_identifier(target))
        .collect::<Result<Vec<_>, _>>()?;
    normalized_targets.sort();
    normalized_targets.dedup();
    Ok(normalized_targets)
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
        "parent_session_id": entry.parent_session_id.to_string(),
        "prompt_source": entry.prompt_source.clone(),
        "updated_at": entry.updated_at_ms.to_string(),
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
/// `queued_message_counts` maps stable child sessions to their exact pending message counts.
/// `queued_message_counts` 将稳定子会话映射到其精确待处理消息数量。
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
    entries: Vec<(String, SubagentRegistryEntry)>,
    queued_message_counts: &BTreeMap<SessionNodeId, usize>,
    timeout_ms: u64,
    completed: bool,
) -> VulcanToolInvocationResult {
    let model_agents = entries
        .iter()
        .map(|(tree_path, entry)| {
            let queued_message_count = queued_message_counts
                .get(&entry.session_id)
                .copied()
                .unwrap_or(0);
            let mut payload = subagent_registry_entry_status_payload(entry, queued_message_count);
            payload["tree_path"] = Value::String(tree_path.clone());
            payload
        })
        .collect::<Vec<_>>();
    let host_agents = entries
        .iter()
        .map(|(tree_path, entry)| {
            let queued_message_count = queued_message_counts
                .get(&entry.session_id)
                .copied()
                .unwrap_or(0);
            let mut payload =
                subagent_registry_entry_host_status_payload(entry, queued_message_count);
            payload["tree_path"] = Value::String(tree_path.clone());
            payload
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
/// A tool result that carries the child-agent feedback claimed by this wait call.
/// 一个携带本次等待调用已认领子代理反馈的工具结果。
pub(super) fn build_wait_agent_activity_tool_result(
    current_session_id: SessionId,
    timeout_ms: u64,
    timed_out: bool,
    events: &[SubagentMailboxEvent],
) -> VulcanToolInvocationResult {
    let (message, activity_status) = if timed_out {
        (
            WaitAgentActivityResult::timed_out().message,
            "timeout".to_string(),
        )
    } else {
        wait_agent_activity_message(events)
    };
    let event_summaries = wait_agent_event_summaries(events, false);
    let host_event_summaries = wait_agent_event_summaries(events, true);
    let terminal_event_count = events
        .iter()
        .filter(|event| wait_agent_mailbox_event_is_terminal(event.kind))
        .count();
    let message_event_count = events
        .iter()
        .filter(|event| event.kind == SubagentMailboxEventKind::Message)
        .count();
    let status_event_count = events
        .iter()
        .filter(|event| {
            matches!(
                event.kind,
                SubagentMailboxEventKind::Started | SubagentMailboxEventKind::StatusUpdate
            )
        })
        .count();
    let feedback_text = wait_agent_feedback_text(events);
    let model_payload = json!({
        "tool": WAIT_AGENT_TOOL,
        "parent_session_id": current_session_id.to_string(),
        "message": message,
        "activity_status": activity_status,
        "result": feedback_text,
        "timed_out": timed_out,
        "timeout_ms": timeout_ms,
        "delivered_event_count": events.len(),
        "no_active_agents": false,
        "terminal_event_count": terminal_event_count,
        "message_event_count": message_event_count,
        "status_event_count": status_event_count,
        "agents": event_summaries,
    });
    let host_payload = json!({
        "tool": WAIT_AGENT_TOOL,
        "parent_session_id": current_session_id.to_string(),
        "message": message,
        "activity_status": activity_status,
        "result": feedback_text,
        "timed_out": timed_out,
        "timeout_ms": timeout_ms,
        "delivered_event_count": events.len(),
        "no_active_agents": false,
        "terminal_event_count": terminal_event_count,
        "message_event_count": message_event_count,
        "status_event_count": status_event_count,
        "agents": host_event_summaries,
    });
    VulcanToolInvocationResult::from_model_output(model_payload).with_host_result(
        VulcanToolHostResult {
            kind: String::from("subagent_wait"),
            payload: host_payload,
        },
    )
}

/// Builds an immediate `wait_agent` result when the parent has no active child agents or queued feedback.
/// 当父会话不存在活动子代理或排队反馈时，构建即时 `wait_agent` 结果。
///
/// # Parameters
/// 参数
/// `current_session_id` is the parent session id that invoked the wait.
/// `current_session_id` 是发起等待的父会话标识。
/// `timeout_ms` is echoed for audit parity without actually waiting.
/// `timeout_ms` 仅为审计一致性回显，实际不会等待。
///
/// # Returns
/// 返回值
/// A terminal tool result instructing the model not to wait again until a child is started.
/// 返回终态工具结果，明确告知模型在启动新子代理前不要再次等待。
pub(super) fn build_wait_agent_no_active_agents_tool_result(
    current_session_id: SessionId,
    timeout_ms: u64,
) -> VulcanToolInvocationResult {
    // message is deliberately directive because another wait cannot produce new activity without a child agent.
    // message 使用明确指令，因为没有子代理时再次等待不可能产生新活动。
    let message = String::from(
        "No active child agents or queued child-agent feedback remain. Do not call wait_agent again until a child agent is started.",
    );
    // model_payload exposes the exact zero-work terminal condition to the provider.
    // model_payload 向 Provider 暴露精确的零工作终态条件。
    let model_payload = json!({
        "tool": WAIT_AGENT_TOOL,
        "parent_session_id": current_session_id.to_string(),
        "message": message,
        "activity_status": "no_active_agents",
        "result": Value::Null,
        "timed_out": false,
        "timeout_ms": timeout_ms,
        "delivered_event_count": 0,
        "terminal_event_count": 0,
        "message_event_count": 0,
        "status_event_count": 0,
        "no_active_agents": true,
        "agents": [],
    });
    // host_payload mirrors the same authority for the WebUI presentation layer.
    // host_payload 为 WebUI 展示层镜像同一份权威状态。
    let host_payload = model_payload.clone();
    VulcanToolInvocationResult::from_model_output(model_payload).with_host_result(
        VulcanToolHostResult {
            kind: String::from("subagent_wait"),
            payload: host_payload,
        },
    )
}

/// Builds the model-visible wait summary for claimed child-agent feedback.
/// 构建已认领子代理反馈的模型可见等待摘要。
///
/// # Parameters
/// 参数
/// `events` are the internal mailbox events that woke this wait call.
/// `events` 是唤醒本次等待调用的内部 mailbox 事件。
///
/// # Returns
/// 返回值
/// A human-readable message and a stable machine-readable status label.
/// 返回人类可读消息与稳定机器可读状态标签。
fn wait_agent_activity_message(events: &[SubagentMailboxEvent]) -> (String, String) {
    if events
        .iter()
        .any(|event| wait_agent_mailbox_event_is_terminal(event.kind))
    {
        return (
            String::from("Subagent terminal result received."),
            String::from("terminal_update"),
        );
    }
    if events
        .iter()
        .any(|event| event.kind == SubagentMailboxEventKind::Message)
    {
        return (
            String::from("Subagent message received."),
            String::from("message_update"),
        );
    }
    if events.iter().any(|event| {
        matches!(
            event.kind,
            SubagentMailboxEventKind::Started | SubagentMailboxEventKind::StatusUpdate
        )
    }) {
        return (
            String::from("Subagent status changed."),
            String::from("status_update"),
        );
    }
    (
        WaitAgentActivityResult::completed().message,
        String::from("mailbox_activity"),
    )
}

/// Converts internal mailbox events into compact diagnostics for wait_agent.
/// 将内部 mailbox 事件转换为 `wait_agent` 使用的紧凑诊断信息。
///
/// # Parameters
/// 参数
/// `events` are the mailbox events claimed by wait_agent.
/// `events` 是被 `wait_agent` 认领的 mailbox 事件。
/// `include_usage_segments` controls whether host-only child usage metadata is retained.
/// `include_usage_segments` 控制是否保留仅供宿主使用的智能体用量元数据。
///
/// # Returns
/// 返回值
/// JSON summaries that include exact child execution ids and feedback text when present.
/// 返回包含精确子执行标识及可用反馈文本的 JSON 摘要。
fn wait_agent_event_summaries(
    events: &[SubagentMailboxEvent],
    include_usage_segments: bool,
) -> Vec<Value> {
    events
        .iter()
        .map(|event| {
            let payload = event.payload.as_object();
            let public_id = payload
                .and_then(|payload| payload.get("public_id"))
                .and_then(Value::as_str)
                .map(str::to_string);
            let status = payload
                .and_then(|payload| payload.get("status"))
                .or_else(|| payload.and_then(|payload| payload.get("task_status")))
                .and_then(Value::as_str)
                .map(str::to_string);
            let display_title = payload
                .and_then(|payload| payload.get("display_title"))
                .and_then(Value::as_str)
                .map(str::to_string);
            let execution_id = wait_agent_payload_text(&event.payload, &["execution_id"]);
            let result =
                wait_agent_payload_text(&event.payload, &["result_text", "result", "message"]);
            let error = wait_agent_payload_text(&event.payload, &["error_text", "error"]);
            let mut summary = json!({
                "agent_id": public_id.clone().unwrap_or_else(|| event.child_thread_id.to_string()),
                "subagent_task_id": event.child_thread_id.to_string(),
                "execution_id": execution_id,
                "display_title": display_title,
                "kind": wait_agent_mailbox_event_kind_label(event.kind),
                "status": status,
                "result": result,
                "error": error,
                "terminal": wait_agent_mailbox_event_is_terminal(event.kind),
                "result_available": wait_agent_event_has_result_payload(&event.payload),
            });
            if include_usage_segments
                && let Some(usage_segments) = event.payload.get("usage_segments")
                && usage_segments.is_array()
                && let Some(summary_object) = summary.as_object_mut()
            {
                summary_object.insert(String::from("usage_segments"), usage_segments.clone());
            }
            summary
        })
        .collect()
}

/// Builds a plain text feedback block from claimed wait_agent events.
/// 从已认领的 wait_agent 事件构建纯文本反馈块。
///
/// # Parameters
/// 参数
/// `events` are the mailbox events consumed by wait_agent.
/// `events` 是 `wait_agent` 消费的 mailbox 事件。
///
/// # Returns
/// 返回值
/// A joined feedback string, or `null` when no feedback text exists.
/// 返回合并后的反馈文本；没有反馈文本时返回 `null`。
fn wait_agent_feedback_text(events: &[SubagentMailboxEvent]) -> Value {
    let lines = events
        .iter()
        .filter_map(|event| {
            let feedback =
                wait_agent_payload_text(&event.payload, &["result_text", "result", "message"])
                    .or_else(|| {
                        wait_agent_payload_text(&event.payload, &["error_text", "error"])
                    })?;
            let name = wait_agent_payload_text(&event.payload, &["display_title"])
                .or_else(|| wait_agent_payload_text(&event.payload, &["public_id"]))
                .unwrap_or_else(|| event.child_thread_id.to_string());
            Some(format!(
                "{name} [{}]: {feedback}",
                wait_agent_mailbox_event_kind_label(event.kind)
            ))
        })
        .collect::<Vec<_>>();
    if lines.is_empty() {
        Value::Null
    } else {
        Value::String(lines.join("\n"))
    }
}

/// Reads a non-empty string field from one wait_agent event payload.
/// 从单个 wait_agent 事件载荷读取非空字符串字段。
///
/// # Parameters
/// 参数
/// `payload` is the structured mailbox event payload.
/// `payload` 是结构化 mailbox 事件载荷。
/// `keys` are the ordered field names to inspect.
/// `keys` 是按顺序检查的字段名。
///
/// # Returns
/// 返回值
/// The first non-empty text value, if present.
/// 返回首个非空文本值。
fn wait_agent_payload_text(payload: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| payload.get(key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Returns whether a mailbox event carries terminal child-agent state.
/// 返回 mailbox 事件是否携带终态子代理状态。
///
/// # Parameters
/// 参数
/// `kind` is the mailbox event kind.
/// `kind` 是 mailbox 事件类型。
///
/// # Returns
/// 返回值
/// `true` for completed, failed, or stopped events.
/// 对完成、失败或停止事件返回 `true`。
pub(super) fn wait_agent_mailbox_event_is_terminal(kind: SubagentMailboxEventKind) -> bool {
    matches!(
        kind,
        SubagentMailboxEventKind::Completed
            | SubagentMailboxEventKind::Failed
            | SubagentMailboxEventKind::Stopped
    )
}

/// Returns whether a mailbox event should satisfy wait_agent.
/// 返回 mailbox 事件是否应满足 wait_agent 等待。
///
/// # Parameters
/// 参数
/// `kind` is the mailbox event kind.
/// `kind` 是 mailbox 事件类型。
///
/// # Returns
/// 返回值
/// `true` for child messages and terminal child-agent results.
/// 对智能体消息与终态智能体结果返回 `true`。
pub(super) fn wait_agent_mailbox_event_is_actionable(kind: SubagentMailboxEventKind) -> bool {
    kind == SubagentMailboxEventKind::Message || wait_agent_mailbox_event_is_terminal(kind)
}

/// Returns the stable JSON label for one mailbox event kind.
/// 返回单个 mailbox 事件类型的稳定 JSON 标签。
///
/// # Parameters
/// 参数
/// `kind` is the mailbox event kind.
/// `kind` 是 mailbox 事件类型。
///
/// # Returns
/// 返回值
/// A snake_case label matching the serialized event contract.
/// 返回与序列化事件契约一致的 snake_case 标签。
fn wait_agent_mailbox_event_kind_label(kind: SubagentMailboxEventKind) -> &'static str {
    match kind {
        SubagentMailboxEventKind::Started => "started",
        SubagentMailboxEventKind::StatusUpdate => "status_update",
        SubagentMailboxEventKind::Message => "message",
        SubagentMailboxEventKind::Completed => "completed",
        SubagentMailboxEventKind::Failed => "failed",
        SubagentMailboxEventKind::Stopped => "stopped",
    }
}

/// Returns whether an event payload contains a non-empty result-like field.
/// 返回事件载荷是否包含非空结果类字段。
///
/// # Parameters
/// 参数
/// `payload` is the structured mailbox event payload.
/// `payload` 是结构化 mailbox 事件载荷。
///
/// # Returns
/// 返回值
/// `true` when a visible result, error, or message payload exists.
/// 存在可见结果、错误或消息载荷时返回 `true`。
fn wait_agent_event_has_result_payload(payload: &Value) -> bool {
    ["result_text", "result", "error_text", "error", "message"]
        .iter()
        .any(|key| {
            payload.get(key).is_some_and(|value| {
                value
                    .as_str()
                    .map(|text| !text.trim().is_empty())
                    .unwrap_or(!value.is_null())
            })
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies progress-only status updates do not satisfy wait_agent.
    /// 验证仅表示进度的状态更新不会满足 wait_agent 等待。
    #[test]
    fn status_update_mailbox_events_do_not_satisfy_wait_agent() {
        assert!(!wait_agent_mailbox_event_is_actionable(
            SubagentMailboxEventKind::Started
        ));
        assert!(!wait_agent_mailbox_event_is_actionable(
            SubagentMailboxEventKind::StatusUpdate
        ));
        assert!(wait_agent_mailbox_event_is_actionable(
            SubagentMailboxEventKind::Message
        ));
        assert!(wait_agent_mailbox_event_is_actionable(
            SubagentMailboxEventKind::Completed
        ));
    }
}
