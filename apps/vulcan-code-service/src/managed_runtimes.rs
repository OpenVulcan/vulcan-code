//! Installs the managed Python and Node runtimes into this distribution ahead of packaging.
//! 在打包前把受管 Python 与 Node 运行时安装进本发行物。
//!
//! Core installs missing runtimes on its first start, but a release package must already carry
//! them, and a freshly synchronized distribution has never run Core. This command runs Core's own
//! installer against the same directories and the same lock, so packaging never depends on an
//! earlier launch and never keeps a second installer.
//! Core 首次启动时会安装缺失的运行时，但 release 包必须预先携带它们，而刚同步的发行物从未运行过
//! Core。本命令以相同目录与相同锁运行 Core 自身的安装器，使打包不依赖先前的启动，也不另设安装器。

use vulcan_service_runtime::managed_runtime_bootstrap::{
    ManagedRuntimeBootstrapStatus, bootstrap_managed_runtimes,
};

/// Installs any missing managed runtime into the distribution that holds this executable, then exits.
/// 将缺失的受管运行时安装到本可执行文件所在的发行物后退出。
pub const ARG_PREPARE_MANAGED_RUNTIMES: &str = "--prepare-managed-runtimes";

/// Installs missing runtimes and returns the summary printed to the operator.
/// 安装缺失的运行时并返回打印给操作者的摘要。
///
/// # Returns
/// 返回值
/// The prepared directory, or the installer's diagnostic when any runtime is still unusable.
/// 返回已准备的目录；任一运行时仍不可用时返回安装器诊断。
pub fn prepare_managed_runtimes() -> Result<String, String> {
    // The installer's lock, downloads and caches stay in the user cache, so the prepared
    // distribution holds nothing but the runtimes themselves.
    // 安装器的锁、下载与缓存留在用户缓存中，因此准备好的发行物只包含运行时本身。
    let work_root = vulcan_service_runtime::luaskills_bridge::managed_runtime_work_root();
    let distribution_root = vulcan_runtime_assets::resolve_managed_runtime_distribution_root()
        .ok_or_else(|| String::from("no distribution root was found beside this executable"))?;
    std::fs::create_dir_all(&distribution_root)
        .map_err(|error| format!("failed to create {}: {error}", distribution_root.display()))?;
    let report = bootstrap_managed_runtimes(&work_root, &distribution_root, |event| {
        eprintln!("{}", event.message);
    });
    match report.status {
        ManagedRuntimeBootstrapStatus::Ready | ManagedRuntimeBootstrapStatus::Installed => Ok(
            format!("Managed runtimes ready in {}", distribution_root.display()),
        ),
        _ => Err(report.diagnostic),
    }
}
