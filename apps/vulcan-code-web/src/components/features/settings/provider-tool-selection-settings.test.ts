import { describe, expect, it } from "vitest";

import type { PublicAiProviderToolTargetSnapshot } from "@/types";
import { uniqueProviderToolModels } from "./provider-tool-selection-settings";

// targetFixture builds one account-expanded image target for grouping regression tests.
// targetFixture 为分组回归测试构建一个按账号展开的图片目标。
function targetFixture(
  connectionId: string,
  targetId: string,
  displayName: string,
): PublicAiProviderToolTargetSnapshot {
  return {
    connectionId,
    connectionDisplayName: connectionId,
    manifestSourceLayer: "bundled",
    target: {
      target_id: targetId,
      provider_id: "provider",
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
});
