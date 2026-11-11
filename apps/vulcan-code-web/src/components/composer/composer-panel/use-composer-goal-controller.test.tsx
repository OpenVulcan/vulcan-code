// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { useComposerGoalController, type ComposerGoalController, type UseComposerGoalControllerOptions } from "./use-composer-goal-controller";

vi.mock("@/i18n", () => ({ useI18n: () => ({ t: (key: string) => key }) }));
vi.mock("@/ui-kit/primitives/toast", () => ({ toast: { error: vi.fn() } }));

// controller exposes the real mounted hook while all persistence callbacks remain synthetic.
// controller 暴露实际挂载的钩子，所有持久化回调保持合成实现。
let controller: ComposerGoalController;
// host and root own the lifecycle of one isolated component mount.
// host 与 root 持有一个隔离组件挂载的生命周期。
let host: HTMLDivElement;
let root: Root;
beforeEach(() => { host = document.createElement("div"); document.body.append(host); root = createRoot(host); vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true); });
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

// Harness observes hook state without reproducing its mutation or scope guards.
// Harness 观察钩子状态，不复刻其变更或作用域守卫。
// Parameters: options is the exact hook contract. Returns: no visible UI.
// 参数：options 是准确的钩子契约。返回值：无可见界面。
function Harness({ options }: { options: UseComposerGoalControllerOptions }) { controller = useComposerGoalController(options); return null; }

// options creates complete callback ownership for an isolated new conversation goal.
// options 为隔离的新会话目标创建完整回调归属。
// Parameters: scopeKey identifies one conversation. Returns: a fully typed synthetic controller input.
// 参数：scopeKey 标识一个会话。返回值：完整类型的合成控制器输入。
function options(scopeKey = "conversation-a"): UseComposerGoalControllerOptions {
  return { scope: { key: scopeKey, owner: "draft" }, goal: null, canAutoSubmitCreatedGoal: true, onPrepareEditor: async () => true, onSaveGoal: async () => true, onResumeGoal: async () => true, onClearGoal: async () => true, onUpdateGoalStatus: async () => true, onSubmitCreatedGoal: async () => {} };
}

it("accepts only one rapid save and auto-submits exactly once", async () => {
  // resolveSave releases the one accepted synthetic save.
  // resolveSave 释放唯一已接受的合成保存。
  let resolveSave: (saved: boolean) => void = () => { throw new Error("Save not started"); };
  const save = vi.fn(() => new Promise<boolean>((resolve) => { resolveSave = resolve; }));
  const submit = vi.fn(async () => {});
  const input = { ...options(), onSaveGoal: save, onSubmitCreatedGoal: submit };
  await act(async () => root.render(<Harness options={input} />));
  await act(async () => { controller.setObjectiveDraft("验收目标"); controller.setDialogOpen(true); });
  await act(async () => { void controller.save(); void controller.save(); void controller.clear(); });
  expect(save).toHaveBeenCalledTimes(1);
  expect(controller.saving).toBe(true);
  await act(async () => resolveSave(true));
  expect(submit).toHaveBeenCalledExactlyOnceWith("验收目标");
  expect(controller.dialogOpen).toBe(false);
  expect(controller.saving).toBe(false);
});

// Both event orders must leave an existing session's first Goal turn with the service.
// 两种事件顺序下，已有会话的目标首轮均由服务端负责。
it.each([false, true])("does not send a saved existing-session Goal as a user prompt (phase arrived: %s)", async (phaseArrived) => {
  // resolveSave releases the acknowledgement relative to the phase update.
  // resolveSave 在阶段更新前后的指定时机释放保存回执。
  let resolveSave: (saved: boolean) => void = () => { throw new Error("Save not started"); };
  // save and submit observe the persistence and ordinary submission callbacks.
  // save 和 submit 观测持久化与普通提交回调。
  const save = vi.fn(() => new Promise<boolean>((resolve) => { resolveSave = resolve; }));
  const submit = vi.fn(async () => {});
  // input represents an existing session that looks idle when the dialog is saved.
  // input 表示保存弹窗时界面仍显示空闲的已有会话。
  const input: UseComposerGoalControllerOptions = {
    ...options(), scope: { key: "conversation-a", owner: "session" }, onSaveGoal: save, onSubmitCreatedGoal: submit,
  };

  await act(async () => root.render(<Harness options={input} />));
  await act(async () => { controller.setObjectiveDraft("Goal objective"); controller.setDialogOpen(true); });
  await act(async () => { void controller.save(); });
  if (phaseArrived) {
    await act(async () => root.render(<Harness options={{ ...input, canAutoSubmitCreatedGoal: false }} />));
  }
  await act(async () => resolveSave(true));

  expect(save).toHaveBeenCalledTimes(1);
  expect(submit).not.toHaveBeenCalled();
  expect(controller.dialogOpen).toBe(false);
});

it("retains failed drafts and rejects budgets outside exact integer precision", async () => {
  const save = vi.fn().mockRejectedValueOnce(new Error("Synthetic failure")).mockResolvedValue(true);
  await act(async () => root.render(<Harness options={{ ...options(), onSaveGoal: save }} />));
  await act(async () => { controller.setObjectiveDraft("保留草稿"); controller.setDialogOpen(true); controller.setTokenBudgetDraft("9007199254740992"); });
  await act(async () => controller.save());
  expect(save).not.toHaveBeenCalled();
  await act(async () => controller.setTokenBudgetDraft("1.5"));
  await act(async () => controller.save());
  expect(save).not.toHaveBeenCalled();
  await act(async () => controller.setTokenBudgetDraft("1000"));
  await act(async () => controller.save());
  expect(controller.objectiveDraft).toBe("保留草稿");
  expect(controller.dialogOpen).toBe(true);
  expect(controller.saving).toBe(false);
  await act(async () => controller.save());
  expect(save).toHaveBeenCalledTimes(2);
  expect(controller.dialogOpen).toBe(false);
});

it("does not let an old same-conversation response close a new visit or release its save", async () => {
  // resolvers preserve request order across leaving and returning to the same stable conversation id.
  // resolvers 在离开并返回同一稳定会话标识时保留请求顺序。
  const resolvers: Array<(saved: boolean) => void> = [];
  const save = vi.fn(() => new Promise<boolean>((resolve) => resolvers.push(resolve)));
  const submit = vi.fn(async () => {});
  const input = { ...options(), onSaveGoal: save, onSubmitCreatedGoal: submit };
  await act(async () => root.render(<Harness options={input} />));
  await act(async () => controller.setObjectiveDraft("旧请求"));
  await act(async () => { void controller.save(); });
  await act(async () => root.render(<Harness options={{ ...input, scope: { key: "conversation-b", owner: "draft" } }} />));
  await act(async () => root.render(<Harness options={input} />));
  await act(async () => { controller.setObjectiveDraft("新请求"); controller.setDialogOpen(true); });
  await act(async () => { void controller.save(); });
  await act(async () => resolvers[0](true));
  expect(controller.dialogOpen).toBe(true);
  expect(controller.saving).toBe(true);
  expect(submit).not.toHaveBeenCalled();
  await act(async () => resolvers[1](true));
  expect(submit).toHaveBeenCalledExactlyOnceWith("新请求");
  expect(controller.saving).toBe(false);
});
