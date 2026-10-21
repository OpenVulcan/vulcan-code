// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { useComposerDraftSessionSettings, type ComposerDraftSessionSettingsController } from "./use-composer-draft-session-settings";
import { useComposerSendController, type ComposerSendController } from "./use-composer-send-controller";
import type { ComposerSubmission } from "@/components/composer/composer-model";

vi.mock("@/i18n", () => ({ useI18n: () => ({ t: (key: string) => key }) }));

// settings and sender expose committed production hooks; only external draft and backend acknowledgements are synthetic.
// settings 与 sender 暴露已提交的生产 Hook；仅外部草稿与后端确认使用合成边界。
let settings: ComposerDraftSessionSettingsController;
let sender: ComposerSendController;
// submit controls backend acknowledgement without contacting a model or changing a user database.
// submit 控制后端确认，不联系模型或修改用户数据库。
const submit = vi.fn<(submission: ComposerSubmission) => Promise<boolean>>();
// root and host isolate each mounted scenario.
// root 与 host 隔离每个挂载场景。
let root: Root;
let host: HTMLDivElement;

/** Composes the real submission controller with the real draft-setting owner.
 * 将真实提交控制器与真实草稿设置所有者组合。
 * Parameters: owner selects the exact slot, running reports its live run, and beforeSubmit holds route persistence; returns no visible surface.
 * 参数：owner 选择精确槽位，running 报告实时运行状态，beforeSubmit 等待路由持久化；不返回可见界面。
 */
function Harness({ owner = "draft:settings-a", running = false, beforeSubmit }: { owner?: string; running?: boolean; beforeSubmit?: () => Promise<boolean> }) {
  settings = useComposerDraftSessionSettings(owner, null);
  sender = useComposerSendController({
    draftSlotKey: owner, draftInput: "synthetic prompt", attachments: [], executionMode: "build",
    reasoningEffort: null, modelSelection: null, isConversationRunning: running, beforeSubmit,
    isDraftReplacementActive: () => false, onStopConversationRun: async () => true,
    captureSessionSettings: settings.captureSubmissionSettings, clearSessionSettings: settings.clearSubmissionSettings,
    clearForSubmission: async () => ({ draftSlotKey: owner, sessionId: null, input: "synthetic prompt", attachments: [] }),
    restoreFailedSubmission: () => {}, restoreInputToSlot: () => {}, onSubmitPrompt: submit,
  });
  return null;
}

beforeEach(async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  submit.mockReset().mockResolvedValue(true);
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  await act(async () => root.render(<Harness />));
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

it.each(["draft", "goal"] as const)("preserves settings edited while a %s submission awaits acknowledgement", async (kind) => {
  // finish accepts the original request after a later setting edit has committed.
  // finish 在后续设置编辑提交后接受原请求。
  let finish!: (accepted: boolean) => void;
  let pending!: Promise<void>;
  submit.mockImplementationOnce(() => new Promise<boolean>((resolve) => { finish = resolve; }));
  await act(async () => settings.setOutputStyle("first-style"));
  await act(async () => { pending = kind === "draft" ? sender.sendDraft() : sender.submitCreatedGoal("synthetic goal"); });
  expect(submit.mock.calls[0][0].sessionSettings).toEqual({ outputStyleId: "first-style" });
  await act(async () => { settings.setOutputStyle("next-style"); settings.setVmmPolicy("precheck", "disabled"); });
  await act(async () => { finish(true); await pending; });
  expect(settings.outputStyleId).toBe("next-style");
  expect(settings.vmmPolicy?.precheckMode).toBe("disabled");
  expect(submit.mock.calls[0][0].sessionSettings).toEqual({ outputStyleId: "first-style" });
});

it("preserves the first override authored after a submission captured inherited defaults", async () => {
  // finish delays a default-only submission while a new explicit knowledge override is authored.
  // finish 延迟仅继承默认值的提交，期间创建新的显式知识库覆盖。
  let finish!: (accepted: boolean) => void;
  let pending!: Promise<void>;
  submit.mockImplementationOnce(() => new Promise<boolean>((resolve) => { finish = resolve; }));
  await act(async () => { pending = sender.sendDraft(); });
  expect(submit.mock.calls[0][0].sessionSettings).toBeNull();
  await act(async () => settings.setKnowledgeSelection({ mode: "explicit", libraryIds: ["next-library"] }));
  await act(async () => { finish(true); await pending; });
  expect(settings.knowledgeSelection).toEqual({ mode: "explicit", libraryIds: ["next-library"] });
});

it.each(["draft", "goal"] as const)("consumes an unchanged setting only after its %s submission is accepted", async (kind) => {
  await act(async () => settings.setOutputStyle("accepted-style"));
  await act(async () => { if (kind === "draft") await sender.sendDraft(); else await sender.submitCreatedGoal("synthetic goal"); });
  expect(settings.outputStyleId).toBeNull();
});

it("retains settings after backend refusal", async () => {
  submit.mockResolvedValue(false);
  await act(async () => settings.setOutputStyle("retry-style"));
  await act(async () => sender.sendDraft());
  expect(settings.outputStyleId).toBe("retry-style");
});

it("clears the unchanged original owner without clearing a newly selected owner's override", async () => {
  // finish resolves owner A after the real settings hook has selected and edited owner B.
  // finish 在真实设置 Hook 已选择并编辑所有者 B 后结束所有者 A。
  let finish!: (accepted: boolean) => void;
  let pending!: Promise<void>;
  submit.mockImplementationOnce(() => new Promise<boolean>((resolve) => { finish = resolve; }));
  await act(async () => settings.setOutputStyle("owner-a-style"));
  await act(async () => { pending = sender.sendDraft(); });
  await act(async () => root.render(<Harness owner="draft:settings-b" />));
  await act(async () => settings.setOutputStyle("owner-b-style"));
  await act(async () => { finish(true); await pending; });
  expect(settings.outputStyleId).toBe("owner-b-style");
  await act(async () => root.render(<Harness />));
  expect(settings.outputStyleId).toBeNull();
});

it("does not auto-submit a goal if a competing run starts during route persistence", async () => {
  // finishRoute controls the actual preflight boundary before a generated first-turn dispatch.
  // finishRoute 控制生成首轮分派前的实际预检边界。
  let finishRoute!: (saved: boolean) => void;
  let pending!: Promise<void>;
  const routeSaved = new Promise<boolean>((resolve) => { finishRoute = resolve; });
  const beforeSubmit = () => routeSaved;
  await act(async () => root.render(<Harness beforeSubmit={beforeSubmit} />));
  await act(async () => { settings.setInitialGoal("retained goal", null); pending = sender.submitCreatedGoal("retained goal"); });
  await act(async () => root.render(<Harness beforeSubmit={beforeSubmit} running />));
  await act(async () => { finishRoute(true); await pending; });
  expect(submit).not.toHaveBeenCalled();
  expect(settings.initialGoalSummary?.objective).toBe("retained goal");
  expect(sender.isSubmissionActive()).toBe(false);
});

it("does not auto-submit an old goal after leaving and returning during route persistence", async () => {
  // finishRoute resolves an old visit after both owner transitions have committed.
  // finishRoute 在两次所有者切换均已提交后结束旧访问的路由等待。
  let finishRoute!: (saved: boolean) => void;
  let pending!: Promise<void>;
  const routeSaved = new Promise<boolean>((resolve) => { finishRoute = resolve; });
  const beforeSubmit = () => routeSaved;
  await act(async () => root.render(<Harness beforeSubmit={beforeSubmit} />));
  await act(async () => { settings.setInitialGoal("retained goal", null); pending = sender.submitCreatedGoal("retained goal"); });
  await act(async () => root.render(<Harness owner="draft:settings-b" beforeSubmit={beforeSubmit} />));
  await act(async () => root.render(<Harness beforeSubmit={beforeSubmit} />));
  await act(async () => { finishRoute(true); await pending; });
  expect(submit).not.toHaveBeenCalled();
  expect(settings.initialGoalSummary?.objective).toBe("retained goal");
  expect(sender.isSubmissionActive()).toBe(false);
});
