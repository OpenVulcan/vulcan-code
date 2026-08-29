import React from "react";
import {
  BrainCircuit,
  Copy,
  FileText,
  Hammer,
  ScrollText,
  TerminalSquare,
  X,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogTitle,
} from "@/components/ui/dialog";
import type { DiffPanelSnapshot } from "@/diff/types";
import { cn } from "@/lib/utils";
import {
  GeneratedMediaDetail,
  type GeneratedMediaAttachment,
} from "@/components/media/generated-media";
import { MarkdownMessage } from "@/components/conversation/conversation-markdown";
import { parseUnifiedDiffAsync } from "@/diff/diff-parse-worker-client";
import { hasRenderableDiff } from "@/diff/parseDiff";
import { useI18n, type TranslationKey } from "@/i18n";

// LazyDiffReviewPanel loads the heavy diff review renderer only for diff detail modals.
// LazyDiffReviewPanel 仅在 diff 详情弹窗中加载较重的 diff 审核渲染器。
const LazyDiffReviewPanel = React.lazy(async () => {
  const module = await import("@/diff/DiffReviewPanel");
  return { default: module.DiffReviewPanel };
});

// ConversationDetail describes the modal payload opened from one special block.
// ConversationDetail 描述从特殊块打开的弹窗载荷。
export interface ConversationDetail {
  // Detail modal title.
  // 详情弹窗标题。
  title: string;
  // Detail modal kind used for visual treatment.
  // 用于视觉处理的详情弹窗类型。
  kind: string;
  // Detail body source.
  // 详情正文源码。
  body: string;
  // Optional detail metadata rows.
  // 可选详情元数据行。
  metadata?: Array<[string, string]>;
  // Optional generated media outputs rendered by the detail modal.
  // 可选生成媒体输出，由详情弹窗渲染。
  mediaAttachments?: GeneratedMediaAttachment[];
  // Optional hidden detail record reference loaded on demand.
  // 可选按需加载的隐藏详情记录引用。
  detailRef?: ConversationDetailRef;
}

// ConversationDetailRef identifies one hidden detail record stored outside the visible timeline.
// ConversationDetailRef 标识一条存放在可见时间线之外的隐藏详情记录。
export interface ConversationDetailRef {
  // Session id that owns the hidden detail record.
  // 拥有隐藏详情记录的会话标识。
  sessionId: string;
  // Stable hidden detail record id.
  // 稳定隐藏详情记录标识。
  recordId?: string;
  // Tool-call id used when the visible call row has not received a detail record id.
  // 当可见调用行尚未收到详情记录标识时使用的工具调用标识。
  toolCallId?: string;
  // Optional JSONL event sequence for direct detail lookup.
  // 用于直接定位详情的可选 JSONL 事件序号。
  seq?: number;
  // Optional conversation message index used when seq is unavailable.
  // seq 不可用时使用的可选会话消息索引。
  messageIndex?: number;
}

// ToolDetailPayload carries one hidden structured tool-detail record.
// ToolDetailPayload 承载单条隐藏结构化工具详情记录。
export interface ToolDetailPayload {
  // Model-facing assembled tool result body.
  // 面向模型的组合工具返回正文。
  body: string;
  // Structured metadata used by display renderers.
  // 展示渲染器使用的结构化元数据。
  metadata?: unknown;
  // Versioned authoritative raw invocation envelope when the runtime can reconstruct it.
  // 运行时可重建时提供的版本化权威原始调用信封。
  rawInvocation?: unknown;
}

// ConversationDetailModal renders the enlarged detail surface for special transcript blocks.
// ConversationDetailModal 渲染特殊转录块的放大详情界面。
export function ConversationDetailModal(props: {
  // Detail payload selected from one transcript block.
  // 从单个转录块选择出的详情载荷。
  detail: ConversationDetail;
  // Closes the detail modal.
  // 关闭详情弹窗。
  onClose: () => void;
  // Opens a generated media attachment in the media player.
  // 在媒体播放器中打开生成媒体附件。
  onOpenMedia: (attachment: GeneratedMediaAttachment) => void;
  // Loads one hidden tool detail record when the detail references it.
  // 在详情引用隐藏记录时加载对应工具详情。
  onLoadToolDetail: (detailRef: ConversationDetailRef) => Promise<ToolDetailPayload | null>;
  // Copies text through the host-provided clipboard path.
  // 通过宿主提供的剪贴板路径复制文本。
  onCopyText: (value: string) => void | Promise<void>;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  // loadedToolDetail stores the hidden structured tool result loaded after the modal opens.
  // loadedToolDetail 保存弹窗打开后按需加载到的隐藏结构化工具结果。
  const [loadedToolDetail, setLoadedToolDetail] = React.useState<ToolDetailPayload | null>(null);
  // detailLoadError stores a short failure note for on-demand detail hydration.
  // detailLoadError 保存按需详情水合失败时的简短提示。
  const [detailLoadError, setDetailLoadError] = React.useState<string | null>(null);
  // diffSnapshot stores the worker-parsed view of the current diff detail body.
  // diffSnapshot 保存当前 diff 详情正文的 Worker 解析视图。
  const [diffSnapshot, setDiffSnapshot] = React.useState<DiffPanelSnapshot | null>(null);
  // diffParseLoading tracks worker parsing for diff detail bodies.
  // diffParseLoading 跟踪 diff 详情正文的 Worker 解析状态。
  const [diffParseLoading, setDiffParseLoading] = React.useState(false);
  // diffParseError stores the latest diff detail parse error.
  // diffParseError 保存最近一次 diff 详情解析错误。
  const [diffParseError, setDiffParseError] = React.useState<string | null>(null);
  // diffParseSeqRef rejects stale async parse results after detail changes.
  // diffParseSeqRef 在详情变化后拒绝过期异步解析结果。
  const diffParseSeqRef = React.useRef(0);
  React.useEffect(() => {
    setLoadedToolDetail(null);
    setDetailLoadError(null);
    const detailRef = props.detail.detailRef;
    if (!detailRef) {
      return;
    }
    let cancelled = false;
    void props.onLoadToolDetail(detailRef)
      .then((payload) => {
        if (!cancelled) {
          setLoadedToolDetail(payload);
        }
      })
      .catch((error) => {
        if (!cancelled) {
          setDetailLoadError(error instanceof Error ? error.message : String(error));
        }
      });
    return () => {
      cancelled = true;
    };
  }, [
    props.detail.detailRef?.messageIndex,
    props.detail.detailRef?.recordId,
    props.detail.detailRef?.seq,
    props.detail.detailRef?.sessionId,
    props.detail.detailRef?.toolCallId,
    props.onLoadToolDetail,
  ]);
  const diffBody = props.detail.body;
  React.useEffect(() => {
    const requestSeq = diffParseSeqRef.current + 1;
    diffParseSeqRef.current = requestSeq;
    if (props.detail.kind !== "diff") {
      setDiffSnapshot(null);
      setDiffParseLoading(false);
      setDiffParseError(null);
      return undefined;
    }
    let cancelled = false;
    setDiffSnapshot(null);
    setDiffParseLoading(true);
    setDiffParseError(null);
    void parseDiffDetailBodyAsync(diffBody)
      .then((snapshot) => {
        if (!cancelled && diffParseSeqRef.current === requestSeq) {
          setDiffSnapshot(snapshot);
          setDiffParseError(null);
        }
      })
      .catch((error) => {
        if (!cancelled && diffParseSeqRef.current === requestSeq) {
          setDiffSnapshot(null);
          setDiffParseError(error instanceof Error ? error.message : String(error));
        }
      })
      .finally(() => {
        if (!cancelled && diffParseSeqRef.current === requestSeq) {
          setDiffParseLoading(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [diffBody, props.detail.kind]);
  const DetailIcon = conversationDetailIcon(props.detail.kind);
  const detailType = conversationDetailTitle(props.detail.kind, t);
  const detailSubtitle = props.detail.metadata
    ?.map(([label, value]) => `${label}: ${value}`)
    .join(" · ");
  const fullBody = props.detail.kind === "tool-raw" && loadedToolDetail?.rawInvocation !== undefined
    ? renderRawToolInvocationMarkdown(loadedToolDetail.rawInvocation, t)
    : loadedToolDetail
    ? `${props.detail.body}\n\n---\n\n${renderToolDetailPayloadMarkdown(loadedToolDetail)}`
    : detailLoadError
      ? `${props.detail.body}\n\n---\n\nDetail load failed: ${detailLoadError}`
      : props.detail.body;
  const diffDetailSnapshot = diffSnapshot ?? {
    source: "message",
    repositoryRoot: "",
    files: [],
    stats: {
      files: 0,
      additions: 0,
      deletions: 0,
      totalChangedLines: 0,
      totalChangedBytes: 0,
      isLarge: false,
      largeReasons: [],
    },
    completeness: "complete",
    isTruncated: false,
    notice: diffDetailEmptyBody(diffParseLoading, diffParseError, t),
    rawDiff: fullBody,
  } satisfies DiffPanelSnapshot;
  return (
    <Dialog open onOpenChange={(open) => !open && props.onClose()}>
      <DialogContent className={cn("detail-modal sm:max-w-[min(1080px,96vw)]", props.detail.kind === "thinking" && "thinking-detail-modal")} showCloseButton={false}>
        <header className="detail-modal-header">
          <div className="detail-title-row">
            <div className="detail-title-icon">
              <DetailIcon size={18} />
            </div>
            <div className="detail-title-copy">
              <span>{detailType}</span>
              <DialogTitle>{props.detail.title}</DialogTitle>
              {detailSubtitle ? <p>{detailSubtitle}</p> : null}
            </div>
          </div>
          <div className="detail-modal-actions">
            <Button aria-label={t("conversation.detail.copy")} className="icon-button" onClick={() => void props.onCopyText(fullBody)} size="icon" type="button" variant="outline">
              <Copy size={17} />
            </Button>
            <Button aria-label={t("conversation.detail.close")} className="icon-button" onClick={props.onClose} size="icon" type="button" variant="outline">
              <X size={17} />
            </Button>
          </div>
        </header>
        <div className="detail-modal-body">
          {props.detail.mediaAttachments?.length ? (
            <GeneratedMediaDetail
              attachments={props.detail.mediaAttachments}
              onOpenMedia={props.onOpenMedia}
            />
          ) : null}
          {props.detail.kind === "diff" ? (
            <React.Suspense fallback={<DiffDetailPanelLoading />}>
              <LazyDiffReviewPanel
                emptyBody={diffDetailEmptyBody(diffParseLoading, diffParseError, t)}
                emptyTitle="Git Diff"
                loading={diffParseLoading}
                snapshot={diffDetailSnapshot}
                onCopyRaw={(rawDiff) => void props.onCopyText(rawDiff)}
              />
            </React.Suspense>
          ) : (
            <MarkdownMessage content={fullBody} eager onOpenMedia={props.onOpenMedia} />
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}

// parseDiffDetailBodyAsync loads diff parsing helpers only for diff detail modals.
// parseDiffDetailBodyAsync 仅为 diff 详情弹窗按需加载 diff 解析辅助模块。
async function parseDiffDetailBodyAsync(diffBody: string): Promise<DiffPanelSnapshot | null> {
  if (!hasRenderableDiff(diffBody)) {
    return null;
  }
  return parseUnifiedDiffAsync(diffBody, "message", "");
}

// DiffDetailPanelLoading renders while the diff review panel chunk is loading.
// DiffDetailPanelLoading 渲染 diff 审核面板 chunk 加载期间的轻量状态。
function DiffDetailPanelLoading() {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  return (
    <div className="detail-empty-state">
      <span>{t("conversation.diff.loadingPanel")}</span>
    </div>
  );
}

// diffDetailEmptyBody formats the detail modal empty-state message for async diff parsing.
// diffDetailEmptyBody 格式化详情弹窗异步 diff 解析的空态文案。
function diffDetailEmptyBody(
  // Whether worker parsing is still running.
  // Worker 解析是否仍在进行。
  loading: boolean,
  // Optional worker parse failure message.
  // 可选的 Worker 解析失败信息。
  error: string | null,
  // Translates stable UI copy keys.
  // 翻译稳定 UI 文案键。
  t: (key: TranslationKey) => string,
): string {
  if (loading) {
    return t("conversation.diff.parsingWorker");
  }
  if (error) {
    return formatConversationDetailTranslation(t, "conversation.diff.parseFailed", { error });
  }
  return t("conversation.diff.unstructuredFallback");
}

// formatConversationDetailTranslation replaces named placeholders in localized detail templates.
// formatConversationDetailTranslation 替换详情弹窗本地化模板中的具名占位符。
function formatConversationDetailTranslation(
  t: (key: TranslationKey) => string,
  key: TranslationKey,
  values: Record<string, string>,
): string {
  return Object.entries(values).reduce(
    (text, [name, value]) => text.replaceAll(`{${name}}`, value),
    t(key),
  );
}

// conversationDetailIcon returns the icon component used by one detail modal kind.
// conversationDetailIcon 返回单个详情弹窗类型使用的图标组件。
function conversationDetailIcon(kind: string) {
  if (kind === "diff") return FileText;
  if (kind === "thinking") return BrainCircuit;
  if (kind === "command") return TerminalSquare;
  if (kind === "tool-call" || kind === "tool-result" || kind === "tool-raw") return Hammer;
  return ScrollText;
}

// conversationDetailTitle returns the user-visible title for one detail modal kind.
// conversationDetailTitle 返回单个详情弹窗类型的用户可见标题。
function conversationDetailTitle(kind: string, t: (key: TranslationKey) => string): string {
  if (kind === "tool-call") return t("conversation.block.toolCall");
  if (kind === "tool-result") return t("conversation.block.toolResult");
  if (kind === "tool-raw") return t("toolPresentation.raw.open");
  if (kind === "command") return t("conversation.block.command");
  if (kind === "diff") return "Git Diff";
  if (kind === "thinking") return t("conversation.block.thinking");
  if (kind === "notice") return t("conversation.block.notice");
  return t("conversation.block.detail");
}

// renderRawToolInvocationMarkdown renders the versioned invocation envelope as one JSON block.
// renderRawToolInvocationMarkdown 将版本化调用信封渲染为单个 JSON 代码块。
function renderRawToolInvocationMarkdown(
  rawInvocation: unknown,
  t: (key: TranslationKey) => string,
): string {
  // envelope exposes completeness only after the raw value is validated as an object.
  // envelope 仅在原始值被校验为对象后暴露完整性字段。
  const envelope = objectValue(rawInvocation);
  // warning explicitly identifies reconstructed legacy payloads before the JSON block.
  // warning 在 JSON 块前明确标识重建的旧版载荷。
  const warning = envelope?.completeness === "legacy_partial"
    ? `> ${t("toolPresentation.raw.legacyPartial")}\n\n`
    : "";
  return `${warning}${fencedMarkdownBlock(JSON.stringify(rawInvocation, null, 2), "json")}`;
}

// renderToolDetailPayloadMarkdown converts structured tool detail metadata into Markdown for the modal.
// renderToolDetailPayloadMarkdown 将结构化工具详情元数据转换为弹窗使用的 Markdown。
function renderToolDetailPayloadMarkdown(payload: ToolDetailPayload): string {
  const display = metadataValue(payload.metadata, ["display"]);
  const modelOutput = payload.body;
  const displayMarkdown = renderToolDisplayMarkdown(display);
  const sections = displayMarkdown ? [`## Structured Display\n\n${displayMarkdown}`] : [];
  if (modelOutput.trim()) {
    sections.push(`## Model Output\n\n${fencedMarkdownBlock(modelOutput, "")}`);
  }
  return sections.join("\n\n---\n\n") || payload.body;
}

// renderToolDisplayMarkdown renders a structured display payload without parsing model-output text.
// renderToolDisplayMarkdown 渲染结构化展示载荷，不解析模型组合输出文本。
function renderToolDisplayMarkdown(display: unknown): string | null {
  const object = objectValue(display);
  if (!object) {
    return null;
  }
  const kind = stringValue(object.kind);
  if (kind === "shell_command") {
    return renderShellCommandDisplayMarkdown(object);
  }
  if (kind === "vulcan_file_list") {
    return renderFileListDisplayMarkdown(object);
  }
  const status = stringValue(object.status);
  const summary = stringValue(object.summary);
  const sections = [
    `Kind: ${kind ?? "generic"}`,
    status ? `Status: ${status}` : null,
  ].filter(Boolean);
  if (object.arguments !== undefined) {
    sections.push(`Arguments:\n\n${fencedMarkdownBlock(JSON.stringify(object.arguments, null, 2), "json")}`);
  }
  if (summary) {
    sections.push(`Summary: ${summary}`);
  }
  return sections.join("\n\n");
}

// renderFileListDisplayMarkdown renders structured file-list fields without duplicating raw output.
// renderFileListDisplayMarkdown 渲染结构化文件列表字段，避免重复原始输出。
function renderFileListDisplayMarkdown(display: Record<string, unknown>): string {
  const rows = [
    ["Root", stringValue(display.root)],
    ["Pattern", stringValue(display.pattern)],
    ["Recursive", display.recursive === undefined ? null : String(display.recursive)],
    ["Ignore", display.ignore === undefined ? null : String(display.ignore)],
    ["Files", display.files_count === undefined ? null : String(display.files_count)],
    ["Truncated", display.truncated === undefined ? null : String(display.truncated)],
    ["Ignored", display.ignored_count === undefined ? null : String(display.ignored_count)],
    ["Status", stringValue(display.status)],
  ] as Array<[string, string | null]>;
  const sections = rows
    .filter(([, value]) => value)
    .map(([label, value]) => `- ${label}: ${value}`);
  const argumentsValue = display.arguments;
  if (argumentsValue !== undefined) {
    sections.push(`### Arguments\n\n${fencedMarkdownBlock(JSON.stringify(argumentsValue, null, 2), "json")}`);
  }
  const files = Array.isArray(display.files)
    ? display.files.map((file) => String(file)).filter(Boolean)
    : [];
  if (files.length > 0) {
    sections.push(`### Files\n\n${files.map((file) => `- \`${file}\``).join("\n")}`);
  }
  return sections.join("\n");
}

// renderShellCommandDisplayMarkdown renders shell_command fields as stable rows and stream blocks.
// renderShellCommandDisplayMarkdown 将 shell_command 字段渲染为稳定信息行和输出流块。
function renderShellCommandDisplayMarkdown(display: Record<string, unknown>): string {
  const rows = [
    ["Command", stringValue(display.command)],
    ["Shell", stringValue(display.shell)],
    ["Working directory", stringValue(display.working_directory)],
    ["Status", stringValue(display.status)],
    ["Duration", stringValue(display.duration)],
    ["Exit code", display.exit_code === undefined ? null : String(display.exit_code)],
  ] as Array<[string, string | null]>;
  const sections = rows
    .filter(([, value]) => value)
    .map(([label, value]) => `- ${label}: ${value}`);
  const stdout = stringValue(display.stdout);
  const stderr = stringValue(display.stderr);
  if (stdout) {
    sections.push(`### stdout\n\n${fencedMarkdownBlock(stdout, "")}`);
  }
  if (stderr) {
    sections.push(`### stderr\n\n${fencedMarkdownBlock(stderr, "")}`);
  }
  return sections.join("\n");
}

// fencedMarkdownBlock wraps arbitrary text in a Markdown code fence with backtick escaping.
// fencedMarkdownBlock 使用 Markdown 代码围栏包裹任意文本并规避反引号冲突。
function fencedMarkdownBlock(source: string, language: string): string {
  const fence = source.includes("```") ? "````" : "```";
  return `${fence}${language}\n${source}\n${fence}`;
}

// metadataValue resolves one nested metadata path from JSON-like objects.
// metadataValue 从类 JSON 对象中解析一条嵌套元数据路径。
function metadataValue(metadata: unknown, path: string[]): unknown {
  for (const candidate of path) {
    const segments = candidate.split(".");
    let current: unknown = metadata;
    let found = true;
    for (const segment of segments) {
      if (!current || typeof current !== "object" || !(segment in current)) {
        found = false;
        break;
      }
      current = (current as Record<string, unknown>)[segment];
    }
    if (found) {
      return current;
    }
  }
  return null;
}

// objectValue narrows an unknown JSON value into a plain object record.
// objectValue 将未知 JSON 值收窄为普通对象记录。
function objectValue(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

// stringValue returns one trimmed string from an unknown JSON value.
// stringValue 从未知 JSON 值中返回一条清理后的字符串。
function stringValue(value: unknown): string | null {
  return typeof value === "string" && value.trim() ? value.trim() : null;
}
