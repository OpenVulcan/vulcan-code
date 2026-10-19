// Reads a tool request the model is still writing into the same ToolCall the finished call will become, so the
// card that will show the result already shows the request taking shape: the path, the lines being replaced,
// the plan being written, or at least how much of the arguments has arrived.
// 将模型仍在书写的工具请求读取为与最终调用相同的 ToolCall，使将来展示结果的卡片已能展示请求的成形过程：
// 路径、正被替换的行、正在书写的计划，或至少已到达的参数量。
import type { ToolCallEvent } from "@/service/conversation-event-display-projection";
import { canonicalToolName, listOf, numberOf, objectOf, textOf, type ToolCall, type ToolCardState } from "./tool-call";
import type { PlainDiffRow } from "./views/diff-view";

// ToolRequestPhase is how far the model has written the arguments.
// ToolRequestPhase 表示模型已将参数写到哪一步。
export type ToolRequestPhase = "receiving" | "arguments_assembled" | "ready";

// RequestPreviewFile is one file a request preview touches, with the lines it shows so far.
// RequestPreviewFile 是请求预览涉及的一个文件及其目前展示的行。
export interface RequestPreviewFile {
  // Path as the model wrote it.
  // 模型书写的路径。
  path: string;
  // Lines shown so far, in arrival order.
  // 按到达顺序目前展示的行。
  rows: PlainDiffRow[];
}

// RequestPreview is the host's sanitized view of arguments still being written.
// RequestPreview 是宿主对仍在书写的参数给出的净化视图。
export interface RequestPreview {
  // Preview kind, named after the tool that produced it.
  // 预览类型，以产生它的工具命名。
  kind: string;
  // Fields that name what the request targets.
  // 指明请求目标的字段。
  title: Record<string, unknown>;
  // Running counters such as line counts.
  // 行数等运行计数。
  progress: Record<string, unknown>;
  // Bounded text body, when the tool previews one.
  // 工具预览的有界正文。
  body: string | null;
  // Files with their lines so far.
  // 文件及其目前的行。
  files: RequestPreviewFile[];
  // Whether the host cut the body or the lines short.
  // 宿主是否截断了正文或行。
  truncated: boolean;
}

// ToolRequest is a request still being written.
// ToolRequest 是仍在书写中的请求。
export interface ToolRequest {
  // Writing phase.
  // 书写阶段。
  phase: ToolRequestPhase;
  // UTF-8 bytes of arguments received so far.
  // 目前已接收的参数 UTF-8 字节数。
  bytes: number;
  // Sanitized preview, for tools that produce one.
  // 净化预览（仅产生预览的工具）。
  preview: RequestPreview | null;
}

// PATCH_FILE_HEADER matches the Apply Patch line that starts a file section.
// PATCH_FILE_HEADER 匹配开始一个文件段的 Apply Patch 行。
const PATCH_FILE_HEADER = /^\*\*\* (?:Add|Update|Delete) File: (.+)$/;

// readRequestPreview reads the host preview object; it never throws on unexpected shapes.
// readRequestPreview 读取宿主预览对象；遇到意外结构时不会抛出。
export function readRequestPreview(value: unknown): RequestPreview | null {
  const preview = objectOf(value);
  const kind = textOf(preview?.kind);
  if (!preview || !kind) return null;
  const flags = listOf(preview.redaction_flags);
  const body = textOf(preview.body_preview);
  const files: RequestPreviewFile[] = [];
  for (const resource of listOf(preview.affected_resources)) {
    const path = textOf(resource);
    if (path && !files.some((file) => file.path === path)) files.push({ path, rows: [] });
  }
  let structured = false;
  for (const item of listOf(preview.structured_lines)) {
    const line = objectOf(item);
    const file = files[numberOf(line?.resource_index) ?? numberOf(line?.operation_index) ?? -1];
    const kind = previewRowKind(line?.side);
    if (!file || !kind || typeof line?.content !== "string") continue;
    file.rows.push({ kind, text: line.content });
    structured = true;
  }
  if (!structured && body && (kind === "file_edit" || kind === "apply_patch")) bodyRows(body, kind, files);
  return {
    body,
    files,
    kind,
    progress: objectOf(preview.progress_fields) ?? {},
    title: objectOf(preview.title_fields) ?? {},
    truncated: flags.includes("body_preview_truncated") || flags.includes("structured_preview_truncated"),
  };
}

// previewRowKind maps a structured line side onto a diff row role.
// previewRowKind 将结构化行的侧别映射为差异行角色。
function previewRowKind(side: unknown): PlainDiffRow["kind"] | null {
  return side === "old" ? "delete" : side === "new" ? "insert" : side === "context" ? "context" : null;
}

// bodyRows fills file rows from the text body when the host sent no structured lines.
// bodyRows 在宿主未发送结构化行时从文本正文填充文件行。
function bodyRows(body: string, kind: "file_edit" | "apply_patch", files: RequestPreviewFile[]): void {
  let file: RequestPreviewFile | undefined = kind === "file_edit" ? files[0] : undefined;
  for (const line of body.split("\n")) {
    if (kind === "apply_patch") {
      const header = PATCH_FILE_HEADER.exec(line);
      if (header) {
        file = files.find((candidate) => candidate.path === header[1]);
        continue;
      }
      if (!file || line.startsWith("*** ") || line.startsWith("@@")) continue;
      if (line.startsWith("+")) file.rows.push({ kind: "insert", text: line.slice(1) });
      else if (line.startsWith("-")) file.rows.push({ kind: "delete", text: line.slice(1) });
      else if (line.startsWith(" ")) file.rows.push({ kind: "context", text: line.slice(1) });
      continue;
    }
    if (!file) return;
    if (line.startsWith("+ ")) file.rows.push({ kind: "insert", text: line.slice(2) });
    else if (line.startsWith("- ")) file.rows.push({ kind: "delete", text: line.slice(2) });
  }
}

// REQUEST_EVENT_STATUS gives the synthetic event the lifecycle matching the card state.
// REQUEST_EVENT_STATUS 为合成事件提供与卡片状态一致的生命周期。
const REQUEST_EVENT_STATUS: Record<ToolCardState, ToolCallEvent["status"]> = {
  done: "completed",
  failed: "failed",
  running: "running",
  stopped: "stopped",
  waiting: "waiting_user",
};

// requestToolCall builds the call a card shows while its request is still being written, or after the turn ended
// before the call existed. The input carries only what the preview has revealed, plus the plan body for
// submit_plan so the plan card can stream it.
// requestToolCall 构建请求仍在书写时（或回合在调用产生前结束后）卡片展示的调用。输入只携带预览已揭示的内容，
// submit_plan 另携带计划正文，使计划卡可以流式展示。
export function requestToolCall(toolName: string, request: ToolRequest, key: string, state: ToolCardState): ToolCall {
  const input: Record<string, unknown> = { ...(request.preview?.title ?? {}) };
  if (request.preview?.kind === "submit_plan" && request.preview.body) input.content = request.preview.body;
  const event: ToolCallEvent = {
    api_call_id: 0,
    // Preview fields arrived as JSON, so they are JSON values.
    // 预览字段以 JSON 到达，因此都是 JSON 值。
    input: input as ToolCallEvent["input"],
    kind: "tool_call",
    request_stream_id: key,
    status: REQUEST_EVENT_STATUS[state],
    tool_call_id: 0,
    tool_name: toolName,
  };
  return {
    durationMs: null,
    event,
    host: null,
    input,
    isError: false,
    key,
    modelOutput: null,
    name: canonicalToolName(toolName),
    request,
    state,
  };
}
