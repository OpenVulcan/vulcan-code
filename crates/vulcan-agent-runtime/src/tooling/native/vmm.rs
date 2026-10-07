//! VMM native tool implementation.
//! VMM 原生工具实现。

use super::*;

/// Builds the VMM memory-search native tool.
/// 构建 VMM 记忆检索原生工具。
pub(crate) fn build_vmm_memory_search_tool(
    snapshot: Arc<NativeToolRuntimeSnapshot>,
) -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from("vmm_memory_search"),
        name: String::from("vmm_memory_search"),
        description: String::from(
            "Search durable VulcanMemoryMesh memories for the current session's configured user and project. Use this when recalled context is insufficient and you need targeted memory lookup.",
        ),
        help_text: None,
        ai_visible: true,
        available_in_plan: true,
    };

    build_native_tool_definition(
        descriptor,
        vmm_memory_search_args_schema(),
        move |request: NativeToolExecutionRequest| {
            let snapshot = snapshot.clone();
            Box::pin(async move { execute_vmm_memory_search_tool(request, snapshot).await })
        },
    )
}

/// Builds the VMM turn-detail native tool.
/// 构建 VMM turn 详情原生工具。
pub(crate) fn build_vmm_turn_details_tool(
    snapshot: Arc<NativeToolRuntimeSnapshot>,
) -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from("vmm_turn_details"),
        name: String::from("vmm_turn_details"),
        description: String::from(
            "Load structured details for one or more VMM turn ids returned by memory search. Use this before relying on a memory that needs original-dialogue verification.",
        ),
        help_text: None,
        ai_visible: true,
        available_in_plan: true,
    };

    build_native_tool_definition(
        descriptor,
        vmm_turn_details_args_schema(),
        move |request: NativeToolExecutionRequest| {
            let snapshot = snapshot.clone();
            Box::pin(async move { execute_vmm_turn_details_tool(request, snapshot).await })
        },
    )
}

/// Builds the VMM direct memory-write native tool.
/// 构建 VMM 主动写记忆原生工具。
pub(crate) fn build_vmm_memory_write_tool(
    snapshot: Arc<NativeToolRuntimeSnapshot>,
) -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from("vmm_memory_write"),
        name: String::from("vmm_memory_write"),
        description: String::from(
            "Write explicit durable memories into VulcanMemoryMesh for the current configured user and project. Use only for stable facts, durable preferences, or user-approved memory candidates.",
        ),
        help_text: Some(vmm_memory_write_usage_help_markdown().to_string()),
        ai_visible: true,
        available_in_plan: false,
    };

    build_native_tool_definition(
        descriptor,
        vmm_memory_write_args_schema(),
        move |request: NativeToolExecutionRequest| {
            let snapshot = snapshot.clone();
            Box::pin(async move { execute_vmm_memory_write_tool(request, snapshot).await })
        },
    )
}

/// Builds the VMM memory-delete native tool.
/// 构建 VMM 记忆删除原生工具。
pub(crate) fn build_vmm_memory_delete_tool(
    snapshot: Arc<NativeToolRuntimeSnapshot>,
) -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from("vmm_memory_delete"),
        name: String::from("vmm_memory_delete"),
        description: String::from(
            "Delete explicit VulcanMemoryMesh memory ids for the current configured user and project. Use only when the user asked to remove a memory or a recalled memory is clearly wrong, stale, or harmful.",
        ),
        help_text: Some(vmm_memory_delete_usage_help_markdown().to_string()),
        ai_visible: true,
        available_in_plan: false,
    };

    build_native_tool_definition(
        descriptor,
        vmm_memory_delete_args_schema(),
        move |request: NativeToolExecutionRequest| {
            let snapshot = snapshot.clone();
            Box::pin(async move { execute_vmm_memory_delete_tool(request, snapshot).await })
        },
    )
}

/// One typed argument payload accepted by `vmm_memory_search`.
/// `vmm_memory_search` 接受的一份强类型参数载荷。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct VmmMemorySearchToolArgs {
    /// Natural-language memory lookup queries.
    /// 自然语言记忆检索查询集合。
    queries: Vec<String>,
    /// Optional maximum hits per query.
    /// 每条查询可选的最大命中数量。
    top_k: Option<u32>,
}

/// One typed argument payload accepted by `vmm_turn_details`.
/// `vmm_turn_details` 接受的一份强类型参数载荷。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct VmmTurnDetailsToolArgs {
    /// Turn identifiers returned by VMM memory search.
    /// VMM 记忆检索返回的 turn 标识集合。
    #[serde(rename = "turnIds")]
    turn_ids: Vec<u64>,
}

/// One typed direct-memory item accepted by `vmm_memory_write`.
/// `vmm_memory_write` 接收的一条强类型主动记忆项。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct VmmMemoryWriteToolItemArgs {
    /// Compact memory abstract.
    /// 精简记忆摘要。
    #[serde(rename = "abstract")]
    abstract_text: String,
    /// Full memory details.
    /// 完整记忆详情。
    details: String,
    /// Numeric memory category.
    /// 数字记忆分类。
    category: i32,
    /// Optional VMM scope level.
    /// 可选 VMM 作用域级别。
    #[serde(rename = "scopeLevel")]
    scope_level: Option<i32>,
    /// Optional VMM priority.
    /// 可选 VMM 优先级。
    priority: Option<i32>,
    /// Optional VMM memory level.
    /// 可选 VMM 记忆层级。
    #[serde(rename = "memoryLevel")]
    memory_level: Option<i32>,
}

/// One typed argument payload accepted by `vmm_memory_write`.
/// `vmm_memory_write` 接受的一份强类型参数载荷。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct VmmMemoryWriteToolArgs {
    /// Direct memory items to persist.
    /// 需要持久化的主动记忆项集合。
    items: Vec<VmmMemoryWriteToolItemArgs>,
}

/// One typed argument payload accepted by `vmm_memory_delete`.
/// `vmm_memory_delete` 接受的一份强类型参数载荷。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct VmmMemoryDeleteToolArgs {
    /// Durable VMM memory ids to delete.
    /// 需要删除的长期 VMM 记忆标识集合。
    #[serde(rename = "memoryIds")]
    memory_ids: Vec<u64>,
    /// Deletion reason used for audit logs.
    /// 用于审计日志的删除原因。
    reason: String,
}

fn vmm_memory_search_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "queries": {
                "type": "array",
                "items": { "type": "string" },
                "minItems": 1,
                "description": "One or more concise natural-language memory lookup queries."
            },
            "top_k": {
                "type": "integer",
                "minimum": 1,
                "maximum": 20,
                "description": "Optional maximum hit count per query. Default: 5."
            }
        },
        "required": ["queries"],
        "additionalProperties": false
    })
}

/// Builds the JSON Schema advertised to the model for `vmm_turn_details`.
/// 为 `vmm_turn_details` 构建对模型声明的 JSON Schema。
fn vmm_turn_details_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "turnIds": {
                "type": "array",
                "items": { "type": "integer", "minimum": 1 },
                "minItems": 1,
                "description": "One or more VMM turn ids returned by memory search."
            }
        },
        "required": ["turnIds"],
        "additionalProperties": false
    })
}

/// Returns Markdown usage guidance for the VMM direct memory-write tool.
/// 返回 VMM 主动写记忆工具的 Markdown 使用说明。
fn vmm_memory_write_usage_help_markdown() -> &'static str {
    r#"Use `vmm_memory_write` only for explicit durable memories: project facts, architecture/API decisions, long-lived requirements, verified bug/debt notes, and specific facts or preferences the user explicitly asks to persist. It writes memory rows only; it does not create a turn, timeline, or profile update by itself.

Do not proactively write user or system profile information; automatic profile extraction owns that data. Information the user explicitly asks to remember may be written.

Avoid writing transient current-turn context, duplicate recalled memories, secrets, credentials, or speculative claims. When a memory was retrieved from VMM, do not write it back unless the user corrects, confirms, or materially extends it.

Each `items[]` entry requires:
- `abstract`: compact search-facing summary.
- `details`: complete durable memory body with enough context to stand alone.
- `category`: numeric category: `0 = general`, `1 = architecture_decision`, `2 = tech_spec_api`, `3 = business_logic`, `4 = requirement_todo`, `5 = project_context`, `6 = logical_bug_debt`, `7 = security_policy`.

Optional numeric fields:
- `scopeLevel`: `0 = VMM default`, `1 = session`, `2 = project`, `3 = user`. Prefer `2` for project facts. Use `3` only for a user-scoped item the user explicitly requested to persist.
- `priority`: `0 = VMM default`, `1 = P0/highest`, `2 = P1/ordinary durable`, `3 = P2/weak or tentative`.
- `memoryLevel`: `0 = VMM infer/default`, `1 = L0`, `2 = L1`, `3 = L2`, `4 = L3`. Prefer `2` for ordinary durable project/user memories unless the user or VMM policy indicates otherwise."#
}

/// Builds the JSON Schema advertised to the model for `vmm_memory_write`.
/// 为 `vmm_memory_write` 构建对模型声明的 JSON Schema。
pub(crate) fn vmm_memory_write_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "items": {
                "type": "array",
                "minItems": 1,
                "items": {
                    "type": "object",
                    "properties": {
                        "abstract": {
                            "type": "string",
                            "description": "Compact durable memory abstract."
                        },
                        "details": {
                            "type": "string",
                            "description": "Full durable memory details."
                        },
                        "category": {
                            "type": "integer",
                            "minimum": 0,
                            "maximum": 7,
                            "description": "Numeric VMM memory category: 0=general, 1=architecture_decision, 2=tech_spec_api, 3=business_logic, 4=requirement_todo, 5=project_context, 6=logical_bug_debt, 7=security_policy."
                        },
                        "scopeLevel": {
                            "type": "integer",
                            "minimum": 0,
                            "maximum": 3,
                            "description": "Optional VMM scope level: 0=VMM default, 1=session, 2=project, 3=user."
                        },
                        "priority": {
                            "type": "integer",
                            "minimum": 0,
                            "maximum": 3,
                            "description": "Optional VMM priority: 0=VMM default, 1=P0/highest, 2=P1/ordinary durable, 3=P2/weak or tentative."
                        },
                        "memoryLevel": {
                            "type": "integer",
                            "minimum": 0,
                            "maximum": 4,
                            "description": "Optional VMM memory level: 0=VMM infer/default, 1=L0, 2=L1, 3=L2, 4=L3."
                        }
                    },
                    "required": ["abstract", "details", "category"],
                    "additionalProperties": false
                },
                "description": "Direct memory items to persist."
            }
        },
        "required": ["items"],
        "additionalProperties": false
    })
}

/// Returns Markdown usage guidance for the VMM memory-delete tool.
/// 返回 VMM 删除记忆工具的 Markdown 使用说明。
fn vmm_memory_delete_usage_help_markdown() -> &'static str {
    r#"Use `vmm_memory_delete` only for explicit durable memory ids. Valid reasons include: the user explicitly asked to remove a memory, a recalled memory is contradicted by newer user-approved information, or the memory contains wrong/stale/harmful data.

Prefer deleting by `MEMORY_ID` shown in recalled VMM context or by `memory_id` returned from `vmm_memory_search`. Do not delete memories just because they are irrelevant to the current turn. Always include a concise audit reason."#
}

/// Builds the JSON Schema advertised to the model for `vmm_memory_delete`.
/// 为 `vmm_memory_delete` 构建对模型声明的 JSON Schema。
fn vmm_memory_delete_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "memoryIds": {
                "type": "array",
                "items": { "type": "integer", "minimum": 1 },
                "minItems": 1,
                "maxItems": 256,
                "description": "One or more durable VMM memory ids to delete. Use MEMORY_ID labels from recalled context or memory_id values returned by vmm_memory_search."
            },
            "reason": {
                "type": "string",
                "maxLength": 1024,
                "description": "Concise deletion reason for audit logs."
            }
        },
        "required": ["memoryIds", "reason"],
        "additionalProperties": false
    })
}

fn parse_vmm_memory_search_arguments(
    arguments: Value,
) -> Result<VmmMemorySearchToolArgs, NativeToolError> {
    let mut parsed =
        serde_json::from_value::<VmmMemorySearchToolArgs>(arguments).map_err(|error| {
            NativeToolError::new(format!(
                "Failed to parse vmm_memory_search arguments: {error}"
            ))
        })?;
    parsed.queries = parsed
        .queries
        .into_iter()
        .map(|query| query.trim().to_string())
        .filter(|query| !query.is_empty())
        .collect();
    if parsed.queries.is_empty() {
        return Err(NativeToolError::new(
            "The `queries` argument for vmm_memory_search cannot be empty.",
        ));
    }
    parsed.top_k = Some(parsed.top_k.unwrap_or(5).clamp(1, 20));
    Ok(parsed)
}

/// Parses and validates VMM turn-detail arguments.
/// 解析并校验 VMM turn 详情参数。
fn parse_vmm_turn_details_arguments(
    arguments: Value,
) -> Result<VmmTurnDetailsToolArgs, NativeToolError> {
    let parsed = serde_json::from_value::<VmmTurnDetailsToolArgs>(arguments).map_err(|error| {
        NativeToolError::new(format!(
            "Failed to parse vmm_turn_details arguments: {error}"
        ))
    })?;
    if parsed.turn_ids.is_empty() || parsed.turn_ids.contains(&0) {
        return Err(NativeToolError::new(
            "The `turnIds` argument for vmm_turn_details must contain positive turn ids.",
        ));
    }
    Ok(parsed)
}

/// Parses and validates VMM direct memory-write arguments.
/// 解析并校验 VMM 主动写记忆参数。
fn parse_vmm_memory_write_arguments(
    arguments: Value,
) -> Result<VmmMemoryWriteToolArgs, NativeToolError> {
    let parsed = serde_json::from_value::<VmmMemoryWriteToolArgs>(arguments).map_err(|error| {
        NativeToolError::new(format!(
            "Failed to parse vmm_memory_write arguments: {error}"
        ))
    })?;
    if parsed.items.is_empty() {
        return Err(NativeToolError::new(
            "The `items` argument for vmm_memory_write cannot be empty.",
        ));
    }
    for item in &parsed.items {
        if item.abstract_text.trim().is_empty() || item.details.trim().is_empty() {
            return Err(NativeToolError::new(
                "`abstract` and `details` are required for every vmm_memory_write item.",
            ));
        }
    }
    Ok(parsed)
}

/// Parses and validates VMM memory-delete arguments.
/// 解析并校验 VMM 记忆删除参数。
fn parse_vmm_memory_delete_arguments(
    arguments: Value,
) -> Result<VmmMemoryDeleteToolArgs, NativeToolError> {
    let mut parsed =
        serde_json::from_value::<VmmMemoryDeleteToolArgs>(arguments).map_err(|error| {
            NativeToolError::new(format!(
                "Failed to parse vmm_memory_delete arguments: {error}"
            ))
        })?;
    parsed.memory_ids = normalized_vmm_memory_delete_ids(parsed.memory_ids)?;
    parsed.reason = parsed.reason.trim().to_string();
    if parsed.reason.is_empty() {
        return Err(NativeToolError::new(
            "The `reason` argument for vmm_memory_delete cannot be empty.",
        ));
    }
    parsed.reason = parsed.reason.chars().take(1024).collect();
    Ok(parsed)
}

/// Normalizes memory-delete ids by removing zeroes and duplicates.
/// 通过移除零值和重复项归一化记忆删除标识。
fn normalized_vmm_memory_delete_ids(memory_ids: Vec<u64>) -> Result<Vec<u64>, NativeToolError> {
    let mut normalized = Vec::new();
    for memory_id in memory_ids {
        if memory_id == 0 || normalized.contains(&memory_id) {
            continue;
        }
        normalized.push(memory_id);
        if normalized.len() > 256 {
            return Err(NativeToolError::new(
                "The `memoryIds` argument for vmm_memory_delete cannot contain more than 256 ids.",
            ));
        }
    }
    if normalized.is_empty() {
        return Err(NativeToolError::new(
            "The `memoryIds` argument for vmm_memory_delete must contain positive memory ids.",
        ));
    }
    Ok(normalized)
}

/// Resolves the effective VMM config for one native tool invocation.
/// 为一次原生工具调用解析最终 VMM 配置。
fn effective_vmm_config_for_native_request(
    request: &NativeToolExecutionRequest,
    snapshot: &NativeToolRuntimeSnapshot,
) -> Result<VmmEffectiveConfig, NativeToolError> {
    let config = VmmEffectiveConfig::resolve(
        &snapshot.vmm_global,
        Some(&request.project_vmm),
        Some(&request.session_vmm),
    );
    if !config.ai_tools_enabled {
        return Err(NativeToolError::new(
            "VMM AI tools are disabled for the current project or session.",
        ));
    }
    match config.diagnose_memory_flow(true) {
        VmmConfigDiagnostic::Ready => Ok(config),
        diagnostic => Err(NativeToolError::new(format!(
            "VMM memory tools are not ready for the current session: {diagnostic:?}"
        ))),
    }
}

/// Connects a VMM client for one native tool invocation.
/// 为一次原生工具调用连接 VMM 客户端。
async fn connect_vmm_native_client(
    config: &VmmEffectiveConfig,
) -> Result<Arc<dyn VmmClient>, NativeToolError> {
    match config.backend_mode {
        VmmBackendMode::Native => {
            process_native_vmm_client().map_err(|error| NativeToolError::new(error.to_string()))
        }
        VmmBackendMode::ExternalGrpc => {
            TonicVmmClient::connect(VmmTransportConfig::from_effective(config))
                .await
                .map(|client| Arc::new(client) as Arc<dyn VmmClient>)
                .map_err(|error| NativeToolError::new(error.to_string()))
        }
    }
}

/// Resolves trusted native caller context without exposing lifecycle bypass controls to AI arguments.
/// 解析可信原生调用方上下文，不向 AI 参数暴露生命周期绕过开关。
/// Native calls require the execution's session ID; external calls retain their existing transport shape.
/// 原生调用要求执行所属的会话 ID；外置调用保持既有传输结构。
fn native_memory_runtime_session(
    session_id: Option<SessionId>,
    mode: VmmBackendMode,
) -> Result<Option<String>, NativeToolError> {
    match mode {
        VmmBackendMode::Native => session_id
            .map(|session| Some(session.to_string()))
            .ok_or_else(|| {
                NativeToolError::new("Native VMM tools require an exact runtime session id.")
            }),
        VmmBackendMode::ExternalGrpc => Ok(None),
    }
}

/// Checks trusted native session propagation and the unchanged sessionless external boundary.
/// 检查可信原生会话传递及保持不变的外置无会话边界。
#[cfg(test)]
#[test]
fn native_memory_runtime_session_is_required_only_for_native_tools() {
    // Use a stable host session identity to assert exact propagation, not merely presence.
    // 使用稳定宿主会话身份断言准确传递，而不仅断言存在。
    let session = "423db0fa-7bfb-4844-8ebc-a1eef75971fe"
        .parse()
        .expect("session id");
    assert_eq!(
        native_memory_runtime_session(Some(session), VmmBackendMode::Native)
            .expect("native context"),
        Some(session.to_string())
    );
    assert!(native_memory_runtime_session(None, VmmBackendMode::Native).is_err());
    assert_eq!(
        native_memory_runtime_session(Some(session), VmmBackendMode::ExternalGrpc)
            .expect("external context"),
        None
    );
    assert_eq!(
        native_memory_runtime_session(None, VmmBackendMode::ExternalGrpc)
            .expect("external without session"),
        None
    );
}

mod result_markdown;

async fn execute_vmm_memory_search_tool(
    request: NativeToolExecutionRequest,
    snapshot: Arc<NativeToolRuntimeSnapshot>,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let parsed_args = parse_vmm_memory_search_arguments(request.arguments.clone())?;
    let config = effective_vmm_config_for_native_request(&request, &snapshot)?;
    let client = connect_vmm_native_client(&config).await?;
    let response = client
        .search_memory_events(SearchMemoryEventsRequest {
            runtime_session_id: native_memory_runtime_session(
                request.session_id,
                config.backend_mode,
            )?,
            user_id: config.user_id.unwrap_or_default(),
            project_id: config.project_id.unwrap_or_default(),
            host_project_id: request.host_project_id.clone().unwrap_or_default(),
            queries: parsed_args.queries,
            top_k: parsed_args.top_k.unwrap_or(5),
        })
        .await
        .map_err(|error| NativeToolError::new(error.to_string()))?;

    Ok(VulcanToolInvocationResult::from_model_output(
        Value::String(result_markdown::search(&response)),
    ))
}

/// Executes the host-owned `vmm_turn_details` tool.
/// 执行宿主自有 `vmm_turn_details` 工具。
async fn execute_vmm_turn_details_tool(
    request: NativeToolExecutionRequest,
    snapshot: Arc<NativeToolRuntimeSnapshot>,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let parsed_args = parse_vmm_turn_details_arguments(request.arguments.clone())?;
    let config = effective_vmm_config_for_native_request(&request, &snapshot)?;
    let client = connect_vmm_native_client(&config).await?;
    let response = client
        .get_turn_details(GetTurnDetailsRequest {
            runtime_session_id: native_memory_runtime_session(
                request.session_id,
                config.backend_mode,
            )?,
            user_id: config.user_id.unwrap_or_default(),
            project_id: config.project_id.unwrap_or_default(),
            host_project_id: request.host_project_id.clone().unwrap_or_default(),
            turn_ids: parsed_args.turn_ids,
        })
        .await
        .map_err(|error| NativeToolError::new(error.to_string()))?;

    Ok(VulcanToolInvocationResult::from_model_output(
        Value::String(result_markdown::turns(&response)),
    ))
}

/// Executes the host-owned `vmm_memory_write` tool.
/// 执行宿主自有 `vmm_memory_write` 工具。
async fn execute_vmm_memory_write_tool(
    request: NativeToolExecutionRequest,
    snapshot: Arc<NativeToolRuntimeSnapshot>,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let parsed_args = parse_vmm_memory_write_arguments(request.arguments.clone())?;
    let session_id = request
        .session_id
        .map(|session_id| session_id.to_string())
        .filter(|session_id| !session_id.trim().is_empty())
        .ok_or_else(|| {
            NativeToolError::new(
                "vmm_memory_write requires a real session id for durable attribution.",
            )
        })?;
    let config = effective_vmm_config_for_native_request(&request, &snapshot)?;
    let client = connect_vmm_native_client(&config).await?;
    let response = client
        .write_memories(WriteMemoriesRequest {
            session_id,
            user_id: config.user_id.unwrap_or_default(),
            project_id: config.project_id.unwrap_or_default(),
            host_project_id: request.host_project_id.clone().unwrap_or_default(),
            items: parsed_args
                .items
                .into_iter()
                .map(|item| WriteMemoryItem {
                    scope_level: item.scope_level.unwrap_or(0),
                    abstract_text: item.abstract_text.trim().to_string(),
                    details: item.details.trim().to_string(),
                    category: item.category,
                    priority: item.priority.unwrap_or(0),
                    memory_level: item.memory_level.unwrap_or(0),
                })
                .collect(),
        })
        .await
        .map_err(|error| NativeToolError::new(error.to_string()))?;

    Ok(VulcanToolInvocationResult::from_model_output(
        Value::String(result_markdown::written(&response)),
    ))
}

/// Executes the host-owned `vmm_memory_delete` tool.
/// 执行宿主自有 `vmm_memory_delete` 工具。
async fn execute_vmm_memory_delete_tool(
    request: NativeToolExecutionRequest,
    snapshot: Arc<NativeToolRuntimeSnapshot>,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let parsed_args = parse_vmm_memory_delete_arguments(request.arguments.clone())?;
    let config = effective_vmm_config_for_native_request(&request, &snapshot)?;
    let client = connect_vmm_native_client(&config).await?;
    let response = client
        .delete_memories(DeleteMemoriesRequest {
            runtime_session_id: native_memory_runtime_session(
                request.session_id,
                config.backend_mode,
            )?,
            user_id: config.user_id.unwrap_or_default(),
            project_id: config.project_id.unwrap_or_default(),
            host_project_id: request.host_project_id.clone().unwrap_or_default(),
            memory_ids: parsed_args.memory_ids,
            reason: parsed_args.reason,
        })
        .await
        .map_err(vmm_memory_delete_native_error)?;

    Ok(VulcanToolInvocationResult::from_model_output(
        Value::String(result_markdown::deleted(&response)),
    ))
}

/// Converts VMM memory-delete errors into model-facing native tool errors.
/// 将 VMM 记忆删除错误转换为面向模型的原生工具错误。
fn vmm_memory_delete_native_error(error: vulcan_vmm::VmmClientError) -> NativeToolError {
    if error.is_unimplemented() {
        NativeToolError::new(
            "The connected VMM service does not support DeleteMemories. Update VulcanMemoryMesh before using vmm_memory_delete.",
        )
    } else {
        NativeToolError::new(error.to_string())
    }
}

/// Verifies memory tool arguments reject undeclared fields, including inside write items.
/// 验证记忆工具参数拒绝未声明字段，包括写入条目内部。
#[cfg(test)]
#[test]
fn vmm_tool_arguments_reject_unknown_fields() {
    assert!(
        serde_json::from_value::<VmmTurnDetailsToolArgs>(serde_json::json!({"turn_ids": [1]}))
            .is_err()
    );
    assert!(
        serde_json::from_value::<VmmMemoryWriteToolArgs>(serde_json::json!({
            "items": [{"abstract": "a", "details": "d", "category": 1, "scope_level": 2}]
        }))
        .is_err()
    );
    serde_json::from_value::<VmmMemoryWriteToolArgs>(serde_json::json!({
        "items": [{"abstract": "a", "details": "d", "category": 1, "scopeLevel": 2}]
    }))
    .expect("declared fields stay accepted");
}
