import * as React from "react";
import {
  CheckCircle2,
  ChevronDown,
  CircleStop,
  Clock3,
  FileText,
  LoaderCircle,
  TerminalSquare,
  Wrench,
  XCircle,
} from "lucide-react";

import { ConversationMeasuredCollapse, useConversationMeasuredExpansion } from "@/components/conversation/conversation-measured-collapse";
import { MarkdownMessage } from "@/components/conversation/conversation-markdown";
import { ConversationV1GeneratedResourceList } from "@/components/conversation/conversation-v1-resource-list";
import type {
  V1ChangeSetFilePresentation,
  V1ChangeSetRow,
  V1FileChangePresentation,
  V1ToolPresentation,
  V1ToolPresentationIcon,
  V1ToolPresentationState,
} from "@/components/conversation/tool-presentations/v1-tool-presentation";
import { resolveV1ToolPresentation } from "@/components/conversation/tool-presentations/v1-tool-presentation";
import { cn } from "@/lib/utils";
import type { ExternalResourceRef } from "@/service/conversation-event-window-store";
import type { ToolCallDisplayItem } from "@/service/conversation-event-display-projection";
import type { ConversationSessionResourceCatalogV1 } from "@/state/conversation-session-resources-v1";

/** Props for one event-native V1 tool card.
 * 一张事件原生 V1 工具卡的属性。 */
export interface ConversationV1ToolCardProps {
  // Complete tool display item produced by the V1 event projection.
  // V1 事件投影产出的完整工具显示项。
  item: ToolCallDisplayItem;
  // Initial expansion used only when the session collapse store has no value.
  // 仅在会话折叠存储没有值时使用的初始展开状态。
  defaultExpanded?: boolean;
  // Kind-qualified external navigation identities that still exist at render time.
  // 渲染时仍然存在的带类型外部导航身份。
  availableExternalRefKeys?: ReadonlySet<string>;
  // Navigation callback; absence disables only the navigation control.
  // 导航回调；缺失时只禁用导航控件。
  onOpenExternalRef?: (reference: ExternalResourceRef) => void;
  // Owner-bound project resource metadata for generated tool resources.
  // 工具生成资源所使用的绑定所有者项目资源元数据。
  resourceCatalog?: ConversationSessionResourceCatalogV1 | null;
}

/** Renders one self-contained V1 tool card without requesting supplemental data.
 * 在不请求补充数据的情况下渲染一张自包含 V1 工具卡。 */
export function ConversationV1ToolCard(props: ConversationV1ToolCardProps) {
  // presentation is a pure projection of the one authoritative tool event.
  // presentation 是单条权威工具事件的纯投影。
  const presentation = React.useMemo(
    () => resolveV1ToolPresentation(props.item),
    [props.item],
  );
  // expanded is scoped by the stable event seq rather than a legacy string call id.
  // expanded 以稳定事件 seq 为作用域，而不是旧字符串调用标识。
  const [expanded, setExpanded] = useConversationMeasuredExpansion(
    presentation.id,
    props.defaultExpanded ?? false,
  );
  // summary is derived once from the same model used by the expanded body.
  // summary 从展开正文使用的同一模型中仅派生一次。
  const summary = presentationSummary(presentation);

  return (
    <article
      className={cn(
        "tool-presentation-card",
        "v1-tool-card",
        presentation.kind.replace("_", "-"),
        expanded && "expanded",
      )}
      data-api-call-id={presentation.apiCallId}
      data-seq={presentation.seq}
      data-tool-kind={presentation.kind}
      data-tool-state={presentation.state}
    >
      <button
        type="button"
        className={cn(
          "tool-presentation-summary",
          "v1-tool-card-summary",
          presentation.icon === null && "without-icon",
        )}
        aria-expanded={expanded}
        onClick={() => setExpanded(!expanded)}
      >
        <ToolPresentationIcon icon={presentation.icon} />
        <ToolPresentationState state={presentation.state} />
        <span className="v1-tool-card-title">{presentation.title}</span>
        <span className="v1-tool-card-target">{summary.target}</span>
        {summary.format ? <span className="file-change-text-format">{summary.format}</span> : null}
        {presentation.kind === "file_change" ? (
          <span className="file-change-counts" data-change-counts>
            <span className="file-change-added">+{presentation.additions}</span>
            <span className="file-change-removed">-{presentation.deletions}</span>
          </span>
        ) : null}
        <ChevronDown className="tool-presentation-toggle" size={14} aria-hidden="true" />
      </button>

      <ConversationMeasuredCollapse collapseId={presentation.id} expanded={expanded}>
        <div className="v1-tool-card-detail" data-tool-detail>
          <ToolPresentationBody presentation={presentation} />
          <ConversationV1GeneratedResourceList
            catalog={props.resourceCatalog ?? null}
            resources={props.item.tool_call.generated_resources ?? []}
          />
          <ToolExternalNavigation
            references={presentation.externalRefs}
            availableReferenceKeys={props.availableExternalRefKeys}
            onOpen={props.onOpenExternalRef}
          />
        </div>
      </ConversationMeasuredCollapse>
    </article>
  );
}

/** Props for a deliberately optional tool icon.
 * 明确可选工具图标的属性。 */
interface ToolPresentationIconProps {
  icon: V1ToolPresentationIcon;
}

/** Renders no node at all when an exact card contract has no valid icon.
 * 当精确卡片契约没有有效图标时完全不渲染节点。 */
function ToolPresentationIcon(props: ToolPresentationIconProps) {
  if (props.icon === null) {
    return null;
  }
  // Icon is selected only from the typed presentation identity.
  // Icon 仅从强类型展示身份中选择。
  const Icon = props.icon === "file"
    ? FileText
    : props.icon === "shell"
      ? TerminalSquare
      : props.icon === "markdown"
        ? FileText
        : Wrench;
  return (
    <span className="v1-tool-card-icon" data-tool-icon={props.icon} aria-hidden="true">
      <Icon size={15} />
    </span>
  );
}

/** Props for the event-native tool status indicator.
 * 事件原生工具状态指示器的属性。 */
interface ToolPresentationStateProps {
  state: V1ToolPresentationState;
}

/** Renders the exact persisted tool state without success/failure regrouping.
 * 渲染精确持久化工具状态，且不按成功失败重新分组。 */
function ToolPresentationState(props: ToolPresentationStateProps) {
  // Icon is a visual companion to the exact status text.
  // Icon 是精确状态文本的视觉辅助。
  const Icon = props.state === "prepared" || props.state === "running"
    ? LoaderCircle
    : props.state === "waiting_user"
      ? Clock3
      : props.state === "completed"
        ? CheckCircle2
        : props.state === "failed"
          ? XCircle
          : CircleStop;
  return (
    <span className={cn("tool-presentation-state", `state-${props.state}`)}>
      <Icon className={props.state === "prepared" || props.state === "running" ? "animate-spin" : undefined} size={14} aria-hidden="true" />
      <span>{toolStateLabel(props.state)}</span>
    </span>
  );
}

/** Returns the user-visible label for one persisted tool status.
 * 返回一个持久化工具状态的用户可见标签。 */
function toolStateLabel(state: V1ToolPresentationState): string {
  switch (state) {
    case "prepared":
      return "准备调用";
    case "running":
      return "正在调用";
    case "waiting_user":
      return "等待用户";
    case "completed":
      return "已执行";
    case "failed":
      return "执行失败";
    case "interrupted":
      return "已中断";
    case "cancelled":
    case "stopped":
      return "已停止";
  }
}

/** Stable collapsed summary shared with the expanded body model.
 * 与展开正文模型共享的稳定折叠摘要。 */
interface V1ToolCardSummary {
  target: string;
  format: string | null;
}

/** Builds the collapsed summary without parsing display text.
 * 在不解析展示文本的情况下构建折叠摘要。 */
function presentationSummary(presentation: V1ToolPresentation): V1ToolCardSummary {
  if (presentation.kind === "file_change") {
    // firstFile is also the first file rendered in the expanded body.
    // firstFile 也是展开正文渲染的第一个文件。
    const firstFile = presentation.files[0];
    return {
      target: firstFile
        ? presentation.files.length === 1
          ? firstFile.displayPath
          : `${firstFile.displayPath} 等 ${presentation.files.length} 个文件`
        : presentation.requestTarget ?? presentation.summary ?? presentation.toolName,
      format: firstFile ? changeSetFormatSummary(firstFile) : null,
    };
  }
  if (presentation.kind === "file_access" || presentation.kind === "markdown") {
    return {
      target: presentation.targets.join(" · "),
      format: null,
    };
  }
  if (presentation.kind === "shell") {
    return { target: presentation.command, format: null };
  }
  return { target: presentation.toolName, format: null };
}

/** Renders the discriminated detail body selected by the pure resolver.
 * 渲染纯 resolver 选择的判别详情正文。 */
function ToolPresentationBody(props: { presentation: V1ToolPresentation }) {
  // presentation is narrowed by its explicit kind, never by title text.
  // presentation 通过显式 kind 收窄，绝不通过标题文本。
  const presentation = props.presentation;
  switch (presentation.kind) {
    case "file_change":
      return <FileChangeBody presentation={presentation} />;
    case "file_access":
    case "markdown":
      return <MarkdownBody markdown={presentation.markdown} />;
    case "shell":
      return (
        <div className="tool-presentation-detail">
          {presentation.command ? (
            <pre className="tool-presentation-command-line"><span>$ </span>{presentation.command}</pre>
          ) : null}
          <MarkdownBody markdown={presentation.outputMarkdown} />
        </div>
      );
    case "native":
    case "generic":
      return (
        <div className="v1-tool-card-json-sections">
          <ToolPayloadSection title="输入" markdown={presentation.inputMarkdown} />
          <ToolPayloadSection title="输出" markdown={presentation.outputMarkdown} />
        </div>
      );
  }
}

/** Props for one Markdown-backed event payload section.
 * 一段 Markdown 事件载荷区域的属性。 */
interface ToolPayloadSectionProps {
  title: string;
  markdown: string;
}

/** Renders one event payload section only when it has persisted content.
 * 仅在存在持久化内容时渲染事件载荷区域。 */
function ToolPayloadSection(props: ToolPayloadSectionProps) {
  if (!props.markdown) {
    return null;
  }
  return (
    <section className="v1-tool-card-section">
      <div className="v1-tool-card-section-title">{props.title}</div>
      <MarkdownBody markdown={props.markdown} />
    </section>
  );
}

/** Renders Markdown already retained by the tool event.
 * 渲染工具事件已经保留的 Markdown。 */
function MarkdownBody(props: { markdown: string }) {
  if (!props.markdown) {
    return <div className="file-change-empty">暂无输出</div>;
  }
  return (
    <div className="tool-presentation-output-scroll markdown-tool-block">
      <MarkdownMessage content={props.markdown} eager />
    </div>
  );
}

/** Props for the canonical ChangeSet body.
 * canonical ChangeSet 正文的属性。 */
interface FileChangeBodyProps {
  presentation: V1FileChangePresentation;
}

/** Renders persisted ChangeSet files and diagnostics without filesystem reads.
 * 在不读取文件系统的情况下渲染持久化 ChangeSet 文件与诊断。 */
function FileChangeBody(props: FileChangeBodyProps) {
  if (props.presentation.files.length === 0) {
    return (
      <div className="file-change-detail">
        <MarkdownBody markdown={props.presentation.outputMarkdown} />
      </div>
    );
  }
  return (
    <div className="file-change-tree" data-change-set-mode={props.presentation.mode ?? "pending"}>
      {props.presentation.summary ? (
        <div className="v1-tool-card-change-summary">{props.presentation.summary}</div>
      ) : null}
      {props.presentation.files.map((file, index) => (
        <ChangeSetFileBody key={`${file.resourceId ?? file.path}:${index}`} file={file} />
      ))}
      {props.presentation.diagnostics.map((diagnostic, index) => (
        <div
          key={`${diagnostic.level}:${index}`}
          className={cn("v1-tool-card-diagnostic", `level-${diagnostic.level}`)}
        >
          {diagnostic.message}
        </div>
      ))}
    </div>
  );
}

/** Renders one canonical ChangeSet file and its exact row model.
 * 渲染一个 canonical ChangeSet 文件及其精确行模型。 */
function ChangeSetFileBody(props: { file: V1ChangeSetFilePresentation }) {
  // formatSummary is reused verbatim by the file header regardless of card expansion timing.
  // formatSummary 无论卡片何时展开，都由文件头原样复用。
  const formatSummary = changeSetFormatSummary(props.file);
  return (
    <section className="file-change-tree-item" data-file-change={props.file.change}>
      <header className="v1-change-file-header">
        <span className="file-change-path-text">{props.file.displayPath}</span>
        {formatSummary ? <span className="file-change-text-format">{formatSummary}</span> : null}
        <span className="file-change-counts">
          <span className="file-change-added">+{props.file.additions}</span>
          <span className="file-change-removed">-{props.file.deletions}</span>
        </span>
      </header>
      <div className="file-change-detail review">
        <div className="v1-change-rows">
          {props.file.rows.map((row, index) => (
            <ChangeSetRow key={`${row.kind}:${row.lineNumber ?? "none"}:${index}`} row={row} />
          ))}
        </div>
      </div>
    </section>
  );
}

/** Renders one exact ChangeSet row including explicit omission accounting.
 * 渲染一条精确 ChangeSet 行，包括显式省略统计。 */
function ChangeSetRow(props: { row: V1ChangeSetRow }) {
  // prefix derives only from the typed row kind.
  // prefix 仅从强类型行类型派生。
  const prefix = props.row.kind === "added" ? "+" : props.row.kind === "deleted" ? "-" : " ";
  return (
    <div
      className={cn("v1-change-row", `row-${props.row.kind}`)}
      data-change-row={props.row.kind}
      data-logical-line-count={props.row.logicalLineCount}
    >
      <span className="v1-change-line-number">{props.row.lineNumber ?? ""}</span>
      <span className="v1-change-prefix">{prefix}</span>
      <span className="v1-change-content">{props.row.content}</span>
    </div>
  );
}

/** Returns stable before/after format metadata for one ChangeSet file.
 * 返回一个 ChangeSet 文件稳定的变更前后格式元数据。 */
function changeSetFormatSummary(file: V1ChangeSetFilePresentation): string | null {
  if (file.beforeFormat && file.afterFormat && file.beforeFormat !== file.afterFormat) {
    return `${file.beforeFormat} → ${file.afterFormat}`;
  }
  return file.afterFormat ?? file.beforeFormat;
}

/** Props for navigation controls backed by persisted external references.
 * 由持久化外部引用支持的导航控件属性。 */
interface ToolExternalNavigationProps {
  references: readonly ExternalResourceRef[];
  availableReferenceKeys?: ReadonlySet<string>;
  onOpen?: (reference: ExternalResourceRef) => void;
}

/** Builds the namespace-safe navigation identity for one external reference.
 * 为一个外部引用构建命名空间安全的导航身份。 */
export function externalResourceRefKey(reference: ExternalResourceRef): string {
  return `${reference.kind}:${reference.id}`;
}

/** Keeps history visible and disables only navigation when a live target is absent.
 * 在实时目标缺失时保留历史显示，并且只禁用导航。 */
function ToolExternalNavigation(props: ToolExternalNavigationProps) {
  if (props.references.length === 0) {
    return null;
  }
  return (
    <div className="v1-tool-card-navigation">
      {props.references.map((reference) => {
        // navigable requires both a current target identity and an explicit callback.
        // navigable 同时要求当前目标身份存在且提供了显式回调。
        const navigable = props.availableReferenceKeys?.has(externalResourceRefKey(reference)) === true && Boolean(props.onOpen);
        return (
          <button
            key={`${reference.kind}:${reference.id}`}
            type="button"
            className="v1-tool-card-navigation-button"
            disabled={!navigable}
            onClick={() => props.onOpen?.(reference)}
          >
            {reference.kind}: {reference.id}
          </button>
        );
      })}
    </div>
  );
}
