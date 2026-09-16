import * as React from "react";
import { LogIn, LogOut } from "lucide-react";

import { MarkdownMessage } from "@/components/conversation/conversation-markdown";
import { useConversationMeasuredExpansion } from "@/components/conversation/conversation-measured-collapse";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useI18n } from "@/i18n";

// ModelExtensionToolPayloadCardsProps describes the authoritative input and output retained for one tool call.
// ModelExtensionToolPayloadCardsProps 描述单次工具调用保留的权威输入与输出。
export interface ModelExtensionToolPayloadCardsProps {
  // Stable presentation identity scopes the remembered input-tab selection.
  // 稳定展示标识限定输入标签选择记忆的范围。
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
}

// ModelExtensionToolPayloadCards renders available payloads as mutually exclusive, keyboard-accessible tabs.
// ModelExtensionToolPayloadCards 将可用载荷渲染为互斥且支持键盘操作的标签页。
// Parameters: props supplies exact payloads, result-owned resources, and their stable call identity.
// 参数：props 提供精确载荷、返回侧资源及其稳定调用身份。
// Returns: output-first tabs, or only input while output is unavailable; empty payloads render nothing.
// 返回值：输出优先的标签页，输出不可用时仅显示输入；所有载荷均为空时不渲染。
export function ModelExtensionToolPayloadCards(props: ModelExtensionToolPayloadCardsProps) {
  // t resolves the existing localized input and output names.
  // t 解析既有的输入与输出本地化名称。
  const { t } = useI18n();
  // hasInput and hasOutput admit only payloads supplied by the tool's presentation model.
  // hasInput 与 hasOutput 仅接纳工具展示模型提供的载荷。
  const hasInput = Boolean(props.inputMarkdown);
  const hasOutput = Boolean(props.outputMarkdown)
    || (props.outputSupplement !== null && props.outputSupplement !== undefined);
  // inputSelected preserves an explicit user choice across collapse/remount and later output arrival.
  // inputSelected 在折叠重挂载及后续输出到达时保留用户显式选择。
  const [inputSelected, setInputSelected] = useConversationMeasuredExpansion(
    `model-extension:${props.presentationId}:input-tab`, false,
  );
  // selectedTab defaults to output once available without interrupting an explicitly selected input.
  // selectedTab 在输出可用后默认选中输出，但不打断显式选择的输入。
  const selectedTab = hasInput && inputSelected ? "input" : hasOutput ? "output" : "input";
  if (!hasInput && !hasOutput) return null;

  return (
    <Tabs className="model-extension-tool-payloads" value={selectedTab}
      onValueChange={(value) => setInputSelected(value === "input")}>
      <TabsList variant="line" aria-label={`${t("toolPresentation.payload.output")} / ${t("toolPresentation.payload.input")}`}>
        {hasOutput ? <TabsTrigger value="output" data-payload-tab="output">
          <LogOut size={14} />{t("toolPresentation.payload.output")}
        </TabsTrigger> : null}
        {hasInput ? <TabsTrigger value="input" data-payload-tab="input"
          onMouseDown={(event) => { if (event.button === 0 && !event.ctrlKey) setInputSelected(true); }}
          onKeyDown={(event) => { if (event.key === "Enter" || event.key === " ") setInputSelected(true); }}>
          <LogIn size={14} />{t("toolPresentation.payload.input")}
        </TabsTrigger> : null}
      </TabsList>
      {hasOutput ? <TabsContent value="output" data-model-extension-payload="output" className="model-extension-payload-body">
        {props.outputMarkdown ? <MarkdownMessage content={props.outputMarkdown} eager /> : null}
        {props.outputSupplement}
      </TabsContent> : null}
      {hasInput ? <TabsContent value="input" data-model-extension-payload="input" className="model-extension-payload-body">
        <MarkdownMessage content={props.inputMarkdown} eager />
      </TabsContent> : null}
    </Tabs>
  );
}
