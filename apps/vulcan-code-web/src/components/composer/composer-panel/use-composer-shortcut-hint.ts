import { useId, useSyncExternalStore } from "react";
import {
  oppositeComposerSendMode,
  readComposerEnterSendMode,
  subscribeComposerEnterSendMode,
} from "@/components/composer/composer-send-preference";
import type { useI18n } from "@/i18n";

// Return the placeholder and optional note for the current run state and draft text.
// 返回当前运行状态与草稿文本对应的占位提示和可选说明。
// The caller supplies its translator so the hint uses the same locale as the rest of the composer.
// 调用方提供翻译函数，使提示与输入区其他内容使用同一语言。
export function useComposerShortcutHint(
  isRunning: boolean,
  hasDraftText: boolean,
  t: ReturnType<typeof useI18n>["t"],
) {
  // Keep the hint synchronized with the same preference the key handler reads.
  // 让提示与按键处理器读取的同一偏好保持同步。
  const enterMode = useSyncExternalStore(subscribeComposerEnterSendMode, readComposerEnterSendMode, readComposerEnterSendMode);
  // Link the running hint to the textarea after typed text hides the placeholder.
  // 输入文字隐藏占位提示后，将运行中的提示关联到文本框。
  const noteId = useId();
  // A running turn uses the saved action for Enter and the opposite action for Ctrl+Enter.
  // 轮次运行中，Enter 使用已保存的操作，Ctrl+Enter 使用相反操作。
  const hint = isRunning
    ? t("composer.runningShortcutHint")
      .replace("{enterAction}", t(`settings.composerEnterSend.${enterMode}`))
      .replace("{ctrlEnterAction}", t(`settings.composerEnterSend.${oppositeComposerSendMode(enterMode)}`))
    : t("composer.shortcutHint");

  return {
    placeholder: t("composer.placeholder").replace("{shortcut}", hint),
    note: isRunning && hasDraftText ? { id: noteId, text: hint } : null,
  };
}
