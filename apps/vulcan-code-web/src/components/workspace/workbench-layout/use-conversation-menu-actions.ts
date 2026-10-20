import React from "react";
import { toast } from "@/ui-kit/primitives/toast";
import type { RenameTarget } from "@/components/project/project-action-dialogs";
import type { ConversationMenuAction } from "@/components/project/project-sidebar";
import { formatWorkbenchTranslation } from "@/components/workspace/workbench-layout/types";
import type { TranslationKey } from "@/i18n";
import {
  archiveConversation,
  deleteUnsupportedConversation,
  isUnsupportedSessionFormatError,
  openProjectDirectoryPath,
  setProjectTreePinned,
} from "@/service/project-actions-service";
import { projectForConversation } from "@/state/workbench-selectors";
import type { ConversationSummary, WorkbenchSnapshot } from "@/types";

// ConversationMenuActionsOptions supplies the workbench state and appliers used by conversation-tree commands.
// ConversationMenuActionsOptions 提供会话树命令使用的工作台状态与应用器。
export interface ConversationMenuActionsOptions {
  // Translates fixed workbench messages in the active locale.
  // 以当前语言翻译固定工作台消息。
  translate: (key: TranslationKey) => string;
  // Current workbench used to resolve the owning project and the archive release boundary.
  // 当前工作台，用于解析归属项目与归档释放边界。
  workbenchState: WorkbenchSnapshot;
  // Opens the shared rename dialog.
  // 打开共享的重命名弹窗。
  setRenameTarget: React.Dispatch<React.SetStateAction<RenameTarget | null>>;
  // Merges an accepted service snapshot; returns false when the revision gate rejects it.
  // 合并已接受的服务快照；版本闸门拒绝时返回 false。
  applyProjectActionSnapshot: (snapshot: WorkbenchSnapshot) => boolean;
  // Applies only the lifecycle fields of a rejected snapshot; returns false when nothing converged.
  // 仅应用被拒绝快照中的生命周期字段；未收敛时返回 false。
  applyLifecycleResponseSnapshot: (snapshot: WorkbenchSnapshot, conversationIds: string[]) => boolean;
}

// ConversationMenuActions exposes the local pin set and the conversation-tree command handler.
// ConversationMenuActions 暴露本地置顶集合与会话树命令处理器。
export interface ConversationMenuActions {
  // Conversations pinned locally until persistence is added.
  // 本地置顶的会话，直到后续接入持久化。
  pinnedConversationIds: Set<string>;
  // Executes one context-menu command for one conversation.
  // 为单个会话执行一个右键菜单命令。
  handleConversationMenuAction: (action: ConversationMenuAction, conversation: ConversationSummary) => Promise<void>;
  // Delete confirmation offered when archiving fails because the session format is no longer supported.
  // 归档因会话格式不再受支持而失败时提供的删除确认。
  unsupportedDelete: UnsupportedDeleteRequest;
}

// UnsupportedDeleteRequest drives the confirmation dialog for one unreadable old-format conversation.
// UnsupportedDeleteRequest 驱动一个无法读取的旧格式会话的确认弹窗。
export interface UnsupportedDeleteRequest {
  conversation: ConversationSummary | null;
  deleting: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

// useConversationMenuActions owns conversation-tree context-menu commands and their archive request locks.
// useConversationMenuActions 持有会话树右键菜单命令及其归档请求锁。
// Parameters: options supplies translation, workbench state, the rename dialog setter and snapshot appliers.
// 参数：options 提供翻译、工作台状态、重命名弹窗设置器与快照应用器。
// Returns: the pin set and the menu command handler.
// 返回值：置顶集合与菜单命令处理器。
export function useConversationMenuActions(options: ConversationMenuActionsOptions): ConversationMenuActions {
  const {
    translate,
    workbenchState,
    setRenameTarget,
    applyProjectActionSnapshot,
    applyLifecycleResponseSnapshot,
  } = options;
  // pinnedConversationIds lists the pinned sessions in the service's pinned order.
  // pinnedConversationIds 按服务的置顶顺序列出已置顶会话。
  const pins = workbenchState.projectTree?.pins;
  const pinnedConversationIds = React.useMemo(
    () => new Set((pins ?? []).filter((pin) => pin.kind === "session").map((pin) => pin.id)),
    [pins],
  );
  // conversationArchivePendingIdsRef blocks duplicate archive RPCs until failure or visible lifecycle convergence.
  // conversationArchivePendingIdsRef 在失败或可见生命周期收敛前阻止重复归档 RPC。
  const conversationArchivePendingIdsRef = React.useRef<Set<string>>(new Set());
  // unsupportedTarget is the old-format conversation awaiting the user's delete decision.
  // unsupportedTarget 是等待用户决定是否删除的旧格式会话。
  const [unsupportedTarget, setUnsupportedTarget] = React.useState<ConversationSummary | null>(null);
  const [deletingUnsupported, setDeletingUnsupported] = React.useState(false);

  // handleConversationMenuAction executes conversation-tree context menu commands.
  // handleConversationMenuAction 执行会话树右键菜单命令。
  const handleConversationMenuAction = React.useCallback(async (action: ConversationMenuAction, conversation: ConversationSummary) => {
    try {
      if (action === "rename") {
        setRenameTarget({ kind: "conversation", id: conversation.id, name: conversation.title });
        return;
      }
      if (action === "pin") {
        const pinned = !pinnedConversationIds.has(conversation.id);
        applyProjectActionSnapshot(await setProjectTreePinned("session", conversation.id, pinned));
        toast.success(formatWorkbenchTranslation(
          translate,
          pinned ? "workbenchConversation.pinned" : "workbenchConversation.unpinned",
          { conversation: conversation.title },
        ));
        return;
      }
      if (action === "archive") {
        if (conversationArchivePendingIdsRef.current.has(conversation.id)) {
          return;
        }
        conversationArchivePendingIdsRef.current.add(conversation.id);
        // archiveCommitted keeps the request locked after success until the active row actually disappears.
        // archiveCommitted 在成功后继续保持请求锁，直到活动行真正消失。
        let archiveCommitted = false;
        try {
          const snapshot = await archiveConversation(conversation.id);
          if (snapshot) {
            const snapshotApplied = applyProjectActionSnapshot(snapshot);
            if (!snapshotApplied && !applyLifecycleResponseSnapshot(snapshot, [conversation.id])) {
              throw new Error(translate("workspace.lifecycle.archiveNotConverged"));
            }
          } else {
            throw new Error(translate("workspace.lifecycle.archiveNoSnapshot"));
          }
          archiveCommitted = true;
          toast.success(formatWorkbenchTranslation(translate, "workbenchConversation.archived", { conversation: conversation.title }));
        } catch (archiveError) {
          // An old-format session can be neither opened nor archived; offer to delete it instead.
          // 旧格式会话既无法打开也无法归档；改为提供删除。
          if (!isUnsupportedSessionFormatError(archiveError)) throw archiveError;
          setUnsupportedTarget(conversation);
        } finally {
          if (!archiveCommitted) {
            conversationArchivePendingIdsRef.current.delete(conversation.id);
          }
        }
        return;
      }
      if (action === "open-directory") {
        const project = projectForConversation(workbenchState, conversation);
        if (!project) {
          toast.error(translate("workbenchConversation.noProject"));
          return;
        }
        const message = await openProjectDirectoryPath(project.path);
        toast.success(message ?? formatWorkbenchTranslation(translate, "projectActions.openRequested", { project: project.name }));
        return;
      }
      if (action === "copy-workdir") {
        const project = projectForConversation(workbenchState, conversation);
        if (!project) {
          toast.error(translate("workbenchConversation.noProject"));
          return;
        }
        await navigator.clipboard.writeText(project.path);
        toast.success(translate("workbenchConversation.copiedWorkspace"));
        return;
      }
      if (action === "copy-title") {
        await navigator.clipboard.writeText(conversation.title);
        toast.success(translate("workbenchConversation.copiedTitle"));
        return;
      }
      if (action === "copy-session-id") {
        await navigator.clipboard.writeText(conversation.id);
        toast.success(translate("workbenchConversation.copiedSessionId"));
        return;
      }
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    }
  }, [applyLifecycleResponseSnapshot, applyProjectActionSnapshot, pinnedConversationIds, setRenameTarget, translate, workbenchState]);

  // Release committed archive request locks only after the authoritative active row is no longer visible.
  // 仅在权威活动行不再可见后释放已提交归档请求锁。
  React.useEffect(() => {
    // activeConversationIds is the current lifecycle boundary rendered by the sidebar.
    // activeConversationIds 是侧栏当前渲染的生命周期边界。
    const activeConversationIds = new Set(workbenchState.conversations.map((conversation) => conversation.id));
    for (const conversationId of conversationArchivePendingIdsRef.current) {
      if (!activeConversationIds.has(conversationId)) {
        conversationArchivePendingIdsRef.current.delete(conversationId);
      }
    }
  }, [workbenchState.conversations]);

  // confirmUnsupportedDelete removes the old-format conversation the user confirmed.
  // confirmUnsupportedDelete 删除用户确认的旧格式会话。
  const confirmUnsupportedDelete = React.useCallback(async () => {
    if (!unsupportedTarget) return;
    setDeletingUnsupported(true);
    try {
      const snapshot = await deleteUnsupportedConversation(unsupportedTarget.id);
      if (!applyProjectActionSnapshot(snapshot)) applyLifecycleResponseSnapshot(snapshot, [unsupportedTarget.id]);
      toast.success(formatWorkbenchTranslation(translate, "workbenchConversation.unsupportedDeleted", { conversation: unsupportedTarget.title }));
      setUnsupportedTarget(null);
    } catch (deleteError) {
      toast.error(deleteError instanceof Error ? deleteError.message : String(deleteError));
    } finally {
      setDeletingUnsupported(false);
    }
  }, [applyLifecycleResponseSnapshot, applyProjectActionSnapshot, translate, unsupportedTarget]);

  const unsupportedDelete: UnsupportedDeleteRequest = {
    conversation: unsupportedTarget,
    deleting: deletingUnsupported,
    onConfirm: () => void confirmUnsupportedDelete(),
    onCancel: () => setUnsupportedTarget(null),
  };
  return { pinnedConversationIds, handleConversationMenuAction, unsupportedDelete };
}
