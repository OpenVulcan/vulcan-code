//! Native executable resources for the Windows launcher and PATH forwarder.
//! Windows 启动器与 PATH 转发器的原生可执行文件资源。

// Shortcuts point at the launcher, so it carries the product icon that Explorer and the taskbar show.
// 快捷方式指向启动器，因此它携带资源管理器与任务栏显示的产品图标。
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // Inspect the target rather than the build host so cross-compilation selects the right resources.
    // 检查编译目标而非构建宿主，使交叉编译选择正确的资源。
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").expect("Cargo target OS");
    if target_os == "windows" {
        println!("cargo:rerun-if-changed=../vulcan-code-service/icons/icon.ico");
        winresource::WindowsResource::new()
            .set_icon("../vulcan-code-service/icons/icon.ico")
            .set("ProductName", "Vulcan Code")
            .set("FileDescription", "Vulcan Code")
            .compile()
            .expect("compile launcher icon and version resources");
    }
}
