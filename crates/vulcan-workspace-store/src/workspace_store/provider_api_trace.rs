//! Opt-in Provider API debug traces for session persistence V1.
//! 会话持久化 V1 的可选 Provider API 调试追踪。
//!
//! A trace is a diagnostic sidecar, never a session fact. Every physical HTTP
//! attempt reserves one durable monotonic sequence before dispatch and later
//! publishes one complete redacted input/output pair into a fixed ten-slot ring.
//! 追踪只是诊断旁路，绝不是会话事实。每次真实 HTTP attempt 在派发前预留一个持久化单调序号，
//! 随后把一对完整脱敏 input/output 发布到固定十槽环中。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use vulcan_session_core::v1::{ApiCallId, JS_SAFE_INTEGER_MAX};

use crate::workspace_store::session_v1::atomic_io::write_file_atomic;

/// Fixed English replacement for every Provider token or secret.
/// 每个 Provider token 或密钥使用的固定英文替代串。
pub const REDACTED_PROVIDER_SECRET: &str = "[REDACTED PROVIDER SECRET]";

/// Fixed number of complete physical attempts retained per trace store.
/// 每个追踪存储固定保留的完整物理 attempt 数量。
pub const PROVIDER_API_TRACE_LIMIT: u64 = 10;

/// Persisted trace schema version.
/// 持久化追踪结构版本。
const PROVIDER_API_TRACE_SCHEMA_VERSION: u32 = 1;

/// Stable machine-readable category of one non-fatal trace diagnostic.
/// 一条非致命追踪诊断的稳定机器可读类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderApiTraceDiagnosticCode {
    /// The durable sequence cannot advance within the JSON-safe range.
    /// 持久化序号无法在 JSON 安全范围内继续推进。
    TraceSequenceExhausted,
    /// Existing trace metadata or payload is incomplete or invalid.
    /// 已有追踪元数据或载荷不完整或无效。
    CorruptSlot,
    /// One sequence was reused with another API or attempt identity.
    /// 同一序号被另一 API 或 attempt 身份复用。
    IdentityConflict,
    /// One identity was retried with different redacted payload bytes.
    /// 同一身份被使用不同的脱敏载荷字节重试。
    ContentMismatch,
    /// A delayed completion attempted to overwrite a newer ring occupant.
    /// 延迟完成试图覆盖更晚的环槽记录。
    SupersededAttempt,
    /// Trace serialization, locking, or filesystem IO failed.
    /// 追踪序列化、加锁或文件系统 IO 失败。
    WriteFailed,
}

impl ProviderApiTraceDiagnosticCode {
    /// Returns the stable lowercase diagnostic code.
    /// 返回稳定的小写诊断码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TraceSequenceExhausted => "trace_sequence_exhausted",
            Self::CorruptSlot => "corrupt_slot",
            Self::IdentityConflict => "identity_conflict",
            Self::ContentMismatch => "content_mismatch",
            Self::SupersededAttempt => "superseded_attempt",
            Self::WriteFailed => "write_failed",
        }
    }
}

/// One non-fatal Provider trace diagnostic.
/// 一条非致命 Provider 追踪诊断。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderApiTraceDiagnostic {
    /// Stable diagnostic code.
    /// 稳定诊断码。
    pub code: ProviderApiTraceDiagnosticCode,
    /// Human-readable diagnostic detail.
    /// 人类可读诊断详情。
    pub detail: String,
}

/// Durable identity reserved for one physical HTTP attempt.
/// 为一次真实 HTTP attempt 预留的持久化身份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderApiTraceAttemptReservation {
    /// Monotonic sequence reserved before physical dispatch.
    /// 在物理派发前预留的单调序号。
    pub trace_sequence: u64,
    /// V1 logical API call containing this transport attempt.
    /// 包含该传输 attempt 的 V1 逻辑 API 调用。
    pub api_call_id: ApiCallId,
    /// Zero-based transport retry attempt index.
    /// 从零开始的传输重试 attempt 下标。
    pub attempt_index: u32,
}

/// Outcome of reserving one physical attempt sequence.
/// 预留一次物理 attempt 序号的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderApiTraceReservationOutcome {
    /// The switch is disabled and no filesystem path was touched.
    /// 开关已关闭且未接触任何文件系统路径。
    Disabled,
    /// One sequence was durably reserved before dispatch.
    /// 一个序号已在派发前可靠预留。
    Reserved(ProviderApiTraceAttemptReservation),
    /// Reservation failed without affecting the Provider main path.
    /// 预留失败但不影响 Provider 主链。
    Diagnostic(ProviderApiTraceDiagnostic),
}

/// Borrowed complete payload of one finished physical attempt.
/// 一次已结束物理 attempt 的借用完整载荷。
pub struct ProviderApiTracePayload<'a> {
    /// Complete Provider input after final wire lowering.
    /// 最终 wire lowering 后的完整 Provider 输入。
    pub input: &'a str,
    /// Complete Provider response or structured transport failure.
    /// 完整 Provider 响应或结构化传输失败。
    pub output: &'a str,
    /// Exact caller-known secrets requiring literal removal.
    /// 调用方明确知道且必须按字面移除的密钥。
    pub secrets: &'a [String],
}

/// Outcome of publishing one complete physical attempt.
/// 发布一次完整物理 attempt 的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderApiTraceWriteOutcome {
    /// The switch is disabled and no filesystem path was touched.
    /// 开关已关闭且未接触任何文件系统路径。
    Disabled,
    /// The complete pair is durable in the reported ring slot.
    /// 完整输入输出对已持久化到所报告的环槽。
    Written {
        /// Deterministic zero-based ring slot.
        /// 确定性的零基环槽。
        slot: u64,
    },
    /// Publication failed without affecting session or Provider semantics.
    /// 发布失败但不影响会话或 Provider 语义。
    Diagnostic(ProviderApiTraceDiagnostic),
}

/// Persisted sequence frontier reserved before dispatch.
/// 在派发前持久化的序号前沿。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProviderApiTraceSequenceDocument {
    /// Trace schema version.
    /// 追踪结构版本。
    schema_version: u32,
    /// Greatest sequence already reserved for dispatch.
    /// 已为派发预留的最大序号。
    last_trace_sequence: u64,
}

/// Committed pointer selecting one complete payload bank.
/// 选择一个完整载荷银行的已提交指针。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProviderApiTraceSlotPointer {
    /// Trace schema version.
    /// 追踪结构版本。
    schema_version: u32,
    /// Monotonic physical attempt sequence.
    /// 单调物理 attempt 序号。
    trace_sequence: u64,
    /// V1 logical API call identity.
    /// V1 逻辑 API 调用身份。
    api_call_id: ApiCallId,
    /// Transport retry attempt index.
    /// 传输重试 attempt 下标。
    attempt_index: u32,
    /// Committed payload bank, either zero or one.
    /// 已提交载荷银行，只能为零或一。
    payload_bank: u8,
    /// Lowercase SHA-256 of the committed redacted input bytes.
    /// 已提交脱敏输入字节的小写 SHA-256。
    input_sha256: String,
    /// Lowercase SHA-256 of the committed redacted output bytes.
    /// 已提交脱敏输出字节的小写 SHA-256。
    output_sha256: String,
}

impl ProviderApiTraceSlotPointer {
    /// Builds a committed pointer from one reservation and bank.
    /// 从一次预留及银行构建已提交指针。
    fn from_reservation(
        reservation: &ProviderApiTraceAttemptReservation,
        payload_bank: u8,
        input_sha256: String,
        output_sha256: String,
    ) -> Self {
        Self {
            schema_version: PROVIDER_API_TRACE_SCHEMA_VERSION,
            trace_sequence: reservation.trace_sequence,
            api_call_id: reservation.api_call_id,
            attempt_index: reservation.attempt_index,
            payload_bank,
            input_sha256,
            output_sha256,
        }
    }

    /// Returns whether the pointer identifies the exact reservation.
    /// 返回指针是否标识完全相同的预留。
    fn identifies(&self, reservation: &ProviderApiTraceAttemptReservation) -> bool {
        self.trace_sequence == reservation.trace_sequence
            && self.api_call_id == reservation.api_call_id
            && self.attempt_index == reservation.attempt_index
    }
}

/// Shared process-local synchronization for one cloned trace owner.
/// 一个克隆追踪 owner 的共享进程内同步状态。
#[derive(Debug)]
struct ProviderApiTraceStoreState {
    /// One owner lock serializing reserve, publish, prepare, and clear operations.
    /// 串行化预留、发布、准备和清理操作的唯一 owner 锁。
    owner: Mutex<ProviderApiTraceOwnerState>,
}

/// Mutable state guarded by one trace-store owner lock.
/// 由单个追踪存储 owner 锁保护的可变状态。
#[derive(Debug, Default)]
struct ProviderApiTraceOwnerState {
    /// Lazily loaded durable reservation frontier.
    /// 延迟读取的持久化预留前沿。
    sequence_frontier: Option<u64>,
}

/// One trace store rooted at a session's `temp/provider-api-trace` directory.
/// 以会话 `temp/provider-api-trace` 目录为根的追踪存储。
#[derive(Debug, Clone)]
pub struct ProviderApiTraceStore {
    /// Absolute trace root directory.
    /// 绝对追踪根目录。
    root: PathBuf,
    /// Immutable opt-in switch captured at construction.
    /// 构建时捕获的不可变可选开关。
    enabled: bool,
    /// Synchronization shared by every clone of this owner.
    /// 该 owner 所有克隆共享的同步状态。
    state: Arc<ProviderApiTraceStoreState>,
}

impl ProviderApiTraceStore {
    /// Creates one trace store without touching its configured path.
    /// 创建一个追踪存储且不接触其配置路径。
    pub fn new(root: PathBuf, enabled: bool) -> Self {
        Self {
            root,
            enabled,
            state: Arc::new(ProviderApiTraceStoreState {
                owner: Mutex::new(ProviderApiTraceOwnerState::default()),
            }),
        }
    }

    /// Creates the exact trace root for the user-facing open-directory action.
    /// 为面向用户的打开目录操作创建精确追踪根目录。
    pub fn prepare_directory(&self) -> Result<PathBuf, ProviderApiTraceDiagnostic> {
        let _owner = self.lock_owner()?;
        ensure_trace_root(&self.root)?;
        Ok(self.root.clone())
    }

    /// Removes the exact trace root and resets the in-memory sequence frontier.
    /// 删除精确追踪根目录并重置内存序号前沿。
    pub fn clear_directory(&self) -> Result<bool, ProviderApiTraceDiagnostic> {
        let mut owner = self.lock_owner()?;
        let parent = self.root.parent().ok_or_else(|| {
            trace_diagnostic(
                ProviderApiTraceDiagnosticCode::WriteFailed,
                format!(
                    "provider trace root {} has no parent directory",
                    self.root.display()
                ),
            )
        })?;
        match fs::symlink_metadata(parent) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Ok(_) => {
                return Err(trace_diagnostic(
                    ProviderApiTraceDiagnosticCode::WriteFailed,
                    format!(
                        "provider trace parent {} is not a regular directory",
                        parent.display()
                    ),
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                owner.sequence_frontier = None;
                return Ok(false);
            }
            Err(error) => {
                return Err(trace_diagnostic(
                    ProviderApiTraceDiagnosticCode::WriteFailed,
                    format!(
                        "inspect provider trace parent {} failed: {error}",
                        parent.display()
                    ),
                ));
            }
        }
        let metadata = match fs::symlink_metadata(&self.root) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                owner.sequence_frontier = None;
                return Ok(false);
            }
            Err(error) => {
                return Err(trace_diagnostic(
                    ProviderApiTraceDiagnosticCode::WriteFailed,
                    format!(
                        "inspect provider trace root {} failed: {error}",
                        self.root.display()
                    ),
                ));
            }
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(trace_diagnostic(
                ProviderApiTraceDiagnosticCode::WriteFailed,
                format!(
                    "provider trace root {} is not a regular directory",
                    self.root.display()
                ),
            ));
        }
        fs::remove_dir_all(&self.root).map_err(|error| {
            trace_diagnostic(
                ProviderApiTraceDiagnosticCode::WriteFailed,
                format!(
                    "remove provider trace root {} failed: {error}",
                    self.root.display()
                ),
            )
        })?;
        owner.sequence_frontier = None;
        Ok(true)
    }

    /// Durably reserves one sequence before the physical request is dispatched.
    /// 在真实请求派发前可靠预留一个序号。
    pub fn reserve_attempt(
        &self,
        api_call_id: ApiCallId,
        attempt_index: u32,
    ) -> ProviderApiTraceReservationOutcome {
        if !self.enabled {
            return ProviderApiTraceReservationOutcome::Disabled;
        }
        let mut owner = match self.lock_owner() {
            Ok(owner) => owner,
            Err(error) => return ProviderApiTraceReservationOutcome::Diagnostic(error),
        };
        if let Err(error) = ensure_trace_root(&self.root) {
            return ProviderApiTraceReservationOutcome::Diagnostic(error);
        }
        let current = match owner.sequence_frontier {
            Some(current) => current,
            None => match self.load_durable_sequence_frontier() {
                Ok(current) => {
                    owner.sequence_frontier = Some(current);
                    current
                }
                Err(error) => return ProviderApiTraceReservationOutcome::Diagnostic(error),
            },
        };
        let Some(trace_sequence) = current.checked_add(1) else {
            return reservation_diagnostic(
                ProviderApiTraceDiagnosticCode::TraceSequenceExhausted,
                "trace sequence overflowed u64".to_string(),
            );
        };
        if trace_sequence > JS_SAFE_INTEGER_MAX {
            return reservation_diagnostic(
                ProviderApiTraceDiagnosticCode::TraceSequenceExhausted,
                format!("trace sequence {trace_sequence} exceeds the JSON-safe range"),
            );
        }
        let sequence_document = ProviderApiTraceSequenceDocument {
            schema_version: PROVIDER_API_TRACE_SCHEMA_VERSION,
            last_trace_sequence: trace_sequence,
        };
        if let Err(error) = write_json_atomic(
            &self.root.join("sequence.json"),
            &sequence_document,
            "provider trace sequence",
        ) {
            return reservation_diagnostic(ProviderApiTraceDiagnosticCode::WriteFailed, error);
        }
        owner.sequence_frontier = Some(trace_sequence);
        ProviderApiTraceReservationOutcome::Reserved(ProviderApiTraceAttemptReservation {
            trace_sequence,
            api_call_id,
            attempt_index,
        })
    }

    /// Publishes one complete redacted request/response pair into its ring slot.
    /// 把一对完整脱敏请求/响应发布到对应环槽。
    pub fn publish_attempt(
        &self,
        reservation: &ProviderApiTraceAttemptReservation,
        payload: ProviderApiTracePayload<'_>,
    ) -> ProviderApiTraceWriteOutcome {
        if !self.enabled {
            return ProviderApiTraceWriteOutcome::Disabled;
        }
        let _owner = match self.lock_owner() {
            Ok(owner) => owner,
            Err(error) => return ProviderApiTraceWriteOutcome::Diagnostic(error),
        };
        if let Err(error) = ensure_trace_root(&self.root) {
            return ProviderApiTraceWriteOutcome::Diagnostic(error);
        }
        if !(1..=JS_SAFE_INTEGER_MAX).contains(&reservation.trace_sequence) {
            return write_diagnostic(
                ProviderApiTraceDiagnosticCode::TraceSequenceExhausted,
                format!(
                    "trace sequence {} is outside the JSON-safe range",
                    reservation.trace_sequence
                ),
            );
        }
        let slot = (reservation.trace_sequence - 1) % PROVIDER_API_TRACE_LIMIT;
        let slot_dir = self.root.join(format!("slot-{slot:02}"));
        let existing = match read_slot_pointer(&slot_dir) {
            Ok(existing) => existing,
            Err(error) => return ProviderApiTraceWriteOutcome::Diagnostic(error),
        };
        let redacted_input = redact_provider_secrets(payload.input, payload.secrets);
        let redacted_output = redact_provider_secrets(payload.output, payload.secrets);
        let input_sha256 = sha256_hex(redacted_input.as_bytes());
        let output_sha256 = sha256_hex(redacted_output.as_bytes());
        if let Some(existing) = existing.as_ref() {
            if existing.trace_sequence > reservation.trace_sequence {
                return write_diagnostic(
                    ProviderApiTraceDiagnosticCode::SupersededAttempt,
                    format!(
                        "trace sequence {} cannot replace newer sequence {} in slot {slot}",
                        reservation.trace_sequence, existing.trace_sequence
                    ),
                );
            }
            if existing.trace_sequence == reservation.trace_sequence {
                if !existing.identifies(reservation) {
                    return write_diagnostic(
                        ProviderApiTraceDiagnosticCode::IdentityConflict,
                        format!(
                            "trace sequence {} is already bound to api_call_id {} attempt {}, attempted api_call_id {} attempt {}",
                            reservation.trace_sequence,
                            existing.api_call_id.value(),
                            existing.attempt_index,
                            reservation.api_call_id.value(),
                            reservation.attempt_index
                        ),
                    );
                }
                return match compare_committed_payload(
                    &slot_dir,
                    existing,
                    redacted_input.as_bytes(),
                    redacted_output.as_bytes(),
                    &input_sha256,
                    &output_sha256,
                ) {
                    Ok(true) => ProviderApiTraceWriteOutcome::Written { slot },
                    Ok(false) => write_diagnostic(
                        ProviderApiTraceDiagnosticCode::ContentMismatch,
                        format!(
                            "trace sequence {} was retried with different redacted payload bytes",
                            reservation.trace_sequence
                        ),
                    ),
                    Err(error) => ProviderApiTraceWriteOutcome::Diagnostic(error),
                };
            }
        }
        let payload_bank = existing
            .as_ref()
            .map_or(0, |pointer| 1_u8.saturating_sub(pointer.payload_bank));
        let pointer = ProviderApiTraceSlotPointer::from_reservation(
            reservation,
            payload_bank,
            input_sha256,
            output_sha256,
        );
        if let Err(error) = publish_trace_slot(
            &slot_dir,
            &pointer,
            redacted_input.as_bytes(),
            redacted_output.as_bytes(),
        ) {
            return write_diagnostic(ProviderApiTraceDiagnosticCode::WriteFailed, error);
        }
        if let Some(previous) = existing {
            let previous_bank = slot_dir.join(format!("payload-{}", previous.payload_bank));
            let _cleanup_result = fs::remove_dir_all(previous_bank);
        }
        ProviderApiTraceWriteOutcome::Written { slot }
    }

    /// Returns the trace root without creating it.
    /// 返回追踪根路径且不创建它。
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns whether trace publication is enabled.
    /// 返回是否启用追踪发布。
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Locks the sole process-local owner state for this trace store.
    /// 锁定该追踪存储唯一的进程内 owner 状态。
    fn lock_owner(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, ProviderApiTraceOwnerState>, ProviderApiTraceDiagnostic>
    {
        self.state.owner.lock().map_err(|error| {
            trace_diagnostic(
                ProviderApiTraceDiagnosticCode::WriteFailed,
                format!("lock provider trace owner failed: {error}"),
            )
        })
    }

    /// Loads the reservation frontier from one exact file plus ten fixed slots.
    /// 从一个精确文件和十个固定槽加载预留前沿。
    fn load_durable_sequence_frontier(&self) -> Result<u64, ProviderApiTraceDiagnostic> {
        let mut maximum = match read_sequence_document(&self.root.join("sequence.json"))? {
            Some(document) => document.last_trace_sequence,
            None => 0,
        };
        for slot in 0..PROVIDER_API_TRACE_LIMIT {
            let slot_dir = self.root.join(format!("slot-{slot:02}"));
            if let Some(pointer) = read_slot_pointer(&slot_dir)? {
                maximum = maximum.max(pointer.trace_sequence);
            }
        }
        Ok(maximum)
    }
}

/// Creates or validates one exact non-symlink trace root.
/// 创建或验证一个精确的非符号链接追踪根目录。
fn ensure_trace_root(root: &Path) -> Result<(), ProviderApiTraceDiagnostic> {
    let parent = root.parent().ok_or_else(|| {
        trace_diagnostic(
            ProviderApiTraceDiagnosticCode::WriteFailed,
            format!(
                "provider trace root {} has no parent directory",
                root.display()
            ),
        )
    })?;
    ensure_trace_parent_directory(parent)?;
    match fs::symlink_metadata(root) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(trace_diagnostic(
                    ProviderApiTraceDiagnosticCode::WriteFailed,
                    format!(
                        "provider trace root {} is not a regular directory",
                        root.display()
                    ),
                ));
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(root).map_err(|create_error| {
                trace_diagnostic(
                    ProviderApiTraceDiagnosticCode::WriteFailed,
                    format!(
                        "create provider trace root {} failed: {create_error}",
                        root.display()
                    ),
                )
            })?;
            let metadata = fs::symlink_metadata(root).map_err(|inspect_error| {
                trace_diagnostic(
                    ProviderApiTraceDiagnosticCode::WriteFailed,
                    format!(
                        "inspect created provider trace root {} failed: {inspect_error}",
                        root.display()
                    ),
                )
            })?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(trace_diagnostic(
                    ProviderApiTraceDiagnosticCode::WriteFailed,
                    format!(
                        "created provider trace root {} is not a regular directory",
                        root.display()
                    ),
                ));
            }
            Ok(())
        }
        Err(error) => Err(trace_diagnostic(
            ProviderApiTraceDiagnosticCode::WriteFailed,
            format!(
                "inspect provider trace root {} failed: {error}",
                root.display()
            ),
        )),
    }
}

/// Creates or validates the immediate non-symlink parent of the trace root.
/// 创建或验证追踪根目录的直接非符号链接父目录。
fn ensure_trace_parent_directory(parent: &Path) -> Result<(), ProviderApiTraceDiagnostic> {
    match fs::symlink_metadata(parent) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(trace_diagnostic(
                    ProviderApiTraceDiagnosticCode::WriteFailed,
                    format!(
                        "provider trace parent {} is not a regular directory",
                        parent.display()
                    ),
                ));
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(parent).map_err(|create_error| {
                trace_diagnostic(
                    ProviderApiTraceDiagnosticCode::WriteFailed,
                    format!(
                        "create provider trace parent {} failed: {create_error}",
                        parent.display()
                    ),
                )
            })?;
            Ok(())
        }
        Err(error) => Err(trace_diagnostic(
            ProviderApiTraceDiagnosticCode::WriteFailed,
            format!(
                "inspect provider trace parent {} failed: {error}",
                parent.display()
            ),
        )),
    }
}

/// Publishes an inactive payload bank first and commits its pointer last.
/// 先发布非活动载荷银行，最后提交其指针。
fn publish_trace_slot(
    slot_dir: &Path,
    pointer: &ProviderApiTraceSlotPointer,
    input: &[u8],
    output: &[u8],
) -> Result<(), String> {
    let payload_dir = slot_dir.join(format!("payload-{}", pointer.payload_bank));
    write_file_atomic(
        &payload_dir.join("input.jsonl"),
        input,
        "provider trace input",
    )
    .map_err(|error| error.to_string())?;
    write_file_atomic(
        &payload_dir.join("output.jsonl"),
        output,
        "provider trace output",
    )
    .map_err(|error| error.to_string())?;
    write_json_atomic(
        &payload_dir.join("identity.json"),
        pointer,
        "provider trace payload identity",
    )?;
    write_json_atomic(
        &slot_dir.join("identity.json"),
        pointer,
        "provider trace committed pointer",
    )
}

/// Compares committed payload bytes for exact idempotency.
/// 比较已提交载荷字节以实现精确幂等。
fn compare_committed_payload(
    slot_dir: &Path,
    pointer: &ProviderApiTraceSlotPointer,
    expected_input: &[u8],
    expected_output: &[u8],
    expected_input_sha256: &str,
    expected_output_sha256: &str,
) -> Result<bool, ProviderApiTraceDiagnostic> {
    if pointer.input_sha256 != expected_input_sha256
        || pointer.output_sha256 != expected_output_sha256
    {
        return Ok(false);
    }
    let payload_dir = slot_dir.join(format!("payload-{}", pointer.payload_bank));
    let payload_identity = read_required_json::<ProviderApiTraceSlotPointer>(
        &payload_dir.join("identity.json"),
        "provider trace payload identity",
    )?;
    if payload_identity != *pointer {
        return Err(trace_diagnostic(
            ProviderApiTraceDiagnosticCode::CorruptSlot,
            format!(
                "provider trace payload identity in {} does not match committed pointer",
                payload_dir.display()
            ),
        ));
    }
    let actual_input = read_required_file(&payload_dir.join("input.jsonl"))?;
    let actual_output = read_required_file(&payload_dir.join("output.jsonl"))?;
    if sha256_hex(&actual_input) != pointer.input_sha256
        || sha256_hex(&actual_output) != pointer.output_sha256
    {
        return Err(trace_diagnostic(
            ProviderApiTraceDiagnosticCode::CorruptSlot,
            format!(
                "provider trace payload hashes in {} do not match committed pointer",
                payload_dir.display()
            ),
        ));
    }
    Ok(actual_input == expected_input && actual_output == expected_output)
}

/// Reads the optional exact sequence frontier document.
/// 读取可选的精确序号前沿文档。
fn read_sequence_document(
    path: &Path,
) -> Result<Option<ProviderApiTraceSequenceDocument>, ProviderApiTraceDiagnostic> {
    let document: Option<ProviderApiTraceSequenceDocument> =
        read_optional_json(path, "provider trace sequence")?;
    let Some(document) = document else {
        return Ok(None);
    };
    if document.schema_version != PROVIDER_API_TRACE_SCHEMA_VERSION
        || document.last_trace_sequence > JS_SAFE_INTEGER_MAX
    {
        return Err(trace_diagnostic(
            ProviderApiTraceDiagnosticCode::CorruptSlot,
            format!(
                "provider trace sequence document {} is invalid",
                path.display()
            ),
        ));
    }
    Ok(Some(document))
}

/// Reads and validates one optional committed slot pointer.
/// 读取并验证一个可选的已提交槽指针。
fn read_slot_pointer(
    slot_dir: &Path,
) -> Result<Option<ProviderApiTraceSlotPointer>, ProviderApiTraceDiagnostic> {
    let pointer_path = slot_dir.join("identity.json");
    let pointer: Option<ProviderApiTraceSlotPointer> =
        read_optional_json(&pointer_path, "provider trace slot pointer")?;
    let Some(pointer) = pointer else {
        return Ok(None);
    };
    if pointer.schema_version != PROVIDER_API_TRACE_SCHEMA_VERSION
        || !(1..=JS_SAFE_INTEGER_MAX).contains(&pointer.trace_sequence)
        || pointer.payload_bank > 1
        || !is_lowercase_sha256(&pointer.input_sha256)
        || !is_lowercase_sha256(&pointer.output_sha256)
    {
        return Err(trace_diagnostic(
            ProviderApiTraceDiagnosticCode::CorruptSlot,
            format!(
                "provider trace slot pointer {} is invalid",
                pointer_path.display()
            ),
        ));
    }
    Ok(Some(pointer))
}

/// Returns whether a string is one canonical lowercase SHA-256 digest.
/// 返回字符串是否为一个规范的小写 SHA-256 摘要。
fn is_lowercase_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
}

/// Computes one canonical lowercase SHA-256 digest.
/// 计算一个规范的小写 SHA-256 摘要。
fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Reads one optional JSON document without scanning sibling paths.
/// 读取一个可选 JSON 文档且不扫描同级路径。
fn read_optional_json<T>(
    path: &Path,
    artifact_kind: &str,
) -> Result<Option<T>, ProviderApiTraceDiagnostic>
where
    T: for<'de> Deserialize<'de>,
{
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(|error| {
            trace_diagnostic(
                ProviderApiTraceDiagnosticCode::CorruptSlot,
                format!("parse {artifact_kind} {} failed: {error}", path.display()),
            )
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(trace_diagnostic(
            ProviderApiTraceDiagnosticCode::CorruptSlot,
            format!("read {artifact_kind} {} failed: {error}", path.display()),
        )),
    }
}

/// Reads one required JSON document.
/// 读取一个必需 JSON 文档。
fn read_required_json<T>(path: &Path, artifact_kind: &str) -> Result<T, ProviderApiTraceDiagnostic>
where
    T: for<'de> Deserialize<'de>,
{
    read_optional_json(path, artifact_kind)?.ok_or_else(|| {
        trace_diagnostic(
            ProviderApiTraceDiagnosticCode::CorruptSlot,
            format!("required {artifact_kind} {} is missing", path.display()),
        )
    })
}

/// Reads one required regular payload file.
/// 读取一个必需的普通载荷文件。
fn read_required_file(path: &Path) -> Result<Vec<u8>, ProviderApiTraceDiagnostic> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        trace_diagnostic(
            ProviderApiTraceDiagnosticCode::CorruptSlot,
            format!(
                "inspect provider trace payload {} failed: {error}",
                path.display()
            ),
        )
    })?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(trace_diagnostic(
            ProviderApiTraceDiagnosticCode::CorruptSlot,
            format!(
                "provider trace payload {} is not a regular file",
                path.display()
            ),
        ));
    }
    fs::read(path).map_err(|error| {
        trace_diagnostic(
            ProviderApiTraceDiagnosticCode::CorruptSlot,
            format!(
                "read provider trace payload {} failed: {error}",
                path.display()
            ),
        )
    })
}

/// Serializes and atomically publishes one JSON document.
/// 序列化并原子发布一个 JSON 文档。
fn write_json_atomic<T>(path: &Path, value: &T, artifact_kind: &str) -> Result<(), String>
where
    T: Serialize,
{
    let encoded = serde_json::to_vec(value)
        .map_err(|error| format!("serialize {artifact_kind} failed: {error}"))?;
    write_file_atomic(path, &encoded, artifact_kind).map_err(|error| error.to_string())
}

/// Replaces caller-known secrets and sensitive JSON fields with one marker.
/// 使用同一标记替换调用方已知密钥和敏感 JSON 字段。
pub fn redact_provider_secrets(input: &str, secrets: &[String]) -> String {
    let mut literal_redacted = input.to_string();
    for secret in secrets.iter().filter(|secret| !secret.is_empty()) {
        literal_redacted = literal_redacted.replace(secret, REDACTED_PROVIDER_SECRET);
    }
    if let Ok(mut value) = serde_json::from_str::<Value>(&literal_redacted) {
        redact_json_value(&mut value);
        return serde_json::to_string(&value).unwrap_or(literal_redacted);
    }
    let mut output = String::with_capacity(literal_redacted.len());
    for line in literal_redacted.split_inclusive('\n') {
        let (body, newline) = line
            .strip_suffix('\n')
            .map(|body| (body, "\n"))
            .unwrap_or((line, ""));
        if let Ok(mut value) = serde_json::from_str::<Value>(body) {
            redact_json_value(&mut value);
            output.push_str(&serde_json::to_string(&value).unwrap_or_else(|_| body.to_string()));
        } else {
            output.push_str(body);
        }
        output.push_str(newline);
    }
    output
}

/// Redacts sensitive keys recursively in one JSON value.
/// 在一个 JSON 值中递归脱敏敏感键。
fn redact_json_value(value: &mut Value) {
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                if is_sensitive_json_key(key) {
                    *child = Value::String(REDACTED_PROVIDER_SECRET.to_string());
                } else {
                    redact_json_value(child);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                redact_json_value(item);
            }
        }
        _ => {}
    }
}

/// Returns whether one JSON key carries a Provider token or secret.
/// 返回一个 JSON 键是否承载 Provider token 或密钥。
fn is_sensitive_json_key(key: &str) -> bool {
    let normalized = key.to_ascii_lowercase().replace('-', "_");
    matches!(
        normalized.as_str(),
        "authorization"
            | "api_key"
            | "apikey"
            | "access_token"
            | "refresh_token"
            | "token"
            | "secret"
            | "client_secret"
            | "password"
            | "credential"
    ) || normalized.ends_with("_token")
        || normalized.ends_with("_secret")
        || normalized.ends_with("_api_key")
}

/// Builds one non-fatal reservation diagnostic outcome.
/// 构建一个非致命预留诊断结果。
fn reservation_diagnostic(
    code: ProviderApiTraceDiagnosticCode,
    detail: String,
) -> ProviderApiTraceReservationOutcome {
    ProviderApiTraceReservationOutcome::Diagnostic(trace_diagnostic(code, detail))
}

/// Builds one non-fatal publication diagnostic outcome.
/// 构建一个非致命发布诊断结果。
fn write_diagnostic(
    code: ProviderApiTraceDiagnosticCode,
    detail: String,
) -> ProviderApiTraceWriteOutcome {
    ProviderApiTraceWriteOutcome::Diagnostic(trace_diagnostic(code, detail))
}

/// Builds one non-fatal trace diagnostic value.
/// 构建一个非致命追踪诊断值。
fn trace_diagnostic(
    code: ProviderApiTraceDiagnosticCode,
    detail: String,
) -> ProviderApiTraceDiagnostic {
    ProviderApiTraceDiagnostic { code, detail }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one valid API call id for tests.
    /// 为测试构建一个有效 API 调用 id。
    fn api_call_id(value: u64) -> ApiCallId {
        ApiCallId::try_new(value).expect("valid API call id")
    }

    /// Reserves one attempt and requires a successful result.
    /// 预留一次 attempt 并要求成功结果。
    fn reserve(
        store: &ProviderApiTraceStore,
        api_call: u64,
        attempt_index: u32,
    ) -> ProviderApiTraceAttemptReservation {
        match store.reserve_attempt(api_call_id(api_call), attempt_index) {
            ProviderApiTraceReservationOutcome::Reserved(reservation) => reservation,
            outcome => panic!("expected reserved trace attempt, got {outcome:?}"),
        }
    }

    /// Publishes one deterministic complete payload pair.
    /// 发布一对确定性的完整载荷。
    fn publish(
        store: &ProviderApiTraceStore,
        reservation: &ProviderApiTraceAttemptReservation,
    ) -> ProviderApiTraceWriteOutcome {
        store.publish_attempt(
            reservation,
            ProviderApiTracePayload {
                input: r#"{"api_key":"input-secret","body":"ok"}"#,
                output: r#"{"token":"output-secret","result":"ok"}"#,
                secrets: &[],
            },
        )
    }

    /// A disabled store never creates its configured root.
    /// 禁用存储绝不创建其配置根目录。
    #[test]
    fn disabled_store_touches_no_filesystem_path() {
        let temporary = tempfile::tempdir().expect("temporary trace parent");
        let root = temporary.path().join("trace");
        let store = ProviderApiTraceStore::new(root.clone(), false);
        assert_eq!(
            store.reserve_attempt(api_call_id(1), 0),
            ProviderApiTraceReservationOutcome::Disabled
        );
        assert!(!root.exists());
    }

    /// Twelve retries of one API consume twelve sequences and retain the newest ten.
    /// 同一 API 的十二次重试消耗十二个序号并保留最新十次。
    #[test]
    fn retries_of_one_api_are_independent_physical_attempts() {
        let temporary = tempfile::tempdir().expect("temporary trace root");
        let store = ProviderApiTraceStore::new(temporary.path().to_path_buf(), true);
        for attempt_index in 0..12 {
            let reservation = reserve(&store, 41, attempt_index);
            assert_eq!(reservation.trace_sequence, u64::from(attempt_index) + 1);
            assert!(matches!(
                publish(&store, &reservation),
                ProviderApiTraceWriteOutcome::Written { .. }
            ));
        }
        let mut retained = Vec::new();
        for slot in 0..PROVIDER_API_TRACE_LIMIT {
            let pointer = read_slot_pointer(&temporary.path().join(format!("slot-{slot:02}")))
                .expect("valid slot pointer")
                .expect("occupied slot");
            retained.push(pointer.trace_sequence);
            assert_eq!(pointer.api_call_id.value(), 41);
        }
        retained.sort_unstable();
        assert_eq!(retained, (3..=12).collect::<Vec<_>>());
    }

    /// Restart continues after the greatest exact persisted sequence.
    /// 重启后从精确持久化的最大序号之后继续。
    #[test]
    fn restart_continues_sequence_frontier() {
        let temporary = tempfile::tempdir().expect("temporary trace root");
        {
            let store = ProviderApiTraceStore::new(temporary.path().to_path_buf(), true);
            for sequence in 1..=10 {
                let reservation = reserve(&store, sequence, 0);
                assert_eq!(reservation.trace_sequence, sequence);
                assert!(matches!(
                    publish(&store, &reservation),
                    ProviderApiTraceWriteOutcome::Written { .. }
                ));
            }
        }
        let restarted = ProviderApiTraceStore::new(temporary.path().to_path_buf(), true);
        assert_eq!(reserve(&restarted, 11, 0).trace_sequence, 11);
    }

    /// Exact duplicate bytes are idempotent while content drift is diagnosed.
    /// 完全相同字节幂等，而内容漂移会被诊断。
    #[test]
    fn duplicate_identity_requires_exact_redacted_payload_bytes() {
        let temporary = tempfile::tempdir().expect("temporary trace root");
        let store = ProviderApiTraceStore::new(temporary.path().to_path_buf(), true);
        let reservation = reserve(&store, 1, 0);
        assert!(matches!(
            publish(&store, &reservation),
            ProviderApiTraceWriteOutcome::Written { .. }
        ));
        assert!(matches!(
            publish(&store, &reservation),
            ProviderApiTraceWriteOutcome::Written { .. }
        ));
        let changed = store.publish_attempt(
            &reservation,
            ProviderApiTracePayload {
                input: r#"{"body":"changed"}"#,
                output: r#"{"result":"ok"}"#,
                secrets: &[],
            },
        );
        assert!(matches!(
            changed,
            ProviderApiTraceWriteOutcome::Diagnostic(ProviderApiTraceDiagnostic {
                code: ProviderApiTraceDiagnosticCode::ContentMismatch,
                ..
            })
        ));
    }

    /// A delayed old completion cannot overwrite the newer occupant of its slot.
    /// 延迟的旧完成不能覆盖其槽位中的较新记录。
    #[test]
    fn delayed_completion_cannot_overwrite_newer_slot() {
        let temporary = tempfile::tempdir().expect("temporary trace root");
        let store = ProviderApiTraceStore::new(temporary.path().to_path_buf(), true);
        let delayed = reserve(&store, 1, 0);
        for api_call in 2..=11 {
            let reservation = reserve(&store, api_call, 0);
            assert!(matches!(
                publish(&store, &reservation),
                ProviderApiTraceWriteOutcome::Written { .. }
            ));
        }
        assert!(matches!(
            publish(&store, &delayed),
            ProviderApiTraceWriteOutcome::Diagnostic(ProviderApiTraceDiagnostic {
                code: ProviderApiTraceDiagnosticCode::SupersededAttempt,
                ..
            })
        ));
        let pointer = read_slot_pointer(&temporary.path().join("slot-00"))
            .expect("valid pointer")
            .expect("occupied slot");
        assert_eq!(pointer.trace_sequence, 11);
    }

    /// Concurrent reservations through one owner remain unique and contiguous.
    /// 通过同一 owner 的并发预留保持唯一且连续。
    #[test]
    fn concurrent_reservations_are_unique_and_contiguous() {
        let temporary = tempfile::tempdir().expect("temporary trace root");
        let store = ProviderApiTraceStore::new(temporary.path().to_path_buf(), true);
        let mut workers = Vec::new();
        for api_call in 1..=16 {
            let worker_store = store.clone();
            workers.push(std::thread::spawn(move || {
                reserve(&worker_store, api_call, 0).trace_sequence
            }));
        }
        let mut sequences = workers
            .into_iter()
            .map(|worker| worker.join().expect("reservation worker"))
            .collect::<Vec<_>>();
        sequences.sort_unstable();
        assert_eq!(sequences, (1..=16).collect::<Vec<_>>());
    }

    /// Caller-known values and nested sensitive JSON fields use one marker.
    /// 调用方已知值与嵌套敏感 JSON 字段使用同一标记。
    #[test]
    fn token_and_secret_redaction_is_fixed_and_recursive() {
        let raw = r#"{"authorization":"Bearer literal-secret","nested":{"client_secret":"hidden"},"body":"literal-secret"}"#;
        let redacted = redact_provider_secrets(raw, &["literal-secret".to_string()]);
        assert!(!redacted.contains("literal-secret"));
        assert!(!redacted.contains("hidden"));
        assert_eq!(redacted.matches(REDACTED_PROVIDER_SECRET).count(), 3);
    }

    /// A filesystem failure remains a non-fatal reservation diagnostic.
    /// 文件系统失败仍是非致命预留诊断。
    #[test]
    fn trace_failure_never_returns_a_main_chain_error() {
        let temporary = tempfile::tempdir().expect("temporary trace parent");
        let occupied = temporary.path().join("occupied");
        fs::write(&occupied, "not a directory").expect("occupy trace root");
        let store = ProviderApiTraceStore::new(occupied, true);
        assert!(matches!(
            store.reserve_attempt(api_call_id(1), 0),
            ProviderApiTraceReservationOutcome::Diagnostic(_)
        ));
    }

    /// Explicit prepare and clear operations share the sequence owner and remain idempotent.
    /// 显式准备和清理操作共享序号 owner 并保持幂等。
    #[test]
    fn prepare_and_clear_share_one_exact_store_owner() {
        let temporary = tempfile::tempdir().expect("temporary trace parent");
        let root = temporary.path().join("temp").join("provider-api-trace");
        let store = ProviderApiTraceStore::new(root.clone(), true);

        assert_eq!(
            store.prepare_directory().expect("prepare exact trace root"),
            root
        );
        assert_eq!(reserve(&store, 1, 0).trace_sequence, 1);
        assert!(store.clear_directory().expect("clear exact trace root"));
        assert!(!root.exists());
        assert!(!store.clear_directory().expect("clear absent trace root"));
        assert_eq!(reserve(&store, 2, 0).trace_sequence, 1);
    }

    /// A symbolic-link parent is rejected before prepare or clear can escape the session owner.
    /// 在准备或清理能够逃逸会话 owner 前拒绝符号链接父目录。
    #[test]
    fn symbolic_link_parent_is_rejected_for_prepare_and_clear() {
        let temporary = tempfile::tempdir().expect("temporary trace parent");
        let outside = tempfile::tempdir().expect("outside trace parent");
        let parent = temporary.path().join("temp");

        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path(), &parent).expect("create temp symlink");
        #[cfg(windows)]
        if let Err(error) = std::os::windows::fs::symlink_dir(outside.path(), &parent) {
            if error.kind() == std::io::ErrorKind::PermissionDenied
                || error.raw_os_error() == Some(1314)
            {
                return;
            }
            panic!("create temp symlink: {error}");
        }

        let store = ProviderApiTraceStore::new(parent.join("provider-api-trace"), true);
        assert!(store.prepare_directory().is_err());
        assert!(store.clear_directory().is_err());
        assert!(!outside.path().join("provider-api-trace").exists());
    }
}
