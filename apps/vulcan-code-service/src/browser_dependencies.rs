//! Installs the Linux system packages Playwright recommends for the built-in Chromium.
//! 安装 Playwright 为内置 Chromium 推荐的 Linux 系统软件包。
//!
//! Windows and macOS packages already carry every browser component (Chromium, ffmpeg, and on
//! Windows winldd). Linux libraries, fonts, and Xvfb belong to the distribution's package manager,
//! so this command runs the packaged Playwright's own `install-deps`, which knows each release's
//! package names and asks for sudo itself.
//! Windows 与 macOS 发行包已带齐全部浏览器组件（Chromium、ffmpeg，Windows 上还有 winldd）。
//! Linux 的库、字体与 Xvfb 归发行版包管理器所有，因此本命令运行随包 Playwright 自带的
//! `install-deps`，由它按各发行版本选择软件包名称并自行请求 sudo。

/// Installs the system packages the built-in browser needs, then exits.
/// 安装内置浏览器所需的系统软件包后退出。
pub const ARG_INSTALL_BROWSER_DEPENDENCIES: &str = "--install-browser-dependencies";

/// Installs the packages and returns the summary printed to the operator.
/// 安装软件包并返回打印给操作者的摘要。
pub fn install_browser_dependencies() -> Result<String, String> {
    #[cfg(target_os = "linux")]
    {
        linux::install()
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err(String::from(
            "only Linux needs system packages for the built-in browser; Windows and macOS packages already include every browser component",
        ))
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::path::PathBuf;
    use std::process::Command;

    use crate::runtime_layout::{
        read_runtime_layout_manifest_with_path, resolve_layout_entry, runtime_layout_root,
    };

    /// Runs the packaged Playwright CLI's `install-deps chromium` with the terminal attached.
    /// 在连接终端的情况下运行随包 Playwright CLI 的 `install-deps chromium`。
    pub(super) fn install() -> Result<String, String> {
        let (manifest, manifest_path) =
            read_runtime_layout_manifest_with_path().map_err(|error| {
                format!("only a built distribution carries the browser runtime: {error}")
            })?;
        if manifest.chromium_executable.is_none() {
            return Err(format!(
                "runtime layout {} declares no built-in browser",
                manifest_path.display()
            ));
        }
        let root = runtime_layout_root(&manifest_path)?;
        let node = resolve_layout_entry(&root, &manifest.browser_host_executable)?;
        let entry = resolve_layout_entry(&root, &manifest.browser_host_entry)?;
        let entry_directory = entry
            .parent()
            .ok_or_else(|| format!("browser host entry {} has no parent", entry.display()))?;
        // Resolve playwright-core exactly as the host does: from the host entry's directory.
        // 与宿主完全相同地解析 playwright-core：从宿主入口所在目录解析。
        let resolved = Command::new(&node)
            .args([
                "-e",
                "process.stdout.write(require.resolve('playwright-core/package.json'))",
            ])
            .current_dir(entry_directory)
            .output()
            .map_err(|error| format!("failed to run {}: {error}", node.display()))?;
        if !resolved.status.success() {
            return Err(format!(
                "the browser runtime has no playwright-core: {}",
                String::from_utf8_lossy(&resolved.stderr).trim()
            ));
        }
        let package = PathBuf::from(String::from_utf8_lossy(&resolved.stdout).trim());
        let cli = package
            .parent()
            .ok_or_else(|| String::from("playwright-core package path has no parent"))?
            .join("cli.js");
        let status = Command::new(&node)
            .arg(&cli)
            .args(["install-deps", "chromium"])
            .status()
            .map_err(|error| format!("failed to run {}: {error}", cli.display()))?;
        if !status.success() {
            return Err(format!(
                "Playwright install-deps failed ({status}); rerun in a terminal where sudo can ask for your password"
            ));
        }
        Ok(String::from(
            "Installed the system packages Playwright recommends for the built-in Chromium.",
        ))
    }
}
