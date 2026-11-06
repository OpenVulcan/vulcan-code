//! What an installation run reports while it works, and how the person confirms it: printed lines
//! without a terminal (or with `--yes`), the interactive installer's pages otherwise
//! (`session.rs`). Printed text stays English; the pages take theirs from the `setup` catalog.
//! 一次安装运行在工作期间报告的内容，以及用户如何确认：没有终端（或带 `--yes`）时打印文字行，否则显示
//! 交互式安装界面的各页（`session.rs`）。打印的文字保持英文；各页的文案取自 `setup` 目录。

use std::io::IsTerminal;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use vulcan_install_core::names::{Channel, ComponentName, Platform};

use crate::integrate::Location;
use crate::prompt::{confirm, megabytes};

/// The error a run returns when the person goes back from the review to the wizard; nothing was
/// changed.
/// 用户从审阅页返回向导时运行返回的错误；未做任何改动。
pub const BACK: &str = "back";

/// One step of an installation run.
/// 安装运行的一个步骤。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// Looking up the newest version on a channel.
    /// 在某通道上查找最新版本。
    Checking(Channel),
    /// Finding which files are already here, which decides the download.
    /// 查找已有的文件，由此决定下载量。
    Preparing,
    /// Downloading and verifying the rest.
    /// 下载并校验其余部分。
    Downloading,
    /// Making the new version the current one.
    /// 把新版本设为当前版本。
    Switching,
    /// Commands, shortcuts and the uninstall entry.
    /// 命令、快捷方式与卸载项。
    Integrating,
    /// Registering the Core service.
    /// 注册 Core 服务。
    Service,
    /// Removing older versions.
    /// 移除更早的版本。
    Cleaning,
}

impl Stage {
    /// Whether the run can still be cancelled without changing the installation.
    /// 运行此时是否仍可取消而不改动安装。
    pub fn cancellable(self) -> bool {
        matches!(
            self,
            Self::Checking(_) | Self::Preparing | Self::Downloading
        )
    }
}

/// What a run does to the installation.
/// 一次运行对安装所做的事。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// A first installation.
    /// 首次安装。
    Install,
    /// A newer version replaces `from`.
    /// 更新的版本替换 `from`。
    Update {
        /// The version installed before.
        /// 之前安装的版本。
        from: String,
    },
    /// The installed version is kept and its integration is set up again.
    /// 保留已安装的版本并重新设置其集成。
    Refresh,
}

/// Everything the person confirms before anything is downloaded or switched.
/// 在下载或切换任何内容之前由用户确认的全部内容。
#[derive(Clone, Debug)]
pub struct Review {
    /// The platform.
    /// 平台。
    pub platform: Platform,
    /// What the run does.
    /// 运行所做的事。
    pub action: Action,
    /// The version installed or kept.
    /// 安装或保留的版本。
    pub version: String,
    /// The release channel.
    /// 发布通道。
    pub channel: Channel,
    /// The components.
    /// 组件。
    pub components: Vec<ComponentName>,
    /// The install root.
    /// 安装根目录。
    pub directory: PathBuf,
    /// Exactly the bytes the download receives: files already here are reused.
    /// 下载将接收的确切字节数：已有的文件会被复用。
    pub download: u64,
    /// Size of the installed files, for a first installation.
    /// 已安装文件的大小，仅用于首次安装。
    pub installed_size: Option<u64>,
    /// Every location outside the root that is written.
    /// 在根目录之外写入的每个位置。
    pub locations: Vec<Location>,
    /// The port range set in this run.
    /// 本次设置的端口范围。
    pub port_range: Option<String>,
}

/// The person's answer to a review.
/// 用户对审阅的回答。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    /// Go on.
    /// 继续。
    Continue,
    /// Return to the wizard.
    /// 返回向导。
    Back,
    /// Leave without changing anything.
    /// 不做任何改动直接离开。
    Cancel,
}

/// How the desktop app is opened after the run.
/// 运行结束后如何打开桌面应用。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DesktopWay {
    /// The Start menu entry (Windows).
    /// 开始菜单项（Windows）。
    StartMenu,
    /// The application menu entry (Linux).
    /// 应用程序菜单项（Linux）。
    AppMenu,
    /// The desktop shortcut.
    /// 桌面快捷方式。
    Shortcut,
    /// Spotlight or the application bundle at this path (macOS).
    /// Spotlight 或此路径下的应用程序包（macOS）。
    Bundle(PathBuf),
    /// This command or program.
    /// 此命令或程序。
    Program(String),
}

/// What a finished run did, and how to go on from there.
/// 已完成的运行做了什么，以及之后如何继续。
#[derive(Clone, Debug)]
pub struct Outcome {
    /// The platform.
    /// 平台。
    pub platform: Platform,
    /// What the run did.
    /// 运行所做的事。
    pub action: Action,
    /// The version now current.
    /// 当前版本。
    pub version: String,
    /// The install root.
    /// 安装根目录。
    pub directory: PathBuf,
    /// The command that starts the terminal client.
    /// 启动终端客户端的命令。
    pub terminal: String,
    /// How the desktop app is opened, when it is installed.
    /// 桌面应用的打开方式（已安装时）。
    pub desktop: Option<DesktopWay>,
    /// The command that runs the setup program again.
    /// 再次运行安装器的命令。
    pub setup: String,
    /// Versions removed in this run.
    /// 本次移除的版本。
    pub removed: Vec<String>,
    /// Whether the built-in browser still needs system libraries (Linux).
    /// 内置浏览器是否仍需系统库（Linux）。
    pub browser_dependencies: bool,
    /// Notes of the integration, in English.
    /// 集成的说明，英文。
    pub notes: Vec<String>,
}

/// The command that installs the built-in browser's system libraries on Linux.
/// 在 Linux 上安装内置浏览器系统库的命令。
pub const BROWSER_DEPENDENCIES_COMMAND: &str = "vulcan-code-service --install-browser-dependencies";

impl Outcome {
    /// Prints the outcome, so it stays in the terminal after the interactive installer closes.
    /// 打印结果，使其在交互式安装界面关闭后仍留在终端中。
    pub fn print(&self) {
        println!();
        let directory = self.directory.display();
        match &self.action {
            Action::Install => println!("Installed Vulcan Code {} into {directory}.", self.version),
            Action::Update { from } => println!(
                "Updated Vulcan Code from {from} to {} in {directory}.",
                self.version
            ),
            Action::Refresh => {
                println!("Vulcan Code {} in {directory} was refreshed.", self.version)
            }
        }
        println!("Terminal: {}", self.terminal);
        match &self.desktop {
            Some(DesktopWay::StartMenu) => println!("Desktop app: Start menu > Vulcan Code"),
            Some(DesktopWay::AppMenu) => {
                println!("Desktop app: Vulcan Code in the application menu")
            }
            Some(DesktopWay::Shortcut) => println!("Desktop app: the Vulcan Code desktop shortcut"),
            // The app goes to the Applications folder of the user's home, which Finder does not
            // show by default, so the way to it is named.
            // 应用放在用户主目录的「应用程序」文件夹中，Finder 默认不显示它，因此说明打开方式。
            Some(DesktopWay::Bundle(path)) => println!(
                "Desktop app: Spotlight (search \"Vulcan Code\") or {}",
                path.display()
            ),
            Some(DesktopWay::Program(program)) => println!("Desktop app: {program}"),
            None => {}
        }
        if !self.removed.is_empty() {
            println!("Removed older versions: {}.", self.removed.join(", "));
        }
        if self.browser_dependencies {
            println!(
                "The built-in browser needs system libraries once: {BROWSER_DEPENDENCIES_COMMAND} \
                 (uses sudo)."
            );
        }
        for note in &self.notes {
            println!("{note}");
        }
        let setup = &self.setup;
        println!("Next time:");
        println!("  {setup}  changes the components, shortcuts or channel");
        println!("  {setup} update  installs the newest version");
        println!(
            "  {setup} uninstall  removes Vulcan Code (add --purge to also delete your sessions \
             and settings)"
        );
        if self.platform.is_windows() {
            println!("  Settings > Apps > Vulcan Code also removes it.");
        }
        println!("The setup program updates itself first whenever it installs or updates.");
    }
}

/// Receives the steps of a run and asks for its confirmation.
/// 接收运行的各步骤并请求确认。
pub trait Reporter: Sync {
    /// A step begins.
    /// 某步骤开始。
    fn stage(&self, stage: Stage);
    /// `received` of `total` download bytes have arrived.
    /// 下载已到达 `total` 字节中的 `received` 字节。
    fn progress(&self, received: u64, total: u64);
    /// The flag that cancels the run's transfers.
    /// 取消本次运行传输的标志。
    fn cancel_with(&self, flag: Arc<AtomicBool>);
    /// Asks whether to go on with `review`.
    /// 询问是否按 `review` 继续。
    fn review(&self, review: &Review) -> Result<Decision, String>;
}

/// Reports with printed lines and asks with a `[Y/n]` question.
/// 以打印行报告，以 `[Y/n]` 问题询问。
pub struct Plain {
    /// Whether `--yes` answers the question.
    /// `--yes` 是否回答该问题。
    yes: bool,
    /// Whether the progress line is waiting for its line break.
    /// 进度行是否尚待换行。
    open_line: AtomicBool,
}

impl Plain {
    /// A reporter that asks unless `yes`.
    /// 除非 `yes`，否则会询问的报告器。
    pub fn new(yes: bool) -> Self {
        Self {
            yes,
            open_line: AtomicBool::new(false),
        }
    }

    /// Ends the progress line.
    /// 结束进度行。
    fn close_line(&self) {
        if self.open_line.swap(false, Ordering::Relaxed) {
            eprintln!();
        }
    }
}

impl Reporter for Plain {
    fn stage(&self, stage: Stage) {
        self.close_line();
        match stage {
            Stage::Checking(channel) => println!("Checking the {} channel...", channel.as_str()),
            Stage::Preparing => println!("Checking which files are already here..."),
            Stage::Service => println!("Registering the service..."),
            Stage::Downloading | Stage::Switching | Stage::Integrating | Stage::Cleaning => {}
        }
    }

    fn progress(&self, received: u64, total: u64) {
        if std::io::stderr().is_terminal() {
            eprint!(
                "\r  downloading {} of {}   ",
                megabytes(received),
                megabytes(total)
            );
            self.open_line.store(true, Ordering::Relaxed);
        }
    }

    fn cancel_with(&self, _flag: Arc<AtomicBool>) {}

    fn review(&self, review: &Review) -> Result<Decision, String> {
        self.close_line();
        println!();
        let version = &review.version;
        match &review.action {
            Action::Install => println!("Install Vulcan Code {version}"),
            Action::Update { from } => println!("Update Vulcan Code {from} to {version}"),
            Action::Refresh => {
                println!("Vulcan Code {version} is up to date; refresh its integration")
            }
        }
        if review.action != Action::Refresh {
            let components: Vec<&str> = review
                .components
                .iter()
                .map(ComponentName::as_str)
                .collect();
            println!("  components: {}", components.join(", "));
            match review.download {
                0 => println!("  download: nothing, every file is already here"),
                bytes => println!("  download: {}", megabytes(bytes)),
            }
            if let Some(size) = review.installed_size {
                println!("  size on disk: {}", megabytes(size));
            }
        }
        println!("  directory: {}", review.directory.display());
        for location in &review.locations {
            println!("  {location}");
        }
        if let Some(range) = &review.port_range {
            println!("  service ports: {range}");
        }
        Ok(if confirm("Continue?", self.yes)? {
            Decision::Continue
        } else {
            Decision::Cancel
        })
    }
}
