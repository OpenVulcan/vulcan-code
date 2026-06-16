import * as React from "react";
import {
  BrainCircuit,
  Maximize2,
  Minimize2,
} from "lucide-react";
import {
  MemoryArea,
  vmmSectionIcon,
  type MemoryActionProps,
  type MemorySubAreaId,
} from "@/components/features/memory/MemoryArea";
import {
  cn,
} from "@/lib/utils";
import type { WorkbenchSnapshot } from "@/types";

// WorkspaceMemoryHeaderState describes the embedded memory title bar projection.
// WorkspaceMemoryHeaderState 描述内嵌记忆设置标题栏投影。
export interface WorkspaceMemoryHeaderState {
  // Header title rendered in the workspace title bar.
  // 渲染在工作台标题栏中的标题。
  title: string;
  // Header detail rendered below the title with trailing fade.
  // 渲染在标题下方并带尾部渐隐的详情。
  detail: string;
  // Header actions rendered on the right side.
  // 渲染在标题栏右侧的操作区。
  actions: React.ReactNode;
}

// WorkspaceMemorySettings renders the embedded VMM memory settings page.
// WorkspaceMemorySettings 渲染内嵌 VMM 记忆设置页面。
export function WorkspaceMemorySettings(props: {
  // Selected embedded memory sub page.
  // 当前选中的内嵌记忆子页面。
  activeSubArea: MemorySubAreaId;
  // Updates the selected embedded memory sub page.
  // 更新当前选中的内嵌记忆子页面。
  onActiveSubAreaChange: (subArea: MemorySubAreaId) => void;
  // Service-backed memory page actions.
  // 服务端支持的记忆页面动作。
  memoryActions: MemoryActionProps;
  // Publishes the computed title bar state to the workspace header.
  // 将计算后的标题栏状态发布到工作台标题栏。
  onHeaderStateChange: (state: WorkspaceMemoryHeaderState) => void;
  // Workbench snapshot containing VMM memory state.
  // 包含 VMM 记忆状态的工作台快照。
  workbench: WorkbenchSnapshot;
}) {
  const {
    activeSubArea,
    memoryActions,
    onActiveSubAreaChange,
    onHeaderStateChange,
    workbench,
  } = props;
  // menuExpanded stores whether the floating memory menu shows every row.
  // menuExpanded 保存浮动记忆菜单是否显示完整行。
  const [menuExpanded, setMenuExpanded] = React.useState(true);
  // rootRef measures the available width for menu-aware centering.
  // rootRef 测量菜单感知居中所需的可用宽度。
  const rootRef = React.useRef<HTMLElement | null>(null);
  // menuOffsetsContent stores whether the expanded menu can reserve layout width.
  // menuOffsetsContent 保存展开菜单是否可以预留布局宽度。
  const [menuOffsetsContent, setMenuOffsetsContent] = React.useState(false);
  // activeMeta stores the current memory section label and detail.
  // activeMeta 保存当前记忆分区标签与详情。
  const activeMeta = memorySubAreaMeta(activeSubArea, workbench);

  React.useEffect(() => {
    onHeaderStateChange({
      actions: null,
      detail: `${activeMeta.label} · ${activeMeta.detail}`,
      title: "记忆设置",
    });
  }, [activeMeta.detail, activeMeta.label, onHeaderStateChange]);

  React.useEffect(() => {
    const element = rootRef.current;
    if (!element || typeof ResizeObserver === "undefined") {
      return;
    }
    // updateMenuOffsetMode keeps the content centered globally when menu reservation would overlap.
    // updateMenuOffsetMode 在菜单预留会产生覆盖时保持内容全局居中。
    const updateMenuOffsetMode = () => {
      const width = element.getBoundingClientRect().width;
      setMenuOffsetsContent(width >= 176 + 800 + 48);
    };
    updateMenuOffsetMode();
    const observer = new ResizeObserver(updateMenuOffsetMode);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  return (
    <section
      className={cn(
        "workspace-skills-settings workspace-memory-settings",
        menuExpanded ? "menu-expanded" : "menu-collapsed",
        menuExpanded && menuOffsetsContent ? "menu-offset" : "menu-overlay",
      )}
      ref={rootRef}
    >
      <FloatingMemoryMenu
        activeSubArea={activeSubArea}
        expanded={menuExpanded}
        onExpandedChange={setMenuExpanded}
        onSelectSubArea={onActiveSubAreaChange}
        workbench={workbench}
      />
      <div className="workspace-skills-content-shell">
        <div className="workspace-skills-content workspace-memory-content">
          <MemoryArea
            activeSection={activeSubArea}
            actions={memoryActions}
            memories={workbench.memories}
            snapshot={workbench.vmmMemory}
          />
        </div>
      </div>
    </section>
  );
}

// FloatingMemoryMenu renders the floating embedded memory selector.
// FloatingMemoryMenu 渲染浮动内嵌记忆选择器。
function FloatingMemoryMenu(props: {
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
  // rows stores the embedded memory menu rows.
  // rows 保存内嵌记忆菜单行。
  const rows = memoryMenuRows(props.workbench);
  // activeMeta stores the selected menu label for the collapsed pill.
  // activeMeta 保存收起胶囊展示的当前菜单标签。
  const activeMeta = memorySubAreaMeta(props.activeSubArea, props.workbench);

  if (!props.expanded) {
    return (
      <button
        className="workspace-skills-menu-collapsed"
        onClick={() => props.onExpandedChange(true)}
        title="展开状态"
        type="button"
      >
        <span>{activeMeta.label}</span>
        <Maximize2 size={13} />
      </button>
    );
  }

  return (
    <aside className="workspace-skills-menu" aria-label="记忆设置菜单">
      <div className="workspace-skills-menu-header">
        <span>菜单</span>
        <button
          aria-label="收起为胶囊"
          className="workspace-skills-menu-icon-button"
          onClick={() => props.onExpandedChange(false)}
          type="button"
        >
          <Minimize2 size={13} />
        </button>
      </div>
      <div className="workspace-skills-menu-list">
        {rows.map((row) => (
          <button
            className={`workspace-skills-menu-row ${props.activeSubArea === row.id ? "active" : ""}`}
            key={row.id}
            onClick={() => props.onSelectSubArea(row.id)}
            title={row.detail}
            type="button"
          >
            {row.icon}
            <span>{row.label}</span>
          </button>
        ))}
      </div>
    </aside>
  );
}

// memoryMenuRows builds embedded memory menu rows from the snapshot.
// memoryMenuRows 根据快照构建内嵌记忆菜单行。
function memoryMenuRows(workbench: WorkbenchSnapshot): Array<{ detail: string; icon: React.ReactNode; id: MemorySubAreaId; label: string }> {
  return [
    {
      detail: workbench.vmmMemory.notice,
      icon: <BrainCircuit size={15} />,
      id: "status",
      label: "状态",
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
function memorySubAreaMeta(subArea: MemorySubAreaId, workbench: WorkbenchSnapshot): { detail: string; label: string } {
  const row = memoryMenuRows(workbench).find((item) => item.id === subArea);
  return {
    detail: row?.detail ?? workbench.vmmMemory.notice,
    label: row?.label ?? "状态",
  };
}
