import React from "react";
import { Bot, Check } from "lucide-react";
import { Button } from "@/ui-kit/primitives/button";
import { HoverCard, HoverCardContent, HoverCardTrigger } from "@/ui-kit/primitives/hover-card";
import { useI18n } from "@/i18n";
import { cn } from "@/lib/utils";
import { useAssistantDock } from "./assistant-dock-context";
import { assistantWindowOf } from "./assistant-dock-model";

/** Initial shown in an assistant avatar.
 * 助手头像中显示的首字。 */
export function assistantInitial(name: string): string {
  return Array.from(name.trim())[0]?.toUpperCase() ?? "?";
}

/** Round initial avatar used by the menu and the window rail.
 * 菜单与窗口侧栏共用的圆形首字头像。 */
export function AssistantAvatar(props: { name: string; active?: boolean; className?: string }) {
  return (
    <span
      aria-hidden="true"
      className={cn(
        "inline-flex size-6 shrink-0 items-center justify-center rounded-full text-xs font-semibold",
        props.active ? "bg-primary text-primary-foreground" : "bg-muted text-muted-foreground",
        props.className,
      )}
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
    <HoverCard closeDelay={180} onOpenChange={setOpen} open={open} openDelay={60}>
      <HoverCardTrigger asChild>
        <Button
          ref={dock.buttonRef}
          aria-expanded={open}
          aria-haspopup="menu"
          aria-label={t("assistantDock.button")}
          className="icon-button relative"
          onClick={() => setOpen((current) => !current)}
          size="icon-sm"
          type="button"
          variant={shownCount > 0 ? "secondary" : "ghost"}
        >
          <Bot size={16} />
        </Button>
      </HoverCardTrigger>
      <HoverCardContent align="end" className="p-1" side="bottom" width="sm">
        <div aria-label={t("assistantDock.menu")} className="flex flex-col gap-0.5" role="menu">
          {dock.assistants.map((assistant) => {
            const shown = assistantWindowOf(dock.state, assistant.id) !== null;
            return (
              <button
                key={assistant.id}
                aria-checked={shown}
                className="flex w-full min-w-0 items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm outline-none hover:bg-accent focus-visible:bg-accent"
                onClick={() => dock.toggle(assistant.id)}
                role="menuitemcheckbox"
                type="button"
              >
                <AssistantAvatar active={shown} name={assistant.name} />
                <span className="flex min-w-0 flex-1 flex-col">
                  <span className="truncate font-medium">{assistant.name}</span>
                  {assistant.projectName ? (
                    <span className="truncate text-xs text-muted-foreground">{assistant.projectName}</span>
                  ) : null}
                </span>
                {shown ? <Check aria-hidden="true" className="shrink-0 text-primary" size={14} /> : null}
              </button>
            );
          })}
        </div>
      </HoverCardContent>
    </HoverCard>
  );
}
