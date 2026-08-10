//! Per-turn provider tool registration and host-side output staging.
//! 每回合供应商工具注册与宿主侧输出暂存。

use crate::attachment_assets::{
    attachment_descriptor_from_mime_type, stage_attachment_bytes, stage_attachment_file_streaming,
};
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use vulcan_provider_adapters::{
    AvailableProviderToolTarget, PersistedAiConfiguration, ProviderToolExecutionResult,
    ProviderToolNormalizedOutput, ProviderToolResourceDownloadPolicy, PublicAiRuntime,
};
use vulcan_tools_manager::{
    VulcanPlanToolPolicy, VulcanToolCandidate, VulcanToolDescriptor, VulcanToolHostResult,
    VulcanToolInvocationResult, VulcanToolSourceKind,
};

/// Maximum generated image or generic file size accepted by the host.
/// 宿主接受的生成图片或通用文件最大字节数。
const PROVIDER_TOOL_DEFAULT_OUTPUT_BYTE_LIMIT: usize = 50 * 1024 * 1024;
/// Maximum generated audio size accepted by the host.
/// 宿主接受的生成音频最大字节数。
const PROVIDER_TOOL_AUDIO_OUTPUT_BYTE_LIMIT: usize = 100 * 1024 * 1024;
/// Maximum generated video size accepted by the host.
/// 宿主接受的生成视频最大字节数。
const PROVIDER_TOOL_VIDEO_OUTPUT_BYTE_LIMIT: usize = 512 * 1024 * 1024;
/// Maximum wall-clock time allowed for one provider resource download.
/// 单个供应商资源下载允许的最大墙钟时间。
const PROVIDER_TOOL_RESOURCE_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// Marks owned asynchronous provider work for cancellation when one tool future is dropped.
/// 当工具 Future 被丢弃时，为其拥有的异步供应商任务记录取消意图。
struct ProviderTaskCancellationOnDrop {
    /// Runtime that owns durable provider-task recovery state.
    /// 持有供应商任务持久恢复状态的运行时。
    runtime: Arc<PublicAiRuntime>,
    /// Exact conversation identity attached to the tool invocation.
    /// 附加到工具调用的精确会话身份。
    owner_session_id: Option<String>,
    /// Whether dropping this guard must record cancellation intent.
    /// 丢弃此守卫时是否必须记录取消意图。
    armed: bool,
}

impl ProviderTaskCancellationOnDrop {
    /// Creates an armed guard for one provider-tool invocation.
    /// 为一次供应商工具调用创建已启用守卫。
    ///
    /// # Parameters
    /// 参数
    /// `runtime` owns task recovery and `owner_session_id` identifies the conversation.
    /// `runtime` 持有任务恢复状态，`owner_session_id` 标识会话。
    ///
    /// # Returns
    /// 返回值
    /// An armed cancellation guard.
    /// 返回已启用的取消守卫。
    fn new(runtime: Arc<PublicAiRuntime>, owner_session_id: Option<String>) -> Self {
        Self {
            runtime,
            owner_session_id,
            armed: true,
        }
    }

    /// Disarms cancellation after the invocation reaches a normal result.
    /// 在调用正常得到结果后解除取消。
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for ProviderTaskCancellationOnDrop {
    /// Persists cancellation intent synchronously before the interrupted future disappears.
    /// 在被中断 Future 消失前同步持久化取消意图。
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let Some(owner_session_id) = self.owner_session_id.as_deref() else {
            return;
        };
        if let Err(error) = self
            .runtime
            .request_provider_task_cancellation_for_owner(owner_session_id)
        {
            eprintln!(
                "[provider-tool] failed to persist provider-task cancellation for owner `{owner_session_id}`: {error}"
            );
        }
    }
}

/// Builds provider tool candidates from the one selected target per capability.
/// 根据每项能力唯一选中的目标构建供应商工具候选项。
///
/// # Parameters
/// 参数
/// `runtime`, `configuration`, and `workspace_root` are frozen for this execution turn.
/// `runtime`、`configuration` 与 `workspace_root` 在当前执行回合保持冻结。
///
/// # Returns
/// 返回值
/// Exact model-native tool candidates after stale and disabled selections are filtered.
/// 过滤过期与已禁用选择后返回精确模型原生工具候选项。
pub(crate) fn build_provider_tool_candidates(
    runtime: Arc<PublicAiRuntime>,
    mut configuration: PersistedAiConfiguration,
    workspace_root: Option<PathBuf>,
    session_selections: &Option<
        std::collections::BTreeMap<
            String,
            vulcan_workbench_model::SessionProviderToolTargetSelection,
        >,
    >,
    suppress_web_search: bool,
) -> Result<Vec<VulcanToolCandidate>, String> {
    apply_session_provider_tool_selections(&mut configuration, session_selections)?;
    let available = runtime.available_provider_tool_targets(&configuration)?;
    let mut candidates = Vec::new();
    for (capability, selection) in &configuration.provider_tool_selections {
        if provider_tool_capability_is_suppressed(*capability, suppress_web_search) {
            // Native route search is registered independently; retain persisted external selection but hide it for this turn.
            // 原生路由搜索独立注册；保留持久化外部选择，但本回合不暴露该工具。
            continue;
        }
        if !configuration
            .provider_tool_enabled_capabilities
            .contains(capability)
        {
            continue;
        }
        let Some(selected) = available
            .iter()
            .find(|candidate| {
                candidate.connection_id == selection.connection_id
                    && candidate.target.target_id == selection.target_id
                    && candidate.target.contract.capability == *capability
            })
            .cloned()
        else {
            return Err(format!(
                "Provider tool capability `{:?}` retains unavailable target `{}` through account `{}`; select a valid model and account before sending.",
                capability, selection.target_id, selection.connection_id
            ));
        };
        candidates.push(build_provider_tool_candidate(
            Arc::clone(&runtime),
            configuration.clone(),
            workspace_root.clone(),
            selected,
        ));
    }
    Ok(candidates)
}

/// Reports whether one configured provider-tool capability is hidden for the current route.
/// 判断当前路由是否需要隐藏某项已配置的供应商工具能力。
fn provider_tool_capability_is_suppressed(
    capability: vulcan_provider_adapters::ProviderToolCapabilityKind,
    suppress_web_search: bool,
) -> bool {
    suppress_web_search
        && capability == vulcan_provider_adapters::ProviderToolCapabilityKind::WebSearch
}

/// Applies an explicit session override to the per-turn provider configuration.
/// 将显式会话覆盖应用到当前回合的供应商配置。
fn apply_session_provider_tool_selections(
    configuration: &mut PersistedAiConfiguration,
    session_selections: &Option<
        std::collections::BTreeMap<
            String,
            vulcan_workbench_model::SessionProviderToolTargetSelection,
        >,
    >,
) -> Result<(), String> {
    let Some(session_selections) = session_selections else {
        return Ok(());
    };
    configuration.provider_tool_selections = session_selections
        .iter()
        .map(|(capability, selection)| {
            let capability =
                serde_json::from_value(Value::String(capability.clone())).map_err(|error| {
                    format!("Session provider tool capability `{capability}` is invalid: {error}")
                })?;
            Ok((
                capability,
                vulcan_provider_adapters::ProviderToolTargetSelection {
                    connection_id: selection.connection_id.clone(),
                    target_id: selection.target_id.clone(),
                },
            ))
        })
        .collect::<Result<_, String>>()?;
    configuration.provider_tool_enabled_capabilities = configuration
        .provider_tool_selections
        .keys()
        .copied()
        .collect();
    Ok(())
}

/// Builds one immutable candidate for one exact selected target.
/// 为一个精确选定目标构建一个不可变候选项。
///
/// # Parameters
/// 参数
/// The runtime, configuration, workspace, and target are frozen into the invocation closure.
/// 运行时、配置、工作区与目标会冻结到调用闭包中。
///
/// # Returns
/// 返回值
/// An AI-host binding with the selected model's unmodified input schema.
/// 返回使用选定模型未修改输入 Schema 的 AI 宿主绑定。
fn build_provider_tool_candidate(
    runtime: Arc<PublicAiRuntime>,
    configuration: PersistedAiConfiguration,
    workspace_root: Option<PathBuf>,
    selected: AvailableProviderToolTarget,
) -> VulcanToolCandidate {
    let contract = selected.target.contract.clone();
    let capability = contract.capability;
    VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: contract.tool_id.clone(),
            name: contract.tool_id,
            description: contract.description,
            help_text: None,
            source: VulcanToolSourceKind::AiHostBinding,
        },
        parameters_schema: contract.input_schema,
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Visible,
        request_streaming: None,
        invoke: Arc::new(move |arguments, owner_session_id| {
            let runtime = Arc::clone(&runtime);
            let configuration = configuration.clone();
            let workspace_root = workspace_root.clone();
            Box::pin(async move {
                let mut cancellation_guard = ProviderTaskCancellationOnDrop::new(
                    Arc::clone(&runtime),
                    owner_session_id.clone(),
                );
                let result = runtime
                    .execute_selected_provider_tool_for_owner_in_workspace(
                        &configuration,
                        capability,
                        arguments,
                        owner_session_id,
                        workspace_root.as_deref(),
                    )
                    .await?;
                let rendered = render_provider_tool_result(workspace_root.as_deref(), result).await;
                cancellation_guard.disarm();
                rendered
            })
        }),
    }
}

/// Stages unified provider resources and builds model/card output.
/// 暂存统一供应商资源并构建模型与卡片输出。
///
/// # Parameters
/// 参数
/// `workspace_root` owns managed attachments and `result` is provider-normalized.
/// `workspace_root` 持有托管附件，`result` 已由供应商归一化。
///
/// # Returns
/// 返回值
/// A model-visible local-file result with generated-attachment host metadata.
/// 返回包含生成附件宿主元数据的模型可见本地文件结果。
async fn render_provider_tool_result(
    workspace_root: Option<&Path>,
    result: ProviderToolExecutionResult,
) -> Result<VulcanToolInvocationResult, String> {
    match result.output {
        ProviderToolNormalizedOutput::Text(text) => {
            let payload = json!({
                "kind": "text",
                "text": text,
                "upstream_response_id": result.upstream_response_id
            });
            Ok(
                VulcanToolInvocationResult::from_model_output(payload.clone()).with_host_result(
                    VulcanToolHostResult {
                        kind: String::from("provider_tool_output"),
                        payload,
                    },
                ),
            )
        }
        ProviderToolNormalizedOutput::Resources(resources) => {
            render_provider_resource_result(workspace_root, result.upstream_response_id, resources)
                .await
        }
        ProviderToolNormalizedOutput::Search(search) => {
            let payload = json!({
                "kind": "search",
                "search": search,
                "upstream_response_id": result.upstream_response_id
            });
            Ok(
                VulcanToolInvocationResult::from_model_output(payload.clone()).with_host_result(
                    VulcanToolHostResult {
                        kind: String::from("provider_tool_output"),
                        payload,
                    },
                ),
            )
        }
        ProviderToolNormalizedOutput::Extract(extract) => {
            let payload = json!({
                "kind": "extract",
                "extract": extract,
                "upstream_response_id": result.upstream_response_id
            });
            Ok(
                VulcanToolInvocationResult::from_model_output(payload.clone()).with_host_result(
                    VulcanToolHostResult {
                        kind: String::from("provider_tool_output"),
                        payload,
                    },
                ),
            )
        }
        ProviderToolNormalizedOutput::Embedding(embedding) => {
            let payload = json!({
                "kind": "embedding",
                "embedding": embedding,
                "upstream_response_id": result.upstream_response_id
            });
            Ok(
                VulcanToolInvocationResult::from_model_output(payload.clone()).with_host_result(
                    VulcanToolHostResult {
                        kind: String::from("provider_tool_output"),
                        payload,
                    },
                ),
            )
        }
        ProviderToolNormalizedOutput::Rerank(rerank) => {
            let payload = json!({
                "kind": "rerank",
                "rerank": rerank,
                "upstream_response_id": result.upstream_response_id
            });
            Ok(
                VulcanToolInvocationResult::from_model_output(payload.clone()).with_host_result(
                    VulcanToolHostResult {
                        kind: String::from("provider_tool_output"),
                        payload,
                    },
                ),
            )
        }
        ProviderToolNormalizedOutput::Composite(composite) => {
            let resources = stage_provider_resources(workspace_root, composite.resources).await?;
            let payload = json!({
                "kind": "composite",
                "composite": {
                    "text": composite.text,
                    "resources": resources,
                    "embeddings": composite.embeddings,
                    "reranks": composite.reranks,
                    "searches": composite.searches,
                    "extracts": composite.extracts
                },
                "upstream_response_id": result.upstream_response_id
            });
            Ok(
                VulcanToolInvocationResult::from_model_output(payload.clone()).with_host_result(
                    VulcanToolHostResult {
                        kind: String::from("provider_tool_output"),
                        payload,
                    },
                ),
            )
        }
    }
}

/// Stages unified provider resources and builds model/card output.
/// 暂存统一供应商资源并构建模型与卡片输出。
///
/// # Parameters
/// 参数
/// `workspace_root`, `upstream_response_id`, and `resources` are provider-normalized.
/// `workspace_root`、`upstream_response_id` 与 `resources` 已由供应商归一化。
///
/// # Returns
/// 返回值
/// A model-visible local-file result with generated-attachment host metadata.
/// 返回包含生成附件宿主元数据的模型可见本地文件结果。
async fn render_provider_resource_result(
    workspace_root: Option<&Path>,
    upstream_response_id: Option<String>,
    resources: Vec<vulcan_provider_adapters::ProviderToolGeneratedResource>,
) -> Result<VulcanToolInvocationResult, String> {
    let outputs = stage_provider_resources(workspace_root, resources).await?;
    let payload = json!({
        "outputs": outputs,
        "upstream_response_id": upstream_response_id
    });
    let local_files = provider_resource_local_files(outputs.as_slice());
    Ok(VulcanToolInvocationResult::from_model_output(json!({
        "resources": {
            "local_files": local_files
        }
    }))
    .with_host_result(VulcanToolHostResult {
        kind: String::from("generated_attachments"),
        payload,
    }))
}

/// Stages normalized provider resources and returns unified local descriptors.
/// 暂存归一化供应商资源并返回统一本地描述。
///
/// # Parameters
/// 参数
/// `workspace_root` owns managed attachments and `resources` are provider-normalized.
/// `workspace_root` 持有托管附件，`resources` 已由供应商归一化。
///
/// # Returns
/// 返回值
/// Ordered local descriptors suitable for model output and host cards.
/// 返回适用于模型输出与宿主卡片的有序本地描述。
async fn stage_provider_resources(
    workspace_root: Option<&Path>,
    resources: Vec<vulcan_provider_adapters::ProviderToolGeneratedResource>,
) -> Result<Vec<Value>, String> {
    if resources.is_empty() {
        return Ok(Vec::new());
    }
    let workspace_root = workspace_root.ok_or_else(|| {
        String::from("Provider tools that generate resources require an active workspace root.")
    })?;
    let workspace_text = vulcan_runtime_assets::native_host_path_text_from_path(workspace_root);
    let mut outputs = Vec::with_capacity(resources.len());
    for resource in resources {
        let descriptor = attachment_descriptor_from_mime_type(resource.mime_type.as_str())
            .ok_or_else(|| {
                format!(
                    "Provider tool resource `{}` has unsupported MIME type `{}`.",
                    resource.output_id, resource.mime_type
                )
            })?;
        let suggested_name = format!("{}.{}", resource.output_id, descriptor.extension);
        let attachment = match (
            resource.bytes,
            resource.download_url.as_deref(),
            resource.download_policy.as_ref(),
        ) {
            (Some(bytes), None, None) => stage_attachment_bytes(
                workspace_text.as_str(),
                &descriptor,
                suggested_name.as_str(),
                None,
                bytes.as_slice(),
            )?,
            (None, Some(download_url), Some(download_policy)) => {
                let temporary_path = download_provider_resource_to_temp(
                    download_url,
                    resource.kind.as_str(),
                    download_policy,
                )
                .await?;
                stage_attachment_file_streaming(
                    workspace_text.as_str(),
                    &descriptor,
                    suggested_name.as_str(),
                    temporary_path.as_ref(),
                )?
            }
            _ => {
                return Err(format!(
                    "Provider tool resource `{}` has an invalid carrier.",
                    resource.output_id
                ));
            }
        };
        outputs.push(json!({
            "kind": resource.kind,
            "file_name": attachment.file_name,
            "local_path": attachment.local_path,
            "preview_local_path": attachment.preview_local_path,
            "mime_type": attachment.mime_type
        }));
    }
    Ok(outputs)
}

/// Collects local file paths from exact normalized resource descriptors.
/// 从精确的归一化资源描述中收集本地文件路径。
///
/// # Parameters
/// 参数
/// `resources` are descriptors produced by `stage_provider_resources`.
/// `resources` 是由 `stage_provider_resources` 产生的描述。
///
/// # Returns
/// 返回值
/// Ordered local paths registered with the tool invocation result.
/// 返回注册到工具调用结果中的有序本地路径。
fn provider_resource_local_files(resources: &[Value]) -> Vec<String> {
    resources
        .iter()
        .filter_map(|output| output.get("local_path").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

/// Downloads one bounded HTTPS provider resource into an automatically cleaned temporary file.
/// 将一个有界 HTTPS 供应商资源下载到会自动清理的临时文件。
///
/// # Parameters
/// 参数
/// `download_url` is the provider-normalized locator, `resource_kind` selects the host byte limit,
/// and `download_policy` carries the frozen manifest network authority.
/// `download_url` 是供应商归一化后的定位符，`resource_kind` 选择宿主字节上限，
/// `download_policy` 携带冻结的 Manifest 网络权限。
///
/// # Returns
/// 返回值
/// A temporary path containing non-empty content below the configured limit.
/// 返回包含低于配置上限的非空内容的临时路径。
async fn download_provider_resource_to_temp(
    download_url: &str,
    resource_kind: &str,
    download_policy: &ProviderToolResourceDownloadPolicy,
) -> Result<tempfile::TempPath, String> {
    // The host revalidates the frozen adapter authority immediately before network access.
    // 宿主在网络访问前立即复验适配器冻结的权限。
    let parsed_url = download_policy.validate_url(download_url, "Provider resource URL")?;
    let byte_limit = match resource_kind {
        "audio" => PROVIDER_TOOL_AUDIO_OUTPUT_BYTE_LIMIT,
        "video" => PROVIDER_TOOL_VIDEO_OUTPUT_BYTE_LIMIT,
        _ => PROVIDER_TOOL_DEFAULT_OUTPUT_BYTE_LIMIT,
    };
    // Redirects are disabled because the provider URL was validated before this request and a redirect could otherwise bypass that boundary.
    // 禁止重定向，因为供应商 URL 已在请求前校验；否则重定向可能绕过该边界。
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(PROVIDER_TOOL_RESOURCE_DOWNLOAD_TIMEOUT)
        .build()
        .map_err(|error| {
            format!("Provider resource download client could not be created: {error}")
        })?;
    let response = client
        .get(parsed_url)
        .send()
        .await
        .map_err(|error| format!("Provider resource download failed: {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Provider resource download failed with HTTP {}.",
            response.status().as_u16()
        ));
    }
    if response
        .content_length()
        .is_some_and(|content_length| content_length > byte_limit as u64)
    {
        return Err(String::from(
            "Provider resource exceeds the host limit for its declared kind.",
        ));
    }
    let temporary_file = tempfile::NamedTempFile::new().map_err(|error| {
        format!("Provider resource temporary file could not be created: {error}")
    })?;
    let temporary_path = temporary_file.into_temp_path();
    let temporary_path_ref: &Path = temporary_path.as_ref();
    let mut file = tokio::fs::File::create(temporary_path_ref)
        .await
        .map_err(|error| {
            format!("Provider resource temporary file could not be opened: {error}")
        })?;
    let mut stream = response.bytes_stream();
    let mut downloaded_bytes = 0_usize;
    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.map_err(|error| format!("Provider resource body could not be read: {error}"))?;
        downloaded_bytes = downloaded_bytes.saturating_add(chunk.len());
        if downloaded_bytes > byte_limit {
            return Err(String::from(
                "Provider resource exceeds the host limit for its declared kind.",
            ));
        }
        file.write_all(chunk.as_ref()).await.map_err(|error| {
            format!("Provider resource temporary file could not be written: {error}")
        })?;
    }
    if downloaded_bytes == 0 {
        return Err(String::from("Provider resource cannot be empty."));
    }
    file.flush().await.map_err(|error| {
        format!("Provider resource temporary file could not be flushed: {error}")
    })?;
    file.sync_all().await.map_err(|error| {
        format!("Provider resource temporary file could not be synchronized: {error}")
    })?;
    Ok(temporary_path)
}

#[cfg(test)]
mod tests {
    use super::{
        apply_session_provider_tool_selections, download_provider_resource_to_temp,
        render_provider_tool_result,
    };
    use serde_json::json;
    use vulcan_provider_adapters::{
        ProviderToolCompositeOutput, ProviderToolEmbeddingOutput, ProviderToolEmbeddingVector,
        ProviderToolExecutionResult, ProviderToolNormalizedOutput, ProviderToolRerankOutput,
        ProviderToolRerankResult, ProviderToolResourceDownloadPolicy, ProviderToolResourceEncoding,
    };

    /// Verifies Agent Runtime production input materialization has no provider-named branch.
    /// 验证 Agent Runtime 生产输入物化不存在供应商命名分支。
    ///
    /// The release gate performs the same repository-wide check; this focused R0 guard keeps the
    /// bridge invariant independently runnable with the crate tests.
    /// 发布门禁会执行同类仓库级检查；本聚焦 R0 守卫使 bridge 不变量可随 crate 测试独立运行。
    #[test]
    fn agent_runtime_has_no_provider_named_input_materialization() {
        // source includes this module so the production prefix is cut at the first test boundary.
        // source 包含当前测试模块，因此在首个测试边界处截取生产前缀。
        let source = include_str!("provider_tool_bridge.rs");
        // production contains only the bridge code that ships in non-test builds.
        // production 只包含非测试构建实际发布的 bridge 代码。
        let production = source
            .split_once("#[cfg(test)]")
            .map(|(production, _)| production)
            .expect("provider tool bridge must retain one explicit test boundary");
        // forbidden_fragments are assembled so the guard strings cannot satisfy themselves.
        // forbidden_fragments 通过分段组装，避免守卫字符串自身命中检查。
        let forbidden_fragments = [
            ["materialize_", "provider_tool_inputs"].concat(),
            ["Ali", "baba"].concat(),
            ["Mini", "Max"].concat(),
            ["Tavi", "ly"].concat(),
            ["ProviderTool", "AdapterKind"].concat(),
        ];
        for forbidden in forbidden_fragments {
            assert!(
                !production.contains(forbidden.as_str()),
                "Agent provider-tool bridge restored forbidden production input authority `{forbidden}`"
            );
        }
    }

    /// Verifies an explicit empty session selection disables every global provider tool.
    /// 验证显式空会话选择会禁用全部全局供应商工具。
    #[test]
    fn explicit_empty_session_selection_clears_global_targets() {
        let mut configuration = vulcan_provider_adapters::PersistedAiConfiguration::default();
        configuration.provider_tool_selections.insert(
            vulcan_provider_adapters::ProviderToolCapabilityKind::WebSearch,
            vulcan_provider_adapters::ProviderToolTargetSelection {
                connection_id: String::from("connection"),
                target_id: String::from("target"),
            },
        );

        apply_session_provider_tool_selections(&mut configuration, &Some(Default::default()))
            .expect("apply explicit empty override");

        assert!(configuration.provider_tool_selections.is_empty());
    }

    /// Verifies the shared resource boundary rejects insecure provider URLs before networking.
    /// 验证共享资源边界会在发起网络请求前拒绝不安全的供应商 URL。
    #[tokio::test]
    async fn provider_resource_download_rejects_insecure_url() {
        // The HTTPS-only policy is the exact authority under test.
        // 仅 HTTPS 策略是本测试使用的精确权限。
        let policy = ProviderToolResourceDownloadPolicy {
            encoding: ProviderToolResourceEncoding::HttpsUrl,
            host_suffixes: vec![String::from("example.com")],
        };
        let error =
            download_provider_resource_to_temp("http://example.com/output.png", "image", &policy)
                .await
                .expect_err("insecure URL must be rejected");
        assert_eq!(
            error,
            "Provider resource URL is outside the declared scheme, default port or exact operation origin, and host suffixes."
        );
    }

    /// Verifies a syntactically valid HTTPS URL cannot escape its manifest-owned host authority.
    /// 验证语法有效的 HTTPS URL 无法逃逸 Manifest 持有的宿主权限。
    #[tokio::test]
    async fn provider_resource_download_rejects_undeclared_host() {
        // The exact host matcher intentionally excludes every subdomain and alternate domain.
        // 精确宿主匹配项刻意排除全部子域与其他域名。
        let policy = ProviderToolResourceDownloadPolicy {
            encoding: ProviderToolResourceEncoding::HttpsUrl,
            host_suffixes: vec![String::from("example.com")],
        };
        let error = download_provider_resource_to_temp(
            "https://assets.example.org/output.png",
            "image",
            &policy,
        )
        .await
        .expect_err("undeclared host must be rejected before networking");
        assert_eq!(
            error,
            "Provider resource URL is outside the declared scheme, default port or exact operation origin, and host suffixes."
        );
    }

    /// Verifies embedding output remains typed in both model and host payloads.
    /// 验证向量输出在模型与宿主载荷中均保持强类型。
    #[tokio::test]
    async fn embedding_output_preserves_vector_indexes_and_values() {
        let result = render_provider_tool_result(
            None,
            ProviderToolExecutionResult {
                upstream_response_id: Some(String::from("embedding-response")),
                output: ProviderToolNormalizedOutput::Embedding(ProviderToolEmbeddingOutput {
                    model: Some(String::from("embedding-model")),
                    vectors: vec![ProviderToolEmbeddingVector {
                        index: 2,
                        values: vec![0.25, -0.5],
                    }],
                }),
            },
        )
        .await
        .expect("embedding output");

        assert_eq!(result.model_output["kind"], "embedding");
        assert_eq!(result.model_output["embedding"]["vectors"][0]["index"], 2);
        assert_eq!(
            result.host_result.as_ref().expect("host output").payload["embedding"]["vectors"][0]["values"],
            json!([0.25, -0.5])
        );
    }

    /// Verifies a resource-free composite output does not require a workspace.
    /// 验证不含资源的复合输出不要求工作区。
    #[tokio::test]
    async fn resource_free_composite_output_keeps_each_semantic_kind() {
        let result = render_provider_tool_result(
            None,
            ProviderToolExecutionResult {
                upstream_response_id: None,
                output: ProviderToolNormalizedOutput::Composite(ProviderToolCompositeOutput {
                    text: vec![String::from("summary")],
                    resources: Vec::new(),
                    embeddings: Vec::new(),
                    reranks: vec![ProviderToolRerankOutput {
                        model: None,
                        results: vec![ProviderToolRerankResult {
                            index: 1,
                            score: 0.75,
                            document: Some(String::from("candidate")),
                        }],
                    }],
                    searches: Vec::new(),
                    extracts: Vec::new(),
                }),
            },
        )
        .await
        .expect("composite output");

        assert_eq!(result.model_output["kind"], "composite");
        assert_eq!(result.model_output["composite"]["text"], json!(["summary"]));
        assert_eq!(
            result.model_output["composite"]["reranks"][0]["results"][0]["score"],
            0.75
        );
    }

    /// Verifies native web search suppresses only the external search capability.
    /// 验证原生网页搜索只抑制外部搜索能力。
    #[test]
    fn native_web_search_suppression_preserves_web_extract_and_other_tools() {
        use super::provider_tool_capability_is_suppressed;
        use vulcan_provider_adapters::ProviderToolCapabilityKind;

        assert!(provider_tool_capability_is_suppressed(
            ProviderToolCapabilityKind::WebSearch,
            true
        ));
        assert!(!provider_tool_capability_is_suppressed(
            ProviderToolCapabilityKind::WebExtract,
            true
        ));
        assert!(!provider_tool_capability_is_suppressed(
            ProviderToolCapabilityKind::MediaAnalyze,
            true
        ));
        assert!(!provider_tool_capability_is_suppressed(
            ProviderToolCapabilityKind::WebSearch,
            false
        ));
    }
}
