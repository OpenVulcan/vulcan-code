//! Actual OS access failures must survive repeated polls and final resource disposal.
//! 实际操作系统访问失败必须在重复轮询及最终资源销毁后保留。

use super::*;

/// Reading an actual write-only file reports the native error repeatedly and releases its lock last.
/// 读取实际只写文件会重复报告原生错误，并在最后释放其锁。
/// No inputs; the OS, rather than a synthetic reader, produces the failure.
/// 无输入；由操作系统产生失败，而非合成读取者。
#[tokio::test]
async fn actual_read_error_remains_failure_after_release() {
    // The parent retains only a separate observation handle.
    // 父测试仅保留独立观察句柄。
    let path = tempfile::NamedTempFile::new().expect("read failure file");
    // This handle lacks read access and owns the actual exclusive lock.
    // 此句柄缺少读取权限，拥有实际排他锁。
    let file = OpenOptions::new()
        .write(true)
        .open(path.path())
        .expect("write-only handle");
    file.lock().expect("write-only lock");
    // The production adapter consumes the failing native resource.
    // 生产适配器消费会失败的原生资源。
    let (mut reader, completion) = NativePipeReader::new(file, tokio::runtime::Handle::current());
    // A nonempty destination forces a real OS read attempt.
    // 非空目标强制实际尝试操作系统读取。
    let mut bytes = [0_u8; 8];
    // The first failure supplies the classification and diagnostic for repeat comparisons.
    // 首次失败提供用于重复比较的分类及诊断。
    let first = reader
        .read(&mut bytes)
        .await
        .expect_err("actual read must fail");
    // Retrying an observer must not turn the original failure into EOF.
    // 重试观察不得把原失败变为 EOF。
    let repeated = reader
        .read(&mut bytes)
        .await
        .expect_err("repeat read must fail");
    assert_eq!(first.kind(), repeated.kind());
    assert_eq!(first.to_string(), repeated.to_string());
    assert!(matches!(
        path.as_file().try_lock(),
        Err(TryLockError::WouldBlock)
    ));
    drop(reader);
    // Final disposal retains the original operation failure instead of reporting clean EOF.
    // 最终销毁保留原操作失败，不报告正常结束。
    let release = completion
        .wait()
        .await
        .expect_err("failed read release receipt");
    assert!(release.to_string().contains(&first.to_string()));
    assert_eq!(
        completion
            .wait()
            .await
            .expect_err("retained receipt")
            .to_string(),
        release.to_string()
    );
    path.as_file()
        .try_lock()
        .expect("failed reader actually released");
}

/// A buffered write to a read-only real file fails at flush and remains failed after cleanup.
/// 对实际只读文件的缓冲写入在刷新时失败，并在清理后保持失败。
/// No inputs; actual lock release proves that failure reporting does not substitute for disposal.
/// 无输入；实际锁释放证明错误报告不能代替销毁。
#[tokio::test]
async fn actual_write_error_remains_failure_after_release() {
    // A shared lock is valid for this read-only handle on every supported platform.
    // 此只读句柄的共享锁在全部支持平台上均有效。
    let path = tempfile::NamedTempFile::new().expect("write failure file");
    // The original resource deliberately lacks write access.
    // 原资源刻意缺少写入权限。
    let file = File::open(path.path()).expect("read-only handle");
    file.lock_shared().expect("read-only shared lock");
    // A successful buffered acceptance is not yet proof of native delivery.
    // 成功接收缓冲数据尚不是原生交付证明。
    let (mut writer, completion) = NativePipeWriter::new(file, tokio::runtime::Handle::current());
    writer
        .write_all(b"must fail")
        .await
        .expect("buffered acceptance");
    // Flush observes the actual OS failure from the retained write.
    // 刷新观察保留写入产生的实际操作系统失败。
    let first = writer.flush().await.expect_err("native write must fail");
    // Repeated shutdown cannot erase an earlier failed write.
    // 重复关闭不能擦除此前写入失败。
    let repeated = writer.shutdown().await.expect_err("failed shutdown");
    assert_eq!(first.kind(), repeated.kind());
    assert_eq!(first.to_string(), repeated.to_string());
    assert!(matches!(
        path.as_file().try_lock(),
        Err(TryLockError::WouldBlock)
    ));
    drop(writer);
    // Completion still reports failure even after the exact file is gone.
    // 即使精确文件已经释放，完成仍报告失败。
    let release = completion
        .wait()
        .await
        .expect_err("failed write release receipt");
    assert!(release.to_string().contains(&first.to_string()));
    path.as_file()
        .try_lock()
        .expect("failed writer actually released");
}
