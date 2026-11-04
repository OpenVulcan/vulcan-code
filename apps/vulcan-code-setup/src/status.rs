//! `vulcan-code-setup status`: what is installed where.
//! `vulcan-code-setup status`：安装了什么、装在哪里。

use vulcan_install_core::install_root::InstallRoot;
use vulcan_install_core::names::Platform;

use crate::args::CommonOptions;
use crate::install::resolve_root;
use crate::root::{self, Current, Ownership};
use crate::state::Item;

/// Prints the installation in `--prefix` or the default root.
/// 打印 `--prefix` 或默认根目录中的安装。
pub fn status(options: &CommonOptions) -> Result<(), String> {
    let platform = Platform::current().ok_or("this platform is not supported")?;
    let root_path = resolve_root(options.prefix.as_deref(), platform)?;
    println!("directory: {}", root_path.display());
    let state = match root::ownership(&root_path)? {
        Ownership::Fresh => {
            println!("Vulcan Code is not installed here.");
            return Ok(());
        }
        Ownership::Legacy => {
            println!("installed by: the archive installer (install.sh); `install` takes it over");
            None
        }
        Ownership::Inno(_) => {
            println!(
                "installed by: the Windows installer (VulcanCode-Setup.exe); the app keeps it up to \
                 date, and `uninstall` runs its uninstaller"
            );
            None
        }
        Ownership::Owned(state) => Some(state),
    };
    let root = InstallRoot::new(&root_path);
    match root::current(&root)? {
        Current::Version(version) => {
            let components = root::installed_components(&root, &version)?;
            println!(
                "current version: {version} ({})",
                components
                    .iter()
                    .map(|name| name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        Current::Legacy(name) => println!("current version: {name}"),
        Current::None => println!("current version: none"),
    }
    let versions = root
        .installed_versions()
        .map_err(|error| error.to_string())?;
    println!(
        "versions on disk: {}",
        versions
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    );
    let Some(state) = state else {
        return Ok(());
    };
    println!("mode: {}", state.mode);
    println!("channel: {}", state.channel.as_str());
    if !state.with.is_empty() {
        println!(
            "optional components: {}",
            state
                .with
                .iter()
                .map(|name| name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    for item in &state.items {
        let line = match item {
            Item::File { path } | Item::Bundle { path } => path.display().to_string(),
            Item::Directory { .. } => continue,
            Item::UserPath { dir } => format!("user PATH entry {}", dir.display()),
            Item::UninstallEntry { key } => format!("uninstall entry {key}"),
            Item::UserService => String::from("systemd user unit vulcan-code.service"),
        };
        println!("  {line}");
    }
    Ok(())
}
