import * as React from "react";
import { CheckCircle2, CircleAlert, LoaderCircle, Play, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Combobox } from "@/components/ui/combobox";
import { Input } from "@/components/ui/input";
import { useI18n } from "@/i18n";
import { preflightVceEmbeddingRoute } from "@/service/vce-service";
import { savePublicAiInferenceRoute } from "@/service/settings-actions-service";
import type { PublicAiInferenceRoute, PublicAiInternalInferenceCandidate, PublicAiSettingsSnapshot,
  VceConfigurationCapabilityStatus, VceEmbeddingDecision, VceEmbeddingPreflight } from "@/types";
import { preferredVceDimension, vceDimensionDomain, vceDimensionSupported } from "./vce-embedding-dimensions";
import { vceInferenceRouteKey, vceRoutePickerOptions } from "./vce-route-picker";

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
  // Host readiness and last explicit saved-model test result.
  // 宿主就绪及最近一次明确已保存模型测试结果。
  status: VceConfigurationCapabilityStatus;
  testSummary?: string;
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
  const { locale } = useI18n();
  const copy = (zh: string, en: string) => locale === "zh-CN" ? zh : en;
  // Saved identity excludes unrelated catalog refreshes, preventing accidental draft loss.
  // 已保存身份排除无关目录刷新，防止意外丢失草稿。
  const savedKey = props.route ? vceInferenceRouteKey(props.route) : "";
  const [draftRoute, setDraftRoute] = React.useState<PublicAiInferenceRoute | null>(props.route ?? null);
  const [dimensionText, setDimensionText] = React.useState(() => initialDimensionText(props));
  const [review, setReview] = React.useState<VceEmbeddingPreflight | null>(null);
  const [decisions, setDecisions] = React.useState<Record<string, VceEmbeddingDecision>>({});
  const [error, setError] = React.useState<string | null>(null);
  const [busy, setBusy] = React.useState<"preflight" | "save" | null>(null);
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
    setBusy("save"); props.onBusyChange(true); setError(null);
    try {
      const snapshot = await savePublicAiInferenceRoute({ consumerProfileId: "vce", operation: "embedding_create",
        route: review.candidate?.route ?? null, embeddingDimensions: review.candidate?.dimension,
        vceEmbeddingConfirmation: { preflightRevision: review.revision,
          decisions: Object.entries(decisions).map(([projectId, decision]) => ({ projectId, decision })) } });
      if (sequence.current === admitted) {
        setReview(null); setDecisions({});
        await props.onSaved(snapshot);
      }
    } catch (cause) {
      if (sequence.current === admitted) { setError(String(cause)); setReview(null); setDecisions({}); }
    } finally {
      inFlight.current = false;
      if (mounted.current) setBusy(null);
      props.onBusyChange(false);
    }
  };

  return <section className="space-y-4 px-4 py-4" aria-label={copy("向量模型与维度", "Embedding model and dimensions")}>
    {/* Let the test action occupy a separate row in narrow settings panes, keeping the model explanation readable. */}
    {/* 在窄设置面板中让测试操作单独占行，保留模型说明的可读宽度。 */}
    <div className="grid grid-cols-[auto_minmax(0,1fr)] items-start gap-3 @xl/vce:grid-cols-[auto_minmax(0,1fr)_auto]">
      {props.status.available ? <CheckCircle2 className="mt-0.5 size-5 text-emerald-600" /> : <CircleAlert className="mt-0.5 size-5 text-amber-600" />}
      <div className="min-w-0 flex-1">
        <h4 className="text-sm font-medium">{copy("Embedding 向量模型", "Embedding model")}</h4>
        <p className="mt-1 text-xs text-muted-foreground">{copy("源码与查询使用同一向量空间。先选择模型和明确维度，再确认已有项目的兼容性。", "Source code and queries share one vector space. Select a model and explicit dimension, then review existing projects.")}</p>
      </div>
      <Button className="col-start-2 justify-self-start @xl/vce:col-start-auto" size="sm" variant="outline" disabled={disabled || dirty || !props.route || props.dimension === undefined} onClick={() => void props.onTest()}>
        {props.testing ? <LoaderCircle className="size-3.5 animate-spin" /> : <Play className="size-3.5" />}{copy("测试已保存模型", "Test saved model")}
      </Button>
    </div>
    <div className="flex flex-wrap items-end gap-3">
      <div className="min-w-56 flex-1 space-y-1.5">
        <span className="text-xs text-muted-foreground">{copy("模型与账号", "Model and account")}</span>
        <Combobox ariaLabel={copy("Embedding 向量模型", "Embedding model")} className="h-10" disabled={disabled || props.candidates.length === 0}
          options={vceRoutePickerOptions(props.candidates)} value={selected ? draftKey : undefined}
          placeholder={draftRoute?.model_id ?? copy("选择模型与账号", "Select model and account")}
          searchPlaceholder={copy("筛选模型、供应商或账号…", "Filter models, providers, or accounts…")}
          emptyText={copy("没有符合条件的模型", "No compatible models")}
          onValueChange={(value) => { const candidate = candidateByKey.get(value); if (candidate) changeRoute(candidate.route); }} />
      </div>
      {draftRoute ? <div className="w-40 space-y-1.5">
        <label className="text-xs text-muted-foreground" htmlFor="vce-embedding-dimension">{copy("向量维度", "Dimensions")}</label>
        {domain?.kind === "discrete" ? <Combobox ariaLabel={copy("向量维度", "Dimensions")} className="h-10" disabled={disabled}
          options={domain.values.map((value) => ({ value: String(value), label: String(value) }))}
          value={domain.values.includes(dimension) ? dimensionText : undefined} placeholder={copy("选择明确维度", "Choose dimensions")}
          searchPlaceholder={copy("筛选维度…", "Filter dimensions…")} emptyText={copy("没有匹配维度", "No matching dimensions")}
          onValueChange={(value) => { invalidate(); setDimensionText(value); }} />
          : <Input id="vce-embedding-dimension" type="number" className="h-10" value={dimensionText} disabled={disabled || !domain}
            min={domain?.kind === "range" ? domain.minimum : undefined} max={domain?.kind === "range" ? domain.maximum : undefined} step={1}
            onChange={(event) => { invalidate(); setDimensionText(event.target.value); }} />}
      </div> : null}
      <Button variant="ghost" size="sm" disabled={disabled || !draftRoute} onClick={() => changeRoute(null)}><Trash2 className="size-3.5" />{copy("清除", "Clear")}</Button>
      {dirty ? <Button variant="ghost" size="sm" disabled={disabled} onClick={() => {
        invalidate(); setDraftRoute(props.route ?? null); setDimensionText(initialDimensionText(props));
      }}>{copy("还原已保存设置", "Restore saved settings")}</Button> : null}
    </div>
    <div className="flex flex-wrap items-center justify-between gap-3 text-xs">
      <span className="text-muted-foreground">{dirty ? copy("更改尚未保存；已有项目不会自动重建。", "Unsaved changes; existing projects will not rebuild automatically.")
        : props.dimension === undefined && draftRoute ? copy("请明确选择维度后保存。", "Select and save an explicit dimension.")
          : props.testSummary ?? copy("维度必须明确；不使用供应商默认档位。", "Dimensions must be explicit; provider defaults are not used.")}</span>
      <Button size="sm" variant="outline" disabled={disabled || !validDraft || (!draftRoute && !props.route)} onClick={() => void inspect()}>
        {busy === "preflight" ? <LoaderCircle className="size-3.5 animate-spin" /> : null}{dirty ? copy("检查变更", "Review changes") : copy("检查项目兼容性", "Review project compatibility")}
      </Button>
    </div>
    {!validDraft && draftRoute ? <p className="text-xs text-amber-700">{copy("请选择模型明确支持的正整数维度。", "Choose a positive dimension explicitly supported by this model.")}</p> : null}
    {error ? <p role="alert" className="break-words text-xs text-destructive">{error}</p> : null}
    {!dirty && props.status.issue ? <p className="break-words text-xs text-amber-700">{props.status.issue}</p> : null}
    {review ? <div className="space-y-3 rounded-lg border bg-muted/20 p-3" aria-label={copy("模型变更确认", "Confirm model changes")}>
      <p className="text-sm font-medium">{review.candidate ? copy(`确认使用 ${review.candidate.dimension} 维向量`, `Confirm ${review.candidate.dimension}-dimension vectors`) : copy("确认清除向量模型", "Confirm embedding removal")}</p>
      {review.projects.filter((project) => !project.compatible).map((project) => <fieldset key={project.impact.scope.project_id} className="space-y-2 rounded-md border bg-background p-3" disabled={disabled}>
        <legend className="px-1 text-sm">{project.projectName}</legend>
        <p className="text-xs text-muted-foreground">{project.impact.hardRebuildReason
          ? copy(`已有索引 ${project.impact.scope.dimension} 维；本次变更必须重建，保存不会自动开始。`, `Existing index: ${project.impact.scope.dimension} dimensions. Rebuild required; saving will not start it.`)
          : copy(`包含 ${project.impact.sourceFingerprints.length} 个精确来源。名称相同也需要您确认兼容。`, `${project.impact.sourceFingerprints.length} exact origins. Matching names do not establish compatibility.`)}</p>
        {!project.impact.hardRebuildReason ? <label className="flex items-start gap-2 text-xs"><input type="radio" name={`vce-compat-${project.impact.scope.project_id}`} checked={decisions[project.impact.scope.project_id] === "compatible"}
          onChange={() => setDecisions((current) => ({ ...current, [project.impact.scope.project_id]: "compatible" }))} />{copy("确认同一向量空间，继续使用已有索引", "Same vector space; keep using the existing index")}</label> : null}
        <label className="flex items-start gap-2 text-xs"><input type="radio" name={`vce-compat-${project.impact.scope.project_id}`} checked={decisions[project.impact.scope.project_id] === "rebuild"}
          onChange={() => setDecisions((current) => ({ ...current, [project.impact.scope.project_id]: "rebuild" }))} />{copy("需要重建，保留旧库并等待我手动操作", "Rebuild required; retain the old index until I start rebuilding")}</label>
        <details className="text-xs text-muted-foreground"><summary>{copy("查看精确来源", "Inspect exact origins")}</summary>
          <p className="mt-1 break-all">{project.impact.scope.project_id} · {project.impact.scope.root_hash}</p>
          {project.impact.sourceFingerprints.map((source) => <p className="mt-1 break-all" key={source}>{source}</p>)}
          {project.impact.hardRebuildReason ? <p className="mt-1">{project.impact.hardRebuildReason}</p> : null}
        </details>
      </fieldset>)}
      {review.projects.every((project) => project.compatible) ? <p className="text-xs text-muted-foreground">{copy("没有需要重新确认的已初始化索引。", "No initialized index requires a new compatibility decision.")}</p> : null}
      {!review.candidate ? <p className="text-xs text-muted-foreground">{copy("引擎将不可用，已有项目索引保留。", "The engine becomes unavailable; existing indexes are retained.")}</p> : null}
      <div className="flex justify-end gap-2"><Button size="sm" variant="ghost" disabled={disabled} onClick={() => { setReview(null); setDecisions({}); }}>{copy("取消", "Cancel")}</Button>
        <Button size="sm" disabled={disabled || !validDraft || !allDecided} onClick={() => void save()}>{busy === "save" ? <LoaderCircle className="size-3.5 animate-spin" /> : null}{copy("确认保存", "Confirm and save")}</Button></div>
    </div> : null}
  </section>;
}
