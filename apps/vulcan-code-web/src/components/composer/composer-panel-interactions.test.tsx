// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { toast } from "@/ui-kit/primitives/toast";
import { ConversationHistoryDialogContext } from "@/state/conversation-history-dialog";
import { ComposerPanel } from "./composer-panel";
import { DEFAULT_THINKING_DISPLAY_MODE } from "@/components/conversation/thinking-display";
import { ComposerEnterSendSettingsSection } from "@/components/features/settings/composer-enter-send-settings";
import { publicAiSettingsWithInvalidDefaultFixture } from "@/test-fixtures/public-ai-settings";
import type { ComposerAttachmentSummary, PendingPromptSummary, PublicAiSessionProviderToolSettings } from "@/types";
import { classifyDroppedComposerFiles, planComposerAttachments, prepareComposerAttachment, stageDroppedComposerFile } from "@/service/conversation-actions-service";

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
  classifyDroppedComposerFiles: vi.fn(),
  stageDroppedComposerFile: vi.fn(),
  cancelConversationSubmission: vi.fn().mockResolvedValue(undefined),
  discardComposerAttachmentPlans: vi.fn().mockResolvedValue(undefined),
  planComposerAttachments: vi.fn().mockResolvedValue([]),
  prepareComposerAttachment: vi.fn(),
  reconcileComposerAttachmentDraftLease: vi.fn().mockResolvedValue(false),
}));
// translate keeps hook dependencies stable while exposing semantic labels for DOM interaction.
// translate 保持 Hook 依赖稳定，同时暴露用于 DOM 交互的语义标签。
const translate = (key: string): string => {
  if (key === "composer.placeholder") return "{shortcut}";
  if (key === "composer.runningShortcutHint") return "{enterAction} / {ctrlEnterAction}";
  return key;
};
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

// Drop Chromium-shaped file or URL data through the mounted composer.
// 通过已挂载的输入区拖入 Chromium 形状的文件或 URL 数据。
async function dragIntoComposer(files: File[], uri = ""): Promise<void> {
  const event = new Event("drop", { bubbles: true, cancelable: true });
  Object.defineProperty(event, "dataTransfer", { value: { files, getData: () => uri } });
  await act(async () => { host.querySelector("textarea")!.dispatchEvent(event); });
  expect(event.defaultPrevented).toBe(true);
}

// Dispatches a real composer paste event with captured file data and optional plain text.
// 用捕获的文件数据及可选纯文本派发真实输入区粘贴事件。
async function pasteIntoComposer(files: File[], plainText = ""): Promise<Event> {
  const event = new Event("paste", { bubbles: true, cancelable: true });
  Object.defineProperty(event, "clipboardData", { value: {
    files, items: [], getData: (format: string) => format === "text/plain" ? plainText : "",
  } });
  await act(async () => { host.querySelector("textarea")!.dispatchEvent(event); });
  return event;
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
  // Use separate storage areas for persistent preferences and this simulated browser tab.
  // 永久偏好与本模拟浏览器标签页使用各自独立的存储。
  const tab = new Map<string, string>();
  vi.stubGlobal("sessionStorage", {
    getItem: (key: string) => tab.get(key) ?? null,
    setItem: (key: string, value: string) => { tab.set(key, value); },
    removeItem: (key: string) => { tab.delete(key); },
  });
  // stored isolates browser preferences without relying on Node's optional localStorage.
  // stored 隔离浏览器偏好，无需依赖 Node 的可选 localStorage。
  const stored = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => stored.get(key) ?? null,
    setItem: (key: string, value: string) => { stored.set(key, value); },
  });
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  // jsdom has no browser object-URL store; preserve the actual image element and URL lifecycle.
  // jsdom 没有浏览器对象 URL 存储，保留真实图片元素与 URL 生命周期。
  vi.stubGlobal("URL", class extends URL {
    static createObjectURL = vi.fn(() => "blob:composer-preview");
    static revokeObjectURL = vi.fn();
  });
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

it("opens the same conversation history action from slash and the plus menu", async () => {
  // Load the exact command as the current persisted composer draft.
  // 将精确指令作为当前持久输入区草稿加载。
  transport.read.mockImplementation(async (sessionId: string) => ({ sessionId, text: "/clean-history", attachments: [] }));
  // Both entry points share the same session owner.
  // 两个入口共用同一个会话所有者。
  const props = composerProps({ slashCommands: [{ id: "clean-history", command: "/clean-history" }] });
  const setOpen = vi.fn();
  await act(async () => root.render(
    <ConversationHistoryDialogContext.Provider value={{ sessionId: props.conversationId!, open: false, setOpen }}>
      <ComposerPanel {...props} />
    </ConversationHistoryDialogContext.Provider>,
  ));
  await pressEnter();
  expect(setOpen).toHaveBeenCalledExactlyOnceWith(true);
  expect(props.onSubmitPrompt).not.toHaveBeenCalled();
  expect(props.onSlashCommand).not.toHaveBeenCalled();
  // Open the real composer menu with the keyboard and select the history action.
  // 用键盘打开真实输入区菜单并选择历史操作。
  const plus = host.querySelector<HTMLButtonElement>(".vk-pick-add")!;
  await act(async () => plus.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })));
  const history = document.querySelector<HTMLElement>('[role="menuitem"][data-action="clean-history"]')!;
  await act(async () => history.click());
  expect(setOpen.mock.calls).toEqual([[true], [true]]);
});

it("references unsupported CHM, restricted image and oversized video through the real drop handler", async () => {
  const files = [
    new File(["documentation"], "guide.chm", { type: "application/vnd.ms-htmlhelp" }),
    new File(["image"], "drawing.gif", { type: "image/gif" }),
    new File([new Uint8Array(2048)], "recording.mp4", { type: "video/mp4" }),
  ];
  vi.stubGlobal("vulcanDesktop", { localFile: { inspect: vi.fn(), pathForFile: (file: File) => `D:/Examples/${file.name}` } });
  vi.mocked(classifyDroppedComposerFiles).mockResolvedValue({ eligible: [false, false], planId: null });
  const props = composerProps({ isConversationRunning: false });
  await mount(props);
  await dragIntoComposer(files);
  expect(host.querySelector("textarea")?.value).toBe("");
  expect([...host.querySelectorAll(".vk-attach-tile")].map(tile => tile.getAttribute("title")))
    .toEqual(files.map(file => "D:/Examples/" + file.name));
  expect(classifyDroppedComposerFiles).toHaveBeenCalledWith(expect.objectContaining({
    conversationId: props.conversationId, files: files.slice(1), attachments: [], forSubmission: true,
  }));
  expect(stageDroppedComposerFile).not.toHaveBeenCalled();
  expect(props.onStageAttachmentBytes).not.toHaveBeenCalled();
});

it("shows a dropped URL and asks for a path when an unsupported file has no native locator", async () => {
  const feedback = vi.spyOn(toast, "info");
  await mount(composerProps());
  await dragIntoComposer([], "https://example.test/reference");
  await dragIntoComposer([new File(["content"], "guide.chm")]);
  expect(host.querySelector("textarea")?.value).toBe("");
  expect(host.querySelector('.vk-attach-tile[title="https://example.test/reference"]')).not.toBeNull();
  expect(feedback).toHaveBeenCalledWith("composer.attachment.dropPathUnavailable");
  expect(classifyDroppedComposerFiles).not.toHaveBeenCalled();
  expect(stageDroppedComposerFile).not.toHaveBeenCalled();
});

it("pastes CHM as a reference and uses the same model and size preflight for EXE, ZIP, PDF, images and video", async () => {
  const samples = [
    new File(["help"], "guide.CHM"),
    new File(["binary"], "program.exe", { type: "application/x-msdownload" }),
    new File(["archive"], "archive.zip", { type: "application/zip" }),
    new File(["pdf"], "document.pdf", { type: "application/pdf" }),
    new File(["image"], "large.png", { type: "image/png" }),
    new File(["video"], "large.mp4", { type: "video/mp4" }),
  ];
  // The browser exposes a large file's size before reading its bytes.
  // 浏览器在读取大文件字节之前即可提供其大小。
  for (const file of samples) Object.defineProperty(file, "size", { value: 62_597_792 });
  vi.stubGlobal("vulcanDesktop", { localFile: { inspect: vi.fn(), pathForFile: (file: File) => `D:/Examples/${file.name}` } });
  vi.mocked(classifyDroppedComposerFiles).mockResolvedValue({ eligible: [false, false, false, false, false], planId: null });
  const props = composerProps();
  await mount(props);
  expect((await pasteIntoComposer(samples)).defaultPrevented).toBe(true);
  expect(classifyDroppedComposerFiles).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({
    files: samples.slice(1), forSubmission: true,
  }));
  const references = [...host.querySelectorAll(".vk-attach-tile")].map(tile => tile.getAttribute("title"));
  expect(references).toEqual(samples.map(file => `D:/Examples/${file.name}`));
  expect(props.onStageAttachmentBytes).not.toHaveBeenCalled();
  expect(stageDroppedComposerFile).not.toHaveBeenCalled();
});

it("pastes native directory and CHM references when the browser exposes no file objects", async () => {
  const readClipboardFileReferences = vi.fn().mockResolvedValue([
    { path: "D:/Examples/Help Folder", name: "Help Folder", directory: true },
    { path: "D:/Examples/guide.chm", name: "guide.chm", directory: false },
  ]);
  vi.stubGlobal("vulcanDesktop", { readClipboardFileReferences });
  await mount(composerProps());
  expect((await pasteIntoComposer([])).defaultPrevented).toBe(true);
  expect(readClipboardFileReferences).toHaveBeenCalledOnce();
  expect(host.querySelector("textarea")?.value).toBe("");
  expect([...host.querySelectorAll(".vk-attach-tile")].map(tile => tile.getAttribute("title")))
    .toEqual(["D:/Examples/Help Folder", "D:/Examples/guide.chm"]);
  expect(classifyDroppedComposerFiles).not.toHaveBeenCalled();
  expect(stageDroppedComposerFile).not.toHaveBeenCalled();
  expect((await pasteIntoComposer([], "ordinary text")).defaultPrevented).toBe(false);
  expect(readClipboardFileReferences).toHaveBeenCalledOnce();
});

it.each(["drop", "paste"] as const)("shows a local image before staging and uses the same attachment strip for %s", async (entry) => {
  // Delay staging to verify that the image is drawn before Core returns a managed asset.
  // 延迟暂存，验证在 Core 返回托管资产前已绘制图像。
  const archive = new File(["archive"], "bundle.zip", { type: "application/zip" });
  const image = new File(["image"], "picture.png", { type: "image/png" });
  vi.stubGlobal("vulcanDesktop", { localFile: { inspect: vi.fn(), pathForFile: (file: File) => `D:/Examples/${file.name}` } });
  vi.mocked(classifyDroppedComposerFiles).mockResolvedValueOnce({ eligible: [false, true], planId: "preview-plan" });
  // Resolve only the accepted image; the rejected archive never reaches byte staging.
  // 仅完成准入图像的暂存，被拒绝的压缩包始终不读取字节。
  let finishStage!: (attachment: ComposerAttachmentSummary) => void;
  vi.mocked(stageDroppedComposerFile).mockImplementationOnce(() =>
    new Promise(resolve => { finishStage = resolve; }));
  const props = composerProps();
  await mount(props);
  if (entry === "drop") await dragIntoComposer([archive, image]);
  else expect((await pasteIntoComposer([archive, image])).defaultPrevented).toBe(true);
  expect(classifyDroppedComposerFiles).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({
    files: [archive, image], forSubmission: true,
  }));
  expect(host.querySelector("textarea")?.value).toBe("");
  expect(host.querySelector('.vk-attach img[alt="picture.png"]')?.getAttribute("src")).toBe("blob:composer-preview");
  expect(props.onStageAttachmentBytes).not.toHaveBeenCalled();
  expect(stageDroppedComposerFile).toHaveBeenCalledExactlyOnceWith("preview-plan", 1, image);
  await act(async () => finishStage({
    kind: "image", fileName: image.name, localPath: "/staged/picture.png", mimeType: image.type, assetId: "preview-image",
  }));
  expect(host.querySelectorAll(".vk-attach")).toHaveLength(1);
  expect(host.querySelector('.vk-attach img[alt="picture.png"]')?.getAttribute("src")).toBe("/staged/picture.png");
  expect(host.querySelector('.vk-attach-tile[title="D:/Examples/bundle.zip"]')).not.toBeNull();
  expect(URL.revokeObjectURL).toHaveBeenCalledWith("blob:composer-preview");
});

it("prompts for the path of a pasted CHM when the clipboard has only its bytes", async () => {
  const feedback = vi.spyOn(toast, "info");
  const props = composerProps();
  await mount(props);
  expect((await pasteIntoComposer([new File(["help"], "guide.chm")])).defaultPrevented).toBe(true);
  expect(feedback).toHaveBeenCalledWith("composer.attachment.dropPathUnavailable");
  expect(props.onStageAttachmentBytes).not.toHaveBeenCalled();
  expect(classifyDroppedComposerFiles).not.toHaveBeenCalled();
  expect(stageDroppedComposerFile).not.toHaveBeenCalled();
});

it("shows a rejected file without a path and still stages the supported image", async () => {
  const supported = new File(["image"], "clipboard.png", { type: "image/png" });
  const oversized = new File(["archive"], "large.zip", { type: "application/zip" });
  Object.defineProperty(oversized, "size", { value: 62_597_792 });
  vi.mocked(classifyDroppedComposerFiles).mockResolvedValue({ eligible: [true, false], planId: "mixed-plan" });
  vi.mocked(stageDroppedComposerFile).mockResolvedValueOnce({
    kind: "image", fileName: supported.name, localPath: "/staged/clipboard.png", mimeType: supported.type, assetId: "clipboard-image",
  });
  const feedback = vi.spyOn(toast, "info");
  const props = composerProps();
  await mount(props);
  await pasteIntoComposer([supported, oversized]);
  expect(host.querySelector('.vk-attach img[alt="clipboard.png"]')).not.toBeNull();
  expect(host.querySelector('.vk-attach-tile[title="large.zip"]')).not.toBeNull();
  expect(feedback).toHaveBeenCalledWith("composer.attachment.dropPathUnavailable");
  expect(props.onStageAttachmentBytes).not.toHaveBeenCalled();
  expect(stageDroppedComposerFile).toHaveBeenCalledExactlyOnceWith("mixed-plan", 0, supported);
});

it("prepares admitted uploads immediately and uses the original path if the active model changes", async () => {
  transport.loadSettings.mockResolvedValue(publicAiSettingsWithInvalidDefaultFixture());
  const image = new File(["image"], "drawing.gif", { type: "image/gif" });
  const video = new File(["video"], "recording.mp4", { type: "video/mp4" });
  vi.stubGlobal("vulcanDesktop", { localFile: { inspect: vi.fn(), pathForFile: (file: File) => `D:/Examples/${file.name}` } });
  vi.mocked(classifyDroppedComposerFiles).mockResolvedValueOnce({ eligible: [true, false], planId: "staging-plan" });
  // The first model stages the original and installs a compressed derivative before upload.
  // 第一个模型先暂存原件，再安装压缩派生文件并上传。
  const staged = { kind: "image", localPath: "/staged/drawing.gif", fileName: "drawing.gif", mimeType: "image/gif", assetId: "image-asset" } satisfies ComposerAttachmentSummary;
  const derivative = { ...staged, localPath: "/staged/drawing.png", fileName: "drawing.png", mimeType: "image/png", assetId: "compressed-image" };
  vi.mocked(stageDroppedComposerFile).mockResolvedValueOnce(staged);
  vi.mocked(planComposerAttachments).mockImplementation(async (_project, _conversation, _slot, generation, model, attachments) =>
    attachments.map(attachment => ({
      attachmentId: attachment.assetId!,
      generation,
      transferId: "transfer-" + generation,
      routeRevision: "route-" + model!.modelId,
      deliveryPlan: model!.modelId === "images" ? "upload_reference" : "metadata_only",
      phase: model!.modelId === "images" ? "upload_queued" : "metadata_only",
      blocksSubmission: model!.modelId === "images",
      reason: model!.modelId === "images" ? "provider_upload_required" : "modality_unavailable",
      error: null,
      ...(attachment.assetId === staged.assetId ? { convertedAttachment: derivative } : {}),
    })));
  vi.mocked(prepareComposerAttachment).mockImplementation(async (_project, _conversation, _model, _attachment, plan) => ({
    ...plan, phase: "direct_ready", blocksSubmission: false,
  }));
  transport.loadSession.mockResolvedValue({
    enabledCapabilities: [], selections: {}, extensionTargetSets: {}, extensionModes: {},
    mediaUnderstandingSelections: {}, conflicts: [], requestsStarted: true,
  } satisfies PublicAiSessionProviderToolSettings);
  // Rerender with the backend-confirmed active model changed, using the same draft owner.
  // 使用同一草稿所有者重新渲染，由后端确认激活模型已更换。
  const propsForModel = (id: string) => composerProps({
    isConversationRunning: false,
    providers: [{ id: "fixture", name: "Fixture", status: "ready", connectionId: "connection",
      models: [{ id, name: id, active: true, routeRevision: "route-" + id }] }],
  });
  const props = propsForModel("images");
  await mount(props);
  await dragIntoComposer([image, video]);
  expect(stageDroppedComposerFile).toHaveBeenCalledExactlyOnceWith("staging-plan", 0, image);
  expect(planComposerAttachments).toHaveBeenCalledWith(expect.anything(), expect.anything(),
    expect.anything(), expect.anything(), expect.objectContaining({ modelId: "images" }), [staged], expect.any(Boolean));
  expect(prepareComposerAttachment).toHaveBeenCalledWith(expect.anything(), expect.anything(),
    expect.objectContaining({ modelId: "images" }), derivative, expect.anything());
  const textOnly = propsForModel("text-only");
  await mount(textOnly);
  await act(async () => { host.querySelector<HTMLButtonElement>('[aria-label="composer.aria.send"]')!.click(); });
  expect(classifyDroppedComposerFiles).toHaveBeenCalledTimes(1);
  expect(stageDroppedComposerFile).toHaveBeenCalledTimes(1);
  expect(textOnly.onSubmitPrompt).toHaveBeenCalledWith(expect.objectContaining({
    prompt: expect.stringContaining('"D:/Examples/drawing.gif"'), attachments: [],
  }));
  expect(textOnly.onSubmitPrompt).toHaveBeenCalledWith(expect.objectContaining({
    prompt: expect.stringContaining('"D:/Examples/recording.mp4"'),
  }));
});

it("includes the managed prefix when classifying a new drop", async () => {
  // The loaded fixture represents a PDF previously added through the file picker.
  // 加载的夹具表示此前通过文件选择器添加的 PDF。
  const managed = { kind: "file", localPath: "/staged/picked.pdf", fileName: "picked.pdf",
    mimeType: "application/pdf", assetId: "managed-pdf" } satisfies ComposerAttachmentSummary;
  transport.read.mockImplementation(async (sessionId: string) => ({ sessionId, text: "", attachments: [managed] }));
  vi.mocked(classifyDroppedComposerFiles).mockResolvedValue({ eligible: [false], planId: null });
  const dropped = new File(["pdf"], "second.pdf", { type: "application/pdf" });
  await mount(composerProps());
  await dragIntoComposer([dropped], "file:///D:/Examples/second.pdf");
  expect(classifyDroppedComposerFiles).toHaveBeenCalledWith(expect.objectContaining({
    attachments: [expect.objectContaining({ assetId: managed.assetId })], files: [dropped],
  }));
  expect(host.querySelector('.vk-attach-tile[title="file:///D:/Examples/second.pdf"]')).not.toBeNull();
  expect(stageDroppedComposerFile).not.toHaveBeenCalled();
});

it("restores only a reference after remount and sends it without staging", async () => {
  transport.loadSettings.mockResolvedValue(publicAiSettingsWithInvalidDefaultFixture());
  transport.loadSession.mockResolvedValue({
    enabledCapabilities: [], selections: {}, extensionTargetSets: {}, extensionModes: {},
    mediaUnderstandingSelections: {}, conflicts: [], requestsStarted: true,
  } satisfies PublicAiSessionProviderToolSettings);
  vi.mocked(classifyDroppedComposerFiles).mockResolvedValue({ eligible: [false], planId: null });
  const props = composerProps({ isConversationRunning: false });
  await mount(props);
  await dragIntoComposer([new File(["original bytes"], "pending.mp4", { type: "video/mp4" })], "file:///D:/Examples/pending.mp4");
  await act(async () => { root.unmount(); });
  root = createRoot(host);
  await mount(props);
  expect(host.querySelector('.vk-attach-tile[title="file:///D:/Examples/pending.mp4"]')).not.toBeNull();
  await act(async () => { host.querySelector<HTMLButtonElement>('[aria-label="composer.aria.send"]')!.click(); });
  expect(classifyDroppedComposerFiles).toHaveBeenCalledTimes(1);
  expect(stageDroppedComposerFile).not.toHaveBeenCalled();
  expect(props.onSubmitPrompt).toHaveBeenCalledWith(expect.objectContaining({
    prompt: expect.stringContaining('"file:///D:/Examples/pending.mp4"'), attachments: [],
  }));
  expect(host.querySelector('.vk-attach-tile[title="file:///D:/Examples/pending.mp4"]')).toBeNull();
});

it("blocks a recovered file without a path until the browser supplies a new original", async () => {
  transport.loadSettings.mockResolvedValue(publicAiSettingsWithInvalidDefaultFixture());
  transport.loadSession.mockResolvedValue({
    enabledCapabilities: [], selections: {}, extensionTargetSets: {}, extensionModes: {},
    mediaUnderstandingSelections: {}, conflicts: [], requestsStarted: true,
  } satisfies PublicAiSessionProviderToolSettings);
  vi.mocked(classifyDroppedComposerFiles).mockResolvedValue({ eligible: [false], planId: null });
  const feedback = vi.spyOn(toast, "info");
  const props = composerProps({ isConversationRunning: false });
  await mount(props);
  await dragIntoComposer([new File(["original bytes"], "no-path.gif", { type: "image/gif" })]);
  await act(async () => { root.unmount(); });
  root = createRoot(host);
  await mount(props);
  await act(async () => { host.querySelector<HTMLButtonElement>('[aria-label="composer.aria.send"]')!.click(); });
  expect(feedback).toHaveBeenCalledWith("composer.attachment.dropSourceUnavailable");
  expect(stageDroppedComposerFile).not.toHaveBeenCalled();
  expect(props.onSubmitPrompt).not.toHaveBeenCalled();
});

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
    // Select the insertion card in the real settings control.
    // 在实际设置控件中选择插队卡片。
    const alternative = host.querySelector<HTMLButtonElement>('[role="radio"][value="insert"]')!;
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

it("updates the mounted composer shortcut hint when the run or the saved Enter action changes", async () => {
  // Re-render the same composer and settings components with the requested run state.
  // 按指定的运行状态重新渲染同一输入区和设置组件。
  const props = composerProps();
  const render = async (isConversationRunning: boolean): Promise<void> => {
    await act(async () => root.render(<>
      <ComposerEnterSendSettingsSection />
      <ComposerPanel {...props} isConversationRunning={isConversationRunning} />
    </>));
  };
  await render(false);
  // Keep the same textarea reference to verify updates without remounting the composer.
  // 保留同一文本框引用，验证无需重挂载输入区即可更新。
  const field = host.querySelector("textarea")!;
  expect(field.placeholder).toBe("composer.shortcutHint");

  await render(true);
  expect(field.placeholder).toBe("settings.composerEnterSend.queue / settings.composerEnterSend.insert");
  await act(async () => host.querySelector<HTMLButtonElement>('[role="radio"][value="insert"]')!.click());
  expect(field.placeholder).toBe("settings.composerEnterSend.insert / settings.composerEnterSend.queue");

  // Type through the native input path so the visible hint moves below the populated field.
  // 通过原生输入路径填写文字，使可见提示移到已有内容的文本框下方。
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")!.set!.call(field, "Follow up");
    field.dispatchEvent(new Event("input", { bubbles: true }));
  });
  expect(document.getElementById(field.getAttribute("aria-describedby")!)?.textContent).toBe(field.placeholder);

  await render(false);
  expect(field.getAttribute("aria-describedby")).toBeNull();
  expect(field.placeholder).toBe("composer.shortcutHint");
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
