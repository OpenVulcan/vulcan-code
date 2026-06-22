//! Host shell detection and resolution helpers shared by settings surfaces and native command tools.
//! 原生命令工具与设置界面共享的宿主 shell 检测与解析辅助逻辑。

use std::env;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// One supported host shell family that may be exposed to AI tools and WebUI settings.
/// 一种可暴露给 AI 工具与WebUI设置的宿主 shell 族。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HostCommandShell {
    /// PowerShell 7+ (`pwsh`) when available on the host.
    /// 宿主存在时使用的 PowerShell 7+（`pwsh`）。
    Pwsh,
    /// Windows PowerShell 5.x (`powershell`) available on legacy Windows systems.
    /// 旧版 Windows 系统可用的 Windows PowerShell 5.x（`powershell`）。
    Powershell,
    /// Windows Command Prompt (`cmd`).
    /// Windows 命令提示符（`cmd`）。
    Cmd,
    /// GNU Bash shell.
    /// GNU Bash shell。
    Bash,
    /// Z shell.
    /// Z shell。
    Zsh,
    /// POSIX `sh` shell.
    /// POSIX `sh` shell。
    Sh,
}

impl HostCommandShell {
    /// Returns the stable persisted identifier used by tool arguments and configuration files.
    /// 返回供工具参数与配置文件复用的稳定持久化标识。
    pub const fn id(self) -> &'static str {
        match self {
            Self::Pwsh => "pwsh",
            Self::Powershell => "powershell",
            Self::Cmd => "cmd",
            Self::Bash => "bash",
            Self::Zsh => "zsh",
            Self::Sh => "sh",
        }
    }

    /// Returns the human-readable shell label rendered in settings summaries and diagnostics.
    /// 返回渲染在设置摘要与诊断中的人类可读 shell 标签。
    pub const fn display_label(self) -> &'static str {
        match self {
            Self::Pwsh => "PowerShell 7 (pwsh)",
            Self::Powershell => "Windows PowerShell",
            Self::Cmd => "Command Prompt (cmd)",
            Self::Bash => "Bash",
            Self::Zsh => "Zsh",
            Self::Sh => "POSIX sh",
        }
    }

    /// Returns the interactive newline sequence that should be used when one session asks the host to submit an input line automatically.
    /// 返回当某个会话请求宿主自动提交一行输入时应使用的交互换行序列。
    pub const fn interactive_newline(self) -> &'static str {
        match self {
            Self::Bash | Self::Zsh | Self::Sh => "\n",
            Self::Pwsh | Self::Powershell | Self::Cmd => {
                if cfg!(windows) {
                    "\r\n"
                } else {
                    "\n"
                }
            }
        }
    }

    /// Parses one persisted or tool-provided shell identifier into a supported shell family.
    /// 将一条持久化或工具传入的 shell 标识解析为受支持的 shell 族。
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "pwsh" => Some(Self::Pwsh),
            "powershell" => Some(Self::Powershell),
            "cmd" => Some(Self::Cmd),
            "bash" => Some(Self::Bash),
            "zsh" => Some(Self::Zsh),
            "sh" => Some(Self::Sh),
            _ => None,
        }
    }

    /// Builds the launcher argument vector required to run one raw command through the current shell family.
    /// 构建通过当前 shell 族运行一条原始命令所需的启动参数向量。
    pub fn build_launcher_args(self, command_text: &str) -> Vec<String> {
        match self {
            Self::Pwsh | Self::Powershell => vec![
                String::from("-NoProfile"),
                String::from("-Command"),
                command_text.to_string(),
            ],
            Self::Cmd => vec![String::from("/C"), command_text.to_string()],
            Self::Bash | Self::Zsh | Self::Sh => {
                vec![String::from("-lc"), command_text.to_string()]
            }
        }
    }

    /// Returns the candidate executable names searched on the host `PATH`.
    /// 返回在宿主 `PATH` 上搜索的候选可执行文件名。
    fn candidate_program_names(self) -> &'static [&'static str] {
        match self {
            Self::Pwsh => &["pwsh"],
            Self::Powershell => &["powershell"],
            Self::Cmd => &["cmd"],
            Self::Bash => &["bash"],
            Self::Zsh => &["zsh"],
            Self::Sh => &["sh"],
        }
    }

    /// Returns the candidate absolute executable paths checked when PATH lookup alone is not enough.
    /// 返回当仅依赖 PATH 查询不足时额外检查的候选绝对可执行路径。
    fn candidate_absolute_programs(self) -> Vec<PathBuf> {
        match self {
            Self::Pwsh => candidate_pwsh_paths(),
            Self::Powershell => candidate_windows_powershell_paths(),
            Self::Cmd => candidate_cmd_paths(),
            Self::Bash => candidate_bash_paths(),
            Self::Zsh => candidate_zsh_paths(),
            Self::Sh => candidate_sh_paths(),
        }
    }
}

/// One detected shell launcher that includes both the shell family and the concrete executable path or name.
/// 一条已检测的 shell 启动器，包含 shell 族与具体可执行路径或名称。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedHostCommandShell {
    /// Stable shell family identifier.
    /// 稳定的 shell 族标识。
    pub shell: HostCommandShell,
    /// Concrete executable path or program name that should be spawned by the host.
    /// 宿主应实际启动的具体可执行路径或程序名。
    pub launcher_program: String,
}

impl DetectedHostCommandShell {
    /// Builds the concrete launcher program, argument vector, and host-visible shell label for one raw command.
    /// 为一条原始命令构建具体启动程序、参数向量与宿主可见 shell 标签。
    pub fn build_launcher(&self, command_text: &str) -> (String, Vec<String>, String) {
        (
            self.launcher_program.clone(),
            self.shell.build_launcher_args(command_text),
            self.shell.id().to_string(),
        )
    }
}

/// Resolved host-shell support snapshot shared by settings rendering and native tool execution.
/// 由设置渲染与原生工具执行共享的宿主 shell 支持快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostCommandShellSupport {
    /// Concrete shell launchers currently supported on the host.
    /// 当前宿主真正支持的具体 shell 启动器集合。
    supported_shells: Vec<DetectedHostCommandShell>,
    /// Optional user-configured default shell used to override `auto` resolution.
    /// 用于覆盖 `auto` 解析的可选用户默认 shell。
    configured_default_shell: Option<HostCommandShell>,
}

impl HostCommandShellSupport {
    /// Builds one support snapshot from the persisted default-shell identifier and the detected host launchers.
    /// 根据持久化默认 shell 标识与宿主检测到的启动器构建一份支持快照。
    pub fn from_configured_default_id(configured_default_shell_id: Option<&str>) -> Self {
        let supported_shells = detected_host_command_shells().to_vec();
        let configured_default_shell = configured_default_shell_id
            .and_then(HostCommandShell::parse)
            .filter(|shell| supported_shells.iter().any(|entry| entry.shell == *shell));

        Self {
            supported_shells,
            configured_default_shell,
        }
    }

    /// Builds one explicit support snapshot from caller-provided launcher rows and one optional configured default shell.
    /// 根据调用方提供的启动器集合与可选配置默认 shell 构建一份显式支持快照。
    pub fn from_test_parts(
        supported_shells: Vec<DetectedHostCommandShell>,
        configured_default_shell: Option<HostCommandShell>,
    ) -> Self {
        Self {
            supported_shells,
            configured_default_shell,
        }
    }

    /// Returns the detected host shell launchers in stable priority order.
    /// 按稳定优先级顺序返回检测到的宿主 shell 启动器集合。
    pub fn supported_shells(&self) -> &[DetectedHostCommandShell] {
        &self.supported_shells
    }

    /// Returns the concrete shell launcher selected by the current `auto` policy.
    /// 返回当前 `auto` 策略选中的具体 shell 启动器。
    pub fn auto_shell(&self) -> Option<&DetectedHostCommandShell> {
        self.configured_default_shell
            .and_then(|shell| {
                self.supported_shells
                    .iter()
                    .find(|entry| entry.shell == shell)
            })
            .or_else(|| self.supported_shells.first())
    }

    /// Returns whether the current `auto` resolution is driven by the user-configured default shell.
    /// 返回当前 `auto` 解析是否由用户配置的默认 shell 驱动。
    pub fn auto_uses_user_preference(&self) -> bool {
        self.configured_default_shell
            .zip(self.auto_shell().map(|entry| entry.shell))
            .is_some_and(|(configured, resolved)| configured == resolved)
    }

    /// Returns the stable argument identifiers exposed by the shell tool schema.
    /// 返回 shell 工具 schema 对外暴露的稳定参数标识。
    pub fn schema_shell_ids(&self) -> Vec<String> {
        let mut ids = vec![String::from("auto")];
        ids.extend(
            self.supported_shells
                .iter()
                .map(|entry| entry.shell.id().to_string()),
        );
        ids
    }

    /// Returns the dynamic shell-field description advertised to AI agents.
    /// 返回对 AI 代理声明的动态 shell 字段描述。
    pub fn schema_shell_description(&self) -> String {
        let supported = if self.supported_shells.is_empty() {
            String::from("none detected")
        } else {
            self.supported_shells
                .iter()
                .map(|entry| entry.shell.id())
                .collect::<Vec<_>>()
                .join(", ")
        };
        let auto_summary = self
            .auto_shell()
            .map(|entry| {
                if self.auto_uses_user_preference() {
                    format!("{} (user default)", entry.shell.id())
                } else {
                    entry.shell.id().to_string()
                }
            })
            .unwrap_or_else(|| String::from("unresolved"));
        let windows_unix_shell_note = if cfg!(windows)
            && self.supported_shells.iter().any(|entry| {
                matches!(
                    entry.shell,
                    HostCommandShell::Bash | HostCommandShell::Sh | HostCommandShell::Zsh
                )
            }) {
            " On Windows, bash/sh/zsh require a compatible Unix-like environment such as Git Bash or another PATH-visible launcher."
        } else {
            ""
        };

        format!(
            "Optional shell selector. Defaults to `auto`. Supported on this host: {supported}. `auto` currently resolves to: {auto_summary}.{windows_unix_shell_note}"
        )
    }

    /// Resolves one optional shell selector into the concrete launcher that should be spawned by the host.
    /// 将一条可选 shell 选择器解析为宿主实际应启动的具体启动器。
    pub fn resolve_requested_shell(
        &self,
        requested_shell_id: Option<&str>,
    ) -> Result<DetectedHostCommandShell, String> {
        let normalized = requested_shell_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("auto")
            .to_ascii_lowercase();

        if normalized == "auto" {
            return self.auto_shell().cloned().ok_or_else(|| {
                String::from("No supported command shell was detected on this host.")
            });
        }

        let requested_shell = HostCommandShell::parse(&normalized)
            .ok_or_else(|| format!("Unsupported shell type: {normalized}."))?;
        self.supported_shells
            .iter()
            .find(|entry| entry.shell == requested_shell)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "No available launcher for shell `{}` was detected on this host.",
                    requested_shell.id()
                )
            })
    }
}

/// Returns the cached host shell detection result reused across repeated settings renders and tool invocations.
/// 返回供重复设置渲染与工具调用复用的宿主 shell 检测缓存结果。
fn detected_host_command_shells() -> &'static [DetectedHostCommandShell] {
    static DETECTED: OnceLock<Vec<DetectedHostCommandShell>> = OnceLock::new();
    DETECTED.get_or_init(detect_supported_host_command_shells)
}

/// Detects the supported host shell launchers in stable platform-specific priority order.
/// 以稳定的平台优先级顺序检测受支持的宿主 shell 启动器。
fn detect_supported_host_command_shells() -> Vec<DetectedHostCommandShell> {
    candidate_shell_order()
        .iter()
        .filter_map(|shell| {
            resolve_launcher_program(*shell).map(|launcher_program| DetectedHostCommandShell {
                shell: *shell,
                launcher_program,
            })
        })
        .collect()
}

/// Returns the platform-specific shell priority order used when no user override is configured.
/// 返回在未配置用户覆盖时使用的平台特定 shell 优先级顺序。
fn candidate_shell_order() -> &'static [HostCommandShell] {
    if cfg!(windows) {
        &[
            HostCommandShell::Pwsh,
            HostCommandShell::Powershell,
            HostCommandShell::Cmd,
            HostCommandShell::Bash,
            HostCommandShell::Sh,
            HostCommandShell::Zsh,
        ]
    } else if cfg!(target_os = "macos") {
        &[
            HostCommandShell::Zsh,
            HostCommandShell::Bash,
            HostCommandShell::Sh,
            HostCommandShell::Pwsh,
        ]
    } else {
        &[
            HostCommandShell::Bash,
            HostCommandShell::Zsh,
            HostCommandShell::Sh,
            HostCommandShell::Pwsh,
        ]
    }
}

/// Resolves the concrete launcher program for one shell family by checking PATH entries and known installation directories.
/// 通过检查 PATH 条目与已知安装目录解析某个 shell 族的具体启动程序。
fn resolve_launcher_program(shell: HostCommandShell) -> Option<String> {
    search_path_for_program(shell.candidate_program_names()).or_else(|| {
        shell
            .candidate_absolute_programs()
            .into_iter()
            .find(|path| path.is_file())
            .map(|path| path.display().to_string())
    })
}

/// Searches every PATH entry for the first matching executable candidate and returns the concrete path string.
/// 在全部 PATH 条目中搜索首个匹配的可执行候选项，并返回具体路径字符串。
fn search_path_for_program(program_names: &[&str]) -> Option<String> {
    let path_value = env::var_os("PATH")?;
    let path_entries = env::split_paths(&path_value).collect::<Vec<_>>();

    program_names.iter().find_map(|program_name| {
        path_entries
            .iter()
            .find_map(|directory| find_program_in_directory(directory, program_name))
            .map(|path| path.display().to_string())
    })
}

/// Searches one directory for the requested program name and platform-specific executable extensions.
/// 在单个目录中搜索请求的程序名及其平台相关可执行扩展名。
fn find_program_in_directory(directory: &Path, program_name: &str) -> Option<PathBuf> {
    let direct_candidate = directory.join(program_name);
    if direct_candidate.is_file() {
        return Some(direct_candidate);
    }

    if !cfg!(windows) {
        return None;
    }

    let normalized_program = program_name.to_ascii_lowercase();
    windows_path_extensions().into_iter().find_map(|extension| {
        if normalized_program.ends_with(&extension.to_ascii_lowercase()) {
            return None;
        }
        let candidate = directory.join(format!("{program_name}{extension}"));
        candidate.is_file().then_some(candidate)
    })
}

/// Returns the current Windows executable extension list used when searching PATH directories.
/// 返回在搜索 PATH 目录时使用的当前 Windows 可执行扩展名列表。
fn windows_path_extensions() -> Vec<String> {
    let configured = env::var_os("PATHEXT")
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| String::from(".COM;.EXE;.BAT;.CMD"));
    let mut extensions = configured
        .split(';')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .collect::<Vec<_>>();

    if extensions.is_empty() {
        extensions = vec![
            String::from(".COM"),
            String::from(".EXE"),
            String::from(".BAT"),
            String::from(".CMD"),
        ];
    }

    extensions
}

/// Returns the Windows system root when it is available in the host environment.
/// 返回宿主环境可用时的 Windows 系统根目录。
fn windows_system_root() -> Option<PathBuf> {
    env::var_os("SystemRoot").map(PathBuf::from)
}

/// Returns the current Windows Program Files directories used to probe PowerShell installations.
/// 返回用于探测 PowerShell 安装位置的当前 Windows Program Files 目录集合。
fn windows_program_files_roots() -> Vec<PathBuf> {
    ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"]
        .into_iter()
        .filter_map(env::var_os)
        .map(PathBuf::from)
        .collect()
}

/// Returns candidate absolute paths for PowerShell 7 (`pwsh`) across Windows and Unix-like hosts.
/// 返回 Windows 与类 Unix 宿主上 PowerShell 7（`pwsh`）的候选绝对路径。
fn candidate_pwsh_paths() -> Vec<PathBuf> {
    if cfg!(windows) {
        return windows_program_files_roots()
            .into_iter()
            .map(|root| root.join("PowerShell").join("7").join("pwsh.exe"))
            .collect();
    }

    vec![
        PathBuf::from("/usr/local/bin/pwsh"),
        PathBuf::from("/usr/bin/pwsh"),
        PathBuf::from("/opt/microsoft/powershell/7/pwsh"),
    ]
}

/// Returns candidate absolute paths for Windows PowerShell 5.x.
/// 返回 Windows PowerShell 5.x 的候选绝对路径。
fn candidate_windows_powershell_paths() -> Vec<PathBuf> {
    if cfg!(windows) {
        return windows_system_root()
            .into_iter()
            .map(|root| {
                root.join("System32")
                    .join("WindowsPowerShell")
                    .join("v1.0")
                    .join("powershell.exe")
            })
            .collect();
    }

    Vec::new()
}

/// Returns candidate absolute paths for Windows Command Prompt.
/// 返回 Windows 命令提示符的候选绝对路径。
fn candidate_cmd_paths() -> Vec<PathBuf> {
    if cfg!(windows) {
        let mut candidates = Vec::new();
        if let Some(com_spec) = env::var_os("ComSpec").map(PathBuf::from) {
            candidates.push(com_spec);
        }
        if let Some(system_root) = windows_system_root() {
            candidates.push(system_root.join("System32").join("cmd.exe"));
        }
        return candidates;
    }

    Vec::new()
}

/// Returns candidate absolute paths for Bash across Windows Git installs and Unix-like hosts.
/// 返回 Windows Git 安装目录与类 Unix 宿主上的 Bash 候选绝对路径。
fn candidate_bash_paths() -> Vec<PathBuf> {
    if cfg!(windows) {
        return windows_program_files_roots()
            .into_iter()
            .flat_map(|root| {
                [
                    root.join("Git").join("bin").join("bash.exe"),
                    root.join("Git").join("usr").join("bin").join("bash.exe"),
                ]
            })
            .collect();
    }

    vec![
        PathBuf::from("/bin/bash"),
        PathBuf::from("/usr/bin/bash"),
        PathBuf::from("/usr/local/bin/bash"),
    ]
}

/// Returns candidate absolute paths for Zsh across Unix-like hosts.
/// 返回类 Unix 宿主上的 Zsh 候选绝对路径。
fn candidate_zsh_paths() -> Vec<PathBuf> {
    if cfg!(windows) {
        return Vec::new();
    }

    vec![
        PathBuf::from("/bin/zsh"),
        PathBuf::from("/usr/bin/zsh"),
        PathBuf::from("/usr/local/bin/zsh"),
    ]
}

/// Returns candidate absolute paths for POSIX `sh` across Unix-like hosts and Git installs on Windows.
/// 返回类 Unix 宿主上的 POSIX `sh` 与 Windows Git 安装目录中的候选绝对路径。
fn candidate_sh_paths() -> Vec<PathBuf> {
    if cfg!(windows) {
        return windows_program_files_roots()
            .into_iter()
            .map(|root| root.join("Git").join("usr").join("bin").join("sh.exe"))
            .collect();
    }

    vec![PathBuf::from("/bin/sh"), PathBuf::from("/usr/bin/sh")]
}

#[cfg(test)]
mod tests {
    use super::{DetectedHostCommandShell, HostCommandShell, HostCommandShellSupport};

    /// Builds one compact test launcher row for the requested shell family.
    /// 为请求的 shell 族构建一条紧凑测试启动器记录。
    fn detected(shell: HostCommandShell) -> DetectedHostCommandShell {
        DetectedHostCommandShell {
            launcher_program: shell.id().to_string(),
            shell,
        }
    }

    /// Verifies `auto` prefers the user-configured default shell when that shell is available on the host.
    /// 验证当用户配置的默认 shell 在宿主可用时，`auto` 会优先采用该 shell。
    #[test]
    fn auto_prefers_user_configured_default_shell() {
        let support = HostCommandShellSupport::from_test_parts(
            vec![
                detected(HostCommandShell::Pwsh),
                detected(HostCommandShell::Powershell),
                detected(HostCommandShell::Cmd),
            ],
            Some(HostCommandShell::Cmd),
        );

        let resolved = support
            .resolve_requested_shell(Some("auto"))
            .expect("auto should resolve");
        assert_eq!(resolved.shell, HostCommandShell::Cmd);
        assert!(support.auto_uses_user_preference());
    }

    /// Verifies `auto` falls back to the first detected launcher when no user default is configured.
    /// 验证在未配置用户默认值时，`auto` 会回退到首个检测到的启动器。
    #[test]
    fn auto_falls_back_to_first_detected_shell() {
        let support = HostCommandShellSupport::from_test_parts(
            vec![
                detected(HostCommandShell::Pwsh),
                detected(HostCommandShell::Powershell),
            ],
            None,
        );

        let resolved = support
            .resolve_requested_shell(None)
            .expect("auto should resolve without explicit selector");
        assert_eq!(resolved.shell, HostCommandShell::Pwsh);
        assert!(!support.auto_uses_user_preference());
    }

    /// Verifies the dynamic schema shell-id list always starts with `auto` and then exposes only detected shells.
    /// 验证动态 schema shell 标识列表总是以 `auto` 开头，并且随后仅暴露检测到的 shell。
    #[test]
    fn schema_shell_ids_start_with_auto() {
        let support = HostCommandShellSupport::from_test_parts(
            vec![
                detected(HostCommandShell::Pwsh),
                detected(HostCommandShell::Cmd),
            ],
            None,
        );

        assert_eq!(
            support.schema_shell_ids(),
            vec![
                String::from("auto"),
                String::from("pwsh"),
                String::from("cmd"),
            ]
        );
    }

    /// Verifies explicit shell resolution rejects unsupported shells instead of silently falling back.
    /// 验证显式 shell 解析会拒绝不受支持的 shell，而不是静默回退。
    #[test]
    fn explicit_shell_resolution_rejects_unsupported_shells() {
        let support =
            HostCommandShellSupport::from_test_parts(vec![detected(HostCommandShell::Pwsh)], None);

        let error = support
            .resolve_requested_shell(Some("powershell"))
            .expect_err("unsupported explicit shell should fail");
        assert!(error.contains("未检测到"));
    }
}
