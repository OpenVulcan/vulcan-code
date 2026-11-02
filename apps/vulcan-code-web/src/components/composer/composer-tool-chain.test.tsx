// @vitest-environment jsdom
// Behaviour of the session tool_chain switch: it saves at once before the first request and asks before breaking the
// context afterwards.
// 会话 tool_chain 开关的行为：首次请求前立即保存，之后先确认再打破上下文。
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

// The translator returns keys, so assertions name controls by catalog key instead of wording.
// 翻译函数返回键名，使断言以目录键而非文案定位控件。
vi.mock("@/i18n", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/i18n")>();
  const t = (key: string) => key;
  return { ...actual, useI18n: () => ({ locale: "en-US", setLocale: () => undefined, t }) };
});
vi.mock("@/ui-kit/primitives/toast", () => ({ toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() } }));

// Every service call is synthetic; nothing reaches a running service.
// 所有服务调用均为合成；不会访问运行中的服务。
const service = vi.hoisted(() => ({
  loadSessionToolChain: vi.fn(),
  saveSessionToolChain: vi.fn(),
}));
vi.mock("@/service/tool-chain-service", () => service);

const { ComposerToolChainBlock } = await import("./composer-tool-chain");

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
  document.body.innerHTML = "";
  vi.unstubAllGlobals();
  vi.clearAllMocks();
});

// button finds a button by its text, which is the catalog key here.
// button 按文本查找按钮，此处即目录键。
function button(label: string): HTMLButtonElement {
  const found = [...document.querySelectorAll<HTMLButtonElement>("button")].find((candidate) => candidate.textContent?.trim() === label);
  expect(found, `button ${label}`).toBeTruthy();
  return found!;
}

// mount renders the block for a session whose snapshot says whether requests were sent.
// mount 为一个会话渲染该区块，其快照说明是否已发送请求。
async function mount(requestsStarted: boolean) {
  service.loadSessionToolChain.mockResolvedValue({ enabled: true, followsSystem: true, requestsStarted });
  service.saveSessionToolChain.mockResolvedValue({ enabled: false, followsSystem: false, requestsStarted });
  await act(async () => root.render(<ComposerToolChainBlock conversationId="session-1" />));
}

it("saves at once before the session sent any request", async () => {
  await mount(false);
  await act(async () => button("toolChain.session.off").click());
  expect(service.saveSessionToolChain).toHaveBeenCalledWith("session-1", false);
  expect(document.body.textContent).not.toContain("toolChain.session.confirmTitle");
});

it("asks before breaking the context and saves only on confirm", async () => {
  await mount(true);
  await act(async () => button("toolChain.session.off").click());
  expect(service.saveSessionToolChain).not.toHaveBeenCalled();
  expect(document.body.textContent).toContain("toolChain.session.confirmTitle");
  await act(async () => button("toolChain.session.confirmCancel").click());
  expect(service.saveSessionToolChain).not.toHaveBeenCalled();

  await act(async () => button("toolChain.session.off").click());
  await act(async () => button("toolChain.session.confirm").click());
  expect(service.saveSessionToolChain).toHaveBeenCalledWith("session-1", false);
});

it("offers nothing to switch for a draft", async () => {
  await act(async () => root.render(<ComposerToolChainBlock conversationId={null} />));
  expect(service.loadSessionToolChain).not.toHaveBeenCalled();
  expect(document.body.textContent).toContain("toolChain.session.draft");
  expect(button("toolChain.session.off").disabled).toBe(true);
});
