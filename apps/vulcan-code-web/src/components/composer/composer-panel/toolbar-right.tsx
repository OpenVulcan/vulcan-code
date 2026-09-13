import { Brain, LoaderCircle, Send, Square } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  ComposerSelectControl,
  ContextUsageOrb,
} from "@/components/composer/composer-controls";
import type { ComposerSelectOption } from "@/components/composer/composer-model";
import { useI18n } from "@/i18n";
import { cn } from "@/lib/utils";
import type {
  ContextCompactionWindowNodeId,
  ContextUsageSnapshot,
  PublicAiContextCompactionWindowNode,
  SessionProviderUsageSummary,
} from "@/types";

// ComposerPanelToolbarRightProps carries the send/stop and reasoning controls for the composer toolbar.
// ComposerPanelToolbarRightProps 承载输入区工具栏的发送/停止与推理控件。
export interface ComposerPanelToolbarRightProps {
  // Exact draft owner; changing it dismisses selectors and isolates pending capacity mutations.
  // 精确草稿所有者；变更时关闭选择器，并隔离未完成的容量修改。
  draftSlotKey: string;
  // Accessible label for the reasoning selector.
  // 推理选择器的无障碍标签。
  reasoningAriaLabel: string;
  // Fallback text for select controls.
  // 选择控件的回退文本。
  selectFallbackLabel: string;
  // Accessible label for the stop button.
  // 停止按钮无障碍标签。
  stopAriaLabel: string;
  // Accessible label for the send button.
  // 发送按钮无障碍标签。
  sendAriaLabel: string;
  // Current context usage snapshot.
  // 当前上下文用量快照。
  contextUsage: ContextUsageSnapshot | null;
  // Session-level Provider accounting shown inside the context-usage card.
  // 会话级 Provider 统计在上下文占用卡片内展示。
  sessionProviderUsage?: SessionProviderUsageSummary | null;
  // Fixed context-compaction nodes projected by the backend.
  // 后端投影出的固定上下文压缩节点。
  contextCompactionWindowNodes: PublicAiContextCompactionWindowNode[];
  // Stable node selected by the current session.
  // 当前会话选择的稳定节点。
  sessionContextCompactionWindowNodeId: ContextCompactionWindowNodeId | null;
  // Hard context limit declared by the active model.
  // 当前模型声明的硬上下文窗口。
  modelContextLimit: number | null;
  // Whether the selected conversation is running.
  // 当前选中会话是否正在运行。
  isConversationRunning: boolean;
  // Current draft text.
  // 当前输入草稿文本。
  draftInput: string;
  // Current attachment count.
  // 当前附件数量。
  attachmentCount: number;
  // Whether unresolved capability choices block a new submission.
  // 未解决的能力选择是否阻止新提交。
  sendBlocked: boolean;
  // Whether this conversation's stop request is awaiting acknowledgement or confirmation.
  // 此会话的停止请求是否正在等待回应或确认。
  stopPending: boolean;
  // Reasoning selector open signal.
  // 推理选择器打开信号。
  reasoningPickerOpenSignal: number;
  // Reasoning selector options.
  // 推理选择器选项。
  reasoningEffortOptions: ComposerSelectOption[];
  // Current reasoning effort value.
  // 当前推理强度值。
  reasoningEffort: string;
  // Handler for reasoning effort changes.
  // 推理强度变化处理器。
  onReasoningEffortChange: (value: string) => void;
  // Persists or clears one session context-compaction window node.
  // 持久化或清除一个会话上下文压缩窗口节点。
  onSetSessionContextCompactionWindow: (nodeId: ContextCompactionWindowNodeId | null) => Promise<boolean>;
  // Handler that sends or stops according to current composer state.
  // 根据当前输入区状态发送或停止的处理器。
  onSend: () => void;
}

// ComposerPanelToolbarRight renders the right side of the composer toolbar.
// ComposerPanelToolbarRight 渲染输入区工具栏右侧区域。
export function ComposerPanelToolbarRight(props: ComposerPanelToolbarRightProps) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  const draftHasContent = props.draftInput.trim().length > 0;
  const hasAttachments = props.attachmentCount > 0;
  const stopOnly = props.isConversationRunning && !draftHasContent && !hasAttachments;
  return (
    <div className="composer-toolbar-right">
      <ContextUsageOrb
        key={`context:${props.draftSlotKey}`}
        contextCompactionWindowNodes={props.contextCompactionWindowNodes}
        modelContextLimit={props.modelContextLimit}
        onNodeChange={props.onSetSessionContextCompactionWindow}
        selectedNodeId={props.sessionContextCompactionWindowNodeId}
        snapshot={props.contextUsage}
        sessionProviderUsage={props.sessionProviderUsage}
      />
      {props.reasoningEffortOptions.length > 0 ? (
        <ComposerSelectControl
          key={`reasoning:${props.draftSlotKey}`}
          ariaLabel={props.reasoningAriaLabel}
          className="reasoning no-indicator"
          disabled={props.reasoningEffortOptions.length === 1}
          fallbackLabel={props.selectFallbackLabel}
          hideTriggerSubtitle
          icon={<Brain size={15} />}
          onValueChange={props.onReasoningEffortChange}
          openSignal={props.reasoningPickerOpenSignal}
          options={props.reasoningEffortOptions}
          tooltip={<strong>{t("composer.reasoning.tooltip")}</strong>}
          value={props.reasoningEffort}
        />
      ) : null}
      <Button
        aria-label={stopOnly ? props.stopAriaLabel : props.sendAriaLabel}
        className={cn("send-button", stopOnly && "running")}
        aria-busy={stopOnly && props.stopPending}
        disabled={stopOnly ? props.stopPending : props.sendBlocked}
        onClick={props.onSend}
        size="icon"
        type="button"
      >
        {stopOnly ? (props.stopPending
          ? <LoaderCircle aria-hidden="true" className="animate-spin" size={18} />
          : <Square fill="currentColor" size={15} strokeWidth={0} />) : <Send size={18} />}
      </Button>
    </div>
  );
}
