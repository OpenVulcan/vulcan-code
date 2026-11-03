// Format and behavior checks for the product identity and release index readers.
// 产品身份与发布索引读取的格式与行为检查。

use super::*;

/// Writes `text` to a fresh file and returns its directory guard and path.
/// 将 `text` 写入新文件，并返回其目录守卫与路径。
fn fixture(name: &str, text: &str) -> (PathBuf, PathBuf) {
    let directory =
        std::env::temp_dir().join(format!("vulcan-product-info-{}-{name}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join(name);
    fs::write(&path, text).unwrap();
    (directory, path)
}

/// The repository's product identity and release index parse with the shipped readers.
/// 仓库的产品身份与发布索引能被发行读取器解析。
#[test]
fn shipped_resources_parse() {
    let product = product().unwrap();
    assert!(parse_version(&product.version).is_some());
    assert!(!releases().unwrap().is_empty());
    assert!(version_at_least("0.0.0").unwrap());
}

/// A product identity must carry an exact version and no unknown fields.
/// 产品身份必须携带精确版本，且不得有未知字段。
#[test]
fn product_identity_rejects_inexact_versions_and_unknown_fields() {
    let (directory, path) = fixture("product.json", r#"{"version":"1.2.3","installEpoch":2}"#);
    assert_eq!(
        load_product(&path).unwrap(),
        ProductInfo {
            version: String::from("1.2.3"),
            install_epoch: 2
        }
    );
    for text in [
        r#"{"version":"1.2","installEpoch":1}"#,
        r#"{"version":"1.2.3-beta","installEpoch":1}"#,
        r#"{"version":"1.2.3","installEpoch":1,"extra":true}"#,
        r#"{"version":"1.2.3"}"#,
    ] {
        fs::write(&path, text).unwrap();
        assert!(load_product(&path).is_err(), "{text}");
    }
    fs::remove_dir_all(directory).unwrap();
}

/// An empty release index is rejected; dates may be null.
/// 空发布索引被拒绝；日期可以为 null。
#[test]
fn release_index_requires_a_release() {
    let (directory, path) = fixture(
        "releases.json",
        r#"{"releases":[{"version":"0.2.0","date":null},{"version":"0.1.0","date":"2026-11-01"}]}"#,
    );
    assert_eq!(load_releases(&path).unwrap().len(), 2);
    fs::write(&path, r#"{"releases":[]}"#).unwrap();
    assert!(load_releases(&path).is_err());
    fs::remove_dir_all(directory).unwrap();
}

/// Release notes are addressed only by an exact version and a plain language code.
/// 更新说明只能通过精确版本与普通语言代码定位。
#[test]
fn release_notes_reject_paths_outside_the_changelog() {
    for (version, code) in [
        ("../0.1.0", "en-US"),
        ("0.1.0", "../product"),
        ("0.1", "en-US"),
    ] {
        assert!(release_notes(version, code).is_err(), "{version} {code}");
    }
}

/// Versions compare numerically per component.
/// 版本按分量进行数值比较。
#[test]
fn versions_compare_numerically() {
    assert!(parse_version("0.10.0") > parse_version("0.9.9"));
    assert_eq!(parse_version("01.0.0"), Some((1, 0, 0)));
    assert_eq!(parse_version("1.0.0.0"), None);
    assert_eq!(parse_version("1..0"), None);
}
