//! Host-owned CDP and DAP tools backed by one LuaSkills System Plugin.
//! 由 LuaSkills System Plugin 承载的宿主自有 CDP 与 DAP 工具。

use super::*;
use crate::luaskills_runtime::LuaSkillsRuntimeHandle;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};
use tokio::sync::Mutex as AsyncMutex;
use vulcan_runtime_assets::VULCAN_DEBUG_RUNTIME_SYSTEM_PLUGIN_ID;

/// Stable identifier for opening one managed debug session.
/// 创建一个受管调试会话的稳定工具标识。
pub const DEBUG_SESSION_OPEN_TOOL_ID: &str = "debug_session_open";

/// Stable identifier for sending one protocol request.
/// 发送一条协议请求的稳定工具标识。
pub const DEBUG_SESSION_REQUEST_TOOL_ID: &str = "debug_session_request";

/// Stable identifier for draining retained protocol events.
/// 排空已保留协议事件的稳定工具标识。
pub const DEBUG_SESSION_EVENTS_TOOL_ID: &str = "debug_session_events";

/// Stable identifier for reading managed debug session status.
/// 读取受管调试会话状态的稳定工具标识。
pub const DEBUG_SESSION_STATUS_TOOL_ID: &str = "debug_session_status";

/// Stable identifier for closing one managed debug session.
/// 关闭一个受管调试会话的稳定工具标识。
pub const DEBUG_SESSION_CLOSE_TOOL_ID: &str = "debug_session_close";

/// Fixed System Plugin manifest file name resolved inside the package root.
/// 在包根目录中解析的固定 System Plugin 清单文件名。
const DEBUG_SYSTEM_PACKAGE_MANIFEST: &str = "dependencies.yaml";

/// Maximum protocol request timeout accepted from model-visible arguments.
/// 模型可见参数允许的最大协议请求超时时间。
const MAX_DEBUG_REQUEST_TIMEOUT_MS: u64 = 60_000;

/// Maximum event wait accepted from model-visible arguments.
/// 模型可见参数允许的最大事件等待时间。
const MAX_DEBUG_EVENT_WAIT_MS: u64 = 10_000;

/// Maximum event count returned by one bounded polling call.
/// 单次有界轮询调用允许返回的最大事件数量。
const MAX_DEBUG_EVENT_PAGE_SIZE: u64 = 32;

/// Maximum concurrently retained protocol sidecars in one native tool catalog.
/// 单个原生工具目录允许同时保留的协议 sidecar 最大数量。
const MAX_ACTIVE_DEBUG_SESSIONS: usize = 8;

/// Extra host-side wait reserved for Lua and sidecar response serialization.
/// 为 Lua 与 sidecar 响应序列化预留的宿主额外等待时间。
const DEBUG_HOST_TIMEOUT_GRACE_MS: u64 = 5_000;

/// Fixed Lua entry used for all ordinary sidecar commands.
/// 所有普通 sidecar 命令使用的固定 Lua 入口。
const DEBUG_DISPATCH_LUA: &str =
    "local plugin = require('runtime.plugin')\nreturn plugin.dispatch(args)";

/// Fixed Lua entry used for deterministic sidecar shutdown.
/// 确定性关闭 sidecar 使用的固定 Lua 入口。
const DEBUG_SHUTDOWN_LUA: &str =
    "local plugin = require('runtime.plugin')\nreturn plugin.shutdown(args)";

/// Protocol selected for one managed debug session.
/// 单个受管调试会话选择的协议。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DebugProtocol {
    /// Chrome DevTools Protocol over a loopback WebSocket.
    /// 通过回环 WebSocket 连接的 Chrome DevTools Protocol。
    Cdp,
    /// Debug Adapter Protocol over a loopback TCP endpoint.
    /// 通过回环 TCP 端点连接的 Debug Adapter Protocol。
    Dap,
}

impl DebugProtocol {
    /// Parses one exact model-visible protocol identifier.
    /// 解析一个精确的模型可见协议标识。
    fn parse(value: &str) -> Result<Self, NativeToolError> {
        match value {
            "cdp" => Ok(Self::Cdp),
            "dap" => Ok(Self::Dap),
            _ => Err(NativeToolError::new("protocol must be `cdp` or `dap`.")),
        }
    }

    /// Returns the stable model-visible protocol identifier.
    /// 返回稳定的模型可见协议标识。
    fn as_str(self) -> &'static str {
        match self {
            Self::Cdp => "cdp",
            Self::Dap => "dap",
        }
    }
}

/// Model-visible arguments for opening one debug session.
/// 创建一个调试会话的模型可见参数。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DebugSessionOpenArgs {
    /// Exact protocol identifier: `cdp` or `dap`.
    /// 精确协议标识：`cdp` 或 `dap`。
    protocol: String,
    /// Protocol-specific local endpoint.
    /// 协议专用的本机端点。
    target: Value,
}

/// Model-visible arguments for one protocol request.
/// 单条协议请求的模型可见参数。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DebugSessionRequestArgs {
    /// Stable host-owned debug session identifier.
    /// 宿主持有的稳定调试会话标识。
    session_id: String,
    /// CDP method name, required only for CDP sessions.
    /// CDP 方法名，仅 CDP 会话必填。
    method: Option<String>,
    /// CDP request parameters.
    /// CDP 请求参数。
    params: Option<Value>,
    /// DAP command name, required only for DAP sessions.
    /// DAP 命令名，仅 DAP 会话必填。
    command: Option<String>,
    /// DAP request arguments.
    /// DAP 请求参数。
    arguments: Option<Value>,
    /// DAP reverse-request sequence being answered.
    /// 正在应答的 DAP 反向请求序号。
    request_seq: Option<u64>,
    /// DAP reverse-request response success state.
    /// DAP 反向请求响应的成功状态。
    success: Option<bool>,
    /// DAP reverse-request response body.
    /// DAP 反向请求响应体。
    body: Option<Value>,
    /// Optional DAP reverse-request failure message.
    /// 可选的 DAP 反向请求失败消息。
    message: Option<String>,
    /// Finite protocol response timeout in milliseconds.
    /// 有限协议响应超时时间，单位为毫秒。
    timeout_ms: Option<u64>,
}

/// Model-visible arguments for reading retained protocol events.
/// 读取已保留协议事件的模型可见参数。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DebugSessionEventsArgs {
    /// Stable host-owned debug session identifier.
    /// 宿主持有的稳定调试会话标识。
    session_id: String,
    /// Maximum number of events returned by this poll.
    /// 本次轮询返回的最大事件数量。
    max_events: Option<u64>,
    /// Finite wait for the first event in milliseconds.
    /// 等待首个事件的有限时长，单位为毫秒。
    timeout_ms: Option<u64>,
}

/// Model-visible arguments for status and close operations.
/// 状态与关闭操作的模型可见参数。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DebugSessionIdArgs {
    /// Stable host-owned debug session identifier.
    /// 宿主持有的稳定调试会话标识。
    session_id: String,
}

/// One active host-owned debug session and its immutable lease identity.
/// 一个活动的宿主自有调试会话及其不可变租约身份。
struct DebugRuntimeSession {
    /// Stable public session identifier returned to the model.
    /// 返回给模型的稳定公开会话标识。
    session_id: String,
    /// Controller session that exclusively owns this debug session.
    /// 独占拥有当前调试会话的控制器会话。
    owner_session_id: String,
    /// Protocol bound at session creation.
    /// 会话创建时绑定的协议。
    protocol: DebugProtocol,
    /// Opaque LuaSkills System lease identifier.
    /// 不透明的 LuaSkills System 租约标识。
    lease_id: String,
    /// LuaSkills System lease generation paired with the lease id.
    /// 与租约标识配对的 LuaSkills System 租约代际。
    generation: u64,
    /// Per-session operation gate preserving request and close ordering.
    /// 保持请求与关闭顺序的单会话操作门。
    operation_gate: AsyncMutex<()>,
    /// Closing marker preventing stale cloned handles from issuing new requests.
    /// 阻止已克隆旧句柄继续发起请求的关闭标记。
    closing: AtomicBool,
}

/// Shared state retained by all five debug native tools.
/// 五个调试原生工具共同保留的共享状态。
struct DebugRuntimeState {
    /// Shared LuaSkills runtime used to create and evaluate System leases.
    /// 用于创建和执行 System 租约的共享 LuaSkills 运行时。
    luaskills_runtime: LuaSkillsRuntimeHandle,
    /// Active debug sessions keyed by host-owned public identifiers.
    /// 以宿主持有公开标识为键的活动调试会话。
    sessions: Mutex<BTreeMap<String, Arc<DebugRuntimeSession>>>,
    /// Failed-to-close sessions retained only for manager-drop cleanup.
    /// 仅为管理器释放清理而保留的关闭失败会话。
    orphaned_sessions: Mutex<Vec<Arc<DebugRuntimeSession>>>,
    /// Monotonic sidecar request identifier source.
    /// 单调递增的 sidecar 请求标识来源。
    next_request_id: AtomicU64,
    /// Count of reserved or active debug sidecars enforcing the process limit.
    /// 用于限制进程数量的已预留或活动调试 sidecar 计数。
    active_sessions: AtomicUsize,
}

/// RAII reservation that returns one debug-session slot unless creation commits it.
/// 在创建提交前自动归还一个调试会话名额的 RAII 预留器。
struct DebugSessionSlotReservation<'a> {
    /// Shared active-session counter owned by the manager state.
    /// 管理器状态持有的共享活动会话计数器。
    counter: &'a AtomicUsize,
    /// Whether successful session registration took ownership of this slot.
    /// 成功登记的会话是否已经接管该名额。
    committed: bool,
}

impl<'a> DebugSessionSlotReservation<'a> {
    /// Atomically reserves one slot without exceeding the configured process limit.
    /// 以原子方式预留一个名额，并确保不超过配置的进程上限。
    fn reserve(counter: &'a AtomicUsize) -> Result<Self, NativeToolError> {
        counter
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                (current < MAX_ACTIVE_DEBUG_SESSIONS).then_some(current + 1)
            })
            .map_err(|_| {
                NativeToolError::new(format!(
                    "debug session limit reached; close one of the {MAX_ACTIVE_DEBUG_SESSIONS} active sessions first."
                ))
            })?;
        Ok(Self {
            counter,
            committed: false,
        })
    }

    /// Transfers the reserved slot to one successfully registered session.
    /// 把已预留名额移交给一个成功登记的会话。
    fn commit(mut self) {
        self.committed = true;
    }
}

impl Drop for DebugSessionSlotReservation<'_> {
    /// Returns an uncommitted slot after any creation failure path.
    /// 在任意创建失败路径后归还尚未提交的名额。
    fn drop(&mut self) {
        if !self.committed {
            release_debug_session_slot(self.counter);
        }
    }
}

impl Drop for DebugRuntimeState {
    /// Retires every remaining System lease when the owning native catalog is dropped.
    /// 当所属原生工具目录释放时退役所有剩余 System 租约。
    fn drop(&mut self) {
        let sessions = self
            .sessions
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let orphaned_sessions = self
            .orphaned_sessions
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if sessions.is_empty() && orphaned_sessions.is_empty() {
            return;
        }
        let Ok(engine) = self.luaskills_runtime.engine.lock() else {
            return;
        };
        for session in sessions.values() {
            session.closing.store(true, Ordering::Release);
            let _ = close_system_lease_sync(&engine, session);
        }
        for session in orphaned_sessions.iter() {
            session.closing.store(true, Ordering::Release);
            let _ = close_system_lease_sync(&engine, session);
        }
        sessions.clear();
        orphaned_sessions.clear();
    }
}

/// Cloneable manager coordinating LuaSkills System leases and protocol sidecars.
/// 协调 LuaSkills System 租约与协议 sidecar 的可克隆管理器。
#[derive(Clone)]
struct DebugRuntimeManager {
    /// Shared manager state.
    /// 共享管理器状态。
    state: Arc<DebugRuntimeState>,
}

impl DebugRuntimeManager {
    /// Builds one manager bound to the application-owned LuaSkills runtime.
    /// 构建一个绑定到应用自有 LuaSkills 运行时的管理器。
    fn new(luaskills_runtime: LuaSkillsRuntimeHandle) -> Self {
        Self {
            state: Arc::new(DebugRuntimeState {
                luaskills_runtime,
                sessions: Mutex::new(BTreeMap::new()),
                orphaned_sessions: Mutex::new(Vec::new()),
                next_request_id: AtomicU64::new(1),
                active_sessions: AtomicUsize::new(0),
            }),
        }
    }

    /// Opens one protocol sidecar inside a dedicated host-owned System lease.
    /// 在专用宿主自有 System 租约中创建一个协议 sidecar。
    async fn open(
        &self,
        request: &NativeToolExecutionRequest,
        args: DebugSessionOpenArgs,
    ) -> Result<Value, NativeToolError> {
        let owner_session_id = required_owner_session_id(request)?;
        let workspace_root = canonical_workspace_root(request)?;
        let protocol = DebugProtocol::parse(&args.protocol)?;
        validate_open_target(protocol, &args.target)?;
        let package_root = validate_debug_system_package(&self.state.luaskills_runtime)?;
        let slot = DebugSessionSlotReservation::reserve(&self.state.active_sessions)?;
        let session_id = Uuid::new_v4().to_string();
        let sid = format!("vulcan-debug/{session_id}");
        let create_request = json!({
            "sid": sid,
            "ttl_sec": 0,
            "replace": false,
            "cwd": normalized_workspace_path_text_from_path(&package_root),
            "workspace_root": normalized_workspace_path_text_from_path(&workspace_root),
            "system_package": {
                "id": VULCAN_DEBUG_RUNTIME_SYSTEM_PLUGIN_ID,
                "root": normalized_workspace_path_text_from_path(&package_root),
                "dependencies_file": DEBUG_SYSTEM_PACKAGE_MANIFEST,
            },
        });
        let runtime = self.state.luaskills_runtime.clone();
        let create_response = call_engine_blocking(runtime, move |engine| {
            let raw = engine
                .create_system_runtime_lease_json(&create_request.to_string())
                .map_err(NativeToolError::new)?;
            parse_engine_response(&raw, "create System runtime lease")
        })
        .await?;
        let lease_id = required_response_string(&create_response, "lease_id")?;
        let generation = required_response_u64(&create_response, "generation")?;
        let session = Arc::new(DebugRuntimeSession {
            session_id: session_id.clone(),
            owner_session_id,
            protocol,
            lease_id,
            generation,
            operation_gate: AsyncMutex::new(()),
            closing: AtomicBool::new(false),
        });
        let open_result = self
            .dispatch(
                Arc::clone(&session),
                json!({
                    "op": "open",
                    "protocol": protocol.as_str(),
                    "target": args.target,
                }),
                30_000,
                DEBUG_DISPATCH_LUA,
            )
            .await;
        let status = match open_result {
            Ok(status) => status,
            Err(error) => {
                if let Err(retirement_error) = self.retire_session(&session, false).await {
                    let retention = self.retain_orphaned_session(Arc::clone(&session));
                    if retention.is_ok() {
                        slot.commit();
                    }
                    return Err(combined_retirement_error(
                        error,
                        retirement_error,
                        retention,
                    ));
                }
                return Err(error);
            }
        };
        let registration_error = {
            match lock_sessions(&self.state.sessions) {
                Ok(sessions) if sessions.contains_key(&session_id) => Some(NativeToolError::new(
                    "generated debug session id already exists.",
                )),
                Ok(mut sessions) => {
                    sessions.insert(session_id.clone(), Arc::clone(&session));
                    None
                }
                Err(error) => Some(error),
            }
        };
        if let Some(error) = registration_error {
            if let Err(retirement_error) = self.retire_session(&session, false).await {
                let retention = self.retain_orphaned_session(Arc::clone(&session));
                if retention.is_ok() {
                    slot.commit();
                }
                return Err(combined_retirement_error(
                    error,
                    retirement_error,
                    retention,
                ));
            }
            return Err(error);
        }
        slot.commit();
        Ok(json!({
            "session_id": session_id,
            "protocol": protocol.as_str(),
            "status": status,
        }))
    }

    /// Sends one protocol-specific request through an owned debug session.
    /// 通过自有调试会话发送一条协议专用请求。
    async fn request(
        &self,
        request: &NativeToolExecutionRequest,
        args: DebugSessionRequestArgs,
    ) -> Result<Value, NativeToolError> {
        let owner_session_id = required_owner_session_id(request)?;
        let session = self.owned_session(&args.session_id, &owner_session_id)?;
        let timeout_ms = bounded_timeout(
            args.timeout_ms.unwrap_or(30_000),
            1,
            MAX_DEBUG_REQUEST_TIMEOUT_MS,
            "timeout_ms",
        )?;
        let payload = match session.protocol {
            DebugProtocol::Cdp => {
                if args.command.is_some()
                    || args.arguments.is_some()
                    || args.request_seq.is_some()
                    || args.success.is_some()
                    || args.body.is_some()
                    || args.message.is_some()
                {
                    return Err(NativeToolError::new(
                        "CDP requests accept only method, params, and timeout_ms.",
                    ));
                }
                json!({
                    "op": "request",
                    "method": required_non_empty(args.method, "method")?,
                    "params": args.params.unwrap_or_else(|| json!({})),
                    "timeout_ms": timeout_ms,
                })
            }
            DebugProtocol::Dap => {
                if args.method.is_some() || args.params.is_some() {
                    return Err(NativeToolError::new(
                        "DAP operations do not accept CDP method or params fields.",
                    ));
                }
                let command = required_non_empty(args.command, "command")?;
                if let Some(request_seq) = args.request_seq {
                    if args.arguments.is_some() {
                        return Err(NativeToolError::new(
                            "DAP reverse-request responses do not accept arguments.",
                        ));
                    }
                    json!({
                        "op": "request",
                        "command": command,
                        "request_seq": bounded_timeout(request_seq, 1, u64::MAX, "request_seq")?,
                        "success": args.success.ok_or_else(|| NativeToolError::new(
                            "success is required for DAP reverse-request responses."
                        ))?,
                        "body": args.body.unwrap_or_else(|| json!({})),
                        "message": args.message,
                    })
                } else {
                    if args.success.is_some() || args.body.is_some() || args.message.is_some() {
                        return Err(NativeToolError::new(
                            "DAP client requests do not accept response fields without request_seq.",
                        ));
                    }
                    json!({
                        "op": "request",
                        "command": command,
                        "arguments": args.arguments.unwrap_or_else(|| json!({})),
                        "timeout_ms": timeout_ms,
                    })
                }
            }
        };
        let response = self
            .dispatch(
                Arc::clone(&session),
                payload,
                timeout_ms + DEBUG_HOST_TIMEOUT_GRACE_MS,
                DEBUG_DISPATCH_LUA,
            )
            .await?;
        Ok(json!({
            "session_id": session.session_id,
            "protocol": session.protocol.as_str(),
            "response": response,
        }))
    }

    /// Drains one bounded page of retained protocol events.
    /// 排空一页有界的已保留协议事件。
    async fn events(
        &self,
        request: &NativeToolExecutionRequest,
        args: DebugSessionEventsArgs,
    ) -> Result<Value, NativeToolError> {
        let owner_session_id = required_owner_session_id(request)?;
        let session = self.owned_session(&args.session_id, &owner_session_id)?;
        let max_events = bounded_timeout(
            args.max_events.unwrap_or(MAX_DEBUG_EVENT_PAGE_SIZE),
            1,
            MAX_DEBUG_EVENT_PAGE_SIZE,
            "max_events",
        )?;
        let timeout_ms = bounded_timeout(
            args.timeout_ms.unwrap_or(0),
            0,
            MAX_DEBUG_EVENT_WAIT_MS,
            "timeout_ms",
        )?;
        let events = self
            .dispatch(
                Arc::clone(&session),
                json!({
                    "op": "events",
                    "max_events": max_events,
                    "timeout_ms": timeout_ms,
                }),
                timeout_ms + DEBUG_HOST_TIMEOUT_GRACE_MS,
                DEBUG_DISPATCH_LUA,
            )
            .await?;
        Ok(json!({
            "session_id": session.session_id,
            "protocol": session.protocol.as_str(),
            "events": events,
        }))
    }

    /// Reads current sidecar and queue status for one owned debug session.
    /// 读取一个自有调试会话当前的 sidecar 与队列状态。
    async fn status(
        &self,
        request: &NativeToolExecutionRequest,
        args: DebugSessionIdArgs,
    ) -> Result<Value, NativeToolError> {
        let owner_session_id = required_owner_session_id(request)?;
        let session = self.owned_session(&args.session_id, &owner_session_id)?;
        let status = self
            .dispatch(
                Arc::clone(&session),
                json!({ "op": "status" }),
                DEBUG_HOST_TIMEOUT_GRACE_MS,
                DEBUG_DISPATCH_LUA,
            )
            .await?;
        Ok(json!({
            "session_id": session.session_id,
            "protocol": session.protocol.as_str(),
            "status": status,
        }))
    }

    /// Closes one owned protocol sidecar and retires its System lease.
    /// 关闭一个自有协议 sidecar 并退役其 System 租约。
    async fn close(
        &self,
        request: &NativeToolExecutionRequest,
        args: DebugSessionIdArgs,
    ) -> Result<Value, NativeToolError> {
        let owner_session_id = required_owner_session_id(request)?;
        let session = self.owned_session(&args.session_id, &owner_session_id)?;
        if session.closing.swap(true, Ordering::AcqRel) {
            return Err(NativeToolError::new("debug session is already closing."));
        }
        let status = self.retire_session(&session, true).await?;
        release_debug_session_slot(&self.state.active_sessions);
        lock_sessions(&self.state.sessions)?.remove(&args.session_id);
        Ok(json!({
            "session_id": session.session_id,
            "protocol": session.protocol.as_str(),
            "closed": true,
            "status": status,
        }))
    }

    /// Resolves one active session and enforces controller-session ownership.
    /// 解析一个活动会话并强制校验控制器会话所有权。
    fn owned_session(
        &self,
        session_id: &str,
        owner_session_id: &str,
    ) -> Result<Arc<DebugRuntimeSession>, NativeToolError> {
        let sessions = lock_sessions(&self.state.sessions)?;
        let session = sessions
            .get(session_id)
            .cloned()
            .ok_or_else(|| NativeToolError::new("debug session was not found."))?;
        if session.owner_session_id != owner_session_id {
            return Err(NativeToolError::new(
                "debug session belongs to a different controller session.",
            ));
        }
        if session.closing.load(Ordering::Acquire) {
            return Err(NativeToolError::new("debug session is closing."));
        }
        Ok(session)
    }

    /// Retains one unreachable failed-to-close lease for manager-drop cleanup.
    /// 为管理器释放清理保留一个外部不可访问且关闭失败的租约。
    fn retain_orphaned_session(
        &self,
        session: Arc<DebugRuntimeSession>,
    ) -> Result<(), NativeToolError> {
        self.state
            .orphaned_sessions
            .lock()
            .map_err(|_| NativeToolError::new("debug orphaned-session lock is poisoned."))?
            .push(session);
        Ok(())
    }

    /// Evaluates one fixed Lua entry while preserving per-session operation order.
    /// 在保持单会话操作顺序的前提下执行一个固定 Lua 入口。
    async fn dispatch(
        &self,
        session: Arc<DebugRuntimeSession>,
        mut payload: Value,
        host_timeout_ms: u64,
        code: &'static str,
    ) -> Result<Value, NativeToolError> {
        let _operation = session.operation_gate.lock().await;
        if session.closing.load(Ordering::Acquire) && code != DEBUG_SHUTDOWN_LUA {
            return Err(NativeToolError::new("debug session is closing."));
        }
        let request_id = self.state.next_request_id.fetch_add(1, Ordering::Relaxed);
        let object = payload
            .as_object_mut()
            .ok_or_else(|| NativeToolError::new("debug sidecar payload must be an object."))?;
        object.insert(String::from("request_id"), json!(request_id.to_string()));
        object.insert(String::from("host_timeout_ms"), json!(host_timeout_ms));
        // Preserve exact JSON object/array shapes across the Lua table boundary.
        // 跨越 Lua 表边界时保留精确的 JSON 对象与数组形状。
        let payload_json = payload.to_string();
        let runtime = self.state.luaskills_runtime.clone();
        let lease_id = session.lease_id.clone();
        let generation = session.generation;
        call_engine_blocking(runtime, move |engine| {
            let eval_request = json!({
                "lease_id": lease_id,
                "generation": generation,
                "code": code,
                "args": {
                    "payload_json": payload_json,
                    "host_timeout_ms": host_timeout_ms,
                },
            });
            let raw = engine
                .eval_system_runtime_lease_json(&eval_request.to_string())
                .map_err(NativeToolError::new)?;
            let response = parse_engine_response(&raw, "evaluate System runtime lease")?;
            parse_sidecar_result(&response)
        })
        .await
    }

    /// Shuts down one sidecar and closes its LuaSkills System lease.
    /// 关闭一个 sidecar 并释放其 LuaSkills System 租约。
    async fn retire_session(
        &self,
        session: &Arc<DebugRuntimeSession>,
        collect_status: bool,
    ) -> Result<Value, NativeToolError> {
        session.closing.store(true, Ordering::Release);
        let shutdown_result = self
            .dispatch(
                Arc::clone(session),
                json!({ "op": "close" }),
                DEBUG_HOST_TIMEOUT_GRACE_MS,
                DEBUG_SHUTDOWN_LUA,
            )
            .await;
        let runtime = self.state.luaskills_runtime.clone();
        let lease_id = session.lease_id.clone();
        let generation = session.generation;
        let close_result = call_engine_blocking(runtime, move |engine| {
            let raw = engine
                .close_system_runtime_lease_json(
                    &json!({
                        "lease_id": lease_id.as_str(),
                        "generation": generation,
                    })
                    .to_string(),
                )
                .map_err(NativeToolError::new)?;
            parse_engine_response(&raw, "close System runtime lease")
        })
        .await;
        close_result?;
        if collect_status {
            match shutdown_result {
                Ok(status) => Ok(status),
                Err(error) => Ok(json!({
                    "graceful_shutdown": false,
                    "diagnostic": error.message,
                })),
            }
        } else {
            Ok(Value::Null)
        }
    }
}

/// Builds all host-owned debug native tools around one shared manager.
/// 围绕一个共享管理器构建全部宿主自有调试原生工具。
pub(crate) fn build_debug_runtime_tools(
    luaskills_runtime: LuaSkillsRuntimeHandle,
) -> Vec<Arc<NativeToolDefinition>> {
    let manager = DebugRuntimeManager::new(luaskills_runtime);
    vec![
        build_debug_session_open_tool(manager.clone()),
        build_debug_session_request_tool(manager.clone()),
        build_debug_session_events_tool(manager.clone()),
        build_debug_session_status_tool(manager.clone()),
        build_debug_session_close_tool(manager),
    ]
}

/// Builds the debug session open tool definition.
/// 构建调试会话创建工具定义。
fn build_debug_session_open_tool(manager: DebugRuntimeManager) -> Arc<NativeToolDefinition> {
    build_native_tool_definition(
        debug_descriptor(
            DEBUG_SESSION_OPEN_TOOL_ID,
            "Open a managed local CDP or DAP debug session.",
        ),
        json!({
            "type": "object",
            "properties": {
                "protocol": { "type": "string", "enum": ["cdp", "dap"] },
                "target": {
                    "type": "object",
                    "properties": {
                        "websocket_url": { "type": "string" },
                        "host": { "type": "string" },
                        "port": { "type": "integer", "minimum": 1, "maximum": 65535 }
                    },
                    "additionalProperties": false
                }
            },
            "required": ["protocol", "target"],
            "additionalProperties": false
        }),
        move |request| {
            let manager = manager.clone();
            Box::pin(async move {
                let args = parse_arguments::<DebugSessionOpenArgs>(&request.arguments)?;
                let output = manager.open(&request, args).await?;
                Ok(VulcanToolInvocationResult::from_model_output(output))
            })
        },
    )
}

/// Builds the protocol request tool definition.
/// 构建协议请求工具定义。
fn build_debug_session_request_tool(manager: DebugRuntimeManager) -> Arc<NativeToolDefinition> {
    build_native_tool_definition(
        debug_descriptor(
            DEBUG_SESSION_REQUEST_TOOL_ID,
            "Send one CDP method, DAP command, or DAP reverse-request response through an owned debug session.",
        ),
        json!({
            "type": "object",
            "properties": {
                "session_id": { "type": "string" },
                "method": { "type": "string" },
                "params": { "type": "object" },
                "command": { "type": "string" },
                "arguments": { "type": "object" },
                "request_seq": { "type": "integer", "minimum": 1 },
                "success": { "type": "boolean" },
                "body": { "type": "object" },
                "message": { "type": "string" },
                "timeout_ms": { "type": "integer", "minimum": 1, "maximum": MAX_DEBUG_REQUEST_TIMEOUT_MS }
            },
            "required": ["session_id"],
            "additionalProperties": false
        }),
        move |request| {
            let manager = manager.clone();
            Box::pin(async move {
                let args = parse_arguments::<DebugSessionRequestArgs>(&request.arguments)?;
                let output = manager.request(&request, args).await?;
                Ok(VulcanToolInvocationResult::from_model_output(output))
            })
        },
    )
}

/// Builds the retained protocol event polling tool definition.
/// 构建已保留协议事件轮询工具定义。
fn build_debug_session_events_tool(manager: DebugRuntimeManager) -> Arc<NativeToolDefinition> {
    build_native_tool_definition(
        debug_descriptor(
            DEBUG_SESSION_EVENTS_TOOL_ID,
            "Read a bounded page of retained events from an owned debug session.",
        ),
        json!({
            "type": "object",
            "properties": {
                "session_id": { "type": "string" },
                "max_events": { "type": "integer", "minimum": 1, "maximum": MAX_DEBUG_EVENT_PAGE_SIZE },
                "timeout_ms": { "type": "integer", "minimum": 0, "maximum": MAX_DEBUG_EVENT_WAIT_MS }
            },
            "required": ["session_id"],
            "additionalProperties": false
        }),
        move |request| {
            let manager = manager.clone();
            Box::pin(async move {
                let args = parse_arguments::<DebugSessionEventsArgs>(&request.arguments)?;
                let output = manager.events(&request, args).await?;
                Ok(VulcanToolInvocationResult::from_model_output(output))
            })
        },
    )
}

/// Builds the debug session status tool definition.
/// 构建调试会话状态工具定义。
fn build_debug_session_status_tool(manager: DebugRuntimeManager) -> Arc<NativeToolDefinition> {
    build_native_tool_definition(
        debug_descriptor(
            DEBUG_SESSION_STATUS_TOOL_ID,
            "Read transport and buffered-event status for an owned debug session.",
        ),
        session_id_schema(),
        move |request| {
            let manager = manager.clone();
            Box::pin(async move {
                let args = parse_arguments::<DebugSessionIdArgs>(&request.arguments)?;
                let output = manager.status(&request, args).await?;
                Ok(VulcanToolInvocationResult::from_model_output(output))
            })
        },
    )
}

/// Builds the deterministic debug session close tool definition.
/// 构建确定性调试会话关闭工具定义。
fn build_debug_session_close_tool(manager: DebugRuntimeManager) -> Arc<NativeToolDefinition> {
    build_native_tool_definition(
        debug_descriptor(
            DEBUG_SESSION_CLOSE_TOOL_ID,
            "Close an owned debug session and retire its managed runtime process.",
        ),
        session_id_schema(),
        move |request| {
            let manager = manager.clone();
            Box::pin(async move {
                let args = parse_arguments::<DebugSessionIdArgs>(&request.arguments)?;
                let output = manager.close(&request, args).await?;
                Ok(VulcanToolInvocationResult::from_model_output(output))
            })
        },
    )
}

/// Builds common metadata for one model-visible debug tool.
/// 为一个模型可见调试工具构建通用元数据。
fn debug_descriptor(id: &str, description: &str) -> NativeToolDescriptor {
    NativeToolDescriptor {
        id: String::from(id),
        name: String::from(id),
        description: String::from(description),
        help_text: Some(String::from(
            "Open one session first, use only the returned session_id, poll events when needed, and always close the session after debugging.",
        )),
        ai_visible: true,
        available_in_plan: false,
    }
}

/// Returns the strict shared schema used by status and close tools.
/// 返回状态与关闭工具使用的严格共享 schema。
fn session_id_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "session_id": { "type": "string" }
        },
        "required": ["session_id"],
        "additionalProperties": false
    })
}

/// Parses one exact native-tool argument object.
/// 解析一个精确的原生工具参数对象。
fn parse_arguments<T>(arguments: &Value) -> Result<T, NativeToolError>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(arguments.clone())
        .map_err(|error| NativeToolError::new(format!("invalid debug tool arguments: {error}")))
}

/// Requires the current controller session identity for ownership enforcement.
/// 要求当前控制器会话身份以执行所有权校验。
fn required_owner_session_id(
    request: &NativeToolExecutionRequest,
) -> Result<String, NativeToolError> {
    request
        .session_id
        .map(|session_id| session_id.to_string())
        .ok_or_else(|| NativeToolError::new("debug tools require a controller session id."))
}

/// Resolves and canonicalizes the current execution workspace root.
/// 解析并规范化当前执行工作区根目录。
fn canonical_workspace_root(
    request: &NativeToolExecutionRequest,
) -> Result<PathBuf, NativeToolError> {
    let workspace_root = request
        .workspace_root
        .as_ref()
        .ok_or_else(|| NativeToolError::new("debug tools require a workspace root."))?;
    fs::canonicalize(workspace_root).map_err(|error| {
        NativeToolError::new(format!(
            "failed to canonicalize debug workspace root `{}`: {error}",
            workspace_root.display()
        ))
    })
}

/// Validates the fixed System Plugin package and every executable source file.
/// 校验固定 System Plugin 包及其全部可执行源文件。
fn validate_debug_system_package(
    runtime: &LuaSkillsRuntimeHandle,
) -> Result<PathBuf, NativeToolError> {
    let system_root = fs::canonicalize(&runtime.system_lua_lib_dir).map_err(|error| {
        NativeToolError::new(format!(
            "debug System Plugin root is unavailable at `{}`: {error}",
            runtime.system_lua_lib_dir.display()
        ))
    })?;
    let package_root =
        fs::canonicalize(system_root.join(VULCAN_DEBUG_RUNTIME_SYSTEM_PLUGIN_ID)).map_err(|error| {
            NativeToolError::new(format!(
                "debug System Plugin package `{VULCAN_DEBUG_RUNTIME_SYSTEM_PLUGIN_ID}` is unavailable: {error}"
            ))
        })?;
    if !package_root.starts_with(&system_root) {
        return Err(NativeToolError::new(
            "debug System Plugin package escaped system_lua_lib.",
        ));
    }
    for relative_path in [
        DEBUG_SYSTEM_PACKAGE_MANIFEST,
        "runtime/plugin.lua",
        "node/sidecar.mjs",
        "node/package.json",
        "node/pnpm-lock.yaml",
    ] {
        let source = fs::canonicalize(package_root.join(relative_path)).map_err(|error| {
            NativeToolError::new(format!(
                "debug System Plugin file `{relative_path}` is unavailable: {error}"
            ))
        })?;
        if !source.starts_with(&package_root) || !source.is_file() {
            return Err(NativeToolError::new(format!(
                "debug System Plugin file `{relative_path}` is invalid."
            )));
        }
    }
    Ok(package_root)
}

/// Validates the protocol-specific local endpoint before a System lease is created.
/// 在创建 System 租约前校验协议专用本机端点。
fn validate_open_target(protocol: DebugProtocol, target: &Value) -> Result<(), NativeToolError> {
    let object = target
        .as_object()
        .ok_or_else(|| NativeToolError::new("target must be an object."))?;
    match protocol {
        DebugProtocol::Cdp => {
            let websocket_url = object
                .get("websocket_url")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| NativeToolError::new("target.websocket_url is required for CDP."))?;
            let endpoint = reqwest::Url::parse(websocket_url).map_err(|error| {
                NativeToolError::new(format!("target.websocket_url is invalid: {error}"))
            })?;
            let is_loopback = matches!(
                endpoint.host_str(),
                Some("127.0.0.1" | "localhost" | "::1" | "[::1]")
            );
            if endpoint.scheme() != "ws" || !is_loopback {
                return Err(NativeToolError::new(
                    "CDP websocket_url must use ws:// with a loopback host.",
                ));
            }
            if object.len() != 1 {
                return Err(NativeToolError::new(
                    "CDP target accepts only websocket_url.",
                ));
            }
        }
        DebugProtocol::Dap => {
            let host = object
                .get("host")
                .and_then(Value::as_str)
                .ok_or_else(|| NativeToolError::new("target.host is required for DAP."))?;
            if !matches!(host, "127.0.0.1" | "localhost" | "::1") {
                return Err(NativeToolError::new("DAP target.host must be loopback."));
            }
            let port = object
                .get("port")
                .and_then(Value::as_u64)
                .ok_or_else(|| NativeToolError::new("target.port is required for DAP."))?;
            bounded_timeout(port, 1, 65_535, "target.port")?;
            if object.len() != 2 {
                return Err(NativeToolError::new(
                    "DAP target accepts only host and port.",
                ));
            }
        }
    }
    Ok(())
}

/// Requires one present and non-empty protocol command field.
/// 要求一个存在且非空的协议命令字段。
fn required_non_empty(value: Option<String>, field: &str) -> Result<String, NativeToolError> {
    value
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| NativeToolError::new(format!("{field} is required for this protocol.")))
}

/// Validates one inclusive finite integer range.
/// 校验一个闭区间内的有限整数。
fn bounded_timeout(
    value: u64,
    minimum: u64,
    maximum: u64,
    field: &str,
) -> Result<u64, NativeToolError> {
    if value < minimum || value > maximum {
        return Err(NativeToolError::new(format!(
            "{field} must be between {minimum} and {maximum}."
        )));
    }
    Ok(value)
}

/// Combines an open/registration error with failed retirement and retention diagnostics.
/// 合并创建或登记错误、退休失败与兜底保留诊断。
fn combined_retirement_error(
    primary: NativeToolError,
    retirement: NativeToolError,
    retention: Result<(), NativeToolError>,
) -> NativeToolError {
    let retention_diagnostic = match retention {
        Ok(()) => "lease retained for manager-drop cleanup".to_string(),
        Err(error) => format!("failed to retain lease for cleanup: {}", error.message),
    };
    NativeToolError::new(format!(
        "{} Additionally failed to retire the System lease: {}; {retention_diagnostic}.",
        primary.message, retirement.message
    ))
}

/// Releases one previously reserved or active debug-session slot without underflow.
/// 在不发生下溢的前提下释放一个先前预留或活动的调试会话名额。
fn release_debug_session_slot(counter: &AtomicUsize) {
    let released = counter.fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
        (current > 0).then_some(current - 1)
    });
    debug_assert!(released.is_ok(), "debug session slot counter underflow");
}

/// Acquires the shared session table while preserving poison recovery diagnostics.
/// 获取共享会话表，并保留互斥锁中毒恢复诊断。
fn lock_sessions(
    sessions: &Mutex<BTreeMap<String, Arc<DebugRuntimeSession>>>,
) -> Result<MutexGuard<'_, BTreeMap<String, Arc<DebugRuntimeSession>>>, NativeToolError> {
    sessions
        .lock()
        .map_err(|_| NativeToolError::new("debug session registry lock is poisoned."))
}

/// Runs one synchronous LuaSkills engine operation outside the async executor thread.
/// 在异步执行器线程之外运行一个同步 LuaSkills 引擎操作。
async fn call_engine_blocking<T>(
    runtime: LuaSkillsRuntimeHandle,
    operation: impl FnOnce(&luaskills::LuaEngine) -> Result<T, NativeToolError> + Send + 'static,
) -> Result<T, NativeToolError>
where
    T: Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        let engine = runtime
            .engine
            .lock()
            .map_err(|_| NativeToolError::new("LuaSkills engine lock is poisoned."))?;
        operation(&engine)
    })
    .await
    .map_err(|error| NativeToolError::new(format!("debug runtime task failed: {error}")))?
}

/// Parses one LuaSkills JSON response and requires its top-level `ok` state.
/// 解析一条 LuaSkills JSON 响应并要求顶层 `ok` 状态。
fn parse_engine_response(raw: &str, operation: &str) -> Result<Value, NativeToolError> {
    let response: Value = serde_json::from_str(raw).map_err(|error| {
        NativeToolError::new(format!("failed to decode {operation} response: {error}"))
    })?;
    if response.get("ok").and_then(Value::as_bool) != Some(true) {
        let message = response
            .get("error")
            .or_else(|| response.get("message"))
            .map(render_error_value)
            .unwrap_or_else(|| response.to_string());
        return Err(NativeToolError::new(format!(
            "{operation} failed: {message}"
        )));
    }
    Ok(response)
}

/// Renders one structured or scalar runtime error without discarding upstream details.
/// 渲染一个结构化或标量运行时错误，同时不丢弃上游细节。
fn render_error_value(error: &Value) -> String {
    error
        .as_str()
        .map(String::from)
        .unwrap_or_else(|| error.to_string())
}

/// Decodes and validates one exact sidecar JSON frame nested in a System lease eval response.
/// 解码并校验 System 租约执行响应中嵌套的一条精确 sidecar JSON 帧。
fn parse_sidecar_result(response: &Value) -> Result<Value, NativeToolError> {
    // Exact JSON frame returned as text so Lua cannot alter nested container shapes.
    // 以文本返回的精确 JSON 帧，确保 Lua 无法改变嵌套容器形状。
    let frame = response
        .get("result")
        .and_then(Value::as_str)
        .ok_or_else(|| NativeToolError::new("System runtime eval returned no JSON frame."))?;
    // Structured sidecar envelope decoded only after returning to the Rust host.
    // 仅在返回 Rust 宿主后解码的结构化 sidecar 信封。
    let sidecar: Value = serde_json::from_str(frame).map_err(|error| {
        NativeToolError::new(format!(
            "failed to decode debug sidecar JSON frame: {error}"
        ))
    })?;
    if sidecar.get("ok").and_then(Value::as_bool) != Some(true) {
        let message = sidecar
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("unknown debug sidecar error");
        return Err(NativeToolError::new(format!(
            "debug sidecar request failed: {message}"
        )));
    }
    Ok(sidecar.get("result").cloned().unwrap_or(Value::Null))
}

/// Requires one non-empty string field from a successful engine response.
/// 从成功引擎响应中要求一个非空字符串字段。
fn required_response_string(response: &Value, field: &str) -> Result<String, NativeToolError> {
    response
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(String::from)
        .ok_or_else(|| NativeToolError::new(format!("System lease response omitted {field}.")))
}

/// Requires one unsigned integer field from a successful engine response.
/// 从成功引擎响应中要求一个无符号整数字段。
fn required_response_u64(response: &Value, field: &str) -> Result<u64, NativeToolError> {
    response
        .get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| NativeToolError::new(format!("System lease response omitted {field}.")))
}

/// Closes one LuaSkills System lease synchronously during manager drop.
/// 在管理器释放期间同步关闭一个 LuaSkills System 租约。
fn close_system_lease_sync(
    engine: &luaskills::LuaEngine,
    session: &DebugRuntimeSession,
) -> Result<Value, NativeToolError> {
    let raw = engine
        .close_system_runtime_lease_json(
            &json!({
                "lease_id": session.lease_id,
                "generation": session.generation,
            })
            .to_string(),
        )
        .map_err(NativeToolError::new)?;
    parse_engine_response(&raw, "close System runtime lease")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies failed reservations return slots and committed sessions enforce the hard limit.
    /// 验证失败预留会归还名额，已提交会话会执行硬上限。
    #[test]
    fn debug_session_slots_are_bounded_and_released() {
        let counter = AtomicUsize::new(0);
        let transient = DebugSessionSlotReservation::reserve(&counter)
            .expect("first debug session slot should be reserved");
        assert_eq!(counter.load(Ordering::Acquire), 1);
        drop(transient);
        assert_eq!(counter.load(Ordering::Acquire), 0);

        for _ in 0..MAX_ACTIVE_DEBUG_SESSIONS {
            DebugSessionSlotReservation::reserve(&counter)
                .expect("bounded debug session slot should be reserved")
                .commit();
        }
        assert!(DebugSessionSlotReservation::reserve(&counter).is_err());
        for _ in 0..MAX_ACTIVE_DEBUG_SESSIONS {
            release_debug_session_slot(&counter);
        }
        assert_eq!(counter.load(Ordering::Acquire), 0);
    }

    /// Rejects remote and path-confused CDP targets before starting a sidecar.
    /// 在启动 sidecar 前拒绝远程与路径混淆的 CDP 目标。
    #[test]
    fn cdp_target_requires_exact_loopback_websocket() {
        assert!(
            validate_open_target(
                DebugProtocol::Cdp,
                &json!({ "websocket_url": "ws://127.0.0.1:9222/devtools/page/1" })
            )
            .is_ok()
        );
        assert!(
            validate_open_target(
                DebugProtocol::Cdp,
                &json!({ "websocket_url": "wss://example.com/devtools/page/1" })
            )
            .is_err()
        );
    }

    /// Rejects non-loopback DAP targets and unexpected endpoint fields.
    /// 拒绝非回环 DAP 目标与意外端点字段。
    #[test]
    fn dap_target_requires_exact_loopback_tcp_endpoint() {
        assert!(
            validate_open_target(
                DebugProtocol::Dap,
                &json!({ "host": "127.0.0.1", "port": 4711 })
            )
            .is_ok()
        );
        assert!(
            validate_open_target(
                DebugProtocol::Dap,
                &json!({ "host": "192.168.1.2", "port": 4711 })
            )
            .is_err()
        );
    }

    /// Requires a successful nested sidecar envelope before exposing protocol data.
    /// 在暴露协议数据前要求成功的嵌套 sidecar 信封。
    #[test]
    fn sidecar_result_requires_nested_success() {
        assert_eq!(
            parse_sidecar_result(&json!({
                "ok": true,
                "result": r#"{"ok":true,"result":{"connected":true,"empty":{}}}"#
            }))
            .expect("successful sidecar response"),
            json!({ "connected": true, "empty": {} })
        );
        assert!(
            parse_sidecar_result(&json!({
                "ok": true,
                "result": r#"{"ok":false,"error":"connection failed"}"#
            }))
            .is_err()
        );
    }

    /// Exercises the real System Plugin through a fetched managed Node runtime when explicitly requested.
    /// 在显式请求时通过已拉取的受管 Node 运行时执行真实 System Plugin。
    #[test]
    #[ignore = "requires a prepared LuaSkills runtime with managed Node assets"]
    fn managed_node_system_plugin_dispatches_one_status_command() {
        let runtime_root = std::env::var_os("VULCAN_DEBUG_RUNTIME_SMOKE_ROOT")
            .map(PathBuf::from)
            .expect("VULCAN_DEBUG_RUNTIME_SMOKE_ROOT must be set");
        let package_root = runtime_root
            .join("system_lua_lib")
            .join(VULCAN_DEBUG_RUNTIME_SYSTEM_PLUGIN_ID);
        let workspace_root = std::env::temp_dir().join(format!(
            "vulcan-debug-system-plugin-workspace-{}",
            Uuid::new_v4()
        ));
        fs::create_dir_all(&workspace_root).expect("create isolated debug smoke-test workspace");
        let mut host_options =
            luaskills::LuaRuntimeHostOptions::with_runtime_root(runtime_root.clone());
        host_options.allow_network_download = false;
        let engine = luaskills::LuaEngine::new(luaskills::LuaEngineOptions::new(
            luaskills::LuaVmPoolConfig {
                min_size: 1,
                max_size: 1,
                idle_ttl_secs: 30,
            },
            host_options,
        ))
        .expect("create LuaSkills smoke-test engine");
        let created = parse_engine_response(
            &engine
                .create_system_runtime_lease_json(
                    &json!({
                        "sid": "vulcan-debug/smoke-test",
                        "ttl_sec": 60,
                        "cwd": normalized_workspace_path_text_from_path(&package_root),
                        "workspace_root": normalized_workspace_path_text_from_path(&workspace_root),
                        "system_package": {
                            "id": VULCAN_DEBUG_RUNTIME_SYSTEM_PLUGIN_ID,
                            "root": normalized_workspace_path_text_from_path(&package_root),
                            "dependencies_file": DEBUG_SYSTEM_PACKAGE_MANIFEST,
                        },
                    })
                    .to_string(),
                )
                .expect("create debug System lease"),
            "create debug System lease",
        )
        .expect("decode debug System lease");
        let lease_id = required_response_string(&created, "lease_id").expect("lease id");
        let generation = required_response_u64(&created, "generation").expect("generation");
        // Exact close command used to verify shutdown before any sidecar process exists.
        // 用于验证 sidecar 进程尚未存在时关闭行为的精确关闭命令。
        let unopened_close_payload = json!({
            "request_id": "smoke-unopened-close",
            "op": "close",
            "host_timeout_ms": DEBUG_HOST_TIMEOUT_GRACE_MS,
        });
        let unopened_close = parse_engine_response(
            &engine
                .eval_system_runtime_lease_json(
                    &json!({
                        "lease_id": lease_id.as_str(),
                        "generation": generation,
                        "code": DEBUG_SHUTDOWN_LUA,
                        "args": {
                            "payload_json": unopened_close_payload.to_string(),
                            "host_timeout_ms": DEBUG_HOST_TIMEOUT_GRACE_MS,
                        },
                    })
                    .to_string(),
                )
                .expect("shutdown unopened debug System Plugin"),
            "shutdown unopened debug System Plugin",
        )
        .expect("decode unopened debug System Plugin shutdown");
        assert_eq!(
            parse_sidecar_result(&unopened_close).expect("decode unopened debug sidecar shutdown")
                ["closed"],
            true
        );
        // Exact status command encoded before entering the Lua System lease.
        // 在进入 Lua System 租约前编码的精确状态命令。
        let status_payload = json!({
            "request_id": "smoke-1",
            "op": "status",
            "host_timeout_ms": DEBUG_HOST_TIMEOUT_GRACE_MS,
        });
        let evaluated = parse_engine_response(
            &engine
                .eval_system_runtime_lease_json(
                    &json!({
                        "lease_id": lease_id.as_str(),
                        "generation": generation,
                        "code": DEBUG_DISPATCH_LUA,
                        "args": {
                            "payload_json": status_payload.to_string(),
                            "host_timeout_ms": DEBUG_HOST_TIMEOUT_GRACE_MS,
                        },
                    })
                    .to_string(),
                )
                .expect("evaluate debug System lease"),
            "evaluate debug System lease",
        )
        .expect("decode debug System eval");
        let status = parse_sidecar_result(&evaluated).expect("decode debug sidecar status");
        assert_eq!(status["connected"], false);
        // Optional real CDP endpoint used to cover empty-object Debugger domain arguments.
        // 用于覆盖 Debugger 域空对象参数的可选真实 CDP 端点。
        if let Some(websocket_url) = std::env::var_os("VULCAN_DEBUG_RUNTIME_CDP_URL") {
            let websocket_url = websocket_url
                .into_string()
                .expect("VULCAN_DEBUG_RUNTIME_CDP_URL must be valid UTF-8");
            // Dispatches one request through the exact System lease bridge used by native tools.
            // 通过原生工具使用的完全相同 System 租约桥接分发一条请求。
            let dispatch = |request_id: &str, args: Value| {
                // Host timeout retained outside the opaque protocol JSON text.
                // 保留在不透明协议 JSON 文本之外的宿主超时时间。
                let host_timeout_ms = args
                    .get("host_timeout_ms")
                    .and_then(Value::as_u64)
                    .unwrap_or(30_000);
                parse_engine_response(
                    &engine
                        .eval_system_runtime_lease_json(
                            &json!({
                                "lease_id": lease_id.as_str(),
                                "generation": generation,
                                "code": DEBUG_DISPATCH_LUA,
                                "args": {
                                    "payload_json": args.to_string(),
                                    "host_timeout_ms": host_timeout_ms,
                                },
                            })
                            .to_string(),
                        )
                        .unwrap_or_else(|error| panic!("evaluate {request_id}: {error}")),
                    request_id,
                )
                .and_then(|response| parse_sidecar_result(&response))
                .unwrap_or_else(|error| panic!("decode {request_id}: {error}"))
            };
            let opened = dispatch(
                "open real CDP endpoint",
                json!({
                    "request_id": "smoke-open",
                    "op": "open",
                    "protocol": "cdp",
                    "target": { "websocket_url": websocket_url },
                    "host_timeout_ms": 30_000,
                }),
            );
            assert_eq!(opened["connected"], true);
            // Empty-object request and response proving both bridge directions preserve shape.
            // 用于证明桥接双向均保持形状的空对象请求与响应。
            let runtime_enabled = dispatch(
                "enable real CDP runtime",
                json!({
                    "request_id": "smoke-runtime-enable",
                    "op": "request",
                    "method": "Runtime.enable",
                    "params": {},
                    "timeout_ms": 30_000,
                    "host_timeout_ms": 35_000,
                }),
            );
            assert_eq!(runtime_enabled, json!({}));
            // Non-empty request confirming ordinary CDP argument values remain unchanged.
            // 用于确认普通 CDP 参数值保持不变的非空请求。
            let evaluated = dispatch(
                "evaluate real CDP expression",
                json!({
                    "request_id": "smoke-evaluate",
                    "op": "request",
                    "method": "Runtime.evaluate",
                    "params": { "expression": "1 + 2" },
                    "timeout_ms": 30_000,
                    "host_timeout_ms": 35_000,
                }),
            );
            assert_eq!(evaluated["result"]["value"], 3);
            let debugger = dispatch(
                "enable real CDP debugger",
                json!({
                    "request_id": "smoke-debugger-enable",
                    "op": "request",
                    "method": "Debugger.enable",
                    "params": {},
                    "timeout_ms": 30_000,
                    "host_timeout_ms": 35_000,
                }),
            );
            assert!(debugger["debuggerId"].as_str().is_some());
            // Initial runtime release required before testing explicit pause and resume transitions.
            // 测试显式暂停与恢复转换前所需的初始运行时释放。
            let waiting_released = dispatch(
                "release real CDP waiting debugger",
                json!({
                    "request_id": "smoke-run-if-waiting",
                    "op": "request",
                    "method": "Runtime.runIfWaitingForDebugger",
                    "params": {},
                    "timeout_ms": 30_000,
                    "host_timeout_ms": 35_000,
                }),
            );
            assert_eq!(waiting_released, json!({}));
            // Explicit debugger pause response used by the following resume regression assertion.
            // 供后续恢复回归断言使用的显式调试器暂停响应。
            let paused = dispatch(
                "pause real CDP debugger",
                json!({
                    "request_id": "smoke-debugger-pause",
                    "op": "request",
                    "method": "Debugger.pause",
                    "params": {},
                    "timeout_ms": 30_000,
                    "host_timeout_ms": 35_000,
                }),
            );
            assert_eq!(paused, json!({}));
            // Explicit debugger resume proving state-changing methods cross every bridge layer.
            // 用于证明状态变更方法能够跨越全部桥接层的显式调试器恢复响应。
            let resumed = dispatch(
                "resume real CDP debugger",
                json!({
                    "request_id": "smoke-debugger-resume",
                    "op": "request",
                    "method": "Debugger.resume",
                    "params": {},
                    "timeout_ms": 30_000,
                    "host_timeout_ms": 35_000,
                }),
            );
            assert_eq!(resumed, json!({}));
        }
        let closed = engine
            .close_system_runtime_lease_json(
                &json!({
                    "lease_id": lease_id.as_str(),
                    "generation": generation,
                })
                .to_string(),
            )
            .expect("close debug System lease");
        parse_engine_response(&closed, "close debug System lease")
            .expect("decode debug System close");
        fs::remove_dir_all(workspace_root).expect("remove isolated debug smoke-test workspace");
    }
}
