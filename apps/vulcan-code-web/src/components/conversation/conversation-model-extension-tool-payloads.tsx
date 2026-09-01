import * as React from "react";
import { ChevronDown, ChevronRight, LogIn, LogOut } from "lucide-react";

import { MarkdownMessage } from "@/components/conversation/conversation-markdown";
import {
  ConversationMeasuredCollapse,
  useConversationMeasuredExpansion,
} from "@/components/conversation/conversation-measured-collapse";
import { useI18n, type TranslationKey } from "@/i18n";
import { cn } from "@/lib/utils";

// ModelExtensionToolPayloadKind names the two independently expandable extension payload cards.
// ModelExtensionToolPayloadKind 命名两个可独立展开的扩展载荷卡片。
type ModelExtensionToolPayloadKind = "input" | "output";

// ModelExtensionToolPayloadCardsProps describes the authoritative input and output retained for one extension call.
// ModelExtensionToolPayloadCardsProps 描述单次扩展调用保留的权威输入与返回。
export interface ModelExtensionToolPayloadCardsProps {
  // Stable parent presentation identity used to isolate both expansion memories.
  // 用于隔离两个展开记忆的稳定父展示标识。
  presentationId: string;
  // Persisted request payload formatted as Markdown.
  // 格式化为 Markdown 的持久化请求载荷。
  inputMarkdown: string;
  // Persisted result payload formatted as Markdown.
  // 格式化为 Markdown 的持久化返回载荷。
  outputMarkdown: string;
  // Optional result-owned controls or generated resources.
  // 可选的返回侧控件或生成资源。
  outputSupplement?: React.ReactNode;
  // Initial input expansion; omitted means folded.
  // 输入初始展开状态；省略时为折叠。
  defaultInputExpanded?: boolean;
  // Initial output expansion; omitted means expanded once output exists.
  // 输出初始展开状态；省略时在返回存在后展开。
  defaultOutputExpanded?: boolean;
}

// ModelExtensionToolPayloadCardProps describes one independently controlled payload card.
// ModelExtensionToolPayloadCardProps 描述一个独立控制的载荷卡片。
interface ModelExtensionToolPayloadCardProps {
  // Stable parent presentation identity.
  // 稳定父展示标识。
  presentationId: string;
  // Input or output discriminator.
  // 输入或返回判别字段。
  kind: ModelExtensionToolPayloadKind;
  // Localized card title key.
  // 本地化卡片标题键。
  titleKey: TranslationKey;
  // Markdown payload rendered inside the bounded body.
  // 在受限高正文中渲染的 Markdown 载荷。
  markdown: string;
  // Optional content owned by this payload.
  // 当前载荷持有的可选内容。
  supplement?: React.ReactNode;
  // Initial expansion when no surface memory exists.
  // 界面记忆不存在时的初始展开状态。
  defaultExpanded: boolean;
}

// ModelExtensionToolPayloadCards renders request and result as separate folded cards.
// ModelExtensionToolPayloadCards 将请求与返回渲染成两个独立折叠卡片。
// Parameters: props contains the stable call identity and exact persisted payloads.
// 参数：props 包含稳定调用身份与精确持久化载荷。
// Returns: the available input card and, after a result exists, the output card.
// 返回值：可用的输入卡，以及返回存在后显示的返回卡。
export function ModelExtensionToolPayloadCards(props: ModelExtensionToolPayloadCardsProps) {
  return (
    <div className="model-extension-tool-payloads">
      <ModelExtensionToolPayloadCard
        defaultExpanded={props.defaultInputExpanded ?? false}
        kind="input"
        markdown={props.inputMarkdown}
        presentationId={props.presentationId}
        titleKey="toolPresentation.payload.input"
      />
      <ModelExtensionToolPayloadCard
        defaultExpanded={props.defaultOutputExpanded ?? true}
        kind="output"
        markdown={props.outputMarkdown}
        presentationId={props.presentationId}
        supplement={props.outputSupplement}
        titleKey="toolPresentation.payload.output"
      />
    </div>
  );
}

// ModelExtensionToolPayloadCard renders one payload with its own remembered expansion state.
// ModelExtensionToolPayloadCard 使用自身记忆的展开状态渲染一个载荷。
// Parameters: props identifies the payload, its localized title and retained content.
// 参数：props 标识载荷、本地化标题及其保留内容。
// Returns: null for an unavailable payload, otherwise one independently expandable card.
// 返回值：载荷不可用时返回 null，否则返回一个可独立展开的卡片。
function ModelExtensionToolPayloadCard(props: ModelExtensionToolPayloadCardProps) {
  // hasSupplement distinguishes a real result-owned node from an absent output.
  // hasSupplement 区分真实的返回侧节点与不存在的输出。
  const hasSupplement = props.supplement !== null && props.supplement !== undefined;
  // hasContent prevents an empty output card from appearing while the tool is still running.
  // hasContent 防止工具仍在运行时出现空返回卡。
  const hasContent = Boolean(props.markdown) || hasSupplement;
  // collapseId gives input and output independent persisted expansion identities.
  // collapseId 为输入与返回提供相互独立的持久化展开身份。
  const collapseId = `model-extension:${props.presentationId}:${props.kind}`;
  // expanded is scoped to the exact payload rather than the whole tool call.
  // expanded 仅属于精确载荷，而不是整个工具调用。
  const [expanded, setExpanded] = useConversationMeasuredExpansion(collapseId, props.defaultExpanded);
  // t resolves the input or output label without exposing raw tool ids.
  // t 解析输入或返回标签，且不暴露原始工具标识。
  const { t } = useI18n();
  if (!hasContent) {
    return null;
  }
  // PayloadIcon gives request and result rows distinct in/out semantics instead of a generic JSON glyph.
  // PayloadIcon 为请求行与返回行提供明确的输入/输出语义，而不是通用 JSON 图标。
  const PayloadIcon = props.kind === "input" ? LogIn : LogOut;
  return (
    <section
      className={cn("tool-presentation-card", "markdown-tool", "model-extension-payload-card", expanded && "expanded")}
      data-model-extension-payload={props.kind}
    >
      <button
        aria-expanded={expanded}
        className="tool-presentation-summary model-extension-payload-summary"
        onClick={() => setExpanded(!expanded)}
        type="button"
      >
        <PayloadIcon className="markdown-tool-icon" size={14} />
        <span className="tool-presentation-command">
          <span className="tool-presentation-command-text">{t(props.titleKey)}</span>
        </span>
        {expanded
          ? <ChevronDown className="tool-presentation-toggle" size={14} />
          : <ChevronRight className="tool-presentation-toggle" size={14} />}
      </button>
      <ConversationMeasuredCollapse collapseId={collapseId} expanded={expanded}>
        <div className="markdown-tool-block model-extension-payload-body">
          {props.markdown ? <MarkdownMessage content={props.markdown} eager /> : null}
          {props.supplement}
        </div>
      </ConversationMeasuredCollapse>
    </section>
  );
}
