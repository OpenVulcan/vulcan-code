import { Target } from "lucide-react";
import { useI18n, type TranslationKey } from "@/i18n";

// Maps the durable reasonCode to localized display copy; unknown codes never leak as prose.
// 将持久化 reasonCode 映射为本地化展示文案；未知编号绝不作为正文泄漏。
// Returns a title for the exact Goal transition, using t from the active locale.
// 使用当前语言的 t，返回精确目标转换的标题。
export function goalNoticeTitle(reasonCode: string, t: (key: TranslationKey) => string): string {
  switch (reasonCode) {
    case "goal_saved": return t("conversation.goalNotice.saved");
    case "goal_resumed": return t("conversation.goalNotice.resumed");
    case "goal_paused": return t("conversation.goalNotice.paused");
    case "goal_blocked": return t("conversation.goalNotice.blocked");
    case "goal_completed": return t("conversation.goalNotice.completed");
    case "goal_cleared": return t("conversation.goalNotice.cleared");
    case "goal_budget_exhausted": return t("conversation.goalNotice.budgetExhausted");
    case "goal_usage_limited": return t("conversation.goalNotice.usageLimited");
    case "goal_usage_unknown": return t("conversation.goalNotice.usageUnknown");
    default: return t("conversation.goalNotice.updated");
  }
}

// Renders reasonCode as a marked status card shared by the canonical and branch views.
// 将 reasonCode 渲染为规范视图与分支视图共用的状态标记卡片。
// seq optionally identifies the source event; returns a non-interactive notice, not assistant prose.
// seq 可选地标识源事件；返回非交互提示，而不是 assistant 正文。
export function GoalStatusNotice(props: { reasonCode: string; seq?: number }) {
  // Use current interface language for both live and restored historical notices.
  // 实时提示和恢复的历史提示都使用当前界面语言。
  const { t } = useI18n();
  return (
    <aside className="special-block notice" role="note" data-event-kind="goal_status" data-seq={props.seq}>
      <div className="special-block-main">
        <Target size={16} aria-hidden="true" />
        <strong>{goalNoticeTitle(props.reasonCode, t)}</strong>
      </div>
    </aside>
  );
}
