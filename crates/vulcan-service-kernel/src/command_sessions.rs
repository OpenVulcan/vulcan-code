//! Command session service handlers.
//! 命令会话服务处理器。

use super::*;

/// Maximum retained output fragments returned by one visible display attachment.
/// 单个可见显示层挂接时返回的最大保留输出分片数。
const COMMAND_SESSION_DISPLAY_BOOTSTRAP_MAX_CHUNKS: usize = 4096;

/// Maximum changed output fragments carried by one pushed display event.
/// 单个主动推送显示事件承载的最大变化输出分片数。
const COMMAND_SESSION_DISPLAY_EVENT_MAX_CHUNKS: usize = 512;

/// Stores ownership and cancellation for one visible command-session display.
/// 保存一个可见命令会话显示层的所有权与取消句柄。
struct CommandSessionDisplaySubscription {
    /// Browser or WebView lifetime that owns this display.
    /// 持有该显示层的浏览器或 WebView 生命周期。
    client_id: Uuid,
    /// Cancels runtime-to-WebUI content delivery.
    /// 取消运行时到 WebUI 的内容交付。
    cancel: tokio::sync::oneshot::Sender<()>,
}

/// Stores subscriptions only for currently visible command-session displays.
/// 仅保存当前可见命令会话显示层的订阅。
static COMMAND_SESSION_DISPLAY_SUBSCRIPTIONS: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<String, CommandSessionDisplaySubscription>>,
> = std::sync::OnceLock::new();

/// Returns the process-local visible command-session display subscription registry.
/// 返回进程内可见命令会话显示订阅注册表。
///
/// # Returns / 返回值
/// Returns the lazily initialized subscription registry.
/// 返回延迟初始化的订阅注册表。
fn command_session_display_subscriptions()
-> &'static std::sync::Mutex<std::collections::HashMap<String, CommandSessionDisplaySubscription>> {
    COMMAND_SESSION_DISPLAY_SUBSCRIPTIONS
        .get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// Runtime and identity context shared by command-session mutating actions.
/// 命令会话变更类动作共享的运行时与身份上下文。
struct CommandSessionActionContext {
    /// Controller runtime used to project command-session summaries.
    /// 用于投影命令会话摘要的控制器运行时。
    controller_runtime: ControllerRuntimeHandle,
    /// Agent runtime that owns the live command-session runtime.
    /// 持有实时命令会话运行时的 Agent runtime。
    agent_runtime: AgentRuntimeBridge,
    /// Parsed target command-session identifier.
    /// 已解析的目标命令会话标识。
    command_session_id: SessionId,
    /// Parsed owner conversation/session identifier.
    /// 已解析的归属会话标识。
    owner_session_id: SessionId,
}

pub(super) async fn service_stop_command_session(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    command_session_id: String,
    owner_session_id: Option<String>,
) -> Result<Vec<CommandSessionSummary>, String> {
    let context =
        load_service_command_session_action_context(&state, command_session_id, owner_session_id)?;
    webui_trace(
        "stop_command_session_requested",
        format!(
            "owner={} command_session={}",
            context.owner_session_id, context.command_session_id
        ),
    );
    context
        .agent_runtime
        .command_session_runtime
        .stop_session(
            context.owner_session_id,
            context.command_session_id,
            Some(1500),
        )
        .await?;
    emit_command_sessions_changed_delta(
        &app,
        &state,
        &context.controller_runtime,
        &context.agent_runtime,
    )
}

/// Reads one live command-session snapshot without rebuilding the full WebUI workbench.
/// 读取单条实时命令会话快照，但不重建完整 WebUI 工作台。
pub(super) async fn service_get_command_session_snapshot(
    state: SharedServiceBridgeState,
    command_session_id: String,
    owner_session_id: Option<String>,
) -> Result<CommandSessionSummary, String> {
    let (_, agent_runtime, _, _, _) = runtime_parts_from_state(&state)?;
    let command_session_id = parse_service_command_session_id(command_session_id)?;
    let snapshot = agent_runtime
        .command_session_runtime
        .try_snapshot_session_by_id(command_session_id)
        .ok_or_else(|| {
            format!("Command session `{command_session_id}` was not found in the live runtime.")
        })?;
    if let Some(owner_session_id) = owner_session_id {
        let owner_session_id = parse_service_owner_session_id(owner_session_id)?;
        if snapshot.owner_session_id != owner_session_id {
            return Err(format!(
                "Command session `{command_session_id}` does not belong to owner `{owner_session_id}`."
            ));
        }
    }
    Ok(command_session_summary_from_snapshot(snapshot))
}

/// Request body for sending structured input to one live command session.
/// 向单条实时命令会话发送结构化输入的请求体。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CommandSessionInputRequest {
    /// Target command session identifier.
    /// 目标命令会话标识。
    command_session_id: String,
    /// Optional owner conversation/session identifier.
    /// 可选归属会话标识。
    owner_session_id: Option<String>,
    /// Ordered terminal input events.
    /// 保序终端输入事件。
    inputs: Vec<CommandSessionInputEvent>,
}

/// Request body for resizing one live command session.
/// 调整单条实时命令会话尺寸的请求体。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CommandSessionResizeRequest {
    /// Target command session identifier.
    /// 目标命令会话标识。
    command_session_id: String,
    /// Optional owner conversation/session identifier.
    /// 可选归属会话标识。
    owner_session_id: Option<String>,
    /// Desired terminal row count.
    /// 期望终端行数。
    rows: u16,
    /// Desired terminal column count.
    /// 期望终端列数。
    cols: u16,
}

/// Resolves command-session ids and owner ids for service terminal actions.
/// 为服务终端动作解析命令会话标识与归属标识。
pub(super) fn resolve_service_command_session_ids(
    agent_runtime: &AgentRuntimeBridge,
    command_session_id: String,
    owner_session_id: Option<String>,
) -> Result<(SessionId, SessionId), String> {
    let command_session_id = parse_service_command_session_id(command_session_id)?;
    let owner_session_id = match owner_session_id {
        Some(owner_session_id) => parse_service_owner_session_id(owner_session_id)?,
        None => agent_runtime
            .command_session_runtime
            .try_snapshot_session_by_id(command_session_id)
            .map(|snapshot| snapshot.owner_session_id)
            .ok_or_else(|| {
                format!("Command session `{command_session_id}` was not found in the live runtime.")
            })?,
    };
    Ok((command_session_id, owner_session_id))
}

/// Attaches one visible read-only command-session terminal display.
/// 挂接一个可见的只读命令会话终端显示层。
///
/// The first response contains a bounded retained tail. Subsequent changes are pushed by runtime
/// notification and stop immediately after explicit detach or terminal completion.
/// 首次响应包含有界的保留输出尾部；后续变化由运行时通知主动推送，并在显式解除挂接或终端完成后立即停止。
///
/// # Parameters / 参数
/// - `app`: Service event emitter used for changed terminal output.
/// - `state`: Shared service runtime state.
/// - `command_session_id`: Raw command-session identity.
/// - `owner_session_id`: Optional owner identity used for strict validation.
/// - `display_client_id`: Browser/WebView lifetime shared with the event socket.
/// - `app`：用于发送终端变化输出的服务事件发送器。
/// - `state`：共享服务运行时状态。
/// - `command_session_id`：原始命令会话标识。
/// - `owner_session_id`：用于严格校验的可选归属标识。
/// - `display_client_id`：与事件套接字共享的浏览器或 WebView 生命周期。
///
/// # Returns / 返回值
/// Returns the opaque subscription id, current summary, and one-time retained output tail.
/// 返回不透明订阅标识、当前摘要以及一次性的保留输出尾部。
pub(super) async fn service_attach_command_session_display(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    command_session_id: String,
    owner_session_id: Option<String>,
    display_client_id: String,
) -> Result<CommandSessionDisplayAttachment, String> {
    let display_client_id = display_client_id.parse::<Uuid>().map_err(|error| {
        format!("Invalid command-session display client id `{display_client_id}`: {error}")
    })?;
    let (_, agent_runtime, _, _, _) = runtime_parts_from_state(&state)?;
    let (command_session_id, owner_session_id) =
        resolve_service_command_session_ids(&agent_runtime, command_session_id, owner_session_id)?;
    let subscription_id = Uuid::new_v4().to_string();
    let (cancel_sender, mut cancel_receiver) = tokio::sync::oneshot::channel();
    command_session_display_subscriptions()
        .lock()
        .map_err(|_| String::from("Command-session display subscription registry is poisoned."))?
        .insert(
            subscription_id.clone(),
            CommandSessionDisplaySubscription {
                client_id: display_client_id,
                cancel: cancel_sender,
            },
        );

    let initial = match agent_runtime
        .command_session_runtime
        .read_recent_session_for_display(
            owner_session_id,
            command_session_id,
            COMMAND_SESSION_DISPLAY_BOOTSTRAP_MAX_CHUNKS,
        )
        .await
    {
        Ok(initial) => initial,
        Err(error) => {
            let _ = service_detach_command_session_display(subscription_id.clone());
            return Err(error);
        }
    };

    let initial_terminal = command_session_summary_from_snapshot(initial.snapshot.clone());
    let initial_chunks = terminal_display_chunks(initial.returned_chunks);
    let runtime = Arc::clone(&agent_runtime.command_session_runtime);
    let task_subscription_id = subscription_id.clone();
    let task_display_client_id = display_client_id.to_string();
    tokio::spawn(async move {
        let mut display_cursor =
            vulcan_host_tools::command_session_runtime::CommandSessionDisplayCursor {
                after_seq: initial.snapshot.last_seq,
                running_was_observed: initial.snapshot.status
                    == vulcan_host_tools::command_session_runtime::CommandSessionStatus::Running,
                screen_frame_seq: initial.snapshot.screen_frame_seq,
                failure_reason: initial.snapshot.failure_reason.clone(),
            };
        loop {
            let change = tokio::select! {
                _ = &mut cancel_receiver => break,
                change = runtime.wait_for_session_display_change(
                    owner_session_id,
                    command_session_id,
                    &display_cursor,
                    COMMAND_SESSION_DISPLAY_EVENT_MAX_CHUNKS,
                ) => change,
            };
            let outcome = match change {
                Ok(outcome) => outcome,
                Err(_) => break,
            };
            let last_delivered_seq = outcome
                .returned_chunks
                .last()
                .map(|chunk| chunk.seq)
                .unwrap_or(display_cursor.after_seq);
            display_cursor =
                vulcan_host_tools::command_session_runtime::CommandSessionDisplayCursor {
                    after_seq: last_delivered_seq,
                    running_was_observed: outcome.snapshot.status
                        == vulcan_host_tools::command_session_runtime::CommandSessionStatus::Running,
                    screen_frame_seq: outcome.snapshot.screen_frame_seq,
                    failure_reason: outcome.snapshot.failure_reason.clone(),
                };
            let emitted = emit_command_session_display_change_if_attached(
                &app,
                &task_subscription_id,
                display_client_id,
                CommandSessionDisplayChangedEvent {
                    display_client_id: task_display_client_id.clone(),
                    subscription_id: task_subscription_id.clone(),
                    terminal: command_session_summary_from_snapshot(outcome.snapshot),
                    chunks: terminal_display_chunks(outcome.returned_chunks),
                },
            )
            .unwrap_or(false);
            if !emitted {
                break;
            }
            if !display_cursor.running_was_observed && !outcome.has_more {
                break;
            }
        }
        if let Ok(mut subscriptions) = command_session_display_subscriptions().lock() {
            subscriptions.remove(&task_subscription_id);
        }
    });

    Ok(CommandSessionDisplayAttachment {
        subscription_id,
        terminal: initial_terminal,
        chunks: initial_chunks,
    })
}

/// Detaches one hidden command-session display and stops all future content delivery.
/// 解除一个已隐藏命令会话显示层的挂接，并停止后续全部内容交付。
///
/// # Parameters / 参数
/// - `subscription_id`: Opaque id returned by the attach call.
/// - `subscription_id`：挂接调用返回的不透明标识。
///
/// # Returns / 返回值
/// Returns `true` only when an active visible subscription was cancelled.
/// 仅在成功取消一个活动可见订阅时返回 `true`。
pub(super) fn service_detach_command_session_display(
    subscription_id: String,
) -> Result<bool, String> {
    let subscription = command_session_display_subscriptions()
        .lock()
        .map_err(|_| String::from("Command-session display subscription registry is poisoned."))?
        .remove(&subscription_id);
    let Some(subscription) = subscription else {
        return Ok(false);
    };
    let _ = subscription.cancel.send(());
    Ok(true)
}

/// Emits one command-session display change only while its exact subscription is registered.
/// 仅在精确命令会话显示订阅仍登记时发出一次变化。
///
/// The registry lock serializes event emission with detach, which makes detach a strict boundary
/// after which the service cannot emit more terminal content for that subscription.
/// 注册表锁将事件发出与解除挂接串行化，使解除挂接成为严格边界；此后服务不能再为该订阅发出终端内容。
///
/// # Parameters / 参数
/// - `app`: Service event emitter for the attached browser.
/// - `subscription_id`: Exact opaque display subscription identity.
/// - `display_client_id`: Exact browser/WebView lifetime identity.
/// - `event`: Fully materialized read-only command-session display change.
/// - `app`：已挂接浏览器的服务事件发送器。
/// - `subscription_id`：精确的不透明显示订阅标识。
/// - `display_client_id`：精确浏览器或 WebView 生命周期标识。
/// - `event`：已完整构建的只读命令会话显示变化。
///
/// # Returns / 返回值
/// Returns true only when the matching active subscription received the event.
/// 仅当匹配的活动订阅收到事件时返回 true。
fn emit_command_session_display_change_if_attached(
    app: &ServiceEventEmitter,
    subscription_id: &str,
    display_client_id: Uuid,
    event: CommandSessionDisplayChangedEvent,
) -> Result<bool, String> {
    let subscriptions = command_session_display_subscriptions()
        .lock()
        .map_err(|_| String::from("Command-session display subscription registry is poisoned."))?;
    let attached = subscriptions
        .get(subscription_id)
        .is_some_and(|subscription| subscription.client_id == display_client_id);
    if !attached {
        return Ok(false);
    }
    app.emit(ServiceEvent::CommandSessionDisplayChanged(Box::new(event)));
    Ok(true)
}

/// Cancels every visible AI terminal display owned by one disconnected browser client.
/// 取消由一个已断开浏览器客户端持有的全部可见 AI 终端显示层。
///
/// # Parameters / 参数
/// - `client_id`: Exact browser/WebView lifetime reported by the event socket.
/// - `client_id`：事件套接字报告的精确浏览器或 WebView 生命周期。
pub(crate) fn cancel_command_session_displays_for_client(client_id: Uuid) {
    let Ok(mut subscriptions) = command_session_display_subscriptions().lock() else {
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

/// Converts runtime output chunks into the stable browser-facing display contract.
/// 将运行时输出分片转换为稳定的浏览器显示契约。
///
/// # Parameters / 参数
/// - `chunks`: Ordered runtime output fragments.
/// - `chunks`：有序的运行时输出片段。
///
/// # Returns / 返回值
/// Returns ordered service API display chunks with stable stream labels.
/// 返回带稳定输出流标签的有序服务 API 显示分片。
fn terminal_display_chunks(
    chunks: Vec<vulcan_host_tools::command_session_runtime::CommandSessionOutputChunk>,
) -> Vec<TerminalDisplayChunk> {
    chunks
        .into_iter()
        .map(|chunk| TerminalDisplayChunk {
            seq: chunk.seq,
            stream: chunk.stream.as_str().to_owned(),
            text: chunk.text,
        })
        .collect()
}

/// Loads runtime handles and resolved ids for one command-session mutating action.
/// 为单个命令会话变更动作加载运行时句柄与已解析标识。
///
/// # Parameters
/// 参数
/// `state` owns the retained service runtime handles.
/// `state` 持有保留的 service 运行时句柄。
/// `command_session_id` is the raw command-session id from the WebUI request.
/// `command_session_id` 是 WebUI 请求中的原始命令会话标识。
/// `owner_session_id` optionally contains the raw owner session id from the WebUI request.
/// `owner_session_id` 可选包含 WebUI 请求中的原始归属会话标识。
///
/// # Returns
/// 返回值
/// A command-session action context ready for runtime mutation and delta emission.
/// 返回可用于运行时变更与增量发送的命令会话动作上下文。
fn load_service_command_session_action_context(
    state: &SharedServiceBridgeState,
    command_session_id: String,
    owner_session_id: Option<String>,
) -> Result<CommandSessionActionContext, String> {
    let (controller_runtime, agent_runtime, _, _, _) = runtime_parts_from_state(state)?;
    let (command_session_id, owner_session_id) =
        resolve_service_command_session_ids(&agent_runtime, command_session_id, owner_session_id)?;
    Ok(CommandSessionActionContext {
        controller_runtime,
        agent_runtime,
        command_session_id,
        owner_session_id,
    })
}

/// Parses one command-session id from a WebUI service request.
/// 从 WebUI 服务请求中解析单个命令会话标识。
///
/// # Parameters
/// 参数
/// `command_session_id` is the raw request identifier text.
/// `command_session_id` 是原始请求标识文本。
///
/// # Returns
/// 返回值
/// The parsed command-session identifier or a request-specific parse error.
/// 返回解析后的命令会话标识，或请求专属解析错误。
fn parse_service_command_session_id(command_session_id: String) -> Result<SessionId, String> {
    parse_service_session_id(command_session_id, "command session")
}

/// Parses one command-session owner id from a WebUI service request.
/// 从 WebUI 服务请求中解析单个命令会话归属标识。
///
/// # Parameters
/// 参数
/// `owner_session_id` is the raw optional owner identifier text.
/// `owner_session_id` 是原始可选归属标识文本。
///
/// # Returns
/// 返回值
/// The parsed owner-session identifier or a request-specific parse error.
/// 返回解析后的归属会话标识，或请求专属解析错误。
fn parse_service_owner_session_id(owner_session_id: String) -> Result<SessionId, String> {
    parse_service_session_id(owner_session_id, "owner session")
}

/// Parses one WebUI service session id with a caller-specific error label.
/// 使用调用方专属错误标签解析单个 WebUI 服务会话标识。
///
/// # Parameters
/// 参数
/// `raw_session_id` is the raw request identifier text.
/// `raw_session_id` 是原始请求标识文本。
/// `error_label` names the request field in parse errors.
/// `error_label` 在解析错误中标识请求字段。
///
/// # Returns
/// 返回值
/// The parsed session identifier or a labelled parse error.
/// 返回解析后的会话标识，或带标签的解析错误。
fn parse_service_session_id(
    raw_session_id: String,
    error_label: &'static str,
) -> Result<SessionId, String> {
    raw_session_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid {error_label} id '{raw_session_id}': {error}"))
}

/// Sends structured input to one live command session and emits a command-session delta.
/// 向单条实时命令会话发送结构化输入并发送命令会话增量。
pub(super) async fn service_send_command_session_input(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: CommandSessionInputRequest,
) -> Result<Vec<CommandSessionSummary>, String> {
    let context = load_service_command_session_action_context(
        &state,
        request.command_session_id,
        request.owner_session_id,
    )?;
    context
        .agent_runtime
        .command_session_runtime
        .write_session_input_events(
            context.owner_session_id,
            context.command_session_id,
            request.inputs,
        )
        .await?;
    emit_command_sessions_changed_delta(
        &app,
        &state,
        &context.controller_runtime,
        &context.agent_runtime,
    )
}

/// Resizes one live command session and emits a command-session delta.
/// 调整单条实时命令会话大小并发送命令会话增量。
pub(super) async fn service_resize_command_session(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: CommandSessionResizeRequest,
) -> Result<Vec<CommandSessionSummary>, String> {
    let context = load_service_command_session_action_context(
        &state,
        request.command_session_id,
        request.owner_session_id,
    )?;
    context
        .agent_runtime
        .command_session_runtime
        .resize_session(
            context.owner_session_id,
            context.command_session_id,
            request.rows,
            request.cols,
        )
        .await?;
    emit_command_sessions_changed_delta(
        &app,
        &state,
        &context.controller_runtime,
        &context.agent_runtime,
    )
}

/// Emits the current command-session summaries without rebuilding the full workbench snapshot.
/// 不重建完整工作台快照，发送当前命令会话摘要。
fn emit_command_sessions_changed_delta(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
) -> Result<Vec<CommandSessionSummary>, String> {
    let command_sessions = command_sessions(
        controller_runtime,
        Some(agent_runtime.command_session_runtime.as_ref()),
    );
    let event = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        let generation = guard.workbench_generation;
        guard.workbench.revision = generation;
        guard.workbench.command_sessions = command_sessions.clone();
        WorkbenchDeltaEnvelope {
            generation,
            delta: WorkbenchDelta::CommandSessionsChanged(CommandSessionsChangedDelta {
                command_sessions: command_sessions.clone(),
            }),
        }
    };
    emit_service_event_with_trace(
        app,
        "workbench_command_sessions_delta_emit",
        ServiceEvent::WorkbenchDelta(Box::new(event)),
    );
    Ok(command_sessions)
}

/// Reloads live command-session summaries from the retained runtime and emits their workbench delta.
/// 从保留的运行时重新加载实时命令会话摘要，并发送对应的工作台增量。
///
/// # Parameters
/// 参数
/// `app` emits the resulting workbench delta to connected WebUI clients.
/// `app` 将生成的工作台增量发送给已连接的 WebUI 客户端。
/// `state` owns the retained controller and agent runtime handles.
/// `state` 持有保留的控制器与 Agent 运行时句柄。
///
/// # Returns
/// 返回值
/// The latest command-session summaries, or an explicit runtime/state projection error.
/// 返回最新命令会话摘要，或明确的运行时/状态投影错误。
pub(crate) fn emit_live_command_sessions_changed_delta(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
) -> Result<Vec<CommandSessionSummary>, String> {
    let (controller_runtime, agent_runtime, _, _, _) = runtime_parts_from_state(state)?;
    emit_command_sessions_changed_delta(app, state, &controller_runtime, &agent_runtime)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies command-session id parse errors keep their request-specific label.
    /// 验证命令会话标识解析错误会保留请求专属标签。
    #[test]
    fn parse_service_command_session_id_reports_command_label() {
        let error = parse_service_command_session_id(String::from("not-a-session-id"))
            .expect_err("invalid command-session id should report a parse error");

        assert!(error.starts_with("invalid command session id 'not-a-session-id':"));
    }

    /// Verifies owner-session id parse errors keep their request-specific label.
    /// 验证归属会话标识解析错误会保留请求专属标签。
    #[test]
    fn parse_service_owner_session_id_reports_owner_label() {
        let error = parse_service_owner_session_id(String::from("not-a-session-id"))
            .expect_err("invalid owner-session id should report a parse error");

        assert!(error.starts_with("invalid owner session id 'not-a-session-id':"));
    }
}
