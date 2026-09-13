import * as React from "react";
import {
  BrainCircuit,
  CheckCircle2,
  CircleAlert,
  Database,
  LoaderCircle,
  Play,
  RefreshCw,
  Trash2,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import { Combobox } from "@/components/ui/combobox";
import { VceEmbeddingSettings } from "./vce-embedding-settings";
import { vceDimensionDomain } from "./vce-embedding-dimensions";
import { vceInferenceRouteKey, vceRoutePickerOptions } from "./vce-route-picker";
export { vceInferenceRouteKey, vceRoutePickerOptions } from "./vce-route-picker";
import { useI18n } from "@/i18n";
import {
  loadPublicAiSettings,
  savePublicAiInferenceRoute,
  testPublicAiInferenceRoute,
} from "@/service/settings-actions-service";
import { loadVceConfigurationStatus } from "@/service/vce-service";
import type {
  PublicAiInferenceRoute,
  PublicAiInternalInferenceCandidate,
  PublicAiOperation,
  PublicAiSettingsSnapshot,
  VceConfigurationCapabilityStatus,
  VceGlobalConfigurationStatus,
} from "@/types";

/// Stable public-AI consumer profile owned by ContextEngine.
/// ContextEngine 持有的稳定公共 AI 消费者配置档。
export const VCE_CONSUMER_PROFILE_ID = "vce";

/// Stable LLM purpose used only for code-search query rewriting.
/// 仅用于代码检索查询改写的稳定 LLM 用途。
export const VCE_QUERY_REWRITE_PURPOSE_ID = "code_retrieval.query_rewrite";

// VceRouteKind identifies one required selector in the ContextEngine settings page.
// VceRouteKind 标识 ContextEngine 设置页中的一个必需选择器。
type VceRouteKind = "embedding" | "query_rewrite" | "rerank";

// VceRouteFieldDefinition describes one required route without duplicating provider behavior.
// VceRouteFieldDefinition 描述一条必需路由且不复制 Provider 行为。
interface VceRouteFieldDefinition {
  // Stable selector identity.
  // 稳定选择器身份。
  kind: VceRouteKind;
  // Exact shared inference operation.
  // 精确共享推理操作。
  operation: PublicAiOperation;
  // Required purpose for the restricted LLM route.
  // 受限 LLM 路由要求的用途。
  purposeId?: string;
  // User-visible route label.
  // 用户可见路由标签。
  label: string;
  // User-visible contract summary.
  // 用户可见契约摘要。
  description: string;
}

// VceRouteFieldProps carries one authoritative route and its catalog-backed candidates.
// VceRouteFieldProps 携带一条权威路由及其目录支持的候选项。
interface VceRouteFieldProps {
  // Immutable selector definition.
  // 不可变选择器定义。
  definition: VceRouteFieldDefinition;
  // Current persisted route when configured.
  // 已配置时的当前持久化路由。
  route?: PublicAiInferenceRoute;
  // Current catalog candidates satisfying the field contract.
  // 满足字段契约的当前目录候选项。
  candidates: PublicAiInternalInferenceCandidate[];
  // Backend-authoritative capability status.
  // 后端权威能力状态。
  status?: VceConfigurationCapabilityStatus;
  // Whether user mutations are temporarily blocked.
  // 是否暂时阻止用户修改。
  disabled: boolean;
  // Whether this exact route is being tested.
  // 是否正在测试此精确路由。
  testing: boolean;
  // Compact successful provider-test result.
  // 紧凑的成功 Provider 测试结果。
  testSummary?: string;
  // Complete route replacement callback.
  // 完整路由替换回调。
  onChange: (route: PublicAiInferenceRoute | null) => Promise<void>;
  // Optional real-provider test callback.
  // 可选真实 Provider 测试回调。
  onTest?: () => Promise<void>;
}

// ContextEngineAreaProps carries the workbench generation used to invalidate route catalogs.
// ContextEngineAreaProps 携带用于使路由目录失效的工作台代际。
interface ContextEngineAreaProps {
  // Optional current workbench generation.
  // 可选当前工作台代际。
  workbenchRevision?: number;
}

/// Returns whether one public-AI candidate satisfies the exact VCE operation contract.
/// 判断一个公共 AI 候选项是否满足精确的 VCE 操作契约。
///
/// Parameters: `candidate` is one executable catalog route and `kind` selects the VCE contract.
/// 参数：`candidate` 是一条可执行目录路由，`kind` 选择 VCE 契约。
/// Returns: `true` only for internal-background candidates with every required capability.
/// 返回值：仅对具备全部必需能力的内部后台候选项返回 `true`。
export function vceCandidateEligible(
  candidate: PublicAiInternalInferenceCandidate,
  kind: VceRouteKind,
): boolean {
  if (!candidate.enabled || candidate.route.usage_scope !== "internal_background") {
    return false;
  }
  if (kind === "embedding") {
    const capability = candidate.capabilities.embedding;
    return candidate.route.operation === "embedding_create"
      && Boolean(capability)
      && (
        capability!.tasks.includes("provider_default")
        || (
          capability!.tasks.includes("document")
          && capability!.tasks.includes("query")
        )
      )
      && capability!.output_kinds.includes("dense")
      && capability!.encodings.includes("float")
      && deterministicEmbeddingDimension(capability!);
  }
  if (kind === "query_rewrite") {
    return candidate.route.operation === "conversation_respond"
      && candidate.capabilities.structured_output !== "unsupported"
      && candidateSupportsDisabledReasoning(candidate);
  }
  return candidate.route.operation === "rerank_documents"
    && Boolean(candidate.capabilities.rerank)
    && candidate.capabilities.rerank!.truncation.some((truncation) => (
      truncation === "disabled" || truncation === "provider_default"
    ));
}

/// Renders and owns the fixed ContextEngine settings surface.
/// 渲染并管理固定的 ContextEngine 设置界面。
///
/// Parameters: `workbenchRevision` invalidates the view after committed public-AI publications.
/// 参数：`workbenchRevision` 在公共 AI 发布提交后使界面状态失效。
/// Returns: the complete route, readiness, reasoning-policy, storage, and test panel.
/// 返回值：完整的路由、就绪状态、推理策略、存储与测试面板。
export function ContextEngineArea(props: ContextEngineAreaProps) {
  // locale selects local bilingual copy without changing the backend-owned diagnostic contract.
  // locale 选择本地双语文案且不改变后端持有的诊断契约。
  const { locale } = useI18n();
  // copy returns Chinese text for the Chinese locale and English text otherwise.
  // copy 为中文语言环境返回中文文案，否则返回英文文案。
  const copy = React.useCallback(
    (zh: string, en: string) => (locale === "zh-CN" ? zh : en),
    [locale],
  );
  // snapshot contains current executable candidates and persisted VCE routes.
  // snapshot 包含当前可执行候选项与已持久化 VCE 路由。
  const [snapshot, setSnapshot] = React.useState<PublicAiSettingsSnapshot | null>(null);
  // readiness is the backend-authoritative all-or-nothing VCE gate.
  // readiness 是后端权威的 VCE 全有或全无闸门。
  const [readiness, setReadiness] = React.useState<VceGlobalConfigurationStatus | null>(null);
  // error retains only the latest settings or validation diagnostic.
  // error 仅保留最近一条设置或校验诊断。
  const [error, setError] = React.useState<string | null>(null);
  // busyKey serializes route mutations and explicit provider tests.
  // busyKey 串行化路由变更与显式 Provider 测试。
  const [busyKey, setBusyKey] = React.useState<string | null>(null);
  // testSummaries retain compact facts from successful real embedding and rerank requests.
  // testSummaries 保留真实向量与重排请求成功后的紧凑事实。
  const [testSummaries, setTestSummaries] = React.useState<Partial<Record<VceRouteKind, string>>>({});
  const mutationInFlightRef = React.useRef(false);
  // Every settings read shares one observation sequence; late successes and errors lose authority.
  // 全部设置读取共享一个观察序号；迟到成功及错误失去权威。
  const observationSequence = React.useRef(0);

  // Reload both authoritative projections; only the latest live read may update the page.
  // 重载两项权威投影；仅最新且仍有效的读取可以更新页面。
  // No parameters; returns the accepted snapshot/sequence or null when this read fails or loses authority.
  // 无参数；返回已接纳快照及序号，读取失败或失去权威时返回空值。
  const reload = React.useCallback(async () => {
    // admitted owns this exact read, including its failure path.
    // admitted 持有此次精确读取，包括其失败路径。
    const admitted = ++observationSequence.current;
    try {
      const [nextSnapshot, nextReadiness] = await Promise.all([
        loadPublicAiSettings(),
        loadVceConfigurationStatus(),
      ]);
      if (observationSequence.current !== admitted) return null;
      setSnapshot(nextSnapshot);
      setReadiness(nextReadiness);
      // Test summaries belong to a verified observation, not every future model shown in the same slot.
      // 测试摘要属于已验证观察，而非同一槽位以后显示的所有模型。
      setTestSummaries({});
      setError(null);
      return { snapshot: nextSnapshot, sequence: admitted };
    } catch (cause) {
      if (observationSequence.current === admitted) setError(errorMessage(cause));
      return null;
    }
  }, []);

  // recheck reloads both catalogs under the same mutation gate used by route saves and tests.
  // recheck 在路由保存与测试共用的修改闸门下重新加载两类目录。
  const recheck = React.useCallback(async () => {
    if (mutationInFlightRef.current) {
      return;
    }
    mutationInFlightRef.current = true;
    setBusyKey("reload");
    setError(null);
    try {
      await reload();
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      mutationInFlightRef.current = false;
      setBusyKey(null);
    }
  }, [reload]);

  React.useEffect(() => {
    void reload();
    return () => {
      observationSequence.current += 1;
    };
  }, [props.workbenchRevision, reload]);

  // profile is the sole persisted public-AI route set owned by VCE.
  // profile 是 VCE 唯一持有的持久化公共 AI 路由集合。
  const profile = snapshot?.inferenceProfiles[VCE_CONSUMER_PROFILE_ID];
  // definitions keep all required selectors aligned with their backend operation identities.
  // definitions 使全部必需选择器与其后端操作身份保持一致。
  const definitions: VceRouteFieldDefinition[] = [
    {
      kind: "embedding",
      operation: "embedding_create",
      label: copy("Embedding 向量模型", "Embedding model"),
      description: copy(
        "复用宿主已验证的源码文档与查询向量能力；支持专用任务或 Provider 默认任务，维度必须可确定。",
        "Reuses host-validated document and query vectors through dedicated or provider-default tasks, with deterministic dimensions.",
      ),
    },
    {
      kind: "query_rewrite",
      operation: "conversation_respond",
      purposeId: VCE_QUERY_REWRITE_PURPOSE_ID,
      label: copy("LLM 查询改写模型", "LLM query rewrite model"),
      description: copy(
        "生成严格 JSON 的代码检索查询变体；宿主固定关闭模型推理。",
        "Generates strict-JSON code-search variants with model reasoning fixed off by the host.",
      ),
    },
    {
      kind: "rerank",
      operation: "rerank_documents",
      label: copy("Rerank 重排模型", "Rerank model"),
      description: copy(
        "对语义候选进行二次排序；使用模型明确声明的禁用截断或 Provider 默认截断策略。",
        "Reorders semantic candidates with an explicitly declared disabled or provider-default truncation policy.",
      ),
    },
  ];

  // saveRoute replaces one complete profile slot and reloads the backend-authoritative gate.
  // saveRoute 替换一个完整配置档槽位并重新加载后端权威闸门。
  const saveRoute = React.useCallback(async (
    definition: VceRouteFieldDefinition,
    route: PublicAiInferenceRoute | null,
  ) => {
    if (mutationInFlightRef.current) {
      return;
    }
    mutationInFlightRef.current = true;
    setBusyKey(`save:${definition.kind}`);
    setError(null);
    setTestSummaries((current) => ({ ...current, [definition.kind]: undefined }));
    try {
      await savePublicAiInferenceRoute({
        consumerProfileId: VCE_CONSUMER_PROFILE_ID,
        purposeId: definition.purposeId,
        operation: definition.operation,
        route,
      });
      // Read current authority after acknowledgement; do not replay a possibly superseded save snapshot.
      // 确认后读取当前权威，不重放可能已被替换的保存快照。
      await reload();
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      mutationInFlightRef.current = false;
      setBusyKey(null);
    }
  }, [reload]);

  // testRoute executes an existing real-provider test for one non-LLM route.
  // testRoute 对一条非 LLM 路由执行现有真实 Provider 测试。
  const testRoute = React.useCallback(async (
    definition: VceRouteFieldDefinition,
  ) => {
    if (definition.operation === "conversation_respond" || mutationInFlightRef.current) {
      return;
    }
    mutationInFlightRef.current = true;
    setBusyKey(`test:${definition.kind}`);
    setError(null);
    // This read sequence owns the test response and any error; unrelated newer reads retire both.
    // 此读取序号持有测试响应及错误；无关较新读取使二者退役。
    let testObservation = observationSequence.current;
    // Freeze the displayed saved route and dimension before the host resolves the actual execution.
    // 在宿主解析实际执行前固定展示中的已保存路由及维度。
    const testedRoute = routeForDefinition(profile, definition);
    const testedDimension = profile?.embedding_dimensions;
    try {
      if (!testedRoute) throw new Error(copy("请先保存测试模型。", "Save the model before testing."));
      const result = await testPublicAiInferenceRoute({
        consumerProfileId: VCE_CONSUMER_PROFILE_ID,
        operation: definition.operation,
      });
      if (observationSequence.current !== testObservation) return;
      // Re-read after execution; the test response itself has no authority to restore old configuration.
      // 执行后重新读取；测试响应本身无权恢复旧配置。
      const reloaded = await reload();
      if (reloaded === null || observationSequence.current !== reloaded.sequence) return;
      testObservation = reloaded.sequence;
      // Match full saved routing intent plus the actual non-secret execution identity returned by the host.
      // 同时匹配完整已保存路由意图及宿主返回的实际非敏感执行身份。
      const currentProfile = reloaded.snapshot.inferenceProfiles[VCE_CONSUMER_PROFILE_ID];
      const currentRoute = routeForDefinition(currentProfile, definition);
      if (!currentRoute || vceInferenceRouteKey(currentRoute) !== vceInferenceRouteKey(testedRoute)
        || result.operation !== definition.operation || result.providerId !== currentRoute.provider_id
        || result.serviceEntryId !== currentRoute.service_entry_id || result.connectionId !== currentRoute.connection_id
        || result.modelId !== currentRoute.model_id
        || (definition.kind === "embedding" && (currentProfile?.embedding_dimensions !== testedDimension
          || testedDimension === undefined || result.dimensions !== testedDimension))) {
        throw new Error(copy("模型配置或测试目标已变化，请重新测试当前模型。", "Model configuration or test target changed; test the current model again."));
      }
      const detail = definition.kind === "embedding"
        ? copy(
          `${result.resultCount} 条结果 · ${result.dimensions ?? 0} 维 · ${result.durationMs} ms`,
          `${result.resultCount} results · ${result.dimensions ?? 0} dimensions · ${result.durationMs} ms`,
        )
        : copy(
          `${result.resultCount} 条结果 · ${result.durationMs} ms`,
          `${result.resultCount} results · ${result.durationMs} ms`,
        );
      setTestSummaries((current) => ({ ...current, [definition.kind]: detail }));
    } catch (cause) {
      if (observationSequence.current === testObservation) setError(errorMessage(cause));
    } finally {
      mutationInFlightRef.current = false;
      setBusyKey(null);
    }
  }, [copy, profile, reload]);

  // unresolvedRouteCount summarizes the remaining configuration work without repeating every diagnostic.
  // unresolvedRouteCount 汇总剩余配置工作，避免重复展示每一条诊断。
  const unresolvedRouteCount = readiness
    ? [readiness.embedding, readiness.queryRewrite, readiness.rerank]
      .filter((status) => !status.available).length
    : 3;

  return (
    // Measure the settings pane itself: workbench sidebars can leave a narrow pane in a wide window.
    // 测量设置面板自身：工作台侧栏可能让宽窗口中的内容区域仍然很窄。
    <section className="@container/vce mx-auto w-full max-w-6xl space-y-5">
      <header className="flex flex-wrap items-start justify-between gap-4 border-b pb-4">
        <div className="flex min-w-0 items-start gap-3">
          <span className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary">
            <BrainCircuit className="size-5" />
          </span>
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-2">
              <h2 className="text-base font-semibold">{copy("语义引擎", "ContextEngine")}</h2>
              <span className={readiness?.ready
                ? "rounded-full bg-emerald-500/15 px-2 py-0.5 text-[11px] text-[var(--vcn-content-success)] dark:text-emerald-300"
                : "rounded-full bg-amber-500/15 px-2 py-0.5 text-[11px] text-[var(--vcn-content-warning)] dark:text-amber-300"}
              >
                {readiness?.ready ? copy("已就绪", "Ready") : copy("已关闭", "Closed")}
              </span>
            </div>
            <p className="mt-1 text-xs text-muted-foreground">
              {copy(
                "为 AI 提供项目级代码语义检索；配置模型后，在项目设置中启用并明确创建索引。",
                "Provides project-scoped semantic code retrieval for AI. After configuring models, enable a project and explicitly create its index in project settings.",
              )}
            </p>
          </div>
        </div>
        <Button disabled={busyKey !== null} onClick={() => void recheck()} size="sm" variant="outline">
          <RefreshCw className="size-3.5" />
          {copy("重新检查", "Recheck")}
        </Button>
      </header>

      <div className="flex flex-wrap gap-x-6 gap-y-2 text-xs text-muted-foreground">
        <div className="flex max-w-xl items-start gap-2">
          <BrainCircuit className="mt-0.5 size-3.5 shrink-0 text-foreground/60" />
          <span>{copy(
            "查询改写固定关闭模型思考，不提供额外思考档位。",
            "Query rewriting keeps model reasoning disabled and exposes no additional effort setting.",
          )}</span>
        </div>
        <div className="flex max-w-xl items-start gap-2">
          <Database className="mt-0.5 size-3.5 shrink-0 text-foreground/60" />
          <span>{copy(
            "索引存储复用 VulcanCode 的 VLDB/LanceDB，无需单独配置数据库。",
            "Index storage reuses VulcanCode's VLDB/LanceDB and needs no separate database setup.",
          )}</span>
        </div>
      </div>

      {error ? (
        <div className="flex items-start gap-2 rounded-lg border border-destructive/40 bg-destructive/5 px-3 py-2 text-xs text-destructive">
          <CircleAlert className="mt-0.5 size-3.5 shrink-0" />
          <span className="min-w-0 break-words">{error}</span>
        </div>
      ) : null}

      <div className="overflow-hidden rounded-xl border bg-card">
        <div className="flex flex-wrap items-center justify-between gap-2 border-b bg-muted/20 px-4 py-3">
          <div>
            <h3 className="text-sm font-medium">{copy("模型路由", "Model routes")}</h3>
            <p className="mt-0.5 text-xs text-muted-foreground">
              {readiness?.ready
                ? copy("三项能力均已就绪。", "All three capabilities are ready.")
                : copy(
                  `还需完成 ${unresolvedRouteCount} 项配置。`,
                  `${unresolvedRouteCount} configuration ${unresolvedRouteCount === 1 ? "item" : "items"} remaining.`,
                )}
            </p>
          </div>
          <span className="text-xs text-muted-foreground">
            {copy("向量变更需要确认", "Embedding changes require confirmation")}
          </span>
        </div>
        {!snapshot || !readiness ? (
          <div className="flex items-center gap-2 px-4 py-8 text-sm text-muted-foreground">
            <LoaderCircle className="size-4 animate-spin" />
            {copy("正在读取公共 AI 模型目录…", "Loading the public AI model catalog…")}
          </div>
        ) : (
          <div className="divide-y">
            {definitions.map((definition) => {
              const route = routeForDefinition(profile, definition);
              const candidates = snapshot.internalInferenceCandidates.filter((candidate) => (
                vceCandidateEligible(candidate, definition.kind)
              ));
              const status = statusForDefinition(readiness, definition.kind);
              if (definition.kind === "embedding") {
                return <VceEmbeddingSettings key={definition.kind} candidates={candidates}
                  route={profile?.embedding} dimension={profile?.embedding_dimensions}
                  status={readiness.embedding} disabled={busyKey !== null}
                  testing={busyKey === "test:embedding"} testSummary={testSummaries.embedding}
                  onTest={() => testRoute(definition)}
                  onBusyChange={(busy) => { mutationInFlightRef.current = busy; setBusyKey(busy ? "draft:embedding" : null); }}
                  onSaved={async () => {
                    setTestSummaries((current) => ({ ...current, embedding: undefined }));
                    // Use the same observation fence after embedding commits as every other route read.
                    // 向量提交后与其他路由读取使用相同的观察保护。
                    await reload();
                  }} />;
              }
              return (
                <VceRouteField
                  candidates={candidates}
                  definition={definition}
                  disabled={busyKey !== null}
                  key={definition.kind}
                  onChange={(nextRoute) => saveRoute(definition, nextRoute)}
                  onTest={definition.operation === "conversation_respond"
                    ? undefined
                    : () => testRoute(definition)}
                  route={route}
                  status={status}
                  testing={busyKey === `test:${definition.kind}`}
                  testSummary={testSummaries[definition.kind]}
                />
              );
            })}
          </div>
        )}
      </div>
    </section>
  );
}

/// Renders one required route selector and its authoritative status.
/// 渲染一个必需路由选择器及其权威状态。
function VceRouteField(props: VceRouteFieldProps) {
  // locale selects bilingual control copy without duplicating route semantics.
  // locale 选择双语控件文案且不复制路由语义。
  const { locale } = useI18n();
  // copy returns the control text for the active locale.
  // copy 返回当前语言环境对应的控件文案。
  const copy = (zh: string, en: string) => (locale === "zh-CN" ? zh : en);
  // selectedKey represents only an exact persisted route that still exists in eligible candidates.
  // selectedKey 仅表示仍存在于合格候选项中的精确持久化路由。
  const selectedKey = props.route ? vceInferenceRouteKey(props.route) : "";
  // candidateByKey resolves selector values without decoding a composite string.
  // candidateByKey 无需解码复合字符串即可解析选择器值。
  const candidateByKey = new Map(props.candidates.map((candidate) => [
    vceInferenceRouteKey(candidate.route),
    candidate,
  ]));
  // pickerOptions expose the shared searchable selector contract without weakening route identity.
  // pickerOptions 暴露共享可搜索选择器契约，同时不削弱路由身份。
  const pickerOptions = vceRoutePickerOptions(props.candidates);
  // selectedValue is defined only while the persisted route remains an eligible exact candidate.
  // selectedValue 仅在持久化路由仍是合格的精确候选项时存在。
  const selectedValue = candidateByKey.has(selectedKey) ? selectedKey : undefined;
  return (
    <div className="grid gap-4 px-4 py-4 @3xl/vce:grid-cols-[minmax(0,0.9fr)_minmax(18rem,1.25fr)_auto] @3xl/vce:items-start">
      <div className="flex min-w-0 items-start gap-3">
        <span className={props.status?.available
          ? "mt-0.5 flex size-7 shrink-0 items-center justify-center rounded-full bg-emerald-500/10 text-[var(--vcn-content-success)]"
          : "mt-0.5 flex size-7 shrink-0 items-center justify-center rounded-full bg-amber-500/10 text-[var(--vcn-content-warning)]"}
        >
          {props.status?.available
            ? <CheckCircle2 className="size-4" />
            : <CircleAlert className="size-4" />}
        </span>
        <div className="min-w-0">
          <div className="text-sm font-medium">{props.definition.label}</div>
          <p className="mt-1 text-xs leading-relaxed text-muted-foreground">{props.definition.description}</p>
        </div>
      </div>
      <div className="min-w-0">
        <Combobox
          ariaLabel={props.definition.label}
          className="h-10"
          disabled={props.disabled || props.candidates.length === 0}
          emptyText={copy("没有匹配的兼容模型", "No compatible models match this filter")}
          onValueChange={(value) => {
            const candidate = candidateByKey.get(value);
            if (candidate) {
              void props.onChange(candidate.route);
            }
          }}
          options={pickerOptions}
          placeholder={props.route?.model_id ?? (props.candidates.length > 0
            ? copy("选择模型与账号", "Select model and account")
            : copy("没有兼容模型", "No compatible models"))}
          searchPlaceholder={copy(
            "筛选模型、ID、供应商或账号…",
            "Filter by model, ID, provider, or account…",
          )}
          value={selectedValue}
        />
        {props.testSummary ? (
          <p className="mt-2 text-xs text-[var(--vcn-content-success)] dark:text-emerald-300">{props.testSummary}</p>
        ) : null}
        {props.status?.issue ? (
          <p className="mt-2 break-words text-xs text-[var(--vcn-content-warning)] dark:text-amber-300">{props.status.issue}</p>
        ) : null}
      </div>
      <div className="flex flex-wrap items-center gap-2 @3xl/vce:justify-end">
        {props.onTest ? (
          <Button
            disabled={props.disabled || !props.route}
            onClick={() => void props.onTest?.()}
            size="sm"
            variant="outline"
          >
            {props.testing ? <LoaderCircle className="size-3.5 animate-spin" /> : <Play className="size-3.5" />}
            {copy("测试", "Test")}
          </Button>
        ) : null}
        <Button
          disabled={props.disabled || !props.route}
          onClick={() => void props.onChange(null)}
          size="sm"
          variant="ghost"
        >
          <Trash2 className="size-3.5" />
          {copy("清除", "Clear")}
        </Button>
      </div>
    </div>
  );
}

/// Resolves the persisted route represented by one VCE field definition.
/// 解析一个 VCE 字段定义表示的持久化路由。
function routeForDefinition(
  profile: PublicAiSettingsSnapshot["inferenceProfiles"][string] | undefined,
  definition: VceRouteFieldDefinition,
): PublicAiInferenceRoute | undefined {
  if (definition.kind === "embedding") {
    return profile?.embedding;
  }
  if (definition.kind === "rerank") {
    return profile?.rerank;
  }
  return profile?.llm_routes?.[VCE_QUERY_REWRITE_PURPOSE_ID];
}

/// Resolves the backend-authoritative capability status for one selector.
/// 解析一个选择器对应的后端权威能力状态。
function statusForDefinition(
  readiness: VceGlobalConfigurationStatus,
  kind: VceRouteKind,
): VceConfigurationCapabilityStatus {
  if (kind === "embedding") {
    return readiness.embedding;
  }
  if (kind === "rerank") {
    return readiness.rerank;
  }
  return readiness.queryRewrite;
}

/// Reports whether one embedding contract declares at least one explicit supported dimension.
/// 判断一个向量契约是否声明至少一个明确受支持维度。
function deterministicEmbeddingDimension(
  capability: NonNullable<PublicAiInternalInferenceCandidate["capabilities"]["embedding"]>,
): boolean {
  return vceDimensionDomain(capability) !== null;
}

/// Reports whether one LLM contract and route can explicitly disable reasoning.
/// 判断一个 LLM 契约与路由能否显式关闭推理。
function candidateSupportsDisabledReasoning(
  candidate: PublicAiInternalInferenceCandidate,
): boolean {
  const control = candidate.capabilities.reasoning_control ?? "unknown";
  if (control === "fixed_off") {
    return true;
  }
  return (
    control === "toggle"
    || control === "effort_selection"
    || control === "provider_adaptive"
  ) && (candidate.capabilities.reasoning_efforts ?? []).includes("none");
}

/// Converts one unknown rejection into a stable user-visible message.
/// 将一项未知拒绝转换为稳定的用户可见消息。
function errorMessage(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}
