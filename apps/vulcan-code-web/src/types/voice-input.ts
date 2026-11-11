// VoiceInputSummary is the composer voice-input state published with the workbench snapshot.
// VoiceInputSummary 是随工作台快照发布的输入区语音输入状态。
export interface VoiceInputSummary {
  // Whether a direct-provider route is configured.
  // 是否已配置直连供应商路由。
  configured: boolean;
  // Eligible native account, pending a cloud catalog lookup.
  // 可使用云端的原生账号，需查询云端目录确认。
  cloudAccountId?: string;
  // Longest single recording in seconds; recording stops at this limit.
  // 单次录音的最长秒数；到达上限时停止录音。
  maxDurationSeconds: number;
}

// VoiceInputTargetSelection identifies one exact speech-transcription target on one account.
// VoiceInputTargetSelection 标识某个账号上的一个精确语音转写目标。
export interface VoiceInputTargetSelection {
  connection_id: string;
  target_id: string;
  // Optional recognition settings supported by the exact target.
  // 精确目标支持的可选识别设置。
  parameters?: VoiceInputParameters;
}

// Optional recognition settings keyed by the signed tool argument name.
// 以签名工具参数名称为键的可选识别设置。
export type VoiceInputParameters = Record<string, string | boolean>;

// A supported argument and the editor control it accepts.
// 受支持的参数及其对应的编辑控件。
export type VoiceInputParameter =
  | { name: "language"; kind: "language"; choices: string[] }
  | { name: "prompt" | "vocabulary_id"; kind: "text" }
  | { name: "enable_itn" | "keep_dialect"; kind: "boolean" };

// VoiceInputCandidate is one target that can transcribe a composer recording.
// VoiceInputCandidate 是一个可以转写输入区录音的目标。
export interface VoiceInputCandidate {
  connectionId: string;
  connectionDisplayName: string;
  providerId: string;
  targetId: string;
  displayName: string;
  // Model-specific options available to the microphone.
  // 麦克风可配置的模型专属选项。
  parameters: VoiceInputParameter[];
}

// OpenVulcanVoiceInputSelection is one microphone-only model bound to its native account.
// OpenVulcanVoiceInputSelection 是绑定到原生账号的一个麦克风专用模型。
export interface OpenVulcanVoiceInputSelection {
  connection_id: string;
  model_id: string;
  language?: string;
}

// VoiceInputSelection is the exact active microphone route.
// VoiceInputSelection 是当前精确的麦克风路由。
export type VoiceInputSelection = VoiceInputTargetSelection | { openvulcan_cloud: OpenVulcanVoiceInputSelection };

// OpenVulcanVoiceInputModel describes one published speech transcription model.
// OpenVulcanVoiceInputModel 描述一个已发布的语音转写模型。
export interface OpenVulcanVoiceInputModel {
  id: string;
  display_name: string;
  supported_languages: string[];
}

// VoiceInputCloudCatalog describes availability and the account-scoped regional catalog.
// VoiceInputCloudCatalog 描述可用状态和账号所属区域的目录。
export type VoiceInputCloudCatalog =
  | { status: "sign_in_required" | "disabled" | "unavailable" }
  | {
    status: "available";
    connection_id: string;
    default_model_id: string;
    models: OpenVulcanVoiceInputModel[];
    // Effective cloud route resolved by Core, including the site default when applicable.
    // Core 解析的生效云端路由，适用时使用站点默认模型。
    selection: OpenVulcanVoiceInputSelection | null;
  };

// VoiceInputSettingsSnapshot is the saved choice with the usable candidates.
// VoiceInputSettingsSnapshot 是已保存的选择及可用候选。
export interface VoiceInputSettingsSnapshot {
  selection: VoiceInputSelection | null;
  // Whether the user explicitly switched voice input off.
  // 用户是否显式关闭语音输入。
  disabled: boolean;
  candidates: VoiceInputCandidate[];
  maxDurationSeconds: number;
}

// VoiceInputTranscription is the text transcribed from one recording.
// VoiceInputTranscription 是一段录音转写得到的文本。
export interface VoiceInputTranscription {
  text: string;
}
