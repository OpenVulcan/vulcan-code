//! Manages the setup-owned PATH export in the user's shell startup files.
//! 管理用户 Shell 启动文件中由安装器写入的 PATH 导出。

use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use vulcan_install_core::digest::Sha256Digest;

/// Startup file to update and the syntax it accepts.
/// 要更新的启动文件及其接受的语法。
pub struct Profile {
    /// Path used by the shell.
    /// Shell 使用的路径。
    pub path: PathBuf,
    /// Whether the file uses fish syntax.
    /// 文件是否使用 fish 语法。
    fish: bool,
}

/// Resolves the login and interactive startup files of the user's shell.
/// 定位用户 Shell 的登录与交互启动文件。
pub fn profiles() -> Result<Vec<Profile>, String> {
    let home = crate::paths::home()?;
    let shell = std::env::var_os("SHELL");
    let shell = shell
        .as_deref()
        .and_then(|path| Path::new(path).file_name());
    Ok(match shell.and_then(|name| name.to_str()) {
        Some("bash") => bash_profiles(&home),
        Some("zsh") => {
            let config = std::env::var_os("ZDOTDIR").map_or(home, PathBuf::from);
            vec![
                Profile {
                    path: config.join(".zprofile"),
                    fish: false,
                },
                Profile {
                    path: config.join(".zshrc"),
                    fish: false,
                },
            ]
        }
        Some("fish") => {
            let config = std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .unwrap_or_else(|| home.join(".config"));
            vec![Profile {
                path: config.join("fish/conf.d/vulcan-code-path.fish"),
                fish: true,
            }]
        }
        _ => vec![Profile {
            path: home.join(".profile"),
            fish: false,
        }],
    })
}

/// Uses Bash's first existing login profile and its independent interactive startup file.
/// 使用 Bash 首个存在的登录配置及独立的交互启动文件。
fn bash_profiles(home: &Path) -> Vec<Profile> {
    let login = [".bash_profile", ".bash_login", ".profile"]
        .into_iter()
        .map(|name| home.join(name))
        .find(|path| path.is_file())
        .unwrap_or_else(|| home.join(".profile"));
    vec![
        Profile {
            path: login,
            fish: false,
        },
        Profile {
            path: home.join(".bashrc"),
            fish: false,
        },
    ]
}

/// Quotes a POSIX shell word so the startup file and displayed commands preserve it literally.
/// 引用 POSIX Shell 参数，使启动文件和展示的命令按原样使用它。
pub fn quote(word: &str) -> String {
    format!("'{}'", word.replace('\'', "'\\''"))
}

/// Resolves the file a profile points at so editing preserves dotfile symlinks.
/// 解析配置指向的文件，使编辑保留配置文件的符号链接。
pub fn destination(path: &Path) -> Result<PathBuf, String> {
    match path.canonicalize() {
        Ok(resolved) => Ok(resolved),
        Err(error) if error.kind() == ErrorKind::NotFound && !path.is_symlink() => {
            crate::paths::absolute(path)
        }
        Err(error) => Err(format!("cannot resolve {}: {error}", path.display())),
    }
}

/// Identifies this root's block so another prefix can keep its own PATH integration.
/// 标识此根目录的区块，使其他安装前缀可保留各自的 PATH 集成。
fn marker(root: &Path) -> String {
    let hash = Sha256Digest::of(root.as_os_str().as_encoded_bytes());
    format!("# Vulcan Code setup PATH {hash}")
}

/// Adds this installation's export to `path`, preserving the original bytes and permissions.
/// 把本安装的导出追加到 `path`，保留原始字节和文件权限。
pub fn install(profile: &Profile, path: &Path, root: &Path) -> Result<(), String> {
    let marker = marker(root);
    let mut bytes = read(path)?;
    if block(&bytes, &marker)?.is_some() {
        return Ok(());
    }
    // The owned leading newline lets uninstall restore a profile that lacked a final newline.
    // 区块拥有开头的换行，使卸载能恢复原本没有末尾换行的配置。
    let export = if profile.fish {
        "if not contains -- \"$HOME/.local/bin\" $PATH\n    set -gx PATH \"$HOME/.local/bin\" $PATH\nend"
    } else {
        "case \":${PATH-}:\" in\n    *\":$HOME/.local/bin:\"*) ;;\n    *) export PATH=\"$HOME/.local/bin${PATH:+:$PATH}\" ;;\nesac"
    };
    bytes.extend_from_slice(format!("\n{marker} begin\n{export}\n{marker} end\n").as_bytes());
    write(path, &bytes)
}

/// Removes only the block this installation owns, leaving edits around it intact.
/// 仅移除本安装拥有的区块，保留其周围的用户编辑。
pub fn remove(path: &Path, root: &Path) -> Result<(), String> {
    let mut bytes = read(path)?;
    if let Some(range) = block(&bytes, &marker(root))? {
        bytes.drain(range);
        write(path, &bytes)?;
    }
    Ok(())
}

/// Finds the entire marked range and reports a damaged marker before changing the profile.
/// 找出整个标记范围，标记损坏时在修改配置前报错。
fn block(bytes: &[u8], marker: &str) -> Result<Option<std::ops::Range<usize>>, String> {
    let begin = format!("\n{marker} begin\n");
    let end = format!("\n{marker} end\n");
    let find = |needle: &str| {
        bytes
            .windows(needle.len())
            .position(|part| part == needle.as_bytes())
    };
    match (find(&begin), find(&end)) {
        (None, None) => Ok(None),
        (Some(start), Some(stop)) if stop >= start + begin.len() => {
            Ok(Some(start..stop + end.len()))
        }
        _ => Err(format!("incomplete shell profile block: {marker}")),
    }
}

/// Reads a profile, treating an absent file as an empty one.
/// 读取配置，尚无该文件时视为空配置。
fn read(path: &Path) -> Result<Vec<u8>, String> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(bytes),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(format!("cannot read {}: {error}", path.display())),
    }
}

/// Atomically replaces a profile using a private, unique temporary file in the same directory.
/// 使用同目录中唯一的私有临时文件原子替换配置。
fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or("shell profile has no parent directory")?;
    let fail = |error: std::io::Error| format!("cannot write {}: {error}", path.display());
    std::fs::create_dir_all(parent).map_err(fail)?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(fail)?;
    if let Ok(metadata) = path.metadata() {
        file.as_file()
            .set_permissions(metadata.permissions())
            .map_err(fail)?;
    }
    file.write_all(bytes).map_err(fail)?;
    file.persist(path).map_err(|error| fail(error.error))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that a fresh Bash exports the path once and uninstall restores the original bytes.
    /// 验证新 Bash 只导出一次路径，卸载后恢复原始字节。
    #[test]
    fn bash_exports_path_and_keeps_the_original_profile() {
        let temp = tempfile::tempdir().expect("temporary home");
        let home = temp.path();
        let root = home.join("install");
        let profile = bash_profiles(home).pop().expect("Bash profile");
        let file = &profile.path;
        let original = b"# Existing profile with no final newline";
        std::fs::write(file, original).expect("original profile");
        install(&profile, file, &root).expect("install PATH");
        install(&profile, file, &root).expect("repeat install");
        let output = std::process::Command::new("bash")
            .args(["--noprofile", "--rcfile"])
            .arg(file)
            .args(["-ic", "sh -c 'printf %s \"$PATH\"'"])
            .env("HOME", home)
            .env("PATH", "/usr/bin:/bin")
            .output()
            .expect("start a new Bash");
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout).expect("PATH"),
            format!("{}/.local/bin:/usr/bin:/bin", home.display())
        );
        remove(file, &root).expect("uninstall PATH");
        assert_eq!(std::fs::read(file).expect("remaining profile"), original);
    }

    /// Keeps both the dotfile symlink and independent PATH exports for other prefixes.
    /// 保留配置文件的符号链接及其他前缀各自的 PATH 导出。
    #[test]
    fn removes_only_the_resolved_block_owned_by_this_prefix() {
        let temp = tempfile::tempdir().expect("temporary home");
        let home = temp.path();
        let file = home.join("dotfiles-bash-profile");
        let profile_path = home.join(".bash_profile");
        std::fs::write(&file, b"# original\n").expect("profile");
        std::os::unix::fs::symlink(&file, &profile_path).expect("symlink");
        let profiles = bash_profiles(home);
        let profile = &profiles[0];
        assert_eq!(profile.path, profile_path);
        let resolved = destination(&profile.path).expect("resolved profile");
        let a = home.join("install a");
        let b = home.join("install b");
        install(profile, &resolved, &a).expect("first PATH");
        install(profile, &resolved, &b).expect("second PATH");
        remove(&resolved, &a).expect("remove first PATH");
        assert!(profile_path.is_symlink());
        let remaining = read(&resolved).expect("remaining profile");
        assert!(
            block(&remaining, &marker(&a))
                .expect("first block")
                .is_none()
        );
        assert!(
            block(&remaining, &marker(&b))
                .expect("second block")
                .is_some()
        );
        remove(&resolved, &b).expect("remove second PATH");
        assert_eq!(read(&resolved).expect("original profile"), b"# original\n");
    }
}
