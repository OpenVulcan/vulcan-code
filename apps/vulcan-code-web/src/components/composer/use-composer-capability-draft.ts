import React from "react";
import {
  COMPOSER_AI_CAPABILITY_OPTIONS,
  defaultAiCapabilityTargetOrder,
  defaultDisabledAiCapabilityKinds,
  defaultDisabledAiCapabilityTargetSelectors,
  serializeAiCapabilityTargetOrder,
  type AiCapabilityKind,
  type ComposerAiCapabilityConfigurationSave,
} from "@/components/composer/composer-model";
import type { AiCapabilitySummary } from "@/types";

// ComposerCapabilitySaveState drives capability dialog save feedback.
// ComposerCapabilitySaveState 驱动能力弹窗的保存反馈。
export type ComposerCapabilitySaveState = "idle" | "saving" | "saved";

// ComposerCapabilitySaveOptions controls post-save dialog behavior.
// ComposerCapabilitySaveOptions 控制保存后的弹窗行为。
export interface ComposerCapabilitySaveOptions {
  // Whether a successful save should close the dialog after feedback.
  // 成功保存后是否应在反馈结束后关闭弹窗。
  closeAfterSave?: boolean;
}

// UseComposerCapabilityDraftOptions defines one scoped capability dialog persistence boundary.
// UseComposerCapabilityDraftOptions 定义一个有作用域的能力弹窗持久化边界。
export interface UseComposerCapabilityDraftOptions {
  // Stable conversation or new-conversation draft slot that owns this dialog.
  // 拥有该弹窗的稳定会话或新建会话草稿槽。
  scopeKey: string;
  // Session-synchronized disabled capability kinds.
  // 与会话同步的禁用能力类型。
  disabledKinds: Set<AiCapabilityKind>;
  // Session-synchronized disabled target selectors.
  // 与会话同步的禁用目标选择器。
  disabledTargetSelectors: Set<string>;
  // Session-synchronized target order grouped by capability kind.
  // 按能力类型分组、与会话同步的目标顺序。
  targetOrder: Map<AiCapabilityKind, string[]>;
  // Service-projected default capability tree used to stage reset values.
  // 用于暂存重置值的服务端默认能力树。
  defaultCapabilities: AiCapabilitySummary[];
  // Persists one complete session-scoped capability configuration.
  // 持久化一份完整的会话级能力配置。
  onSaveConfiguration: (request: ComposerAiCapabilityConfigurationSave) => Promise<boolean>;
}

// ComposerCapabilityDraftController exposes dialog lifecycle, draft state, and mutation actions.
// ComposerCapabilityDraftController 暴露弹窗生命周期、草稿状态与修改操作。
export interface ComposerCapabilityDraftController {
  // Whether the main capability dialog is visible.
  // 主能力弹窗是否可见。
  open: boolean;
  // Whether the dirty-close confirmation dialog is visible.
  // 脏关闭确认弹窗是否可见。
  confirmCloseOpen: boolean;
  // Dialog-local disabled capability kinds.
  // 弹窗本地禁用能力类型。
  draftDisabledKinds: Set<AiCapabilityKind>;
  // Dialog-local disabled target selectors.
  // 弹窗本地禁用目标选择器。
  draftDisabledTargetSelectors: Set<string>;
  // Dialog-local target order grouped by capability kind.
  // 按能力类型分组的弹窗本地目标顺序。
  draftTargetOrder: Map<AiCapabilityKind, string[]>;
  // Trigger-state disabled kinds selected from draft or persisted state as appropriate.
  // 根据弹窗状态从草稿或持久化状态选择的触发器禁用类型。
  activeDisabledKinds: Set<AiCapabilityKind>;
  // Whether the open dialog contains unsaved changes.
  // 打开的弹窗是否包含未保存变更。
  hasUnsavedChanges: boolean;
  // Whether one save request currently owns the synchronous gate.
  // 当前是否有一个保存请求持有同步门控。
  isSaving: boolean;
  // Visible save feedback state.
  // 可见保存反馈状态。
  saveState: ComposerCapabilitySaveState;
  // Whether an external session snapshot changed while the local draft was dirty.
  // 本地草稿脏时外部会话快照是否发生变化。
  externalUpdateNotice: boolean;
  // Applies a requested main-dialog visibility transition with dirty-close protection.
  // 应用带脏关闭保护的主弹窗可见性转换。
  requestOpenChange: (open: boolean) => void;
  // Closes only the dirty-close confirmation and keeps editing.
  // 仅关闭脏关闭确认并继续编辑。
  keepEditing: () => void;
  // Persists the complete local draft.
  // 持久化完整本地草稿。
  saveDraft: (options?: ComposerCapabilitySaveOptions) => Promise<boolean>;
  // Returns saved feedback to the normal idle state.
  // 将已保存反馈恢复为普通空闲状态。
  resetSavedState: () => void;
  // Discards local changes and closes both dialogs.
  // 放弃本地变更并关闭两个弹窗。
  discardDraft: () => void;
  // Stages the service-projected default configuration without saving.
  // 暂存服务投影的默认配置但不保存。
  useDefaultConfiguration: () => void;
  // Updates one dialog-local capability enabled value.
  // 更新一个弹窗本地能力启用值。
  setCapabilityEnabled: (kind: AiCapabilityKind, enabled: boolean) => void;
  // Updates one dialog-local target enabled value.
  // 更新一个弹窗本地目标启用值。
  setTargetEnabled: (targetSelector: string, enabled: boolean) => void;
  // Replaces one dialog-local capability target order.
  // 替换一个弹窗本地能力目标顺序。
  reorderTargets: (kind: AiCapabilityKind, targetSelectors: string[]) => void;
}

// useComposerCapabilityDraft owns scoped dialog state, save concurrency, external updates, and delayed close cleanup.
// useComposerCapabilityDraft 管理有作用域的弹窗状态、保存并发、外部更新与延迟关闭清理。
// Parameters: options provides the owning scope, persisted/default snapshots, and complete-save callback.
// 参数：options 提供所属作用域、持久化/默认快照与完整保存回调。
// Returns: dialog state plus guarded open, save, discard, reset, toggle, and reorder actions.
// 返回值：弹窗状态及受保护的打开、保存、放弃、重置、切换与排序操作。
export function useComposerCapabilityDraft(
  options: UseComposerCapabilityDraftOptions,
): ComposerCapabilityDraftController {
  // open tracks whether the full capability dialog is visible.
  // open 跟踪完整能力弹窗是否可见。
  const [open, setOpen] = React.useState(false);
  // confirmCloseOpen tracks the dirty-close confirmation dialog.
  // confirmCloseOpen 跟踪脏关闭确认弹窗。
  const [confirmCloseOpen, setConfirmCloseOpen] = React.useState(false);
  // draftDisabledKinds stores dialog-local capability choices.
  // draftDisabledKinds 保存弹窗本地能力选择。
  const [draftDisabledKinds, setDraftDisabledKinds] = React.useState(() => cloneAiCapabilityKindSet(options.disabledKinds));
  // draftDisabledTargetSelectors stores dialog-local target choices.
  // draftDisabledTargetSelectors 保存弹窗本地目标选择。
  const [draftDisabledTargetSelectors, setDraftDisabledTargetSelectors] = React.useState(() => new Set(options.disabledTargetSelectors));
  // draftTargetOrder stores dialog-local route order choices.
  // draftTargetOrder 保存弹窗本地路由顺序选择。
  const [draftTargetOrder, setDraftTargetOrder] = React.useState(() => cloneAiCapabilityTargetOrder(options.targetOrder));
  // baselineSignature stores the persisted snapshot captured when editing starts.
  // baselineSignature 保存开始编辑时捕获的持久化快照签名。
  const [baselineSignature, setBaselineSignature] = React.useState(() => (
    composerCapabilityDraftSignature(options.disabledKinds, options.disabledTargetSelectors, options.targetOrder)
  ));
  // saveState drives visible save feedback.
  // saveState 驱动可见保存反馈。
  const [saveState, setSaveState] = React.useState<ComposerCapabilitySaveState>("idle");
  // externalUpdateNotice records a persisted snapshot change while the draft is dirty.
  // externalUpdateNotice 记录草稿脏时发生的持久化快照变化。
  const [externalUpdateNotice, setExternalUpdateNotice] = React.useState(false);
  // savingRef synchronously blocks duplicate saves before React state commits.
  // savingRef 在 React 状态提交前同步拦截重复保存。
  const savingRef = React.useRef(false);
  // activeScopeKeyRef prevents an old save or timer from mutating a newly selected session.
  // activeScopeKeyRef 防止旧保存或定时器修改新选中的会话。
  const activeScopeKeyRef = React.useRef(options.scopeKey);
  activeScopeKeyRef.current = options.scopeKey;
  // previousScopeKeyRef detects actual scope changes without reacting to same-scope snapshot refreshes.
  // previousScopeKeyRef 检测真实作用域变化，且不响应同作用域快照刷新。
  const previousScopeKeyRef = React.useRef(options.scopeKey);
  // saveRequestVersionRef invalidates old saves when scope or newer requests advance.
  // saveRequestVersionRef 在作用域或较新请求推进时使旧保存失效。
  const saveRequestVersionRef = React.useRef(0);
  // closeTimerRef owns the delayed save-and-close timer.
  // closeTimerRef 持有延迟保存并关闭定时器。
  const closeTimerRef = React.useRef<number | null>(null);
  // draftSignature identifies the complete local configuration.
  // draftSignature 标识完整本地配置。
  const draftSignature = React.useMemo(
    () => composerCapabilityDraftSignature(draftDisabledKinds, draftDisabledTargetSelectors, draftTargetOrder),
    [draftDisabledKinds, draftDisabledTargetSelectors, draftTargetOrder],
  );
  // sessionSignature identifies the complete persisted configuration.
  // sessionSignature 标识完整持久化配置。
  const sessionSignature = React.useMemo(
    () => composerCapabilityDraftSignature(options.disabledKinds, options.disabledTargetSelectors, options.targetOrder),
    [options.disabledKinds, options.disabledTargetSelectors, options.targetOrder],
  );
  // hasUnsavedChanges compares the open draft with its captured baseline.
  // hasUnsavedChanges 比较打开的草稿与其捕获基线。
  const hasUnsavedChanges = open && draftSignature !== baselineSignature;
  // isSaving mirrors the synchronous gate into render state.
  // isSaving 将同步门控镜像为渲染状态。
  const isSaving = saveState === "saving";
  // activeDisabledKinds keeps the closed trigger bound to persisted state instead of stale drafts.
  // activeDisabledKinds 让关闭触发器绑定持久化状态，而不是遗留草稿。
  const activeDisabledKinds = open ? draftDisabledKinds : options.disabledKinds;

  // clearCloseTimer cancels any delayed close before a new lifecycle transition.
  // clearCloseTimer 在新生命周期转换前取消任何延迟关闭。
  const clearCloseTimer = React.useCallback(() => {
    if (closeTimerRef.current !== null) {
      window.clearTimeout(closeTimerRef.current);
      closeTimerRef.current = null;
    }
  }, []);
  // resetDraftFromSnapshot replaces every draft dimension from the latest persisted snapshot.
  // resetDraftFromSnapshot 使用最新持久化快照替换全部草稿维度。
  const resetDraftFromSnapshot = React.useCallback(() => {
    // nextDisabledKinds isolates the mutable Set from persisted props.
    // nextDisabledKinds 将可变 Set 与持久化 props 隔离。
    const nextDisabledKinds = cloneAiCapabilityKindSet(options.disabledKinds);
    // nextDisabledTargetSelectors isolates the mutable selector Set from persisted props.
    // nextDisabledTargetSelectors 将可变选择器 Set 与持久化 props 隔离。
    const nextDisabledTargetSelectors = new Set(options.disabledTargetSelectors);
    // nextTargetOrder deep-clones selector arrays in the mutable order Map.
    // nextTargetOrder 深拷贝可变顺序 Map 中的选择器数组。
    const nextTargetOrder = cloneAiCapabilityTargetOrder(options.targetOrder);
    setDraftDisabledKinds(nextDisabledKinds);
    setDraftDisabledTargetSelectors(nextDisabledTargetSelectors);
    setDraftTargetOrder(nextTargetOrder);
    setBaselineSignature(composerCapabilityDraftSignature(nextDisabledKinds, nextDisabledTargetSelectors, nextTargetOrder));
    setSaveState("idle");
    setExternalUpdateNotice(false);
  }, [options.disabledKinds, options.disabledTargetSelectors, options.targetOrder]);

  React.useLayoutEffect(() => {
    if (previousScopeKeyRef.current === options.scopeKey) {
      return;
    }
    previousScopeKeyRef.current = options.scopeKey;
    saveRequestVersionRef.current += 1;
    savingRef.current = false;
    clearCloseTimer();
    setOpen(false);
    setConfirmCloseOpen(false);
    resetDraftFromSnapshot();
  }, [clearCloseTimer, options.scopeKey, resetDraftFromSnapshot]);

  React.useEffect(() => clearCloseTimer, [clearCloseTimer]);

  // requestOpenChange applies open/close transitions while preserving explicit dirty confirmation.
  // requestOpenChange 应用打开/关闭转换，同时保留显式脏确认。
  const requestOpenChange = React.useCallback((nextOpen: boolean) => {
    if (nextOpen) {
      clearCloseTimer();
      resetDraftFromSnapshot();
      setConfirmCloseOpen(false);
      setOpen(true);
      return;
    }
    if (savingRef.current) {
      return;
    }
    if (hasUnsavedChanges) {
      setConfirmCloseOpen(true);
      return;
    }
    clearCloseTimer();
    setOpen(false);
  }, [clearCloseTimer, hasUnsavedChanges, resetDraftFromSnapshot]);
  // keepEditing closes only the confirmation dialog.
  // keepEditing 仅关闭确认弹窗。
  const keepEditing = React.useCallback(() => {
    setConfirmCloseOpen(false);
  }, []);
  // saveDraft persists one exact scoped draft with synchronous duplicate and stale-result protection.
  // saveDraft 使用同步重复保护与过期结果保护持久化一份精确作用域草稿。
  const saveDraft = React.useCallback(async (saveOptions?: ComposerCapabilitySaveOptions) => {
    if (savingRef.current) {
      return false;
    }
    // requestScopeKey binds this save to the session visible when it started.
    // requestScopeKey 将本次保存绑定到启动时可见的会话。
    const requestScopeKey = options.scopeKey;
    // requestVersion identifies this save among scope changes and newer saves.
    // requestVersion 在作用域变化与较新保存中标识本次保存。
    const requestVersion = saveRequestVersionRef.current + 1;
    saveRequestVersionRef.current = requestVersion;
    savingRef.current = true;
    clearCloseTimer();
    setSaveState("saving");
    // request is the complete serialized dialog draft sent to persistence.
    // request 是发送到持久化层的完整序列化弹窗草稿。
    const request: ComposerAiCapabilityConfigurationSave = {
      disabledAiCapabilityKinds: Array.from(draftDisabledKinds),
      disabledAiCapabilityTargetSelectors: Array.from(draftDisabledTargetSelectors),
      aiCapabilityTargetOrderOverrides: serializeAiCapabilityTargetOrder(draftTargetOrder),
    };
    try {
      // saved records whether the owning session accepted the complete draft.
      // saved 记录所属会话是否接受完整草稿。
      const saved = await options.onSaveConfiguration(request);
      if (
        activeScopeKeyRef.current !== requestScopeKey
        || saveRequestVersionRef.current !== requestVersion
      ) {
        return false;
      }
      savingRef.current = false;
      if (!saved) {
        setSaveState("idle");
        return false;
      }
      setBaselineSignature(draftSignature);
      setSaveState("saved");
      setExternalUpdateNotice(false);
      setConfirmCloseOpen(false);
      if (saveOptions?.closeAfterSave) {
        closeTimerRef.current = window.setTimeout(() => {
          closeTimerRef.current = null;
          if (
            activeScopeKeyRef.current !== requestScopeKey
            || saveRequestVersionRef.current !== requestVersion
          ) {
            return;
          }
          setOpen(false);
          setConfirmCloseOpen(false);
          setSaveState("idle");
        }, 220);
      }
      return true;
    } catch (error) {
      if (saveRequestVersionRef.current === requestVersion) {
        savingRef.current = false;
        setSaveState("idle");
      }
      throw error;
    }
  }, [
    clearCloseTimer,
    draftDisabledKinds,
    draftDisabledTargetSelectors,
    draftSignature,
    draftTargetOrder,
    options.onSaveConfiguration,
    options.scopeKey,
  ]);
  // resetSavedState returns visible saved feedback to idle after pointer departure.
  // resetSavedState 在指针离开后将可见已保存反馈恢复为空闲。
  const resetSavedState = React.useCallback(() => {
    if (saveState === "saved") {
      setConfirmCloseOpen(false);
      setSaveState("idle");
    }
  }, [saveState]);
  // discardDraft cancels delayed close and closes both dialogs without applying local changes.
  // discardDraft 取消延迟关闭并关闭两个弹窗，且不应用本地变更。
  const discardDraft = React.useCallback(() => {
    clearCloseTimer();
    setConfirmCloseOpen(false);
    setOpen(false);
    setSaveState("idle");
    setExternalUpdateNotice(false);
  }, [clearCloseTimer]);
  // useDefaultConfiguration stages defaults as a new local draft.
  // useDefaultConfiguration 将默认值暂存为新的本地草稿。
  const useDefaultConfiguration = React.useCallback(() => {
    if (savingRef.current) {
      return;
    }
    clearCloseTimer();
    setDraftDisabledKinds(defaultDisabledAiCapabilityKinds(options.defaultCapabilities));
    setDraftDisabledTargetSelectors(defaultDisabledAiCapabilityTargetSelectors(options.defaultCapabilities));
    setDraftTargetOrder(defaultAiCapabilityTargetOrder(options.defaultCapabilities));
    setExternalUpdateNotice(false);
    setSaveState("idle");
  }, [clearCloseTimer, options.defaultCapabilities]);
  // setCapabilityEnabled updates one disabled-kind membership without mutating prior state.
  // setCapabilityEnabled 在不修改旧状态的情况下更新一个禁用类型成员关系。
  const setCapabilityEnabled = React.useCallback((kind: AiCapabilityKind, enabled: boolean) => {
    if (savingRef.current) {
      return;
    }
    clearCloseTimer();
    setDraftDisabledKinds((current) => {
      // next isolates the mutable Set before applying the requested membership.
      // next 在应用请求成员关系前隔离可变 Set。
      const next = new Set(current);
      if (enabled) {
        next.delete(kind);
      } else {
        next.add(kind);
      }
      return next;
    });
    setSaveState("idle");
  }, [clearCloseTimer]);
  // setTargetEnabled updates one disabled-target membership without mutating prior state.
  // setTargetEnabled 在不修改旧状态的情况下更新一个禁用目标成员关系。
  const setTargetEnabled = React.useCallback((targetSelector: string, enabled: boolean) => {
    if (savingRef.current) {
      return;
    }
    clearCloseTimer();
    setDraftDisabledTargetSelectors((current) => {
      // next isolates the mutable Set before applying the requested membership.
      // next 在应用请求成员关系前隔离可变 Set。
      const next = new Set(current);
      if (enabled) {
        next.delete(targetSelector);
      } else {
        next.add(targetSelector);
      }
      return next;
    });
    setSaveState("idle");
  }, [clearCloseTimer]);
  // reorderTargets replaces one capability order only when the requested sequence changed.
  // reorderTargets 仅在请求顺序变化时替换一个能力顺序。
  const reorderTargets = React.useCallback((kind: AiCapabilityKind, targetSelectors: string[]) => {
    if (savingRef.current) {
      return;
    }
    clearCloseTimer();
    setDraftTargetOrder((current) => {
      // next isolates the mutable Map before comparing and replacing one order.
      // next 在比较并替换一个顺序前隔离可变 Map。
      const next = new Map(current);
      // currentOrder is the exact prior selector sequence for this capability.
      // currentOrder 是该能力的精确旧选择器序列。
      const currentOrder = next.get(kind) ?? [];
      if (
        currentOrder.length === targetSelectors.length
        && currentOrder.every((selector, index) => selector === targetSelectors[index])
      ) {
        return current;
      }
      next.set(kind, targetSelectors);
      return next;
    });
    setSaveState("idle");
  }, [clearCloseTimer]);

  React.useEffect(() => {
    if (!open || savingRef.current || sessionSignature === baselineSignature) {
      return;
    }
    if (!hasUnsavedChanges) {
      resetDraftFromSnapshot();
      return;
    }
    setExternalUpdateNotice(true);
  }, [baselineSignature, hasUnsavedChanges, open, resetDraftFromSnapshot, sessionSignature]);

  return {
    open,
    confirmCloseOpen,
    draftDisabledKinds,
    draftDisabledTargetSelectors,
    draftTargetOrder,
    activeDisabledKinds,
    hasUnsavedChanges,
    isSaving,
    saveState,
    externalUpdateNotice,
    requestOpenChange,
    keepEditing,
    saveDraft,
    resetSavedState,
    discardDraft,
    useDefaultConfiguration,
    setCapabilityEnabled,
    setTargetEnabled,
    reorderTargets,
  };
}

// cloneAiCapabilityKindSet isolates one mutable capability-kind Set.
// cloneAiCapabilityKindSet 隔离一个可变能力类型 Set。
// Parameters: value is the source Set to clone.
// 参数：value 是待克隆的源 Set。
// Returns: a new Set with identical capability kinds.
// 返回值：包含相同能力类型的新 Set。
function cloneAiCapabilityKindSet(value: Set<AiCapabilityKind>): Set<AiCapabilityKind> {
  return new Set(value);
}

// cloneAiCapabilityTargetOrder deep-clones one mutable target-order Map.
// cloneAiCapabilityTargetOrder 深拷贝一个可变目标顺序 Map。
// Parameters: value is the source order Map with selector arrays.
// 参数：value 是包含选择器数组的源顺序 Map。
// Returns: a new Map containing cloned selector arrays.
// 返回值：包含克隆选择器数组的新 Map。
function cloneAiCapabilityTargetOrder(value: Map<AiCapabilityKind, string[]>): Map<AiCapabilityKind, string[]> {
  return new Map(Array.from(value.entries()).map(([kind, selectors]) => [kind, [...selectors]]));
}

// composerCapabilityDraftSignature serializes all submission-relevant draft dimensions deterministically.
// composerCapabilityDraftSignature 以确定性方式序列化全部与提交相关的草稿维度。
// Parameters: disabledKinds, disabledTargetSelectors, and targetOrder form the complete capability draft.
// 参数：disabledKinds、disabledTargetSelectors 与 targetOrder 组成完整能力草稿。
// Returns: a stable JSON signature used for dirty and external-update detection.
// 返回值：用于脏检查与外部更新检测的稳定 JSON 签名。
function composerCapabilityDraftSignature(
  disabledKinds: Set<AiCapabilityKind>,
  disabledTargetSelectors: Set<string>,
  targetOrder: Map<AiCapabilityKind, string[]>,
): string {
  return JSON.stringify({
    disabledKinds: Array.from(disabledKinds).sort(),
    disabledTargetSelectors: Array.from(disabledTargetSelectors).sort(),
    targetOrder: COMPOSER_AI_CAPABILITY_OPTIONS.map((option) => [
      option.value,
      targetOrder.get(option.value) ?? [],
    ]),
  });
}
