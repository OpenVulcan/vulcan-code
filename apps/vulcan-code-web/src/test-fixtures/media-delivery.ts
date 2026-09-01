import type { PublicAiMediaDeliverySettingsSnapshot, PublicAiMediaPurposePolicy } from "@/types";

// Returns an isolated disabled policy for tests, not production defaults or a client-side fallback.
// 返回供测试使用的独立禁用策略，不是生产默认值或客户端兜底。
function disabledPurposeFixture(): PublicAiMediaPurposePolicy {
  return {
    image: { with_upload_max_bytes: 0, without_upload_max_bytes: 0 },
    audio: { with_upload_max_bytes: 0, without_upload_max_bytes: 0 },
    video: { with_upload_max_bytes: 0, without_upload_max_bytes: 0 },
    file: { with_upload_max_bytes: 0, without_upload_max_bytes: 0 },
    max_total_encoded_bytes: 0,
  };
}

// Returns a complete browser settings fixture with distinct objects for each purpose.
// 返回完整浏览器设置夹具，为每种用途使用独立对象。
export function mediaDeliverySettingsFixture(): PublicAiMediaDeliverySettingsSnapshot {
  return {
    policy: {
      version: 1,
      conversation: disabledPurposeFixture(),
      independent_extension: disabledPurposeFixture(),
    },
    revision: "test-media-policy-revision",
    maxInlineRawBytes: 50_331_648,
    maxTotalEncodedBytes: 67_108_864,
  };
}
