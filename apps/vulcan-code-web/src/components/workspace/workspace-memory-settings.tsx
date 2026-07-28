import * as React from "react";
import {
  BrainCircuit,
} from "lucide-react";
import {
  MemoryArea,
  vmmSectionIcon,
  type MemoryActionProps,
  type MemorySubAreaId,
} from "@/components/features/memory/MemoryArea";
import { useI18n } from "@/i18n";
import type { WorkbenchSnapshot } from "@/types";
import { WorkspaceToolPanelMenu } from "@/components/workspace/workspace-tool-panel-menu";

// WorkspaceMemorySettings renders the embedded VMM memory settings page.
// WorkspaceMemorySettings 渲染内嵌 VMM 记忆设置页面。
export function WorkspaceMemorySettings(props: {
  // Selected embedded memory sub page.
  // 当前选中的内嵌记忆子页面。
  activeSubArea: MemorySubAreaId;
  // Service-backed memory page actions.
  // 服务端支持的记忆页面动作。
  memoryActions: MemoryActionProps;
  // Workbench snapshot containing VMM memory state.
  // 包含 VMM 记忆状态的工作台快照。
  workbench: WorkbenchSnapshot;
}) {
  const {
    activeSubArea,
    memoryActions,
    workbench,
  } = props;

  return (
    <MemoryArea
      activeSection={activeSubArea}
      actions={memoryActions}
      memories={workbench.memories}
      snapshot={workbench.vmmMemory}
    />
  );
}

// FloatingMemoryMenu renders the floating embedded memory selector.
// FloatingMemoryMenu 渲染浮动内嵌记忆选择器。
export function FloatingMemoryMenu(props: {
  // Selected embedded memory sub page.
  // 当前选中的内嵌记忆子页面。
  activeSubArea: MemorySubAreaId;
  // Whether the menu is expanded.
  // 菜单是否展开。
  expanded: boolean;
  // Updates the expanded menu state.
  // 更新菜单展开状态。
  onExpandedChange: (expanded: boolean) => void;
  // Selects one embedded memory sub page.
  // 选择一个内嵌记忆子页面。
  onSelectSubArea: (subArea: MemorySubAreaId) => void;
  // Workbench snapshot containing VMM memory sections.
  // 包含 VMM 记忆分区的工作台快照。
  workbench: WorkbenchSnapshot;
}) {
  const { t } = useI18n();
  // rows stores the embedded memory menu rows.
  // rows 保存内嵌记忆菜单行。
  const rows = memoryMenuRows(props.workbench, t);
  // activeMeta stores the selected menu label for the collapsed pill.
  // activeMeta 保存收起胶囊展示的当前菜单标签。
  const activeMeta = memorySubAreaMeta(props.activeSubArea, props.workbench, t);

  return (
    <WorkspaceToolPanelMenu
      activeLabel={activeMeta.label}
      ariaLabel={t("workspaceToolPanel.memorySettingsMenu")}
      collapseLabel={t("workspaceToolPanel.collapseToPill")}
      expanded={props.expanded}
      expandLabel={t("workspaceToolPanel.expandState")}
      heading={t("workspaceToolPanel.menu")}
      onExpandedChange={props.onExpandedChange}
    >
      {rows.map((row) => (
        <button
          className={`workspace-tool-panel-menu-row ${props.activeSubArea === row.id ? "active" : ""}`}
          key={row.id}
          onClick={() => props.onSelectSubArea(row.id)}
          title={row.detail}
          type="button"
        >
          {row.icon}
          <span>{row.label}</span>
        </button>
      ))}
    </WorkspaceToolPanelMenu>
  );
}

// memoryMenuRows builds embedded memory menu rows from the snapshot.
// memoryMenuRows 根据快照构建内嵌记忆菜单行。
function memoryMenuRows(workbench: WorkbenchSnapshot, t: ReturnType<typeof useI18n>["t"]): Array<{ detail: string; icon: React.ReactNode; id: MemorySubAreaId; label: string }> {
  return [
    {
      detail: workbench.vmmMemory.notice,
      icon: <BrainCircuit size={15} />,
      id: "status",
      label: t("workspaceToolPanel.status"),
    },
    ...workbench.vmmMemory.sections.map((section) => ({
      detail: section.description,
      icon: vmmSectionIcon(section.id),
      id: section.id,
      label: section.label,
    })),
  ];
}

// memorySubAreaMeta returns the compact label and detail for one embedded memory page.
// memorySubAreaMeta 返回单个内嵌记忆页面的紧凑标签与详情。
export function memorySubAreaMeta(subArea: MemorySubAreaId, workbench: WorkbenchSnapshot, t: ReturnType<typeof useI18n>["t"]): { detail: string; label: string } {
  const row = memoryMenuRows(workbench, t).find((item) => item.id === subArea);
  return {
    detail: row?.detail ?? workbench.vmmMemory.notice,
    label: row?.label ?? t("workspaceToolPanel.status"),
  };
}
