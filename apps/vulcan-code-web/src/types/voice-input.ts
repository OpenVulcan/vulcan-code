// VoiceInputSummary is the composer voice-input state published with the workbench snapshot.
// VoiceInputSummary 是随工作台快照发布的输入区语音输入状态。
export interface VoiceInputSummary {
  // Whether the user has chosen a speech-transcription target for voice input.
  // 用户是否已为语音输入选择语音转写目标。
  configured: boolean;
  // Longest single recording in seconds; recording stops at this limit.
  // 单次录音的最长秒数；到达上限时停止录音。
  maxDurationSeconds: number;
}

// VoiceInputTargetSelection identifies one exact speech-transcription target on one account.
// VoiceInputTargetSelection 标识某个账号上的一个精确语音转写目标。
export interface VoiceInputTargetSelection {
  connection_id: string;
  target_id: string;
}

// VoiceInputCandidate is one target that can transcribe a composer recording.
// VoiceInputCandidate 是一个可以转写输入区录音的目标。
export interface VoiceInputCandidate {
  connectionId: string;
  connectionDisplayName: string;
  providerId: string;
  targetId: string;
  displayName: string;
}

// VoiceInputSettingsSnapshot is the saved choice with the usable candidates.
// VoiceInputSettingsSnapshot 是已保存的选择及可用候选。
export interface VoiceInputSettingsSnapshot {
  selection: VoiceInputTargetSelection | null;
  candidates: VoiceInputCandidate[];
  maxDurationSeconds: number;
}

// VoiceInputTranscription is the text transcribed from one recording.
// VoiceInputTranscription 是一段录音转写得到的文本。
export interface VoiceInputTranscription {
  text: string;
}
