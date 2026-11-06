//! The AppArmor profile the Linux desktop app needs on systems that restrict unprivileged user
//! namespaces (`vulcan-install-core` `apparmor`). Installations made before the setup program
//! installed it get it here, after an in-app update, with the administrator password asked
//! through polkit; once installed it covers every later version.
//! Linux 桌面应用在限制非特权用户命名空间的系统上所需的 AppArmor 配置（`vulcan-install-core` 的
//! `apparmor`）。安装器开始安装它之前的安装在应用内更新后由此获得它，管理员密码经 polkit 询问；
//! 安装一次即覆盖之后的每个版本。

use std::path::PathBuf;
use std::process::{Command, Stdio};

use vulcan_install_core::apparmor::{self, STAGED_PROFILE};
use vulcan_install_core::install_root::UPDATES_DIR;
use vulcan_install_core::names::DESKTOP_COMPONENT;
use vulcan_service_api::AppUpdateDesktopProfileStatus;

use crate::installed::Installed;

/// Exit status of `pkexec` when the password was not given or the person is not authorized.
/// 未提供密码或用户未获授权时 `pkexec` 的退出状态。
const PKEXEC_NOT_AUTHORIZED: i32 = 126;

/// Exit status of `pkexec` when no authentication agent answered or the command could not run.
/// 没有认证代理应答或命令无法运行时 `pkexec` 的退出状态。
const PKEXEC_FAILED: i32 = 127;

/// The profile one installation needs.
/// 一个安装所需的配置。
pub(crate) struct DesktopProfile {
    /// The profile text.
    /// 配置文本。
    needed: String,
    /// Where it is prepared before it is copied into place.
    /// 复制到位之前准备它的位置。
    staged: PathBuf,
}

impl DesktopProfile {
    /// The profile `installed` needs: on Linux, with the desktop app installed and an install
    /// root a profile can name; `None` otherwise.
    /// `installed` 所需的配置：在 Linux 上、已安装桌面应用且安装根目录可被配置写明时给出；否则为 `None`。
    pub(crate) fn of(installed: &Installed) -> Option<Self> {
        if !cfg!(target_os = "linux")
            || !installed
                .components
                .iter()
                .any(|component| component.as_str() == DESKTOP_COMPONENT)
        {
            return None;
        }
        let version_dir = installed.root.version_dir(&installed.version);
        let desktop = apparmor::desktop_executable(&version_dir)?;
        if !version_dir.join(&desktop).is_file() {
            return None;
        }
        let root = std::fs::canonicalize(installed.root.path()).ok()?;
        Some(Self {
            needed: apparmor::profile(&root, &desktop)?,
            staged: installed.root.path().join(UPDATES_DIR).join(STAGED_PROFILE),
        })
    }

    /// What the installed profile lacks; `None` when the system needs none or has it, or when a
    /// profile someone else wrote is there.
    /// 已安装配置的欠缺；系统不需要或已具备配置，或存在他人写出的配置时为 `None`。
    pub(crate) fn status(&self) -> Option<AppUpdateDesktopProfileStatus> {
        if !apparmor::userns_restricted() {
            return None;
        }
        match apparmor::installed(&self.needed) {
            apparmor::Installed::Missing => Some(AppUpdateDesktopProfileStatus::Missing),
            apparmor::Installed::Outdated => Some(AppUpdateDesktopProfileStatus::Outdated),
            apparmor::Installed::Current | apparmor::Installed::Foreign => None,
        }
    }

    /// The line that installs the prepared profile by hand.
    /// 手动安装已准备配置的命令行。
    pub(crate) fn manual_command(&self) -> String {
        apparmor::shell_line(&apparmor::install_commands(&self.staged), Some("sudo"))
    }

    /// Prepares the profile where this user can write, so that the manual command works.
    /// 在本用户可写的位置准备配置，使手动命令可用。
    pub(crate) fn stage(&self) -> Result<(), String> {
        let fail =
            |error: std::io::Error| format!("cannot write {}: {error}", self.staged.display());
        if let Some(parent) = self.staged.parent() {
            std::fs::create_dir_all(parent).map_err(fail)?;
        }
        std::fs::write(&self.staged, &self.needed).map_err(fail)
    }

    /// Installs and loads the profile with administrator rights: directly as root, otherwise
    /// through `pkexec`, whose polkit agent asks for the password in the desktop session.
    /// 以管理员权限安装并加载配置：root 下直接运行，否则经 `pkexec`，由其 polkit 代理在桌面会话中询问密码。
    pub(crate) fn install(&self) -> Result<(), String> {
        self.stage()?;
        let line = apparmor::shell_line(&apparmor::install_commands(&self.staged), None);
        let mut command = if running_as_root() {
            Command::new("/bin/sh")
        } else {
            let mut pkexec = Command::new("pkexec");
            pkexec.arg("/bin/sh");
            pkexec
        };
        let output = command
            .arg("-c")
            .arg(&line)
            .stdin(Stdio::null())
            .output()
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    String::from("pkexec is not installed, so no password can be asked here")
                } else {
                    format!("cannot run pkexec: {error}")
                }
            })?;
        if output.status.success() {
            let _ = std::fs::remove_file(&self.staged);
            return Ok(());
        }
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let reason = match output.status.code() {
            Some(PKEXEC_NOT_AUTHORIZED) => String::from("the administrator password was not given"),
            Some(PKEXEC_FAILED) => String::from(
                "no password prompt answered; polkit may have no authentication agent for this \
                 process",
            ),
            Some(code) => format!("installing the profile failed with exit status {code}"),
            None => String::from("installing the profile was interrupted"),
        };
        Err(if detail.is_empty() {
            reason
        } else {
            format!("{reason}: {detail}")
        })
    }
}

/// Whether this process runs as root.
/// 本进程是否以 root 运行。
fn running_as_root() -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        // `/proc/self` belongs to the effective user of the process reading it.
        // `/proc/self` 属于读取它的进程的有效用户。
        std::fs::metadata("/proc/self").is_ok_and(|metadata| metadata.uid() == 0)
    }
    #[cfg(not(unix))]
    {
        false
    }
}
