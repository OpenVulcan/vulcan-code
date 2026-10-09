// The GUI-local choice only controls compression of newly attached composer images.
// 此 GUI 本地选择只控制新加入输入区图片的压缩。
const COMPOSER_IMAGE_COMPRESSION_KEY = "vulcan.gui.composer-compress-images.v1";

// readComposerImageCompressionPreference reads the explicit viewer choice, defaulting to enabled without writing.
// readComposerImageCompressionPreference 读取当前查看者的明确选择；未设置时默认开启，不回写存储。
// Takes no arguments; returns whether new images are compressed before planning.
// 无参数；返回新图片是否在规划前压缩。
export function readComposerImageCompressionPreference(): boolean {
  try {
    return window.localStorage.getItem(COMPOSER_IMAGE_COMPRESSION_KEY) !== "false";
  } catch {
    // An unavailable local store keeps the in-memory product default.
    // 本地存储不可用时使用产品内存默认值。
    return true;
  }
}

// saveComposerImageCompressionPreference remembers an explicit viewer choice for later drafts.
// saveComposerImageCompressionPreference 记住查看者的明确选择，供之后的草稿使用。
// enabled is the new choice; a failed write keeps the choice for the current page only.
// enabled 是新选择；写入失败时该选择只在当前页面有效。
export function saveComposerImageCompressionPreference(enabled: boolean): void {
  try {
    window.localStorage.setItem(COMPOSER_IMAGE_COMPRESSION_KEY, String(enabled));
  } catch {
    // The live React state still carries the choice; persistence is only a convenience.
    // React 实时状态仍保留该选择；持久化只是便利功能。
  }
}

// composerAttachmentOverflowCount counts attachments that exceed the single-request encoded budget.
// composerAttachmentOverflowCount 统计超出单次请求编码预算的附件数量。
// preparations are the current draft plans; returns the number reported with the planner's budget reason.
// preparations 是当前草稿规划；返回带有规划器预算原因的数量。
export function composerAttachmentOverflowCount(
  preparations: Readonly<Record<string, { reason: string }>>,
): number {
  return Object.values(preparations).filter((preparation) => preparation.reason === "encoded_budget").length;
}
