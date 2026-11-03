//! Composer voice input: the speech-transcription target choice and one-shot recording transcription.
//! 输入区语音输入：语音转写目标选择与单次录音转写。

use base64::Engine;
use serde::{Deserialize, Serialize};
use vulcan_provider_adapters::{
    PersistedAiConfiguration, ProviderToolCapabilityKind, ProviderToolExecutionKind,
    ProviderToolNormalizedOutput, ProviderToolOutputKind, ProviderToolTarget,
    ProviderToolTargetSelection,
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

/// Voice-input settings shown on the settings page.
/// 设置页展示的语音输入设置。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VoiceInputSettingsSnapshot {
    /// Persisted target choice, kept even when its target is currently unavailable.
    /// 已持久化的目标选择；目标当前不可用时仍保留。
    selection: Option<ProviderToolTargetSelection>,
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
}

/// Saves or clears the voice-input target.
/// 保存或清除语音输入目标。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct VoiceInputSelectionSaveRequest {
    /// One exact target, or absence to clear the choice.
    /// 一个精确目标；缺失时清除选择。
    selection: Option<ProviderToolTargetSelection>,
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

/// Lists the voice-input candidates usable through the configured accounts.
/// 列出可通过已配置账号使用的语音输入候选。
fn voice_input_candidates(
    runtime: &vulcan_provider_adapters::PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
) -> Result<Vec<VoiceInputCandidate>, String> {
    Ok(runtime
        .available_provider_tool_targets(configuration)?
        .into_iter()
        .filter(|available| voice_input_target_supported(&available.target))
        .map(|available| VoiceInputCandidate {
            connection_id: available.connection_id,
            connection_display_name: available.connection_display_name,
            provider_id: available.target.provider_id,
            target_id: available.target.target_id,
            display_name: available.target.display_name,
        })
        .collect())
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
        candidates: voice_input_candidates(runtime.as_ref(), &configuration)?,
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
    let candidates = voice_input_candidates(runtime.as_ref(), &current_configuration)?;
    if let Some(selection) = request.selection.as_ref()
        && !candidates.iter().any(|candidate| {
            candidate.connection_id == selection.connection_id
                && candidate.target_id == selection.target_id
        })
    {
        return Err(format!(
            "Provider tool target `{}` is not a voice-input target available through connection `{}`.",
            selection.target_id, selection.connection_id
        ));
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
        candidates,
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
    let extension = voice_input_recording_extension(request.mime_type.as_str()).ok_or_else(|| {
        format!(
            "Voice input recording type `{}` is not supported.",
            request.mime_type
        )
    })?;
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
    let file_name = format!("recording.{extension}");
    tokio::fs::write(directory.path().join(&file_name), &bytes)
        .await
        .map_err(|error| format!("Failed to write the voice input recording: {error}"))?;
    let result = runtime
        .execute_provider_tool_target_for_owner_in_workspace(
            &configuration,
            ProviderToolCapabilityKind::SpeechTranscribe,
            &selection,
            serde_json::json!({ VOICE_INPUT_AUDIO_PATH_ARGUMENT: file_name }),
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
