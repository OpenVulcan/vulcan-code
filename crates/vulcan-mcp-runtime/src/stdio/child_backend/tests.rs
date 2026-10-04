//! Real child protocol pipes verify close while reads or official framed sends remain pending.
//! 真实子进程协议管道验证读取或官方分帧发送仍待完成时的关闭。

use super::*;
use crate::stdio::spawn_sanitized_stderr_capture;
use rmcp::model::{CallToolRequest, CallToolRequestParams, ClientRequest};
use std::{
    collections::VecDeque,
    fs::TryLockError,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

/// Starts the existing actual child fixture and waits for its original diagnostic readiness.
/// 启动既有实际子进程夹具，并等待其原诊断就绪信号。
/// Path identifies its real file lock and mode selects EOF or stubborn behavior; returns the production backend.
/// path 指定真实文件锁，mode 选择结束或顽固行为；返回生产后端。
async fn ready_backend(path: &Path, mode: &str) -> ManagedChildBackend {
    // Exact fixture identity reuses the existing supervised process test entry.
    // 精确夹具身份复用既有受监督进程测试入口。
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
    // Readiness is emitted only after the real child owns its OS lock.
    // 仅在真实子进程持有操作系统锁后才输出就绪信号。
    let logs = Arc::new(Mutex::new(VecDeque::new()));
    // The native diagnostic worker receives this log reference.
    // 原生诊断工作线程接收此日志引用。
    let capture = Arc::clone(&logs);
    // The production constructor owns all actual process and pipe resources before readiness.
    // 生产构造函数在就绪之前拥有全部实际进程及管道资源。
    let backend = ManagedChildBackend::spawn(command, move |stderr| {
        spawn_sanitized_stderr_capture(stderr, Vec::new(), capture)
    })
    .await
    .expect("real protocol backend");
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if logs
                .lock()
                .expect("diagnostic log lock")
                .iter()
                .any(|line| line.contains("mcp-native-child-ready"))
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("real child readiness deadline");
    backend
}

/// Closing after a cancelled receive waits for the original protocol pipe release receipts.
/// 接收取消后的关闭等待原协议管道释放回执。
/// No inputs; actual child EOF and repeated close must release all original resources.
/// 无输入；实际子进程结束及重复关闭必须释放全部原资源。
#[tokio::test]
async fn cancelled_receive_closes_actual_child_and_all_pipes() {
    // Child liveness is witnessed by a real OS file lock rather than protocol task status.
    // 子进程存活由真实操作系统文件锁见证，而非协议任务状态。
    let lease = tempfile::NamedTempFile::new().expect("protocol read lease");
    // This fixture exits when the original protocol input handle is closed.
    // 原协议输入句柄关闭时，此夹具退出。
    let mut backend = ready_backend(lease.path(), "eof").await;
    // These are original receipts, independent of backend lifetime and close observation.
    // 这些是原回执，独立于后端生命周期及关闭观察。
    let receipts = [
        backend.stdin.clone(),
        backend.stdout.clone(),
        backend.stderr.clone(),
    ];
    assert!(
        tokio::time::timeout(Duration::from_millis(30), backend.receive())
            .await
            .is_err()
    );
    assert!(matches!(
        lease.as_file().try_lock(),
        Err(TryLockError::WouldBlock)
    ));
    tokio::time::timeout(Duration::from_secs(5), backend.close())
        .await
        .expect("protocol close deadline")
        .expect("actual pipe close");
    backend.close().await.expect("repeat close");
    for receipt in receipts {
        receipt
            .wait()
            .await
            .expect("original native release receipt");
    }
    lease
        .as_file()
        .try_lock()
        .expect("actual child resource released");
}

/// A blocked framed send cannot prevent native termination and its real write failure remains observable.
/// 阻塞分帧发送不能阻止原生终止，且真实写入失败保持可观察。
/// No inputs; a stubborn child never reads stdin, so the original send and close share one real failure.
/// 无输入；顽固子进程从不读取标准输入，原发送及关闭因而共享同一真实失败。
#[tokio::test]
async fn blocked_official_send_cannot_prevent_actual_pipe_cleanup() {
    // The child keeps this actual file locked throughout its non-reading lifetime.
    // 子进程在不读取期间一直锁定此实际文件。
    let lease = tempfile::NamedTempFile::new().expect("blocked write lease");
    // The fixture will require native termination because it never consumes protocol input.
    // 夹具从不消费协议输入，因此需要原生终止。
    let mut backend = ready_backend(lease.path(), "hold").await;
    // A large ordinary tool argument puts pressure on the real OS pipe, without a substitute codec.
    // 普通工具的大参数对真实操作系统管道施加压力，不使用替代编解码器。
    let params = CallToolRequestParams::new("blocked-native-write").with_arguments(
        serde_json::Map::from_iter([(
            String::from("text"),
            serde_json::Value::String("x".repeat(4 * 1024 * 1024)),
        )]),
    );
    // The exact official request shape exercises the production framed sender.
    // 精确官方请求形状验证生产分帧发送方。
    let message = TxJsonRpcMessage::<RoleClient>::request(
        ClientRequest::CallToolRequest(CallToolRequest::new(params)),
        rmcp::model::RequestId::Number(1),
    );
    // The original send owns the official writer mutex until native output completes or fails.
    // 原发送持有官方写入互斥锁，直至原生输出完成或失败。
    let send = backend.send(message);
    tokio::pin!(send);
    assert!(
        tokio::time::timeout(Duration::from_millis(30), send.as_mut())
            .await
            .is_err()
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(10), backend.close())
            .await
            .is_err()
    );
    assert!(matches!(
        lease.as_file().try_lock(),
        Err(TryLockError::WouldBlock)
    ));
    // Native supervision must progress while close itself waits for the official sender lock.
    // 关闭自身等待官方发送锁时，原生监督必须继续推进。
    let (sent, closed) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(send.as_mut(), backend.close())
    })
    .await
    .expect("blocked output cleanup deadline");
    sent.expect_err("native pipe write must fail after child termination");
    // Retain the first cleanup diagnostic for repeated-observer comparison.
    // 保留首次清理诊断，以比较重复观察结果。
    let failure = closed
        .expect_err("close must preserve actual write failure")
        .to_string();
    assert!(
        failure.starts_with("MCP protocol pipe I/O failed:"),
        "{failure}"
    );
    assert_eq!(
        backend
            .close()
            .await
            .expect_err("retained close failure")
            .to_string(),
        failure
    );
    backend
        .stdin
        .wait()
        .await
        .expect_err("original failed write receipt");
    backend.stdout.wait().await.expect("actual output release");
    backend
        .stderr
        .wait()
        .await
        .expect("actual diagnostic release");
    lease
        .as_file()
        .try_lock()
        .expect("actual stubborn child exited");
}
