import { Circle, CircleCheck, CircleDot } from "lucide-react";
import { formatMessage, useI18n } from "@/i18n";
import type { ToolCallEvent } from "@/service/conversation-event-display-projection";
import { domainObject, resolveV1ToolDomainPresentation, type V1ToolDomainSection } from "./tool-presentations/v1-tool-domain-presentation";
import "@/styles/tool-domain.css";

/** Renders accepted or explicitly pending tasks as a compact status checklist.
 * 将已接受或显式待提交任务渲染为紧凑状态清单。
 * Parameter section contains the declared step/status items; returns progress and every task without field-table repetition.
 * 参数 section 包含声明的 step/status 条目；返回进度与所有任务，不重复字段表格。
 */
export function TodoListResult(props: { section: V1ToolDomainSection }) {
  const { t } = useI18n();
  if (!Array.isArray(props.section.value)) return <div role="status">{t("toolDomain.todo.invalidList")}</div>;
  // items remain in producer order and accept only the native todolist step/status contract.
  // items 保留产出顺序，仅接受原生 todolist 的 step/status 契约。
  const items = props.section.value.map(domainObject);
  if (items.some(item => item === null || typeof item.step !== "string" || !["pending", "in_progress", "completed"].includes(String(item.status)))) return <div role="status">{t("toolDomain.todo.invalidItem")}</div>;
  // completed counts only confirmed completed states, including the all-complete and empty cases.
  // completed 仅统计确认完成的状态，覆盖全部完成与空清单情况。
  const completed = items.filter(item => item!.status === "completed").length;
  return <section className="tool-todo-card" aria-label={props.section.title}>
    <header className="tool-todo-header"><span>{props.section.title}</span><span className="tool-todo-count">{formatMessage(t("toolDomain.todo.count"), { completed, total: items.length })}</span></header>
    <div className="tool-todo-progress" role="progressbar" aria-label={t("toolDomain.todo.progressLabel")} aria-valuemin={0} aria-valuemax={items.length || 1} aria-valuenow={completed}>
      <span style={{ width: `${items.length === 0 ? 0 : completed / items.length * 100}%` }} />
    </div>
    <ol className="tool-todo-list">{items.map((item, index) => <li key={index} className="tool-todo-item" data-status={String(item!.status)}>
      <span className="tool-todo-status" aria-label={t(item!.status === "completed" ? "toolDomain.status.completed" : item!.status === "in_progress" ? "toolDomain.status.in_progress" : "toolDomain.status.pending")}>{item!.status === "completed" ? <CircleCheck size={16} /> : item!.status === "in_progress" ? <CircleDot size={16} /> : <Circle size={16} />}</span>
      <span className="tool-todo-step">{String(item!.step)}</span>
      {item!.status === "in_progress" ? <span className="tool-todo-active-label">{t("toolDomain.status.in_progress")}</span> : null}
    </li>)}</ol>
    {items.length === 0 ? <div className="v1-tool-domain-muted">{t("toolDomain.todo.empty")}</div> : null}
  </section>;
}

/** Renders the already available todo snapshot synchronously, without a deferred result-module fallback.
 * 同步渲染已经可用的待办快照，不经过延迟结果模块的占位。
 * Parameter event owns accepted items or explicitly pending input; returns the checklist, explanation and diagnostics.
 * 参数 event 持有已接受条目或显式待提交输入；返回清单、说明与诊断。
 */
export function ConversationTodoResult(props: { event: ToolCallEvent }) {
  // domain preserves the existing accepted-host-result versus pending-input boundary.
  // domain 保留既有的已接受宿主结果与待提交输入边界。
  const { t } = useI18n();
  const domain = resolveV1ToolDomainPresentation(props.event, t);
  return <div className="v1-tool-domain">
    {domain.diagnostic ? <div className="v1-tool-domain-error" role="alert">{domain.diagnostic}</div> : null}
    {domain.sections.map((section, index) => section.presentation === "todo-list"
      ? <TodoListResult key={index} section={section} />
      : <section key={index} className="v1-tool-domain-section"><h4>{section.title}</h4><div className="v1-tool-domain-text">{String(section.value)}</div></section>)}
    {domain.sections.length === 0 && !domain.diagnostic ? <div className="v1-tool-domain-muted">{t("toolDomain.noResult")}</div> : null}
  </div>;
}
