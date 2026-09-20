//! Core-owned browser installation stored outside disposable build artifacts.
//! Core 持有的浏览器安装，保存在可清理构建产物之外。

use crate::runtime_layout::{
    read_runtime_layout_manifest_with_path, resolve_layout_entry, runtime_layout_root,
};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
};

/// Reads the explicitly installed runtime, returning None only when no installation is recorded.
/// 读取明确安装的运行时；仅当没有安装记录时返回 None。
/// data_root is the Core user root and host_entry identifies the pinned package; errors require repair.
/// data_root 是 Core 用户根，host_entry 标识固定版本包；发生错误时需要修复。
pub(crate) fn installed_chromium(
    data_root: &Path,
    host_entry: &Path,
) -> Result<Option<PathBuf>, String> {
    let root = data_root.join("browser-runtime");
    let manifest_path = root.join("current.json");
    let text = match std::fs::read_to_string(&manifest_path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Cannot read browser installation: {error}")),
    };
    let manifest: serde_json::Value = serde_json::from_str(&text)
        .map_err(|error| format!("Invalid browser installation manifest: {error}"))?;
    let app_root = host_entry
        .parent()
        .and_then(Path::parent)
        .ok_or("Invalid browser host entry")?;
    // Development dist and packaged app have explicit, different dependency roots.
    // 开发 dist 与发行 app 使用明确且不同的依赖根。
    let dependency_root = if app_root.file_name().is_some_and(|name| name == "dist") {
        app_root
            .parent()
            .ok_or("Invalid development browser host directory")?
    } else {
        app_root
    };
    let package: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dependency_root.join("node_modules/playwright-core/package.json"))
            .map_err(|error| format!("Cannot read browser host package: {error}"))?,
    )
    .map_err(|error| format!("Invalid browser host package: {error}"))?;
    if manifest["schemaVersion"] != 1
        || manifest["playwrightVersion"].as_str().is_none()
        || manifest["playwrightVersion"] != package["version"]
    {
        return Err("Installed Chromium belongs to a different browser host version. Repair the Core browser component.".into());
    }
    let relative = manifest["chromiumExecutable"]
        .as_str()
        .ok_or("Browser installation has no executable declaration")?;
    let executable = resolve_layout_entry(&root, relative)?;
    if !executable.is_file() {
        return Err(format!(
            "Installed Chromium is missing: {}. Repair the Core browser component.",
            executable.display()
        ));
    }
    let canonical = executable
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if !canonical.starts_with(root.canonicalize().map_err(|error| error.to_string())?) {
        return Err("Installed Chromium escapes its Core-owned directory.".into());
    }
    Ok(Some(executable))
}

/// Resolves only the declared distribution or the established debug workspace tools.
/// 仅解析已声明发行物或既有开发工作区工具。
/// Returns the executable and installer script, or an actionable distribution error.
/// 返回程序与安装脚本，或可操作的发行物错误。
fn installer_tools() -> Result<(PathBuf, PathBuf), String> {
    let (executable, host_entry) = match read_runtime_layout_manifest_with_path() {
        Ok((manifest, manifest_path)) => {
            let root = runtime_layout_root(&manifest_path)?;
            (
                resolve_layout_entry(&root, &manifest.desktop_executable)?,
                resolve_layout_entry(&root, &manifest.browser_host_entry)?,
            )
        }
        Err(error)
            if cfg!(debug_assertions)
                && crate::runtime_layout::explicit_runtime_layout_path()?.is_none() =>
        {
            let desktop =
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/vulcan-code-desktop");
            let binary = if cfg!(target_os = "windows") {
                "electron.exe"
            } else if cfg!(target_os = "macos") {
                "Electron.app/Contents/MacOS/Electron"
            } else {
                "electron"
            };
            let executable = desktop.join("node_modules/electron/dist").join(binary);
            if !executable.is_file() {
                return Err(error);
            }
            (executable, desktop.join("dist/chromium-host/main.js"))
        }
        Err(error) => return Err(error),
    };
    let installer = host_entry.with_file_name("install.js");
    if !executable.is_file() || !host_entry.is_file() || !installer.is_file() {
        return Err("Browser installer is missing from the desktop runtime. Rebuild or update the desktop package.".into());
    }
    Ok((executable, installer))
}

/// Builds the installer capability without downloading or modifying any runtime files.
/// 构造安装器能力，不下载或修改任何运行文件。
/// data_root selects the user-owned installation directory; returns a blocking worker callback.
/// data_root 选择用户持有的安装目录；返回阻塞工作回调。
pub(crate) fn installer(
    data_root: PathBuf,
) -> Result<vulcan_service_kernel::browser::BrowserRuntimeInstaller, String> {
    let (executable, entry) = installer_tools()?;
    Ok(Arc::new(move || {
        let root = data_root.join("browser-runtime");
        std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        let log_path = root.join("install.log");
        let log = std::fs::File::create(&log_path).map_err(|error| error.to_string())?;
        let mut command = Command::new(&executable);
        command
            .arg(&entry)
            .arg(&root)
            .env("ELECTRON_RUN_AS_NODE", "1")
            .stdin(Stdio::null())
            .stdout(log.try_clone().map_err(|error| error.to_string())?)
            .stderr(log);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let status = command
            .status()
            .map_err(|error| format!("Cannot run browser installer: {error}"))?;
        if !status.success() {
            let log = std::fs::read_to_string(&log_path).map_err(|error| error.to_string())?;
            let tail: String = log
                .chars()
                .rev()
                .take(6000)
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            return Err(format!(
                "Browser installation failed ({status}). Log: {}\n{tail}",
                log_path.display()
            ));
        }
        crate::service_runtime::service_browser_host_runtime(&data_root)
    }))
}
