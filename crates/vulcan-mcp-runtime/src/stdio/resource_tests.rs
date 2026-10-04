//! Real file locks verify that actual stdio owners retain host authority through failed initialization.
//! 真实文件锁验证实际标准输入输出所有者在初始化失败期间保留宿主权威。

use super::*;
use crate::{McpResourceRelease, McpTransportResources};
use rmcp::transport::Transport;
use std::{
    fs::{File, OpenOptions, TryLockError},
    time::Duration,
};

/// Executor destruction preserves the original public launch's actual release observation.
/// 执行器销毁保留原公开启动的实际释放观察。
/// No inputs; a second executor observes the original process and pipe owners without restarting them.
/// 无输入；第二个执行器观察原进程及管道所有者，不重新启动它们。
#[test]
fn executor_loss_releases_original_initializing_resources() {
    // Independent witnesses distinguish actual child exit from host authority release.
    // 独立见证区分实际子进程退出及宿主权威释放。
    let child_lease = tempfile::NamedTempFile::new().expect("executor child witness");
    // This original scope is moved into initialization exactly once.
    // 此原范围仅一次移入初始化。
    let (host, resources, release) = host_lease();
    // Only the path is copied; the parent's witness handle stays outside the doomed executor.
    // 仅复制路径；父观察句柄保留于即将销毁执行器之外。
    let child_path = child_lease
        .path()
        .to_str()
        .expect("UTF-8 fixture path")
        .to_owned();
    // The first runtime owns the initialization future but never owns native process cleanup.
    // 首个运行时拥有初始化 future，但绝不拥有原生进程清理。
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("initializing runtime");
    // Detaching this handle leaves cancellation to actual runtime teardown below.
    // 分离此句柄，将取消交给下方实际运行时销毁。
    let initializing = runtime.spawn(async move {
        // The existing child entry blocks initialization with exact private environment values.
        // 既有子入口使用精确私有环境值阻挡初始化。
        let env = BTreeMap::from([
            (String::from("VULCAN_MCP_CHILD_LEASE"), child_path),
            (String::from("VULCAN_MCP_CHILD_MODE"), String::from("hold")),
        ]);
        StdioTransport::spawn_with_resources(
            "resource-executor-loss",
            std::env::current_exe().expect("test executable"),
            &[
                "--exact",
                "stdio::child::tests::native_child_fixture",
                "--ignored",
                "--nocapture",
            ],
            None,
            &env,
            true,
            resources,
        )
        .await
    });
    runtime.block_on(wait_for_child_lock(child_lease.as_file()));
    assert!(matches!(
        host.as_file().try_lock(),
        Err(TryLockError::WouldBlock)
    ));
    drop(initializing);
    runtime.shutdown_timeout(Duration::from_millis(10));
    // Observation on another runtime cannot resume any task on the destroyed first runtime.
    // 在另一运行时观察，不能恢复已销毁首个运行时的任何任务。
    let observer = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("release observer runtime");
    observer.block_on(async {
        tokio::time::timeout(Duration::from_secs(5), release.wait())
            .await
            .expect("executor loss release deadline")
            .expect("original resources actually released");
    });
    child_lease
        .as_file()
        .try_lock()
        .expect("actual child exited after executor loss");
    host.as_file()
        .try_lock()
        .expect("original host lease released after executor loss");
}

/// Creates an exclusively locked host resource and transfers it into one launch scope.
/// 创建排他锁定宿主资源，并移交给单次启动范围。
/// No inputs; returns the independent OS witness, launch authority and original release observer.
/// 无输入；返回独立操作系统见证、启动权威及原释放观察者。
fn host_lease() -> (
    tempfile::NamedTempFile,
    McpTransportResources,
    McpResourceRelease,
) {
    // The independent parent handle must conflict with the actual retained lease.
    // 独立父句柄必须与实际保留租约冲突。
    let path = tempfile::NamedTempFile::new().expect("host lease path");
    // This exact file enters the opaque host bundle and has no other owning copy.
    // 此精确文件进入不透明宿主资源包，没有其他自有副本。
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path.path())
        .expect("host lease handle");
    file.lock().expect("host lease lock");
    // Only actual transport owners can delay this receipt after the root scope is transferred.
    // 根范围移交后，仅实际传输所有者能延迟此回执。
    let (resources, release) = McpTransportResources::new(file);
    (path, resources, release)
}

/// Observes the child fixture's real file lock without holding a competing parent lock.
/// 观察子进程夹具真实文件锁，不持有竞争父锁。
/// File is an independent witness; returns after the actual child has acquired its exclusive lease.
/// file 是独立见证；实际子进程取得排他租约后返回。
async fn wait_for_child_lock(file: &File) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match file.try_lock() {
                Err(TryLockError::WouldBlock) => return,
                Ok(()) => file.unlock().expect("release transient observer lock"),
                Err(error) => panic!("child lease observation failed: {error}"),
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("real child entry deadline");
}

/// Builds the exact existing fixture command with an independently witnessed lifetime.
/// 构建精确既有夹具命令，并独立见证其生命周期。
/// Path selects the child lease and mode selects declared fixture behavior; returns an unspawned command.
/// path 指定子进程租约，mode 选择明确夹具行为；返回尚未启动的命令。
fn resource_child_command(path: &Path, mode: &str) -> Command {
    // The established factory preserves invisible background startup on Windows.
    // 既有工厂保留 Windows 不可见后台启动。
    let mut command = vulcan_host_tools::process_creation::new_background_tokio_command(
        std::env::current_exe().expect("test executable"),
    );
    command
        .args([
            "--exact",
            "stdio::child::tests::native_child_fixture",
            "--ignored",
            "--nocapture",
        ])
        .env("VULCAN_MCP_CHILD_LEASE", path)
        .env("VULCAN_MCP_CHILD_MODE", mode);
    command
}

/// Native process ownership retains host leases even after every protocol pipe has been dropped.
/// 即使全部协议管道都已释放，原生进程所有权仍保留宿主租约。
/// No inputs; a stubborn real child keeps the launch bundle until actual termination.
/// 无输入；顽固真实子进程保留启动资源包，直至实际终止。
#[tokio::test]
async fn process_owner_retains_host_lease_after_pipe_disposal() {
    // Separate files distinguish child lifetime from the host's retained authority.
    // 独立文件区分子进程生命周期及宿主保留权威。
    let child_lease = tempfile::NamedTempFile::new().expect("child process witness");
    // A distinct host lease and its receipt follow only this launch's actual owners.
    // 独立宿主租约及其回执仅跟随此次启动的实际所有者。
    let (host, resources, release) = host_lease();
    // The original native supervisor receives ownership before its first spawn.
    // 原原生监督器在首次启动前接收所有权。
    let (child, pipes) = child::ManagedChild::spawn_with_resources(
        resource_child_command(child_lease.path(), "hold"),
        resources,
    )
    .await
    .expect("retained native child");
    wait_for_child_lock(child_lease.as_file()).await;
    drop(pipes);
    assert!(
        tokio::time::timeout(Duration::from_millis(10), child.close())
            .await
            .is_err()
    );
    assert!(matches!(
        host.as_file().try_lock(),
        Err(TryLockError::WouldBlock)
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(10), release.wait())
            .await
            .is_err()
    );
    drop(child);
    tokio::time::timeout(Duration::from_secs(5), release.wait())
        .await
        .expect("host release deadline")
        .expect("host release after process exit");
    child_lease
        .as_file()
        .try_lock()
        .expect("actual child released its resource");
    host.as_file()
        .try_lock()
        .expect("host authority released after actual exit");
}

/// A retained diagnostic pipe keeps host leases after process exit and cancelled close observation.
/// 进程退出及关闭观察取消后，保留的诊断管道继续持有宿主租约。
/// No inputs; a native gate blocks only final release of the actual original stderr handle.
/// 无输入；原生屏障仅阻挡实际原标准错误句柄的最终释放。
#[tokio::test]
async fn diagnostic_owner_retains_host_lease_after_child_exit() {
    // Child and host lease witnesses are independently observable through OS locking.
    // 子进程及宿主租约见证可通过操作系统锁独立观察。
    let child_lease = tempfile::NamedTempFile::new().expect("diagnostic child witness");
    // A distinct host lease and its receipt follow only this launch's actual owners.
    // 独立宿主租约及其回执仅跟随此次启动的实际所有者。
    let (host, resources, release) = host_lease();
    // Native release remains possible even if asynchronous close observation disappears.
    // 即使异步关闭观察消失，仍可原生释放。
    let (allow_release, gate) = std::sync::mpsc::channel();
    // EOF confirms the child side has closed, while the original parent handle remains held.
    // EOF 确认子端已关闭，同时原父句柄继续被持有。
    let (drained, drained_receiver) = tokio::sync::oneshot::channel();
    // The production constructor supplies the lease-bound original diagnostic pipe.
    // 生产构造函数提供绑定租约的原诊断管道。
    let mut backend = ManagedChildBackend::spawn_with_resources(
        resource_child_command(child_lease.path(), "eof"),
        resources,
        move |mut stderr| {
            // This receipt is published only after the same actual pipe and its host authority are released.
            // 仅在相同实际管道及其宿主权威释放后发布此回执。
            let (publisher, completion) = PipeCompletion::channel();
            std::thread::Builder::new()
                .name(String::from("mcp-retained-stderr-test"))
                .spawn(move || {
                    // Drain actual child output before isolating the final resource release boundary.
                    // 在单独验证最终资源释放边界前，排空实际子进程输出。
                    let outcome = std::io::copy(&mut stderr, &mut std::io::sink()).map(|_| ());
                    let _ = drained.send(());
                    // Gate disposal also releases the original pipe if this test unwinds.
                    // 测试栈展开时，屏障销毁也会释放原管道。
                    let outcome = outcome.and_then(|()| gate.recv().map_err(io::Error::other));
                    drop(stderr);
                    publisher.send_replace(Some(outcome.map_err(|error| error.to_string())));
                })?;
            Ok(completion)
        },
    )
    .await
    .expect("retained backend");
    assert!(
        tokio::time::timeout(Duration::from_millis(150), backend.close())
            .await
            .is_err()
    );
    tokio::time::timeout(Duration::from_secs(5), drained_receiver)
        .await
        .expect("pipe EOF deadline")
        .expect("pipe EOF");
    child_lease
        .as_file()
        .try_lock()
        .expect("child resource released");
    assert!(matches!(
        host.as_file().try_lock(),
        Err(TryLockError::WouldBlock)
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(10), release.wait())
            .await
            .is_err()
    );
    allow_release
        .send(())
        .expect("release actual diagnostic owner");
    tokio::time::timeout(Duration::from_secs(5), backend.close())
        .await
        .expect("backend close deadline")
        .expect("backend release");
    release.wait().await.expect("original host receipt");
    release.wait().await.expect("repeated host receipt");
    host.as_file()
        .try_lock()
        .expect("actual host lease released");
}

/// Cancelling public initialization preserves a release observer even without a connected transport.
/// 取消公开初始化后，即使没有已连接传输，也保留释放观察者。
/// No inputs; the actual child and host file leases must both be gone before release completes.
/// 无输入；释放完成前，实际子进程及宿主文件租约均须消失。
#[tokio::test]
async fn cancelled_initialize_releases_actual_host_resources() {
    // The child fixture deliberately never sends an MCP initialize response.
    // 子进程夹具刻意从不发送 MCP 初始化响应。
    let child_lease = tempfile::NamedTempFile::new().expect("initializing child witness");
    // A distinct host lease and its receipt follow only this launch's actual owners.
    // 独立宿主租约及其回执仅跟随此次启动的实际所有者。
    let (host, resources, release) = host_lease();
    // Only this exact environment reaches the declared child fixture.
    // 仅此精确环境到达明确子进程夹具。
    let env = BTreeMap::from([
        (
            String::from("VULCAN_MCP_CHILD_LEASE"),
            child_lease
                .path()
                .to_str()
                .expect("UTF-8 fixture path")
                .to_owned(),
        ),
        (String::from("VULCAN_MCP_CHILD_MODE"), String::from("hold")),
    ]);
    {
        // Scope exit cancels initialization, never the independent actual release observer.
        // 范围退出取消初始化，绝不取消独立实际释放观察。
        let initializing = StdioTransport::spawn_with_resources(
            "resource-cancellation",
            std::env::current_exe().expect("test executable"),
            &[
                "--exact",
                "stdio::child::tests::native_child_fixture",
                "--ignored",
                "--nocapture",
            ],
            None,
            &env,
            true,
            resources,
        );
        tokio::pin!(initializing);
        tokio::select! {
            result = initializing.as_mut() => panic!("initialization completed unexpectedly: {:?}", result.err()),
            () = wait_for_child_lock(child_lease.as_file()) => {}
        }
        assert!(matches!(
            host.as_file().try_lock(),
            Err(TryLockError::WouldBlock)
        ));
    }
    tokio::time::timeout(Duration::from_secs(5), release.wait())
        .await
        .expect("cancelled initialization release deadline")
        .expect("actual initialization resources released");
    child_lease
        .as_file()
        .try_lock()
        .expect("initializing child actually exited");
    host.as_file()
        .try_lock()
        .expect("original host lease released");
}

/// Failed creation releases its original host resources without claiming that a child was created.
/// 创建失败释放原宿主资源，不声称已创建子进程。
/// No inputs; a guaranteed absent executable exercises the native pre-handoff failure path.
/// 无输入；保证不存在的可执行文件验证原生移交前失败路径。
#[tokio::test]
async fn failed_spawn_releases_original_host_resources() {
    // This private root guarantees that the exact requested executable does not exist.
    // 此私有根保证精确请求的可执行文件不存在。
    let root = tempfile::tempdir().expect("absent executable root");
    // A distinct host lease and its receipt follow only this launch's actual owners.
    // 独立宿主租约及其回执仅跟随此次启动的实际所有者。
    let (host, resources, release) = host_lease();
    // The original spawn error remains distinct from the resource-release observation.
    // 原启动错误与资源释放观察保持分离。
    let result = StdioTransport::spawn_with_resources(
        "missing-resource-test",
        root.path().join("missing-mcp-executable"),
        &[],
        None,
        &BTreeMap::new(),
        true,
        resources,
    )
    .await;
    assert!(result.is_err(), "missing program unexpectedly started");
    tokio::time::timeout(Duration::from_secs(5), release.wait())
        .await
        .expect("failed spawn release deadline")
        .expect("failed spawn actual release");
    host.as_file()
        .try_lock()
        .expect("failed spawn released original lease");
}
