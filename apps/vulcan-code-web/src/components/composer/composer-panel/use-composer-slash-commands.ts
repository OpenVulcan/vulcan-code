import React from "react";
import {
  composerSlashCommandMatchesInput,
  exactComposerSlashCommand,
  normalizedComposerSlashCommandId,
  type ComposerSlashCommandResult,
} from "@/components/composer/composer-model";
import type { ComposerSettingsSection } from "@/components/composer/composer-settings-request";
import type { ComposerSlashCommandSummary } from "@/types";
import { useComposerExternalCommands, type ComposerOwner } from "@/components/composer/composer-external-commands";
import type { ComposerPromptSkillOverrides } from "@/components/composer/composer-model";
import { oppositeComposerSendMode, readComposerEnterSendMode, type ComposerSendMode } from "@/components/composer/composer-send-preference";

// UseComposerSlashCommandsOptions defines the input state and routed actions owned outside slash selection.
// UseComposerSlashCommandsOptions 定义 slash 选择之外持有的输入状态与路由动作。
export interface UseComposerSlashCommandsOptions {
  // Current composer input used to filter and activate commands.
  // 用于筛选与激活指令的当前输入区文本。
  input: string;
  // Updates composer input after typing or command activation.
  // 在输入或指令激活后更新输入区文本。
  setInput: React.Dispatch<React.SetStateAction<string>>;
  // Commands projected by the shared workbench service.
  // 共享工作台服务投影出的指令。
  commands: ComposerSlashCommandSummary[];
  // Whether Plan mode is currently active.
  // 当前是否启用计划模式。
  planModeEnabled: boolean;
  // Persists or previews one Plan mode transition.
  // 持久化或预览一次计划模式转换。
  onSetPlanMode: (enabled: boolean) => Promise<boolean>;
  // Routes one non-Plan command through the owning workbench.
  // 通过所属工作台路由一个非 Plan 指令。
  onRouteCommand: (command: ComposerSlashCommandSummary) => ComposerSlashCommandResult;
  // Opens the existing model picker.
  // 打开现有模型选择器。
  onOpenModelPicker: () => void;
  // Opens the existing reasoning picker.
  // 打开现有推理选择器。
  onOpenReasoningPicker: () => void;
  // Opens the session goal editor.
  // 打开会话目标编辑器。
  onOpenGoal: () => void;
  // Opens the conversation settings menu, optionally at one submenu.
  // 打开对话设置菜单，可选地直接展开某个子菜单。
  onOpenSettings: (section: ComposerSettingsSection | null) => void;
  // Sends with sendMode after slash navigation; allowStop permits an empty Enter to stop an active turn.
  // slash 导航处理后以 sendMode 发送；allowStop 允许空草稿按 Enter 停止当前回合。
  onSend: (sendMode: ComposerSendMode, allowStop: boolean) => Promise<void>;
  // Receives the caret at the end of the draft after another surface inserted text into it.
  // 其他界面向草稿插入文本后，在此输入框中将光标放到草稿末尾。
  textareaRef: React.RefObject<HTMLTextAreaElement | null>;
  // Stages files another surface attached together with its inserted text, under the composer's own checks.
  // 按输入区自身的校验暂存其他界面随插入文本一并附带的文件。
  onAttachFiles: (files: File[]) => void;
  // Draft this composer shows, so text meant for a new conversation in one project lands only there.
  // 本输入区显示的草稿，使发给某个项目新会话的文本只落在那里。
  owner: ComposerOwner;
  // Applies the skill switches another surface sent with its text to the draft's session settings.
  // 将其他界面随文本发送的技能开关应用到草稿的会话设置。
  onSetDraftSkills: (skills: ComposerPromptSkillOverrides) => void;
}

// ComposerSlashCommandsController exposes slash panel state and keyboard actions to ComposerPanel.
// ComposerSlashCommandsController 向 ComposerPanel 暴露 slash 面板状态与键盘操作。
export interface ComposerSlashCommandsController {
  // Commands matching the current non-dismissed slash input.
  // 匹配当前未关闭 slash 输入的指令。
  visibleCommands: ComposerSlashCommandSummary[];
  // Whether at least one command is visible.
  // 是否至少有一个可见指令。
  panelVisible: boolean;
  // Index highlighted by keyboard or pointer navigation.
  // 由键盘或指针导航高亮的索引。
  highlightedIndex: number;
  // Updates the highlighted command index from pointer navigation.
  // 通过指针导航更新高亮指令索引。
  setHighlightedIndex: React.Dispatch<React.SetStateAction<number>>;
  // Updates composer input and re-enables filtering after a dismissed value changes.
  // 更新输入区文本，并在被关闭值变化后重新启用筛选。
  handleInputChange: (value: string) => void;
  // Appends one globally redirected printable key and re-enables filtering.
  // 追加一个全局重定向的可打印按键并重新启用筛选。
  appendGlobalInput: (value: string) => void;
  // Activates one selected command and applies its routed result.
  // 激活一个选中指令并应用其路由结果。
  activateCommand: (command: ComposerSlashCommandSummary) => void;
  // Handles slash navigation before applying ordinary composer Enter-to-send behavior.
  // 在应用普通输入区 Enter 发送行为前处理 slash 导航。
  handleKeyDown: (event: React.KeyboardEvent<HTMLTextAreaElement>) => void;
  // Resets command dismissal and highlighting when the active draft slot changes.
  // 当前草稿槽变化时重置指令关闭状态与高亮。
  reset: () => void;
}

// useComposerSlashCommands owns slash filtering, navigation, activation, and keyboard priority.
// useComposerSlashCommands 管理 slash 筛选、导航、激活与键盘优先级。
// Parameters: options provides composer input, projected commands, and routed side effects.
// 参数：options 提供输入区文本、投影指令与路由副作用。
// Returns: slash panel state and stable input, activation, keyboard, and reset actions.
// 返回值：slash 面板状态及稳定的输入、激活、键盘与重置操作。
export function useComposerSlashCommands(options: UseComposerSlashCommandsOptions): ComposerSlashCommandsController {
  // highlightedIndex stores the active command row while the panel is visible.
  // highlightedIndex 保存面板可见时的活动指令行。
  const [highlightedIndex, setHighlightedIndex] = React.useState(0);
  // dismissedInput stores the exact slash input dismissed by Escape.
  // dismissedInput 保存被 Escape 关闭的精确 slash 输入。
  const [dismissedInput, setDismissedInput] = React.useState<string | null>(null);
  // visibleCommands applies prefix filtering unless the exact current value was dismissed.
  // visibleCommands 应用前缀筛选，但排除被关闭的精确当前值。
  const visibleCommands = React.useMemo(() => {
    if (!options.input.startsWith("/") || dismissedInput === options.input) {
      return [];
    }
    return options.commands.filter((command) => composerSlashCommandMatchesInput(command, options.input));
  }, [dismissedInput, options.commands, options.input]);
  // panelVisible indicates whether keyboard navigation has at least one valid target.
  // panelVisible 表示键盘导航是否至少存在一个有效目标。
  const panelVisible = visibleCommands.length > 0;
  // highlightedCommand resolves the clamped keyboard target.
  // highlightedCommand 解析经过边界收敛的键盘目标。
  const highlightedCommand = visibleCommands[
    Math.min(highlightedIndex, Math.max(visibleCommands.length - 1, 0))
  ] ?? null;
  // exactCommand takes precedence over the highlighted row when Enter activates a full trigger.
  // exactCommand 在 Enter 激活完整触发词时优先于高亮行。
  const exactCommand = React.useMemo(
    () => exactComposerSlashCommand(visibleCommands, options.input),
    [options.input, visibleCommands],
  );

  React.useEffect(() => {
    setHighlightedIndex((current) => {
      if (!visibleCommands.length) {
        return 0;
      }
      return Math.min(current, visibleCommands.length - 1);
    });
  }, [visibleCommands.length]);

  // resetSelection clears slash navigation state and applies the requested next input.
  // resetSelection 清理 slash 导航状态并应用请求的下一输入值。
  const resetSelection = React.useCallback((nextInput: string) => {
    options.setInput(nextInput);
    setDismissedInput(null);
    setHighlightedIndex(0);
  }, [options.setInput]);
  // handleInputChange tracks textarea edits and re-enables matching after the dismissed value changes.
  // handleInputChange 跟踪文本框编辑，并在被关闭值变化后重新启用匹配。
  const handleInputChange = React.useCallback((value: string) => {
    options.setInput(value);
    setDismissedInput((current) => current !== null && value !== current ? null : current);
  }, [options.setInput]);
  // appendGlobalInput appends one redirected key without capturing stale input.
  // appendGlobalInput 追加一个重定向按键，同时避免捕获过期输入。
  const appendGlobalInput = React.useCallback((value: string) => {
    options.setInput((current) => `${current}${value}`);
    setDismissedInput((current) => current === null ? current : null);
  }, [options.setInput]);
  // activateCommand handles the local Plan command before applying workbench-routed results.
  // activateCommand 在应用工作台路由结果前处理本地 Plan 指令。
  const activateCommand = React.useCallback((command: ComposerSlashCommandSummary) => {
    if (normalizedComposerSlashCommandId(command) === "plan") {
      // The input stays editable while the save is pending, so only the command text itself is cleared.
      // 保存等待期间输入框仍可编辑，因此只清除指令文本本身。
      const commandInput = options.input;
      void options.onSetPlanMode(!options.planModeEnabled).then((handled) => {
        if (handled) {
          options.setInput((current) => current === commandInput ? "" : current);
          setDismissedInput(null);
          setHighlightedIndex(0);
        }
      });
      return;
    }
    // result describes the synchronous UI effect routed by the owning workbench.
    // result 描述所属工作台同步路由出的界面效果。
    const result = options.onRouteCommand(command);
    if (!result.handled) {
      return;
    }
    resetSelection(result.replacementText ?? "");
    if (result.openTarget === "model") {
      options.onOpenModelPicker();
    }
    if (result.openTarget === "reasoning") {
      options.onOpenReasoningPicker();
    }
    if (result.openTarget === "goal") {
      options.onOpenGoal();
    }
    if (result.openTarget === "settings") {
      options.onOpenSettings(result.settingsSection ?? null);
    }
  }, [
    options.input,
    options.onOpenGoal,
    options.onOpenModelPicker,
    options.onOpenReasoningPicker,
    options.onOpenSettings,
    options.onRouteCommand,
    options.onSetPlanMode,
    options.planModeEnabled,
    options.setInput,
    resetSelection,
  ]);
  // handleKeyDown gives visible slash navigation priority over ordinary Enter-to-send behavior.
  // handleKeyDown 让可见 slash 导航优先于普通 Enter 发送行为。
  const handleKeyDown = React.useCallback((event: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (panelVisible) {
      if (event.key === "ArrowDown") {
        event.preventDefault();
        setHighlightedIndex((current) => (current + 1) % visibleCommands.length);
        return;
      }
      if (event.key === "ArrowUp") {
        event.preventDefault();
        setHighlightedIndex((current) => (current - 1 + visibleCommands.length) % visibleCommands.length);
        return;
      }
      if (event.key === "Escape") {
        event.preventDefault();
        setDismissedInput(options.input);
        return;
      }
      if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
        // command prioritizes an exact trigger before the currently highlighted row.
        // command 优先选择精确触发词，再选择当前高亮行。
        const command = exactCommand ?? highlightedCommand;
        if (command) {
          event.preventDefault();
          activateCommand(command);
          return;
        }
      }
    }
    if (event.key !== "Enter" || event.shiftKey || event.nativeEvent.isComposing) {
      return;
    }
    event.preventDefault();
    // A held key must not turn a just-completed send into an empty-draft stop.
    // 长按回车不能在发送完成后把空草稿变成停止操作。
    if (event.repeat) return;
    // alternate selects Ctrl+Enter, which reverses the saved choice and only sends messages.
    // alternate 选择 Ctrl+Enter，它反转已保存的选项，仅用于发送消息。
    const alternate = event.ctrlKey && !event.altKey && !event.metaKey;
    // Read the preference at keydown; the service still decides whether the message needs to queue.
    // 在按键时读取偏好，是否需要排队仍由服务决定。
    const enterMode = readComposerEnterSendMode();
    void options.onSend(alternate ? oppositeComposerSendMode(enterMode) : enterMode, !alternate);
  }, [activateCommand, exactCommand, highlightedCommand, options.input, options.onSend, panelVisible, visibleCommands.length]);
  // reset clears transient slash state without mutating the newly restored draft input.
  // reset 清理临时 slash 状态，但不修改刚恢复的草稿输入。
  const reset = React.useCallback(() => {
    setDismissedInput(null);
    setHighlightedIndex(0);
  }, []);
  // The command palette and Ctrl+M reach the model picker and slash commands through these same handlers.
  // 命令面板与 Ctrl+M 通过这些相同的处理器打开模型选择器并运行 slash 指令。
  const insertText = React.useCallback((text: string, files: readonly File[], skills: ComposerPromptSkillOverrides | null) => {
    if (skills) options.onSetDraftSkills(skills);
    options.setInput((current) => (current.trim() === "" ? text : `${current}
${text}`));
    if (files.length) options.onAttachFiles([...files]);
    // The caret lands only after the inserted draft has rendered.
    // 光标只在插入后的草稿渲染完成后落下。
    window.requestAnimationFrame(() => {
      const textarea = options.textareaRef.current;
      if (!textarea) return;
      textarea.focus({ preventScroll: true });
      textarea.setSelectionRange(textarea.value.length, textarea.value.length);
    });
  }, [options.setInput, options.onAttachFiles, options.onSetDraftSkills, options.textareaRef]);
  useComposerExternalCommands(options.onOpenModelPicker, options.commands, activateCommand, insertText, options.owner);

  return {
    visibleCommands,
    panelVisible,
    highlightedIndex,
    setHighlightedIndex,
    handleInputChange,
    appendGlobalInput,
    activateCommand,
    handleKeyDown,
    reset,
  };
}
