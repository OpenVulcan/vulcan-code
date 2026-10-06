//! Core-side browser host control channel, request correlation, and page identity.
//! Core 侧浏览器宿主控制通道、请求关联与页面身份。
//!
//! One Core browser session owns exactly one host process. This module speaks the documented
//! length-prefixed control protocol to that host, correlates responses by request id, tracks the
//! host event sequence, and converts a request timeout into an explicit unknown outcome instead of
//! retrying an action that may already have been applied.
//! 一个 Core 浏览器会话恰好拥有一个宿主进程。本模块与该宿主通信，使用文档规定的长度前缀控制协议：
//! 按请求标识关联响应、跟踪宿主事件序号，并把请求超时转换为明确的未知结果，而不是重试可能已经生效的动作。

use std::{
    collections::{BTreeMap, VecDeque},
    io::{Read, Write},
    net::{Shutdown, TcpStream},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::Digest;

/// Control protocol version understood by this Core build.
/// 本 Core 构建理解的控制协议版本。
pub const BROWSER_HOST_PROTOCOL_VERSION: u32 = 2;

/// Maximum bytes accepted for one control frame.
/// 单个控制帧接受的最大字节数。
pub const BROWSER_HOST_MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;

/// Four-byte big-endian length prefix size.
/// 4 字节大端长度前缀的字节数。
pub const BROWSER_HOST_LENGTH_PREFIX_BYTES: usize = 4;

/// Discovery file name written by one browser host inside its own user-data directory.
/// 单个浏览器宿主在其自有用户数据目录内写入的发现文件名。
pub const BROWSER_HOST_CONTROL_DISCOVERY_FILENAME: &str = "browser-host-control.json";

/// Schema version of the control discovery document.
/// 控制发现文档的架构版本。
pub const BROWSER_HOST_CONTROL_DISCOVERY_SCHEMA_VERSION: u32 = 1;

/// First message Core sends on the control connection to prove channel ownership.
/// Core 在控制连接上发送的首条消息，用于证明通道归属。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserHostCoreHello {
    /// Frame discriminator.
    /// 帧判别字段。
    pub kind: String,
    /// Control protocol version.
    /// 控制协议版本。
    pub protocol_version: u32,
    /// One-time secret Core passed to this host at spawn time.
    /// Core 在启动本宿主时传入的一次性密钥。
    pub connection_secret: String,
    /// Session identity assigned by Core.
    /// 由 Core 分配的会话标识。
    pub session_id: String,
    /// Host generation assigned by Core.
    /// 由 Core 分配的宿主代际。
    pub host_generation: String,
}

/// First message sent by the host so Core can verify its own session and generation.
/// 宿主发送的首条消息，使 Core 能核对自身分配的会话与代际。
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserHostHandshake {
    /// Control protocol version.
    /// 控制协议版本。
    pub protocol_version: u32,
    /// Session identity assigned by Core.
    /// 由 Core 分配的会话标识。
    pub session_id: String,
    /// Host generation assigned by Core.
    /// 由 Core 分配的宿主代际。
    pub host_generation: String,
    /// Operating-system process id of this host.
    /// 本宿主的操作系统进程标识。
    pub pid: u32,
    /// Runtime version string reported for diagnostics.
    /// 用于诊断的运行时版本文本。
    #[serde(default)]
    pub runtime_version: String,
    /// Capability names this host actually provides.
    /// 本宿主实际提供的能力名称。
    #[serde(default)]
    pub capabilities: Vec<String>,
}

/// One response sent from the host to Core.
/// 宿主发送给 Core 的一条响应。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserHostResponse {
    /// Correlation identity copied from the request.
    /// 从请求复制的关联标识。
    pub request_id: String,
    /// Whether the operation succeeded.
    /// 操作是否成功。
    pub ok: bool,
    /// Typed result when the operation succeeded.
    /// 操作成功时的类型化结果。
    #[serde(default)]
    pub result: Option<Value>,
    /// Stable error code and message when the operation failed.
    /// 操作失败时的稳定错误码与消息。
    #[serde(default)]
    pub error: Option<BrowserHostResponseError>,
}

/// Stable error reported by a browser host.
/// 浏览器宿主报告的稳定错误。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserHostResponseError {
    /// Stable machine-readable error code.
    /// 稳定的机器可读错误码。
    pub code: String,
    /// Human-readable diagnostic.
    /// 人类可读诊断。
    #[serde(default)]
    pub message: String,
}

/// One unsolicited event emitted by the host.
/// 宿主发出的一条非请求事件。
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserHostEvent {
    /// Monotonic event sequence within this host lifetime.
    /// 本宿主生命周期内单调的事件序号。
    pub event_sequence: String,
    /// Event kind understood by Core.
    /// Core 理解的事件类型。
    pub kind: String,
    /// Typed event payload.
    /// 类型化事件载荷。
    #[serde(default)]
    pub payload: Value,
}

/// Outcome of one Core request against a browser host.
/// Core 对浏览器宿主发出的一次请求的结果。
#[derive(Debug, Clone, PartialEq)]
pub enum BrowserHostRequestOutcome {
    /// The host answered successfully.
    /// 宿主成功应答。
    Completed(Value),
    /// The host answered with a stable typed failure.
    /// 宿主以稳定类型化失败应答。
    Failed {
        /// Stable machine-readable error code.
        /// 稳定的机器可读错误码。
        code: String,
        /// Human-readable diagnostic.
        /// 人类可读诊断。
        message: String,
    },
    /// The result is unknown because the request timed out or the channel failed mid-flight.
    /// 由于请求超时或通道中途失败，结果未知。
    ///
    /// The action may already have been applied, so callers must not blind-retry it.
    /// 该动作可能已经生效，因此调用方不得盲目重试。
    Unknown {
        /// Why the outcome could not be determined.
        /// 无法确定结果的原因。
        reason: String,
    },
}

impl BrowserHostRequestOutcome {
    /// Returns whether this outcome leaves the real browser state unknown.
    /// 返回本结果是否使真实浏览器状态处于未知。
    pub fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown { .. })
    }
}

/// Encodes one JSON control frame with its big-endian length prefix.
/// 使用大端长度前缀编码一个 JSON 控制帧。
pub fn encode_host_frame(value: &impl Serialize) -> Result<Vec<u8>, String> {
    let body = serde_json::to_vec(value)
        .map_err(|error| format!("failed to encode host frame: {error}"))?;
    if body.len() > BROWSER_HOST_MAX_FRAME_BYTES {
        return Err(format!(
            "host control frame of {} bytes exceeds the {BROWSER_HOST_MAX_FRAME_BYTES}-byte limit",
            body.len()
        ));
    }
    let mut frame = Vec::with_capacity(BROWSER_HOST_LENGTH_PREFIX_BYTES + body.len());
    frame.extend_from_slice(&(body.len() as u32).to_be_bytes());
    frame.extend_from_slice(&body);
    Ok(frame)
}

/// Incremental decoder that yields one JSON value per complete control frame.
/// 增量解码器，每完成一个控制帧产出一个 JSON 值。
#[derive(Default)]
pub struct BrowserHostFrameDecoder {
    /// Bytes that do not yet form one complete frame.
    /// 尚未构成完整帧的字节。
    pending: Vec<u8>,
}

impl BrowserHostFrameDecoder {
    /// Creates one empty decoder.
    /// 创建一个空解码器。
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends received bytes and returns every frame completed by this chunk.
    /// 追加接收到的字节，并返回本块完成的所有帧。
    ///
    /// # Parameters
    /// 参数
    /// `chunk` is one arbitrary byte slice as delivered by the transport.
    /// `chunk` 是传输层交付的任意字节片段。
    ///
    /// # Returns
    /// 返回值
    /// Fully decoded frames in arrival order, or a diagnostic for an oversized length prefix.
    /// 按到达顺序返回完整解码的帧；长度前缀超限时返回诊断。
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<Value>, String> {
        self.pending.extend_from_slice(chunk);
        let mut values = Vec::new();
        loop {
            if self.pending.len() < BROWSER_HOST_LENGTH_PREFIX_BYTES {
                return Ok(values);
            }
            let length = u32::from_be_bytes([
                self.pending[0],
                self.pending[1],
                self.pending[2],
                self.pending[3],
            ]) as usize;
            if length > BROWSER_HOST_MAX_FRAME_BYTES {
                return Err(format!(
                    "host control frame of {length} bytes exceeds the {BROWSER_HOST_MAX_FRAME_BYTES}-byte limit"
                ));
            }
            let end = BROWSER_HOST_LENGTH_PREFIX_BYTES + length;
            if self.pending.len() < end {
                return Ok(values);
            }
            let body = self.pending[BROWSER_HOST_LENGTH_PREFIX_BYTES..end].to_vec();
            self.pending.drain(..end);
            let value: Value = serde_json::from_slice(&body)
                .map_err(|error| format!("host control frame is not valid JSON: {error}"))?;
            values.push(value);
        }
    }
}

/// One authenticated control channel to a single browser host process.
/// 指向单个浏览器宿主进程的一条已鉴权控制通道。
pub struct BrowserHostConnection {
    /// Complete decoded messages awaiting their ordered consumer.
    /// 等待按序消费的完整已解码消息。
    pending_frames: VecDeque<Value>,
    /// Blocking stream carrying length-prefixed frames.
    /// 承载长度前缀帧的阻塞流。
    stream: TcpStream,
    /// Incremental decoder for inbound frames.
    /// 入站帧的增量解码器。
    decoder: BrowserHostFrameDecoder,
    /// Verified host handshake for this channel.
    /// 本通道已核验的宿主握手。
    handshake: BrowserHostHandshake,
    /// Highest event sequence consumed from this host.
    /// 从本宿主已消费的最大事件序号。
    last_event_sequence: u64,
    /// Event frames received but not yet drained by the caller.
    /// 已接收但尚未被调用方取走的事件帧。
    pending_events: Vec<BrowserHostEvent>,
    /// Next request correlation index.
    /// 下一个请求关联序号。
    next_request_index: u64,
    /// Session identity this channel must present.
    /// 本通道必须出示的会话标识。
    session_id: String,
    /// Host generation this channel must present.
    /// 本通道必须出示的宿主代际。
    host_generation: String,
}

impl BrowserHostConnection {
    /// Opens one control channel and completes the authenticated handshake.
    /// 打开一条控制通道并完成已鉴权握手。
    ///
    /// # Parameters
    /// 参数
    /// `address` is the loopback control endpoint, `hello` proves channel ownership, and
    /// `handshake_timeout` bounds the whole open-and-handshake sequence.
    /// `address` 是回环控制端点，`hello` 证明通道归属，`handshake_timeout` 界定整个打开与握手过程。
    ///
    /// # Returns
    /// 返回值
    /// The connected channel, or a diagnostic when the endpoint, protocol, session, or generation
    /// does not match what Core allocated.
    /// 返回已连接通道；端点、协议、会话或代际与 Core 分配不符时返回诊断。
    pub fn connect(
        address: &str,
        hello: BrowserHostCoreHello,
        handshake_timeout: Duration,
    ) -> Result<Self, String> {
        let stream = TcpStream::connect(address).map_err(|error| {
            format!("failed to connect to browser host control endpoint {address}: {error}")
        })?;
        stream
            .set_read_timeout(Some(handshake_timeout))
            .map_err(|error| format!("failed to bound the browser host handshake: {error}"))?;
        stream
            .set_write_timeout(Some(handshake_timeout))
            .map_err(|error| format!("failed to bound the browser host handshake: {error}"))?;
        let mut connection = Self {
            pending_frames: VecDeque::new(),
            stream,
            decoder: BrowserHostFrameDecoder::new(),
            handshake: BrowserHostHandshake {
                protocol_version: 0,
                session_id: String::new(),
                host_generation: String::new(),
                pid: 0,
                runtime_version: String::new(),
                capabilities: Vec::new(),
            },
            last_event_sequence: 0,
            pending_events: Vec::new(),
            next_request_index: 0,
            session_id: hello.session_id.clone(),
            host_generation: hello.host_generation.clone(),
        };
        connection.write_frame(&hello)?;
        let handshake_value = connection.read_frame_value()?;
        let handshake: BrowserHostHandshake = serde_json::from_value(handshake_value)
            .map_err(|error| format!("browser host handshake is unreadable: {error}"))?;
        if handshake.protocol_version != BROWSER_HOST_PROTOCOL_VERSION {
            return Err(format!(
                "browser host protocol version {} is not supported; expected {BROWSER_HOST_PROTOCOL_VERSION}",
                handshake.protocol_version
            ));
        }
        if handshake.session_id != hello.session_id
            || handshake.host_generation != hello.host_generation
        {
            // A channel that answers for another session or generation must never be adopted.
            // 为其他会话或代际应答的通道绝不能被采用。
            return Err(format!(
                "browser host answered for session `{}` generation `{}` instead of session `{}` generation `{}`",
                handshake.session_id,
                handshake.host_generation,
                hello.session_id,
                hello.host_generation
            ));
        }
        connection.handshake = handshake;
        Ok(connection)
    }

    /// Returns the verified handshake for this channel.
    /// 返回本通道已核验的握手。
    pub fn handshake(&self) -> &BrowserHostHandshake {
        &self.handshake
    }

    /// Returns the highest event sequence consumed from this host.
    /// 返回从本宿主已消费的最大事件序号。
    pub fn last_event_sequence(&self) -> u64 {
        self.last_event_sequence
    }

    /// Sends one request and correlates its response.
    /// 发送一条请求并关联其响应。
    ///
    /// # Parameters
    /// 参数
    /// `operation` is the host operation name, `params` is its parameter object, and `timeout`
    /// bounds the wait for the matching response.
    /// `operation` 是宿主操作名，`params` 是其参数对象，`timeout` 界定对匹配响应的等待。
    ///
    /// # Returns
    /// 返回值
    /// A completed, failed, or explicitly unknown outcome. A timeout never becomes a retry.
    /// 返回完成、失败或明确未知的结果。超时绝不转为重试。
    pub fn request(
        &mut self,
        operation: &str,
        params: Value,
        timeout: Duration,
    ) -> BrowserHostRequestOutcome {
        // Events must not restart the deadline of an in-flight operation.
        // 事件不得重置在途操作的截止时间。
        let deadline = Instant::now() + timeout;
        self.next_request_index += 1;
        let request_id = format!("core-{}", self.next_request_index);
        let request = json!({
            "requestId": request_id,
            "sessionId": self.session_id,
            "hostGeneration": self.host_generation,
            "operation": operation,
            "params": params,
        });
        if let Err(error) = self.write_frame(&request) {
            return BrowserHostRequestOutcome::Unknown { reason: error };
        }
        self.stream
            .set_read_timeout(Some(timeout))
            .unwrap_or_default();
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return BrowserHostRequestOutcome::Unknown {
                    reason: format!("{operation} exceeded its response deadline"),
                };
            }
            if let Err(error) = self.stream.set_read_timeout(Some(remaining)) {
                return BrowserHostRequestOutcome::Unknown {
                    reason: error.to_string(),
                };
            }
            let value = match self.read_frame_value() {
                Ok(value) => value,
                Err(error) => {
                    return BrowserHostRequestOutcome::Unknown {
                        reason: format!("{operation} produced no readable response: {error}"),
                    };
                }
            };
            // Events may interleave with the response, so they are buffered rather than dropped.
            // 事件可能与响应交错到达，因此缓冲而不是丢弃。
            if let Ok(event) = serde_json::from_value::<BrowserHostEvent>(value.clone())
                && !value.get("requestId").is_some()
            {
                if let Err(error) = self.accept_event(event) {
                    return BrowserHostRequestOutcome::Unknown { reason: error };
                }
                continue;
            }
            let response: BrowserHostResponse = match serde_json::from_value(value) {
                Ok(response) => response,
                Err(error) => {
                    return BrowserHostRequestOutcome::Unknown {
                        reason: format!("{operation} produced an unreadable response: {error}"),
                    };
                }
            };
            if response.request_id != request_id {
                // A response for a different request must not be applied to this one.
                // 属于其他请求的响应绝不能被应用到本请求。
                continue;
            }
            return if response.ok {
                BrowserHostRequestOutcome::Completed(response.result.unwrap_or(Value::Null))
            } else {
                let error = response.error.unwrap_or(BrowserHostResponseError {
                    code: String::from("unknown_host_error"),
                    message: String::new(),
                });
                BrowserHostRequestOutcome::Failed {
                    code: error.code,
                    message: error.message,
                }
            };
        }
    }

    /// Drains every event received so far, validating monotonic event sequences.
    /// 取走目前已接收的全部事件，并校验事件序号单调。
    ///
    /// # Returns
    /// 返回值
    /// Events in sequence order, or a diagnostic when the host repeated or reordered a sequence.
    /// 按序号顺序返回事件；宿主重复或乱序时返回诊断。
    pub fn drain_events(&mut self) -> Result<Vec<BrowserHostEvent>, String> {
        // A prior read may have decoded several complete events together with its response.
        // 先前读取可能将多个完整事件和响应一起解码，必须保留并处理这些事件。
        while let Some(value) = self.pending_frames.pop_front() {
            if let Ok(event) = serde_json::from_value::<BrowserHostEvent>(value) {
                self.accept_event(event)?;
            }
        }
        self.stream
            .set_read_timeout(Some(Duration::from_millis(1)))
            .map_err(|error| format!("failed to bound browser host event read: {error}"))?;
        // A short non-blocking-style read collects whatever the host already pushed.
        // 短暂的类非阻塞读取收集宿主已推送的内容。
        let mut buffer = [0_u8; 8192];
        loop {
            match self.stream.read(&mut buffer) {
                Ok(0) => {
                    return Err(String::from("browser host closed the control channel"));
                }
                Ok(read) => {
                    for value in self.decoder.push(&buffer[..read])? {
                        if let Ok(event) = serde_json::from_value::<BrowserHostEvent>(value) {
                            self.accept_event(event)?;
                        }
                    }
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    break;
                }
                Err(error) => {
                    return Err(format!("browser host event read failed: {error}"));
                }
            }
        }
        Ok(std::mem::take(&mut self.pending_events))
    }

    /// Records one event after validating its monotonic sequence.
    /// 校验事件序号单调后记录一个事件。
    fn accept_event(&mut self, event: BrowserHostEvent) -> Result<(), String> {
        let sequence = event.event_sequence.parse::<u64>().map_err(|_| {
            format!(
                "browser host event sequence `{}` is not a decimal number",
                event.event_sequence
            )
        })?;
        if sequence <= self.last_event_sequence {
            return Err(format!(
                "browser host repeated or reordered event sequence {sequence} after {}",
                self.last_event_sequence
            ));
        }
        self.last_event_sequence = sequence;
        self.pending_events.push(event);
        Ok(())
    }

    /// Closes the channel so the host stops input and exits by design.
    /// 关闭通道，使宿主按设计停止输入并退出。
    pub fn close(self) {
        let _ = self.stream.shutdown(Shutdown::Both);
    }

    /// Writes one frame to the host.
    /// 向宿主写入一个帧。
    fn write_frame(&mut self, value: &impl Serialize) -> Result<(), String> {
        let frame = encode_host_frame(value)?;
        self.stream.write_all(&frame).map_err(|error| {
            format!("failed to write to the browser host control channel: {error}")
        })
    }

    /// Reads exactly one complete frame value from the host.
    /// 从宿主精确读取一个完整帧值。
    fn read_frame_value(&mut self) -> Result<Value, String> {
        let mut buffer = [0_u8; 8192];
        loop {
            if let Some(value) = self.pending_frames.pop_front() {
                return Ok(value);
            }
            let read = self
                .stream
                .read(&mut buffer)
                .map_err(|error| format!("browser host control channel read failed: {error}"))?;
            if read == 0 {
                return Err(String::from("browser host closed the control channel"));
            }
            self.pending_frames
                .extend(self.decoder.push(&buffer[..read])?);
        }
    }
}

/// Page identity mapping that never uses the page URL as a primary key.
/// 绝不以页面地址作为主键的页面身份映射。
///
/// Two pages showing the same URL are different pages, and a page keeps its Core page id while its
/// URL changes, so every lookup is by Core-assigned identity or by the host target id.
/// 显示同一地址的两个页面是两个不同页面；页面在地址变化时保留其 Core 页面标识。因此所有查找都按
/// Core 分配的标识或宿主目标标识进行。
#[derive(Debug, Default, Clone)]
pub struct BrowserPageRegistry {
    /// Host generation these page identities belong to.
    /// 这些页面身份所属的宿主代际。
    host_generation: u64,
    /// Next Core page identity index.
    /// 下一个 Core 页面标识序号。
    next_page_index: u64,
    /// Page records keyed by Core page id.
    /// 按 Core 页面标识索引的页面记录。
    pages: BTreeMap<String, BrowserPageRecord>,
    /// Core page id lookup by host target id.
    /// 按宿主目标标识查找 Core 页面标识。
    target_index: BTreeMap<String, String>,
}

/// One Core page identity bound to one host target.
/// 一个绑定到宿主目标的 Core 页面身份。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserPageRecord {
    /// Stable Core page identity.
    /// 稳定的 Core 页面标识。
    pub page_id: String,
    /// Current host target identity.
    /// 当前宿主目标标识。
    pub target_id: String,
    /// Last observed page URL, for display only.
    /// 最近观察到的页面地址，仅用于展示。
    pub url: String,
    /// Last Chromium target title; absent until host metadata arrives.
    /// 最新 Chromium 目标标题；宿主元数据到达前为空。
    #[serde(default)]
    pub title: Option<String>,
    /// Page identity that opened this page, when one did.
    /// 打开本页面的页面标识（如有）。
    pub opener_page_id: Option<String>,
    /// Host generation that produced this page.
    /// 产生本页面的宿主代际。
    pub host_generation: u64,
}

impl BrowserPageRegistry {
    /// Creates one empty registry for the given host generation.
    /// 为给定宿主代际创建一个空注册表。
    pub fn new(host_generation: u64) -> Self {
        Self {
            host_generation,
            next_page_index: 0,
            pages: BTreeMap::new(),
            target_index: BTreeMap::new(),
        }
    }

    /// Returns the host generation these identities belong to.
    /// 返回这些身份所属的宿主代际。
    pub fn host_generation(&self) -> u64 {
        self.host_generation
    }

    /// Adopts one host target as a Core page, or returns the existing page for that target.
    /// 将一个宿主目标纳入 Core 页面，或返回该目标已有的页面。
    ///
    /// # Parameters
    /// 参数
    /// `target_id` is the host target identity and `url` is the currently observed URL.
    /// `target_id` 是宿主目标标识，`url` 是当前观察到的地址。
    ///
    /// # Returns
    /// 返回值
    /// The stable Core page id for that target.
    /// 返回该目标的稳定 Core 页面标识。
    pub fn adopt_target(&mut self, target_id: &str, url: &str) -> String {
        if let Some(page_id) = self.target_index.get(target_id) {
            if let Some(page) = self.pages.get_mut(page_id) {
                // A navigation changes the URL but must never change the page identity.
                // 导航会改变地址，但绝不改变页面标识。
                if page.url != url {
                    page.title = None;
                    page.url = String::from(url);
                }
            }
            return page_id.clone();
        }
        self.next_page_index += 1;
        let page_id = format!("page-{}", self.next_page_index);
        self.pages.insert(
            page_id.clone(),
            BrowserPageRecord {
                page_id: page_id.clone(),
                target_id: String::from(target_id),
                url: String::from(url),
                title: None,
                opener_page_id: None,
                host_generation: self.host_generation,
            },
        );
        self.target_index
            .insert(String::from(target_id), page_id.clone());
        page_id
    }

    /// Adopts one page opened by an existing page, preserving the opener relation.
    /// 纳入一个由既有页面打开的新页面，并保留打开者关系。
    pub fn adopt_opened_target(
        &mut self,
        target_id: &str,
        url: &str,
        opener_page_id: Option<&str>,
    ) -> String {
        let page_id = self.adopt_target(target_id, url);
        if let (Some(opener), Some(page)) = (opener_page_id, self.pages.get_mut(&page_id))
            && page.opener_page_id.is_none()
        {
            page.opener_page_id = Some(String::from(opener));
        }
        page_id
    }

    /// Resolves one Core page id to its current host target.
    /// 将一个 Core 页面标识解析为其当前宿主目标。
    pub fn target_for_page(&self, page_id: &str) -> Option<&BrowserPageRecord> {
        self.pages.get(page_id)
    }

    /// Updates URL and title for a registered target; returns false for a destroyed or unknown target.
    /// 更新已登记目标的地址与标题；目标已销毁或未知时返回 false。
    /// `target_id`, `url`, and `title` are the same Chromium target metadata snapshot.
    /// `target_id`、`url` 和 `title` 来自同一份 Chromium 目标元数据快照。
    pub fn update_metadata(&mut self, target_id: &str, url: &str, title: &str) -> bool {
        let Some(page_id) = self.target_index.get(target_id) else {
            return false;
        };
        let Some(page) = self.pages.get_mut(page_id) else {
            return false;
        };
        page.url = String::from(url);
        page.title = Some(String::from(title));
        true
    }

    /// Removes a destroyed host target from both page indexes.
    /// 从两个页面索引移除已销毁的宿主目标。
    ///
    /// # Returns / 返回值
    /// The removed page when the target belonged to this generation.
    /// 目标属于本代际时返回被移除的页面。
    pub fn remove_target(&mut self, target_id: &str) -> Option<BrowserPageRecord> {
        let page_id = self.target_index.remove(target_id)?;
        self.pages.remove(&page_id)
    }

    /// Lists every page known for this host generation, ordered by page id.
    /// 按页面标识顺序列出本宿主代际已知的全部页面。
    pub fn pages(&self) -> Vec<BrowserPageRecord> {
        self.pages.values().cloned().collect()
    }

    /// Replaces the host generation and invalidates every previous page identity.
    /// 替换宿主代际并使此前所有页面身份失效。
    ///
    /// # Returns
    /// 返回值
    /// The number of page identities invalidated by the generation change.
    /// 返回因代际变化而失效的页面身份数量。
    pub fn advance_host_generation(&mut self, host_generation: u64) -> usize {
        let invalidated = self.pages.len();
        self.host_generation = host_generation;
        self.pages.clear();
        self.target_index.clear();
        // Keep the counter across host generations: reusing `page-1` would let a stale client
        // address a different page after a crash and explicit reopen.
        // 跨宿主代际保留序号：复用 `page-1` 会使旧客户端在崩溃并明确重开后指向另一页面。
        invalidated
    }
}

/// Supervision state of one session's browser host.
/// 单个会话浏览器宿主的监督状态。
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum BrowserHostSupervisionState {
    /// No host process has been started for this session yet.
    /// 本会话尚未启动任何宿主进程。
    NotStarted,
    /// A host process is running with a confirmed control channel.
    /// 宿主进程正在运行且控制通道已确认。
    Running,
    /// The host process ended unexpectedly; the snapshot is retained for diagnosis.
    /// 宿主进程意外结束；快照被保留用于诊断。
    Crashed,
    /// The session ended explicitly and its host was closed.
    /// 会话显式结束且其宿主已关闭。
    Closed,
}

/// Retained diagnosis of one crashed browser host.
/// 一个已崩溃浏览器宿主的保留诊断。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserHostCrashSnapshot {
    /// Host generation that crashed.
    /// 崩溃的宿主代际。
    pub host_generation: u64,
    /// Operating-system process id of the crashed host.
    /// 崩溃宿主的操作系统进程标识。
    pub pid: u32,
    /// Exit code reported by the operating system, when one was observed.
    /// 操作系统报告的退出码（如已观察到）。
    pub exit_code: Option<i32>,
    /// User-facing reason for the crash.
    /// 面向用户的崩溃原因。
    pub reason: String,
}

/// Supervision record for one session's browser host.
/// 单个会话浏览器宿主的监督记录。
#[derive(Debug, Clone)]
pub struct BrowserHostSupervision {
    /// Current supervision state.
    /// 当前监督状态。
    state: BrowserHostSupervisionState,
    /// Current host generation assigned by Core.
    /// Core 分配的当前宿主代际。
    host_generation: u64,
    /// Operating-system process id of the live host.
    /// 活动宿主的操作系统进程标识。
    live_pid: Option<u32>,
    /// Retained crash snapshot, when the last host crashed.
    /// 上次宿主崩溃时保留的崩溃快照。
    crash: Option<BrowserHostCrashSnapshot>,
    /// Page identities bound to the current host generation.
    /// 绑定到当前宿主代际的页面身份。
    pages: BrowserPageRegistry,
}

impl Default for BrowserHostSupervision {
    fn default() -> Self {
        Self::new()
    }
}

impl BrowserHostSupervision {
    /// Creates supervision state for a session that has not started a host yet.
    /// 为一个尚未启动宿主的会话创建监督状态。
    pub fn new() -> Self {
        Self {
            state: BrowserHostSupervisionState::NotStarted,
            host_generation: 0,
            live_pid: None,
            crash: None,
            pages: BrowserPageRegistry::new(0),
        }
    }

    /// Returns the current supervision state.
    /// 返回当前监督状态。
    pub fn state(&self) -> &BrowserHostSupervisionState {
        &self.state
    }

    /// Returns the current host generation.
    /// 返回当前宿主代际。
    pub fn host_generation(&self) -> u64 {
        self.host_generation
    }

    /// Returns the live host process id, when one is running.
    /// 返回活动宿主进程标识（如正在运行）。
    pub fn live_pid(&self) -> Option<u32> {
        self.live_pid
    }

    /// Returns the retained crash snapshot, when the last host crashed.
    /// 返回保留的崩溃快照（如上次宿主崩溃）。
    pub fn crash(&self) -> Option<&BrowserHostCrashSnapshot> {
        self.crash.as_ref()
    }

    /// Returns the page identities of the current host generation.
    /// 返回当前宿主代际的页面身份。
    pub fn pages(&self) -> &BrowserPageRegistry {
        &self.pages
    }

    /// Returns the mutable page identities of the current host generation.
    /// 返回当前宿主代际的可变页面身份。
    pub fn pages_mut(&mut self) -> &mut BrowserPageRegistry {
        &mut self.pages
    }

    /// Records a confirmed running host and clears any previous crash snapshot.
    /// 记录一个已确认运行的宿主并清除此前的崩溃快照。
    ///
    /// # Returns
    /// 返回值
    /// The host generation this running host owns.
    /// 返回该运行宿主拥有的宿主代际。
    pub fn record_running(&mut self, pid: u32) -> u64 {
        self.host_generation += 1;
        self.state = BrowserHostSupervisionState::Running;
        self.live_pid = Some(pid);
        self.crash = None;
        let generation = self.host_generation;
        self.pages.advance_host_generation(generation);
        generation
    }

    /// Records an unexpected host exit and retains a diagnosis snapshot.
    /// 记录一次意外的宿主退出并保留诊断快照。
    ///
    /// # Returns
    /// 返回值
    /// The invalidated page identity count for the crashed generation.
    /// 返回崩溃代际被失效的页面身份数量。
    pub fn record_crash(&mut self, exit_code: Option<i32>, reason: &str) -> usize {
        let pid = self.live_pid.unwrap_or_default();
        let invalidated = self.pages.advance_host_generation(self.host_generation);
        self.state = BrowserHostSupervisionState::Crashed;
        self.live_pid = None;
        self.crash = Some(BrowserHostCrashSnapshot {
            host_generation: self.host_generation,
            pid,
            exit_code,
            reason: String::from(reason),
        });
        invalidated
    }

    /// Records an explicit session close, which is the only path that ends the context.
    /// 记录一次显式的会话结束，这是唯一结束上下文的路径。
    pub fn record_closed(&mut self) -> usize {
        let invalidated = self.pages.advance_host_generation(self.host_generation);
        self.state = BrowserHostSupervisionState::Closed;
        self.live_pid = None;
        invalidated
    }

    /// Returns whether reopening this session requires an explicit user action.
    /// 返回重开本会话是否需要明确的用户操作。
    ///
    /// A crashed host is never restarted implicitly, so unsubmitted page state is never silently
    /// lost and a persistent profile is never reopened behind the user's back.
    /// 崩溃的宿主绝不被隐式重启，因此未提交的页面状态绝不被静默丢弃，持久画像也绝不被背着用户重开。
    pub fn requires_explicit_reopen(&self) -> bool {
        matches!(self.state, BrowserHostSupervisionState::Crashed)
    }
}

/// Control discovery document published by one browser host.
/// 单个浏览器宿主发布的控制发现文档。
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserHostControlDiscovery {
    /// Schema version of this document.
    /// 本文档的架构版本。
    pub schema_version: u32,
    /// Control protocol version understood by the host.
    /// 宿主理解的控制协议版本。
    pub protocol_version: u32,
    /// Loopback host of the control listener.
    /// 控制监听器的回环主机。
    pub control_host: String,
    /// Ephemeral loopback port of the control listener.
    /// 控制监听器的临时回环端口。
    pub control_port: u16,
    /// Operating-system process id of the host.
    /// 宿主的操作系统进程标识。
    pub pid: u32,
    /// Session identity assigned by Core.
    /// 由 Core 分配的会话标识。
    pub session_id: String,
    /// Host generation assigned by Core.
    /// 由 Core 分配的宿主代际。
    pub host_generation: String,
}

/// Reads and validates one control discovery document.
/// 读取并校验一份控制发现文档。
///
/// # Parameters
/// 参数
/// `path` is the discovery file inside the host's own user-data directory.
/// `path` 是宿主自有用户数据目录内的发现文件。
///
/// # Returns
/// 返回值
/// The validated document, or `None` while the host has not published a usable endpoint yet.
/// 返回已校验文档；宿主尚未发布可用端点时返回 `None`。
pub fn read_control_discovery(path: &std::path::Path) -> Option<BrowserHostControlDiscovery> {
    let body = std::fs::read_to_string(path).ok()?;
    let document: BrowserHostControlDiscovery = serde_json::from_str(&body).ok()?;
    if document.schema_version != BROWSER_HOST_CONTROL_DISCOVERY_SCHEMA_VERSION
        || document.protocol_version != BROWSER_HOST_PROTOCOL_VERSION
        || document.control_port == 0
        || document.control_host.is_empty()
    {
        return None;
    }
    Some(document)
}

/// One planned browser host process launch.
/// 一次已规划的浏览器宿主进程启动。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserHostLaunchPlan {
    /// Managed Node executable that hosts the independent Core browser.
    /// 承载独立 Core 浏览器的受管 Node 可执行程序。
    pub executable: std::path::PathBuf,
    /// Entry script of the browser host inside the runtime.
    /// 运行时内浏览器宿主的入口脚本。
    pub entry: std::path::PathBuf,
    /// Exact standalone Chromium executable selected by the runtime layout.
    /// 由运行时布局选定的精确独立 Chromium 可执行文件。
    pub chromium_executable: std::path::PathBuf,
    /// Session identity assigned by Core.
    /// 由 Core 分配的会话标识。
    pub session_id: String,
    /// Host generation assigned by Core.
    /// 由 Core 分配的宿主代际。
    pub host_generation: String,
    /// Per-session user-data directory owned by this host.
    /// 本宿主拥有的每会话用户数据目录。
    pub user_data_directory: std::path::PathBuf,
    /// Directory where this attempt publishes its control endpoint.
    /// 本次尝试发布其控制端点的目录。
    ///
    /// This is deliberately separate from `user_data_directory`: a persistent profile is shared by
    /// several sequential sessions, so a discovery file stored inside it would be inherited as
    /// stale state by the next session.
    /// 该目录有意与 `user_data_directory` 分离：持久画像会被多个顺序会话共享，因此存放在其中的
    /// 发现文件会被下一个会话当作陈旧状态继承。
    pub control_directory: std::path::PathBuf,
    /// One-time control secret passed only to this host.
    /// 只传给本宿主的一次性控制密钥。
    pub control_secret: String,
    /// Initial URL loaded before Core connects.
    /// Core 连接前加载的初始地址。
    pub initial_url: String,
}

/// Converts one Rust path into the form Chromium accepts on the command line.
/// 将 Rust 路径转换为 Chromium 在命令行上可接受的形式。
///
/// # Parameters
/// 参数
/// `path` is any absolute path, possibly in the Windows verbatim form.
/// `path` 是任意绝对路径，可能是 Windows verbatim 形式。
///
/// # Returns
/// 返回值
/// The path text with the Windows verbatim prefix removed.
/// 已移除 Windows verbatim 前缀的路径文本。
///
/// # Why this exists / 存在原因
/// `std::fs::canonicalize` on Windows returns an extended-length path beginning with `\\?\`.
/// Chromium and its leveldb and network-service stores do not create their databases correctly
/// under that form: measured on Windows, the profile's `Local Storage/leveldb` became a one-byte
/// file instead of a directory and `Network/Cookies` was never created at all, so no cookie or
/// local-storage entry ever survived into a later session. Rust filesystem calls keep using the
/// original path; only the text handed to the child process is normalised.
/// Windows 上 `std::fs::canonicalize` 返回以 `\\?\` 开头的扩展长度路径。Chromium 及其 leveldb 与
/// 网络服务存储在该形式下无法正确创建数据库：在 Windows 实测中，画像的 `Local Storage/leveldb`
/// 变成一个一字节的文件而不是目录，`Network/Cookies` 则完全未被创建，因此没有任何 Cookie 或
/// 本地存储条目能延续到后续会话。Rust 文件系统调用继续使用原始路径；只有交给子进程的文本被规范化。
pub fn chromium_path_argument(path: &std::path::Path) -> String {
    let text = path.to_string_lossy();
    // `\\?\UNC\server\share` denotes a network path whose real form is `\\server\share`.
    // `\\?\UNC\server\share` 表示真实形式为 `\\server\share` 的网络路径。
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    if let Some(rest) = text.strip_prefix(r"\\?\") {
        return String::from(rest);
    }
    text.into_owned()
}

impl BrowserHostLaunchPlan {
    /// Builds the exact argument vector for the host process.
    /// 构建宿主进程的精确参数向量。
    ///
    /// # Returns
    /// 返回值
    /// The documented host arguments in their stable order.
    /// 按稳定顺序返回文档规定的宿主参数。
    pub fn arguments(&self) -> Vec<String> {
        vec![
            // Node's module loader rejects Windows verbatim paths even when the OS accepts them.
            // 即使操作系统接受 Windows verbatim 路径，Node 模块加载器也会拒绝它们。
            chromium_path_argument(&self.entry),
            format!("--session-id={}", self.session_id),
            format!("--host-generation={}", self.host_generation),
            format!(
                "--user-data-dir={}",
                chromium_path_argument(&self.user_data_directory)
            ),
            // The endpoint directory is passed explicitly so this attempt's publication never
            // collides with a profile directory shared by earlier sessions.
            // 端点目录被显式传入，使本次尝试的发布绝不与早前会话共享的画像目录冲突。
            format!(
                "--control-dir={}",
                chromium_path_argument(&self.control_directory)
            ),
            // Port zero asks the runtime for an ephemeral loopback port, which the host reports
            // back through its own discovery file instead of a machine-wide port scan.
            // 端口零要求运行时提供临时回环端口，宿主通过其自有发现文件回报，而不是全机器扫描端口。
            String::from("--cdp-port=0"),
            format!(
                "--chromium-executable={}",
                chromium_path_argument(&self.chromium_executable)
            ),
            format!("--control-secret={}", self.control_secret),
            format!("--initial-url={}", self.initial_url),
        ]
    }

    /// Returns the control discovery path for this attempt.
    /// 返回本次尝试的控制发现路径。
    pub fn control_discovery_path(&self) -> std::path::PathBuf {
        self.control_directory
            .join(BROWSER_HOST_CONTROL_DISCOVERY_FILENAME)
    }

    /// Waits for this host to publish a usable control endpoint.
    /// 等待本宿主发布可用控制端点。
    ///
    /// # Parameters
    /// 参数
    /// `timeout` bounds the wait; the host is expected to publish within a few seconds.
    /// `timeout` 界定等待时间；宿主预期在数秒内发布。
    ///
    /// # Returns
    /// 返回值
    /// The published discovery document, or a diagnostic when the deadline expires.
    /// 返回已发布发现文档；超过截止时间时返回诊断。
    pub fn await_control_discovery(
        &self,
        timeout: Duration,
    ) -> Result<BrowserHostControlDiscovery, String> {
        let path = self.control_discovery_path();
        let deadline = std::time::Instant::now() + timeout;
        // A file left by an earlier attempt is stale state, not an answer. It is never adopted, and
        // it does not end the wait: this attempt's own host is still expected to publish.
        // 早前尝试留下的文件是陈旧状态，而不是答案。它绝不会被采用，也不会结束等待：本次尝试自己的
        // 宿主仍被期待发布端点。
        let mut stale: Option<String> = None;
        loop {
            if let Some(document) = read_control_discovery(&path) {
                if document.session_id == self.session_id
                    && document.host_generation == self.host_generation
                {
                    return Ok(document);
                }
                stale = Some(format!(
                    "session `{}` generation `{}`",
                    document.session_id, document.host_generation
                ));
            }
            if std::time::Instant::now() >= deadline {
                return match stale {
                    Some(stale) => Err(format!(
                        "browser host did not publish a control endpoint at {} within {:?}; the file still names {stale} instead of `{}`/`{}`",
                        path.display(),
                        timeout,
                        self.session_id,
                        self.host_generation
                    )),
                    None => Err(format!(
                        "browser host did not publish a control endpoint at {} within {:?}",
                        path.display(),
                        timeout
                    )),
                };
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

/// One live browser host process owned by Core.
/// Core 拥有的一个活动浏览器宿主进程。
pub trait BrowserHostProcessHandle: Send {
    /// Returns the operating-system process id.
    /// 返回操作系统进程标识。
    fn pid(&self) -> u32;

    /// Returns an ended process's exit code or an explicit inspection error.
    /// 返回已结束进程的退出码，或显式的检查错误。
    fn try_wait(&mut self) -> Result<Option<i32>, String>;

    /// Terminates the process when it is still running.
    /// 进程仍在运行时终止它。
    fn kill(&mut self);
}

/// Spawns one browser host process from a launch plan.
/// 根据启动计划启动一个浏览器宿主进程。
///
/// The manager depends on this trait rather than on `std::process::Command` directly, so tests can
/// supply a deterministic host and production supplies the shared desktop runtime.
/// 管理器依赖本 trait 而不是直接依赖 `std::process::Command`，因此测试可以提供确定性宿主，生产环境
/// 提供共享桌面运行时。
pub trait BrowserHostSpawner: Send + Sync {
    /// Starts one host process for the given plan.
    /// 为给定计划启动一个宿主进程。
    fn spawn(
        &self,
        plan: &BrowserHostLaunchPlan,
    ) -> Result<Box<dyn BrowserHostProcessHandle>, String>;
}

/// Real browser host process backed by one operating-system child.
/// 由一个操作系统子进程支撑的真实浏览器宿主进程。
pub struct SystemBrowserHostProcess {
    /// Owned child process.
    /// 拥有的子进程。
    child: std::process::Child,
}

impl BrowserHostProcessHandle for SystemBrowserHostProcess {
    /// Returns the operating-system process id of the child.
    /// 返回子进程的操作系统进程标识。
    fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Returns an ended child's exit code or an explicit inspection error.
    /// 返回已结束子进程的退出码，或显式的检查错误。
    fn try_wait(&mut self) -> Result<Option<i32>, String> {
        match self.child.try_wait() {
            Ok(Some(status)) => Ok(Some(status.code().unwrap_or(-1))),
            Ok(None) => Ok(None),
            Err(error) => Err(format!(
                "failed to inspect browser host process {}: {error}",
                self.child.id()
            )),
        }
    }

    /// Terminates the child process.
    /// 终止子进程。
    fn kill(&mut self) {
        let _ = self.child.kill();
    }
}

/// Spawner that starts the independent Node runtime as one browser host.
/// 将独立 Node 运行时作为浏览器宿主启动的启动器。
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemBrowserHostSpawner;

impl BrowserHostSpawner for SystemBrowserHostSpawner {
    /// Starts one detached browser host process.
    /// 启动一个分离的浏览器宿主进程。
    ///
    /// # Parameters
    /// 参数
    /// `plan` carries the exact executable, entry, profile directory, and one-time control secret.
    /// `plan` 携带精确的可执行文件、入口、画像目录与一次性控制密钥。
    ///
    /// # Returns
    /// 返回值
    /// The owned child handle, or a diagnostic when the runtime could not be started.
    /// 返回拥有的子进程句柄；运行时无法启动时返回诊断。
    fn spawn(
        &self,
        plan: &BrowserHostLaunchPlan,
    ) -> Result<Box<dyn BrowserHostProcessHandle>, String> {
        // CREATE_NO_WINDOW cannot safely inherit a Windows console stderr handle. Give Node
        // a real file handle whose lifetime is independent of the Core console and GUI.
        // CREATE_NO_WINDOW 无法安全继承 Windows 控制台的 stderr 句柄。为 Node 提供真实文件句柄，
        // 使其生命周期独立于 Core 控制台与 GUI，避免慢截图日志触发 EBADF 并终止宿主。
        let diagnostics_path = plan.control_directory.join("browser-host-stderr.log");
        let diagnostics = std::fs::File::create(&diagnostics_path).map_err(|error| {
            format!(
                "failed to create browser host diagnostics {}: {error}",
                diagnostics_path.display()
            )
        })?;
        // Core starts the declared Node runtime, independently of the GUI's Electron process.
        // Core 启动已声明的 Node 运行时，与 GUI 的 Electron 进程独立。
        let mut command = std::process::Command::new(&plan.executable);
        command
            .args(plan.arguments())
            .env_remove("ELECTRON_RUN_AS_NODE")
            // Keep the established authenticated control channel independent of process stdio.
            // 保持既有鉴权控制通道独立于进程标准输入输出。
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::from(diagnostics));
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // A background browser must not open a separate Node console window.
            // 后台浏览器不应弹出额外 Node 控制台窗口。
            command.creation_flags(vulcan_host_tools::process_creation::WINDOWS_CREATE_NO_WINDOW);
        }
        // The child owns its inherited file handle even after this local Command is dropped.
        // 即使本地 Command 被销毁，子进程仍拥有继承的文件句柄。
        let child = command.spawn().map_err(|error| {
            format!(
                "failed to start the browser host {}: {error}",
                plan.executable.display()
            )
        })?;
        Ok(Box::new(SystemBrowserHostProcess { child }))
    }
}

/// Runtime configuration required to start browser hosts.
/// 启动浏览器宿主所需的运行时配置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserHostRuntimeConfig {
    /// Managed Node executable that hosts the independent Core browser.
    /// 承载独立 Core 浏览器的受管 Node 可执行程序。
    pub executable: std::path::PathBuf,
    /// Entry script of the browser host inside the runtime.
    /// 运行时内浏览器宿主的入口脚本。
    pub entry: std::path::PathBuf,
    /// Exact standalone Chromium executable for every host this Core launches.
    /// 此 Core 启动的每个宿主所用的精确独立 Chromium 可执行文件。
    pub chromium_executable: std::path::PathBuf,
    /// Root directory holding ephemeral per-browser and persistent per-conversation profiles.
    /// 保存按浏览器隔离的临时画像与按对话隔离的持久画像的根目录。
    pub profiles_root: std::path::PathBuf,
    /// Root directory holding files Core stages on behalf of a session.
    /// 存放 Core 代表会话暂存文件的根目录。
    ///
    /// Uploaded content is materialised here so a client's own filesystem path is never handed to
    /// the browser; the host only ever receives a path Core created.
    /// 上传内容在此落地，因此客户端自身的文件系统路径绝不会交给浏览器；宿主只会收到 Core 创建的路径。
    pub staging_root: std::path::PathBuf,
    /// How long Core waits for a started host to publish its control endpoint.
    /// Core 等待已启动宿主发布控制端点的最长时间。
    pub discovery_timeout: Duration,
    /// How long Core waits for one host request response.
    /// Core 等待单次宿主请求响应的最长时间。
    pub request_timeout: Duration,
}

impl BrowserHostRuntimeConfig {
    /// Builds one launch plan for the given session, profile mode, and generation.
    /// 为给定会话、画像模式与代际构建一份启动计划。
    ///
    /// # Design note / 设计说明
    /// A persistent profile is stable only within its owning conversation. Hashing the owner
    /// prevents client-provided IDs from becoming path components. Ephemeral profiles use the
    /// Core-generated browser session ID and never share state.
    /// 持久画像仅在所属对话内稳定复用。对归属标识取哈希可防止客户端提交的 ID 成为路径片段。
    /// 临时画像使用 Core 生成的浏览器会话 ID，绝不共享状态。
    pub fn launch_plan(
        &self,
        session_id: &str,
        owner_session_id: &str,
        profile_mode: crate::browser::BrowserProfileMode,
        host_generation: u64,
        control_secret: &str,
    ) -> Result<BrowserHostLaunchPlan, String> {
        let user_data_directory = match profile_mode {
            crate::browser::BrowserProfileMode::Persistent => self
                .profiles_root
                .join("persistent")
                .join(hex::encode(sha2::Sha256::digest(
                    owner_session_id.as_bytes(),
                ))),
            crate::browser::BrowserProfileMode::Ephemeral => {
                self.profiles_root.join("ephemeral").join(session_id)
            }
        };
        std::fs::create_dir_all(&user_data_directory).map_err(|error| {
            format!(
                "failed to create the browser profile directory {}: {error}",
                user_data_directory.display()
            )
        })?;
        // The endpoint of this attempt is published per session, never inside a profile directory
        // that a later session may inherit.
        // 本次尝试的端点按会话发布，绝不放进后续会话可能继承的画像目录内。
        let control_directory = self.profiles_root.join("control").join(session_id);
        std::fs::create_dir_all(&control_directory).map_err(|error| {
            format!(
                "failed to create the browser control directory {}: {error}",
                control_directory.display()
            )
        })?;
        Ok(BrowserHostLaunchPlan {
            executable: self.executable.clone(),
            entry: self.entry.clone(),
            chromium_executable: self.chromium_executable.clone(),
            session_id: String::from(session_id),
            host_generation: host_generation.to_string(),
            user_data_directory,
            control_directory,
            control_secret: String::from(control_secret),
            initial_url: String::from("about:blank"),
        })
    }
}

/// Serializes one supervision record as a client-facing JSON summary.
/// 将一份监督记录序列化为面向客户端的 JSON 摘要。
pub fn browser_host_supervision_summary(supervision: &BrowserHostSupervision) -> Value {
    json!({
        "state": match supervision.state() {
            BrowserHostSupervisionState::NotStarted => "notStarted",
            BrowserHostSupervisionState::Running => "running",
            BrowserHostSupervisionState::Crashed => "crashed",
            BrowserHostSupervisionState::Closed => "closed",
        },
        "hostGeneration": supervision.host_generation().to_string(),
        "pid": supervision.live_pid(),
        "pageCount": supervision.pages().pages().len(),
        "requiresExplicitReopen": supervision.requires_explicit_reopen(),
        "crash": supervision.crash().map(|crash| json!({
            "hostGeneration": crash.host_generation.to_string(),
            "pid": crash.pid,
            "exitCode": crash.exit_code,
            "reason": crash.reason,
        })),
    })
}

/// Test-only helpers for injecting host events without a live socket.
/// 仅测试使用：在没有活动套接字的情况下注入宿主事件的辅助方法。
#[cfg(test)]
impl BrowserHostConnection {
    /// Records one synthetic host event with the next monotonic sequence.
    /// 以下一个单调序号记录一条合成宿主事件。
    ///
    /// # Parameters
    /// 参数
    /// `kind` is the event kind and `payload` is its typed payload.
    /// `kind` 是事件类型，`payload` 是其类型化载荷。
    ///
    /// # Returns
    /// 返回值
    /// `Ok(())` when the event was accepted, or a diagnostic when the sequence could not advance.
    /// 事件被接受时返回 `Ok(())`；序号无法推进时返回诊断。
    pub(crate) fn push_test_event(&mut self, kind: &str, payload: Value) -> Result<(), String> {
        let sequence = self.last_event_sequence + 1;
        self.accept_event(BrowserHostEvent {
            event_sequence: sequence.to_string(),
            kind: String::from(kind),
            payload,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Preserves events and responses when TCP delivers several complete frames together.
    /// TCP 一次交付多个完整帧时保留全部事件和响应。
    #[test]
    fn coalesced_handshake_events_and_response_are_all_retained() {
        // Preload the complete burst before requesting a response to force a coalesced read.
        // 在读取响应前交付完整突发消息，以覆盖合并读取。
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("address");
        let (sent_tx, sent_rx) = std::sync::mpsc::channel();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().expect("accept");
            let mut burst = encode_host_frame(&handshake_value("session", "1")).expect("handshake");
            burst.extend(
                encode_host_frame(&json!({
                    "eventSequence":"1", "kind":"pageCreated", "payload":{"targetId":"page"}
                }))
                .expect("event"),
            );
            burst.extend(
                encode_host_frame(&json!({
                    "requestId":"core-1", "ok":true, "result":{"alive":true}
                }))
                .expect("response"),
            );
            socket.write_all(&burst).expect("send burst");
            sent_tx.send(()).expect("sent");
            done_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("finish");
        });
        let mut connection = BrowserHostConnection::connect(
            &address.to_string(),
            hello_fixture("session", "1"),
            Duration::from_secs(1),
        )
        .expect("connect");
        sent_rx.recv().expect("burst sent");
        let result = connection.request("host.ping", json!({}), Duration::from_millis(250));
        let events = connection.drain_events().expect("drain events");
        done_tx.send(()).expect("finish server");
        server.join().expect("join server");
        assert!(
            matches!(result, BrowserHostRequestOutcome::Completed(_)),
            "{result:?}"
        );
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, "pageCreated");
    }

    /// Rejects a disconnected host channel instead of reporting an empty event batch.
    /// 拒绝已断开的宿主通道，避免将其误报为空事件批次。
    #[test]
    fn event_drain_rejects_a_closed_host_channel() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("address");
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().expect("accept");
            let mut buffer = [0_u8; 4096];
            // A TCP read may split the hello frame; acknowledge only a complete decoded message.
            // TCP 读取可能拆分 hello 帧；仅在完整解码消息后确认。
            let mut decoder = BrowserHostFrameDecoder::new();
            loop {
                let read = socket.read(&mut buffer).expect("read hello");
                assert_ne!(read, 0, "client closed before hello completed");
                if !decoder
                    .push(&buffer[..read])
                    .expect("decode hello")
                    .is_empty()
                {
                    break;
                }
            }
            let frame = encode_host_frame(&handshake_value("session", "1")).expect("handshake");
            socket.write_all(&frame).expect("write handshake");
        });
        let mut connection = BrowserHostConnection::connect(
            &address.to_string(),
            hello_fixture("session", "1"),
            Duration::from_secs(1),
        )
        .expect("connect");
        server.join().expect("closed host");

        let error = connection
            .drain_events()
            .expect_err("closed host must fail");
        assert!(error.contains("closed the control channel"), "{error}");
    }

    /// Builds one hello for the given session and generation.
    fn hello_fixture(session_id: &str, host_generation: &str) -> BrowserHostCoreHello {
        BrowserHostCoreHello {
            kind: String::from("coreHello"),
            protocol_version: BROWSER_HOST_PROTOCOL_VERSION,
            connection_secret: String::from("test-secret"),
            session_id: String::from(session_id),
            host_generation: String::from(host_generation),
        }
    }

    /// Builds one valid handshake document.
    fn handshake_value(session_id: &str, host_generation: &str) -> Value {
        json!({
            "protocolVersion": BROWSER_HOST_PROTOCOL_VERSION,
            "sessionId": session_id,
            "hostGeneration": host_generation,
            "pid": 4242,
            "runtimeVersion": "0.1.0",
            "capabilities": ["playwright-endpoint-on-demand", "page.create"],
        })
    }

    /// Verifies the frame codec survives the chunking that the real transport produces.
    /// 验证帧编解码器能承受真实传输产生的分块。
    #[test]
    fn frame_decoder_survives_arbitrary_transport_chunking() {
        let frame = encode_host_frame(&json!({ "kind": "hostControlReady", "protocolVersion": 1 }))
            .expect("encode frame");
        // The two-byte prefix split reproduced the original production failure.
        // 两字节前缀切分复现了最初的生产故障。
        for split in 1..frame.len() {
            let mut decoder = BrowserHostFrameDecoder::new();
            let mut values = decoder.push(&frame[..split]).expect("first chunk");
            values.extend(decoder.push(&frame[split..]).expect("second chunk"));
            assert_eq!(
                values.len(),
                1,
                "split at {split} must yield exactly one frame"
            );
            assert_eq!(values[0]["kind"], "hostControlReady");
        }
    }

    /// Verifies an oversized declared length is rejected before allocating.
    /// 验证超限的声明长度在分配之前被拒绝。
    #[test]
    fn frame_decoder_rejects_oversized_lengths() {
        let mut decoder = BrowserHostFrameDecoder::new();
        let mut chunk = ((BROWSER_HOST_MAX_FRAME_BYTES as u32) + 1)
            .to_be_bytes()
            .to_vec();
        chunk.extend_from_slice(b"{}");
        assert!(decoder.push(&chunk).is_err());
    }

    /// Verifies handshake validation refuses a foreign session or generation.
    /// 验证握手校验拒绝外来会话或代际。
    #[test]
    fn handshake_rejects_a_foreign_session_or_generation() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind test listener");
        let address = listener.local_addr().expect("listener address");
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut decoder = BrowserHostFrameDecoder::new();
            let mut buffer = [0_u8; 4096];
            let read = stream.read(&mut buffer).expect("read hello");
            assert_eq!(
                decoder.push(&buffer[..read]).expect("decode hello").len(),
                1
            );
            let body = encode_host_frame(&handshake_value("other-session", "1")).expect("encode");
            stream.write_all(&body).expect("write handshake");
        });

        let outcome = BrowserHostConnection::connect(
            &address.to_string(),
            hello_fixture("w06-session", "1"),
            Duration::from_secs(5),
        );
        assert!(outcome.is_err(), "a foreign session must not be adopted");
        server.join().expect("server thread");
    }

    /// Verifies responses are correlated by request id and never applied across requests.
    /// 验证响应按请求标识关联，绝不跨请求套用。
    #[test]
    fn responses_are_correlated_by_request_id() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind test listener");
        let address = listener.local_addr().expect("listener address");
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut decoder = BrowserHostFrameDecoder::new();
            let mut buffer = [0_u8; 4096];
            let read = stream.read(&mut buffer).expect("read hello");
            decoder.push(&buffer[..read]).expect("decode hello");
            stream
                .write_all(
                    &encode_host_frame(&handshake_value("w06-session", "1")).expect("encode"),
                )
                .expect("write handshake");
            let read = stream.read(&mut buffer).expect("read request");
            let frames = decoder.push(&buffer[..read]).expect("decode request");
            let request_id = frames[0]["requestId"]
                .as_str()
                .expect("request id")
                .to_string();
            // A stale response for another request arrives first and must be ignored.
            // 属于其他请求的过期响应先到达，必须被忽略。
            stream
                .write_all(&encode_host_frame(&json!({ "requestId": "core-999", "ok": true, "result": { "wrong": true } })).expect("encode"))
                .expect("write stale response");
            stream
                .write_all(
                    &encode_host_frame(&json!({ "requestId": request_id, "ok": true, "result": { "right": true } }))
                        .expect("encode"),
                )
                .expect("write real response");
        });

        let mut connection = BrowserHostConnection::connect(
            &address.to_string(),
            hello_fixture("w06-session", "1"),
            Duration::from_secs(5),
        )
        .expect("connect");
        let outcome = connection.request("host.ping", json!({}), Duration::from_secs(5));
        match outcome {
            BrowserHostRequestOutcome::Completed(value) => {
                assert_eq!(value["right"], true, "the matching response must win");
            }
            other => panic!("expected a completed outcome, got {other:?}"),
        }
        server.join().expect("server thread");
    }

    /// Verifies a silent host yields an explicit unknown outcome rather than a retry.
    /// 验证静默宿主产生明确的未知结果，而不是重试。
    #[test]
    fn a_silent_host_yields_an_unknown_outcome() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind test listener");
        let address = listener.local_addr().expect("listener address");
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut decoder = BrowserHostFrameDecoder::new();
            let mut buffer = [0_u8; 4096];
            let read = stream.read(&mut buffer).expect("read hello");
            decoder.push(&buffer[..read]).expect("decode hello");
            stream
                .write_all(
                    &encode_host_frame(&handshake_value("w06-session", "1")).expect("encode"),
                )
                .expect("write handshake");
            // Accept the request but never answer it.
            // 接受请求但永不回答。
            let _ = stream.read(&mut buffer);
            std::thread::sleep(Duration::from_millis(400));
        });

        let mut connection = BrowserHostConnection::connect(
            &address.to_string(),
            hello_fixture("w06-session", "1"),
            Duration::from_secs(5),
        )
        .expect("connect");
        let outcome = connection.request("page.create", json!({}), Duration::from_millis(150));
        assert!(
            outcome.is_unknown(),
            "a timed-out request must be unknown, got {outcome:?}"
        );
        server.join().expect("server thread");
    }

    /// Verifies typed host failures are surfaced with their stable code.
    /// 验证类型化宿主失败以其稳定错误码暴露。
    #[test]
    fn typed_host_failures_surface_their_code() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind test listener");
        let address = listener.local_addr().expect("listener address");
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut decoder = BrowserHostFrameDecoder::new();
            let mut buffer = [0_u8; 4096];
            let read = stream.read(&mut buffer).expect("read hello");
            decoder.push(&buffer[..read]).expect("decode hello");
            stream
                .write_all(
                    &encode_host_frame(&handshake_value("w06-session", "1")).expect("encode"),
                )
                .expect("write handshake");
            let read = stream.read(&mut buffer).expect("read request");
            let frames = decoder.push(&buffer[..read]).expect("decode request");
            let request_id = frames[0]["requestId"]
                .as_str()
                .expect("request id")
                .to_string();
            stream
                .write_all(
                    &encode_host_frame(&json!({
                        "requestId": request_id,
                        "ok": false,
                        "error": { "code": "capability_unsupported", "message": "no such operation" },
                    }))
                    .expect("encode"),
                )
                .expect("write response");
        });

        let mut connection = BrowserHostConnection::connect(
            &address.to_string(),
            hello_fixture("w06-session", "1"),
            Duration::from_secs(5),
        )
        .expect("connect");
        match connection.request("page.explode", json!({}), Duration::from_secs(5)) {
            BrowserHostRequestOutcome::Failed { code, message } => {
                assert_eq!(code, "capability_unsupported");
                assert_eq!(message, "no such operation");
            }
            other => panic!("expected a typed failure, got {other:?}"),
        }
        server.join().expect("server thread");
    }

    /// Verifies page identity is never keyed by URL and survives navigation.
    /// 验证页面身份绝不以地址为键，并在导航后保持。
    #[test]
    fn page_identity_is_not_keyed_by_url_and_survives_navigation() {
        let mut registry = BrowserPageRegistry::new(1);
        let first = registry.adopt_target("target-a", "https://example.test/same");
        let second = registry.adopt_target("target-b", "https://example.test/same");
        assert!(registry.update_metadata("target-a", "https://example.test/same", "First title"));
        assert!(registry.update_metadata("target-b", "https://example.test/same", "Second title"));
        assert_eq!(
            registry.target_for_page(&first).unwrap().title.as_deref(),
            Some("First title")
        );
        assert_ne!(
            first, second,
            "two pages showing the same URL must be two distinct pages"
        );
        // Navigating keeps the page identity while its URL changes.
        // 导航保留页面标识，同时地址发生变化。
        let again = registry.adopt_target("target-a", "https://example.test/other");
        assert_eq!(registry.target_for_page(&first).unwrap().title, None);
        assert_eq!(
            registry.target_for_page(&second).unwrap().title.as_deref(),
            Some("Second title")
        );
        assert!(registry.update_metadata(
            "target-a",
            "https://example.test/other",
            "Updated title"
        ));
        assert_eq!(again, first, "navigation must not change the page identity");
        assert_eq!(
            registry
                .target_for_page(&first)
                .map(|page| page.url.as_str()),
            Some("https://example.test/other")
        );
        // Re-adopting the same target must not allocate a second page.
        // 再次纳入同一目标不得分配第二个页面。
        assert_eq!(
            registry.adopt_target("target-a", "https://example.test/other"),
            first
        );
        assert_eq!(registry.pages().len(), 2);
        let removed = registry
            .remove_target("target-a")
            .expect("remove closed page");
        assert_eq!(removed.page_id, first);
        assert!(registry.target_for_page(&first).is_none());
        assert_eq!(registry.pages().len(), 1);
        assert!(registry.remove_target("target-a").is_none());
        assert!(!registry.update_metadata("target-a", "https://example.test/other", "Late title"));
    }

    /// Verifies an opened page keeps its opener relation.
    /// 验证被打开的页面保留其打开者关系。
    #[test]
    fn opened_pages_keep_their_opener_relation() {
        let mut registry = BrowserPageRegistry::new(1);
        let opener = registry.adopt_target("target-a", "https://example.test/");
        let popup =
            registry.adopt_opened_target("target-b", "https://example.test/popup", Some(&opener));
        assert_eq!(
            registry
                .target_for_page(&popup)
                .and_then(|page| page.opener_page_id.clone()),
            Some(opener)
        );
    }

    /// Verifies a host crash invalidates page identity and demands an explicit reopen.
    /// 验证宿主崩溃使页面身份失效并要求明确重开。
    #[test]
    fn a_host_crash_invalidates_pages_and_requires_an_explicit_reopen() {
        let mut supervision = BrowserHostSupervision::new();
        let generation = supervision.record_running(9001);
        supervision
            .pages_mut()
            .adopt_target("target-a", "https://example.test/");
        assert_eq!(supervision.state(), &BrowserHostSupervisionState::Running);
        assert!(!supervision.requires_explicit_reopen());

        let invalidated = supervision.record_crash(Some(-1), "host process exited unexpectedly");
        assert_eq!(
            invalidated, 1,
            "the crashed generation's pages must be invalidated"
        );
        assert_eq!(supervision.state(), &BrowserHostSupervisionState::Crashed);
        assert!(supervision.requires_explicit_reopen());
        assert_eq!(supervision.live_pid(), None);
        let crash = supervision.crash().expect("crash snapshot");
        assert_eq!(crash.host_generation, generation);
        assert_eq!(crash.pid, 9001);
        assert_eq!(crash.exit_code, Some(-1));
        // Old page identities must not survive into the new generation.
        // 旧页面身份绝不得存活到新代际。
        assert!(supervision.pages().pages().is_empty());
        assert!(supervision.pages().target_for_page("page-1").is_none());
    }

    /// Verifies a successful reopen allocates a new generation and clears the crash snapshot.
    /// 验证成功重开会分配新代际并清除崩溃快照。
    #[test]
    fn reopening_allocates_a_new_generation_and_clears_the_crash() {
        let mut supervision = BrowserHostSupervision::new();
        let first = supervision.record_running(1);
        let old_page = supervision
            .pages_mut()
            .adopt_target("target-1", "about:blank");
        supervision.record_crash(None, "killed");
        let second = supervision.record_running(2);
        let new_page = supervision
            .pages_mut()
            .adopt_target("target-1", "about:blank");
        assert_eq!(second, first + 1);
        assert_ne!(
            old_page, new_page,
            "old page identities must never be reused"
        );
        assert!(supervision.pages().target_for_page(&old_page).is_none());
        assert_eq!(supervision.crash(), None);
        assert!(!supervision.requires_explicit_reopen());
        assert_eq!(supervision.pages().host_generation(), second);
    }

    /// Verifies an explicit close is distinct from a crash.
    /// 验证显式结束与崩溃是不同的状态。
    #[test]
    fn explicit_close_is_distinct_from_a_crash() {
        let mut supervision = BrowserHostSupervision::new();
        supervision.record_running(7);
        supervision
            .pages_mut()
            .adopt_target("target-a", "https://example.test/");
        supervision.record_closed();
        assert_eq!(supervision.state(), &BrowserHostSupervisionState::Closed);
        assert_eq!(supervision.crash(), None);
        assert!(!supervision.requires_explicit_reopen());
    }

    /// Verifies the verbatim Windows path form never reaches the Chromium command line.
    /// 验证 Windows verbatim 路径形式绝不进入 Chromium 命令行。
    #[test]
    fn chromium_arguments_never_carry_a_verbatim_path_prefix() {
        assert_eq!(
            chromium_path_argument(std::path::Path::new(r"\\?\F:\data\profiles\persistent")),
            r"F:\data\profiles\persistent"
        );
        // A verbatim network path must lose the extended prefix but keep its UNC form.
        // verbatim 网络路径必须去掉扩展前缀但保留其 UNC 形式。
        assert_eq!(
            chromium_path_argument(std::path::Path::new(r"\\?\UNC\server\share\profiles")),
            r"\\server\share\profiles"
        );
        // An ordinary path is returned unchanged.
        // 普通路径原样返回。
        assert_eq!(
            chromium_path_argument(std::path::Path::new(r"F:\data\profiles")),
            r"F:\data\profiles"
        );
        let plan = BrowserHostLaunchPlan {
            executable: std::path::PathBuf::from("electron.exe"),
            entry: std::path::PathBuf::from(r"\\?\F:\runtime\main.js"),
            chromium_executable: std::path::PathBuf::from(r"\\?\F:\runtime\chrome.exe"),
            session_id: String::from("session-1"),
            host_generation: String::from("1"),
            user_data_directory: std::path::PathBuf::from(r"\\?\F:\data\profiles\persistent"),
            control_directory: std::path::PathBuf::from(r"\\?\F:\data\control\session-1"),
            control_secret: String::from("secret"),
            initial_url: String::from("about:blank"),
        };
        let arguments = plan.arguments();
        assert!(
            arguments.iter().all(|argument| !argument.contains(r"\\?\")),
            "no Chromium argument may carry a verbatim path prefix: {arguments:?}"
        );
    }

    /// Verifies the launch plan builds the documented host arguments in order.
    /// 验证启动计划按顺序构建文档规定的宿主参数。
    #[test]
    fn launch_plan_builds_the_documented_host_arguments() {
        let plan = BrowserHostLaunchPlan {
            executable: std::path::PathBuf::from("C:/runtime/electron.exe"),
            entry: std::path::PathBuf::from("C:/runtime/host/main.js"),
            chromium_executable: std::path::PathBuf::from("C:/runtime/chromium/chrome.exe"),
            session_id: String::from("session-1"),
            host_generation: String::from("3"),
            user_data_directory: std::path::PathBuf::from("C:/data/profiles/session-1"),
            control_directory: std::path::PathBuf::from("C:/data/control/session-1"),
            control_secret: String::from("secret-value"),
            initial_url: String::from("about:blank"),
        };
        assert_eq!(
            plan.arguments(),
            vec![
                String::from("C:/runtime/host/main.js"),
                String::from("--session-id=session-1"),
                String::from("--host-generation=3"),
                String::from("--user-data-dir=C:/data/profiles/session-1"),
                String::from("--control-dir=C:/data/control/session-1"),
                String::from("--cdp-port=0"),
                String::from("--chromium-executable=C:/runtime/chromium/chrome.exe"),
                String::from("--control-secret=secret-value"),
                String::from("--initial-url=about:blank"),
            ]
        );
        // The host never receives a fixed debugging port, so no port is guessed or reused.
        // 宿主绝不接收固定调试端口，因此不猜测也不复用端口。
        assert_eq!(
            plan.arguments()
                .iter()
                .filter(|argument| argument.starts_with("--cdp-port="))
                .count(),
            1
        );
        assert!(plan.arguments().contains(&String::from("--cdp-port=0")));
        // The endpoint is published in the per-attempt control directory, never inside a profile
        // directory that a later session could inherit as stale state.
        // 端点在每次尝试的控制目录中发布，绝不放进后续会话可能作为陈旧状态继承的画像目录内。
        assert_eq!(
            plan.control_discovery_path(),
            std::path::PathBuf::from("C:/data/control/session-1/browser-host-control.json")
        );
        assert!(
            !plan
                .control_discovery_path()
                .starts_with(&plan.user_data_directory),
            "the endpoint must not be published inside the user-data directory"
        );
    }

    /// Verifies the client-facing summary exposes generation, pages, and crash facts.
    /// 验证面向客户端的摘要暴露代际、页面与崩溃事实。
    #[test]
    fn supervision_summary_reports_generation_pages_and_crash() {
        let mut supervision = BrowserHostSupervision::new();
        supervision.record_running(31);
        supervision
            .pages_mut()
            .adopt_target("target-a", "https://example.test/");
        let summary = browser_host_supervision_summary(&supervision);
        assert_eq!(summary["state"], "running");
        assert_eq!(summary["hostGeneration"], "1");
        assert_eq!(summary["pageCount"], 1);
        assert_eq!(summary["requiresExplicitReopen"], false);

        supervision.record_crash(Some(3), "unexpected exit");
        let crashed = browser_host_supervision_summary(&supervision);
        assert_eq!(crashed["state"], "crashed");
        assert_eq!(crashed["requiresExplicitReopen"], true);
        assert_eq!(crashed["crash"]["exitCode"], 3);
    }

    /// Verifies the discovery wait refuses a document from another session or generation.
    /// 验证发现等待拒绝属于其他会话或代际的文档。
    #[test]
    fn discovery_wait_refuses_a_foreign_session_or_generation() {
        let directory =
            std::env::temp_dir().join(format!("vulcan-w06-discovery-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).expect("create discovery directory");
        let plan = BrowserHostLaunchPlan {
            executable: std::path::PathBuf::from("electron.exe"),
            entry: std::path::PathBuf::from("main.js"),
            chromium_executable: std::path::PathBuf::from("chrome.exe"),
            session_id: String::from("session-1"),
            host_generation: String::from("2"),
            user_data_directory: directory.clone(),
            control_directory: directory.clone(),
            control_secret: String::from("secret"),
            initial_url: String::from("about:blank"),
        };
        std::fs::write(
            plan.control_discovery_path(),
            serde_json::to_string(&json!({
                "schemaVersion": BROWSER_HOST_CONTROL_DISCOVERY_SCHEMA_VERSION,
                "protocolVersion": BROWSER_HOST_PROTOCOL_VERSION,
                "controlHost": "127.0.0.1",
                "controlPort": 50000,
                "pid": 12,
                "sessionId": "another-session",
                "hostGeneration": "2",
            }))
            .expect("encode discovery"),
        )
        .expect("write discovery");
        let outcome = plan.await_control_discovery(Duration::from_millis(200));
        assert!(outcome.is_err(), "a foreign session must not be adopted");

        // A matching document is adopted exactly once.
        // 匹配的文档恰好被采用一次。
        std::fs::write(
            plan.control_discovery_path(),
            serde_json::to_string(&json!({
                "schemaVersion": BROWSER_HOST_CONTROL_DISCOVERY_SCHEMA_VERSION,
                "protocolVersion": BROWSER_HOST_PROTOCOL_VERSION,
                "controlHost": "127.0.0.1",
                "controlPort": 50001,
                "pid": 13,
                "sessionId": "session-1",
                "hostGeneration": "2",
            }))
            .expect("encode discovery"),
        )
        .expect("write discovery");
        let document = plan
            .await_control_discovery(Duration::from_millis(200))
            .expect("adopt");
        assert_eq!(document.control_port, 50001);
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// Exercises real hidden Node stderr writes and verifies their independent file lifetime.
    /// 验证真实隐藏 Node 的 stderr 写入及日志文件的独立生命周期。
    /// Requires the packaged Node; returns normally only after every diagnostic is retained.
    /// 要求已打包的 Node；仅在全部诊断保留且子进程正常退出后返回。
    #[test]
    #[cfg(windows)]
    #[ignore = "requires the packaged Node runtime"]
    fn real_hidden_node_retains_diagnostics_without_console_handles() {
        // The runtime layout is authoritative; no executable path or version is guessed.
        // 运行时布局是权威来源，不猜测可执行文件路径或版本。
        let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let output = std::env::var_os("VULCAN_W08_ASSET_ROOT")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| repository.join("output"));
        let layout: Value = serde_json::from_slice(
            &std::fs::read(output.join("runtime-layout.json")).expect("read runtime layout"),
        )
        .expect("decode runtime layout");
        // The probe outlives spawn(), writes more than a pipe buffer, and then writes asynchronously.
        // 探针在 spawn() 返回后继续运行，写入超过管道缓冲区的数据，再异步写入。
        let temporary = tempfile::tempdir().expect("isolated diagnostic probe");
        let script = temporary.path().join("diagnostic-probe.cjs");
        std::fs::write(&script, r#"
const { writeSync } = require('node:fs');
for (let index = 0; index < 4096; index++) writeSync(2, '[vulcan-chromium-host] slow page.capture\n');
setTimeout(() => process.stderr.write('diagnostics-complete\n'), 100);
"#).expect("write real Node stderr probe");
        // The same production spawner must supply a usable handle with CREATE_NO_WINDOW.
        // 必须由相同的生产启动器在 CREATE_NO_WINDOW 下提供可用句柄。
        let plan = BrowserHostLaunchPlan {
            executable: output.join(layout["browserHostExecutable"].as_str().expect("Node path")),
            entry: script,
            chromium_executable: output.join(
                layout["chromiumExecutable"]
                    .as_str()
                    .expect("Chromium path"),
            ),
            session_id: String::from("diagnostic-probe"),
            host_generation: String::from("1"),
            user_data_directory: temporary.path().to_path_buf(),
            control_directory: temporary.path().to_path_buf(),
            control_secret: String::from("unused-probe-secret"),
            initial_url: String::from("about:blank"),
        };
        let mut child = SystemBrowserHostSpawner
            .spawn(&plan)
            .expect("start hidden Node");
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(code) = child.try_wait().expect("poll hidden Node") {
                assert_eq!(code, 0, "diagnostic writes must not terminate Node");
                break;
            }
            if Instant::now() >= deadline {
                child.kill();
                panic!("hidden Node stalled while writing diagnostics");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let diagnostics = std::fs::read_to_string(temporary.path().join("browser-host-stderr.log"))
            .expect("read retained diagnostics");
        assert_eq!(diagnostics.matches("slow page.capture").count(), 4096);
        assert!(diagnostics.ends_with("diagnostics-complete\n"));
    }
}
