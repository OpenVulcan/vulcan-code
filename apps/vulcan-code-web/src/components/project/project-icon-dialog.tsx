import React from "react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
} from "@/components/ui/select";
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@/components/ui/tabs";
import { cn } from "@/lib/utils";
import type { ProjectSummary } from "@/types";
import {
  DEFAULT_PROJECT_BADGE_CONFIG,
  PROJECT_BADGE_COLORS,
  PROJECT_ICON_MAP,
  PROJECT_ICON_STREAM_BATCH_SIZE,
  ProjectIconifyIcon,
  encodeProjectBadgeKey,
  decodeProjectBadgeKey,
  isIconifyProjectIconPack,
  normalizeProjectBadgeText,
  projectIconOptionsForTab,
  projectIconPackLabel,
  type ProjectIconBadgeConfig,
  type ProjectIconBadgeShape,
  type ProjectIconPack,
  type ProjectIconSelection,
  type ProjectIconTab,
} from "./project-icon-model";
import { ProjectIcon } from "./project-icons";

// ProjectIconDialogTarget stores the project currently editing its icon.
// ProjectIconDialogTarget 保存当前正在编辑图标的项目。
export interface ProjectIconDialogTarget {
  // Project row whose icon is being edited.
  // 当前正在编辑图标的项目行。
  project: ProjectSummary;
}

// ProjectIconDialog renders the project icon picker and generated badge editor.
// ProjectIconDialog 渲染项目图标选择器与生成式徽标编辑器。
export function ProjectIconDialog(props: {
  // Project icon target currently edited by this dialog.
  // 当前弹窗正在编辑的项目图标目标。
  target: ProjectIconDialogTarget | null;
  // Cancels the current icon edit.
  // 取消当前图标编辑。
  onCancel: () => void;
  // Persists the selected icon.
  // 持久化选中的图标。
  onConfirm: (project: ProjectSummary, icon: ProjectIconSelection) => void;
  // Resets the selected project to default folder icons.
  // 将选中项目还原为默认文件夹图标。
  onReset: (project: ProjectSummary) => void;
}) {
  const project = props.target?.project ?? null;
  // selectedIcon stores the currently selected built-in icon.
  // selectedIcon 保存当前选中的内置图标。
  const [selectedIcon, setSelectedIcon] = React.useState<ProjectIconSelection>({ pack: "lucide", key: "folder" });
  // badgeConfig stores the generated badge draft.
  // badgeConfig 保存生成式徽标草稿。
  const [badgeConfig, setBadgeConfig] = React.useState<ProjectIconBadgeConfig>(DEFAULT_PROJECT_BADGE_CONFIG);
  // activeTab stores the visible picker tab.
  // activeTab 保存当前可见的选择器标签。
  const [activeTab, setActiveTab] = React.useState<ProjectIconTab>("skill-icons");
  // iconSearch stores the Lucide icon filter text.
  // iconSearch 保存当前图标包的过滤文本。
  const [iconSearch, setIconSearch] = React.useState("");
  // visibleIconLimit stores the streamed icon render limit.
  // visibleIconLimit 保存图标流式渲染上限。
  const [visibleIconLimit, setVisibleIconLimit] = React.useState(PROJECT_ICON_STREAM_BATCH_SIZE);

  React.useEffect(() => {
    if (!project) {
      return;
    }
    if (project.iconPack === "badge") {
      setActiveTab("badge");
      setBadgeConfig(decodeProjectBadgeKey(project.iconKey) ?? {
        ...DEFAULT_PROJECT_BADGE_CONFIG,
        text: normalizeProjectBadgeText(project.name) || DEFAULT_PROJECT_BADGE_CONFIG.text,
      });
      return;
    }
    if (isIconifyProjectIconPack(project.iconPack) && project.iconKey) {
      setActiveTab(project.iconPack);
      setSelectedIcon({ pack: project.iconPack, key: project.iconKey });
      setIconSearch("");
      setVisibleIconLimit(PROJECT_ICON_STREAM_BATCH_SIZE);
      setBadgeConfig({
        ...DEFAULT_PROJECT_BADGE_CONFIG,
        text: normalizeProjectBadgeText(project.name) || DEFAULT_PROJECT_BADGE_CONFIG.text,
      });
      return;
    }
    const key = project.iconPack === "lucide" && project.iconKey && PROJECT_ICON_MAP.has(project.iconKey)
      ? project.iconKey
      : "Folder";
    setActiveTab("lucide");
    setSelectedIcon({ pack: "lucide", key });
    setIconSearch("");
    setVisibleIconLimit(PROJECT_ICON_STREAM_BATCH_SIZE);
    setBadgeConfig({
      ...DEFAULT_PROJECT_BADGE_CONFIG,
      text: normalizeProjectBadgeText(project.name) || DEFAULT_PROJECT_BADGE_CONFIG.text,
    });
  }, [project]);

  React.useEffect(() => {
    setVisibleIconLimit(PROJECT_ICON_STREAM_BATCH_SIZE);
  }, [activeTab, iconSearch]);

  // updateBadgeText applies the Chinese-one-or-English-two text rule.
  // updateBadgeText 应用中文一个字或英文两个字母的文字规则。
  const updateBadgeText = React.useCallback((value: string) => {
    setBadgeConfig((current) => ({
      ...current,
      text: normalizeProjectBadgeText(value),
    }));
  }, []);

  // handleDialogInteractOutside keeps nested Radix select popovers from closing this dialog.
  // handleDialogInteractOutside 避免嵌套的 Radix 选择器浮层被弹窗误判为外部点击而关闭。
  const handleDialogInteractOutside = React.useCallback((event: Event) => {
    const target = event.target instanceof HTMLElement ? event.target : null;
    if (!target) {
      return;
    }
    if (target.closest("[data-slot='select-content'], [data-slot='select-trigger']")) {
      event.preventDefault();
    }
  }, []);

  if (!project) {
    return null;
  }

  const selectedBadgeIcon: ProjectIconSelection = {
    pack: "badge",
    key: encodeProjectBadgeKey(badgeConfig),
  };
  const finalSelection = activeTab === "badge" ? selectedBadgeIcon : selectedIcon;
  const normalizedSearch = iconSearch.trim().toLowerCase();
  const activeIconOptions = projectIconOptionsForTab(activeTab);
  const visibleIconOptions = normalizedSearch.length === 0
    ? activeIconOptions
    : activeIconOptions.filter((option) => option.label.toLowerCase().includes(normalizedSearch));
  const streamedIconOptions = visibleIconOptions.slice(0, visibleIconLimit);
  const hasMoreIcons = streamedIconOptions.length < visibleIconOptions.length;

  // handleIconGridScroll streams the next icon batch when the user reaches the list bottom.
  // handleIconGridScroll 在用户滚动到列表底部时流式追加下一批图标。
  const handleIconGridScroll = (event: React.UIEvent<HTMLDivElement>) => {
    const element = event.currentTarget;
    if (!hasMoreIcons) {
      return;
    }
    const distanceFromBottom = element.scrollHeight - element.scrollTop - element.clientHeight;
    if (distanceFromBottom > 96) {
      return;
    }
    setVisibleIconLimit((current) => Math.min(current + PROJECT_ICON_STREAM_BATCH_SIZE, visibleIconOptions.length));
  };

  return (
    <Dialog open={Boolean(project)} onOpenChange={(open) => !open && props.onCancel()}>
      <DialogContent
        className="project-icon-dialog sm:max-w-[720px]"
        onInteractOutside={handleDialogInteractOutside}
      >
        <DialogHeader>
          <DialogTitle>设置项目图标</DialogTitle>
          <DialogDescription>
            为 {project.name} 选择固定图标，或生成一个字母徽标。还原后会重新使用默认文件夹开关图标。
          </DialogDescription>
        </DialogHeader>
        <div className="project-icon-layout">
          <div className="project-icon-preview" aria-label="项目图标预览">
            <ProjectIcon project={{ ...project, iconPack: finalSelection.pack, iconKey: finalSelection.key }} size={42} />
            <strong>{project.name}</strong>
          </div>
          <Tabs className="project-icon-tabs" value={activeTab} onValueChange={(value) => setActiveTab(value as ProjectIconTab)}>
            <TabsList className="project-icon-tabs-list">
              <TabsTrigger value="skill-icons">Skill Icons</TabsTrigger>
              <TabsTrigger value="material-icon-theme">Material Icon Theme</TabsTrigger>
              <TabsTrigger value="lucide">Lucide</TabsTrigger>
              <TabsTrigger value="badge">自定义</TabsTrigger>
            </TabsList>
            {(["skill-icons", "material-icon-theme", "lucide"] as ProjectIconPack[]).map((pack) => (
              <TabsContent className="project-icon-tab-content" key={pack} value={pack}>
                {activeTab === pack ? (
                  <>
                    <div className="project-icon-filter">
                      <Input
                        onChange={(event) => setIconSearch(event.target.value)}
                        placeholder={`过滤 ${projectIconPackLabel(pack)} 图标`}
                        value={iconSearch}
                      />
                      <span>{streamedIconOptions.length} / {visibleIconOptions.length} / {activeIconOptions.length}</span>
                    </div>
                    <div className="project-icon-grid picker-icon-grid" onScroll={handleIconGridScroll}>
                      {streamedIconOptions.map((option) => (
                        <button
                          aria-pressed={selectedIcon.pack === option.pack && selectedIcon.key === option.key}
                          className={cn(
                            "project-icon-option",
                            option.pack !== "lucide" && "color-icon-option",
                            selectedIcon.pack === option.pack && selectedIcon.key === option.key && "selected",
                          )}
                          key={`${option.pack}:${option.key}`}
                          onClick={() => setSelectedIcon({ pack: option.pack, key: option.key })}
                          title={option.label}
                          type="button"
                        >
                          {option.iconName ? (
                            <ProjectIconifyIcon iconName={option.iconName} size={26} />
                          ) : option.Icon ? (
                            <option.Icon size={22} />
                          ) : null}
                          <span>{option.label}</span>
                        </button>
                      ))}
                      {hasMoreIcons ? (
                        <button
                          className="project-icon-option icon-load-more"
                          onClick={() => setVisibleIconLimit((current) => Math.min(current + PROJECT_ICON_STREAM_BATCH_SIZE, visibleIconOptions.length))}
                          type="button"
                        >
                          加载更多
                        </button>
                      ) : null}
                    </div>
                  </>
                ) : null}
              </TabsContent>
            ))}
            <TabsContent className="project-icon-tab-content badge-tab" value="badge">
              <div className="badge-editor">
                <label>
                  <span>文字</span>
                  <Input
                    maxLength={4}
                    onChange={(event) => updateBadgeText(event.target.value)}
                    placeholder="中文 1 个或英文 2 个"
                    value={badgeConfig.text}
                  />
                </label>
                <div className="badge-editor-field">
                  <span>形状</span>
                  <Select
                    value={badgeConfig.shape}
                    onValueChange={(shape) => setBadgeConfig((current) => ({ ...current, shape: shape as ProjectIconBadgeShape }))}
                  >
                    <SelectTrigger className="badge-shape-trigger">
                      <span>{badgeShapeLabel(badgeConfig.shape)}</span>
                    </SelectTrigger>
                    <SelectContent className="project-icon-select-content" position="popper">
                      <SelectItem value="circle">圆形</SelectItem>
                      <SelectItem value="square">方形</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
                <ColorPickerRow
                  label="背景色"
                  value={badgeConfig.background}
                  onChange={(background) => setBadgeConfig((current) => ({ ...current, background }))}
                />
                <ColorPickerRow
                  label="边框色"
                  value={badgeConfig.border}
                  onChange={(border) => setBadgeConfig((current) => ({ ...current, border }))}
                />
                <ColorPickerRow
                  label="字体色"
                  value={badgeConfig.foreground}
                  onChange={(foreground) => setBadgeConfig((current) => ({ ...current, foreground }))}
                />
              </div>
            </TabsContent>
          </Tabs>
        </div>
        <DialogFooter>
          <Button type="button" variant="ghost" onClick={props.onCancel}>取消</Button>
          <Button type="button" variant="outline" onClick={() => props.onReset(project)}>还原默认</Button>
          <Button type="button" onClick={() => props.onConfirm(project, finalSelection)}>设置</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

// ColorPickerRow renders one color input with quick color swatches.
// ColorPickerRow 渲染一个带快捷色块的颜色输入。
function ColorPickerRow(props: {
  // User-facing color field label.
  // 面向用户显示的颜色字段名称。
  label: string;
  // Current hex color.
  // 当前十六进制颜色。
  value: string;
  // Updates the current hex color.
  // 更新当前十六进制颜色。
  onChange: (value: string) => void;
}) {
  return (
    <div className="color-picker-row">
      <span>{props.label}</span>
      <input
        aria-label={props.label}
        className="color-picker-input"
        onChange={(event) => props.onChange(event.target.value)}
        type="color"
        value={props.value}
      />
      <div className="color-swatch-row">
        {PROJECT_BADGE_COLORS.map((color) => (
          <button
            aria-label={`${props.label} ${color}`}
            className="color-swatch"
            key={color}
            onClick={() => props.onChange(color)}
            style={{ backgroundColor: color }}
            type="button"
          />
        ))}
      </div>
    </div>
  );
}

// badgeShapeLabel returns the Chinese label for a generated badge shape.
// badgeShapeLabel 返回生成式徽标形状的中文标签。
function badgeShapeLabel(shape: ProjectIconBadgeShape): string {
  switch (shape) {
    case "circle":
      return "圆形";
    case "square":
      return "方形";
  }
}
