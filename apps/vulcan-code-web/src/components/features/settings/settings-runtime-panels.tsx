import { Database } from "lucide-react"

import { PanelTitle, StatusPill } from "@/components/shared/status"
import { useI18n } from "@/i18n"
import type { RuntimeModuleSnapshot, StartupStepStatus } from "@/types"

// MODULE_STATUS_LABELS translates the service status enum into plain words for the module cards.
// MODULE_STATUS_LABELS 将服务状态枚举翻译成模块卡片上的通俗文字。
const MODULE_STATUS_LABELS: Record<StartupStepStatus, [zh: string, en: string]> = {
  pending: ["等待中", "Waiting"],
  running: ["启动中", "Starting"],
  ready: ["正常", "OK"],
  warning: ["需要留意", "Needs attention"],
  error: ["出错", "Error"],
}

// RuntimeModulesPanel renders module status cards.
// RuntimeModulesPanel 渲染模块状态卡片。
export function RuntimeModulesPanel(props: {
  // Runtime modules shown as cards.
  // 以卡片展示的运行时模块。
  modules: RuntimeModuleSnapshot[]
}) {
  const { locale, t } = useI18n()
  return (
    <section className="panel modules-panel">
      <PanelTitle icon={<Database size={17} />} title={t("settings.diagnostics.runtimeModules")} />
      <div className="module-grid">
        {props.modules.map((module) => (
          <article className="module-card" key={module.id}>
            <StatusPill status={module.status} label={MODULE_STATUS_LABELS[module.status]?.[locale === "zh-CN" ? 0 : 1] ?? module.status} />
            <strong>{module.label}</strong>
            <span>{module.message}</span>
          </article>
        ))}
      </div>
    </section>
  )
}
