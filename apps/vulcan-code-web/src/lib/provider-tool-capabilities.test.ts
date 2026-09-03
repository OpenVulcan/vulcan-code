import { describe, expect, it } from "vitest";

import type { PublicAiModelCapabilities, PublicAiResourceKind } from "@/types";
import { modelCapabilitiesSupportMediaUnderstanding } from "./provider-tool-capabilities";

// modelCapabilitiesFixture builds one exact conversation route for media admission tests.
// modelCapabilitiesFixture 构建一条用于媒体准入测试的精确会话路由。
function modelCapabilitiesFixture(resource: PublicAiResourceKind): PublicAiModelCapabilities {
  return {
    operations: ["conversation_respond"],
    input_resources: [resource],
    structured_output: "unsupported",
    evidence_revision: "test",
    reasoning: "native",
    prompt_cache: {
      provider_default_behavior: "provider_default",
      supports_managed_explicit: false,
      supported_ttl_options: [],
    },
    model_parameters: [],
  };
}

describe("modelCapabilitiesSupportMediaUnderstanding", () => {
  it("admits media routes without requiring structured output", () => {
    expect(modelCapabilitiesSupportMediaUnderstanding(
      modelCapabilitiesFixture("image"),
      "image_understand",
    )).toBe(true);
    expect(modelCapabilitiesSupportMediaUnderstanding(
      modelCapabilitiesFixture("video"),
      "video_understand",
    )).toBe(true);
    expect(modelCapabilitiesSupportMediaUnderstanding(
      modelCapabilitiesFixture("audio"),
      "audio_understand",
    )).toBe(true);
  });

  it("rejects routes without conversation execution or the requested media input", () => {
    // missingConversation removes the exact operation required by the replacement backend.
    // missingConversation 移除替代后端要求的精确操作。
    const missingConversation = modelCapabilitiesFixture("video");
    missingConversation.operations = [];

    expect(modelCapabilitiesSupportMediaUnderstanding(
      missingConversation,
      "video_understand",
    )).toBe(false);
    expect(modelCapabilitiesSupportMediaUnderstanding(
      modelCapabilitiesFixture("image"),
      "video_understand",
    )).toBe(false);
    expect(modelCapabilitiesSupportMediaUnderstanding(
      modelCapabilitiesFixture("video"),
      "video_generate",
    )).toBe(false);
  });
});
