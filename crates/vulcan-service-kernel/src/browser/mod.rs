//! Core browser ownership: sessions, leases, observation, input, and automation.
//! Core 浏览器归属：会话、租约、观察、输入与自动化。

mod ai_tools;
#[cfg(test)]
mod ai_tools_acceptance;
mod automation_grant;
pub mod host;

pub(crate) use ai_tools::native_browser_runtime;
pub(crate) use ai_tools::prepare_browser_mcp_bindings;

pub use host::{
    BROWSER_HOST_CONTROL_DISCOVERY_FILENAME, BROWSER_HOST_CONTROL_DISCOVERY_SCHEMA_VERSION,
    BROWSER_HOST_LENGTH_PREFIX_BYTES, BROWSER_HOST_MAX_FRAME_BYTES, BROWSER_HOST_PROTOCOL_VERSION,
    BrowserHostConnection, BrowserHostControlDiscovery, BrowserHostCoreHello,
    BrowserHostCrashSnapshot, BrowserHostEvent, BrowserHostFrameDecoder, BrowserHostHandshake,
    BrowserHostLaunchPlan, BrowserHostProcessHandle, BrowserHostRequestOutcome,
    BrowserHostRuntimeConfig, BrowserHostSpawner, BrowserHostSupervision,
    BrowserHostSupervisionState, BrowserPageRecord, BrowserPageRegistry, SystemBrowserHostSpawner,
    browser_host_supervision_summary, encode_host_frame, read_control_discovery,
};

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::{Arc, Mutex, atomic::AtomicBool},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{SharedServiceBridgeState, current_unix_millis_text};

/// Maximum Core browser sessions owned by one service process.
/// 单个服务进程拥有的最大 Core 浏览器会话数。
const BROWSER_MAX_SESSIONS: usize = 16;

/// Maximum explicitly closed browser records retained for bounded in-process history.
/// 为限制进程内历史增长而保留的显式已关闭浏览器记录上限。
const BROWSER_MAX_RETAINED_CLOSED_SESSIONS: usize = 64;

/// Maximum queued input events retained per session.
/// 每个会话保留的最大排队输入事件数。
const BROWSER_MAX_QUEUED_INPUTS: usize = 256;

/// Maximum observation frames retained per subscriber.
/// 每个订阅者保留的最大观察帧数。
const BROWSER_MAX_QUEUED_FRAMES: usize = 2;

/// Maximum binary frame bytes accepted from one browser host.
/// 单个浏览器宿主接受的最大二进制帧字节数。
const BROWSER_MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;

/// User-held control lease time-to-live.
/// 用户持有控制租约的存活时间。
const BROWSER_USER_LEASE_TTL: Duration = Duration::from_secs(15);

/// User-held control lease heartbeat interval.
/// 用户持有控制租约的心跳间隔。
const BROWSER_USER_LEASE_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

/// Unconfirmed work recorded when an attached automation lease expires.
/// 自动化仍挂接时租约到期所记录的未确认工作。
const BROWSER_AUTOMATION_LEASE_EXPIRED_OPERATION: &str =
    "automation lease expired while gateway attached";

/// Stable Core browser error codes served on the typed browser routes.
/// 类型化浏览器路由提供的稳定 Core 浏览器错误码。
pub const BROWSER_ERROR_UNKNOWN_SESSION: &str = "unknown_session";
/// Stable Core browser error codes served on the typed browser routes.
/// 类型化浏览器路由提供的稳定 Core 浏览器错误码。
pub const BROWSER_ERROR_SESSION_LIMIT: &str = "session_limit";
/// Stable Core browser error codes served on the typed browser routes.
/// 类型化浏览器路由提供的稳定 Core 浏览器错误码。
pub const BROWSER_ERROR_LEASE_CONFLICT: &str = "lease_conflict";
/// Stable Core browser error codes served on the typed browser routes.
/// 类型化浏览器路由提供的稳定 Core 浏览器错误码。
pub const BROWSER_ERROR_LEASE_REQUIRED: &str = "lease_required";
/// Stable Core browser error codes served on the typed browser routes.
/// 类型化浏览器路由提供的稳定 Core 浏览器错误码。
pub const BROWSER_ERROR_STALE_GENERATION: &str = "stale_generation";
/// Stable Core browser error codes served on the typed browser routes.
/// 类型化浏览器路由提供的稳定 Core 浏览器错误码。
pub const BROWSER_ERROR_AUTOMATION_CONFLICT: &str = "automation_conflict";
/// Stable Core browser error codes served on the typed browser routes.
/// 类型化浏览器路由提供的稳定 Core 浏览器错误码。
pub const BROWSER_ERROR_CAPABILITY_UNSUPPORTED: &str = "capability_unsupported";
/// Stable Core browser error codes served on the typed browser routes.
/// 类型化浏览器路由提供的稳定 Core 浏览器错误码。
pub const BROWSER_ERROR_SERVICE_STOPPING: &str = "service_stopping";
/// Stable Core browser error codes served on the typed browser routes.
/// 类型化浏览器路由提供的稳定 Core 浏览器错误码。
pub const BROWSER_ERROR_INVALID_REQUEST: &str = "invalid_request";
/// Refuses new stream tickets once one session reaches its bounded pending-ticket limit.
/// 单个会话待消费流凭据达到上限时拒绝继续签发。
pub const BROWSER_ERROR_STREAM_TICKET_LIMIT: &str = "stream_ticket_limit";
/// Refuses additional staged uploads after one browser reaches its bounded file or byte quota.
/// 单个浏览器达到暂存文件数或字节配额后拒绝继续上传。
pub const BROWSER_ERROR_UPLOAD_LIMIT: &str = "upload_limit";
/// Refuses new observation subscriptions after one session reaches its bounded observer limit.
/// 单个会话观察订阅达到上限后拒绝继续登记。
pub const BROWSER_ERROR_OBSERVER_LIMIT: &str = "observer_limit";

/// Raised when a second persistent browser profile would share one Chromium user-data directory.
/// 当第二个持久画像会话将共享同一个 Chromium 用户数据目录时返回。
pub const BROWSER_ERROR_PROFILE_IN_USE: &str = "profile_in_use";

/// Raised when a takeover cannot proceed because in-flight work was not confirmed stopped.
/// 当在途工作未被确认停止而使接管无法继续时返回。
pub const BROWSER_ERROR_LEASE_BLOCKED: &str = "lease_blocked";

/// Raised when input was computed against a viewport generation that has since changed.
/// 当输入是针对此后已改变的视口代次计算时返回。
pub const BROWSER_ERROR_STALE_VIEWPORT: &str = "stale_viewport";

/// Accepts only browser page URLs admitted by the Core browser contract.
/// 仅接受 Core 浏览器契约允许的页面地址。
///
/// # Returns / 返回值
/// Success for HTTP(S) or exactly `about:blank`; invalid_request otherwise.
/// HTTP(S) 或严格的 `about:blank` 返回成功；其余返回 invalid_request。
fn validate_browser_url(url: &str) -> Result<(), String> {
    if url == "about:blank" {
        return Ok(());
    }
    let parsed =
        reqwest::Url::parse(url).map_err(|_| String::from(BROWSER_ERROR_INVALID_REQUEST))?;
    if matches!(parsed.scheme(), "http" | "https") && parsed.host().is_some() {
        Ok(())
    } else {
        Err(String::from(BROWSER_ERROR_INVALID_REQUEST))
    }
}

/// How long one issued stream ticket stays valid before it must be reissued.
/// 已签发的流凭据在必须重新签发前的有效时长。
const BROWSER_STREAM_TICKET_TTL: Duration = Duration::from_secs(30);
/// Maximum unconsumed stream tickets retained by one Core browser session.
/// 单个 Core 浏览器会话最多保留的未消费流凭据数量。
const BROWSER_MAX_PENDING_STREAM_TICKETS: usize = 64;
/// Maximum observers whose retained frames one Core browser session can hold at once.
/// 单个 Core 浏览器会话可同时保留帧的观察者数量上限。
const BROWSER_MAX_OBSERVERS: usize = 32;

/// Core browser session lifecycle owned by the background manager.
/// 后台管理器拥有的 Core 浏览器会话生命周期。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum BrowserSessionStatus {
    /// Session is being created and its host is starting.
    /// 会话正在创建，宿主正在启动。
    Creating,
    /// Session is running and accepts observation and leased control.
    /// 会话运行中，接受观察与租约控制。
    Running,
    /// Session is closing and refuses new work.
    /// 会话正在关闭，拒绝新工作。
    Closing,
    /// Session is closed and retained only for history.
    /// 会话已关闭，仅保留历史。
    Closed,
    /// Session host crashed and needs an explicit user decision.
    /// 会话宿主已崩溃，需要用户明确决策。
    Crashed,
    /// Session failed and carries a user-facing diagnostic.
    /// 会话已失败，携带面向用户的诊断。
    Failed,
}

/// Core browser profile mode selected at session creation.
/// 会话创建时选择的 Core 浏览器画像模式。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum BrowserProfileMode {
    /// Ephemeral profile discarded when the session closes.
    /// 会话关闭时丢弃的临时画像。
    Ephemeral,
    /// Persistent profile retained across sessions.
    /// 跨会话保留的持久画像。
    Persistent,
}

/// Control lease lifecycle for one Core browser session.
/// 单个 Core 浏览器会话的控制租约生命周期。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum BrowserLeaseState {
    /// No holder owns control; any eligible caller may acquire it.
    /// 无持有者拥有控制权；任何合格调用方均可获取。
    Free,
    /// One holder owns control and heartbeats before expiry.
    /// 单个持有者拥有控制权，并在到期前发送心跳。
    Held,
    /// A takeover is draining in-flight work before handoff.
    /// 接管正在排空在途工作，然后再交接。
    Revoking,
    /// In-flight work is draining after a revoke or release.
    /// 撤销或释放后正在排空在途工作。
    Draining,
    /// A takeover cannot proceed because in-flight work could not be confirmed stopped.
    /// 由于无法确认在途工作已停止，接管无法继续。
    Blocked,
}

/// Automation attachment lifecycle for one Core browser session.
/// 单个 Core 浏览器会话的自动化挂接生命周期。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum BrowserAutomationState {
    /// No automation connection owns the session.
    /// 无自动化连接拥有该会话。
    Detached,
    /// One automation connection owns the session.
    /// 单个自动化连接拥有该会话。
    Attached,
}

/// Public Core browser session projection served to clients.
/// 提供给客户端的公开 Core 浏览器会话投影。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSessionProjection {
    /// Stable session identifier.
    /// 稳定会话标识。
    pub session_id: String,
    /// Immutable owning VulcanCode conversation session identity.
    /// 不可变的所属 VulcanCode 会话身份。
    pub owner_session_id: String,
    /// Current session lifecycle.
    /// 当前会话生命周期。
    pub status: BrowserSessionStatus,
    /// Profile mode selected at creation.
    /// 创建时选择的画像模式。
    pub profile_mode: BrowserProfileMode,
    /// Current page URL, when one is loaded.
    /// 当前页面地址（如已加载）。
    pub current_url: Option<String>,
    /// Current page title, when one is known.
    /// 当前页面标题（如已知）。
    pub current_title: Option<String>,
    /// Current control lease lifecycle.
    /// 当前控制租约生命周期。
    pub lease_state: BrowserLeaseState,
    /// Current automation attachment lifecycle.
    /// 当前自动化挂接生命周期。
    pub automation_state: BrowserAutomationState,
    /// Monotonic page generation guarding input and observation.
    /// 保护输入与观察的单调页面代次。
    /// Serialized as decimal text because it crosses the JavaScript integer boundary.
    /// 跨 JavaScript 整数边界时序列化为十进制文本。
    #[serde(with = "browser_decimal_u64")]
    pub page_generation: u64,
    /// Monotonic control epoch guarding lease handoff.
    /// 保护租约交接的单调控制代次。
    /// Serialized as decimal text because it crosses the JavaScript integer boundary.
    /// 跨 JavaScript 整数边界时序列化为十进制文本。
    #[serde(with = "browser_decimal_u64")]
    pub control_epoch: u64,
    /// Supervised host lifecycle for this session.
    /// 本会话受监督的宿主生命周期。
    pub host_state: BrowserHostSupervisionState,
    /// Current host generation owned by Core.
    /// Core 拥有的当前宿主代际。
    /// Serialized as decimal text because it crosses the JavaScript integer boundary.
    /// 跨 JavaScript 整数边界时序列化为十进制文本。
    #[serde(with = "browser_decimal_u64")]
    pub host_generation: u64,
    /// Number of pages registered for the current host generation.
    /// 当前宿主代际已登记的页面数量。
    pub page_count: usize,
    /// Whether reopening this session requires an explicit user action.
    /// 重开本会话是否需要明确的用户操作。
    pub requires_explicit_reopen: bool,
    /// Monotonic viewport generation guarding pointer coordinates.
    /// 保护指针坐标的单调视口代次。
    ///
    /// Coordinates are meaningful only for the viewport they were measured in, so a viewport change
    /// invalidates every coordinate computed before it.
    /// 坐标只对其被测量时的视口有意义，因此视口变化会使此前计算的每个坐标失效。
    /// Serialized as decimal text because it crosses the JavaScript integer boundary.
    /// 跨 JavaScript 整数边界时序列化为十进制文本。
    #[serde(with = "browser_decimal_u64")]
    pub viewport_generation: u64,
    /// Current viewport size in device-independent pixels, when one was established.
    /// 当前视口尺寸（设备无关像素，如已确定）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewport_size: Option<BrowserViewportSize>,
    /// Why control is blocked, when a takeover could not be honoured.
    /// 控制被阻塞的原因（当接管无法被满足时）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked_reason: Option<String>,
}

/// Viewport size in device-independent pixels.
/// 以设备无关像素表示的视口尺寸。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserViewportSize {
    /// Width in device-independent pixels.
    /// 宽度（设备无关像素）。
    pub width: u32,
    /// Height in device-independent pixels.
    /// 高度（设备无关像素）。
    pub height: u32,
}

/// The real meaning of one input dispatch attempt.
/// 一次输入派发尝试的真实含义。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum BrowserInputOutcome {
    /// Every check passed and the input was handed to the host; application is not yet confirmed.
    /// 所有检查通过且输入已交给宿主；应用结果尚未确认。
    Accepted,
    /// The host confirmed the input was applied to the page.
    /// 宿主已确认输入被应用到页面。
    Applied,
    /// A check failed, so nothing was attempted and the page is unchanged.
    /// 某项检查失败，因此未尝试任何操作且页面未改变。
    Rejected,
    /// The input reached the host but its outcome could not be determined; it is never retried
    /// blindly, because the page may already have changed.
    /// 输入已到达宿主但其结果无法确定；绝不盲目重试，因为页面可能已经改变。
    Unknown,
}

/// Structured result of one input dispatch attempt.
/// 一次输入派发尝试的结构化结果。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserInputDispatchOutcome {
    /// Real meaning of the attempt.
    /// 本次尝试的真实含义。
    pub outcome: BrowserInputOutcome,
    /// Core-assigned input sequence, present when the input was accepted.
    /// Core 分配的输入序号（输入被接受时存在）。
    /// Serialized as decimal text because it crosses the JavaScript integer boundary.
    /// 跨 JavaScript 整数边界时序列化为十进制文本。
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "browser_decimal_u64_option"
    )]
    pub sequence: Option<u64>,
    /// Stable reason code, present when the input was rejected or unknown.
    /// 稳定原因码（输入被拒绝或结果未知时存在）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Host-reported detail, such as where a clamped pointer actually landed.
    /// 宿主报告的细节，例如被收敛的指针实际落点。
    ///
    /// Without this a client cannot tell that its out-of-content click was clamped, and the
    /// whitespace-coordinate behaviour would be unverifiable from the outside.
    /// 缺少它时客户端无法得知其内容外点击已被收敛，而留白坐标行为也将无法从外部验证。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<serde_json::Value>,
}

/// One queued browser input event awaiting host dispatch.
/// 一个等待宿主派发的排队浏览器输入事件。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserInputEvent {
    /// Input sequence number supplied by the sender.
    /// 发送方提供的输入序号。
    pub sequence: String,
    /// Input kind such as pointer, key, or scroll.
    /// 输入类型，例如指针、键盘或滚动。
    pub kind: String,
    /// JSON input payload interpreted by the host adapter.
    /// 由宿主适配器解释的 JSON 输入载荷。
    pub payload: serde_json::Value,
}

/// One browser observation frame retained for a subscriber.
/// 为订阅者保留的一个浏览器观察帧。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserObservationFrame {
    /// Monotonic frame sequence as a decimal string.
    /// 单调帧序号，十进制字符串。
    pub sequence: String,
    /// Stable Core page identity shown in this frame.
    /// 本帧显示的稳定 Core 页面身份。
    pub page_id: String,
    /// Page generation observed by this frame.
    /// 本帧观察到的页面代次。
    /// Serialized as decimal text because it crosses the JavaScript integer boundary.
    /// 跨 JavaScript 整数边界时序列化为十进制文本。
    #[serde(with = "browser_decimal_u64")]
    pub page_generation: u64,
    /// Viewport generation at capture time.
    /// 捕获时的视口代次。
    #[serde(with = "browser_decimal_u64")]
    pub viewport_generation: u64,
    /// Captured content width in device-independent pixels.
    /// 捕获的内容宽度（设备无关像素）。
    pub width: u32,
    /// Captured content height in device-independent pixels.
    /// 捕获的内容高度（设备无关像素）。
    pub height: u32,
    /// Host capture time as Unix milliseconds.
    /// 宿主捕获时间（Unix 毫秒）。
    pub captured_at: String,
    /// Frame MIME type supplied by the host.
    /// 宿主提供的帧 MIME 类型。
    pub mime_type: String,
    /// Frame bytes encoded by the host adapter.
    /// 宿主适配器编码的帧字节。
    #[serde(with = "browser_base64_bytes")]
    pub data: Vec<u8>,
}

/// Internal Core browser session record owned by the service.
/// 服务拥有的内部 Core 浏览器会话记录。
struct BrowserSessionRecord {
    /// Serializes authorization, host delivery, and control handoff for this session only.
    /// 仅为本会话串行化授权、宿主交付及控制交接。
    dispatch_gate: Arc<Mutex<()>>,
    /// Keeps a retained download readable until any history-eviction cleanup finishes.
    /// 在历史驱逐清理结束前保护已保留下载的读取。
    download_gate: Arc<Mutex<()>>,
    /// An expired grant still requires host key cleanup even after credentials were revoked.
    /// 到期授权即使已撤销凭据，仍需清理宿主按键。
    needs_input_release: bool,
    /// Public projection cloned for RPC responses.
    /// 为 RPC 响应克隆的公开投影。
    projection: BrowserSessionProjection,
    /// Current lease holder identity, when held.
    /// 当前租约持有者身份（如已持有）。
    lease_holder: Option<String>,
    /// Current lease secret used to authorize control writes.
    /// 授权控制写入的当前租约凭据。
    lease_secret: Option<String>,
    /// Lease expiry measured on the service monotonic clock.
    /// 服务单调时钟度量的租约到期时间。
    lease_expires_at: Option<Instant>,
    /// Automation connection identity, when attached.
    /// 自动化连接身份（如已挂接）。
    automation_connection_id: Option<String>,
    /// Whether this blocked browser's Chromium context was confirmed closed by its host.
    /// 此阻塞浏览器的 Chromium 上下文是否已由宿主确认关闭。
    recovery_context_closed: bool,
    /// In-flight operations whose completion could not be confirmed.
    /// 无法确认是否结束的在途操作。
    ///
    /// A takeover must never fabricate a new lease while one of these is outstanding, because the
    /// previous holder's work may still be running against the same page.
    /// 当其中任何一项仍未结束时，接管绝不伪造新租约，因为前任持有者的工作可能仍在同一页面上运行。
    unconfirmed_operations: Vec<String>,
    /// Last known outcome per Core input sequence, bounded by the queued-input bound.
    /// 按 Core 输入序号记录的最后已知结果，受排队输入上限约束。
    input_outcomes: BTreeMap<u64, BrowserInputOutcome>,
    /// Issued one-time stream tickets and their monotonic expiry.
    /// 已签发的一次性流凭据及其单调到期时间。
    stream_tickets: BTreeMap<String, (String, Instant)>,
    /// Subscriptions owned by a stream connection rather than by an explicit subscribe call.
    /// 由流连接而非显式订阅调用拥有的订阅。    ///
    /// A disconnecting stream releases only its own subscriptions, so closing one observer can never
    /// disturb another observer or the session.
    /// 断开的流只释放它自己的订阅，因此关闭一个观察端绝不会影响其他观察端或会话。
    stream_subscriptions: BTreeSet<String>,
    /// Page-bound stream identifiers cancelled while their owning sockets are still unwinding.
    /// 已取消但所属套接字尚未完成退出的页面绑定流标识。
    cancelled_stream_subscriptions: BTreeSet<String>,
    /// Files Core staged for this session, keyed by upload identity.
    /// 本会话由 Core 暂存的文件，按上传标识索引。
    uploads: BTreeMap<String, BrowserUploadRecord>,
    /// Download metadata owned by this session and keyed by an opaque download identity.
    /// 此会话拥有的下载元数据，按不透明下载身份索引。
    downloads: BTreeMap<String, BrowserDownloadRecord>,
    /// Pending JavaScript dialogs keyed by the host's opaque dialog identity.
    /// 待处理的 JavaScript 对话框，按宿主的不透明对话框身份索引。
    dialogs: BTreeMap<String, BrowserDialogPublicRecord>,
    /// Intercepted HTML file choosers awaiting a controller-selected upload.
    /// 等待控制者选择上传文件的已拦截 HTML 文件选择器。
    file_choosers: BTreeMap<String, BrowserFileChooserPublicRecord>,
    /// Queued input events awaiting host dispatch.
    /// 等待宿主派发的排队输入事件。
    queued_inputs: VecDeque<BrowserInputEvent>,
    /// Retained observation frames per subscriber.
    /// 每个订阅者保留的观察帧。
    observation_frames: BTreeMap<String, VecDeque<BrowserObservationFrame>>,
    /// Page identity selected independently by each live observation stream.
    /// 每条活动观察流独立选择的页面身份。
    observation_pages: BTreeMap<String, String>,
    /// Next input sequence expected from holders.
    /// 期望持有者发送的下一个输入序号。
    next_input_sequence: u64,
    /// Next observation frame sequence.
    /// 下一个观察帧序号。
    next_frame_sequence: u64,
    /// Supervised host lifecycle, page identities, and crash diagnosis.
    /// 受监督的宿主生命周期、页面身份与崩溃诊断。
    supervision: BrowserHostSupervision,
}

/// Shared Core browser manager state retained inside the service state.
/// 服务状态内保留的共享 Core 浏览器管理器状态。
#[derive(Default)]
struct BrowserManagerState {
    /// Sessions keyed by stable session id.
    /// 按稳定会话标识索引的会话。
    sessions: BTreeMap<String, BrowserSessionRecord>,
    /// Closure order used to evict only old terminal records after the retained-history bound.
    /// 用于在历史上限后仅驱逐较早终态记录的关闭顺序。
    closed_session_order: VecDeque<String>,
    /// AI-selected browser by trusted VulcanCode conversation identity.
    /// 按可信 VulcanCode 会话身份保存 AI 当前选中的浏览器。
    ai_selected_sessions: BTreeMap<String, String>,
    /// Host startups reserved before any blocking process work begins.
    /// 在任何阻塞性进程工作开始前预留的宿主启动槽。
    starting_hosts: BTreeSet<String>,
}

/// Releases a host-start reservation on success, failure, or early return.
/// 在成功、失败或提前返回时释放宿主启动预留。
struct BrowserHostStartReservation {
    /// Shared state whose reservation is released by Drop.
    /// 由 Drop 释放预留的共享状态。
    state: Arc<Mutex<BrowserManagerState>>,
    /// Core-generated browser session owning the startup attempt.
    /// 拥有本次启动尝试的 Core 生成浏览器会话。
    session_id: String,
}

impl Drop for BrowserHostStartReservation {
    /// Removes the startup reservation after host ownership has been adopted or startup fails.
    /// 在宿主归属被接管或启动失败后移除启动预留。
    fn drop(&mut self) {
        if let Ok(mut state) = self.state.lock() {
            state.starting_hosts.remove(&self.session_id);
        }
    }
}

/// Shared Core browser manager owned by the service process.
/// 服务进程拥有的共享 Core 浏览器管理器。
#[derive(Clone)]
pub struct BrowserManager {
    /// Interior-shared manager state.
    /// 内部共享的管理器状态。
    inner: Arc<Mutex<BrowserManagerState>>,
    /// Live host handles keyed by session, each serialized on its own dispatch point.
    /// 按会话索引的活动宿主句柄，各自在自己的调度点上串行化。
    hosts: Arc<Mutex<BTreeMap<String, Arc<Mutex<BrowserHostOwnership>>>>>,
    /// Runtime configuration required to start hosts.
    /// 启动宿主所需的运行时配置。
    host_config: Arc<Mutex<Option<BrowserHostRuntimeConfig>>>,
    /// Serialized AI-to-MCP connection state keyed by trusted conversation identity.
    /// 按可信会话身份索引的串行化 AI 至 MCP 连接状态。
    ai_connections: Arc<
        Mutex<BTreeMap<String, Arc<tokio::sync::Mutex<Option<ai_tools::BrowserAiConnection>>>>>,
    >,
    /// Admission latch closed before browser-plugin drain begins.
    /// 浏览器插件排空开始前关闭的 AI 准入锁存位。
    ai_drain_closed: Arc<AtomicBool>,
    /// Async barrier covering every AI transition, including work before its owner slot exists.
    /// 覆盖所有 AI 转换的异步屏障，包括创建所有者槽位前的工作。
    ai_transition_gate: Arc<tokio::sync::RwLock<()>>,
    /// Spawner used to start host processes.
    /// 用于启动宿主进程的启动器。
    host_spawner: Arc<dyn BrowserHostSpawner>,
}

impl Default for BrowserManager {
    fn default() -> Self {
        Self::new()
    }
}

/// One session's owned host process together with its control channel.
/// 单个会话拥有的宿主进程及其控制通道。
pub struct BrowserHostOwnership {
    /// Core-assigned host generation for this attempt.
    /// 本次尝试的 Core 分配宿主代际。
    pub host_generation: u64,
    /// Owned operating-system process handle.
    /// 拥有的操作系统进程句柄。
    pub process: Box<dyn BrowserHostProcessHandle>,
    /// Authenticated control channel to that process.
    /// 指向该进程的已鉴权控制通道。
    pub connection: BrowserHostConnection,
}

impl BrowserManager {
    /// Returns a session gate without holding the global state lock during host I/O.
    /// 返回会话调度锁，不在宿主输入输出期间持有全局状态锁。
    /// The stable session ID selects the gate; unknown sessions fail before any dispatch.
    /// 稳定会话标识选择调度锁；未知会话在任何派发前失败。
    fn dispatch_gate(&self, session_id: &str) -> Result<Arc<Mutex<()>>, String> {
        self.inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?
            .sessions
            .get(session_id)
            .map(|session| Arc::clone(&session.dispatch_gate))
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))
    }

    /// Clears pressed keys before ownership changes; uncertain cleanup blocks further control.
    /// 在归属变化前清除按键；清理结果不确定时阻止继续控制。
    /// The caller holds the session dispatch gate; the result requires a real host acknowledgement.
    /// 调用者持有会话调度锁；返回成功需要真实宿主确认。
    fn release_host_inputs(&self, session_id: &str) -> Result<(), String> {
        let result = if !self.has_live_host(session_id) {
            Ok(())
        } else {
            match self.request_host(session_id, "input.releaseAll", serde_json::json!({})) {
                Ok(BrowserHostRequestOutcome::Completed(_)) => Ok(()),
                outcome => {
                    let reason = format!("input.releaseAll: {outcome:?}");
                    self.mark_operation_unconfirmed(session_id, &reason)?;
                    Err(format!("{BROWSER_ERROR_LEASE_BLOCKED}: {reason}"))
                }
            }
        };
        if result.is_ok() {
            let mut state = self
                .inner
                .lock()
                .map_err(|error| format!("failed to lock browser manager: {error}"))?;
            let session = state
                .sessions
                .get_mut(session_id)
                .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
            session.needs_input_release = false;
        }
        result
    }

    /// Expires idle leases and releases host keys even when every client disconnected.
    /// 即使所有客户端断开，也使空闲租约到期并释放宿主按键。
    /// Returns the number expired; active dispatches finish before their lease can be handed off.
    /// 返回到期数量；正在派发的操作完成后才能交接其租约。
    pub fn expire_control_leases(&self) -> Result<usize, String> {
        let session_ids: Vec<String> = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?
            .sessions
            .keys()
            .cloned()
            .collect();
        let mut expired = 0;
        for session_id in session_ids {
            let gate = self.dispatch_gate(&session_id)?;
            let _dispatch = match gate.try_lock() {
                Ok(guard) => guard,
                Err(std::sync::TryLockError::WouldBlock) => continue,
                Err(error) => return Err(format!("failed to lock browser dispatch: {error}")),
            };
            let should_expire = {
                let mut state = self
                    .inner
                    .lock()
                    .map_err(|error| format!("failed to lock browser manager: {error}"))?;
                let session = state
                    .sessions
                    .get_mut(&session_id)
                    .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
                expire_lease_if_needed(session);
                // An unknown cleanup is already blocked; do not silently retry it on every tick.
                // 清理结果未知时已进入阻塞，不在每次巡检时静默重试。
                session.needs_input_release
                    && session.projection.lease_state != BrowserLeaseState::Blocked
            };
            if !should_expire {
                continue;
            }
            self.release_host_inputs(&session_id)?;
            expired += 1;
        }
        Ok(expired)
    }

    /// Validates and delivers input atomically with respect to lease changes and session close.
    /// 相对于租约变化与会话关闭，原子地校验并交付输入。
    /// Session, credential, and generations authorize the event; the result reports host confirmation.
    /// 会话、凭据和代次授权事件；结果报告宿主确认情况。
    pub fn dispatch_host_input(
        &self,
        session_id: &str,
        page_id: &str,
        lease_secret: &str,
        page_generation: u64,
        viewport_generation: u64,
        event: BrowserInputEvent,
    ) -> Result<BrowserInputDispatchOutcome, String> {
        let gate = self.dispatch_gate(session_id)?;
        let _dispatch = gate
            .lock()
            .map_err(|error| format!("failed to lock browser dispatch: {error}"))?;
        self.sync_host_events(session_id)?;
        let outcome = self.dispatch_input_outcome(
            session_id,
            lease_secret,
            page_generation,
            viewport_generation,
            event.clone(),
        )?;
        if outcome.outcome != BrowserInputOutcome::Accepted {
            return Ok(outcome);
        }
        let sequence = outcome
            .sequence
            .ok_or_else(|| String::from(BROWSER_ERROR_INVALID_REQUEST))?;
        let target_id = match self.authorized_page_target(session_id, page_id, lease_secret) {
            Ok(target_id) => target_id,
            Err(reason) => {
                self.record_input_outcome(session_id, sequence, BrowserInputOutcome::Rejected)?;
                let mut state = self
                    .inner
                    .lock()
                    .map_err(|error| format!("failed to lock browser manager: {error}"))?;
                if let Some(session) = state.sessions.get_mut(session_id) {
                    session.queued_inputs.pop_back();
                }
                return Ok(BrowserInputDispatchOutcome {
                    outcome: BrowserInputOutcome::Rejected,
                    sequence: Some(sequence),
                    reason: Some(reason),
                    detail: None,
                });
            }
        };
        self.deliver_input_to_host(
            session_id,
            &target_id,
            page_generation,
            viewport_generation,
            sequence,
            &event,
        )
    }

    /// Creates one empty browser manager.
    /// 创建一个空浏览器管理器。
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(BrowserManagerState::default())),
            hosts: Arc::new(Mutex::new(BTreeMap::new())),
            host_config: Arc::new(Mutex::new(None)),
            ai_connections: Arc::new(Mutex::new(BTreeMap::new())),
            ai_drain_closed: Arc::new(AtomicBool::new(false)),
            ai_transition_gate: Arc::new(tokio::sync::RwLock::new(())),
            host_spawner: Arc::new(SystemBrowserHostSpawner),
        }
    }

    /// Creates one manager with an explicit host spawner, used by deterministic tests.
    /// 使用显式宿主启动器创建一个管理器，供确定性测试使用。
    pub fn with_host_spawner(host_spawner: Arc<dyn BrowserHostSpawner>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(BrowserManagerState::default())),
            hosts: Arc::new(Mutex::new(BTreeMap::new())),
            host_config: Arc::new(Mutex::new(None)),
            ai_connections: Arc::new(Mutex::new(BTreeMap::new())),
            ai_drain_closed: Arc::new(AtomicBool::new(false)),
            ai_transition_gate: Arc::new(tokio::sync::RwLock::new(())),
            host_spawner,
        }
    }

    /// Records the runtime configuration used to start browser hosts.
    /// 记录用于启动浏览器宿主的运行时配置。
    pub fn configure_host_runtime(&self, config: BrowserHostRuntimeConfig) -> Result<(), String> {
        let mut slot = self
            .host_config
            .lock()
            .map_err(|error| format!("failed to lock browser host configuration: {error}"))?;
        *slot = Some(config);
        Ok(())
    }

    /// Returns the configured host runtime, when one was recorded.
    /// 返回已记录的宿主运行时（如已配置）。
    pub fn host_runtime_config(&self) -> Result<Option<BrowserHostRuntimeConfig>, String> {
        self.host_config
            .lock()
            .map(|slot| slot.clone())
            .map_err(|error| format!("failed to lock browser host configuration: {error}"))
    }

    /// Starts and adopts one host process for a session.
    /// 为一个会话启动并接管一个宿主进程。
    ///
    /// # Returns
    /// 返回值
    /// The updated projection with a confirmed running host.
    /// 返回已确认运行宿主的更新后投影。
    ///
    /// # Design note / 设计说明
    /// The whole spawn, discovery, and handshake sequence runs **without** the manager state lock,
    /// so slow process startup never blocks service status or other sessions.
    /// 整个启动、发现与握手序列在**不持有**管理器状态锁的情况下运行，因此缓慢的进程启动绝不会阻塞
    /// 服务状态或其他会话。
    pub fn start_host(&self, session_id: &str) -> Result<BrowserSessionProjection, String> {
        let config = self
            .host_runtime_config()?
            .ok_or_else(|| String::from(BROWSER_ERROR_CAPABILITY_UNSUPPORTED))?;
        // The per-session gate prevents a concurrent close from reviving a host after it was
        // explicitly closed. The manager lock only protects the reservation, never process I/O.
        // 每会话调度门防止并发关闭后又复活宿主；管理器锁只保护预留，不覆盖进程 I/O。
        let gate = self.dispatch_gate(session_id)?;
        let _dispatch = gate
            .lock()
            .map_err(|error| format!("failed to lock browser dispatch: {error}"))?;
        let (generation, profile_mode, owner_session_id) = {
            let mut state = self
                .inner
                .lock()
                .map_err(|error| format!("failed to lock browser manager: {error}"))?;
            let session = state
                .sessions
                .get(session_id)
                .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
            if session.projection.status == BrowserSessionStatus::Closed {
                return Err(String::from(BROWSER_ERROR_INVALID_REQUEST));
            }
            if !session.unconfirmed_operations.is_empty() {
                // A crashed host can leave Chromium alive, so a new generation must not reuse
                // its profile while the preceding writes remain unconfirmed.
                // 崩溃的宿主可能遗留 Chromium；先前写入结果未确认时，新代际不得复用画像。
                return Err(String::from(BROWSER_ERROR_LEASE_BLOCKED));
            }
            let generation = session.supervision.host_generation() + 1;
            let profile_mode = session.projection.profile_mode;
            let owner_session_id = session.projection.owner_session_id.clone();
            let hosts = self
                .hosts
                .lock()
                .map_err(|error| format!("failed to lock browser hosts: {error}"))?;
            if state.starting_hosts.contains(session_id) || hosts.contains_key(session_id) {
                return Err(String::from(BROWSER_ERROR_PROFILE_IN_USE));
            }
            if profile_mode == BrowserProfileMode::Persistent {
                let busy = state.sessions.iter().any(|(other_id, other)| {
                    other_id != session_id
                        && other.projection.profile_mode == BrowserProfileMode::Persistent
                        && other.projection.owner_session_id == owner_session_id
                        && (state.starting_hosts.contains(other_id)
                            || hosts.contains_key(other_id)
                            || !other.unconfirmed_operations.is_empty())
                });
                if busy {
                    return Err(String::from(BROWSER_ERROR_PROFILE_IN_USE));
                }
            }
            drop(hosts);
            state.starting_hosts.insert(String::from(session_id));
            (generation, profile_mode, owner_session_id)
        };
        let _reservation = BrowserHostStartReservation {
            state: Arc::clone(&self.inner),
            session_id: String::from(session_id),
        };
        let secret = Uuid::new_v4().to_string();
        let plan = config.launch_plan(
            session_id,
            &owner_session_id,
            profile_mode,
            generation,
            &secret,
        )?;

        let mut process = self.host_spawner.spawn(&plan)?;
        let pid = process.pid();
        let document = match plan.await_control_discovery(config.discovery_timeout) {
            Ok(document) => document,
            Err(error) => {
                // A host that never publishes its endpoint is a real failure, not a silent retry.
                // 从未发布端点的宿主是真实失败，不是静默重试。
                process.kill();
                return Err(error);
            }
        };
        let hello = BrowserHostCoreHello {
            kind: String::from("coreHello"),
            protocol_version: BROWSER_HOST_PROTOCOL_VERSION,
            connection_secret: secret,
            session_id: String::from(session_id),
            host_generation: generation.to_string(),
        };
        let address = format!("{}:{}", document.control_host, document.control_port);
        let connection =
            match BrowserHostConnection::connect(&address, hello, config.discovery_timeout) {
                Ok(connection) => connection,
                Err(error) => {
                    process.kill();
                    return Err(error);
                }
            };

        // The confirmed identity is adopted in one short critical section.
        // 已确认身份在一个短暂的临界区内被采用。
        let mut hosts = self
            .hosts
            .lock()
            .map_err(|error| format!("failed to lock browser hosts: {error}"))?;
        hosts.insert(
            String::from(session_id),
            Arc::new(Mutex::new(BrowserHostOwnership {
                host_generation: generation,
                process,
                connection,
            })),
        );
        drop(hosts);
        self.record_host_running(session_id, pid)?;
        // A host that already had pages before Core connected reports them through page.list, so
        // the session's first page is registered even if its creation event predates the channel.
        // 在 Core 连接前就已有页面的宿主通过 page.list 报告它们，因此即使首个页面的创建事件早于通道
        // 建立，该页面也会被登记。
        if let Err(enumeration_error) = self.enumerate_host_pages(session_id) {
            // A host whose initial page state cannot be confirmed must not be advertised as
            // running. Close its context before releasing the profile or returning an error.
            // 无法确认初始页面状态的宿主不得被发布为运行中；返回错误前先关闭其上下文，
            // 再释放画像占用。
            let cleanup = self
                .stop_host_for_explicit_close_under_gate(session_id)
                .and_then(|_| self.close_session_under_gate(session_id).map(|_| ()));
            return Err(match cleanup {
                Ok(()) => {
                    format!("browser host initial page enumeration failed: {enumeration_error}")
                }
                Err(cleanup_error) => format!(
                    "browser host initial page enumeration failed ({enumeration_error}); cleanup failed ({cleanup_error})"
                ),
            });
        }
        // The returned projection must reflect the pages that enumeration just registered, so a
        // client never sees a stale page count in the creation response.
        // 返回的投影必须反映枚举刚刚登记的页面，使客户端绝不会在创建响应中看到过时的页面计数。
        self.session_projection(session_id)
    }

    /// Enumerates the host's current pages and adopts any that Core does not know yet.
    /// 枚举宿主当前页面，并纳入 Core 尚不知晓的页面。
    ///
    /// # Returns
    /// 返回值
    /// The number of pages adopted by this enumeration.
    /// 返回本次枚举纳入的页面数量。
    pub fn enumerate_host_pages(&self, session_id: &str) -> Result<usize, String> {
        let outcome = self.request_host(session_id, "page.list", serde_json::json!({}))?;
        let result = match outcome {
            BrowserHostRequestOutcome::Completed(result) => result,
            BrowserHostRequestOutcome::Failed { code, message } => {
                return Err(format!("browser host page.list failed ({code}): {message}"));
            }
            BrowserHostRequestOutcome::Unknown { reason } => {
                return Err(format!(
                    "browser host page.list outcome is unknown: {reason}"
                ));
            }
        };
        let pages = result
            .get("pages")
            .and_then(|value| value.as_array())
            .cloned()
            .ok_or_else(|| String::from("browser host page.list omitted its pages array"))?;
        let mut adopted = 0;
        for page in pages {
            let target_id = page
                .get("targetId")
                .and_then(|value| value.as_str())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| String::from("browser host page.list returned no targetId"))?;
            let url = page
                .get("url")
                .and_then(|value| value.as_str())
                .ok_or_else(|| String::from("browser host page.list returned no URL"))?;
            let known = self.page_id_for_target(session_id, target_id)?.is_some();
            self.adopt_page(session_id, target_id, url, None)?;
            if !known {
                adopted += 1;
            }
        }
        // Events emitted before the channel existed are flushed by the host at handshake time.
        // 通道建立前发出的事件由宿主在握手时送出。
        self.sync_host_events(session_id)?;
        Ok(adopted)
    }

    /// Returns whether a confirmed host process is owned for one session.
    /// 返回某个会话是否拥有已确认的宿主进程。
    pub fn has_live_host(&self, session_id: &str) -> bool {
        self.hosts
            .lock()
            .map(|hosts| hosts.contains_key(session_id))
            .unwrap_or(false)
    }

    /// Sends one request to a session's host without holding the manager state lock.
    /// 在不持有管理器状态锁的情况下向某个会话的宿主发送一条请求。
    ///
    /// # Returns
    /// 返回值
    /// The host outcome, or a diagnostic when the session owns no live host.
    /// 返回宿主结果；会话没有活动宿主时返回诊断。
    pub fn request_host(
        &self,
        session_id: &str,
        operation: &str,
        params: serde_json::Value,
    ) -> Result<BrowserHostRequestOutcome, String> {
        let ownership = self.owned_host(session_id)?;
        let timeout = self
            .host_runtime_config()?
            .map(|config| config.request_timeout)
            .unwrap_or_else(|| Duration::from_secs(10));
        // The per-session mutex is the single dispatch point: writes of one session serialize
        // while other sessions and the global state lock stay free.
        // 每会话互斥锁是唯一调度点：同一会话的写入串行化，而其他会话与全局状态锁保持空闲。
        let mut ownership = ownership
            .lock()
            .map_err(|error| format!("failed to lock browser host: {error}"))?;
        Ok(ownership.connection.request(operation, params, timeout))
    }

    /// Returns the number of host generations confirmed for one session.
    /// 返回某个会话已确认的宿主代际数。
    pub fn confirmed_host_generation(&self, session_id: &str) -> Option<u64> {
        self.owned_host(session_id)
            .ok()
            .and_then(|ownership| ownership.lock().ok().map(|guard| guard.host_generation))
    }

    /// Reaps one session's host if its process has already exited.
    /// 若某个会话的宿主进程已退出则回收它。
    ///
    /// # Returns
    /// 返回值
    /// `Ok(Some(projection))` when a dead host was reaped, `Ok(None)` while it still runs.
    /// 已回收死宿主时返回 `Ok(Some(projection))`；仍在运行时返回 `Ok(None)`。
    pub fn reap_host(&self, session_id: &str) -> Result<Option<BrowserSessionProjection>, String> {
        let gate = self.dispatch_gate(session_id)?;
        let _dispatch = match gate.try_lock() {
            Ok(guard) => guard,
            Err(std::sync::TryLockError::WouldBlock) => return Ok(None),
            Err(error) => return Err(format!("failed to lock browser dispatch: {error}")),
        };
        let ownership = match self.owned_host(session_id) {
            Ok(ownership) => ownership,
            Err(error) if error == BROWSER_ERROR_UNKNOWN_SESSION => return Ok(None),
            Err(error) => return Err(error),
        };
        let exit_code = {
            let mut ownership = ownership
                .lock()
                .map_err(|error| format!("failed to lock browser host: {error}"))?;
            ownership.process.try_wait()?
        };
        let Some(exit_code) = exit_code else {
            // Still running: ownership has remained in the registry throughout inspection.
            // 仍在运行：检查期间归属始终保留在注册表中。
            return Ok(None);
        };
        let recovery_context_closed = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?
            .sessions
            .get(session_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?
            .recovery_context_closed;
        // Only a confirmed process exit permits removing the process proof. An inspection error
        // or a session-state error leaves the registered ownership available for another check.
        // 仅确认进程退出后才能移除进程凭据；检查或会话状态出错时保留归属供后续复查。
        let removed = self.take_owned_host(session_id).ok_or_else(|| {
            String::from("browser host ownership disappeared after confirmed process exit")
        })?;
        drop(removed);
        if recovery_context_closed {
            // A prior blocked retirement already proved Chromium closed. The exact host process
            // has now exited as well, so this is an expected close rather than a host crash.
            // 此前的阻塞退役已证明 Chromium 关闭；此处又观察到精确宿主进程退出，
            // 因而这是预期关闭，不是宿主崩溃。
            return self.close_session_under_gate(session_id).map(Some);
        }
        self.record_host_crash(
            session_id,
            Some(exit_code),
            "browser host process exited unexpectedly",
        )
        .map(Some)
    }

    /// Reaps every session whose host process has exited.
    /// 回收每个宿主进程已退出的会话。
    pub fn reap_hosts(&self) -> Result<Vec<BrowserSessionProjection>, String> {
        let session_ids: Vec<String> = self
            .hosts
            .lock()
            .map_err(|error| format!("failed to lock browser hosts: {error}"))?
            .keys()
            .cloned()
            .collect();
        let mut retired = Vec::new();
        let mut failures = Vec::new();
        for session_id in session_ids {
            match self.reap_host(&session_id) {
                Ok(Some(projection)) => retired.push(projection),
                Ok(None) => {}
                Err(error) => failures.push(format!("{session_id}: {error}")),
            }
        }
        if failures.is_empty() {
            Ok(retired)
        } else {
            // One uninspectable process must not starve later sessions in the same sweep. Keep
            // both the failure and the successfully reaped identities in the diagnostic.
            // 单个无法检查的进程不得阻断本轮后续会话；诊断同时保留失败和已回收的会话身份。
            let reaped = retired
                .iter()
                .map(|projection| projection.session_id.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            Err(format!(
                "browser host liveness checks failed: {}; reaped sessions: [{reaped}]",
                failures.join("; ")
            ))
        }
    }

    /// Closes every owned host channel so each host stops and exits by design.
    /// 关闭所有已拥有宿主的通道，使每个宿主按设计停止并退出。
    ///
    /// # Returns
    /// 返回值
    /// The number of host channels closed.
    /// 返回已关闭的宿主通道数量。
    pub fn shutdown_hosts(&self) -> Result<usize, String> {
        let mut hosts = self
            .hosts
            .lock()
            .map_err(|error| format!("failed to lock browser hosts: {error}"))?;
        let mut closed = 0;
        for (_session_id, ownership) in hosts.iter() {
            if let Ok(ownership) = ownership.lock() {
                closed += 1;
                let _ = &ownership.host_generation;
            }
        }
        // Draining the map drops each handle, whose connection close ends the host process.
        // 清空映射会丢弃每个句柄，其连接关闭会结束宿主进程。
        hosts.clear();
        Ok(closed)
    }

    /// Returns the shared ownership handle for one session.
    /// 返回某个会话的共享归属句柄。
    fn owned_host(&self, session_id: &str) -> Result<Arc<Mutex<BrowserHostOwnership>>, String> {
        self.hosts
            .lock()
            .map_err(|error| format!("failed to lock browser hosts: {error}"))?
            .get(session_id)
            .cloned()
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))
    }

    /// Waits for the exact owned host process to exit without dropping its proof handle.
    /// 在不丢弃证明句柄的前提下等待精确归属的宿主进程退出。
    ///
    /// `session_id` selects the process; `timeout` bounds the wait. Exit returns true, timeout keeps ownership, and inspection errors block shutdown.
    /// `session_id` 选中进程；`timeout` 限定等待。确认退出返回真，超时保留归属，检查错误阻塞关闭。
    fn wait_owned_host_exit(&self, session_id: &str, timeout: Duration) -> Result<bool, String> {
        let ownership = self.owned_host(session_id)?;
        let deadline = Instant::now() + timeout;
        loop {
            let inspected = ownership
                .lock()
                .map_err(|error| format!("failed to lock browser host: {error}"))
                .and_then(|mut ownership| ownership.process.try_wait());
            match inspected {
                Ok(Some(_)) => return Ok(true),
                Ok(None) => {}
                Err(error) => {
                    self.mark_operation_unconfirmed(session_id, "host.shutdown")?;
                    return Err(format!("{BROWSER_ERROR_LEASE_BLOCKED}: {error}"));
                }
            }
            if Instant::now() >= deadline {
                return Ok(false);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Removes one session's ownership handle from the registry.
    /// 从注册表移除某个会话的归属句柄。
    fn take_owned_host(&self, session_id: &str) -> Option<Arc<Mutex<BrowserHostOwnership>>> {
        self.hosts.lock().ok()?.remove(session_id)
    }

    /// Creates one Core browser session owned by the background manager.
    /// 创建一个由后台管理器拥有的 Core 浏览器会话。
    #[cfg(test)]
    pub fn create_session(
        &self,
        profile_mode: BrowserProfileMode,
    ) -> Result<BrowserSessionProjection, String> {
        self.create_owned_session("browser-test-owner", profile_mode)
    }

    /// Creates one Core browser that belongs to an exact VulcanCode conversation.
    /// 创建一个归属精确 VulcanCode 会话的 Core 浏览器。
    ///
    /// # Parameters
    /// 参数
    /// `owner_session_id` is the caller-validated conversation identity and `profile_mode` selects storage.
    /// `owner_session_id` 是由调用方校验的会话身份，`profile_mode` 选择存储方式。
    ///
    /// # Returns
    /// 返回值
    /// Returns the new session projection with immutable ownership.
    /// 返回具有不可变归属的新会话投影。
    pub fn create_owned_session(
        &self,
        owner_session_id: &str,
        profile_mode: BrowserProfileMode,
    ) -> Result<BrowserSessionProjection, String> {
        if owner_session_id.trim().is_empty() || owner_session_id != owner_session_id.trim() {
            return Err(String::from(BROWSER_ERROR_INVALID_REQUEST));
        }
        let mut state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let live = state
            .sessions
            .values()
            .filter(|session| {
                !matches!(
                    session.projection.status,
                    BrowserSessionStatus::Closed
                        | BrowserSessionStatus::Crashed
                        | BrowserSessionStatus::Failed
                )
            })
            .count();
        if live >= BROWSER_MAX_SESSIONS {
            return Err(String::from(BROWSER_ERROR_SESSION_LIMIT));
        }
        let session_id = Uuid::new_v4().to_string();
        let projection = BrowserSessionProjection {
            session_id: session_id.clone(),
            owner_session_id: owner_session_id.to_owned(),
            status: BrowserSessionStatus::Running,
            profile_mode,
            current_url: None,
            current_title: None,
            lease_state: BrowserLeaseState::Free,
            viewport_generation: 1,
            viewport_size: None,
            blocked_reason: None,
            automation_state: BrowserAutomationState::Detached,
            page_generation: 1,
            control_epoch: 1,
            // A new session has no supervised host process yet; Core starts one on demand.
            // 新会话尚无受监督的宿主进程；Core 在需要时启动一个。
            host_state: BrowserHostSupervisionState::NotStarted,
            host_generation: 0,
            page_count: 0,
            requires_explicit_reopen: false,
        };
        state.sessions.insert(
            session_id,
            BrowserSessionRecord {
                dispatch_gate: Arc::new(Mutex::new(())),
                download_gate: Arc::new(Mutex::new(())),
                needs_input_release: false,
                projection: projection.clone(),
                lease_holder: None,
                lease_secret: None,
                lease_expires_at: None,
                automation_connection_id: None,
                recovery_context_closed: false,
                unconfirmed_operations: Vec::new(),
                input_outcomes: BTreeMap::new(),
                stream_tickets: BTreeMap::new(),
                stream_subscriptions: BTreeSet::new(),
                cancelled_stream_subscriptions: BTreeSet::new(),
                uploads: BTreeMap::new(),
                downloads: BTreeMap::new(),
                dialogs: BTreeMap::new(),
                file_choosers: BTreeMap::new(),
                queued_inputs: VecDeque::new(),
                observation_frames: BTreeMap::new(),
                observation_pages: BTreeMap::new(),
                next_input_sequence: 1,
                next_frame_sequence: 1,
                supervision: BrowserHostSupervision::new(),
            },
        );
        Ok(projection)
    }

    /// Lists all retained Core browser sessions.
    /// 列出全部保留的 Core 浏览器会话。
    pub fn list_sessions(&self) -> Result<Vec<BrowserSessionProjection>, String> {
        let state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        Ok(state
            .sessions
            .values()
            .map(|session| session.projection.clone())
            .collect())
    }

    /// Lists only browser sessions owned by one VulcanCode conversation.
    /// 仅列出一个 VulcanCode 会话拥有的浏览器会话。
    ///
    /// # Parameters
    /// 参数
    /// `owner_session_id` is the exact conversation identity supplied by the authorized caller.
    /// `owner_session_id` 是已授权调用方提供的精确会话身份。
    ///
    /// # Returns
    /// 返回值
    /// Returns matching projections without exposing another conversation's browsers.
    /// 返回匹配的投影，不暴露其他会话的浏览器。
    pub fn list_owned_sessions(
        &self,
        owner_session_id: &str,
    ) -> Result<Vec<BrowserSessionProjection>, String> {
        let state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        Ok(state
            .sessions
            .values()
            .filter(|session| session.projection.owner_session_id == owner_session_id)
            .map(|session| session.projection.clone())
            .collect())
    }

    /// Requires one browser to belong to the exact VulcanCode conversation before routing access.
    /// 路由访问前要求浏览器归属精确的 VulcanCode 会话。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` identifies the browser and `owner_session_id` identifies the conversation.
    /// `session_id` 标识浏览器，`owner_session_id` 标识会话。
    ///
    /// # Returns
    /// 返回值
    /// Returns success for exact ownership and one indistinguishable error otherwise.
    /// 归属精确匹配时返回成功，否则返回统一错误。
    pub fn require_owner(&self, session_id: &str, owner_session_id: &str) -> Result<(), String> {
        let state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        match state.sessions.get(session_id) {
            Some(session) if session.projection.owner_session_id == owner_session_id => Ok(()),
            _ => Err(String::from(BROWSER_ERROR_UNKNOWN_SESSION)),
        }
    }

    /// Reads one session projection by stable id.
    /// 按稳定标识读取一个会话投影。
    pub fn session_projection(&self, session_id: &str) -> Result<BrowserSessionProjection, String> {
        let state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        state
            .sessions
            .get(session_id)
            .map(|session| session.projection.clone())
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))
    }

    /// Records that Core confirmed a running host process for one session.
    /// 记录 Core 已确认某个会话的宿主进程正在运行。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` is the owning session and `pid` is the confirmed host process id.
    /// `session_id` 是归属会话，`pid` 是已确认的宿主进程标识。
    ///
    /// # Returns
    /// 返回值
    /// The updated projection, or `unknown_session` when no such session exists.
    /// 返回更新后的投影；不存在该会话时返回 `unknown_session`。
    pub fn record_host_running(
        &self,
        session_id: &str,
        pid: u32,
    ) -> Result<BrowserSessionProjection, String> {
        let mut state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let session = state
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
        let generation = session.supervision.record_running(pid);
        session.projection.status = BrowserSessionStatus::Running;
        session.projection.host_state = BrowserHostSupervisionState::Running;
        session.projection.host_generation = generation;
        session.projection.page_count = 0;
        session.projection.requires_explicit_reopen = false;
        session.recovery_context_closed = false;
        Ok(session.projection.clone())
    }

    /// Records an unexpected host exit for one session and retains its diagnosis.
    /// 记录某个会话的宿主意外退出并保留其诊断。
    ///
    /// # Returns
    /// 返回值
    /// The updated projection with `crashed` state and the invalidated page count applied.
    /// 返回更新后的投影，其中含 `crashed` 状态与已失效页面数。
    pub fn record_host_crash(
        &self,
        session_id: &str,
        exit_code: Option<i32>,
        reason: &str,
    ) -> Result<BrowserSessionProjection, String> {
        let mut state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let session = state
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
        // A host crash invalidates its control credentials but does not prove its Chromium child
        // stopped. Preserve an existing uncertainty block across any explicit reopen attempt.
        // 宿主崩溃使控制凭据失效，但不能证明 Chromium 子进程已停；
        // 显式重开之前仍需保留已有的未知结果阻塞。
        session.supervision.record_crash(exit_code, reason);
        session.projection.status = BrowserSessionStatus::Crashed;
        session.projection.host_state = BrowserHostSupervisionState::Crashed;
        session.projection.page_count = 0;
        session.projection.requires_explicit_reopen = true;
        session.projection.lease_state = if session.unconfirmed_operations.is_empty() {
            BrowserLeaseState::Free
        } else {
            BrowserLeaseState::Blocked
        };
        session.projection.blocked_reason = if session.unconfirmed_operations.is_empty() {
            None
        } else {
            Some(session.unconfirmed_operations.join("; "))
        };
        session.projection.automation_state = BrowserAutomationState::Detached;
        session.lease_holder = None;
        session.lease_secret = None;
        session.lease_expires_at = None;
        session.automation_connection_id = None;
        session.queued_inputs.clear();
        Ok(session.projection.clone())
    }

    /// Adopts one host target as a Core page inside the owning session.
    /// 在归属会话内将一个宿主目标纳入 Core 页面。
    ///
    /// # Parameters
    /// 参数
    /// `target_id` is the host target identity, `url` is only display state, and `opener_page_id`
    /// records the page that opened this one when known.
    /// `target_id` 是宿主目标标识，`url` 仅是展示状态，`opener_page_id` 在已知时记录打开本页的页面。
    ///
    /// # Returns
    /// 返回值
    /// The stable Core page identity, which never changes when the page navigates.
    /// 返回稳定的 Core 页面标识；页面导航时该标识绝不改变。
    pub fn adopt_page(
        &self,
        session_id: &str,
        target_id: &str,
        url: &str,
        opener_page_id: Option<&str>,
    ) -> Result<BrowserPageRecord, String> {
        if target_id.trim().is_empty() {
            return Err(String::from(BROWSER_ERROR_INVALID_REQUEST));
        }
        let mut state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let session = state
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
        let page_id =
            session
                .supervision
                .pages_mut()
                .adopt_opened_target(target_id, url, opener_page_id);
        session.projection.page_count = session.supervision.pages().pages().len();
        if let Some(opener) = opener_page_id {
            let _ = opener;
        }
        session
            .supervision
            .pages()
            .target_for_page(&page_id)
            .cloned()
            .ok_or_else(|| String::from(BROWSER_ERROR_INVALID_REQUEST))
    }

    /// Drains one session's host events and registers every reported page.
    /// 取走某个会话的宿主事件并登记其中报告的每个页面。
    ///
    /// # Returns
    /// 返回值
    /// The number of pages registered or refreshed by this drain.
    /// 返回本次取走中登记或刷新的页面数量。
    ///
    /// # Design note / 设计说明
    /// Events carry host target identities, so page registration never guesses from URLs: a popup,
    /// an automation-created page, and a host-created page all arrive through the same channel.
    /// 事件携带宿主目标标识，因此页面登记绝不从地址猜测：弹出页、自动化创建的页面与宿主创建的页面
    /// 都经同一通道到达。
    pub fn sync_host_events(&self, session_id: &str) -> Result<usize, String> {
        let ownership = match self.owned_host(session_id) {
            Ok(ownership) => ownership,
            Err(error) if error == BROWSER_ERROR_UNKNOWN_SESSION => return Ok(0),
            Err(error) => return Err(error),
        };
        let events = {
            let mut ownership = ownership
                .lock()
                .map_err(|error| format!("failed to lock browser host: {error}"))?;
            ownership.connection.drain_events()?
        };
        let mut applied = 0;
        for event in events {
            if self.apply_host_event(session_id, &event)? {
                applied += 1;
            }
        }
        Ok(applied)
    }

    /// Applies one host event to the owning session.
    /// 将一个宿主事件应用到归属会话。
    ///
    /// # Returns
    /// 返回值
    /// `true` when the event changed a registered page.
    /// 事件改变了已登记页面时返回 `true`。
    fn apply_host_event(&self, session_id: &str, event: &BrowserHostEvent) -> Result<bool, String> {
        let target_id = event
            .payload
            .get("targetId")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let url = event
            .payload
            .get("url")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        match event.kind.as_str() {
            // A new host target becomes a Core page with a stable identity.
            // 新的宿主目标成为具有稳定标识的 Core 页面。
            "pageCreated" => {
                if target_id.is_empty() {
                    return Ok(false);
                }
                let opener = event
                    .payload
                    .get("openerTargetId")
                    .and_then(|value| value.as_str())
                    .filter(|value| !value.is_empty())
                    .map(|value| self.page_id_for_target(session_id, value))
                    .transpose()?
                    .flatten();
                self.adopt_page(session_id, target_id, url, opener.as_deref())?;
                Ok(true)
            }
            // Navigation refreshes the URL of the existing page without changing its identity.
            // 导航刷新既有页面的地址，但不改变其标识。
            "pageNavigated" => {
                if target_id.is_empty() {
                    return Ok(false);
                }
                let known = self.page_id_for_target(session_id, target_id)?.is_some();
                if !known {
                    // A navigation for an unseen target still proves the page exists.
                    // 未见过目标的导航同样证明该页面存在。
                    self.adopt_page(session_id, target_id, url, None)?;
                    return Ok(true);
                }
                self.adopt_page(session_id, target_id, url, None)?;
                Ok(true)
            }
            // A destroyed target cannot remain addressable through its former Core page id.
            // 已销毁的目标不能继续通过先前的 Core 页面标识访问。
            "pageDestroyed" => {
                if target_id.is_empty() {
                    return Ok(false);
                }
                let mut state = self
                    .inner
                    .lock()
                    .map_err(|error| format!("failed to lock browser manager: {error}"))?;
                let session = state
                    .sessions
                    .get_mut(session_id)
                    .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
                let removed = session.supervision.pages_mut().remove_target(target_id);
                if removed.is_none() {
                    return Ok(false);
                }
                session.projection.page_count = session.supervision.pages().pages().len();
                session.projection.page_generation =
                    session.projection.page_generation.wrapping_add(1);
                session.queued_inputs.clear();
                Ok(true)
            }
            "downloadStarted" => {
                let download_id = event
                    .payload
                    .get("downloadId")
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| String::from(BROWSER_ERROR_INVALID_REQUEST))?;
                Uuid::parse_str(download_id)
                    .map_err(|_| String::from(BROWSER_ERROR_INVALID_REQUEST))?;
                let page_id = self
                    .page_id_for_target(session_id, target_id)?
                    .ok_or_else(|| String::from(BROWSER_ERROR_STALE_GENERATION))?;
                let file_name = browser_download_filename(
                    event
                        .payload
                        .get("fileName")
                        .and_then(|value| value.as_str())
                        .unwrap_or_default(),
                );
                let mut state = self
                    .inner
                    .lock()
                    .map_err(|error| format!("failed to lock browser manager: {error}"))?;
                let session = state
                    .sessions
                    .get_mut(session_id)
                    .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
                if session.downloads.contains_key(download_id) {
                    return Err(String::from(BROWSER_ERROR_INVALID_REQUEST));
                }
                session.downloads.insert(
                    download_id.to_string(),
                    BrowserDownloadRecord {
                        public: BrowserDownloadPublicRecord {
                            download_id: download_id.to_string(),
                            session_id: session_id.to_string(),
                            page_id,
                            file_name,
                            status: BrowserDownloadStatus::Pending,
                            byte_length: None,
                            sha256: None,
                        },
                        target_id: target_id.to_string(),
                    },
                );
                Ok(true)
            }
            "downloadFinished" => {
                let download_id = event
                    .payload
                    .get("downloadId")
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| String::from(BROWSER_ERROR_INVALID_REQUEST))?;
                let host_completed = event.payload.get("status").and_then(|value| value.as_str())
                    == Some("completed");
                let expected_length = event
                    .payload
                    .get("byteLength")
                    .and_then(|value| value.as_u64());
                let verified = if host_completed {
                    let path = self.download_storage_path(session_id, download_id)?;
                    match std::fs::metadata(&path) {
                        Ok(metadata)
                            if metadata.len() <= BROWSER_MAX_UPLOAD_BYTES as u64
                                && Some(metadata.len()) == expected_length =>
                        {
                            std::fs::read(&path).ok().map(|bytes| {
                                let digest = browser_sha256_hex(&bytes);
                                (metadata.len(), digest)
                            })
                        }
                        _ => None,
                    }
                } else {
                    None
                };
                // A failed or oversized download has no readable Core record, so its host file
                // must not remain on disk after the completion event.
                // 失败或超限下载没有可读取的 Core 记录，因此完成事件后不应保留宿主文件。
                let failed_path = if verified.is_none() {
                    Some(self.download_storage_path(session_id, download_id)?)
                } else {
                    None
                };
                let mut state = self
                    .inner
                    .lock()
                    .map_err(|error| format!("failed to lock browser manager: {error}"))?;
                let record = state
                    .sessions
                    .get_mut(session_id)
                    .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?
                    .downloads
                    .get_mut(download_id)
                    .ok_or_else(|| String::from(BROWSER_ERROR_INVALID_REQUEST))?;
                if record.target_id != target_id
                    || record.public.status != BrowserDownloadStatus::Pending
                {
                    return Err(String::from(BROWSER_ERROR_INVALID_REQUEST));
                }
                if let Some((byte_length, sha256)) = verified {
                    record.public.status = BrowserDownloadStatus::Completed;
                    record.public.byte_length = Some(byte_length);
                    record.public.sha256 = Some(sha256);
                } else {
                    record.public.status = BrowserDownloadStatus::Failed;
                    record.public.byte_length = expected_length;
                }
                drop(state);
                if let Some(path) = failed_path {
                    match std::fs::remove_file(&path) {
                        Ok(()) => {}
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                        Err(error) => {
                            tracing::warn!(session_id, download_id, path = %path.display(), %error, "failed to remove rejected browser download");
                        }
                    }
                }
                Ok(true)
            }
            "dialogOpened" => {
                let dialog_id = event
                    .payload
                    .get("dialogId")
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| String::from(BROWSER_ERROR_INVALID_REQUEST))?;
                Uuid::parse_str(dialog_id)
                    .map_err(|_| String::from(BROWSER_ERROR_INVALID_REQUEST))?;
                let page_id = self
                    .page_id_for_target(session_id, target_id)?
                    .ok_or_else(|| String::from(BROWSER_ERROR_STALE_GENERATION))?;
                let dialog_type = event
                    .payload
                    .get("dialogType")
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| String::from(BROWSER_ERROR_INVALID_REQUEST))?;
                if !matches!(dialog_type, "alert" | "confirm" | "prompt" | "beforeunload") {
                    return Err(String::from(BROWSER_ERROR_INVALID_REQUEST));
                }
                let record = BrowserDialogPublicRecord {
                    dialog_id: dialog_id.to_string(),
                    session_id: session_id.to_string(),
                    page_id,
                    dialog_type: dialog_type.to_string(),
                    message: event
                        .payload
                        .get("message")
                        .and_then(|value| value.as_str())
                        .unwrap_or_default()
                        .chars()
                        .take(4096)
                        .collect(),
                    default_prompt: event
                        .payload
                        .get("defaultPrompt")
                        .and_then(|value| value.as_str())
                        .unwrap_or_default()
                        .chars()
                        .take(4096)
                        .collect(),
                };
                let mut state = self
                    .inner
                    .lock()
                    .map_err(|error| format!("failed to lock browser manager: {error}"))?;
                let session = state
                    .sessions
                    .get_mut(session_id)
                    .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
                session
                    .dialogs
                    .retain(|_, existing| existing.page_id != record.page_id);
                session.dialogs.insert(dialog_id.to_string(), record);
                Ok(true)
            }
            "dialogClosed" => {
                let dialog_id = event
                    .payload
                    .get("dialogId")
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| String::from(BROWSER_ERROR_INVALID_REQUEST))?;
                let mut state = self
                    .inner
                    .lock()
                    .map_err(|error| format!("failed to lock browser manager: {error}"))?;
                let session = state
                    .sessions
                    .get_mut(session_id)
                    .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
                session.dialogs.remove(dialog_id);
                Ok(true)
            }
            "fileChooserOpened" => {
                let chooser_id = event
                    .payload
                    .get("chooserId")
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| String::from(BROWSER_ERROR_INVALID_REQUEST))?;
                Uuid::parse_str(chooser_id)
                    .map_err(|_| String::from(BROWSER_ERROR_INVALID_REQUEST))?;
                let page_id = self
                    .page_id_for_target(session_id, target_id)?
                    .ok_or_else(|| String::from(BROWSER_ERROR_STALE_GENERATION))?;
                let mode = event
                    .payload
                    .get("mode")
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| String::from(BROWSER_ERROR_INVALID_REQUEST))?;
                if !matches!(mode, "selectSingle" | "selectMultiple") {
                    return Err(String::from(BROWSER_ERROR_INVALID_REQUEST));
                }
                let record = BrowserFileChooserPublicRecord {
                    chooser_id: chooser_id.to_string(),
                    session_id: session_id.to_string(),
                    page_id,
                    mode: mode.to_string(),
                };
                let mut state = self
                    .inner
                    .lock()
                    .map_err(|error| format!("failed to lock browser manager: {error}"))?;
                let session = state
                    .sessions
                    .get_mut(session_id)
                    .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
                session
                    .file_choosers
                    .retain(|_, existing| existing.page_id != record.page_id);
                session.file_choosers.insert(chooser_id.to_string(), record);
                Ok(true)
            }
            "fileChooserClosed" => {
                let chooser_id = event
                    .payload
                    .get("chooserId")
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| String::from(BROWSER_ERROR_INVALID_REQUEST))?;
                let mut state = self
                    .inner
                    .lock()
                    .map_err(|error| format!("failed to lock browser manager: {error}"))?;
                let session = state
                    .sessions
                    .get_mut(session_id)
                    .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
                session.file_choosers.remove(chooser_id);
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// Resolves the Core page identity currently bound to one host target.
    /// 解析当前绑定到某个宿主目标的 Core 页面标识。
    fn page_id_for_target(
        &self,
        session_id: &str,
        target_id: &str,
    ) -> Result<Option<String>, String> {
        let state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let session = state
            .sessions
            .get(session_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
        Ok(session
            .supervision
            .pages()
            .pages()
            .into_iter()
            .find(|page| page.target_id == target_id)
            .map(|page| page.page_id))
    }

    /// Lists the pages of one session with their stable Core identities.
    /// 列出某个会话的页面及其稳定 Core 标识。
    pub fn list_pages(&self, session_id: &str) -> Result<Vec<BrowserPageRecord>, String> {
        // Host events are drained first so a client asking for pages sees the pages that already
        // exist in the real browser, not only the ones Core was told about earlier.
        // 先取走宿主事件，使请求页面的客户端看到真实浏览器中已存在的页面，而不仅是 Core 早前被告知的。
        self.sync_host_events(session_id)?;
        let state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let session = state
            .sessions
            .get(session_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
        Ok(session.supervision.pages().pages())
    }

    /// Returns the supervision summary of one session, including crash diagnosis.
    /// 返回某个会话的监督摘要，含崩溃诊断。
    pub fn host_supervision_summary(&self, session_id: &str) -> Result<serde_json::Value, String> {
        let state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let session = state
            .sessions
            .get(session_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
        Ok(browser_host_supervision_summary(&session.supervision))
    }

    /// Closes one session and releases its lease and automation ownership.
    /// 关闭一个会话并释放其租约与自动化归属。
    pub fn close_session(&self, session_id: &str) -> Result<BrowserSessionProjection, String> {
        let gate = self.dispatch_gate(session_id)?;
        let _dispatch = gate
            .lock()
            .map_err(|error| format!("failed to lock browser dispatch: {error}"))?;
        self.stop_host_for_explicit_close_under_gate(session_id)?;
        self.close_session_under_gate(session_id)
    }

    /// Closes a session only while the caller's control grant remains authoritative.
    /// 仅在调用者控制授权仍权威时关闭会话。
    pub fn close_session_with_lease(
        &self,
        session_id: &str,
        lease_secret: &str,
    ) -> Result<BrowserSessionProjection, String> {
        let gate = self.dispatch_gate(session_id)?;
        let _dispatch = gate
            .lock()
            .map_err(|error| format!("failed to lock browser dispatch: {error}"))?;
        self.authorize_host_write(session_id, lease_secret)?;
        self.stop_host_for_explicit_close_under_gate(session_id)?;
        self.close_session_under_gate(session_id)
    }

    /// Stops the exact browser process before an explicit close frees its persistent profile.
    /// 在显式关闭释放持久画像前停止精确的浏览器进程。
    ///
    /// `session_id` selects a host while its dispatch gate is held; success proves that Chromium
    /// context closure was acknowledged and the owned host process has exited.
    /// `session_id` 在持有调度门时选中宿主；成功表示 Chromium 上下文关闭已获确认且归属宿主进程已退出。
    fn stop_host_for_explicit_close_under_gate(&self, session_id: &str) -> Result<(), String> {
        if !self.has_live_host(session_id) {
            return Ok(());
        }
        // Closing the control socket is not evidence that Chromium stopped; the host must first
        // acknowledge context closure while its authenticated channel is still connected.
        // 关闭控制套接字不能证明 Chromium 已停止；宿主必须先在鉴权通道仍连接时确认上下文关闭。
        let closed = match self.request_host(session_id, "host.closeContext", serde_json::json!({}))
        {
            Ok(BrowserHostRequestOutcome::Completed(result)) => {
                result.get("closed").and_then(serde_json::Value::as_bool) == Some(true)
            }
            _ => false,
        };
        if !closed {
            self.mark_operation_unconfirmed(session_id, "host.closeContext")?;
            return Err(format!(
                "{BROWSER_ERROR_LEASE_BLOCKED}: browser context closure was not confirmed"
            ));
        }
        {
            let mut state = self
                .inner
                .lock()
                .map_err(|error| format!("failed to lock browser manager: {error}"))?;
            let session = state
                .sessions
                .get_mut(session_id)
                .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
            session.recovery_context_closed = true;
        }
        // Host shutdown may drop its own reply while exiting. The retained OS process handle,
        // rather than that reply, determines when this session may become Closed.
        // 宿主退出时可能来不及应答关闭请求；是否允许会话变为 Closed 由保留的操作系统进程句柄决定。
        let _ = self.request_host(session_id, "host.shutdown", serde_json::json!({}));
        if !self.wait_owned_host_exit(session_id, Duration::from_secs(7))? {
            self.owned_host(session_id)?
                .lock()
                .map_err(|error| format!("failed to lock browser host: {error}"))?
                .process
                .kill();
            if !self.wait_owned_host_exit(session_id, Duration::from_secs(2))? {
                self.mark_operation_unconfirmed(session_id, "host.shutdown")?;
                return Err(format!(
                    "{BROWSER_ERROR_LEASE_BLOCKED}: browser host process did not exit"
                ));
            }
        }
        Ok(())
    }

    /// Applies closure while the per-session dispatch gate is already held.
    /// 在每会话派发门已被持有时执行关闭。
    fn close_session_under_gate(
        &self,
        session_id: &str,
    ) -> Result<BrowserSessionProjection, String> {
        let mut state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        // The explicit-close caller already observed the exact host process exit. Only now may
        // ownership be dropped and the persistent profile slot become available again.
        // 显式关闭调用方已观察到精确宿主进程退出；只有此时才能丢弃归属并重新开放持久画像位置。
        if let Some(ownership) = self.take_owned_host(session_id) {
            drop(ownership);
        }
        let session = state
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
        // An explicit close is the only path that ends the browser context.
        // 显式结束是唯一结束浏览器上下文的路径。
        let was_closed = session.projection.status == BrowserSessionStatus::Closed;
        session.supervision.record_closed();
        session.projection.status = BrowserSessionStatus::Closed;
        session.projection.host_state = BrowserHostSupervisionState::Closed;
        session.projection.page_count = 0;
        session.projection.requires_explicit_reopen = false;
        session.projection.lease_state = BrowserLeaseState::Free;
        session.projection.blocked_reason = None;
        session.projection.automation_state = BrowserAutomationState::Detached;
        session.lease_holder = None;
        session.lease_secret = None;
        session.lease_expires_at = None;
        session.automation_connection_id = None;
        session.recovery_context_closed = false;
        session.unconfirmed_operations.clear();
        session.queued_inputs.clear();
        session.observation_frames.clear();
        session.observation_pages.clear();
        // A closed session must revoke unconsumed one-time stream grants as well as active sockets.
        // 已关闭会话必须同时撤销未消费的一次性流授权与活动套接字。
        session.stream_tickets.clear();
        // A closed history record cannot own a live or cancelled socket identity. Late releases
        // remain harmless because they no longer find a connection-owned subscription.
        // 已关闭的历史记录不能持有活动或已取消的套接字标识；迟到释放找不到连接所属订阅即可安全返回。
        session.stream_subscriptions.clear();
        session.cancelled_stream_subscriptions.clear();
        session.uploads.clear();
        let projection = session.projection.clone();
        state
            .ai_selected_sessions
            .retain(|_, selected| selected != session_id);
        if !was_closed {
            state
                .closed_session_order
                .push_back(String::from(session_id));
        }
        // Retain each evicted record's read gate until its on-disk downloads are removed.
        // 保留每条被驱逐记录的读取门，直到其磁盘下载文件清理完成。
        let mut evicted_downloads = Vec::new();
        while state.closed_session_order.len() > BROWSER_MAX_RETAINED_CLOSED_SESSIONS {
            if let Some(oldest) = state.closed_session_order.pop_front() {
                if state
                    .sessions
                    .get(&oldest)
                    .is_some_and(|record| record.projection.status == BrowserSessionStatus::Closed)
                {
                    if let Some(record) = state.sessions.remove(&oldest) {
                        evicted_downloads.push((oldest, record.download_gate));
                    }
                }
            }
        }
        drop(state);
        // Host shutdown precedes temporary-file removal; neither filesystem I/O nor a failed
        // cleanup may hold the global manager lock or reopen the already closed browser.
        // 宿主退出后才移除临时文件；文件系统操作及清理失败均不能占用全局管理锁或重开已关闭浏览器。
        self.cleanup_closed_session_temp_data(session_id, projection.profile_mode);
        for (evicted_id, download_gate) in evicted_downloads {
            match download_gate.lock() {
                Ok(_read) => self.cleanup_evicted_session_control_data(&evicted_id),
                Err(error) => {
                    tracing::warn!(session_id = %evicted_id, %error, "browser download gate was poisoned during history cleanup");
                }
            }
        }
        Ok(projection)
    }

    /// Removes only temporary upload and ephemeral-profile data after a confirmed session close.
    /// 仅在会话确认关闭后移除上传暂存和临时画像数据。
    ///
    /// `session_id` is the Core-generated UUID and `profile_mode` selects whether the profile is
    /// disposable. Cleanup failures are logged because the browser context is already closed;
    /// the service startup cleanup may retry orphaned temporary directories.
    /// `session_id` 是 Core 生成的 UUID，`profile_mode` 决定画像是否可删除。浏览器上下文此时已关闭，
    /// 清理失败只记日志；服务启动清理可重试遗留临时目录。
    fn cleanup_closed_session_temp_data(&self, session_id: &str, profile_mode: BrowserProfileMode) {
        // The configured roots are the only authority for this session's disposable files.
        // 已配置的根目录是该会话可丢弃文件的唯一归属依据。
        let config = match self.host_runtime_config() {
            Ok(Some(config)) => config,
            Ok(None) => return,
            Err(error) => {
                tracing::warn!(session_id, %error, "browser temporary-data cleanup could not read host configuration");
                return;
            }
        };
        // Staged uploads always expire; only ephemeral profiles expire with the session.
        // 上传暂存始终随会话失效；仅临时画像随会话失效。
        let mut directories = vec![config.staging_root.join(session_id)];
        if profile_mode == BrowserProfileMode::Ephemeral {
            directories.push(config.profiles_root.join("ephemeral").join(session_id));
        }
        // Each path names a Core-generated session child, never a client-selected directory.
        // 每条路径都指向 Core 生成的会话子目录，绝不采用客户端指定的目录。
        for directory in directories {
            match std::fs::remove_dir_all(&directory) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    tracing::warn!(session_id, path = %directory.display(), %error, "browser temporary-data cleanup failed");
                }
            }
        }
    }

    /// Removes control files only after a closed browser history record loses download access.
    /// 仅在已关闭浏览器的历史记录失去下载访问能力后移除控制文件。
    ///
    /// `session_id` is a Core-generated UUID. The caller holds the record's download gate, so a
    /// reader that already began can finish before its files are deleted. Errors are logged and
    /// startup cleanup can retry them after this Core exits.
    /// `session_id` 是 Core 生成的 UUID。调用方持有该记录的下载门，已开始的读取可在删除文件前完成。
    /// 错误会被记录；此 Core 退出后的启动清理可重试。
    fn cleanup_evicted_session_control_data(&self, session_id: &str) {
        // The configured control root is the only place that may hold this browser's downloads.
        // 已配置的控制根目录是该浏览器下载文件唯一允许存放的位置。
        let config = match self.host_runtime_config() {
            Ok(Some(config)) => config,
            Ok(None) => return,
            Err(error) => {
                tracing::warn!(session_id, %error, "browser history cleanup could not read host configuration");
                return;
            }
        };
        // A generated session UUID selects exactly one former control directory.
        // Core 生成的会话 UUID 精确选中一个旧控制目录。
        let directory = config.profiles_root.join("control").join(session_id);
        match std::fs::remove_dir_all(&directory) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                tracing::warn!(session_id, path = %directory.display(), %error, "browser history control-data cleanup failed");
            }
        }
    }

    /// Navigates one registered page only after its current lease and host confirm the write.
    /// 仅在当前租约和宿主确认写入后导航一个已登记页面。
    ///
    /// # Parameters / 参数
    /// `page_id` is the Core identity; `lease_secret` is the current exclusive control grant.
    /// `page_id` 是 Core 身份；`lease_secret` 是当前独占控制授权。
    ///
    /// # Returns / 返回值
    /// The session projection after the host-confirmed navigation.
    /// 宿主确认导航后的会话投影。
    pub fn navigate_page_on_host(
        &self,
        session_id: &str,
        page_id: &str,
        url: &str,
        lease_secret: &str,
    ) -> Result<BrowserSessionProjection, String> {
        validate_browser_url(url)?;
        let gate = self.dispatch_gate(session_id)?;
        let _dispatch = gate
            .lock()
            .map_err(|error| format!("failed to lock browser dispatch: {error}"))?;
        self.sync_host_events(session_id)?;
        let target_id = self.authorized_page_target(session_id, page_id, lease_secret)?;
        let result = match self.request_host(
            session_id,
            "page.navigate",
            serde_json::json!({ "targetId": target_id, "url": url }),
        )? {
            BrowserHostRequestOutcome::Completed(result) => result,
            BrowserHostRequestOutcome::Failed { code, message } => {
                return Err(format!("{code}: {message}"));
            }
            BrowserHostRequestOutcome::Unknown { reason } => {
                self.mark_operation_unconfirmed(session_id, "page.navigate")?;
                return Err(format!("{BROWSER_ERROR_LEASE_BLOCKED}: {reason}"));
            }
        };
        let confirmed_target = result
            .get("targetId")
            .and_then(|value| value.as_str())
            .ok_or_else(|| String::from(BROWSER_ERROR_INVALID_REQUEST))?;
        let confirmed_url = result
            .get("url")
            .and_then(|value| value.as_str())
            .ok_or_else(|| String::from(BROWSER_ERROR_INVALID_REQUEST))?;
        if confirmed_target != target_id || confirmed_url.is_empty() {
            self.mark_operation_unconfirmed(session_id, "page.navigate")?;
            return Err(String::from(BROWSER_ERROR_LEASE_BLOCKED));
        }
        self.adopt_page(session_id, &target_id, confirmed_url, None)?;
        let mut state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let session = state
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
        session.projection.current_url = Some(confirmed_url.to_string());
        session.projection.page_generation = session.projection.page_generation.wrapping_add(1);
        session.queued_inputs.clear();
        Ok(session.projection.clone())
    }

    /// Resolves a page target only for the holder of the current, unblocked lease.
    /// 仅为当前未阻塞租约的持有者解析页面目标。
    ///
    /// # Returns / 返回值
    /// The target id belonging to the current host generation.
    /// 属于当前宿主代次的目标标识。
    fn authorized_page_target(
        &self,
        session_id: &str,
        page_id: &str,
        lease_secret: &str,
    ) -> Result<String, String> {
        let mut state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let session = state
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
        expire_lease_if_needed(session);
        if session.projection.status != BrowserSessionStatus::Running {
            return Err(String::from(BROWSER_ERROR_INVALID_REQUEST));
        }
        if session.projection.lease_state == BrowserLeaseState::Blocked {
            return Err(String::from(BROWSER_ERROR_LEASE_BLOCKED));
        }
        if session.projection.lease_state != BrowserLeaseState::Held
            || session.lease_secret.as_deref() != Some(lease_secret)
        {
            return Err(String::from(BROWSER_ERROR_LEASE_REQUIRED));
        }
        let page = session
            .supervision
            .pages()
            .target_for_page(page_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_INVALID_REQUEST))?;
        if page.host_generation != session.projection.host_generation {
            return Err(String::from(BROWSER_ERROR_STALE_GENERATION));
        }
        Ok(page.target_id.clone())
    }

    /// Checks an exclusive lease for a session-wide host write.
    /// 检查一个会话级宿主写入的独占租约。
    ///
    /// # Returns / 返回值
    /// Success only while the named holder may write to the running session.
    /// 只有指定持有者可写入正在运行的会话时才成功。
    fn authorize_host_write(&self, session_id: &str, lease_secret: &str) -> Result<(), String> {
        let mut state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let session = state
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
        expire_lease_if_needed(session);
        if session.projection.status != BrowserSessionStatus::Running {
            return Err(String::from(BROWSER_ERROR_INVALID_REQUEST));
        }
        if session.projection.lease_state == BrowserLeaseState::Blocked {
            return Err(String::from(BROWSER_ERROR_LEASE_BLOCKED));
        }
        if session.projection.lease_state != BrowserLeaseState::Held
            || session.lease_secret.as_deref() != Some(lease_secret)
        {
            return Err(String::from(BROWSER_ERROR_LEASE_REQUIRED));
        }
        Ok(())
    }

    /// Acquires the exclusive control lease for one session.
    /// 获取一个会话的独占控制租约。
    pub fn acquire_lease(
        &self,
        session_id: &str,
        holder: &str,
        ttl: Option<Duration>,
    ) -> Result<(BrowserSessionProjection, String), String> {
        let gate = self.dispatch_gate(session_id)?;
        let _dispatch = gate
            .lock()
            .map_err(|error| format!("failed to lock browser dispatch: {error}"))?;
        let mut state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let session = state
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
        expire_lease_if_needed(session);
        if !matches!(session.projection.status, BrowserSessionStatus::Running) {
            return Err(String::from(BROWSER_ERROR_INVALID_REQUEST));
        }
        if session.projection.automation_state == BrowserAutomationState::Attached {
            return Err(String::from(BROWSER_ERROR_AUTOMATION_CONFLICT));
        }
        if session.projection.lease_state != BrowserLeaseState::Free {
            return Err(String::from(BROWSER_ERROR_LEASE_CONFLICT));
        }
        if holder.trim().is_empty() {
            return Err(String::from(BROWSER_ERROR_INVALID_REQUEST));
        }
        drop(state);
        self.release_host_inputs(session_id)?;
        let mut state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let session = state
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
        if session.projection.status != BrowserSessionStatus::Running {
            return Err(String::from(BROWSER_ERROR_INVALID_REQUEST));
        }
        if session.projection.automation_state == BrowserAutomationState::Attached {
            return Err(String::from(BROWSER_ERROR_AUTOMATION_CONFLICT));
        }
        if session.projection.lease_state != BrowserLeaseState::Free {
            return Err(String::from(BROWSER_ERROR_LEASE_CONFLICT));
        }
        let secret = Uuid::new_v4().to_string();
        session.projection.lease_state = BrowserLeaseState::Held;
        session.projection.control_epoch = session.projection.control_epoch.wrapping_add(1);
        session.lease_holder = Some(holder.to_string());
        session.lease_secret = Some(secret.clone());
        session.lease_expires_at = Some(Instant::now() + ttl.unwrap_or(BROWSER_USER_LEASE_TTL));
        Ok((session.projection.clone(), secret))
    }

    /// Takes control of one session explicitly, revoking the previous holder.
    /// 明确接管一个会话的控制权，并撤销前任持有者。
    ///
    /// # Parameters
    /// 参数
    /// `holder` is the new controller identity and `ttl` optionally overrides the lease lifetime.
    /// `holder` 是新控制者身份，`ttl` 可选地覆盖租约时长。
    ///
    /// # Returns
    /// 返回值
    /// The rotated projection and the new lease secret.
    /// 返回轮换后的投影与新租约凭据。
    ///
    /// # Design note / 设计说明
    /// A takeover is never silently granted. If any in-flight operation could not be confirmed
    /// stopped, the session becomes `blocked` and **no** new lease is issued, because the previous
    /// holder's work may still be mutating the same page.
    /// 接管绝不静默授予。若任何在途操作无法确认已停止，会话进入 `blocked` 且**不**签发新租约，
    /// 因为前任持有者的工作可能仍在改动同一页面。
    pub fn takeover_lease(
        &self,
        session_id: &str,
        holder: &str,
        ttl: Option<Duration>,
    ) -> Result<(BrowserSessionProjection, String), String> {
        let gate = self.dispatch_gate(session_id)?;
        let _dispatch = gate
            .lock()
            .map_err(|error| format!("failed to lock browser dispatch: {error}"))?;
        let mut state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let session = state
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
        if holder.trim().is_empty() {
            return Err(String::from(BROWSER_ERROR_INVALID_REQUEST));
        }
        if !matches!(session.projection.status, BrowserSessionStatus::Running) {
            return Err(String::from(BROWSER_ERROR_INVALID_REQUEST));
        }
        if session.projection.automation_state == BrowserAutomationState::Attached {
            return Err(String::from(BROWSER_ERROR_AUTOMATION_CONFLICT));
        }
        if !session.unconfirmed_operations.is_empty() {
            // The block is made visible and durable: it is not a transient refusal that a retry
            // could silently bypass while the same work is still running.
            // 该阻塞是可见且持久的：它不是一次可被重试静默绕过的临时拒绝，因为同一工作仍在运行。
            let reason = session.unconfirmed_operations.join("; ");
            session.projection.lease_state = BrowserLeaseState::Blocked;
            session.projection.blocked_reason = Some(reason.clone());
            // The existing holder keeps its lease: nothing is fabricated for the newcomer.
            // 既有持有者保留其租约：绝不向新来者伪造任何东西。
            return Err(format!("{BROWSER_ERROR_LEASE_BLOCKED}: {reason}"));
        }
        expire_lease_if_needed(session);
        drop(state);
        self.release_host_inputs(session_id)?;
        let mut state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let session = state
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
        if session.projection.status != BrowserSessionStatus::Running {
            return Err(String::from(BROWSER_ERROR_INVALID_REQUEST));
        }
        if session.projection.automation_state == BrowserAutomationState::Attached {
            return Err(String::from(BROWSER_ERROR_AUTOMATION_CONFLICT));
        }
        let secret = Uuid::new_v4().to_string();
        // Rotating the secret is what makes the previous holder's later commands fail.
        // 轮换凭据正是使前任持有者后续命令失效的机制。
        session.projection.lease_state = BrowserLeaseState::Held;
        session.projection.blocked_reason = None;
        session.projection.control_epoch = session.projection.control_epoch.wrapping_add(1);
        session.lease_holder = Some(holder.to_string());
        session.lease_secret = Some(secret.clone());
        session.lease_expires_at = Some(Instant::now() + ttl.unwrap_or(BROWSER_USER_LEASE_TTL));
        Ok((session.projection.clone(), secret))
    }

    /// Records that one in-flight operation could not be confirmed stopped.
    /// 记录一项无法确认已停止的在途操作。
    ///
    /// # Returns
    /// 返回值
    /// The updated projection, whose lease state becomes `blocked` while control is held.
    /// 返回更新后的投影；控制权被持有时其租约状态变为 `blocked`。
    pub fn mark_operation_unconfirmed(
        &self,
        session_id: &str,
        operation: &str,
    ) -> Result<BrowserSessionProjection, String> {
        let mut state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let session = state
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
        if !session
            .unconfirmed_operations
            .iter()
            .any(|known| known == operation)
        {
            session.unconfirmed_operations.push(String::from(operation));
        }
        let reason = session.unconfirmed_operations.join("; ");
        session.projection.lease_state = BrowserLeaseState::Blocked;
        session.projection.blocked_reason = Some(reason);
        Ok(session.projection.clone())
    }

    /// Records that in-flight work for one session was observed to have stopped.
    /// 记录某个会话的在途工作已被观察到停止。
    ///
    /// # Returns
    /// 返回值
    /// The updated projection with the block cleared.
    /// 返回已清除阻塞的更新后投影。
    pub fn confirm_operations_stopped(
        &self,
        session_id: &str,
        operation: Option<&str>,
    ) -> Result<BrowserSessionProjection, String> {
        let mut state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let session = state
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
        let automation_attached =
            session.projection.automation_state == BrowserAutomationState::Attached;
        match operation {
            // Confirming one operation clears only that operation.
            // 确认单项操作只清除该项。
            Some(operation) => session.unconfirmed_operations.retain(|known| {
                known != operation
                    || (known == BROWSER_AUTOMATION_LEASE_EXPIRED_OPERATION && automation_attached)
            }),
            // Confirming without naming an operation means the drain finished.
            // 未指明操作的确认表示排空已完成。
            None => {
                if automation_attached {
                    session
                        .unconfirmed_operations
                        .retain(|known| known == BROWSER_AUTOMATION_LEASE_EXPIRED_OPERATION);
                } else {
                    session.unconfirmed_operations.clear();
                }
            }
        }
        if session.unconfirmed_operations.is_empty() {
            session.projection.blocked_reason = None;
            // Control returns to the holder that still owns the lease, or to `free` if none does.
            // 控制权回到仍持有租约的持有者；若无人持有则回到 `free`。
            session.projection.lease_state = if session.lease_secret.is_some() {
                BrowserLeaseState::Held
            } else {
                BrowserLeaseState::Free
            };
        }
        Ok(session.projection.clone())
    }

    /// Returns the in-flight operations that are still unconfirmed for one session.
    /// 返回某个会话仍未确认的在途操作。
    pub fn unconfirmed_operations(&self, session_id: &str) -> Result<Vec<String>, String> {
        let state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let session = state
            .sessions
            .get(session_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
        Ok(session.unconfirmed_operations.clone())
    }

    /// Releases the exclusive control lease for one session.
    /// 释放一个会话的独占控制租约。
    pub fn release_lease(
        &self,
        session_id: &str,
        lease_secret: &str,
    ) -> Result<BrowserSessionProjection, String> {
        let gate = self.dispatch_gate(session_id)?;
        let _dispatch = gate
            .lock()
            .map_err(|error| format!("failed to lock browser dispatch: {error}"))?;
        let mut state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let session = state
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
        if session.lease_secret.as_deref() != Some(lease_secret) {
            return Err(String::from(BROWSER_ERROR_LEASE_REQUIRED));
        }
        if session.projection.automation_state == BrowserAutomationState::Attached {
            return Err(String::from(BROWSER_ERROR_AUTOMATION_CONFLICT));
        }
        drop(state);
        self.release_host_inputs(session_id)?;
        let mut state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let session = state
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
        if session.projection.automation_state == BrowserAutomationState::Attached {
            return Err(String::from(BROWSER_ERROR_AUTOMATION_CONFLICT));
        }
        // Releasing credentials cannot prove that an unknown operation has stopped.
        // 释放凭据不能证明结果未知的操作已经停止。
        session.projection.lease_state = if session.unconfirmed_operations.is_empty() {
            BrowserLeaseState::Free
        } else {
            BrowserLeaseState::Blocked
        };
        session.projection.control_epoch = session.projection.control_epoch.wrapping_add(1);
        session.lease_holder = None;
        session.lease_secret = None;
        session.lease_expires_at = None;
        session.queued_inputs.clear();
        Ok(session.projection.clone())
    }

    /// Heartbeats one held lease before the service monotonic expiry.
    /// 在服务单调到期前为一个持有租约发送心跳。
    pub fn heartbeat_lease(
        &self,
        session_id: &str,
        lease_secret: &str,
    ) -> Result<BrowserSessionProjection, String> {
        self.heartbeat_lease_with_ttl(session_id, lease_secret, BROWSER_USER_LEASE_TTL)
    }

    /// Renews an existing exclusive lease for a caller-specific, bounded lifetime.
    /// 按调用方指定的有界时长续订既有独占租约。
    ///
    /// `session_id` identifies the browser, `lease_secret` proves current ownership, and
    /// `ttl` is the lifetime from this heartbeat. Returns the updated projection.
    /// `session_id` 标识浏览器，`lease_secret` 证明当前所有权，`ttl` 是从本次心跳起的
    /// 有效时长。返回更新后的投影。
    pub(crate) fn heartbeat_lease_with_ttl(
        &self,
        session_id: &str,
        lease_secret: &str,
        ttl: Duration,
    ) -> Result<BrowserSessionProjection, String> {
        if ttl.is_zero() {
            return Err(String::from(BROWSER_ERROR_INVALID_REQUEST));
        }
        let next_expiry = Instant::now()
            .checked_add(ttl)
            .ok_or_else(|| String::from(BROWSER_ERROR_INVALID_REQUEST))?;
        let gate = self.dispatch_gate(session_id)?;
        let _dispatch = gate
            .lock()
            .map_err(|error| format!("failed to lock browser dispatch: {error}"))?;
        let mut state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let session = state
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
        if session.lease_secret.as_deref() != Some(lease_secret) {
            return Err(String::from(BROWSER_ERROR_LEASE_REQUIRED));
        }
        expire_lease_if_needed(session);
        if session.projection.lease_state != BrowserLeaseState::Held {
            return Err(String::from(BROWSER_ERROR_LEASE_REQUIRED));
        }
        session.lease_expires_at = Some(next_expiry);
        Ok(session.projection.clone())
    }

    /// Dispatches one leased input event after generation and sequence checks.
    /// 在代次与序号检查后派发一个租约输入事件。
    pub fn dispatch_input(
        &self,
        session_id: &str,
        lease_secret: &str,
        page_generation: u64,
        event: BrowserInputEvent,
    ) -> Result<u64, String> {
        // Legacy convenience form: it dispatches against the session's current viewport and turns a
        // rejection back into the stable reason code, so there is exactly one implementation of the
        // checks themselves.
        // 旧版便捷形式：针对会话当前视口派发，并把拒绝还原为稳定原因码，使检查逻辑只有一份实现。
        let viewport_generation = self.session_projection(session_id)?.viewport_generation;
        let outcome = self.dispatch_input_outcome(
            session_id,
            lease_secret,
            page_generation,
            viewport_generation,
            event,
        )?;
        match outcome.outcome {
            BrowserInputOutcome::Rejected => Err(outcome
                .reason
                .unwrap_or_else(|| String::from(BROWSER_ERROR_INVALID_REQUEST))),
            _ => outcome
                .sequence
                .ok_or_else(|| String::from(BROWSER_ERROR_INVALID_REQUEST)),
        }
    }

    /// Establishes a viewport and advances its generation.
    /// 建立视口并推进其代次。
    ///
    /// # Parameters
    /// 参数
    /// `width` and `height` are device-independent pixels.
    /// `width` 与 `height` 以设备无关像素为单位。
    ///
    /// # Returns
    /// 返回值
    /// The projection carrying the new viewport generation.
    /// 携带新视口代次的投影。
    ///
    /// # Design note / 设计说明
    /// Pointer coordinates are meaningful only for the viewport they were measured in, so changing
    /// the viewport invalidates every coordinate computed before it.
    /// 指针坐标只对其被测量时的视口有意义，因此改变视口会使此前计算的每个坐标失效。
    fn set_viewport(
        &self,
        session_id: &str,
        width: u32,
        height: u32,
    ) -> Result<BrowserSessionProjection, String> {
        if width == 0 || height == 0 {
            return Err(String::from(BROWSER_ERROR_INVALID_REQUEST));
        }
        let mut state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let session = state
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
        session.projection.viewport_generation =
            session.projection.viewport_generation.wrapping_add(1);
        session.projection.viewport_size = Some(BrowserViewportSize { width, height });
        // Coordinates queued for the previous viewport must not reach the new one.
        // 为前一个视口排队的坐标绝不能进入新视口。
        session.queued_inputs.clear();
        Ok(session.projection.clone())
    }

    /// Applies a viewport to the real host before advancing Core's viewport generation.
    /// 先把视口应用于真实宿主，再推进 Core 的视口代次。
    ///
    /// # Parameters / 参数
    /// `width` and `height` are requested content pixels; `lease_secret` authorizes the write.
    /// `width` 与 `height` 是请求的内容像素；`lease_secret` 授权写入。
    ///
    /// # Returns / 返回值
    /// A projection whose dimensions the host confirmed for every open page.
    /// 所有已打开页面的尺寸均获宿主确认的投影。
    pub fn set_host_viewport(
        &self,
        session_id: &str,
        width: u32,
        height: u32,
        lease_secret: &str,
    ) -> Result<BrowserSessionProjection, String> {
        if width == 0 || height == 0 || width > 4096 || height > 4096 {
            return Err(String::from(BROWSER_ERROR_INVALID_REQUEST));
        }
        let gate = self.dispatch_gate(session_id)?;
        let _dispatch = gate
            .lock()
            .map_err(|error| format!("failed to lock browser dispatch: {error}"))?;
        self.authorize_host_write(session_id, lease_secret)?;
        let result = match self.request_host(
            session_id,
            "viewport.set",
            serde_json::json!({ "width": width, "height": height }),
        )? {
            BrowserHostRequestOutcome::Completed(result) => result,
            BrowserHostRequestOutcome::Failed { code, message } => {
                self.mark_operation_unconfirmed(session_id, "viewport.set")?;
                return Err(format!("{BROWSER_ERROR_LEASE_BLOCKED}: {code}: {message}"));
            }
            BrowserHostRequestOutcome::Unknown { reason } => {
                self.mark_operation_unconfirmed(session_id, "viewport.set")?;
                return Err(format!("{BROWSER_ERROR_LEASE_BLOCKED}: {reason}"));
            }
        };
        let confirmed_width = result.get("width").and_then(|value| value.as_u64());
        let confirmed_height = result.get("height").and_then(|value| value.as_u64());
        if confirmed_width != Some(u64::from(width)) || confirmed_height != Some(u64::from(height))
        {
            self.mark_operation_unconfirmed(session_id, "viewport.set")?;
            return Err(String::from(BROWSER_ERROR_LEASE_BLOCKED));
        }
        self.set_viewport(session_id, width, height)
    }

    /// Dispatches one input event and reports its real outcome.
    /// 派发一个输入事件并报告其真实结果。
    ///
    /// # Parameters
    /// 参数
    /// `page_generation` and `viewport_generation` are the generations the input was computed
    /// against; `event` carries the caller's own sequence for correlation.
    /// `page_generation` 与 `viewport_generation` 是输入据以计算的代次；`event` 携带调用方自身的
    /// 序号用于关联。
    ///
    /// # Returns
    /// 返回值
    /// `Rejected` with a stable reason when any check fails, otherwise `Accepted` with the
    /// Core-assigned sequence. Only an unknown session is a hard error, because a rejection is a
    /// normal protocol result rather than a transport failure.
    /// 任何检查失败时返回带稳定原因的 `Rejected`，否则返回带 Core 分配序号的 `Accepted`。只有未知会话
    /// 才是硬错误，因为拒绝是正常协议结果而非传输失败。
    pub fn dispatch_input_outcome(
        &self,
        session_id: &str,
        lease_secret: &str,
        page_generation: u64,
        viewport_generation: u64,
        event: BrowserInputEvent,
    ) -> Result<BrowserInputDispatchOutcome, String> {
        let mut state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let session = state
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
        // Every check returns `Rejected` rather than an error, so a client can distinguish "the
        // service refused this input" from "the service itself failed".
        // 每项检查都返回 `Rejected` 而不是错误，使客户端能区分“服务拒绝了该输入”与“服务本身失败”。
        let reject = |reason: &str| BrowserInputDispatchOutcome {
            outcome: BrowserInputOutcome::Rejected,
            sequence: None,
            reason: Some(String::from(reason)),
            detail: None,
        };
        // A retained secret identifies its former holder; it does not authorize work while blocked.
        // 保留的凭据标识原持有者，但不能在阻塞状态下授权工作。
        if session.projection.status != BrowserSessionStatus::Running {
            return Ok(reject(BROWSER_ERROR_INVALID_REQUEST));
        }
        expire_lease_if_needed(session);
        if session.projection.lease_state == BrowserLeaseState::Blocked {
            return Ok(reject(BROWSER_ERROR_LEASE_BLOCKED));
        }
        if session.projection.lease_state != BrowserLeaseState::Held {
            return Ok(reject(BROWSER_ERROR_LEASE_REQUIRED));
        }
        if session.lease_secret.as_deref() != Some(lease_secret) {
            return Ok(reject(BROWSER_ERROR_LEASE_REQUIRED));
        }
        if page_generation != session.projection.page_generation {
            return Ok(reject(BROWSER_ERROR_STALE_GENERATION));
        }
        if viewport_generation != session.projection.viewport_generation {
            return Ok(reject(BROWSER_ERROR_STALE_VIEWPORT));
        }
        if event.sequence.trim().is_empty() {
            return Ok(reject(BROWSER_ERROR_INVALID_REQUEST));
        }
        if session.queued_inputs.len() >= BROWSER_MAX_QUEUED_INPUTS {
            // The oldest sequenced input is dropped together with its recorded outcome, so the
            // outcome map stays bounded exactly like the queue.
            // 最旧的已排序输入连同其结果记录一起被丢弃，使结果映射与队列一样保持有界。
            if let Some(dropped) = session.queued_inputs.pop_front() {
                let _ = dropped;
            }
        }
        session.queued_inputs.push_back(event);
        session.next_input_sequence = session.next_input_sequence.wrapping_add(1);
        let sequence = session.next_input_sequence.wrapping_sub(1);
        session
            .input_outcomes
            .insert(sequence, BrowserInputOutcome::Accepted);
        while session.input_outcomes.len() > BROWSER_MAX_QUEUED_INPUTS {
            let oldest = session.input_outcomes.keys().next().copied();
            match oldest {
                Some(oldest) => {
                    session.input_outcomes.remove(&oldest);
                }
                None => break,
            }
        }
        Ok(BrowserInputDispatchOutcome {
            outcome: BrowserInputOutcome::Accepted,
            sequence: Some(sequence),
            reason: None,
            detail: None,
        })
    }

    /// Stages uploaded content under Core's own staging root.
    /// 将上传内容暂存到 Core 自有的暂存根目录下。
    ///
    /// # Parameters
    /// 参数
    /// `name` is the client-supplied display name and `data` is the content itself.
    /// `name` 是客户端提供的展示名称，`data` 是内容本身。
    ///
    /// # Returns
    /// 返回值
    /// The staged record, whose path Core chose and whose digest Core computed.
    /// 返回暂存记录，其路径由 Core 选择、摘要由 Core 计算。
    ///
    /// # Design note / 设计说明
    /// The client supplies content and a display name, never a filesystem path. The staged path is
    /// built from a Core-generated identity, so a name containing separators or traversal cannot
    /// reach outside the session's staging directory.
    /// 客户端提供内容与展示名称，绝不提供文件系统路径。暂存路径由 Core 生成的标识构建，因此包含分隔符
    /// 或路径穿越的名称无法离开该会话的暂存目录。
    /// The caller holds this session's dispatch gate while checking the quota and writing the file.
    /// 调用方在检查额度及写入文件时持有本会话的派发门。
    fn stage_upload(
        &self,
        session_id: &str,
        name: &str,
        data: Vec<u8>,
    ) -> Result<BrowserUploadRecord, String> {
        if data.len() > BROWSER_MAX_UPLOAD_BYTES {
            return Err(String::from(BROWSER_ERROR_INVALID_REQUEST));
        }
        let config = self
            .host_runtime_config()?
            .ok_or_else(|| String::from(BROWSER_ERROR_CAPABILITY_UNSUPPORTED))?;
        // The service RPC already holds this session's dispatch gate, so no concurrent close or
        // upload can change this quota before its file is recorded.
        // 服务 RPC 已持有本会话的派发门，因此并发关闭或上传不能在文件入账前改变此额度。
        {
            let state = self
                .inner
                .lock()
                .map_err(|error| format!("failed to lock browser manager: {error}"))?;
            let session = state
                .sessions
                .get(session_id)
                .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
            if session.projection.status != BrowserSessionStatus::Running {
                return Err(String::from(BROWSER_ERROR_INVALID_REQUEST));
            }
            let staged_bytes = session.uploads.values().fold(0usize, |total, record| {
                total.saturating_add(record.byte_length)
            });
            if session.uploads.len() >= BROWSER_MAX_STAGED_UPLOADS_PER_SESSION
                || staged_bytes.saturating_add(data.len())
                    > BROWSER_MAX_STAGED_UPLOAD_BYTES_PER_SESSION
            {
                return Err(String::from(BROWSER_ERROR_UPLOAD_LIMIT));
            }
        }
        // Only the base name is retained, so a traversal name cannot influence any path.
        // 仅保留基名，因此路径穿越名称无法影响任何路径。
        let display_name = name
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(name)
            .trim()
            .to_string();
        // Windows interprets a colon as an alternate data stream and device names as special
        // files; reject names whose on-disk identity could differ from Core's upload record.
        // Windows 会把冒号解释为附加数据流、把设备名解释为特殊文件；拒绝磁盘身份可能与 Core
        // 上传记录不一致的名称。
        let stem = display_name
            .split('.')
            .next()
            .unwrap_or_default()
            .trim_end();
        let reserved_windows_device = matches!(
            stem.to_ascii_uppercase().as_str(),
            "CON"
                | "PRN"
                | "AUX"
                | "NUL"
                | "CONIN$"
                | "CONOUT$"
                | "COM1"
                | "COM2"
                | "COM3"
                | "COM4"
                | "COM5"
                | "COM6"
                | "COM7"
                | "COM8"
                | "COM9"
                | "COM¹"
                | "COM²"
                | "COM³"
                | "LPT1"
                | "LPT2"
                | "LPT3"
                | "LPT4"
                | "LPT5"
                | "LPT6"
                | "LPT7"
                | "LPT8"
                | "LPT9"
                | "LPT¹"
                | "LPT²"
                | "LPT³"
        );
        let invalid_character = display_name.chars().any(|character| {
            character.is_control() || matches!(character, '<' | '>' | ':' | '"' | '|' | '?' | '*')
        });
        if display_name.is_empty()
            || display_name.len() > 255
            || display_name.ends_with('.')
            || invalid_character
            || reserved_windows_device
        {
            return Err(String::from(BROWSER_ERROR_INVALID_REQUEST));
        }
        let directory = config.staging_root.join(session_id);
        std::fs::create_dir_all(&directory).map_err(|error| {
            format!(
                "failed to create the browser staging directory {}: {error}",
                directory.display()
            )
        })?;
        let upload_id = Uuid::new_v4().to_string();
        // The directory is Core-chosen and the file keeps its display name inside it, so the page
        // sees the real filename while the client still never supplies a path.
        // 目录由 Core 选择，文件在其中保留其展示名称，因此页面看到真实文件名，而客户端仍从不提供路径。
        let upload_directory = directory.join(&upload_id);
        std::fs::create_dir_all(&upload_directory).map_err(|error| {
            format!(
                "failed to create the browser upload directory {}: {error}",
                upload_directory.display()
            )
        })?;
        let staged_path = upload_directory.join(&display_name);
        if let Err(error) = std::fs::write(&staged_path, &data) {
            // A partial write must not leave an untracked file outside the session quota.
            // 部分写入失败时不能留下不计入会话额度的文件。
            if let Err(cleanup_error) = std::fs::remove_dir_all(&upload_directory) {
                tracing::warn!(path = %upload_directory.display(), %cleanup_error, "failed to remove partial browser upload");
            }
            return Err(format!("failed to stage uploaded content: {error}"));
        }
        let record = BrowserUploadRecord {
            upload_id,
            display_name,
            staged_path: staged_path.to_string_lossy().into_owned(),
            byte_length: data.len(),
            sha256: browser_sha256_hex(&data),
        };
        // Registration can fail after the file is written, so keep the file only once Core owns it.
        // 文件写入后仍可能登记失败，因此仅在 Core 确认归属后保留该文件。
        let registration = (|| {
            let mut state = self
                .inner
                .lock()
                .map_err(|error| format!("failed to lock browser manager: {error}"))?;
            let session = state
                .sessions
                .get_mut(session_id)
                .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
            session
                .uploads
                .insert(record.upload_id.clone(), record.clone());
            Ok::<(), String>(())
        })();
        if let Err(error) = registration {
            if let Err(cleanup_error) = std::fs::remove_dir_all(&upload_directory) {
                tracing::warn!(path = %upload_directory.display(), %cleanup_error, "failed to remove unregistered browser upload");
            }
            return Err(error);
        }
        Ok(record)
    }

    /// Returns the staged uploads one session currently owns.
    /// 返回某个会话当前拥有的暂存上传。
    pub fn uploads(&self, session_id: &str) -> Result<Vec<BrowserUploadRecord>, String> {
        let state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let session = state
            .sessions
            .get(session_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
        Ok(session.uploads.values().cloned().collect())
    }

    /// Resolves only a UUID-named file under the Core-assigned session download root.
    /// 只解析 Core 分配的会话下载根下以 UUID 命名的文件。
    fn download_storage_path(
        &self,
        session_id: &str,
        download_id: &str,
    ) -> Result<std::path::PathBuf, String> {
        Uuid::parse_str(session_id).map_err(|_| String::from(BROWSER_ERROR_INVALID_REQUEST))?;
        Uuid::parse_str(download_id).map_err(|_| String::from(BROWSER_ERROR_INVALID_REQUEST))?;
        let config = self
            .host_runtime_config()?
            .ok_or_else(|| String::from(BROWSER_ERROR_CAPABILITY_UNSUPPORTED))?;
        Ok(config
            .profiles_root
            .join("control")
            .join(session_id)
            .join("downloads")
            .join(download_id))
    }

    /// Lists path-free downloads after applying pending host lifecycle events.
    /// 应用待处理宿主生命周期事件后列出不含路径的下载。
    pub fn downloads(&self, session_id: &str) -> Result<Vec<BrowserDownloadPublicRecord>, String> {
        self.sync_host_events(session_id)?;
        let state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let session = state
            .sessions
            .get(session_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
        Ok(session
            .downloads
            .values()
            .map(|record| record.public.clone())
            .collect())
    }

    /// Reads one completed download by opaque identity and verifies its retained digest.
    /// 按不透明身份读取一项已完成下载，并核验其保留摘要。
    pub fn download_content(
        &self,
        download_id: &str,
        owner_session_id: &str,
    ) -> Result<(BrowserDownloadPublicRecord, Vec<u8>), String> {
        Uuid::parse_str(download_id).map_err(|_| String::from(BROWSER_ERROR_INVALID_REQUEST))?;
        // Locate the owning session and its read gate without holding manager state during I/O.
        // 查找所属会话及其读取门，文件读写期间不持有管理器全局状态锁。
        let (session_id, download_gate) = {
            let state = self
                .inner
                .lock()
                .map_err(|error| format!("failed to lock browser manager: {error}"))?;
            state
                .sessions
                .values()
                .filter(|session| session.projection.owner_session_id == owner_session_id)
                .find_map(|session| {
                    session.downloads.contains_key(download_id).then(|| {
                        (
                            session.projection.session_id.clone(),
                            Arc::clone(&session.download_gate),
                        )
                    })
                })
                .ok_or_else(|| String::from(BROWSER_ERROR_INVALID_REQUEST))?
        };
        // Eviction takes the same gate before deleting this session's download directory.
        // 历史驱逐在删除本会话下载目录前也会取得同一读取门。
        let _read = download_gate
            .lock()
            .map_err(|error| format!("failed to lock browser download: {error}"))?;
        // Eviction may have removed the record while this reader waited for the gate.
        // 读取者等待门锁期间，历史驱逐可能已经删除记录。
        let record = {
            let state = self
                .inner
                .lock()
                .map_err(|error| format!("failed to lock browser manager: {error}"))?;
            state
                .sessions
                .get(&session_id)
                .filter(|session| session.projection.owner_session_id == owner_session_id)
                .and_then(|session| {
                    session
                        .downloads
                        .get(download_id)
                        .map(|record| record.public.clone())
                })
                .ok_or_else(|| String::from(BROWSER_ERROR_INVALID_REQUEST))?
        };
        if record.status != BrowserDownloadStatus::Completed {
            return Err(String::from(BROWSER_ERROR_INVALID_REQUEST));
        }
        let path = self.download_storage_path(&record.session_id, download_id)?;
        let metadata = std::fs::metadata(&path)
            .map_err(|error| format!("browser download is unavailable: {error}"))?;
        if metadata.len() > BROWSER_MAX_UPLOAD_BYTES as u64
            || Some(metadata.len()) != record.byte_length
        {
            return Err(String::from(BROWSER_ERROR_INVALID_REQUEST));
        }
        let bytes = std::fs::read(&path)
            .map_err(|error| format!("browser download is unavailable: {error}"))?;
        if record.sha256.as_deref() != Some(browser_sha256_hex(&bytes).as_str()) {
            return Err(String::from(BROWSER_ERROR_INVALID_REQUEST));
        }
        Ok((record, bytes))
    }

    /// Lists unresolved page dialogs without giving an observer authority to answer them.
    /// 列出未解决的页面对话框，而不授予观察者答复权限。
    pub fn dialogs(&self, session_id: &str) -> Result<Vec<BrowserDialogPublicRecord>, String> {
        self.sync_host_events(session_id)?;
        let state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let session = state
            .sessions
            .get(session_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
        Ok(session.dialogs.values().cloned().collect())
    }

    /// Lists pending file choosers without exposing a host DOM node identity.
    /// 列出待处理的文件选择器，而不暴露宿主 DOM 节点身份。
    pub fn file_choosers(
        &self,
        session_id: &str,
    ) -> Result<Vec<BrowserFileChooserPublicRecord>, String> {
        self.sync_host_events(session_id)?;
        let state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let session = state
            .sessions
            .get(session_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
        Ok(session.file_choosers.values().cloned().collect())
    }

    /// Answers one exact pending page dialog only under the current exclusive lease.
    /// 仅凭当前独占租约答复一个精确的待处理页面对话框。
    pub fn respond_dialog(
        &self,
        session_id: &str,
        page_id: &str,
        dialog_id: &str,
        lease_secret: &str,
        accept: bool,
        prompt_text: Option<&str>,
    ) -> Result<serde_json::Value, String> {
        if prompt_text.is_some_and(|text| text.chars().count() > 4096) {
            return Err(String::from(BROWSER_ERROR_INVALID_REQUEST));
        }
        let gate = self.dispatch_gate(session_id)?;
        let _dispatch = gate
            .lock()
            .map_err(|error| format!("failed to lock browser dispatch: {error}"))?;
        self.sync_host_events(session_id)?;
        let target_id = self.authorized_page_target(session_id, page_id, lease_secret)?;
        {
            let state = self
                .inner
                .lock()
                .map_err(|error| format!("failed to lock browser manager: {error}"))?;
            let dialog = state
                .sessions
                .get(session_id)
                .and_then(|session| session.dialogs.get(dialog_id))
                .ok_or_else(|| String::from(BROWSER_ERROR_INVALID_REQUEST))?;
            if dialog.page_id != page_id {
                return Err(String::from(BROWSER_ERROR_INVALID_REQUEST));
            }
        }
        match self.request_host(
            session_id,
            "dialog.respond",
            serde_json::json!({
                "targetId": target_id, "dialogId": dialog_id,
                "accept": accept, "promptText": prompt_text,
            }),
        )? {
            BrowserHostRequestOutcome::Completed(value) => {
                self.sync_host_events(session_id)?;
                Ok(value)
            }
            BrowserHostRequestOutcome::Failed { code, message } => {
                Err(format!("{code}: {message}"))
            }
            BrowserHostRequestOutcome::Unknown { reason } => {
                self.mark_operation_unconfirmed(session_id, "dialog.respond")?;
                Err(format!("{BROWSER_ERROR_LEASE_BLOCKED}: {reason}"))
            }
        }
    }

    /// Places one staged upload into a page's file input through the host.
    /// 通过宿主将一个暂存上传放入页面的文件输入中。
    ///
    /// # Returns
    /// 返回值
    /// The host's report, which names the staged path it used.
    /// 返回宿主的报告，其中标明其使用的暂存路径。
    pub fn attach_upload_to_page(
        &self,
        session_id: &str,
        upload_id: &str,
        target_id: &str,
        chooser_id: Option<&str>,
    ) -> Result<serde_json::Value, String> {
        let record = self
            .uploads(session_id)?
            .into_iter()
            .find(|record| record.upload_id == upload_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_INVALID_REQUEST))?;
        match self.request_host(
            session_id,
            "page.upload",
            serde_json::json!({
                "targetId": target_id,
                // The host receives the Core-staged path, never a client-supplied one.
                // 宿主收到的是 Core 暂存的路径，绝不是客户端提供的路径。
                "stagedPath": record.staged_path,
                "displayName": record.display_name,
                "chooserId": chooser_id,
            }),
        )? {
            BrowserHostRequestOutcome::Completed(value) => Ok(value),
            BrowserHostRequestOutcome::Failed { code, message } => {
                Err(format!("{code}: {message}"))
            }
            BrowserHostRequestOutcome::Unknown { reason } => {
                self.mark_operation_unconfirmed(session_id, "page.upload")?;
                Err(format!("{BROWSER_ERROR_LEASE_BLOCKED}: {reason}"))
            }
        }
    }

    /// Sends every accepted input to the session's host and records the real result.
    /// 将一个已接受的输入发送到会话宿主并记录真实结果。
    ///
    /// # Parameters
    /// 参数
    /// `event` is the accepted input whose outcome is being resolved.
    /// `event` 是正在被确定结果的已接受输入。
    ///
    /// # Returns
    /// 返回值
    /// `Applied` when the host confirmed the input reached the page, `Unknown` when the outcome
    /// cannot be determined, or `Rejected` when the session owns no live host to receive it.
    /// 宿主确认输入到达页面时返回 `Applied`；结果无法确定时返回 `Unknown`；会话没有活动宿主可接收时
    /// 返回 `Rejected`。
    ///
    /// # Design note / 设计说明
    /// An unknown outcome both marks the input unknown and blocks the session, because a takeover
    /// must not proceed while an input of unknown effect may still reach the page.
    /// 未知结果会同时把该输入标记为未知并使会话阻塞，因为在效果未知的输入可能仍会到达页面时不得进行接管。
    pub fn deliver_input_to_host(
        &self,
        session_id: &str,
        target_id: &str,
        page_generation: u64,
        viewport_generation: u64,
        sequence: u64,
        event: &BrowserInputEvent,
    ) -> Result<BrowserInputDispatchOutcome, String> {
        if !self.has_live_host(session_id) {
            return Ok(BrowserInputDispatchOutcome {
                outcome: BrowserInputOutcome::Rejected,
                sequence: Some(sequence),
                reason: Some(String::from(BROWSER_ERROR_CAPABILITY_UNSUPPORTED)),
                detail: None,
            });
        }
        let params = serde_json::json!({
            "kind": event.kind,
            "pageGeneration": page_generation.to_string(),
            "viewportGeneration": viewport_generation.to_string(),
            "payload": event.payload,
            "sequence": event.sequence,
        });
        // The payload is flattened so the host reads one flat parameter object rather than guessing
        // at a nested shape.
        // 载荷被展平，使宿主读取一个扁平参数对象，而无需猜测嵌套结构。
        let mut merged = params.as_object().cloned().unwrap_or_default();
        if let Some(payload) = event.payload.as_object() {
            for (key, value) in payload {
                merged.insert(key.clone(), value.clone());
            }
        }
        // Core's page registry overrides any targetId supplied inside untrusted input payload.
        // Core 页面注册表覆盖不可信输入载荷中提供的任何 targetId。
        merged.insert("targetId".to_string(), serde_json::json!(target_id));
        match self.request_host(
            session_id,
            "input.dispatch",
            serde_json::Value::Object(merged),
        ) {
            Ok(BrowserHostRequestOutcome::Completed(detail)) => {
                self.record_input_applied(session_id, sequence)?;
                Ok(BrowserInputDispatchOutcome {
                    outcome: BrowserInputOutcome::Applied,
                    sequence: Some(sequence),
                    reason: None,
                    // The host's own report is preserved so a client can see, for example, where a
                    // clamped pointer actually landed.
                    // 宿主自身的报告被保留，使客户端能看到例如被收敛的指针实际落点。
                    detail: Some(detail),
                })
            }
            Ok(BrowserHostRequestOutcome::Failed { code, message }) => {
                Ok(BrowserInputDispatchOutcome {
                    outcome: BrowserInputOutcome::Rejected,
                    sequence: Some(sequence),
                    reason: Some(format!("{code}: {message}")),
                    detail: None,
                })
            }
            Ok(BrowserHostRequestOutcome::Unknown { reason }) => {
                self.record_input_unknown(session_id, sequence)?;
                Ok(BrowserInputDispatchOutcome {
                    outcome: BrowserInputOutcome::Unknown,
                    sequence: Some(sequence),
                    reason: Some(reason),
                    detail: None,
                })
            }
            // No live host was reachable even though the registry claimed one: the honest answer is
            // that the input never reached a page.
            // 尽管注册表声称有宿主，却无法触达任何活动宿主：诚实的答复是该输入从未到达页面。
            Err(message) => Ok(BrowserInputDispatchOutcome {
                outcome: BrowserInputOutcome::Rejected,
                sequence: Some(sequence),
                reason: Some(message),
                detail: None,
            }),
        }
    }

    /// Records that the host applied one accepted input to the page.
    /// 记录宿主已将一个已接受输入应用到页面。
    pub fn record_input_applied(
        &self,
        session_id: &str,
        sequence: u64,
    ) -> Result<BrowserSessionProjection, String> {
        self.record_input_outcome(session_id, sequence, BrowserInputOutcome::Applied)
    }

    /// Records that one input's outcome could not be determined.
    /// 记录一个输入的结果无法确定。
    ///
    /// # Returns
    /// 返回值
    /// The updated projection; the session also becomes `blocked`, because a takeover must not
    /// proceed while an input of unknown effect may still be in flight.
    /// 返回更新后的投影；会话同时进入 `blocked`，因为在效果未知的输入可能仍在途中时不得进行接管。
    pub fn record_input_unknown(
        &self,
        session_id: &str,
        sequence: u64,
    ) -> Result<BrowserSessionProjection, String> {
        let operation = format!("input.dispatch:{sequence}");
        self.mark_operation_unconfirmed(session_id, &operation)?;
        self.record_input_outcome(session_id, sequence, BrowserInputOutcome::Unknown)
    }

    /// Returns the last known outcome of one input sequence.
    /// 返回某个输入序号最后已知的结果。
    pub fn input_outcome(
        &self,
        session_id: &str,
        sequence: u64,
    ) -> Result<Option<BrowserInputOutcome>, String> {
        let state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let session = state
            .sessions
            .get(session_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
        Ok(session.input_outcomes.get(&sequence).copied())
    }

    /// Writes one input outcome into the bounded per-sequence map.
    /// 将一个输入结果写入按序号索引的有界映射。
    fn record_input_outcome(
        &self,
        session_id: &str,
        sequence: u64,
        outcome: BrowserInputOutcome,
    ) -> Result<BrowserSessionProjection, String> {
        let mut state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let session = state
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
        session.input_outcomes.insert(sequence, outcome);
        Ok(session.projection.clone())
    }

    /// Releases one connection-owned observation subscription when its stream ends.
    /// 当流结束时释放一个由连接拥有的观察订阅。
    ///
    /// # Returns
    /// 返回值
    /// `true` when a connection-owned subscription was released, `false` when the identifier belongs
    /// to an explicit observer subscription that only its owner may release.
    /// 释放了连接所有的订阅时返回 `true`；标识属于只能由其所有者释放的显式观察订阅时返回 `false`。
    ///
    /// # Design note / 设计说明
    /// Closing one observer must never disturb another observer or the session, so this removes only
    /// subscriptions this stream created and leaves every other subscription's retained frames alone.
    /// 关闭一个观察端绝不干扰其他观察端或会话，因此这里只移除本流创建的订阅，其他订阅保留的帧不受影响。
    pub fn release_stream_subscription(
        &self,
        session_id: &str,
        subscription_id: &str,
    ) -> Result<bool, String> {
        let mut state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let session = state
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
        let active = session.stream_subscriptions.remove(subscription_id);
        let cancelled = session
            .cancelled_stream_subscriptions
            .remove(subscription_id);
        if !active && !cancelled {
            return Ok(false);
        }
        session.observation_frames.remove(subscription_id);
        session.observation_pages.remove(subscription_id);
        Ok(true)
    }

    /// Returns the identifiers of observation subscriptions one session currently retains.
    /// 返回某个会话当前保留的观察订阅标识。
    pub fn observation_subscription_ids(&self, session_id: &str) -> Result<Vec<String>, String> {
        let state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let session = state
            .sessions
            .get(session_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
        Ok(session.observation_frames.keys().cloned().collect())
    }

    /// Binds one new stream subscription to a stable page in its owning session.
    /// 将一条新流订阅绑定到所属会话中的稳定页面。
    ///
    /// # Returns / 返回值
    /// Success only for a live page and a subscription id not owned by another stream.
    /// 仅在页面仍活动且订阅标识未由其他流占用时成功。
    pub fn bind_stream_page(
        &self,
        session_id: &str,
        page_id: &str,
        subscription_id: &str,
    ) -> Result<(), String> {
        if subscription_id.trim().is_empty() {
            return Err(String::from(BROWSER_ERROR_INVALID_REQUEST));
        }
        self.sync_host_events(session_id)?;
        let mut state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let session = state
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
        let page = session
            .supervision
            .pages()
            .target_for_page(page_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_INVALID_REQUEST))?;
        if page.host_generation != session.projection.host_generation {
            return Err(String::from(BROWSER_ERROR_STALE_GENERATION));
        }
        if session.observation_frames.contains_key(subscription_id)
            || session
                .cancelled_stream_subscriptions
                .contains(subscription_id)
        {
            return Err(String::from(BROWSER_ERROR_LEASE_CONFLICT));
        }
        if session.observation_frames.len() + session.cancelled_stream_subscriptions.len()
            >= BROWSER_MAX_OBSERVERS
        {
            return Err(String::from(BROWSER_ERROR_OBSERVER_LIMIT));
        }
        session
            .observation_frames
            .insert(subscription_id.to_string(), VecDeque::new());
        session
            .observation_pages
            .insert(subscription_id.to_string(), page_id.to_string());
        session
            .stream_subscriptions
            .insert(subscription_id.to_string());
        Ok(())
    }

    /// Lists unique pages currently watched by at least one stream for frame production.
    /// 列出至少被一条流观察的唯一页面，供帧生产使用。
    ///
    /// # Returns / 返回值
    /// Session/page pairs whose host generation is still current.
    /// 宿主代次仍有效的会话与页面组合。
    pub fn observed_page_targets(&self) -> Result<Vec<(String, String)>, String> {
        let state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let mut pages = BTreeSet::new();
        for (session_id, session) in &state.sessions {
            if session.projection.status != BrowserSessionStatus::Running {
                continue;
            }
            for page_id in session.observation_pages.values() {
                if session
                    .supervision
                    .pages()
                    .target_for_page(page_id)
                    .is_some_and(|page| page.host_generation == session.projection.host_generation)
                {
                    pages.insert((session_id.clone(), page_id.clone()));
                }
            }
        }
        Ok(pages.into_iter().collect())
    }

    /// Returns how many frames one subscription currently retains.
    /// 返回某个订阅当前保留的帧数。
    pub fn retained_frame_count(
        &self,
        session_id: &str,
        subscription_id: &str,
    ) -> Result<usize, String> {
        let state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let session = state
            .sessions
            .get(session_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
        Ok(session
            .observation_frames
            .get(subscription_id)
            .map(VecDeque::len)
            .unwrap_or(0))
    }

    /// Issues one single-use ticket authorising one observation stream connection.
    /// 签发一个仅授权一次观察流连接的一次性凭据。
    ///
    /// # Returns
    /// 返回值
    /// The ticket value, which is removed the first time it is consumed.
    /// 返回凭据值；它在第一次被消费时即被移除。
    pub fn issue_stream_ticket(&self, session_id: &str) -> Result<BrowserStreamTicket, String> {
        let mut state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let session = state
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
        if session.projection.status == BrowserSessionStatus::Closed {
            return Err(String::from(BROWSER_ERROR_INVALID_REQUEST));
        }
        // Expired tickets are dropped rather than accumulating for the session's lifetime.
        // 已过期凭据会被丢弃，而不是在整个会话生命周期内累积。
        let now = Instant::now();
        session
            .stream_tickets
            .retain(|_, (_, expiry)| *expiry > now);
        if session.stream_tickets.len() >= BROWSER_MAX_PENDING_STREAM_TICKETS {
            return Err(String::from(BROWSER_ERROR_STREAM_TICKET_LIMIT));
        }
        let ticket = Uuid::new_v4().to_string();
        session.stream_tickets.insert(
            ticket.clone(),
            (String::from(session_id), now + BROWSER_STREAM_TICKET_TTL),
        );
        Ok(BrowserStreamTicket {
            ticket,
            session_id: String::from(session_id),
        })
    }

    /// Consumes one stream ticket, which can never be used twice.
    /// 消费一个流凭据；它绝不能被使用两次。
    ///
    /// # Returns
    /// 返回值
    /// The authorised session identity, or `invalid_request` for an unknown, expired, or already
    /// consumed ticket.
    /// 返回被授权的会话标识；对未知、已过期或已被消费的凭据返回 `invalid_request`。
    pub fn consume_stream_ticket(&self, ticket: &str) -> Result<String, String> {
        let mut state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let now = Instant::now();
        let mut authorized: Option<String> = None;
        for session in state.sessions.values_mut() {
            if let Some((session_id, expiry)) = session.stream_tickets.remove(ticket) {
                // Removal happens before the expiry test, so a replayed ticket is always refused
                // even when the original had already expired.
                // 移除发生在过期判断之前，因此即使原凭据已过期，重放的凭据也总是被拒绝。
                if expiry > now && session.projection.status != BrowserSessionStatus::Closed {
                    authorized = Some(session_id);
                }
                break;
            }
        }
        authorized.ok_or_else(|| String::from(BROWSER_ERROR_INVALID_REQUEST))
    }

    /// Resumes one observation stream from the client's cursor.
    /// 从客户端游标恢复一个观察流。
    ///
    /// # Parameters
    /// 参数
    /// `subscription_id` names the observer and `cursor` is what it last observed, or `None` for a
    /// client that has no position yet.
    /// `subscription_id` 命名观察者，`cursor` 是其最后观察到的位置；尚无位置的客户端传 `None`。
    ///
    /// # Returns
    /// 返回值
    /// A resume answer that demands a snapshot whenever the cursor cannot be trusted.
    /// 当游标不可信时要求快照的恢复答复。
    ///
    /// # Design note / 设计说明
    /// A cursor is trusted only when it names the current page generation and no frame between it
    /// and the retained window was evicted. Otherwise the observer must resynchronise. Observation
    /// never consumes or replays input belonging to the independent controller.
    /// 只有当游标指向当前页面代次、且其与保留窗口之间没有任何帧被淘汰时，游标才被信任。否则观察者
    /// 必须重新同步。观察连接绝不消费或重放独立控制者的输入。
    pub fn resume_stream(
        &self,
        session_id: &str,
        subscription_id: &str,
        cursor: Option<BrowserStreamCursor>,
    ) -> Result<BrowserStreamResume, String> {
        self.resume_stream_internal(session_id, subscription_id, cursor, false)
    }

    /// Resumes a previously page-bound stream only while its original subscription remains live.
    /// 仅在原订阅仍存活时恢复先前已绑定页面的流。
    ///
    /// # Parameters / 参数
    /// `session_id` and `subscription_id` identify the bound stream; `cursor` is optional on first use.
    /// `session_id` 与 `subscription_id` 标识已绑定流；首次使用时 `cursor` 可以缺省。
    ///
    /// # Returns / 返回值
    /// Returns its frame window, or an error when cancellation happened during initial capture.
    /// 返回其画面窗口；如果初次抓取期间发生取消，则返回错误。
    pub fn resume_bound_stream(
        &self,
        session_id: &str,
        subscription_id: &str,
        cursor: Option<BrowserStreamCursor>,
    ) -> Result<BrowserStreamResume, String> {
        self.resume_stream_internal(session_id, subscription_id, cursor, true)
    }

    /// Polls only an existing page-bound WebSocket stream without recreating a cancelled observer.
    /// 只轮询现有且已绑定页面的 WebSocket 流，不重建已取消的观察者。
    ///
    /// # Parameters / 参数
    /// `session_id` and `subscription_id` select the live stream; `cursor` names its last frame.
    /// `session_id` 与 `subscription_id` 选中活动流；`cursor` 标识其最后一帧。
    ///
    /// # Returns / 返回值
    /// Returns the next resume answer, or an explicit error after the stream was cancelled.
    /// 返回下一份恢复答复；流被取消后返回明确错误。
    pub fn poll_bound_stream(
        &self,
        session_id: &str,
        subscription_id: &str,
        cursor: BrowserStreamCursor,
    ) -> Result<BrowserStreamResume, String> {
        self.resume_bound_stream(session_id, subscription_id, Some(cursor))
    }

    /// Applies one resume or bound poll under the same manager lock as subscription cancellation.
    /// 在与订阅取消相同的管理器锁下处理一次恢复或已绑定轮询。
    ///
    /// # Parameters / 参数
    /// `require_bound` is true for an established WebSocket poll and false for initial resume.
    /// `require_bound` 对已建立 WebSocket 的轮询为真，对初次恢复为假。
    ///
    /// # Returns / 返回值
    /// Returns the authoritative frame window or an explicit invalid/stale subscription error.
    /// 返回权威画面窗口，或明确的无效、过期订阅错误。
    fn resume_stream_internal(
        &self,
        session_id: &str,
        subscription_id: &str,
        cursor: Option<BrowserStreamCursor>,
        require_bound: bool,
    ) -> Result<BrowserStreamResume, String> {
        let mut state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let session = state
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
        if subscription_id.trim().is_empty() {
            return Err(String::from(BROWSER_ERROR_INVALID_REQUEST));
        }
        // The same lock also protects explicit unsubscribe, so a cancelled socket cannot be
        // resurrected between checking its page binding and reading its retained frames.
        // 同一把锁还保护显式取消订阅，因此已取消套接字无法在核对页面绑定与读取保留帧之间复活。
        if require_bound
            && (!session.stream_subscriptions.contains(subscription_id)
                || !session.observation_pages.contains_key(subscription_id)
                || !session.observation_frames.contains_key(subscription_id))
        {
            return Err(String::from(BROWSER_ERROR_INVALID_REQUEST));
        }
        if session
            .cancelled_stream_subscriptions
            .contains(subscription_id)
        {
            return Err(String::from(BROWSER_ERROR_LEASE_CONFLICT));
        }
        if let Some(page_id) = session.observation_pages.get(subscription_id)
            && session
                .supervision
                .pages()
                .target_for_page(page_id)
                .is_none()
        {
            return Err(String::from(BROWSER_ERROR_STALE_GENERATION));
        }
        // A stream connection is itself an observation subscription, so it is registered here.
        // Without this the subscriber would appear connected while frame publication had nowhere to
        // deliver, which looks exactly like a frozen picture rather than a missing subscription.
        // 流连接本身就是一种观察订阅，因此在此登记。否则订阅者看似已连接，但帧发布无处可投递——
        // 其表现与画面冻结一模一样，而不是像缺少订阅。
        if !session.observation_frames.contains_key(subscription_id)
            && session.observation_frames.len() + session.cancelled_stream_subscriptions.len()
                >= BROWSER_MAX_OBSERVERS
        {
            return Err(String::from(BROWSER_ERROR_OBSERVER_LIMIT));
        }
        session
            .observation_frames
            .entry(String::from(subscription_id))
            .or_default();
        // The subscription is remembered as connection-owned so that a later disconnect releases
        // exactly this subscription and nothing else.
        // 该订阅被记为连接所有，使后续断开恰好释放此订阅而不影响其他。
        session
            .stream_subscriptions
            .insert(String::from(subscription_id));
        let current_generation = session.projection.page_generation;
        let retained: Vec<BrowserObservationFrame> = session
            .observation_frames
            .get(subscription_id)
            .map(|frames| frames.iter().cloned().collect())
            .unwrap_or_default();
        let oldest_retained = retained
            .first()
            .and_then(|frame| frame.sequence.parse::<u64>().ok());

        // The cursor is trusted only within the retained window of the current generation.
        // 游标仅在当前代次的保留窗口内才被信任。
        let trusted = match cursor {
            Some(cursor) if cursor.page_generation == current_generation => {
                match cursor.frame_sequence {
                    // A client with no frame yet must take a snapshot, because it has no baseline.
                    // 尚无帧的客户端必须取快照，因为它没有基线。
                    None => false,
                    Some(sequence) => match oldest_retained {
                        // The client is contiguous when nothing between it and the window was evicted.
                        // 当客户端与保留窗口之间没有帧被淘汰时，它是连续的。
                        Some(oldest) => sequence.saturating_add(1) >= oldest,
                        None => false,
                    },
                }
            }
            _ => false,
        };

        if trusted {
            let sequence = cursor.and_then(|cursor| cursor.frame_sequence).unwrap_or(0);
            let frames = retained
                .into_iter()
                .filter(|frame| {
                    frame
                        .sequence
                        .parse::<u64>()
                        .map(|value| value > sequence)
                        .unwrap_or(false)
                })
                .collect();
            return Ok(BrowserStreamResume {
                mode: BrowserStreamResumeMode::Incremental,
                page_generation: current_generation,
                frames,
                dropped_frames: 0,
                // An incremental resume continues an existing connection, so nothing is discarded.
                // 增量恢复延续既有连接，因此不丢弃任何内容。
                discarded_inputs: 0,
            });
        }

        // Observation reconnect never mutates controller input. No old input is replayed by the
        // stream, and a read-only observer cannot discard another client's pending work.
        // 观察重连绝不修改控制者输入。流不会重放旧输入，只读观察者也不能丢弃其他客户端的待处理操作。
        let discarded_inputs = 0;
        let dropped_frames = match (oldest_retained, cursor.and_then(|c| c.frame_sequence)) {
            (Some(oldest), Some(sequence)) if oldest > sequence.saturating_add(1) => {
                usize::try_from(oldest.saturating_sub(sequence).saturating_sub(1))
                    .unwrap_or(usize::MAX)
            }
            _ => 0,
        };
        Ok(BrowserStreamResume {
            mode: BrowserStreamResumeMode::Snapshot,
            page_generation: current_generation,
            frames: retained,
            dropped_frames,
            discarded_inputs,
        })
    }

    /// Attaches one automation connection to a leased session.
    /// 将一个自动化连接挂接到已租约会话。
    pub fn attach_automation(
        &self,
        session_id: &str,
        lease_secret: &str,
        connection_id: &str,
    ) -> Result<BrowserSessionProjection, String> {
        let mut state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let session = state
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
        if session.lease_secret.as_deref() != Some(lease_secret) {
            return Err(String::from(BROWSER_ERROR_LEASE_REQUIRED));
        }
        if session.projection.automation_state == BrowserAutomationState::Attached
            && session.automation_connection_id.as_deref() != Some(connection_id)
        {
            return Err(String::from(BROWSER_ERROR_AUTOMATION_CONFLICT));
        }
        if connection_id.trim().is_empty() {
            return Err(String::from(BROWSER_ERROR_INVALID_REQUEST));
        }
        session.projection.automation_state = BrowserAutomationState::Attached;
        session.automation_connection_id = Some(connection_id.to_string());
        Ok(session.projection.clone())
    }

    /// Detaches one automation connection and invalidates its writes.
    /// 分离一个自动化连接并使其写入失效。
    pub fn detach_automation(
        &self,
        session_id: &str,
        connection_id: &str,
    ) -> Result<BrowserSessionProjection, String> {
        let mut state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let session = state
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
        if session.automation_connection_id.as_deref() != Some(connection_id) {
            return Err(String::from(BROWSER_ERROR_INVALID_REQUEST));
        }
        session.projection.automation_state = BrowserAutomationState::Detached;
        session.automation_connection_id = None;
        session.projection.control_epoch = session.projection.control_epoch.wrapping_add(1);
        session.queued_inputs.clear();
        Ok(session.projection.clone())
    }

    /// Subscribes one observer and returns its subscription identity.
    /// 订阅一个观察者并返回其订阅身份。
    pub fn subscribe_observer(&self, session_id: &str) -> Result<String, String> {
        let mut state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let session = state
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
        if !matches!(session.projection.status, BrowserSessionStatus::Running) {
            return Err(String::from(BROWSER_ERROR_INVALID_REQUEST));
        }
        if session.observation_frames.len() + session.cancelled_stream_subscriptions.len()
            >= BROWSER_MAX_OBSERVERS
        {
            return Err(String::from(BROWSER_ERROR_OBSERVER_LIMIT));
        }
        let subscription_id = Uuid::new_v4().to_string();
        session
            .observation_frames
            .insert(subscription_id.clone(), VecDeque::new());
        Ok(subscription_id)
    }

    /// Unsubscribes one observer and drops its retained frames.
    /// 取消订阅一个观察者并丢弃其保留帧。
    pub fn unsubscribe_observer(
        &self,
        session_id: &str,
        subscription_id: &str,
    ) -> Result<(), String> {
        let mut state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let session = state
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
        // Reserve a cancelled page-bound id until its original socket releases it, so an old
        // capture or disconnect cannot resume or tear down a new connection using the same id.
        // 在原套接字释放前保留已取消的页面绑定标识，避免旧抓取或断开操作接管或拆掉复用该标识的新连接。
        if session.stream_subscriptions.contains(subscription_id)
            && session.observation_pages.contains_key(subscription_id)
        {
            session
                .cancelled_stream_subscriptions
                .insert(String::from(subscription_id));
        }
        session.observation_frames.remove(subscription_id);
        session.observation_pages.remove(subscription_id);
        session.stream_subscriptions.remove(subscription_id);
        Ok(())
    }

    /// Publishes one host frame to every subscriber, keeping only the newest frames.
    /// 向每个订阅者发布一个宿主帧，仅保留最新帧。
    pub fn publish_page_frame(
        &self,
        session_id: &str,
        page_id: &str,
        page_generation: u64,
        viewport_generation: u64,
        width: u32,
        height: u32,
        captured_at: &str,
        mime_type: &str,
        data: Vec<u8>,
    ) -> Result<u64, String> {
        if data.len() > BROWSER_MAX_FRAME_BYTES {
            return Err(String::from(BROWSER_ERROR_INVALID_REQUEST));
        }
        let mut state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let session = state
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| String::from(BROWSER_ERROR_UNKNOWN_SESSION))?;
        if page_generation != session.projection.page_generation {
            return Err(String::from(BROWSER_ERROR_STALE_GENERATION));
        }
        if !page_id.is_empty() {
            let page = session
                .supervision
                .pages()
                .target_for_page(page_id)
                .ok_or_else(|| String::from(BROWSER_ERROR_STALE_GENERATION))?;
            if page.host_generation != session.projection.host_generation {
                return Err(String::from(BROWSER_ERROR_STALE_GENERATION));
            }
            if viewport_generation != session.projection.viewport_generation {
                return Err(String::from(BROWSER_ERROR_STALE_VIEWPORT));
            }
        }
        let sequence = session.next_frame_sequence;
        session.next_frame_sequence = session.next_frame_sequence.wrapping_add(1);
        for (subscription_id, frames) in session.observation_frames.iter_mut() {
            if session
                .observation_pages
                .get(subscription_id)
                .is_some_and(|observed_page| observed_page != page_id)
            {
                continue;
            }
            if frames.len() >= BROWSER_MAX_QUEUED_FRAMES {
                frames.pop_front();
            }
            frames.push_back(BrowserObservationFrame {
                sequence: sequence.to_string(),
                page_id: page_id.to_string(),
                page_generation,
                viewport_generation,
                width,
                height,
                captured_at: captured_at.to_string(),
                mime_type: mime_type.to_string(),
                data: data.clone(),
            });
        }
        // The sequence is returned so a producer can correlate the frame it published with the
        // sequence observers will report receiving.
        // 返回序号，使生产者能把其发布的帧与观察者报告收到的序号关联起来。
        Ok(sequence)
    }

    /// Publishes a test frame without a host page, preserving deterministic stream tests.
    /// 发布不绑定宿主页面的测试帧，以保留确定性的流测试。
    #[cfg(test)]
    pub fn publish_frame(
        &self,
        session_id: &str,
        page_generation: u64,
        mime_type: &str,
        data: Vec<u8>,
    ) -> Result<u64, String> {
        self.publish_page_frame(
            session_id,
            "",
            page_generation,
            1,
            0,
            0,
            "0",
            mime_type,
            data,
        )
    }
}

/// Expires one held lease whose service monotonic deadline passed.
/// 使一个服务单调期限已过的持有租约到期。
fn expire_lease_if_needed(session: &mut BrowserSessionRecord) {
    let expired = session
        .lease_expires_at
        .is_some_and(|expires_at| Instant::now() >= expires_at);
    if expired && session.projection.lease_state == BrowserLeaseState::Held {
        session.needs_input_release = true;
        session.projection.control_epoch = session.projection.control_epoch.wrapping_add(1);
        session.lease_expires_at = None;
        session.queued_inputs.clear();
        if session.projection.automation_state == BrowserAutomationState::Attached {
            // A timed-out lease cannot prove that the independent Playwright socket stopped.
            // 租约超时不能证明独立 Playwright 套接字已经停止。
            if !session
                .unconfirmed_operations
                .iter()
                .any(|known| known == BROWSER_AUTOMATION_LEASE_EXPIRED_OPERATION)
            {
                session
                    .unconfirmed_operations
                    .push(String::from(BROWSER_AUTOMATION_LEASE_EXPIRED_OPERATION));
            }
            session.projection.lease_state = BrowserLeaseState::Blocked;
            session.projection.blocked_reason = Some(session.unconfirmed_operations.join("; "));
        } else {
            session.projection.lease_state = BrowserLeaseState::Free;
            session.lease_holder = None;
            session.lease_secret = None;
        }
    }
}

/// Returns the user lease heartbeat interval in milliseconds.
/// 返回用户租约心跳间隔毫秒数。
pub fn browser_user_lease_heartbeat_interval_ms() -> u64 {
    BROWSER_USER_LEASE_HEARTBEAT_INTERVAL.as_millis() as u64
}

/// Creates one Core browser session from shared service state.
/// 从共享服务状态创建一个 Core 浏览器会话。
pub async fn service_browser_session_create(
    state: SharedServiceBridgeState,
    request: BrowserSessionCreateRequest,
    owner_session_id: String,
) -> Result<BrowserSessionProjection, String> {
    let manager = browser_manager_from_state(&state)?;
    let session = manager.create_owned_session(
        &owner_session_id,
        request
            .profile_mode
            .unwrap_or(BrowserProfileMode::Ephemeral),
    )?;
    if manager.host_runtime_config()?.is_none() {
        // Without a host runtime there is no background browser to own the session, so Core must
        // say so instead of handing out a session that can never display or drive a page.
        // 没有宿主运行时就没有拥有该会话的后台浏览器，Core 必须如实报告，而不是交付一个永远无法
        // 显示或驱动页面的会话。
        let _ = manager.close_session(&session.session_id);
        return Err(String::from(BROWSER_ERROR_CAPABILITY_UNSUPPORTED));
    }
    match manager.start_host(&session.session_id) {
        Ok(running) => Ok(running),
        Err(error) => {
            let _ = manager.close_session(&session.session_id);
            Err(error)
        }
    }
}

/// Returns the shared Core browser manager owned by one service state.
/// 返回某个服务状态拥有的共享 Core 浏览器管理器。
///
/// # Parameters
/// 参数
/// `state` is the shared service state that owns the manager.
/// `state` 是拥有该管理器的共享服务状态。
///
/// # Returns
/// 返回值
/// A clone of the shared manager handle, or a diagnostic when the state lock is poisoned.
/// 返回共享管理器句柄的克隆；状态锁中毒时返回诊断。
pub fn service_browser_manager(state: &SharedServiceBridgeState) -> Result<BrowserManager, String> {
    browser_manager_from_state(state)
}

/// Closes every owned Core browser host channel during service shutdown.
/// 在服务关闭期间关闭所有已拥有的 Core 浏览器宿主通道。
///
/// # Returns
/// 返回值
/// The number of host channels closed, and each host exits on its own because its channel ended.
/// 返回已关闭的宿主通道数量；每个宿主因通道结束而自行退出。
pub fn service_browser_shutdown_hosts(state: &SharedServiceBridgeState) -> Result<usize, String> {
    service_browser_manager(state)?.shutdown_hosts()
}

/// Lists Core browser sessions from shared service state.
/// 从共享服务状态列出 Core 浏览器会话。
pub async fn service_browser_session_list(
    state: SharedServiceBridgeState,
    owner_session_id: String,
) -> Result<Vec<BrowserSessionProjection>, String> {
    let manager = browser_manager_from_state(&state)?;
    manager.list_owned_sessions(&owner_session_id)
}

/// Closes one Core browser session from shared service state.
/// 从共享服务状态关闭一个 Core 浏览器会话。
pub async fn service_browser_session_close(
    state: SharedServiceBridgeState,
    request: BrowserSessionCloseRequest,
) -> Result<BrowserSessionProjection, String> {
    let manager = browser_manager_from_state(&state)?;
    manager.close_session_with_lease(&request.session_id, &request.lease_secret)
}

/// Navigates one Core browser session from shared service state.
/// 从共享服务状态导航一个 Core 浏览器会话。
pub async fn service_browser_session_navigate(
    state: SharedServiceBridgeState,
    request: BrowserSessionNavigateRequest,
) -> Result<BrowserSessionProjection, String> {
    let manager = browser_manager_from_state(&state)?;
    manager.navigate_page_on_host(
        &request.session_id,
        &request.page_id,
        &request.url,
        &request.lease_secret,
    )
}

/// Reads one Core browser session projection from shared service state.
/// 从共享服务状态读取一个 Core 浏览器会话投影。
pub async fn service_browser_session_snapshot(
    state: SharedServiceBridgeState,
    request: BrowserSessionSnapshotRequest,
) -> Result<BrowserSessionProjection, String> {
    let manager = browser_manager_from_state(&state)?;
    manager.session_projection(&request.session_id)
}

/// Acquires one Core browser control lease from shared service state.
/// 从共享服务状态获取一个 Core 浏览器控制租约。
pub async fn service_browser_lease_acquire(
    state: SharedServiceBridgeState,
    request: BrowserLeaseAcquireRequest,
) -> Result<BrowserLeaseAcquireResponse, String> {
    let manager = browser_manager_from_state(&state)?;
    let (session, lease_secret) =
        manager.acquire_lease(&request.session_id, &request.holder, None)?;
    Ok(BrowserLeaseAcquireResponse {
        session,
        lease_secret,
        heartbeat_interval_ms: browser_user_lease_heartbeat_interval_ms(),
        control_epoch: manager
            .session_projection(&request.session_id)?
            .control_epoch,
    })
}

/// Releases one Core browser control lease from shared service state.
/// 从共享服务状态释放一个 Core 浏览器控制租约。
pub async fn service_browser_lease_release(
    state: SharedServiceBridgeState,
    request: BrowserLeaseReleaseRequest,
) -> Result<BrowserSessionProjection, String> {
    let manager = browser_manager_from_state(&state)?;
    manager.release_lease(&request.session_id, &request.lease_secret)
}

/// Heartbeats one Core browser control lease from shared service state.
/// 从共享服务状态为一个 Core 浏览器控制租约发送心跳。
pub async fn service_browser_lease_heartbeat(
    state: SharedServiceBridgeState,
    request: BrowserLeaseHeartbeatRequest,
) -> Result<BrowserSessionProjection, String> {
    let manager = browser_manager_from_state(&state)?;
    manager.heartbeat_lease(&request.session_id, &request.lease_secret)
}

/// Subscribes one Core browser observer from shared service state.
/// 从共享服务状态订阅一个 Core 浏览器观察者。
pub async fn service_browser_observe_subscribe(
    state: SharedServiceBridgeState,
    request: BrowserObserveSubscribeRequest,
) -> Result<BrowserObserveSubscribeResponse, String> {
    let manager = browser_manager_from_state(&state)?;
    let subscription_id = manager.subscribe_observer(&request.session_id)?;
    Ok(BrowserObserveSubscribeResponse {
        subscription_id,
        page_generation: manager
            .session_projection(&request.session_id)?
            .page_generation,
    })
}

/// Unsubscribes one Core browser observer from shared service state.
/// 从共享服务状态取消订阅一个 Core 浏览器观察者。
pub async fn service_browser_observe_unsubscribe(
    state: SharedServiceBridgeState,
    request: BrowserObserveUnsubscribeRequest,
) -> Result<(), String> {
    let manager = browser_manager_from_state(&state)?;
    manager.unsubscribe_observer(&request.session_id, &request.subscription_id)
}

/// Takes control of one Core browser session explicitly.
/// 明确取得某个 Core 浏览器会话的控制权。
pub async fn service_browser_lease_takeover(
    state: SharedServiceBridgeState,
    request: BrowserLeaseTakeoverRequest,
) -> Result<BrowserLeaseTakeoverResponse, String> {
    let manager = browser_manager_from_state(&state)?;
    let (session, lease_secret) = manager
        .takeover_after_retiring_ai(&request.session_id, &request.holder)
        .await?;
    Ok(BrowserLeaseTakeoverResponse {
        session,
        lease_secret,
    })
}

/// Confirms that in-flight browser work for one session stopped.
/// 确认某个会话的浏览器在途工作已停止。
pub async fn service_browser_operations_confirm(
    state: SharedServiceBridgeState,
    request: BrowserOperationsConfirmRequest,
) -> Result<BrowserSessionProjection, String> {
    let manager = browser_manager_from_state(&state)?;
    manager.session_projection(&request.session_id)?;
    // A caller's assertion is not host evidence; only a supervised completion may clear a block.
    // 调用方的声明不是宿主证据；只有受监督的完成结果才能清除阻塞。
    Err(format!(
        "{BROWSER_ERROR_CAPABILITY_UNSUPPORTED}: client confirmation cannot prove an operation stopped; close the session to terminate uncertain work"
    ))
}

/// Lists the still-unconfirmed in-flight operations of one session.
/// 列出某个会话仍未确认的在途操作。
pub async fn service_browser_operations_unconfirmed(
    state: SharedServiceBridgeState,
    request: BrowserHostStatusRequest,
) -> Result<Vec<String>, String> {
    let manager = browser_manager_from_state(&state)?;
    manager.unconfirmed_operations(&request.session_id)
}

/// Lists the registered pages of one Core browser session.
/// 列出单个 Core 浏览器会话已登记的页面。
pub async fn service_browser_pages_list(
    state: SharedServiceBridgeState,
    request: BrowserPagesListRequest,
) -> Result<serde_json::Value, String> {
    let manager = browser_manager_from_state(&state)?;
    let pages = manager.list_pages(&request.session_id)?;
    serde_json::to_value(pages)
        .map_err(|error| format!("failed to serialize browser pages: {error}"))
}

/// Creates one new page inside a Core browser session through its host.
/// 通过宿主在一个 Core 浏览器会话内创建一个新页面。
///
/// # Returns
/// 返回值
/// The registered pages after the host reported the new target.
/// 返回宿主报告新目标后已登记的页面。
pub async fn service_browser_page_create(
    state: SharedServiceBridgeState,
    request: BrowserPageCreateRequest,
) -> Result<serde_json::Value, String> {
    let manager = browser_manager_from_state(&state)?;
    validate_browser_url(&request.url)?;
    let gate = manager.dispatch_gate(&request.session_id)?;
    let _dispatch = gate
        .lock()
        .map_err(|error| format!("failed to lock browser dispatch: {error}"))?;
    manager.authorize_host_write(&request.session_id, &request.lease_secret)?;
    let outcome = manager.request_host(
        &request.session_id,
        "page.create",
        serde_json::json!({ "url": request.url }),
    )?;
    match outcome {
        BrowserHostRequestOutcome::Completed(_) => {}
        BrowserHostRequestOutcome::Failed { code, message } => {
            return Err(format!("{code}: {message}"));
        }
        BrowserHostRequestOutcome::Unknown { reason } => {
            // The page may exist even though the answer was lost, so the outcome is reported as
            // unknown instead of being retried.
            // 即使应答丢失该页面也可能已存在，因此结果以未知报告，而不是重试。
            manager.mark_operation_unconfirmed(&request.session_id, "page.create")?;
            return Err(format!("{BROWSER_ERROR_LEASE_BLOCKED}: {reason}"));
        }
    }
    let pages = manager.list_pages(&request.session_id)?;
    serde_json::to_value(pages)
        .map_err(|error| format!("failed to serialize browser pages: {error}"))
}

/// Hides the local host windows of one Core browser session without ending the session.
/// 隐藏某个 Core 浏览器会话的宿主本地窗口而不结束该会话。
///
/// # Returns
/// 返回值
/// The session projection, which stays `running` because hiding never ends the context.
/// 返回会话投影；因为隐藏绝不结束上下文，所以它保持 `running`。
pub async fn service_browser_hide_windows(
    state: SharedServiceBridgeState,
    request: BrowserSessionHideWindowsRequest,
) -> Result<BrowserSessionProjection, String> {
    let manager = browser_manager_from_state(&state)?;
    let gate = manager.dispatch_gate(&request.session_id)?;
    let _dispatch = gate
        .lock()
        .map_err(|error| format!("failed to lock browser dispatch: {error}"))?;
    manager.authorize_host_write(&request.session_id, &request.lease_secret)?;
    match manager.request_host(&request.session_id, "window.hide", serde_json::json!({}))? {
        BrowserHostRequestOutcome::Completed(_) => {}
        BrowserHostRequestOutcome::Failed { code, message } => {
            return Err(format!("{code}: {message}"));
        }
        BrowserHostRequestOutcome::Unknown { reason } => {
            return Err(format!("{BROWSER_ERROR_INVALID_REQUEST}: {reason}"));
        }
    }
    manager.session_projection(&request.session_id)
}

/// Reports the supervised host state of one Core browser session.
/// 报告单个 Core 浏览器会话受监督的宿主状态。
pub async fn service_browser_host_status(
    state: SharedServiceBridgeState,
    request: BrowserHostStatusRequest,
) -> Result<serde_json::Value, String> {
    let manager = browser_manager_from_state(&state)?;
    manager.host_supervision_summary(&request.session_id)
}

/// Dispatches one Core browser input event from shared service state.
/// 从共享服务状态派发一个 Core 浏览器输入事件。
pub async fn service_browser_input_dispatch(
    state: SharedServiceBridgeState,
    request: BrowserInputDispatchRequest,
) -> Result<BrowserInputDispatchResponse, String> {
    let manager = browser_manager_from_state(&state)?;
    let event = BrowserInputEvent {
        sequence: request.sequence.clone(),
        kind: request.kind.clone(),
        payload: request.payload.clone(),
    };
    let outcome = manager.dispatch_host_input(
        &request.session_id,
        &request.page_id,
        &request.lease_secret,
        request.page_generation,
        request.viewport_generation,
        event,
    )?;
    Ok(BrowserInputDispatchResponse {
        outcome: outcome.outcome,
        accepted_sequence: outcome.sequence.map(|sequence| sequence.to_string()),
        reason: outcome.reason,
        detail: outcome.detail,
    })
}

/// Request object for issuing one single-use browser stream ticket.
/// 签发一个一次性浏览器流凭据的请求对象。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserStreamTicketRequest {
    /// Stable session identifier.
    /// 稳定会话标识。
    pub session_id: String,
}

/// Request object for resuming one browser observation stream.
/// 恢复一个浏览器观察流的请求对象。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserStreamResumeRequest {
    /// Stable Core browser identity checked against the caller's conversation before consuming the ticket.
    /// 消费凭据前根据调用者会话校验的稳定 Core 浏览器身份。
    pub session_id: String,
    /// One-time ticket previously issued by Core.
    /// Core 先前签发的一次性凭据。
    pub ticket: String,
    /// Stable Core page identity selected by this observer.
    /// 本观察端选择的稳定 Core 页面身份。
    pub page_id: String,
    /// Stable subscription identity chosen by the client.
    /// 客户端选择的稳定订阅身份。
    pub subscription_id: String,
    /// What the client last observed, or none when it has no position yet.
    /// 客户端最后观察到的位置；尚无位置时缺省。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<BrowserStreamCursor>,
}

/// Issues one single-use ticket for one Core browser observation stream.
/// 为一个 Core 浏览器观察流签发一次性凭据。
pub async fn service_browser_stream_ticket(
    state: SharedServiceBridgeState,
    request: BrowserStreamTicketRequest,
) -> Result<BrowserStreamTicket, String> {
    let manager = browser_manager_from_state(&state)?;
    manager.issue_stream_ticket(&request.session_id)
}

/// Resumes one Core browser observation stream using a single-use ticket.
/// 使用一次性凭据恢复一个 Core 浏览器观察流。
pub async fn service_browser_stream_resume(
    state: SharedServiceBridgeState,
    request: BrowserStreamResumeRequest,
) -> Result<BrowserStreamResume, String> {
    let manager = browser_manager_from_state(&state)?;
    // The ticket is the authority for which session may be observed, so a client cannot name an
    // arbitrary session to observe.
    // 凭据是“可观察哪个会话”的权威，因此客户端无法指定任意会话来观察。
    let session_id = manager.consume_stream_ticket(&request.ticket)?;
    if session_id != request.session_id {
        return Err(String::from(BROWSER_ERROR_UNKNOWN_SESSION));
    }
    manager.bind_stream_page(&session_id, &request.page_id, &request.subscription_id)?;
    if let Err(error) = service_browser_page_capture(
        state,
        BrowserPageCaptureRequest {
            session_id: session_id.clone(),
            page_id: request.page_id,
        },
    )
    .await
    {
        let _ = manager.release_stream_subscription(&session_id, &request.subscription_id);
        return Err(error);
    }
    // The RPC returns one snapshot answer and owns no continuing socket. Release its observer
    // after cloning the answer, whether resume succeeded or failed; WebSocket streams retain their
    // separate connection-owned subscriptions until disconnect.
    // RPC 只返回一次快照，没有持续套接字。复制答复后无论恢复成功与否都释放其观察者；WebSocket
    // 流的连接自有订阅则独立保留至断开。
    // resume owns the snapshot result independently of the subscription queue.
    // resume 独立于订阅队列持有快照结果。
    let resume = manager.resume_bound_stream(&session_id, &request.subscription_id, request.cursor);
    // release records whether the exact RPC-owned subscription was actually removed.
    // release 记录精确的 RPC 自有订阅是否确实被移除。
    let release = manager.release_stream_subscription(&session_id, &request.subscription_id);
    match (resume, release) {
        (Ok(answer), Ok(true)) => Ok(answer),
        (Err(error), _) => Err(error),
        (_, Err(error)) => Err(error),
        (_, Ok(false)) => Err(String::from(BROWSER_ERROR_INVALID_REQUEST)),
    }
}

/// One file Core staged on behalf of a session.
/// 一个由 Core 代表会话暂存的文件。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserUploadRecord {
    /// Core-generated upload identity.
    /// Core 生成的上传标识。
    pub upload_id: String,
    /// Display name with any path component removed.
    /// 已移除任何路径成分的展示名称。
    pub display_name: String,
    /// Absolute path Core chose inside its own staging root.
    /// Core 在其自有暂存根目录内选择的绝对路径。
    pub staged_path: String,
    /// Content length in bytes.
    /// 内容字节长度。
    pub byte_length: usize,
    /// Lowercase hexadecimal SHA-256 of the staged content.
    /// 暂存内容的小写十六进制 SHA-256。
    pub sha256: String,
}

/// Public upload metadata with the Core-owned staging path omitted.
/// 不包含 Core 自有暂存路径的公开上传元数据。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserUploadPublicRecord {
    /// Core-generated upload identity.
    /// Core 生成的上传标识。
    pub upload_id: String,
    /// Sanitized file name shown to the user.
    /// 向用户展示的已清理文件名。
    pub display_name: String,
    /// Content length in bytes.
    /// 内容字节长度。
    pub byte_length: usize,
    /// Lowercase SHA-256 of the uploaded content.
    /// 上传内容的小写 SHA-256 摘要。
    pub sha256: String,
}

/// Lifecycle of a download saved inside a Core-owned session directory.
/// 保存在 Core 自有会话目录内的下载生命周期。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum BrowserDownloadStatus {
    /// The host has reserved a path but has not finished writing content.
    /// 宿主已预留路径，但尚未写完内容。
    Pending,
    /// Core verified the completed file and its digest.
    /// Core 已核验完成文件及其摘要。
    Completed,
    /// The host cancelled or Core rejected the file.
    /// 宿主取消文件，或 Core 拒绝文件。
    Failed,
}

/// Path-free metadata exposed for a Core-owned browser download.
/// 对外暴露的 Core 自有浏览器下载元数据，不包含路径。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserDownloadPublicRecord {
    /// Opaque download identity generated by the host.
    /// 宿主生成的不透明下载身份。
    pub download_id: String,
    /// Stable Core browser session identity.
    /// 稳定的 Core 浏览器会话身份。
    pub session_id: String,
    /// Stable Core page identity that initiated the download.
    /// 发起下载的稳定 Core 页面身份。
    pub page_id: String,
    /// Sanitized filename shown to the user.
    /// 向用户展示的已清理文件名。
    pub file_name: String,
    /// Verified lifecycle state.
    /// 已核验的生命周期状态。
    pub status: BrowserDownloadStatus,
    /// Received byte length after completion.
    /// 完成后的接收字节长度。
    pub byte_length: Option<u64>,
    /// Core-computed SHA-256 after successful completion.
    /// 成功完成后由 Core 计算的 SHA-256。
    pub sha256: Option<String>,
}

/// One unresolved dialog belonging to a stable Core browser page.
/// 一个归属稳定 Core 浏览器页面的未解决对话框。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserDialogPublicRecord {
    /// Opaque host dialog identity.
    /// 不透明的宿主对话框身份。
    pub dialog_id: String,
    /// Owning Core session identity.
    /// 归属 Core 会话身份。
    pub session_id: String,
    /// Owning stable Core page identity.
    /// 归属的稳定 Core 页面身份。
    pub page_id: String,
    /// JavaScript dialog kind.
    /// JavaScript 对话框类型。
    pub dialog_type: String,
    /// Page-provided message displayed to the controller.
    /// 展示给控制者的页面消息。
    pub message: String,
    /// Page-provided initial prompt value.
    /// 页面提供的提示框初始值。
    pub default_prompt: String,
}

/// Controller answer to one pending page dialog.
/// 控制者对一个待处理页面对话框的答复。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserDialogRespondRequest {
    /// Owning Core session identity.
    /// 归属 Core 会话身份。
    pub session_id: String,
    /// Stable Core page identity.
    /// 稳定 Core 页面身份。
    pub page_id: String,
    /// Exact pending dialog identity.
    /// 精确的待处理对话框身份。
    pub dialog_id: String,
    /// Current exclusive control credential.
    /// 当前独占控制凭据。
    pub lease_secret: String,
    /// Whether to accept rather than dismiss the dialog.
    /// 是否接受而不是拒绝该对话框。
    pub accept: bool,
    /// Text supplied only when answering a prompt dialog.
    /// 仅在答复提示框时提供的文字。
    pub prompt_text: Option<String>,
}

/// One intercepted HTML file chooser awaiting a controller upload.
/// 一个等待控制者上传的已拦截 HTML 文件选择器。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserFileChooserPublicRecord {
    /// Opaque chooser identity generated by the host.
    /// 宿主生成的不透明选择器身份。
    pub chooser_id: String,
    /// Owning Core session identity.
    /// 归属 Core 会话身份。
    pub session_id: String,
    /// Owning stable Core page identity.
    /// 归属的稳定 Core 页面身份。
    pub page_id: String,
    /// Single or multiple file-selection mode.
    /// 单文件或多文件选择模式。
    pub mode: String,
}

/// Internal record binds public metadata to the exact host target that started it.
/// 内部记录将公开元数据绑定到发起下载的精确宿主目标。
#[derive(Debug, Clone)]
struct BrowserDownloadRecord {
    /// Metadata safe for client projection.
    /// 可安全投影给客户端的元数据。
    public: BrowserDownloadPublicRecord,
    /// Host target identity used to reject a completion from another page.
    /// 用来拒绝其他页面伪造完成事件的宿主目标身份。
    target_id: String,
}

/// Removes path syntax and control characters from an origin-provided filename.
/// 从来源提供的文件名中移除路径语法与控制字符。
fn browser_download_filename(raw: &str) -> String {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or_default();
    let name: String = base
        .chars()
        .take(180)
        .map(|character| {
            if character.is_control() || "<>:\"/\\|?*".contains(character) {
                '_'
            } else {
                character
            }
        })
        .collect();
    if name.trim().is_empty() {
        String::from("download")
    } else {
        name
    }
}

/// Converts an internal upload record into path-free public metadata.
/// 将内部上传记录转换为不含路径的公开元数据。
///
/// # Returns / 返回值
/// A public record that cannot reveal the Core staging root.
/// 无法泄露 Core 暂存根目录的公开记录。
fn public_upload_record(record: BrowserUploadRecord) -> BrowserUploadPublicRecord {
    BrowserUploadPublicRecord {
        upload_id: record.upload_id,
        display_name: record.display_name,
        byte_length: record.byte_length,
        sha256: record.sha256,
    }
}

/// Returns the lowercase hexadecimal SHA-256 digest of one byte slice.
/// 返回一段字节的小写十六进制 SHA-256 摘要。
fn browser_sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    let digest = hasher.finalize();
    let mut text = String::with_capacity(digest.len() * 2);
    for byte in digest {
        text.push_str(&format!("{byte:02x}"));
    }
    text
}

/// Largest file Core stages for one upload.
/// Core 为单次上传暂存的最大文件。
const BROWSER_MAX_UPLOAD_BYTES: usize = 64 * 1024 * 1024;
/// Maximum number of Core-staged uploads kept by one live browser session.
/// 单个活动浏览器会话最多保留的 Core 暂存上传数。
const BROWSER_MAX_STAGED_UPLOADS_PER_SESSION: usize = 16;
/// Maximum total bytes of Core-staged uploads kept by one live browser session.
/// 单个活动浏览器会话最多保留的 Core 暂存上传总字节数。
const BROWSER_MAX_STAGED_UPLOAD_BYTES_PER_SESSION: usize = 128 * 1024 * 1024;

/// Stages one uploaded file for a Core browser session.
/// 为一个 Core 浏览器会话暂存一个上传文件。
///
/// # Returns
/// 返回值
/// The staged record; the client learns the upload identity, the digest, and the display name, and
/// never needs to know any filesystem path.
/// 返回暂存记录；客户端获得上传标识、摘要与展示名称，且无需知道任何文件系统路径。
pub async fn service_browser_upload_stage(
    state: SharedServiceBridgeState,
    request: BrowserUploadStageRequest,
) -> Result<BrowserUploadPublicRecord, String> {
    use base64::Engine as _;

    let manager = browser_manager_from_state(&state)?;
    let gate = manager.dispatch_gate(&request.session_id)?;
    let _dispatch = gate
        .lock()
        .map_err(|error| format!("failed to lock browser dispatch: {error}"))?;
    manager.authorize_host_write(&request.session_id, &request.lease_secret)?;
    let data = base64::engine::general_purpose::STANDARD
        .decode(&request.data_base64)
        .map_err(|error| {
            format!("{BROWSER_ERROR_INVALID_REQUEST}: invalid upload encoding: {error}")
        })?;
    manager
        .stage_upload(&request.session_id, &request.name, data)
        .map(public_upload_record)
}

/// Places one staged upload into a page's file input.
/// 将一个暂存上传放入页面的文件输入中。
pub async fn service_browser_upload_attach(
    state: SharedServiceBridgeState,
    request: BrowserUploadAttachRequest,
) -> Result<serde_json::Value, String> {
    let manager = browser_manager_from_state(&state)?;
    let gate = manager.dispatch_gate(&request.session_id)?;
    let _dispatch = gate
        .lock()
        .map_err(|error| format!("failed to lock browser dispatch: {error}"))?;
    manager.sync_host_events(&request.session_id)?;
    let target_id = manager.authorized_page_target(
        &request.session_id,
        &request.page_id,
        &request.lease_secret,
    )?;
    if let Some(chooser_id) = request.chooser_id.as_deref() {
        let matches = manager
            .file_choosers(&request.session_id)?
            .into_iter()
            .any(|chooser| chooser.chooser_id == chooser_id && chooser.page_id == request.page_id);
        if !matches {
            return Err(String::from(BROWSER_ERROR_INVALID_REQUEST));
        }
    }
    manager.attach_upload_to_page(
        &request.session_id,
        &request.upload_id,
        &target_id,
        request.chooser_id.as_deref(),
    )
}

/// Lists the staged uploads of one Core browser session.
/// 列出某个 Core 浏览器会话的暂存上传。
pub async fn service_browser_upload_list(
    state: SharedServiceBridgeState,
    request: BrowserHostStatusRequest,
) -> Result<Vec<BrowserUploadPublicRecord>, String> {
    let manager = browser_manager_from_state(&state)?;
    manager
        .uploads(&request.session_id)
        .map(|uploads| uploads.into_iter().map(public_upload_record).collect())
}

/// Lists intercepted file choosers so a controller can select local files in its own UI.
/// 列出已拦截的文件选择器，使控制者能够在自己的界面中选择本地文件。
pub async fn service_browser_file_chooser_list(
    state: SharedServiceBridgeState,
    request: BrowserHostStatusRequest,
) -> Result<Vec<BrowserFileChooserPublicRecord>, String> {
    browser_manager_from_state(&state)?.file_choosers(&request.session_id)
}

/// Lists one session's download lifecycle without exposing host file paths.
/// 列出一个会话的下载生命周期，而不暴露宿主文件路径。
pub async fn service_browser_download_list(
    state: SharedServiceBridgeState,
    request: BrowserHostStatusRequest,
) -> Result<Vec<BrowserDownloadPublicRecord>, String> {
    browser_manager_from_state(&state)?.downloads(&request.session_id)
}

/// Lists pending dialogs so observers can see them without receiving answer authority.
/// 列出待处理对话框，使观察者可见但不获得答复权限。
pub async fn service_browser_dialog_list(
    state: SharedServiceBridgeState,
    request: BrowserHostStatusRequest,
) -> Result<Vec<BrowserDialogPublicRecord>, String> {
    browser_manager_from_state(&state)?.dialogs(&request.session_id)
}

/// Sends a dialog answer from the current controller to the exact owning host page.
/// 将当前控制者的对话框答复发送到精确的归属宿主页面。
pub async fn service_browser_dialog_respond(
    state: SharedServiceBridgeState,
    request: BrowserDialogRespondRequest,
) -> Result<serde_json::Value, String> {
    browser_manager_from_state(&state)?.respond_dialog(
        &request.session_id,
        &request.page_id,
        &request.dialog_id,
        &request.lease_secret,
        request.accept,
        request.prompt_text.as_deref(),
    )
}

/// Request object for staging one uploaded file.
/// 暂存一个上传文件的请求对象。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserUploadStageRequest {
    /// Stable session identifier.
    /// 稳定会话标识。
    pub session_id: String,
    /// Current control lease secret authorizing file staging.
    /// 授权暂存文件的当前控制租约凭据。
    pub lease_secret: String,
    /// Client-supplied display name; any path component is discarded.
    /// 客户端提供的展示名称；任何路径成分都会被丢弃。
    pub name: String,
    /// Base64-encoded file content.
    /// Base64 编码的文件内容。
    pub data_base64: String,
}

/// Request object for placing one staged upload into a page.
/// 将一个暂存上传放入页面的请求对象。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserUploadAttachRequest {
    /// Stable session identifier.
    /// 稳定会话标识。
    pub session_id: String,
    /// Stable Core page identity receiving the file.
    /// 接收文件的稳定 Core 页面身份。
    pub page_id: String,
    /// Current control lease secret authorizing file attachment.
    /// 授权附加文件的当前控制租约凭据。
    pub lease_secret: String,
    /// Upload identity returned by staging.
    /// 暂存时返回的上传标识。
    pub upload_id: String,
    /// Exact clicked file chooser, if this upload answers one.
    /// 若此次上传答复一个已点击的文件选择器，则为其精确身份。
    pub chooser_id: Option<String>,
}

/// Captures one page through its host and publishes the frame to observers.
/// 通过宿主捕获一个页面并将该帧发布给观察者。
///
/// # Returns
/// 返回值
/// The published frame's sequence and the page it came from.
/// 返回已发布帧的序号及其来源页面。
///
/// # Design note / 设计说明
/// The image travels as base64 inside the host's JSON response because the control protocol carries
/// text frames. A capture the host reports as failed is surfaced as a failure; it is never replaced
/// by a stale frame, because observers would then believe they were seeing the current page.
/// 图像以 base64 承载在宿主的 JSON 应答中，因为控制协议承载文本帧。宿主报告失败的捕获会作为失败
/// 暴露，绝不替换为旧帧，否则观察者会以为自己看到的是当前页面。
pub async fn service_browser_page_capture(
    state: SharedServiceBridgeState,
    request: BrowserPageCaptureRequest,
) -> Result<serde_json::Value, String> {
    use base64::Engine as _;

    let manager = browser_manager_from_state(&state)?;
    let gate = manager.dispatch_gate(&request.session_id)?;
    let _dispatch = gate
        .lock()
        .map_err(|error| format!("failed to lock browser dispatch: {error}"))?;
    let projection = manager.session_projection(&request.session_id)?;
    let page_generation = projection.page_generation;
    let page = manager
        .list_pages(&request.session_id)?
        .into_iter()
        .find(|page| page.page_id == request.page_id)
        .ok_or_else(|| String::from(BROWSER_ERROR_INVALID_REQUEST))?;
    if page.host_generation != projection.host_generation {
        return Err(String::from(BROWSER_ERROR_STALE_GENERATION));
    }
    let outcome = manager.request_host(
        &request.session_id,
        "page.capture",
        serde_json::json!({ "targetId": page.target_id }),
    )?;
    let result = match outcome {
        BrowserHostRequestOutcome::Completed(value) => value,
        BrowserHostRequestOutcome::Failed { code, message } => {
            return Err(format!("{code}: {message}"));
        }
        BrowserHostRequestOutcome::Unknown { reason } => {
            return Err(format!("{BROWSER_ERROR_INVALID_REQUEST}: {reason}"));
        }
    };
    let encoded = result
        .get("dataBase64")
        .and_then(|value| value.as_str())
        .ok_or_else(|| String::from(BROWSER_ERROR_INVALID_REQUEST))?;
    if result.get("targetId").and_then(|value| value.as_str()) != Some(page.target_id.as_str()) {
        return Err(String::from(BROWSER_ERROR_STALE_GENERATION));
    }
    let mime_type = result
        .get("mimeType")
        .and_then(|value| value.as_str())
        .unwrap_or("image/png");
    let width = result
        .get("width")
        .and_then(|value| value.as_u64())
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| String::from(BROWSER_ERROR_INVALID_REQUEST))?;
    let height = result
        .get("height")
        .and_then(|value| value.as_u64())
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| String::from(BROWSER_ERROR_INVALID_REQUEST))?;
    let captured_at = result
        .get("capturedAt")
        .and_then(|value| value.as_str())
        .ok_or_else(|| String::from(BROWSER_ERROR_INVALID_REQUEST))?;
    if let Some(viewport) = &projection.viewport_size
        && (viewport.width != width || viewport.height != height)
    {
        return Err(String::from(BROWSER_ERROR_STALE_VIEWPORT));
    }
    let data = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|error| {
            format!("{BROWSER_ERROR_INVALID_REQUEST}: invalid frame encoding: {error}")
        })?;
    let sequence = manager.publish_page_frame(
        &request.session_id,
        &page.page_id,
        page_generation,
        projection.viewport_generation,
        width,
        height,
        captured_at,
        mime_type,
        data,
    )?;
    Ok(serde_json::json!({
        "sequence": sequence.to_string(),
        "pageId": page.page_id,
        "pageGeneration": page_generation.to_string(),
        "viewportGeneration": projection.viewport_generation.to_string(),
        "width": width,
        "height": height,
        "targetId": result.get("targetId").cloned().unwrap_or(serde_json::Value::Null),
        "url": result.get("url").cloned().unwrap_or(serde_json::Value::Null),
        "mimeType": mime_type,
    }))
}

/// Request object for capturing one page of a Core browser session.
/// 捕获某个 Core 浏览器会话中一个页面的请求对象。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserPageCaptureRequest {
    /// Stable session identifier.
    /// 稳定会话标识。
    pub session_id: String,
    /// Stable Core page identity to capture.
    /// 要捕获的稳定 Core 页面身份。
    pub page_id: String,
}

/// Establishes the viewport of one Core browser session.
/// 建立某个 Core 浏览器会话的视口。
pub async fn service_browser_viewport_set(
    state: SharedServiceBridgeState,
    request: BrowserViewportSetRequest,
) -> Result<BrowserSessionProjection, String> {
    let manager = browser_manager_from_state(&state)?;
    manager.set_host_viewport(
        &request.session_id,
        request.width,
        request.height,
        &request.lease_secret,
    )
}

/// Attaches one automation connection from shared service state.
/// 从共享服务状态挂接一个自动化连接。
pub async fn service_browser_automation_attach(
    state: SharedServiceBridgeState,
    request: BrowserAutomationAttachRequest,
) -> Result<BrowserSessionProjection, String> {
    let manager = browser_manager_from_state(&state)?;
    manager.attach_automation(
        &request.session_id,
        &request.lease_secret,
        &request.connection_id,
    )
}

/// Detaches one automation connection from shared service state.
/// 从共享服务状态分离一个自动化连接。
pub async fn service_browser_automation_detach(
    state: SharedServiceBridgeState,
    request: BrowserAutomationDetachRequest,
) -> Result<BrowserSessionProjection, String> {
    let manager = browser_manager_from_state(&state)?;
    manager.detach_automation(&request.session_id, &request.connection_id)
}

/// Resolves the Core browser manager owned by shared service state.
/// 解析共享服务状态拥有的 Core 浏览器管理器。
fn browser_manager_from_state(state: &SharedServiceBridgeState) -> Result<BrowserManager, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    Ok(guard.browser_manager.clone())
}

/// Creates one Core browser session.
/// 创建一个 Core 浏览器会话。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserSessionCreateRequest {
    /// Profile mode selected for the new session.
    /// 为新会话选择的画像模式。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_mode: Option<BrowserProfileMode>,
}

/// Closes one Core browser session.
/// 关闭一个 Core 浏览器会话。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserSessionCloseRequest {
    /// Stable session identifier.
    /// 稳定会话标识。
    pub session_id: String,
    /// Current control lease secret authorizing session closure.
    /// 授权关闭会话的当前控制租约凭据。
    pub lease_secret: String,
}

/// Navigates one Core browser session.
/// 导航一个 Core 浏览器会话。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserSessionNavigateRequest {
    /// Stable session identifier.
    /// 稳定会话标识。
    pub session_id: String,
    /// Stable Core page identity to navigate.
    /// 要导航的稳定 Core 页面身份。
    pub page_id: String,
    /// Destination URL.
    /// 目标地址。
    pub url: String,
    /// Current control lease secret authorizing navigation.
    /// 授权导航的当前控制租约凭据。
    pub lease_secret: String,
}

/// Reads one Core browser session projection.
/// 读取一个 Core 浏览器会话投影。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserSessionSnapshotRequest {
    /// Stable session identifier.
    /// 稳定会话标识。
    pub session_id: String,
}

/// Acquires one Core browser control lease.
/// 获取一个 Core 浏览器控制租约。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserLeaseAcquireRequest {
    /// Stable session identifier.
    /// 稳定会话标识。
    pub session_id: String,
    /// Human-readable holder identity.
    /// 人类可读的持有者身份。
    pub holder: String,
}

/// Acquired lease response carrying the one-time control secret.
/// 携带一次性控制凭据的已获取租约响应。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserLeaseAcquireResponse {
    /// Session projection after acquisition.
    /// 获取后的会话投影。
    pub session: BrowserSessionProjection,
    /// One-time lease secret authorizing control writes.
    /// 授权控制写入的一次性租约凭据。
    pub lease_secret: String,
    /// Heartbeat interval in milliseconds.
    /// 心跳间隔毫秒数。
    pub heartbeat_interval_ms: u64,
    /// Observed control epoch after acquisition.
    /// 获取后观察到的控制代次。
    /// Serialized as decimal text because it crosses the JavaScript integer boundary.
    /// 跨 JavaScript 整数边界时序列化为十进制文本。
    #[serde(with = "browser_decimal_u64")]
    pub control_epoch: u64,
}

/// Releases one Core browser control lease.
/// 释放一个 Core 浏览器控制租约。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserLeaseReleaseRequest {
    /// Stable session identifier.
    /// 稳定会话标识。
    pub session_id: String,
    /// Lease secret returned by acquisition.
    /// 获取时返回的租约凭据。
    pub lease_secret: String,
}

/// Heartbeats one Core browser control lease.
/// 为一个 Core 浏览器控制租约发送心跳。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserLeaseHeartbeatRequest {
    /// Stable session identifier.
    /// 稳定会话标识。
    pub session_id: String,
    /// Lease secret returned by acquisition.
    /// 获取时返回的租约凭据。
    pub lease_secret: String,
}

/// Subscribes one Core browser observer.
/// 订阅一个 Core 浏览器观察者。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserObserveSubscribeRequest {
    /// Stable session identifier.
    /// 稳定会话标识。
    pub session_id: String,
}

/// Observer subscription response.
/// 观察者订阅响应。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserObserveSubscribeResponse {
    /// Stable subscription identity.
    /// 稳定订阅身份。
    pub subscription_id: String,
    /// Page generation observed at subscribe time.
    /// 订阅时观察到的页面代次。
    /// Serialized as decimal text because it crosses the JavaScript integer boundary.
    /// 跨 JavaScript 整数边界时序列化为十进制文本。
    #[serde(with = "browser_decimal_u64")]
    pub page_generation: u64,
}

/// Unsubscribes one Core browser observer.
/// 取消订阅一个 Core 浏览器观察者。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserObserveUnsubscribeRequest {
    /// Stable session identifier.
    /// 稳定会话标识。
    pub session_id: String,
    /// Stable subscription identity.
    /// 稳定订阅身份。
    pub subscription_id: String,
}

/// Dispatches one Core browser input event.
/// 派发一个 Core 浏览器输入事件。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserInputDispatchRequest {
    /// Stable session identifier.
    /// 稳定会话标识。
    pub session_id: String,
    /// Stable Core page identity receiving this input.
    /// 接收该输入的稳定 Core 页面身份。
    pub page_id: String,
    /// Lease secret returned by acquisition.
    /// 获取时返回的租约凭据。
    pub lease_secret: String,
    /// Page generation observed by the sender.
    /// 发送方观察到的页面代次。
    /// Serialized as decimal text because it crosses the JavaScript integer boundary.
    /// 跨 JavaScript 整数边界时序列化为十进制文本。
    #[serde(with = "browser_decimal_u64")]
    pub page_generation: u64,
    /// Viewport generation the coordinates were measured against.
    /// 坐标据以测量的视口代次。
    /// Serialized as decimal text because it crosses the JavaScript integer boundary.
    /// 跨 JavaScript 整数边界时序列化为十进制文本。
    #[serde(default = "browser_default_generation", with = "browser_decimal_u64")]
    pub viewport_generation: u64,
    /// Input sequence number as a decimal string.
    /// 十进制字符串输入序号。
    pub sequence: String,
    /// Input kind such as pointer, key, or scroll.
    /// 输入类型，例如指针、键盘或滚动。
    pub kind: String,
    /// JSON input payload interpreted by the host adapter.
    /// 由宿主适配器解释的 JSON 输入载荷。
    pub payload: serde_json::Value,
}

/// Input dispatch acknowledgement carrying the real outcome.
/// 携带真实结果的输入派发确认。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserInputDispatchResponse {
    /// Real meaning of the attempt: accepted, applied, rejected, or unknown.
    /// 本次尝试的真实含义：accepted、applied、rejected 或 unknown。
    pub outcome: BrowserInputOutcome,
    /// Core-assigned input sequence as a decimal string, present when accepted.
    /// Core 分配的输入序号（十进制字符串，被接受时存在）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accepted_sequence: Option<String>,
    /// Stable reason code, present when rejected or unknown.
    /// 稳定原因码（被拒绝或结果未知时存在）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Host-reported detail, such as the coordinate a clamped pointer actually landed on.
    /// 宿主报告的细节，例如被收敛的指针实际落点。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<serde_json::Value>,
}

/// Request object for establishing the viewport of one session.
/// 建立某个会话视口的请求对象。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserViewportSetRequest {
    /// Stable session identifier.
    /// 稳定会话标识。
    pub session_id: String,
    /// Current control lease secret authorizing resize.
    /// 授权调整尺寸的当前控制租约凭据。
    pub lease_secret: String,
    /// Viewport width in device-independent pixels.
    /// 视口宽度（设备无关像素）。
    pub width: u32,
    /// Viewport height in device-independent pixels.
    /// 视口高度（设备无关像素）。
    pub height: u32,
}

/// Attaches one automation connection.
/// 挂接一个自动化连接。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserAutomationAttachRequest {
    /// Stable session identifier.
    /// 稳定会话标识。
    pub session_id: String,
    /// Lease secret returned by acquisition.
    /// 获取时返回的租约凭据。
    pub lease_secret: String,
    /// Stable automation connection identity.
    /// 稳定自动化连接身份。
    pub connection_id: String,
}

/// Detaches one automation connection.
/// 分离一个自动化连接。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserAutomationDetachRequest {
    /// Stable session identifier.
    /// 稳定会话标识。
    pub session_id: String,
    /// Stable automation connection identity.
    /// 稳定自动化连接身份。
    pub connection_id: String,
}

/// Lists the pages registered for one Core browser session.
/// 列出某个 Core 浏览器会话已登记的页面。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserPagesListRequest {
    /// Stable session identifier.
    /// 稳定会话标识。
    pub session_id: String,
}

/// Reads the supervised host state of one Core browser session.
/// 读取某个 Core 浏览器会话受监督的宿主状态。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserHostStatusRequest {
    /// Stable session identifier.
    /// 稳定会话标识。
    pub session_id: String,
}

/// Request object for an explicit browser control takeover.
/// 明确浏览器控制接管的请求对象。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserLeaseTakeoverRequest {
    /// Stable session identifier.
    /// 稳定会话标识。
    pub session_id: String,
    /// Identity of the new controller.
    /// 新控制者的身份。
    pub holder: String,
}

/// Response carrying the rotated lease credentials.
/// 携带轮换后租约凭据的响应。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserLeaseTakeoverResponse {
    /// Projection after the takeover.
    /// 接管后的投影。
    pub session: BrowserSessionProjection,
    /// New lease secret; the previous holder's secret no longer authorizes anything.
    /// 新租约凭据；前任持有者的凭据不再授权任何操作。
    pub lease_secret: String,
}

/// Request object for confirming that in-flight browser work stopped.
/// 确认浏览器在途工作已停止的请求对象。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserOperationsConfirmRequest {
    /// Stable session identifier.
    /// 稳定会话标识。
    pub session_id: String,
    /// One specific operation to clear, or none to clear every unconfirmed operation.
    /// 要清除的单项操作；缺省则清除全部未确认操作。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
}

/// Creates one new page inside a Core browser session.
/// 在一个 Core 浏览器会话内创建一个新页面。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserPageCreateRequest {
    /// Stable session identifier.
    /// 稳定会话标识。
    pub session_id: String,
    /// Current control lease secret authorizing page creation.
    /// 授权创建页面的当前控制租约凭据。
    pub lease_secret: String,
    /// Initial URL for the new page.
    /// 新页面的初始地址。
    pub url: String,
}

/// Hides the local host windows of one Core browser session.
/// 隐藏某个 Core 浏览器会话的宿主本地窗口。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserSessionHideWindowsRequest {
    /// Stable session identifier.
    /// 稳定会话标识。
    pub session_id: String,
    /// Current control lease secret authorizing host window changes.
    /// 授权修改宿主窗口的当前控制租约凭据。
    pub lease_secret: String,
}

/// Serializes frame bytes as base64 while retaining binary bytes inside Core.
/// 将帧字节序列化为 base64，同时在 Core 内保留二进制字节。
mod browser_base64_bytes {
    use base64::Engine as _;
    use serde::{Deserialize, Deserializer, Serializer};

    /// Encodes one frame body without a large JSON integer array.
    /// 编码一个帧体，避免庞大的 JSON 整数数组。
    ///
    /// # Returns / 返回值
    /// The serializer result for a base64 string.
    /// base64 字符串的序列化结果。
    pub fn serialize<S: Serializer>(data: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&base64::engine::general_purpose::STANDARD.encode(data))
    }

    /// Decodes one base64 frame body from a service payload.
    /// 从服务载荷解码一个 base64 帧体。
    ///
    /// # Returns / 返回值
    /// The original frame bytes or a decoding error.
    /// 原始帧字节，或解码错误。
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(deserializer)?;
        base64::engine::general_purpose::STANDARD
            .decode(text)
            .map_err(serde::de::Error::custom)
    }
}

/// Serializes one monotonic browser counter as decimal text across the JS boundary.
/// 跨 JS 边界将一个单调浏览器计数器序列化为十进制文本。
mod browser_decimal_u64 {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    /// Serializes one counter as decimal text.
    /// 将一个计数器序列化为十进制文本。
    pub(super) fn serialize<S>(value: &u64, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        value.to_string().serialize(serializer)
    }

    /// Deserializes one counter from decimal text or a JSON number.
    /// 从十进制文本或 JSON 数字反序列化一个计数器。
    pub(super) fn deserialize<'de, D>(deserializer: D) -> Result<u64, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        match value {
            serde_json::Value::String(text) => {
                text.parse::<u64>().map_err(serde::de::Error::custom)
            }
            serde_json::Value::Number(number) => number
                .as_u64()
                .ok_or_else(|| serde::de::Error::custom("browser counter out of u64 range")),
            _ => Err(serde::de::Error::custom(
                "browser counter must be decimal text",
            )),
        }
    }
}

/// Returns the default browser generation used when a peer omits one.
/// 返回对端省略代次时使用的默认浏览器代次。
fn browser_default_generation() -> u64 {
    1
}

/// How a client must resume a browser observation stream.
/// 客户端必须如何恢复浏览器观察流。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum BrowserStreamResumeMode {
    /// The client must adopt a full snapshot and complete frame before acting.
    /// 客户端必须先采用完整快照与完整帧，然后才能操作。
    Snapshot,
    /// The client may continue from its cursor because nothing was missed.
    /// 由于没有遗漏，客户端可以从其游标继续。
    Incremental,
}

/// The frame position one client last observed.
/// 某个客户端最后观察到的帧位置。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserStreamCursor {
    /// Page generation the client observed.
    /// 客户端观察到的页面代次。
    /// Serialized as decimal text because it crosses the JavaScript integer boundary.
    /// 跨 JavaScript 整数边界时序列化为十进制文本。
    #[serde(with = "browser_decimal_u64")]
    pub page_generation: u64,
    /// Last frame sequence the client confirmed, when it has one.
    /// 客户端最后确认的帧序号（如已有）。
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "browser_decimal_u64_option"
    )]
    pub frame_sequence: Option<u64>,
}

/// The authoritative answer to one stream resume request.
/// 对一次流恢复请求的权威答复。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserStreamResume {
    /// Whether the client must resynchronise from a snapshot.
    /// 客户端是否必须从快照重新同步。
    pub mode: BrowserStreamResumeMode,
    /// Page generation the frames below belong to.
    /// 下列帧所属的页面代次。
    /// Serialized as decimal text because it crosses the JavaScript integer boundary.
    /// 跨 JavaScript 整数边界时序列化为十进制文本。
    #[serde(with = "browser_decimal_u64")]
    pub page_generation: u64,
    /// Frames to deliver, oldest first.
    /// 要交付的帧，最旧在前。
    pub frames: Vec<BrowserObservationFrame>,
    /// Frames that were evicted before the client's cursor and are therefore unrecoverable.
    /// 在客户端游标之前已被淘汰、因此不可恢复的帧数。
    pub dropped_frames: usize,
    /// Inputs discarded with the previous connection, so a client can see none were replayed.
    /// 随上一个连接一起被丢弃的输入数，使客户端可见没有任何输入被重放。
    pub discarded_inputs: usize,
}

/// A one-time ticket that authorises exactly one stream connection.
/// 仅授权一次流连接的一次性凭据。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserStreamTicket {
    /// The ticket value itself.
    /// 凭据值本身。
    pub ticket: String,
    /// Session the ticket authorises.
    /// 凭据所授权的会话。
    pub session_id: String,
}

/// Optional decimal-text serialization for one browser counter.
/// 单个浏览器计数器的可选十进制文本序列化。
mod browser_decimal_u64_option {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    /// Serializes an optional counter as decimal text, omitting `None`.
    /// 将可选计数器序列化为十进制文本，`None` 时省略。
    pub(super) fn serialize<S>(value: &Option<u64>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(counter) => counter.to_string().serialize(serializer),
            None => serializer.serialize_none(),
        }
    }

    /// Deserializes an optional counter from decimal text or a JSON number.
    /// 从十进制文本或 JSON 数字反序列化可选计数器。
    pub(super) fn deserialize<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Option::<serde_json::Value>::deserialize(deserializer)?;
        match value {
            None | Some(serde_json::Value::Null) => Ok(None),
            Some(serde_json::Value::String(text)) => text
                .parse::<u64>()
                .map(Some)
                .map_err(serde::de::Error::custom),
            Some(serde_json::Value::Number(number)) => number
                .as_u64()
                .map(Some)
                .ok_or_else(|| serde::de::Error::custom("browser counter out of u64 range")),
            Some(_) => Err(serde::de::Error::custom(
                "browser counter must be decimal text",
            )),
        }
    }
}

/// Returns the current UTC Unix millisecond text used by browser diagnostics.
/// 返回浏览器诊断使用的当前 UTC Unix 毫秒文本。
#[allow(dead_code)]
pub(crate) fn current_browser_millis_text() -> String {
    current_unix_millis_text()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::test_support::test_service_state_with_parent_session;
    use serde_json::json;
    use std::{
        io::{Read, Write},
        net::{SocketAddr, TcpListener},
        sync::atomic::{AtomicBool, Ordering as AtomicOrdering},
        thread,
    };
    use vulcan_agent_runtime::model::SessionId;

    /// In-process browser host that speaks the documented control protocol.
    /// 使用文档控制协议的进程内浏览器宿主。
    ///
    /// It exists so host ownership can be tested deterministically without starting a desktop
    /// runtime, while still exercising the real sockets, framing, and handshake code paths.
    /// 它的存在使宿主归属可以在不启动桌面运行时的前提下被确定性测试，同时仍然走真实的套接字、
    /// 帧编解码与握手代码路径。
    struct FakeBrowserHost {
        /// Loopback address the fake host listens on.
        /// 伪宿主监听的地址。
        address: SocketAddr,
    }

    /// Spawner that publishes a fake host endpoint instead of starting a desktop runtime.
    /// 发布伪宿主端点而不是启动桌面运行时的启动器。
    struct FakeBrowserHostSpawner {
        /// Loopback address of the fake host.
        /// 伪宿主的回环地址。
        address: SocketAddr,
        /// Liveness flag shared with the fake host thread and the process handle.
        /// 与伪宿主线程及进程句柄共享的存活标志。
        alive: Arc<AtomicBool>,
        /// Whether process liveness inspection must fail.
        /// 进程存活检查是否必须失败。
        poll_error: Arc<AtomicBool>,
    }

    /// Process handle for a fake host that stays alive until it is killed.
    /// 在未被终止前保持存活的伪宿主进程句柄。
    struct FakeBrowserHostProcess {
        /// Whether the fake process is still running.
        /// 伪进程是否仍在运行。
        alive: Arc<AtomicBool>,
        /// Whether process liveness inspection must fail.
        /// 进程存活检查是否必须失败。
        poll_error: Arc<AtomicBool>,
        /// Operating-system process id reported to Core.
        /// 报告给 Core 的操作系统进程标识。
        pid: u32,
    }

    impl BrowserHostProcessHandle for FakeBrowserHostProcess {
        fn pid(&self) -> u32 {
            self.pid
        }

        fn try_wait(&mut self) -> Result<Option<i32>, String> {
            if self.poll_error.swap(false, AtomicOrdering::SeqCst) {
                return Err(String::from("injected host process inspection failure"));
            }
            if self.alive.load(AtomicOrdering::SeqCst) {
                Ok(None)
            } else {
                Ok(Some(1))
            }
        }

        fn kill(&mut self) {
            self.alive.store(false, AtomicOrdering::SeqCst);
        }
    }

    impl FakeBrowserHost {
        /// Starts one fake host that answers requests after the configured handshake delay.
        /// 启动一个在配置的握手延迟后开始应答请求的伪宿主。
        ///
        /// The host accepts any number of sequential control connections so tests can start more
        /// than one session against it, exactly as separate real host processes would serve them.
        /// 该宿主接受任意数量的顺序控制连接，使测试可以针对它启动多个会话，就像多个真实宿主进程
        /// 分别服务它们一样。
        fn start(handshake_delay: Duration, alive: Arc<AtomicBool>) -> Self {
            Self::start_with_page_list_failure(handshake_delay, alive, false)
        }

        /// Starts a host that can reject the mandatory initial page enumeration.
        /// 启动一个可拒绝必需的初始页面枚举的宿主。
        fn start_with_page_list_failure(
            handshake_delay: Duration,
            alive: Arc<AtomicBool>,
            fail_page_list: bool,
        ) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake host");
            let address = listener.local_addr().expect("fake host address");
            thread::spawn(move || {
                loop {
                    let Ok((mut stream, _)) = listener.accept() else {
                        return;
                    };
                    let alive = Arc::clone(&alive);
                    thread::spawn(move || {
                        let mut decoder = BrowserHostFrameDecoder::new();
                        let mut buffer = [0_u8; 8192];
                        // The first frame is the owning Core's hello, which names the session and
                        // generation the host must answer for.
                        // 首帧是归属 Core 的 hello，其中指明宿主必须为其应答的会话与代际。
                        let mut hello = serde_json::Value::Null;
                        while hello.is_null() {
                            match stream.read(&mut buffer) {
                                Ok(0) | Err(_) => return,
                                Ok(read) => {
                                    if let Ok(values) = decoder.push(&buffer[..read])
                                        && let Some(value) = values.into_iter().next()
                                    {
                                        hello = value;
                                    }
                                }
                            }
                        }
                        thread::sleep(handshake_delay);
                        let handshake = json!({
                            "protocolVersion": BROWSER_HOST_PROTOCOL_VERSION,
                            "sessionId": hello["sessionId"],
                            "hostGeneration": hello["hostGeneration"],
                            "pid": 4321,
                            "runtimeVersion": "0.1.0",
                            "capabilities": ["playwright-endpoint-on-demand", "page.create"],
                        });
                        let Ok(frame) = encode_host_frame(&handshake) else {
                            return;
                        };
                        if stream.write_all(&frame).is_err() {
                            return;
                        }
                        loop {
                            match stream.read(&mut buffer) {
                                // Core closed the channel, which is how a host learns to exit.
                                // Core 关闭通道，宿主由此得知应当退出。
                                Ok(0) | Err(_) => {
                                    alive.store(false, AtomicOrdering::SeqCst);
                                    return;
                                }
                                Ok(read) => {
                                    let Ok(values) = decoder.push(&buffer[..read]) else {
                                        return;
                                    };
                                    for value in values {
                                        let request_id = value["requestId"]
                                            .as_str()
                                            .unwrap_or_default()
                                            .to_string();
                                        if fail_page_list
                                            && value["operation"].as_str() == Some("page.list")
                                        {
                                            let response = json!({
                                                "requestId": request_id,
                                                "ok": false,
                                                "error": {
                                                    "code": "fixture_page_list_failure",
                                                    "message": "initial page list failed",
                                                },
                                            });
                                            let Ok(frame) = encode_host_frame(&response) else {
                                                return;
                                            };
                                            if stream.write_all(&frame).is_err() {
                                                return;
                                            }
                                            continue;
                                        }
                                        let result = match value["operation"].as_str() {
                                            Some("page.list") => json!({ "pages": [] }),
                                            Some("page.navigate") => json!({
                                                "targetId": value["params"]["targetId"],
                                                "url": value["params"]["url"],
                                            }),
                                            Some("viewport.set") => json!({
                                                "width": value["params"]["width"],
                                                "height": value["params"]["height"],
                                            }),
                                            Some("input.dispatch") => json!({
                                                "targetId": value["params"]["targetId"],
                                            }),
                                            Some("host.closeContext") => json!({ "closed": true }),
                                            Some("host.shutdown") => {
                                                alive.store(false, AtomicOrdering::SeqCst);
                                                json!({ "acknowledged": true })
                                            }
                                            _ => json!({ "operation": value["operation"] }),
                                        };
                                        let response = json!({
                                            "requestId": request_id,
                                            "ok": true,
                                            "result": result,
                                        });
                                        let Ok(frame) = encode_host_frame(&response) else {
                                            return;
                                        };
                                        if stream.write_all(&frame).is_err() {
                                            return;
                                        }
                                    }
                                }
                            }
                        }
                    });
                }
            });
            Self { address }
        }
    }

    impl BrowserHostSpawner for FakeBrowserHostSpawner {
        fn spawn(
            &self,
            plan: &BrowserHostLaunchPlan,
        ) -> Result<Box<dyn BrowserHostProcessHandle>, String> {
            // The real host publishes this document itself; the fake publishes it on spawn so the
            // production discovery-wait path is exercised unchanged.
            // 真实宿主自行发布该文档；伪宿主在启动时发布它，使生产发现等待路径被原样验证。
            std::fs::create_dir_all(&plan.user_data_directory)
                .map_err(|error| format!("fake host profile: {error}"))?;
            let document = json!({
                "schemaVersion": BROWSER_HOST_CONTROL_DISCOVERY_SCHEMA_VERSION,
                "protocolVersion": BROWSER_HOST_PROTOCOL_VERSION,
                "controlHost": self.address.ip().to_string(),
                "controlPort": self.address.port(),
                "pid": 4321,
                "sessionId": plan.session_id,
                "hostGeneration": plan.host_generation,
            });
            std::fs::write(
                plan.control_discovery_path(),
                serde_json::to_string(&document).map_err(|error| error.to_string())?,
            )
            .map_err(|error| format!("fake host discovery: {error}"))?;
            Ok(Box::new(FakeBrowserHostProcess {
                alive: Arc::clone(&self.alive),
                poll_error: Arc::clone(&self.poll_error),
                pid: 4321,
            }))
        }
    }

    /// Builds one manager wired to a fake host with an isolated profile root.
    /// 构建一个连接到伪宿主且使用隔离画像根的管理器。
    fn manager_with_fake_host(handshake_delay: Duration) -> (BrowserManager, Arc<AtomicBool>) {
        manager_with_fake_host_options(handshake_delay, false)
    }

    /// Builds a fake-host manager with an optional initial page-list refusal.
    /// 构建一个可选择拒绝初始页面枚举的伪宿主管理器。
    fn manager_with_fake_host_options(
        handshake_delay: Duration,
        fail_page_list: bool,
    ) -> (BrowserManager, Arc<AtomicBool>) {
        let (manager, alive, _) = manager_with_fake_host_poll(handshake_delay, fail_page_list);
        (manager, alive)
    }

    /// Builds a fake-host manager with an observable process-inspection failure switch.
    /// 构建带可观察进程检查失败开关的伪宿主管理器。
    fn manager_with_fake_host_poll(
        handshake_delay: Duration,
        fail_page_list: bool,
    ) -> (BrowserManager, Arc<AtomicBool>, Arc<AtomicBool>) {
        let alive = Arc::new(AtomicBool::new(true));
        let poll_error = Arc::new(AtomicBool::new(false));
        let host = FakeBrowserHost::start_with_page_list_failure(
            handshake_delay,
            Arc::clone(&alive),
            fail_page_list,
        );
        let profiles_root =
            std::env::temp_dir().join(format!("vulcan-w06-profiles-{}", Uuid::new_v4()));
        let manager = BrowserManager::with_host_spawner(Arc::new(FakeBrowserHostSpawner {
            address: host.address,
            alive: Arc::clone(&alive),
            poll_error: Arc::clone(&poll_error),
        }));
        manager
            .configure_host_runtime(BrowserHostRuntimeConfig {
                executable: std::path::PathBuf::from("fake-electron.exe"),
                entry: std::path::PathBuf::from("fake-host.js"),
                chromium_executable: std::path::PathBuf::from("fake-chromium.exe"),
                profiles_root: profiles_root.clone(),
                staging_root: profiles_root.join("staging"),
                discovery_timeout: Duration::from_secs(10),
                request_timeout: Duration::from_secs(5),
            })
            .expect("configure host runtime");
        (manager, alive, poll_error)
    }

    /// Keeps a rejected initial page inventory from publishing a live browser or leaking its host.
    /// 防止初始页面清单被拒后仍发布运行中浏览器或遗留宿主。
    #[test]
    fn rejected_initial_page_list_closes_the_new_host() {
        let (manager, alive) = manager_with_fake_host_options(Duration::ZERO, true);
        let session = manager
            .create_owned_session("conversation-a", BrowserProfileMode::Ephemeral)
            .expect("create browser");

        let error = manager
            .start_host(&session.session_id)
            .expect_err("initial page enumeration must fail");
        assert!(error.contains("fixture_page_list_failure"), "{error}");
        assert!(!manager.has_live_host(&session.session_id));
        assert!(!alive.load(AtomicOrdering::SeqCst));
        assert_eq!(
            manager
                .session_projection(&session.session_id)
                .expect("closed projection")
                .status,
            BrowserSessionStatus::Closed
        );
    }

    /// Verifies uncertain AI work is recoverable only by closing the host browser context.
    /// 验证未知 AI 操作只能在关闭宿主浏览器上下文后恢复。
    #[test]
    fn blocked_ai_gateway_closes_the_browser_before_releasing_ownership() {
        let (manager, _alive) = manager_with_fake_host(Duration::ZERO);
        let session = manager
            .create_owned_session("conversation-a", BrowserProfileMode::Ephemeral)
            .expect("create owned browser");
        manager.start_host(&session.session_id).expect("start host");
        let (_, lease_secret) = manager
            .acquire_lease(&session.session_id, "automation", None)
            .expect("acquire automation lease");
        manager
            .attach_automation(&session.session_id, &lease_secret, "connection-a")
            .expect("attach automation");
        manager
            .mark_operation_unconfirmed(&session.session_id, "in-flight CDP")
            .expect("block uncertain work");
        assert!(
            manager
                .close_uncertain_automation_session(
                    &session.session_id,
                    "conversation-b",
                    "connection-a",
                )
                .is_err()
        );
        assert!(
            manager
                .close_uncertain_automation_session(
                    &session.session_id,
                    "conversation-a",
                    "connection-b",
                )
                .is_err()
        );
        assert_eq!(
            manager
                .session_projection(&session.session_id)
                .expect("still blocked")
                .lease_state,
            BrowserLeaseState::Blocked
        );
        let closed = manager
            .close_uncertain_automation_session(
                &session.session_id,
                "conversation-a",
                "connection-a",
            )
            .expect("host confirms context closure");
        assert_eq!(closed.status, BrowserSessionStatus::Closed);
        assert_eq!(closed.lease_state, BrowserLeaseState::Free);
        assert_eq!(closed.automation_state, BrowserAutomationState::Detached);
        assert_eq!(closed.blocked_reason, None);
        assert!(!manager.has_live_host(&session.session_id));
    }

    /// Verifies the host supervisor finishes a confirmed closure after a slow process exit.
    /// 验证宿主监督器在进程延迟退出后完成已确认的关闭。
    #[test]
    fn supervisor_finishes_confirmed_blocked_retirement_as_close_not_crash() {
        let (manager, alive) = manager_with_fake_host(Duration::ZERO);
        let session = manager
            .create_owned_session("conversation-a", BrowserProfileMode::Ephemeral)
            .expect("create owned browser");
        manager.start_host(&session.session_id).expect("start host");
        manager
            .mark_operation_unconfirmed(&session.session_id, "in-flight CDP")
            .expect("block uncertain work");
        manager
            .inner
            .lock()
            .expect("lock browser state")
            .sessions
            .get_mut(&session.session_id)
            .expect("browser record")
            .recovery_context_closed = true;
        alive.store(false, AtomicOrdering::SeqCst);
        let closed = manager
            .reap_host(&session.session_id)
            .expect("reap host")
            .expect("retirement is complete");
        assert_eq!(closed.status, BrowserSessionStatus::Closed);
        assert_eq!(closed.blocked_reason, None);
        assert!(!manager.has_live_host(&session.session_id));
    }

    /// Verifies a host crash cannot erase an unconfirmed write or reopen the same profile.
    /// 验证宿主崩溃不能抹去未确认写入或重开同一画像。
    #[test]
    fn crashed_host_keeps_uncertain_profile_blocked() {
        let (manager, alive) = manager_with_fake_host(Duration::ZERO);
        let first = manager
            .create_owned_session("conversation-a", BrowserProfileMode::Persistent)
            .expect("create first browser");
        manager
            .start_host(&first.session_id)
            .expect("start first host");
        manager
            .mark_operation_unconfirmed(&first.session_id, "in-flight CDP")
            .expect("block uncertain work");
        alive.store(false, AtomicOrdering::SeqCst);
        let crashed = manager
            .reap_host(&first.session_id)
            .expect("reap first host")
            .expect("host exited");
        assert_eq!(crashed.status, BrowserSessionStatus::Crashed);
        assert_eq!(crashed.lease_state, BrowserLeaseState::Blocked);
        assert_eq!(
            manager.start_host(&first.session_id).unwrap_err(),
            BROWSER_ERROR_LEASE_BLOCKED
        );
        let second = manager
            .create_owned_session("conversation-a", BrowserProfileMode::Persistent)
            .expect("create second browser");
        assert_eq!(
            manager.start_host(&second.session_id).unwrap_err(),
            BROWSER_ERROR_PROFILE_IN_USE
        );
    }

    /// Verifies a started host is adopted with a confirmed generation and process id.
    /// 验证已启动宿主被接管且代际与进程标识已确认。
    #[test]
    fn a_started_host_is_adopted_with_a_confirmed_generation() {
        let (manager, _alive) = manager_with_fake_host(Duration::from_millis(0));
        let session = manager
            .create_session(BrowserProfileMode::Persistent)
            .expect("create session");
        let running = manager.start_host(&session.session_id).expect("start host");
        assert_eq!(running.host_state, BrowserHostSupervisionState::Running);
        assert_eq!(running.host_generation, 1);
        assert!(manager.has_live_host(&session.session_id));
        assert_eq!(
            manager.confirmed_host_generation(&session.session_id),
            Some(1)
        );
    }

    /// Verifies navigation and viewport state change only through an authorized host response.
    /// 验证导航和视口状态仅通过已授权的宿主应答改变。
    #[test]
    fn host_backed_navigation_and_viewport_require_the_live_lease() {
        let (manager, _alive) = manager_with_fake_host(Duration::ZERO);
        let session = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create session");
        manager.start_host(&session.session_id).expect("start host");
        let page = manager
            .adopt_page(&session.session_id, "target-a", "about:blank", None)
            .expect("adopt page");
        let (_, secret) = manager
            .acquire_lease(&session.session_id, "user", None)
            .expect("acquire lease");
        assert_eq!(
            manager
                .navigate_page_on_host(
                    &session.session_id,
                    &page.page_id,
                    "https://example.test/new",
                    "wrong",
                )
                .unwrap_err(),
            BROWSER_ERROR_LEASE_REQUIRED
        );
        assert_eq!(
            manager
                .navigate_page_on_host(
                    &session.session_id,
                    &page.page_id,
                    "file:///secret.txt",
                    &secret,
                )
                .unwrap_err(),
            BROWSER_ERROR_INVALID_REQUEST
        );
        assert_eq!(
            manager
                .session_projection(&session.session_id)
                .expect("before")
                .page_generation,
            1
        );
        let navigated = manager
            .navigate_page_on_host(
                &session.session_id,
                &page.page_id,
                "https://example.test/new",
                &secret,
            )
            .expect("navigate host");
        assert_eq!(navigated.page_generation, 2);
        assert_eq!(
            navigated.current_url.as_deref(),
            Some("https://example.test/new")
        );
        assert_eq!(
            manager.list_pages(&session.session_id).expect("pages")[0].page_id,
            page.page_id
        );
        assert_eq!(
            manager
                .set_host_viewport(&session.session_id, 1024, 768, "wrong")
                .unwrap_err(),
            BROWSER_ERROR_LEASE_REQUIRED
        );
        let resized = manager
            .set_host_viewport(&session.session_id, 1024, 768, &secret)
            .expect("resize host");
        assert_eq!(resized.viewport_generation, 2);
        assert_eq!(
            resized.viewport_size,
            Some(BrowserViewportSize {
                width: 1024,
                height: 768
            })
        );
    }

    /// Verifies Core resolves a stable page id and discards a forged host target in input.
    /// 验证 Core 解析稳定页面标识并丢弃输入中伪造的宿主目标。
    #[test]
    fn host_input_uses_core_page_identity_instead_of_client_target() {
        let (manager, _alive) = manager_with_fake_host(Duration::ZERO);
        let session = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create session");
        manager.start_host(&session.session_id).expect("start host");
        let page = manager
            .adopt_page(&session.session_id, "target-a", "about:blank", None)
            .expect("adopt page");
        let (_, secret) = manager
            .acquire_lease(&session.session_id, "user", None)
            .expect("acquire lease");
        let event = BrowserInputEvent {
            sequence: String::from("1"),
            kind: String::from("pointer"),
            payload: json!({ "targetId": "target-b", "x": 10, "y": 10 }),
        };
        let applied = manager
            .dispatch_host_input(
                &session.session_id,
                &page.page_id,
                &secret,
                1,
                1,
                event.clone(),
            )
            .expect("dispatch input");
        assert_eq!(applied.outcome, BrowserInputOutcome::Applied);
        assert_eq!(applied.detail.as_ref().unwrap()["targetId"], "target-a");
        let rejected = manager
            .dispatch_host_input(&session.session_id, "missing-page", &secret, 1, 1, event)
            .expect("reject unknown page");
        assert_eq!(rejected.outcome, BrowserInputOutcome::Rejected);
        assert_eq!(
            rejected.reason.as_deref(),
            Some(BROWSER_ERROR_INVALID_REQUEST)
        );
    }

    /// Verifies slow host startup never blocks the global manager state lock.
    /// 验证缓慢的宿主启动绝不阻塞全局管理器状态锁。
    #[test]
    fn slow_host_startup_does_not_block_the_global_state_lock() {
        // The fake host delays its handshake long enough that a blocking implementation would
        // hold the state lock for the whole window.
        // 伪宿主延迟握手足够久，以致阻塞式实现会在整个窗口内持有状态锁。
        let (manager, _alive) = manager_with_fake_host(Duration::from_millis(600));
        let session = manager
            .create_session(BrowserProfileMode::Persistent)
            .expect("create session");
        let worker = {
            let manager = manager.clone();
            let session_id = session.session_id.clone();
            thread::spawn(move || manager.start_host(&session_id))
        };
        // While the host is starting, unrelated state reads must stay responsive.
        // 宿主启动期间，无关的状态读取必须保持可响应。
        thread::sleep(Duration::from_millis(150));
        let started = std::time::Instant::now();
        let sessions = manager
            .list_sessions()
            .expect("list sessions during startup");
        let elapsed = started.elapsed();
        assert_eq!(sessions.len(), 1);
        assert!(
            elapsed < Duration::from_millis(300),
            "listing sessions during host startup took {elapsed:?}; the state lock was held across host I/O"
        );
        worker
            .join()
            .expect("host startup thread")
            .expect("start host");
    }

    /// Verifies only browsers of one conversation reuse its persistent login state.
    /// 验证只有同一对话的浏览器复用其持久登录态。
    #[test]
    fn persistent_profiles_are_isolated_by_conversation() {
        let profiles_root =
            std::env::temp_dir().join(format!("vulcan-w06-modes-{}", Uuid::new_v4()));
        let config = BrowserHostRuntimeConfig {
            executable: std::path::PathBuf::from("fake-electron.exe"),
            entry: std::path::PathBuf::from("fake-host.js"),
            chromium_executable: std::path::PathBuf::from("fake-chromium.exe"),
            profiles_root: profiles_root.clone(),
            staging_root: profiles_root.join("staging"),
            discovery_timeout: Duration::from_secs(1),
            request_timeout: Duration::from_secs(1),
        };
        // Two browsers of one conversation reuse login state; a different conversation cannot.
        // 同一对话的两个浏览器复用登录态；不同对话不能复用。
        let first = config
            .launch_plan(
                "session-a",
                "conversation-a",
                BrowserProfileMode::Persistent,
                1,
                "secret",
            )
            .expect("first persistent plan");
        let second = config
            .launch_plan(
                "session-b",
                "conversation-a",
                BrowserProfileMode::Persistent,
                1,
                "secret",
            )
            .expect("second persistent plan");
        assert_eq!(first.user_data_directory, second.user_data_directory);
        let other = config
            .launch_plan(
                "session-c",
                "conversation-b",
                BrowserProfileMode::Persistent,
                1,
                "secret",
            )
            .expect("different conversation persistent plan");
        assert_ne!(first.user_data_directory, other.user_data_directory);
        assert_eq!(
            first.user_data_directory.parent(),
            Some(profiles_root.join("persistent").as_path())
        );
        let path_like_owner = config
            .launch_plan(
                "session-d",
                "../conversation-a",
                BrowserProfileMode::Persistent,
                1,
                "secret",
            )
            .expect("path-like owner remains confined");
        assert_eq!(
            path_like_owner.user_data_directory.parent(),
            Some(profiles_root.join("persistent").as_path())
        );
        // Ephemeral sessions must never share a directory with each other or with the persistent one.
        // 临时会话之间以及与会话持久目录之间绝不共享目录。
        let ephemeral_a = config
            .launch_plan(
                "session-a",
                "conversation-a",
                BrowserProfileMode::Ephemeral,
                1,
                "secret",
            )
            .expect("ephemeral plan a");
        let ephemeral_b = config
            .launch_plan(
                "session-b",
                "conversation-a",
                BrowserProfileMode::Ephemeral,
                1,
                "secret",
            )
            .expect("ephemeral plan b");
        assert_ne!(
            ephemeral_a.user_data_directory,
            ephemeral_b.user_data_directory
        );
        assert_ne!(ephemeral_a.user_data_directory, first.user_data_directory);
        assert_ne!(ephemeral_b.user_data_directory, first.user_data_directory);
    }

    /// Verifies a second concurrent persistent session is refused rather than sharing one profile.
    /// 验证第二个并发持久会话被拒绝，而不是共享同一画像。
    #[test]
    fn a_second_concurrent_persistent_session_is_refused() {
        let (manager, _alive) = manager_with_fake_host(Duration::from_millis(0));
        let first = manager
            .create_session(BrowserProfileMode::Persistent)
            .expect("first persistent session");
        manager.start_host(&first.session_id).expect("first host");
        let second = manager
            .create_session(BrowserProfileMode::Persistent)
            .expect("second persistent session");
        assert_eq!(
            manager.start_host(&second.session_id).unwrap_err(),
            BROWSER_ERROR_PROFILE_IN_USE,
            "two concurrent hosts must never share one Chromium user-data directory"
        );
        // An ephemeral session is unaffected because it owns its own directory.
        // 临时会话不受影响，因为它拥有自己的目录。
        let ephemeral = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("ephemeral session");
        assert!(manager.start_host(&ephemeral.session_id).is_ok());
    }

    /// Verifies a startup reserves its owner's profile before the slow host handshake completes.
    /// 验证缓慢宿主握手完成前，启动已预留所属对话的画像。
    #[test]
    fn in_flight_host_start_reserves_only_its_owners_persistent_profile() {
        let (manager, _alive) = manager_with_fake_host(Duration::from_millis(400));
        let first = manager
            .create_owned_session("conversation-a", BrowserProfileMode::Persistent)
            .expect("first browser");
        let worker = {
            let manager = manager.clone();
            let session_id = first.session_id.clone();
            thread::spawn(move || manager.start_host(&session_id))
        };
        for _ in 0..100 {
            if manager
                .inner
                .lock()
                .expect("manager state")
                .starting_hosts
                .contains(&first.session_id)
            {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(
            manager
                .inner
                .lock()
                .expect("manager state")
                .starting_hosts
                .contains(&first.session_id)
        );
        let same_owner = manager
            .create_owned_session("conversation-a", BrowserProfileMode::Persistent)
            .expect("second browser of same conversation");
        assert_eq!(
            manager.start_host(&same_owner.session_id).unwrap_err(),
            BROWSER_ERROR_PROFILE_IN_USE
        );
        let other_owner = manager
            .create_owned_session("conversation-b", BrowserProfileMode::Persistent)
            .expect("browser of another conversation");
        manager
            .start_host(&other_owner.session_id)
            .expect("another conversation may start independently");
        worker.join().expect("start worker").expect("first host");
    }

    /// Verifies an explicit close cannot be overwritten by a late host-start response.
    /// 验证显式关闭不会被迟到的宿主启动响应覆盖。
    #[test]
    fn closing_during_host_start_cannot_revive_the_browser() {
        let (manager, _alive) = manager_with_fake_host(Duration::from_millis(300));
        let session = manager
            .create_owned_session("conversation-a", BrowserProfileMode::Persistent)
            .expect("browser session");
        let worker = {
            let manager = manager.clone();
            let session_id = session.session_id.clone();
            thread::spawn(move || manager.start_host(&session_id))
        };
        for _ in 0..100 {
            if manager
                .inner
                .lock()
                .expect("manager state")
                .starting_hosts
                .contains(&session.session_id)
            {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(
            manager
                .inner
                .lock()
                .expect("manager state")
                .starting_hosts
                .contains(&session.session_id)
        );
        let closed = manager
            .close_session(&session.session_id)
            .expect("close browser");
        worker
            .join()
            .expect("start worker")
            .expect("startup completed before close");
        assert_eq!(closed.status, BrowserSessionStatus::Closed);
        assert!(!manager.has_live_host(&session.session_id));
        assert_eq!(
            manager
                .session_projection(&session.session_id)
                .expect("projection")
                .status,
            BrowserSessionStatus::Closed
        );
    }

    /// Verifies closing a persistent session frees the shared profile for the next session.
    /// 验证关闭持久会话会为下一个会话释放共享画像。
    #[test]
    fn closing_a_persistent_session_frees_the_shared_profile() {
        let (manager, alive) = manager_with_fake_host(Duration::from_millis(0));
        let first = manager
            .create_session(BrowserProfileMode::Persistent)
            .expect("first persistent session");
        manager.start_host(&first.session_id).expect("first host");
        manager
            .close_session(&first.session_id)
            .expect("close first");
        assert!(!alive.load(AtomicOrdering::SeqCst));
        // Closing ends the host, so the shared profile is no longer occupied.
        // 关闭会结束宿主，因此共享画像不再被占用。
        assert!(!manager.has_live_host(&first.session_id));
        let second = manager
            .create_session(BrowserProfileMode::Persistent)
            .expect("second persistent session");
        assert!(
            manager.start_host(&second.session_id).is_ok(),
            "a later persistent session must be able to reuse the shared profile"
        );
    }

    /// Verifies repeated open-close cycles cannot grow retained browser history without a bound.
    /// 验证反复开启和关闭不会让浏览器保留历史无界增长。
    #[test]
    fn closed_browser_history_is_bounded() {
        let manager = BrowserManager::new();
        let mut oldest = String::new();
        let mut newest = String::new();
        for index in 0..BROWSER_MAX_RETAINED_CLOSED_SESSIONS + 5 {
            let browser = manager
                .create_owned_session("conversation-a", BrowserProfileMode::Ephemeral)
                .expect("create browser");
            if index == 0 {
                oldest = browser.session_id.clone();
            }
            newest = browser.session_id.clone();
            manager
                .close_session(&browser.session_id)
                .expect("close browser");
        }
        assert_eq!(
            manager
                .list_owned_sessions("conversation-a")
                .expect("list browsers")
                .len(),
            BROWSER_MAX_RETAINED_CLOSED_SESSIONS
        );
        assert_eq!(
            manager.session_projection(&oldest).unwrap_err(),
            BROWSER_ERROR_UNKNOWN_SESSION
        );
        assert_eq!(
            manager
                .session_projection(&newest)
                .expect("newest history")
                .status,
            BrowserSessionStatus::Closed
        );
    }

    /// Verifies an expired AI lease cannot hand a still-attached Playwright socket to another holder.
    /// 验证到期的 AI 租约不会把仍挂接的 Playwright 套接字交给其他控制者。
    #[test]
    fn expired_attached_automation_lease_blocks_new_control_until_revoked() {
        let (manager, _alive) = manager_with_fake_host(Duration::ZERO);
        let session = manager
            .create_owned_session("conversation-a", BrowserProfileMode::Ephemeral)
            .expect("create browser");
        manager.start_host(&session.session_id).expect("start host");
        let (_, ai_secret) = manager
            .acquire_lease(
                &session.session_id,
                "ai:conversation-a",
                Some(Duration::from_millis(20)),
            )
            .expect("acquire AI lease");
        manager
            .attach_automation(&session.session_id, &ai_secret, "connection-a")
            .expect("attach automation");
        thread::sleep(Duration::from_millis(40));

        assert_eq!(
            manager
                .acquire_lease(&session.session_id, "user", None)
                .unwrap_err(),
            BROWSER_ERROR_AUTOMATION_CONFLICT
        );
        assert_eq!(
            manager
                .session_projection(&session.session_id)
                .expect("projection")
                .lease_state,
            BrowserLeaseState::Blocked
        );
        assert!(
            manager
                .release_lease(&session.session_id, &ai_secret)
                .is_err()
        );
        manager
            .confirm_operations_stopped(&session.session_id, None)
            .expect("ordinary confirmation");
        assert_eq!(
            manager
                .session_projection(&session.session_id)
                .expect("projection")
                .lease_state,
            BrowserLeaseState::Blocked,
            "general drain confirmation must not clear an attached gateway's expired lease"
        );

        manager
            .detach_automation(&session.session_id, "connection-a")
            .expect("confirmed gateway revocation");
        manager
            .confirm_operations_stopped(
                &session.session_id,
                Some(BROWSER_AUTOMATION_LEASE_EXPIRED_OPERATION),
            )
            .expect("confirm expired gateway stopped");
        manager
            .release_lease(&session.session_id, &ai_secret)
            .expect("release revoked AI lease");
        manager
            .acquire_lease(&session.session_id, "user", None)
            .expect("new holder after confirmed revocation");
    }

    /// Verifies one session's host requests serialize on a single dispatch point.
    /// 验证单个会话的宿主请求在唯一调度点上串行化。
    #[test]
    fn same_session_requests_serialize_on_one_dispatch_point() {
        let (manager, _alive) = manager_with_fake_host(Duration::from_millis(0));
        let session = manager
            .create_session(BrowserProfileMode::Persistent)
            .expect("create session");
        manager.start_host(&session.session_id).expect("start host");

        let workers: Vec<_> = (0..4)
            .map(|index| {
                let manager = manager.clone();
                let session_id = session.session_id.clone();
                thread::spawn(move || {
                    manager
                        .request_host(&session_id, "host.ping", json!({ "index": index }))
                        .expect("request host")
                })
            })
            .collect();
        for worker in workers {
            let outcome = worker.join().expect("request thread");
            assert!(
                matches!(outcome, BrowserHostRequestOutcome::Completed(_)),
                "every serialized request must complete, got {outcome:?}"
            );
        }
    }

    /// Verifies an exited host is reaped as a crash with a retained snapshot.
    /// 验证已退出宿主被作为崩溃回收并保留快照。
    #[test]
    fn an_exited_host_is_reaped_as_a_crash() {
        let (manager, alive) = manager_with_fake_host(Duration::from_millis(0));
        let session = manager
            .create_session(BrowserProfileMode::Persistent)
            .expect("create session");
        manager.start_host(&session.session_id).expect("start host");
        manager
            .adopt_page(
                &session.session_id,
                "target-a",
                "https://example.test/",
                None,
            )
            .expect("adopt page");

        // The process disappears without an explicit close, which is a crash.
        // 进程在没有显式关闭的情况下消失，即崩溃。
        alive.store(false, AtomicOrdering::SeqCst);
        let reaped = manager
            .reap_host(&session.session_id)
            .expect("reap host")
            .expect("a dead host must be reaped");
        assert_eq!(reaped.status, BrowserSessionStatus::Crashed);
        assert!(reaped.requires_explicit_reopen);
        assert_eq!(reaped.page_count, 0);
        assert!(!manager.has_live_host(&session.session_id));
        // Reaping again must not invent a second crash.
        // 再次回收不得凭空产生第二次崩溃。
        assert!(
            manager
                .reap_host(&session.session_id)
                .expect("reap again")
                .is_none()
        );
    }

    /// Keeps the owned host available when the OS cannot determine whether it exited.
    /// 操作系统无法确认宿主是否退出时保留归属句柄，以供后续复查。
    #[test]
    fn host_process_inspection_failure_preserves_ownership() {
        let (manager, alive, poll_error) = manager_with_fake_host_poll(Duration::ZERO, false);
        let session = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create session");
        manager.start_host(&session.session_id).expect("start host");

        poll_error.store(true, AtomicOrdering::SeqCst);
        let error = manager
            .reap_host(&session.session_id)
            .expect_err("inspection failure must not mean the process is running");
        assert!(error.contains("injected host process inspection failure"));
        assert!(manager.has_live_host(&session.session_id));
        assert_eq!(
            manager
                .session_projection(&session.session_id)
                .expect("projection after failed inspection")
                .status,
            BrowserSessionStatus::Running
        );

        poll_error.store(false, AtomicOrdering::SeqCst);
        alive.store(false, AtomicOrdering::SeqCst);
        let reaped = manager
            .reap_host(&session.session_id)
            .expect("inspect after recovery")
            .expect("exited host must be reaped");
        assert_eq!(reaped.status, BrowserSessionStatus::Crashed);
        assert!(!manager.has_live_host(&session.session_id));
    }

    /// Continues checking independent hosts after one transient process-inspection error.
    /// 一个进程检查暂时失败后继续检查其他独立宿主。
    #[test]
    fn one_host_inspection_failure_does_not_starve_other_sessions() {
        let (manager, alive, poll_error) = manager_with_fake_host_poll(Duration::ZERO, false);
        let first = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create first session");
        let second = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create second session");
        manager
            .start_host(&first.session_id)
            .expect("start first host");
        manager
            .start_host(&second.session_id)
            .expect("start second host");

        alive.store(false, AtomicOrdering::SeqCst);
        poll_error.store(true, AtomicOrdering::SeqCst);
        let error = manager
            .reap_hosts()
            .expect_err("one liveness inspection must fail explicitly");
        assert!(error.contains("injected host process inspection failure"));
        assert!(error.contains("reaped sessions:"));
        let remaining = [&first.session_id, &second.session_id]
            .into_iter()
            .filter(|session_id| manager.has_live_host(session_id))
            .count();
        assert_eq!(remaining, 1, "the other exited host must be reaped");

        let recovered = manager.reap_hosts().expect("retry remaining host");
        assert_eq!(recovered.len(), 1);
        assert!(!manager.has_live_host(&first.session_id));
        assert!(!manager.has_live_host(&second.session_id));
    }

    /// Blocks explicit close until a previously uninspectable host is confirmed exited.
    /// 无法检查宿主进程时阻塞显式关闭，直到确认该进程退出。
    #[test]
    fn explicit_close_blocks_on_process_inspection_failure() {
        let (manager, alive, poll_error) = manager_with_fake_host_poll(Duration::ZERO, false);
        let session = manager
            .create_session(BrowserProfileMode::Persistent)
            .expect("create session");
        manager.start_host(&session.session_id).expect("start host");

        poll_error.store(true, AtomicOrdering::SeqCst);
        let error = manager
            .close_session(&session.session_id)
            .expect_err("process inspection cannot confirm closure");
        assert!(error.contains(BROWSER_ERROR_LEASE_BLOCKED), "{error}");
        assert!(error.contains("injected host process inspection failure"));
        assert!(manager.has_live_host(&session.session_id));
        assert_eq!(
            manager
                .session_projection(&session.session_id)
                .expect("blocked projection")
                .lease_state,
            BrowserLeaseState::Blocked
        );
        assert!(!alive.load(AtomicOrdering::SeqCst));

        let closed = manager
            .reap_host(&session.session_id)
            .expect("recheck exact host exit")
            .expect("the stopped host must be retired");
        assert_eq!(closed.status, BrowserSessionStatus::Closed);
        assert!(!manager.has_live_host(&session.session_id));
    }

    /// Verifies a live host is not reaped by a liveness check.
    /// 验证存活检查不会回收仍在运行的宿主。
    #[test]
    fn a_live_host_is_not_reaped() {
        let (manager, _alive) = manager_with_fake_host(Duration::from_millis(0));
        let session = manager
            .create_session(BrowserProfileMode::Persistent)
            .expect("create session");
        manager.start_host(&session.session_id).expect("start host");
        assert!(
            manager
                .reap_host(&session.session_id)
                .expect("reap")
                .is_none()
        );
        assert!(manager.has_live_host(&session.session_id));
        assert!(
            manager.reap_hosts().expect("reap all").is_empty(),
            "no running host may be reported as crashed"
        );
    }

    /// Verifies Core shutdown closes every host channel so no host is orphaned.
    /// 验证 Core 关闭会关闭所有宿主通道，因此没有宿主被遗留为孤儿。
    #[test]
    fn core_shutdown_closes_every_host_channel() {
        let (manager, _alive) = manager_with_fake_host(Duration::from_millis(0));
        let session = manager
            .create_session(BrowserProfileMode::Persistent)
            .expect("create session");
        manager.start_host(&session.session_id).expect("start host");
        assert_eq!(manager.shutdown_hosts().expect("shutdown hosts"), 1);
        assert!(!manager.has_live_host(&session.session_id));
        // A second shutdown is a harmless no-op.
        // 第二次关闭是无害的空操作。
        assert_eq!(manager.shutdown_hosts().expect("shutdown again"), 0);
    }

    /// Verifies starting a host without runtime configuration reports a real error.
    /// 验证未配置运行时时启动宿主返回真实错误。
    #[test]
    fn starting_a_host_without_configuration_reports_a_real_error() {
        let manager = BrowserManager::new();
        let session = manager
            .create_session(BrowserProfileMode::Persistent)
            .expect("create session");
        assert_eq!(
            manager.start_host(&session.session_id).unwrap_err(),
            BROWSER_ERROR_CAPABILITY_UNSUPPORTED
        );
        assert_eq!(
            manager.start_host("missing").unwrap_err(),
            BROWSER_ERROR_CAPABILITY_UNSUPPORTED
        );
    }

    /// Verifies host requests reject a session that owns no live host.
    /// 验证宿主请求拒绝没有活动宿主的会话。
    #[test]
    fn host_requests_reject_a_session_without_a_live_host() {
        let (manager, _alive) = manager_with_fake_host(Duration::from_millis(0));
        let session = manager
            .create_session(BrowserProfileMode::Persistent)
            .expect("create session");
        assert_eq!(
            manager
                .request_host(&session.session_id, "host.ping", json!({}))
                .unwrap_err(),
            BROWSER_ERROR_UNKNOWN_SESSION
        );
    }

    /// Verifies an explicit takeover revokes the previous holder and rotates its credentials.
    /// 验证明确接管撤销前任持有者并轮换其凭据。
    #[test]
    fn takeover_revokes_the_previous_holder_and_rotates_credentials() {
        let manager = BrowserManager::new();
        let session = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create session");
        let (before, old_secret) = manager
            .acquire_lease(&session.session_id, "alpha", None)
            .expect("alpha acquires");
        let old_epoch = before.control_epoch;

        let (after, new_secret) = manager
            .takeover_lease(&session.session_id, "beta", None)
            .expect("beta takes over");
        assert_ne!(old_secret, new_secret, "a takeover must rotate the secret");
        assert!(
            after.control_epoch > old_epoch,
            "a takeover must advance the control epoch"
        );
        assert_eq!(after.lease_state, BrowserLeaseState::Held);

        // The previous holder's later commands must fail against the new epoch.
        // 前任持有者随后的命令必须在新代次下失败。
        assert_eq!(
            manager
                .dispatch_input(
                    &session.session_id,
                    &old_secret,
                    after.page_generation,
                    BrowserInputEvent {
                        sequence: String::from("1"),
                        kind: String::from("pointer"),
                        payload: json!({}),
                    },
                )
                .unwrap_err(),
            BROWSER_ERROR_LEASE_REQUIRED
        );
        // The new holder can act.
        // 新持有者可以操作。
        assert!(
            manager
                .dispatch_input(
                    &session.session_id,
                    &new_secret,
                    after.page_generation,
                    BrowserInputEvent {
                        sequence: String::from("2"),
                        kind: String::from("pointer"),
                        payload: json!({}),
                    },
                )
                .is_ok()
        );
    }

    /// Verifies an AI heartbeat retains its long lease instead of reverting to the short UI TTL.
    /// 验证 AI 心跳保持长租约，而不会退回较短的界面租约时长。
    #[test]
    fn automation_heartbeat_preserves_its_explicit_ttl() {
        let manager = BrowserManager::new();
        let session = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create browser session");
        let (_, secret) = manager
            .acquire_lease(
                &session.session_id,
                "ai-owner",
                Some(Duration::from_secs(1)),
            )
            .expect("acquire short initial lease");
        manager
            .heartbeat_lease_with_ttl(&session.session_id, &secret, Duration::from_secs(600))
            .expect("renew AI lease");
        let state = manager.inner.lock().expect("lock browser manager");
        let remaining = state.sessions[&session.session_id]
            .lease_expires_at
            .expect("lease expiry")
            .saturating_duration_since(Instant::now());
        assert!(remaining > Duration::from_secs(590));
    }

    /// Verifies input without a lease is rejected and changes nothing.
    /// 验证无租约的输入被拒绝且不改变任何状态。
    #[test]
    fn input_without_a_lease_is_rejected_and_changes_nothing() {
        let manager = BrowserManager::new();
        let session = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create session");
        assert_eq!(
            manager
                .dispatch_input(
                    &session.session_id,
                    "not-a-secret",
                    session.page_generation,
                    BrowserInputEvent {
                        sequence: String::from("1"),
                        kind: String::from("pointer"),
                        payload: json!({}),
                    },
                )
                .unwrap_err(),
            BROWSER_ERROR_LEASE_REQUIRED
        );
        // No lease exists, so nothing was queued for the host.
        // 不存在租约，因此没有内容被排队给宿主。
        assert_eq!(
            manager
                .session_projection(&session.session_id)
                .expect("projection")
                .lease_state,
            BrowserLeaseState::Free
        );
    }

    /// Rejects expired credentials at dispatch even when no competing client renewed or acquired.
    /// 即使没有竞争客户端续租或获取，也在派发处拒绝已到期凭据。
    #[test]
    fn input_checks_expiry_without_another_lease_operation() {
        let manager = BrowserManager::new();
        let session = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .unwrap();
        let (_, secret) = manager
            .acquire_lease(&session.session_id, "alpha", Some(Duration::ZERO))
            .unwrap();
        let error = manager
            .dispatch_input(
                &session.session_id,
                &secret,
                session.page_generation,
                BrowserInputEvent {
                    sequence: "1".into(),
                    kind: "text".into(),
                    payload: json!({"text":"must not arrive"}),
                },
            )
            .unwrap_err();
        assert_eq!(error, BROWSER_ERROR_LEASE_REQUIRED);
        assert_eq!(
            manager
                .session_projection(&session.session_id)
                .unwrap()
                .lease_state,
            BrowserLeaseState::Free
        );
    }

    /// Keeps uncertain work blocked after its holder releases credentials.
    /// 持有者释放凭据后，结果不确定的工作仍保持阻塞。
    #[test]
    fn release_cannot_bypass_an_unconfirmed_operation() {
        let manager = BrowserManager::new();
        let session = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .unwrap();
        let (_, secret) = manager
            .acquire_lease(&session.session_id, "alpha", None)
            .unwrap();
        manager
            .mark_operation_unconfirmed(&session.session_id, "input:unknown")
            .unwrap();
        assert_eq!(
            manager
                .release_lease(&session.session_id, &secret)
                .unwrap()
                .lease_state,
            BrowserLeaseState::Blocked
        );
        assert_eq!(
            manager
                .acquire_lease(&session.session_id, "beta", None)
                .unwrap_err(),
            BROWSER_ERROR_LEASE_CONFLICT
        );
    }

    /// Verifies a takeover blocked by unconfirmed work issues no lease and stays visible.
    /// 验证被未确认工作阻塞的接管不签发租约且保持可见。
    #[test]
    fn a_blocked_takeover_issues_no_new_lease() {
        let manager = BrowserManager::new();
        let session = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create session");
        let (_projection, holder_secret) = manager
            .acquire_lease(&session.session_id, "alpha", None)
            .expect("alpha acquires");
        // An in-flight script whose completion could not be confirmed.
        // 一个无法确认是否结束的在途脚本。
        let blocked = manager
            .mark_operation_unconfirmed(&session.session_id, "automation.attach:script-7")
            .expect("mark unconfirmed");
        assert_eq!(blocked.lease_state, BrowserLeaseState::Blocked);
        assert_eq!(
            blocked.blocked_reason.as_deref(),
            Some("automation.attach:script-7")
        );

        // The takeover is refused, and no new credentials are fabricated.
        // 接管被拒绝，且不伪造新凭据。
        let error = manager
            .takeover_lease(&session.session_id, "beta", None)
            .unwrap_err();
        assert!(
            error.starts_with(BROWSER_ERROR_LEASE_BLOCKED),
            "a blocked takeover must report lease_blocked, got {error}"
        );
        // Retaining the former identity for diagnosis must not allow further page mutations.
        // 为诊断保留原身份不能允许继续修改页面。
        let projection = manager
            .session_projection(&session.session_id)
            .expect("projection");
        assert_eq!(projection.lease_state, BrowserLeaseState::Blocked);
        assert_eq!(
            manager
                .dispatch_input(
                    &session.session_id,
                    &holder_secret,
                    projection.page_generation,
                    BrowserInputEvent {
                        sequence: String::from("1"),
                        kind: String::from("pointer"),
                        payload: json!({}),
                    },
                )
                .unwrap_err(),
            BROWSER_ERROR_LEASE_BLOCKED
        );
        assert_eq!(
            manager
                .unconfirmed_operations(&session.session_id)
                .expect("unconfirmed"),
            vec![String::from("automation.attach:script-7")]
        );
    }

    /// Verifies confirming the stopped work unblocks and then allows the takeover.
    /// 验证确认工作已停止后解除阻塞并允许接管。
    #[test]
    fn confirming_stopped_work_unblocks_the_takeover() {
        let manager = BrowserManager::new();
        let session = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create session");
        manager
            .acquire_lease(&session.session_id, "alpha", None)
            .expect("alpha acquires");
        manager
            .mark_operation_unconfirmed(&session.session_id, "automation.attach:script-7")
            .expect("mark unconfirmed");
        assert!(
            manager
                .takeover_lease(&session.session_id, "beta", None)
                .is_err()
        );

        // Only an explicit confirmation clears the block; nothing guesses that work stopped.
        // 只有明确确认才会解除阻塞；绝不猜测工作已经停止。
        let cleared = manager
            .confirm_operations_stopped(&session.session_id, None)
            .expect("confirm stopped");
        assert!(cleared.blocked_reason.is_none());
        assert_eq!(cleared.lease_state, BrowserLeaseState::Held);

        let (taken, _secret) = manager
            .takeover_lease(&session.session_id, "beta", None)
            .expect("takeover after confirmation");
        assert_eq!(taken.lease_state, BrowserLeaseState::Held);
    }

    /// Verifies a single confirmed operation does not clear the others.
    /// 验证确认单项操作不会清除其他项。
    #[test]
    fn confirming_one_operation_keeps_the_others() {
        let manager = BrowserManager::new();
        let session = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create session");
        manager
            .mark_operation_unconfirmed(&session.session_id, "operation-a")
            .expect("mark a");
        manager
            .mark_operation_unconfirmed(&session.session_id, "operation-b")
            .expect("mark b");
        // Re-marking the same operation must not duplicate it.
        // 重复标记同一操作不得使其重复。
        manager
            .mark_operation_unconfirmed(&session.session_id, "operation-a")
            .expect("mark a again");
        assert_eq!(
            manager
                .unconfirmed_operations(&session.session_id)
                .expect("unconfirmed"),
            vec![String::from("operation-a"), String::from("operation-b")]
        );

        let partial = manager
            .confirm_operations_stopped(&session.session_id, Some("operation-a"))
            .expect("confirm a");
        assert_eq!(partial.lease_state, BrowserLeaseState::Blocked);
        assert_eq!(
            manager
                .unconfirmed_operations(&session.session_id)
                .expect("unconfirmed"),
            vec![String::from("operation-b")]
        );
    }

    /// Verifies takeover rejects an empty holder or an unknown session.
    /// 验证接管拒绝空持有者或未知会话。
    #[test]
    fn takeover_rejects_an_empty_holder_or_unknown_session() {
        let manager = BrowserManager::new();
        let session = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create session");
        assert_eq!(
            manager
                .takeover_lease(&session.session_id, "  ", None)
                .unwrap_err(),
            BROWSER_ERROR_INVALID_REQUEST
        );
        assert_eq!(
            manager.takeover_lease("missing", "beta", None).unwrap_err(),
            BROWSER_ERROR_UNKNOWN_SESSION
        );
    }

    /// Verifies input computed for an old viewport is rejected after the viewport changes.
    /// 验证视口改变后，针对旧视口计算的输入被拒绝。
    #[test]
    fn input_from_a_stale_viewport_is_rejected() {
        let manager = BrowserManager::new();
        let session = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create session");
        let (projection, secret) = manager
            .acquire_lease(&session.session_id, "alpha", None)
            .expect("acquire");
        let original_viewport = projection.viewport_generation;

        // The viewport changes, which invalidates every coordinate measured before it.
        // 视口发生变化，使此前测量的每个坐标失效。
        let resized = manager
            .set_viewport(&session.session_id, 1280, 800)
            .expect("set viewport");
        assert!(resized.viewport_generation > original_viewport);
        assert_eq!(resized.viewport_size.map(|size| size.width), Some(1280));

        // Input computed against the old viewport must be rejected, not silently reinterpreted.
        // 针对旧视口计算的输入必须被拒绝，而不是被静默重新解释。
        let stale = manager
            .dispatch_input_outcome(
                &session.session_id,
                &secret,
                resized.page_generation,
                original_viewport,
                BrowserInputEvent {
                    sequence: String::from("1"),
                    kind: String::from("pointer"),
                    payload: json!({ "x": 10, "y": 20 }),
                },
            )
            .expect("dispatch stale");
        assert_eq!(stale.outcome, BrowserInputOutcome::Rejected);
        assert_eq!(stale.reason.as_deref(), Some(BROWSER_ERROR_STALE_VIEWPORT));
        assert!(stale.sequence.is_none());

        // Input against the new viewport is accepted, so the client can act after a fresh frame.
        // 针对新视口的输入被接受，因此客户端可以在取得新帧后操作。
        let fresh = manager
            .dispatch_input_outcome(
                &session.session_id,
                &secret,
                resized.page_generation,
                resized.viewport_generation,
                BrowserInputEvent {
                    sequence: String::from("2"),
                    kind: String::from("pointer"),
                    payload: json!({ "x": 10, "y": 20 }),
                },
            )
            .expect("dispatch fresh");
        assert_eq!(fresh.outcome, BrowserInputOutcome::Accepted);
        assert!(fresh.sequence.is_some());
    }

    /// Verifies the four outcomes carry their real meaning.
    /// 验证四种结果各自承载其真实含义。
    #[test]
    fn input_outcomes_carry_their_real_meaning() {
        let manager = BrowserManager::new();
        let session = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create session");
        let (projection, secret) = manager
            .acquire_lease(&session.session_id, "alpha", None)
            .expect("acquire");
        let event = |sequence: &str| BrowserInputEvent {
            sequence: String::from(sequence),
            kind: String::from("pointer"),
            payload: json!({}),
        };

        // Rejected: a failed check means nothing was attempted.
        // Rejected：检查失败意味着未尝试任何操作。
        let rejected = manager
            .dispatch_input_outcome(
                &session.session_id,
                "wrong-secret",
                projection.page_generation,
                projection.viewport_generation,
                event("1"),
            )
            .expect("dispatch");
        assert_eq!(rejected.outcome, BrowserInputOutcome::Rejected);
        assert_eq!(
            rejected.reason.as_deref(),
            Some(BROWSER_ERROR_LEASE_REQUIRED)
        );

        // Rejected: a stale page generation is refused before it reaches the host.
        // Rejected：过时页面代次在到达宿主之前即被拒绝。
        let stale = manager
            .dispatch_input_outcome(
                &session.session_id,
                &secret,
                projection.page_generation + 5,
                projection.viewport_generation,
                event("2"),
            )
            .expect("dispatch");
        assert_eq!(stale.outcome, BrowserInputOutcome::Rejected);
        assert_eq!(
            stale.reason.as_deref(),
            Some(BROWSER_ERROR_STALE_GENERATION)
        );

        // Accepted: handed to the host, application not yet confirmed.
        // Accepted：已交给宿主，应用结果尚未确认。
        let accepted = manager
            .dispatch_input_outcome(
                &session.session_id,
                &secret,
                projection.page_generation,
                projection.viewport_generation,
                event("3"),
            )
            .expect("dispatch");
        assert_eq!(accepted.outcome, BrowserInputOutcome::Accepted);
        let sequence = accepted.sequence.expect("accepted sequence");
        assert_eq!(
            manager
                .input_outcome(&session.session_id, sequence)
                .expect("outcome"),
            Some(BrowserInputOutcome::Accepted)
        );

        // Applied: the host confirmed it reached the page.
        // Applied：宿主确认其已到达页面。
        manager
            .record_input_applied(&session.session_id, sequence)
            .expect("record applied");
        assert_eq!(
            manager
                .input_outcome(&session.session_id, sequence)
                .expect("outcome"),
            Some(BrowserInputOutcome::Applied)
        );

        // Unknown: the outcome cannot be determined, and the session blocks a takeover rather than
        // pretending the input did not happen.
        // Unknown：结果无法确定，会话改为阻塞接管，而不是假定该输入未发生。
        let unknown_projection = manager
            .record_input_unknown(&session.session_id, sequence)
            .expect("record unknown");
        assert_eq!(
            manager
                .input_outcome(&session.session_id, sequence)
                .expect("outcome"),
            Some(BrowserInputOutcome::Unknown)
        );
        assert_eq!(unknown_projection.lease_state, BrowserLeaseState::Blocked);
        assert!(
            manager
                .takeover_lease(&session.session_id, "beta", None)
                .is_err(),
            "an input of unknown effect must block a takeover"
        );
        // Confirming the operation stopped clears both the block and the outstanding operation.
        // 确认操作已停止会同时清除阻塞与未结束操作。
        manager
            .confirm_operations_stopped(
                &session.session_id,
                Some(&format!("input.dispatch:{sequence}")),
            )
            .expect("confirm stopped");
        assert!(
            manager
                .unconfirmed_operations(&session.session_id)
                .expect("list")
                .is_empty()
        );
    }

    /// Verifies input sequence bookkeeping stays bounded as inputs accumulate.
    /// 验证输入序号记录随着输入累积保持有界。
    #[test]
    fn input_outcome_bookkeeping_stays_bounded() {
        let manager = BrowserManager::new();
        let session = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create session");
        let (projection, secret) = manager
            .acquire_lease(&session.session_id, "alpha", None)
            .expect("acquire");
        for index in 0..(BROWSER_MAX_QUEUED_INPUTS + 40) {
            let outcome = manager
                .dispatch_input_outcome(
                    &session.session_id,
                    &secret,
                    projection.page_generation,
                    projection.viewport_generation,
                    BrowserInputEvent {
                        sequence: index.to_string(),
                        kind: String::from("pointer"),
                        payload: json!({}),
                    },
                )
                .expect("dispatch");
            assert_eq!(outcome.outcome, BrowserInputOutcome::Accepted);
        }
        // The queue and the outcome map are both bounded, so a fast client cannot grow memory.
        // 队列与结果映射均为有界，因此高速客户端无法使内存增长。
        let state = manager.inner.lock().expect("lock");
        let record = state.sessions.get(&session.session_id).expect("session");
        assert!(record.queued_inputs.len() <= BROWSER_MAX_QUEUED_INPUTS);
        assert!(record.input_outcomes.len() <= BROWSER_MAX_QUEUED_INPUTS);
    }

    /// Verifies an empty sequence or a zero-sized viewport is refused.
    /// 验证空序号或零尺寸视口被拒绝。
    #[test]
    fn empty_sequences_and_zero_sized_viewports_are_refused() {
        let manager = BrowserManager::new();
        let session = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create session");
        let (projection, secret) = manager
            .acquire_lease(&session.session_id, "alpha", None)
            .expect("acquire");
        let empty = manager
            .dispatch_input_outcome(
                &session.session_id,
                &secret,
                projection.page_generation,
                projection.viewport_generation,
                BrowserInputEvent {
                    sequence: String::from("   "),
                    kind: String::from("pointer"),
                    payload: json!({}),
                },
            )
            .expect("dispatch");
        assert_eq!(empty.outcome, BrowserInputOutcome::Rejected);
        assert_eq!(empty.reason.as_deref(), Some(BROWSER_ERROR_INVALID_REQUEST));
        assert_eq!(
            manager
                .set_viewport(&session.session_id, 0, 800)
                .unwrap_err(),
            BROWSER_ERROR_INVALID_REQUEST
        );
        assert_eq!(
            manager.set_viewport("missing", 100, 100).unwrap_err(),
            BROWSER_ERROR_UNKNOWN_SESSION
        );
    }

    /// Verifies a stream ticket authorises exactly one connection.
    /// 验证流凭据仅授权一次连接。
    #[test]
    fn a_stream_ticket_authorises_exactly_one_connection() {
        let manager = BrowserManager::new();
        let session = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create session");
        let ticket = manager
            .issue_stream_ticket(&session.session_id)
            .expect("issue ticket");
        assert_eq!(
            manager
                .consume_stream_ticket(&ticket.ticket)
                .expect("first use"),
            session.session_id
        );
        // A replayed ticket must never open a second stream.
        // 重放的凭据绝不能被用来打开第二条流。
        assert_eq!(
            manager.consume_stream_ticket(&ticket.ticket).unwrap_err(),
            BROWSER_ERROR_INVALID_REQUEST
        );
        // An invented ticket is refused just like a spent one.
        // 伪造的凭据与已用尽的凭据同样被拒绝。
        assert_eq!(
            manager.consume_stream_ticket("not-a-ticket").unwrap_err(),
            BROWSER_ERROR_INVALID_REQUEST
        );
        assert_eq!(
            manager.issue_stream_ticket("missing").unwrap_err(),
            BROWSER_ERROR_UNKNOWN_SESSION
        );
    }

    /// Verifies unconsumed tickets remain bounded and a consumed slot becomes reusable.
    /// 验证未消费凭据数量有界，消费后名额可以复用。
    #[test]
    fn pending_stream_tickets_have_a_per_session_limit() {
        // manager owns a single isolated browser session with no host process.
        // manager 持有一个无需宿主进程的隔离浏览器会话。
        let manager = BrowserManager::new();
        // session identifies the exact owner of the bounded ticket map.
        // session 标识有界凭据表的精确所属浏览器。
        let session = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create session");
        // tickets retain the grant IDs until the limit is reached.
        // tickets 保留签发 ID，直至达到上限。
        let tickets = (0..BROWSER_MAX_PENDING_STREAM_TICKETS)
            .map(|_| {
                manager
                    .issue_stream_ticket(&session.session_id)
                    .expect("issue ticket")
            })
            .collect::<Vec<_>>();
        assert_eq!(
            manager
                .issue_stream_ticket(&session.session_id)
                .unwrap_err(),
            BROWSER_ERROR_STREAM_TICKET_LIMIT,
        );
        assert_eq!(
            manager
                .consume_stream_ticket(&tickets[0].ticket)
                .expect("consume one ticket"),
            session.session_id,
        );
        manager
            .issue_stream_ticket(&session.session_id)
            .expect("issue after consumption");
    }

    /// Verifies direct and resumed observers share one bound and unsubscribe clears stream ownership.
    /// 验证直接观察与恢复观察共用一个上限，取消订阅也清除流所有权。
    #[test]
    fn observation_subscriptions_have_a_per_session_limit() {
        // manager owns the one session whose frame queues are bounded.
        // manager 持有帧队列受限的单个会话。
        let manager = BrowserManager::new();
        // session identifies the browser whose observer resources are allocated.
        // session 标识分配观察资源的浏览器。
        let session = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create session");
        // subscriptions retain every allocated observer slot until one is released.
        // subscriptions 保留已分配的全部观察名额，直至释放其中一个。
        let subscriptions = (0..BROWSER_MAX_OBSERVERS)
            .map(|_| {
                manager
                    .subscribe_observer(&session.session_id)
                    .expect("subscribe")
            })
            .collect::<Vec<_>>();
        assert_eq!(
            manager.subscribe_observer(&session.session_id).unwrap_err(),
            BROWSER_ERROR_OBSERVER_LIMIT,
        );
        assert_eq!(
            manager
                .resume_stream(&session.session_id, "another-stream", None)
                .unwrap_err(),
            BROWSER_ERROR_OBSERVER_LIMIT,
        );
        manager
            .unsubscribe_observer(&session.session_id, &subscriptions[0])
            .expect("unsubscribe");
        manager
            .resume_stream(&session.session_id, "replacement-stream", None)
            .expect("resume after slot release");
        manager
            .unsubscribe_observer(&session.session_id, "replacement-stream")
            .expect("unsubscribe resumed stream");
        assert!(
            !manager
                .release_stream_subscription(&session.session_id, "replacement-stream")
                .expect("stream ownership was cleared")
        );
    }

    /// Verifies a client with no cursor must resynchronise from a snapshot.
    /// 验证没有游标的客户端必须从快照重新同步。
    #[test]
    fn a_stream_without_a_cursor_resynchronises_from_a_snapshot() {
        let manager = BrowserManager::new();
        let session = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create session");
        let subscription = manager
            .subscribe_observer(&session.session_id)
            .expect("subscribe");
        manager
            .publish_frame(
                &session.session_id,
                session.page_generation,
                "image/webp",
                vec![1, 2, 3],
            )
            .expect("publish frame");

        let resume = manager
            .resume_stream(&session.session_id, &subscription, None)
            .expect("resume");
        assert_eq!(resume.mode, BrowserStreamResumeMode::Snapshot);
        assert_eq!(
            resume.frames.len(),
            1,
            "a snapshot carries the retained frames"
        );
        assert_eq!(resume.discarded_inputs, 0);
    }

    /// Verifies a stale page generation forces a snapshot instead of a silent continuation.
    /// 验证过时页面代次强制快照，而不是静默继续。
    #[test]
    fn a_stale_generation_forces_a_snapshot() {
        let manager = BrowserManager::new();
        let session = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create session");
        let subscription = manager
            .subscribe_observer(&session.session_id)
            .expect("subscribe");
        manager
            .publish_frame(
                &session.session_id,
                session.page_generation,
                "image/webp",
                vec![9],
            )
            .expect("publish frame");
        let resume = manager
            .resume_stream(
                &session.session_id,
                &subscription,
                Some(BrowserStreamCursor {
                    // A generation that is not the current one cannot be extended.
                    // 非当前代次无法被延续。
                    page_generation: session.page_generation + 3,
                    frame_sequence: Some(1),
                }),
            )
            .expect("resume");
        assert_eq!(resume.mode, BrowserStreamResumeMode::Snapshot);
    }

    /// Verifies a contiguous cursor resumes incrementally and delivers only newer frames.
    /// 验证连续游标以增量方式恢复且只交付更新的帧。
    #[test]
    fn a_contiguous_cursor_resumes_incrementally() {
        let manager = BrowserManager::new();
        let session = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create session");
        let subscription = manager
            .subscribe_observer(&session.session_id)
            .expect("subscribe");
        for marker in 0..4_u8 {
            manager
                .publish_frame(
                    &session.session_id,
                    session.page_generation,
                    "image/webp",
                    vec![marker],
                )
                .expect("publish frame");
        }
        // Frames 1..4 exist; a client that confirmed frame 2 needs only frames 3 and 4.
        // 已存在帧 1..4；已确认帧 2 的客户端只需要帧 3 与 4。
        let resume = manager
            .resume_stream(
                &session.session_id,
                &subscription,
                Some(BrowserStreamCursor {
                    page_generation: session.page_generation,
                    frame_sequence: Some(2),
                }),
            )
            .expect("resume");
        assert_eq!(resume.mode, BrowserStreamResumeMode::Incremental);
        assert_eq!(resume.frames.len(), 2);
        assert_eq!(resume.frames[0].sequence, "3");
        assert_eq!(resume.frames[1].sequence, "4");
        assert_eq!(resume.dropped_frames, 0);
    }

    /// Verifies an observer snapshot never consumes another controller's pending input.
    /// 验证观察者快照绝不消费另一个控制者的待处理输入。
    #[test]
    fn a_snapshot_resume_preserves_controller_input_without_replaying_it() {
        let manager = BrowserManager::new();
        let session = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create session");
        let (projection, secret) = manager
            .acquire_lease(&session.session_id, "alpha", None)
            .expect("acquire");
        for index in 0..3 {
            manager
                .dispatch_input(
                    &session.session_id,
                    &secret,
                    projection.page_generation,
                    BrowserInputEvent {
                        sequence: index.to_string(),
                        kind: String::from("pointer"),
                        payload: json!({}),
                    },
                )
                .expect("dispatch");
        }
        let resume = manager
            .resume_stream(&session.session_id, "observer-a", None)
            .expect("resume");
        assert_eq!(resume.mode, BrowserStreamResumeMode::Snapshot);
        assert_eq!(resume.discarded_inputs, 0);
        // Nothing is replayed by the stream, while controller-owned input remains untouched.
        // 流不重放任何输入，同时控制者拥有的输入保持不变。
        let state = manager.inner.lock().expect("lock");
        let record = state.sessions.get(&session.session_id).expect("session");
        assert_eq!(record.queued_inputs.len(), 3);
    }

    /// Verifies a resume rejects an unnamed subscription or unknown session.
    /// 验证恢复拒绝未命名订阅或未知会话。
    #[test]
    fn stream_resume_rejects_an_unnamed_subscription_or_unknown_session() {
        let manager = BrowserManager::new();
        let session = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create session");
        assert_eq!(
            manager
                .resume_stream(&session.session_id, "  ", None)
                .unwrap_err(),
            BROWSER_ERROR_INVALID_REQUEST
        );
        assert_eq!(
            manager
                .resume_stream("missing", "observer", None)
                .unwrap_err(),
            BROWSER_ERROR_UNKNOWN_SESSION
        );
    }

    /// Verifies a stream subscriber receives frames without a prior explicit subscribe call.
    /// 验证流订阅者无需事先显式订阅即可收到帧。
    ///
    /// Regression guard: a stream connection used to appear healthy while frame publication had no
    /// destination for it, which is indistinguishable from a frozen picture.
    /// 回归保护：流连接曾经看似正常，但帧发布对它没有投递目标——这与画面冻结无法区分。
    #[test]
    fn a_stream_subscriber_receives_published_frames() {
        let manager = BrowserManager::new();
        let session = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create session");
        // The first resume is what a stream connection performs.
        // 首次恢复正是流连接所执行的动作。
        let initial = manager
            .resume_stream(&session.session_id, "observer-a", None)
            .expect("initial resume");
        assert_eq!(initial.mode, BrowserStreamResumeMode::Snapshot);
        assert!(initial.frames.is_empty());

        manager
            .publish_frame(
                &session.session_id,
                session.page_generation,
                "image/png",
                vec![7, 7, 7],
            )
            .expect("publish frame");

        let resumed = manager
            .resume_stream(
                &session.session_id,
                "observer-a",
                Some(BrowserStreamCursor {
                    page_generation: session.page_generation,
                    frame_sequence: Some(0),
                }),
            )
            .expect("resume after publish");
        assert_eq!(
            resumed.frames.len(),
            1,
            "a stream subscriber must receive frames published after it connected"
        );
        assert_eq!(resumed.frames[0].mime_type, "image/png");
    }

    /// Verifies a disconnecting observer releases only its own subscription.
    /// 验证断开的观察端只释放它自己的订阅。
    #[test]
    fn a_disconnecting_observer_releases_only_its_own_subscription() {
        let manager = BrowserManager::new();
        let session = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create session");
        // Two stream observers plus one explicit observer subscription.
        // 两个流观察端，外加一个显式观察订阅。
        manager
            .resume_stream(&session.session_id, "observer-a", None)
            .expect("observer a");
        manager
            .resume_stream(&session.session_id, "observer-b", None)
            .expect("observer b");
        let explicit = manager
            .subscribe_observer(&session.session_id)
            .expect("explicit observer");
        manager
            .publish_frame(
                &session.session_id,
                session.page_generation,
                "image/png",
                vec![1],
            )
            .expect("publish");

        // Observer A goes away.
        // 观察端 A 断开。
        assert!(
            manager
                .release_stream_subscription(&session.session_id, "observer-a")
                .expect("release a")
        );
        let remaining = manager
            .observation_subscription_ids(&session.session_id)
            .expect("subscriptions");
        assert!(!remaining.contains(&String::from("observer-a")));
        // The session, the other stream observer, and the explicit subscription all survive.
        // 会话、另一个流观察端与显式订阅全部存活。
        assert!(remaining.contains(&String::from("observer-b")));
        assert!(remaining.contains(&explicit));
        assert_eq!(
            manager
                .retained_frame_count(&session.session_id, "observer-b")
                .expect("count b"),
            1,
            "the surviving observer keeps its retained frames"
        );
        assert_eq!(
            manager
                .session_projection(&session.session_id)
                .expect("projection")
                .status,
            BrowserSessionStatus::Running,
            "closing an observer must not end the session"
        );
        // Released twice is not an error, and an explicit subscription is never released this way.
        // 重复释放不是错误，且显式订阅绝不会以此方式被释放。
        assert!(
            !manager
                .release_stream_subscription(&session.session_id, "observer-a")
                .expect("release again")
        );
        assert!(
            !manager
                .release_stream_subscription(&session.session_id, &explicit)
                .expect("release explicit")
        );
    }

    /// Verifies cancellation during first-frame capture cannot recreate the stream on initial resume.
    /// 验证首帧抓取期间发生取消时，初次恢复不能重建流。
    #[test]
    fn cancelled_page_bound_stream_cannot_reappear_on_initial_resume() {
        let manager = BrowserManager::new();
        let session = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create session");
        manager
            .record_host_running(&session.session_id, 4242)
            .expect("record host");
        let page = manager
            .adopt_page(&session.session_id, "target-1", "about:blank", None)
            .expect("adopt page");
        manager
            .bind_stream_page(&session.session_id, &page.page_id, "initial-stream")
            .expect("bind before capture");
        manager
            .unsubscribe_observer(&session.session_id, "initial-stream")
            .expect("cancel during capture");
        assert_eq!(
            manager
                .resume_bound_stream(&session.session_id, "initial-stream", None)
                .expect_err("cancelled initial stream must not restart"),
            BROWSER_ERROR_INVALID_REQUEST
        );
        assert!(
            !manager
                .observation_subscription_ids(&session.session_id)
                .expect("subscriptions")
                .contains(&String::from("initial-stream"))
        );
        assert_eq!(
            manager
                .bind_stream_page(&session.session_id, &page.page_id, "initial-stream")
                .expect_err("old socket still owns the cancelled identity"),
            BROWSER_ERROR_LEASE_CONFLICT
        );
        assert!(
            manager
                .release_stream_subscription(&session.session_id, "initial-stream")
                .expect("old socket releases cancelled identity")
        );
        manager
            .bind_stream_page(&session.session_id, &page.page_id, "initial-stream")
            .expect("identity can be reused after old socket releases it");
    }

    /// Verifies cancelled sockets continue occupying a bounded slot until their release completes.
    /// 验证已取消套接字在完成释放前仍占用有界名额。
    #[test]
    fn cancelled_page_bound_streams_cannot_bypass_observer_limit() {
        let manager = BrowserManager::new();
        let session = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create session");
        manager
            .record_host_running(&session.session_id, 4242)
            .expect("record host");
        let page = manager
            .adopt_page(&session.session_id, "target-1", "about:blank", None)
            .expect("adopt page");
        for index in 0..BROWSER_MAX_OBSERVERS {
            let subscription_id = format!("cancelled-{index}");
            manager
                .bind_stream_page(&session.session_id, &page.page_id, &subscription_id)
                .expect("bind stream");
            manager
                .unsubscribe_observer(&session.session_id, &subscription_id)
                .expect("cancel stream");
        }
        assert_eq!(
            manager
                .subscribe_observer(&session.session_id)
                .expect_err("cancelled sockets must still count against the limit"),
            BROWSER_ERROR_OBSERVER_LIMIT
        );
        assert!(
            manager
                .release_stream_subscription(&session.session_id, "cancelled-0")
                .expect("release old socket")
        );
        manager
            .subscribe_observer(&session.session_id)
            .expect("released slot is available");
    }

    /// Verifies closure revokes stream tickets and active or cancelled socket identities.
    /// 验证关闭会话会撤销流凭据及活动或已取消的套接字标识。
    #[test]
    fn closing_browser_clears_all_stream_ownership() {
        let manager = BrowserManager::new();
        let session = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create session");
        manager
            .record_host_running(&session.session_id, 4242)
            .expect("record host");
        let page = manager
            .adopt_page(&session.session_id, "target-1", "about:blank", None)
            .expect("adopt page");
        manager
            .bind_stream_page(&session.session_id, &page.page_id, "active-stream")
            .expect("bind active stream");
        manager
            .bind_stream_page(&session.session_id, &page.page_id, "cancelled-stream")
            .expect("bind stream to cancel");
        manager
            .unsubscribe_observer(&session.session_id, "cancelled-stream")
            .expect("cancel stream");
        let ticket = manager
            .issue_stream_ticket(&session.session_id)
            .expect("issue ticket before close");
        manager
            .close_session(&session.session_id)
            .expect("close session");
        assert_eq!(
            manager.consume_stream_ticket(&ticket.ticket).unwrap_err(),
            BROWSER_ERROR_INVALID_REQUEST
        );
        assert_eq!(
            manager
                .issue_stream_ticket(&session.session_id)
                .unwrap_err(),
            BROWSER_ERROR_INVALID_REQUEST
        );
        for subscription_id in ["active-stream", "cancelled-stream"] {
            assert!(
                !manager
                    .release_stream_subscription(&session.session_id, subscription_id)
                    .expect("late socket release finds no ownership")
            );
        }
    }

    /// Verifies a closed browser releases staged uploads and its disposable profile on disk.
    /// 验证已关闭浏览器会释放磁盘上的上传暂存文件及临时画像。
    #[test]
    fn closing_browser_removes_temporary_files_but_preserves_persistent_profiles() {
        // The fixture root is unique and removed automatically after the test.
        // 夹具根目录唯一，并在测试结束后自动删除。
        let root = tempfile::tempdir().expect("isolated browser data root");
        // Profile and staging roots mirror the production runtime layout.
        // 画像根目录与暂存根目录对应正式运行时布局。
        let profiles_root = root.path().join("profiles");
        let staging_root = root.path().join("staging");
        // The manager owns only this isolated fixture configuration.
        // 管理器只拥有这份隔离夹具配置。
        let manager = BrowserManager::new();
        manager
            .configure_host_runtime(BrowserHostRuntimeConfig {
                executable: root.path().join("host"),
                entry: root.path().join("entry"),
                chromium_executable: root.path().join("chromium"),
                profiles_root: profiles_root.clone(),
                staging_root: staging_root.clone(),
                discovery_timeout: Duration::from_secs(1),
                request_timeout: Duration::from_secs(1),
            })
            .expect("configure isolated host root");
        // The ephemeral session is the only browser whose files may be removed.
        // 该临时会话是唯一允许删除文件的浏览器。
        let session = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create ephemeral browser");
        // The upload record supplies the exact on-disk file asserted below.
        // 上传记录提供下文核对的磁盘文件精确路径。
        let upload = manager
            .stage_upload(&session.session_id, "中文文件.txt", b"content".to_vec())
            .expect("stage upload");
        // These two profile paths distinguish disposable data from persistent data.
        // 两条画像路径用于区分可丢弃数据与持久数据。
        let ephemeral = profiles_root.join("ephemeral").join(&session.session_id);
        let persistent = profiles_root.join("persistent").join("keep");
        std::fs::create_dir_all(&ephemeral).expect("create disposable profile");
        std::fs::create_dir_all(&persistent).expect("create persistent profile");
        assert!(std::path::Path::new(&upload.staged_path).is_file());
        manager
            .close_session(&session.session_id)
            .expect("close browser");
        assert!(!staging_root.join(&session.session_id).exists());
        assert!(!ephemeral.exists());
        assert!(persistent.is_dir());
        assert!(
            manager
                .uploads(&session.session_id)
                .expect("list uploads")
                .is_empty()
        );
    }

    /// Verifies unsafe names cannot create special files and staged files have a session quota.
    /// 验证不安全名称不能创建特殊文件，暂存文件也受到会话额度约束。
    #[test]
    fn staged_upload_rejects_unsafe_names_and_count_overflow() {
        // The disposable root keeps every staged file outside the real service data tree.
        // 可丢弃根目录使全部暂存文件远离真实服务数据树。
        let root = tempfile::tempdir().expect("isolated browser data root");
        let staging_root = root.path().join("staging");
        let manager = BrowserManager::new();
        manager
            .configure_host_runtime(BrowserHostRuntimeConfig {
                executable: root.path().join("host"),
                entry: root.path().join("entry"),
                chromium_executable: root.path().join("chromium"),
                profiles_root: root.path().join("profiles"),
                staging_root: staging_root.clone(),
                discovery_timeout: Duration::from_secs(1),
                request_timeout: Duration::from_secs(1),
            })
            .expect("configure isolated host root");
        // Production upload RPC holds this gate across authorization, quota, and staging.
        // 正式上传 RPC 在授权、额度校验和暂存期间持有此派发门。
        let session = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create browser");
        let gate = manager
            .dispatch_gate(&session.session_id)
            .expect("session gate");
        let dispatch = gate.lock().expect("hold upload dispatch gate");
        // Invalid Windows names must fail before creating a directory or an alternate stream.
        // 无效 Windows 名称必须在创建目录或附加数据流之前被拒绝。
        for name in [
            "file:stream.txt",
            "CON.txt",
            "COM¹.txt",
            "bad?.txt",
            "trailing.",
            "..",
        ] {
            assert_eq!(
                manager
                    .stage_upload(&session.session_id, name, b"x".to_vec())
                    .unwrap_err(),
                BROWSER_ERROR_INVALID_REQUEST
            );
        }
        assert!(!staging_root.join(&session.session_id).exists());
        assert!(
            manager
                .uploads(&session.session_id)
                .expect("list uploads")
                .is_empty()
        );
        for _ in 0..BROWSER_MAX_STAGED_UPLOADS_PER_SESSION {
            manager
                .stage_upload(&session.session_id, "fixture.txt", b"x".to_vec())
                .expect("stage within limit");
        }
        assert_eq!(
            manager
                .stage_upload(&session.session_id, "overflow.txt", b"x".to_vec())
                .unwrap_err(),
            BROWSER_ERROR_UPLOAD_LIMIT
        );
        assert_eq!(
            manager
                .uploads(&session.session_id)
                .expect("list uploads")
                .len(),
            BROWSER_MAX_STAGED_UPLOADS_PER_SESSION
        );
        drop(dispatch);
        manager
            .close_session(&session.session_id)
            .expect("close browser");
        assert!(!staging_root.join(&session.session_id).exists());
    }

    /// Verifies failed downloads vanish and completed files survive until history eviction.
    /// 验证失败下载被删除，已完成文件则保留至历史记录驱逐。
    #[test]
    fn download_files_follow_failure_close_and_history_eviction() {
        // The isolated runtime root prevents this test from touching a real browser profile.
        // 隔离的运行时根目录使本测试不会触及真实浏览器画像。
        let root = tempfile::tempdir().expect("isolated browser data root");
        let manager = BrowserManager::new();
        manager
            .configure_host_runtime(BrowserHostRuntimeConfig {
                executable: root.path().join("host"),
                entry: root.path().join("entry"),
                chromium_executable: root.path().join("chromium"),
                profiles_root: root.path().join("profiles"),
                staging_root: root.path().join("staging"),
                discovery_timeout: Duration::from_secs(1),
                request_timeout: Duration::from_secs(1),
            })
            .expect("configure isolated host root");
        // The registered page owns both download lifecycle events.
        // 已登记页面拥有两次下载的生命周期事件。
        let session = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create browser");
        manager
            .record_host_running(&session.session_id, 4242)
            .expect("record host");
        manager
            .adopt_page(&session.session_id, "target-1", "about:blank", None)
            .expect("adopt page");
        // The completed identity and path remain available for post-close and eviction checks.
        // 已完成下载的身份与路径保留供关闭后及驱逐后的核对。
        let mut completed = None;
        for (content, status) in [
            (b"failed".as_slice(), "failed"),
            (b"kept".as_slice(), "completed"),
        ] {
            // Every host download uses a distinct Core-checked UUID and storage path.
            // 每次宿主下载都使用独立且经 Core 校验的 UUID 与存储路径。
            let download_id = Uuid::new_v4().to_string();
            let started = BrowserHostEvent {
                event_sequence: String::from("1"),
                kind: String::from("downloadStarted"),
                payload: serde_json::json!({
                    "targetId": "target-1",
                    "downloadId": download_id,
                    "fileName": "fixture.txt"
                }),
            };
            manager
                .apply_host_event(&session.session_id, &started)
                .expect("record download start");
            let path = manager
                .download_storage_path(&session.session_id, &download_id)
                .expect("Core download path");
            std::fs::create_dir_all(path.parent().expect("download directory"))
                .expect("create download directory");
            std::fs::write(&path, content).expect("write host download");
            let finished = BrowserHostEvent {
                event_sequence: String::from("2"),
                kind: String::from("downloadFinished"),
                payload: serde_json::json!({
                    "targetId": "target-1",
                    "downloadId": download_id,
                    "status": status,
                    "byteLength": content.len()
                }),
            };
            manager
                .apply_host_event(&session.session_id, &finished)
                .expect("record download completion");
            if status == "completed" {
                completed = Some((download_id.clone(), path.clone()));
                assert!(path.is_file());
                assert_eq!(
                    manager
                        .download_content(&download_id, &session.owner_session_id)
                        .expect("completed download remains readable")
                        .1,
                    content
                );
            } else {
                assert!(!path.exists());
                assert_eq!(
                    manager
                        .download_content(&download_id, &session.owner_session_id)
                        .unwrap_err(),
                    BROWSER_ERROR_INVALID_REQUEST
                );
            }
        }
        // A closed history record still authorizes its completed download until it is evicted.
        // 已关闭的历史记录在被驱逐前仍授权读取已完成下载。
        let (completed_id, completed_path) = completed.expect("completed download identity");
        manager
            .close_session(&session.session_id)
            .expect("close downloaded browser");
        assert!(completed_path.is_file());
        assert_eq!(
            manager
                .download_content(&completed_id, &session.owner_session_id)
                .expect("closed history download remains readable")
                .1,
            b"kept"
        );
        // Retained history is bounded; the oldest control directory expires with its record.
        // 历史记录有上限；最旧记录的控制目录与记录一同到期。
        for _ in 1..BROWSER_MAX_RETAINED_CLOSED_SESSIONS {
            let later = manager
                .create_session(BrowserProfileMode::Ephemeral)
                .expect("create later browser");
            manager
                .close_session(&later.session_id)
                .expect("close later browser");
            assert!(completed_path.is_file());
        }
        // Holding the old record's read gate makes the final eviction wait after it removes the
        // metadata, so an already-started download read cannot lose its file mid-transfer.
        // 持有旧记录的读取门，使最终驱逐在删除元数据后等待，从而避免已开始的读取途中丢失文件。
        let old_gate = {
            let state = manager.inner.lock().expect("lock browser manager");
            Arc::clone(
                &state
                    .sessions
                    .get(&session.session_id)
                    .expect("old history remains")
                    .download_gate,
            )
        };
        let held_read = old_gate.lock().expect("hold old download read gate");
        let final_session = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create final browser");
        let closing_manager = manager.clone();
        let closing = std::thread::spawn(move || {
            closing_manager
                .close_session(&final_session.session_id)
                .expect("close final browser");
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        while manager.session_projection(&session.session_id).is_ok() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            manager.session_projection(&session.session_id).unwrap_err(),
            BROWSER_ERROR_UNKNOWN_SESSION
        );
        assert!(completed_path.is_file());
        drop(held_read);
        closing.join().expect("history eviction completes");
        assert!(!completed_path.exists());
        assert_eq!(
            manager
                .download_content(&completed_id, &session.owner_session_id)
                .unwrap_err(),
            BROWSER_ERROR_INVALID_REQUEST
        );
    }

    /// Verifies cancelling a page-bound stream ends its next poll without restoring a frozen queue.
    /// 验证取消已绑定页面的流后，下次轮询明确结束且不会恢复冻结的队列。
    #[test]
    fn cancelled_page_bound_stream_cannot_reappear_on_poll() {
        let manager = BrowserManager::new();
        let session = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create session");
        manager
            .record_host_running(&session.session_id, 4242)
            .expect("record host");
        let page = manager
            .adopt_page(&session.session_id, "target-a", "about:blank", None)
            .expect("adopt page");
        manager
            .bind_stream_page(&session.session_id, &page.page_id, "connection-stream")
            .expect("bind stream");
        let cursor = BrowserStreamCursor {
            page_generation: session.page_generation,
            frame_sequence: None,
        };
        manager
            .poll_bound_stream(&session.session_id, "connection-stream", cursor)
            .expect("poll live stream");
        manager
            .unsubscribe_observer(&session.session_id, "connection-stream")
            .expect("cancel stream");
        assert_eq!(
            manager
                .poll_bound_stream(&session.session_id, "connection-stream", cursor)
                .expect_err("cancelled stream must not restart"),
            BROWSER_ERROR_INVALID_REQUEST
        );
        assert!(
            !manager
                .observation_subscription_ids(&session.session_id)
                .expect("subscriptions")
                .contains(&String::from("connection-stream"))
        );
    }

    /// Verifies a slow observer's retained frames stay bounded and never starve the others.
    /// 验证缓慢观察端保留的帧保持有界，且绝不使其他观察端挨饿。
    #[test]
    fn a_slow_observer_stays_bounded_and_does_not_starve_others() {
        let manager = BrowserManager::new();
        let session = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create session");
        manager
            .resume_stream(&session.session_id, "slow", None)
            .expect("slow observer");
        manager
            .resume_stream(&session.session_id, "fast", None)
            .expect("fast observer");

        // The slow observer never confirms a frame while the fast one keeps consuming.
        // 缓慢观察端从不确认帧，而快速观察端持续消费。
        let total = BROWSER_MAX_QUEUED_FRAMES * 3;
        let mut fast_sequence = Some(0_u64);
        for index in 0..total {
            manager
                .publish_frame(
                    &session.session_id,
                    session.page_generation,
                    "image/png",
                    vec![index as u8],
                )
                .expect("publish");
            if let Some(sequence) = fast_sequence {
                let resumed = manager
                    .resume_stream(
                        &session.session_id,
                        "fast",
                        Some(BrowserStreamCursor {
                            page_generation: session.page_generation,
                            frame_sequence: Some(sequence),
                        }),
                    )
                    .expect("fast resume");
                assert_eq!(
                    resumed.mode,
                    BrowserStreamResumeMode::Incremental,
                    "the fast observer must keep resuming incrementally"
                );
                fast_sequence = resumed
                    .frames
                    .last()
                    .and_then(|frame| frame.sequence.parse::<u64>().ok())
                    .or(fast_sequence);
            }
        }

        // The stalled observer's memory is bounded, not proportional to what it missed.
        // 停滞观察端的内存是有界的，而不与其错过的帧数成正比。
        let slow_retained = manager
            .retained_frame_count(&session.session_id, "slow")
            .expect("slow count");
        assert!(
            slow_retained <= BROWSER_MAX_QUEUED_FRAMES,
            "a slow observer retained {slow_retained} frames, above the {BROWSER_MAX_QUEUED_FRAMES} bound"
        );
        // Its gap is reported rather than hidden, so the client knows it must resynchronise.
        // 其缺口被如实报告而不是被隐藏，因此客户端知道自己必须重新同步。
        let stalled = manager
            .resume_stream(
                &session.session_id,
                "slow",
                Some(BrowserStreamCursor {
                    page_generation: session.page_generation,
                    frame_sequence: Some(1),
                }),
            )
            .expect("slow resume");
        assert_eq!(stalled.mode, BrowserStreamResumeMode::Snapshot);
        assert!(
            stalled.dropped_frames > 0,
            "an evicted gap must be reported as dropped frames"
        );
    }

    /// Verifies a created session starts with no supervised host and no pages.
    /// 验证新建会话没有受监督宿主也没有页面。
    #[test]
    fn created_sessions_begin_without_a_host_or_pages() {
        let manager = BrowserManager::new();
        let session = manager
            .create_session(BrowserProfileMode::Persistent)
            .expect("create session");
        assert_eq!(session.host_state, BrowserHostSupervisionState::NotStarted);
        assert_eq!(session.host_generation, 0);
        assert_eq!(session.page_count, 0);
        assert!(!session.requires_explicit_reopen);
        assert!(
            manager
                .list_pages(&session.session_id)
                .expect("list pages")
                .is_empty()
        );
    }

    /// Verifies host page events register real targets as Core pages.
    /// 验证宿主页面事件把真实目标登记为 Core 页面。
    #[test]
    fn host_page_events_register_real_targets_as_pages() {
        let (manager, _alive) = manager_with_fake_host(Duration::from_millis(0));
        let session = manager
            .create_session(BrowserProfileMode::Persistent)
            .expect("create session");
        manager.start_host(&session.session_id).expect("start host");

        // Events arrive from the host exactly as the real host reports them.
        // 事件按真实宿主的报告方式到达。
        emit_fake_host_events(
            &manager,
            &session.session_id,
            vec![
                (
                    "pageCreated",
                    json!({ "targetId": "11", "url": "https://example.test/one", "openerTargetId": null }),
                ),
                (
                    "pageCreated",
                    json!({ "targetId": "12", "url": "https://example.test/one", "openerTargetId": "11" }),
                ),
            ],
        );

        let pages = manager.list_pages(&session.session_id).expect("list pages");
        assert_eq!(pages.len(), 2, "both targets must become Core pages");
        // Two pages showing the same URL must still be two distinct identities.
        // 显示同一地址的两个页面仍必须是两个不同标识。
        assert_ne!(pages[0].page_id, pages[1].page_id);
        let popup = pages
            .iter()
            .find(|page| page.target_id == "12")
            .expect("popup page");
        let opener = pages
            .iter()
            .find(|page| page.target_id == "11")
            .expect("opener page");
        assert_eq!(
            popup.opener_page_id.as_deref(),
            Some(opener.page_id.as_str()),
            "a page opened by another page must record its Core opener identity"
        );
    }

    /// Verifies a navigation event refreshes the URL without changing the page identity.
    /// 验证导航事件刷新地址但不改变页面标识。
    #[test]
    fn host_navigation_events_keep_the_page_identity() {
        let (manager, _alive) = manager_with_fake_host(Duration::from_millis(0));
        let session = manager
            .create_session(BrowserProfileMode::Persistent)
            .expect("create session");
        manager.start_host(&session.session_id).expect("start host");
        emit_fake_host_events(
            &manager,
            &session.session_id,
            vec![(
                "pageCreated",
                json!({ "targetId": "21", "url": "https://example.test/first" }),
            )],
        );
        let before = manager.list_pages(&session.session_id).expect("pages")[0].clone();
        emit_fake_host_events(
            &manager,
            &session.session_id,
            vec![(
                "pageNavigated",
                json!({ "targetId": "21", "url": "https://example.test/second" }),
            )],
        );
        let after = manager.list_pages(&session.session_id).expect("pages");
        assert_eq!(after.len(), 1, "navigation must not create a second page");
        assert_eq!(after[0].page_id, before.page_id);
        assert_eq!(after[0].url, "https://example.test/second");
    }

    /// Verifies unknown host event kinds are ignored rather than treated as failures.
    /// 验证未知宿主事件类型被忽略而不是被当作失败。
    #[test]
    fn unknown_host_events_are_ignored() {
        let (manager, _alive) = manager_with_fake_host(Duration::from_millis(0));
        let session = manager
            .create_session(BrowserProfileMode::Persistent)
            .expect("create session");
        manager.start_host(&session.session_id).expect("start host");
        emit_fake_host_events(
            &manager,
            &session.session_id,
            vec![("futureEventKind", json!({ "anything": true }))],
        );
        assert!(
            manager
                .list_pages(&session.session_id)
                .expect("pages")
                .is_empty()
        );
    }

    /// Writes host events into one session's control channel from the test side.
    /// 从测试侧把宿主事件写入某个会话的控制通道。
    fn emit_fake_host_events(
        manager: &BrowserManager,
        session_id: &str,
        events: Vec<(&str, serde_json::Value)>,
    ) {
        let ownership = manager.owned_host(session_id).expect("owned host");
        let mut ownership = ownership.lock().expect("lock host");
        for (kind, payload) in events {
            ownership
                .connection
                .push_test_event(kind, payload)
                .expect("push host event");
        }
    }

    /// Verifies a confirmed host owns a generation and its pages are registered under it.
    /// 验证已确认宿主拥有代际，其页面在该代际下登记。
    #[test]
    fn a_confirmed_host_owns_the_generation_and_its_pages() {
        let manager = BrowserManager::new();
        let session = manager
            .create_session(BrowserProfileMode::Persistent)
            .expect("create session");

        let running = manager
            .record_host_running(&session.session_id, 4242)
            .expect("record running");
        assert_eq!(running.host_state, BrowserHostSupervisionState::Running);
        assert_eq!(running.host_generation, 1);

        let first = manager
            .adopt_page(
                &session.session_id,
                "target-a",
                "https://example.test/one",
                None,
            )
            .expect("adopt first page");
        // A page opened by an existing page records its opener.
        // 由既有页面打开的新页面会记录其打开者。
        let opened = manager
            .adopt_page(
                &session.session_id,
                "target-b",
                "https://example.test/two",
                Some(&first.page_id),
            )
            .expect("adopt opened page");
        assert_eq!(
            opened.opener_page_id.as_deref(),
            Some(first.page_id.as_str())
        );

        assert_eq!(
            manager
                .list_pages(&session.session_id)
                .expect("list pages")
                .len(),
            2
        );
        let projection = manager
            .session_projection(&session.session_id)
            .expect("projection");
        assert_eq!(projection.page_count, 2);
    }

    /// Verifies a navigation keeps the page identity while its URL changes.
    /// 验证导航保留页面标识而地址变化。
    #[test]
    fn navigation_keeps_the_core_page_identity() {
        let manager = BrowserManager::new();
        let session = manager
            .create_session(BrowserProfileMode::Persistent)
            .expect("create session");
        manager
            .record_host_running(&session.session_id, 1)
            .expect("record running");
        let page = manager
            .adopt_page(
                &session.session_id,
                "target-a",
                "https://example.test/first",
                None,
            )
            .expect("adopt page");
        let again = manager
            .adopt_page(
                &session.session_id,
                "target-a",
                "https://example.test/second",
                None,
            )
            .expect("re-adopt page");
        assert_eq!(again.page_id, page.page_id);
        assert_eq!(again.url, "https://example.test/second");
        assert_eq!(
            manager
                .list_pages(&session.session_id)
                .expect("list pages")
                .len(),
            1
        );
    }

    /// Verifies two sessions never share page identity or host state.
    /// 验证两个会话绝不共享页面标识或宿主状态。
    #[test]
    fn two_sessions_do_not_share_pages_or_host_state() {
        let manager = BrowserManager::new();
        let first = manager
            .create_session(BrowserProfileMode::Persistent)
            .expect("create first session");
        let second = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create second session");
        manager
            .record_host_running(&first.session_id, 11)
            .expect("first host");
        manager
            .record_host_running(&second.session_id, 22)
            .expect("second host");
        manager
            .adopt_page(&first.session_id, "target-a", "https://example.test/", None)
            .expect("first page");

        assert_eq!(
            manager
                .list_pages(&first.session_id)
                .expect("first pages")
                .len(),
            1
        );
        assert!(
            manager
                .list_pages(&second.session_id)
                .expect("second pages")
                .is_empty(),
            "one session must never expose another session's pages"
        );
    }

    /// Verifies a host crash invalidates page identity and demands an explicit reopen.
    /// 验证宿主崩溃使页面标识失效并要求明确重开。
    #[test]
    fn a_crashed_host_invalidates_pages_and_requires_an_explicit_reopen() {
        let manager = BrowserManager::new();
        let session = manager
            .create_session(BrowserProfileMode::Persistent)
            .expect("create session");
        manager
            .record_host_running(&session.session_id, 77)
            .expect("record running");
        manager
            .adopt_page(
                &session.session_id,
                "target-a",
                "https://example.test/",
                None,
            )
            .expect("adopt page");

        let crashed = manager
            .record_host_crash(&session.session_id, Some(9), "host exited unexpectedly")
            .expect("record crash");
        assert_eq!(crashed.status, BrowserSessionStatus::Crashed);
        assert_eq!(crashed.host_state, BrowserHostSupervisionState::Crashed);
        assert_eq!(crashed.page_count, 0);
        assert!(crashed.requires_explicit_reopen);
        assert!(
            manager
                .list_pages(&session.session_id)
                .expect("pages")
                .is_empty(),
            "a crashed generation must not keep its page identities"
        );
        let summary = manager
            .host_supervision_summary(&session.session_id)
            .expect("summary");
        assert_eq!(summary["state"], "crashed");
        assert_eq!(summary["crash"]["exitCode"], 9);
        assert_eq!(summary["requiresExplicitReopen"], true);
    }

    /// Verifies an explicit close ends the context without looking like a crash.
    /// 验证显式结束上下文且不表现为崩溃。
    #[test]
    fn an_explicit_close_is_not_a_crash() {
        let manager = BrowserManager::new();
        let session = manager
            .create_session(BrowserProfileMode::Persistent)
            .expect("create session");
        manager
            .record_host_running(&session.session_id, 5)
            .expect("record running");
        manager
            .adopt_page(
                &session.session_id,
                "target-a",
                "https://example.test/",
                None,
            )
            .expect("adopt page");
        let closed = manager
            .close_session(&session.session_id)
            .expect("close session");
        assert_eq!(closed.status, BrowserSessionStatus::Closed);
        assert_eq!(closed.host_state, BrowserHostSupervisionState::Closed);
        assert!(!closed.requires_explicit_reopen);
        let summary = manager
            .host_supervision_summary(&session.session_id)
            .expect("summary");
        assert_eq!(summary["state"], "closed");
        assert_eq!(summary["crash"], serde_json::Value::Null);
    }

    /// Verifies supervision operations reject an unknown session instead of inventing one.
    /// 验证监督操作拒绝未知会话，而不是凭空创建一个。
    #[test]
    fn supervision_operations_reject_an_unknown_session() {
        let manager = BrowserManager::new();
        assert_eq!(
            manager.record_host_running("missing", 1).unwrap_err(),
            BROWSER_ERROR_UNKNOWN_SESSION
        );
        assert_eq!(
            manager
                .adopt_page("missing", "target-a", "https://example.test/", None)
                .unwrap_err(),
            BROWSER_ERROR_UNKNOWN_SESSION
        );
        assert_eq!(
            manager.list_pages("missing").unwrap_err(),
            BROWSER_ERROR_UNKNOWN_SESSION
        );
        assert_eq!(
            manager.host_supervision_summary("missing").unwrap_err(),
            BROWSER_ERROR_UNKNOWN_SESSION
        );
        // An empty target identity is a request error, not a new page.
        // 空目标标识是请求错误，而不是新页面。
        let session = manager
            .create_session(BrowserProfileMode::Persistent)
            .expect("create session");
        manager
            .record_host_running(&session.session_id, 1)
            .expect("record running");
        assert_eq!(
            manager
                .adopt_page(&session.session_id, "  ", "https://example.test/", None)
                .unwrap_err(),
            BROWSER_ERROR_INVALID_REQUEST
        );
    }

    /// Builds one shared service state carrying an empty browser manager.
    /// 构建一个携带空浏览器管理器的共享服务状态。
    fn browser_test_state() -> SharedServiceBridgeState {
        Arc::new(Mutex::new(test_service_state_with_parent_session(
            SessionId::new_v4(),
        )))
    }

    /// Verifies session creation assigns a running ephemeral session.
    /// 验证会话创建分配一个运行中的临时会话。
    #[test]
    fn browser_session_create_assigns_running_session() {
        let manager = BrowserManager::new();
        let session = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create session");
        assert_eq!(session.status, BrowserSessionStatus::Running);
        assert_eq!(session.page_generation, 1);
        assert_eq!(session.control_epoch, 1);
    }

    /// Verifies the lease lifecycle rejects a second holder.
    /// 验证租约生命周期拒绝第二个持有者。
    #[test]
    fn browser_lease_rejects_second_holder() {
        let manager = BrowserManager::new();
        let session = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create session");
        let (_, secret) = manager
            .acquire_lease(&session.session_id, "user", None)
            .expect("acquire lease");
        assert!(secret.trim().len() > 8);
        assert!(
            manager
                .acquire_lease(&session.session_id, "other", None)
                .is_err()
        );
        manager
            .release_lease(&session.session_id, &secret)
            .expect("release lease");
        assert!(
            manager
                .acquire_lease(&session.session_id, "other", None)
                .is_ok()
        );
    }

    /// Verifies stale page generations are rejected for input dispatch.
    /// 验证输入派发拒绝过期页面代次。
    #[test]
    fn browser_input_rejects_stale_generation() {
        let manager = BrowserManager::new();
        let session = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create session");
        let (_, secret) = manager
            .acquire_lease(&session.session_id, "user", None)
            .expect("acquire lease");
        let event = BrowserInputEvent {
            sequence: String::from("1"),
            kind: String::from("pointer"),
            payload: serde_json::json!({}),
        };
        assert!(
            manager
                .dispatch_input(
                    &session.session_id,
                    &secret,
                    session.page_generation + 1,
                    event
                )
                .is_err()
        );
    }

    /// Verifies automation attach requires the lease secret.
    /// 验证自动化挂接要求租约凭据。
    #[test]
    fn browser_automation_requires_lease_secret() {
        let manager = BrowserManager::new();
        let session = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create session");
        assert!(
            manager
                .attach_automation(&session.session_id, "wrong-secret", "automation-1")
                .is_err()
        );
    }

    /// Verifies observation subscriptions are isolated per subscriber.
    /// 验证观察订阅按订阅者隔离。
    #[test]
    fn browser_observation_isolated_per_subscriber() {
        let manager = BrowserManager::new();
        let session = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create session");
        let first = manager
            .subscribe_observer(&session.session_id)
            .expect("subscribe first");
        let second = manager
            .subscribe_observer(&session.session_id)
            .expect("subscribe second");
        assert_ne!(first, second);
        manager
            .publish_frame(
                &session.session_id,
                session.page_generation,
                "image/jpeg",
                vec![1, 2, 3],
            )
            .expect("publish frame");
        manager
            .unsubscribe_observer(&session.session_id, &first)
            .expect("unsubscribe first");
    }

    /// Verifies monotonic browser counters use decimal text across the JS boundary.
    /// 验证单调浏览器计数器跨 JS 边界使用十进制文本。
    #[test]
    fn browser_counters_serialize_as_decimal_text() {
        let manager = BrowserManager::new();
        let session = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create session");
        let value = serde_json::to_value(&session).expect("serialize projection");
        assert_eq!(value.get("pageGeneration"), Some(&serde_json::json!("1")));
        assert_eq!(value.get("controlEpoch"), Some(&serde_json::json!("1")));
        let decoded: BrowserSessionProjection =
            serde_json::from_value(value).expect("decode projection");
        assert_eq!(decoded, session);
    }

    /// Verifies detailed status reports draining admission before bootstrap phases.
    /// 验证详细状态在引导阶段之前报告排空准入。
    #[test]
    fn browser_status_snapshot_reports_stopping_during_drain() {
        let state = browser_test_state();
        let stopping = crate::service_status_snapshot_from_state(&state);
        assert_eq!(
            stopping.lifecycle,
            vulcan_service_api::ServiceLifecycle::Ready
        );
        let _handles = state
            .lock()
            .expect("lock isolated service state")
            .session_runtime_registry
            .begin_shutdown()
            .expect("begin shutdown");
        {
            state
                .lock()
                .expect("lock isolated service state")
                .service_lifecycle_state
                .begin_shutdown_drain();
        }
        let snapshot = crate::service_status_snapshot_from_state(&state);
        assert_eq!(
            snapshot.lifecycle,
            vulcan_service_api::ServiceLifecycle::Stopping
        );
        assert_eq!(snapshot.active_work.active_stream_tasks, 0);
        assert_eq!(snapshot.active_work.vce_jobs, 0);
        assert_eq!(snapshot.active_work.running_terminals, 0);
        crate::service_resume_after_failed_app_shutdown(&state).expect("resume shutdown");
        let recovered = crate::service_status_snapshot_from_state(&state);
        assert_eq!(
            recovered.lifecycle,
            vulcan_service_api::ServiceLifecycle::Ready
        );
        assert!(recovered.last_failure.is_none());
    }

    /// Verifies state-backed session creation flows through shared service state.
    /// 验证经共享服务状态的会话创建流程。
    #[tokio::test]
    async fn browser_state_backed_session_create_lists() {
        // The state-backed path must own a background host, so the shared manager is replaced by
        // one wired to a deterministic in-process host instead of a desktop runtime.
        // 经状态的路径必须拥有后台宿主，因此共享管理器被替换为连接进程内确定性宿主的管理器，
        // 而不是连接桌面运行时。
        let state = browser_test_state();
        let alive = Arc::new(AtomicBool::new(true));
        let host = FakeBrowserHost::start(Duration::from_millis(0), Arc::clone(&alive));
        let profiles_root =
            std::env::temp_dir().join(format!("vulcan-w06-state-{}", Uuid::new_v4()));
        {
            let mut guard = state.lock().expect("lock test state");
            guard.browser_manager =
                BrowserManager::with_host_spawner(Arc::new(FakeBrowserHostSpawner {
                    address: host.address,
                    alive,
                    poll_error: Arc::new(AtomicBool::new(false)),
                }));
            guard
                .browser_manager
                .configure_host_runtime(BrowserHostRuntimeConfig {
                    executable: std::path::PathBuf::from("fake-electron.exe"),
                    entry: std::path::PathBuf::from("fake-host.js"),
                    chromium_executable: std::path::PathBuf::from("fake-chromium.exe"),
                    profiles_root: profiles_root.clone(),
                    staging_root: profiles_root.join("staging"),
                    discovery_timeout: Duration::from_secs(10),
                    request_timeout: Duration::from_secs(5),
                })
                .expect("configure host runtime");
        }
        let created = service_browser_session_create(
            state.clone(),
            BrowserSessionCreateRequest { profile_mode: None },
            String::from("conversation-a"),
        )
        .await
        .expect("create through state");
        // A session handed to a client always names the host that owns it.
        // 交付给客户端的会话总是标明拥有它的宿主。
        assert_eq!(created.host_state, BrowserHostSupervisionState::Running);
        assert_eq!(created.host_generation, 1);
        let listed = service_browser_session_list(state, String::from("conversation-a"))
            .await
            .expect("list through state");
        assert!(
            listed
                .iter()
                .any(|session| session.session_id == created.session_id)
        );
    }

    /// Verifies the state-backed path refuses to hand out a session with no background browser.
    /// 验证经状态的路径拒绝交付没有后台浏览器的会话。
    #[tokio::test]
    async fn browser_state_backed_session_create_requires_a_host_runtime() {
        let state = browser_test_state();
        let outcome = service_browser_session_create(
            state.clone(),
            BrowserSessionCreateRequest { profile_mode: None },
            String::from("conversation-a"),
        )
        .await;
        assert_eq!(outcome.unwrap_err(), BROWSER_ERROR_CAPABILITY_UNSUPPORTED);
        // The manager retains closed sessions as history, so the real property is that the refused
        // creation left no live session a client could try to use.
        // 管理器会把已关闭会话作为历史保留，因此真正的性质是：被拒绝的创建没有留下任何客户端可能
        // 试图使用的活动会话。
        assert!(
            service_browser_session_list(state, String::from("conversation-a"))
                .await
                .expect("list through state")
                .iter()
                .all(|session| session.status == BrowserSessionStatus::Closed),
            "a refused creation must not leave a live session"
        );
    }

    /// Verifies one conversation cannot enumerate or address another conversation's Core browser.
    /// 验证一个会话不能枚举或访问另一个会话的 Core 浏览器。
    #[test]
    fn browser_manager_rejects_cross_conversation_access() {
        // manager owns both browsers, making ownership the only distinguishing boundary.
        // manager 持有两个浏览器，使归属成为唯一区分边界。
        let manager = BrowserManager::new();
        // first belongs only to conversation A.
        // first 只归属会话 A。
        let first = manager
            .create_owned_session("conversation-a", BrowserProfileMode::Ephemeral)
            .expect("create first browser");
        // second belongs only to conversation B.
        // second 只归属会话 B。
        let second = manager
            .create_owned_session("conversation-b", BrowserProfileMode::Ephemeral)
            .expect("create second browser");
        assert_eq!(
            manager
                .list_owned_sessions("conversation-a")
                .expect("list A"),
            vec![first.clone()]
        );
        assert_eq!(
            manager
                .list_owned_sessions("conversation-b")
                .expect("list B"),
            vec![second]
        );
        assert!(
            manager
                .require_owner(&first.session_id, "conversation-a")
                .is_ok()
        );
        assert_eq!(
            manager
                .require_owner(&first.session_id, "conversation-b")
                .unwrap_err(),
            BROWSER_ERROR_UNKNOWN_SESSION
        );
    }
}
