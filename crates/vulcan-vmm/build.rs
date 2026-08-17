//! Build script for generated VMM gRPC bindings.
//! 用于生成 VMM gRPC 绑定的构建脚本。

/// Generates Rust bindings from the checked-in VMM proto file.
/// 从已随 crate 提交的 VMM proto 文件生成 Rust 绑定。
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    let mut prost_config = prost_build::Config::new();
    prost_config.protoc_executable(protoc);
    tonic_prost_build::configure()
        .build_server(false)
        .compile_with_config(prost_config, &["proto/v1/vmm.proto"], &["proto"])?;
    println!("cargo:rerun-if-changed=proto/v1/vmm.proto");
    Ok(())
}
