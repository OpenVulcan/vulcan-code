import { GitBranch } from "lucide-react";
import { RuntimeModulesPanel } from "@/components/features/settings/settings-runtime-panels";
import { PanelTitle, StatusPill } from "@/components/shared/status";
import type {
  CommandSessionSummary,
  RuntimeModuleSnapshot,
} from "@/types";

// AutomationArea renders workflow and background execution placeholders.
// AutomationArea 渲染工作流与后台执行的预览入口。
export function AutomationArea(props: {
  // Command sessions used to preview background execution.
  // 用于预览后台执行状态。
  commandSessions: CommandSessionSummary[];
  // Runtime modules used to show automation dependency readiness.
  // 用于展示自动化依赖就绪情况的运行时模块。
  modules: RuntimeModuleSnapshot[];
}) {
  const automationCards = [
    { id: "workflow", title: "工作流", status: "规划中", detail: "脚本化任务、插件流程与 WebUI 编排入口" },
    { id: "queue", title: "后台队列", status: "预览", detail: `${props.commandSessions.length} 条命令会话可作为后续队列数据源` },
    { id: "schedule", title: "计划任务", status: "待接入", detail: "后续承接定时、监控与远程触发任务" },
  ];

  return (
    <div className="area-grid two-columns">
      <section className="panel">
        <PanelTitle icon={<GitBranch size={17} />} title="自动" />
        <div className="card-grid">
          {automationCards.map((card) => (
            <article className="data-card" key={card.id}>
              <StatusPill status={card.status === "预览" ? "ready" : "warning"} label={card.status} />
              <strong>{card.title}</strong>
              <span>{card.detail}</span>
            </article>
          ))}
        </div>
      </section>
      <RuntimeModulesPanel modules={props.modules.filter((module) => module.id === "core" || module.id === "mcp" || module.id === "luaskills")} />
    </div>
  );
}
