import { ScanSearch } from "lucide-react";
import { Button } from "@/components/ui/button";
import { DropdownMenuItem, DropdownMenuRadioGroup, DropdownMenuRadioItem, DropdownMenuSeparator,
  DropdownMenuSub, DropdownMenuSubContent, DropdownMenuSubTrigger } from "@/components/ui/dropdown-menu";
import { useI18n } from "@/i18n";
import { VceJobProgress, vceActiveJobStatusLabel, vceSynchronizationControl } from "./vce-job-progress";
import { useVceProjectSurface, type VceSurfaceGate } from "./vce-project-surface";

// Map the explicit gate to actionable user text, not technical lifecycle enum names.
// 将明确门禁映射为可操作用户文案，而非技术生命周期枚举名。
// gate is authoritative UI priority; chinese chooses locale. Returns a concise label.
// gate 为权威界面优先级；chinese 选择语言。返回简洁标签。
export function vceSurfaceLabel(gate: VceSurfaceGate, chinese: boolean): string {
  const labels: Record<VceSurfaceGate, [string, string]> = {
    no_project: ["请选择项目", "Select a project"],
    loading: ["正在读取状态", "Loading status"],
    unavailable: ["状态不可用（点击重试）", "Status unavailable (retry)"],
    engine_off: ["引擎未启用（点击设置）", "Engine unavailable (settings)"],
    project_off: ["项目未启用（点击设置）", "Project disabled (settings)"],
    uninitialized: ["索引未初始化（点击设置）", "Index not initialized (settings)"],
    rebuild_required: ["索引需要重建（点击设置）", "Index needs rebuilding (settings)"],
    ready: ["索引可用", "Index available"],
  };
  return labels[gate][chinese ? 0 : 1];
}

// Render project-owned semantic controls inside the existing composer plus menu.
// 在既有输入区加号菜单内渲染项目自有语义控件。
// Takes no props; consumes shared project authority. Returns no item outside the workspace owner.
// 无参数；消费共享项目权威。工作区归属之外不返回菜单项。
export function VceComposerMenu() {
  const surface = useVceProjectSurface();
  const { locale } = useI18n();
  const chinese = locale !== "en-US";
  if (surface === null) return null;
  // Both the menu and project settings expose the same bounded follow-up operation.
  // 菜单与项目设置暴露相同的有界后续操作。
  const synchronization = vceSynchronizationControl(surface.jobState.job, chinese);
  // activeJobLabel keeps every compact surface aligned with the exact durable operation kind.
  // activeJobLabel 使每个紧凑界面都与持久作业的精确操作类型保持一致。
  const activeJobLabel = vceActiveJobStatusLabel(surface.jobState.job, chinese);
  const openGate = surface.gate === "engine_off" ? surface.openEngineSettings
    : surface.gate === "unavailable" ? surface.reload : surface.openProjectSettings;
  return <DropdownMenuSub>
    <DropdownMenuSubTrigger><ScanSearch size={15} /><span>{chinese ? "语义引擎" : "ContextEngine"}</span></DropdownMenuSubTrigger>
    <DropdownMenuSubContent>
      {surface.gate === "ready" && surface.activity !== null ? <>
        <DropdownMenuSub>
          <DropdownMenuSubTrigger>{chinese ? "索引更新模式" : "Index update mode"}</DropdownMenuSubTrigger>
          <DropdownMenuSubContent>
            <DropdownMenuRadioGroup value={surface.activity.settings.updateMode} onValueChange={(value) => {
              if (value === "manual" || value === "automatic") void surface.saveMode(value);
            }}>
              <DropdownMenuRadioItem value="manual" disabled={surface.pending}>{chinese ? "手动更新" : "Manual"}</DropdownMenuRadioItem>
              <DropdownMenuRadioItem value="automatic" disabled={surface.pending}>{chinese ? "自动更新" : "Automatic"}</DropdownMenuRadioItem>
            </DropdownMenuRadioGroup>
          </DropdownMenuSubContent>
        </DropdownMenuSub>
        <DropdownMenuItem disabled={surface.pending || synchronization.disabled} onSelect={() => { void surface.synchronize(); }}>
          {synchronization.label}
        </DropdownMenuItem>
      </> : <DropdownMenuItem disabled={surface.gate === "loading" || surface.gate === "no_project"} onSelect={openGate}>
        {vceSurfaceLabel(surface.gate, chinese)}
      </DropdownMenuItem>}
      {surface.jobState.active ? <><DropdownMenuSeparator /><DropdownMenuItem onSelect={surface.openProjectSettings}>
        {activeJobLabel ?? (chinese ? "作业进行中" : "Job in progress")}{chinese ? "，查看进度" : " — view details"}
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
  const { locale } = useI18n();
  const chinese = locale !== "en-US";
  if (surface === null || !surface.visible) return null;
  // Summarize execution only after prerequisite errors have been given priority.
  // 仅在优先呈现前提错误之后概括执行状态。
  let label = vceSurfaceLabel(surface.gate, chinese);
  // Source evidence never replaces query admission; unknown is not a failed index.
  // 源码证据绝不替代查询准入；未知不代表索引失败。
  const freshness = surface.activity?.sourceFreshness;
  const freshnessLabel = freshness?.pendingChanges ? (chinese ? "有待同步修改" : "Changes awaiting sync")
    : freshness?.reconciliationRequired ? (chinese ? "新鲜度待确认" : "Freshness unverified")
    : (chinese ? "已同步至最近观察" : "Synced through last observation");
  if (surface.gate === "ready") label = freshnessLabel;
  if (surface.gate === "ready" || surface.gate === "uninitialized") {
    const activeJobLabel = vceActiveJobStatusLabel(surface.jobState.job, chinese);
    if (activeJobLabel !== null) label = activeJobLabel;
    else if (surface.jobState.job?.state === "failed") label = chinese ? "索引更新失败" : "Index update failed";
    else if (surface.jobState.job?.state === "cancelled") label = chinese ? "索引更新已取消" : "Index update cancelled";
    else if (surface.jobState.job?.state === "interrupted") label = chinese ? "索引任务中断" : "Index job interrupted";
  }
  if (props.collapsed) return <span className="conversation-session-tool-collapsed-row">
    <ScanSearch size={15} /><span>{chinese ? "语义索引" : "Semantic index"}</span><strong>{label}</strong>
  </span>;
  const lastSuccess = surface.activity?.lastSuccessfulUpdateUnixMs;
  // The enclosing tool menu uses nowrap; semantic diagnostics must explicitly opt back into wrapping.
  // 外层工具菜单使用不换行；语义诊断必须明确恢复换行，避免长错误撑宽浮窗。
  return <section className="min-w-0 space-y-2 whitespace-normal border-t px-3 py-3 [overflow-wrap:anywhere]" aria-label={chinese ? "项目语义索引" : "Project semantic index"}>
    <div className="flex items-center justify-between gap-2 text-sm">
      <strong className="flex items-center gap-2"><ScanSearch size={15} />{chinese ? "语义索引" : "Semantic index"}</strong>
      <Button type="button" variant="ghost" size="sm" onClick={surface.gate === "engine_off" ? surface.openEngineSettings : surface.openProjectSettings}>
        {chinese ? "设置" : "Settings"}
      </Button>
    </div>
    <p className="text-xs text-muted-foreground">{label}</p>
    {surface.gate === "ready" && label !== freshnessLabel ? <p className="text-xs text-muted-foreground">{freshnessLabel}</p> : null}
    <p className="text-xs text-muted-foreground">{chinese ? "上次成功更新：" : "Last successful update: "}{lastSuccess != null
      ? new Date(lastSuccess).toLocaleString(locale) : chinese ? "暂无可验证记录" : "No verified record"}</p>
    {surface.jobState.job !== null ? <VceJobProgress job={surface.jobState.job} chinese={chinese}
      onRetry={!surface.pending && (surface.gate === "ready" || surface.gate === "uninitialized") ? () => { void surface.retryLatest(); } : undefined}
      queryAvailable={surface.gate === "ready"} onCancel={() => { void surface.jobState.cancel(); }} cancelPending={surface.jobState.cancelPending} /> : null}
    {surface.error || surface.jobState.error ? <p className="break-words text-xs text-destructive">{surface.error ?? surface.jobState.error}</p> : null}
    {surface.activity?.status.lastError ? <p className="break-words text-xs text-destructive">{surface.activity.status.lastError}</p> : null}
  </section>;
}
