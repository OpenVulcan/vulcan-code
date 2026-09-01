// PublicAiMediaInlineThresholds carries backend-owned raw-byte limits; it defines no browser defaults.
// PublicAiMediaInlineThresholds 承载后端持有的原始字节限制，不定义浏览器默认值。
export interface PublicAiMediaInlineThresholds {
  // Inclusive threshold with a real upload contract; zero disables inline preference.
  // 有真实上传契约时的包含边界阈值；零禁用内联偏好。
  with_upload_max_bytes: number;
  // Inclusive threshold without real upload; zero disables inline delivery.
  // 无真实上传时的包含边界阈值；零禁用内联投递。
  without_upload_max_bytes: number;
}

// PublicAiMediaPurposePolicy keeps four media kinds and their aggregate encoded budget independent.
// PublicAiMediaPurposePolicy 独立保存四种媒体类型及其累计编码预算。
export interface PublicAiMediaPurposePolicy {
  // Image thresholds, exactly as returned by the backend.
  // 后端返回的精确图片阈值。
  image: PublicAiMediaInlineThresholds;
  // Audio thresholds, exactly as returned by the backend.
  // 后端返回的精确音频阈值。
  audio: PublicAiMediaInlineThresholds;
  // Video thresholds, exactly as returned by the backend.
  // 后端返回的精确视频阈值。
  video: PublicAiMediaInlineThresholds;
  // File thresholds, including PDF inputs.
  // 文件阈值，包括 PDF 输入。
  file: PublicAiMediaInlineThresholds;
  // Combined Base64 bytes; protocol framing remains a separate backend check.
  // 累计 Base64 字节；协议封装仍由后端单独检查。
  max_total_encoded_bytes: number;
}

// PublicAiMediaDeliveryPolicy matches the exact versioned Rust persistence contract.
// PublicAiMediaDeliveryPolicy 与精确版本化 Rust 持久化契约一致。
export interface PublicAiMediaDeliveryPolicy {
  // Exact supported policy version.
  // 精确支持的策略版本。
  version: 1;
  // Main conversation policy, including sub-agent conversations.
  // 主会话策略，包括子智能体会话。
  conversation: PublicAiMediaPurposePolicy;
  // Independent extension policy; not inherited from the main conversation.
  // 独立扩展策略，不继承主会话策略。
  independent_extension: PublicAiMediaPurposePolicy;
}

// PublicAiMediaDeliverySettingsSnapshot exposes effective settings without client-side policy resolution.
// PublicAiMediaDeliverySettingsSnapshot 公开生效设置，不在客户端解析策略。
export interface PublicAiMediaDeliverySettingsSnapshot {
  // Complete effective policy supplied by the backend.
  // 后端提供的完整生效策略。
  policy: PublicAiMediaDeliveryPolicy;
  // Settings content revision, separate from provider manifest generations.
  // 设置内容修订，与供应商清单代际分离。
  revision: string;
  // Backend ceiling for configurable raw inline thresholds, not file admission.
  // 后端对可配置原始内联阈值的上限，不是文件准入限制。
  maxInlineRawBytes: number;
  // Backend ceiling for configurable aggregate encoded budgets.
  // 后端对可配置累计编码预算的上限。
  maxTotalEncodedBytes: number;
}
