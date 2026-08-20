import { describe, expect, it } from "vitest";

import type { PublicAiContextCompactionWindowNode, PublicAiSettingsSnapshot } from "@/types";
import {
  availableContextCompactionWindowNodes,
  composerCapabilityIsEnabled,
  formatContextWindowNodeTokens,
  orderProviderToolCapabilities,
  orderedProviderToolCapabilities,
  providerToolCapabilitySwitchDisabled,
  providerToolSettingsConflicts,
} from "./composer-capability-controls";

// contextCompactionWindowNodesFixture returns the official fixed-node test catalog.
// contextCompactionWindowNodesFixture 返回官方固定节点测试目录。
function contextCompactionWindowNodesFixture(): PublicAiContextCompactionWindowNode[] {
  return [
    { id: "compression-window-1", tokens: 200_000 },
    { id: "compression-window-2", tokens: 256_000 },
    { id: "compression-window-3", tokens: 272_000 },
    { id: "compression-window-4", tokens: 373_000 },
    { id: "compression-window-5", tokens: 500_000 },
  ];
}

// settingsFixture builds the smallest catalog required to validate stale session routes.
// settingsFixture 构建验证过期会话路由所需的最小目录。
function settingsFixture(): PublicAiSettingsSnapshot {
  return {
    usageRefreshIntervalSeconds: 300,
    contextCompactionWindowNodes: {
      nodes: [
        { id: "compression-window-1", tokens: 200_000 },
        { id: "compression-window-2", tokens: 256_000 },
        { id: "compression-window-3", tokens: 272_000 },
        { id: "compression-window-4", tokens: 373_000 },
        { id: "compression-window-5", tokens: 500_000 },
      ],
    },
    providerPriorities: {},
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
    providerToolTargets: [
      {
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
      },
      {
        connectionId: "media-account",
        connectionDisplayName: "Media Account",
        manifestSourceLayer: "bundled",
        target: {
          target_id: "entry.video-01",
          provider_id: "provider",
          service_entry_id: "entry",
          credential_profile_ids: ["profile"],
          upstream_target_id: "video-01",
          display_name: "Video 01",
          contract: {
            tool_id: "video_understand",
            description: "Understand one video.",
            capability: "video_understand",
            input_schema: { type: "object" },
            output: {
              kind: "text",
              source_path: "$.text",
              normalized_result_path: "$.text",
            },
          },
        },
      },
    ],
    providerToolEnabledCapabilities: [],
    providerToolSelections: {},
    mediaUnderstandingSelections: {
      video_understand: {
        mode: "provider_tool",
        selection: {
          connection_id: "media-account",
          target_id: "entry.video-01",
        },
      },
    },
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

  it("validates enabled media against its fixed global backend without a legacy session selection", () => {
    // conflicts proves the session switch resolves the dedicated global backend store.
    // conflicts 证明会话开关解析专用全局后端存储。
    const conflicts = providerToolSettingsConflicts(settingsFixture(), {
      enabledCapabilities: ["video_understand"],
      selections: {},
      extensionTargetSets: {},
    });

    expect(conflicts).toEqual([]);
  });

  it("blocks enabled media when its fixed global backend is unavailable", () => {
    // snapshot removes only the selected video target while retaining its backend identity.
    // snapshot 仅移除已选视频目标，同时保留其后端身份。
    const snapshot = settingsFixture();
    snapshot.providerToolTargets = snapshot.providerToolTargets.filter(
      (target) => target.target.contract.capability !== "video_understand",
    );
    // conflicts must expose the stale exact route instead of silently choosing another backend.
    // conflicts 必须暴露过期精确路由，而不是静默选择其他后端。
    const conflicts = providerToolSettingsConflicts(snapshot, {
      enabledCapabilities: ["video_understand"],
      selections: {},
      extensionTargetSets: {},
    });

    expect(conflicts).toMatchObject([{
      capability: "video_understand",
      kind: "target_unavailable",
    }]);
  });

  it("accepts explicit none reasoning for a fixed-off replacement model", () => {
    // snapshot retains one admitted fixed-off video model with no selectable effort list.
    // snapshot 保留一个已准入、没有可选思考档位列表的固定关闭视频模型。
    const snapshot = settingsFixture();
    snapshot.providerToolTargets = snapshot.providerToolTargets.filter(
      (target) => target.target.contract.capability !== "video_understand",
    );
    snapshot.multimodalUnderstandingCandidateModels = [{
      providerId: "provider",
      modelId: "video-model",
    }];
    snapshot.mediaUnderstandingModelCandidates = [{
      providerDisplayName: "Provider",
      serviceEntryDisplayName: "Entry",
      connectionDisplayName: "Media Account",
      modelDisplayName: "Video Model",
      selection: {
        provider_id: "provider",
        model_id: "video-model",
        connection_id: "media-account",
      },
      catalogGeneration: 1,
      capabilities: {
        operations: ["conversation_respond"],
        input_resources: ["video"],
        structured_output: "json_object",
        reasoning: "unsupported",
        reasoning_control: "fixed_off",
        reasoning_efforts: [],
        prompt_cache: {
          provider_default_behavior: "disabled",
          supports_managed_explicit: false,
          supported_ttl_options: [],
        },
        model_parameters: [],
        evidence_revision: "test",
      },
    }];
    snapshot.mediaUnderstandingSelections = {
      video_understand: {
        mode: "model_replacement",
        selection: {
          provider_id: "provider",
          model_id: "video-model",
          connection_id: "media-account",
        },
        reasoning_effort: "none",
      },
    };
    // conflicts proves the exact fixed-off selection stays executable after persistence.
    // conflicts 证明精确固定关闭选择在持久化后仍保持可执行。
    const conflicts = providerToolSettingsConflicts(snapshot, {
      enabledCapabilities: ["video_understand"],
      selections: {},
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

describe("providerToolCapabilitySwitchDisabled", () => {
  it("allows an enabled stale media capability to be turned off", () => {
    expect(providerToolCapabilitySwitchDisabled(false, 0, true)).toBe(false);
    expect(providerToolCapabilitySwitchDisabled(false, 0, false)).toBe(true);
    expect(providerToolCapabilitySwitchDisabled(true, 1, true)).toBe(true);
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

  it("keeps a model-replacement-only media backend in the composer capability list", () => {
    // snapshot retains one media backend whose capability has no provider-tool target.
    // snapshot 保留一个没有供应商工具目标的媒体后端能力。
    const snapshot = settingsFixture();
    snapshot.providerToolTargets = snapshot.providerToolTargets.filter(
      (target) => target.target.contract.capability !== "video_understand",
    );
    snapshot.mediaUnderstandingSelections = {
      video_understand: {
        mode: "model_replacement",
        selection: {
          provider_id: "provider",
          model_id: "video-model",
          connection_id: "media-account",
        },
        reasoning_effort: "medium",
      },
    };

    expect(orderedProviderToolCapabilities(snapshot)).toContain("video_understand");
  });
});

describe("availableContextCompactionWindowNodes", () => {
  it("shows only nodes strictly below the current model hard window", () => {
    // nodes is the complete fixed-node catalog used by the boundary assertions.
    // nodes 是边界断言使用的完整固定节点目录。
    const nodes = contextCompactionWindowNodesFixture();

    expect(availableContextCompactionWindowNodes(nodes, 400_000).map((node) => node.tokens))
      .toEqual([200_000, 256_000, 272_000, 373_000]);
    expect(availableContextCompactionWindowNodes(nodes, 500_000).map((node) => node.tokens))
      .toEqual([200_000, 256_000, 272_000, 373_000]);
    expect(availableContextCompactionWindowNodes(nodes, 500_001).map((node) => node.tokens))
      .toEqual([200_000, 256_000, 272_000, 373_000, 500_000]);
  });
});

describe("formatContextWindowNodeTokens", () => {
  it("formats original model windows as integer K or M labels", () => {
    expect(formatContextWindowNodeTokens(400_000)).toBe("400K");
    expect(formatContextWindowNodeTokens(1_000_000)).toBe("1M");
    expect(formatContextWindowNodeTokens(1_500_000)).toBe("1500K");
  });
});
