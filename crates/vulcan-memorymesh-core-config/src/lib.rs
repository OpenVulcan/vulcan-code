//! Authoritative embedded resources, typed configuration, and immutable snapshots.
//! 权威嵌入资源、强类型配置与不可变快照。

mod noise;
mod pii;
mod pii_condition;
mod source_regex;

pub use noise::{
    DEFAULT_SEMANTIC_THRESHOLD, NOISE_RULE_SCHEMA_VERSION, NoiseDecision, NoiseDetector,
    NoisePhraseTarget, NoisePolicy, NoisePrototypeRow,
};
pub use pii::{PiiRuleTestMatch, PiiRuleTestResult, PiiScrubResult, PiiScrubber, test_pii_rule};

use include_dir::{Dir, include_dir};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use vulcan_memorymesh_core_contracts::{
    ConfigSnapshotId, CoreError, ErrorCode, ErrorStage, MemoryLevel, MemoryStability,
    PreCheckPolicy, RESOURCE_FORMAT_VERSION, RetrievalPolicy, ScoreContractDescriptor,
};

/// Embeds the exact authoritative resource tree into the Core artifact.
/// 将精确的权威资源树嵌入 Core 工件。
static EMBEDDED_RESOURCES: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/resources");

/// Names the required prompt scenes shared by all bundles.
/// 命名所有 Prompt Bundle 共享的必需场景。
pub const REQUIRED_PROMPT_SCENES: [&str; 5] = [
    "precheck_l1_main.md",
    "precheck_l2_main.md",
    "postaction_l1_main.md",
    "postaction_l2_main.md",
    "profile_instruction_main.md",
];

/// Identifies the typed memory-policy schema compiled into one snapshot.
/// 标识编译进一个快照的强类型记忆策略 Schema。
pub const MEMORY_POLICY_SCHEMA_VERSION: u32 = 7;

/// Defines the maximum UTF-8 byte length of one configuration identifier or selector.
/// 定义单个配置标识或选择器允许的最大 UTF-8 字节长度。
pub const MAX_CONFIG_IDENTIFIER_BYTES: usize = 512;

/// Defines the maximum UTF-8 byte length of one normalized override resource path.
/// 定义单个规范化覆盖资源路径允许的最大 UTF-8 字节长度。
pub const MAX_RESOURCE_PATH_BYTES: usize = 512;

/// Defines the maximum byte length of one textual override resource.
/// 定义单个文本覆盖资源允许的最大字节长度。
pub const MAX_RESOURCE_FILE_BYTES: usize = 4 * 1024 * 1024;

/// Defines the maximum number of override resources carried by one precedence layer.
/// 定义单个优先级层允许携带的最大覆盖资源数量。
pub const MAX_RESOURCE_LAYER_FILES: usize = 256;

/// Defines the maximum aggregate bytes carried by one override precedence layer.
/// 定义单个覆盖优先级层允许携带的最大聚合字节数。
pub const MAX_RESOURCE_LAYER_BYTES: usize = 16 * 1024 * 1024;

/// Defines the maximum supported embedding dimension for configuration and Host facts.
/// 定义配置与 Host 事实支持的最大嵌入维度。
pub const MAX_EMBEDDING_DIMENSION: u32 = 65_536;

/// Defines deterministic memory policies shared by every host adapter.
/// 定义由所有宿主适配器共享的确定性记忆策略。
#[derive(Clone, Debug, PartialEq)]
pub struct MemoryPolicy {
    /// Identifies the closed policy schema understood by this Core version.
    /// 标识当前 Core 版本理解的封闭策略 Schema。
    pub schema_version: u32,
    /// Carries the default lifetime of session-owned direct memories.
    /// 携带会话归属主动记忆的默认生命周期。
    pub direct_memory_session_expiry_ms: u64,
    /// Carries the default lifetime of environment-owned direct memories.
    /// 携带环境归属主动记忆的默认生命周期。
    pub direct_memory_environment_expiry_ms: u64,
    /// Carries the default lifetime of subject-owned direct memories.
    /// 携带主体归属主动记忆的默认生命周期。
    pub direct_memory_subject_expiry_ms: u64,
    /// Limits already-refined historical turns loaded for `PostAction` L1.
    /// 限制为 `PostAction` L1 加载的已提炼历史轮次数量。
    pub postaction_history_turn_limit: u32,
    /// Limits the approximate target-plus-history token budget for `PostAction` L1.
    /// 限制 `PostAction` L1 目标轮次与历史上下文的近似 Token 预算。
    pub postaction_max_input_tokens: u32,
    /// Carries the normalized default priority for turn-extracted memories.
    /// 携带轮次提炼记忆的规范化默认优先级。
    pub postaction_memory_priority: u32,
    /// Classifies the default retention stability for turn-extracted memories.
    /// 对轮次提炼记忆的默认保留稳定性进行分类。
    pub postaction_memory_stability: MemoryStability,
    /// Classifies the default lifecycle level for turn-extracted memories.
    /// 对轮次提炼记忆的默认生命周期等级进行分类。
    pub postaction_memory_level: MemoryLevel,
    /// Carries the initial refresh weight for turn-extracted memories.
    /// 携带轮次提炼记忆的初始刷新权重。
    pub postaction_memory_refresh_weight: u32,
    /// Carries the default lifetime in milliseconds for turn-extracted memories.
    /// 携带轮次提炼记忆的默认生命周期毫秒数。
    pub postaction_memory_expiry_ms: u64,
    /// Carries the complete strong-typed retrieval strategy owned by Core.
    /// 携带由 Core 拥有的完整强类型检索策略。
    pub retrieval_policy: RetrievalPolicy,
    /// Carries independent `PreCheck` query and redacted-review admission rules.
    /// 携带独立的 `PreCheck` 查询及脱敏审核准入规则。
    pub precheck_policy: PreCheckPolicy,
}

impl Default for MemoryPolicy {
    /// Creates the frozen Core v1 memory policy.
    /// 创建冻结的 Core v1 记忆策略。
    fn default() -> Self {
        Self {
            schema_version: MEMORY_POLICY_SCHEMA_VERSION,
            direct_memory_session_expiry_ms: 15 * 24 * 60 * 60 * 1_000,
            direct_memory_environment_expiry_ms: 180 * 24 * 60 * 60 * 1_000,
            direct_memory_subject_expiry_ms: 365 * 24 * 60 * 60 * 1_000,
            postaction_history_turn_limit: 3,
            postaction_max_input_tokens: 6_000,
            postaction_memory_priority: 20,
            postaction_memory_stability: MemoryStability::Stable,
            postaction_memory_level: MemoryLevel::Stable,
            postaction_memory_refresh_weight: 1,
            postaction_memory_expiry_ms: 180 * 24 * 60 * 60 * 1_000,
            retrieval_policy: RetrievalPolicy::v1(),
            precheck_policy: PreCheckPolicy::default(),
        }
    }
}

/// Classifies the semantic impact of changing immutable snapshots.
/// 对不可变快照变更的语义影响进行分类。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConfigImpact {
    /// Changes prompts or bounded runtime policy for newly started commands.
    /// 变更新命令使用的 Prompt 或有界运行策略。
    RuntimePolicy,
    /// Changes durable data interpretation such as PII or Noise semantics.
    /// 变更 PII 或 Noise 等持久数据解释语义。
    DataSemantics,
    /// Changes the host-declared embedding vector-space identity.
    /// 变更宿主声明的嵌入向量空间身份。
    VectorSpace,
}

/// Carries host-provided resource files for one precedence layer.
/// 携带一个优先级层的宿主资源文件。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResourceLayer {
    /// Maps normalized slash-separated resource paths to exact bytes.
    /// 将规范化斜杠分隔资源路径映射到精确字节。
    pub files: BTreeMap<String, Vec<u8>>,
}

impl ResourceLayer {
    /// Inserts one validated relative resource path.
    /// 插入一条经过校验的相对资源路径。
    ///
    /// # Errors
    /// Returns an error for invalid paths, empty or oversized bytes, duplicate paths, or layer limits.
    /// 当路径无效、字节为空或超限、路径重复或层级总量超限时返回错误。
    pub fn insert(&mut self, path: impl Into<String>, bytes: Vec<u8>) -> Result<(), CoreError> {
        // Materialize the path once so validation and insertion use identical text.
        // 仅实例化一次路径，确保校验与插入使用相同文本。
        let path = path.into();
        validate_resource_entry(&path, &bytes)?;
        if self.files.contains_key(&path) {
            return Err(config_error(
                ErrorCode::InvalidConfiguration,
                format!("duplicate resource path in one layer: {path}"),
            ));
        }
        if self.files.len() >= MAX_RESOURCE_LAYER_FILES {
            return Err(config_error(
                ErrorCode::InvalidConfiguration,
                format!(
                    "resource layer exceeds {MAX_RESOURCE_LAYER_FILES} files after inserting: {path}"
                ),
            ));
        }
        let current_bytes = self.files.values().try_fold(0_usize, |total, value| {
            total.checked_add(value.len()).ok_or_else(|| {
                config_error(
                    ErrorCode::InvalidConfiguration,
                    "resource layer byte total overflowed",
                )
            })
        })?;
        if current_bytes
            .checked_add(bytes.len())
            .is_none_or(|total| total > MAX_RESOURCE_LAYER_BYTES)
        {
            return Err(config_error(
                ErrorCode::InvalidConfiguration,
                format!(
                    "resource layer exceeds {MAX_RESOURCE_LAYER_BYTES} aggregate bytes after inserting: {path}"
                ),
            ));
        }
        self.files.insert(path, bytes);
        Ok(())
    }
}

/// Selects resource bundles and optional host precedence layers.
/// 选择资源 Bundle 与可选宿主优先级层。
#[derive(Clone, Debug, PartialEq)]
pub struct ConfigBundle {
    /// Carries explicit Noise switches and the default semantic threshold.
    /// 携带显式 Noise 开关与默认语义阈值。
    pub noise_policy: NoisePolicy,
    /// Names the host-visible configuration registration.
    /// 命名宿主可见配置注册项。
    pub name: String,
    /// Selects the prompt bundle or one documented alias.
    /// 选择 Prompt Bundle 或一个已记录别名。
    pub prompt_bundle: String,
    /// Selects the locale-specific PII bundle.
    /// 选择特定区域的 PII Bundle。
    pub pii_language: String,
    /// Selects the locale-specific Noise bundle.
    /// 选择特定区域的 Noise Bundle。
    pub noise_language: String,
    /// Carries the typed memory policy compiled into the snapshot.
    /// 携带编译进快照的强类型记忆策略。
    pub memory_policy: MemoryPolicy,
    /// Identifies the host-resolved vector-space semantics without naming a provider.
    /// 标识宿主解析后的向量空间语义且不命名 Provider。
    pub vector_space_fingerprint: String,
    /// Carries the exact embedding dimension required by this vector space.
    /// 携带该向量空间要求的精确嵌入维度。
    pub embedding_dimension: u32,
    /// Carries product-owned overrides above built-in resources.
    /// 携带高于内置资源的产品层覆盖。
    pub product_layer: ResourceLayer,
    /// Carries user-owned overrides above product resources.
    /// 携带高于产品资源的用户层覆盖。
    pub user_layer: ResourceLayer,
}

impl Default for ConfigBundle {
    /// Creates the documented default resource selection.
    /// 创建文档规定的默认资源选择。
    fn default() -> Self {
        Self {
            noise_policy: NoisePolicy::default(),
            name: "default".to_owned(),
            prompt_bundle: "default_en".to_owned(),
            pii_language: "zh-CN".to_owned(),
            noise_language: "zh-CN".to_owned(),
            memory_policy: MemoryPolicy::default(),
            vector_space_fingerprint: "host-resolved-vector-space-v1".to_owned(),
            embedding_dimension: 32,
            product_layer: ResourceLayer::default(),
            user_layer: ResourceLayer::default(),
        }
    }
}

/// Reports the authoritative embedded resource identity.
/// 报告权威嵌入资源身份。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceIdentity {
    /// Reports the manifest format version.
    /// 报告清单格式版本。
    pub manifest_version: u32,
    /// Reports the resource package version.
    /// 报告资源包版本。
    pub package_version: String,
    /// Reports the number of authoritative files excluding the manifest.
    /// 报告不含清单的权威文件数量。
    pub file_count: usize,
    /// Reports the aggregate authoritative byte length.
    /// 报告权威文件字节总长度。
    pub total_bytes: u64,
    /// Reports a digest over manifest-declared paths, lengths, and file digests.
    /// 报告覆盖清单声明路径、长度与文件摘要的总摘要。
    pub digest: String,
}

/// Represents one compiled immutable configuration snapshot.
/// 表示一个已编译的不可变配置快照。
#[derive(Clone)]
pub struct ConfigSnapshot {
    /// Carries the canonical compiled Noise policy bound into this snapshot identity.
    /// 携带绑定进此快照身份的规范化 Noise 策略。
    pub noise_policy: NoisePolicy,
    /// Identifies the immutable snapshot.
    /// 标识不可变快照。
    pub id: ConfigSnapshotId,
    /// Names the source registration.
    /// 命名来源注册项。
    pub name: String,
    /// Names the normalized prompt bundle.
    /// 命名规范化后的 Prompt Bundle。
    pub prompt_bundle: String,
    /// Names the selected PII language.
    /// 命名选定的 PII 语言。
    pub pii_language: String,
    /// Names the selected Noise language.
    /// 命名选定的 Noise 语言。
    pub noise_language: String,
    /// Carries the compiled immutable memory policy.
    /// 携带已编译的不可变记忆策略。
    pub memory_policy: MemoryPolicy,
    /// Carries the compiled immutable retrieval strategy owned by Core.
    /// 携带由 Core 拥有的已编译不可变检索策略。
    pub retrieval_policy: RetrievalPolicy,
    /// Reports the stable digest of the compiled memory policy.
    /// 报告已编译记忆策略的稳定摘要。
    pub memory_policy_digest: String,
    /// Reports the stable digest of resolved prompt bytes.
    /// 报告已解析 Prompt 字节的稳定摘要。
    pub prompt_digest: String,
    /// Reports the stable digest of resolved PII rule bytes.
    /// 报告已解析 PII 规则字节的稳定摘要。
    pub pii_digest: String,
    /// Reports the stable digest of resolved Noise rule bytes.
    /// 报告已解析 Noise 规则字节的稳定摘要。
    pub noise_digest: String,
    /// Identifies the host-resolved vector-space semantics.
    /// 标识宿主解析后的向量空间语义。
    pub vector_space_fingerprint: String,
    /// Carries the exact embedding dimension frozen into this snapshot.
    /// 携带冻结进该快照的精确嵌入维度。
    pub embedding_dimension: u32,
    /// Carries all required prompt scene texts.
    /// 携带所有必需 Prompt 场景文本。
    pub prompts: BTreeMap<String, Arc<str>>,
    /// Carries the compiled PII scrubber.
    /// 携带已编译 PII 清洗器。
    pub pii_scrubber: PiiScrubber,
    /// Carries the compiled Noise detector.
    /// 携带已编译 Noise 检测器。
    pub noise_detector: NoiseDetector,
    /// Reports the built-in authoritative resource identity.
    /// 报告内置权威资源身份。
    pub resource_identity: ResourceIdentity,
}

impl std::fmt::Debug for ConfigSnapshot {
    /// Formats snapshot identity without dumping prompts or rule payloads.
    /// 格式化快照身份而不输出 Prompt 或规则载荷。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ConfigSnapshot")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("prompt_bundle", &self.prompt_bundle)
            .field("pii_language", &self.pii_language)
            .field("noise_language", &self.noise_language)
            .field("memory_policy_digest", &self.memory_policy_digest)
            .field("vector_space_fingerprint", &self.vector_space_fingerprint)
            .field("embedding_dimension", &self.embedding_dimension)
            .field("prompt_count", &self.prompts.len())
            .field("resource_identity", &self.resource_identity)
            .finish_non_exhaustive()
    }
}

impl ConfigSnapshot {
    /// Returns one required prompt scene.
    /// 返回一个必需 Prompt 场景。
    ///
    /// # Errors
    /// Returns an error when the scene is not part of the frozen bundle.
    /// 当场景不属于冻结 Bundle 时返回错误。
    pub fn prompt(&self, scene: &str) -> Result<&str, CoreError> {
        self.prompts.get(scene).map(AsRef::as_ref).ok_or_else(|| {
            config_error(
                ErrorCode::MissingResource,
                format!("prompt scene is not available in snapshot: {scene}"),
            )
            .with_snapshot(self.id.clone())
        })
    }

    /// Classifies every semantic impact introduced since another snapshot.
    /// 对相较另一快照引入的全部语义影响进行分类。
    #[must_use]
    pub fn impacts_compared_to(&self, previous: &Self) -> Vec<ConfigImpact> {
        let mut impacts = Vec::new();
        if self.prompt_digest != previous.prompt_digest
            || self.memory_policy_digest != previous.memory_policy_digest
        {
            impacts.push(ConfigImpact::RuntimePolicy);
        }
        if self.pii_digest != previous.pii_digest
            || self.noise_digest != previous.noise_digest
            || self.noise_policy != previous.noise_policy
        {
            impacts.push(ConfigImpact::DataSemantics);
        }
        if self.vector_space_fingerprint != previous.vector_space_fingerprint
            || self.embedding_dimension != previous.embedding_dimension
        {
            impacts.push(ConfigImpact::VectorSpace);
        }
        impacts
    }
}

/// Registers, compiles, activates, and retains immutable configuration snapshots.
/// 注册、编译、激活并保留不可变配置快照。
#[derive(Default)]
pub struct ConfigRegistry {
    /// Maps registration names to their newest snapshots.
    /// 将注册名称映射到其最新快照。
    registrations: BTreeMap<String, Arc<ConfigSnapshot>>,
    /// Retains every referenced snapshot by stable identity.
    /// 按稳定标识保留每个被引用快照。
    snapshots: BTreeMap<ConfigSnapshotId, Arc<ConfigSnapshot>>,
    /// Identifies the active snapshot.
    /// 标识当前活跃快照。
    active: Option<ConfigSnapshotId>,
}

impl ConfigRegistry {
    /// Creates an empty registry after validating embedded resource integrity.
    /// 在校验嵌入资源完整性后创建空注册表。
    ///
    /// # Errors
    /// Returns an error when the embedded manifest or any embedded file is invalid.
    /// 当嵌入清单或任一嵌入文件无效时返回错误。
    pub fn new() -> Result<Self, CoreError> {
        embedded_resource_identity()?;
        Ok(Self::default())
    }

    /// Compiles and registers a configuration without changing the active snapshot.
    /// 编译并注册配置，但不改变当前活跃快照。
    ///
    /// # Errors
    /// Returns an error for invalid names, resources, rules, or incomplete prompt overrides.
    /// 当名称、资源、规则无效或 Prompt 覆盖不完整时返回错误。
    pub fn register(&mut self, bundle: ConfigBundle) -> Result<Arc<ConfigSnapshot>, CoreError> {
        // Compile to a detached immutable value before publishing it in either registry map.
        // 在发布到任一注册表映射前，先编译成独立不可变值。
        let snapshot = Arc::new(compile_snapshot(bundle)?);
        self.snapshots.insert(snapshot.id.clone(), snapshot.clone());
        self.registrations
            .insert(snapshot.name.clone(), snapshot.clone());
        Ok(snapshot)
    }

    /// Activates the newest snapshot registered under one exact name.
    /// 激活一个精确名称下注册的最新快照。
    ///
    /// # Errors
    /// Returns an error when the registration does not exist.
    /// 当注册项不存在时返回错误。
    pub fn activate(&mut self, name: &str) -> Result<Arc<ConfigSnapshot>, CoreError> {
        // Clone the Arc so activation never exposes a mutable registry borrow.
        // 克隆 Arc，确保激活操作绝不暴露可变注册表借用。
        let snapshot = self.registrations.get(name).cloned().ok_or_else(|| {
            config_error(
                ErrorCode::SnapshotNotFound,
                format!("configuration registration not found: {name}"),
            )
        })?;
        self.active = Some(snapshot.id.clone());
        Ok(snapshot)
    }

    /// Returns the active immutable snapshot.
    /// 返回当前活跃的不可变快照。
    ///
    /// # Errors
    /// Returns an error when no configuration has been activated.
    /// 当尚未激活配置时返回错误。
    pub fn active_snapshot(&self) -> Result<Arc<ConfigSnapshot>, CoreError> {
        // Resolve the identity first so missing activation and corrupt registry state stay distinct.
        // 先解析标识，使缺少激活与注册表状态损坏保持区分。
        let snapshot_id = self.active.as_ref().ok_or_else(|| {
            config_error(
                ErrorCode::SnapshotNotFound,
                "no configuration snapshot is active",
            )
        })?;
        self.snapshots.get(snapshot_id).cloned().ok_or_else(|| {
            config_error(
                ErrorCode::Internal,
                "active configuration snapshot is missing from the registry",
            )
        })
    }

    /// Returns a retained snapshot by stable identity.
    /// 按稳定标识返回一个保留快照。
    #[must_use]
    pub fn snapshot(&self, snapshot_id: &ConfigSnapshotId) -> Option<Arc<ConfigSnapshot>> {
        self.snapshots.get(snapshot_id).cloned()
    }
}

/// Describes the on-disk manifest schema.
/// 描述磁盘清单 Schema。
#[derive(Deserialize)]
struct ResourceManifest {
    /// Carries the manifest format version.
    /// 携带清单格式版本。
    manifest_version: u32,
    /// Carries the package version.
    /// 携带资源包版本。
    resource_package_version: String,
    /// Carries the minimum Core version.
    /// 携带最低 Core 版本。
    minimum_core_version: String,
    /// Carries component schema versions.
    /// 携带组件 Schema 版本。
    schema_versions: SchemaVersions,
    /// Carries documented default selections and aliases.
    /// 携带已记录的默认选择与别名。
    defaults: ManifestDefaults,
    /// Carries documented layering contracts.
    /// 携带已记录的分层契约。
    layering: LayeringContracts,
    /// Carries prompt bundle requirements.
    /// 携带 Prompt Bundle 要求。
    prompts: PromptManifest,
    /// Carries authoritative file entries.
    /// 携带权威文件条目。
    files: Vec<ManifestFile>,
}

/// Describes component schema versions in the manifest.
/// 描述清单中的组件 Schema 版本。
#[derive(Deserialize)]
struct SchemaVersions {
    /// Carries the prompt schema version.
    /// 携带 Prompt Schema 版本。
    prompts: u32,
    /// Carries the PII schema version.
    /// 携带 PII Schema 版本。
    pii_rules: u32,
    /// Carries the Noise schema version.
    /// 携带 Noise Schema 版本。
    noise_rules: u32,
}

/// Describes default resource selections and prompt aliases.
/// 描述默认资源选择与 Prompt 别名。
#[derive(Deserialize)]
struct ManifestDefaults {
    /// Carries the default prompt bundle.
    /// 携带默认 Prompt Bundle。
    prompt_bundle: String,
    /// Carries the default PII language.
    /// 携带默认 PII 语言。
    pii_language: String,
    /// Carries the default Noise language.
    /// 携带默认 Noise 语言。
    noise_language: String,
    /// Carries aliases for the English default bundle.
    /// 携带英文默认 Bundle 的别名。
    prompt_aliases_default_en: Vec<String>,
    /// Carries aliases for the Chinese default bundle.
    /// 携带中文默认 Bundle 的别名。
    prompt_aliases_default_cn: Vec<String>,
}

/// Describes exact resource layering contracts.
/// 描述精确资源分层契约。
#[derive(Deserialize)]
struct LayeringContracts {
    /// Carries the prompt layering contract identity.
    /// 携带 Prompt 分层契约标识。
    #[serde(rename = "prompt_bundle")]
    prompt: String,
    /// Carries the PII layering contract identity.
    /// 携带 PII 分层契约标识。
    #[serde(rename = "pii_bundle")]
    pii: String,
    /// Carries the Noise layering contract identity.
    /// 携带 Noise 分层契约标识。
    #[serde(rename = "noise_bundle")]
    noise: String,
}

/// Describes prompt bundles and required scene files.
/// 描述 Prompt Bundle 与必需场景文件。
#[derive(Deserialize)]
struct PromptManifest {
    /// Carries available prompt bundles.
    /// 携带可用 Prompt Bundle。
    bundles: Vec<String>,
    /// Carries required scene files.
    /// 携带必需场景文件。
    required_scene_files: Vec<String>,
}

/// Describes one authoritative resource file entry.
/// 描述一条权威资源文件条目。
#[derive(Deserialize)]
struct ManifestFile {
    /// Carries the normalized relative path.
    /// 携带规范化相对路径。
    path: String,
    /// Carries the exact byte length.
    /// 携带精确字节长度。
    length: u64,
    /// Carries the lowercase SHA-256 digest.
    /// 携带小写 SHA-256 摘要。
    sha256: String,
}

/// Compiles one host configuration into an immutable snapshot.
/// 将一个宿主配置编译成不可变快照。
fn compile_snapshot(mut bundle: ConfigBundle) -> Result<ConfigSnapshot, CoreError> {
    // Resolve the documented zero default before hashing; reject nonfinite or out-of-range policy.
    // 在哈希前解析约定的零值默认行为；拒绝非有限或越界策略。
    if !(0.0..=1.0).contains(&bundle.noise_policy.default_semantic_threshold) {
        return Err(config_error(
            ErrorCode::InvalidConfiguration,
            "Noise default threshold must be finite and between zero and one",
        ));
    }
    if bundle.noise_policy.default_semantic_threshold == 0.0 {
        bundle.noise_policy.default_semantic_threshold = DEFAULT_SEMANTIC_THRESHOLD;
    }
    // Revalidate every public host-owned field here because callers can construct the public
    // structs directly without using `ResourceLayer::insert`.
    // 在此重新校验所有宿主拥有的公开字段，因为调用方可以不经 `ResourceLayer::insert` 直接构造公开结构体。
    validate_config_identifier("configuration registration name", &bundle.name)?;
    // The manifest explicitly declares the empty prompt selector as a default alias.
    // 清单明确将空 Prompt 选择器声明为默认别名。
    if !bundle.prompt_bundle.is_empty() {
        validate_config_identifier("prompt bundle", &bundle.prompt_bundle)?;
    }
    validate_config_identifier("PII language", &bundle.pii_language)?;
    validate_config_identifier("Noise language", &bundle.noise_language)?;
    validate_config_identifier("vector-space fingerprint", &bundle.vector_space_fingerprint)?;
    validate_resource_layer("product", &bundle.product_layer)?;
    validate_resource_layer("user", &bundle.user_layer)?;
    // Load the manifest once because aliases and resource identity are part of the snapshot hash.
    // 仅加载一次清单，因为别名与资源身份都是快照哈希的一部分。
    let manifest = load_manifest()?;
    validate_memory_policy(&bundle.memory_policy)?;
    if !(1..=MAX_EMBEDDING_DIMENSION).contains(&bundle.embedding_dimension) {
        return Err(config_error(
            ErrorCode::InvalidConfiguration,
            "embedding dimension must be between 1 and 65536",
        ));
    }
    // Normalize the prompt alias using the exact manifest-declared lists.
    // 使用清单精确声明的列表规范化 Prompt 别名。
    let prompt_bundle = normalize_prompt_bundle(&bundle.prompt_bundle, &manifest.defaults);
    if !manifest.prompts.bundles.contains(&prompt_bundle) {
        return Err(config_error(
            ErrorCode::MissingResource,
            format!("prompt bundle does not exist: {prompt_bundle}"),
        ));
    }
    // Resolve prompts by whole-bundle precedence so partial high-priority bundles are rejected.
    // 按完整 Bundle 优先级解析 Prompt，从而拒绝不完整的高优先级 Bundle。
    let prompt_bytes = resolve_prompt_bundle(
        &prompt_bundle,
        &manifest.prompts.required_scene_files,
        &bundle.product_layer,
        &bundle.user_layer,
    )?;
    // Resolve PII files independently because the existing contract replaces common and locale files per layer.
    // 独立解析 PII 文件，因为现有契约按层分别替换 common 与区域文件。
    let pii_files = resolve_language_files(
        "pii_rules",
        &bundle.pii_language,
        &bundle.product_layer,
        &bundle.user_layer,
    )?;
    // Resolve Noise files independently before category-level locale override compilation.
    // 在按类别执行区域覆盖编译前，独立解析 Noise 文件。
    let noise_files = resolve_language_files(
        "noise_rules",
        &bundle.noise_language,
        &bundle.product_layer,
        &bundle.user_layer,
    )?;
    // Compile rule engines before publishing any snapshot identity.
    // 在发布任何快照标识前编译规则引擎。
    let pii_scrubber = PiiScrubber::compile(&pii_files)?;
    // Compile Noise category overrides with the selected language identity.
    // 使用选定语言标识编译 Noise 类别覆盖。
    let noise_detector = NoiseDetector::compile(&noise_files, bundle.noise_policy)?;
    // Convert prompt bytes to validated UTF-8 immutable strings.
    // 将 Prompt 字节转换为经过 UTF-8 校验的不可变字符串。
    let prompts = prompt_bytes
        .into_iter()
        .map(|(scene, bytes)| {
            String::from_utf8(bytes)
                .map(|text| (scene, Arc::<str>::from(text)))
                .map_err(|error| {
                    config_error(
                        ErrorCode::InvalidConfiguration,
                        format!("prompt is not valid UTF-8: {error}"),
                    )
                })
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    // Calculate resource identity after all manifest integrity checks pass.
    // 在全部清单完整性检查通过后计算资源身份。
    let resource_identity = identity_from_manifest(&manifest)?;
    let prompt_digest = digest_text_map(&prompts);
    let pii_digest = digest_named_bytes(&pii_files);
    let noise_digest = digest_named_bytes(&noise_files);
    let memory_policy_digest = digest_memory_policy(&bundle.memory_policy);
    // Hash the registration identity plus resolved semantics and exact bytes so an equal named
    // registration and equal effective configuration share one identity.
    // 哈希注册身份、已解析语义与精确字节，使同名注册且有效配置相同的快照共享同一身份。
    let snapshot_id = snapshot_digest(
        &bundle,
        &prompt_bundle,
        &prompts,
        &pii_files,
        &noise_files,
        &resource_identity,
    )?;
    Ok(ConfigSnapshot {
        noise_policy: bundle.noise_policy,
        id: snapshot_id,
        name: bundle.name,
        prompt_bundle,
        pii_language: bundle.pii_language,
        noise_language: bundle.noise_language,
        memory_policy: bundle.memory_policy.clone(),
        retrieval_policy: bundle.memory_policy.retrieval_policy.clone(),
        memory_policy_digest,
        prompt_digest,
        pii_digest,
        noise_digest,
        vector_space_fingerprint: bundle.vector_space_fingerprint,
        embedding_dimension: bundle.embedding_dimension,
        prompts,
        pii_scrubber,
        noise_detector,
        resource_identity,
    })
}

/// Loads and validates the embedded manifest structure and fixed contracts.
/// 加载并校验嵌入清单结构与固定契约。
fn load_manifest() -> Result<ResourceManifest, CoreError> {
    // Resolve the embedded manifest by its fixed authoritative path.
    // 通过固定权威路径解析嵌入清单。
    let file = EMBEDDED_RESOURCES
        .get_file("resource-manifest.toml")
        .ok_or_else(|| config_error(ErrorCode::MissingResource, "embedded manifest is missing"))?;
    // Decode TOML as UTF-8 before typed parsing.
    // 在强类型解析前将 TOML 解码为 UTF-8。
    let text = std::str::from_utf8(file.contents()).map_err(|error| {
        config_error(
            ErrorCode::InvalidConfiguration,
            format!("embedded manifest is not UTF-8: {error}"),
        )
    })?;
    // Parse into a closed typed schema rather than a dynamic TOML value.
    // 解析为封闭的强类型 Schema，而不是动态 TOML 值。
    let manifest: ResourceManifest = toml::from_str(text).map_err(|error| {
        config_error(
            ErrorCode::InvalidConfiguration,
            format!("embedded manifest is invalid: {error}"),
        )
    })?;
    if manifest.manifest_version != RESOURCE_FORMAT_VERSION {
        return Err(config_error(
            ErrorCode::InvalidConfiguration,
            "embedded manifest version does not match Core",
        ));
    }
    if manifest.minimum_core_version != env!("CARGO_PKG_VERSION") {
        return Err(config_error(
            ErrorCode::InvalidConfiguration,
            "embedded manifest minimum Core version does not match this artifact",
        ));
    }
    if manifest.schema_versions.prompts != 1
        || manifest.schema_versions.pii_rules != 1
        || manifest.schema_versions.noise_rules != 1
    {
        return Err(config_error(
            ErrorCode::InvalidConfiguration,
            "embedded component schema version is unsupported",
        ));
    }
    if manifest.defaults.prompt_bundle != "default_en"
        || manifest.defaults.pii_language != "zh-CN"
        || manifest.defaults.noise_language != "zh-CN"
    {
        return Err(config_error(
            ErrorCode::InvalidConfiguration,
            "embedded default resource selections drifted",
        ));
    }
    if manifest.prompts.required_scene_files != REQUIRED_PROMPT_SCENES.map(str::to_owned).to_vec() {
        return Err(config_error(
            ErrorCode::InvalidConfiguration,
            "embedded required prompt scene list drifted",
        ));
    }
    if manifest.layering.prompt != "selected_user_bundle_replaces_system_bundle_else_system_bundle"
        || manifest.layering.pii != "user_language_or_common_bundle_replaces_same_system_bundle"
        || manifest.layering.noise
            != "user_file_precedes_system_file_then_language_category_overrides_common_category"
    {
        return Err(config_error(
            ErrorCode::InvalidConfiguration,
            "embedded layering contract identity drifted",
        ));
    }
    Ok(manifest)
}

/// Validates every embedded resource and computes the public identity.
/// 校验每个嵌入资源并计算公共身份。
fn identity_from_manifest(manifest: &ResourceManifest) -> Result<ResourceIdentity, CoreError> {
    // Track paths to reject duplicate manifest entries.
    // 跟踪路径以拒绝重复清单条目。
    let mut declared_paths = BTreeSet::new();
    // Hash stable manifest semantics independent of TOML formatting.
    // 哈希与 TOML 格式无关的稳定清单语义。
    let mut aggregate = Sha256::new();
    // Accumulate exact authoritative bytes.
    // 累加精确权威字节数。
    let mut total_bytes = 0_u64;
    for entry in &manifest.files {
        validate_resource_path(&entry.path)?;
        if !declared_paths.insert(entry.path.clone()) {
            return Err(config_error(
                ErrorCode::InvalidConfiguration,
                format!("duplicate manifest path: {}", entry.path),
            ));
        }
        // Resolve exactly one embedded file by the declared normalized path.
        // 通过声明的规范化路径精确解析一个嵌入文件。
        let file = EMBEDDED_RESOURCES.get_file(&entry.path).ok_or_else(|| {
            config_error(
                ErrorCode::MissingResource,
                format!("manifest resource is missing: {}", entry.path),
            )
        })?;
        if file.contents().len() as u64 != entry.length {
            return Err(config_error(
                ErrorCode::ResourceDigestMismatch,
                format!("resource length mismatch: {}", entry.path),
            ));
        }
        // Calculate the exact per-file SHA-256 digest.
        // 计算精确的逐文件 SHA-256 摘要。
        let digest = hex_digest(file.contents());
        if digest != entry.sha256 {
            return Err(config_error(
                ErrorCode::ResourceDigestMismatch,
                format!("resource digest mismatch: {}", entry.path),
            ));
        }
        aggregate.update(entry.path.as_bytes());
        aggregate.update([0]);
        aggregate.update(entry.length.to_le_bytes());
        aggregate.update(entry.sha256.as_bytes());
        aggregate.update([0]);
        total_bytes = total_bytes.saturating_add(entry.length);
    }
    // Count embedded files excluding the manifest to detect undeclared additions.
    // 统计不含清单的嵌入文件，以检测未声明新增项。
    let mut actual_paths = BTreeSet::new();
    collect_embedded_paths(&EMBEDDED_RESOURCES, &mut actual_paths)?;
    actual_paths.remove("resource-manifest.toml");
    if actual_paths != declared_paths {
        return Err(config_error(
            ErrorCode::ResourceDigestMismatch,
            "embedded resource set differs from the manifest",
        ));
    }
    Ok(ResourceIdentity {
        manifest_version: manifest.manifest_version,
        package_version: manifest.resource_package_version.clone(),
        file_count: manifest.files.len(),
        total_bytes,
        digest: format!("{:x}", aggregate.finalize()),
    })
}

/// Recursively collects every embedded file path without relying on shallow directory iteration.
/// 递归收集每个嵌入文件路径，不依赖浅层目录迭代。
fn collect_embedded_paths(
    directory: &Dir<'_>,
    paths: &mut BTreeSet<String>,
) -> Result<(), CoreError> {
    for file in directory.files() {
        // Convert the compile-time resource path to its normalized UTF-8 identity.
        // 将编译期资源路径转换为规范化 UTF-8 身份。
        let path = file.path().to_str().ok_or_else(|| {
            config_error(
                ErrorCode::InvalidConfiguration,
                "embedded resource path is not valid UTF-8",
            )
        })?;
        paths.insert(path.replace('\\', "/"));
    }
    for child in directory.dirs() {
        collect_embedded_paths(child, paths)?;
    }
    Ok(())
}

/// Returns the validated authoritative embedded resource identity.
/// 返回经过校验的权威嵌入资源身份。
///
/// # Errors
/// Returns an error when the manifest or any resource has drifted.
/// 当清单或任一资源发生漂移时返回错误。
pub fn embedded_resource_identity() -> Result<ResourceIdentity, CoreError> {
    // Parse and validate the manifest for every explicit identity request.
    // 为每次显式身份请求解析并校验清单。
    let manifest = load_manifest()?;
    identity_from_manifest(&manifest)
}

/// Lists every validated built-in text resource path declared by the authoritative manifest.
/// 列出权威清单声明的每个已校验内置文本资源路径。
///
/// # Returns
/// 返回值
/// Paths in manifest order after the complete embedded package passes integrity validation.
/// 完整嵌入资源包通过完整性校验后，按清单顺序返回路径。
///
/// # Errors
/// 错误
/// Returns an error when the manifest or any declared embedded resource has drifted.
/// 清单或任一声明的嵌入资源发生漂移时返回错误。
pub fn embedded_resource_paths() -> Result<Vec<String>, CoreError> {
    let manifest = load_manifest()?;
    identity_from_manifest(&manifest)?;
    Ok(manifest.files.into_iter().map(|file| file.path).collect())
}

/// Reads one validated built-in text resource without exposing the embedded directory implementation.
/// 读取一份已校验内置文本资源，且不暴露嵌入目录实现。
///
/// # Parameters
/// 参数
/// `path` must exactly match a path declared by the authoritative resource manifest.
/// `path` 必须准确匹配权威资源清单声明的路径。
///
/// # Returns
/// 返回值
/// The exact UTF-8 source text compiled into the Core artifact.
/// 返回编译进 Core 工件的准确 UTF-8 原文。
///
/// # Errors
/// 错误
/// Returns an error for package drift, undeclared paths, missing files, or invalid UTF-8.
/// 资源包漂移、路径未声明、文件缺失或 UTF-8 无效时返回错误。
pub fn embedded_resource_text(path: &str) -> Result<String, CoreError> {
    let manifest = load_manifest()?;
    identity_from_manifest(&manifest)?;
    if !manifest.files.iter().any(|file| file.path == path) {
        return Err(config_error(
            ErrorCode::MissingResource,
            format!("embedded resource path is not declared: {path}"),
        ));
    }
    let bytes = embedded_bytes(path)?;
    String::from_utf8(bytes).map_err(|error| {
        config_error(
            ErrorCode::InvalidConfiguration,
            format!("embedded resource is not UTF-8: {path}: {error}"),
        )
    })
}

/// Resolves one whole prompt bundle using built-in, product, and user precedence.
/// 使用内置、产品和用户优先级解析一个完整 Prompt Bundle。
fn resolve_prompt_bundle(
    bundle: &str,
    scenes: &[String],
    product: &ResourceLayer,
    user: &ResourceLayer,
) -> Result<BTreeMap<String, Vec<u8>>, CoreError> {
    // Select the highest layer containing any file from the requested bundle.
    // 选择包含请求 Bundle 任一文件的最高优先级层。
    let selected_layer = [user, product].into_iter().find(|layer| {
        layer
            .files
            .keys()
            .any(|path| path.starts_with(&format!("prompts/{bundle}/")))
    });
    // Resolve every scene from the selected whole-bundle layer or embedded resources.
    // 从选定完整 Bundle 层或嵌入资源解析每个场景。
    let mut resolved = BTreeMap::new();
    for scene in scenes {
        // Build the only valid prompt path for this bundle and scene.
        // 构建该 Bundle 与场景唯一有效的 Prompt 路径。
        let path = format!("prompts/{bundle}/{scene}");
        // Read from the selected complete override or authoritative embedded file.
        // 从选定完整覆盖或权威嵌入文件读取。
        let bytes = match selected_layer {
            Some(layer) => layer.files.get(&path).cloned().ok_or_else(|| {
                config_error(
                    ErrorCode::MissingResource,
                    format!("selected prompt override bundle is incomplete: {path}"),
                )
            })?,
            None => embedded_bytes(&path)?,
        };
        resolved.insert(scene.clone(), bytes);
    }
    Ok(resolved)
}

/// Resolves common and locale rule files independently by layer precedence.
/// 按层优先级独立解析 common 与区域规则文件。
fn resolve_language_files(
    root: &str,
    language: &str,
    product: &ResourceLayer,
    user: &ResourceLayer,
) -> Result<Vec<(String, Vec<u8>)>, CoreError> {
    if language.is_empty() || language.contains('/') || language.contains('\\') {
        return Err(config_error(
            ErrorCode::InvalidConfiguration,
            format!("invalid resource language: {language}"),
        ));
    }
    // Build the required common and selected locale paths in merge order.
    // 按合并顺序构建必需的 common 与选定区域路径。
    let paths = [
        format!("{root}/common.json"),
        format!("{root}/{language}.json"),
    ];
    paths
        .into_iter()
        .map(|path| {
            // Resolve the exact path from user, product, then embedded precedence.
            // 按用户、产品、内置优先级解析精确路径。
            let bytes = user
                .files
                .get(&path)
                .or_else(|| product.files.get(&path))
                .cloned()
                .map_or_else(|| embedded_bytes(&path), Ok)?;
            Ok((path, bytes))
        })
        .collect()
}

/// Returns exact embedded bytes for one validated resource path.
/// 返回一个已校验资源路径的精确嵌入字节。
fn embedded_bytes(path: &str) -> Result<Vec<u8>, CoreError> {
    EMBEDDED_RESOURCES
        .get_file(path)
        .map(|file| file.contents().to_vec())
        .ok_or_else(|| {
            config_error(
                ErrorCode::MissingResource,
                format!("embedded resource is missing: {path}"),
            )
        })
}

/// Normalizes only aliases explicitly declared by the resource manifest.
/// 仅规范化资源清单显式声明的别名。
fn normalize_prompt_bundle(value: &str, defaults: &ManifestDefaults) -> String {
    // Normalize case for documented aliases while leaving custom exact names unchanged.
    // 为已记录别名规范化大小写，同时保持自定义精确名称不变。
    let normalized = value.to_ascii_lowercase();
    if defaults.prompt_aliases_default_en.contains(&normalized) {
        "default_en".to_owned()
    } else if defaults.prompt_aliases_default_cn.contains(&normalized) {
        "default_cn".to_owned()
    } else {
        value.to_owned()
    }
}

/// Calculates a stable snapshot digest over resolved configuration semantics.
/// 对已解析配置语义计算稳定快照摘要。
fn snapshot_digest(
    bundle: &ConfigBundle,
    prompt_bundle: &str,
    prompts: &BTreeMap<String, Arc<str>>,
    pii_files: &[(String, Vec<u8>)],
    noise_files: &[(String, Vec<u8>)],
    identity: &ResourceIdentity,
) -> Result<ConfigSnapshotId, CoreError> {
    // Hash selected identities before resolved bytes to make field ownership explicit.
    // 在已解析字节前哈希选定身份，使字段归属显式化。
    let mut hasher = Sha256::new();
    for value in [
        bundle.name.as_str(),
        prompt_bundle,
        bundle.pii_language.as_str(),
        bundle.noise_language.as_str(),
        identity.digest.as_str(),
        bundle.vector_space_fingerprint.as_str(),
    ] {
        hash_length_prefixed(&mut hasher, value.as_bytes());
    }
    hasher.update(bundle.memory_policy.schema_version.to_le_bytes());
    hasher.update(bundle.embedding_dimension.to_le_bytes());
    hasher.update([
        u8::from(bundle.noise_policy.enabled),
        u8::from(bundle.noise_policy.semantic_enabled),
    ]);
    hasher.update(
        bundle
            .noise_policy
            .default_semantic_threshold
            .to_bits()
            .to_le_bytes(),
    );
    // Bind the complete retrieval strategy into the snapshot identity so any ranking
    // decision input change produces a different immutable snapshot id.
    // 将完整检索策略绑定进快照身份，使任一排序决策输入变化都产生不同的不可变快照 id。
    hash_length_prefixed(
        &mut hasher,
        digest_memory_policy(&bundle.memory_policy).as_bytes(),
    );
    hash_collection_len(&mut hasher, prompts.len());
    for (scene, text) in prompts {
        hash_length_prefixed(&mut hasher, scene.as_bytes());
        hash_length_prefixed(&mut hasher, text.as_bytes());
    }
    for files in [pii_files, noise_files] {
        hash_collection_len(&mut hasher, files.len());
        for (path, bytes) in files {
            hash_length_prefixed(&mut hasher, path.as_bytes());
            hash_length_prefixed(&mut hasher, bytes);
        }
    }
    ConfigSnapshotId::new(format!("sha256:{:x}", hasher.finalize())).map_err(|error| {
        config_error(
            ErrorCode::Internal,
            format!("snapshot identifier generation failed: {error}"),
        )
    })
}

/// Validates one closed memory policy before it can influence a snapshot.
/// 在记忆策略影响快照前校验其封闭契约。
#[allow(clippy::too_many_lines)]
fn validate_memory_policy(policy: &MemoryPolicy) -> Result<(), CoreError> {
    if policy.schema_version != MEMORY_POLICY_SCHEMA_VERSION {
        return Err(config_error(
            ErrorCode::InvalidConfiguration,
            "memory policy schema version is unsupported",
        ));
    }
    // Review admission consumes the final per-query score and has a separate fan-out bound.
    // 审核准入消费每查询最终分，并具有独立扇出上限，不能复用物理通道阈值。
    let precheck = &policy.precheck_policy;
    if !(1..=policy.retrieval_policy.top_k_max).contains(&precheck.recall_top_k)
        || precheck.review_candidate_limit == 0
        || precheck.review_candidate_limit
            > vulcan_memorymesh_core_contracts::MAX_PRECHECK_REVIEW_CANDIDATES
        || !precheck.min_review_score.is_finite()
        || !(0.0..=1.0).contains(&precheck.min_review_score)
    {
        return Err(config_error(
            ErrorCode::InvalidConfiguration,
            "PreCheck policy limits or reviewer score are invalid",
        ));
    }
    if !(1..=64).contains(&policy.postaction_history_turn_limit) {
        return Err(config_error(
            ErrorCode::InvalidConfiguration,
            "PostAction history turn limit must be between 1 and 64",
        ));
    }
    if !(1..=131_072).contains(&policy.postaction_max_input_tokens) {
        return Err(config_error(
            ErrorCode::InvalidConfiguration,
            "PostAction input token budget must be between 1 and 131072",
        ));
    }
    if policy.postaction_memory_priority > 100 {
        return Err(config_error(
            ErrorCode::InvalidConfiguration,
            "PostAction memory priority must not exceed 100",
        ));
    }
    if policy.postaction_memory_refresh_weight == 0
        || policy.postaction_memory_refresh_weight > 1_000_000
    {
        return Err(config_error(
            ErrorCode::InvalidConfiguration,
            "PostAction memory refresh weight must be between one and one million",
        ));
    }
    if policy.postaction_memory_expiry_ms == 0
        || policy.postaction_memory_expiry_ms > 10 * 365 * 24 * 60 * 60 * 1_000
    {
        return Err(config_error(
            ErrorCode::InvalidConfiguration,
            "PostAction memory expiry must be between one millisecond and ten years",
        ));
    }
    for (name, value) in [
        (
            "direct session memory expiry",
            policy.direct_memory_session_expiry_ms,
        ),
        (
            "direct environment memory expiry",
            policy.direct_memory_environment_expiry_ms,
        ),
        (
            "direct subject memory expiry",
            policy.direct_memory_subject_expiry_ms,
        ),
    ] {
        if value == 0 || value > 10 * 365 * 24 * 60 * 60 * 1_000 {
            return Err(config_error(
                ErrorCode::InvalidConfiguration,
                format!("{name} must be between one millisecond and ten years"),
            ));
        }
    }
    let retrieval = &policy.retrieval_policy;
    if retrieval.schema_version != 8 {
        return Err(config_error(
            ErrorCode::InvalidConfiguration,
            "retrieval policy schema version is unsupported",
        ));
    }
    if retrieval.rrf_k == 0 || retrieval.rrf_k > i64::MAX.unsigned_abs() {
        return Err(config_error(
            ErrorCode::InvalidConfiguration,
            "RRF constant must be within the positive signed-64 source range",
        ));
    }
    if retrieval.candidate_pool_max == 0 || retrieval.top_k_max == 0 {
        return Err(config_error(
            ErrorCode::InvalidConfiguration,
            "retrieval policy requires positive fusion and window parameters",
        ));
    }
    // Keep the public and internal retrieval windows inside the authoritative VMM bound.
    // 将公共与内部检索窗口限制在权威 VMM 上限内。
    if retrieval.top_k_max > 32 || retrieval.candidate_pool_max > 32 {
        return Err(config_error(
            ErrorCode::InvalidConfiguration,
            "retrieval public and candidate windows exceed the authoritative bound",
        ));
    }
    if retrieval.top_k_default == 0 || retrieval.top_k_default > retrieval.top_k_max {
        return Err(config_error(
            ErrorCode::InvalidConfiguration,
            "retrieval policy top-k default exceeds maximum",
        ));
    }
    if retrieval.candidate_pool_max < retrieval.top_k_max
        || retrieval.lexical_candidate_floor == 0
        || retrieval.lexical_candidate_floor > retrieval.candidate_pool_max
        || retrieval.replacement_top_k == 0
        || retrieval.replacement_top_k > retrieval.top_k_max
        || retrieval.hard_dedupe_pool_top_k == 0
        || retrieval.hard_dedupe_pool_top_k > retrieval.candidate_pool_max
        || retrieval.rerank_top_n > retrieval.candidate_pool_max
    {
        return Err(config_error(
            ErrorCode::InvalidConfiguration,
            "retrieval windows must contain positive lexical, replacement, and hard-dedupe limits",
        ));
    }
    if retrieval.max_query_items == 0 || retrieval.max_query_items > 64 {
        return Err(config_error(
            ErrorCode::InvalidConfiguration,
            "retrieval policy query batch bound is invalid",
        ));
    }
    if retrieval.vector_contract.contract.id.trim().is_empty()
        || retrieval.vector_contract.contract.id.len() > 128
        || retrieval.vector_contract.contract.version == 0
        || retrieval.lexical_contract.contract.id.trim().is_empty()
        || retrieval.lexical_contract.contract.id.len() > 128
        || retrieval.lexical_contract.contract.version == 0
    {
        return Err(config_error(
            ErrorCode::InvalidConfiguration,
            "retrieval policy requires versioned vector and lexical score contracts",
        ));
    }
    if retrieval.vector_contract.contract == retrieval.lexical_contract.contract {
        return Err(config_error(
            ErrorCode::InvalidConfiguration,
            "vector and lexical channels require distinct score contract identities",
        ));
    }
    if let Some(fallback) = &retrieval.separate_channel_fallback {
        if retrieval.candidate_read_mode
            != vulcan_memorymesh_core_contracts::CandidateReadMode::SingleObservation
            || !retrieval.hybrid_enabled
            || retrieval.rrf_score_mode
                != vulcan_memorymesh_core_contracts::RrfScoreMode::FusedRankOnly
        {
            return Err(config_error(
                ErrorCode::InvalidConfiguration,
                "separate fallback requires enabled single-observation rank fusion",
            ));
        }
        if fallback.vector_contract.contract.id.trim().is_empty()
            || fallback.vector_contract.contract.id.len() > 128
            || fallback.vector_contract.contract.version == 0
            || fallback.lexical_contract.contract.id.trim().is_empty()
            || fallback.lexical_contract.contract.id.len() > 128
            || fallback.lexical_contract.contract.version == 0
            || fallback.vector_contract.contract == fallback.lexical_contract.contract
            || fallback.vector_contract.decision_score_mode
                != vulcan_memorymesh_core_contracts::DecisionScoreMode::HostProjected
            || fallback.lexical_contract.decision_score_mode
                != vulcan_memorymesh_core_contracts::DecisionScoreMode::HostProjected
        {
            return Err(config_error(
                ErrorCode::InvalidConfiguration,
                "separate fallback requires distinct versioned host-projected producers",
            ));
        }
        if !fallback.dual_channel_bonus.is_finite()
            || !(0.0..=1.0).contains(&fallback.dual_channel_bonus)
        {
            return Err(config_error(
                ErrorCode::InvalidConfiguration,
                "separate fallback dual-channel bonus is invalid",
            ));
        }
    }
    // Only enabled rank-fusion paths may consume producers without a host projection.
    // 仅实际启用的排名融合路径能消费无宿主投影的生产者；未启用词法通道不阻止合法纯向量配置。
    let rank_fusion = retrieval.hybrid_enabled
        && retrieval.rrf_score_mode
            == vulcan_memorymesh_core_contracts::RrfScoreMode::FusedRankOnly;
    for (descriptor, enabled) in [
        (&retrieval.vector_contract, true),
        (&retrieval.lexical_contract, retrieval.hybrid_enabled),
    ] {
        if descriptor.decision_score_mode
            == vulcan_memorymesh_core_contracts::DecisionScoreMode::RankOnly
            && ((enabled && !rank_fusion)
                || retrieval
                    .min_similarity
                    .iter()
                    .any(|binding| binding.contract == descriptor.contract))
        {
            return Err(config_error(
                ErrorCode::InvalidConfiguration,
                "rank-only scores cannot satisfy this retrieval mode or a physical decision threshold",
            ));
        }
    }
    if !retrieval.dual_channel_bonus.is_finite()
        || !(0.0..=1.0).contains(&retrieval.dual_channel_bonus)
    {
        return Err(config_error(
            ErrorCode::InvalidConfiguration,
            "dual-channel bonus must be finite and between zero and one",
        ));
    }
    for bindings in [
        retrieval.min_similarity.as_slice(),
        retrieval.replace_min_similarity.as_slice(),
        retrieval.hard_dedupe.as_slice(),
    ] {
        let mut identities = BTreeSet::new();
        for binding in bindings {
            if binding.contract.id.trim().is_empty()
                || binding.contract.id.len() > 128
                || binding.contract.version == 0
            {
                return Err(config_error(
                    ErrorCode::InvalidConfiguration,
                    "retrieval threshold contract identity is invalid",
                ));
            }
            if !binding.threshold.is_finite() || !(0.0..=1.0).contains(&binding.threshold) {
                return Err(config_error(
                    ErrorCode::InvalidConfiguration,
                    "retrieval threshold must be finite and between zero and one",
                ));
            }
            let fallback_contract =
                retrieval
                    .separate_channel_fallback
                    .as_ref()
                    .is_some_and(|fallback| {
                        binding.contract == fallback.vector_contract.contract
                            || binding.contract == fallback.lexical_contract.contract
                    });
            if binding.contract != retrieval.vector_contract.contract
                && binding.contract != retrieval.lexical_contract.contract
                && !fallback_contract
            {
                return Err(config_error(
                    ErrorCode::InvalidConfiguration,
                    "retrieval threshold references a score contract not selected by this snapshot",
                ));
            }
            if !identities.insert((binding.contract.id.clone(), binding.contract.version)) {
                return Err(config_error(
                    ErrorCode::InvalidConfiguration,
                    "retrieval threshold set contains a duplicate score contract identity",
                ));
            }
        }
    }
    if retrieval.decay.level_scales.len() != 4
        || retrieval
            .decay
            .level_scales
            .iter()
            .any(|value| !value.is_finite() || *value <= 0.0)
    {
        return Err(config_error(
            ErrorCode::InvalidConfiguration,
            "retrieval decay requires four positive level scales",
        ));
    }
    if retrieval.decay.priority_scales.len() != 3
        || retrieval
            .decay
            .priority_scales
            .iter()
            .any(|value| !value.is_finite() || *value <= 0.0)
    {
        return Err(config_error(
            ErrorCode::InvalidConfiguration,
            "retrieval decay requires three positive priority scales",
        ));
    }
    if retrieval.decay.scope_scales.len() != 3
        || retrieval
            .decay
            .scope_scales
            .iter()
            .any(|value| !value.is_finite() || *value <= 0.0)
    {
        return Err(config_error(
            ErrorCode::InvalidConfiguration,
            "retrieval decay requires three positive scope scales",
        ));
    }
    if !retrieval.decay.shape.is_finite()
        || retrieval.decay.shape <= 0.0
        || !retrieval.decay.scale_hours.is_finite()
        || retrieval.decay.scale_hours <= 0.0
        || !retrieval.decay.min_multiplier.is_finite()
        || !(0.0..=1.0).contains(&retrieval.decay.min_multiplier)
    {
        return Err(config_error(
            ErrorCode::InvalidConfiguration,
            "retrieval decay requires a positive Weibull shape and scale with a valid floor",
        ));
    }
    if !retrieval.mmr.lambda.is_finite() || !(0.0..=1.0).contains(&retrieval.mmr.lambda) {
        return Err(config_error(
            ErrorCode::InvalidConfiguration,
            "MMR lambda must be finite and between zero and one",
        ));
    }
    if retrieval.rerank_top_n > 256 {
        return Err(config_error(
            ErrorCode::InvalidConfiguration,
            "retrieval rerank window exceeds the supported bound",
        ));
    }
    // Multipliers and context weights retain exact finite values; negative or nonfinite inputs are not valid policies.
    // 倍率与情境权重保留精确有限数值；负数或非有限输入不属于合法策略。
    for value in [
        retrieval.decay.reinforce_weight,
        retrieval.decay.cross_session_boost,
        retrieval.decay.refresh_weight_factor,
        retrieval.context.support_step,
        retrieval.context.support_cap,
        retrieval.context.rebuttal_step,
        retrieval.context.rebuttal_cap,
    ] {
        if !value.is_finite() || value < 0.0 {
            return Err(config_error(
                ErrorCode::InvalidConfiguration,
                "retrieval weights must be finite and nonnegative",
            ));
        }
    }
    Ok(())
}

/// Calculates a stable digest over resolved prompt identities and text.
/// 对已解析 Prompt 身份与文本计算稳定摘要。
fn digest_text_map(values: &BTreeMap<String, Arc<str>>) -> String {
    let mut hasher = Sha256::new();
    hash_collection_len(&mut hasher, values.len());
    for (name, value) in values {
        hash_length_prefixed(&mut hasher, name.as_bytes());
        hash_length_prefixed(&mut hasher, value.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

/// Calculates a stable digest over ordered named byte resources.
/// 对有序命名字节资源计算稳定摘要。
fn digest_named_bytes(values: &[(String, Vec<u8>)]) -> String {
    let mut hasher = Sha256::new();
    hash_collection_len(&mut hasher, values.len());
    for (name, value) in values {
        hash_length_prefixed(&mut hasher, name.as_bytes());
        hash_length_prefixed(&mut hasher, value);
    }
    format!("{:x}", hasher.finalize())
}

/// Hashes one complete score producer descriptor in its declared field order.
/// 按声明字段顺序哈希一份完整评分生产者描述符；不返回独立摘要。
fn hash_score_contract_descriptor(hasher: &mut Sha256, descriptor: &ScoreContractDescriptor) {
    hash_length_prefixed(hasher, descriptor.contract.id.as_bytes());
    hasher.update(descriptor.contract.version.to_le_bytes());
    hasher.update((descriptor.raw_score_kind as u32).to_le_bytes());
    hasher.update((descriptor.direction as u32).to_le_bytes());
    hasher.update((descriptor.decision_score_mode as u32).to_le_bytes());
}

/// Calculates a stable digest over the typed memory policy.
/// 对强类型记忆策略计算稳定摘要；返回小写十六进制 SHA-256。
fn digest_memory_policy(policy: &MemoryPolicy) -> String {
    let mut hasher = Sha256::new();
    hasher.update(policy.schema_version.to_le_bytes());
    hasher.update(policy.direct_memory_session_expiry_ms.to_le_bytes());
    hasher.update(policy.direct_memory_environment_expiry_ms.to_le_bytes());
    hasher.update(policy.direct_memory_subject_expiry_ms.to_le_bytes());
    hasher.update(policy.postaction_history_turn_limit.to_le_bytes());
    hasher.update(policy.postaction_max_input_tokens.to_le_bytes());
    hasher.update(policy.postaction_memory_priority.to_le_bytes());
    hasher.update((policy.postaction_memory_stability as u32).to_le_bytes());
    hasher.update((policy.postaction_memory_level as u32).to_le_bytes());
    hasher.update(policy.postaction_memory_refresh_weight.to_le_bytes());
    hasher.update(policy.postaction_memory_expiry_ms.to_le_bytes());
    hasher.update(policy.precheck_policy.recall_top_k.to_le_bytes());
    hasher.update(policy.precheck_policy.review_candidate_limit.to_le_bytes());
    hasher.update(
        policy
            .precheck_policy
            .min_review_score
            .to_bits()
            .to_le_bytes(),
    );
    // Hash every closed retrieval strategy decision input in declared field order.
    // 按声明字段顺序哈希每个封闭检索策略决策输入。
    let retrieval = &policy.retrieval_policy;
    hasher.update(retrieval.schema_version.to_le_bytes());
    hasher.update(retrieval.candidate_read_mode.as_u32().to_le_bytes());
    hasher.update([u8::from(retrieval.hybrid_enabled)]);
    hasher.update(retrieval.rrf_k.to_le_bytes());
    hasher.update(retrieval.rrf_math_profile.as_u32().to_le_bytes());
    hasher.update(retrieval.rrf_score_mode.as_u32().to_le_bytes());
    hasher.update(retrieval.dual_channel_bonus.to_le_bytes());
    for bindings in [
        retrieval.min_similarity.as_slice(),
        retrieval.replace_min_similarity.as_slice(),
        retrieval.hard_dedupe.as_slice(),
    ] {
        hash_collection_len(&mut hasher, bindings.len());
        for binding in bindings {
            hash_length_prefixed(&mut hasher, binding.contract.id.as_bytes());
            hasher.update(binding.contract.version.to_le_bytes());
            hasher.update(binding.threshold.to_le_bytes());
        }
    }
    hasher.update(retrieval.hard_dedupe_pool_top_k.to_le_bytes());
    hasher.update(retrieval.top_k_default.to_le_bytes());
    hasher.update(retrieval.top_k_max.to_le_bytes());
    hasher.update(retrieval.max_query_items.to_le_bytes());
    hasher.update(retrieval.candidate_pool_max.to_le_bytes());
    hasher.update(retrieval.lexical_candidate_floor.to_le_bytes());
    hasher.update(retrieval.replacement_top_k.to_le_bytes());
    // Bind the exact producer identities and their declared raw-score semantics. A backend
    // switch must therefore produce a different immutable snapshot even when all thresholds
    // and ranking parameters remain unchanged.
    // 绑定精确生产者身份及其声明的原始评分语义。因此，即使所有阈值和排序参数不变，
    // 后端切换也必须产生不同的不可变快照。
    for descriptor in [&retrieval.vector_contract, &retrieval.lexical_contract] {
        hash_score_contract_descriptor(&mut hasher, descriptor);
    }
    match &retrieval.separate_channel_fallback {
        None => hasher.update([0]),
        Some(fallback) => {
            hasher.update([1]);
            for descriptor in [&fallback.vector_contract, &fallback.lexical_contract] {
                hash_score_contract_descriptor(&mut hasher, descriptor);
            }
            hasher.update(fallback.rrf_math_profile.as_u32().to_le_bytes());
            hasher.update(fallback.dual_channel_bonus.to_le_bytes());
        }
    }
    hasher.update([u8::from(retrieval.decay.enabled)]);
    for value in [
        retrieval.decay.shape,
        retrieval.decay.min_multiplier,
        retrieval.decay.reinforce_weight,
        retrieval.decay.cross_session_boost,
        retrieval.decay.refresh_weight_factor,
    ] {
        hasher.update(value.to_le_bytes());
    }
    hasher.update(retrieval.decay.scale_hours.to_le_bytes());
    hasher.update(u32::from(retrieval.decay.math_profile).to_le_bytes());
    for values in [
        retrieval.decay.level_scales.as_slice(),
        retrieval.decay.priority_scales.as_slice(),
        retrieval.decay.scope_scales.as_slice(),
    ] {
        hash_collection_len(&mut hasher, values.len());
        for value in values {
            hasher.update(value.to_le_bytes());
        }
    }
    hasher.update([u8::from(retrieval.context.enabled)]);
    for value in [
        retrieval.context.support_step,
        retrieval.context.support_cap,
        retrieval.context.rebuttal_step,
        retrieval.context.rebuttal_cap,
    ] {
        hasher.update(value.to_le_bytes());
    }
    hasher.update([u8::from(retrieval.mmr.enabled)]);
    hasher.update(retrieval.mmr.lambda.to_le_bytes());
    hasher.update(retrieval.rerank_top_n.to_le_bytes());
    format!("{:x}", hasher.finalize())
}

/// Writes one collection length into digest material without platform-width ambiguity.
/// 将一个集合长度写入摘要材料且不引入平台位宽歧义。
fn hash_collection_len(hasher: &mut Sha256, length: usize) {
    hasher.update(u64::try_from(length).unwrap_or(u64::MAX).to_le_bytes());
}

/// Writes one byte sequence with an explicit length prefix into digest material.
/// 将一个带显式长度前缀的字节序列写入摘要材料。
fn hash_length_prefixed(hasher: &mut Sha256, value: &[u8]) {
    hash_collection_len(hasher, value.len());
    hasher.update(value);
}

/// Validates one resource path against the closed Core resource namespace.
/// 校验一个资源路径是否属于封闭的 Core 资源命名空间。
fn validate_resource_path(path: &str) -> Result<(), CoreError> {
    if path.is_empty()
        || path.len() > MAX_RESOURCE_PATH_BYTES
        || path.contains('\0')
        || path.starts_with('/')
        || path.contains('\\')
        || path
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
        || !["prompts/", "pii_rules/", "noise_rules/"]
            .iter()
            .any(|root| path.starts_with(root))
    {
        return Err(config_error(
            ErrorCode::InvalidConfiguration,
            format!("invalid resource path: {path}"),
        ));
    }
    Ok(())
}

/// Validates one non-empty bounded configuration identifier without silent normalization.
/// 校验单个非空且有界的配置标识，并禁止静默规范化。
fn validate_config_identifier(label: &str, value: &str) -> Result<(), CoreError> {
    if value.is_empty()
        || value.trim() != value
        || value.len() > MAX_CONFIG_IDENTIFIER_BYTES
        || value.contains('\0')
    {
        return Err(config_error(
            ErrorCode::InvalidConfiguration,
            format!(
                "{label} must be non-empty, trimmed, NUL-free, and at most {MAX_CONFIG_IDENTIFIER_BYTES} UTF-8 bytes"
            ),
        ));
    }
    Ok(())
}

/// Validates one resource entry independently of how its public layer was constructed.
/// 独立校验单个资源条目，不依赖其公开层级采用何种方式构造。
fn validate_resource_entry(path: &str, bytes: &[u8]) -> Result<(), CoreError> {
    validate_resource_path(path)?;
    if bytes.is_empty() || bytes.len() > MAX_RESOURCE_FILE_BYTES {
        return Err(config_error(
            ErrorCode::InvalidConfiguration,
            format!("resource file must contain 1..={MAX_RESOURCE_FILE_BYTES} bytes: {path}"),
        ));
    }
    Ok(())
}

/// Validates one complete public override layer and all aggregate resource bounds.
/// 校验一个完整公开覆盖层及其全部聚合资源边界。
fn validate_resource_layer(label: &str, layer: &ResourceLayer) -> Result<(), CoreError> {
    if layer.files.len() > MAX_RESOURCE_LAYER_FILES {
        return Err(config_error(
            ErrorCode::InvalidConfiguration,
            format!("{label} resource layer exceeds {MAX_RESOURCE_LAYER_FILES} files"),
        ));
    }
    let mut total_bytes = 0_usize;
    for (path, bytes) in &layer.files {
        validate_resource_entry(path, bytes)?;
        total_bytes = total_bytes.checked_add(bytes.len()).ok_or_else(|| {
            config_error(
                ErrorCode::InvalidConfiguration,
                format!("{label} resource layer byte total overflowed"),
            )
        })?;
        if total_bytes > MAX_RESOURCE_LAYER_BYTES {
            return Err(config_error(
                ErrorCode::InvalidConfiguration,
                format!(
                    "{label} resource layer exceeds {MAX_RESOURCE_LAYER_BYTES} aggregate bytes"
                ),
            ));
        }
    }
    Ok(())
}

/// Calculates lowercase SHA-256 for exact bytes.
/// 为精确字节计算小写 SHA-256。
fn hex_digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Creates one configuration-stage error.
/// 创建一个配置阶段错误。
fn config_error(code: ErrorCode, diagnostic: impl Into<String>) -> CoreError {
    CoreError::new(code, ErrorStage::Configuration, diagnostic)
}
