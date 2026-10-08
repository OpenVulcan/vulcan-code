//! Project-level terminal service handlers and visible-display subscriptions.
//! 项目级终端服务处理器与可见显示订阅。

use super::*;
use base64::Engine;
use vulcan_host_tools::project_terminal_runtime::{
    ProjectTerminalOutputChunk, ProjectTerminalRuntime, ProjectTerminalSnapshot,
    ProjectTerminalStartRequest, ProjectTerminalStatus,
};
use vulcan_host_tools::shell_host::HostCommandShellSupport;

/// Maximum retained fragments returned by one visible project terminal attachment.
/// 单个可见项目终端挂接时返回的最大保留分片数。
const PROJECT_TERMINAL_DISPLAY_BOOTSTRAP_MAX_CHUNKS: usize = 2048;

/// Maximum changed fragments carried by one project terminal push event.
/// 单个项目终端推送事件承载的最大变化分片数。
const PROJECT_TERMINAL_DISPLAY_EVENT_MAX_CHUNKS: usize = 512;

/// Bounds one acknowledged browser input batch before it reaches the host protocol pipe.
/// 限制单个已确认浏览器输入批次在进入宿主协议管道前的大小。
pub(crate) const PROJECT_TERMINAL_INPUT_BATCH_MAX_BYTES: usize = 64 * 1024;

/// Bounds input event metadata carried by one browser batch.
/// 限制单个浏览器批次承载的输入事件元数据数量。
pub(crate) const PROJECT_TERMINAL_INPUT_BATCH_MAX_EVENTS: usize = 256;

/// Stores cancellation and ownership for one visible project terminal display.
/// 保存一个可见项目终端显示层的取消句柄与归属信息。
struct ProjectTerminalDisplaySubscription {
    /// Browser or WebView lifetime that owns this display.
    /// 持有该显示层的浏览器或 WebView 生命周期。
    client_id: Uuid,
    /// Exact owning project identity.
    /// 精确归属项目标识。
    project_id: String,
    /// Exact terminal identity.
    /// 精确终端标识。
    terminal_id: Uuid,
    /// Cancels content delivery when the display becomes hidden.
    /// 显示层隐藏时取消内容交付。
    cancel: tokio::sync::oneshot::Sender<()>,
}

/// Owns project terminals independently from AI session runtimes.
/// 独立于 AI 会话运行时持有项目终端。
static PROJECT_TERMINAL_RUNTIME: std::sync::OnceLock<Arc<ProjectTerminalRuntime>> =
    std::sync::OnceLock::new();

/// Stores only currently visible project terminal display subscriptions.
/// 仅保存当前可见项目终端显示订阅。
static PROJECT_TERMINAL_DISPLAY_SUBSCRIPTIONS: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<String, ProjectTerminalDisplaySubscription>>,
> = std::sync::OnceLock::new();

/// Request body for creating one terminal under an explicit project.
/// 在一个显式项目下创建终端的请求体。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ProjectTerminalCreateRequest {
    /// Stable owning project identity.
    /// 稳定归属项目标识。
    project_id: String,
    /// Optional user-facing terminal name.
    /// 可选用户可见终端名称。
    name: Option<String>,
    /// Selected shell id or `auto`.
    /// 已选择 Shell 标识或 `auto`。
    shell_id: String,
    /// Initial xterm row count.
    /// 初始 xterm 行数。
    rows: u16,
    /// Initial xterm column count.
    /// 初始 xterm 列数。
    cols: u16,
}

/// Request body for sending one ordered terminal input batch.
/// 发送一批有序终端输入的请求体。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ProjectTerminalInputRequest {
    /// Stable owning project identity.
    /// 稳定归属项目标识。
    project_id: String,
    /// Stable target terminal identity.
    /// 稳定目标终端标识。
    terminal_id: String,
    /// First contiguous browser input sequence in `events`.
    /// `events` 中首个连续浏览器输入序号。
    first_client_seq: u64,
    /// Ordered UTF-8 or binary xterm input events.
    /// 有序的 UTF-8 或二进制 xterm 输入事件。
    events: Vec<ProjectTerminalInputEvent>,
}

/// Carries exactly one UTF-8 or binary xterm input event.
/// 承载恰好一个 UTF-8 或二进制 xterm 输入事件。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectTerminalInputEvent {
    /// Unicode/control text produced by xterm onData.
    /// xterm onData 产生的 Unicode 或控制文本。
    utf8_data: Option<String>,
    /// Standard-base64 bytes produced by xterm onBinary.
    /// xterm onBinary 产生的标准 Base64 字节。
    binary_data_base64: Option<String>,
}

/// Request body for resizing one project terminal.
/// 调整一个项目终端尺寸的请求体。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ProjectTerminalResizeRequest {
    /// Stable owning project identity.
    /// 稳定归属项目标识。
    project_id: String,
    /// Stable target terminal identity.
    /// 稳定目标终端标识。
    terminal_id: String,
    /// Positive terminal row count.
    /// 大于零的终端行数。
    rows: u16,
    /// Positive terminal column count.
    /// 大于零的终端列数。
    cols: u16,
}

/// Request body for renaming one project terminal.
/// 重命名单个项目终端的请求体。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ProjectTerminalRenameRequest {
    /// Stable owning project identity.
    /// 稳定归属项目标识。
    project_id: String,
    /// Stable target terminal identity.
    /// 稳定目标终端标识。
    terminal_id: String,
    /// Requested user-facing terminal name.
    /// 请求的用户可见终端名称。
    name: String,
}

/// Returns the process-local project terminal runtime.
/// 返回进程内项目终端运行时。
///
/// # Returns / 返回值
/// Returns the runtime shared across all conversation switches.
/// 返回由所有对话切换共享的运行时。
pub(crate) fn project_terminal_runtime() -> &'static Arc<ProjectTerminalRuntime> {
    PROJECT_TERMINAL_RUNTIME.get_or_init(|| Arc::new(ProjectTerminalRuntime::new()))
}

/// Counts currently running project terminals for the protected status endpoint.
/// 为受保护状态端点统计当前运行中的项目终端。
///
/// # Returns / 返回值
/// Best-effort running-terminal gauge; a poisoned registry reports zero.
/// 尽力而为的运行中终端度量；注册表损坏时报告零。
pub(crate) fn project_terminal_running_gauge() -> u64 {
    project_terminal_runtime()
        .running_terminal_count()
        .unwrap_or(0)
}

/// Returns the registry containing only visible terminal displays.
/// 返回仅包含可见终端显示层的注册表。
///
/// # Returns / 返回值
/// Returns the lazily initialized subscription registry.
/// 返回延迟初始化的订阅注册表。
fn project_terminal_display_subscriptions()
-> &'static std::sync::Mutex<std::collections::HashMap<String, ProjectTerminalDisplaySubscription>>
{
    PROJECT_TERMINAL_DISPLAY_SUBSCRIPTIONS
        .get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// Lists host shells and terminals for one exact active project.
/// 列出一个精确活动项目的宿主 Shell 与终端。
///
/// # Parameters / 参数
/// - `state`: Service state used to validate project and shell preference.
/// - `project_id`: Explicit active project identity.
/// - `state`：用于验证项目与 Shell 偏好的服务状态。
/// - `project_id`：显式活动项目标识。
///
/// # Returns / 返回值
/// Returns a project catalog independent from selected conversation state.
/// 返回独立于选中对话状态的项目终端目录。
pub(super) async fn service_list_project_terminals(
    state: SharedServiceBridgeState,
    project_id: String,
) -> Result<ProjectTerminalCatalog, String> {
    let project_id = require_project_id(project_id)?;
    service_project_path(&state, &project_id)?;
    let (_, agent_runtime, _, _, _) = runtime_parts_from_state(&state)?;
    build_project_terminal_catalog(&project_id, &agent_runtime).await
}

/// Creates one project-owned terminal using a strictly resolved host shell.
/// 使用严格解析的宿主 Shell 创建一个项目级终端。
///
/// # Parameters / 参数
/// - `state`: Service state used for project and shell validation.
/// - `request`: Explicit project, shell, name, and dimensions.
/// - `state`：用于项目及 Shell 验证的服务状态。
/// - `request`：显式项目、Shell、名称与尺寸。
///
/// # Returns / 返回值
/// Returns the refreshed catalog after host acknowledgement.
/// 宿主确认后返回刷新的目录。
pub(super) async fn service_create_project_terminal(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: ProjectTerminalCreateRequest,
) -> Result<ProjectTerminalCatalog, String> {
    let project_id = require_project_id(request.project_id)?;
    let cwd = service_project_path(&state, &project_id)?;
    let (_, agent_runtime, _, _, _) = runtime_parts_from_state(&state)?;
    let shell = project_terminal_shell_support(&agent_runtime)
        .resolve_requested_shell(Some(&request.shell_id))?;
    let existing = project_terminal_runtime()
        .list_project_terminals(&project_id)
        .await?;
    let name = request
        .name
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| format!("终端 {}", existing.len() + 1));
    let terminal = project_terminal_runtime()
        .start_terminal(ProjectTerminalStartRequest {
            project_id: project_id.clone(),
            name,
            shell_id: shell.shell.id().to_owned(),
            program: shell.launcher_program,
            args: Vec::new(),
            cwd,
            rows: request.rows,
            cols: request.cols,
        })
        .await?;
    spawn_project_terminal_running_count_monitor(
        app.clone(),
        project_id.clone(),
        terminal.terminal_id,
    );
    let catalog = build_project_terminal_catalog(&project_id, &agent_runtime).await?;
    emit_project_terminal_running_count(&app, &catalog);
    Ok(catalog)
}

/// Writes one sequenced xterm input batch to a project terminal.
/// 向一个项目终端写入一批带序号的 xterm 输入。
///
/// # Parameters / 参数
/// - `request`: Project ownership, terminal identity, and base64 bytes.
/// - `request`：项目归属、终端标识与 Base64 字节。
///
/// # Returns / 返回值
/// Returns the highest contiguous client input sequence accepted by the isolated host pipe.
/// 返回隔离宿主管道接受的最高连续客户端输入序号。
pub(super) async fn service_send_project_terminal_input(
    request: ProjectTerminalInputRequest,
) -> Result<u64, String> {
    let project_id = require_project_id(request.project_id)?;
    let terminal_id = parse_project_terminal_id(request.terminal_id)?;
    if request.events.is_empty() || request.events.len() > PROJECT_TERMINAL_INPUT_BATCH_MAX_EVENTS {
        return Err(format!(
            "Project terminal input batch must contain between 1 and {PROJECT_TERMINAL_INPUT_BATCH_MAX_EVENTS} events."
        ));
    }
    if request.first_client_seq == 0 {
        return Err(String::from(
            "Project terminal input sequence must start above zero.",
        ));
    }
    let event_count = request.events.len();
    let last_client_seq = request
        .first_client_seq
        .checked_add(event_count as u64 - 1)
        .ok_or_else(|| String::from("Project terminal input sequence range overflowed."))?;
    let bytes = decode_project_terminal_input_events(request.events)?;
    project_terminal_runtime()
        .write_terminal_input_batch(
            &project_id,
            terminal_id,
            bytes,
            request.first_client_seq,
            last_client_seq,
            event_count,
        )
        .await
}

/// Decodes one bounded ordered list of xterm input events without changing byte semantics.
/// 解码一组有界有序的 xterm 输入事件，且不改变字节语义。
///
/// # Parameters / 参数
/// - `events`: Ordered events containing exactly one non-empty UTF-8 or Base64 payload each.
/// - `events`：有序事件；每项恰好包含一个非空 UTF-8 或 Base64 负载。
///
/// # Returns / 返回值
/// Returns exact concatenated bytes, or an explicit contract error.
/// 返回精确拼接后的字节，或明确的契约错误。
fn decode_project_terminal_input_events(
    events: Vec<ProjectTerminalInputEvent>,
) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    for event in events {
        match (event.utf8_data, event.binary_data_base64) {
            (Some(text), None) if !text.is_empty() => bytes.extend_from_slice(text.as_bytes()),
            (None, Some(binary)) => {
                let decoded = base64::engine::general_purpose::STANDARD
                    .decode(binary)
                    .map_err(|error| {
                        format!("Invalid project terminal binary input base64: {error}")
                    })?;
                if decoded.is_empty() {
                    return Err(String::from(
                        "Project terminal binary input event must not be empty.",
                    ));
                }
                bytes.extend_from_slice(&decoded);
            }
            _ => {
                return Err(String::from(
                    "Each project terminal input event must contain exactly one non-empty UTF-8 or binary payload.",
                ));
            }
        }
        if bytes.len() > PROJECT_TERMINAL_INPUT_BATCH_MAX_BYTES {
            return Err(format!(
                "Project terminal input batch exceeds {PROJECT_TERMINAL_INPUT_BATCH_MAX_BYTES} bytes."
            ));
        }
    }
    Ok(bytes)
}

/// Resizes one project terminal through its isolated host.
/// 通过隔离宿主调整一个项目终端尺寸。
///
/// # Parameters / 参数
/// - `request`: Project ownership, terminal identity, rows, and columns.
/// - `request`：项目归属、终端标识、行数与列数。
///
/// # Returns / 返回值
/// Returns success after the host accepts the resize request.
/// 宿主接受尺寸调整请求后返回成功。
pub(super) async fn service_resize_project_terminal(
    request: ProjectTerminalResizeRequest,
) -> Result<(), String> {
    let project_id = require_project_id(request.project_id)?;
    let terminal_id = parse_project_terminal_id(request.terminal_id)?;
    project_terminal_runtime()
        .resize_terminal(&project_id, terminal_id, request.rows, request.cols)
        .await
}

/// Renames one exact project terminal and returns its refreshed catalog.
/// 重命名一个精确项目终端并返回刷新后的目录。
///
/// # Parameters / 参数
/// - `state`: Service state containing authoritative project ownership.
/// - `request`: Exact project, terminal, and new name.
/// - `state`：包含权威项目归属的服务状态。
/// - `request`：精确的项目、终端与新名称。
///
/// # Returns / 返回值
/// Returns the refreshed project terminal catalog.
/// 返回刷新后的项目终端目录。
pub(super) async fn service_rename_project_terminal(
    state: SharedServiceBridgeState,
    request: ProjectTerminalRenameRequest,
) -> Result<ProjectTerminalCatalog, String> {
    let project_id = require_project_id(request.project_id)?;
    service_project_path(&state, &project_id)?;
    let terminal_id = parse_project_terminal_id(request.terminal_id)?;
    project_terminal_runtime()
        .rename_terminal(&project_id, terminal_id, request.name)
        .await?;
    let (_, agent_runtime, _, _, _) = runtime_parts_from_state(&state)?;
    build_project_terminal_catalog(&project_id, &agent_runtime).await
}

/// Stops one project terminal while retaining its tab and output.
/// 停止一个项目终端，同时保留其标签与输出。
///
/// # Parameters / 参数
/// - `project_id`: Exact owning project identity.
/// - `terminal_id`: Exact target terminal identity.
/// - `project_id`：精确归属项目标识。
/// - `terminal_id`：精确目标终端标识。
///
/// # Returns / 返回值
/// Returns success after the host accepts the stop request.
/// 宿主接受停止请求后返回成功。
pub(super) async fn service_stop_project_terminal(
    project_id: String,
    terminal_id: String,
) -> Result<(), String> {
    let project_id = require_project_id(project_id)?;
    let terminal_id = parse_project_terminal_id(terminal_id)?;
    project_terminal_runtime()
        .stop_terminal(&project_id, terminal_id)
        .await
}

/// Closes one project terminal and cancels its attached display.
/// 关闭一个项目终端并取消其已挂接显示层。
///
/// # Parameters / 参数
/// - `state`: Service state used to rebuild the catalog.
/// - `project_id`: Exact owning project identity.
/// - `terminal_id`: Exact target terminal identity.
/// - `state`：用于重建目录的服务状态。
/// - `project_id`：精确归属项目标识。
/// - `terminal_id`：精确目标终端标识。
///
/// # Returns / 返回值
/// Returns the refreshed project terminal catalog.
/// 返回刷新的项目终端目录。
pub(super) async fn service_close_project_terminal(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    project_id: String,
    terminal_id: String,
) -> Result<ProjectTerminalCatalog, String> {
    let project_id = require_project_id(project_id)?;
    let terminal_id = parse_project_terminal_id(terminal_id)?;
    project_terminal_runtime()
        .close_terminal(&project_id, terminal_id)
        .await?;
    cancel_project_terminal_displays(&project_id, terminal_id)?;
    let (_, agent_runtime, _, _, _) = runtime_parts_from_state(&state)?;
    let catalog = build_project_terminal_catalog(&project_id, &agent_runtime).await?;
    emit_project_terminal_running_count(&app, &catalog);
    Ok(catalog)
}

/// Attaches one visible display for one-time query plus active pushes.
/// 挂接一个可见显示层，执行一次查询后转为主动推送。
///
/// # Parameters / 参数
/// - `app`: Event emitter used only while the display is visible.
/// - `project_id`: Exact owning project identity.
/// - `terminal_id`: Exact displayed terminal identity.
/// - `display_client_id`: Browser/WebView lifetime shared with the event socket.
/// - `app`：仅在显示层可见时使用的事件发送器。
/// - `project_id`：精确归属项目标识。
/// - `terminal_id`：精确显示终端标识。
/// - `display_client_id`：与事件套接字共享的浏览器或 WebView 生命周期。
///
/// # Returns / 返回值
/// Returns subscription id, terminal summary, and retained output tail.
/// 返回订阅标识、终端摘要与保留输出尾部。
pub(super) async fn service_attach_project_terminal_display(
    app: ServiceEventEmitter,
    project_id: String,
    terminal_id: String,
    display_client_id: String,
) -> Result<ProjectTerminalDisplayAttachment, String> {
    let project_id = require_project_id(project_id)?;
    let terminal_id = parse_project_terminal_id(terminal_id)?;
    let display_client_id = display_client_id.parse::<Uuid>().map_err(|error| {
        format!("Invalid project-terminal display client id `{display_client_id}`: {error}")
    })?;
    let subscription_id = Uuid::new_v4().to_string();
    let (cancel, mut cancelled) = tokio::sync::oneshot::channel();
    project_terminal_display_subscriptions()
        .lock()
        .map_err(|_| String::from("Project terminal display subscription registry is poisoned."))?
        .insert(
            subscription_id.clone(),
            ProjectTerminalDisplaySubscription {
                client_id: display_client_id,
                project_id: project_id.clone(),
                terminal_id,
                cancel,
            },
        );
    let initial = match project_terminal_runtime()
        .read_recent_for_display(
            &project_id,
            terminal_id,
            PROJECT_TERMINAL_DISPLAY_BOOTSTRAP_MAX_CHUNKS,
        )
        .await
    {
        Ok(initial) => initial,
        Err(error) => {
            let _ = service_detach_project_terminal_display(subscription_id.clone());
            return Err(error);
        }
    };
    let initial_summary = project_terminal_summary(initial.snapshot.clone());
    let initial_chunks = project_terminal_display_chunks(initial.chunks);
    let task_subscription_id = subscription_id.clone();
    let task_display_client_id = display_client_id;
    tokio::spawn(async move {
        let mut after_seq = initial.snapshot.last_seq;
        let mut running_was_observed = initial.snapshot.status == ProjectTerminalStatus::Running;
        let mut observed_state_revision = initial.snapshot.state_revision;
        loop {
            let changed = tokio::select! {
                _ = &mut cancelled => break,
                changed = project_terminal_runtime().wait_for_display_change(
                    &project_id,
                    terminal_id,
                    after_seq,
                    running_was_observed,
                    observed_state_revision,
                    PROJECT_TERMINAL_DISPLAY_EVENT_MAX_CHUNKS,
                ) => changed,
            };
            let outcome = match changed {
                Ok(outcome) => outcome,
                Err(_) => break,
            };
            let last_delivered_seq = outcome
                .chunks
                .last()
                .map(|chunk| chunk.seq)
                .unwrap_or(after_seq);
            running_was_observed = outcome.snapshot.status == ProjectTerminalStatus::Running;
            observed_state_revision = outcome.snapshot.state_revision;
            let emitted = emit_project_terminal_display_change_if_attached(
                &app,
                &task_subscription_id,
                task_display_client_id,
                &project_id,
                terminal_id,
                ProjectTerminalDisplayChangedEvent {
                    display_client_id: task_display_client_id.to_string(),
                    subscription_id: task_subscription_id.clone(),
                    terminal: project_terminal_summary(outcome.snapshot),
                    chunks: project_terminal_display_chunks(outcome.chunks),
                },
            )
            .unwrap_or(false);
            if !emitted {
                break;
            }
            after_seq = last_delivered_seq;
            if !running_was_observed && !outcome.has_more {
                break;
            }
        }
        if let Ok(mut subscriptions) = project_terminal_display_subscriptions().lock() {
            subscriptions.remove(&task_subscription_id);
        }
    });
    Ok(ProjectTerminalDisplayAttachment {
        subscription_id,
        terminal: initial_summary,
        chunks: initial_chunks,
    })
}

/// Detaches one hidden display and stops future content delivery.
/// 解除一个已隐藏显示层的挂接并停止后续内容交付。
///
/// # Parameters / 参数
/// - `subscription_id`: Opaque id returned by attach.
/// - `subscription_id`：挂接调用返回的不透明标识。
///
/// # Returns / 返回值
/// Returns true only when an active subscription was cancelled.
/// 仅在成功取消活动订阅时返回 true。
pub(super) fn service_detach_project_terminal_display(
    subscription_id: String,
) -> Result<bool, String> {
    let subscription = project_terminal_display_subscriptions()
        .lock()
        .map_err(|_| String::from("Project terminal display subscription registry is poisoned."))?
        .remove(&subscription_id);
    let Some(subscription) = subscription else {
        return Ok(false);
    };
    let _ = subscription.cancel.send(());
    Ok(true)
}

/// Emits one display change only while the exact subscription remains registered.
/// 仅在精确订阅仍登记时发出一次显示变化。
///
/// The registry lock serializes event emission with detach, so no content can be emitted after
/// a successful detach call returns.
/// 注册表锁将事件发出与解除挂接串行化，因此成功解除挂接返回后不会再发出内容。
///
/// # Parameters / 参数
/// - `app`: Service event emitter for the attached browser.
/// - `subscription_id`: Exact opaque display subscription identity.
/// - `display_client_id`: Exact browser/WebView lifetime identity.
/// - `project_id`: Exact owning project identity.
/// - `terminal_id`: Exact displayed terminal identity.
/// - `event`: Fully materialized display change.
/// - `app`：已挂接浏览器的服务事件发送器。
/// - `subscription_id`：精确的不透明显示订阅标识。
/// - `display_client_id`：精确浏览器或 WebView 生命周期标识。
/// - `project_id`：精确归属项目标识。
/// - `terminal_id`：精确显示终端标识。
/// - `event`：已完整构建的显示变化。
///
/// # Returns / 返回值
/// Returns true only when the matching active subscription received the event.
/// 仅当匹配的活动订阅收到事件时返回 true。
fn emit_project_terminal_display_change_if_attached(
    app: &ServiceEventEmitter,
    subscription_id: &str,
    display_client_id: Uuid,
    project_id: &str,
    terminal_id: Uuid,
    event: ProjectTerminalDisplayChangedEvent,
) -> Result<bool, String> {
    let subscriptions = project_terminal_display_subscriptions()
        .lock()
        .map_err(|_| String::from("Project terminal display subscription registry is poisoned."))?;
    let attached = subscriptions
        .get(subscription_id)
        .is_some_and(|subscription| {
            subscription.client_id == display_client_id
                && subscription.project_id == project_id
                && subscription.terminal_id == terminal_id
        });
    if !attached {
        return Ok(false);
    }
    app.emit(ServiceEvent::ProjectTerminalDisplayChanged(Box::new(event)));
    Ok(true)
}

/// Builds one catalog from runtime state and configured shell preference.
/// 根据运行时状态与已配置 Shell 偏好构建目录。
///
/// # Parameters / 参数
/// - `project_id`: Exact project identity.
/// - `agent_runtime`: Bridge containing the configured default shell.
/// - `project_id`：精确项目标识。
/// - `agent_runtime`：包含已配置默认 Shell 的桥。
///
/// # Returns / 返回值
/// Returns available shell options and project terminals.
/// 返回可用 Shell 选项与项目终端。
async fn build_project_terminal_catalog(
    project_id: &str,
    agent_runtime: &AgentRuntimeBridge,
) -> Result<ProjectTerminalCatalog, String> {
    let shell_support = project_terminal_shell_support(agent_runtime);
    let default_shell_id = shell_support.auto_shell().map(|shell| shell.shell.id());
    let shells = shell_support
        .supported_shells()
        .iter()
        .map(|shell| ProjectTerminalShellOption {
            id: shell.shell.id().to_owned(),
            label: shell.shell.display_label().to_owned(),
            default: default_shell_id == Some(shell.shell.id()),
        })
        .collect();
    let terminals = project_terminal_runtime()
        .list_project_terminals(project_id)
        .await?
        .into_iter()
        .map(project_terminal_summary)
        .collect();
    let running_count_revision =
        project_terminal_runtime().project_running_count_revision(project_id)?;
    Ok(ProjectTerminalCatalog {
        project_id: project_id.to_owned(),
        shells,
        terminals,
        running_count_revision,
    })
}

/// Starts one lightweight lifecycle observer for a newly created running terminal.
/// 为一个新建的运行中终端启动轻量生命周期观察器。
///
/// # Parameters / 参数
/// - `app`: Service event emitter receiving count-only changes.
/// - `project_id`: Stable owning project identity.
/// - `terminal_id`: Stable terminal identity.
/// - `app`：接收纯数量变化的服务事件发送器。
/// - `project_id`：稳定归属项目标识。
/// - `terminal_id`：稳定终端标识。
fn spawn_project_terminal_running_count_monitor(
    app: ServiceEventEmitter,
    project_id: String,
    terminal_id: Uuid,
) {
    tokio::spawn(async move {
        if project_terminal_runtime()
            .wait_for_lifecycle_change(&project_id, terminal_id, ProjectTerminalStatus::Running)
            .await
            .is_err()
        {
            return;
        }
        let Ok(terminals) = project_terminal_runtime()
            .list_project_terminals(&project_id)
            .await
        else {
            return;
        };
        let Ok(revision) = project_terminal_runtime().project_running_count_revision(&project_id)
        else {
            return;
        };
        emit_project_terminal_running_count_values(&app, project_id, terminals, revision);
    });
}

/// Emits the running count represented by one authoritative project catalog.
/// 发出一份权威项目目录所表示的运行数量。
///
/// # Parameters / 参数
/// - `app`: Service event emitter receiving the lightweight event.
/// - `catalog`: Authoritative catalog used for count and revision.
/// - `app`：接收轻量事件的服务事件发送器。
/// - `catalog`：用于数量与修订号的权威目录。
fn emit_project_terminal_running_count(
    app: &ServiceEventEmitter,
    catalog: &ProjectTerminalCatalog,
) {
    let running_count = catalog
        .terminals
        .iter()
        .filter(|terminal| terminal.status == ProjectTerminalStatus::Running.as_str())
        .count();
    app.emit(ServiceEvent::ProjectTerminalRunningCountChanged(Box::new(
        ProjectTerminalRunningCountChangedEvent {
            project_id: catalog.project_id.clone(),
            running_count,
            revision: catalog.running_count_revision,
        },
    )));
}

/// Emits a count-only lifecycle event from runtime snapshots.
/// 从运行时快照发出仅包含数量的生命周期事件。
///
/// # Parameters / 参数
/// - `app`: Service event emitter receiving the lightweight event.
/// - `project_id`: Stable owning project identity.
/// - `terminals`: Current runtime snapshots for the project.
/// - `revision`: Monotonic running-count revision.
/// - `app`：接收轻量事件的服务事件发送器。
/// - `project_id`：稳定归属项目标识。
/// - `terminals`：项目当前运行时快照。
/// - `revision`：单调运行数量修订号。
fn emit_project_terminal_running_count_values(
    app: &ServiceEventEmitter,
    project_id: String,
    terminals: Vec<ProjectTerminalSnapshot>,
    revision: u64,
) {
    let running_count = terminals
        .iter()
        .filter(|terminal| terminal.status == ProjectTerminalStatus::Running)
        .count();
    app.emit(ServiceEvent::ProjectTerminalRunningCountChanged(Box::new(
        ProjectTerminalRunningCountChangedEvent {
            project_id,
            running_count,
            revision,
        },
    )));
}

/// Uses the same configured default shell as native AI tools.
/// 使用与 AI 原生工具相同的已配置默认 Shell。
///
/// # Parameters / 参数
/// - `agent_runtime`: Runtime bridge containing provider configuration.
/// - `agent_runtime`：包含 Provider 配置的运行时桥。
///
/// # Returns / 返回值
/// Returns host-detected shell support.
/// 返回宿主检测到的 Shell 支持。
fn project_terminal_shell_support(agent_runtime: &AgentRuntimeBridge) -> HostCommandShellSupport {
    HostCommandShellSupport::from_configured_default_id(
        agent_runtime
            .provider_configuration
            .default_command_shell
            .as_deref(),
    )
}

/// Resolves the exact persisted project path without conversation membership.
/// 解析精确持久化项目路径，不使用对话成员关系。
///
/// # Parameters / 参数
/// - `state`: Service state containing authoritative project summaries.
/// - `project_id`: Exact project identity.
/// - `state`：包含权威项目摘要的服务状态。
/// - `project_id`：精确项目标识。
///
/// # Returns / 返回值
/// Returns the matching project path or an explicit not-found error.
/// 返回匹配项目路径或明确的未找到错误。
fn service_project_path(
    state: &SharedServiceBridgeState,
    project_id: &str,
) -> Result<PathBuf, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let project = guard
        .workbench
        .projects
        .iter()
        .find(|project| project.id == project_id)
        .ok_or_else(|| format!("Project `{project_id}` was not found in the active workbench."))?;
    Ok(PathBuf::from(&project.path))
}

/// Validates and trims one explicit project identity.
/// 验证并去除一个显式项目标识的首尾空白。
///
/// # Parameters / 参数
/// - `project_id`: Raw request project identity.
/// - `project_id`：请求中的原始项目标识。
///
/// # Returns / 返回值
/// Returns a non-empty project identity.
/// 返回非空项目标识。
fn require_project_id(project_id: String) -> Result<String, String> {
    let project_id = project_id.trim().to_owned();
    if project_id.is_empty() {
        return Err(String::from(
            "Project terminal project id must not be empty.",
        ));
    }
    Ok(project_id)
}

/// Parses one stable project terminal UUID.
/// 解析一个稳定项目终端 UUID。
///
/// # Parameters / 参数
/// - `terminal_id`: Raw terminal identity.
/// - `terminal_id`：原始终端标识。
///
/// # Returns / 返回值
/// Returns the UUID or a labelled parse error.
/// 返回 UUID 或带标签解析错误。
fn parse_project_terminal_id(terminal_id: String) -> Result<Uuid, String> {
    terminal_id
        .parse::<Uuid>()
        .map_err(|error| format!("Invalid project terminal id `{terminal_id}`: {error}"))
}

/// Converts a runtime snapshot into the browser-facing summary.
/// 将运行时快照转换为浏览器摘要。
///
/// # Parameters / 参数
/// - `snapshot`: Immutable project terminal state.
/// - `snapshot`：不可变项目终端状态。
///
/// # Returns / 返回值
/// Returns a stable service API summary.
/// 返回稳定服务 API 摘要。
fn project_terminal_summary(snapshot: ProjectTerminalSnapshot) -> ProjectTerminalSummary {
    ProjectTerminalSummary {
        id: snapshot.terminal_id.to_string(),
        project_id: snapshot.project_id,
        name: snapshot.name,
        shell_id: snapshot.shell_id,
        cwd: snapshot.cwd.to_string_lossy().into_owned(),
        status: snapshot.status.as_str().to_owned(),
        exit_code: snapshot.exit_code,
        failure_reason: snapshot.failure_reason,
        last_seq: snapshot.last_seq,
        dropped_before_seq: snapshot.dropped_before_seq,
        state_revision: snapshot.state_revision,
        last_accepted_input_seq: snapshot.last_accepted_input_seq,
        rows: snapshot.rows,
        cols: snapshot.cols,
    }
}

/// Converts project output into the shared terminal display contract.
/// 将项目输出转换为共享终端显示契约。
///
/// # Parameters / 参数
/// - `chunks`: Ordered runtime output fragments.
/// - `chunks`：有序运行时输出片段。
///
/// # Returns / 返回值
/// Returns ordered display chunks.
/// 返回有序显示分片。
fn project_terminal_display_chunks(
    chunks: Vec<ProjectTerminalOutputChunk>,
) -> Vec<TerminalDisplayChunk> {
    chunks
        .into_iter()
        .map(|chunk| TerminalDisplayChunk {
            seq: chunk.seq,
            stream: chunk.stream.to_owned(),
            text: chunk.text,
        })
        .collect()
}

/// Cancels all visible displays attached to one closing terminal.
/// 取消挂接到一个正在关闭终端的全部可见显示层。
///
/// # Parameters / 参数
/// - `project_id`: Exact owning project identity.
/// - `terminal_id`: Exact closing terminal identity.
/// - `project_id`：精确归属项目标识。
/// - `terminal_id`：精确正在关闭的终端标识。
///
/// # Returns / 返回值
/// Returns success after matching subscriptions are cancelled.
/// 取消匹配订阅后返回成功。
pub(crate) fn cancel_project_terminal_displays(
    project_id: &str,
    terminal_id: Uuid,
) -> Result<(), String> {
    let mut subscriptions = project_terminal_display_subscriptions()
        .lock()
        .map_err(|_| String::from("Project terminal display subscription registry is poisoned."))?;
    let matching_ids = subscriptions
        .iter()
        .filter(|(_, subscription)| {
            subscription.project_id == project_id && subscription.terminal_id == terminal_id
        })
        .map(|(subscription_id, _)| subscription_id.clone())
        .collect::<Vec<_>>();
    for subscription_id in matching_ids {
        if let Some(subscription) = subscriptions.remove(&subscription_id) {
            let _ = subscription.cancel.send(());
        }
    }
    Ok(())
}

/// Cancels every visible project-terminal display during application shutdown.
/// 在应用关闭期间取消全部可见项目终端显示订阅。
///
/// # Returns / 返回值
/// Returns the number of active display subscriptions cancelled.
/// 返回已取消的活动显示订阅数量。
pub(crate) fn cancel_all_project_terminal_displays() -> Result<usize, String> {
    let mut subscriptions = project_terminal_display_subscriptions()
        .lock()
        .map_err(|_| String::from("Project-terminal display subscription registry is poisoned."))?;
    let drained = subscriptions
        .drain()
        .map(|(_, subscription)| subscription)
        .collect::<Vec<_>>();
    let count = drained.len();
    drop(subscriptions);
    for subscription in drained {
        let _ = subscription.cancel.send(());
    }
    Ok(count)
}

/// Cancels every visible project terminal display owned by one disconnected browser client.
/// 取消由一个已断开浏览器客户端持有的全部可见项目终端显示层。
///
/// # Parameters / 参数
/// - `client_id`: Exact browser/WebView lifetime reported by the event socket.
/// - `client_id`：事件套接字报告的精确浏览器或 WebView 生命周期。
pub(crate) fn cancel_project_terminal_displays_for_client(client_id: Uuid) {
    let Ok(mut subscriptions) = project_terminal_display_subscriptions().lock() else {
        return;
    };
    let matching_ids = subscriptions
        .iter()
        .filter(|(_, subscription)| subscription.client_id == client_id)
        .map(|(subscription_id, _)| subscription_id.clone())
        .collect::<Vec<_>>();
    for subscription_id in matching_ids {
        if let Some(subscription) = subscriptions.remove(&subscription_id) {
            let _ = subscription.cancel.send(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        PROJECT_TERMINAL_INPUT_BATCH_MAX_BYTES, ProjectTerminalInputEvent,
        decode_project_terminal_input_events, parse_project_terminal_id, require_project_id,
    };
    use base64::Engine as _;

    /// Verifies blank project identities are rejected before runtime access.
    /// 验证空白项目标识在访问运行时前被拒绝。
    #[test]
    fn blank_project_id_is_rejected() {
        assert!(require_project_id(String::from("   ")).is_err());
    }

    /// Verifies malformed terminal ids retain their request label.
    /// 验证格式错误的终端标识保留请求标签。
    #[test]
    fn malformed_terminal_id_is_rejected() {
        let error = parse_project_terminal_id(String::from("invalid"))
            .expect_err("malformed terminal id must fail");
        assert!(error.starts_with("Invalid project terminal id"));
    }

    /// Verifies UTF-8 and binary xterm events preserve exact ordered bytes.
    /// 验证 UTF-8 与二进制 xterm 事件保留精确的有序字节。
    #[test]
    fn input_events_preserve_unicode_and_binary_bytes() {
        let binary = vec![0_u8, 13, 255];
        let decoded = decode_project_terminal_input_events(vec![
            ProjectTerminalInputEvent {
                utf8_data: Some(String::from("中🙂")),
                binary_data_base64: None,
            },
            ProjectTerminalInputEvent {
                utf8_data: None,
                binary_data_base64: Some(base64::engine::general_purpose::STANDARD.encode(&binary)),
            },
        ])
        .expect("valid mixed xterm events must decode");
        let mut expected = String::from("中🙂").into_bytes();
        expected.extend_from_slice(&binary);
        assert_eq!(decoded, expected);
    }

    /// Verifies each event contains exactly one non-empty representation.
    /// 验证每个事件恰好包含一种非空表示。
    #[test]
    fn input_events_reject_ambiguous_or_empty_payloads() {
        assert!(
            decode_project_terminal_input_events(vec![ProjectTerminalInputEvent {
                utf8_data: Some(String::from("x")),
                binary_data_base64: Some(String::from("eA==")),
            }])
            .is_err()
        );
        assert!(
            decode_project_terminal_input_events(vec![ProjectTerminalInputEvent {
                utf8_data: Some(String::new()),
                binary_data_base64: None,
            }])
            .is_err()
        );
        assert!(
            decode_project_terminal_input_events(vec![ProjectTerminalInputEvent {
                utf8_data: None,
                binary_data_base64: Some(String::from("not-base64")),
            }])
            .is_err()
        );
    }

    /// Verifies the service enforces the exact acknowledged input byte ceiling.
    /// 验证服务严格执行已确认输入字节上限。
    #[test]
    fn input_events_enforce_byte_limit() {
        let accepted = decode_project_terminal_input_events(vec![ProjectTerminalInputEvent {
            utf8_data: Some("x".repeat(PROJECT_TERMINAL_INPUT_BATCH_MAX_BYTES)),
            binary_data_base64: None,
        }])
        .expect("the exact input byte limit must be accepted");
        assert_eq!(accepted.len(), PROJECT_TERMINAL_INPUT_BATCH_MAX_BYTES);
        assert!(
            decode_project_terminal_input_events(vec![ProjectTerminalInputEvent {
                utf8_data: Some("x".repeat(PROJECT_TERMINAL_INPUT_BATCH_MAX_BYTES + 1)),
                binary_data_base64: None,
            }])
            .is_err()
        );
    }
}
