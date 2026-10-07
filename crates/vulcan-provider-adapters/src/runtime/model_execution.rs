//! Frozen model execution, required request validation, and explicit remote compaction.
//! 冻结模型执行、必需请求校验与显式远端压缩。

use super::credentials::ProviderExecutionCredentialLease;
use super::{PublicAiRuntime, ResolvedPublicAiSelection};
use crate::{
    PersistedAiConfiguration, ProviderExecutionClient, ProviderExecutionFailure,
    ProviderExecutionRoute, ProviderExecutionStream, ProviderWireRequestCapture,
};
use vulcan_ai_contract::{
    AiExecutionRequest, AiMessage, AiOperation, ContentItem, EmbeddingBatchCollectedResult,
    EmbeddingBatchPlan, EmbeddingBatchSnapshot, EmbeddingBatchSubmission, EmbeddingBatchTaskPlan,
    EmbeddingBatchUploadedInput, EmbeddingRequest, EmbeddingResponse, ExecutionRouteSnapshot,
    InferenceUsageScope, MessageRole, PromptCachePolicy, ReasoningControlMode, ReasoningEffort,
    RerankRequest, RerankResponse,
};

/// Carries one successful remote Responses compaction result with its owning route identity.
/// 携带一份成功的远端 Responses 压缩结果及其归属路由身份。
#[derive(Debug, Clone)]
pub struct ResponsesCompactExecution {
    /// Ordered opaque compaction output returned by the provider.
    /// 供应商返回的有序不透明压缩输出。
    pub result: crate::ResponsesCompactResult,
    /// Exact provider identifier.
    /// 精确供应商标识。
    pub provider_id: String,
    /// Exact credential connection identifier.
    /// 精确凭据连接标识。
    pub connection_id: String,
    /// Non-secret resolved Responses route fingerprint.
    /// 不含秘密的已解析 Responses 路由指纹。
    pub route_fingerprint: String,
    /// Exact protocol profile identifier.
    /// 精确协议配置标识。
    pub protocol_profile_id: String,
    /// Exact model identifier.
    /// 精确模型标识。
    pub model_id: String,
}

/// Validates one immutable execution route for the standalone Codex compaction endpoint.
/// 校验一条用于独立 Codex 压缩端点的不可变执行路由。
///
/// # Parameters
/// 参数
/// `execution` is the exact credential-bearing route prepared for one compact request.
/// `execution` 是为单次压缩请求准备的精确携带凭据路由。
///
/// # Returns
/// 返回值
/// Success only for an explicitly enabled Codex Responses route, otherwise a pre-dispatch failure.
/// 仅对显式启用的 Codex Responses 路由返回成功，否则返回分派前失败。
fn validate_responses_compact_execution_route(
    execution: &ResolvedPublicAiExecution,
) -> Result<(), ProviderExecutionFailure> {
    if execution.route.protocol_route.protocol_profile_id
        != vulcan_llm_core::OPENAI_CODEX_RESPONSES_PROFILE_ID
    {
        return Err(crate::execution::pre_dispatch_failure(
            "Remote Responses compaction requires the Codex Responses protocol.",
        ));
    }
    // enabled requires an explicit per-route opt-in because compatibility providers vary.
    // enabled 要求逐路由显式开启，因为兼容供应商的能力并不一致。
    let enabled = execution.route.protocol_route.remote_compaction_enabled();
    if !enabled {
        return Err(crate::execution::pre_dispatch_failure(
            "Remote Responses compaction is disabled for the selected provider route.",
        ));
    }
    Ok(())
}

/// Carries frozen route inputs and one live credential lease resolved before controller dispatch.
/// 承载控制器分派前解析完成的冻结路由输入与一份实时凭据租约。
pub struct ResolvedPublicAiExecution {
    /// Frozen non-secret media telemetry identity and the existing host observer.
    /// 冻结的非秘密媒体遥测身份及既有宿主观察器。
    pub(super) media_upload_telemetry: super::conversation_media::FrozenMediaUploadTelemetry,
    /// Exact provider execution route.
    /// 精确供应商执行路由。
    pub route: ProviderExecutionRoute,
    /// Exact credential-scoped model snapshot.
    /// 精确凭据作用域模型快照。
    pub model: crate::ConnectionModel,
    /// Exact account lease that resolves the latest secret revision for every physical request.
    /// 为每次真实请求解析最新秘密修订版的精确账号租约。
    pub credential: ProviderExecutionCredentialLease,
    /// Local model execution overrides.
    /// 本地模型执行覆盖。
    pub overrides: crate::ModelExecutionOverrides,
    /// Media settings copied from the frozen selection, never re-read during execution.
    /// 从冻结选择复制的媒体设置，执行期间不重新读取。
    pub media_delivery_policy: crate::MediaDeliveryPolicy,
    /// Model-level protocol binding that produced this execution.
    /// 产生此执行的模型级协议绑定。
    pub protocol_binding: crate::provider_manifest::ModelProtocolBinding,
    /// Shared public execution client.
    /// 共享公共执行客户端。
    pub client: ProviderExecutionClient,
}

impl ResolvedPublicAiExecution {
    /// Returns the frozen route revision for replay ownership checks without consulting mutable catalogs.
    /// 返回冻结路由修订供重放归属检查使用，不查询可变目录。
    pub fn frozen_route_revision(&self) -> &str {
        &self.media_upload_telemetry.route_revision
    }
}

impl PublicAiRuntime {
    /// Resolves one immutable execution route, model, secret, and local overrides.
    /// 解析一条不可变执行路由、模型、秘密与本地覆盖。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` is forwarded for API compatibility; selection uses the published generation.
    /// `configuration` 为 API 兼容而转发；选择使用已发布代际。
    ///
    /// # Returns
    /// 返回值
    /// The exact execution inputs or a pre-dispatch error.
    /// 返回精确执行输入或分派前错误。
    pub async fn resolve_active_execution(
        &self,
        configuration: &PersistedAiConfiguration,
    ) -> Result<ResolvedPublicAiExecution, String> {
        let selection = self.resolve_active_selection(configuration)?;
        self.resolve_execution_from_selection(selection).await
    }

    /// Resolves immutable execution inputs for one requested provider and model.
    /// 为一个请求指定的供应商与模型解析不可变执行输入。
    ///
    /// # Parameters
    /// 参数
    ///
    /// `configuration` is a compatibility input; the published generation owns selection state.
    /// `provider_id` and `model_id` identify the exact request-scoped model.
    /// `configuration` 是兼容输入；已发布代际持有选择状态。
    /// `provider_id` 与 `model_id` 标识精确请求级模型。
    ///
    /// # Returns
    /// 返回值
    ///
    /// The exact execution route, credential, model, and local overrides.
    /// 返回精确执行路由、凭据、模型与本地覆盖。
    pub async fn resolve_model_execution(
        &self,
        configuration: &PersistedAiConfiguration,
        provider_id: &str,
        model_id: &str,
    ) -> Result<ResolvedPublicAiExecution, String> {
        let selection = self.resolve_model_selection(configuration, provider_id, model_id)?;
        self.resolve_execution_from_selection(selection).await
    }

    /// Resolves complete execution inputs for one exact persisted connection and model.
    /// 为一个精确持久化连接与模型解析完整执行输入。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` is forwarded for compatibility; the published generation owns the graph.
    /// The remaining identifiers are the exact selection frozen by the submitting session.
    /// `configuration` 为兼容而转发；已发布代际持有连接图。
    /// 其余标识是提交会话冻结的精确选择。
    ///
    /// # Returns
    /// 返回值
    /// The route, model, secret, and local overrides for that exact account.
    /// 该精确账号的路由、模型、秘密与本地覆盖。
    pub async fn resolve_connection_model_execution(
        &self,
        configuration: &PersistedAiConfiguration,
        connection_id: &str,
        provider_id: &str,
        model_id: &str,
    ) -> Result<ResolvedPublicAiExecution, String> {
        let selection = self.resolve_connection_model_selection(
            configuration,
            connection_id,
            provider_id,
            model_id,
        )?;
        self.resolve_execution_from_selection(selection).await
    }

    /// Attaches credential material to one already validated public selection.
    /// 为一个已经校验的公共选择附加凭据材料。
    ///
    /// # Parameters
    /// 参数
    ///
    /// `selection` is the immutable route and model selection resolved from current state.
    /// `selection` 是从当前状态解析得到的不可变路由与模型选择。
    ///
    /// # Returns
    /// 返回值
    ///
    /// Complete execution inputs, or a controlled credential-storage error.
    /// 返回完整执行输入，或受控的凭据存储错误。
    async fn resolve_execution_from_selection(
        &self,
        selection: ResolvedPublicAiSelection,
    ) -> Result<ResolvedPublicAiExecution, String> {
        // credential freezes account identity while keeping the access token live per physical request.
        // credential 冻结账号身份，同时让 Access Token 在每次真实请求前保持实时。
        let credential = self.execution_credential_lease(
            selection.route.connection.id.as_str(),
            selection.route.connection.secret_reference.as_str(),
        );
        // The initial resolution preserves the pre-dispatch credential failure boundary without freezing this value for execution.
        // 初始解析保留分派前凭据失败边界，但不会把该值冻结给后续执行。
        credential.resolve_for_request().await?;
        // Capture the installed observer without holding its registry lock across callbacks or uploads.
        // 捕获已安装观察器，不在回调或上传期间持有其注册表锁。
        let media_upload_observer = self
            .runtime_media_upload_progress_observer
            .read()
            .map_err(|_| String::from("The runtime media upload observer lock is poisoned."))?
            .clone();
        Ok(ResolvedPublicAiExecution {
            media_upload_telemetry: super::conversation_media::FrozenMediaUploadTelemetry {
                observer: media_upload_observer,
                manifest_generation: selection.manifest_generation,
                route_revision: selection.route_revision,
            },
            media_delivery_policy: selection.media_delivery_policy,
            route: selection.route,
            model: selection.model,
            credential,
            overrides: selection.overrides,
            protocol_binding: selection.protocol_binding,
            client: self.execution_client.clone(),
        })
    }

    /// Builds one execution from a fully frozen selection by re-reading only the secret.
    /// 通过仅重读秘密，从一个完全冻结的选择构建执行对象。
    ///
    /// This is the controlled execution entry for prepared turns: it never re-resolves the
    /// active catalog, never re-selects a protocol, and never rebuilds overlays. The frozen
    /// route, model, overrides, and binding are used verbatim; only the current secret for
    /// the frozen `secret_reference` is read (plan §6.3/§R4.3).
    /// 这是已准备回合的受控执行入口：绝不重新解析活动目录、绝不重新选择协议、绝不重建
    /// 覆盖。冻结的路由、模型、覆盖与绑定原样使用；只为冻结的 `secret_reference` 读取
    /// 当前秘密（方案 §6.3/§R4.3）。
    ///
    /// # Parameters
    /// 参数
    /// `selection` is the non-secret frozen selection captured at the activation boundary.
    /// `selection` 是在激活边界捕获的非秘密冻结选择。
    ///
    /// # Returns
    /// 返回值
    /// Complete execution inputs, or a controlled credential-storage error that never
    /// re-selects another account or protocol.
    /// 返回完整执行输入，或绝不改选其他账号或协议的受控凭据存储错误。
    pub async fn resolve_execution_from_frozen_selection(
        &self,
        selection: ResolvedPublicAiSelection,
    ) -> Result<ResolvedPublicAiExecution, String> {
        self.resolve_execution_from_selection(selection).await
    }

    /// Validates one selected model and protocol with an executable disabled-reasoning probe.
    /// 使用可执行的关闭思考探针校验一个已选模型与协议。
    ///
    /// # Parameters
    /// 参数
    /// `selection` is the immutable route, model capability, and protocol snapshot resolved for
    /// one configured inference purpose.
    /// `selection` 是为一个已配置推理用途解析出的不可变路由、模型能力与协议快照。
    ///
    /// # Returns
    /// 返回值
    /// Success only when exact request encoding preserves the required no-reasoning contract.
    /// 仅当精确请求编码保留必需的无思考契约时返回成功。
    pub fn validate_disabled_reasoning_route_projection(
        &self,
        selection: &ResolvedPublicAiSelection,
    ) -> Result<(), String> {
        // modelParameters mirrors the VMM host policy by removing only conflicts declared by the
        // frozen manifest before the disabled effort is encoded.
        // modelParameters 通过仅移除冻结 Manifest 声明的冲突项来复现 VMM 宿主策略，然后再编码关闭档位。
        let mut model_parameters = selection.overrides.model_parameters.clone();
        for parameter_id in selection
            .route
            .model_parameters_conflicting_with_request_field("reasoning_effort")
        {
            model_parameters.remove(parameter_id.as_str());
        }
        // probe uses one bounded user message so every registered conversation protocol executes
        // its ordinary production encoder rather than a special configuration-only shortcut.
        // probe 使用一条有界用户消息，使每个已注册对话协议执行普通生产编码器，而不是配置专用捷径。
        let probe = AiExecutionRequest {
            operation: AiOperation::ConversationRespond,
            model_id: selection.model.id.clone(),
            cache_scope_id: None,
            continuation: None,
            messages: vec![AiMessage {
                role: MessageRole::User,
                content: vec![ContentItem::Text {
                    text: String::from("VMM disabled-reasoning route validation probe."),
                }],
                provider_context_items: Vec::new(),
            }],
            tools: Vec::new(),
            native_tools: Vec::new(),
            tool_choice: Default::default(),
            parallel_tool_calls: Default::default(),
            structured_output: Default::default(),
            stop_sequences: Vec::new(),
            prompt_cache: PromptCachePolicy::ProviderDefault,
            reasoning_effort: Some(ReasoningEffort::None),
            reasoning_output: Default::default(),
            model_parameters,
            resources: Vec::new(),
            output_token_limit: selection
                .model
                .capabilities
                .output_limit
                .map(|limit| limit.min(128))
                .or(Some(128)),
        };
        self.validate_disabled_reasoning_request_projection(selection, &probe)
    }

    /// Validates the final encoded body for one exact required no-reasoning request.
    /// 校验一项精确必需无思考请求的最终编码请求体。
    ///
    /// # Parameters
    /// 参数
    /// `selection` is the frozen model route and `request` is the exact provider-neutral request
    /// that will be executed without protocol reselection.
    /// `selection` 是冻结模型路由，`request` 是将不经协议重选直接执行的精确供应商无关请求。
    ///
    /// # Returns
    /// 返回值
    /// Success only when the final post-projection body cannot enable reasoning and switchable
    /// models retain an explicit disable field.
    /// 仅当最终投影请求体不会开启思考，且可切换模型保留显式关闭字段时返回成功。
    pub fn validate_disabled_reasoning_request_projection(
        &self,
        selection: &ResolvedPublicAiSelection,
        request: &AiExecutionRequest,
    ) -> Result<(), String> {
        self.execution_client
            .encode_required_disabled_reasoning_request(
                &selection.route,
                request,
                selection.model.capabilities.reasoning_control == ReasoningControlMode::FixedOff,
            )
            .map(|_| ())
            .map_err(|failure| failure.error.message)
    }

    /// Plans deterministic asynchronous embedding Batch tasks for one resolved selection.
    /// 为一项已解析选择规划确定性的异步向量 Batch 任务。
    pub fn plan_internal_embedding_batch(
        &self,
        selection: &ResolvedPublicAiSelection,
        request: &EmbeddingRequest,
        usage_scope: InferenceUsageScope,
    ) -> Result<EmbeddingBatchPlan, ProviderExecutionFailure> {
        self.execution_client.plan_embedding_batch(
            &selection.route,
            &selection.model,
            request,
            usage_scope,
            crate::native_inference::ExecutionRevisionRefs {
                catalog: selection.catalog_revision.as_str(),
                route: selection.route_revision.as_str(),
            },
        )
    }

    /// Reconstructs the current immutable embedding Batch route snapshot and request fingerprint.
    /// 重建当前不可变的向量 Batch 路由快照及请求指纹。
    pub fn internal_embedding_batch_request_identity(
        &self,
        selection: &ResolvedPublicAiSelection,
        request: &EmbeddingRequest,
        usage_scope: InferenceUsageScope,
    ) -> Result<(ExecutionRouteSnapshot, String), ProviderExecutionFailure> {
        self.execution_client.embedding_batch_request_identity(
            &selection.route,
            &selection.model,
            request,
            usage_scope,
            crate::native_inference::ExecutionRevisionRefs {
                catalog: selection.catalog_revision.as_str(),
                route: selection.route_revision.as_str(),
            },
        )
    }

    /// Uploads one exact file-backed embedding Batch input with fresh credentials.
    /// 使用新鲜凭据上传一个精确的文件型向量 Batch 输入。
    pub async fn upload_internal_embedding_batch_input(
        &self,
        selection: &ResolvedPublicAiSelection,
        request: &EmbeddingRequest,
        task: &EmbeddingBatchTaskPlan,
        artifact_sink: Option<&dyn crate::EmbeddingBatchArtifactSink>,
    ) -> Result<EmbeddingBatchUploadedInput, ProviderExecutionFailure> {
        let secret = self
            .resolve_fresh_execution_secret(
                selection.route.connection.id.as_str(),
                selection.route.connection.secret_reference.as_str(),
            )
            .await
            .map_err(crate::execution::pre_dispatch_failure)?;
        self.execution_client
            .upload_embedding_batch_input(
                &selection.route,
                &selection.model,
                request,
                task,
                &secret,
                artifact_sink,
            )
            .await
    }

    /// Submits one deterministic asynchronous embedding Batch task with fresh credentials.
    /// 使用新鲜凭据提交一个确定性异步向量 Batch 任务。
    pub async fn submit_internal_embedding_batch(
        &self,
        selection: &ResolvedPublicAiSelection,
        request: &EmbeddingRequest,
        task: &EmbeddingBatchTaskPlan,
        uploaded_input: Option<&EmbeddingBatchUploadedInput>,
        artifact_sink: Option<&dyn crate::EmbeddingBatchArtifactSink>,
    ) -> Result<EmbeddingBatchSubmission, ProviderExecutionFailure> {
        let secret = self
            .resolve_fresh_execution_secret(
                selection.route.connection.id.as_str(),
                selection.route.connection.secret_reference.as_str(),
            )
            .await
            .map_err(crate::execution::pre_dispatch_failure)?;
        self.execution_client
            .submit_embedding_batch(
                &selection.route,
                &selection.model,
                request,
                task,
                uploaded_input,
                &secret,
                artifact_sink,
            )
            .await
    }

    /// Reconciles one file-backed task after an indeterminate creation response.
    /// 在创建响应不确定后对账一个文件型任务。
    pub async fn reconcile_internal_embedding_batch_submission(
        &self,
        selection: &ResolvedPublicAiSelection,
        task: &EmbeddingBatchTaskPlan,
        uploaded_input: &EmbeddingBatchUploadedInput,
    ) -> Result<Option<EmbeddingBatchSubmission>, ProviderExecutionFailure> {
        let secret = self
            .resolve_fresh_execution_secret(
                selection.route.connection.id.as_str(),
                selection.route.connection.secret_reference.as_str(),
            )
            .await
            .map_err(crate::execution::pre_dispatch_failure)?;
        self.execution_client
            .reconcile_embedding_batch_submission(
                &selection.route,
                &selection.model,
                task,
                uploaded_input,
                &secret,
            )
            .await
    }

    /// Polls one durable asynchronous embedding Batch submission with fresh credentials.
    /// 使用新鲜凭据轮询一个持久异步向量 Batch 提交。
    pub async fn internal_embedding_batch_status(
        &self,
        selection: &ResolvedPublicAiSelection,
        submission: &EmbeddingBatchSubmission,
        artifact_sink: Option<&dyn crate::EmbeddingBatchArtifactSink>,
    ) -> Result<EmbeddingBatchSnapshot, ProviderExecutionFailure> {
        let secret = self
            .resolve_fresh_execution_secret(
                selection.route.connection.id.as_str(),
                selection.route.connection.secret_reference.as_str(),
            )
            .await
            .map_err(crate::execution::pre_dispatch_failure)?;
        self.execution_client
            .embedding_batch_status(
                &selection.route,
                &selection.model,
                submission,
                &secret,
                artifact_sink,
            )
            .await
    }

    /// Collects one completed asynchronous embedding Batch task with fresh credentials.
    /// 使用新鲜凭据收集一个已完成异步向量 Batch 任务。
    pub async fn collect_internal_embedding_batch(
        &self,
        selection: &ResolvedPublicAiSelection,
        request: &EmbeddingRequest,
        task: &EmbeddingBatchTaskPlan,
        snapshot: &EmbeddingBatchSnapshot,
        usage_scope: InferenceUsageScope,
        artifact_sink: Option<&dyn crate::EmbeddingBatchArtifactSink>,
    ) -> Result<EmbeddingBatchCollectedResult, ProviderExecutionFailure> {
        let secret = self
            .resolve_fresh_execution_secret(
                selection.route.connection.id.as_str(),
                selection.route.connection.secret_reference.as_str(),
            )
            .await
            .map_err(crate::execution::pre_dispatch_failure)?;
        self.execution_client
            .collect_embedding_batch(
                &selection.route,
                &selection.model,
                crate::native_inference_batch::EmbeddingBatchCollectionContext {
                    request,
                    task,
                    snapshot,
                    usage_scope,
                    revisions: crate::native_inference::ExecutionRevisionRefs {
                        catalog: selection.catalog_revision.as_str(),
                        route: selection.route_revision.as_str(),
                    },
                },
                &secret,
                artifact_sink,
            )
            .await
    }

    /// Deletes one temporary provider input file with fresh credentials after durable ingestion.
    /// 在结果持久入库后使用新鲜凭据删除一个供应商临时输入文件。
    pub async fn cleanup_internal_embedding_batch_input(
        &self,
        selection: &ResolvedPublicAiSelection,
        submission: &EmbeddingBatchSubmission,
    ) -> Result<(), ProviderExecutionFailure> {
        let secret = self
            .resolve_fresh_execution_secret(
                selection.route.connection.id.as_str(),
                selection.route.connection.secret_reference.as_str(),
            )
            .await
            .map_err(crate::execution::pre_dispatch_failure)?;
        self.execution_client
            .cleanup_embedding_batch_input(&selection.route, &selection.model, submission, &secret)
            .await
    }

    /// Cancels one durable asynchronous embedding Batch submission with fresh credentials.
    /// 使用新鲜凭据取消一个持久异步向量 Batch 提交。
    pub async fn cancel_internal_embedding_batch(
        &self,
        selection: &ResolvedPublicAiSelection,
        submission: &EmbeddingBatchSubmission,
    ) -> Result<EmbeddingBatchSnapshot, ProviderExecutionFailure> {
        let secret = self
            .resolve_fresh_execution_secret(
                selection.route.connection.id.as_str(),
                selection.route.connection.secret_reference.as_str(),
            )
            .await
            .map_err(crate::execution::pre_dispatch_failure)?;
        self.execution_client
            .cancel_embedding_batch(&selection.route, &selection.model, submission, &secret)
            .await
    }

    /// Executes one resolved embedding request while optionally capturing the exact secret-safe transport object.
    /// 执行一项已解析向量请求，并可选捕获精确且不含秘密的传输对象。
    ///
    /// `wire_request_capture` receives the final native JSON request immediately before dispatch.
    /// `wire_request_capture` 在真实分派前立即接收最终原生 JSON 请求。
    pub async fn execute_internal_embedding_with_wire_request_capture(
        &self,
        selection: &ResolvedPublicAiSelection,
        request: &EmbeddingRequest,
        usage_scope: InferenceUsageScope,
        wire_request_capture: Option<tokio::sync::oneshot::Sender<ProviderWireRequestCapture>>,
    ) -> Result<EmbeddingResponse, ProviderExecutionFailure> {
        let secret = self
            .resolve_fresh_execution_secret(
                selection.route.connection.id.as_str(),
                selection.route.connection.secret_reference.as_str(),
            )
            .await
            .map_err(crate::execution::pre_dispatch_failure)?;
        self.execution_client
            .execute_embedding(
                &selection.route,
                &selection.model,
                request,
                usage_scope,
                crate::native_inference::ExecutionRevisionRefs {
                    catalog: selection.catalog_revision.as_str(),
                    route: selection.route_revision.as_str(),
                },
                &secret,
                wire_request_capture,
            )
            .await
    }

    /// Executes one resolved internal rerank selection with fresh credential material.
    /// 使用新鲜凭据材料执行一项已解析内部重排选择。
    ///
    /// # Parameters
    /// 参数
    /// `selection`, `request`, and `usage_scope` are already authorized immutable facts.
    /// `selection`、`request` 与 `usage_scope` 是已授权不可变事实。
    ///
    /// # Returns
    /// 返回值
    /// A normalized ranked result or a structured execution failure.
    /// 返回归一化排序结果或结构化执行失败。
    pub async fn execute_internal_rerank(
        &self,
        selection: &ResolvedPublicAiSelection,
        request: &RerankRequest,
        usage_scope: InferenceUsageScope,
    ) -> Result<RerankResponse, ProviderExecutionFailure> {
        let secret = self
            .resolve_fresh_execution_secret(
                selection.route.connection.id.as_str(),
                selection.route.connection.secret_reference.as_str(),
            )
            .await
            .map_err(crate::execution::pre_dispatch_failure)?;
        self.execution_client
            .execute_rerank(
                &selection.route,
                &selection.model,
                request,
                usage_scope,
                crate::native_inference::ExecutionRevisionRefs {
                    catalog: selection.catalog_revision.as_str(),
                    route: selection.route_revision.as_str(),
                },
                &secret,
            )
            .await
    }

    /// Executes one resolved Responses request with fresh credential material.
    /// 使用新鲜凭据材料执行一项已解析的 Responses 请求。
    ///
    /// # Parameters
    /// 参数
    /// `selection` is the exact pre-authorized route, while `request` contains the complete
    /// provider-neutral tool and resource semantics for this execution.
    /// `selection` 是精确预授权路由，`request` 包含本次执行完整的供应商无关工具与资源语义。
    ///
    /// # Returns
    /// 返回值
    /// A normalized provider event stream or a structured pre-dispatch failure.
    /// 返回归一化供应商事件流或结构化分派前失败。
    pub async fn execute_internal_llm(
        &self,
        selection: &ResolvedPublicAiSelection,
        request: AiExecutionRequest,
        execution_id: String,
    ) -> Result<ProviderExecutionStream, ProviderExecutionFailure> {
        self.execute_internal_llm_with_wire_request_capture(selection, request, execution_id, None)
            .await
    }

    /// Executes one resolved internal LLM request while optionally capturing the exact secret-safe transport object.
    /// 执行一项已解析内部 LLM 请求，并可选捕获精确且不含秘密的传输对象。
    ///
    /// # Parameters
    /// 参数
    /// `selection`, `request`, and `execution_id` freeze the authorized physical execution;
    /// `wire_request_capture` receives the one object handed to the selected HTTP or WebSocket transport.
    /// `selection`、`request` 与 `execution_id` 冻结已授权的真实执行；
    /// `wire_request_capture` 接收交给所选 HTTP 或 WebSocket 传输的唯一对象。
    ///
    /// # Returns
    /// 返回值
    /// A normalized event stream whose first poll performs the exact dispatch, or a structured pre-dispatch failure.
    /// 返回在首次轮询时执行精确分派的规范化事件流，或结构化分派前失败。
    pub async fn execute_internal_llm_with_wire_request_capture(
        &self,
        selection: &ResolvedPublicAiSelection,
        request: AiExecutionRequest,
        execution_id: String,
        wire_request_capture: Option<tokio::sync::oneshot::Sender<ProviderWireRequestCapture>>,
    ) -> Result<ProviderExecutionStream, ProviderExecutionFailure> {
        if request.operation != AiOperation::ConversationRespond || request.continuation.is_some() {
            return Err(crate::execution::pre_dispatch_failure(
                "Responses execution requires a conversation operation and does not accept host-private continuation metadata.",
            ));
        }
        if request.model_id != selection.model.id {
            return Err(crate::execution::pre_dispatch_failure(format!(
                "Restricted internal LLM request model `{}` does not match resolved model `{}`.",
                request.model_id, selection.model.id
            )));
        }
        let secret = self
            .resolve_fresh_execution_secret(
                selection.route.connection.id.as_str(),
                selection.route.connection.secret_reference.as_str(),
            )
            .await
            .map_err(crate::execution::pre_dispatch_failure)?;
        Ok(if let Some(capture) = wire_request_capture {
            self.execution_client.execute_with_wire_request_capture(
                selection.route.clone(),
                request,
                secret,
                execution_id,
                capture,
            )
        } else {
            self.execution_client
                .execute(selection.route.clone(), request, secret, execution_id)
        })
    }

    /// Executes remote Responses compaction only when the exact session route explicitly enables it.
    /// 仅当精确会话路由显式开启远端压缩时执行 Responses 远端压缩。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` and `connection_id` identify the exact session-scoped route; `request` is the
    /// provider-neutral compaction input assembled from the canonical state.
    /// `provider_id` 与 `connection_id` 标识精确会话级路由；`request` 是从规范状态组装出的供应商无关压缩输入。
    ///
    /// # Returns
    /// 返回值
    /// A route-bound opaque compaction result, or a controlled failure that the caller may safely
    /// downgrade to local compaction only before dispatch.
    /// 返回绑定路由的不透明压缩结果；或调用方可仅在分派前安全降级本地压缩的受控失败。
    pub async fn execute_responses_compact(
        &self,
        provider_id: &str,
        connection_id: &str,
        request: AiExecutionRequest,
    ) -> Result<ResponsesCompactExecution, ProviderExecutionFailure> {
        // modelId is retained before the request moves into the prepared dispatch.
        // modelId 在请求移入已准备分派前完成保留。
        let model_id = request.model_id.clone();
        let execution = self
            .prepare_responses_compact_execution(provider_id, connection_id, model_id.as_str())
            .await?;
        self.execute_prepared_responses_compact(execution, request)
            .await
    }

    /// Resolves and validates the exact immutable execution used to project a compact request.
    /// 解析并校验用于投影压缩请求的精确不可变执行对象。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id`, `connection_id`, and `model_id` are the route frozen by the owning session.
    /// `provider_id`、`connection_id` 与 `model_id` 是所属会话冻结的路由。
    ///
    /// # Returns
    /// 返回值
    /// A credential-bearing immutable execution safe to use for both projection and immediate dispatch.
    /// 返回可同时用于投影与立即分派、且携带凭据的不可变执行对象。
    pub async fn prepare_responses_compact_execution(
        &self,
        provider_id: &str,
        connection_id: &str,
        model_id: &str,
    ) -> Result<ResolvedPublicAiExecution, ProviderExecutionFailure> {
        // configuration freezes the authoritative graph for the complete prepared execution.
        // configuration 为完整的已准备执行冻结权威连接图。
        let configuration = self
            .configuration_snapshot()
            .map_err(crate::execution::pre_dispatch_failure)?;
        // execution resolves the exact credential once so request projection and dispatch cannot diverge.
        // execution 只解析一次精确凭据，避免请求投影与分派发生分歧。
        let execution = self
            .resolve_connection_model_execution(
                &configuration,
                connection_id,
                provider_id,
                model_id,
            )
            .await
            .map_err(crate::execution::pre_dispatch_failure)?;
        validate_responses_compact_execution_route(&execution)?;
        Ok(execution)
    }

    /// Dispatches one compact request through the same immutable execution used for projection.
    /// 通过用于投影的同一不可变执行对象分派一次压缩请求。
    ///
    /// # Parameters
    /// 参数
    /// `execution` is prepared by `prepare_responses_compact_execution`; `request` is projected
    /// against that exact route and may retain the current model-visible tool catalog.
    /// `execution` 由 `prepare_responses_compact_execution` 准备；`request` 基于该精确路由投影，
    /// 并可保留当前模型可见工具目录。
    ///
    /// # Returns
    /// 返回值
    /// A route-bound opaque result, or a controlled failure before or after dispatch.
    /// 返回绑定路由的不透明结果，或分派前后产生的受控失败。
    pub async fn execute_prepared_responses_compact(
        &self,
        execution: impl Into<std::sync::Arc<ResolvedPublicAiExecution>>,
        request: AiExecutionRequest,
    ) -> Result<ResponsesCompactExecution, ProviderExecutionFailure> {
        // Media projection and compact dispatch may share the same immutable execution without resolving it again.
        // 媒体投影和压缩分派可以共享同一个不可变执行，无需再次解析。
        let execution = execution.into();
        if request.operation != AiOperation::ConversationRespond
            || !request.resources.is_empty()
            || request.continuation.is_some()
        {
            return Err(crate::execution::pre_dispatch_failure(
                "Remote Responses compaction rejects resources, continuation, and non-conversation operations.",
            ));
        }
        validate_responses_compact_execution_route(&execution)?;
        if request.model_id != execution.model.id {
            return Err(crate::execution::pre_dispatch_failure(format!(
                "Remote Responses compaction request model `{}` does not match resolved model `{}`.",
                request.model_id, execution.model.id
            )));
        }
        // routeFingerprint is captured before credential consumption for the returned checkpoint identity.
        // routeFingerprint 在消耗凭据前捕获，用于返回的检查点身份。
        let route_fingerprint = execution.route.request_url("/responses");
        // providerId and connectionId retain the exact prepared route after dispatch consumes the secret.
        // providerId 与 connectionId 在分派消耗凭据后仍保留精确准备路由。
        let provider_id = execution.route.provider.id.clone();
        let connection_id = execution.route.connection.id.clone();
        let protocol_profile_id = execution.route.protocol_route.protocol_profile_id.clone();
        let model_id = execution.model.id.clone();
        // secret is resolved immediately before the first physical compaction attempt.
        // secret 在第一次真实压缩尝试之前立即解析。
        let mut secret = execution
            .credential
            .resolve_for_request()
            .await
            .map_err(crate::execution::pre_dispatch_failure)?;
        // oauthReplayUsed limits this unary operation to one safe same-account 401 replay.
        // oauthReplayUsed 将该一元操作限制为一次安全的同账号 401 重放。
        let mut oauth_replay_used = false;
        // result retries only after the exact renewable credential has been refreshed successfully.
        // result 仅在精确可续期凭据成功刷新后才进行重试。
        let result = loop {
            // rejectedRevision binds recovery to the credential sent by this exact physical attempt.
            // rejectedRevision 将恢复绑定到本次真实尝试发送的凭据。
            let rejected_revision = secret.revision();
            match execution
                .client
                .execute_responses_compact(execution.route.clone(), request.clone(), secret.clone())
                .await
            {
                Ok(result) => {
                    execution.credential.mark_request_succeeded();
                    break result;
                }
                Err(mut failure)
                    if !oauth_replay_used
                        && failure.error.http_status == Some(401)
                        && !failure.error.semantic_output_observed
                        && !failure.error.side_effect_observed =>
                {
                    match execution
                        .credential
                        .refresh_after_unauthorized(rejected_revision)
                        .await
                    {
                        Ok(true) => {
                            oauth_replay_used = true;
                            secret = execution
                                .credential
                                .resolve_for_request()
                                .await
                                .map_err(crate::execution::pre_dispatch_failure)?;
                        }
                        Ok(false) => return Err(failure),
                        Err(refresh_error) => {
                            failure.error.message = format!(
                                "{} Host OAuth credential refresh failed after the upstream 401 response: {refresh_error}",
                                failure.error.message
                            );
                            return Err(failure);
                        }
                    }
                }
                Err(failure) => return Err(failure),
            }
        };
        Ok(ResponsesCompactExecution {
            result,
            provider_id,
            connection_id,
            route_fingerprint,
            protocol_profile_id,
            model_id,
        })
    }
}
