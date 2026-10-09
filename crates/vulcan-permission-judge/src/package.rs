//! Verified loading of one local judge model package.
//! 本机判定模型包的验证加载。
//!
//! A package is a directory exported by the training side. `MANIFEST.json` lists every file with
//! its size and SHA-256; `runtime-config.json` holds the tensor and decision limits; the policy and
//! tokenizer live under `assets/`. The supported layout is fixed here, not declared by the package:
//! an unknown layout is refused instead of guessed.
//! 模型包是训练端导出的目录。`MANIFEST.json` 列出每个文件的大小与 SHA-256；`runtime-config.json`
//! 保存张量与判定上限；策略与分词器位于 `assets/`。支持的布局在此固定，而非由模型包声明：
//! 未知布局直接拒绝，不做猜测。

use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::decision::JudgeLabel;
use crate::policy::JudgePolicy;

/// File listing every package file with its size and digest.
/// 列出每个模型包文件及其大小与摘要的文件。
pub const MANIFEST_FILE: &str = "MANIFEST.json";
/// The only manifest format this build reads.
/// 本构建唯一读取的清单格式。
pub const MANIFEST_FORMAT_VERSION: u32 = 1;
/// Tensor and decision limits.
/// 张量与判定上限。
pub const RUNTIME_CONFIG_FILE: &str = "runtime-config.json";
/// Policy text the weights were trained with.
/// 权重训练所用的策略文本。
pub const POLICY_FILE: &str = "assets/policy-v2.json";
/// Tokenizer of the encoder.
/// 编码器的分词器。
pub const TOKENIZER_FILE: &str = "assets/tokenizer.json";
/// Self-check inputs.
/// 自检输入。
pub const EXAMPLES_FILE: &str = "example-input.json";
/// Expected self-check results.
/// 自检预期结果。
pub const VALIDATION_FILE: &str = "reports/delivery-validation.json";
/// Graph inputs of the supported model family, in feed order.
/// 支持的模型家族的计算图输入，按输入顺序排列。
pub const INPUT_FIELDS: [&str; 5] = [
    "input_ids",
    "attention_mask",
    "marker_pos",
    "marker_mask",
    "qtype",
];

/// Bytes read per hashing step.
/// 每个哈希步骤读取的字节数。
const HASH_CHUNK_BYTES: usize = 1 << 20;

/// Tensor and decision limits declared by the package.
/// 模型包声明的张量与判定上限。
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RuntimeConfig {
    /// Longest complete sequence the model may read; longer calls are not evaluated.
    /// 模型可读取的最长完整序列；更长的调用不评估。
    pub max_tokens: u32,
    /// Token budget of the policy head including the option markers.
    /// 策略头（含选项标记）的词元预算。
    pub head_tokens: u32,
    /// Token cap of each criterion text.
    /// 每条判据文本的词元上限。
    pub option_tokens: u32,
    /// Most rows one graph run may take.
    /// 单次计算图运行可接收的最多行数。
    pub max_request_items: u32,
    /// Preferred rows per graph run.
    /// 单次计算图运行的首选行数。
    pub micro_batch: u32,
    /// Logit lead allow needs over the best other label.
    /// allow 需要领先其他最高分的分数差。
    pub allow_margin: f32,
    /// Labels in logit order.
    /// 按分数顺序排列的标签。
    pub labels: Vec<String>,
    /// Graph input names.
    /// 计算图输入名称。
    pub input_fields: Vec<String>,
    /// Graph file relative to the package root.
    /// 相对模型包根目录的计算图文件。
    pub model_file: String,
    /// Weight precision description.
    /// 权重精度说明。
    pub precision: String,
    /// GPU memory cap for the CUDA arena.
    /// CUDA 内存池的显存上限。
    pub gpu_memory_limit_mib: u64,
    /// Sequence length above which temporary GPU arenas are released after a run.
    /// 序列长度超过该值时，运行后释放临时显存池。
    pub arena_trim_above_tokens: u32,
}

/// Identity and provenance of a verified package, for registries and audit records.
/// 已验证模型包的身份与来源，用于登记表与审计记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageSummary {
    /// Export status declared by the training side.
    /// 训练端声明的导出状态。
    pub status: String,
    /// Digest of the trained checkpoint the package was converted from.
    /// 模型包转换来源的训练检查点摘要。
    pub parent_sha256: String,
    /// Weight precision description.
    /// 权重精度说明。
    pub precision: String,
    /// Policy revision identifier.
    /// 策略修订标识。
    pub policy_version: String,
    /// Digest of `MANIFEST.json`, which pins every file of the package.
    /// `MANIFEST.json` 的摘要，它固定了模型包的每个文件。
    pub manifest_sha256: String,
    /// Total size of all listed files.
    /// 所有列出文件的总大小。
    pub total_bytes: u64,
}

/// Why a package was refused; each variant names the failing step and file.
/// 模型包被拒绝的原因；每个变体指明失败的步骤与文件。
#[derive(Debug, thiserror::Error)]
pub enum PackageError {
    /// The path is relative, on a network share, missing or not a directory.
    /// 路径为相对路径、位于网络共享、不存在或不是目录。
    #[error("Package path `{path}` is not usable: {reason}.")]
    InvalidPath {
        /// Offending path.
        /// 有问题的路径。
        path: PathBuf,
        /// Reason in English.
        /// 英文原因。
        reason: &'static str,
    },
    /// A file could not be read.
    /// 文件无法读取。
    #[error("Package file `{file}` could not be read: {source}.")]
    Unreadable {
        /// Relative file path.
        /// 相对文件路径。
        file: String,
        /// I/O failure.
        /// I/O 失败。
        source: io::Error,
    },
    /// A file is not valid JSON of the expected shape.
    /// 文件不是预期形状的合法 JSON。
    #[error("Package file `{file}` is malformed: {detail}.")]
    Malformed {
        /// Relative file path.
        /// 相对文件路径。
        file: String,
        /// Parser or consistency detail.
        /// 解析或一致性细节。
        detail: String,
    },
    /// The package uses a layout or contract this build does not support.
    /// 模型包使用本构建不支持的布局或契约。
    #[error("Package is not a supported judge model: {0}.")]
    Unsupported(String),
    /// A file the supported layout needs is not listed in the manifest.
    /// 支持的布局所需的文件未列入清单。
    #[error("Package manifest does not list required file `{0}`.")]
    MissingFile(String),
    /// A manifest entry escapes the package directory.
    /// 清单条目逃逸出模型包目录。
    #[error("Package manifest entry `{0}` is not a plain relative path.")]
    UnsafeEntry(String),
    /// A file differs in size from the manifest.
    /// 文件大小与清单不符。
    #[error("Package file `{file}` has {actual} bytes; the manifest lists {expected}.")]
    SizeMismatch {
        /// Relative file path.
        /// 相对文件路径。
        file: String,
        /// Listed size.
        /// 清单所列大小。
        expected: u64,
        /// Size on disk.
        /// 磁盘上的大小。
        actual: u64,
    },
    /// A file differs in content from the manifest.
    /// 文件内容与清单不符。
    #[error("Package file `{0}` does not match its manifest digest.")]
    DigestMismatch(String),
}

impl PackageError {
    /// Stable code for clients to localize the failure.
    /// 供客户端本地化失败原因的稳定代码。
    pub fn code(&self) -> &'static str {
        match self {
            PackageError::InvalidPath { .. } => "invalid_path",
            PackageError::Unreadable { .. } => "unreadable",
            PackageError::Malformed { .. } => "malformed",
            PackageError::Unsupported(_) => "unsupported",
            PackageError::MissingFile(_) => "missing_file",
            PackageError::UnsafeEntry(_) => "unsafe_entry",
            PackageError::SizeMismatch { .. } => "size_mismatch",
            PackageError::DigestMismatch(_) => "digest_mismatch",
        }
    }
}

/// File identity used to reuse a digest until the file changes.
/// 用于在文件变化前复用摘要的文件身份。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DigestKey {
    /// Absolute file path.
    /// 绝对文件路径。
    pub path: PathBuf,
    /// File size.
    /// 文件大小。
    pub bytes: u64,
    /// Last modification time.
    /// 最后修改时间。
    pub modified: SystemTime,
}

/// Remembers file digests keyed by path, size and modification time.
/// 按路径、大小与修改时间记住文件摘要。
pub trait DigestCache: Send + Sync {
    /// Returns the remembered lowercase hex digest for this exact file state.
    /// 返回该精确文件状态已记住的小写十六进制摘要。
    fn get(&self, key: &DigestKey) -> Option<String>;
    /// Remembers a digest computed for this exact file state.
    /// 记住为该精确文件状态计算的摘要。
    fn put(&self, key: DigestKey, digest: String);
}

/// Process-lifetime digest cache.
/// 进程生命周期内的摘要缓存。
#[derive(Debug, Default)]
pub struct MemoryDigestCache(Mutex<HashMap<DigestKey, String>>);

impl DigestCache for MemoryDigestCache {
    fn get(&self, key: &DigestKey) -> Option<String> {
        self.0.lock().ok()?.get(key).cloned()
    }

    fn put(&self, key: DigestKey, digest: String) {
        if let Ok(mut entries) = self.0.lock() {
            entries.insert(key, digest);
        }
    }
}

/// Manifest fields this build reads.
/// 本构建读取的清单字段。
#[derive(Deserialize)]
struct Manifest {
    format_version: u32,
    status: String,
    parent_sha256: String,
    files: BTreeMap<String, ManifestEntry>,
}

/// Size and digest of one listed file.
/// 一个列出文件的大小与摘要。
#[derive(Deserialize)]
struct ManifestEntry {
    bytes: u64,
    sha256: String,
}

/// A package whose every listed file matched its manifest and whose contract this build supports.
/// 每个列出文件都与清单一致、且契约受本构建支持的模型包。
#[derive(Debug, Clone)]
pub struct JudgePackage {
    root: PathBuf,
    /// Tensor and decision limits.
    /// 张量与判定上限。
    pub config: RuntimeConfig,
    /// Policy the weights were trained with.
    /// 权重训练所用的策略。
    pub policy: JudgePolicy,
    /// Identity and provenance.
    /// 身份与来源。
    pub summary: PackageSummary,
}

impl JudgePackage {
    /// Verifies and opens the package at `root`; no step can be skipped.
    /// 验证并打开 `root` 处的模型包；任何步骤都不能跳过。
    pub fn open(root: &Path, cache: &dyn DigestCache) -> Result<Self, PackageError> {
        check_root(root)?;
        let manifest_bytes = read_file(root, MANIFEST_FILE)?;
        let manifest: Manifest = parse_json(MANIFEST_FILE, &manifest_bytes)?;
        if manifest.format_version != MANIFEST_FORMAT_VERSION {
            return Err(PackageError::Unsupported(format!(
                "manifest format {} (supported: {MANIFEST_FORMAT_VERSION})",
                manifest.format_version
            )));
        }
        for required in [
            RUNTIME_CONFIG_FILE,
            POLICY_FILE,
            TOKENIZER_FILE,
            EXAMPLES_FILE,
            VALIDATION_FILE,
        ] {
            if !manifest.files.contains_key(required) {
                return Err(PackageError::MissingFile(required.to_string()));
            }
        }
        let mut total_bytes = 0;
        for (file, entry) in &manifest.files {
            verify_entry(root, file, entry, cache)?;
            total_bytes += entry.bytes;
        }
        let config: RuntimeConfig =
            parse_json(RUNTIME_CONFIG_FILE, &read_file(root, RUNTIME_CONFIG_FILE)?)?;
        check_config(&config)?;
        if !manifest.files.contains_key(&config.model_file) {
            return Err(PackageError::MissingFile(config.model_file.clone()));
        }
        let policy: JudgePolicy = parse_json(POLICY_FILE, &read_file(root, POLICY_FILE)?)?;
        policy
            .validate()
            .map_err(|detail| PackageError::Malformed {
                file: POLICY_FILE.to_string(),
                detail,
            })?;
        let summary = PackageSummary {
            status: manifest.status,
            parent_sha256: manifest.parent_sha256,
            precision: config.precision.clone(),
            policy_version: policy.version.clone(),
            manifest_sha256: hex::encode(Sha256::digest(&manifest_bytes)),
            total_bytes,
        };
        Ok(Self {
            root: root.to_path_buf(),
            config,
            policy,
            summary,
        })
    }

    /// Package root directory.
    /// 模型包根目录。
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Absolute path of a file in the package.
    /// 模型包内某个文件的绝对路径。
    pub fn file(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }
}

/// Accepts only an existing local directory given as an absolute path.
/// 只接受以绝对路径给出的已存在本地目录。
fn check_root(root: &Path) -> Result<(), PackageError> {
    let invalid = |reason| PackageError::InvalidPath {
        path: root.to_path_buf(),
        reason,
    };
    if !root.is_absolute() {
        return Err(invalid("the path must be absolute"));
    }
    if let Some(Component::Prefix(prefix)) = root.components().next() {
        use std::path::Prefix;
        if !matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_)) {
            return Err(invalid("network and device paths are not accepted"));
        }
    }
    if !root.is_dir() {
        return Err(invalid("the directory does not exist"));
    }
    Ok(())
}

/// Checks the limits and tensor contract of the supported model family.
/// 检查受支持模型家族的上限与张量契约。
fn check_config(config: &RuntimeConfig) -> Result<(), PackageError> {
    let labels: Vec<&str> = JudgeLabel::ORDER.iter().map(|label| label.key()).collect();
    if config.labels != labels {
        return Err(PackageError::Unsupported(format!(
            "labels {:?} (supported: {labels:?})",
            config.labels
        )));
    }
    if config.input_fields != INPUT_FIELDS {
        return Err(PackageError::Unsupported(format!(
            "graph inputs {:?} (supported: {INPUT_FIELDS:?})",
            config.input_fields
        )));
    }
    let consistent = config.head_tokens > 0
        && config.option_tokens > 0
        && config.max_tokens > config.head_tokens
        && config.micro_batch > 0
        && config.max_request_items > 0
        && config.allow_margin.is_finite();
    if !consistent {
        return Err(PackageError::Malformed {
            file: RUNTIME_CONFIG_FILE.to_string(),
            detail: String::from("token limits, batch sizes or the allow margin are inconsistent"),
        });
    }
    Ok(())
}

/// Checks one listed file's path, size and digest.
/// 检查一个列出文件的路径、大小与摘要。
fn verify_entry(
    root: &Path,
    file: &str,
    entry: &ManifestEntry,
    cache: &dyn DigestCache,
) -> Result<(), PackageError> {
    let relative = Path::new(file);
    if file.contains('\\')
        || !relative
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    {
        return Err(PackageError::UnsafeEntry(file.to_string()));
    }
    let path = root.join(relative);
    let unreadable = |source| PackageError::Unreadable {
        file: file.to_string(),
        source,
    };
    let metadata = std::fs::metadata(&path).map_err(unreadable)?;
    if metadata.len() != entry.bytes {
        return Err(PackageError::SizeMismatch {
            file: file.to_string(),
            expected: entry.bytes,
            actual: metadata.len(),
        });
    }
    let key = DigestKey {
        path: path.clone(),
        bytes: metadata.len(),
        modified: metadata.modified().map_err(unreadable)?,
    };
    let digest = match cache.get(&key) {
        Some(digest) => digest,
        None => {
            let digest = hash_file(&path).map_err(unreadable)?;
            cache.put(key, digest.clone());
            digest
        }
    };
    if !digest.eq_ignore_ascii_case(&entry.sha256) {
        return Err(PackageError::DigestMismatch(file.to_string()));
    }
    Ok(())
}

/// Streams one file through SHA-256.
/// 以流方式计算一个文件的 SHA-256。
fn hash_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; HASH_CHUNK_BYTES];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Reads one package file.
/// 读取一个模型包文件。
fn read_file(root: &Path, file: &str) -> Result<Vec<u8>, PackageError> {
    std::fs::read(root.join(file)).map_err(|source| PackageError::Unreadable {
        file: file.to_string(),
        source,
    })
}

/// Parses one JSON package file.
/// 解析一个 JSON 模型包文件。
fn parse_json<T: serde::de::DeserializeOwned>(file: &str, bytes: &[u8]) -> Result<T, PackageError> {
    serde_json::from_slice(bytes).map_err(|error| PackageError::Malformed {
        file: file.to_string(),
        detail: error.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::tests::POLICY;
    use serde_json::json;

    /// Writes a synthetic package whose manifest matches every file it lists.
    /// 写出一个合成模型包，其清单与列出的每个文件一致。
    fn write_package(root: &Path) {
        let config = json!({
            "max_tokens": 1024, "head_tokens": 64, "option_tokens": 8, "max_request_items": 4,
            "micro_batch": 2, "allow_margin": 1.0, "labels": ["allow", "review", "block"],
            "input_fields": INPUT_FIELDS, "model_file": "models/graph.onnx", "precision": "fp16",
            "gpu_memory_limit_mib": 1024, "arena_trim_above_tokens": 512
        });
        let files: [(&str, Vec<u8>); 6] = [
            (RUNTIME_CONFIG_FILE, serde_json::to_vec(&config).unwrap()),
            (POLICY_FILE, POLICY.as_bytes().to_vec()),
            (TOKENIZER_FILE, b"{}".to_vec()),
            (EXAMPLES_FILE, b"[]".to_vec()),
            (VALIDATION_FILE, b"{}".to_vec()),
            ("models/graph.onnx", vec![7; 3000]),
        ];
        let mut listed = serde_json::Map::new();
        for (name, bytes) in &files {
            let path = root.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, bytes).unwrap();
            listed.insert(
                name.to_string(),
                json!({"bytes": bytes.len(), "sha256": hex::encode(Sha256::digest(bytes))}),
            );
        }
        let manifest = json!({
            "format_version": 1, "status": "candidate", "parent_sha256": "abc", "files": listed
        });
        std::fs::write(
            root.join(MANIFEST_FILE),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn a_consistent_package_opens_and_reuses_cached_digests() {
        let dir = tempfile::tempdir().unwrap();
        write_package(dir.path());
        let cache = MemoryDigestCache::default();
        let package = JudgePackage::open(dir.path(), &cache).unwrap();
        assert_eq!(package.policy.options.len(), 2);
        assert_eq!(package.summary.policy_version, "test-v1");
        assert_eq!(cache.0.lock().unwrap().len(), 6);
        JudgePackage::open(dir.path(), &cache).unwrap();
    }

    #[test]
    fn tampering_and_unsafe_entries_are_refused_by_step() {
        let dir = tempfile::tempdir().unwrap();
        write_package(dir.path());
        // Each step uses a fresh cache so a coarse modification clock cannot hide a rewrite.
        // 每个步骤使用全新缓存，避免粗粒度修改时间掩盖重写。
        let cache = MemoryDigestCache::default;

        std::fs::write(dir.path().join("models/graph.onnx"), vec![8; 3000]).unwrap();
        let error = JudgePackage::open(dir.path(), &cache()).unwrap_err();
        assert_eq!(error.code(), "digest_mismatch");

        std::fs::write(dir.path().join("models/graph.onnx"), vec![7; 10]).unwrap();
        let error = JudgePackage::open(dir.path(), &cache()).unwrap_err();
        assert_eq!(error.code(), "size_mismatch");

        let manifest_path = dir.path().join(MANIFEST_FILE);
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
        manifest["files"]["../escape"] = json!({"bytes": 0, "sha256": ""});
        std::fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        let error = JudgePackage::open(dir.path(), &cache()).unwrap_err();
        assert_eq!(error.code(), "unsafe_entry");

        manifest["format_version"] = json!(2);
        std::fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        let error = JudgePackage::open(dir.path(), &cache()).unwrap_err();
        assert_eq!(error.code(), "unsupported");
    }

    #[test]
    fn relative_and_missing_roots_are_refused() {
        let cache = MemoryDigestCache::default();
        let error = JudgePackage::open(Path::new("relative/package"), &cache).unwrap_err();
        assert_eq!(error.code(), "invalid_path");
        let dir = tempfile::tempdir().unwrap();
        let error = JudgePackage::open(&dir.path().join("absent"), &cache).unwrap_err();
        assert_eq!(error.code(), "invalid_path");
    }
}
