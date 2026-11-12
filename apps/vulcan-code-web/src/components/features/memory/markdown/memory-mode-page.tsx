import * as React from "react";
import { PageEmptyState, PageHint, PageItem, PageItemList, PageNotice, PageRow, PageSection } from "@/components/page";
import { Badge } from "@/ui-kit/primitives/badge";
import { Button } from "@/ui-kit/primitives/button";
import { SegmentedControl, SegmentedControlItem } from "@/ui-kit/primitives/choice-controls";
import { Input } from "@/ui-kit/primitives/input";
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";
import { Switch } from "@/ui-kit/primitives/switch";
import { toast } from "@/ui-kit/primitives/toast";
import { formatMessage, useI18n } from "@/i18n";
import type { TranslationKey } from "@/i18n-messages";
import { errorMessage } from "@/lib/display-format";
import {
  applyMarkdownMemoryProjectAction,
  loadMarkdownMemory,
  saveMemorySettings,
} from "@/service/markdown-memory-service";
import type { MarkdownMemoryProject, MemoryBudgetPart, MemoryInjectionBudget, MemoryMode } from "@/types";
import { MemoryTransferSection } from "./markdown-memory-transfer";
import { publishMemorySettings, retryMemorySettings, useMemorySettings, useMemorySettingsLoadError } from "./memory-mode-store";

// MODE_OPTIONS lists the modes with their label and explanation keys.
// MODE_OPTIONS 列出各模式及其标签与说明键。
const MODE_OPTIONS: ReadonlyArray<{ hint: TranslationKey; label: TranslationKey; value: MemoryMode }> = [
  { hint: "memory.markdown.mode.offHint", label: "memory.markdown.mode.off", value: "off" },
  { hint: "memory.markdown.mode.vmmHint", label: "memory.markdown.mode.vmm", value: "vmm" },
  { hint: "memory.markdown.mode.markdownHint", label: "memory.markdown.mode.markdown", value: "markdown" },
];

// BUDGET_PARTS orders the budget fields: full-text parts first, then catalogs.
// BUDGET_PARTS 排列预算字段：先全文部分，再目录。
export const BUDGET_PARTS: ReadonlyArray<{ label: TranslationKey; part: MemoryBudgetPart }> = [
  { label: "memory.markdown.part.user", part: "user" },
  { label: "memory.markdown.part.spaceBase", part: "spaceBase" },
  { label: "memory.markdown.part.projectBase", part: "projectBase" },
  { label: "memory.markdown.part.spaceCatalog", part: "spaceCatalog" },
  { label: "memory.markdown.part.projectCatalog", part: "projectCatalog" },
];

// Shows the global mode and displays the supplied VMM controls when VMM is selected for configuration.
// 显示全局记忆模式，并在选择配置 VMM 时展示传入的 VMM 控件。
export function MemoryModePage({ vmmContent }: { vmmContent?: React.ReactNode }) {
  const { t } = useI18n();
  const settings = useMemorySettings();
  const loadError = useMemorySettingsLoadError();
  const [mode, setMode] = React.useState<MemoryMode | null>(null);
  const [budget, setBudget] = React.useState<Record<MemoryBudgetPart, string> | null>(null);
  const [saving, setSaving] = React.useState(false);
  // The draft starts from the saved settings once they arrive and after every save.
  // 草稿在已保存设置到达时及每次保存后从其开始。
  React.useEffect(() => {
    if (!settings) return;
    setMode(settings.mode);
    setBudget(budgetDraft(settings.budget));
  }, [settings]);

  if (!settings && loadError) {
    return <PageNotice tone="error" details={loadError} detailsLabel={t("settings.errors.showErrorDetails")}
      actions={<Button type="button" size="sm" variant="outline" onClick={retryMemorySettings}>{t("composer.capability.retry")}</Button>}>
      {t("memory.markdown.loadFailed")}
    </PageNotice>;
  }
  if (!settings || mode === null || budget === null) {
    return <StreamPlaceholder label={t("memory.markdown.loading")} shape="rows" count={3} />;
  }
  const parsed = parseBudget(budget, settings.maxPartTokens);
  const changed = mode !== settings.mode
    || (parsed !== null && BUDGET_PARTS.some(({ part }) => parsed[part] !== settings.budget[part]));
  const save = async () => {
    if (parsed === null) return;
    setSaving(true);
    try {
      publishMemorySettings(await saveMemorySettings({ mode, budget: parsed }));
      toast.success(t("memory.markdown.saved"));
    } catch (error) {
      toast.error(t("memory.markdown.saveFailed"), { description: errorMessage(error) });
    } finally {
      setSaving(false);
    }
  };
  const selected = MODE_OPTIONS.find((option) => option.value === mode) ?? MODE_OPTIONS[0];
  const total = parsed ? BUDGET_PARTS.reduce((sum, { part }) => sum + parsed[part], 0) : null;

  return <>
    <PageSection layout="rows" data-memory-mode="" title={t("memory.markdown.mode.title")}
      description={t("memory.markdown.mode.hint")}
      footer={<Button type="button" size="sm" data-memory-mode-save="" disabled={saving || !changed || parsed === null}
        onClick={() => void save()}>{t("memory.markdown.save")}</Button>}>
      <PageRow label={t("memory.markdown.mode.title")} stacked>
        <SegmentedControl aria-label={t("memory.markdown.mode.title")} disabled={saving}
          onValueChange={(value) => setMode(value as MemoryMode)} value={mode}>
          {MODE_OPTIONS.map((option) => <SegmentedControlItem key={option.value} value={option.value}>{t(option.label)}</SegmentedControlItem>)}
        </SegmentedControl>
        <PageHint>{t(selected.hint)}</PageHint>
      </PageRow>
      {mode === "markdown" ? <>
        {BUDGET_PARTS.map(({ label, part }) => <PageRow key={part} label={t(label)} htmlFor={`memory-budget-${part}`}>
          <Input id={`memory-budget-${part}`} className="vk-mm-budget-input" type="number" inputMode="numeric" min={0}
            max={settings.maxPartTokens} step={50} disabled={saving} value={budget[part]}
            aria-invalid={parsed === null && !validPart(budget[part], settings.maxPartTokens) ? true : undefined}
            onChange={(event) => setBudget({ ...budget, [part]: event.target.value })} />
        </PageRow>)}
        <PageRow label={t("memory.markdown.budget.title")} description={t("memory.markdown.budget.hint")}>
          <div className="vk-mm-budget-total">
            <span>{total === null
              ? formatMessage(t("memory.markdown.budget.invalid"), { max: settings.maxPartTokens })
              : formatMessage(t("memory.markdown.budget.total"), { tokens: total })}</span>
            <Button type="button" size="sm" variant="outline" disabled={saving}
              onClick={() => setBudget(budgetDraft(settings.defaultBudget))}>{t("memory.markdown.budget.reset")}</Button>
          </div>
        </PageRow>
      </> : null}
    </PageSection>
    {mode === "vmm" ? vmmContent : null}
    {mode === "markdown" && settings.mode === "markdown" ? <MarkdownMemorySections /> : null}
  </>;
}

// MarkdownMemorySections shows the memory projects and the import/export tools; an import reloads the projects.
// MarkdownMemorySections 显示记忆项目与导入导出工具；导入后重新加载项目。
function MarkdownMemorySections() {
  const [generation, setGeneration] = React.useState(0);
  return <>
    <MemoryProjectsSection key={generation} />
    <MemoryTransferSection onImported={() => setGeneration((value) => value + 1)} />
  </>;
}

// MemoryProjectsSection lists memory projects with their per-project switch.
// MemoryProjectsSection 列出记忆项目及其逐项目开关。
function MemoryProjectsSection() {
  const { t } = useI18n();
  const [projects, setProjects] = React.useState<MarkdownMemoryProject[] | null>(null);
  const [loadError, setLoadError] = React.useState<string | null>(null);
  const [busyId, setBusyId] = React.useState<string | null>(null);
  const load = React.useCallback(async () => {
    setLoadError(null);
    try {
      setProjects((await loadMarkdownMemory({})).projects);
    } catch (error) {
      setLoadError(errorMessage(error));
    }
  }, []);
  React.useEffect(() => { void load(); }, [load]);
  const toggle = async (project: MarkdownMemoryProject, enabled: boolean) => {
    setBusyId(project.id);
    try {
      setProjects(await applyMarkdownMemoryProjectAction({ action: "set_enabled", memoryProjectId: project.id, enabled }));
    } catch (error) {
      toast.error(t("memory.markdown.saveFailed"), { description: errorMessage(error) });
    } finally {
      setBusyId(null);
    }
  };
  return <PageSection data-memory-projects="" title={t("memory.markdown.projects.title")} description={t("memory.markdown.projects.hint")}>
    {loadError ? <PageNotice tone="error" details={loadError} detailsLabel={t("settings.errors.showErrorDetails")}
      actions={<Button type="button" size="sm" variant="outline" onClick={() => void load()}>{t("composer.capability.retry")}</Button>}>
      {t("memory.markdown.loadFailed")}
    </PageNotice> : projects === null ? <StreamPlaceholder label={t("memory.markdown.loading")} shape="rows" count={2} />
      : projects.length === 0 ? <PageEmptyState title={t("memory.markdown.projects.empty")} />
        : <PageItemList>
          {projects.map((project) => <PageItem key={project.id} data-memory-project={project.id} title={project.name}
            description={project.paths.join(" · ")}
            meta={project.orphaned ? <Badge variant="warning">{t("memory.markdown.projects.orphaned")}</Badge> : undefined}
            actions={<Switch aria-label={t("memory.markdown.projects.enabled")} checked={project.memoryEnabled}
              disabled={busyId !== null} onCheckedChange={(checked) => void toggle(project, checked)} />} />)}
        </PageItemList>}
  </PageSection>;
}

// budgetDraft turns a budget into editable text fields.
// budgetDraft 将预算转为可编辑文本字段。
function budgetDraft(budget: MemoryInjectionBudget): Record<MemoryBudgetPart, string> {
  return Object.fromEntries(BUDGET_PARTS.map(({ part }) => [part, String(budget[part])])) as Record<MemoryBudgetPart, string>;
}

// validPart reports whether one field holds a whole number within the limit.
// validPart 判断单个字段是否为上限内的整数。
function validPart(text: string, max: number): boolean {
  const value = Number(text);
  return text.trim() !== "" && Number.isInteger(value) && value >= 0 && value <= max;
}

// parseBudget returns the budget when every field is valid, otherwise null.
// parseBudget 在全部字段有效时返回预算，否则返回 null。
function parseBudget(draft: Record<MemoryBudgetPart, string>, max: number): MemoryInjectionBudget | null {
  if (!BUDGET_PARTS.every(({ part }) => validPart(draft[part], max))) return null;
  return Object.fromEntries(BUDGET_PARTS.map(({ part }) => [part, Number(draft[part])])) as unknown as MemoryInjectionBudget;
}
