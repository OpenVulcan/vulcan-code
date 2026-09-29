//! Runtime stopping requires actual launch release, including initialization that never returned.
//! 运行时停止必须等待实际启动释放，包括从未返回的初始化。

use super::*;
use std::fs::{OpenOptions, TryLockError};

/// An unpublished launch's original resource receipt gates stopped state without a returned transport.
/// 未发布启动的原资源回执在尚无返回传输时控制停止状态。
/// No inputs; real host file ownership survives timed-out/cancelled observers until its actual owner drops.
/// 无输入；真实宿主文件所有权在观察超时及取消后保留，直至实际所有者销毁。
#[tokio::test]
async fn original_resource_receipt_gates_unpublished_shutdown() {
    // Disabled bootstrap gives the test sole ownership of an unpublished launch's lifecycle.
    // 停用引导使测试独占未发布启动的生命周期。
    let witness = tempfile::NamedTempFile::new().expect("unpublished lease witness");
    let mut server = native_server(witness.path());
    server.enabled = false;
    let runtime = bootstrap_mcp_runtime(vec![server.clone()])
        .await
        .expect("unpublished runtime");
    // This actual separately opened file enters the same public resource bundle used by transport owners.
    // 此实际单独打开文件进入与传输所有者相同的公开资源包。
    let lease = OpenOptions::new()
        .read(true)
        .write(true)
        .open(witness.path())
        .expect("unpublished host lease");
    lease.lock().expect("unpublished exclusive host lease");
    let (actual_owner, release) = vulcan_mcp_runtime::McpTransportResources::new(lease);
    runtime
        .inner
        .resource_releases
        .write()
        .expect("release map")
        .insert(server.id.clone(), release);
    let inner = runtime.inner.clone();
    let server_id = server.id.clone();
    let cleanup =
        tokio::spawn(async move { shutdown_server_runtime_state(&inner, &server_id).await });
    runtime
        .wait_until_stopped(Duration::from_millis(50))
        .await
        .expect_err("actual owner still retains its lease");
    assert_eq!(
        runtime.snapshot().servers[&server.id].status,
        McpRuntimeServerStatus::Stopping
    );
    assert!(matches!(
        witness.as_file().try_lock(),
        Err(TryLockError::WouldBlock)
    ));
    assert!(!cleanup.is_finished());
    // Cancelling a second observer does not cancel the one original resource owner or cleanup.
    // 取消第二个观察者不会取消唯一原资源所有者或清理。
    let observing = runtime.clone();
    let observer = tokio::spawn(async move { observing.wait_until_stopped(RELEASE_OBSERVE).await });
    tokio::task::yield_now().await;
    observer.abort();
    assert!(
        observer
            .await
            .expect_err("observer cancelled")
            .is_cancelled()
    );
    drop(actual_owner);
    tokio::time::timeout(RELEASE_OBSERVE, cleanup)
        .await
        .expect("original cleanup deadline")
        .expect("original cleanup joined");
    runtime
        .wait_until_stopped(RELEASE_OBSERVE)
        .await
        .expect("actual resource release observed");
    witness
        .as_file()
        .try_lock()
        .expect("host file released before successful stopped");
    assert!(runtime.is_fully_stopped());
}

/// One observation deadline for the real process release fixture.
/// 真实进程释放夹具共用的一项观察截止时间。
const RELEASE_OBSERVE: Duration = Duration::from_secs(10);

/// Holds an actual exclusive file lock while deliberately ignoring protocol input and EOF.
/// 故意忽略协议输入及 EOF，同时持有真实排他文件锁。
/// No arguments; the parent supplies a private fixture path and owns eventual process termination.
/// 无参数；父测试提供私有夹具路径，并负责最终进程终止。
#[test]
#[ignore = "Child-only entry point invoked by the host release-barrier test."]
fn resource_barrier_child_fixture() {
    // Only this exact child owns the contested lease, never the parent supervisor.
    // 仅此精确子进程拥有争用租约，父监督者绝不拥有。
    let path = std::env::var_os("VULCAN_HOST_RELEASE_BARRIER_LEASE").expect("private child lease");
    let lease = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .expect("child lease handle");
    lease.lock().expect("child exclusive lease");
    loop {
        std::thread::park();
    }
}

/// Builds the exact native child declaration for the retained file witness.
/// 为保留文件见证构建精确原生子进程声明。
/// Lease is an existing file path; returns one enabled stdio declaration with a fixed fixture entry.
/// lease 为现存文件路径；返回具有固定夹具入口的一项启用标准输入输出声明。
fn native_server(lease: &std::path::Path) -> McpServerConfig {
    McpServerConfig {
        source: crate::mcp::config::McpServerSource::UserConfigured,
        id: "actual-release-barrier".into(),
        name: "Actual release barrier".into(),
        enabled: true,
        transport: crate::mcp::config::McpServerTransportConfig::Stdio {
            command: std::env::current_exe()
                .expect("host test executable")
                .to_str()
                .expect("UTF-8 test executable")
                .into(),
            args: vec![
                "--exact".into(),
                "mcp::runtime::handle::release_barrier_tests::resource_barrier_child_fixture"
                    .into(),
                "--ignored".into(),
                "--nocapture".into(),
            ],
            cwd: None,
            env: BTreeMap::from([(
                "VULCAN_HOST_RELEASE_BARRIER_LEASE".into(),
                lease.to_str().expect("UTF-8 lease path").into(),
            )]),
            env_passthrough: Vec::new(),
            inherit_environment: true,
        },
    }
}

/// Native cleanup keeps the host stopping until the real child releases its file lock.
/// 原生清理保持宿主停止中，直至真实子进程释放文件锁。
/// No inputs; finite observers may leave, but successful stopped state requires actual release.
/// 无输入；有限观察者可以退出，但成功停止状态必须具有实际释放证据。
#[tokio::test]
async fn cancelled_initialize_stays_stopping_until_real_child_exit() {
    // A separate parent handle detects actual process lifetime rather than an internal task flag.
    // 独立父句柄检测实际进程寿命，而非内部任务标志。
    let witness = tempfile::NamedTempFile::new().expect("actual child witness");
    let server = native_server(witness.path());
    let runtime = bootstrap_mcp_runtime(vec![server.clone()])
        .await
        .expect("actual host runtime");
    // Any assertion failure still requests cleanup for the original native child.
    // 任何断言失败仍会请求清理原原生子进程。
    let mut guard = McpRuntimeStartupGuard::new();
    guard.track(&runtime);
    tokio::time::timeout(RELEASE_OBSERVE, async {
        loop {
            match witness.as_file().try_lock() {
                Ok(()) => witness.as_file().unlock().expect("release parent probe"),
                Err(TryLockError::WouldBlock) => break,
                Err(error) => panic!("actual lease inspection failed: {error}"),
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("child acquired its actual lease");
    assert_eq!(
        runtime.snapshot().servers[&server.id].status,
        McpRuntimeServerStatus::Connecting
    );
    runtime.shutdown();
    // Cancelled initialization requests immediate native stop, so no minimum cleanup duration is assumed.
    // 取消初始化请求立即原生停止，因此不假设最短清理耗时。
    runtime
        .wait_until_stopped(RELEASE_OBSERVE)
        .await
        .expect("actual child and pipes fully released");
    assert!(runtime.is_fully_stopped());
    witness
        .as_file()
        .try_lock()
        .expect("actual child lease released before stopped");
    assert!(
        runtime
            .inner
            .resource_releases
            .read()
            .expect("release map")
            .is_empty()
    );
}

/// Duplicate identifiers cannot create competing lifecycle writers for the same release receipt.
/// 重复标识不能为同一释放回执创建竞争生命周期写入者。
/// No inputs; rejects the complete input before either child can start.
/// 无输入；在任一子进程可能启动前拒绝整组输入。
#[tokio::test]
async fn duplicate_server_ids_are_rejected_before_start() {
    // The path is valid, but no process should ever reach it.
    // 路径有效，但任何进程均不应触达。
    let witness = tempfile::NamedTempFile::new().expect("duplicate input witness");
    let server = native_server(witness.path());
    let error = bootstrap_mcp_runtime(vec![server.clone(), server])
        .await
        .expect_err("duplicate lifecycle writer");
    assert!(error.contains("Duplicate MCP server identifier"));
}
