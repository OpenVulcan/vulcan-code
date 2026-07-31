//! Host-owned backup and official vector-rebuild orchestration for managed VMM data.
//! 宿主持有的托管 VMM 数据备份与正式向量重建编排。

use crate::managed_memory_mesh_runtime::{
    ManagedVmmMaintenancePreparation, cleanup_managed_vmm_maintenance_manifest,
    finalize_managed_vmm_rebuild,
};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc, OnceLock, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;
use vulcan_service_api::{VmmMaintenancePhase, VmmMaintenanceSummary};

/// Polling interval used while the official maintenance process owns the data space.
/// 正式维护进程持有数据空间期间使用的轮询间隔。
const MAINTENANCE_POLL_INTERVAL: Duration = Duration::from_millis(200);

/// Process-local owner retained across WebUI navigation and workbench refreshes.
/// 跨 WebUI 导航与工作台刷新保留的进程内所有者。
static MANAGED_VMM_MAINTENANCE: OnceLock<ManagedVmmMaintenanceRuntime> = OnceLock::new();

/// Owns one process-local maintenance status and cancellation channel.
/// 持有一个进程内维护状态与取消通道。
#[derive(Clone)]
pub(crate) struct ManagedVmmMaintenanceRuntime {
    /// Authoritative operation snapshot shared across page navigation.
    /// 跨页面导航共享的权威操作快照。
    snapshot: Arc<RwLock<VmmMaintenanceSummary>>,
    /// Cooperative cancellation signal checked during backup and rebuild.
    /// 在备份与重建期间检查的协作式取消信号。
    cancel: Arc<AtomicBool>,
}

impl Default for ManagedVmmMaintenanceRuntime {
    /// Creates an idle process-local maintenance owner.
    /// 创建一个空闲的进程内维护所有者。
    fn default() -> Self {
        Self {
            snapshot: Arc::new(RwLock::new(VmmMaintenanceSummary::default())),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl ManagedVmmMaintenanceRuntime {
    /// Returns the latest authoritative maintenance snapshot.
    /// 返回最近的权威维护快照。
    pub(crate) fn snapshot(&self) -> VmmMaintenanceSummary {
        self.snapshot
            .read()
            .map(|snapshot| snapshot.clone())
            .unwrap_or_else(|error| VmmMaintenanceSummary {
                phase: VmmMaintenancePhase::Failed,
                message: String::from("无法读取记忆维护状态。"),
                technical_detail: Some(error.to_string()),
                ..VmmMaintenanceSummary::default()
            })
    }

    /// Starts one background backup and official vector rebuild.
    /// 启动一次后台备份与正式向量重建。
    ///
    /// # Parameters
    /// 参数
    /// `preparation` contains the exact managed manifest, data root, and vector identity.
    /// `preparation` 包含准确的托管清单、数据根与向量身份。
    ///
    /// # Returns
    /// 返回值
    /// Success after the operation is exclusively accepted, not after it completes.
    /// 操作被独占接纳后返回成功，而不是等待操作完成。
    pub(crate) fn start(
        &self,
        preparation: ManagedVmmMaintenancePreparation,
    ) -> Result<VmmMaintenanceSummary, String> {
        // acceptedSnapshot is published under one write lock so concurrent starts cannot both win.
        // acceptedSnapshot 在同一个写锁下发布，防止并发启动同时成功。
        let accepted_snapshot = {
            let mut current = self
                .snapshot
                .write()
                .map_err(|error| format!("锁定记忆维护状态失败：{error}"))?;
            if matches!(
                current.phase,
                VmmMaintenancePhase::BackingUp
                    | VmmMaintenancePhase::Rebuilding
                    | VmmMaintenancePhase::Cancelling
            ) {
                return Err(String::from("记忆数据维护已经在进行中。"));
            }
            // Reset the previous operation's cancellation only before publishing the new active state.
            // 仅在发布新的活动状态前清除上一次操作的取消信号，避免接纳后覆盖用户刚发出的取消。
            self.cancel.store(false, Ordering::SeqCst);
            let now = current_unix_millis();
            *current = VmmMaintenanceSummary {
                phase: VmmMaintenancePhase::BackingUp,
                progress_percent: 1,
                message: String::from("正在安全备份历史记忆数据…"),
                can_cancel: true,
                backup_created: false,
                started_at_unix_ms: Some(now),
                updated_at_unix_ms: Some(now),
                technical_detail: None,
            };
            current.clone()
        };
        let snapshot = Arc::clone(&self.snapshot);
        let cancel = Arc::clone(&self.cancel);
        if let Err(error) = thread::Builder::new()
            .name(String::from("vmm-managed-maintenance"))
            .spawn(move || run_maintenance(snapshot, cancel, preparation))
        {
            update_terminal(
                &self.snapshot,
                VmmMaintenancePhase::Failed,
                accepted_snapshot.progress_percent,
                "无法启动记忆数据维护任务。",
                Some(error.to_string()),
            );
            return Err(format!("启动记忆数据维护任务失败：{error}"));
        }
        Ok(accepted_snapshot)
    }

    /// Requests cooperative cancellation of the active operation.
    /// 请求协作式取消当前操作。
    pub(crate) fn cancel(&self) -> Result<VmmMaintenanceSummary, String> {
        let current = self.snapshot();
        if !matches!(
            current.phase,
            VmmMaintenancePhase::BackingUp | VmmMaintenancePhase::Rebuilding
        ) {
            return Err(String::from("当前没有可取消的记忆数据维护操作。"));
        }
        self.cancel.store(true, Ordering::SeqCst);
        update_snapshot(
            &self.snapshot,
            VmmMaintenanceSummary {
                phase: VmmMaintenancePhase::Cancelling,
                message: String::from("正在安全停止记忆数据维护…"),
                can_cancel: false,
                updated_at_unix_ms: Some(current_unix_millis()),
                ..current
            },
        );
        Ok(self.snapshot())
    }
}

/// Returns the process-local managed VMM maintenance owner.
/// 返回进程内托管 VMM 维护所有者。
pub(crate) fn runtime() -> &'static ManagedVmmMaintenanceRuntime {
    MANAGED_VMM_MAINTENANCE.get_or_init(ManagedVmmMaintenanceRuntime::default)
}

/// Executes backup, official rebuild, and marker publication on one background thread.
/// 在一个后台线程上执行备份、正式重建与标记发布。
fn run_maintenance(
    snapshot: Arc<RwLock<VmmMaintenanceSummary>>,
    cancel: Arc<AtomicBool>,
    preparation: ManagedVmmMaintenancePreparation,
) {
    let result = run_maintenance_inner(&snapshot, &cancel, &preparation);
    cleanup_managed_vmm_maintenance_manifest(&preparation.manifest_path);
    match result {
        Ok(()) => update_terminal(
            &snapshot,
            VmmMaintenancePhase::Completed,
            100,
            "历史记忆已安全备份，向量索引已完成重建。",
            None,
        ),
        Err(error) if cancel.load(Ordering::SeqCst) => update_terminal(
            &snapshot,
            VmmMaintenancePhase::Cancelled,
            snapshot
                .read()
                .map(|current| current.progress_percent)
                .unwrap_or_default(),
            "记忆数据维护已取消；原始数据未被删除。",
            Some(error),
        ),
        Err(error) => update_terminal(
            &snapshot,
            VmmMaintenancePhase::Failed,
            snapshot
                .read()
                .map(|current| current.progress_percent)
                .unwrap_or_default(),
            "记忆数据维护失败，已保留原始数据和已完成的安全备份。",
            Some(error),
        ),
    }
}

/// Runs the fallible maintenance stages while preserving the original data root.
/// 在保留原始数据根的前提下执行可能失败的维护阶段。
fn run_maintenance_inner(
    snapshot: &Arc<RwLock<VmmMaintenanceSummary>>,
    cancel: &Arc<AtomicBool>,
    preparation: &ManagedVmmMaintenancePreparation,
) -> Result<(), String> {
    let backup_root = preparation
        .data_root
        .parent()
        .ok_or_else(|| String::from("无法解析托管记忆数据根的父目录。"))?
        .join("backups");
    fs::create_dir_all(&backup_root).map_err(|error| format!("创建记忆备份目录失败：{error}"))?;
    let backup_name = format!(
        "vmm-data-{}-{}",
        current_unix_millis(),
        Uuid::new_v4().as_simple()
    );
    let partial_backup = backup_root.join(format!("{backup_name}.partial"));
    let completed_backup = backup_root.join(backup_name);
    if let Err(error) =
        copy_directory_with_progress(&preparation.data_root, &partial_backup, snapshot, cancel)
    {
        let _ = fs::remove_dir_all(&partial_backup);
        return Err(error);
    }
    if cancel.load(Ordering::SeqCst) {
        let _ = fs::remove_dir_all(&partial_backup);
        return Err(String::from("备份阶段收到取消请求。"));
    }
    if let Err(error) = fs::rename(&partial_backup, &completed_backup) {
        let _ = fs::remove_dir_all(&partial_backup);
        return Err(format!("完成记忆备份目录提交失败：{error}"));
    }
    let current = snapshot
        .read()
        .map(|current| current.clone())
        .unwrap_or_default();
    update_snapshot(
        snapshot,
        VmmMaintenanceSummary {
            phase: VmmMaintenancePhase::Rebuilding,
            progress_percent: 50,
            message: String::from("安全备份已完成，正在重建向量索引…"),
            can_cancel: true,
            backup_created: true,
            updated_at_unix_ms: Some(current_unix_millis()),
            ..current
        },
    );
    run_official_vector_rebuild(preparation, snapshot, cancel)?;
    finalize_managed_vmm_rebuild(&preparation.data_root, &preparation.embedding_fingerprint)?;
    Ok(())
}

/// Copies one directory tree into a new backup while publishing byte-based progress.
/// 把一个目录树复制到新备份，并发布基于字节的进度。
fn copy_directory_with_progress(
    source: &Path,
    target: &Path,
    snapshot: &Arc<RwLock<VmmMaintenanceSummary>>,
    cancel: &Arc<AtomicBool>,
) -> Result<(), String> {
    if target.exists() {
        fs::remove_dir_all(target).map_err(|error| format!("清理上次未完成备份失败：{error}"))?;
    }
    fs::create_dir_all(target).map_err(|error| format!("创建记忆备份失败：{error}"))?;
    let files = collect_backup_files(source)?;
    let total_bytes = files
        .iter()
        .try_fold(0_u64, |total, path| {
            path.metadata()
                .map(|metadata| total.saturating_add(metadata.len()))
        })
        .map_err(|error| format!("统计记忆备份大小失败：{error}"))?;
    let mut copied_bytes = 0_u64;
    for source_file in files {
        if cancel.load(Ordering::SeqCst) {
            return Err(String::from("备份阶段收到取消请求。"));
        }
        let relative = source_file
            .strip_prefix(source)
            .map_err(|error| format!("解析记忆备份相对路径失败：{error}"))?;
        let target_file = target.join(relative);
        if let Some(parent) = target_file.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("创建记忆备份子目录失败：{error}"))?;
        }
        copy_file_with_progress(
            &source_file,
            &target_file,
            total_bytes,
            &mut copied_bytes,
            snapshot,
            cancel,
        )?;
    }
    Ok(())
}

/// Copies one regular file in bounded chunks so cancellation remains responsive.
/// 以有界分块复制一个常规文件，使取消操作保持及时响应。
fn copy_file_with_progress(
    source: &Path,
    target: &Path,
    total_bytes: u64,
    copied_bytes: &mut u64,
    snapshot: &Arc<RwLock<VmmMaintenanceSummary>>,
    cancel: &Arc<AtomicBool>,
) -> Result<(), String> {
    let mut source_file =
        fs::File::open(source).map_err(|error| format!("打开记忆源文件失败：{error}"))?;
    let mut target_file =
        fs::File::create(target).map_err(|error| format!("创建记忆备份文件失败：{error}"))?;
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err(String::from("备份阶段收到取消请求。"));
        }
        let read = source_file
            .read(&mut buffer)
            .map_err(|error| format!("读取记忆数据失败：{error}"))?;
        if read == 0 {
            break;
        }
        target_file
            .write_all(&buffer[..read])
            .map_err(|error| format!("写入记忆备份失败：{error}"))?;
        *copied_bytes = copied_bytes.saturating_add(read as u64);
        let ratio = if total_bytes == 0 {
            1.0
        } else {
            *copied_bytes as f64 / total_bytes as f64
        };
        let progress = 1_u8.saturating_add((ratio * 44.0).round() as u8).min(45);
        let current = snapshot
            .read()
            .map(|current| current.clone())
            .unwrap_or_default();
        update_snapshot(
            snapshot,
            VmmMaintenanceSummary {
                progress_percent: progress,
                updated_at_unix_ms: Some(current_unix_millis()),
                ..current
            },
        );
    }
    target_file
        .sync_all()
        .map_err(|error| format!("同步记忆备份文件失败：{error}"))?;
    let permissions = fs::metadata(source)
        .map_err(|error| format!("读取记忆源文件权限失败：{error}"))?
        .permissions();
    fs::set_permissions(target, permissions)
        .map_err(|error| format!("应用记忆备份文件权限失败：{error}"))?;
    Ok(())
}

/// Collects regular files under one source root in stable path order.
/// 按稳定路径顺序收集一个来源根下的常规文件。
fn collect_backup_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in
            fs::read_dir(&directory).map_err(|error| format!("读取记忆数据目录失败：{error}"))?
        {
            let entry = entry.map_err(|error| format!("读取记忆数据项失败：{error}"))?;
            let file_type = entry
                .file_type()
                .map_err(|error| format!("读取记忆数据类型失败：{error}"))?;
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if file_type.is_file() {
                files.push(entry.path());
            } else {
                return Err(format!(
                    "记忆数据包含不支持的链接或特殊文件：{}",
                    entry.path().display()
                ));
            }
        }
    }
    files.sort();
    Ok(files)
}

/// Runs the official VMM maintenance companion and observes cancellation.
/// 运行 VMM 正式维护配套程序并观察取消请求。
fn run_official_vector_rebuild(
    preparation: &ManagedVmmMaintenancePreparation,
    snapshot: &Arc<RwLock<VmmMaintenanceSummary>>,
    cancel: &Arc<AtomicBool>,
) -> Result<(), String> {
    let log_path = preparation
        .manifest_path
        .with_file_name("managed-maintenance.log");
    let output = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&log_path)
        .map_err(|error| format!("创建 VMM 维护日志失败：{error}"))?;
    let error_output = output
        .try_clone()
        .map_err(|error| format!("复制 VMM 维护日志句柄失败：{error}"))?;
    let mut command = Command::new(&preparation.migrate_binary_path);
    command
        .arg("-vulcan-managed-config")
        .arg(&preparation.manifest_path)
        .arg("-vector-rebuild")
        .arg("-confirm-vector-rebuild")
        .stdin(Stdio::null())
        .stdout(Stdio::from(output))
        .stderr(Stdio::from(error_output));
    crate::managed_memory_mesh_runtime::configure_hidden_child(&mut command);
    let mut child = command
        .spawn()
        .map_err(|error| format!("启动 VMM 正式向量重建失败：{error}"))?;
    #[cfg(windows)]
    let _child_job = vulcan_windows_console_helper::WindowsKillOnCloseJob::attach(&child)
        .map_err(|error| format!("保护 VMM 维护子进程失败：{error}"))?;
    loop {
        if cancel.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(String::from("向量重建阶段收到取消请求。"));
        }
        match child
            .try_wait()
            .map_err(|error| format!("查询 VMM 向量重建状态失败：{error}"))?
        {
            Some(status) if status.success() => break,
            Some(status) => {
                let detail = fs::read_to_string(&log_path)
                    .unwrap_or_else(|_| format!("vmm-migrate 退出状态：{status}"));
                return Err(format!("VMM 向量重建失败：{detail}"));
            }
            None => {
                let current = snapshot
                    .read()
                    .map(|current| current.clone())
                    .unwrap_or_default();
                update_snapshot(
                    snapshot,
                    VmmMaintenanceSummary {
                        updated_at_unix_ms: Some(current_unix_millis()),
                        ..current
                    },
                );
                thread::sleep(MAINTENANCE_POLL_INTERVAL);
            }
        }
    }
    Ok(())
}

/// Publishes one complete operation snapshot without exposing its lock to callers.
/// 发布一个完整操作快照且不向调用方暴露锁。
fn update_snapshot(snapshot: &Arc<RwLock<VmmMaintenanceSummary>>, next: VmmMaintenanceSummary) {
    if let Ok(mut current) = snapshot.write() {
        *current = next;
    }
}

/// Publishes one terminal operation outcome with stable cancellation semantics.
/// 发布一个具有稳定取消语义的操作终态。
fn update_terminal(
    snapshot: &Arc<RwLock<VmmMaintenanceSummary>>,
    phase: VmmMaintenancePhase,
    progress_percent: u8,
    message: &str,
    technical_detail: Option<String>,
) {
    let current = snapshot
        .read()
        .map(|current| current.clone())
        .unwrap_or_default();
    update_snapshot(
        snapshot,
        VmmMaintenanceSummary {
            phase,
            progress_percent,
            message: message.to_string(),
            can_cancel: false,
            updated_at_unix_ms: Some(current_unix_millis()),
            technical_detail,
            ..current
        },
    );
}

/// Returns the current Unix timestamp in milliseconds.
/// 返回当前 Unix 毫秒时间戳。
fn current_unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// Verifies backup discovery rejects links instead of silently following data outside the root.
    /// 验证备份发现会拒绝链接，而不是静默跟随数据根之外的内容。
    #[test]
    fn backup_file_collection_returns_stable_regular_files() {
        let root = tempdir().expect("temporary root");
        fs::create_dir_all(root.path().join("nested")).expect("nested directory");
        fs::write(root.path().join("b.txt"), b"b").expect("write b");
        fs::write(root.path().join("nested").join("a.txt"), b"a").expect("write a");
        let files = collect_backup_files(root.path()).expect("collect files");
        let relative = files
            .iter()
            .map(|path| {
                path.strip_prefix(root.path())
                    .expect("relative path")
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect::<Vec<_>>();
        assert_eq!(relative, vec!["b.txt", "nested/a.txt"]);
    }

    /// Verifies cancellation before the first copy leaves the original data untouched.
    /// 验证首次复制前取消时原始数据保持不变。
    #[test]
    fn cancelled_backup_keeps_original_data() {
        let root = tempdir().expect("temporary root");
        let source = root.path().join("source");
        let target = root.path().join("target");
        fs::create_dir_all(&source).expect("source directory");
        fs::write(source.join("memory.db"), b"durable").expect("source data");
        let snapshot = Arc::new(RwLock::new(VmmMaintenanceSummary::default()));
        let cancel = Arc::new(AtomicBool::new(true));
        let error = copy_directory_with_progress(&source, &target, &snapshot, &cancel)
            .expect_err("cancelled backup");
        assert!(error.contains("取消"));
        assert_eq!(
            fs::read(source.join("memory.db")).expect("original data"),
            b"durable"
        );
    }
}
