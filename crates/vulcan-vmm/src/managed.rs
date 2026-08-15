//! Strongly typed Vulcan Code managed-runtime manifest and readiness contracts.
//! Vulcan Code 托管运行时清单与就绪握手的强类型契约。

use std::fmt;

use serde::{Deserialize, Serialize};

/// Exact managed manifest version accepted by both processes.
/// 两个进程共同接受的精确托管清单版本。
pub const MANAGED_CONTRACT_VERSION: u32 = 2;

/// Stable build identity shared by the packaged VMM binaries and the host manifest.
/// 打包 VMM 二进制与宿主清单共享的稳定构建身份。
pub const MANAGED_RUNTIME_BUILD_ID: &str = "vmm-runtime-v1";

/// Stable authority owner written into every generated manifest.
/// 写入每份生成清单的稳定权威所有者。
pub const MANAGED_CONTRACT_OWNER: &str = "vulcan-code";

/// Closed inference mode that routes all VMM AI work through Vulcan Code.
/// 将全部 VMM AI 工作路由到 Vulcan Code 的封闭推理模式。
pub const MANAGED_INFERENCE_MODE: &str = "vulcan_code";

/// Identifies the exact parent process that owns one VMM generation.
/// 标识拥有单个 VMM 代次的精确父进程。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedParent {
    /// Host process identifier.
    /// 宿主进程标识。
    pub process_id: u32,
    /// Host process start identity in Unix milliseconds.
    /// 宿主进程的 Unix 毫秒启动身份。
    pub started_at_unix_ms: u64,
}

/// Identifies system and optional user-managed prompt assets.
/// 标识系统提示词资产与可选用户托管资产。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedAssets {
    /// Existing absolute system asset root.
    /// 已存在的系统资产绝对根目录。
    pub system_root: String,
    /// Optional existing absolute user override root.
    /// 可选的已存在用户覆盖绝对根目录。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_override_root: Option<String>,
}

/// Contains method-specific gRPC request timeouts.
/// 包含各方法专用的 gRPC 请求超时。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedGrpcRequestTimeout {
    /// Workspace-management RPC timeout.
    /// 工作区管理 RPC 超时。
    pub workspace: String,
    /// PreCheck RPC timeout.
    /// PreCheck RPC 超时。
    pub pre_check: String,
    /// PostAction RPC timeout.
    /// PostAction RPC 超时。
    pub post_action: String,
}

/// Contains server keepalive and connection lifecycle limits.
/// 包含服务端保活与连接生命周期限制。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedGrpcKeepalive {
    /// Enables server keepalive.
    /// 是否启用服务端保活。
    pub enabled: bool,
    /// Idle period before a ping.
    /// 发送 Ping 前的空闲时间。
    pub time: String,
    /// Ping acknowledgement timeout.
    /// Ping 应答超时。
    pub timeout: String,
    /// Optional idle connection limit.
    /// 可选空闲连接限制。
    pub max_connection_idle: String,
    /// Optional absolute connection-age limit.
    /// 可选绝对连接寿命限制。
    pub max_connection_age: String,
    /// Connection drain grace period.
    /// 连接排空宽限期。
    pub max_connection_age_grace: String,
    /// Minimum accepted client ping interval.
    /// 客户端 Ping 的最小允许间隔。
    pub min_ping_interval: String,
    /// Allows pings without an active stream.
    /// 是否允许无活动流时发送 Ping。
    pub permit_without_stream: bool,
}

/// Contains the child gRPC listener and authentication policy.
/// 包含子进程 gRPC 监听与鉴权策略。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedGrpcConfig {
    /// Preferred loopback address, with dynamic fallback controlled separately.
    /// 首选回环地址，动态回退由独立字段控制。
    pub preferred_listen_addr: String,
    /// Allows binding a dynamic loopback port when the preferred port is busy.
    /// 首选端口被占用时是否允许绑定动态回环端口。
    pub allow_ephemeral_fallback: bool,
    /// Ephemeral process-local Bearer token.
    /// 进程级临时 Bearer Token。
    pub access_token: String,
    /// Maximum inbound gRPC message size.
    /// 最大入站 gRPC 消息大小。
    pub max_receive_message_bytes: u32,
    /// Per-method request timeout group.
    /// 各方法请求超时组。
    pub request_timeout: ManagedGrpcRequestTimeout,
    /// Graceful gRPC shutdown timeout.
    /// gRPC 优雅关闭超时。
    pub shutdown_timeout: String,
    /// Server keepalive configuration.
    /// 服务端保活配置。
    pub keepalive: ManagedGrpcKeepalive,
}

/// Contains the isolated HTTP management listener, credential, and resource limits.
/// 包含隔离的 HTTP 管理监听器、凭据与资源限制。
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedManagementConfig {
    /// Preferred loopback address, with dynamic fallback controlled separately.
    /// 首选回环地址，动态回退由独立字段控制。
    pub preferred_listen_addr: String,
    /// Allows binding a dynamic loopback port when the preferred port is busy.
    /// 首选端口被占用时是否允许绑定动态回环端口。
    pub allow_ephemeral_fallback: bool,
    /// Ephemeral process-local management Bearer token.
    /// 进程级临时管理 Bearer Token。
    pub access_token: String,
    /// Maximum decoded HTTP request body size.
    /// HTTP 请求体解码前的最大字节数。
    pub max_request_body_bytes: u64,
    /// Default collection page size.
    /// 集合默认分页大小。
    pub default_page_size: u32,
    /// Maximum accepted collection page size.
    /// 集合允许的最大分页大小。
    pub max_page_size: u32,
    /// HTTP request-header read timeout.
    /// HTTP 请求头读取超时。
    pub read_header_timeout: String,
    /// HTTP request processing timeout.
    /// HTTP 请求处理超时。
    pub request_timeout: String,
    /// HTTP idle connection timeout.
    /// HTTP 空闲连接超时。
    pub idle_timeout: String,
    /// Graceful HTTP shutdown timeout.
    /// HTTP 优雅关闭超时。
    pub shutdown_timeout: String,
}

/// Redacts the management Bearer token from every debug representation.
/// 从所有调试表示中脱敏管理 Bearer Token。
impl fmt::Debug for ManagedManagementConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ManagedManagementConfig")
            .field("preferred_listen_addr", &self.preferred_listen_addr)
            .field("allow_ephemeral_fallback", &self.allow_ephemeral_fallback)
            .field("access_token", &"<redacted>")
            .field("max_request_body_bytes", &self.max_request_body_bytes)
            .field("default_page_size", &self.default_page_size)
            .field("max_page_size", &self.max_page_size)
            .field("read_header_timeout", &self.read_header_timeout)
            .field("request_timeout", &self.request_timeout)
            .field("idle_timeout", &self.idle_timeout)
            .field("shutdown_timeout", &self.shutdown_timeout)
            .finish()
    }
}

/// Contains lease and timeout settings for the shared external Controller.
/// 包含共享外部 Controller 的租约与超时设置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedControllerLease {
    /// Existing Controller endpoint.
    /// 已存在的 Controller 端点。
    pub endpoint: String,
    /// Must remain disabled in managed mode.
    /// 托管模式下必须保持禁用。
    pub auto_spawn: bool,
    /// Must remain empty in managed mode.
    /// 托管模式下必须保持为空。
    pub executable: String,
    /// Controller process mode retained for the client configuration.
    /// 为客户端配置保留的 Controller 进程模式。
    pub process_mode: String,
    /// Managed minimum uptime setting.
    /// 托管最短存活时间设置。
    pub minimum_uptime: String,
    /// Managed idle timeout setting.
    /// 托管空闲超时设置。
    pub idle_timeout: String,
    /// Client lease time-to-live.
    /// 客户端租约存活时间。
    pub lease_ttl: String,
    /// Per-attempt connection timeout.
    /// 单次连接超时。
    pub connect_timeout: String,
    /// Overall startup timeout.
    /// 总启动超时。
    pub startup_timeout: String,
    /// Startup retry interval.
    /// 启动重试间隔。
    pub startup_retry_interval: String,
    /// Background lease renewal interval.
    /// 后台租约续期间隔。
    pub lease_renew_interval: String,
    /// Proxied storage request timeout.
    /// 代理存储请求超时。
    pub request_timeout: String,
    /// Stable isolated VMM space identifier.
    /// 稳定隔离的 VMM Space 标识。
    pub space_id: String,
    /// Diagnostic VMM space label.
    /// VMM Space 诊断标签。
    pub space_label: String,
}

/// Binds VMM to one already-running shared Controller.
/// 将 VMM 绑定到一个已运行的共享 Controller。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedStorageConfig {
    /// Must be `controller`.
    /// 必须为 `controller`。
    pub mode: String,
    /// Exact current Controller endpoint.
    /// 当前 Controller 的精确端点。
    pub controller_endpoint: String,
    /// Must remain false.
    /// 必须保持为 false。
    pub controller_auto_spawn: bool,
    /// Isolated VMM Controller space.
    /// 隔离的 VMM Controller Space。
    pub controller_space_id: String,
    /// Human-readable Controller space label.
    /// Controller Space 可读标签。
    pub controller_space_label: String,
    /// Closed Controller client configuration.
    /// 封闭的 Controller 客户端配置。
    pub controller_lease: ManagedControllerLease,
}

/// Selects the protected Vulcan Code inference service.
/// 选择受保护的 Vulcan Code 推理服务。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedInferenceConfig {
    /// Must be `vulcan_code`.
    /// 必须为 `vulcan_code`。
    pub mode: String,
    /// Absolute protected inference discovery path.
    /// 受保护推理发现文件的绝对路径。
    pub discovery_file: String,
    /// Exact caller id bound to the discovery grant.
    /// 发现授权绑定的精确调用方标识。
    pub expected_caller_id: String,
    /// Exact VMM consumer profile.
    /// 精确的 VMM 消费者配置档。
    pub consumer_profile_id: String,
    /// Inference transport timeout.
    /// 推理传输超时。
    pub startup_timeout: String,
    /// Embedding dimension used by the VMM vector space.
    /// VMM 向量空间使用的向量维度。
    pub embedding_dimension: u32,
    /// Maximum outbound inference connections per upstream host.
    /// 每个上游主机允许的最大推理出站连接数。
    pub max_connections_per_host: u32,
}

/// Contains protected VMM logging settings.
/// 包含受保护的 VMM 日志设置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedLoggingConfig {
    /// Runtime log level.
    /// 运行时日志级别。
    pub level: String,
    /// Runtime log format.
    /// 运行时日志格式。
    pub format: String,
    /// Plaintext RPC payload logging flag.
    /// 明文 RPC 载荷日志开关。
    pub debug_rpc_payloads: bool,
    /// LLM output logging flag.
    /// LLM 输出日志开关。
    pub llm_output_enabled: bool,
    /// Protected payload logging flag.
    /// 受保护载荷日志开关。
    pub protect_payloads: bool,
    /// Optional payload encryption key.
    /// 可选载荷加密密钥。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub payload_encryption_key: String,
}

/// Contains the PII rule language selector.
/// 包含 PII 规则语言选择器。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedPiiConfig {
    /// Default PII rule language.
    /// 默认 PII 规则语言。
    pub default_language: String,
}

/// Contains memory-admission noise controls.
/// 包含记忆准入噪声控制。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedNoiseConfig {
    /// Enables noise filtering.
    /// 是否启用噪声过滤。
    pub enabled: bool,
    /// Default noise-rule language.
    /// 默认噪声规则语言。
    pub default_language: String,
    /// Enables semantic noise detection.
    /// 是否启用语义噪声检测。
    pub semantic_enabled: bool,
    /// Semantic rejection threshold.
    /// 语义拒绝阈值。
    pub semantic_threshold: f64,
}

/// Contains the selected managed prompt bundle.
/// 包含选择的托管提示词包。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedPromptConfig {
    /// Prompt bundle language or directory token.
    /// 提示词包语言或目录标识。
    pub prompt_language: String,
}

/// Selects the Controller-backed vector adapter.
/// 选择 Controller 托管的向量适配器。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedVectorConfig {
    /// Vector backend provider.
    /// 向量后端提供方。
    pub provider: String,
}

/// Selects the Controller-backed relational adapter.
/// 选择 Controller 托管的关系适配器。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedRelationalConfig {
    /// Relational backend provider.
    /// 关系后端提供方。
    pub provider: String,
}

/// Contains PreCheck recall controls.
/// 包含 PreCheck 召回控制。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedPreCheckConfig {
    /// Intent extraction timeout.
    /// 意图提取超时。
    pub intent_timeout: String,
    /// Maximum initial recall count.
    /// 初始召回最大数量。
    pub top_k: u32,
    /// Optional search scope.
    /// 可选搜索范围。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub search_scope: String,
    /// Optional similarity threshold.
    /// 可选相似度阈值。
    #[serde(default)]
    pub similarity_threshold: f64,
}

/// Contains asynchronous PostAction extraction controls.
/// 包含异步 PostAction 提炼控制。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedPostActionConfig {
    /// Accepted input validation mode.
    /// 接受的输入校验模式。
    pub input_mode: String,
    /// Turn threshold retained by VMM.
    /// VMM 保留的轮次阈值。
    pub session_analysis_turn_threshold: u32,
    /// Token threshold retained by VMM.
    /// VMM 保留的 Token 阈值。
    pub session_analysis_token_threshold: u32,
    /// Idle analysis timeout.
    /// 空闲分析超时。
    pub session_analysis_idle_timeout: String,
    /// History turns provided to extraction.
    /// 提供给提炼流程的历史轮数。
    pub session_analysis_history_turns: u32,
    /// Maximum extraction input tokens.
    /// 最大提炼输入 Token。
    pub session_analysis_max_input_tokens: u32,
    /// Total deadline for one background turn analysis.
    /// 单条后台 turn 分析的总截止时间。
    pub session_analysis_timeout: String,
    /// Consecutive failure count that moves one turn into Pass.
    /// 将单条 turn 转入 Pass 的连续失败次数阈值。
    pub failure_pass_threshold: u32,
    /// Maximum asynchronous queue workers.
    /// 最大异步队列工作数。
    pub max_queue_workers: u32,
}

/// Contains managed recall pipeline controls.
/// 包含托管召回流水线控制。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedMemoryPipelineConfig {
    /// Maximum generated search keywords.
    /// 最大生成搜索关键词数量。
    pub max_search_keywords: u32,
    /// Optional minimum similarity score.
    /// 可选最小相似度分数。
    pub min_similarity_score: Option<f64>,
    /// Optional replacement similarity score.
    /// 可选替换相似度分数。
    pub replace_min_similarity_score: Option<f64>,
    /// Optional hard deduplication threshold.
    /// 可选硬去重阈值。
    pub hard_dedupe_cosine_threshold: Option<f64>,
    /// Hard deduplication candidate count.
    /// 硬去重候选数量。
    pub hard_dedupe_pool_top_k: u32,
    /// Enables hybrid retrieval.
    /// 是否启用混合检索。
    pub hybrid_enabled: bool,
    /// Lexical recall count.
    /// 词法召回数量。
    pub lexical_top_k: u32,
    /// Reciprocal-rank-fusion constant.
    /// 倒数排名融合常量。
    pub rrf_k: u32,
    /// Enables diversity filtering.
    /// 是否启用多样性过滤。
    pub mmr_enabled: bool,
    /// Diversity balance.
    /// 多样性平衡系数。
    pub mmr_lambda: f64,
    /// Enables Weibull read-time decay.
    /// 是否启用 Weibull 读时衰减。
    pub weibull_enabled: bool,
    /// Weibull shape.
    /// Weibull 形状参数。
    pub weibull_shape: f64,
    /// Weibull scale in hours.
    /// Weibull 小时尺度。
    pub weibull_scale_hours: f64,
    /// Minimum decay multiplier.
    /// 最小衰减倍率。
    pub weibull_min_multiplier: f64,
    /// Reinforcement weight.
    /// 强化权重。
    pub weibull_reinforce_weight: f64,
    /// Cross-session boost.
    /// 跨会话增强系数。
    pub weibull_cross_session_boost: f64,
}

/// Contains cold-data retention controls.
/// 包含冷数据保留控制。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedRetentionConfig {
    /// Enables retention scanning.
    /// 是否启用保留扫描。
    pub enabled: bool,
    /// Recycle scan interval.
    /// 回收扫描间隔。
    pub recycle_scan_interval: String,
    /// Extra turns retained in the hot window.
    /// 热窗口额外保留轮数。
    pub turn_keep_extra_turns: u32,
    /// Idle period before session recycling.
    /// 会话回收前的空闲时间。
    pub session_idle_recycle_after: String,
    /// Trash retention duration.
    /// 回收站保留时长。
    pub trash_retention: String,
    /// Optional protected priority floor.
    /// 可选受保护优先级下限。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub protect_priority_floor: String,
    /// Optional protected memory-level floor.
    /// 可选受保护记忆级别下限。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub protect_memory_level_floor: String,
    /// Skips protected shared memories.
    /// 是否跳过受保护共享记忆。
    pub skip_protected_shared_memories: bool,
}

/// Contains the complete managed runtime snapshot.
/// 包含完整托管运行时快照。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedRuntimeConfig {
    /// Stable VMM data root.
    /// 稳定 VMM 数据根目录。
    pub data_root: String,
    /// Atomic child readiness status path.
    /// 子进程原子就绪状态路径。
    pub status_file: String,
    /// Host-owned graceful shutdown marker path.
    /// 宿主持有的优雅关闭标记路径。
    pub shutdown_file: String,
    /// Managed gRPC configuration.
    /// 托管 gRPC 配置。
    pub grpc: ManagedGrpcConfig,
    /// Managed HTTP administration configuration.
    /// 托管 HTTP 管理配置。
    pub management: ManagedManagementConfig,
    /// Managed storage configuration.
    /// 托管存储配置。
    pub storage: ManagedStorageConfig,
    /// Managed inference configuration.
    /// 托管推理配置。
    pub inference: ManagedInferenceConfig,
    /// Managed logging configuration.
    /// 托管日志配置。
    pub logging: ManagedLoggingConfig,
    /// Managed PII configuration.
    /// 托管 PII 配置。
    pub pii: ManagedPiiConfig,
    /// Managed noise configuration.
    /// 托管噪声配置。
    pub noise: ManagedNoiseConfig,
    /// Managed prompt configuration.
    /// 托管提示词配置。
    pub prompts: ManagedPromptConfig,
    /// Managed vector configuration.
    /// 托管向量配置。
    pub vector: ManagedVectorConfig,
    /// Managed relational configuration.
    /// 托管关系配置。
    pub relational: ManagedRelationalConfig,
    /// Managed PreCheck configuration.
    /// 托管 PreCheck 配置。
    pub pre_check: ManagedPreCheckConfig,
    /// Managed PostAction configuration.
    /// 托管 PostAction 配置。
    pub post_action: ManagedPostActionConfig,
    /// Managed recall pipeline configuration.
    /// 托管召回流水线配置。
    pub memory_pipeline: ManagedMemoryPipelineConfig,
    /// Managed retention configuration.
    /// 托管保留配置。
    pub retention: ManagedRetentionConfig,
}

/// Is the complete single-document child startup contract.
/// 完整的单文档子进程启动契约。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedConfig {
    /// Exact contract version.
    /// 精确契约版本。
    pub contract_version: u32,
    /// Exact packaged runtime build identity.
    /// 精确的打包运行时构建身份。
    pub runtime_build_id: String,
    /// Authoritative owner.
    /// 权威所有者。
    pub owner: String,
    /// Random runtime instance identifier.
    /// 随机运行时实例标识。
    pub instance_id: String,
    /// Monotonic process-local generation.
    /// 进程内单调代次。
    pub generation: u64,
    /// Exact parent identity.
    /// 精确父进程身份。
    pub parent: ManagedParent,
    /// Explicit asset roots.
    /// 显式资产根目录。
    pub assets: ManagedAssets,
    /// Complete runtime snapshot.
    /// 完整运行时快照。
    pub runtime: ManagedRuntimeConfig,
}

/// Mirrors the authenticated child readiness document.
/// 镜像经过鉴权的子进程就绪文档。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedStatus {
    /// Exact managed contract version.
    /// 精确托管契约版本。
    pub contract_version: u32,
    /// Exact packaged runtime build identity copied from the manifest.
    /// 从清单复制的精确打包运行时构建身份。
    pub runtime_build_id: String,
    /// Instance identifier copied from the manifest.
    /// 从清单复制的实例标识。
    pub instance_id: String,
    /// Generation copied from the manifest.
    /// 从清单复制的代次。
    pub generation: u64,
    /// Child process identifier.
    /// 子进程标识。
    pub process_id: u32,
    /// Parent process identifier.
    /// 父进程标识。
    pub parent_process_id: u32,
    /// SHA-256 digest of the exact manifest bytes.
    /// 精确清单字节的 SHA-256 摘要。
    pub manifest_digest: String,
    /// Actual loopback gRPC address.
    /// 实际回环 gRPC 地址。
    pub grpc_listen_addr: String,
    /// gRPC protocol version.
    /// gRPC 协议版本。
    pub grpc_proto_version: u32,
    /// Actual loopback HTTP management address.
    /// 实际回环 HTTP 管理地址。
    pub management_listen_addr: String,
    /// Management HTTP protocol version.
    /// 管理 HTTP 协议版本。
    pub management_protocol_version: u32,
    /// Indicates that every required management listener finished binding.
    /// 表示全部必需管理监听器已经完成绑定。
    pub management_ready: bool,
    /// Vulcan inference protocol version.
    /// Vulcan 推理协议版本。
    pub inference_protocol_version: u32,
}

#[cfg(test)]
mod tests {
    use super::ManagedManagementConfig;

    /// Verifies debug formatting retains diagnostics while never exposing the management token.
    /// 验证调试格式保留诊断字段但绝不暴露管理 Token。
    #[test]
    fn management_debug_redacts_access_token() {
        let config = ManagedManagementConfig {
            preferred_listen_addr: String::from("127.0.0.1:0"),
            allow_ephemeral_fallback: true,
            access_token: String::from("management-secret"),
            max_request_body_bytes: 1 << 20,
            default_page_size: 30,
            max_page_size: 100,
            read_header_timeout: String::from("5s"),
            request_timeout: String::from("30s"),
            idle_timeout: String::from("1m"),
            shutdown_timeout: String::from("10s"),
        };

        let debug = format!("{config:?}");
        assert!(debug.contains("<redacted>"));
        assert!(debug.contains("127.0.0.1:0"));
        assert!(!debug.contains("management-secret"));
    }
}
