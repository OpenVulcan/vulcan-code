// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { LuaSkillSummary, LuaSkillsOperationProgressSummary } from "@/types";
import type { SkillActionProps } from "./skill-area-types";
import { LuaSkillsPanel } from "./lua-skills-panel";

vi.mock("@/i18n", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/i18n")>();
  const t = (key: import("@/i18n").TranslationKey) => actual.readLocaleMessage("en-US", key);
  return { ...actual, useI18n: () => ({ locale: "en-US", setLocale: () => undefined, t }) };
});

let root: Root;
let host: HTMLDivElement;
beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});
afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.unstubAllGlobals();
});

// configEvent builds one retained configuration event.
// configEvent 构建一条保留的配置事件。
// skillId null models a storage-domain event, which no package owns.
// skillId 为 null 表示存储域事件，它不属于任何技能包。
function configEvent(
  skillId: string | null,
  error: { code: string; message: string } | null,
  source = "external_reload",
  storeScope = "skills",
) {
  return { sequence: "4", type: "config_reload_failed", storeScope, skillId,
    revision: "1", changedKeys: [], source, restartRequiredKeys: [],
    complete: null, error: error ? { key: null, ...error } : null };
}

// settle waits past the panel's 1500 ms first-poll delay using real time, because
// faking timers would starve the recursive poll that reschedules itself.
// settle 用真实时间越过面板 1500 毫秒的首次轮询延迟，因为伪造定时器会让自我重排的递归
// 轮询饥饿。
async function settle(ms: number) {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, ms));
  });
}

it("shows a storage-domain watch failure in the list with no dialog open", async () => {
  const actions = {
    onPollLuaSkillConfigEvents: vi.fn().mockResolvedValue({ nextSequence: "4", events: [
      configEvent(null, { code: "CONFIG_WATCH_FAILED", message: "watcher unavailable" })] }),
  } as unknown as SkillActionProps;
  await act(async () => root.render(<LuaSkillsPanel actions={actions} skills={[]} />));
  await settle(1800);
  // The failure has no skillId, so a per-package filter would have discarded it.
  // 该故障没有 skillId，因此按技能包过滤会丢弃它。
  expect(document.body.textContent).toContain("CONFIG_WATCH_FAILED");
  expect(document.body.textContent).toContain("watcher unavailable");
  expect(document.body.textContent).toContain("affects every package");
});

it("names the owning package when a scoped event reports the failure", async () => {
  const actions = {
    onPollLuaSkillConfigEvents: vi.fn().mockResolvedValue({ nextSequence: "4", events: [
      configEvent("fixture", { code: "CONFIG_RELOAD_FAILED", message: "reload rejected" })] }),
  } as unknown as SkillActionProps;
  await act(async () => root.render(<LuaSkillsPanel actions={actions} skills={[]} />));
  await settle(1800);
  expect(document.body.textContent).toContain("CONFIG_RELOAD_FAILED");
  expect(document.body.textContent).toContain("fixture");
});

it("clears the banner once a newer event shows configuration activity resumed", async () => {
  const poll = vi.fn()
    .mockResolvedValueOnce({ nextSequence: "4", events: [
      configEvent(null, { code: "CONFIG_WATCH_FAILED", message: "watcher unavailable" })] })
    .mockResolvedValue({ nextSequence: "5", events: [configEvent("fixture", null)] });
  const actions = { onPollLuaSkillConfigEvents: poll } as unknown as SkillActionProps;
  await act(async () => root.render(<LuaSkillsPanel actions={actions} skills={[]} />));
  await settle(1800);
  expect(document.body.textContent).toContain("CONFIG_WATCH_FAILED");
  // The next scheduled poll reports healthy activity, so a stale warning must not
  // outlive the failure it described.
  // 下一次计划轮询报告健康活动，因此过时警告不得比它所描述的失败存活更久。
  await settle(3300);
  expect(document.body.textContent).not.toContain("CONFIG_WATCH_FAILED");
  // This test crosses two real poll intervals, so it needs more than the default 5 s
  // budget; faking timers would starve the recursive poll instead.
  // 本测试跨越两个真实轮询间隔，因此需要超过默认 5 秒的预算；伪造定时器反而会让递归轮询
  // 饥饿。
}, 15000);

it("keeps a watcher failure visible after an unrelated local write succeeds", async () => {
  const poll = vi.fn()
    .mockResolvedValueOnce({ nextSequence: "4", events: [
      configEvent(null, { code: "CONFIG_WATCH_FAILED", message: "watcher unavailable" })] })
    .mockResolvedValue({ nextSequence: "5", events: [configEvent("fixture", null, "local_write")] });
  const actions = { onPollLuaSkillConfigEvents: poll } as unknown as SkillActionProps;
  await act(async () => root.render(<LuaSkillsPanel actions={actions} skills={[]} />));
  await settle(1800);
  expect(document.body.textContent).toContain("CONFIG_WATCH_FAILED");
  // A direct write proves storage is writable, but it cannot prove that the failed
  // filesystem watcher was recreated or resumed.
  // 直接写入只能证明存储可写，不能证明失败的文件系统监听器已经重建或恢复。
  await settle(3300);
  expect(document.body.textContent).toContain("CONFIG_WATCH_FAILED");
}, 15000);

it("clears only the store whose external watcher has recovered", async () => {
  const poll = vi.fn()
    .mockResolvedValueOnce({ nextSequence: "4", events: [
      configEvent(null, { code: "SKILLS_WATCH_FAILED", message: "skills watcher unavailable" }),
      configEvent(null, { code: "SYSTEM_WATCH_FAILED", message: "system watcher unavailable" },
        "external_reload", "system-skills"),
    ] })
    .mockResolvedValue({ nextSequence: "5", events: [configEvent("fixture", null)] });
  const actions = { onPollLuaSkillConfigEvents: poll } as unknown as SkillActionProps;
  await act(async () => root.render(<LuaSkillsPanel actions={actions} skills={[]} />));
  await settle(1800);
  expect(document.body.textContent).toContain("SKILLS_WATCH_FAILED");
  expect(document.body.textContent).toContain("SYSTEM_WATCH_FAILED");
  // A healthy external event proves recovery only for its own store. The sibling
  // store keeps its warning until that store produces its own recovery evidence.
  // 健康的外部事件只能证明其所属存储域已经恢复；同级存储域在产生自己的恢复证据前继续保留警告。
  await settle(3300);
  expect(document.body.textContent).not.toContain("SKILLS_WATCH_FAILED");
  expect(document.body.textContent).toContain("SYSTEM_WATCH_FAILED");
}, 15000);

// progress builds one operation-progress summary with the supplied overrides.
// progress 以给定覆盖项构建一个操作进度摘要。
function progress(
  overrides: Partial<LuaSkillsOperationProgressSummary> = {},
): LuaSkillsOperationProgressSummary {
  return { correlationId: "luaskills-op-0", action: "install", inFlight: false,
    outcome: "succeeded", overflowed: false, sawUpstreamTerminal: true, phases: 2, ...overrides };
}

// renderPanel mounts the list with a never-resolving config poll so only the
// operation-progress rendering under test can change the DOM.
// renderPanel 以永不结束的配置轮询挂载列表，使只有被测的操作进度渲染能改动 DOM。
async function renderPanel(operationProgress: LuaSkillsOperationProgressSummary | null) {
  const actions = {
    onPollLuaSkillConfigEvents: () => new Promise(() => {}),
  } as unknown as SkillActionProps;
  await act(async () => root.render(
    <LuaSkillsPanel actions={actions} operationProgress={operationProgress} skills={[]} />));
}

it("shows a running operation with its phase and percentage", async () => {
  await renderPanel(progress({ inFlight: true, outcome: null, percent: 42,
    latestPhase: "downloading_archive", skillId: "fixture" }));
  expect(document.body.textContent).toContain("in progress");
  expect(document.body.textContent).toContain("downloading_archive");
  expect(document.body.textContent).toContain("42%");
  expect(document.body.textContent).toContain("fixture");
});

it("shows a failed operation with the upstream recovery detail", async () => {
  await renderPanel(progress({ outcome: "failed",
    message: "checksum mismatch for <path>" }));
  expect(document.body.textContent).toContain("failed");
  // The message is the recovery entry point and must reach the list unchanged.
  // 该消息是恢复入口，必须原样到达列表。
  expect(document.body.textContent).toContain("checksum mismatch for <path>");
});

it("says phases were dropped instead of letting a gap read as a stall", async () => {
  await renderPanel(progress({ inFlight: true, outcome: null, overflowed: true }));
  expect(document.body.textContent).toContain("some phases were dropped");
});

it("renders no operation banner when no operation is tracked", async () => {
  await renderPanel(null);
  expect(document.body.textContent).not.toContain("in progress");
  expect(document.body.textContent).not.toContain("Skill package operation");
});

// QUEUE_LIMITATION is the exact user-facing sentence that discloses the exclusive
// engine lock held for the whole install and update.
// QUEUE_LIMITATION 是披露整次安装与更新期间所持引擎独占锁的确切用户可见文案。
const QUEUE_LIMITATION =
  "The skill engine is held exclusively during install/update, including download and verification, so other skill operations queue behind it.";

it("discloses the exclusive engine lock while an install or update runs", async () => {
  // The write lock spans download and verification, so the resulting queueing is a real
  // limitation the user must be able to see rather than an implementation detail.
  // 写锁跨越下载与校验全程持有，因此由此产生的排队是用户必须能看到的真实限制，而不是实现细节。
  await renderPanel(progress({ inFlight: true, outcome: null }));
  expect(document.body.textContent).toContain(QUEUE_LIMITATION);
});

it("does not claim exclusive locking when no operation is running", async () => {
  // A settled operation must not leave the limitation visible, because nothing holds the
  // engine lock once the RPC has returned.
  // 已结束的操作不得让该限制继续可见，因为 RPC 返回后已没有任何东西持有引擎锁。
  await renderPanel(progress({ inFlight: false, outcome: "succeeded" }));
  expect(document.body.textContent).not.toContain(QUEUE_LIMITATION);
});

it("reports a settled operation as completed rather than as a failure", async () => {
  // The success and failure texts are adjacent branches, so swapping them would be silent
  // unless the success terminal state is asserted on its own instead of only being used
  // as setup for an unrelated assertion.
  // 成功与失败文案是相邻分支，除非单独断言成功终态（而不是仅用作其他断言的铺垫），
  // 否则二者互换不会被发现。
  await renderPanel(progress({ inFlight: false, outcome: "succeeded" }));
  const text = document.body.textContent ?? "";
  expect(text).toContain("Skill package operation completed.");
  expect(text).not.toContain("Skill package operation failed.");
});

// skill builds one projected Lua Skill row; only the fields the filters read are set.
// `tools` must be present because the card counts it, so a row without it would fail in
// the card rather than in the filter this fixture exists to exercise.
// skill 构建一条投影后的 Lua Skill 行；只设置筛选器读取的那些字段。`tools` 必须存在，因为
// 卡片会统计它；缺少它会在卡片中失败，而不是在本 fixture 想要验证的筛选器里失败。
function skill(skillId: string, enabled: boolean): LuaSkillSummary {
  const row = { skillId, displayName: skillId, version: "1.0.0", rootName: "user", enabled, tools: [] };
  return row as unknown as LuaSkillSummary;
}

it("distinguishes an empty catalog from a filter that matched nothing", async () => {
  const actions = {
    onPollLuaSkillConfigEvents: vi.fn().mockResolvedValue({ nextSequence: "4", events: [] }),
  } as unknown as SkillActionProps;

  // An empty catalog tells the operator to install something.
  // 空目录告诉操作者去安装。
  await act(async () => root.render(<LuaSkillsPanel actions={actions} skills={[]} />));
  expect(document.body.textContent).toContain("Real skill packages appear after startup completes or refresh runs.");
  expect(document.body.textContent).not.toContain("No items match the current filters.");

  // A filter that matched nothing must say so instead of claiming nothing is installed:
  // the two messages send the operator to different fixes, so a swap between them is a
  // misleading instruction rather than a wording preference.
  // 筛选无匹配必须如实说明，而不能声称什么都没安装：两条文案把操作者引向不同的修复动作，
  // 因此二者互换是误导性指引，而不是措辞偏好。
  await act(async () => root.render(<LuaSkillsPanel actions={actions} skills={[skill("pkg", true)]} />));
  const stopped = [...document.querySelectorAll("button")]
    .find((button) => button.textContent === "Disabled");
  if (!stopped) throw new Error("the stopped-status filter chip must exist");
  await act(async () => { stopped.click(); });

  expect(document.body.textContent).toContain("No items match the current filters.");
  expect(document.body.textContent).not.toContain("Real skill packages appear after startup completes or refresh runs.");
});
