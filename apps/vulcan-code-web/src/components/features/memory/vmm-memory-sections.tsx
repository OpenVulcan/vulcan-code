import type * as React from "react"
import {
  BrainCircuit,
  Database,
  FolderTree,
  ScrollText,
  Settings2,
  SlidersHorizontal,
} from "lucide-react"
import type { TranslationKey } from "@/i18n-messages"
import type { VmmManagementSectionSummary } from "@/types"

// VmmScope identifies the exact configuration layer targeted by one save.
// VmmScope 标识一次保存所针对的准确配置层。
export type VmmScope = "global" | "project"

// VMM_SECTION_TRANSLATION_KEYS binds every service-projected section id to its frontend translation keys.
// VMM_SECTION_TRANSLATION_KEYS 将服务投影的每个分区标识绑定到前端翻译键。
const VMM_SECTION_TRANSLATION_KEYS: Record<string, { detail: TranslationKey; label: TranslationKey }> = {
  overview: { detail: "memory.overview.description", label: "memory.overview.title" },
  inference: { detail: "settings.inference.subtitle", label: "settings.inference.title" },
  rules: { detail: "memory.rules.description", label: "memory.rules.title" },
  content: { detail: "memory.content.description", label: "memory.content.title" },
  identity_projects: { detail: "memory.identity.description", label: "memory.identity.title" },
  profile: { detail: "memory.profile.description", label: "memory.profile.title" },
  advanced: { detail: "memory.advanced.description", label: "memory.advanced.title" },
}

// localizedVmmManagementSection resolves one stable section id into locale-aware navigation text.
// localizedVmmManagementSection 将稳定分区标识解析为当前语言的导航文案。
export function localizedVmmManagementSection(
  section: VmmManagementSectionSummary,
  t: (key: TranslationKey) => string,
): { detail: string; label: string } {
  const keys = VMM_SECTION_TRANSLATION_KEYS[section.id]
  if (!keys) {
    return {
      detail: t("area.memory.description"),
      label: t("workspaceToolPanel.status"),
    }
  }
  return {
    detail: t(keys.detail),
    label: t(keys.label),
  }
}

// vmmSectionIcon maps product-facing memory sections to shared sidebar icons.
// vmmSectionIcon 将产品级记忆分区映射为共享侧栏图标。
export function vmmSectionIcon(sectionId: string): React.ReactNode {
  if (sectionId === "overview") {
    return <BrainCircuit size={16} />
  }
  if (sectionId === "inference") {
    return <Database size={16} />
  }
  if (sectionId === "rules") {
    return <SlidersHorizontal size={16} />
  }
  if (sectionId === "content") {
    return <Database size={16} />
  }
  if (sectionId === "identity_projects") {
    return <FolderTree size={16} />
  }
  if (sectionId === "profile") {
    return <ScrollText size={16} />
  }
  return <Settings2 size={16} />
}
