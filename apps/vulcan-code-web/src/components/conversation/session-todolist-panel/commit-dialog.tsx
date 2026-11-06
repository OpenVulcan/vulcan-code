// The shared Git commit and push dialog. It loads on first open, so the workbench's first screen does not carry it.
// 共享的 Git 提交与推送弹窗。首次打开时才加载，使工作台首屏不包含它。
import React from "react";
import { Check, ChevronDown, CloudUpload, GitBranch, GitCommitHorizontal, Plus } from "lucide-react";
import {
  conversationSessionBranchDisplayName,
  conversationSessionDefaultNewBranchName,
} from "@/components/conversation/session-todolist-panel/git-branch-controls";
import { toast } from "@/ui-kit/primitives/toast";
import {
  generateGitCommitMessage,
  runGitCommitAction,
  type GitCommitActionResponse,
} from "@/service/conversation-actions-service";
import { Checkbox } from "@/ui-kit/primitives/checkbox";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/ui-kit/primitives/dialog";
import { Input } from "@/ui-kit/primitives/input";
import { Textarea } from "@/ui-kit/primitives/textarea";
import { useI18n } from "@/i18n";
import type { ConversationSessionCommitBusyState, ConversationSessionReviewSummary } from "@/components/conversation/session-todolist-panel";

// ConversationSessionCommitTarget identifies where the local commit draft would land.
// ConversationSessionCommitTarget 标识本地提交草稿的目标分支位置。
type ConversationSessionCommitTarget = "current" | "new";

// ConversationSessionCommitActionId identifies one runnable action in the commit dialog.
// ConversationSessionCommitActionId 标识提交弹窗内的单个可执行动作。
type ConversationSessionCommitActionId = "commit" | "commitPush" | "push";

// ConversationSessionCommitDialogProps is the input of the shared Git commit dialog.
// ConversationSessionCommitDialogProps 是共享 Git 提交弹窗的输入。
export interface ConversationSessionCommitDialogProps {
  // Whether the shared Git commit dialog is currently open.
  // 共享 Git 提交弹窗当前是否打开。
  open: boolean;
  // Opens or closes the shared Git commit dialog.
  // 打开或关闭共享 Git 提交弹窗。
  onOpenChange: (open: boolean) => void;
  // Shared Git commit new-branch prefix projected from service configuration.
  // 从服务端配置投影出的共享 Git 提交新分支前缀。
  commitBranchPrefix: string;
  // Selected conversation id that owns Git commit generation.
  // 拥有 Git 提交生成动作的当前会话标识。
  conversationId: string | null;
  // Current outer Git action phase, if one is already running.
  // 当前外层 Git 动作阶段；存在时表示已有动作正在运行。
  busyState: ConversationSessionCommitBusyState;
  // Review summary used to gate local commit actions.
  // 用于限制本地提交动作的审核摘要。
  reviewSummary: ConversationSessionReviewSummary;
  // Workspace path used to resolve the Git repository on the backend.
  // 后端用于解析 Git 仓库的工作区路径。
  workspacePath: string | null;
  // Stable project-relative repository key targeted by this dialog.
  // 本弹窗操作的稳定项目相对仓库键。
  repositoryKey?: string | null;
  // Updates every shared Git trigger with the active action phase.
  // 使用当前动作阶段更新每个共享 Git 入口。
  onBusyStateChange: (repositoryKey: string | null, state: ConversationSessionCommitBusyState) => void;
  // Refreshes the compact Git review summary after backend Git actions.
  // 后端 Git 动作完成后刷新紧凑 Git 审查摘要。
  onRefreshReviewSummary: () => Promise<void>;
}

// ConversationSessionCommitDialog renders the shared Git commit or push dialog.
// ConversationSessionCommitDialog 渲染共享的 Git 提交或推送弹窗。
export function ConversationSessionCommitDialog(props: ConversationSessionCommitDialogProps) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();

  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent className="vk-commit" layout="document">
        <DialogTitle className="vk-sr-only">{t("gitCommitDialog.title")}</DialogTitle>
        <DialogDescription className="vk-sr-only">
          {t("gitCommitDialog.description")}
        </DialogDescription>
        <ConversationSessionCommitPanel
          commitBranchPrefix={props.commitBranchPrefix}
          conversationId={props.conversationId}
          busyState={props.busyState}
          reviewSummary={props.reviewSummary}
          repositoryKey={props.repositoryKey}
          workspacePath={props.workspacePath}
          onClose={() => props.onOpenChange(false)}
          onBusyStateChange={(state) => props.onBusyStateChange(props.repositoryKey ?? null, state)}
          onRefreshReviewSummary={props.onRefreshReviewSummary}
        />
      </DialogContent>
    </Dialog>
  );
}

// ConversationSessionCommitPanel renders the visual-only commit draft surface.
// ConversationSessionCommitPanel 渲染仅界面状态的提交草稿浮层。
function ConversationSessionCommitPanel(props: {
  // Shared Git commit new-branch prefix projected from service configuration.
  // 从服务端配置投影出的共享 Git 提交新分支前缀。
  commitBranchPrefix: string;
  // Selected conversation id that owns Git commit generation.
  // 拥有 Git 提交生成动作的当前会话标识。
  conversationId: string | null;
  // Current outer Git action phase, if one is already running.
  // 当前外层 Git 动作阶段；存在时表示已有动作正在运行。
  busyState: ConversationSessionCommitBusyState;
  // Review summary used to gate local commit actions.
  // 用于限制本地提交动作的审核摘要。
  reviewSummary: ConversationSessionReviewSummary;
  // Workspace path used to resolve the Git repository on the backend.
  // 后端用于解析 Git 仓库的工作区路径。
  workspacePath: string | null;
  // Stable project-relative repository key targeted by this panel.
  // 本面板操作的稳定项目相对仓库键。
  repositoryKey?: string | null;
  // Closes the commit dialog after a Git action starts.
  // Git 动作开始后关闭提交弹窗。
  onClose: () => void;
  // Updates the collapsed Git trigger row with the active action phase.
  // 使用当前动作阶段更新折叠 Git 入口行。
  onBusyStateChange: (state: ConversationSessionCommitBusyState) => void;
  // Refreshes the compact Git review summary after backend Git actions.
  // 后端 Git 动作完成后刷新紧凑 Git 审查摘要。
  onRefreshReviewSummary: () => Promise<void>;
}) {
  const { t } = useI18n();
  // includeUnstaged stores whether unstaged working tree changes are included by default.
  // includeUnstaged 保存默认是否包含工作区未暂存变更。
  const [includeUnstaged, setIncludeUnstaged] = React.useState(props.reviewSummary.unstagedChangedFiles > 0);
  // target stores whether the draft points at the current branch or a new branch.
  // target 保存草稿指向当前分支还是新分支。
  const [target, setTarget] = React.useState<ConversationSessionCommitTarget>("current");
  // branchMenuOpen stores the lightweight branch target menu state.
  // branchMenuOpen 保存轻量分支目标菜单状态。
  const [branchMenuOpen, setBranchMenuOpen] = React.useState(false);
  // branchName stores the editable new branch name shown by the visual draft.
  // branchName 保存界面草稿中展示的可编辑新分支名。
  const [branchName, setBranchName] = React.useState("");
  // branchNameEdited tracks whether the user has overridden the generated branch name.
  // branchNameEdited 标记用户是否已经覆盖自动生成的分支名。
  const [branchNameEdited, setBranchNameEdited] = React.useState(false);
  // commitMessage stores the manually typed commit message draft.
  // commitMessage 保存用户手动输入的提交信息草稿。
  const [commitMessage, setCommitMessage] = React.useState("");
  // lockedAction stores the action confirmed by Ctrl+Enter.
  // lockedAction 保存 Ctrl+Enter 当前确认执行的动作。
  const [lockedAction, setLockedAction] = React.useState<ConversationSessionCommitActionId>("commit");
  // actionButtonRefs store focus targets for keyboard action locking.
  // actionButtonRefs 保存键盘动作锁定使用的焦点目标。
  const actionButtonRefs = React.useRef<Record<ConversationSessionCommitActionId, HTMLButtonElement | null>>({
    commit: null,
    commitPush: null,
    push: null,
  });
  const currentBranchName = props.reviewSummary.branchName
    ? conversationSessionBranchDisplayName(props.reviewSummary.branchName)
    : t("gitCommitDialog.currentBranch");
  const defaultNewBranchName = React.useMemo(
    () => conversationSessionDefaultNewBranchName(currentBranchName, props.reviewSummary.branchNames, props.commitBranchPrefix),
    [currentBranchName, props.commitBranchPrefix, props.reviewSummary.branchNames],
  );
  const hasStagedChanges = props.reviewSummary.stagedChangedFiles > 0;
  const hasUnstagedChanges = props.reviewSummary.unstagedChangedFiles > 0;
  const hasIncludedChanges = hasStagedChanges || (includeUnstaged && hasUnstagedChanges);
  const hasPushOnlyChanges = props.reviewSummary.unpushedCommits > 0;
  const includedAdditions = props.reviewSummary.stagedAdditions
    + (includeUnstaged ? props.reviewSummary.unstagedAdditions : 0);
  const includedDeletions = props.reviewSummary.stagedDeletions
    + (includeUnstaged ? props.reviewSummary.unstagedDeletions : 0);
  const hasValidCommitTarget = target === "current" || branchName.trim().length > 0;
  const includeUnstagedDisabled = !hasUnstagedChanges;
  const effectiveTargetLabel = target === "new" ? (branchName.trim() || t("gitCommitDialog.newBranch")) : currentBranchName;
  const newBranchPlaceholder = `${t("gitCommitDialog.recommendedPrefix")}${defaultNewBranchName}`;
  const backendReady = Boolean(props.conversationId && props.workspacePath);
  const controlsDisabled = props.busyState !== null || !backendReady;
  const commitActionDisabled = controlsDisabled || !hasIncludedChanges || !hasValidCommitTarget;
  // Push-only publishes existing commits and never touches the worktree, so uncommitted changes must not block it.
  // 仅推送只发布已有提交、不触及工作区，因此未提交变更不得阻止推送。
  const pushOnlyActionDisabled = controlsDisabled || !hasPushOnlyChanges;

  React.useEffect(() => {
    if (!hasUnstagedChanges) {
      setIncludeUnstaged(false);
    }
  }, [hasUnstagedChanges]);

  React.useEffect(() => {
    if (target === "new" && !branchNameEdited) {
      setBranchName(defaultNewBranchName);
    }
  }, [branchNameEdited, defaultNewBranchName, target]);

  // commitTargetRequest builds the backend target payload from the current branch controls.
  // commitTargetRequest 根据当前分支控件构建后端目标载荷。
  const commitTargetRequest = React.useCallback(() => {
    if (target === "new") {
      return { mode: "new" as const, branchName: branchName.trim() };
    }
    return { mode: "current" as const };
  }, [branchName, target]);

  // runCommitAction executes a commit or commit-and-push when the message draft is ready.
  // runCommitAction 在提交信息草稿就绪后执行提交或提交并推送。
  const runCommitAction = React.useCallback(async (push: boolean) => {
    if (!props.conversationId || !props.workspacePath || props.busyState) {
      return;
    }
    if (!hasIncludedChanges || !hasValidCommitTarget) {
      return;
    }
    props.onClose();
    try {
      let message = commitMessage.trim();
      if (!message) {
        props.onBusyStateChange({ label: t("gitCommitDialog.preparingCommitContent") });
        await Promise.resolve();
        props.onBusyStateChange({ label: t("gitCommitDialog.generatingCommitMessage") });
        const response = await generateGitCommitMessage({
          conversationId: props.conversationId,
          workspacePath: props.workspacePath,
          repositoryKey: props.repositoryKey,
          includeUnstaged,
        });
        message = response?.message?.trim() ?? "";
        if (!message) {
          throw new Error(t("gitCommitDialog.generateEmpty"));
        }
      }
      props.onBusyStateChange({ label: t("gitCommitDialog.committing") });
      const commitResponse = await runGitCommitAction({
        conversationId: props.conversationId,
        workspacePath: props.workspacePath,
        repositoryKey: props.repositoryKey,
        target: commitTargetRequest(),
        includeUnstaged,
        message,
        push: false,
      });
      if (push) {
        props.onBusyStateChange({ label: t("gitCommitDialog.pushingDetailed") });
        const pushResponse = await runGitCommitAction({
          conversationId: props.conversationId,
          workspacePath: props.workspacePath,
          repositoryKey: props.repositoryKey,
          target: { mode: "current" },
          includeUnstaged: false,
          message: "",
          push: true,
        });
        showGitCommitActionToast(pushResponse ?? commitResponse, t("gitCommitDialog.pushDone"));
      } else {
        showGitCommitActionToast(commitResponse, t("gitCommitDialog.actionDone"));
      }
      await props.onRefreshReviewSummary();
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      props.onBusyStateChange(null);
    }
  }, [commitMessage, commitTargetRequest, hasIncludedChanges, hasValidCommitTarget, includeUnstaged, props, t]);

  // runPushAction pushes the current branch without creating a commit.
  // runPushAction 推送当前分支且不创建提交。
  const runPushAction = React.useCallback(async () => {
    if (!props.conversationId || !props.workspacePath || props.busyState) {
      return;
    }
    props.onClose();
    props.onBusyStateChange({ label: t("gitCommitDialog.pushingDetailed") });
    try {
      const response = await runGitCommitAction({
        conversationId: props.conversationId,
        workspacePath: props.workspacePath,
        repositoryKey: props.repositoryKey,
        target: { mode: "current" },
        includeUnstaged: false,
        message: "",
        push: true,
      });
      showGitCommitActionToast(response, t("gitCommitDialog.pushDone"));
      await props.onRefreshReviewSummary();
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      props.onBusyStateChange(null);
    }
  }, [props, t]);

  const commitActionChoices = React.useMemo(() => ([
    {
      disabled: commitActionDisabled,
      id: "commit" as const,
      run: () => {
        void runCommitAction(false);
      },
    },
    {
      disabled: commitActionDisabled,
      id: "commitPush" as const,
      run: () => {
        void runCommitAction(true);
      },
    },
    {
      disabled: pushOnlyActionDisabled,
      id: "push" as const,
      run: () => {
        void runPushAction();
      },
    },
  ]), [commitActionDisabled, pushOnlyActionDisabled, runCommitAction, runPushAction]);

  React.useEffect(() => {
    const lockedChoice = commitActionChoices.find((choice) => choice.id === lockedAction);
    if (lockedChoice && !lockedChoice.disabled) {
      return;
    }
    const nextChoice = commitActionChoices.find((choice) => !choice.disabled);
    if (nextChoice) {
      setLockedAction(nextChoice.id);
    }
  }, [commitActionChoices, lockedAction]);

  // moveLockedCommitAction rotates the keyboard-confirmed action among enabled choices.
  // moveLockedCommitAction 在可用动作之间轮换键盘确认的锁定项。
  const moveLockedCommitAction = React.useCallback((direction: -1 | 1) => {
    const enabledChoices = commitActionChoices.filter((choice) => !choice.disabled);
    if (!enabledChoices.length) {
      return;
    }
    const currentIndex = enabledChoices.findIndex((choice) => choice.id === lockedAction);
    const safeIndex = currentIndex >= 0 ? currentIndex : 0;
    const nextIndex = (safeIndex + direction + enabledChoices.length) % enabledChoices.length;
    const nextActionId = enabledChoices[nextIndex].id;
    setBranchMenuOpen(false);
    setLockedAction(nextActionId);
    actionButtonRefs.current[nextActionId]?.focus();
  }, [commitActionChoices, lockedAction]);

  // executeLockedCommitAction runs the action currently selected for Ctrl+Enter.
  // executeLockedCommitAction 执行当前为 Ctrl+Enter 锁定的动作。
  const executeLockedCommitAction = React.useCallback(() => {
    const choice = commitActionChoices.find((candidate) => candidate.id === lockedAction);
    if (!choice || choice.disabled) {
      return;
    }
    choice.run();
  }, [commitActionChoices, lockedAction]);

  // handleCommitPanelKeyDown applies dialog-level shortcuts without stealing text editing arrows.
  // handleCommitPanelKeyDown 应用弹窗级快捷键，同时不抢占文本编辑方向键。
  const handleCommitPanelKeyDown = React.useCallback((event: React.KeyboardEvent<HTMLDivElement>) => {
    if ((event.ctrlKey || event.metaKey) && event.key === "Enter") {
      event.preventDefault();
      executeLockedCommitAction();
      return;
    }
    if (event.key !== "ArrowUp" && event.key !== "ArrowDown") {
      return;
    }
    if (!commitPanelTargetAllowsActionNavigation(event.target)) {
      return;
    }
    event.preventDefault();
    moveLockedCommitAction(event.key === "ArrowUp" ? -1 : 1);
  }, [executeLockedCommitAction, moveLockedCommitAction]);

  return (
    <div className="vk-commit-panel" onKeyDown={handleCommitPanelKeyDown}>
      <div className="vk-commit-head">
        <div className="vk-commit-target">
          <button
            aria-expanded={branchMenuOpen}
            className="vk-branch-btn"
            onClick={() => setBranchMenuOpen((current) => !current)}
            type="button"
          >
            <GitBranch size={13} />
            <span>{effectiveTargetLabel}</span>
            <ChevronDown size={12} />
          </button>
          {branchMenuOpen ? (
            <div className="vk-pop vk-commit-menu vk-menu" role="menu">
              <p className="vk-menu-note">{t("gitCommitDialog.commitTo")}</p>
              <button
                className="vk-menu-item"
                onClick={() => {
                  setTarget("current");
                  setBranchMenuOpen(false);
                }}
                type="button"
              >
                <GitBranch size={14} />
                <span className="vk-menu-text">{currentBranchName}</span>
                {target === "current" ? <Check size={14} /> : null}
              </button>
              <button
                className="vk-menu-item"
                onClick={() => {
                  setTarget("new");
                  setBranchName(defaultNewBranchName);
                  setBranchNameEdited(false);
                  setBranchMenuOpen(false);
                }}
                type="button"
              >
                <Plus size={14} />
                <span className="vk-menu-text">{t("gitCommitDialog.newBranch")}</span>
                {target === "new" ? <Check size={14} /> : null}
              </button>
            </div>
          ) : null}
        </div>
        {hasIncludedChanges ? (
          <strong className="vk-stools-delta">
            <span className="vk-add">+{includedAdditions}</span>
            <span className="vk-del">-{includedDeletions}</span>
          </strong>
        ) : (
          <strong>{t("gitCommitDialog.noChanges")}</strong>
        )}
      </div>
      {target === "new" ? (
        <Input
          aria-label={t("gitCommitDialog.branchName")}
          onChange={(event) => {
            setBranchNameEdited(true);
            setBranchName(event.currentTarget.value);
          }}
          placeholder={newBranchPlaceholder}
          value={branchName}
        />
      ) : null}
      <Textarea
        aria-label={t("gitCommitDialog.message")}
        disabled={controlsDisabled || !hasIncludedChanges}
        onChange={(event) => {
          setCommitMessage(event.currentTarget.value);
        }}
        placeholder={t("gitCommitDialog.messagePlaceholder")}
        rows={2}
        value={commitMessage}
      />
      <label className="vk-check-label" data-disabled={includeUnstagedDisabled ? "" : undefined}>
        <Checkbox
          checked={includeUnstaged}
          disabled={includeUnstagedDisabled}
          onCheckedChange={(checked) => setIncludeUnstaged(checked === true)}
        />
        <span>{t("gitCommitDialog.includeUnstaged")}</span>
      </label>
      <div className="vk-commit-acts">
        <button
          aria-pressed={lockedAction === "commit"}
          className="vk-commit-act"
          data-action="commit"
          disabled={commitActionDisabled}
          onClick={() => void runCommitAction(false)}
          onFocus={() => setLockedAction("commit")}
          ref={(element) => {
            actionButtonRefs.current.commit = element;
          }}
          type="button"
        >
          <GitCommitHorizontal size={15} />
          <span>{t("gitCommitDialog.commit")}</span>
          {lockedAction === "commit" ? <kbd className="vk-kbd">Ctrl+↵</kbd> : null}
        </button>
        <button
          aria-pressed={lockedAction === "commitPush"}
          className="vk-commit-act"
          data-action="commitPush"
          disabled={commitActionDisabled}
          onClick={() => void runCommitAction(true)}
          onFocus={() => setLockedAction("commitPush")}
          ref={(element) => {
            actionButtonRefs.current.commitPush = element;
          }}
          type="button"
        >
          <CloudUpload size={15} />
          <span>{t("gitCommitDialog.commitAndPush")}</span>
          {lockedAction === "commitPush" ? <kbd className="vk-kbd">Ctrl+↵</kbd> : null}
        </button>
        <button
          aria-pressed={lockedAction === "push"}
          className="vk-commit-act"
          data-action="push"
          disabled={pushOnlyActionDisabled}
          onClick={() => void runPushAction()}
          onFocus={() => setLockedAction("push")}
          ref={(element) => {
            actionButtonRefs.current.push = element;
          }}
          type="button"
        >
          <CloudUpload size={15} />
          <span>{t("gitCommitDialog.push")}</span>
          {lockedAction === "push" ? <kbd className="vk-kbd">Ctrl+↵</kbd> : null}
        </button>
      </div>
    </div>
  );
}

// showGitCommitActionToast reports a completed Git action without forcing the dialog to stay open.
// showGitCommitActionToast 在不保持弹窗打开的情况下报告完成的 Git 操作。
function showGitCommitActionToast(response: GitCommitActionResponse | null | undefined, title: string): void {
  const details = [
    response?.commitHash ? `# ${response.commitHash}` : null,
    response?.commitMessage ? response.commitMessage : null,
  ].filter((item): item is string => Boolean(item));
  toast.success(title, {
    description: details.length > 0 ? details.join("\n") : response?.summary,
  });
}

// commitPanelTargetAllowsActionNavigation returns whether arrow keys should switch locked actions.
// commitPanelTargetAllowsActionNavigation 返回方向键是否应切换锁定动作。
function commitPanelTargetAllowsActionNavigation(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) {
    return true;
  }
  // Multi-line editors own vertical caret movement; single-line inputs still switch actions.
  // 多行编辑器自行处理光标的上下移动；单行输入框仍可切换动作。
  const tagName = target.tagName.toLowerCase();
  return tagName !== "select" && tagName !== "textarea" && !target.isContentEditable;
}
