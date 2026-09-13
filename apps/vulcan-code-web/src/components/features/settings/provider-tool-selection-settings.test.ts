// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { act, createElement, type ComponentProps } from "react";
import { createRoot, type Root } from "react-dom/client";
import { renderToStaticMarkup } from "react-dom/server";

import { I18nProvider } from "@/i18n";
import * as settingsService from "@/service/settings-actions-service";
import { mediaUnderstandingCapabilityToggleDisabled } from "@/lib/provider-tool-capabilities";
import type {
  PublicAiMediaUnderstandingModelCandidate,
  PublicAiProviderToolTargetSnapshot,
  PublicAiSettingsSnapshot,
} from "@/types";
import {
  MediaUnderstandingDefaults,
  ProviderToolSelectionSettingsArea,
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

    expect(markup).toContain("供应商专用模式");
    expect(markup).toContain("模型代替模式");
    expect(markup).toContain("请选择模型");
    expect(markup).toContain("请选择账号");
    expect(markup).not.toContain("MiMo V2.5");
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

// mountedRoots tracks only test-owned React trees for deterministic cleanup.
// mountedRoots 仅跟踪测试自有 React 树，确保确定性清理。
const mountedRoots: Root[] = [];
Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
afterEach(async () => {
  await act(async () => { mountedRoots.splice(0).forEach((root) => root.unmount()); });
  document.body.replaceChildren();
  vi.restoreAllMocks();
});

describe("empty auxiliary capability configuration", () => {
  it.each(["web_search", "web_extract", "image_generate", "image_edit", "video_generate", "speech_synthesize", "speech_transcribe", "music_generate"] as const)("keeps real configuration controls visible for %s", async (capability) => {
    // The read fixture contains exactly the empty projections consumed by this view.
    // 读取夹具仅包含该视图实际使用的空投影。
    vi.spyOn(settingsService, "loadPublicAiSettings").mockResolvedValue({
      providerToolTargets: [], providerExtensionTargetSets: {}, providerToolSelections: {},
      providerToolEnabledCapabilities: [],
    } as unknown as PublicAiSettingsSnapshot);
    // The mounted tree is isolated from real settings and uses the shared controls.
    // 挂载树与真实设置隔离，并使用共享控件。
    const container = document.createElement("div");
    const root = createRoot(container);
    document.body.append(container);
    mountedRoots.push(root);
    await act(async () => root.render(createElement(I18nProvider, { initialLocale: "zh-CN",
      children: createElement(ProviderToolSelectionSettingsArea, { capability }),
    })));
    expect(container.querySelector(".settings-provider-tool-defaults")).not.toBeNull();
    const selectors = Array.from(container.querySelectorAll<HTMLButtonElement>("button[role=combobox]"));
    expect(selectors).toHaveLength(2);
    expect(selectors.every((selector) => selector.disabled)).toBe(true);
    if (capability === "web_search" || capability === "web_extract") {
      expect(container.querySelector<HTMLButtonElement>("button[role=switch]")?.disabled).toBe(true);
    } else {
      expect(container.querySelector("button[role=switch]")).toBeNull();
    }
  });
});

// mountUnderstanding renders the real controls with exact synthetic routes and returns the owned container.
// mountUnderstanding 使用精确合成路由渲染真实控件，并返回自有容器。
async function mountUnderstanding(props: ComponentProps<typeof MediaUnderstandingDefaults>): Promise<HTMLDivElement> {
  // container and root belong exclusively to this test.
  // container 和 root 完全属于当前测试。
  const container = document.createElement("div");
  const root = createRoot(container);
  document.body.append(container);
  mountedRoots.push(root);
  await act(async () => root.render(createElement(I18nProvider, {
    initialLocale: "zh-CN",
    children: createElement(MediaUnderstandingDefaults, props),
  })));
  return container;
}

describe("understanding mode transitions", () => {
  it("shows an unavailable saved default as off even when another valid candidate exists", async () => {
    const candidate = audioCandidateFixture();
    const onEnabledChange = vi.fn();
    const container = await mountUnderstanding({ candidates: [candidate], targets: [], enabled: true, pending: false,
      selection: { mode: "model_replacement", selection: { ...candidate.selection, model_id: "deleted-model" } },
      onEnabledChange, onProviderTargetChange: vi.fn(), onSelectionChange: vi.fn() });
    const toggle = container.querySelector<HTMLButtonElement>("button[role=switch]")!;
    expect(toggle.getAttribute("aria-checked")).toBe("false");
    expect(toggle.disabled).toBe(true);
    expect(onEnabledChange).not.toHaveBeenCalled();
  });

  it("keeps an explicitly disabled default off after its exact selected target reappears", async () => {
    const candidate = audioCandidateFixture();
    const onEnabledChange = vi.fn();
    const props: ComponentProps<typeof MediaUnderstandingDefaults> = { candidates: [], targets: [], enabled: false, pending: false,
      selection: { mode: "model_replacement", selection: candidate.selection }, onEnabledChange,
      onProviderTargetChange: vi.fn(), onSelectionChange: vi.fn() };
    const container = await mountUnderstanding(props);
    expect(container.querySelector<HTMLButtonElement>("button[role=switch]")?.disabled).toBe(true);
    await act(async () => mountedRoots[mountedRoots.length - 1].render(createElement(I18nProvider, { initialLocale: "zh-CN",
      children: createElement(MediaUnderstandingDefaults, { ...props, candidates: [candidate] }),
    })));
    const toggle = container.querySelector<HTMLButtonElement>("button[role=switch]")!;
    expect(toggle.getAttribute("aria-checked")).toBe("false");
    expect(toggle.disabled).toBe(false);
    expect(onEnabledChange).not.toHaveBeenCalled();
  });

  it("shows both modes and complete disabled fields without any targets", async () => {
    const container = await mountUnderstanding({ candidates: [], targets: [], enabled: false, pending: false,
      onEnabledChange: vi.fn(), onProviderTargetChange: vi.fn(), onSelectionChange: vi.fn() });
    expect(container.querySelectorAll("button[aria-pressed]")).toHaveLength(2);
    expect(container.querySelectorAll("button[role=combobox]")).toHaveLength(2);
    expect(container.querySelector<HTMLButtonElement>("button[role=switch]")?.disabled).toBe(true);
  });

  it("disables the default before entering an empty mode and preserves saved routes", async () => {
    const candidate = audioCandidateFixture();
    const onEnabledChange = vi.fn(async () => true);
    const onSelectionChange = vi.fn();
    const onProviderTargetChange = vi.fn();
    const container = await mountUnderstanding({ candidates: [candidate], targets: [], enabled: true, pending: false,
      selection: { mode: "model_replacement", selection: candidate.selection }, onEnabledChange, onSelectionChange, onProviderTargetChange });
    await act(async () => container.querySelector<HTMLButtonElement>("button[aria-pressed=false]")?.click());
    expect(onEnabledChange).toHaveBeenCalledExactlyOnceWith(false);
    expect(onSelectionChange).not.toHaveBeenCalled();
    expect(onProviderTargetChange).not.toHaveBeenCalled();
    expect(container.querySelector("button[aria-pressed=true]")?.textContent).toContain("供应商");
  });

  it("keeps the previous mode when the default switch cannot be saved", async () => {
    const candidate = audioCandidateFixture();
    const container = await mountUnderstanding({ candidates: [candidate], targets: [], enabled: true, pending: false,
      selection: { mode: "model_replacement", selection: candidate.selection },
      onEnabledChange: vi.fn(async () => false), onSelectionChange: vi.fn(), onProviderTargetChange: vi.fn() });
    await act(async () => container.querySelector<HTMLButtonElement>("button[aria-pressed=false]")?.click());
    expect(container.querySelector("button[aria-pressed=true]")?.textContent).toContain("模型");
  });

  it("does not enable the default merely because the other mode has a candidate", async () => {
    const candidate = audioCandidateFixture();
    const container = await mountUnderstanding({ candidates: [candidate], targets: [], enabled: false, pending: false,
      onEnabledChange: vi.fn(), onSelectionChange: vi.fn(), onProviderTargetChange: vi.fn() });
    expect(container.querySelector<HTMLButtonElement>("button[role=switch]")?.disabled).toBe(true);
  });
});
