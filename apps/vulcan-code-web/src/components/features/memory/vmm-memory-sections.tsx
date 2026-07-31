import type * as React from "react"
import {
  BrainCircuit,
  Database,
  FolderTree,
  ScrollText,
  Settings2,
  SlidersHorizontal,
} from "lucide-react"

// VmmScope identifies the exact configuration layer targeted by one save.
// VmmScope 标识一次保存所针对的准确配置层。
export type VmmScope = "global" | "project"

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
