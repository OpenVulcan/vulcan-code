//! Native executable resources for the independent Core service.
//! 独立 Core 服务的原生可执行文件资源。

// Embed the executable icon and version metadata, and configure platform-specific linking.
// 嵌入可执行文件图标和版本元数据，并配置平台专属链接行为。
//
// A `--no-default-features` build keeps this script inert so Linux CI can link the
// headless service without enabling the desktop tray host or its graphics libraries.
// `--no-default-features` 构建保持本脚本惰性，使 Linux CI 无需启用桌面托盘宿主或其图形库
// 即可链接无头服务。
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // Inspect the target rather than the build host so cross-compilation selects the right resources.
    // 检查编译目标而非构建宿主，使交叉编译选择正确的资源。
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").expect("Cargo target OS");
    if target_os == "windows" {
        if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
            println!("cargo:rustc-link-arg=/NOIMPLIB");
            println!("cargo:rustc-link-arg=/NOEXP");
        }
        #[cfg(feature = "desktop-tray")]
        {
            println!("cargo:rerun-if-changed=icons/icon.ico");
            winresource::WindowsResource::new()
                .set_icon("icons/icon.ico")
                .set("ProductName", "Vulcan Code")
                .set("FileDescription", "Vulcan Code Service")
                .compile()
                .expect("compile Core executable icon and version resources");
        }
    }
}
