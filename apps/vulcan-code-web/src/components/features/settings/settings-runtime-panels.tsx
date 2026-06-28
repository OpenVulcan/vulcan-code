import { Database } from "lucide-react"

import { PanelTitle, StatusPill } from "@/components/shared/status"
import { useI18n } from "@/i18n"
import type { RuntimeModuleSnapshot } from "@/types"

// RuntimeModulesPanel renders module status cards.
// RuntimeModulesPanel 渲染模块状态卡片。
export function RuntimeModulesPanel(props: {
  // Runtime modules shown as cards.
  // 以卡片展示的运行时模块。
  modules: RuntimeModuleSnapshot[]
}) {
  const { t } = useI18n()
  return (
    <section className="panel modules-panel">
      <PanelTitle icon={<Database size={17} />} title={t("settings.diagnostics.runtimeModules")} />
      <div className="module-grid">
        {props.modules.map((module) => (
          <article className="module-card" key={module.id}>
            <StatusPill status={module.status} label={module.status} />
            <strong>{module.label}</strong>
            <span>{module.message}</span>
          </article>
        ))}
      </div>
    </section>
  )
}
