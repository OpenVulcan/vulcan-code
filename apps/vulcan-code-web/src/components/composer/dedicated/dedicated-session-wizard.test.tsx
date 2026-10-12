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
import { DedicatedPromptGeneratorDialog } from "./dedicated-prompt-generator-dialog";
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

// renderGenerator mounts the open prompt dialog with a filled description.
// renderGenerator 以已填写的描述挂载已打开的提示词弹窗。
async function renderGenerator(routes: ProviderSummary[], onApply = vi.fn()) {
  await act(async () => root.render(
    <TooltipProvider>
      <DedicatedPromptGeneratorDialog
        currentPrompt=""
        onApply={onApply}
        onOpenChange={vi.fn()}
        onRequirementChange={vi.fn()}
        open
        providers={routes}
        requirement="translate"
      />
    </TooltipProvider>,
  ));
}

// clickButton presses the first button whose label contains `label`; dialogs render in a portal.
// clickButton 按下标签包含 `label` 的第一个按钮；弹窗渲染在传送门中。
async function clickButton(label: string) {
  const button = [...document.body.querySelectorAll("button")].find((candidate) => candidate.textContent?.includes(label));
  expect(button, `button ${label}`).toBeDefined();
  await act(async () => button!.click());
}

it("drafts on the model's default thinking level and streams only its own thinking", async () => {
  let emit: (event: DedicatedPromptDraftProgressEvent) => void = () => undefined;
  const unsubscribe = vi.fn();
  vi.mocked(subscribeDedicatedPromptDraftProgress).mockImplementation(async (listener) => { emit = listener; return unsubscribe; });
  let finish: (response: DedicatedPromptDraftResponse) => void = () => undefined;
  vi.mocked(draftDedicatedPrompt).mockImplementation(() => new Promise((resolve) => { finish = resolve; }));
  await renderGenerator(providers({
    options: [{ value: "low", label: "low" }, { value: "medium", label: "medium" }, { value: "high", label: "high" }],
    selectedEffort: "high",
    defaultEffort: "medium",
  }));
  expect(document.body.querySelector('[aria-label="composer.dedicated.thinkingLevel"]')).not.toBeNull();
  await clickButton("composer.dedicated.generate");
  const request = vi.mocked(draftDedicatedPrompt).mock.calls[0]![0];
  expect(request.reasoningEffort).toBe("medium");
  expect(request.progressId).toBeTruthy();
  await act(async () => {
    emit({ progressId: "other", reasoningDelta: "foreign", outputDelta: "" });
    emit({ progressId: request.progressId!, reasoningDelta: "weighing tone", outputDelta: "You translate" });
  });
  expect(document.body.textContent).toContain("weighing tone");
  expect(document.body.textContent).toContain("You translate");
  expect(document.body.textContent).not.toContain("foreign");
  await act(async () => finish({ prompt: "You translate.", provider_id: "p", model_id: "m" }));
  expect(unsubscribe).toHaveBeenCalledTimes(1);
});

it("lets the service resolve the level when the model exposes nothing to choose", async () => {
  vi.mocked(subscribeDedicatedPromptDraftProgress).mockResolvedValue(() => undefined);
  vi.mocked(draftDedicatedPrompt).mockResolvedValue({ prompt: "You translate.", provider_id: "p", model_id: "m" });
  await renderGenerator(providers({ options: [{ value: "fixed-on", label: "on" }], selectedEffort: "fixed-on", defaultEffort: null }));
  expect(document.body.querySelector('[aria-label="composer.dedicated.thinkingLevel"]')).toBeNull();
  await clickButton("composer.dedicated.generate");
  expect(vi.mocked(draftDedicatedPrompt).mock.calls[0]![0].reasoningEffort).toBeUndefined();
});

it("hands the generated draft back only when the user applies it", async () => {
  vi.mocked(subscribeDedicatedPromptDraftProgress).mockResolvedValue(() => undefined);
  vi.mocked(draftDedicatedPrompt).mockResolvedValue({ prompt: "You translate.", provider_id: "p", model_id: "m" });
  const onApply = vi.fn();
  await renderGenerator(providers({ options: [], selectedEffort: null, defaultEffort: null } as unknown as ReasoningSelectionSummary), onApply);
  await clickButton("composer.dedicated.generate");
  expect(onApply).not.toHaveBeenCalled();
  await clickButton("composer.dedicated.apply");
  expect(onApply).toHaveBeenCalledWith("You translate.");
});

it("creates with the recommended answer mode and style, not the global ones, and the shown thinking level", async () => {
  const onSubmitPrompt = vi.fn(() => Promise.resolve(true));
  const state: DedicatedDraftState = { ...INITIAL_DEDICATED_DRAFT, kind: "dedicated", step: "memory", prompt: "You translate." };
  const catalog = {
    defaultId: "plain",
    selectedId: "concise",
    entries: [
      { id: "plain", labelZh: "plain", labelEn: "plain", constraintLevel: "", promptCharCount: 1 },
      { id: "concise", labelZh: "concise", labelEn: "concise", constraintLevel: "", promptCharCount: 1 },
      { id: "unrestricted", labelZh: "unrestricted", labelEn: "unrestricted", constraintLevel: "", promptCharCount: 0 },
    ],
  } as unknown as OutputStyleCatalogSummary;
  await act(async () => root.render(
    <TooltipProvider>
      <DedicatedSessionWizard
        draft={{ draftId: "d", projectId: "project", projects: [], submissionPending: false, onChangeProject: vi.fn() }}
        onBusyChange={vi.fn()}
        onChange={vi.fn()}
        onSelectWorkbenchModel={vi.fn()}
        onSubmitPrompt={onSubmitPrompt}
        outputStyleCatalog={catalog}
        providers={providers({
          options: [{ value: "low", label: "low" }, { value: "high", label: "high" }],
          selectedEffort: "high",
          defaultEffort: "low",
        })}
        state={state}
      />
    </TooltipProvider>,
  ));
  await clickButton("composer.dedicated.lockNow");
  const submission = (onSubmitPrompt.mock.calls[0] as unknown as [{ sessionSettings: Record<string, unknown> }])[0];
  expect(submission.sessionSettings.answerMode).toBe("standard");
  expect(submission.sessionSettings.outputStyleId).toBe("unrestricted");
  // The draft follows the composer rule: the model's selected level is shown and submitted.
  // 草稿遵循输入区规则：显示并提交模型的选中档位。
  expect((onSubmitPrompt.mock.calls[0] as unknown as [{ reasoningEffort: string | null }])[0].reasoningEffort).toBe("high");
});
