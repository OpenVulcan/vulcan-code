//! Process-owned diagnostics with bounded synchronous retention.
//! 进程拥有的诊断日志，采用有界同步留存。

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use tracing_subscriber::prelude::*;

/// Maximum size of each retained log segment.
/// 每个日志分段的大小上限。
const SEGMENT_BYTES: u64 = 8 * 1024 * 1024;
/// Number of completed segments retained alongside the current segment.
/// 当前分段之外保留的已完成分段数量。
const ARCHIVE_COUNT: usize = 4;
/// Installed data-root identity; a process never silently switches log ownership.
/// 已安装的数据根身份，进程绝不静默切换日志归属。
static INSTALLED: OnceLock<Result<PathBuf, String>> = OnceLock::new();
/// The native consumer shares the exact tracing writer, including its rotation ownership.
/// 原生消费者共享 tracing 的精确写入器及其轮转所有权。
static SHARED_WRITER: OnceLock<LogWriter> = OnceLock::new();

/// Return the log directory for the already resolved authoritative data root.
/// 根据已解析的权威数据根返回日志目录。
pub fn directory(data_root: &Path) -> PathBuf {
    vulcan_runtime_assets::service_log_directory(data_root)
}

/// Report startup failure through the installed sink, or stderr before a sink can exist.
/// 通过已安装的接收器报告启动失败，在接收器尚不可用时使用 stderr。
pub fn startup_error(message: &str) {
    match INSTALLED.get() {
        Some(Ok(_)) => tracing::error!("{message}"),
        _ => eprintln!("{message}"),
    }
}

/// Install one process subscriber after acquiring the service instance lock; report startup failures.
/// 取得服务实例锁后安装唯一进程接收器，启动失败时返回错误。
pub fn initialize(data_root: &Path) -> Result<(), String> {
    // Preserve the first initialization result instead of overwriting another subscriber.
    // 保留首次初始化结果，避免覆盖其他接收器。
    let installed = INSTALLED.get_or_init(|| {
        install(data_root)
            .map(|()| data_root.to_path_buf())
            .map_err(|error| error.to_string())
    });
    match installed {
        Ok(root) if root == data_root => Ok(()),
        Ok(_) => Err("Service logging is already owned by another data root".into()),
        Err(error) => Err(format!("Service logging initialization failed: {error}")),
    }
}

/// Configure JSON files and console output for this root; return any file/filter/subscriber error.
/// 为指定根配置 JSON 文件和控制台输出，返回文件、过滤器或接收器错误。
fn install(data_root: &Path) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // A synchronous writer survives process::exit without requiring an async queue to drain.
    // 同步写入无需异步队列排空即可适配 process::exit 退出路径。
    let writer = LogWriter(Arc::new(Mutex::new(RollingLog::open(
        directory(data_root),
        SEGMENT_BYTES,
    )?)));
    // Only application diagnostics default to info; dependencies remain at warn unless explicitly selected.
    // 应用诊断默认 info，依赖默认 warn，除非明确配置其他过滤级别。
    let filter = match std::env::var("RUST_LOG") {
        Ok(value) => tracing_subscriber::EnvFilter::try_new(value)?,
        Err(std::env::VarError::NotPresent) => tracing_subscriber::EnvFilter::try_new(
            "warn,vulcan_code_service=info,vulcan_service_kernel=info,vulcan_agent_runtime=info,vulcan_service_runtime=info",
        )?,
        Err(error) => return Err(error.into()),
    };
    // Keep one writer identity for tracing and the metadata-only native consumer.
    // tracing 与仅元数据原生消费者保留同一写入器身份。
    let native_writer = writer.clone();
    tracing_subscriber::registry()
        .with(filter)
        .with(
            tracing_subscriber::fmt::layer()
                .json()
                .with_ansi(false)
                .with_writer(move || writer.clone()),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(io::stderr),
        )
        .try_init()?;
    SHARED_WRITER
        .set(native_writer)
        .map_err(|_| io::Error::other("service diagnostic writer is already installed"))?;
    // Panic records reach disk even when the GUI deliberately discards stderr.
    // 即使 GUI 主动丢弃 stderr，panic 记录也能落盘。
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        tracing::error!("service panic: {panic}");
        previous(panic);
    }));
    tracing::info!(
        pid = std::process::id(),
        "Service diagnostic logging initialized"
    );
    Ok(())
}

/// Cloneable writer shared by every formatting call; the mutex serializes rotation and writes.
/// 所有格式化调用共享的可克隆写入器，互斥锁串行化轮转和写入。
#[derive(Clone)]
pub(crate) struct LogWriter(Arc<Mutex<RollingLog>>);

/// Return the installed writer or an explicit initialization error; never open another log.
/// 返回已安装写入器或明确的初始化错误；绝不打开第二份日志。
pub(crate) fn shared_writer() -> Result<LogWriter, String> {
    SHARED_WRITER
        .get()
        .cloned()
        .ok_or_else(|| "Service diagnostic writer is not installed".into())
}

impl LogWriter {
    /// Persist whitelisted JSON `record` and return its actual rotation segment, or the IO error.
    /// 持久化白名单 JSON `record` 并返回其真实轮转分段，或返回 IO 错误。
    pub(crate) fn write_native_record(&self, record: &serde_json::Value) -> io::Result<u64> {
        // Serialization and disk access run only on the native consumer, never on a Core callback.
        // 序列化及磁盘访问仅在原生消费者执行，绝不在 Core 回调执行。
        let mut bytes = serde_json::to_vec(record)?;
        bytes.push(b'\n');
        let mut log = self
            .0
            .lock()
            .map_err(|_| io::Error::other("service log lock poisoned"))?;
        log.write_record(&bytes)?;
        Ok(log.segment)
    }

    /// Place and persist dedicated `checkpoint` in one existing writer lock; return its actual segment.
    /// 在一次既有写入器锁内确定并持久化专用 `checkpoint` 落点；返回其真实分段。
    /// `first_segment` identifies the capture's original window; rotation resets copied segment counts.
    /// `first_segment` 标识采集的原窗口；轮转会重置已复制的分段计数。
    pub(crate) fn write_native_checkpoint(
        &self,
        checkpoint: &mut crate::native_diagnostics::NativeCheckpoint,
        first_segment: u64,
    ) -> io::Result<u64> {
        // Both external tracing rotation and this checkpoint's own capacity decision are serialized here.
        // 外部 tracing 轮转及本检查点自身容量判断均在此串行化。
        let mut log = self
            .0
            .lock()
            .map_err(|_| io::Error::other("service log lock poisoned"))?;
        checkpoint.bind_segment(log.segment, first_segment)?;
        // Exact serialized size decides rotation; no independent capacity or speculative size is introduced.
        // 精确序列化大小决定轮转；不引入独立容量或推测大小。
        let mut bytes = checkpoint.line()?;
        if bytes.len() as u64 > log.limit {
            return Err(io::Error::other(
                "native checkpoint exceeds the service log segment",
            ));
        }
        if log.file.is_none() || log.bytes.saturating_add(bytes.len() as u64) > log.limit {
            log.rotate()?;
            checkpoint.bind_segment(log.segment, first_segment)?;
            bytes = checkpoint.line()?;
            // A segment-number digit change can change the final JSON length; reject rather than omit it.
            // 分段编号位数变化可能改变最终 JSON 长度；直接拒绝，而非省略检查点。
            if bytes.len() as u64 > log.limit {
                return Err(io::Error::other(
                    "native checkpoint exceeds the service log segment",
                ));
            }
        }
        // The same lock keeps this exact bound and segment stable through the original write path.
        // 同一锁使精确容量及分段通过原写入路径时保持稳定。
        log.write_record(&bytes)?;
        Ok(log.segment)
    }

    /// Return the current rotation segment for window-local accounting, or the lock error.
    /// 返回当前轮转分段供窗口内计数，或返回锁错误。
    pub(crate) fn native_segment(&self) -> io::Result<u64> {
        Ok(self
            .0
            .lock()
            .map_err(|_| io::Error::other("service log lock poisoned"))?
            .segment)
    }

    /// Flush the same actual file used by tracing; report a closed handle or IO error.
    /// 刷新 tracing 使用的同一真实文件；报告句柄关闭或 IO 错误。
    pub(crate) fn flush_native(&self) -> io::Result<()> {
        self.0
            .lock()
            .map_err(|_| io::Error::other("service log lock poisoned"))?
            .file
            .as_mut()
            .ok_or_else(|| io::Error::other("service log file is closed"))?
            .flush()
    }

    /// Create an isolated real writer with `limit` for native consumer tests.
    /// 使用 `limit` 创建隔离的真实写入器供原生消费者测试。
    #[cfg(test)]
    pub(crate) fn native_test_writer(root: PathBuf, limit: u64) -> io::Result<Self> {
        Ok(Self(Arc::new(Mutex::new(RollingLog::open(root, limit)?))))
    }

    /// Close an isolated test handle so the fixture can replace its directory with an actual IO fault.
    /// 关闭隔离测试句柄，使夹具可将目录替换为真实 IO 故障。
    #[cfg(test)]
    pub(crate) fn native_test_close(&self) {
        if let Ok(mut log) = self.0.lock() {
            log.file.take();
        }
    }
}

impl Write for LogWriter {
    /// Persist one formatted record and report IO failure without recursively logging it.
    /// 持久化一个已格式化记录，直接报告 IO 失败，不递归记录日志。
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .map_err(|_| io::Error::other("service log lock poisoned"))?
            .write_record(bytes)
    }

    /// File writes are unbuffered in userspace; return success without a second queue.
    /// 文件在用户态不另设缓冲，直接返回成功，不引入第二个队列。
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Size-bounded file and rotation state, exclusively protected by LogWriter.
/// 由 LogWriter 独占保护的有界文件与轮转状态。
struct RollingLog {
    /// Authoritative logs directory.
    /// 权威日志目录。
    root: PathBuf,
    /// Current file; absent only during rotation.
    /// 当前文件，仅在轮转期间为空。
    file: Option<File>,
    /// Current segment byte length.
    /// 当前分段字节长度。
    bytes: u64,
    /// Segment size passed by production or the deterministic test fixture.
    /// 由生产入口或确定性测试夹具传入的分段大小。
    limit: u64,
    /// Monotonic process-local rotation segment, not a lifetime event counter.
    /// 进程内单调轮转分段，不是生命周期事件计数。
    segment: u64,
}

impl RollingLog {
    /// Create or reopen this root's current log with the supplied per-segment bound.
    /// 使用传入的单段上限创建或重新打开此根的当前日志。
    fn open(root: PathBuf, limit: u64) -> io::Result<Self> {
        fs::create_dir_all(&root)?;
        // Explicit Unix permissions keep new diagnostic files private to their owner.
        // 显式 Unix 权限使新诊断文件仅对所属用户开放。
        let mut options = OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file =
            options.open(root.join(vulcan_runtime_assets::service_log_segment_file_name(0)))?;
        let bytes = file.metadata()?.len();
        Ok(Self {
            root,
            file: Some(file),
            bytes,
            limit,
            segment: 0,
        })
    }

    /// Write a complete record; replace oversized records with a valid JSON diagnostic.
    /// 写入完整记录，将过大记录替换为合法的 JSON 诊断。
    fn write_record(&mut self, bytes: &[u8]) -> io::Result<usize> {
        // Preserve JSON validity instead of truncating arbitrary UTF-8 or serialized fields.
        // 保留 JSON 有效性，不截断任意 UTF-8 或序列化字段。
        let record = if bytes.len() as u64 > self.limit {
            b"{\"level\":\"WARN\",\"message\":\"oversized service log record omitted\"}\n"
                .as_slice()
        } else {
            bytes
        };
        if record.len() as u64 > self.limit {
            return Err(io::Error::other(
                "log segment is smaller than overflow diagnostic",
            ));
        }
        if self.file.is_none() || self.bytes + record.len() as u64 > self.limit {
            self.rotate()?;
        }
        self.file
            .as_mut()
            .ok_or_else(|| io::Error::other("service log file is closed"))?
            .write_all(record)?;
        self.bytes += record.len() as u64;
        Ok(bytes.len())
    }

    /// Close the Windows handle before bounded archive rotation; propagate filesystem failures.
    /// 在有界归档轮转前关闭 Windows 文件句柄，向上传递文件系统失败。
    fn rotate(&mut self) -> io::Result<()> {
        self.file.take();
        for index in (1..=ARCHIVE_COUNT).rev() {
            // Archive indices identify files, never records or source-code positions.
            // 归档编号标识文件，不代表记录或源码位置。
            let destination = self
                .root
                .join(vulcan_runtime_assets::service_log_segment_file_name(index));
            if index == ARCHIVE_COUNT && destination.try_exists()? {
                fs::remove_file(&destination)?;
            }
            let source = self
                .root
                .join(vulcan_runtime_assets::service_log_segment_file_name(
                    index - 1,
                ));
            if source.try_exists()? {
                fs::rename(source, destination)?;
            }
        }
        // Preserve the segment identity across reopen so native accounting cannot span deleted history.
        // 重新打开后保留分段身份，使原生计数不能跨越已删除历史。
        let next_segment = self.segment.saturating_add(1);
        *self = Self::open(self.root.clone(), self.limit)?;
        self.segment = next_segment;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run the real initializer in an isolated child with discarded stderr; verify errors and panics survive exit.
    /// 在丢弃 stderr 的隔离子进程中运行真实初始化器，验证错误与 panic 在退出后仍保留。
    #[test]
    fn hidden_process_persists_diagnostics() {
        // Only this exact child test receives the temporary root, never the running user's service.
        // 仅此精确子测试接收临时根目录，绝不访问用户正在运行的服务。
        if let Some(root) = std::env::var_os("VULCAN_LOGGING_TEST_CHILD_ROOT") {
            initialize(Path::new(&root)).unwrap();
            startup_error("hidden startup failure fixture");
            let _ = std::panic::catch_unwind(|| panic!("hidden panic fixture"));
            std::process::exit(0);
        }
        let root = tempfile::tempdir().unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "logging::tests::hidden_process_persists_diagnostics",
                "--nocapture",
            ])
            .env("VULCAN_LOGGING_TEST_CHILD_ROOT", root.path())
            .env("RUST_LOG", "warn,vulcan_code_service=info")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success(), "isolated logging child failed: {status}");
                break;
            }
            if std::time::Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("isolated logging child exceeded its deadline");
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let contents = fs::read_to_string(directory(root.path()).join("service.jsonl")).unwrap();
        assert!(contents.contains("hidden startup failure fixture"));
        assert!(contents.contains("hidden panic fixture"));
        for line in contents.lines() {
            serde_json::from_str::<serde_json::Value>(line).unwrap();
        }
    }

    /// Prove retention bounds, reopening and complete records without touching user logs.
    /// 在不访问用户日志的前提下验证留存上限、重新打开和记录完整性。
    #[test]
    fn rotates_and_reopens_bounded_segments() {
        let root = tempfile::tempdir().unwrap();
        let record = b"{\"message\":\"bounded record\"}\n";
        let limit = (record.len() * 4) as u64;
        let mut log = RollingLog::open(root.path().to_owned(), limit).unwrap();
        for _ in 0..40 {
            log.write_record(record).unwrap();
        }
        drop(log);
        let mut log = RollingLog::open(root.path().to_owned(), limit).unwrap();
        log.write_record(record).unwrap();
        assert_eq!(
            fs::read_dir(root.path()).unwrap().count(),
            ARCHIVE_COUNT + 1
        );
        for entry in fs::read_dir(root.path()).unwrap() {
            let contents = fs::read(entry.unwrap().path()).unwrap();
            assert!(contents.len() as u64 <= limit);
            for line in String::from_utf8(contents).unwrap().lines() {
                serde_json::from_str::<serde_json::Value>(line).unwrap();
            }
        }
    }

    /// Prove structured diagnostics reach the real file without a console or global subscriber.
    /// 无需控制台或全局接收器，证明结构化诊断可以写入真实文件。
    #[test]
    fn records_structured_events_and_bounds_oversized_records() {
        let root = tempfile::tempdir().unwrap();
        let writer = LogWriter(Arc::new(Mutex::new(
            RollingLog::open(root.path().to_owned(), 512).unwrap(),
        )));
        let subscriber = tracing_subscriber::fmt()
            .json()
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            tracing::warn!(request_id = "fixture-request", "fixture failure");
            tracing::warn!("{}", "x".repeat(1024));
        });
        let contents = fs::read_to_string(root.path().join("service.jsonl")).unwrap();
        assert!(contents.contains("fixture-request"));
        assert!(contents.contains("oversized service log record omitted"));
        for line in contents.lines() {
            serde_json::from_str::<serde_json::Value>(line).unwrap();
        }
    }
}
