//! Owner-bound tool invocation, session admission, dispatch, closure, and idle retirement.
//! 所有者绑定工具调用、会话准入、分发、关闭与空闲退役。

use super::bridge::VulcanPluginBridgeLeaseScope;
use super::environment::plugin_execution_package_root;
use super::lease::{call_engine_blocking, parse_engine_response, required_lua_runtime};
use super::{
    ManagedVulcanPluginSession, VulcanPluginRuntimeError, VulcanPluginRuntimeManager, lock_sessions,
};
use crate::{
    VULCAN_PLUGIN_RESERVED_PAYLOAD_ARGUMENTS, VULCAN_PLUGIN_SESSION_ID_ARGUMENT,
    VulcanPluginRuntimeState, VulcanPluginToolDescriptor, VulcanPluginToolExecutionKind,
};
use serde_json::{Map, Value, json};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

/// One model tool invocation projected into the generic plugin runtime.
/// 投影到通用插件运行时的单次模型工具调用。
#[derive(Debug, Clone)]
pub struct VulcanPluginInvocationRequest {
    /// Exact plugin tool identifier.
    /// 精确的插件工具标识。
    pub tool_id: String,
    /// Raw JSON argument object validated by the provider tool schema.
    /// 已由 Provider 工具 Schema 校验的原始 JSON 参数对象。
    pub arguments: Value,
    /// Stable controller session that owns opened plugin sessions.
    /// 拥有已打开插件会话的稳定控制器会话。
    pub owner_session_id: String,
    /// Canonical workspace root exposed to the managed lease.
    /// 暴露给受管租约的规范化工作区根目录。
    pub workspace_root: PathBuf,
}

impl VulcanPluginRuntimeManager {
    /// Invokes one currently enabled plugin tool through the generic session lifecycle.
    /// 通过通用会话生命周期调用一个当前已启用的插件工具。
    ///
    /// # Parameters
    /// 参数
    /// `request` contains the tool id, model arguments, owner id, and workspace root.
    /// `request` 包含工具标识、模型参数、所有者标识和工作区根。
    ///
    /// # Returns
    /// 返回值
    /// Returns the plugin result with host session metadata when applicable.
    /// 返回插件结果，并在适用时包含宿主会话元数据。
    pub async fn invoke(
        &self,
        request: VulcanPluginInvocationRequest,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        let tool = self
            .snapshot()
            .enabled_tools
            .get(&request.tool_id)
            .cloned()
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new(format!(
                    "Vulcan Plugin tool `{}` is not enabled in the current catalog.",
                    request.tool_id
                ))
            })?;
        self.ensure_current_tool_generation(&tool)?;
        match tool.manifest.execution.kind {
            VulcanPluginToolExecutionKind::Stateless => self.invoke_stateless(&tool, request).await,
            VulcanPluginToolExecutionKind::SessionOpen => self.open_session(&tool, request).await,
            VulcanPluginToolExecutionKind::SessionCall => self.call_session(&tool, request).await,
            VulcanPluginToolExecutionKind::SessionClose => self.close_session(&tool, request).await,
        }
    }

    /// Invokes one tool through a tracked formal single-call instance, retaining ownership until actual retirement.
    /// 通过已跟踪的正式单次实例调用单个工具，保留所有权至实际退役。
    /// tool fixes the declared operation and request supplies owner/workspace arguments; returns the result or combined invocation/cleanup errors.
    /// tool固定声明的操作，request提供所有者/工作区参数；返回结果或合并的调用/清理错误。
    async fn invoke_stateless(
        &self,
        tool: &VulcanPluginToolDescriptor,
        request: VulcanPluginInvocationRequest,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        let arguments = require_argument_object(request.arguments)?;
        if arguments.contains_key(VULCAN_PLUGIN_SESSION_ID_ARGUMENT) {
            return Err(VulcanPluginRuntimeError::new(
                "A stateless Vulcan Plugin tool must not accept the host-owned session_id argument.",
            ));
        }
        let timeout_ms = resolve_host_timeout(tool, &arguments)?;
        let payload = plugin_payload(&tool.manifest.execution.operation, arguments)?;
        self.invoke_formal_stateless(
            tool,
            request.owner_session_id,
            request.workspace_root,
            payload,
            timeout_ms,
        )
        .await
    }

    /// Schedules idle cleanup without retaining the plugin manager past host shutdown.
    /// 调度空闲清理，同时不让插件管理器越过宿主关闭继续存活。
    /// session identifies the registered lease; schedules idle checks and cleanup, returning no value.
    /// session标识已注册租约；调度空闲检查与清理，不返回值。
    fn schedule_session_idle_reaper(&self, session: Arc<ManagedVulcanPluginSession>) {
        let Some(timeout_sec) = session
            .manifest
            .lua_runtime
            .as_ref()
            .and_then(|runtime| runtime.session_idle_timeout_sec)
        else {
            return;
        };
        let timeout_ms = timeout_sec.saturating_mul(1000);
        let weak_state = Arc::downgrade(&self.state);
        tokio::spawn(async move {
            loop {
                let last_activity = session.last_activity_unix_ms.load(Ordering::Acquire);
                let elapsed = current_unix_millis_u64().saturating_sub(last_activity);
                if elapsed < timeout_ms {
                    tokio::time::sleep(Duration::from_millis(
                        timeout_ms.saturating_sub(elapsed).max(1),
                    ))
                    .await;
                    continue;
                }
                let Some(state) = weak_state.upgrade() else {
                    return;
                };
                let manager = VulcanPluginRuntimeManager { state };
                let is_current = lock_sessions(&manager.state.sessions)
                    .ok()
                    .and_then(|sessions| sessions.get(&session.session_id).cloned())
                    .is_some_and(|current| Arc::ptr_eq(&current, &session));
                if !is_current || session.closing.load(Ordering::Acquire) {
                    return;
                }
                let refreshed_elapsed = current_unix_millis_u64()
                    .saturating_sub(session.last_activity_unix_ms.load(Ordering::Acquire));
                if refreshed_elapsed < timeout_ms {
                    continue;
                }
                match manager.drain_sessions(vec![Arc::clone(&session)]).await {
                    Ok(report) => {
                        for diagnostic in report.failures {
                            let _ = manager.record_cleanup_failure(&session.plugin_id, diagnostic);
                        }
                    }
                    Err(error) => {
                        let _ = manager.record_cleanup_failure(
                            &session.plugin_id,
                            format!(
                                "Session `{}` idle cleanup failed: {error}",
                                session.session_id
                            ),
                        );
                    }
                }
                return;
            }
        });
    }

    /// Opens and registers one new owner-bound managed session.
    /// 打开并注册一个新的所有者绑定受管会话。
    /// tool selects the declaration and request supplies owner/workspace; returns the registered session envelope or a compensated error.
    /// tool选择声明，request提供所有者/工作区；返回注册后的会话信封或经补偿的错误。
    async fn open_session(
        &self,
        tool: &VulcanPluginToolDescriptor,
        request: VulcanPluginInvocationRequest,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        let arguments = require_argument_object(request.arguments)?;
        if arguments.contains_key(VULCAN_PLUGIN_SESSION_ID_ARGUMENT) {
            return Err(VulcanPluginRuntimeError::new(
                "A session-open tool must not accept the host-owned session_id argument.",
            ));
        }
        let workspace_root = request.workspace_root.canonicalize().map_err(|error| {
            VulcanPluginRuntimeError::new(format!("Failed to resolve workspace root: {error}"))
        })?;
        let snapshot = self.snapshot();
        let plugin = snapshot.plugins.get(&tool.plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{}` is missing from the current catalog.",
                tool.plugin_id
            ))
        })?;
        let manifest = plugin.manifest.clone().ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{}` has no valid manifest.",
                tool.plugin_id
            ))
        })?;
        let lua_runtime = required_lua_runtime(&manifest)?;
        // executionRoot pins the opened plugin session to one trusted package generation.
        // executionRoot 把打开的插件会话固定到一个可信插件包代次。
        let execution_root = plugin_execution_package_root(&self.state, plugin)
            .map_err(VulcanPluginRuntimeError::new)?;
        let timeout_ms = resolve_host_timeout(tool, &arguments)?;
        let payload = plugin_payload(&tool.manifest.execution.operation, arguments)?;
        self.reserve_session_slot(&tool.plugin_id, lua_runtime.max_active_sessions)?;
        let session_id = Uuid::new_v4().to_string();
        let create_result = self
            .create_managed_lease(
                &session_id,
                &execution_root,
                &workspace_root,
                &manifest,
                VulcanPluginBridgeLeaseScope::Enabled,
            )
            .await;
        let (lease_id, sid, generation) = match create_result {
            Ok(lease) => lease,
            Err(error) => {
                self.release_session_slot(&tool.plugin_id);
                return Err(error);
            }
        };
        let session = Arc::new(ManagedVulcanPluginSession {
            session_id: session_id.clone(),
            plugin_id: tool.plugin_id.clone(),
            plugin_generation: tool.plugin_generation,
            owner_session_id: request.owner_session_id,
            lease_id,
            sid,
            generation,
            manifest,
            operation_gate: AsyncMutex::new(()),
            close_gate: AsyncMutex::new(()),
            closing: AtomicBool::new(false),
            last_activity_unix_ms: AtomicU64::new(current_unix_millis_u64()),
        });
        let open_result = self
            .dispatch(Arc::clone(&session), payload, timeout_ms, false)
            .await;
        let result = match open_result {
            Ok(result) => result,
            Err(error) => match self.close_lease(Arc::clone(&session)).await {
                Ok(_) => {
                    self.release_session_slot(&tool.plugin_id);
                    return Err(error);
                }
                Err(cleanup_error) => {
                    let diagnostic = format!(
                        "Vulcan Plugin session `{session_id}` open failed and lease cleanup failed; the lease remains retained: {cleanup_error}"
                    );
                    self.retain_failed_session_lease(&session, diagnostic.clone())?;
                    return Err(VulcanPluginRuntimeError::new(format!(
                        "{error}; {diagnostic}"
                    )));
                }
            },
        };
        session
            .last_activity_unix_ms
            .store(current_unix_millis_u64(), Ordering::Release);
        let registration = match self.state.session_registration_gate.lock() {
            Ok(_registration) => match self.ensure_session_runtime_current(&session) {
                Ok(()) => match lock_sessions(&self.state.sessions) {
                    Ok(sessions) if sessions.contains_key(&session_id) => {
                        Err(VulcanPluginRuntimeError::new(
                            "Generated Vulcan Plugin session id already exists.",
                        ))
                    }
                    Ok(mut sessions) => {
                        sessions.insert(session_id.clone(), Arc::clone(&session));
                        Ok(())
                    }
                    Err(error) => Err(error),
                },
                Err(error) => Err(error),
            },
            Err(_) => Err(VulcanPluginRuntimeError::new(
                "Vulcan Plugin session registration gate is poisoned.",
            )),
        };
        if let Err(error) = registration {
            match self.close_lease(Arc::clone(&session)).await {
                Ok(_) => {
                    self.release_session_slot(&tool.plugin_id);
                    return Err(error);
                }
                Err(cleanup_error) => {
                    let diagnostic = format!(
                        "Vulcan Plugin session `{session_id}` registration failed and lease cleanup failed; the lease remains retained: {cleanup_error}"
                    );
                    self.retain_failed_session_lease(&session, diagnostic.clone())?;
                    return Err(VulcanPluginRuntimeError::new(format!(
                        "{error}; {diagnostic}"
                    )));
                }
            }
        }
        self.schedule_session_idle_reaper(Arc::clone(&session));
        Ok(wrap_session_result(&session_id, result, false))
    }

    /// Calls one operation through an existing owner-bound session.
    /// 通过一个已有的所有者绑定会话调用操作。
    /// tool and request select an exact owner-bound operation; returns the host-wrapped result or stale/ownership error.
    /// tool与request选择精确的所有者绑定操作；返回宿主包装结果或陈旧/所有权错误。
    async fn call_session(
        &self,
        tool: &VulcanPluginToolDescriptor,
        request: VulcanPluginInvocationRequest,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        let mut arguments = require_argument_object(request.arguments)?;
        let session_id = take_session_id(&mut arguments)?;
        let session =
            self.owned_session(&session_id, &request.owner_session_id, &tool.plugin_id)?;
        if let Err(stale_error) = self.ensure_session_generation(tool, &session) {
            let report = self.drain_sessions(vec![Arc::clone(&session)]).await?;
            let diagnostic = if report.failures.is_empty() {
                stale_error.to_string()
            } else {
                format!("{stale_error}; {}", report.failures.join("; "))
            };
            return Err(VulcanPluginRuntimeError::new(diagnostic));
        }
        session
            .last_activity_unix_ms
            .store(current_unix_millis_u64(), Ordering::Release);
        let timeout_ms = resolve_host_timeout(tool, &arguments)?;
        let payload = plugin_payload(&tool.manifest.execution.operation, arguments)?;
        let result = self.dispatch(session, payload, timeout_ms, false).await?;
        Ok(wrap_session_result(&session_id, result, false))
    }

    /// Closes one existing owner-bound session and retires its managed lease.
    /// 关闭一个已有的所有者绑定会话并回收其受管租约。
    /// tool and request select the owned session; returns its close envelope or a retained-cleanup diagnostic.
    /// tool与request选择所属会话；返回关闭信封或保留清理诊断。
    async fn close_session(
        &self,
        tool: &VulcanPluginToolDescriptor,
        request: VulcanPluginInvocationRequest,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        let mut arguments = require_argument_object(request.arguments)?;
        let session_id = take_session_id(&mut arguments)?;
        if !arguments.is_empty() {
            return Err(VulcanPluginRuntimeError::new(
                "A session-close tool only accepts the host-owned session_id argument.",
            ));
        }
        let session =
            self.owned_session(&session_id, &request.owner_session_id, &tool.plugin_id)?;
        if self.ensure_session_generation(tool, &session).is_err() {
            let report = self.drain_sessions(vec![Arc::clone(&session)]).await?;
            if report.closed_sessions == 0 {
                return Err(VulcanPluginRuntimeError::new(format!(
                    "Stale Vulcan Plugin session `{session_id}` could not be closed: {}",
                    report.failures.join("; ")
                )));
            }
            return Ok(wrap_session_result(
                &session_id,
                json!({
                    "closed_stale_generation": true,
                    "diagnostics": report.failures,
                }),
                true,
            ));
        }
        let _close = session.close_gate.lock().await;
        let is_registered = lock_sessions(&self.state.sessions)?
            .get(&session_id)
            .is_some_and(|current| Arc::ptr_eq(current, &session));
        if !is_registered {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin session `{session_id}` was already closed concurrently."
            )));
        }
        session
            .closing
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| {
                VulcanPluginRuntimeError::new(format!(
                    "Vulcan Plugin session `{session_id}` is already closing."
                ))
            })?;
        let payload = plugin_payload(&tool.manifest.execution.operation, Map::new())?;
        let timeout_ms = resolve_host_timeout(tool, &Map::new())?;
        let shutdown_result = self
            .dispatch(Arc::clone(&session), payload, timeout_ms, true)
            .await;
        let close_result = self.close_lease(Arc::clone(&session)).await;
        if let Err(error) = close_result {
            session.closing.store(false, Ordering::Release);
            let diagnostic =
                format!("Session `{session_id}` lease cleanup failed and remains active: {error}");
            self.record_cleanup_failure(&tool.plugin_id, diagnostic.clone())?;
            return Err(VulcanPluginRuntimeError::new(diagnostic));
        }
        lock_sessions(&self.state.sessions)?.remove(&session_id);
        self.release_session_slot(&tool.plugin_id);
        let result = shutdown_result.unwrap_or_else(|error| {
            json!({
                "graceful_shutdown": false,
                "diagnostic": error.message,
            })
        });
        Ok(wrap_session_result(&session_id, result, true))
    }

    /// Resolves one session and enforces exact owner and plugin identity.
    /// 解析单个会话并执行精确的所有者与插件身份校验。
    /// session_id, owner_session_id, and plugin_id must match; returns the exact registered handle or an explicit ownership error.
    /// session_id、owner_session_id及plugin_id必须匹配；返回精确注册句柄或明确所有权错误。
    fn owned_session(
        &self,
        session_id: &str,
        owner_session_id: &str,
        plugin_id: &str,
    ) -> Result<Arc<ManagedVulcanPluginSession>, VulcanPluginRuntimeError> {
        let session = lock_sessions(&self.state.sessions)?
            .get(session_id)
            .cloned()
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new(format!(
                    "Vulcan Plugin session `{session_id}` does not exist."
                ))
            })?;
        if session.owner_session_id != owner_session_id {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin session `{session_id}` belongs to another conversation."
            )));
        }
        if session.plugin_id != plugin_id {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin session `{session_id}` belongs to plugin `{}`.",
                session.plugin_id
            )));
        }
        Ok(session)
    }

    /// Requires one retained session to match the current tool and plugin generation.
    /// 要求一个保留会话匹配当前工具与插件代次。
    /// tool and session supply frozen generations; returns unit only when the enabled catalog and session both match.
    /// tool与session提供冻结代次；仅当启用目录与会话均匹配时返回空值。
    fn ensure_session_generation(
        &self,
        tool: &VulcanPluginToolDescriptor,
        session: &ManagedVulcanPluginSession,
    ) -> Result<(), VulcanPluginRuntimeError> {
        self.ensure_current_tool_generation(tool)?;
        if session.plugin_generation != tool.plugin_generation {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin session `{}` belongs to a stale lifecycle generation.",
                session.session_id
            )));
        }
        Ok(())
    }

    /// Verifies one managed session still belongs to the currently enabled plugin generation.
    /// 验证单个受管会话是否仍属于当前已启用插件代次。
    /// session supplies plugin ownership/generation; returns unit only for the current enabled catalog identity.
    /// session提供插件归属/代次；仅当前启用目录身份匹配时返回空值。
    fn ensure_session_runtime_current(
        &self,
        session: &ManagedVulcanPluginSession,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let snapshot = self.snapshot();
        let plugin = snapshot.plugins.get(&session.plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{}` is no longer discovered.",
                session.plugin_id
            ))
        })?;
        if plugin.runtime_state != VulcanPluginRuntimeState::Enabled
            || plugin.generation != session.plugin_generation
        {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin session `{}` belongs to a stale or inactive lifecycle generation.",
                session.session_id
            )));
        }
        Ok(())
    }

    /// Dispatches one exact JSON payload through the plugin Lua bundle.
    /// 通过插件 Lua bundle 调度一个精确 JSON 载荷。
    ///
    /// # Parameters
    /// 参数
    /// `session` owns the lease; payload, timeout, and shutdown select the exact ABI call.
    /// `session` 持有租约；payload、timeout 与 shutdown 选择精确 ABI 调用。
    ///
    /// # Returns
    /// 返回值
    /// Returns the decoded plugin result.
    /// 返回解码后的插件结果。
    pub(super) async fn dispatch(
        &self,
        session: Arc<ManagedVulcanPluginSession>,
        mut payload: Value,
        host_timeout_ms: u64,
        shutdown: bool,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        let _operation = session.operation_gate.lock().await;
        if session.closing.load(Ordering::Acquire) && !shutdown {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin session `{}` is closing.",
                session.session_id
            )));
        }
        if !shutdown {
            self.ensure_session_runtime_current(&session)?;
        }
        let request_id = self.state.next_request_id.fetch_add(1, Ordering::Relaxed);
        let object = payload.as_object_mut().ok_or_else(|| {
            VulcanPluginRuntimeError::new("Vulcan Plugin payload must be a JSON object.")
        })?;
        object.insert(String::from("request_id"), json!(request_id.to_string()));
        object.insert(String::from("host_timeout_ms"), json!(host_timeout_ms));
        let payload_json = payload.to_string();
        let runtime = required_lua_runtime(&session.manifest)?;
        let function = if shutdown {
            runtime.shutdown_function.clone()
        } else {
            runtime.invoke_function.clone()
        };
        self.evaluate_bridge_function(
            &session.lease_id,
            &session.sid,
            session.generation,
            &function,
            json!({
                "payload_json": payload_json,
                "host_timeout_ms": host_timeout_ms,
            }),
            host_timeout_ms,
        )
        .await
    }

    /// Closes one LuaSkills lease after plugin shutdown is attempted.
    /// 在尝试插件关闭后释放单个 LuaSkills 租约。
    /// session supplies exact lease identity; returns the engine close envelope or an explicit failure.
    /// session提供精确租约身份；返回引擎关闭信封或明确失败。
    pub(super) async fn close_lease(
        &self,
        session: Arc<ManagedVulcanPluginSession>,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        let lease_id = session.lease_id.clone();
        let sid = session.sid.clone();
        let generation = session.generation;
        let engine = self.runtime_engine()?;
        call_engine_blocking(engine, move |engine| {
            let raw = engine
                .close_system_runtime_lease_json(
                    &json!({
                        "lease_id": lease_id,
                        "sid": sid,
                        "generation": generation,
                    })
                    .to_string(),
                )
                .map_err(VulcanPluginRuntimeError::new)?;
            parse_engine_response(&raw, "close Vulcan Plugin runtime lease")
        })
        .await
    }

    /// Reserves one plugin-scoped session slot under an exact hard limit.
    /// 在精确硬上限下预留一个插件作用域会话名额。
    /// plugin_id and maximum select the capacity bound; returns unit after reservation/publication or compensates on failure.
    /// plugin_id与maximum选择容量上限；预留/发布后返回空值，失败时执行补偿。
    pub(super) fn reserve_session_slot(
        &self,
        plugin_id: &str,
        maximum: usize,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let publication = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin session registration gate is poisoned.")
        })?;
        self.reserve_session_slot_locked(plugin_id, maximum, &publication)
    }

    /// Reserve `plugin_id` below `maximum` while the caller retains this manager's publication gate.
    /// 调用方保留此管理器发布门时，在 `maximum` 以内预留 `plugin_id`。
    /// `publication` proves the caller-held gate; return success only after count projection is published.
    /// `publication` 表示调用方持有的门；仅在计数投影发布后返回成功。
    pub(crate) fn reserve_session_slot_locked(
        &self,
        plugin_id: &str,
        maximum: usize,
        publication: &std::sync::MutexGuard<'_, ()>,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let mut counts = self.state.active_session_counts.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin session counter lock is poisoned.")
        })?;
        let count = counts.get(plugin_id).copied().unwrap_or_default();
        if count >= maximum {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{plugin_id}` reached its active session limit of {maximum}."
            )));
        }
        let active_count = count + 1;
        self.publish_active_session_count(plugin_id, active_count, publication)?;
        counts.insert(plugin_id.to_owned(), active_count);
        Ok(())
    }

    /// Releases one previously reserved plugin-scoped session slot.
    /// 释放一个此前预留的插件作用域会话名额。
    /// plugin_id selects one reserved count; decrements and publishes it without returning a value.
    /// plugin_id选择一项预留计数；递减并发布，不返回值。
    pub(super) fn release_session_slot(&self, plugin_id: &str) {
        let _ = self.release_session_slot_checked(plugin_id);
    }

    /// Release one known reservation for `plugin_id`, reporting metadata failures without changing its count.
    /// 释放 `plugin_id` 的一个已知预留；元数据失败时报告错误且不改变计数。
    /// Returns success only after the reduced count is published; missing ownership is an invariant error.
    /// 仅在发布减少后的计数后返回成功；缺失所有权是不变量错误。
    pub(crate) fn release_session_slot_checked(
        &self,
        plugin_id: &str,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Every reservation change takes publication before counters, preventing stale projections and nested gates.
        // 每次预留变化先取发布门再取计数锁，防止过期投影及嵌套门。
        let publication = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin session registration gate is poisoned.")
        })?;
        let mut counts = self.state.active_session_counts.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin session counter lock is poisoned.")
        })?;
        let active_count = counts
            .get(plugin_id)
            .and_then(|count| count.checked_sub(1))
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new(
                    "Vulcan Plugin active session reservation is missing.",
                )
            })?;
        self.publish_active_session_count(plugin_id, active_count, &publication)?;
        if active_count == 0 {
            counts.remove(plugin_id);
        } else {
            counts.insert(plugin_id.to_owned(), active_count);
        }
        Ok(())
    }

    /// Publishes one active-session count without changing plugin lifecycle generation.
    /// 发布单个活动会话计数，同时不改变插件生命周期代次。
    /// plugin_id selects the projection and active_session_count is authoritative; returns publication status without advancing generation.
    /// plugin_id选择投影，active_session_count是权威计数；返回发布状态，不推进代次。
    /// The supplied publication guard prevents nested acquisition and orders count changes with catalog changes.
    /// 传入发布守卫防止嵌套获取，并将计数变化与目录变化排序。
    fn publish_active_session_count(
        &self,
        plugin_id: &str,
        active_session_count: usize,
        _publication: &std::sync::MutexGuard<'_, ()>,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let mut snapshot = (*self.snapshot()).clone();
        let Some(plugin) = snapshot.plugins.get_mut(plugin_id) else {
            return Ok(());
        };
        plugin.active_session_count = active_session_count;
        snapshot.revision = self.state.next_revision.fetch_add(1, Ordering::Relaxed);
        let mut guard = self.state.snapshot.write().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin catalog lock is poisoned.")
        })?;
        *guard = Arc::new(snapshot);
        Ok(())
    }
}

/// Requires one exact JSON object from provider-visible arguments.
/// 从 Provider 可见参数中要求一个精确 JSON 对象。
/// arguments must be an object; returns its owned map or an explicit type error.
/// arguments必须为对象；返回其自有映射或明确类型错误。
fn require_argument_object(
    arguments: Value,
) -> Result<Map<String, Value>, VulcanPluginRuntimeError> {
    arguments.as_object().cloned().ok_or_else(|| {
        VulcanPluginRuntimeError::new("Vulcan Plugin tool arguments must be a JSON object.")
    })
}

/// Removes and validates the host-owned public session identifier.
/// 移除并校验宿主自有的公开会话标识。
/// Mutates arguments by removing the host field; returns the nonempty id or a missing-field error.
/// 从arguments移除宿主字段；返回非空标识或字段缺失错误。
fn take_session_id(arguments: &mut Map<String, Value>) -> Result<String, VulcanPluginRuntimeError> {
    arguments
        .remove(VULCAN_PLUGIN_SESSION_ID_ARGUMENT)
        .and_then(|value| value.as_str().map(str::to_owned))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| VulcanPluginRuntimeError::new("Vulcan Plugin session_id is required."))
}

/// Builds one exact plugin-owned payload without accepting an argument-level operation override.
/// 构建一个精确的插件自有载荷，同时拒绝参数层覆盖操作名。
/// operation is host-selected and arguments is plugin input; returns a typed object after rejecting reserved fields.
/// operation由宿主选择，arguments是插件输入；拒绝保留字段后返回强类型对象。
pub(super) fn plugin_payload(
    operation: &str,
    mut arguments: Map<String, Value>,
) -> Result<Value, VulcanPluginRuntimeError> {
    if let Some(reserved) = VULCAN_PLUGIN_RESERVED_PAYLOAD_ARGUMENTS
        .iter()
        .find(|reserved| arguments.contains_key(**reserved))
    {
        return Err(VulcanPluginRuntimeError::new(format!(
            "Vulcan Plugin tool arguments must not contain host-owned field `{reserved}`."
        )));
    }
    arguments.insert(String::from("op"), Value::String(operation.to_owned()));
    Ok(Value::Object(arguments))
}

/// Resolves one bounded host timeout from the tool manifest and exact argument object.
/// 根据工具清单与精确参数对象解析一个有界宿主超时。
/// tool provides timeout policy and arguments may select its declared limit; returns checked milliseconds or an explicit policy/overflow error.
/// tool提供超时策略，arguments可选择声明的限值；返回经检查的毫秒值或明确策略/溢出错误。
fn resolve_host_timeout(
    tool: &VulcanPluginToolDescriptor,
    arguments: &Map<String, Value>,
) -> Result<u64, VulcanPluginRuntimeError> {
    let execution = &tool.manifest.execution;
    let operation_timeout = match (&execution.timeout_argument, execution.max_timeout_ms) {
        (Some(argument), Some(maximum)) => match arguments.get(argument) {
            Some(value) => {
                let selected = value.as_u64().ok_or_else(|| {
                    VulcanPluginRuntimeError::new(format!(
                        "Vulcan Plugin timeout argument `{argument}` must be an unsigned integer."
                    ))
                })?;
                if selected > maximum {
                    return Err(VulcanPluginRuntimeError::new(format!(
                        "Vulcan Plugin timeout argument `{argument}` exceeds {maximum} ms."
                    )));
                }
                selected
            }
            None => execution.timeout_ms,
        },
        (None, None) => execution.timeout_ms,
        _ => {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin tool `{}` has an invalid timeout manifest.",
                tool.id
            )));
        }
    };
    operation_timeout
        .checked_add(execution.timeout_grace_ms)
        .ok_or_else(|| VulcanPluginRuntimeError::new("Vulcan Plugin host timeout overflowed."))
}

/// Wraps one plugin-owned result with stable host session metadata.
/// 使用稳定宿主会话元数据包装一个插件自有结果。
/// Wraps result using the exact session_id and closed flag; returns the unchanged result under host metadata.
/// 使用精确session_id与closed标志包装result；返回宿主元数据下未经修改的结果。
fn wrap_session_result(session_id: &str, result: Value, closed: bool) -> Value {
    json!({
        "session_id": session_id,
        "closed": closed,
        "result": result,
    })
}

/// Returns a saturating Unix timestamp in milliseconds for idle-session accounting.
/// 返回用于空闲会话计时的饱和 Unix 毫秒时间戳。
/// Takes no arguments; returns saturated u64 milliseconds, or zero before the Unix epoch.
/// 不接收参数；返回饱和u64毫秒值，Unix纪元前返回零。
fn current_unix_millis_u64() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
