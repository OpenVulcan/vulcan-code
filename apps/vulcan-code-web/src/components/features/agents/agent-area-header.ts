import { formatSkillText } from "@/components/features/featureText"
import type { FeatureHeaderSummary } from "@/components/features/feature-area-types"
import type { useI18n } from "@/i18n"
import type { WorkbenchSnapshot } from "@/types"

// activeAgentsHeaderSummary returns the title and metrics without loading the agent editor.
// activeAgentsHeaderSummary 在不加载智能体编辑器的情况下返回标题与统计。
export function activeAgentsHeaderSummary(
  workbench: WorkbenchSnapshot,
  t: ReturnType<typeof useI18n>["t"],
): FeatureHeaderSummary {
  // enabledCount is the exact enabled-definition count from the current snapshot.
  // enabledCount 是当前快照中的精确启用定义数量。
  const enabledCount = workbench.subagentCatalog.definitions.filter((definition) => definition.enabled).length
  return {
    title: t("area.agents"),
    detail: formatSkillText(t("agents.header.detail"), {
      total: workbench.subagentCatalog.definitions.length,
      enabled: enabledCount,
      warnings: workbench.subagentCatalog.errors.length,
    }),
  }
}
