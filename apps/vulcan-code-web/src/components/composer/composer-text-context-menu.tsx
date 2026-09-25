import React from "react";
import { toast } from "sonner";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { useI18n } from "@/i18n";
import { readElectronDesktopBridge } from "@/host-shell/electron-bridge";

// ComposerTextContextMenuProps binds edit actions to the current controlled draft.
// ComposerTextContextMenuProps 将编辑操作绑定到当前受控草稿。
interface ComposerTextContextMenuProps {
  // children is the real composer textarea and remains the context-menu trigger.
  // children 是实际会话输入框，并保持为右键菜单触发元素。
  children: React.ReactElement;
  // disabled prevents edits while a queued prompt replacement owns the draft.
  // disabled 在排队提示词替换占有草稿期间阻止编辑。
  disabled: boolean;
  // enabled limits the custom menu to the Electron host; web browsers retain their native menu.
  // enabled 将自定义菜单限定于 Electron 宿主；网页浏览器保留原生菜单。
  enabled: boolean;
  // ownerKey identifies the exact conversation draft selected when the menu opens.
  // ownerKey 标识菜单打开时选中的精确会话草稿。
  ownerKey: string;
  // textareaRef points at the controlled input rather than an unrelated page selection.
  // textareaRef 指向受控输入框，而不是页面中的其他选区。
  textareaRef: React.RefObject<HTMLTextAreaElement | null>;
  // value is the current controlled draft text.
  // value 是当前受控草稿文本。
  value: string;
  // onChange enters edits through the existing draft and slash-command pipeline.
  // onChange 让编辑通过既有草稿及斜杠命令处理链进入。
  onChange: (value: string) => void;
}

// CapturedSelection freezes the draft owner and selection before menu focus changes.
// CapturedSelection 在菜单改变焦点前冻结草稿归属和选区。
interface CapturedSelection {
  // ownerKey is the draft owner at right-click time.
  // ownerKey 是右键发生时的草稿归属。
  ownerKey: string;
  // value is the text whose offsets were captured.
  // value 是捕获选区偏移时的文本。
  value: string;
  // start is the first selected UTF-16 offset.
  // start 是选区第一个 UTF-16 偏移。
  start: number;
  // end is the offset immediately after the selection.
  // end 是选区结束位置之后的偏移。
  end: number;
}

// writeClipboardText prefers the owning Electron host and supports older shells through the Web API.
// writeClipboardText 优先使用所属 Electron 宿主；旧版壳通过网页剪贴板 API 兼容。
async function writeClipboardText(value: string): Promise<void> {
  // bridge is the trusted desktop path when this shell version exposes clipboard actions.
  // bridge 是当前壳版本开放剪贴板操作时的受信桌面路径。
  const bridge = readElectronDesktopBridge();
  if (bridge?.writeClipboardText) {
    await bridge.writeClipboardText(value);
    return;
  }
  if (!navigator.clipboard?.writeText) throw new Error("Clipboard write is unavailable.");
  await navigator.clipboard.writeText(value);
}

// readClipboardText prefers the owning Electron host and supports older shells through the Web API.
// readClipboardText 优先使用所属 Electron 宿主；旧版壳通过网页剪贴板 API 兼容。
async function readClipboardText(): Promise<string> {
  // bridge is the trusted desktop path when this shell version exposes clipboard actions.
  // bridge 是当前壳版本开放剪贴板操作时的受信桌面路径。
  const bridge = readElectronDesktopBridge();
  if (bridge?.readClipboardText) return bridge.readClipboardText();
  if (!navigator.clipboard?.readText) throw new Error("Clipboard read is unavailable.");
  return navigator.clipboard.readText();
}

// ComposerTextContextMenu provides Copy, Paste, Cut, and Select All for the real composer field.
// ComposerTextContextMenu 为实际会话输入框提供复制、粘贴、剪切和全选。
export function ComposerTextContextMenu(props: ComposerTextContextMenuProps) {
  // t resolves current-client labels and explicit clipboard failure messages.
  // t 解析当前客户端的菜单文字和明确的剪贴板失败提示。
  const { t } = useI18n();
  // selection preserves the text range while the menu takes focus.
  // selection 在菜单取得焦点时保留文本范围。
  const [selection, setSelection] = React.useState<CapturedSelection | null>(null);
  // current identifies the latest draft so a delayed clipboard operation cannot edit another session.
  // current 标识最新草稿，避免延迟的剪贴板操作修改另一个会话。
  const current = React.useRef({ ownerKey: props.ownerKey, value: props.value, disabled: props.disabled });
  // pendingCaret restores the cursor after React commits a controlled draft edit.
  // pendingCaret 在 React 提交受控草稿编辑后恢复光标。
  const pendingCaret = React.useRef<{ ownerKey: string; value: string; offset: number } | null>(null);

  React.useLayoutEffect(() => {
    current.current = { ownerKey: props.ownerKey, value: props.value, disabled: props.disabled };
  }, [props.disabled, props.ownerKey, props.value]);

  React.useLayoutEffect(() => {
    // pending is the caret target attached to one committed draft value.
    // pending 是绑定到某个已提交草稿值的目标光标位置。
    const pending = pendingCaret.current;
    // field is the current input, which may change when the composer remounts.
    // field 是当前输入框，会话编辑器重新挂载时它可能变化。
    const field = props.textareaRef.current;
    if (!pending || !field || pending.ownerKey !== props.ownerKey || pending.value !== props.value) return;
    field.focus({ preventScroll: true });
    field.setSelectionRange(pending.offset, pending.offset);
    pendingCaret.current = null;
  }, [props.ownerKey, props.textareaRef, props.value]);

  // captureSelection records the real textarea range before Radix moves focus into the menu.
  // captureSelection 在 Radix 把焦点移入菜单前记录实际输入框选区。
  const captureSelection = React.useCallback(() => {
    // field is the only input whose selection may become a composer action.
    // field 是其选区可被转化为会话输入操作的唯一输入框。
    const field = props.textareaRef.current;
    if (!field || props.disabled) return;
    setSelection({
      ownerKey: props.ownerKey,
      value: props.value,
      start: field.selectionStart,
      end: field.selectionEnd,
    });
  }, [props.disabled, props.ownerKey, props.textareaRef, props.value]);

  // isCurrent excludes edits after the user switches drafts or changes text while clipboard I/O waits.
  // isCurrent 排除用户切换草稿或剪贴板等待期间修改文本后的过期编辑。
  const isCurrent = React.useCallback((captured: CapturedSelection): boolean =>
    current.current.ownerKey === captured.ownerKey && current.current.value === captured.value,
  []);

  // reportClipboardFailure displays a failed action instead of silently losing an edit.
  // reportClipboardFailure 显示失败操作，不让编辑静默丢失。
  const reportClipboardFailure = React.useCallback((error: unknown) => {
    toast.error(t("composer.contextMenu.clipboardFailed"), {
      description: error instanceof Error ? error.message : String(error),
    });
  }, [t]);

  // replaceSelection changes the controlled draft and restores its caret after rendering.
  // replaceSelection 修改受控草稿，并在渲染后恢复光标。
  const replaceSelection = React.useCallback((captured: CapturedSelection, replacement: string) => {
    if (!isCurrent(captured) || current.current.disabled) return;
    // next preserves text outside the captured selection and enters the normal draft path.
    // next 保留捕获选区以外的文本，并进入正常草稿处理链。
    const next = captured.value.slice(0, captured.start) + replacement + captured.value.slice(captured.end);
    pendingCaret.current = {
      ownerKey: captured.ownerKey,
      value: next,
      offset: captured.start + replacement.length,
    };
    props.onChange(next);
  }, [isCurrent, props.onChange]);

  // copy writes exactly the selected composer text to the clipboard.
  // copy 只把会话输入框中选中的文本写入剪贴板。
  const copy = React.useCallback(async () => {
    if (!selection || selection.start === selection.end || !isCurrent(selection)) return;
    try {
      await writeClipboardText(selection.value.slice(selection.start, selection.end));
    } catch (error) {
      reportClipboardFailure(error);
    } finally {
      // Explicitly restore focus after async clipboard I/O, even if menu closing is delayed.
      // 异步剪贴板操作结束后显式恢复焦点，即使菜单关闭过程延迟也不影响继续输入。
      const field = props.textareaRef.current;
      if (field && isCurrent(selection)) {
        field.focus({ preventScroll: true });
        field.setSelectionRange(selection.start, selection.end);
      }
    }
  }, [isCurrent, props.textareaRef, reportClipboardFailure, selection]);

  // cut removes selected text only after the clipboard confirms the copy.
  // cut 仅在剪贴板确认写入后删除选中的文本。
  const cut = React.useCallback(async () => {
    if (!selection || selection.start === selection.end || !isCurrent(selection)) return;
    try {
      await writeClipboardText(selection.value.slice(selection.start, selection.end));
      replaceSelection(selection, "");
    } catch (error) {
      reportClipboardFailure(error);
    }
  }, [isCurrent, replaceSelection, reportClipboardFailure, selection]);

  // paste uses native page Paste for attachments and falls back to text for older shells.
  // paste 对附件使用页面原生粘贴；旧版壳回退为纯文本粘贴。
  const paste = React.useCallback(async () => {
    if (!selection || !isCurrent(selection)) return;
    try {
      // bridge can dispatch the native paste event through the existing attachment pipeline.
      // bridge 可通过既有附件处理链派发原生粘贴事件。
      const bridge = readElectronDesktopBridge();
      if (bridge?.pasteClipboard) {
        // field must regain the saved selection before Electron executes the edit command.
        // Electron 执行编辑命令前，field 必须恢复保存的选区。
        const field = props.textareaRef.current;
        if (!field || current.current.disabled) return;
        field.focus({ preventScroll: true });
        field.setSelectionRange(selection.start, selection.end);
        await bridge.pasteClipboard();
        return;
      }
      // text is the plain-text clipboard payload for the active menu action.
      // text 是当前菜单操作的剪贴板纯文本内容。
      const text = await readClipboardText();
      if (text) replaceSelection(selection, text);
    } catch (error) {
      reportClipboardFailure(error);
    }
  }, [isCurrent, props.textareaRef, replaceSelection, reportClipboardFailure, selection]);

  // selectAll selects only the textarea contents and keeps keyboard editing ready.
  // selectAll 仅选中输入框内容，并保持可继续键盘编辑。
  const selectAll = React.useCallback(() => {
    // field stays within the composer even if the surrounding transcript has a selection.
    // field 始终限定于会话输入框，即使周围会话转录已有选区。
    const field = props.textareaRef.current;
    if (!field || props.disabled) return;
    field.focus({ preventScroll: true });
    field.setSelectionRange(0, field.value.length);
  }, [props.disabled, props.textareaRef]);

  if (!props.enabled) return props.children;

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild onContextMenuCapture={captureSelection}>
        {props.children}
      </ContextMenuTrigger>
      <ContextMenuContent onCloseAutoFocus={(event) => {
        // Restore the composer after menu focus leaves so typing can continue immediately.
        // 菜单焦点离开后恢复会话输入框，使用户可以立即继续输入。
        event.preventDefault();
        props.textareaRef.current?.focus({ preventScroll: true });
      }}>
        <ContextMenuItem disabled={props.disabled || !selection || selection.start === selection.end} onSelect={() => void copy()}>
          {t("composer.contextMenu.copy")}
        </ContextMenuItem>
        <ContextMenuItem disabled={props.disabled} onSelect={() => void paste()}>
          {t("composer.contextMenu.paste")}
        </ContextMenuItem>
        <ContextMenuItem disabled={props.disabled || !selection || selection.start === selection.end} onSelect={() => void cut()}>
          {t("composer.contextMenu.cut")}
        </ContextMenuItem>
        <ContextMenuItem disabled={props.disabled} onSelect={selectAll}>
          {t("composer.contextMenu.selectAll")}
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  );
}
