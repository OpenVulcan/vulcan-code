// ComposerSendMode selects the delivery action while a conversation is running.
// ComposerSendMode 选择会话运行时的消息发送操作。
export type ComposerSendMode = "queue" | "insert";

// STORAGE_KEY stores the explicit Enter action for all conversations in this client.
// STORAGE_KEY 保存此客户端所有会话共用的显式 Enter 操作。
const STORAGE_KEY = "vulcan.composer.enter-send-mode.v1";

// readComposerEnterSendMode takes no arguments and returns the saved action or the queue default.
// readComposerEnterSendMode 无参数，返回已保存的操作或默认排队。
export function readComposerEnterSendMode(): ComposerSendMode {
  try {
    return window.localStorage.getItem(STORAGE_KEY) === "insert" ? "insert" : "queue";
  } catch {
    return "queue";
  }
}

// saveComposerEnterSendMode writes the explicitly selected mode and returns after storage succeeds.
// saveComposerEnterSendMode 写入用户显式选择的 mode，在存储成功后返回。
export function saveComposerEnterSendMode(mode: ComposerSendMode): void {
  window.localStorage.setItem(STORAGE_KEY, mode);
}
