// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { toast } from "@/ui-kit/primitives/toast";
import { ComposerPanel } from "./composer-panel";
import { DEFAULT_THINKING_DISPLAY_MODE } from "@/components/conversation/thinking-display";
import { ComposerEnterSendSettingsSection } from "@/components/features/settings/composer-enter-send-settings";
import { publicAiSettingsWithInvalidDefaultFixture } from "@/test-fixtures/public-ai-settings";
import type { ComposerAttachmentSummary, PendingPromptSummary, PublicAiSessionProviderToolSettings } from "@/types";

// transport replaces only external service boundaries; composer controls and all draft hooks remain real.
// transport 仅替换外部服务边界；输入区控件和全部草稿 Hook 均使用真实实现。
const transport = vi.hoisted(() => ({ read: vi.fn(), write: vi.fn(), loadSettings: vi.fn(), loadSession: vi.fn() }));
vi.mock("@/service/conversation-session-drafts-v1", () => ({ sessionDraftClientV1: transport }));
vi.mock("@/service/settings-actions-service", () => ({
  loadPublicAiSettings: transport.loadSettings,
  loadPublicAiSessionProviderToolSelections: transport.loadSession,
  savePublicAiSessionProviderToolSelections: vi.fn(),
  rebuildPublicAiSessionProviderToolSelections: vi.fn(),
}));
vi.mock("@/service/conversation-actions-service", () => ({
  stageComposerAttachmentPreviewBytes: vi.fn(),
  cancelConversationSubmission: vi.fn().mockResolvedValue(undefined),
  discardComposerAttachmentPlans: vi.fn().mockResolvedValue(undefined),
  planComposerAttachments: vi.fn().mockResolvedValue([]),
  prepareComposerAttachment: vi.fn(),
  reconcileComposerAttachmentDraftLease: vi.fn().mockResolvedValue(false),
}));
// translate keeps hook dependencies stable while exposing semantic labels for DOM interaction.
// translate 保持 Hook 依赖稳定，同时暴露用于 DOM 交互的语义标签。
const translate = (key: string) => key;
vi.mock("@/i18n", () => ({ useI18n: () => ({ locale: "en-US", t: translate }) }));

// root and host isolate each complete composer instance.
// root 与 host 隔离每个完整输入区实例。
let root: Root;
let host: HTMLDivElement;

/** Creates complete typed composer props without a service process or real conversation.
 * 创建完整强类型输入区参数，不启动服务进程或真实会话。
 * Parameters: overrides selects the precise test scenario; returns independently owned callbacks and data.
 * 参数：overrides 选择精确测试场景；返回独立持有的回调和数据。
 */
function composerProps(overrides: Partial<React.ComponentProps<typeof ComposerPanel>> = {}): React.ComponentProps<typeof ComposerPanel> {
  return {
    attachmentCapabilities: [{ kind: "file", label: "Files", enabled: true, native: false, bridged: true, accept: "*/*" }],
    projectId: "panel-test-project", conversationId: "panel-test-session", contextUsage: null, defaultAnswerMode: "standard",
    slashCommands: [], pendingPrompts: [], sessionGoal: null, sessionTodolist: null, sessionReviewSummary: null,
    sessionPlanMode: null, sessionVmmPolicy: null, sessionOutputStyle: null,
    outputStyleCatalog: { defaultId: "default", selectedId: "default", entries: [] },
    contextCompactionWindowNodes: [], sessionContextWindowSelection: null, customContextWindowMinTokens: 100_000,
    vmmAvailable: false, isConversationRunning: true, thinkingDisplayMode: DEFAULT_THINKING_DISPLAY_MODE,
    providers: [], reasoningSelection: null,
    onSelectWorkbenchModel: vi.fn().mockResolvedValue(true),
    onListWorkbenchModelSecondaryRoutes: vi.fn().mockResolvedValue([]),
    onSelectWorkbenchModelSecondaryRoute: vi.fn().mockResolvedValue(true),
    onSelectWorkbenchReasoningEffort: vi.fn().mockResolvedValue(true),
    onSlashCommand: vi.fn().mockReturnValue({ handled: false }), onOpenKnowledge: vi.fn(),
    onSubmitPrompt: vi.fn().mockResolvedValue(true),
    onSaveSessionGoal: vi.fn().mockResolvedValue(true), onResumeSessionGoal: vi.fn().mockResolvedValue(true),
    onClearSessionGoal: vi.fn().mockResolvedValue(true), onUpdateSessionGoalStatus: vi.fn().mockResolvedValue(true),
    onSetSessionPlanMode: vi.fn().mockResolvedValue(true), onSetSessionVmmPolicy: vi.fn().mockResolvedValue(true),
    onSetSessionOutputStyle: vi.fn().mockResolvedValue(true), onSetSessionContextCompactionWindow: vi.fn().mockResolvedValue(true),
    onStopConversationRun: vi.fn().mockResolvedValue(true), onStageAttachmentBytes: vi.fn().mockResolvedValue(null),
    resolveMediaSourceUrl: (path) => path,
    onEditPendingPrompt: vi.fn().mockResolvedValue(null), onInsertPendingPrompt: vi.fn().mockResolvedValue(undefined),
    onRemovePendingPrompt: vi.fn().mockResolvedValue(undefined), onReorderPendingPrompts: vi.fn().mockResolvedValue(undefined),
    onThinkingDisplayModeChange: vi.fn(),
    ...overrides,
  };
}

/** Mounts the real composer after the exact service draft and capability result settle.
 * 在精确服务草稿与能力结果结束后挂载真实输入区。
 * Parameter: props supplies the full scenario; returns after React commits effects.
 * 参数：props 提供完整场景；在 React 提交副作用后返回。
 */
async function mount(props: React.ComponentProps<typeof ComposerPanel>): Promise<void> {
  await act(async () => root.render(<ComposerPanel {...props} />));
}

/** Dispatches the actual textarea key event without invoking internal callbacks.
 * 分派真实文本框键盘事件，不直接调用内部回调。
 * Parameter: init adds modifiers or composition state; returns the dispatched event for default-action assertions.
 * 参数：init 添加修饰键或输入法状态；返回事件供默认行为断言使用。
 */
async function pressEnter(init: KeyboardEventInit = {}): Promise<KeyboardEvent> {
  // event follows the same bubbling path used by a real keyboard submission.
  // event 遵循真实键盘提交使用的同一冒泡路径。
  const event = new KeyboardEvent("keydown", { key: "Enter", code: "Enter", bubbles: true, cancelable: true, ...init });
  await act(async () => { host.querySelector("textarea")!.dispatchEvent(event); });
  return event;
}

beforeEach(() => {
  vi.clearAllMocks();
  // stored isolates browser preferences without relying on Node's optional localStorage.
  // stored 隔离浏览器偏好，无需依赖 Node 的可选 localStorage。
  const stored = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => stored.get(key) ?? null,
    setItem: (key: string, value: string) => { stored.set(key, value); },
  });
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  Element.prototype.scrollIntoView = vi.fn();
  transport.read.mockImplementation(async (sessionId: string) => ({ sessionId, text: "", attachments: [] }));
  transport.write.mockImplementation(async (sessionId: string, text: string, attachments: ComposerAttachmentSummary[]) => ({ sessionId, text, attachments }));
  // A failed capability read is a declared production send barrier, independent of stop eligibility.
  // 能力读取失败是明确的生产发送屏障，与停止资格无关。
  transport.loadSettings.mockRejectedValue(new Error("synthetic capability read failure"));
  transport.loadSession.mockResolvedValue(null);
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); vi.restoreAllMocks(); });

it("blocks Enter exactly like the disabled send button when capabilities are unavailable during a run", async () => {
  transport.read.mockImplementation(async (sessionId: string) => ({ sessionId, text: "queued next message", attachments: [] }));
  // props records external admission without mocking the send controller or keyboard handler.
  // props 记录外部准入，不替换发送控制器或键盘处理器。
  const props = composerProps();
  await mount(props);
  expect(host.querySelector<HTMLButtonElement>('[aria-label="composer.aria.send"]')?.disabled).toBe(true);
  await pressEnter();
  expect(props.onSubmitPrompt).not.toHaveBeenCalled();
  expect(host.querySelector<HTMLTextAreaElement>("textarea")?.value).toBe("queued next message");
});

it("stops an empty running conversation while an accepted file is still staging", async () => {
  // resolveStage settles only the synthetic file staging after the stop assertion.
  // resolveStage 仅在停止断言后结束合成文件暂存。
  let resolveStage!: (attachment: ComposerAttachmentSummary | null) => void;
  const props = composerProps({ onStageAttachmentBytes: vi.fn(() => new Promise<ComposerAttachmentSummary | null>((resolve) => { resolveStage = resolve; })) });
  await mount(props);
  // fileInput exercises the real file selection handler and its synchronous staging barrier.
  // fileInput 验证真实文件选择处理器及其同步暂存屏障。
  const fileInput = host.querySelector<HTMLInputElement>('input[type="file"]')!;
  Object.defineProperty(fileInput, "files", { configurable: true, value: [new File(["fixture"], "fixture.txt", { type: "text/plain" })] });
  await act(async () => { fileInput.dispatchEvent(new Event("change", { bubbles: true })); });
  expect(host.querySelector('[role="status"]')?.textContent).toContain("composer.attachment.stagingPending");
  // stop is visibly enabled even though the unsent file has not reached the staged attachment list.
  // stop 可见且启用，但未发送文件还没有进入已暂存附件列表。
  const stop = host.querySelector<HTMLButtonElement>('[aria-label="composer.aria.stop"]')!;
  expect(stop.disabled).toBe(false);
  await act(async () => stop.click());
  await act(async () => resolveStage(null));
  expect(props.onStopConversationRun).toHaveBeenCalledTimes(1);
  expect(props.onSubmitPrompt).not.toHaveBeenCalled();
});

it("keeps stopping available during a pending queue-to-draft replacement", async () => {
  // resolveEdit controls the external queue edit result while the real draft gate remains held.
  // resolveEdit 在真实草稿门控被持有期间控制外部队列编辑结果。
  let resolveEdit!: (entry: PendingPromptSummary | null) => void;
  const entry: PendingPromptSummary = { id: "pending-test", prompt: "queued target", preview: "queued target", attachments: [], state: "ready", createdAt: "0" };
  const props = composerProps({ pendingPrompts: [entry], onEditPendingPrompt: vi.fn(() => new Promise<PendingPromptSummary | null>((resolve) => { resolveEdit = resolve; })) });
  await mount(props);
  // edit is resolved by the accessible label rendered by the production queue strip.
  // edit 通过生产队列条实际渲染的无障碍标签定位。
  const edit = host.querySelector<HTMLButtonElement>('[aria-label="pendingPrompt.edit"]')!;
  expect(edit).not.toBeNull();
  await act(async () => edit.click());
  expect(host.querySelector<HTMLTextAreaElement>("textarea")?.disabled).toBe(true);
  await act(async () => host.querySelector<HTMLButtonElement>('[aria-label="composer.aria.stop"]')!.click());
  await act(async () => resolveEdit(entry));
  expect(props.onStopConversationRun).toHaveBeenCalledTimes(1);
  expect(props.onSubmitPrompt).not.toHaveBeenCalled();
});

it("keeps Shift+Enter and IME confirmation out of the submission path", async () => {
  // props observes both send and stop to prove that neither keyboard action escapes its editing role.
  // props 同时观察发送和停止，证明两类键盘操作都不会越过编辑职责。
  const props = composerProps();
  await mount(props);
  expect((await pressEnter({ shiftKey: true })).defaultPrevented).toBe(false);
  expect((await pressEnter({ isComposing: true })).defaultPrevented).toBe(false);
  expect(props.onSubmitPrompt).not.toHaveBeenCalled();
  expect(props.onStopConversationRun).not.toHaveBeenCalled();
});

it.each([
  [null, false, undefined],
  [null, true, true],
  ["insert", false, true],
  ["insert", true, undefined],
] as const)("routes Enter with preference %s and Ctrl %s to the selected delivery action", async (preference, ctrlKey, insertion) => {
  if (preference === "insert") {
    await act(async () => root.render(<ComposerEnterSendSettingsSection />));
    await act(async () => host.querySelector<HTMLButtonElement>('[role="combobox"]')!.click());
    // alternative is the only unselected option in the real settings control.
    // alternative 是实际设置控件中唯一未选中的选项。
    const alternative = document.querySelector<HTMLElement>('[role="option"][aria-selected="false"]')!;
    await act(async () => alternative.click());
  }
  transport.loadSettings.mockResolvedValue(publicAiSettingsWithInvalidDefaultFixture());
  transport.loadSession.mockResolvedValue({
    enabledCapabilities: [], selections: {}, extensionTargetSets: {}, extensionModes: {},
    mediaUnderstandingSelections: {}, conflicts: [], requestsStarted: true,
  } satisfies PublicAiSessionProviderToolSettings);
  transport.read.mockImplementation(async (sessionId: string) => ({ sessionId, text: "next prompt", attachments: [] }));
  // props observes the actual submission while keeping draft persistence and key routing active.
  // props 观察真实提交，并保留草稿持久化和按键路由。
  const props = composerProps();
  await mount(props);
  expect((await pressEnter({ ctrlKey })).defaultPrevented).toBe(true);
  expect(props.onSubmitPrompt).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({
    prompt: "next prompt",
  }));
  expect(vi.mocked(props.onSubmitPrompt).mock.calls[0]?.[0].insertIntoCurrentTurn).toBe(insertion);
});

it("keeps an empty Ctrl+Enter and repeated Enter from stopping the active turn", async () => {
  // props exposes both effects of the key routing while the draft is empty.
  // props 在草稿为空时观察按键路由的两种副作用。
  const props = composerProps();
  await mount(props);
  await pressEnter({ ctrlKey: true });
  await pressEnter({ repeat: true });
  expect(props.onSubmitPrompt).not.toHaveBeenCalled();
  expect(props.onStopConversationRun).not.toHaveBeenCalled();
});

it("returns keyboard focus to the actual textarea after confirmed removal of its final attachment", async () => {
  // attachment comes from the exact persisted draft read; all removal and lease hooks remain production code.
  // attachment 来自精确持久草稿读取；全部移除与租约 Hook 保持生产实现。
  const attachment: ComposerAttachmentSummary = { assetId: "focus-file", kind: "file", fileName: "notes.txt", localPath: "synthetic/notes.txt", mimeType: "text/plain" };
  transport.read.mockImplementation(async (sessionId: string) => ({ sessionId, text: "draft remains", attachments: [attachment] }));
  await mount(composerProps({ isConversationRunning: false }));
  // remove is the real visible action; the assertion waits for cancellation and exact-set lease reconciliation.
  // remove 是真实可见操作；断言等待取消与精确集合租约协调。
  const remove = host.querySelector<HTMLButtonElement>(".vk-attach-remove")!;
  expect(remove).not.toBeNull();
  await act(async () => { remove.focus(); remove.click(); });
  expect(host.querySelector(".vk-attach")).toBeNull();
  expect(document.activeElement).toBe(host.querySelector("textarea"));
  expect(host.querySelector<HTMLTextAreaElement>("textarea")?.value).toBe("draft remains");
});

it("deduplicates stop clicks and Enter while preserving a failed stop for explicit retry", async () => {
  // resolveStop retains the first stop acknowledgement while duplicate UI actions arrive.
  // resolveStop 在重复界面操作到达期间保留第一次停止确认。
  let resolveStop!: (stopped: boolean) => void;
  const props = composerProps({ onStopConversationRun: vi.fn()
    .mockImplementationOnce(() => new Promise<boolean>((resolve) => { resolveStop = resolve; }))
    .mockResolvedValue(true) });
  const feedback = vi.spyOn(toast, "error");
  await mount(props);
  const stop = host.querySelector<HTMLButtonElement>('[aria-label="composer.aria.stop"]')!;
  await act(async () => { stop.click(); stop.click(); });
  await pressEnter();
  expect(props.onStopConversationRun).toHaveBeenCalledTimes(1);
  expect(stop.disabled).toBe(true);
  expect(stop.getAttribute("aria-busy")).toBe("true");
  await act(async () => resolveStop(false));
  expect(feedback).toHaveBeenCalledWith("composer.error.stopCurrentFailed");
  expect(stop.disabled).toBe(false);
  await act(async () => stop.click());
  expect(props.onStopConversationRun).toHaveBeenCalledTimes(2);
  expect(transport.write).not.toHaveBeenCalled();
});

it("keeps a new conversation's pending stop locked when the old conversation finishes", async () => {
  // finishA and finishB resolve independently owned backend stops across visible conversation changes.
  // finishA 与 finishB 在可见会话变更之间结束各自独立归属的后端停止请求。
  let finishA!: (stopped: boolean) => void;
  let finishB!: (stopped: boolean) => void;
  const propsA = composerProps({ onStopConversationRun: vi.fn(() => new Promise<boolean>((resolve) => { finishA = resolve; })) });
  const propsB = composerProps({ conversationId: "panel-test-session-b", onStopConversationRun: vi.fn(() => new Promise<boolean>((resolve) => { finishB = resolve; })) });
  await mount(propsA);
  await act(async () => host.querySelector<HTMLButtonElement>('[aria-label="composer.aria.stop"]')!.click());
  await mount(propsB);
  expect(host.querySelector<HTMLButtonElement>('[aria-label="composer.aria.stop"]')?.disabled).toBe(false);
  await act(async () => host.querySelector<HTMLButtonElement>('[aria-label="composer.aria.stop"]')!.click());
  await act(async () => finishA(true));
  expect(host.querySelector<HTMLButtonElement>('[aria-label="composer.aria.stop"]')?.disabled).toBe(true);
  await pressEnter();
  expect(propsA.onStopConversationRun).toHaveBeenCalledTimes(1);
  expect(propsB.onStopConversationRun).toHaveBeenCalledTimes(1);
  await act(async () => finishB(true));
  expect(host.querySelector<HTMLButtonElement>('[aria-label="composer.aria.stop"]')?.disabled).toBe(false);
});
