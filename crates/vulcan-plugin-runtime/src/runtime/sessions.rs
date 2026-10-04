//! Owner-bound tool invocation, session admission, dispatch, closure, and idle retirement.
//! 所有者绑定工具调用、会话准入、分发、关闭与空闲退役。

use super::{VulcanPluginRuntimeError, VulcanPluginRuntimeManager};
use crate::{
    VULCAN_PLUGIN_RESERVED_PAYLOAD_ARGUMENTS, VULCAN_PLUGIN_SESSION_ID_ARGUMENT,
    VulcanPluginToolDescriptor, VulcanPluginToolExecutionKind,
};
use serde_json::{Map, Value};
use std::path::PathBuf;

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
        let timeout_ms = resolve_host_timeout(tool, &arguments)?;
        let payload = plugin_payload(&tool.manifest.execution.operation, arguments)?;
        self.open_formal_session(
            tool,
            request.owner_session_id,
            request.workspace_root,
            payload,
            timeout_ms,
        )
        .await
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
        let timeout_ms = resolve_host_timeout(tool, &arguments)?;
        let payload = plugin_payload(&tool.manifest.execution.operation, arguments)?;
        self.call_formal_session(
            tool,
            &session_id,
            &request.owner_session_id,
            payload,
            timeout_ms,
        )
        .await
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
        let timeout_ms = resolve_host_timeout(tool, &Map::new())?;
        self.close_formal_session(tool, &session_id, &request.owner_session_id, timeout_ms)
            .await
    }

    /// Reserve `plugin_id` below `maximum` while the caller retains this manager's publication gate.
    /// 调用方保留此管理器发布门时，在 `maximum` 以内预留 `plugin_id`。
    /// `publication` proves the caller-held gate; return success after updating the sole live counter.
    /// `publication` 表示调用方持有的门；更新唯一实时计数后返回成功。
    pub(crate) fn reserve_session_slot_locked(
        &self,
        plugin_id: &str,
        maximum: usize,
        _publication: &std::sync::MutexGuard<'_, ()>,
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
        counts.insert(plugin_id.to_owned(), active_count);
        Ok(())
    }

    /// Release one known reservation for `plugin_id`, reporting accounting failures without changing its count.
    /// 释放 `plugin_id` 的一个已知预留；记账失败时报告错误且不改变计数。
    /// Returns success after reducing the sole live counter; missing ownership is an invariant error.
    /// 减少唯一实时计数后返回成功；缺失所有权是不变量错误。
    pub(crate) fn release_session_slot_checked(
        &self,
        plugin_id: &str,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Every reservation change takes publication before counters, preventing stale projections and nested gates.
        // 每次预留变化先取发布门再取计数锁，防止过期投影及嵌套门。
        let publication = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin session registration gate is poisoned.")
        })?;
        self.release_session_slot_with_publication(plugin_id, &publication)
    }

    /// Release the known `plugin_id` reservation while the caller owns this manager's publication gate.
    /// 调用方拥有此管理器发布门时，释放已知 `plugin_id` 预留。
    /// Returns explicit accounting failures without nested gate acquisition or count mutation.
    /// 明确返回记账失败，不嵌套获取发布门，也不改变失败计数。
    pub(crate) fn release_session_slot_with_publication(
        &self,
        plugin_id: &str,
        _publication: &std::sync::MutexGuard<'_, ()>,
    ) -> Result<(), VulcanPluginRuntimeError> {
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
        if active_count == 0 {
            counts.remove(plugin_id);
        } else {
            counts.insert(plugin_id.to_owned(), active_count);
        }
        Ok(())
    }

    /// Read the sole live reservation count without acquiring the registration gate again.
    /// 读取唯一实时预留计数，不再次获取登记门。
    /// `plugin_id` selects the namespace; returns its count or an explicit poisoned-lock error.
    /// `plugin_id` 选择命名空间；返回其计数或明确锁污染错误。
    pub fn active_session_count(&self, plugin_id: &str) -> Result<usize, VulcanPluginRuntimeError> {
        self.state.active_session_count(plugin_id)
    }
}

impl super::VulcanPluginManagerState {
    /// Read actual reservations from their unique map while preserving the caller's lock scope.
    /// 从唯一映射读取实际预留，保持调用方锁作用域。
    /// `plugin_id` selects the namespace; absence means zero, and poisoned accounting fails explicitly.
    /// `plugin_id` 选择命名空间；缺失表示零，记账锁污染明确失败。
    pub(super) fn active_session_count(
        &self,
        plugin_id: &str,
    ) -> Result<usize, VulcanPluginRuntimeError> {
        self.active_session_counts
            .lock()
            .map(|counts| counts.get(plugin_id).copied().unwrap_or_default())
            .map_err(|_| {
                VulcanPluginRuntimeError::new("Vulcan Plugin session counter lock is poisoned.")
            })
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
pub(super) fn resolve_host_timeout(
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

#[cfg(test)]
mod tests;
