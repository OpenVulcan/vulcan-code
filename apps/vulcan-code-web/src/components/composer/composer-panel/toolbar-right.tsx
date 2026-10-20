import type { ReactNode } from "react";
import { LoaderCircle, Send, Square } from "lucide-react";
import { ContextUsageOrb } from "@/components/composer/composer-controls";
import type {
  ContextCompactionWindowNodeId,
  ContextUsageSnapshot,
  PublicAiContextCompactionWindowNode,
  SessionProviderUsageSummary,
} from "@/types";

// ComposerPanelToolbarRightProps carries the context, model and send/stop controls for the composer toolbar.
// ComposerPanelToolbarRightProps 承载输入区工具栏的上下文、模型与发送/停止控件。
export interface ComposerPanelToolbarRightProps {
  // Exact draft owner; changing it dismisses selectors and isolates pending capacity mutations.
  // 精确草稿所有者；变更时关闭选择器，并隔离未完成的容量修改。
  draftSlotKey: string;
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
  // Model, account and reasoning controls placed between the context orb and the send button.
  // 放在上下文圆环与发送按钮之间的模型、账号与思考控件。
  modelControls?: ReactNode;
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
  const draftHasContent = props.draftInput.trim().length > 0;
  const hasAttachments = props.attachmentCount > 0;
  const stopOnly = props.isConversationRunning && !draftHasContent && !hasAttachments;
  return (
    <div className="vk-composer-end">
      <ContextUsageOrb
        key={`context:${props.draftSlotKey}`}
        contextCompactionWindowNodes={props.contextCompactionWindowNodes}
        modelContextLimit={props.modelContextLimit}
        onNodeChange={props.onSetSessionContextCompactionWindow}
        selectedNodeId={props.sessionContextCompactionWindowNodeId}
        snapshot={props.contextUsage}
        sessionProviderUsage={props.sessionProviderUsage}
      />
      {props.modelControls}
      <button
        aria-label={stopOnly ? props.stopAriaLabel : props.sendAriaLabel}
        className="vk-send"
        data-state={stopOnly ? "stop" : !draftHasContent && !hasAttachments ? "empty" : undefined}
        aria-busy={stopOnly && props.stopPending}
        disabled={stopOnly ? props.stopPending : props.sendBlocked}
        onClick={props.onSend}
        type="button"
      >
        {stopOnly ? (props.stopPending
          ? <LoaderCircle aria-hidden="true" className="vk-spin" size={18} />
          : <Square fill="currentColor" size={15} strokeWidth={0} />) : <Send size={18} />}
      </button>
    </div>
  );
}
