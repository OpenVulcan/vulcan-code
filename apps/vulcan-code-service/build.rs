//! Tauri build script for the Vulcan Code service tray host.
//! Vulcan Code 服务托盘宿主的 Tauri 构建脚本。

// Build the Tauri application resources and configure platform-specific linker behavior.
// 构建 Tauri 应用资源并配置平台专属的链接器行为。
//
// A `--no-default-features` build keeps this script inert so Linux CI can link the
// headless service without enabling the desktop tray host or its graphics libraries.
// `--no-default-features` 构建保持本脚本惰性，使 Linux CI 无需启用桌面托盘宿主或其图形库
// 即可链接无头服务。
fn main() {
    // LuaJIT exposes compatibility symbols from its static archive, but the desktop executable is not a linkable SDK.
    // LuaJIT 静态库会导出兼容符号，但桌面可执行文件并不是供其他程序链接的 SDK。
    #[cfg(all(target_os = "windows", target_env = "msvc"))]
    {
        println!("cargo:rustc-link-arg=/NOIMPLIB");
        println!("cargo:rustc-link-arg=/NOEXP");
    }

    #[cfg(not(feature = "desktop-tray"))]
    {
        println!("cargo:rerun-if-changed=build.rs");
    }
    #[cfg(feature = "desktop-tray")]
    tauri_build::build();
}
