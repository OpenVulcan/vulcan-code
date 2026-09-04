import * as React from "react";
import { Database, FileCode2, LoaderCircle, RefreshCw, Save, SearchCheck } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { useI18n } from "@/i18n";
import {
  cancelVceOperation,
  executeVceMaintenance,
  listVceFiles,
  listVceIntegrityIssues,
  listVceOperations,
  loadVceBlobDetail,
  loadVceProjectOverview,
  planVceMaintenance,
  retryVceOperation,
} from "@/service/vce-service";
import type {
  VceBlobDetailResponse,
  VceFileSummary,
  VceIntegrityIssueSummary,
  VceMaintenancePlanResponse,
  VceOperationSummary,
  VceProjectOverviewResponse,
  VceProjectSettingsInput,
} from "@/types";
import {
  emptyVceManagementPage,
  materializeVceManagementPage,
  nextVceManagementPageRequest,
  previousVceManagementPageRequest,
  VCE_MANAGEMENT_PAGE_SIZE,
  type VceManagementPageRequest,
  type VceManagementPageState,
} from "./code-context-pagination";

// loadAuthoritativeVceOverview reads and validates the current host-owned project snapshot.
// loadAuthoritativeVceOverview 读取并校验当前由宿主持有的项目快照。
//
// Parameters: projectId is the exact mounted project and loader performs the service read.
// 参数：projectId 是精确的已挂载项目，loader 执行服务读取。
// Returns: the validated current overview for the requested project.
// 返回值：请求项目经过校验的当前概览。
export async function loadAuthoritativeVceOverview(
  projectId: string,
  loader: (projectId: string) => Promise<VceProjectOverviewResponse> = loadVceProjectOverview,
): Promise<VceProjectOverviewResponse> {
  const overview = await loader(projectId);
  if (overview.status.projectId !== projectId) {
    throw new Error("VCE overview recovery returned a mismatched project identity.");
  }
  return overview;
}

// CodeContextArea renders project settings and paged VCE management data.
// CodeContextArea 渲染项目设置与分页 VCE 管理数据。
export function CodeContextArea(props: {
  // Exact project identity selected by the settings dialog.
  // 设置弹窗选择的精确项目身份。
  projectId: string;
  // Current authoritative settings and runtime overview.
  // 当前权威设置与运行时概览。
  overview: VceProjectOverviewResponse | null;
  // Whether an overview read or project mutation is active.
  // 是否正在执行概览读取或项目修改。
  busy: boolean;
  // Persists a complete editable settings document.
  // 持久化完整可编辑设置文档。
  onSave: (settings: VceProjectSettingsInput) => Promise<void>;
  // Requests one explicit full refresh.
  // 请求一次显式全量刷新。
  onRefresh: () => Promise<void>;
  // Replaces the parent-owned overview after a management mutation.
  // 在管理修改后替换父级持有的概览。
  onOverviewChanged: (overview: VceProjectOverviewResponse) => void;
}) {
  // locale selects local bilingual copy without exposing VCE implementation names to users.
  // locale 选择本地双语文案，且不向用户暴露 VCE 实现名称。
  const { locale } = useI18n();
  // zh returns Chinese copy for the active Chinese locale and English otherwise.
  // zh 为当前中文语言返回中文文案，否则返回英文文案。
  const copy = React.useCallback(
    (zh: string, en: string) => (locale === "zh-CN" ? zh : en),
    [locale],
  );
  // draft holds only user-editable fields and never mirrors host-owned revision fields.
  // draft 只保存用户可编辑字段，绝不镜像宿主持有的修订字段。
  const [draft, setDraft] = React.useState<VceProjectSettingsInput | null>(() => (
    props.overview ? editableSettings(props.overview.settings) : null
  ));
  // managementBusy covers bounded file, operation, and integrity reads.
  // managementBusy 表示有界文件、操作与完整性读取正在进行。
  const [managementBusy, setManagementBusy] = React.useState(false);
  // filePage stores one bounded file page and its opaque navigation history.
  // filePage 保存一页有界文件记录及其不透明导航历史。
  const [filePage, setFilePage] = React.useState<VceManagementPageState<VceFileSummary>>(
    emptyVceManagementPage,
  );
  // operationPage stores one bounded durable-operation page.
  // operationPage 保存一页有界持久操作记录。
  const [operationPage, setOperationPage] = React.useState<
    VceManagementPageState<VceOperationSummary>
  >(emptyVceManagementPage);
  // issuePage stores one bounded integrity-difference page.
  // issuePage 保存一页有界完整性差异。
  const [issuePage, setIssuePage] = React.useState<
    VceManagementPageState<VceIntegrityIssueSummary>
  >(emptyVceManagementPage);
  // blob stores the currently inspected exact Blob detail.
  // blob 保存当前检查的精确 Blob 详情。
  const [blob, setBlob] = React.useState<VceBlobDetailResponse | null>(null);
  // maintenancePlan stores the latest read-only maintenance preview.
  // maintenancePlan 保存最近一份只读维护预览。
  const [maintenancePlan, setMaintenancePlan] = React.useState<VceMaintenancePlanResponse | null>(null);
  // managementError stores a bounded management-load diagnostic.
  // managementError 保存一条有界管理加载诊断。
  const [managementError, setManagementError] = React.useState<string | null>(null);
  // managementRequestVersion invalidates stale asynchronous responses after project or page changes.
  // managementRequestVersion 在项目或分页变化后使陈旧异步响应失效。
  const managementRequestVersion = React.useRef(0);
  // mountedProjectRef identifies the exact project lifetime owned by this mounted panel.
  // mountedProjectRef 标识当前已挂载面板持有的精确项目生命周期。
  const mountedProjectRef = React.useRef<string | null>(props.projectId);
  // maintenanceMutationInFlightRef prevents double submission before React can repaint disabled controls.
  // maintenanceMutationInFlightRef 防止 React 重绘禁用控件前重复提交维护操作。
  const maintenanceMutationInFlightRef = React.useRef(false);

  React.useEffect(() => {
    const projectId = props.projectId;
    mountedProjectRef.current = projectId;
    return () => {
      if (mountedProjectRef.current === projectId) {
        mountedProjectRef.current = null;
      }
      managementRequestVersion.current += 1;
    };
  }, [props.projectId]);

  React.useEffect(() => {
    if (!props.overview) {
      setDraft(null);
      return;
    }
    // settings is projected explicitly so host-owned metadata cannot be resubmitted.
    // settings 被显式投影，因此宿主持有的元数据不会被重新提交。
    const settings = props.overview.settings;
    setDraft({
      enabled: settings.enabled,
      maxFileBytes: settings.maxFileBytes,
      extraIgnorePatterns: settings.extraIgnorePatterns,
      allowQueryRewrite: settings.allowQueryRewrite,
      allowRerank: settings.allowRerank,
      embeddingMaxConcurrency: settings.embeddingMaxConcurrency,
      queryMaxConcurrency: settings.queryMaxConcurrency,
      maxResults: settings.maxResults,
      maxContextChars: settings.maxContextChars,
    });
  }, [props.overview]);

  // reloadManagement loads independent bounded lists only for an enabled runtime.
  // reloadManagement 仅为已启用运行时加载相互独立的有界列表。
  const reloadManagement = React.useCallback(async () => {
    const requestVersion = managementRequestVersion.current + 1;
    managementRequestVersion.current = requestVersion;
    if (!props.overview?.settings.enabled) {
      setFilePage(emptyVceManagementPage());
      setOperationPage(emptyVceManagementPage());
      setIssuePage(emptyVceManagementPage());
      setBlob(null);
      return;
    }
    setManagementBusy(true);
    setManagementError(null);
    try {
      // results preserve independent list ownership while sharing one loading phase.
      // results 在共享一次加载阶段时仍保留各列表的独立归属。
      const [filePage, operationPage, issuePage] = await Promise.all([
        listVceFiles(props.projectId, null, VCE_MANAGEMENT_PAGE_SIZE),
        listVceOperations(props.projectId, null, VCE_MANAGEMENT_PAGE_SIZE),
        listVceIntegrityIssues(props.projectId, null, VCE_MANAGEMENT_PAGE_SIZE),
      ]);
      if (managementRequestVersion.current !== requestVersion) {
        return;
      }
      const firstPageRequest = { cursor: null, previousCursors: [] };
      setFilePage(materializeVceManagementPage(filePage, firstPageRequest));
      setOperationPage(materializeVceManagementPage(operationPage, firstPageRequest));
      setIssuePage(materializeVceManagementPage(issuePage, firstPageRequest));
    } catch (error) {
      if (managementRequestVersion.current === requestVersion) {
        setManagementError(error instanceof Error ? error.message : String(error));
      }
    } finally {
      if (managementRequestVersion.current === requestVersion) {
        setManagementBusy(false);
      }
    }
  }, [props.overview?.settings.enabled, props.projectId]);

  React.useEffect(() => {
    void reloadManagement();
    return () => {
      managementRequestVersion.current += 1;
    };
  }, [reloadManagement, props.overview?.status.checkpointVersion]);

  // loadManagementPage commits only the latest page response for the selected project.
  // loadManagementPage 只提交所选项目的最新分页响应。
  const loadManagementPage = React.useCallback(async <T,>(
    request: VceManagementPageRequest | null,
    loader: (cursor: string | null) => Promise<import("@/types").VcePageResponse<T>>,
    applyPage: (page: VceManagementPageState<T>) => void,
  ) => {
    if (!request) {
      return;
    }
    const requestVersion = managementRequestVersion.current + 1;
    managementRequestVersion.current = requestVersion;
    setManagementBusy(true);
    setManagementError(null);
    try {
      const response = await loader(request.cursor);
      if (managementRequestVersion.current === requestVersion) {
        applyPage(materializeVceManagementPage(response, request));
      }
    } catch (error) {
      if (managementRequestVersion.current === requestVersion) {
        setManagementError(error instanceof Error ? error.message : String(error));
      }
    } finally {
      if (managementRequestVersion.current === requestVersion) {
        setManagementBusy(false);
      }
    }
  }, []);

  // inspectBlob loads only the selected immutable Blob identity.
  // inspectBlob 只加载所选不可变 Blob 身份。
  const inspectBlob = async (blobName: string | null | undefined) => {
    if (!blobName) {
      return;
    }
    const projectId = props.projectId;
    setManagementBusy(true);
    setManagementError(null);
    try {
      const nextBlob = await loadVceBlobDetail(projectId, blobName);
      if (mountedProjectRef.current === projectId) {
        setBlob(nextBlob);
      }
    } catch (error) {
      if (mountedProjectRef.current === projectId) {
        setManagementError(error instanceof Error ? error.message : String(error));
      }
    } finally {
      if (mountedProjectRef.current === projectId) {
        setManagementBusy(false);
      }
    }
  };

  // loadMaintenancePlan obtains a read-only plan and performs no mutation.
  // loadMaintenancePlan 获取只读计划且不执行修改。
  const loadMaintenancePlan = async (mode: "repair" | "rebuild" | "cleanup_staging") => {
    const projectId = props.projectId;
    setManagementBusy(true);
    setManagementError(null);
    try {
      const nextPlan = await planVceMaintenance(projectId, mode);
      if (mountedProjectRef.current === projectId) {
        setMaintenancePlan(nextPlan);
      }
    } catch (error) {
      if (mountedProjectRef.current === projectId) {
        setManagementError(error instanceof Error ? error.message : String(error));
      }
    } finally {
      if (mountedProjectRef.current === projectId) {
        setManagementBusy(false);
      }
    }
  };

  // saveDraft reloads authoritative settings when a stale revision or another host error rejects the save.
  // saveDraft 在陈旧修订或其他宿主错误拒绝保存时重新加载权威设置。
  const saveDraft = async (next: VceProjectSettingsInput) => {
    setManagementError(null);
    try {
      await props.onSave(next);
    } catch (error) {
      // saveError preserves the original mutation failure even when recovery also fails.
      // saveError 保留原始修改失败，即使恢复读取也失败。
      const saveError = error instanceof Error ? error.message : String(error);
      // projectId binds the recovery read to the panel that initiated the rejected mutation.
      // projectId 将恢复读取绑定到发起被拒修改的面板。
      const projectId = props.projectId;
      try {
        // overview is a fresh host read; stale props must never restore a rejected revision.
        // overview 是新的宿主读取；不得用陈旧 props 恢复被拒绝的修订。
        const overview = await loadAuthoritativeVceOverview(projectId);
        if (mountedProjectRef.current === projectId) {
          props.onOverviewChanged(overview);
          setDraft(editableSettings(overview.settings));
          setManagementError(saveError);
        }
      } catch (recoveryError) {
        if (mountedProjectRef.current === projectId) {
          // recoveryMessage makes both the rejected write and failed authoritative reload visible.
          // recoveryMessage 同时呈现被拒写入与权威重载失败。
          const recoveryMessage = recoveryError instanceof Error
            ? recoveryError.message
            : String(recoveryError);
          setManagementError(`${saveError} ${copy(
            `重新加载权威设置失败：${recoveryMessage}`,
            `Reloading authoritative settings failed: ${recoveryMessage}`,
          )}`);
        }
      }
    }
  };

  // refreshIndex reports a rejected refresh locally instead of creating an unhandled promise.
  // refreshIndex 在本地报告被拒绝的刷新，而不是产生未处理 Promise。
  const refreshIndex = async () => {
    setManagementError(null);
    try {
      await props.onRefresh();
      await reloadManagement();
    } catch (error) {
      setManagementError(error instanceof Error ? error.message : String(error));
    }
  };

  // executeMaintenance confirms and executes only the exact current read-only plan.
  // executeMaintenance 仅确认并执行精确的当前只读计划。
  const executeMaintenance = async () => {
    const overview = props.overview;
    if (maintenanceMutationInFlightRef.current) {
      return;
    }
    if (!overview || !maintenancePlan || maintenancePlan.actions.length === 0) {
      setManagementError(copy("当前计划没有可执行操作。", "The current plan has no executable actions."));
      return;
    }
    const projectId = props.projectId;
    const checkpoint = overview.status.checkpointVersion;
    if (checkpoint === null || checkpoint === undefined) {
      setManagementError(copy("维护操作要求一个已发布检查点。", "Maintenance requires a published checkpoint."));
      return;
    }
    const confirmed = window.confirm(copy(
      `确认执行 ${maintenancePlan.mode} 维护？该操作将影响 ${maintenancePlan.affectedBlobCount} 个 Blob。`,
      `Execute ${maintenancePlan.mode} maintenance affecting ${maintenancePlan.affectedBlobCount} Blobs?`,
    ));
    if (!confirmed) {
      return;
    }
    maintenanceMutationInFlightRef.current = true;
    setManagementBusy(true);
    setManagementError(null);
    try {
      const updatedOverview = await executeVceMaintenance(
        projectId,
        maintenancePlan.mode,
        overview.settings.revision,
        checkpoint,
        maintenancePlan.actions.map((action) => action.actionId),
      );
      if (mountedProjectRef.current !== projectId) {
        return;
      }
      props.onOverviewChanged(updatedOverview);
      setMaintenancePlan(null);
      await reloadManagement();
    } catch (error) {
      if (mountedProjectRef.current === projectId) {
        setManagementError(error instanceof Error ? error.message : String(error));
      }
    } finally {
      maintenanceMutationInFlightRef.current = false;
      if (mountedProjectRef.current === projectId) {
        setManagementBusy(false);
      }
    }
  };

  // retryOperation reruns one failed or cancelled operation as a guarded recovery refresh.
  // retryOperation 将一项失败或已取消操作重跑为受保护恢复刷新。
  const retryOperation = async (operationId: string) => {
    const overview = props.overview;
    if (!overview || maintenanceMutationInFlightRef.current) {
      return;
    }
    const projectId = props.projectId;
    maintenanceMutationInFlightRef.current = true;
    setManagementBusy(true);
    setManagementError(null);
    try {
      const updatedOverview = await retryVceOperation(
        projectId,
        operationId,
        overview.settings.revision,
        overview.status.checkpointVersion ?? null,
      );
      if (mountedProjectRef.current !== projectId) {
        return;
      }
      props.onOverviewChanged(updatedOverview);
      await reloadManagement();
    } catch (error) {
      if (mountedProjectRef.current === projectId) {
        setManagementError(error instanceof Error ? error.message : String(error));
      }
    } finally {
      maintenanceMutationInFlightRef.current = false;
      if (mountedProjectRef.current === projectId) {
        setManagementBusy(false);
      }
    }
  };

  // cancelOperation requests cancellation only for the exact visible active operation.
  // cancelOperation 仅请求取消精确可见的活动操作。
  const cancelOperation = async (operationId: string) => {
    const overview = props.overview;
    if (!overview || maintenanceMutationInFlightRef.current) {
      return;
    }
    const projectId = props.projectId;
    maintenanceMutationInFlightRef.current = true;
    setManagementBusy(true);
    setManagementError(null);
    try {
      const updatedOverview = await cancelVceOperation(
        projectId,
        operationId,
        overview.settings.revision,
        overview.status.checkpointVersion ?? null,
      );
      if (mountedProjectRef.current !== projectId) {
        return;
      }
      props.onOverviewChanged(updatedOverview);
      await reloadManagement();
    } catch (error) {
      if (mountedProjectRef.current === projectId) {
        setManagementError(error instanceof Error ? error.message : String(error));
      }
    } finally {
      maintenanceMutationInFlightRef.current = false;
      if (mountedProjectRef.current === projectId) {
        setManagementBusy(false);
      }
    }
  };

  if (!props.overview || !draft) {
    return (
      <div className="project-settings-section">
        <LoaderCircle className="animate-spin" size={18} />
        <p className="project-settings-muted">{copy("正在加载代码语义索引…", "Loading code semantic index…")}</p>
      </div>
    );
  }

  // status exposes lifecycle and checkpoint state without provider implementation details.
  // status 暴露生命周期与检查点状态且不包含 Provider 实现细节。
  const status = props.overview.status;
  // counts supplies aggregate storage statistics when the runtime is active.
  // counts 在运行时活动时提供存储聚合统计。
  const counts = props.overview.counts;
  return (
    <div className="project-settings-section">
      <header className="project-settings-heading">
        <Database size={18} />
        <div>
          <strong>{copy("代码语义索引", "Code semantic index")}</strong>
          <span>{copy("为 AI 提供当前项目的自然语言代码检索。", "Provides natural-language code retrieval for AI in this project.")}</span>
        </div>
        <Button
          aria-label={copy("刷新状态", "Refresh status")}
          disabled={props.busy || !draft.enabled}
          onClick={() => void refreshIndex()}
          size="icon-sm"
          type="button"
          variant="outline"
        >
          <RefreshCw className={props.busy ? "animate-spin" : undefined} size={15} />
        </Button>
      </header>

      <div className="project-settings-toggle-row">
        <Switch
          aria-label={copy("启用代码语义索引", "Enable code semantic index")}
          checked={draft.enabled}
          disabled={props.busy}
          onCheckedChange={(enabled) => {
            const next = { ...draft, enabled };
            setDraft(next);
            void saveDraft(next);
          }}
        />
        <span>
          <strong>{copy("启用代码语义索引", "Enable code semantic index")}</strong>
          <small>{copy("首次启用会建立完整索引，之后随文件变化增量刷新。", "The first enable builds a full index; later file changes refresh incrementally.")}</small>
        </span>
      </div>

      <div className="project-settings-grid">
        <div className="project-settings-panel"><strong>{copy("生命周期", "Lifecycle")}</strong><p>{status.lifecycle}</p></div>
        <div className="project-settings-panel"><strong>{copy("检查点", "Checkpoint")}</strong><p>{status.checkpointVersion ?? "—"}</p></div>
        <div className="project-settings-panel"><strong>{copy("文件", "Files")}</strong><p>{counts?.searchableFiles ?? status.presentFiles}</p></div>
        <div className="project-settings-panel"><strong>{copy("代码块", "Chunks")}</strong><p>{counts?.chunks ?? "—"}</p></div>
        <div className="project-settings-panel"><strong>{copy("向量", "Vectors")}</strong><p>{counts?.vectors ?? "—"}</p></div>
        <div className="project-settings-panel"><strong>{copy("精确符号", "Exact symbols")}</strong><p>{counts?.symbols ?? "—"}</p></div>
      </div>

      {status.lastError ? <p className="rounded-lg border border-destructive/30 bg-destructive/5 p-3 text-sm text-destructive">{status.lastError}</p> : null}

      <section className="project-settings-panel space-y-3">
        <strong>{copy("高级设置", "Advanced settings")}</strong>
        <label className="grid gap-1 text-sm">
          {copy("附加忽略规则（每行一条）", "Additional ignore rules (one per line)")}
          <Textarea
            disabled={props.busy}
            onChange={(event) => setDraft({ ...draft, extraIgnorePatterns: event.target.value.split(/\r?\n/).map((value) => value.trim()).filter(Boolean) })}
            value={draft.extraIgnorePatterns.join("\n")}
          />
        </label>
        <div className="grid grid-cols-2 gap-3">
          <NumericSetting disabled={props.busy} label={copy("最大文件字节数", "Maximum file bytes")} value={draft.maxFileBytes} onChange={(value) => setDraft({ ...draft, maxFileBytes: value })} />
          <NumericSetting disabled={props.busy} label={copy("最大结果数", "Maximum results")} value={draft.maxResults} onChange={(value) => setDraft({ ...draft, maxResults: value })} />
          <NumericSetting disabled={props.busy} label={copy("上下文字符预算", "Context character budget")} value={draft.maxContextChars} onChange={(value) => setDraft({ ...draft, maxContextChars: value })} />
          <NumericSetting disabled={props.busy} label={copy("向量并发", "Embedding concurrency")} value={draft.embeddingMaxConcurrency} onChange={(value) => setDraft({ ...draft, embeddingMaxConcurrency: value })} />
          <NumericSetting disabled={props.busy} label={copy("查询并发", "Query concurrency")} value={draft.queryMaxConcurrency} onChange={(value) => setDraft({ ...draft, queryMaxConcurrency: value })} />
        </div>
        <ToggleSetting checked={draft.allowQueryRewrite} disabled={props.busy} label={copy("允许 LLM 查询改写", "Allow LLM query rewriting")} onChange={(value) => setDraft({ ...draft, allowQueryRewrite: value })} />
        <ToggleSetting checked={draft.allowRerank} disabled={props.busy} label={copy("允许候选重排", "Allow candidate reranking")} onChange={(value) => setDraft({ ...draft, allowRerank: value })} />
        <Button disabled={props.busy} onClick={() => void saveDraft(draft)} type="button">
          <Save size={15} /> {copy("保存设置", "Save settings")}
        </Button>
      </section>

      {draft.enabled ? (
        <>
          <section className="project-settings-panel space-y-2">
            <div className="flex items-center justify-between gap-2">
              <strong>{copy("文件与 Blob", "Files and Blobs")}</strong>
              <Button disabled={managementBusy} onClick={() => void reloadManagement()} size="sm" type="button" variant="outline"><RefreshCw size={14} />{copy("刷新", "Refresh")}</Button>
            </div>
            {filePage.items.length === 0 ? <p className="project-settings-muted">{copy("暂无已索引文件。", "No indexed files.")}</p> : filePage.items.map((file) => (
              <button className="flex w-full items-center justify-between gap-3 rounded-md border p-2 text-left text-sm" key={`${file.path}:${file.generation}`} onClick={() => void inspectBlob(file.committedBlobName ?? file.blobName)} type="button">
                <span className="min-w-0"><FileCode2 className="mr-2 inline" size={14} /><span className="break-all">{file.path}</span></span>
                <span className="shrink-0 text-xs text-muted-foreground">{file.chunkCount} {copy("块", "chunks")}</span>
              </button>
            ))}
            <ManagementPagination
              busy={managementBusy}
              copy={copy}
              onNext={() => void loadManagementPage(
                nextVceManagementPageRequest(filePage),
                (cursor) => listVceFiles(props.projectId, cursor, VCE_MANAGEMENT_PAGE_SIZE),
                setFilePage,
              )}
              onPrevious={() => void loadManagementPage(
                previousVceManagementPageRequest(filePage),
                (cursor) => listVceFiles(props.projectId, cursor, VCE_MANAGEMENT_PAGE_SIZE),
                setFilePage,
              )}
              page={filePage}
            />
          </section>

          {blob ? (
            <section className="project-settings-panel space-y-2">
              <strong>{copy("Blob 详情", "Blob detail")}: {blob.path}</strong>
              <p className="break-all text-xs text-muted-foreground">{blob.blobName}</p>
              {blob.chunks.map((chunk) => (
                <pre className="max-h-48 overflow-auto rounded-md border bg-muted/20 p-2 text-xs" key={chunk.chunkId}>{`${chunk.startLine}-${chunk.endLine} · ${chunk.kind}\n${chunk.preview ?? ""}`}</pre>
              ))}
            </section>
          ) : null}

          <section className="project-settings-panel space-y-2">
            <strong>{copy("作业历史", "Operation history")}</strong>
            {operationPage.items.length === 0 ? <p className="project-settings-muted">{copy("暂无作业记录。", "No operation history.")}</p> : operationPage.items.map((operation) => (
              <div className="flex items-center justify-between gap-2 text-sm" key={operation.operationId}>
                <p>{operation.kind} · {operation.phase} · {operation.completedUnits}/{operation.totalUnits}{operation.error ? ` · ${operation.error}` : ""}</p>
                {operation.phase === "failed" || operation.phase === "cancelled" ? (
                  <Button disabled={managementBusy} onClick={() => void retryOperation(operation.operationId)} size="sm" type="button" variant="outline">{copy("重试", "Retry")}</Button>
                ) : operation.phase !== "completed" ? (
                  <Button disabled={managementBusy} onClick={() => void cancelOperation(operation.operationId)} size="sm" type="button" variant="outline">{copy("取消", "Cancel")}</Button>
                ) : null}
              </div>
            ))}
            <ManagementPagination
              busy={managementBusy}
              copy={copy}
              onNext={() => void loadManagementPage(
                nextVceManagementPageRequest(operationPage),
                (cursor) => listVceOperations(props.projectId, cursor, VCE_MANAGEMENT_PAGE_SIZE),
                setOperationPage,
              )}
              onPrevious={() => void loadManagementPage(
                previousVceManagementPageRequest(operationPage),
                (cursor) => listVceOperations(props.projectId, cursor, VCE_MANAGEMENT_PAGE_SIZE),
                setOperationPage,
              )}
              page={operationPage}
            />
          </section>

          <section className="project-settings-panel space-y-2">
            <strong>{copy("完整性与维护", "Integrity and maintenance")}</strong>
            {issuePage.items.length === 0 ? <p className="project-settings-muted"><SearchCheck className="mr-2 inline" size={14} />{copy("未发现完整性问题。", "No integrity issues found.")}</p> : issuePage.items.map((issue) => <p className="text-sm text-destructive" key={issue.issueId}>{issue.kind}: {issue.message}</p>)}
            <div className="flex flex-wrap gap-2">
              <Button disabled={managementBusy} onClick={() => void loadMaintenancePlan("repair")} size="sm" type="button" variant="outline">{copy("生成修复计划", "Plan repair")}</Button>
              <Button disabled={managementBusy} onClick={() => void loadMaintenancePlan("rebuild")} size="sm" type="button" variant="outline">{copy("生成重建计划", "Plan rebuild")}</Button>
              <Button disabled={managementBusy} onClick={() => void loadMaintenancePlan("cleanup_staging")} size="sm" type="button" variant="outline">{copy("生成暂存清理计划", "Plan staging cleanup")}</Button>
            </div>
            {maintenancePlan ? (
              <div className="space-y-2">
                <p className="text-sm">{copy("计划动作", "Planned actions")}: {maintenancePlan.actions.length} · {copy("影响 Blob", "Affected Blobs")}: {maintenancePlan.affectedBlobCount}</p>
                {maintenancePlan.actions.map((action) => <p className="text-sm" key={action.actionId}>{action.description}</p>)}
                <Button disabled={managementBusy || maintenancePlan.actions.length === 0} onClick={() => void executeMaintenance()} size="sm" type="button">{copy("确认并执行", "Confirm and execute")}</Button>
              </div>
            ) : null}
            <ManagementPagination
              busy={managementBusy}
              copy={copy}
              onNext={() => void loadManagementPage(
                nextVceManagementPageRequest(issuePage),
                (cursor) => listVceIntegrityIssues(props.projectId, cursor, VCE_MANAGEMENT_PAGE_SIZE),
                setIssuePage,
              )}
              onPrevious={() => void loadManagementPage(
                previousVceManagementPageRequest(issuePage),
                (cursor) => listVceIntegrityIssues(props.projectId, cursor, VCE_MANAGEMENT_PAGE_SIZE),
                setIssuePage,
              )}
              page={issuePage}
            />
          </section>
        </>
      ) : null}

      {managementBusy ? <p className="project-settings-muted"><LoaderCircle className="mr-2 inline animate-spin" size={14} />{copy("正在加载管理数据…", "Loading management data…")}</p> : null}
      {managementError ? <p className="rounded-lg border border-destructive/30 bg-destructive/5 p-3 text-sm text-destructive">{managementError}</p> : null}
    </div>
  );
}

// ManagementPagination renders cursor-safe previous and next controls for one bounded list.
// ManagementPagination 为一项有界列表渲染游标安全的上一页与下一页控件。
function ManagementPagination<T>(props: {
  // Whether another management request is active.
  // 是否有其他管理请求正在执行。
  busy: boolean;
  // Localized copy selector owned by the parent surface.
  // 父界面持有的本地化文案选择器。
  copy: (zh: string, en: string) => string;
  // Loads the immediately following opaque page.
  // 加载紧邻的下一不透明分页。
  onNext: () => void;
  // Loads the immediately preceding opaque page.
  // 加载紧邻的上一不透明分页。
  onPrevious: () => void;
  // Current page and cursor history.
  // 当前分页及游标历史。
  page: VceManagementPageState<T>;
}) {
  const pageNumber = props.page.previousCursors.length + 1;
  return (
    <div className="flex items-center justify-end gap-2 text-xs text-muted-foreground">
      <span>
        {props.copy("第", "Page")} {pageNumber}
        {props.page.total === null ? "" : ` · ${props.copy("共", "Total")} ${props.page.total}`}
      </span>
      <Button
        disabled={props.busy || props.page.previousCursors.length === 0}
        onClick={props.onPrevious}
        size="sm"
        type="button"
        variant="outline"
      >
        {props.copy("上一页", "Previous")}
      </Button>
      <Button
        disabled={props.busy || props.page.nextCursor === null}
        onClick={props.onNext}
        size="sm"
        type="button"
        variant="outline"
      >
        {props.copy("下一页", "Next")}
      </Button>
    </div>
  );
}

// editableSettings strips host-owned fields from one settings summary.
// editableSettings 从一项设置摘要中剥离宿主持有字段。
function editableSettings(settings: VceProjectOverviewResponse["settings"]): VceProjectSettingsInput {
  return {
    enabled: settings.enabled,
    maxFileBytes: settings.maxFileBytes,
    extraIgnorePatterns: settings.extraIgnorePatterns,
    allowQueryRewrite: settings.allowQueryRewrite,
    allowRerank: settings.allowRerank,
    embeddingMaxConcurrency: settings.embeddingMaxConcurrency,
    queryMaxConcurrency: settings.queryMaxConcurrency,
    maxResults: settings.maxResults,
    maxContextChars: settings.maxContextChars,
  };
}

// NumericSetting renders one explicit integer input used by a VCE budget.
// NumericSetting 渲染一个用于 VCE 预算的明确整数输入。
function NumericSetting(props: {
  // Whether the setting must reject edits during an authoritative host mutation.
  // 设置在权威宿主修改期间是否必须拒绝编辑。
  disabled: boolean;
  // Human-facing setting label.
  // 面向用户的设置标签。
  label: string;
  // Current integer value.
  // 当前整数值。
  value: number;
  // Receives the exact parsed integer.
  // 接收精确解析的整数。
  onChange: (value: number) => void;
}) {
  return <label className="grid gap-1 text-sm">{props.label}<Input disabled={props.disabled} min={1} onChange={(event) => props.onChange(Number.parseInt(event.target.value, 10) || 0)} type="number" value={props.value} /></label>;
}

// ToggleSetting renders one labeled Boolean VCE setting.
// ToggleSetting 渲染一个带标签的布尔 VCE 设置。
function ToggleSetting(props: {
  // Current Boolean setting value.
  // 当前布尔设置值。
  checked: boolean;
  // Whether the toggle must reject edits during an authoritative host mutation.
  // 开关在权威宿主修改期间是否必须拒绝编辑。
  disabled: boolean;
  // Human-facing setting label.
  // 面向用户的设置标签。
  label: string;
  // Receives the next explicit Boolean value.
  // 接收下一项明确布尔值。
  onChange: (value: boolean) => void;
}) {
  return <label className="flex items-center gap-2 text-sm"><Switch checked={props.checked} disabled={props.disabled} onCheckedChange={props.onChange} /><span>{props.label}</span></label>;
}
