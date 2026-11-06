//! Service settings chosen by the operator, such as the port range read at start and the client a
//! tray double click opens.
//! 由操作者选择的服务设置，例如启动时读取的端口范围，以及托盘双击打开的客户端。
//!
//! The settings live beside the discovery document in `service-settings.json`; Debug and Release
//! own separate user roots and therefore separate settings. Only an explicit choice writes the
//! file, and a choice equal to the default removes its entry (AGENTS.md section one, rule 5). The
//! `VULCAN_WEBUI_SERVICE_PORT` override still wins over the configured range.
//! 设置位于发现文档旁的 `service-settings.json`；Debug 与 Release 拥有各自的用户根目录，因此设置也各自
//! 独立。只有显式选择才写入文件，等于默认值的选择会移除对应条目（AGENTS.md 第一节第 5 条）。
//! `VULCAN_WEBUI_SERVICE_PORT` 覆盖值仍优先于配置的范围。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{WEBUI_SERVICE_HTTP_PORT, WEBUI_SERVICE_HTTP_PORT_END};

/// File name of the service settings below the user Vulcan Code root.
/// 用户 Vulcan Code 根目录下服务设置的文件名。
pub const WEBUI_SERVICE_SETTINGS_FILE: &str = "service-settings.json";

/// Format version of [`WEBUI_SERVICE_SETTINGS_FILE`]; other versions are refused.
/// [`WEBUI_SERVICE_SETTINGS_FILE`] 的格式版本；其他版本一律拒绝。
pub const WEBUI_SERVICE_SETTINGS_SCHEMA: u32 = 1;

/// Lowest port a configured range may use; lower ports need privileges on Unix.
/// 配置范围可使用的最低端口；更低的端口在 Unix 上需要特权。
pub const WEBUI_SERVICE_PORT_RANGE_MIN: u16 = 1024;

/// Inclusive range of loopback ports the service tries in order.
/// 服务依次尝试的回环端口闭区间。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebuiServicePortRange {
    /// First port tried.
    /// 首先尝试的端口。
    pub first: u16,
    /// Last port tried.
    /// 最后尝试的端口。
    pub last: u16,
}

impl WebuiServicePortRange {
    /// The range used when none is configured.
    /// 未配置时使用的范围。
    pub const DEFAULT: Self = Self {
        first: WEBUI_SERVICE_HTTP_PORT,
        last: WEBUI_SERVICE_HTTP_PORT_END,
    };

    /// Checks a range: both ends at least [`WEBUI_SERVICE_PORT_RANGE_MIN`], first not after last.
    /// 检查范围：两端均不低于 [`WEBUI_SERVICE_PORT_RANGE_MIN`]，且起始不晚于结束。
    pub fn new(first: u16, last: u16) -> Result<Self, String> {
        if first < WEBUI_SERVICE_PORT_RANGE_MIN {
            return Err(format!(
                "the port range must start at {WEBUI_SERVICE_PORT_RANGE_MIN} or higher, not {first}"
            ));
        }
        if first > last {
            return Err(format!(
                "the port range starts at {first}, after its last port {last}"
            ));
        }
        Ok(Self { first, last })
    }

    /// Parses `first-last`, or one port for a range of one.
    /// 解析 `first-last`；只给一个端口时表示只含该端口的范围。
    pub fn parse(text: &str) -> Result<Self, String> {
        let port = |value: &str| {
            value
                .trim()
                .parse::<u16>()
                .map_err(|_| format!("`{text}` is not a port range such as {}", Self::DEFAULT))
        };
        match text.split_once('-') {
            Some((first, last)) => Self::new(port(first)?, port(last)?),
            None => {
                let only = port(text)?;
                Self::new(only, only)
            }
        }
    }

    /// Every port of the range in the order they are tried.
    /// 范围内的全部端口，按尝试顺序排列。
    pub fn ports(self) -> Vec<u16> {
        (self.first..=self.last).collect()
    }
}

impl std::fmt::Display for WebuiServicePortRange {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}-{}", self.first, self.last)
    }
}

/// Content of [`WEBUI_SERVICE_SETTINGS_FILE`]; an absent entry means the default.
/// [`WEBUI_SERVICE_SETTINGS_FILE`] 的内容；缺少的条目表示使用默认值。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebuiServiceSettings {
    /// Always [`WEBUI_SERVICE_SETTINGS_SCHEMA`].
    /// 恒为 [`WEBUI_SERVICE_SETTINGS_SCHEMA`]。
    pub schema: u32,
    /// The configured port range.
    /// 配置的端口范围。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port_range: Option<WebuiServicePortRange>,
    /// The client a double click on the tray icon opens.
    /// 双击托盘图标时打开的客户端。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tray_open_target: Option<TrayOpenTarget>,
}

/// A client the tray can open; the desktop client is the default.
/// 托盘可打开的客户端；默认是桌面客户端。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrayOpenTarget {
    /// The desktop client (GUI).
    /// 桌面客户端（GUI）。
    #[default]
    Desktop,
    /// The WebUI in the system browser.
    /// 系统浏览器中的 WebUI。
    Webui,
    /// The terminal client (TUI).
    /// 终端客户端（TUI）。
    Terminal,
}

impl Default for WebuiServiceSettings {
    fn default() -> Self {
        Self {
            schema: WEBUI_SERVICE_SETTINGS_SCHEMA,
            port_range: None,
            tray_open_target: None,
        }
    }
}

impl WebuiServiceSettings {
    /// The client a tray double click opens: the configured one, or the desktop client.
    /// 托盘双击打开的客户端：已配置的客户端，或桌面客户端。
    pub fn effective_tray_open_target(&self) -> TrayOpenTarget {
        self.tray_open_target.unwrap_or_default()
    }

    /// Records `target`; the default target is recorded as no entry.
    /// 记录 `target`；默认客户端记录为无条目。
    pub fn set_tray_open_target(&mut self, target: TrayOpenTarget) {
        self.tray_open_target = (target != TrayOpenTarget::default()).then_some(target);
    }

    /// The port range in effect: the configured one, or [`WebuiServicePortRange::DEFAULT`].
    /// 生效的端口范围：已配置的范围，或 [`WebuiServicePortRange::DEFAULT`]。
    pub fn effective_port_range(&self) -> WebuiServicePortRange {
        self.port_range.unwrap_or(WebuiServicePortRange::DEFAULT)
    }

    /// Records `range`; the default range is recorded as no entry.
    /// 记录 `range`；默认范围记录为无条目。
    pub fn set_port_range(&mut self, range: WebuiServicePortRange) {
        self.port_range = (range != WebuiServicePortRange::DEFAULT).then_some(range);
    }

    /// Reads the settings at `path`; a missing file means every default.
    /// 读取 `path` 处的设置；文件不存在表示全部使用默认值。
    pub fn read(path: &Path) -> Result<Self, String> {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => return Err(format!("cannot read {}: {error}", path.display())),
        };
        let settings: Self = serde_json::from_slice(&bytes)
            .map_err(|error| format!("{} is not valid: {error}", path.display()))?;
        if settings.schema != WEBUI_SERVICE_SETTINGS_SCHEMA {
            return Err(format!(
                "{} has format {}, which this service does not know",
                path.display(),
                settings.schema
            ));
        }
        if let Some(range) = settings.port_range {
            WebuiServicePortRange::new(range.first, range.last)
                .map_err(|error| format!("{}: {error}", path.display()))?;
        }
        Ok(settings)
    }

    /// Replaces the settings at `path` atomically; settings with no entry remove the file.
    /// 原子替换 `path` 处的设置；没有任何条目时删除该文件。
    pub fn write(&self, path: &Path) -> Result<(), String> {
        let fail = |error: std::io::Error| format!("cannot write {}: {error}", path.display());
        if *self == Self::default() {
            return match std::fs::remove_file(path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(fail(error)),
            };
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(fail)?;
        }
        let mut bytes = serde_json::to_vec_pretty(self).map_err(|error| error.to_string())?;
        bytes.push(b'\n');
        let mut partial = path.as_os_str().to_owned();
        partial.push(".partial");
        std::fs::write(&partial, bytes).map_err(fail)?;
        std::fs::rename(&partial, path).map_err(fail)
    }
}

/// Resolves the service settings path beside the discovery document.
/// 解析位于发现文档旁的服务设置路径。
///
/// # Parameters
/// 参数
/// `home_dir` is the optional user home directory and `temp_dir` is the fallback temporary directory.
/// `home_dir` 是可选的用户主目录，`temp_dir` 是回退用的临时目录。
pub fn webui_service_settings_path(home_dir: Option<PathBuf>, temp_dir: PathBuf) -> PathBuf {
    let root = home_dir.unwrap_or(temp_dir);
    vulcan_runtime_assets::user_vulcan_code_root_for_home(&root).join(WEBUI_SERVICE_SETTINGS_FILE)
}

/// Resolves the ports to try: a valid `port_override` alone, otherwise the configured range.
/// 解析要尝试的端口：有效的 `port_override` 单独使用，否则使用配置的范围。
///
/// # Parameters
/// 参数
/// `port_override` is the raw `VULCAN_WEBUI_SERVICE_PORT` value; `settings` are the operator's
/// service settings.
/// `port_override` 是原始的 `VULCAN_WEBUI_SERVICE_PORT` 值；`settings` 是操作者的服务设置。
pub fn webui_service_http_bind_ports(
    port_override: Option<&str>,
    settings: &WebuiServiceSettings,
) -> Vec<u16> {
    if let Some(port) = port_override
        .map(str::trim)
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|port| *port > 0)
    {
        return vec![port];
    }
    settings.effective_port_range().ports()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_ranges_parse_and_refuse_privileged_or_reversed_ports() {
        assert_eq!(
            WebuiServicePortRange::parse(" 20000 - 20009 "),
            Ok(WebuiServicePortRange {
                first: 20000,
                last: 20009
            })
        );
        assert_eq!(
            WebuiServicePortRange::parse("20000").map(WebuiServicePortRange::ports),
            Ok(vec![20000])
        );
        for refused in ["80-90", "20009-20000", "a-b", "", "1-2-3", "70000"] {
            assert!(
                WebuiServicePortRange::parse(refused).is_err(),
                "accepted {refused}"
            );
        }
        let default = WebuiServicePortRange::DEFAULT.to_string();
        assert_eq!(
            WebuiServicePortRange::parse(&default),
            Ok(WebuiServicePortRange::DEFAULT)
        );
    }

    #[test]
    fn only_a_chosen_non_default_range_is_written() {
        let temp = tempfile::tempdir().expect("temp dir");
        let path = temp.path().join("code").join(WEBUI_SERVICE_SETTINGS_FILE);
        assert_eq!(
            WebuiServiceSettings::read(&path),
            Ok(WebuiServiceSettings::default())
        );
        let mut settings = WebuiServiceSettings::default();
        settings.set_port_range(WebuiServicePortRange::DEFAULT);
        settings.write(&path).expect("write default");
        assert!(!path.exists());
        let chosen = WebuiServicePortRange::new(20000, 20009).expect("range");
        settings.set_port_range(chosen);
        settings.write(&path).expect("write chosen");
        let read = WebuiServiceSettings::read(&path).expect("read");
        assert_eq!(read.effective_port_range(), chosen);
        assert_eq!(webui_service_http_bind_ports(None, &read), chosen.ports());
        assert_eq!(
            webui_service_http_bind_ports(Some(" 19000 "), &read),
            vec![19000]
        );
        assert_eq!(
            webui_service_http_bind_ports(Some("0"), &read),
            chosen.ports()
        );
        settings.set_port_range(WebuiServicePortRange::DEFAULT);
        settings.write(&path).expect("reset");
        assert!(!path.exists());
    }

    #[test]
    fn the_tray_target_keeps_the_port_range_and_writes_only_a_non_default_choice() {
        let temp = tempfile::tempdir().expect("temp dir");
        let path = temp.path().join(WEBUI_SERVICE_SETTINGS_FILE);
        let mut settings = WebuiServiceSettings::default();
        assert_eq!(
            settings.effective_tray_open_target(),
            TrayOpenTarget::Desktop
        );
        settings.set_tray_open_target(TrayOpenTarget::Desktop);
        settings.write(&path).expect("write default");
        assert!(!path.exists());
        let range = WebuiServicePortRange::new(20000, 20009).expect("range");
        settings.set_port_range(range);
        settings.set_tray_open_target(TrayOpenTarget::Terminal);
        settings.write(&path).expect("write chosen");
        let mut read = WebuiServiceSettings::read(&path).expect("read");
        assert_eq!(read.effective_tray_open_target(), TrayOpenTarget::Terminal);
        assert_eq!(read.effective_port_range(), range);
        read.set_port_range(WebuiServicePortRange::DEFAULT);
        read.set_tray_open_target(TrayOpenTarget::Desktop);
        read.write(&path).expect("reset");
        assert!(!path.exists());
    }

    #[test]
    fn unknown_formats_and_invalid_ranges_are_refused() {
        let temp = tempfile::tempdir().expect("temp dir");
        let path = temp.path().join(WEBUI_SERVICE_SETTINGS_FILE);
        for content in [
            r#"{"schema":2}"#,
            r#"{"schema":1,"portRange":{"first":80,"last":90}}"#,
            r#"{"schema":1,"portRange":{"first":20009,"last":20000}}"#,
            r#"{"schema":1,"other":true}"#,
            r#"{"schema":1,"trayOpenTarget":"editor"}"#,
        ] {
            std::fs::write(&path, content).expect("write");
            assert!(
                WebuiServiceSettings::read(&path).is_err(),
                "accepted {content}"
            );
        }
    }
}
