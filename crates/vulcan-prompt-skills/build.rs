//! Cargo change tracking for embedded system skill assets.
//! Cargo 对内嵌系统技能资源的变更跟踪。

/// Rebuilds the embedded system skills when their source tree changes.
/// 系统技能源目录发生变化时重新编译对应的内嵌资源。
fn main() {
    println!("cargo:rerun-if-changed=src/assets/system");
}
