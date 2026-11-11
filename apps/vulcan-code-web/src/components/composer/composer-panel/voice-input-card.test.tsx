// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { loadVoiceInputCloudCatalog, loadVoiceInputSettings } from "@/service/voice-input-service";
import type { VoiceInputSettingsSnapshot } from "@/types";
import { VoiceInputCard } from "./voice-input-card";
import { readVoiceInputDevice, saveVoiceInputDevice } from "./voice-input-device";

vi.mock("@/i18n", () => ({ useI18n: () => ({ t: (key: string) => key }), formatMessage: (template: string) => template }));
vi.mock("@/service/voice-input-service", async (original) => ({
  ...await original<typeof import("@/service/voice-input-service")>(),
  loadVoiceInputSettings: vi.fn(),
  loadVoiceInputCloudCatalog: vi.fn(),
}));

const SETTINGS: VoiceInputSettingsSnapshot = {
  disabled: false,
  selection: { connection_id: "openai-main", target_id: "gpt-transcribe" },
  candidates: [{
    connectionId: "openai-main", connectionDisplayName: "OpenAI main", providerId: "openai",
    targetId: "gpt-transcribe", displayName: "GPT Transcribe", parameters: [],
  }],
  maxDurationSeconds: 120,
};

// memoryStorage is a fresh per-test stand-in for the browser store that keeps the microphone choice.
// memoryStorage 是每个测试全新的浏览器存储替身，用于保存麦克风选择。
function memoryStorage() {
  const values = new Map<string, string>();
  return {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => { values.set(key, value); },
    removeItem: (key: string) => { values.delete(key); },
  };
}

let root: Root;
let host: HTMLDivElement;
const enumerateDevices = vi.fn();

/** Render the card and let the settings read and device listing resolve.
 * 渲染卡片，并等待设置读取与设备列举完成。 */
async function render(configured: boolean, onConfigure = vi.fn()) {
  await act(async () => {
    root.render(<VoiceInputCard configured={configured} maxDurationSeconds={120} onConfigure={onConfigure} />);
  });
  await act(async () => { await new Promise((resolve) => setTimeout(resolve, 0)); });
  return { onConfigure, radios: () => Array.from(host.querySelectorAll<HTMLButtonElement>("[role=radio]")) };
}

beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  vi.stubGlobal("localStorage", memoryStorage());
  vi.mocked(loadVoiceInputSettings).mockResolvedValue(SETTINGS);
  enumerateDevices.mockResolvedValue([
    { kind: "audioinput", deviceId: "default", label: "Default" },
    { kind: "audioinput", deviceId: "usb-mic", label: "USB Mic" },
    { kind: "audioinput", deviceId: "built-in", label: "" },
    { kind: "videoinput", deviceId: "camera", label: "Camera" },
  ]);
  Object.defineProperty(navigator, "mediaDevices", {
    configurable: true,
    value: { enumerateDevices, addEventListener: vi.fn(), removeEventListener: vi.fn() },
  });
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
  vi.unstubAllGlobals();
  vi.clearAllMocks();
});

it("shows the saved model and lists microphones without browser aliases", async () => {
  const { radios } = await render(true);
  expect(host.textContent).toContain(SETTINGS.candidates[0]!.displayName);
  expect(host.textContent).toContain(SETTINGS.candidates[0]!.connectionDisplayName);
  expect(radios().map((radio) => radio.textContent)).toEqual([
    "composer.voice.systemDefault", "USB Mic", "composer.voice.micFallback",
  ]);
  expect(radios()[0]!.getAttribute("aria-checked")).toBe("true");
  expect(host.textContent).toContain("composer.voice.micNamesHidden");
  expect(host.textContent).toContain("composer.voice.configure");
});

it("remembers the picked microphone and returns to the system default", async () => {
  const { radios } = await render(true);
  await act(async () => { radios()[1]!.click(); });
  expect(readVoiceInputDevice()).toBe("usb-mic");
  expect(radios()[1]!.getAttribute("aria-checked")).toBe("true");

  await act(async () => { radios()[0]!.click(); });
  expect(readVoiceInputDevice()).toBeNull();
});

it("flags a remembered microphone that is no longer connected", async () => {
  saveVoiceInputDevice("unplugged");
  const { radios } = await render(true);
  const missing = radios().at(-1)!;
  expect(missing.textContent).toBe("composer.voice.micMissing");
  expect(missing.getAttribute("aria-checked")).toBe("true");
});

it("offers quick setup when no model is selected", async () => {
  vi.mocked(loadVoiceInputSettings).mockResolvedValue({ ...SETTINGS, selection: null, disabled: true });
  const { onConfigure } = await render(false);
  expect(host.textContent).toContain("composer.voice.modelNone");
  const setup = Array.from(host.querySelectorAll("button")).find((button) => button.textContent === "composer.voice.configure")!;
  await act(async () => { setup.click(); });
  expect(onConfigure).toHaveBeenCalledTimes(1);
});

it("shows the effective cloud model when the saved publication is retired", async () => {
  // The saved cloud publication has disappeared while the native account is unchanged.
  // 已保存的云端发布条目已下架，原生账号保持一致。
  vi.mocked(loadVoiceInputSettings).mockResolvedValue({
    ...SETTINGS, selection: { openvulcan_cloud: { connection_id: "native", model_id: "retired-asr" } },
  });
  // Core reports the current replacement as the effective model.
  // Core 返回当前替代模型作为生效模型。
  const replacement = { id: "current-asr", display_name: "Replacement ASR", supported_languages: [] };
  vi.mocked(loadVoiceInputCloudCatalog).mockResolvedValue({
    status: "available", connection_id: "native", default_model_id: replacement.id,
    models: [replacement], selection: { connection_id: "native", model_id: replacement.id },
  });
  await render(true);
  expect(host.textContent).toContain(replacement.display_name);
  expect(host.textContent).toContain("settings.voiceInput.cloudTitle");
});
