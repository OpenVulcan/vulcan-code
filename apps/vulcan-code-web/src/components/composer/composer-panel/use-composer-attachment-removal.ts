import React from "react";
import { composerAttachmentIdentity } from "@/service/composer-attachment-reference";
import type { ComposerAttachmentSummary } from "@/types";
import type { ComposerAttachmentPreparationController } from "./use-composer-attachment-preparation";

// ComposerAttachmentRemovalOptions binds removal to a committed draft and its existing preparation controller.
// ComposerAttachmentRemovalOptions 将移除绑定到已提交草稿及其现有准备控制器。
interface ComposerAttachmentRemovalOptions {
  // draftSlotKey identifies the owner that must not change across asynchronous removal.
  // draftSlotKey 标识异步移除期间不得变化的所有者。
  draftSlotKey: string;
  // attachments are the canonical objects currently rendered in this draft.
  // attachments 是当前草稿渲染的规范对象。
  attachments: ComposerAttachmentSummary[];
  // preparation provides settled cancellation and serialized lease replacement.
  // preparation 提供已结算取消与串行租约替换。
  preparation: ComposerAttachmentPreparationController;
  // remove removes the exact captured object using a functional state update.
  // remove 通过函数式状态更新移除精确捕获对象。
  remove: (attachment: ComposerAttachmentSummary) => void;
}

// useComposerAttachmentRemoval keeps source release and UI removal behind confirmed cancellation.
// useComposerAttachmentRemoval 将来源释放与界面移除保持在已确认取消之后。
// Parameter: options supplies the current owner and existing mutations; returns an awaitable index handler.
// 参数：options 提供当前所有者与现有变更方法；返回可等待的索引处理函数。
// A concurrent replacement restores its exact lease and requires another deliberate removal action.
// 并发替换会恢复其精确租约，并要求再次明确执行移除动作。
export function useComposerAttachmentRemoval(options: ComposerAttachmentRemovalOptions): (index: number) => Promise<void> {
  // latest contains committed UI facts, not an abandoned concurrent render.
  // latest 包含已提交界面事实，而非被放弃的并发渲染。
  const latest = React.useRef(options);
  React.useLayoutEffect(() => { latest.current = options; }, [options]);
  return React.useCallback(async (index: number) => {
    // owner and attachment freeze the exact click target before cancellation can yield.
    // owner 与 attachment 在取消让出执行前冻结精确点击目标。
    const owner = latest.current;
    const attachment = owner.attachments[index];
    if (!attachment) return;
    // attachmentId uses the same occurrence identity as the preparation controller.
    // attachmentId 使用与准备控制器相同的出现身份。
    const attachmentId = composerAttachmentIdentity(attachment, index);
    if (attachmentId) await owner.preparation.cancel(attachmentId);
    // current includes additions made during cancellation, without crossing into another draft.
    // current 包含取消期间新增的附件，但不会跨入其他草稿。
    const current = latest.current;
    if (current.draftSlotKey !== owner.draftSlotKey || !current.attachments.includes(attachment)) return;
    // remaining is the exact post-removal lease set, including concurrent additions.
    // remaining 是精确的移除后租约集合，包含并发新增项。
    const remaining = current.attachments.filter((candidate) => candidate !== attachment);
    await current.preparation.reconcileDraft(remaining);
    // settled detects a replacement that occurred while the lease RPC was in flight.
    // settled 检测租约 RPC 在途期间发生的替换。
    const settled = latest.current;
    if (settled.draftSlotKey !== current.draftSlotKey || settled.attachments !== current.attachments) {
      await current.preparation.reconcileDraft(settled.draftSlotKey === current.draftSlotKey
        ? settled.attachments
        : current.attachments);
      return;
    }
    current.remove(attachment);
  }, []);
}
