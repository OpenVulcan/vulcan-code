// Shared building blocks for tool presentation cards: result tabs, request progress, detail loading and file labels.
// 工具展示卡共享构件：结果标签、请求进度、详情加载与文件标签。
import * as React from "react";
import type { ToolCallEvent } from "@/service/conversation-event-display-projection";
import type { ConversationDetailRef, ToolDetailPayload } from "@/components/conversation/conversation-detail";
import { ConversationToolResultTabs } from "../conversation-tool-result-tabs";
import type { FileAccessLineRangePresentation, FileChangeFilePresentation, ToolPresentationDisplayState } from "@/components/conversation/conversation-tool-presentations";
import type { TranslationKey } from "@/i18n";

// The exact event is shared with expanded bodies without rebuilding payloads from summaries.
// 精确事件与展开正文共享，不从摘要重建载荷。
export const ToolResultEventContext = React.createContext<ToolCallEvent | undefined>(undefined);

/** Wraps result-oriented bodies with canonical tabs while preserving dedicated controls and source cards.
 * 为结果型正文包裹规范标签，同时保留专属控制与源码卡。
 * Parameter children is the original specialized renderer; returns that renderer or its tabbed shell.
 * 参数 children 为原专属渲染器；返回该渲染器或其标签外壳。 */
export function ToolResultBody(props: { children: React.ReactNode }) {
  // event is absent only on the supported legacy rendering path.
  // event 仅在受支持的旧版渲染路径缺失。
  const event = React.useContext(ToolResultEventContext);
  return event ? <ConversationToolResultTabs event={event}>{props.children}</ConversationToolResultTabs> : <>{props.children}</>;
}

// ToolRequestCardProgress carries request-state text without replacing the resolved presentation model.
// ToolRequestCardProgress 在不替换已解析展示模型的前提下携带请求状态文案。
export interface ToolRequestCardProgress {
  // Exact tool identity already discovered before arguments are available.
  // 参数可用前已经发现的精确工具身份。
  toolName: string;
  // Whether the card is still in the request phase and may use a stable activity treatment.
  // 卡片是否仍处于请求阶段并可使用稳定的活动态样式。
  receiving: boolean;
  // Request-receiving state that overrides premature execution wording.
  // 覆盖过早执行文案的请求接收状态。
  statusLabel: string;
}

// ToolDetailLoader resolves one authoritative hidden detail record after tool completion.
// ToolDetailLoader 在工具完成后解析单条权威隐藏详情记录。
export type ToolDetailLoader = (
  detailRef: ConversationDetailRef,
) => Promise<ToolDetailPayload | null>;

// activeToolStatusLabel keeps transient progress visible and removes generic terminal success or failure labels.
// activeToolStatusLabel 保留瞬时进度，并移除通用终态成功或失败文案。
// Parameters: requestProgress owns request formation, state owns execution, and runningLabel is card-specific copy.
// 参数：requestProgress 持有请求构造，state 持有执行状态，runningLabel 是卡片专用文案。
// Returns: a transient status label, or null after the tool reaches any terminal state.
// 返回值：返回瞬时状态文案；工具进入任意终态后返回 null。
export function activeToolStatusLabel(
  requestProgress: ToolRequestCardProgress | null,
  state: ToolPresentationDisplayState,
  runningLabel: string,
): string | null {
  if (requestProgress) {
    return requestProgress.statusLabel;
  }
  return state === "running" ? runningLabel : null;
}

// fileTargetLabel combines one exact path with zero or more structured line ranges.
// fileTargetLabel 将一个精确路径与零个或多个结构化行范围组合。
export function fileTargetLabel(
  path: string,
  lineRanges: FileAccessLineRangePresentation[],
  t: (key: TranslationKey) => string,
): string {
  // rangeLabel localizes the structured ranges only at the final render boundary.
  // rangeLabel 仅在最终渲染边界对结构化范围执行本地化。
  const rangeLabel = lineRanges
    .map((range) => range.endLine === null
      ? formatTranslation(t, "toolPresentation.fileRead.rangeFrom", { start: String(range.startLine) })
      : formatTranslation(t, "toolPresentation.fileRead.range", {
          end: String(range.endLine),
          start: String(range.startLine),
        }))
    .join(", ");
  return rangeLabel ? `${path} · ${rangeLabel}` : path;
}

// FileTextFormatBadge renders encoding and line-ending facts without duplicating gutter ranges.
// FileTextFormatBadge 渲染编码与换行事实，且不重复代码 gutter 的范围。
export function FileTextFormatBadge(props: {
  // File-like row whose native text metadata may be available.
  // 可能包含原生文本元数据的类文件行。
  file: Pick<FileChangeFilePresentation, "textFormat">;
  // Translates stable UI copy keys.
  // 翻译稳定 UI 文案键。
  t: (key: TranslationKey) => string;
}) {
  const format = props.file.textFormat;
  if (!format) {
    return null;
  }
  const tokens = [
    format.encoding,
    format.mixedLineEndings
      ? props.t("toolPresentation.file.mixedLineEndings")
      : format.lineEnding,
    format.bom ? "BOM" : null,
  ].filter((value): value is string => Boolean(value));
  return tokens.length
    ? <span className="file-change-text-format">{tokens.join(" · ")}</span>
    : null;
}

// FileChangeErrorDetail renders failed native vulcan-file details without opening the generic modal.
// FileChangeErrorDetail 渲染失败原生 vulcan-file 详情，避免打开通用弹窗。
export function FileChangeErrorDetail(props: {
  // Error detail text.
  // 错误详情文本。
  detail: string;
}) {
  return (
    <div className="file-change-detail error">
      <pre className="file-change-error-text">{props.detail}</pre>
    </div>
  );
}

// formatTranslation replaces simple named placeholders in localized UI templates.
// formatTranslation 替换本地化 UI 模板中的简单具名占位符。
export function formatTranslation(
  t: (key: TranslationKey) => string,
  key: TranslationKey,
  values: Record<string, string>,
): string {
  return Object.entries(values).reduce(
    (text, [name, value]) => text.replaceAll(`{${name}}`, value),
    t(key),
  );
}
