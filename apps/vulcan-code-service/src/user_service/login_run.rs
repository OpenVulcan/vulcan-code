//! The Windows login registration: a value under the per-user `Run` key that starts the Core with
//! its tray at sign-in. It needs no elevated rights. The value names the Core inside its version
//! directory, so a Core of a newer current version moves it at startup ([`refresh`]).
//! Windows 登录注册：每用户 `Run` 键下的一个值，在登录时以托盘方式启动 Core，不需要提升权限。该值指向
//! 版本目录中的 Core，因此更新后的当前版本 Core 会在启动时移动它（[`refresh`]）。
//!
//! The registry is changed through the system `reg.exe`, so this crate keeps its no-unsafe rule.
//! 注册表经由系统 `reg.exe` 修改，使本 crate 保持不使用 unsafe 的规则。

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use super::{data_root_is_free, registered_executable, require_distribution, wait_until_ready};
use crate::launch::ARG_SERVICE;

/// The per-user key whose values Windows runs at sign-in.
/// Windows 在登录时运行其值的每用户键。
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

/// Name of the value; Debug and Release own separate data roots and therefore separate values.
/// 值的名称；Debug 与 Release 拥有各自的数据根，因此使用各自的值。
fn value_name() -> &'static str {
    if cfg!(debug_assertions) {
        "Vulcan Code Debug"
    } else {
        "Vulcan Code"
    }
}

/// The system `reg.exe`, never one found on PATH.
/// 系统的 `reg.exe`，从不使用 PATH 中找到的同名程序。
fn reg() -> PathBuf {
    std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join("System32")
        .join("reg.exe")
}

/// Runs `reg.exe` with `args` and returns its output.
/// 以 `args` 运行 `reg.exe` 并返回其输出。
fn run_reg(args: &[&str]) -> Result<std::process::Output, String> {
    Command::new(reg())
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("failed to run reg.exe: {error}"))
}

/// The registered command line; `None` when there is none.
/// 已注册的命令行；没有时为 `None`。
fn read() -> Result<Option<String>, String> {
    let output = run_reg(&["query", RUN_KEY, "/v", value_name()])?;
    if !output.status.success() {
        return Ok(None);
    }
    Ok(parse_query(
        &String::from_utf8_lossy(&output.stdout),
        value_name(),
    ))
}

/// Extracts the data of `name` from `reg query` output, whose value lines read
/// `    <name>    REG_SZ    <data>`.
/// 从 `reg query` 输出中提取 `name` 的数据，其值行形如 `    <name>    REG_SZ    <data>`。
fn parse_query(output: &str, name: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let rest = line.trim_start().strip_prefix(name)?;
        let data = rest.trim_start().strip_prefix("REG_SZ")?;
        Some(data.trim().to_owned())
    })
}

/// Writes `command_line` as the registered value.
/// 将 `command_line` 写为已注册的值。
fn write(command_line: &str) -> Result<(), String> {
    let output = run_reg(&[
        "add",
        RUN_KEY,
        "/v",
        value_name(),
        "/t",
        "REG_SZ",
        "/d",
        command_line,
        "/f",
    ])?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "reg add {RUN_KEY} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

/// The command line that starts `executable` with its tray.
/// 以托盘方式启动 `executable` 的命令行。
fn command_line(executable: &Path) -> String {
    format!("\"{}\" {ARG_SERVICE}", executable.display())
}

/// The executable a registered command line starts.
/// 已注册命令行所启动的可执行文件。
fn executable_of(command_line: &str) -> Option<PathBuf> {
    let quoted = command_line.trim().strip_prefix('"')?;
    let end = quoted.find('"')?;
    Some(PathBuf::from(&quoted[..end]))
}

/// `path` without the `\\?\` prefix that canonical Windows paths carry, which programs started at
/// sign-in may not accept.
/// 去掉 Windows 规范路径带有的 `\\?\` 前缀后的 `path`；登录时启动的程序可能不接受该前缀。
fn plain(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest),
        _ => path.to_path_buf(),
    }
}

/// Whether two paths name the same file, compared as Windows compares names.
/// 两个路径是否指向同一文件，按 Windows 比较名称的方式比较。
fn same_path(left: &Path, right: &Path) -> bool {
    plain(left).to_string_lossy().to_lowercase() == plain(right).to_string_lossy().to_lowercase()
}

/// Starts `executable` with its tray outside this process's console and process group.
/// 在本进程的控制台与进程组之外以托盘方式启动 `executable`。
fn start(executable: &Path) -> Result<Child, String> {
    use std::os::windows::process::CommandExt as _;
    /// Starts the child without a console of its own.
    /// 启动不带自身控制台的子进程。
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    /// Keeps console control events of this process away from the child.
    /// 使本进程的控制台控制事件不影响子进程。
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    Command::new(executable)
        .arg(ARG_SERVICE)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
        .spawn()
        .map_err(|error| format!("failed to start {}: {error}", executable.display()))
}

/// Registers this Core to start with its tray at sign-in, and starts it now unless one runs.
/// 注册本 Core 在登录时以托盘方式启动；没有 Core 在运行时立即启动。
pub(super) fn install() -> Result<String, String> {
    require_distribution()?;
    let executable = plain(&registered_executable()?);
    write(&command_line(&executable))?;
    let registered = format!(
        "Registered {} to start at sign-in: {RUN_KEY}\\{}",
        executable.display(),
        value_name()
    );
    let data_root = crate::service_runtime::authoritative_data_root()?;
    if !data_root_is_free(&data_root)? {
        return Ok(format!(
            "{registered}\nA Vulcan Code service is already running; this registration starts it from your next sign-in."
        ));
    }
    let discovery_path =
        crate::launch::service_launch_description(executable.clone(), data_root).discovery_path;
    let mut child = start(&executable)?;
    let pid = child.id();
    let base_url = wait_until_ready(&discovery_path, || match child.try_wait() {
        Ok(Some(status)) => Err(format!("the service exited during startup ({status})")),
        Ok(None) => Ok(Some(pid)),
        Err(error) => Err(format!("failed to watch the started service: {error}")),
    })?;
    Ok(format!(
        "{registered}\nStarted it with its tray.\nWebUI: {base_url}"
    ))
}

/// Removes the registration; a running Core keeps running until it is quit from its tray.
/// 移除注册；正在运行的 Core 会继续运行，直到从托盘退出。
pub(super) fn uninstall() -> Result<String, String> {
    if read()?.is_none() {
        return Ok(format!(
            "{RUN_KEY}\\{} is not registered; nothing to remove",
            value_name()
        ));
    }
    let output = run_reg(&["delete", RUN_KEY, "/v", value_name(), "/f"])?;
    if !output.status.success() {
        return Err(format!(
            "reg delete {RUN_KEY} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(format!(
        "Removed {RUN_KEY}\\{}; a running Vulcan Code service keeps running until you quit it from its tray.",
        value_name()
    ))
}

/// Moves an existing registration of the same installation to this Core when it runs the current
/// version; registrations of other installations and missing ones stay as they are.
/// 本 Core 运行当前版本时，把同一安装的已有注册移到本 Core；其他安装的注册与不存在的注册保持不变。
pub(super) fn refresh() -> Result<(), String> {
    let Some(registered) = read()? else {
        return Ok(());
    };
    let Some(target) = executable_of(&registered) else {
        return Ok(());
    };
    let own = std::env::current_exe()
        .map_err(|error| format!("failed to resolve the service executable: {error}"))?;
    if same_path(&target, &own) {
        return Ok(());
    }
    let Some((root, true)) = vulcan_app_update::installed_root(&own) else {
        return Ok(());
    };
    let versions = plain(&root.join(vulcan_install_core::install_root::VERSIONS_DIR));
    let inside = plain(&target)
        .to_string_lossy()
        .to_lowercase()
        .starts_with(&format!("{}\\", versions.to_string_lossy().to_lowercase()));
    if !inside {
        return Ok(());
    }
    write(&command_line(&plain(&own)))?;
    tracing::info!(
        "[vulcan-code-service] moved the login registration to {}",
        own.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registered_values_are_read_back_from_reg_query_output() {
        let output = "\r\nHKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Run\r\n    Vulcan Code    REG_SZ    \"C:\\Program Files\\Vulcan\\bin\\core.exe\" --service\r\n\r\n";
        let data = parse_query(output, "Vulcan Code").expect("value");
        assert_eq!(
            executable_of(&data),
            Some(PathBuf::from(r"C:\Program Files\Vulcan\bin\core.exe"))
        );
        assert_eq!(
            data,
            command_line(Path::new(r"C:\Program Files\Vulcan\bin\core.exe"))
        );
        assert_eq!(parse_query(output, "Vulcan Code Debug"), None);
        assert_eq!(executable_of("core.exe --service"), None);
    }

    #[test]
    fn verbatim_prefixes_are_dropped_and_names_compare_without_case() {
        assert_eq!(
            plain(Path::new(r"\\?\C:\Vulcan\core.exe")),
            PathBuf::from(r"C:\Vulcan\core.exe")
        );
        assert_eq!(
            plain(Path::new(r"\\?\UNC\server\share\core.exe")),
            PathBuf::from(r"\\?\UNC\server\share\core.exe")
        );
        assert!(same_path(
            Path::new(r"\\?\C:\Vulcan\Core.exe"),
            Path::new(r"c:\vulcan\core.exe")
        ));
    }
}
