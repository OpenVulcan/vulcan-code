//! Composer voice input: the speech-transcription target choice and one-shot recording transcription.
//! 输入区语音输入：语音转写目标选择与单次录音转写。

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use vulcan_provider_adapters::{
    AvailableProviderToolTarget, PersistedAiConfiguration, ProviderToolCapabilityKind,
    ProviderToolExecutionKind, ProviderToolNormalizedOutput, ProviderToolOutputKind,
    ProviderToolTarget, VoiceInputSelection, canonicalize_provider_tool_arguments,
};
use vulcan_service_api::{ServiceEventEmitter, VoiceInputSummary, VoiceInputTranscription};

use super::{
    public_ai_runtime_and_configuration,
    publication::mutate_public_ai_configuration_and_publish_without_session_model_reconciliation_in_transaction,
};
use crate::{SharedServiceBridgeState, begin_public_ai_configuration_transaction};

/// Longest single voice recording in seconds; the composer derives its countdown from this value.
/// 单次语音录音的最长秒数；输入区倒计时由该值派生。
const VOICE_INPUT_MAX_DURATION_SECONDS: u32 = 120;

/// The only argument a voice-input target may require: the local recording path.
/// 语音输入目标唯一允许的必填参数：本地录音路径。
const VOICE_INPUT_AUDIO_PATH_ARGUMENT: &str = "audio_path";

/// Recognition options supported by the microphone settings editor.
/// 麦克风设置编辑器支持的识别参数。
const VOICE_INPUT_PARAMETER_NAMES: [&str; 5] = [
    "language",
    "prompt",
    "enable_itn",
    "keep_dialect",
    "vocabulary_id",
];

/// Voice-input settings shown on the settings page.
/// 设置页展示的语音输入设置。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VoiceInputSettingsSnapshot {
    /// Persisted target choice, kept even when its target is currently unavailable.
    /// 已持久化的目标选择；目标当前不可用时仍保留。
    selection: Option<VoiceInputSelection>,
    /// Targets that can transcribe one composer recording through the configured accounts.
    /// 可通过已配置账号转写一次输入区录音的目标。
    candidates: Vec<VoiceInputCandidate>,
    /// Longest single recording in seconds.
    /// 单次录音的最长秒数。
    max_duration_seconds: u32,
}

/// One selectable voice-input target.
/// 一个可选的语音输入目标。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct VoiceInputCandidate {
    /// Exact credential connection.
    /// 精确凭据连接。
    connection_id: String,
    /// User-visible account name.
    /// 用户可见的账号名称。
    connection_display_name: String,
    /// Provider owning the target.
    /// 持有目标的供应商。
    provider_id: String,
    /// Exact provider tool target.
    /// 精确供应商工具目标。
    target_id: String,
    /// User-visible target name.
    /// 用户可见的目标名称。
    display_name: String,
    /// Only settings declared for this exact model, excluding recording transport fields.
    /// 仅包含精确模型声明的设置，排除录音传输字段。
    parameters: Vec<VoiceInputParameter>,
}

/// Describes a user-editable model parameter from the signed input schema.
/// 描述签名输入 Schema 中的一项用户可编辑模型参数。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct VoiceInputParameter {
    /// Exact argument name passed to the provider tool.
    /// 传给供应商工具的精确参数名称。
    name: String,
    /// Editor representation for the supported schema property.
    /// 已支持 Schema 属性的编辑控件表示。
    #[serde(flatten)]
    control: VoiceInputParameterControl,
}

/// Identifies the controls supported by the microphone settings editor.
/// 标识麦克风设置编辑器支持的控件。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum VoiceInputParameterControl {
    /// One preferred language chosen from the model's exact enum.
    /// 从模型精确枚举中选择的一项优先语言。
    Language {
        /// Language codes declared by the model.
        /// 模型声明的语言代码。
        choices: Vec<String>,
    },
    /// An optional string argument such as a recognition prompt or vocabulary ID.
    /// 可选字符串参数，例如识别提示词或热词词表 ID。
    Text,
    /// An optional boolean argument with an independent model default.
    /// 可选布尔参数，带有独立的模型默认值。
    Boolean,
}

/// Saves or clears the voice-input target.
/// 保存或清除语音输入目标。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct VoiceInputSelectionSaveRequest {
    /// One exact target, or absence to clear the choice.
    /// 一个精确目标；缺失时清除选择。
    selection: Option<VoiceInputSelection>,
}

/// One recorded composer clip to transcribe.
/// 一段待转写的输入区录音。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct VoiceInputTranscribeRequest {
    /// Base64-encoded recording bytes.
    /// Base64 编码的录音字节。
    data_base64: String,
    /// Recording MIME type reported by the browser recorder, parameters allowed.
    /// 浏览器录音器报告的录音 MIME 类型，允许带参数。
    mime_type: String,
}

/// Builds the composer voice-input state from one AI configuration.
/// 根据一份 AI 配置构建输入区语音输入状态。
///
/// # Parameters
/// 参数
/// `configuration` is the current persisted AI configuration.
/// `configuration` 是当前已持久化的 AI 配置。
///
/// # Returns
/// 返回值
/// Whether a target is chosen, with the recording duration limit.
/// 是否已选择目标及录音时长上限。
pub(crate) fn voice_input_summary(configuration: &PersistedAiConfiguration) -> VoiceInputSummary {
    VoiceInputSummary {
        configured: configuration.voice_input_selection.is_some(),
        max_duration_seconds: VOICE_INPUT_MAX_DURATION_SECONDS,
    }
}

/// Returns whether `target` can transcribe one composer recording in a single synchronous call.
/// 返回 `target` 能否以一次同步调用转写一段输入区录音。
///
/// A voice-input target returns text synchronously and requires only the recording path, so the
/// composer never has to know provider-specific arguments.
/// 语音输入目标同步返回文本，且只要求录音路径，因此输入区无需了解供应商专属参数。
fn voice_input_target_supported(target: &ProviderToolTarget) -> bool {
    let required = target
        .contract
        .input_schema
        .get("required")
        .and_then(serde_json::Value::as_array);
    target.contract.capability == ProviderToolCapabilityKind::SpeechTranscribe
        && target.contract.output.kind == ProviderToolOutputKind::Text
        && matches!(
            target.execution.execution_kind,
            ProviderToolExecutionKind::HttpJsonSync | ProviderToolExecutionKind::HttpMultipartSse
        )
        && required.is_some_and(|required| {
            required.len() == 1 && required[0] == VOICE_INPUT_AUDIO_PATH_ARGUMENT
        })
}

/// Projects only supported microphone options from the exact target input schema.
/// 只从精确目标的输入 Schema 投影受支持的麦克风选项。
///
/// `schema` is the signed tool input contract. Returns an empty list when it declares none.
/// `schema` 是签名工具输入契约。没有声明相应属性时返回空列表。
fn voice_input_parameters(schema: &Value) -> Vec<VoiceInputParameter> {
    // Missing properties describe a target without user-editable options.
    // 缺少属性表示目标没有用户可编辑选项。
    let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
        return Vec::new();
    };
    // Keep the language picker first and expose only model options with a known editor and meaning.
    // 语言选择器放在最前，只公开有明确控件和含义的模型选项。
    VOICE_INPUT_PARAMETER_NAMES
        .into_iter()
        .filter_map(|name| {
            // The model's signed property decides whether the option is available.
            // 是否提供该选项由模型签名属性决定。
            let property = properties.get(name)?;
            // Require an exact schema type before assigning a control.
            // 分配控件之前要求精确的 Schema 类型。
            let control = match (name, property.get("type").and_then(Value::as_str)) {
                ("language", Some("string")) => {
                    // Reject a missing or mixed enum instead of inventing language codes.
                    // 缺少枚举或枚举类型混用时不自行补造语言代码。
                    let choices = property
                        .get("enum")?
                        .as_array()?
                        .iter()
                        .map(|choice| choice.as_str().map(str::to_owned))
                        .collect::<Option<Vec<_>>>()?;
                    if choices.is_empty() {
                        return None;
                    }
                    VoiceInputParameterControl::Language { choices }
                }
                ("prompt" | "vocabulary_id", Some("string")) => VoiceInputParameterControl::Text,
                ("enable_itn" | "keep_dialect", Some("boolean")) => {
                    VoiceInputParameterControl::Boolean
                }
                _ => return None,
            };
            Some(VoiceInputParameter {
                name: name.to_owned(),
                control,
            })
        })
        .collect()
}

/// Builds the tool arguments with the recording path owned by the caller.
/// 使用由调用方持有的录音路径构建工具参数。
///
/// `parameters` are saved options and `file_name` is the host's recording file name.
/// `parameters` 是保存的选项，`file_name` 是宿主的录音文件名。
/// Returns a tool argument object or rejects a field outside the voice-input option contract.
/// 返回工具参数对象，或拒绝语音输入选项契约之外的字段。
fn voice_input_arguments(
    parameters: &Map<String, Value>,
    file_name: String,
) -> Result<Map<String, Value>, String> {
    // These options affect recognition only. The host retains control over the audio source and format.
    // 这些选项只影响识别。音频来源和格式继续由宿主控制。
    if parameters
        .keys()
        .any(|name| !VOICE_INPUT_PARAMETER_NAMES.contains(&name.as_str()))
    {
        return Err(String::from(
            "Voice input settings contain an unsupported parameter.",
        ));
    }
    // Combine the persisted settings with the exact file written for this call.
    // 将已保存设置与为本次调用写入的精确文件合并。
    let mut arguments = parameters.clone();
    arguments.insert(
        VOICE_INPUT_AUDIO_PATH_ARGUMENT.to_owned(),
        Value::String(file_name),
    );
    Ok(arguments)
}

/// Resolves the exact selected account and target from the current available inventory.
/// 从当前可用清单解析所选的精确账号与目标。
///
/// Returns the selected target or an explicit error if it is no longer eligible.
/// 返回所选目标，目标不再符合条件时返回明确错误。
fn selected_voice_input_target<'a>(
    targets: &'a [AvailableProviderToolTarget],
    selection: &VoiceInputSelection,
) -> Result<&'a ProviderToolTarget, String> {
    targets.iter()
        .find(|available| {
            available.connection_id == selection.connection_id
                && available.target.target_id == selection.target_id
                && voice_input_target_supported(&available.target)
        })
        .map(|available| &available.target)
        .ok_or_else(|| format!(
            "Provider tool target `{}` is not a voice-input target available through connection `{}`.",
            selection.target_id, selection.connection_id
        ))
}

/// Projects usable transcription targets and their model-specific settings for the editor.
/// 将可用转写目标及其模型专属设置投影给编辑器。
///
/// `targets` are the current account-scoped provider tool inventory.
/// `targets` 是当前按账号划分的供应商工具清单。
/// Returns the targets compatible with one microphone recording.
/// 返回与单次麦克风录音兼容的目标。
fn voice_input_candidates(targets: &[AvailableProviderToolTarget]) -> Vec<VoiceInputCandidate> {
    targets
        .iter()
        .filter(|available| voice_input_target_supported(&available.target))
        .map(|available| VoiceInputCandidate {
            connection_id: available.connection_id.clone(),
            connection_display_name: available.connection_display_name.clone(),
            provider_id: available.target.provider_id.clone(),
            target_id: available.target.target_id.clone(),
            display_name: available.target.display_name.clone(),
            parameters: voice_input_parameters(&available.target.contract.input_schema),
        })
        .collect()
}

/// Returns the voice-input settings.
/// 返回语音输入设置。
///
/// # Parameters
/// 参数
/// `state` owns the current runtime and configuration.
/// `state` 持有当前运行时与配置。
///
/// # Returns
/// 返回值
/// The saved choice, the usable candidates and the duration limit, or an explicit failure.
/// 已保存的选择、可用候选与时长上限，或明确失败。
pub(crate) fn service_voice_input_settings(
    state: SharedServiceBridgeState,
) -> Result<VoiceInputSettingsSnapshot, String> {
    let (runtime, configuration) = public_ai_runtime_and_configuration(&state)?;
    Ok(VoiceInputSettingsSnapshot {
        selection: configuration.voice_input_selection.clone(),
        candidates: voice_input_candidates(
            &runtime.available_provider_tool_targets(&configuration)?,
        ),
        max_duration_seconds: VOICE_INPUT_MAX_DURATION_SECONDS,
    })
}

/// Saves or clears the voice-input target and republishes the workbench.
/// 保存或清除语音输入目标并重新发布工作台。
///
/// # Parameters
/// 参数
/// `app` publishes the workbench; `state` owns runtime/configuration; `request` is the user's choice.
/// `app` 发布工作台；`state` 持有运行时与配置；`request` 是用户的选择。
///
/// # Returns
/// 返回值
/// The settings after saving, or an explicit failure when the target is not a usable candidate.
/// 保存后的设置；目标不是可用候选时返回明确失败。
pub(crate) async fn service_save_voice_input_selection(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: VoiceInputSelectionSaveRequest,
) -> Result<VoiceInputSettingsSnapshot, String> {
    // transaction serializes target validation through Workbench publication.
    // transaction 把目标校验串行化到工作台发布完成。
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    // Validate against the exact target before publishing or writing the selection.
    // 发布或写入选择之前，先根据精确目标校验。
    let targets = runtime.available_provider_tool_targets(&current_configuration)?;
    if let Some(selection) = request.selection.as_ref() {
        // Resolve the exact account and target pair selected in the editor.
        // 解析编辑器所选的精确账号与目标组合。
        let target = selected_voice_input_target(&targets, selection)?;
        // Match the editor's allowed fields even when a schema accepts other optional inputs.
        // 即使 Schema 接受其他可选输入，也按编辑器允许的字段校验。
        let allowed = voice_input_parameters(&target.contract.input_schema);
        if selection
            .parameters
            .keys()
            .any(|key| !allowed.iter().any(|parameter| parameter.name == *key))
        {
            return Err(String::from(
                "Voice input settings contain a parameter not declared for this target.",
            ));
        }
        // The path only satisfies the required schema field. Saving never opens a recording or calls the provider.
        // 路径只用于满足 Schema 的必填字段。保存时不会读取录音或调用供应商。
        let arguments =
            voice_input_arguments(&selection.parameters, String::from("recording.wav"))?;
        canonicalize_provider_tool_arguments(target, Value::Object(arguments))?;
    }
    let (configuration, ()) =
        mutate_public_ai_configuration_and_publish_without_session_model_reconciliation_in_transaction(
            &app,
            &state,
            runtime.as_ref(),
            &current_configuration,
            "Failed to save the voice input target",
            |configuration| {
                configuration.voice_input_selection = request.selection.clone();
                Ok(())
            },
            |_| Ok(()),
        )?;
    drop(transaction);
    Ok(VoiceInputSettingsSnapshot {
        selection: configuration.voice_input_selection.clone(),
        candidates: voice_input_candidates(&targets),
        max_duration_seconds: VOICE_INPUT_MAX_DURATION_SECONDS,
    })
}

/// Maps a browser recording MIME type to the file extension the upload binding resolves.
/// 将浏览器录音 MIME 类型映射为上传绑定解析所用的文件扩展名。
fn voice_input_recording_extension(mime_type: &str) -> Option<&'static str> {
    let essence = mime_type.split(';').next().unwrap_or_default().trim();
    match essence.to_ascii_lowercase().as_str() {
        "audio/webm" => Some("webm"),
        "audio/ogg" => Some("ogg"),
        "audio/mp4" => Some("m4a"),
        "audio/wav" | "audio/x-wav" => Some("wav"),
        "audio/mpeg" => Some("mp3"),
        _ => None,
    }
}

/// Transcribes one composer recording with the chosen voice-input target.
/// 使用所选语音输入目标转写一段输入区录音。
///
/// The recording lives only in a private temporary directory that is removed when this call
/// returns; it is never kept.
/// 录音只存在于本调用返回时即删除的私有临时目录中，不做保留。
///
/// # Parameters
/// 参数
/// `state` owns runtime/configuration; `request` carries the recording bytes and MIME type.
/// `state` 持有运行时与配置；`request` 携带录音字节与 MIME 类型。
///
/// # Returns
/// 返回值
/// The transcript, or an explicit configuration, upload or provider failure.
/// 转写文本，或明确的配置、上传与供应商失败。
pub(crate) async fn service_transcribe_voice_input(
    state: SharedServiceBridgeState,
    request: VoiceInputTranscribeRequest,
) -> Result<VoiceInputTranscription, String> {
    let (runtime, configuration) = public_ai_runtime_and_configuration(&state)?;
    let selection = configuration
        .voice_input_selection
        .clone()
        .ok_or_else(|| String::from("No voice input target is selected."))?;
    let extension =
        voice_input_recording_extension(request.mime_type.as_str()).ok_or_else(|| {
            format!(
                "Voice input recording type `{}` is not supported.",
                request.mime_type
            )
        })?;
    // Freeze the selected target's declared input contract for assembling this recording request.
    // 冻结本次录音请求所选目标声明的输入契约。
    let targets = runtime.available_provider_tool_targets(&configuration)?;
    let target = selected_voice_input_target(&targets, &selection)?;
    let file_name = format!("recording.{extension}");
    let mut arguments = voice_input_arguments(&selection.parameters, file_name.clone())?;
    // Targets with an explicit format field receive the actual browser recording format.
    // 显式声明格式字段的目标接收真实的浏览器录音格式。
    if target
        .contract
        .input_schema
        .pointer("/properties/format/type")
        .and_then(Value::as_str)
        == Some("string")
    {
        arguments.insert("format".to_owned(), Value::String(extension.to_owned()));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(request.data_base64.as_bytes())
        .map_err(|error| format!("Voice input recording is not valid base64: {error}"))?;
    if bytes.is_empty() {
        return Err(String::from("Voice input recording is empty."));
    }
    let directory = tempfile::Builder::new()
        .prefix("vulcan-voice-input-")
        .tempdir()
        .map_err(|error| format!("Failed to create the voice input directory: {error}"))?;
    tokio::fs::write(directory.path().join(&file_name), &bytes)
        .await
        .map_err(|error| format!("Failed to write the voice input recording: {error}"))?;
    let result = runtime
        .execute_provider_tool_target_for_owner_in_workspace(
            &configuration,
            ProviderToolCapabilityKind::SpeechTranscribe,
            &selection.target(),
            Value::Object(arguments),
            None,
            Some(directory.path()),
        )
        .await?;
    match result.output {
        ProviderToolNormalizedOutput::Text(text) => Ok(VoiceInputTranscription {
            text: text.trim().to_string(),
        }),
        _ => Err(String::from(
            "Voice input target returned a non-text result.",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Checks that the settings projection exposes signed recognition options and hides transport fields.
    /// 检查设置投影公开签名识别选项，并隐藏传输字段。
    #[test]
    fn projects_only_declared_voice_recognition_options() {
        // A synchronous ASR contract can declare both user options and recording transport fields.
        // 同步 ASR 契约可同时声明用户选项和录音传输字段。
        let schema = serde_json::json!({
            "properties": {
                "audio_path": {"type": "string"},
                "format": {"type": "string"},
                "language": {"type": "string", "enum": ["it", "zh"]},
                "enable_itn": {"type": "boolean"},
                "prompt": {"type": "string"},
                "vocabulary_id": {"type": "integer"}
            }
        });
        assert_eq!(
            serde_json::to_value(voice_input_parameters(&schema)).unwrap(),
            serde_json::json!([
                {"name": "language", "kind": "language", "choices": ["it", "zh"]},
                {"name": "prompt", "kind": "text"},
                {"name": "enable_itn", "kind": "boolean"}
            ])
        );
    }

    /// Checks backwards-readable selections and exact option forwarding to the transcription tool.
    /// 检查旧选择可读，并将识别选项原样传给转写工具。
    #[test]
    fn forwards_explicit_voice_options_without_changing_legacy_selections() {
        // Selections saved before recognition options were introduced have only IDs.
        // 引入识别选项之前保存的选择只有 ID。
        let legacy = serde_json::json!({"connection_id": "account", "target_id": "asr"});
        let mut selection: VoiceInputSelection = serde_json::from_value(legacy.clone()).unwrap();
        assert_eq!(serde_json::to_value(&selection).unwrap(), legacy);
        selection
            .parameters
            .insert("language".to_owned(), Value::String("it".to_owned()));
        selection
            .parameters
            .insert("enable_itn".to_owned(), Value::Bool(false));
        assert_eq!(
            Value::Object(
                voice_input_arguments(&selection.parameters, "recording.wav".to_owned()).unwrap()
            ),
            serde_json::json!({"audio_path": "recording.wav", "language": "it", "enable_itn": false})
        );
        selection
            .parameters
            .insert("format".to_owned(), Value::String("mp3".to_owned()));
        assert!(voice_input_arguments(&selection.parameters, "recording.wav".to_owned()).is_err());
    }

    #[test]
    fn recording_mime_types_map_to_binding_extensions() {
        assert_eq!(
            voice_input_recording_extension("audio/webm;codecs=opus"),
            Some("webm")
        );
        assert_eq!(voice_input_recording_extension("Audio/MP4"), Some("m4a"));
        assert_eq!(voice_input_recording_extension("video/webm"), None);
        assert_eq!(voice_input_recording_extension(""), None);
    }
}
