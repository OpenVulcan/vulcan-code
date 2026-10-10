import * as React from "react";
import { Activity, Box, ChevronLeft, History, ChevronRight, Gauge, RefreshCw, RotateCcw } from "lucide-react";
import { toast } from "sonner";
import {
  PageBody, PageEmptyState, PageFacts, PageHint, PageItem, PageNotice, PageRow, PageSection, PageToolbar, type PageFact,
} from "@/components/page";
import { StatusPill } from "@/components/shared/status";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { SegmentedControl, SegmentedControlItem } from "@/components/ui/choice-controls";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { StreamPlaceholder } from "@/components/ui/stream-placeholder";
import { Switch } from "@/components/ui/switch";
import { formatMessage, useI18n, type Translate, type TranslationKey } from "@/i18n";
import { errorMessage, formatBytes } from "@/lib/display-format";
import { PermissionJudgeCloudSection } from "@/components/features/settings/permission-judge-cloud-section";
import {
  PermissionJudgeCudaPage, cudaMissingLabels, cudaReadiness,
} from "@/components/features/settings/permission-judge-cuda-page";
import { DEVICE_KEYS, MODE_KEYS, OPTION_HINT_KEYS, OPTION_KEYS, PHASE_KEYS, REASON_KEYS, label } from "@/components/features/settings/permission-judge-labels";
import { PermissionJudgePolicyPage } from "@/components/features/settings/permission-judge-policy-page";
import { PermissionJudgeRecordsPage } from "@/components/features/settings/permission-judge-records-page";
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
import type {
  JudgeBackend,
  JudgeDevice,
  JudgeState,
  PermissionJudgeBenchmarkResult,
  PermissionJudgeSettingsSnapshot,
  PermissionJudgeStatus,
  ProjectSummary,
} from "@/types";

// Labels of stable codes sent by the service; unknown codes render as themselves.
// 服务发送的稳定代码对应的标签；未知代码按原样显示。
const BACKEND_KEYS: Record<JudgeBackend, TranslationKey> = {
  local: "settings.permissionJudge.backend.local",
  cloud: "settings.permissionJudge.backend.cloud",
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

// Choices are the system-level judge settings besides the on/off state.
// Choices 是开关状态之外的系统级判定设置。
interface Choices {
  backend: JudgeBackend;
  device: JudgeDevice | null;
  cudaLibraryDirectory: string | null;
  llmMayAllowBlocked: boolean;
}

// savedChoices reads the saved choices; an unset backend is local and an unset device follows the
// recommendation.
// savedChoices 读取已保存的选择；未设置的后端为本机，未设置的设备跟随推荐值。
function savedChoices(snapshot: PermissionJudgeSettingsSnapshot): Choices {
  return {
    backend: snapshot.backend ?? "local",
    device: snapshot.device ?? snapshot.recommendedDevice,
    cudaLibraryDirectory: snapshot.cuda.lockedDirectory,
    llmMayAllowBlocked: snapshot.llmMayAllowBlocked,
  };
}

// View is the page shown inside the settings frame: the overview or one of its sub-pages.
// View 是设置框架内显示的页面：总览或其子页面之一。
type View = "overview" | "models" | "cuda" | "diagnostics" | "records" | "policy";

// Failure is one failed action: a readable outcome plus the raw service error.
// Failure 是一次失败的操作：可读结果加原始服务错误。
interface Failure {
  message: string;
  detail: string;
}

// RunAction executes one named page action and reports its failure.
// RunAction 执行一个具名页面操作并报告其失败。
type RunAction = (name: string, failureKey: TranslationKey, action: () => Promise<void>) => Promise<void>;

// PermissionJudgeSettingsArea manages the permission judge. The overview reads top-down: turn the
// judge on, choose where calls are judged and set that judge up, then the LLM review and tools.
// Detailed work opens as sub-pages: model management, the CUDA environment and diagnostics.
// Every choice saves at once; while the judge was never set up, choices wait until it is turned on,
// so nothing is written before that explicit choice.
// PermissionJudgeSettingsArea 管理权限判定。总览自上而下：开启判定、选择在哪里判定并完成该判定器的设置，
// 然后是 LLM 复核与工具。细节操作以子页面打开：模型管理、CUDA 环境与诊断。每个选择立即保存；判定从未设置时，
// 选择会等到开启时才一并保存，因此在该显式选择之前不写入任何内容。
// Parameters: none. Returns the settings page.
// 参数：无。返回设置页。
export function PermissionJudgeSettingsArea(props: { projects?: readonly ProjectSummary[] }) {
  const { locale, t } = useI18n();
  // openGuide re-enters the setup guide; absent outside the workbench layout.
  // openGuide 重新进入初始化引导；在工作台布局之外为空。
  const openGuide = React.useContext(PermissionJudgeEntryContext)?.openGuide;
  // snapshot is the last full read; status is refreshed separately by polling.
  // snapshot 是最近一次完整读取；status 由轮询单独刷新。
  const [snapshot, setSnapshot] = React.useState<PermissionJudgeSettingsSnapshot | null>(null);
  const [status, setStatus] = React.useState<PermissionJudgeStatus | null>(null);
  // held keeps choices that cannot be saved yet: everything before the judge is first turned on,
  // and a backend switch whose backend is not set up.
  // held 保存暂时无法保存的选择：判定首次开启前的全部选择，以及尚未设置完成的后端切换。
  const [held, setHeld] = React.useState<Partial<Choices>>({});
  const [view, setView] = React.useState<View>("overview");
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
  // topRef brings a newly opened page or a new failure into view.
  // topRef 将新打开的页面或新失败滚动到可见处。
  const topRef = React.useRef<HTMLDivElement>(null);
  React.useEffect(() => {
    topRef.current?.scrollIntoView?.({ block: "nearest" });
  }, [view, failure]);

  // applySnapshot replaces the page state with a fresh service read.
  // applySnapshot 用最新服务读取结果替换页面状态。
  const applySnapshot = React.useCallback((next: PermissionJudgeSettingsSnapshot) => {
    setSnapshot(next);
    setStatus(next.status);
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
  const run: RunAction = async (name, failureKey, action) => {
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

  const failureNotice = failure ? <PageNotice tone="error" details={failure.detail} detailsLabel={t("settings.errors.showErrorDetails")}>
    {failure.message}
  </PageNotice> : null;

  if (!snapshot || !status) {
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

  const saved = savedChoices(snapshot);
  const choices: Choices = { ...saved, ...held };
  const activeModel = snapshot.models.find((model) => model.id === snapshot.activeModelId) ?? null;
  const local = choices.backend === "local";
  const ready = (backend: JudgeBackend) => backend === "local" ? activeModel !== null : cloudConfigured;
  const enabled = snapshot.state === "enabled";

  // persist saves `change` at once in `state`, or holds it while the judge was never set up or while
  // the enabled judge would switch to a backend that is not set up.
  // persist 立即以 `state` 保存 `change`；判定从未设置，或已启用的判定将切换到尚未设置的后端时暂存。
  const persist = (change: Partial<Choices>, state: JudgeState | null = snapshot.state) => {
    const next = { ...choices, ...change };
    if (state === snapshot.state && JSON.stringify(next) === JSON.stringify(saved)) {
      setHeld({});
      return;
    }
    if (state === null || (state === "enabled" && !ready(next.backend))) {
      setHeld({ ...held, ...change });
      return;
    }
    void run("save", "settings.permissionJudge.saveFailed", async () => {
      applySnapshot(await savePermissionJudgeSettings({ state, ...next }));
      setHeld({});
      toast.success(t("settings.permissionJudge.saved"));
    });
  };

  const back = () => setView("overview");
  const subPage = (title: string, content: React.ReactNode) => <PageBody data-permission-judge-page="" data-judge-view={view}>
    <div ref={topRef}>
      <PageToolbar>
        <Button type="button" size="sm" variant="ghost" data-judge-back="" onClick={back}>
          <ChevronLeft size={14} />{t("settings.permissionJudge")}
        </Button>
        <span aria-hidden="true" className="text-muted-foreground">/</span>
        <h2 className="text-sm font-semibold">{title}</h2>
      </PageToolbar>
    </div>
    {failureNotice}
    {content}
  </PageBody>;

  if (view === "models") {
    return subPage(t("settings.permissionJudge.models"),
      <ModelsPage applySnapshot={applySnapshot} busy={busy} run={run} snapshot={snapshot} t={t} />);
  }
  if (view === "cuda") {
    return subPage(t("settings.permissionJudge.cuda"),
      <PermissionJudgeCudaPage busy={busy} cuda={snapshot.cuda} install={status.cudaInstall}
        selectedDirectory={choices.cudaLibraryDirectory} t={t}
        onInstall={(component) => void run(`install-${component}`, "settings.permissionJudge.cuda.installFailed",
          async () => setStatus(await installPermissionJudgeCuda(component)))}
        onSelectDirectory={(cudaLibraryDirectory) => persist({ cudaLibraryDirectory })} />);
  }
  if (view === "policy") {
    return subPage(t("settings.permissionJudge.policy.title"), <PermissionJudgePolicyPage projects={props.projects ?? []} />);
  }
  if (view === "records") {
    return subPage(t("settings.permissionJudge.records.title"), <PermissionJudgeRecordsPage />);
  }
  if (view === "diagnostics") {
    const savedCloud = enabled && snapshot.backend === "cloud";
    const benchmarkReady = savedCloud || (snapshot.backend !== "cloud" && status.worker.phase === "ready");
    return subPage(t("settings.permissionJudge.diagnostics"), <>
      {snapshot.backend !== "cloud" ? <WorkerStatusSection snapshot={snapshot} status={status} t={t} /> : null}
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
        {benchmarks.map((result, index) => (
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
        ))}
      </PageSection>
    </>);
  }

  // The toolbar summarizes what is saved, not held choices.
  // 工具栏概括已保存的内容，而非暂存的选择。
  const stateLabel = enabled
    ? `${t("settings.permissionJudge.state.enabled")} · ${t(BACKEND_KEYS[snapshot.backend ?? "local"])}`
    : t(snapshot.state === "off" ? "settings.permissionJudge.state.off" : "settings.permissionJudge.state.unset");
  const stateTone = enabled ? "ready" : snapshot.state === "off" ? "pending" : "warning";
  const switching = enabled && held.backend !== undefined && held.backend !== snapshot.backend;
  const worker = status.worker;
  const phaseTone = worker.phase === "ready" ? "ready" : worker.phase === "failed" ? "error" : worker.phase === "warming" ? "warning" : "pending";
  const cudaMissing = snapshot.cuda.supported ? cudaMissingLabels(t, cudaReadiness(snapshot.cuda)) : [];
  const open = (next: View, text = t("settings.permissionJudge.open")) => <Button type="button" size="sm" variant="outline" data-judge-open={next} onClick={() => setView(next)}>
    {text}<ChevronRight size={14} />
  </Button>;

  return <PageBody data-permission-judge-page="" data-judge-view="overview">
    <div ref={topRef}>
      <PageToolbar actions={openGuide
        ? <Button type="button" size="sm" variant="outline" onClick={openGuide}>{t("setupGuide.open")}</Button>
        : null}>
        <StatusPill label={stateLabel} status={stateTone} />
      </PageToolbar>
    </div>
    {failureNotice}

    <PageSection layout="rows" title={t("settings.permissionJudge")} description={t("settings.permissionJudge.description")}>
      <PageRow label={t("settings.permissionJudge.enabled")}
        description={!enabled && !ready(choices.backend)
          ? t(local ? "settings.permissionJudge.needsModel" : "settings.permissionJudge.needsCloudRoute")
          : t("settings.permissionJudge.enabledHint")}>
        <Switch aria-label={t("settings.permissionJudge.enabled")} checked={enabled}
          disabled={busy !== null || (!enabled && !ready(choices.backend))}
          onCheckedChange={(next) => persist({}, next ? "enabled" : "off")} />
      </PageRow>
      <PageRow label={t("settings.permissionJudge.backend")} description={t("settings.permissionJudge.backendHint")}>
        <SegmentedControl aria-label={t("settings.permissionJudge.backend")} disabled={busy !== null} value={choices.backend}
          onValueChange={(backend) => persist({ backend: backend as JudgeBackend })}>
          {(Object.keys(BACKEND_KEYS) as JudgeBackend[]).map((backend) => (
            <SegmentedControlItem key={backend} value={backend}>{t(BACKEND_KEYS[backend])}</SegmentedControlItem>
          ))}
        </SegmentedControl>
      </PageRow>
      {switching ? <PageNotice tone="info" data-judge-switching=""
        actions={<Button type="button" size="sm" disabled={busy !== null || !ready(choices.backend)} onClick={() => persist({})}>
          {formatMessage(t("settings.permissionJudge.switchBackend"), { backend: t(BACKEND_KEYS[choices.backend]) })}
        </Button>}>
        {formatMessage(t("settings.permissionJudge.switchBackendPending"), { backend: t(BACKEND_KEYS[choices.backend]) })}
      </PageNotice> : null}
      {snapshot.state === null && Object.keys(held).length > 0
        ? <PageNotice tone="info" data-judge-held="">{t("settings.permissionJudge.heldUntilEnabled")}</PageNotice>
        : null}
    </PageSection>

    {local ? <PageSection layout="rows" data-judge-local="" title={t("settings.permissionJudge.backend.local")}>
      <PageRow label={t("settings.permissionJudge.activeModel")} description={activeModel
        ? `${activeModel.name} · ${formatMessage(t("settings.permissionJudge.modelMeta"), {
          policy: activeModel.policyVersion, precision: activeModel.precision, size: formatBytes(activeModel.totalBytes),
        })}`
        : t("settings.permissionJudge.noActiveModel")}>
        <Button type="button" size="sm" variant={activeModel ? "outline" : "default"} data-judge-open="models" onClick={() => setView("models")}>
          {t(activeModel ? "settings.permissionJudge.manageModels" : "settings.permissionJudge.addModel")}<ChevronRight size={14} />
        </Button>
      </PageRow>
      <PageRow label={t("settings.permissionJudge.device")} description={snapshot.recommendedDevice
        ? formatMessage(t("settings.permissionJudge.recommended"), { device: t(DEVICE_KEYS[snapshot.recommendedDevice]) })
        : undefined}>
        <Select disabled={busy !== null} value={choices.device ?? undefined}
          onValueChange={(device) => persist({ device: device as JudgeDevice })}>
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
      </PageRow>
      {snapshot.cuda.supported ? <PageRow label={t("settings.permissionJudge.cuda")} description={cudaMissing.length === 0
        ? t("settings.permissionJudge.cuda.summaryReady")
        : formatMessage(t("settings.permissionJudge.cuda.summaryMissing"), { items: new Intl.ListFormat(locale, { type: "conjunction" }).format(cudaMissing) })}>
        <div className="flex items-center gap-2">
          <StatusPill status={cudaMissing.length === 0 ? "ready" : "warning"}
            label={t(cudaMissing.length === 0 ? "settings.permissionJudge.cuda.ready" : "settings.permissionJudge.cuda.missing")} />
          <Button type="button" size="sm" variant="outline" data-judge-open="cuda" onClick={() => setView("cuda")}>
            {t("settings.permissionJudge.cuda.manage")}<ChevronRight size={14} />
          </Button>
        </div>
      </PageRow> : null}
      {activeModel ? <PageRow label={t("settings.permissionJudge.runState")} description={worker.phase === "ready"
        ? formatMessage(t("settings.permissionJudge.status.latencyValue"), {
          p50: formatMs(t, worker.recentP50Ms), p95: formatMs(t, worker.recentP95Ms), count: worker.evaluations,
        })
        : undefined}>
        <div className="flex items-center gap-2">
          <StatusPill label={label(t, PHASE_KEYS, worker.phase)} status={phaseTone} />
          {worker.phase === "failed" ? <Button type="button" size="sm" variant="outline" disabled={busy !== null}
            onClick={() => void run("reload", "settings.permissionJudge.reloadFailed", async () => setStatus(await reloadPermissionJudge()))}>
            <RefreshCw size={14} />{t("settings.permissionJudge.reload")}
          </Button> : null}
        </div>
      </PageRow> : null}
      {worker.phase === "failed" && worker.error ? <PageNotice tone="error" details={worker.error} detailsLabel={t("settings.errors.showErrorDetails")}>
        {t("settings.permissionJudge.status.loadFailed")}
      </PageNotice> : null}
      {snapshot.activeModelIssue ? <PageNotice tone="error" details={snapshot.activeModelIssue.detail} detailsLabel={t("settings.errors.showErrorDetails")}>
        {formatMessage(t("settings.permissionJudge.activeModelIssue"), { code: snapshot.activeModelIssue.code })}
      </PageNotice> : null}
    </PageSection> : <PermissionJudgeCloudSection onConfiguredChange={setCloudConfigured} />}

    {enabled && snapshot.policyOptions.length > 0 ? <PageSection layout="rows" data-judge-policy-summary=""
      title={t("settings.permissionJudge.policy.title")} description={t("settings.permissionJudge.policy.description")}
      actions={open("policy", t("settings.permissionJudge.policy.manage"))}>
      {snapshot.policyOptions.map((option) => (
        <PageRow key={option.key} label={label(t, OPTION_KEYS, option.key)}
          description={Object.hasOwn(OPTION_HINT_KEYS, option.key) ? t(OPTION_HINT_KEYS[option.key]) : undefined}>
          <span className="text-sm text-muted-foreground">
            {formatMessage(t("settings.permissionJudge.policy.default"), { mode: label(t, MODE_KEYS, option.defaultMode) })}
          </span>
        </PageRow>
      ))}
    </PageSection> : null}

    <PermissionReviewRouteSection>
      <PageRow label={t("settings.permissionJudge.llmMayAllowBlocked")} description={t("settings.permissionJudge.llmMayAllowBlockedHint")}>
        <Switch aria-label={t("settings.permissionJudge.llmMayAllowBlocked")} checked={choices.llmMayAllowBlocked}
          disabled={busy !== null} onCheckedChange={(llmMayAllowBlocked) => persist({ llmMayAllowBlocked })} />
      </PageRow>
    </PermissionReviewRouteSection>

    <PageSection layout="rows" data-judge-tools="" title={t("settings.permissionJudge.tools")}>
      <PageItem icon={<Activity />} title={t("settings.permissionJudge.diagnostics")}
        description={t("settings.permissionJudge.diagnosticsHint")} actions={open("diagnostics")} />
      <PageItem icon={<History />} title={t("settings.permissionJudge.records.title")}
        description={t("settings.permissionJudge.records.entryHint")} actions={open("records")} />
    </PageSection>
  </PageBody>;
}

// WorkerStatusSection shows the loaded model's phase, memory, GPU use and latency.
// WorkerStatusSection 展示已加载模型的阶段、内存、GPU 占用与延迟。
function WorkerStatusSection(props: {
  snapshot: PermissionJudgeSettingsSnapshot;
  status: PermissionJudgeStatus;
  t: Translate;
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
    status={<StatusPill label={label(t, PHASE_KEYS, worker.phase)} status={pill} />}>
    <PageFacts items={facts} layout="grid" />
  </PageSection>;
}

// ModelsPage lists registered models with their switch, rollback and removal, then adds new ones
// from local folders.
// ModelsPage 列出已登记模型及其切换、回退与移除操作，然后从本机文件夹添加新模型。
function ModelsPage(props: {
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
