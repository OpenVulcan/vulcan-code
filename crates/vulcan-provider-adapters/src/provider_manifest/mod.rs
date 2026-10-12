//! Declarative provider capability manifest: schema, validation, and engine contracts.
//! 声明式供应商能力清单：Schema、验证与引擎契约。

pub mod binding;
pub mod compiler;
pub mod custom_projection;
pub mod inheritance;
pub mod matcher;
pub mod provenance;
pub mod schema;
pub mod scope;
pub mod validation;

#[cfg(test)]
mod tests;

pub use binding::*;
pub use compiler::*;
pub use custom_projection::*;
pub use inheritance::*;
pub use matcher::*;
pub use provenance::*;
pub use schema::*;
pub use scope::*;
pub use validation::*;

/// Maximum UTF-8 bytes accepted for one manifest document.
/// 单份清单文档允许的最大 UTF-8 字节数。
///
/// Single source of truth for every manifest byte bound: the packaged manifest read bound, the
/// store's candidate and custom-provider file bounds, and the provider-manifest CLI file read
/// bound all derive from this constant. Change the manifest byte policy here only.
/// 所有清单字节上限的唯一来源：随包清单读取上限、存储的候选与自定义供应商文件上限、以及
/// provider-manifest CLI 的文件读取上限均由它派生。清单字节策略只在此处修改。
pub const MAX_MANIFEST_BYTES: usize = 4 * 1024 * 1024;

/// Maximum service entries declared by one manifest.
/// 单份清单声明的最大服务入口数量。
pub const MAX_SERVICES: usize = 64;

/// Maximum model rules declared by one manifest.
/// 单份清单声明的最大模型规则数量。
pub const MAX_MODEL_RULES: usize = 4_096;

/// Maximum protocol routes declared by one service entry.
/// 单个服务入口声明的最大协议路由数量。
pub const MAX_PROTOCOL_ROUTES: usize = 16;

/// Maximum UTF-8 characters accepted for one identifier or display string.
/// 单个标识或展示字符串允许的最大 UTF-8 字符数。
pub const MAX_STRING_LEN: usize = 2_048;

/// Maximum ASCII characters accepted for one stable provider identifier.
/// 单个稳定供应商标识允许的最大 ASCII 字符数。
///
/// The bound keeps every encoded Active, Previous, staging, quarantine, transaction, and custom
/// Manifest filename below the 255-byte cross-platform file-component ceiling, including the
/// longest closed failure suffix used by the store.
/// 该上限确保编码后的 Active、Previous、staging、quarantine、transaction 与自定义 Manifest
/// 文件名（包括 Store 使用的最长封闭失败后缀）均低于跨平台 255 字节文件组件上限。
pub const MAX_PROVIDER_IDENTIFIER_LEN: usize = 64;

/// Maximum UTF-8 characters accepted for one stable rule matcher value.
/// 单个稳定规则匹配值允许的最大 UTF-8 字符数。
pub const MAX_MATCHER_VALUE_LEN: usize = 512;

/// Maximum JSON nesting depth accepted by the manifest validator.
/// Manifest 验证器接受的最大 JSON 嵌套深度。
pub const MAX_JSON_DEPTH: usize = 24;

/// Maximum total JSON nodes accepted by the manifest validator.
/// Manifest 验证器接受的最大 JSON 节点总数。
pub const MAX_JSON_NODES: usize = 200_000;

/// Validates one runtime JSON value against the shared depth and node-count ceilings.
/// 根据共享深度与节点数量上限校验一个运行时 JSON 值。
///
/// # Parameters
/// 参数
/// `value` is the already byte-bounded JSON tree and `subject` scopes the controlled diagnostic.
/// `value` 是已经过字节限制的 JSON 树，`subject` 限定受控诊断范围。
///
/// # Returns
/// 返回值
/// Success only when the complete tree fits both shared bounds.
/// 仅当完整树同时符合两项共享上限时返回成功。
pub fn validate_runtime_json_bounds(
    value: &serde_json::Value,
    subject: &str,
) -> Result<(), String> {
    let mut node_count = 0_usize;
    validate_runtime_json_bounds_inner(value, subject, 0, &mut node_count)
}

/// Recursively validates one runtime JSON subtree while sharing the document-wide node counter.
/// 递归校验一个运行时 JSON 子树，并共享整份文档的节点计数器。
///
/// # Parameters
/// 参数
/// `value` is the current subtree, `subject` scopes diagnostics, `depth` is the current nesting
/// level, and `node_count` accumulates every sibling and descendant.
/// `value` 是当前子树，`subject` 限定诊断范围，`depth` 是当前嵌套层级，
/// `node_count` 累计全部兄弟节点与后代节点。
///
/// # Returns
/// 返回值
/// Success for the bounded subtree or the first deterministic depth/count failure.
/// 子树有界时返回成功，否则返回首个确定性的深度或数量失败。
fn validate_runtime_json_bounds_inner(
    value: &serde_json::Value,
    subject: &str,
    depth: usize,
    node_count: &mut usize,
) -> Result<(), String> {
    if depth > MAX_JSON_DEPTH {
        return Err(format!(
            "{subject} exceeds the {MAX_JSON_DEPTH} level JSON depth limit."
        ));
    }
    *node_count = node_count
        .checked_add(1)
        .ok_or_else(|| format!("{subject} JSON node count overflowed."))?;
    if *node_count > MAX_JSON_NODES {
        return Err(format!(
            "{subject} exceeds the {MAX_JSON_NODES} JSON node limit."
        ));
    }
    match value {
        serde_json::Value::Object(object) => {
            for child in object.values() {
                validate_runtime_json_bounds_inner(child, subject, depth + 1, node_count)?;
            }
        }
        serde_json::Value::Array(items) => {
            for child in items {
                validate_runtime_json_bounds_inner(child, subject, depth + 1, node_count)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Maximum JSON Pointer segments accepted for one declared field path.
/// 单个声明字段路径接受的最大 JSON Pointer 段数。
pub const MAX_POINTER_DEPTH: usize = 16;

/// Maximum distinct MIME types accepted for one operation resource binding.
/// 单个操作资源绑定接受的最大不同 MIME 类型数量。
pub const MAX_MIME_TYPES: usize = 64;

/// Maximum response-header names tracked as provider request identifiers.
/// 作为供应商请求标识跟踪的最大响应头名称数量。
pub const MAX_REQUEST_ID_HEADERS: usize = 8;

/// Minimum manifest-declared asynchronous poll interval in milliseconds.
/// Manifest 声明的异步轮询间隔最小毫秒数。
pub const MIN_ASYNC_POLL_INTERVAL_MS: u64 = 250;

/// Maximum manifest-declared asynchronous poll or backoff interval in milliseconds.
/// Manifest 声明的异步轮询或退避间隔最大毫秒数。
pub const MAX_ASYNC_POLL_OR_BACKOFF_MS: u64 = 300_000;

/// Maximum manifest-declared asynchronous lifecycle timeout in milliseconds.
/// Manifest 声明的异步生命周期最大超时毫秒数。
pub const MAX_ASYNC_TIMEOUT_MS: u64 = 86_400_000;

/// Maximum fixed request headers declared for one service entry or protocol route.
/// 单个服务入口或协议路由声明的最大固定请求头数量。
pub const MAX_FIXED_HEADERS: usize = 16;

/// Maximum byte length of one manifest-declared fixed request header value.
/// 单个 Manifest 声明的固定请求头值的最大字节长度。
pub const MAX_FIXED_HEADER_VALUE_BYTES: usize = 1_024;

/// Manifest engine contract version currently understood by this host.
/// 当前宿主理解的 Manifest 引擎契约版本。
pub const CURRENT_MANIFEST_ENGINE: u32 = 2;

/// Manifest schema revision identifier fixed by this crate.
/// 当前 crate 固定的 Manifest Schema 修订标识。
pub const MANIFEST_SCHEMA_VERSION: &str = "vulcan.provider-manifest.v2";

/// Header names that no manifest may declare, for system or custom providers.
/// 任何清单（系统或自定义）都不得声明的请求头名称。
pub const FORBIDDEN_MANIFEST_HEADERS: &[&str] = &[
    "authorization",
    "host",
    "content-length",
    "transfer-encoding",
    "connection",
    "proxy-authorization",
    "proxy-authenticate",
    "te",
    "trailer",
    "upgrade",
    "keep-alive",
];

/// Maximum bytes accepted for one dynamic session-header name.
/// 单个动态会话请求头名称允许的最大字节数。
pub(crate) const MAX_SESSION_HEADER_NAME_BYTES: usize = 64;

/// Returns whether a session-header name satisfies the host's bounded ASCII name contract.
/// 判断会话请求头名称是否满足宿主有界 ASCII 名称契约。
///
/// # Parameters
/// 参数
/// `name` is the candidate session-header name before HTTP header parsing.
/// `name` 是进入 HTTP 请求头解析前的候选会话请求头名称。
///
/// # Returns
/// 返回值
/// `true` when the name is non-empty, bounded, and contains only accepted ASCII characters.
/// 当名称非空、长度受限且只包含允许的 ASCII 字符时返回 `true`。
pub(crate) fn is_valid_session_header_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_SESSION_HEADER_NAME_BYTES
        && name.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
}

/// Header names owned by the host's final session, authentication, transport, or identity projection.
/// 由宿主最终会话、鉴权、传输或身份投影持有的请求头名称。
const HOST_OWNED_SESSION_HEADERS: &[&str] = &[
    "authorization",
    "api-key",
    "x-api-key",
    "x-auth-token",
    "x-goog-api-key",
    "chatgpt-account-id",
    "x-goog-user-project",
    "cookie",
    "set-cookie",
    "host",
    "content-length",
    "transfer-encoding",
    "connection",
    "proxy-authorization",
    "proxy-authenticate",
    "www-authenticate",
    "accept-encoding",
    "content-type",
    "te",
    "trailer",
    "upgrade",
    "keep-alive",
    "user-agent",
    "openai-beta",
    "origin",
    "expect",
];

/// Returns whether a session-header name is owned by a later host projection and must be skipped.
/// 判断会话请求头名称是否由后续宿主投影持有，并因此必须跳过。
///
/// # Parameters
/// 参数
/// `name` is the case-insensitive candidate request-header name.
/// `name` 是不区分大小写的候选请求头名称。
///
/// # Returns
/// 返回值
/// `true` when the host owns the name or one of its reserved identity namespaces.
/// 当宿主持有该名称或其保留身份命名空间时返回 `true`。
pub(crate) fn is_host_owned_session_header(name: &str) -> bool {
    let normalized = name.to_ascii_lowercase();
    HOST_OWNED_SESSION_HEADERS.contains(&normalized.as_str())
        || normalized.starts_with("x-msh-")
        || normalized.starts_with("x-vulcan-")
        || normalized.starts_with("sec-websocket-")
}

/// Wire-body roots owned exclusively by the host protocol encoder; manifest projection steps
/// may never overwrite them (plan §R8.3). This is the single fact source shared by the
/// validator and the execution-time projection guard.
/// 仅由宿主协议编码器拥有的 Wire 请求体根；Manifest 投影步骤绝不能覆盖它们（方案 §R8.3）。
/// 这是验证器与执行期投影守卫共享的单一事实源。
pub const PROTECTED_REQUEST_BODY_ROOTS: &[&str] =
    &["model", "messages", "input", "tools", "stream"];

/// Carries one stable, actionable manifest validation failure.
/// 承载一个稳定、可操作的 Manifest 验证失败。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestValidationError {
    /// Stable machine-readable failure code.
    /// 稳定的机器可读失败代码。
    pub code: &'static str,
    /// Exact field path or JSON Pointer where the failure occurred.
    /// 失败发生的精确字段路径或 JSON Pointer。
    pub path: String,
    /// Human-readable failure description in English.
    /// 人类可读的英文失败描述。
    pub message: String,
}

impl ManifestValidationError {
    /// Constructs one typed validation failure.
    /// 构造一个类型化验证失败。
    pub fn new(code: &'static str, path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code,
            path: path.into(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ManifestValidationError {
    /// Formats the failure for logs and diagnostics.
    /// 格式化失败用于日志与诊断。
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at {}: {}", self.code, self.path, self.message)
    }
}

impl std::error::Error for ManifestValidationError {}

/// Frozen snapshot of engine-registered protocol profiles and execution primitives.
/// 引擎已注册协议 Profile 与执行原语的冻结快照。
///
/// The validator stays pure: every registered capability is injected by the caller instead of
/// being probed from the host at validation time.
/// 验证器保持纯函数化：所有已注册能力由调用方注入，而不是在验证时探测宿主。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnginePrimitiveRegistry {
    /// Protocol profile identifiers registered by the protocol engine.
    /// 协议引擎注册的协议 Profile 标识。
    pub protocol_profiles: std::collections::BTreeSet<String>,
    /// Generic execution primitive identifiers registered by the execution engine.
    /// 执行引擎注册的通用执行原语标识。
    pub execution_primitives: std::collections::BTreeSet<String>,
    /// Upload primitive identifiers registered by the upload engine.
    /// 上传引擎注册的上传原语标识。
    pub upload_primitives: std::collections::BTreeSet<String>,
    /// Field-mapping transformer identifiers registered by the projection engine.
    /// 投影引擎注册的字段映射转换器标识。
    pub transformers: std::collections::BTreeSet<String>,
}

impl EnginePrimitiveRegistry {
    /// Builds the registry from the protocol constants currently compiled into the host.
    /// 从当前编译进宿主的协议常量构建注册表。
    ///
    /// # Returns
    /// 返回值
    /// A registry seeded with every protocol profile identifier implemented by the engine.
    /// 返回预置引擎实现的全部协议 Profile 标识的注册表。
    pub fn current_protocol_baseline() -> Self {
        use vulcan_llm_core::{
            ANTHROPIC_MESSAGES_PROFILE_ID, ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID,
            CLAUDE_CODE_MESSAGES_PROFILE_ID, COHERE_CHAT_PROFILE_ID,
            GEMINI_GENERATE_CONTENT_PROFILE_ID, GEMINI_INTERACTIONS_PROFILE_ID,
            GOOGLE_CODE_ASSIST_PROFILE_ID, OPENAI_CHAT_PROFILE_ID,
            OPENAI_CODEX_RESPONSES_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID,
            OPENVULCAN_NATIVE_PROFILE_ID, TYPESAFE_SYSTEMONE_PROFILE_ID,
        };
        let mut registry = Self::default();
        for profile in [
            ANTHROPIC_MESSAGES_PROFILE_ID,
            ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID,
            CLAUDE_CODE_MESSAGES_PROFILE_ID,
            COHERE_CHAT_PROFILE_ID,
            GEMINI_GENERATE_CONTENT_PROFILE_ID,
            GEMINI_INTERACTIONS_PROFILE_ID,
            GOOGLE_CODE_ASSIST_PROFILE_ID,
            OPENAI_CHAT_PROFILE_ID,
            OPENAI_CODEX_RESPONSES_PROFILE_ID,
            OPENAI_RESPONSES_PROFILE_ID,
            TYPESAFE_SYSTEMONE_PROFILE_ID,
            OPENVULCAN_NATIVE_PROFILE_ID,
        ] {
            registry.protocol_profiles.insert(profile.to_string());
        }
        // Only execution families with a complete runtime dispatch path are advertised here.
        // Schema-reserved families remain parseable but fail engine-contract validation until
        // their request, response, bounds, and fixture contracts are implemented end to end.
        // 此处仅公布具备完整运行时分派链路的执行族。Schema 预留族仍可解析，但在其请求、
        // 响应、界限与 fixture 契约完整实现前，必须在引擎契约校验阶段失败关闭。
        for primitive in [
            "http_json_sync",
            "http_json_sse",
            "http_multipart_sse",
            "http_json_async_poll",
            "conversation_media_analyze",
            "mcp_http_call",
        ] {
            registry.execution_primitives.insert(primitive.to_string());
        }
        // A reasoning replay contract is advertised only once its request projection, replay
        // acceptance and regression tests ship together, so the remaining declared values are
        // added together with their own implementation.
        // 推理回传契约只在其请求投影、重放接受条件与回归测试一同交付后才公布，
        // 因此其余取值会随各自的实现一并加入。
        for contract in [
            ReasoningReplayContract::DeepseekResponsesPlaintextV1,
            ReasoningReplayContract::DeepseekChatReasoningContentV1,
            ReasoningReplayContract::DeepseekAnthropicThinkingV1,
        ] {
            registry
                .execution_primitives
                .insert(contract.primitive_id().to_string());
        }
        // The closed upload primitive family from plan §16.3; ids match the
        // `UploadPrimitiveKind` snake_case contract.
        // 方案 §16.3 的封闭上传原语族；标识与 `UploadPrimitiveKind` snake_case 契约一致。
        for primitive in [
            "inline_data_url",
            "inline_base64",
            "remote_url_passthrough",
            "multipart_file",
            "provider_file_create",
            "signed_object_upload",
        ] {
            registry.upload_primitives.insert(primitive.to_string());
        }
        for transformer in schema::REGISTERED_MAPPING_TRANSFORMER_IDS {
            registry.transformers.insert((*transformer).to_string());
        }
        registry
    }
}
