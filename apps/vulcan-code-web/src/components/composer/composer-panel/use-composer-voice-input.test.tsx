// @vitest-environment jsdom
import * as React from "react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { transcribeVoiceInput } from "@/service/voice-input-service";
import { toast } from "@/ui-kit/primitives/toast";
import { TooltipProvider } from "@/ui-kit/primitives/tooltip";
import type { VoiceInputSummary } from "@/types";
import { useComposerVoiceInput } from "./use-composer-voice-input";
import { saveVoiceInputDevice } from "./voice-input-device";
import { encodeVoiceInputWav } from "./voice-input-wav";

vi.mock("@/i18n", () => ({ useI18n: () => ({ t: (key: string) => key }), formatMessage: (template: string) => template }));
vi.mock("@/ui-kit/primitives/toast", () => ({ toast: { error: vi.fn(), info: vi.fn(), success: vi.fn() } }));
vi.mock("@/service/voice-input-service", () => ({ transcribeVoiceInput: vi.fn(), loadVoiceInputSettings: vi.fn() }));
// The browser decoder is exercised independently of the push-to-talk lifecycle.
// 浏览器音频解码与按住说话的生命周期分开验证。
vi.mock("./voice-input-wav", () => ({
  encodeVoiceInputWav: vi.fn(async () => new Blob(["pcm"], { type: "audio/wav" })),
}));

const CONFIGURED: VoiceInputSummary = { configured: true, maxDurationSeconds: 120 };
// Sample CJK draft and transcript; the test checks how they join, not what they say.
// CJK 草稿与转写样例；测试只检查拼接方式，不检查内容。
const CJK_DRAFT = "你好";
const CJK_TRANSCRIPT = "世界";

// FakeRecorder stands in for the browser recorder; stopping flushes one chunk then reports the stop.
// FakeRecorder 代替浏览器录音器；停止时先交出一段数据再报告停止。
class FakeRecorder {
  static isTypeSupported(type: string) { return type === "audio/webm"; }
  state: "inactive" | "recording" = "inactive";
  mimeType: string;
  ondataavailable: ((event: { data: Blob }) => void) | null = null;
  onstop: (() => void) | null = null;
  constructor(_stream: unknown, options: { mimeType: string }) { this.mimeType = options.mimeType; }
  start() { this.state = "recording"; }
  stop() {
    this.state = "inactive";
    this.ondataavailable?.({ data: new Blob(["audio"], { type: this.mimeType }) });
    this.onstop?.();
  }
}

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
let now = 0;
let frames: FrameRequestCallback[] = [];
let state: { draft: string; busy: boolean };
const track = { stop: vi.fn() };
const getUserMedia = vi.fn();
const transcribe = vi.mocked(transcribeVoiceInput);

/** Render the actual hook beside a real textarea whose caret sits after the initial draft.
 * 在真实文本框旁渲染实际钩子，光标位于初始草稿末尾。 */
function Harness({ voiceInput, initial }: { voiceInput: VoiceInputSummary | null; initial: string }) {
  const [draft, setDraft] = React.useState(initial);
  const textareaRef = React.useRef<HTMLTextAreaElement | null>(null);
  const voice = useComposerVoiceInput({ voiceInput, disabled: false, textareaRef, setDraftInput: setDraft });
  state = { draft, busy: voice.busy };
  return (
    <TooltipProvider>
      <textarea onChange={(event) => setDraft(event.target.value)} ref={textareaRef} value={draft} />
      {voice.control}
    </TooltipProvider>
  );
}

async function render(voiceInput: VoiceInputSummary | null, initial = "Hello") {
  await act(async () => { root.render(<Harness initial={initial} voiceInput={voiceInput} />); });
  const textarea = host.querySelector("textarea")!;
  textarea.setSelectionRange(initial.length, initial.length);
  return { button: host.querySelector("button")!, textarea };
}

async function settle() {
  for (let index = 0; index < 5; index += 1) {
    await act(async () => { await new Promise((resolve) => setTimeout(resolve, 0)); });
  }
  await act(async () => { frames.splice(0).forEach((frame) => frame(0)); });
}

async function key(target: EventTarget, type: "keydown" | "keyup", value: string) {
  await act(async () => { target.dispatchEvent(new KeyboardEvent(type, { key: value, bubbles: true })); });
}

beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  now = 1_000;
  frames = [];
  vi.spyOn(Date, "now").mockImplementation(() => now);
  vi.stubGlobal("localStorage", memoryStorage());
  vi.stubGlobal("MediaRecorder", FakeRecorder);
  vi.stubGlobal("requestAnimationFrame", (frame: FrameRequestCallback) => { frames.push(frame); return frames.length; });
  getUserMedia.mockResolvedValue({ getTracks: () => [track] });
  Object.defineProperty(navigator, "mediaDevices", { configurable: true, value: { getUserMedia } });
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  vi.clearAllMocks();
});

it("records from the remembered microphone and from the system default otherwise", async () => {
  const { button } = await render(CONFIGURED);
  await key(button, "keydown", " ");
  await settle();
  await key(button, "keydown", "Escape");
  await settle();
  expect(getUserMedia).toHaveBeenLastCalledWith({ audio: true });

  saveVoiceInputDevice("usb-mic");
  await key(button, "keydown", " ");
  await settle();
  expect(getUserMedia).toHaveBeenLastCalledWith({ audio: { deviceId: { exact: "usb-mic" } } });
});

it("stays inert until a voice input target is configured", async () => {
  const { button } = await render({ configured: false, maxDurationSeconds: 120 });
  expect(button.getAttribute("aria-disabled")).toBe("true");
  await key(button, "keydown", " ");
  await settle();
  expect(getUserMedia).not.toHaveBeenCalled();
});

it("records while held, holds sending, and inserts the selected transcript at the caret", async () => {
  transcribe.mockResolvedValue({ text: "world" });
  const { button, textarea } = await render(CONFIGURED);
  await key(button, "keydown", " ");
  await settle();
  expect(button.getAttribute("data-state")).toBe("recording");
  expect(state.busy).toBe(true);

  now += 2_000;
  await key(button, "keyup", " ");
  await settle();

  expect(transcribe).toHaveBeenCalledTimes(1);
  expect(vi.mocked(encodeVoiceInputWav).mock.calls[0]![0].type).toBe("audio/webm");
  expect(transcribe.mock.calls[0]![0].type).toBe("audio/wav");
  expect(track.stop).toHaveBeenCalled();
  expect(state).toEqual({ draft: "Hello world", busy: false });
  expect([textarea.selectionStart, textarea.selectionEnd]).toEqual([6, 11]);
});

it("joins CJK transcripts without a separating space", async () => {
  transcribe.mockResolvedValue({ text: CJK_TRANSCRIPT });
  const { button } = await render(CONFIGURED, CJK_DRAFT);
  await key(button, "keydown", " ");
  await settle();
  now += 2_000;
  await key(button, "keyup", " ");
  await settle();
  expect(state.draft).toBe(`${CJK_DRAFT}${CJK_TRANSCRIPT}`);
});

it("discards the recording on Escape and on a short tap", async () => {
  const { button } = await render(CONFIGURED);
  await key(button, "keydown", " ");
  await settle();
  now += 2_000;
  await key(window, "keydown", "Escape");
  await settle();
  expect(state.busy).toBe(false);

  await key(button, "keydown", " ");
  await settle();
  now += 100;
  await key(button, "keyup", " ");
  await settle();

  expect(transcribe).not.toHaveBeenCalled();
  expect(toast.info).toHaveBeenCalledWith("composer.voice.tooShort");
  expect(state.draft).toBe("Hello");
});
