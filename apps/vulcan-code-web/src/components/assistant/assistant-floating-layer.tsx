import React from "react";
import { Combine, Minus, SquareArrowOutUpRight, Undo2, X } from "lucide-react";
import { toast } from "sonner";
import { dedicatedErrorMessage } from "@/components/composer/dedicated/dedicated-errors";
import { Button } from "@/components/ui/button";
import { ProjectFloatingLayer, type FloatingLayerPreferredSize } from "@/components/workspace/project-floating-layer";
import { useI18n } from "@/i18n";
import { AssistantAvatar } from "./assistant-dock-button";
import { AssistantConversation } from "./assistant-conversation";
import { useAssistantDock, type AssistantDockController, type AssistantEntry } from "./assistant-dock-context";
import type { AssistantWindow } from "./assistant-dock-model";
import "./assistant-floating-layer.css";

/** First-open size of an assistant window: a small dialog rather than a workspace panel.
 * 助手窗口首次打开的尺寸：小对话框而非工作区面板。 */
const ASSISTANT_WINDOW_SIZE: FloatingLayerPreferredSize = { width: 440, height: 560 };

/** How long a closed window stays mounted so its minimize animation can reach the top-bar button.
 * 关闭的窗口保持挂载的时长，使其最小化动画能抵达顶部按钮。 */
const ASSISTANT_WINDOW_CLOSE_MS = 700;

/** Every open assistant window, each minimizing into the top-bar assistant button.
 * 所有打开的助手窗口，各自最小化收入顶部助手按钮。 */
export default function AssistantFloatingLayer() {
  const dock = useAssistantDock();
  const rendered = useRenderedAssistantWindows(dock?.state.windows ?? []);
  if (!dock) return null;
  const assistants = new Map(dock.assistants.map((assistant) => [assistant.id, assistant]));
  return (
    <>
      {rendered.map(({ window, expanded }) => {
        const active = assistants.get(window.activeId);
        if (!active) return null;
        return (
          <ProjectFloatingLayer
            key={window.id}
            defaultSize={ASSISTANT_WINDOW_SIZE}
            expanded={expanded}
            header={<AssistantWindowHeader active={active} dock={dock} window={window} />}
            minimizeAnchorRef={dock.buttonRef}
            projectId={window.id}
          >
            <div className="assistant-window-body" onPointerDownCapture={() => dock.focusWindow(window.id)}>
              {window.assistantIds.length > 1 ? (
                <AssistantRail assistants={assistants} dock={dock} window={window} />
              ) : null}
              <div className="assistant-window-main">
                <AssistantConversation key={active.id} assistant={active} />
              </div>
            </div>
          </ProjectFloatingLayer>
        );
      })}
    </>
  );
}

/** Title bar: the assistant name plus split, merge, restore, hide, and close commands.
 * 标题栏：助手名称及分离、合并、还原、隐藏与关闭命令。 */
function AssistantWindowHeader(props: { window: AssistantWindow; active: AssistantEntry; dock: AssistantDockController }) {
  const { t } = useI18n();
  const { active, dock, window } = props;
  const [restoring, setRestoring] = React.useState(false);
  const shared = window.assistantIds.length > 1;
  const restore = async () => {
    setRestoring(true);
    try {
      await dock.restore(active.id);
    } catch (reason) {
      toast.error(dedicatedErrorMessage(reason, t));
    } finally {
      setRestoring(false);
    }
  };
  const action = (label: string, icon: React.ReactNode, onClick: () => void, disabled = false) => (
    <Button aria-label={label} disabled={disabled} onClick={onClick} size="icon-xs" title={label} type="button" variant="ghost">
      {icon}
    </Button>
  );
  return (
    <div className="assistant-window-header">
      <AssistantAvatar active name={active.name} />
      <span className="assistant-window-title">
        <strong>{active.name}</strong>
        {active.projectName ? <span>{active.projectName}</span> : null}
      </span>
      {shared ? action(t("assistantDock.detach"), <SquareArrowOutUpRight size={14} />, () => dock.detach(window.id, active.id)) : null}
      {dock.state.windows.length > 1 ? action(t("assistantDock.merge"), <Combine size={14} />, () => dock.merge(window.id)) : null}
      {action(t("assistantDock.restore"), <Undo2 size={14} />, () => void restore(), restoring)}
      {shared ? action(t("assistantDock.hide"), <Minus size={14} />, () => dock.hide(active.id)) : null}
      {action(t("assistantDock.close"), <X size={14} />, () => dock.closeWindow(window.id))}
    </div>
  );
}

/** Left column of initials that switches between the assistants merged into one window.
 * 左侧首字栏，在合并到同一窗口的助手之间切换。 */
function AssistantRail(props: {
  window: AssistantWindow;
  assistants: ReadonlyMap<string, AssistantEntry>;
  dock: AssistantDockController;
}) {
  return (
    <nav className="assistant-rail">
      {props.window.assistantIds.map((id) => {
        const assistant = props.assistants.get(id);
        if (!assistant) return null;
        const active = id === props.window.activeId;
        return (
          <button
            key={id}
            aria-label={assistant.name}
            aria-pressed={active}
            className="assistant-rail-item"
            onClick={() => props.dock.activate(props.window.id, id)}
            title={assistant.name}
            type="button"
          >
            <AssistantAvatar active={active} className="assistant-rail-avatar" name={assistant.name} />
          </button>
        );
      })}
    </nav>
  );
}

/** Open windows plus recently closed ones kept mounted, collapsed, until their animation ends.
 * 打开的窗口，以及在动画结束前保持挂载并收起的刚关闭窗口。 */
function useRenderedAssistantWindows(windows: readonly AssistantWindow[]) {
  const [closing, setClosing] = React.useState<AssistantWindow[]>([]);
  const previousRef = React.useRef(windows);
  const timersRef = React.useRef(new Set<number>());
  React.useEffect(() => {
    const openIds = new Set(windows.map((window) => window.id));
    const removed = previousRef.current.filter((window) => !openIds.has(window.id));
    previousRef.current = windows;
    if (removed.length === 0) return;
    const removedIds = new Set(removed.map((window) => window.id));
    setClosing((current) => [...current.filter((window) => !removedIds.has(window.id)), ...removed]);
    const timer = window.setTimeout(() => {
      timersRef.current.delete(timer);
      setClosing((current) => current.filter((window) => !removedIds.has(window.id)));
    }, ASSISTANT_WINDOW_CLOSE_MS);
    timersRef.current.add(timer);
  }, [windows]);
  React.useEffect(() => {
    const timers = timersRef.current;
    return () => timers.forEach((timer) => window.clearTimeout(timer));
  }, []);
  const openIds = new Set(windows.map((window) => window.id));
  return [
    ...windows.map((window) => ({ window, expanded: true })),
    ...closing.filter((window) => !openIds.has(window.id)).map((window) => ({ window, expanded: false })),
  ];
}
