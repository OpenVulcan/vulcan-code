// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { ComposerPanel } from "./composer-panel";
import { DEFAULT_THINKING_DISPLAY_MODE } from "@/components/conversation/thinking-display";
import { AnswerReplyContext, AnswerReplyStore } from "@/components/conversation/answer-components/answer-reply";
import type { ComposerAttachmentSummary } from "@/types";

// transport replaces only external service boundaries; the capability editor, whose own settings read decides the
// provider tool gate, is the one control left out so the send path can be reached.
// transport 仅替换外部服务边界；能力编辑器自身的设置读取决定供应商工具闸门，它是唯一被省略的控件，使发送路径可达。
const transport = vi.hoisted(() => ({ read: vi.fn(), write: vi.fn() }));
vi.mock("@/service/conversation-session-drafts-v1", () => ({ sessionDraftClientV1: transport }));
vi.mock("@/service/settings-actions-service", () => ({
  loadPublicAiSettings: vi.fn(() => new Promise(() => {})),
  loadPublicAiSessionProviderToolSelections: vi.fn().mockResolvedValue(null),
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
vi.mock("@/components/composer/composer-controls", async (importOriginal) => ({
  ...await importOriginal<typeof import("@/components/composer/composer-controls")>(),
  ComposerAiCapabilityControl: () => null,
}));
const translate = (key: string) => key;
vi.mock("@/i18n", () => ({ useI18n: () => ({ locale: "en-US", t: translate }) }));

let root: Root;
let host: HTMLDivElement;

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  Element.prototype.scrollIntoView = vi.fn();
  transport.read.mockImplementation(async (sessionId: string) => ({ sessionId, text: "Earlier text", attachments: [] }));
  transport.write.mockImplementation(async (sessionId: string, text: string, attachments: ComposerAttachmentSummary[]) => ({ sessionId, text, attachments }));
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); vi.restoreAllMocks(); });

// A reply sent from an answer component joins the existing draft and leaves through the composer's own send path.
// 从回答组件直接发送的回复并入已有草稿，并经输入区自身的发送路径发出。
it("sends an answer reply through the normal send path, appended to the existing draft", async () => {
  const onSubmitPrompt = vi.fn().mockResolvedValue(true);
  const props: React.ComponentProps<typeof ComposerPanel> = {
    attachmentCapabilities: [], projectId: "reply-project", conversationId: "reply-session", contextUsage: null,
    defaultAnswerMode: "standard", slashCommands: [], pendingPrompts: [], sessionGoal: null, sessionTodolist: null,
    sessionReviewSummary: null, sessionPlanMode: null, sessionVmmPolicy: null, sessionOutputStyle: null,
    outputStyleCatalog: { defaultId: "default", selectedId: "default", entries: [] },
    contextCompactionWindowNodes: [], sessionContextWindowSelection: null, customContextWindowMinTokens: 100_000,
    vmmAvailable: false, isConversationRunning: false, thinkingDisplayMode: DEFAULT_THINKING_DISPLAY_MODE,
    providers: [], reasoningSelection: null,
    onSelectWorkbenchModel: vi.fn().mockResolvedValue(true),
    onListWorkbenchModelSecondaryRoutes: vi.fn().mockResolvedValue([]),
    onSelectWorkbenchModelSecondaryRoute: vi.fn().mockResolvedValue(true),
    onSelectWorkbenchReasoningEffort: vi.fn().mockResolvedValue(true),
    onSlashCommand: vi.fn().mockReturnValue({ handled: false }), onOpenKnowledge: vi.fn(), onSubmitPrompt,
    onSaveSessionGoal: vi.fn().mockResolvedValue(true), onResumeSessionGoal: vi.fn().mockResolvedValue(true),
    onClearSessionGoal: vi.fn().mockResolvedValue(true), onUpdateSessionGoalStatus: vi.fn().mockResolvedValue(true),
    onSetSessionPlanMode: vi.fn().mockResolvedValue(true), onSetSessionVmmPolicy: vi.fn().mockResolvedValue(true),
    onSetSessionOutputStyle: vi.fn().mockResolvedValue(true), onSetSessionContextCompactionWindow: vi.fn().mockResolvedValue(true),
    onStopConversationRun: vi.fn().mockResolvedValue(true), onStageAttachmentBytes: vi.fn().mockResolvedValue(null),
    resolveMediaSourceUrl: (path) => path,
    onEditPendingPrompt: vi.fn().mockResolvedValue(null), onInsertPendingPrompt: vi.fn().mockResolvedValue(undefined),
    onRemovePendingPrompt: vi.fn().mockResolvedValue(undefined), onReorderPendingPrompts: vi.fn().mockResolvedValue(undefined),
    onThinkingDisplayModeChange: vi.fn(),
  };
  const store = new AnswerReplyStore();
  await act(async () => root.render(<AnswerReplyContext.Provider value={store}><ComposerPanel {...props} /></AnswerReplyContext.Provider>));
  await vi.waitFor(() => expect(host.querySelector("textarea")?.value).toBe("Earlier text"));
  await act(async () => store.set("fence", { kind: "decision", question: "Q?", picks: ["A"], recommended: [], note: "" }));
  await act(async () => { expect(store.deliver("send", () => "Decisions: Q? → A")).toBe(true); });
  await vi.waitFor(() => expect(onSubmitPrompt).toHaveBeenCalledOnce());
  expect(onSubmitPrompt.mock.calls[0][0].prompt).toBe("Earlier text\n\nDecisions: Q? → A");
  expect(store.size()).toBe(0);
});
