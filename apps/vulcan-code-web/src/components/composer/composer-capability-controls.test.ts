import { describe, expect, it } from "vitest";

import type { PublicAiSettingsSnapshot } from "@/types";
import {
  composerCapabilityIsEnabled,
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
    providerToolTemplateModels: [],
    connectionModelAccessPolicyRecords: [],
    customModels: {},
    customProviderSystemModelDefaults: {
      context_limit: 200_000,
      output_limit: 32_768,
      input_resources: [],
      reasoning: "native",
      reasoning_control: "effort_selection",
      reasoning_efforts: ["none", "minimal", "low", "medium", "high", "xhigh"],
    },
    modelCapabilityOverrides: {},
    modelCapabilityOverrideRecords: [],
    providerToolTargets: [{
      connectionId: "account-a",
      connectionDisplayName: "Account A",
      manifestSourceLayer: "bundled",
      target: {
        target_id: "entry.search-01",
        provider_id: "provider",
        service_entry_id: "entry",
        credential_profile_ids: ["profile"],
        upstream_target_id: "search-01",
        display_name: "Search 01",
        contract: {
          tool_id: "web_search",
          description: "Search the web.",
          capability: "web_search",
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
    providerExtensionTargetSets: {},
    internalInferenceCandidates: [],
    inferenceProfiles: {},
    inferenceRouteDiagnostics: [],
    internalModelOverrides: {},
    contextWindowPolicyOverrides: {},
  };
}

describe("providerToolSettingsConflicts", () => {
  it("blocks an enabled capability whose saved account disappeared", () => {
    const conflicts = providerToolSettingsConflicts(settingsFixture(), {
      enabledCapabilities: ["web_search"],
      selections: {
        web_search: {
          connection_id: "removed-account",
          target_id: "entry.search-01",
        },
      },
      extensionTargetSets: {},
    });

    expect(conflicts).toMatchObject([{
      capability: "web_search",
      kind: "target_unavailable",
    }]);
  });

  it("does not block a disabled capability with a stale saved route", () => {
    const conflicts = providerToolSettingsConflicts(settingsFixture(), {
      enabledCapabilities: [],
      selections: {
        web_search: {
          connection_id: "removed-account",
          target_id: "entry.search-01",
        },
      },
      extensionTargetSets: {},
    });

    expect(conflicts).toEqual([]);
  });
});

describe("composerCapabilityIsEnabled", () => {
  it("treats extension selections as enabled without adding them to direct switches", () => {
    const settings = {
      enabledCapabilities: ["web_search" as const],
      selections: {},
      extensionTargetSets: {
        image_generate: [{ connection_id: "image-account", target_id: "image-model" }],
      },
    };

    expect(composerCapabilityIsEnabled(settings, "web_search")).toBe(true);
    expect(composerCapabilityIsEnabled(settings, "image_generate")).toBe(true);
    expect(composerCapabilityIsEnabled(settings, "video_generate")).toBe(false);
  });
});

describe("orderProviderToolCapabilities", () => {
  it("keeps web search and web extract as the first two capabilities", () => {
    expect(orderProviderToolCapabilities([
      "image_understand",
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
      "image_understand",
    ]);
  });
});
