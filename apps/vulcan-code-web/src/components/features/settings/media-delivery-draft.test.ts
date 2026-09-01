import { describe, expect, it } from "vitest";
import { mediaDeliverySettingsFixture } from "@/test-fixtures/media-delivery";
import { createMediaDeliveryDraft, MEDIA_DELIVERY_KINDS, MEDIA_DELIVERY_PURPOSES, MediaDeliveryDraftError, parseMediaDeliveryDraft } from "./media-delivery-draft";

// settingsFixture supplies deliberately small backend ceilings so tests cannot pass through frontend defaults.
// settingsFixture 提供刻意较小的后端上限，防止测试依靠前端默认值通过。
function settingsFixture() {
  return { ...mediaDeliverySettingsFixture(), maxInlineRawBytes: 8192, maxTotalEncodedBytes: 16384 };
}

describe("media delivery editor draft", () => {
  // Every arbitrary raw byte remains exact across backend-to-KiB-to-backend conversion.
  // 任意原始字节均在后端、KiB、后端往返转换中保持精确。
  it("round trips sixteen distinct byte thresholds and both independent budgets", () => {
    const settings = settingsFixture();
    let bytes = 1;
    for (const purpose of MEDIA_DELIVERY_PURPOSES) {
      for (const kind of MEDIA_DELIVERY_KINDS) {
        settings.policy[purpose][kind].with_upload_max_bytes = bytes++;
        settings.policy[purpose][kind].without_upload_max_bytes = bytes++;
      }
      settings.policy[purpose].max_total_encoded_bytes = bytes++;
    }
    const before = JSON.stringify(settings);
    expect(parseMediaDeliveryDraft(createMediaDeliveryDraft(settings.policy), settings)).toEqual(settings.policy);
    expect(JSON.stringify(settings)).toBe(before);
  });

  // Editing one case cannot leak into another media kind or the other execution purpose.
  // 编辑一个分支不能泄漏到其他媒体类型或另一执行用途。
  it("changes only the selected purpose kind and upload case", () => {
    const settings = settingsFixture();
    const draft = createMediaDeliveryDraft(settings.policy);
    draft.independent_extension.video.with_upload_max_bytes = "2.5";
    const parsed = parseMediaDeliveryDraft(draft, settings);
    expect(parsed.independent_extension.video.with_upload_max_bytes).toBe(2560);
    expect(parsed.independent_extension.video.without_upload_max_bytes).toBe(settings.policy.independent_extension.video.without_upload_max_bytes);
    expect(parsed.conversation).toEqual(settings.policy.conversation);
    expect(parsed.independent_extension.image).toEqual(settings.policy.independent_extension.image);
    expect(settings.policy.independent_extension.video.with_upload_max_bytes).toBe(0);
  });

  // Equal-to-limit and zero remain intentional values, including one-byte granularity.
  // 等于上限及零均保持明确值，并支持单字节粒度。
  it("accepts inclusive backend ceilings explicit zero and one byte", () => {
    const settings = settingsFixture();
    const draft = createMediaDeliveryDraft(settings.policy);
    draft.conversation.image.without_upload_max_bytes = "8";
    draft.conversation.file.without_upload_max_bytes = "0.0009765625000000000000";
    draft.independent_extension.video.without_upload_max_bytes = "0";
    draft.independent_extension.max_total_encoded_bytes = "16";
    const policy = parseMediaDeliveryDraft(draft, settings);
    expect(policy.conversation.image.without_upload_max_bytes).toBe(8192);
    expect(policy.conversation.file.without_upload_max_bytes).toBe(1);
    expect(policy.independent_extension.video.without_upload_max_bytes).toBe(0);
    expect(policy.independent_extension.max_total_encoded_bytes).toBe(16384);
  });

  // Invalid edits remain errors rather than being coerced to disabled defaults or rounded byte counts.
  // 无效编辑保持错误，不强制变成禁用默认值，也不对字节数取整。
  it.each([
    ["", "number"], [" ", "number"], ["-1", "number"], ["NaN", "number"], ["Infinity", "number"],
    ["1e2", "number"], ["0x10", "number"], ["8.0009765625", "limit"], ["0.1", "fraction"],
    ["1.00000000000000000001", "fraction"], ["7.99999999999999999999", "fraction"],
  ] as const)("rejects %j with an exact field and reason", (value, code) => {
    const settings = settingsFixture();
    const draft = createMediaDeliveryDraft(settings.policy);
    draft.conversation.audio.with_upload_max_bytes = value;
    try {
      parseMediaDeliveryDraft(draft, settings);
      throw new Error("invalid editor value unexpectedly passed");
    } catch (error) {
      expect(error).toBeInstanceOf(MediaDeliveryDraftError);
      expect(error).toMatchObject({ field: "conversation-audio-with_upload_max_bytes", code });
    }
  });

  // Aggregate limits use their own backend ceiling, not the per-file threshold ceiling.
  // 累计限制使用自身后端上限，而不是逐文件阈值上限。
  it("uses the independently supplied aggregate ceiling", () => {
    const settings = settingsFixture();
    const draft = createMediaDeliveryDraft(settings.policy);
    draft.conversation.max_total_encoded_bytes = "16.0009765625";
    expect(() => parseMediaDeliveryDraft(draft, settings)).toThrow(MediaDeliveryDraftError);
    draft.conversation.max_total_encoded_bytes = "12";
    expect(parseMediaDeliveryDraft(draft, settings).conversation.max_total_encoded_bytes).toBe(12288);
  });

  // Every byte remainder must survive exact decimal parsing, including values near the backend ceiling.
  // 每一种字节余数都必须通过精确十进制解析，包括接近后端上限的值。
  it("preserves every KiB byte remainder near zero and the raw ceiling", () => {
    // These are actual advertised test-fixture ceilings, not defaults supplied by the editor.
    // 此处使用测试夹具实际公布的上限，不是编辑器提供的默认值。
    const settings = mediaDeliverySettingsFixture();
    settings.policy.independent_extension.max_total_encoded_bytes = settings.maxTotalEncodedBytes - 1;
    for (const base of [0, 1024, settings.maxInlineRawBytes - 1024]) {
      for (let remainder = 0; remainder < 1024; remainder += 1) {
        settings.policy.conversation.image.without_upload_max_bytes = base + remainder;
        expect(parseMediaDeliveryDraft(createMediaDeliveryDraft(settings.policy), settings)).toEqual(settings.policy);
      }
    }
  });
});
