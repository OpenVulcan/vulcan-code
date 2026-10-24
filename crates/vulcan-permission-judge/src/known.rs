//! Official judge packages this build recognizes, their format variants and download locations.
//! 本构建识别的官方判定模型包、其格式变体与下载位置。
//!
//! A package is official when the checkpoint it was converted from is listed here; any other
//! verified package can still be registered, but only as a custom model. The list names packages
//! for onboarding and settings, and is the single source for the official or custom mark.
//! 模型包的转换来源检查点列在此处时即为官方模型包；其他通过验证的模型包仍可登记，但只能作为自定义
//! 模型。该清单为引导与设置页提供模型包名称，并是官方或自定义标记的唯一来源。
//!
//! Each package lists the weight formats it is published in and the devices each format is
//! verified on. A format without a published download location is still listed, so the
//! recommendation stays visible; it can only be registered from a local directory until then.
//! 每个模型包列出其发布的权重格式与各格式经过验证的设备。尚未公布下载位置的格式仍会列出，使推荐保持
//! 可见；在此之前只能从本机目录登记。

use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

use crate::protocol::JudgeDevice;

/// Schema version of `known-packages.json` this build reads.
/// 本构建读取的 `known-packages.json` Schema 版本。
const KNOWN_PACKAGES_SCHEMA_VERSION: u32 = 3;

/// Lowest CUDA compute capability with native half-precision arithmetic; older GPUs emulate it
/// slowly, so they are recommended full precision.
/// 具备原生半精度运算的最低 CUDA 计算能力；更旧的显卡以较慢的方式模拟，因此推荐全精度。
pub const FP16_MIN_CUDA_COMPUTE_CAPABILITY: (u32, u32) = (5, 3);

/// Weight format of a published package.
/// 已发布模型包的权重格式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ModelFormat {
    /// Half-precision projections with full-precision attention: half the memory and fastest on GPUs.
    /// 半精度投影加全精度注意力：显存减半，在 GPU 上最快。
    #[serde(rename = "fp16-mixed")]
    Fp16Mixed,
    /// Full precision throughout: fastest on CPUs and GPUs without native half precision.
    /// 全程全精度：在 CPU 与不具备原生半精度的 GPU 上最快。
    #[serde(rename = "fp32")]
    Fp32,
}

impl ModelFormat {
    /// Stable key, also the key of its localized label.
    /// 稳定键，同时是其本地化标签的键。
    pub fn key(self) -> &'static str {
        match self {
            ModelFormat::Fp16Mixed => "fp16-mixed",
            ModelFormat::Fp32 => "fp32",
        }
    }

    /// Format of a package from the precision its `runtime-config.json` declares, or `None` for a
    /// precision no published format uses.
    /// 由模型包 `runtime-config.json` 声明的精度得出格式；任何已发布格式都不使用的精度返回 `None`。
    pub fn from_precision(precision: &str) -> Option<Self> {
        match precision {
            "fp32" => Some(ModelFormat::Fp32),
            precision if precision.starts_with("fp16") => Some(ModelFormat::Fp16Mixed),
            _ => None,
        }
    }

    /// Formats in the order a device runs them best.
    /// 按设备运行效果由好到差排列的格式。
    fn preference(device: JudgeDevice) -> [ModelFormat; 2] {
        match device {
            JudgeDevice::Cuda | JudgeDevice::WebGpu => [ModelFormat::Fp16Mixed, ModelFormat::Fp32],
            JudgeDevice::Cpu => [ModelFormat::Fp32, ModelFormat::Fp16Mixed],
        }
    }
}

/// Published location of one format: the package directory as files below `base_url`.
/// 一种格式的发布位置：以 `base_url` 下文件形式提供的模型包目录。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnownDownload {
    /// HTTPS directory URL ending in `/`; `MANIFEST.json` and every file it lists sit below it.
    /// 以 `/` 结尾的 HTTPS 目录 URL；`MANIFEST.json` 及其列出的每个文件都位于其下。
    pub base_url: String,
    /// SHA-256 of `MANIFEST.json`, which pins every other file of the package.
    /// `MANIFEST.json` 的 SHA-256，它固定了模型包的其他每个文件。
    pub manifest_sha256: String,
    /// Total size of all listed files, shown before downloading.
    /// 所有列出文件的总大小，下载前展示。
    pub total_bytes: u64,
}

/// One format a package is published in.
/// 模型包发布的一种格式。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnownVariant {
    /// Weight format.
    /// 权重格式。
    pub format: ModelFormat,
    /// Devices this format is verified on.
    /// 该格式经过验证的设备。
    pub devices: Vec<JudgeDevice>,
    /// Published location, or `None` while it is not published yet.
    /// 发布位置；尚未发布时为 `None`。
    pub download: Option<KnownDownload>,
}

/// One official package family.
/// 一个官方模型包家族。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnownPackage {
    /// Stable package identifier, also the key of its localized description.
    /// 稳定的模型包标识，同时是其本地化说明的键。
    pub id: String,
    /// Product name shown as is in every language.
    /// 在所有语言中原样显示的产品名称。
    pub display_name: String,
    /// Digests of the trained checkpoints released under this package.
    /// 以该模型包发布的训练检查点摘要。
    pub checkpoints: Vec<String>,
    /// Model id an OpenVulcan edge endpoint serves this package under, or `None` when it has no
    /// official direct route.
    /// OpenVulcan 边缘端点提供该模型包所用的模型 id；没有官方直连路由时为 `None`。
    pub edge_model_id: Option<String>,
    /// Formats the package is published in.
    /// 模型包发布的格式。
    pub variants: Vec<KnownVariant>,
}

impl KnownPackage {
    /// The published format of this package.
    /// 该模型包的某个已发布格式。
    pub fn variant(&self, format: ModelFormat) -> Option<&KnownVariant> {
        self.variants
            .iter()
            .find(|variant| variant.format == format)
    }

    /// The format to recommend for `device`: the first one the device runs best that is verified on
    /// it. Half precision on CUDA also needs a compute capability with native half arithmetic;
    /// `cuda_capability` is the lowest capability among the NVIDIA GPUs, `None` when unknown.
    /// 为 `device` 推荐的格式：按设备运行效果排序、且已在该设备上验证的第一个格式。CUDA 上的半精度还需要
    /// 具备原生半精度运算的计算能力；`cuda_capability` 为各 NVIDIA 显卡中最低的计算能力，未知时为 `None`。
    pub fn recommended_variant(
        &self,
        device: JudgeDevice,
        cuda_capability: Option<(u32, u32)>,
    ) -> Option<&KnownVariant> {
        ModelFormat::preference(device)
            .into_iter()
            .filter(|format| {
                !(device == JudgeDevice::Cuda
                    && *format == ModelFormat::Fp16Mixed
                    && cuda_capability
                        .is_some_and(|capability| capability < FP16_MIN_CUDA_COMPUTE_CAPABILITY))
            })
            .find_map(|format| {
                self.variant(format)
                    .filter(|variant| variant.devices.contains(&device))
            })
    }
}

/// Shape of `known-packages.json`.
/// `known-packages.json` 的形状。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KnownPackagesFile {
    schema_version: u32,
    packages: Vec<KnownPackage>,
}

/// Parsed and checked list, built once from the embedded resource.
/// 从内嵌资源构建一次的已解析并已检查清单。
static KNOWN_PACKAGES: LazyLock<Vec<KnownPackage>> = LazyLock::new(|| {
    parse_known_packages(include_str!("../resources/known-packages.json"))
        .expect("embedded known-packages.json must be valid")
});

/// Tells whether `text` is a lowercase hex SHA-256.
/// 判断 `text` 是否为小写十六进制 SHA-256。
fn is_sha256(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Tells whether `text` is an identifier the edge-access route accepts: 1–64 characters of
/// `[A-Za-z0-9._:-]`.
/// 判断 `text` 是否为边缘访问接口接受的标识：1–64 个 `[A-Za-z0-9._:-]` 字符。
fn is_edge_model_id(text: &str) -> bool {
    (1..=64).contains(&text.len())
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte))
}

/// Parses the list and refuses duplicate identifiers, checkpoints, edge models or formats,
/// malformed digests or edge model ids, and download locations that are not HTTPS directories.
/// 解析清单，并拒绝重复的标识、检查点、边缘模型或格式，格式错误的摘要或边缘模型 id，以及不是 HTTPS
/// 目录的下载位置。
fn parse_known_packages(text: &str) -> Result<Vec<KnownPackage>, String> {
    let file: KnownPackagesFile = serde_json::from_str(text).map_err(|error| error.to_string())?;
    if file.schema_version != KNOWN_PACKAGES_SCHEMA_VERSION {
        return Err(format!(
            "known packages schema {} is not {KNOWN_PACKAGES_SCHEMA_VERSION}",
            file.schema_version
        ));
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut checkpoints = std::collections::BTreeSet::new();
    let mut edge_models = std::collections::BTreeSet::new();
    for package in &file.packages {
        if package.id.is_empty() || !ids.insert(package.id.as_str()) {
            return Err(format!("package id `{}` is empty or repeated", package.id));
        }
        if let Some(model) = &package.edge_model_id
            && (!is_edge_model_id(model) || !edge_models.insert(model.as_str()))
        {
            return Err(format!(
                "package `{}` edge model `{model}` is malformed or repeated",
                package.id
            ));
        }
        if package.display_name.trim().is_empty()
            || package.checkpoints.is_empty()
            || package.variants.is_empty()
        {
            return Err(format!(
                "package `{}` lacks a name, checkpoints or formats",
                package.id
            ));
        }
        for checkpoint in &package.checkpoints {
            if !is_sha256(checkpoint) || !checkpoints.insert(checkpoint.as_str()) {
                return Err(format!(
                    "checkpoint `{checkpoint}` is not a unique lowercase SHA-256"
                ));
            }
        }
        let mut formats = std::collections::BTreeSet::new();
        for variant in &package.variants {
            if !formats.insert(variant.format) || variant.devices.is_empty() {
                return Err(format!(
                    "package `{}` repeats format `{}` or lists no device for it",
                    package.id,
                    variant.format.key()
                ));
            }
            if let Some(download) = &variant.download {
                let valid = download.base_url.starts_with("https://")
                    && download.base_url.ends_with('/')
                    && is_sha256(&download.manifest_sha256)
                    && download.total_bytes > 0;
                if !valid {
                    return Err(format!(
                        "package `{}` format `{}` has a malformed download location",
                        package.id,
                        variant.format.key()
                    ));
                }
            }
        }
    }
    Ok(file.packages)
}

/// Every official package, in display order.
/// 所有官方模型包，按显示顺序排列。
pub fn known_packages() -> &'static [KnownPackage] {
    &KNOWN_PACKAGES
}

/// The official package with identifier `id`.
/// 标识为 `id` 的官方模型包。
pub fn known_package(id: &str) -> Option<&'static KnownPackage> {
    known_packages().iter().find(|package| package.id == id)
}

/// The official package a checkpoint belongs to, or `None` for a custom model.
/// 检查点所属的官方模型包；自定义模型返回 `None`。
pub fn find_known_package(parent_sha256: &str) -> Option<&'static KnownPackage> {
    known_packages().iter().find(|package| {
        package
            .checkpoints
            .iter()
            .any(|checkpoint| checkpoint == parent_sha256)
    })
}

/// The official package an edge endpoint serves under `edge_model_id`.
/// 边缘端点以 `edge_model_id` 提供的官方模型包。
pub fn find_edge_package(edge_model_id: &str) -> Option<&'static KnownPackage> {
    known_packages()
        .iter()
        .find(|package| package.edge_model_id.as_deref() == Some(edge_model_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_list_parses_and_identifies_its_checkpoints() {
        assert!(!known_packages().is_empty());
        for package in known_packages() {
            assert_eq!(known_package(&package.id), Some(package));
            for checkpoint in &package.checkpoints {
                assert_eq!(find_known_package(checkpoint), Some(package));
            }
        }
        assert_eq!(find_known_package(&"0".repeat(64)), None);
        for package in known_packages() {
            if let Some(model) = &package.edge_model_id {
                assert_eq!(find_edge_package(model), Some(package));
            }
        }
        assert_eq!(find_edge_package("unknown-model"), None);
    }

    #[test]
    fn list_rejects_repeated_checkpoints_or_edge_models_and_unknown_fields() {
        let variants = r#"[{"format":"fp32","devices":["cpu"],"download":null}]"#;
        let package = |id: &str, digest: char, edge: &str| {
            format!(
                r#"{{"id":"{id}","display_name":"{id}","checkpoints":["{}"],"edge_model_id":{edge},"variants":{variants}}}"#,
                digest.to_string().repeat(64)
            )
        };
        let list = |packages: &[String]| {
            format!(
                r#"{{"schema_version":3,"packages":[{}]}}"#,
                packages.join(",")
            )
        };
        let distinct = list(&[
            package("a", 'a', r#""shell-judge""#),
            package("b", 'b', "null"),
        ]);
        let parsed = parse_known_packages(&distinct).unwrap();
        assert_eq!(parsed[0].edge_model_id.as_deref(), Some("shell-judge"));
        assert_eq!(parsed[1].edge_model_id, None);
        for broken in [
            list(&[package("a", 'a', "null"), package("b", 'a', "null")]),
            list(&[package("a", 'a', r#""m""#), package("b", 'b', r#""m""#)]),
            list(&[package("a", 'a', r#""shell judge""#)]),
            list(&[package("a", 'a', r#""""#)]),
            distinct.replace(r#""variants""#, r#""url":"x","variants""#),
            distinct.replace(r#""schema_version":3"#, r#""schema_version":2"#),
        ] {
            assert!(parse_known_packages(&broken).is_err(), "{broken}");
        }
    }

    #[test]
    fn downloads_must_be_pinned_https_directories() {
        let digest = "a".repeat(64);
        let list = |download: &str| {
            format!(
                r#"{{"schema_version":3,"packages":[{{"id":"a","display_name":"A",
                "checkpoints":["{digest}"],"edge_model_id":null,
                "variants":[{{"format":"fp32","devices":["cpu"],"download":{download}}}]}}]}}"#
            )
        };
        let pinned = format!(
            r#"{{"base_url":"https://models.example/a/fp32/","manifest_sha256":"{digest}","total_bytes":1}}"#
        );
        assert!(parse_known_packages(&list(&pinned)).is_ok());
        for broken in [
            pinned.replace("https://", "http://"),
            pinned.replace("fp32/\"", "fp32\""),
            pinned.replace(&digest, "abc"),
            pinned.replace("\"total_bytes\":1", "\"total_bytes\":0"),
        ] {
            assert!(parse_known_packages(&list(&broken)).is_err(), "{broken}");
        }
    }

    // Each device gets the format it runs best among those verified on it; old CUDA GPUs get full precision.
    // 每种设备得到其上已验证格式中运行效果最好的一种；旧 CUDA 显卡得到全精度。
    #[test]
    fn recommendation_follows_the_device_and_its_half_precision_support() {
        let variant = |format, devices: &[JudgeDevice]| KnownVariant {
            format,
            devices: devices.to_vec(),
            download: None,
        };
        let all = [JudgeDevice::Cuda, JudgeDevice::WebGpu, JudgeDevice::Cpu];
        let package = KnownPackage {
            id: String::from("a"),
            display_name: String::from("A"),
            checkpoints: Vec::new(),
            edge_model_id: None,
            variants: vec![
                variant(ModelFormat::Fp16Mixed, &all),
                variant(ModelFormat::Fp32, &all),
            ],
        };
        let pick = |device, capability| {
            package
                .recommended_variant(device, capability)
                .map(|variant| variant.format)
        };
        assert_eq!(
            pick(JudgeDevice::Cuda, Some((8, 9))),
            Some(ModelFormat::Fp16Mixed)
        );
        assert_eq!(pick(JudgeDevice::Cuda, None), Some(ModelFormat::Fp16Mixed));
        assert_eq!(
            pick(JudgeDevice::Cuda, Some((5, 2))),
            Some(ModelFormat::Fp32)
        );
        assert_eq!(
            pick(JudgeDevice::WebGpu, None),
            Some(ModelFormat::Fp16Mixed)
        );
        assert_eq!(pick(JudgeDevice::Cpu, None), Some(ModelFormat::Fp32));

        let gpu_only = KnownPackage {
            variants: vec![variant(ModelFormat::Fp16Mixed, &[JudgeDevice::Cuda])],
            ..package.clone()
        };
        assert_eq!(
            gpu_only
                .recommended_variant(JudgeDevice::Cpu, None)
                .map(|variant| variant.format),
            None
        );
        assert_eq!(
            ModelFormat::from_precision("fp16_projections_fp32_attention"),
            Some(ModelFormat::Fp16Mixed)
        );
        assert_eq!(ModelFormat::from_precision("fp32"), Some(ModelFormat::Fp32));
        assert_eq!(ModelFormat::from_precision("int4"), None);
    }
}
