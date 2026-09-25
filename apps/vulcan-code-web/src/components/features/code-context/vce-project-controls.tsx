import { ScanSearch } from "lucide-react";
import { Button } from "@/components/ui/button";
import { DropdownMenuItem, DropdownMenuRadioGroup, DropdownMenuRadioItem, DropdownMenuSeparator,
  DropdownMenuSub, DropdownMenuSubContent, DropdownMenuSubTrigger } from "@/components/ui/dropdown-menu";
import { useI18n, type Translate, type TranslationKey } from "@/i18n";
import { VceJobProgress, vceActiveJobStatusLabel, vceSynchronizationControl } from "./vce-job-progress";
import { useVceProjectSurface, type VceSurfaceGate } from "./vce-project-surface";
import { formatDateTime } from "@/lib/display-format";

// Map the explicit gate to actionable user text, not technical lifecycle enum names.
// 将明确门禁映射为可操作用户文案，而非技术生命周期枚举名。
// gate is authoritative UI priority; t is the active-locale lookup. Returns a concise label.
// gate 为权威界面优先级；t 为当前语言查询。返回简洁标签。
export function vceSurfaceLabel(gate: VceSurfaceGate, t: Translate): string {
  const labels: Record<VceSurfaceGate, TranslationKey> = {
    no_project: "codeContext.project.gate.noProject",
    loading: "codeContext.project.gate.loading",
    unavailable: "codeContext.project.gate.unavailable",
    engine_off: "codeContext.project.gate.engineOff",
    project_off: "codeContext.project.gate.projectOff",
    uninitialized: "codeContext.project.gate.uninitialized",
    rebuild_required: "codeContext.project.gate.rebuildRequired",
    ready: "codeContext.project.gate.ready",
  };
  return t(labels[gate]);
}

// Render project-owned semantic controls inside the existing composer plus menu.
// 在既有输入区加号菜单内渲染项目自有语义控件。
// Takes no props; consumes shared project authority. Returns no item outside the workspace owner.
// 无参数；消费共享项目权威。工作区归属之外不返回菜单项。
export function VceComposerMenu() {
  const surface = useVceProjectSurface();
  const { t } = useI18n();
  if (surface === null) return null;
  // Both the menu and project settings expose the same bounded follow-up operation.
  // 菜单与项目设置暴露相同的有界后续操作。
  const synchronization = vceSynchronizationControl(surface.jobState.job, t);
  // activeJobLabel keeps every compact surface aligned with the exact durable operation kind.
  // activeJobLabel 使每个紧凑界面都与持久作业的精确操作类型保持一致。
  const activeJobLabel = vceActiveJobStatusLabel(surface.jobState.job, t);
  const openGate = surface.gate === "engine_off" ? surface.openEngineSettings
    : surface.gate === "unavailable" ? surface.reload : surface.openProjectSettings;
  return <DropdownMenuSub>
    <DropdownMenuSubTrigger><ScanSearch size={15} /><span>{t("codeContext.project.contextengine")}</span></DropdownMenuSubTrigger>
    <DropdownMenuSubContent>
      {surface.gate === "ready" && surface.activity !== null ? <>
        <DropdownMenuSub>
          <DropdownMenuSubTrigger>{t("codeContext.project.indexUpdateMode")}</DropdownMenuSubTrigger>
          <DropdownMenuSubContent>
            <DropdownMenuRadioGroup value={surface.activity.settings.updateMode} onValueChange={(value) => {
              if (value === "manual" || value === "automatic") void surface.saveMode(value);
            }}>
              <DropdownMenuRadioItem value="manual" disabled={surface.pending}>{t("codeContext.project.manual")}</DropdownMenuRadioItem>
              <DropdownMenuRadioItem value="automatic" disabled={surface.pending}>{t("codeContext.project.automatic")}</DropdownMenuRadioItem>
            </DropdownMenuRadioGroup>
          </DropdownMenuSubContent>
        </DropdownMenuSub>
        <DropdownMenuItem disabled={surface.pending || synchronization.disabled} onSelect={() => { void surface.synchronize(); }}>
          {synchronization.label}
        </DropdownMenuItem>
      </> : <DropdownMenuItem disabled={surface.gate === "loading" || surface.gate === "no_project"} onSelect={openGate}>
        {vceSurfaceLabel(surface.gate, t)}
      </DropdownMenuItem>}
      {surface.jobState.active ? <><DropdownMenuSeparator /><DropdownMenuItem onSelect={surface.openProjectSettings}>
        {activeJobLabel ?? t("codeContext.project.jobInProgress")}{t("codeContext.project.viewDetails")}
      </DropdownMenuItem></> : null}
    </DropdownMenuSubContent>
  </DropdownMenuSub>;
}

// Render the semantic node independently from Git repository detection.
// 独立于 Git 仓库检测渲染语义节点。
// collapsed selects the existing compact floating-row layout; returns nothing for disabled projects.
// collapsed 选择既有紧凑浮动行布局；关闭项目时不返回节点。
export function VceFloatingNode(props: {
  // Compact rows live inside the panel's existing expand button and must not nest buttons.
  // 紧凑行位于面板既有展开按钮中，不能嵌套按钮。
  collapsed?: boolean;
}) {
  const surface = useVceProjectSurface();
  const { t, locale } = useI18n();
  if (surface === null || !surface.visible) return null;
  // Summarize execution only after prerequisite errors have been given priority.
  // 仅在优先呈现前提错误之后概括执行状态。
  let label = vceSurfaceLabel(surface.gate, t);
  // Source evidence never replaces query admission; unknown is not a failed index.
  // 源码证据绝不替代查询准入；未知不代表索引失败。
  const freshness = surface.activity?.sourceFreshness;
  const freshnessLabel = freshness?.pendingChanges ? t("codeContext.project.changesAwaitingSync")
    : freshness?.reconciliationRequired ? t("codeContext.project.freshnessUnverified")
    : t("codeContext.project.syncedThroughLastObservation");
  if (surface.gate === "ready") label = freshnessLabel;
  if (surface.gate === "ready" || surface.gate === "uninitialized") {
    const activeJobLabel = vceActiveJobStatusLabel(surface.jobState.job, t);
    if (activeJobLabel !== null) label = activeJobLabel;
    else if (surface.jobState.job?.state === "failed") label = t("codeContext.project.indexUpdateFailed");
    else if (surface.jobState.job?.state === "cancelled") label = t("codeContext.project.indexUpdateCancelled");
    else if (surface.jobState.job?.state === "interrupted") label = t("codeContext.project.indexJobInterrupted");
  }
  if (props.collapsed) return <span className="conversation-session-tool-collapsed-row">
    <ScanSearch size={15} /><span>{t("codeContext.project.semanticIndex")}</span><strong>{label}</strong>
  </span>;
  const lastSuccess = surface.activity?.lastSuccessfulUpdateUnixMs;
  // The enclosing tool menu uses nowrap; semantic diagnostics must explicitly opt back into wrapping.
  // 外层工具菜单使用不换行；语义诊断必须明确恢复换行，避免长错误撑宽浮窗。
  return <section className="min-w-0 space-y-2 whitespace-normal border-t px-3 py-3 [overflow-wrap:anywhere]" aria-label={t("codeContext.project.projectSemanticIndex")}>
    <div className="flex items-center justify-between gap-2 text-sm">
      <strong className="flex items-center gap-2"><ScanSearch size={15} />{t("codeContext.project.semanticIndex")}</strong>
      <Button type="button" variant="ghost" size="sm" onClick={surface.gate === "engine_off" ? surface.openEngineSettings : surface.openProjectSettings}>
        {t("codeContext.project.settings")}
      </Button>
    </div>
    <p className="text-xs text-muted-foreground">{label}</p>
    {surface.gate === "ready" && label !== freshnessLabel ? <p className="text-xs text-muted-foreground">{freshnessLabel}</p> : null}
    <p className="text-xs text-muted-foreground">{t("codeContext.project.lastSuccessfulUpdate")}{lastSuccess != null
      ? formatDateTime(lastSuccess, locale) : t("codeContext.project.noVerifiedRecord")}</p>
    {surface.jobState.job !== null ? <VceJobProgress job={surface.jobState.job}
      onRetry={!surface.pending && (surface.gate === "ready" || surface.gate === "uninitialized") ? () => { void surface.retryLatest(); } : undefined}
      queryAvailable={surface.gate === "ready"} onCancel={() => { void surface.jobState.cancel(); }} cancelPending={surface.jobState.cancelPending} /> : null}
    {surface.error || surface.jobState.error ? <p className="break-words text-xs text-destructive">{surface.error ?? surface.jobState.error}</p> : null}
    {surface.activity?.status.lastError && surface.activity.status.lastError !== surface.jobState.job?.error ? <p className="break-words text-xs text-destructive">{surface.activity.status.lastError}</p> : null}
  </section>;
}
