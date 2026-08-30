import type { TranslationKey } from "@/i18n-messages";
import type {
  ComposerAttachmentSummary,
  ConversationMessage,
  ProviderSummary,
} from "@/types";
import type {
  ConversationRenderItem,
} from "@/components/conversation/conversation-model";
import { projectConversationRenderItems } from "@/components/conversation/conversation-transcript";
import type {
  ConversationEventBody,
  ConversationEventSummary,
  ResourceId,
  StatBucket,
} from "@/service/conversation-event-window-store";
import type { ConversationSessionResourceCatalogV1 } from "@/state/conversation-session-resources-v1";
import { requestConfigChangeKind } from "@/service/conversation-event-display-projection";
import { modelSwitchRouteLabel } from "./conversation-model-switch";

// V1RootInputPresentationMetadata stores the exact generation anchor used by restored edit and fork actions.
// V1RootInputPresentationMetadata 保存恢复后的编辑与分叉操作使用的精确 generation 锚点。
export interface V1RootInputPresentationMetadata {
  // Canonical root-input seq.
  // 规范根输入 seq。
  seq: number;
  // Canonical root-input text.
  // 规范根输入正文。
  text: string;
}

// ConversationBranchPresentationV1 is the complete in-memory input for the restored branch workspace.
// ConversationBranchPresentationV1 是恢复后的分支 workspace 使用的完整纯内存输入。
export interface ConversationBranchPresentationV1 {
  // Synthetic display records projected only from typed V1 events.
  // 仅从强类型 V1 事件投影得到的合成展示记录。
  messages: ConversationMessage[];
  // Restored branch render items built from the synthetic display records.
  // 从合成展示记录构建的恢复后分支渲染项。
  renderItems: ConversationRenderItem[];
}

// ApiStatsBody narrows the terminal request event used by error and duration projection.
// ApiStatsBody 收窄错误与耗时投影使用的终态请求事件。
type ApiStatsBody = Extract<ConversationEventBody, { kind: "api_stats" }>;

// ToolCallBody narrows one canonical V1 tool event.
// ToolCallBody 收窄一条规范 V1 工具事件。
type ToolCallBody = Extract<ConversationEventBody, { kind: "tool_call" }>;

// UserInputBody narrows one canonical V1 user input event.
// UserInputBody 收窄一条规范 V1 用户输入事件。
type UserInputBody = Extract<ConversationEventBody, { kind: "user_input" }>;

// Translation resolves one branch presentation key at render time.
// Translation 在渲染时解析一个分支展示文案键。
type Translation = (key: TranslationKey) => string;

// projectConversationBranchPresentationV1 adapts canonical V1 events to the restored branch presentation model.
// projectConversationBranchPresentationV1 将规范 V1 事件适配为恢复后的分支展示模型。
// Parameters: sessionId owns the event window, events are strictly ordered, resources resolve project ResourceIds, and t localizes branch summaries.
// 参数：sessionId 拥有事件窗口，events 严格有序，resources 解析项目级 ResourceId，t 本地化分支摘要。
// Returns: one pure in-memory presentation that never becomes a persistence or RPC authority.
// 返回值：一份绝不成为持久化或 RPC 权威的纯内存展示结果。
export function projectConversationBranchPresentationV1(
  sessionId: string,
  events: readonly ConversationEventSummary[],
  resources: ConversationSessionResourceCatalogV1,
  t: Translation,
  // Configured display catalog; isolated hosts may have no provider configuration.
  // 已配置的显示目录；隔离宿主可以不带供应商配置。
  providers: readonly ProviderSummary[] = [],
): ConversationBranchPresentationV1 {
  // apiStatsByCall makes terminal request facts available without changing event display order.
  // apiStatsByCall 让终态请求事实可供读取，同时不改变事件显示顺序。
  const apiStatsByCall = new Map<number, ApiStatsBody>();
  // rootInputByTurn binds final assistant actions to the root input of the same logical Turn.
  // rootInputByTurn 将最终回答操作绑定到同一逻辑 Turn 的根输入。
  const rootInputByTurn = new Map<number, V1RootInputPresentationMetadata>();
  for (const summary of events) {
    if (summary.event.kind === "api_stats") {
      apiStatsByCall.set(summary.event.api_call_id, summary.event);
    }
    if (
      summary.turn_id !== null
      && summary.event.kind === "user_input"
      && summary.event.input_source === "root"
    ) {
      rootInputByTurn.set(summary.turn_id, {
        seq: summary.seq,
        text: summary.event.text,
      });
    }
  }

  // messages preserves the canonical event order and only expands one tool event into adjacent call/result display records.
  // messages 保持规范事件顺序，仅把一条工具事件展开为相邻的调用/结果展示记录。
  const messages: ConversationMessage[] = [];
  for (const summary of events) {
    const rootInput = summary.turn_id === null ? undefined : rootInputByTurn.get(summary.turn_id);
    messages.push(...conversationMessagesForV1Event(
      sessionId,
      summary,
      rootInput,
      apiStatsByCall,
      resources,
      providers,
    ));
  }

  return {
    messages,
    renderItems: projectConversationRenderItems(messages, t),
  };
}

// conversationMessagesForV1Event maps one event without consulting legacy messages or records.
// conversationMessagesForV1Event 映射一条事件，且不读取旧 messages 或 records。
function conversationMessagesForV1Event(
  sessionId: string,
  summary: ConversationEventSummary,
  rootInput: V1RootInputPresentationMetadata | undefined,
  apiStatsByCall: ReadonlyMap<number, ApiStatsBody>,
  resources: ConversationSessionResourceCatalogV1,
  providers: readonly ProviderSummary[],
): ConversationMessage[] {
  const base = conversationMessageBase(sessionId, summary, rootInput);
  switch (summary.event.kind) {
    case "user_input":
      return [userInputMessage(base, summary.event, resources)];
    case "assistant_reasoning":
      return [{
        ...base,
        role: "assistant",
        recordKind: "thinking-output",
        title: "Thinking",
        body: summary.event.text,
        metadata: {
          ...presentationMetadata(summary, rootInput),
          api_call_id: summary.event.api_call_id,
          status: summary.event.status,
          stream_state: summary.event.status === "streaming" ? "active" : "sealed",
          duration_ms: apiDurationMs(apiStatsByCall.get(summary.event.api_call_id)),
        },
      }];
    case "assistant_text":
      return [{
        ...base,
        role: "assistant",
        body: summary.event.text,
        metadata: {
          ...presentationMetadata(summary, rootInput),
          api_call_id: summary.event.api_call_id,
          status: summary.event.status,
          stream_state: summary.event.status === "streaming" ? "active" : "sealed",
        },
      }];
    case "tool_call":
      return toolMessages(base, summary, summary.event, rootInput, resources);
    case "turn_stats":
      return [{
        ...base,
        role: "assistant",
        recordKind: "turn-stats",
        title: "Turn statistics",
        body: "Turn statistics",
        metadata: {
          ...presentationMetadata(summary, rootInput),
          turn_stats: turnStatsMetadata(summary.event.buckets.total, summary.event.buckets.subagent, summary.event.duration_ms, summary.event.outcome),
        },
      }];
    case "display_note":
      return [displayNoteMessage(base, summary, summary.event, rootInput, apiStatsByCall)];
    case "request_config_changed": {
      // changeKind is derived from exact persisted fields, never from a display label.
      // changeKind 从精确持久化字段派生，绝不依赖展示文案。
      const changeKind = requestConfigChangeKind(summary.event);
      if (changeKind === "none") {
        return [];
      }
      if (changeKind === "thinking") {
        return [{
          ...base,
          role: "assistant",
          recordKind: "display-note",
          title: "Reasoning effort",
          body: "Reasoning effort changed",
          metadata: {
            ...presentationMetadata(summary, rootInput),
            notice_kind: "reasoning_effort_switch",
            activation: "next_provider_request",
            previous_thinking_level: summary.event.previous.thinking_level,
            current_thinking_level: summary.event.current.thinking_level,
          },
        }];
      }
      return [{
        ...base,
        role: "assistant",
        recordKind: "display-note",
        title: "Model selection",
        body: "Model selection changed",
        metadata: {
          ...presentationMetadata(summary, rootInput),
          notice_kind: "model_switch",
          previous_model_label: modelSwitchRouteLabel(summary.event.previous, providers),
          current_model_label: modelSwitchRouteLabel(summary.event.current, providers),
        },
      }];
    }
    case "api_stats":
      return [];
    case "subagent_return":
      return [{
        ...base,
        role: "assistant",
        title: "Subagent return",
        body: summary.event.content,
        metadata: {
          ...presentationMetadata(summary, rootInput),
          child_session_id: summary.event.child_session_id,
          subagent_task_id: summary.event.child_task_id,
        },
      }];
    case "queued_input_resolution":
    case "system_context":
    case "api_started":
    case "turn_boundary":
    case "subagent_return_stats":
    case "execution_control":
      return [];
  }
}

// conversationMessageBase creates the stable display identity shared by all projections of one event.
// conversationMessageBase 创建一条事件的所有投影共享的稳定展示身份。
function conversationMessageBase(
  sessionId: string,
  summary: ConversationEventSummary,
  rootInput: V1RootInputPresentationMetadata | undefined,
): Pick<ConversationMessage, "id" | "conversationId" | "createdAt" | "metadata"> {
  // displayedCreatedAtMs uses the durable last-update timestamp only for editable user-input facts.
  // displayedCreatedAtMs 仅对可编辑的用户输入事实使用持久化最后更新时间。
  const displayedCreatedAtMs = summary.event.kind === "user_input"
    ? summary.updated_at_ms
    : summary.created_at_ms;
  return {
    id: `v1:${summary.seq}`,
    conversationId: sessionId,
    createdAt: String(displayedCreatedAtMs),
    metadata: presentationMetadata(summary, rootInput),
  };
}

// presentationMetadata stores only explicit V1 identities required by restored interactions.
// presentationMetadata 仅保存恢复后交互所需的显式 V1 身份。
function presentationMetadata(
  summary: ConversationEventSummary,
  rootInput: V1RootInputPresentationMetadata | undefined,
): Record<string, unknown> {
  return {
    v1_event: true,
    v1_seq: summary.seq,
    v1_event_revision: summary.event_revision,
    v1_turn_id: summary.turn_id,
    ...(rootInput ? {
      v1_root_input_seq: rootInput.seq,
      v1_root_input_text: rootInput.text,
    } : {}),
  };
}

// userInputMessage projects one typed user input and its ordered project resources.
// userInputMessage 投影一条强类型用户输入及其有序项目资源。
function userInputMessage(
  base: Pick<ConversationMessage, "id" | "conversationId" | "createdAt" | "metadata">,
  event: UserInputBody,
  resources: ConversationSessionResourceCatalogV1,
): ConversationMessage {
  return {
    ...base,
    role: "user",
    body: event.text,
    editableUserPrompt: event.input_source === "root",
    attachments: (event.resource_ids ?? []).map((resourceIdValue) => (
      composerAttachmentForResource(resourceIdValue, resources)
    )),
    metadata: {
      ...(base.metadata as Record<string, unknown>),
      starts_user_turn: event.input_source !== "pending_in_turn",
      turn_process: event.input_source === "pending_in_turn",
      client_action_id: event.client_action_id,
      input_id: event.input_id,
      input_source: event.input_source,
      resource_ids: event.resource_ids ?? [],
    },
  };
}

// composerAttachmentForResource converts browser-safe V1 resource metadata into the restored attachment strip model.
// composerAttachmentForResource 将浏览器安全的 V1 资源元数据转换为恢复后的附件条模型。
function composerAttachmentForResource(
  currentResourceId: ResourceId,
  resources: ConversationSessionResourceCatalogV1,
): ComposerAttachmentSummary {
  const resource = resources.resources.get(currentResourceId);
  if (!resource || resource.availability === "missing") {
    return {
      kind: "file",
      fileName: resource?.placeholder ?? "[MISSING PROJECT RESOURCE]",
      localPath: "",
      projectResourceId: currentResourceId,
      byteLen: null,
      mimeType: "application/octet-stream",
    };
  }
  return {
    kind: resourceKind(resource.mime),
    fileName: resource.original_name,
    localPath: resources.contentUrl(currentResourceId),
    projectResourceId: currentResourceId,
    byteLen: resource.size,
    mimeType: resource.mime ?? "application/octet-stream",
  };
}

// resourceKind maps one declared MIME type to the established attachment families.
// resourceKind 将一个声明的 MIME 类型映射为既有附件类别。
function resourceKind(mime: string | undefined): string {
  if (mime?.startsWith("image/")) return "image";
  if (mime?.startsWith("audio/")) return "audio";
  if (mime?.startsWith("video/")) return "video";
  if (mime === "application/pdf") return "pdf";
  return "file";
}

// toolMessages expands one same-seq V1 tool state into the adjacent call/result blocks expected by the branch card system.
// toolMessages 将一条同 seq 的 V1 工具状态展开为分支卡片系统需要的相邻调用/结果块。
function toolMessages(
  base: Pick<ConversationMessage, "id" | "conversationId" | "createdAt" | "metadata">,
  summary: ConversationEventSummary,
  event: ToolCallBody,
  rootInput: V1RootInputPresentationMetadata | undefined,
  resources: ConversationSessionResourceCatalogV1,
): ConversationMessage[] {
  // displayOutput is the exact model-facing result retained inside the durable tool envelope.
  // displayOutput 是可靠工具封装中保留的精确模型可见结果。
  const displayOutput = toolDisplayOutput(event);
  const terminal = event.status === "completed"
    || event.status === "failed"
    || event.status === "cancelled"
    || event.status === "stopped"
    || event.status === "interrupted";
  const metadata = toolMetadata(summary, event, rootInput, resources);
  const call: ConversationMessage = {
    ...base,
    id: `${base.id}:call`,
    role: "assistant",
    recordKind: "tool-call-output",
    title: event.tool_name,
    body: displayPayload(event.input),
    metadata,
  };
  if (!terminal) return [call];
  return [
    call,
    {
      ...base,
      id: `${base.id}:result`,
      role: "assistant",
      recordKind: "tool-result-output",
      title: event.tool_name,
      body: displayPayload(displayOutput),
      metadata,
    },
  ];
}

// toolMetadata normalizes typed V1 tool facts into the branch presentation contract without a detail lookup.
// toolMetadata 在不进行详情查询的前提下，把强类型 V1 工具事实规范化为分支展示契约。
function toolMetadata(
  summary: ConversationEventSummary,
  event: ToolCallBody,
  rootInput: V1RootInputPresentationMetadata | undefined,
  resources: ConversationSessionResourceCatalogV1,
): Record<string, unknown> {
  // hostResult preserves the typed producer-owned payload required by specialized branch cards.
  // hostResult 保留专用分支卡片所需的强类型生产者权威载荷。
  const hostResult = toolHostResult(event);
  // displayOutput is shared by the visible result body and generic metadata consumers.
  // displayOutput 由可见结果正文与通用元数据消费者共同使用。
  const displayOutput = toolDisplayOutput(event);
  const terminalRef = event.external_refs?.find((reference) => reference.kind === "terminal");
  const subagentRef = event.external_refs?.find((reference) => reference.kind === "subagent");
  return {
    ...presentationMetadata(summary, rootInput),
    v1_tool_event: event,
    tool_name: event.tool_name,
    tool_call_id: String(event.tool_call_id),
    request_stream_id: event.request_stream_id,
    tool_display_state: toolDisplayState(event.status),
    arguments: event.input,
    result: {
      status: event.status,
      output: displayOutput,
    },
    ...(hostResult ? { host_result: hostResult } : {}),
    ...(terminalRef ? { command_session_id: terminalRef.id } : {}),
    ...(subagentRef ? { subagent_task_id: subagentRef.id } : {}),
    display: {
      outputs: generatedResourceOutputs(event, resources),
    },
  };
}

// toolOutputEnvelope narrows the durable success envelope without probing alternate field names.
// toolOutputEnvelope 收窄可靠成功封装，且不探测其他候选字段名。
function toolOutputEnvelope(event: ToolCallBody): Record<string, unknown> | null {
  return event.output && typeof event.output === "object" && !Array.isArray(event.output)
    ? event.output as Record<string, unknown>
    : null;
}

// toolDisplayOutput selects the exact model output or the strongly typed terminal failure document.
// toolDisplayOutput 选择精确模型输出或强类型工具终态失败文档。
function toolDisplayOutput(event: ToolCallBody): unknown {
  const output = toolOutputEnvelope(event);
  if (output && Object.prototype.hasOwnProperty.call(output, "model_output")) {
    return output.model_output;
  }
  return event.output ?? event.changeset ?? { status: event.status };
}

// toolHostResult returns the canonical V1 ChangeSet or the exact runtime host result from the durable envelope.
// toolHostResult 返回规范 V1 ChangeSet，或可靠封装中的精确运行时宿主结果。
function toolHostResult(event: ToolCallBody): Record<string, unknown> | null {
  if (event.changeset) {
    return { kind: "change_set", payload: event.changeset };
  }
  const output = toolOutputEnvelope(event);
  const hostResult = output?.host_result;
  return hostResult && typeof hostResult === "object" && !Array.isArray(hostResult)
    ? hostResult as Record<string, unknown>
    : null;
}

// generatedResourceOutputs maps typed generated resources to authenticated opaque content URLs for the restored media cards.
// generatedResourceOutputs 将强类型生成资源映射为恢复后媒体卡使用的鉴权不透明内容 URL。
function generatedResourceOutputs(
  event: ToolCallBody,
  resources: ConversationSessionResourceCatalogV1,
): Record<string, unknown>[] {
  return (event.generated_resources ?? []).map((reference) => {
    const resource = resources.resources.get(reference.resource_id);
    return {
      kind: reference.kind === "pdf" || reference.kind === "file" ? "document" : reference.kind,
      file_name: resource?.availability === "available" ? resource.original_name : "[MISSING PROJECT RESOURCE]",
      local_path: resource?.availability === "available" ? resources.contentUrl(reference.resource_id) : "",
      mime_type: resource?.availability === "available" ? resource.mime : undefined,
      resource_id: reference.resource_id,
    };
  });
}

// toolDisplayState maps the V1 lifecycle to the branch card lifecycle without inferring from text.
// toolDisplayState 在不从正文推断的情况下，把 V1 生命周期映射为分支卡片生命周期。
function toolDisplayState(status: ToolCallBody["status"]): string {
  if (status === "completed") return "completed";
  if (status === "failed") return "failed";
  if (status === "cancelled" || status === "stopped" || status === "interrupted") return "stopped";
  return "running";
}

// displayNoteMessage projects durable display facts through the restored notice and error-card suite.
// displayNoteMessage 通过恢复后的提示与错误卡套件投影可靠显示事实。
function displayNoteMessage(
  base: Pick<ConversationMessage, "id" | "conversationId" | "createdAt" | "metadata">,
  summary: ConversationEventSummary,
  event: Extract<ConversationEventBody, { kind: "display_note" }>,
  rootInput: V1RootInputPresentationMetadata | undefined,
  apiStatsByCall: ReadonlyMap<number, ApiStatsBody>,
): ConversationMessage {
  const apiStats = event.api_call_id === undefined ? undefined : apiStatsByCall.get(event.api_call_id);
  if (event.severity === "error") {
    const provider = apiStats?.provider ?? "unknown";
    const error = apiStats?.error;
    const payload = {
      category: error?.category ?? event.notice_kind,
      stage: "terminal",
      dispatch_outcome: apiStats?.status === "prepared" ? "not_dispatched" : "dispatched",
      semantic_output_observed: false,
      side_effect_observed: false,
      terminal: true,
      retryable: apiStats?.continuation_eligibility === "allowed",
      upstream_code: error?.error_code ?? event.reason_code,
      upstream_message: event.redacted_detail ?? error?.message ?? "",
      provider_id: provider,
      model_id: apiStats?.model ?? "",
      message: event.redacted_detail ?? error?.message ?? event.reason_code,
    };
    return {
      ...base,
      role: "system",
      recordKind: "transcript",
      body: `Vulcan 公共契约流式执行失败：provider ${provider} failure: ${JSON.stringify(payload)}`,
      metadata: presentationMetadata(summary, rootInput),
    };
  }
  return {
    ...base,
    role: "assistant",
    recordKind: "display-note",
    title: event.notice_kind,
    body: event.redacted_detail ?? event.reason_code,
    metadata: {
      ...presentationMetadata(summary, rootInput),
      notice_kind: event.notice_kind === "retry_started" || event.notice_kind === "retry_exhausted"
        ? "provider_retry"
        : event.notice_kind,
      provider_retry_state: event.notice_kind === "retry_started"
        ? "retrying"
        : event.notice_kind === "retry_exhausted" ? "failed" : undefined,
      retry_count: event.retry_count,
      retry_limit: event.retry_limit,
      reason_code: event.reason_code,
      summary: event.redacted_detail ?? event.reason_code,
      status: event.severity,
    },
  };
}

// turnStatsMetadata maps one typed bucket set to the branch tooltip's established field contract.
// turnStatsMetadata 将一组强类型统计桶映射为分支悬浮卡的既有字段契约。
function turnStatsMetadata(
  total: StatBucket,
  subagent: StatBucket,
  durationMs: number | undefined,
  outcome: "completed" | "abandoned" | "stopped",
): Record<string, unknown> {
  return {
    turn_kind: "user_turn",
    status: outcome,
    duration_ms: durationMs,
    provider_call_count: total.api_call_count,
    unknown_usage_api_call_count: total.unknown_usage_api_call_count,
    tool_call_count: total.tool_call_count,
    subagent_return_count: total.subagent_return_count,
    ...usageFields(total),
    usage_groups: subagent.api_call_count > 0 || subagent.tool_call_count > 0 || subagent.subagent_return_count > 0
      ? [{
        kind: "subagent",
        provider_call_count: subagent.api_call_count,
        unknown_usage_api_call_count: subagent.unknown_usage_api_call_count,
        tool_call_count: subagent.tool_call_count,
        subagent_return_count: subagent.subagent_return_count,
        ...usageFields(subagent),
      }]
      : [],
  };
}

// usageFields preserves unknown token dimensions by omitting them from the display metadata.
// usageFields 通过省略未知 token 维度来保留其未知语义。
function usageFields(bucket: StatBucket): Record<string, unknown> {
  return {
    ...(bucket.usage.input_tokens === undefined ? {} : { input_tokens: bucket.usage.input_tokens }),
    ...(bucket.usage.uncached_input_tokens === undefined ? {} : { uncached_input_tokens: bucket.usage.uncached_input_tokens }),
    ...(bucket.usage.output_tokens === undefined ? {} : { output_tokens: bucket.usage.output_tokens }),
    ...(bucket.usage.reasoning_tokens === undefined ? {} : { reasoning_tokens: bucket.usage.reasoning_tokens }),
    ...(bucket.usage.total_tokens === undefined ? {} : { total_tokens: bucket.usage.total_tokens }),
    ...(bucket.usage.cache_read_tokens === undefined ? {} : { cached_input_tokens: bucket.usage.cache_read_tokens }),
    ...(bucket.usage.cache_write_tokens === undefined ? {} : { cache_creation_input_tokens: bucket.usage.cache_write_tokens }),
  };
}

// apiDurationMs derives one display-only duration from persisted request timing checkpoints.
// apiDurationMs 从持久化请求时间点派生一个纯展示耗时。
function apiDurationMs(stats: ApiStatsBody | undefined): number | undefined {
  const start = stats?.timing.first_event_at_ms ?? stats?.timing.dispatched_at_ms ?? stats?.timing.prepared_at_ms;
  const end = stats?.timing.completed_at_ms;
  return start === undefined || end === undefined ? undefined : Math.max(0, end - start);
}

// displayPayload serializes one typed tool payload without text parsing or fallback field probing.
// displayPayload 序列化一个强类型工具载荷，不进行正文解析或后备字段探测。
function displayPayload(payload: unknown): string {
  if (typeof payload === "string") return payload;
  return JSON.stringify(payload, null, 2);
}

// rootInputPresentationMetadata reads the exact adapter marker from one restored display record.
// rootInputPresentationMetadata 从一条恢复后的展示记录读取精确适配标记。
export function rootInputPresentationMetadata(
  message: ConversationMessage | undefined,
): V1RootInputPresentationMetadata | null {
  if (!message?.metadata || typeof message.metadata !== "object" || Array.isArray(message.metadata)) {
    return null;
  }
  const metadata = message.metadata as Record<string, unknown>;
  return Number.isSafeInteger(metadata.v1_root_input_seq) && typeof metadata.v1_root_input_text === "string"
    ? {
      seq: metadata.v1_root_input_seq as number,
      text: metadata.v1_root_input_text,
    }
    : null;
}

// completedTurnEndSeqForPresentation resolves the terminal TurnStats seq for one exact displayed Turn.
// completedTurnEndSeqForPresentation 为一个精确展示回合解析终态 TurnStats seq。
// Parameters: messages are canonical V1 projections and sourceMessage is the Turn-end action owner.
// 参数：messages 是规范 V1 投影，sourceMessage 是回合末尾操作的拥有者。
// Returns: the exact terminal seq, or null when the Turn has not completed.
// 返回值：精确终态 seq；回合尚未完成时返回 null。
export function completedTurnEndSeqForPresentation(
  messages: readonly ConversationMessage[],
  sourceMessage: ConversationMessage | undefined,
): number | null {
  if (
    !sourceMessage?.metadata
    || typeof sourceMessage.metadata !== "object"
    || Array.isArray(sourceMessage.metadata)
  ) {
    return null;
  }
  const sourceMetadata = sourceMessage.metadata as Record<string, unknown>;
  if (!Number.isSafeInteger(sourceMetadata.v1_turn_id)) return null;
  const turnStatsMessage = messages.find((message) => {
    if (
      message.recordKind !== "turn-stats"
      || !message.metadata
      || typeof message.metadata !== "object"
      || Array.isArray(message.metadata)
    ) {
      return false;
    }
    const metadata = message.metadata as Record<string, unknown>;
    return metadata.v1_turn_id === sourceMetadata.v1_turn_id;
  });
  if (
    !turnStatsMessage?.metadata
    || typeof turnStatsMessage.metadata !== "object"
    || Array.isArray(turnStatsMessage.metadata)
  ) {
    return null;
  }
  const turnStatsMetadata = turnStatsMessage.metadata as Record<string, unknown>;
  return Number.isSafeInteger(turnStatsMetadata.v1_seq)
    ? turnStatsMetadata.v1_seq as number
    : null;
}
