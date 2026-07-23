//! Tauri build script for the Vulcan Code desktop shell.
//! Vulcan Code 桌面壳的 Tauri 构建脚本。

// Build the Tauri application resources and configure platform-specific linker behavior.
// 构建 Tauri 应用资源并配置平台专属的链接器行为。
fn main() {
    // LuaJIT exposes compatibility symbols from its static archive, but the desktop executable is not a linkable SDK.
    // LuaJIT 静态库会导出兼容符号，但桌面可执行文件并不是供其他程序链接的 SDK。
    #[cfg(all(target_os = "windows", target_env = "msvc"))]
    {
        println!("cargo:rustc-link-arg=/NOIMPLIB");
        println!("cargo:rustc-link-arg=/NOEXP");
    }

    tauri_build::build();
}
