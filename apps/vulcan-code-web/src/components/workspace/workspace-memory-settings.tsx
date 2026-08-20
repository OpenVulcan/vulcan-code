import * as React from "react";
import { localizedVmmManagementSection, vmmSectionIcon } from "@/components/features/memory/vmm-memory-sections";
import type { MemoryActionProps } from "@/components/features/memory/use-memory-area-controller";
import { useI18n } from "@/i18n";
import type { WorkbenchSnapshot } from "@/types";
import { WorkspaceToolPanelMenu } from "@/components/workspace/workspace-tool-panel-menu";

// MemorySubAreaId identifies one service-projected VMM section.
// MemorySubAreaId 标识一个由服务投影的 VMM 分区。
type MemorySubAreaId = string;

// LazyMemoryArea keeps the full VMM editor outside the console-first workbench chunk.
// LazyMemoryArea 将完整 VMM 编辑器保留在控制台优先的工作台分块之外。
const LazyMemoryArea = React.lazy(() => import("@/components/features/memory/MemoryArea").then((module) => ({ default: module.MemoryArea })));

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
  // Opens credential management when VMM prerequisite models are not configured.
  // 在 VMM 前置模型未配置时打开凭据管理。
  onOpenCredentialManagement: () => void;
  // Updates the selected embedded memory sub page.
  // 更新选中的内嵌记忆子页面。
  onSelectSubArea: (subArea: MemorySubAreaId) => void;
}) {
  const {
    activeSubArea,
    memoryActions,
    workbench,
  } = props;

  return (
    <React.Suspense fallback={null}>
      <LazyMemoryArea
        activeSection={activeSubArea}
        actions={memoryActions}
        onOpenCredentialManagement={props.onOpenCredentialManagement}
        onSelectSection={props.onSelectSubArea}
        snapshot={workbench.vmmMemory}
      />
    </React.Suspense>
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
  return workbench.vmmMemory.sections.map((section) => ({
    ...localizedVmmManagementSection(section, t),
    icon: vmmSectionIcon(section.id),
    id: section.id,
  }));
}

// memorySubAreaMeta returns the compact label and detail for one embedded memory page.
// memorySubAreaMeta 返回单个内嵌记忆页面的紧凑标签与详情。
export function memorySubAreaMeta(subArea: MemorySubAreaId, workbench: WorkbenchSnapshot, t: ReturnType<typeof useI18n>["t"]): { detail: string; label: string } {
  const row = memoryMenuRows(workbench, t).find((item) => item.id === subArea);
  return {
    detail: row?.detail ?? t("area.memory.description"),
    label: row?.label ?? t("workspaceToolPanel.status"),
  };
}
