//! Removing an installation item by item, as recorded; user data stays unless `--purge` is given.
//! 按记录逐项移除安装；除非给出 `--purge`，用户数据保留。

use std::path::{Path, PathBuf};

use vulcan_install_core::install_root::{
    CURRENT_FILE, CURRENT_LINK, InstallRoot, UPDATES_DIR, VERSIONS_DIR,
};
use vulcan_install_core::names::Platform;

use crate::args::UninstallOptions;
use crate::install::resolve_root;
use crate::integrate::{self, CLI_DIR};
use crate::paths;
use crate::prompt::confirm;
use crate::report::Plain;
use crate::root::{self, Current, Description, LEGACY_MARKER, Ownership, Version};
use crate::state::{Item, SETUP_DIR};

/// Launchers `scripts/install.sh` wrote into `~/.local/bin`.
/// `scripts/install.sh` 写入 `~/.local/bin` 的启动器。
const LEGACY_LAUNCHERS: [&str; 3] = ["vulcan-code", "vulcan-code-service", "vulcan-tui"];

/// `vulcan-code-setup uninstall`.
/// `vulcan-code-setup uninstall`。
pub fn uninstall(options: &UninstallOptions) -> Result<(), String> {
    let platform = Platform::current().ok_or("this platform is not supported")?;
    let root_path = resolve_root(options.common.prefix.as_deref(), platform)?;
    let items = match root::ownership(&root_path)? {
        Ownership::Owned(state) => state.items,
        Ownership::Legacy => legacy_items(&root_path)?,
        Ownership::Inno(uninstaller) => return uninstall_inno(&root_path, &uninstaller, options),
        Ownership::Fresh => {
            return Err(format!(
                "Vulcan Code is not installed in {}",
                root_path.display()
            ));
        }
    };
    let root = InstallRoot::new(&root_path);
    let version_dir = current_version_dir(&root_path)?;
    let version = version_dir
        .as_deref()
        .and_then(|dir| Version::read(dir).ok());
    let user_data = purge_target(version_dir.as_deref(), options.purge)?;
    println!("Remove Vulcan Code from {}", root_path.display());
    let running = running_programs(&root_path);
    if !running.is_empty() {
        println!("  first closing the running {}", running.join(", "));
    }
    for item in &items {
        if let Some(line) = describe(item) {
            println!("  {line}");
        }
    }
    match &user_data {
        Some(dir) => println!("  your data in {}", dir.display()),
        None => println!("Your sessions, settings and memory are kept; --purge removes them."),
    }
    if !confirm("Continue?", options.common.yes)? {
        return Err(String::from(crate::CANCELLED));
    }
    // Nothing is changed while a program of this root still runs, so a failure leaves the
    // installation whole.
    // 本根目录的程序仍在运行时不做任何改动，因此失败时安装保持完整。
    close_programs(&root_path)?;

    let lock = root.lock().map_err(|error| error.to_string())?;
    let reporter = Plain::new(options.common.yes);
    let mut problems = Vec::new();
    // The service runs from the version directory, so it goes first.
    // 服务从版本目录运行，因此最先移除。
    if items.contains(&Item::UserService)
        && let Err(error) =
            integrate::remove(&Item::UserService, &root_path, version.as_ref(), &reporter)
    {
        problems.push(error);
    }
    remove_programs(&root_path)?;
    for item in items.iter().rev() {
        if *item != Item::UserService
            && let Err(error) = integrate::remove(item, &root_path, version.as_ref(), &reporter)
        {
            problems.push(error);
        }
    }
    drop(lock);
    let _ = std::fs::remove_dir_all(root_path.join(UPDATES_DIR));

    if let Some(dir) = user_data.filter(|dir| dir.exists()) {
        purge(&dir, options.common.yes)?;
    }
    remove_setup(&root_path)?;
    for problem in &problems {
        eprintln!("{problem}");
    }
    println!("Vulcan Code was removed from {}.", root_path.display());
    Ok(())
}

/// How many times the running programs are looked for and ended; the desktop app may start the
/// service again while it is being closed.
/// 查找并结束运行中程序的次数；桌面应用可能在关闭过程中再次启动服务。
#[cfg(windows)]
const CLOSE_ROUNDS: usize = 3;

/// How long one program may take to exit after it was ended.
/// 单个程序被结束后退出可用的最长时间。
#[cfg(windows)]
const CLOSE_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

/// File names of the programs of this root that are running; Windows cannot remove a running
/// program, while Linux and macOS can and stop the service through the user service removal.
/// 本根目录中正在运行的程序文件名；Windows 无法删除运行中的程序，Linux 与 macOS 可以，并通过移除用户
/// 服务停止服务。
fn running_programs(root: &Path) -> Vec<String> {
    #[cfg(windows)]
    {
        let mut names: Vec<String> = crate::windows::processes_below(root)
            .into_iter()
            .filter_map(|(_, program)| {
                program
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .collect();
        names.sort();
        names.dedup();
        names
    }
    #[cfg(not(windows))]
    {
        let _ = root;
        Vec::new()
    }
}

/// Ends every running program of this root, or fails without having changed anything.
/// 结束本根目录中所有正在运行的程序；失败时不做任何改动。
fn close_programs(root: &Path) -> Result<(), String> {
    #[cfg(windows)]
    for _ in 0..CLOSE_ROUNDS {
        let running = crate::windows::processes_below(root);
        if running.is_empty() {
            return Ok(());
        }
        for (pid, _) in running {
            crate::windows::end_process(pid, CLOSE_WAIT);
        }
    }
    let left = running_programs(root);
    if left.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "cannot close {}; quit Vulcan Code, including the service in the notification area, \
             and run the uninstall again",
            left.join(", ")
        ))
    }
}

/// The directory of the current version, when one is installed.
/// 当前版本所在的目录（若已安装）。
fn current_version_dir(root_path: &Path) -> Result<Option<PathBuf>, String> {
    let root = InstallRoot::new(root_path);
    Ok(match root::current(&root)? {
        Current::Version(version) => Some(root.version_dir(&version)),
        Current::Legacy(name) => Some(root_path.join(VERSIONS_DIR).join(name)),
        Current::None => None,
    })
}

/// The user data `--purge` removes; the installed service names its data directory, so the setup
/// program never guesses it.
/// `--purge` 删除的用户数据；已安装的服务给出其数据目录，因此安装器从不猜测。
fn purge_target(version_dir: Option<&Path>, purge: bool) -> Result<Option<PathBuf>, String> {
    match (purge, version_dir) {
        (false, _) => Ok(None),
        (true, Some(dir)) => user_directory(dir).map(Some),
        (true, None) => Err(String::from(
            "the current version is missing, so its data directory cannot be found; uninstall \
             without --purge and remove the data yourself",
        )),
    }
}

/// How long the Windows uninstaller may take to remove itself after its first process has exited.
/// Windows 卸载程序在其第一个进程退出后删除自身可用的最长时间。
const INNO_UNINSTALL_WAIT: std::time::Duration = std::time::Duration::from_secs(120);

/// Uninstalls a root the Windows installer owns by running its uninstaller, then purges the data.
/// 运行 Windows 安装程序的卸载程序来卸载其所属的根目录，然后删除数据。
fn uninstall_inno(
    root_path: &Path,
    uninstaller: &Path,
    options: &UninstallOptions,
) -> Result<(), String> {
    let version_dir = current_version_dir(root_path)?;
    let user_data = purge_target(version_dir.as_deref(), options.purge)?;
    println!(
        "Remove Vulcan Code from {} with the Windows uninstaller",
        root_path.display()
    );
    match &user_data {
        Some(dir) => println!("  your data in {}", dir.display()),
        None => println!("Your sessions, settings and memory are kept; --purge removes them."),
    }
    if !confirm("Continue?", options.common.yes)? {
        return Err(String::from(crate::CANCELLED));
    }
    // The confirmation above is the only question; the uninstaller shows no dialogs of its own.
    // 上面的确认是唯一的提问；卸载程序不再显示自己的对话框。
    let status = std::process::Command::new(uninstaller)
        .args(["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART"])
        .current_dir(std::env::temp_dir())
        .status()
        .map_err(|error| format!("cannot run {}: {error}", uninstaller.display()))?;
    if !status.success() {
        return Err(format!(
            "{} failed with {status}; close Vulcan Code and run the uninstall again",
            uninstaller.display()
        ));
    }
    // The uninstaller continues from a copy in the temporary directory and removes itself last.
    // 卸载程序从临时目录中的副本继续运行，最后删除自身。
    let deadline = std::time::Instant::now() + INNO_UNINSTALL_WAIT;
    while uninstaller.exists() {
        if std::time::Instant::now() > deadline {
            return Err(format!(
                "{} did not finish within {} seconds",
                uninstaller.display(),
                INNO_UNINSTALL_WAIT.as_secs()
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    if let Some(dir) = user_data.filter(|dir| dir.exists()) {
        purge(&dir, options.common.yes)?;
    }
    remove_setup(root_path)?;
    println!("Vulcan Code was removed from {}.", root_path.display());
    Ok(())
}

/// What `scripts/install.sh` created outside its root: launchers carrying the shared marker, and
/// the user service when its unit runs a program from this root.
/// `scripts/install.sh` 在其根目录之外创建的内容：带共享标记的启动器，以及单元运行本根目录程序时的
/// 用户服务。
fn legacy_items(root: &Path) -> Result<Vec<Item>, String> {
    let bin = paths::bin_dir()?;
    let mut items: Vec<Item> = LEGACY_LAUNCHERS
        .iter()
        .map(|name| Item::File {
            path: bin.join(name),
        })
        .collect();
    let real = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let owned = format!("{}/{VERSIONS_DIR}/", real.display());
    if std::fs::read_to_string(paths::systemd_user_unit()?).is_ok_and(|text| text.contains(&owned))
    {
        items.push(Item::UserService);
    }
    Ok(items)
}

/// The Vulcan user directory (plan section 18.6: `~/.vulcan`), the parent of the data directory
/// the service of `version_dir` reports.
/// Vulcan 用户目录（方案 18.6 节：`~/.vulcan`），即 `version_dir` 的服务所报告数据目录的上级目录。
fn user_directory(version_dir: &Path) -> Result<PathBuf, String> {
    let data_root = Description::read(version_dir)?.data_root;
    data_root
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| format!("{} has no parent directory", data_root.display()))
}

/// A line naming `item` for the confirmation, when it is outside the root.
/// 用于确认时列出 `item` 的一行（当它位于根目录之外时）。
fn describe(item: &Item) -> Option<String> {
    match item {
        Item::File { path } | Item::Bundle { path } => Some(path.display().to_string()),
        Item::Directory { .. } => None,
        Item::UserPath { dir } => Some(format!("{} from the user PATH", dir.display())),
        Item::UninstallEntry { .. } => Some(String::from("the entry in Settings > Apps")),
        Item::Shortcut { path } => Some(format!("the shortcut {}", path.display())),
        Item::ToastActivator { .. } => None,
        Item::UserService => Some(String::from(
            "the user service started with your login session",
        )),
        Item::Linger { user } => Some(format!("lingering of {user}")),
        Item::AppArmorProfile { path } => Some(format!(
            "the AppArmor profile {} (asks for administrator rights)",
            path.display()
        )),
    }
}

/// Removes the versions and everything that points into them.
/// 移除各版本以及指向它们的一切。
fn remove_programs(root: &Path) -> Result<(), String> {
    let in_use = |path: &Path, error: std::io::Error| {
        format!(
            "cannot remove {}: {error}; close Vulcan Code and run the uninstall again",
            path.display()
        )
    };
    for file in [CURRENT_FILE, CURRENT_LINK, LEGACY_MARKER] {
        let path = root.join(file);
        if std::fs::symlink_metadata(&path).is_ok_and(|metadata| !metadata.is_dir()) {
            std::fs::remove_file(&path).map_err(|error| in_use(&path, error))?;
        }
    }
    for dir in [VERSIONS_DIR, CLI_DIR] {
        let path = root.join(dir);
        if path.is_dir() {
            std::fs::remove_dir_all(&path).map_err(|error| in_use(&path, error))?;
        }
    }
    Ok(())
}

/// Deletes the user data directory after listing it.
/// 列出用户数据目录后将其删除。
fn purge(user_data: &Path, yes: bool) -> Result<(), String> {
    let mut names: Vec<String> = std::fs::read_dir(user_data)
        .map_err(|error| format!("cannot read {}: {error}", user_data.display()))?
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    println!("{} holds: {}", user_data.display(), names.join(", "));
    if !confirm(
        "Delete it, including all sessions, settings and memory?",
        yes,
    )? {
        println!("Kept {}.", user_data.display());
        return Ok(());
    }
    std::fs::remove_dir_all(user_data)
        .map_err(|error| format!("cannot remove {}: {error}", user_data.display()))
}

/// Removes the setup program, its state and the root when nothing else is left in it.
/// 移除安装器及其状态，根目录中没有其他内容时一并移除。
fn remove_setup(root: &Path) -> Result<(), String> {
    let setup = root.join(SETUP_DIR);
    let running_from_setup = std::env::current_exe()
        .ok()
        .and_then(|running| running.canonicalize().ok())
        .zip(setup.canonicalize().ok())
        .is_some_and(|(running, setup)| running.starts_with(setup));
    if cfg!(windows) && running_from_setup {
        // Anything besides the setup directory belongs to the user and keeps the root.
        // 安装器目录之外的任何内容都属于用户，根目录因此保留。
        let others = std::fs::read_dir(root)
            .map(|entries| {
                entries
                    .flatten()
                    .any(|entry| entry.file_name() != std::ffi::OsStr::new(SETUP_DIR))
            })
            .unwrap_or(true);
        if others {
            println!("Kept {} and its remaining contents.", root.display());
        }
        return remove_after_exit(&setup, (!others).then_some(root));
    }
    if setup.exists() {
        std::fs::remove_dir_all(&setup)
            .map_err(|error| format!("cannot remove {}: {error}", setup.display()))?;
    }
    // Anything else left in the root belongs to the user; the root is never removed recursively.
    // 根目录中剩下的其他内容属于用户；绝不递归删除根目录。
    if std::fs::remove_dir(root).is_err() && root.exists() {
        println!("Kept {} and its remaining contents.", root.display());
    }
    Ok(())
}

/// A running program cannot delete itself on Windows, so a detached `cmd` removes the setup
/// directory, and the root when it is given, once this process has exited.
/// Windows 上运行中的程序无法删除自身，因此由分离的 `cmd` 在本进程退出后删除安装器目录，以及给出时的
/// 根目录。
#[cfg(windows)]
fn remove_after_exit(setup: &Path, root: Option<&Path>) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use windows_sys::Win32::System::Threading::{CREATE_NO_WINDOW, DETACHED_PROCESS};
    // `ping` waits about two seconds without needing a console, unlike `timeout`.
    // `ping` 无需控制台即可等待约两秒，`timeout` 则需要。
    let mut script = format!(
        "ping -n 3 127.0.0.1 >nul & rmdir /s /q \"{}\"",
        setup.display()
    );
    if let Some(root) = root {
        script.push_str(&format!(" & rmdir \"{}\" 2>nul", root.display()));
    }
    std::process::Command::new("cmd")
        .args(["/d", "/c"])
        .raw_arg(script)
        .current_dir(std::env::temp_dir())
        .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
        .spawn()
        .map_err(|error| format!("cannot start the cleanup of {}: {error}", setup.display()))?;
    Ok(())
}

/// Only Windows cannot delete a running program.
/// 只有 Windows 无法删除正在运行的程序。
#[cfg(not(windows))]
fn remove_after_exit(_setup: &Path, _root: Option<&Path>) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn programs_are_removed_but_foreign_root_content_is_kept() {
        let temp = tempfile::tempdir().expect("temp dir");
        let root = temp.path().join("root");
        std::fs::create_dir_all(root.join(VERSIONS_DIR).join("0.1.0/bin")).expect("version");
        std::fs::create_dir_all(root.join(SETUP_DIR)).expect("setup");
        std::fs::write(root.join(CURRENT_FILE), "{}").expect("current");
        std::fs::write(root.join("notes.txt"), "mine").expect("foreign");
        remove_programs(&root).expect("programs");
        remove_setup(&root).expect("setup");
        assert!(!root.join(VERSIONS_DIR).exists());
        assert!(!root.join(CURRENT_FILE).exists());
        assert!(!root.join(SETUP_DIR).exists());
        assert!(root.join("notes.txt").exists());
        std::fs::remove_file(root.join("notes.txt")).expect("foreign");
        remove_setup(&root).expect("setup");
        assert!(!root.exists());
    }
}
