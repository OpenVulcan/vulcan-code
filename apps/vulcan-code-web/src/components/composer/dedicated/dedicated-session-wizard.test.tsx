// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { TooltipProvider } from "@/components/ui/tooltip";
import {
  draftDedicatedPrompt,
  subscribeDedicatedPromptDraftProgress,
  type DedicatedPromptDraftProgressEvent,
  type DedicatedPromptDraftResponse,
} from "@/service/dedicated-session-service";
import type { OutputStyleCatalogSummary, ProviderSummary, ReasoningSelectionSummary } from "@/types";
import { INITIAL_DEDICATED_DRAFT, type DedicatedDraftState } from "./dedicated-draft-store";
import { DedicatedSessionWizard } from "./dedicated-session-wizard";

// translate keeps callback identity stable while exposing exact semantic labels.
// translate 保持回调身份稳定，同时暴露精确语义标签。
const translate = (key: string) => key;
vi.mock("@/i18n", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/i18n")>()),
  useI18n: () => ({ locale: "en-US", t: translate }),
}));
vi.mock("@/service/dedicated-session-service", () => ({
  draftDedicatedPrompt: vi.fn(),
  subscribeDedicatedPromptDraftProgress: vi.fn(),
}));
vi.mock("@/service/knowledge-base-service", () => ({ listKnowledgeLibraries: vi.fn(() => Promise.resolve([])) }));

let root: Root;
let host: HTMLDivElement;
beforeEach(() => {
  vi.clearAllMocks(); vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

// providers builds one active model route with the given reasoning selector.
// providers 以给定思考选择器构建单个激活模型路由。
function providers(reasoningSelection: ReasoningSelectionSummary): ProviderSummary[] {
  return [{ id: "a", sourceProviderId: "p", connectionId: "a", accountName: "Account", name: "Provider", status: "ready",
    models: [{ id: "m", name: "Model", active: true, reasoningSelection }] }] as ProviderSummary[];
}

// renderWizard mounts the requirement step with a filled description.
// renderWizard 以已填写的描述挂载需求步骤。
async function renderWizard(routes: ProviderSummary[]) {
  const state: DedicatedDraftState = { ...INITIAL_DEDICATED_DRAFT, kind: "dedicated", step: "requirement", requirement: "translate" };
  await act(async () => root.render(
    <TooltipProvider>
      <DedicatedSessionWizard
        draft={{ draftId: "d", projectId: "project", projects: [], submissionPending: false, onChangeProject: vi.fn() }}
        onBusyChange={vi.fn()}
        onChange={vi.fn()}
        onSelectWorkbenchModel={vi.fn()}
        onSubmitPrompt={vi.fn()}
        outputStyleCatalog={{ entries: [], selectedId: "" } as unknown as OutputStyleCatalogSummary}
        providers={routes}
        state={state}
      />
    </TooltipProvider>,
  ));
}

// clickGenerate presses the generate action of the requirement step.
// clickGenerate 按下需求步骤的生成操作。
async function clickGenerate() {
  const button = [...host.querySelectorAll("button")].find((candidate) => candidate.textContent?.includes("composer.dedicated.generate"));
  expect(button).toBeDefined();
  await act(async () => button!.click());
}

it("drafts on the model's default thinking level and streams only its own thinking", async () => {
  let emit: (event: DedicatedPromptDraftProgressEvent) => void = () => undefined;
  const unsubscribe = vi.fn();
  vi.mocked(subscribeDedicatedPromptDraftProgress).mockImplementation(async (listener) => { emit = listener; return unsubscribe; });
  let finish: (response: DedicatedPromptDraftResponse) => void = () => undefined;
  vi.mocked(draftDedicatedPrompt).mockImplementation(() => new Promise((resolve) => { finish = resolve; }));
  await renderWizard(providers({
    options: [{ value: "low", label: "low" }, { value: "medium", label: "medium" }, { value: "high", label: "high" }],
    selectedEffort: "high",
    defaultEffort: "medium",
  }));
  expect(host.querySelector('[aria-label="composer.dedicated.thinkingLevel"]')).not.toBeNull();
  await clickGenerate();
  const request = vi.mocked(draftDedicatedPrompt).mock.calls[0]![0];
  expect(request.reasoningEffort).toBe("medium");
  expect(request.progressId).toBeTruthy();
  await act(async () => {
    emit({ progressId: "other", reasoningDelta: "foreign", outputDelta: "" });
    emit({ progressId: request.progressId!, reasoningDelta: "weighing tone", outputDelta: "You translate" });
  });
  expect(host.textContent).toContain("weighing tone");
  expect(host.textContent).toContain("You translate");
  expect(host.textContent).not.toContain("foreign");
  await act(async () => finish({ prompt: "You translate.", provider_id: "p", model_id: "m" }));
  expect(unsubscribe).toHaveBeenCalledTimes(1);
});

it("lets the service resolve the level when the model exposes nothing to choose", async () => {
  vi.mocked(subscribeDedicatedPromptDraftProgress).mockResolvedValue(() => undefined);
  vi.mocked(draftDedicatedPrompt).mockResolvedValue({ prompt: "You translate.", provider_id: "p", model_id: "m" });
  await renderWizard(providers({ options: [{ value: "fixed-on", label: "on" }], selectedEffort: "fixed-on", defaultEffort: null }));
  expect(host.querySelector('[aria-label="composer.dedicated.thinkingLevel"]')).toBeNull();
  await clickGenerate();
  expect(vi.mocked(draftDedicatedPrompt).mock.calls[0]![0].reasoningEffort).toBeUndefined();
});
