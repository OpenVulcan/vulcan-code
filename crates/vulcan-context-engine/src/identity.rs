//! Stable content-addressed identities for workspace artifacts.
//! 为工作区产物提供稳定的内容寻址身份。

use sha2::{Digest, Sha256};
use std::error::Error;
use std::fmt::{Display, Formatter};

/// Errors raised while canonicalizing a workspace-relative path.
/// 规范化工作区相对路径时产生的错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathError {
    /// The path is exactly empty or normalizes to no components.
    /// 路径为空字符串或规范化后不包含任何组成部分。
    Empty,
    /// The path is absolute and cannot identify a workspace-relative file.
    /// 路径是绝对路径，不能作为工作区相对文件身份。
    Absolute,
    /// The path attempts to escape the workspace with a parent component.
    /// 路径试图通过父目录组件逃逸工作区。
    ParentTraversal,
}

impl Display for PathError {
    /// Formats the path error for diagnostics.
    /// 将路径错误格式化为诊断文本。
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => formatter.write_str("workspace-relative path is empty"),
            Self::Absolute => formatter.write_str("workspace-relative path must not be absolute"),
            Self::ParentTraversal => {
                formatter.write_str("workspace-relative path must not contain parent traversal")
            }
        }
    }
}

impl Error for PathError {}

/// Normalize a workspace-relative path without changing its case.
/// 在不改变大小写的前提下规范化工作区相对路径。
pub fn normalize_relative_path(path: &str) -> Result<String, PathError> {
    if path.is_empty() {
        return Err(PathError::Empty);
    }

    let slash_path = path.replace('\\', "/");
    if slash_path.starts_with('/') || has_windows_drive_prefix(&slash_path) {
        return Err(PathError::Absolute);
    }

    let mut components = Vec::new();
    for component in slash_path.split('/') {
        match component {
            "" | "." => {}
            ".." => return Err(PathError::ParentTraversal),
            value => components.push(value),
        }
    }

    if components.is_empty() {
        return Err(PathError::Empty);
    }

    Ok(components.join("/"))
}

/// Compute the canonical Blob identity from an unambiguously framed path and content.
/// 根据具有明确边界的路径与内容计算规范 Blob 身份。
pub fn blob_name(relative_path: &str, content: &str) -> Result<String, PathError> {
    let normalized_path = normalize_relative_path(relative_path)?;
    // A domain tag and decimal byte length make the path/content boundary collision-free.
    // 域标记和十进制字节长度使路径与内容之间的边界不会产生拼接碰撞。
    let path_length = normalized_path.len().to_string();
    Ok(sha256_hex_parts([
        b"vce-blob-v1\0".as_slice(),
        path_length.as_bytes(),
        b"\0".as_slice(),
        normalized_path.as_bytes(),
        content.as_bytes(),
    ]))
}

/// Compute the content identity of a Chunk.
/// 计算代码块内容身份。
pub fn content_hash(content: &str) -> String {
    sha256_hex_parts([content.as_bytes()])
}

/// Return whether a value is one canonical lowercase SHA-256 identity.
/// 返回一个值是否为规范的小写 SHA-256 身份。
pub fn is_sha256_identity(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Return whether a value is one canonical type-tagged lowercase SHA-256 identity.
/// 返回一个值是否为带规范类型前缀的小写 SHA-256 身份。
///
/// `value` must contain the exact `sha256:` tag followed by one raw digest. Returns false for
/// untagged, uppercase, shortened, or extended values.
/// `value` 必须包含精确的 `sha256:` 前缀及一个原始摘要。无前缀、大写、缩短或扩展值均返回 false。
pub fn is_tagged_sha256_identity(value: &str) -> bool {
    value
        .strip_prefix("sha256:")
        .is_some_and(is_sha256_identity)
}

/// Compute a stable occurrence identity for a Chunk inside a Blob.
/// 计算代码块在 Blob 内出现位置的稳定身份。
pub fn chunk_id(
    blob_name: &str,
    chunk_content_hash: &str,
    start_line: usize,
    end_line: usize,
) -> String {
    chunk_id_with_offset(blob_name, chunk_content_hash, start_line, end_line, 0)
}

/// Compute a stable Chunk occurrence identity with an optional first-line character offset.
/// 使用可选的首行字符偏移计算稳定的代码块出现位置身份。
pub fn chunk_id_with_offset(
    blob_name: &str,
    chunk_content_hash: &str,
    start_line: usize,
    end_line: usize,
    start_char_offset: usize,
) -> String {
    let line_range = format!("{start_line}:{end_line}");
    if start_char_offset == 0 {
        sha256_hex_parts([
            blob_name.as_bytes(),
            b"\n",
            chunk_content_hash.as_bytes(),
            b"\n",
            line_range.as_bytes(),
        ])
    } else {
        let offset = start_char_offset.to_string();
        sha256_hex_parts([
            blob_name.as_bytes(),
            b"\n",
            chunk_content_hash.as_bytes(),
            b"\n",
            line_range.as_bytes(),
            b"\n",
            offset.as_bytes(),
        ])
    }
}

/// Compute a lowercase SHA-256 digest over multiple byte slices without an intermediate buffer.
/// 对多个字节片段计算小写 SHA-256 摘要，避免创建中间拼接缓冲区。
pub fn sha256_hex_parts<'a, I>(parts: I) -> String
where
    I: IntoIterator<Item = &'a [u8]>,
{
    let mut digest = Sha256::new();
    for part in parts {
        digest.update(part);
    }
    let output = digest.finalize();
    let mut encoded = String::with_capacity(output.len() * 2);
    for byte in output {
        encoded.push(HEX[(byte >> 4) as usize]);
        encoded.push(HEX[(byte & 0x0f) as usize]);
    }
    encoded
}

/// Detect a drive-letter prefix before path normalization can treat it as relative.
/// 在路径规范化前识别盘符前缀，避免把盘符路径误认为相对路径。
fn has_windows_drive_prefix(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

const HEX: &[char; 16] = &[
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c', 'd', 'e', 'f',
];

#[cfg(test)]
// Test fixtures intentionally use panic-style extraction to keep invariant failures local.
// 测试夹具有意使用恐慌式提取，使不变量失败保持在本地。
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// Verify canonical path separators and traversal rejection.
    /// 验证路径分隔符规范化以及父目录逃逸拒绝。
    #[test]
    fn normalizes_relative_paths() {
        assert_eq!(
            normalize_relative_path("src\\lib.rs").unwrap(),
            "src/lib.rs"
        );
        assert_eq!(
            normalize_relative_path("./src/./lib.rs").unwrap(),
            "src/lib.rs"
        );
        assert_eq!(
            normalize_relative_path("../lib.rs"),
            Err(PathError::ParentTraversal)
        );
        assert_eq!(
            normalize_relative_path("D:/repo/lib.rs"),
            Err(PathError::Absolute)
        );
        assert_eq!(
            normalize_relative_path(" src/lib.rs ").unwrap(),
            " src/lib.rs "
        );
    }

    /// Verify that both path and content participate in the Blob identity.
    /// 验证路径和内容都会参与 Blob 身份计算。
    #[test]
    fn blob_identity_changes_with_path_or_content() {
        let first = blob_name("src/lib.rs", "fn main() {}").unwrap();
        assert_ne!(first, blob_name("src/main.rs", "fn main() {}").unwrap());
        assert_ne!(first, blob_name("src/lib.rs", "fn main() {  }").unwrap());
        assert_eq!(first, blob_name("src\\lib.rs", "fn main() {}").unwrap());
    }

    /// Verify a path/content boundary cannot collide with another concatenation boundary.
    /// 验证路径与内容边界不会和另一种拼接边界发生碰撞。
    #[test]
    fn blob_identity_frames_path_and_content_unambiguously() {
        assert_ne!(blob_name("ab", "c").unwrap(), blob_name("a", "bc").unwrap());
    }

    /// Verify that a Chunk occurrence ID changes when its source range changes.
    /// 验证代码块行号范围变化会改变出现位置身份。
    #[test]
    fn chunk_identity_includes_range() {
        let hash = content_hash("fn main() {}");
        assert_ne!(chunk_id("blob", &hash, 1, 1), chunk_id("blob", &hash, 2, 2));
    }

    /// Verify the compatibility identity is unchanged at column zero and distinct after it.
    /// 验证零列兼容身份保持不变，非零列身份保持可区分。
    #[test]
    fn chunk_identity_extends_only_for_nonzero_offset() {
        let hash = content_hash("same");
        assert_eq!(
            chunk_id("blob", &hash, 1, 1),
            chunk_id_with_offset("blob", &hash, 1, 1, 0)
        );
        assert_ne!(
            chunk_id_with_offset("blob", &hash, 1, 1, 0),
            chunk_id_with_offset("blob", &hash, 1, 1, 4)
        );
    }

    /// Verify that persisted identities accept only canonical lowercase SHA-256 text.
    /// 验证持久化身份仅接受规范的小写 SHA-256 文本。
    #[test]
    fn validates_canonical_sha256_identity_text() {
        let identity = content_hash("source");
        assert!(is_sha256_identity(&identity));
        assert!(!is_sha256_identity(&identity.to_uppercase()));
        assert!(!is_sha256_identity("short"));
    }

    /// Accept only the exact tagged representation used by physical contracts.
    /// 仅接受物理契约使用的精确带前缀表示。
    #[test]
    fn validates_canonical_tagged_sha256_identity_text() {
        let identity = content_hash("physical-contract");
        assert!(is_tagged_sha256_identity(&format!("sha256:{identity}")));
        assert!(!is_tagged_sha256_identity(&identity));
        assert!(!is_tagged_sha256_identity(&format!(
            "SHA256:{}",
            identity.to_uppercase()
        )));
        assert!(!is_tagged_sha256_identity(&format!(
            "sha256:{identity}:extra"
        )));
    }
}
