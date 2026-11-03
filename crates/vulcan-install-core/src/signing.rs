//! Ed25519 detached signatures over the raw bytes of channel pointers and release manifests.
//! 覆盖通道指针与版本清单原始字节的 Ed25519 分离签名。
//!
//! Clients trust only the keys built into them. Rotation ships a release that carries both the old
//! and the new key before anything is signed with the new one.
//! 客户端只信任内置公钥。轮换时先发布一版同时内置新旧两把公钥的版本，再改用新钥签名。

use ed25519_dalek::{Signer as _, SigningKey, VerifyingKey};

use crate::error::{FormatError, FormatResult};
use crate::limits::SIGNATURE_BYTES;

/// A public key clients accept release signatures from.
/// 客户端接受其发布签名的公钥。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrustedKey {
    /// Identifier signed documents declare in `keyId`.
    /// 已签名文档在 `keyId` 中声明的标识。
    pub key_id: &'static str,
    /// Raw 32-byte Ed25519 public key.
    /// 32 字节原始 Ed25519 公钥。
    pub public_key: [u8; 32],
}

/// The release signing keys built into this program.
/// 内置于本程序的发布签名公钥。
///
/// `update-2026` in SPKI form is `MCowBQYDK2VwAyEAaKlL7g4a5ryVajOv9aRIhV7OgPL3jrnlVtUyN+Y0xNY=`.
/// `update-2026` 的 SPKI 形式为 `MCowBQYDK2VwAyEAaKlL7g4a5ryVajOv9aRIhV7OgPL3jrnlVtUyN+Y0xNY=`。
pub const BUILT_IN_KEYS: &[TrustedKey] = &[TrustedKey {
    key_id: "update-2026",
    public_key: decode_hex_32("68a94bee0e1ae6bc956a33aff5a448855ece80f2f78eb9e556d53237e634c4d6"),
}];

/// Decodes 64 hexadecimal characters at compile time; invalid input fails the build.
/// 在编译期解码 64 个十六进制字符；输入无效会使构建失败。
const fn decode_hex_32(text: &str) -> [u8; 32] {
    const fn nibble(byte: u8) -> u8 {
        match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            _ => panic!("built-in key must be lowercase hexadecimal"),
        }
    }
    let bytes = text.as_bytes();
    assert!(bytes.len() == 64, "built-in key must be 64 hex characters");
    let mut output = [0_u8; 32];
    let mut index = 0;
    while index < 32 {
        output[index] = (nibble(bytes[2 * index]) << 4) | nibble(bytes[2 * index + 1]);
        index += 1;
    }
    output
}

/// Verifies a detached signature over `document` and returns the one trusted key that signed it.
/// 校验覆盖 `document` 的分离签名，并返回签署它的唯一受信公钥。
///
/// Every key is tried before any JSON is parsed, so untrusted bytes never choose their verifier;
/// the caller then checks that the document's declared `keyId` names the returned key.
/// 解析任何 JSON 之前先尝试全部公钥，不可信字节因此无法选择自己的验证者；调用方随后核对文档声明的
/// `keyId` 是否就是返回的公钥。
pub fn verify_detached<'k>(
    document: &[u8],
    signature: &[u8],
    keys: &'k [TrustedKey],
) -> FormatResult<&'k TrustedKey> {
    let signature: [u8; SIGNATURE_BYTES] =
        signature
            .try_into()
            .map_err(|_| FormatError::SignatureLength {
                expected: SIGNATURE_BYTES,
                actual: signature.len(),
            })?;
    let signature = ed25519_dalek::Signature::from_bytes(&signature);
    let mut matched = None;
    for key in keys {
        let verifying_key = VerifyingKey::from_bytes(&key.public_key).map_err(|error| {
            FormatError::InvalidTrustedKey {
                key_id: key.key_id.to_owned(),
                reason: error.to_string(),
            }
        })?;
        if verifying_key.verify_strict(document, &signature).is_ok() {
            if matched.is_some() {
                return Err(FormatError::AmbiguousSigningKey);
            }
            matched = Some(key);
        }
    }
    matched.ok_or(FormatError::SignatureRejected)
}

/// Bounds, verifies and only then parses a signed JSON document; returns it with its signing key.
/// 先限制大小、再验签，之后才解析已签名的 JSON 文档；返回文档与其签名公钥。
pub(crate) fn parse_signed<'k, T: serde::de::DeserializeOwned>(
    what: &'static str,
    limit: u64,
    document: &[u8],
    signature: &[u8],
    keys: &'k [TrustedKey],
) -> FormatResult<(T, &'k TrustedKey)> {
    if document.len() as u64 > limit {
        return Err(FormatError::TooLarge { what, limit });
    }
    let key = verify_detached(document, signature, keys)?;
    let parsed =
        serde_json::from_slice(document).map_err(|source| FormatError::Json { what, source })?;
    Ok((parsed, key))
}

/// Fails unless the document's declared key id is the key whose signature matched.
/// 除非文档声明的公钥标识就是签名匹配的公钥，否则失败。
pub(crate) fn expect_key_id(declared: &str, matched: &TrustedKey) -> FormatResult<()> {
    if declared == matched.key_id {
        Ok(())
    } else {
        Err(FormatError::KeyIdMismatch {
            declared: declared.to_owned(),
            matched: matched.key_id.to_owned(),
        })
    }
}

/// Signs `document` with a raw 32-byte Ed25519 secret key; used by the release tooling only.
/// 使用 32 字节原始 Ed25519 私钥签署 `document`；仅供发布工具使用。
pub fn sign_detached(document: &[u8], secret_key: &[u8; 32]) -> [u8; SIGNATURE_BYTES] {
    SigningKey::from_bytes(secret_key).sign(document).to_bytes()
}

/// The public key of a raw 32-byte Ed25519 secret key.
/// 32 字节原始 Ed25519 私钥对应的公钥。
pub fn public_key_of(secret_key: &[u8; 32]) -> [u8; 32] {
    SigningKey::from_bytes(secret_key)
        .verifying_key()
        .to_bytes()
}

/// DER prefix of an unencrypted PKCS#8 v1 Ed25519 private key; the 32-byte seed follows.
/// 未加密 PKCS#8 v1 Ed25519 私钥的 DER 前缀；其后紧跟 32 字节种子。
const PKCS8_ED25519_PREFIX: [u8; 16] = [
    0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
];

/// Reads the raw secret key of an unencrypted PKCS#8 PEM (`-----BEGIN PRIVATE KEY-----`), the form
/// `openssl genpkey -algorithm ed25519` writes; errors never echo key material.
/// 读取未加密 PKCS#8 PEM（`-----BEGIN PRIVATE KEY-----`）中的原始私钥，即
/// `openssl genpkey -algorithm ed25519` 写出的形式；错误信息绝不包含密钥内容。
pub fn secret_key_from_pkcs8_pem(pem: &str) -> FormatResult<[u8; 32]> {
    use base64::Engine as _;
    let invalid = |reason: &str| Err(FormatError::invalid("signing key", reason));
    let mut lines = pem.lines().map(str::trim).filter(|line| !line.is_empty());
    if lines.next() != Some("-----BEGIN PRIVATE KEY-----") {
        return invalid("must start with `-----BEGIN PRIVATE KEY-----`");
    }
    let mut body = String::new();
    let mut ended = false;
    for line in lines.by_ref() {
        if line == "-----END PRIVATE KEY-----" {
            ended = true;
            break;
        }
        body.push_str(line);
    }
    if !ended || lines.next().is_some() {
        return invalid("must hold exactly one private key block");
    }
    let Ok(der) = base64::engine::general_purpose::STANDARD.decode(body) else {
        return invalid("the key block is not valid base64");
    };
    match der.strip_prefix(PKCS8_ED25519_PREFIX.as_slice()) {
        Some(seed) if seed.len() == 32 => {
            let mut secret = [0_u8; 32];
            secret.copy_from_slice(seed);
            Ok(secret)
        }
        _ => invalid("is not an unencrypted PKCS#8 Ed25519 private key"),
    }
}

/// The trusted key whose public half belongs to `secret_key`; signing with any other key would
/// publish documents no client accepts.
/// 公钥部分属于 `secret_key` 的受信公钥；用其他私钥签名只会发布任何客户端都不接受的文档。
pub fn trusted_key_of<'k>(
    secret_key: &[u8; 32],
    keys: &'k [TrustedKey],
) -> FormatResult<&'k TrustedKey> {
    let public_key = public_key_of(secret_key);
    keys.iter()
        .find(|key| key.public_key == public_key)
        .ok_or_else(|| {
            FormatError::invalid("signing key", "its public key is not a trusted release key")
        })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn pkcs8_pem_keys_are_read_and_matched_to_trusted_keys() {
        use base64::Engine as _;
        let der: Vec<u8> = PKCS8_ED25519_PREFIX
            .iter()
            .chain(&TEST_SECRET)
            .copied()
            .collect();
        let pem = format!(
            "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n",
            base64::engine::general_purpose::STANDARD.encode(&der)
        );
        let secret = secret_key_from_pkcs8_pem(&pem).expect("pem");
        assert_eq!(secret, TEST_SECRET);
        assert_eq!(
            trusted_key_of(&secret, &test_keys())
                .expect("trusted")
                .key_id,
            "test-key"
        );
        assert!(trusted_key_of(&secret, BUILT_IN_KEYS).is_err());
        assert!(secret_key_from_pkcs8_pem(&pem.replace("PRIVATE KEY", "PUBLIC KEY")).is_err());
        assert!(secret_key_from_pkcs8_pem(&format!("{pem}{pem}")).is_err());
        let short = format!(
            "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n",
            base64::engine::general_purpose::STANDARD.encode(&der[..47])
        );
        assert!(secret_key_from_pkcs8_pem(&short).is_err());
    }

    /// A throwaway secret key for tests.
    /// 测试用的一次性私钥。
    pub(crate) const TEST_SECRET: [u8; 32] = [7; 32];

    /// A key set that trusts only [`TEST_SECRET`].
    /// 只信任 [`TEST_SECRET`] 的公钥集。
    pub(crate) fn test_keys() -> Vec<TrustedKey> {
        vec![TrustedKey {
            key_id: "test-key",
            public_key: public_key_of(&TEST_SECRET),
        }]
    }

    #[test]
    fn built_in_keys_are_valid_and_unique() {
        for (index, key) in BUILT_IN_KEYS.iter().enumerate() {
            assert!(VerifyingKey::from_bytes(&key.public_key).is_ok());
            assert!(
                BUILT_IN_KEYS[..index]
                    .iter()
                    .all(|other| other.key_id != key.key_id)
            );
        }
    }

    #[test]
    fn signatures_verify_only_over_the_signed_bytes() {
        let keys = test_keys();
        let signature = sign_detached(b"document", &TEST_SECRET);
        let matched = verify_detached(b"document", &signature, &keys).expect("valid signature");
        assert_eq!(matched.key_id, "test-key");
        assert!(matches!(
            verify_detached(b"documenT", &signature, &keys),
            Err(FormatError::SignatureRejected)
        ));
        assert!(matches!(
            verify_detached(b"document", &signature[..63], &keys),
            Err(FormatError::SignatureLength { .. })
        ));
        assert!(matches!(
            verify_detached(b"document", &signature, BUILT_IN_KEYS),
            Err(FormatError::SignatureRejected)
        ));
    }

    #[test]
    fn a_key_trusted_twice_is_ambiguous() {
        let mut keys = test_keys();
        keys.push(TrustedKey {
            key_id: "same-key-other-name",
            ..keys[0]
        });
        let signature = sign_detached(b"document", &TEST_SECRET);
        assert!(matches!(
            verify_detached(b"document", &signature, &keys),
            Err(FormatError::AmbiguousSigningKey)
        ));
    }
}
