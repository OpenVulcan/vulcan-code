//! Publishing the setup program apart from the product (plan section 18.6): `setup-publish` copies
//! every platform's program and both install scripts to `setup/<version>/` and signs the setup
//! pointer of a channel; `setup-promote` signs another channel's pointer over the same files after
//! verifying the pointer it starts from; `setup-check` verifies a downloaded pointer and program, as
//! the product release does before it bundles the stable setup program.
//! 独立于产品发布安装器（方案 18.6 节）：`setup-publish` 把各平台程序与两个安装脚本复制到
//! `setup/<版本>/`，并签署某通道的安装器指针；`setup-promote` 校验起始指针后，为同一组文件签署另一通道的
//! 指针；`setup-check` 校验下载的指针与程序，产品发布在内置 stable 安装器之前执行此校验。

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context as _, bail};
use vulcan_install_core::layout::{setup_file_key, setup_pointer_key, signature_key};
use vulcan_install_core::names::{Channel, Platform, ReleaseVersion, SCHEMA_VERSION};
use vulcan_install_core::pack::write_object;
use vulcan_install_core::setup::{
    ENTRY_SCRIPTS, SETUP_PRODUCT, SetupFile, SetupPointer, program_file,
};
use vulcan_install_core::signing::{TrustedKey, sign_detached, trusted_key_of};

use crate::publish::{hash_file, read_object};

/// Everything a setup release is made from.
/// 构成一次安装器发布的全部内容。
pub struct SetupInput<'a> {
    /// The setup version.
    /// 安装器版本。
    pub version: ReleaseVersion,
    /// The channel whose pointer is signed.
    /// 要签署指针的通道。
    pub channel: Channel,
    /// Release time as `YYYY-MM-DDTHH:MM:SSZ`.
    /// 发布时间，格式为 `YYYY-MM-DDTHH:MM:SSZ`。
    pub released_at: String,
    /// Holds every platform's program under its release name ([`program_file`]).
    /// 以发行名称（[`program_file`]）存放各平台程序。
    pub programs: &'a Path,
    /// Holds both filled [`ENTRY_SCRIPTS`].
    /// 存放两个已填写的 [`ENTRY_SCRIPTS`]。
    pub scripts: &'a Path,
}

/// Copies the programs and scripts below `objects`, then writes and signs the channel's pointer.
/// 将程序与脚本复制到 `objects` 之下，再写入并签署该通道的指针。
pub fn publish_setup(
    objects: &Path,
    input: SetupInput<'_>,
    secret: &[u8; 32],
    keys: &[TrustedKey],
) -> anyhow::Result<SetupPointer> {
    let key = trusted_key_of(secret, keys)?;
    let mut programs = BTreeMap::new();
    for platform in Platform::ALL {
        let file = program_file(&input.version, platform);
        programs.insert(
            platform,
            copy_file(objects, &input.version, input.programs, &file)?,
        );
    }
    let mut scripts = Vec::with_capacity(ENTRY_SCRIPTS.len());
    for script in ENTRY_SCRIPTS {
        scripts.push(copy_file(objects, &input.version, input.scripts, script)?);
    }
    let pointer = SetupPointer {
        schema: SCHEMA_VERSION,
        product: SETUP_PRODUCT.to_owned(),
        channel: input.channel,
        version: input.version,
        released_at: input.released_at,
        programs,
        scripts,
        key_id: key.key_id.to_owned(),
    };
    sign_pointer(objects, &pointer, secret)?;
    Ok(pointer)
}

/// Verifies the `from` pointer below `objects`, which must name `version`, and signs a `to` pointer
/// over the same files.
/// 校验 `objects` 之下的 `from` 指针（必须指向 `version`），并为同一组文件签署 `to` 指针。
pub fn promote_setup(
    objects: &Path,
    from: Channel,
    to: Channel,
    version: &ReleaseVersion,
    secret: &[u8; 32],
    keys: &[TrustedKey],
) -> anyhow::Result<SetupPointer> {
    let key = trusted_key_of(secret, keys)?;
    let source_key = setup_pointer_key(from);
    let source = SetupPointer::verify(
        &read_object(objects, &source_key)?,
        &read_object(objects, &signature_key(&source_key))?,
        keys,
        from,
    )
    .with_context(|| format!("the {from} setup pointer does not verify"))?;
    if &source.version != version {
        bail!(
            "the {from} setup pointer names {} instead of {version}",
            source.version
        );
    }
    let pointer = SetupPointer {
        channel: to,
        key_id: key.key_id.to_owned(),
        ..source
    };
    sign_pointer(objects, &pointer, secret)?;
    Ok(pointer)
}

/// Verifies a downloaded setup pointer of `channel` and the `platform` program beside it.
/// 校验下载的 `channel` 安装器指针及其旁边的 `platform` 程序。
pub fn check_setup(
    pointer: &Path,
    channel: Channel,
    platform: Platform,
    program: &Path,
    keys: &[TrustedKey],
) -> anyhow::Result<SetupPointer> {
    let document =
        std::fs::read(pointer).with_context(|| format!("reading {}", pointer.display()))?;
    let signature_path = pointer.with_file_name(format!(
        "{}.sig",
        pointer
            .file_name()
            .and_then(|name| name.to_str())
            .context("the pointer has no usable file name")?
    ));
    let signature = std::fs::read(&signature_path)
        .with_context(|| format!("reading {}", signature_path.display()))?;
    let pointer = SetupPointer::verify(&document, &signature, keys, channel)?;
    let expected = pointer.program(platform)?;
    let (size, sha256) = hash_file(program)?;
    if size != expected.size || sha256 != expected.sha256 {
        bail!(
            "{} does not match the {platform} program of setup {}",
            program.display(),
            pointer.version
        );
    }
    Ok(pointer)
}

/// Copies `source/file` to `setup/<version>/<file>` below `objects` and describes it.
/// 将 `source/file` 复制到 `objects` 之下的 `setup/<版本>/<文件>`，并描述该文件。
fn copy_file(
    objects: &Path,
    version: &ReleaseVersion,
    source: &Path,
    file: &str,
) -> anyhow::Result<SetupFile> {
    let path = source.join(file);
    let target = objects.join(setup_file_key(version, file));
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(&path, &target).with_context(|| format!("copying {}", path.display()))?;
    let (size, sha256) = hash_file(&target)?;
    Ok(SetupFile {
        file: file.to_owned(),
        sha256,
        size,
    })
}

/// Writes `pointer` and its signature below `objects`.
/// 将 `pointer` 及其签名写入 `objects` 之下。
fn sign_pointer(objects: &Path, pointer: &SetupPointer, secret: &[u8; 32]) -> anyhow::Result<()> {
    let document = pointer.to_document()?;
    let key = setup_pointer_key(pointer.channel);
    write_object(objects, &key, &document)?;
    write_object(
        objects,
        &signature_key(&key),
        &sign_detached(&document, secret),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verify::tests::{SECRET, keys};

    /// Writes one synthetic program per platform and both scripts below `root`.
    /// 在 `root` 之下为每个平台写一个合成程序，并写入两个脚本。
    fn inputs(root: &Path, version: &ReleaseVersion) {
        std::fs::create_dir_all(root).expect("root");
        for platform in Platform::ALL {
            std::fs::write(
                root.join(program_file(version, platform)),
                platform.as_str(),
            )
            .expect("program");
        }
        for script in ENTRY_SCRIPTS {
            std::fs::write(root.join(script), script).expect("script");
        }
    }

    #[test]
    fn a_published_setup_is_promoted_and_checked_against_its_files() {
        let temp = tempfile::tempdir().expect("temp");
        let objects = temp.path().join("objects");
        let built = temp.path().join("built");
        let version: ReleaseVersion = "0.2.0".parse().expect("version");
        inputs(&built, &version);
        let keys = keys();
        let preview = publish_setup(
            &objects,
            SetupInput {
                version: version.clone(),
                channel: Channel::Preview,
                released_at: "2026-11-20T08:00:00Z".to_owned(),
                programs: &built,
                scripts: &built,
            },
            &SECRET,
            &keys,
        )
        .expect("publish");
        assert!(
            objects
                .join(setup_file_key(&version, "install.ps1"))
                .is_file()
        );

        let older: ReleaseVersion = "0.1.0".parse().expect("version");
        assert!(
            promote_setup(
                &objects,
                Channel::Preview,
                Channel::Stable,
                &older,
                &SECRET,
                &keys
            )
            .is_err()
        );
        let stable = promote_setup(
            &objects,
            Channel::Preview,
            Channel::Stable,
            &version,
            &SECRET,
            &keys,
        )
        .expect("promote");
        assert_eq!(stable.programs, preview.programs);

        let pointer = objects.join(setup_pointer_key(Channel::Stable));
        let windows = built.join(program_file(&version, Platform::WindowsX64));
        check_setup(
            &pointer,
            Channel::Stable,
            Platform::WindowsX64,
            &windows,
            &keys,
        )
        .expect("check");
        assert!(
            check_setup(
                &pointer,
                Channel::Stable,
                Platform::LinuxX64,
                &windows,
                &keys
            )
            .is_err()
        );
        assert!(
            check_setup(
                &pointer,
                Channel::Preview,
                Platform::WindowsX64,
                &windows,
                &keys
            )
            .is_err()
        );
    }

    #[test]
    fn a_missing_program_stops_the_publication() {
        let temp = tempfile::tempdir().expect("temp");
        let built = temp.path().join("built");
        let version: ReleaseVersion = "0.2.0".parse().expect("version");
        inputs(&built, &version);
        std::fs::remove_file(built.join(program_file(&version, Platform::MacosX64)))
            .expect("remove");
        assert!(
            publish_setup(
                &temp.path().join("objects"),
                SetupInput {
                    version,
                    channel: Channel::Preview,
                    released_at: "2026-11-20T08:00:00Z".to_owned(),
                    programs: &built,
                    scripts: &built,
                },
                &SECRET,
                &keys(),
            )
            .is_err()
        );
    }
}
