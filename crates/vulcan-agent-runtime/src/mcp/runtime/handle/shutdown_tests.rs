//! Final transport-close results must outlive bounded runtime observations.
//! 最终传输关闭结果必须在有界运行时观测之后继续存活。

use super::*;
use async_trait::async_trait;
use serde_json::{Value, json};
use std::sync::Weak;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::{broadcast, oneshot};
use vulcan_mcp_runtime::{McpRuntimeError, McpToolSchema};

/// A transport with separate bounded-observation and final-completion entry points.
/// 具有独立有界观测入口及最终完成入口的传输。
struct DeferredCloseTransport {
    /// Required transport event source.
    /// 传输契约要求的事件源。
    events: broadcast::Sender<McpTransportEvent>,
    /// Signals when the actual lifecycle wait starts.
    /// 实际生命周期等待开始时发出信号。
    entered: Mutex<Option<oneshot::Sender<()>>>,
    /// Test-controlled terminal result of the exact close operation.
    /// 测试控制的精确关闭操作终态结果。
    completion: tokio::sync::Mutex<Option<oneshot::Receiver<Result<(), McpRuntimeError>>>>,
    /// Counts bounded calls, which lifecycle supervision must avoid.
    /// 统计生命周期监督应当避免的有界调用。
    bounded_calls: Arc<AtomicUsize>,
    /// Counts actual final-result waits.
    /// 统计实际最终结果等待次数。
    final_calls: Arc<AtomicUsize>,
    /// A real retained allocation used to detect premature transport release.
    /// 用于检测传输过早释放的真实保留分配。
    _resource: Arc<()>,
}

#[async_trait]
impl McpTransport for DeferredCloseTransport {
    /// Returns the fixed credential-free test endpoint; takes no arguments.
    /// 返回固定且无凭据的测试端点；无参数。
    fn endpoint(&self) -> &str {
        "deferred-close"
    }

    /// Returns one valid schema so registered cleanup must withdraw a visible tool.
    /// 返回一个有效模式，要求已注册清理撤下可见工具。
    /// No inputs; returns one deterministic tool without invoking it.
    /// 无输入；返回确定性工具，不执行工具。
    async fn list_tools(&self) -> Result<Vec<McpToolSchema>, McpRuntimeError> {
        Ok(vec![McpToolSchema {
            name: String::from("retained_tool"),
            description: String::from("Lifecycle ownership fixture"),
            args_schema: json!({"type": "object"}),
            result_schema: json!({"type": "object"}),
        }])
    }

    /// Subscribes to fixture events; takes no arguments and returns a receiver.
    /// 订阅夹具事件；无参数并返回接收端。
    fn event_receiver(&self) -> broadcast::Receiver<McpTransportEvent> {
        self.events.subscribe()
    }

    /// Models an expired bounded observation without completing the original cleanup.
    /// 模拟有界观测到期，但不完成原清理。
    /// No inputs; always returns the observation error and records this entry.
    /// 无输入；始终返回观测错误并记录此入口调用。
    async fn shutdown(&self) -> Result<(), McpRuntimeError> {
        self.bounded_calls.fetch_add(1, Ordering::SeqCst);
        Err(McpRuntimeError::transport("bounded observation expired"))
    }

    /// Holds the transport until the original terminal result is explicitly released.
    /// 保留传输，直至原终态结果被明确放行。
    /// No inputs; returns the controlled close result exactly once.
    /// 无输入；仅返回一次受控关闭结果。
    async fn shutdown_and_wait(&self) -> Result<(), McpRuntimeError> {
        self.final_calls.fetch_add(1, Ordering::SeqCst);
        self.entered
            .lock()
            .expect("entry signal lock")
            .take()
            .expect("one close operation")
            .send(())
            .expect("entry observer retained");
        // Move the one receiver out before waiting; no mutex is held across cleanup.
        // 等待前移出唯一接收端；清理期间不持有互斥锁。
        let completion = self
            .completion
            .lock()
            .await
            .take()
            .expect("one terminal-result receiver");
        completion
            .await
            .expect("test must publish the final result")
    }

    /// Rejects unintended business execution; inputs are unused test call fields.
    /// 拒绝意外业务执行；输入为未使用的测试调用字段。
    /// Returns an explicit error so this fixture cannot hide tool invocation.
    /// 返回明确错误，防止夹具掩盖工具调用。
    async fn call_tool(&self, _name: &str, _args: Value) -> Result<Value, McpRuntimeError> {
        Err(McpRuntimeError::transport("fixture tools must not run"))
    }
}

/// Controls one isolated runtime with either a registered or an unpublished transport.
/// 控制一个包含已注册或未发布传输的隔离运行时。
struct ShutdownFixture {
    /// Runtime state whose terminal transition is being verified.
    /// 正在验证终态转换的运行时状态。
    runtime: McpRuntimeHandle,
    /// Exact server configuration shared by retry and shutdown paths.
    /// 重试与关闭路径共用的精确服务器配置。
    server: McpServerConfig,
    /// Carries a transport only when it has not entered the registry.
    /// 仅在传输尚未进入注册表时持有传输。
    unpublished: Option<Arc<dyn McpTransport>>,
    /// Signals that actual cleanup has started.
    /// 发出实际清理已开始的信号。
    entered: oneshot::Receiver<()>,
    /// Releases actual cleanup with a success or failure.
    /// 用成功或失败结果放行实际清理。
    complete: oneshot::Sender<Result<(), McpRuntimeError>>,
    /// Counts requests to the bounded observation API.
    /// 统计有界观测接口请求。
    bounded_calls: Arc<AtomicUsize>,
    /// Counts the final-result operation entry.
    /// 统计最终结果操作入口。
    final_calls: Arc<AtomicUsize>,
    /// Detects the exact transport's lifetime without keeping it alive.
    /// 检测精确传输的寿命，不延长其存活时间。
    resource: Weak<()>,
}

/// Creates a deterministic runtime without launching an external child.
/// 创建确定性运行时，不启动外部子进程。
/// The flag selects registration state; returns the exact owner and synchronization controls.
/// 标志选择注册状态；返回精确所有者及同步控制。
async fn fixture(registered: bool) -> ShutdownFixture {
    // Disabled bootstrap avoids a competing connection supervisor in this controlled fixture.
    // 停用引导避免此受控夹具出现竞争连接监督者。
    let server = McpServerConfig {
        source: crate::mcp::config::McpServerSource::Plugin {
            plugin_id: String::from("shutdown-fixture"),
            generation: 1,
        },
        id: String::from("shutdown-fixture:server"),
        name: String::from("Shutdown fixture"),
        enabled: false,
        transport: crate::mcp::config::McpServerTransportConfig::Stdio {
            command: String::from("unused"),
            args: Vec::new(),
            cwd: None,
            env: BTreeMap::new(),
            env_passthrough: Vec::new(),
            inherit_environment: false,
        },
    };
    let runtime = bootstrap_mcp_runtime(vec![server.clone()])
        .await
        .expect("controlled runtime");
    // Channels separate worker entry from final completion.
    // 通道将工作进入与最终完成分开。
    let (entered_tx, entered) = oneshot::channel();
    let (complete, completion) = oneshot::channel();
    let (events, _) = broadcast::channel(1);
    // Shared counters never own the transport resource.
    // 共享计数器不持有传输资源。
    let bounded_calls = Arc::new(AtomicUsize::new(0));
    let final_calls = Arc::new(AtomicUsize::new(0));
    let resource = Arc::new(());
    let weak_resource = Arc::downgrade(&resource);
    let transport: Arc<dyn McpTransport> = Arc::new(DeferredCloseTransport {
        events,
        entered: Mutex::new(Some(entered_tx)),
        completion: tokio::sync::Mutex::new(Some(completion)),
        bounded_calls: Arc::clone(&bounded_calls),
        final_calls: Arc::clone(&final_calls),
        _resource: resource,
    });
    // Both branches use the same production publication and shutdown functions.
    // 两个分支使用同一组生产发布与关闭函数。
    let unpublished = if registered {
        refresh_server_tools(&runtime.inner, &server, transport)
            .await
            .expect("publish fixture tool");
        None
    } else {
        update_server_status(
            &runtime.inner,
            &server.id,
            McpRuntimeServerStatus::Connecting,
            None,
            Vec::new(),
        );
        Some(transport)
    };
    ShutdownFixture {
        runtime,
        server,
        unpublished,
        entered,
        complete,
        bounded_calls,
        final_calls,
        resource: weak_resource,
    }
}

/// Checks final success and failure after both timeout and cancellation of foreground observers.
/// 校验前台观察者超时及取消之后的最终成功与失败。
/// No inputs or return value; verifies registered and unpublished ownership and tool withdrawal.
/// 无输入与返回值；验证已注册及未发布所有权和工具撤下。
#[tokio::test]
async fn final_cleanup_updates_original_state_after_observers_leave() {
    for registered in [false, true] {
        for failed in [false, true] {
            // The lifecycle worker is independent of all front-end observation futures.
            // 生命周期工作任务独立于全部前台观测任务。
            let fixture = fixture(registered).await;
            // Retirement keeps cleanup observable even after the last old execution view disappears.
            // 最后一个旧执行视图消失后，退役登记仍保持清理可观测。
            let previous_generation = McpRuntimeAccess::new(
                fixture.runtime.overview(),
                BTreeMap::from([(
                    String::from("plugin:shutdown-fixture:1"),
                    fixture.runtime.clone(),
                )]),
            );
            let retired_view = previous_generation.without_plugin_partition("shutdown-fixture");
            previous_generation
                .retire_plugin_partitions_after("shutdown-fixture", Duration::from_secs(3_600));
            drop(previous_generation);
            let inner = Arc::clone(&fixture.runtime.inner);
            let server_id = fixture.server.id.clone();
            let unpublished = fixture.unpublished;
            let worker = tokio::spawn(async move {
                shutdown_server_runtime_state_with_transport(&inner, &server_id, unpublished).await;
            });
            tokio::time::timeout(Duration::from_secs(5), fixture.entered)
                .await
                .expect("cleanup must start")
                .expect("cleanup entry signal");
            // Stopping is an explicit nonterminal state and does not expose cached tools.
            // 停止中是明确的非终态，不暴露缓存工具。
            let snapshot = fixture.runtime.snapshot();
            let state = snapshot
                .servers
                .get(&fixture.server.id)
                .expect("exact server");
            assert_eq!(state.status, McpRuntimeServerStatus::Stopping);
            assert!(state.visible_tool_descriptors().is_empty());
            assert!(
                fixture
                    .runtime
                    .transport_for_server(&fixture.server.id)
                    .is_none()
            );
            assert!(!fixture.runtime.is_fully_stopped());
            assert!(retired_view.has_usable_retired_plugin_partitions("shutdown-fixture"));
            fixture
                .runtime
                .wait_until_stopped(Duration::from_millis(10))
                .await
                .expect_err("finite observer must time out without completing cleanup");
            // Cancel a second foreground observer after polling it once.
            // 轮询第二个前台观察者后取消它。
            let observer_runtime = fixture.runtime.clone();
            let observer = tokio::spawn(async move {
                observer_runtime
                    .wait_until_stopped(Duration::from_secs(5))
                    .await
            });
            tokio::task::yield_now().await;
            observer.abort();
            assert!(
                observer
                    .await
                    .expect_err("observer cancelled")
                    .is_cancelled()
            );
            assert!(!worker.is_finished());
            assert!(fixture.resource.upgrade().is_some());
            assert_eq!(fixture.bounded_calls.load(Ordering::SeqCst), 0);
            assert_eq!(fixture.final_calls.load(Ordering::SeqCst), 1);
            fixture
                .complete
                .send(if failed {
                    Err(McpRuntimeError::transport(
                        "actual terminal cleanup failure",
                    ))
                } else {
                    Ok(())
                })
                .expect("release exact operation");
            tokio::time::timeout(Duration::from_secs(5), worker)
                .await
                .expect("lifecycle must publish completion")
                .expect("lifecycle join");
            assert!(fixture.resource.upgrade().is_none());
            let result = fixture
                .runtime
                .wait_until_stopped(Duration::from_millis(10))
                .await;
            if failed {
                assert_eq!(
                    result.expect_err("terminal cleanup failed"),
                    "actual terminal cleanup failure"
                );
                assert!(!fixture.runtime.is_fully_stopped());
                assert!(retired_view.has_usable_retired_plugin_partitions("shutdown-fixture"));
            } else {
                result.expect("late final success must advance original runtime");
                assert!(fixture.runtime.is_fully_stopped());
                assert!(!retired_view.has_usable_retired_plugin_partitions("shutdown-fixture"));
            }
        }
    }
}

/// Cleanup failure stops automatic retries before the ordinary connection budget is exhausted.
/// 清理失败会在普通连接预算耗尽前停止自动重试。
/// No inputs or return value; verifies original discovery and cleanup errors both remain visible.
/// 无输入与返回值；验证原发现错误与清理错误均保持可见。
#[tokio::test]
async fn failed_cleanup_blocks_reconnect_with_retry_budget_remaining() {
    // This instance failed discovery before publication and still owns its exact cleanup.
    // 此实例在发布前发现失败，仍持有自身精确清理。
    let fixture = fixture(false).await;
    let inner = Arc::clone(&fixture.runtime.inner);
    let server = fixture.server.clone();
    let unpublished = fixture.unpublished;
    let worker = tokio::spawn(async move {
        // The first connection failure still has automatic retries available.
        // 第一次连接失败仍有自动重试额度。
        let mut failures = 0;
        let stop = record_server_failure_and_should_stop(
            &inner,
            &server,
            &mut failures,
            unpublished,
            String::from("original discovery failure"),
        )
        .await;
        (stop, failures)
    });
    tokio::time::timeout(Duration::from_secs(5), fixture.entered)
        .await
        .expect("cleanup entry deadline")
        .expect("cleanup entry");
    assert!(!worker.is_finished());
    // The failure that initiated cleanup must remain inspectable throughout the pending state.
    // 在清理等待期间，触发清理的失败必须始终可供检查。
    let pending_snapshot = fixture.runtime.snapshot();
    let pending_state = pending_snapshot
        .servers
        .get(&fixture.server.id)
        .expect("pending server");
    assert_eq!(pending_state.status, McpRuntimeServerStatus::Stopping);
    assert_eq!(
        pending_state.last_error.as_deref(),
        Some("original discovery failure")
    );
    fixture
        .complete
        .send(Err(McpRuntimeError::transport("actual cleanup failure")))
        .expect("release failed cleanup");
    // A true stop decision prevents the caller's retry path from starting another transport.
    // 返回停止决定，阻止调用方重试路径启动另一传输。
    let (stop, failures) = tokio::time::timeout(Duration::from_secs(5), worker)
        .await
        .expect("failure completion deadline")
        .expect("failure worker join");
    assert!(stop);
    assert_eq!(failures, 1);
    let snapshot = fixture.runtime.snapshot();
    let state = snapshot
        .servers
        .get(&fixture.server.id)
        .expect("failed server");
    assert_eq!(state.status, McpRuntimeServerStatus::Failed);
    let error = state.last_error.as_deref().expect("combined diagnostic");
    assert!(error.contains("original discovery failure"));
    assert!(error.contains("actual cleanup failure"));
    assert_eq!(fixture.bounded_calls.load(Ordering::SeqCst), 0);
    assert_eq!(fixture.final_calls.load(Ordering::SeqCst), 1);
    assert!(fixture.resource.upgrade().is_none());
}
