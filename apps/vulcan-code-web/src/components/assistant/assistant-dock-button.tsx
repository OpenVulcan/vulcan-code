import React from "react";
import { Bot, Check } from "lucide-react";
import { Button } from "@/ui-kit/primitives/button";
import { HoverCard, HoverCardContent, HoverCardTrigger } from "@/ui-kit/primitives/hover-card";
import { useI18n } from "@/i18n";
import { cx } from "@/ui-kit/behavior/core";
import { useAssistantDock } from "./assistant-dock-context";
import { assistantWindowOf } from "./assistant-dock-model";

/** Initial shown in an assistant avatar.
 * 助手头像中显示的首字。 */
export function assistantInitial(name: string): string {
  return Array.from(name.trim())[0]?.toUpperCase() ?? "?";
}

/** Square initial avatar used by the menu and the window rail; the active one is filled with ember.
 * 菜单与窗口侧栏共用的方形首字头像；激活者以余烬色填充。 */
export function AssistantAvatar(props: { name: string; active?: boolean; className?: string }) {
  return (
    <span
      aria-hidden="true"
      className={cx("vk-avatar", props.className)}
      data-active={props.active ? "" : undefined}
    >
      {assistantInitial(props.name)}
    </span>
  );
}

/** Top-bar entry: hovering lists the assistants, clicking one shows or hides it.
 * 顶部入口：悬停列出助手，点击助手显示或隐藏它。 */
export function AssistantDockButton() {
  const dock = useAssistantDock();
  const { t } = useI18n();
  const [open, setOpen] = React.useState(false);
  if (!dock || dock.assistants.length === 0) return null;
  const shownCount = dock.state.windows.reduce((count, window) => count + window.assistantIds.length, 0);
  return (
    <HoverCard onOpenChange={setOpen} open={open} openDelay={60}>
      <HoverCardTrigger asChild>
        <Button
          ref={dock.buttonRef}
          aria-expanded={open}
          aria-haspopup="menu"
          aria-label={t("assistantDock.button")}
          onClick={() => setOpen((current) => !current)}
          size="icon-sm"
          type="button"
          variant={shownCount > 0 ? "secondary" : "ghost"}
        >
          <Bot size={16} />
        </Button>
      </HoverCardTrigger>
      <HoverCardContent align="end" side="bottom" width="lg">
        <div aria-label={t("assistantDock.menu")} className="vk-assist-menu" role="menu">
          {dock.assistants.map((assistant) => {
            const shown = assistantWindowOf(dock.state, assistant.id) !== null;
            return (
              <button
                key={assistant.id}
                aria-checked={shown}
                className="vk-assist-item"
                onClick={() => dock.toggle(assistant.id)}
                role="menuitemcheckbox"
                type="button"
              >
                <AssistantAvatar active={shown} name={assistant.name} />
                <span className="vk-assist-text">
                  <strong>{assistant.name}</strong>
                  {assistant.projectName ? <small>{assistant.projectName}</small> : null}
                </span>
                {shown ? <Check aria-hidden="true" size={14} /> : null}
              </button>
            );
          })}
        </div>
      </HoverCardContent>
    </HoverCard>
  );
}
