import * as React from "react";
import { Box, ExternalLink, FolderCheck, Gauge, RefreshCw, RotateCcw } from "lucide-react";
import { toast } from "sonner";
import {
  PageActionBar, PageBody, PageEmptyState, PageFacts, PageHint, PageItem, PageItemList, PageNotice, PageRow, PageSection,
  PageToolbar, type PageFact,
} from "@/components/page";
import { StatusPill } from "@/components/shared/status";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { StreamPlaceholder } from "@/components/ui/stream-placeholder";
import { Switch } from "@/components/ui/switch";
import { formatMessage, useI18n, type Translate, type TranslationKey } from "@/i18n";
import { errorMessage, formatBytes } from "@/lib/display-format";
import { PermissionJudgeCloudSection } from "@/components/features/settings/permission-judge-cloud-section";
import { DEVICE_KEYS, PHASE_KEYS, REASON_KEYS, label } from "@/components/features/settings/permission-judge-labels";
import { PermissionJudgeRegisterForm } from "@/components/features/settings/permission-judge-register-form";
import { PermissionJudgeEntryContext } from "@/components/composer/permission-judge-entry-context";
import { PermissionReviewRouteSection } from "@/components/features/settings/permission-review-route-section";
import {
  activatePermissionJudgeModel,
  benchmarkPermissionJudge,
  installPermissionJudgeCuda,
  loadPermissionJudgeSettings,
  loadPermissionJudgeStatus,
  registerPermissionJudgeModel,
  reloadPermissionJudge,
  removePermissionJudgeModel,
  rollbackPermissionJudgeModel,
  savePermissionJudgeSettings,
} from "@/service/permission-judge-service";
import { openExternalUrl } from "@/service/project-actions-service";
import type {
  JudgeBackend,
  JudgeDevice,
  PermissionJudgeBenchmarkResult,
  PermissionJudgeCudaComponent,
  PermissionJudgeSettingsSnapshot,
  PermissionJudgeStatus,
} from "@/types";

// Labels of stable codes sent by the service; unknown codes render as themselves.
// 服务发送的稳定代码对应的标签；未知代码按原样显示。
const BACKEND_KEYS: Record<JudgeBackend, TranslationKey> = {
  local: "settings.permissionJudge.backend.local",
  cloud: "settings.permissionJudge.backend.cloud",
};
const INSTALL_PHASE_KEYS: Record<string, TranslationKey> = {
  downloading: "settings.permissionJudge.cuda.phase.downloading",
  verifying: "settings.permissionJudge.cuda.phase.verifying",
  extracting: "settings.permissionJudge.cuda.phase.extracting",
  completed: "settings.permissionJudge.cuda.phase.completed",
};
const SOURCE_KEYS: Record<string, TranslationKey> = {
  managed: "settings.permissionJudge.cuda.source.managed",
  toolkit: "settings.permissionJudge.cuda.source.toolkit",
  search_path: "settings.permissionJudge.cuda.source.searchPath",
};
const LINK_KEYS: Record<string, TranslationKey> = {
  cuda_toolkit: "settings.permissionJudge.cuda.link.toolkit",
  cuda_toolkit_archive: "settings.permissionJudge.cuda.link.toolkitArchive",
  nvidia_driver: "settings.permissionJudge.cuda.link.driver",
  cuda_redistributables: "settings.permissionJudge.cuda.link.redistributables",
  cuda_compatibility: "settings.permissionJudge.cuda.link.compatibility",
  onnxruntime_cuda_requirements: "settings.permissionJudge.cuda.link.onnxruntimeRequirements",
};
const COMPONENT_KEYS: Record<PermissionJudgeCudaComponent, TranslationKey> = {
  onnxruntime: "settings.permissionJudge.cuda.onnxruntime",
  cuda_libraries: "settings.permissionJudge.cuda.libraries",
};

// formatMs renders a duration, or a dash when the reading is absent.
// formatMs 渲染时长；读数缺失时显示破折号。
function formatMs(t: Translate, value: number | null | undefined): string {
  return value == null ? "—" : formatMessage(t("settings.permissionJudge.ms"), { value: value.toFixed(1) });
}

// formatOptionalBytes renders a size, or a dash when the reading is absent.
// formatOptionalBytes 渲染大小；读数缺失时显示破折号。
function formatOptionalBytes(value: number | null | undefined): string {
  return value == null ? "—" : formatBytes(value);
}

// Draft holds the system-level choices until the user saves them.
// Draft 在用户保存前保存系统级选择。
interface Draft {
  enabled: boolean;
  backend: JudgeBackend;
  device: JudgeDevice | null;
  cudaLibraryDirectory: string;
  llmMayAllowBlocked: boolean;
}

// draftOf reads the saved choices; an unset backend is local and an unset device follows the
// recommendation.
// draftOf 读取已保存的选择；未设置的后端为本机，未设置的设备跟随推荐值。
function draftOf(snapshot: PermissionJudgeSettingsSnapshot): Draft {
  return {
    enabled: snapshot.state === "enabled",
    backend: snapshot.backend ?? "local",
    device: snapshot.device ?? snapshot.recommendedDevice,
    cudaLibraryDirectory: snapshot.cuda.lockedDirectory ?? "",
    llmMayAllowBlocked: snapshot.llmMayAllowBlocked,
  };
}

// Failure is one failed action: a readable outcome plus the raw service error.
// Failure 是一次失败的操作：可读结果加原始服务错误。
interface Failure {
  message: string;
  detail: string;
}

// PermissionJudgeSettingsArea manages the permission judge with the shared page kit: the saved
// state in the toolbar, the decision choices committed through the save bar, then the cloud
// decision route or the local models, live status and CUDA runtime, the LLM review and speed tests.
// Nothing persists until the user saves or acts.
// PermissionJudgeSettingsArea 以共享页面套件管理权限判定：工具栏显示已保存状态，判定选项经保存栏提交，
// 其后是云端判定路由或本机模型、实时状态与 CUDA 执行库，以及 LLM 复核与速度测试。用户保存或操作前不持久化任何内容。
// Parameters: none. Returns the settings page.
// 参数：无。返回设置页。
export function PermissionJudgeSettingsArea() {
  const { t } = useI18n();
  // openGuide re-enters the setup guide; absent outside the workbench layout.
  // openGuide 重新进入初始化引导；在工作台布局之外为空。
  const openGuide = React.useContext(PermissionJudgeEntryContext)?.openGuide;
  // snapshot is the last full read; status is refreshed separately by polling.
  // snapshot 是最近一次完整读取；status 由轮询单独刷新。
  const [snapshot, setSnapshot] = React.useState<PermissionJudgeSettingsSnapshot | null>(null);
  const [status, setStatus] = React.useState<PermissionJudgeStatus | null>(null);
  const [draft, setDraft] = React.useState<Draft | null>(null);
  // busy names the running action so only one action runs at a time.
  // busy 记录正在执行的操作，保证同一时间只执行一个操作。
  const [busy, setBusy] = React.useState<string | null>(null);
  const [failure, setFailure] = React.useState<Failure | null>(null);
  const [loadError, setLoadError] = React.useState<string | null>(null);
  // benchmarks keeps this page's speed test results, newest first; they are never saved.
  // benchmarks 保存本页速度测试结果（最新在前），不会持久化。
  const [benchmarks, setBenchmarks] = React.useState<PermissionJudgeBenchmarkResult[]>([]);
  // cloudConfigured tells whether a cloud decision route is saved, as reported by its section.
  // cloudConfigured 表示是否已保存云端判定路由，由其区块报告。
  const [cloudConfigured, setCloudConfigured] = React.useState(false);
  // failureRef brings a new failure into view, since the action that failed may be far below it.
  // failureRef 将新失败滚动到可见处，因为失败的操作可能位于其下方很远。
  const failureRef = React.useRef<HTMLDivElement>(null);
  React.useEffect(() => {
    if (failure) failureRef.current?.scrollIntoView?.({ block: "nearest", behavior: "smooth" });
  }, [failure]);

  // applySnapshot replaces the page state with a fresh service read.
  // applySnapshot 用最新服务读取结果替换页面状态。
  const applySnapshot = React.useCallback((next: PermissionJudgeSettingsSnapshot) => {
    setSnapshot(next);
    setStatus(next.status);
    setDraft(draftOf(next));
  }, []);

  const reload = React.useCallback(async () => {
    try {
      applySnapshot(await loadPermissionJudgeSettings());
      setLoadError(null);
    } catch (cause) {
      setLoadError(errorMessage(cause));
    }
  }, [applySnapshot]);

  React.useEffect(() => { void reload(); }, [reload]);

  // Poll the live status at the interval it publishes; a finished CUDA installation re-reads detection.
  // 按实时状态发布的间隔轮询；CUDA 安装结束后重新读取探测结果。
  const installRunning = status?.cudaInstall?.running ?? false;
  const pollIntervalMs = status?.pollIntervalMs;
  const wasInstalling = React.useRef(false);
  React.useEffect(() => {
    if (pollIntervalMs === undefined) return;
    let cancelled = false;
    const timer = window.setInterval(() => {
      void loadPermissionJudgeStatus().then((next) => { if (!cancelled) setStatus(next); }).catch(() => undefined);
    }, pollIntervalMs);
    return () => { cancelled = true; window.clearInterval(timer); };
  }, [pollIntervalMs]);
  React.useEffect(() => {
    if (wasInstalling.current && !installRunning) void reload();
    wasInstalling.current = installRunning;
  }, [installRunning, reload]);

  // run executes one action, keeping the failure visible until the next action.
  // run 执行一个操作，失败信息保持显示直到下一次操作。
  const run = async (name: string, failureKey: TranslationKey, action: () => Promise<void>) => {
    if (busy) return;
    setBusy(name);
    setFailure(null);
    try {
      await action();
    } catch (cause) {
      setFailure({ message: t(failureKey), detail: errorMessage(cause) });
    } finally {
      setBusy(null);
    }
  };

  if (!snapshot || !draft || !status) {
    return <PageBody>
      {loadError
        ? <PageNotice tone="error" details={loadError} detailsLabel={t("settings.errors.showErrorDetails")}
          actions={<Button type="button" size="sm" variant="outline" onClick={() => void reload()}>
            <RefreshCw size={14} />{t("settings.permissionJudge.reload")}
          </Button>}>
          {t("settings.permissionJudge.loadFailed")}
        </PageNotice>
        : <StreamPlaceholder label={t("ui.loading")} shape="rows" count={4} />}
    </PageBody>;
  }

  const saved = draftOf(snapshot);
  const dirty = JSON.stringify(saved) !== JSON.stringify(draft);
  const activeModel = snapshot.models.find((model) => model.id === snapshot.activeModelId) ?? null;
  const local = draft.backend === "local";
  const backendReady = local ? activeModel !== null : cloudConfigured;
  const savedCloud = snapshot.state === "enabled" && snapshot.backend === "cloud";
  const benchmarkReady = savedCloud
    ? !dirty
    : snapshot.backend !== "cloud" && status.worker.phase === "ready";
  // The toolbar summarizes what is saved, not the unsaved draft.
  // 工具栏概括已保存的内容，而非未保存草稿。
  const stateLabel = snapshot.state === "enabled"
    ? `${t("settings.permissionJudge.state.enabled")} · ${t(BACKEND_KEYS[snapshot.backend ?? "local"])}`
    : t(snapshot.state === "off" ? "settings.permissionJudge.state.off" : "settings.permissionJudge.state.unset");
  const stateTone = snapshot.state === "enabled" ? "ready" : snapshot.state === "off" ? "pending" : "warning";
  const save = () => run("save", "settings.permissionJudge.saveFailed", async () => {
    const directory = draft.cudaLibraryDirectory.trim();
    applySnapshot(await savePermissionJudgeSettings({
      state: draft.enabled ? "enabled" : "off",
      backend: draft.backend,
      device: draft.device,
      cudaLibraryDirectory: directory === "" ? null : directory,
      llmMayAllowBlocked: draft.llmMayAllowBlocked,
    }));
    toast.success(t("settings.permissionJudge.saved"));
  });

  return <PageBody data-permission-judge-page="">
    <PageToolbar actions={openGuide
      ? <Button type="button" size="sm" variant="outline" onClick={openGuide}>{t("setupGuide.open")}</Button>
      : null}>
      <StatusPill label={stateLabel} status={stateTone} />
    </PageToolbar>

    {!backendReady ? <PageNotice tone="info">
      {t(local ? "settings.permissionJudge.needsModel" : "settings.permissionJudge.needsCloudRoute")}
    </PageNotice> : null}
    {failure ? <div ref={failureRef}>
      <PageNotice tone="error" details={failure.detail} detailsLabel={t("settings.errors.showErrorDetails")}>
        {failure.message}
      </PageNotice>
    </div> : null}

    <PageSection layout="rows" title={t("settings.permissionJudge.decision")} description={t("settings.permissionJudge.decisionHint")}>
      <PageRow label={t("settings.permissionJudge.enabled")} description={t("settings.permissionJudge.enabledHint")}>
        <Switch aria-label={t("settings.permissionJudge.enabled")} checked={draft.enabled}
          disabled={busy !== null || (!draft.enabled && !backendReady)}
          onCheckedChange={(enabled) => setDraft({ ...draft, enabled })} />
      </PageRow>
      <PageRow label={t("settings.permissionJudge.backend")} description={t("settings.permissionJudge.backendHint")}>
        <Select disabled={busy !== null} value={draft.backend}
          onValueChange={(backend) => setDraft({ ...draft, backend: backend as JudgeBackend })}>
          <SelectTrigger aria-label={t("settings.permissionJudge.backend")} className="w-56"><SelectValue /></SelectTrigger>
          <SelectContent>
            {(Object.keys(BACKEND_KEYS) as JudgeBackend[]).map((backend) => (
              <SelectItem key={backend} value={backend}>{t(BACKEND_KEYS[backend])}</SelectItem>
            ))}
          </SelectContent>
        </Select>
      </PageRow>
      {local ? <PageRow label={t("settings.permissionJudge.device")} description={snapshot.recommendedDevice
        ? formatMessage(t("settings.permissionJudge.recommended"), { device: t(DEVICE_KEYS[snapshot.recommendedDevice]) })
        : undefined}>
        <Select disabled={busy !== null} value={draft.device ?? undefined}
          onValueChange={(device) => setDraft({ ...draft, device: device as JudgeDevice })}>
          <SelectTrigger aria-label={t("settings.permissionJudge.device")} className="w-56"><SelectValue /></SelectTrigger>
          <SelectContent>
            {snapshot.devices.map((device) => (
              <SelectItem disabled={device.unavailableReason !== null} key={device.device} value={device.device}>
                {t(DEVICE_KEYS[device.device])}
                {device.unavailableReason ? ` · ${label(t, REASON_KEYS, device.unavailableReason)}` : ""}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </PageRow> : null}
      <PageRow label={t("settings.permissionJudge.llmMayAllowBlocked")} description={t("settings.permissionJudge.llmMayAllowBlockedHint")}>
        <Switch aria-label={t("settings.permissionJudge.llmMayAllowBlocked")} checked={draft.llmMayAllowBlocked}
          disabled={busy !== null} onCheckedChange={(llmMayAllowBlocked) => setDraft({ ...draft, llmMayAllowBlocked })} />
      </PageRow>
    </PageSection>

    {local ? <>
      <ModelsSection applySnapshot={applySnapshot} busy={busy} run={run} snapshot={snapshot} t={t} />
      <WorkerStatusSection busy={busy} run={run} snapshot={snapshot} status={status} t={t}
        onReloaded={(next) => setStatus(next)} />
    </> : <PermissionJudgeCloudSection onConfiguredChange={setCloudConfigured} />}

    <PermissionReviewRouteSection />

    <PageSection layout="rows" title={t("settings.permissionJudge.benchmark")}
      description={t(savedCloud ? "settings.permissionJudge.benchmarkCloudHint" : "settings.permissionJudge.benchmarkHint")}
      actions={([1, 10] as const).map((batchSize) => (
        <Button key={batchSize} type="button" variant="outline" size="sm"
          disabled={busy !== null || !benchmarkReady}
          onClick={() => void run(`benchmark-${batchSize}`, "settings.permissionJudge.benchmarkFailed", async () => {
            const result = await benchmarkPermissionJudge(batchSize);
            setBenchmarks((previous) => [result, ...previous].slice(0, 6));
          })}>
          <Gauge size={14} />{t(batchSize === 1 ? "settings.permissionJudge.benchmarkSingle" : "settings.permissionJudge.benchmarkBatch")}
        </Button>
      ))}>
      {benchmarks.length > 0 ? benchmarks.map((result, index) => (
        <PageItem key={index} icon={<Gauge />} title={result.device
          ? formatMessage(t("settings.permissionJudge.benchmarkResult"), {
            batch: result.batchSize,
            device: t(DEVICE_KEYS[result.device]),
            p50: formatMs(t, result.p50Ms),
            p95: formatMs(t, result.p95Ms),
            perItem: formatMs(t, result.perItemMs),
            rate: result.itemsPerSecond.toFixed(1),
          })
          : formatMessage(t("settings.permissionJudge.benchmarkCloudResult"), {
            batch: result.batchSize,
            model: result.modelId,
            total: formatMs(t, result.p50Ms),
            perItem: formatMs(t, result.perItemMs),
            rate: result.itemsPerSecond.toFixed(1),
          })} />
      )) : null}
    </PageSection>

    {local ? <CudaSection busy={busy} draft={draft} run={run} setDraft={setDraft} snapshot={snapshot} status={status} t={t}
      onStatus={(next) => setStatus(next)} /> : null}

    {dirty ? <PageActionBar pending={busy === "save"} actions={<>
      <Button type="button" size="sm" variant="outline" disabled={busy !== null} onClick={() => setDraft(saved)}>
        {t("settings.permissionJudge.discard")}
      </Button>
      <Button type="button" size="sm" disabled={busy !== null} onClick={() => void save()}>{t("settings.permissionJudge.save")}</Button>
    </>}>
      {t("settings.permissionJudge.unsaved")}
    </PageActionBar> : null}
  </PageBody>;
}

// RunAction executes one named page action and reports its failure.
// RunAction 执行一个具名页面操作并报告其失败。
type RunAction = (name: string, failureKey: TranslationKey, action: () => Promise<void>) => Promise<void>;

// WorkerStatusSection shows the loaded model's phase, memory, GPU use and latency.
// WorkerStatusSection 展示已加载模型的阶段、内存、GPU 占用与延迟。
function WorkerStatusSection(props: {
  busy: string | null;
  run: RunAction;
  snapshot: PermissionJudgeSettingsSnapshot;
  status: PermissionJudgeStatus;
  t: Translate;
  onReloaded: (status: PermissionJudgeStatus) => void;
}) {
  const { t } = props;
  const worker = props.status.worker;
  const model = props.snapshot.models.find((candidate) => candidate.id === worker.modelId);
  const facts: PageFact[] = [
    { key: "model", label: t("settings.permissionJudge.status.model"), value: model?.name ?? worker.modelId ?? "—" },
    { key: "device", label: t("settings.permissionJudge.status.device"), value: worker.device ? t(DEVICE_KEYS[worker.device]) : "—" },
    { key: "process", label: t("settings.permissionJudge.status.process"), value: worker.processId == null ? "—" : String(worker.processId) },
    { key: "memory", label: t("settings.permissionJudge.status.memory"), value: formatMessage(t("settings.permissionJudge.status.memoryValue"), {
      workingSet: formatOptionalBytes(worker.workingSetBytes),
      private: formatOptionalBytes(worker.privateBytes),
    }) },
    { key: "gpuMemory", label: t("settings.permissionJudge.status.gpuMemory"), value: formatMessage(t("settings.permissionJudge.status.gpuMemoryValue"), {
      dedicated: formatOptionalBytes(worker.gpuDedicatedBytes),
      shared: formatOptionalBytes(worker.gpuSharedBytes),
    }) },
    { key: "gpuUtilization", label: t("settings.permissionJudge.status.gpuUtilization"), value: worker.gpuUtilizationPercent == null ? "—" : `${worker.gpuUtilizationPercent.toFixed(0)}%` },
    { key: "startup", label: t("settings.permissionJudge.status.startup"), value: formatMessage(t("settings.permissionJudge.status.startupValue"), {
      verify: formatMs(t, worker.verifyMs),
      load: formatMs(t, worker.loadMs),
      warmup: formatMs(t, worker.warmupMs),
    }) },
    { key: "latency", label: t("settings.permissionJudge.status.latency"), value: formatMessage(t("settings.permissionJudge.status.latencyValue"), {
      p50: formatMs(t, worker.recentP50Ms),
      p95: formatMs(t, worker.recentP95Ms),
      count: worker.evaluations,
    }) },
  ];
  const pill = worker.phase === "ready" ? "ready" : worker.phase === "failed" ? "error" : worker.phase === "warming" ? "warning" : "pending";
  return <PageSection title={t("settings.permissionJudge.status")}
    status={<StatusPill label={label(t, PHASE_KEYS, worker.phase)} status={pill} />}
    actions={worker.phase === "failed" ? <Button type="button" size="sm" variant="outline" disabled={props.busy !== null}
      onClick={() => void props.run("reload", "settings.permissionJudge.reloadFailed", async () => props.onReloaded(await reloadPermissionJudge()))}>
      <RefreshCw size={14} />{t("settings.permissionJudge.reload")}
    </Button> : undefined}>
    {worker.phase === "failed" && worker.error ? <PageNotice tone="error" details={worker.error} detailsLabel={t("settings.errors.showErrorDetails")}>
      {t("settings.permissionJudge.status.loadFailed")}
    </PageNotice> : null}
    {props.snapshot.activeModelIssue ? <PageNotice tone="error" details={props.snapshot.activeModelIssue.detail} detailsLabel={t("settings.errors.showErrorDetails")}>
      {formatMessage(t("settings.permissionJudge.activeModelIssue"), { code: props.snapshot.activeModelIssue.code })}
    </PageNotice> : null}
    <PageFacts items={facts} layout="grid" />
  </PageSection>;
}

// ModelsSection lists registered models, then registers new ones from local folders in its own section.
// ModelsSection 列出已登记模型，并在独立分区中从本机文件夹登记新模型。
function ModelsSection(props: {
  applySnapshot: (snapshot: PermissionJudgeSettingsSnapshot) => void;
  busy: string | null;
  run: RunAction;
  snapshot: PermissionJudgeSettingsSnapshot;
  t: Translate;
}) {
  const { t, snapshot } = props;
  const packageName = (id: string | null) => snapshot.knownPackages.find((known) => known.id === id)?.displayName ?? id ?? "";
  const previous = snapshot.previousModelId
    ? snapshot.models.find((model) => model.id === snapshot.previousModelId) ?? null
    : null;
  return <>
    <PageSection layout="rows" title={t("settings.permissionJudge.models")} description={t("settings.permissionJudge.modelsHint")}
      actions={previous ? <Button type="button" size="sm" variant="outline" disabled={props.busy !== null}
        onClick={() => void props.run("rollback", "settings.permissionJudge.activateFailed", async () => props.applySnapshot(await rollbackPermissionJudgeModel()))}>
        <RotateCcw size={14} />{formatMessage(t("settings.permissionJudge.rollback"), { name: previous.name })}
      </Button> : undefined}>
      {snapshot.models.length === 0
        ? <PageEmptyState icon={<Box />} title={t("settings.permissionJudge.modelsEmpty")} />
        : snapshot.models.map((model) => {
          const active = model.id === snapshot.activeModelId;
          return <PageItem key={model.id} icon={<Box />} tone={active ? "success" : "neutral"} title={model.name}
            meta={<>
              {active ? <Badge variant="secondary">{t("settings.permissionJudge.modelActive")}</Badge> : null}
              <Badge variant="outline">{model.knownPackageId
                ? formatMessage(t("settings.permissionJudge.modelOfficial"), { name: packageName(model.knownPackageId) })
                : t("settings.permissionJudge.modelCustom")}</Badge>
            </>}
            description={formatMessage(t("settings.permissionJudge.modelMeta"), {
              policy: model.policyVersion, precision: model.precision, size: formatBytes(model.totalBytes),
            })}
            actions={active ? undefined : <>
              <Button type="button" size="sm" variant="outline" disabled={props.busy !== null}
                onClick={() => void props.run(`activate-${model.id}`, "settings.permissionJudge.activateFailed", async () => props.applySnapshot(await activatePermissionJudgeModel(model.id)))}>
                {t("settings.permissionJudge.activate")}
              </Button>
              <Button type="button" size="sm" variant="ghost" disabled={props.busy !== null}
                onClick={() => void props.run(`remove-${model.id}`, "settings.permissionJudge.removeFailed", async () => props.applySnapshot(await removePermissionJudgeModel(model.id)))}>
                {t("settings.permissionJudge.remove")}
              </Button>
            </>}>
            <PageHint className="break-all">{model.path}</PageHint>
          </PageItem>;
        })}
    </PageSection>
    <PageSection title={t("settings.permissionJudge.register")} description={t("settings.permissionJudge.registerHint")}>
      <PermissionJudgeRegisterForm busy={props.busy} knownPackages={snapshot.knownPackages}
        onRegister={(request, reset) => props.run("register", "settings.permissionJudge.registerFailed", async () => {
          props.applySnapshot(await registerPermissionJudgeModel(request.path, request.name, request.custom));
          reset();
        })} />
    </PageSection>
  </>;
}

// CudaSection shows the driver, the detected CUDA folders, one-click installs and official links.
// CudaSection 展示驱动、探测到的 CUDA 文件夹、一键安装与官方链接。
function CudaSection(props: {
  busy: string | null;
  draft: Draft;
  run: RunAction;
  setDraft: (draft: Draft) => void;
  snapshot: PermissionJudgeSettingsSnapshot;
  status: PermissionJudgeStatus;
  t: Translate;
  onStatus: (status: PermissionJudgeStatus) => void;
}) {
  const { t, draft } = props;
  const cuda = props.snapshot.cuda;
  const install = props.status.cudaInstall;
  const open = (url: string) => void openExternalUrl(url).catch((cause) => {
    toast.error(t("settings.permissionJudge.cuda.openFailed"), { description: errorMessage(cause) });
  });
  if (!cuda.supported) {
    return <PageSection title={t("settings.permissionJudge.cuda")} description={t("settings.permissionJudge.cuda.unsupported")} />;
  }
  const driver = cuda.driver
    ? formatMessage(t("settings.permissionJudge.cuda.driverValue"), { version: cuda.driver.version, gpus: cuda.driver.gpus.join(", ") })
    : t("settings.permissionJudge.cuda.driverMissing");
  const facts: PageFact[] = [
    {
      key: "driver",
      label: t("settings.permissionJudge.cuda.driver"),
      value: cuda.minimumDriverMajor != null && cuda.cudaMajor != null
        ? `${driver} · ${formatMessage(t("settings.permissionJudge.cuda.driverMinimum"), { major: cuda.minimumDriverMajor, cuda: cuda.cudaMajor })}`
        : driver,
      tone: cuda.driver ? "neutral" : "attention",
    },
    {
      key: "onnxruntime",
      label: t("settings.permissionJudge.cuda.onnxruntime"),
      value: cuda.onnxruntimeDirectory ?? t("settings.permissionJudge.cuda.notInstalled"),
      tone: cuda.onnxruntimeDirectory ? "neutral" : "attention",
    },
    {
      key: "libraries",
      label: t("settings.permissionJudge.cuda.libraries"),
      value: formatMessage(t("settings.permissionJudge.cuda.required"), { libraries: cuda.requiredLibraries.join(", ") }),
    },
  ];
  return <PageSection title={t("settings.permissionJudge.cuda")} description={t("settings.permissionJudge.cudaHint")}
    footer={cuda.links.length > 0 ? <div className="flex w-full flex-wrap items-center gap-x-1 gap-y-1">
      <PageHint className="mr-2">{t("settings.permissionJudge.cuda.links")}</PageHint>
      {cuda.links.map((link) => (
        <Button key={link.id} type="button" size="sm" variant="ghost" onClick={() => open(link.url)}>
          <ExternalLink size={14} />{label(t, LINK_KEYS, link.id)}
        </Button>
      ))}
    </div> : undefined}>
    <PageFacts items={facts} layout="list" />
    {cuda.detectedDirectories.length > 0 ? <PageItemList heading={t("settings.permissionJudge.cuda.detected")}>
      {cuda.detectedDirectories.map((detected) => (
        <PageItem key={detected.directory} title={label(t, SOURCE_KEYS, detected.source)} description={detected.directory}
          tone={draft.cudaLibraryDirectory === detected.directory ? "success" : "neutral"}
          actions={draft.cudaLibraryDirectory === detected.directory
            ? <Badge variant="secondary"><FolderCheck size={12} />{t("settings.permissionJudge.cuda.locked")}</Badge>
            : <Button type="button" size="sm" variant="outline" disabled={props.busy !== null}
              onClick={() => props.setDraft({ ...draft, cudaLibraryDirectory: detected.directory })}>
              {t("settings.permissionJudge.cuda.useDirectory")}
            </Button>} />
      ))}
    </PageItemList> : null}
    <PageRow stacked label={t("settings.permissionJudge.cuda.directory")}>
      <Input aria-label={t("settings.permissionJudge.cuda.directory")} className="w-full"
        value={draft.cudaLibraryDirectory} onChange={(event) => props.setDraft({ ...draft, cudaLibraryDirectory: event.target.value })} />
    </PageRow>
    {cuda.lockedDirectory && cuda.lockedDirectoryMissing.length > 0 ? <PageNotice tone="warning">
      {formatMessage(t("settings.permissionJudge.cuda.lockedMissing"), { libraries: cuda.lockedDirectoryMissing.join(", ") })}
    </PageNotice> : null}
    {cuda.archives.length > 0 ? <PageItemList heading={t("settings.permissionJudge.cuda.install")}>
      {cuda.archives.map((archive) => (
        <PageItem key={archive.url} title={t(COMPONENT_KEYS[archive.component])} description={`${archive.name} · ${formatBytes(archive.size)}`}
          actions={<Button type="button" size="sm" variant="outline" disabled={props.busy !== null || install?.running === true}
            onClick={() => void props.run(`install-${archive.component}`, "settings.permissionJudge.cuda.installFailed", async () => props.onStatus(await installPermissionJudgeCuda(archive.component)))}>
            {t("settings.permissionJudge.cuda.installAction")}
          </Button>} />
      ))}
    </PageItemList> : null}
    {install?.running ? <PageNotice tone="info">
      {formatMessage(t("settings.permissionJudge.cuda.installing"), {
        component: t(COMPONENT_KEYS[install.component]),
        phase: install.phase ? label(t, INSTALL_PHASE_KEYS, install.phase) : "…",
        progress: install.completedBytes != null && install.totalBytes
          ? `${formatBytes(install.completedBytes)} / ${formatBytes(install.totalBytes)}`
          : "",
      })}
    </PageNotice> : install?.error ? <PageNotice tone="error" details={install.error} detailsLabel={t("settings.errors.showErrorDetails")}>
      {t("settings.permissionJudge.cuda.installFailed")}
    </PageNotice> : install ? <PageNotice tone="success">
      {formatMessage(t("settings.permissionJudge.cuda.installed"), { component: t(COMPONENT_KEYS[install.component]), directory: install.directory ?? "" })}
    </PageNotice> : null}
  </PageSection>;
}
