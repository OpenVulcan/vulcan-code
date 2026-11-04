//! Object keys below the `vulcancode/` prefix of every download origin (plan section 7).
//! 每个下载源 `vulcancode/` 前缀下的对象键（方案第七节）。
//!
//! openvulcan-web builds the same keys in `server/internal/staticsync/format.go`; change both together.
//! openvulcan-web 在 `server/internal/staticsync/format.go` 中构造相同的键；两处须同时修改。

use crate::digest::Sha256Digest;
use crate::names::{Channel, Platform, ReleaseVersion};

/// The primary download origin; its path is the object key prefix in the bucket that serves it.
/// 主下载源；其路径即提供它的存储桶中的对象键前缀。
pub const PRIMARY_ORIGIN: &str = "https://static.openvulcan.com/vulcancode";

/// The mainland China mirror that openvulcan-web fills from [`PRIMARY_ORIGIN`]; objects arrive there
/// first and pointers last, and nothing it serves is trusted beyond the signatures and digests.
/// openvulcan-web 从 [`PRIMARY_ORIGIN`] 同步的中国大陆镜像；对象先到、指针最后到，其提供的内容除签名与
/// 摘要外不受信任。
pub const CHINA_ORIGIN: &str = "https://static.openvulcan.cn/vulcancode";

/// The mutable signed pointer of a channel and platform.
/// 某通道与平台的可变已签名指针。
pub fn pointer_key(channel: Channel, platform: Platform) -> String {
    format!("channels/{channel}/{platform}.json")
}

/// The signed release manifest of a version and platform.
/// 某版本与平台的已签名版本清单。
pub fn manifest_key(version: &ReleaseVersion, platform: Platform) -> String {
    format!("releases/{version}/{platform}/manifest.json")
}

/// The release notes of a version.
/// 某版本的更新说明。
pub fn notes_key(version: &ReleaseVersion) -> String {
    format!("notes/{version}.json")
}

/// A component index, named by the digest of its compressed bytes.
/// 组件索引，以其压缩后字节的摘要命名。
pub fn index_key(digest: Sha256Digest) -> String {
    format!("indexes/{digest}.json.zst")
}

/// A file pack, named by the digest of its bytes.
/// 文件包，以其字节的摘要命名。
pub fn pack_key(digest: Sha256Digest) -> String {
    format!("packs/{digest}.pack")
}

/// A full installer or install script of a version.
/// 某版本的完整安装包或安装脚本。
pub fn installer_key(version: &ReleaseVersion, file: &str) -> String {
    format!("installers/{version}/{file}")
}

/// The detached signature next to a signed document.
/// 已签名文档旁的分离签名。
pub fn signature_key(document_key: &str) -> String {
    format!("{document_key}.sig")
}
