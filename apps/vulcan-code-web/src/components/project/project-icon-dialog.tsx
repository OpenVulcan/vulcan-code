import React from "react";
import { Button } from "@/ui-kit/primitives/button";
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/ui-kit/primitives/dialog";
import { Input } from "@/ui-kit/primitives/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
} from "@/ui-kit/primitives/select";
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@/ui-kit/primitives/tabs";
import { useI18n, type TranslationKey } from "@/i18n";
import { cn } from "@/lib/utils";
import type { ProjectSummary } from "@/types";
import {
  DEFAULT_PROJECT_BADGE_CONFIG,
  PROJECT_BADGE_COLORS,
  PROJECT_ICON_STREAM_BATCH_SIZE,
  ProjectIconifyIcon,
  encodeProjectBadgeKey,
  decodeProjectBadgeKey,
  isIconifyProjectIconPack,
  normalizeProjectBadgeText,
  projectIconPackLabel,
  type ProjectIconBadgeConfig,
  type ProjectIconBadgeShape,
  type ProjectIconPack,
  type ProjectIconOption,
  type ProjectIconSelection,
  type ProjectIconTab,
} from "./project-icon-model";
import { loadProjectIconCatalog } from "./project-icon-catalog-loader";
import { ProjectIcon } from "./project-icons";
import type { ProjectIconDialogTarget } from "./project-icon-contract";

export type { ProjectIconDialogTarget } from "./project-icon-contract";

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
  onConfirm: (project: ProjectSummary, icon: ProjectIconSelection) => Promise<void>;
  // Resets the selected project to default folder icons.
  // 将选中项目还原为默认文件夹图标。
  onReset: (project: ProjectSummary) => Promise<void>;
}) {
  const { t } = useI18n();
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
  // activeIconOptions stores only the catalog owned by the visible picker tab.
  // activeIconOptions 只保存当前可见选择器标签所属的图标目录。
  const [activeIconOptions, setActiveIconOptions] = React.useState<ProjectIconOption[]>([]);
  // Operation state belongs to the picker and does not imply successful persistence before the RPC settles.
  // 操作状态属于选择器，在 RPC 完成前不表示持久化成功。
  const [busy, setBusy] = React.useState(false);
  const savingRef = React.useRef(false);
  const [error, setError] = React.useState<string | null>(null);
  // Catalog state distinguishes unavailable imports from an empty search result.
  // 目录状态区分导入不可用与搜索结果为空。
  const [catalogError, setCatalogError] = React.useState<string | null>(null);
  const [catalogLoading, setCatalogLoading] = React.useState(false);
  const [catalogRevision, setCatalogRevision] = React.useState(0);

  React.useEffect(() => {
    setError(null);
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
    const key = project.iconPack === "lucide" && project.iconKey ? project.iconKey : "Folder";
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

  React.useEffect(() => {
    setActiveIconOptions([]);
    setCatalogError(null);
    if (activeTab === "badge") {
      setCatalogLoading(false);
      return;
    }
    // cancelled prevents a slower previous tab import from replacing the current catalog.
    // cancelled 防止较慢的前一标签导入覆盖当前图标目录。
    let cancelled = false;
    setCatalogLoading(true);
    void loadProjectIconCatalog(activeTab).then((catalog) => {
      if (!cancelled) {
        setActiveIconOptions(catalog.PROJECT_ICON_OPTIONS);
      }
    }).catch((failure) => {
      if (!cancelled) setCatalogError(failure instanceof Error ? failure.message : String(failure));
    }).finally(() => {
      if (!cancelled) setCatalogLoading(false);
    });
    return () => {
      cancelled = true;
    };
  }, [activeTab, catalogRevision]);

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
    if (savingRef.current) { event.preventDefault(); return; }
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
  // persist keeps the selected icon intact on failure and serializes save and reset together.
  // persist 在失败时保持所选图标，并将保存与还原共同串行化。
  const persist = async (reset: boolean) => {
    if (savingRef.current) return;
    savingRef.current = true; setBusy(true); setError(null);
    try { if (reset) await props.onReset(project); else await props.onConfirm(project, finalSelection); }
    catch (failure) { setError(failure instanceof Error ? failure.message : String(failure)); }
    finally { savingRef.current = false; setBusy(false); }
  };
  const normalizedSearch = iconSearch.trim().toLowerCase();
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
    <Dialog open={Boolean(project)} onOpenChange={(open) => !open && !savingRef.current && props.onCancel()}>
      <DialogContent
        className="project-icon-dialog flex min-h-0 flex-col"
        size="xl"
        scrollMode="fixed-footer"
        showCloseButton={!busy}
        onEscapeKeyDown={event => { if (savingRef.current) event.preventDefault(); }}
        onInteractOutside={handleDialogInteractOutside}
      >
        <DialogHeader className="shrink-0">
          <DialogTitle>{t("projectIcon.title")}</DialogTitle>
          <DialogDescription>
            {t("projectIcon.description").replace("{project}", project.name)}
          </DialogDescription>
        </DialogHeader>
        <fieldset disabled={busy} className="project-icon-layout min-h-0 min-w-0 flex-1 overflow-y-auto">
          <div className="project-icon-preview" aria-label={t("projectIcon.previewAria")}>
            <ProjectIcon project={{ ...project, iconPack: finalSelection.pack, iconKey: finalSelection.key }} size={42} />
            <strong>{project.name}</strong>
          </div>
          <Tabs className="project-icon-tabs" value={activeTab} onValueChange={(value) => setActiveTab(value as ProjectIconTab)}>
            <TabsList className="project-icon-tabs-list">
              <TabsTrigger value="skill-icons">Skill Icons</TabsTrigger>
              <TabsTrigger value="material-icon-theme">Material Icon Theme</TabsTrigger>
              <TabsTrigger value="lucide">Lucide</TabsTrigger>
              <TabsTrigger value="badge">{t("projectIcon.customTab")}</TabsTrigger>
            </TabsList>
            {(["skill-icons", "material-icon-theme", "lucide"] as ProjectIconPack[]).map((pack) => (
              <TabsContent className="project-icon-tab-content" key={pack} value={pack}>
                {activeTab === pack ? (
                  <>
                    <div className="project-icon-filter">
                      <Input
                        aria-label={t("projectIcon.filterPlaceholder").replace("{pack}", projectIconPackLabel(pack))}
                        onChange={(event) => setIconSearch(event.target.value)}
                        placeholder={t("projectIcon.filterPlaceholder").replace("{pack}", projectIconPackLabel(pack))}
                        value={iconSearch}
                      />
                      <span>{streamedIconOptions.length} / {visibleIconOptions.length} / {activeIconOptions.length}</span>
                    </div>
                    {catalogLoading && <StreamPlaceholder label={t("project.icon.loadingIconCatalog")} shape="tiles" count={6} />}
                    {catalogError && <div role="alert"><p className="text-sm text-destructive">{catalogError}</p><Button type="button" size="sm" variant="outline" onClick={() => setCatalogRevision(value => value + 1)}>{t("project.icon.retry")}</Button></div>}
                    {!catalogLoading && !catalogError && visibleIconOptions.length === 0 && <p>{t("project.icon.noMatchingIcons")}</p>}
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
                          {t("projectIcon.loadMore")}
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
                  <span>{t("projectIcon.text")}</span>
                  <Input
                    maxLength={4}
                    onChange={(event) => updateBadgeText(event.target.value)}
                    placeholder={t("projectIcon.textPlaceholder")}
                    value={badgeConfig.text}
                  />
                </label>
                <div className="badge-editor-field">
                  <span>{t("projectIcon.shape")}</span>
                  <Select
                    disabled={busy}
                    value={badgeConfig.shape}
                    onValueChange={(shape) => setBadgeConfig((current) => ({ ...current, shape: shape as ProjectIconBadgeShape }))}
                  >
                    <SelectTrigger className="badge-shape-trigger">
                      <span>{badgeShapeLabel(badgeConfig.shape, t)}</span>
                    </SelectTrigger>
                    <SelectContent className="project-icon-select-content">
                      <SelectItem value="circle">{t("projectIcon.shapeCircle")}</SelectItem>
                      <SelectItem value="square">{t("projectIcon.shapeSquare")}</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
                <ColorPickerRow
                  label={t("projectIcon.backgroundColor")}
                  value={badgeConfig.background}
                  onChange={(background) => setBadgeConfig((current) => ({ ...current, background }))}
                />
                <ColorPickerRow
                  label={t("projectIcon.borderColor")}
                  value={badgeConfig.border}
                  onChange={(border) => setBadgeConfig((current) => ({ ...current, border }))}
                />
                <ColorPickerRow
                  label={t("projectIcon.foregroundColor")}
                  value={badgeConfig.foreground}
                  onChange={(foreground) => setBadgeConfig((current) => ({ ...current, foreground }))}
                />
              </div>
            </TabsContent>
          </Tabs>
          {error && <p role="alert" className="text-sm text-destructive">{error}</p>}
        </fieldset>
        <DialogFooter className="shrink-0">
          <Button disabled={busy} type="button" variant="ghost" onClick={props.onCancel}>{t("conversation.action.cancel")}</Button>
          <Button disabled={busy} type="button" variant="outline" onClick={() => void persist(true)}>{t("projectIcon.resetDefault")}</Button>
          <Button disabled={busy} type="button" onClick={() => void persist(false)}>{busy ? t("project.icon.saving") : t("projectIcon.set")}</Button>
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

// badgeShapeLabel returns the localized label for a generated badge shape.
// badgeShapeLabel 返回生成式徽标形状的本地化标签。
function badgeShapeLabel(shape: ProjectIconBadgeShape, t: (key: TranslationKey) => string): string {
  switch (shape) {
    case "circle":
      return t("projectIcon.shapeCircle");
    case "square":
      return t("projectIcon.shapeSquare");
  }
}
