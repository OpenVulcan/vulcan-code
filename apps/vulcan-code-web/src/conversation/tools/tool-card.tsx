// The one tool card frame: a compact head row (family tile, verb, target, facts, state) over a body that mounts
// only while open, so a long transcript keeps every closed card cheap.
// 唯一的工具卡框架：一行紧凑标题（家族图块、动作、目标、事实、状态）加仅在展开时挂载的正文，
// 使长转录中的每张收起卡片都保持轻量。
import * as React from "react";
import { Check, ChevronRight, CircleSlash, FileSearch2, Hourglass, X, type LucideIcon } from "lucide-react";
import { useWorkspaceFileOpener, type WorkspaceFileLines, type WorkspaceFileOpener } from "@/components/workspace/workspace-file-open-request";
import { useI18n, type TranslationKey } from "@/i18n";
import { formatBytes } from "@/lib/display-format";
import { IconButton } from "@/ui-kit/primitives/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/ui-kit/primitives/collapsible";
import { Spinner } from "@/ui-kit/primitives/spinner";
import type { ToolCall, ToolCardState } from "./tool-call";
import type { ToolRequest } from "./tool-request";

// ToolFamily groups tools by what they do; it tints the card edge and the sequence strip.
// ToolFamily 按工具的作用分组；它为卡片边缘与序列色条着色。
export type ToolFamily = "read" | "search" | "edit" | "run" | "web" | "media" | "agent" | "plan" | "other";

// TOOL_FAMILIES fixes the order families are counted in a sequence summary.
// TOOL_FAMILIES 固定序列摘要中统计家族的顺序。
export const TOOL_FAMILIES: readonly ToolFamily[] = ["read", "search", "edit", "run", "web", "media", "agent", "plan", "other"];

// ToolCardProps is what the dispatcher hands every card.
// ToolCardProps 是分派器交给每张卡的属性。
export interface ToolCardProps {
  // The call this card shows.
  // 当前卡片展示的调用。
  call: ToolCall;
  // Family chosen by the dispatcher for this call.
  // 分派器为该调用选定的家族。
  family: ToolFamily;
  // Whether the body is open.
  // 正文是否展开。
  open: boolean;
  // Requests the next open state.
  // 请求下一个展开状态。
  onOpenChange: (open: boolean) => void;
}

// STATE_LABEL_KEYS names each state for assistive technology and the state tooltip.
// STATE_LABEL_KEYS 为辅助技术与状态提示命名每种状态。
const STATE_LABEL_KEYS: Record<ToolCardState, TranslationKey> = {
  running: "toolCard.state.running",
  waiting: "toolCard.state.waiting",
  done: "toolCard.state.done",
  failed: "toolCard.state.failed",
  stopped: "toolCard.state.stopped",
};

// STATE_ICONS draws each settled state; running uses the shared spinner.
// STATE_ICONS 绘制每种已定状态；运行中使用共享旋转指示器。
const STATE_ICONS: Record<Exclude<ToolCardState, "running">, LucideIcon> = {
  waiting: Hourglass,
  done: Check,
  failed: X,
  stopped: CircleSlash,
};

// ToolStateMark renders the trailing lifecycle mark of one head row.
// ToolStateMark 渲染单个标题行末尾的生命周期标记。
function ToolStateMark(props: { state: ToolCardState }) {
  const { t } = useI18n();
  const label = t(STATE_LABEL_KEYS[props.state]);
  if (props.state === "running") return <span className="vk-tool-state" title={label}><Spinner /></span>;
  const Icon = STATE_ICONS[props.state];
  return <span aria-label={label} className="vk-tool-state" role="img" title={label}><Icon aria-hidden="true" /></span>;
}

// ToolFrameProps describes one head row and its optional body.
// ToolFrameProps 描述一个标题行及其可选正文。
export interface ToolFrameProps extends Pick<ToolCardProps, "family" | "open" | "onOpenChange"> {
  // The call, when the card passes it on; a request still being written adds its progress to the facts.
  // 卡片传入的调用；仍在书写的请求会把进度加入事实。
  call?: ToolCall;
  // Visual lifecycle.
  // 视觉生命周期。
  state: ToolCardState;
  // Family tile icon.
  // 家族图块图标。
  icon: LucideIcon;
  // Short action verb.
  // 简短动作词。
  verb: React.ReactNode;
  // Exact target (path, command, query), shown monospace and truncated.
  // 精确目标（路径、命令、查询），以等宽字体截断显示。
  target?: React.ReactNode;
  // Untruncated target for the native tooltip.
  // 原生提示使用的未截断目标。
  targetTitle?: string;
  // Project file the target names; the head then offers to show it in the files panel where one exists.
  // 目标指名的项目文件；存在文件面板时标题行提供在其中显示该文件的操作。
  filePath?: string | null;
  // Lines of filePath the card is about, brought into view when the file is shown.
  // 卡片涉及的 filePath 行，显示文件时滚动到视野内。
  fileLines?: WorkspaceFileLines | null;
  // Right-aligned facts such as line counts, diffstat, exit code or duration.
  // 右对齐事实，例如行数、差异统计、退出码或耗时。
  meta?: React.ReactNode;
  // Body content; a card without one has a fixed head row.
  // 正文内容；没有正文的卡片只有固定标题行。
  children?: React.ReactNode;
}

// ToolRequestMark shows how much of a request the model has written so far.
// ToolRequestMark 展示模型目前已书写的请求量。
// A request that stopped before its call existed keeps only the size it reached.
// 在调用产生前停止的请求只保留其达到的大小。
function ToolRequestMark(props: { request: ToolRequest; state: ToolCardState }) {
  const { t } = useI18n();
  const size = formatBytes(props.request.bytes);
  if (props.state !== "running") return <span className="vk-tool-request">{size}</span>;
  const key = props.request.phase === "receiving" ? "toolCard.request.receiving" : "toolCard.request.ready";
  return <span className="vk-tool-request" data-phase={props.request.phase}>{t(key).replace("{size}", size)}</span>;
}

// ToolFileAction shows the card's file in the files panel; it sits beside the head trigger, never inside it.
// ToolFileAction 在文件面板中显示卡片的文件；它位于标题触发器旁边，从不嵌入其中。
function ToolFileAction(props: { path: string; lines: WorkspaceFileLines | null; open: WorkspaceFileOpener }) {
  const { t } = useI18n();
  const label = t("workspaceFiles.showInFilesPanel");
  return (
    <IconButton aria-label={label} className="vk-tool-file" onClick={() => props.open(props.path, props.lines)} size="xs" title={label} type="button">
      <FileSearch2 aria-hidden="true" />
    </IconButton>
  );
}

/** Renders the shared tool card frame.
 * 渲染共享工具卡框架。 */
export function ToolFrame(props: ToolFrameProps) {
  const openFile = useWorkspaceFileOpener();
  const expandable = props.children !== undefined && props.children !== null && props.children !== false;
  const open = expandable && props.open;
  const Icon = props.icon;
  const head = (
    <CollapsibleTrigger className="vk-tool-head" title={props.targetTitle}>
      <span className="vk-tool-icon"><Icon aria-hidden="true" /></span>
      <span className="vk-tool-title">
        <b>{props.verb}</b>
        {props.target ? <code>{props.target}</code> : null}
      </span>
      <span className="vk-tool-meta">
        {props.meta}
        {props.call?.request ? <ToolRequestMark request={props.call.request} state={props.state} /> : null}
      </span>
      <ToolStateMark state={props.state} />
      {expandable ? <ChevronRight aria-hidden="true" className="vk-tool-chev" /> : <span />}
    </CollapsibleTrigger>
  );
  return (
    <Collapsible
      className="vk-tool"
      data-family={props.family}
      data-open={open ? "" : undefined}
      data-request={props.call?.request ? "" : undefined}
      data-state={props.state}
      disabled={!expandable}
      onOpenChange={props.onOpenChange}
      open={open}
    >
      {openFile && props.filePath
        ? <div className="vk-tool-row">{head}<ToolFileAction lines={props.fileLines ?? null} open={openFile} path={props.filePath} /></div>
        : head}
      {expandable ? <CollapsibleContent className="vk-tool-body">{props.children}</CollapsibleContent> : null}
    </Collapsible>
  );
}
