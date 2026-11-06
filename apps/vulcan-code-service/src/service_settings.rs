//! `--set-port-range`: the one-shot command that records the port range the service tries.
//! `--set-port-range`：记录服务尝试的端口范围的一次性命令。
//!
//! The setting takes effect at the next start of the service; the format and its checks belong to
//! `vulcan_service_api::WebuiServiceSettings`, so the setup program and every other caller go
//! through this command instead of writing the file.
//! 设置在服务下次启动时生效；格式及其校验属于 `vulcan_service_api::WebuiServiceSettings`，因此安装器
//! 及其他调用方都经由本命令修改，而不是直接写入文件。

use vulcan_service_api::{
    WEBUI_SERVICE_SETTINGS_FILE, WebuiServicePortRange, WebuiServiceSettings,
};

/// Records the port range given as its value: `first-last`, one port, or `default`.
/// 记录作为其值给出的端口范围：`first-last`、单个端口或 `default`。
pub const ARG_SET_PORT_RANGE: &str = "--set-port-range";

/// The value of [`ARG_SET_PORT_RANGE`] that returns to the built-in range.
/// [`ARG_SET_PORT_RANGE`] 恢复内置范围的取值。
const DEFAULT_VALUE: &str = "default";

/// Returns the requested range text when `--set-port-range` is given, alone with its value.
/// 给出 `--set-port-range` 时返回请求的范围文本；它只能与其取值单独出现。
pub fn requested(args: &[String]) -> Result<Option<String>, String> {
    let tail = args.get(1..).unwrap_or_default();
    let inline = |arg: &String| {
        arg.strip_prefix(ARG_SET_PORT_RANGE)
            .and_then(|rest| rest.strip_prefix('='))
            .map(str::to_owned)
    };
    if !tail
        .iter()
        .any(|arg| arg == ARG_SET_PORT_RANGE || inline(arg).is_some())
    {
        return Ok(None);
    }
    match tail {
        [flag, value] if flag == ARG_SET_PORT_RANGE => Ok(Some(value.clone())),
        [only] => inline(only)
            .map(Some)
            .ok_or_else(|| format!("{ARG_SET_PORT_RANGE} needs a value")),
        _ => Err(format!(
            "{ARG_SET_PORT_RANGE} and its value must be the only arguments"
        )),
    }
}

/// Records `value` in the service settings of this build's data root and returns the summary.
/// 将 `value` 记录到本构建数据根的服务设置中，并返回摘要。
pub fn set_port_range(value: &str) -> Result<String, String> {
    let range = if value.trim() == DEFAULT_VALUE {
        WebuiServicePortRange::DEFAULT
    } else {
        WebuiServicePortRange::parse(value)?
    };
    let path = crate::service_runtime::authoritative_data_root()?.join(WEBUI_SERVICE_SETTINGS_FILE);
    let mut settings = WebuiServiceSettings::read(&path)?;
    settings.set_port_range(range);
    settings.write(&path)?;
    Ok(format!(
        "The service will use ports {range} from its next start."
    ))
}

/// Reads the client a tray double click opens from this build's service settings.
/// 从本构建的服务设置读取托盘双击打开的客户端。
#[cfg(all(feature = "desktop-tray", windows))]
pub fn tray_open_target() -> Result<vulcan_service_api::TrayOpenTarget, String> {
    let path = crate::service_runtime::authoritative_data_root()?.join(WEBUI_SERVICE_SETTINGS_FILE);
    Ok(WebuiServiceSettings::read(&path)?.effective_tray_open_target())
}

/// Records the client a tray double click opens, keeping every other setting.
/// 记录托盘双击打开的客户端，保留其他所有设置。
#[cfg(all(feature = "desktop-tray", windows))]
pub fn set_tray_open_target(target: vulcan_service_api::TrayOpenTarget) -> Result<(), String> {
    let path = crate::service_runtime::authoritative_data_root()?.join(WEBUI_SERVICE_SETTINGS_FILE);
    let mut settings = WebuiServiceSettings::read(&path)?;
    settings.set_tray_open_target(target);
    settings.write(&path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(words: &[&str]) -> Vec<String> {
        std::iter::once("vulcan-code-service")
            .chain(words.iter().copied())
            .map(String::from)
            .collect()
    }

    #[test]
    fn the_range_is_taken_only_as_the_sole_command() {
        assert_eq!(requested(&args(&["--service"])), Ok(None));
        assert_eq!(
            requested(&args(&["--set-port-range", "20000-20009"])),
            Ok(Some(String::from("20000-20009")))
        );
        assert_eq!(
            requested(&args(&["--set-port-range=default"])),
            Ok(Some(String::from("default")))
        );
        assert!(requested(&args(&["--set-port-range"])).is_err());
        assert!(requested(&args(&["--set-port-range", "20000", "--service"])).is_err());
        assert!(requested(&args(&["--service", "--set-port-range=20000"])).is_err());
    }
}
