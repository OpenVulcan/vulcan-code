import * as React from "react";
import { ArrowRight, Copy, FolderCog } from "lucide-react";
import { Button } from "@/ui-kit/primitives/button";
import { InlineStreamPlaceholder, StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";
import { InlineDisclosure } from "@/ui-kit/primitives/disclosure";
import { useI18n } from "@/i18n";
import { listProjectKnowledgeBindings, subscribeKnowledgeProjectBindingsChanged } from "@/service/knowledge-base-service";
import { runVmmMemoryAction } from "@/service/memory-actions-service";
import type { KnowledgeProjectBinding, ProjectSummary, VmmConfigDraftSummary, VmmMemorySnapshot, VmmNativeProjectCatalogSummary } from "@/types";
import type { ProjectSettingsSectionId } from "./project-settings-types";
import "@/ui-kit/styles/features/project-overview.css";

// ProjectOverviewSection summarizes exact project authority and routes to existing settings without editing it.
// ProjectOverviewSection 汇总精确项目权威并进入既有设置，不修改项目状态。
// Parameters: props carries loaded settings, source errors and project-scoped navigation. Returns: a compact overview.
// 参数：props 提供已加载设置、来源错误和项目作用域导航。返回值：紧凑的项目概览。
export function ProjectOverviewSection(props: {
  // Exact project identity and its auxiliary activity/storage metadata.
  // 精确项目身份及其辅助活动和存储元信息。
  project: ProjectSummary; attachmentRoot: string; conversationCount: number;
  // Loaded project draft and authoritative read state; null never means inherited settings.
  // 已加载项目草稿与权威读取状态；null 绝不代表继承设置。
  vmmDraft: VmmConfigDraftSummary | null; vmmLoading: boolean; vmmError: string | null; vmmDirty: boolean;
  // Existing settings invalidation refreshes the catalog after binding changes while this overview stays open.
  // 既有设置失效版本在概览保持打开时于绑定变化后刷新目录。
  vmmRevision: number;
  // The snapshot retains its selected-project identity, required before displaying session runtime readiness.
  // 快照保留选中项目身份，显示会话运行就绪前必须核对。
  memory: Pick<VmmMemorySnapshot, "selectedProjectId" | "runtime">;
  // Skill override authority distinguishes successful empty results from failed or pending reads.
  // 技能覆盖权威区分成功空结果与失败或待定读取。
  agentOverrides: Record<string, string>; agentLoaded: boolean; agentLoading: boolean; agentError: string | null;
  // Selects one existing settings section in the same project dialog.
  // 在同一项目弹窗中选择一个既有设置分区。
  onOpenSection: (section: ProjectSettingsSectionId) => void;
}) {
  // t resolves existing user-facing labels.
  // t 解析既有用户文案。
  const { t } = useI18n();
  // knowledge and catalog retain the project identity which owns each asynchronous result.
  // knowledge 与 catalog 保留拥有每个异步结果的项目身份。
  const [knowledge, setKnowledge] = React.useState<{ projectId: string; rows: KnowledgeProjectBinding[] | null; error: string | null } | null>(null);
  const [catalog, setCatalog] = React.useState<{ projectId: string; value: VmmNativeProjectCatalogSummary | null; error: string | null } | null>(null);
  // copied describes this explicit clipboard operation without obscuring the selectable path.
  // copied 描述此次显式剪贴板操作，不遮挡可选择路径。
  const [copied, setCopied] = React.useState<"copied" | "copyFailed" | null>(null);
  React.useEffect(() => {
    // active and sequence fence unmounted projects and superseded binding refreshes independently.
    // active 与 sequence 分别隔离已卸载项目及被取代的绑定刷新。
    let active = true;
    let sequence = 0;
    // reload reads only existing authorization rows; failures remain visible instead of becoming zero bindings.
    // reload 仅读取既有授权记录；失败保持可见，不转成零绑定。
    const reload = async () => {
      // requestSequence identifies this exact read within the mounted project.
      // requestSequence 标识当前挂载项目内的此次精确读取。
      const requestSequence = ++sequence;
      setKnowledge({ projectId: props.project.id, rows: null, error: null });
      try {
        // rows contains active-library bindings for the exact requested project.
        // rows 包含准确请求项目的活动知识库绑定。
        const rows = await listProjectKnowledgeBindings(props.project.id);
        if (active && sequence === requestSequence) setKnowledge({ projectId: props.project.id, rows, error: null });
      } catch (error) {
        if (active && sequence === requestSequence) setKnowledge({ projectId: props.project.id, rows: null, error: error instanceof Error ? error.message : String(error) });
      }
    };
    void reload();
    // unsubscribe removes only this overview's project-binding listener.
    // unsubscribe 仅移除此概览的项目绑定监听器。
    const unsubscribe = subscribeKnowledgeProjectBindingsChanged((projectId) => { if (projectId === props.project.id) void reload(); });
    return () => { active = false; unsubscribe(); };
  }, [props.project.id]);
  React.useEffect(() => {
    if (props.vmmDraft === null || props.vmmDraft.backendMode !== "native") return;
    // active fences the native catalog against project/backend changes and unmounts.
    // active 隔离项目或后端切换以及卸载后的原生目录结果。
    let active = true;
    setCatalog({ projectId: props.project.id, value: null, error: null });
    void runVmmMemoryAction({ action: "native_project_catalog", commandId: crypto.randomUUID(), scope: "global", projectId: null })
      .then((result) => {
        if (result.data.kind !== "native_project_catalog") throw new Error("Memory catalog returned an unexpected result kind.");
        if (active) setCatalog({ projectId: props.project.id, value: result.data.value, error: null });
      }).catch((error) => {
        if (active) setCatalog({ projectId: props.project.id, value: null, error: error instanceof Error ? error.message : String(error) });
      });
    return () => { active = false; };
  }, [props.project.id, props.vmmDraft?.backendMode, props.vmmRevision]);
  // binding, projectKnowledge and runtime reject results belonging to any other project.
  // binding、projectKnowledge 与 runtime 拒绝属于任何其他项目的结果。
  const nativeCatalog = catalog?.projectId === props.project.id ? catalog : null;
  const binding = nativeCatalog?.value?.bindings.find((item) => item.vulcanProjectId === props.project.id);
  // owner resolves the binding's stable identity through the same authoritative catalog.
  // owner 通过同一权威目录解析绑定的稳定身份。
  const owner = binding && nativeCatalog?.value?.projects.find((item) => item.projectId === binding.ownerProjectId);
  const projectKnowledge = knowledge?.projectId === props.project.id ? knowledge : null;
  const runtime = props.memory.selectedProjectId === props.project.id ? props.memory.runtime : null;
  // issues contains only observed failures, never an inferred healthy state.
  // issues 仅包含已观察的失败，绝不推断健康状态。
  const issues = [props.vmmError, props.agentError, projectKnowledge?.error,
    runtime?.backendError, runtime?.scopeError, runtime?.precheckError, runtime?.postactionError,
    props.vmmDraft?.backendMode === "native" ? nativeCatalog?.error : null,
    props.vmmDraft?.backendMode === "native" && binding && !owner ? t("projectOverview.missingOwner").replace("{id}", String(binding.ownerProjectId)) : null].filter((issue): issue is string => Boolean(issue));
  // openAction preserves the existing section identity and gives every status a useful next step.
  // openAction 保留既有分区身份，并为每项状态提供有效下一步。
  const openAction = (section: ProjectSettingsSectionId, label: string) => (
    <Button size="sm" variant="ghost" onClick={() => props.onOpenSection(section)} aria-label={label}>{label}<ArrowRight size={13} /></Button>
  );
  // copyPath writes the project path and reports failure so the visible path can still be selected by hand.
  // copyPath 写入项目路径并报告失败，使可见路径仍可手动选择复制。
  const copyPath = async () => {
    try { await navigator.clipboard.writeText(props.project.path); setCopied("copied"); }
    catch { setCopied("copyFailed"); }
  };
  return <div className="project-overview">
    <header className="project-overview-identity"><FolderCog size={22} /><div><h3>{props.project.name}</h3>
      <span className="project-overview-path-row">
        <span className="project-overview-path" title={props.project.path}>{props.project.path}</span>
        <Button size="icon-xs" variant="ghost" aria-label={t("projectOverview.copyPath")} title={t("projectOverview.copyPath")} onClick={() => void copyPath()}><Copy size={12} /></Button>
        <span className="project-overview-copy-status" role="status">{copied ? t(`projectOverview.${copied}`) : null}</span>
      </span>
      <span>{t("projectOverview.conversations").replace("{count}", String(props.conversationCount))}{props.project.pinned ? ` · ${t("projectOverview.pinned")}` : ""}</span></div></header>
    {issues.length ? <div className="project-overview-issues" role="alert"><strong>{t("projectOverview.issues")}</strong>{[...new Set(issues)].map((issue) => issue.length > 180 || issue.includes("\n")
      ? <div className="project-overview-issue-long" key={issue}><p>{issue.split("\n", 1)[0].slice(0, 180)}…</p>
        <InlineDisclosure label={t("projectOverview.issueExpand")} openLabel={t("projectOverview.issueCollapse")}><p>{issue}</p></InlineDisclosure></div>
      : <p key={issue}>{issue}</p>)}</div> : null}
    <div className="project-overview-capabilities">
      <section><div><strong>{t("projectSettings.vmmMemory")}</strong>
        {props.vmmLoading ? <StreamPlaceholder label={t("projectOverview.memoryLoading")} count={2} /> : props.vmmDraft === null ? <p>{t("projectOverview.memoryUnknown")}</p> : <>
          <p>{t(props.vmmDraft.backendMode === "native" ? "projectOverview.native" : "projectOverview.external")} · {t(props.vmmDraft.enabled ? "projectOverview.enabled" : "projectOverview.disabled")} · {t(props.vmmDraft.hasOverride ? "projectOverview.override" : "projectOverview.inherited")}{props.vmmDirty ? ` · ${t("projectOverview.unsaved")}` : ""}</p>
          <p>{props.vmmDraft.backendMode === "native" ? nativeCatalog?.value ? binding
            ? t("projectOverview.nativeBinding").replace("{name}", owner ? owner.name : t("projectOverview.ownerUnknown")).replace("{id}", String(binding.ownerProjectId)).replace("{count}", String(binding.linkedProjectIds.length))
            : t("projectOverview.unbound") : t("projectOverview.bindingUnknown") : props.vmmDraft.projectId
              ? t("projectOverview.externalBinding").replace("{id}", props.vmmDraft.projectId) : t("projectOverview.externalUnbound")}</p>
          <small>{t("projectOverview.precheck")}: {t(props.vmmDraft.precheckEnabled ? "projectOverview.on" : "projectOverview.off")} · {t("projectOverview.writeback")}: {t(props.vmmDraft.memoryWritebackEnabled ? "projectOverview.on" : "projectOverview.off")}</small>
          <small>{runtime ? `${t("projectOverview.session")}: ${t(runtime.scopeReady ? "projectOverview.scopeReady" : "projectOverview.scopeNotReady")} · ${t(runtime.precheckReady ? "projectOverview.precheckReady" : "projectOverview.precheckNotReady")} · ${t(runtime.postactionReady ? "projectOverview.postactionReady" : "projectOverview.postactionNotReady")}` : t("projectOverview.runtimeUnknown")}</small>
        </>}
      </div>{openAction("vmm", t("projectOverview.memorySettings"))}</section>
      <section><div><strong>{t("projectSettings.knowledgeBases")}</strong><p>{projectKnowledge?.rows
        ? t("projectOverview.knowledgeCount").replace("{total}", String(projectKnowledge.rows.length)).replace("{enabled}", String(projectKnowledge.rows.filter((item) => item.defaultEnabled).length))
        : projectKnowledge?.error ? t("projectOverview.knowledgeFailed") : <InlineStreamPlaceholder label={t("projectOverview.knowledgeLoading")} />}</p></div>{openAction("knowledge-bases", t("projectOverview.knowledgeSettings"))}</section>
      <section><div><strong>{t("projectSettings.agentTitle")}</strong><p>{props.agentLoading ? <InlineStreamPlaceholder label={t("projectOverview.skillsLoading")} /> : !props.agentLoaded ? t("projectOverview.skillsUnknown")
        : Object.keys(props.agentOverrides).length ? t("projectOverview.skillsCount").replace("{count}", String(Object.keys(props.agentOverrides).length)) : t("projectOverview.skillsInherited")}</p></div>{openAction("agent-skills", t("projectOverview.skillSettings"))}</section>
    </div>
    <div className="project-overview-maintenance"><span>{t("projectOverview.maintenance")}</span>{openAction("attachments", t("projectOverview.attachments"))}</div>
    <section className="project-overview-location" aria-label={t("projectOverview.details")}>
      <strong>{t("projectOverview.details")}</strong>
      <dl><dt>{t("projectOverview.attachmentPath")}</dt><dd>{props.attachmentRoot}</dd><dt>{t("projectOverview.id")}</dt><dd>{props.project.id}</dd></dl>
    </section>
  </div>;
}
