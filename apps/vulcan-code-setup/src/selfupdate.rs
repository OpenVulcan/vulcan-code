//! Updating the setup program itself before `install` or `update` (plan section 18.6): the setup
//! program is released on its own, and a newer one named by the signed setup pointer of the channel
//! replaces this one and runs again with the same arguments. A product release only asks for it
//! through `minSetupVersion`.
//! 在 `install` 或 `update` 之前更新安装器自身（方案 18.6 节）：安装器独立发布，通道的已签名安装器
//! 指针所指的更新版本替换当前安装器，并以相同参数再次运行。产品版本只通过 `minSetupVersion` 提出要求。

use std::path::{Path, PathBuf};
use std::process::Command;
use std::{fmt, io::Write};

use vulcan_install_core::fetch::{Downloader, Route};
use vulcan_install_core::layout::setup_file_key;
use vulcan_install_core::names::{Channel, Platform, ReleaseVersion};
use vulcan_install_core::setup::{SetupFile, SetupPointer};
use vulcan_install_core::signing::BUILT_IN_KEYS;
use vulcan_install_core::update::find_setup_update;

use crate::args::SelfUpdate;
use crate::install::channels;
use crate::prompt::megabytes;

/// The version of this setup program, defined only in its `Cargo.toml`.
/// 本安装器的版本，只在其 `Cargo.toml` 中定义。
pub const SETUP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The argument that keeps the restarted program from updating again.
/// 让重新启动的程序不再更新的参数。
const NO_SELF_UPDATE: &str = "--no-self-update";

/// The version of this setup program.
/// 本安装器的版本。
pub fn version() -> Result<ReleaseVersion, String> {
    SETUP_VERSION
        .parse()
        .map_err(|_| format!("the setup program carries an invalid version {SETUP_VERSION}"))
}

/// What the caller does after the check.
/// 检查之后调用方要做的事。
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Carry on with this program.
    /// 用本程序继续。
    Continue,
    /// The updated program ran in its place and exited with this code (Windows).
    /// 更新后的程序已代为运行，并以此退出码结束（Windows）。
    #[cfg_attr(unix, allow(dead_code))]
    Finished(i32),
}

/// The caller's interface, which determines how progress and the replacement process are shown.
/// 调用方的界面类型，决定如何显示更新进度并启动替换进程。
#[derive(Clone, Copy)]
pub enum Surface {
    /// Report progress to the terminal and keep it attached across the restart.
    /// 向终端报告进度，重启后仍连接同一终端。
    Terminal,
    /// Run on behalf of an installer window that may have no valid standard streams.
    /// 代表可能没有有效标准流的安装窗口运行。
    #[cfg(windows)]
    Graphical,
}

impl Surface {
    /// Writes a progress message when a terminal is in use. An output error cannot stop updating.
    /// 使用终端时写入一条进度消息。输出错误不能中断更新。
    fn announce(self, message: fmt::Arguments<'_>) {
        if matches!(self, Self::Terminal) {
            let _ = writeln!(std::io::stdout().lock(), "{message}");
        }
    }
}

/// Checks `channel` unless opted out, then verifies, replaces and restarts with `arguments`.
/// The `surface` controls progress output and how the child is launched.
/// 未关闭自更新时检查 `channel`，校验后替换本程序并以 `arguments` 重启。
/// `surface` 控制进度输出和子进程的启动方式。
pub fn check(
    platform: Platform,
    channel: Channel,
    route: Route,
    choice: SelfUpdate,
    arguments: &[String],
    surface: Surface,
) -> Result<Outcome, String> {
    let program = std::env::current_exe()
        .map_err(|error| format!("cannot find the setup program: {error}"))?;
    remove_aside(&program);
    if choice == SelfUpdate::Never {
        return Ok(Outcome::Continue);
    }
    let own = version()?;
    // The lookup reaches the network before anything else is shown, so it says what it is doing.
    // 该查询在显示任何其他内容之前访问网络，因此先说明正在做什么。
    surface.announce(format_args!("Looking for a newer setup program..."));
    let mut downloader = crate::route::downloader(route).map_err(|error| error.to_string())?;
    let pointer = match find_setup_update(&mut downloader, BUILT_IN_KEYS, channels(channel), &own) {
        Ok(Some(pointer)) => pointer,
        Ok(None) => return Ok(Outcome::Continue),
        // The release itself still checks `minSetupVersion` before anything is installed.
        // 安装任何内容之前，版本本身仍会检查 `minSetupVersion`。
        Err(error) => {
            surface.announce(format_args!(
                "Could not look for a newer setup program: {error}"
            ));
            return Ok(Outcome::Continue);
        }
    };
    let newer = &pointer.version;
    let file = pointer
        .program(platform)
        .map_err(|error| error.to_string())?;
    surface.announce(format_args!(
        "Downloading setup program {newer} ({})...",
        megabytes(file.size)
    ));
    replace(&mut downloader, &program, &pointer, file)?;
    restart(&program, arguments, surface)
}

/// Where the running program is moved before it is replaced on Windows.
/// Windows 上替换正在运行的程序之前，将其移到的位置。
fn aside(program: &Path) -> PathBuf {
    let mut path = program.as_os_str().to_owned();
    path.push(".old");
    PathBuf::from(path)
}

/// Removes the program a previous self-update moved aside; a copy still running stays until later.
/// 删除先前自更新移开的程序；仍在运行的副本留待之后删除。
fn remove_aside(program: &Path) {
    let _ = std::fs::remove_file(aside(program));
}

/// Downloads and verifies the new program beside `program`, then puts it in its place.
/// 在 `program` 旁下载并校验新程序，然后放到它的位置上。
fn replace(
    downloader: &mut Downloader,
    program: &Path,
    pointer: &SetupPointer,
    file: &SetupFile,
) -> Result<(), String> {
    let staged = program.with_file_name(format!(".{}.download", file.file));
    downloader
        .download_file(
            &setup_file_key(&pointer.version, &file.file),
            &staged,
            file.size,
            file.sha256,
        )
        .map_err(|error| format!("cannot download the setup program: {error}"))?;
    let fail = |error: std::io::Error| format!("cannot replace {}: {error}", program.display());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755)).map_err(fail)?;
        std::fs::rename(&staged, program).map_err(fail)
    }
    #[cfg(not(unix))]
    {
        // A running program cannot be overwritten on Windows, but it can be renamed.
        // Windows 上无法覆盖正在运行的程序，但可以改名。
        let aside = aside(program);
        let _ = std::fs::remove_file(&aside);
        std::fs::rename(program, &aside).map_err(fail)?;
        if let Err(error) = std::fs::rename(&staged, program) {
            let _ = std::fs::rename(&aside, program);
            return Err(fail(error));
        }
        Ok(())
    }
}

/// The arguments the updated program runs with: the same, except that it does not update again.
/// 更新后的程序运行时的参数：与原参数相同，只是不再更新。
fn restart_arguments(arguments: &[String]) -> Vec<String> {
    let mut next = Vec::with_capacity(arguments.len() + 1);
    let mut arguments = arguments.iter();
    while let Some(argument) = arguments.next() {
        if argument == "--self-update" {
            continue;
        }
        next.push(argument.clone());
        if crate::args::takes_separate_value(argument) {
            next.extend(arguments.next().cloned());
        }
    }
    next.push(String::from(NO_SELF_UPDATE));
    next
}

/// Runs the updated `program` with `arguments` and preserves the caller's interface.
/// 使用 `arguments` 运行更新后的 `program`，保留调用方的界面类型。
fn restart(program: &Path, arguments: &[String], surface: Surface) -> Result<Outcome, String> {
    let mut command = Command::new(program);
    command.args(restart_arguments(arguments));
    #[cfg(windows)]
    if matches!(surface, Surface::Graphical) {
        use std::os::windows::process::CommandExt;
        use std::process::Stdio;

        // A GUI relaunch must not inherit handles left behind by FreeConsole.
        // 图形界面重启时不能继承 FreeConsole 留下的失效句柄。
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    }
    surface.announce(format_args!("Restarting the updated setup program..."));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let error = command.exec();
        Err(format!("cannot run {}: {error}", program.display()))
    }
    #[cfg(not(unix))]
    {
        let status = command
            .status()
            .map_err(|error| format!("cannot run {}: {error}", program.display()))?;
        Ok(Outcome::Finished(status.code().unwrap_or(1)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_built_in_version_is_a_release_version() {
        assert!(version().is_ok());
    }

    #[test]
    fn the_restarted_program_keeps_the_arguments_but_never_updates_again() {
        let arguments: Vec<String> = [
            "install",
            "--self-update",
            "--mode",
            "terminal",
            "--prefix",
            "--self-update",
            "--route=china",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        assert_eq!(
            restart_arguments(&arguments),
            [
                "install",
                "--mode",
                "terminal",
                "--prefix",
                "--self-update",
                "--route=china",
                NO_SELF_UPDATE
            ]
        );
        // The replacement installer uses the chosen route without reopening the selection page.
        // 替换后的安装器沿用选定线路，无需再次打开选择页。
        let Ok(crate::args::Command::Install(options)) =
            crate::args::parse(&restart_arguments(&arguments))
        else {
            panic!("install expected");
        };
        assert_eq!(options.common.route, Some(Route::China));
    }
}
