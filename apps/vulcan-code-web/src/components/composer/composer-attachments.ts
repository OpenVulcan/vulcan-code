import React from "react";
import { toast } from "@/ui-kit/primitives/toast";
import {
  composerAttachmentAccept,
  MAX_COMPOSER_ATTACHMENTS,
} from "@/components/composer/composer-model";
import {
  createComposerImagePreviewFile,
  inferComposerAttachmentKindFromFile,
} from "@/components/composer/composer-attachment-media";
import { useI18n, type TranslationKey } from "@/i18n";
import { stageComposerAttachmentPreviewBytes } from "@/service/conversation-actions-service";
import type {
  ComposerAttachmentCapabilitySummary,
  ComposerAttachmentSummary,
} from "@/types";

// UseComposerAttachmentsArgs describes the runtime dependencies needed by composer attachment staging.
// UseComposerAttachmentsArgs 描述输入区附件暂存所需的运行时依赖。
export interface UseComposerAttachmentsArgs {
  // initialAttachments seeds staged files when restoring an in-memory composer draft.
  // initialAttachments 在恢复内存输入区草稿时初始化已暂存文件。
  initialAttachments?: ComposerAttachmentSummary[];
  // attachmentCapabilities contains backend-advertised attachment gates.
  // attachmentCapabilities 包含后端公布的附件能力门控。
  attachmentCapabilities: ComposerAttachmentCapabilitySummary[];
  // projectId scopes staged attachment files to the active project.
  // projectId 将暂存附件文件限定到当前项目。
  projectId: string | null;
  // conversationId scopes staged attachment files to the active conversation.
  // conversationId 将暂存附件文件限定到当前会话。
  conversationId: string | null;
  // onStageAttachmentBytes persists one browser file through the service boundary.
  // onStageAttachmentBytes 通过服务边界持久化单个浏览器文件。
  onStageAttachmentBytes: (
    projectId: string | null,
    conversationId: string | null,
    file: File,
  ) => Promise<ComposerAttachmentSummary | null>;
}

// UseComposerAttachmentsState exposes composer attachment state and event handlers.
// UseComposerAttachmentsState 暴露输入区附件状态与事件处理函数。
export interface UseComposerAttachmentsState {
  // attachments stores staged attachments submitted with the next prompt.
  // attachments 保存下一条提示词将携带的已暂存附件。
  attachments: ComposerAttachmentSummary[];
  // pendingAttachmentCount counts accepted files still being staged for this exact draft.
  // pendingAttachmentCount 统计当前精确草稿中仍在暂存的已接受文件。
  pendingAttachmentCount: number;
  // hasPendingAttachmentStaging reads the synchronous barrier before any send can clear the draft.
  // hasPendingAttachmentStaging 在任何发送清空草稿前读取同步屏障。
  hasPendingAttachmentStaging: () => boolean;
  // setAttachments replaces staged attachments for send failure restore and pending prompt edit.
  // setAttachments 替换已暂存附件，用于发送失败恢复与待发送提示词编辑。
  setAttachments: React.Dispatch<React.SetStateAction<ComposerAttachmentSummary[]>>;
  // isAttachmentDragOver indicates whether the composer should show drag highlighting.
  // isAttachmentDragOver 表示输入区是否应显示拖拽高亮。
  isAttachmentDragOver: boolean;
  // setIsAttachmentDragOver updates drag highlighting from panel-level leave events.
  // setIsAttachmentDragOver 允许面板级离开事件更新拖拽高亮。
  setIsAttachmentDragOver: React.Dispatch<React.SetStateAction<boolean>>;
  // attachmentPickerKind stores the current hidden file input filter.
  // attachmentPickerKind 保存当前隐藏文件输入框过滤类型。
  attachmentPickerKind: string | null;
  // attachmentFileInputRef opens the browser file picker for manual attachment selection.
  // attachmentFileInputRef 打开浏览器文件选择器用于手动选择附件。
  attachmentFileInputRef: React.RefObject<HTMLInputElement | null>;
  // enabledAttachmentCapabilities lists currently accepted attachment capability gates.
  // enabledAttachmentCapabilities 列出当前接受的附件能力门控。
  enabledAttachmentCapabilities: ComposerAttachmentCapabilitySummary[];
  // attachmentInputAccept stores the hidden input accept attribute.
  // attachmentInputAccept 保存隐藏输入框的 accept 属性。
  attachmentInputAccept: string;
  // openAttachmentPicker opens the browser file picker with a kind-specific accept filter.
  // openAttachmentPicker 使用类型特定 accept 过滤器打开浏览器文件选择器。
  openAttachmentPicker: (kind: string | null) => void;
  // handleAttachmentInputChange stages files selected through the hidden input.
  // handleAttachmentInputChange 暂存通过隐藏输入框选择的文件。
  handleAttachmentInputChange: (event: React.ChangeEvent<HTMLInputElement>) => void;
  // handleComposerPaste gives file and image clipboard payloads priority over plain text.
  // handleComposerPaste 让文件与图片剪贴板载荷优先于普通文本处理。
  handleComposerPaste: (event: React.ClipboardEvent<HTMLTextAreaElement>) => void;
  // handleComposerDragOver highlights the composer only when dragged files are acceptable.
  // handleComposerDragOver 仅在拖入文件可接受时高亮输入区。
  handleComposerDragOver: (event: React.DragEvent<HTMLElement>) => void;
  // handleComposerDrop stages dropped files and clears drag highlighting.
  // handleComposerDrop 暂存拖入文件并清理拖拽高亮。
  handleComposerDrop: (event: React.DragEvent<HTMLElement>) => void;
  // removeComposerAttachment removes only the captured object, never a later occupant of its old index.
  // removeComposerAttachment 只移除捕获的对象，绝不移除后来占用旧索引的附件。
  removeComposerAttachment: (attachment: ComposerAttachmentSummary) => void;
  // invalidatePendingAttachmentStaging prevents an older async file selection from entering a replacement draft.
  // invalidatePendingAttachmentStaging 阻止较早的异步文件选择进入替换后的草稿。
  invalidatePendingAttachmentStaging: () => void;
}

// useComposerAttachments owns attachment validation, staging, drag-drop, and paste behavior.
// useComposerAttachments 管理附件校验、暂存、拖拽投放与粘贴行为。
// Parameters: args supplies the current draft owner, staging services and capability limits; returns attachment state, pending counts and synchronous submission guards.
// 参数：args 提供当前草稿归属、暂存服务与能力限制；返回附件状态、在途计数与同步发送屏障。
export function useComposerAttachments(args: UseComposerAttachmentsArgs): UseComposerAttachmentsState {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  // attachments stores staged composer attachments submitted with the next prompt.
  // attachments 保存下一条提示词将携带的已暂存输入区附件。
  const [attachments, commitAttachments] = React.useState<ComposerAttachmentSummary[]>(() => args.initialAttachments ?? []);
  // attachmentsRef reserves accurate slots across simultaneous staging batches before the next render.
  // attachmentsRef 在下一次渲染前为同时暂存的批次保留准确名额。
  const attachmentsRef = React.useRef(attachments);
  const setAttachments = React.useCallback<React.Dispatch<React.SetStateAction<ComposerAttachmentSummary[]>>>((update) => {
    const next = typeof update === "function" ? update(attachmentsRef.current) : update;
    attachmentsRef.current = next;
    commitAttachments(next);
  }, []);
  // pendingStagingRef is the immediate send barrier; state supplies its visible projection.
  // pendingStagingRef 是即时发送屏障；state 提供其可见投影。
  const pendingStagingRef = React.useRef(0);
  const [pendingAttachmentCount, setPendingAttachmentCount] = React.useState(0);
  const hasPendingAttachmentStaging = React.useCallback(() => pendingStagingRef.current > 0, []);
  // isAttachmentDragOver highlights the composer when valid files are dragged over it.
  // isAttachmentDragOver 在有效文件拖入输入区时高亮边框。
  const [isAttachmentDragOver, setIsAttachmentDragOver] = React.useState(false);
  // attachmentPickerKind stores the current hidden file input filter.
  // attachmentPickerKind 保存当前隐藏文件输入框过滤类型。
  const [attachmentPickerKind, setAttachmentPickerKind] = React.useState<string | null>(null);
  // attachmentFileInputRef opens the browser file picker for manual attachment selection.
  // attachmentFileInputRef 打开浏览器文件选择器用于手动选择附件。
  const attachmentFileInputRef = React.useRef<HTMLInputElement | null>(null);
  // stagingGenerationRef invalidates async staging work that belongs to an earlier complete draft.
  // stagingGenerationRef 使属于较早完整草稿的异步暂存工作失效。
  const stagingGenerationRef = React.useRef(0);
  // enabledAttachmentCapabilities resolves host attachment categories; direct delivery remains route-specific.
  // enabledAttachmentCapabilities 解析宿主附件分类；直接传输仍由具体路线决定。
  const enabledAttachmentCapabilities = React.useMemo(
    () => args.attachmentCapabilities.filter((capability) => capability.enabled),
    [args.attachmentCapabilities],
  );
  // enabledAttachmentKinds speeds up local drop and paste validation.
  // enabledAttachmentKinds 加速本地拖拽与粘贴校验。
  const enabledAttachmentKinds = React.useMemo(
    () => new Set(enabledAttachmentCapabilities.map((capability) => capability.kind)),
    [enabledAttachmentCapabilities],
  );
  // attachmentInputAccept stores the current hidden input accept filter.
  // attachmentInputAccept 保存当前隐藏输入框的过滤规则。
  const attachmentInputAccept = attachmentPickerKind
    ? enabledAttachmentCapabilities.find((capability) => capability.kind === attachmentPickerKind)?.accept ?? ""
    : composerAttachmentAccept(enabledAttachmentCapabilities);

  // stageComposerFiles validates and stages files selected, dropped, or pasted into the composer.
  // stageComposerFiles 校验并暂存通过选择、拖拽或粘贴进入输入区的文件。
  const stageComposerFiles = React.useCallback(async (files: File[], preferredKind?: string | null) => {
    const stagingGeneration = stagingGenerationRef.current;
    const candidates = files.filter((file) => {
      const kind = inferComposerAttachmentKindFromFile(file);
      return Boolean(kind && (!preferredKind || kind === preferredKind) && enabledAttachmentKinds.has(kind));
    });
    if (!candidates.length) {
      toast.info(t("composer.attachment.unsupported"));
      return;
    }
    const availableSlots = MAX_COMPOSER_ATTACHMENTS - attachmentsRef.current.length - pendingStagingRef.current;
    if (availableSlots <= 0) {
      toast.info(formatComposerAttachmentTranslation(t, "composer.attachment.maxLimit", { count: String(MAX_COMPOSER_ATTACHMENTS) }));
      return;
    }
    const acceptedFiles = candidates.slice(0, availableSlots);
    if (candidates.length > acceptedFiles.length) {
      toast.info(formatComposerAttachmentTranslation(t, "composer.attachment.partialMaxLimit", { count: String(MAX_COMPOSER_ATTACHMENTS) }));
    }
    const staged: ComposerAttachmentSummary[] = [];
    pendingStagingRef.current += acceptedFiles.length;
    setPendingAttachmentCount(pendingStagingRef.current);
    try {
    for (const file of acceptedFiles) {
      if (stagingGeneration !== stagingGenerationRef.current) {
        return;
      }
      try {
        let attachment = await args.onStageAttachmentBytes(args.projectId, args.conversationId, file);
        if (stagingGeneration !== stagingGenerationRef.current) {
          return;
        }
        if (attachment) {
          const previewFile = await createComposerImagePreviewFile(file);
          if (stagingGeneration !== stagingGenerationRef.current) return;
          if (previewFile && attachment.assetId) {
            try {
              // previewLocalPath points at a sidecar of the canonical asset and never creates a second attachment index record.
              // previewLocalPath 指向规范资产的旁路文件，绝不会创建第二条附件索引记录。
              const previewLocalPath = await stageComposerAttachmentPreviewBytes(
                args.projectId,
                args.conversationId,
                attachment.assetId,
                previewFile,
              );
              attachment = { ...attachment, previewLocalPath };
            } catch (error) {
              if (stagingGeneration === stagingGenerationRef.current) toast.error(error instanceof Error ? error.message : String(error));
            }
          }
          staged.push(attachment);
        }
      } catch (error) {
        if (stagingGeneration === stagingGenerationRef.current) toast.error(error instanceof Error ? error.message : String(error));
      }
    }
    if (!staged.length) {
      return;
    }
    if (stagingGeneration !== stagingGenerationRef.current) {
      return;
    }
    setAttachments((current) => {
      const seen = new Set(current.map((attachment) => `${attachment.localPath}\n${attachment.fileName}`));
      const unique = staged.filter((attachment) => {
        const key = `${attachment.localPath}\n${attachment.fileName}`;
        if (seen.has(key)) {
          return false;
        }
        seen.add(key);
        return true;
      });
      return [...current, ...unique].slice(0, MAX_COMPOSER_ATTACHMENTS);
    });
    } finally {
      if (stagingGeneration === stagingGenerationRef.current) {
        pendingStagingRef.current -= acceptedFiles.length;
        setPendingAttachmentCount(pendingStagingRef.current);
      }
    }
  }, [
    args.conversationId,
    args.onStageAttachmentBytes,
    args.projectId,
    setAttachments,
    enabledAttachmentKinds,
    t,
  ]);

  // openAttachmentPicker opens the browser file picker with a kind-specific accept filter.
  // openAttachmentPicker 使用类型特定 accept 过滤器打开浏览器文件选择器。
  const openAttachmentPicker = React.useCallback((kind: string | null) => {
    setAttachmentPickerKind(kind);
    window.requestAnimationFrame(() => attachmentFileInputRef.current?.click());
  }, []);

  // handleAttachmentInputChange stages files selected through the hidden input.
  // handleAttachmentInputChange 暂存通过隐藏输入框选择的文件。
  const handleAttachmentInputChange = React.useCallback((event: React.ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(event.currentTarget.files ?? []);
    event.currentTarget.value = "";
    if (files.length) {
      void stageComposerFiles(files, attachmentPickerKind);
    }
  }, [attachmentPickerKind, stageComposerFiles]);

  // handleComposerPaste gives file and image clipboard payloads priority over plain text.
  // handleComposerPaste 让文件与图片剪贴板载荷优先于普通文本处理。
  const handleComposerPaste = React.useCallback((event: React.ClipboardEvent<HTMLTextAreaElement>) => {
    const fileList = Array.from(event.clipboardData.files ?? []);
    const itemFiles = Array.from(event.clipboardData.items ?? [])
      .map((item) => item.kind === "file" ? item.getAsFile() : null)
      .filter((file): file is File => Boolean(file));
    const files = fileList.length ? fileList : itemFiles;
    if (!files.length) {
      return;
    }
    const hasSupportedFile = files.some((file) => {
      const kind = inferComposerAttachmentKindFromFile(file);
      return Boolean(kind && enabledAttachmentKinds.has(kind));
    });
    if (!hasSupportedFile) {
      return;
    }
    event.preventDefault();
    void stageComposerFiles(files);
  }, [enabledAttachmentKinds, stageComposerFiles]);

  // handleComposerDragOver highlights the composer only when dragged files are acceptable.
  // handleComposerDragOver 仅在拖入文件可接受时高亮输入区。
  const handleComposerDragOver = React.useCallback((event: React.DragEvent<HTMLElement>) => {
    const hasFiles = Array.from(event.dataTransfer.types).includes("Files");
    if (!hasFiles || enabledAttachmentCapabilities.length === 0) {
      return;
    }
    event.preventDefault();
    event.dataTransfer.dropEffect = "copy";
    setIsAttachmentDragOver(true);
  }, [enabledAttachmentCapabilities.length]);

  // handleComposerDrop stages dropped files and clears drag highlighting.
  // handleComposerDrop 暂存拖入文件并清理拖拽高亮。
  const handleComposerDrop = React.useCallback((event: React.DragEvent<HTMLElement>) => {
    const files = Array.from(event.dataTransfer.files ?? []);
    if (!files.length) {
      setIsAttachmentDragOver(false);
      return;
    }
    event.preventDefault();
    setIsAttachmentDragOver(false);
    void stageComposerFiles(files);
  }, [stageComposerFiles]);

  // removeComposerAttachment accepts the captured object and preserves files added while cancellation waited.
  // removeComposerAttachment 接收捕获对象，保留取消等待期间新添加的文件。
  // Returns nothing; a replaced or already removed object leaves the current draft unchanged.
  // 无返回值；对象已被替换或移除时保持当前草稿不变。
  const removeComposerAttachment = React.useCallback((attachment: ComposerAttachmentSummary) => {
    setAttachments((current) => current.filter((candidate) => candidate !== attachment));
  }, []);

  // invalidatePendingAttachmentStaging advances the draft generation without cancelling canonical asset persistence.
  // invalidatePendingAttachmentStaging 推进草稿代际，但不取消规范资产的持久化。
  const invalidatePendingAttachmentStaging = React.useCallback(() => {
    stagingGenerationRef.current += 1;
    pendingStagingRef.current = 0;
    setPendingAttachmentCount(0);
    setIsAttachmentDragOver(false);
    if (attachmentFileInputRef.current) {
      attachmentFileInputRef.current.value = "";
    }
  }, []);
  React.useLayoutEffect(() => {
    invalidatePendingAttachmentStaging();
    return () => { stagingGenerationRef.current += 1; pendingStagingRef.current = 0; };
  }, [args.projectId, args.conversationId, invalidatePendingAttachmentStaging]);

  return {
    attachments,
    pendingAttachmentCount,
    hasPendingAttachmentStaging,
    setAttachments,
    isAttachmentDragOver,
    setIsAttachmentDragOver,
    attachmentPickerKind,
    attachmentFileInputRef,
    enabledAttachmentCapabilities,
    attachmentInputAccept,
    openAttachmentPicker,
    handleAttachmentInputChange,
    handleComposerPaste,
    handleComposerDragOver,
    handleComposerDrop,
    removeComposerAttachment,
    invalidatePendingAttachmentStaging,
  };
}

// formatComposerAttachmentTranslation replaces named placeholders in localized attachment messages.
// formatComposerAttachmentTranslation 替换附件本地化消息中的具名占位符。
function formatComposerAttachmentTranslation(
  t: (key: TranslationKey) => string,
  key: TranslationKey,
  values: Record<string, string>,
): string {
  return Object.entries(values).reduce(
    (text, [name, value]) => text.replaceAll(`{${name}}`, value),
    t(key),
  );
}
