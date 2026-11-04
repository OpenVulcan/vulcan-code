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
pub mod state;
mod status;
mod uninstall;
#[cfg(windows)]
mod windows;
mod wizard;

use args::{Command, USAGE};

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
    let result = match command {
        Command::Help => {
            println!("{USAGE}");
            Ok(())
        }
        Command::Install(options) => install::install(&options),
        Command::Update(options) => install::update(&options),
        Command::Uninstall(options) => uninstall::uninstall(&options),
        Command::Status(options) => status::status(&options),
    };
    match result {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("vulcan-code-setup: {error}");
            1
        }
    }
}
