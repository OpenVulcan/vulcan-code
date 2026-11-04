//! Independent Vulcan Code service launch modes and startup coordination.
//! 独立 Vulcan Code 服务启动模式与启动协调。

use std::path::PathBuf;

/// Stable service process name shared by discovery, logs, and the startup description.
/// 发现、日志与启动描述共享的稳定服务进程名。
pub const SERVICE_NAME: &str = "vulcan-code-service";

/// GUI-initiated launch: acquire the data-root lock and stay in the background.
/// GUI 发起的启动：获取数据根锁并留在后台，不创建工作台。
pub const ARG_GUI_LAUNCH: &str = "--gui-launch";

/// Standalone launch with a desktop tray when a desktop session exists.
/// 独立启动；在有桌面会话时提供托盘。
pub const ARG_SERVICE: &str = "--service";

/// Headless launch without tray or WebView, hosted by a terminal or process manager.
/// 无头启动；不初始化托盘或 WebView，由终端或进程管理器托管。
pub const ARG_HEADLESS: &str = "--headless";

/// Describe-only launch: print build configuration without starting business logic.
/// 仅描述启动：输出构建配置，不启动业务逻辑。
pub const ARG_DESCRIBE: &str = "--describe";

/// Test-only data root used by isolated integration processes.
/// 仅测试使用的数据根，供隔离集成进程使用。
pub const ARG_TEST_ROOT: &str = "--test-root";

/// Suppresses the single Debug Core console when an external terminal or harness owns diagnostics.
/// 外部终端或测试宿主持有诊断输出时，屏蔽 Debug Core 的单一控制台。
pub const ARG_NO_DEBUG_CONSOLE: &str = "--no-debug-console";

/// Set by a service that restarts into an updated version: the replacement waits for the old
/// owner to release the data-root lock instead of exiting as a losing concurrent launcher.
/// 由重启到更新版本的服务设置：替代进程等待旧所有者释放数据根锁，而不是作为竞争失败的并发启动器退出。
pub const ARG_AWAIT_OWNER_EXIT: &str = "--await-owner-exit";

/// How long a replacement service waits for the old owner to exit.
/// 替代服务等待旧所有者退出的时长。
pub const OWNER_EXIT_WAIT: std::time::Duration = std::time::Duration::from_secs(120);

/// Whether `args` ask to wait for the old owner (see [`ARG_AWAIT_OWNER_EXIT`]).
/// `args` 是否要求等待旧所有者（见 [`ARG_AWAIT_OWNER_EXIT`]）。
pub fn await_owner_exit_requested(args: &[String]) -> bool {
    args.iter().skip(1).any(|arg| arg == ARG_AWAIT_OWNER_EXIT)
}

/// Selected service launch mode parsed from process arguments.
/// 从进程参数解析出的服务启动模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceLaunchMode {
    /// GUI-initiated background launch without a workbench window.
    /// GUI 发起的后台启动，不创建工作台窗口。
    GuiLaunch,
    /// Standalone launch with tray support when available.
    /// 独立启动；可用时提供托盘支持。
    Service,
    /// Headless launch without tray or WebView.
    /// 无头启动；不含托盘或 WebView。
    Headless,
}

/// User-facing launch description printed by `--describe` without starting business logic.
/// `--describe` 输出的面向用户启动描述，不启动业务逻辑。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServiceLaunchDescription {
    /// Launch protocol version understood by GUI launchers.
    /// GUI 启动器理解的启动协议版本。
    pub protocol_version: u32,
    /// Stable service process name.
    /// 稳定服务进程名。
    pub service_name: String,
    /// Cargo package version of this service build.
    /// 本服务构建的 Cargo 包版本。
    pub service_version: String,
    /// Cargo profile that built this executable.
    /// 构建本可执行文件的 Cargo 配置。
    pub profile: String,
    /// Canonical path of the running executable.
    /// 运行中可执行文件的规范路径。
    pub executable_path: PathBuf,
    /// Authoritative user data root for this profile; GUI launchers must not rebuild it.
    /// 本配置的权威用户数据根；GUI 启动器不得重新拼接。
    pub data_root: PathBuf,
    /// Discovery file path below the authoritative data root.
    /// 权威数据根下的发现文件路径。
    pub discovery_path: PathBuf,
    /// Exclusive lock file path below the authoritative data root.
    /// 权威数据根下的排他锁文件路径。
    pub lock_path: PathBuf,
    /// WebUI mode served by this profile.
    /// 本配置提供的 WebUI 模式。
    pub default_web_ui_mode: String,
    /// Vite URL in debug builds; null in release builds.
    /// 开发构建的 Vite 地址；发布构建为 null。
    pub vite_url: Option<String>,
    /// Launch modes supported by this compiled artifact.
    /// 本编译产物支持的启动模式。
    pub modes: Vec<String>,
}

/// Parses one service launch mode from explicit process arguments.
/// 从显式进程参数解析一个服务启动模式.
///
/// # Parameters
/// 参数
/// `args` are the full process arguments including the executable name.
/// `args` 是包含可执行文件名的完整进程参数。
///
/// # Returns
/// 返回值
/// The selected launch mode, or a user-facing diagnostic for missing or conflicting modes.
/// 返回选定的启动模式；缺失或冲突时返回面向用户的诊断。
pub fn parse_service_launch_args(args: &[String]) -> Result<ServiceLaunchMode, String> {
    parse_service_launch_args_impl(args, cfg!(feature = "test-support"))
}

/// Parses one service launch mode while recording whether `--describe` was requested.
/// 在解析启动模式的同时记录是否请求 `--describe`。
pub(crate) fn parse_service_launch_args_impl(
    args: &[String],
    allow_test_root: bool,
) -> Result<ServiceLaunchMode, String> {
    let mut selected: Option<ServiceLaunchMode> = None;
    let mut describe_requested = false;
    // pending_test_root_value consumes the documented two-argument test-root form.
    // pending_test_root_value 消费已声明的双参数测试根形式。
    let mut pending_test_root_value = false;
    for arg in args.iter().skip(1) {
        if pending_test_root_value {
            if arg.starts_with("--") {
                return Err(String::from("--test-root requires a directory value"));
            }
            pending_test_root_value = false;
            continue;
        }
        if arg == ARG_TEST_ROOT {
            if !allow_test_root {
                return Err(String::from(
                    "--test-root is available only in test-support builds",
                ));
            }
            pending_test_root_value = true;
            continue;
        }
        // test_root overrides the user namespace only for isolated integration processes.
        // test_root 仅为隔离集成进程覆盖用户命名空间。
        if test_root_argument_value(arg).is_some() {
            if !allow_test_root {
                return Err(String::from(
                    "--test-root is available only in test-support builds",
                ));
            }
            continue;
        }
        if arg == ARG_NO_DEBUG_CONSOLE || arg == ARG_AWAIT_OWNER_EXIT {
            continue;
        }
        let mode = match arg.as_str() {
            ARG_GUI_LAUNCH => Some(ServiceLaunchMode::GuiLaunch),
            ARG_SERVICE => Some(ServiceLaunchMode::Service),
            ARG_HEADLESS => Some(ServiceLaunchMode::Headless),
            ARG_DESCRIBE => {
                describe_requested = true;
                None
            }
            _ => return Err(format!("unknown service argument: {arg}")),
        };
        if let Some(mode) = mode {
            if selected.is_some() {
                return Err(String::from(
                    "conflicting service launch modes; specify only one of --gui-launch, --service, --headless",
                ));
            }
            selected = Some(mode);
        }
    }
    if pending_test_root_value {
        return Err(String::from("--test-root requires a directory value"));
    }
    if describe_requested {
        return Err(String::from(
            "--describe takes no launch mode and starts no service; rerun without a launch mode to print the description",
        ));
    }
    selected.ok_or_else(|| {
        String::from("missing service launch mode; specify --gui-launch, --service, --headless, or --describe")
    })
}

/// Validate the describe-only invocation before starting any service business logic.
/// 在启动任何服务业务逻辑前校验仅描述调用。
/// args are complete process arguments; returns whether describe is selected or an error for mixed launch flags.
/// args 是完整进程参数；返回是否选择描述模式，混用启动参数时返回错误。
pub fn service_describe_requested(args: &[String]) -> Result<bool, String> {
    // requested records the explicit describe flag without interpreting other launch modes.
    // requested 记录明确的描述标志，不解释其他启动模式。
    let requested = args.iter().skip(1).any(|arg| arg == ARG_DESCRIBE);
    if requested {
        // The test root must reach --describe so the GUI learns the same isolated data root as Core.
        // 测试根必须传给 --describe，使 GUI 获得与 Core 相同的隔离数据根。
        // describe_count rejects duplicated describe flags before any test-root installation.
        // describe_count 在安装任何测试根之前拒绝重复的描述标志。
        let mut describe_count = 0;
        // expecting_test_root_value preserves the supported two-argument test-root form.
        // expecting_test_root_value 保留已支持的测试根双参数形式。
        let mut expecting_test_root_value = false;
        for arg in args.iter().skip(1) {
            if expecting_test_root_value {
                if arg.starts_with("--") {
                    return Err(String::from("--test-root requires a directory value"));
                }
                expecting_test_root_value = false;
                continue;
            }
            if arg == ARG_DESCRIBE {
                describe_count += 1;
            } else if arg == ARG_TEST_ROOT {
                expecting_test_root_value = true;
            } else if arg != ARG_NO_DEBUG_CONSOLE && test_root_argument_value(arg).is_none() {
                return Err(String::from(
                    "--describe cannot be combined with a launch mode",
                ));
            }
        }
        if expecting_test_root_value {
            return Err(String::from("--test-root requires a directory value"));
        }
        if describe_count != 1 {
            return Err(String::from("--describe must be specified exactly once"));
        }
    }
    Ok(requested)
}

/// Reports whether a one-shot management `flag` was requested on its own.
/// 报告是否单独请求了一次性管理参数 `flag`。
///
/// # Parameters
/// 参数
/// `args` are the full process arguments including the executable name.
/// `args` 是包含可执行文件名的完整进程参数。
///
/// # Returns
/// 返回值
/// `Ok(true)` when the flag is the only argument, `Ok(false)` when absent, or an error when it is
/// combined with anything else.
/// 该参数为唯一参数时返回 `Ok(true)`；未出现时返回 `Ok(false)`；与其他参数混用时返回错误。
pub fn standalone_flag_requested(args: &[String], flag: &str) -> Result<bool, String> {
    let tail = args.get(1..).unwrap_or_default();
    if !tail.iter().any(|arg| arg == flag) {
        return Ok(false);
    }
    match tail {
        [_] => Ok(true),
        _ => Err(format!("{flag} must be the only argument")),
    }
}

/// Returns the isolated test root carried by `--test-root` or `--test-root=value`.
/// 返回 `--test-root` 或 `--test-root=value` 携带的隔离测试根。
pub fn test_root_argument_value(arg: &str) -> Option<&str> {
    if arg == ARG_TEST_ROOT {
        return Some("");
    }
    arg.strip_prefix("--test-root=")
}

/// Returns the current service's test root as one argument for reopening its GUI.
/// 将当前服务的测试根整理成一个参数，供重新打开所属 GUI 使用。
/// args are complete validated process arguments; returns the normalized argument when present.
/// args 是已验证的完整进程参数；存在测试根时返回归一化参数。
#[cfg(any(feature = "desktop-tray", test))]
pub fn forwarded_test_root_argument(args: &[String]) -> Option<String> {
    // arguments starts after the executable and preserves the value following a split flag.
    // arguments 从可执行文件名之后开始，并保留双参数形式标志后面的目录值。
    let mut arguments = args.iter().skip(1);
    // argument is each original launch token examined for the two supported test-root forms.
    // argument 是逐项检查两种受支持测试根形式的原始启动参数。
    while let Some(argument) = arguments.next() {
        if argument == ARG_TEST_ROOT {
            return arguments
                .next()
                .map(|value| format!("{ARG_TEST_ROOT}={value}"));
        }
        if argument.starts_with("--test-root=") {
            return Some(argument.clone());
        }
    }
    None
}

/// Builds one service launch description for the current build profile.
/// 为当前构建配置构建一个服务启动描述。
pub fn service_launch_description(
    executable_path: PathBuf,
    data_root: PathBuf,
) -> ServiceLaunchDescription {
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    ServiceLaunchDescription {
        protocol_version: 1,
        service_name: String::from(SERVICE_NAME),
        service_version: String::from(vulcan_product_info::version()),
        profile: String::from(profile),
        executable_path,
        discovery_path: data_root.join("webui-service.json"),
        lock_path: data_root.join("service.lock"),
        data_root,
        default_web_ui_mode: if cfg!(debug_assertions) {
            String::from("vite")
        } else {
            String::from("service")
        },
        vite_url: if cfg!(debug_assertions) {
            Some(String::from("http://127.0.0.1:17601"))
        } else {
            None
        },
        modes: vec![
            String::from(ARG_GUI_LAUNCH),
            String::from(ARG_SERVICE),
            String::from(ARG_HEADLESS),
            String::from(ARG_DESCRIBE),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds process arguments from one executable name and tail flags.
    /// 从可执行文件名与尾部参数构建进程参数。
    fn launch_args(tail: &[&str]) -> Vec<String> {
        let mut args = vec![String::from("vulcan-code-service")];
        args.extend(tail.iter().map(|arg| (*arg).to_string()));
        args
    }

    /// Verifies a one-shot flag is recognized alone and rejected beside other arguments.
    /// 验证一次性参数单独使用时被识别，与其他参数混用时被拒绝。
    #[test]
    fn one_shot_flag_must_stand_alone() {
        let flag = "--one-shot";
        assert_eq!(
            standalone_flag_requested(&launch_args(&[]), flag),
            Ok(false)
        );
        assert_eq!(
            standalone_flag_requested(&launch_args(&[ARG_HEADLESS]), flag),
            Ok(false)
        );
        assert_eq!(
            standalone_flag_requested(&launch_args(&[flag]), flag),
            Ok(true)
        );
        assert!(standalone_flag_requested(&launch_args(&[flag, ARG_HEADLESS]), flag).is_err());
    }

    /// Verifies each launch mode parses without a describe conflict.
    /// 验证每个启动模式都能无描述冲突地解析。
    #[test]
    fn each_launch_mode_parses() {
        assert_eq!(
            parse_service_launch_args(&launch_args(&["--service"])),
            Ok(ServiceLaunchMode::Service)
        );
        assert_eq!(
            parse_service_launch_args(&launch_args(&["--gui-launch"])),
            Ok(ServiceLaunchMode::GuiLaunch)
        );
        assert_eq!(
            parse_service_launch_args(&launch_args(&["--headless"])),
            Ok(ServiceLaunchMode::Headless)
        );
    }

    /// Verifies terminal-owned Debug runs can suppress a separate console without changing mode.
    /// 验证终端持有的 Debug 运行可屏蔽额外控制台，同时不改变启动模式。
    #[test]
    fn debug_console_suppression_keeps_explicit_launch_mode() {
        assert_eq!(
            parse_service_launch_args(&launch_args(&["--gui-launch", ARG_NO_DEBUG_CONSOLE])),
            Ok(ServiceLaunchMode::GuiLaunch)
        );
    }

    /// Verifies conflicting launch modes are rejected with one diagnostic.
    /// 验证冲突的启动模式会被同一个诊断拒绝。
    #[test]
    fn conflicting_launch_modes_are_rejected() {
        assert!(parse_service_launch_args(&launch_args(&["--service", "--headless"])).is_err());
    }

    /// Verifies a missing launch mode reports usage instead of defaulting.
    /// 验证缺失启动模式时报告用法而不是默认选择。
    #[test]
    fn missing_launch_mode_reports_usage() {
        assert!(parse_service_launch_args(&launch_args(&[])).is_err());
    }

    /// Verifies describe is detected before business startup.
    /// 验证在业务启动前检测到描述请求。
    #[test]
    fn describe_request_is_detected() {
        assert_eq!(
            service_describe_requested(&launch_args(&["--describe"])),
            Ok(true)
        );
        assert_eq!(
            service_describe_requested(&launch_args(&["--service"])),
            Ok(false)
        );
        assert!(service_describe_requested(&launch_args(&["--describe", "--service"])).is_err());
        assert_eq!(
            service_describe_requested(&launch_args(&["--describe", "--test-root=C:/isolated"])),
            Ok(true)
        );
        assert_eq!(
            service_describe_requested(&launch_args(&["--describe", "--test-root", "C:/isolated"])),
            Ok(true)
        );
        assert_eq!(
            service_describe_requested(&launch_args(&["--describe", ARG_NO_DEBUG_CONSOLE])),
            Ok(true)
        );
        assert!(service_describe_requested(&launch_args(&["--describe", "--describe"])).is_err());
    }

    /// Verifies both test-root forms preserve the launch role and reject a missing value.
    /// 验证两种测试根形式保留启动角色，且拒绝缺失的目录值。
    #[test]
    fn test_root_forms_preserve_launch_mode() {
        assert_eq!(
            parse_service_launch_args_impl(
                &launch_args(&["--headless", "--test-root=C:/isolated"]),
                true,
            ),
            Ok(ServiceLaunchMode::Headless)
        );
        assert_eq!(
            parse_service_launch_args_impl(
                &launch_args(&["--headless", "--test-root", "C:/isolated"]),
                true,
            ),
            Ok(ServiceLaunchMode::Headless)
        );
        assert!(
            parse_service_launch_args_impl(&launch_args(&["--headless", "--test-root"]), true)
                .is_err()
        );
        assert!(
            parse_service_launch_args_impl(
                &launch_args(&["--headless", "--test-root", "--service"]),
                true,
            )
            .is_err()
        );
    }

    /// Verifies a tray-launched GUI receives the exact test root in either supported form.
    /// 验证托盘重开的 GUI 能从两种受支持形式获得精确测试根。
    #[test]
    fn tray_forwards_both_test_root_forms() {
        assert_eq!(
            forwarded_test_root_argument(&launch_args(&["--service", "--test-root=C:/isolated",])),
            Some(String::from("--test-root=C:/isolated"))
        );
        assert_eq!(
            forwarded_test_root_argument(&launch_args(&[
                "--service",
                "--test-root",
                "C:/isolated",
            ])),
            Some(String::from("--test-root=C:/isolated"))
        );
    }

    /// Verifies the description carries the authoritative data-root paths.
    /// 验证描述携带权威数据根路径。
    #[test]
    fn description_carries_authoritative_paths() {
        let data_root = PathBuf::from("example-vulcan-code");
        let description =
            service_launch_description(PathBuf::from("example-service"), data_root.clone());
        assert_eq!(description.protocol_version, 1);
        assert_eq!(description.service_name, SERVICE_NAME);
        assert_eq!(
            description.discovery_path,
            data_root.join("webui-service.json")
        );
        assert_eq!(description.lock_path, data_root.join("service.lock"));
        assert_eq!(description.modes.len(), 4);
    }
}
