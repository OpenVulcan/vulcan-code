//! Updating the setup program itself before `install` or `update` (plan section 18.6): the newest
//! release on the channel publishes its own setup program, which replaces this one and runs again
//! with the same arguments.
//! 在 `install` 或 `update` 之前更新安装器自身（方案 18.6 节）：通道上的最新版本发布了自己的安装器，
//! 它替换当前安装器，并以相同参数再次运行。

use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::Command;

use vulcan_install_core::fetch::{Downloader, Route};
use vulcan_install_core::layout::installer_key;
use vulcan_install_core::manifest::{Download, DownloadKind};
use vulcan_install_core::names::{Channel, Platform, ReleaseVersion};
use vulcan_install_core::signing::BUILT_IN_KEYS;
use vulcan_install_core::update::{Offer, find_update};

use crate::args::SelfUpdate;
use crate::install::{USER_AGENT, channels};
use crate::prompt::{confirm, megabytes};

/// The product version this setup program was built with (`build.rs`).
/// 构建本安装器时的产品版本（`build.rs`）。
pub const SETUP_VERSION: &str = env!("VULCAN_SETUP_VERSION");

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

/// Looks for a newer setup program on `channel` and, when one is wanted or required, replaces this
/// program with it and runs it with `arguments`.
/// 在 `channel` 上查找更新的安装器；需要或要求更新时，用它替换本程序并以 `arguments` 运行。
pub fn check(
    platform: Platform,
    channel: Channel,
    route: Route,
    choice: SelfUpdate,
    yes: bool,
    arguments: &[String],
) -> Result<Outcome, String> {
    let program = std::env::current_exe()
        .map_err(|error| format!("cannot find the setup program: {error}"))?;
    remove_aside(&program);
    if choice == SelfUpdate::Never {
        return Ok(Outcome::Continue);
    }
    let own = version()?;
    let mut downloader =
        Downloader::new(route.origins(), USER_AGENT).map_err(|error| error.to_string())?;
    let offer = match find_update(
        &mut downloader,
        BUILT_IN_KEYS,
        platform,
        channels(channel),
        &own,
    ) {
        Ok(Some(offer)) => offer,
        Ok(None) => return Ok(Outcome::Continue),
        // The release itself is checked again before anything is installed.
        // 安装任何内容之前，还会再次检查版本本身。
        Err(error) => {
            println!("Could not look for a newer setup program: {error}");
            return Ok(Outcome::Continue);
        }
    };
    let release = offer.manifest.version.clone();
    let required = offer
        .manifest
        .min_setup_version
        .as_ref()
        .filter(|minimum| **minimum > own)
        .cloned();
    let refuse = |minimum: &ReleaseVersion| {
        format!("{release} needs setup program {minimum} or newer, and this one is {own}")
    };
    let Some(download) = setup_download(&offer) else {
        return match &required {
            Some(minimum) => Err(format!("{}; it publishes none", refuse(minimum))),
            None => Ok(Outcome::Continue),
        };
    };
    let wanted = match choice {
        SelfUpdate::Always => true,
        SelfUpdate::Never => false,
        SelfUpdate::Ask if yes || !std::io::stdin().is_terminal() => required.is_some(),
        SelfUpdate::Ask => confirm(
            &format!(
                "A newer setup program {release} is available (this one is {own}). Update it and \
                 continue?"
            ),
            false,
        )?,
    };
    if !wanted {
        return match &required {
            Some(minimum) => Err(refuse(minimum)),
            None => Ok(Outcome::Continue),
        };
    }
    println!(
        "Downloading setup program {release} ({})...",
        megabytes(download.size)
    );
    replace(&mut downloader, &program, &release, download)?;
    restart(&program, arguments)
}

/// The setup program the offered release publishes.
/// 所提供版本发布的安装器。
fn setup_download(offer: &Offer) -> Option<&Download> {
    offer
        .manifest
        .downloads
        .iter()
        .find(|download| download.kind == DownloadKind::Setup)
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
    release: &ReleaseVersion,
    download: &Download,
) -> Result<(), String> {
    let staged = program.with_file_name(format!(".{}.download", download.file));
    downloader
        .download_file(
            &installer_key(release, &download.file),
            &staged,
            download.size,
            download.sha256,
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
    let mut next: Vec<String> = arguments
        .iter()
        .filter(|argument| argument.as_str() != "--self-update")
        .cloned()
        .collect();
    next.push(String::from(NO_SELF_UPDATE));
    next
}

/// Runs the updated program in place of this one.
/// 以更新后的程序代替本程序运行。
fn restart(program: &Path, arguments: &[String]) -> Result<Outcome, String> {
    let mut command = Command::new(program);
    command.args(restart_arguments(arguments));
    println!("Restarting the updated setup program...");
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
        let arguments: Vec<String> = ["install", "--self-update", "--mode", "terminal"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(
            restart_arguments(&arguments),
            ["install", "--mode", "terminal", NO_SELF_UPDATE]
        );
        assert!(crate::args::parse(&restart_arguments(&arguments)).is_ok());
    }
}
