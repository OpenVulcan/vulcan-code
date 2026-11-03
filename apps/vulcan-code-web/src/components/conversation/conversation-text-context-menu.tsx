import React from "react";
import { toast } from "@/ui-kit/primitives/toast";
import { useI18n } from "@/i18n";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/ui-kit/primitives/context-menu";
import { useWorkspaceFileOpener, type WorkspaceFileLines } from "@/components/workspace/workspace-file-open-request";
import { localFileActionsAvailable, readFileReference } from "@/components/workspace/workspace-file-actions";

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
  // onQuoteText hands selected text to the composer as a quote the user can annotate before sending.
  // onQuoteText 将选中文本作为引用交给输入区，用户可在发送前为其添加标注。
  onQuoteText: (text: string) => void;
}

// ConversationTextContextMenu adds text/link copying, quoting into the composer and transcript-scoped Select
// All to the conversation; on a file reference it also offers to show the file in the side panel, open it with the
// default program, show it in its folder or copy its path.
// ConversationTextContextMenu 为会话增加文本与链接复制、引用到输入区，以及仅限转录内容的“全选”；在文件引用上还提供
// 在侧边栏显示、用默认程序打开、在文件夹中显示与复制路径。
export function ConversationTextContextMenu(props: ConversationTextContextMenuProps) {
  // t resolves the current client's menu labels and clipboard failure message.
  // t 解析当前客户端的菜单文案与剪贴板失败提示。
  const { t } = useI18n();
  // selectedText retains the selection before Radix moves focus into the menu.
  // selectedText 在 Radix 将焦点移入菜单前保留已选文本。
  const [selectedText, setSelectedText] = React.useState("");
  // linkAddress captures the clicked anchor before menu focus changes the DOM selection.
  // linkAddress 在菜单焦点改变 DOM 选区前保存被点击链接的地址。
  const [linkAddress, setLinkAddress] = React.useState("");
  // fileReference captures the clicked file reference; file actions need the workbench's file opener.
  // fileReference 保存被点击的文件引用；文件操作需要工作台的文件打开动作。
  const [fileReference, setFileReference] = React.useState<{ path: string; lines: WorkspaceFileLines | null } | null>(null);
  const openFile = useWorkspaceFileOpener();

  // captureSelection accepts only a selection fully owned by the visible timeline.
  // captureSelection 只接收完全属于当前可见时间线的文本选区。
  const captureSelection = React.useCallback((event: React.MouseEvent<HTMLElement>) => {
    const timeline = props.timelineRef.current;
    // Anchor lookup includes nested text and SVG icons but never adjacent workspace controls.
    // 链接查找包含嵌套文字与 SVG 图标，但不包含相邻工作台控件。
    const anchor = timeline && event.target instanceof Element && timeline.contains(event.target)
      ? event.target.closest<HTMLAnchorElement>("a[href]") : null;
    // Empty sanitized hrefs are not links to the current page and must not be offered for copying.
    // 清理后的空 href 不表示当前页面链接，不应提供复制。
    setLinkAddress(anchor && anchor.getAttribute("href")?.trim() ? anchor.href : "");
    setFileReference(timeline && event.target instanceof Element && timeline.contains(event.target)
      ? readFileReference(event.target) : null);
    const selection = window.getSelection();
    const anchoredInside = timeline && selection?.anchorNode && selection.focusNode
      && timeline.contains(selection.anchorNode) && timeline.contains(selection.focusNode);
    setSelectedText(anchoredInside ? selection.toString() : "");
  }, [props.timelineRef]);

  // copyText writes the captured selection or URL and reports failure without claiming success.
  // copyText 写入捕获的选区或 URL，失败时明确报告，不误报成功。
  // text is the menu's captured value; resolves after the shared clipboard path completes.
  // text 是菜单捕获的值；共享剪贴板路径完成后返回。
  const copyText = React.useCallback(async (text: string) => {
    if (!text) return;
    try {
      await props.onCopyText(text);
    } catch (error) {
      toast.error(t("conversation.control.copyFailed"), {
        description: error instanceof Error ? error.message : String(error),
      });
    }
  }, [props.onCopyText, t]);

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
        {fileReference && openFile ? <>
          <ContextMenuItem onSelect={() => openFile(fileReference.path, fileReference.lines)}>
            {t("fileActions.open")}
          </ContextMenuItem>
          {localFileActionsAvailable() ? <>
            <ContextMenuItem onSelect={() => openFile(fileReference.path, fileReference.lines, "open")}>
              {t("fileActions.openWithDefaultApp")}
            </ContextMenuItem>
            <ContextMenuItem onSelect={() => openFile(fileReference.path, fileReference.lines, "reveal")}>
              {t("fileActions.showInFolder")}
            </ContextMenuItem>
          </> : null}
          <ContextMenuItem onSelect={() => void copyText(fileReference.path)}>
            {t("fileActions.copyPath")}
          </ContextMenuItem>
          <ContextMenuSeparator />
        </> : null}
        {linkAddress ? <ContextMenuItem onSelect={() => void copyText(linkAddress)}>
          {t("conversation.contextMenu.copyLinkAddress")}
        </ContextMenuItem> : null}
        <ContextMenuItem disabled={!selectedText} onSelect={() => void copyText(selectedText)}>
          {t("conversation.contextMenu.copy")}
        </ContextMenuItem>
        <ContextMenuItem disabled={!selectedText.trim()} onSelect={() => props.onQuoteText(selectedText)}>
          {t("conversation.contextMenu.quote")}
        </ContextMenuItem>
        <ContextMenuItem onSelect={selectTranscript}>
          {t("conversation.contextMenu.selectAll")}
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  );
}
