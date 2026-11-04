//! Stable Windows launcher for the multi-version install layout.
//! 多版本安装布局的稳定 Windows 启动器。
//!
//! ```text
//! <root>\VulcanCode.exe        desktop launcher (`vulcan-code-launcher`, GUI subsystem)
//! <root>\cli\vulcan-tui.exe    PATH forwarder (`vulcan-tui-forwarder`, console subsystem)
//! <root>\current.json          {"version":"0.1.3","path":"versions\\0.1.3"}
//! <root>\versions\<version>\   complete release tree with its runtime-layout.json
//! ```
//!
//! Both programs read `current.json`, then start the executable that the version's
//! `runtime-layout.json` names, passing through the raw command line, console, standard streams
//! and exit code. The child always runs from its real version directory, so switching versions
//! never disturbs a running process. Each program has a fixed role: one binary cannot serve both,
//! because the PE subsystem decides whether Windows opens a console and whether shells wait.
//!
//! 两个程序都读取 `current.json`，再启动该版本 `runtime-layout.json` 指定的可执行文件，并透传原始命令行、
//! 控制台、标准流与退出码。子进程始终从真实版本目录运行，因此切换版本不会干扰运行中的进程。每个程序的
//! 角色固定：同一个二进制无法同时承担两者，因为 PE 子系统决定 Windows 是否打开控制台、Shell 是否等待。

use std::ffi::OsString;
use std::fmt;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

mod trial;
#[cfg(windows)]
mod windows;

#[cfg(test)]
mod tests;

/// File in the install root that names the current version directory.
/// 安装根目录中指明当前版本目录的文件。
pub const CURRENT_FILE: &str = "current.json";

/// File in each version directory that names its executables.
/// 每个版本目录中指明其可执行文件的文件。
pub const RUNTIME_LAYOUT_FILE: &str = "runtime-layout.json";

/// What one launcher program starts.
/// 一个启动程序所启动的目标。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// `<root>\VulcanCode.exe` starts the desktop app.
    /// `<root>\VulcanCode.exe` 启动桌面应用。
    Desktop,
    /// `<root>\cli\vulcan-tui.exe` starts the terminal client.
    /// `<root>\cli\vulcan-tui.exe` 启动终端客户端。
    Terminal,
}

impl Role {
    /// Key of the executable in `runtime-layout.json`.
    /// 可执行文件在 `runtime-layout.json` 中的键。
    fn layout_key(self) -> &'static str {
        match self {
            Self::Desktop => "desktopExecutable",
            Self::Terminal => "tuiExecutable",
        }
    }
}

/// A reason the launcher cannot start its target.
/// 启动器无法启动目标的原因。
#[derive(Debug, PartialEq, Eq)]
pub struct LaunchError(String);

impl fmt::Display for LaunchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Derives the install root from the launcher's own path.
/// 根据启动器自身路径推导安装根目录。
///
/// The desktop launcher lives in the root and the forwarder in `<root>\cli`.
/// 桌面启动器位于根目录，转发器位于 `<root>\cli`。
pub fn install_root(role: Role, executable: &Path) -> Result<PathBuf, LaunchError> {
    let directory = executable.parent();
    let root = match role {
        Role::Desktop => directory,
        Role::Terminal => directory.and_then(Path::parent),
    };
    root.filter(|root| !root.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            LaunchError(format!(
                "{} is not inside a Vulcan Code installation",
                executable.display()
            ))
        })
}

/// Resolves the executable the current version provides for `role`.
/// 解析当前版本为 `role` 提供的可执行文件。
pub fn resolve_target(root: &Path, role: Role) -> Result<PathBuf, LaunchError> {
    let current = read_json(&root.join(CURRENT_FILE))?;
    let version_dir = root.join(relative_entry(
        string_field(&current, "path", CURRENT_FILE)?,
        CURRENT_FILE,
    )?);
    let layout = read_json(&version_dir.join(RUNTIME_LAYOUT_FILE))?;
    let target = version_dir.join(relative_entry(
        string_field(&layout, role.layout_key(), RUNTIME_LAYOUT_FILE)?,
        RUNTIME_LAYOUT_FILE,
    )?);
    if target.is_file() {
        Ok(target)
    } else {
        Err(LaunchError(format!("{} is missing", target.display())))
    }
}

/// Reads one JSON document.
/// 读取一个 JSON 文档。
fn read_json(path: &Path) -> Result<serde_json::Value, LaunchError> {
    let bytes = std::fs::read(path)
        .map_err(|error| LaunchError(format!("cannot read {}: {error}", path.display())))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| LaunchError(format!("{} is not valid JSON: {error}", path.display())))
}

/// Reads one non-empty string field.
/// 读取一个非空字符串字段。
fn string_field<'a>(
    document: &'a serde_json::Value,
    key: &str,
    file: &str,
) -> Result<&'a str, LaunchError> {
    document
        .get(key)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| LaunchError(format!("{file} has no \"{key}\" string")))
}

/// Accepts only a relative path made of plain names, so an entry cannot leave its base directory.
/// 只接受由普通名称组成的相对路径，使条目无法离开其基准目录。
fn relative_entry(value: &str, file: &str) -> Result<PathBuf, LaunchError> {
    let path = PathBuf::from(value);
    if path
        .components()
        .all(|component| matches!(component, Component::Normal(_)))
    {
        Ok(path)
    } else {
        Err(LaunchError(format!(
            "{file} names {value}, which is not a plain relative path"
        )))
    }
}

/// Returns the arguments that follow the program name in a raw Windows command line.
/// 返回原始 Windows 命令行中位于程序名之后的参数部分。
///
/// The program name follows the C runtime rule: a leading quote runs to the next quote and no
/// escapes apply; otherwise it ends at the first space or tab. The rest is forwarded verbatim, so
/// quoting survives exactly as the caller wrote it.
/// 程序名遵循 C 运行时规则：以引号开头时延续到下一个引号且不处理转义；否则在首个空格或制表符处结束。
/// 其余部分原样转发，调用方写下的引号因此完全保留。
pub fn command_line_tail(line: &[u16]) -> &[u16] {
    const QUOTE: u16 = b'"' as u16;
    const SPACE: u16 = b' ' as u16;
    const TAB: u16 = b'\t' as u16;
    let name_end = if line.first() == Some(&QUOTE) {
        line[1..]
            .iter()
            .position(|unit| *unit == QUOTE)
            .map_or(line.len(), |index| index + 2)
    } else {
        line.iter()
            .position(|unit| *unit == SPACE || *unit == TAB)
            .unwrap_or(line.len())
    };
    let rest = &line[name_end..];
    let start = rest
        .iter()
        .position(|unit| *unit != SPACE && *unit != TAB)
        .unwrap_or(rest.len());
    &rest[start..]
}

/// Starts the target for `role`, waits for it and returns its exit code; failures are reported to the user.
/// 启动 `role` 的目标，等待其结束并返回退出码；失败时向用户报告。
pub fn run(role: Role) -> i32 {
    match launch(role) {
        Ok(code) => code,
        Err(error) => {
            report(role, &error);
            1
        }
    }
}

/// Resolves, starts and waits for the target.
/// 解析、启动并等待目标。
fn launch(role: Role) -> Result<i32, LaunchError> {
    let executable = std::env::current_exe()
        .map_err(|error| LaunchError(format!("cannot locate the launcher: {error}")))?;
    let root = install_root(role, &executable)?;
    if role == Role::Desktop {
        // Only the desktop launch counts startup attempts of a freshly updated version.
        // 只有桌面启动统计刚更新版本的启动尝试。
        trial::advance(&root)?;
    }
    let target = resolve_target(&root, role)?;
    let mut command = Command::new(&target);
    forward_arguments(&mut command);
    if role == Role::Terminal {
        // The client owns Ctrl+C; the forwarder only has to outlive it to return its exit code.
        // Ctrl+C 由终端客户端处理；转发器只需活得比它久，才能返回其退出码。
        #[cfg(windows)]
        windows::ignore_console_interrupts();
    }
    let status = command
        .status()
        .map_err(|error| LaunchError(format!("cannot start {}: {error}", target.display())))?;
    Ok(status.code().unwrap_or(1))
}

/// Forwards the launcher's own arguments to `command`.
/// 将启动器自身的参数转发给 `command`。
fn forward_arguments(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        use std::os::windows::process::CommandExt;
        let line = windows::command_line();
        let tail = command_line_tail(&line);
        if !tail.is_empty() {
            command.raw_arg(OsString::from_wide(tail));
        }
    }
    #[cfg(not(windows))]
    {
        command.args(std::env::args_os().skip(1).collect::<Vec<OsString>>());
    }
}

/// Tells the user why nothing started: a message box for the desktop, standard error for the terminal.
/// 告诉用户为何没有启动：桌面使用消息框，终端使用标准错误。
fn report(role: Role, error: &LaunchError) {
    let message = format!(
        "Vulcan Code cannot start: {error}. Reinstall Vulcan Code to repair the installation."
    );
    match role {
        Role::Terminal => eprintln!("vulcan-tui: {message}"),
        Role::Desktop => {
            #[cfg(windows)]
            windows::show_error(&message);
            #[cfg(not(windows))]
            eprintln!("{message}");
        }
    }
}
