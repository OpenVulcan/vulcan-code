//! Frozen model execution and required request validation.
//! 冻结模型执行与必需请求校验。

use super::credentials::ProviderExecutionCredentialLease;
use super::{PublicAiRuntime, ResolvedPublicAiSelection};
use crate::{
    PersistedAiConfiguration, ProviderExecutionClient, ProviderExecutionFailure,
    ProviderExecutionRoute, ProviderExecutionStream, ProviderWireRequestCapture,
};
use vulcan_ai_contract::{
    AiExecutionRequest, AiMessage, AiOperation, ContentItem, DecisionRequest, DecisionResponse,
    EmbeddingRequest, EmbeddingResponse, InferenceUsageScope, MessageRole, PromptCachePolicy,
    ReasoningControlMode, ReasoningEffort, RerankRequest, RerankResponse,
};

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

    /// Executes one resolved internal decision selection with fresh credential material.
    /// 使用新鲜凭据材料执行一项已解析内部决策选择。
    ///
    /// # Parameters
    /// 参数
    /// `selection`, `request`, and `usage_scope` are already authorized immutable facts.
    /// `selection`、`request` 与 `usage_scope` 是已授权不可变事实。
    ///
    /// # Returns
    /// 返回值
    /// A validated decision response or a structured execution failure.
    /// 返回已校验决策响应或结构化执行失败。
    pub async fn execute_internal_decision(
        &self,
        selection: &ResolvedPublicAiSelection,
        request: &DecisionRequest,
        usage_scope: InferenceUsageScope,
    ) -> Result<DecisionResponse, ProviderExecutionFailure> {
        let secret = self
            .resolve_fresh_execution_secret(
                selection.route.connection.id.as_str(),
                selection.route.connection.secret_reference.as_str(),
            )
            .await
            .map_err(crate::execution::pre_dispatch_failure)?;
        self.execution_client
            .execute_decision(
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
}
