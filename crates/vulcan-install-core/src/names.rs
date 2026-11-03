//! Product, platform, channel, component and version names used by every release document.
//! 每个发布文档都使用的产品、平台、通道、组件与版本名。

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::{FormatError, FormatResult};
use crate::limits::MAX_COMPONENT_NAME_BYTES;

/// The only product a pointer or manifest may describe.
/// 指针或版本清单唯一可描述的产品。
pub const PRODUCT: &str = "vulcan-code";

/// The format version of every release document this crate reads and writes.
/// 本 crate 读写的每个发布文档的格式版本。
pub const SCHEMA_VERSION: u32 = 1;

/// A release platform (plan section 18.4).
/// 发布平台（方案第 18.4 节）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Platform {
    /// Windows on x86-64.
    /// x86-64 上的 Windows。
    #[serde(rename = "windows-x64")]
    WindowsX64,
    /// Linux on x86-64.
    /// x86-64 上的 Linux。
    #[serde(rename = "linux-x64")]
    LinuxX64,
    /// Linux on ARM64.
    /// ARM64 上的 Linux。
    #[serde(rename = "linux-arm64")]
    LinuxArm64,
    /// macOS on Intel.
    /// Intel 上的 macOS。
    #[serde(rename = "macos-intel")]
    MacosIntel,
    /// macOS on Apple silicon.
    /// Apple 芯片上的 macOS。
    #[serde(rename = "macos-arm64")]
    MacosArm64,
}

impl Platform {
    /// Every release platform.
    /// 全部发布平台。
    pub const ALL: [Self; 5] = [
        Self::WindowsX64,
        Self::LinuxX64,
        Self::LinuxArm64,
        Self::MacosIntel,
        Self::MacosArm64,
    ];

    /// The name used in object keys and documents.
    /// 对象键与文档中使用的名称。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WindowsX64 => "windows-x64",
            Self::LinuxX64 => "linux-x64",
            Self::LinuxArm64 => "linux-arm64",
            Self::MacosIntel => "macos-intel",
            Self::MacosArm64 => "macos-arm64",
        }
    }

    /// The platform this program was built for, if it is a release platform.
    /// 本程序构建所针对的平台（若为发布平台）。
    pub const fn current() -> Option<Self> {
        if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
            Some(Self::WindowsX64)
        } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
            Some(Self::LinuxX64)
        } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
            Some(Self::LinuxArm64)
        } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
            Some(Self::MacosIntel)
        } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            Some(Self::MacosArm64)
        } else {
            None
        }
    }

    /// Whether this is a Windows platform.
    /// 是否为 Windows 平台。
    pub const fn is_windows(self) -> bool {
        matches!(self, Self::WindowsX64)
    }

    /// Whether this is a macOS platform.
    /// 是否为 macOS 平台。
    pub const fn is_macos(self) -> bool {
        matches!(self, Self::MacosIntel | Self::MacosArm64)
    }
}

impl fmt::Display for Platform {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A release channel (plan section 18.2).
/// 发布通道（方案第 18.2 节）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    /// Released versions.
    /// 正式版本。
    Stable,
    /// Pre-release versions such as `0.2.0-preview.1`.
    /// 预发布版本，例如 `0.2.0-preview.1`。
    Preview,
}

impl Channel {
    /// The name used in object keys and documents.
    /// 对象键与文档中使用的名称。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Preview => "preview",
        }
    }
}

impl fmt::Display for Channel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A component name: lowercase ASCII letters, digits and inner hyphens, starting with a letter.
/// 组件名：小写 ASCII 字母、数字与中间的连字符，以字母开头。
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ComponentName(String);

impl ComponentName {
    /// Validates a component name.
    /// 校验组件名。
    pub fn new(name: impl Into<String>) -> FormatResult<Self> {
        let name = name.into();
        let bytes = name.as_bytes();
        let well_formed = !bytes.is_empty()
            && bytes.len() <= MAX_COMPONENT_NAME_BYTES
            && bytes[0].is_ascii_lowercase()
            && bytes[bytes.len() - 1] != b'-'
            && !name.contains("--")
            && bytes
                .iter()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-');
        if well_formed {
            Ok(Self(name))
        } else {
            Err(FormatError::invalid(
                "component name",
                format!("`{name}` must be lowercase letters, digits and single inner hyphens"),
            ))
        }
    }

    /// The name text.
    /// 名称文本。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ComponentName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Serialize for ComponentName {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ComponentName {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// A product version: SemVer with an optional pre-release and no build metadata.
/// 产品版本：可带预发布段、不带构建元数据的语义化版本。
///
/// Ordering is SemVer precedence, so `0.2.0-preview.1` sorts before `0.2.0`.
/// 排序遵循语义化版本优先级，因此 `0.2.0-preview.1` 排在 `0.2.0` 之前。
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ReleaseVersion(semver::Version);

impl ReleaseVersion {
    /// The parsed version.
    /// 解析后的版本。
    pub fn semver(&self) -> &semver::Version {
        &self.0
    }

    /// Whether this is a pre-release version.
    /// 是否为预发布版本。
    pub fn is_prerelease(&self) -> bool {
        !self.0.pre.is_empty()
    }
}

impl FromStr for ReleaseVersion {
    type Err = FormatError;

    fn from_str(text: &str) -> FormatResult<Self> {
        let version = semver::Version::parse(text)
            .map_err(|error| FormatError::invalid("version", format!("`{text}`: {error}")))?;
        if !version.build.is_empty() {
            return Err(FormatError::invalid(
                "version",
                format!("`{text}` must not carry build metadata"),
            ));
        }
        Ok(Self(version))
    }
}

impl fmt::Display for ReleaseVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, formatter)
    }
}

impl Serialize for ReleaseVersion {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ReleaseVersion {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_names_follow_the_naming_rule() {
        for accepted in ["core", "tui", "desktop-runtime", "lua-runtime2"] {
            assert!(ComponentName::new(accepted).is_ok(), "{accepted}");
        }
        for rejected in ["", "Core", "2core", "core-", "a--b", "core_x", "a/b"] {
            assert!(ComponentName::new(rejected).is_err(), "{rejected}");
        }
    }

    #[test]
    fn versions_order_by_semver_and_refuse_build_metadata() {
        let preview: ReleaseVersion = "0.2.0-preview.1".parse().expect("preview");
        let release: ReleaseVersion = "0.2.0".parse().expect("release");
        assert!(preview < release);
        assert!(preview.is_prerelease());
        assert!("0.2.0+abc".parse::<ReleaseVersion>().is_err());
        assert!("v0.2.0".parse::<ReleaseVersion>().is_err());
    }

    #[test]
    fn platform_names_match_their_serialized_form() {
        for platform in Platform::ALL {
            let json = serde_json::to_string(&platform).expect("serialize");
            assert_eq!(json, format!("\"{}\"", platform.as_str()));
        }
    }
}
