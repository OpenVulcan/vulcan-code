//! Actual pipe and file handles verify diagnostic ownership through cancellation and executor loss.
//! 实际管道及文件句柄验证取消与执行器丢失期间的诊断所有权。

use super::child::{ChildPipes, ManagedChild};
use super::*;
use rmcp::transport::Transport;
use std::fs::{OpenOptions, TryLockError};
use std::io::Write;
use std::time::Duration;

/// A real diagnostic read failure reaches backend close after both process and input handle release.
/// 实际诊断读取失败在进程及输入句柄都释放后到达后端关闭。
/// No inputs; a write-only file chained after the real stderr pipe triggers an actual OS error.
/// 无输入；真实 stderr 管道后串接只写文件，触发实际操作系统错误。
#[tokio::test]
async fn backend_close_preserves_native_stderr_failure() {
    // This lease witnesses the real child process lifetime.
    // 此租约见证实际子进程生命周期。
    let child_lease = tempfile::NamedTempFile::new().expect("backend child lease");
    // This second lease witnesses the native reader's resource, independently of child exit.
    // 此第二租约独立于子进程退出，见证原生读取者资源。
    let read_lease = tempfile::NamedTempFile::new().expect("backend read lease");
    // Actual OS permissions produce the failure after normal pipe EOF.
    // 实际操作系统权限在管道正常结束后产生失败。
    let write_only = OpenOptions::new()
        .write(true)
        .open(read_lease.path())
        .expect("write-only reader");
    write_only.lock().expect("read resource lease");
    // This child exits when backend close removes its stdin writer.
    // 后端关闭移除标准输入写入端时，此子进程退出。
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
        .env("VULCAN_MCP_CHILD_LEASE", child_lease.path())
        .env("VULCAN_MCP_CHILD_MODE", "eof");
    // The original child pipe and failing file are both consumed by the production native reader.
    // 原子进程管道及失败文件都由生产原生读取者消费。
    let mut backend = ManagedChildBackend::spawn(command, move |stderr| {
        spawn_sanitized_stderr_capture(
            stderr.chain(write_only),
            Vec::new(),
            Arc::new(Mutex::new(VecDeque::new())),
        )
    })
    .await
    .expect("backend with native error source");
    // Close must preserve the diagnostic failure rather than using process success as its result.
    // 关闭必须保留诊断失败，不能用进程成功作为结果。
    let error = tokio::time::timeout(Duration::from_secs(5), backend.close())
        .await
        .expect("backend failure deadline")
        .expect_err("native diagnostic read failed")
        .to_string();
    assert!(error.starts_with("MCP stderr read failed:"), "{error}");
    assert_eq!(
        backend
            .close()
            .await
            .expect_err("retained native error")
            .to_string(),
        error
    );
    child_lease
        .as_file()
        .try_lock()
        .expect("actual child exited");
    read_lease
        .as_file()
        .try_lock()
        .expect("actual diagnostic input released");
}

/// The private fixture's complete secret is never permitted in captured diagnostics.
/// 私有夹具的完整秘密绝不允许出现在捕获诊断中。
const FIXTURE_SECRET: &str = "native-capture-secret-79f1";
/// Readiness is published only after the child has acquired the real file lock.
/// 仅在子进程获取真实文件锁后发布就绪。
const READY_LINE: &str = "mcp-native-stderr-ready";

/// Emits diagnostics after receiving a byte, then waits for EOF and emits a final unterminated line.
/// 收到一个字节后输出诊断，再等待 EOF 并输出最终未终止行。
/// A private environment path names the child-held lease; no user environment is modified.
/// 私有环境路径指定子进程所持租约；不修改用户环境。
#[test]
#[ignore = "Child-only entry point invoked by native stderr ownership tests."]
fn native_stderr_fixture() {
    // A real lock witnesses child liveness independently of the diagnostic observer.
    // 真实锁独立于诊断观察者见证子进程存活。
    let lease = OpenOptions::new()
        .read(true)
        .write(true)
        .open(std::env::var_os("VULCAN_MCP_STDERR_LEASE").expect("stderr lease path"))
        .expect("open stderr fixture lease");
    lease.lock().expect("lock stderr fixture lease");
    writeln!(std::io::stderr(), "{READY_LINE}").expect("announce stderr readiness");
    // Trigger bytes are sent only after the parent has completed its cancellation or shutdown action.
    // 触发字节仅在父进程完成其取消或销毁操作后发送。
    let mut trigger = [0_u8; 1];
    std::io::stdin()
        .read_exact(&mut trigger)
        .expect("wait for diagnostic trigger");
    write!(std::io::stderr(), "first {}", &FIXTURE_SECRET[..8]).expect("write secret prefix");
    writeln!(std::io::stderr(), "{}", &FIXTURE_SECRET[8..]).expect("write secret suffix");
    std::io::copy(&mut std::io::stdin(), &mut std::io::sink()).expect("wait for protocol EOF");
    write!(std::io::stderr(), "tail {FIXTURE_SECRET}").expect("write unterminated tail");
    drop(lease);
}

/// Creates the exact current-executable fixture command without requiring Node or a shell.
/// 创建精确的当前可执行文件夹具命令，无需 Node 或命令壳。
/// Path identifies a private lease; returns the configured command without spawning.
/// path 指定私有租约；返回已配置命令，不执行启动。
fn fixture_command(path: &Path) -> Command {
    // The production no-window factory keeps fixture execution invisible on Windows.
    // 生产无窗口工厂使 Windows 夹具执行不可见。
    let mut command = vulcan_host_tools::process_creation::new_background_tokio_command(
        std::env::current_exe().expect("current test executable"),
    );
    command
        .args([
            "--exact",
            "stdio::stderr_tests::native_stderr_fixture",
            "--ignored",
            "--nocapture",
        ])
        .env("VULCAN_MCP_STDERR_LEASE", path);
    command
}

/// Waits for the captured ready line without conflating task scheduling with actual child readiness.
/// 等待捕获的就绪行，不把任务调度混同实际子进程就绪。
/// Logs are the real native reader output; returns after the bounded ready observation succeeds.
/// logs 是真实原生读取者输出；有界就绪观察成功后返回。
async fn wait_for_ready(logs: &Arc<Mutex<VecDeque<String>>>) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if logs
                .lock()
                .expect("diagnostic log lock")
                .iter()
                .any(|line| line.contains(READY_LINE))
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("native stderr ready deadline");
}

/// Verifies both complete and EOF-terminated diagnostics used the unchanged redaction path.
/// 验证完整诊断及 EOF 终止诊断都使用未变脱敏路径。
/// Logs hold captured output; assertions reject exposure of the original injected secret.
/// logs 持有捕获输出；断言拒绝暴露原注入秘密。
fn assert_redacted_tail(logs: &Arc<Mutex<VecDeque<String>>>) {
    // Aggregate only this private fixture's already-bounded sanitized lines.
    // 仅汇总此私有夹具已受界限约束的脱敏行。
    let output = logs
        .lock()
        .expect("diagnostic log lock")
        .iter()
        .cloned()
        .collect::<String>();
    assert!(output.contains("first [REDACTED]"), "{output}");
    assert!(output.contains("tail [REDACTED]"), "{output}");
    assert!(!output.contains(FIXTURE_SECRET), "raw secret was captured");
}

/// A cancelled diagnostic observer cannot end the actual reader or lose its final EOF line.
/// 取消诊断观察者不能结束实际读取者，也不能丢失最终 EOF 行。
/// No inputs; repeated observers see the same success after native capture and actual process exit.
/// 无输入；原生捕获及实际进程退出后，重复观察者得到相同成功。
#[tokio::test]
async fn native_stderr_observer_cancellation_preserves_capture() {
    // The parent retains a separate file handle for checking the child's actual lock.
    // 父进程保留独立文件句柄，以检查子进程实际锁。
    let lease = tempfile::NamedTempFile::new().expect("stderr lease");
    // Exact pipe ownership is transferred only to the production capture worker.
    // 精确管道所有权仅移交给生产捕获工作线程。
    let (
        owner,
        ChildPipes {
            mut stdin,
            stdout,
            stderr,
            resources: _resources,
        },
    ) = ManagedChild::spawn(fixture_command(lease.path()))
        .await
        .expect("stderr fixture");
    // The same bounded log sink is shared with every observer.
    // 每个观察者共享相同有界日志接收端。
    let logs = Arc::new(Mutex::new(VecDeque::new()));
    // The production reader owns this actual child pipe.
    // 生产读取者拥有此实际子进程管道。
    let completion =
        spawn_sanitized_stderr_capture(stderr, vec![FIXTURE_SECRET.to_string()], Arc::clone(&logs))
            .expect("native stderr owner");
    wait_for_ready(&logs).await;
    // A second observer cannot consume the first observer's terminal receipt.
    // 第二个观察者不能消费第一个观察者的终态回执。
    let second = completion.clone();
    assert!(
        tokio::time::timeout(Duration::from_millis(10), completion.wait())
            .await
            .is_err()
    );
    assert!(matches!(
        lease.as_file().try_lock(),
        Err(TryLockError::WouldBlock)
    ));
    stdin.write_all(b"x").expect("trigger diagnostics");
    drop((stdin, stdout));
    tokio::time::timeout(Duration::from_secs(5), async {
        owner.close().await.expect("actual child exit");
        completion.wait().await.expect("actual stderr release");
    })
    .await
    .expect("capture completion deadline");
    second.wait().await.expect("same completion receipt");
    completion
        .wait()
        .await
        .expect("repeated completion receipt");
    assert_redacted_tail(&logs);
    lease
        .as_file()
        .try_lock()
        .expect("actual child resource released");
}

/// Destroying the originating executor cannot abandon the native diagnostic reader.
/// 销毁发起执行器不能遗弃原生诊断读取者。
/// No inputs; the child emits its secret and final tail only after the original runtime is gone.
/// 无输入；子进程仅在原运行时消失后输出秘密及最终尾行。
#[test]
fn native_stderr_reads_after_originating_executor_shutdown() {
    // Actual child resources outlive the executor, so no task-completion shortcut can satisfy this test.
    // 实际子进程资源比执行器存续更久，不能用任务完成捷径满足此测试。
    let lease = tempfile::NamedTempFile::new().expect("executor stderr lease");
    // The runtime only launches the process and observes initial diagnostics.
    // 此运行时仅启动进程并观察初始诊断。
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("origin runtime");
    // These owners remain in this synchronous test across executor teardown.
    // 这些所有者在执行器销毁期间保留于此同步测试。
    let (
        owner,
        ChildPipes {
            mut stdin,
            stdout,
            stderr,
            resources: _resources,
        },
    ) = runtime
        .block_on(ManagedChild::spawn(fixture_command(lease.path())))
        .expect("runtime stderr fixture");
    // Captured lines are written by the native reader without needing this runtime.
    // 捕获行由原生读取者写入，无需此运行时。
    let logs = Arc::new(Mutex::new(VecDeque::new()));
    // The native reader is started while the runtime exists but is not registered with it.
    // 原生读取者在运行时存在期间启动，但不注册到该运行时。
    let completion =
        spawn_sanitized_stderr_capture(stderr, vec![FIXTURE_SECRET.to_string()], Arc::clone(&logs))
            .expect("native diagnostic owner");
    runtime.block_on(wait_for_ready(&logs));
    drop(runtime);
    stdin.write_all(b"x").expect("emit after executor shutdown");
    drop((stdin, stdout));
    // A new independent executor only observes receipts; it does not restart the reader.
    // 新独立执行器仅观察回执，不重启读取者。
    let observer = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("observer runtime");
    observer.block_on(async {
        tokio::time::timeout(Duration::from_secs(5), async {
            owner.close().await.expect("actual process exit");
            completion.wait().await.expect("actual pipe release");
        })
        .await
        .expect("post-executor capture deadline");
    });
    assert_redacted_tail(&logs);
    lease.as_file().try_lock().expect("OS lease released");
}

/// Real OS read failure is retained and does not flush an incomplete sensitive diagnostic line.
/// 保留真实操作系统读取失败，不刷新未完成的敏感诊断行。
/// No inputs; the owned write-only file's lock must be released before the error receipt is published.
/// 无输入；发布错误回执前，必须释放自有只写文件的锁。
#[tokio::test]
async fn native_stderr_read_failure_releases_handle_without_flushing_partial_line() {
    // The file provides an actual OS failure, rather than a mocked read result.
    // 此文件提供实际操作系统失败，而非模拟读取结果。
    let lease = tempfile::NamedTempFile::new().expect("read failure lease");
    // This independent write-only handle will reject a real read on every supported platform.
    // 此独立只写句柄在每个受支持平台都会拒绝实际读取。
    let write_only = OpenOptions::new()
        .write(true)
        .open(lease.path())
        .expect("write-only source");
    write_only.lock().expect("source holds actual lock");
    assert!(matches!(
        lease.as_file().try_lock(),
        Err(TryLockError::WouldBlock)
    ));
    // An initial partial secret reaches framing before the second reader produces the real OS error.
    // 初始部分秘密先进入分帧，然后第二读取者产生真实操作系统错误。
    let reader = std::io::Cursor::new(b"native-capture-sec".to_vec()).chain(write_only);
    // The failed partial line must never reach this bounded log sink.
    // 失败部分行绝不能到达此有界日志接收端。
    let logs = Arc::new(Mutex::new(VecDeque::new()));
    // Production capture logic owns the source and publishes failure only after dropping it.
    // 生产捕获逻辑拥有来源，仅在释放后发布失败。
    let completion =
        spawn_sanitized_stderr_capture(reader, vec![FIXTURE_SECRET.to_string()], Arc::clone(&logs))
            .expect("failure capture owner");
    // The native failure remains observable after its first report.
    // 原生失败在首次报告后仍可观察。
    let failure = tokio::time::timeout(Duration::from_secs(5), completion.wait())
        .await
        .expect("read failure deadline")
        .expect_err("OS read must fail")
        .to_string();
    assert!(failure.starts_with("MCP stderr read failed:"), "{failure}");
    assert_eq!(
        completion
            .wait()
            .await
            .expect_err("retained failure")
            .to_string(),
        failure
    );
    assert!(
        logs.lock().expect("log lock").is_empty(),
        "read failure exposed a partial secret"
    );
    lease
        .as_file()
        .try_lock()
        .expect("real input handle released before receipt");
}

/// A vanished publisher is an explicit error, never an implicit pipe-release success.
/// 发布者消失是明确错误，绝非隐含管道释放成功。
/// No inputs; repeated waits retain the missing-evidence outcome.
/// 无输入；重复等待保留缺少证据的结果。
#[tokio::test]
async fn missing_native_pipe_receipt_is_not_success() {
    // Closing the publication channel without a result models a worker ending unexpectedly.
    // 未发布结果便关闭通道，模拟工作线程异常结束。
    let (publisher, completion) = PipeCompletion::channel();
    drop(publisher);
    assert!(
        completion
            .wait()
            .await
            .expect_err("missing receipt")
            .to_string()
            .contains("without confirming")
    );
    assert!(completion.wait().await.is_err());
}

/// Backend close cannot succeed while a real diagnostic pipe remains held after process exit.
/// 进程退出后真实诊断管道仍被持有时，后端关闭不能成功。
/// No inputs; a native gate delays actual handle release while repeated close observers time out.
/// 无输入；原生屏障延迟实际句柄释放，期间重复关闭观察超时。
#[tokio::test]
async fn backend_close_waits_for_actual_stderr_release() {
    // The ordinary child fixture exits on stdin EOF and retains a real OS file lease until then.
    // 普通子进程夹具在标准输入结束时退出，此前保留真实操作系统文件租约。
    let lease = tempfile::NamedTempFile::new().expect("backend stderr lease");
    // The current executable provides a deterministic child without external runtime dependencies.
    // 当前可执行文件提供确定性子进程，不依赖外部运行时。
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
        .env("VULCAN_MCP_CHILD_LEASE", lease.path())
        .env("VULCAN_MCP_CHILD_MODE", "eof");
    // Only this gate permits the native test owner to release the actual stderr handle.
    // 仅此屏障允许原生测试所有者释放实际标准错误句柄。
    let (release, gate) = std::sync::mpsc::channel();
    // The drained marker proves the actual pipe reached EOF before the retained-handle assertion.
    // 已排空标记证明保留句柄断言前，实际管道已到达 EOF。
    let (drained, drained_receiver) = tokio::sync::oneshot::channel();
    // The production backend must wait on the receipt supplied by the actual pipe owner.
    // 生产后端必须等待实际管道所有者提供的回执。
    let mut backend = ManagedChildBackend::spawn(command, move |mut stderr| {
        // Publish only after dropping the same original pipe passed by the backend.
        // 仅在释放后端传入的相同原管道后发布。
        let (publisher, completion) = PipeCompletion::channel();
        std::thread::Builder::new()
            .name(String::from("mcp-stderr-test-gate"))
            .spawn(move || {
                // Fixture output is discarded; the retained pipe itself is the resource under test.
                // 丢弃夹具输出；保留的管道本身才是被测资源。
                let read = std::io::copy(&mut stderr, &mut std::io::sink()).map(|_| ());
                let _ = drained.send(());
                // A dropped gate also releases ownership, so failing tests cannot strand the native worker.
                // 屏障发送端释放也会解除持有，避免失败测试遗留原生工作线程。
                let outcome = read.and_then(|()| gate.recv().map_err(io::Error::other));
                drop(stderr);
                publisher.send_replace(Some(outcome.map_err(|error| error.to_string())));
            })?;
        Ok(completion)
    })
    .await
    .expect("backend with real diagnostic owner");
    assert!(
        tokio::time::timeout(Duration::from_millis(150), backend.close())
            .await
            .is_err()
    );
    tokio::time::timeout(Duration::from_secs(5), drained_receiver)
        .await
        .expect("real pipe EOF deadline")
        .expect("real pipe EOF");
    lease
        .as_file()
        .try_lock()
        .expect("child resource released while stderr handle remains owned");
    assert!(
        tokio::time::timeout(Duration::from_millis(10), backend.close())
            .await
            .is_err()
    );
    release.send(()).expect("allow actual stderr release");
    tokio::time::timeout(Duration::from_secs(5), backend.close())
        .await
        .expect("backend final close deadline")
        .expect("actual stderr release closes backend");
}
