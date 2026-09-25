import * as React from "react";
import { toast } from "sonner";
import { VceActionButton } from "./vce-action-button";
import {
  BrainCircuit,
  CheckCircle2,
  CircleAlert,
  Database,
  LoaderCircle,
  Bug,
  RefreshCw,
  Eraser,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import { StreamPlaceholder } from "@/components/ui/stream-placeholder";
import { Combobox } from "@/components/ui/combobox";
import { VceEmbeddingSettings } from "./vce-embedding-settings";
import { VceHelpTooltip } from "./vce-help-tooltip";
import { vceDimensionDomain } from "./vce-embedding-dimensions";
import { vceInferenceRouteKey, vceRoutePickerOptions } from "./vce-route-picker";
export { vceInferenceRouteKey, vceRoutePickerOptions } from "./vce-route-picker";
import { formatMessage, useI18n } from "@/i18n";
import { formatDurationSeconds } from "@/lib/usage-format";
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
import { errorMessage } from "@/lib/display-format";

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
  // t selects local bilingual copy without changing the backend-owned diagnostic contract.
  // t 选择本地双语文案且不改变后端持有的诊断契约。
  const { t } = useI18n();
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
      label: t("contextEngine.embeddingModel"),
      description: t("contextEngine.reusesHostValidatedDocument"),
    },
    {
      kind: "query_rewrite",
      operation: "conversation_respond",
      purposeId: VCE_QUERY_REWRITE_PURPOSE_ID,
      label: t("contextEngine.largeLanguageModel"),
      description: t("contextEngine.generatesStrictJsonCode"),
    },
    {
      kind: "rerank",
      operation: "rerank_documents",
      label: t("contextEngine.rerankingModel"),
      description: t("contextEngine.reordersSemanticCandidatesWith"),
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
      if (!testedRoute) throw new Error(t("contextEngine.saveTheModelBefore"));
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
        throw new Error(t("contextEngine.modelConfigurationOrTest"));
      }
      const detail = definition.kind === "embedding"
        ? formatMessage(t("contextEngine.resultsDimensions"), { resultCount: result.resultCount, dimensions: result.dimensions ?? 0, value: formatDurationSeconds(result.durationMs) })
        : formatMessage(t("contextEngine.results"), { resultCount: result.resultCount, value: formatDurationSeconds(result.durationMs) });
      toast.success(t("contextEngine.connectionTestSucceeded"), { description: detail });
    } catch (cause) {
      if (observationSequence.current === testObservation) toast.error(t("contextEngine.connectionTestFailed"), { description: errorMessage(cause) });
    } finally {
      mutationInFlightRef.current = false;
      setBusyKey(null);
    }
  }, [profile, reload, t]);

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
              <h2 className="text-base font-semibold">{t("contextEngine.codeSemanticIndex")}</h2>
              <span className={readiness?.ready
                ? "rounded-full bg-emerald-500/15 px-2 py-0.5 text-[11px] text-[var(--vcn-content-success)] dark:text-emerald-300"
                : "rounded-full bg-amber-500/15 px-2 py-0.5 text-[11px] text-[var(--vcn-content-warning)] dark:text-amber-300"}
              >
                {readiness?.ready ? t("contextEngine.ready") : t("contextEngine.closed")}
              </span>
            </div>
            <p className="mt-1 text-xs text-muted-foreground">
              {t("contextEngine.providesProjectScopedSemantic")}
            </p>
          </div>
        </div>
        <Button disabled={busyKey !== null} onClick={() => void recheck()} size="sm" variant="outline">
          <RefreshCw className="size-3.5" />
          {t("contextEngine.recheck")}
        </Button>
      </header>

      <div className="flex flex-wrap gap-x-6 gap-y-2 text-xs text-muted-foreground">
        <div className="flex max-w-xl items-start gap-2">
          <BrainCircuit className="mt-0.5 size-3.5 shrink-0 text-foreground/60" />
          <span>{t("contextEngine.queryRewriting")}</span><VceHelpTooltip label={t("contextEngine.queryRewritingNotes")}>{t("contextEngine.queryRewritingKeepsModel")}</VceHelpTooltip>
        </div>
        <div className="flex max-w-xl items-start gap-2">
          <Database className="mt-0.5 size-3.5 shrink-0 text-foreground/60" />
          <span>{t("contextEngine.indexStorage")}</span><VceHelpTooltip label={t("contextEngine.indexStorageNotes")}>{t("contextEngine.indexStorageReusesVulcancode")}</VceHelpTooltip>
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
            <h3 className="text-sm font-medium">{t("contextEngine.modelRoutes")}</h3>
            <p className="mt-0.5 text-xs text-muted-foreground">
              {readiness?.ready
                ? t("contextEngine.allThreeCapabilitiesAre")
                : formatMessage(t(unresolvedRouteCount === 1 ? "contextEngine.remainingOne" : "contextEngine.remaining"), { count: unresolvedRouteCount })}
            </p>
          </div>
          <span className="text-xs text-muted-foreground">
            {t("contextEngine.embeddingChangesRequireConfirmation")}
          </span>
        </div>
        {!snapshot || !readiness ? (
          <StreamPlaceholder className="px-2 py-3" label={t("contextEngine.loadingThePublicAi")} shape="rows" count={4} />
        ) : (
          <div className="divide-y">
            <div className="hidden gap-3 px-4 py-2 text-xs text-muted-foreground @xl/vce:grid @xl/vce:grid-cols-[8rem_minmax(12rem,1fr)_9.5rem]">
              <span>{t("contextEngine.purpose")}</span><span>{t("contextEngine.modelAndAccount")}</span><span className="text-right">{t("contextEngine.actions")}</span>
            </div>
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
                  testing={busyKey === "test:embedding"}
                  onTest={() => testRoute(definition)}
                  onBusyChange={(busy) => { mutationInFlightRef.current = busy; setBusyKey(busy ? "draft:embedding" : null); }}
                  onSaved={async () => {
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
  // t selects bilingual control copy without duplicating route semantics.
  // t 选择双语控件文案且不复制路由语义。
  const { t } = useI18n();
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
    <div className="grid items-center gap-3 px-4 py-3 @xl/vce:grid-cols-[8rem_minmax(12rem,1fr)_9.5rem]">
      <div className="flex min-w-0 items-center gap-2">
        <span className={props.status?.available
          ? "flex size-7 shrink-0 items-center justify-center rounded-full bg-emerald-500/10 text-[var(--vcn-content-success)]"
          : "flex size-7 shrink-0 items-center justify-center rounded-full bg-amber-500/10 text-[var(--vcn-content-warning)]"}
        >
          {props.status?.available
            ? <CheckCircle2 className="size-4" />
            : <CircleAlert className="size-4" />}
        </span>
        <div className="min-w-0">
          <div className="flex items-center gap-1 text-xs font-medium">{props.definition.label}<VceHelpTooltip label={`${props.definition.label} ${t("contextEngine.notes")}`}>{props.definition.description}</VceHelpTooltip></div>
        </div>
      </div>
      <div className="min-w-0">
        <Combobox
          ariaLabel={props.definition.label}
          className="h-8 px-2 text-xs"
          disabled={props.disabled || props.candidates.length === 0}
          emptyText={t("contextEngine.noCompatibleModelsMatch")}
          onValueChange={(value) => {
            const candidate = candidateByKey.get(value);
            if (candidate) {
              void props.onChange(candidate.route);
            }
          }}
          options={pickerOptions}
          placeholder={props.route?.model_id ?? (props.candidates.length > 0
            ? t("contextEngine.selectModelAndAccount")
            : t("contextEngine.noCompatibleModels"))}
          searchPlaceholder={t("contextEngine.filterByModelId")}
          value={selectedValue}
        />
        {props.status?.issue ? (
          <p className="mt-2 break-words text-xs text-[var(--vcn-content-warning)] dark:text-amber-300">{props.status.issue}</p>
        ) : null}
      </div>
      <div className="flex flex-wrap items-center gap-2 @xl/vce:justify-end">
        {props.onTest ? (
          <VceActionButton label={t("contextEngine.testConnectivity")}
            disabled={props.disabled || !props.route} onClick={() => void props.onTest?.()}
            icon={props.testing ? <LoaderCircle className="size-4 animate-spin" /> : <Bug className="size-4" />} />
        ) : null}
        <VceActionButton label={t("contextEngine.clear")} disabled={props.disabled || !props.route}
          onClick={() => void props.onChange(null)} icon={<Eraser className="size-4" />} />
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
