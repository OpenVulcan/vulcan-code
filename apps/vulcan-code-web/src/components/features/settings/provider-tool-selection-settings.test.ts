import { describe, expect, it } from "vitest";

import type { PublicAiProviderToolTargetSnapshot } from "@/types";
import {
  mediaUnderstandingCapabilityToggleDisabled,
  providerToolModelKey,
  uniqueProviderToolModels,
} from "./provider-tool-selection-settings";

// targetFixture builds one account-expanded image target for grouping regression tests.
// targetFixture 为分组回归测试构建一个按账号展开的图片目标。
function targetFixture(
  connectionId: string,
  targetId: string,
  displayName: string,
  providerId = "provider",
): PublicAiProviderToolTargetSnapshot {
  return {
    connectionId,
    connectionDisplayName: connectionId,
    manifestSourceLayer: "bundled",
    target: {
      target_id: targetId,
      provider_id: providerId,
      service_entry_id: "entry",
      credential_profile_ids: ["profile"],
      upstream_target_id: targetId,
      display_name: displayName,
      contract: {
        tool_id: "image_generate",
        description: "Generate an image.",
        capability: "image_generate",
        input_schema: { type: "object" },
        output: {
          kind: "resources",
          source_path: "$.data",
          normalized_result_path: "$.resources",
        },
      },
    },
  };
}

describe("uniqueProviderToolModels", () => {
  it("collapses the same model exposed through multiple accounts", () => {
    const models = uniqueProviderToolModels([
      targetFixture("account-b", "entry.image-01", "Image 01"),
      targetFixture("account-a", "entry.image-01", "Image 01"),
      targetFixture("account-c", "entry.image-02", "Image 02"),
    ]);

    expect(models.map((target) => target.target.target_id)).toEqual([
      "entry.image-01",
      "entry.image-02",
    ]);
  });

  it("keeps identical upstream target ids from different providers distinct", () => {
    const models = uniqueProviderToolModels([
      targetFixture("account-a", "video-understanding", "Video", "provider-a"),
      targetFixture("account-b", "video-understanding", "Video", "provider-b"),
    ]);

    expect(models).toHaveLength(2);
    expect(new Set(models.map(providerToolModelKey)).size).toBe(2);
  });
});

describe("mediaUnderstandingCapabilityToggleDisabled", () => {
  it("allows an enabled stale capability to be turned off when its backend disappears", () => {
    expect(mediaUnderstandingCapabilityToggleDisabled(false, false, true)).toBe(false);
  });

  it("blocks new enablement without a backend and every concurrent mutation", () => {
    expect(mediaUnderstandingCapabilityToggleDisabled(false, false, false)).toBe(true);
    expect(mediaUnderstandingCapabilityToggleDisabled(true, true, true)).toBe(true);
  });
});
