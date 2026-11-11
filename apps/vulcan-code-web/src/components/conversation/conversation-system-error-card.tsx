import * as React from "react";
import { ChevronLeft, ChevronRight, CircleAlert } from "lucide-react";

import { StatusPill } from "@/components/shared/status";
import { useI18n } from "@/i18n";
import { Button } from "@/ui-kit/primitives/button";
import { formatDurationSeconds } from "@/lib/usage-format";

// SystemProviderError describes the structured failure emitted by the public provider contract.
// SystemProviderError 描述公共 Provider 契约发出的结构化失败信息。
export interface SystemProviderError {
  // Original transcript body retained for technical inspection.
  // 保留用于技术查看的原始转录正文。
  raw: string;
  // Provider route identifier that received the request.
  // 接收请求的 Provider 路由标识。
  provider: string;
  // Stable failure category assigned by the execution contract.
  // 执行契约分配的稳定失败类别。
  category: string;
  // Provider execution stage where the failure occurred.
  // 发生失败的 Provider 执行阶段。
  stage: string;
  // Dispatch outcome recorded before the provider failure was reported.
  // Provider 失败上报前记录的派发结果。
  dispatchOutcome: string;
  // Whether semantic provider output had already been observed.
  // 是否已经观察到 Provider 的语义输出。
  semanticOutputObserved: boolean;
  // Whether the execution contract considers the failure retryable.
  // 执行契约是否认为该失败可以重试。
  retryable: boolean;
  // Whether a tool or external side effect was already observed.
  // 是否已经观察到工具或外部副作用。
  sideEffectObserved: boolean;
  // Whether the execution was terminal when the failure was recorded.
  // 记录失败时执行是否已经终结。
  terminal: boolean;
  // Optional HTTP status included by the provider contract.
  // Provider 契约可选携带的 HTTP 状态码。
  httpStatus: string | null;
  // Optional provider-specific failure subcategory.
  // Provider 专属的可选失败子类别。
  subcategory: string | null;
  // Optional upstream error code.
  // 可选的上游错误代码。
  upstreamCode: string | null;
  // Optional upstream error type.
  // 可选的上游错误类型。
  upstreamType: string | null;
  // Optional upstream parameter associated with the error.
  // 与错误关联的可选上游参数。
  upstreamParameter: string | null;
  // Optional upstream message retained separately from the display message.
  // 与展示消息分开保留的可选上游消息。
  upstreamMessage: string | null;
  // Optional upstream or client request identifier.
  // 可选的上游或客户端请求编号。
  requestId: string | null;
  // Exact provider entry identifier retained in the JSON payload.
  // JSON 载荷中保留的精确 Provider 入口标识。
  entryId: string | null;
  // Exact model identifier retained in the JSON payload.
  // JSON 载荷中保留的精确模型标识。
  modelId: string | null;
  // Optional retry-after delay in milliseconds.
  // 可选的重试等待时长，单位为毫秒。
  retryAfterMs: string | null;
  // Human-readable provider error message.
  // 面向用户的 Provider 错误消息。
  message: string;
  // Lossless JSON object used by the expanded Markdown code block.
  // 展开后的 Markdown JSON 代码块使用的无损 JSON 对象。
  structured: Record<string, unknown>;
}

// SystemErrorCardProps describes the structured failure rendered by one system error card.
// SystemErrorCardProps 描述一张系统错误卡片渲染的结构化失败。
export interface SystemErrorCardProps {
  // Parsed provider failure shown in the transcript.
  // 转录区中展示的已解析 Provider 失败信息。
  error: SystemProviderError;
}

// ProviderRetryFailurePage describes the parsed diagnostic for one physical attempt in a grouped card.
// ProviderRetryFailurePage 描述合并卡中一次物理请求的解析后诊断。
interface ProviderRetryFailurePage {
  // Attempt number within the original retry sequence, when the first attempt is loaded.
  // 已加载首次尝试时，表示原始重试序列中的尝试编号。
  attempt: number | null;
  // Complete diagnostic for the displayed attempt.
  // 当前展示尝试的完整诊断。
  error: SystemProviderError;
}

// readProviderRetryFailurePages reads the branch projection's page metadata and returns parsed error pages.
// readProviderRetryFailurePages 读取分支投影的分页元数据并返回解析后的错误页。
export function readProviderRetryFailurePages(metadata: unknown): ProviderRetryFailurePage[] | null {
  // Only a grouped failure has this presentation field, and the regular card handles single failures.
  // 只有合并失败具有该展示字段，单次失败由常规卡片处理。
  const entries = isRecord(metadata) ? metadata.provider_retry_failure_pages : undefined;
  if (!Array.isArray(entries) || entries.length < 2) return null;
  // All attempts must parse so paging cannot silently discard a diagnostic.
  // 每次尝试都必须可解析，避免翻页时静默丢弃诊断。
  const pages: ProviderRetryFailurePage[] = [];
  for (const entry of entries) {
    if (!isRecord(entry) || (entry.attempt !== null && typeof entry.attempt !== "number")
      || typeof entry.body !== "string") return null;
    const error = parseSystemProviderError(entry.body);
    if (!error) return null;
    pages.push({ attempt: entry.attempt, error });
  }
  return pages;
}

// ProviderRetryErrorCard receives the grouped attempts and renders one diagnostic with a pager above it.
// ProviderRetryErrorCard 接收合并后的尝试列表，并在单条诊断上方渲染分页器。
export function ProviderRetryErrorCard(props: { pages: readonly ProviderRetryFailurePage[] }) {
  // Localize the attempt label and both paging actions.
  // 本地化尝试标签和两个翻页操作。
  const { t } = useI18n();
  // Null follows the latest failure while an explicit page keeps earlier diagnostics readable during retries.
  // 空值表示跟随最新失败，显式页号让重试期间仍可阅读先前诊断。
  const [selectedIndex, setSelectedIndex] = React.useState<number | null>(null);
  // The newest failure is selected by default.
  // 默认选中最新一次失败。
  const lastIndex = props.pages.length - 1;
  // A selection outside the loaded group returns to its newest failure.
  // 所选尝试超出已加载的组时回到该组最新失败。
  const index = Math.min(selectedIndex ?? lastIndex, lastIndex);
  // Both the card and its report action receive the exact diagnostic for this page.
  // 卡片及其反馈操作都接收当前页的精确诊断。
  const page = props.pages[index];
  // A truncated history can show page position without assigning an unverified request number.
  // 历史被截断时可显示页码，避免赋予未经证实的请求编号。
  const label = page.attempt === null ? `${index + 1} / ${props.pages.length}`
    : t("conversation.providerRetry.attempt").replace("{attempt}", String(page.attempt));
  return (
    <div className="vk-error-retry-group">
      <div className="vk-memory-pager vk-error-retry-pager">
        <Button aria-label={t("conversation.providerRetry.previousAttempt")} disabled={index === 0}
          onClick={() => setSelectedIndex(index - 1)} size="icon-xs" type="button" variant="ghost">
          <ChevronLeft aria-hidden="true" size={12} />
        </Button>
        <span>{label}</span>
        <Button aria-label={t("conversation.providerRetry.nextAttempt")} disabled={index === lastIndex}
          onClick={() => setSelectedIndex(index + 1 === lastIndex ? null : index + 1)} size="icon-xs" type="button" variant="ghost">
          <ChevronRight aria-hidden="true" size={12} />
        </Button>
      </div>
      <SystemErrorCard error={page.error} />
    </div>
  );
}

// Registered prefixes cover the runtime provider failure format.
// 已登记前缀覆盖运行时 Provider 失败格式。
const SYSTEM_PROVIDER_ERROR_PREFIXES = [
  "Provider streaming execution failed: provider ",
];

// SYSTEM_PROVIDER_ERROR_FAILURE_MARKER separates the provider id from the JSON failure object.
// SYSTEM_PROVIDER_ERROR_FAILURE_MARKER 分隔 Provider 标识与 JSON 失败对象。
const SYSTEM_PROVIDER_ERROR_FAILURE_MARKER = " failure: ";

// Both runtime formats append this exact route suffix; old records also include Chinese punctuation.
// 两种运行时格式均追加此精确路由后缀；旧记录还含有中文标点。
const SYSTEM_PROVIDER_ERROR_ROUTE_MARKER = " [provider_route: ";

// AGENTSPACE_RECONCILIATION_SUBCATEGORY is assigned only by the route-aware backend boundary.
// AGENTSPACE_RECONCILIATION_SUBCATEGORY 仅由感知路由的后端边界赋值。
const AGENTSPACE_RECONCILIATION_SUBCATEGORY =
  "agentspace_result_reconciliation_required";

// snakeCase normalizes the PascalCase values of legacy text records to the JSON contract spelling.
// snakeCase 把旧文本记录的 PascalCase 取值归一为 JSON 契约写法。
export function snakeCase(value: string): string {
  return value.replace(/([a-z0-9])([A-Z])/g, "$1_$2").toLowerCase();
}

// SYSTEM_PROVIDER_ERROR_FIELD_NAMES lists fields emitted by the legacy text formatter.
// SYSTEM_PROVIDER_ERROR_FIELD_NAMES 列出旧文本格式化器发出的字段。
const SYSTEM_PROVIDER_ERROR_FIELD_NAMES = new Set([
  "category",
  "stage",
  "dispatch_outcome",
  "semantic_output_observed",
  "retryable",
  "subcategory",
  "upstream_code",
  "retry_after_ms",
]);

// isRecord identifies JSON objects before accessing their named properties.
// isRecord 在读取命名属性前确认值确实是 JSON 对象。
function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

// stringValue reads one optional JSON string without coercing unrelated values.
// stringValue 读取可选 JSON 字符串，不把无关类型强制转换成字符串。
function stringValue(value: unknown): string | null {
  return typeof value === "string" && value.trim() ? value.trim() : null;
}

// booleanValue reads one required JSON boolean without accepting textual lookalikes.
// booleanValue 读取必需的 JSON 布尔值，不接受看似布尔值的文本。
function booleanValue(value: unknown): boolean | null {
  return typeof value === "boolean" ? value : null;
}

// scalarText preserves numeric JSON diagnostics in the card's technical field display.
// scalarText 保留数字 JSON 诊断值，供卡片技术字段展示。
function scalarText(value: unknown): string | null {
  return typeof value === "string" || typeof value === "number" ? String(value) : null;
}

// retryAfterMilliseconds reads the normalized retry-after object from the provider contract.
// retryAfterMilliseconds 从 Provider 契约读取归一化的 retry-after 对象。
function retryAfterMilliseconds(value: unknown): string | null {
  return isRecord(value) ? scalarText(value.delay_milliseconds) : null;
}

// retryAfterDisplay renders the exact retry delay as seconds; non-numeric text keeps its raw millisecond form instead of becoming zero.
// retryAfterDisplay 将精确重试等待时长渲染为秒；非数值文本保留原始毫秒形式，不会显示为零。
function retryAfterDisplay(retryAfterMs: string): string {
  const milliseconds = Number(retryAfterMs);
  return Number.isFinite(milliseconds) ? formatDurationSeconds(milliseconds) : `${retryAfterMs} ms`;
}

// structuredProviderDisplayMessage derives a non-empty summary only from normalized failure facts.
// structuredProviderDisplayMessage 仅从规范化失败事实派生非空摘要。
function structuredProviderDisplayMessage(
  structured: Record<string, unknown>,
  category: string,
  stage: string,
): string {
  const explicitMessage = stringValue(structured.message)
    ?? stringValue(structured.upstream_message);
  if (explicitMessage) {
    return explicitMessage;
  }
  const httpStatus = scalarText(structured.http_status);
  return [httpStatus ? `HTTP ${httpStatus}` : null, category, stage]
    .filter((value): value is string => Boolean(value))
    .join(" · ");
}

// providerFailurePayload removes only the registered route suffix before parsing the canonical JSON object.
// providerFailurePayload 仅在解析规范 JSON 对象前移除已登记的路由后缀。
function providerFailurePayload(structuredBody: string): string {
  if (!structuredBody.endsWith("]")) {
    return structuredBody;
  }
  // Use the last actual suffix so a marker quoted inside upstream JSON cannot truncate that JSON.
  // 使用最后一个实际后缀，避免上游 JSON 中引用的标记截断该 JSON。
  const routeMarkerIndex = structuredBody.lastIndexOf(SYSTEM_PROVIDER_ERROR_ROUTE_MARKER);
  if (routeMarkerIndex < 0) return structuredBody;
  // Legacy wrappers add this punctuation outside the JSON object, immediately before the suffix.
  // 旧外壳将此标点添加在 JSON 对象外、路由后缀之前。
  const payload = structuredBody.slice(0, routeMarkerIndex).trim();
  return payload.endsWith("。") ? payload.slice(0, -1).trim() : payload;
}

// buildStructuredError creates the shared card model from the exact JSON object.
// buildStructuredError 根据精确 JSON 对象创建卡片共用模型。
function buildStructuredError(
  raw: string,
  provider: string,
  structured: Record<string, unknown>,
): SystemProviderError | null {
  const category = stringValue(structured.category);
  const stage = stringValue(structured.stage);
  const dispatchOutcome = stringValue(structured.dispatch_outcome);
  const semanticOutputObserved = booleanValue(structured.semantic_output_observed);
  const retryable = booleanValue(structured.retryable);
  if (!category || !stage || !dispatchOutcome || semanticOutputObserved === null || retryable === null) {
    return null;
  }
  // message remains present even when an upstream 5xx response supplied empty text fields.
  // message 即使在上游 5xx 返回空文本字段时也保持可展示。
  const message = structuredProviderDisplayMessage(structured, category, stage);

  return {
    raw,
    provider,
    category,
    stage,
    dispatchOutcome,
    semanticOutputObserved,
    retryable,
    sideEffectObserved: booleanValue(structured.side_effect_observed) ?? false,
    terminal: booleanValue(structured.terminal) ?? true,
    httpStatus: scalarText(structured.http_status),
    subcategory: stringValue(structured.subcategory),
    upstreamCode: stringValue(structured.upstream_code),
    upstreamType: stringValue(structured.upstream_type),
    upstreamParameter: stringValue(structured.upstream_parameter),
    upstreamMessage: stringValue(structured.upstream_message),
    requestId: stringValue(structured.request_id),
    entryId: stringValue(structured.entry_id),
    modelId: stringValue(structured.model_id),
    retryAfterMs: retryAfterMilliseconds(structured.retry_after),
    message,
    structured,
  };
}

// buildLegacyStructuredError keeps already persisted text-format failures inspectable after the JSON migration.
// buildLegacyStructuredError 让 JSON 迁移前已持久化的文本失败仍然可查看。
function buildLegacyStructuredError(
  raw: string,
  provider: string,
  values: Record<string, string>,
  message: string,
): SystemProviderError | null {
  const semanticOutputObserved = values.semantic_output_observed === "true"
    ? true
    : values.semantic_output_observed === "false"
      ? false
      : null;
  const retryable = values.retryable === "true"
    ? true
    : values.retryable === "false"
      ? false
      : null;
  const category = values.category?.trim();
  const stage = values.stage?.trim();
  const dispatchOutcome = values.dispatch_outcome?.trim();
  if (!category || !stage || !dispatchOutcome || semanticOutputObserved === null || retryable === null || !message) {
    return null;
  }

  return buildStructuredError(raw, provider, {
    provider_id: provider,
    category,
    stage,
    dispatch_outcome: dispatchOutcome,
    semantic_output_observed: semanticOutputObserved,
    retryable,
    ...(values.http_status ? { http_status: Number(values.http_status) } : {}),
    ...(values.subcategory ? { subcategory: values.subcategory } : {}),
    ...(values.upstream_code ? { upstream_code: values.upstream_code } : {}),
    ...(values.retry_after_ms
      ? { retry_after: { delay_milliseconds: Number(values.retry_after_ms) } }
      : {}),
    message,
  });
}

// parseSystemProviderError parses one canonical provider failure body into card-ready fields.
// parseSystemProviderError 将一条规范 Provider 失败正文解析为卡片所需字段。
//
// Parameters:
// - body is the persisted system-error transcript body.
// 参数：
// - body 是持久化的系统错误转录正文。
//
// Returns:
// A structured provider error, or null when the body is not this exact failure format.
// 返回值：
// 结构化 Provider 错误；如果正文不符合该精确失败格式则返回 null。
export function parseSystemProviderError(body: string): SystemProviderError | null {
  const normalizedBody = body.trim();
  // Prefix selects an explicitly supported persisted format, not a fuzzy match.
  // prefix 选择显式支持的持久化格式，不进行模糊匹配。
  const prefix = SYSTEM_PROVIDER_ERROR_PREFIXES.find((candidate) => normalizedBody.startsWith(candidate));
  if (!prefix) {
    return null;
  }

  const failureMarkerIndex = normalizedBody.indexOf(
    SYSTEM_PROVIDER_ERROR_FAILURE_MARKER,
    prefix.length,
  );
  if (failureMarkerIndex <= prefix.length) {
    return null;
  }

  const provider = normalizedBody
    .slice(prefix.length, failureMarkerIndex)
    .trim();
  if (!provider) {
    return null;
  }

  const structuredBody = normalizedBody.slice(
    failureMarkerIndex + SYSTEM_PROVIDER_ERROR_FAILURE_MARKER.length,
  ).trim();
  // structuredPayload excludes the separately formatted route context while raw retains it for inspection.
  // structuredPayload 排除独立格式化的路由上下文，raw 仍为技术查看保留完整内容。
  const structuredPayload = providerFailurePayload(structuredBody);
  try {
    const parsed = JSON.parse(structuredPayload) as unknown;
    if (isRecord(parsed)) {
      const structuredError = buildStructuredError(normalizedBody, provider, parsed);
      if (structuredError) {
        return structuredError;
      }
    }
  } catch {
    // Legacy persisted failures are handled by the exact delimiter parser below.
    // 旧版持久化失败由下方的精确分隔符解析器处理。
  }

  const values: Record<string, string> = {};
  const messageParts: string[] = [];
  for (const rawPart of structuredPayload.split(" | ")) {
    const part = rawPart.trim();
    const separatorIndex = part.indexOf("=");
    const fieldName = separatorIndex > 0 ? part.slice(0, separatorIndex).trim() : "";
    if (SYSTEM_PROVIDER_ERROR_FIELD_NAMES.has(fieldName)) {
      values[fieldName] = part.slice(separatorIndex + 1).trim();
      continue;
    }
    if (/^HTTP \d+$/.test(part)) {
      values.http_status = part.slice("HTTP ".length);
      continue;
    }
    if (part) {
      messageParts.push(part);
    }
  }

  return buildLegacyStructuredError(normalizedBody, provider, values, messageParts.join(" | ").trim());
}

// parseCompactionProviderError extracts a structured failure from the compaction producer's wrapper.
// parseCompactionProviderError 从压缩生产端的固定外壳提取结构化失败。
// Given a compaction diagnostic, returns the original Provider facts or null for local failures.
// 输入压缩诊断，返回原始供应商事实。本地失败返回 null。
export function parseCompactionProviderError(diagnostic: string): SystemProviderError | null {
  // Match the full persisted wrapper so provider diagnostics quoted in unrelated messages stay plain text.
  // 匹配完整持久化外壳，使无关消息中引用的供应商诊断仍保持纯文本。
  const wrapper = /^Prompt compaction on [^\r\n]+? did not produce a usable summary: (.+)\.$/s.exec(diagnostic);
  return wrapper === null ? null : parseSystemProviderError(wrapper[1]);
}

// isProviderResultReconciliationPending returns whether the backend classified this exact attempt as unresolved.
// isProviderResultReconciliationPending 返回后端是否已将当前精确尝试分类为待确认。
export function isProviderResultReconciliationPending(
  error: SystemProviderError,
): boolean {
  return error.provider === "agentspace"
    && error.entryId === "chatgpt-plan"
    && error.upstreamCode === "reconciliation_required"
    && error.subcategory === AGENTSPACE_RECONCILIATION_SUBCATEGORY
    && error.dispatchOutcome === "dispatched"
    && error.terminal
    && !error.retryable;
}

// formatSystemProviderErrorJson returns the expanded JSON Markdown code-block body.
// formatSystemProviderErrorJson 返回展开后的 JSON Markdown 代码块正文。
export function formatSystemProviderErrorJson(error: SystemProviderError): string {
  return JSON.stringify(error.structured, null, 2);
}

// Load error actions when the diagnostic is expanded.
// 在诊断展开时加载错误操作。
const ProviderErrorActions = React.lazy(() => import("./conversation-system-error-actions"));

// SystemErrorCard renders a compact error summary with a metadata disclosure panel.
// SystemErrorCard 渲染紧凑错误摘要与可展开的元数据面板。
export function SystemErrorCard(props: SystemErrorCardProps) {
  const { t } = useI18n();
  // reconciliationPending is derived exclusively from the backend-owned closed classification.
  // reconciliationPending 仅从后端持有的封闭分类派生。
  const reconciliationPending = isProviderResultReconciliationPending(props.error);
  // Track whether the action row is visible inside the diagnostic.
  // 跟踪诊断中的操作行是否可见。
  const [expanded, setExpanded] = React.useState(false);
  const fields: Array<{ label: string; value: string }> = [
    { label: t("conversation.systemError.category"), value: props.error.category },
    { label: t("conversation.systemError.stage"), value: props.error.stage },
    { label: t("conversation.systemError.dispatchOutcome"), value: props.error.dispatchOutcome },
    {
      label: t("conversation.systemError.semanticOutput"),
      value: props.error.semanticOutputObserved
        ? t("conversation.systemError.true")
        : t("conversation.systemError.false"),
    },
    {
      label: t("conversation.systemError.retryable"),
      value: props.error.retryable
        ? t("conversation.systemError.true")
        : t("conversation.systemError.false"),
    },
  ];
  if (props.error.httpStatus) {
    fields.push({ label: t("conversation.systemError.httpStatus"), value: props.error.httpStatus });
  }
  if (props.error.subcategory) {
    fields.push({ label: t("conversation.systemError.subcategory"), value: props.error.subcategory });
  }
  if (props.error.upstreamCode) {
    fields.push({ label: t("conversation.systemError.upstreamCode"), value: props.error.upstreamCode });
  }
  if (props.error.retryAfterMs) {
    fields.push({
      label: t("conversation.systemError.retryAfter"),
      value: retryAfterDisplay(props.error.retryAfterMs),
    });
  }
  if (reconciliationPending) {
    fields.push(
      {
        label: t("conversation.systemError.requestIdentifier"),
        value: props.error.requestId ?? t("conversation.systemError.identifierMissing"),
      },
      {
        label: t("conversation.systemError.requestIdentifierSource"),
        value: props.error.requestId
          ? t("conversation.systemError.upstreamErrorPayload")
          : t("conversation.systemError.identifierMissing"),
      },
      {
        label: t("conversation.systemError.resultConfirmation"),
        value: t("conversation.systemError.unconfirmed"),
      },
      {
        label: t("conversation.systemError.usageConfirmation"),
        value: t("conversation.systemError.unconfirmed"),
      },
    );
  }

  // The card is transcript history that remounts on every conversation switch, so it carries no live-region role
  // (which would also replace the native disclosure semantics of <details>).
  // 该卡片属于转录历史，每次切换会话都会重新挂载，因此不带实时区域角色（该角色还会覆盖 <details> 原生的展开语义）。
  return (
    <details
      aria-label={reconciliationPending
        ? t("conversation.systemError.pendingTitle")
        : t("conversation.systemError.title")}
      className="vk-error-card"
      data-pending={reconciliationPending || undefined}
      onToggle={(event) => setExpanded(event.currentTarget.open)}
    >
      <summary className="vk-error-summary">
        <CircleAlert aria-hidden="true" className="vk-error-icon" />
        <div className="vk-error-message">
          <span>{t("conversation.systemError.message")}</span>
          <p>{props.error.message}</p>
        </div>
        <ChevronRight aria-hidden="true" className="vk-error-chev" />
      </summary>
      <div className="vk-error-details">
        <div className="vk-error-head">
          <strong>
              {reconciliationPending
                ? t("conversation.systemError.pendingTitle")
                : t("conversation.systemError.title")}
          </strong>
          <StatusPill
              status={reconciliationPending ? "pending" : "error"}
              label={reconciliationPending
                ? t("conversation.systemError.pendingStatus")
                : t("conversation.systemError.status")}
          />
        </div>
        {expanded ? (
          <React.Suspense fallback={null}>
            <ProviderErrorActions error={props.error} />
          </React.Suspense>
        ) : null}
        {reconciliationPending ? (
          <p className="vk-error-pending">
            {t("conversation.systemError.pendingNotice")}
          </p>
        ) : null}
        <dl className="vk-facts">
          {fields.map((field) => (
            <React.Fragment key={field.label}>
              <dt>{field.label}</dt>
              <dd>{field.value}</dd>
            </React.Fragment>
          ))}
        </dl>
        <div className="vk-error-raw">
          <span>{t("conversation.systemError.raw")}</span>
          <pre><code className="language-json">{formatSystemProviderErrorJson(props.error)}</code></pre>
        </div>
      </div>
    </details>
  );
}
