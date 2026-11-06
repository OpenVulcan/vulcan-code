//! The Vulcan Code setup program: installs, updates, modifies and removes a per-user installation
//! from the signed release objects, using the same `vulcan-install-core` as the in-app update
//! (plan section 18.6).
//! Vulcan Code 安装器：从已签名的发布对象安装、更新、修改与移除每用户安装，与应用内更新共用
//! `vulcan-install-core`（方案 18.6 节）。

pub mod args;
mod install;
mod integrate;
pub mod paths;
mod prompt;
mod root;
mod selfupdate;
pub mod state;
mod status;
mod uninstall;
#[cfg(windows)]
mod windows;
mod wizard;

use args::{Command, InstallOptions, USAGE};

/// The error a command returns when the user declines to continue or leaves the wizard; nothing was
/// changed, so it is reported as a cancellation, not as a failure.
/// 用户拒绝继续或离开向导时命令返回的错误；未做任何改动，因此报告为取消而非失败。
pub(crate) const CANCELLED: &str = "cancelled";

/// The exit code of a cancelled command, as a shell reports an interrupted program. scripts/install.ps1
/// recognizes it and ends quietly instead of reporting a failure.
/// 被取消命令的退出码，与 shell 报告被中断程序的方式相同。scripts/install.ps1 识别它并安静结束，而不是报告失败。
pub const CANCELLED_EXIT_CODE: i32 = 130;
use vulcan_install_core::names::Platform;

/// Runs the setup program with the arguments after the program name; returns the exit code.
/// 以程序名之后的参数运行安装器；返回退出码。
pub fn run(arguments: &[String]) -> i32 {
    let command = match args::parse(arguments) {
        Ok(command) => command,
        Err(error) => {
            eprintln!("vulcan-code-setup: {error}\n\n{USAGE}");
            return 2;
        }
    };
    // A newer setup program runs `install` and `update` in place of this one.
    // 更新的安装器代替本程序执行 `install` 与 `update`。
    let updating = match &command {
        Command::Install(options) => Some(options.clone()),
        Command::Update(common) => Some(InstallOptions {
            common: common.clone(),
            ..InstallOptions::default()
        }),
        _ => None,
    };
    if let Some(options) = updating {
        match self_update(&options, arguments) {
            Ok(selfupdate::Outcome::Continue) => {}
            Ok(selfupdate::Outcome::Finished(code)) => return code,
            Err(error) => {
                eprintln!("vulcan-code-setup: {error}");
                return 1;
            }
        }
    }
    let result = match command {
        Command::Help => {
            println!("{USAGE}");
            Ok(())
        }
        Command::Version => {
            println!("vulcan-code-setup {}", selfupdate::SETUP_VERSION);
            Ok(())
        }
        Command::Install(options) => install::install(&options),
        Command::Update(options) => install::update(&options),
        Command::Uninstall(options) => uninstall::uninstall(&options),
        Command::Status(options) => status::status(&options),
    };
    match result {
        Ok(()) => 0,
        Err(error) if error == CANCELLED => {
            println!("Cancelled; nothing was changed.");
            CANCELLED_EXIT_CODE
        }
        Err(error) => {
            eprintln!("vulcan-code-setup: {error}");
            1
        }
    }
}

/// Looks for a newer setup program on the channel `install` or `update` will use.
/// 在 `install` 或 `update` 将使用的通道上查找更新的安装器。
fn self_update(
    options: &InstallOptions,
    arguments: &[String],
) -> Result<selfupdate::Outcome, String> {
    let platform = Platform::current().ok_or("this platform is not supported")?;
    let (channel, route) = install::preferences(options, platform);
    selfupdate::check(
        platform,
        channel,
        route,
        options.common.self_update,
        options.common.yes,
        arguments,
    )
}
