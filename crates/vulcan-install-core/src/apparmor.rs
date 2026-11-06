//! The AppArmor profile that lets the Linux desktop app create user namespaces, shared by the
//! setup program and the in-app updater so both write the same bytes.
//!
//! Ubuntu 23.10 and later restrict unprivileged user namespaces to programs whose AppArmor profile
//! allows them. Electron then falls back to its setuid sandbox helper, which an installation
//! without root cannot set up, and stops at start. A program inherits the profile of what started
//! it, so the app may start from the desktop folder, whose icons run under a permissive profile,
//! and still fail from the dock or the app menu. The profile attaches to the desktop program of
//! every version below one install root, so it is installed once and keeps working across updates.
//!
//! 允许 Linux 桌面应用创建用户命名空间的 AppArmor 配置，由安装器与应用内更新器共用，使两者写出相同的字节。
//!
//! Ubuntu 23.10 及以后只允许 AppArmor 配置许可的程序使用非特权用户命名空间。Electron 此时退回其 setuid
//! 沙箱助手，而无 root 的安装无法设置它，于是在启动时停止。程序继承启动者的配置，因此应用可能从桌面
//! 文件夹（其图标在宽松配置下运行）启动成功，却从程序坞或应用菜单启动失败。该配置附着到一个安装根目录下
//! 每个版本的桌面程序，因此只需安装一次，跨更新持续有效。

use std::path::Path;

use crate::install_root::{GENERATED_MARKER, RUNTIME_LAYOUT_FILE, VERSIONS_DIR};

/// Where the profile is installed.
/// 配置的安装位置。
pub const PROFILE_PATH: &str = "/etc/apparmor.d/vulcan-code";

/// Kernel setting that is `1` while unprivileged user namespaces are restricted by AppArmor.
/// AppArmor 限制非特权用户命名空间时值为 `1` 的内核设置。
pub const USERNS_RESTRICTION: &str = "/proc/sys/kernel/apparmor_restrict_unprivileged_userns";

/// Name of the file in which the profile is prepared where the user can write, before it is
/// copied into place as root.
/// 在用户可写位置准备配置所用的文件名，随后以 root 复制到位。
pub const STAGED_PROFILE: &str = "vulcan-code.apparmor";

/// Characters a profile reads as a pattern or as quoting, which an attachment path must not hold.
/// 配置会解读为模式或引号的字符，附着路径中不得含有。
const PATTERN_CHARACTERS: [char; 10] = ['"', '\\', '*', '?', '[', ']', '{', '}', '^', '\n'];

/// Whether AppArmor restricts unprivileged user namespaces on this system (Ubuntu 23.10 and later).
/// 本系统的 AppArmor 是否限制非特权用户命名空间（Ubuntu 23.10 及以后）。
pub fn userns_restricted() -> bool {
    std::fs::read_to_string(USERNS_RESTRICTION).is_ok_and(|value| value.trim() == "1")
        && Path::new(PROFILE_PATH).parent().is_some_and(Path::is_dir)
}

/// The profile that attaches to the desktop program `desktop` (relative to a version directory) of
/// every version below `root` and allows it user namespaces; `None` when `root` or `desktop` holds
/// characters a profile reads as a pattern or quoting. `root` must be resolved, since AppArmor
/// matches resolved paths.
/// 附着到 `root` 下每个版本的桌面程序 `desktop`（相对版本目录）并允许其使用用户命名空间的配置；`root` 或
/// `desktop` 含有配置会解读为模式或引号的字符时为 `None`。`root` 必须已解析，因为 AppArmor 匹配解析后的路径。
pub fn profile(root: &Path, desktop: &str) -> Option<String> {
    let root = root.to_str()?;
    if root.contains(PATTERN_CHARACTERS) || desktop.contains(PATTERN_CHARACTERS) {
        return None;
    }
    Some(format!(
        "{GENERATED_MARKER}\n# Lets the Vulcan Code desktop app create the user namespaces its sandbox needs.\n\
         abi <abi/4.0>,\ninclude <tunables/global>\n\n\
         profile vulcan-code \"{}\" flags=(unconfined) {{\n  userns,\n\n  \
         include if exists <local/vulcan-code>\n}}\n",
        attachment(root, desktop)
    ))
}

/// The attachment path of the profile: the desktop program of every version below `root`.
/// 配置的附着路径：`root` 下每个版本的桌面程序。
fn attachment(root: &str, desktop: &str) -> String {
    format!("{root}/{VERSIONS_DIR}/*/{desktop}")
}

/// Whether the profile text `content` was written by Vulcan Code for the install root `root`
/// (resolved), whatever desktop program it names.
/// 配置文本 `content` 是否由 Vulcan Code 为安装根目录 `root`（已解析）写出，不论其写明哪个桌面程序。
pub fn names_root(content: &str, root: &Path) -> bool {
    root.to_str().is_some_and(|root| {
        content.contains(GENERATED_MARKER)
            && content.contains(&format!("profile vulcan-code \"{}", attachment(root, "")))
    })
}

/// What is installed at [`PROFILE_PATH`] compared with the profile this installation needs.
/// [`PROFILE_PATH`] 处已安装的内容与本安装所需配置的比较结果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Installed {
    /// Exactly the needed profile.
    /// 恰为所需配置。
    Current,
    /// Nothing.
    /// 什么都没有。
    Missing,
    /// A profile Vulcan Code wrote with other content, for example for another install root.
    /// Vulcan Code 写出的其他内容的配置，例如为另一个安装根目录写出。
    Outdated,
    /// A file Vulcan Code did not write, which belongs to whoever put it there.
    /// 不是 Vulcan Code 写出的文件，属于放置它的人。
    Foreign,
}

/// Compares the file at [`PROFILE_PATH`] with `needed`.
/// 将 [`PROFILE_PATH`] 处的文件与 `needed` 比较。
pub fn installed(needed: &str) -> Installed {
    compare(Path::new(PROFILE_PATH), needed)
}

/// Compares the file at `path` with `needed`.
/// 将 `path` 处的文件与 `needed` 比较。
fn compare(path: &Path, needed: &str) -> Installed {
    match std::fs::read_to_string(path) {
        Ok(existing) if existing == needed => Installed::Current,
        Ok(existing) if existing.contains(GENERATED_MARKER) => Installed::Outdated,
        Ok(_) => Installed::Foreign,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Installed::Missing,
        // Unreadable content cannot be told apart from someone else's file.
        // 无法读取的内容无法与他人的文件区分。
        Err(_) => Installed::Foreign,
    }
}

/// The commands that copy the profile prepared in `staged` into place and load it, to run with
/// administrator rights.
/// 把在 `staged` 中准备好的配置复制到位并加载的命令，需以管理员权限运行。
pub fn install_commands(staged: &Path) -> Vec<Vec<String>> {
    vec![
        vec![
            String::from("install"),
            String::from("-m"),
            String::from("0644"),
            staged.display().to_string(),
            String::from(PROFILE_PATH),
        ],
        vec![
            String::from("apparmor_parser"),
            String::from("-r"),
            String::from(PROFILE_PATH),
        ],
    ]
}

/// The commands that unload and delete the profile, to run with administrator rights.
/// 卸载并删除配置的命令，需以管理员权限运行。
pub fn remove_commands() -> Vec<Vec<String>> {
    vec![
        vec![
            String::from("apparmor_parser"),
            String::from("-R"),
            String::from(PROFILE_PATH),
        ],
        vec![
            String::from("rm"),
            String::from("-f"),
            String::from(PROFILE_PATH),
        ],
    ]
}

/// `commands` as one POSIX shell line that runs them in order, stopping at the first failure; each
/// command is prefixed with `prefix` (such as `sudo`) when one is given.
/// 将 `commands` 写成一行按顺序运行、遇到首个失败即停止的 POSIX Shell 命令；给出 `prefix`（如 `sudo`）时
/// 加在每条命令之前。
pub fn shell_line(commands: &[Vec<String>], prefix: Option<&str>) -> String {
    commands
        .iter()
        .map(|command| {
            prefix
                .into_iter()
                .map(str::to_owned)
                .chain(command.iter().map(|word| shell_word(word)))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join(" && ")
}

/// `word` quoted for a POSIX shell when it holds anything beyond plain path characters.
/// 含有普通路径字符以外的内容时，为 POSIX Shell 加上引号的 `word`。
fn shell_word(word: &str) -> String {
    if !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./=:@%+,".contains(c))
    {
        word.to_owned()
    } else {
        format!("'{}'", word.replace('\'', r"'\''"))
    }
}

/// The desktop program a version declares in its runtime layout, relative to `version_dir`;
/// `None` when the layout names none or cannot be read.
/// 某版本在其运行布局中声明的桌面程序（相对 `version_dir`）；布局未写明或无法读取时为 `None`。
pub fn desktop_executable(version_dir: &Path) -> Option<String> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Layout {
        desktop_executable: Option<String>,
    }
    let bytes = std::fs::read(version_dir.join(RUNTIME_LAYOUT_FILE)).ok()?;
    serde_json::from_slice::<Layout>(&bytes)
        .ok()?
        .desktop_executable
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_profile_attaches_to_every_version_and_refuses_pattern_characters() {
        let root = Path::new("/home/a b/.local/share/vulcan-code");
        let profile = profile(root, "desktop/vulcan-code-desktop").expect("profile");
        assert!(profile.starts_with(GENERATED_MARKER));
        assert!(profile.contains(
            "profile vulcan-code \"/home/a b/.local/share/vulcan-code/versions/*/desktop/vulcan-code-desktop\""
        ));
        assert!(profile.contains("  userns,\n"));
        assert!(names_root(&profile, root));
        assert!(!names_root(&profile, Path::new("/home/a b/.local/share")));
        for bad in ["/home/x*y", "/home/x\"y", "/home/x[y]", "/home/x\\y"] {
            assert_eq!(super::profile(Path::new(bad), "desktop/app"), None, "{bad}");
        }
        assert_eq!(super::profile(root, "desktop/a?p"), None);
    }

    #[test]
    fn installed_profiles_are_told_apart_by_content_and_marker() {
        let temp = tempfile::tempdir().expect("temp dir");
        let path = temp.path().join("profile");
        let needed = profile(Path::new("/opt/vc"), "desktop/app").expect("profile");
        assert_eq!(compare(&path, &needed), Installed::Missing);
        std::fs::write(&path, &needed).expect("write");
        assert_eq!(compare(&path, &needed), Installed::Current);
        let other = profile(Path::new("/opt/other"), "desktop/app").expect("profile");
        std::fs::write(&path, other).expect("write");
        assert_eq!(compare(&path, &needed), Installed::Outdated);
        std::fs::write(&path, "profile mine /opt/vc/** { userns, }\n").expect("write");
        assert_eq!(compare(&path, &needed), Installed::Foreign);
    }

    #[test]
    fn shell_lines_quote_words_the_shell_would_split() {
        let commands = install_commands(Path::new("/home/a b/it's"));
        assert_eq!(
            shell_line(&commands, Some("sudo")),
            format!(
                "sudo install -m 0644 '/home/a b/it'\\''s' {PROFILE_PATH} && sudo \
                 apparmor_parser -r {PROFILE_PATH}"
            )
        );
        assert_eq!(
            shell_line(&remove_commands(), None),
            format!("apparmor_parser -R {PROFILE_PATH} && rm -f {PROFILE_PATH}")
        );
    }

    #[test]
    fn the_desktop_program_comes_from_the_runtime_layout() {
        let temp = tempfile::tempdir().expect("temp dir");
        assert_eq!(desktop_executable(temp.path()), None);
        std::fs::write(
            temp.path().join(RUNTIME_LAYOUT_FILE),
            r#"{"serviceExecutable":"bin/s","desktopExecutable":"desktop/app"}"#,
        )
        .expect("layout");
        assert_eq!(
            desktop_executable(temp.path()).as_deref(),
            Some("desktop/app")
        );
    }
}
