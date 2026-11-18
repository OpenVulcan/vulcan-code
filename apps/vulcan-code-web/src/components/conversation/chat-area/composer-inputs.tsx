import React from "react";
import { RuntimeCueLine } from "@/components/conversation/conversation-live-reasoning";
import type { RuntimeCue } from "@/components/conversation/conversation-model";
import { conversationSelectionEqual, useEqualValue } from "@/state/conversation-event-window-react";

/** Returns the composer's runtime status element, keeping one element identity per distinct cue.
 * 返回输入区的运行状态元素，每个不同的提示保持同一元素身份。
 *
 * @param cue cue shown in the composer dock, or null when another status takes precedence.
 *            输入区停靠层显示的提示；其他状态优先时为 null。
 * @returns the cue line element, or null when there is no cue.
 *          提示行元素；无提示时为 null。 */
export function useComposerRuntimeStatus(cue: RuntimeCue | null): React.ReactNode {
  const stableCue = useEqualValue(cue, conversationSelectionEqual);
  return React.useMemo(
    () => stableCue && <RuntimeCueLine cue={stableCue} key={`${stableCue.text}:${stableCue.startedAtMs ?? ""}`} />,
    [stableCue],
  );
}
