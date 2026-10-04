//! Queued operations must retain real ownership until executed or actually discarded.
//! 排队操作必须在执行或实际丢弃之前保留真实所有权。

use super::*;
use std::{future::poll_fn, pin::Pin, task::Poll};
use tokio::io::{AsyncRead, ReadBuf};

/// Dropping the full reader cancels a queued operation without invoking the native callback.
/// 释放完整读取者会取消排队操作，不调用原生回调。
/// No inputs; a real locked file and a separately occupied worker prove queue disposal.
/// 无输入；真实锁定文件及独立占用的工作线程证明队列销毁。
#[test]
fn queued_read_is_disposed_without_native_entry() {
    // The reader's exact native resource begins locked.
    // 读取者精确原生资源初始处于锁定状态。
    let (path, file) = locked_file(b"queued");
    // One worker ensures that the read stays queued behind our actual native gate.
    // 单工作线程确保读取在实际原生屏障后排队。
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .expect("queued read runtime");
    // The blocker can always be released by dropping its sender during unwinding.
    // 栈展开时释放发送方总能解除阻挡。
    let (release_worker, worker_gate) = mpsc::channel();
    // Entry confirms that the only worker is occupied before read submission.
    // 进入信号确认提交读取前唯一工作线程已被占用。
    let (worker_entered, worker_entry) = tokio::sync::oneshot::channel();
    runtime.spawn_blocking(move || {
        let _ = worker_entered.send(());
        let _ = worker_gate.recv();
    });
    runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(5), worker_entry)
            .await
            .expect("worker entry deadline")
            .expect("worker entry")
    });
    // Receiving this marker would prove that cancellation incorrectly executed the read.
    // 收到此标记将证明取消错误地执行了读取。
    let (entered, mut entry) = tokio::sync::oneshot::channel();
    // A disconnected gate makes unexpected callback entry fail instead of hanging the test.
    // 断开屏障使意外回调进入时失败，而非挂起测试。
    let (release_read, read_gate) = mpsc::channel();
    // The adapter takes the actual locked resource before its operation enters the queue.
    // 操作进入队列前，适配器取得实际锁定资源。
    let (mut reader, completion) = NativePipeReader::new(
        GatedReader {
            file,
            entered: Some(entered),
            release: read_gate,
        },
        runtime.handle().clone(),
    );
    runtime.block_on(poll_fn(|cx| {
        // A single actual poll queues the read while the sole worker remains occupied.
        // 唯一工作线程仍被占用时，一次实际轮询使读取进入队列。
        let mut bytes = [0_u8; 8];
        // This temporary async view cannot own the queued native read buffer.
        // 此临时异步视图不能拥有排队原生读取缓冲。
        let mut destination = ReadBuf::new(&mut bytes);
        assert!(
            Pin::new(&mut reader)
                .poll_read(cx, &mut destination)
                .is_pending()
        );
        Poll::Ready(())
    }));
    assert!(matches!(
        path.as_file().try_lock(),
        Err(TryLockError::WouldBlock)
    ));
    drop(reader);
    drop(release_read);
    runtime.shutdown_timeout(Duration::from_millis(10));
    release_worker.send(()).expect("release occupied worker");
    // Another executor observes actual disposal, without polling the discarded operation.
    // 另一执行器观察实际销毁，不轮询已丢弃操作。
    let observer = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("queued read observer");
    observer.block_on(async {
        tokio::time::timeout(Duration::from_secs(5), completion.wait())
            .await
            .expect("queue disposal deadline")
            .expect("cancelled unread resource release")
    });
    assert!(matches!(
        entry.try_recv(),
        Err(tokio::sync::oneshot::error::TryRecvError::Closed)
    ));
    path.as_file()
        .try_lock()
        .expect("queued reader actually released");
}

/// Dropped accepted writes must not report success when their executor rejects the queued work.
/// 当执行器拒绝排队工作时，已接收且被丢弃的写入不得报告成功。
/// No inputs; a stopped real executor rejects execution and the real file remains empty.
/// 无输入；真实已停止执行器拒绝执行，真实文件保持为空。
#[test]
fn discarded_unobserved_write_retains_failure_receipt() {
    // The write owns a real locked file, even though its executor has already stopped.
    // 即使执行器已停止，写入仍拥有真实锁定文件。
    let (path, file) = locked_file(&[]);
    // This originating executor is deliberately stopped before submission.
    // 此发起执行器刻意在提交前停止。
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("stopped write runtime");
    // Handles remain callable after shutdown but cannot run new blocking operations.
    // 关闭后句柄仍可调用，但无法运行新阻塞操作。
    let handle = runtime.handle().clone();
    runtime.shutdown_timeout(Duration::from_millis(10));
    // The actual adapter still owns the original file despite the stopped execution handle.
    // 即使执行句柄已停止，实际适配器仍拥有原文件。
    let (mut writer, completion) = NativePipeWriter::new(file, handle);
    // The observer only polls acceptance, then disappears before observing the join failure.
    // 观察者仅轮询接收，然后在观察等待失败前消失。
    let observer = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("discarded write observer");
    observer.block_on(async {
        writer
            .write_all(b"unexecuted")
            .await
            .expect("buffered acceptance")
    });
    drop(writer);
    observer.block_on(async {
        completion
            .wait()
            .await
            .expect_err("discarded accepted write cannot report successful delivery")
    });
    path.as_file()
        .try_lock()
        .expect("discarded writer actually released");
    path.as_file().unlock().expect("release observation lock");
    assert!(
        std::fs::read(path.path())
            .expect("actual empty output")
            .is_empty()
    );
}
