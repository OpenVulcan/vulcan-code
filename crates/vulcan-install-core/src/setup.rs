//! The setup program and the install scripts, released apart from the product (plan section 18.6): the
//! files of a setup version live at `setup/<version>/<file>`, and the signed pointer
//! `setup/channels/<channel>.json` names the newest setup version of a channel with the digest and
//! size of every platform's program and of both scripts. The setup version is its own; a product
//! release asks for a newer one only through `minSetupVersion`.
//! 安装器与安装脚本独立于产品发布（方案 18.6 节）：某安装器版本的文件位于 `setup/<版本>/<文件>`，已签名指针
//! `setup/channels/<通道>.json` 写明该通道的最新安装器版本，以及各平台程序与两个脚本的摘要和大小。安装器
//! 版本独立编号；产品版本只通过 `minSetupVersion` 要求更新的安装器。
//!
//! Like the product pointers, the pointer and its `.sig` cannot be replaced atomically, so a reader
//! whose verification fails fetches both once more.
//! 与产品指针一样，指针与其 `.sig` 无法原子替换，因此验签失败的读取方应重新获取两者一次。

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::digest::Sha256Digest;
use crate::error::{FormatError, FormatResult};
use crate::limits::{MAX_POINTER_BYTES, MAX_SCRIPT_BYTES, MAX_SETUP_PROGRAM_BYTES};
use crate::manifest::is_utc_timestamp;
use crate::names::{Channel, Platform, ReleaseVersion, SCHEMA_VERSION};
use crate::signing::{TrustedKey, expect_key_id, parse_signed};

/// The `product` of every setup pointer.
/// 每个安装器指针的 `product`。
pub const SETUP_PRODUCT: &str = "vulcan-code-setup";

/// The install scripts every setup version publishes; `scripts/entry-scripts.mjs` fills them.
/// 每个安装器版本发布的安装脚本；由 `scripts/entry-scripts.mjs` 填写。
pub const ENTRY_SCRIPTS: [&str; 2] = ["install.sh", "install.ps1"];

/// The file name of a platform's setup program; Windows adds `.exe`.
/// 某平台安装器的文件名；Windows 加 `.exe`。
pub fn program_file(version: &ReleaseVersion, platform: Platform) -> String {
    let extension = if platform.is_windows() { ".exe" } else { "" };
    format!("vulcan-code-setup-{version}-{platform}{extension}")
}

/// One published file of a setup version.
/// 某安装器版本发布的一个文件。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetupFile {
    /// File name below `setup/<version>/`.
    /// `setup/<版本>/` 下的文件名。
    pub file: String,
    /// SHA-256 of the file.
    /// 文件的 SHA-256。
    pub sha256: Sha256Digest,
    /// Size of the file.
    /// 文件大小。
    pub size: u64,
}

/// The newest setup version of one channel.
/// 某通道的最新安装器版本。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupPointer {
    /// Format version; only [`SCHEMA_VERSION`] is accepted.
    /// 格式版本；只接受 [`SCHEMA_VERSION`]。
    pub schema: u32,
    /// Always [`SETUP_PRODUCT`].
    /// 恒为 [`SETUP_PRODUCT`]。
    pub product: String,
    /// The channel this pointer belongs to.
    /// 本指针所属的通道。
    pub channel: Channel,
    /// The setup version.
    /// 安装器版本。
    pub version: ReleaseVersion,
    /// Release time as `YYYY-MM-DDTHH:MM:SSZ`.
    /// 发布时间，格式为 `YYYY-MM-DDTHH:MM:SSZ`。
    pub released_at: String,
    /// The setup program of every platform.
    /// 每个平台的安装器。
    pub programs: BTreeMap<Platform, SetupFile>,
    /// Both [`ENTRY_SCRIPTS`], filled for this version.
    /// 为此版本填写的两个 [`ENTRY_SCRIPTS`]。
    pub scripts: Vec<SetupFile>,
    /// The key that signed this pointer.
    /// 签署本指针的公钥。
    pub key_id: String,
}

impl SetupPointer {
    /// Verifies a downloaded pointer for the expected channel.
    /// 按期望的通道校验下载的指针。
    pub fn verify(
        document: &[u8],
        signature: &[u8],
        keys: &[TrustedKey],
        channel: Channel,
    ) -> FormatResult<Self> {
        let (pointer, key): (Self, _) = parse_signed(
            "setup pointer",
            MAX_POINTER_BYTES,
            document,
            signature,
            keys,
        )?;
        expect_key_id(&pointer.key_id, key)?;
        pointer.validate()?;
        if pointer.channel != channel {
            return Err(FormatError::invalid(
                "setup pointer",
                format!("describes {} instead of {channel}", pointer.channel),
            ));
        }
        Ok(pointer)
    }

    /// Checks the rules that do not depend on where the pointer was fetched from.
    /// 校验与指针获取位置无关的规则。
    pub fn validate(&self) -> FormatResult<()> {
        let invalid = |reason: String| Err(FormatError::invalid("setup pointer", reason));
        if self.schema != SCHEMA_VERSION {
            return invalid(format!("unsupported schema {}", self.schema));
        }
        if self.product != SETUP_PRODUCT {
            return invalid(format!("describes product `{}`", self.product));
        }
        if self.channel == Channel::Stable && self.version.is_prerelease() {
            return invalid(format!("stable channel names pre-release {}", self.version));
        }
        if !is_utc_timestamp(&self.released_at) {
            return invalid(format!(
                "releasedAt `{}` is not YYYY-MM-DDTHH:MM:SSZ",
                self.released_at
            ));
        }
        let platforms: BTreeSet<Platform> = self.programs.keys().copied().collect();
        if platforms != Platform::ALL.into_iter().collect() {
            return invalid(String::from(
                "does not list exactly one program per platform",
            ));
        }
        for (platform, program) in &self.programs {
            if program.file != program_file(&self.version, *platform) {
                return invalid(format!(
                    "the {platform} program `{}` is not named for {}",
                    program.file, self.version
                ));
            }
            check_size(program, MAX_SETUP_PROGRAM_BYTES)?;
        }
        let scripts: Vec<&str> = self.scripts.iter().map(|file| file.file.as_str()).collect();
        if scripts != ENTRY_SCRIPTS {
            return invalid(format!(
                "lists scripts {scripts:?} instead of {ENTRY_SCRIPTS:?}"
            ));
        }
        for script in &self.scripts {
            check_size(script, MAX_SCRIPT_BYTES)?;
        }
        Ok(())
    }

    /// Validates and serializes the pointer as the bytes to sign and publish.
    /// 校验并序列化指针，得到待签名与发布的字节。
    pub fn to_document(&self) -> FormatResult<Vec<u8>> {
        self.validate()?;
        let document = serde_json::to_vec(self).map_err(|source| FormatError::Json {
            what: "setup pointer",
            source,
        })?;
        if document.len() as u64 > MAX_POINTER_BYTES {
            return Err(FormatError::TooLarge {
                what: "setup pointer",
                limit: MAX_POINTER_BYTES,
            });
        }
        Ok(document)
    }

    /// The setup program of `platform`; a validated pointer has one for every platform.
    /// `platform` 的安装器；已校验的指针为每个平台都列出一个。
    pub fn program(&self, platform: Platform) -> FormatResult<&SetupFile> {
        self.programs.get(&platform).ok_or_else(|| {
            FormatError::invalid("setup pointer", format!("has no {platform} program"))
        })
    }
}

/// Fails unless `file` has a size in `1..=limit`.
/// 除非 `file` 的大小在 `1..=limit` 内，否则失败。
fn check_size(file: &SetupFile, limit: u64) -> FormatResult<()> {
    if file.size == 0 || file.size > limit {
        return Err(FormatError::invalid(
            "setup pointer",
            format!("`{}` size {} is outside 1..={limit}", file.file, file.size),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signing::sign_detached;
    use crate::signing::tests::{TEST_SECRET, test_keys};

    fn pointer(channel: Channel, version: &str) -> SetupPointer {
        let version: ReleaseVersion = version.parse().expect("version");
        let file = |file: String| SetupFile {
            sha256: Sha256Digest::of(file.as_bytes()),
            file,
            size: 10,
        };
        SetupPointer {
            schema: SCHEMA_VERSION,
            product: SETUP_PRODUCT.to_owned(),
            channel,
            programs: Platform::ALL
                .into_iter()
                .map(|platform| (platform, file(program_file(&version, platform))))
                .collect(),
            scripts: ENTRY_SCRIPTS
                .into_iter()
                .map(|name| file(name.to_owned()))
                .collect(),
            version,
            released_at: "2026-11-20T08:00:00Z".to_owned(),
            key_id: "test-key".to_owned(),
        }
    }

    #[test]
    fn a_signed_pointer_round_trips_for_its_own_channel_only() {
        let original = pointer(Channel::Preview, "0.2.0-rc.1");
        let document = original.to_document().expect("document");
        let signature = sign_detached(&document, &TEST_SECRET);
        let verified = SetupPointer::verify(&document, &signature, &test_keys(), Channel::Preview)
            .expect("verified");
        assert_eq!(verified, original);
        assert_eq!(
            verified
                .program(Platform::WindowsX64)
                .expect("program")
                .file,
            "vulcan-code-setup-0.2.0-rc.1-windows-x64.exe"
        );
        assert!(
            SetupPointer::verify(&document, &signature, &test_keys(), Channel::Stable).is_err()
        );
    }

    #[test]
    fn every_platform_and_both_scripts_are_required() {
        assert!(pointer(Channel::Stable, "0.2.0-rc.1").validate().is_err());

        let mut missing = pointer(Channel::Stable, "0.2.0");
        missing.programs.remove(&Platform::MacosX64);
        assert!(missing.validate().is_err());

        let mut renamed = pointer(Channel::Stable, "0.2.0");
        if let Some(program) = renamed.programs.get_mut(&Platform::LinuxX64) {
            program.file = "vulcan-code-setup-0.1.9-linux-x64".to_owned();
        }
        assert!(renamed.validate().is_err());

        let mut one_script = pointer(Channel::Stable, "0.2.0");
        one_script.scripts.pop();
        assert!(one_script.validate().is_err());

        let mut oversized = pointer(Channel::Stable, "0.2.0");
        oversized.scripts[0].size = MAX_SCRIPT_BYTES + 1;
        assert!(oversized.validate().is_err());
    }
}
