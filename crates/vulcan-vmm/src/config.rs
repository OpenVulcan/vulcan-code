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

/// Reserved future install-management state.
/// 预留的未来安装管理状态。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Optional compact-aware recall switch.
    /// 可选压缩感知召回开关。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_compact_recall: Option<bool>,
    /// Optional automatic precheck recall switch.
    /// 可选自动前置召回开关。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub precheck_enabled: Option<bool>,
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
    /// Final compact-aware recall switch.
    /// 最终压缩感知召回开关。
    pub session_compact_recall: bool,
    /// Final automatic precheck recall switch.
    /// 最终自动前置召回开关。
    pub precheck_enabled: bool,
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
    /// Builds the default disabled-but-ready VMM effective configuration.
    /// 构建默认的未配置但可用 VMM 最终配置。
    fn default() -> Self {
        Self {
            enabled: true,
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
            session_compact_recall: true,
            precheck_enabled: true,
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
            PreCheckRecallMode::PreCheckRecallModeLegacy
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
    if let Some(precheck_enabled) = layer.precheck_enabled {
        config.precheck_enabled = precheck_enabled;
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

#[cfg(test)]
mod tests {
    //! Tests for VMM configuration resolution.
    //! VMM 配置解析测试。

    use super::*;

    /// Builds a minimal layer with only ids and target.
    /// 构建只包含标识与目标的最小配置层。
    fn base_global_layer() -> VmmConfigLayer {
        VmmConfigLayer {
            grpc_target: Some(String::from("127.0.0.1:50051")),
            user_id: Some(7),
            project_id: Some(9),
            ..VmmConfigLayer::default()
        }
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

        let missing_target = VmmEffectiveConfig::resolve(&VmmConfigLayer::default(), None, None);
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
}
