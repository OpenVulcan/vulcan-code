//! The macOS login registration: a launchd agent in `~/Library/LaunchAgents` that starts the Core
//! with its menu bar icon in the user's session. It needs no elevated rights and runs the stable
//! `<root>/current/<rel>` path, so updates need no new registration.
//! macOS 登录注册：`~/Library/LaunchAgents` 中的 launchd 代理，在用户会话中以菜单栏图标方式启动 Core。它不需要
//! 提升权限，并运行稳定路径 `<root>/current/<rel>`，因此更新无需重新注册。

use std::path::Path;

use crate::launch::ARG_SERVICE;

/// Label of the agent; Debug and Release own separate data roots and therefore separate agents.
/// 代理的标签；Debug 与 Release 拥有各自的数据根，因此使用各自的代理。
fn label() -> &'static str {
    if cfg!(debug_assertions) {
        "com.openvulcan.vulcancode.service.debug"
    } else {
        "com.openvulcan.vulcancode.service"
    }
}

/// Escapes `text` for an XML element.
/// 为 XML 元素转义 `text`。
fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Renders the agent that runs `executable` with its menu bar icon, restarting it only after a
/// failure; `path` is the installing shell's `PATH`, kept so the agent finds the same tools.
/// The started replacement of an in-app update leaves the agent's process group, so launchd does
/// not stop it with the old process.
/// 渲染以菜单栏图标方式运行 `executable` 的代理，仅在故障后重启；`path` 是安装时 shell 的 `PATH`，使代理
/// 找到相同的工具。应用内更新启动的替代进程离开代理的进程组，因此 launchd 不会随旧进程一起停止它。
fn render_plist(executable: &Path, path: Option<&str>) -> String {
    let environment = path
        .map(|path| {
            format!(
                "  <key>EnvironmentVariables</key>\n  <dict>\n    <key>PATH</key>\n    <string>{}</string>\n  </dict>\n",
                xml(path)
            )
        })
        .unwrap_or_default();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n\
         <dict>\n\
         \x20 <key>Label</key>\n\
         \x20 <string>{label}</string>\n\
         \x20 <key>ProgramArguments</key>\n\
         \x20 <array>\n\
         \x20   <string>{executable}</string>\n\
         \x20   <string>{ARG_SERVICE}</string>\n\
         \x20 </array>\n\
         \x20 <key>RunAtLoad</key>\n\
         \x20 <true/>\n\
         \x20 <key>KeepAlive</key>\n\
         \x20 <dict>\n\
         \x20   <key>SuccessfulExit</key>\n\
         \x20   <false/>\n\
         \x20 </dict>\n\
         \x20 <key>AbandonProcessGroup</key>\n\
         \x20 <true/>\n\
         \x20 <key>ProcessType</key>\n\
         \x20 <string>Interactive</string>\n\
         {environment}\
         </dict>\n\
         </plist>\n",
        label = xml(label()),
        executable = xml(&executable.to_string_lossy()),
    )
}

/// The process id `launchctl print` reports for a running job.
/// `launchctl print` 为运行中的作业报告的进程号。
fn running_pid(print: &str) -> Option<u32> {
    print
        .lines()
        .find_map(|line| line.trim().strip_prefix("pid = "))
        .and_then(|pid| pid.trim().parse().ok())
}

#[cfg(target_os = "macos")]
mod launchd {
    use std::path::PathBuf;
    use std::process::{Command, Output, Stdio};

    use super::super::{
        registered_executable, require_distribution, require_free_data_root, wait_until_ready,
    };
    use super::{label, render_plist, running_pid};

    /// Path of the agent below `~/Library/LaunchAgents`.
    /// 代理位于 `~/Library/LaunchAgents` 下的路径。
    fn plist_path() -> Result<PathBuf, String> {
        Ok(vulcan_runtime_assets::user_home_dir()
            .ok_or_else(|| String::from("failed to resolve user home directory"))?
            .join("Library")
            .join("LaunchAgents")
            .join(format!("{}.plist", label())))
    }

    /// The launchd domain of this user's graphical session, `gui/<uid>`.
    /// 本用户图形会话的 launchd 域 `gui/<uid>`。
    fn domain() -> Result<String, String> {
        let output = Command::new("/usr/bin/id")
            .arg("-u")
            .output()
            .map_err(|error| format!("failed to run id -u: {error}"))?;
        let uid = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if !output.status.success() || uid.is_empty() {
            return Err(String::from("failed to read the user id"));
        }
        Ok(format!("gui/{uid}"))
    }

    /// Runs `launchctl` with `args` and returns its output.
    /// 以 `args` 运行 `launchctl` 并返回其输出。
    fn launchctl(args: &[&str]) -> Result<Output, String> {
        Command::new("/bin/launchctl")
            .args(args)
            .stdin(Stdio::null())
            .output()
            .map_err(|error| format!("failed to run launchctl: {error}"))
    }

    /// Runs `launchctl` with `args`, turning a non-zero exit into its error text.
    /// 以 `args` 运行 `launchctl`，非零退出时返回其错误文本。
    fn launchctl_checked(args: &[&str]) -> Result<Output, String> {
        let output = launchctl(args)?;
        if output.status.success() {
            Ok(output)
        } else {
            Err(format!(
                "launchctl {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            ))
        }
    }

    /// Writes the agent, loads it into the session, and waits for readiness.
    /// 写入代理、将其载入会话并等待就绪。
    pub(in super::super) fn install() -> Result<String, String> {
        require_distribution()?;
        let executable = registered_executable()?;
        let path = plist_path()?;
        let domain = domain()?;
        let target = format!("{domain}/{}", label());
        // Reinstalling replaces a running registration, so release its lock before checking it.
        // 重新安装会替换正在运行的注册，因此检查锁之前先让其释放。
        if path.exists() {
            let _ = launchctl(&["bootout", &target]);
        }
        let data_root = crate::service_runtime::authoritative_data_root()?;
        require_free_data_root(&data_root)?;
        let directory = path
            .parent()
            .ok_or_else(|| String::from("launch agent path has no parent"))?;
        std::fs::create_dir_all(directory)
            .map_err(|error| format!("failed to create {}: {error}", directory.display()))?;
        let search_path = std::env::var("PATH").ok();
        // A same-directory temporary file keeps launchd from ever reading a partial agent.
        // 同目录临时文件确保 launchd 不会读到不完整的代理。
        let staged = path.with_extension("plist.tmp");
        std::fs::write(&staged, render_plist(&executable, search_path.as_deref()))
            .map_err(|error| format!("failed to write {}: {error}", staged.display()))?;
        std::fs::rename(&staged, &path)
            .map_err(|error| format!("failed to install {}: {error}", path.display()))?;
        let path_text = path.to_string_lossy().into_owned();
        launchctl_checked(&["bootstrap", &domain, &path_text])?;
        let discovery_path =
            crate::launch::service_launch_description(executable, data_root).discovery_path;
        // Until launchd reports a process, no discovery document can be the agent's own.
        // 在 launchd 报告进程之前，任何发现文档都不可能属于该代理。
        let base_url = wait_until_ready(&discovery_path, || {
            let print = launchctl_checked(&["print", &target])?;
            Ok(Some(
                running_pid(&String::from_utf8_lossy(&print.stdout)).unwrap_or(0),
            ))
        })
        .map_err(|error| format!("{error}; inspect `launchctl print {target}`"))?;
        Ok(format!(
            "Installed and started {label}: {path}\nWebUI: {base_url}\nIt starts with your login session.\nStatus: launchctl print {target}",
            label = label(),
            path = path.display(),
        ))
    }

    /// Unloads the agent, which stops the Core gracefully, then deletes its file.
    /// 卸载代理（优雅停止 Core），随后删除其文件。
    pub(in super::super) fn uninstall() -> Result<String, String> {
        let path = plist_path()?;
        if !path.exists() {
            return Ok(format!("{} is not installed; nothing to remove", label()));
        }
        let target = format!("{}/{}", domain()?, label());
        // An agent that is not loaded has nothing to stop.
        // 未载入的代理无需停止。
        let _ = launchctl(&["bootout", &target]);
        std::fs::remove_file(&path)
            .map_err(|error| format!("failed to remove {}: {error}", path.display()))?;
        Ok(format!(
            "Stopped and removed {}: {}",
            label(),
            path.display()
        ))
    }
}

#[cfg(target_os = "macos")]
pub(super) use launchd::{install, uninstall};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_agent_runs_the_exact_executable_with_escaped_values() {
        let plist = render_plist(
            Path::new("/Users/a&b/VulcanCode/current/bin/vulcan-code-service"),
            Some("/usr/bin:/opt/<x>"),
        );
        assert!(plist.contains(
            "<string>/Users/a&amp;b/VulcanCode/current/bin/vulcan-code-service</string>"
        ));
        assert!(plist.contains(&format!("<string>{ARG_SERVICE}</string>")));
        assert!(plist.contains("<string>/usr/bin:/opt/&lt;x&gt;</string>"));
        assert!(plist.contains(&format!("<string>{}</string>", label())));
        assert!(!render_plist(Path::new("/x"), None).contains("EnvironmentVariables"));
    }

    #[test]
    fn the_running_pid_is_read_from_launchctl_print() {
        assert_eq!(running_pid("\tstate = running\n\tpid = 4242\n"), Some(4242));
        assert_eq!(running_pid("\tstate = not running\n"), None);
    }
}
