// Vendored from openai/codex (codex-rs/code-mode-protocol/build.rs) at c419fd01a70dd1d05356da443ac44328bda2e6f4.
// Modified for Vulcan Code; see NOTICE.md in this crate.
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // VULCAN MODIFICATION: the upstream `cfg(codex_bazel)` declaration is dropped with the Bazel module.
    println!("cargo:rerun-if-changed=src/grpc");

    let mut config = tonic_prost_build::Config::new();
    config.protoc_executable(protoc_bin_vendored::protoc_bin_path()?);
    let proto_files = glob::glob("src/grpc/*.proto")?.collect::<Result<Vec<_>, _>>()?;

    tonic_prost_build::configure()
        .build_client(/*enable*/ true)
        .build_server(/*enable*/ true)
        .compile_with_config(config, &proto_files, &[PathBuf::from("src/grpc")])?;

    Ok(())
}
