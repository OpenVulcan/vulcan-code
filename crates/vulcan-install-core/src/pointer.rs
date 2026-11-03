//! The signed channel pointer `channels/<channel>/<platform>.json` (plan section 7, fixed 2026-11-03).
//! 已签名的通道指针 `channels/<通道>/<平台>.json`（方案第七节，2026-11-03 定稿）。
//!
//! The pointer and its `.sig` cannot be replaced atomically, so a reader whose verification fails
//! fetches both once more before reporting the failure.
//! 指针与其 `.sig` 无法原子替换，因此验签失败的读取方应重新获取两者一次，再报告失败。

use serde::{Deserialize, Serialize};

use crate::digest::Sha256Digest;
use crate::error::{FormatError, FormatResult};
use crate::limits::{MAX_MANIFEST_BYTES, MAX_POINTER_BYTES};
use crate::names::{Channel, PRODUCT, Platform, ReleaseVersion, SCHEMA_VERSION};
use crate::signing::{TrustedKey, expect_key_id, parse_signed};

/// The digest and size of a document another document points to.
/// 另一文档所指向文档的摘要与大小。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentRef {
    /// SHA-256 of the raw document bytes.
    /// 文档原始字节的 SHA-256。
    pub sha256: Sha256Digest,
    /// Size of the raw document bytes.
    /// 文档原始字节的大小。
    pub size: u64,
}

impl DocumentRef {
    /// Describes `bytes`.
    /// 描述 `bytes`。
    pub fn of(bytes: &[u8]) -> Self {
        Self {
            sha256: Sha256Digest::of(bytes),
            size: bytes.len() as u64,
        }
    }

    /// Fails unless `bytes` are exactly the referenced document.
    /// 除非 `bytes` 正是所引用的文档，否则失败。
    pub fn expect_matches(&self, what: &'static str, bytes: &[u8]) -> FormatResult<()> {
        let actual = bytes.len() as u64;
        if actual != self.size {
            return Err(FormatError::SizeMismatch {
                what,
                expected: self.size,
                actual,
            });
        }
        self.sha256.expect_eq(what, Sha256Digest::of(bytes))
    }
}

/// The newest release of one channel and platform.
/// 某通道与平台的最新发布。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelPointer {
    /// Format version; only [`SCHEMA_VERSION`] is accepted.
    /// 格式版本；只接受 [`SCHEMA_VERSION`]。
    pub schema: u32,
    /// Always [`PRODUCT`].
    /// 恒为 [`PRODUCT`]。
    pub product: String,
    /// The channel this pointer belongs to.
    /// 本指针所属的通道。
    pub channel: Channel,
    /// The platform this pointer belongs to.
    /// 本指针所属的平台。
    pub platform: Platform,
    /// The version the manifest describes.
    /// 版本清单所描述的版本。
    pub version: ReleaseVersion,
    /// The release manifest of that version and platform.
    /// 该版本与平台的版本清单。
    pub manifest: DocumentRef,
    /// The key that signed this pointer.
    /// 签署本指针的公钥。
    pub key_id: String,
}

impl ChannelPointer {
    /// Verifies a downloaded pointer for the expected channel and platform.
    /// 按期望的通道与平台校验下载的指针。
    pub fn verify(
        document: &[u8],
        signature: &[u8],
        keys: &[TrustedKey],
        channel: Channel,
        platform: Platform,
    ) -> FormatResult<Self> {
        let (pointer, key): (Self, _) = parse_signed(
            "channel pointer",
            MAX_POINTER_BYTES,
            document,
            signature,
            keys,
        )?;
        expect_key_id(&pointer.key_id, key)?;
        pointer.validate()?;
        if pointer.channel != channel || pointer.platform != platform {
            return Err(FormatError::invalid(
                "channel pointer",
                format!(
                    "describes {}/{} instead of {channel}/{platform}",
                    pointer.channel, pointer.platform
                ),
            ));
        }
        Ok(pointer)
    }

    /// Checks the rules that do not depend on where the pointer was fetched from.
    /// 校验与指针获取位置无关的规则。
    pub fn validate(&self) -> FormatResult<()> {
        let invalid = |reason: String| Err(FormatError::invalid("channel pointer", reason));
        if self.schema != SCHEMA_VERSION {
            return invalid(format!("unsupported schema {}", self.schema));
        }
        if self.product != PRODUCT {
            return invalid(format!("describes product `{}`", self.product));
        }
        if self.channel == Channel::Stable && self.version.is_prerelease() {
            return invalid(format!("stable channel names pre-release {}", self.version));
        }
        if self.manifest.size == 0 || self.manifest.size > MAX_MANIFEST_BYTES {
            return invalid(format!(
                "manifest size {} is outside 1..={MAX_MANIFEST_BYTES}",
                self.manifest.size
            ));
        }
        Ok(())
    }

    /// Validates and serializes the pointer as the bytes to sign and publish.
    /// 校验并序列化指针，得到待签名与发布的字节。
    pub fn to_document(&self) -> FormatResult<Vec<u8>> {
        self.validate()?;
        serde_json::to_vec(self).map_err(|source| FormatError::Json {
            what: "channel pointer",
            source,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signing::sign_detached;
    use crate::signing::tests::{TEST_SECRET, test_keys};

    fn pointer(channel: Channel, version: &str) -> ChannelPointer {
        ChannelPointer {
            schema: SCHEMA_VERSION,
            product: PRODUCT.to_owned(),
            channel,
            platform: Platform::LinuxX64,
            version: version.parse().expect("version"),
            manifest: DocumentRef::of(b"{}"),
            key_id: "test-key".to_owned(),
        }
    }

    #[test]
    fn a_signed_pointer_round_trips_for_its_own_channel_and_platform() {
        let document = pointer(Channel::Stable, "0.1.3")
            .to_document()
            .expect("document");
        let signature = sign_detached(&document, &TEST_SECRET);
        let keys = test_keys();
        let verified = ChannelPointer::verify(
            &document,
            &signature,
            &keys,
            Channel::Stable,
            Platform::LinuxX64,
        )
        .expect("verified");
        assert_eq!(verified, pointer(Channel::Stable, "0.1.3"));
        assert!(
            ChannelPointer::verify(
                &document,
                &signature,
                &keys,
                Channel::Preview,
                Platform::LinuxX64
            )
            .is_err()
        );
        assert!(
            ChannelPointer::verify(
                &document,
                &signature,
                &keys,
                Channel::Stable,
                Platform::WindowsX64
            )
            .is_err()
        );
    }

    #[test]
    fn a_pointer_naming_another_key_is_rejected() {
        let mut other = pointer(Channel::Stable, "0.1.3");
        other.key_id = "update-2026".to_owned();
        let document = other.to_document().expect("document");
        let signature = sign_detached(&document, &TEST_SECRET);
        assert!(matches!(
            ChannelPointer::verify(
                &document,
                &signature,
                &test_keys(),
                Channel::Stable,
                Platform::LinuxX64
            ),
            Err(FormatError::KeyIdMismatch { .. })
        ));
    }

    #[test]
    fn stable_pointers_never_name_pre_releases() {
        assert!(
            pointer(Channel::Stable, "0.2.0-preview.1")
                .to_document()
                .is_err()
        );
        assert!(
            pointer(Channel::Preview, "0.2.0-preview.1")
                .to_document()
                .is_ok()
        );
    }

    #[test]
    fn oversized_pointers_are_rejected_before_verification() {
        let document = vec![b' '; MAX_POINTER_BYTES as usize + 1];
        let signature = sign_detached(&document, &TEST_SECRET);
        assert!(matches!(
            ChannelPointer::verify(
                &document,
                &signature,
                &test_keys(),
                Channel::Stable,
                Platform::LinuxX64
            ),
            Err(FormatError::TooLarge { .. })
        ));
    }
}
