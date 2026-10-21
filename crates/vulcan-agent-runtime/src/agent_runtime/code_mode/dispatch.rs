//! Routes nested JavaScript tool calls into the `exec` or `wait` call observing their cell.
//! 将嵌套 JavaScript 工具调用路由到正在观察其单元的 `exec` 或 `wait` 调用。
//!
//! Adapted from openai/codex `codex-rs/core/src/tools/code_mode/delegate.rs` (Apache-2.0).
//! Upstream queues nested calls on a session broker served by whichever turn is active. Vulcan
//! records every tool call under a durable identity, so a cell's nested calls run only while an
//! `exec` or `wait` call observes that cell and carry that call's identity; between observations
//! they wait, and `notify()` text is buffered for the next observation instead of being injected.
//! 改编自 openai/codex `codex-rs/core/src/tools/code_mode/delegate.rs`（Apache-2.0）。上游把嵌套
//! 调用排入会话级代理，由当前活动回合处理。Vulcan 的每次工具调用都有可靠身份，因此单元的嵌套调用
//! 只在 `exec` 或 `wait` 观察该单元时运行并使用该调用的身份；两次观察之间嵌套调用等待，
//! `notify()` 文本缓存到下一次观察，而不是直接注入。

use crate::agent_runtime::tool_permission::PermissionTexts;
use crate::agent_runtime::tool_runtime_bridge::VulcanManagedToolRuntimeBridge;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use vulcan_code_mode::{
    CellId, CodeModeNestedToolCall, CodeModeSessionDelegate, NotificationFuture, PUBLIC_TOOL_NAME,
    ToolInvocationFuture,
};
use vulcan_tools_manager::VulcanToolInvocationContext;

/// Dispatch state of one JavaScript cell, shared by its delegate and the calls observing it.
/// 一个 JavaScript 单元的分派状态，由其委托与观察它的调用共享。
pub(super) struct CellChannel {
    /// Dispatcher of the call currently observing the cell, if any.
    /// 当前正在观察该单元的调用的分派器（如有）。
    dispatcher: watch::Sender<Option<Arc<ActiveDispatcher>>>,
    /// `notify()` texts not yet delivered to the model.
    /// 尚未交付给模型的 `notify()` 文本。
    notifications: Mutex<Vec<String>>,
}

impl CellChannel {
    /// Creates the channel of a cell that no call observes yet.
    /// 创建一个尚无调用观察的单元通道。
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self {
            dispatcher: watch::channel(None).0,
            notifications: Mutex::new(Vec::new()),
        })
    }

    /// Makes one call the observer of this cell until the returned lease ends.
    /// 让一个调用成为此单元的观察者，直到返回的租约结束。
    pub(super) fn observe(self: &Arc<Self>, dispatcher: ActiveDispatcher) -> DispatchLease {
        let dispatcher = Arc::new(dispatcher);
        self.dispatcher.send_replace(Some(Arc::clone(&dispatcher)));
        DispatchLease {
            channel: Arc::clone(self),
            dispatcher,
            finished: false,
        }
    }

    /// Removes and returns the undelivered `notify()` texts.
    /// 移除并返回尚未交付的 `notify()` 文本。
    pub(super) fn take_notifications(&self) -> Vec<String> {
        std::mem::take(
            &mut *self
                .notifications
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        )
    }
}

/// Runs nested calls under the durable identity of one observing `exec` or `wait` call.
/// 在一次观察中的 `exec` 或 `wait` 调用的可靠身份下运行嵌套调用。
pub(super) struct ActiveDispatcher {
    /// Bridge without code mode, so a nested call can never re-enter `exec`.
    /// 不含代码模式的桥接层，嵌套调用永远无法重新进入 `exec`。
    bridge: VulcanManagedToolRuntimeBridge,
    /// Tool identifiers the observing run lets JavaScript call.
    /// 观察方所在运行允许 JavaScript 调用的工具标识。
    nested_tool_ids: Arc<BTreeSet<String>>,
    /// Durable identity of the observing call.
    /// 观察调用的可靠身份。
    context: VulcanToolInvocationContext,
    /// Latest user request, given to the permission pipeline.
    /// 交给权限管线的最近用户请求。
    user_request: Option<String>,
    /// Serializes nested calls: they share one identity, so approvals must not overlap.
    /// 串行化嵌套调用：它们共享同一身份，因此审批不得重叠。
    serial: tokio::sync::Mutex<()>,
    /// Set once the observing call returns; later nested calls wait for the next observer.
    /// 观察调用返回后置位；之后的嵌套调用等待下一位观察者。
    closed: AtomicBool,
    /// Cancelled when the observing call is abandoned, aborting its in-flight nested call.
    /// 观察调用被放弃时取消，并中止其进行中的嵌套调用。
    cancellation: CancellationToken,
}

impl ActiveDispatcher {
    /// Builds the dispatcher of one observing call.
    /// 构建一个观察调用的分派器。
    pub(super) fn new(
        bridge: VulcanManagedToolRuntimeBridge,
        nested_tool_ids: Arc<BTreeSet<String>>,
        context: VulcanToolInvocationContext,
        user_request: Option<String>,
    ) -> Self {
        Self {
            bridge,
            nested_tool_ids,
            context,
            user_request,
            serial: tokio::sync::Mutex::new(()),
            closed: AtomicBool::new(false),
            cancellation: CancellationToken::new(),
        }
    }

    /// Admits and runs one nested call; the caller holds `serial`.
    /// 放行并运行一次嵌套调用；调用方持有 `serial`。
    async fn run(&self, invocation: CodeModeNestedToolCall) -> Result<Value, String> {
        let tool_id = invocation.tool_name.name;
        if !self.nested_tool_ids.contains(&tool_id) {
            return Err(format!(
                "Tool `{tool_id}` is not available inside `{PUBLIC_TOOL_NAME}`."
            ));
        }
        // Every Vulcan tool is a JSON function, so nested input is always an argument object.
        // 每个 Vulcan 工具都是 JSON 函数，因此嵌套输入始终是参数对象。
        let arguments = match invocation.input {
            None => json!({}),
            Some(Value::Object(arguments)) => Value::Object(arguments),
            Some(_) => {
                return Err(format!(
                    "Tool `{tool_id}` expects a JSON object for its arguments."
                ));
            }
        };
        let definition = self
            .bridge
            .resolved_definition(&tool_id)
            .ok_or_else(|| format!("Tool `{tool_id}` is not registered for this run."))?;
        let raw_arguments = serde_json::to_string(&arguments).ok();
        let mut context = self.context.clone();
        context.controller_tool_call_id = format!(
            "{}/{}",
            self.context.controller_tool_call_id, invocation.runtime_tool_call_id
        );
        let invocation = self.bridge.invoke_definition(
            definition,
            arguments,
            PermissionTexts {
                raw_arguments: raw_arguments.as_deref(),
                user_request: self.user_request.as_deref(),
            },
            context,
        );
        let result = tokio::select! {
            _ = self.cancellation.cancelled() => {
                return Err(format!(
                    "The `{PUBLIC_TOOL_NAME}` call observing this cell ended; the nested call was cancelled."
                ));
            }
            result = invocation => result?,
        };
        // Host credentials are redacted once from the final `exec` output by the bridge.
        // 宿主凭据由桥接层从最终 `exec` 输出中统一移除。
        Ok(result.model_output)
    }
}

/// Keeps one call registered as the observer of a cell.
/// 让一个调用保持注册为某单元的观察者。
pub(super) struct DispatchLease {
    /// Channel of the observed cell.
    /// 被观察单元的通道。
    channel: Arc<CellChannel>,
    /// Dispatcher this lease installed.
    /// 此租约安装的分派器。
    dispatcher: Arc<ActiveDispatcher>,
    /// Whether `finish` drained the dispatcher.
    /// `finish` 是否已排空分派器。
    finished: bool,
}

impl DispatchLease {
    /// Stops accepting nested calls and waits for the in-flight one, so it ends inside this call.
    /// 停止接收嵌套调用并等待进行中的调用结束，使其在本次调用内完成。
    pub(super) async fn finish(mut self) {
        self.close();
        let _drained = self.dispatcher.serial.lock().await;
        self.finished = true;
    }

    /// Uninstalls this lease's dispatcher, leaving a newer observer in place.
    /// 卸载此租约的分派器，保留更新的观察者。
    fn close(&self) {
        // Clear the observer before marking it closed, so a waiting call never spins on it.
        // 先清除观察者再标记关闭，避免等待中的调用在其上空转。
        self.channel.dispatcher.send_if_modified(|current| {
            if current
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, &self.dispatcher))
            {
                *current = None;
                true
            } else {
                false
            }
        });
        self.dispatcher.closed.store(true, Ordering::SeqCst);
    }
}

impl Drop for DispatchLease {
    /// Cancels the in-flight nested call when the observing call is abandoned.
    /// 观察调用被放弃时取消进行中的嵌套调用。
    fn drop(&mut self) {
        if !self.finished {
            self.close();
            self.dispatcher.cancellation.cancel();
        }
    }
}

/// Session delegate of one cell, forwarding nested calls to the cell's current observer.
/// 单个单元的会话委托，将嵌套调用转交给该单元当前的观察者。
pub(super) struct CellDelegate {
    /// Channel of the cell.
    /// 该单元的通道。
    channel: Arc<CellChannel>,
}

impl CellDelegate {
    /// Creates the delegate of one cell.
    /// 创建一个单元的委托。
    pub(super) fn new(channel: Arc<CellChannel>) -> Self {
        Self { channel }
    }

    /// Waits for an observer and runs one nested call under its identity.
    /// 等待观察者并在其身份下运行一次嵌套调用。
    async fn dispatch(&self, invocation: CodeModeNestedToolCall) -> Result<Value, String> {
        let mut observers = self.channel.dispatcher.subscribe();
        loop {
            let dispatcher = observers
                .wait_for(Option::is_some)
                .await
                .map_err(|_| String::from("The code-mode cell was released."))?
                .clone()
                .ok_or_else(|| String::from("The code-mode cell lost its observer."))?;
            let _serial = dispatcher.serial.lock().await;
            if dispatcher.closed.load(Ordering::SeqCst) {
                continue;
            }
            return dispatcher.run(invocation).await;
        }
    }
}

impl CodeModeSessionDelegate for CellDelegate {
    fn invoke_tool<'a>(
        &'a self,
        invocation: CodeModeNestedToolCall,
        cancellation_token: CancellationToken,
    ) -> ToolInvocationFuture<'a> {
        Box::pin(async move {
            tokio::select! {
                _ = cancellation_token.cancelled() => {
                    Err(String::from("The nested tool call was cancelled."))
                }
                result = self.dispatch(invocation) => result,
            }
        })
    }

    fn notify<'a>(
        &'a self,
        _call_id: String,
        _cell_id: CellId,
        text: String,
        _cancellation_token: CancellationToken,
    ) -> NotificationFuture<'a> {
        if !text.trim().is_empty() {
            self.channel
                .notifications
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(text);
        }
        Box::pin(async { Ok(()) })
    }

    fn cell_closed(&self, _cell_id: &CellId) {}
}
