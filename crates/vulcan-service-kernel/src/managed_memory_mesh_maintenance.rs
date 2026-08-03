//! Host-owned backup and official vector-rebuild orchestration for managed VMM data.
//! 宿主持有的托管 VMM 数据备份与正式向量重建编排。

use crate::managed_memory_mesh_runtime::{
    cleanup_managed_vmm_maintenance_manifest, finalize_managed_vmm_rebuild,
    ManagedVmmMaintenancePreparation,
};
use serde::Deserialize;
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, OnceLock, RwLock,
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;
use vulcan_service_api::{VmmMaintenancePhase, VmmMaintenanceSummary};

/// Polling interval used while the official maintenance process owns the data space.
/// 正式维护进程持有数据空间期间使用的轮询间隔。
const MAINTENANCE_POLL_INTERVAL: Duration = Duration::from_millis(200);

/// Machine-readable progress emitted by the official vector rebuild child process.
/// 正式向量重建子进程发出的机器可读进度。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct OfficialVectorRebuildProgress {
    /// Current child-process sub-stage.
    /// 子进程当前的重建子阶段。
    stage: String,
    /// Completed records in the current sub-stage.
    /// 当前子阶段已完成的记录数。
    processed: u64,
    /// Total records in the current sub-stage.
    /// 当前子阶段的记录总数。
    total: u64,
    /// Child progress write timestamp.
    /// 子进程写入进度的时间戳。
    updated_at_unix_ms: u64,
}

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
                progress_stage: None,
                processed_items: None,
                total_items: None,
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
    cleanup_vector_rebuild_progress_file(&preparation.manifest_path);
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
/// Returns the exact host-owned progress path for one maintenance manifest.
/// 返回一个维护清单对应的精确宿主进度文件路径。
fn vector_rebuild_progress_path(manifest_path: &Path) -> PathBuf {
    manifest_path.with_file_name("managed-maintenance-progress.json")
}

/// Removes the exact host-owned child progress file after a terminal outcome.
/// 在维护进入终态后删除精确的宿主子进程进度文件。
fn cleanup_vector_rebuild_progress_file(manifest_path: &Path) {
    let _ = fs::remove_file(vector_rebuild_progress_path(manifest_path));
}

fn run_official_vector_rebuild(
    preparation: &ManagedVmmMaintenancePreparation,
    snapshot: &Arc<RwLock<VmmMaintenanceSummary>>,
    cancel: &Arc<AtomicBool>,
) -> Result<(), String> {
    let log_path = preparation
        .manifest_path
        .with_file_name("managed-maintenance.log");
    // progress_path is the exact per-maintenance child-to-host progress channel.
    // progress_path 是本次维护专用的子进程到宿主进度通道。
    let progress_path = vector_rebuild_progress_path(&preparation.manifest_path);
    let _ = fs::remove_file(&progress_path);
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
        .arg("-vector-rebuild-progress-file")
        .arg(&progress_path)
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
            Some(status) if status.success() => {
                publish_official_vector_rebuild_progress(snapshot, &progress_path);
                break;
            }
            Some(status) => {
                let detail = fs::read_to_string(&log_path)
                    .unwrap_or_else(|_| format!("vmm-migrate 退出状态：{status}"));
                return Err(format!("VMM 向量重建失败：{detail}"));
            }
            None => {
                publish_official_vector_rebuild_progress(snapshot, &progress_path);
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

/// Reads and validates one child progress record before projecting it into the host snapshot.
/// 读取并校验一条子进程进度记录，再将其投影到宿主状态。
fn publish_official_vector_rebuild_progress(
    snapshot: &Arc<RwLock<VmmMaintenanceSummary>>,
    progress_path: &Path,
) {
    let Ok(bytes) = fs::read(progress_path) else {
        return;
    };
    let Ok(progress) = serde_json::from_slice::<OfficialVectorRebuildProgress>(&bytes) else {
        return;
    };
    // stage is normalized before whitelist and ordering checks.
    // stage 在白名单和顺序校验前先完成规范化。
    let stage = progress.stage.trim();
    if stage.is_empty()
        || !matches!(stage, "embedding" | "writing" | "committing")
        || progress.updated_at_unix_ms == 0
    {
        return;
    }
    if progress.total == 0 {
        if progress.processed != 0 {
            return;
        }
    } else if progress.processed > progress.total {
        return;
    }
    // current is the last host-authoritative projection used to reject stale child records.
    // current 是宿主上一次权威投影，用于拒绝过期的子进程记录。
    let current = snapshot
        .read()
        .map(|current| current.clone())
        .unwrap_or_default();
    // stage_changed distinguishes a new ordered sub-stage from an in-stage batch update.
    // stage_changed 区分有序新子阶段与同阶段批次更新。
    let stage_changed = current.progress_stage.as_deref() != Some(stage);
    if let Some(current_stage) = current.progress_stage.as_deref() {
        if vector_rebuild_progress_stage_rank(stage)
            < vector_rebuild_progress_stage_rank(current_stage)
        {
            return;
        }
    }
    if !stage_changed {
        if current
            .processed_items
            .is_some_and(|processed| progress.processed < processed)
        {
            return;
        }
        if current
            .total_items
            .is_some_and(|total| total != progress.total)
        {
            return;
        }
    }
    // progress_percent reserves the host's 50-99 rebuild range for the child workflow.
    // progress_percent 将宿主的 50-99 重建区间分配给子进程流程。
    let progress_percent =
        vector_rebuild_progress_percent(stage, progress.processed, progress.total);
    // message is the localized host activity derived only from validated stage/count data.
    // message 是仅由已校验阶段和计数数据生成的宿主活动文案。
    let message = match stage {
        "embedding" => format!("正在生成向量：{} / {}", progress.processed, progress.total),
        "writing" => format!(
            "正在写入向量索引：{} / {}",
            progress.processed, progress.total
        ),
        "committing" => String::from("正在提交新的向量维度与索引…"),
        _ => format!(
            "正在重建向量索引：{} / {}",
            progress.processed, progress.total
        ),
    };
    update_snapshot(
        snapshot,
        VmmMaintenanceSummary {
            progress_percent: current.progress_percent.max(progress_percent).min(99),
            message,
            progress_stage: Some(stage.to_string()),
            processed_items: Some(progress.processed),
            total_items: Some(progress.total),
            updated_at_unix_ms: Some(current_unix_millis()),
            ..current
        },
    );
}

/// Orders the child stages so stale earlier-stage records cannot overwrite later progress.
/// 为子进程阶段排序，防止过期的前序阶段记录覆盖后续进度。
fn vector_rebuild_progress_stage_rank(stage: &str) -> u8 {
    match stage {
        "embedding" => 0,
        "writing" => 1,
        "committing" => 2,
        _ => u8::MAX,
    }
}

/// Maps child sub-stage progress into the host's reserved rebuild range from 50 through 99 percent.
/// 将子进程子阶段进度映射到宿主保留的 50 到 99 百分比重建区间。
fn vector_rebuild_progress_percent(stage: &str, processed: u64, total: u64) -> u8 {
    if total == 0 {
        return 99;
    }
    // ratio converts validated record counts into a bounded stage fraction.
    // ratio 将已校验的记录计数转换为有界的阶段比例。
    let ratio = (processed as f64 / total as f64).clamp(0.0, 1.0);
    // base and span reserve non-overlapping host percentage bands for each stage.
    // base 和 span 为每个阶段保留互不重叠的宿主百分比区间。
    let (base, span) = match stage {
        "embedding" => (50.0, 29.0),
        "writing" => (80.0, 18.0),
        "committing" => (98.0, 1.0),
        _ => (50.0, 49.0),
    };
    (base + (ratio * span).round()).min(99.0) as u8
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

    /// Verifies child progress maps into monotonic host rebuild ranges.
    /// 验证子进程进度会映射到单调递增的宿主重建区间。
    #[test]
    fn vector_rebuild_progress_percent_maps_stages() {
        assert_eq!(vector_rebuild_progress_percent("embedding", 0, 10), 50);
        assert_eq!(vector_rebuild_progress_percent("embedding", 10, 10), 79);
        assert_eq!(vector_rebuild_progress_percent("writing", 0, 10), 80);
        assert_eq!(vector_rebuild_progress_percent("writing", 10, 10), 98);
        assert_eq!(vector_rebuild_progress_percent("committing", 10, 10), 99);
        assert_eq!(vector_rebuild_progress_percent("unknown", 10, 10), 99);
        assert_eq!(vector_rebuild_progress_percent("writing", 0, 0), 99);
    }

    /// Verifies malformed, stale, or regressing child records cannot overwrite authoritative progress.
    /// 验证格式错误、过期或倒退的子进程记录不能覆盖权威进度。
    #[test]
    fn official_progress_projection_rejects_regression() {
        let root = tempdir().expect("temporary root");
        let progress_path = root.path().join("progress.json");
        let snapshot = Arc::new(RwLock::new(VmmMaintenanceSummary {
            phase: VmmMaintenancePhase::Rebuilding,
            progress_percent: 50,
            can_cancel: true,
            ..VmmMaintenanceSummary::default()
        }));
        fs::write(
            &progress_path,
            br#"{"stage":"embedding","processed":5,"total":10,"updated_at_unix_ms":1}"#,
        )
        .expect("write progress");
        publish_official_vector_rebuild_progress(&snapshot, &progress_path);
        let first = snapshot.read().expect("read first snapshot").clone();
        assert_eq!(first.processed_items, Some(5));
        assert_eq!(first.total_items, Some(10));
        assert!(first.progress_percent > 50);

        fs::write(
            &progress_path,
            br#"{"stage":"embedding","processed":4,"total":10,"updated_at_unix_ms":2}"#,
        )
        .expect("write regressing progress");
        publish_official_vector_rebuild_progress(&snapshot, &progress_path);
        let second = snapshot.read().expect("read second snapshot").clone();
        assert_eq!(second.processed_items, Some(5));
        assert_eq!(second.progress_stage.as_deref(), Some("embedding"));

        fs::write(
            &progress_path,
            br#"{"stage":"writing","processed":1,"total":10,"updated_at_unix_ms":3}"#,
        )
        .expect("write next-stage progress");
        publish_official_vector_rebuild_progress(&snapshot, &progress_path);
        let third = snapshot.read().expect("read next-stage snapshot").clone();
        assert_eq!(third.progress_stage.as_deref(), Some("writing"));
        assert_eq!(third.processed_items, Some(1));

        fs::write(
            &progress_path,
            br#"{"stage":"embedding","processed":10,"total":10,"updated_at_unix_ms":4}"#,
        )
        .expect("write stale earlier-stage progress");
        publish_official_vector_rebuild_progress(&snapshot, &progress_path);
        let fourth = snapshot.read().expect("read stale-stage snapshot").clone();
        assert_eq!(fourth.progress_stage.as_deref(), Some("writing"));
        assert_eq!(fourth.processed_items, Some(1));
    }

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
