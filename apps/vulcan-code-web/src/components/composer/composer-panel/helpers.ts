import type { ComposerAttachmentSummary } from "@/types";

// ComposerSessionPersistedDraftV1 is the exact browser-local draft envelope for the first desktop contract.
// ComposerSessionPersistedDraftV1 是第一版桌面契约使用的精确浏览器本地草稿信封。
interface ComposerSessionPersistedDraftV1 {
  // Exact schema version; no legacy payload is probed or migrated.
  // 精确 Schema 版本；不探测或迁移旧载荷。
  version: 1;
  // Raw user-authored input.
  // 用户输入的原始文本。
  input: string;
  // Canonical attachment facts without transient delivery decisions.
  // 不含瞬时传输决策的规范附件事实。
  attachments: ComposerAttachmentSummary[];
}

// COMPOSER_SESSION_DRAFT_STORAGE_PREFIX isolates first-generation draft payloads from unrelated preferences.
// COMPOSER_SESSION_DRAFT_STORAGE_PREFIX 将第一代草稿载荷与无关偏好设置隔离。
const COMPOSER_SESSION_DRAFT_STORAGE_PREFIX = "vulcan-code.composer-draft.v1:";
export interface ComposerSessionMemoryDraft {
  // input is the raw textarea value, including user-authored whitespace.
  // input 是原始输入框内容，保留用户输入的空白字符。
  input: string;
  // attachments are staged files that should travel with the draft input.
  // attachments 是应随草稿输入一起保留的已暂存文件。
  attachments: ComposerAttachmentSummary[];
}

// composerSessionDraftMemory keeps detached hot copies while durable browser storage survives app restarts.
// composerSessionDraftMemory 保存脱离引用的热副本，持久浏览器存储则跨应用重启保留草稿。
const composerSessionDraftMemory = new Map<string, ComposerSessionMemoryDraft>();

// isOptionalNullableString validates one optional nullable text property from untrusted browser storage.
// isOptionalNullableString 校验来自不可信浏览器存储的可选可空文本属性。
// Parameters: value is one untrusted property value.
// 参数：value 是一项不可信属性值。
// Returns: true only for undefined, null, or text.
// 返回值：仅对 undefined、null 或文本返回 true。
function isOptionalNullableString(value: unknown): value is string | null | undefined {
  return value === undefined || value === null || typeof value === "string";
}

// isOptionalNullableSafeInteger validates one optional nullable non-negative byte count.
// isOptionalNullableSafeInteger 校验一项可选可空的非负字节数。
// Parameters: value is one untrusted property value.
// 参数：value 是一项不可信属性值。
// Returns: true only for undefined, null, or a non-negative safe integer.
// 返回值：仅对 undefined、null 或非负安全整数返回 true。
function isOptionalNullableSafeInteger(value: unknown): value is number | null | undefined {
  return value === undefined
    || value === null
    || (Number.isSafeInteger(value) && (value as number) >= 0);
}

// composerSessionPersistableAttachment projects only canonical attachment facts into durable storage.
// composerSessionPersistableAttachment 只把规范附件事实投影到持久存储。
// Parameters: attachment is one service-issued staged attachment summary.
// 参数：attachment 是一条服务签发的已暂存附件摘要。
// Returns: a detached summary with request-local delivery state removed.
// 返回值：移除单次请求局部传输状态后的脱离引用摘要。
function composerSessionPersistableAttachment(
  attachment: ComposerAttachmentSummary,
): ComposerAttachmentSummary {
  return {
    kind: attachment.kind,
    fileName: attachment.fileName,
    localPath: attachment.localPath,
    projectResourceId: attachment.projectResourceId,
    resourcePath: attachment.resourcePath,
    sourcePath: attachment.sourcePath,
    assetId: attachment.assetId,
    byteLen: attachment.byteLen,
    sourceSha256: attachment.sourceSha256,
    previewLocalPath: attachment.previewLocalPath,
    mimeType: attachment.mimeType,
  };
}

// composerSessionAttachmentFromStorage validates and projects one untrusted persisted attachment.
// composerSessionAttachmentFromStorage 校验并投影一条不可信持久附件。
// Parameters: value is one JSON-decoded browser-storage element.
// 参数：value 是一项经 JSON 解码的浏览器存储元素。
// Returns: one canonical attachment summary, or null for an invalid element.
// 返回值：规范附件摘要；元素无效时返回 null。
function composerSessionAttachmentFromStorage(value: unknown): ComposerAttachmentSummary | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return null;
  }
  const candidate = value as Record<string, unknown>;
  if (
    typeof candidate.kind !== "string"
    || typeof candidate.fileName !== "string"
    || typeof candidate.localPath !== "string"
    || typeof candidate.mimeType !== "string"
    || !isOptionalNullableSafeInteger(candidate.projectResourceId)
    || !isOptionalNullableString(candidate.resourcePath)
    || !isOptionalNullableString(candidate.sourcePath)
    || !isOptionalNullableString(candidate.assetId)
    || !isOptionalNullableSafeInteger(candidate.byteLen)
    || !isOptionalNullableString(candidate.sourceSha256)
    || !isOptionalNullableString(candidate.previewLocalPath)
  ) {
    return null;
  }
  return composerSessionPersistableAttachment({
    kind: candidate.kind,
    fileName: candidate.fileName,
    localPath: candidate.localPath,
    projectResourceId: candidate.projectResourceId,
    resourcePath: candidate.resourcePath,
    sourcePath: candidate.sourcePath,
    assetId: candidate.assetId,
    byteLen: candidate.byteLen,
    sourceSha256: candidate.sourceSha256,
    previewLocalPath: candidate.previewLocalPath,
    mimeType: candidate.mimeType,
  });
}

// composerSessionDraftStorage returns the browser-local durable store without assuming a DOM runtime.
// composerSessionDraftStorage 返回浏览器本地持久存储，且不假定当前一定存在 DOM 运行时。
// Returns: localStorage when accessible, otherwise null.
// 返回值：可访问时返回 localStorage，否则返回 null。
function composerSessionDraftStorage(): Storage | null {
  if (typeof window === "undefined") {
    return null;
  }
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

// composerSessionDraftStorageKey binds one durable payload to the exact draft slot.
// composerSessionDraftStorageKey 将一份持久载荷绑定到精确草稿槽。
// Parameters: key is the canonical conversation or project-draft slot key.
// 参数：key 是规范会话或项目草稿槽键。
// Returns: the isolated localStorage key.
// 返回值：隔离后的 localStorage 键。
function composerSessionDraftStorageKey(key: string): string {
  return `${COMPOSER_SESSION_DRAFT_STORAGE_PREFIX}${key}`;
}

// readPersistedComposerSessionDraft decodes only the exact first-generation payload shape.
// readPersistedComposerSessionDraft 只解码精确的第一代载荷结构。
// Parameters: key identifies the draft slot and storage is the already-authorized browser store.
// 参数：key 标识草稿槽，storage 是已授权的浏览器存储。
// Returns: a validated draft, or null when no valid payload exists.
// 返回值：通过校验的草稿；不存在有效载荷时返回 null。
function readPersistedComposerSessionDraft(
  key: string,
  storage: Storage,
): ComposerSessionMemoryDraft | null {
  try {
    const raw = storage.getItem(composerSessionDraftStorageKey(key));
    if (!raw) {
      return null;
    }
    const decoded = JSON.parse(raw) as unknown;
    if (!decoded || typeof decoded !== "object" || Array.isArray(decoded)) {
      storage.removeItem(composerSessionDraftStorageKey(key));
      return null;
    }
    const candidate = decoded as Record<string, unknown>;
    if (candidate.version !== 1 || typeof candidate.input !== "string" || !Array.isArray(candidate.attachments)) {
      storage.removeItem(composerSessionDraftStorageKey(key));
      return null;
    }
    const attachments = candidate.attachments.map(composerSessionAttachmentFromStorage);
    if (attachments.some((attachment) => attachment === null)) {
      storage.removeItem(composerSessionDraftStorageKey(key));
      return null;
    }
    return {
      input: candidate.input,
      attachments: attachments as ComposerAttachmentSummary[],
    };
  } catch {
    try {
      storage.removeItem(composerSessionDraftStorageKey(key));
    } catch {
      // Browser storage can become unavailable between the read and cleanup attempts.
      // 浏览器存储可能在读取与清理尝试之间变为不可用。
    }
    return null;
  }
}

// persistComposerSessionDraft writes one exact versioned payload and never serializes transient transfer state.
// persistComposerSessionDraft 写入一份精确版本化载荷，且绝不序列化瞬时传输状态。
// Parameters: key identifies the draft slot, draft contains canonical facts, and storage is browser-local.
// 参数：key 标识草稿槽，draft 包含规范事实，storage 是浏览器本地存储。
// Returns: nothing; storage failures leave the in-memory draft authoritative for this process.
// 返回值：无；存储失败时，本进程仍以内存草稿为权威。
function persistComposerSessionDraft(
  key: string,
  draft: ComposerSessionMemoryDraft | null,
  storage: Storage,
): void {
  try {
    if (!draft) {
      storage.removeItem(composerSessionDraftStorageKey(key));
      return;
    }
    const payload: ComposerSessionPersistedDraftV1 = {
      version: 1,
      input: draft.input,
      attachments: draft.attachments.map(composerSessionPersistableAttachment),
    };
    storage.setItem(composerSessionDraftStorageKey(key), JSON.stringify(payload));
  } catch {
    // The current-process memory copy still retains the exact draft when quota or policy rejects persistence.
    // 当配额或策略拒绝持久化时，当前进程内存副本仍保留精确草稿。
  }
}

// composerShortcutHint returns a platform-specific keyboard hint for the composer placeholder.
// composerShortcutHint 返回输入区 placeholder 使用的平台化快捷键提示。
export function composerShortcutHint(baseHint: string): string {
  const platformText = typeof navigator === "undefined"
    ? ""
    : `${navigator.platform} ${navigator.userAgent}`.toLowerCase();
  if (platformText.includes("mac")) {
    return baseHint;
  }
  return baseHint;
}

// composerSessionDraftKey returns the stable slot for one selected or draft conversation.
// composerSessionDraftKey 返回当前选中会话或新建会话对应的稳定草稿槽。
export function composerSessionDraftKey(projectId: string | null, conversationId: string | null): string {
  if (conversationId) {
    return `conversation:${conversationId}`;
  }
  return `new:${projectId?.trim() || "workspace"}`;
}

// cloneComposerSessionMemoryDraft returns a detached draft copy for safe React state use.
// cloneComposerSessionMemoryDraft 返回脱离原对象的草稿副本，供 React 状态安全使用。
function cloneComposerSessionMemoryDraft(draft: ComposerSessionMemoryDraft): ComposerSessionMemoryDraft {
  return {
    input: draft.input,
    attachments: draft.attachments.map((attachment) => ({ ...attachment })),
  };
}

// emptyComposerSessionMemoryDraft creates an empty draft object.
// emptyComposerSessionMemoryDraft 创建空输入区草稿对象。
function emptyComposerSessionMemoryDraft(): ComposerSessionMemoryDraft {
  return {
    input: "",
    attachments: [],
  };
}

// readComposerSessionMemoryDraft reads one hot or durable draft slot without sharing mutable arrays.
// readComposerSessionMemoryDraft 读取单个热缓存或持久草稿槽，并避免共享可变数组。
export function readComposerSessionMemoryDraft(key: string): ComposerSessionMemoryDraft {
  const hotDraft = composerSessionDraftMemory.get(key);
  if (hotDraft) {
    return cloneComposerSessionMemoryDraft(hotDraft);
  }
  const storage = composerSessionDraftStorage();
  const persistedDraft = storage ? readPersistedComposerSessionDraft(key, storage) : null;
  if (!persistedDraft) {
    return emptyComposerSessionMemoryDraft();
  }
  composerSessionDraftMemory.set(key, cloneComposerSessionMemoryDraft(persistedDraft));
  return cloneComposerSessionMemoryDraft(persistedDraft);
}

// writeComposerSessionMemoryDraft saves or clears one hot and durable draft slot.
// writeComposerSessionMemoryDraft 保存或清理单个热缓存与持久草稿槽。
export function writeComposerSessionMemoryDraft(
  key: string,
  input: string,
  attachments: ComposerAttachmentSummary[],
): void {
  const storage = composerSessionDraftStorage();
  if (!input && attachments.length === 0) {
    composerSessionDraftMemory.delete(key);
    if (storage) {
      persistComposerSessionDraft(key, null, storage);
    }
    return;
  }
  const draft: ComposerSessionMemoryDraft = {
    input,
    attachments: attachments.map(composerSessionPersistableAttachment),
  };
  composerSessionDraftMemory.set(key, cloneComposerSessionMemoryDraft(draft));
  if (storage) {
    persistComposerSessionDraft(key, draft, storage);
  }
}

