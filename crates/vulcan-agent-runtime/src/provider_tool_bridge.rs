//! Per-turn provider tool registration and host-side resource materialization.
//! 每回合供应商工具注册与宿主侧资源物化。

use crate::attachment_assets::{attachment_descriptor_from_mime_type, stage_attachment_bytes};
use base64::Engine;
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use vulcan_provider_adapters::{
    AvailableProviderToolTarget, PersistedAiConfiguration, ProviderToolAdapterKind,
    ProviderToolExecutionResult, ProviderToolNormalizedOutput, PublicAiRuntime,
};
use vulcan_tools_manager::{
    VulcanPlanToolPolicy, VulcanToolCandidate, VulcanToolDescriptor, VulcanToolHostResult,
    VulcanToolInvocationResult, VulcanToolSourceKind,
};

/// Maximum local image size accepted by provider input materializers.
/// 供应商输入物化器接受的本地图片最大字节数。
const PROVIDER_TOOL_IMAGE_INPUT_BYTE_LIMIT: usize = 50 * 1024 * 1024;
/// Maximum generated image or generic file size accepted by the host.
/// 宿主接受的生成图片或通用文件最大字节数。
const PROVIDER_TOOL_DEFAULT_OUTPUT_BYTE_LIMIT: usize = 50 * 1024 * 1024;
/// Maximum generated audio size accepted by the host.
/// 宿主接受的生成音频最大字节数。
const PROVIDER_TOOL_AUDIO_OUTPUT_BYTE_LIMIT: usize = 100 * 1024 * 1024;
/// Maximum generated video size accepted by the host.
/// 宿主接受的生成视频最大字节数。
const PROVIDER_TOOL_VIDEO_OUTPUT_BYTE_LIMIT: usize = 512 * 1024 * 1024;

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
) -> Result<Vec<VulcanToolCandidate>, String> {
    apply_session_provider_tool_selections(&mut configuration, session_selections)?;
    let available = runtime.available_provider_tool_targets(&configuration)?;
    let mut candidates = Vec::new();
    for (capability, selection) in &configuration.provider_tool_selections {
        if configuration
            .provider_tool_disabled_targets
            .get(capability)
            .is_some_and(|disabled| disabled.contains(selection))
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
            continue;
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
    let adapter = selected.target.execution.adapter;
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
        invoke: Arc::new(move |arguments, _owner_session_id| {
            let runtime = Arc::clone(&runtime);
            let configuration = configuration.clone();
            let workspace_root = workspace_root.clone();
            Box::pin(async move {
                let projected_arguments = materialize_provider_tool_inputs(
                    adapter,
                    workspace_root.as_deref(),
                    arguments,
                )?;
                let result = runtime
                    .execute_selected_provider_tool(&configuration, capability, projected_arguments)
                    .await?;
                render_provider_tool_result(workspace_root.as_deref(), result).await
            })
        }),
    }
}

/// Converts local semantic resource paths into provider-safe carriers.
/// 将本地语义资源路径转换为供应商安全载体。
///
/// # Parameters
/// 参数
/// `adapter`, `workspace_root`, and `arguments` belong to one selected target.
/// `adapter`、`workspace_root` 与 `arguments` 属于一个选定目标。
///
/// # Returns
/// 返回值
/// Provider-adapter arguments that contain no local path for handled resource fields.
/// 返回已处理资源字段中不含本地路径的供应商适配器参数。
fn materialize_provider_tool_inputs(
    adapter: ProviderToolAdapterKind,
    workspace_root: Option<&Path>,
    mut arguments: Value,
) -> Result<Value, String> {
    match adapter {
        ProviderToolAdapterKind::MiniMaxImageGenerate => {
            let Some(references) = arguments
                .as_object_mut()
                .and_then(|object| object.get_mut("subject_references"))
            else {
                return Ok(arguments);
            };
            let paths = references.as_array().ok_or_else(|| {
                String::from("Provider image subject_references must be an array of local paths.")
            })?;
            let materialized = paths
                .iter()
                .map(|path| {
                    let path = path.as_str().ok_or_else(|| {
                        String::from(
                            "Provider image subject reference must be a local path string.",
                        )
                    })?;
                    local_image_data_url(workspace_root, path, false)
                })
                .collect::<Result<Vec<_>, String>>()?;
            *references = Value::Array(materialized.into_iter().map(Value::String).collect());
            Ok(arguments)
        }
        ProviderToolAdapterKind::MiniMaxCodingPlanVision => {
            let object = arguments.as_object_mut().ok_or_else(|| {
                String::from("MiniMax image-understanding arguments must be an object.")
            })?;
            let image_path = object
                .remove("image_path")
                .and_then(|value| value.as_str().map(str::to_string))
                .ok_or_else(|| {
                    String::from(
                        "MiniMax image-understanding image_path must be a local path string.",
                    )
                })?;
            let image_url = local_image_data_url(workspace_root, image_path.as_str(), true)?;
            object.insert(String::from("image_url"), Value::String(image_url));
            Ok(arguments)
        }
        ProviderToolAdapterKind::MiniMaxVideoGenerate => {
            let object = arguments.as_object_mut().ok_or_else(|| {
                String::from("MiniMax video-generation arguments must be an object.")
            })?;
            for (path_field, image_field) in [
                ("first_frame_path", "first_frame_image"),
                ("last_frame_path", "last_frame_image"),
                ("subject_reference_path", "subject_reference_image"),
            ] {
                let Some(path) = object
                    .remove(path_field)
                    .map(|value| {
                        value.as_str().map(str::to_string).ok_or_else(|| {
                            format!(
                                "MiniMax video-generation {path_field} must be a local path string."
                            )
                        })
                    })
                    .transpose()?
                else {
                    continue;
                };
                let image = local_image_data_url(workspace_root, path.as_str(), true)?;
                object.insert(String::from(image_field), Value::String(image));
            }
            Ok(arguments)
        }
        _ => Ok(arguments),
    }
}

/// Reads one local provider image and returns a bounded Data URL.
/// 读取一张本地供应商图片并返回有界 Data URL。
///
/// # Parameters
/// 参数
/// `workspace_root` resolves relative paths, `path` is supplied by the AI tool call, and
/// `allow_webp` follows the exact selected adapter contract.
/// `workspace_root` 用于解析相对路径，`path` 由 AI 工具调用提供，`allow_webp`
/// 遵循精确的所选适配器契约。
///
/// # Returns
/// 返回值
/// A provider-safe inline carrier.
/// 返回供应商安全的内联载体。
fn local_image_data_url(
    workspace_root: Option<&Path>,
    path: &str,
    allow_webp: bool,
) -> Result<String, String> {
    let source = PathBuf::from(path);
    let resolved = if source.is_absolute() {
        source
    } else {
        workspace_root
            .ok_or_else(|| {
                String::from(
                    "Relative provider tool resource paths require an active workspace root.",
                )
            })?
            .join(source)
    };
    let canonical = std::fs::canonicalize(&resolved).map_err(|error| {
        format!(
            "Provider tool resource `{}` cannot be resolved: {error}",
            resolved.display()
        )
    })?;
    let descriptor = crate::attachment_assets::attachment_descriptor_from_path(&canonical)
        .filter(|descriptor| {
            matches!(descriptor.mime_type, "image/jpeg" | "image/png")
                || allow_webp && descriptor.mime_type == "image/webp"
        })
        .ok_or_else(|| {
            format!(
                "Provider tool resource `{}` has an unsupported image format.",
                canonical.display()
            )
        })?;
    let bytes = std::fs::read(&canonical).map_err(|error| {
        format!(
            "Provider tool resource `{}` cannot be read: {error}",
            canonical.display()
        )
    })?;
    if bytes.is_empty() || bytes.len() > PROVIDER_TOOL_IMAGE_INPUT_BYTE_LIMIT {
        return Err(format!(
            "Provider tool resource `{}` must be non-empty and no larger than 50 MiB.",
            canonical.display()
        ));
    }
    Ok(format!(
        "data:{};base64,{}",
        descriptor.mime_type,
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
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
        let bytes = match (resource.bytes, resource.download_url.as_deref()) {
            (Some(bytes), None) => bytes,
            (None, Some(download_url)) => {
                download_provider_resource(download_url, resource.kind.as_str()).await?
            }
            _ => {
                return Err(format!(
                    "Provider tool resource `{}` has an invalid carrier.",
                    resource.output_id
                ));
            }
        };
        let descriptor = attachment_descriptor_from_mime_type(resource.mime_type.as_str())
            .ok_or_else(|| {
                format!(
                    "Provider tool resource `{}` has unsupported MIME type `{}`.",
                    resource.output_id, resource.mime_type
                )
            })?;
        let suggested_name = format!("{}.{}", resource.output_id, descriptor.extension);
        let attachment = stage_attachment_bytes(
            workspace_text.as_str(),
            &descriptor,
            suggested_name.as_str(),
            None,
            bytes.as_slice(),
        )?;
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

/// Downloads one bounded HTTPS provider resource.
/// 下载一个有界 HTTPS 供应商资源。
///
/// # Parameters
/// 参数
/// `download_url` is the provider-normalized public URL.
/// `download_url` 是供应商归一化后的公开 URL。
///
/// # Returns
/// 返回值
/// Non-empty bytes below the configured limit.
/// 返回低于配置上限的非空字节。
async fn download_provider_resource(
    download_url: &str,
    resource_kind: &str,
) -> Result<Vec<u8>, String> {
    let parsed_url = reqwest::Url::parse(download_url)
        .map_err(|_| String::from("Provider resource URL is invalid."))?;
    if parsed_url.scheme() != "https"
        || parsed_url.host_str().is_none()
        || !parsed_url.username().is_empty()
        || parsed_url.password().is_some()
    {
        return Err(String::from(
            "Provider resource URL must be a credential-free absolute HTTPS URL.",
        ));
    }
    let byte_limit = match resource_kind {
        "audio" => PROVIDER_TOOL_AUDIO_OUTPUT_BYTE_LIMIT,
        "video" => PROVIDER_TOOL_VIDEO_OUTPUT_BYTE_LIMIT,
        _ => PROVIDER_TOOL_DEFAULT_OUTPUT_BYTE_LIMIT,
    };
    let response = reqwest::Client::new()
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
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.map_err(|error| format!("Provider resource body could not be read: {error}"))?;
        if bytes.len().saturating_add(chunk.len()) > byte_limit {
            return Err(String::from(
                "Provider resource exceeds the host limit for its declared kind.",
            ));
        }
        bytes.extend_from_slice(chunk.as_ref());
    }
    if bytes.is_empty() {
        return Err(String::from("Provider resource cannot be empty."));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::{
        apply_session_provider_tool_selections, download_provider_resource,
        materialize_provider_tool_inputs, render_provider_tool_result,
    };
    use serde_json::json;
    use vulcan_provider_adapters::{
        ProviderToolAdapterKind, ProviderToolCompositeOutput, ProviderToolEmbeddingOutput,
        ProviderToolEmbeddingVector, ProviderToolExecutionResult, ProviderToolNormalizedOutput,
        ProviderToolRerankOutput, ProviderToolRerankResult,
    };

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

    /// Verifies capabilities without resources preserve exact native arguments.
    /// 验证不含资源的能力会保留精确原生参数。
    #[test]
    fn non_resource_capability_keeps_arguments() {
        let arguments = json!({"query": "Vulcan"});
        assert_eq!(
            materialize_provider_tool_inputs(
                ProviderToolAdapterKind::TavilySearch,
                None,
                arguments.clone()
            )
            .expect("unchanged arguments"),
            arguments
        );
    }

    /// Verifies the shared resource boundary rejects insecure provider URLs before networking.
    /// 验证共享资源边界会在发起网络请求前拒绝不安全的供应商 URL。
    #[tokio::test]
    async fn provider_resource_download_rejects_insecure_url() {
        let error = download_provider_resource("http://example.com/output.png", "image")
            .await
            .expect_err("insecure URL must be rejected");
        assert!(error.contains("absolute HTTPS"));
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
}
