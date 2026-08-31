//! Provider-tool invocation orchestration within one frozen operation budget.
//! 同一冻结操作预算内的供应商工具调用编排。

use super::media::{
    RuntimeMediaUploadProgressContext, RuntimeMediaUploadTerminalGuard,
    prepare_provider_tool_media_upload, provider_tool_materialized_reference,
    provider_tool_media_resource_kind, provider_tool_media_upload_progress_context,
};
use super::tool_diagnostics::{
    diagnose_provider_tool_failure, diagnose_provider_tool_failure_with_secret,
};
use super::{PublicAiRuntime, current_unix_millis, remaining_provider_operation_timeout};
use crate::{
    PersistedAiConfiguration, ProviderToolCapabilityKind, ProviderToolExecutionResult,
    ProviderToolTargetSelection,
};
use std::path::Path;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

/// Host-owned deadline for legacy code targets that have no Manifest operation declaration.
/// 没有 Manifest operation 声明的旧代码目标所使用的宿主持有截止时间。
const LEGACY_PROVIDER_TOOL_OPERATION_TIMEOUT: Duration = Duration::from_secs(60);

impl PublicAiRuntime {
    /// Executes the one exact target selected for a semantic capability.
    /// 执行某项语义能力唯一选中的精确目标。
    ///
    /// # Parameters
    /// 参数
    /// `configuration`, `capability`, and `arguments` are frozen invocation inputs.
    /// `configuration`、`capability` 与 `arguments` 是冻结的调用输入。
    ///
    /// # Returns
    /// 返回值
    /// A unified typed provider tool result.
    /// 返回统一强类型供应商工具结果。
    pub async fn execute_selected_provider_tool(
        &self,
        configuration: &PersistedAiConfiguration,
        capability: ProviderToolCapabilityKind,
        arguments: serde_json::Value,
    ) -> Result<ProviderToolExecutionResult, String> {
        self.execute_selected_provider_tool_for_owner(configuration, capability, arguments, None)
            .await
    }

    /// Executes one selected provider tool while retaining its optional session ownership.
    /// 执行一个已选供应商工具，同时保留其可选会话所有权。
    ///
    /// # Parameters
    /// 参数
    /// `configuration`, `capability`, and `arguments` are frozen invocation inputs;
    /// `owner_session_id` is retained only in non-secret asynchronous task recovery metadata.
    /// `configuration`、`capability` 与 `arguments` 是冻结调用输入；
    /// `owner_session_id` 仅保留在非秘密异步任务恢复元数据中。
    ///
    /// # Returns
    /// 返回值
    /// One normalized terminal result, including recovered asynchronous completion.
    /// 返回一个归一化终态结果，包括恢复后的异步完成结果。
    pub async fn execute_selected_provider_tool_for_owner(
        &self,
        configuration: &PersistedAiConfiguration,
        capability: ProviderToolCapabilityKind,
        arguments: serde_json::Value,
        owner_session_id: Option<String>,
    ) -> Result<ProviderToolExecutionResult, String> {
        self.execute_selected_provider_tool_for_owner_in_workspace(
            configuration,
            capability,
            arguments,
            owner_session_id,
            None,
        )
        .await
    }

    /// Executes one selected provider tool with an explicit host workspace boundary.
    /// 使用显式宿主工作区边界执行一个已选供应商工具。
    ///
    /// # Parameters
    /// 参数
    /// `configuration`, `capability`, and `arguments` freeze the target invocation;
    /// `owner_session_id` owns recoverable work and `workspace_root` bounds local resources.
    /// `configuration`、`capability` 与 `arguments` 冻结目标调用；`owner_session_id` 持有
    /// 可恢复任务，`workspace_root` 约束本地资源。
    ///
    /// # Returns
    /// 返回值
    /// One normalized terminal result after workspace-bounded upload materialization.
    /// 在工作区有界上传物化后返回一个归一化终态结果。
    pub async fn execute_selected_provider_tool_for_owner_in_workspace(
        &self,
        configuration: &PersistedAiConfiguration,
        capability: ProviderToolCapabilityKind,
        arguments: serde_json::Value,
        owner_session_id: Option<String>,
        workspace_root: Option<&Path>,
    ) -> Result<ProviderToolExecutionResult, String> {
        let selection = configuration
            .provider_tool_selections
            .get(&capability)
            .cloned()
            .ok_or_else(|| {
                format!("No provider tool target is selected for capability `{capability:?}`.")
            })?;
        self.execute_provider_tool_target_for_owner_in_workspace(
            configuration,
            capability,
            &selection,
            arguments,
            owner_session_id,
            workspace_root,
        )
        .await
    }

    /// Executes one exact provider-tool target without changing the persisted default selection.
    /// 在不修改持久化默认选择的情况下执行一个精确供应商工具目标。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` owns the current connection graph, `capability` and `selection` freeze the
    /// exact target, `arguments` match that target's schema, and `owner_session_id` optionally binds
    /// a recoverable asynchronous task to one session.
    /// `configuration` 持有当前连接图，`capability` 与 `selection` 固定精确目标，
    /// `arguments` 匹配该目标的 Schema，`owner_session_id` 可选地把可恢复异步任务绑定到会话。
    ///
    /// # Returns
    /// 返回值
    /// The normalized provider-tool result or an explicit resolution, credential, or execution error.
    /// 返回规范化供应商工具结果，或明确的解析、凭据与执行错误。
    pub async fn execute_provider_tool_target_for_owner(
        &self,
        configuration: &PersistedAiConfiguration,
        capability: ProviderToolCapabilityKind,
        selection: &ProviderToolTargetSelection,
        arguments: serde_json::Value,
        owner_session_id: Option<String>,
    ) -> Result<ProviderToolExecutionResult, String> {
        self.execute_provider_tool_target_for_owner_in_workspace(
            configuration,
            capability,
            selection,
            arguments,
            owner_session_id,
            None,
        )
        .await
    }

    /// Executes one exact provider-tool target inside an explicit local-resource workspace.
    /// 在显式本地资源工作区内执行一个精确供应商工具目标。
    ///
    /// # Parameters
    /// 参数
    /// `configuration`, `capability`, `selection`, and `arguments` freeze the invocation;
    /// `owner_session_id` owns recoverable tasks and `workspace_root` bounds every local path.
    /// `configuration`、`capability`、`selection` 与 `arguments` 冻结调用；`owner_session_id`
    /// 持有可恢复任务，`workspace_root` 约束每条本地路径。
    ///
    /// # Returns
    /// 返回值
    /// The normalized result or a controlled resolution, upload, credential, or execution error.
    /// 返回归一化结果，或受控的解析、上传、凭据与执行错误。
    pub async fn execute_provider_tool_target_for_owner_in_workspace(
        &self,
        configuration: &PersistedAiConfiguration,
        capability: ProviderToolCapabilityKind,
        selection: &ProviderToolTargetSelection,
        arguments: serde_json::Value,
        owner_session_id: Option<String>,
        workspace_root: Option<&Path>,
    ) -> Result<ProviderToolExecutionResult, String> {
        if !capability.is_ai_visible() {
            return Err(format!(
                "Capability `{capability:?}` is internal inference and cannot execute as an AI-visible provider tool."
            ));
        }
        let resolved = self.resolve_provider_tool_target(configuration, capability, selection)?;
        let diagnostic_context = resolved.diagnostic_context();
        // operationTimeout is Manifest-owned for declarative operations; legacy code targets use
        // one explicit host ceiling instead of inheriting the streaming client's unlimited body.
        // operationTimeout 对声明式操作由 Manifest 持有；旧代码目标使用显式宿主上限，
        // 而不是继承流式客户端的无限正文时长。
        let operation_timeout = match resolved.operation.as_ref() {
            Some(operation) => Duration::from_millis(operation.timeout_ms.ok_or_else(|| {
                diagnose_provider_tool_failure(
                    format!(
                        "Provider operation `{}` has no required timeout_ms.",
                        operation.operation_id
                    ),
                    diagnostic_context.as_str(),
                )
            })?),
            None => LEGACY_PROVIDER_TOOL_OPERATION_TIMEOUT,
        };
        let operation_deadline =
            Instant::now()
                .checked_add(operation_timeout)
                .ok_or_else(|| {
                    diagnose_provider_tool_failure(
                        "Provider operation monotonic deadline overflowed.".to_owned(),
                        diagnostic_context.as_str(),
                    )
                })?;
        let operation_deadline_unix_ms = current_unix_millis()?
            .checked_add(operation_timeout.as_millis() as u64)
            .ok_or_else(|| {
                diagnose_provider_tool_failure(
                    "Provider operation persisted deadline overflowed.".to_owned(),
                    diagnostic_context.as_str(),
                )
            })?;
        let arguments = crate::provider_tool_execution::canonicalize_provider_tool_arguments(
            &resolved.target,
            arguments,
        )
        .map_err(|error| diagnose_provider_tool_failure(error, diagnostic_context.as_str()))?;
        let secret = tokio::time::timeout(
            remaining_provider_operation_timeout(
                operation_deadline,
                "Provider tool credential resolution",
            )?,
            self.resolve_fresh_execution_secret(
                resolved.connection.id.as_str(),
                resolved.connection.secret_reference.as_str(),
            ),
        )
        .await
        .map_err(|_| {
            diagnose_provider_tool_failure(
                "Provider tool credential resolution exceeded the frozen operation timeout."
                    .to_owned(),
                diagnostic_context.as_str(),
            )
        })?
        .map_err(|error| diagnose_provider_tool_failure(error, diagnostic_context.as_str()))?;
        // Model-extension understanding tools have no conversation ledger; their upload manager
        // reuses a provider object only for the same modality, source bytes, and upload contract.
        // 模型扩展理解工具没有会话账本；上传管理器只按相同模态、源字节与上传契约复用供应商对象。
        let mut arguments = arguments;
        let mut operation_for_upload = resolved.operation.clone();
        let mut media_upload_preflight = None;
        if provider_tool_media_resource_kind(capability).is_some()
            && let Some(operation) = operation_for_upload.as_ref()
            && !operation.upload_bindings.is_empty()
        {
            let credential_profile = resolved
                .entry
                .credential_profiles
                .iter()
                .find(|profile| profile.id == resolved.connection.credential_profile_id)
                .ok_or_else(|| {
                    diagnose_provider_tool_failure(
                        format!(
                            "Provider tool credential profile `{}` is absent from service entry `{}`.",
                            resolved.connection.credential_profile_id, resolved.entry.id
                        ),
                        diagnostic_context.as_str(),
                    )
                })?;
            let preflight = tokio::time::timeout(
                remaining_provider_operation_timeout(
                    operation_deadline,
                    "Provider tool upload cache preflight",
                )?,
                prepare_provider_tool_media_upload(
                    &self.execution_client,
                    &resolved,
                    operation,
                    &arguments,
                    credential_profile,
                    workspace_root,
                    capability,
                ),
            )
            .await
            .map_err(|_| {
                diagnose_provider_tool_failure(
                    "Provider tool upload cache preflight exceeded the frozen operation timeout."
                        .to_owned(),
                    diagnostic_context.as_str(),
                )
            })?
            .map_err(|error| diagnose_provider_tool_failure(error, diagnostic_context.as_str()))?;
            if let Some((preflight, cached_reference)) = preflight {
                if let Some(cached_reference) = cached_reference {
                    arguments = crate::upload_engine::materialize_cached_scalar_operation_upload(
                        &preflight.binding,
                        &preflight.inspection.resource,
                        arguments,
                        cached_reference.provider_reference,
                        cached_reference.reference_encoding,
                    )
                    .map_err(|error| {
                        diagnose_provider_tool_failure(error, diagnostic_context.as_str())
                    })?;
                    operation_for_upload
                        .as_mut()
                        .ok_or_else(|| {
                            diagnose_provider_tool_failure(
                                "Media upload cache hit lost its owning operation.".to_owned(),
                                diagnostic_context.as_str(),
                            )
                        })?
                        .upload_bindings
                        .clear();
                } else {
                    media_upload_preflight = Some(preflight);
                }
            }
        }
        let arguments = match operation_for_upload.as_ref() {
            Some(operation) if !operation.upload_bindings.is_empty() => {
                // credentialProfile is shared by upload authorization and exact progress routing.
                // credentialProfile 由上传鉴权与精确进度路由共同使用。
                let credential_profile = resolved
                    .entry
                    .credential_profiles
                    .iter()
                    .find(|profile| profile.id == resolved.connection.credential_profile_id)
                    .ok_or_else(|| {
                        diagnose_provider_tool_failure(
                            format!(
                                "Provider tool credential profile `{}` is absent from service entry `{}`.",
                                resolved.connection.credential_profile_id, resolved.entry.id
                            ),
                            diagnostic_context.as_str(),
                        )
                    })?;
                // observer is cloned before upload so callbacks never retain the runtime registry lock.
                // observer 在上传前克隆，确保回调不会占用运行时注册表锁。
                let observer = self
                    .runtime_media_upload_progress_observer
                    .read()
                    .map_err(|_| {
                        diagnose_provider_tool_failure(
                            "The runtime media upload observer lock is poisoned.".to_owned(),
                            diagnostic_context.as_str(),
                        )
                    })?
                    .clone();
                // progressContext is present only for one owned AI-visible media understanding upload.
                // progressContext 仅在存在所有者的 AI 可见媒体理解上传中存在。
                let progress_context = provider_tool_media_upload_progress_context(
                    observer,
                    owner_session_id.as_deref(),
                    capability,
                    &resolved,
                    operation,
                    &arguments,
                    credential_profile,
                    workspace_root,
                )
                .await
                .map_err(|error| {
                    diagnose_provider_tool_failure(error, diagnostic_context.as_str())
                })?;
                if let Some(context) = progress_context.as_ref() {
                    context.emit(
                        crate::RuntimeMediaUploadPhase::Preparing,
                        0,
                        context.base.total_bytes,
                        None,
                    );
                }
                // progressTerminalGuard emits cancellation when the owning tool future is dropped.
                // progressTerminalGuard 在所属工具 Future 被丢弃时发出取消事件。
                let mut progress_terminal_guard =
                    RuntimeMediaUploadTerminalGuard::new(progress_context.as_ref());
                // progress forwards byte facts from the exact generic upload engine invocation.
                // progress 从精确的通用上传引擎调用转发字节事实。
                let progress = progress_context
                    .as_ref()
                    .map(RuntimeMediaUploadProgressContext::byte_callback);
                // uploadCancellation owns this single synchronous provider-tool upload attempt.
                // uploadCancellation 持有本次同步供应商工具上传尝试。
                let upload_cancellation = crate::MediaUploadCancellation::new();
                // uploadResult retains terminal failure so the UI receives a failed replacement.
                // uploadResult 保留终态失败，使界面收到失败替换事件。
                let upload_result = tokio::time::timeout(
                    remaining_provider_operation_timeout(
                        operation_deadline,
                        "Provider tool upload materialization",
                    )?,
                    crate::upload_engine::materialize_operation_uploads(
                        crate::upload_engine::OperationUploadExecutionContext {
                            http: &self.execution_client.http,
                            entry: &resolved.entry,
                            secret: &secret,
                            credential_profile,
                            channels: &resolved.upload_channels,
                            progress,
                            cancellation: &upload_cancellation,
                            model_id: None,
                            workspace_root,
                            fixture_loopback_origin: None,
                        },
                        operation,
                        arguments,
                    ),
                )
                .await;
                let upload_result = match upload_result {
                    Ok(Ok(arguments)) => Ok(arguments),
                    Ok(Err(error)) => Err(diagnose_provider_tool_failure_with_secret(
                        error,
                        diagnostic_context.as_str(),
                        &secret,
                    )),
                    Err(_) => Err(diagnose_provider_tool_failure(
                        "Provider tool upload materialization exceeded the frozen operation timeout."
                            .to_owned(),
                        diagnostic_context.as_str(),
                    )),
                };
                match upload_result {
                    Ok(arguments) => {
                        if let Some(preflight) = media_upload_preflight.as_ref() {
                            let local_path = preflight
                                .inspection
                                .resource
                                .local_path
                                .as_ref()
                                .ok_or_else(|| {
                                    diagnose_provider_tool_failure(
                                        "Media upload cache preflight lost its local source path."
                                            .to_owned(),
                                        diagnostic_context.as_str(),
                                    )
                                })?;
                            let provider_reference = provider_tool_materialized_reference(
                                &arguments,
                                &preflight.binding,
                            )
                            .map_err(|error| {
                                diagnose_provider_tool_failure(error, diagnostic_context.as_str())
                            })?;
                            self.execution_client
                                .remember_provider_tool_media_reference(
                                    resolved.connection.provider_id.as_str(),
                                    resolved.entry.id.as_str(),
                                    resolved.connection.id.as_str(),
                                    &preflight.inspection.channel,
                                    local_path,
                                    preflight.resource_kind,
                                    preflight.inspection.resource.mime_type.as_str(),
                                    &preflight.source_identity,
                                    provider_reference,
                                    preflight.first_used_at_unix_millis,
                                )
                                .await
                                .map_err(|error| {
                                    diagnose_provider_tool_failure_with_secret(
                                        error,
                                        diagnostic_context.as_str(),
                                        &secret,
                                    )
                                })?;
                        }
                        if let Some(context) = progress_context.as_ref() {
                            context.emit(
                                crate::RuntimeMediaUploadPhase::Completed,
                                context.base.total_bytes.unwrap_or_default(),
                                context.base.total_bytes,
                                None,
                            );
                        }
                        progress_terminal_guard.disarm();
                        arguments
                    }
                    Err(error) => {
                        if let Some(context) = progress_context.as_ref() {
                            context.emit(
                                crate::RuntimeMediaUploadPhase::Failed,
                                context.last_bytes_sent.load(Ordering::Acquire),
                                context.base.total_bytes,
                                Some(error.clone()),
                            );
                        }
                        progress_terminal_guard.disarm();
                        return Err(error);
                    }
                }
            }
            _ => arguments,
        };
        if resolved.target.execution.execution_kind
            == crate::ProviderToolExecutionKind::HttpJsonAsyncPoll
        {
            let execution_id = tokio::time::timeout(
                remaining_provider_operation_timeout(
                    operation_deadline,
                    "Provider asynchronous task start",
                )?,
                self.start_provider_async_task(
                    resolved,
                    selection.clone(),
                    arguments,
                    secret,
                    owner_session_id,
                    operation_deadline_unix_ms,
                ),
            )
            .await
            .map_err(|_| {
                diagnose_provider_tool_failure(
                    "Provider asynchronous task start exceeded the frozen operation timeout."
                        .to_owned(),
                    diagnostic_context.as_str(),
                )
            })??;
            return self
                .await_provider_async_task(configuration, execution_id.as_str())
                .await;
        }
        tokio::time::timeout(
            remaining_provider_operation_timeout(operation_deadline, "Provider tool execution")?,
            self.execution_client.execute_provider_tool_with_profiles(
                &resolved.entry,
                &resolved.target,
                &resolved.request_profiles,
                &resolved.response_profiles,
                arguments,
                &secret,
            ),
        )
        .await
        .map_err(|_| {
            diagnose_provider_tool_failure(
                "Provider tool execution exceeded the frozen operation timeout.".to_owned(),
                diagnostic_context.as_str(),
            )
        })?
        .map_err(|error| {
            diagnose_provider_tool_failure_with_secret(error, diagnostic_context.as_str(), &secret)
        })
    }
}
