//! Command line of the setup program (plan section 18.6).
//! 安装器的命令行（方案 18.6 节）。

use std::path::PathBuf;

use vulcan_install_core::fetch::Route;
use vulcan_install_core::names::{Channel, ComponentName};

use crate::state::Mode;

/// Usage text printed by `--help` and after an argument error.
/// `--help` 及参数错误后打印的用法说明。
pub const USAGE: &str = "\
usage: vulcan-code-setup [install] [--yes] [--mode desktop|terminal|service] [--with browser]
                         [--channel stable|preview] [--route auto|international|china]
                         [--prefix <dir>] [--no-shortcut] [--no-path]
       vulcan-code-setup update [--yes] [--prefix <dir>]
       vulcan-code-setup uninstall [--yes] [--purge] [--prefix <dir>]
       vulcan-code-setup status [--prefix <dir>]

install    installs Vulcan Code, or refreshes and updates an existing installation
update     installs the newest version with the choices recorded at installation
uninstall  removes everything the setup program created; --purge also removes ~/.vulcan
status     shows what is installed

--yes      runs without asking; required when there is no terminal
--mode     desktop (desktop app and terminal client), terminal (terminal client only), or
           service (Linux only: the WebUI service as a systemd user unit, plus the terminal client)
--with     adds an optional component; `browser` provides the built-in browser
--channel  stable (default) or preview
--route    download origins: auto (default), international, or china
--prefix   installation directory; the platform default is used when omitted";

/// What the setup program was asked to do.
/// 安装器被要求执行的操作。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// Install, or refresh and update an existing installation.
    /// 安装，或刷新并更新已有安装。
    Install(InstallOptions),
    /// Install the newest version with the recorded choices.
    /// 以记录的选择安装最新版本。
    Update(CommonOptions),
    /// Remove the installation.
    /// 移除安装。
    Uninstall(UninstallOptions),
    /// Show the installation.
    /// 显示安装。
    Status(CommonOptions),
    /// Print the usage.
    /// 打印用法。
    Help,
}

/// Options every command accepts.
/// 每个命令都接受的选项。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CommonOptions {
    /// Run without asking.
    /// 不询问直接执行。
    pub yes: bool,
    /// Installation directory, when not the platform default.
    /// 安装目录（不使用平台默认值时）。
    pub prefix: Option<PathBuf>,
}

/// Options of `install`; `None` keeps the recorded choice or the default.
/// `install` 的选项；`None` 表示沿用记录的选择或默认值。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InstallOptions {
    /// Options every command accepts.
    /// 每个命令都接受的选项。
    pub common: CommonOptions,
    /// Installation mode.
    /// 安装模式。
    pub mode: Option<Mode>,
    /// Optional components to add.
    /// 要添加的可选组件。
    pub with: Vec<ComponentName>,
    /// Release channel.
    /// 发布通道。
    pub channel: Option<Channel>,
    /// Download origins.
    /// 下载源。
    pub route: Option<Route>,
    /// Skip the desktop shortcut.
    /// 不创建桌面快捷方式。
    pub no_shortcut: bool,
    /// Skip putting `vulcan-tui` on PATH.
    /// 不把 `vulcan-tui` 加入 PATH。
    pub no_path: bool,
}

/// Options of `uninstall`.
/// `uninstall` 的选项。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UninstallOptions {
    /// Options every command accepts.
    /// 每个命令都接受的选项。
    pub common: CommonOptions,
    /// Also remove the user data in `~/.vulcan`.
    /// 同时删除 `~/.vulcan` 中的用户数据。
    pub purge: bool,
}

/// Components that `--with` may add; the others follow from the mode.
/// `--with` 可以添加的组件；其余组件由模式决定。
pub const OPTIONAL_COMPONENTS: [&str; 1] = ["browser"];

/// Parses the arguments after the program name.
/// 解析程序名之后的参数。
pub fn parse(arguments: &[String]) -> Result<Command, String> {
    let mut rest = arguments.iter().map(String::as_str).peekable();
    let verb = match rest.peek() {
        Some(word) if !word.starts_with('-') => {
            let verb = *word;
            rest.next();
            verb
        }
        _ => "install",
    };
    let mut install = InstallOptions::default();
    let mut purge = false;
    while let Some(argument) = rest.next() {
        let (flag, inline) = match argument.split_once('=') {
            Some((flag, value)) if flag.starts_with("--") => (flag, Some(value)),
            _ => (argument, None),
        };
        let mut value = |name: &str| -> Result<String, String> {
            match inline {
                Some(value) => Ok(value.to_owned()),
                None => rest
                    .next()
                    .map(str::to_owned)
                    .ok_or_else(|| format!("{name} needs a value")),
            }
        };
        let switch_only = |flag: &str| match inline {
            Some(_) => Err(format!("{flag} takes no value")),
            None => Ok(()),
        };
        match flag {
            "-h" | "--help" => return Ok(Command::Help),
            "-y" | "--yes" => {
                switch_only(flag)?;
                install.common.yes = true;
            }
            "--prefix" => install.common.prefix = Some(PathBuf::from(value(flag)?)),
            "--mode" => install.mode = Some(value(flag)?.parse()?),
            "--with" => {
                let name = value(flag)?;
                if !OPTIONAL_COMPONENTS.contains(&name.as_str()) {
                    return Err(format!(
                        "--with accepts {}, not {name}",
                        OPTIONAL_COMPONENTS.join(", ")
                    ));
                }
                let component = ComponentName::new(name).map_err(|error| error.to_string())?;
                if !install.with.contains(&component) {
                    install.with.push(component);
                }
            }
            "--channel" => {
                install.channel = Some(
                    value(flag)?
                        .parse()
                        .map_err(|_| String::from("--channel accepts stable or preview"))?,
                );
            }
            "--route" => install.route = Some(parse_route(&value(flag)?)?),
            "--no-shortcut" => {
                switch_only(flag)?;
                install.no_shortcut = true;
            }
            "--no-path" => {
                switch_only(flag)?;
                install.no_path = true;
            }
            "--purge" => {
                switch_only(flag)?;
                purge = true;
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    let install_only = install.mode.is_some()
        || !install.with.is_empty()
        || install.channel.is_some()
        || install.route.is_some()
        || install.no_shortcut
        || install.no_path;
    match verb {
        "install" if !purge => Ok(Command::Install(install)),
        "update" | "status" if !install_only && !purge => Ok(if verb == "update" {
            Command::Update(install.common)
        } else {
            Command::Status(install.common)
        }),
        "uninstall" if !install_only => Ok(Command::Uninstall(UninstallOptions {
            common: install.common,
            purge,
        })),
        "install" | "update" | "status" | "uninstall" => {
            Err(format!("an option given is not accepted by {verb}"))
        }
        other => Err(format!("unknown command {other}")),
    }
}

/// Parses a `--route` value.
/// 解析 `--route` 的值。
fn parse_route(value: &str) -> Result<Route, String> {
    match value {
        "auto" => Ok(Route::Auto),
        "international" => Ok(Route::International),
        "china" => Ok(Route::China),
        _ => Err(String::from("--route accepts auto, international or china")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses words separated by spaces.
    /// 解析以空格分隔的单词。
    fn parse_words(line: &str) -> Result<Command, String> {
        parse(
            &line
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn install_is_the_default_command_and_values_may_be_inline() {
        let Ok(Command::Install(options)) =
            parse_words("--yes --mode=terminal --with browser --channel preview --prefix /opt/v")
        else {
            panic!("install expected");
        };
        assert!(options.common.yes);
        assert_eq!(options.mode, Some(Mode::Terminal));
        assert_eq!(options.with.len(), 1);
        assert_eq!(options.channel, Some(Channel::Preview));
        assert_eq!(options.common.prefix, Some(PathBuf::from("/opt/v")));
    }

    #[test]
    fn options_of_another_command_are_refused() {
        assert!(parse_words("uninstall --mode desktop").is_err());
        assert!(parse_words("status --purge").is_err());
        assert!(parse_words("install --purge").is_err());
        assert!(parse_words("--with core").is_err());
        assert!(parse_words("--yes=1").is_err());
        assert!(parse_words("--mode").is_err());
        assert!(parse_words("repair").is_err());
        assert_eq!(
            parse_words("uninstall --purge --yes"),
            Ok(Command::Uninstall(UninstallOptions {
                common: CommonOptions {
                    yes: true,
                    prefix: None
                },
                purge: true
            }))
        );
    }
}
