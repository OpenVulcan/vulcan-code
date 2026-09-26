//! Retained terminal publication retries alongside normal detached stream delivery.
//! 在正常分离流投递旁保留终态发布重试。

use super::*;
use tokio::time::{Duration, Instant};

/// Immutable terminal evidence retained independently of later stream sink updates.
/// 与后续流接收器更新相互独立保留的不可变终态凭据。
struct PendingTerminal {
    /// Original parent event, including exact mailbox identity and failure cause.
    /// 原父事件，包含精确邮箱身份与失败原因。
    event: RuntimeEvent,
    /// Startup abort and natural completion require different admission authority.
    /// 启动中止与自然完成需要不同准入权威。
    startup_aborted: bool,
    /// Successful source receipt must not be replaced by a later auxiliary API.
    /// 成功来源回执不得被后续辅助 API 替换。
    completed_assistant: Option<ProviderV1CompletedAssistantText>,
    /// Next attempt uses monotonic time, unaffected by wall-clock corrections.
    /// 下次尝试使用单调时间，不受墙钟修正影响。
    next_attempt: Instant,
    /// Bounded delay prevents persistent storage failures from creating a busy loop.
    /// 有界延迟防止持续存储失败形成忙循环。
    delay: Duration,
    /// Retain the last diagnostic to avoid repeating the same user-facing warning.
    /// 保留最后诊断，避免重复相同面向用户警告。
    last_error: Option<String>,
}

impl PendingTerminal {
    /// Captures `event` only after exact owner validation; clones a successful source from `sink` once.
    /// 仅在精确归属校验后捕获 `event`；从 `sink` 一次性克隆成功来源。
    /// Returns a pending terminal, or no value for ordinary stream delivery.
    /// 返回待发布终态；普通流投递返回空值。
    fn capture(
        event: &DetachedTaskRuntimeEvent,
        parent_id: SessionId,
        task_id: SessionId,
        execution_id: &str,
        sink: &ProviderV1StreamSink,
    ) -> Result<Option<Self>, String> {
        // Select only explicit terminal variants; EOF never manufactures a terminal outcome.
        // 仅选择明确终态分支；流结束绝不制造终态结果。
        let (event_parent, event, startup_aborted) = match event {
            DetachedTaskRuntimeEvent::StartupAborted {
                parent_session_id,
                event,
            } => (*parent_session_id, event, true),
            DetachedTaskRuntimeEvent::ParentRuntime {
                parent_session_id,
                event,
            } if event.kind == RuntimeEventKind::DelegateFinished => {
                (*parent_session_id, event, false)
            }
            _ => return Ok(None),
        };
        if event_parent != parent_id {
            return Err(String::from(
                "Child terminal has a different original parent.",
            ));
        }
        non_success::terminal_mailbox(event, parent_id, task_id, execution_id)?;
        Ok(Some(Self {
            event: event.clone(),
            startup_aborted,
            completed_assistant: sink.completed_assistant_text().cloned(),
            next_attempt: Instant::now(),
            delay: Duration::from_millis(250),
            last_error: None,
        }))
    }

    /// Checks whether a duplicate has the same immutable terminal identity and content.
    /// 检查重复事件是否具有相同不可变终态身份与内容。
    /// `other` cannot replace the first accepted evidence; returns exact equality for relevant host fields.
    /// `other` 不能替换首次接纳凭据；返回相关宿主字段的精确相等结果。
    fn same_terminal(&self, other: &Self) -> bool {
        self.startup_aborted == other.startup_aborted
            && self.event.kind == other.event.kind
            && self.event.title == other.event.title
            && self.event.detail == other.event.detail
            && self.event.metadata == other.event.metadata
    }

    /// Reschedules an unsuccessful attempt and returns whether its diagnostic changed.
    /// 重新安排未成功尝试，并返回诊断是否发生变化。
    /// `error` is preserved verbatim; the maximum retry interval is thirty seconds.
    /// 原样保留 `error`；重试间隔最大为三十秒。
    fn defer(&mut self, error: String) -> bool {
        // Failed attempts never release their original evidence or execution ownership.
        // 失败尝试绝不释放原凭据或执行归属。
        let changed = self.last_error.as_ref() != Some(&error);
        self.last_error = Some(error);
        self.next_attempt = Instant::now() + self.delay;
        self.delay = (self.delay * 2).min(Duration::from_secs(30));
        changed
    }
}

/// Drives `stream` while retrying its exact terminal after publication or continuation failure.
/// 驱动 `stream`，并在发布或续跑失败后重试其精确终态。
/// Captured service owners and startup owner authorize retries; sinks continue receiving ordinary events.
/// 捕获的服务所有者及启动所有者授权重试；接收器继续接收普通事件。
/// Returns after EOF and successful terminal processing; unresolved failures retain their owner in this task.
/// 流结束且终态处理成功后返回；未解决失败在此任务中保留其所有者。
// Stream ownership, generation guards and independently borrowed telemetry remain explicit at this event boundary.
// 事件边界显式保留流所有权、代际守卫和独立借用的遥测状态。
#[expect(
    clippy::too_many_arguments,
    reason = "Stream ownership, generation guards and independently borrowed telemetry remain explicit at this event boundary."
)]
pub(crate) async fn drive<S>(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller: &HostRuntimeHandle,
    runtime: &AgentRuntimeBridge,
    parent_id: SessionId,
    task_id: SessionId,
    execution_id: &str,
    child: &Arc<SessionHandle>,
    startup_owner: startup_rejection::DetachedStartupOwner,
    stream: S,
    telemetry: &mut ProviderTelemetryHookGuard,
    sink: &mut ProviderV1StreamSink,
) where
    S: futures_util::Stream<Item = DetachedTaskRuntimeEvent>,
{
    // Pin the original subscription once; a retry never opens a second consumer.
    // 只固定原订阅一次；重试绝不打开第二个消费者。
    let mut stream = Box::pin(stream);
    // At most one terminal belongs to one concrete execution.
    // 一个具体执行最多拥有一个终态。
    let mut pending: Option<PendingTerminal> = None;
    // EOF stops polling the input but does not drop an unresolved terminal.
    // 流结束停止轮询输入，但不丢弃未解决终态。
    let mut ended = false;
    loop {
        if ended && pending.is_none() {
            break;
        }
        // The disabled timer branch never substitutes for the absent pending terminal.
        // 禁用的计时分支绝不替代缺失的待处理终态。
        let deadline = pending
            .as_ref()
            .map(|entry| entry.next_attempt)
            .unwrap_or_else(Instant::now);
        tokio::select! {
            biased;
            // An already-due attempt runs immediately; timer-wheel rounding must not defer first publication.
            // 已到期尝试立即执行；计时轮取整不得延后首次发布。
            _ = async {
                if deadline > Instant::now() {
                    tokio::time::sleep_until(deadline).await;
                }
            }, if pending.is_some() => {
                // The timer can run only with the exact retained evidence still present.
                // 只有精确保留凭据仍然存在时计时分支才能运行。
                let terminal = pending.as_mut().expect("pending terminal enables its timer");
                // Each retry re-enters the production idempotent publication and continuation path.
                // 每次重试重新进入生产幂等发布与续跑路径。
                let result = if terminal.startup_aborted {
                    startup_abort::handle_startup_abort(app, state, runtime, parent_id, task_id, execution_id, child, startup_owner, &terminal.event)
                } else {
                    // Continuation preparation carries a large future; keep it off the listener's stack frame.
                    // 续跑准备携带较大 Future；将其移出监听器栈帧。
                    Box::pin(terminal::handle_parent_event(app, state, controller, runtime, task_id, execution_id, child, parent_id, terminal.event.clone(), terminal.completed_assistant.as_ref())).await
                };
                match result {
                    Ok(()) => pending = None,
                    Err(error) => {
                        if terminal.defer(error.clone()) {
                            tracing::warn!(%task_id, %execution_id, %error, "Child terminal remains pending; retry retained");
                            if let Err(diagnostic) = crate::push_webui_service_diagnostic(state, format!("子代理结果暂未完成交付，正在保留原结果重试：{error}")) {
                                tracing::warn!(%diagnostic, "Child terminal retry diagnostic failed");
                            }
                        }
                    }
                }
            }
            incoming = stream.next(), if !ended => {
                let Some(event) = incoming else { ended = true; continue; };
                webui_trace("detached_listener_event", format!("task={task_id} execution_id={execution_id} kind={} {}", detached_task_event_label(&event), detached_task_event_trace_detail(&event)));
                match PendingTerminal::capture(&event, parent_id, task_id, execution_id, sink) {
                    Ok(Some(terminal)) => {
                        if let Some(original) = &pending {
                            if !original.same_terminal(&terminal) {
                                tracing::error!(%task_id, %execution_id, "Conflicting terminal cannot replace retained child evidence");
                            }
                        } else { pending = Some(terminal); }
                    }
                    Ok(None) => {
                        // Ordinary Provider handling also owns a large future, independently of the retry branch.
                        // 普通 Provider 处理同样拥有较大 Future，与重试分支分别分配。
                        if let Err(error) = Box::pin(handle_webui_detached_subagent_task_event(app, state, controller, runtime, task_id, execution_id, child, telemetry, sink, event)).await {
                            tracing::warn!(%task_id, %execution_id, %error, "Detached child event failed");
                        }
                    }
                    Err(error) => tracing::error!(%task_id, %execution_id, %error, "Invalid child terminal rejected before retry admission"),
                }
            }
        }
    }
}
