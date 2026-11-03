//! SHA-256 digests as they appear in release documents: 64 lowercase hexadecimal characters.
//! 发布文档中的 SHA-256 摘要：64 个小写十六进制字符。

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest as _, Sha256};

use crate::error::{FormatError, FormatResult};

/// A SHA-256 digest; its text form is lowercase hexadecimal and no other spelling is accepted.
/// SHA-256 摘要；文本形式为小写十六进制，不接受其他写法。
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Sha256Digest([u8; 32]);

impl Sha256Digest {
    /// Wraps raw digest bytes.
    /// 包装原始摘要字节。
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the raw digest bytes.
    /// 返回原始摘要字节。
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Digests a byte slice.
    /// 计算字节切片的摘要。
    pub fn of(bytes: &[u8]) -> Self {
        Self(Sha256::digest(bytes).into())
    }

    /// Finishes an incremental hasher.
    /// 结束增量哈希计算。
    pub fn finish(hasher: Sha256) -> Self {
        Self(hasher.finalize().into())
    }

    /// Fails unless `actual` equals this promised digest.
    /// 除非 `actual` 等于本承诺摘要，否则失败。
    pub fn expect_eq(self, what: &'static str, actual: Self) -> FormatResult<()> {
        if self == actual {
            Ok(())
        } else {
            Err(FormatError::DigestMismatch {
                what,
                expected: self,
                actual,
            })
        }
    }
}

impl fmt::Display for Sha256Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&hex::encode(self.0))
    }
}

impl fmt::Debug for Sha256Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Sha256Digest({self})")
    }
}

impl FromStr for Sha256Digest {
    type Err = FormatError;

    fn from_str(text: &str) -> FormatResult<Self> {
        let well_formed = text.len() == 64
            && text
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
        if !well_formed {
            return Err(FormatError::invalid(
                "sha256 digest",
                "expected 64 lowercase hexadecimal characters",
            ));
        }
        let mut bytes = [0_u8; 32];
        hex::decode_to_slice(text, &mut bytes)
            .map_err(|error| FormatError::invalid("sha256 digest", error.to_string()))?;
        Ok(Self(bytes))
    }
}

impl Serialize for Sha256Digest {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Sha256Digest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_form_is_strict_lowercase_hex() {
        let digest = Sha256Digest::of(b"abc");
        let text = digest.to_string();
        assert_eq!(text.len(), 64);
        assert_eq!(text.parse::<Sha256Digest>().expect("round trip"), digest);
        assert!(text.to_uppercase().parse::<Sha256Digest>().is_err());
        assert!(text[..63].parse::<Sha256Digest>().is_err());
        assert!(format!("{text}0").parse::<Sha256Digest>().is_err());
    }
}
