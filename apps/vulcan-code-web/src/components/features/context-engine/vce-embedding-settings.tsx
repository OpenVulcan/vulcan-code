import * as React from "react";
import { CheckCircle2, CircleAlert, LoaderCircle, Bug, Eraser, Axis3D, ScanSearch } from "lucide-react";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription } from "@/components/ui/dialog";
import { VceActionButton } from "./vce-action-button";
import { Button } from "@/components/ui/button";
import { Combobox } from "@/components/ui/combobox";
import { Input } from "@/components/ui/input";
import { formatMessage, useI18n } from "@/i18n";
import { preflightVceEmbeddingRoute } from "@/service/vce-service";
import { savePublicAiInferenceRoute } from "@/service/settings-actions-service";
import type { PublicAiInferenceRoute, PublicAiInternalInferenceCandidate, PublicAiSettingsSnapshot,
  VceConfigurationCapabilityStatus, VceEmbeddingDecision, VceEmbeddingPreflight } from "@/types";
import { preferredVceDimension, vceDimensionDomain, vceDimensionSupported } from "./vce-embedding-dimensions";
import { vceInferenceRouteKey, vceRoutePickerOptions } from "./vce-route-picker";
import { VceHelpTooltip } from "./vce-help-tooltip";

// Saved host state and shared mutation callbacks for the embedding-only draft editor.
// 向量专属草稿编辑器使用的宿主已保存状态及共享修改回调。
interface VceEmbeddingSettingsProps {
  // Exact current saved route and consumer dimension; absence requires explicit user configuration.
  // 当前精确已保存路由及消费者维度；缺失时要求用户明确配置。
  route?: PublicAiInferenceRoute;
  dimension?: number;
  // Already eligible common model-filter candidates.
  // 已符合资格的通用模型过滤器候选。
  candidates: PublicAiInternalInferenceCandidate[];
  // Host-authoritative readiness for the saved embedding route.
  // 宿主对已保存向量路由的权威就绪状态。
  status: VceConfigurationCapabilityStatus;
  // Shared page mutation state; editing does not register or execute a model.
  // 页面共享修改状态；编辑不注册或执行模型。
  disabled: boolean;
  testing: boolean;
  // Receive committed configuration and then reload the host readiness gate.
  // 接收已提交配置并重新加载宿主就绪闸门。
  onSaved(snapshot: PublicAiSettingsSnapshot): Promise<void>;
  // Serialize explicit preflight/save with other page mutations; receives active/inactive state.
  // 将明确预检、保存与页面其他修改串行化；接收活动或非活动状态。
  onBusyChange(busy: boolean): void;
  // Execute only the saved route's paid connection test on explicit user input.
  // 仅在明确用户输入时执行已保存路由的付费连接测试。
  onTest(): Promise<void>;
}

// Build an unsaved initial dimension suggestion from explicit declarations while preserving saved intent.
// 从明确声明构建未保存的初始维度建议，同时保留已保存意图。
// props provides saved route/capabilities; returns a number string or an explicitly unknown empty value.
// props 提供已保存路由及能力；返回数值字符串或明确未知的空值。
function initialDimensionText(props: VceEmbeddingSettingsProps): string {
  if (props.dimension !== undefined) return String(props.dimension);
  const route = props.route;
  if (!route) return "";
  const capability = props.candidates.find((candidate) =>
    vceInferenceRouteKey(candidate.route) === vceInferenceRouteKey(route))?.capabilities.embedding;
  const domain = capability ? vceDimensionDomain(capability) : null;
  return domain ? String(preferredVceDimension(domain)) : "";
}

// Render a draft-first model/dimension workflow followed by exact per-project confirmation.
// 渲染先编辑模型及维度草稿、后逐项目精确确认的流程。
// props supplies saved authority and mutation callbacks; returns controls that never auto-save or index.
// props 提供已保存权威及修改回调；返回绝不自动保存或建库的控件。
export function VceEmbeddingSettings(props: VceEmbeddingSettingsProps) {
  // Locale changes presentation only; all route identities stay backend-owned.
  // 语言仅改变展示；全部路由身份仍由后端持有。
  const { t } = useI18n();
  // Saved identity excludes unrelated catalog refreshes, preventing accidental draft loss.
  // 已保存身份排除无关目录刷新，防止意外丢失草稿。
  const savedKey = props.route ? vceInferenceRouteKey(props.route) : "";
  const [draftRoute, setDraftRoute] = React.useState<PublicAiInferenceRoute | null>(props.route ?? null);
  const [dimensionText, setDimensionText] = React.useState(() => initialDimensionText(props));
  // Modal contains draft dimensions and project compatibility decisions.
  // 弹窗承载维度草稿及项目兼容性决定。
  const [dialogOpen, setDialogOpen] = React.useState(false);
  const [dialogMode, setDialogMode] = React.useState<"dimensions" | "compatibility">("dimensions");
  const [review, setReview] = React.useState<VceEmbeddingPreflight | null>(null);
  const [decisions, setDecisions] = React.useState<Record<string, VceEmbeddingDecision>>({});
  const [error, setError] = React.useState<string | null>(null);
  const [busy, setBusy] = React.useState<"preflight" | "save" | null>(null);
  // Preserve the confirmed result while save acknowledgements refresh the underlying configuration.
  // 保存确认刷新底层配置期间，保留用户已确认的检测结果。
  const [savingReview, setSavingReview] = React.useState<{
    // Exact review and explicit choices shown throughout the pending save.
    // 整个保存等待期间展示的精确审查及明确选择。
    review: VceEmbeddingPreflight;
    decisions: Record<string, VceEmbeddingDecision>;
  } | null>(null);
  // Presentation is frozen only during saving; validation continues using the current authoritative draft.
  // 仅保存期间冻结展示；校验仍使用当前权威草稿。
  const displayedReview = savingReview?.review ?? review;
  const displayedDecisions = savingReview?.decisions ?? decisions;
  // The sequence rejects stale responses after a draft change, committed replacement, or unmount.
  // 序号拒绝草稿变化、已提交替换或卸载后的过期响应。
  const sequence = React.useRef(0);
  const inFlight = React.useRef(false);
  // Unmount invalidates responses but still lets the owning request release its shared mutation gate.
  // 卸载使响应失效，但仍允许归属请求释放共享修改闸门。
  const mounted = React.useRef(false);
  const savedRouteRef = React.useRef(props.route);
  savedRouteRef.current = props.route;
  // Refreshed capabilities may update the reset suggestion without discarding an existing user draft.
  // 刷新的能力可以更新还原建议，但不丢弃现有用户草稿。
  const initialDimensionRef = React.useRef(initialDimensionText(props));
  initialDimensionRef.current = initialDimensionText(props);

  React.useEffect(() => {
    sequence.current += 1;
    setDraftRoute(savedRouteRef.current ?? null);
    setDimensionText(initialDimensionRef.current);
    setReview(null);
    setDecisions({});
    setError(null);
  }, [savedKey, props.dimension]);
  React.useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; sequence.current += 1; };
  }, []);

  // Candidate lookup uses the same exact common-filter identity, never a decoded display label.
  // 候选查询使用相同的通用过滤器精确身份，绝不解码展示标签。
  const draftKey = draftRoute ? vceInferenceRouteKey(draftRoute) : "";
  const candidateByKey = new Map(props.candidates.map((candidate) => [vceInferenceRouteKey(candidate.route), candidate]));
  const selected = candidateByKey.get(draftKey);
  const domain = selected?.capabilities.embedding ? vceDimensionDomain(selected.capabilities.embedding) : null;
  const dimension = Number(dimensionText);
  const validDraft = !draftRoute || (domain !== null && vceDimensionSupported(domain, dimension));
  const dirty = draftKey !== savedKey || (draftRoute !== null && dimension !== props.dimension);
  const disabled = props.disabled || busy !== null;
  const allDecided = review !== null && review.projects.every((project) =>
    project.compatible || decisions[project.impact.scope.project_id] !== undefined);

  // Invalidate confirmation whenever draft identity changes; no save or test request is made.
  // 草稿身份变化时使确认失效；不发出保存或测试请求。
  const invalidate = () => {
    sequence.current += 1;
    setReview(null);
    setDecisions({});
    setError(null);
  };

  // Update one route draft while retaining an explicit supported user dimension when possible.
  // 更新一项路由草稿，并在支持时保留明确用户维度。
  // next is exact selection/removal; returns without activating a route or creating an index.
  // next 为精确选择或移除；返回时不激活路由、不创建索引。
  const changeRoute = (next: PublicAiInferenceRoute | null) => {
    invalidate();
    setDraftRoute(next);
    setDialogMode("dimensions");
    setDialogOpen(true);
    if (!next) { setDimensionText(""); return; }
    const capability = candidateByKey.get(vceInferenceRouteKey(next))?.capabilities.embedding;
    const nextDomain = capability ? vceDimensionDomain(capability) : null;
    if (nextDomain) setDimensionText(String(preferredVceDimension(nextDomain, dimensionText === "" ? props.dimension : dimension)));
    else setDimensionText("");
  };

  // Inspect only after a deliberate click; provider calls and index writes are forbidden here.
  // 仅在主动点击后检查；此处禁止供应商调用及索引写入。
  // No arguments; stores a current review or explicit error without persisting the draft.
  // 无参数；保存当前审查或明确错误，不持久化草稿。
  const inspect = async () => {
    if (inFlight.current || disabled || !validDraft) return;
    inFlight.current = true;
    const admitted = ++sequence.current;
    setBusy("preflight"); props.onBusyChange(true); setError(null); setReview(null); setDecisions({});
    try {
      const result = await preflightVceEmbeddingRoute(draftRoute, draftRoute ? dimension : undefined);
      if (sequence.current === admitted) setReview(result);
    } catch (cause) {
      if (sequence.current === admitted) setError(String(cause));
    } finally {
      inFlight.current = false;
      if (mounted.current) setBusy(null);
      props.onBusyChange(false);
    }
  };

  // Save exact reviewed route, dimensions, and user choices in the existing public inference transaction.
  // 在现有公共推理事务中保存精确已审查路由、维度及用户选择。
  // No arguments; returns after committed snapshot/readiness publication or shows a non-silent failure.
  // 无参数；已提交快照及就绪发布后返回，失败时明确展示错误。
  const save = async () => {
    if (inFlight.current || disabled || !validDraft || !review || !allDecided) return;
    inFlight.current = true;
    const admitted = ++sequence.current;
    setSavingReview({ review, decisions });
    setBusy("save"); props.onBusyChange(true); setError(null);
    try {
      const snapshot = await savePublicAiInferenceRoute({ consumerProfileId: "vce", operation: "embedding_create",
        route: review.candidate?.route ?? null, embeddingDimensions: review.candidate?.dimension,
        vceEmbeddingConfirmation: { preflightRevision: review.revision,
          decisions: Object.entries(decisions).map(([projectId, decision]) => ({ projectId, decision })) } });
      if (sequence.current === admitted) {
        await props.onSaved(snapshot);
        if (mounted.current) setDialogOpen(false);
      }
    } catch (cause) {
      if (sequence.current === admitted) { setError(String(cause)); setReview(null); setDecisions({}); }
    } finally {
      inFlight.current = false;
      if (mounted.current) { setBusy(null); setSavingReview(null); }
      props.onBusyChange(false);
    }
  };

  // Discard unsaved edits when closing the modal; no configuration or index writes occur.
  // 关闭弹窗时丢弃未保存编辑；不写入配置或索引。
  const resetDraft = () => {
    invalidate(); setDraftRoute(props.route ?? null); setDimensionText(initialDimensionText(props));
  };

  return <section className="space-y-4 px-4 py-3" aria-label={t("contextEngine.embedding.embeddingModelAndDimensions")}>
    {/* Align purpose, model and icon actions with the other inference routes. */}
    {/* 将用途、模型和图标操作对齐到其他推理路由。 */}
    <div className="grid items-center gap-3 @xl/vce:grid-cols-[8rem_minmax(12rem,1fr)_9.5rem]">
      <div className="flex min-w-0 items-center gap-2">
      <span className={props.status.available
        ? "flex size-7 shrink-0 items-center justify-center rounded-full bg-emerald-500/10 text-[var(--vcn-content-success)]"
        : "flex size-7 shrink-0 items-center justify-center rounded-full bg-amber-500/10 text-[var(--vcn-content-warning)]"}>
        {props.status.available ? <CheckCircle2 className="size-4" /> : <CircleAlert className="size-4" />}
      </span>
      <div className="min-w-0 flex-1">
        <h4 className="flex items-center gap-1 text-xs font-medium">{t("contextEngine.embedding.embeddingModel")}<VceHelpTooltip label={t("contextEngine.embedding.embeddingModelNotes")}>{t("contextEngine.embedding.sourceCodeAndQueries")}</VceHelpTooltip></h4>
      </div>
      </div>
      <div className="min-w-0 space-y-1.5">
        <span className="text-xs text-muted-foreground @xl/vce:hidden">{t("contextEngine.embedding.modelAndAccount")}</span>
        <Combobox ariaLabel={t("contextEngine.embedding.embeddingModel")} className="h-8 px-2 text-xs" disabled={disabled || props.candidates.length === 0}
          options={vceRoutePickerOptions(props.candidates)} value={selected ? draftKey : undefined}
          placeholder={draftRoute?.model_id ?? t("contextEngine.embedding.selectModelAndAccount")}
          searchPlaceholder={t("contextEngine.embedding.filterModelsProvidersOr")}
          emptyText={t("contextEngine.embedding.noCompatibleModels")}
          onValueChange={(value) => { const candidate = candidateByKey.get(value); if (candidate) changeRoute(candidate.route); }} />
      </div>
      <div className="flex flex-wrap items-center gap-2 @xl/vce:justify-end">
      <VceActionButton label={t("contextEngine.embedding.vectorDimensions")}
        tooltip={`${t("contextEngine.embedding.vectorDimensions")}: ${props.dimension ?? t("contextEngine.embedding.notConfigured")}`}
        disabled={disabled || !draftRoute} icon={<Axis3D className="size-4" />}
        onClick={() => { setDialogMode("dimensions"); setDialogOpen(true); }} />
      <VceActionButton label={t("contextEngine.embedding.testConnectivity")}
        disabled={disabled || dirty || !props.route || props.dimension === undefined}
        onClick={() => void props.onTest()} icon={props.testing ? <LoaderCircle className="size-4 animate-spin" /> : <Bug className="size-4" />} />
      <VceActionButton label={t("contextEngine.embedding.checkProjectCompatibility")}
        disabled={disabled || !validDraft || (!draftRoute && !props.route)} icon={<ScanSearch className="size-4" />}
        onClick={() => { setDialogMode("compatibility"); setDialogOpen(true); void inspect(); }} />
      <VceActionButton label={t("contextEngine.embedding.clear")} disabled={disabled || !draftRoute}
        onClick={() => changeRoute(null)} icon={<Eraser className="size-4" />} />
      </div>
    </div>
    {props.status.issue ? <p className="break-words text-xs text-[var(--vcn-content-warning)]">{props.status.issue}</p> : null}
    <Dialog open={dialogOpen} onOpenChange={(open) => { if (!disabled) { if (!open) resetDraft(); setDialogOpen(open); } }}>
      <DialogContent size="md" scrollMode="body-scroll" showCloseButton={!disabled}>
        <DialogHeader>
          <DialogTitle>{dialogMode === "compatibility" ? t("contextEngine.embedding.projectCompatibility") : draftRoute ? t("contextEngine.embedding.vectorDimensions2") : t("contextEngine.embedding.clearEmbeddingModel")}</DialogTitle>
          <DialogDescription>{t("contextEngine.embedding.reviewExistingProjectCompatibility")}</DialogDescription>
        </DialogHeader>
        {dialogMode === "dimensions" ? <>
      <div className="min-w-0 space-y-1.5">{draftRoute ? <>
        <label className="flex items-center gap-1 text-xs text-muted-foreground" htmlFor="vce-embedding-dimension">{t("contextEngine.embedding.dimensions")}<VceHelpTooltip label={t("contextEngine.embedding.dimensionNotes")}>{t("contextEngine.embedding.dimensionsMustBeExplicit")}</VceHelpTooltip></label>
        {domain?.kind === "discrete" ? <Combobox ariaLabel={t("contextEngine.embedding.dimensions")} className="h-10" disabled={disabled}
          options={domain.values.map((value) => ({ value: String(value), label: String(value) }))}
          value={domain.values.includes(dimension) ? dimensionText : undefined} placeholder={t("contextEngine.embedding.chooseDimensions")}
          searchPlaceholder={t("contextEngine.embedding.filterDimensions")} emptyText={t("contextEngine.embedding.noMatchingDimensions")}
          onValueChange={(value) => { invalidate(); setDimensionText(value); }} />
          : <Input id="vce-embedding-dimension" type="number" className="h-10" value={dimensionText} disabled={disabled || !domain}
            min={domain?.kind === "range" ? domain.minimum : undefined} max={domain?.kind === "range" ? domain.maximum : undefined} step={1}
            onChange={(event) => { invalidate(); setDimensionText(event.target.value); }} />}
      </> : null}</div>
        </> : null}
        {!validDraft && draftRoute ? <p className="text-xs text-[var(--vcn-content-warning)]">{t("contextEngine.embedding.chooseAPositiveDimension")}</p> : null}
        {error ? <p role="alert" className="break-words text-xs text-destructive">{error}</p> : null}
        {busy === "preflight" ? <p role="status" className="flex items-center gap-2 text-sm"><LoaderCircle className="size-4 animate-spin" />{t("contextEngine.embedding.checkingProjectCompatibility")}</p> : null}
        {busy === "save" ? <p role="status" className="flex items-center gap-2 text-sm"><LoaderCircle className="size-4 animate-spin" />{t("contextEngine.embedding.savingAndUpdatingStatus")}</p> : null}
    {displayedReview ? <div className="space-y-3 rounded-lg border bg-muted/20 p-3" aria-label={t("contextEngine.embedding.confirmModelChanges")}>
      <p className="text-sm font-medium">{displayedReview.candidate ? formatMessage(t("contextEngine.embedding.confirmDimensionVectors"), { dimension: displayedReview.candidate.dimension }) : t("contextEngine.embedding.confirmEmbeddingRemoval")}</p>
      {displayedReview.projects.filter((project) => !project.compatible).map((project) => <fieldset key={project.impact.scope.project_id} className="space-y-2 rounded-md border bg-background p-3" disabled={disabled}>
        <legend className="px-1 text-sm">{project.projectName}</legend>
        <p className="text-xs text-muted-foreground">{project.impact.hardRebuildReason
          ? formatMessage(t("contextEngine.embedding.existingIndexDimensionsRebuild"), { dimension: project.impact.scope.dimension })
          : formatMessage(t("contextEngine.embedding.exactOriginsMatchingNames"), { length: project.impact.sourceFingerprints.length })}</p>
        {!project.impact.hardRebuildReason ? <label className="flex items-start gap-2 text-xs"><input type="radio" name={`vce-compat-${project.impact.scope.project_id}`} checked={displayedDecisions[project.impact.scope.project_id] === "compatible"}
          onChange={() => setDecisions((current) => ({ ...current, [project.impact.scope.project_id]: "compatible" }))} />{t("contextEngine.embedding.sameVectorSpaceKeep")}</label> : null}
        <label className="flex items-start gap-2 text-xs"><input type="radio" name={`vce-compat-${project.impact.scope.project_id}`} checked={displayedDecisions[project.impact.scope.project_id] === "rebuild"}
          onChange={() => setDecisions((current) => ({ ...current, [project.impact.scope.project_id]: "rebuild" }))} />{t("contextEngine.embedding.rebuildRequiredRetainThe")}</label>
        <details className="text-xs text-muted-foreground"><summary>{t("contextEngine.embedding.inspectExactOrigins")}</summary>
          <p className="mt-1 break-all">{project.impact.scope.project_id} · {project.impact.scope.root_hash}</p>
          {project.impact.sourceFingerprints.map((source) => <p className="mt-1 break-all" key={source}>{source}</p>)}
          {project.impact.hardRebuildReason ? <p className="mt-1">{project.impact.hardRebuildReason}</p> : null}
        </details>
      </fieldset>)}
      {displayedReview.projects.every((project) => project.compatible) ? <p className="text-xs text-muted-foreground">{t("contextEngine.embedding.noInitializedIndexRequires")}</p> : null}
      {!displayedReview.candidate ? <p className="text-xs text-muted-foreground">{t("contextEngine.embedding.theEngineBecomesUnavailable")}</p> : null}
    </div> : null}
        <div className="flex justify-end gap-2">
          <Button size="sm" variant="ghost" disabled={disabled} onClick={() => { resetDraft(); setDialogOpen(false); }}>{t("contextEngine.embedding.cancel")}</Button>
          <Button size="sm" disabled={disabled || !validDraft || (review !== null && !allDecided)} onClick={() => void (review ? save() : inspect())}>
            {busy === "save" ? <LoaderCircle className="size-4 animate-spin" /> : null}
            {busy === "save" ? t("contextEngine.embedding.saving") : review ? t("contextEngine.embedding.confirmAndSave") : dialogMode === "compatibility" ? t("contextEngine.embedding.checkAgain") : t("contextEngine.embedding.save")}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  </section>;
}
