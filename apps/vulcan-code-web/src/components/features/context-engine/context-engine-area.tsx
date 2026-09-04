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
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
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

/// Creates a deterministic identity for one exact public-AI inference route.
/// 为一条精确公共 AI 推理路由创建确定性身份。
///
/// Parameters: `route` contains provider, service, account, model, binding, operation, and scope.
/// 参数：`route` 包含供应商、服务、账号、模型、绑定、操作与作用域。
/// Returns: a stable selector value without parsing persisted composite keys.
/// 返回值：无需解析持久化组合键的稳定选择器值。
export function vceInferenceRouteKey(route: PublicAiInferenceRoute): string {
  const binding = route.binding.kind === "protocol"
    ? `protocol:${route.binding.protocol_profile_id}`
    : `action:${route.binding.action_binding_id}`;
  return [
    route.provider_id,
    route.service_entry_id,
    route.connection_id,
    route.model_id,
    binding,
    route.operation,
    route.usage_scope,
  ].join("\u0000");
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

  const reload = React.useCallback(async () => {
    const [nextSnapshot, nextReadiness] = await Promise.all([
      loadPublicAiSettings(),
      loadVceConfigurationStatus(),
    ]);
    setSnapshot(nextSnapshot);
    setReadiness(nextReadiness);
    setError(null);
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
    let active = true;
    void Promise.all([loadPublicAiSettings(), loadVceConfigurationStatus()])
      .then(([nextSnapshot, nextReadiness]) => {
        if (active) {
          setSnapshot(nextSnapshot);
          setReadiness(nextReadiness);
          setError(null);
        }
      })
      .catch((cause) => {
        if (active) {
          setError(errorMessage(cause));
        }
      });
    return () => {
      active = false;
    };
  }, [props.workbenchRevision]);

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
      const next = await savePublicAiInferenceRoute({
        consumerProfileId: VCE_CONSUMER_PROFILE_ID,
        purposeId: definition.purposeId,
        operation: definition.operation,
        route,
      });
      const nextReadiness = await loadVceConfigurationStatus();
      setSnapshot(next);
      setReadiness(nextReadiness);
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      mutationInFlightRef.current = false;
      setBusyKey(null);
    }
  }, []);

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
    try {
      const result = await testPublicAiInferenceRoute({
        consumerProfileId: VCE_CONSUMER_PROFILE_ID,
        operation: definition.operation,
      });
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
      await reload();
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      mutationInFlightRef.current = false;
      setBusyKey(null);
    }
  }, [copy, reload]);

  return (
    <section className="rounded-xl border bg-card p-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="flex min-w-0 items-start gap-3">
          <BrainCircuit className="mt-0.5 size-5 shrink-0" />
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-2">
              <strong>{copy("语义引擎", "ContextEngine")}</strong>
              <span className="rounded-full border px-2 py-0.5 text-[11px] text-muted-foreground">
                {copy("内置能力", "Built-in capability")}
              </span>
              <span className={readiness?.ready
                ? "rounded-full bg-emerald-500/15 px-2 py-0.5 text-[11px] text-emerald-700 dark:text-emerald-300"
                : "rounded-full bg-amber-500/15 px-2 py-0.5 text-[11px] text-amber-700 dark:text-amber-300"}
              >
                {readiness?.ready ? copy("已就绪", "Ready") : copy("已关闭", "Closed")}
              </span>
            </div>
            <p className="mt-1 text-xs text-muted-foreground">
              {copy(
                "为 AI 提供项目级代码语义检索。三条模型路由全部有效后，项目入口、索引监听与检索工具才会启用。",
                "Provides project-scoped semantic code retrieval for AI. Project UI, indexing, file watching, and the retrieval tool remain closed until all three model routes are valid.",
              )}
            </p>
          </div>
        </div>
        <Button disabled={busyKey !== null} onClick={() => void recheck()} size="sm" variant="outline">
          <RefreshCw className="size-3.5" />
          {copy("重新检查", "Recheck")}
        </Button>
      </div>

      <div className="mt-3 grid gap-2 md:grid-cols-2">
        <div className="flex items-start gap-2 rounded-lg border bg-muted/30 px-3 py-2 text-xs text-muted-foreground">
          <BrainCircuit className="mt-0.5 size-3.5 shrink-0" />
          <span>{copy(
            "查询改写固定关闭模型推理，不提供用户档位；不能明确关闭推理的 LLM 不可选择。",
            "Query rewriting always disables model reasoning. There is no user-selectable effort, and incompatible LLMs cannot be selected.",
          )}</span>
        </div>
        <div className="flex items-start gap-2 rounded-lg border bg-muted/30 px-3 py-2 text-xs text-muted-foreground">
          <Database className="mt-0.5 size-3.5 shrink-0" />
          <span>{copy(
            "索引存储由 VulcanCode 的 VLDB/LanceDB 数据平面统一提供，VCE 不创建独立数据库配置。",
            "Index storage comes from VulcanCode's shared VLDB/LanceDB data plane; VCE creates no separate database configuration.",
          )}</span>
        </div>
      </div>

      {error ? (
        <div className="mt-3 flex items-start gap-2 rounded-lg border border-destructive/40 bg-destructive/5 px-3 py-2 text-xs text-destructive">
          <CircleAlert className="mt-0.5 size-3.5 shrink-0" />
          <span>{error}</span>
        </div>
      ) : null}

      {!snapshot || !readiness ? (
        <div className="mt-4 flex items-center gap-2 text-sm text-muted-foreground">
          <LoaderCircle className="size-4 animate-spin" />
          {copy("正在读取公共 AI 模型目录…", "Loading the public AI model catalog…")}
        </div>
      ) : (
        <div className="mt-4 grid gap-3 lg:grid-cols-3">
          {definitions.map((definition) => {
            const route = routeForDefinition(profile, definition);
            const candidates = snapshot.internalInferenceCandidates.filter((candidate) => (
              vceCandidateEligible(candidate, definition.kind)
            ));
            const status = statusForDefinition(readiness, definition.kind);
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

      {readiness && readiness.issues.length > 0 ? (
        <ul className="mt-3 list-disc space-y-1 pl-5 text-xs text-amber-700 dark:text-amber-300">
          {readiness.issues.map((issue, index) => <li key={`${index}:${issue}`}>{issue}</li>)}
        </ul>
      ) : null}
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
  return (
    <div className="rounded-lg border p-3">
      <div className="flex items-start justify-between gap-2">
        <div>
          <div className="text-sm font-medium">{props.definition.label}</div>
          <p className="mt-0.5 text-xs text-muted-foreground">{props.definition.description}</p>
        </div>
        {props.status?.available ? (
          <CheckCircle2 className="size-4 shrink-0 text-emerald-600" />
        ) : (
          <CircleAlert className="size-4 shrink-0 text-amber-600" />
        )}
      </div>
      <Select
        disabled={props.disabled || props.candidates.length === 0}
        onValueChange={(value) => {
          const candidate = candidateByKey.get(value);
          if (candidate) {
            void props.onChange(candidate.route);
          }
        }}
        value={candidateByKey.has(selectedKey) ? selectedKey : undefined}
      >
        <SelectTrigger className="mt-3 w-full">
          <SelectValue placeholder={props.candidates.length > 0
            ? copy("选择模型与账号", "Select model and account")
            : copy("没有兼容模型", "No compatible models")} />
        </SelectTrigger>
        <SelectContent>
          {props.candidates.map((candidate) => (
            <SelectItem key={vceInferenceRouteKey(candidate.route)} value={vceInferenceRouteKey(candidate.route)}>
              {candidate.modelDisplayName} · {candidate.connectionDisplayName}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <div className="mt-2 flex flex-wrap items-center gap-2">
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
      {props.testSummary ? <p className="mt-2 text-xs text-emerald-700 dark:text-emerald-300">{props.testSummary}</p> : null}
      {props.status?.issue ? <p className="mt-2 text-xs text-amber-700 dark:text-amber-300">{props.status.issue}</p> : null}
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

/// Reports whether one embedding contract has a single deterministic positive dimension.
/// 判断一个向量契约是否具有唯一且确定的正维度。
function deterministicEmbeddingDimension(
  capability: NonNullable<PublicAiInternalInferenceCandidate["capabilities"]["embedding"]>,
): boolean {
  if ((capability.default_dimensions ?? 0) > 0) {
    return true;
  }
  if (capability.dimensions.length === 1 && capability.dimensions[0] > 0) {
    return true;
  }
  return (capability.minimum_dimensions ?? 0) > 0
    && capability.minimum_dimensions === capability.maximum_dimensions;
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
