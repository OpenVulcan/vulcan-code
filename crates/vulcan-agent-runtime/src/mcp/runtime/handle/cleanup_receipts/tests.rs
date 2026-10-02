//! Startup diagnostics, real cleanup results and lifecycle failure have separate authority.
//! 启动诊断、真实清理结果及生命周期失败具有分别独立的权威。

use super::*;
use crate::mcp::config::{McpServerOAuthConfig, McpServerSource, McpServerTransportConfig};
use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::broadcast;
use tokio::sync::oneshot;
use vulcan_mcp_runtime::{McpRuntimeError, McpToolSchema};

/// Creates one declared disabled runtime without spawning any transport or lifecycle task.
/// 创建单个声明停用的运行时，不启动任何传输或生命周期任务。
/// No inputs; returns its original handle and immutable declaration for direct cleanup tests.
/// 无输入；返回原句柄及不可变声明，用于直接清理测试。
async fn disabled_runtime() -> (McpRuntimeHandle, McpServerConfig) {
    let server = McpServerConfig {
        source: McpServerSource::UserConfigured,
        id: "cleanup-observation".into(),
        name: "Cleanup observation".into(),
        enabled: false,
        transport: McpServerTransportConfig::Stdio {
            command: "never-started".into(),
            args: Vec::new(),
            cwd: None,
            env: BTreeMap::new(),
            env_passthrough: Vec::new(),
            inherit_environment: false,
        },
    };
    let runtime = bootstrap_mcp_runtime(vec![server.clone()])
        .await
        .expect("disabled original runtime");
    (runtime, server)
}

/// A legitimate never-launched declaration still requires latched shutdown before it is retired.
/// 合法从未启动声明仍要求锁存关闭后才能退役。
#[tokio::test]
async fn cleanup_receipt_never_launched_disabled_requires_explicit_shutdown() {
    let (runtime, server) = disabled_runtime().await;
    assert!(runtime.inner.partition_release.is_released());
    assert!(
        !runtime.is_fully_stopped(),
        "physical absence alone does not close admission"
    );
    runtime.shutdown();
    assert!(
        runtime.is_fully_stopped(),
        "immutable disabled declaration never launched resources"
    );
    assert_eq!(
        runtime.snapshot().servers[&server.id].status,
        McpRuntimeServerStatus::Disabled
    );
    assert!(
        runtime
            .inner
            .cleanup_observations
            .read()
            .expect("cleanup observations")
            .is_empty(),
        "never-launched evidence must not fabricate an actual cleanup operation"
    );
}

/// A hand-written terminal snapshot cannot manufacture a successful actual cleanup result.
/// 手写终态快照不能制造成功实际清理结果。
#[tokio::test]
async fn cleanup_receipt_terminal_snapshot_without_actual_completion_is_unavailable() {
    let (runtime, server) = disabled_runtime().await;
    runtime.shutdown();
    update_server_status(
        &runtime.inner,
        &server.id,
        McpRuntimeServerStatus::Stopped,
        None,
        Vec::new(),
    );
    assert!(!runtime.is_fully_stopped());
    assert!(matches!(
        runtime.cleanup_admission(),
        McpCleanupAdmission::Pending
    ));
    // Even invoking real cleanup cannot turn missing original ownership into positive evidence.
    // 即使调用真实清理，也不能将缺少原所有权转换为正向证据。
    shutdown_server_runtime_state(&runtime.inner, &server.id).await;
    assert!(!runtime.is_fully_stopped());
    let error = runtime
        .wait_until_stopped(Duration::from_millis(10))
        .await
        .expect_err("a hand-written snapshot and empty maps cannot prove actual cleanup");
    assert!(error.contains("no original transport or release receipt"));
    shutdown_server_runtime_state(&runtime.inner, &server.id).await;
    assert_eq!(
        runtime
            .wait_until_stopped(Duration::from_millis(10))
            .await
            .expect_err("missing original authority remains negative"),
        error
    );
}

/// A real enabled OAuth initialization failure releases the original file lease and retains its diagnostic.
/// 真实启用 OAuth 初始化失败释放原文件租约，并保留其诊断。
#[tokio::test]
async fn cleanup_receipt_success_preserves_original_startup_failure_diagnostic() {
    let witness = tempfile::NamedTempFile::new().expect("actual prelaunch OAuth file witness");
    let lease = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(witness.path())
        .expect("original prelaunch OAuth file owner");
    lease
        .lock()
        .expect("original prelaunch OAuth exclusive lease");
    let (resources, original_release) = vulcan_mcp_runtime::McpTransportResources::new(lease);
    let server = McpServerConfig {
        source: McpServerSource::UserConfigured,
        id: "actual-prelaunch-oauth-failure".into(),
        name: "Actual prelaunch OAuth failure".into(),
        enabled: true,
        transport: McpServerTransportConfig::Http {
            // The actual session constructor rejects this URL before any native transport exists.
            // 实际会话构造器在任何原生传输存在之前拒绝此 URL。
            url: "relative-url-without-a-base".into(),
            bearer_token: None,
            headers: BTreeMap::new(),
            headers_from_env: BTreeMap::new(),
            oauth: Some(McpServerOAuthConfig::default()),
        },
    };
    let runtime = create_mcp_runtime(
        vec![server.clone()],
        resources,
        McpRuntimePurpose::TargetPreparation,
    )
    .expect("actual enabled prelaunch lifecycle");
    assert!(matches!(
        witness.as_file().try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    let original_error = runtime
        .wait_until_ready(ORIGINAL_CLEANUP_OBSERVATION)
        .await
        .expect_err("actual OAuth constructor rejects its original URL");
    assert!(original_error.contains("mcp.oauth.init"));
    tokio::time::timeout(ORIGINAL_CLEANUP_OBSERVATION, original_release.wait())
        .await
        .expect("actual prelaunch owner release deadline")
        .expect("original file owner released");
    witness
        .as_file()
        .try_lock()
        .expect("real prelaunch file lease released");
    let snapshot = runtime.snapshot();
    assert_eq!(
        snapshot.servers[&server.id].status,
        McpRuntimeServerStatus::Failed
    );
    assert_eq!(
        snapshot.servers[&server.id].last_error.as_deref(),
        Some(original_error.as_str())
    );
    assert!(
        runtime
            .inner
            .launch_inputs
            .read()
            .expect("actual inputs")
            .is_empty()
    );
    assert!(
        runtime
            .inner
            .transports
            .read()
            .expect("actual transports")
            .is_empty()
    );
    assert!(
        !runtime.is_fully_stopped(),
        "physical release before shutdown does not retire admission"
    );
    runtime.shutdown();
    runtime
        .wait_until_stopped(ORIGINAL_CLEANUP_OBSERVATION)
        .await
        .expect("same actual successful prelaunch scope cleanup");
    assert!(runtime.is_fully_stopped());
    assert_eq!(
        runtime.snapshot().servers[&server.id].last_error.as_deref(),
        Some(original_error.as_str())
    );
    let observations = runtime
        .inner
        .cleanup_observations
        .read()
        .expect("actual prelaunch cleanup observation");
    let observation = &observations[&server.id];
    assert!(observation.original_transport.is_none());
    assert!(
        observation
            .original_release
            .as_ref()
            .expect("same actual prelaunch receipt")
            .is_released()
    );
    assert!(matches!(&*observation.completion.borrow(), Some(Ok(_))));
}

/// Closing before a real enabled task's first poll still retires its original file-owning attempt scope.
/// 在真实启用任务首次轮询前关闭，仍退役其原文件所有尝试范围。
#[tokio::test]
async fn cleanup_receipt_first_shutdown_before_launch_releases_original_attempt_scope() {
    let witness = tempfile::NamedTempFile::new().expect("actual first-shutdown file witness");
    let lease = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(witness.path())
        .expect("original first-shutdown owner");
    lease
        .lock()
        .expect("original first-shutdown exclusive lease");
    let (resources, original_release) = vulcan_mcp_runtime::McpTransportResources::new(lease);
    let server = McpServerConfig {
        source: McpServerSource::UserConfigured,
        id: "actual-unpolled-shutdown".into(),
        name: "Actual unpolled shutdown".into(),
        enabled: true,
        transport: McpServerTransportConfig::Stdio {
            command: "must-never-be-spawned".into(),
            args: Vec::new(),
            cwd: None,
            env: BTreeMap::new(),
            env_passthrough: Vec::new(),
            inherit_environment: false,
        },
    };
    // The current-thread executor cannot poll its spawned task until this test yields after latched shutdown.
    // 当前线程执行器只有在本测试于锁存关闭后让出执行时，才能轮询其已启动任务。
    let runtime = create_mcp_runtime(
        vec![server.clone()],
        resources,
        McpRuntimePurpose::TargetPreparation,
    )
    .expect("original actual first-shutdown runtime");
    assert!(matches!(
        witness.as_file().try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    runtime.shutdown();
    assert!(!runtime.is_fully_stopped());
    runtime
        .wait_until_stopped(ORIGINAL_CLEANUP_OBSERVATION)
        .await
        .expect("actual original no-launch scope released");
    assert!(original_release.is_released());
    witness
        .as_file()
        .try_lock()
        .expect("original physical first-shutdown file lease released");
    assert!(runtime.is_fully_stopped());
    assert_eq!(
        runtime
            .inner
            .retry_counts
            .read()
            .expect("actual retry state")[&server.id],
        0
    );
    assert!(
        runtime
            .inner
            .launch_inputs
            .read()
            .expect("actual inputs")
            .is_empty()
    );
    let observations = runtime
        .inner
        .cleanup_observations
        .read()
        .expect("original attempt observation");
    let observation = &observations[&server.id];
    assert!(observation.original_transport.is_none());
    assert!(
        observation
            .original_release
            .as_ref()
            .expect("original actual attempt receipt")
            .is_released()
    );
    assert!(matches!(&*observation.completion.borrow(), Some(Ok(_))));
}

/// Controlled protocol failure supplies an actual result without any native release receipt.
/// 受控协议失败提供实际结果，不具有任何原生释放回执。
struct FailedCleanupTransport {
    /// Original event channel identifies this exact allocated transport.
    /// 原事件通道标识此精确已分配传输。
    events: broadcast::Sender<McpTransportEvent>,
}

#[async_trait]
impl McpTransport for FailedCleanupTransport {
    /// Returns a credential-free label; no inputs.
    /// 返回无凭据标签；无输入。
    fn endpoint(&self) -> &str {
        "failed-original-cleanup"
    }
    /// Returns an empty valid discovery result; no inputs.
    /// 返回空有效发现结果；无输入。
    async fn list_tools(&self) -> Result<Vec<McpToolSchema>, McpRuntimeError> {
        Ok(Vec::new())
    }
    /// Subscribes to the original lifecycle event channel; no inputs.
    /// 订阅原生命周期事件通道；无输入。
    fn event_receiver(&self) -> broadcast::Receiver<McpTransportEvent> {
        self.events.subscribe()
    }
    /// Returns the same original protocol failure even when no separate native receipt exists.
    /// 即使没有单独原生回执，也返回相同原协议失败。
    async fn shutdown_and_wait(&self) -> Result<(), McpRuntimeError> {
        Err(McpRuntimeError::transport("actual protocol cleanup failed"))
    }
    /// Rejects all business calls; name and args are unused test inputs.
    /// 拒绝全部业务调用；name 及 args 为未使用测试输入。
    async fn call_tool(
        &self,
        _name: &str,
        _args: serde_json::Value,
    ) -> Result<serde_json::Value, McpRuntimeError> {
        Err(McpRuntimeError::transport("unused"))
    }
}

/// An empty native receipt map cannot convert a real protocol cleanup error into successful retirement.
/// 空原生回执映射不能将真实协议清理错误转换为成功退役。
#[tokio::test]
async fn cleanup_receipt_protocol_failure_without_native_receipt_stays_failed() {
    let (runtime, server) = disabled_runtime().await;
    let (events, _) = broadcast::channel(1);
    let transport: Arc<dyn McpTransport> = Arc::new(FailedCleanupTransport { events });
    runtime.shutdown();
    shutdown_server_runtime_state_with_transport(&runtime.inner, &server.id, Some(transport)).await;
    assert!(
        runtime
            .inner
            .resource_releases
            .read()
            .expect("native receipts")
            .is_empty()
    );
    assert!(runtime.inner.partition_release.is_released());
    assert!(!runtime.is_fully_stopped());
    let error = runtime
        .wait_until_stopped(Duration::from_millis(10))
        .await
        .expect_err("real cleanup failure remains failure");
    assert!(error.contains("actual protocol cleanup failed"));
    assert!(
        !error.contains("timed out"),
        "terminal failure must preserve its own result"
    );
    // Repeating a no-op shutdown observes the same original error and cannot overwrite it with success.
    // 重复空操作关闭观察同一原错误，不能以成功覆盖。
    shutdown_server_runtime_state(&runtime.inner, &server.id).await;
    let repeated = runtime
        .wait_until_stopped(Duration::from_millis(10))
        .await
        .expect_err("original cleanup error must be immutable across repeated shutdown");
    assert_eq!(repeated, error);
    assert!(!runtime.is_fully_stopped());
}

/// A deferred actual protocol method retains a real file lease until original shutdown completes.
/// 延迟实际协议方法保留真实文件租约，直至原关闭完成。
struct DeferredOriginalCleanupTransport {
    /// The same allocated transport owns its original event channel.
    /// 相同已分配传输拥有原事件通道。
    events: broadcast::Sender<McpTransportEvent>,
    /// One native method entry signals that protocol cleanup really began.
    /// 单次原生方法进入发出协议清理真正开始的信号。
    entered: Mutex<Option<oneshot::Sender<()>>>,
    /// The original controlled final result stays pending until its actual operation completes.
    /// 原受控最终结果保持待决，直至其实际操作完成。
    result: tokio::sync::Mutex<Option<oneshot::Receiver<Result<(), McpRuntimeError>>>>,
    /// Counts real shutdown method execution, never resource identity or request authority.
    /// 统计真实关闭方法执行，绝不充当资源身份或请求权威。
    calls: Arc<AtomicUsize>,
    /// This exact transport owns the actual physical file lease observed by the original release receipt.
    /// 此精确传输拥有原释放回执观察的实际物理文件租约。
    _resources: vulcan_mcp_runtime::McpTransportResources,
}

#[async_trait]
impl McpTransport for DeferredOriginalCleanupTransport {
    /// Returns a credential-free endpoint; no inputs.
    /// 返回无凭据端点；无输入。
    fn endpoint(&self) -> &str {
        "deferred-original-cleanup"
    }
    /// Returns an empty valid tool list; no inputs.
    /// 返回空有效工具列表；无输入。
    async fn list_tools(&self) -> Result<Vec<McpToolSchema>, McpRuntimeError> {
        Ok(Vec::new())
    }
    /// Subscribes to this original transport's events; no inputs.
    /// 订阅此原传输事件；无输入。
    fn event_receiver(&self) -> broadcast::Receiver<McpTransportEvent> {
        self.events.subscribe()
    }
    /// Executes only the original protocol cleanup and waits for its true controlled final result.
    /// 仅执行原协议清理，并等待其真实受控最终结果。
    async fn shutdown_and_wait(&self) -> Result<(), McpRuntimeError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.entered
            .lock()
            .expect("actual entry signal")
            .take()
            .expect("one actual cleanup method")
            .send(())
            .expect("original entry observer");
        let result = self
            .result
            .lock()
            .await
            .take()
            .expect("one original final-result receiver");
        result
            .await
            .expect("original operation must publish completion")
    }
    /// Rejects unintended calls; name and args are unused test arguments.
    /// 拒绝意外调用；name 及 args 为未使用测试参数。
    async fn call_tool(
        &self,
        _name: &str,
        _args: serde_json::Value,
    ) -> Result<serde_json::Value, McpRuntimeError> {
        Err(McpRuntimeError::transport("unused"))
    }
}

/// Controlled outcomes belong to the original actual test operation, not a production state machine.
/// 受控结果属于原实际测试操作，不属于生产状态机。
#[derive(Clone, Copy)]
enum OriginalCleanupOutcome {
    /// Original protocol and native disposal both succeed.
    /// 原协议及原生销毁均成功。
    Success,
    /// Original protocol explicitly returns its actual failure.
    /// 原协议显式返回其实际失败。
    ProtocolError,
    /// The original producer is canceled while its real protocol call is pending.
    /// 真实协议调用待决时取消原产出者。
    ProducerLost,
}

/// Bounds observation of one controlled actual cleanup without changing production deadlines.
/// 限定单个受控实际清理的观察时间，不更改生产期限。
const ORIGINAL_CLEANUP_OBSERVATION: Duration = Duration::from_secs(5);

/// Runs concurrent callers against one real transport and file lease for the selected original outcome.
/// 针对选定原结果，在同一真实传输及文件租约上运行并发调用者。
/// Outcome controls the real method response or cancellation; returns after asserting shared immutable completion.
/// outcome 控制真实方法响应或取消；断言共享不可变完成结果后返回。
async fn assert_concurrent_original_cleanup(outcome: OriginalCleanupOutcome) {
    let (runtime, server) = disabled_runtime().await;
    let witness = tempfile::NamedTempFile::new().expect("actual deferred file witness");
    let lease = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(witness.path())
        .expect("actual deferred owner");
    lease.lock().expect("actual deferred exclusive lease");
    let (resources, release) = vulcan_mcp_runtime::McpTransportResources::new(lease);
    runtime
        .inner
        .resource_releases
        .write()
        .expect("original native receipt")
        .insert(server.id.clone(), release.clone());
    let (events, _) = broadcast::channel(1);
    let (entered, entry) = oneshot::channel();
    let (complete, result) = oneshot::channel();
    let calls = Arc::new(AtomicUsize::new(0));
    let transport: Arc<dyn McpTransport> = Arc::new(DeferredOriginalCleanupTransport {
        events,
        entered: Mutex::new(Some(entered)),
        result: tokio::sync::Mutex::new(Some(result)),
        calls: calls.clone(),
        _resources: resources,
    });
    let original_argument = transport.clone();
    let duplicate_argument = transport.clone();
    runtime
        .inner
        .transports
        .write()
        .expect("actual original transport")
        .insert(server.id.clone(), transport);
    runtime.shutdown();
    let original_inner = runtime.inner.clone();
    let original_id = server.id.clone();
    let original = tokio::spawn(async move {
        shutdown_server_runtime_state_with_transport(
            &original_inner,
            &original_id,
            Some(original_argument),
        )
        .await
    });
    tokio::time::timeout(ORIGINAL_CLEANUP_OBSERVATION, entry)
        .await
        .expect("actual original protocol entry")
        .expect("original entered");
    let duplicate_inner = runtime.inner.clone();
    let duplicate_id = server.id.clone();
    let duplicate = tokio::spawn(async move {
        shutdown_server_runtime_state_with_transport(
            &duplicate_inner,
            &duplicate_id,
            Some(duplicate_argument),
        )
        .await
    });
    runtime
        .wait_until_stopped(Duration::from_millis(10))
        .await
        .expect_err("original final result and real lease still pending");
    assert!(!original.is_finished());
    assert!(
        !duplicate.is_finished(),
        "duplicate must await the same pending original result"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(matches!(
        witness.as_file().try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    // Control the actual original producer; observers never publish a guessed result from empty maps.
    // 控制实际原产出者；观察者绝不根据空映射发布猜测结果。
    match outcome {
        OriginalCleanupOutcome::Success => complete
            .send(Ok(()))
            .expect("release original protocol completion"),
        OriginalCleanupOutcome::ProtocolError => complete
            .send(Err(McpRuntimeError::transport(
                "actual deferred cleanup failure",
            )))
            .expect("publish original protocol failure"),
        OriginalCleanupOutcome::ProducerLost => {
            original.abort();
        }
    }
    tokio::time::timeout(ORIGINAL_CLEANUP_OBSERVATION, async {
        match outcome {
            OriginalCleanupOutcome::ProducerLost => assert!(
                original
                    .await
                    .expect_err("original producer actually canceled")
                    .is_cancelled()
            ),
            _ => original.await.expect("original cleanup task"),
        }
        duplicate.await.expect("duplicate observer task");
    })
    .await
    .expect("same original protocol and physical final result");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "duplicate must not replay protocol shutdown"
    );
    witness
        .as_file()
        .try_lock()
        .expect("original physical owner released");
    assert!(
        release.is_released(),
        "same actual native receipt observed physical disposal"
    );
    match outcome {
        OriginalCleanupOutcome::Success => assert!(runtime.is_fully_stopped()),
        OriginalCleanupOutcome::ProtocolError | OriginalCleanupOutcome::ProducerLost => {
            assert!(
                !runtime.is_fully_stopped(),
                "physical disposal must not promote original failure"
            );
            let original_error = runtime
                .wait_until_stopped(Duration::from_millis(10))
                .await
                .expect_err("original cleanup remains negative");
            match outcome {
                OriginalCleanupOutcome::ProtocolError => {
                    assert!(original_error.contains("actual deferred cleanup failure"))
                }
                OriginalCleanupOutcome::ProducerLost => {
                    assert!(original_error.contains("owner ended without publishing"))
                }
                OriginalCleanupOutcome::Success => {
                    unreachable!("successful outcome handled separately")
                }
            }
            shutdown_server_runtime_state(&runtime.inner, &server.id).await;
            assert_eq!(
                runtime
                    .wait_until_stopped(Duration::from_millis(10))
                    .await
                    .expect_err("no-op repeat cannot rewrite original failure"),
                original_error
            );
            assert_eq!(calls.load(Ordering::SeqCst), 1);
            assert!(!runtime.is_fully_stopped());
        }
    }
}

/// Concurrent callers cannot replay shutdown or finish before the original protocol and physical release.
/// 并发调用者不能重放关闭，也不能早于原协议及物理释放完成。
#[tokio::test]
async fn cleanup_receipt_concurrent_callers_share_one_original_protocol_and_physical_result() {
    assert_concurrent_original_cleanup(OriginalCleanupOutcome::Success).await;
}

/// Both concurrent callers retain one original protocol error across repeated no-op cleanup.
/// 两个并发调用者跨重复空操作清理保留同一原协议错误。
#[tokio::test]
async fn cleanup_receipt_concurrent_protocol_error_is_immutable_after_physical_release() {
    assert_concurrent_original_cleanup(OriginalCleanupOutcome::ProtocolError).await;
}

/// Canceling the actual producer publishes a negative result to every original observer.
/// 取消实际产出者向每个原观察者发布负向结果。
#[tokio::test]
async fn cleanup_receipt_concurrent_producer_loss_is_negative_after_physical_release() {
    assert_concurrent_original_cleanup(OriginalCleanupOutcome::ProducerLost).await;
}

/// Unexpected original task disposal invalidates a previously successful cleanup observation.
/// 意外原任务销毁使先前成功清理观察失效。
#[tokio::test]
async fn cleanup_receipt_abnormal_lifecycle_disposal_never_promotes_physical_release() {
    let (runtime, server) = disabled_runtime().await;
    let (resources, release) = vulcan_mcp_runtime::McpTransportResources::new(());
    let admission =
        lifecycle_guard::retain_lifecycle_admission(runtime.inner.clone(), &server, resources);
    runtime.shutdown();
    // A distinct original cleanup resource really releases before the later abnormal lifecycle disposal.
    // 单独原清理资源在之后的异常生命周期销毁之前真正释放。
    let (actual_cleanup_owner, actual_cleanup_release) =
        vulcan_mcp_runtime::McpTransportResources::new(());
    runtime
        .inner
        .resource_releases
        .write()
        .expect("original cleanup receipt")
        .insert(server.id.clone(), actual_cleanup_release);
    drop(actual_cleanup_owner);
    shutdown_server_runtime_state(&runtime.inner, &server.id).await;
    // A new nonterminal original lifecycle state must not inherit its preceding operation's successful receipt.
    // 新非终态原生命周期状态不能继承其前一操作的成功回执。
    update_server_status(
        &runtime.inner,
        &server.id,
        McpRuntimeServerStatus::Connecting,
        None,
        Vec::new(),
    );
    drop(admission);
    assert!(release.is_released());
    assert!(runtime.inner.partition_release.is_released());
    assert!(!runtime.is_fully_stopped());
    assert_eq!(
        runtime.snapshot().servers[&server.id].status,
        McpRuntimeServerStatus::Failed
    );
    assert!(matches!(
        runtime.cleanup_admission(),
        McpCleanupAdmission::Failed(_)
    ));
}
