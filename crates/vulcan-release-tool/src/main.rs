//! Release workflow tool for the incremental update objects (plan sections 7 and 11).
//! 增量更新对象的发布工作流工具（方案第七、十一节）。
//!
//! `pack` turns a built release tree into component indexes and packs, storing only contents no
//! published index holds; `manifest` writes and signs the release manifest; `pointer` signs a channel
//! pointer; `verify` checks a local object tree the way a client does and installs from it. Every
//! command writes the object layout below `--objects`, which the workflow uploads unchanged. The
//! signing key is read from the environment variable named by `--key-env` and never printed.
//!
//! `pack` 将构建好的发行树转为组件索引与文件包，只存储已发布索引中没有的内容；`manifest` 写入并签署版本
//! 清单；`pointer` 签署通道指针；`verify` 按客户端的方式校验本地对象树并从中安装。每个命令都在
//! `--objects` 之下写出对象布局，工作流原样上传。签名私钥从 `--key-env` 指名的环境变量读取，绝不输出。

mod args;
mod pack;
mod publish;
mod verify;

use std::process::ExitCode;

use anyhow::{Context as _, bail};
use vulcan_install_core::signing::{BUILT_IN_KEYS, secret_key_from_pkcs8_pem};

use crate::args::Args;

/// Command line summary printed on misuse.
/// 用法错误时输出的命令行摘要。
const USAGE: &str = "\
usage:
  vulcan-release-tool pack --tree DIR --rules FILE --objects DIR --components-out FILE [--published DIR]
  vulcan-release-tool manifest --objects DIR --components FILE --version V --platform P
      --install-epoch N --released-at YYYY-MM-DDTHH:MM:SSZ --key-env NAME
      [--notes FILE] [--download KIND=FILE]... [--min-setup-version V]
  vulcan-release-tool pointer --objects DIR --channel C --platform P --version V --key-env NAME
  vulcan-release-tool verify --objects DIR --platform P --version V --install-root DIR [--channel C]";

fn main() -> ExitCode {
    let mut arguments = std::env::args().skip(1);
    let Some(command) = arguments.next() else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let result = match command.as_str() {
        "pack" => run_pack(arguments),
        "manifest" => run_manifest(arguments),
        "pointer" => run_pointer(arguments),
        "verify" => run_verify(arguments),
        _ => {
            eprintln!("unknown command `{command}`\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

/// Runs `pack`.
/// 执行 `pack`。
fn run_pack(arguments: impl Iterator<Item = String>) -> anyhow::Result<()> {
    let args = Args::parse(
        arguments,
        &["tree", "rules", "objects", "components-out", "published"],
    )?;
    let rules = pack::ComponentRules::read(&args.path("rules")?)?;
    let published = pack::published_locations(args.optional_path("published")?.as_deref())?;
    let objects = args.path("objects")?;
    let report = pack::pack_tree(&args.path("tree")?, &rules, published, &objects)?;
    let document = serde_json::to_vec_pretty(&report.components)?;
    let target = args.path("components-out")?;
    std::fs::write(&target, document).with_context(|| format!("writing {}", target.display()))?;
    println!(
        "packed {} components into {} new packs",
        report.components.len(),
        report.new_packs
    );
    Ok(())
}

/// Runs `manifest`.
/// 执行 `manifest`。
fn run_manifest(arguments: impl Iterator<Item = String>) -> anyhow::Result<()> {
    let args = Args::parse(
        arguments,
        &[
            "objects",
            "components",
            "version",
            "platform",
            "install-epoch",
            "released-at",
            "key-env",
            "notes",
            "download",
            "min-setup-version",
        ],
    )?;
    let components_path = args.path("components")?;
    let components = serde_json::from_slice(
        &std::fs::read(&components_path)
            .with_context(|| format!("reading {}", components_path.display()))?,
    )
    .context("the components file is not valid")?;
    let mut downloads = Vec::new();
    for download in args.all("download") {
        let Some((kind, file)) = download.split_once('=') else {
            bail!("`--download {download}` must be KIND=FILE");
        };
        let kind = serde_json::from_value(serde_json::Value::String(kind.to_owned()))
            .with_context(|| format!("`{kind}` is not a download kind"))?;
        downloads.push((kind, file.into()));
    }
    let input = publish::ManifestInput {
        version: args.required("version")?.parse()?,
        platform: args.required("platform")?.parse()?,
        install_epoch: args
            .required("install-epoch")?
            .parse()
            .context("`--install-epoch` must be a positive integer")?,
        released_at: args.required("released-at")?.to_owned(),
        components,
        notes: args.optional_path("notes")?,
        downloads,
        min_setup_version: args
            .optional("min-setup-version")?
            .map(str::parse)
            .transpose()?,
    };
    let secret = signing_secret(args.required("key-env")?)?;
    let manifest = publish::write_manifest(&args.path("objects")?, input, &secret, BUILT_IN_KEYS)?;
    println!(
        "signed the {} {} manifest with {}",
        manifest.version, manifest.platform, manifest.key_id
    );
    Ok(())
}

/// Runs `pointer`.
/// 执行 `pointer`。
fn run_pointer(arguments: impl Iterator<Item = String>) -> anyhow::Result<()> {
    let args = Args::parse(
        arguments,
        &["objects", "channel", "platform", "version", "key-env"],
    )?;
    let secret = signing_secret(args.required("key-env")?)?;
    let pointer = publish::write_pointer(
        &args.path("objects")?,
        args.required("channel")?.parse()?,
        args.required("platform")?.parse()?,
        &args.required("version")?.parse()?,
        &secret,
        BUILT_IN_KEYS,
    )?;
    println!(
        "signed the {}/{} pointer to {}",
        pointer.channel, pointer.platform, pointer.version
    );
    Ok(())
}

/// Runs `verify`.
/// 执行 `verify`。
fn run_verify(arguments: impl Iterator<Item = String>) -> anyhow::Result<()> {
    let args = Args::parse(
        arguments,
        &["objects", "platform", "version", "install-root", "channel"],
    )?;
    let installed = verify::verify_release(
        &args.path("objects")?,
        args.required("platform")?.parse()?,
        &args.required("version")?.parse()?,
        args.optional("channel")?.map(str::parse).transpose()?,
        &args.path("install-root")?,
        BUILT_IN_KEYS,
    )?;
    println!("verified and installed into {}", installed.display());
    Ok(())
}

/// Reads the PKCS#8 signing key from the environment variable `name`.
/// 从环境变量 `name` 读取 PKCS#8 签名私钥。
fn signing_secret(name: &str) -> anyhow::Result<[u8; 32]> {
    let pem = std::env::var(name)
        .with_context(|| format!("the environment variable `{name}` holds no signing key"))?;
    Ok(secret_key_from_pkcs8_pem(&pem)?)
}
