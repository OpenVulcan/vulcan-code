import React from "react";
import { toast } from "@/ui-kit/primitives/toast";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from "@/ui-kit/primitives/context-menu";
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

// COMPOSER_MENU_ITEM_HOVER_CLASS_NAME highlights items by pointer when they never take focus.
// COMPOSER_MENU_ITEM_HOVER_CLASS_NAME 在菜单项不获取焦点时以指针悬停高亮。
const COMPOSER_MENU_ITEM_HOVER_CLASS_NAME = "hover:bg-(--vcn-fill-hover) hover:text-foreground";

// isContextMenuKey reports the keyboard shortcuts that open a context menu on Windows and Linux.
// isContextMenuKey 判断 Windows 与 Linux 上打开右键菜单的键盘快捷键。
function isContextMenuKey(event: React.KeyboardEvent): boolean {
  return event.key === "ContextMenu" || (event.shiftKey && event.key === "F10");
}

// keepPointerFocusInComposer stops Radix from focusing items on hover or the menu on leave.
// keepPointerFocusInComposer 阻止 Radix 在悬停时聚焦菜单项、离开时聚焦菜单本身。
function keepPointerFocusInComposer(event: React.PointerEvent<HTMLDivElement>) {
  event.preventDefault();
}

// ComposerMenuItemProps describes one action row and who owns focus while the menu is open.
// ComposerMenuItemProps 描述一行操作项，以及菜单打开期间由谁持有焦点。
interface ComposerMenuItemProps {
  // disabled blocks the action for an empty selection or a locked draft.
  // disabled 在选区为空或草稿被锁定时阻止该操作。
  disabled: boolean;
  // keyboard lets Radix move focus between rows for arrow-key navigation.
  // keyboard 允许 Radix 在菜单行之间移动焦点以支持方向键导航。
  keyboard: boolean;
  // onSelect runs the edit action.
  // onSelect 执行编辑操作。
  onSelect: () => void;
  // children is the translated row label.
  // children 是已翻译的菜单行文字。
  children: React.ReactNode;
}

// ComposerMenuItem is one action row; for pointer menus it leaves focus and the selection in the composer.
// ComposerMenuItem 是一行操作项；指针打开的菜单中焦点与选区始终留在会话输入框。
function ComposerMenuItem(props: ComposerMenuItemProps) {
  return (
    <ContextMenuItem
      className={props.keyboard ? undefined : COMPOSER_MENU_ITEM_HOVER_CLASS_NAME}
      disabled={props.disabled}
      onPointerLeave={props.keyboard ? undefined : keepPointerFocusInComposer}
      onPointerMove={props.keyboard ? undefined : keepPointerFocusInComposer}
      onSelect={props.onSelect}
    >
      {props.children}
    </ContextMenuItem>
  );
}

// ComposerTextContextMenu provides Copy, Paste, Cut, and Select All for the real composer field.
// ComposerTextContextMenu 为实际会话输入框提供复制、粘贴、剪切和全选。
// The menu is non-modal. Opened by pointer it never holds focus, so the textarea keeps its visible
// selection like the native editing menu; opened by keyboard it takes focus for arrow-key navigation.
// 菜单为非模态。指针打开时从不持有焦点，输入框如原生编辑菜单一样保留可见选区；键盘打开时由菜单接手
// 焦点以支持方向键导航。
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
  // keyboardRequest is set by a context-menu key and cleared by any other key or pointer press,
  // because a pointer-opened menu always receives pointerdown before contextmenu.
  // keyboardRequest 由右键菜单键设置，任何其他按键或指针按下都会清除；指针打开菜单时 pointerdown 总在
  // contextmenu 之前到达。
  const keyboardRequest = React.useRef(false);
  // keyboardMenu records whether the open menu was requested by keyboard and therefore owns focus.
  // keyboardMenu 记录当前打开的菜单是否由键盘请求，从而持有焦点。
  const [keyboardMenu, setKeyboardMenu] = React.useState(false);

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
    setKeyboardMenu(keyboardRequest.current);
    keyboardRequest.current = false;
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
    // modal=false removes the focus trap that would bounce field.focus() back into the menu.
    // modal=false 移除焦点陷阱，否则 field.focus() 会被弹回菜单。
    <ContextMenu modal={false}>
      <ContextMenuTrigger
        asChild
        onContextMenuCapture={captureSelection}
        onKeyDownCapture={(event) => { keyboardRequest.current = isContextMenuKey(event); }}
        onPointerDownCapture={() => { keyboardRequest.current = false; }}
      >
        {props.children}
      </ContextMenuTrigger>
      <ContextMenuContent
        onCloseAutoFocus={(event) => {
          // Restore the composer after the menu closes so typing can continue immediately.
          // 菜单关闭后恢复会话输入框，使用户可以立即继续输入。
          event.preventDefault();
          props.textareaRef.current?.focus({ preventScroll: true });
        }}
        onFocus={() => {
          // Radix focuses the menu when it opens; for pointer menus hand focus back so the selection
          // stays painted. Keyboard menus keep focus so arrow keys and Enter drive the rows.
          // Radix 打开菜单时会聚焦菜单；指针菜单把焦点交还输入框，使选区保持可见。键盘菜单保留焦点，
          // 由方向键与 Enter 操作菜单行。
          if (!keyboardMenu) props.textareaRef.current?.focus({ preventScroll: true });
        }}
        onFocusOutside={(event) => {
          // Focus returning to the composer is expected and must not dismiss the open menu. The
          // hand-back above also leaves a stale event targeting the menu, so judge by live focus.
          // 焦点回到会话输入框属于预期行为，不得关闭已打开的菜单。上面的交还焦点还会留下一个以菜单为
          // 目标的过期事件，因此按实时焦点判断。
          const field = props.textareaRef.current;
          if (field && field.ownerDocument.activeElement === field) event.preventDefault();
        }}
        onMouseDown={(event) => {
          // Clicking a row must not blur the textarea before the action reads its selection.
          // 点击菜单行不得在操作读取选区前使输入框失焦。
          event.preventDefault();
        }}
      >
        <ComposerMenuItem disabled={props.disabled || !selection || selection.start === selection.end} keyboard={keyboardMenu} onSelect={() => void copy()}>
          {t("composer.contextMenu.copy")}
        </ComposerMenuItem>
        <ComposerMenuItem disabled={props.disabled} keyboard={keyboardMenu} onSelect={() => void paste()}>
          {t("composer.contextMenu.paste")}
        </ComposerMenuItem>
        <ComposerMenuItem disabled={props.disabled || !selection || selection.start === selection.end} keyboard={keyboardMenu} onSelect={() => void cut()}>
          {t("composer.contextMenu.cut")}
        </ComposerMenuItem>
        <ComposerMenuItem disabled={props.disabled} keyboard={keyboardMenu} onSelect={selectAll}>
          {t("composer.contextMenu.selectAll")}
        </ComposerMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  );
}
