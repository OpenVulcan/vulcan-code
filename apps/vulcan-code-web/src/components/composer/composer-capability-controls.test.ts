import { describe, expect, it } from "vitest";
import { mediaDeliverySettingsFixture } from "@/test-fixtures/media-delivery";
import {
  mediaReplacementReasoningEfforts,
  mediaReplacementReasoningOverrides,
} from "@/lib/reasoning-effort-labels";

import type {
  PublicAiContextCompactionWindowNode,
  PublicAiModelCapabilities,
  PublicAiSettingsSnapshot,
} from "@/types";
import {
  availableContextCompactionWindowNodes,
  composerCapabilityIsEnabled,
  formatContextWindowNodeTokens,
  orderProviderToolCapabilities,
  orderedProviderToolCapabilities,
  providerToolCapabilitySwitchDisabled,
  providerToolSettingsConflicts,
  supportedMediaReplacementReasoningEffort,
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
    manualModelDefaults: { contextLimit: 200_000, outputLimit: 32_000 },
    usageRefreshIntervalSeconds: 300,
    mediaDelivery: mediaDeliverySettingsFixture(),
    contextCompactionWindowNodes: {
      nodes: [
        { id: "compression-window-1", tokens: 200_000 },
        { id: "compression-window-2", tokens: 256_000 },
        { id: "compression-window-3", tokens: 272_000 },
        { id: "compression-window-4", tokens: 373_000 },
        { id: "compression-window-5", tokens: 500_000 },
      ],
    },
    newSessionContextCompactionWindowNodeId: null,
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
    mediaUnderstandingBackendOptions: {
      video_understand: {
        provider_tool: {
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
  it("accepts an empty session-owned media route snapshot", () => {
    const legacySettings = {
      enabledCapabilities: [],
      selections: {},
      extensionTargetSets: {},
      mediaUnderstandingSelections: {},
    } as unknown as Parameters<typeof providerToolSettingsConflicts>[1];

    expect(providerToolSettingsConflicts(settingsFixture(), legacySettings)).toEqual([]);
  });

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
      mediaUnderstandingSelections: {},
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
      mediaUnderstandingSelections: {},
    });

    expect(conflicts).toEqual([]);
  });

  it("validates enabled media against its exact session route", () => {
    // conflicts proves the session owns the provider target instead of resolving a global route.
    // conflicts 证明会话持有供应商目标，而不是解析全局路由。
    const conflicts = providerToolSettingsConflicts(settingsFixture(), {
      enabledCapabilities: ["video_understand"],
      selections: {},
      extensionTargetSets: {},
      mediaUnderstandingSelections: {
        video_understand: {
          mode: "provider_tool",
          selection: {
            connection_id: "media-account",
            target_id: "entry.video-01",
          },
        },
      },
    });

    expect(conflicts).toEqual([]);
  });

  it("ignores a disabled stale media route", () => {
    const conflicts = providerToolSettingsConflicts(settingsFixture(), {
      enabledCapabilities: [],
      selections: {},
      extensionTargetSets: {},
      mediaUnderstandingSelections: {
        video_understand: {
          mode: "provider_tool",
          selection: {
            connection_id: "removed-account",
            target_id: "entry.video-01",
          },
        },
      },
    });

    expect(conflicts).toEqual([]);
  });

  it("blocks enabled media when its exact session route is unavailable", () => {
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
      mediaUnderstandingSelections: {
        video_understand: {
          mode: "provider_tool",
          selection: {
            connection_id: "media-account",
            target_id: "entry.video-01",
          },
        },
      },
    });

    expect(conflicts).toMatchObject([{
      capability: "video_understand",
      kind: "target_unavailable",
    }]);
  });

  it("accepts explicit none reasoning for a fixed-off replacement model without structured output", () => {
    // snapshot retains one admitted fixed-off video model that returns plain text only.
    // snapshot 保留一个已准入且只返回纯文本的固定关闭视频模型。
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
        structured_output: "unsupported",
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
    snapshot.mediaUnderstandingBackendOptions = {
      video_understand: {
        model_replacement: {
          selection: {
            provider_id: "provider",
            model_id: "video-model",
            connection_id: "media-account",
          },
          reasoning_effort: "none",
        },
      },
    };
    // conflicts proves the exact fixed-off selection stays executable after persistence.
    // conflicts 证明精确固定关闭选择在持久化后仍保持可执行。
    const conflicts = providerToolSettingsConflicts(snapshot, {
      enabledCapabilities: ["video_understand"],
      selections: {},
      extensionTargetSets: {},
      mediaUnderstandingSelections: {
        video_understand: {
          mode: "model_replacement",
          selection: {
            provider_id: "provider",
            model_id: "video-model",
            connection_id: "media-account",
          },
          reasoning_effort: "none",
        },
      },
    });

    expect(conflicts).toEqual([]);
  });
});

describe("supportedMediaReplacementReasoningEffort", () => {
  it("drops a stale system effort instead of poisoning an independent session route", () => {
    const snapshot = settingsFixture();
    const candidate = {
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
        ...snapshot.customProviderSystemModelDefaults,
        operations: ["conversation_respond" as const],
        input_resources: ["video" as const],
        reasoning_control: "effort_selection" as const,
        reasoning_efforts: ["low" as const, "high" as const],
        prompt_cache: {
          provider_default_behavior: "provider_default" as const,
          supports_managed_explicit: false,
          supported_ttl_options: [],
        },
        model_parameters: [],
        evidence_revision: "test",
      },
    };

    expect(supportedMediaReplacementReasoningEffort(candidate, "high")).toBe("high");
    expect(supportedMediaReplacementReasoningEffort(candidate, "adaptive")).toBeUndefined();
  });
});

describe("mediaReplacementReasoningEfforts", () => {
  it("separates route-accepted values from user-overridable selector values", () => {
    // capabilities contains the complete signed route shape consumed by the selector projector.
    // capabilities 包含选择器投影器消费的完整签名路由形状。
    const capabilities: PublicAiModelCapabilities = {
      operations: ["conversation_respond"],
      input_resources: ["video"],
      structured_output: "json_object",
      reasoning: "native",
      reasoning_control: "provider_adaptive",
      reasoning_efforts: ["adaptive"],
      prompt_cache: {
        provider_default_behavior: "provider_default",
        supports_managed_explicit: false,
        supported_ttl_options: [],
      },
      model_parameters: [],
      evidence_revision: "test",
    };
    expect(mediaReplacementReasoningEfforts(capabilities)).toEqual(["adaptive"]);
    expect(mediaReplacementReasoningOverrides(capabilities)).toEqual([]);
    expect(mediaReplacementReasoningEfforts({
      ...capabilities,
      reasoning_control: "fixed_off",
      reasoning_efforts: [],
    })).toEqual(["none"]);
    expect(mediaReplacementReasoningOverrides({
      ...capabilities,
      reasoning_control: "fixed_off",
      reasoning_efforts: [],
    })).toEqual([]);
    expect(mediaReplacementReasoningEfforts({
      ...capabilities,
      reasoning_control: "fixed_on",
      reasoning_efforts: [],
    })).toEqual([]);
    expect(mediaReplacementReasoningEfforts({
      ...capabilities,
      reasoning_control: "toggle",
      reasoning_efforts: ["none", "adaptive"],
    })).toEqual(["none", "adaptive"]);
    expect(mediaReplacementReasoningOverrides({
      ...capabilities,
      reasoning_control: "toggle",
      reasoning_efforts: ["none", "adaptive"],
    })).toEqual(["none", "adaptive"]);
    expect(mediaReplacementReasoningEfforts({
      ...capabilities,
      reasoning_control: "effort_selection",
      reasoning_efforts: ["low", "high"],
    })).toEqual(["low", "high"]);
    expect(mediaReplacementReasoningOverrides({
      ...capabilities,
      reasoning_control: "effort_selection",
      reasoning_efforts: ["low", "high"],
    })).toEqual(["low", "high"]);
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
      mediaUnderstandingSelections: {},
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

  it("keeps an available media model in the list without a system default", () => {
    // snapshot exposes one executable video model while system defaults remain empty.
    // snapshot 公开一个可执行视频模型，同时系统默认值保持为空。
    const snapshot = settingsFixture();
    snapshot.providerToolTargets = snapshot.providerToolTargets.filter(
      (target) => target.target.contract.capability !== "video_understand",
    );
    snapshot.mediaUnderstandingSelections = {};
    snapshot.multimodalUnderstandingCandidateModels = [];
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
        structured_output: "unsupported",
        reasoning: "native",
        reasoning_control: "provider_adaptive",
        reasoning_efforts: ["adaptive"],
        prompt_cache: {
          provider_default_behavior: "provider_default",
          supports_managed_explicit: false,
          supported_ttl_options: [],
        },
        model_parameters: [],
        evidence_revision: "test",
      },
    }];

    expect(orderedProviderToolCapabilities(snapshot)).toContain("video_understand");
  });

  it("shows audio understanding whenever an executable model supports audio input", () => {
    // snapshot has no saved audio backend and does not require a system admission switch.
    // snapshot 没有已保存的音频后端，也不要求系统准入开关。
    const snapshot = settingsFixture();
    snapshot.multimodalUnderstandingCandidateModels = [{
      providerId: "provider",
      modelId: "audio-model",
    }];
    snapshot.mediaUnderstandingModelCandidates = [{
      providerDisplayName: "Provider",
      serviceEntryDisplayName: "Entry",
      connectionDisplayName: "Media Account",
      modelDisplayName: "Audio Model",
      selection: {
        provider_id: "provider",
        model_id: "audio-model",
        connection_id: "media-account",
      },
      catalogGeneration: 1,
      capabilities: {
        operations: ["conversation_respond"],
        input_resources: ["audio"],
        structured_output: "unsupported",
        reasoning: "native",
        reasoning_control: "fixed_on",
        reasoning_efforts: [],
        prompt_cache: {
          provider_default_behavior: "provider_default",
          supports_managed_explicit: false,
          supported_ttl_options: [],
        },
        model_parameters: [],
        evidence_revision: "test",
      },
    }];

    expect(orderedProviderToolCapabilities(snapshot)).toContain("audio_understand");
    snapshot.multimodalUnderstandingCandidateModels = [];
    expect(orderedProviderToolCapabilities(snapshot)).toContain("audio_understand");
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
