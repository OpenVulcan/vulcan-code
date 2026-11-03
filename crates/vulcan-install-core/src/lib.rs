//! Signed release formats, file packs and their verification for Vulcan Code installs and updates.
//! Vulcan Code 安装与更新所用的已签名发布格式、文件包及其校验。
//!
//! A first install is an update from an empty state, so the setup program, the in-app updater and
//! the release tooling share this one implementation instead of keeping a cross-repository contract
//! aligned. The trust chain is: the channel pointer is signed and names the release manifest digest,
//! the signed manifest names each component index digest, and each index names the digest and pack
//! location of every file. Packs, indexes and notes therefore need no signature of their own, and no
//! mirror has to be trusted.
//!
//! 首次安装就是一次从空状态开始的更新，因此安装器、应用内更新与发布工具共用这一份实现，而不是维护一份
//! 需要跨仓库对齐的契约。信任链为：通道指针已签名并写明版本清单摘要，已签名的版本清单写明每个组件索引的
//! 摘要，每个索引写明每个文件的摘要与所在文件包位置。因此文件包、索引与更新说明都不需要单独签名，任何
//! 镜像都无需被信任。

#![warn(missing_docs)]

pub mod digest;
pub mod error;
pub mod index;
pub mod layout;
pub mod limits;
pub mod manifest;
pub mod names;
pub mod pack;
pub mod path;
pub mod pointer;
pub mod signing;

pub use digest::Sha256Digest;
pub use error::{FormatError, FormatResult};
