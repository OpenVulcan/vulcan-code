//! Recent output of running `shell_command` calls for open command cards.
//! 为已展开的命令卡提供运行中 `shell_command` 调用的最近输出。
//!
//! A card attaches while it is open and the command runs; the service then pushes the latest recent lines, at
//! most once per push interval, until the card detaches, its browser disconnects or the command ends. Nothing is
//! stored: once the command ends the card shows the durable result instead.
//! 卡片在展开且命令运行期间挂接；服务随后推送最新的最近若干行，每个推送间隔至多一次，直到卡片解除挂接、其浏览器
//! 断开或命令结束。不保存任何内容：命令结束后卡片改为显示可靠结果。

use super::*;
use vulcan_agent_runtime::tooling::native::{
    SHELL_COMMAND_LIVE_TAIL_LINES, ShellCommandLiveKey, shell_command_live_run,
};
use vulcan_service_api::{ShellCommandOutputAttachment, ShellCommandOutputChangedEvent};

/// Shortest time between two pushes for one card; bursts of output inside it become one push.
/// 同一卡片两次推送之间的最短时间；其间的突发输出合并为一次推送。
const SHELL_COMMAND_OUTPUT_PUSH_INTERVAL: std::time::Duration =
    std::time::Duration::from_millis(500);

/// Ownership and cancellation of one open card's subscription.
/// 一个已展开卡片订阅的所有权与取消句柄。
struct ShellCommandOutputSubscription {
    /// Browser or WebView lifetime that owns the card.
    /// 持有该卡片的浏览器或 WebView 生命周期。
    client_id: Uuid,
    /// Stops the push task.
    /// 停止推送任务。
    cancel: tokio::sync::oneshot::Sender<()>,
}

/// Subscriptions of currently open cards.
/// 当前已展开卡片的订阅。
static SHELL_COMMAND_OUTPUT_SUBSCRIPTIONS: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<String, ShellCommandOutputSubscription>>,
> = std::sync::OnceLock::new();

/// Returns the process-local subscription registry.
/// 返回进程内订阅注册表。
fn shell_command_output_subscriptions()
-> &'static std::sync::Mutex<std::collections::HashMap<String, ShellCommandOutputSubscription>> {
    SHELL_COMMAND_OUTPUT_SUBSCRIPTIONS
        .get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// Attaches one open command card to the recent output of its running command.
/// 将一张已展开的命令卡挂接到其运行中命令的最近输出。
///
/// # Parameters / 参数
/// - `app`: Service event emitter used for pushes.
/// - `session_id`: Session that owns the tool call.
/// - `api_call_id`, `tool_call_id`: Exact tool call within that session.
/// - `display_client_id`: Browser/WebView lifetime shared with the event socket.
/// - `app`：用于推送的服务事件发送器。
/// - `session_id`：持有该工具调用的会话。
/// - `api_call_id`、`tool_call_id`：该会话内的精确工具调用。
/// - `display_client_id`：与事件套接字共享的浏览器或 WebView 生命周期。
///
/// # Returns / 返回值
/// Returns the current recent lines and, while the command runs, a subscription for later changes.
/// 返回当前最近输出行；命令仍在运行时同时返回用于后续变化的订阅。
pub(super) fn service_attach_shell_command_output(
    app: ServiceEventEmitter,
    session_id: String,
    api_call_id: u64,
    tool_call_id: u64,
    display_client_id: String,
) -> Result<ShellCommandOutputAttachment, String> {
    let display_client_id = display_client_id.parse::<Uuid>().map_err(|error| {
        format!("Invalid shell-command output display client id `{display_client_id}`: {error}")
    })?;
    let not_running = ShellCommandOutputAttachment {
        subscription_id: None,
        running: false,
        lines: Vec::new(),
        line_limit: SHELL_COMMAND_LIVE_TAIL_LINES,
    };
    let key = ShellCommandLiveKey::new(&session_id, api_call_id, tool_call_id);
    let Some(run) = shell_command_live_run(&key) else {
        return Ok(not_running);
    };
    // Subscribing before the running check means an end after the check still wakes the push task.
    // 先订阅再检查是否运行，使检查之后发生的结束仍会唤醒推送任务。
    let mut changes = run.subscribe();
    if !run.is_running() {
        return Ok(not_running);
    }
    let subscription_id = Uuid::new_v4().to_string();
    let (cancel_sender, mut cancel_receiver) = tokio::sync::oneshot::channel();
    shell_command_output_subscriptions()
        .lock()
        .map_err(|_| String::from("Shell-command output subscription registry is poisoned."))?
        .insert(
            subscription_id.clone(),
            ShellCommandOutputSubscription {
                client_id: display_client_id,
                cancel: cancel_sender,
            },
        );
    let lines = run.recent_lines();

    let task_subscription_id = subscription_id.clone();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut cancel_receiver => break,
                changed = changes.changed() => if changed.is_err() { break },
            }
            // Waiting out the interval first folds a burst of output into one push.
            // 先等满间隔，使一阵突发输出合并为一次推送。
            if run.is_running() {
                tokio::select! {
                    _ = &mut cancel_receiver => break,
                    () = tokio::time::sleep(SHELL_COMMAND_OUTPUT_PUSH_INTERVAL) => {}
                }
            }
            let running = run.is_running();
            let emitted = emit_shell_command_output_if_attached(
                &app,
                display_client_id,
                ShellCommandOutputChangedEvent {
                    display_client_id: display_client_id.to_string(),
                    subscription_id: task_subscription_id.clone(),
                    running,
                    lines: run.recent_lines(),
                    line_limit: SHELL_COMMAND_LIVE_TAIL_LINES,
                },
            );
            if !emitted || !running {
                break;
            }
        }
        if let Ok(mut subscriptions) = shell_command_output_subscriptions().lock() {
            subscriptions.remove(&task_subscription_id);
        }
    });

    Ok(ShellCommandOutputAttachment {
        subscription_id: Some(subscription_id),
        running: true,
        lines,
        line_limit: SHELL_COMMAND_LIVE_TAIL_LINES,
    })
}

/// Detaches one closed command card and stops its pushes.
/// 解除一张已收起命令卡的挂接并停止其推送。
///
/// # Returns / 返回值
/// Returns `true` only when an active subscription was cancelled.
/// 仅在成功取消一个活动订阅时返回 `true`。
pub(super) fn service_detach_shell_command_output(subscription_id: String) -> Result<bool, String> {
    let subscription = shell_command_output_subscriptions()
        .lock()
        .map_err(|_| String::from("Shell-command output subscription registry is poisoned."))?
        .remove(&subscription_id);
    let Some(subscription) = subscription else {
        return Ok(false);
    };
    let _ = subscription.cancel.send(());
    Ok(true)
}

/// Emits one change only while its exact subscription is registered; the registry lock makes detach a strict
/// boundary after which no more output is pushed for that subscription.
/// 仅在精确订阅仍登记时发出一次变化；注册表锁使解除挂接成为严格边界，此后不再为该订阅推送输出。
fn emit_shell_command_output_if_attached(
    app: &ServiceEventEmitter,
    display_client_id: Uuid,
    event: ShellCommandOutputChangedEvent,
) -> bool {
    let Ok(subscriptions) = shell_command_output_subscriptions().lock() else {
        return false;
    };
    let attached = subscriptions
        .get(&event.subscription_id)
        .is_some_and(|subscription| subscription.client_id == display_client_id);
    if attached {
        app.emit(ServiceEvent::ShellCommandOutputChanged(Box::new(event)));
    }
    attached
}

/// Cancels every command-card subscription owned by one disconnected browser client.
/// 取消由一个已断开浏览器客户端持有的全部命令卡订阅。
pub(crate) fn cancel_shell_command_outputs_for_client(client_id: Uuid) {
    let Ok(mut subscriptions) = shell_command_output_subscriptions().lock() else {
        return;
    };
    subscriptions.retain(|_, subscription| subscription.client_id != client_id);
}
