// @vitest-environment jsdom
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
  loadSubagentModels: vi.fn(),
  saveSubagentModels: vi.fn(),
  loadSessionSubagentModels: vi.fn(),
  saveSessionSubagentModels: vi.fn(),
}));
vi.mock("@/service/subagent-models-service", () => service);

const { SubagentModelsSettingsArea } = await import("./subagent-models-settings-area");
const { default: SubagentModelsSessionDialog } = await import("./subagent-models-session-dialog");

let host: HTMLDivElement;
let root: Root;
const first = { providerId: "openrouter", modelId: "vendor/first", note: "wide searches" };
const second = { providerId: "openai", modelId: "second", note: "" };

beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
});
afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  document.body.innerHTML = "";
  vi.unstubAllGlobals();
  vi.clearAllMocks();
});

// typeInto changes a React-controlled textarea the way a user edit does.
// typeInto 以用户编辑的方式修改受 React 控制的文本框。
function typeInto(element: HTMLTextAreaElement, value: string) {
  Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")!.set!.call(element, value);
  element.dispatchEvent(new Event("input", { bubbles: true }));
}

// button finds a button by its accessible label prefix, which is the catalog key here.
// button 按无障碍标签前缀查找按钮，此处即目录键。
function button(scope: ParentNode, label: string): HTMLButtonElement {
  const found = [...scope.querySelectorAll<HTMLButtonElement>("button")].find((candidate) => (
    candidate.getAttribute("aria-label")?.startsWith(label) || candidate.textContent?.trim() === label
  ));
  expect(found, `button ${label}`).toBeTruthy();
  return found!;
}

// Verifies the system list saves reordered, trimmed entries and refuses a note over the limit.
// 验证系统清单保存重排并去除首尾空白的条目，且拒绝超出限制的说明。
it("saves the reordered system list and blocks notes over the limit", async () => {
  service.loadSubagentModels.mockResolvedValue({ models: [first, second], maxNoteChars: 20, maxModels: 64 });
  service.saveSubagentModels.mockImplementation(async (models) => ({ models, maxNoteChars: 20, maxModels: 64 }));
  await act(async () => root.render(<SubagentModelsSettingsArea providers={[]} />));
  const save = host.querySelector<HTMLButtonElement>("[data-subagent-models-save]")!;
  expect(save.disabled).toBe(true);

  const cards = () => [...host.querySelectorAll("[data-sam-model]")].map((card) => card.getAttribute("data-sam-model"));
  expect(cards()).toEqual(["openrouter/vendor/first", "openai/second"]);
  // The first card's move-up is disabled, so the first enabled one belongs to the second card.
  // 第一张卡片的上移不可用，因此第一个可用的上移属于第二张卡片。
  const moveUp = [...host.querySelectorAll<HTMLButtonElement>("button[aria-label^='subagentModels.moveUp']")].find((candidate) => !candidate.disabled)!;
  await act(async () => moveUp.click());
  expect(cards()).toEqual(["openai/second", "openrouter/vendor/first"]);

  const note = host.querySelector<HTMLTextAreaElement>("[data-sam-note='openai/second']")!;
  await act(async () => typeInto(note, "x".repeat(21)));
  expect(save.disabled).toBe(true);
  await act(async () => typeInto(note, "  quick reads  "));
  expect(save.disabled).toBe(false);
  await act(async () => save.click());
  expect(service.saveSubagentModels).toHaveBeenCalledWith([{ ...second, note: "quick reads" }, first]);
  expect(save.disabled).toBe(true);

  await act(async () => button(host, "subagentModels.remove").click());
  expect(cards()).toEqual(["openrouter/vendor/first"]);
});

// Verifies a following session saves a copy of the system list as its own, and a custom one can restore the default.
// 验证跟随系统的会话将系统清单副本保存为自有清单，自定义会话可以恢复系统默认。
it("turns a following session custom on save and restores the system default", async () => {
  const onClose = vi.fn();
  service.loadSessionSubagentModels.mockResolvedValue({
    sessionId: "s1", models: null, systemModels: [first], maxNoteChars: 400, maxModels: 64,
  });
  service.saveSessionSubagentModels.mockResolvedValue({});
  await act(async () => root.render(<SubagentModelsSessionDialog conversationId="s1" onClose={onClose} providers={[]} />));
  expect(document.querySelector("[data-sam-follows]")).not.toBeNull();
  const save = document.querySelector<HTMLButtonElement>("[data-subagent-models-session-save]")!;
  expect(save.disabled).toBe(false);
  await act(async () => save.click());
  expect(service.saveSessionSubagentModels).toHaveBeenCalledWith("s1", [first]);
  expect(onClose).toHaveBeenCalledTimes(1);

  await act(async () => root.unmount());
  root = createRoot(host);
  service.loadSessionSubagentModels.mockResolvedValue({
    sessionId: "s1", models: [second], systemModels: [first], maxNoteChars: 400, maxModels: 64,
  });
  await act(async () => root.render(<SubagentModelsSessionDialog conversationId="s1" onClose={onClose} providers={[]} />));
  expect(document.querySelector("[data-sam-follows]")).toBeNull();
  expect(document.querySelector<HTMLButtonElement>("[data-subagent-models-session-save]")!.disabled).toBe(true);
  await act(async () => button(document, "subagentModels.session.restore").click());
  expect(service.saveSessionSubagentModels).toHaveBeenLastCalledWith("s1", null);
});
