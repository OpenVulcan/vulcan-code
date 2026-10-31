//! LuaSkills runtime handle, entry listing and invocation helpers.
//! LuaSkills 运行时句柄、入口列举与调用辅助模块。

use crate::model::SessionId;
use crate::project_temp_maintenance;
use crate::runtime_assets;
use crate::tool_result_format::{
    DEFAULT_INLINE_TOOL_RESULT_BYTES, HostRenderOptions, HostTextRenderInput, HostToolOverflowMode,
    ToolResultBudgetSnapshot, render_tool_result_text,
};
use crate::tooling::change_set::{
    change_set_host_verification_notice, luaskills_change_set_host_result_capabilities,
};
use futures_channel::oneshot;
use luaskills::{
    LuaEngine, LuaInvocationContext, RuntimeClientInfo, RuntimeEntryDescriptor,
    RuntimeInvocationResult, RuntimeRequestContext, RuntimeSkillRoot, SkillManagementAuthority,
};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

pub mod database_bridge;
pub mod database_callbacks;
pub mod host_database_providers;
mod runlua;

/// Process-local sequence used to correlate all diagnostics from one LuaSkills host invocation.
/// 用于关联同一次 LuaSkills 宿主调用全部诊断的进程内序列。
static LUASKILLS_HOST_INVOCATION_SEQUENCE: AtomicU64 = AtomicU64::new(1);

mod schema;
mod types;

#[cfg(test)]
mod concurrency_tests;

pub use runlua::{
    RUNLUA_SESSION_TOOL_NAME, RUNLUA_TOOL_NAME, build_runlua_tool_candidates,
    set_runlua_ai_tools_enabled,
};
pub use schema::{
    LUASKILL_SID_PARAMETER_NAME, LUASKILLS_HOST_MANAGED_SID_PREFIX, PWD_PARAMETER_NAME,
    luaskills_entry_args_schema,
};
pub use types::*;
use types::{RunLuaOwnedSession, RunLuaSessionRegistry};

/// Execute one LuaSkills host operation on a dedicated blocking thread without stalling the async caller.
/// 在专用阻塞线程上执行单次 LuaSkills 宿主操作，并避免阻塞异步调用方。
async fn run_luaskills_blocking_task_async_with_diagnostics<T, F>(
    thread_name: &str,
    operation_label: String,
    admission: Arc<Semaphore>,
    operation: F,
) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(Arc<AtomicU8>, u64) -> Result<T, String> + Send + 'static,
{
    // stage distinguishes queueing, engine-lock contention, skill execution, and terminal delivery.
    // stage 用于区分排队、引擎锁竞争、技能执行与终态投递。
    let stage = Arc::new(AtomicU8::new(0));
    let thread_name = thread_name.to_string();
    // invocation_id correlates queue, lock, body, and result-delivery diagnostics for one call.
    // invocation_id 关联一次调用的排队、锁、技能体和结果投递诊断。
    let invocation_id = LUASKILLS_HOST_INVOCATION_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    // started_at includes admission wait so queue pressure remains observable before any OS thread exists.
    // started_at 包含入场等待，使系统线程尚未创建时的排队压力仍可观测。
    let started_at = Instant::now();
    // permit_future queues only the async caller and is cancelled without creating a worker thread.
    // permit_future 只排队异步调用方；取消时不会创建工作线程。
    let permit_future = admission.acquire_owned();
    tokio::pin!(permit_future);
    // next_diagnostic_delay emits once at five seconds, again at thirty seconds, then once per minute.
    // next_diagnostic_delay 在五秒、三十秒以及之后每分钟输出一次诊断。
    let mut next_diagnostic_delay = Duration::from_secs(5);
    let permit = loop {
        tokio::select! {
            permit = &mut permit_future => {
                break permit.map_err(|_| format!(
                    "LuaSkills host admission for `{operation_label}` is closed"
                ))?;
            }
            _ = tokio::time::sleep(next_diagnostic_delay) => {
                let elapsed_ms = started_at.elapsed().as_millis();
                eprintln!(
                    "[luaskills-runtime][invoke-still-running] invocation_id={invocation_id} operation={operation_label} stage=host_admission_wait elapsed_ms={elapsed_ms}"
                );
                next_diagnostic_delay = if elapsed_ms < 30_000 {
                    Duration::from_secs(25)
                } else {
                    Duration::from_secs(60)
                };
            }
        }
    };

    let (sender, receiver) = oneshot::channel();
    let worker_stage = Arc::clone(&stage);

    std::thread::Builder::new()
        .name(thread_name.clone())
        .spawn(move || {
            // permit stays owned by the worker even if the async waiter is cancelled.
            // 即使异步等待方被取消，permit 仍由工作线程持有。
            let _permit = permit;
            worker_stage.store(1, Ordering::Release);
            let operation_stage = Arc::clone(&worker_stage);
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                operation(operation_stage, invocation_id)
            }))
            .map_err(|panic_payload| {
                let panic_message = if let Some(message) = panic_payload.downcast_ref::<String>() {
                    message.clone()
                } else if let Some(message) = panic_payload.downcast_ref::<&'static str>() {
                    (*message).to_string()
                } else {
                    String::from("unknown panic")
                };
                format!("LuaSkills blocking worker panicked: {panic_message}")
            })
            .and_then(|value| value);
            worker_stage.store(3, Ordering::Release);
            let _ = sender.send(result);
        })
        .map_err(|error| format!("failed to start LuaSkills blocking worker: {error}"))?;

    // receiver remains pinned to the same worker result while diagnostics wake independently.
    // receiver 在诊断独立唤醒期间始终绑定同一工作线程结果。
    let mut receiver = receiver;
    // Reset diagnostics after admission while preserving total elapsed time in emitted measurements.
    // 入场后重置诊断周期，同时在输出测量中保留总耗时。
    next_diagnostic_delay = Duration::from_secs(5);
    loop {
        tokio::select! {
            result = &mut receiver => {
                return result
                    .map_err(|_| format!("LuaSkills blocking worker `{thread_name}` disconnected unexpectedly"))?;
            }
            _ = tokio::time::sleep(next_diagnostic_delay) => {
                let elapsed_ms = started_at.elapsed().as_millis();
                let stage_label = match stage.load(Ordering::Acquire) {
                    0 => "thread_queued",
                    1 => "engine_read_lock_wait",
                    2 => "skill_body_or_host_call",
                    3 => "result_delivery",
                    4 => "blocking_runtime_operation",
                    5 => "engine_write_lock_wait",
                    _ => "unknown",
                };
                eprintln!(
                    "[luaskills-runtime][invoke-still-running] invocation_id={invocation_id} operation={operation_label} stage={stage_label} elapsed_ms={elapsed_ms}"
                );
                next_diagnostic_delay = if elapsed_ms < 30_000 {
                    Duration::from_secs(25)
                } else {
                    Duration::from_secs(60)
                };
            }
        }
    }
}

/// Executes one unlabeled LuaSkills host operation through the shared stage watchdog.
/// 通过共享阶段看门狗执行一次无独立标签的 LuaSkills 宿主操作。
///
/// # Parameters
/// 参数
/// `thread_name` is both the worker-thread name and stable diagnostic label.
/// `thread_name` 同时作为工作线程名称与稳定诊断标签。
/// `operation` is the blocking LuaSkills operation executed on that worker.
/// `operation` 是在该工作线程上执行的阻塞 LuaSkills 操作。
///
/// # Returns
/// 返回值
/// The operation result or the exact worker startup/delivery failure.
/// 返回操作结果或精确的工作线程启动/投递失败。
async fn run_luaskills_blocking_task_async<T, F>(
    thread_name: &str,
    admission: Arc<Semaphore>,
    operation: F,
) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    run_luaskills_blocking_task_async_with_diagnostics(
        thread_name,
        thread_name.to_string(),
        admission,
        move |stage, _invocation_id| {
            stage.store(4, Ordering::Release);
            operation()
        },
    )
    .await
}

/// List every LuaSkills runtime entry currently visible to the host tool surface.
/// 列出当前对宿主工具表面可见的全部 LuaSkills 运行时入口。
pub fn list_luaskills_tool_entries(
    handle: &LuaSkillsRuntimeHandle,
) -> Result<Vec<RuntimeEntryDescriptor>, String> {
    handle
        .tool_entries
        .lock()
        .map(|entries| entries.clone())
        .map_err(|_| {
            String::from(
                "LuaSkills tool entry cache lock is poisoned; the tool table cannot be read",
            )
        })
}

/// One rendered help node of a LuaSkills package.
/// LuaSkills 包中一个已渲染的帮助节点。
#[derive(Debug, Clone)]
pub struct LuaSkillsHelpNode {
    /// Help flow name: the main flow or one declared topic.
    /// 帮助流程名：主流程或一个已声明主题。
    pub flow_name: String,
    /// One-line node description declared by the package.
    /// 包声明的单行节点说明。
    pub description: String,
    /// Rendered Markdown body.
    /// 已渲染的 Markdown 正文。
    pub content: String,
}

/// Rendered help tree of one LuaSkills package.
/// 单个 LuaSkills 包的已渲染帮助树。
#[derive(Debug, Clone)]
pub struct LuaSkillsHelpTree {
    /// Stable skill identifier.
    /// 稳定技能标识。
    pub skill_id: String,
    /// Main help node; its content is empty when rendering failed.
    /// 主帮助节点；渲染失败时正文为空。
    pub main: LuaSkillsHelpNode,
    /// Successfully rendered topics in declaration order.
    /// 按声明顺序排列的已成功渲染主题。
    pub topics: Vec<LuaSkillsHelpNode>,
    /// Rendered topic name keyed by canonical entry name; absent entries are covered by main help.
    /// 以 canonical 入口名为键的已渲染主题名；未出现的入口由主帮助覆盖。
    pub entry_topics: BTreeMap<String, String>,
}

/// Render the help trees of the given LuaSkills packages.
/// 渲染指定 LuaSkills 包的帮助树。
///
/// # Parameters
/// 参数
/// - `handle`: Shared LuaSkills runtime handle.
/// - `handle`：共享 LuaSkills 运行时句柄。
/// - `skill_ids`: Packages whose help is needed.
/// - `skill_ids`：需要帮助的包。
///
/// # Returns
/// 返回值
/// Help trees plus one warning per help node that failed to render, or the help listing failure.
/// 帮助树以及每个渲染失败帮助节点的一条警告；帮助列表失败时返回错误。
pub fn render_luaskills_help_trees(
    handle: &LuaSkillsRuntimeHandle,
    skill_ids: &BTreeSet<String>,
) -> Result<(Vec<LuaSkillsHelpTree>, Vec<String>), String> {
    let engine = lock_runtime_engine(handle)?;
    let descriptors = engine.list_skill_help_for_authority(SkillManagementAuthority::System)?;
    let mut warnings = Vec::new();
    let mut render_node = |skill_id: &str, flow_name: &str| -> Option<String> {
        match engine.render_skill_help_detail_for_authority(
            SkillManagementAuthority::System,
            skill_id,
            flow_name,
            None,
        ) {
            Ok(Some(detail)) => Some(detail.content),
            Ok(None) => None,
            Err(error) => {
                warnings.push(format!(
                    "Failed to render help `{flow_name}` of LuaSkills package `{skill_id}`: {error}"
                ));
                None
            }
        }
    };
    let mut trees = Vec::new();
    for descriptor in descriptors
        .into_iter()
        .filter(|descriptor| skill_ids.contains(&descriptor.skill_id))
    {
        let skill_id = descriptor.skill_id;
        let main_content = render_node(&skill_id, &descriptor.main.flow_name).unwrap_or_default();
        let mut topics = Vec::new();
        let mut entry_topics = BTreeMap::new();
        for flow in descriptor.flows {
            let Some(content) = render_node(&skill_id, &flow.flow_name) else {
                continue;
            };
            for entry in flow.related_entries {
                entry_topics.insert(entry, flow.flow_name.clone());
            }
            topics.push(LuaSkillsHelpNode {
                flow_name: flow.flow_name,
                description: flow.description,
                content,
            });
        }
        trees.push(LuaSkillsHelpTree {
            skill_id,
            main: LuaSkillsHelpNode {
                flow_name: descriptor.main.flow_name,
                description: descriptor.main.description,
                content: main_content,
            },
            topics,
            entry_topics,
        });
    }
    Ok((trees, warnings))
}

/// Call one LuaSkills runtime entry on a dedicated blocking thread and return the structured result.
/// 在专用阻塞线程上调用单个 LuaSkills 运行时入口，并返回结构化结果。
pub async fn call_luaskills_tool_entry(
    handle: LuaSkillsRuntimeHandle,
    tool_name: String,
    args: Value,
    invocation_context: Option<LuaInvocationContext>,
) -> Result<RuntimeInvocationResult, String> {
    let operation_label = tool_name.clone();
    let admission = Arc::clone(&handle.skill_call_admission);
    run_luaskills_blocking_task_async_with_diagnostics(
        "luaskills-tool-call",
        operation_label,
        admission,
        move |stage, invocation_id| {
        // lock_started_at measures only host read-lock contention for the shared LuaSkills engine.
        // lock_started_at 仅测量共享 LuaSkills 引擎的宿主读锁竞争耗时。
        let lock_started_at = Instant::now();
        let engine = lock_runtime_engine(&handle)?;
        let lock_wait_ms = lock_started_at.elapsed().as_millis();
        if lock_wait_ms >= 1_000 {
                eprintln!(
                    "[luaskills-runtime][engine-read-lock-slow] invocation_id={invocation_id} operation={tool_name} elapsed_ms={lock_wait_ms}"
                );
        }
        stage.store(2, Ordering::Release);
        // invocation_started_at separates engine contention from time spent inside Lua or one host call.
        // invocation_started_at 将引擎竞争耗时与 Lua 或宿主调用内部耗时分离。
        let invocation_started_at = Instant::now();
        let result = engine.call_skill(&tool_name, &args, invocation_context.as_ref());
        let invocation_elapsed_ms = invocation_started_at.elapsed().as_millis();
        if invocation_elapsed_ms >= 1_000 {
                eprintln!(
                    "[luaskills-runtime][skill-call-slow] invocation_id={invocation_id} operation={tool_name} elapsed_ms={invocation_elapsed_ms} status={}",
                if result.is_ok() { "ok" } else { "error" }
            );
        }
        result
        },
    )
    .await
}

/// Render one LuaSkills invocation result into the final host-managed text returned to the model.
/// 将单次 LuaSkills 调用结果渲染为返回给模型的最终宿主管理文本。
///
/// # Parameters
/// # 参数
/// - `handle`: Runtime handle that supplies skill and shared template roots.
/// - `handle`：提供技能模板根目录和共享模板根目录的运行时句柄。
/// - `tool_name`: Canonical tool name used to resolve the client budget.
/// - `tool_name`：用于解析客户端预算的规范工具名称。
/// - `skill_name`: Optional skill identity used to select a local overflow template.
/// - `skill_name`：用于选择本地超限模板的可选技能标识。
/// - `result`: Exact LuaSkills runtime invocation result.
/// - `result`：精确的 LuaSkills 运行时调用结果。
/// - `workspace_root`: Optional project root used to derive the lazy spill path.
/// - `workspace_root`：用于派生延迟超限路径的可选项目根目录。
///
/// # Returns
/// # 返回值
/// Returns rendered model text, or the exact host rendering and persistence failure.
/// 返回渲染后的模型文本，或精确的宿主渲染与持久化失败。
pub fn render_luaskills_invocation_result_text(
    handle: &LuaSkillsRuntimeHandle,
    tool_name: &str,
    skill_name: Option<&str>,
    result: &RuntimeInvocationResult,
    workspace_root: Option<&Path>,
) -> Result<String, String> {
    let client_budget = build_default_luaskills_budget_snapshot(tool_name);
    let spill_root = workspace_root
        .filter(|path| !path.as_os_str().is_empty())
        .map(project_temp_maintenance::resolve_project_temp_dir);
    let render_options = HostRenderOptions {
        spill_root,
        template_skill_roots: handle
            .skill_roots
            .iter()
            .map(|root| root.skills_dir.clone())
            .collect(),
        template_resources_root: Some(handle.runtime_root.join("resources")),
    };

    render_tool_result_text(
        &host_text_render_input_from_runtime_result(result),
        skill_name,
        &client_budget,
        &render_options,
    )
}

/// Convert one LuaSkills runtime result into the host-owned generic text render input.
/// 将单条 LuaSkills 运行时结果转换为宿主自有的通用文本渲染输入。
fn host_text_render_input_from_runtime_result(
    result: &RuntimeInvocationResult,
) -> HostTextRenderInput {
    let mut content = result.content.clone();
    if let Some(host_result) = result.host_result.as_ref()
        && let Some(verification_notice) = (host_result.kind == "change_set")
            .then(|| change_set_host_verification_notice(&host_result.payload))
            .flatten()
    {
        if !content.trim().is_empty() {
            content.push_str("\n\n");
        }
        content.push_str(&verification_notice);
    }
    let normalized = normalize_host_render_text(&content);
    HostTextRenderInput {
        content,
        overflow_mode: result.overflow_mode.map(|mode| match mode {
            luaskills::ToolOverflowMode::Truncate => HostToolOverflowMode::Truncate,
            luaskills::ToolOverflowMode::Page => HostToolOverflowMode::Page,
        }),
        template_hint: result.template_hint.clone(),
        content_bytes: normalized.len(),
        content_lines: if normalized.is_empty() {
            0
        } else {
            normalized.split('\n').count()
        },
    }
}

/// Build one default LuaSkills invocation context used by host-managed tool calls.
/// 构建一份供宿主管理工具调用使用的默认 LuaSkills 调用上下文。
pub fn build_luaskills_tool_invocation_context(
    session_id: Option<SessionId>,
    transport_name: &str,
    tool_name: &str,
) -> LuaInvocationContext {
    let request_context = RuntimeRequestContext {
        request_id: Some(format!("luaskills-tool:{tool_name}")),
        client_name: Some(String::from("vulcan-code")),
        transport_name: Some(transport_name.to_string()),
        session_id: session_id.map(|value| value.to_string()),
        client_info: Some(RuntimeClientInfo {
            kind: Some(String::from("desktop")),
            name: Some(String::from("VulcanCode")),
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
        }),
        client_capabilities: luaskills_change_set_host_result_capabilities(),
    };
    let client_budget = build_default_luaskills_client_budget(tool_name);

    LuaInvocationContext::new(
        Some(request_context),
        client_budget,
        Value::Object(Map::new()),
    )
}

/// Normalizes host-render text so byte and line metrics stay deterministic after host-side warning injection.
/// 对宿主渲染文本进行规范化，使宿主侧注入警告后的字节与行数统计保持确定性。
fn normalize_host_render_text(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// Builds one default client-budget snapshot for host-driven LuaSkills tool calls with unlimited lines and a 200KB byte ceiling.
/// 为宿主驱动的 LuaSkills 工具调用构建一份默认客户端预算快照，采用无限行与 200KB 字节上限。
fn build_default_luaskills_client_budget(tool_name: &str) -> Value {
    let budget = build_default_luaskills_budget_snapshot(tool_name);
    json!({
        "client_name": "vulcan-code",
        "tool_name": tool_name,
        "matched_client_pattern": "vulcan-code",
        "tool_result": {
            "bytes": budget.tool_result.bytes,
            "lines": budget.tool_result.lines
        },
        "file_read": {
            "bytes": budget.file_read.bytes,
            "lines": budget.file_read.lines
        },
        "tool_config": {}
    })
}

/// Builds one strongly typed default budget snapshot reused by host-side result rendering and Lua invocation context injection.
/// 构建一份强类型默认预算快照，供宿主侧结果渲染与 Lua 调用上下文注入复用。
fn build_default_luaskills_budget_snapshot(_tool_name: &str) -> ToolResultBudgetSnapshot {
    ToolResultBudgetSnapshot::uniform_bytes(DEFAULT_INLINE_TOOL_RESULT_BYTES)
}

/// Acquire one immutable runtime-engine guard from the shared host handle.
/// 从共享宿主句柄中获取一份只读运行时引擎锁守卫。
///
/// Multiple read guards may coexist, so independent skill, runLua and plugin calls overlap instead
/// of being serialized by the host. The guard must not be held across `.await`.
/// 多份读守卫可以并存，因此相互独立的技能、runLua 与插件调用可以重叠，而不再被宿主串行化。
/// 该守卫不得跨越 `.await` 持有。
fn lock_runtime_engine(
    handle: &LuaSkillsRuntimeHandle,
) -> Result<std::sync::RwLockReadGuard<'_, LuaEngine>, String> {
    handle.engine.read().map_err(|_| {
        String::from("LuaSkills runtime read lock is poisoned; the skill catalog cannot be read")
    })
}

/// Acquire one mutable runtime-engine guard from the shared host handle.
/// 从共享宿主句柄中获取一份可变运行时引擎锁守卫。
///
/// The write guard waits for every in-flight read guard to drain, which is exactly the engine
/// change barrier required by upstream methods that take `&mut LuaEngine`.
/// 写守卫会等待所有在途读守卫排空，这正是取 `&mut LuaEngine` 的上游方法所需的引擎变更屏障。
#[cfg(test)]
fn lock_runtime_engine_mut(
    handle: &LuaSkillsRuntimeHandle,
) -> Result<std::sync::RwLockWriteGuard<'_, LuaEngine>, String> {
    handle
        .engine
        .write()
        .map_err(|_| String::from("LuaSkills 运行时写锁已损坏，无法执行技能管理操作。"))
}
