//! Real file locks verify that in-flight operations own resources after adapters and executors leave.
//! 真实文件锁验证适配器及执行器离开后，在途操作仍拥有资源。

use super::*;
use std::{
    fs::{File, OpenOptions, TryLockError},
    io::{Read, Write},
    sync::mpsc,
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[cfg(test)]
mod failures;
#[cfg(test)]
mod queued;

/// Gates the first actual read while retaining an exclusively locked real file.
/// 保留真实排他锁定文件，并阻挡首次实际读取。
struct GatedReader {
    /// The actual OS resource belongs to this reader until final destruction.
    /// 实际操作系统资源属于此读取者，直至最终销毁。
    file: File,
    /// One entry marker proves the operation is inside the native callback.
    /// 单次进入标记证明操作已位于原生回调内。
    entered: Option<tokio::sync::oneshot::Sender<()>>,
    /// The parent releases the operation without any Tokio timer or executor.
    /// 父测试不依赖任何 Tokio 计时器或执行器便可释放操作。
    release: mpsc::Receiver<()>,
}

impl Read for GatedReader {
    /// Blocks only the first read, then reads actual file bytes into the supplied destination.
    /// 仅阻挡首次读取，然后把实际文件字节读入给定目标。
    /// Destination receives native file bytes; returns the real read result or explicit test-gate failure.
    /// destination 接收原生文件字节；返回实际读取结果或明确测试屏障失败。
    fn read(&mut self, destination: &mut [u8]) -> io::Result<usize> {
        if let Some(entered) = self.entered.take() {
            let _ = entered.send(());
            self.release.recv().map_err(io::Error::other)?;
        }
        self.file.read(destination)
    }
}

/// Gates an actual file write while accepted bytes remain owned by the blocking operation.
/// 阻挡实际文件写入，同时使已接收字节保留于阻塞操作。
struct GatedWriter {
    /// The exclusive OS file lock witnesses native ownership.
    /// 排他操作系统文件锁见证原生所有权。
    file: File,
    /// The first write signals entry before blocking.
    /// 首次写入在阻挡之前发出进入信号。
    entered: Option<tokio::sync::oneshot::Sender<()>>,
    /// A native channel permits completion after executor shutdown.
    /// 原生通道允许在执行器销毁后完成。
    release: mpsc::Receiver<()>,
}

impl Write for GatedWriter {
    /// Waits at the first write gate and forwards the exact bytes to the actual file.
    /// 在首次写入屏障等待，并把精确字节转发给实际文件。
    /// Bytes remain borrowed only for this synchronous operation; returns the file's original result.
    /// bytes 仅在此同步操作期间借用；返回文件原结果。
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if let Some(entered) = self.entered.take() {
            let _ = entered.send(());
            self.release.recv().map_err(io::Error::other)?;
        }
        self.file.write(bytes)
    }

    /// Flushes the actual file instead of synthesizing a successful flush receipt.
    /// 刷新实际文件，不合成成功刷新回执。
    /// No inputs; returns the original native flush result.
    /// 无输入；返回原原生刷新结果。
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

/// Creates an initialized test file and a separate exclusively locked native handle at offset zero.
/// 创建已初始化测试文件及一个位于零偏移的独立排他锁定原生句柄。
/// Bytes seed the actual file; returns the path owner and the exact handle transferred into an adapter.
/// bytes 初始化实际文件；返回路径所有者及将移交给适配器的精确句柄。
fn locked_file(bytes: &[u8]) -> (tempfile::NamedTempFile, File) {
    // This parent handle does not own the child adapter's exclusive lock.
    // 此父句柄不拥有子适配器的排他锁。
    let mut path = tempfile::NamedTempFile::new().expect("native I/O file");
    path.write_all(bytes).expect("initial file bytes");
    // A distinct open description gives an independent cursor and lock owner.
    // 独立打开描述提供独立光标及锁所有者。
    let resource = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path.path())
        .expect("owned I/O handle");
    resource.lock().expect("actual I/O lock");
    (path, resource)
}

/// Begins a read, proves native entry, and cancels only the surrounding read future.
/// 开始读取，证明原生进入，并仅取消外层读取 future。
/// Reader retains its pending operation; entered is sent by the real gated native callback.
/// reader 保留待完成操作；entered 由真实受屏障控制的原生回调发送。
async fn enter_then_cancel_read(
    reader: &mut NativePipeReader<GatedReader>,
    entered: tokio::sync::oneshot::Receiver<()>,
) {
    // A larger first destination exercises preservation when the replacement read is smaller.
    // 首次较大目标用于验证替代读取变小时的保留行为。
    let mut destination = [0_u8; 128];
    tokio::time::timeout(Duration::from_secs(5), async {
        tokio::select! {
            entered = entered => entered.expect("native read entry"),
            result = reader.read(&mut destination) => panic!("read completed before release: {result:?}"),
        }
    }).await.expect("native read entry deadline");
}

/// Cancelled reads preserve bytes and keep the real file locked until the adapter itself is released.
/// 取消读取保留字节，并在适配器本身释放前保持真实文件锁定。
/// No inputs; subsequent smaller reads must reconstruct the exact original content once.
/// 无输入；后续更小读取必须仅一次重建精确原内容。
#[tokio::test]
async fn cancelled_read_preserves_buffer_and_actual_owner() {
    // Expected data exceeds the replacement destination, exposing dropped or duplicated prefixes.
    // 预期数据超过替代目标大小，可暴露丢失或重复前缀。
    let expected = b"native pipe bytes survive a cancelled read";
    // The actual reader holds the exclusive file lock.
    // 实际读取者持有排他文件锁。
    let (path, file) = locked_file(expected);
    // Native entry and release have independent channels.
    // 原生进入及释放拥有独立通道。
    let (entered, entry) = tokio::sync::oneshot::channel();
    // The gate remains owned by the test until the cancelled read is established.
    // 直至已建立取消读取，屏障一直由测试持有。
    let (release, gate) = mpsc::channel();
    // The exact file is consumed by the production adapter.
    // 精确文件由生产适配器消费。
    let (mut reader, completion) = NativePipeReader::new(
        GatedReader {
            file,
            entered: Some(entered),
            release: gate,
        },
        tokio::runtime::Handle::current(),
    );
    enter_then_cancel_read(&mut reader, entry).await;
    assert!(matches!(
        path.as_file().try_lock(),
        Err(TryLockError::WouldBlock)
    ));
    release.send(()).expect("release actual read");
    // Reading just three bytes must not discard the rest of the completed original native request.
    // 仅读取三个字节不能丢弃原已完成原生请求的剩余部分。
    let mut prefix = [0_u8; 3];
    reader.read_exact(&mut prefix).await.expect("smaller read");
    // The remaining byte stream is still owned by the same adapter.
    // 剩余字节流仍由相同适配器持有。
    let mut rest = Vec::new();
    reader
        .read_to_end(&mut rest)
        .await
        .expect("remaining exact bytes");
    assert_eq!([prefix.as_slice(), rest.as_slice()].concat(), expected);
    assert!(
        tokio::time::timeout(Duration::from_millis(10), completion.wait())
            .await
            .is_err()
    );
    drop(reader);
    completion
        .wait()
        .await
        .expect("actual read handle released");
    path.as_file()
        .try_lock()
        .expect("read lock released only with actual owner");
}

/// In-flight native reads keep real resources after both adapter destruction and runtime shutdown.
/// 在途原生读取在适配器销毁及运行时退出后仍保留真实资源。
/// No inputs; a native gate permits eventual completion without reviving the original executor.
/// 无输入；原生屏障允许最终完成，无需恢复原执行器。
#[test]
fn in_flight_read_survives_adapter_and_executor_loss() {
    // A real file lock distinguishes worker ownership from asynchronous task state.
    // 真实文件锁区分工作线程所有权与异步任务状态。
    let (path, file) = locked_file(b"still owned");
    // Limit blocking concurrency so the test cannot create an unbounded worker set.
    // 限制阻塞并发，使测试不能创建无界工作线程集合。
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .expect("read runtime");
    // The callback announces its actual entry before the adapter is dropped.
    // 适配器释放前，回调通告其实际进入。
    let (entered, entry) = tokio::sync::oneshot::channel();
    // This native gate survives destruction of the original async runtime.
    // 此原生屏障在原异步运行时销毁后存续。
    let (release, gate) = mpsc::channel();
    // The adapter consumes the original resource and returns only an independent release observer.
    // 适配器消费原资源，仅返回独立释放观察者。
    let (mut reader, completion) = NativePipeReader::new(
        GatedReader {
            file,
            entered: Some(entered),
            release: gate,
        },
        runtime.handle().clone(),
    );
    runtime.block_on(enter_then_cancel_read(&mut reader, entry));
    drop(reader);
    runtime.shutdown_timeout(Duration::from_millis(10));
    assert!(matches!(
        path.as_file().try_lock(),
        Err(TryLockError::WouldBlock)
    ));
    // A second runtime may observe cleanup but cannot perform the original native read.
    // 第二个运行时可以观察清理，但不能执行原原生读取。
    let observer = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("read observer");
    observer.block_on(async {
        assert!(
            tokio::time::timeout(Duration::from_millis(10), completion.wait())
                .await
                .is_err()
        );
    });
    release.send(()).expect("complete retained native read");
    observer.block_on(async {
        tokio::time::timeout(Duration::from_secs(5), completion.wait())
            .await
            .expect("actual read release deadline")
            .expect("actual read release");
    });
    path.as_file()
        .try_lock()
        .expect("last native owner released real lock");
}

/// Accepted writes remain owned while their flush observer and executor are destroyed.
/// 已接收写入在刷新观察者及执行器被销毁期间保持被持有。
/// No inputs; actual written bytes and OS lock release prove late native completion.
/// 无输入；实际写入字节及操作系统锁释放证明迟到原生完成。
#[test]
fn in_flight_write_survives_adapter_and_executor_loss() {
    // The file begins empty so the final contents prove the original accepted write actually happened.
    // 文件初始为空，使最终内容证明原已接收写入确实发生。
    let (path, file) = locked_file(&[]);
    // The one-worker runtime is deliberately destroyed while the writer is blocked.
    // 单工作线程运行时刻意在写入者阻塞时销毁。
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .expect("write runtime");
    // Native entry precedes observer cancellation.
    // 原生进入先于观察取消。
    let (entered, entry) = tokio::sync::oneshot::channel();
    // Native release is possible without polling a future on the lost executor.
    // 无需在已丢失执行器上轮询 future，也可原生释放。
    let (release, gate) = mpsc::channel();
    // The real write-only operation is wrapped by the production writer adapter.
    // 实际写入操作由生产写入适配器包装。
    let (mut writer, completion) = NativePipeWriter::new(
        GatedWriter {
            file,
            entered: Some(entered),
            release: gate,
        },
        runtime.handle().clone(),
    );
    runtime.block_on(async {
        writer
            .write_all(b"late native output")
            .await
            .expect("accepted bytes");
        tokio::time::timeout(Duration::from_secs(5), entry)
            .await
            .expect("native write entry deadline")
            .expect("native write entry");
        assert!(
            tokio::time::timeout(Duration::from_millis(10), writer.flush())
                .await
                .is_err()
        );
    });
    drop(writer);
    runtime.shutdown_timeout(Duration::from_millis(10));
    assert!(matches!(
        path.as_file().try_lock(),
        Err(TryLockError::WouldBlock)
    ));
    // The independent observer cannot release the worker's original file ownership.
    // 独立观察者不能释放工作线程原文件所有权。
    let observer = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("write observer");
    observer.block_on(async {
        assert!(
            tokio::time::timeout(Duration::from_millis(10), completion.wait())
                .await
                .is_err()
        );
    });
    release.send(()).expect("finish actual write");
    observer.block_on(async {
        tokio::time::timeout(Duration::from_secs(5), completion.wait())
            .await
            .expect("actual write release deadline")
            .expect("actual write release");
    });
    path.as_file().try_lock().expect("writer OS lock released");
    // Release our own verification lock before reading through a different handle on Windows.
    // 在 Windows 上经另一个句柄读取前，释放我们自己的验证锁。
    path.as_file()
        .unlock()
        .expect("release parent verification lock");
    assert_eq!(
        std::fs::read(path.path()).expect("actual output"),
        b"late native output"
    );
}

/// Large writes preserve exact ordering while shutdown releases the actual file and stays idempotent.
/// 大写入保持精确顺序，同时关闭释放实际文件并保持幂等。
/// No inputs; data spans multiple bounded operations and writes after shutdown must fail.
/// 无输入；数据跨越多个有界操作，关闭后的写入必须失败。
#[tokio::test]
async fn writer_chunks_flushes_and_releases_on_idempotent_shutdown() {
    // More than two operation capacities forces real partial acceptance and ordered continuation.
    // 超过两次操作容量，强制实际部分接收及有序续传。
    let expected = (0..PIPE_IO_CHUNK_BYTES * 2 + 17)
        .map(|index| (index % 251) as u8)
        .collect::<Vec<_>>();
    // An exclusive real file lock remains held until the writer's final shutdown.
    // 真实排他文件锁保持持有，直至写入者最终关闭。
    let (path, file) = locked_file(&[]);
    // The adapter uses the existing runtime rather than starting a permanent native thread.
    // 适配器使用既有运行时，不启动永久原生线程。
    let (mut writer, completion) = NativePipeWriter::new(file, tokio::runtime::Handle::current());
    writer
        .write_all(&expected)
        .await
        .expect("ordered bounded writes");
    writer.shutdown().await.expect("actual writer shutdown");
    writer.shutdown().await.expect("repeated shutdown");
    completion
        .wait()
        .await
        .expect("original actual release receipt");
    assert_eq!(
        writer
            .write_all(b"x")
            .await
            .expect_err("closed writer")
            .kind(),
        io::ErrorKind::BrokenPipe
    );
    path.as_file()
        .try_lock()
        .expect("shutdown released actual handle");
    // Release our own verification lock before reading through a different handle on Windows.
    // 在 Windows 上经另一个句柄读取前，释放我们自己的验证锁。
    path.as_file()
        .unlock()
        .expect("release parent verification lock");
    assert_eq!(std::fs::read(path.path()).expect("written bytes"), expected);
}
