import React from "react";
import { toast } from "sonner";
import { useI18n } from "@/i18n";
import type { PermissionMode, UnattendedConfig } from "@/service/permission-mode";
import {
  loadSessionUnattendedDefault,
  setSessionPermissionMode,
  setSessionUnattended,
  type UnattendedDefault,
} from "@/service/permission-mode-service";

/** Session default every new conversation starts with unless its draft chooses another mode.
 * 新会话在草稿未选择其他模式时使用的会话默认值。 */
export const DEFAULT_PERMISSION_MODE: PermissionMode = "workspace_write";

/** Permission mode bound to one hydrated session and history generation.
 * 绑定到一个已加载会话及历史代次的权限模式。 */
export interface PermissionModeOwner {
  /** Exact session identity.
   * 精确会话身份。 */
  sessionId: string;
  /** Exact history generation.
   * 精确历史代次。 */
  generation: number;
  /** Authoritative persisted mode.
   * 权威持久模式。 */
  mode: PermissionMode;
  /** Authoritative unattended mode; null while it is off.
   * 权威无人值守模式；关闭时为 null。 */
  unattended: UnattendedConfig | null;
}

/** Inputs bind the selector and asynchronous receipts to the exact visible draft or session.
 * 输入把选择器及异步回执绑定到精确可见草稿或会话。 */
interface PermissionModeComposerOptions {
  /** Stable draft slot, including unmaterialized conversations.
   * 稳定草稿槽，包含尚未物化的会话。 */
  scopeKey: string;
  /** Existing session, or null for a new draft.
   * 已有会话；新草稿为空。 */
  sessionId: string | null;
  /** Core mode for the current hydrated generation, or null while loading.
   * 当前已加载代次的 Core 模式；加载期间为空。 */
  authority: PermissionModeOwner | null;
  /** Explicit draft choice owned by the atomic first-submission settings capture.
   * 原子首次提交设置捕获持有的显式草稿选择。 */
  draftMode: PermissionMode | null;
  /** Synchronously update only the captured draft slot.
   * 同步更新捕获的草稿槽。 */
  setDraftMode: (mode: PermissionMode) => void;
}

/** Own the composer permission mode; returns rendered state and a send-time persistence barrier.
 * 管理输入区权限模式；返回显示状态和发送时的持久化屏障。
 * `options` carries authoritative ownership; late or failed writes cannot alter another scope.
 * `options` 携带权威归属；迟到或失败的写入不能修改其他作用域。 */
export function useComposerPermissionMode(options: PermissionModeComposerOptions) {
  // t resolves all feedback from the shared locale catalog.
  // t 从共享语言目录解析全部反馈。
  const { t } = useI18n();
  // ownerKey invalidates old receipts on selection changes and history rewrites.
  // ownerKey 在选择变化及历史重写时使旧回执失效。
  const ownerKey = `${options.scopeKey}:${options.authority?.generation ?? "loading"}`;
  const activeOwner = React.useRef({ key: ownerKey });
  if (activeOwner.current.key !== ownerKey) activeOwner.current = { key: ownerKey };
  const owner = activeOwner.current;
  const mounted = React.useRef(true);
  React.useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);
  // accepted keeps a saved receipt only while the projection still shows the mode it replaced.
  // accepted 仅在投影仍显示被替换的模式时保留已保存回执。
  const [accepted, setAccepted] = React.useState<{ key: string; base: PermissionMode; mode: PermissionMode } | null>(null);
  const [savingOwner, setSavingOwner] = React.useState<typeof owner | null>(null);
  // Unattended mode exists only for a saved auto-approve session; a receipt stays shown until the
  // projection moves past the state it replaced.
  // 无人值守模式只适用于已保存的自动审批会话；回执会一直显示，直到投影越过它所替换的状态。
  const [acceptedUnattended, setAcceptedUnattended] = React.useState<{
    key: string; base: string | null; value: UnattendedConfig | null;
  } | null>(null);
  // pending synchronously deduplicates selections and allows submit to await this exact write.
  // pending 同步去重选择，并允许提交等待这次精确写入。
  const pending = React.useRef<{ owner: typeof owner; promise: Promise<boolean> } | null>(null);
  const authority = options.authority?.sessionId === options.sessionId ? options.authority : null;
  const current = authority !== null && accepted?.key === ownerKey && accepted.base === authority.mode
    ? accepted.mode : authority?.mode ?? null;
  const mode: PermissionMode | null = options.sessionId === null
    ? options.draftMode ?? DEFAULT_PERMISSION_MODE
    : current;
  const saving = savingOwner === owner;

  // select changes drafts synchronously or saves one existing-session update.
  // select 同步修改草稿，或保存一次已有会话更新。
  const select = React.useCallback((next: PermissionMode) => {
    if (options.sessionId === null) {
      options.setDraftMode(next);
      return;
    }
    if (authority === null || current === next || pending.current?.owner === owner) return;
    const sessionId = options.sessionId;
    const base = authority.mode;
    const unattendedBase = authority.unattended?.deny_policy ?? null;
    setSavingOwner(owner);
    const promise = setSessionPermissionMode(sessionId, next).then((response) => {
      if (!mounted.current || activeOwner.current !== owner) return false;
      if (response.session_id !== sessionId) throw new Error("Permission mode response owner changed.");
      setAccepted({ key: ownerKey, base, mode: response.permission_mode });
      // Leaving auto-approve turns unattended mode off; show that before the projection catches up.
      // 离开自动审批会关闭无人值守模式；在投影追上之前先显示该结果。
      setAcceptedUnattended({ key: ownerKey, base: unattendedBase, value: response.unattended });
      return true;
    }).catch(() => {
      if (mounted.current && activeOwner.current === owner) toast.error(t("permissionMode.saveFailed"));
      return false;
    }).finally(() => {
      if (pending.current?.promise === promise) pending.current = null;
      if (mounted.current && activeOwner.current === owner) setSavingOwner(null);
    });
    pending.current = { owner, promise };
  }, [authority, current, options, owner, ownerKey, t]);

  // flush waits only for this owner; it never transfers an old session's receipt to a new draft.
  // flush 只等待当前归属；绝不把旧会话回执转给新草稿。
  const flush = React.useCallback(async () => {
    const write = pending.current;
    return write?.owner === owner ? await write.promise : true;
  }, [owner]);

  const [savingUnattended, setSavingUnattended] = React.useState<typeof owner | null>(null);
  const baseUnattended = authority?.unattended?.deny_policy ?? null;
  const unattended = authority === null ? null
    : acceptedUnattended?.key === ownerKey && acceptedUnattended.base === baseUnattended
      ? acceptedUnattended.value : authority.unattended;
  // applyUnattended saves the confirmed policy, or turns the mode off with null.
  // applyUnattended 保存确认的策略，或以 null 关闭该模式。
  const applyUnattended = React.useCallback(async (next: UnattendedConfig | null) => {
    if (options.sessionId === null || authority === null) return false;
    const sessionId = options.sessionId;
    setSavingUnattended(owner);
    try {
      const response = await setSessionUnattended(sessionId, next);
      if (!mounted.current || activeOwner.current !== owner) return false;
      if (response.session_id !== sessionId) throw new Error("Unattended mode response owner changed.");
      setAcceptedUnattended({ key: ownerKey, base: baseUnattended, value: response.unattended });
      return true;
    } catch {
      if (mounted.current && activeOwner.current === owner) toast.error(t("permissionMode.unattended.saveFailed"));
      return false;
    } finally {
      if (mounted.current && activeOwner.current === owner) setSavingUnattended(null);
    }
  }, [authority, baseUnattended, options.sessionId, owner, ownerKey, t]);
  // loadUnattendedDefault reads the prefill for the enable dialog; it applies nothing.
  // loadUnattendedDefault 读取开启对话框的预填值；不会应用任何内容。
  const loadUnattendedDefault = React.useCallback(async (): Promise<UnattendedDefault | null> => {
    return options.sessionId === null ? null : await loadSessionUnattendedDefault(options.sessionId);
  }, [options.sessionId]);
  return {
    mode, saving, available: mode !== null, select, flush,
    unattended: {
      value: unattended,
      available: options.sessionId !== null && authority !== null,
      saving: savingUnattended === owner,
      apply: applyUnattended,
      loadDefault: loadUnattendedDefault,
    },
  };
}

/** Controller shape consumed by the conversation settings menu.
 * 会话设置菜单使用的控制器形状。 */
export type ComposerPermissionModeController = ReturnType<typeof useComposerPermissionMode>;
