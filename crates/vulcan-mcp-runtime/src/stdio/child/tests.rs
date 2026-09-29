//! Real child-held file locks distinguish OS exit from an abandoned asynchronous observer.
//! 子进程持有的真实文件锁区分操作系统退出与异步观察者放弃。

use super::*;
use std::fs::{OpenOptions, TryLockError};
use std::io::Write;
use tokio::io::{AsyncBufReadExt, BufReader};

/// The test harness subprocess acquires a real exclusive lease and exposes a ready marker.
/// 测试框架子进程获取真实排他租约并输出就绪标记。
/// Private environment variables select the fixture only; no user configuration is changed.
/// 私有环境变量仅选择夹具；不修改用户配置。
#[test]
#[ignore = "Child-only entry point launched by native supervisor tests."]
fn native_child_fixture() {
    // The exact file path is passed only to this fixture process.
    // 精确文件路径仅传递给此夹具进程。
    let path = std::env::var_os("VULCAN_MCP_CHILD_LEASE").expect("private lease path");
    // This handle proves the process retains an actual operating-system resource.
    // 此句柄证明进程保留实际操作系统资源。
    let lease = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .expect("open child lease");
    lease.lock().expect("lock child lease");
    std::io::stderr()
        .write_all(b"mcp-native-child-ready\n")
        .expect("announce ready");
    if std::env::var("VULCAN_MCP_CHILD_MODE").expect("private mode") == "eof" {
        std::io::copy(&mut std::io::stdin(), &mut std::io::sink())
            .expect("wait for protocol input EOF");
    } else {
        std::thread::sleep(Duration::from_secs(60));
    }
    drop(lease);
}

/// Starts the real fixture and waits for its lease-acquired marker with a bounded observer.
/// 启动真实夹具，并以有界观察等待其已获取租约标记。
/// Path names one private test file and mode selects normal or stubborn exit; returns exact ownership.
/// path 指定单个私有测试文件，mode 选择正常或顽固退出；返回精确所有权。
async fn fixture(path: &std::path::Path, mode: &str) -> (ManagedChild, ChildStdin, ChildStdout) {
    // Use the current test executable instead of relying on a developer-installed scripting runtime.
    // 使用当前测试可执行文件，不依赖开发机安装的脚本运行时。
    let mut command = vulcan_host_tools::process_creation::new_background_tokio_command(
        std::env::current_exe().expect("current test executable"),
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
    // The native owner must already exist when the ready observer begins reading.
    // 就绪观察者开始读取时，原生所有者必须已存在。
    let (owner, pipes) = ManagedChild::spawn(command)
        .await
        .expect("spawn native fixture");
    // Convert only the diagnostic pipe for a bounded ready read; remaining handles stay exact.
    // 仅转换诊断管道以有界读取就绪；其余句柄保持精确。
    let ChildPipes {
        stdin,
        stdout,
        stderr,
    } = pipes;
    // A line stream cannot consume any protocol output from the test harness.
    // 行流不会消费测试框架的任何协议输出。
    let mut stderr =
        BufReader::new(tokio::process::ChildStderr::from_std(stderr).expect("stderr adapter"));
    // This marker is emitted only after the child's OS file lock has been acquired.
    // 此标记仅在子进程操作系统文件锁已获取后输出。
    let mut ready = String::new();
    tokio::time::timeout(Duration::from_secs(5), stderr.read_line(&mut ready))
        .await
        .expect("child ready deadline")
        .expect("read child ready");
    assert_eq!(ready, "mcp-native-child-ready\n");
    // No further stderr reads are started after the marker, so the adapter can be released here.
    // 标记后不启动额外标准错误读取，因此可在此释放适配器。
    drop(stderr);
    (owner, stdin, stdout)
}

/// Confirms a real child-held lock survives until protocol EOF, then all observers share exit evidence.
/// 确认真实子进程锁存续至协议结束，随后全部观察者共享退出证据。
/// No inputs; verifies the actual file can be exclusively reacquired after child exit.
/// 无输入；验证子进程退出后可重新排他获取实际文件。
#[tokio::test]
async fn native_child_close_reaps_before_reporting_success() {
    // Temporary file ownership is independent from the child process supervisor.
    // 临时文件所有权独立于子进程监督器。
    let lease = tempfile::NamedTempFile::new().expect("lease file");
    // The fixture waits for input EOF while owning the exclusive file lock.
    // 夹具拥有排他文件锁并等待输入结束。
    let (owner, stdin, stdout) = fixture(lease.path(), "eof").await;
    assert!(matches!(
        lease.as_file().try_lock(),
        Err(TryLockError::WouldBlock)
    ));
    // Completion clones cannot release or cancel the actual child owner.
    // 完成回执克隆不能释放或取消实际子进程所有者。
    let second = owner.completion.clone();
    drop(stdin);
    drop(stdout);
    tokio::time::timeout(Duration::from_secs(5), owner.close())
        .await
        .expect("exit deadline")
        .expect("normal child exit");
    second.wait().await.expect("same retained completion");
    owner.close().await.expect("repeated close is idempotent");
    lease
        .as_file()
        .try_lock()
        .expect("OS file lock released before successful receipt");
}

/// Cancelling close observation leaves the original child alive until the owner explicitly exits.
/// 取消关闭观察会保留原子进程，直至所有者明确退出。
/// No inputs; drop escalates the same native worker without relying on another async cleanup task.
/// 无输入；释放升级相同原生工作线程，不依赖另一个异步清理任务。
#[tokio::test]
async fn native_child_observer_cancellation_preserves_cleanup() {
    // A stubborn fixture makes the graceful period genuinely observable.
    // 顽固夹具使宽限期真实可观察。
    let lease = tempfile::NamedTempFile::new().expect("cancel lease file");
    // Keep both protocol pipes owned while checking the child resource.
    // 检查子进程资源时继续持有两个协议管道。
    let (owner, stdin, stdout) = fixture(lease.path(), "hold").await;
    // The independent receipt remains usable after the requesting handle is destroyed.
    // 请求句柄销毁后，独立回执仍然可用。
    let completion = owner.completion.clone();
    assert!(
        tokio::time::timeout(Duration::from_millis(10), owner.close())
            .await
            .is_err()
    );
    assert!(matches!(
        lease.as_file().try_lock(),
        Err(TryLockError::WouldBlock)
    ));
    drop(owner);
    tokio::time::timeout(Duration::from_secs(5), completion.wait())
        .await
        .expect("drop cleanup deadline")
        .expect("actual direct child exit");
    lease
        .as_file()
        .try_lock()
        .expect("native cleanup released actual file lock");
    drop((stdin, stdout));
}

/// Destroying the owning executor drops its task but native process cleanup still reaches real exit.
/// 销毁所属执行器释放其任务，但原生进程清理仍到达真实退出。
/// No inputs; the second runtime observes the first runtime's original completion receipt.
/// 无输入；第二个运行时观察第一个运行时的原完成回执。
#[test]
fn native_child_survives_owner_executor_shutdown_for_cleanup() {
    // The lock witnesses OS ownership across both otherwise unrelated async runtimes.
    // 此锁见证两个无关异步运行时之间的操作系统所有权。
    let lease = tempfile::NamedTempFile::new().expect("executor lease file");
    // The first runtime owns only observation and transport lifetime, never the actual child process.
    // 第一个运行时仅拥有观察及传输生命周期，绝不拥有实际子进程。
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("owner runtime");
    // The child is already executing and holding its real file before the runtime is destroyed.
    // 运行时销毁前，子进程已执行并持有真实文件。
    let (owner, stdin, stdout) = runtime.block_on(fixture(lease.path(), "hold"));
    assert!(matches!(
        lease.as_file().try_lock(),
        Err(TryLockError::WouldBlock)
    ));
    // Copy only the completion channel, so it cannot prolong transport ownership.
    // 仅复制完成通道，使其不能延长传输所有权。
    let completion = owner.completion.clone();
    runtime.spawn(async move {
        // The runtime task owns every foreground process handle and is aborted by runtime destruction.
        // 运行时任务拥有全部前台进程句柄，并由运行时销毁中止。
        let _foreground = (owner, stdin, stdout);
        futures::future::pending::<()>().await;
    });
    runtime.block_on(tokio::task::yield_now());
    drop(runtime);
    // A new observer must not restart the old child or its cleanup operation.
    // 新观察者不得重新启动旧子进程或其清理操作。
    let observer = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("observer runtime");
    observer.block_on(async {
        tokio::time::timeout(Duration::from_secs(5), completion.wait())
            .await
            .expect("executor loss cleanup deadline")
            .expect("native exit after runtime loss");
    });
    lease
        .as_file()
        .try_lock()
        .expect("OS lock released after owning runtime was destroyed");
}

/// A missing executable returns the original spawn failure without producing a phantom child owner.
/// 缺失可执行文件返回原启动失败，不产生虚假子进程所有者。
/// No inputs; the private directory guarantees that the requested executable does not exist.
/// 无输入；私有目录保证所请求可执行文件不存在。
#[tokio::test]
async fn native_child_spawn_failure_is_explicit() {
    // A fresh temporary root prevents accidental resolution to a real executable.
    // 全新临时根防止意外解析到真实可执行文件。
    let root = tempfile::tempdir().expect("missing executable fixture");
    // Preserve normal command configuration; only the executable itself is deliberately absent.
    // 保留普通命令配置；仅有意使可执行文件本身缺失。
    let command = Command::new(root.path().join("missing-program"));
    assert!(
        matches!(ManagedChild::spawn(command).await, Err(error) if error.kind() == io::ErrorKind::NotFound)
    );
}
