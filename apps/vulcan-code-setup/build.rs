//! Compiles the product version into the setup program.
//! 将产品版本编译进安装器。
//!
//! The product binaries read `product.json` at run time so their bytes stay the same across versions.
//! The setup program is downloaded on its own, has no resources beside it, and must know its version to
//! update itself and to honour `minSetupVersion`; it is never reused across releases.
//! 产品二进制在运行时读取 `product.json`，使其字节跨版本保持不变。安装器单独下载，旁边没有资源，并且必须知道
//! 自身版本才能自更新并遵守 `minSetupVersion`；它从不跨版本复用。

use std::path::Path;

fn main() {
    let product = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/vulcan-runtime-assets/resources/product.json");
    println!("cargo:rerun-if-changed={}", product.display());
    let text = std::fs::read_to_string(&product)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", product.display()));
    let document: serde_json::Value = serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("cannot parse {}: {error}", product.display()));
    let Some(version) = document.get("version").and_then(serde_json::Value::as_str) else {
        panic!("{} has no version", product.display());
    };
    println!("cargo:rustc-env=VULCAN_SETUP_VERSION={version}");
}
