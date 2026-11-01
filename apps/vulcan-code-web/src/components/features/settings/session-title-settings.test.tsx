// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { SettingsActionProps } from "@/components/features/settings/settings-action-types";
import type { PublicAiInternalInferenceCandidate, PublicAiSettingsSnapshot } from "@/types";

// The translator returns keys, so assertions name controls by catalog key instead of wording.
// 翻译函数返回键名，使断言以目录键而非文案定位控件。
vi.mock("@/i18n", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/i18n")>();
  const t = (key: string) => key;
  return { ...actual, useI18n: () => ({ locale: "en-US", setLocale: () => undefined, t }) };
});
vi.mock("@/ui-kit/primitives/toast", () => ({ toast: { success: vi.fn() } }));

// Every service call is synthetic; nothing reaches a running service.
// 所有服务调用均为合成；不会访问运行中的服务。
const service = vi.hoisted(() => ({
  loadPublicAiSettings: vi.fn(),
  savePublicAiInferenceRoute: vi.fn(),
}));
vi.mock("@/service/settings-actions-service", () => service);

const { SessionTitleSettingsSection, sessionTitleCandidates } = await import("./session-title-settings");

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
});
afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.unstubAllGlobals();
  vi.clearAllMocks();
});

// candidate builds one conversation route in the given usage scope with the given efforts.
// candidate 构建一条指定用途范围与思考档位的对话路由。
function candidate(
  modelId: string,
  efforts: string[],
  usageScope = "internal_background",
): PublicAiInternalInferenceCandidate {
  return {
    providerDisplayName: "P", serviceEntryDisplayName: "S", connectionDisplayName: "C", modelDisplayName: modelId,
    route: {
      provider_id: "p", service_entry_id: "s", connection_id: "c", model_id: modelId,
      binding: { kind: "protocol", protocol_profile_id: "chat" },
      operation: "conversation_respond", usage_scope: usageScope,
    },
    catalogGeneration: 1, enabled: true, usageScopes: [usageScope],
    capabilities: { reasoning_control: "effort_selection", reasoning_efforts: efforts },
    overrides: {},
  } as unknown as PublicAiInternalInferenceCandidate;
}

// actions builds settings actions whose title switch save is observable.
// actions 构建可观察标题开关保存的设置动作。
function actions(onSaveSessionTitleGeneration: (enabled: boolean) => Promise<void>): SettingsActionProps {
  return { onSaveSessionTitleGeneration } as unknown as SettingsActionProps;
}

it("offers internal routes with the models that can turn thinking off first", () => {
  const thinking = candidate("thinking", ["low", "high"]);
  const quiet = candidate("quiet", ["none", "low"]);
  const foreground = candidate("foreground", ["none"], "foreground");
  expect(sessionTitleCandidates([thinking, quiet, foreground])).toEqual([quiet, thinking]);
});

it("saves the switch and hides the model rows while it is off", async () => {
  service.loadPublicAiSettings.mockResolvedValue({
    internalInferenceCandidates: [candidate("quiet", ["none"])],
    inferenceProfiles: {},
    inferenceRouteDiagnostics: [],
  } as unknown as PublicAiSettingsSnapshot);
  const save = vi.fn(async () => undefined);

  await act(async () => root.render(<SessionTitleSettingsSection actions={actions(save)} enabled />));
  expect(host.querySelector('[aria-label="settings.sessionTitle.route"]')).not.toBeNull();
  const toggle = host.querySelector<HTMLElement>('[aria-label="settings.sessionTitle.title"]');
  await act(async () => toggle?.click());
  expect(save).toHaveBeenCalledExactlyOnceWith(false);

  await act(async () => root.render(<SessionTitleSettingsSection actions={actions(save)} enabled={false} />));
  expect(host.querySelector('[aria-label="settings.sessionTitle.route"]')).toBeNull();
});
