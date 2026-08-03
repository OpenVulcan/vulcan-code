import { describe, expect, it } from "vitest";

import type { PublicAiSettingsSnapshot } from "@/types";
import {
  orderProviderToolCapabilities,
  providerToolSettingsConflicts,
} from "./composer-capability-controls";

// settingsFixture builds the smallest catalog required to validate stale session routes.
// settingsFixture 构建验证过期会话路由所需的最小目录。
function settingsFixture(): PublicAiSettingsSnapshot {
  return {
    usageRefreshIntervalSeconds: 300,
    usageCache: {},
    connections: [],
    providers: [],
    providerDefaults: {},
    catalogs: {},
    modelSupportCatalogs: {},
    catalogErrors: {},
    sourceCatalogs: {},
    catalogStatuses: {},
    modelOverrides: {},
    modelOverrideRecords: [],
    modelAccessRecords: [],
    connectionModelAccessPolicyRecords: [],
    customModels: {},
    modelCapabilityOverrides: {},
    modelCapabilityOverrideRecords: [],
    providerToolTargets: [{
      connectionId: "account-a",
      connectionDisplayName: "Account A",
      target: {
        target_id: "entry.image-01",
        provider_id: "provider",
        service_entry_id: "entry",
        credential_profile_ids: ["profile"],
        upstream_target_id: "image-01",
        display_name: "Image 01",
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
    }],
    providerToolEnabledCapabilities: [],
    providerToolSelections: {},
    internalInferenceCandidates: [],
    inferenceProfiles: {},
    inferenceRouteDiagnostics: [],
    internalModelOverrides: {},
  };
}

describe("providerToolSettingsConflicts", () => {
  it("blocks an enabled capability whose saved account disappeared", () => {
    const conflicts = providerToolSettingsConflicts(settingsFixture(), {
      enabledCapabilities: ["image_generate"],
      selections: {
        image_generate: {
          connection_id: "removed-account",
          target_id: "entry.image-01",
        },
      },
    });

    expect(conflicts).toMatchObject([{
      capability: "image_generate",
      kind: "target_unavailable",
    }]);
  });

  it("does not block a disabled capability with a stale saved route", () => {
    const conflicts = providerToolSettingsConflicts(settingsFixture(), {
      enabledCapabilities: [],
      selections: {
        image_generate: {
          connection_id: "removed-account",
          target_id: "entry.image-01",
        },
      },
    });

    expect(conflicts).toEqual([]);
  });
});

describe("orderProviderToolCapabilities", () => {
  it("keeps web search and web extract as the first two capabilities", () => {
    expect(orderProviderToolCapabilities([
      "media_analyze",
      "web_extract",
      "image_generate",
      "web_search",
      "image_edit",
      "web_search",
    ])).toEqual([
      "web_search",
      "web_extract",
      "image_edit",
      "image_generate",
      "media_analyze",
    ]);
  });
});
