import { describe, expect, it } from "vitest";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";

import { I18nProvider } from "@/i18n";
import type {
  PublicAiMediaUnderstandingModelCandidate,
  PublicAiProviderToolTargetSnapshot,
} from "@/types";
import {
  MediaUnderstandingDefaults,
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

// audioCandidateFixture builds one executable audio-replacement route.
// audioCandidateFixture 构建一条可执行的音频模型代替路由。
function audioCandidateFixture(): PublicAiMediaUnderstandingModelCandidate {
  return {
    providerDisplayName: "OpenCode Go",
    serviceEntryDisplayName: "OpenCode Go",
    connectionDisplayName: "go",
    modelDisplayName: "MiMo V2.5",
    selection: {
      provider_id: "opencode-go",
      model_id: "mimo-v2.5",
      connection_id: "connection-go",
    },
    catalogGeneration: 1,
    capabilities: {
      operations: ["conversation_respond"],
      input_resources: ["image", "audio", "video"],
      structured_output: "unsupported",
      reasoning: "unsupported",
      reasoning_control: "unknown",
      reasoning_efforts: [],
      prompt_cache: {
        provider_default_behavior: "provider_default",
        supports_managed_explicit: false,
        supported_ttl_options: [],
      },
      model_parameters: [],
      evidence_revision: "test",
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

describe("MediaUnderstandingDefaults", () => {
  it("does not present the first available candidate as a persisted backend", () => {
    const markup = renderToStaticMarkup(createElement(
      I18nProvider,
      {
        initialLocale: "zh-CN",
        children: createElement(MediaUnderstandingDefaults, {
          candidates: [audioCandidateFixture()],
          enabled: false,
          onEnabledChange: () => undefined,
          onProviderTargetChange: () => undefined,
          onSelectionChange: () => undefined,
          pending: false,
          targets: [],
        }),
      },
    ));

    expect(markup).toContain("请选择理解后端模式");
    expect(markup).toContain("该能力尚未选择目标。");
  });

  it("omits the complete reasoning control when the exact route has no override contract", () => {
    // candidate carries an explicit unsupported reasoning route rather than an empty UI fallback.
    // candidate 携带显式不支持覆写的推理路线，而不是空的 UI 回退。
    const candidate = audioCandidateFixture();
    // markup renders an active model-replacement route whose reasoning control must be absent.
    // markup 渲染一条活动模型替代路线，其推理控件必须完全缺席。
    const markup = renderToStaticMarkup(createElement(
      I18nProvider,
      {
        initialLocale: "zh-CN",
        children: createElement(MediaUnderstandingDefaults, {
          candidates: [candidate],
          enabled: true,
          onEnabledChange: () => undefined,
          onProviderTargetChange: () => undefined,
          onSelectionChange: () => undefined,
          pending: false,
          selection: {
            mode: "model_replacement",
            selection: candidate.selection,
          },
          targets: [],
        }),
      },
    ));

    expect(markup).not.toContain("思考级别");
    expect(markup).not.toContain("使用模型默认档位");
  });

  it("renders reasoning only when the exact route declares selectable levels", () => {
    // candidate clones the route identity while declaring an explicit effort-selection contract.
    // candidate 复用路线身份，同时声明显式档位选择契约。
    const candidate = {
      ...audioCandidateFixture(),
      capabilities: {
        ...audioCandidateFixture().capabilities,
        reasoning: "native" as const,
        reasoning_control: "effort_selection" as const,
        reasoning_efforts: ["low" as const, "high" as const],
      },
    };
    // markup must contain the whole reasoning field for this route.
    // markup 必须为当前路线包含完整推理字段。
    const markup = renderToStaticMarkup(createElement(
      I18nProvider,
      {
        initialLocale: "zh-CN",
        children: createElement(MediaUnderstandingDefaults, {
          candidates: [candidate],
          enabled: true,
          onEnabledChange: () => undefined,
          onProviderTargetChange: () => undefined,
          onSelectionChange: () => undefined,
          pending: false,
          selection: {
            mode: "model_replacement",
            selection: candidate.selection,
          },
          targets: [],
        }),
      },
    ));

    expect(markup).toContain("思考级别");
  });
});
