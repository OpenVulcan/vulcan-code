// @vitest-environment jsdom
import * as React from "react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { loadVoiceInputCloudCatalog, transcribeVoiceInput } from "@/service/voice-input-service";
import { toast } from "@/ui-kit/primitives/toast";
import { TooltipProvider } from "@/ui-kit/primitives/tooltip";
import type { VoiceInputSummary } from "@/types";
import { useComposerVoiceInput } from "./use-composer-voice-input";
import { saveVoiceInputDevice } from "./voice-input-device";
import { saveVoiceInputShortcut } from "./voice-input-shortcut";
import { encodeVoiceInputWav } from "./voice-input-wav";

vi.mock("@/i18n", () => ({ useI18n: () => ({ t: (key: string) => key }), formatMessage: (template: string) => template }));
vi.mock("@/ui-kit/primitives/toast", () => ({ toast: { error: vi.fn(), info: vi.fn(), success: vi.fn() } }));
vi.mock("@/service/voice-input-service", () => ({
  transcribeVoiceInput: vi.fn(), loadVoiceInputSettings: vi.fn(), loadVoiceInputCloudCatalog: vi.fn(),
}));
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

// Dispatches a cancelable browser key event and returns it for ownership assertions.
// 派发可取消的浏览器按键事件，并返回事件用于断言键盘所有权。
async function key(target: EventTarget, type: "keydown" | "keyup", value: string, options?: KeyboardEventInit) {
  // Modifier and physical key data mirror a real shortcut event.
  // 修饰键与物理按键数据模拟真实的快捷键事件。
  const event = new KeyboardEvent(type, { key: value, bubbles: true, cancelable: true, ...options });
  await act(async () => { target.dispatchEvent(event); });
  return event;
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

it("enables the microphone after native sign-in only while the cloud route is eligible", async () => {
  // The initial workbench has no explicit target; signing in publishes an eligible account.
  // 初始工作台没有显式目标；登录后发布可用云端的账号。
  const unset = { configured: false, maxDurationSeconds: 120 };
  const { button } = await render(unset);
  vi.mocked(loadVoiceInputCloudCatalog).mockResolvedValue({
    status: "available", connection_id: "native", default_model_id: "site-asr",
    models: [{ id: "site-asr", display_name: "Default ASR", supported_languages: [] }],
    selection: { connection_id: "native", model_id: "site-asr" },
  });
  await render({ ...unset, cloudAccountId: "native" });
  await settle();
  await key(button, "keydown", " ");
  await settle();
  await key(button, "keydown", "Escape");
  await settle();
  expect(getUserMedia).toHaveBeenCalledTimes(1);

  await render(unset);
  await key(button, "keydown", " ");
  await settle();
  expect(getUserMedia).toHaveBeenCalledTimes(1);
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

it("records a held shortcut once, stops when its modifier is released, and preserves Ctrl+Enter", async () => {
  // Exercise shortcut matching after the composer has already mounted.
  // 验证输入区挂载后新保存的快捷键可以立即匹配。
  const { textarea } = await render(CONFIGURED);
  textarea.focus();
  saveVoiceInputShortcut("Control+Shift+KeyV");
  transcribe.mockResolvedValue({ text: "world" });
  expect((await key(textarea, "keydown", "Enter", { code: "Enter", ctrlKey: true })).defaultPrevented).toBe(false);
  expect((await key(textarea, "keydown", "V", { code: "KeyV", ctrlKey: true, shiftKey: true })).defaultPrevented).toBe(true);
  await settle();
  await key(textarea, "keydown", "V", { code: "KeyV", ctrlKey: true, shiftKey: true, repeat: true });
  expect(getUserMedia).toHaveBeenCalledTimes(1);
  now += 2_000;
  await key(window, "keyup", "Control", { code: "ControlLeft", shiftKey: true });
  await settle();
  expect(transcribe).toHaveBeenCalledTimes(1);
  expect(state).toEqual({ draft: "Hello world", busy: false });
});

it("yields to another editor and discards the held shortcut when the window loses focus", async () => {
  saveVoiceInputShortcut("F8");
  // Simulate a focused workbench editor outside the composer.
  // 模拟输入区之外当前具有焦点的工作台编辑器。
  const otherEditor = document.createElement("input");
  host.before(otherEditor);
  try {
    // The mounted composer becomes eligible again when focus returns to its textarea.
    // 焦点回到输入区文本框后，已挂载的输入区再次可以使用快捷键。
    const { textarea } = await render(CONFIGURED);
    otherEditor.focus();
    expect((await key(otherEditor, "keydown", "F8", { code: "F8" })).defaultPrevented).toBe(false);
    expect(getUserMedia).not.toHaveBeenCalled();
    textarea.focus();
    await key(textarea, "keydown", "F8", { code: "F8" });
    await settle();
    expect(state.busy).toBe(true);
    now += 2_000;
    await act(async () => { window.dispatchEvent(new Event("blur")); });
    await settle();
    expect(state.busy).toBe(false);
    expect(transcribe).not.toHaveBeenCalled();
  } finally {
    otherEditor.remove();
  }
});
