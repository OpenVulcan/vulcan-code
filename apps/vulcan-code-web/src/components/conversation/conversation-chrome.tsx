import React from "react";
import { Bot, Boxes, ChevronLeft, Square } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ShinyText } from "@/components/ui/shiny-text";
import { useI18n } from "@/i18n";
import { cn } from "@/lib/utils";
import type { WorkbenchSnapshot } from "@/types";

// SubagentNavigationTrace records user initiated subagent title-menu actions.
// SubagentNavigationTrace 记录用户触发的子代理标题菜单动作。
export type SubagentNavigationTrace = (action: string, detail: Record<string, unknown>) => void;

// ConversationLoadingCenter renders the cache-miss conversation hydration placeholder.
// ConversationLoadingCenter 渲染无缓存会话水合时的居中占位提示。
export function ConversationLoadingCenter() {
  return (
    <section className="conversation-loading-center" aria-live="polite" aria-busy="true">
      <ShinyText text="✨ 正在加载会话信息" />
    </section>
  );
}

// EmptyConversationPrompt renders the blank-chat prompt above the shared composer dock.
// EmptyConversationPrompt 在共享输入区上方渲染空会话提示。
export function EmptyConversationPrompt(props: {
  // Whether the prompt is playing its exit animation.
  // 当前提示是否正在播放退出动画。
  leaving: boolean;
  // Project name used by the empty-state prompt.
  // 空状态提示使用的项目名称。
  projectName: string;
}) {
  // t translates the empty conversation prompt while projectName remains user data.
  // t 翻译空会话引导语，projectName 仍保留为用户数据。
  const { t } = useI18n();
  // promptText inserts the selected project name into the localized empty prompt.
  // promptText 将当前项目名称插入本地化空会话提示。
  const promptText = t("console.emptyPrompt").replace("{projectName}", props.projectName);

  return (
    <section className={cn("empty-conversation-prompt", props.leaving && "leaving")} aria-hidden={props.leaving}>
      <h2>{promptText}</h2>
    </section>
  );
}

// SubagentNavigationMenu renders the backend-projected subagent switcher from the conversation title bar.
// SubagentNavigationMenu 从会话标题栏渲染由后端投影的子代理切换器。
export function SubagentNavigationMenu(props: {
  // Backend-projected subagent navigation snapshot.
  // 后端投影出的子代理导航快照。
  navigation: WorkbenchSnapshot["subagentNavigation"];
  // Opens one subagent transcript surface.
  // 打开一个子代理转录界面。
  onOpenSubagentPanel: (parentConversationId: string, taskId: string) => Promise<boolean>;
  // Returns from an active subagent transcript to the parent conversation.
  // 从活动子代理转录返回父会话。
  onCloseSubagentPanel: () => Promise<void>;
  // Stops one running subagent task.
  // 停止一个运行中的子代理任务。
  onStopSubagentTask: (parentConversationId: string, taskId: string) => Promise<void>;
  // Emits local click diagnostics for menu actions.
  // 为菜单动作输出本地点击诊断。
  onTrace: SubagentNavigationTrace;
}) {
  const navigation = props.navigation;
  const tasks = navigation?.tasks ?? [];
  const activeTaskId = navigation?.activeTaskId ?? null;
  const runningCount = tasks.filter((task) => task.isRunning).length;
  const canReturnToParent = Boolean(activeTaskId);
  if (!navigation || tasks.length === 0) {
    return null;
  }
  return (
    <div className={cn("subagent-title-menu", runningCount > 0 && "running")}>
      {canReturnToParent ? (
        <Button
          aria-label="返回主会话"
          className="subagent-title-return"
          onClick={() => {
            props.onTrace("subagent.menu.return", {
              parentConversationId: navigation.parentConversationId,
              activeTaskId,
              runningCount,
            });
            void props.onCloseSubagentPanel();
          }}
          size="icon-sm"
          title="返回主会话"
          type="button"
          variant="ghost"
        >
          <ChevronLeft size={15} />
        </Button>
      ) : null}
      <Button
        aria-label={runningCount > 0 ? `${runningCount} 个子代理正在运行` : "子代理调度"}
        className="subagent-title-trigger"
        size="icon-sm"
        type="button"
        variant="ghost"
      >
        <Bot size={15} />
      </Button>
      <nav className="subagent-title-flyout" aria-label="子代理调度切换">
        {tasks.map((task) => (
          <div className={cn("subagent-title-row-group", task.isActive && "active")} key={task.taskId}>
            <Button
              className="subagent-title-row"
              onClick={() => {
                props.onTrace("subagent.menu.open_task", {
                  parentConversationId: navigation.parentConversationId,
                  taskId: task.taskId,
                  runState: task.runState,
                  isRunning: task.isRunning,
                });
                void props.onOpenSubagentPanel(navigation.parentConversationId, task.taskId);
              }}
              size="sm"
              type="button"
              variant="ghost"
            >
              <Boxes size={14} />
              <span>{task.label}</span>
              <span className={cn("subagent-navigation-status", task.runState)}>{subagentRunStateLabel(task.runState)}</span>
            </Button>
            {task.isRunning ? (
              <Button
                aria-label={`停止子代理 ${task.label}`}
                className="subagent-title-stop"
                onClick={() => {
                  props.onTrace("subagent.menu.stop_task", {
                    parentConversationId: navigation.parentConversationId,
                    taskId: task.taskId,
                    runState: task.runState,
                  });
                  void props.onStopSubagentTask(navigation.parentConversationId, task.taskId);
                }}
                size="icon"
                type="button"
                variant="ghost"
              >
                <Square size={12} />
              </Button>
            ) : null}
          </div>
        ))}
      </nav>
    </div>
  );
}

// subagentRunStateLabel maps backend run-state keys to compact WebUI labels.
// subagentRunStateLabel 将后端运行状态键映射为紧凑 WebUI 标签。
function subagentRunStateLabel(runState: string): string {
  switch (runState) {
    case "running":
      return "运行中";
    case "stopped":
      return "已停止";
    case "failed":
      return "失败";
    default:
      return "已完成";
  }
}
