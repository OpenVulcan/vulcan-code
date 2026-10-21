//! JavaScript code-mode tool surface backed by the vendored Codex sandbox host.
//! 由搬运自 Codex 的沙盒宿主支撑的 JavaScript 代码模式工具面。
//!
//! Models whose frozen route declares `tool_mode: code_mode` or `code_mode_only` see an `exec`
//! tool that runs JavaScript in the isolated `vulcan-code-mode-host` process and a `wait` tool
//! that resumes a yielded cell. Nested tool calls made from JavaScript return to the ordinary
//! host bridge, so plan-mode guards and the permission pipeline still admit every call.
//! 冻结路由声明 `tool_mode: code_mode` 或 `code_mode_only` 的模型会看到在隔离的
//! `vulcan-code-mode-host` 进程中运行 JavaScript 的 `exec` 工具，以及恢复已让出单元的 `wait`
//! 工具。JavaScript 中的嵌套工具调用回到普通宿主桥接层，因此计划模式守卫与权限管线仍逐次放行。
//!
//! Portions of the tool declarations, the exec grammar and the output header are adapted from
//! openai/codex (codex-rs/core/src/tools/code_mode) at c419fd01a70dd1d05356da443ac44328bda2e6f4,
//! licensed under Apache-2.0; see `crates/vulcan-code-mode/LICENSE-APACHE` and its `NOTICE.md`.
//! 工具声明、exec 语法与输出标题的部分内容改编自 openai/codex（codex-rs/core/src/tools/code_mode，
//! 提交 c419fd01a70dd1d05356da443ac44328bda2e6f4），以 Apache-2.0 授权；见
//! `crates/vulcan-code-mode/LICENSE-APACHE` 及其 `NOTICE.md`。

mod dispatch;
mod output;

use crate::agent_runtime::provider_protocol_tool_definition::{
    ProviderToolDefinition, resolved_tool_definition_to_provider_tool_definition,
};
use crate::agent_runtime::tool_runtime_bridge::VulcanManagedToolRuntimeBridge;
use crate::model::SessionId;
use dispatch::{ActiveDispatcher, CellChannel, CellDelegate};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};
use std::time::Instant;
use vulcan_code_mode::{
    CellId, CodeModeSession, CodeModeSessionProvider, CodeModeToolKind, DEFAULT_EXEC_YIELD_TIME_MS,
    DEFAULT_INPUT_SCHEMA_MAX_BYTES, DEFAULT_WAIT_YIELD_TIME_MS, DeferredToolDiscovery,
    ExecuteRequest, ImageDetailVisibility, PUBLIC_TOOL_NAME, ProcessOwnedCodeModeSessionProvider,
    RuntimeResponse, ToolDefinition as CodeModeToolDefinition, ToolName as CodeModeToolName,
    WAIT_TOOL_NAME, WaitOutcome, WaitRequest, augment_tool_definition, build_exec_tool_description,
    build_wait_tool_description, parse_exec_source,
};
use vulcan_core::types::ToolInvocationRequest as CoreToolInvocationRequest;
use vulcan_host_tools::native_host_binary::resolve_native_host_binary_in_cargo_target;
use vulcan_provider_adapters::ToolMode;
use vulcan_tools_manager::{
    ResolvedToolDefinition, VulcanToolInvocationContext, VulcanToolInvocationResult,
};

/// Binary stem of the isolated code-mode host shipped beside the service.
/// 与服务一同发布的隔离代码模式宿主二进制名。
const CODE_MODE_HOST_STEM: &str = "vulcan-code-mode-host";

/// Environment variable naming an explicit code-mode host executable.
/// 指定显式代码模式宿主可执行文件的环境变量。
const CODE_MODE_HOST_OVERRIDE: &str = "VULCAN_CODE_MODE_HOST";

/// Development target directory of the separate code-mode host Cargo workspace.
/// 独立代码模式宿主 Cargo 工作区的开发 target 目录。
const CODE_MODE_HOST_CARGO_TARGET: &str = "target/code-mode-host";

/// Source kind attached to the provider-visible `exec` and `wait` definitions.
/// 附加到 Provider 可见 `exec` 与 `wait` 定义上的来源类型。
pub(crate) const CODE_MODE_TOOL_SOURCE_KIND: &str = "code_mode";

/// Human-readable source label of the code-mode tools.
/// 代码模式工具的人类可读来源标签。
const CODE_MODE_TOOL_SOURCE_LABEL: &str = "Code mode";

/// Provider-visible name of the JavaScript execution tool.
/// JavaScript 执行工具的 Provider 可见名称。
pub(crate) const CODE_MODE_EXEC_TOOL_NAME: &str = PUBLIC_TOOL_NAME;

/// Lark grammar that constrains freeform `exec` input to JavaScript with an optional pragma line.
/// 将自由文本 `exec` 输入约束为可带首行指令的 JavaScript 的 Lark 语法。
///
/// Copied from openai/codex `codex-rs/core/src/tools/code_mode/execute_spec.rs`.
/// 复制自 openai/codex `codex-rs/core/src/tools/code_mode/execute_spec.rs`。
pub(crate) const CODE_MODE_EXEC_LARK_GRAMMAR: &str = r#"
start: pragma_source | plain_source
pragma_source: PRAGMA_LINE NEWLINE SOURCE
plain_source: SOURCE

PRAGMA_LINE: /[ \t]*\/\/ @exec:[^\r\n]*/
NEWLINE: /\r?\n/
SOURCE: /[\s\S]+/
"#;

/// Returns whether one protocol profile encodes freeform custom tools on the wire.
/// 返回一个协议 Profile 是否在线路上编码自由文本自定义工具。
///
/// Other profiles receive `exec` as an ordinary JSON function with a `code` string argument.
/// 其他 Profile 以带 `code` 字符串参数的普通 JSON 函数接收 `exec`。
pub(crate) fn protocol_accepts_freeform_exec(protocol_profile_id: &str) -> bool {
    protocol_profile_id == vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID
        || protocol_profile_id == vulcan_llm_core::OPENAI_CODEX_RESPONSES_PROFILE_ID
}

/// Process-wide host provider keyed by the resolved host executable.
/// 以已解析宿主可执行文件为键的进程级宿主提供者。
static HOST_PROVIDER: Mutex<Option<(PathBuf, Arc<ProcessOwnedCodeModeSessionProvider>)>> =
    Mutex::new(None);

/// Live JavaScript sessions keyed by the controller session that owns their stored values.
/// 以持有其存储值的控制器会话为键的活动 JavaScript 会话。
static SESSIONS: LazyLock<Mutex<HashMap<SessionId, Arc<CodeModeSessionSlot>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Whether the one-time fallback warning for `code_mode` has already been logged.
/// `code_mode` 的一次性回退警告是否已经记录。
static FALLBACK_WARNING_EMITTED: AtomicBool = AtomicBool::new(false);

/// Resolves the shared host provider, reusing it while the resolved executable is unchanged.
/// 解析共享宿主提供者；已解析可执行文件不变时复用。
fn host_provider() -> Result<Arc<ProcessOwnedCodeModeSessionProvider>, String> {
    let host_program = resolve_native_host_binary_in_cargo_target(
        CODE_MODE_HOST_STEM,
        CODE_MODE_HOST_OVERRIDE,
        CODE_MODE_HOST_CARGO_TARGET,
    )?;
    let mut cached = HOST_PROVIDER
        .lock()
        .map_err(|_| String::from("The code-mode host registry is unavailable."))?;
    if let Some((cached_program, provider)) = cached.as_ref()
        && *cached_program == host_program
    {
        return Ok(Arc::clone(provider));
    }
    let provider = Arc::new(ProcessOwnedCodeModeSessionProvider::with_host_program(
        host_program.clone(),
    ));
    provider.availability()?;
    *cached = Some((host_program, Arc::clone(&provider)));
    Ok(provider)
}

/// One controller session's lazily started JavaScript session.
/// 一个控制器会话惰性启动的 JavaScript 会话。
pub(crate) struct CodeModeSessionSlot {
    /// Host provider that created this slot.
    /// 创建此槽位的宿主提供者。
    provider: Arc<ProcessOwnedCodeModeSessionProvider>,
    /// Session created on the first `exec`; values stored by `store()` live here.
    /// 首次 `exec` 时创建的会话；`store()` 保存的值存在于此。
    session: tokio::sync::OnceCell<Arc<dyn CodeModeSession>>,
    /// Dispatch channels of cells not yet observed in a final state; they outlive runs.
    /// 尚未在终态被观察到的单元分派通道；它们可跨越多次运行。
    cells: Mutex<HashMap<CellId, Arc<CellChannel>>>,
}

impl CodeModeSessionSlot {
    /// Creates the slot of one controller session.
    /// 创建一个控制器会话的槽位。
    fn new(provider: Arc<ProcessOwnedCodeModeSessionProvider>) -> Self {
        Self {
            provider,
            session: tokio::sync::OnceCell::new(),
            cells: Mutex::new(HashMap::new()),
        }
    }

    /// Returns the live session, starting it on first use.
    /// 返回活动会话，首次使用时启动。
    async fn session(&self) -> Result<Arc<dyn CodeModeSession>, String> {
        self.session
            .get_or_try_init(|| self.provider.create_session())
            .await
            .map(Arc::clone)
    }

    /// Returns the dispatch channel of one live cell.
    /// 返回一个活动单元的分派通道。
    fn cell(&self, cell_id: &CellId) -> Option<Arc<CellChannel>> {
        self.cells
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(cell_id)
            .cloned()
    }

    /// Records the dispatch channel of one started cell.
    /// 记录一个已启动单元的分派通道。
    fn register_cell(&self, cell_id: CellId, channel: Arc<CellChannel>) {
        self.cells
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(cell_id, channel);
    }

    /// Takes the cell's pending notifications and forgets the cell once it reached a final state.
    /// 取出单元待交付的通知，并在单元到达终态后将其遗忘。
    fn observe(&self, response: &RuntimeResponse) -> Vec<String> {
        let (cell_id, closed) = match response {
            RuntimeResponse::Yielded { cell_id, .. } => (cell_id, false),
            RuntimeResponse::Terminated { cell_id, .. }
            | RuntimeResponse::Result { cell_id, .. } => (cell_id, true),
        };
        let mut cells = self.cells.lock().unwrap_or_else(PoisonError::into_inner);
        let channel = if closed {
            cells.remove(cell_id)
        } else {
            cells.get(cell_id).cloned()
        };
        channel
            .map(|channel| channel.take_notifications())
            .unwrap_or_default()
    }
}

/// Code-mode decision frozen for one controller run before its tools are resolved.
/// 在工具解析前为一次控制器运行冻结的代码模式决定。
#[derive(Clone)]
pub(crate) struct CodeModeActivation {
    /// Effective non-direct tool mode.
    /// 生效的非直接工具模式。
    tool_mode: ToolMode,
    /// Whether the frozen protocol carries tool-result images natively.
    /// 冻结协议是否原生承载工具结果图片。
    media: bool,
    /// Session slot of the controller session.
    /// 控制器会话的会话槽位。
    slot: Arc<CodeModeSessionSlot>,
}

/// Resolves whether one controller run uses code mode.
/// 解析一次控制器运行是否使用代码模式。
///
/// # Parameters / 参数
/// - `tool_mode`: Mode frozen into the execution route.
/// - `session_id`: Controller session that owns the JavaScript session.
/// - `protocol_profile_id`: Frozen protocol; it decides whether images reach the model.
/// - `tool_mode`：冻结进执行路由的模式。
/// - `session_id`：持有 JavaScript 会话的控制器会话。
/// - `protocol_profile_id`：冻结协议；决定图片能否交给模型。
///
/// # Returns / 返回值
/// `None` for direct tools, including `code_mode` when the host is unavailable (logged once);
/// an error when `code_mode_only` cannot start its host.
/// 直接工具时返回 `None`，包括宿主不可用时的 `code_mode`（只记录一次）；
/// `code_mode_only` 无法启动宿主时返回错误。
pub(crate) fn resolve_code_mode_activation(
    tool_mode: ToolMode,
    session_id: SessionId,
    protocol_profile_id: &str,
) -> Result<Option<CodeModeActivation>, String> {
    if tool_mode.is_direct() {
        return Ok(None);
    }
    let provider = match host_provider() {
        Ok(provider) => provider,
        Err(reason) if tool_mode == ToolMode::CodeMode => {
            if !FALLBACK_WARNING_EMITTED.swap(true, Ordering::Relaxed) {
                eprintln!("Code mode is unavailable; falling back to direct tools: {reason}");
            }
            return Ok(None);
        }
        Err(reason) => {
            return Err(format!(
                "This model requires code mode, but the code-mode host is unavailable: {reason}"
            ));
        }
    };
    let mut sessions = SESSIONS
        .lock()
        .map_err(|_| String::from("The code-mode session registry is unavailable."))?;
    let slot = sessions
        .entry(session_id)
        .or_insert_with(|| Arc::new(CodeModeSessionSlot::new(provider)))
        .clone();
    Ok(Some(CodeModeActivation {
        tool_mode,
        media: crate::agent_runtime::tool_result_media::protocol_projects_tool_result_media(
            protocol_profile_id,
        ),
        slot,
    }))
}

/// Builds the provider-visible tool list one frozen route exposes, without starting the host.
/// 构建一条冻结路由暴露的 Provider 可见工具列表，不启动宿主。
///
/// Requests outside the ordinary turn, such as remote compaction and the request preview, use
/// this so they carry the same `exec` and `wait` surface as the turn itself.
/// 普通回合之外的请求（如远端压缩与请求预览）使用此函数，使其携带与回合本身相同的
/// `exec` 与 `wait` 工具面。
///
/// # Parameters / 参数
/// - `tool_mode`: Mode frozen into the execution route.
/// - `session_id`: Controller session that owns the JavaScript session.
/// - `protocol_profile_id`: Frozen protocol of the route.
/// - `resolved_tools`: Tools resolved for the run.
/// - `rejected_tool_ids`: Tools the run rejects.
/// - `tool_mode`：冻结进执行路由的模式。
/// - `session_id`：持有 JavaScript 会话的控制器会话。
/// - `protocol_profile_id`：路由的冻结协议。
/// - `resolved_tools`：本次运行解析出的工具。
/// - `rejected_tool_ids`：本次运行拒绝的工具。
///
/// # Returns / 返回值
/// The same list an ordinary turn sends, or the error that turn would fail with.
/// 与普通回合发送的相同列表，或该回合将失败时的错误。
pub(crate) fn route_provider_tool_definitions(
    tool_mode: ToolMode,
    session_id: SessionId,
    protocol_profile_id: &str,
    resolved_tools: &[ResolvedToolDefinition],
    rejected_tool_ids: &[String],
) -> Result<Vec<ProviderToolDefinition>, String> {
    let direct = resolved_tools
        .iter()
        .map(resolved_tool_definition_to_provider_tool_definition)
        .collect::<Vec<_>>();
    Ok(
        match resolve_code_mode_activation(tool_mode, session_id, protocol_profile_id)? {
            Some(activation) => CodeModeTurn::new(activation, resolved_tools, rejected_tool_ids)
                .provider_tool_definitions(direct),
            None => direct,
        },
    )
}

/// Releases the JavaScript session of one controller session and its stored values.
/// 释放一个控制器会话的 JavaScript 会话及其存储值。
pub fn release_code_mode_session(session_id: SessionId) {
    let slot = match SESSIONS.lock() {
        Ok(mut sessions) => sessions.remove(&session_id),
        Err(_) => return,
    };
    let Some(session) = slot.and_then(|slot| slot.session.get().cloned()) else {
        return;
    };
    if let Ok(runtime) = tokio::runtime::Handle::try_current() {
        runtime.spawn(async move {
            if let Err(error) = session.shutdown().await {
                eprintln!("Failed to shut down a code-mode session: {error}");
            }
        });
    }
}

/// Code-mode tool surface of one controller run.
/// 一次控制器运行的代码模式工具面。
pub(crate) struct CodeModeTurn {
    /// Effective non-direct tool mode.
    /// 生效的非直接工具模式。
    tool_mode: ToolMode,
    /// Session slot shared with later runs of the same controller session.
    /// 与同一控制器会话后续运行共享的会话槽位。
    slot: Arc<CodeModeSessionSlot>,
    /// Nested tools advertised to JavaScript, without schemas, as the host expects them.
    /// 按宿主期望去掉 Schema 后通告给 JavaScript 的嵌套工具。
    enabled_tools: Vec<CodeModeToolDefinition>,
    /// Tool identifiers JavaScript may call.
    /// JavaScript 可以调用的工具标识。
    nested_tool_ids: Arc<BTreeSet<String>>,
    /// Model-facing `exec` description.
    /// 面向模型的 `exec` 说明。
    exec_description: String,
    /// Whether the route carries tool-result images natively.
    /// 路由是否原生承载工具结果图片。
    media: bool,
}

/// Vulcan-specific behavior appended to the upstream `exec` description.
/// 追加到上游 `exec` 说明之后的 Vulcan 专属行为。
const VULCAN_CELL_LIFECYCLE_NOTE: &str = "- In this host, a yielded cell keeps running across turns until it finishes, is terminated, or the session ends. Its nested tool calls pause until `exec` or `wait` observes that cell, and `notify(...)` text is delivered with the next `exec` or `wait` result for that cell.";

impl CodeModeTurn {
    /// Builds the run surface from the resolved host tools.
    /// 根据已解析宿主工具构建本次运行的工具面。
    ///
    /// # Parameters / 参数
    /// - `activation`: Frozen code-mode decision.
    /// - `resolved_tools`: Tools resolved for this run.
    /// - `rejected_tool_ids`: Tools the run rejects; JavaScript cannot call them either.
    /// - `activation`：冻结的代码模式决定。
    /// - `resolved_tools`：本次运行解析出的工具。
    /// - `rejected_tool_ids`：本次运行拒绝的工具；JavaScript 同样不能调用。
    pub(crate) fn new(
        activation: CodeModeActivation,
        resolved_tools: &[ResolvedToolDefinition],
        rejected_tool_ids: &[String],
    ) -> Self {
        let mut described = resolved_tools
            .iter()
            .filter(|tool| {
                vulcan_code_mode::is_code_mode_nested_tool(&tool.descriptor.id)
                    && !rejected_tool_ids.contains(&tool.descriptor.id)
            })
            .map(|tool| CodeModeToolDefinition {
                name: tool.descriptor.id.clone(),
                tool_name: CodeModeToolName::plain(tool.descriptor.id.clone()),
                description: tool.descriptor.description.clone(),
                kind: CodeModeToolKind::Function,
                input_schema: Some(tool.parameters_schema.clone()),
                input_schema_max_bytes: Some(DEFAULT_INPUT_SCHEMA_MAX_BYTES),
                output_schema: None,
            })
            .collect::<Vec<_>>();
        described.sort_by(|left, right| left.name.cmp(&right.name));
        described.dedup_by(|left, right| left.name == right.name);
        let code_mode_only = activation.tool_mode == ToolMode::CodeModeOnly;
        let mut exec_description = build_exec_tool_description(
            &described,
            &[],
            &BTreeMap::new(),
            DEFAULT_EXEC_YIELD_TIME_MS,
            code_mode_only,
            ImageDetailVisibility::Hidden,
            DeferredToolDiscovery::Catalog,
            None,
        );
        exec_description.push('\n');
        exec_description.push_str(VULCAN_CELL_LIFECYCLE_NOTE);
        let nested_tool_ids = described
            .iter()
            .map(|tool| tool.name.clone())
            .collect::<BTreeSet<_>>();
        let enabled_tools = described
            .into_iter()
            .map(|tool| {
                let mut tool = augment_tool_definition(tool);
                tool.input_schema = None;
                tool.output_schema = None;
                tool
            })
            .collect();
        Self {
            tool_mode: activation.tool_mode,
            slot: activation.slot,
            enabled_tools,
            nested_tool_ids: Arc::new(nested_tool_ids),
            exec_description,
            media: activation.media,
        }
    }

    /// Returns whether direct tool calls are refused because every tool lives inside `exec`.
    /// 返回是否因为所有工具都位于 `exec` 内而拒绝直接工具调用。
    pub(crate) fn is_code_mode_only(&self) -> bool {
        self.tool_mode == ToolMode::CodeModeOnly
    }

    /// Returns whether this surface owns one provider-requested tool name.
    /// 返回此工具面是否拥有一个 Provider 请求的工具名。
    pub(crate) fn owns_tool(&self, tool_name: &str) -> bool {
        tool_name == PUBLIC_TOOL_NAME || tool_name == WAIT_TOOL_NAME
    }

    /// Builds the provider-visible tool list for this run.
    /// 构建本次运行的 Provider 可见工具列表。
    ///
    /// `direct` is the ordinary list; `code_mode` keeps it beside `exec` and `wait`, while
    /// `code_mode_only` exposes only the two code-mode tools.
    /// `direct` 是普通列表；`code_mode` 将其保留在 `exec` 与 `wait` 旁边，`code_mode_only`
    /// 只暴露这两个代码模式工具。
    pub(crate) fn provider_tool_definitions(
        &self,
        direct: Vec<ProviderToolDefinition>,
    ) -> Vec<ProviderToolDefinition> {
        let mut tools = vec![
            ProviderToolDefinition {
                name: String::from(PUBLIC_TOOL_NAME),
                description: self.exec_description.clone(),
                source_kind: String::from(CODE_MODE_TOOL_SOURCE_KIND),
                source_label: String::from(CODE_MODE_TOOL_SOURCE_LABEL),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "code": {
                            "type": "string",
                            "description": "Raw JavaScript source text, optionally starting with a `// @exec:` pragma line.",
                        },
                    },
                    "required": ["code"],
                    "additionalProperties": false,
                }),
            },
            ProviderToolDefinition {
                name: String::from(WAIT_TOOL_NAME),
                description: format!(
                    "Waits on a yielded `{PUBLIC_TOOL_NAME}` cell and returns new output or completion.\n{}",
                    build_wait_tool_description().trim()
                ),
                source_kind: String::from(CODE_MODE_TOOL_SOURCE_KIND),
                source_label: String::from(CODE_MODE_TOOL_SOURCE_LABEL),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "cell_id": {
                            "type": "string",
                            "description": "Identifier of the running exec cell.",
                        },
                        "yield_time_ms": {
                            "type": "number",
                            "description": "Wait before yielding more output. Defaults to 10000 ms.",
                        },
                        "max_tokens": {
                            "type": "number",
                            "description": "Output token budget for this wait call. Defaults to 10000 tokens.",
                        },
                        "terminate": {
                            "type": "boolean",
                            "description": "True stops the running exec cell; false or omitted waits for output.",
                        },
                    },
                    "required": ["cell_id"],
                    "additionalProperties": false,
                }),
            },
        ];
        if !self.is_code_mode_only() {
            tools.extend(
                direct
                    .into_iter()
                    .filter(|tool| !self.owns_tool(&tool.name)),
            );
        }
        tools
    }

    /// Runs one `exec` or `wait` call.
    /// 运行一次 `exec` 或 `wait` 调用。
    ///
    /// # Parameters / 参数
    /// - `bridge`: Host bridge whose permission pipeline admits nested calls.
    /// - `request`: Provider-requested call owned by this surface.
    /// - `context`: Durable identity of that call.
    /// - `bridge`：以其权限管线放行嵌套调用的宿主桥接层。
    /// - `request`：此工具面拥有的 Provider 请求调用。
    /// - `context`：该调用的可靠身份。
    ///
    /// # Returns / 返回值
    /// The model-visible result, or an error the bridge reports as a tool failure.
    /// 模型可见结果，或由桥接层报告为工具失败的错误。
    pub(crate) async fn invoke(
        &self,
        bridge: &VulcanManagedToolRuntimeBridge,
        request: &CoreToolInvocationRequest,
        context: VulcanToolInvocationContext,
    ) -> Result<VulcanToolInvocationResult, String> {
        if request.tool_name == PUBLIC_TOOL_NAME {
            self.execute(bridge, request, context).await
        } else {
            self.wait(bridge, request, context).await
        }
    }

    /// Builds the dispatcher that runs nested calls under one observing call's identity.
    /// 构建在一次观察调用的身份下运行嵌套调用的分派器。
    fn dispatcher(
        &self,
        bridge: &VulcanManagedToolRuntimeBridge,
        request: &CoreToolInvocationRequest,
        context: VulcanToolInvocationContext,
    ) -> ActiveDispatcher {
        ActiveDispatcher::new(
            bridge.without_code_mode(),
            Arc::clone(&self.nested_tool_ids),
            context,
            request.user_request.text().map(str::to_owned),
        )
    }

    /// Starts one JavaScript cell and returns its first observation.
    /// 启动一个 JavaScript 单元并返回其首个观察结果。
    async fn execute(
        &self,
        bridge: &VulcanManagedToolRuntimeBridge,
        request: &CoreToolInvocationRequest,
        context: VulcanToolInvocationContext,
    ) -> Result<VulcanToolInvocationResult, String> {
        let source = request
            .arguments
            .get("code")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                format!("`{PUBLIC_TOOL_NAME}` expects JavaScript source in the `code` argument.")
            })?;
        let parsed = parse_exec_source(source)?;
        let started_at = Instant::now();
        let session = self.slot.session().await?;
        let channel = CellChannel::new();
        let lease = channel.observe(self.dispatcher(bridge, request, context));
        let started = session
            .execute(
                ExecuteRequest {
                    tool_call_id: request.tool_call_id.clone(),
                    enabled_tools: self.enabled_tools.clone(),
                    source: parsed.code,
                    yield_time_ms: Some(parsed.yield_time_ms.unwrap_or(DEFAULT_EXEC_YIELD_TIME_MS)),
                    max_output_tokens: parsed.max_output_tokens,
                },
                Arc::new(CellDelegate::new(Arc::clone(&channel))),
                None,
            )
            .await?;
        self.slot.register_cell(started.cell_id.clone(), channel);
        let response = started.initial_response().await;
        lease.finish().await;
        Ok(self.render(response?, parsed.max_output_tokens, started_at))
    }

    /// Resumes or terminates one yielded cell.
    /// 恢复或终止一个已让出的单元。
    async fn wait(
        &self,
        bridge: &VulcanManagedToolRuntimeBridge,
        request: &CoreToolInvocationRequest,
        context: VulcanToolInvocationContext,
    ) -> Result<VulcanToolInvocationResult, String> {
        let arguments = serde_json::from_value::<WaitArguments>(request.arguments.clone())
            .map_err(|error| format!("Invalid `{WAIT_TOOL_NAME}` arguments: {error}"))?;
        let started_at = Instant::now();
        let cell_id = CellId::new(arguments.cell_id);
        let session = self.slot.session().await?;
        let outcome = if arguments.terminate {
            session.terminate(cell_id).await
        } else {
            // This call observes the cell, so its pending nested calls run under this identity.
            // 本次调用观察该单元，因此其等待中的嵌套调用在本身份下运行。
            let lease = self
                .slot
                .cell(&cell_id)
                .map(|channel| channel.observe(self.dispatcher(bridge, request, context)));
            let outcome = session
                .wait(
                    WaitRequest {
                        cell_id,
                        yield_time_ms: arguments
                            .yield_time_ms
                            .unwrap_or(DEFAULT_WAIT_YIELD_TIME_MS),
                    },
                    None,
                )
                .await;
            if let Some(lease) = lease {
                lease.finish().await;
            }
            outcome
        };
        let response = match outcome? {
            WaitOutcome::LiveCell(response) | WaitOutcome::MissingCell(response) => response,
        };
        Ok(self.render(response, arguments.max_tokens, started_at))
    }

    /// Renders one observation together with the cell's pending notifications.
    /// 将一次观察结果与单元待交付的通知一起渲染。
    fn render(
        &self,
        response: RuntimeResponse,
        max_output_tokens: Option<usize>,
        started_at: Instant,
    ) -> VulcanToolInvocationResult {
        let notifications = self.slot.observe(&response);
        output::render_runtime_response(
            response,
            max_output_tokens,
            started_at.elapsed(),
            notifications,
            self.media,
        )
    }
}

/// Arguments of the `wait` tool.
/// `wait` 工具的参数。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WaitArguments {
    /// Cell to resume.
    /// 要恢复的单元。
    cell_id: String,
    /// Time to wait before yielding again.
    /// 再次让出前的等待时间。
    #[serde(default)]
    yield_time_ms: Option<u64>,
    /// Output token budget of this call.
    /// 本次调用的输出 Token 预算。
    #[serde(default)]
    max_tokens: Option<usize>,
    /// Whether to stop the cell instead of waiting.
    /// 是否停止单元而不是等待。
    #[serde(default)]
    terminate: bool,
}

#[cfg(test)]
mod tests;
