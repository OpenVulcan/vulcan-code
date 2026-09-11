//! One-time private data-root selection for a dedicated integration-test process.
//! 专用集成测试进程的一次性私有数据根选择。

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Exact canonical root shared by all startup threads in this test process, never serialized.
/// 此测试进程全部启动线程共享的精确规范根，不进行序列化。
static ISOLATED_PROCESS_ROOT: OnceLock<PathBuf> = OnceLock::new();

/// Install one existing absolute test directory before any service or configuration access.
/// 在访问任何服务或配置之前，安装一个已经存在的绝对测试目录。
/// `root` belongs to the calling test; returns an error for invalid paths or a second installation.
/// `root` 属于调用测试；路径非法或第二次安装时返回错误。
/// Isolation lives until process exit so background threads cannot outlive a temporary override.
/// 隔离持续到进程退出，避免后台线程在临时覆盖结束后仍然存活。
pub fn install_isolated_process_root(root: &Path) -> Result<(), String> {
    if !root.is_absolute() {
        return Err("Integration-test root must be absolute".into());
    }
    // Canonicalization binds all derived paths to one exact existing directory.
    // 规范化将全部派生路径绑定到一个精确的既有目录。
    let canonical = root
        .canonicalize()
        .map_err(|error| format!("Resolve integration-test root: {error}"))?;
    if !canonical.is_dir() {
        return Err("Integration-test root must be a directory".into());
    }
    ISOLATED_PROCESS_ROOT
        .set(canonical)
        .map_err(|_| "Integration-test root was already installed".into())
}

/// Return the isolated user namespace only after the dedicated process explicitly installs it.
/// 仅在专用进程明确安装后返回隔离用户命名空间。
/// No arguments; absence retains normal path resolution without reading environment overrides.
/// 无参数；未安装时保留普通路径解析，不读取环境覆盖。
pub(super) fn isolated_user_root() -> Option<PathBuf> {
    ISOLATED_PROCESS_ROOT
        .get()
        .map(|root| root.join("user-vulcan"))
}
