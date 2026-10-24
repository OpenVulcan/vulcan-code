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

/// Reports whether the browser dependency command was requested on its own.
/// 报告是否单独请求了浏览器依赖安装命令。
///
/// # Parameters
/// 参数
/// `args` are the full process arguments including the executable name.
/// `args` 是包含可执行文件名的完整进程参数。
///
/// # Returns
/// 返回值
/// `Ok(true)` when the flag is the only argument, `Ok(false)` when absent, or an error when it is
/// combined with anything else.
/// 该参数为唯一参数时返回 `Ok(true)`；未出现时返回 `Ok(false)`；与其他参数混用时返回错误。
pub fn browser_dependencies_requested(args: &[String]) -> Result<bool, String> {
    let tail = args.get(1..).unwrap_or_default();
    if !tail
        .iter()
        .any(|arg| arg == ARG_INSTALL_BROWSER_DEPENDENCIES)
    {
        return Ok(false);
    }
    match tail {
        [_] => Ok(true),
        _ => Err(format!(
            "{ARG_INSTALL_BROWSER_DEPENDENCIES} must be the only argument"
        )),
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds process arguments from one executable name and tail flags.
    /// 从可执行文件名与尾部参数构建进程参数。
    fn args(tail: &[&str]) -> Vec<String> {
        std::iter::once("vulcan-code-service")
            .chain(tail.iter().copied())
            .map(String::from)
            .collect()
    }

    /// Verifies the flag is recognized alone and rejected beside other arguments.
    /// 验证该参数单独使用时被识别，与其他参数混用时被拒绝。
    #[test]
    fn install_flag_must_stand_alone() {
        assert_eq!(browser_dependencies_requested(&args(&[])), Ok(false));
        assert_eq!(
            browser_dependencies_requested(&args(&["--headless"])),
            Ok(false)
        );
        assert_eq!(
            browser_dependencies_requested(&args(&[ARG_INSTALL_BROWSER_DEPENDENCIES])),
            Ok(true)
        );
        assert!(
            browser_dependencies_requested(&args(&[
                ARG_INSTALL_BROWSER_DEPENDENCIES,
                "--headless"
            ]))
            .is_err()
        );
    }
}
