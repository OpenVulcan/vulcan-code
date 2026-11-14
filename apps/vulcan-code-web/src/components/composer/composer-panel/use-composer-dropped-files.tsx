import React from "react";
import { toast } from "@/ui-kit/primitives/toast";
import { useI18n } from "@/i18n";
import { readElectronClipboardFileReferences, readElectronLocalFileBridge } from "@/host-shell/electron-bridge";
import { classifyDroppedComposerFiles, reconcileComposerAttachmentDraftLease, stageDroppedComposerFile } from "@/service/conversation-actions-service";
import { MAX_COMPOSER_ATTACHMENTS, type ComposerModelSelectionSnapshot } from "../composer-model";
import { ComposerLocalAttachmentTile } from "../composer-attachment-controls";
import { inferComposerAttachmentKindFromFile } from "../composer-attachment-media";
import type { UseComposerAttachmentsState } from "../composer-attachments";
import type { ComposerAttachmentSummary } from "@/types";

// Tracks one visible file or the original path of an installed managed attachment.
// 跟踪可见文件，或已安装托管附件的原始路径。
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
  // Stable asset identity after an admitted original enters the managed draft.
  // 已准入原件进入托管草稿后的稳定资产标识。
  assetId?: string;
  // Optional browser image retained for a thumbnail on a path-only tile.
  // 可选浏览器图像，用于仅引用路径附件块的缩略图。
  previewFile?: File;
};

// Identifies a captured original and its local reference for model admission.
// 标识用于模型准入的已捕获原件和本地引用。
type IncomingFile = DroppedFile & { file: File };

// Reads real URI entries in transfer order, omitting RFC 2483 comments.
// 按传输顺序读取真实 URI 条目，并忽略 RFC 2483 注释。
function transferUris(transfer: DataTransfer): string[] {
  return transfer.getData("text/uri-list").split(/\r?\n/).map(line => line.trim())
    .filter(line => line && !line.startsWith("#"));
}

// Treats compiled HTML Help as a local reference even on routes that accept other documents.
// 即使路线接受其他文档，编译版 HTML 帮助也仅作为本地引用。
function isCompiledHelp(name: string, mimeType: string): boolean {
  return /\.chm$/i.test(name) || mimeType === "application/vnd.ms-htmlhelp";
}

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
      return [{
        file: null, name: entry.name, reference: entry.reference,
        ...("assetId" in entry && typeof entry.assetId === "string" ? { assetId: entry.assetId } : {}),
      }];
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
    window.sessionStorage?.setItem(key, JSON.stringify(files.map(({ name, reference, assetId }) => ({ name, reference, assetId }))));
  } else {
    window.sessionStorage?.removeItem(key);
  }
}

// The live owner and route are read again after pending model writes finish.
// 待生效模型写入完成后，重新读取活动所有者与路线。
interface DroppedFilesOptions {
  draftSlotKey: string;
  // Separate UI drafts can share a service conversation while storing references independently.
  // 独立界面草稿可共用服务会话，各自保存本地引用。
  storageKey?: string;
  projectId: string | null;
  conversationId: string | null;
  modelSelection: ComposerModelSelectionSnapshot | null;
  routeRevision: string;
  attachments: ComposerAttachmentSummary[];
  projectAttachments: (attachments: ComposerAttachmentSummary[]) => ComposerAttachmentSummary[];
  // Installs admitted uploads in the managed draft, which owns compression, preview and transfer.
  // 将准入文件装入托管草稿，由草稿负责压缩、预览和传输。
  stageApprovedFiles: UseComposerAttachmentsState["stageApprovedComposerFiles"];
  // The file picker can reserve managed slots while preflight is taking place.
  // 文件选择器可在预检期间预留托管附件槽位。
  pendingAttachmentCount: number;
  // Compact surfaces may use the Core send plan rather than a live preparation planner.
  // 紧凑界面可使用 Core 发送规划，无需实时准备规划器。
  recheckManagedAtSend?: boolean;
  setDragOver: (active: boolean) => void;
  flushRoute: () => Promise<boolean>;
}

// Wraps the existing conversion handler with a live bridge for local source tracking.
// 为既有转换处理器包装用于追踪本地来源的实时桥接。
export function useComposerDroppedAssetConversion(install: (replacements: ReadonlyMap<ComposerAttachmentSummary, ComposerAttachmentSummary>) => void) {
  // The drop hook is initialized after attachment preparation, so this ref links their callbacks.
  // 拖入 Hook 在附件准备之后初始化，由此引用连接两者的回调。
  const remapSources = React.useRef<typeof install>(() => undefined);
  // Use the exact same replacement map for both the source and attachment draft.
  // 对来源和附件草稿使用同一份精确替换映射。
  const installWithSources = React.useCallback<typeof install>((replacements) => {
    remapSources.current(replacements);
    install(replacements);
  }, [install]);
  return { remapSources, installWithSources };
}

// Serialize real paths as a list so spaces, line breaks and backslashes stay unambiguous.
// 将真实路径序列化为列表，明确保留空格、换行及反斜杠。
function appendReferences(input: string, references: string[]): string {
  return references.length
    ? [input, "Referenced files:\n" + JSON.stringify([...new Set(references)], null, 2)].filter(Boolean).join("\n\n")
    : input;
}

// Return the one explicitly dropped URI or native path associated with a file.
// 返回与文件关联的明确拖入 URI 或原生路径。
function droppedReference(file: File, uri: string | null): string | null {
  return readElectronLocalFileBridge()?.pathForFile?.(file) || uri;
}

// Builds a display entry for an exact path or URI that has no browser file handle.
// 为没有浏览器文件句柄的真实路径或 URI 构建展示条目。
function referenceEntry(reference: string): DroppedFile {
  // Keep the full locator in the tooltip and show its last segment on the tile.
  // 提示中保留完整地址，附件块展示末尾片段。
  const name = reference.replace(/[/\\]+$/, "").split(/[/\\]/).pop() || reference;
  return { file: null, name, reference };
}

// Compare the owner and the rendered model route while asynchronous work is outstanding.
// 在异步操作期间比较所属草稿和界面当前显示的模型路线。
function sameDropTarget(left: DroppedFilesOptions, right: DroppedFilesOptions): boolean {
  return left.draftSlotKey === right.draftSlotKey
    && left.routeRevision === right.routeRevision
    && left.modelSelection?.providerId === right.modelSelection?.providerId
    && left.modelSelection?.connectionId === right.modelSelection?.connectionId
    && left.modelSelection?.modelId === right.modelSelection?.modelId;
}

// Admit clipboard and drop payloads for one draft; return tiles and the final prompt preparation.
// 为一个草稿准入剪贴板和拖放数据，返回附件块与最终提示词准备操作。
export function useComposerDroppedFiles(options: DroppedFilesOptions) {
  const { t } = useI18n();
  // One component may switch among several drafts without releasing their browser file handles.
  // 同一组件可在多个草稿间切换，同时保留各自的浏览器文件句柄。
  const drafts = React.useRef(new Map<string, DroppedFile[]>());
  // Publish local draft writes without making React state the asynchronous source of truth.
  // 发布本地草稿写入，避免把 React 状态用作异步操作权威。
  const [, refresh] = React.useReducer(value => value + 1, 0);
  const latest = React.useRef(options);
  latest.current = options;
  // Use the service slot unless this composer keeps an independent local draft.
  // 除非此输入区拥有独立本地草稿，否则使用服务槽位。
  const storageKey = options.storageKey ?? options.draftSlotKey;
  if (!drafts.current.has(options.draftSlotKey)) {
    // Compact drafts keep managed attachments in memory. After a page reload, restore paths for assets whose summaries went away.
    // 紧凑草稿将托管附件保留于内存。页面刷新后，摘要已消失的资产恢复为原始路径。
    const restored = readDroppedDraft(storageKey).map(item =>
      options.recheckManagedAtSend && item.assetId && !options.attachments.some(attachment => attachment.assetId === item.assetId)
        ? { ...item, assetId: undefined }
        : item);
    drafts.current.set(options.draftSlotKey, restored);
  }
  const files = drafts.current.get(options.draftSlotKey)!;
  const checking = React.useRef<string | null>(null);
  const [checkingSlot, setCheckingSlot] = React.useState<string | null>(null);
  // Keep inspection and submission mutually exclusive for this draft.
  // 当前草稿的拖入检查和提交准备互斥。
  const preparing = React.useRef(false);

  // Commits an exact owner list immediately so refresh during a pending send can recover the references.
  // 立即提交精确所有者的列表，使发送等待期间刷新也可恢复引用。
  function updateDroppedDraft(owner: string, next: DroppedFile[]): void {
    drafts.current.set(owner, next);
    refresh();
    try {
      writeDroppedDraft(storageKey, next);
    } catch (error) {
      toast.error(t("composer.error.draftPersistFailed"), {
        description: error instanceof Error ? error.message : String(error),
      });
    }
  }

  // Moves the original locator to the exact derivative that replaces its managed asset.
  // 将原始地址转移到准确替换其托管资产的派生文件。
  function remapAssets(replacements: ReadonlyMap<ComposerAttachmentSummary, ComposerAttachmentSummary>): void {
    // Only managed attachments have asset identities.
    // 只有托管附件具有资产标识。
    const ids = new Map(Array.from(replacements).flatMap(([from, to]) =>
      from.assetId && to.assetId ? [[from.assetId, to.assetId] as const] : []));
    // Skip the storage write when no tracked asset was converted.
    // 没有已跟踪资产发生转换时跳过存储写入。
    let changed = false;
    // Preserve the original path while changing the identity used at send time.
    // 更改发送时使用的身份，同时保留原始路径。
    const next = drafts.current.get(options.draftSlotKey)!.map(item => {
      // Find the exact derivative identity for this source.
      // 查找此来源对应的准确派生资产标识。
      const assetId = item.assetId && ids.get(item.assetId);
      if (!assetId) return item;
      changed = true;
      return { ...item, assetId };
    });
    if (changed) updateDroppedDraft(options.draftSlotKey, next);
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
    const uris = transferUris(transfer);
    const incoming = Array.from(transfer.files);
    if (!incoming.length && !uris.length) return;
    event.preventDefault();
    const candidates = incoming.map(file => ({
      file, name: file.name, reference: droppedReference(file, incoming.length === 1 && uris.length === 1 ? uris[0] : null),
    }));
    await admit(candidates, incoming.length ? [] : uris);
  }

  // Sends pasted files through the shared admission flow; native text uses the browser's normal edit.
  // 粘贴文件走共享准入，原生文本沿用浏览器的普通编辑行为。
  async function paste(event: React.ClipboardEvent<HTMLTextAreaElement>): Promise<void> {
    const transfer = event.clipboardData;
    const uris = transferUris(transfer);
    const direct = Array.from(transfer.files);
    const incoming = direct.length ? direct : Array.from(transfer.items)
      .flatMap(item => {
        const file = item.kind === "file" ? item.getAsFile() : null;
        return file ? [file] : [];
      });
    const readNative = readElectronClipboardFileReferences();
    if (!incoming.length && !uris.length && (transfer.getData("text/plain") || !readNative)) return;
    event.preventDefault();
    const candidates = incoming.map(file => ({
      file, name: file.name, reference: droppedReference(file, null),
    }));
    // Use the native list for paths and directories omitted by Chromium's paste event.
    // 使用原生列表取得 Chromium 粘贴事件省略的路径和目录。
    let native: Awaited<ReturnType<NonNullable<typeof readNative>>> = [];
    if (readNative) {
      try {
        native = await readNative();
      } catch {
        // The captured paste event still supplies the browser's file and URI payloads.
        // 已捕获的粘贴事件仍能提供浏览器文件及 URI。
      }
    }
    const references = incoming.length || native.length ? [] : uris;
    for (const item of native) {
      const index = candidates.findIndex(candidate => candidate.reference === item.path);
      const named = candidates.filter(candidate => !candidate.reference && candidate.name === item.name);
      const match = index >= 0 ? candidates[index]
        : named.length === 1 && native.filter(entry => entry.name === item.name).length === 1 ? named[0] : undefined;
      if (match && !item.directory) {
        match.reference = item.path;
      } else {
        references.push(item.path);
        if (match) candidates.splice(candidates.indexOf(match), 1);
      }
    }
    if (candidates.length === 1 && incoming.length === 1 && uris.length === 1) {
      candidates[0].reference ??= uris[0];
    }
    // Native-only file copies and directory copies can be represented by their original path.
    // 仅有原生引用的文件与目录复制可使用原始路径表示。
    await admit(candidates, [...new Set(references)]);
  }

  // Shows captured files immediately and stages only the positions admitted by Core.
  // 立即显示捕获的文件，仅暂存 Core 准入的位置。
  async function admit(candidates: IncomingFile[], directReferences: string[]): Promise<void> {
    const owner = options.draftSlotKey;
    if (latest.current.draftSlotKey !== owner || checking.current !== null || preparing.current) return;
    // CHM and native-only entries already have a path-only decision.
    // CHM 和仅原生条目已确定按路径处理。
    const incoming: DroppedFile[] = [...candidates.map(item => isCompiledHelp(item.name, item.file.type)
      ? { ...item, file: null } : item), ...directReferences.map(referenceEntry)];
    if (!incoming.length) return;
    // Replace at most one recovered occurrence for each new matching original.
    // 每个新原件最多替换一条匹配的已恢复记录。
    const retained = drafts.current.get(owner)!.slice();
    for (const item of incoming) {
      const replacement = retained.findIndex(entry => !entry.file && !entry.assetId && (
        entry.reference ? entry.reference === item.reference : entry.name === item.name
      ));
      if (replacement >= 0) retained.splice(replacement, 1);
    }
    // The initial tiles are local only and survive a refresh as path references.
    // 初始附件块仅在本地展示，刷新后可作为路径引用恢复。
    updateDroppedDraft(owner, [...retained, ...incoming]);
    // Metadata-only decisions replace the transient file handle without touching attachment storage.
    // 仅引用的判定会释放临时句柄，不触碰附件存储。
    const originals = incoming.flatMap(item => item.file ? [item.file] : []);
    if (incoming.some(item => !item.file && !item.reference)) toast.info(t("composer.attachment.dropPathUnavailable"));
    if (!originals.length) return;
    checking.current = owner;
    setCheckingSlot(owner);
    // Keep the original batch visible if an RPC fails so the user can remove or re-add it.
    // RPC 失败时保留原批次可见，用户可移除或重新添加。
    let next = incoming;
    try {
      if (!await options.flushRoute() || latest.current.draftSlotKey !== owner) return;
      const current = latest.current;
      // Include originals already pending so the same route budget is used across multiple drops.
      // 包含已待传的原文件，使多次拖放共用同一路线预算。
      const existing = retained.flatMap(item => item.file ? [item.file] : []);
      const { eligible, planId } = await classifyDroppedComposerFiles({
        projectId: current.projectId, conversationId: current.conversationId, draftSlotKey: owner,
        modelSelection: current.modelSelection, attachments: current.projectAttachments(current.attachments),
        maxNewAttachments: Math.max(0, MAX_COMPOSER_ATTACHMENTS - current.attachments.length - current.pendingAttachmentCount),
        files: [...existing, ...originals], forSubmission: true,
      });
      if (!sameDropTarget(latest.current, current)) return;
      // Preserve preflight indices because rejected positions still count in the RPC envelope.
      // 保留预检索引，因为被拒位置仍计入 RPC 请求体。
      const positions = new Map(originals.flatMap((file, index) =>
        eligible[index + existing.length] ? [[file, index + existing.length] as const] : []));
      next = incoming.map(item => item.file && !positions.has(item.file)
        ? { ...item, file: null, ...(inferComposerAttachmentKindFromFile(item.file) === "image" ? { previewFile: item.file } : {}) }
        : item);
      if (next.some((item, index) => !item.file && incoming[index].file && !item.reference)) {
        toast.info(t("composer.attachment.dropPathUnavailable"));
      }
      if (positions.size) {
        if (!planId) throw new Error("Core did not return an attachment staging plan.");
        // Record only actual staged assets, so a failed transfer leaves a retryable original.
        // 仅记录实际暂存的资产，传输失败时仍可重试原件。
        const stagedIds = new Map<File, string>();
        const installed = await current.stageApprovedFiles([...positions.keys()], async file => {
          // The index belongs to this exact file in the frozen preflight.
          // 索引属于冻结预检中的这个确切文件。
          const position = positions.get(file);
          if (position === undefined) throw new Error("File is missing from the attachment staging plan.");
          // The returned summary carries the identity used by managed preview and preparation.
          // 返回的摘要包含托管预览和准备使用的标识。
          const staged = await stageDroppedComposerFile(planId, position, file);
          if (staged.assetId) stagedIds.set(file, staged.assetId);
          return staged;
        });
        // A draft switch can abort installation after Core has staged the bytes.
        // 切换草稿可能在 Core 完成暂存后中止安装。
        const installedIds = new Set(installed.map(attachment => attachment.assetId));
        next = next.map(item => {
          // Discard browser bytes when a managed asset has taken ownership.
          // 托管资产接手后释放浏览器原件。
          const assetId = item.file && stagedIds.get(item.file);
          return assetId && installedIds.has(assetId)
            ? { ...item, file: current.recheckManagedAtSend ? item.file : null, assetId }
            : item;
        });
      }
    } catch (error) {
      if (latest.current.draftSlotKey === owner) {
        toast.error(t("composer.attachment.unsupported"), { description: error instanceof Error ? error.message : String(error) });
      }
    } finally {
      // Write the captured owner even when the user has moved to another draft.
      // 即使用户切换到另一草稿，也只写入最初捕获的草稿。
      // Merge only this batch so a concurrent compression result keeps its source mapping.
      // 只合并本批次，使并行压缩完成的原始来源映射得以保留。
      const updates = new Map(incoming.map((item, index) => [item, next[index]]));
      updateDroppedDraft(owner, drafts.current.get(owner)!.map(item => updates.get(item) ?? item));
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
      // A managed asset that the user removed must not reappear as a path on send.
      // 用户已移除的托管资产不能在发送时以路径重新出现。
      const tracked = new Map(files.flatMap(item => item.assetId ? [[item.assetId, item] as const] : []));
      // The compact composer has no live route planner; recheck its staged originals using metadata only.
      // 紧凑输入区没有实时路线规划器，因此仅凭元数据重新校验其已暂存原件。
      const compactOriginals = current.recheckManagedAtSend ? submittedAttachments.flatMap(attachment => {
        const item = attachment.assetId && tracked.get(attachment.assetId);
        return item && item.file ? [{ attachment, file: item.file }] : [];
      }) : [];
      // Keep the rejected identities separate so an approved asset is never staged twice.
      // 单独记录被拒身份，避免对已准入资产重复暂存。
      const compactRejected = new Set<ComposerAttachmentSummary>();
      if (compactOriginals.length) {
        const { eligible } = await classifyDroppedComposerFiles({
          projectId: current.projectId, conversationId: current.conversationId, draftSlotKey: owner,
          modelSelection: current.modelSelection, attachments: [],
          maxNewAttachments: MAX_COMPOSER_ATTACHMENTS,
          files: compactOriginals.map(item => item.file), forSubmission: false,
        });
        if (!sameDropTarget(latest.current, current)) return null;
        for (const [index, item] of compactOriginals.entries()) {
          if (!eligible[index]) compactRejected.add(item.attachment);
        }
      }
      // Keep staged media on its normal delivery path, except when the route now needs a reference.
      // 保持暂存媒体的正常交付，当前路线仅能按引用发送时除外。
      const managed = submittedAttachments.filter(attachment => {
        // Preserve existing picker/project attachments and approved managed media.
        // 保留已有选择器及项目附件，以及仍获准的托管媒体。
        const item = attachment.assetId && tracked.get(attachment.assetId);
        return !item || (attachment.deliveryPlan !== "metadata_only" && !compactRejected.has(attachment));
      });
      // Recover original paths from tracked media downgraded by the live route planner.
      // 从实时路线规划已降级的跟踪媒体恢复原始路径。
      const local = [...files.filter(item => !item.assetId), ...submittedAttachments.flatMap(attachment => {
        // Only matched managed assets can supply an original locator.
        // 只有身份匹配的托管资产可以提供原始地址。
        const item = attachment.assetId && tracked.get(attachment.assetId);
        return item && (attachment.deliveryPlan === "metadata_only" || compactRejected.has(attachment))
          ? [{ ...item, file: null }]
          : [];
      })];
      if (local.some(item => !item.file && !item.reference)) {
        toast.info(t("composer.attachment.dropSourceUnavailable"));
        return null;
      }
      const originals = local.flatMap(item => item.file ? [item.file] : []);
      const { eligible, planId } = originals.length ? await classifyDroppedComposerFiles({
        projectId: current.projectId, conversationId: current.conversationId, draftSlotKey: owner,
        modelSelection: current.modelSelection, attachments: managed,
        maxNewAttachments: Math.max(0, MAX_COMPOSER_ATTACHMENTS - managed.length),
        files: originals, forSubmission: true,
      }) : { eligible: [], planId: null };
      if (latest.current.draftSlotKey !== owner) return null;
      let decisionIndex = 0;
      const decisions = local.map((item) => Boolean(item.file && eligible[decisionIndex++]));
      if (local.some((item, index) => !decisions[index] && !item.reference)) {
        toast.info(t("composer.attachment.dropPathUnavailable"));
        return null;
      }
      const references: string[] = [];
      // Stage indices count only originals because recovered references have no wire file.
      // 暂存索引只统计原件，因为已恢复引用没有线上文件。
      let stageIndex = 0;
      for (const [index, item] of local.entries()) {
        if (!sameDropTarget(latest.current, current)) return null;
        if (item.file) {
          if (decisions[index]) {
            if (!planId) throw new Error("Core did not return a dropped-file send plan.");
            attachments.push(await stageDroppedComposerFile(planId, stageIndex, item.file));
          }
          stageIndex += 1;
        }
        if (!decisions[index] && item.reference) references.push(item.reference);
      }
      if (!sameDropTarget(latest.current, current)) return null;
      handedToSubmission = true;
      return {
        prompt: appendReferences(prompt, references),
        attachments: [...managed, ...attachments],
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

  // Managed assets already have a normal tile. Only the local entries need a separate renderer.
  // 托管资产已有常规附件块，只有本地条目需要单独渲染。
  const localFiles = files.filter(item => !item.assetId);
  // Insert local tiles into the same strip that renders the canonical attachments.
  // 将本地附件块插入渲染规范附件的同一附件栏。
  const preview = localFiles.map((item, index) => (
    <ComposerLocalAttachmentTile key={index} file={item.file ?? item.previewFile ?? null} pathOnly={!item.file}
      name={item.name} reference={item.reference}
      disabled={checkingSlot === options.draftSlotKey}
      onRemove={() => updateDroppedDraft(options.draftSlotKey, drafts.current.get(options.draftSlotKey)!.filter(file => file !== item))} />
  ));
  // Keep send preparation active for managed sources without letting removed sources count as content.
  // 为托管来源保持发送准备，同时不把已移除来源算作内容。
  const count = localFiles.length + options.attachments.filter(attachment =>
    attachment.assetId && files.some(item => item.assetId === attachment.assetId)).length;
  return { count, localCount: localFiles.length, checking: checkingSlot === options.draftSlotKey, dragOver, drop, paste, prepare, preview, remapAssets };
}
