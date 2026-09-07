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
  VceJobSummary,
  VceMaintenancePlanResponse,
  VceOperationSummary,
  VceProjectOverviewResponse,
  VceProjectSettingsInput,
  VceProjectSettingsSummary,
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
import { useVceProjectJob } from "./use-vce-project-job";
import { useVceActionScope } from "./use-vce-action-scope";
import { VceJobProgress, vceSynchronizationControl } from "./vce-job-progress";

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
  onSave: (settings: VceProjectSettingsInput, expectedRevision: number) => Promise<VceProjectSettingsSummary>;
  // Shared activity read error; never inferred from empty management results.
  // 共享活动读取错误；绝不从空管理结果推断。
  activityError: string | null;
  // Reload shared lightweight authority without scanning management data; resolves after the read chain.
  // 重载共享轻量权威，不扫描管理数据；读取链完成后返回。
  onReloadActivity: () => Promise<void>;
  // Admit explicit initialization or synchronization and return its durable receipt, not completion.
  // 准入明确初始化或同步，返回其持久回执而非完成状态。
  onRefresh: () => Promise<VceJobSummary>;
  // Replaces the parent-owned overview after a management mutation.
  // 在管理修改后替换父级持有的概览。
  onOverviewChanged: (overview: VceProjectOverviewResponse) => void;
  // Navigate to host-owned global model settings; no arguments or mutation result.
  // 导航到宿主持有的全局模型设置；无参数及修改结果。
  onOpenEngineSettings: () => void;
}) {
  // locale selects local bilingual copy without exposing VCE implementation names to users.
  // locale 选择本地双语文案，且不向用户暴露 VCE 实现名称。
  const { locale } = useI18n();
  // indexing follows this project's independent durable job, not the conversation state.
  // indexing 跟随本项目独立持久作业，而非会话状态。
  const indexing = useVceProjectJob(props.projectId);
  // Use the acknowledged project job to offer at most one subsequent manual synchronization.
  // 使用已确认项目作业提供至多一次后续手动同步。
  const synchronization = vceSynchronizationControl(indexing.job, locale === "zh-CN");
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
  // draftBase freezes the exact persisted revision from which the user's editable draft was created.
  // draftBase 固定用户可编辑草稿创建时的精确持久修订。
  const draftBase = React.useRef<VceProjectSettingsSummary | null>(props.overview?.settings ?? null);
  // draftConflict warns about remote changes without discarding unsaved local edits.
  // draftConflict 提示远端变化，不丢弃未保存的本地编辑。
  const [draftConflict, setDraftConflict] = React.useState(false);
  // managementExpanded is a human request to load optional lists and aggregate counts.
  // managementExpanded 为人为请求加载可选列表和聚合计数。
  const [managementExpanded, setManagementExpanded] = React.useState(false);
  // managementOverview retains statistics only for their exact project/checkpoint/settings identity.
  // managementOverview 仅按其精确项目、检查点及设置身份保留统计。
  const [managementOverview, setManagementOverview] = React.useState<VceProjectOverviewResponse | null>(props.overview);
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
  // Actions retain the admitted project and service lifetime, not merely a reusable project ID.
  // 操作保留准入项目和服务生命周期，而不只是可重复使用的项目编号。
  const isCurrentAction = useVceActionScope(props.projectId);
  // maintenanceMutationInFlightRef prevents double submission before React can repaint disabled controls.
  // maintenanceMutationInFlightRef 防止 React 重绘禁用控件前重复提交维护操作。
  const maintenanceMutationInFlightRef = React.useRef(false);

  React.useLayoutEffect(() => {
    managementRequestVersion.current += 1;
    maintenanceMutationInFlightRef.current = false;
    setManagementBusy(false);
    setMaintenancePlan(null);
    setManagementError(null);
    return () => {
      managementRequestVersion.current += 1;
    };
  }, [isCurrentAction]);

  React.useEffect(() => {
    if (!props.overview) return;
    // settings is projected explicitly so host-owned metadata cannot be resubmitted.
    // settings 被显式投影，因此宿主持有的元数据不会被重新提交。
    const settings = props.overview.settings;
    if (draftBase.current !== null && draftBase.current.revision >= settings.revision) return;
    // A new checkpoint/progress snapshot is not a settings edit. A remote settings edit cannot
    // silently rebase a dirty draft onto a newer revision and overwrite someone else's choices.
    // 新检查点或进度快照不是设置编辑。远端设置编辑不能静默把脏草稿
    // 改基到新修订并覆盖他人的选择。
    if (draft !== null && draftBase.current !== null
      && JSON.stringify(draft) !== JSON.stringify(editableSettings(draftBase.current))) {
      setDraftConflict(true);
      return;
    }
    draftBase.current = settings;
    setDraft(editableSettings(settings));
    setDraftConflict(false);
  }, [props.overview, draft]);

  // reloadManagement binds content lists to one overview identity; operation history remains live.
  // reloadManagement 将内容列表绑定同一概览身份；操作历史保持实时。
  const reloadManagement = React.useCallback(async () => {
    const requestVersion = managementRequestVersion.current + 1;
    managementRequestVersion.current = requestVersion;
    // Retired details and lists cannot remain beside a newly published overview, even on read failure.
    // 即使读取失败，退役详情和列表也不能继续与新发布概览并列显示。
    setManagementOverview(null);
    setMaintenancePlan(null);
    setFilePage(emptyVceManagementPage());
    setOperationPage(emptyVceManagementPage());
    setIssuePage(emptyVceManagementPage());
    setBlob(null);
    if (!managementExpanded || !props.overview?.configuration.ready || !props.overview.settings.enabled || props.overview.status.checkpointVersion == null) {
      setFilePage(emptyVceManagementPage());
      setOperationPage(emptyVceManagementPage());
      setIssuePage(emptyVceManagementPage());
      setBlob(null);
      setManagementBusy(false);
      return;
    }
    setManagementBusy(true);
    setManagementError(null);
    try {
      // Pin the host-issued identity before requesting lists; never infer it from a numeric checkpoint.
      // 请求列表之前固定宿主签发身份；绝不从数字检查点推断身份。
      const overview = await loadAuthoritativeVceOverview(props.projectId);
      if (managementRequestVersion.current !== requestVersion) return;
      if (!overview.readIdentity) throw new Error(copy("索引尚无可读取的已发布版本。", "No published index is available for management reads."));
      const [filePage, operationPage, issuePage] = await Promise.all([
        listVceFiles(props.projectId, overview.readIdentity, null, VCE_MANAGEMENT_PAGE_SIZE),
        listVceOperations(props.projectId, null, VCE_MANAGEMENT_PAGE_SIZE),
        listVceIntegrityIssues(props.projectId, overview.readIdentity, null, VCE_MANAGEMENT_PAGE_SIZE),
      ]);
      if (managementRequestVersion.current !== requestVersion) {
        return;
      }
      setManagementOverview(overview);
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
  }, [copy, isCurrentAction, managementExpanded, props.overview?.configuration.ready, props.overview?.settings.enabled, props.overview?.status.checkpointVersion, props.overview?.settings.revision, props.projectId]);

  React.useEffect(() => {
    // A replacement can publish a new physical namespace at the same numeric checkpoint.
    // 替换可以在相同数字检查点发布新的物理命名空间。
    // Durable job identity/state changes retire lists and previews; progress-only sequence changes do not scan.
    // 持久作业身份或状态变化使列表与预览退役；仅进度序号变化不触发扫描。
    void reloadManagement();
    return () => {
      managementRequestVersion.current += 1;
    };
  }, [reloadManagement, props.overview?.status.checkpointVersion, indexing.job?.jobId, indexing.job?.state]);

  // loadManagementPage commits only the latest page response for the selected project.
  // loadManagementPage 只提交所选项目的最新分页响应。
  const loadManagementPage = React.useCallback(async <T,>(
    request: VceManagementPageRequest | null,
    loader: (cursor: string | null, readIdentity: string) => Promise<import("@/types").VcePageResponse<T>>,
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
      if (!managementOverview?.readIdentity) throw new Error(copy("请先刷新索引详情。", "Refresh index details first."));
      const response = await loader(request.cursor, managementOverview.readIdentity);
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
  }, [copy, managementOverview]);

  // inspectBlob loads only the selected immutable Blob identity.
  // inspectBlob 只加载所选不可变 Blob 身份。
  const inspectBlob = async (blobName: string | null | undefined) => {
    if (!blobName) {
      return;
    }
    const projectId = props.projectId;
    // Use the same request fence as overview and pagination so delayed details cannot reappear.
    // 与概览及分页共用请求保护，避免迟到详情重新出现。
    const requestVersion = ++managementRequestVersion.current;
    setBlob(null);
    setManagementBusy(true);
    setManagementError(null);
    try {
      if (!managementOverview?.readIdentity) throw new Error(copy("请先刷新索引详情。", "Refresh index details first."));
      const nextBlob = await loadVceBlobDetail(projectId, blobName, managementOverview.readIdentity);
      if (isCurrentAction() && managementRequestVersion.current === requestVersion) {
        setBlob(nextBlob);
      }
    } catch (error) {
      if (isCurrentAction() && managementRequestVersion.current === requestVersion) {
        setManagementError(error instanceof Error ? error.message : String(error));
      }
    } finally {
      if (isCurrentAction() && managementRequestVersion.current === requestVersion) {
        setManagementBusy(false);
      }
    }
  };

  // loadMaintenancePlan obtains a read-only plan and performs no mutation.
  // loadMaintenancePlan 获取只读计划且不执行修改。
  const loadMaintenancePlan = async (mode: "repair" | "rebuild" | "cleanup_staging") => {
    const projectId = props.projectId;
    // A preview is retired by the same publication, collapse and reload boundary as its displayed data.
    // 预览与其显示数据共享发布、折叠及重载退役边界。
    const requestVersion = ++managementRequestVersion.current;
    setMaintenancePlan(null);
    setManagementBusy(true);
    setManagementError(null);
    try {
      const nextPlan = await planVceMaintenance(projectId, mode);
      if (isCurrentAction() && managementRequestVersion.current === requestVersion) {
        setMaintenancePlan(nextPlan);
      }
    } catch (error) {
      if (isCurrentAction() && managementRequestVersion.current === requestVersion) {
        setManagementError(error instanceof Error ? error.message : String(error));
      }
    } finally {
      if (isCurrentAction() && managementRequestVersion.current === requestVersion) {
        setManagementBusy(false);
      }
    }
  };

  // Save next against its frozen revision; refresh lightweight authority on failure without erasing edits.
  // 按冻结修订保存 next；失败时刷新轻量权威，不擦除编辑。
  // Returns after acknowledgement or visible error handling; management scans are never required.
  // 确认或可见错误处理后返回；绝不要求管理扫描。
  const saveDraft = async (next: VceProjectSettingsInput) => {
    if (draftBase.current === null) return;
    setManagementError(null);
    try {
      // saved is the acknowledged settings document, not a synthesized local revision.
      // saved 为已确认设置文档，绝非本地合成修订。
      // Preserve raw draft lines while typing; remove blank rules only at the existing save boundary.
      // 输入时保留原始草稿行；仅在既有保存边界移除空规则。
      const saved = await props.onSave({
        ...next,
        extraIgnorePatterns: next.extraIgnorePatterns.map((value) => value.trim()).filter(Boolean),
      }, draftBase.current.revision);
      if (isCurrentAction()) {
        draftBase.current = saved;
        setDraft(editableSettings(saved));
        setDraftConflict(false);
      }
    } catch (error) {
      if (isCurrentAction()) {
        setManagementError(error instanceof Error ? error.message : String(error));
        // Shared read errors are rendered independently through activityError, never swallowed here.
        // 共享读取错误通过 activityError 独立渲染，绝不在此吞掉。
        await props.onReloadActivity();
      }
    }
  };

  // refreshIndex reports a rejected refresh locally instead of creating an unhandled promise.
  // refreshIndex 在本地报告被拒绝的刷新，而不是产生未处理 Promise。
  const refreshIndex = async () => {
    if (!props.overview?.configuration.ready || !props.overview.settings.enabled) return;
    setManagementError(null);
    try {
      const receipt = await props.onRefresh();
      if (isCurrentAction()) indexing.receive(receipt);
    } catch (error) {
      if (isCurrentAction()) setManagementError(error instanceof Error ? error.message : String(error));
    }
  };

  // Refresh visible metadata only; this icon has no authority to scan or initialize an index.
  // 仅刷新可见元数据；此图标无权扫描或初始化索引。
  // Takes no arguments and replaces only the current project's authoritative overview.
  // 无参数，仅替换当前项目权威概览。
  const refreshStatus = async () => {
    try {
      await props.onReloadActivity();
    } catch (error) {
      if (isCurrentAction()) setManagementError(error instanceof Error ? error.message : String(error));
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
      `确认执行 ${maintenancePlan.mode} 维护？该操作将影响 ${maintenancePlan.affectedBlobCount} 个 Blob，目标向量维度为 ${maintenancePlan.embeddingDimension}。`,
      `Execute ${maintenancePlan.mode} maintenance affecting ${maintenancePlan.affectedBlobCount} Blobs with target dimension ${maintenancePlan.embeddingDimension}?`,
    ));
    if (!confirmed) {
      return;
    }
    maintenanceMutationInFlightRef.current = true;
    setManagementBusy(true);
    setManagementError(null);
    try {
      // result distinguishes durable admission from completed cleanup; it is never assumed to be completion.
      // result 区分持久准入与已完成清理；绝不把准入当作完成。
      const result = await executeVceMaintenance(
        projectId,
        maintenancePlan.mode,
        overview.settings.revision,
        checkpoint,
        maintenancePlan.actions.map((action) => action.actionId),
        crypto.randomUUID(),
        maintenancePlan.embeddingFingerprint,
      );
      if (!isCurrentAction()) {
        return;
      }
      if (result.kind === "job") {
        indexing.receive(result.job);
      } else {
        props.onOverviewChanged(result.overview);
      }
      setMaintenancePlan(null);
      await reloadManagement();
    } catch (error) {
      if (isCurrentAction()) {
        setManagementError(error instanceof Error ? error.message : String(error));
      }
    } finally {
      if (isCurrentAction()) {
        maintenanceMutationInFlightRef.current = false;
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
      const receipt = await retryVceOperation(
        projectId,
        operationId,
        overview.settings.revision,
        overview.status.checkpointVersion ?? null,
      );
      if (!isCurrentAction()) {
        return;
      }
      indexing.receive(receipt);
    } catch (error) {
      if (isCurrentAction()) {
        setManagementError(error instanceof Error ? error.message : String(error));
      }
    } finally {
      if (isCurrentAction()) {
        maintenanceMutationInFlightRef.current = false;
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
      const receipt = await cancelVceOperation(
        projectId,
        operationId,
      );
      if (!isCurrentAction()) {
        return;
      }
      indexing.receive(receipt);
    } catch (error) {
      if (isCurrentAction()) {
        setManagementError(error instanceof Error ? error.message : String(error));
      }
    } finally {
      if (isCurrentAction()) {
        maintenanceMutationInFlightRef.current = false;
        setManagementBusy(false);
      }
    }
  };

  if (!props.overview || !draft) {
    return (
      <div className="project-settings-section">
        {props.activityError ? <>
          <p role="alert" className="text-sm text-destructive">{props.activityError}</p>
          <Button type="button" onClick={() => void props.onReloadActivity()}>{copy("重新读取状态", "Reload status")}</Button>
        </> : <>
          <LoaderCircle className="animate-spin" size={18} />
          <p className="project-settings-muted">{copy("正在加载代码语义索引…", "Loading code semantic index…")}</p>
        </>}
      </div>
    );
  }

  // status exposes lifecycle and checkpoint state without provider implementation details.
  // status 暴露生命周期与检查点状态且不包含 Provider 实现细节。
  const status = props.overview.status;
  // counts supplies aggregate storage statistics when the runtime is active.
  // counts 在运行时活动时提供存储聚合统计。
  const counts = managementOverview?.status.projectId === props.projectId
    && managementOverview.status.checkpointVersion === status.checkpointVersion
    && managementOverview.settings.revision === props.overview.settings.revision
    ? managementOverview.counts : null;
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
          onClick={() => void refreshStatus()}
          size="icon-sm"
          type="button"
          variant="outline"
        >
          <RefreshCw className={props.busy ? "animate-spin" : undefined} size={15} />
        </Button>
      </header>

      {props.activityError ? <p role="alert" className="text-sm text-destructive">{props.activityError}</p> : null}
      {draftConflict ? <div role="alert" className="rounded-lg border border-amber-500/30 p-3 text-sm">
        <p>{copy("项目设置已在其他位置更新。您的未保存修改已保留；请重新载入设置后再保存，避免覆盖新设置。", "Project settings changed elsewhere. Unsaved edits are preserved; reload before saving to avoid overwriting newer settings.")}</p>
        <Button type="button" variant="outline" size="sm" disabled={props.busy} onClick={() => {
          draftBase.current = props.overview!.settings;
          setDraft(editableSettings(props.overview!.settings));
          setDraftConflict(false);
        }}>{copy("放弃修改并载入当前设置", "Discard edits and load current settings")}</Button>
      </div> : null}

      {!props.overview.configuration.ready ? <div className="rounded-lg border border-amber-500/30 bg-amber-500/5 p-3 text-sm">
        <p>{copy("引擎模型尚未就绪。您可以保存项目设置，但创建、更新索引和 AI 检索保持关闭。", "Engine models are not ready. Project settings can be saved, but indexing and AI retrieval remain disabled.")}</p>
        <Button type="button" variant="link" size="sm" onClick={props.onOpenEngineSettings}>{copy("配置语义引擎模型", "Configure engine models")}</Button>
      </div> : null}

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
          <small>{copy("开启只授权此项目使用语义引擎；索引需要单独创建，默认手动更新。", "Enabling only authorizes this project to use the engine; create its index separately. Updates are manual by default.")}</small>
        </span>
      </div>

      {draft.enabled ? (
        <section className="space-y-3 rounded-lg border p-4">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div>
              <strong>{status.checkpointVersion == null ? copy("创建项目索引", "Create project index") : copy("更新项目索引", "Update project index")}</strong>
              <p className="mt-1 text-sm text-muted-foreground">{status.checkpointVersion == null ? copy("只有点击创建，才会扫描此项目并调用向量模型。", "Files are scanned and embedded only after you choose to create this project's index.") : copy("手动同步立即受理，不受自动更新间隔限制。", "Manual synchronization bypasses the automatic update interval.")}</p>
            </div>
            <Button type="button" disabled={props.busy || synchronization.disabled || !props.overview.configuration.ready || !props.overview.settings.enabled || status.lifecycle === "rebuild_required"} onClick={() => void refreshIndex()}>
              <Database size={15} />{status.checkpointVersion == null ? copy("创建语义索引", "Create semantic index") : synchronization.label}
            </Button>
          </div>
          {indexing.job ? <VceJobProgress job={indexing.job} chinese={locale === "zh-CN"} onCancel={() => void indexing.cancel()} cancelPending={indexing.cancelPending}
            onRetry={indexing.job.kind !== "rebuild" && !managementBusy && props.overview?.configuration.ready === true && draft.enabled && props.overview.status.lifecycle !== "rebuild_required"
              ? () => void retryOperation(indexing.job!.jobId) : undefined} /> : null}
          {indexing.error ? <p role="alert" className="text-sm text-destructive">{copy("索引状态暂不可用：", "Index status unavailable: ")}{indexing.error}</p> : null}
          <fieldset className="space-y-2" disabled={props.busy}>
            <legend className="text-sm font-medium">{copy("索引更新模式", "Index update mode")}</legend>
            <div className="flex flex-wrap gap-4 text-sm">
              {(["manual", "automatic"] as const).map((mode) => <label key={mode} className="flex items-center gap-2"><input type="radio" name={`vce-update-mode-${props.projectId}`} checked={draft.updateMode === mode} onChange={() => { const next = { ...draft, updateMode: mode }; setDraft(next); void saveDraft(next); }} />{mode === "manual" ? copy("手动更新", "Manual updates") : copy("自动更新", "Automatic updates")}</label>)}
            </div>
            <p className="text-xs text-muted-foreground">{copy("此设置属于项目，所有会话共享。自动更新仅在首次初始化成功后跟随文件变化。", "This project setting is shared across conversations. Automatic file-change updates start only after initialization succeeds.")}</p>
          </fieldset>
          <label className="flex items-center gap-3 text-sm">
            {copy("自动更新最低间隔（秒）", "Minimum automatic interval (seconds)")}
            <Input className="w-28" type="number" min={60} max={86400} disabled={props.busy} value={draft.autoUpdateMinIntervalSeconds} onChange={(event) => setDraft({ ...draft, autoUpdateMinIntervalSeconds: Number(event.target.value) })} />
            <Button type="button" size="sm" variant="outline" disabled={props.busy || !Number.isInteger(draft.autoUpdateMinIntervalSeconds) || draft.autoUpdateMinIntervalSeconds < 60 || draft.autoUpdateMinIntervalSeconds > 86400} onClick={() => void saveDraft(draft)}>{copy("保存间隔", "Save interval")}</Button>
          </label>
        </section>
      ) : null}

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
            onChange={(event) => setDraft({ ...draft, extraIgnorePatterns: event.target.value.split(/\r?\n/) })}
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

      {props.overview.configuration.ready && draft.enabled && status.checkpointVersion != null ? <Button
        type="button" variant="outline" aria-expanded={managementExpanded} onClick={() => setManagementExpanded(!managementExpanded)}>
        {managementExpanded ? copy("收起索引详情", "Hide index details") : copy("查看索引详情与统计", "View index details and statistics")}
      </Button> : null}
      {managementExpanded && props.overview.configuration.ready && draft.enabled && status.checkpointVersion != null ? (
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
                (cursor, readIdentity) => listVceFiles(props.projectId, readIdentity, cursor, VCE_MANAGEMENT_PAGE_SIZE),
                setFilePage,
              )}
              onPrevious={() => void loadManagementPage(
                previousVceManagementPageRequest(filePage),
                (cursor, readIdentity) => listVceFiles(props.projectId, readIdentity, cursor, VCE_MANAGEMENT_PAGE_SIZE),
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
                (cursor, readIdentity) => listVceIntegrityIssues(props.projectId, readIdentity, cursor, VCE_MANAGEMENT_PAGE_SIZE),
                setIssuePage,
              )}
              onPrevious={() => void loadManagementPage(
                previousVceManagementPageRequest(issuePage),
                (cursor, readIdentity) => listVceIntegrityIssues(props.projectId, readIdentity, cursor, VCE_MANAGEMENT_PAGE_SIZE),
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
    updateMode: settings.updateMode,
    autoUpdateMinIntervalSeconds: settings.autoUpdateMinIntervalSeconds,
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
