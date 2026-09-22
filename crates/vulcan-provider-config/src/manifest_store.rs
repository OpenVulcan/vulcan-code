//! Local provider-manifest store with atomic activation and startup recovery.
//! 本地供应商清单存储：原子激活与启动恢复。
//!
//! The store owns only the file layer (plan §17.1): system active/previous/staging,
//! custom declarations, quarantine, and the bounded `state.json` sync bookkeeping.
//! It never interprets manifest semantics; callers compile and validate candidates
//! before staging and activate only fully verified snapshots.
//! 本存储只拥有文件层（方案 §17.1）：系统 active/previous/staging、自定义声明、
//! 隔离区与有界 `state.json` 同步簿记。它不解释清单语义；调用方在 staging 前
//! 完成编译与校验，且只激活完全验证的快照。

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};

/// Maximum bytes accepted for one staged manifest candidate.
/// 单个 staged 清单候选接受的最大字节数。
///
/// Derived from the single manifest byte bound in `vulcan-provider-adapters`; never restate it.
/// 由 `vulcan-provider-adapters` 中唯一的清单字节上限派生；不要重写该字面量。
pub const MANIFEST_CANDIDATE_MAX_BYTES: u64 = vulcan_provider_adapters::MAX_MANIFEST_BYTES as u64;
/// Maximum bytes accepted for one standalone custom provider manifest file.
/// 单个独立自定义供应商清单文件接受的最大字节数。
///
/// Derived from the same single manifest byte bound; never restate it.
/// 由同一个唯一的清单字节上限派生；不要重写该字面量。
pub const CUSTOM_MANIFEST_MAX_BYTES: u64 = vulcan_provider_adapters::MAX_MANIFEST_BYTES as u64;
/// Maximum standalone custom provider manifests accepted from one host-owned directory.
/// 单个宿主持有目录允许接受的独立自定义供应商 Manifest 最大数量。
const CUSTOM_MANIFEST_MAX_FILES: usize = 256;
/// Maximum filesystem entries inspected during one manifest-directory scan.
/// 单次 Manifest 目录扫描允许检查的文件系统条目最大数量。
const MANIFEST_DIRECTORY_SCAN_MAX_ENTRIES: usize = 4_096;
/// Maximum bytes accepted for the bounded sync-state file.
/// 有界同步状态文件接受的最大字节数。
const SYNC_STATE_MAX_BYTES: u64 = 64 * 1024;
/// Maximum bytes accepted for one persisted conditional-request header value.
/// 单个持久化条件请求头值允许的最大字节数。
const SYNC_STATE_HEADER_MAX_BYTES: usize = 1_024;
/// Maximum bytes accepted for one independent synchronization-check state file.
/// 单份独立同步检查状态文件接受的最大字节数。
const SYNC_CHECK_STATE_MAX_BYTES: u64 = 16 * 1024;
/// Maximum UTF-8 bytes accepted for one stable synchronization failure kind.
/// 单个稳定同步失败类型接受的最大 UTF-8 字节数。
const SYNC_CHECK_FAILURE_KIND_MAX_BYTES: usize = 128;
/// Maximum quarantined candidates retained for one provider.
/// 单个供应商允许保留的最大隔离候选数量。
const QUARANTINE_MAX_FILES_PER_PROVIDER: usize = 32;
/// Maximum total quarantined bytes retained for one provider.
/// 单个供应商允许保留的最大隔离候选总字节数。
const QUARANTINE_MAX_BYTES_PER_PROVIDER: u64 = 32 * 1024 * 1024;
/// Maximum newest quarantine summaries returned through one status snapshot.
/// 一份状态快照允许返回的最新隔离摘要最大数量。
const QUARANTINE_SUMMARY_MAX_ITEMS: usize = 16;
/// Serializes process-local quarantine publication and pruning into one mutation transaction.
/// 把进程内隔离文件发布与裁剪串行为一个变更事务。
static QUARANTINE_MUTATION_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
/// Closed schema version for the four-file system transaction journal.
/// 四文件系统事务日志的封闭 Schema 版本。
const SYSTEM_TRANSACTION_JOURNAL_SCHEMA_VERSION: u32 = 2;
/// Maximum bytes accepted for one system-manifest transaction journal.
/// 单个系统清单事务日志接受的最大字节数。
///
/// Active and Previous are both hex encoded (four manifest-size units total); sync and check
/// state are also hex encoded. The final fixed allowance covers JSON field names and framing.
/// Active 与 Previous 均使用十六进制编码（合计四个 Manifest 大小单位）；同步与检查状态同样
/// 使用十六进制编码。最后的固定余量覆盖 JSON 字段名与结构开销。
const SYSTEM_TRANSACTION_JOURNAL_MAX_BYTES: u64 = (MANIFEST_CANDIDATE_MAX_BYTES * 4)
    + (SYNC_STATE_MAX_BYTES * 2)
    + (SYNC_CHECK_STATE_MAX_BYTES * 2)
    + (64 * 1024);

/// Owns the resolved provider-manifest directory layout under one host-owned root.
/// 持有宿主解析根目录下的供应商清单目录布局。
#[derive(Debug, Clone)]
pub struct ManifestStoreLayout {
    /// Host-resolved application data root; never hard-coded (plan §17.1).
    /// 宿主解析的应用数据根目录；绝不硬编码（方案 §17.1）。
    root: PathBuf,
}

/// Bounded non-secret sync bookkeeping for one provider.
/// 一个供应商的有界非秘密同步簿记。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestSyncState {
    /// Active manifest revision.
    /// 活动清单修订号。
    pub revision: u64,
    /// SHA-256 hex digest of the active bytes.
    /// 活动字节的 SHA-256 十六进制摘要。
    pub sha256: String,
    /// Previously active manifest identity retained for authenticated rollback and recovery.
    /// 为可信回滚与恢复保留的上一活动清单身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous: Option<ManifestVersionState>,
    /// Optional remote ETag used for conditional requests.
    /// 用于条件请求的可选远端 ETag。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    /// Optional remote index ETag used for conditional index requests; the index is shared
    /// across providers but each per-provider transaction records the etag it last observed.
    /// 用于条件 Index 请求的可选远端 Index ETag；Index 在供应商间共享，但每个逐供应商
    /// 事务记录其最近一次观察到的 ETag。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index_etag: Option<String>,
    /// Optional remote Last-Modified used for conditional requests.
    /// 用于条件请求的可选远端 Last-Modified。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
    /// Last successful synchronization unix milliseconds.
    /// 最近一次成功同步的 Unix 毫秒时间。
    pub synced_at_unix_ms: u64,
    /// Classified failure kind of the last rejected candidate.
    /// 最近一次被拒候选的分类失败类型。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error_kind: Option<String>,
}

/// Closed result of one configured remote manifest check.
/// 一次已配置远端 Manifest 检查的封闭结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManifestCheckOutcome {
    /// The signed index did not pin this provider.
    /// 签名 Index 未固定此供应商。
    NotPinned,
    /// The authenticated active version already matched the signed candidate.
    /// 已认证 active 版本已与签名候选一致。
    UpToDate,
    /// A candidate completed activation and every externally visible projection.
    /// 候选已完成激活及全部外部可见投影。
    Activated,
    /// A provider-scoped candidate was rejected with a stable classification.
    /// 供应商作用域候选以稳定分类被拒绝。
    Rejected,
    /// The configured index, network, projection, or status transaction failed.
    /// 已配置 Index、网络、投影或状态事务失败。
    Failed,
}

/// Bounded non-secret bookkeeping for the latest configured remote check.
/// 最近一次已配置远端检查的有界非秘密簿记。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestCheckState {
    /// Unix milliseconds when the most recent configured check finished.
    /// 最近一次已配置检查完成时的 Unix 毫秒时间。
    pub checked_at_unix_ms: u64,
    /// Unix milliseconds when a candidate last completed activation and projection.
    /// 候选最近一次完成激活与投影时的 Unix 毫秒时间。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_successful_at_unix_ms: Option<u64>,
    /// Closed result of the most recent configured check.
    /// 最近一次已配置检查的封闭结果。
    pub outcome: ManifestCheckOutcome,
    /// Stable classified failure kind for rejected or failed checks.
    /// 被拒绝或失败检查的稳定分类失败类型。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error_kind: Option<String>,
}

/// One parsed check-state file or explicit absence.
/// 一份已解析检查状态文件或明确缺失状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckStateLoad {
    /// No configured remote check has completed for this provider.
    /// 此供应商尚未完成任何已配置远端检查。
    Absent,
    /// The parsed bounded check state.
    /// 已解析的有界检查状态。
    Present(ManifestCheckState),
}

/// Exact before-image of one independent synchronization-check state file.
/// 一份独立同步检查状态文件的精确前镜像。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestCheckStateSnapshot {
    /// Exact prior file bytes, or absence when no check state existed.
    /// 先前文件的精确字节；检查状态不存在时为空。
    bytes: Option<Vec<u8>>,
}

/// Content-addressed identity of one locally stored manifest version.
/// 一份本地存储清单版本的内容寻址身份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestVersionState {
    /// Manifest revision parsed from the signed document.
    /// 从签名文档解析出的清单修订号。
    pub revision: u64,
    /// SHA-256 digest of the exact authenticated bytes.
    /// 精确已认证字节的 SHA-256 摘要。
    pub sha256: String,
}

/// Exact byte snapshot of the four files that form one system-manifest store transaction.
/// 构成一个系统清单存储事务的四份文件的精确字节快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemManifestStoreSnapshot {
    /// Active bytes before the transaction, or absence for a first activation.
    /// 事务前的 Active 字节；首次激活时不存在。
    active: Option<Vec<u8>>,
    /// Previous bytes before the transaction.
    /// 事务前的 Previous 字节。
    previous: Option<Vec<u8>>,
    /// Raw sync-state bytes before the transaction.
    /// 事务前的原始同步状态字节。
    sync_state: Option<Vec<u8>>,
    /// Raw operator-visible check-state bytes before the transaction.
    /// 事务前面向操作员的原始检查状态字节。
    check_state: Option<Vec<u8>>,
}

/// Durable phase of one four-file system-manifest transaction.
/// 一个四文件系统清单事务的持久化阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SystemManifestTransactionPhase {
    /// File replacement may be incomplete and startup must restore the before-image.
    /// 文件替换可能尚未完成，启动时必须恢复前镜像。
    Applying,
    /// The Active, Previous, sync-state, and check-state files form one committed generation.
    /// Active、Previous、同步状态与检查状态文件已经构成一个提交代际。
    Committed,
}

/// Hex-encoded before-image retained until a system-manifest transaction commits.
/// 系统清单事务提交前保留的十六进制前镜像。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EncodedSystemManifestStoreSnapshot {
    /// Optional Active bytes encoded as lowercase hexadecimal.
    /// 可选 Active 字节的小写十六进制编码。
    active_hex: Option<String>,
    /// Optional Previous bytes encoded as lowercase hexadecimal.
    /// 可选 Previous 字节的小写十六进制编码。
    previous_hex: Option<String>,
    /// Optional raw sync-state bytes encoded as lowercase hexadecimal.
    /// 可选原始同步状态字节的小写十六进制编码。
    sync_state_hex: Option<String>,
    /// Optional raw check-state bytes encoded as lowercase hexadecimal.
    /// 可选原始检查状态字节的小写十六进制编码。
    check_state_hex: Option<String>,
}

/// Recoverable journal for one system-manifest store transaction.
/// 一个系统清单存储事务的可恢复日志。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SystemManifestTransactionJournal {
    /// Closed journal schema version.
    /// 封闭的日志 Schema 版本。
    schema_version: u32,
    /// Exact system provider owning the transaction.
    /// 拥有该事务的精确系统供应商。
    provider_id: String,
    /// Durable transaction phase.
    /// 持久化事务阶段。
    phase: SystemManifestTransactionPhase,
    /// Exact before-image used only when the phase is applying.
    /// 仅在 applying 阶段使用的精确前镜像。
    before: EncodedSystemManifestStoreSnapshot,
}

impl EncodedSystemManifestStoreSnapshot {
    /// Encodes one exact store snapshot into a compact bounded JSON representation.
    /// 将一份精确存储快照编码为紧凑有界的 JSON 表示。
    ///
    /// # Parameters
    /// 参数
    /// `snapshot` contains the four optional raw file images.
    /// `snapshot` 包含四份可选原始文件镜像。
    ///
    /// # Returns
    /// 返回值
    /// A lowercase-hex representation without lossy text conversion.
    /// 返回无损文本转换的小写十六进制表示。
    fn encode(snapshot: &SystemManifestStoreSnapshot) -> Self {
        Self {
            active_hex: snapshot.active.as_deref().map(hex::encode),
            previous_hex: snapshot.previous.as_deref().map(hex::encode),
            sync_state_hex: snapshot.sync_state.as_deref().map(hex::encode),
            check_state_hex: snapshot.check_state.as_deref().map(hex::encode),
        }
    }

    /// Decodes a durable journal before-image back into exact raw file bytes.
    /// 将持久化日志前镜像解码回精确原始文件字节。
    ///
    /// # Returns
    /// 返回值
    /// The exact store snapshot or a controlled malformed-hex failure.
    /// 返回精确存储快照，或受控的畸形十六进制错误。
    fn decode(&self) -> Result<SystemManifestStoreSnapshot, String> {
        let decode = |label: &str, value: &Option<String>| -> Result<Option<Vec<u8>>, String> {
            value
                .as_deref()
                .map(|value| {
                    hex::decode(value).map_err(|error| {
                        format!(
                            "System manifest transaction journal contains invalid {label} hex: {error}"
                        )
                    })
                })
                .transpose()
        };
        Ok(SystemManifestStoreSnapshot {
            active: decode("Active", &self.active_hex)?,
            previous: decode("Previous", &self.previous_hex)?,
            sync_state: decode("sync-state", &self.sync_state_hex)?,
            check_state: decode("check-state", &self.check_state_hex)?,
        })
    }
}

/// One parsed sync-state file or an explicit bounded-format failure.
/// 一份已解析同步状态文件，或明确的格式/边界失败。
pub enum SyncStateLoad {
    /// No sync state has ever been written.
    /// 从未写入过同步状态。
    Absent,
    /// The parsed bounded state.
    /// 已解析的有界状态。
    Present(ManifestSyncState),
}

/// One classified quarantined manifest candidate for a provider.
/// 一个供应商的一条分类隔离清单候选。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuarantineSummary {
    /// Candidate revision that failed verification.
    /// 未通过校验的候选修订号。
    pub revision: u64,
    /// Classified rejection kind recorded by the sync transaction.
    /// 同步事务记录的分类拒绝类型。
    pub failure_kind: String,
    /// Best-effort quarantine write time; zero when the file predates time tracking.
    /// 尽力而为的隔离写入时间；文件早于时间跟踪时为零。
    pub quarantined_at_unix_ms: u64,
}

impl ManifestStoreLayout {
    /// Creates the layout rooted at one host-resolved application data directory.
    /// 以宿主解析的应用数据目录为根创建布局。
    ///
    /// # Parameters
    /// 参数
    /// `root` is the host-owned application data directory; the store creates no directory
    /// until a write operation actually needs it.
    /// `root` 是宿主持有的应用数据目录；存储只在写入操作真正需要时才创建目录。
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }

    /// Returns the provider-manifests root.
    /// 返回 provider-manifests 根目录。
    pub fn manifests_root(&self) -> PathBuf {
        self.root.join("provider-manifests")
    }

    /// Returns the system active directory.
    /// 返回系统 active 目录。
    pub fn system_active_dir(&self) -> PathBuf {
        self.manifests_root().join("system").join("active")
    }

    /// Returns the system previous directory.
    /// 返回系统 previous 目录。
    pub fn system_previous_dir(&self) -> PathBuf {
        self.manifests_root().join("system").join("previous")
    }

    /// Returns the system staging directory.
    /// 返回系统 staging 目录。
    pub fn system_staging_dir(&self) -> PathBuf {
        self.manifests_root().join("system").join("staging")
    }

    /// Returns the quarantine directory.
    /// 返回隔离区目录。
    pub fn quarantine_dir(&self) -> PathBuf {
        self.manifests_root().join("quarantine")
    }

    /// Returns the standalone custom provider manifest directory.
    /// 返回独立自定义供应商清单目录。
    ///
    /// Every `custom:` provider owns exactly one JSON file below this directory; the file
    /// name is a reversible percent-encoding of the provider identifier (plan §R6.1) so
    /// identifiers never collide with filesystem separators or reserved characters.
    /// 每个 `custom:` 供应商在此目录下独占一份 JSON 文件；文件名为供应商标识的可逆
    /// 百分号编码（方案 §R6.1），标识不会与文件系统分隔符或保留字符冲突。
    pub fn custom_manifests_dir(&self) -> PathBuf {
        self.manifests_root().join("custom")
    }

    /// Returns the standalone custom manifest path for one custom provider.
    /// 返回一个自定义供应商的独立清单路径。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` must use the `custom:` namespace; the encoded stem is filesystem-safe
    /// on every supported platform.
    /// `provider_id` 必须使用 `custom:` 命名空间；编码后的文件名在所有支持平台上均安全。
    pub fn custom_manifest_path(&self, provider_id: &str) -> PathBuf {
        self.custom_manifests_dir().join(format!(
            "{}.json",
            encode_custom_provider_file_stem(provider_id)
        ))
    }

    /// Atomically writes one standalone custom provider manifest file.
    /// 原子写入一份独立自定义供应商清单文件。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` selects the exact `custom:` provider and `bytes` are the exact manifest bytes.
    /// `provider_id` 选择精确 `custom:` 供应商，`bytes` 是精确清单字节。
    ///
    /// # Returns
    /// 返回值
    /// Success after the bounded bytes are published atomically, or a controlled failure.
    /// 有界字节原子发布后返回成功，或返回受控失败。
    pub fn write_custom_manifest(&self, provider_id: &str, bytes: &[u8]) -> Result<(), String> {
        vulcan_provider_adapters::validate_custom_provider_identifier(provider_id)?;
        if bytes.len() as u64 > CUSTOM_MANIFEST_MAX_BYTES {
            return Err(format!(
                "Custom manifest for `{provider_id}` exceeds the {CUSTOM_MANIFEST_MAX_BYTES} byte bound."
            ));
        }
        let path = self.custom_manifest_path(provider_id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "Failed to create custom manifest directory `{}`: {error}",
                    parent.display()
                )
            })?;
        }
        crate::atomic_file::write_file_atomically(&path, bytes)
    }

    /// Reads one bounded standalone custom provider manifest file.
    /// 读取一份有界独立自定义供应商清单文件。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` selects the exact `custom:` provider.
    /// `provider_id` 选择精确 `custom:` 供应商。
    ///
    /// # Returns
    /// 返回值
    /// The exact manifest text, `None` when the file is absent, or a controlled failure
    /// when the file exceeds the byte bound or cannot be read.
    /// 返回精确清单文本；文件缺失返回 `None`；文件超过字节上限或无法读取时返回受控失败。
    pub fn read_custom_manifest(&self, provider_id: &str) -> Result<Option<String>, String> {
        vulcan_provider_adapters::validate_custom_provider_identifier(provider_id)?;
        let path = self.custom_manifest_path(provider_id);
        let Some(bytes) =
            read_optional_bounded_bytes(&path, CUSTOM_MANIFEST_MAX_BYTES, "custom manifest")?
        else {
            return Ok(None);
        };
        String::from_utf8(bytes).map(Some).map_err(|error| {
            format!("Custom manifest for `{provider_id}` is not valid UTF-8: {error}")
        })
    }

    /// Lists every standalone custom provider identifier present in the custom directory.
    /// 列出自定义目录中存在的全部独立自定义供应商标识。
    ///
    /// # Returns
    /// 返回值
    /// The exact `custom:` identifiers, or a controlled directory failure.
    /// 返回精确的 `custom:` 标识，或受控目录失败。
    pub fn list_custom_manifest_ids(&self) -> Result<Vec<String>, String> {
        let directory = self.custom_manifests_dir();
        if !path_exists(&directory, "custom manifest directory")? {
            return Ok(Vec::new());
        }
        let mut identifiers = Vec::new();
        // scanned_entries closes the directory-enumeration resource boundary even for junk names.
        // scanned_entries 即使面对无效文件名也会封闭目录枚举资源边界。
        let mut scanned_entries = 0usize;
        for entry in std::fs::read_dir(&directory).map_err(|error| {
            format!(
                "Failed to list custom manifest directory `{}`: {error}",
                directory.display()
            )
        })? {
            scanned_entries = scanned_entries.saturating_add(1);
            if scanned_entries > MANIFEST_DIRECTORY_SCAN_MAX_ENTRIES {
                return Err(format!(
                    "Custom manifest directory `{}` exceeds the {MANIFEST_DIRECTORY_SCAN_MAX_ENTRIES}-entry scan limit.",
                    directory.display()
                ));
            }
            let entry = entry.map_err(|error| {
                format!(
                    "Failed to read custom manifest directory entry in `{}`: {error}",
                    directory.display()
                )
            })?;
            let file_name = entry.file_name();
            let Some(stem) = file_name
                .to_str()
                .and_then(|name| name.strip_suffix(".json"))
            else {
                continue;
            };
            if let Some(provider_id) = decode_custom_provider_file_stem(stem)
                && vulcan_provider_adapters::validate_custom_provider_identifier(
                    provider_id.as_str(),
                )
                .is_ok()
            {
                if identifiers.len() >= CUSTOM_MANIFEST_MAX_FILES {
                    return Err(format!(
                        "Custom manifest directory `{}` exceeds the {CUSTOM_MANIFEST_MAX_FILES}-provider limit.",
                        directory.display()
                    ));
                }
                identifiers.push(provider_id);
            }
        }
        identifiers.sort();
        Ok(identifiers)
    }

    /// Removes one standalone custom provider manifest file.
    /// 删除一份独立自定义供应商清单文件。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` selects the exact `custom:` provider.
    /// `provider_id` 选择精确 `custom:` 供应商。
    ///
    /// # Returns
    /// 返回值
    /// Success when the file is absent or removed, or a controlled failure.
    /// 文件缺失或已删除时返回成功，或返回受控失败。
    pub fn remove_custom_manifest(&self, provider_id: &str) -> Result<(), String> {
        vulcan_provider_adapters::validate_custom_provider_identifier(provider_id)?;
        let path = self.custom_manifest_path(provider_id);
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!(
                "Failed to remove custom manifest `{}`: {error}",
                path.display()
            )),
        }
    }

    /// Returns the per-provider bounded sync-state file.
    /// 返回逐供应商的有界同步状态文件。
    ///
    /// Sync bookkeeping is transaction-scoped per provider so one provider's activation and
    /// rollback never shares state with another (plan §17.3).
    /// 同步簿记按供应商独立事务，一个供应商的激活与回滚绝不与其他供应商共享状态（方案 §17.3）。
    pub fn sync_state_path(&self, provider_id: &str) -> PathBuf {
        self.manifests_root()
            .join("system")
            .join("state")
            .join(format!(
                "{}.json",
                encode_system_provider_file_stem(provider_id)
            ))
    }

    /// Returns the independent per-provider synchronization-check state path.
    /// 返回独立的逐供应商同步检查状态路径。
    ///
    /// The provider identifier is encoded before it becomes a file name so this observability
    /// record can never escape the host-owned check-state directory.
    /// 供应商标识在成为文件名之前会被编码，使该可观测性记录绝不可能逸出宿主持有的
    /// 检查状态目录。
    pub fn sync_check_state_path(&self, provider_id: &str) -> PathBuf {
        self.manifests_root()
            .join("system")
            .join("check-state")
            .join(format!(
                "{}.json",
                encode_custom_provider_file_stem(provider_id)
            ))
    }

    /// Returns the recoverable transaction-journal path for one system provider.
    /// 返回一个系统供应商的可恢复事务日志路径。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` identifies the exact provider transaction.
    /// `provider_id` 标识精确的供应商事务。
    ///
    /// # Returns
    /// 返回值
    /// A filesystem-safe journal path below the host-owned manifest root.
    /// 返回宿主清单根目录下的文件系统安全日志路径。
    fn system_transaction_journal_path(&self, provider_id: &str) -> PathBuf {
        self.manifests_root()
            .join("system")
            .join("transactions")
            .join(format!(
                "{}.json",
                encode_custom_provider_file_stem(provider_id)
            ))
    }

    /// Returns the active path for one system provider.
    /// 返回一个系统供应商的 active 路径。
    pub fn active_path(&self, provider_id: &str) -> PathBuf {
        self.system_active_dir().join(format!(
            "{}.json",
            encode_system_provider_file_stem(provider_id)
        ))
    }

    /// Returns the previous path for one system provider.
    /// 返回一个系统供应商的 previous 路径。
    pub fn previous_path(&self, provider_id: &str) -> PathBuf {
        self.system_previous_dir().join(format!(
            "{}.json",
            encode_system_provider_file_stem(provider_id)
        ))
    }

    /// Returns the staging path for one candidate.
    /// 返回一个候选的 staging 路径。
    pub fn staging_path(&self, provider_id: &str, revision: u64) -> PathBuf {
        self.system_staging_dir().join(format!(
            "{}.{revision}.json",
            encode_system_provider_file_stem(provider_id)
        ))
    }

    /// Returns the quarantine path for one rejected candidate.
    /// 返回一个被拒候选的隔离路径。
    pub fn quarantine_path(&self, provider_id: &str, revision: u64, failure_kind: &str) -> PathBuf {
        self.quarantine_dir().join(format!(
            "{}.{revision}.{failure_kind}.json",
            encode_system_provider_file_stem(provider_id)
        ))
    }

    /// Writes one staged candidate atomically and returns its staging path.
    /// 原子写入一个 staged 候选并返回其 staging 路径。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` and `revision` identify the candidate; `bytes` are the exact candidate bytes.
    /// `provider_id` 与 `revision` 标识候选；`bytes` 是精确候选字节。
    ///
    /// # Returns
    /// 返回值
    /// The staging path, or a controlled filesystem failure.
    /// 返回 staging 路径，或受控文件系统失败。
    pub fn stage_candidate(
        &self,
        provider_id: &str,
        revision: u64,
        bytes: &[u8],
    ) -> Result<PathBuf, String> {
        if bytes.len() as u64 > MANIFEST_CANDIDATE_MAX_BYTES {
            return Err(format!(
                "Manifest candidate for `{provider_id}` exceeds the {MANIFEST_CANDIDATE_MAX_BYTES} byte bound."
            ));
        }
        let staging = self.staging_path(provider_id, revision);
        if let Some(parent) = staging.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "Failed to create manifest staging directory `{}`: {error}",
                    parent.display()
                )
            })?;
        }
        // One globally serialized provider synchronization may retain only its newest candidate;
        // crash leftovers and pre-activation failures must not accumulate by revision forever.
        // 全局串行化的供应商同步仅可保留其最新候选；崩溃残留与激活前失败不得按修订永久累积。
        self.prune_staged_candidates(provider_id, revision)?;
        crate::atomic_file::write_file_atomically(&staging, bytes)?;
        Ok(staging)
    }

    /// Removes obsolete staged revisions for one exact system provider.
    /// 移除一个精确系统供应商的过期暂存修订。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` owns the staged partition and `retained_revision` is the only revision that
    /// may remain before the next atomic write.
    /// `provider_id` 持有暂存分区，`retained_revision` 是下次原子写入前唯一可保留的修订。
    ///
    /// # Returns
    /// 返回值
    /// Success after a bounded directory scan and exact regular-file cleanup, or a controlled
    /// type, enumeration, or removal failure.
    /// 在有界目录扫描与精确常规文件清理后返回成功，或返回受控的类型、枚举、删除失败。
    fn prune_staged_candidates(
        &self,
        provider_id: &str,
        retained_revision: u64,
    ) -> Result<(), String> {
        let directory = self.system_staging_dir();
        let provider_file_stem = encode_system_provider_file_stem(provider_id);
        let mut scanned_entries = 0_usize;
        for entry in std::fs::read_dir(&directory).map_err(|error| {
            format!(
                "Failed to enumerate manifest staging directory `{}`: {error}",
                directory.display()
            )
        })? {
            scanned_entries = scanned_entries.saturating_add(1);
            if scanned_entries > MANIFEST_DIRECTORY_SCAN_MAX_ENTRIES {
                return Err(format!(
                    "Manifest staging directory `{}` exceeds the {} entry scan bound.",
                    directory.display(),
                    MANIFEST_DIRECTORY_SCAN_MAX_ENTRIES
                ));
            }
            let entry = entry.map_err(|error| {
                format!(
                    "Failed to enumerate an entry in manifest staging directory `{}`: {error}",
                    directory.display()
                )
            })?;
            let file_type = entry.file_type().map_err(|error| {
                format!(
                    "Failed to inspect staged manifest `{}`: {error}",
                    entry.path().display()
                )
            })?;
            if !file_type.is_file() {
                continue;
            }
            let file_name = entry.file_name().to_string_lossy().into_owned();
            let Some((entry_provider_file_stem, entry_revision)) =
                staged_candidate_identity(file_name.as_str())
            else {
                continue;
            };
            if entry_provider_file_stem == provider_file_stem && entry_revision != retained_revision
            {
                std::fs::remove_file(entry.path()).map_err(|error| {
                    format!(
                        "Failed to remove obsolete staged manifest `{}`: {error}",
                        entry.path().display()
                    )
                })?;
            }
        }
        Ok(())
    }

    /// Atomically activates one staged candidate: the current active file becomes previous and
    /// the staged file becomes active. On failure the previous file is restored as active.
    /// 原子激活一个 staged 候选：当前 active 文件变为 previous，staged 文件变为 active；
    /// 失败时 previous 文件恢复为 active。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` identifies the exact provider and `revision` selects the staged file.
    /// `provider_id` 标识精确供应商，`revision` 选择 staged 文件。
    pub fn activate(&self, provider_id: &str, revision: u64) -> Result<(), String> {
        let active = self.active_path(provider_id);
        let previous = self.previous_path(provider_id);
        let staged = self.staging_path(provider_id, revision);
        if let Some(parent) = previous.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "Failed to create previous directory `{}`: {error}",
                    parent.display()
                )
            })?;
        }
        let Some(staged_bytes) =
            read_optional_bounded_bytes(&staged, MANIFEST_CANDIDATE_MAX_BYTES, "staged manifest")?
        else {
            return Err(format!(
                "Staged manifest `{}` is missing before activation.",
                staged.display()
            ));
        };
        // Move the current active file into previous; the rename fails cleanly when absent.
        // On Windows an existing previous target makes rename fail, so the superseded
        // previous layer is removed first inside the transaction (plan §17.1).
        // 将当前 active 文件移入 previous；文件缺失时重命名干净失败。
        // Windows 上已存在的 previous 目标会使重命名失败，因此事务内先移除被取代的
        // previous 层（方案 §17.1）。
        let had_active = path_exists(&active, "active manifest")?;
        if had_active {
            if path_exists(&previous, "previous manifest")? {
                std::fs::remove_file(&previous).map_err(|error| {
                    format!(
                        "Failed to remove superseded previous manifest `{}`: {error}",
                        previous.display()
                    )
                })?;
            }
            std::fs::rename(&active, &previous).map_err(|error| {
                format!(
                    "Failed to move active manifest `{}` to previous: {error}",
                    active.display()
                )
            })?;
        }
        // Publish the staged bytes as active through the same atomic writer; a failed restore
        // of the previous layer is surfaced so operators never see a silent active gap.
        // 通过同一原子写入器发布 staged 字节为 active；previous 层恢复失败会被显式暴露，
        // 操作者绝不会看到静默的 active 空洞。
        if let Err(error) = crate::atomic_file::write_file_atomically(&active, &staged_bytes) {
            let mut message = format!("Failed to activate staged manifest: {error}");
            if had_active && let Err(restore_error) = std::fs::rename(&previous, &active) {
                message.push_str(&format!(
                    "; additionally failed to restore the previous layer: {restore_error}"
                ));
            }
            return Err(message);
        }
        // The staged file is consumed only after Active is published. Cleanup failure remains a
        // transaction failure so the service-owned before-image can restore authoritative state.
        // 仅在 Active 发布后消费 staged 文件。清理失败仍属于事务失败，使服务持有的前镜像
        // 可以恢复权威状态。
        remove_file_if_present(&staged, "consumed staged manifest")?;
        Ok(())
    }

    /// Restores the previous manifest as active after a failed activation transaction.
    /// 在失败激活事务后将 previous 清单恢复为 active。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` identifies the exact provider.
    /// `provider_id` 标识精确供应商。
    pub fn rollback(&self, provider_id: &str) -> Result<(), String> {
        let active = self.active_path(provider_id);
        let previous = self.previous_path(provider_id);
        if !path_exists(&previous, "previous manifest")? {
            return Err(format!(
                "No previous manifest exists for rollback of `{provider_id}`."
            ));
        }
        let previous_bytes = read_required_bounded_bytes(
            &previous,
            MANIFEST_CANDIDATE_MAX_BYTES,
            "previous manifest",
        )?;
        crate::atomic_file::write_file_atomically(&active, &previous_bytes)?;
        Ok(())
    }

    /// Atomically swaps authenticated Active and Previous bytes for an explicit rollback.
    /// 为显式回滚原子交换已认证的 Active 与 Previous 字节。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` selects the exact system provider whose two retained versions are swapped.
    /// `provider_id` 选择需要交换两个保留版本的精确系统供应商。
    ///
    /// # Returns
    /// 返回值
    /// Success when both files are durably exchanged; a failed second write restores the
    /// original Previous bytes before returning an error.
    /// 两个文件持久交换后成功；第二次写入失败时先恢复原 Previous 字节再返回错误。
    pub fn swap_active_previous(&self, provider_id: &str) -> Result<(), String> {
        let active = self.active_path(provider_id);
        let previous = self.previous_path(provider_id);
        let active_bytes = read_required_bounded_bytes(
            &active,
            MANIFEST_CANDIDATE_MAX_BYTES,
            "active manifest for rollback swap",
        )?;
        let previous_bytes = read_required_bounded_bytes(
            &previous,
            MANIFEST_CANDIDATE_MAX_BYTES,
            "previous manifest for rollback swap",
        )?;
        crate::atomic_file::write_file_atomically(&previous, &active_bytes)?;
        if let Err(error) = crate::atomic_file::write_file_atomically(&active, &previous_bytes) {
            let restore = crate::atomic_file::write_file_atomically(&previous, &previous_bytes);
            return Err(format!(
                "Failed to publish rollback Active bytes: {error}; Previous restore: {restore:?}"
            ));
        }
        Ok(())
    }

    /// Removes one uncommitted Active layer during cross-layer activation rollback.
    /// 在跨层激活回滚期间移除一份未提交 Active 层。
    ///
    /// Missing files are treated as already rolled back.
    /// 文件缺失视为已经完成回滚。
    pub fn remove_active(&self, provider_id: &str) -> Result<(), String> {
        remove_file_if_present(&self.active_path(provider_id), "active manifest")
    }

    /// Removes one unauthenticated Previous layer after startup recovery consumes it.
    /// 在启动恢复消费一份未认证 Previous 层后移除该层。
    ///
    /// Missing files are treated as already removed.
    /// 文件缺失视为已经移除。
    pub fn remove_previous(&self, provider_id: &str) -> Result<(), String> {
        remove_file_if_present(&self.previous_path(provider_id), "previous manifest")
    }

    /// Removes one uncommitted sync-state document during first-activation rollback.
    /// 在首次激活回滚期间移除一份未提交同步状态文档。
    ///
    /// Missing files are treated as already rolled back.
    /// 文件缺失视为已经完成回滚。
    pub fn remove_sync_state(&self, provider_id: &str) -> Result<(), String> {
        remove_file_if_present(&self.sync_state_path(provider_id), "manifest sync state")
    }

    /// Removes one untrusted operator-visible check-state document.
    /// 移除一份不可信的面向操作员检查状态文档。
    ///
    /// Missing files are treated as already removed.
    /// 文件缺失视为已经移除。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` selects the exact provider-local check state.
    /// `provider_id` 选择精确的逐供应商检查状态。
    ///
    /// # Returns
    /// 返回值
    /// Success when the check-state file no longer exists.
    /// 检查状态文件不再存在时返回成功。
    pub fn remove_check_state(&self, provider_id: &str) -> Result<(), String> {
        remove_file_if_present(
            &self.sync_check_state_path(provider_id),
            "manifest check state",
        )
    }

    /// Captures the exact Active, Previous, sync-state, and check-state bytes before a commit.
    /// 在提交前捕获精确的 Active、Previous、同步状态与检查状态字节。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` selects the isolated provider transaction.
    /// `provider_id` 选择隔离的供应商事务。
    ///
    /// # Returns
    /// 返回值
    /// A byte-for-byte recovery snapshot, including file absence.
    /// 返回逐字节恢复快照，并保留文件不存在这一状态。
    pub fn capture_system_snapshot(
        &self,
        provider_id: &str,
    ) -> Result<SystemManifestStoreSnapshot, String> {
        Ok(SystemManifestStoreSnapshot {
            active: read_optional_bounded_bytes(
                &self.active_path(provider_id),
                MANIFEST_CANDIDATE_MAX_BYTES,
                "active manifest",
            )?,
            previous: read_optional_bounded_bytes(
                &self.previous_path(provider_id),
                MANIFEST_CANDIDATE_MAX_BYTES,
                "previous manifest",
            )?,
            sync_state: read_optional_bounded_bytes(
                &self.sync_state_path(provider_id),
                SYNC_STATE_MAX_BYTES,
                "manifest sync state",
            )?,
            check_state: read_optional_bounded_bytes(
                &self.sync_check_state_path(provider_id),
                SYNC_CHECK_STATE_MAX_BYTES,
                "manifest check state",
            )?,
        })
    }

    /// Starts one recoverable system-manifest transaction before any authoritative file changes.
    /// 在任何权威文件变更前启动一个可恢复的系统清单事务。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` owns the transaction and `snapshot` is the exact before-image.
    /// `provider_id` 拥有该事务，`snapshot` 是精确前镜像。
    ///
    /// # Returns
    /// 返回值
    /// Success only after the applying journal is durably published.
    /// 仅在 applying 日志持久发布后返回成功。
    pub fn begin_system_transaction(
        &self,
        provider_id: &str,
        snapshot: &SystemManifestStoreSnapshot,
    ) -> Result<(), String> {
        self.recover_system_transaction(provider_id)?;
        self.write_system_transaction_journal(
            provider_id,
            SystemManifestTransactionPhase::Applying,
            snapshot,
        )
    }

    /// Marks one system-manifest transaction committed and removes its journal when possible.
    /// 将一个系统清单事务标记为已提交，并在可能时移除其日志。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` owns the transaction and `snapshot` preserves the exact journal payload.
    /// `provider_id` 拥有该事务，`snapshot` 保留精确日志载荷。
    ///
    /// # Returns
    /// 返回值
    /// Success once a crash can no longer cause startup to restore the before-image.
    /// 一旦崩溃不再会使启动恢复前镜像即返回成功。
    pub fn commit_system_transaction(
        &self,
        provider_id: &str,
        snapshot: &SystemManifestStoreSnapshot,
    ) -> Result<(), String> {
        self.write_system_transaction_journal(
            provider_id,
            SystemManifestTransactionPhase::Committed,
            snapshot,
        )?;
        let journal = self.system_transaction_journal_path(provider_id);
        let _ = remove_file_if_present(&journal, "system manifest transaction journal");
        Ok(())
    }

    /// Aborts one applying transaction by restoring its exact before-image and clearing the journal.
    /// 通过恢复精确前镜像并清除日志来中止一个 applying 事务。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` owns the transaction and `snapshot` is its exact before-image.
    /// `provider_id` 拥有该事务，`snapshot` 是其精确前镜像。
    ///
    /// # Returns
    /// 返回值
    /// Success only when both restoration and journal cleanup complete.
    /// 仅当恢复和日志清理均完成时返回成功。
    pub fn abort_system_transaction(
        &self,
        provider_id: &str,
        snapshot: &SystemManifestStoreSnapshot,
    ) -> Result<(), String> {
        self.restore_system_snapshot(provider_id, snapshot)?;
        remove_file_if_present(
            &self.system_transaction_journal_path(provider_id),
            "system manifest transaction journal",
        )
    }

    /// Discards one unusable system transaction journal after all persisted authority is removed.
    /// 在全部持久化权威被移除后丢弃一份不可用的系统事务日志。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` identifies the exact trusted system provider whose journal cannot be decoded.
    /// `provider_id` 标识事务日志无法解码的精确可信系统供应商。
    ///
    /// # Returns
    /// 返回值
    /// Success when no journal remains; callers must remove all four authoritative files first.
    /// 不再存在事务日志时返回成功；调用方必须先移除四份权威文件。
    pub fn discard_system_transaction_journal(&self, provider_id: &str) -> Result<(), String> {
        remove_file_if_present(
            &self.system_transaction_journal_path(provider_id),
            "system manifest transaction journal",
        )
    }

    /// Recovers one interrupted system-manifest transaction before any store read or mutation.
    /// 在任何存储读取或变更前恢复一个中断的系统清单事务。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` identifies the exact transaction journal.
    /// `provider_id` 标识精确的事务日志。
    ///
    /// # Returns
    /// 返回值
    /// `true` when a journal was consumed; applying journals restore their before-image while
    /// committed journals only require cleanup.
    /// 消费过日志时返回 `true`；applying 日志恢复前镜像，committed 日志仅需清理。
    pub fn recover_system_transaction(&self, provider_id: &str) -> Result<bool, String> {
        let path = self.system_transaction_journal_path(provider_id);
        let Some(bytes) = read_optional_bounded_bytes(
            &path,
            SYSTEM_TRANSACTION_JOURNAL_MAX_BYTES,
            "system manifest transaction journal",
        )?
        else {
            return Ok(false);
        };
        let journal = deserialize_bounded_store_json::<SystemManifestTransactionJournal>(
            bytes.as_slice(),
            "System manifest transaction journal",
        )
        .map_err(|error| {
            format!(
                "Failed to parse system manifest transaction journal `{}`: {error}",
                path.display()
            )
        })?;
        if journal.schema_version != SYSTEM_TRANSACTION_JOURNAL_SCHEMA_VERSION
            || journal.provider_id != provider_id
        {
            return Err(format!(
                "System manifest transaction journal `{}` has incompatible identity or schema.",
                path.display()
            ));
        }
        if journal.phase == SystemManifestTransactionPhase::Applying {
            let snapshot = journal.before.decode()?;
            self.restore_system_snapshot(provider_id, &snapshot)?;
        }
        remove_file_if_present(&path, "system manifest transaction journal")?;
        Ok(true)
    }

    /// Writes one bounded system-manifest transaction journal atomically.
    /// 原子写入一份有界系统清单事务日志。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id`, `phase`, and `snapshot` form the complete recovery record.
    /// `provider_id`、`phase` 与 `snapshot` 构成完整恢复记录。
    ///
    /// # Returns
    /// 返回值
    /// Success after the complete journal replaces any previous journal.
    /// 完整日志替换旧日志后返回成功。
    fn write_system_transaction_journal(
        &self,
        provider_id: &str,
        phase: SystemManifestTransactionPhase,
        snapshot: &SystemManifestStoreSnapshot,
    ) -> Result<(), String> {
        let journal = SystemManifestTransactionJournal {
            schema_version: SYSTEM_TRANSACTION_JOURNAL_SCHEMA_VERSION,
            provider_id: provider_id.to_string(),
            phase,
            before: EncodedSystemManifestStoreSnapshot::encode(snapshot),
        };
        let bytes = serde_json::to_vec_pretty(&journal).map_err(|error| {
            format!("Failed to serialize system manifest transaction journal: {error}")
        })?;
        if bytes.len() as u64 > SYSTEM_TRANSACTION_JOURNAL_MAX_BYTES {
            return Err(format!(
                "System manifest transaction journal for `{provider_id}` exceeds the {SYSTEM_TRANSACTION_JOURNAL_MAX_BYTES} byte bound."
            ));
        }
        let path = self.system_transaction_journal_path(provider_id);
        crate::atomic_file::write_file_atomically(&path, &bytes)
    }

    /// Restores one exact store snapshot after any later commit or publication stage fails.
    /// 在后续任一提交或发布阶段失败后恢复一份精确存储快照。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` selects the transaction and `snapshot` contains the former bytes.
    /// `provider_id` 选择事务，`snapshot` 包含原有字节。
    ///
    /// # Returns
    /// 返回值
    /// Success only when all four files again match their former presence and content.
    /// 仅当四份文件的存在状态与内容全部恢复时返回成功。
    pub fn restore_system_snapshot(
        &self,
        provider_id: &str,
        snapshot: &SystemManifestStoreSnapshot,
    ) -> Result<(), String> {
        let targets = [
            (
                self.active_path(provider_id),
                snapshot.active.as_deref(),
                "active manifest",
            ),
            (
                self.previous_path(provider_id),
                snapshot.previous.as_deref(),
                "previous manifest",
            ),
            (
                self.sync_state_path(provider_id),
                snapshot.sync_state.as_deref(),
                "manifest sync state",
            ),
            (
                self.sync_check_state_path(provider_id),
                snapshot.check_state.as_deref(),
                "manifest check state",
            ),
        ];
        let mut failures = Vec::new();
        for (path, bytes, kind) in targets {
            let result = match bytes {
                Some(bytes) => crate::atomic_file::write_file_atomically(&path, bytes),
                None => remove_file_if_present(&path, kind),
            };
            if let Err(error) = result {
                failures.push(format!("{kind}: {error}"));
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "Failed to restore provider `{provider_id}` store snapshot: {}",
                failures.join("; ")
            ))
        }
    }

    /// Moves one rejected candidate into quarantine with its classified failure kind.
    /// 将一条被拒候选及其分类失败类型移入隔离区。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id`, `revision`, and `failure_kind` identify the candidate; `bytes` are
    /// preserved for post-mortem analysis.
    /// `provider_id`、`revision` 与 `failure_kind` 标识候选；`bytes` 保留用于事后分析。
    pub fn quarantine(
        &self,
        provider_id: &str,
        revision: u64,
        failure_kind: &str,
        bytes: &[u8],
    ) -> Result<(), String> {
        validate_quarantine_provider_id(provider_id)?;
        validate_quarantine_failure_kind(failure_kind)?;
        if bytes.len() as u64 > MANIFEST_CANDIDATE_MAX_BYTES {
            return Err(format!(
                "Quarantined manifest candidate for `{provider_id}` exceeds the {MANIFEST_CANDIDATE_MAX_BYTES} byte bound."
            ));
        }
        // mutationGuard keeps atomic publication and its retention pass in one process-local
        // critical section so concurrent failures cannot prune the same file twice.
        // mutationGuard 把原子发布及其保留裁剪置于同一个进程内临界区，避免并发失败重复
        // 裁剪同一文件。
        let _mutation_guard = QUARANTINE_MUTATION_LOCK
            .lock()
            .map_err(|_| String::from("Provider manifest quarantine mutation lock is poisoned."))?;
        let quarantine = self.quarantine_path(provider_id, revision, failure_kind);
        if let Some(parent) = quarantine.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "Failed to create quarantine directory `{}`: {error}",
                    parent.display()
                )
            })?;
        }
        crate::atomic_file::write_file_atomically(&quarantine, bytes)?;
        self.prune_quarantine(provider_id)
    }

    /// Prunes one provider's quarantine files by oldest-first count and byte limits.
    /// 按最旧优先规则裁剪单个供应商的隔离文件数量与总字节数。
    fn prune_quarantine(&self, provider_id: &str) -> Result<(), String> {
        let directory = self.quarantine_dir();
        let mut entries = quarantine_file_entries(&directory, provider_id)?;
        entries.sort_by(|left, right| {
            left.modified_at_unix_ms
                .cmp(&right.modified_at_unix_ms)
                .then_with(|| left.revision.cmp(&right.revision))
                .then_with(|| left.file_name.cmp(&right.file_name))
        });
        let mut total_bytes = entries
            .iter()
            .fold(0_u64, |total, entry| total.saturating_add(entry.byte_len));
        let mut remove_count = 0_usize;
        while entries.len().saturating_sub(remove_count) > QUARANTINE_MAX_FILES_PER_PROVIDER
            || total_bytes > QUARANTINE_MAX_BYTES_PER_PROVIDER
        {
            let entry = &entries[remove_count];
            std::fs::remove_file(&entry.path).map_err(|error| {
                format!(
                    "Failed to prune quarantined manifest `{}`: {error}",
                    entry.path.display()
                )
            })?;
            total_bytes = total_bytes.saturating_sub(entry.byte_len);
            remove_count = remove_count.saturating_add(1);
        }
        Ok(())
    }

    /// Reads the active manifest bytes for one system provider.
    /// 读取一个系统供应商的 active 清单字节。
    pub fn read_active(&self, provider_id: &str) -> Result<Option<String>, String> {
        self.read_bounded(provider_id, &self.active_path(provider_id), "active")
    }

    /// Reads the previous manifest bytes for one system provider.
    /// 读取一个系统供应商的 previous 清单字节。
    pub fn read_previous(&self, provider_id: &str) -> Result<Option<String>, String> {
        self.read_bounded(provider_id, &self.previous_path(provider_id), "previous")
    }

    /// Reads one bounded manifest file.
    /// 读取一份有界清单文件。
    fn read_bounded(
        &self,
        provider_id: &str,
        path: &Path,
        layer: &str,
    ) -> Result<Option<String>, String> {
        let Some(bytes) = read_optional_bounded_bytes(
            path,
            MANIFEST_CANDIDATE_MAX_BYTES,
            &format!("{layer} manifest for `{provider_id}`"),
        )?
        else {
            return Ok(None);
        };
        String::from_utf8(bytes).map(Some).map_err(|error| {
            format!("{layer} manifest for `{provider_id}` is not valid UTF-8: {error}")
        })
    }

    /// Loads the bounded per-provider sync state.
    /// 加载有界的逐供应商同步状态。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` selects the exact provider transaction scope.
    /// `provider_id` 选择精确供应商事务作用域。
    pub fn load_sync_state(&self, provider_id: &str) -> Result<SyncStateLoad, String> {
        let path = self.sync_state_path(provider_id);
        let Some(bytes) =
            read_optional_bounded_bytes(&path, SYNC_STATE_MAX_BYTES, "manifest sync state")?
        else {
            return Ok(SyncStateLoad::Absent);
        };
        let state: ManifestSyncState =
            deserialize_bounded_store_json(bytes.as_slice(), "Manifest sync state")?;
        validate_manifest_sync_state(&state)?;
        Ok(SyncStateLoad::Present(state))
    }

    /// Loads one bounded independent synchronization-check state.
    /// 加载一份有界的独立同步检查状态。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` selects the exact provider check scope.
    /// `provider_id` 选择精确的供应商检查作用域。
    ///
    /// # Returns
    /// 返回值
    /// Explicit absence or a validated non-secret check state.
    /// 返回明确缺失状态或已验证的非秘密检查状态。
    pub fn load_check_state(&self, provider_id: &str) -> Result<CheckStateLoad, String> {
        // path is the encoded provider-owned state file below the fixed check-state directory.
        // path 是固定检查状态目录下经编码的供应商自有状态文件。
        let path = self.sync_check_state_path(provider_id);
        let Some(bytes) =
            read_optional_bounded_bytes(&path, SYNC_CHECK_STATE_MAX_BYTES, "manifest check state")?
        else {
            return Ok(CheckStateLoad::Absent);
        };
        // state is validated after bounded strict serde parsing rejects every unknown field.
        // state 在有界严格 serde 解析拒绝全部未知字段后接受验证。
        let state: ManifestCheckState =
            deserialize_bounded_store_json(bytes.as_slice(), "Manifest check state")?;
        validate_manifest_check_state(&state)?;
        Ok(CheckStateLoad::Present(state))
    }

    /// Atomically persists one bounded independent synchronization-check state.
    /// 原子持久化一份有界的独立同步检查状态。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` selects the exact provider scope and `state` is the next closed value.
    /// `provider_id` 选择精确供应商作用域，`state` 是下一个封闭值。
    ///
    /// # Returns
    /// 返回值
    /// Success only after validation and atomic replacement complete.
    /// 仅在验证与原子替换完成后返回成功。
    pub fn save_check_state(
        &self,
        provider_id: &str,
        state: &ManifestCheckState,
    ) -> Result<(), String> {
        validate_manifest_check_state(state)?;
        // serialized is the bounded non-secret document written atomically below.
        // serialized 是下方原子写入的有界非秘密文档。
        let serialized = serde_json::to_vec(state)
            .map_err(|error| format!("Failed to serialize manifest check state: {error}"))?;
        if serialized.len() as u64 > SYNC_CHECK_STATE_MAX_BYTES {
            return Err(String::from(
                "Manifest check state exceeds the bounded size.",
            ));
        }
        // path is the encoded provider-owned destination.
        // path 是经编码的供应商自有目标路径。
        let path = self.sync_check_state_path(provider_id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "Failed to create manifest check-state directory `{}`: {error}",
                    parent.display()
                )
            })?;
        }
        crate::atomic_file::write_file_atomically(&path, &serialized)
    }

    /// Captures the exact before-image of one synchronization-check state file.
    /// 捕获一份同步检查状态文件的精确前镜像。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` selects the exact provider-owned file.
    /// `provider_id` 选择精确的供应商自有文件。
    ///
    /// # Returns
    /// 返回值
    /// A bounded opaque snapshot suitable only for exact compensation.
    /// 返回仅适用于精确补偿的有界不透明快照。
    pub fn capture_check_state(
        &self,
        provider_id: &str,
    ) -> Result<ManifestCheckStateSnapshot, String> {
        // path is the only file included in this independent state transaction.
        // path 是此独立状态事务包含的唯一文件。
        let path = self.sync_check_state_path(provider_id);
        // The shared reader retains the exact bytes while enforcing the ordinary-file and
        // post-open byte boundaries against replacement races.
        // 共享读取器保留精确字节，同时针对替换竞态强制普通文件与打开后字节边界。
        let bytes = read_optional_bounded_bytes(
            &path,
            SYNC_CHECK_STATE_MAX_BYTES,
            "manifest check-state before-image",
        )?;
        Ok(ManifestCheckStateSnapshot { bytes })
    }

    /// Restores one synchronization-check state file to an exact captured before-image.
    /// 把一份同步检查状态文件恢复到精确捕获的前镜像。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` selects the exact file and `snapshot` supplies prior bytes or absence.
    /// `provider_id` 选择精确文件，`snapshot` 提供先前字节或缺失状态。
    ///
    /// # Returns
    /// 返回值
    /// Success only after exact atomic replacement or controlled removal completes.
    /// 仅在精确原子替换或受控删除完成后返回成功。
    pub fn restore_check_state(
        &self,
        provider_id: &str,
        snapshot: &ManifestCheckStateSnapshot,
    ) -> Result<(), String> {
        // path is the encoded provider-owned compensation target.
        // path 是经编码的供应商自有补偿目标。
        let path = self.sync_check_state_path(provider_id);
        match snapshot.bytes.as_deref() {
            Some(bytes) => {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).map_err(|error| {
                        format!(
                            "Failed to create manifest check-state directory `{}`: {error}",
                            parent.display()
                        )
                    })?;
                }
                crate::atomic_file::write_file_atomically(&path, bytes)
            }
            None => remove_file_if_present(&path, "manifest check state"),
        }
    }

    /// Computes the lowercase SHA-256 hex digest of one byte slice.
    /// 计算一个字节切片的十六进制 SHA-256 摘要。
    pub fn sha256_hex(bytes: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        let digest = hasher.finalize();
        let mut text = String::with_capacity(64);
        for byte in digest {
            use std::fmt::Write;
            let _ = write!(text, "{byte:02x}");
        }
        text
    }

    /// Reads one quarantined candidate file for a provider and returns its classified summary.
    /// 读取一个供应商的一条隔离候选文件并返回其分类摘要。
    ///
    /// Quarantine file names encode `{provider}.{revision}.{failure_kind}.json`; malformed
    /// names are ignored and never made eligible for cleanup (plan §R10).
    /// 隔离文件名为 `{provider}.{revision}.{failure_kind}.json`；畸形文件名会被忽略，且
    /// 永远不会进入可清理集合（方案 §R10）。
    pub fn quarantine_summaries(
        &self,
        provider_id: &str,
    ) -> Result<Vec<QuarantineSummary>, String> {
        let mut summaries = quarantine_file_entries(&self.quarantine_dir(), provider_id)?
            .into_iter()
            .map(|entry| QuarantineSummary {
                revision: entry.revision,
                failure_kind: entry.failure_kind,
                quarantined_at_unix_ms: entry.modified_at_unix_ms,
            })
            .collect::<Vec<_>>();
        summaries.sort_by(|left, right| {
            right
                .quarantined_at_unix_ms
                .cmp(&left.quarantined_at_unix_ms)
                .then_with(|| right.revision.cmp(&left.revision))
                .then_with(|| left.failure_kind.cmp(&right.failure_kind))
        });
        summaries.truncate(QUARANTINE_SUMMARY_MAX_ITEMS);
        Ok(summaries)
    }

    /// Atomically persists the bounded per-provider sync state.
    /// 原子持久化有界的逐供应商同步状态。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` selects the exact provider transaction scope and `state` is the next value.
    /// `provider_id` 选择精确供应商事务作用域，`state` 是下一值。
    pub fn save_sync_state(
        &self,
        provider_id: &str,
        state: &ManifestSyncState,
    ) -> Result<(), String> {
        validate_manifest_sync_state(state)?;
        let serialized = serde_json::to_vec(state)
            .map_err(|error| format!("Failed to serialize manifest sync state: {error}"))?;
        if serialized.len() as u64 > SYNC_STATE_MAX_BYTES {
            return Err(String::from(
                "Manifest sync state exceeds the bounded size.",
            ));
        }
        let path = self.sync_state_path(provider_id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "Failed to create sync state directory `{}`: {error}",
                    parent.display()
                )
            })?;
        }
        crate::atomic_file::write_file_atomically(&path, &serialized)
    }
}

/// Validates one persisted synchronization state before it can influence conditional requests.
/// 在一份持久化同步状态影响条件请求前对其进行校验。
///
/// # Parameters
/// 参数
/// `state` is the exact non-secret active identity and remote cache metadata.
/// `state` 是精确的非秘密活动身份与远端缓存元数据。
///
/// # Returns
/// 返回值
/// Success only for content-addressed identities, bounded HTTP header values, and a closed
/// failure-kind identifier.
/// 仅当内容寻址身份、HTTP 头值边界与封闭失败类型标识均有效时返回成功。
fn validate_manifest_sync_state(state: &ManifestSyncState) -> Result<(), String> {
    validate_manifest_version_state("active", state.revision, &state.sha256)?;
    if let Some(previous) = state.previous.as_ref() {
        validate_manifest_version_state("previous", previous.revision, &previous.sha256)?;
    }
    for (field, value) in [
        ("etag", state.etag.as_deref()),
        ("index_etag", state.index_etag.as_deref()),
        ("last_modified", state.last_modified.as_deref()),
    ] {
        let Some(value) = value else {
            continue;
        };
        if value.is_empty() || value.len() > SYNC_STATE_HEADER_MAX_BYTES {
            return Err(format!(
                "Manifest sync-state `{field}` must contain 1..={SYNC_STATE_HEADER_MAX_BYTES} bytes."
            ));
        }
        reqwest::header::HeaderValue::from_str(value).map_err(|_| {
            format!("Manifest sync-state `{field}` is not a valid single-line HTTP header value.")
        })?;
    }
    if let Some(failure_kind) = state.last_error_kind.as_deref() {
        validate_sync_failure_kind(failure_kind, "Manifest sync-state")?;
    }
    Ok(())
}

/// Validates one positive revision and lowercase SHA-256 content identity.
/// 校验一个正修订号与小写 SHA-256 内容身份。
///
/// # Parameters
/// 参数
/// `layer` labels the active or previous identity; `revision` and `sha256` are exact persisted
/// values.
/// `layer` 标识 active 或 previous 身份；`revision` 与 `sha256` 是精确持久化值。
///
/// # Returns
/// 返回值
/// Success for one structurally valid content identity, otherwise a stable corruption error.
/// 结构有效的内容身份返回成功，否则返回稳定的损坏错误。
fn validate_manifest_version_state(layer: &str, revision: u64, sha256: &str) -> Result<(), String> {
    if revision == 0 {
        return Err(format!(
            "Manifest sync-state {layer} revision must be positive."
        ));
    }
    if sha256.len() != 64
        || !sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!(
            "Manifest sync-state {layer} SHA-256 must be 64 lowercase hexadecimal characters."
        ));
    }
    Ok(())
}

/// Validates one bounded lowercase failure-kind identifier shared by sync status documents.
/// 校验同步状态文档共享的有界小写失败类型标识。
///
/// # Parameters
/// 参数
/// `failure_kind` is the exact candidate value and `document` labels the owning state file.
/// `failure_kind` 是精确候选值，`document` 标识所属状态文件。
///
/// # Returns
/// 返回值
/// Success for the closed identifier alphabet, otherwise a stable validation error.
/// 封闭标识字符集有效时返回成功，否则返回稳定校验错误。
fn validate_sync_failure_kind(failure_kind: &str, document: &str) -> Result<(), String> {
    if failure_kind.is_empty()
        || failure_kind.len() > SYNC_CHECK_FAILURE_KIND_MAX_BYTES
        || !failure_kind
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(format!(
            "{document} failure kind must be 1-{SYNC_CHECK_FAILURE_KIND_MAX_BYTES} lowercase ASCII identifier bytes."
        ));
    }
    Ok(())
}

/// Validates one independent synchronization-check state before reading or writing it.
/// 在读取或写入前验证一份独立同步检查状态。
///
/// # Parameters
/// 参数
/// `state` is the exact closed document to validate.
/// `state` 是待验证的精确封闭文档。
///
/// # Returns
/// 返回值
/// Success only when timestamps, outcome, and the bounded stable failure kind agree.
/// 仅当时间戳、结果与有界稳定失败类型一致时返回成功。
fn validate_manifest_check_state(state: &ManifestCheckState) -> Result<(), String> {
    if let Some(last_successful_at_unix_ms) = state.last_successful_at_unix_ms
        && last_successful_at_unix_ms > state.checked_at_unix_ms
    {
        return Err(String::from(
            "Manifest check-state last-successful time exceeds its last-checked time.",
        ));
    }
    if let Some(failure_kind) = state.last_error_kind.as_deref() {
        validate_sync_failure_kind(failure_kind, "Manifest check-state")?;
    }
    // requiresFailure distinguishes terminal failure outcomes from successful observations.
    // requiresFailure 区分终态失败结果与成功观测。
    let requires_failure = matches!(
        state.outcome,
        ManifestCheckOutcome::Rejected | ManifestCheckOutcome::Failed
    );
    if requires_failure != state.last_error_kind.is_some() {
        return Err(String::from(
            "Manifest check-state outcome and failure-kind presence disagree.",
        ));
    }
    Ok(())
}

/// One parsed quarantine file used for bounded retention and status projection.
/// 一条用于有界保留与状态投影的已解析隔离文件记录。
struct QuarantineFileEntry {
    /// Exact file path eligible for non-recursive removal.
    /// 可执行非递归删除的精确文件路径。
    path: PathBuf,
    /// Exact file name used as the deterministic final ordering key.
    /// 用作确定性最终排序键的精确文件名。
    file_name: String,
    /// Manifest revision encoded in the file name.
    /// 文件名中编码的 Manifest 修订号。
    revision: u64,
    /// Classified failure kind encoded in the file name.
    /// 文件名中编码的分类失败类型。
    failure_kind: String,
    /// Bounded file byte length.
    /// 有界文件字节长度。
    byte_len: u64,
    /// Best-effort filesystem modification timestamp in Unix milliseconds.
    /// 尽力读取的文件系统 Unix 毫秒修改时间戳。
    modified_at_unix_ms: u64,
}

/// Reads exact provider-owned quarantine files without following directories or guessing names.
/// 读取精确归属供应商的隔离文件，不跟随目录且不猜测文件名。
fn quarantine_file_entries(
    directory: &Path,
    provider_id: &str,
) -> Result<Vec<QuarantineFileEntry>, String> {
    if !path_exists(directory, "quarantine directory")? {
        return Ok(Vec::new());
    }
    let entries = std::fs::read_dir(directory).map_err(|error| {
        format!(
            "Failed to read quarantine directory `{}`: {error}",
            directory.display()
        )
    })?;
    let mut parsed = Vec::new();
    // scanned_entries prevents one polluted shared directory from creating an unbounded scan.
    // scanned_entries 防止一个被污染的共享目录造成无界扫描。
    let mut scanned_entries = 0usize;
    for entry in entries {
        scanned_entries = scanned_entries.saturating_add(1);
        if scanned_entries > MANIFEST_DIRECTORY_SCAN_MAX_ENTRIES {
            return Err(format!(
                "Quarantine directory `{}` exceeds the {MANIFEST_DIRECTORY_SCAN_MAX_ENTRIES}-entry scan limit.",
                directory.display()
            ));
        }
        let entry = entry.map_err(|error| {
            format!(
                "Failed to read quarantine entry `{}`: {error}",
                directory.display()
            )
        })?;
        let file_type = entry.file_type().map_err(|error| {
            format!(
                "Failed to read quarantine entry type `{}`: {error}",
                entry.path().display()
            )
        })?;
        if !file_type.is_file() {
            continue;
        }
        let file_name = entry.file_name().to_string_lossy().into_owned();
        // Quarantine file names use `{provider}.{revision}.{failure_kind}.json`.
        // 隔离文件名为 `{provider}.{revision}.{failure_kind}.json`。
        let Some(rest) = file_name.strip_prefix(&format!("{provider_id}.")) else {
            continue;
        };
        let Some((revision_text, failure_rest)) = rest.split_once('.') else {
            continue;
        };
        let Ok(revision) = revision_text.parse::<u64>() else {
            continue;
        };
        let Some(failure_kind) = failure_rest.strip_suffix(".json") else {
            continue;
        };
        if validate_quarantine_failure_kind(failure_kind).is_err() {
            continue;
        }
        // failure_kind_owned ends the file-name borrow before the owned name moves into the record.
        // failure_kind_owned 在文件名移入记录前结束对文件名的借用。
        let failure_kind_owned = failure_kind.to_string();
        let metadata = entry.metadata().map_err(|error| {
            format!(
                "Failed to read quarantine metadata `{}`: {error}",
                entry.path().display()
            )
        })?;
        let modified_at_unix_ms = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(0);
        parsed.push(QuarantineFileEntry {
            path: entry.path(),
            file_name,
            revision,
            failure_kind: failure_kind_owned,
            byte_len: metadata.len(),
            modified_at_unix_ms,
        });
    }
    Ok(parsed)
}

/// Validates one system provider identifier before it participates in a quarantine file name.
/// 在系统供应商标识参与隔离文件名之前对其进行校验。
///
/// # Parameters
/// 参数
/// `provider_id` is the exact validated manifest owner expected by the system store.
/// `provider_id` 是系统存储预期的精确已校验 Manifest 所有者。
///
/// # Returns
/// 返回值
/// Success only for a bounded stable identifier that cannot create another path component.
/// 仅当标识有界、稳定且无法创建其他路径组件时返回成功。
fn validate_quarantine_provider_id(provider_id: &str) -> Result<(), String> {
    if provider_id.is_empty()
        || provider_id.len()
            > vulcan_provider_adapters::provider_manifest::MAX_PROVIDER_IDENTIFIER_LEN
        || !provider_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(format!(
            "Quarantine provider_id must use 1-{} ASCII alphanumeric, dot, underscore, or hyphen bytes.",
            vulcan_provider_adapters::provider_manifest::MAX_PROVIDER_IDENTIFIER_LEN
        ));
    }
    Ok(())
}

/// Validates one stable failure category before it participates in a quarantine file name.
/// 在稳定失败分类参与隔离文件名之前对其进行校验。
///
/// # Parameters
/// 参数
/// `failure_kind` is the closed verification or recovery category supplied by the caller.
/// `failure_kind` 是调用方提供的封闭校验或恢复分类。
///
/// # Returns
/// 返回值
/// Success only for a bounded lowercase identifier that cannot escape the quarantine directory.
/// 仅当标识有界、小写且无法逃逸隔离目录时返回成功。
fn validate_quarantine_failure_kind(failure_kind: &str) -> Result<(), String> {
    if failure_kind.is_empty()
        || failure_kind.len() > SYNC_CHECK_FAILURE_KIND_MAX_BYTES
        || !failure_kind
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(format!(
            "Quarantine failure_kind must use 1-{SYNC_CHECK_FAILURE_KIND_MAX_BYTES} lowercase ASCII letter, digit, or underscore bytes."
        ));
    }
    Ok(())
}

/// Removes one exact file while treating absence as an idempotent success.
/// 移除一个精确文件，并把文件缺失视为幂等成功。
fn remove_file_if_present(path: &Path, kind: &str) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "Failed to remove {kind} `{}` during rollback: {error}",
            path.display()
        )),
    }
}

/// Tests path presence without converting filesystem access failures into absence.
/// 检查路径是否存在，且不把文件系统访问失败转换成不存在。
///
/// # Parameters
/// 参数
/// `path` is exact and `kind` scopes the controlled diagnostic.
/// `path` 是精确路径，`kind` 限定受控诊断范围。
///
/// # Returns
/// 返回值
/// Presence, absence, or the exact metadata-probe failure.
/// 返回存在、不存在或精确的元数据探测失败。
fn path_exists(path: &Path, kind: &str) -> Result<bool, String> {
    path.try_exists()
        .map_err(|error| format!("Failed to inspect {kind} `{}`: {error}", path.display()))
}

/// Parses one canonical staged Manifest file name into its encoded provider stem and revision.
/// 将一个规范暂存 Manifest 文件名解析为编码后的供应商文件主干与修订号。
///
/// # Parameters
/// 参数
/// `file_name` is one directory-local file name, never a path.
/// `file_name` 是一个目录内文件名，绝不是路径。
///
/// # Returns
/// 返回值
/// The exact encoded provider stem and numeric revision for `<stem>.<revision>.json`, otherwise
/// `None` for unrelated files.
/// 对 `<stem>.<revision>.json` 返回精确编码供应商主干与数字修订；无关文件返回 `None`。
fn staged_candidate_identity(file_name: &str) -> Option<(&str, u64)> {
    let stem = file_name.strip_suffix(".json")?;
    let (provider_file_stem, revision) = stem.rsplit_once('.')?;
    if provider_file_stem.is_empty() {
        return None;
    }
    Some((provider_file_stem, revision.parse::<u64>().ok()?))
}

/// Reads one optional file under an exact byte bound for transaction snapshotting.
/// 在精确字节上限内读取一份可选文件，用于事务快照。
///
/// # Parameters
/// 参数
/// `path` is exact, `max_bytes` is the hard bound, and `kind` identifies diagnostics.
/// `path` 为精确路径，`max_bytes` 为硬上限，`kind` 用于诊断标识。
///
/// # Returns
/// 返回值
/// Raw bytes when present, `None` when absent, or a controlled filesystem/bound failure.
/// 文件存在时返回原始字节，不存在时返回 `None`，否则返回受控文件系统或边界错误。
fn read_optional_bounded_bytes(
    path: &Path,
    max_bytes: u64,
    kind: &str,
) -> Result<Option<Vec<u8>>, String> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "Failed to stat {kind} `{}`: {error}",
                path.display()
            ));
        }
    };
    if !metadata.file_type().is_file() {
        return Err(format!(
            "{kind} `{}` is not a regular file.",
            path.display()
        ));
    }
    if metadata.len() > max_bytes {
        return Err(format!(
            "{kind} `{}` exceeds the {max_bytes} byte bound.",
            path.display()
        ));
    }
    let file = std::fs::File::open(path)
        .map_err(|error| format!("Failed to open {kind} `{}`: {error}", path.display()))?;
    let mut limited = file.take(max_bytes.saturating_add(1));
    let mut bytes = Vec::with_capacity(metadata.len().min(max_bytes) as usize);
    limited
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Failed to read {kind} `{}`: {error}", path.display()))?;
    if bytes.len() as u64 > max_bytes {
        return Err(format!(
            "{kind} `{}` changed while reading and exceeds the {max_bytes} byte bound.",
            path.display()
        ));
    }
    Ok(Some(bytes))
}

/// Reads one required regular file through the shared post-open byte boundary.
/// 通过共享的打开后字节边界读取一份必需的普通文件。
///
/// # Parameters
/// 参数
/// `path` is exact, `max_bytes` is the hard bound, and `kind` identifies diagnostics.
/// `path` 为精确路径，`max_bytes` 为硬上限，`kind` 用于诊断标识。
///
/// # Returns
/// 返回值
/// Exact bytes when present, otherwise a deterministic absence, filesystem, or bound failure.
/// 文件存在时返回精确字节，否则返回确定性的缺失、文件系统或边界错误。
fn read_required_bounded_bytes(path: &Path, max_bytes: u64, kind: &str) -> Result<Vec<u8>, String> {
    read_optional_bounded_bytes(path, max_bytes, kind)?
        .ok_or_else(|| format!("Required {kind} `{}` is missing.", path.display()))
}

/// Deserializes one local store document after enforcing shared JSON depth and node bounds.
/// 在强制执行共享 JSON 深度与节点数量上限后反序列化一份本地存储文档。
///
/// # Parameters
/// 参数
/// `bytes` are already read through the owning file boundary and `label` scopes diagnostics.
/// `bytes` 已经通过所属文件边界读取，`label` 限定诊断范围。
///
/// # Returns
/// 返回值
/// The requested closed state type or a deterministic syntax, depth, count, or shape failure.
/// 返回所需封闭状态类型，或确定性的语法、深度、数量或结构失败。
fn deserialize_bounded_store_json<Document>(bytes: &[u8], label: &str) -> Result<Document, String>
where
    Document: DeserializeOwned,
{
    let value: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|error| format!("{label} is invalid JSON: {error}"))?;
    vulcan_provider_adapters::provider_manifest::validate_runtime_json_bounds(&value, label)?;
    serde_json::from_value(value).map_err(|error| format!("{label} structure is invalid: {error}"))
}

/// Encodes one system provider identifier into a filesystem-safe stem without changing valid IDs.
/// 把一个系统供应商标识编码为文件系统安全且不改变合法标识的主干名。
///
/// # Parameters
/// 参数
/// `provider_id` may originate at an RPC boundary and therefore cannot be trusted to have passed
/// Manifest validation before a store path is constructed.
/// `provider_id` 可能来自 RPC 边界，因此在构造存储路径前不能假定其已经通过 Manifest 校验。
///
/// # Returns
/// 返回值
/// A stem preserving the Manifest stable `[A-Za-z0-9._-]` set while percent-encoding every byte
/// that could introduce a path separator, drive prefix, or alternate filename interpretation.
/// 返回保留 Manifest 稳定字符集 `[A-Za-z0-9._-]` 的主干名，同时百分号编码任何可能引入
/// 路径分隔符、盘符前缀或其他文件名解释的字节。
fn encode_system_provider_file_stem(provider_id: &str) -> String {
    let mut encoded = String::with_capacity(provider_id.len() + 8);
    for byte in provider_id.as_bytes() {
        match *byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' => {
                encoded.push(*byte as char)
            }
            other => {
                encoded.push('%');
                encoded.push(hex_digit(other >> 4));
                encoded.push(hex_digit(other & 0x0F));
            }
        }
    }
    encoded
}

/// Encodes one custom provider identifier into a reversible filesystem-safe file stem.
/// 将自定义供应商标识编码为可逆且文件系统安全的文件主干名。
///
/// Only lowercase letters, digits, `-`, and `_` stay literal; every other byte (including
/// uppercase letters, the `:` inside `custom:`, dots, separators, and percent signs) is
/// percent-encoded. Encoding uppercase bytes prevents case-insensitive filesystems from
/// collapsing distinct provider identifiers while preserving a reversible mapping.
/// 仅小写字母、数字、`-` 与 `_` 保持字面；其余字节（包括大写字母、`custom:` 中的冒号、
/// 点号、分隔符与百分号）全部百分号编码。编码大写字节可防止大小写不敏感文件系统合并
/// 不同供应商标识，同时保持映射可逆。
fn encode_custom_provider_file_stem(provider_id: &str) -> String {
    let mut encoded = String::with_capacity(provider_id.len() + 8);
    for byte in provider_id.as_bytes() {
        match *byte {
            b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' => {
                encoded.push(*byte as char);
            }
            other => {
                encoded.push('%');
                encoded.push(hex_digit((other >> 4) & 0x0F));
                encoded.push(hex_digit(other & 0x0F));
            }
        }
    }
    encoded
}

/// Decodes one percent-encoded custom provider file stem back into its identifier.
/// 将百分号编码的自定义供应商文件主干名解码回标识。
///
/// # Parameters
/// 参数
/// `stem` is the exact encoded file stem without the `.json` suffix.
/// `stem` 是不含 `.json` 后缀的精确编码文件主干名。
///
/// # Returns
/// 返回值
/// The decoded identifier, or `None` for malformed percent sequences.
/// 返回解码后的标识；百分号序列格式错误时返回 `None`。
fn decode_custom_provider_file_stem(stem: &str) -> Option<String> {
    let mut decoded = Vec::with_capacity(stem.len());
    let bytes = stem.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' => {
                let high = hex_value(*bytes.get(index + 1)?)?;
                let low = hex_value(*bytes.get(index + 2)?)?;
                decoded.push(high << 4 | low);
                index += 3;
            }
            other => {
                decoded.push(other);
                index += 1;
            }
        }
    }
    String::from_utf8(decoded).ok()
}

/// Returns the uppercase hex digit for one nibble.
/// 返回一个半字节对应的大写十六进制数字。
fn hex_digit(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        _ => (b'A' + nibble - 10) as char,
    }
}

/// Returns the nibble value for one hex digit.
/// 返回一个十六进制数字对应的半字节值。
fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// One system manifest resolved through the startup recovery chain (plan §17.5).
/// 通过启动恢复链解析的一份系统清单（方案 §17.5）。
pub enum SystemManifestRecovery {
    /// The active layer compiled successfully.
    /// active 层编译成功。
    Active(std::sync::Arc<vulcan_provider_adapters::ResolvedProviderManifest>),
    /// The active layer failed and the previous layer was restored as active.
    /// active 层失败，previous 层已恢复为 active。
    PreviousRestored(std::sync::Arc<vulcan_provider_adapters::ResolvedProviderManifest>),
    /// Both persisted layers failed and the bundled baseline is used.
    /// 两层持久化均失败，使用内置基线。
    BundledFallback(std::sync::Arc<vulcan_provider_adapters::ResolvedProviderManifest>),
}

/// Loads one system provider manifest through the recovery chain:
/// validated active, restored previous, then bundled baseline.
/// 通过恢复链加载一个系统供应商清单：校验 active、恢复 previous、回退内置基线。
///
/// # Parameters
/// 参数
/// `store` owns the file layout, `provider_id` selects the provider, and `bundled_text` is
/// the read-only runtime-assets baseline for this provider.
/// `store` 持有文件布局，`provider_id` 选择供应商，`bundled_text` 是该供应商的
/// 只读 runtime-assets 基线。
///
/// # Returns
/// 返回值
/// The compiled manifest and its layer, or a controlled failure that quarantines invalid bytes.
/// 返回已编译清单及其层级，或隔离无效字节后的受控失败。
pub fn load_system_manifest_with_recovery(
    store: &ManifestStoreLayout,
    provider_id: &str,
    bundled_text: &str,
) -> Result<SystemManifestRecovery, String> {
    store.recover_system_transaction(provider_id)?;
    let compile = |text: &str| {
        vulcan_provider_adapters::compile_manifest_text(
            text,
            vulcan_provider_adapters::ManifestOwnership::System,
        )
    };
    let active_text = store.read_active(provider_id)?;
    if let Some(text) = active_text.as_deref() {
        match compile(text) {
            Ok(resolved) => {
                return Ok(SystemManifestRecovery::Active(std::sync::Arc::new(
                    resolved,
                )));
            }
            Err(error) => {
                store.quarantine(provider_id, 0, "active_invalid", text.as_bytes())?;
                let _ = error;
            }
        }
    }
    let previous_text = store.read_previous(provider_id)?;
    if let Some(text) = previous_text.as_deref() {
        match compile(text) {
            Ok(resolved) => {
                store.rollback(provider_id)?;
                return Ok(SystemManifestRecovery::PreviousRestored(
                    std::sync::Arc::new(resolved),
                ));
            }
            Err(error) => {
                store.quarantine(provider_id, 0, "previous_invalid", text.as_bytes())?;
                let _ = error;
            }
        }
    }
    let bundled = compile(bundled_text).map_err(|error| {
        format!("Bundled manifest for `{provider_id}` failed to compile: {error}")
    })?;
    Ok(SystemManifestRecovery::BundledFallback(
        std::sync::Arc::new(bundled),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one isolated store layout inside a temporary directory.
    /// 在临时目录内构建一个隔离的存储布局。
    fn layout() -> (ManifestStoreLayout, tempfile::TempDir) {
        let directory = tempfile::tempdir().expect("temp dir should create");
        let store = ManifestStoreLayout::new(directory.path());
        (store, directory)
    }

    /// Verifies staging, activation, and previous rotation are atomic and reversible.
    /// 验证 staging、激活与 previous 轮换是原子且可逆的。
    #[test]
    fn activation_rotates_active_to_previous_atomically() {
        let (store, _directory) = layout();
        store
            .stage_candidate("openrouter", 1, br#"{"revision":1}"#)
            .expect("first candidate should stage");
        store
            .activate("openrouter", 1)
            .expect("first activation should publish");
        assert_eq!(
            store.read_active("openrouter").expect("read active"),
            Some(String::from(r#"{"revision":1}"#))
        );
        assert_eq!(
            store.read_previous("openrouter").expect("read previous"),
            None
        );

        store
            .stage_candidate("openrouter", 2, br#"{"revision":2}"#)
            .expect("second candidate should stage");
        store
            .activate("openrouter", 2)
            .expect("second activation should rotate");
        assert_eq!(
            store.read_active("openrouter").expect("read active"),
            Some(String::from(r#"{"revision":2}"#))
        );
        assert_eq!(
            store.read_previous("openrouter").expect("read previous"),
            Some(String::from(r#"{"revision":1}"#))
        );

        store
            .rollback("openrouter")
            .expect("rollback should restore");
        assert_eq!(
            store.read_active("openrouter").expect("read active"),
            Some(String::from(r#"{"revision":1}"#))
        );
    }

    /// Verifies staging a newer revision removes only older candidates for the same provider.
    /// 验证暂存较新修订只会移除同一供应商的较旧候选。
    #[test]
    fn staging_retains_only_the_latest_candidate_per_provider() {
        let (store, _directory) = layout();
        let older = store
            .stage_candidate("provider-with-dash", 7, br#"{"revision":7}"#)
            .expect("older candidate should stage");
        let unrelated = store
            .stage_candidate("another-provider", 3, br#"{"revision":3}"#)
            .expect("unrelated candidate should stage");
        let latest = store
            .stage_candidate("provider-with-dash", 8, br#"{"revision":8}"#)
            .expect("latest candidate should stage");

        assert!(!older.exists());
        assert!(unrelated.exists());
        assert!(latest.exists());
        let provider_file_stem = encode_system_provider_file_stem("provider-with-dash");
        let latest_file_name = format!("{provider_file_stem}.8.json");
        assert_eq!(
            staged_candidate_identity(latest_file_name.as_str()),
            Some((provider_file_stem.as_str(), 8))
        );
    }

    /// Verifies a missing staged candidate cannot activate.
    /// 验证缺失的 staged 候选不能激活。
    #[test]
    fn activation_rejects_missing_staged_candidate() {
        let (store, _directory) = layout();
        let error = store
            .activate("openrouter", 99)
            .expect_err("missing staged candidate must fail");
        assert!(error.contains("missing before activation"), "{error}");
    }

    /// Verifies the third activation supersedes an existing previous layer cleanly.
    /// 验证第三次激活干净地取代已存在的 previous 层。
    ///
    /// Windows rename fails when the target exists, so the transaction must remove the
    /// superseded previous file before rotating (plan §17.1).
    /// Windows 的 rename 在目标存在时会失败，因此事务必须先移除被取代的 previous 文件
    /// 再轮换（方案 §17.1）。
    #[test]
    fn third_activation_supersedes_existing_previous_layer() {
        let (store, _directory) = layout();
        for (revision, content) in [
            (1u64, br#"{"revision":1}"# as &[u8]),
            (2, br#"{"revision":2}"#),
            (3, br#"{"revision":3}"#),
        ] {
            store
                .stage_candidate("openrouter", revision, content)
                .expect("candidate should stage");
            store
                .activate("openrouter", revision)
                .expect("activation should publish");
        }
        assert_eq!(
            store.read_active("openrouter").expect("read active"),
            Some(String::from(r#"{"revision":3}"#))
        );
        assert_eq!(
            store.read_previous("openrouter").expect("read previous"),
            Some(String::from(r#"{"revision":2}"#)),
            "the second layer must be the retained previous layer"
        );
        // rollback restores the retained previous layer.
        // 回滚恢复保留的 previous 层。
        store
            .rollback("openrouter")
            .expect("rollback should restore");
        assert_eq!(
            store.read_active("openrouter").expect("read active"),
            Some(String::from(r#"{"revision":2}"#))
        );
    }

    /// Verifies quarantine preserves rejected candidate bytes.
    /// 验证隔离区保留被拒候选字节。
    #[test]
    fn quarantine_preserves_rejected_candidate() {
        let (store, _directory) = layout();
        store
            .quarantine("openrouter", 3, "schema", br#"{"bad":true}"#)
            .expect("quarantine should write");
        let path = store.quarantine_path("openrouter", 3, "schema");
        assert_eq!(
            std::fs::read_to_string(&path).expect("quarantine file should read"),
            r#"{"bad":true}"#
        );
    }

    /// Verifies quarantine retention is bounded and status exposes only the newest window.
    /// 验证隔离区保留有界，且状态仅暴露最新窗口。
    #[test]
    fn quarantine_retention_and_status_are_bounded() {
        // store owns the isolated provider quarantine fixture.
        // store 持有隔离的供应商隔离区测试夹具。
        let (store, _directory) = layout();
        // candidate is a small valid byte payload used to exercise the count bound cheaply.
        // candidate 是用于低成本验证数量上限的小型有效字节载荷。
        let candidate = br#"{"bad":true}"#;
        for revision in 0..(QUARANTINE_MAX_FILES_PER_PROVIDER as u64 + 8) {
            store
                .quarantine("openrouter", revision, "schema", candidate)
                .expect("bounded quarantine write should succeed");
        }
        // entries is the exact retained file set after automatic pruning.
        // entries 是自动裁剪后精确保留的文件集合。
        let entries = quarantine_file_entries(&store.quarantine_dir(), "openrouter")
            .expect("quarantine entries should read");
        assert_eq!(entries.len(), QUARANTINE_MAX_FILES_PER_PROVIDER);
        assert!(
            entries.iter().map(|entry| entry.byte_len).sum::<u64>()
                <= QUARANTINE_MAX_BYTES_PER_PROVIDER
        );
        // summaries is the newest bounded status projection, independent from disk capacity.
        // summaries 是独立于磁盘容量的最新有界状态投影。
        let summaries = store
            .quarantine_summaries("openrouter")
            .expect("quarantine summaries should read");
        assert_eq!(summaries.len(), QUARANTINE_SUMMARY_MAX_ITEMS);
        assert_eq!(
            summaries.first().map(|summary| summary.revision),
            Some(QUARANTINE_MAX_FILES_PER_PROVIDER as u64 + 7)
        );
    }

    /// Verifies quarantine rejects a candidate that exceeds the shared manifest byte bound.
    /// 验证隔离区拒绝超过共享 Manifest 字节上限的候选。
    #[test]
    fn quarantine_rejects_oversized_candidate() {
        // store owns the isolated oversized-candidate fixture.
        // store 持有隔离的超大候选测试夹具。
        let (store, _directory) = layout();
        // oversized exceeds the exact accepted candidate bound by one byte.
        // oversized 精确超过可接受候选上限一个字节。
        let oversized = vec![0_u8; MANIFEST_CANDIDATE_MAX_BYTES as usize + 1];
        // error proves rejection occurs before any quarantine file is published.
        // error 证明拒绝发生在任何隔离文件发布之前。
        let error = store
            .quarantine("openrouter", 1, "schema", &oversized)
            .expect_err("oversized quarantine candidate must fail");
        assert!(error.contains("exceeds"), "error: {error}");
        assert!(!store.quarantine_path("openrouter", 1, "schema").exists());
    }

    /// Verifies malformed names and traversal-shaped identities never become cleanup targets.
    /// 验证畸形文件名与路径穿越形态身份永远不会成为清理目标。
    #[test]
    fn quarantine_ignores_malformed_names_and_never_deletes_outside_its_directory() {
        // store owns one isolated quarantine root and directory owns its cleanup lifetime.
        // store 持有隔离区根，directory 持有其清理生命周期。
        let (store, directory) = layout();
        // outsideSentinel proves retention never crosses the exact quarantine directory.
        // outsideSentinel 证明保留清理绝不越过精确隔离目录。
        let outside_sentinel = directory.path().join("outside-sentinel.json");
        std::fs::write(&outside_sentinel, b"outside").expect("outside sentinel should write");
        std::fs::create_dir_all(store.quarantine_dir())
            .expect("quarantine directory should create");
        // malformedRevision and malformedKind both share the provider prefix but not the closed
        // controlled file-name grammar.
        // malformedRevision 与 malformedKind 均共享供应商前缀，但不符合封闭受控文件名语法。
        let malformed_revision = store
            .quarantine_dir()
            .join("openrouter.not-a-revision.schema.json");
        let malformed_kind = store
            .quarantine_dir()
            .join("openrouter.999.schema.extra.json");
        std::fs::write(&malformed_revision, b"malformed revision")
            .expect("malformed revision fixture should write");
        std::fs::write(&malformed_kind, b"malformed kind")
            .expect("malformed kind fixture should write");

        for revision in 0..(QUARANTINE_MAX_FILES_PER_PROVIDER as u64 + 8) {
            store
                .quarantine("openrouter", revision, "schema", br#"{"bad":true}"#)
                .expect("controlled quarantine write should succeed");
        }
        let provider_error = store
            .quarantine("../outside", 1, "schema", b"blocked")
            .expect_err("path-shaped provider id must fail closed");
        let failure_error = store
            .quarantine("openrouter", 1, "../../outside", b"blocked")
            .expect_err("path-shaped failure kind must fail closed");

        assert!(provider_error.contains("provider_id"));
        assert!(failure_error.contains("failure_kind"));
        assert!(outside_sentinel.exists());
        assert!(malformed_revision.exists());
        assert!(malformed_kind.exists());
        assert_eq!(
            store
                .quarantine_summaries("openrouter")
                .expect("controlled summaries should read")
                .len(),
            QUARANTINE_SUMMARY_MAX_ITEMS
        );
    }

    /// Verifies concurrent quarantine publications serialize write and retention cleanup.
    /// 验证并发隔离发布会串行执行写入与保留清理。
    #[test]
    fn concurrent_quarantine_writes_preserve_a_bounded_consistent_file_set() {
        // store is shared by every concurrent rejection writer.
        // store 由每个并发拒绝写入者共享。
        let (store, _directory) = layout();
        let store = std::sync::Arc::new(store);
        // writerCount exceeds the retention bound and forces multiple overlapping prune attempts.
        // writerCount 超过保留上限，从而强制多次重叠裁剪尝试。
        let writer_count = QUARANTINE_MAX_FILES_PER_PROVIDER + 8;
        // barrier releases every writer at one deterministic contention boundary.
        // barrier 在一个确定性竞争边界释放全部写入者。
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(writer_count));
        // handles retain every writer result so no thread failure can be ignored.
        // handles 保留每个写入者结果，确保线程失败不会被忽略。
        let handles = (0..writer_count)
            .map(|revision| {
                let store = std::sync::Arc::clone(&store);
                let barrier = std::sync::Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    store.quarantine(
                        "openrouter",
                        revision as u64,
                        "schema",
                        format!(r#"{{"revision":{revision}}}"#).as_bytes(),
                    )
                })
            })
            .collect::<Vec<_>>();
        for handle in handles {
            handle
                .join()
                .expect("quarantine writer must not panic")
                .expect("quarantine writer must not race cleanup");
        }

        let entries = quarantine_file_entries(&store.quarantine_dir(), "openrouter")
            .expect("retained quarantine files should parse");
        assert_eq!(entries.len(), QUARANTINE_MAX_FILES_PER_PROVIDER);
        assert!(
            entries.iter().map(|entry| entry.byte_len).sum::<u64>()
                <= QUARANTINE_MAX_BYTES_PER_PROVIDER
        );
    }

    /// Verifies the total-byte bound independently prunes oldest controlled candidates.
    /// 验证总字节上限会独立裁剪最旧的受控候选。
    #[test]
    fn quarantine_total_byte_bound_prunes_oldest_candidates() {
        // store owns one isolated byte-retention fixture.
        // store 持有一个隔离的字节保留测试夹具。
        let (store, _directory) = layout();
        // candidate reaches the per-candidate bound so nine writes exceed the aggregate bound.
        // candidate 达到单候选上限，因此九次写入会超过总量上限。
        let candidate = vec![b'x'; MANIFEST_CANDIDATE_MAX_BYTES as usize];
        for revision in 0..9_u64 {
            store
                .quarantine("openrouter", revision, "schema", &candidate)
                .expect("bounded candidate should write and prune");
        }
        let entries = quarantine_file_entries(&store.quarantine_dir(), "openrouter")
            .expect("byte-bounded entries should read");
        assert_eq!(entries.len(), 8);
        assert_eq!(
            entries.iter().map(|entry| entry.byte_len).sum::<u64>(),
            QUARANTINE_MAX_BYTES_PER_PROVIDER
        );
    }

    /// Verifies cleanup failures are surfaced without deleting an unremovable controlled file.
    /// 验证清理失败会显式返回，且不会删除无法移除的受控文件。
    #[cfg(windows)]
    #[test]
    fn quarantine_cleanup_failure_is_reported() {
        use std::os::windows::fs::OpenOptionsExt;

        // store owns one Windows read-only cleanup fixture.
        // store 持有一个 Windows 只读清理测试夹具。
        let (store, _directory) = layout();
        std::fs::create_dir_all(store.quarantine_dir())
            .expect("quarantine directory should create");
        for revision in 0..=(QUARANTINE_MAX_FILES_PER_PROVIDER as u64) {
            std::fs::write(
                store.quarantine_path("openrouter", revision, "schema"),
                b"candidate",
            )
            .expect("controlled cleanup fixture should write");
        }
        // protectedPath is deterministically the oldest revision under equal timestamps.
        // protectedPath 在时间戳相同时确定性地对应最旧修订。
        let protected_path = store.quarantine_path("openrouter", 0, "schema");
        // lockedFile deliberately omits FILE_SHARE_DELETE so Windows rejects the prune operation.
        // lockedFile 故意不授予 FILE_SHARE_DELETE，使 Windows 拒绝裁剪操作。
        let locked_file = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&protected_path)
            .expect("protected fixture should open without delete sharing");

        let result = store.prune_quarantine("openrouter");
        drop(locked_file);

        let error = result.expect_err("delete-shared denial must surface cleanup failure");
        assert!(error.contains("Failed to prune quarantined manifest"));
        assert!(protected_path.exists());
    }

    /// Verifies sync state round-trips and rejects unbounded input.
    /// 验证同步状态往返，并拒绝无界输入。
    #[test]
    fn sync_state_round_trips_and_stays_bounded() {
        let (store, _directory) = layout();
        assert!(matches!(
            store.load_sync_state("probe").expect("load state"),
            SyncStateLoad::Absent
        ));
        store
            .save_sync_state(
                "probe",
                &ManifestSyncState {
                    revision: 7,
                    sha256: "a".repeat(64),
                    previous: Some(ManifestVersionState {
                        revision: 6,
                        sha256: "b".repeat(64),
                    }),
                    etag: Some(String::from("\"etag-1\"")),
                    index_etag: Some(String::from("\"index-etag-1\"")),
                    last_modified: None,
                    synced_at_unix_ms: 1000,
                    last_error_kind: Some(String::from("schema")),
                },
            )
            .expect("state should save");
        match store.load_sync_state("probe").expect("reload state") {
            SyncStateLoad::Present(state) => {
                assert_eq!(state.revision, 7);
                assert_eq!(state.last_error_kind.as_deref(), Some("schema"));
                assert_eq!(state.previous.as_ref().map(|value| value.revision), Some(6));
            }
            SyncStateLoad::Absent => panic!("state must be present after save"),
        }
        let oversized = "x".repeat(SYNC_STATE_HEADER_MAX_BYTES + 1);
        let error = store
            .save_sync_state(
                "probe",
                &ManifestSyncState {
                    revision: 8,
                    sha256: "c".repeat(64),
                    previous: None,
                    etag: Some(oversized),
                    index_etag: None,
                    last_modified: None,
                    synced_at_unix_ms: 0,
                    last_error_kind: None,
                },
            )
            .expect_err("unbounded state header must be rejected");
        assert!(error.contains("etag"), "unexpected error: {error}");
    }

    /// Reads the bundled deepseek manifest text used as the recovery baseline.
    /// 读取作为恢复基线的内置 deepseek 清单文本。
    fn bundled_deepseek_text() -> String {
        let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
        std::fs::read_to_string(directory.join("deepseek.json"))
            .expect("bundled deepseek manifest must exist")
    }

    /// Verifies the recovery chain prefers a validated active layer.
    /// 验证恢复链优先使用已校验的 active 层。
    #[test]
    fn recovery_prefers_validated_active_layer() {
        let (store, _directory) = layout();
        store
            .stage_candidate("deepseek", 2, bundled_deepseek_text().as_bytes())
            .expect("candidate should stage");
        store
            .activate("deepseek", 2)
            .expect("candidate should activate");
        let result =
            load_system_manifest_with_recovery(&store, "deepseek", &bundled_deepseek_text())
                .expect("recovery should resolve");
        assert!(matches!(result, SystemManifestRecovery::Active(_)));
    }

    /// Verifies an invalid active layer is quarantined and the previous layer is restored.
    /// 验证无效 active 层被隔离，且 previous 层被恢复。
    #[test]
    fn recovery_restores_previous_after_invalid_active() {
        let (store, _directory) = layout();
        store
            .stage_candidate("deepseek", 1, bundled_deepseek_text().as_bytes())
            .expect("valid candidate should stage");
        store
            .activate("deepseek", 1)
            .expect("valid candidate should activate");
        // A second activation rotates v1 into previous, giving the recovery chain a layer
        // to restore when the newer active layer is invalid.
        // 第二次激活把 v1 轮换到 previous，为恢复链提供一层可在新 active 层无效时恢复的层。
        store
            .stage_candidate("deepseek", 2, bundled_deepseek_text().as_bytes())
            .expect("second candidate should stage");
        store
            .activate("deepseek", 2)
            .expect("second candidate should activate");
        // Overwrite active with invalid bytes through a direct write to the active path.
        // 通过直接写入 active 路径，用无效字节覆盖 active。
        std::fs::write(
            store.active_path("deepseek"),
            br#"{"schema_version":"broken"}"#,
        )
        .expect("invalid active should write");
        let result =
            load_system_manifest_with_recovery(&store, "deepseek", &bundled_deepseek_text())
                .expect("recovery should resolve");
        assert!(
            matches!(result, SystemManifestRecovery::PreviousRestored(_)),
            "previous layer must restore"
        );
        assert!(
            store
                .quarantine_path("deepseek", 0, "active_invalid")
                .exists(),
            "invalid active bytes must be quarantined"
        );
    }

    /// Verifies both persisted layers failing falls back to the bundled baseline.
    /// 验证两层持久化均失败时回退到内置基线。
    #[test]
    fn recovery_falls_back_to_bundled_baseline() {
        let (store, _directory) = layout();
        let result =
            load_system_manifest_with_recovery(&store, "deepseek", &bundled_deepseek_text())
                .expect("recovery should resolve");
        assert!(
            matches!(result, SystemManifestRecovery::BundledFallback(_)),
            "absent persisted layers must use the bundled baseline"
        );
    }

    /// Verifies untrusted system-provider identifiers cannot escape any store-owned directory.
    /// 验证不可信系统供应商标识无法逃逸任何存储层持有目录。
    #[test]
    fn system_manifest_paths_encode_non_identifier_bytes() {
        let (store, _directory) = layout();
        // hostileId covers both Windows and Unix separators plus a drive-prefix delimiter.
        // hostileId 同时覆盖 Windows、Unix 分隔符与盘符分隔符。
        let hostile_id = "..\\..\\outside/path:C";
        assert_eq!(
            store.active_path(hostile_id).parent(),
            Some(store.system_active_dir().as_path())
        );
        assert_eq!(
            store.previous_path(hostile_id).parent(),
            Some(store.system_previous_dir().as_path())
        );
        assert_eq!(
            store.staging_path(hostile_id, 7).parent(),
            Some(store.system_staging_dir().as_path())
        );
        assert_eq!(
            store.sync_state_path(hostile_id).parent(),
            Some(
                store
                    .manifests_root()
                    .join("system")
                    .join("state")
                    .as_path()
            )
        );
        for path in [
            store.active_path(hostile_id),
            store.previous_path(hostile_id),
            store.staging_path(hostile_id, 7),
            store.sync_state_path(hostile_id),
        ] {
            let file_name = path
                .file_name()
                .and_then(|value| value.to_str())
                .expect("encoded system manifest path should have one UTF-8 file name");
            assert!(!file_name.contains('/'));
            assert!(!file_name.contains('\\'));
            assert!(!file_name.contains(':'));
        }
        assert_eq!(
            store
                .active_path("Provider.v1-test_name")
                .file_name()
                .and_then(|value| value.to_str()),
            Some("Provider.v1-test_name.json"),
            "valid Manifest identifiers must retain existing on-disk names"
        );
    }

    /// Verifies custom manifests round-trip through the filesystem-safe encoded stem.
    /// 验证自定义清单通过文件系统安全编码主干名完成往返。
    #[test]
    fn custom_manifest_round_trips_through_encoded_stem() {
        let (store, _directory) = layout();
        assert_eq!(
            store
                .read_custom_manifest("custom:probe")
                .expect("absent custom manifest should read as None"),
            None
        );
        store
            .write_custom_manifest("custom:probe", br#"{"id":"custom:probe"}"#)
            .expect("custom manifest should write");
        assert_eq!(
            store
                .read_custom_manifest("custom:probe")
                .expect("custom manifest should read"),
            Some(String::from(r#"{"id":"custom:probe"}"#))
        );
        assert_eq!(
            store
                .list_custom_manifest_ids()
                .expect("custom manifest ids should list"),
            vec![String::from("custom:probe")]
        );
    }

    /// Verifies valid custom manifest identifiers stay reversible and case-isolated.
    /// 验证有效自定义清单标识保持可逆且大小写隔离。
    #[test]
    fn custom_manifest_encoding_is_injective_and_safe() {
        let (store, _directory) = layout();
        let tricky_ids = [
            "custom:a.b",
            "custom:a-b",
            "custom:a_b",
            "custom:Foo",
            "custom:foo",
            "custom:100",
        ];
        for provider_id in tricky_ids {
            store
                .write_custom_manifest(
                    provider_id,
                    format!(r#"{{"id":"{provider_id}"}}"#).as_bytes(),
                )
                .expect("tricky custom manifest should write");
        }
        let mut listed = store
            .list_custom_manifest_ids()
            .expect("custom manifest ids should list");
        listed.sort();
        let mut expected = tricky_ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>();
        expected.sort();
        assert_eq!(listed, expected, "every tricky id must round-trip exactly");
        for provider_id in tricky_ids {
            let decoded = decode_custom_provider_file_stem(
                store
                    .custom_manifest_path(provider_id)
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .expect("stem must be utf-8"),
            )
            .expect("stem must decode");
            assert_eq!(decoded, provider_id);
        }
        // case_folded_upper and case_folded_lower model path identity on Windows/macOS defaults.
        // case_folded_upper 与 case_folded_lower 模拟 Windows/macOS 默认文件系统的路径身份。
        let case_folded_upper = store
            .custom_manifest_path("custom:Foo")
            .to_string_lossy()
            .to_ascii_lowercase();
        let case_folded_lower = store
            .custom_manifest_path("custom:foo")
            .to_string_lossy()
            .to_ascii_lowercase();
        assert_ne!(
            case_folded_upper, case_folded_lower,
            "case-insensitive filesystems must retain distinct provider files"
        );
    }

    /// Verifies custom manifest file operations reject invalid and filesystem-unsafe identities.
    /// 验证自定义清单文件操作拒绝无效及文件系统不安全的身份。
    #[test]
    fn custom_manifest_operations_reject_invalid_provider_identities() {
        let (store, _directory) = layout();
        let oversized = format!(
            "custom:{}",
            "a".repeat(
                vulcan_provider_adapters::provider_manifest::MAX_PROVIDER_IDENTIFIER_LEN
                    - "custom:".len()
                    + 1
            )
        );
        for invalid in ["custom:path/escape", "custom:has space", oversized.as_str()] {
            store
                .write_custom_manifest(invalid, br#"{"version":1}"#)
                .expect_err("invalid custom identity must not be written");
            store
                .read_custom_manifest(invalid)
                .expect_err("invalid custom identity must not be read");
            store
                .remove_custom_manifest(invalid)
                .expect_err("invalid custom identity must not be removed");
        }
    }

    /// Verifies custom manifest writes reject oversized bytes and removal is idempotent.
    /// 验证自定义清单写入拒绝超限字节，且删除操作幂等。
    #[test]
    fn custom_manifest_enforces_bound_and_idempotent_removal() {
        let (store, _directory) = layout();
        let oversized = vec![b'x'; (CUSTOM_MANIFEST_MAX_BYTES + 1) as usize];
        let error = store
            .write_custom_manifest("custom:oversized", &oversized)
            .expect_err("oversized custom manifest must be rejected");
        assert!(error.contains("byte bound"), "{error}");
        store
            .remove_custom_manifest("custom:missing")
            .expect("removing an absent custom manifest is a no-op");
        store
            .write_custom_manifest("custom:probe", br#"{"id":"custom:probe"}"#)
            .expect("custom manifest should write");
        store
            .remove_custom_manifest("custom:probe")
            .expect("custom manifest should remove");
        assert_eq!(
            store
                .read_custom_manifest("custom:probe")
                .expect("absent custom manifest should read as None"),
            None
        );
    }

    /// Verifies standalone custom provider discovery rejects an unbounded file population.
    /// 验证独立自定义供应商发现会拒绝无界文件数量。
    #[test]
    fn custom_manifest_listing_enforces_provider_count_limit() {
        let (store, _directory) = layout();
        // files fill the complete reviewed provider capacity plus one adversarial entry.
        // files 填满全部已审核供应商容量，并额外加入一个恶意条目。
        for index in 0..=CUSTOM_MANIFEST_MAX_FILES {
            let provider_id = format!("custom:bounded-{index}");
            store
                .write_custom_manifest(provider_id.as_str(), br#"{"version":1}"#)
                .expect("bounded fixture manifest should write");
        }

        // error proves enumeration fails before returning an oversized retained vector.
        // error 证明枚举会在返回超大保留向量前失败。
        let error = store
            .list_custom_manifest_ids()
            .expect_err("provider count above the limit must fail closed");
        assert!(error.contains("256-provider limit"), "{error}");
    }

    /// Verifies an exact transaction snapshot restores Active, Previous, and state together.
    /// 验证精确事务快照会共同恢复 Active、Previous 与状态。
    #[test]
    fn atomic_system_snapshot_restores_all_store_layers() {
        let (store, _directory) = layout();
        store
            .stage_candidate("probe", 1, b"version-one")
            .and_then(|_| store.activate("probe", 1))
            .expect("first version should activate");
        store
            .stage_candidate("probe", 2, b"version-two")
            .and_then(|_| store.activate("probe", 2))
            .expect("second version should activate");
        store
            .save_sync_state(
                "probe",
                &ManifestSyncState {
                    revision: 2,
                    sha256: "b".repeat(64),
                    previous: Some(ManifestVersionState {
                        revision: 1,
                        sha256: "a".repeat(64),
                    }),
                    etag: None,
                    index_etag: None,
                    last_modified: None,
                    synced_at_unix_ms: 2,
                    last_error_kind: None,
                },
            )
            .expect("second state should save");
        let snapshot = store
            .capture_system_snapshot("probe")
            .expect("snapshot should capture");

        store
            .stage_candidate("probe", 3, b"version-three")
            .and_then(|_| store.activate("probe", 3))
            .expect("third version should activate");
        store
            .save_sync_state(
                "probe",
                &ManifestSyncState {
                    revision: 3,
                    sha256: "c".repeat(64),
                    previous: Some(ManifestVersionState {
                        revision: 2,
                        sha256: "b".repeat(64),
                    }),
                    etag: None,
                    index_etag: None,
                    last_modified: None,
                    synced_at_unix_ms: 3,
                    last_error_kind: None,
                },
            )
            .expect("third state should save");

        store
            .restore_system_snapshot("probe", &snapshot)
            .expect("snapshot should restore");
        assert_eq!(
            store.read_active("probe").expect("active read"),
            Some(String::from("version-two"))
        );
        assert_eq!(
            store.read_previous("probe").expect("previous read"),
            Some(String::from("version-one"))
        );
        match store.load_sync_state("probe").expect("state read") {
            SyncStateLoad::Present(state) => assert_eq!(state.revision, 2),
            SyncStateLoad::Absent => panic!("restored state must exist"),
        }
    }

    /// Verifies startup rolls an interrupted applying transaction back to its exact before-image.
    /// 验证启动会把中断的 applying 事务回滚到精确前镜像。
    #[test]
    fn applying_system_transaction_journal_restores_before_image() {
        let (store, _directory) = layout();
        store
            .stage_candidate("probe", 1, b"version-one")
            .and_then(|_| store.activate("probe", 1))
            .expect("first version should activate");
        // formerCheck is the operator-visible state paired with version one.
        // formerCheck 是与版本一配对的面向操作员状态。
        let former_check = ManifestCheckState {
            checked_at_unix_ms: 10,
            last_successful_at_unix_ms: Some(10),
            outcome: ManifestCheckOutcome::Activated,
            last_error_kind: None,
        };
        store
            .save_check_state("probe", &former_check)
            .expect("former check state should save");
        let snapshot = store
            .capture_system_snapshot("probe")
            .expect("before-image should capture");
        store
            .begin_system_transaction("probe", &snapshot)
            .expect("applying journal should persist");
        store
            .stage_candidate("probe", 2, b"version-two")
            .and_then(|_| store.activate("probe", 2))
            .expect("partial target should write");
        // targetCheck simulates the success record written before the journal commit boundary.
        // targetCheck 模拟日志提交边界前写入的成功记录。
        let target_check = ManifestCheckState {
            checked_at_unix_ms: 20,
            last_successful_at_unix_ms: Some(20),
            outcome: ManifestCheckOutcome::Activated,
            last_error_kind: None,
        };
        store
            .save_check_state("probe", &target_check)
            .expect("target check state should save");

        assert!(
            store
                .recover_system_transaction("probe")
                .expect("interrupted transaction should recover")
        );
        assert_eq!(
            store.read_active("probe").expect("active should read"),
            Some(String::from("version-one"))
        );
        assert_eq!(
            store.read_previous("probe").expect("previous should read"),
            None
        );
        assert_eq!(
            store.load_check_state("probe"),
            Ok(CheckStateLoad::Present(former_check)),
            "Applying recovery must restore the operator-visible before-image"
        );
    }

    /// Verifies a committed journal never rolls an already complete generation backward.
    /// 验证 committed 日志绝不会把已完成代际向后回滚。
    #[test]
    fn committed_system_transaction_journal_preserves_target_generation() {
        let (store, _directory) = layout();
        store
            .stage_candidate("probe", 1, b"version-one")
            .and_then(|_| store.activate("probe", 1))
            .expect("first version should activate");
        // formerCheck is the check-state component of the committed before-image.
        // formerCheck 是已提交前镜像中的检查状态组成部分。
        let former_check = ManifestCheckState {
            checked_at_unix_ms: 10,
            last_successful_at_unix_ms: Some(10),
            outcome: ManifestCheckOutcome::Activated,
            last_error_kind: None,
        };
        store
            .save_check_state("probe", &former_check)
            .expect("former check state should save");
        let snapshot = store
            .capture_system_snapshot("probe")
            .expect("before-image should capture");
        store
            .stage_candidate("probe", 2, b"version-two")
            .and_then(|_| store.activate("probe", 2))
            .expect("target generation should write");
        // targetCheck is the operator-visible state that belongs to the committed generation.
        // targetCheck 是属于已提交代际的面向操作员状态。
        let target_check = ManifestCheckState {
            checked_at_unix_ms: 20,
            last_successful_at_unix_ms: Some(20),
            outcome: ManifestCheckOutcome::Activated,
            last_error_kind: None,
        };
        store
            .save_check_state("probe", &target_check)
            .expect("target check state should save");
        store
            .write_system_transaction_journal(
                "probe",
                SystemManifestTransactionPhase::Committed,
                &snapshot,
            )
            .expect("committed journal should persist");

        assert!(
            store
                .recover_system_transaction("probe")
                .expect("committed journal should clear")
        );
        assert_eq!(
            store.read_active("probe").expect("active should read"),
            Some(String::from("version-two"))
        );
        assert_eq!(
            store.read_previous("probe").expect("previous should read"),
            Some(String::from("version-one"))
        );
        assert_eq!(
            store.load_check_state("probe"),
            Ok(CheckStateLoad::Present(target_check)),
            "Committed recovery must retain the target operator-visible state"
        );
    }

    /// Verifies check-state writes remain independent and restore their exact before-image.
    /// 验证检查状态写入保持独立并恢复其精确前镜像。
    #[test]
    fn manifest_check_state_round_trips_and_restores_exactly() {
        // store owns the isolated temporary check-state directory.
        // store 持有隔离的临时检查状态目录。
        let (store, _directory) = layout();
        // successful is the last completed activation before a later failed check.
        // successful 是后续检查失败前最后一次完成的激活。
        let successful = ManifestCheckState {
            checked_at_unix_ms: 10,
            last_successful_at_unix_ms: Some(10),
            outcome: ManifestCheckOutcome::Activated,
            last_error_kind: None,
        };
        store
            .save_check_state("probe", &successful)
            .expect("successful check state should save");
        // snapshot retains the exact successful representation for transaction compensation.
        // snapshot 为事务补偿保留精确成功表示。
        let snapshot = store
            .capture_check_state("probe")
            .expect("check-state snapshot should capture");
        // failed advances last-checked while deliberately preserving last-successful.
        // failed 推进最近检查时间，同时有意保留最近成功时间。
        let failed = ManifestCheckState {
            checked_at_unix_ms: 20,
            last_successful_at_unix_ms: Some(10),
            outcome: ManifestCheckOutcome::Failed,
            last_error_kind: Some(String::from("projection_failed")),
        };
        store
            .save_check_state("probe", &failed)
            .expect("failed check state should save");
        assert_eq!(
            store.load_check_state("probe"),
            Ok(CheckStateLoad::Present(failed))
        );
        store
            .restore_check_state("probe", &snapshot)
            .expect("check-state before-image should restore");
        assert_eq!(
            store.load_check_state("probe"),
            Ok(CheckStateLoad::Present(successful))
        );
    }

    /// Verifies the check-state contract rejects mixed success/failure semantics and unsafe ids.
    /// 验证检查状态契约拒绝混合成功/失败语义与不安全标识。
    #[test]
    fn manifest_check_state_rejects_invalid_failure_contracts() {
        // store owns the isolated temporary check-state directory.
        // store 持有隔离的临时检查状态目录。
        let (store, _directory) = layout();
        // missingFailure lies about a failed outcome without a stable classification.
        // missingFailure 对缺少稳定分类的失败结果作出错误声明。
        let missing_failure = ManifestCheckState {
            checked_at_unix_ms: 20,
            last_successful_at_unix_ms: Some(10),
            outcome: ManifestCheckOutcome::Failed,
            last_error_kind: None,
        };
        assert!(
            store
                .save_check_state("probe", &missing_failure)
                .expect_err("failed outcome must carry a failure kind")
                .contains("disagree")
        );
        // unsafeFailure attempts to persist an unbounded free-form diagnostic as a category.
        // unsafeFailure 尝试把无界自由文本诊断持久化为分类。
        let unsafe_failure = ManifestCheckState {
            checked_at_unix_ms: 20,
            last_successful_at_unix_ms: Some(10),
            outcome: ManifestCheckOutcome::Rejected,
            last_error_kind: Some(String::from("HTTP 401: Authorization secret")),
        };
        assert!(
            store
                .save_check_state("probe", &unsafe_failure)
                .expect_err("unsafe failure kind must be rejected")
                .contains("lowercase ASCII")
        );
        assert!(
            store
                .sync_check_state_path("../outside")
                .starts_with(store.manifests_root().join("system").join("check-state")),
            "encoded provider ids must stay inside the fixed check-state directory"
        );
    }
}
