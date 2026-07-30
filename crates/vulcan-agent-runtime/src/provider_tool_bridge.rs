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
        invoke: Arc::new(move |arguments, owner_session_id| {
            let runtime = Arc::clone(&runtime);
            let configuration = configuration.clone();
            let workspace_root = workspace_root.clone();
            Box::pin(async move {
                let projected_arguments = materialize_provider_tool_inputs(
                    adapter,
                    workspace_root.as_deref(),
                    arguments,
                )?;
                let mut cancellation_guard = ProviderTaskCancellationOnDrop::new(
                    Arc::clone(&runtime),
                    owner_session_id.clone(),
                );
                let result = runtime
                    .execute_selected_provider_tool_for_owner(
                        &configuration,
                        capability,
                        projected_arguments,
                        owner_session_id,
                    )
                    .await?;
                let rendered = render_provider_tool_result(workspace_root.as_deref(), result).await;
                cancellation_guard.disarm();
                rendered
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
        ProviderToolAdapterKind::AlibabaQwenImageEdit => materialize_local_image_path_array(
            workspace_root,
            &mut arguments,
            "source_paths",
            "source_images",
            &[
                "image/jpeg",
                "image/png",
                "image/bmp",
                "image/webp",
                "image/gif",
            ],
        ),
        ProviderToolAdapterKind::AlibabaWanImageGenerate => materialize_local_image_path_array(
            workspace_root,
            &mut arguments,
            "reference_paths",
            "reference_images",
            &[
                "image/jpeg",
                "image/png",
                "image/bmp",
                "image/webp",
                "image/gif",
            ],
        ),
        ProviderToolAdapterKind::AlibabaQwenSpeechTranscribe => {
            let object = arguments.as_object_mut().ok_or_else(|| {
                String::from("Alibaba speech-transcription arguments must be an object.")
            })?;
            let audio_path = object
                .remove("audio_path")
                .and_then(|value| value.as_str().map(str::to_string))
                .ok_or_else(|| {
                    String::from(
                        "Alibaba speech-transcription audio_path must be a local path string.",
                    )
                })?;
            let audio_url = local_media_data_url_for_mime_types(
                workspace_root,
                audio_path.as_str(),
                &[
                    "audio/mpeg",
                    "audio/mp3",
                    "audio/wav",
                    "audio/x-wav",
                    "audio/mp4",
                    "audio/aac",
                    "audio/ogg",
                    "audio/flac",
                    "audio/webm",
                ],
            )?;
            object.insert(String::from("audio_url"), Value::String(audio_url));
            Ok(arguments)
        }
        ProviderToolAdapterKind::AlibabaQwenOmniMediaAnalyze => {
            let object = arguments.as_object_mut().ok_or_else(|| {
                String::from("Alibaba media-analysis arguments must be an object.")
            })?;
            let paths = object
                .remove("media_paths")
                .and_then(|value| value.as_array().cloned())
                .ok_or_else(|| {
                    String::from("Alibaba media-analysis media_paths must be an array.")
                })?;
            let media = paths
                .iter()
                .map(|path| {
                    let path = path.as_str().ok_or_else(|| {
                        String::from("Alibaba media-analysis paths must be local path strings.")
                    })?;
                    local_typed_media_data_url(
                        workspace_root,
                        path,
                        &[
                            "image/jpeg",
                            "image/png",
                            "image/webp",
                            "audio/wav",
                            "audio/x-wav",
                            "audio/mpeg",
                            "audio/mp3",
                            "audio/amr",
                            "audio/aac",
                            "audio/mp4",
                            "audio/x-m4a",
                            "audio/ogg",
                            "audio/3gpp",
                        ],
                    )
                })
                .collect::<Result<Vec<_>, String>>()?;
            object.insert(String::from("media"), Value::Array(media));
            Ok(arguments)
        }
        ProviderToolAdapterKind::AlibabaFunAsrTranscribe => {
            let object = arguments
                .as_object_mut()
                .ok_or_else(|| String::from("Alibaba Fun-ASR arguments must be an object."))?;
            let paths = object
                .remove("source_paths")
                .and_then(|value| value.as_array().cloned())
                .ok_or_else(|| String::from("Alibaba Fun-ASR source_paths must be an array."))?;
            let sources = paths
                .iter()
                .map(|path| {
                    let path = path.as_str().ok_or_else(|| {
                        String::from("Alibaba Fun-ASR sources must be local path strings.")
                    })?;
                    local_provider_file_descriptor(
                        workspace_root,
                        path,
                        &[
                            "audio/aac",
                            "audio/amr",
                            "audio/flac",
                            "audio/mp4",
                            "audio/mpeg",
                            "audio/ogg",
                            "audio/opus",
                            "audio/wav",
                            "audio/x-ms-wma",
                            "video/mp4",
                            "video/mpeg",
                            "video/quicktime",
                            "video/webm",
                            "video/x-flv",
                            "video/x-matroska",
                            "video/x-ms-wmv",
                            "video/x-msvideo",
                        ],
                        2 * 1024 * 1024 * 1024,
                    )
                })
                .collect::<Result<Vec<_>, String>>()?;
            object.insert(String::from("sources"), Value::Array(sources));
            Ok(arguments)
        }
        ProviderToolAdapterKind::AlibabaHappyHorseVideoGenerate => {
            materialize_alibaba_happyhorse_generation(workspace_root, &mut arguments)
        }
        ProviderToolAdapterKind::AlibabaHappyHorseVideoEdit => {
            materialize_alibaba_happyhorse_edit(workspace_root, &mut arguments)
        }
        ProviderToolAdapterKind::AlibabaWanVideoGenerate => {
            materialize_alibaba_wan_video(workspace_root, &mut arguments)
        }
        _ => Ok(arguments),
    }
}

/// Materializes exact Wan 2.7 local media without exposing provider handles to AI.
/// 物化精确 Wan 2.7 本地媒体且不向 AI 暴露供应商句柄。
///
/// # Parameters
/// 参数
/// `workspace_root` resolves local paths and `arguments` contains one model-specific schema.
/// `workspace_root` 解析本地路径，`arguments` 包含一个模型专属 Schema。
///
/// # Returns
/// 返回值
/// Image Data URLs and an optional trusted audio upload descriptor.
/// 返回图片 Data URL 与可选的受信任音频上传描述符。
fn materialize_alibaba_wan_video(
    workspace_root: Option<&Path>,
    arguments: &mut Value,
) -> Result<Value, String> {
    let object = arguments
        .as_object_mut()
        .ok_or_else(|| String::from("Alibaba Wan video arguments must be an object."))?;
    for (path_key, carrier_key) in [
        ("first_frame_path", "first_frame"),
        ("last_frame_path", "last_frame"),
    ] {
        let Some(path) = object.remove(path_key) else {
            continue;
        };
        let path = path
            .as_str()
            .ok_or_else(|| format!("Alibaba Wan video {path_key} must be a local path string."))?;
        let data_url = local_media_data_url_for_mime_types(
            workspace_root,
            path,
            &["image/jpeg", "image/png", "image/webp"],
        )?;
        object.insert(carrier_key.to_string(), Value::String(data_url));
    }
    if let Some(path) = object.remove("audio_path") {
        let path = path.as_str().ok_or_else(|| {
            String::from("Alibaba Wan video audio_path must be a local path string.")
        })?;
        let descriptor = local_provider_file_descriptor(
            workspace_root,
            path,
            &["audio/mpeg", "audio/mp3", "audio/wav", "audio/x-wav"],
            2 * 1024 * 1024 * 1024,
        )?;
        object.insert(String::from("audio_source"), descriptor);
    }
    Ok(arguments.clone())
}

/// Materializes exact HappyHorse generation resources without exposing provider handles to AI.
/// 物化精确 HappyHorse 生成资源且不向 AI 暴露供应商句柄。
///
/// # Parameters
/// 参数
/// `workspace_root` resolves local paths and `arguments` contains one model-specific schema.
/// `workspace_root` 解析本地路径，`arguments` 包含一个模型专属 Schema。
///
/// # Returns
/// 返回值
/// Trusted internal image carriers and upload descriptors.
/// 返回受信任的内部图片载体与上传描述符。
fn materialize_alibaba_happyhorse_generation(
    workspace_root: Option<&Path>,
    arguments: &mut Value,
) -> Result<Value, String> {
    let object = arguments
        .as_object_mut()
        .ok_or_else(|| String::from("Alibaba HappyHorse arguments must be an object."))?;
    if let Some(path) = object
        .remove("first_frame_path")
        .map(|value| {
            value.as_str().map(str::to_string).ok_or_else(|| {
                String::from("HappyHorse first_frame_path must be a local path string.")
            })
        })
        .transpose()?
    {
        let image = local_image_data_url(workspace_root, path.as_str(), true)?;
        object.insert(String::from("first_frame_image"), Value::String(image));
    }
    let Some(references) = object.remove("references") else {
        return Ok(arguments.clone());
    };
    let references = references
        .as_array()
        .ok_or_else(|| String::from("HappyHorse references must be an array."))?;
    let materialized = references
        .iter()
        .map(|reference| {
            let reference = reference
                .as_object()
                .ok_or_else(|| String::from("HappyHorse reference must be an object."))?;
            let path = reference
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| String::from("HappyHorse reference path is required."))?;
            let kind = reference
                .get("kind")
                .and_then(Value::as_str)
                .ok_or_else(|| String::from("HappyHorse reference kind is required."))?;
            let carrier = match kind {
                "image" => Value::String(local_image_data_url(workspace_root, path, true)?),
                "video" => local_provider_file_descriptor(
                    workspace_root,
                    path,
                    &["video/mp4", "video/quicktime"],
                    2 * 1024 * 1024 * 1024,
                )?,
                _ => return Err(String::from("HappyHorse reference kind is unsupported.")),
            };
            let voice = reference
                .get("voice_path")
                .map(|value| {
                    let voice_path = value.as_str().ok_or_else(|| {
                        String::from("HappyHorse voice_path must be a local path string.")
                    })?;
                    local_provider_file_descriptor(
                        workspace_root,
                        voice_path,
                        &[
                            "audio/mpeg",
                            "audio/wav",
                            "audio/mp4",
                            "audio/aac",
                            "audio/flac",
                        ],
                        2 * 1024 * 1024 * 1024,
                    )
                })
                .transpose()?;
            Ok(json!({
                "kind": kind,
                "carrier": carrier,
                "voice": voice
            }))
        })
        .collect::<Result<Vec<_>, String>>()?;
    object.insert(
        String::from("materialized_references"),
        Value::Array(materialized),
    );
    Ok(arguments.clone())
}

/// Materializes one HappyHorse edit source and optional image references.
/// 物化一个 HappyHorse 编辑来源及可选图片参考。
///
/// # Parameters
/// 参数
/// `workspace_root` resolves local files and `arguments` contains the edit schema.
/// `workspace_root` 解析本地文件，`arguments` 包含编辑 Schema。
///
/// # Returns
/// 返回值
/// Trusted internal source descriptor and inline reference images.
/// 返回受信任的内部来源描述符与内联参考图片。
fn materialize_alibaba_happyhorse_edit(
    workspace_root: Option<&Path>,
    arguments: &mut Value,
) -> Result<Value, String> {
    let object = arguments
        .as_object_mut()
        .ok_or_else(|| String::from("Alibaba HappyHorse edit arguments must be an object."))?;
    let source_path = object
        .remove("source_video_path")
        .and_then(|value| value.as_str().map(str::to_string))
        .ok_or_else(|| String::from("HappyHorse source_video_path must be a local path string."))?;
    let source = local_provider_file_descriptor(
        workspace_root,
        source_path.as_str(),
        &["video/mp4", "video/quicktime"],
        2 * 1024 * 1024 * 1024,
    )?;
    object.insert(String::from("source_video"), source);
    let Some(reference_paths) = object.remove("reference_image_paths") else {
        return Ok(arguments.clone());
    };
    let reference_paths = reference_paths
        .as_array()
        .ok_or_else(|| String::from("HappyHorse reference_image_paths must be an array."))?;
    let references = reference_paths
        .iter()
        .map(|path| {
            let path = path.as_str().ok_or_else(|| {
                String::from("HappyHorse reference image must be a local path string.")
            })?;
            local_image_data_url(workspace_root, path, true).map(Value::String)
        })
        .collect::<Result<Vec<_>, String>>()?;
    object.insert(String::from("reference_images"), Value::Array(references));
    Ok(arguments.clone())
}

/// Replaces one optional AI-visible local-path array with host-created image Data URLs.
/// 将一个可选 AI 可见本地路径数组替换为宿主创建的图片 Data URL。
///
/// # Parameters
/// 参数
/// `workspace_root`, `arguments`, source/destination fields, and `allowed_mime_types` define
/// one exact adapter-owned resource projection.
/// `workspace_root`、`arguments`、来源/目标字段及 `allowed_mime_types`
/// 定义一项精确适配器持有的资源投影。
///
/// # Returns
/// 返回值
/// Updated arguments containing no local path in the handled field.
/// 返回在已处理字段中不含本地路径的更新参数。
fn materialize_local_image_path_array(
    workspace_root: Option<&Path>,
    arguments: &mut Value,
    source_field: &str,
    destination_field: &str,
    allowed_mime_types: &[&str],
) -> Result<Value, String> {
    let object = arguments
        .as_object_mut()
        .ok_or_else(|| String::from("Provider image tool arguments must be a JSON object."))?;
    let Some(paths) = object.remove(source_field) else {
        return Ok(arguments.clone());
    };
    let paths = paths
        .as_array()
        .ok_or_else(|| format!("Provider image {source_field} must be an array of local paths."))?;
    let materialized = paths
        .iter()
        .map(|path| {
            let path = path.as_str().ok_or_else(|| {
                format!("Provider image {source_field} entries must be local path strings.")
            })?;
            local_media_data_url_for_mime_types(workspace_root, path, allowed_mime_types)
                .map(Value::String)
        })
        .collect::<Result<Vec<_>, String>>()?;
    object.insert(destination_field.to_string(), Value::Array(materialized));
    Ok(arguments.clone())
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
    let allowed_mime_types = if allow_webp {
        &["image/jpeg", "image/png", "image/webp"][..]
    } else {
        &["image/jpeg", "image/png"][..]
    };
    local_media_data_url_for_mime_types(workspace_root, path, allowed_mime_types)
}

/// Reads one local media file using the exact MIME set owned by the selected adapter.
/// 使用所选适配器持有的精确 MIME 集读取一个本地媒体文件。
///
/// # Parameters
/// 参数
/// `workspace_root` resolves relative paths, `path` identifies the source, and
/// `allowed_mime_types` is the closed provider contract.
/// `workspace_root` 解析相对路径，`path` 标识来源，`allowed_mime_types`
/// 是封闭供应商合同。
///
/// # Returns
/// 返回值
/// A bounded provider-safe media Data URL.
/// 返回有界且供应商安全的媒体 Data URL。
fn local_media_data_url_for_mime_types(
    workspace_root: Option<&Path>,
    path: &str,
    allowed_mime_types: &[&str],
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
        .filter(|descriptor| allowed_mime_types.contains(&descriptor.mime_type))
        .ok_or_else(|| {
            format!(
                "Provider tool resource `{}` has an unsupported media format.",
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

/// Builds a typed inline carrier for one verified image or audio file.
/// 为一个已验证图片或音频文件构建带类型内联载体。
///
/// # Parameters
/// 参数
/// `workspace_root`, `path`, and `allowed_mime_types` define one exact resource contract.
/// `workspace_root`、`path` 与 `allowed_mime_types` 定义一个精确资源合同。
///
/// # Returns
/// 返回值
/// An internal object containing the semantic media kind, MIME type, and Data URL.
/// 返回包含语义媒体类别、MIME 类型和 Data URL 的内部对象。
fn local_typed_media_data_url(
    workspace_root: Option<&Path>,
    path: &str,
    allowed_mime_types: &[&str],
) -> Result<Value, String> {
    let canonical = canonical_provider_resource_path(workspace_root, path)?;
    let descriptor = crate::attachment_assets::attachment_descriptor_from_path(&canonical)
        .filter(|descriptor| allowed_mime_types.contains(&descriptor.mime_type))
        .ok_or_else(|| {
            format!(
                "Provider tool resource `{}` has an unsupported media format.",
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
    let kind = if descriptor.mime_type.starts_with("image/") {
        "image"
    } else if descriptor.mime_type.starts_with("audio/") {
        "audio"
    } else {
        return Err(format!(
            "Provider tool resource `{}` is neither image nor audio.",
            canonical.display()
        ));
    };
    Ok(json!({
        "kind": kind,
        "mime_type": descriptor.mime_type,
        "data_url": format!(
            "data:{};base64,{}",
            descriptor.mime_type,
            base64::engine::general_purpose::STANDARD.encode(bytes)
        )
    }))
}

/// Builds a trusted streaming-file descriptor after exact local validation.
/// 在精确本地校验后构建受信任的流式文件描述符。
///
/// # Parameters
/// 参数
/// `workspace_root`, `path`, MIME set, and byte limit define the selected provider contract.
/// `workspace_root`、`path`、MIME 集与字节上限定义所选供应商合同。
///
/// # Returns
/// 返回值
/// A host-only canonical path descriptor consumed by the provider upload boundary.
/// 返回由供应商上传边界消费的仅宿主可见规范路径描述符。
fn local_provider_file_descriptor(
    workspace_root: Option<&Path>,
    path: &str,
    allowed_mime_types: &[&str],
    maximum_bytes: u64,
) -> Result<Value, String> {
    let canonical = canonical_provider_resource_path(workspace_root, path)?;
    let descriptor = crate::attachment_assets::attachment_descriptor_from_path(&canonical)
        .filter(|descriptor| allowed_mime_types.contains(&descriptor.mime_type))
        .ok_or_else(|| {
            format!(
                "Provider tool resource `{}` has an unsupported media format.",
                canonical.display()
            )
        })?;
    let metadata = std::fs::metadata(&canonical).map_err(|error| {
        format!(
            "Provider tool resource `{}` metadata cannot be read: {error}",
            canonical.display()
        )
    })?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > maximum_bytes {
        return Err(format!(
            "Provider tool resource `{}` must be a non-empty regular file no larger than {} bytes.",
            canonical.display(),
            maximum_bytes
        ));
    }
    Ok(json!({
        "absolute_path": canonical.to_string_lossy(),
        "mime_type": descriptor.mime_type,
        "size_bytes": metadata.len()
    }))
}

/// Resolves and canonicalizes one provider-tool local resource path.
/// 解析并规范化一个供应商工具本地资源路径。
///
/// # Parameters
/// 参数
/// `workspace_root` resolves relative paths and `path` is the AI-supplied semantic path.
/// `workspace_root` 解析相对路径，`path` 是 AI 提供的语义路径。
///
/// # Returns
/// 返回值
/// The canonical existing filesystem path.
/// 返回已存在文件系统路径的规范形式。
fn canonical_provider_resource_path(
    workspace_root: Option<&Path>,
    path: &str,
) -> Result<PathBuf, String> {
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
    std::fs::canonicalize(&resolved).map_err(|error| {
        format!(
            "Provider tool resource `{}` cannot be resolved: {error}",
            resolved.display()
        )
    })
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
