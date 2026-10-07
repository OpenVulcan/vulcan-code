//! Platform-specific linking for the terminal client.
//! 终端客户端的平台专属链接配置。

// aws-lc-sys, unified into every workspace build by workspace-hack, declares its jitterentropy entry
// points with `dllexport`, so MSVC would emit an unused import library and export file for this
// executable and report it as a linker message. The client exports nothing, so both are suppressed.
// workspace-hack 将 aws-lc-sys 统一进全部工作区构建，其 jitterentropy 入口以 `dllexport` 声明，
// 因此 MSVC 会为本可执行文件生成无用的导入库与导出文件并作为链接器消息报告。
// 本客户端不导出任何符号，故两者均被抑制。
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // Inspect the target rather than the build host so cross-compilation selects the right linker flags.
    // 检查编译目标而非构建宿主，使交叉编译选择正确的链接参数。
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        println!("cargo:rustc-link-arg=/NOIMPLIB");
        println!("cargo:rustc-link-arg=/NOEXP");
    }
}
