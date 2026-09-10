import { ChevronRight, CircleAlert } from "lucide-react";

import { StatusPill } from "@/components/shared/status";
import { useI18n } from "@/i18n";

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

// SYSTEM_PROVIDER_ERROR_PREFIX identifies the canonical public-contract failure prefix.
// SYSTEM_PROVIDER_ERROR_PREFIX 标识公共契约失败使用的规范前缀。
const SYSTEM_PROVIDER_ERROR_PREFIX = "Vulcan 公共契约流式执行失败：provider ";

// SYSTEM_PROVIDER_ERROR_FAILURE_MARKER separates the provider id from the JSON failure object.
// SYSTEM_PROVIDER_ERROR_FAILURE_MARKER 分隔 Provider 标识与 JSON 失败对象。
const SYSTEM_PROVIDER_ERROR_FAILURE_MARKER = " failure: ";

// SYSTEM_PROVIDER_ERROR_ROUTE_MARKER starts the exact diagnostic suffix appended after the JSON failure object.
// SYSTEM_PROVIDER_ERROR_ROUTE_MARKER 标识追加在 JSON 失败对象后的精确路由诊断后缀起点。
const SYSTEM_PROVIDER_ERROR_ROUTE_MARKER = "。 [provider_route: ";

// AGENTSPACE_RECONCILIATION_SUBCATEGORY is assigned only by the route-aware backend boundary.
// AGENTSPACE_RECONCILIATION_SUBCATEGORY 仅由感知路由的后端边界赋值。
const AGENTSPACE_RECONCILIATION_SUBCATEGORY =
  "agentspace_result_reconciliation_required";

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
  const routeMarkerIndex = structuredBody.lastIndexOf(SYSTEM_PROVIDER_ERROR_ROUTE_MARKER);
  if (routeMarkerIndex < 0 || !structuredBody.endsWith("]")) {
    return structuredBody;
  }
  return structuredBody.slice(0, routeMarkerIndex).trim();
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
  if (!normalizedBody.startsWith(SYSTEM_PROVIDER_ERROR_PREFIX)) {
    return null;
  }

  const failureMarkerIndex = normalizedBody.indexOf(
    SYSTEM_PROVIDER_ERROR_FAILURE_MARKER,
    SYSTEM_PROVIDER_ERROR_PREFIX.length,
  );
  if (failureMarkerIndex <= SYSTEM_PROVIDER_ERROR_PREFIX.length) {
    return null;
  }

  const provider = normalizedBody
    .slice(SYSTEM_PROVIDER_ERROR_PREFIX.length, failureMarkerIndex)
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

// SystemErrorCard renders a compact error summary with a metadata disclosure panel.
// SystemErrorCard 渲染紧凑错误摘要与可展开的元数据面板。
export function SystemErrorCard(props: SystemErrorCardProps) {
  const { t } = useI18n();
  // reconciliationPending is derived exclusively from the backend-owned closed classification.
  // reconciliationPending 仅从后端持有的封闭分类派生。
  const reconciliationPending = isProviderResultReconciliationPending(props.error);
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
      value: `${props.error.retryAfterMs} ms`,
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

  return (
    <details
      aria-label={reconciliationPending
        ? t("conversation.systemError.pendingTitle")
        : t("conversation.systemError.title")}
      className={reconciliationPending
        ? "system-error-card system-error-card-pending"
        : "system-error-card"}
      role={reconciliationPending ? "status" : "alert"}
    >
      <summary className="system-error-summary">
        <div className="system-error-message">
          <span>{t("conversation.systemError.message")}</span>
          <p>{props.error.message}</p>
        </div>
        <ChevronRight aria-hidden="true" className="system-error-chevron" size={17} />
      </summary>
      <div className="system-error-details">
        <div className="system-error-header">
          <div className="system-error-title">
            <CircleAlert aria-hidden="true" className="system-error-icon" size={17} />
            <strong>
              {reconciliationPending
                ? t("conversation.systemError.pendingTitle")
                : t("conversation.systemError.title")}
            </strong>
          </div>
          <div className="system-error-actions">
            <StatusPill
              status={reconciliationPending ? "pending" : "error"}
              label={reconciliationPending
                ? t("conversation.systemError.pendingStatus")
                : t("conversation.systemError.status")}
            />
          </div>
        </div>
        {reconciliationPending ? (
          <p className="system-error-reconciliation-notice">
            {t("conversation.systemError.pendingNotice")}
          </p>
        ) : null}
        <dl className="system-error-fields">
          {fields.map((field) => (
            <div className="system-error-field" key={field.label}>
              <dt>{field.label}</dt>
              <dd>{field.value}</dd>
            </div>
          ))}
        </dl>
        <div className="system-error-raw">
          <span>{t("conversation.systemError.raw")}</span>
          <pre><code className="language-json">{formatSystemProviderErrorJson(props.error)}</code></pre>
        </div>
      </div>
    </details>
  );
}
