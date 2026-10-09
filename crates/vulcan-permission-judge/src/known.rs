//! Official judge packages this build recognizes, without any download location.
//! 本构建识别的官方判定模型包，不含任何下载位置。
//!
//! A package is official when the checkpoint it was converted from is listed here; any other
//! verified package can still be registered, but only as a custom model. The list names packages
//! for onboarding and settings, and is the single source for the official or custom mark.
//! 模型包的转换来源检查点列在此处时即为官方模型包；其他通过验证的模型包仍可登记，但只能作为自定义
//! 模型。该清单为引导与设置页提供模型包名称，并是官方或自定义标记的唯一来源。

use std::sync::LazyLock;

use serde::Deserialize;

/// Schema version of `known-packages.json` this build reads.
/// 本构建读取的 `known-packages.json` Schema 版本。
const KNOWN_PACKAGES_SCHEMA_VERSION: u32 = 1;

/// One official package family.
/// 一个官方模型包家族。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnownPackage {
    /// Stable package identifier, also the key of its localized description.
    /// 稳定的模型包标识，同时是其本地化说明的键。
    pub id: String,
    /// Product name shown as is in every language.
    /// 在所有语言中原样显示的产品名称。
    pub display_name: String,
    /// Digests of the trained checkpoints released under this package.
    /// 以该模型包发布的训练检查点摘要。
    pub checkpoints: Vec<String>,
}

/// Shape of `known-packages.json`.
/// `known-packages.json` 的形状。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KnownPackagesFile {
    schema_version: u32,
    packages: Vec<KnownPackage>,
}

/// Parsed and checked list, built once from the embedded resource.
/// 从内嵌资源构建一次的已解析并已检查清单。
static KNOWN_PACKAGES: LazyLock<Vec<KnownPackage>> = LazyLock::new(|| {
    parse_known_packages(include_str!("../resources/known-packages.json"))
        .expect("embedded known-packages.json must be valid")
});

/// Parses the list and refuses duplicate identifiers or checkpoints and malformed digests.
/// 解析清单，并拒绝重复的标识或检查点以及格式错误的摘要。
fn parse_known_packages(text: &str) -> Result<Vec<KnownPackage>, String> {
    let file: KnownPackagesFile = serde_json::from_str(text).map_err(|error| error.to_string())?;
    if file.schema_version != KNOWN_PACKAGES_SCHEMA_VERSION {
        return Err(format!(
            "known packages schema {} is not {KNOWN_PACKAGES_SCHEMA_VERSION}",
            file.schema_version
        ));
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut checkpoints = std::collections::BTreeSet::new();
    for package in &file.packages {
        if package.id.is_empty() || !ids.insert(package.id.as_str()) {
            return Err(format!("package id `{}` is empty or repeated", package.id));
        }
        if package.display_name.trim().is_empty() || package.checkpoints.is_empty() {
            return Err(format!(
                "package `{}` lacks a name or checkpoints",
                package.id
            ));
        }
        for checkpoint in &package.checkpoints {
            let valid = checkpoint.len() == 64
                && checkpoint
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
            if !valid || !checkpoints.insert(checkpoint.as_str()) {
                return Err(format!(
                    "checkpoint `{checkpoint}` is not a unique lowercase SHA-256"
                ));
            }
        }
    }
    Ok(file.packages)
}

/// Every official package, in display order.
/// 所有官方模型包，按显示顺序排列。
pub fn known_packages() -> &'static [KnownPackage] {
    &KNOWN_PACKAGES
}

/// The official package a checkpoint belongs to, or `None` for a custom model.
/// 检查点所属的官方模型包；自定义模型返回 `None`。
pub fn find_known_package(parent_sha256: &str) -> Option<&'static KnownPackage> {
    known_packages().iter().find(|package| {
        package
            .checkpoints
            .iter()
            .any(|checkpoint| checkpoint == parent_sha256)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_list_parses_and_identifies_its_checkpoints() {
        assert!(!known_packages().is_empty());
        for package in known_packages() {
            for checkpoint in &package.checkpoints {
                assert_eq!(find_known_package(checkpoint), Some(package));
            }
        }
        assert_eq!(find_known_package(&"0".repeat(64)), None);
    }

    #[test]
    fn list_rejects_repeated_checkpoints_and_unknown_fields() {
        let digest = "a".repeat(64);
        let repeated = format!(
            r#"{{"schema_version":1,"packages":[
                {{"id":"a","display_name":"A","checkpoints":["{digest}"]}},
                {{"id":"b","display_name":"B","checkpoints":["{digest}"]}}]}}"#
        );
        assert!(parse_known_packages(&repeated).is_err());
        let extra = format!(
            r#"{{"schema_version":1,"packages":[
                {{"id":"a","display_name":"A","checkpoints":["{digest}"],"url":"x"}}]}}"#
        );
        assert!(parse_known_packages(&extra).is_err());
    }
}
