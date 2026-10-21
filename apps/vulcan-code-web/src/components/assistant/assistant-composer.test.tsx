// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { AssistantComposer } from "./assistant-conversation";

// Only the text catalog and the service boundary are synthetic.
// 仅文案目录与服务边界为合成。
vi.mock("@/i18n", async (importOriginal) => ({
  ...await importOriginal<typeof import("@/i18n")>(),
  useI18n: () => ({ t: (key: string) => key }),
}));

const submitConversationPrompt = vi.fn<(...args: unknown[]) => Promise<unknown>>();
vi.mock("@/service/conversation-actions-service", () => ({
  fetchModelAttachmentCapabilities: vi.fn(async () => []),
  reconcileComposerAttachmentDraftLease: vi.fn(async () => undefined),
  stageComposerAttachmentBytes: vi.fn(),
  stopConversationRun: vi.fn(),
  submitConversationPrompt: (...args: unknown[]) => submitConversationPrompt(...args),
}));

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
  submitConversationPrompt.mockReset();
  vi.unstubAllGlobals();
});

const input = () => host.querySelector<HTMLTextAreaElement>("textarea")!;

// type replaces the input's value the way a keystroke does, so React sees the change.
// type 以按键的方式替换输入框的值，使 React 观察到变化。
async function type(value: string) {
  const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")!.set!;
  await act(async () => {
    setter.call(input(), value);
    input().dispatchEvent(new Event("input", { bubbles: true }));
  });
}

async function pressEnter() {
  await act(async () => {
    input().dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  });
}

// sendPending renders the composer, sends `text`, and returns the receipt still pending.
// sendPending 渲染输入区并发送 `text`，返回仍在等待的回执。
async function sendPending(text: string) {
  let resolve!: (ack: unknown) => void;
  submitConversationPrompt.mockReturnValue(new Promise((done) => { resolve = done; }));
  const assistant = { id: "draft-assistant", name: "Helper", projectId: null, projectName: null, running: false };
  await act(async () => root.render(
    <AssistantComposer assistant={assistant} running={false} sessionConfig={null} onOpenMedia={() => undefined} />,
  ));
  await type(text);
  await pressEnter();
  expect(submitConversationPrompt).toHaveBeenCalledOnce();
  return async () => { await act(async () => resolve({ accepted: true })); };
}

describe("assistant composer", () => {
  it("clears only what was sent when the receipt arrives", async () => {
    const receive = await sendPending("first");
    await receive();
    expect(input().value).toBe("");
  });

  it("keeps a draft typed while the receipt was pending", async () => {
    const receive = await sendPending("first");
    await type("second");
    await receive();
    expect(input().value).toBe("second");
  });
});
