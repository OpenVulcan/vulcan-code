//! Cooperative cross-process exclusion for host-owned VCE index mutations.
//! 宿主自有 VCE 索引修改的协作式跨进程互斥。

use super::{metadata_is_link, verify_regular_file_or_absent};
use std::fs::{File, TryLockError};
use std::path::Path;

/// Non-cloneable OS handle retained across admission, queued work and durable finalization.
/// 跨准入、排队工作及持久终态处理保留的不可克隆操作系统句柄。
/// Dropping the final owner closes the handle; the lock file is never removed or truncated.
/// 最后持有者释放时关闭句柄；绝不删除或截断锁文件。
/// This excludes cooperating hosts, but does not prove in-process storage operations have drained.
/// 此类型排斥协作宿主，但不证明进程内存储操作已经排空。
#[derive(Debug)]
pub struct VceProjectWriterLease {
    /// Private handle cannot be cloned or explicitly unlocked through the public surface.
    /// 私有句柄不能通过公共表面克隆或显式解锁。
    _file: File,
}

impl VceProjectWriterLease {
    /// Lock the exact path already validated by the project storage resolver without waiting.
    /// 不等待地锁定项目存储解析器已校验的精确路径。
    /// `path` is the permanent project lock file. Returns ownership, busy, or an explicit error.
    /// `path` 为永久项目锁文件。返回归属、忙状态或明确错误。
    pub(super) fn try_acquire(path: &Path) -> Result<Option<Self>, String> {
        // options preserve existing file contents and do not expose database files to OS locking.
        // options 保留现有文件内容，不对数据库文件施加操作系统锁。
        let mut options = File::options();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            // Deny deletion of a live lock and inspect the final reparse point rather than following it.
            // 禁止删除活动锁文件，并检查末端重解析点而不跟随它。
            options.share_mode(0x0000_0003).custom_flags(0x0020_0000);
        }
        // file is independently opened; no inherited clone can silently extend this lease.
        // file 独立打开；没有继承的克隆可静默延长此租约。
        let file = options
            .open(path)
            .map_err(|error| format!("Open VCE project writer lease: {error}"))?;
        // metadata verifies the opened object, not only the pre-open path observation.
        // metadata 校验已打开对象，而不只是打开前的路径观察。
        let metadata = file
            .metadata()
            .map_err(|error| format!("Inspect VCE project writer handle: {error}"))?;
        if metadata_is_link(&metadata) || !metadata.is_file() {
            return Err(String::from(
                "VCE project writer handle must be a regular non-link file",
            ));
        }
        verify_regular_file_or_absent(path)?;
        match file.try_lock() {
            Ok(()) => Ok(Some(Self { _file: file })),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(error)) => {
                Err(format!("Acquire VCE project writer lease: {error}"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::VceStoragePaths;
    use std::io::{BufRead, Read, Write};
    use std::process::{Child, Command, Stdio};
    use std::time::Duration;

    /// Verify independent handles exclude the same project without allocating database directories.
    /// 验证独立句柄排斥同项目，不分配数据库目录。
    /// Takes no input; assertions use only an automatically removed temporary fixture.
    /// 无输入；断言仅使用自动移除的临时夹具。
    #[test]
    fn writer_lease_is_project_scoped_and_non_destructive() {
        // temporary and paths contain no user configuration or real index.
        // temporary 与 paths 不含用户配置或真实索引。
        let temporary = tempfile::tempdir().expect("temporary root");
        let paths = VceStoragePaths::resolve(temporary.path()).expect("paths");
        assert!(paths.try_acquire_writer("a", false).is_err());
        assert!(!paths.root().exists());
        // first owns only the permanent lock file in the explicitly admitted project directory.
        // first 仅持有明确准入项目目录中的永久锁文件。
        let first = paths
            .try_acquire_writer("a", true)
            .expect("first")
            .expect("owner");
        // project exposes only verified paths for filesystem assertions.
        // project 仅暴露经过验证的路径用于文件系统断言。
        let project = paths.project("a").expect("project");
        assert!(!project.lancedb_path.exists());
        assert!(!project.root.join("sqlite").exists());
        assert!(
            paths
                .try_acquire_writer("a", false)
                .expect("busy")
                .is_none()
        );
        #[cfg(windows)]
        assert!(std::fs::remove_file(project.root.join("writer.lock")).is_err());
        // second demonstrates that the lock does not serialize unrelated projects.
        // second 证明该锁不会串行化无关项目。
        let second = paths
            .try_acquire_writer("b", true)
            .expect("second")
            .expect("other project");
        drop(first);
        assert!(project.root.join("writer.lock").is_file());
        // Existing content is retained when reopening; locking never rewrites an ownership marker.
        // 重新打开时保留现有内容；加锁绝不重写归属标记。
        std::fs::write(project.root.join("writer.lock"), b"fixture").expect("fixture marker");
        // reopened has a new independent handle to the same permanent file.
        // reopened 对相同永久文件持有新的独立句柄。
        let reopened = paths
            .try_acquire_writer("a", false)
            .expect("reopen")
            .expect("owner");
        drop(reopened);
        assert_eq!(
            std::fs::read(project.root.join("writer.lock")).expect("marker"),
            b"fixture"
        );
        drop(second);
        assert!(!temporary.path().join("vmm").exists());
    }

    /// Reject lock-file type collisions before any writer can claim ownership.
    /// 在任何写入者取得归属前拒绝锁文件类型冲突。
    /// No parameters or return value; the directory collision exists only inside the fixture.
    /// 无参数及返回值；目录冲突仅存在于夹具内部。
    #[test]
    fn writer_lease_rejects_directory_collision() {
        // temporary limits every mutation to this test's unique scope.
        // temporary 将全部修改限制于此测试的唯一范围。
        let temporary = tempfile::tempdir().expect("temporary root");
        // paths and project derive the exact collision target from host ownership.
        // paths 与 project 从宿主归属派生精确冲突目标。
        let paths = VceStoragePaths::resolve(temporary.path()).expect("paths");
        let project = paths.prepare_project("a").expect("project");
        std::fs::create_dir(project.root.join("writer.lock")).expect("collision");
        assert!(paths.try_acquire_writer("a", false).is_err());
    }

    /// Own only a spawned fixture process so failed assertions cannot leave it waiting on stdin.
    /// 仅持有生成的夹具进程，避免断言失败后遗留等待标准输入的进程。
    struct FixtureChild {
        /// Exact process created by the test, never discovered from user process enumeration.
        /// 测试创建的精确进程，绝不从用户进程枚举中发现。
        child: Child,
    }

    impl Drop for FixtureChild {
        /// Stop and reap this fixture child on either success or unwinding; no parameters or output.
        /// 成功或展开时停止并回收此夹具子进程；无参数及输出。
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    /// Start the exact child-test entry and wait at most ten seconds for its lock result.
    /// 启动精确子测试入口，最多等待十秒取得其锁结果。
    /// `root` is the fixture root; returns an owned child and its explicit acquired/busy marker.
    /// `root` 为夹具根；返回自有子进程及其明确已取得或忙标记。
    fn spawn_writer_child(root: &std::path::Path) -> (FixtureChild, String) {
        // child runs no other test and inherits no process-wide mutation from the parent.
        // child 不运行其他测试，不从父进程继承进程级修改。
        let mut child = FixtureChild {
            child: Command::new(std::env::current_exe().expect("test executable"))
                .args([
                    "--exact",
                    "vce_storage_paths::writer_lease::tests::writer_lease_child",
                    "--ignored",
                    "--nocapture",
                ])
                .env("VCE_WRITER_TEST_ROOT", root)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .expect("spawn fixture"),
        };
        // stdout is drained by one bounded-result reader so a failed child cannot hang the harness.
        // stdout 由一个有界结果读取器排空，避免失败子进程挂住测试框架。
        let stdout = child.child.stdout.take().expect("stdout");
        // sender/receiver transport one explicit marker without an unbounded output buffer.
        // sender 与 receiver 传输一个明确标记，不使用无界输出缓冲区。
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(stdout)
                .lines()
                .map_while(Result::ok)
            {
                if line == "VCE_WRITER_ACQUIRED" || line == "VCE_WRITER_BUSY" {
                    let _ = sender.send(line);
                    break;
                }
            }
        });
        // marker is acknowledged by the child, not inferred from process creation or timing.
        // marker 由子进程确认，不从进程创建或时间推断。
        let marker = receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("bounded child marker");
        (child, marker)
    }

    /// Verify a live foreign process is excluded and its abnormal exit releases only its own lease.
    /// 验证活动外部进程被排斥，且其异常退出仅释放自身租约。
    /// No parameters or output; only explicitly spawned fixture children are terminated.
    /// 无参数及输出；仅终止明确生成的夹具子进程。
    #[test]
    fn writer_lease_survives_cross_process_contention_and_releases_on_exit() {
        // temporary and first establish a known parent-owned lock before starting the contender.
        // temporary 与 first 在启动竞争者前建立已知父进程持有锁。
        let temporary = tempfile::tempdir().expect("temporary root");
        let paths = VceStoragePaths::resolve(temporary.path()).expect("paths");
        let first = paths
            .try_acquire_writer("a", true)
            .expect("first")
            .expect("owner");
        // busy_child proves exclusion across a real independent process.
        // busy_child 证明真实独立进程之间的互斥。
        let (busy_child, marker) = spawn_writer_child(temporary.path());
        assert_eq!(marker, "VCE_WRITER_BUSY");
        drop(busy_child);
        drop(first);
        // owner_child now holds the lock while its parent remains a contender.
        // owner_child 此时持有锁，而父进程作为竞争者。
        let (owner_child, marker) = spawn_writer_child(temporary.path());
        assert_eq!(marker, "VCE_WRITER_ACQUIRED");
        assert!(
            paths
                .try_acquire_writer("a", false)
                .expect("busy")
                .is_none()
        );
        drop(owner_child);
        // after_exit proves OS release after killing only the fixture process, without deleting a lock file.
        // after_exit 证明仅终止夹具进程后操作系统释放锁，无需删除锁文件。
        let after_exit = paths
            .try_acquire_writer("a", false)
            .expect("after exit")
            .expect("released");
        drop(after_exit);
        assert!(
            paths
                .project("a")
                .expect("project")
                .root
                .join("writer.lock")
                .is_file()
        );
    }

    /// Run only under the parent's explicit fixture environment and report actual OS ownership.
    /// 仅在父进程的明确夹具环境中运行，并报告实际操作系统归属。
    /// No arguments or return value; waits on fixture stdin only after acknowledging its lease.
    /// 无参数及返回值；仅在确认租约后等待夹具标准输入。
    #[test]
    #[ignore = "subprocess fixture invoked only by its bounded parent test"]
    fn writer_lease_child() {
        // root is passed solely to the spawned child, never set in the parent's shared environment.
        // root 仅传给生成的子进程，绝不设置于父进程的共享环境。
        let root = std::env::var_os("VCE_WRITER_TEST_ROOT").expect("fixture root");
        // paths and lease use exactly the same production resolver and lock as the parent.
        // paths 与 lease 使用与父进程完全相同的生产解析器及锁。
        let paths = VceStoragePaths::resolve(std::path::Path::new(&root)).expect("paths");
        let lease = paths
            .try_acquire_writer("a", false)
            .expect("child acquisition");
        if lease.is_some() {
            println!("VCE_WRITER_ACQUIRED");
            std::io::stdout().flush().expect("flush marker");
            // release is a fixture-only handshake; the parent can alternatively kill this exact child.
            // release 为夹具专用交接；父进程也可以终止此精确子进程。
            let mut release = [0_u8; 1];
            std::io::stdin()
                .read_exact(&mut release)
                .expect("fixture release");
        } else {
            println!("VCE_WRITER_BUSY");
            std::io::stdout().flush().expect("flush marker");
        }
        drop(lease);
    }
}
