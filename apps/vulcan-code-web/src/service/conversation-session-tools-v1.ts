import type { ConversationEventWindowControllerSnapshot } from "@/service/conversation-event-window-controller";
import type { ConversationEventSummary, ToolJsonValue } from "@/service/conversation-event-window-store";
import type { SessionTodolistItemSummary, SessionTodolistSummary } from "@/types/planning-questions";
import type { CommandSessionSummary } from "@/types/runtime-context";

/** Exact object-shaped JSON value accepted by the V1 tool-result selectors.
 * V1 工具结果选择器接受的精确对象形 JSON 值。 */
type ToolJsonObject = { [key: string]: ToolJsonValue };

/** Selects the latest durable model-authored todolist from the selected V1 event window.
 * 从选中的 V1 事件窗口选择最近一份可靠的模型待办清单。
 *
 * Parameters: snapshot is the atomic selected-session V1 event window.
 * 参数：snapshot 是选中会话的原子 V1 事件窗口。
 * Returns: the latest exact todolist summary, or null when no valid latest update exists.
 * 返回值：最近一份精确待办摘要；不存在有效的最近更新时返回 null。
 */
export function selectConversationSessionTodolistV1(
  snapshot: ConversationEventWindowControllerSnapshot,
): SessionTodolistSummary | null {
  if (!snapshot.owner) {
    return null;
  }
  let latest: ConversationEventSummary | null = null;
  for (const summary of snapshot.events) {
    if (
      summary.event.kind === "tool_call"
      && summary.event.tool_name === "todolist"
      && summary.event.status === "completed"
    ) {
      latest = summary;
    }
  }
  if (!latest || latest.event.kind !== "tool_call") {
    return null;
  }
  const hostResult = exactToolHostResult(latest);
  if (!hostResult || hostResult.kind !== "todolist") {
    return null;
  }
  const payload = objectValue(hostResult.payload);
  const rawItems = payload?.items;
  if (!payload || !Array.isArray(rawItems) || rawItems.length === 0) {
    return null;
  }
  const items: SessionTodolistItemSummary[] = [];
  let inProgressCount = 0;
  for (const rawItem of rawItems) {
    const item = todolistItem(rawItem);
    if (!item) {
      return null;
    }
    if (item.status === "in_progress") {
      inProgressCount += 1;
    }
    items.push(item);
  }
  if (inProgressCount > 1) {
    return null;
  }
  const explanation = payload.explanation;
  if (explanation !== undefined && explanation !== null && typeof explanation !== "string") {
    return null;
  }
  return {
    explanation: explanation ?? null,
    items,
    completedCount: items.filter((item) => item.status === "completed").length,
    totalCount: items.length,
    updatedAt: String(latest.updated_at_ms),
  };
}

/** Selects the latest durable terminal snapshot for every terminal referenced by the V1 session.
 * 为 V1 会话引用的每个终端选择最近一份可靠终端快照。
 *
 * Parameters: snapshot is the atomic selected-session V1 event window.
 * 参数：snapshot 是选中会话的原子 V1 事件窗口。
 * Returns: terminal summaries in first-reference order with latest exact payload values.
 * 返回值：按首次引用顺序排列、使用最近精确载荷值的终端摘要。
 */
export function selectConversationSessionTerminalsV1(
  snapshot: ConversationEventWindowControllerSnapshot,
): CommandSessionSummary[] {
  if (!snapshot.owner) {
    return [];
  }
  const orderedIds: string[] = [];
  const latestById = new Map<string, CommandSessionSummary | null>();
  for (const summary of snapshot.events) {
    if (
      summary.event.kind !== "tool_call"
      || summary.event.tool_name !== "terminal_session"
      || summary.event.status !== "completed"
    ) {
      continue;
    }
    const terminalReference = summary.event.external_refs?.find((reference) => reference.kind === "terminal");
    if (!terminalReference) {
      continue;
    }
    if (!latestById.has(terminalReference.id)) {
      orderedIds.push(terminalReference.id);
    }
    // A malformed newer payload invalidates the older snapshot instead of reviving stale terminal state.
    // 格式错误的较新载荷会使旧快照失效，而不是让过期终端状态重新出现。
    latestById.set(
      terminalReference.id,
      terminalSummary(summary, terminalReference.id, snapshot.owner.session_id),
    );
  }
  return orderedIds.flatMap((id) => {
    const summary = latestById.get(id);
    return summary ? [summary] : [];
  });
}

/** Selects currently running terminals by intersecting durable V1 identities with live runtime facts.
 * 通过求可靠 V1 身份与实时运行态事实的交集，选择当前仍在运行的终端。
 *
 * Parameters: durable contains V1 terminal snapshots and live contains runtime summaries.
 * 参数：durable 包含 V1 终端快照，live 包含运行态摘要。
 * Returns: only matching, owner-consistent, currently running terminals in durable first-reference order.
 * 返回值：仅返回身份匹配、归属一致且当前运行中的终端，并保留可靠首次引用顺序。
 */
export function mergeConversationSessionTerminalsV1(
  durable: readonly CommandSessionSummary[],
  live: readonly CommandSessionSummary[],
): CommandSessionSummary[] {
  const liveById = new Map(live.map((summary) => [summary.id, summary]));
  return durable.flatMap((summary) => {
    const liveSummary = liveById.get(summary.id);
    if (
      !liveSummary
      || liveSummary.status !== "running"
      || Boolean(liveSummary.lost)
      || liveSummary.ownerSessionId !== summary.ownerSessionId
    ) {
      return [];
    }
    return [liveSummary];
  });
}

/** Extracts the exact host-result envelope persisted by one terminal tool event.
 * 提取一条终端工具事件持久化的精确宿主结果封装。
 *
 * Parameters: summary is one V1 event summary.
 * 参数：summary 是一条 V1 事件摘要。
 * Returns: the exact host-result object, or null when the envelope is absent or malformed.
 * 返回值：精确宿主结果对象；封装缺失或格式错误时返回 null。
 */
function exactToolHostResult(summary: ConversationEventSummary): ToolJsonObject | null {
  if (summary.event.kind !== "tool_call") {
    return null;
  }
  const output = objectValue(summary.event.output);
  return objectValue(output?.host_result);
}

/** Builds one strict todolist item from a native-tool host payload item.
 * 从原生工具宿主载荷条目构建一个严格待办条目。
 *
 * Parameters: value is one untrusted JSON item from the persisted host payload.
 * 参数：value 是持久宿主载荷中的一个未信任 JSON 条目。
 * Returns: the typed item, or null when any required field violates the contract.
 * 返回值：类型化条目；任一必填字段违反契约时返回 null。
 */
function todolistItem(value: ToolJsonValue): SessionTodolistItemSummary | null {
  const item = objectValue(value);
  const step = item?.step;
  const status = item?.status;
  if (typeof step !== "string" || step.trim().length === 0 || step.trim() !== step) {
    return null;
  }
  if (status !== "pending" && status !== "in_progress" && status !== "completed") {
    return null;
  }
  return { step, status };
}

/** Builds one strict terminal summary from a durable V1 terminal host payload.
 * 从可靠 V1 终端宿主载荷构建一个严格终端摘要。
 *
 * Parameters: summary is the source event, terminalId is its typed external reference, and ownerSessionId is the selected V1 owner.
 * 参数：summary 是来源事件，terminalId 是其强类型外部引用，ownerSessionId 是选中的 V1 归属。
 * Returns: the terminal summary, or null when source identities or required fields disagree.
 * 返回值：终端摘要；来源身份或必填字段不一致时返回 null。
 */
function terminalSummary(
  summary: ConversationEventSummary,
  terminalId: string,
  ownerSessionId: string,
): CommandSessionSummary | null {
  const hostResult = exactToolHostResult(summary);
  if (!hostResult || hostResult.kind !== "terminal_session") {
    return null;
  }
  const payload = objectValue(hostResult.payload);
  if (
    !payload
    || payload.terminal_session_id !== terminalId
    || payload.owner_session_id !== ownerSessionId
    || typeof payload.command !== "string"
    || typeof payload.status !== "string"
    || typeof payload.name !== "string"
    || typeof payload.cwd !== "string"
    || typeof payload.shell !== "string"
    || typeof payload.backend !== "string"
  ) {
    return null;
  }
  const screenSize = objectValue(payload.screen_size);
  if (
    !screenSize
    || !isNullableFiniteNumber(payload.screen_frame_seq)
    || !isNullableFiniteNumber(screenSize.rows)
    || !isNullableFiniteNumber(screenSize.cols)
    || !isNullableString(payload.screen_text)
    || !isNullableFiniteNumber(payload.exit_code)
    || !isNullableString(payload.failure_reason)
  ) {
    return null;
  }
  return {
    id: terminalId,
    name: payload.name,
    command: payload.command,
    status: payload.status,
    preview: payload.screen_text ?? "",
    ownerSessionId,
    cwd: payload.cwd,
    shellLabel: payload.shell,
    backend: payload.backend,
    screenFrameSeq: payload.screen_frame_seq,
    screenRows: screenSize.rows,
    screenCols: screenSize.cols,
    screenText: payload.screen_text,
    exitCode: payload.exit_code,
    failureReason: payload.failure_reason,
    lost: false,
  };
}

/** Narrows an unknown tool JSON value to its object form.
 * 将未知工具 JSON 值收窄为对象形态。
 *
 * Parameters: value is one optional JSON value.
 * 参数：value 是一个可选 JSON 值。
 * Returns: the object value, or null for arrays, primitives, and absence.
 * 返回值：对象值；数组、基础类型与缺失值返回 null。
 */
function objectValue(value: ToolJsonValue | undefined): ToolJsonObject | null {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? value
    : null;
}

/** Reports whether one required JSON field is a nullable string.
 * 判断一个必填 JSON 字段是否为可空字符串。
 *
 * Parameters: value is one required JSON field from the persisted host payload.
 * 参数：value 是持久化宿主载荷中的一个必填 JSON 字段。
 * Returns: true only for string or null; absence and every other type are rejected.
 * 返回值：仅字符串或 null 返回 true；缺失与所有其他类型均被拒绝。
 */
function isNullableString(value: ToolJsonValue | undefined): value is string | null {
  return value === null || typeof value === "string";
}

/** Reports whether one required JSON field is a nullable finite number.
 * 判断一个必填 JSON 字段是否为可空有限数值。
 *
 * Parameters: value is one required JSON field from the persisted host payload.
 * 参数：value 是持久化宿主载荷中的一个必填 JSON 字段。
 * Returns: true only for a finite number or null; absence and every other type are rejected.
 * 返回值：仅有限数值或 null 返回 true；缺失与所有其他类型均被拒绝。
 */
function isNullableFiniteNumber(value: ToolJsonValue | undefined): value is number | null {
  return value === null || (typeof value === "number" && Number.isFinite(value));
}
