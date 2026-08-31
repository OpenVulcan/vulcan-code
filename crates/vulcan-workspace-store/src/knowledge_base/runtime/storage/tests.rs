//! Storage identity and atomic publication regressions.
//! 存储身份与原子发布回归。

use super::*;
use tempfile::tempdir;

/// Verifies library IDs reject non-canonical and path-like input.
/// 验证知识库标识符拒绝非规范格式与路径式输入。
#[test]
fn library_id_requires_canonical_uuid() {
    assert!(validate_library_id("../../escape").is_err());
    assert!(validate_library_id("550E8400-E29B-41D4-A716-446655440000").is_err());
    assert!(validate_library_id("550e8400-e29b-41d4-a716-446655440000").is_ok());
}

/// Verifies atomic replacement overwrites an existing manifest on every supported platform.
/// 验证原子替换会在所有受支持平台覆盖既有清单。
#[test]
fn atomic_file_replacement_overwrites_existing_destination() {
    let temporary = tempdir().expect("create atomic replacement root");
    let destination = temporary.path().join("manifest.json");
    fs::write(&destination, "old manifest").expect("write existing destination");
    let mut source =
        tempfile::NamedTempFile::new_in(temporary.path()).expect("create replacement source");
    source
        .write_all(b"new manifest")
        .expect("write replacement source");

    persist_tempfile_atomically(source, &destination).expect("replace existing destination");

    assert_eq!(
        fs::read_to_string(&destination).expect("read replaced destination"),
        "new manifest"
    );
}
