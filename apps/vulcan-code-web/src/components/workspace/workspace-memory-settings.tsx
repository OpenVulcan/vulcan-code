import * as React from "react";
import { Folder, Globe, SlidersHorizontal, UserRound } from "lucide-react";
import { DirectoryRow, PageTabs, WorkspaceDirectoryLayout } from "@/components/workspace/workspace-directory-layout";
import { localizedVmmManagementSection, vmmSectionIcon } from "@/components/features/memory/vmm-memory-sections";
import type { MemoryActionProps } from "@/components/features/memory/use-memory-area-controller";
import {
  isMarkdownMemorySection,
  memorySectionForMode,
  useMemorySettings,
  useMemorySettingsSnapshot,
} from "@/components/features/memory/markdown/memory-mode-store";
import type { TranslationKey } from "@/i18n-messages";
import { useI18n } from "@/i18n";
import type { MemoryMode, WorkbenchSnapshot } from "@/types";

// MemorySubAreaId identifies one service-projected VMM section.
// MemorySubAreaId 标识一个由服务投影的 VMM 分区。
type MemorySubAreaId = string;

// LazyMemoryArea keeps the full VMM editor outside the console-first workbench chunk.
// LazyMemoryArea 将完整 VMM 编辑器保留在控制台优先的工作台分块之外。
const LazyMemoryArea = React.lazy(() => import("@/components/features/memory/memory-area").then((module) => ({ default: module.MemoryArea })));
// LazyMarkdownMemoryArea keeps the Markdown memory pages and the mode settings in their own chunk.
// LazyMarkdownMemoryArea 将 Markdown 记忆页面与模式设置保留在独立分块中。
const LazyMarkdownMemoryArea = React.lazy(() => import("@/components/features/memory/markdown/markdown-memory-area").then((module) => ({ default: module.MarkdownMemoryArea })));
// Load the shared mode and model configuration only when opening a memory service page.
// 仅在打开记忆服务页面时加载共享的模式和模型配置。
const LazyMemoryModePage = React.lazy(() => import("@/components/features/memory/markdown/memory-mode-page").then((module) => ({ default: module.MemoryModePage })));

// MARKDOWN_SECTION_KEYS binds Markdown content sections to their navigation copy and icon.
// MARKDOWN_SECTION_KEYS 将 Markdown 内容分区绑定到导航文案与图标。
const MARKDOWN_SECTION_KEYS: Record<string, { detail: TranslationKey; icon: React.ReactNode; label: TranslationKey }> = {
  md_user: { detail: "memory.markdown.detail.user", icon: <UserRound size={16} />, label: "memory.markdown.nav.user" },
  md_space: { detail: "memory.markdown.detail.space", icon: <Globe size={16} />, label: "memory.markdown.nav.space" },
  md_project: { detail: "memory.markdown.detail.project", icon: <Folder size={16} />, label: "memory.markdown.nav.project" },
};

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
  const mode = useMemorySettings()?.mode ?? null;
  const activeSubArea = effectiveMemorySubArea(props.activeSubArea, workbench, mode);

  if (isMarkdownMemorySection(activeSubArea)) {
    return (
      <WorkspaceDirectoryLayout contentClassName="vmm-workspace-body vk-atmosphere" pageKey={activeSubArea}>
        <React.Suspense fallback={null}>
          <LazyMarkdownMemoryArea
            section={activeSubArea}
            workbench={workbench}
          />
        </React.Suspense>
      </WorkspaceDirectoryLayout>
    );
  }

  // Keep the saved mode and service tab independent so switching modes never loses the settings entry.
  // 独立维护已保存模式和服务页签，切换模式时仍能留在设置入口。
  const servicePage = serviceSections.includes(activeSubArea);
  // The VMM controls keep their existing service actions and diagnostics.
  // VMM 控件复用现有服务操作和诊断。
  const vmmContent = (
    <LazyMemoryArea
      activeSection={activeSubArea}
      actions={memoryActions}
      onOpenCredentialManagement={props.onOpenCredentialManagement}
      onSelectSection={props.onSelectSubArea}
      snapshot={workbench.vmmMemory}
    />
  );
  return (
    <WorkspaceDirectoryLayout contentClassName="vmm-workspace-body vk-atmosphere" pageKey={activeSubArea}>
      {mode === "vmm" ? <MemoryServiceTabs activeSubArea={activeSubArea} onSelectSubArea={props.onSelectSubArea} workbench={workbench} /> : null}
      <React.Suspense fallback={null}>
        {servicePage ? <LazyMemoryModePage vmmContent={vmmContent} /> : vmmContent}
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

// effectiveMemorySubArea maps the request onto the pages of the memory mode, then sends first-use VMM
// content navigation to setup until a backend is configured.
// effectiveMemorySubArea 先把请求映射到当前记忆模式的页面，再在 VMM 后端配置完成前将首次使用时的
// 内容入口导向配置总览。
// Parameters: the requested section, the snapshot and the mode (null while loading). Returns the section shown.
// 参数：请求的分区、快照与模式（加载中为 null）。返回实际显示的分区。
function effectiveMemorySubArea(subArea: MemorySubAreaId, workbench: WorkbenchSnapshot, mode: MemoryMode | null): MemorySubAreaId {
  const section = memorySectionForMode(subArea, mode);
  return section === "content" && workbench.vmmMemory.experienceState.deployment === "disabled" ? "overview" : section;
}

// MemoryWorkspaceNavigation renders fixed task navigation for the panel beside the rail. Props own selection; returns the navigation.
// MemoryWorkspaceNavigation 为轨道旁面板渲染固定任务导航。props 持有选择状态；返回导航。
export function MemoryWorkspaceNavigation(props: MemoryNavigationProps) {
  const { t } = useI18n();
  const mode = useMemorySettings()?.mode ?? null;
  // activeSubArea mirrors the section the page actually shows, so first-use redirects stay highlighted.
  // activeSubArea 与页面实际显示的分区一致，使首次使用的重定向保持高亮。
  const activeSubArea = effectiveMemorySubArea(props.activeSubArea, props.workbench, mode);
  // rows preserves capability filtering from the service snapshot.
  // rows 保留服务快照的能力过滤。
  const rows = memoryMenuRows(props.workbench, t);
  // groups orders reading before management and configuration; the mode decides which pages exist.
  // groups 将阅读排在管理与配置之前；模式决定存在哪些页面。
  const groups = mode === "markdown" ? [
    { label: t("memory.workspace.read"), ids: ["md_user", "md_space", "md_project"] },
    { label: t("memory.workspace.settings"), ids: ["service"] },
  ] : mode === "off" ? [
    { label: t("memory.workspace.settings"), ids: ["service"] },
  ] : [
    { label: t("memory.workspace.read"), ids: ["content", "profile"] },
    { label: t("memory.workspace.manage"), ids: ["identity_projects", "maintenance"] },
    { label: t("memory.workspace.settings"), ids: ["service", "rules", "resource_rules"] },
  ];
  return <nav className="vk-dir" aria-label={t("workspaceToolPanel.memorySettingsMenu")}>
    {groups.map((group) => <div className="vk-dir-group" key={group.label}>
      <p className="vk-dir-head vk-label">{group.label}</p>
      {group.ids.map((id) => {
        // Configuration has a stable entry even when the VMM service has no available sections.
        // 即使 VMM 服务没有可用分区，配置也有稳定入口。
        if (id === "service") {
          return <DirectoryRow active={serviceSections.includes(activeSubArea)} icon={<SlidersHorizontal size={16} />}
            key={id} label={t("memory.workspace.service")} onSelect={() => props.onSelectSubArea("inference")} />;
        }
        // Resolve the remaining entries against the backend's visible section catalog.
        // 根据后端的可见分区目录解析其余入口。
        const row = rows.find((item) => item.id === id);
        if (!row) return null;
        return <DirectoryRow active={activeSubArea === id} icon={row.icon} key={id} label={row.label}
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
  // Use the status tab when the selected backend has no separate inference page.
  // 当前后端没有独立推理页时，高亮状态页签。
  const rows = memoryMenuRows(props.workbench, t).filter((row) => serviceSections.includes(row.id));
  // A single service page needs no tab strip.
  // 只有一个服务页面时无需页签栏。
  if (rows.length < 2) return null;
  return <PageTabs
    ariaLabel={t("memory.workspace.service")}
    className="vmm-page-tabs"
    onValueChange={props.onSelectSubArea}
    options={rows.map((row) => ({ label: row.label, value: row.id }))}
    value={rows.some((row) => row.id === props.activeSubArea) ? props.activeSubArea : "overview"}
  />;
}

// memoryMenuRows builds embedded memory menu rows from the snapshot.
// memoryMenuRows 根据快照构建内嵌记忆菜单行。
function memoryMenuRows(workbench: WorkbenchSnapshot, t: ReturnType<typeof useI18n>["t"]): Array<{ detail: string; icon: React.ReactNode; id: MemorySubAreaId; label: string }> {
  return [
    ...workbench.vmmMemory.sections.map((section) => ({
      ...localizedVmmManagementSection(section, workbench.vmmMemory.experienceState.deployment, t),
      icon: vmmSectionIcon(section.id),
      id: section.id,
    })),
    ...Object.entries(MARKDOWN_SECTION_KEYS).map(([id, keys]) => ({
      detail: t(keys.detail),
      icon: keys.icon,
      id,
      label: t(keys.label),
    })),
  ];
}

// useMemorySubAreaMeta returns the header title and description for one embedded memory page.
// Service subpages share the "models and services" title because their own names are the page tabs.
// useMemorySubAreaMeta 返回单个内嵌记忆页面的标题栏标题与说明。
// 服务子页面共用“模型与服务”标题，因为其各自名称已作为页内标签显示。
export function useMemorySubAreaMeta(subArea: MemorySubAreaId, workbench: WorkbenchSnapshot, t: ReturnType<typeof useI18n>["t"]): { detail: string; label: string } {
  const mode = useMemorySettingsSnapshot()?.mode ?? null;
  const visibleSubArea = effectiveMemorySubArea(subArea, workbench, mode);
  const row = memoryMenuRows(workbench, t).find((item) => item.id === visibleSubArea);
  return {
    detail: row?.detail ?? t("area.memory.description"),
    label: serviceSections.includes(visibleSubArea) ? t("memory.workspace.service") : row?.label ?? t("workspaceToolPanel.status"),
  };
}
