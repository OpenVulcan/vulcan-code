//! Composer voice input: the speech-transcription target choice and one-shot recording transcription.
//! 输入区语音输入：语音转写目标选择与单次录音转写。

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use vulcan_provider_adapters::{
    AvailableProviderToolTarget, CredentialExecutionSecret, OPENVULCAN_VOICE_DURATION_SECONDS,
    OPENVULCAN_VOICE_MAX_AUDIO_BYTES, OpenVulcanVoiceCatalog, OpenVulcanVoiceError,
    OpenVulcanVoiceInputSelection, OpenVulcanVoiceModel, PersistedAiConfiguration,
    ProviderToolCapabilityKind, ProviderToolExecutionKind, ProviderToolNormalizedOutput,
    ProviderToolOutputKind, ProviderToolTarget, PublicAiRuntime, VoiceInputRoute,
    VoiceInputSelection, canonicalize_provider_tool_arguments, request_openvulcan_voice_catalog,
    request_openvulcan_voice_transcription,
};
use vulcan_service_api::{
    ServiceEventEmitter, VoiceInputSummary, VoiceInputTranscription, WebuiLocalizedMessage,
    WebuiServiceRpcError,
};

use super::{
    public_ai_runtime_and_configuration,
    publication::mutate_public_ai_configuration_and_publish_without_session_model_reconciliation_in_transaction,
};
use crate::{SharedServiceBridgeState, begin_public_ai_configuration_transaction};

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
    selection: Option<VoiceInputRoute>,
    /// Whether the user explicitly switched voice input off.
    /// 用户是否显式关闭语音输入。
    disabled: bool,
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
    selection: Option<VoiceInputRoute>,
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
/// `runtime` resolves the native account from the current `configuration`.
/// `runtime` 从当前 `configuration` 解析原生账号。
///
/// # Returns
/// 返回值
/// The direct-route state, eligible cloud account and recording duration limit.
/// 直连路由状态、可用云端账号及录音时长上限。
pub(crate) fn voice_input_summary(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
) -> VoiceInputSummary {
    // Only an eligible cloud preference needs native account discovery in the composer.
    // 仅当允许使用云端时，输入区才需要查询原生账号。
    let cloud_account_id = cloud_voice_preference(configuration).and_then(|preference| {
        super::openvulcan_account::account_connection(runtime, configuration)
            .ok()
            .flatten()
            .filter(|account| {
                preference.is_none_or(|selection| selection.connection_id == account.id)
            })
            .map(|account| account.id)
    });
    VoiceInputSummary {
        configured: !configuration.voice_input_disabled
            && matches!(
                configuration.voice_input_selection,
                Some(VoiceInputRoute::Provider(_))
            ),
        cloud_account_id,
        max_duration_seconds: OPENVULCAN_VOICE_DURATION_SECONDS,
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

/// Cloud availability resolved independently from local provider settings.
/// 独立于本地供应商设置解析的云端可用状态。
#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(crate) enum CloudVoiceInputCatalog {
    /// No native credential is available for the account.
    /// 账号无可用原生凭据。
    SignInRequired,
    /// The authenticated site has explicitly disabled free voice input.
    /// 已认证站点已明确停用免费语音输入。
    Disabled,
    /// Transport or directory failure prevented a reliable result.
    /// 传输或目录故障导致无法可靠取得结果。
    Unavailable,
    /// The active native account and its published directory.
    /// 当前原生账号及其发布目录。
    Available {
        /// Exact native connection ID for subsequent saves.
        /// 后续保存使用的精确原生连接标识。
        connection_id: String,
        /// Site-recommended model ID.
        /// 站点推荐的模型标识。
        default_model_id: String,
        /// Published microphone-only models.
        /// 已发布的麦克风专用模型。
        models: Vec<OpenVulcanVoiceModel>,
        /// Effective cloud route, resolved from current preferences and publications.
        /// 根据当前偏好和发布目录解析出的生效云端路由。
        selection: Option<OpenVulcanVoiceInputSelection>,
    },
}

/// Returns the saved cloud preference or an empty preference for an unconfigured microphone.
/// 返回已保存的云端偏好；麦克风未配置时返回空偏好。
/// An outer `None` means the configuration explicitly excludes a cloud route.
/// 外层 `None` 表示当前配置明确排除了云端路由。
fn cloud_voice_preference(
    configuration: &PersistedAiConfiguration,
) -> Option<Option<&OpenVulcanVoiceInputSelection>> {
    if configuration.voice_input_disabled {
        return None;
    }
    match configuration.voice_input_selection.as_ref() {
        Some(VoiceInputRoute::Provider(_)) => None,
        Some(VoiceInputRoute::Cloud(route)) => Some(Some(&route.openvulcan_cloud)),
        None => Some(None),
    }
}

/// Resolves a microphone route for the authenticated `connection_id` and its validated `catalog`.
/// 根据已认证的 `connection_id` 与经校验的 `catalog` 解析麦克风路由。
/// Returns the existing choice while published, or the site default when it is gone or unset.
/// 已选模型仍在发布时继续使用；下架或未选择时返回站点默认模型。
fn resolve_cloud_voice_selection(
    configuration: &PersistedAiConfiguration,
    connection_id: &str,
    catalog: &OpenVulcanVoiceCatalog,
) -> Option<OpenVulcanVoiceInputSelection> {
    if !catalog.available {
        return None;
    }
    let preference = cloud_voice_preference(configuration)?;
    if let Some(saved) = preference {
        if saved.connection_id != connection_id {
            return None;
        }
        if catalog
            .models
            .iter()
            .any(|model| model.id == saved.model_id)
        {
            return Some(saved.clone());
        }
    }
    // Carry the language hint to the new default only if that model declares it.
    // 仅当新的默认模型声明支持时，才继承原优先语言。
    let default = catalog
        .models
        .iter()
        .find(|model| model.id == catalog.default_model_id)?;
    let language = preference
        .and_then(|saved| saved.language.as_ref())
        .filter(|language| default.supported_languages.contains(language))
        .cloned();
    Some(OpenVulcanVoiceInputSelection {
        connection_id: connection_id.to_owned(),
        model_id: default.id.clone(),
        language,
    })
}

/// Returns the verified regional account identity, or a localized reauthorization error.
/// 返回校验过的区域账号身份，或返回本地化的重新授权错误。
async fn voice_credential(
    state: &SharedServiceBridgeState,
) -> Result<(String, CredentialExecutionSecret), OpenVulcanVoiceError> {
    super::openvulcan_account::signed_in_credential(state)
        .await
        .map_err(|_| OpenVulcanVoiceError::SignIn)?
        .ok_or(OpenVulcanVoiceError::SignIn)
}

/// Confirms that the native connection still matches the frozen catalog or recording owner.
/// 确认原生连接仍与已冻结的目录或录音所有者一致。
fn check_voice_account(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    expected_id: &str,
) -> Result<(), OpenVulcanVoiceError> {
    let connection = super::openvulcan_account::account_connection(runtime, configuration)
        .map_err(|_| OpenVulcanVoiceError::Unavailable)?;
    if connection.is_some_and(|account| account.id == expected_id) {
        return Ok(());
    }
    Err(OpenVulcanVoiceError::SignIn)
}

/// Reports cloud status without delaying or replacing direct-provider settings.
/// 报告云端状态，独立于直连供应商设置的读取。
pub(crate) async fn service_voice_input_cloud_catalog(
    state: SharedServiceBridgeState,
) -> Result<CloudVoiceInputCatalog, String> {
    let (connection_id, credential) = match voice_credential(&state).await {
        Ok(credential) => credential,
        Err(_) => return Ok(CloudVoiceInputCatalog::SignInRequired),
    };
    let answer = request_openvulcan_voice_catalog(&credential).await;
    let (runtime, configuration) = public_ai_runtime_and_configuration(&state)?;
    if check_voice_account(&runtime, &configuration, &connection_id).is_err() {
        return Ok(CloudVoiceInputCatalog::SignInRequired);
    }
    Ok(match answer {
        Ok(catalog) if catalog.available => CloudVoiceInputCatalog::Available {
            selection: resolve_cloud_voice_selection(&configuration, &connection_id, &catalog),
            connection_id,
            default_model_id: catalog.default_model_id,
            models: catalog.models,
        },
        Ok(_) => CloudVoiceInputCatalog::Disabled,
        Err(OpenVulcanVoiceError::SignIn) => CloudVoiceInputCatalog::SignInRequired,
        Err(_) => CloudVoiceInputCatalog::Unavailable,
    })
}

/// Converts typed cloud errors to the WebUI catalog without exposing upstream data.
/// 将类型化云端错误转换为 WebUI 目录消息，不暴露上游数据。
fn cloud_voice_rpc_error(error: OpenVulcanVoiceError) -> WebuiServiceRpcError {
    use OpenVulcanVoiceError as Error;
    match error {
        Error::SignIn => WebuiLocalizedMessage::new("settings.voiceInput.cloudSignIn"),
        Error::Unavailable => WebuiLocalizedMessage::new("settings.voiceInput.cloudUnavailable"),
        Error::ModelUnavailable => WebuiLocalizedMessage::new("settings.voiceInput.unavailable"),
        Error::UnsupportedLanguage => {
            WebuiLocalizedMessage::new("settings.voiceInput.cloudLanguageUnavailable")
        }
        Error::InvalidAudio => WebuiLocalizedMessage::new("settings.voiceInput.cloudInvalidAudio"),
        Error::RetryAfter(seconds) => {
            WebuiLocalizedMessage::new("settings.voiceInput.cloudRetryAfter")
                .with("seconds", seconds.to_string())
        }
    }
    .into()
}

/// Validates an exact published model and language before saving the user's cloud choice.
/// 保存用户的云端选择前，校验精确的发布模型及语言。
async fn validate_cloud_voice_selection(
    state: &SharedServiceBridgeState,
    selection: &OpenVulcanVoiceInputSelection,
) -> Result<(), OpenVulcanVoiceError> {
    let (connection_id, credential) = voice_credential(state).await?;
    if connection_id != selection.connection_id {
        return Err(OpenVulcanVoiceError::SignIn);
    }
    let catalog = request_openvulcan_voice_catalog(&credential).await?;
    let model = catalog
        .models
        .iter()
        .find(|model| model.id == selection.model_id)
        .ok_or(OpenVulcanVoiceError::ModelUnavailable)?;
    if let Some(language) = selection.language.as_ref()
        && !model.supported_languages.contains(language)
    {
        return Err(OpenVulcanVoiceError::UnsupportedLanguage);
    }
    Ok(())
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
        disabled: configuration.voice_input_disabled,
        candidates: voice_input_candidates(
            &runtime.available_provider_tool_targets(&configuration)?,
        ),
        max_duration_seconds: OPENVULCAN_VOICE_DURATION_SECONDS,
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
) -> Result<VoiceInputSettingsSnapshot, WebuiServiceRpcError> {
    // Remote catalog reads happen before the local configuration transaction.
    // 先读取远端目录，再进入本地配置事务。
    if let Some(VoiceInputRoute::Cloud(route)) = request.selection.as_ref() {
        validate_cloud_voice_selection(&state, &route.openvulcan_cloud)
            .await
            .map_err(cloud_voice_rpc_error)?;
    }
    // transaction serializes target validation through Workbench publication.
    // transaction 把目标校验串行化到工作台发布完成。
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    // Validate against the exact target before publishing or writing the selection.
    // 发布或写入选择之前，先根据精确目标校验。
    let targets = runtime.available_provider_tool_targets(&current_configuration)?;
    if let Some(VoiceInputRoute::Cloud(route)) = request.selection.as_ref() {
        check_voice_account(
            &runtime,
            &current_configuration,
            &route.openvulcan_cloud.connection_id,
        )
        .map_err(cloud_voice_rpc_error)?;
    }
    if let Some(VoiceInputRoute::Provider(selection)) = request.selection.as_ref() {
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
            )
            .into());
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
                configuration.voice_input_disabled = request.selection.is_none();
                Ok(())
            },
            |_| Ok(()),
        )?;
    drop(transaction);
    Ok(VoiceInputSettingsSnapshot {
        selection: configuration.voice_input_selection.clone(),
        disabled: configuration.voice_input_disabled,
        candidates: voice_input_candidates(&targets),
        max_duration_seconds: OPENVULCAN_VOICE_DURATION_SECONDS,
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

/// Dispatches one composer recording through the exact selected microphone route.
/// 通过精确选择的麦克风路由转发一段输入区录音。
pub(crate) async fn service_transcribe_voice_input(
    state: SharedServiceBridgeState,
    request: VoiceInputTranscribeRequest,
) -> Result<VoiceInputTranscription, WebuiServiceRpcError> {
    let (runtime, configuration) = public_ai_runtime_and_configuration(&state)?;
    if configuration.voice_input_disabled {
        return Err(String::from("Voice input is disabled.").into());
    }
    match configuration.voice_input_selection.as_ref() {
        Some(VoiceInputRoute::Cloud(_)) | None => {
            let account = super::openvulcan_account::account_connection(&runtime, &configuration)?
                .ok_or_else(|| cloud_voice_rpc_error(OpenVulcanVoiceError::SignIn))?;
            transcribe_cloud_voice(&state, &account.id, &configuration, request)
                .await
                .map_err(cloud_voice_rpc_error)
        }
        Some(VoiceInputRoute::Provider(selection)) => {
            transcribe_provider_voice(&runtime, &configuration, selection, request)
                .await
                .map_err(Into::into)
        }
    }
}

/// Resolves the current model for `account_id` and submits the WAV in `request` once.
/// 解析 `account_id` 当前生效的模型，并仅提交一次 `request` 中的 WAV。
/// `state` verifies that the account and the initial `configuration` preference still match.
/// `state` 验证账号与初始 `configuration` 偏好均未改变。
async fn transcribe_cloud_voice(
    state: &SharedServiceBridgeState,
    account_id: &str,
    initial_configuration: &PersistedAiConfiguration,
    request: VoiceInputTranscribeRequest,
) -> Result<VoiceInputTranscription, OpenVulcanVoiceError> {
    // Reject oversized audio before decoding the browser's base64 envelope.
    // 在解码浏览器的 base64 封套前拒绝超限录音。
    if request.mime_type != "audio/wav"
        || request.data_base64.len() > OPENVULCAN_VOICE_MAX_AUDIO_BYTES.div_ceil(3) * 4
    {
        return Err(OpenVulcanVoiceError::InvalidAudio);
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(request.data_base64.as_bytes())
        .map_err(|_| OpenVulcanVoiceError::InvalidAudio)?;
    let (connection_id, credential) = voice_credential(state).await?;
    if connection_id != account_id {
        return Err(OpenVulcanVoiceError::SignIn);
    }
    // Refresh publication before uploading audio so a retired ID never requires an upload retry.
    // 上传前刷新发布目录，避免因模型下架而需要重传录音。
    let catalog = request_openvulcan_voice_catalog(&credential).await?;
    let (runtime, configuration) = public_ai_runtime_and_configuration(state)
        .map_err(|_| OpenVulcanVoiceError::Unavailable)?;
    check_voice_account(&runtime, &configuration, &connection_id)?;
    if cloud_voice_preference(&configuration) != cloud_voice_preference(initial_configuration) {
        return Err(OpenVulcanVoiceError::ModelUnavailable);
    }
    if !catalog.available {
        return Err(OpenVulcanVoiceError::Unavailable);
    }
    let selection = resolve_cloud_voice_selection(&configuration, &connection_id, &catalog)
        .ok_or(OpenVulcanVoiceError::ModelUnavailable)?;
    let text = request_openvulcan_voice_transcription(
        &credential,
        &selection.model_id,
        selection.language.as_deref(),
        bytes,
    )
    .await?;
    Ok(VoiceInputTranscription { text })
}

/// Transcribes one composer recording with the chosen direct provider.
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
async fn transcribe_provider_voice(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    selection: &VoiceInputSelection,
    request: VoiceInputTranscribeRequest,
) -> Result<VoiceInputTranscription, String> {
    let extension =
        voice_input_recording_extension(request.mime_type.as_str()).ok_or_else(|| {
            format!(
                "Voice input recording type `{}` is not supported.",
                request.mime_type
            )
        })?;
    // Freeze the selected target's declared input contract for assembling this recording request.
    // 冻结本次录音请求所选目标声明的输入契约。
    let targets = runtime.available_provider_tool_targets(configuration)?;
    let target = selected_voice_input_target(&targets, selection)?;
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
            configuration,
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

    /// Exercises default selection, replacement, account binding and explicit opt-out.
    /// 覆盖默认选择、模型换代、账号绑定及显式关闭。
    #[test]
    fn voice_input_resolves_the_active_regional_default() {
        // Use a current publication alongside a still-active optional model.
        // 使用当前默认发布及另一个仍可选模型。
        let mut catalog: OpenVulcanVoiceCatalog = serde_json::from_value(serde_json::json!({
            "contract_version": 1, "available": true, "max_duration_seconds": OPENVULCAN_VOICE_DURATION_SECONDS,
            "default_model_id": "new-asr", "models": [
                {"id":"new-asr", "display_name":"Default ASR", "category":"speech_transcribe", "supported_languages":["it"]},
                {"id":"other-asr", "display_name":"Alternate ASR", "category":"speech_transcribe", "supported_languages":["it", "zh"]}
            ]
        })).unwrap();
        // Round-trip the explicit off setting so it remains distinct from a missing legacy field.
        // 往返读写显式关闭，使其保持与旧配置缺省字段的区别。
        let mut configuration = PersistedAiConfiguration::default();
        let legacy = serde_json::to_value(&configuration).unwrap();
        assert!(legacy.get("voice_input_disabled").is_none());
        configuration = serde_json::from_value(legacy).unwrap();
        assert_eq!(
            resolve_cloud_voice_selection(&configuration, "native", &catalog)
                .unwrap()
                .model_id,
            "new-asr",
        );
        configuration.voice_input_disabled = true;
        configuration =
            serde_json::from_value(serde_json::to_value(&configuration).unwrap()).unwrap();
        assert!(resolve_cloud_voice_selection(&configuration, "native", &catalog).is_none());
        configuration.voice_input_disabled = false;
        configuration.voice_input_selection =
            Some(VoiceInputRoute::Provider(VoiceInputSelection {
                connection_id: "direct".into(),
                target_id: "asr".into(),
                parameters: Map::new(),
            }));
        assert!(resolve_cloud_voice_selection(&configuration, "native", &catalog).is_none());

        // Freeze a saved cloud preference and remove it from the published directory.
        // 固定一个已保存的云端偏好，再将其从发布目录移除。
        let preference = OpenVulcanVoiceInputSelection {
            connection_id: "native".into(),
            model_id: "other-asr".into(),
            language: Some("it".into()),
        };
        configuration.voice_input_selection = Some(VoiceInputRoute::Cloud(
            vulcan_provider_adapters::OpenVulcanVoiceInputRoute {
                openvulcan_cloud: preference.clone(),
            },
        ));
        assert_eq!(
            resolve_cloud_voice_selection(&configuration, "native", &catalog),
            Some(preference)
        );
        assert!(resolve_cloud_voice_selection(&configuration, "other-account", &catalog).is_none());
        catalog
            .models
            .retain(|model| model.id == catalog.default_model_id);
        // The replacement keeps a supported language and drops it if the new model removes support.
        // 替代模型支持原语言时保留提示，取消支持后移除提示。
        let replacement =
            resolve_cloud_voice_selection(&configuration, "native", &catalog).unwrap();
        assert_eq!(replacement.model_id, "new-asr");
        assert_eq!(replacement.language.as_deref(), Some("it"));
        catalog
            .models
            .iter_mut()
            .for_each(|model| model.supported_languages.clear());
        assert_eq!(
            resolve_cloud_voice_selection(&configuration, "native", &catalog)
                .unwrap()
                .language,
            None
        );
        catalog.available = false;
        assert!(resolve_cloud_voice_selection(&configuration, "native", &catalog).is_none());
    }

    /// Checks that the cloud choice is disjoint from a provider tool and omits an unset language.
    /// 检查云端选择与供应商工具互斥，并省略未设置的语言。
    #[test]
    fn voice_input_cloud_route_omits_default_language() {
        let cloud = serde_json::json!({"openvulcan_cloud":{"connection_id":"native-account","model_id":"qwen-asr"}});
        let route: VoiceInputRoute = serde_json::from_value(cloud.clone()).unwrap();
        assert_eq!(route.connection_id(), "native-account");
        assert_eq!(serde_json::to_value(route).unwrap(), cloud);
        let mixed = serde_json::json!({"connection_id":"provider","target_id":"tool","openvulcan_cloud":cloud["openvulcan_cloud"]});
        assert!(serde_json::from_value::<VoiceInputRoute>(mixed).is_err());
    }

    /// Checks that the settings projection exposes signed recognition options and hides transport fields.
    /// 检查设置投影公开签名识别选项，并隐藏传输字段。
    #[test]
    fn voice_input_projects_only_declared_recognition_options() {
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
    fn voice_input_forwards_explicit_options_without_changing_legacy_selections() {
        // Selections saved before recognition options were introduced have only IDs.
        // 引入识别选项之前保存的选择只有 ID。
        let legacy = serde_json::json!({"connection_id": "account", "target_id": "asr"});
        let decoded: VoiceInputRoute = serde_json::from_value(legacy.clone()).unwrap();
        let VoiceInputRoute::Provider(mut selection) = decoded else {
            panic!("Legacy microphone selections must keep their direct provider route");
        };
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
