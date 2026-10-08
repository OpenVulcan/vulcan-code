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

/// Fixed native subject identity used by the single-user VulcanCode host.
/// 单用户 VulcanCode 宿主使用的固定原生主体标识。
pub const DEFAULT_NATIVE_USER_ID: u64 = 1;

/// Protected fallback native VMM project identity.
/// 受保护的原生 VMM 兜底项目标识。
pub const DEFAULT_NATIVE_PROJECT_ID: u64 = 1;

/// Default external Team/Space prefix used for automatically created VMM projects.
/// 自动创建外置 VMM 项目时使用的默认 Team/Space 前缀。
pub const DEFAULT_EXTERNAL_PROJECT_PREFIX: &str = "vulcan/space";

/// Selects how newly discovered VulcanCode projects obtain a native VMM owner project.
/// 选择新发现的 VulcanCode 项目如何获得原生 VMM 所有者项目。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VmmNativeProjectBindingMode {
    /// Binds every new VulcanCode project to one configured native VMM project.
    /// 将每个新增 VulcanCode 项目绑定到一个已配置的原生 VMM 项目。
    #[default]
    Fixed,
    /// Creates or reuses one native VMM project from each VulcanCode project tag.
    /// 根据每个 VulcanCode 项目标签创建或复用一个原生 VMM 项目。
    PerProject,
}

impl VmmNativeProjectBindingMode {
    /// Returns the stable serialized spelling used by service APIs.
    /// 返回服务 API 使用的稳定序列化名称。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fixed => "fixed",
            Self::PerProject => "per_project",
        }
    }
}

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

/// Selects the process-local native core or the externally hosted gRPC backend.
/// 选择进程内原生 Core 或外置 gRPC 后端。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VmmBackendMode {
    /// Uses the Rust-native VMM Core owned by VulcanCode.
    /// 使用由 VulcanCode 负责生命周期的 Rust 原生 VMM Core。
    #[default]
    Native,
    /// Uses the externally deployed VMM gRPC service.
    /// 使用由用户自行部署的外置 VMM gRPC 服务。
    ExternalGrpc,
}

impl VmmBackendMode {
    /// Returns the stable serialized spelling used by service APIs.
    /// 返回服务 API 使用的稳定序列化名称。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::ExternalGrpc => "external_grpc",
        }
    }
}

/// One optional VMM configuration layer.
/// 一层可选的 VMM 配置。
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmmConfigLayer {
    /// Optional global backend selection; project and session layers leave it unset.
    /// 可选的全局后端选择；项目层和会话层保持未设置。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend_mode: Option<VmmBackendMode>,
    /// Optional global native project binding policy; project and session layers ignore it.
    /// 可选的全局原生项目绑定策略；项目层与会话层忽略该字段。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_project_binding_mode: Option<VmmNativeProjectBindingMode>,
    /// Optional global external project binding policy; project and session layers ignore it.
    /// 可选的全局外置项目绑定策略；项目层与会话层忽略该字段。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_project_binding_mode: Option<VmmNativeProjectBindingMode>,
    /// Optional global fixed native VMM project identity.
    /// 可选的全局固定原生 VMM 项目标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_fixed_project_id: Option<u64>,
    /// Optional global Team/Space prefix for external automatic project creation.
    /// 外置 VMM 自动创建项目时使用的可选全局 Team/Space 前缀。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_project_prefix: Option<String>,
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
}

impl std::fmt::Debug for VmmConfigLayer {
    /// Formats every override while reducing the transport secrets to presence flags.
    /// 格式化全部覆盖项，同时将传输秘密收敛为存在性标记。
    ///
    /// # Parameters
    /// 参数
    /// `formatter` receives the redacted structural representation.
    /// `formatter` 接收脱敏后的结构表示。
    ///
    /// # Returns
    /// 返回值
    /// Returns the formatter result without any secret value.
    /// 返回不含任何秘密值的格式化结果。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VmmConfigLayer")
            .field("backend_mode", &self.backend_mode)
            .field(
                "native_project_binding_mode",
                &self.native_project_binding_mode,
            )
            .field(
                "external_project_binding_mode",
                &self.external_project_binding_mode,
            )
            .field("native_fixed_project_id", &self.native_fixed_project_id)
            .field("external_project_prefix", &self.external_project_prefix)
            .field("enabled", &self.enabled)
            .field("grpc_target", &self.grpc_target)
            .field("has_grpc_api_key", &self.grpc_api_key.is_some())
            .field("management_base_url", &self.management_base_url)
            .field(
                "has_management_bearer_token",
                &self.management_bearer_token.is_some(),
            )
            .field("management_timeout_ms", &self.management_timeout_ms)
            .field("handshake_timeout_ms", &self.handshake_timeout_ms)
            .field("receive_timeout_ms", &self.receive_timeout_ms)
            .field("max_receive_message_bytes", &self.max_receive_message_bytes)
            .field("grpc_keepalive_time_ms", &self.grpc_keepalive_time_ms)
            .field("grpc_keepalive_timeout_ms", &self.grpc_keepalive_timeout_ms)
            .field(
                "grpc_keepalive_permit_without_calls",
                &self.grpc_keepalive_permit_without_calls,
            )
            .field("user_id", &self.user_id)
            .field("project_id", &self.project_id)
            .field("visible_memory_injection", &self.visible_memory_injection)
            .field("implicit_memory_turns", &self.implicit_memory_turns)
            .field("profile_refresh_turns", &self.profile_refresh_turns)
            .field("profile_node_limit", &self.profile_node_limit)
            .field("session_compact_recall", &self.session_compact_recall)
            .field("precheck_enabled", &self.precheck_enabled)
            .field("memory_writeback_enabled", &self.memory_writeback_enabled)
            .field("ai_tools_enabled", &self.ai_tools_enabled)
            .field("injection_tag", &self.injection_tag)
            .field("injection_description", &self.injection_description)
            .finish()
    }
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
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmmEffectiveConfig {
    /// Final backend selected by the global configuration.
    /// 由全局配置选择出的最终后端。
    pub backend_mode: VmmBackendMode,
    /// Final native project binding policy resolved only from global configuration.
    /// 仅从全局配置解析出的最终原生项目绑定策略。
    pub native_project_binding_mode: VmmNativeProjectBindingMode,
    /// Final external project binding policy resolved only from global configuration.
    /// 仅从全局配置解析出的最终外置项目绑定策略。
    pub external_project_binding_mode: VmmNativeProjectBindingMode,
    /// Final fixed native VMM project identity.
    /// 最终固定原生 VMM 项目标识。
    pub native_fixed_project_id: u64,
    /// Final external Team/Space prefix for automatic project creation.
    /// 外置 VMM 自动创建项目使用的最终 Team/Space 前缀。
    pub external_project_prefix: String,
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
}

impl std::fmt::Debug for VmmEffectiveConfig {
    /// Formats the resolved configuration while reducing the transport secret to a presence flag.
    /// 格式化解析后的配置，同时将传输秘密收敛为存在性标记。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VmmEffectiveConfig")
            .field("backend_mode", &self.backend_mode)
            .field(
                "native_project_binding_mode",
                &self.native_project_binding_mode,
            )
            .field(
                "external_project_binding_mode",
                &self.external_project_binding_mode,
            )
            .field("native_fixed_project_id", &self.native_fixed_project_id)
            .field("external_project_prefix", &self.external_project_prefix)
            .field("enabled", &self.enabled)
            .field("enabled_source", &self.enabled_source)
            .field("grpc_target", &self.grpc_target)
            .field("grpc_target_source", &self.grpc_target_source)
            .field("has_grpc_api_key", &self.grpc_api_key.is_some())
            .field("handshake_timeout_ms", &self.handshake_timeout_ms)
            .field("receive_timeout_ms", &self.receive_timeout_ms)
            .field("max_receive_message_bytes", &self.max_receive_message_bytes)
            .field("grpc_keepalive_time_ms", &self.grpc_keepalive_time_ms)
            .field("grpc_keepalive_timeout_ms", &self.grpc_keepalive_timeout_ms)
            .field(
                "grpc_keepalive_permit_without_calls",
                &self.grpc_keepalive_permit_without_calls,
            )
            .field("user_id", &self.user_id)
            .field("project_id", &self.project_id)
            .field("visible_memory_injection", &self.visible_memory_injection)
            .field("implicit_memory_turns", &self.implicit_memory_turns)
            .field("profile_refresh_turns", &self.profile_refresh_turns)
            .field("profile_node_limit", &self.profile_node_limit)
            .field("session_compact_recall", &self.session_compact_recall)
            .field("precheck_enabled", &self.precheck_enabled)
            .field("precheck_enabled_source", &self.precheck_enabled_source)
            .field("memory_writeback_enabled", &self.memory_writeback_enabled)
            .field(
                "memory_writeback_enabled_source",
                &self.memory_writeback_enabled_source,
            )
            .field("ai_tools_enabled", &self.ai_tools_enabled)
            .field("injection_tag", &self.injection_tag)
            .field("injection_description", &self.injection_description)
            .finish()
    }
}

impl Default for VmmEffectiveConfig {
    /// Builds the default disabled and unconfigured VMM effective configuration.
    /// 构建默认关闭且尚未配置的 VMM 最终配置。
    fn default() -> Self {
        Self {
            backend_mode: VmmBackendMode::Native,
            native_project_binding_mode: VmmNativeProjectBindingMode::Fixed,
            external_project_binding_mode: VmmNativeProjectBindingMode::Fixed,
            native_fixed_project_id: DEFAULT_NATIVE_PROJECT_ID,
            external_project_prefix: DEFAULT_EXTERNAL_PROJECT_PREFIX.to_string(),
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
        // Backend selection is application-global. Project and session layers may control memory
        // behavior, but they cannot switch the process-wide native/external implementation.
        // 后端选择属于应用全局配置；项目层与会话层可以控制记忆行为，但不能切换进程级原生/外置实现。
        config.backend_mode = global.backend_mode.unwrap_or_else(|| {
            // A pre-existing non-empty global gRPC target is the only unambiguous legacy signal
            // when the explicit selector is absent. An empty configuration keeps Native.
            // 显式选择器缺失时，既有非空全局 gRPC 目标是唯一明确的旧配置外置信号；空配置继续使用 Native。
            if non_empty(&global.grpc_target).is_some() {
                VmmBackendMode::ExternalGrpc
            } else {
                VmmBackendMode::Native
            }
        });
        config.grpc_target = config.grpc_target.trim().to_string();
        config.visible_memory_injection = false;
        config.session_compact_recall = true;
        config.injection_tag = normalize_tag_or_default(&config.injection_tag);
        config.injection_description =
            normalize_description_or_default(&config.injection_description);
        // Native execution resolves the real project scope from the host project binding. These
        // fixed envelope identifiers keep fallback calls valid, while external gRPC continues to
        // require its explicitly configured remote identities.
        // 原生执行会从宿主项目绑定解析真实项目作用域；固定封套标识保证兜底调用有效，外置 gRPC 则继续要求显式远端身份。
        if config.backend_mode == VmmBackendMode::Native {
            config.user_id = Some(DEFAULT_NATIVE_USER_ID);
            config.project_id = Some(DEFAULT_NATIVE_PROJECT_ID);
        }
        // Project binding behavior is global application policy and must never inherit project or
        // session overrides. Empty persisted prefixes resolve to the public external default.
        // 项目绑定行为属于全局应用策略，绝不能继承项目层或会话层覆盖；空持久化前缀回退到公开的外置默认值。
        config.native_project_binding_mode = global.native_project_binding_mode.unwrap_or_default();
        config.external_project_binding_mode =
            global.external_project_binding_mode.unwrap_or_default();
        config.native_fixed_project_id = global
            .native_fixed_project_id
            .filter(|project_id| *project_id > 0)
            .unwrap_or(DEFAULT_NATIVE_PROJECT_ID);
        config.external_project_prefix =
            normalize_external_project_prefix(global.external_project_prefix.as_deref());
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
        self.diagnose_memory_configuration(grpc_online)
    }

    /// Diagnoses saved backend requirements independently of the memory execution switch.
    /// 独立于记忆执行开关诊断已保存的后端配置要求。
    /// `grpc_online` reports external transport availability; native configuration ignores it.
    /// `grpc_online` 表示外部传输可用性；原生配置不使用该参数。
    /// Returns the first missing requirement, or Ready even when configured memory is paused.
    /// 返回首个未满足要求；配置完整时即使记忆已暂停也返回 Ready。
    pub fn diagnose_memory_configuration(&self, grpc_online: bool) -> VmmConfigDiagnostic {
        if self.backend_mode == VmmBackendMode::Native {
            return VmmConfigDiagnostic::Ready;
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
    if let Some(backend_mode) = layer.backend_mode {
        config.backend_mode = backend_mode;
    }
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

/// Normalizes one external automatic-project prefix to an exact Team/Space pair.
/// 将一个外置自动项目的前缀规范化为准确的 Team/Space 二元组。
fn normalize_external_project_prefix(value: Option<&str>) -> String {
    let Some(value) = value else {
        return DEFAULT_EXTERNAL_PROJECT_PREFIX.to_string();
    };
    let segments = value.split('/').map(str::trim).collect::<Vec<_>>();
    if segments.len() != 2 || segments.iter().any(|segment| segment.is_empty()) {
        return DEFAULT_EXTERNAL_PROJECT_PREFIX.to_string();
    }
    format!("{}/{}", segments[0], segments[1])
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
    fn native_identity_is_host_owned_while_external_identity_remains_explicit() {
        let native = VmmEffectiveConfig::resolve(
            &VmmConfigLayer {
                backend_mode: Some(VmmBackendMode::Native),
                user_id: Some(91),
                project_id: Some(92),
                ..VmmConfigLayer::default()
            },
            None,
            None,
        );
        assert_eq!(native.user_id, Some(DEFAULT_NATIVE_USER_ID));
        assert_eq!(native.project_id, Some(DEFAULT_NATIVE_PROJECT_ID));

        let external = VmmEffectiveConfig::resolve(
            &VmmConfigLayer {
                backend_mode: Some(VmmBackendMode::ExternalGrpc),
                ..VmmConfigLayer::default()
            },
            None,
            None,
        );
        assert_eq!(external.user_id, None);
        assert_eq!(external.project_id, None);
    }

    /// Verifies project and session layers cannot change the global backend implementation.
    /// 验证项目层与会话层不能改变全局后端实现。
    #[test]
    fn backend_mode_is_global_only() {
        let global = VmmConfigLayer {
            backend_mode: Some(VmmBackendMode::ExternalGrpc),
            ..VmmConfigLayer::default()
        };
        let project = VmmConfigLayer {
            backend_mode: Some(VmmBackendMode::Native),
            ..VmmConfigLayer::default()
        };
        let session = VmmConfigLayer {
            backend_mode: Some(VmmBackendMode::Native),
            ..VmmConfigLayer::default()
        };

        let resolved = VmmEffectiveConfig::resolve(&global, Some(&project), Some(&session));

        assert_eq!(resolved.backend_mode, VmmBackendMode::ExternalGrpc);
    }

    /// Verifies project-binding behavior is global-only and has stable public defaults.
    /// 验证项目绑定行为仅由全局层控制，并具有稳定的公开默认值。
    #[test]
    fn project_binding_policy_is_global_only() {
        let global = VmmConfigLayer {
            native_project_binding_mode: Some(VmmNativeProjectBindingMode::PerProject),
            external_project_binding_mode: Some(VmmNativeProjectBindingMode::PerProject),
            native_fixed_project_id: Some(42),
            external_project_prefix: Some(String::from(" custom / memory ")),
            ..VmmConfigLayer::default()
        };
        let project = VmmConfigLayer {
            native_project_binding_mode: Some(VmmNativeProjectBindingMode::Fixed),
            external_project_binding_mode: Some(VmmNativeProjectBindingMode::Fixed),
            native_fixed_project_id: Some(7),
            external_project_prefix: Some(String::from("ignored/project")),
            ..VmmConfigLayer::default()
        };
        let resolved = VmmEffectiveConfig::resolve(&global, Some(&project), None);

        assert_eq!(
            resolved.native_project_binding_mode,
            VmmNativeProjectBindingMode::PerProject
        );
        assert_eq!(
            resolved.external_project_binding_mode,
            VmmNativeProjectBindingMode::PerProject
        );
        assert_eq!(resolved.native_fixed_project_id, 42);
        assert_eq!(resolved.external_project_prefix, "custom/memory");

        let defaults = VmmEffectiveConfig::default();
        assert_eq!(
            defaults.native_project_binding_mode,
            VmmNativeProjectBindingMode::Fixed
        );
        assert_eq!(
            defaults.external_project_binding_mode,
            VmmNativeProjectBindingMode::Fixed
        );
        assert_eq!(defaults.native_fixed_project_id, DEFAULT_NATIVE_PROJECT_ID);
        assert_eq!(
            defaults.external_project_prefix,
            DEFAULT_EXTERNAL_PROJECT_PREFIX
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
                backend_mode: Some(VmmBackendMode::ExternalGrpc),
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

    /// Preserves configured backend readiness while keeping paused memory execution disabled.
    /// 保留已配置后端的配置就绪状态，同时维持暂停期间的记忆执行禁用状态。
    #[test]
    fn paused_memory_keeps_configuration_ready_without_enabling_flows() {
        // Both backends share the execution switch but validate their own configuration requirements.
        // 两种后端共用执行开关，但分别校验自身的配置要求。
        for backend in [VmmBackendMode::Native, VmmBackendMode::ExternalGrpc] {
            // The saved endpoint and identity are complete even though memory is paused.
            // 即使记忆已暂停，保存的端点和身份仍然完整。
            let resolved = VmmEffectiveConfig::resolve(
                &VmmConfigLayer {
                    backend_mode: Some(backend),
                    enabled: Some(false),
                    precheck_enabled: Some(true),
                    memory_writeback_enabled: Some(true),
                    ..base_global_layer()
                },
                None,
                None,
            );
            assert_eq!(
                resolved.diagnose_memory_configuration(true),
                VmmConfigDiagnostic::Ready
            );
            assert_eq!(
                resolved.diagnose_memory_flow(true),
                VmmConfigDiagnostic::DisabledByConfig
            );
            assert_eq!(
                resolved.diagnose_precheck_flow(true),
                VmmConfigDiagnostic::DisabledByConfig
            );
            assert_eq!(
                resolved.diagnose_writeback_flow(true),
                VmmConfigDiagnostic::DisabledByConfig
            );
        }
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
}
