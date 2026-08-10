//! Bounded non-secret operational telemetry shared by provider runtime surfaces.
//! 供应商运行时各界面共享的有界非秘密运行遥测。

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// Maximum number of operational events retained in the process-wide diagnostic history.
/// 进程级诊断历史保留的运行事件最大数量。
pub const PROVIDER_OPERATIONAL_TELEMETRY_MAX_EVENTS: usize = 256;
/// Maximum UTF-8 byte length accepted for one non-secret identity field.
/// 单个非秘密身份字段允许的最大 UTF-8 字节数。
const PROVIDER_OPERATIONAL_TELEMETRY_FIELD_MAX_BYTES: usize = 512;
/// Maximum number of profile revisions retained in one event.
/// 单条事件保留的 Profile 修订最大数量。
const PROVIDER_OPERATIONAL_TELEMETRY_PROFILE_MAX_ITEMS: usize = 32;

/// Closed provider-operational event names used by logs and bounded status snapshots.
/// 日志与有界状态快照使用的封闭供应商运行事件名称。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderOperationalTelemetryEventName {
    /// A signed manifest-index check started.
    /// 签名 Manifest Index 检查已开始。
    ManifestIndexCheckStarted,
    /// A conditional manifest-index request returned not modified.
    /// 条件式 Manifest Index 请求返回未修改。
    ManifestIndexCheckNotModified,
    /// A manifest-index check failed before a provider candidate could complete.
    /// Manifest Index 检查在供应商候选完成前失败。
    ManifestIndexCheckFailed,
    /// A manifest-index check completed for one provider.
    /// 一个供应商的 Manifest Index 检查已完成。
    ManifestIndexCheckCompleted,
    /// A provider candidate was downloaded under an authenticated index pin.
    /// 供应商候选已按认证 Index 固定项下载。
    ProviderCandidateDownloaded,
    /// A provider candidate was rejected by a closed verification category.
    /// 供应商候选因封闭校验分类被拒绝。
    ProviderCandidateRejected,
    /// A verified provider candidate entered the uncommitted activation transaction.
    /// 已校验供应商候选进入尚未提交的激活事务。
    ProviderCandidateStaged,
    /// A provider manifest and all derived projections committed successfully.
    /// 供应商 Manifest 及全部派生投影已成功提交。
    ProviderManifestActivated,
    /// A post-activation failure restored every captured before-image.
    /// 激活后失败已恢复全部捕获的前镜像。
    ProviderManifestPostActivationRolledBack,
    /// Startup or explicit recovery selected a trusted previous or bundled layer.
    /// 启动或显式恢复选择了可信 previous 或 bundled 层。
    ProviderManifestFallbackSelected,
    /// Startup isolated one provider because no trusted executable manifest layer remained.
    /// 启动因没有剩余可信可执行 Manifest 层而隔离了一个供应商。
    ProviderManifestIsolated,
    /// One exact provider upload or inline materialization started.
    /// 一次精确供应商上传或内联物化已开始。
    ProviderUploadStarted,
    /// One exact provider upload or inline materialization completed.
    /// 一次精确供应商上传或内联物化已完成。
    ProviderUploadCompleted,
    /// One exact provider upload or inline materialization failed.
    /// 一次精确供应商上传或内联物化失败。
    ProviderUploadFailed,
    /// One exact provider upload or inline materialization was cancelled.
    /// 一次精确供应商上传或内联物化被取消。
    ProviderUploadCancelled,
    /// A durable asynchronous provider task entered a distinct lifecycle phase.
    /// 一个持久异步供应商任务进入不同生命周期阶段。
    ProviderAsyncTaskStateChanged,
}

impl ProviderOperationalTelemetryEventName {
    /// Returns the stable serialized event name used by invariant checks.
    /// 返回不变量检查使用的稳定序列化事件名称。
    ///
    /// # Returns
    /// 返回值
    /// One lowercase snake-case event identifier.
    /// 返回一个小写蛇形事件标识。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ManifestIndexCheckStarted => "manifest_index_check_started",
            Self::ManifestIndexCheckNotModified => "manifest_index_check_not_modified",
            Self::ManifestIndexCheckFailed => "manifest_index_check_failed",
            Self::ManifestIndexCheckCompleted => "manifest_index_check_completed",
            Self::ProviderCandidateDownloaded => "provider_candidate_downloaded",
            Self::ProviderCandidateRejected => "provider_candidate_rejected",
            Self::ProviderCandidateStaged => "provider_candidate_staged",
            Self::ProviderManifestActivated => "provider_manifest_activated",
            Self::ProviderManifestPostActivationRolledBack => {
                "provider_manifest_post_activation_rolled_back"
            }
            Self::ProviderManifestFallbackSelected => "provider_manifest_fallback_selected",
            Self::ProviderManifestIsolated => "provider_manifest_isolated",
            Self::ProviderUploadStarted => "provider_upload_started",
            Self::ProviderUploadCompleted => "provider_upload_completed",
            Self::ProviderUploadFailed => "provider_upload_failed",
            Self::ProviderUploadCancelled => "provider_upload_cancelled",
            Self::ProviderAsyncTaskStateChanged => "provider_async_task_state_changed",
        }
    }
}

/// One immutable profile identity retained without request or response bodies.
/// 一条不保留请求或响应正文的不可变 Profile 身份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderOperationalProfileRevision {
    /// Stable profile identifier.
    /// 稳定 Profile 标识。
    pub profile_id: String,
    /// Immutable profile revision.
    /// 不可变 Profile 修订号。
    pub revision: u64,
}

/// One versioned, non-secret provider operational event.
/// 一条版本化、非秘密的供应商运行事件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderOperationalTelemetryEvent {
    /// Exact event schema version.
    /// 精确事件 Schema 版本。
    pub schema_version: u32,
    /// Host-generated unique event identity.
    /// 宿主生成的唯一事件身份。
    pub event_id: String,
    /// Closed event name.
    /// 封闭事件名称。
    pub event_name: ProviderOperationalTelemetryEventName,
    /// Unix milliseconds captured when the event was created.
    /// 事件创建时捕获的 Unix 毫秒时间。
    pub occurred_at_unix_ms: u64,
    /// Exact provider identifier when the event is provider-scoped.
    /// 事件按供应商限定时的精确供应商标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    /// Exact service-entry identifier when execution has selected one.
    /// 执行已选择服务入口时的精确入口标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_entry_id: Option<String>,
    /// Exact credential connection identifier without credential material.
    /// 不含凭据材料的精确凭据连接标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
    /// Exact upstream model identifier when one owns the operation.
    /// 存在操作所属模型时的精确上游模型标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    /// Exact provider-neutral protocol profile identifier.
    /// 精确供应商无关协议 Profile 标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol_profile_id: Option<String>,
    /// Content-addressed complete route revision.
    /// 内容寻址的完整路由修订号。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route_revision: Option<String>,
    /// Manifest revision frozen for the event.
    /// 事件冻结的 Manifest 修订号。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_revision: Option<u64>,
    /// Runtime manifest generation frozen for the event.
    /// 事件冻结的运行时 Manifest 代际。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_generation: Option<u64>,
    /// Authenticated lowercase SHA-256 without a source URL.
    /// 不含源 URL 的认证小写 SHA-256。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_sha256: Option<String>,
    /// Closed active source layer such as `active`, `previous`, or `bundled`.
    /// 封闭活动来源层，例如 `active`、`previous` 或 `bundled`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_source: Option<String>,
    /// Stable model protocol binding source.
    /// 稳定模型协议绑定来源。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding_source: Option<String>,
    /// Exact model rule that produced the binding.
    /// 产生绑定的精确模型规则。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding_rule_id: Option<String>,
    /// Ordered frozen request profile identities.
    /// 有序冻结请求 Profile 身份。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub request_profiles: Vec<ProviderOperationalProfileRevision>,
    /// Ordered frozen response profile identities.
    /// 有序冻结响应 Profile 身份。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub response_profiles: Vec<ProviderOperationalProfileRevision>,
    /// Exact provider-neutral operation identifier.
    /// 精确供应商无关 operation 标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    /// Exact manifest-declared upload channel, or the closed host inline channel identity.
    /// 精确 Manifest 声明上传通道，或封闭宿主内联通道身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upload_channel_id: Option<String>,
    /// Exact provider-tool target identity when one owns the operation.
    /// 存在操作所属供应商工具目标时的精确目标身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_target_id: Option<String>,
    /// Stable host identity of one durable provider task.
    /// 一个持久供应商任务的稳定宿主身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_execution_id: Option<String>,
    /// Closed serialized asynchronous task phase.
    /// 封闭序列化异步任务阶段。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_phase: Option<String>,
    /// Stable typed failure category without an upstream response body.
    /// 不含上游响应正文的稳定类型化失败分类。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_kind: Option<String>,
    /// Controlled JSON/schema path when verification identified one.
    /// 校验确定字段时的受控 JSON/Schema 路径。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_path: Option<String>,
}

impl ProviderOperationalTelemetryEvent {
    /// Creates one empty event envelope with host-owned identity and time.
    /// 使用宿主持有的身份与时间创建一个空事件信封。
    ///
    /// # Parameters
    /// 参数
    /// `event_name` is one closed provider-operational lifecycle point.
    /// `event_name` 是一个封闭供应商运行生命周期点。
    ///
    /// # Returns
    /// 返回值
    /// A version-one envelope whose domain fields must be populated from verified facts.
    /// 返回版本一信封，其领域字段必须从已校验事实填充。
    pub fn new(event_name: ProviderOperationalTelemetryEventName) -> Self {
        Self {
            schema_version: 1,
            event_id: Uuid::new_v4().to_string(),
            event_name,
            occurred_at_unix_ms: current_unix_millis(),
            provider_id: None,
            service_entry_id: None,
            connection_id: None,
            model_id: None,
            protocol_profile_id: None,
            route_revision: None,
            manifest_revision: None,
            manifest_generation: None,
            manifest_sha256: None,
            manifest_source: None,
            binding_source: None,
            binding_rule_id: None,
            request_profiles: Vec::new(),
            response_profiles: Vec::new(),
            operation_id: None,
            upload_channel_id: None,
            tool_target_id: None,
            task_execution_id: None,
            task_phase: None,
            failure_kind: None,
            failure_path: None,
        }
    }

    /// Validates event-specific identity closure and secret-safe bounded fields.
    /// 校验事件专属身份闭包及秘密安全的有界字段。
    ///
    /// # Returns
    /// 返回值
    /// Success for one publishable event, otherwise an explicit internal contract failure.
    /// 可发布事件返回成功，否则返回明确内部契约失败。
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err(format!(
                "Provider operational telemetry schema version `{}` is unsupported.",
                self.schema_version
            ));
        }
        validate_safe_identity("event_id", &self.event_id)?;
        for (field, value) in [
            ("provider_id", self.provider_id.as_deref()),
            ("service_entry_id", self.service_entry_id.as_deref()),
            ("connection_id", self.connection_id.as_deref()),
            ("model_id", self.model_id.as_deref()),
            ("protocol_profile_id", self.protocol_profile_id.as_deref()),
            ("route_revision", self.route_revision.as_deref()),
            ("manifest_source", self.manifest_source.as_deref()),
            ("binding_source", self.binding_source.as_deref()),
            ("binding_rule_id", self.binding_rule_id.as_deref()),
            ("operation_id", self.operation_id.as_deref()),
            ("upload_channel_id", self.upload_channel_id.as_deref()),
            ("tool_target_id", self.tool_target_id.as_deref()),
            ("task_execution_id", self.task_execution_id.as_deref()),
            ("task_phase", self.task_phase.as_deref()),
            ("failure_kind", self.failure_kind.as_deref()),
            ("failure_path", self.failure_path.as_deref()),
        ] {
            if let Some(value) = value {
                validate_safe_identity(field, value)?;
            }
        }
        if self.request_profiles.len() > PROVIDER_OPERATIONAL_TELEMETRY_PROFILE_MAX_ITEMS
            || self.response_profiles.len() > PROVIDER_OPERATIONAL_TELEMETRY_PROFILE_MAX_ITEMS
        {
            return Err(String::from(
                "Provider operational telemetry profile history exceeds its 32-item bound.",
            ));
        }
        for profile in self
            .request_profiles
            .iter()
            .chain(self.response_profiles.iter())
        {
            validate_safe_identity("profile_id", &profile.profile_id)?;
        }
        if let Some(sha256) = self.manifest_sha256.as_deref()
            && (sha256.len() != 64
                || !sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
        {
            return Err(String::from(
                "Provider operational telemetry manifest SHA-256 must be lowercase hexadecimal.",
            ));
        }
        if let Some(path) = self.failure_path.as_deref()
            && !(path.starts_with('/') || path.starts_with('$'))
        {
            return Err(String::from(
                "Provider operational telemetry failure_path must be an absolute JSON Pointer or root-based schema path.",
            ));
        }
        self.validate_required_fields()
    }

    /// Validates the required identity tuple for the closed event name.
    /// 校验封闭事件名称要求的身份元组。
    ///
    /// # Returns
    /// 返回值
    /// Success when the event cannot be mistaken for another lifecycle or route.
    /// 事件不会被误认为其他生命周期或路由时返回成功。
    fn validate_required_fields(&self) -> Result<(), String> {
        let provider_required = !matches!(
            self.event_name,
            ProviderOperationalTelemetryEventName::ManifestIndexCheckStarted
                | ProviderOperationalTelemetryEventName::ManifestIndexCheckFailed
                | ProviderOperationalTelemetryEventName::ManifestIndexCheckCompleted
        );
        if provider_required && self.provider_id.is_none() {
            return Err(format!(
                "Provider operational event `{}` requires provider_id.",
                self.event_name.as_str()
            ));
        }
        match self.event_name {
            ProviderOperationalTelemetryEventName::ProviderCandidateDownloaded
            | ProviderOperationalTelemetryEventName::ProviderCandidateStaged => {
                if self.manifest_revision.is_none() || self.manifest_sha256.is_none() {
                    return Err(format!(
                        "Provider candidate event `{}` requires revision and SHA-256.",
                        self.event_name.as_str()
                    ));
                }
                if self.failure_kind.is_some() || self.failure_path.is_some() {
                    return Err(format!(
                        "Provider candidate event `{}` cannot carry rejection facts.",
                        self.event_name.as_str()
                    ));
                }
            }
            ProviderOperationalTelemetryEventName::ProviderCandidateRejected => {
                if self.manifest_revision.is_none()
                    || self.manifest_sha256.is_none()
                    || self.failure_kind.is_none()
                {
                    return Err(String::from(
                        "Provider candidate rejection telemetry requires revision, SHA-256, and failure_kind.",
                    ));
                }
            }
            ProviderOperationalTelemetryEventName::ProviderManifestActivated
            | ProviderOperationalTelemetryEventName::ProviderManifestFallbackSelected => {
                if self.manifest_revision.is_none() || self.manifest_source.is_none() {
                    return Err(format!(
                        "Provider manifest event `{}` requires revision and source.",
                        self.event_name.as_str()
                    ));
                }
            }
            ProviderOperationalTelemetryEventName::ProviderManifestPostActivationRolledBack => {
                if self.manifest_revision.is_none() || self.failure_kind.is_none() {
                    return Err(String::from(
                        "Provider manifest rollback telemetry requires revision and failure_kind.",
                    ));
                }
            }
            ProviderOperationalTelemetryEventName::ProviderManifestIsolated => {
                if self.manifest_generation.is_none()
                    || self.manifest_source.as_deref() != Some("isolated")
                    || self.failure_kind.is_none()
                {
                    return Err(String::from(
                        "Provider manifest isolation telemetry requires generation, isolated source, and failure_kind.",
                    ));
                }
            }
            ProviderOperationalTelemetryEventName::ProviderUploadStarted
            | ProviderOperationalTelemetryEventName::ProviderUploadCompleted
            | ProviderOperationalTelemetryEventName::ProviderUploadFailed
            | ProviderOperationalTelemetryEventName::ProviderUploadCancelled => {
                if self.connection_id.is_none()
                    || self.model_id.is_none()
                    || self.protocol_profile_id.is_none()
                    || self.route_revision.is_none()
                    || self.manifest_revision.is_none()
                    || self.manifest_generation.is_none()
                    || self.operation_id.is_none()
                    || self.upload_channel_id.is_none()
                {
                    return Err(format!(
                        "Provider upload event `{}` requires the complete non-secret route and operation identity.",
                        self.event_name.as_str()
                    ));
                }
            }
            ProviderOperationalTelemetryEventName::ProviderAsyncTaskStateChanged => {
                if self.connection_id.is_none()
                    || self.manifest_revision.is_none()
                    || self.operation_id.is_none()
                    || self.task_execution_id.is_none()
                    || self.task_phase.is_none()
                {
                    return Err(String::from(
                        "Provider asynchronous task telemetry requires frozen route, operation, execution, and phase identities.",
                    ));
                }
            }
            ProviderOperationalTelemetryEventName::ManifestIndexCheckFailed => {
                if self.failure_kind.is_none() {
                    return Err(String::from(
                        "Manifest index failure telemetry requires failure_kind.",
                    ));
                }
            }
            ProviderOperationalTelemetryEventName::ManifestIndexCheckStarted
            | ProviderOperationalTelemetryEventName::ManifestIndexCheckNotModified
            | ProviderOperationalTelemetryEventName::ManifestIndexCheckCompleted => {}
        }
        Ok(())
    }
}

/// Records one validated event in a bounded history and writes one structured JSON log line.
/// 在有界历史中记录一条已校验事件，并写入一行结构化 JSON 日志。
///
/// # Parameters
/// 参数
/// `event` contains only closed non-secret fields copied from authoritative runtime facts.
/// `event` 仅包含从权威运行时事实复制的封闭非秘密字段。
///
/// # Returns
/// 返回值
/// Success after the event is retained and serialized, otherwise an explicit contract failure.
/// 事件保留并序列化后返回成功，否则返回明确契约失败。
pub fn record_provider_operational_telemetry(
    event: ProviderOperationalTelemetryEvent,
) -> Result<(), String> {
    event.validate()?;
    let serialized = serde_json::to_string(&event)
        .map_err(|error| format!("Provider operational telemetry serialization failed: {error}"))?;
    {
        let mut history = provider_operational_telemetry_history()
            .lock()
            .map_err(|_| {
                String::from("Provider operational telemetry history lock is poisoned.")
            })?;
        history.push_back(event);
        while history.len() > PROVIDER_OPERATIONAL_TELEMETRY_MAX_EVENTS {
            history.pop_front();
        }
    }
    eprintln!("[provider-operational-telemetry] {serialized}");
    Ok(())
}

/// Returns newest-first bounded operational events for one exact provider.
/// 返回一个精确供应商按新到旧排列的有界运行事件。
///
/// # Parameters
/// 参数
/// `provider_id` is the exact provider filter and `maximum_items` is capped by the global bound.
/// `provider_id` 是精确供应商过滤条件，`maximum_items` 受全局上限约束。
///
/// # Returns
/// 返回值
/// A cloned, non-secret event snapshot suitable for status serialization.
/// 返回适合状态序列化的克隆非秘密事件快照。
pub fn provider_operational_telemetry_snapshot(
    provider_id: &str,
    maximum_items: usize,
) -> Result<Vec<ProviderOperationalTelemetryEvent>, String> {
    let maximum_items = maximum_items.min(PROVIDER_OPERATIONAL_TELEMETRY_MAX_EVENTS);
    Ok(provider_operational_telemetry_history()
        .lock()
        .map_err(|_| String::from("Provider operational telemetry history lock is poisoned."))?
        .iter()
        .rev()
        .filter(|event| event.provider_id.as_deref() == Some(provider_id))
        .take(maximum_items)
        .cloned()
        .collect())
}

/// Returns the process-wide bounded telemetry ring.
/// 返回进程级有界遥测环。
fn provider_operational_telemetry_history()
-> &'static Mutex<VecDeque<ProviderOperationalTelemetryEvent>> {
    static HISTORY: OnceLock<Mutex<VecDeque<ProviderOperationalTelemetryEvent>>> = OnceLock::new();
    HISTORY.get_or_init(|| Mutex::new(VecDeque::new()))
}

/// Rejects unbounded, multiline, URL-shaped, or credential-shaped telemetry fields.
/// 拒绝无界、多行、URL 形态或凭据形态的遥测字段。
///
/// # Parameters
/// 参数
/// `field` identifies the internal contract and `value` is the exact candidate text.
/// `field` 标识内部契约，`value` 是精确候选文本。
///
/// # Returns
/// 返回值
/// Success only when the value is non-empty, bounded, single-line, and secret-safe.
/// 仅当值非空、有界、单行且秘密安全时返回成功。
fn validate_safe_identity(field: &str, value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > PROVIDER_OPERATIONAL_TELEMETRY_FIELD_MAX_BYTES {
        return Err(format!(
            "Provider operational telemetry field `{field}` must contain 1..={PROVIDER_OPERATIONAL_TELEMETRY_FIELD_MAX_BYTES} UTF-8 bytes."
        ));
    }
    if value.chars().any(char::is_control) {
        return Err(format!(
            "Provider operational telemetry field `{field}` must be single-line printable text."
        ));
    }
    let lowercase = value.to_ascii_lowercase();
    let trimmed = value.trim();
    let lowercase_trimmed = lowercase.trim();
    if lowercase.contains("://")
        || lowercase.contains("authorization")
        || lowercase.contains("x-api-key")
        || lowercase.contains("api-key")
        || lowercase.contains("api_key=")
        || lowercase.contains("apikey=")
        || lowercase.contains("secret=")
        || lowercase.contains("token=")
        || lowercase_trimmed.starts_with("bearer ")
        || lowercase_trimmed.starts_with("basic ")
        || lowercase_trimmed.starts_with("sk-")
        || lowercase_trimmed.starts_with("sk_")
        || lowercase_trimmed.starts_with("xoxb-")
        || lowercase_trimmed.starts_with("xoxp-")
        || lowercase_trimmed.starts_with("ghp_")
        || lowercase_trimmed.starts_with("github_pat_")
        || lowercase_trimmed.starts_with("akia")
        || lowercase_trimmed.starts_with("aiza")
        || trimmed.starts_with('{')
        || trimmed.starts_with('[')
        || (field != "failure_path"
            && (trimmed.starts_with('/')
                || trimmed.starts_with("\\\\")
                || (trimmed.len() >= 3
                    && trimmed.as_bytes()[1] == b':'
                    && matches!(trimmed.as_bytes()[2], b'\\' | b'/'))))
    {
        return Err(format!(
            "Provider operational telemetry field `{field}` contains a forbidden URL, credential, body, or local-path shape."
        ));
    }
    Ok(())
}

/// Returns a monotonic-enough wall-clock timestamp for diagnostic ordering.
/// 返回足以用于诊断排序的墙钟时间戳。
///
/// # Returns
/// 返回值
/// Unix milliseconds, saturating to zero only before the Unix epoch.
/// 返回 Unix 毫秒；仅在 Unix 纪元前饱和为零。
fn current_unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies complete upload identity is required and credential-shaped fields are rejected.
    /// 验证上传必须具备完整身份，且凭据形态字段会被拒绝。
    #[test]
    fn upload_event_requires_complete_secret_safe_identity() {
        let mut event = ProviderOperationalTelemetryEvent::new(
            ProviderOperationalTelemetryEventName::ProviderUploadStarted,
        );
        event.provider_id = Some(String::from("provider-a"));
        event.service_entry_id = Some(String::from("public-api"));
        event.connection_id = Some(String::from("connection-a"));
        event.model_id = Some(String::from("model-a"));
        event.protocol_profile_id = Some(String::from("openai.responses"));
        event.route_revision = Some(String::from("sha256:route-a"));
        event.manifest_revision = Some(7);
        event.manifest_generation = Some(11);
        event.operation_id = Some(String::from("conversation.media_input"));
        event.upload_channel_id = Some(String::from("host.inline_payload"));
        event.validate().expect("complete event should validate");

        event.upload_channel_id = Some(String::from("https://secret.example/upload?token=x"));
        let error = event
            .validate()
            .expect_err("full signed URLs must not enter telemetry");
        assert!(error.contains("forbidden URL, credential, body, or local-path shape"));
    }

    /// Verifies raw authorization values, API keys, request bodies, and local paths are rejected.
    /// 验证原始授权值、API Key、请求正文与本地路径均被拒绝。
    #[test]
    fn secret_body_and_local_path_shapes_never_enter_operational_history() {
        for forbidden in [
            "Bearer opaque-credential",
            "sk-live-credential",
            "{\"prompt\":\"private local body\"}",
            r"C:\private\candidate.json",
            "/private/candidate.json",
        ] {
            let error = validate_safe_identity("model_id", forbidden)
                .expect_err("sensitive shape must be rejected");
            assert!(
                error.contains("forbidden URL, credential, body, or local-path shape"),
                "unexpected error for `{forbidden}`: {error}"
            );
        }
        validate_safe_identity("failure_path", "/properties/token_count")
            .expect("absolute JSON paths remain permitted only in failure_path");
    }

    /// Verifies candidate events require authenticated revision and digest facts.
    /// 验证候选事件必须包含认证修订与摘要事实。
    #[test]
    fn candidate_event_requires_authenticated_identity() {
        let mut event = ProviderOperationalTelemetryEvent::new(
            ProviderOperationalTelemetryEventName::ProviderCandidateRejected,
        );
        event.provider_id = Some(String::from("provider-a"));
        event.manifest_revision = Some(9);
        let error = event
            .validate()
            .expect_err("candidate rejection without digest and kind must be rejected");
        assert!(error.contains("requires revision, SHA-256, and failure_kind"));

        event.manifest_sha256 = Some("a".repeat(64));
        event.failure_kind = Some(String::from("schema_semantic_failed"));
        event.failure_path = Some(String::from("$.model_rules[0].selected_protocol"));
        event
            .validate()
            .expect("authenticated candidate with compiler path should validate");
    }

    /// Verifies provider isolation is observable only with a closed generation and reason.
    /// 验证供应商隔离仅在具备封闭代际与原因时可观测。
    #[test]
    fn manifest_isolation_event_requires_closed_provider_local_identity() {
        let mut event = ProviderOperationalTelemetryEvent::new(
            ProviderOperationalTelemetryEventName::ProviderManifestIsolated,
        );
        event.provider_id = Some(String::from("deepseek"));
        event.manifest_generation = Some(9);
        event.manifest_source = Some(String::from("isolated"));
        let error = event
            .validate()
            .expect_err("isolation without a failure kind must be rejected");
        assert!(error.contains("requires generation, isolated source, and failure_kind"));

        event.failure_kind = Some(String::from("bundled_baseline_invalid"));
        event
            .validate()
            .expect("complete provider-local isolation event should validate");
    }
}
