//! Propagates Cargo's exact compilation target into the Vulcan Plugin runtime.
//! 将 Cargo 的精确编译目标传播到 Vulcan 插件运行时。

use std::env;

/// Publishes Cargo's exact compilation target for deterministic manifest platform checks.
/// 发布 Cargo 的精确编译目标，用于确定性的插件清单平台校验。
///
/// # Returns
/// 返回值
/// This build entry returns no value and fails when Cargo does not provide `TARGET`.
/// 该构建入口不返回值，并在 Cargo 未提供 `TARGET` 时失败。
fn main() {
    // target is Cargo's normalized target triple for the current crate build.
    // target 是 Cargo 为当前 crate 构建提供的规范化目标三元组。
    let target = env::var("TARGET").expect("Cargo must provide TARGET to vulcan-plugin-runtime");
    println!("cargo:rustc-env=VULCAN_PLUGIN_RUNTIME_TARGET={target}");
}
