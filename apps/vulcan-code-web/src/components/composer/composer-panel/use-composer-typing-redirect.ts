import React from "react";
import {
  hasBlockingComposerPopup,
  isComposerPrintableKey,
  isComposerTextEntryElement,
} from "@/components/composer/composer-model";
import { getWorkspaceInputOwner } from "@/components/workspace/workspace-input-owner";

// useComposerTypingRedirect redirects ordinary workspace typing into the composer while no other text entry,
// terminal, modal or blocking composer popup owns the keyboard.
// useComposerTypingRedirect 在没有其他文本输入、终端、模态框或阻塞型输入区弹层占用键盘时，将工作台中的普通键入重定向到输入区。
export function useComposerTypingRedirect(
  textareaRef: React.RefObject<HTMLTextAreaElement | null>,
  pendingPromptEditRef: React.RefObject<boolean>,
  appendGlobalInput: (value: string) => void,
): void {
  React.useEffect(() => {
    // handleGlobalWorkspaceTyping redirects ordinary workspace typing into the composer.
    // handleGlobalWorkspaceTyping 将工作台中的普通键入重定向到输入区。
    const handleGlobalWorkspaceTyping = (event: KeyboardEvent) => {
      if (
        pendingPromptEditRef.current
        || event.defaultPrevented
        || !isComposerPrintableKey(event)
      ) {
        return;
      }
      const inputOwner = getWorkspaceInputOwner();
      if (inputOwner.kind === "project_terminal" || inputOwner.kind === "modal") {
        return;
      }
      // textarea is the unique composer input that receives redirected typing.
      // textarea 是接收重定向输入的唯一输入区文本框。
      const textarea = textareaRef.current;
      if (!textarea || document.activeElement === textarea) {
        return;
      }
      if (hasBlockingComposerPopup() || isComposerTextEntryElement(document.activeElement)) {
        return;
      }
      event.preventDefault();
      textarea.focus({ preventScroll: true });
      appendGlobalInput(event.key);
    };
    document.addEventListener("keydown", handleGlobalWorkspaceTyping);
    return () => document.removeEventListener("keydown", handleGlobalWorkspaceTyping);
  }, [appendGlobalInput, pendingPromptEditRef, textareaRef]);
}
