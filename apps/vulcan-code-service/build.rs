//! Native executable resources for the independent Core service.
//! 独立 Core 服务的原生可执行文件资源。

// Embed the executable icon and version metadata; MSVC link arguments live in `.cargo/config.toml`.
// 嵌入可执行文件图标和版本元数据；MSVC 链接参数位于 `.cargo/config.toml`。
//
// The macOS Core also exports the Lua ABI used by the bundled native modules.
// macOS Core 还会导出随包原生模块使用的 Lua ABI。
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // Inspect the target rather than the build host so cross-compilation selects the right resources.
    // 检查编译目标而非构建宿主，使交叉编译选择正确的资源。
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").expect("Cargo target OS");
    if target_os == "macos" {
        // Bundled Lua modules resolve these symbols from Core's static LuaJIT in the flat namespace.
        // 随包 Lua 模块在扁平命名空间中从 Core 静态链接的 LuaJIT 解析这些符号。
        // Root every native import and preserve exports through release LTO and dead stripping.
        // 将每个原生导入保留为链接根，并在发布版 LTO 与死代码剔除期间保留导出。
        println!("cargo:rustc-link-arg-bin=vulcan-code-service=-Wl,-export_dynamic");
        // The distribution check compares this allowlist with the actual module imports.
        // 发行校验将此清单与实际模块导入进行比对。
        for symbol in include_str!("macos-lua-native-symbols.txt").lines() {
            println!("cargo:rustc-link-arg-bin=vulcan-code-service=-Wl,-u,{symbol}");
        }
        println!("cargo:rerun-if-changed=macos-lua-native-symbols.txt");
    }
    if target_os == "windows" {
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
