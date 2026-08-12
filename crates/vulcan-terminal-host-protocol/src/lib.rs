//! Versioned framing contract shared by the terminal host and its parent runtime.
//! 终端宿主及其父运行时共享的版本化分帧契约。

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::io::{self, Read, Write};
use std::path::PathBuf;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Identifies the only terminal-host protocol version accepted by this build.
/// 标识当前构建唯一接受的终端宿主协议版本。
pub const TERMINAL_HOST_CONTRACT: &str = "terminal-host-v1";

/// Marks every frame so accidental stdout text cannot be decoded as protocol data.
/// 标记每个数据帧，避免意外的标准输出文本被解码为协议数据。
pub const TERMINAL_HOST_FRAME_MAGIC: [u8; 4] = *b"VTH1";

/// Defines the fixed binary frame-header length in bytes.
/// 定义固定二进制帧头的字节长度。
pub const TERMINAL_HOST_FRAME_HEADER_LEN: usize = 20;

/// Bounds one frame so a corrupt or hostile peer cannot force unbounded allocation.
/// 限制单帧大小，避免损坏或恶意对端造成无界内存分配。
pub const TERMINAL_HOST_MAX_PAYLOAD_LEN: usize = 1024 * 1024;

/// Bounds terminal rows so malformed resize requests cannot force excessive screen allocation.
/// 限制终端行数，避免畸形尺寸请求造成过量屏幕内存分配。
pub const TERMINAL_HOST_MAX_ROWS: u16 = 1000;

/// Bounds terminal columns so malformed resize requests cannot force excessive screen allocation.
/// 限制终端列数，避免畸形尺寸请求造成过量屏幕内存分配。
pub const TERMINAL_HOST_MAX_COLS: u16 = 1000;

/// Defines the wire-level message kinds exchanged with one terminal host.
/// 定义与单个终端宿主交换的线级消息类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum TerminalHostFrameKind {
    /// Starts the single terminal owned by the host process.
    /// 启动由宿主进程拥有的唯一终端。
    Start = 1,
    /// Sends raw user or AI input bytes to the terminal.
    /// 将用户或 AI 的原始输入字节发送到终端。
    InputBytes = 2,
    /// Changes the terminal character-grid dimensions.
    /// 更改终端字符网格尺寸。
    Resize = 3,
    /// Requests graceful termination of the terminal process tree.
    /// 请求优雅终止终端进程树。
    Stop = 4,
    /// Confirms that the terminal child has started.
    /// 确认终端子进程已经启动。
    Started = 101,
    /// Carries raw bytes read from the terminal output stream.
    /// 承载从终端输出流读取的原始字节。
    OutputBytes = 102,
    /// Reports a terminal lifecycle-state transition.
    /// 报告终端生命周期状态转换。
    StateChanged = 103,
    /// Reports the final terminal exit state.
    /// 报告最终终端退出状态。
    Exited = 104,
    /// Reports a protocol or terminal-host failure.
    /// 报告协议或终端宿主故障。
    Error = 105,
}

impl TerminalHostFrameKind {
    /// Converts a wire value into a known message kind.
    /// 将线级数值转换为已知消息类型。
    ///
    /// # Parameters / 参数
    /// - `value`: Unsigned message-kind value from the frame header.
    /// - `value`：来自帧头的无符号消息类型值。
    ///
    /// # Returns / 返回值
    /// Returns the decoded message kind, or `InvalidData` for an unknown value.
    /// 返回解码后的消息类型；未知值返回 `InvalidData`。
    pub fn from_wire_value(value: u16) -> io::Result<Self> {
        match value {
            1 => Ok(Self::Start),
            2 => Ok(Self::InputBytes),
            3 => Ok(Self::Resize),
            4 => Ok(Self::Stop),
            101 => Ok(Self::Started),
            102 => Ok(Self::OutputBytes),
            103 => Ok(Self::StateChanged),
            104 => Ok(Self::Exited),
            105 => Ok(Self::Error),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unknown terminal-host frame kind: {value}"),
            )),
        }
    }

    /// Returns the stable wire value written into a frame header.
    /// 返回写入帧头的稳定线级数值。
    ///
    /// # Returns / 返回值
    /// Returns the protocol-defined unsigned message-kind value.
    /// 返回协议定义的无符号消息类型值。
    pub const fn wire_value(self) -> u16 {
        self as u16
    }
}

/// Carries one decoded terminal-host protocol frame.
/// 承载一个已解码的终端宿主协议帧。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalHostFrame {
    /// Identifies the semantic message type.
    /// 标识消息的语义类型。
    pub kind: TerminalHostFrameKind,
    /// Orders messages emitted by the same protocol endpoint.
    /// 对同一协议端点发出的消息排序。
    pub sequence: u64,
    /// Contains JSON control data or raw terminal bytes according to `kind`.
    /// 根据 `kind` 包含 JSON 控制数据或原始终端字节。
    pub payload: Vec<u8>,
}

/// Validates one strictly contiguous endpoint sequence and advances the observed watermark.
/// 验证一个严格连续的端点序号，并推进已观察水位。
///
/// # Parameters / 参数
/// - `last_sequence`: Last accepted sequence from this endpoint.
/// - `received_sequence`: Sequence carried by the newly decoded frame.
/// - `last_sequence`：当前端点最后接受的序号。
/// - `received_sequence`：新解码数据帧携带的序号。
///
/// # Returns / 返回值
/// Returns success only when `received_sequence` is exactly the next valid sequence.
/// 仅当 `received_sequence` 恰好是下一个有效序号时返回成功。
pub fn validate_terminal_host_sequence(
    last_sequence: &mut u64,
    received_sequence: u64,
) -> io::Result<()> {
    let expected_sequence = last_sequence.checked_add(1).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "terminal-host sequence exhausted its valid range",
        )
    })?;
    if received_sequence != expected_sequence {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "terminal-host sequence mismatch: expected {expected_sequence}, received {received_sequence}"
            ),
        ));
    }
    *last_sequence = received_sequence;
    Ok(())
}

/// Describes the shell process that a terminal host must launch.
/// 描述终端宿主必须启动的 Shell 进程。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TerminalHostStartRequest {
    /// Contains the executable path or command name.
    /// 包含可执行文件路径或命令名。
    pub program: String,
    /// Contains ordered process arguments.
    /// 包含有序的进程参数。
    pub args: Vec<String>,
    /// Defines the child working directory.
    /// 定义子进程工作目录。
    pub cwd: PathBuf,
    /// Defines the initial terminal column count.
    /// 定义终端初始列数。
    pub cols: u16,
    /// Defines the initial terminal row count.
    /// 定义终端初始行数。
    pub rows: u16,
}

/// Describes a terminal character-grid resize request.
/// 描述终端字符网格调整请求。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TerminalHostResizeRequest {
    /// Defines the requested terminal column count.
    /// 定义请求的终端列数。
    pub cols: u16,
    /// Defines the requested terminal row count.
    /// 定义请求的终端行数。
    pub rows: u16,
}

/// Confirms the process identifier of a newly started terminal.
/// 确认新启动终端的进程标识符。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TerminalHostStartedEvent {
    /// Contains the child process identifier when the platform exposes it.
    /// 在平台可提供时包含子进程标识符。
    pub process_id: Option<u32>,
}

/// Describes the final exit state of the terminal child.
/// 描述终端子进程的最终退出状态。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TerminalHostExitedEvent {
    /// Contains the platform exit code when available.
    /// 在可用时包含平台退出码。
    pub exit_code: Option<u32>,
    /// Indicates whether the parent requested the stop.
    /// 指示是否由父进程请求停止。
    pub stopped_by_request: bool,
}

/// Describes a terminal-host error without mixing diagnostics into stdout.
/// 描述终端宿主错误，避免将诊断文本混入标准输出。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TerminalHostErrorEvent {
    /// Contains a stable machine-readable error category.
    /// 包含稳定的机器可读错误类别。
    pub code: String,
    /// Contains the human-readable diagnostic message.
    /// 包含人类可读的诊断消息。
    pub message: String,
}

/// Serializes a control payload as UTF-8 JSON.
/// 将控制负载序列化为 UTF-8 JSON。
///
/// # Parameters / 参数
/// - `value`: Serializable protocol payload.
/// - `value`：可序列化的协议负载。
///
/// # Returns / 返回值
/// Returns encoded JSON bytes, or `InvalidData` when serialization fails.
/// 返回编码后的 JSON 字节；序列化失败时返回 `InvalidData`。
pub fn encode_terminal_host_json<T: Serialize>(value: &T) -> io::Result<Vec<u8>> {
    serde_json::to_vec(value).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

/// Deserializes a UTF-8 JSON control payload.
/// 反序列化 UTF-8 JSON 控制负载。
///
/// # Parameters / 参数
/// - `payload`: JSON bytes from a validated frame.
/// - `payload`：来自已验证数据帧的 JSON 字节。
///
/// # Returns / 返回值
/// Returns the decoded value, or `InvalidData` when decoding fails.
/// 返回解码后的值；解码失败时返回 `InvalidData`。
pub fn decode_terminal_host_json<T: DeserializeOwned>(payload: &[u8]) -> io::Result<T> {
    serde_json::from_slice(payload)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

/// Reads one complete frame from a blocking stream.
/// 从阻塞流读取一个完整数据帧。
///
/// # Parameters / 参数
/// - `reader`: Blocking source that contains framed protocol bytes.
/// - `reader`：包含分帧协议字节的阻塞读取源。
///
/// # Returns / 返回值
/// Returns `None` on clean EOF before a new frame, otherwise one validated frame.
/// 在新帧开始前正常 EOF 时返回 `None`，否则返回一个已验证数据帧。
pub fn read_terminal_host_frame<R: Read>(reader: &mut R) -> io::Result<Option<TerminalHostFrame>> {
    let mut header = [0_u8; TERMINAL_HOST_FRAME_HEADER_LEN];
    let first_read = reader.read(&mut header[..1])?;
    if first_read == 0 {
        return Ok(None);
    }
    reader.read_exact(&mut header[1..])?;
    let (kind, sequence, payload_len) = decode_terminal_host_header(&header)?;
    let mut payload = vec![0_u8; payload_len];
    reader.read_exact(&mut payload)?;
    Ok(Some(TerminalHostFrame {
        kind,
        sequence,
        payload,
    }))
}

/// Writes one complete frame to a blocking stream and flushes it.
/// 向阻塞流写入一个完整数据帧并刷新。
///
/// # Parameters / 参数
/// - `writer`: Blocking destination for framed protocol bytes.
/// - `frame`: Validated frame to encode.
/// - `writer`：分帧协议字节的阻塞写入目标。
/// - `frame`：需要编码的已验证数据帧。
///
/// # Returns / 返回值
/// Returns success after header and payload are flushed.
/// 在帧头与负载完成刷新后返回成功。
pub fn write_terminal_host_frame<W: Write>(
    writer: &mut W,
    frame: &TerminalHostFrame,
) -> io::Result<()> {
    let header = encode_terminal_host_header(frame)?;
    writer.write_all(&header)?;
    writer.write_all(&frame.payload)?;
    writer.flush()
}

/// Reads one complete frame from an asynchronous stream.
/// 从异步流读取一个完整数据帧。
///
/// # Parameters / 参数
/// - `reader`: Asynchronous source that contains framed protocol bytes.
/// - `reader`：包含分帧协议字节的异步读取源。
///
/// # Returns / 返回值
/// Returns `None` on clean EOF before a new frame, otherwise one validated frame.
/// 在新帧开始前正常 EOF 时返回 `None`，否则返回一个已验证数据帧。
///
/// # Cancellation safety / 取消安全
/// This operation is not cancellation-safe after polling begins because a partial frame header
/// may already have been consumed. Keep the future alive until completion or run it in one
/// dedicated reader task that owns the stream.
/// 该操作在开始轮询后不具备取消安全性，因为帧头可能已被部分消费。必须保持 future 直到完成，或在一个独占该流的专用读取任务中运行。
pub async fn read_terminal_host_frame_async<R: AsyncRead + Unpin>(
    reader: &mut R,
) -> io::Result<Option<TerminalHostFrame>> {
    let mut header = [0_u8; TERMINAL_HOST_FRAME_HEADER_LEN];
    let first_read = reader.read(&mut header[..1]).await?;
    if first_read == 0 {
        return Ok(None);
    }
    reader.read_exact(&mut header[1..]).await?;
    let (kind, sequence, payload_len) = decode_terminal_host_header(&header)?;
    let mut payload = vec![0_u8; payload_len];
    reader.read_exact(&mut payload).await?;
    Ok(Some(TerminalHostFrame {
        kind,
        sequence,
        payload,
    }))
}

/// Writes one complete frame to an asynchronous stream and flushes it.
/// 向异步流写入一个完整数据帧并刷新。
///
/// # Parameters / 参数
/// - `writer`: Asynchronous destination for framed protocol bytes.
/// - `frame`: Validated frame to encode.
/// - `writer`：分帧协议字节的异步写入目标。
/// - `frame`：需要编码的已验证数据帧。
///
/// # Returns / 返回值
/// Returns success after header and payload are flushed.
/// 在帧头与负载完成刷新后返回成功。
pub async fn write_terminal_host_frame_async<W: AsyncWrite + Unpin>(
    writer: &mut W,
    frame: &TerminalHostFrame,
) -> io::Result<()> {
    let header = encode_terminal_host_header(frame)?;
    writer.write_all(&header).await?;
    writer.write_all(&frame.payload).await?;
    writer.flush().await
}

/// Encodes and validates the fixed terminal-host frame header.
/// 编码并验证固定的终端宿主帧头。
///
/// # Parameters / 参数
/// - `frame`: Frame whose metadata will be encoded.
/// - `frame`：需要编码元数据的数据帧。
///
/// # Returns / 返回值
/// Returns the fixed-size header, or `InvalidInput` for an oversized payload.
/// 返回固定长度帧头；负载过大时返回 `InvalidInput`。
fn encode_terminal_host_header(
    frame: &TerminalHostFrame,
) -> io::Result<[u8; TERMINAL_HOST_FRAME_HEADER_LEN]> {
    if frame.payload.len() > TERMINAL_HOST_MAX_PAYLOAD_LEN {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "terminal-host frame payload exceeds {} bytes",
                TERMINAL_HOST_MAX_PAYLOAD_LEN
            ),
        ));
    }
    let payload_len = u32::try_from(frame.payload.len()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "terminal-host frame payload length cannot be represented",
        )
    })?;
    let mut header = [0_u8; TERMINAL_HOST_FRAME_HEADER_LEN];
    header[0..4].copy_from_slice(&TERMINAL_HOST_FRAME_MAGIC);
    header[4..6].copy_from_slice(&1_u16.to_be_bytes());
    header[6..8].copy_from_slice(&frame.kind.wire_value().to_be_bytes());
    header[8..16].copy_from_slice(&frame.sequence.to_be_bytes());
    header[16..20].copy_from_slice(&payload_len.to_be_bytes());
    Ok(header)
}

/// Decodes and validates the fixed terminal-host frame header.
/// 解码并验证固定的终端宿主帧头。
///
/// # Parameters / 参数
/// - `header`: Complete fixed-size frame header.
/// - `header`：完整的固定长度帧头。
///
/// # Returns / 返回值
/// Returns message kind, sequence, and payload length after strict validation.
/// 严格验证后返回消息类型、序号和负载长度。
fn decode_terminal_host_header(
    header: &[u8; TERMINAL_HOST_FRAME_HEADER_LEN],
) -> io::Result<(TerminalHostFrameKind, u64, usize)> {
    if header[0..4] != TERMINAL_HOST_FRAME_MAGIC {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "terminal-host frame magic mismatch: received {:02X?}",
                &header[0..4]
            ),
        ));
    }
    let version = u16::from_be_bytes([header[4], header[5]]);
    if version != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unsupported terminal-host frame version: {version}"),
        ));
    }
    let kind = TerminalHostFrameKind::from_wire_value(u16::from_be_bytes([header[6], header[7]]))?;
    let sequence = u64::from_be_bytes(header[8..16].try_into().map_err(|_| {
        io::Error::new(io::ErrorKind::InvalidData, "invalid terminal-host sequence")
    })?);
    let payload_len = u32::from_be_bytes(header[16..20].try_into().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid terminal-host payload length",
        )
    })?) as usize;
    if payload_len > TERMINAL_HOST_MAX_PAYLOAD_LEN {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "terminal-host payload length {payload_len} exceeds {} bytes",
                TERMINAL_HOST_MAX_PAYLOAD_LEN
            ),
        ));
    }
    Ok((kind, sequence, payload_len))
}

#[cfg(test)]
mod tests {
    use super::{
        TERMINAL_HOST_FRAME_HEADER_LEN, TERMINAL_HOST_MAX_PAYLOAD_LEN, TerminalHostFrame,
        TerminalHostFrameKind, TerminalHostResizeRequest, decode_terminal_host_json,
        encode_terminal_host_json, read_terminal_host_frame, validate_terminal_host_sequence,
        write_terminal_host_frame,
    };
    use std::io::{Cursor, ErrorKind};

    /// Verifies a binary frame survives a blocking encode/decode round trip.
    /// 验证二进制数据帧经过阻塞编码与解码后保持一致。
    #[test]
    fn blocking_frame_round_trip_preserves_payload() {
        let frame = TerminalHostFrame {
            kind: TerminalHostFrameKind::InputBytes,
            sequence: 42,
            payload: vec![0, 1, 2, 255],
        };
        let mut bytes = Vec::new();
        write_terminal_host_frame(&mut bytes, &frame).expect("encode frame");
        let decoded = read_terminal_host_frame(&mut Cursor::new(bytes))
            .expect("decode frame")
            .expect("frame present");
        assert_eq!(decoded, frame);
    }

    /// Verifies JSON control messages use the stable camel-case contract.
    /// 验证 JSON 控制消息使用稳定的小驼峰契约。
    #[test]
    fn resize_json_round_trip_preserves_dimensions() {
        let request = TerminalHostResizeRequest {
            cols: 120,
            rows: 36,
        };
        let bytes = encode_terminal_host_json(&request).expect("encode resize");
        let decoded: TerminalHostResizeRequest =
            decode_terminal_host_json(&bytes).expect("decode resize");
        assert_eq!(decoded, request);
    }

    /// Verifies endpoint sequences accept only the exact next positive value.
    /// 验证端点序号只接受精确的下一个正整数值。
    #[test]
    fn sequence_validation_rejects_duplicates_and_gaps() {
        let mut last_sequence = 0;
        validate_terminal_host_sequence(&mut last_sequence, 1).expect("first sequence");
        let duplicate = validate_terminal_host_sequence(&mut last_sequence, 1)
            .expect_err("duplicate sequence must fail");
        assert_eq!(duplicate.kind(), ErrorKind::InvalidData);
        let gap = validate_terminal_host_sequence(&mut last_sequence, 3)
            .expect_err("sequence gap must fail");
        assert_eq!(gap.kind(), ErrorKind::InvalidData);
        validate_terminal_host_sequence(&mut last_sequence, 2).expect("next sequence");
    }

    /// Verifies unknown frame kinds fail closed before payload allocation.
    /// 验证未知帧类型会在分配负载前封闭失败。
    #[test]
    fn unknown_frame_kind_is_rejected() {
        let mut bytes = encoded_empty_frame();
        bytes[6..8].copy_from_slice(&999_u16.to_be_bytes());
        let error =
            read_terminal_host_frame(&mut Cursor::new(bytes)).expect_err("unknown kind must fail");
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    /// Verifies protocol-version mismatch fails closed.
    /// 验证协议版本不匹配时会封闭失败。
    #[test]
    fn unsupported_frame_version_is_rejected() {
        let mut bytes = encoded_empty_frame();
        bytes[4..6].copy_from_slice(&2_u16.to_be_bytes());
        let error = read_terminal_host_frame(&mut Cursor::new(bytes))
            .expect_err("unsupported version must fail");
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    /// Verifies oversized outgoing and incoming payload declarations are rejected.
    /// 验证超大出站负载与入站负载声明都会被拒绝。
    #[test]
    fn oversized_payload_is_rejected_in_both_directions() {
        let outgoing = TerminalHostFrame {
            kind: TerminalHostFrameKind::InputBytes,
            sequence: 1,
            payload: vec![0; TERMINAL_HOST_MAX_PAYLOAD_LEN + 1],
        };
        let error = write_terminal_host_frame(&mut Vec::new(), &outgoing)
            .expect_err("oversized outgoing payload must fail");
        assert_eq!(error.kind(), ErrorKind::InvalidInput);

        let mut incoming = encoded_empty_frame();
        incoming[16..20]
            .copy_from_slice(&((TERMINAL_HOST_MAX_PAYLOAD_LEN + 1) as u32).to_be_bytes());
        let error = read_terminal_host_frame(&mut Cursor::new(incoming))
            .expect_err("oversized incoming payload must fail");
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    /// Verifies truncated headers and payloads are never accepted as complete frames.
    /// 验证截断的帧头与负载绝不会被接受为完整帧。
    #[test]
    fn truncated_frame_is_rejected() {
        let full_header = encoded_empty_frame();
        let error = read_terminal_host_frame(&mut Cursor::new(
            full_header[..TERMINAL_HOST_FRAME_HEADER_LEN - 1].to_vec(),
        ))
        .expect_err("truncated header must fail");
        assert_eq!(error.kind(), ErrorKind::UnexpectedEof);

        let frame = TerminalHostFrame {
            kind: TerminalHostFrameKind::OutputBytes,
            sequence: 1,
            payload: vec![1, 2, 3],
        };
        let mut bytes = Vec::new();
        write_terminal_host_frame(&mut bytes, &frame).expect("encode payload frame");
        bytes.pop();
        let error = read_terminal_host_frame(&mut Cursor::new(bytes))
            .expect_err("truncated payload must fail");
        assert_eq!(error.kind(), ErrorKind::UnexpectedEof);
    }

    /// Verifies concatenated frames remain independently decodable in wire order.
    /// 验证拼接帧仍可按线序独立解码。
    #[test]
    fn concatenated_frames_decode_in_order() {
        let first = TerminalHostFrame {
            kind: TerminalHostFrameKind::InputBytes,
            sequence: 1,
            payload: vec![7],
        };
        let second = TerminalHostFrame {
            kind: TerminalHostFrameKind::OutputBytes,
            sequence: 2,
            payload: vec![7],
        };
        let mut bytes = Vec::new();
        write_terminal_host_frame(&mut bytes, &first).expect("encode first frame");
        write_terminal_host_frame(&mut bytes, &second).expect("encode second frame");
        let mut reader = Cursor::new(bytes);
        assert_eq!(
            read_terminal_host_frame(&mut reader).expect("read first"),
            Some(first)
        );
        assert_eq!(
            read_terminal_host_frame(&mut reader).expect("read second"),
            Some(second)
        );
        assert_eq!(
            read_terminal_host_frame(&mut reader).expect("read eof"),
            None
        );
    }

    /// Builds one valid empty frame used by malformed-header tests.
    /// 构建一个供畸形帧头测试使用的有效空帧。
    ///
    /// # Returns / 返回值
    /// Returns the complete wire bytes for an empty stop frame.
    /// 返回空 Stop 帧的完整线级字节。
    fn encoded_empty_frame() -> Vec<u8> {
        let frame = TerminalHostFrame {
            kind: TerminalHostFrameKind::Stop,
            sequence: 1,
            payload: Vec::new(),
        };
        let mut bytes = Vec::new();
        write_terminal_host_frame(&mut bytes, &frame).expect("encode empty frame");
        bytes
    }

    /// Legacy heartbeat wire values should be rejected because process ownership is authoritative for liveness.
    /// 旧心跳线值应被拒绝，因为进程所有权才是存活状态的权威来源。
    #[test]
    fn legacy_heartbeat_wire_values_are_rejected() {
        for legacy_value in [5_u16, 106_u16] {
            let error = TerminalHostFrameKind::from_wire_value(legacy_value)
                .expect_err("legacy heartbeat value must fail");
            assert_eq!(error.kind(), ErrorKind::InvalidData);
        }
    }
}
