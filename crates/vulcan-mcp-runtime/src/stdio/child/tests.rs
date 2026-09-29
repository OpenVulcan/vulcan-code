//! Real child-held file locks distinguish OS exit from an abandoned asynchronous observer.
//! 子进程持有的真实文件锁区分操作系统退出与异步观察者放弃。

use super::*;
use std::fs::{OpenOptions, TryLockError};
use std::io::Write;
use tokio::io::{AsyncBufReadExt, BufReader};

/// Starts a real background descendant and confirms its root has actually exited.
/// 启动真实后台后代，并确认其根进程实际退出。
/// Path and mode select the descendant witness; returns the original owner and exact protocol pipes.
/// path 与 mode 选择后代见证；返回原所有者及精确协议管道。
#[cfg(windows)]
async fn exited_root_family(
    path: &std::path::Path,
    mode: &str,
) -> (ManagedChild, ChildStdin, ChildStdout) {
    // A separate root lock disappears only through OS process exit in the fixture.
    // 独立根锁在夹具中仅通过操作系统进程退出消失。
    let root_lease = tempfile::NamedTempFile::new().expect("family root witness");
    // Retain native ownership while waiting for the root's independent witness.
    // 等待根进程独立见证期间保留原生所有权。
    let owned = fixture_with_entry(
        path,
        mode,
        "stdio::child::tests::native_family_fixture",
        Some(root_lease.path()),
    )
    .await;
    wait_for_lease_release(root_lease.as_file()).await;
    owned
}

/// Graceful close eventually terminates stubborn descendants after their root has already exited.
/// 根进程已经退出后，正常关闭最终终止顽固后代。
/// No inputs; a successful receipt must immediately allow reacquiring the descendant's real lock.
/// 无输入；成功回执必须立即允许重新获取后代真实锁。
#[cfg(windows)]
#[tokio::test]
async fn native_family_close_waits_for_stubborn_descendant() {
    // The descendant holds this file for sixty seconds unless the owned job is terminated.
    // 除非所持 Job 被终止，后代会持有此文件六十秒。
    let lease = tempfile::NamedTempFile::new().expect("family close witness");
    // The helper proves the root has exited before the grace period starts.
    // 帮助函数证明宽限期开始前根进程已退出。
    let (owner, stdin, stdout) = exited_root_family(lease.path(), "hold").await;
    assert!(matches!(
        lease.as_file().try_lock(),
        Err(TryLockError::WouldBlock)
    ));
    drop((stdin, stdout));
    tokio::time::timeout(Duration::from_secs(5), owner.close())
        .await
        .expect("family close deadline")
        .expect("family close");
    lease
        .as_file()
        .try_lock()
        .expect("descendant resource released before close success");
    owner
        .close()
        .await
        .expect("same family close remains observable");
}

/// Cancelling one family-close observer cannot abandon a still-running descendant.
/// 取消一个进程族关闭观察者不能遗弃仍运行的后代。
/// No inputs; dropping the owner escalates the original worker and preserves its completion receipt.
/// 无输入；释放所有者升级原工作线程，并保留其完成回执。
#[cfg(windows)]
#[tokio::test]
async fn native_family_cancelled_observer_keeps_descendant_owned() {
    // This file is held by the descendant, never by the exited root.
    // 此文件由后代持有，绝非已退出根进程持有。
    let lease = tempfile::NamedTempFile::new().expect("family cancellation witness");
    // The original receipt must survive both timeout and owner destruction.
    // 原完成回执必须在超时及所有者销毁后继续存续。
    let (owner, stdin, stdout) = exited_root_family(lease.path(), "hold").await;
    // Cloning only the observer cannot keep the child scope running after owner destruction.
    // 仅克隆观察者不能在所有者销毁后继续保持子进程范围运行。
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
        .expect("cancelled family cleanup deadline")
        .expect("cancelled family cleanup");
    lease
        .as_file()
        .try_lock()
        .expect("descendant released before completion");
    drop((stdin, stdout));
}

/// Executor destruction still allows the native owner to terminate and confirm all job descendants.
/// 执行器销毁后，原生所有者仍能终止并确认全部 Job 后代退出。
/// No inputs; a new executor observes the original receipt and real descendant resource release.
/// 无输入；新执行器观察原回执及真实后代资源释放。
#[cfg(windows)]
#[test]
fn native_family_executor_shutdown_cleans_descendant() {
    // File ownership spans the otherwise unrelated executors.
    // 文件所有权跨越两个本来无关的执行器。
    let lease = tempfile::NamedTempFile::new().expect("family executor witness");
    // Only transport ownership is tied to this runtime; the native child scope is independent.
    // 仅传输所有权绑定此运行时；原生子进程范围独立。
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("family owner runtime");
    // The root has already exited before the task owning the descendant's scope is created.
    // 创建持有后代范围的任务前，根进程已退出。
    let (owner, stdin, stdout) = runtime.block_on(exited_root_family(lease.path(), "hold"));
    // Completion remains available after runtime teardown.
    // 运行时销毁后，完成回执仍可使用。
    let completion = owner.completion.clone();
    runtime.spawn(async move {
        // Actual ownership stays inside this future until executor shutdown drops it.
        // 实际所有权留在此 future 内，直至执行器销毁将其释放。
        let _owned = (owner, stdin, stdout);
        std::future::pending::<()>().await;
    });
    assert!(matches!(
        lease.as_file().try_lock(),
        Err(TryLockError::WouldBlock)
    ));
    drop(runtime);
    // This independent observer cannot perform cleanup on behalf of the dead runtime.
    // 此独立观察者不能替已销毁运行时执行清理。
    let observer = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("family observer runtime");
    observer.block_on(async {
        tokio::time::timeout(Duration::from_secs(5), completion.wait())
            .await
            .expect("family executor cleanup deadline")
            .expect("family executor cleanup");
    });
    lease
        .as_file()
        .try_lock()
        .expect("descendant resource released after executor loss");
}

/// Waits for exclusive reacquisition of the supplied real file lock, retaining it on success.
/// 等待重新排他获取所给真实文件锁，成功后继续持有。
/// File belongs to the parent test; a bounded deadline makes failed child cleanup observable.
/// file 属于父测试；有界期限使子进程清理失败可观察。
#[cfg(windows)]
async fn wait_for_lease_release(file: &std::fs::File) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match file.try_lock() {
                Ok(()) => break,
                Err(TryLockError::WouldBlock) => {
                    tokio::time::sleep(Duration::from_millis(10)).await
                }
                Err(error) => panic!("lease query failed: {error}"),
            }
        }
    })
    .await
    .expect("process released its actual lease");
}

/// Leaves a real descendant holding the lease after this fixture's root process exits.
/// 此夹具根进程退出后，留下持有租约的真实后代。
/// Private mode and path are inherited by the descendant; readiness is forwarded only after its lock exists.
/// 后代继承私有模式及路径；只在其锁存在后转发就绪。
#[cfg(windows)]
#[test]
#[ignore = "Child-only launcher invoked by the Windows process-family regression."]
fn native_family_fixture() {
    // This non-inheritable file handle is closed only by OS process exit, not by Rust unwinding.
    // 此不可继承文件句柄仅由操作系统进程退出关闭，不经 Rust 栈展开。
    let root_lease = OpenOptions::new()
        .read(true)
        .write(true)
        .open(std::env::var_os("VULCAN_MCP_ROOT_LEASE").expect("root lease path"))
        .expect("open root lease");
    root_lease.lock().expect("root lease lock");
    // The descendant uses its own stdio, but root exit is witnessed by the separate lock.
    // 后代使用自己的标准输入输出，但根进程退出由独立锁见证。
    let mut command = vulcan_host_tools::process_creation::new_background_std_command(
        std::env::current_exe().expect("current test executable"),
    );
    command
        .args([
            "--exact",
            "stdio::child::tests::native_child_fixture",
            "--ignored",
            "--nocapture",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    // Root exit deliberately leaves the descendant alive for the supervisor to contain.
    // 根进程退出刻意保留后代存活，由监督器纳管。
    let mut descendant = command.spawn().expect("spawn real descendant");
    // The inherited private lease is held before the root advertises readiness and exits.
    // 根进程通告就绪及退出前，继承的私有租约已经持有。
    let mut stderr = std::io::BufReader::new(descendant.stderr.take().expect("descendant stderr"));
    // Only this marker is forwarded; the child's test output never enters the protocol pipe.
    // 仅转发此标记；子进程测试输出绝不进入协议管道。
    let mut ready = String::new();
    std::io::BufRead::read_line(&mut stderr, &mut ready).expect("descendant ready marker");
    assert_eq!(ready, "mcp-native-child-ready\n");
    std::io::stderr()
        .write_all(ready.as_bytes())
        .expect("forward descendant ready");
    // Skip Rust destructors so reacquiring the root lease proves actual process exit.
    // 跳过 Rust 析构，使重新获取根租约证明实际进程退出。
    std::process::exit(0);
}

/// Root exit cannot certify release while a real descendant still owns an OS file lock.
/// 真实后代仍持有操作系统文件锁时，根进程退出不能证明资源释放。
/// No inputs; also waits out the bounded pre-fix descendant so a failing regression cannot leak it.
/// 无输入；也等待有界修复前后代结束，避免失败回归遗留它。
#[cfg(windows)]
#[tokio::test]
async fn native_family_root_exit_keeps_completion_pending() {
    // The short-lived fixture bounds cleanup even against the previous root-only implementation.
    // 短生命周期夹具使旧版仅根进程实现下的清理也有界。
    let lease = tempfile::NamedTempFile::new().expect("descendant lease file");
    // The root witness is held until process::exit closes its actual OS handle.
    // 根进程见证持有至 process::exit 关闭其实际操作系统句柄。
    let root_lease = tempfile::NamedTempFile::new().expect("root lease file");
    // Exact alternate entry spawns the real descendant before allowing its root to exit.
    // 精确备用测试入口在允许根进程退出前启动真实后代。
    let (owner, stdin, stdout) = fixture_with_entry(
        lease.path(),
        "short",
        "stdio::child::tests::native_family_fixture",
        Some(root_lease.path()),
    )
    .await;
    drop(stdin);
    // An inherited pipe can outlive the root; the root's independent file lock is the exit witness.
    // 可继承管道能比根进程存活更久；根进程独立文件锁才是退出见证。
    wait_for_lease_release(root_lease.as_file()).await;
    drop(stdout);
    assert!(matches!(
        lease.as_file().try_lock(),
        Err(TryLockError::WouldBlock)
    ));
    // Observe the result before dropping the owner, which would request immediate family termination.
    // 释放所有者前观察结果；释放会请求立即终止进程族。
    let premature = tokio::time::timeout(Duration::from_millis(150), owner.completion.wait()).await;
    // Preserve the original receipt beyond owner destruction.
    // 在所有者销毁后保留原完成回执。
    let completion = owner.completion.clone();
    drop(owner);
    tokio::time::timeout(Duration::from_secs(5), completion.wait())
        .await
        .expect("family cleanup deadline")
        .expect("family exit");
    // The pre-fix implementation may have returned early; wait for the bounded fixture before asserting.
    // 修复前实现可能提前返回；断言前等待有界夹具结束。
    wait_for_lease_release(lease.as_file()).await;
    assert!(
        premature.is_err(),
        "root exit was incorrectly reported as complete family exit"
    );
}

/// Starts the selected private fixture and returns the real owner and protocol pipes after readiness.
/// 启动选定私有夹具，在就绪后返回真实所有者及协议管道。
/// Path and mode belong to this test; entry is the exact harness test name, never a production fallback.
/// path 和 mode 属于本测试；entry 是精确测试入口名，绝不是生产回退。
/// Optional root_lease supplies the separate OS-exit witness only for the process-family fixture.
/// 可选 root_lease 仅为进程族夹具提供独立操作系统退出见证。
async fn fixture_with_entry(
    path: &std::path::Path,
    mode: &str,
    entry: &str,
    root_lease: Option<&std::path::Path>,
) -> (ManagedChild, ChildStdin, ChildStdout) {
    // Use the current test executable instead of relying on a developer-installed scripting runtime.
    // 使用当前测试可执行文件，不依赖开发机安装的脚本运行时。
    let mut command = vulcan_host_tools::process_creation::new_background_tokio_command(
        std::env::current_exe().expect("current test executable"),
    );
    command
        .args(["--exact", entry, "--ignored", "--nocapture"])
        .env("VULCAN_MCP_CHILD_LEASE", path)
        .env("VULCAN_MCP_CHILD_MODE", mode);
    if let Some(root_lease) = root_lease {
        command.env("VULCAN_MCP_ROOT_LEASE", root_lease);
    }
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
    // Fixture modes keep the pre-fix regression bounded without guessing a descendant PID.
    // 夹具模式使修复前回归有界，无需猜测后代 PID。
    let mode = std::env::var("VULCAN_MCP_CHILD_MODE").expect("private mode");
    if mode == "eof" {
        std::io::copy(&mut std::io::stdin(), &mut std::io::sink())
            .expect("wait for protocol input EOF");
    } else if mode == "short" {
        std::thread::sleep(Duration::from_secs(3));
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
    fixture_with_entry(
        path,
        mode,
        "stdio::child::tests::native_child_fixture",
        None,
    )
    .await
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
