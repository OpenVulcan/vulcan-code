import * as React from "react";
import { DirectoryRow, PageTabs, WorkspaceDirectoryLayout } from "@/components/workspace/workspace-directory-layout";
import { localizedVmmManagementSection, vmmSectionIcon } from "@/components/features/memory/vmm-memory-sections";
import type { MemoryActionProps } from "@/components/features/memory/use-memory-area-controller";
import { useI18n } from "@/i18n";
import type { WorkbenchSnapshot } from "@/types";

// MemorySubAreaId identifies one service-projected VMM section.
// MemorySubAreaId 标识一个由服务投影的 VMM 分区。
type MemorySubAreaId = string;

// LazyMemoryArea keeps the full VMM editor outside the console-first workbench chunk.
// LazyMemoryArea 将完整 VMM 编辑器保留在控制台优先的工作台分块之外。
const LazyMemoryArea = React.lazy(() => import("@/components/features/memory/memory-area").then((module) => ({ default: module.MemoryArea })));

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
    memoryActions,
    workbench,
  } = props;
  const activeSubArea = effectiveMemorySubArea(props.activeSubArea, workbench);

  // The section menu lives in the navigation panel beside the rail (MemoryWorkspaceNavigation).
  // 分区菜单位于轨道旁的导航面板中（MemoryWorkspaceNavigation）。
  return (
    <WorkspaceDirectoryLayout contentClassName="vmm-workspace-body">
      <MemoryServiceTabs activeSubArea={activeSubArea} onSelectSubArea={props.onSelectSubArea} workbench={workbench} />
      <React.Suspense fallback={null}>
        <LazyMemoryArea
          activeSection={activeSubArea}
          actions={memoryActions}
          onOpenCredentialManagement={props.onOpenCredentialManagement}
          onSelectSection={props.onSelectSubArea}
          snapshot={workbench.vmmMemory}
        />
      </React.Suspense>
    </WorkspaceDirectoryLayout>
  );
}

// MemoryNavigationProps binds stable section IDs to workspace selection.
// MemoryNavigationProps 将稳定分区 ID 绑定到工作区选择。
interface MemoryNavigationProps {
  // Currently selected service section.
  // 当前选中的服务分区。
  activeSubArea: MemorySubAreaId;
  // Receives a known service section ID.
  // 接收已知的服务分区 ID。
  onSelectSubArea: (subArea: MemorySubAreaId) => void;
  // Authoritative available sections and backend capabilities.
  // 权威的可用分区与后端能力。
  workbench: WorkbenchSnapshot;
}

// serviceSections groups connection, runtime, and inference without changing backend IDs.
// serviceSections 将连接、运行状态与推理归组，不改变后端 ID。
const serviceSections = ["inference", "overview", "advanced"];

// effectiveMemorySubArea sends first-use content navigation to setup until a backend is configured.
// effectiveMemorySubArea 在后端配置完成前，将首次使用时的内容入口导向配置总览。
// Parameters: the requested section and the snapshot. Returns the section actually shown.
// 参数：请求的分区与快照。返回实际显示的分区。
function effectiveMemorySubArea(subArea: MemorySubAreaId, workbench: WorkbenchSnapshot): MemorySubAreaId {
  return subArea === "content" && workbench.vmmMemory.experienceState.deployment === "disabled" ? "overview" : subArea;
}

// MemoryWorkspaceNavigation renders fixed task navigation for the panel beside the rail. Props own selection; returns the navigation.
// MemoryWorkspaceNavigation 为轨道旁面板渲染固定任务导航。props 持有选择状态；返回导航。
export function MemoryWorkspaceNavigation(props: MemoryNavigationProps) {
  const { t } = useI18n();
  // activeSubArea mirrors the section the page actually shows, so first-use redirects stay highlighted.
  // activeSubArea 与页面实际显示的分区一致，使首次使用的重定向保持高亮。
  const activeSubArea = effectiveMemorySubArea(props.activeSubArea, props.workbench);
  // rows preserves capability filtering from the service snapshot.
  // rows 保留服务快照的能力过滤。
  const rows = memoryMenuRows(props.workbench, t);
  // groups orders reading before management and configuration.
  // groups 将阅读排在管理与配置之前。
  const groups = [
    { label: t("memory.workspace.read"), ids: ["content", "profile"] },
    { label: t("memory.workspace.manage"), ids: ["identity_projects", "maintenance"] },
    { label: t("memory.workspace.settings"), ids: ["rules", "service", "resource_rules"] },
  ];
  return <nav className="vk-dir" aria-label={t("workspaceToolPanel.memorySettingsMenu")}>
    {groups.map((group) => <div className="vk-dir-group" key={group.label}>
      <p className="vk-dir-head vk-label">{group.label}</p>
      {group.ids.map((id) => {
        // row resolves service navigation to the first supported destination.
        // row 将服务导航解析到首个受支持的目标。
        const row = id === "service"
          ? rows.find((item) => item.id === "inference") ?? rows.find((item) => item.id === "overview")
          : rows.find((item) => item.id === id);
        if (!row) return null;
        // active also highlights the parent when a service subpage is selected.
        // active 在服务子页面被选中时也高亮其父入口。
        const active = id === "service" ? serviceSections.includes(activeSubArea) : activeSubArea === id;
        return <DirectoryRow active={active} icon={row.icon} key={id} label={id === "service" ? t("memory.workspace.service") : row.label}
          onSelect={() => props.onSelectSubArea(row.id)} />;
      })}
    </div>)}
  </nav>;
}

// MemoryServiceTabs exposes all supported service pages. Props own selection; returns tabs or null.
// MemoryServiceTabs 暴露所有受支持的服务页面。props 持有选择状态；返回页签或空值。
function MemoryServiceTabs(props: MemoryNavigationProps) {
  const { t } = useI18n();
  if (!serviceSections.includes(props.activeSubArea)) return null;
  return <PageTabs
    ariaLabel={t("memory.workspace.service")}
    className="vmm-page-tabs"
    onValueChange={props.onSelectSubArea}
    options={memoryMenuRows(props.workbench, t).filter((row) => serviceSections.includes(row.id)).map((row) => ({ label: row.label, value: row.id }))}
    value={props.activeSubArea}
  />;
}

// memoryMenuRows builds embedded memory menu rows from the snapshot.
// memoryMenuRows 根据快照构建内嵌记忆菜单行。
function memoryMenuRows(workbench: WorkbenchSnapshot, t: ReturnType<typeof useI18n>["t"]): Array<{ detail: string; icon: React.ReactNode; id: MemorySubAreaId; label: string }> {
  return workbench.vmmMemory.sections.map((section) => ({
    ...localizedVmmManagementSection(section, workbench.vmmMemory.experienceState.deployment, t),
    icon: vmmSectionIcon(section.id),
    id: section.id,
  }));
}

// memorySubAreaMeta returns the header title and description for one embedded memory page.
// Service subpages share the "models and services" title because their own names are the page tabs.
// memorySubAreaMeta 返回单个内嵌记忆页面的标题栏标题与说明。
// 服务子页面共用“模型与服务”标题，因为其各自名称已作为页内标签显示。
export function memorySubAreaMeta(subArea: MemorySubAreaId, workbench: WorkbenchSnapshot, t: ReturnType<typeof useI18n>["t"]): { detail: string; label: string } {
  const visibleSubArea = effectiveMemorySubArea(subArea, workbench);
  const row = memoryMenuRows(workbench, t).find((item) => item.id === visibleSubArea);
  return {
    detail: row?.detail ?? t("area.memory.description"),
    label: serviceSections.includes(visibleSubArea) ? t("memory.workspace.service") : row?.label ?? t("workspaceToolPanel.status"),
  };
}
