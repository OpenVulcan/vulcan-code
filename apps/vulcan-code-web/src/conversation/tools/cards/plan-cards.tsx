// Planning cards. A todo update shows the checklist with its progress; a goal call shows the goal with its usage;
// a submitted plan shows the plan itself, streaming while the model writes it.
// 规划卡。待办更新展示清单及进度；目标调用展示目标及用量；提交的计划展示计划本身，模型书写时流式呈现。
import * as React from "react";
import { Check, ClipboardList, Flag, ListChecks } from "lucide-react";
import { useI18n, type TranslationKey } from "@/i18n";
import { formatGoalWorkDuration } from "@/components/composer/composer-panel/goal-work-duration";
import { ToolFrame, type ToolCardProps } from "../tool-card";
import { listOf, objectOf, textOf } from "../tool-call";
import { ErrorNote, MarkdownResult } from "../views/result-views";

// TodoItem is one checklist step.
// TodoItem 是一个清单步骤。
export interface TodoItem {
  // Step text.
  // 步骤文本。
  step: string;
  // Step status.
  // 步骤状态。
  status: "pending" | "in_progress" | "completed";
}

// todoItems reads checklist steps, dropping malformed entries.
// todoItems 读取清单步骤，丢弃格式错误的条目。
export function todoItems(value: unknown): TodoItem[] {
  const items: TodoItem[] = [];
  for (const entry of listOf(value)) {
    const item = objectOf(entry);
    const step = textOf(item?.step);
    const status = textOf(item?.status);
    if (step && (status === "pending" || status === "in_progress" || status === "completed")) items.push({ status, step });
  }
  return items;
}

// STEP_STATES maps a step status onto the kit step state.
// STEP_STATES 将步骤状态映射为套件步骤状态。
const STEP_STATES: Record<TodoItem["status"], string | undefined> = { completed: "done", in_progress: "active", pending: undefined };

/** Renders one todolist call; the accepted host list wins over the requested arguments.
 * 渲染一次 todolist 调用；已接受的宿主列表优先于请求参数。 */
export function TodoCard(props: ToolCardProps) {
  const { t } = useI18n();
  const { call } = props;
  const accepted = call.host?.kind === "todolist" ? call.host.payload : null;
  const source = accepted ?? call.input;
  const items = React.useMemo(() => todoItems(source.items), [source.items]);
  const explanation = textOf(source.explanation)?.trim() ?? "";
  const done = items.filter((item) => item.status === "completed").length;
  const current = items.find((item) => item.status === "in_progress")?.step ?? "";
  return (
    <ToolFrame
      {...props}
      icon={ListChecks}
      meta={items.length > 0 ? <span>{t("toolCard.todo.progress").replace("{done}", String(done)).replace("{total}", String(items.length))}</span> : null}
      state={call.state}
      target={current}
      targetTitle={current}
      verb={t("toolCard.todo.update")}
    >
      {call.isError ? <ErrorNote text={call.modelOutput} /> : items.length > 0 ? (
        <>
          {explanation ? <p className="vk-tool-note">{explanation}</p> : null}
          <ol className="vk-steps-list">
            {items.map((item, index) => (
              <li className="vk-step" data-state={STEP_STATES[item.status]} key={index}>
                <span className="vk-step-mark">{item.status === "completed" ? <Check aria-hidden="true" /> : index + 1}</span>
                <span className="vk-step-text">{item.step}</span>
                <span />
              </li>
            ))}
          </ol>
        </>
      ) : null}
    </ToolFrame>
  );
}

// GoalReport is the goal snapshot a goal tool returns.
// GoalReport 是目标工具返回的目标快照。
export interface GoalReport {
  // Lifecycle status, or null when no goal is set.
  // 生命周期状态；未设置目标时为 null。
  status: string | null;
  // Objective text.
  // 目标内容。
  objective: string;
  // Tokens used, when known.
  // 已用 Token（已知时）。
  tokensUsed: number | null;
  // Token budget, when set.
  // Token 预算（已设置时）。
  tokenBudget: number | null;
  // Elapsed seconds.
  // 已用秒数。
  elapsedSeconds: number | null;
}

// goalNumber reads one "Label: n" line of the goal report.
// goalNumber 读取目标报告中的一行 "Label: n"。
function goalNumber(text: string, label: string): number | null {
  const match = new RegExp(`^${label}: (\\d+)\\s*$`, "m").exec(text);
  return match ? Number(match[1]) : null;
}

// goalReport parses the stable "# Goal" Markdown a goal tool returns; null when the text is not one.
// goalReport 解析目标工具返回的稳定 "# Goal" Markdown；文本不是该格式时返回 null。
export function goalReport(text: string): GoalReport | null {
  const normalized = text.replace(/\r\n?/g, "\n");
  if (!normalized.startsWith("# Goal\n")) return null;
  const status = /^Status: (\S+)\s*$/m.exec(normalized)?.[1] ?? null;
  const objective = /\n## Objective\n\n([\s\S]*?)\n(?:\n(?:Tokens used|Token budget|Elapsed seconds):|$)/.exec(normalized)?.[1]?.trim() ?? "";
  return {
    elapsedSeconds: goalNumber(normalized, "Elapsed seconds"),
    objective,
    status,
    tokenBudget: goalNumber(normalized, "Token budget"),
    tokensUsed: goalNumber(normalized, "Tokens used"),
  };
}

// GOAL_VERBS names each goal tool.
// GOAL_VERBS 命名每个目标工具。
const GOAL_VERBS: Record<string, TranslationKey> = {
  create_goal: "toolCard.goal.create",
  get_goal: "toolCard.goal.get",
  update_goal: "toolCard.goal.update",
};

// GOAL_STATUS_LABELS names each goal lifecycle status.
// GOAL_STATUS_LABELS 命名每个目标生命周期状态。
const GOAL_STATUS_LABELS: Record<string, TranslationKey> = {
  active: "toolCard.goal.status.active",
  blocked: "toolCard.goal.status.blocked",
  budget_limited: "toolCard.goal.status.budgetLimited",
  complete: "toolCard.goal.status.complete",
  paused: "toolCard.goal.status.paused",
  usage_limited: "toolCard.goal.status.usageLimited",
};

/** Renders one get_goal, create_goal or update_goal call.
 * 渲染一次 get_goal、create_goal 或 update_goal 调用。 */
export function GoalCard(props: ToolCardProps) {
  const { t } = useI18n();
  const { call } = props;
  const report = React.useMemo(() => (call.modelOutput && !call.isError ? goalReport(call.modelOutput) : null), [call.isError, call.modelOutput]);
  const status = report?.status ?? (call.name === "update_goal" ? textOf(call.input.status) : null);
  const statusLabel = status ? (Object.hasOwn(GOAL_STATUS_LABELS, status) ? t(GOAL_STATUS_LABELS[status]) : status) : null;
  const objective = report?.objective || textOf(call.input.objective)?.trim() || "";
  const unset = report !== null && report.status === null;
  return (
    <ToolFrame
      {...props}
      icon={Flag}
      meta={unset ? <span>{t("toolCard.goal.none")}</span> : statusLabel ? <span>{statusLabel}</span> : null}
      state={call.state}
      target={objective}
      targetTitle={objective}
      verb={t(GOAL_VERBS[call.name] ?? "toolCard.goal.get")}
    >
      {call.isError ? <ErrorNote text={call.modelOutput} /> : report && !unset ? (
        <dl className="vk-facts">
          {report.objective ? <><dt>{t("toolCard.goal.objective")}</dt><dd>{report.objective}</dd></> : null}
          {statusLabel ? <><dt>{t("toolCard.goal.statusLabel")}</dt><dd>{statusLabel}</dd></> : null}
          <dt>{t("toolCard.goal.tokens")}</dt>
          <dd>
            {report.tokensUsed === null ? t("toolCard.goal.unknown") : report.tokensUsed.toLocaleString()}
            {report.tokenBudget !== null ? ` / ${report.tokenBudget.toLocaleString()}` : ""}
          </dd>
          {report.elapsedSeconds !== null ? <><dt>{t("toolCard.goal.elapsed")}</dt><dd>{formatGoalWorkDuration(report.elapsedSeconds * 1000)}</dd></> : null}
        </dl>
      ) : null}
    </ToolFrame>
  );
}

/** Renders one submit_plan call. Until the plan is saved the body streams the requested plan; a saved plan appears
 * as its own plan block in the transcript, so the card then only names it.
 * 渲染一次 submit_plan 调用。计划保存前正文流式展示请求的计划；保存后的计划在转录中以独立计划块出现，卡片此时只标明它。 */
export function PlanCard(props: ToolCardProps) {
  const { t } = useI18n();
  const { call } = props;
  const artifact = call.host?.kind === "plan_artifact" ? call.host.payload : null;
  const name = textOf(artifact?.name) ?? textOf(call.input.name) ?? "";
  const pending = artifact === null ? textOf(call.input.content) ?? "" : "";
  const path = textOf(artifact?.path);
  return (
    <ToolFrame
      {...props}
      icon={ClipboardList}
      state={call.state}
      target={name}
      targetTitle={path ?? name}
      verb={t("toolCard.plan.submit")}
    >
      {call.isError ? <ErrorNote text={call.modelOutput} /> : pending.trim() ? <MarkdownResult text={pending} /> : null}
    </ToolFrame>
  );
}
