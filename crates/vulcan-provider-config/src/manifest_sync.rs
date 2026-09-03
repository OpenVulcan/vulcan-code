//! Signed remote manifest index verification and local sync activation (plan §17.2-17.4).
//! 签名远端清单 Index 校验与本地同步激活（方案 §17.2-17.4）。
//!
//! Production synchronization stays behind the external confirmation gate; this module
//! provides the local architecture and a fully testable sync flow against a local server.
//! 生产同步仍处于外部确认门之后；本模块提供本地架构与可对本地服务器完整测试的同步流程。

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Maximum accepted bytes for one remote index document.
/// 单个远端 Index 文档接受的最大字节数。
pub const REMOTE_INDEX_MAX_BYTES: u64 = 1024 * 1024;
/// Maximum accepted bytes for one remote provider manifest.
/// 单个远端供应商清单接受的最大字节数。
pub const REMOTE_PROVIDER_MAX_BYTES: u64 = 4 * 1024 * 1024;
/// Index schema version accepted by this engine.
/// 本引擎接受的 Index Schema 版本。
pub const INDEX_SCHEMA_VERSION: &str = "vulcan.provider-index.v1";

/// One trusted signing key bundled with the client binary.
/// 随客户端二进制内置的一把受信签名密钥。
#[derive(Debug, Clone)]
pub struct TrustedSigningKey {
    /// Stable key identifier referenced by the index.
    /// 被 Index 引用的稳定密钥标识。
    pub key_id: String,
    /// Ed25519 public key bytes.
    /// Ed25519 公钥字节。
    pub public_key: [u8; 32],
}

/// One remote provider entry pinned by the signed index.
/// 由签名 Index 固定的一个远端供应商条目。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteProviderEntry {
    /// Exact provider identifier.
    /// 精确供应商标识。
    pub provider_id: String,
    /// Provider manifest revision.
    /// 供应商清单修订号。
    pub revision: u64,
    /// Index-relative URL; resolved strictly under the fixed HTTPS origin.
    /// Index 相对 URL；严格在固定 HTTPS 源下解析。
    pub relative_url: String,
    /// SHA-256 hex digest of the exact provider bytes.
    /// 精确供应商字节的 SHA-256 十六进制摘要。
    pub sha256: String,
    /// Exact byte count of the provider file.
    /// 供应商文件的精确字节数。
    pub bytes: u64,
    /// Minimum engine contract required by this provider revision.
    /// 该供应商修订要求的最低引擎契约。
    pub minimum_engine_contract: u64,
}

/// The signed remote index binding every provider JSON path and digest.
/// 绑定全部供应商 JSON 路径与摘要的签名远端 Index。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestRemoteIndex {
    /// Index schema version.
    /// Index Schema 版本。
    pub schema_version: String,
    /// Monotonic index revision.
    /// 单调 Index 修订号。
    pub index_revision: u64,
    /// Unix milliseconds when the index became valid.
    /// Index 生效的 Unix 毫秒时间。
    pub issued_at_unix_ms: i64,
    /// Unix milliseconds after which the index expires.
    /// Index 过期的 Unix 毫秒时间。
    pub expires_at_unix_ms: i64,
    /// Signing key identifier.
    /// 签名密钥标识。
    pub signing_key_id: String,
    /// Provider entries keyed by provider id.
    /// 按供应商 ID 索引的供应商条目。
    pub providers: BTreeMap<String, RemoteProviderEntry>,
}

/// Outcome of one bounded sync attempt for one provider.
/// 一个供应商的一次有界同步尝试结果。
pub enum SyncOutcome {
    /// The remote index pins the already-active revision.
    /// 远端 Index 固定的是已活动修订。
    UpToDate,
    /// The candidate was verified and activated.
    /// 候选已校验并激活。
    Activated {
        /// Activated manifest revision.
        /// 已激活清单修订号。
        revision: u64,
        /// Compiled activation snapshot ready for the runtime hot replacement without a
        /// second compile; carries the exact verified bytes' semantics.
        /// 可直接用于运行时热替换的编译激活快照，无需二次编译；携带精确已验证字节的语义。
        manifest: std::sync::Arc<vulcan_provider_adapters::ResolvedProviderManifest>,
        /// Exact former store bytes retained until runtime publication succeeds.
        /// 在运行时发布成功前保留的精确原存储字节。
        store_snapshot: crate::manifest_store::SystemManifestStoreSnapshot,
    },
    /// The candidate failed verification and was quarantined.
    /// 候选校验失败并被隔离。
    Rejected {
        /// Rejected candidate revision.
        /// 被拒候选修订号。
        revision: u64,
        /// Classified failure kind.
        /// 分类失败类型。
        failure_kind: String,
        /// Bounded compiler-owned field path when schema or semantic validation identified one.
        /// Schema 或语义校验确定字段时的有界编译器权威路径。
        failure_path: Option<String>,
    },
}

/// Stable category for one rejected provider-manifest candidate.
/// 单个被拒供应商清单候选的稳定类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderManifestVerificationFailureKind {
    /// Downloaded bytes disagree with the signed index pin.
    /// 下载字节与签名 Index 固定值不一致。
    Integrity,
    /// The candidate requires a newer engine contract.
    /// 候选需要更新的引擎契约。
    EngineIncompatible,
    /// JSON, schema, or semantic compilation failed.
    /// JSON、Schema 或语义编译失败。
    SchemaSemantic,
    /// Provider identity or manifest revision disagrees with the signed pin.
    /// 供应商身份或清单修订号与签名固定值不一致。
    IdentityMismatch,
    /// The bundled trust baseline is unavailable or invalid.
    /// 随包信任基线不可用或无效。
    TrustBaseline,
    /// The candidate attempts to exceed the bundled authority ceiling.
    /// 候选尝试突破随包权限上限。
    TrustCeiling,
}

impl ProviderManifestVerificationFailureKind {
    /// Returns the stable wire/storage identifier used by quarantine, reports, and UI state.
    /// 返回隔离目录、同步报告与 UI 状态使用的稳定线协议/存储标识。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Integrity => "integrity_failed",
            Self::EngineIncompatible => "engine_incompatible",
            Self::SchemaSemantic => "schema_semantic_failed",
            Self::IdentityMismatch => "identity_mismatch",
            Self::TrustBaseline => "trust_baseline_failed",
            Self::TrustCeiling => "trust_ceiling_failed",
        }
    }
}

/// Classified provider-manifest verification failure with a diagnostic message.
/// 带诊断消息的分类供应商清单校验失败。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderManifestVerificationError {
    /// Stable rejection category.
    /// 稳定拒绝类别。
    kind: ProviderManifestVerificationFailureKind,
    /// Human-readable diagnostic retaining the exact failed invariant.
    /// 保留精确失败不变量的可读诊断。
    message: String,
    /// Bounded compiler-owned JSON/schema path when one exact field failed.
    /// 存在精确字段失败时的有界编译器权威 JSON/Schema 路径。
    failure_path: Option<String>,
}

impl ProviderManifestVerificationError {
    /// Creates one classified verification failure.
    /// 创建一个分类校验失败。
    fn new(kind: ProviderManifestVerificationFailureKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            failure_path: None,
        }
    }

    /// Attaches one compiler-owned path after applying the public diagnostic bound.
    /// 在应用公开诊断边界后附加一条编译器权威路径。
    ///
    /// # Parameters
    /// 参数
    /// `failure_path` is the exact path carried by `ManifestValidationError`.
    /// `failure_path` 是 `ManifestValidationError` 携带的精确路径。
    ///
    /// # Returns
    /// 返回值
    /// The same classified error with a bounded absolute JSON/schema path when valid.
    /// 返回同一分类错误；路径有效时附带有界绝对 JSON/Schema 路径。
    fn with_failure_path(mut self, failure_path: &str) -> Self {
        self.failure_path = bounded_verification_failure_path(failure_path);
        self
    }

    /// Returns the stable failure category.
    /// 返回稳定失败类别。
    pub const fn kind(&self) -> ProviderManifestVerificationFailureKind {
        self.kind
    }

    /// Returns the stable wire/storage identifier.
    /// 返回稳定线协议/存储标识。
    pub const fn failure_kind(&self) -> &'static str {
        self.kind.as_str()
    }

    /// Returns the bounded compiler-owned field path when validation identified one.
    /// 返回校验确定字段时的有界编译器权威路径。
    ///
    /// # Returns
    /// 返回值
    /// An absolute JSON/schema path, or `None` for non-field verification failures.
    /// 返回绝对 JSON/Schema 路径；非字段校验失败返回 `None`。
    pub fn failure_path(&self) -> Option<&str> {
        self.failure_path.as_deref()
    }
}

/// Maximum UTF-8 bytes retained for one structured verification failure path.
/// 单条结构化校验失败路径保留的最大 UTF-8 字节数。
const VERIFICATION_FAILURE_PATH_MAX_BYTES: usize = 512;

/// Bounds one compiler-owned absolute path without parsing or guessing its structure.
/// 在不解析或猜测结构的前提下限制一条编译器权威绝对路径。
///
/// # Parameters
/// 参数
/// `failure_path` is an exact JSON Pointer or root-based schema path.
/// `failure_path` 是精确 JSON Pointer 或根起始 Schema 路径。
///
/// # Returns
/// 返回值
/// A boundary-safe path prefix, or `None` when the compiler supplied no absolute safe path.
/// 返回字符边界安全的路径前缀；编译器未提供绝对安全路径时返回 `None`。
fn bounded_verification_failure_path(failure_path: &str) -> Option<String> {
    if failure_path.is_empty()
        || !(failure_path.starts_with('/') || failure_path.starts_with('$'))
        || failure_path.chars().any(char::is_control)
    {
        return None;
    }
    if failure_path.len() <= VERIFICATION_FAILURE_PATH_MAX_BYTES {
        return Some(failure_path.to_string());
    }
    // boundary retreats to an exact UTF-8 boundary while preserving the absolute root prefix.
    // boundary 后退到精确 UTF-8 边界，同时保留绝对根前缀。
    let mut boundary = VERIFICATION_FAILURE_PATH_MAX_BYTES;
    while boundary > 0 && !failure_path.is_char_boundary(boundary) {
        boundary -= 1;
    }
    Some(failure_path[..boundary].to_string())
}

impl std::fmt::Display for ProviderManifestVerificationError {
    /// Formats the exact diagnostic without losing its stable category.
    /// 格式化精确诊断且不丢失稳定类别。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.failure_kind(), self.message)
    }
}

impl std::error::Error for ProviderManifestVerificationError {}

/// Maximum accepted provider entries inside one remote index.
/// 单个远端 Index 接受的最大供应商条目数。
pub const MAX_INDEX_PROVIDER_ENTRIES: usize = 256;
/// Expected hex length of one SHA-256 digest.
/// 单个 SHA-256 摘要的预期十六进制长度。
pub const SHA256_HEX_LENGTH: usize = 64;
/// Maximum accepted trusted signing keys (bounded verification set, plan §R7.2).
/// 接受的最大受信签名密钥数（有界验签集合，方案 §R7.2）。
pub const MAX_TRUSTED_SIGNING_KEYS: usize = 8;

/// Parses and validates one bounded remote index document.
/// 解析并校验一份有界远端 Index 文档。
///
/// # Parameters
/// 参数
/// `index_bytes` are the exact raw index bytes and `signature` is the detached Ed25519
/// signature over those raw bytes. Strict signature verification over the raw bytes
/// precedes every trust decision: the signing key id is extracted only to select the
/// verifying key, and schema/time-window checks run after verification (plan §17.2).
/// `index_bytes` 是精确原始 Index 字节，`signature` 是对原始字节的分离 Ed25519 签名。
/// 严格验签先于一切信任决策：签名密钥 ID 仅为选择验证密钥而提取，schema/时间窗
/// 校验在验签通过后执行（方案 §17.2）。
///
/// # Returns
/// 返回值
/// The parsed index, or a controlled signature/format failure.
/// 返回解析后的 Index，或受控签名/格式失败。
pub fn verify_and_parse_index(
    index_bytes: &[u8],
    signature: &[u8],
    trusted_keys: &[TrustedSigningKey],
    now_unix_ms: i64,
) -> Result<ManifestRemoteIndex, String> {
    // Pre-verification checks stay byte-level only: no JSON field is trusted before the
    // signature check (plan §R7.1).
    // 验签前校验仅限字节级：验签前不信任任何 JSON 字段（方案 §R7.1）。
    if index_bytes.len() as u64 > REMOTE_INDEX_MAX_BYTES {
        return Err(format!(
            "Remote index exceeds the {REMOTE_INDEX_MAX_BYTES} byte bound."
        ));
    }
    if signature.len() != 64 {
        return Err(String::from(
            "Remote index signature must be exactly 64 Ed25519 bytes.",
        ));
    }
    // The trusted key set is bounded and its identifiers must be unique so exactly one key
    // can match a signed document (plan §R7.2).
    // 受信密钥集有界且标识必须唯一，使恰好一个密钥能匹配签名文档（方案 §R7.2）。
    if trusted_keys.len() > MAX_TRUSTED_SIGNING_KEYS {
        return Err(format!(
            "Trusted signing key set exceeds the {MAX_TRUSTED_SIGNING_KEYS} key bound."
        ));
    }
    for (index, key) in trusted_keys.iter().enumerate() {
        if trusted_keys[..index]
            .iter()
            .any(|other| other.key_id == key.key_id)
        {
            return Err(format!(
                "Trusted signing key id `{}` is duplicated.",
                key.key_id
            ));
        }
    }
    let signature_bytes: [u8; 64] = signature
        .try_into()
        .map_err(|_| String::from("Remote index signature length mismatch."))?;
    // Every bounded trusted key is attempted before parsing JSON. Exactly one key must match;
    // this prevents untrusted bytes from selecting their own verifier and rejects duplicate
    // public keys hidden behind different identifiers (plan §17.2 and repair plan §R7.2).
    // 解析 JSON 前尝试全部有界受信密钥。必须恰好一把密钥匹配；这可防止不可信字节
    // 自行选择验证器，并拒绝隐藏在不同标识后的重复公钥（方案 §17.2、修补方案 §R7.2）。
    let mut matched_key: Option<&TrustedSigningKey> = None;
    for trusted in trusted_keys {
        let verifying_key =
            ed25519_dalek::VerifyingKey::from_bytes(&trusted.public_key).map_err(|error| {
                format!(
                    "Trusted signing key `{}` is invalid: {error}",
                    trusted.key_id
                )
            })?;
        if verifying_key
            .verify_strict(index_bytes, &signature_bytes.into())
            .is_ok()
        {
            if matched_key.is_some() {
                return Err(String::from(
                    "Remote index signature matched more than one trusted signing key.",
                ));
            }
            matched_key = Some(trusted);
        }
    }
    let matched_key =
        matched_key.ok_or_else(|| String::from("Remote index signature verification failed."))?;
    // The raw bytes are now trusted; parse and enforce the bounded contract.
    // 原始字节现已受信；解析并强制执行有界契约。
    let index_value: serde_json::Value = serde_json::from_slice(index_bytes)
        .map_err(|error| format!("Remote index is invalid JSON: {error}"))?;
    vulcan_provider_adapters::provider_manifest::validate_runtime_json_bounds(
        &index_value,
        "Remote index",
    )?;
    let parsed: ManifestRemoteIndex = serde_json::from_value(index_value)
        .map_err(|error| format!("Remote index structure is invalid: {error}"))?;
    if parsed.signing_key_id != matched_key.key_id {
        return Err(format!(
            "Remote index declares signing key `{}` but the signature matched trusted key `{}`.",
            parsed.signing_key_id, matched_key.key_id
        ));
    }
    if parsed.schema_version != INDEX_SCHEMA_VERSION {
        return Err(format!(
            "Remote index schema `{}` is not accepted by this engine.",
            parsed.schema_version
        ));
    }
    if parsed.providers.len() > MAX_INDEX_PROVIDER_ENTRIES {
        return Err(format!(
            "Remote index exceeds the {MAX_INDEX_PROVIDER_ENTRIES} provider entry bound."
        ));
    }
    if now_unix_ms < parsed.issued_at_unix_ms || now_unix_ms > parsed.expires_at_unix_ms {
        return Err(String::from(
            "Remote index is outside its issued/expires time window.",
        ));
    }
    // Every map key must equal its entry's provider id so a forged key can never redirect
    // bytes for one provider into another provider's transaction.
    // 每个 Map 键必须等于其条目的供应商 ID，使伪造键绝不可能把字节重定向到其他
    // 供应商的事务中。
    for (key, entry) in &parsed.providers {
        validate_remote_provider_identity(key)?;
        if key != &entry.provider_id {
            return Err(format!(
                "Remote index key `{key}` does not match entry provider `{}`.",
                entry.provider_id
            ));
        }
        validate_remote_relative_url(&entry.relative_url)?;
        if entry.revision == 0 {
            return Err(format!(
                "Remote index entry `{key}` must have a positive revision."
            ));
        }
        if entry.sha256.len() != SHA256_HEX_LENGTH
            || !entry
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(format!(
                "Remote index entry `{key}` SHA-256 must be exactly {SHA256_HEX_LENGTH} lowercase hex characters."
            ));
        }
        if entry.bytes == 0 || entry.bytes > REMOTE_PROVIDER_MAX_BYTES {
            return Err(format!(
                "Remote index entry `{key}` byte count {} is outside the accepted provider bound.",
                entry.bytes
            ));
        }
    }
    Ok(parsed)
}

/// Validates one signed remote system-provider identity before any local path is constructed.
/// 在构造任何本地路径前校验一个已签名远端系统供应商身份。
///
/// # Parameters
/// 参数
/// `provider_id` is the authenticated Index map key and later becomes a Store filename stem.
/// `provider_id` 是已认证 Index 映射键，之后会成为 Store 文件名主干。
///
/// # Returns
/// 返回值
/// Success only for the shared bounded path-safe ASCII identity contract.
/// 仅对共享有界且路径安全的 ASCII 身份契约返回成功。
fn validate_remote_provider_identity(provider_id: &str) -> Result<(), String> {
    if provider_id.is_empty()
        || provider_id.len()
            > vulcan_provider_adapters::provider_manifest::MAX_PROVIDER_IDENTIFIER_LEN
        || !provider_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(format!(
            "Remote index provider identity must use 1..={} path-safe ASCII bytes.",
            vulcan_provider_adapters::provider_manifest::MAX_PROVIDER_IDENTIFIER_LEN
        ));
    }
    Ok(())
}

/// Validates one index-relative provider URL for same-origin path safety.
/// 校验一条 Index 相对供应商 URL 的同源路径安全。
///
/// # Returns
/// 返回值
/// `Ok(())` only when the URL is an absolute origin-relative path without scheme, host,
/// query, fragment, backslash, or parent-directory traversal.
/// 仅当 URL 是不含 scheme、host、query、fragment、反斜杠或父目录穿越的绝对源相对路径时
/// 返回 `Ok(())`。
pub fn validate_remote_relative_url(relative_url: &str) -> Result<(), String> {
    if relative_url.is_empty() || !relative_url.starts_with('/') || relative_url.starts_with("//") {
        return Err(format!(
            "Remote provider URL `{relative_url}` must be a single-slash absolute origin-relative path."
        ));
    }
    if relative_url.contains('\\')
        || relative_url.contains("://")
        || relative_url.contains('?')
        || relative_url.contains('#')
    {
        return Err(format!(
            "Remote provider URL `{relative_url}` contains a forbidden scheme/query/fragment/backslash component."
        ));
    }
    for segment in relative_url.split('/') {
        // Decode only URL dot markers for traversal detection without accepting a second parser policy.
        // 仅为穿越检测解码 URL 点标记，避免接受第二套解析器策略。
        let normalized_segment = segment.to_ascii_lowercase().replace("%2e", ".");
        if normalized_segment == ".."
            || normalized_segment == "."
            || normalized_segment.contains("%2f")
            || normalized_segment.contains("%5c")
        {
            return Err(format!(
                "Remote provider URL `{relative_url}` must not contain encoded or literal traversal separators."
            ));
        }
    }
    Ok(())
}

/// Resolves one verified index-relative URL strictly under one fixed HTTPS origin.
/// 在一个固定 HTTPS 源下严格解析一条已验证的 Index 相对 URL。
///
/// # Parameters
/// 参数
/// `base_origin` must be a reviewed HTTPS origin (scheme + authority only) and
/// `relative_url` must already pass `validate_remote_relative_url`.
/// `base_origin` 必须是已审核的 HTTPS 源（仅 scheme + authority），`relative_url`
/// 必须已通过 `validate_remote_relative_url`。
///
/// # Returns
/// 返回值
/// The exact joined URL, or a controlled origin failure.
/// 返回精确拼接 URL，或受控源失败。
pub fn resolve_remote_provider_url(
    base_origin: &str,
    relative_url: &str,
) -> Result<String, String> {
    validate_remote_relative_url(relative_url)?;

    // Parse the reviewed origin once so scheme, authority, userinfo, path, query, and fragment
    // checks all use URL semantics instead of string-prefix assumptions.
    // 仅解析一次已审核源，使 scheme、authority、userinfo、path、query 与 fragment 检查
    // 全部使用 URL 语义，而不是字符串前缀假设。
    let parsed_origin = reqwest::Url::parse(base_origin).map_err(|error| {
        format!("Remote provider base origin `{base_origin}` is not a valid URL: {error}")
    })?;
    if parsed_origin.scheme() != "https" {
        return Err(format!(
            "Remote provider base origin `{base_origin}` must use HTTPS."
        ));
    }
    if parsed_origin.host_str().is_none()
        || !parsed_origin.username().is_empty()
        || parsed_origin.password().is_some()
        || base_origin.contains('@')
        || parsed_origin.path() != "/"
        || parsed_origin.query().is_some()
        || parsed_origin.fragment().is_some()
        || base_origin.contains('\\')
    {
        return Err(format!(
            "Remote provider base origin `{base_origin}` must be a bare scheme+authority origin."
        ));
    }

    // Resolve with URL semantics and then prove the result retained the reviewed origin.
    // 使用 URL 语义解析，并再次证明结果保留了已审核源。
    let resolved_url = parsed_origin.join(relative_url).map_err(|error| {
        format!(
            "Remote provider URL `{relative_url}` cannot be resolved under `{base_origin}`: {error}"
        )
    })?;
    if resolved_url.origin() != parsed_origin.origin() {
        return Err(format!(
            "Remote provider URL `{relative_url}` escapes reviewed origin `{base_origin}`."
        ));
    }
    Ok(resolved_url.to_string())
}

/// Verifies that a remote system manifest stays within its bundled trust ceiling.
/// 校验远端系统清单没有超出其随包信任上限。
///
/// # Parameters
/// 参数
/// `bundled` is the software-reviewed baseline and `candidate` is the signed remote document
/// for the same provider. The comparison covers every authority-bearing host, credential,
/// protocol, primitive, upload channel, operation, and provider-tool execution declaration.
/// `bundled` 是软件审核过的基线，`candidate` 是同一供应商的签名远端文档。比较覆盖所有
/// 承载权限的主机、凭据、协议、原语、上传通道、操作与供应商工具执行声明。
///
/// # Returns
/// 返回值
/// Success only when the candidate removes or reuses reviewed authority without expanding it.
/// 仅当候选只删除或复用已审核权限、没有扩大权限时成功。
pub fn validate_bundled_trust_ceiling(
    bundled: &vulcan_provider_adapters::ProviderManifestV1,
    candidate: &vulcan_provider_adapters::ProviderManifestV1,
) -> Result<(), String> {
    if bundled.provider_id != candidate.provider_id {
        return Err(format!(
            "Trust ceiling provider `{}` does not match candidate `{}`.",
            bundled.provider_id, candidate.provider_id
        ));
    }
    if bundled.credential_registration != candidate.credential_registration {
        return Err(format!(
            "Provider `{}` remote manifest changes its bundled credential registration primitive.",
            candidate.provider_id
        ));
    }
    if bundled.display.promotion != candidate.display.promotion {
        return Err(format!(
            "Provider `{}` remote manifest changes its bundled public promotion authority.",
            candidate.provider_id
        ));
    }
    if !unknown_model_policy_is_subset(
        candidate.model_defaults.unknown_model_policy,
        bundled.model_defaults.unknown_model_policy,
    ) {
        return Err(format!(
            "Provider `{}` remote manifest expands its bundled unknown-model binding authority.",
            candidate.provider_id
        ));
    }
    if !catalog_membership_policy_is_subset(
        candidate.model_defaults.catalog_membership_policy,
        bundled.model_defaults.catalog_membership_policy,
    ) {
        return Err(format!(
            "Provider `{}` remote manifest expands its bundled catalog-membership authority.",
            candidate.provider_id
        ));
    }
    validate_required_primitive_subset(
        "protocol profile",
        &candidate.engine_contract.required_protocol_profiles,
        &bundled.engine_contract.required_protocol_profiles,
    )?;
    validate_required_primitive_subset(
        "execution primitive",
        &candidate.engine_contract.required_execution_primitives,
        &bundled.engine_contract.required_execution_primitives,
    )?;
    validate_required_primitive_subset(
        "upload primitive",
        &candidate.engine_contract.required_upload_primitives,
        &bundled.engine_contract.required_upload_primitives,
    )?;
    validate_required_primitive_subset(
        "mapping transformer",
        &candidate.engine_contract.required_transformers,
        &bundled.engine_contract.required_transformers,
    )?;

    for candidate_entry in &candidate.services {
        let bundled_entry = bundled
            .services
            .iter()
            .find(|entry| entry.service_entry_id == candidate_entry.service_entry_id)
            .ok_or_else(|| {
                format!(
                    "Provider `{}` remote manifest adds unreviewed service entry `{}`.",
                    candidate.provider_id, candidate_entry.service_entry_id
                )
            })?;
        validate_service_entry_trust_ceiling(bundled_entry, candidate_entry)?;
    }
    for candidate_channel in &candidate.upload_channels {
        let bundled_channel = bundled
            .upload_channels
            .iter()
            .find(|channel| channel.upload_channel_id == candidate_channel.upload_channel_id)
            .ok_or_else(|| {
                format!(
                    "Provider `{}` remote manifest adds unreviewed upload channel `{}`.",
                    candidate.provider_id, candidate_channel.upload_channel_id
                )
            })?;
        validate_upload_channel_trust_ceiling(bundled_channel, candidate_channel)?;
    }
    for candidate_operation in &candidate.operations {
        let bundled_operation = bundled
            .operations
            .iter()
            .find(|operation| operation.operation_id == candidate_operation.operation_id)
            .ok_or_else(|| {
                format!(
                    "Provider `{}` remote manifest adds unreviewed operation `{}`.",
                    candidate.provider_id, candidate_operation.operation_id
                )
            })?;
        validate_operation_trust_ceiling(bundled_operation, candidate_operation)?;
    }
    for candidate_target in &candidate.tool_targets {
        let bundled_target = bundled
            .tool_targets
            .iter()
            .find(|target| target.target_id == candidate_target.target_id)
            .ok_or_else(|| {
                format!(
                    "Provider `{}` remote manifest adds unreviewed tool target `{}`.",
                    candidate.provider_id, candidate_target.target_id
                )
            })?;
        if candidate_target.service_entry_id != bundled_target.service_entry_id
            || candidate_target.upstream_target_id != bundled_target.upstream_target_id
            || candidate_target.description != bundled_target.description
            || candidate_target.tool_id != bundled_target.tool_id
            || candidate_target.capability != bundled_target.capability
            || candidate_target.input_schema != bundled_target.input_schema
            || candidate_target.input_numeric_product_ranges
                != bundled_target.input_numeric_product_ranges
            || candidate_target.output != bundled_target.output
            || candidate_target.execution != bundled_target.execution
            || !credential_profile_scope_is_subset(
                &candidate_target.credential_profile_ids,
                &bundled_target.credential_profile_ids,
            )
        {
            return Err(format!(
                "Provider `{}` tool target `{}` expands its bundled identity, contract, or execution authority.",
                candidate.provider_id, candidate_target.target_id
            ));
        }
    }
    for candidate_template in &candidate.tool_target_templates {
        let bundled_template = bundled
            .tool_target_templates
            .iter()
            .find(|template| template.template_id == candidate_template.template_id)
            .ok_or_else(|| {
                format!(
                    "Provider `{}` remote manifest adds unreviewed tool-target template `{}`.",
                    candidate.provider_id, candidate_template.template_id
                )
            })?;
        if candidate_template != bundled_template {
            return Err(format!(
                "Provider `{}` tool-target template `{}` changes bundled execution authority.",
                candidate.provider_id, candidate_template.template_id
            ));
        }
    }
    for candidate_binding in &candidate.tool_target_template_bindings {
        // bundled_binding identifies a previously reviewed data binding with the same model identity.
        // bundled_binding 标识具有相同模型身份、此前已审核的数据绑定。
        let bundled_binding = bundled
            .tool_target_template_bindings
            .iter()
            .find(|binding| {
                binding.service_entry_id == candidate_binding.service_entry_id
                    && binding.model_id == candidate_binding.model_id
            });
        if let Some(bundled_binding) = bundled_binding {
            if !sequence_is_subset(
                &candidate_binding.template_refs,
                &bundled_binding.template_refs,
            ) {
                return Err(format!(
                    "Provider `{}` model `{}` attaches an unreviewed target template.",
                    candidate.provider_id, candidate_binding.model_id
                ));
            }
            continue;
        }
    }
    Ok(())
}

/// Returns whether a candidate unknown-model policy is no broader than the bundled policy.
/// 返回候选未知模型策略是否不宽于随包策略。
///
/// # Parameters
/// 参数
/// `candidate` is the signed remote policy and `bundled` is the reviewed authority ceiling.
/// `candidate` 是签名远端策略，`bundled` 是经过审核的权限上限。
///
/// # Returns
/// 返回值
/// `true` when unmatched models gain no fallback binding authority; otherwise `false`.
/// 未命中模型没有获得新的兜底绑定权限时返回 `true`，否则返回 `false`。
fn unknown_model_policy_is_subset(
    candidate: vulcan_provider_adapters::UnknownModelPolicy,
    bundled: vulcan_provider_adapters::UnknownModelPolicy,
) -> bool {
    matches!(
        (candidate, bundled),
        (
            vulcan_provider_adapters::UnknownModelPolicy::DeclaredRuleRequired,
            _
        ) | (
            vulcan_provider_adapters::UnknownModelPolicy::DefaultBindingAllowed,
            vulcan_provider_adapters::UnknownModelPolicy::DefaultBindingAllowed
        )
    )
}

/// Returns whether candidate catalog evidence sources are a subset of bundled sources.
/// 返回候选目录证据来源是否为随包来源的子集。
///
/// # Parameters
/// 参数
/// `candidate` is the signed remote membership policy and `bundled` is the reviewed ceiling.
/// `candidate` 是签名远端成员策略，`bundled` 是经过审核的上限。
///
/// # Returns
/// 返回值
/// `true` when the candidate preserves or removes admission sources; otherwise `false`.
/// 候选保持或移除准入来源时返回 `true`，否则返回 `false`。
fn catalog_membership_policy_is_subset(
    candidate: vulcan_provider_adapters::CatalogMembershipPolicy,
    bundled: vulcan_provider_adapters::CatalogMembershipPolicy,
) -> bool {
    use vulcan_provider_adapters::CatalogMembershipPolicy::{
        DeclaredAndDiscovery, DeclaredOnly, DiscoveryEvidence,
    };

    matches!(
        (candidate, bundled),
        (DeclaredOnly, DeclaredOnly | DeclaredAndDiscovery)
            | (DiscoveryEvidence, DiscoveryEvidence | DeclaredAndDiscovery)
            | (DeclaredAndDiscovery, DeclaredAndDiscovery)
    )
}

/// Verifies that candidate primitive identifiers are a subset of the bundled identifiers.
/// 校验候选原语标识是随包标识集合的子集。
fn validate_required_primitive_subset(
    kind: &str,
    candidate: &[String],
    bundled: &[String],
) -> Result<(), String> {
    if let Some(value) = candidate.iter().find(|value| !bundled.contains(value)) {
        return Err(format!(
            "Remote manifest requires unreviewed {kind} `{value}` outside the bundled trust ceiling."
        ));
    }
    Ok(())
}

/// Verifies authority-bearing fields of one service entry and all nested credentials/routes.
/// 校验一个服务入口及其嵌套凭据与路由中承载权限的字段。
fn validate_service_entry_trust_ceiling(
    bundled: &vulcan_provider_adapters::ManifestServiceEntry,
    candidate: &vulcan_provider_adapters::ManifestServiceEntry,
) -> Result<(), String> {
    // A remote update may remove a reviewed session header, but it may not introduce or retarget
    // the dynamic session scope to a new header name.
    // 远端更新可以移除已审核的会话请求头，但不能新增或把动态会话作用域改投到新请求头名称。
    let session_header_expands = candidate
        .session_header
        .as_ref()
        .is_some_and(|candidate_header| bundled.session_header.as_ref() != Some(candidate_header));
    if candidate.kind != bundled.kind
        || candidate.base_url != bundled.base_url
        || candidate.default_authentication != bundled.default_authentication
        || candidate.request_projector != bundled.request_projector
        || candidate.service_origin_override_policy != bundled.service_origin_override_policy
        || candidate.native_inference_bindings != bundled.native_inference_bindings
        || session_header_expands
        || !map_is_exact_subset(&candidate.request_headers, &bundled.request_headers)
    {
        return Err(format!(
            "Service entry `{}` expands its bundled host, authentication, header, session-header, or projector authority.",
            candidate.service_entry_id
        ));
    }
    // Catalog identity is data-only and is the explicit hot-update surface for newly opened
    // models. The trust ceiling freezes only discovery authority: source policy, discovery
    // primitive, and path. Compiler limits still bound the candidate model inventory.
    // 目录身份属于纯数据，也是新开放模型的显式热更新面。信任上限只冻结发现权限：
    // 来源策略、发现原语与路径；编译器限制仍会约束候选模型清单。
    let model_discovery_authority_changed = match (
        bundled.model_catalog.as_ref(),
        candidate.model_catalog.as_ref(),
    ) {
        (None, None) => false,
        (Some(bundled_catalog), Some(candidate_catalog)) => {
            candidate_catalog.source_policy != bundled_catalog.source_policy
                || candidate_catalog.discovery_kind != bundled_catalog.discovery_kind
                || candidate_catalog.path != bundled_catalog.path
        }
        (None, Some(_)) | (Some(_), None) => true,
    };
    if model_discovery_authority_changed {
        return Err(format!(
            "Service entry `{}` changes its bundled model-discovery authority.",
            candidate.service_entry_id
        ));
    }
    for candidate_profile in &candidate.credential_profiles {
        let bundled_profile = bundled
            .credential_profiles
            .iter()
            .find(|profile| {
                profile.credential_profile_id == candidate_profile.credential_profile_id
            })
            .ok_or_else(|| {
                format!(
                    "Service entry `{}` adds unreviewed credential profile `{}`.",
                    candidate.service_entry_id, candidate_profile.credential_profile_id
                )
            })?;
        if candidate_profile.credential_mode != bundled_profile.credential_mode
            || candidate_profile.authorization != bundled_profile.authorization
            || candidate_profile.account_usage_query != bundled_profile.account_usage_query
            || candidate_profile.authentication != bundled_profile.authentication
            || candidate_profile.key_prefix != bundled_profile.key_prefix
            || candidate_profile.credential_role != bundled_profile.credential_role
        {
            return Err(format!(
                "Credential profile `{}` expands its bundled credential authority.",
                candidate_profile.credential_profile_id
            ));
        }
    }
    for candidate_route in &candidate.protocol_routes {
        let bundled_route = bundled
            .protocol_routes
            .iter()
            .find(|route| route.protocol_profile_id == candidate_route.protocol_profile_id)
            .ok_or_else(|| {
                format!(
                    "Service entry `{}` adds unreviewed protocol route `{}`.",
                    candidate.service_entry_id, candidate_route.protocol_profile_id
                )
            })?;
        if candidate_route.endpoint_path != bundled_route.endpoint_path
            || candidate_route.base_url_override != bundled_route.base_url_override
            || candidate_route.authentication != bundled_route.authentication
            || candidate_route.model_discovery != bundled_route.model_discovery
            || candidate_route.model_discovery_path != bundled_route.model_discovery_path
            || candidate_route.model_discovery_base_url_override
                != bundled_route.model_discovery_base_url_override
            || candidate_route.responses_features != bundled_route.responses_features
            || !map_is_exact_subset(
                &candidate_route.request_headers,
                &bundled_route.request_headers,
            )
            || !sequence_is_subset(
                &candidate_route.provider_request_id_headers,
                &bundled_route.provider_request_id_headers,
            )
            || (candidate_route.verified && !bundled_route.verified)
            || !input_resource_constraints_not_expanded(
                &candidate_route.input_resource_constraints,
                &bundled_route.input_resource_constraints,
            )
        {
            return Err(format!(
                "Protocol route `{}` expands its bundled endpoint, authentication, header, discovery, verification, diagnostic, or transport authority.",
                candidate_route.protocol_profile_id
            ));
        }
    }
    Ok(())
}

/// Reports whether candidate route resource constraints preserve or narrow bundled authority.
/// 报告候选路由资源约束是否保持或收紧随包权限。
fn input_resource_constraints_not_expanded(
    candidate: &[vulcan_provider_adapters::InputResourceConstraint],
    bundled: &[vulcan_provider_adapters::InputResourceConstraint],
) -> bool {
    if candidate.iter().any(|candidate_constraint| {
        !bundled.iter().any(|bundled_constraint| {
            bundled_constraint.resource_kind == candidate_constraint.resource_kind
        })
    }) {
        return false;
    }
    bundled.iter().all(|bundled_constraint| {
        let Some(candidate_constraint) = candidate.iter().find(|candidate_constraint| {
            candidate_constraint.resource_kind == bundled_constraint.resource_kind
        }) else {
            return false;
        };
        let mime_types_not_expanded = bundled_constraint.accepted_mime_types.is_empty()
            || (!candidate_constraint.accepted_mime_types.is_empty()
                && sequence_is_subset(
                    &candidate_constraint.accepted_mime_types,
                    &bundled_constraint.accepted_mime_types,
                ));
        let reference_policy_not_expanded = match bundled_constraint.reference_policy {
            vulcan_provider_adapters::ResourceReferencePolicy::Any => true,
            vulcan_provider_adapters::ResourceReferencePolicy::LocalOnly => {
                candidate_constraint.reference_policy
                    == vulcan_provider_adapters::ResourceReferencePolicy::LocalOnly
            }
            vulcan_provider_adapters::ResourceReferencePolicy::RemoteOnly => {
                candidate_constraint.reference_policy
                    == vulcan_provider_adapters::ResourceReferencePolicy::RemoteOnly
            }
        };
        mime_types_not_expanded
            && sequence_is_subset(
                &candidate_constraint.upload_channel_refs,
                &bundled_constraint.upload_channel_refs,
            )
            && optional_limit_not_expanded(
                candidate_constraint.max_bytes,
                bundled_constraint.max_bytes,
            )
            && optional_limit_not_expanded(
                candidate_constraint.max_count,
                bundled_constraint.max_count,
            )
            && reference_policy_not_expanded
    })
}

/// Verifies one upload channel cannot contact a new endpoint or relax reviewed limits.
/// 校验一个上传通道不能联系新端点或放宽已审核限制。
fn validate_upload_channel_trust_ceiling(
    bundled: &vulcan_provider_adapters::UploadChannelDeclaration,
    candidate: &vulcan_provider_adapters::UploadChannelDeclaration,
) -> Result<(), String> {
    if candidate.primitive != bundled.primitive
        || candidate.signing_primitive != bundled.signing_primitive
        || candidate.endpoint_host != bundled.endpoint_host
        || candidate.endpoint_path != bundled.endpoint_path
        || candidate.method != bundled.method
        || candidate.credential_role != bundled.credential_role
        || candidate.file_field != bundled.file_field
        || candidate.response_id_pointer != bundled.response_id_pointer
        || candidate.response_id_encoding != bundled.response_id_encoding
        || candidate.response_url_pointer != bundled.response_url_pointer
        || candidate.reference_template != bundled.reference_template
        || candidate.reference_encoding != bundled.reference_encoding
        || !host_allowlist_scope_is_subset(
            &candidate.response_host_suffixes,
            &bundled.response_host_suffixes,
        )
        || !host_allowlist_scope_is_subset(
            &candidate.remote_url_host_suffixes,
            &bundled.remote_url_host_suffixes,
        )
        || !wildcard_empty_sequence_is_subset(
            &candidate.accepted_mime_types,
            &bundled.accepted_mime_types,
        )
        || !optional_limit_not_expanded(candidate.max_bytes, bundled.max_bytes)
        || !optional_limit_not_expanded(candidate.max_count, bundled.max_count)
        || !optional_limit_not_expanded(candidate.timeout_ms, bundled.timeout_ms)
        || !optional_cache_duration_not_expanded(
            candidate.cache_duration_ms,
            bundled.cache_duration_ms,
        )
        || !map_is_exact_subset(&candidate.fixed_fields, &bundled.fixed_fields)
        || (candidate.progress_supported && !bundled.progress_supported)
        || (candidate.cancellation_supported && !bundled.cancellation_supported)
    {
        return Err(format!(
            "Upload channel `{}` expands its bundled endpoint, credential, primitive, or resource authority.",
            candidate.upload_channel_id
        ));
    }
    Ok(())
}

/// Verifies one operation reuses only bundled carriers, references, and execution bounds.
/// 校验一个操作只复用随包载体、引用和执行边界。
fn validate_operation_trust_ceiling(
    bundled: &vulcan_provider_adapters::OperationDeclaration,
    candidate: &vulcan_provider_adapters::OperationDeclaration,
) -> Result<(), String> {
    let async_not_expanded = match (&candidate.async_execution, &bundled.async_execution) {
        (None, _) => true,
        (Some(_), None) => false,
        (Some(candidate_async), Some(bundled_async)) => {
            // A remote lifecycle may poll less often or stop sooner, but it may never reduce the
            // failure backoff ceiling and thereby issue more observation requests than bundled.
            // 远端生命周期可以降低轮询频率或更早停止，但绝不能降低失败退避上限，进而发出比
            // 随包基线更多的观测请求。
            candidate_async.poll_interval_ms >= bundled_async.poll_interval_ms
                && candidate_async.max_backoff_ms >= bundled_async.max_backoff_ms
                && candidate_async.timeout_ms <= bundled_async.timeout_ms
                && (!candidate_async.cancellation_supported || bundled_async.cancellation_supported)
                && candidate_async.generic_json == bundled_async.generic_json
        }
    };
    if candidate.operation != bundled.operation
        || candidate.carrier != bundled.carrier
        || !sequence_is_subset(&candidate.input_resources, &bundled.input_resources)
        || !sequence_is_subset(&candidate.output_resources, &bundled.output_resources)
        || !sequence_is_subset(&candidate.tool_target_refs, &bundled.tool_target_refs)
        || !sequence_is_subset(
            &candidate.tool_target_template_refs,
            &bundled.tool_target_template_refs,
        )
        || !sequence_is_subset(&candidate.upload_channel_refs, &bundled.upload_channel_refs)
        || !sequence_is_subset(&candidate.upload_bindings, &bundled.upload_bindings)
        || !sequence_is_subset(
            &candidate.request_profile_refs,
            &bundled.request_profile_refs,
        )
        || !sequence_is_subset(
            &candidate.response_profile_refs,
            &bundled.response_profile_refs,
        )
        || !optional_limit_not_expanded(candidate.timeout_ms, bundled.timeout_ms)
        || (candidate.idempotent && !bundled.idempotent)
        || !async_not_expanded
    {
        return Err(format!(
            "Operation `{}` expands its bundled action, carrier, resource binding, reference, retry, polling, or timeout authority.",
            candidate.operation_id
        ));
    }
    Ok(())
}

/// Reports whether every candidate sequence value exists in the bundled sequence.
/// 报告候选序列的每个值是否都存在于随包序列。
fn sequence_is_subset<T: PartialEq>(candidate: &[T], bundled: &[T]) -> bool {
    candidate.iter().all(|value| bundled.contains(value))
}

/// Reports whether every candidate exact-host or DNS-suffix matcher narrows bundled authority.
/// 报告每个候选精确宿主或 DNS 后缀匹配项是否收紧随包权限。
///
/// # Parameters
/// 参数
/// `candidate` and `bundled` contain validated exact hosts or dot-prefixed strict-subdomain
/// suffixes. An empty list authorizes no host.
/// `candidate` 与 `bundled` 包含已校验的精确宿主或点前缀严格子域后缀；空列表不授权任何宿主。
///
/// # Returns
/// 返回值
/// `true` only when every host admitted by a candidate matcher was already admitted by one
/// bundled matcher.
/// 仅当每个候选匹配项允许的全部宿主都已被某个随包匹配项允许时返回 `true`。
fn host_allowlist_scope_is_subset(candidate: &[String], bundled: &[String]) -> bool {
    candidate.iter().all(|candidate_matcher| {
        bundled.iter().any(|bundled_matcher| {
            candidate_matcher == bundled_matcher
                || (bundled_matcher.starts_with('.')
                    && candidate_matcher.len() > bundled_matcher.len()
                    && candidate_matcher.ends_with(bundled_matcher))
        })
    })
}

/// Reports whether a sequence with empty-as-wildcard semantics preserves or narrows authority.
/// 报告具有“空列表即通配”语义的序列是否保持或收紧权限。
///
/// # Parameters
/// 参数
/// `candidate` and `bundled` are allowlists whose empty value means every value, so an empty
/// candidate cannot be treated as an ordinary mathematical subset of a bounded baseline.
/// `candidate` 与 `bundled` 是空值表示全部值的允许列表，因此空候选不能被当作有界基线的
/// 普通数学子集。
///
/// # Returns
/// 返回值
/// `true` when the bundled scope was already open or the candidate remains non-empty and bounded.
/// 当随包作用域原本已开放，或候选保持非空且受限时返回 `true`。
fn wildcard_empty_sequence_is_subset<T: PartialEq>(candidate: &[T], bundled: &[T]) -> bool {
    bundled.is_empty() || (!candidate.is_empty() && sequence_is_subset(candidate, bundled))
}

/// Reports whether a candidate credential scope preserves or narrows bundled access.
/// 报告候选凭据作用域是否保持或收紧随包访问权限。
///
/// # Parameters
/// 参数
/// `candidate` and `bundled` are credential-profile identifiers where an empty set means all
/// profiles, so ordinary mathematical subset semantics would invert the authority meaning.
/// `candidate` 与 `bundled` 是凭据配置档标识；空集合表示全部配置档，因此普通数学子集语义会
/// 颠倒权限含义。
///
/// # Returns
/// 返回值
/// `true` only when the candidate cannot execute under a profile excluded by the bundled scope.
/// 仅当候选不能使用随包作用域排除的配置档执行时返回 `true`。
fn credential_profile_scope_is_subset(
    candidate: &BTreeSet<String>,
    bundled: &BTreeSet<String>,
) -> bool {
    bundled.is_empty() || (!candidate.is_empty() && candidate.is_subset(bundled))
}

/// Reports whether every candidate map entry exists unchanged in the bundled map.
/// 报告候选映射的每个条目是否都原样存在于随包映射。
fn map_is_exact_subset(
    candidate: &BTreeMap<String, String>,
    bundled: &BTreeMap<String, String>,
) -> bool {
    candidate
        .iter()
        .all(|(key, value)| bundled.get(key) == Some(value))
}

/// Reports whether an optional numeric limit is equal to or stricter than its bundled limit.
/// 报告可选数值限制是否等于或严于随包限制。
fn optional_limit_not_expanded(candidate: Option<u64>, bundled: Option<u64>) -> bool {
    match (candidate, bundled) {
        (None, None) | (Some(_), None) => true,
        (None, Some(_)) => false,
        (Some(candidate), Some(bundled)) => candidate <= bundled,
    }
}

/// Reports whether an optional cache lifetime stays disabled or becomes no longer than bundled.
/// 报告可选缓存生命周期是否保持禁用或不长于随包值。
///
/// # Parameters
/// 参数
/// `candidate` and `bundled` are cache lifetimes where absence means caching is disabled.
/// `candidate` 与 `bundled` 是缓存生命周期，其中缺省表示禁用缓存。
///
/// # Returns
/// 返回值
/// True when the candidate cannot retain provider resources longer than the bundled policy.
/// 候选不能比随包策略更久地保留供应商资源时返回 true。
fn optional_cache_duration_not_expanded(candidate: Option<u64>, bundled: Option<u64>) -> bool {
    match (candidate, bundled) {
        (None, _) => true,
        (Some(_), None) => false,
        (Some(candidate), Some(bundled)) => candidate <= bundled,
    }
}

/// Verifies one downloaded provider manifest against its index pin.
/// 校验一份已下载供应商清单与其 Index 固定值。
///
/// # Parameters
/// 参数
/// `entry` is the signed index pin and `bytes` are the exact downloaded bytes.
/// `entry` 是签名 Index 固定值，`bytes` 是精确下载字节。
///
/// # Returns
/// 返回值
/// The compiled snapshot when the byte count, SHA-256 digest, engine contract, compilation,
/// and the manifest's own provider id all agree.
/// 当字节数、SHA-256 摘要、引擎契约、编译与清单自身供应商 ID 全部一致时返回编译快照。
pub fn verify_provider_bytes(
    entry: &RemoteProviderEntry,
    bytes: &[u8],
) -> Result<
    std::sync::Arc<vulcan_provider_adapters::ResolvedProviderManifest>,
    ProviderManifestVerificationError,
> {
    if bytes.len() as u64 != entry.bytes {
        return Err(ProviderManifestVerificationError::new(
            ProviderManifestVerificationFailureKind::Integrity,
            format!(
                "Provider `{}` byte count {} does not match the index pin {}.",
                entry.provider_id,
                bytes.len(),
                entry.bytes
            ),
        ));
    }
    let digest = {
        use sha2::Digest;
        sha2::Sha256::digest(bytes)
    };
    let digest_hex = hex::encode(digest);
    if digest_hex != entry.sha256 {
        return Err(ProviderManifestVerificationError::new(
            ProviderManifestVerificationFailureKind::Integrity,
            format!(
                "Provider `{}` SHA-256 does not match the index pin.",
                entry.provider_id
            ),
        ));
    }
    // Compilation doubles as schema/semantic validation; the compiled value also proves the
    // manifest's own provider id equals the index pin so identity can never be redirected.
    // 编译兼任 schema/语义校验；编译值同时证明清单自身供应商 ID 等于 Index 固定值，
    // 使身份绝不可能被重定向。
    let candidate_text = std::str::from_utf8(bytes).map_err(|_| {
        ProviderManifestVerificationError::new(
            ProviderManifestVerificationFailureKind::SchemaSemantic,
            format!("Provider `{}` is not valid UTF-8.", entry.provider_id),
        )
    })?;
    let engine_contract = vulcan_provider_adapters::provider_manifest::CURRENT_MANIFEST_ENGINE;
    if entry.minimum_engine_contract > engine_contract as u64 {
        return Err(ProviderManifestVerificationError::new(
            ProviderManifestVerificationFailureKind::EngineIncompatible,
            format!(
                "Provider `{}` requires engine contract {} but this client supports {}. Upgrade the client before activating this manifest.",
                entry.provider_id, entry.minimum_engine_contract, engine_contract
            ),
        ));
    }
    let compiled = verify_system_manifest_text_against_bundled_classified(
        &entry.provider_id,
        candidate_text,
        vulcan_provider_adapters::ManifestSourceLayer::Active,
    )?;
    if compiled.revision != entry.revision {
        return Err(ProviderManifestVerificationError::new(
            ProviderManifestVerificationFailureKind::IdentityMismatch,
            format!(
                "Provider `{}` manifest revision {} does not match the signed index revision {}.",
                entry.provider_id, compiled.revision, entry.revision
            ),
        ));
    }
    Ok(compiled)
}

/// Compiles one stored system manifest and verifies it against the bundled trust ceiling.
/// 编译一份已存储系统清单并依据随包信任上限进行校验。
///
/// # Parameters
/// 参数
/// `provider_id` is the exact bundled identity, `text` contains the candidate bytes, and
/// `source_layer` records whether the bytes came from Active or Previous storage.
/// `provider_id` 是精确随包身份，`text` 包含候选字节，`source_layer` 记录字节来自 Active
/// 还是 Previous 存储层。
///
/// # Returns
/// 返回值
/// The compiled immutable manifest only when identity, semantics, and authority all verify.
/// 仅在身份、语义与权限全部通过校验时返回编译后的不可变清单。
pub fn verify_system_manifest_text_against_bundled(
    provider_id: &str,
    text: &str,
    source_layer: vulcan_provider_adapters::ManifestSourceLayer,
) -> Result<std::sync::Arc<vulcan_provider_adapters::ResolvedProviderManifest>, String> {
    verify_system_manifest_text_against_bundled_classified(provider_id, text, source_layer)
        .map_err(|error| error.to_string())
}

/// Compiles and trust-checks one system manifest while preserving a stable failure category.
/// 编译并校验一份系统清单的信任边界，同时保留稳定失败类别。
///
/// # Parameters
/// 参数
/// `provider_id`, `text`, and `source_layer` have the same exact semantics as the public
/// string-error wrapper.
/// `provider_id`、`text` 与 `source_layer` 与公开字符串错误包装函数具有完全相同的语义。
///
/// # Returns
/// 返回值
/// The immutable compiled manifest or one classified verification failure.
/// 返回不可变编译清单，或一个分类校验失败。
fn verify_system_manifest_text_against_bundled_classified(
    provider_id: &str,
    text: &str,
    source_layer: vulcan_provider_adapters::ManifestSourceLayer,
) -> Result<
    std::sync::Arc<vulcan_provider_adapters::ResolvedProviderManifest>,
    ProviderManifestVerificationError,
> {
    let compiled = vulcan_provider_adapters::compile_manifest_text_with_layer(
        text,
        vulcan_provider_adapters::ManifestOwnership::System,
        source_layer,
    )
    .map_err(|error| {
        // failurePath is the exact compiler-owned candidate field path.
        // failurePath 是精确的编译器权威候选字段路径。
        let failure_path = error.path.clone();
        ProviderManifestVerificationError::new(
            ProviderManifestVerificationFailureKind::SchemaSemantic,
            format!("Provider `{provider_id}` failed to compile: {error}"),
        )
        .with_failure_path(failure_path.as_str())
    })?;
    if compiled.descriptor.id != provider_id {
        return Err(ProviderManifestVerificationError::new(
            ProviderManifestVerificationFailureKind::IdentityMismatch,
            format!(
                "Provider bytes declare provider `{}` but storage expects `{provider_id}`.",
                compiled.descriptor.id
            ),
        ));
    }
    // Every non-bundled system layer is constrained by the exact software-reviewed source.
    // 每个非随包系统层都受精确的软件审核源约束。
    let bundled_bytes =
        vulcan_provider_adapters::provider_manifest::read_bundled_provider_manifest_bytes(
            provider_id,
        )
        .map_err(|error| {
            ProviderManifestVerificationError::new(
                ProviderManifestVerificationFailureKind::TrustBaseline,
                format!(
                    "Provider `{provider_id}` has no readable bounded bundled trust baseline: {error}"
                ),
            )
        })?;
    let bundled_text = std::str::from_utf8(&bundled_bytes).map_err(|error| {
        ProviderManifestVerificationError::new(
            ProviderManifestVerificationFailureKind::TrustBaseline,
            format!("Provider `{provider_id}` bundled trust baseline is not UTF-8: {error}"),
        )
    })?;
    vulcan_provider_adapters::compile_manifest_text_with_layer(
        bundled_text,
        vulcan_provider_adapters::ManifestOwnership::System,
        vulcan_provider_adapters::ManifestSourceLayer::Bundled,
    )
    .map_err(|error| {
        // failurePath is the exact compiler-owned bundled-baseline field path.
        // failurePath 是精确的编译器权威内置基线路径。
        let failure_path = error.path.clone();
        ProviderManifestVerificationError::new(
            ProviderManifestVerificationFailureKind::TrustBaseline,
            format!(
                "Provider `{}` bundled trust baseline failed to compile: {error}",
                provider_id
            ),
        )
        .with_failure_path(failure_path.as_str())
    })?;
    let bundled_document: vulcan_provider_adapters::ProviderManifestV1 =
        serde_json::from_str(bundled_text).map_err(|error| {
            ProviderManifestVerificationError::new(
                ProviderManifestVerificationFailureKind::TrustBaseline,
                format!(
                    "Provider `{}` bundled trust baseline is invalid JSON: {error}",
                    provider_id
                ),
            )
        })?;
    let candidate_document: vulcan_provider_adapters::ProviderManifestV1 =
        serde_json::from_str(text).map_err(|error| {
            ProviderManifestVerificationError::new(
                ProviderManifestVerificationFailureKind::SchemaSemantic,
                format!(
                    "Provider `{}` candidate is invalid JSON after compilation: {error}",
                    provider_id
                ),
            )
        })?;
    validate_bundled_trust_ceiling(&bundled_document, &candidate_document).map_err(|error| {
        ProviderManifestVerificationError::new(
            ProviderManifestVerificationFailureKind::TrustCeiling,
            error,
        )
    })?;
    Ok(std::sync::Arc::new(compiled))
}

/// Revalidates one local Active/Previous document against its persisted content identity.
/// 依据持久化内容身份重新校验一份本地 Active/Previous 文档。
///
/// # Parameters
/// 参数
/// `provider_id`, `text`, and `source_layer` identify the stored document; `expected` is the
/// revision and digest authenticated by the prior signed activation transaction.
/// `provider_id`、`text` 与 `source_layer` 标识已存储文档；`expected` 是先前签名激活事务
/// 认证的修订号与摘要。
///
/// # Returns
/// 返回值
/// The compiled manifest when digest, revision, identity, semantics, and trust ceiling agree.
/// 当摘要、修订、身份、语义与信任上限一致时返回编译清单。
pub fn verify_stored_system_manifest(
    provider_id: &str,
    text: &str,
    source_layer: vulcan_provider_adapters::ManifestSourceLayer,
    expected: &crate::manifest_store::ManifestVersionState,
) -> Result<std::sync::Arc<vulcan_provider_adapters::ResolvedProviderManifest>, String> {
    use sha2::Digest;
    let actual_sha256 = hex::encode(sha2::Sha256::digest(text.as_bytes()));
    if actual_sha256 != expected.sha256 {
        return Err(format!(
            "Provider `{provider_id}` stored SHA-256 does not match its authenticated state."
        ));
    }
    let compiled = verify_system_manifest_text_against_bundled(provider_id, text, source_layer)?;
    if compiled.revision != expected.revision {
        return Err(format!(
            "Provider `{provider_id}` stored revision {} does not match authenticated revision {}.",
            compiled.revision, expected.revision
        ));
    }
    Ok(compiled)
}

/// Returns the current unix milliseconds, or zero when the system clock is unavailable.
/// 返回当前 Unix 毫秒；系统时钟不可用时返回零。
fn current_unix_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

/// Identifies deterministic transaction boundaries available to failure-injection tests.
/// 标识故障注入测试可用的确定性事务边界。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProviderActivationStage {
    /// Applying journal is durable and Active publication is next.
    /// Applying 日志已持久化，下一步发布 Active。
    PublishActive,
    /// Active bytes are published and sync-state persistence is next.
    /// Active 字节已发布，下一步持久化同步状态。
    WriteState,
    /// Active and state are complete and transaction commit is next.
    /// Active 与状态已完整，下一步提交事务。
    CommitTransaction,
}

/// Persists one rejection category without changing the last successful active identity.
/// 持久化一个拒绝类别，但不改变最近成功的 Active 身份。
///
/// # Parameters
/// 参数
/// `store` owns the state file, `provider_id` selects the exact record, and `failure_kind` is
/// the stable rejection identifier.
/// `store` 持有状态文件，`provider_id` 选择精确记录，`failure_kind` 是稳定拒绝标识。
///
/// # Returns
/// 返回值
/// Success after an existing state records the error, or when no successful state exists yet.
/// 既有状态记录错误后返回成功；尚无成功状态时也返回成功。
fn persist_rejection_kind(
    store: &crate::manifest_store::ManifestStoreLayout,
    provider_id: &str,
    failure_kind: &str,
) -> Result<(), String> {
    let crate::manifest_store::SyncStateLoad::Present(mut state) =
        store.load_sync_state(provider_id)?
    else {
        return Ok(());
    };
    state.last_error_kind = Some(failure_kind.to_string());
    store.save_sync_state(provider_id, &state)
}

/// Quarantines one rejected candidate and records its stable outcome.
/// 隔离一个被拒候选并记录其稳定结果。
///
/// # Parameters
/// 参数
/// `store`, `entry`, and `bytes` identify the exact candidate; `failure_kind` classifies it and
/// `failure_path` retains an optional compiler-owned field path.
/// `store`、`entry` 与 `bytes` 标识精确候选，`failure_kind` 对其分类，`failure_path` 保留
/// 可选的编译器权威字段路径。
///
/// # Returns
/// 返回值
/// A rejected sync outcome after quarantine and best-known state stay consistent.
/// 在隔离区与已知状态保持一致后返回拒绝同步结果。
fn reject_candidate(
    store: &crate::manifest_store::ManifestStoreLayout,
    entry: &RemoteProviderEntry,
    bytes: &[u8],
    failure_kind: &str,
    failure_path: Option<&str>,
) -> Result<SyncOutcome, String> {
    store.quarantine(&entry.provider_id, entry.revision, failure_kind, bytes)?;
    persist_rejection_kind(store, &entry.provider_id, failure_kind)?;
    Ok(SyncOutcome::Rejected {
        revision: entry.revision,
        failure_kind: failure_kind.to_string(),
        failure_path: failure_path.map(str::to_string),
    })
}

/// Activates one provider candidate through the store with full verification and rollback.
/// 通过存储激活一个供应商候选，带完整校验与回滚。
///
/// # Parameters
/// 参数
/// `store` owns the file layout, `entry` is the signed index pin, `bytes` are the exact
/// downloaded bytes, and `etag`/`last_modified` are the remote conditional headers persisted
/// with the transaction.
/// `store` 持有文件布局，`entry` 是签名 Index 固定值，`bytes` 是精确下载字节，
/// `etag`/`last_modified` 是随事务持久化的远端条件请求头。
///
/// # Returns
/// 返回值
/// The sync outcome; unverified or out-of-order candidates are quarantined with their
/// failure kind and a failed state write rolls the file activation back.
/// 返回同步结果；未校验或乱序候选按失败类型隔离，状态写入失败会把文件激活回滚。
pub fn activate_synced_provider(
    store: &crate::manifest_store::ManifestStoreLayout,
    entry: &RemoteProviderEntry,
    bytes: &[u8],
    etag: Option<String>,
    index_etag: Option<String>,
    last_modified: Option<String>,
) -> Result<SyncOutcome, String> {
    activate_synced_provider_with_stage_hook(
        store,
        entry,
        bytes,
        ProviderActivationTransactionOptions {
            etag,
            index_etag,
            last_modified,
            commit_transaction: true,
        },
        |_stage| Ok(()),
    )
}

/// Stages one fully verified provider activation while retaining its Applying recovery journal.
/// 暂存一次完整校验的供应商激活，同时保留其 Applying 恢复日志。
///
/// # Parameters
/// 参数
/// Inputs match `activate_synced_provider`; the caller owns the later projection transaction.
/// 输入与 `activate_synced_provider` 相同；调用方持有后续投影事务。
///
/// # Returns
/// 返回值
/// An activation outcome whose snapshot must be explicitly committed or aborted after projection.
/// 返回激活结果；其中的快照必须在投影后显式提交或中止。
pub fn stage_synced_provider_activation(
    store: &crate::manifest_store::ManifestStoreLayout,
    entry: &RemoteProviderEntry,
    bytes: &[u8],
    etag: Option<String>,
    index_etag: Option<String>,
    last_modified: Option<String>,
) -> Result<SyncOutcome, String> {
    activate_synced_provider_with_stage_hook(
        store,
        entry,
        bytes,
        ProviderActivationTransactionOptions {
            etag,
            index_etag,
            last_modified,
            commit_transaction: false,
        },
        |_stage| Ok(()),
    )
}

/// Groups remote validators and transaction ownership for one provider activation.
/// 对一次供应商激活的远端校验器与事务所有权进行分组。
struct ProviderActivationTransactionOptions {
    /// Provider-document entity tag persisted with the activated generation.
    /// 随激活代际持久化的供应商文档实体标签。
    etag: Option<String>,
    /// Signed-index entity tag persisted with the activated generation.
    /// 随激活代际持久化的签名索引实体标签。
    index_etag: Option<String>,
    /// Provider-document last-modified validator persisted with the activated generation.
    /// 随激活代际持久化的供应商文档最后修改校验器。
    last_modified: Option<String>,
    /// Whether this call owns and must commit the store transaction.
    /// 本次调用是否持有并必须提交存储事务。
    commit_transaction: bool,
}

/// Executes one verified activation while exposing deterministic pre-commit stage hooks.
/// 执行一次已验证激活，同时暴露确定性的提交前阶段钩子。
///
/// # Parameters
/// 参数
/// The manifest inputs match `activate_synced_provider`; `options` groups remote validators and
/// transaction ownership, while `stage_hook` may reject one exact boundary before the
/// corresponding authoritative mutation runs.
/// Manifest 输入与 `activate_synced_provider` 相同；`options` 对远端校验器与事务所有权分组，
/// `stage_hook` 可在对应权威变更执行前拒绝一个精确边界。
///
/// # Returns
/// 返回值
/// The normal sync outcome, or a stage-classified error after exact before-image restoration.
/// 返回正常同步结果，或在精确恢复前镜像后的阶段分类错误。
fn activate_synced_provider_with_stage_hook<F>(
    store: &crate::manifest_store::ManifestStoreLayout,
    entry: &RemoteProviderEntry,
    bytes: &[u8],
    options: ProviderActivationTransactionOptions,
    mut stage_hook: F,
) -> Result<SyncOutcome, String>
where
    F: FnMut(ProviderActivationStage) -> Result<(), String>,
{
    let ProviderActivationTransactionOptions {
        etag,
        index_etag,
        last_modified,
        commit_transaction,
    } = options;
    // A transaction interrupted by process termination is resolved before any current-state
    // decision. This prevents a partially rotated Active/Previous pair from becoming the next
    // activation baseline.
    // 进程终止所中断的事务会在任何当前状态决策前完成恢复，避免部分轮换的
    // Active/Previous 对成为下一次激活基线。
    store.recover_system_transaction(&entry.provider_id)?;
    // Verification is mandatory inside the transaction; callers can never activate bytes
    // that were not pinned and compiled. The compiled snapshot is reused for the runtime
    // hot replacement so activation and replacement always observe the same revision.
    // 校验在事务内部强制执行；调用方绝不可能激活未被固定且未编译的字节。
    // 编译快照被复用于运行时热替换，使激活与替换始终观察同一修订。
    let compiled = match verify_provider_bytes(entry, bytes) {
        Ok(compiled) => compiled,
        Err(error) => {
            let failure_kind = error.failure_kind();
            return reject_candidate(store, entry, bytes, failure_kind, error.failure_path());
        }
    };
    let current = store.load_sync_state(&entry.provider_id)?;
    // State metadata is authoritative only when the exact Active bytes revalidate against
    // their digest, revision, semantics, and bundled ceiling. A stale or tampered state may
    // never short-circuit a repair activation (plan §17.5 and repair plan §R7.6).
    // 状态元数据仅在精确 Active 字节通过摘要、修订、语义与随包上限复验时才具权威性。
    // 过期或被篡改的状态绝不能短路修复激活（方案 §17.5、修补方案 §R7.6）。
    let validated_current = match &current {
        crate::manifest_store::SyncStateLoad::Present(state) => {
            store.read_active(&entry.provider_id)?.and_then(|text| {
                let expected = crate::manifest_store::ManifestVersionState {
                    revision: state.revision,
                    sha256: state.sha256.clone(),
                };
                verify_stored_system_manifest(
                    &entry.provider_id,
                    &text,
                    vulcan_provider_adapters::ManifestSourceLayer::Active,
                    &expected,
                )
                .ok()
                .map(|_| expected)
            })
        }
        crate::manifest_store::SyncStateLoad::Absent => None,
    };
    if let Some(state) = &validated_current {
        if state.revision == entry.revision {
            if state.sha256 == entry.sha256 {
                return Ok(SyncOutcome::UpToDate);
            }
            // The same revision pinned to different content is a forgery or a broken
            // publish; it must never replace the active bytes.
            // 同一修订固定到不同内容属于伪造或错误发布；绝不允许替换活动字节。
            return reject_candidate(store, entry, bytes, "same_revision_conflict", None);
        }
        if entry.revision < state.revision {
            return reject_candidate(store, entry, bytes, "revision_downgrade", None);
        }
    }
    // store_snapshot is captured before the first file mutation and remains the only rollback
    // source. Deriving recovery from partially written Active/Previous files would lose the
    // former Previous generation after an interrupted activation.
    // store_snapshot 在首次文件变更前捕获，并作为唯一回滚来源。若从部分写入的
    // Active/Previous 文件推导恢复材料，激活中断后会丢失原 Previous 代际。
    let store_snapshot = store.capture_system_snapshot(&entry.provider_id)?;
    if let Err(_error) = store.stage_candidate(&entry.provider_id, entry.revision, bytes) {
        return reject_candidate(store, entry, bytes, "stage_failed", None);
    }
    store.begin_system_transaction(&entry.provider_id, &store_snapshot)?;
    if let Err(error) = stage_hook(ProviderActivationStage::PublishActive) {
        let restore = store.abort_system_transaction(&entry.provider_id, &store_snapshot);
        return Err(format!(
            "Provider `{}` activation failed before Active publication: {error}; transaction abort: {restore:?}",
            entry.provider_id
        ));
    }
    if let Err(_error) = store.activate(&entry.provider_id, entry.revision) {
        store.quarantine(&entry.provider_id, entry.revision, "activate_failed", bytes)?;
        store.abort_system_transaction(&entry.provider_id, &store_snapshot)?;
        persist_rejection_kind(store, &entry.provider_id, "activate_failed")?;
        return Ok(SyncOutcome::Rejected {
            revision: entry.revision,
            failure_kind: String::from("activate_failed"),
            failure_path: None,
        });
    }
    // The file activation is only durable after the per-provider state write succeeds; a
    // failed state write rolls the activation back so the on-disk pair never diverges.
    // 文件激活只有在逐供应商状态写入成功后才算持久化；状态写入失败会回滚激活，
    // 使磁盘上的两者永不背离。
    if let Err(error) = stage_hook(ProviderActivationStage::WriteState) {
        let restore = store.abort_system_transaction(&entry.provider_id, &store_snapshot);
        return Err(format!(
            "Provider `{}` activation failed before sync-state write: {error}; transaction abort: {restore:?}",
            entry.provider_id
        ));
    }
    if let Err(error) = store.save_sync_state(
        &entry.provider_id,
        &crate::manifest_store::ManifestSyncState {
            revision: entry.revision,
            sha256: entry.sha256.clone(),
            previous: validated_current,
            etag,
            index_etag,
            last_modified,
            synced_at_unix_ms: current_unix_millis(),
            last_error_kind: None,
        },
    ) {
        let rollback_error = store.abort_system_transaction(&entry.provider_id, &store_snapshot);
        let rollback_report = match rollback_error {
            Ok(()) => String::from("ok"),
            Err(message) => format!("failed: {message}"),
        };
        return Err(format!(
            "Failed to persist sync state after activation: {error}; rollback {rollback_report}"
        ));
    }
    if commit_transaction {
        if let Err(error) = stage_hook(ProviderActivationStage::CommitTransaction) {
            let restore = store.abort_system_transaction(&entry.provider_id, &store_snapshot);
            return Err(format!(
                "Provider `{}` activation failed before transaction commit: {error}; transaction abort: {restore:?}",
                entry.provider_id
            ));
        }
        if let Err(error) = store.commit_system_transaction(&entry.provider_id, &store_snapshot) {
            let rollback_error =
                store.abort_system_transaction(&entry.provider_id, &store_snapshot);
            let rollback_report = match rollback_error {
                Ok(()) => String::from("ok"),
                Err(message) => format!("failed: {message}"),
            };
            return Err(format!(
                "Failed to commit system manifest transaction: {error}; rollback {rollback_report}"
            ));
        }
    }
    Ok(SyncOutcome::Activated {
        revision: entry.revision,
        manifest: compiled,
        store_snapshot,
    })
}

/// Builds one unsigned index document for the publishing toolchain.
/// 为发布工具链构建一份未签名 Index 文档。
///
/// # Parameters
/// 参数
/// `entries` are the pinned provider records, `index_revision` is monotonic, and
/// `issued_at_unix_ms`/`expires_at_unix_ms` bound the validity window.
/// `entries` 是固定的供应商记录，`index_revision` 单调递增，
/// `issued_at_unix_ms`/`expires_at_unix_ms` 限定有效窗口。
///
/// # Returns
/// 返回值
/// The serialized index bytes ready for detached signing.
/// 返回可进行分离签名的序列化 Index 字节。
pub fn build_index_document(
    entries: Vec<RemoteProviderEntry>,
    index_revision: u64,
    signing_key_id: &str,
    issued_at_unix_ms: i64,
    expires_at_unix_ms: i64,
) -> Vec<u8> {
    let providers = entries
        .into_iter()
        .map(|entry| (entry.provider_id.clone(), entry))
        .collect::<BTreeMap<_, _>>();
    let index = ManifestRemoteIndex {
        schema_version: String::from(INDEX_SCHEMA_VERSION),
        index_revision,
        issued_at_unix_ms,
        expires_at_unix_ms,
        signing_key_id: signing_key_id.to_string(),
        providers,
    };
    serde_json::to_vec(&index).expect("index must serialize")
}

/// Signs one index document with the publishing key.
/// 用发布密钥签名一份 Index 文档。
///
/// # Parameters
/// 参数
/// `index_bytes` are the exact raw bytes and `secret_key` is the publishing Ed25519 seed.
/// `index_bytes` 是精确原始字节，`secret_key` 是发布 Ed25519 种子。
///
/// # Returns
/// 返回值
/// The 64-byte detached signature.
/// 返回 64 字节分离签名。
pub fn sign_index_document(index_bytes: &[u8], secret_key: &[u8; 32]) -> Result<[u8; 64], String> {
    use ed25519_dalek::Signer;
    let signing_key = ed25519_dalek::SigningKey::from_bytes(secret_key);
    Ok(signing_key.sign(index_bytes).to_bytes())
}

/// Derives the public key bytes for one publishing seed.
/// 为一个发布种子推导公钥字节。
pub fn public_key_for_seed(secret_key: &[u8; 32]) -> [u8; 32] {
    ed25519_dalek::SigningKey::from_bytes(secret_key)
        .verifying_key()
        .to_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one deterministic trusted key pair for tests only.
    /// 仅为测试构建确定性受信密钥对。
    fn test_key(key_id: &str) -> ([u8; 32], TrustedSigningKey) {
        let mut seed = [0_u8; 32];
        seed[..key_id.len().min(32)].copy_from_slice(key_id.as_bytes());
        let public_key = public_key_for_seed(&seed);
        (
            seed,
            TrustedSigningKey {
                key_id: key_id.to_string(),
                public_key,
            },
        )
    }

    /// Loads one bundled provider document and rewrites only its revision for sync tests.
    /// 加载一份随包供应商文档，并仅为同步测试改写其修订号。
    fn bundled_manifest_with_revision(provider_id: &str, revision: u64) -> String {
        let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
        let text = std::fs::read_to_string(directory.join(format!("{provider_id}.json")))
            .expect("bundled provider manifest must exist");
        let mut value: serde_json::Value =
            serde_json::from_str(&text).expect("bundled provider manifest must parse");
        value["revision"] = serde_json::json!(revision);
        serde_json::to_string(&value).expect("rewritten provider manifest must serialize")
    }

    /// Builds one signed-index provider pin for the exact manifest text.
    /// 为精确清单文本构建一条签名 Index 供应商固定记录。
    fn remote_entry_for_text(provider_id: &str, revision: u64, text: &str) -> RemoteProviderEntry {
        use sha2::Digest;
        RemoteProviderEntry {
            provider_id: provider_id.to_string(),
            revision,
            relative_url: String::from("/p.json"),
            sha256: hex::encode(sha2::Sha256::digest(text.as_bytes())),
            bytes: text.len() as u64,
            minimum_engine_contract: 1,
        }
    }

    /// Verifies index signing, parsing, and strict signature enforcement.
    /// 验证 Index 签名、解析与严格签名强制。
    #[test]
    fn signed_index_round_trip_and_signature_enforcement() {
        let (secret, trusted) = test_key("test-key-1");
        let provider_bytes = br#"{"provider_id":"synced-provider"}"#;
        let digest = {
            use sha2::Digest;
            sha2::Sha256::digest(provider_bytes)
        };
        let entry = RemoteProviderEntry {
            provider_id: String::from("synced-provider"),
            revision: 5,
            relative_url: String::from("/providers/synced-provider/5.json"),
            sha256: hex::encode(digest),
            bytes: provider_bytes.len() as u64,
            minimum_engine_contract: 1,
        };
        let now = 1_000_000;
        let index_bytes =
            build_index_document(vec![entry.clone()], 3, "test-key-1", now - 1000, now + 1000);
        let signature = sign_index_document(&index_bytes, &secret).expect("sign should work");
        let parsed = verify_and_parse_index(
            &index_bytes,
            &signature,
            std::slice::from_ref(&trusted),
            now,
        )
        .expect("signed index must verify");
        assert_eq!(parsed.index_revision, 3);
        assert_eq!(
            parsed.providers["synced-provider"].revision, 5,
            "provider pin must parse"
        );

        // A tampered value that keeps the JSON valid must fail strict signature verification.
        // 篡改值但保持 JSON 有效必须通过严格验签失败。
        let mut tampered_value = serde_json::from_slice::<serde_json::Value>(&index_bytes)
            .expect("index should parse for tampering");
        tampered_value["index_revision"] = serde_json::json!(99);
        let tampered = serde_json::to_vec(&tampered_value).expect("tampered index should encode");
        let error =
            verify_and_parse_index(&tampered, &signature, std::slice::from_ref(&trusted), now)
                .expect_err("tampered index must be rejected");
        assert!(error.contains("signature"), "{error}");

        // A document whose declared key id differs from the only matching trusted key must
        // fail after byte-level verification and parsing.
        // 文档声明的密钥 ID 与唯一匹配受信密钥不同时，必须在字节级验签和解析后失败。
        let error = verify_and_parse_index(
            &index_bytes,
            &signature,
            &[TrustedSigningKey {
                key_id: String::from("other-key"),
                public_key: trusted.public_key,
            }],
            now,
        )
        .expect_err("mismatched key id must be rejected");
        assert!(error.contains("declares signing key"), "{error}");

        // An expired window must be rejected.
        // 过期窗口必须被拒绝。
        let error = verify_and_parse_index(&index_bytes, &signature, &[trusted], now + 5000)
            .expect_err("expired index must be rejected");
        assert!(error.contains("time window"), "{error}");
    }

    /// Verifies provider bytes are pinned by count, digest, and compilation.
    /// 验证供应商字节被字节数、摘要与编译固定。
    #[test]
    fn provider_bytes_must_match_index_pin() {
        let provider_bytes = br#"{"provider_id":"synced-provider"}"#;
        let digest = {
            use sha2::Digest;
            sha2::Sha256::digest(provider_bytes)
        };
        let entry = RemoteProviderEntry {
            provider_id: String::from("synced-provider"),
            revision: 5,
            relative_url: String::from("/p.json"),
            sha256: hex::encode(digest),
            bytes: provider_bytes.len() as u64,
            minimum_engine_contract: 1,
        };
        assert!(
            verify_provider_bytes(&entry, provider_bytes).is_err(),
            "placeholder JSON must not compile"
        );

        let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
        let text = std::fs::read_to_string(directory.join("deepseek.json"))
            .expect("bundled deepseek manifest must exist");
        let revision = serde_json::from_str::<serde_json::Value>(&text)
            .expect("bundled deepseek manifest must parse")["revision"]
            .as_u64()
            .expect("bundled deepseek revision must be an unsigned integer");
        let bytes = text.as_bytes();
        let digest = {
            use sha2::Digest;
            sha2::Sha256::digest(bytes)
        };
        let entry = RemoteProviderEntry {
            provider_id: String::from("deepseek"),
            revision,
            relative_url: String::from("/p.json"),
            sha256: hex::encode(digest),
            bytes: bytes.len() as u64,
            minimum_engine_contract: 1,
        };
        verify_provider_bytes(&entry, bytes).expect("bundled deepseek must verify");

        let mut wrong = entry.clone();
        wrong.sha256 = "0".repeat(64);
        let error = verify_provider_bytes(&wrong, bytes).expect_err("digest mismatch must fail");
        assert_eq!(
            error.kind(),
            ProviderManifestVerificationFailureKind::Integrity
        );
        assert!(error.to_string().contains("SHA-256"), "{error}");
    }

    /// Verifies candidate rejection preserves stable engine, schema, and trust categories.
    /// 验证候选拒绝会保留稳定的引擎、Schema 与信任类别。
    #[test]
    fn provider_candidate_rejections_keep_stable_failure_categories() {
        let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
        let text = std::fs::read_to_string(directory.join("deepseek.json"))
            .expect("bundled DeepSeek manifest must exist");
        let revision = serde_json::from_str::<serde_json::Value>(&text)
            .expect("bundled DeepSeek manifest must parse")["revision"]
            .as_u64()
            .expect("DeepSeek revision must be an unsigned integer");

        let mut engine_entry = remote_entry_for_text("deepseek", revision, &text);
        engine_entry.minimum_engine_contract =
            vulcan_provider_adapters::provider_manifest::CURRENT_MANIFEST_ENGINE as u64 + 1;
        let engine_error = verify_provider_bytes(&engine_entry, text.as_bytes())
            .expect_err("a newer engine contract must fail");
        assert_eq!(
            engine_error.kind(),
            ProviderManifestVerificationFailureKind::EngineIncompatible
        );
        assert!(
            engine_error.to_string().contains("Upgrade the client"),
            "an incompatible online generation must require a client upgrade"
        );

        let malformed = "{}";
        let malformed_entry = remote_entry_for_text("deepseek", revision + 1, malformed);
        let schema_error = verify_provider_bytes(&malformed_entry, malformed.as_bytes())
            .expect_err("an incomplete manifest must fail compilation");
        assert_eq!(
            schema_error.kind(),
            ProviderManifestVerificationFailureKind::SchemaSemantic
        );
        assert!(
            schema_error
                .failure_path()
                .is_some_and(|path| path.starts_with('$')),
            "schema rejection must preserve the compiler-owned field path"
        );
        let store_directory = tempfile::tempdir().expect("temporary store should create");
        let store = crate::manifest_store::ManifestStoreLayout::new(store_directory.path());
        let active_entry = remote_entry_for_text("deepseek", revision, &text);
        activate_synced_provider(&store, &active_entry, text.as_bytes(), None, None, None)
            .expect("the bundled candidate should activate first");
        let outcome = activate_synced_provider(
            &store,
            &malformed_entry,
            malformed.as_bytes(),
            None,
            None,
            None,
        )
        .expect("verification rejection must remain a normal sync outcome");
        assert!(matches!(
            outcome,
            SyncOutcome::Rejected {
                failure_kind,
                failure_path: Some(failure_path),
                ..
            } if failure_kind == "schema_semantic_failed" && failure_path.starts_with('$')
        ));
        let crate::manifest_store::SyncStateLoad::Present(state) = store
            .load_sync_state("deepseek")
            .expect("sync state should remain readable")
        else {
            panic!("the successful active state must remain present");
        };
        assert_eq!(
            state.last_error_kind.as_deref(),
            Some("schema_semantic_failed")
        );
        assert_eq!(state.revision, revision);

        let mut candidate = serde_json::from_str::<serde_json::Value>(&text)
            .expect("bundled DeepSeek manifest must parse");
        candidate["revision"] = serde_json::Value::from(revision + 1);
        candidate["services"][0]["base_url"] =
            serde_json::Value::String(String::from("https://unauthorized.example.test"));
        let candidate_text =
            serde_json::to_string(&candidate).expect("candidate manifest must serialize");
        let trust_entry = remote_entry_for_text("deepseek", revision + 1, &candidate_text);
        let trust_error = verify_provider_bytes(&trust_entry, candidate_text.as_bytes())
            .expect_err("an unauthorized host must exceed the trust ceiling");
        assert_eq!(
            trust_error.kind(),
            ProviderManifestVerificationFailureKind::TrustCeiling
        );
    }

    /// Verifies trusted key id uniqueness is enforced before signature work (plan §R7.2).
    /// 验证受信密钥 ID 唯一性在验签前强制执行（方案 §R7.2）。
    #[test]
    fn duplicate_trusted_key_ids_are_rejected() {
        let (secret, trusted) = test_key("test-key-1");
        let entry = RemoteProviderEntry {
            provider_id: String::from("synced-provider"),
            revision: 5,
            relative_url: String::from("/p.json"),
            sha256: "a".repeat(SHA256_HEX_LENGTH),
            bytes: 1,
            minimum_engine_contract: 1,
        };
        let now = 1_000_000;
        let index_bytes =
            build_index_document(vec![entry], 3, "test-key-1", now - 1000, now + 1000);
        let signature = sign_index_document(&index_bytes, &secret).expect("sign should work");
        let duplicate = TrustedSigningKey {
            key_id: String::from("test-key-1"),
            public_key: trusted.public_key,
        };
        let error = verify_and_parse_index(&index_bytes, &signature, &[trusted, duplicate], now)
            .expect_err("duplicate key ids must be rejected");
        assert!(error.contains("duplicated"), "{error}");
    }

    /// Verifies duplicate public keys under distinct identifiers cannot satisfy unique matching.
    /// 验证不同标识下的重复公钥不能满足唯一匹配要求。
    #[test]
    fn duplicate_trusted_public_keys_are_rejected() {
        let (secret, trusted) = test_key("test-key-1");
        let entry = RemoteProviderEntry {
            provider_id: String::from("synced-provider"),
            revision: 5,
            relative_url: String::from("/p.json"),
            sha256: "a".repeat(SHA256_HEX_LENGTH),
            bytes: 1,
            minimum_engine_contract: 1,
        };
        let now = 1_000_000;
        let index_bytes =
            build_index_document(vec![entry], 3, "test-key-1", now - 1000, now + 1000);
        let signature = sign_index_document(&index_bytes, &secret).expect("sign should work");
        let alias = TrustedSigningKey {
            key_id: String::from("test-key-alias"),
            public_key: trusted.public_key,
        };
        let error = verify_and_parse_index(&index_bytes, &signature, &[trusted, alias], now)
            .expect_err("duplicate public keys must be rejected");
        assert!(error.contains("more than one"), "{error}");
    }

    /// Verifies remote manifests cannot expand bundled hosts, credentials, primitives, or actions.
    /// 验证远端清单不能扩大随包主机、凭据、原语或动作权限。
    #[test]
    fn bundled_trust_ceiling_rejects_authority_expansion() {
        let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
        let deepseek_text = std::fs::read_to_string(directory.join("deepseek.json"))
            .expect("bundled deepseek manifest must exist");
        let bundled: vulcan_provider_adapters::ProviderManifestV1 =
            serde_json::from_str(&deepseek_text).expect("bundled deepseek manifest must parse");

        // changed_registration simulates a signed manifest selecting a different onboarding primitive.
        // changed_registration 模拟签名清单选择不同的接入原语。
        let mut changed_registration = bundled.clone();
        changed_registration.credential_registration =
            vulcan_provider_adapters::CredentialRegistrationKind::MiniMaxRegionalApiKey;
        let error = validate_bundled_trust_ceiling(&bundled, &changed_registration)
            .expect_err("an unreviewed credential registration primitive must be rejected");
        assert!(
            error.contains("credential registration primitive"),
            "{error}"
        );

        // changed_promotion simulates a signed manifest redirecting a public registration link.
        // changed_promotion 模拟签名清单重定向公开注册链接。
        let mut changed_promotion = bundled.clone();
        changed_promotion.display.promotion = Some(vulcan_provider_adapters::ManifestPromotion {
            signup_url: String::from("https://unreviewed.example.invalid/register"),
            referral_url: None,
            campaign_id: None,
            partnership_kind: vulcan_provider_adapters::ProviderPartnershipKind::None,
            disclosure_key:
                vulcan_provider_adapters::ProviderPromotionDisclosureKey::NeutralRegistration,
        });
        let error = validate_bundled_trust_ceiling(&bundled, &changed_promotion)
            .expect_err("an unreviewed public promotion URL must be rejected");
        assert!(error.contains("promotion authority"), "{error}");

        // narrowed_unknown_policy proves a signed update may remove unmatched-model fallback authority.
        // narrowed_unknown_policy 证明签名更新可以移除未命中模型的兜底绑定权限。
        let mut narrowed_unknown_policy = bundled.clone();
        narrowed_unknown_policy.model_defaults.unknown_model_policy =
            vulcan_provider_adapters::UnknownModelPolicy::DeclaredRuleRequired;
        validate_bundled_trust_ceiling(&bundled, &narrowed_unknown_policy)
            .expect("unknown-model authority narrowing must remain allowed");

        // expanded_unknown_policy proves a later signed update cannot restore broader fallback authority.
        // expanded_unknown_policy 证明后续签名更新不能恢复更宽的兜底绑定权限。
        let mut expanded_unknown_policy = narrowed_unknown_policy.clone();
        expanded_unknown_policy.model_defaults.unknown_model_policy =
            vulcan_provider_adapters::UnknownModelPolicy::DefaultBindingAllowed;
        let error =
            validate_bundled_trust_ceiling(&narrowed_unknown_policy, &expanded_unknown_policy)
                .expect_err("unknown-model fallback authority expansion must be rejected");
        assert!(error.contains("unknown-model binding authority"), "{error}");

        // declared_membership_baseline models a reviewed ceiling that admits only signed identities.
        // declared_membership_baseline 模拟仅接纳签名身份的已审核上限。
        let mut declared_membership_baseline = bundled.clone();
        declared_membership_baseline
            .model_defaults
            .catalog_membership_policy =
            vulcan_provider_adapters::CatalogMembershipPolicy::DeclaredOnly;
        // expanded_membership admits discovery evidence that the reviewed baseline did not trust.
        // expanded_membership 接纳了审核基线并不信任的发现证据。
        let mut expanded_membership = declared_membership_baseline.clone();
        expanded_membership.model_defaults.catalog_membership_policy =
            vulcan_provider_adapters::CatalogMembershipPolicy::DeclaredAndDiscovery;
        let error =
            validate_bundled_trust_ceiling(&declared_membership_baseline, &expanded_membership)
                .expect_err("catalog admission-source expansion must be rejected");
        assert!(error.contains("catalog-membership authority"), "{error}");

        // broader_membership_baseline proves either reviewed source can be removed independently.
        // broader_membership_baseline 证明两种已审核来源都可以被独立移除。
        validate_bundled_trust_ceiling(&expanded_membership, &declared_membership_baseline)
            .expect("catalog admission-source narrowing must remain allowed");

        let mut changed_host = bundled.clone();
        changed_host.services[0].base_url = String::from("https://unreviewed.example.invalid");
        let error = validate_bundled_trust_ceiling(&bundled, &changed_host)
            .expect_err("an unreviewed host must be rejected");
        assert!(error.contains("host"), "{error}");

        let mut reviewed_session_header = bundled.clone();
        reviewed_session_header.services[0].session_header =
            Some(String::from("x-reviewed-session"));
        let mut changed_session_header = reviewed_session_header.clone();
        changed_session_header.services[0].session_header =
            Some(String::from("x-unreviewed-session"));
        let error =
            validate_bundled_trust_ceiling(&reviewed_session_header, &changed_session_header)
                .expect_err("remote session-header retargeting must be rejected");
        assert!(error.contains("session-header"), "{error}");
        let mut removed_session_header = reviewed_session_header.clone();
        removed_session_header.services[0].session_header = None;
        validate_bundled_trust_ceiling(&reviewed_session_header, &removed_session_header)
            .expect("remote session-header removal must remain allowed");
        validate_bundled_trust_ceiling(&reviewed_session_header, &reviewed_session_header)
            .expect("unchanged session-header authority must remain allowed");

        let mut changed_credential = bundled.clone();
        changed_credential.services[0].credential_profiles[0].credential_role =
            Some(String::from("unreviewed-role"));
        let error = validate_bundled_trust_ceiling(&bundled, &changed_credential)
            .expect_err("an unreviewed credential role must be rejected");
        assert!(error.contains("credential authority"), "{error}");

        // changed_usage_primitive simulates a signed manifest selecting an unreviewed account endpoint.
        // changed_usage_primitive 模拟签名清单选择未经审核的账号端点。
        let mut changed_usage_primitive = bundled.clone();
        changed_usage_primitive.services[0].credential_profiles[0].account_usage_query =
            vulcan_provider_adapters::AccountUsageQueryKind::OpenRouterCredits;
        let error = validate_bundled_trust_ceiling(&bundled, &changed_usage_primitive)
            .expect_err("an unreviewed account usage primitive must be rejected");
        assert!(error.contains("credential authority"), "{error}");

        let mut changed_primitive = bundled.clone();
        changed_primitive
            .engine_contract
            .required_execution_primitives
            .push(String::from("unreviewed.primitive"));
        let error = validate_bundled_trust_ceiling(&bundled, &changed_primitive)
            .expect_err("an unreviewed execution primitive must be rejected");
        assert!(error.contains("execution primitive"), "{error}");

        let mut changed_protocol = bundled.clone();
        let mut route = changed_protocol.services[0].protocol_routes[0].clone();
        route.protocol_profile_id = String::from("unreviewed.protocol");
        changed_protocol.services[0].protocol_routes.push(route);
        let error = validate_bundled_trust_ceiling(&bundled, &changed_protocol)
            .expect_err("an unreviewed protocol route must be rejected");
        assert!(error.contains("protocol route"), "{error}");

        let mut changed_upload = bundled.clone();
        changed_upload.upload_channels.push(
            serde_json::from_value(serde_json::json!({
                "upload_channel_id": "unreviewed-upload"
            }))
            .expect("minimal upload declaration must parse"),
        );
        let error = validate_bundled_trust_ceiling(&bundled, &changed_upload)
            .expect_err("an unreviewed upload channel must be rejected");
        assert!(error.contains("upload channel"), "{error}");

        let mut changed_operation = bundled.clone();
        changed_operation.operations.push(
            serde_json::from_value(serde_json::json!({
                "operation_id": "unreviewed-operation",
                "operation": "image_generation",
                "carrier": "unsupported"
            }))
            .expect("minimal operation declaration must parse"),
        );
        let error = validate_bundled_trust_ceiling(&bundled, &changed_operation)
            .expect_err("an unreviewed operation must be rejected");
        assert!(error.contains("operation"), "{error}");

        let alibaba_text = std::fs::read_to_string(directory.join("alibaba.json"))
            .expect("bundled alibaba manifest must exist");
        let alibaba: vulcan_provider_adapters::ProviderManifestV1 =
            serde_json::from_str(&alibaba_text).expect("bundled alibaba manifest must parse");

        // changed_upload_response_host expands the dynamic destination returned by the OSS signer.
        // changed_upload_response_host 扩大 OSS 签名端返回的动态目标范围。
        let mut changed_upload_response_host = alibaba.clone();
        changed_upload_response_host.upload_channels[0]
            .response_host_suffixes
            .push(String::from(".unreviewed.example"));
        // error proves a signed data update cannot create new network authority indirectly.
        // error 证明签名数据更新不能间接创建新的网络权限。
        let error = validate_bundled_trust_ceiling(&alibaba, &changed_upload_response_host)
            .expect_err("a dynamic upload host suffix must remain inside the bundled allowlist");
        assert!(error.contains("Upload channel"), "{error}");

        // changed_template simulates a signed candidate redirecting immutable execution authority.
        // changed_template 模拟签名候选重定向不可变执行权限。
        let mut changed_template = alibaba.clone();
        changed_template.tool_target_templates[0]
            .execution
            .endpoint_path = String::from("/unreviewed-template-endpoint");
        let error = validate_bundled_trust_ceiling(&alibaba, &changed_template)
            .expect_err("a reviewed target template must remain byte-for-byte immutable");
        assert!(
            error.contains("changes bundled execution authority"),
            "{error}"
        );

        // changed_target_identity bypasses templates by redirecting a reviewed fixed target.
        // changed_target_identity 通过重定向已审核固定目标来绕过模板。
        let mut changed_target_identity = alibaba.clone();
        changed_target_identity.tool_targets[0].upstream_target_id =
            String::from("unreviewed-upstream-model");
        let error = validate_bundled_trust_ceiling(&alibaba, &changed_target_identity)
            .expect_err("a reviewed fixed target must not redirect its upstream identity");
        assert!(
            error.contains("identity, contract, or execution"),
            "{error}"
        );

        // changed_target_contract removes the reviewed input constraints from a fixed target.
        // changed_target_contract 从固定目标移除已审核输入约束。
        let mut changed_target_contract = alibaba.clone();
        changed_target_contract.tool_targets[0].input_schema =
            serde_json::json!({"type": "object"});
        let error = validate_bundled_trust_ceiling(&alibaba, &changed_target_contract)
            .expect_err("a reviewed fixed target contract must remain immutable");
        assert!(
            error.contains("identity, contract, or execution"),
            "{error}"
        );

        // expanded_target_credentials clears a bounded profile set, whose schema meaning is all profiles.
        // expanded_target_credentials 清空有界配置档集合；该 Schema 语义是允许全部配置档。
        let mut expanded_target_credentials = alibaba.clone();
        expanded_target_credentials.tool_targets[0]
            .credential_profile_ids
            .clear();
        let error = validate_bundled_trust_ceiling(&alibaba, &expanded_target_credentials)
            .expect_err("an empty candidate credential scope must not expand to every profile");
        assert!(
            error.contains("identity, contract, or execution"),
            "{error}"
        );

        // added_template simulates a signed candidate adding a new executable template.
        // added_template 模拟签名候选新增一条可执行模板。
        let mut added_template = alibaba.clone();
        // unreviewed_template copies authority only to prove identity changes do not authorize it.
        // unreviewed_template 复制权限仅用于证明改变身份不会获得授权。
        let mut unreviewed_template = added_template.tool_target_templates[0].clone();
        unreviewed_template.template_id = String::from("unreviewed.template.v1");
        added_template
            .tool_target_templates
            .push(unreviewed_template);
        let error = validate_bundled_trust_ceiling(&alibaba, &added_template)
            .expect_err("an unreviewed target template must be rejected");
        assert!(
            error.contains("adds unreviewed tool-target template"),
            "{error}"
        );

        // changed_operation_semantics reuses a reviewed carrier under a different canonical action.
        // changed_operation_semantics 在不同规范动作下复用已审核载体。
        let mut changed_operation_semantics = alibaba.clone();
        changed_operation_semantics.operations[0].operation =
            String::from("unreviewed_canonical_action");
        let error = validate_bundled_trust_ceiling(&alibaba, &changed_operation_semantics)
            .expect_err("an operation must not change its canonical action");
        assert!(error.contains("Operation"), "{error}");

        // changed_operation_profiles attaches an unreviewed wire projection to reviewed execution.
        // changed_operation_profiles 为已审核执行附加未审核 Wire 投影。
        let mut changed_operation_profiles = alibaba.clone();
        changed_operation_profiles.operations[0]
            .request_profile_refs
            .push(String::from("unreviewed-request-profile"));
        let error = validate_bundled_trust_ceiling(&alibaba, &changed_operation_profiles)
            .expect_err("an operation must not acquire an unreviewed request projection");
        assert!(error.contains("Operation"), "{error}");

        // changed_operation_resources adds a new local resource class to reviewed execution.
        // changed_operation_resources 为已审核执行增加新的本地资源类别。
        let mut changed_operation_resources = alibaba.clone();
        changed_operation_resources.operations[0]
            .input_resources
            .push(
                serde_json::from_value(serde_json::json!("file"))
                    .expect("the canonical file resource kind must parse"),
            );
        let error = validate_bundled_trust_ceiling(&alibaba, &changed_operation_resources)
            .expect_err("an operation must not acquire an unreviewed resource class");
        assert!(error.contains("Operation"), "{error}");

        // reduced_backoff_ceiling would let repeated failures poll more often than bundled review.
        // reduced_backoff_ceiling 会让重复失败后的轮询频率高于随包审核结果。
        let mut reduced_backoff_ceiling = alibaba.clone();
        let async_execution = reduced_backoff_ceiling
            .operations
            .iter_mut()
            .find_map(|operation| operation.async_execution.as_mut())
            .expect("Alibaba must expose one asynchronous operation");
        async_execution.max_backoff_ms = async_execution
            .max_backoff_ms
            .saturating_sub(1)
            .max(async_execution.poll_interval_ms);
        let error = validate_bundled_trust_ceiling(&alibaba, &reduced_backoff_ceiling)
            .expect_err("a remote lifecycle must not reduce its reviewed backoff ceiling");
        assert!(error.contains("Operation"), "{error}");

        // changed_upload_binding makes an existing operation read a different canonical argument path.
        // changed_upload_binding 让现有操作读取不同的规范参数路径。
        let mut changed_upload_binding = alibaba.clone();
        // operation is the reviewed upload-bearing template selected for mutation.
        // operation 是选中用于变更的已审核含上传模板。
        let operation = changed_upload_binding
            .operations
            .iter_mut()
            .find(|operation| !operation.upload_bindings.is_empty())
            .expect("Alibaba must expose an operation with an upload binding");
        // binding is a copied reviewed binding whose source path is deliberately expanded.
        // binding 是复制的已审核绑定，其源路径会被刻意扩大。
        let mut binding = operation.upload_bindings[0].clone();
        binding.source_path = String::from("/unreviewed_local_resource");
        operation.upload_bindings.push(binding);
        let error = validate_bundled_trust_ceiling(&alibaba, &changed_upload_binding)
            .expect_err("an operation must not read an unreviewed resource path");
        assert!(error.contains("Operation"), "{error}");

        // changed_cache_lifetime enables or extends provider-resource retention beyond bundled review.
        // changed_cache_lifetime 启用或延长超过随包审核范围的供应商资源保留时间。
        let mut changed_cache_lifetime = alibaba.clone();
        // bundled_cache_lifetime captures whether resource caching was reviewed at all.
        // bundled_cache_lifetime 记录资源缓存是否曾被审核。
        let bundled_cache_lifetime = changed_cache_lifetime.upload_channels[0].cache_duration_ms;
        changed_cache_lifetime.upload_channels[0].cache_duration_ms = Some(
            bundled_cache_lifetime
                .map(|value| value.saturating_add(1))
                .unwrap_or(1),
        );
        let error = validate_bundled_trust_ceiling(&alibaba, &changed_cache_lifetime)
            .expect_err("an upload cache lifetime must not exceed bundled review");
        assert!(error.contains("Upload channel"), "{error}");

        // expanded_upload_timeout prolongs network side effects beyond the reviewed deadline.
        // expanded_upload_timeout 将网络副作用延长到已审核期限之外。
        let mut expanded_upload_timeout = alibaba.clone();
        // timed_upload_channel selects a production network channel with an explicit deadline.
        // timed_upload_channel 选择一条带显式期限的生产网络通道。
        let timed_upload_channel = expanded_upload_timeout
            .upload_channels
            .iter_mut()
            .find(|channel| channel.timeout_ms.is_some())
            .expect("the trust fixture must expose a timed network upload channel");
        timed_upload_channel.timeout_ms = timed_upload_channel
            .timeout_ms
            .map(|timeout_ms| timeout_ms.saturating_add(1));
        let error = validate_bundled_trust_ceiling(&alibaba, &expanded_upload_timeout)
            .expect_err("an upload timeout must not exceed bundled review");
        assert!(error.contains("Upload channel"), "{error}");

        // expanded_upload_mime clears a bounded MIME allowlist, whose runtime meaning is any MIME.
        // expanded_upload_mime 清空有界 MIME 允许列表；其运行时语义是允许任意 MIME。
        let mut expanded_upload_mime = alibaba.clone();
        // expanded_upload_channel selects an actually bounded production upload declaration.
        // expanded_upload_channel 选择一条真实有界的生产上传声明。
        let expanded_upload_channel = expanded_upload_mime
            .upload_channels
            .iter_mut()
            .find(|channel| !channel.accepted_mime_types.is_empty())
            .expect("the trust fixture must expose a bounded MIME allowlist");
        expanded_upload_channel.accepted_mime_types.clear();
        let error = validate_bundled_trust_ceiling(&alibaba, &expanded_upload_mime)
            .expect_err("an empty candidate MIME list must not expand to every MIME");
        assert!(error.contains("Upload channel"), "{error}");

        let mut changed_target = bundled.clone();
        changed_target.tool_targets.push(
            alibaba
                .tool_targets
                .first()
                .cloned()
                .expect("alibaba must expose one manifest tool target"),
        );
        let error = validate_bundled_trust_ceiling(&bundled, &changed_target)
            .expect_err("an unreviewed tool target must be rejected");
        assert!(error.contains("tool target"), "{error}");
    }

    /// Verifies signed updates cannot promote an unverified route or inspect new diagnostics.
    /// 验证签名更新不能提升未验证路由或读取新的诊断响应头。
    #[test]
    fn bundled_trust_ceiling_freezes_route_verification_and_diagnostic_headers() {
        // directory resolves the immutable bundled manifest source.
        // directory 解析不可变的随包清单来源。
        let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
        // text contains one production route used to build a synthetic reviewed baseline.
        // text 包含一条用于构造合成审核基线的生产路由。
        let text = std::fs::read_to_string(directory.join("deepseek.json"))
            .expect("bundled DeepSeek manifest must exist");
        // baseline models an installed route whose verification and diagnostic authority are bounded.
        // baseline 模拟验证状态与诊断权限均有界的已安装路由。
        let mut baseline: vulcan_provider_adapters::ProviderManifestV1 =
            serde_json::from_str(&text).expect("bundled DeepSeek manifest must parse");
        baseline.services[0].protocol_routes[0].verified = false;
        baseline.services[0].protocol_routes[0].provider_request_id_headers =
            vec![String::from("x-request-id")];

        // promoted enables an execution route that the reviewed baseline kept disabled.
        // promoted 启用审核基线保持禁用的执行路由。
        let mut promoted = baseline.clone();
        promoted.services[0].protocol_routes[0].verified = true;
        let error = validate_bundled_trust_ceiling(&baseline, &promoted)
            .expect_err("a signed update must not promote an unverified route");
        assert!(error.contains("verification"), "{error}");

        // expanded_diagnostics reads an additional response header outside reviewed authority.
        // expanded_diagnostics 读取审核权限之外的额外响应头。
        let mut expanded_diagnostics = baseline.clone();
        expanded_diagnostics.services[0].protocol_routes[0]
            .provider_request_id_headers
            .push(String::from("x-unreviewed-request-id"));
        let error = validate_bundled_trust_ceiling(&baseline, &expanded_diagnostics)
            .expect_err("a signed update must not inspect a new diagnostic header");
        assert!(error.contains("diagnostic"), "{error}");

        // narrowed_diagnostics removes extraction authority and therefore remains valid.
        // narrowed_diagnostics 删除提取权限，因此仍然有效。
        let mut narrowed_diagnostics = baseline.clone();
        narrowed_diagnostics.services[0].protocol_routes[0]
            .provider_request_id_headers
            .clear();
        validate_bundled_trust_ceiling(&baseline, &narrowed_diagnostics)
            .expect("removing diagnostic extraction authority must remain allowed");
    }

    /// Verifies remote URL upload hosts remain inside the reviewed semantic host scope.
    /// 验证远程 URL 上传宿主始终位于已审核的语义宿主范围内。
    #[test]
    fn upload_channel_trust_ceiling_freezes_remote_url_hosts() {
        // baseline authorizes only strict subdomains of example.com.
        // baseline 仅授权 example.com 的严格子域。
        let baseline: vulcan_provider_adapters::UploadChannelDeclaration =
            serde_json::from_value(serde_json::json!({
                "upload_channel_id": "remote-url",
                "primitive": "remote_url_passthrough",
                "remote_url_host_suffixes": [".example.com"]
            }))
            .expect("the synthetic remote URL channel must parse");

        // expanded adds a wholly unrelated host outside the reviewed allowlist.
        // expanded 增加完全不属于审核允许列表的宿主。
        let mut expanded = baseline.clone();
        expanded
            .remote_url_host_suffixes
            .push(String::from("unreviewed.example"));
        validate_upload_channel_trust_ceiling(&baseline, &expanded)
            .expect_err("an unrelated remote URL host must exceed the trust ceiling");

        // narrowed admits only a deeper strict-subdomain suffix.
        // narrowed 仅允许更深层的严格子域后缀。
        let mut narrowed = baseline.clone();
        narrowed.remote_url_host_suffixes = vec![String::from(".media.example.com")];
        validate_upload_channel_trust_ceiling(&baseline, &narrowed)
            .expect("a deeper host suffix must be accepted as authority narrowing");

        // root_host proves a suffix grant never implicitly authorizes its DNS root.
        // root_host 证明后缀授权绝不会隐式授权其 DNS 根域。
        let mut root_host = baseline.clone();
        root_host.remote_url_host_suffixes = vec![String::from("example.com")];
        validate_upload_channel_trust_ceiling(&baseline, &root_host)
            .expect_err("a strict-subdomain grant must not authorize the root host");
    }

    /// Verifies signed updates cannot change the reviewed upload response identifier scalar type.
    /// 验证签名更新不能改变已审核的上传响应标识标量类型。
    #[test]
    fn upload_channel_trust_ceiling_freezes_response_id_encoding() {
        let baseline: vulcan_provider_adapters::UploadChannelDeclaration =
            serde_json::from_value(serde_json::json!({
                "upload_channel_id": "numeric-file-id",
                "primitive": "multipart_file",
                "response_id_pointer": "/file/file_id",
                "response_id_encoding": "unsigned_integer"
            }))
            .expect("the synthetic upload channel must parse");
        let mut changed = baseline.clone();
        changed.response_id_encoding =
            Some(vulcan_provider_adapters::UploadResponseIdEncoding::String);
        validate_upload_channel_trust_ceiling(&baseline, &changed)
            .expect_err("a response identifier scalar change must exceed the trust ceiling");
    }

    /// Verifies a signed remote revision may add model rules inside the bundled authority.
    /// 验证签名远端修订可在随包权限范围内新增模型规则。
    #[test]
    fn bundled_trust_ceiling_allows_data_only_model_rules() {
        let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
        let text = std::fs::read_to_string(directory.join("deepseek.json"))
            .expect("bundled deepseek manifest must exist");
        let bundled: vulcan_provider_adapters::ProviderManifestV1 =
            serde_json::from_str(&text).expect("bundled deepseek manifest must parse");
        let mut candidate = bundled.clone();
        let mut rule = candidate
            .model_rules
            .first()
            .cloned()
            .expect("deepseek must expose one model rule");
        rule.rule_id = String::from("remote-new-model");
        rule.matcher.value = String::from("deepseek-new-model");
        candidate.model_rules.push(rule);
        validate_bundled_trust_ceiling(&bundled, &candidate)
            .expect("data-only model rules must stay inside the trust ceiling");
    }

    /// Verifies signed route resource constraints may narrow but never remove bundled bounds.
    /// 验证签名路由资源约束可以收紧但绝不能移除随包边界。
    #[test]
    fn bundled_trust_ceiling_freezes_resource_constraint_authority() {
        // directory is the authoritative bundled manifest source.
        // directory 是权威的随包清单来源。
        let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
        // text contains the reviewed OpenRouter resource boundary.
        // text 包含已审核的 OpenRouter 资源边界。
        let text = std::fs::read_to_string(directory.join("openrouter.json"))
            .expect("bundled OpenRouter manifest must exist");
        // bundled is the immutable trust baseline.
        // bundled 是不可变信任基线。
        let bundled: vulcan_provider_adapters::ProviderManifestV1 =
            serde_json::from_str(&text).expect("bundled OpenRouter manifest must parse");
        // removed drops both reviewed Chat resource constraints and therefore expands authority.
        // removed 删除两条已审核 Chat 资源约束，因此会扩大权限。
        let mut removed = bundled.clone();
        removed.services[0]
            .protocol_routes
            .iter_mut()
            .find(|route| route.protocol_profile_id == "openai-chat-completions.v1")
            .expect("OpenRouter Chat route must exist")
            .input_resource_constraints
            .clear();
        assert!(
            validate_bundled_trust_ceiling(&bundled, &removed).is_err(),
            "removing a bundled resource constraint must fail"
        );

        // narrowed preserves the MIME boundary and adds a stricter byte limit.
        // narrowed 保留 MIME 边界并增加更严格的字节限制。
        let mut narrowed = bundled.clone();
        narrowed.services[0]
            .protocol_routes
            .iter_mut()
            .find(|route| route.protocol_profile_id == "openai-chat-completions.v1")
            .expect("OpenRouter Chat route must exist")
            .input_resource_constraints
            .iter_mut()
            .find(|constraint| constraint.accepted_mime_types == [String::from("application/pdf")])
            .expect("OpenRouter file constraint must exist")
            .max_bytes = Some(5_000_000);
        validate_bundled_trust_ceiling(&bundled, &narrowed)
            .expect("a stricter resource byte limit must stay inside the trust ceiling");
    }

    /// Verifies both model catalogs may grow independently while discovery authority stays frozen.
    /// 验证两类模型目录均可独立增长，而发现权限保持冻结。
    #[test]
    fn signed_new_multimedia_model_can_bind_only_to_pre_authorized_template() {
        // directory resolves the authoritative bundled manifest directory for this trust-ceiling test.
        // directory 解析本信任上限测试使用的权威内置清单目录。
        let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
        // text contains the exact reviewed Alibaba manifest bytes used as the trust ceiling.
        // text 保存作为信任上限的已审核 Alibaba 清单原文。
        let text = std::fs::read_to_string(directory.join("alibaba.json"))
            .expect("bundled Alibaba manifest must exist");
        // bundled is the parsed immutable authority against which the candidate is checked.
        // bundled 是用于校验候选版本的已解析不可变权威基线。
        let bundled: vulcan_provider_adapters::ProviderManifestV1 =
            serde_json::from_str(&text).expect("bundled Alibaba manifest must parse");
        // candidate models a signed hot-update candidate derived from the reviewed baseline.
        // candidate 模拟从已审核基线派生出的签名热更新候选版本。
        let mut candidate = bundled.clone();
        // reviewed_template_id identifies the exact pre-reviewed execution authority reused by the new model.
        // reviewed_template_id 标识新模型复用的精确已审核执行权限。
        let reviewed_template_id = "alibaba.model-studio-cn.qwen-image-generate.v1";
        // reviewed_binding anchors the model addition to the service that actually owns the reviewed template binding.
        // reviewed_binding 将模型新增绑定到实际拥有已审核模板绑定的服务。
        let reviewed_binding = candidate
            .tool_target_template_bindings
            .iter()
            .find(|binding| {
                binding
                    .template_refs
                    .iter()
                    .any(|reference| reference == reviewed_template_id)
            })
            .cloned()
            .expect("Alibaba must expose one reviewed image template binding");
        // reviewed_service_entry_id prevents service-array ordering from becoming accidental authority.
        // reviewed_service_entry_id 防止服务数组顺序意外成为权威依据。
        let reviewed_service_entry_id = reviewed_binding.service_entry_id.clone();
        // reviewed_service resolves the unique catalog that owns the reviewed binding.
        // reviewed_service 解析拥有该已审核绑定的唯一模型目录。
        let reviewed_service = candidate
            .services
            .iter_mut()
            .find(|service| service.service_entry_id == reviewed_service_entry_id)
            .expect("Alibaba reviewed image binding service must exist");
        reviewed_service
            .model_catalog
            .as_mut()
            .expect("Alibaba model catalog declaration must be present")
            .models
            .as_mut()
            .expect("Alibaba static model inventory must be present")
            .push(
                vulcan_provider_adapters::provider_manifest::ManifestCatalogModel {
                    model_id: String::from("qwen-trust-ceiling-hot-add"),
                    display_name: String::from("Qwen Trust Ceiling Hot Add"),
                },
            );
        // reviewed_conversation_rule supplies the exact static-catalog capability and protocol
        // authority required by the newly declared conversation identity.
        // reviewed_conversation_rule 提供新声明会话身份所需的精确静态目录能力与协议权限。
        let mut reviewed_conversation_rule = candidate
            .model_rules
            .iter()
            .find(|rule| {
                rule.matcher.value == "qwen3.7-plus"
                    && rule
                        .scope
                        .covers_service_entry(reviewed_service_entry_id.as_str())
                    && rule.scope.covers_credential_profile("api-key")
            })
            .cloned()
            .expect("Alibaba must expose one reviewed Model Studio conversation rule");
        reviewed_conversation_rule.rule_id = String::from("remote-qwen-trust-ceiling-hot-add");
        reviewed_conversation_rule.matcher.value = String::from("qwen-trust-ceiling-hot-add");
        candidate.model_rules.push(reviewed_conversation_rule);
        // new_binding independently extends the media/tool catalog through reviewed execution authority.
        // new_binding 通过已审核执行权限独立扩展媒体/工具目录。
        let mut new_binding = reviewed_binding.clone();
        new_binding.model_id = String::from("qwen-tool-template-hot-add");
        new_binding.display_name = String::from("Qwen Tool Template Hot Add");
        candidate.tool_target_template_bindings.push(new_binding);
        validate_bundled_trust_ceiling(&bundled, &candidate)
            .expect("both data-only model catalogs must remain inside the trust ceiling");
        let candidate_text =
            serde_json::to_string(&candidate).expect("data-only candidate must serialize");
        let compiled = vulcan_provider_adapters::compile_manifest_text(
            &candidate_text,
            vulcan_provider_adapters::ManifestOwnership::System,
        )
        .expect("independent media model binding must compile through reviewed template authority");
        assert!(
            compiled
                .tool_target_template_instances
                .values()
                .any(|instance| instance.model_id == "qwen-tool-template-hot-add"),
            "the independently declared media model must instantiate one reviewed target"
        );

        // switched_binding proves an existing model cannot acquire another reviewed template by
        // rewriting its data binding.
        // switched_binding 证明现有模型不能通过重写数据绑定来取得另一条已审核模板。
        let mut switched_binding_candidate = bundled.clone();
        switched_binding_candidate.tool_target_template_bindings[0].template_refs =
            vec![String::from("alibaba.model-studio-cn.wan-t2v.v1")];
        let error = validate_bundled_trust_ceiling(&bundled, &switched_binding_candidate)
            .expect_err("an existing binding must not attach a different template");
        assert!(error.contains("unreviewed target template"), "{error}");

        // candidate_service re-resolves the same service before mutating discovery authority.
        // candidate_service 在修改发现权限前重新解析同一服务。
        let candidate_service = candidate
            .services
            .iter_mut()
            .find(|service| service.service_entry_id == reviewed_service_entry_id)
            .expect("Alibaba reviewed image binding service must remain present");
        candidate_service
            .model_catalog
            .as_mut()
            .expect("Alibaba model catalog declaration must be present")
            .path = Some(String::from("/unreviewed-models"));
        let error = validate_bundled_trust_ceiling(&bundled, &candidate)
            .expect_err("a discovery path change must remain outside the trust ceiling");
        assert!(error.contains("model-discovery authority"), "{error}");
    }

    /// Verifies oversized trusted key sets are rejected (plan §R7.2).
    /// 验证超限受信密钥集被拒绝（方案 §R7.2）。
    #[test]
    fn oversized_trusted_key_set_is_rejected() {
        let (secret, trusted) = test_key("test-key-1");
        let entry = RemoteProviderEntry {
            provider_id: String::from("synced-provider"),
            revision: 5,
            relative_url: String::from("/p.json"),
            sha256: "a".repeat(SHA256_HEX_LENGTH),
            bytes: 1,
            minimum_engine_contract: 1,
        };
        let now = 1_000_000;
        let index_bytes =
            build_index_document(vec![entry], 3, "test-key-1", now - 1000, now + 1000);
        let signature = sign_index_document(&index_bytes, &secret).expect("sign should work");
        let mut keys = vec![trusted];
        for index in 1..=MAX_TRUSTED_SIGNING_KEYS {
            keys.push(TrustedSigningKey {
                key_id: format!("extra-{index}"),
                public_key: [0_u8; 32],
            });
        }
        let error = verify_and_parse_index(&index_bytes, &signature, &keys, now)
            .expect_err("oversized key set must be rejected");
        assert!(error.contains("key bound"), "{error}");
    }

    /// Verifies index entry shape validation: revision, SHA-256 hex, and byte-count bounds.
    /// 验证 Index 条目形态校验：修订号、SHA-256 十六进制与字节数上限。
    #[test]
    fn index_entry_shape_validation_is_enforced() {
        let (secret, trusted) = test_key("test-key-1");
        let now = 1_000_000;
        let valid_entry = RemoteProviderEntry {
            provider_id: String::from("synced-provider"),
            revision: 5,
            relative_url: String::from("/p.json"),
            sha256: "a".repeat(SHA256_HEX_LENGTH),
            bytes: 1,
            minimum_engine_contract: 1,
        };

        let mut zero_revision = valid_entry.clone();
        zero_revision.revision = 0;
        let index_bytes =
            build_index_document(vec![zero_revision], 3, "test-key-1", now - 1000, now + 1000);
        let signature = sign_index_document(&index_bytes, &secret).expect("sign should work");
        let error = verify_and_parse_index(
            &index_bytes,
            &signature,
            std::slice::from_ref(&trusted),
            now,
        )
        .expect_err("zero revision must be rejected");
        assert!(error.contains("revision"), "{error}");

        let mut short_sha = valid_entry.clone();
        short_sha.sha256 = String::from("abc");
        let index_bytes =
            build_index_document(vec![short_sha], 3, "test-key-1", now - 1000, now + 1000);
        let signature = sign_index_document(&index_bytes, &secret).expect("sign should work");
        let error = verify_and_parse_index(
            &index_bytes,
            &signature,
            std::slice::from_ref(&trusted),
            now,
        )
        .expect_err("short SHA-256 must be rejected");
        assert!(error.contains("SHA-256"), "{error}");

        // The executable validator must match the JSON Schema's lowercase digest pattern.
        // 可执行验证器必须与 JSON Schema 的小写摘要模式一致。
        let mut uppercase_sha = valid_entry.clone();
        uppercase_sha.sha256 = "A".repeat(SHA256_HEX_LENGTH);
        let index_bytes =
            build_index_document(vec![uppercase_sha], 3, "test-key-1", now - 1000, now + 1000);
        let signature = sign_index_document(&index_bytes, &secret).expect("sign should work");
        let error = verify_and_parse_index(
            &index_bytes,
            &signature,
            std::slice::from_ref(&trusted),
            now,
        )
        .expect_err("uppercase SHA-256 must be rejected");
        assert!(error.contains("lowercase hex"), "{error}");

        let mut oversized_bytes = valid_entry.clone();
        oversized_bytes.bytes = REMOTE_PROVIDER_MAX_BYTES + 1;
        let index_bytes = build_index_document(
            vec![oversized_bytes],
            3,
            "test-key-1",
            now - 1000,
            now + 1000,
        );
        let signature = sign_index_document(&index_bytes, &secret).expect("sign should work");
        let error = verify_and_parse_index(
            &index_bytes,
            &signature,
            std::slice::from_ref(&trusted),
            now,
        )
        .expect_err("oversized provider bytes must be rejected");
        assert!(error.contains("byte count"), "{error}");

        // oversizedIdentity is signed but cannot become a cross-platform Store filename.
        // oversizedIdentity 已签名，但不能成为跨平台 Store 文件名。
        let mut oversized_identity = valid_entry;
        oversized_identity.provider_id = "a"
            .repeat(vulcan_provider_adapters::provider_manifest::MAX_PROVIDER_IDENTIFIER_LEN + 1);
        let index_bytes = build_index_document(
            vec![oversized_identity],
            3,
            "test-key-1",
            now - 1000,
            now + 1000,
        );
        let signature = sign_index_document(&index_bytes, &secret).expect("sign should work");
        let error = verify_and_parse_index(
            &index_bytes,
            &signature,
            std::slice::from_ref(&trusted),
            now,
        )
        .expect_err("oversized provider identity must fail before Store use");
        assert!(error.contains("path-safe ASCII"), "{error}");
    }

    /// Verifies a duplicated provider id key is rejected as a shape violation.
    /// 验证重复供应商 ID 键作为形态违规被拒绝。
    #[test]
    fn index_entry_provider_id_key_mismatch_is_rejected() {
        let (secret, trusted) = test_key("test-key-1");
        let now = 1_000_000;
        let mut entry = RemoteProviderEntry {
            provider_id: String::from("expected-id"),
            revision: 5,
            relative_url: String::from("/p.json"),
            sha256: "a".repeat(SHA256_HEX_LENGTH),
            bytes: 1,
            minimum_engine_contract: 1,
        };
        let mut index = ManifestRemoteIndex {
            schema_version: String::from(INDEX_SCHEMA_VERSION),
            index_revision: 3,
            issued_at_unix_ms: now - 1000,
            expires_at_unix_ms: now + 1000,
            signing_key_id: String::from("test-key-1"),
            providers: BTreeMap::new(),
        };
        entry.provider_id = String::from("forged-id");
        index
            .providers
            .insert(String::from("expected-id"), entry.clone());
        let index_bytes = serde_json::to_vec(&index).expect("index should encode");
        let signature = sign_index_document(&index_bytes, &secret).expect("sign should work");
        let error = verify_and_parse_index(&index_bytes, &signature, &[trusted], now)
            .expect_err("key mismatch must be rejected");
        assert!(error.contains("does not match"), "{error}");
    }

    /// Verifies the full sync flow: stage, activate, and up-to-date short-circuit.
    /// 验证完整同步流程：staging、激活与 up-to-date 短路。
    #[test]
    fn sync_flow_stages_activates_and_short_circuits() {
        let directory = tempfile::tempdir().expect("temp dir should create");
        let store = crate::manifest_store::ManifestStoreLayout::new(directory.path());
        let text = bundled_manifest_with_revision("deepseek", 9);
        let bytes = text.as_bytes();
        let entry = remote_entry_for_text("deepseek", 9, &text);
        let outcome = activate_synced_provider(&store, &entry, bytes, None, None, None)
            .expect("sync should activate");
        match outcome {
            SyncOutcome::Activated {
                revision, manifest, ..
            } => {
                assert_eq!(revision, 9);
                assert_eq!(manifest.descriptor.id, "deepseek");
            }
            _ => panic!("first sync must activate"),
        }
        assert_eq!(
            store.read_active("deepseek").expect("read active"),
            Some(text.clone())
        );

        let outcome = activate_synced_provider(&store, &entry, bytes, None, None, None)
            .expect("second sync should short-circuit");
        assert!(matches!(outcome, SyncOutcome::UpToDate));

        let changed_text = bundled_manifest_with_revision("deepseek", 10);
        let changed = remote_entry_for_text("deepseek", 10, &changed_text);
        let outcome =
            activate_synced_provider(&store, &changed, changed_text.as_bytes(), None, None, None)
                .expect("new revision should activate");
        assert!(matches!(
            outcome,
            SyncOutcome::Activated { revision: 10, .. }
        ));
    }

    /// Verifies remote provider URLs stay same-origin and reject traversal components.
    /// 验证远端供应商 URL 保持同源并拒绝穿越组件。
    #[test]
    fn remote_url_resolution_enforces_same_origin() {
        assert!(validate_remote_relative_url("/providers/deepseek/9.json").is_ok());
        assert!(validate_remote_relative_url("/v1/manifests/a.json").is_ok());
        for unsafe_url in [
            "deepseek.json",
            "//evil.example/providers/deepseek.json",
            "https://evil.example/providers/deepseek.json",
            "/providers/../deepseek.json",
            "/providers/%2e%2e/deepseek.json",
            "/providers/%2E%2E%2fdeepseek.json",
            "/providers/deepseek.json?x=1",
            "/providers/deepseek.json#frag",
            "/providers\\deepseek.json",
            "/providers/./deepseek.json",
        ] {
            let error =
                validate_remote_relative_url(unsafe_url).expect_err("unsafe URL must be rejected");
            assert!(!error.is_empty());
        }
        assert_eq!(
            resolve_remote_provider_url(
                "https://manifests.example.com",
                "/providers/deepseek/9.json",
            )
            .expect("resolved URL"),
            "https://manifests.example.com/providers/deepseek/9.json"
        );
        let error = resolve_remote_provider_url("http://insecure.example.com", "/p.json")
            .expect_err("non-HTTPS origin must be rejected");
        assert!(error.contains("HTTPS"), "{error}");
        for unsafe_origin in [
            "https://user:secret@manifests.example.com",
            "https://manifests.example.com/base",
            "https://manifests.example.com?source=remote",
            "https://manifests.example.com#remote",
            "https://manifests.example.com\\base",
        ] {
            let error = resolve_remote_provider_url(unsafe_origin, "/p.json")
                .expect_err("non-bare origin must be rejected");
            assert!(error.contains("bare scheme+authority"), "{error}");
        }
    }

    /// Verifies sync rejects revision downgrades and same-revision content conflicts.
    /// 验证同步拒绝修订降级与同修订内容冲突。
    #[test]
    fn sync_rejects_downgrade_and_same_revision_conflict() {
        let directory = tempfile::tempdir().expect("temp dir should create");
        let store = crate::manifest_store::ManifestStoreLayout::new(directory.path());
        let text = bundled_manifest_with_revision("deepseek", 9);
        let bytes = text.as_bytes();
        let entry = remote_entry_for_text("deepseek", 9, &text);
        activate_synced_provider(&store, &entry, bytes, None, None, None)
            .expect("first sync should activate");

        // The same revision with different pinned content must be quarantined. The second
        // candidate is the same provider with a re-serialized revision field, so its own
        // pin matches its bytes, verification passes, and the per-provider state conflict
        // is what rejects it.
        // 同一修订但固定内容不同必须被隔离。第二个候选是同一供应商但重序列化修订字段的
        // 副本，其自身固定值与字节匹配、校验通过，拒绝它的是逐供应商状态冲突。
        let mut value: serde_json::Value =
            serde_json::from_str(&text).expect("manifest should parse for mutation");
        value["published_at"] = serde_json::json!("2026-08-07T00:00:00Z");
        let other_text = serde_json::to_string(&value).expect("manifest should serialize");
        let other_bytes = other_text.as_bytes();
        let conflict = remote_entry_for_text("deepseek", 9, &other_text);
        let outcome = activate_synced_provider(&store, &conflict, other_bytes, None, None, None)
            .expect("conflict must be handled");
        assert!(matches!(
            outcome,
            SyncOutcome::Rejected { failure_kind, .. } if failure_kind == "same_revision_conflict"
        ));

        // An older revision must never replace the active layer.
        // 更旧修订绝不允许替换活动层。
        let downgrade_text = bundled_manifest_with_revision("deepseek", 8);
        let downgrade = remote_entry_for_text("deepseek", 8, &downgrade_text);
        let outcome = activate_synced_provider(
            &store,
            &downgrade,
            downgrade_text.as_bytes(),
            None,
            None,
            None,
        )
        .expect("downgrade must be handled");
        assert!(matches!(
            outcome,
            SyncOutcome::Rejected { failure_kind, .. } if failure_kind == "revision_downgrade"
        ));
        assert_eq!(
            store.read_active("deepseek").expect("read active"),
            Some(text.clone()),
            "the active layer must stay untouched"
        );

        // The same revision and content short-circuits as up-to-date.
        // 相同修订与内容短路为 up-to-date。
        let outcome = activate_synced_provider(&store, &entry, bytes, None, None, None)
            .expect("same revision must short-circuit");
        assert!(matches!(outcome, SyncOutcome::UpToDate));
    }

    /// Verifies staged production activation stays recoverable until its caller explicitly commits.
    /// 验证生产暂存激活在调用方显式提交前始终可恢复。
    #[test]
    fn staged_activation_retains_applying_journal_until_projection_commit() {
        // recoveryStore exercises crash recovery while commitStore exercises the success boundary.
        // recoveryStore 验证崩溃恢复，commitStore 验证成功提交边界。
        let recovery_directory = tempfile::tempdir().expect("recovery directory should create");
        let recovery_store =
            crate::manifest_store::ManifestStoreLayout::new(recovery_directory.path());
        let text = bundled_manifest_with_revision("deepseek", 9);
        let entry = remote_entry_for_text("deepseek", 9, &text);
        let recovery_outcome = stage_synced_provider_activation(
            &recovery_store,
            &entry,
            text.as_bytes(),
            None,
            None,
            None,
        )
        .expect("staged activation should succeed");
        assert!(matches!(recovery_outcome, SyncOutcome::Activated { .. }));
        assert!(
            recovery_store
                .recover_system_transaction("deepseek")
                .expect("Applying transaction should recover")
        );
        assert_eq!(
            recovery_store
                .read_active("deepseek")
                .expect("recovered active should read"),
            None,
            "startup recovery must restore the exact absent before-image"
        );

        let commit_directory = tempfile::tempdir().expect("commit directory should create");
        let commit_store = crate::manifest_store::ManifestStoreLayout::new(commit_directory.path());
        let commit_outcome = stage_synced_provider_activation(
            &commit_store,
            &entry,
            text.as_bytes(),
            None,
            None,
            None,
        )
        .expect("second staged activation should succeed");
        let store_snapshot = match commit_outcome {
            SyncOutcome::Activated { store_snapshot, .. } => store_snapshot,
            _ => panic!("second staged activation must produce an activation snapshot"),
        };
        commit_store
            .commit_system_transaction("deepseek", &store_snapshot)
            .expect("projection commit should close the transaction");
        assert!(
            !commit_store
                .recover_system_transaction("deepseek")
                .expect("committed transaction should leave no recovery journal")
        );
        assert_eq!(
            commit_store
                .read_active("deepseek")
                .expect("committed active should read"),
            Some(text)
        );
    }

    /// Verifies provider bytes whose manifest identity disagrees with the pin are rejected.
    /// 验证清单身份与固定值不一致的供应商字节被拒绝。
    #[test]
    fn provider_bytes_reject_identity_mismatch() {
        let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
        let text = std::fs::read_to_string(directory.join("deepseek.json"))
            .expect("bundled deepseek manifest must exist");
        let bytes = text.as_bytes();
        let digest = {
            use sha2::Digest;
            sha2::Sha256::digest(bytes)
        };
        let entry = RemoteProviderEntry {
            provider_id: String::from("alibaba"),
            revision: 1,
            relative_url: String::from("/p.json"),
            sha256: hex::encode(digest),
            bytes: bytes.len() as u64,
            minimum_engine_contract: 1,
        };
        let error = verify_provider_bytes(&entry, bytes).expect_err("identity mismatch must fail");
        assert_eq!(
            error.kind(),
            ProviderManifestVerificationFailureKind::IdentityMismatch
        );
        assert!(error.to_string().contains("declare provider"), "{error}");
    }

    /// Verifies every deterministic activation failpoint restores the exact durable before-image.
    /// 验证每个确定性激活故障点都会恢复精确持久前镜像。
    #[test]
    fn activation_failpoints_restore_active_previous_and_state() {
        for failpoint in [
            ProviderActivationStage::PublishActive,
            ProviderActivationStage::WriteState,
            ProviderActivationStage::CommitTransaction,
        ] {
            let directory = tempfile::tempdir().expect("temp dir should create");
            let store = crate::manifest_store::ManifestStoreLayout::new(directory.path());
            let first_text = bundled_manifest_with_revision("deepseek", 1);
            let first_entry = remote_entry_for_text("deepseek", 1, &first_text);
            activate_synced_provider(
                &store,
                &first_entry,
                first_text.as_bytes(),
                None,
                None,
                None,
            )
            .expect("first revision should activate");
            let before = store
                .capture_system_snapshot("deepseek")
                .expect("before-image should capture");
            let second_text = bundled_manifest_with_revision("deepseek", 2);
            let second_entry = remote_entry_for_text("deepseek", 2, &second_text);
            let result = activate_synced_provider_with_stage_hook(
                &store,
                &second_entry,
                second_text.as_bytes(),
                ProviderActivationTransactionOptions {
                    etag: Some(String::from("revision-two")),
                    index_etag: Some(String::from("index-two")),
                    last_modified: Some(String::from("last-modified-two")),
                    commit_transaction: true,
                },
                |stage| {
                    if stage == failpoint {
                        Err(format!("injected {failpoint:?}"))
                    } else {
                        Ok(())
                    }
                },
            );
            let error = match result {
                Ok(_) => panic!("injected activation stage must fail"),
                Err(error) => error,
            };
            assert!(error.contains("injected"), "{error}");
            assert_eq!(
                store
                    .capture_system_snapshot("deepseek")
                    .expect("restored snapshot should capture"),
                before,
                "failpoint {failpoint:?} must restore every authoritative file"
            );
            assert!(
                !store
                    .recover_system_transaction("deepseek")
                    .expect("transaction recovery should remain healthy"),
                "failpoint {failpoint:?} must clear its applying journal"
            );
        }
    }

    /// Verifies an index whose map key disagrees with its entry provider id is rejected.
    /// 验证 Map 键与条目供应商 ID 不一致的 Index 被拒绝。
    #[test]
    fn index_rejects_key_entry_mismatch() {
        let (secret, trusted) = test_key("test-key-1");
        let mut index = ManifestRemoteIndex {
            schema_version: String::from(INDEX_SCHEMA_VERSION),
            index_revision: 3,
            issued_at_unix_ms: 999_000,
            expires_at_unix_ms: 1_001_000,
            signing_key_id: String::from("test-key-1"),
            providers: BTreeMap::new(),
        };
        index.providers.insert(
            String::from("forged-key"),
            RemoteProviderEntry {
                provider_id: String::from("deepseek"),
                revision: 1,
                relative_url: String::from("/p.json"),
                sha256: "0".repeat(64),
                bytes: 4,
                minimum_engine_contract: 1,
            },
        );
        let index_bytes = serde_json::to_vec(&index).expect("index should serialize");
        let signature = sign_index_document(&index_bytes, &secret).expect("sign should work");
        let error = verify_and_parse_index(
            &index_bytes,
            &signature,
            std::slice::from_ref(&trusted),
            1_000_000,
        )
        .expect_err("forged key must be rejected");
        assert!(error.contains("does not match entry provider"), "{error}");
    }
}
