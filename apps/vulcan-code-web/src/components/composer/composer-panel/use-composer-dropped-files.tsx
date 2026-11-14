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
type DroppedFile = {
  // Browser-owned original; absent after reloading the client.
  // 浏览器持有的原件，客户端刷新后为空。
  file: File | null;
  // Display name identifies the item if the browser cannot restore its original handle.
  // 浏览器无法恢复原件句柄时，用展示名称识别此项。
  name: string;
  // Genuine native path or explicit dropped URI, when the client provided one.
  // 客户端提供的真实原生路径或明确拖入 URI。
  reference: string | null;
};

// Session storage keeps only local reference metadata and isolates independent browser tabs.
// 会话存储只保存本地引用元数据，并隔离不同浏览器标签页。
const DROP_DRAFT_STORAGE_PREFIX = "vulcan.composer.pending-drops.v1:";

// Restores one draft's reference list, leaving its browser-owned file handles empty.
// 恢复单份草稿的引用列表，浏览器持有的文件句柄保持为空。
function readDroppedDraft(owner: string): DroppedFile[] {
  try {
    const stored = window.sessionStorage?.getItem(DROP_DRAFT_STORAGE_PREFIX + owner);
    if (!stored) return [];
    const values: unknown = JSON.parse(stored);
    if (!Array.isArray(values)) return [];
    return values.flatMap((entry: unknown) => {
      if (typeof entry !== "object" || entry === null || !("name" in entry)
        || typeof entry.name !== "string" || !("reference" in entry)
        || (entry.reference !== null && typeof entry.reference !== "string")) return [];
      return [{ file: null, name: entry.name, reference: entry.reference }];
    });
  } catch {
    return [];
  }
}

// Writes the recoverable references and display names for one draft without storing bytes.
// 写入单份草稿可恢复的引用与展示名称，无需存储字节。
function writeDroppedDraft(owner: string, files: readonly DroppedFile[]): void {
  const key = DROP_DRAFT_STORAGE_PREFIX + owner;
  if (files.length) {
    window.sessionStorage?.setItem(key, JSON.stringify(files.map(({ name, reference }) => ({ name, reference }))));
  } else {
    window.sessionStorage?.removeItem(key);
  }
}

// The live owner and route are read again after pending model writes finish.
// 待生效模型写入完成后，重新读取活动所有者与路线。
interface DroppedFilesOptions {
  draftSlotKey: string;
  projectId: string | null;
  conversationId: string | null;
  modelSelection: ComposerModelSelectionSnapshot | null;
  routeRevision: string;
  attachments: ComposerAttachmentSummary[];
  projectAttachments: (attachments: ComposerAttachmentSummary[]) => ComposerAttachmentSummary[];
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
  // Cache restored entries so removing or completing an item retains exact object identity.
  // 缓存恢复条目，使移除或完成操作保留精确对象身份。
  const restoredDrafts = React.useRef(new Map<string, DroppedFile[]>());
  const latest = React.useRef(options);
  latest.current = options;
  const ownerFiles = drafts.get(options.draftSlotKey);
  if (!ownerFiles && !restoredDrafts.current.has(options.draftSlotKey)) {
    restoredDrafts.current.set(options.draftSlotKey, readDroppedDraft(options.draftSlotKey));
  }
  const files = ownerFiles ?? restoredDrafts.current.get(options.draftSlotKey) ?? [];
  const checking = React.useRef<string | null>(null);
  const [checkingSlot, setCheckingSlot] = React.useState<string | null>(null);
  // Keep inspection and submission mutually exclusive for this draft.
  // 当前草稿的拖入检查和提交准备互斥。
  const preparing = React.useRef(false);

  // Commits an exact owner list immediately so refresh during a pending send can recover the references.
  // 立即提交精确所有者的列表，使发送等待期间刷新也可恢复引用。
  function updateDroppedDraft(owner: string, next: DroppedFile[]): void {
    setDrafts(previous => new Map(previous).set(owner, next));
    try {
      writeDroppedDraft(owner, next);
    } catch (error) {
      toast.error(t("composer.error.draftPersistFailed"), {
        description: error instanceof Error ? error.message : String(error),
      });
    }
  }

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
      file, name: file.name, reference: droppedReference(file, incoming.length === 1 && uris.length === 1 ? uris[0] : null),
    }));
    checking.current = owner;
    setCheckingSlot(owner);
    try {
      if (!await options.flushRoute() || latest.current.draftSlotKey !== owner) return;
      const current = latest.current;
      // Include originals already pending so the same route budget is used across multiple drops.
      // 包含已待传的原文件，使多次拖放共用同一路线预算。
      const existing = files.flatMap(item => item.file ? [item.file] : []);
      const { eligible } = await classifyDroppedComposerFiles({
        projectId: current.projectId, conversationId: current.conversationId, draftSlotKey: owner,
        modelSelection: current.modelSelection, attachments: current.projectAttachments(current.attachments),
        maxNewAttachments: Math.max(0, MAX_COMPOSER_ATTACHMENTS - current.attachments.length),
        files: [...existing, ...incoming], forSubmission: false,
      });
      const plans = eligible.slice(existing.length);
      if (!sameDropTarget(latest.current, current)) return;
      const references: string[] = [];
      const pending: DroppedFile[] = [];
      let missingPath = false;
      for (const [index, item] of candidates.entries()) {
        if (plans[index]) pending.push(item);
        else if (item.reference) references.push(item.reference);
        else missingPath = true;
      }
      // One drop replaces at most one recovered occurrence of the same source.
      // 每个拖入文件最多替换一个同源的恢复条目。
      const retained = files.slice();
      for (const candidate of candidates) {
        const replacement = retained.findIndex(item => !item.file && (
          item.reference ? item.reference === candidate.reference : item.name === candidate.name
        ));
        if (replacement >= 0 && (candidate.reference || pending.includes(candidate))) retained.splice(replacement, 1);
      }
      if (missingPath) toast.info(t("composer.attachment.dropPathUnavailable"));
      if (references.length) latest.current.setDraftInput(input => appendReferences(input, references));
      if (retained.length !== files.length || pending.length) {
        updateDroppedDraft(owner, [...retained, ...pending]);
      }
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
  async function prepare(prompt: string, submittedAttachments: ComposerAttachmentSummary[]): Promise<{
    prompt: string; attachments: ComposerAttachmentSummary[]; planId?: string;
    complete: () => void; rollback: (restoredAttachments?: ComposerAttachmentSummary[]) => Promise<void>;
  } | null> {
    const owner = options.draftSlotKey;
    if (!files.length) return null;
    if (checking.current !== null || preparing.current || !sameDropTarget(latest.current, options)) return null;
    preparing.current = true;
    setCheckingSlot(owner);
    const current = latest.current;
    const attachments: ComposerAttachmentSummary[] = [];
    let handedToSubmission = false;
    // Release the draft barrier after the send or its rollback actually settles.
    // 发送或回滚真正结算后才释放草稿屏障。
    function finish(): void {
      preparing.current = false;
      setCheckingSlot(null);
    }
    // Dispose of draft references for files prepared by an abandoned or rejected submission.
    // 清理被放弃或拒绝的提交所准备的文件草稿引用。
    async function rollback(restoredAttachments?: ComposerAttachmentSummary[]): Promise<void> {
      try {
        if (attachments.length) {
          // Use the restored snapshot directly because React may still render the cleared draft.
          // 直接使用恢复快照，因为 React 可能仍渲染已清空的草稿。
          const existing = restoredAttachments
            ?? (latest.current.draftSlotKey === owner ? latest.current.attachments : current.attachments);
          await reconcileComposerAttachmentDraftLease(current.projectId, current.conversationId, owner, existing);
        }
      } catch (error) {
        if (latest.current.draftSlotKey === owner) {
          toast.error(t("composer.error.draftPersistFailed"), {
            description: error instanceof Error ? error.message : String(error),
          });
        }
      } finally {
        finish();
      }
    }
    try {
      if (files.some(item => !item.file && !item.reference)) {
        toast.info(t("composer.attachment.dropSourceUnavailable"));
        return null;
      }
      const originals = files.flatMap(item => item.file ? [item.file] : []);
      const { eligible, planId } = originals.length ? await classifyDroppedComposerFiles({
        projectId: current.projectId, conversationId: current.conversationId, draftSlotKey: owner,
        modelSelection: current.modelSelection, attachments: submittedAttachments,
        maxNewAttachments: Math.max(0, MAX_COMPOSER_ATTACHMENTS - submittedAttachments.length),
        files: originals, forSubmission: true,
      }) : { eligible: [], planId: null };
      if (latest.current.draftSlotKey !== owner) return null;
      let decisionIndex = 0;
      const decisions = files.map((item) => Boolean(item.file && eligible[decisionIndex++]));
      if (files.some((item, index) => !decisions[index] && !item.reference)) {
        toast.info(t("composer.attachment.dropPathUnavailable"));
        return null;
      }
      const references: string[] = [];
      // Stage indices count only originals because recovered references have no wire file.
      // 暂存索引只统计原件，因为已恢复引用没有线上文件。
      let stageIndex = 0;
      for (const [index, item] of files.entries()) {
        if (latest.current.draftSlotKey !== owner) return null;
        if (item.file) {
          if (decisions[index]) {
            if (!planId) throw new Error("Core did not return a dropped-file send plan.");
            attachments.push(await stageDroppedComposerFile(planId, stageIndex, item.file));
          }
          stageIndex += 1;
        }
        if (!decisions[index] && item.reference) references.push(item.reference);
      }
      if (latest.current.draftSlotKey !== owner) return null;
      handedToSubmission = true;
      return {
        prompt: appendReferences(prompt, references),
        attachments,
        ...(planId ? { planId } : {}),
        rollback,
        complete: () => {
          updateDroppedDraft(owner, []);
          finish();
        },
      };
    } finally {
      if (!handedToSubmission) await rollback();
    }
  }

  // Render pending originals separately from the managed attachment strip.
  // 将待传原始文件与托管附件条分开渲染。
  const preview = files.length ? (
    <div>
      {files.map((item, index) => (
        <Button key={index} type="button" size="sm" variant="ghost" disabled={checkingSlot === options.draftSlotKey} title={t("composer.attachment.remove")}
          onClick={() => updateDroppedDraft(options.draftSlotKey, files.filter(file => file !== item))}>
          {item.file ? item.name : item.reference ?? item.name} ×
        </Button>
      ))}
    </div>
  ) : null;
  return { count: files.length, checking: checkingSlot === options.draftSlotKey, dragOver, drop, prepare, preview };
}
