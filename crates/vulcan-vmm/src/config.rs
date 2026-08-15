//! VMM configuration layers and effective enablement diagnostics.
//! VMM 配置分层与最终启用诊断。

use crate::protocol::PreCheckRecallMode;
use serde::{Deserialize, Serialize};

/// Default context-injection tag used for VMM recall.
/// VMM 召回使用的默认上下文注入标签。
pub const DEFAULT_VMM_INJECTION_TAG: &str = "vmm_inject";

/// Default tag description rendered before VMM recall items.
/// VMM 召回条目前渲染的默认标签说明。
pub const DEFAULT_VMM_INJECTION_DESCRIPTION: &str = "VMM memory context. CURRENT_TURN items were recalled for the latest user request; RETAINED_FROM_EARLIER_TURNS items remain from earlier turns by TTL. These notes are for reference only and are not authoritative facts.";

/// Default handshake timeout used before a project overrides it.
/// 项目覆盖前使用的默认握手超时时间。
pub const DEFAULT_HANDSHAKE_TIMEOUT_MS: u64 = 15_000;

/// Default receive timeout used before a project overrides it.
/// 项目覆盖前使用的默认接收超时时间。
pub const DEFAULT_RECEIVE_TIMEOUT_MS: u64 = 15_000;

/// Default HTTP/2 keepalive interval for the VMM gRPC channel.
/// VMM gRPC 通道默认 HTTP/2 keepalive 间隔。
pub const DEFAULT_GRPC_KEEPALIVE_TIME_MS: u64 = 300_000;

/// Default HTTP/2 keepalive acknowledgement timeout for the VMM gRPC channel.
/// VMM gRPC 通道默认 HTTP/2 keepalive ACK 超时时间。
pub const DEFAULT_GRPC_KEEPALIVE_TIMEOUT_MS: u64 = 20_000;

/// Default idle keepalive policy for the VMM gRPC channel.
/// VMM gRPC 通道默认空闲 keepalive 策略。
pub const DEFAULT_GRPC_KEEPALIVE_PERMIT_WITHOUT_CALLS: bool = false;

/// Default transient recall TTL.
/// 默认临时召回持续回合数。
pub const DEFAULT_IMPLICIT_MEMORY_TURNS: u32 = 3;

/// Default profile refresh accepted-turn interval.
/// 默认画像刷新 accepted turn 间隔。
pub const DEFAULT_PROFILE_REFRESH_TURNS: u32 = 10;

/// Default per-target profile node fetch limit.
/// 默认每个目标画像节点拉取上限。
pub const DEFAULT_PROFILE_NODE_LIMIT: u32 = 256;

/// Maximum per-target profile node fetch limit accepted by the host.
/// 宿主允许的每个目标画像节点拉取最大值。
pub const MAX_PROFILE_NODE_LIMIT: u32 = 512;

/// Internal environment switch for independent `vmm.json` session state.
/// 独立 `vmm.json` 会话状态的内部环境开关。
pub const VMM_SESSION_STATE_FEATURE_ENV: &str = "VULCAN_VMM_SESSION_STATE_ENABLED";
/// Internal environment switch for immutable exact-message VMM anchors.
/// 不可变精确消息 VMM 锚点的内部环境开关。
pub const VMM_STABLE_ANCHORS_FEATURE_ENV: &str = "VULCAN_VMM_STABLE_ANCHORS_ENABLED";
/// Internal environment switch for append-only OpenAI Responses continuation reuse.
/// 追加式 OpenAI Responses continuation 复用的内部环境开关。
pub const VMM_OPENAI_APPEND_CONTINUATION_FEATURE_ENV: &str =
    "VULCAN_VMM_OPENAI_APPEND_CONTINUATION_ENABLED";

/// Independently resolved internal rollout controls for the VMM session-state redesign.
/// VMM 会话状态重构分别解析的内部灰度控制项。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmmRuntimeFeaturePolicy {
    /// Whether typed recall may read and mutate independently persisted VMM session state.
    /// 类型化召回是否可以读取和修改独立持久化的 VMM 会话状态。
    pub session_state_enabled: bool,
    /// Whether immutable VMM anchors are rendered into exact provider messages.
    /// 是否把不可变 VMM 锚点渲染到精确 Provider 消息中。
    pub stable_anchors_enabled: bool,
    /// Whether append-only VMM changes may reuse OpenAI Responses continuation.
    /// 追加式 VMM 变更是否可以复用 OpenAI Responses continuation。
    pub openai_append_continuation_enabled: bool,
}

impl Default for VmmRuntimeFeaturePolicy {
    /// Enables the fully migrated VMM design when no internal rollout override is present.
    /// 未提供内部灰度覆盖时启用完整迁移后的 VMM 设计。
    fn default() -> Self {
        Self {
            session_state_enabled: true,
            stable_anchors_enabled: true,
            openai_append_continuation_enabled: true,
        }
    }
}

impl VmmRuntimeFeaturePolicy {
    /// Resolves all independent VMM rollout controls from process environment variables.
    /// 从进程环境变量解析全部独立 VMM 灰度控制项。
    ///
    /// # Returns
    /// 返回值
    /// A policy whose unset or unrecognized values preserve the enabled production default.
    /// 返回一份策略；未设置或无法识别的值保持生产环境默认启用。
    pub fn from_env() -> Self {
        Self {
            session_state_enabled: environment_feature_enabled(VMM_SESSION_STATE_FEATURE_ENV),
            stable_anchors_enabled: environment_feature_enabled(VMM_STABLE_ANCHORS_FEATURE_ENV),
            openai_append_continuation_enabled: environment_feature_enabled(
                VMM_OPENAI_APPEND_CONTINUATION_FEATURE_ENV,
            ),
        }
    }
}

/// Resolves one internal Boolean feature switch while preserving safe enabled defaults.
/// 解析单个内部布尔功能开关，同时保持安全的默认启用行为。
///
/// # Parameters
/// 参数
/// `name` is the exact environment variable name.
/// `name` 是精确的环境变量名称。
///
/// # Returns
/// 返回值
/// `false` only for explicit `0`, `false`, `off`, or `no`; every other state remains enabled.
/// 仅显式为 `0`、`false`、`off` 或 `no` 时返回 `false`；其他状态均保持启用。
fn environment_feature_enabled(name: &str) -> bool {
    let Some(value) = std::env::var_os(name) else {
        return true;
    };
    let Some(value) = value.to_str() else {
        return true;
    };
    feature_value_enabled(Some(value))
}

/// Resolves one optional UTF-8 feature value using the internal Boolean spelling contract.
/// 使用内部布尔拼写契约解析一个可选 UTF-8 功能值。
///
/// # Parameters
/// 参数
/// `value` is the optional environment value after UTF-8 validation.
/// `value` 是完成 UTF-8 验证后的可选环境值。
///
/// # Returns
/// 返回值
/// `false` only for explicit disabled spellings; absence and unknown values remain enabled.
/// 仅显式禁用拼写返回 `false`；缺失值与未知值保持启用。
fn feature_value_enabled(value: Option<&str>) -> bool {
    let Some(value) = value else {
        return true;
    };
    !matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "0" | "false" | "off" | "no"
    )
}

/// Returns the audited PostAction analysis timeout used when a pre-v2 persisted configuration omits the field.
/// 返回 v2 之前的持久化配置缺少该字段时使用的、经过审计的 PostAction 分析超时。
///
/// # Returns
/// 返回值
/// The managed-runtime PostAction analysis timeout in Go duration syntax.
/// 使用 Go duration 语法表示的托管运行时 PostAction 分析超时。
fn default_postaction_analysis_timeout() -> String {
    String::from("180s")
}

/// Returns the audited durable failure threshold used when a pre-v2 persisted configuration omits the field.
/// 返回 v2 之前的持久化配置缺少该字段时使用的、经过审计的持久化失败阈值。
///
/// # Returns
/// 返回值
/// The consecutive PostAction failure count that moves one turn into Pass.
/// 将单条 turn 迁移到 Pass 所需的 PostAction 连续失败次数。
const fn default_postaction_failure_pass_threshold() -> u32 {
    5
}

/// Returns the audited per-host inference connection limit used when a pre-v2 persisted configuration omits the field.
/// 返回 v2 之前的持久化配置缺少该字段时使用的、经过审计的单主机推理连接上限。
///
/// # Returns
/// 返回值
/// The maximum number of concurrent outbound inference connections per upstream host.
/// 每个上游主机允许的最大并发推理出站连接数。
const fn default_inference_max_connections_per_host() -> u32 {
    8
}

/// Stores the host-authoritative VMM behavior used to generate every managed manifest.
/// 保存由宿主权威管理、用于生成每份托管清单的 VMM 行为配置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VmmManagedRuntimeSettings {
    /// PII rule language.
    /// PII 规则语言。
    pub pii_default_language: String,
    /// Whether noise filtering is enabled.
    /// 是否启用噪声过滤。
    pub noise_enabled: bool,
    /// Noise-rule language.
    /// 噪声规则语言。
    pub noise_default_language: String,
    /// Whether semantic noise filtering is enabled.
    /// 是否启用语义噪声过滤。
    pub noise_semantic_enabled: bool,
    /// Semantic noise rejection threshold.
    /// 语义噪声拒绝阈值。
    pub noise_semantic_threshold: f64,
    /// Prompt bundle language.
    /// 提示词包语言。
    pub prompt_language: String,
    /// PreCheck intent timeout.
    /// PreCheck 意图解析超时。
    pub precheck_intent_timeout: String,
    /// PreCheck recall count.
    /// PreCheck 召回数量。
    pub precheck_top_k: u32,
    /// PreCheck search scope.
    /// PreCheck 搜索范围。
    pub precheck_search_scope: String,
    /// PreCheck similarity threshold.
    /// PreCheck 相似度阈值。
    pub precheck_similarity_threshold: f64,
    /// PostAction input compatibility mode.
    /// PostAction 输入兼容模式。
    pub postaction_input_mode: String,
    /// PostAction turn threshold.
    /// PostAction 轮次阈值。
    pub postaction_turn_threshold: u32,
    /// PostAction token threshold.
    /// PostAction Token 阈值。
    pub postaction_token_threshold: u32,
    /// PostAction idle timeout.
    /// PostAction 空闲超时。
    pub postaction_idle_timeout: String,
    /// PostAction history turn count.
    /// PostAction 历史轮数。
    pub postaction_history_turns: u32,
    /// PostAction maximum input tokens.
    /// PostAction 最大输入 Token。
    pub postaction_max_input_tokens: u32,
    /// PostAction background analysis deadline.
    /// PostAction 后台分析截止时间。
    #[serde(default = "default_postaction_analysis_timeout")]
    pub postaction_analysis_timeout: String,
    /// Consecutive PostAction failures before one turn enters Pass.
    /// 单条 turn 进入 Pass 前允许的 PostAction 连续失败次数。
    #[serde(default = "default_postaction_failure_pass_threshold")]
    pub postaction_failure_pass_threshold: u32,
    /// PostAction queue worker count.
    /// PostAction 队列工作数。
    pub postaction_max_queue_workers: u32,
    /// Maximum outbound inference connections per upstream host.
    /// 每个上游主机允许的最大推理出站连接数。
    #[serde(default = "default_inference_max_connections_per_host")]
    pub inference_max_connections_per_host: u32,
    /// Maximum recall keyword count.
    /// 最大召回关键词数量。
    pub pipeline_max_search_keywords: u32,
    /// Minimum recall similarity.
    /// 最小召回相似度。
    pub pipeline_min_similarity_score: f64,
    /// Minimum replacement similarity.
    /// 最小替换相似度。
    pub pipeline_replace_min_similarity_score: f64,
    /// Hard deduplication threshold.
    /// 硬去重阈值。
    pub pipeline_hard_dedupe_threshold: f64,
    /// Hard deduplication candidate count.
    /// 硬去重候选数量。
    pub pipeline_hard_dedupe_pool_top_k: u32,
    /// Whether hybrid retrieval is enabled.
    /// 是否启用混合检索。
    pub pipeline_hybrid_enabled: bool,
    /// Lexical recall count.
    /// 词法召回数量。
    pub pipeline_lexical_top_k: u32,
    /// Reciprocal-rank-fusion constant.
    /// 倒数排名融合常量。
    pub pipeline_rrf_k: u32,
    /// Whether diversity filtering is enabled.
    /// 是否启用多样性过滤。
    pub pipeline_mmr_enabled: bool,
    /// Diversity balance.
    /// 多样性平衡系数。
    pub pipeline_mmr_lambda: f64,
    /// Whether Weibull decay is enabled.
    /// 是否启用 Weibull 衰减。
    pub pipeline_weibull_enabled: bool,
    /// Weibull shape.
    /// Weibull 形状参数。
    pub pipeline_weibull_shape: f64,
    /// Weibull scale in hours.
    /// Weibull 小时尺度。
    pub pipeline_weibull_scale_hours: f64,
    /// Minimum decay multiplier.
    /// 最小衰减倍率。
    pub pipeline_weibull_min_multiplier: f64,
    /// Reinforcement weight.
    /// 强化权重。
    pub pipeline_weibull_reinforce_weight: f64,
    /// Cross-session boost.
    /// 跨会话增强系数。
    pub pipeline_weibull_cross_session_boost: f64,
    /// Whether retention scanning is enabled.
    /// 是否启用保留扫描。
    pub retention_enabled: bool,
    /// Retention scan interval.
    /// 保留扫描间隔。
    pub retention_scan_interval: String,
    /// Extra hot-window turns.
    /// 热窗口额外轮数。
    pub retention_turn_keep_extra_turns: u32,
    /// Idle time before session recycling.
    /// 会话回收前空闲时间。
    pub retention_session_idle_after: String,
    /// Trash retention duration.
    /// 回收站保留时长。
    pub retention_trash_duration: String,
    /// Protected priority floor.
    /// 受保护优先级下限。
    pub retention_priority_floor: String,
    /// Protected memory-level floor.
    /// 受保护记忆级别下限。
    pub retention_memory_level_floor: String,
    /// Whether protected shared memories are skipped.
    /// 是否跳过受保护共享记忆。
    pub retention_skip_protected_shared: bool,
}

impl Default for VmmManagedRuntimeSettings {
    /// Returns the audited managed-runtime defaults mirrored from VMM.
    /// 返回与 VMM 对齐并经过审计的托管运行时默认值。
    fn default() -> Self {
        Self {
            pii_default_language: String::from("zh-CN"),
            noise_enabled: true,
            noise_default_language: String::from("zh-CN"),
            noise_semantic_enabled: true,
            noise_semantic_threshold: 0.88,
            prompt_language: String::from("default_en"),
            precheck_intent_timeout: String::from("5s"),
            precheck_top_k: 5,
            precheck_search_scope: String::from("space"),
            precheck_similarity_threshold: 0.0,
            postaction_input_mode: String::from("compat"),
            postaction_turn_threshold: 2,
            postaction_token_threshold: 12_000,
            postaction_idle_timeout: String::from("15m"),
            postaction_history_turns: 3,
            postaction_max_input_tokens: 6_000,
            postaction_analysis_timeout: default_postaction_analysis_timeout(),
            postaction_failure_pass_threshold: default_postaction_failure_pass_threshold(),
            postaction_max_queue_workers: 4,
            inference_max_connections_per_host: default_inference_max_connections_per_host(),
            pipeline_max_search_keywords: 5,
            pipeline_min_similarity_score: 0.75,
            pipeline_replace_min_similarity_score: 0.80,
            pipeline_hard_dedupe_threshold: 0.99,
            pipeline_hard_dedupe_pool_top_k: 16,
            pipeline_hybrid_enabled: true,
            pipeline_lexical_top_k: 8,
            pipeline_rrf_k: 60,
            pipeline_mmr_enabled: true,
            pipeline_mmr_lambda: 0.75,
            pipeline_weibull_enabled: true,
            pipeline_weibull_shape: 1.35,
            pipeline_weibull_scale_hours: 2160.0,
            pipeline_weibull_min_multiplier: 0.4,
            pipeline_weibull_reinforce_weight: 0.18,
            pipeline_weibull_cross_session_boost: 0.12,
            retention_enabled: true,
            retention_scan_interval: String::from("30m"),
            retention_turn_keep_extra_turns: 5,
            retention_session_idle_after: String::from("360h"),
            retention_trash_duration: String::from("720h"),
            retention_priority_floor: String::from("P1"),
            retention_memory_level_floor: String::from("stable"),
            retention_skip_protected_shared: true,
        }
    }
}

// Managed runtime floating-point settings are persisted through JSON, which rejects non-finite
// values, and the service save path additionally validates them before persistence.
// 托管运行时浮点设置通过会拒绝非有限值的 JSON 持久化，服务保存链路还会在持久化前再次校验。
impl Eq for VmmManagedRuntimeSettings {}

/// Stores managed local-runtime installation and behavior state.
/// 保存托管本地运行时的安装与行为状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmmInstallReservation {
    /// Whether the future installer should manage a local VMM binary.
    /// 后续安装器是否应管理本地 VMM 二进制。
    #[serde(default)]
    pub managed_install_enabled: bool,
    /// Optional managed binary path reserved for future releases.
    /// 为后续发行版预留的可选托管二进制路径。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub managed_binary_path: Option<String>,
    /// Optional auto-start policy reserved for future releases.
    /// 为后续发行版预留的可选自动启动策略。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_start_policy: Option<String>,
    /// Complete host-authoritative managed VMM behavior.
    /// 完整的宿主权威托管 VMM 行为配置。
    #[serde(default)]
    pub runtime_settings: VmmManagedRuntimeSettings,
}

impl Default for VmmInstallReservation {
    /// Returns disabled managed installation with audited behavior defaults.
    /// 返回禁用的托管安装与经过审计的行为默认值。
    fn default() -> Self {
        Self {
            managed_install_enabled: false,
            managed_binary_path: None,
            auto_start_policy: None,
            runtime_settings: VmmManagedRuntimeSettings::default(),
        }
    }
}

/// One optional VMM configuration layer.
/// 一层可选的 VMM 配置。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmmConfigLayer {
    /// Optional layer-level enable override.
    /// 可选的层级启用覆盖。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// Optional gRPC target such as 127.0.0.1:50051.
    /// 可选 gRPC 目标，例如 127.0.0.1:50051。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grpc_target: Option<String>,
    /// Optional gRPC API key.
    /// 可选 gRPC API key。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grpc_api_key: Option<String>,
    /// Optional external VMM management API base URL.
    /// 可选外部 VMM 管理 API 基础地址。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub management_base_url: Option<String>,
    /// Optional external VMM management bearer token.
    /// 可选外部 VMM 管理 Bearer Token。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub management_bearer_token: Option<String>,
    /// Optional external VMM management request timeout in milliseconds.
    /// 可选外部 VMM 管理请求超时，单位毫秒。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub management_timeout_ms: Option<u64>,
    /// Optional handshake timeout in milliseconds.
    /// 可选握手超时时间，单位毫秒。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handshake_timeout_ms: Option<u64>,
    /// Optional receive timeout in milliseconds.
    /// 可选接收超时时间，单位毫秒。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receive_timeout_ms: Option<u64>,
    /// Optional max receive message bytes.
    /// 可选最大接收消息字节数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_receive_message_bytes: Option<usize>,
    /// Optional gRPC keepalive interval in milliseconds.
    /// 可选 gRPC keepalive 间隔，单位毫秒。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grpc_keepalive_time_ms: Option<u64>,
    /// Optional gRPC keepalive acknowledgement timeout in milliseconds.
    /// 可选 gRPC keepalive ACK 超时时间，单位毫秒。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grpc_keepalive_timeout_ms: Option<u64>,
    /// Optional gRPC idle keepalive policy.
    /// 可选 gRPC 空闲 keepalive 策略。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grpc_keepalive_permit_without_calls: Option<bool>,
    /// Optional VMM user id.
    /// 可选 VMM 用户标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<u64>,
    /// Optional VMM project id.
    /// 可选 VMM 项目标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<u64>,
    /// Optional visible-memory mode switch.
    /// 可选可见记忆模式开关。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible_memory_injection: Option<bool>,
    /// Optional transient recall TTL.
    /// 可选临时召回持续回合数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub implicit_memory_turns: Option<u32>,
    /// Optional accepted-turn interval for profile refresh.
    /// 可选画像刷新 accepted turn 间隔。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_refresh_turns: Option<u32>,
    /// Optional per-target profile node fetch limit.
    /// 可选每个目标画像节点拉取上限。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_node_limit: Option<u32>,
    /// Optional compact-aware recall switch.
    /// 可选压缩感知召回开关。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_compact_recall: Option<bool>,
    /// Optional automatic precheck recall switch.
    /// 可选自动前置召回开关。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub precheck_enabled: Option<bool>,
    /// Optional completed-turn memory writeback switch.
    /// 可选已完成回合记忆写回开关。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_writeback_enabled: Option<bool>,
    /// Optional AI-only VMM tool exposure switch.
    /// 可选 AI 专属 VMM 工具暴露开关。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_tools_enabled: Option<bool>,
    /// Optional injection tag override.
    /// 可选注入标签覆盖。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub injection_tag: Option<String>,
    /// Optional injection description override.
    /// 可选注入说明覆盖。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub injection_description: Option<String>,
    /// Reserved future install-management state.
    /// 预留的未来安装管理状态。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install: Option<VmmInstallReservation>,
}

impl VmmConfigLayer {
    /// Returns whether this layer carries no explicit override.
    /// 返回该配置层是否不包含任何显式覆盖。
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

/// Identifies which layer supplied one effective value.
/// 标识某个最终值来自哪一层配置。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VmmEffectiveConfigSource {
    /// Value came from the built-in default.
    /// 值来自内置默认值。
    Default,
    /// Value came from global configuration.
    /// 值来自全局配置。
    Global,
    /// Value came from project configuration.
    /// 值来自项目配置。
    Project,
    /// Value came from session configuration.
    /// 值来自会话配置。
    Session,
}

/// Final VMM configuration after global, project, and session layers are resolved.
/// 解析全局、项目与会话层之后得到的最终 VMM 配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmmEffectiveConfig {
    /// Final enable switch before online and scope diagnostics.
    /// 在线状态和作用域诊断前的最终启用开关。
    pub enabled: bool,
    /// Source of the final enable switch.
    /// 最终启用开关的来源。
    pub enabled_source: VmmEffectiveConfigSource,
    /// Final gRPC target.
    /// 最终 gRPC 目标。
    pub grpc_target: String,
    /// Source of the final gRPC target.
    /// 最终 gRPC 目标的来源。
    pub grpc_target_source: VmmEffectiveConfigSource,
    /// Final optional gRPC API key.
    /// 最终可选 gRPC API key。
    pub grpc_api_key: Option<String>,
    /// Final handshake timeout.
    /// 最终握手超时时间。
    pub handshake_timeout_ms: u64,
    /// Final receive timeout.
    /// 最终接收超时时间。
    pub receive_timeout_ms: u64,
    /// Final optional max receive bytes.
    /// 最终可选最大接收字节数。
    pub max_receive_message_bytes: Option<usize>,
    /// Final gRPC keepalive interval.
    /// 最终 gRPC keepalive 间隔。
    pub grpc_keepalive_time_ms: u64,
    /// Final gRPC keepalive acknowledgement timeout.
    /// 最终 gRPC keepalive ACK 超时时间。
    pub grpc_keepalive_timeout_ms: u64,
    /// Final gRPC idle keepalive policy.
    /// 最终 gRPC 空闲 keepalive 策略。
    pub grpc_keepalive_permit_without_calls: bool,
    /// Final VMM user id.
    /// 最终 VMM 用户标识。
    pub user_id: Option<u64>,
    /// Final VMM project id.
    /// 最终 VMM 项目标识。
    pub project_id: Option<u64>,
    /// Final visible-memory mode switch.
    /// 最终可见记忆模式开关。
    pub visible_memory_injection: bool,
    /// Final transient recall TTL.
    /// 最终临时召回持续回合数。
    pub implicit_memory_turns: u32,
    /// Final profile refresh interval.
    /// 最终画像刷新间隔。
    pub profile_refresh_turns: u32,
    /// Final per-target profile node fetch limit.
    /// 最终每个目标画像节点拉取上限。
    pub profile_node_limit: u32,
    /// Final compact-aware recall switch.
    /// 最终压缩感知召回开关。
    pub session_compact_recall: bool,
    /// Final automatic precheck recall switch.
    /// 最终自动前置召回开关。
    pub precheck_enabled: bool,
    /// Source of the final automatic precheck recall switch.
    /// 最终自动前置召回开关的来源。
    pub precheck_enabled_source: VmmEffectiveConfigSource,
    /// Final completed-turn memory writeback switch.
    /// 最终已完成回合记忆写回开关。
    pub memory_writeback_enabled: bool,
    /// Source of the final completed-turn memory writeback switch.
    /// 最终已完成回合记忆写回开关的来源。
    pub memory_writeback_enabled_source: VmmEffectiveConfigSource,
    /// Final AI-only VMM tool exposure switch.
    /// 最终 AI 专属 VMM 工具暴露开关。
    pub ai_tools_enabled: bool,
    /// Final injection tag.
    /// 最终注入标签。
    pub injection_tag: String,
    /// Final injection description.
    /// 最终注入说明。
    pub injection_description: String,
    /// Reserved future install-management state.
    /// 预留的未来安装管理状态。
    pub install: VmmInstallReservation,
}

impl Default for VmmEffectiveConfig {
    /// Builds the default disabled and unconfigured VMM effective configuration.
    /// 构建默认关闭且尚未配置的 VMM 最终配置。
    fn default() -> Self {
        Self {
            enabled: false,
            enabled_source: VmmEffectiveConfigSource::Default,
            grpc_target: String::new(),
            grpc_target_source: VmmEffectiveConfigSource::Default,
            grpc_api_key: None,
            handshake_timeout_ms: DEFAULT_HANDSHAKE_TIMEOUT_MS,
            receive_timeout_ms: DEFAULT_RECEIVE_TIMEOUT_MS,
            max_receive_message_bytes: None,
            grpc_keepalive_time_ms: DEFAULT_GRPC_KEEPALIVE_TIME_MS,
            grpc_keepalive_timeout_ms: DEFAULT_GRPC_KEEPALIVE_TIMEOUT_MS,
            grpc_keepalive_permit_without_calls: DEFAULT_GRPC_KEEPALIVE_PERMIT_WITHOUT_CALLS,
            user_id: None,
            project_id: None,
            visible_memory_injection: false,
            implicit_memory_turns: DEFAULT_IMPLICIT_MEMORY_TURNS,
            profile_refresh_turns: DEFAULT_PROFILE_REFRESH_TURNS,
            profile_node_limit: DEFAULT_PROFILE_NODE_LIMIT,
            session_compact_recall: true,
            precheck_enabled: true,
            precheck_enabled_source: VmmEffectiveConfigSource::Default,
            memory_writeback_enabled: true,
            memory_writeback_enabled_source: VmmEffectiveConfigSource::Default,
            ai_tools_enabled: true,
            injection_tag: DEFAULT_VMM_INJECTION_TAG.to_string(),
            injection_description: DEFAULT_VMM_INJECTION_DESCRIPTION.to_string(),
            install: VmmInstallReservation::default(),
        }
    }
}

impl VmmEffectiveConfig {
    /// Resolves final configuration from global, project, and session layers.
    /// 从全局、项目与会话层解析最终配置。
    pub fn resolve(
        global: &VmmConfigLayer,
        project: Option<&VmmConfigLayer>,
        session: Option<&VmmConfigLayer>,
    ) -> Self {
        let mut config = Self::default();
        apply_layer(&mut config, global, VmmEffectiveConfigSource::Global);
        if let Some(project) = project {
            apply_layer(&mut config, project, VmmEffectiveConfigSource::Project);
        }
        if let Some(session) = session {
            apply_layer(&mut config, session, VmmEffectiveConfigSource::Session);
        }
        config.grpc_target = config.grpc_target.trim().to_string();
        config.visible_memory_injection = false;
        config.session_compact_recall = true;
        config.injection_tag = normalize_tag_or_default(&config.injection_tag);
        config.injection_description =
            normalize_description_or_default(&config.injection_description);
        config
    }

    /// Returns the recall mode implied by compact-aware recall.
    /// 返回压缩感知召回开关对应的召回模式。
    pub const fn precheck_recall_mode(&self) -> PreCheckRecallMode {
        if self.session_compact_recall {
            PreCheckRecallMode::PreCheckRecallModeSessionCompact
        } else {
            PreCheckRecallMode::PreCheckRecallModeFull
        }
    }

    /// Diagnoses whether ordinary VMM memory flow may run now.
    /// 诊断当前普通 VMM 记忆链路是否可运行。
    pub fn diagnose_memory_flow(&self, grpc_online: bool) -> VmmConfigDiagnostic {
        if !self.enabled {
            return VmmConfigDiagnostic::DisabledByConfig;
        }
        if self.grpc_target.trim().is_empty() {
            return VmmConfigDiagnostic::MissingGrpcTarget;
        }
        if !grpc_online {
            return VmmConfigDiagnostic::GrpcOffline;
        }
        if self.user_id.filter(|value| *value > 0).is_none() {
            return VmmConfigDiagnostic::MissingUserId;
        }
        if self.project_id.filter(|value| *value > 0).is_none() {
            return VmmConfigDiagnostic::MissingProjectId;
        }
        VmmConfigDiagnostic::Ready
    }

    /// Diagnoses whether automatic VMM precheck recall may run now.
    /// 诊断当前自动 VMM 前置召回是否可运行。
    pub fn diagnose_precheck_flow(&self, grpc_online: bool) -> VmmConfigDiagnostic {
        if !self.precheck_enabled {
            return VmmConfigDiagnostic::DisabledByConfig;
        }
        self.diagnose_memory_flow(grpc_online)
    }

    /// Diagnoses whether completed-turn memory writeback may run now.
    /// 诊断当前已完成回合记忆写回是否可运行。
    pub fn diagnose_writeback_flow(&self, grpc_online: bool) -> VmmConfigDiagnostic {
        if !self.memory_writeback_enabled {
            return VmmConfigDiagnostic::DisabledByConfig;
        }
        self.diagnose_memory_flow(grpc_online)
    }
}

/// Effective VMM enablement diagnostic.
/// 最终 VMM 启用诊断。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VmmConfigDiagnostic {
    /// VMM can run.
    /// VMM 可以运行。
    Ready,
    /// A configuration layer disabled VMM.
    /// 某层配置禁用了 VMM。
    DisabledByConfig,
    /// No gRPC target is configured.
    /// 未配置 gRPC 目标。
    MissingGrpcTarget,
    /// gRPC target is configured but currently offline.
    /// gRPC 目标已配置但当前离线。
    GrpcOffline,
    /// No valid VMM user id is configured.
    /// 未配置有效 VMM 用户标识。
    MissingUserId,
    /// No valid VMM project id is configured.
    /// 未配置有效 VMM 项目标识。
    MissingProjectId,
}

/// Applies one layer to an effective config.
/// 将一层配置应用到最终配置。
fn apply_layer(
    config: &mut VmmEffectiveConfig,
    layer: &VmmConfigLayer,
    source: VmmEffectiveConfigSource,
) {
    if let Some(enabled) = layer.enabled {
        config.enabled = enabled;
        config.enabled_source = source;
    }
    if let Some(grpc_target) = non_empty(&layer.grpc_target) {
        config.grpc_target = grpc_target.to_string();
        config.grpc_target_source = source;
    }
    if let Some(grpc_api_key) = non_empty(&layer.grpc_api_key) {
        config.grpc_api_key = Some(grpc_api_key.to_string());
    }
    if let Some(handshake_timeout_ms) = layer.handshake_timeout_ms {
        config.handshake_timeout_ms = handshake_timeout_ms;
    }
    if let Some(receive_timeout_ms) = layer.receive_timeout_ms {
        config.receive_timeout_ms = receive_timeout_ms;
    }
    if let Some(max_receive_message_bytes) = layer.max_receive_message_bytes {
        config.max_receive_message_bytes = Some(max_receive_message_bytes);
    }
    if let Some(grpc_keepalive_time_ms) = layer.grpc_keepalive_time_ms {
        config.grpc_keepalive_time_ms = grpc_keepalive_time_ms;
    }
    if let Some(grpc_keepalive_timeout_ms) = layer.grpc_keepalive_timeout_ms {
        config.grpc_keepalive_timeout_ms = grpc_keepalive_timeout_ms;
    }
    if let Some(grpc_keepalive_permit_without_calls) = layer.grpc_keepalive_permit_without_calls {
        config.grpc_keepalive_permit_without_calls = grpc_keepalive_permit_without_calls;
    }
    if let Some(user_id) = layer.user_id {
        config.user_id = Some(user_id);
    }
    if let Some(project_id) = layer.project_id {
        config.project_id = Some(project_id);
    }
    if let Some(implicit_memory_turns) = layer.implicit_memory_turns {
        config.implicit_memory_turns = implicit_memory_turns;
    }
    if let Some(profile_refresh_turns) = layer.profile_refresh_turns {
        config.profile_refresh_turns = profile_refresh_turns;
    }
    if let Some(profile_node_limit) = layer.profile_node_limit {
        config.profile_node_limit = normalize_profile_node_limit(profile_node_limit);
    }
    if let Some(precheck_enabled) = layer.precheck_enabled {
        config.precheck_enabled = precheck_enabled;
        config.precheck_enabled_source = source;
    }
    if let Some(memory_writeback_enabled) = layer.memory_writeback_enabled {
        config.memory_writeback_enabled = memory_writeback_enabled;
        config.memory_writeback_enabled_source = source;
    }
    if let Some(ai_tools_enabled) = layer.ai_tools_enabled {
        config.ai_tools_enabled = ai_tools_enabled;
    }
    if let Some(injection_tag) = non_empty(&layer.injection_tag) {
        config.injection_tag = injection_tag.to_string();
    }
    if let Some(injection_description) = non_empty(&layer.injection_description) {
        config.injection_description = injection_description.to_string();
    }
    if let Some(install) = layer.install.clone() {
        config.install = install;
    }
}

/// Returns a trimmed non-empty option string.
/// 返回裁剪后的非空可选字符串。
fn non_empty(value: &Option<String>) -> Option<&str> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// Normalizes an XML-like injection tag or falls back to the default.
/// 规范化类 XML 注入标签，失败时回退默认标签。
fn normalize_tag_or_default(value: &str) -> String {
    let normalized = value
        .trim()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '-')
        .collect::<String>();
    if normalized.is_empty() {
        DEFAULT_VMM_INJECTION_TAG.to_string()
    } else {
        normalized
    }
}

/// Normalizes an injection description or falls back to the default.
/// 规范化注入说明，失败时回退默认说明。
fn normalize_description_or_default(value: &str) -> String {
    let normalized = value.trim();
    if normalized.is_empty() {
        DEFAULT_VMM_INJECTION_DESCRIPTION.to_string()
    } else {
        normalized.to_string()
    }
}

/// Normalizes a configured profile-node limit into the accepted host range.
/// 将配置的画像节点上限规范化到宿主接受范围内。
const fn normalize_profile_node_limit(value: u32) -> u32 {
    if value == 0 {
        DEFAULT_PROFILE_NODE_LIMIT
    } else if value > MAX_PROFILE_NODE_LIMIT {
        MAX_PROFILE_NODE_LIMIT
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    //! Tests for VMM configuration resolution.
    //! VMM 配置解析测试。

    use super::*;

    /// Builds a minimal layer with only ids and target.
    /// 构建只包含标识与目标的最小配置层。
    fn base_global_layer() -> VmmConfigLayer {
        VmmConfigLayer {
            enabled: Some(true),
            grpc_target: Some(String::from("127.0.0.1:50051")),
            user_id: Some(7),
            project_id: Some(9),
            ..VmmConfigLayer::default()
        }
    }

    /// Verifies an installation without any persisted VMM layer stays disabled.
    /// 验证没有任何持久化 VMM 配置层的全新安装保持禁用。
    #[test]
    fn default_effective_config_is_disabled() {
        let resolved = VmmEffectiveConfig::resolve(&VmmConfigLayer::default(), None, None);

        assert!(!resolved.enabled);
        assert_eq!(
            resolved.diagnose_memory_flow(true),
            VmmConfigDiagnostic::DisabledByConfig
        );
    }

    #[test]
    fn resolve_prefers_session_over_project_over_global() {
        let global = base_global_layer();
        let project = VmmConfigLayer {
            project_id: Some(100),
            implicit_memory_turns: Some(4),
            ..VmmConfigLayer::default()
        };
        let session = VmmConfigLayer {
            enabled: Some(false),
            implicit_memory_turns: Some(2),
            ..VmmConfigLayer::default()
        };

        let resolved = VmmEffectiveConfig::resolve(&global, Some(&project), Some(&session));

        assert!(!resolved.enabled);
        assert_eq!(resolved.enabled_source, VmmEffectiveConfigSource::Session);
        assert_eq!(resolved.project_id, Some(100));
        assert_eq!(resolved.implicit_memory_turns, 2);
    }

    #[test]
    fn resolve_forces_hidden_memory_and_compact_recall() {
        let global = VmmConfigLayer {
            visible_memory_injection: Some(true),
            session_compact_recall: Some(false),
            ..base_global_layer()
        };
        let project = VmmConfigLayer {
            visible_memory_injection: Some(true),
            session_compact_recall: Some(false),
            ..VmmConfigLayer::default()
        };
        let session = VmmConfigLayer {
            visible_memory_injection: Some(true),
            session_compact_recall: Some(false),
            ..VmmConfigLayer::default()
        };

        let resolved = VmmEffectiveConfig::resolve(&global, Some(&project), Some(&session));

        assert!(!resolved.visible_memory_injection);
        assert!(resolved.session_compact_recall);
        assert_eq!(
            resolved.precheck_recall_mode(),
            PreCheckRecallMode::PreCheckRecallModeSessionCompact
        );
    }

    #[test]
    fn diagnose_requires_online_target_and_business_scope() {
        let resolved = VmmEffectiveConfig::resolve(&base_global_layer(), None, None);
        assert_eq!(
            resolved.diagnose_memory_flow(true),
            VmmConfigDiagnostic::Ready
        );
        assert_eq!(
            resolved.diagnose_memory_flow(false),
            VmmConfigDiagnostic::GrpcOffline
        );

        let missing_target = VmmEffectiveConfig::resolve(
            &VmmConfigLayer {
                enabled: Some(true),
                ..VmmConfigLayer::default()
            },
            None,
            None,
        );
        assert_eq!(
            missing_target.diagnose_memory_flow(true),
            VmmConfigDiagnostic::MissingGrpcTarget
        );
    }

    #[test]
    fn precheck_enabled_can_be_overridden_by_session_layer() {
        let global = VmmConfigLayer {
            precheck_enabled: Some(true),
            ..base_global_layer()
        };
        let session = VmmConfigLayer {
            precheck_enabled: Some(false),
            ..VmmConfigLayer::default()
        };
        let resolved = VmmEffectiveConfig::resolve(&global, None, Some(&session));

        assert!(!resolved.precheck_enabled);
        assert_eq!(
            resolved.diagnose_precheck_flow(true),
            VmmConfigDiagnostic::DisabledByConfig
        );
        assert_eq!(
            resolved.diagnose_memory_flow(true),
            VmmConfigDiagnostic::Ready
        );
    }

    #[test]
    fn profile_node_limit_uses_default_for_zero_and_clamps_maximum() {
        let zero = VmmConfigLayer {
            profile_node_limit: Some(0),
            ..base_global_layer()
        };
        let oversized = VmmConfigLayer {
            profile_node_limit: Some(MAX_PROFILE_NODE_LIMIT + 1),
            ..base_global_layer()
        };

        assert_eq!(
            VmmEffectiveConfig::resolve(&zero, None, None).profile_node_limit,
            DEFAULT_PROFILE_NODE_LIMIT
        );
        assert_eq!(
            VmmEffectiveConfig::resolve(&oversized, None, None).profile_node_limit,
            MAX_PROFILE_NODE_LIMIT
        );
    }

    /// Verifies rollout feature values require an explicit disabled spelling.
    /// 验证灰度功能值只有显式禁用拼写才会关闭。
    #[test]
    fn feature_value_defaults_enabled_and_recognizes_disabled_spellings() {
        assert!(feature_value_enabled(None));
        assert!(feature_value_enabled(Some("true")));
        assert!(feature_value_enabled(Some("unexpected")));
        assert!(!feature_value_enabled(Some("0")));
        assert!(!feature_value_enabled(Some(" FALSE ")));
        assert!(!feature_value_enabled(Some("off")));
        assert!(!feature_value_enabled(Some("no")));
    }

    /// Verifies pre-v2 persisted settings receive defaults only for the three fields introduced by managed contract v2.
    /// 验证 v2 之前的持久化设置只会为托管契约 v2 新增的三个字段补充默认值。
    #[test]
    fn pre_v2_managed_runtime_settings_default_only_v2_fields() {
        // persistedSettings mirrors an otherwise complete pre-v2 document before the new reliability fields existed.
        // persistedSettings 模拟新增可靠性字段之前、其余内容完整的 v2 前文档。
        let mut persisted_settings = serde_json::to_value(VmmManagedRuntimeSettings::default())
            .expect("serialize managed runtime settings");
        let persisted_object = persisted_settings
            .as_object_mut()
            .expect("managed runtime settings must serialize as an object");
        persisted_object.remove("postaction_analysis_timeout");
        persisted_object.remove("postaction_failure_pass_threshold");
        persisted_object.remove("inference_max_connections_per_host");

        // restoredSettings proves the compatibility path uses the exact audited defaults.
        // restoredSettings 证明兼容路径使用精确的审计默认值。
        let restored_settings: VmmManagedRuntimeSettings =
            serde_json::from_value(persisted_settings.clone())
                .expect("deserialize pre-v2 managed runtime settings");
        assert_eq!(restored_settings.postaction_analysis_timeout, "180s");
        assert_eq!(restored_settings.postaction_failure_pass_threshold, 5);
        assert_eq!(restored_settings.inference_max_connections_per_host, 8);

        // Removing a legacy required field must still fail closed instead of broad-defaulting damaged documents.
        // 删除旧有必填字段后仍必须失败关闭，不能对损坏文档进行宽泛默认填充。
        persisted_settings
            .as_object_mut()
            .expect("managed runtime settings must remain an object")
            .remove("prompt_language");
        assert!(serde_json::from_value::<VmmManagedRuntimeSettings>(persisted_settings).is_err());
    }
}
