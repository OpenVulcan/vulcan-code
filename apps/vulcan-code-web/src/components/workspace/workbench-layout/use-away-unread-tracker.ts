import React from "react";

import { shellClient } from "@/service/webui-service-runtime";
import {
  INITIAL_SELECTED_RUN_OBSERVATION,
  clearAwayUnread,
  hasAwayUnread,
  markAwayUnread,
  observeSelectedRun,
} from "@/state/away-unread";
import type { ConversationSummary } from "@/types";

// useAwayUnreadTracker marks the selected conversation unread when its run settles while the window is unfocused
// or hidden, and clears the marks at the user's first pointer or key input after that.
// useAwayUnreadTracker 在窗口失焦或隐藏期间选中会话的运行结束时将其标为未读，并在之后用户首次指针或键盘输入时清除。
export function useAwayUnreadTracker(
  selectedConversationId: string | null,
  conversations: readonly ConversationSummary[],
): void {
  // attendedRef starts attended so an unknown window state never invents a mark.
  // attendedRef 初始视为有人关注，未知窗口状态绝不凭空产生标记。
  const attendedRef = React.useRef(true);
  const observationRef = React.useRef(INITIAL_SELECTED_RUN_OBSERVATION);

  React.useEffect(() => {
    let active = true;
    let release: (() => void) | null = null;
    const refresh = () => {
      void shellClient.getNotificationPresentationState()
        .then((state) => {
          if (active) attendedRef.current = state.focused && state.foreground;
        })
        .catch((error) => console.warn("[workbench] window presentation state read failed", error));
    };
    refresh();
    void shellClient.subscribeNotificationPresentationChange(refresh)
      .then((unsubscribe) => {
        if (active) release = unsubscribe;
        else unsubscribe();
      })
      .catch((error) => console.warn("[workbench] window presentation subscription failed", error));
    return () => {
      active = false;
      release?.();
    };
  }, []);

  React.useEffect(() => {
    const { next, settledWhileAway } = observeSelectedRun(
      observationRef.current,
      selectedConversationId,
      conversations,
      attendedRef.current,
    );
    observationRef.current = next;
    if (settledWhileAway !== null) markAwayUnread(settledWhileAway);
  }, [conversations, selectedConversationId]);

  React.useEffect(() => {
    // Pointer and key input reach the page only while it has focus, so any of them proves the user is back.
    // 指针与键盘输入只在页面有焦点时到达，因此任一输入都证明用户已回来。
    const handleInput = () => {
      if (hasAwayUnread()) clearAwayUnread();
    };
    window.addEventListener("pointerdown", handleInput, true);
    window.addEventListener("keydown", handleInput, true);
    return () => {
      window.removeEventListener("pointerdown", handleInput, true);
      window.removeEventListener("keydown", handleInput, true);
    };
  }, []);
}
