import React from "react";
import { Button } from "@/ui-kit/primitives/button";
import { toast } from "@/ui-kit/primitives/toast";
import { useI18n } from "@/i18n";
import { readElectronLocalFileBridge } from "@/host-shell/electron-bridge";
import { classifyDroppedComposerFiles, reconcileComposerAttachmentDraftLease, stageDroppedComposerFile } from "@/service/conversation-actions-service";
import { MAX_COMPOSER_ATTACHMENTS, type ComposerModelSelectionSnapshot } from "../composer-model";
import type { ComposerAttachmentSummary } from "@/types";

// A dropped file holds its original browser handle and any verified reference until send.
// 拖入文件保留原始浏览器句柄和已验证引用，直到发送。
type DroppedFile = { file: File; reference: string | null };

// The live owner and route are read again after pending model writes finish.
// 待生效模型写入完成后，重新读取活动所有者与路线。
interface DroppedFilesOptions {
  draftSlotKey: string;
  projectId: string | null;
  conversationId: string | null;
  modelSelection: ComposerModelSelectionSnapshot | null;
  routeRevision: string;
  attachments: ComposerAttachmentSummary[];
  setDraftInput: React.Dispatch<React.SetStateAction<string>>;
  setDragOver: (active: boolean) => void;
  flushRoute: () => Promise<boolean>;
}

// Add actual addresses on separate lines so paths containing spaces remain intact.
// 将实际地址按行追加，保留包含空格的路径。
function appendReferences(input: string, references: string[]): string {
  return references.length ? [input.trimEnd(), ...references].filter(Boolean).join("\n") : input;
}

// Return the one explicitly dropped URI or native path associated with a file.
// 返回与文件关联的明确拖入 URI 或原生路径。
function droppedReference(file: File, uri: string | null): string | null {
  return readElectronLocalFileBridge()?.pathForFile?.(file) || uri;
}

// Compare the owner and the rendered model route while asynchronous work is outstanding.
// 在异步操作期间比较所属草稿和界面当前显示的模型路线。
function sameDropTarget(left: DroppedFilesOptions, right: DroppedFilesOptions): boolean {
  return left.draftSlotKey === right.draftSlotKey && left.modelSelection === right.modelSelection
    && left.routeRevision === right.routeRevision;
}

// Keep dropped file handles out of persistent drafts until Core confirms delivery at send time.
// 在 Core 于发送时确认交付之前，不将拖入文件句柄写入持久草稿。
export function useComposerDroppedFiles(options: DroppedFilesOptions) {
  const { t } = useI18n();
  // One component may switch among several drafts without releasing their browser file handles.
  // 同一组件可在多个草稿间切换，同时保留各自的浏览器文件句柄。
  const [drafts, setDrafts] = React.useState(() => new Map<string, DroppedFile[]>());
  const latest = React.useRef(options);
  latest.current = options;
  const files = drafts.get(options.draftSlotKey);
  const checking = React.useRef<string | null>(null);
  const [checkingSlot, setCheckingSlot] = React.useState<string | null>(null);
  // Keep inspection and submission mutually exclusive for this draft.
  // 当前草稿的拖入检查和提交准备互斥。
  const preparing = React.useRef(false);

  // Reserve the drop target for native files and URI lists, including path-only drops.
  // 为原生文件与 URI 列表保留拖放目标，包括仅路径的拖放。
  function dragOver(event: React.DragEvent<HTMLElement>): void {
    if (!Array.from(event.dataTransfer.types).some(type => type === "Files" || type === "text/uri-list")) return;
    event.preventDefault();
    event.dataTransfer.dropEffect = "copy";
    options.setDragOver(true);
  }

  // Capture native drag data synchronously, then ask Core about the selected delivery route.
  // 同步捕获原生拖放数据，再向 Core 查询选定交付路线。
  async function drop(event: React.DragEvent<HTMLElement>): Promise<void> {
    options.setDragOver(false);
    const transfer = event.dataTransfer;
    const uris = transfer.getData("text/uri-list").split(/\r?\n/).map(line => line.trim())
      .filter(line => line && !line.startsWith("#"));
    const incoming = Array.from(transfer.files);
    if (!incoming.length && !uris.length) return;
    event.preventDefault();
    const owner = options.draftSlotKey;
    if (checking.current !== null || preparing.current) return;
    if (!incoming.length) {
      options.setDraftInput(input => appendReferences(input, uris));
      return;
    }
    const candidates = incoming.map(file => ({
      file, reference: droppedReference(file, incoming.length === 1 && uris.length === 1 ? uris[0] : null),
    }));
    checking.current = owner;
    setCheckingSlot(owner);
    try {
      if (!await options.flushRoute() || latest.current.draftSlotKey !== owner) return;
      const current = latest.current;
      // Include originals already pending so the same route budget is used across multiple drops.
      // 包含已待传的原文件，使多次拖放共用同一路线预算。
      const existing = files ?? [];
      const plans = (await classifyDroppedComposerFiles(current.conversationId, current.modelSelection,
        [...existing.map(item => item.file), ...incoming])).slice(existing.length);
      if (!sameDropTarget(latest.current, current)) return;
      const references: string[] = [];
      const pending: DroppedFile[] = [];
      let missingPath = false;
      for (const [index, item] of candidates.entries()) {
        if (plans[index]) pending.push(item);
        else if (item.reference) references.push(item.reference);
        else missingPath = true;
      }
      const slots = Math.max(0, MAX_COMPOSER_ATTACHMENTS - latest.current.attachments.length - (files?.length ?? 0));
      if (pending.length > slots) toast.info(t("composer.attachment.partialMaxLimit").replace("{count}", String(MAX_COMPOSER_ATTACHMENTS)));
      if (missingPath) toast.info(t("composer.attachment.dropPathUnavailable"));
      if (references.length) latest.current.setDraftInput(input => appendReferences(input, references));
      if (slots && pending.length) setDrafts(previous => new Map(previous).set(owner, [
        ...(previous.get(owner) ?? []), ...pending.slice(0, slots),
      ]));
    } catch (error) {
      if (latest.current.draftSlotKey === owner) {
        toast.error(t("composer.attachment.unsupported"), { description: error instanceof Error ? error.message : String(error) });
      }
    } finally {
      checking.current = null;
      setCheckingSlot(null);
    }
  }

  // Reclassify against the actual sending route before reading any file or creating an asset.
  // 在读取任何文件或创建资产之前，按实际发送路线重新判定。
  async function prepare(prompt: string): Promise<{
    prompt: string; attachments: ComposerAttachmentSummary[]; complete: () => void; rollback: () => Promise<void>;
  } | null> {
    const owner = options.draftSlotKey;
    if (!files?.length) return null;
    if (checking.current !== null || preparing.current || !sameDropTarget(latest.current, options)) return null;
    preparing.current = true;
    setCheckingSlot(owner);
    const current = latest.current;
    const attachments: ComposerAttachmentSummary[] = [];
    let handedToSubmission = false;
    // Dispose of draft references for files prepared by an abandoned or rejected submission.
    // 清理被放弃或拒绝的提交所准备的文件草稿引用。
    async function rollback(): Promise<void> {
      if (!attachments.length) return;
      const existing = latest.current.draftSlotKey === owner ? latest.current.attachments : current.attachments;
      try {
        await reconcileComposerAttachmentDraftLease(current.projectId, current.conversationId, owner, existing);
      } catch (error) {
        if (latest.current.draftSlotKey === owner) {
          toast.error(t("composer.error.draftPersistFailed"), {
            description: error instanceof Error ? error.message : String(error),
          });
        }
      }
    }
    try {
      const plans = await classifyDroppedComposerFiles(current.conversationId, current.modelSelection, files.map(item => item.file));
      if (!sameDropTarget(latest.current, current)) return null;
      if (files.some((item, index) => !plans[index] && !item.reference)) {
        toast.info(t("composer.attachment.dropPathUnavailable"));
        return null;
      }
      const references: string[] = [];
      for (const [index, item] of files.entries()) {
        if (!sameDropTarget(latest.current, current)) return null;
        if (plans[index]) {
          const attachment = await stageDroppedComposerFile(
            current.projectId, current.conversationId, owner, current.modelSelection, item.file,
          );
          if (attachment) {
            attachments.push(attachment);
            continue;
          }
        }
        if (!item.reference) {
          toast.info(t("composer.attachment.dropPathUnavailable"));
          return null;
        }
        references.push(item.reference);
      }
      if (!sameDropTarget(latest.current, current)) return null;
      handedToSubmission = true;
      return {
        prompt: appendReferences(prompt, references),
        attachments,
        rollback,
        complete: () => setDrafts(previous => {
          const remaining = previous.get(owner)?.filter(item => !files.includes(item)) ?? [];
          const updated = new Map(previous);
          if (remaining.length) updated.set(owner, remaining);
          else updated.delete(owner);
          return updated;
        }),
      };
    } finally {
      if (!handedToSubmission) await rollback();
      preparing.current = false;
      setCheckingSlot(null);
    }
  }

  // Render pending originals separately from the managed attachment strip.
  // 将待传原始文件与托管附件条分开渲染。
  const preview = files?.length ? (
    <div>
      {files.map((item, index) => (
        <Button key={index} type="button" size="sm" variant="ghost" disabled={checkingSlot === options.draftSlotKey} title={t("composer.attachment.remove")}
          onClick={() => setDrafts(previous => new Map(previous).set(options.draftSlotKey,
            (previous.get(options.draftSlotKey) ?? []).filter(file => file !== item)))}>
          {item.file.name} ×
        </Button>
      ))}
    </div>
  ) : null;
  return { count: files?.length ?? 0, checking: checkingSlot === options.draftSlotKey, dragOver, drop, prepare, preview };
}
