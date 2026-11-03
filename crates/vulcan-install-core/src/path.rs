//! Paths inside a version root, valid on every release platform.
//! 版本根内的路径，在所有发布平台上都有效。
//!
//! An index is produced once and installed on Windows, Linux and macOS, so a path is accepted only
//! if it means the same location on all of them: forward slashes, no `.` or `..`, no absolute or
//! drive-qualified form, no Windows reserved device name, no character Windows refuses, and no
//! trailing dot or space (Windows silently strips those).
//!
//! 索引只生成一次，却要安装到 Windows、Linux 与 macOS，因此只接受在所有平台上指向同一位置的路径：使用
//! 正斜杠，不含 `.` 或 `..`，不是绝对路径或带盘符形式，不是 Windows 保留设备名，不含 Windows 拒绝的字符，
//! 也不以点或空格结尾（Windows 会静默去掉它们）。

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::{FormatError, FormatResult};
use crate::limits::MAX_PATH_BYTES;

/// Device names Windows reserves in every directory, with or without an extension.
/// Windows 在每个目录中保留的设备名，无论是否带扩展名。
const RESERVED_DEVICE_NAMES: [&str; 4] = ["con", "prn", "aux", "nul"];

/// Numbered device prefixes Windows reserves when followed by one digit or superscript digit.
/// 后接一位数字或上标数字时 Windows 保留的编号设备前缀。
const RESERVED_NUMBERED_DEVICES: [&str; 2] = ["com", "lpt"];

/// A validated relative path inside a version root, with forward slashes.
/// 经过校验、使用正斜杠的版本根内相对路径。
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RelativePath(String);

impl RelativePath {
    /// Validates a relative path.
    /// 校验相对路径。
    pub fn new(path: impl Into<String>) -> FormatResult<Self> {
        let path = path.into();
        if path.is_empty() || path.len() > MAX_PATH_BYTES {
            return Err(FormatError::invalid(
                "path",
                format!("`{path}` must be 1 to {MAX_PATH_BYTES} bytes"),
            ));
        }
        for segment in path.split('/') {
            check_segment(&path, segment, false)?;
        }
        Ok(Self(path))
    }

    /// The path text, with forward slashes.
    /// 路径文本，使用正斜杠。
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The path segments from the root.
    /// 从根开始的路径段。
    pub fn segments(&self) -> impl Iterator<Item = &str> {
        self.0.split('/')
    }

    /// The parent directories from the root, excluding the path itself.
    /// 从根开始的各级父目录，不含路径本身。
    pub fn ancestors(&self) -> impl Iterator<Item = &str> {
        self.0
            .match_indices('/')
            .map(|(position, _)| &self.0[..position])
    }

    /// A key that is equal for paths a case-insensitive file system treats as one file.
    /// 对大小写不敏感的文件系统视为同一文件的路径，此键相等。
    pub fn case_folded(&self) -> String {
        self.0.to_lowercase()
    }

    /// Checks that a symbolic link at this path with `target` stays inside the version root.
    /// 校验位于本路径、目标为 `target` 的符号链接不会越出版本根。
    pub fn check_symlink_target(&self, target: &str) -> FormatResult<()> {
        if target.is_empty() || target.len() > MAX_PATH_BYTES {
            return Err(FormatError::invalid(
                "symlink target",
                format!("`{target}` must be 1 to {MAX_PATH_BYTES} bytes"),
            ));
        }
        // The link resolves relative to its own directory.
        // 链接相对于其所在目录解析。
        let mut depth = self.segments().count() - 1;
        for segment in target.split('/') {
            check_segment(target, segment, true)?;
            if segment == ".." {
                depth = depth.checked_sub(1).ok_or_else(|| {
                    FormatError::invalid(
                        "symlink target",
                        format!("`{target}` of `{}` leaves the version root", self.0),
                    )
                })?;
            } else {
                depth += 1;
            }
        }
        Ok(())
    }
}

/// Checks one path segment; `allow_parent` admits `..` for symbolic link targets.
/// 校验一个路径段；`allow_parent` 为符号链接目标允许 `..`。
fn check_segment(path: &str, segment: &str, allow_parent: bool) -> FormatResult<()> {
    let reject = |rule: &str| FormatError::invalid("path", format!("`{path}`: {rule}"));
    if segment.is_empty() {
        return Err(reject("empty segment (leading, trailing or doubled slash)"));
    }
    if segment == "." {
        return Err(reject("`.` segment"));
    }
    if segment == ".." {
        return if allow_parent {
            Ok(())
        } else {
            Err(reject("`..` segment"))
        };
    }
    if segment.ends_with('.') || segment.ends_with(' ') {
        return Err(reject("segment ends with a dot or space"));
    }
    if let Some(character) = segment
        .chars()
        .find(|character| character.is_control() || "\\:<>\"|?*".contains(*character))
    {
        return Err(reject(&format!("character {character:?} is not portable")));
    }
    let stem = segment
        .split('.')
        .next()
        .unwrap_or(segment)
        .trim_end_matches(' ')
        .to_lowercase();
    let numbered_device = RESERVED_NUMBERED_DEVICES.iter().any(|prefix| {
        stem.strip_prefix(prefix).is_some_and(|suffix| {
            let mut characters = suffix.chars();
            matches!(
                (characters.next(), characters.next()),
                (Some('0'..='9' | '¹' | '²' | '³'), None)
            )
        })
    });
    if RESERVED_DEVICE_NAMES.contains(&stem.as_str()) || numbered_device {
        return Err(reject("Windows reserved device name"));
    }
    Ok(())
}

impl fmt::Display for RelativePath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl fmt::Debug for RelativePath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "RelativePath({:?})", self.0)
    }
}

impl Serialize for RelativePath {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for RelativePath {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_paths_are_accepted() {
        for accepted in [
            "bin/vulcan-code-service.exe",
            "webui/index.html",
            "a/.hidden",
            "console/readme",
            "com10.txt",
            "lpt/x",
            "名字/文件.txt",
        ] {
            assert!(RelativePath::new(accepted).is_ok(), "{accepted}");
        }
    }

    #[test]
    fn non_portable_paths_are_rejected() {
        for rejected in [
            "",
            "/etc/passwd",
            "a//b",
            "a/",
            "./a",
            "a/../b",
            "..",
            "C:/x",
            "a\\b",
            "a/b.",
            "a/b ",
            "CON",
            "aux.txt",
            "dir/Nul.tar.gz",
            "COM1",
            "lpt9.log",
            "com².x",
            "a?b",
            "a\u{0}b",
        ] {
            assert!(RelativePath::new(rejected).is_err(), "{rejected:?}");
        }
        assert!(RelativePath::new("a".repeat(MAX_PATH_BYTES + 1)).is_err());
    }

    #[test]
    fn symlink_targets_must_stay_inside_the_root() {
        let link = RelativePath::new("desktop/Frameworks/x").expect("link path");
        assert!(link.check_symlink_target("Versions/Current/x").is_ok());
        assert!(link.check_symlink_target("../../webui/index.html").is_ok());
        assert!(link.check_symlink_target("../../../outside").is_err());
        assert!(link.check_symlink_target("/usr/lib/x").is_err());
        assert!(link.check_symlink_target("./x").is_err());
        assert!(link.check_symlink_target("").is_err());
        let top = RelativePath::new("x").expect("top-level link");
        assert!(top.check_symlink_target("../x").is_err());
    }

    #[test]
    fn ancestors_list_parent_directories_from_the_root() {
        let path = RelativePath::new("a/b/c.txt").expect("path");
        assert_eq!(path.ancestors().collect::<Vec<_>>(), ["a", "a/b"]);
    }
}
