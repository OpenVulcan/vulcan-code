import React from "react";
import { toast } from "sonner";
import { useI18n } from "@/i18n";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";

// ConversationTextContextMenuProps binds text actions to the visible transcript, not the whole page.
// ConversationTextContextMenuProps 将文本操作限定在可见会话转录，而不是整个页面。
interface ConversationTextContextMenuProps {
  // enabled keeps ordinary browser clients on their full native link and media context menus.
  // enabled 使普通网页客户端继续使用包含链接与媒体操作的完整浏览器原生右键菜单。
  enabled: boolean;
  // children is the existing scroll-content element used as the right-click target.
  // children 是作为右键目标的既有滚动内容元素。
  children: React.ReactElement;
  // timelineRef identifies the exact transcript text selected by Select All.
  // timelineRef 标识“全选”作用的精确转录文本范围。
  timelineRef: React.RefObject<HTMLElement | null>;
  // onCopyText writes selected text through the existing workspace clipboard path.
  // onCopyText 通过既有工作区剪贴板路径写入选中文本。
  onCopyText: (text: string) => Promise<void> | void;
}

// ConversationTextContextMenu adds Copy and transcript-scoped Select All to the conversation.
// ConversationTextContextMenu 为会话增加“复制”和仅限转录内容的“全选”。
export function ConversationTextContextMenu(props: ConversationTextContextMenuProps) {
  // t resolves the current client's menu labels and clipboard failure message.
  // t 解析当前客户端的菜单文案与剪贴板失败提示。
  const { t } = useI18n();
  // selectedText retains the selection before Radix moves focus into the menu.
  // selectedText 在 Radix 将焦点移入菜单前保留已选文本。
  const [selectedText, setSelectedText] = React.useState("");

  // captureSelection accepts only a selection fully owned by the visible timeline.
  // captureSelection 只接收完全属于当前可见时间线的文本选区。
  const captureSelection = React.useCallback(() => {
    const timeline = props.timelineRef.current;
    const selection = window.getSelection();
    const anchoredInside = timeline && selection?.anchorNode && selection.focusNode
      && timeline.contains(selection.anchorNode) && timeline.contains(selection.focusNode);
    setSelectedText(anchoredInside ? selection.toString() : "");
  }, [props.timelineRef]);

  // copySelection reports clipboard refusal without claiming that text was copied.
  // copySelection 在剪贴板拒绝写入时显式报告失败，不误报复制成功。
  const copySelection = React.useCallback(async () => {
    if (!selectedText) return;
    try {
      await props.onCopyText(selectedText);
    } catch (error) {
      toast.error(t("conversation.control.copyFailed"), {
        description: error instanceof Error ? error.message : String(error),
      });
    }
  }, [props.onCopyText, selectedText, t]);

  // selectTranscript selects only the visible timeline, leaving the composer and sidebar out.
  // selectTranscript 只选择可见时间线，不包含输入区和侧边栏。
  const selectTranscript = React.useCallback(() => {
    const timeline = props.timelineRef.current;
    const selection = window.getSelection();
    if (!timeline || !selection) return;
    const range = document.createRange();
    range.selectNodeContents(timeline);
    selection.removeAllRanges();
    selection.addRange(range);
    setSelectedText(selection.toString());
  }, [props.timelineRef]);

  if (!props.enabled) return props.children;

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild onContextMenuCapture={captureSelection}>
        {props.children}
      </ContextMenuTrigger>
      <ContextMenuContent onCloseAutoFocus={(event) => event.preventDefault()}>
        <ContextMenuItem disabled={!selectedText} onSelect={() => void copySelection()}>
          {t("conversation.contextMenu.copy")}
        </ContextMenuItem>
        <ContextMenuItem onSelect={selectTranscript}>
          {t("conversation.contextMenu.selectAll")}
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  );
}
