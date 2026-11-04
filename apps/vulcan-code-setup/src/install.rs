//! Installing and updating: a first installation is an assembly from an empty root, so it uses the
//! same verified objects and code path as the in-app update (plan section 18.6).
//! 安装与更新：首次安装就是从空根目录开始的一次组装，因此与应用内更新使用同一批已校验对象和同一条
//! 代码路径（方案 18.6 节）。

use std::collections::BTreeSet;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use vulcan_install_core::assemble::Assembly;
use vulcan_install_core::fetch::{Downloader, Route};
use vulcan_install_core::index::ComponentIndex;
use vulcan_install_core::install_root::InstallRoot;
use vulcan_install_core::manifest::ReleaseManifest;
use vulcan_install_core::names::{Channel, ComponentName, Platform, ReleaseVersion};
use vulcan_install_core::record::RECORD_FILE_NAME;
use vulcan_install_core::signing::BUILT_IN_KEYS;
use vulcan_install_core::update::{find_update, read_indexes};

use crate::args::{BROWSER_COMPONENT, CommonOptions, InstallOptions};
use crate::integrate::{self, Plan, Recorder, SETUP_PROGRAM};
use crate::paths;
use crate::prompt::{confirm, megabytes};
use crate::root::{self, Current, Description, LEGACY_MARKER, Layout, Ownership};
use crate::state::{Item, Mode, SETUP_DIR, STATE_SCHEMA, SetupState};
use crate::wizard::{self, Draft};

/// User agent of every request.
/// 每个请求的用户代理。
const USER_AGENT: &str = "vulcan-code-setup";

/// Oldest glibc the Linux release runs on: it is built on the `ubuntu-22.04` runner
/// (`.github/workflows/release.yml`), whose glibc is 2.35.
/// Linux 发行版可运行的最旧 glibc：它在 `ubuntu-22.04` 运行器上构建（`.github/workflows/release.yml`），
/// 该系统的 glibc 为 2.35。
const MIN_GLIBC: (u32, u32) = (2, 35);

/// Components every mode installs, as named in `scripts/release-components.json`.
/// 每种模式都安装的组件，名称见 `scripts/release-components.json`。
const BASE_COMPONENTS: [&str; 2] = ["core", "tui"];

/// The desktop component, installed in desktop mode.
/// 桌面组件，在桌面模式下安装。
const DESKTOP_COMPONENT: &str = "gui";

/// The choices an installation is made with.
/// 安装所依据的选择。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choices {
    /// Installation mode.
    /// 安装模式。
    pub mode: Mode,
    /// Release channel.
    /// 发布通道。
    pub channel: Channel,
    /// Download origins.
    /// 下载源。
    pub route: Route,
    /// Optional components.
    /// 可选组件。
    pub with: Vec<ComponentName>,
    /// Whether a menu entry or application bundle is wanted.
    /// 是否需要菜单项或应用程序包。
    pub shortcut: bool,
    /// Whether commands are wanted on PATH.
    /// 是否需要把命令加入 PATH。
    pub path: bool,
}

impl Choices {
    /// Combines the options given with the choices recorded at the last installation.
    /// 将给出的选项与上次安装记录的选择合并。
    pub fn resolve(
        options: &InstallOptions,
        recorded: Option<&SetupState>,
        platform: Platform,
    ) -> Result<Self, String> {
        let mode = options
            .mode
            .or(recorded.map(|state| state.mode))
            .ok_or_else(|| String::from("choose --mode desktop, terminal or service"))?;
        if mode == Mode::Desktop && platform.is_windows() {
            return Err(String::from(
                "on Windows the desktop app is installed with VulcanCode-Setup.exe; this program \
                 installs --mode terminal",
            ));
        }
        if mode == Mode::Service && (platform.is_windows() || platform.is_macos()) {
            return Err(String::from("--mode service is available on Linux only"));
        }
        let mut with = recorded.map(|state| state.with.clone()).unwrap_or_default();
        for component in &options.with {
            if !with.contains(component) {
                with.push(component.clone());
            }
        }
        Ok(Self {
            mode,
            channel: options
                .channel
                .or(recorded.map(|state| state.channel))
                .unwrap_or(Channel::Stable),
            route: options
                .route
                .or(recorded.map(|state| state.route))
                .unwrap_or_default(),
            with,
            shortcut: !options.no_shortcut && recorded.is_none_or(|state| state.shortcut),
            path: !options.no_path && recorded.is_none_or(|state| state.path),
        })
    }

    /// The components this choice installs.
    /// 此选择安装的组件。
    pub fn components(&self) -> Result<BTreeSet<ComponentName>, String> {
        let mut names: Vec<&str> = BASE_COMPONENTS.to_vec();
        if self.mode == Mode::Desktop {
            names.push(DESKTOP_COMPONENT);
        }
        let mut components = BTreeSet::new();
        for name in names {
            components.insert(ComponentName::new(name).map_err(|error| error.to_string())?);
        }
        components.extend(self.with.iter().cloned());
        Ok(components)
    }

    /// The channels whose pointers are read.
    /// 读取其指针的通道。
    fn channels(&self) -> &'static [Channel] {
        match self.channel {
            Channel::Stable => &[Channel::Stable],
            Channel::Preview => &[Channel::Stable, Channel::Preview],
        }
    }
}

/// `vulcan-code-setup install`.
/// `vulcan-code-setup install`。
pub fn install(options: &InstallOptions) -> Result<(), String> {
    let platform = Platform::current().ok_or("this platform is not supported")?;
    check_system(platform)?;
    let root = resolve_root(options.common.prefix.as_deref(), platform)?;
    let ownership = root::ownership(&root)?;
    let recorded = match &ownership {
        Ownership::Owned(state) => Some(state),
        _ => None,
    };
    if !wizard::wanted(options.common.yes) {
        let choices = Choices::resolve(options, recorded, platform)?;
        return apply(platform, &root, &ownership, &choices, options.common.yes);
    }
    // The wizard starts from the options given, then the recorded choices, then the defaults.
    // 向导依次以给出的选项、记录的选择与默认值为起点。
    let browser = ComponentName::new(BROWSER_COMPONENT).map_err(|error| error.to_string())?;
    let draft = Draft {
        mode: options
            .mode
            .or(recorded.map(|state| state.mode))
            .unwrap_or_else(|| wizard::default_mode(platform)),
        browser: options.with.contains(&browser)
            || recorded.is_some_and(|state| state.with.contains(&browser)),
        root: root.display().to_string(),
        shortcut: !options.no_shortcut && recorded.is_none_or(|state| state.shortcut),
        path: !options.no_path && recorded.is_none_or(|state| state.path),
        channel: options
            .channel
            .or(recorded.map(|state| state.channel))
            .unwrap_or(Channel::Stable),
    };
    let Some(draft) = wizard::run(platform, draft)? else {
        return Err(String::from("cancelled"));
    };
    let root = resolve_root(Some(Path::new(draft.root.trim())), platform)?;
    let ownership = root::ownership(&root)?;
    let route = match &ownership {
        Ownership::Owned(state) => options.route.unwrap_or(state.route),
        _ => options.route.unwrap_or_default(),
    };
    let choices = Choices {
        mode: draft.mode,
        channel: draft.channel,
        route,
        with: if draft.browser {
            vec![browser]
        } else {
            Vec::new()
        },
        shortcut: draft.shortcut,
        path: draft.path,
    };
    apply(platform, &root, &ownership, &choices, false)
}

/// `vulcan-code-setup update`.
/// `vulcan-code-setup update`。
pub fn update(options: &CommonOptions) -> Result<(), String> {
    let platform = Platform::current().ok_or("this platform is not supported")?;
    check_system(platform)?;
    let root = resolve_root(options.prefix.as_deref(), platform)?;
    let ownership = root::ownership(&root)?;
    let Ownership::Owned(state) = &ownership else {
        return Err(format!(
            "{} was not installed by the setup program; run `vulcan-code-setup install`",
            root.display()
        ));
    };
    let choices = Choices::resolve(
        &InstallOptions {
            common: options.clone(),
            ..InstallOptions::default()
        },
        Some(state),
        platform,
    )?;
    apply(platform, &root, &ownership, &choices, options.yes)
}

/// The install root: `--prefix` or the platform default, absolute and usable in launchers.
/// 安装根目录：`--prefix` 或平台默认值，为绝对路径且可用于启动器。
pub fn resolve_root(prefix: Option<&Path>, platform: Platform) -> Result<PathBuf, String> {
    let root = match prefix {
        Some(prefix) => paths::absolute(prefix)?,
        None => paths::default_root(platform)?,
    };
    if !platform.is_windows() {
        let text = root.to_str().ok_or_else(|| {
            format!(
                "the installation directory must be valid UTF-8: {}",
                root.display()
            )
        })?;
        if !embeddable(text) {
            return Err(format!(
                "the installation directory must not contain quotes, dollar signs, backquotes or \
                 backslashes: {text}"
            ));
        }
    }
    Ok(root)
}

/// Whether `text` can be embedded in double quotes in launchers and menu entries.
/// `text` 能否嵌入启动器与菜单项的双引号中。
fn embeddable(text: &str) -> bool {
    !text.contains(['"', '$', '`', '\\', '\n'])
}

/// Refuses systems the release cannot run on before anything is downloaded.
/// 在下载任何内容之前拒绝发行版无法运行的系统。
fn check_system(platform: Platform) -> Result<(), String> {
    if platform.is_windows() || platform.is_macos() {
        return Ok(());
    }
    let output = std::process::Command::new("getconf")
        .arg("GNU_LIBC_VERSION")
        .output();
    let found = output
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| parse_glibc(&String::from_utf8_lossy(&output.stdout)));
    let (major, minor) = MIN_GLIBC;
    match found {
        Some(version) if version >= MIN_GLIBC => Ok(()),
        Some((found_major, found_minor)) => Err(format!(
            "Vulcan Code needs glibc {major}.{minor} or newer; this system has {found_major}.\
             {found_minor}"
        )),
        None => Err(format!(
            "Vulcan Code needs glibc {major}.{minor} or newer, and this system does not report \
             one (musl-based systems such as Alpine are not supported)"
        )),
    }
}

/// Parses the output of `getconf GNU_LIBC_VERSION`, such as `glibc 2.35`.
/// 解析 `getconf GNU_LIBC_VERSION` 的输出，例如 `glibc 2.35`。
fn parse_glibc(output: &str) -> Option<(u32, u32)> {
    let version = output.trim().strip_prefix("glibc ")?;
    let mut parts = version.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    Some((major, minor))
}

/// What an installation run does with the versions.
/// 一次安装运行对版本所做的事。
enum Work {
    /// Assemble and switch to a newer version.
    /// 组装并切换到更新的版本。
    Assemble {
        /// Its verified manifest.
        /// 其已校验的清单。
        manifest: Box<ReleaseManifest>,
        /// Indexes of the chosen components.
        /// 所选组件的索引。
        indexes: Vec<ComponentIndex>,
    },
    /// Keep the current version and refresh the integration.
    /// 保留当前版本并刷新集成。
    Refresh(ReleaseVersion),
}

/// Installs or refreshes `root` with `choices`.
/// 以 `choices` 安装或刷新 `root`。
fn apply(
    platform: Platform,
    root_path: &Path,
    ownership: &Ownership,
    choices: &Choices,
    yes: bool,
) -> Result<(), String> {
    let root = InstallRoot::new(root_path);
    let current = if root_path.exists() {
        root::current(&root)?
    } else {
        Current::None
    };
    let installed = match &current {
        Current::Version(version) => Some(version.clone()),
        _ => None,
    };
    let components = choices.components()?;
    let floor = match &installed {
        Some(version) => version.clone(),
        None => "0.0.0"
            .parse::<ReleaseVersion>()
            .map_err(|error| error.to_string())?,
    };
    let mut downloader =
        Downloader::new(choices.route.origins(), USER_AGENT).map_err(|error| error.to_string())?;
    println!("Checking the {} channel...", choices.channel.as_str());
    let offer = find_update(
        &mut downloader,
        BUILT_IN_KEYS,
        platform,
        choices.channels(),
        &floor,
    )
    .map_err(|error| error.to_string())?;
    let work = match (offer, &installed) {
        (Some(offer), _) => {
            if let Some(minimum) = &offer.manifest.min_setup_version {
                return Err(format!(
                    "{} needs setup program {minimum} or newer; download the newest setup program",
                    offer.manifest.version
                ));
            }
            let indexes = read_indexes(&mut downloader, &offer.manifest, &components)
                .map_err(|error| error.to_string())?;
            Work::Assemble {
                manifest: Box::new(offer.manifest),
                indexes,
            }
        }
        (None, Some(version)) => {
            let present = root::installed_components(&root, version)?;
            if present != components {
                return Err(format!(
                    "{version} is installed with {}; it cannot change to {} until a newer version \
                     is published, or after `vulcan-code-setup uninstall`",
                    join(&present),
                    join(&components)
                ));
            }
            Work::Refresh(version.clone())
        }
        (None, None) => {
            return Err(format!(
                "no release is published for {} on the {} channel",
                platform.as_str(),
                choices.channel.as_str()
            ));
        }
    };
    let version = match &work {
        Work::Assemble { manifest, .. } => manifest.version.clone(),
        Work::Refresh(version) => version.clone(),
    };
    let version_text = version.to_string();
    let plan = Plan {
        platform,
        root: root_path,
        version: &version_text,
        mode: choices.mode,
        shortcut: choices.shortcut,
        path: choices.path,
    };
    println!();
    match &work {
        Work::Assemble { indexes, .. } => {
            let size: u64 = indexes.iter().map(ComponentIndex::installed_size).sum();
            match &installed {
                Some(installed) => println!("Update Vulcan Code {installed} to {version}"),
                None => println!("Install Vulcan Code {version}"),
            }
            println!("  components: {}", join(&components));
            println!("  size: {}", megabytes(size));
        }
        Work::Refresh(_) => {
            println!("Vulcan Code {version} is up to date; refresh its integration")
        }
    }
    println!("  directory: {}", root_path.display());
    for line in plan.locations()? {
        println!("  {line}");
    }
    if !confirm("Continue?", yes)? {
        return Err(String::from("cancelled"));
    }

    std::fs::create_dir_all(root_path)
        .map_err(|error| format!("cannot create {}: {error}", root_path.display()))?;
    let lock = root.lock().map_err(|error| error.to_string())?;
    let switched = match work {
        Work::Assemble { manifest, indexes } => {
            let version_dir = root.version_dir(&version);
            if !version_dir.join(RECORD_FILE_NAME).is_file() {
                let prepared = Assembly::new(&root, &lock, &manifest, indexes)
                    .and_then(|assembly| assembly.prepare())
                    .map_err(|error| error.to_string())?;
                let total = prepared.download_bytes();
                let counter = downloader.received_counter();
                let start = counter.load(Ordering::Relaxed);
                let done = Arc::new(AtomicBool::new(false));
                let result = std::thread::scope(|scope| {
                    if std::io::stderr().is_terminal() {
                        let done = Arc::clone(&done);
                        let counter = Arc::clone(&counter);
                        scope.spawn(move || {
                            while !done.load(Ordering::Relaxed) {
                                let received = counter.load(Ordering::Relaxed) - start;
                                eprint!(
                                    "\r  downloading {} of {}   ",
                                    megabytes(received.min(total)),
                                    megabytes(total)
                                );
                                std::thread::sleep(Duration::from_millis(250));
                            }
                            eprintln!();
                        });
                    }
                    let result = prepared.complete(&mut downloader);
                    done.store(true, Ordering::Relaxed);
                    result
                });
                result.map_err(|error| error.to_string())?;
                if let Err(error) = check_release_build(&version_dir) {
                    let _ = std::fs::remove_dir_all(&version_dir);
                    return Err(error);
                }
            }
            root.switch_to(&lock, &version)
                .map_err(|error| error.to_string())?;
            true
        }
        Work::Refresh(_) => false,
    };
    if *ownership == Ownership::Legacy {
        let _ = std::fs::remove_file(root_path.join(LEGACY_MARKER));
    }

    let layout = Layout::read(&root.version_dir(&version))?;
    let previous_items: &[Item] = match ownership {
        Ownership::Owned(state) => &state.items,
        _ => &[],
    };
    let mut recorder = Recorder::new(previous_items);
    let integrated = integrate::integrate(&plan, &layout, switched, &mut recorder)
        .and_then(|()| copy_self(root_path));
    let mut state = SetupState {
        schema: STATE_SCHEMA,
        mode: choices.mode,
        channel: choices.channel,
        route: choices.route,
        with: choices.with.clone(),
        shortcut: choices.shortcut,
        path: choices.path,
        items: Vec::new(),
    };
    // Items of an earlier run that this run did not create are removed; when this run failed they
    // are kept so that uninstalling still finds them.
    // 更早一次运行创建、而本次未创建的条目会被移除；本次失败时保留它们，使卸载仍能找到。
    let mut leftovers = Vec::new();
    for item in previous_items {
        if !recorder.items.contains(item) {
            if integrated.is_ok() {
                leftovers.push(item.clone());
            } else {
                state.add(item.clone());
            }
        }
    }
    for item in &recorder.items {
        state.add(item.clone());
    }
    let mut problems = Vec::new();
    for item in leftovers.iter().rev() {
        if let Err(error) = integrate::remove(item, root_path, Some(&layout)) {
            problems.push(error);
            state.add(item.clone());
        }
    }
    state.write(root_path)?;
    integrated?;

    let removed = root
        .prune_versions(&lock, &version)
        .map_err(|error| error.to_string())?;
    let keep = match &current {
        Current::Legacy(name) if switched => Some(name.as_str()),
        _ => None,
    };
    root::remove_legacy_generations(&root, keep);
    drop(lock);

    println!();
    if switched {
        println!(
            "Installed Vulcan Code {version} into {}.",
            root_path.display()
        );
    } else {
        println!(
            "Vulcan Code {version} in {} was refreshed.",
            root_path.display()
        );
    }
    if !removed.is_empty() {
        println!(
            "Removed older versions: {}.",
            removed
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    if choices
        .with
        .iter()
        .any(|name| name.as_str() == BROWSER_COMPONENT)
        && !platform.is_windows()
        && !platform.is_macos()
    {
        println!(
            "The built-in browser needs system libraries once: vulcan-code-service \
             --install-browser-dependencies (uses sudo)."
        );
    }
    for note in recorder.notes.iter().chain(&problems) {
        println!("{note}");
    }
    Ok(())
}

/// Requires the assembled version to be a release build that loads here.
/// 要求组装好的版本是能在本机加载的发行构建。
fn check_release_build(version_dir: &Path) -> Result<(), String> {
    if Description::read(version_dir)?.profile != "release" {
        return Err(format!("{} is not a release build", version_dir.display()));
    }
    Ok(())
}

/// Copies the running setup program into `<root>/setup` unless it already runs from there.
/// 将正在运行的安装器复制到 `<root>/setup`，除非它已从那里运行。
fn copy_self(root: &Path) -> Result<(), String> {
    let running = std::env::current_exe()
        .map_err(|error| format!("cannot find the setup program: {error}"))?;
    let dir = root.join(SETUP_DIR);
    let target = dir.join(SETUP_PROGRAM);
    let same = match (running.canonicalize(), target.canonicalize()) {
        (Ok(running), Ok(target)) => running == target,
        _ => false,
    };
    if same {
        return Ok(());
    }
    let fail = |error: std::io::Error| format!("cannot write {}: {error}", target.display());
    std::fs::create_dir_all(&dir).map_err(fail)?;
    let mut partial = target.as_os_str().to_owned();
    partial.push(".partial");
    std::fs::copy(&running, &partial).map_err(fail)?;
    if target.exists() {
        // A running copy cannot be replaced on Windows, but it can be renamed.
        // Windows 上无法替换正在运行的副本，但可以改名。
        let mut aside = target.as_os_str().to_owned();
        aside.push(".old");
        let _ = std::fs::remove_file(&aside);
        let _ = std::fs::rename(&target, &aside);
    }
    std::fs::rename(&partial, &target).map_err(fail)
}

/// Component names separated by commas.
/// 以逗号分隔的组件名。
fn join(components: &BTreeSet<ComponentName>) -> String {
    components
        .iter()
        .map(ComponentName::as_str)
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn platform(name: &str) -> Platform {
        Platform::ALL
            .into_iter()
            .find(|platform| platform.as_str() == name)
            .expect("platform")
    }

    #[test]
    fn choices_follow_the_options_then_the_recorded_state() {
        let linux = platform("linux-x64");
        assert!(Choices::resolve(&InstallOptions::default(), None, linux).is_err());
        let options = InstallOptions {
            mode: Some(Mode::Service),
            no_shortcut: true,
            ..InstallOptions::default()
        };
        let choices = Choices::resolve(&options, None, linux).expect("choices");
        assert_eq!(choices.channel, Channel::Stable);
        assert!(!choices.shortcut);
        assert!(choices.path);
        let recorded = SetupState {
            schema: STATE_SCHEMA,
            mode: choices.mode,
            channel: Channel::Preview,
            route: Route::China,
            with: vec![ComponentName::new("browser").expect("name")],
            shortcut: choices.shortcut,
            path: choices.path,
            items: Vec::new(),
        };
        let again =
            Choices::resolve(&InstallOptions::default(), Some(&recorded), linux).expect("again");
        assert_eq!(again.mode, Mode::Service);
        assert_eq!(again.channel, Channel::Preview);
        assert_eq!(again.route, Route::China);
        assert!(!again.shortcut);
        let names: Vec<String> = again
            .components()
            .expect("components")
            .iter()
            .map(|name| name.as_str().to_owned())
            .collect();
        assert_eq!(names, ["browser", "core", "tui"]);
    }

    #[test]
    fn modes_are_refused_where_they_do_not_exist() {
        let mode = |mode| InstallOptions {
            mode: Some(mode),
            ..InstallOptions::default()
        };
        assert!(Choices::resolve(&mode(Mode::Desktop), None, platform("windows-x64")).is_err());
        assert!(Choices::resolve(&mode(Mode::Service), None, platform("macos-arm64")).is_err());
        let desktop =
            Choices::resolve(&mode(Mode::Desktop), None, platform("macos-arm64")).expect("desktop");
        assert!(
            desktop
                .components()
                .expect("components")
                .iter()
                .any(|name| name.as_str() == DESKTOP_COMPONENT)
        );
    }

    #[test]
    fn glibc_versions_are_read_from_getconf() {
        assert_eq!(parse_glibc("glibc 2.35\n"), Some((2, 35)));
        assert_eq!(parse_glibc("glibc 2.39"), Some((2, 39)));
        assert_eq!(parse_glibc("musl"), None);
        assert!((2, 31) < MIN_GLIBC && (2, 40) > MIN_GLIBC);
    }

    #[test]
    fn launcher_paths_refuse_shell_characters() {
        assert!(embeddable("/opt/vulcan code"));
        assert!(!embeddable("/opt/$HOME"));
        assert!(!embeddable("/opt/\"x"));
    }
}
