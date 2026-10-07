//! Host-owned AI tool adapters for LuaSkills one-shot and persistent Lua execution.
//! 面向 LuaSkills 一次性与持久 Lua 执行的宿主自有 AI 工具适配器。

use super::*;
use serde::Deserialize;
use std::io::Write;
use std::sync::atomic::Ordering;
use std::time::Instant;
use tempfile::{Builder as TempFileBuilder, NamedTempFile};
use vulcan_tools_manager::{
    VulcanPlanToolPolicy, VulcanToolCandidate, VulcanToolDescriptor, VulcanToolHostResult,
    VulcanToolInvocationResult, VulcanToolOrigin, VulcanToolSourceKind,
};

/// Stable public tool identifier for one-shot Lua execution.
/// 一次性 Lua 执行的稳定公开工具标识。
pub const RUNLUA_TOOL_NAME: &str = "runlua";

/// Stable public tool identifier for persistent Lua runtime leases.
/// 持久 Lua 运行时租约的稳定公开工具标识。
pub const RUNLUA_SESSION_TOOL_NAME: &str = "runlua_session";

/// Stable host-result kind consumed by the dedicated one-shot Lua presentation.
/// 由专用一次性 Lua 展示消费的稳定宿主结果类型。
pub const RUNLUA_HOST_RESULT_KIND: &str = "lua_execution";

/// Stable host-result kind consumed by the persistent Lua session presentation.
/// 由持久 Lua 会话展示消费的稳定宿主结果类型。
pub const RUNLUA_SESSION_HOST_RESULT_KIND: &str = "lua_runtime_session";

/// Maximum model-visible RunLua output retained inline before page-mode spill.
/// RunLua 输出进入分页落盘前允许内联保留的最大字节数。
const RUNLUA_INLINE_RESULT_BYTES: u64 = 64 * 1024;

/// Maximum one-shot or session-eval Lua instruction timeout accepted from AI.
/// AI 可请求的一次性或会话执行 Lua 指令最大超时。
const RUNLUA_MAX_TIMEOUT_MS: u64 = 60_000;

/// Stable default Lua instruction timeout used when the caller omits one.
/// 调用方省略时使用的稳定 Lua 指令默认超时。
const RUNLUA_DEFAULT_TIMEOUT_MS: u64 = 60_000;

/// Stable default idle lease lifetime for persistent public Lua sessions.
/// 持久公开 Lua 会话的稳定默认闲置租约生命周期。
const RUNLUA_DEFAULT_SESSION_TTL_SEC: u64 = 600;

/// Maximum idle lease lifetime accepted by the LuaSkills public lease API.
/// LuaSkills 公开租约 API 接受的最大闲置租约生命周期。
const RUNLUA_MAX_SESSION_TTL_SEC: u64 = 3_600;

/// Maximum persistent Lua sessions owned by one conversation.
/// 单个会话允许持有的最大持久 Lua 会话数量。
const RUNLUA_MAX_SESSIONS_PER_OWNER: usize = 4;

/// Maximum AI-provided Lua source size accepted by either public tool.
/// 两个公开工具允许接收的 AI Lua 源码最大字节数。
const RUNLUA_MAX_CODE_BYTES: usize = 128 * 1024;

/// Concise provider-facing guidance for the isolated RunLua tool.
/// 面向 Provider 的一次性 RunLua 工具简短指引。
const RUNLUA_TOOL_DESCRIPTION: &str = "Execute one bounded Lua snippet in the LuaSkills runtime. Before first use, call the native `skill` tool with {\"name\":\"runlua\"}; do not use `luaskill` to retrieve this guide.";

/// Concise provider-facing guidance for the persistent RunLua session tool.
/// 面向 Provider 的持久 RunLua 会话工具简短指引。
const RUNLUA_SESSION_TOOL_DESCRIPTION: &str = "Manage stateful Lua runtime leases for multi-step execution. Before first use, call the native `skill` tool with {\"name\":\"runlua\"}; do not use `luaskill` to retrieve this guide.";

/// One-shot RunLua arguments decoded from the provider tool call.
/// 从 Provider 工具调用解码的一次性 RunLua 参数。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunLuaToolArgs {
    /// Short model-authored task label rendered in the conversation card.
    /// 在会话卡片中渲染的模型生成简短任务标签。
    task: String,
    /// Inline Lua source executed in an isolated pooled VM.
    /// 在隔离池化 VM 中执行的内联 Lua 源码。
    code: String,
    /// Structured object exposed to Lua as local `args`.
    /// 以局部变量 `args` 暴露给 Lua 的结构化对象。
    #[serde(default = "empty_json_object")]
    args: Value,
    /// Lua instruction timeout in milliseconds.
    /// Lua 指令执行超时毫秒数。
    #[serde(default = "default_runlua_timeout_ms")]
    timeout_ms: u64,
}

/// Supported persistent RunLua session actions.
/// 持久 RunLua 会话支持的动作。
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum RunLuaSessionAction {
    /// Lists active sessions owned by the current conversation.
    /// 列出当前会话拥有的活跃 Session。
    List,
    /// Starts one persistent Lua runtime lease.
    /// 启动一个持久 Lua 运行时租约。
    Start,
    /// Evaluates Lua source inside an existing persistent lease.
    /// 在已有持久租约中执行 Lua 源码。
    Eval,
    /// Reads the exact upstream status of one owned lease.
    /// 读取一个已归属租约的精确上游状态。
    Status,
    /// Closes one owned lease and releases its VM.
    /// 关闭一个已归属租约并释放其 VM。
    Close,
}

impl RunLuaSessionAction {
    /// Returns the stable model- and UI-facing action label.
    /// 返回稳定的模型与界面动作标签。
    fn as_str(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Start => "start",
            Self::Eval => "eval",
            Self::Status => "status",
            Self::Close => "close",
        }
    }
}

/// Flat persistent RunLua session arguments validated according to `action`.
/// 根据 `action` 校验的扁平持久 RunLua 会话参数。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunLuaSessionToolArgs {
    /// Exact lifecycle action requested by the caller.
    /// 调用方请求的精确生命周期动作。
    action: RunLuaSessionAction,
    /// Human-oriented name required by `start`.
    /// `start` 动作要求的面向使用者名称。
    #[serde(default)]
    name: Option<String>,
    /// Opaque lease identifier required by `eval`, `status`, and `close`.
    /// `eval`、`status` 与 `close` 要求的不透明租约标识。
    #[serde(default)]
    session_id: Option<String>,
    /// Inline Lua source required by `eval`.
    /// `eval` 动作要求的内联 Lua 源码。
    #[serde(default)]
    code: Option<String>,
    /// Structured object exposed to Lua as local `args` during `eval`.
    /// `eval` 期间以局部变量 `args` 暴露给 Lua 的结构化对象。
    #[serde(default = "empty_json_object")]
    args: Value,
    /// Finite idle lifetime requested by `start`.
    /// `start` 动作请求的有限闲置生命周期。
    #[serde(default = "default_runlua_session_ttl_sec")]
    ttl_sec: u64,
    /// Lua instruction timeout requested by `eval`.
    /// `eval` 动作请求的 Lua 指令超时。
    #[serde(default = "default_runlua_timeout_ms")]
    timeout_ms: u64,
}

/// Compact request fields retained for the persistent Lua conversation presentation.
/// 为持久 Lua 会话展示保留的紧凑请求字段。
#[derive(Debug)]
struct RunLuaSessionPresentationContext {
    /// Optional human-oriented session name from a start action.
    /// start 动作提供的可选人类可读会话名称。
    name: Option<String>,
    /// Optional owner-validated session identifier from a lifecycle action.
    /// 生命周期动作提供的可选且已校验归属的会话标识。
    session_id: Option<String>,
    /// Optional bounded Lua source preview from an eval action.
    /// eval 动作提供的可选有界 Lua 源码预览。
    code_preview: Option<String>,
}

/// One atomic owner/name reservation retained until an upstream start commits or fails.
/// 一条原子所有者/名称预留，持续到上游启动提交或失败为止。
struct RunLuaSessionStartReservation {
    /// Shared runtime handle that owns the reservation registry.
    /// 持有预留注册表的共享运行时句柄。
    handle: LuaSkillsRuntimeHandle,
    /// Exact conversation owner used by the reservation key.
    /// 预留键使用的精确会话所有者。
    owner_session_id: String,
    /// Exact normalized session name used by the reservation key.
    /// 预留键使用的精确规范化会话名称。
    name: String,
}

impl Drop for RunLuaSessionStartReservation {
    /// Releases an uncommitted owner/name reservation on every exit path.
    /// 在所有退出路径释放尚未提交的所有者/名称预留。
    fn drop(&mut self) {
        let mut registry = self
            .handle
            .runlua_sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        registry
            .pending_starts
            .remove(&(self.owner_session_id.clone(), self.name.clone()));
    }
}

/// Returns one fresh JSON object used by omitted structured arguments.
/// 为省略的结构化参数返回一个新的 JSON 对象。
fn empty_json_object() -> Value {
    Value::Object(Map::new())
}

/// Creates one temporary Lua entry whose parent is the exact execution workspace.
/// 创建一个临时 Lua 入口文件，其父目录就是精确执行工作区。
///
/// `workspace_root` is the host-selected project directory; an absent workspace uses the OS temp directory.
/// `workspace_root` 是宿主选定的项目目录；没有工作区时使用操作系统临时目录。
///
/// Returns an auto-deleting file retained for the complete upstream execution.
/// 返回一个在完整上游执行期间保留、随后自动删除的文件。
fn create_runlua_workspace_source_file(
    workspace_root: Option<&Path>,
) -> Result<NamedTempFile, String> {
    let mut builder = TempFileBuilder::new();
    builder.prefix(".vulcan-runlua-").suffix(".lua");
    match workspace_root {
        Some(workspace_root) => {
            let canonical_workspace_root = fs::canonicalize(workspace_root).map_err(|error| {
                format!(
                    "Failed to canonicalize RunLua workspace `{}`: {error}",
                    workspace_root.display()
                )
            })?;
            if !canonical_workspace_root.is_dir() {
                return Err(format!(
                    "RunLua workspace `{}` is not a directory.",
                    canonical_workspace_root.display()
                ));
            }
            builder
                .tempfile_in(&canonical_workspace_root)
                .map_err(|error| {
                    format!(
                        "Failed to create a RunLua source file in workspace `{}`: {error}",
                        canonical_workspace_root.display()
                    )
                })
        }
        None => builder
            .tempfile()
            .map_err(|error| format!("Failed to create an isolated RunLua source file: {error}")),
    }
}

/// Returns the stable default one-shot and eval timeout.
/// 返回稳定的一次性与 eval 默认超时。
fn default_runlua_timeout_ms() -> u64 {
    RUNLUA_DEFAULT_TIMEOUT_MS
}

/// Returns the stable default persistent Lua session TTL.
/// 返回稳定的持久 Lua 会话默认 TTL。
fn default_runlua_session_ttl_sec() -> u64 {
    RUNLUA_DEFAULT_SESSION_TTL_SEC
}

/// Returns whether isolated RunLua execution is currently enabled for provider exposure.
/// 返回隔离 RunLua 执行当前是否允许向 Provider 暴露。
pub fn runlua_ai_tool_enabled(handle: &LuaSkillsRuntimeHandle) -> bool {
    handle.runlua_ai_tool_enabled.load(Ordering::Acquire)
}

/// Returns whether persistent RunLua sessions are currently enabled for provider exposure.
/// 返回持久 RunLua 会话当前是否允许向 Provider 暴露。
pub fn runlua_session_ai_tool_enabled(handle: &LuaSkillsRuntimeHandle) -> bool {
    handle
        .runlua_session_ai_tool_enabled
        .load(Ordering::Acquire)
}

/// Builds both provider-facing RunLua candidates when the host exposure switch is enabled.
/// 当宿主曝光开关启用时构建两个面向 Provider 的 RunLua 候选工具。
pub fn build_runlua_tool_candidates(
    handle: LuaSkillsRuntimeHandle,
    workspace_root: Option<PathBuf>,
) -> Vec<VulcanToolCandidate> {
    let mut candidates = Vec::with_capacity(2);
    if runlua_ai_tool_enabled(&handle) {
        // one_shot_handle retains the exact engine and live exposure switch for invocation-time checks.
        // one_shot_handle 为调用期校验保留精确引擎与实时曝光开关。
        let one_shot_handle = handle.clone();
        // one_shot_workspace_root pins page-mode spill files to the current project.
        // one_shot_workspace_root 将分页落盘文件固定到当前项目。
        let one_shot_workspace_root = workspace_root.clone();
        candidates.push(VulcanToolCandidate {
            descriptor: VulcanToolDescriptor::new(
                RUNLUA_TOOL_NAME,
                RUNLUA_TOOL_NAME,
                RUNLUA_TOOL_DESCRIPTION,
                None,
                VulcanToolOrigin::new(
                    VulcanToolSourceKind::LuaSkills,
                    "luaskills",
                    RUNLUA_TOOL_NAME,
                ),
            ),
            parameters_schema: runlua_args_schema(),
            ai_visible: true,
            plan_policy: VulcanPlanToolPolicy::Hidden,
            request_streaming: None,
            invoke: Arc::new(move |arguments, _owner_session_id| {
                // invocation_handle isolates one cloned long-lived runtime handle for this future.
                // invocation_handle 为当前 Future 隔离一份克隆的长生命周期运行时句柄。
                let invocation_handle = one_shot_handle.clone();
                // invocation_workspace_root preserves the project-specific spill destination.
                // invocation_workspace_root 保留项目专用分页落盘位置。
                let invocation_workspace_root = one_shot_workspace_root.clone();
                Box::pin(async move {
                    execute_runlua_tool(invocation_handle, invocation_workspace_root, arguments)
                        .await
                })
            }),
        });
    }
    if runlua_session_ai_tool_enabled(&handle) {
        // session_handle retains the shared lease manager and host ownership registry.
        // session_handle 保留共享租约管理器与宿主归属注册表。
        let session_handle = handle;
        // session_workspace_root supplies the only permitted cwd/workspace identity for `start`.
        // session_workspace_root 为 `start` 提供唯一允许的 cwd/workspace 身份。
        let session_workspace_root = workspace_root;
        candidates.push(VulcanToolCandidate {
            descriptor: VulcanToolDescriptor::new(
                RUNLUA_SESSION_TOOL_NAME,
                RUNLUA_SESSION_TOOL_NAME,
                RUNLUA_SESSION_TOOL_DESCRIPTION,
                None,
                VulcanToolOrigin::new(
                    VulcanToolSourceKind::LuaSkills,
                    "luaskills",
                    RUNLUA_SESSION_TOOL_NAME,
                ),
            ),
            parameters_schema: runlua_session_args_schema(),
            ai_visible: true,
            plan_policy: VulcanPlanToolPolicy::Hidden,
            request_streaming: None,
            invoke: Arc::new(move |arguments, context| {
                // invocation_handle keeps the registry shared across every action call.
                // invocation_handle 使归属注册表在所有动作调用之间保持共享。
                let invocation_handle = session_handle.clone();
                // invocation_workspace_root preserves the host-resolved workspace identity.
                // invocation_workspace_root 保留宿主解析的工作区身份。
                let invocation_workspace_root = session_workspace_root.clone();
                Box::pin(async move {
                    execute_runlua_session_tool(
                        invocation_handle,
                        invocation_workspace_root,
                        Some(context.session_id),
                        arguments,
                    )
                    .await
                })
            }),
        });
    }
    candidates
}

/// Applies both live AI exposure switches and closes AI-owned leases when sessions are disabled.
/// 应用两个实时 AI 曝光开关，并在关闭会话工具时释放 AI 所有的租约。
pub async fn set_runlua_ai_tools_enabled(
    handle: LuaSkillsRuntimeHandle,
    runlua_enabled: bool,
    runlua_session_enabled: bool,
) -> Result<(), String> {
    handle
        .runlua_ai_tool_enabled
        .store(runlua_enabled, Ordering::Release);
    handle
        .runlua_session_ai_tool_enabled
        .store(runlua_session_enabled, Ordering::Release);
    if runlua_session_enabled {
        return Ok(());
    }

    // retired_sessions removes host authorization before any upstream close can block.
    // retired_sessions 在任何上游关闭可能阻塞前先移除宿主授权。
    let retired_sessions = drain_runlua_session_registry(&handle);
    if retired_sessions.is_empty() {
        return Ok(());
    }
    let admission = Arc::clone(&handle.runlua_session_call_admission);
    run_luaskills_blocking_task_async("runlua-disable-close", admission, move || {
        // engine serializes lease shutdown with every LuaSkills runtime mutation.
        // engine 将租约关闭与所有 LuaSkills 运行时变更串行化。
        let engine = lock_runtime_engine(&handle)?;
        for session in retired_sessions {
            // close_request carries all exact identity fields returned by LuaSkills.
            // close_request 携带 LuaSkills 返回的全部精确身份字段。
            let close_request = json!({
                "lease_id": session.lease_id,
                "sid": session.sid,
                "generation": session.generation,
            });
            engine.close_runtime_lease_json(&close_request.to_string())?;
        }
        Ok(())
    })
    .await
}

/// Executes one isolated RunLua request and returns bounded model output plus UI metadata.
/// 执行一次隔离 RunLua 请求，并返回有界模型输出与界面元数据。
async fn execute_runlua_tool(
    handle: LuaSkillsRuntimeHandle,
    workspace_root: Option<PathBuf>,
    arguments: Value,
) -> Result<VulcanToolInvocationResult, String> {
    ensure_runlua_tool_enabled(&handle)?;
    // parsed contains only fields declared by the strict provider schema.
    // parsed 仅包含严格 Provider schema 声明的字段。
    let parsed: RunLuaToolArgs = serde_json::from_value(arguments)
        .map_err(|error| format!("Invalid runlua arguments: {error}"))?;
    validate_task_label(&parsed.task)?;
    validate_lua_code(&parsed.code, "runlua code")?;
    validate_structured_args(&parsed.args)?;
    validate_timeout_ms(parsed.timeout_ms)?;

    // task is normalized once for stable model output and card metadata.
    // task 仅归一化一次，以稳定模型输出与卡片元数据。
    let task = parsed.task.trim().to_string();
    // code_preview keeps transcript metadata bounded independently of the full invocation arguments.
    // code_preview 独立于完整调用参数保持转录元数据有界。
    let code_preview = compact_code_preview(&parsed.code);
    // timeout_ms is copied before parsed moves into the blocking closure.
    // timeout_ms 在 parsed 移入阻塞闭包前完成复制。
    let timeout_ms = parsed.timeout_ms;
    // source_file binds relative Lua filesystem and process operations to one explicit directory.
    // source_file 将 Lua 相对文件系统与进程操作绑定到一个明确目录。
    let mut source_file = create_runlua_workspace_source_file(workspace_root.as_deref())?;
    source_file
        .write_all(parsed.code.as_bytes())
        .and_then(|_| source_file.flush())
        .map_err(|error| format!("Failed to write the RunLua workspace source file: {error}"))?;
    // started_at measures complete engine-lock and execution latency for presentation.
    // started_at 测量完整引擎锁等待与执行延迟以供展示。
    let started_at = Instant::now();
    // request_json uses the private file surface so LuaSkills binds cwd to the host-selected parent.
    // request_json 使用内部文件接口，使 LuaSkills 将 cwd 绑定到宿主选定的父目录。
    let request_json = json!({
        "task": task,
        "file": runtime_assets::native_host_path(source_file.path()),
        "args": parsed.args,
        "timeout_ms": timeout_ms,
        "caller_tool_name": RUNLUA_TOOL_NAME,
    })
    .to_string();
    // markdown is the upstream canonical print and multi-return rendering.
    // markdown 是上游规范的打印与多返回值渲染结果。
    let admission = Arc::clone(&handle.runlua_call_admission);
    let markdown = run_luaskills_blocking_task_async("runlua-tool-call", admission, move || {
        let engine = lock_runtime_engine(&handle)?;
        engine.execute_runlua_request_json_inline(&request_json)
    })
    .await?;
    // source_file must remain present until LuaSkills has read and executed the private entry file.
    // source_file 必须保留到 LuaSkills 读取并执行完内部入口文件为止。
    drop(source_file);
    // elapsed_ms is bounded to u64 for JSON-compatible host metadata.
    // elapsed_ms 被限制为 u64 以形成 JSON 兼容宿主元数据。
    let elapsed_ms = u64::try_from(started_at.elapsed().as_millis()).unwrap_or(u64::MAX);
    // model_output applies the RunLua-specific inline budget and project spill policy.
    // model_output 应用 RunLua 专用内联预算与项目分页策略。
    let model_output = render_runlua_text(&markdown, workspace_root.as_deref())?;
    // host_payload excludes the full code and output because they already exist in trusted transcript blocks.
    // host_payload 排除完整代码与输出，因为它们已存在于可信转录块中。
    let host_payload = json!({
        "tool": RUNLUA_TOOL_NAME,
        "mode": "one_shot",
        "action": "eval",
        "status": "completed",
        "task": task,
        "codePreview": code_preview,
        "timeoutMs": timeout_ms,
        "elapsedMs": elapsed_ms,
        "outputBytes": markdown.len(),
    });
    Ok(
        VulcanToolInvocationResult::from_model_output(Value::String(model_output))
            .with_host_result(VulcanToolHostResult {
                kind: String::from(RUNLUA_HOST_RESULT_KIND),
                payload: host_payload,
            }),
    )
}

/// Executes one persistent RunLua lifecycle action for an exact conversation owner.
/// 为精确会话所有者执行一个持久 RunLua 生命周期动作。
async fn execute_runlua_session_tool(
    handle: LuaSkillsRuntimeHandle,
    workspace_root: Option<PathBuf>,
    owner_session_id: Option<String>,
    arguments: Value,
) -> Result<VulcanToolInvocationResult, String> {
    ensure_runlua_session_tool_enabled(&handle)?;
    // owner_session_id is mandatory because public leases must never be shared across conversations.
    // owner_session_id 为必填，因为公开租约绝不能跨会话共享。
    let owner_session_id = owner_session_id
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| String::from("runlua_session requires a conversation owner."))?;
    // parsed contains one flat schema whose action-specific requirements are validated below.
    // parsed 包含一份扁平 schema，其动作专用要求在下方校验。
    let parsed: RunLuaSessionToolArgs = serde_json::from_value(arguments)
        .map_err(|error| format!("Invalid runlua_session arguments: {error}"))?;
    validate_structured_args(&parsed.args)?;
    validate_timeout_ms(parsed.timeout_ms)?;
    validate_session_ttl(parsed.ttl_sec)?;
    // action is copied for stable post-execution metadata.
    // action 被复制以生成稳定的执行后元数据。
    let action = parsed.action;
    // session_id is normalized once for both registry-backed naming and compact presentation metadata.
    // session_id 只规范化一次，同时用于注册表名称查询与紧凑展示元数据。
    let presentation_session_id = parsed
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    // presentation_name comes from the start request or the exact owner-bound session registry.
    // presentation_name 来自启动请求，或精确绑定所有者的会话注册表。
    let presentation_name = match action {
        RunLuaSessionAction::Start => parsed
            .name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
        RunLuaSessionAction::Eval | RunLuaSessionAction::Status | RunLuaSessionAction::Close => {
            owned_session_display_name(
                &handle,
                &owner_session_id,
                presentation_session_id.as_deref(),
            )
        }
        RunLuaSessionAction::List => None,
    };
    // presentation_context retains only bounded request fields before parsed moves into the action.
    // presentation_context 在 parsed 移入动作前只保留有界请求字段。
    let presentation_context = RunLuaSessionPresentationContext {
        name: presentation_name,
        session_id: presentation_session_id,
        code_preview: parsed.code.as_deref().map(compact_code_preview),
    };
    // started_at measures complete ownership lookup, engine wait, and action execution latency.
    // started_at 测量完整归属查询、引擎等待与动作执行延迟。
    let started_at = Instant::now();
    // response is the exact normalized LuaSkills lifecycle payload.
    // response 是精确归一化后的 LuaSkills 生命周期载荷。
    let response_result = match action {
        RunLuaSessionAction::List => list_owned_runlua_sessions(&handle, &owner_session_id).await,
        RunLuaSessionAction::Start => {
            start_owned_runlua_session(
                &handle,
                workspace_root.as_deref(),
                &owner_session_id,
                parsed,
            )
            .await
        }
        RunLuaSessionAction::Eval => {
            eval_owned_runlua_session(&handle, &owner_session_id, parsed).await
        }
        RunLuaSessionAction::Status => {
            status_owned_runlua_session(&handle, &owner_session_id, parsed).await
        }
        RunLuaSessionAction::Close => {
            close_owned_runlua_session(&handle, &owner_session_id, parsed).await
        }
    };
    // response converts host-side failures into the same explicit JSON failure contract as LuaSkills.
    // response 将宿主侧失败转换为与 LuaSkills 相同的显式 JSON 失败契约。
    let response = response_result.unwrap_or_else(|message| {
        json!({
            "ok": false,
            "error_code": "host_error",
            "message": message,
        })
    });
    // elapsed_ms is bounded to u64 for JSON-compatible host metadata.
    // elapsed_ms 被限制为 u64 以形成 JSON 兼容宿主元数据。
    let elapsed_ms = u64::try_from(started_at.elapsed().as_millis()).unwrap_or(u64::MAX);
    // readable_response strips lease internals only at the conversation output boundary.
    // readable_response 仅在会话输出边界移除租约内部信息。
    let readable_response = result_markdown::render(action, &response)?;
    // model_output applies the same bounded result policy as one-shot RunLua.
    // model_output 应用与一次性 RunLua 相同的有界结果策略。
    let model_output = render_runlua_text(&readable_response, workspace_root.as_deref())?;
    // host_payload is intentionally compact and never duplicates a potentially large eval result.
    // host_payload 刻意保持紧凑，绝不复制可能很大的 eval 结果。
    let host_payload =
        runlua_session_host_payload(action, &presentation_context, &response, elapsed_ms);
    Ok(
        VulcanToolInvocationResult::from_model_output(Value::String(model_output))
            .with_host_result(VulcanToolHostResult {
                kind: String::from(RUNLUA_SESSION_HOST_RESULT_KIND),
                payload: host_payload,
            }),
    )
}

mod result_markdown;

/// Starts one owner-bound public LuaSkills runtime lease.
/// 启动一个绑定所有者的 LuaSkills 公开运行时租约。
async fn start_owned_runlua_session(
    handle: &LuaSkillsRuntimeHandle,
    workspace_root: Option<&Path>,
    owner_session_id: &str,
    args: RunLuaSessionToolArgs,
) -> Result<Value, String> {
    // name is required and normalized before deriving the upstream SID.
    // name 为必填，并在派生上游 SID 前完成归一化。
    let name = normalize_session_name(args.name.as_deref())?;
    // Refreshing exact upstream status removes expired, closed, and restart-invalidated records.
    // 刷新精确上游状态会移除过期、已关闭及重启后失效的记录。
    prune_inactive_owned_sessions(handle, owner_session_id).await?;
    // reservation closes the check/create/commit race without holding a mutex across await.
    // reservation 在不跨 await 持有互斥锁的情况下消除检查/创建/提交竞态。
    let reservation = reserve_owner_session_start(handle, owner_session_id, &name)?;
    // workspace_root is host-controlled and is the only cwd/module context accepted publicly.
    // workspace_root 由宿主控制，是公开接口接受的唯一 cwd/模块上下文。
    let workspace_root = workspace_root
        .ok_or_else(|| String::from("runlua_session start requires a workspace root."))?;
    // lua_workspace_root removes relative aliases and Windows verbatim prefixes before passing path identity to LuaSkills.
    // lua_workspace_root 在将路径身份交给 LuaSkills 前消除相对别名与 Windows verbatim 前缀。
    let lua_workspace_root =
        runtime_assets::native_host_path(&fs::canonicalize(workspace_root).map_err(|error| {
            format!(
                "Failed to canonicalize runlua_session workspace `{}`: {error}",
                workspace_root.display()
            )
        })?);
    // sid namespaces the human name under the exact conversation owner.
    // sid 将人类可读名称限定在精确会话所有者命名空间内。
    let sid = build_owned_runtime_sid(owner_session_id, &name)?;
    // ttl_sec was already validated against the exact upstream public limit.
    // ttl_sec 已依据精确上游公开限制完成校验。
    let ttl_sec = args.ttl_sec;
    // request uses no caller-provided module roots, native roots, or mounts.
    // request 不使用任何调用方提供的 Lua 根、原生根或挂载。
    let request = json!({
        "sid": sid,
        "ttl_sec": ttl_sec,
        "replace": false,
        "cwd": lua_workspace_root,
        "workspace_root": lua_workspace_root,
        "lua_roots": [],
        "c_roots": [],
        "mounts": {},
    })
    .to_string();
    // invocation_handle owns one clone suitable for the blocking engine call.
    // invocation_handle 持有适用于阻塞引擎调用的一份克隆。
    let invocation_handle = handle.clone();
    // response is the exact upstream create payload.
    // response 是精确的上游创建载荷。
    let admission = Arc::clone(&invocation_handle.runlua_session_call_admission);
    let response_text =
        run_luaskills_blocking_task_async("runlua-session-start", admission, move || {
            let engine = lock_runtime_engine(&invocation_handle)?;
            engine.create_runtime_lease_json(&request)
        })
        .await?;
    // response is validated before publishing host ownership.
    // response 在发布宿主归属前完成校验。
    let response = parse_runtime_lease_json("start", &response_text)?;
    if !runtime_response_ok(&response)? {
        return Ok(response);
    }
    // lease_id is the only public session identifier accepted by later actions.
    // lease_id 是后续动作接受的唯一公开 Session 标识。
    let lease_id = required_response_string(&response, "lease_id")?;
    // generation protects against stale handles after replacement or reload.
    // generation 防止替换或 reload 后使用陈旧句柄。
    let generation = required_response_u64(&response, "generation")?;
    // owned_session captures the exact upstream identity and finite TTL.
    // owned_session 捕获精确的上游身份与有限 TTL。
    let owned_session = RunLuaOwnedSession {
        owner_session_id: owner_session_id.to_string(),
        name,
        sid,
        lease_id: lease_id.clone(),
        generation,
        ttl_sec,
    };
    // unpublished_session preserves exact upstream identity for compensation if commit is rejected.
    // unpublished_session 在提交被拒绝时保留精确上游身份用于补偿关闭。
    let unpublished_session = owned_session.clone();
    if let Err(commit_error) = commit_owned_session_start(handle, &reservation, owned_session) {
        // The tool may have been disabled or reloaded after upstream creation but before commit.
        // 工具可能在上游创建后、提交前被停用或重载。
        let cleanup_error = close_unpublished_runlua_session(handle, &unpublished_session)
            .await
            .err();
        return Err(match cleanup_error {
            Some(cleanup_error) => format!("{commit_error} Cleanup also failed: {cleanup_error}"),
            None => commit_error,
        });
    }
    Ok(response)
}

/// Evaluates Lua inside one exact owner-bound persistent lease.
/// 在一个精确绑定所有者的持久租约中执行 Lua。
async fn eval_owned_runlua_session(
    handle: &LuaSkillsRuntimeHandle,
    owner_session_id: &str,
    args: RunLuaSessionToolArgs,
) -> Result<Value, String> {
    // session identifies the exact lease after host ownership validation.
    // session 在宿主归属校验后标识精确租约。
    let session = require_owned_session(handle, owner_session_id, args.session_id.as_deref())?;
    // code is mandatory for eval and remains inline by design.
    // code 是 eval 必填项，并按设计保持内联。
    let code = args
        .code
        .as_deref()
        .ok_or_else(|| String::from("runlua_session eval requires `code`."))?;
    validate_lua_code(code, "runlua_session eval code")?;
    // request echoes every exact identity field to let LuaSkills reject stale handles.
    // request 回传全部精确身份字段，使 LuaSkills 能拒绝陈旧句柄。
    let request = json!({
        "lease_id": session.lease_id,
        "sid": session.sid,
        "generation": session.generation,
        "code": code,
        "args": args.args,
        "timeout_ms": args.timeout_ms,
    })
    .to_string();
    let invocation_handle = handle.clone();
    let admission = Arc::clone(&invocation_handle.runlua_session_call_admission);
    let response_text =
        run_luaskills_blocking_task_async("runlua-session-eval", admission, move || {
            let engine = lock_runtime_engine(&invocation_handle)?;
            engine.eval_runtime_lease_json(&request)
        })
        .await?;
    let response = parse_runtime_lease_json("eval", &response_text)?;
    retire_unusable_owned_session(handle, &session.lease_id, &response)?;
    Ok(response)
}

/// Reads the exact status of one owner-bound persistent lease.
/// 读取一个绑定所有者的持久租约精确状态。
async fn status_owned_runlua_session(
    handle: &LuaSkillsRuntimeHandle,
    owner_session_id: &str,
    args: RunLuaSessionToolArgs,
) -> Result<Value, String> {
    let session = require_owned_session(handle, owner_session_id, args.session_id.as_deref())?;
    let response = fetch_owned_session_status(handle, &session).await?;
    retire_unusable_owned_session(handle, &session.lease_id, &response)?;
    Ok(response)
}

/// Lists exact upstream status snapshots for leases owned by one conversation.
/// 列出一个会话拥有租约的精确上游状态快照。
async fn list_owned_runlua_sessions(
    handle: &LuaSkillsRuntimeHandle,
    owner_session_id: &str,
) -> Result<Value, String> {
    // sessions snapshots ownership without holding the registry lock across engine calls.
    // sessions 在不跨引擎调用持有注册表锁的情况下快照归属。
    let sessions = owned_session_snapshot(handle, owner_session_id);
    // active_sessions contains only leases still accepted by LuaSkills.
    // active_sessions 仅包含仍被 LuaSkills 接受的租约。
    let mut active_sessions = Vec::new();
    for session in sessions {
        let response = fetch_owned_session_status(handle, &session).await?;
        if runtime_response_ok(&response)? || !runtime_response_retires_lease(&response)? {
            // decorated keeps transient failures visible and adds host-owned display identity.
            // decorated 保留瞬态失败可见性，并增加宿主显示身份。
            let mut decorated = response;
            let object = decorated.as_object_mut().ok_or_else(|| {
                String::from("LuaSkills runtime lease list status must be an object.")
            })?;
            object.insert(String::from("name"), Value::String(session.name));
            object.insert(String::from("session_id"), Value::String(session.lease_id));
            active_sessions.push(decorated);
        } else {
            remove_owned_session(handle, &session.lease_id);
        }
    }
    Ok(json!({
        "ok": true,
        "sessions": active_sessions,
    }))
}

/// Closes one owner-bound lease and removes its host authorization.
/// 关闭一个绑定所有者的租约并移除其宿主授权。
async fn close_owned_runlua_session(
    handle: &LuaSkillsRuntimeHandle,
    owner_session_id: &str,
    args: RunLuaSessionToolArgs,
) -> Result<Value, String> {
    let session = require_owned_session(handle, owner_session_id, args.session_id.as_deref())?;
    let request = runtime_lease_identity_request(&session).to_string();
    let invocation_handle = handle.clone();
    let admission = Arc::clone(&invocation_handle.runlua_session_call_admission);
    let response_text =
        run_luaskills_blocking_task_async("runlua-session-close", admission, move || {
            let engine = lock_runtime_engine(&invocation_handle)?;
            engine.close_runtime_lease_json(&request)
        })
        .await?;
    let response = parse_runtime_lease_json("close", &response_text)?;
    if runtime_response_releases_host_ownership(&response)? {
        remove_owned_session(handle, &session.lease_id);
    }
    Ok(response)
}

/// Fetches one upstream status payload without holding the ownership lock.
/// 在不持有归属锁的情况下获取一份上游状态载荷。
async fn fetch_owned_session_status(
    handle: &LuaSkillsRuntimeHandle,
    session: &RunLuaOwnedSession,
) -> Result<Value, String> {
    let request = runtime_lease_identity_request(session).to_string();
    let invocation_handle = handle.clone();
    let admission = Arc::clone(&invocation_handle.runlua_session_call_admission);
    let response_text =
        run_luaskills_blocking_task_async("runlua-session-status", admission, move || {
            let engine = lock_runtime_engine(&invocation_handle)?;
            engine.runtime_lease_status_json(&request)
        })
        .await?;
    parse_runtime_lease_json("status", &response_text)
}

/// Builds the exact identity request shared by eval-independent lease actions.
/// 构建不依赖 eval 的租约动作共享的精确身份请求。
fn runtime_lease_identity_request(session: &RunLuaOwnedSession) -> Value {
    json!({
        "lease_id": session.lease_id,
        "sid": session.sid,
        "generation": session.generation,
    })
}

/// Returns one owned lease only when both the opaque ID and conversation owner match.
/// 仅当不透明 ID 与会话所有者同时匹配时返回租约。
fn require_owned_session(
    handle: &LuaSkillsRuntimeHandle,
    owner_session_id: &str,
    session_id: Option<&str>,
) -> Result<RunLuaOwnedSession, String> {
    let session_id = session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| String::from("runlua_session action requires `session_id`."))?;
    let registry = handle
        .runlua_sessions
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = registry
        .leases
        .get(session_id)
        .ok_or_else(|| format!("Unknown runlua_session `{session_id}`."))?;
    if session.owner_session_id != owner_session_id {
        return Err(String::from(
            "The requested runlua_session belongs to another conversation.",
        ));
    }
    Ok(session.clone())
}

/// Returns the human-readable name for one exact owner-bound public session.
/// 返回一个精确绑定所有者的公开会话的人类可读名称。
fn owned_session_display_name(
    handle: &LuaSkillsRuntimeHandle,
    owner_session_id: &str,
    session_id: Option<&str>,
) -> Option<String> {
    let session_id = session_id?;
    let registry = handle
        .runlua_sessions
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    registry
        .leases
        .get(session_id)
        .filter(|session| session.owner_session_id == owner_session_id)
        .map(|session| session.name.clone())
}

/// Rejects duplicate names and excessive leases before creating an upstream VM.
/// 在创建上游 VM 前拒绝重复名称与过量租约。
fn reserve_owner_session_start(
    handle: &LuaSkillsRuntimeHandle,
    owner_session_id: &str,
    name: &str,
) -> Result<RunLuaSessionStartReservation, String> {
    let mut registry = handle
        .runlua_sessions
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    reserve_owner_session_start_in_registry(&mut registry, owner_session_id, name)?;
    Ok(RunLuaSessionStartReservation {
        handle: handle.clone(),
        owner_session_id: owner_session_id.to_string(),
        name: name.to_string(),
    })
}

/// Atomically validates and records one pending owner/name start reservation.
/// 原子校验并记录一条待处理的所有者/名称启动预留。
fn reserve_owner_session_start_in_registry(
    registry: &mut RunLuaSessionRegistry,
    owner_session_id: &str,
    name: &str,
) -> Result<(), String> {
    let active_count = registry
        .leases
        .values()
        .filter(|session| session.owner_session_id == owner_session_id)
        .count();
    let pending_count = registry
        .pending_starts
        .iter()
        .filter(|(owner, _)| owner == owner_session_id)
        .count();
    if active_count + pending_count >= RUNLUA_MAX_SESSIONS_PER_OWNER {
        return Err(format!(
            "runlua_session allows at most {RUNLUA_MAX_SESSIONS_PER_OWNER} active sessions per conversation."
        ));
    }
    if registry
        .leases
        .values()
        .any(|session| session.owner_session_id == owner_session_id && session.name == name)
        || registry
            .pending_starts
            .contains(&(owner_session_id.to_string(), name.to_string()))
    {
        return Err(format!(
            "runlua_session name `{name}` already exists in this conversation."
        ));
    }
    registry
        .pending_starts
        .insert((owner_session_id.to_string(), name.to_string()));
    Ok(())
}

/// Commits one upstream lease only while its reservation and live tool exposure remain valid.
/// 仅在预留与实时工具曝光仍有效时提交一条上游租约。
fn commit_owned_session_start(
    handle: &LuaSkillsRuntimeHandle,
    reservation: &RunLuaSessionStartReservation,
    session: RunLuaOwnedSession,
) -> Result<(), String> {
    let mut registry = handle
        .runlua_sessions
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let reservation_key = (
        reservation.owner_session_id.clone(),
        reservation.name.clone(),
    );
    if !registry.pending_starts.remove(&reservation_key) {
        return Err(String::from(
            "runlua_session start reservation was invalidated before commit.",
        ));
    }
    if !runlua_session_ai_tool_enabled(handle) {
        return Err(String::from(
            "runlua_session was disabled before the new session could be committed.",
        ));
    }
    registry.leases.insert(session.lease_id.clone(), session);
    Ok(())
}

/// Queries exact upstream status and removes every terminal owner-bound registry entry.
/// 查询精确上游状态，并移除所有已终止的所有者绑定注册表记录。
async fn prune_inactive_owned_sessions(
    handle: &LuaSkillsRuntimeHandle,
    owner_session_id: &str,
) -> Result<(), String> {
    let sessions = owned_session_snapshot(handle, owner_session_id);
    for session in sessions {
        let response = fetch_owned_session_status(handle, &session).await?;
        if runtime_response_retires_lease(&response)? {
            remove_owned_session(handle, &session.lease_id);
        }
    }
    Ok(())
}

/// Returns a detached snapshot of all registry entries owned by one conversation.
/// 返回一个会话拥有的全部注册表记录的脱离锁快照。
fn owned_session_snapshot(
    handle: &LuaSkillsRuntimeHandle,
    owner_session_id: &str,
) -> Vec<RunLuaOwnedSession> {
    let registry = handle
        .runlua_sessions
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    registry
        .leases
        .values()
        .filter(|session| session.owner_session_id == owner_session_id)
        .cloned()
        .collect()
}

/// Closes an upstream lease that could not be published to the host ownership registry.
/// 关闭一条无法发布到宿主归属注册表的上游租约。
async fn close_unpublished_runlua_session(
    handle: &LuaSkillsRuntimeHandle,
    session: &RunLuaOwnedSession,
) -> Result<(), String> {
    let request = runtime_lease_identity_request(session).to_string();
    let invocation_handle = handle.clone();
    let admission = Arc::clone(&invocation_handle.runlua_session_call_admission);
    let response_text =
        run_luaskills_blocking_task_async("runlua-unpublished-close", admission, move || {
            let engine = lock_runtime_engine(&invocation_handle)?;
            engine.close_runtime_lease_json(&request)
        })
        .await?;
    let response = parse_runtime_lease_json("unpublished close", &response_text)?;
    if runtime_response_releases_host_ownership(&response)? {
        Ok(())
    } else {
        Err(lua_runtime_response_error_text(&response)?)
    }
}

/// Removes one unusable lease only for exact terminal or identity-invalid upstream errors.
/// 仅在上游返回精确终态或身份失效错误时移除不可用租约。
fn retire_unusable_owned_session(
    handle: &LuaSkillsRuntimeHandle,
    lease_id: &str,
    response: &Value,
) -> Result<(), String> {
    if runtime_response_retires_lease(response)? {
        remove_owned_session(handle, lease_id);
    }
    Ok(())
}

/// Returns whether one exact LuaSkills response proves the public lease cannot be reused.
/// 返回一份精确 LuaSkills 响应是否证明公开租约已无法复用。
fn runtime_response_retires_lease(response: &Value) -> Result<bool, String> {
    if runtime_response_ok(response)? {
        return Ok(false);
    }
    // eval_failed and lease_busy preserve the upstream lease and must remain retryable.
    // eval_failed 与 lease_busy 会保留上游租约，因此必须保持可重试。
    let error_code = response
        .get("error_code")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            String::from("LuaSkills failed runtime lease response is missing `error_code`.")
        })?;
    Ok(matches!(
        error_code,
        "lease_not_found"
            | "lease_closed"
            | "lease_expired"
            | "lease_replaced"
            | "lease_sid_mismatch"
            | "lease_generation_mismatch"
            | "lease_profile_mismatch"
    ))
}

/// Returns whether one response proves the upstream lease no longer needs host ownership.
/// 返回一份响应是否证明上游租约不再需要宿主归属。
fn runtime_response_releases_host_ownership(response: &Value) -> Result<bool, String> {
    Ok(runtime_response_ok(response)? || runtime_response_retires_lease(response)?)
}

/// Removes one lease from the host ownership registry.
/// 从宿主归属注册表移除一条租约。
fn remove_owned_session(handle: &LuaSkillsRuntimeHandle, lease_id: &str) {
    let mut registry = handle
        .runlua_sessions
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    registry.leases.remove(lease_id);
}

/// Drains all host-owned leases for disable-time best-effort upstream closure.
/// 排空全部宿主自有租约，以便关闭开关时尽力关闭上游租约。
fn drain_runlua_session_registry(handle: &LuaSkillsRuntimeHandle) -> Vec<RunLuaOwnedSession> {
    let mut registry = handle
        .runlua_sessions
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    registry.pending_starts.clear();
    std::mem::take(&mut registry.leases).into_values().collect()
}

/// Parses one exact LuaSkills lease JSON payload and requires an object-shaped response.
/// 解析一份精确 LuaSkills 租约 JSON 载荷，并要求响应为对象形。
fn parse_runtime_lease_json(action: &str, response_text: &str) -> Result<Value, String> {
    let response: Value = serde_json::from_str(response_text).map_err(|error| {
        format!("LuaSkills runlua_session {action} returned invalid JSON: {error}")
    })?;
    if !response.is_object() {
        return Err(format!(
            "LuaSkills runlua_session {action} response must be a JSON object."
        ));
    }
    runtime_response_ok(&response)?;
    Ok(response)
}

/// Reads the mandatory boolean `ok` field from one LuaSkills lease response.
/// 从一份 LuaSkills 租约响应读取必需的布尔 `ok` 字段。
fn runtime_response_ok(response: &Value) -> Result<bool, String> {
    response
        .get("ok")
        .and_then(Value::as_bool)
        .ok_or_else(|| String::from("LuaSkills runtime lease response is missing boolean `ok`."))
}

/// Reads one mandatory non-empty string field from a successful lease response.
/// 从成功租约响应读取一个必需的非空字符串字段。
fn required_response_string(response: &Value, field: &str) -> Result<String, String> {
    response
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("LuaSkills runtime lease response is missing `{field}`."))
}

/// Reads one mandatory unsigned integer field from a successful lease response.
/// 从成功租约响应读取一个必需的无符号整数字段。
fn required_response_u64(response: &Value, field: &str) -> Result<u64, String> {
    response
        .get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("LuaSkills runtime lease response is missing `{field}`."))
}

/// Formats the exact required error code and message from one failed LuaSkills response.
/// 从一份失败的 LuaSkills 响应格式化精确必需的错误码与消息。
fn lua_runtime_response_error_text(response: &Value) -> Result<String, String> {
    let error_code = required_response_string(response, "error_code")?;
    let message = required_response_string(response, "message")?;
    Ok(format!("{error_code}: {message}"))
}

/// Builds compact host-only metadata for the persistent Lua session card.
/// 为持久 Lua 会话卡片构建紧凑的宿主专用元数据。
fn runlua_session_host_payload(
    action: RunLuaSessionAction,
    presentation_context: &RunLuaSessionPresentationContext,
    response: &Value,
    elapsed_ms: u64,
) -> Value {
    json!({
        "tool": RUNLUA_SESSION_TOOL_NAME,
        "mode": "persistent",
        "action": action.as_str(),
        "status": if response.get("ok").and_then(Value::as_bool) == Some(true) {
            "completed"
        } else {
            "failed"
        },
        "name": presentation_context.name,
        "codePreview": presentation_context.code_preview,
        "sessionId": response
            .get("lease_id")
            .and_then(Value::as_str)
            .or(presentation_context.session_id.as_deref()),
        "generation": response.get("generation").and_then(Value::as_u64),
        "ttlSec": response.get("ttl_sec").and_then(Value::as_u64),
        "expiresAtUnixMs": response.get("expires_at_unix_ms").cloned(),
        "errorCode": response.get("error_code").and_then(Value::as_str),
        "message": response.get("message").and_then(Value::as_str),
        "sessionCount": response.get("sessions").and_then(Value::as_array).map(Vec::len),
        "elapsedMs": elapsed_ms,
    })
}

/// Applies the RunLua-specific inline budget and existing project page spill mechanism.
/// 应用 RunLua 专用内联预算与现有项目分页落盘机制。
///
/// # Parameters
/// # 参数
/// - `content`: Canonical RunLua result text to render.
/// - `content`：需要渲染的规范 RunLua 结果文本。
/// - `workspace_root`: Optional project root used to derive the lazy spill path.
/// - `workspace_root`：用于派生延迟超限路径的可选项目根目录。
///
/// # Returns
/// # 返回值
/// Returns inline text or a page pointer, or the exact rendering failure.
/// 返回内联文本或分页指针，或精确的渲染失败。
fn render_runlua_text(content: &str, workspace_root: Option<&Path>) -> Result<String, String> {
    let render_input = HostTextRenderInput {
        content: content.to_string(),
        overflow_mode: Some(HostToolOverflowMode::Page),
        template_hint: None,
        content_bytes: content.len(),
        content_lines: content.lines().count(),
    };
    let budget = ToolResultBudgetSnapshot::uniform_bytes(RUNLUA_INLINE_RESULT_BYTES);
    let spill_root = workspace_root
        .filter(|path| !path.as_os_str().is_empty())
        .map(project_temp_maintenance::resolve_project_temp_dir);
    let render_options = HostRenderOptions {
        spill_root,
        template_skill_roots: Vec::new(),
        template_resources_root: None,
    };
    render_tool_result_text(&render_input, None, &budget, &render_options)
}

/// Builds the strict JSON schema for one-shot RunLua execution.
/// 构建一次性 RunLua 执行的严格 JSON schema。
fn runlua_args_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["task", "code"],
        "properties": {
            "task": {
                "type": "string",
                "minLength": 1,
                "maxLength": 120,
                "description": "Short task label shown in the conversation card."
            },
            "code": {
                "type": "string",
                "minLength": 1,
                "maxLength": RUNLUA_MAX_CODE_BYTES,
                "description": "Inline Lua source. File execution is intentionally not exposed."
            },
            "args": {
                "type": "object",
                "default": {},
                "description": "Structured values exposed to Lua as local `args`."
            },
            "timeout_ms": {
                "type": "integer",
                "minimum": 1,
                "maximum": RUNLUA_MAX_TIMEOUT_MS,
                "default": RUNLUA_DEFAULT_TIMEOUT_MS,
                "description": "Lua instruction timeout in milliseconds. Blocking host calls require their own timeout."
            }
        }
    })
}

/// Builds the flat action schema for persistent RunLua sessions.
/// 构建持久 RunLua 会话的扁平动作 schema。
fn runlua_session_args_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["action"],
        "properties": {
            "action": {
                "type": "string",
                "enum": ["list", "start", "eval", "status", "close"],
                "description": "Persistent Lua session lifecycle action."
            },
            "name": {
                "type": "string",
                "minLength": 1,
                "maxLength": 64,
                "description": "Task-oriented session name required by `start`."
            },
            "session_id": {
                "type": "string",
                "minLength": 1,
                "description": "Opaque owned session id required by `eval`, `status`, and `close`."
            },
            "code": {
                "type": "string",
                "minLength": 1,
                "maxLength": RUNLUA_MAX_CODE_BYTES,
                "description": "Inline Lua source required by `eval`. Return JSON-compatible values instead of relying on print output."
            },
            "args": {
                "type": "object",
                "default": {},
                "description": "Structured values exposed to Lua as local `args` during `eval`."
            },
            "ttl_sec": {
                "type": "integer",
                "minimum": 1,
                "maximum": RUNLUA_MAX_SESSION_TTL_SEC,
                "default": RUNLUA_DEFAULT_SESSION_TTL_SEC,
                "description": "Finite idle lease lifetime used by `start`."
            },
            "timeout_ms": {
                "type": "integer",
                "minimum": 1,
                "maximum": RUNLUA_MAX_TIMEOUT_MS,
                "default": RUNLUA_DEFAULT_TIMEOUT_MS,
                "description": "Lua instruction timeout used by `eval`."
            }
        }
    })
}

/// Rejects invocation after the live host exposure switch has been disabled.
/// 在实时宿主曝光开关关闭后拒绝调用。
fn ensure_runlua_tool_enabled(handle: &LuaSkillsRuntimeHandle) -> Result<(), String> {
    if runlua_ai_tool_enabled(handle) {
        Ok(())
    } else {
        Err(String::from("runlua is disabled in LuaSkills settings."))
    }
}

/// Rejects persistent invocation after its live host exposure switch has been disabled.
/// 在持久会话实时宿主曝光开关关闭后拒绝调用。
fn ensure_runlua_session_tool_enabled(handle: &LuaSkillsRuntimeHandle) -> Result<(), String> {
    if runlua_session_ai_tool_enabled(handle) {
        Ok(())
    } else {
        Err(String::from(
            "runlua_session is disabled in LuaSkills settings.",
        ))
    }
}

/// Validates and normalizes the short one-shot task label.
/// 校验并规范化一次性执行的简短任务标签。
fn validate_task_label(task: &str) -> Result<(), String> {
    let task = task.trim();
    if task.is_empty() {
        return Err(String::from("runlua `task` must not be empty."));
    }
    if task.len() > 120 {
        return Err(String::from("runlua `task` must not exceed 120 bytes."));
    }
    Ok(())
}

/// Validates inline Lua source against empty and oversized payloads.
/// 校验内联 Lua 源码，拒绝空内容与超大载荷。
fn validate_lua_code(code: &str, field_name: &str) -> Result<(), String> {
    if code.trim().is_empty() {
        return Err(format!("{field_name} must not be empty."));
    }
    if code.len() > RUNLUA_MAX_CODE_BYTES {
        return Err(format!(
            "{field_name} must not exceed {RUNLUA_MAX_CODE_BYTES} bytes."
        ));
    }
    Ok(())
}

/// Requires object-shaped arguments because LuaSkills exposes them as one local table.
/// 要求参数为对象形，因为 LuaSkills 将其作为一个局部表暴露。
fn validate_structured_args(args: &Value) -> Result<(), String> {
    if args.is_object() {
        Ok(())
    } else {
        Err(String::from("RunLua `args` must be a JSON object."))
    }
}

/// Validates the host-enforced Lua instruction timeout range.
/// 校验宿主强制的 Lua 指令超时范围。
fn validate_timeout_ms(timeout_ms: u64) -> Result<(), String> {
    if (1..=RUNLUA_MAX_TIMEOUT_MS).contains(&timeout_ms) {
        Ok(())
    } else {
        Err(format!(
            "RunLua `timeout_ms` must be between 1 and {RUNLUA_MAX_TIMEOUT_MS}."
        ))
    }
}

/// Validates the exact finite TTL range supported by public LuaSkills leases.
/// 校验 LuaSkills 公开租约支持的精确有限 TTL 范围。
fn validate_session_ttl(ttl_sec: u64) -> Result<(), String> {
    if (1..=RUNLUA_MAX_SESSION_TTL_SEC).contains(&ttl_sec) {
        Ok(())
    } else {
        Err(format!(
            "runlua_session `ttl_sec` must be between 1 and {RUNLUA_MAX_SESSION_TTL_SEC}."
        ))
    }
}

/// Normalizes one human-oriented session name without inventing aliases.
/// 规范化一个面向使用者的会话名称，不创建任何猜测性别名。
fn normalize_session_name(name: Option<&str>) -> Result<String, String> {
    let name = name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| String::from("runlua_session start requires `name`."))?;
    if name.len() > 64 {
        return Err(String::from(
            "runlua_session `name` must not exceed 64 bytes.",
        ));
    }
    if name.contains('\0') {
        return Err(String::from(
            "runlua_session `name` must not contain NUL bytes.",
        ));
    }
    Ok(name.to_string())
}

/// Builds one exact owner-namespaced SID accepted by LuaSkills public leases.
/// 构建一个 LuaSkills 公开租约接受的精确所有者命名空间 SID。
fn build_owned_runtime_sid(owner_session_id: &str, name: &str) -> Result<String, String> {
    let sid = format!("vulcan-runlua:{owner_session_id}:{name}");
    if sid.len() > 128 {
        return Err(String::from(
            "runlua_session owner and name exceed the LuaSkills 128-byte SID limit.",
        ));
    }
    Ok(sid)
}

/// Produces a bounded first-lines preview for transcript metadata.
/// 为转录元数据生成有界的前若干行预览。
fn compact_code_preview(code: &str) -> String {
    const MAX_PREVIEW_LINES: usize = 8;
    const MAX_PREVIEW_CHARS: usize = 1_024;
    let mut preview = code
        .lines()
        .take(MAX_PREVIEW_LINES)
        .collect::<Vec<_>>()
        .join("\n");
    if preview.chars().count() > MAX_PREVIEW_CHARS {
        preview = preview.chars().take(MAX_PREVIEW_CHARS).collect::<String>();
        preview.push('…');
    } else if code.lines().count() > MAX_PREVIEW_LINES {
        preview.push_str("\n…");
    }
    preview
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the one-shot schema stays strict and advertises the required system Skill flow.
    /// 验证一次性 schema 保持严格，并声明所需系统 Skill 流程。
    #[test]
    fn runlua_schema_is_strict_and_bounded() {
        let schema = runlua_args_schema();
        assert_eq!(schema["additionalProperties"], Value::Bool(false));
        assert_eq!(
            schema["properties"]["timeout_ms"]["maximum"],
            json!(RUNLUA_MAX_TIMEOUT_MS)
        );
        assert_eq!(
            schema["properties"]["code"]["maxLength"],
            json!(RUNLUA_MAX_CODE_BYTES)
        );
    }

    /// Verifies public session names produce deterministic owner-bound LuaSkills SIDs.
    /// 验证公开 Session 名称生成确定性的所有者绑定 LuaSkills SID。
    #[test]
    fn runlua_session_sid_is_owner_bound() {
        let sid = build_owned_runtime_sid("owner-a", "analysis")
            .expect("owner-bound SID should be valid");
        assert_eq!(sid, "vulcan-runlua:owner-a:analysis");
    }

    /// Verifies oversized code previews remain bounded without losing truncation evidence.
    /// 验证超大代码预览保持有界且不会丢失截断证据。
    #[test]
    fn code_preview_is_bounded() {
        let code = (0..20)
            .map(|index| format!("local value_{index} = {index}"))
            .collect::<Vec<_>>()
            .join("\n");
        let preview = compact_code_preview(&code);
        assert!(preview.ends_with('…'));
        assert!(preview.lines().count() <= 9);
    }

    /// Verifies both resident descriptions route detailed guidance through the native prompt Skill.
    /// 验证两个常驻工具说明都通过原生提示词 Skill 路由详细指引。
    #[test]
    fn runlua_descriptions_require_native_skill_lookup() {
        for description in [RUNLUA_TOOL_DESCRIPTION, RUNLUA_SESSION_TOOL_DESCRIPTION] {
            assert!(description.contains("native `skill` tool"));
            assert!(description.contains("{\"name\":\"runlua\"}"));
            assert!(description.contains("do not use `luaskill`"));
            assert!(description.len() < 260);
        }
    }

    /// Verifies persistent presentation metadata retains the requested session identity and code preview.
    /// 验证持久会话展示元数据会保留请求的会话身份与代码预览。
    #[test]
    fn runlua_session_host_payload_contains_bounded_request_context() {
        let context = RunLuaSessionPresentationContext {
            name: Some(String::from("counter")),
            session_id: Some(String::from("lease-1")),
            code_preview: Some(String::from("return counter")),
        };
        let response = json!({"ok": true, "generation": 2, "ttl_sec": 600});
        let payload =
            runlua_session_host_payload(RunLuaSessionAction::Eval, &context, &response, 12);
        assert_eq!(payload["sessionId"], json!("lease-1"));
        assert_eq!(payload["name"], json!("counter"));
        assert_eq!(payload["codePreview"], json!("return counter"));
        assert_eq!(payload["status"], json!("completed"));
        assert_eq!(payload["elapsedMs"], json!(12));

        let failed_payload = runlua_session_host_payload(
            RunLuaSessionAction::Eval,
            &context,
            &json!({
                "ok": false,
                "error_code": "host_error",
                "message": "engine unavailable"
            }),
            15,
        );
        assert_eq!(failed_payload["status"], json!("failed"));
        assert_eq!(failed_payload["name"], json!("counter"));
        assert_eq!(failed_payload["sessionId"], json!("lease-1"));
        assert_eq!(failed_payload["errorCode"], json!("host_error"));
    }

    /// Verifies Lua evaluation failures remain retryable while terminal lease errors retire ownership.
    /// 验证 Lua 执行失败保持可重试，而租约终态错误会移除归属。
    #[test]
    fn only_terminal_runtime_errors_retire_owned_sessions() {
        assert!(
            !runtime_response_retires_lease(
                &json!({"ok": false, "error_code": "eval_failed", "message": "boom"})
            )
            .expect("eval failure response should be valid")
        );
        assert!(
            !runtime_response_retires_lease(
                &json!({"ok": false, "error_code": "lease_busy", "message": "busy"})
            )
            .expect("busy response should be valid")
        );
        assert!(
            runtime_response_retires_lease(
                &json!({"ok": false, "error_code": "lease_expired", "message": "expired"})
            )
            .expect("terminal response should be valid")
        );
    }

    /// Verifies the hidden source file is rooted in the selected workspace and removed on drop.
    /// 验证隐藏源码文件位于选定工作区，并在释放后删除。
    #[test]
    fn runlua_source_file_is_bound_to_workspace() {
        let workspace = tempfile::tempdir().expect("create workspace fixture");
        let mut source = create_runlua_workspace_source_file(Some(workspace.path()))
            .expect("create workspace-bound RunLua source");
        source
            .write_all(b"return 42")
            .and_then(|_| source.flush())
            .expect("write RunLua source fixture");
        let source_path = source.path().to_path_buf();
        assert_eq!(
            source_path.parent(),
            Some(
                fs::canonicalize(workspace.path())
                    .expect("canonicalize workspace fixture")
                    .as_path()
            )
        );
        assert_eq!(
            fs::read_to_string(&source_path).expect("read open RunLua source fixture"),
            "return 42"
        );
        drop(source);
        assert!(!source_path.exists());
    }

    /// Verifies pending starts participate in the exact per-owner concurrency limit.
    /// 验证待启动预留会参与精确的每所有者并发限制。
    #[test]
    fn pending_runlua_starts_cannot_exceed_owner_limit() {
        let mut registry = RunLuaSessionRegistry::default();
        for index in 0..3 {
            let lease_id = format!("lease-{index}");
            registry.leases.insert(
                lease_id.clone(),
                RunLuaOwnedSession {
                    owner_session_id: String::from("owner-a"),
                    name: format!("session-{index}"),
                    sid: format!("sid-{index}"),
                    lease_id,
                    generation: 1,
                    ttl_sec: 600,
                },
            );
        }
        reserve_owner_session_start_in_registry(&mut registry, "owner-a", "session-3")
            .expect("fourth owner session should reserve successfully");
        let error = reserve_owner_session_start_in_registry(&mut registry, "owner-a", "session-4")
            .expect_err("fifth owner session must be rejected");
        assert!(error.contains("at most 4 active sessions"));
    }

    /// Verifies retryable close failures preserve ownership while terminal responses remove it.
    /// 验证可重试关闭失败会保留归属，而终态响应会移除归属。
    #[test]
    fn only_successful_or_terminal_close_releases_host_ownership() {
        assert!(
            !runtime_response_releases_host_ownership(
                &json!({"ok": false, "error_code": "lease_busy", "message": "busy"})
            )
            .expect("busy close response should be valid")
        );
        assert!(
            runtime_response_releases_host_ownership(
                &json!({"ok": false, "error_code": "lease_not_found", "message": "missing"})
            )
            .expect("missing lease response should be valid")
        );
        assert!(
            runtime_response_releases_host_ownership(&json!({"ok": true}))
                .expect("successful close response should be valid")
        );
    }
}
