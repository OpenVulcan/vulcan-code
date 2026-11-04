//! Startup trial of a freshly switched version (plan section 8).
//! 刚切换的版本的启动试运行（方案第八节）。
//!
//! After an incremental update the service makes the new version current and leaves
//! `updates/startup-trial.json`; the new service removes it once it is ready. Each desktop launch
//! counts one attempt; once the marker's `max_attempts` were made without the marker going away,
//! the launcher points `current.json` back at the previous version and leaves `updates/rollback.json`
//! for the restored service to announce. The marker owns the bound, so no copy of it lives here.
//! The file names and fields mirror `crates/vulcan-install-core/src/install_root.rs`
//! (`STARTUP_TRIAL_FILE`, `ROLLBACK_FILE`, `StartupTrial`, `Rollback`); change both together. The
//! launcher keeps only `serde_json` as a dependency, so it cannot use that crate.
//!
//! 增量更新后，服务将新版本设为当前版本并留下 `updates/startup-trial.json`；新服务就绪后删除它。
//! 每次桌面启动计一次尝试；标记中的 `max_attempts` 次尝试后标记仍在时，启动器把 `current.json` 指回
//! 上一版本，并留下 `updates/rollback.json` 供恢复后的服务提示。上限由标记持有，此处不保留副本。
//! 文件名与字段镜像 `crates/vulcan-install-core/src/install_root.rs`（`STARTUP_TRIAL_FILE`、
//! `ROLLBACK_FILE`、`StartupTrial`、`Rollback`），两处须同时修改。启动器只依赖 `serde_json`，
//! 因此不能使用该 crate。

use std::fs::{File, TryLockError};
use std::path::Path;

use serde_json::{Value, json};

use crate::{CURRENT_FILE, LaunchError};

/// Directory below the install root that holds update state.
/// 安装根目录下保存更新状态的目录。
const UPDATES_DIR: &str = "updates";

/// Directory below the install root that holds the installed versions.
/// 安装根目录下保存已安装版本的目录。
const VERSIONS_DIR: &str = "versions";

/// Install lock below [`UPDATES_DIR`], shared with the service.
/// [`UPDATES_DIR`] 下与服务共享的安装锁。
const LOCK_FILE: &str = ".lock";

/// Marker of a version on trial.
/// 试运行版本的标记。
const STARTUP_TRIAL_FILE: &str = "startup-trial.json";

/// Marker of a version switch made by the launcher.
/// 启动器所做版本切换的标记。
const ROLLBACK_FILE: &str = "rollback.json";

/// Suffix of a file written before it replaces its destination.
/// 替换目标前先写入的文件后缀。
const PARTIAL_SUFFIX: &str = ".partial";

/// What the trial step did.
/// 试运行步骤所做的事。
#[derive(Debug, PartialEq, Eq)]
pub enum TrialOutcome {
    /// No version is on trial, or the service holds the install lock right now.
    /// 没有版本在试运行，或服务此刻持有安装锁。
    Idle,
    /// One more attempt of the new version was counted.
    /// 新版本又计入一次尝试。
    Counted,
    /// The previous version is current again.
    /// 上一版本重新成为当前版本。
    RolledBack,
}

/// Counts one launch of a version on trial, or switches back when its attempts are used up.
/// 为试运行版本计入一次启动，或在尝试次数用完时切换回去。
pub fn advance(root: &Path) -> Result<TrialOutcome, LaunchError> {
    let updates = root.join(UPDATES_DIR);
    let trial_path = updates.join(STARTUP_TRIAL_FILE);
    if !trial_path.is_file() {
        return Ok(TrialOutcome::Idle);
    }
    let lock = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(updates.join(LOCK_FILE))
        .map_err(|error| io_error("open the install lock", &error))?;
    match lock.try_lock() {
        Ok(()) => {}
        // A running service is changing the install; it settles the trial itself.
        // 运行中的服务正在修改安装；试运行由它自行处理。
        Err(TryLockError::WouldBlock) => return Ok(TrialOutcome::Idle),
        Err(TryLockError::Error(error)) => {
            return Err(io_error("take the install lock", &error));
        }
    }
    let mut trial = crate::read_json(&trial_path)?;
    let field = |key: &str| {
        trial
            .get(key)
            .and_then(Value::as_u64)
            .ok_or_else(|| LaunchError(format!("{STARTUP_TRIAL_FILE} has no \"{key}\" number")))
    };
    let attempts = field("attempts")?;
    let max_attempts = field("max_attempts")?;
    let version = crate::string_field(&trial, "version", STARTUP_TRIAL_FILE)?.to_string();
    let previous = crate::string_field(&trial, "previous", STARTUP_TRIAL_FILE)?.to_string();
    let current = crate::read_json(&root.join(CURRENT_FILE))?;
    if crate::string_field(&current, "version", CURRENT_FILE)? != version {
        // Something else, such as the full installer, made another version current since.
        // 此后有其他程序（如完整安装包）将另一个版本设为当前版本。
        std::fs::remove_file(&trial_path)
            .map_err(|error| io_error("remove the startup trial", &error))?;
        return Ok(TrialOutcome::Idle);
    }
    if attempts < max_attempts {
        trial["attempts"] = json!(attempts + 1);
        write_atomic(&trial_path, &trial)?;
        return Ok(TrialOutcome::Counted);
    }
    let previous_path = format!("{VERSIONS_DIR}{}{previous}", std::path::MAIN_SEPARATOR);
    let previous_dir = root.join(crate::relative_entry(&previous_path, STARTUP_TRIAL_FILE)?);
    if !previous_dir.is_dir() {
        return Err(LaunchError(format!(
            "{version} did not start and {previous} is no longer installed"
        )));
    }
    write_atomic(
        &root.join(CURRENT_FILE),
        &json!({ "version": previous, "path": previous_path }),
    )?;
    write_atomic(
        &updates.join(ROLLBACK_FILE),
        &json!({ "failed": version, "restored": previous }),
    )?;
    std::fs::remove_file(&trial_path)
        .map_err(|error| io_error("remove the startup trial", &error))?;
    Ok(TrialOutcome::RolledBack)
}

/// Replaces `path` with `value` atomically.
/// 以 `value` 原子替换 `path`。
fn write_atomic(path: &Path, value: &Value) -> Result<(), LaunchError> {
    let mut partial = path.as_os_str().to_owned();
    partial.push(PARTIAL_SUFFIX);
    std::fs::write(&partial, value.to_string())
        .map_err(|error| io_error("write an update marker", &error))?;
    std::fs::rename(&partial, path).map_err(|error| io_error("replace an update marker", &error))
}

/// Describes a failed file operation.
/// 描述失败的文件操作。
fn io_error(what: &str, error: &std::io::Error) -> LaunchError {
    LaunchError(format!("cannot {what}: {error}"))
}
