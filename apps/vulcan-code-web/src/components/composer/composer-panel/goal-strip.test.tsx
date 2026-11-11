import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { TooltipProvider } from "@/ui-kit/primitives/tooltip";
import type { SessionGoalStatus, SessionGoalSummary, SessionGoalUserStatusUpdate } from "@/types";
import { ComposerGoalStrip } from "./goal-strip";
import { ComposerGoalDetails } from "./goal-details";
import { useComposerGoalController, type ComposerGoalController } from "./use-composer-goal-controller";

// renderGoal captures the real controller action while rendering the production goal strip.
// renderGoal 在渲染生产目标条时捕获真实控制器动作。
// Parameters: status is the authoritative backend state. Returns: markup, action, and observed status requests.
// 参数：status 是权威后端状态。返回值：标记、动作和已观察的状态请求。
function renderGoal(status: SessionGoalStatus) {
  // goal retains the same objective and budget when a suspended goal resumes.
  // goal 在暂停的目标恢复时保留同一目标正文与预算。
  const goal: SessionGoalSummary = {
    sessionId: "session", goalId: "goal", objective: "Finish the remaining steps", status,
    tokenBudget: 20_000, tokensUsed: 0, usageTokens: 0, apiCallCount: 0, toolCallCount: 0,
    inputTokens: 0, cachedInputTokens: 0, outputTokens: 0, reasoningTokens: 0, totalTokens: 0,
    timeUsedSeconds: 0, createdAtMs: 1, updatedAtMs: 2,
  };
  // onUpdateGoalStatus observes the exact RPC target chosen by the real controller callback.
  // onUpdateGoalStatus 观察真实控制器回调选择的精确 RPC 目标状态。
  const onUpdateGoalStatus = vi.fn<(next: SessionGoalUserStatusUpdate) => Promise<boolean>>().mockResolvedValue(true);
  // controller is assigned synchronously by the server renderer; this fixture does not emulate browser state updates.
  // controller 由服务端渲染器同步赋值；此夹具不模拟浏览器状态更新。
  let controller!: ComposerGoalController;
  // Harness binds the same controller and strip contract as ComposerPanel; it has no parameters and returns the strip.
  // Harness 绑定与 ComposerPanel 相同的控制器和目标条契约；无参数，返回目标条。
  function Harness() {
    controller = useComposerGoalController({
      scope: { key: "session", owner: "session" }, goal, canAutoSubmitCreatedGoal: false,
      onPrepareEditor: async () => true, onSaveGoal: async () => true,
      onResumeGoal: async () => true, onClearGoal: async () => true,
      onSubmitCreatedGoal: async () => undefined, onUpdateGoalStatus,
    });
    return <ComposerGoalStrip goal={goal} disabled={false}
      onEdit={controller.openEditor} onResume={controller.openResumeEditor}
      onToggleStatus={controller.toggleStatus} onClear={controller.clear} />;
  }
  // markup exercises the real button labels and icon selection without introducing a DOM dependency.
  // markup 验证真实按钮标签与图标选择，不引入 DOM 依赖。
  const markup = renderToStaticMarkup(<I18nProvider initialLocale="zh-CN"><TooltipProvider>
    <Harness />
  </TooltipProvider></I18nProvider>);
  return { markup, toggleStatus: controller.toggleStatus, onUpdateGoalStatus, goal };
}

describe("goal strip lifecycle actions", () => {
  // Cases follow pause_goal/resume_goal admission; budget resumes keep their separate editor and completed goals cannot toggle.
  // 场景遵循 pause_goal/resume_goal 准入；预算恢复保留独立编辑器，已完成目标不能切换。
  it.each<[SessionGoalStatus, SessionGoalUserStatusUpdate | null, string | null]>([
    ["active", "paused", "暂停目标"],
    ["paused", "active", "恢复目标"],
    ["blocked", "active", "恢复目标"],
    ["usage_limited", "active", "恢复目标"],
    ["budget_limited", null, "继续任务"],
    ["complete", null, null],
  ])("renders and dispatches the same permitted action for %s", async (status, nextStatus, label) => {
    // rendered binds display assertions and action assertions to the same authoritative goal.
    // rendered 将展示断言与动作断言绑定到同一权威目标。
    const rendered = renderGoal(status);
    if (label !== null) expect(rendered.markup).toContain(`aria-label="${label}"`);
    if (status !== "active") expect(rendered.markup).not.toContain('aria-label="暂停目标"');
    if (status === "complete") expect(rendered.markup).not.toContain('aria-label="恢复目标"');
    await rendered.toggleStatus();
    if (nextStatus === null) {
      expect(rendered.onUpdateGoalStatus).not.toHaveBeenCalled();
    } else {
      expect(rendered.onUpdateGoalStatus).toHaveBeenCalledExactlyOnceWith(nextStatus);
    }
    expect(rendered.goal.status).toBe(status);
  });

  // Blocking is a paused execution with a retained reason, not a request to pause again.
  // 受阻表示执行已暂停并保留原因，而不是需要再次暂停。
  it("shows the blocked reason as paused with a play control", () => {
    // markup must expose the paused reason as well as the actual play icon.
    // markup 必须同时暴露暂停原因与真实播放图标。
    const { markup } = renderGoal("blocked");
    expect(markup).toContain("受阻的目标（已暂停）");
    expect(markup).toContain("lucide-circle-play");
    expect(markup).not.toContain("lucide-circle-pause");
  });

  it("keeps usage out of the strip and inside the full-objective two-column card", () => {
    // rendered exercises the production objective trigger and separate details content.
    // rendered 验证生产目标触发器及独立详情内容。
    const rendered = renderGoal("blocked");
    expect(rendered.markup).not.toContain("composer-goal-strip-budget");
    expect(rendered.markup).not.toContain("20K");
    expect(rendered.markup).toContain("0:00:00");
    const detail = renderToStaticMarkup(<I18nProvider initialLocale="zh-CN">
      <ComposerGoalDetails goal={{ ...rendered.goal, objective: "First line\n" + "完整目标".repeat(200),
        // The backend owns budget usage independently of displayed token dimensions.
        // 后端独立持有预算用量，不由展示的 Token 维度反推。
        tokensUsed: 150,
        inputTokens: 100, cachedInputTokens: 900, outputTokens: 50, reasoningTokens: 20, totalTokens: 1050 }} />
    </I18nProvider>);
    expect(detail).toContain("完整目标".repeat(200));
    expect(detail).toContain("data-goal-metrics");
    expect(detail).toContain("150 / 20K");
    expect(detail).toContain("90.0%");
    expect(detail).toContain("1.1K");
  });
});
